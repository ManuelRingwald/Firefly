//! CAT062 framing and data items.
//!
//! A [`Cat062Encoder`] turns the tracks of one scan into a single CAT062 *data
//! block*: a `[CAT][LEN][record…]` envelope (see [`data_block`]) around one
//! [`record`](Cat062Encoder::record) per track. Each record carries the data
//! source (I062/010), time of track (I062/070), WGS-84 position (I062/105),
//! the system-stereographic position (I062/100, ADR 0006), Cartesian velocity
//! (I062/185), track number (I062/040) and the safety-relevant status:
//! confirmation/coasting (I062/080), update age (I062/290) and position
//! accuracy (I062/500) — the same status the tracker decides (ADR 0008).
//!
//! I062/100 is computed by projecting the track's WGS-84 position onto the
//! system-stereographic plane (see [`firefly_geo::StereographicProjection`]),
//! tangent at the system reference point given to
//! [`Cat062Encoder::new`] — carried *additionally* to I062/105, not instead of
//! it (ADR 0006-Nachtrag).
//!
//! When a track carries an SSR identity (FR-TRK-009) the record additionally
//! carries the Mode 3/A code (I062/060) and the Mode S 24-bit address as the
//! Target Address subfield of Aircraft Derived Data (I062/380); both items are
//! omitted for a primary-only track.
//!
//! REQ: FR-IO-003, FR-TRK-008, FR-TRK-009, FR-GEO-004

use firefly_core::{SystemTrack, Timestamp};
use firefly_geo::{StereographicProjection, Wgs84};

use crate::fspec::RecordBuilder;

/// The ASTERIX category number for system tracks.
const CATEGORY: u8 = 62;

/// The CAT062 UAP slots (field reference numbers) we encode so far. Keeping the
/// FRNs named and in one place documents the bit layout and stops magic numbers
/// from drifting between the FSPEC and the payload order.
mod uap {
    /// I062/010 — Data Source Identifier (SAC/SIC).
    pub const DATA_SOURCE_IDENTIFIER: u8 = 1;
    /// I062/070 — Time of Track Information.
    pub const TIME_OF_TRACK_INFORMATION: u8 = 4;
    /// I062/105 — Calculated Track Position (WGS-84).
    pub const POSITION_WGS84: u8 = 5;
    /// I062/100 — Calculated Track Position (Cartesian, system-stereographic).
    pub const POSITION_CARTESIAN: u8 = 6;
    /// I062/185 — Calculated Track Velocity (Cartesian).
    pub const VELOCITY_CARTESIAN: u8 = 7;
    /// I062/060 — Track Mode 3/A Code.
    pub const MODE_3A_CODE: u8 = 9;
    /// I062/380 — Aircraft Derived Data (here: the Target Address subfield).
    pub const AIRCRAFT_DERIVED_DATA: u8 = 11;
    /// I062/040 — Track Number.
    pub const TRACK_NUMBER: u8 = 12;
    /// I062/080 — Track Status.
    pub const TRACK_STATUS: u8 = 13;
    /// I062/290 — System Track Update Ages.
    pub const UPDATE_AGES: u8 = 14;
    /// I062/500 — Estimated Accuracies.
    pub const ESTIMATED_ACCURACIES: u8 = 16;
}

/// I062/070 is counted in units of 1/128 second since midnight.
const TIME_LSB_SECONDS: f64 = 1.0 / 128.0;
/// Time of day wraps every 24 hours; we fold the timestamp into one day so the
/// 24-bit field never overflows.
const SECONDS_PER_DAY: f64 = 86_400.0;
/// I062/105 stores latitude and longitude as signed counts of 180/2²⁵ degrees.
const POSITION_LSB_DEGREES: f64 = 180.0 / (1u32 << 25) as f64;
/// I062/100 stores X and Y as 24-bit signed counts of 0.5 m. Verified against
/// SUR.ET1.ST05.2000-STD-09-01: range ±2²³ · 0.5 m = ±4,194,304 m.
const POSITION_CARTESIAN_LSB_METRES: f64 = 0.5;
/// Smallest/largest value a 24-bit two's-complement field can hold.
const I24_MIN: i32 = -(1 << 23);
const I24_MAX: i32 = (1 << 23) - 1;
/// I062/185 stores each velocity component as a signed count of 0.25 m/s.
const VELOCITY_LSB_MPS: f64 = 0.25;
/// I062/290 stores each update age as one octet of 1/4-second steps.
const AGE_LSB_SECONDS: f64 = 0.25;
/// I062/500 APC stores each position-accuracy component in 0.5-metre steps.
const ACCURACY_LSB_METRES: f64 = 0.5;
/// I062/060 carries the Mode 3/A reply in its low 12 bits (4 octal digits); the
/// upper bits (V/G/CH/spare) stay zero — validated, not garbled, unchanged.
const MODE_3A_CODE_MASK: u16 = 0x0FFF;
/// I062/380 primary subfield, bit 8 (spec "ADR"): the 24-bit Target Address
/// (Mode S address) subfield is present. Verified against
/// SUR.ET1.ST05.2000-STD-09-01 Ed. 1.10 §5.2.24.
const ADR_TARGET_ADDRESS_PRESENT: u8 = 0x80;

/// The field-extension bit (lowest bit of an octet): "another octet follows".
/// Used both by the FSPEC and, here, *inside* the variable-length I062/080.
const FX: u8 = 0x01;
/// I062/080 octet 1, bit 2 (CNF): set means the track is still *tentative*.
const STATUS_CNF_TENTATIVE: u8 = 0x02;
/// I062/080 octet 4, bit 8 (CST): set means the track is *coasting*.
const STATUS_CST_COASTING: u8 = 0x80;
/// I062/290 primary subfield, bit 7 (spec bit-15, "PSR"): the PSR-age subfield
/// is present. Verified against SUR.ET1.ST05.2000-STD-09-01 Ed. 1.10 §5.2.20.
const AGE_PSR_PRESENT: u8 = 0x40;
/// I062/500 primary subfield, bit 8 (spec bit-16, "APC"): the Cartesian
/// position-accuracy subfield is present. Verified against
/// SUR.ET1.ST05.2000-STD-09-01 Ed. 1.10 §5.2.26.
const ACCURACY_APC_PRESENT: u8 = 0x80;

/// The originator of the tracks, as it appears in I062/010.
///
/// `sac` (System Area Code) and `sic` (System Identification Code) together name
/// *which system* produced this data — here, our tracker. They are configuration,
/// not something the tracker computes, so the encoder is told them once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataSourceId {
    /// System Area Code.
    pub sac: u8,
    /// System Identification Code.
    pub sic: u8,
}

impl DataSourceId {
    /// A data source identifier from its two octets.
    pub fn new(sac: u8, sic: u8) -> Self {
        Self { sac, sic }
    }
}

/// Encodes neutral system tracks into CAT062 data blocks.
///
/// Holds the fixed [`DataSourceId`] and the [`StereographicProjection`] used to
/// compute I062/100 (system-stereographic X/Y, ADR 0006); everything else comes
/// from the tracks and the scan time passed to [`encode`](Cat062Encoder::encode).
#[derive(Debug, Clone, Copy)]
pub struct Cat062Encoder {
    source: DataSourceId,
    system_projection: StereographicProjection,
}

impl Cat062Encoder {
    /// An encoder that stamps every block with `source` as the data source and
    /// projects positions onto the system-stereographic plane tangent at
    /// `system_reference_point` (the system reference point, e.g. the tracking
    /// frame's origin) for I062/100.
    pub fn new(source: DataSourceId, system_reference_point: Wgs84) -> Self {
        Self {
            source,
            system_projection: StereographicProjection::new(system_reference_point),
        }
    }

    /// Encode all `tracks` of one scan (taken at data-time `time`) into a single
    /// CAT062 data block. With no tracks the block is a valid, empty envelope.
    pub fn encode(&self, time: Timestamp, tracks: &[SystemTrack]) -> Vec<u8> {
        let records: Vec<Vec<u8>> = tracks.iter().map(|t| self.record(time, t)).collect();
        data_block(&records)
    }

    /// One CAT062 record for one track: FSPEC + the present items in UAP order.
    ///
    /// The identity items (I062/060, I062/380) are only added when the track
    /// actually carries that identity; the [`RecordBuilder`] then reflects their
    /// presence in the FSPEC automatically.
    fn record(&self, time: Timestamp, track: &SystemTrack) -> Vec<u8> {
        let mut builder = RecordBuilder::new()
            .item(
                uap::DATA_SOURCE_IDENTIFIER,
                encode_data_source(&self.source),
            )
            .item(uap::TIME_OF_TRACK_INFORMATION, encode_time_of_track(time))
            .item(uap::POSITION_WGS84, encode_position(track))
            .item(
                uap::POSITION_CARTESIAN,
                encode_position_cartesian(track, &self.system_projection),
            )
            .item(uap::VELOCITY_CARTESIAN, encode_velocity(track))
            .item(uap::TRACK_NUMBER, encode_track_number(track))
            .item(uap::TRACK_STATUS, encode_track_status(track))
            .item(uap::UPDATE_AGES, encode_update_ages(track))
            .item(uap::ESTIMATED_ACCURACIES, encode_accuracies(track));

        if let Some(code) = track.mode_3a {
            builder = builder.item(uap::MODE_3A_CODE, encode_mode_3a(code));
        }
        if let Some(address) = track.icao_address {
            builder = builder.item(uap::AIRCRAFT_DERIVED_DATA, encode_target_address(address));
        }

        builder.finish()
    }
}

/// I062/010 — two octets, `[SAC, SIC]`, verbatim.
fn encode_data_source(source: &DataSourceId) -> Vec<u8> {
    vec![source.sac, source.sic]
}

/// I062/070 — time of track as a 24-bit count of 1/128-second ticks since
/// midnight, big-endian.
fn encode_time_of_track(time: Timestamp) -> Vec<u8> {
    let tod = time.as_secs().rem_euclid(SECONDS_PER_DAY);
    let ticks = (tod / TIME_LSB_SECONDS).round() as u32; // ≤ 11_059_200 < 2^24
    let bytes = ticks.to_be_bytes();
    bytes[1..4].to_vec() // low three octets; the top octet is always zero here
}

/// I062/105 — position in WGS-84: latitude then longitude, each a 32-bit signed
/// count of 180/2²⁵-degree steps, big-endian. The sign is plain two's complement
/// (`i32`), so southern/western coordinates need no special handling.
fn encode_position(track: &SystemTrack) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(&encode_wgs84_angle(track.position.lat_deg()));
    out.extend_from_slice(&encode_wgs84_angle(track.position.lon_deg()));
    out
}

/// One WGS-84 angle (degrees) as a signed 32-bit count of position LSBs.
fn encode_wgs84_angle(degrees: f64) -> [u8; 4] {
    let ticks = (degrees / POSITION_LSB_DEGREES).round() as i32;
    ticks.to_be_bytes()
}

/// I062/100 — position in the system-stereographic plane: X then Y, each a
/// 24-bit signed count of 0.5-metre steps, big-endian. The track's WGS-84
/// position is projected with `projection` (tangent at the system reference
/// point, ADR 0006); carried *additionally* to I062/105, not instead of it.
fn encode_position_cartesian(track: &SystemTrack, projection: &StereographicProjection) -> Vec<u8> {
    let (x, y) = projection.project(track.position);
    let mut out = Vec::with_capacity(6);
    out.extend_from_slice(&encode_cartesian_component(x));
    out.extend_from_slice(&encode_cartesian_component(y));
    out
}

/// One system-stereographic component (metres) as a signed 24-bit count of
/// 0.5-metre steps, clamped to the field's range (±2²³ · 0.5 m ≈ ±4,194 km).
fn encode_cartesian_component(metres: f64) -> [u8; 3] {
    let ticks = (metres / POSITION_CARTESIAN_LSB_METRES)
        .round()
        .clamp(I24_MIN as f64, I24_MAX as f64) as i32;
    let bytes = ticks.to_be_bytes();
    [bytes[1], bytes[2], bytes[3]]
}

/// I062/185 — velocity in the system Cartesian frame: Vx then Vy, each a 16-bit
/// signed count of 0.25 m/s steps, big-endian. The tracker's local ENU east/north
/// components map straight onto the system X/Y axes (NFR-INT-002).
fn encode_velocity(track: &SystemTrack) -> Vec<u8> {
    let mut out = Vec::with_capacity(4);
    out.extend_from_slice(&encode_velocity_component(track.v_east));
    out.extend_from_slice(&encode_velocity_component(track.v_north));
    out
}

/// One velocity component (m/s) as a signed 16-bit count of velocity LSBs.
fn encode_velocity_component(mps: f64) -> [u8; 2] {
    let ticks = (mps / VELOCITY_LSB_MPS).round() as i16;
    ticks.to_be_bytes()
}

/// I062/060 — Track Mode 3/A Code: two octets carrying the 12-bit reply (four
/// octal digits) in the low bits. The validation flags (V/G/CH, top bits) stay
/// zero: the tracker reports a clean, validated code. Our internal `mode_3a`
/// already stores the code as that 12-bit octal value, so it maps straight in.
fn encode_mode_3a(code: u16) -> Vec<u8> {
    (code & MODE_3A_CODE_MASK).to_be_bytes().to_vec()
}

/// I062/380 — Aircraft Derived Data, here just the **Target Address** (ADR)
/// subfield: a primary subfield announcing ADR, then the 24-bit Mode S address,
/// big-endian. The address is the eventual correlation key for multi-radar
/// fusion (FR-TRK-009); only its low 24 bits are meaningful.
fn encode_target_address(address: u32) -> Vec<u8> {
    let octets = address.to_be_bytes(); // [b3, b2, b1, b0]; b3 is always 0
    vec![ADR_TARGET_ADDRESS_PRESENT, octets[1], octets[2], octets[3]]
}

/// I062/040 — the 16-bit track number, big-endian. CAT062 track numbers are
/// 16-bit, so we carry the low 16 bits of the (wider) internal track id.
fn encode_track_number(track: &SystemTrack) -> Vec<u8> {
    let number = track.id.0 as u16;
    number.to_be_bytes().to_vec()
}

/// I062/080 — Track Status, a **variable-length** item whose octets chain via the
/// FX bit. We carry the two safety-relevant flags from ADR 0008:
///
/// - **CNF** (octet 1, bit 2): confirmed vs. tentative.
/// - **CST** (octet 4, bit 8): coasting.
///
/// CST lives in the fourth octet, so a coasting track must extend that far;
/// octets 2 and 3 then carry only their FX bit (all their fields default to 0).
/// A non-coasting track needs no extension at all — one octet suffices, because
/// CST's default is already "not coasting".
fn encode_track_status(track: &SystemTrack) -> Vec<u8> {
    let mut octet1 = 0u8;
    if !track.confirmed {
        octet1 |= STATUS_CNF_TENTATIVE;
    }
    if !track.coasting {
        return vec![octet1];
    }
    vec![octet1 | FX, FX, FX, STATUS_CST_COASTING]
}

/// I062/290 — System Track Update Ages, a compound item: a primary subfield
/// (which ages follow) plus the present age octets, each in 1/4-second steps.
///
/// For the single-radar demo the tracker's generic "time since the last
/// measurement" maps to the **PSR age** subfield (age of the last primary
/// detection used to update the track). Per-technology ages (SSR, Mode S, ADS-B)
/// arrive with multi-sensor provenance in M4.
fn encode_update_ages(track: &SystemTrack) -> Vec<u8> {
    let age = scaled_u8(track.update_age, AGE_LSB_SECONDS);
    vec![AGE_PSR_PRESENT, age]
}

/// I062/500 — Estimated Accuracies, a compound item. We carry the **APC**
/// (Cartesian position accuracy) subfield: the tracker's 1σ position uncertainty
/// (metres) in both the X and Y components, a circular approximation of the error
/// ellipse, each a 16-bit count of 0.5-metre steps.
fn encode_accuracies(track: &SystemTrack) -> Vec<u8> {
    let sigma = scaled_u16(track.position_uncertainty, ACCURACY_LSB_METRES);
    let mut out = vec![ACCURACY_APC_PRESENT];
    out.extend_from_slice(&sigma.to_be_bytes()); // X component
    out.extend_from_slice(&sigma.to_be_bytes()); // Y component
    out
}

/// Quantise a non-negative value to LSB steps, saturating into one octet.
fn scaled_u8(value: f64, lsb: f64) -> u8 {
    (value / lsb).round().clamp(0.0, u8::MAX as f64) as u8
}

/// Quantise a non-negative value to LSB steps, saturating into two octets.
fn scaled_u16(value: f64, lsb: f64) -> u16 {
    (value / lsb).round().clamp(0.0, u16::MAX as f64) as u16
}

/// Wrap encoded records in the CAT062 envelope: `[CAT][LEN][record…]`, where
/// `LEN` is the total block length (header included), big-endian.
fn data_block(records: &[Vec<u8>]) -> Vec<u8> {
    let body: usize = records.iter().map(Vec::len).sum();
    let total = 3 + body; // 1 (CAT) + 2 (LEN) + payload
    let mut out = Vec::with_capacity(total);
    out.push(CATEGORY);
    out.extend_from_slice(&(total as u16).to_be_bytes());
    for record in records {
        out.extend_from_slice(record);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::TrackId;
    use firefly_geo::Wgs84;

    /// A track at a given geodetic position and velocity; status fields are
    /// irrelevant to the items encoded so far.
    fn track_at(id: u32, lat_deg: f64, lon_deg: f64, v_east: f64, v_north: f64) -> SystemTrack {
        SystemTrack {
            id: TrackId(id),
            time: Timestamp(0.0),
            position: Wgs84::from_degrees(lat_deg, lon_deg, 500.0),
            v_east,
            v_north,
            confirmed: true,
            coasting: false,
            update_age: 0.0,
            position_uncertainty: 0.0,
            mode_3a: None,
            icao_address: None,
            contributing_sensors: Vec::new(),
        }
    }

    /// The reference track used by the dump tests: deliberately chosen so every
    /// field lands on a round LSB count. 45° = 2²⁵/4 steps, 11.25° = 2²⁵/16 steps;
    /// 100 m/s = 400 steps, −50 m/s = −200 steps; age 2 s = 8 quarter-seconds;
    /// uncertainty 100 m = 200 half-metres. Confirmed and not coasting.
    fn track(id: u32) -> SystemTrack {
        let mut t = track_at(id, 45.0, 11.25, 100.0, -50.0);
        t.update_age = 2.0;
        t.position_uncertainty = 100.0;
        t
    }

    /// The system reference point used by the dump tests: the same position as
    /// [`track`], so I062/100 (system-stereographic X/Y) encodes to a clean
    /// `(0, 0)` — the reference point always projects to the plane origin.
    fn system_reference_point() -> Wgs84 {
        Wgs84::from_degrees(45.0, 11.25, 0.0)
    }

    /// Time of track scales by 1/128 s. 12.0 s → 12·128 = 1536 = 0x000600.
    /// REQ: FR-IO-003
    #[test]
    fn time_of_track_scales_to_128th_seconds() {
        assert_eq!(
            encode_time_of_track(Timestamp(12.0)),
            vec![0x00, 0x06, 0x00]
        );
        // One tick is 1/128 s; just under two ticks still rounds to one.
        assert_eq!(
            encode_time_of_track(Timestamp(1.0 / 128.0)),
            vec![0x00, 0x00, 0x01]
        );
    }

    /// Time of day folds into 24 hours so the field cannot overflow.
    #[test]
    fn time_of_track_wraps_at_one_day() {
        let just_past_midnight = encode_time_of_track(Timestamp(86_400.0 + 12.0));
        assert_eq!(just_past_midnight, vec![0x00, 0x06, 0x00]);
    }

    /// Track number is the 16-bit big-endian id.
    #[test]
    fn track_number_is_big_endian_u16() {
        assert_eq!(
            encode_track_number(&track_at(0x1234, 0.0, 0.0, 0.0, 0.0)),
            vec![0x12, 0x34]
        );
    }

    /// Position scales by 180/2²⁵ degrees and signs via two's complement.
    /// 45° = 2²⁵/4 = 0x00800000; 11.25° = 2²⁵/16 = 0x00200000; the negatives are
    /// their two's complements. REQ: FR-IO-003
    #[test]
    fn position_scales_to_wgs84_lsb_and_signs_via_twos_complement() {
        let north_east = track_at(1, 45.0, 11.25, 0.0, 0.0);
        assert_eq!(
            encode_position(&north_east),
            vec![0x00, 0x80, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00]
        );

        let south_west = track_at(1, -45.0, -11.25, 0.0, 0.0);
        assert_eq!(
            encode_position(&south_west),
            vec![0xFF, 0x80, 0x00, 0x00, 0xFF, 0xE0, 0x00, 0x00]
        );
    }

    /// I062/100 X/Y components scale by 0.5 m and sign via 24-bit two's
    /// complement; out-of-range values clamp to the field's limits
    /// (±2²³ · 0.5 m). REQ: FR-IO-003
    #[test]
    fn cartesian_component_scales_to_half_metre_and_signs_via_twos_complement() {
        // 1000 m / 0.5 m = 2000 = 0x0007D0.
        assert_eq!(encode_cartesian_component(1000.0), [0x00, 0x07, 0xD0]);
        // -1000 m -> -2000, 24-bit two's complement = 0xFFF830.
        assert_eq!(encode_cartesian_component(-1000.0), [0xFF, 0xF8, 0x30]);

        // Beyond the ±4,194,304 m range, the value clamps instead of wrapping.
        assert_eq!(encode_cartesian_component(1.0e9), [0x7F, 0xFF, 0xFF]);
        assert_eq!(encode_cartesian_component(-1.0e9), [0x80, 0x00, 0x00]);
    }

    /// I062/100 projects the track's WGS-84 position onto the
    /// system-stereographic plane (ADR 0006): a track at the system reference
    /// point itself encodes to `(0, 0)`, and a track east/north of it gets
    /// positive X/Y. REQ: FR-IO-003
    #[test]
    fn position_cartesian_uses_system_stereographic_projection() {
        let reference = Wgs84::from_degrees(45.0, 11.25, 0.0);
        let projection = StereographicProjection::new(reference);

        // The reference point itself projects to the plane origin.
        let at_reference = track_at(1, 45.0, 11.25, 0.0, 0.0);
        assert_eq!(
            encode_position_cartesian(&at_reference, &projection),
            vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );

        // A track slightly north-east of the reference gets positive X and Y,
        // matching a direct call to the projection.
        let north_east = track_at(1, 45.01, 11.26, 0.0, 0.0);
        let (x, y) = projection.project(north_east.position);
        let mut expected = encode_cartesian_component(x).to_vec();
        expected.extend_from_slice(&encode_cartesian_component(y));
        assert_eq!(
            encode_position_cartesian(&north_east, &projection),
            expected
        );
        assert!(x > 0.0 && y > 0.0, "north-east of reference -> +X, +Y");
    }

    /// Velocity scales by 0.25 m/s and signs via two's complement.
    /// 100 m/s = 400 = 0x0190; −50 m/s = −200 = 0xFF38. REQ: FR-IO-003
    #[test]
    fn velocity_scales_to_quarter_mps_and_signs_via_twos_complement() {
        let t = track_at(1, 0.0, 0.0, 100.0, -50.0);
        assert_eq!(encode_velocity(&t), vec![0x01, 0x90, 0xFF, 0x38]);
    }

    /// Mode 3/A: the 12-bit octal reply lands in the low bits, validation flags
    /// zero. 0o2613 = 0x58B → [0x05, 0x8B]; bits above the 12 are masked off.
    /// REQ: FR-IO-003, FR-TRK-009
    #[test]
    fn mode_3a_carries_the_octal_reply_in_low_twelve_bits() {
        assert_eq!(encode_mode_3a(0o2613), vec![0x05, 0x8B]);
        // 0o7777 fills all twelve bits; anything above is masked away.
        assert_eq!(encode_mode_3a(0o7777), vec![0x0F, 0xFF]);
        assert_eq!(encode_mode_3a(0xF000 | 0o1234), encode_mode_3a(0o1234));
    }

    /// Target address: I062/380 with only the ADR subfield present (0x80) and
    /// the 24-bit Mode S address big-endian. 0x3C65AC → [0x80,0x3C,0x65,0xAC].
    /// REQ: FR-IO-003, FR-TRK-009
    #[test]
    fn target_address_uses_adr_subfield_with_24_bit_address() {
        assert_eq!(
            encode_target_address(0x3C_65_AC),
            vec![0x80, 0x3C, 0x65, 0xAC]
        );
    }

    /// A track with an SSR identity adds I062/060 (FRN 9) and I062/380 (FRN 11);
    /// the FSPEC reflects both and the payloads land in UAP order between the
    /// velocity and the track number. REQ: FR-IO-003, FR-TRK-009
    #[test]
    fn identity_items_appear_only_when_present() {
        let encoder = Cat062Encoder::new(DataSourceId::new(0x19, 0x02), system_reference_point());

        // FRN 9 and FRN 11 both live in the *second* FSPEC octet: FRN 9 → bit
        // 1<<(7-((9-1)%7)) = 0x40, FRN 11 → 0x10. Octet 1 is untouched.
        let frn9_bit = 0x40;
        let frn11_bit = 0x10;

        // Baseline: no identity → neither bit set, octet 2 stays 0x0F.
        let plain = encoder.encode(Timestamp(12.0), &[track(1)]);
        assert_eq!(plain[3], 0x9F, "octet 1 unchanged by identity");
        assert_eq!(plain[4], 0x0F, "no identity → FRN 9 and 11 absent");

        // With identity → both bits appear in FSPEC octet 2.
        let mut t = track(1);
        t.mode_3a = Some(0o2613);
        t.icao_address = Some(0x3C_65_AC);
        let block = encoder.encode(Timestamp(12.0), &[t]);
        assert_eq!(block[3], 0x9F, "octet 1 still unchanged");
        assert_eq!(
            block[4],
            0x0F | frn9_bit | frn11_bit,
            "FRN 9 and 11 present"
        );

        // The two payloads sit in UAP order: I062/060 right after velocity, and
        // I062/380 right after it (both before the track number 0x00,0x01).
        let mode_3a = encode_mode_3a(0o2613);
        let address = encode_target_address(0x3C_65_AC);
        let velocity = [0x01, 0x90, 0xFF, 0x38];
        let needle: Vec<u8> = velocity
            .iter()
            .chain(mode_3a.iter())
            .chain(address.iter())
            .copied()
            .collect();
        assert!(
            block.windows(needle.len()).any(|w| w == needle.as_slice()),
            "velocity, Mode 3/A, then Target Address appear contiguously in UAP order"
        );
    }

    /// Track status is one octet unless the track is coasting. CNF (bit 2) marks
    /// a tentative track; CST lives in the 4th octet, so coasting extends the
    /// item via FX, with octets 2 and 3 as FX-only fillers. REQ: FR-IO-003, FR-TRK-008
    #[test]
    fn track_status_carries_cnf_and_extends_for_cst() {
        let mut t = track_at(1, 0.0, 0.0, 0.0, 0.0);

        t.confirmed = true;
        t.coasting = false;
        assert_eq!(encode_track_status(&t), vec![0x00], "confirmed, fresh");

        t.confirmed = false;
        assert_eq!(encode_track_status(&t), vec![0x02], "tentative, fresh");

        t.confirmed = true;
        t.coasting = true;
        assert_eq!(
            encode_track_status(&t),
            vec![0x01, 0x01, 0x01, 0x80],
            "confirmed but coasting → four octets, CST set"
        );

        t.confirmed = false;
        assert_eq!(
            encode_track_status(&t),
            vec![0x03, 0x01, 0x01, 0x80],
            "tentative and coasting"
        );
    }

    /// Update ages: the PSR-age subfield is present (0x40) and the age is in
    /// quarter-seconds. 2.0 s → 8; saturates at 255. REQ: FR-IO-003, FR-TRK-008
    #[test]
    fn update_ages_use_psr_subfield_in_quarter_seconds() {
        let mut t = track_at(1, 0.0, 0.0, 0.0, 0.0);
        t.update_age = 2.0;
        assert_eq!(encode_update_ages(&t), vec![0x40, 0x08]);

        t.update_age = 1_000.0; // far beyond 63.75 s → saturates
        assert_eq!(encode_update_ages(&t), vec![0x40, 0xFF]);
    }

    /// Estimated accuracies: the APC subfield is present (0x80); the 1σ
    /// uncertainty fills both components in half-metres. 100 m → 200 = 0x00C8.
    /// REQ: FR-IO-003, FR-TRK-008
    #[test]
    fn accuracies_use_apc_subfield_in_half_metres() {
        let mut t = track_at(1, 0.0, 0.0, 0.0, 0.0);
        t.position_uncertainty = 100.0;
        assert_eq!(encode_accuracies(&t), vec![0x80, 0x00, 0xC8, 0x00, 0xC8]);
    }

    /// One track encodes to a fully known byte string — the reference dump.
    ///
    /// Hand derivation (present FRNs {1, 4, 5, 6, 7, 12, 13, 14, 16}):
    /// - FSPEC: octet 1 = FRN1·0x80 + FRN4·0x10 + FRN5·0x08 + FRN6·0x04 + FRN7·0x02 + FX = `0x9F`;
    ///   octet 2 = FRN12·0x08 + FRN13·0x04 + FRN14·0x02 + FX = `0x0F`;
    ///   octet 3 = FRN16·0x40 = `0x40`.
    /// - I062/010 SAC/SIC = `[0x19, 0x02]`.
    /// - I062/070 at 12.0 s = 1536 ticks = `[0x00, 0x06, 0x00]`.
    /// - I062/105 lat 45° = `[0x00,0x80,0x00,0x00]`, lon 11.25° = `[0x00,0x20,0x00,0x00]`.
    /// - I062/100: the system reference point *is* the track position, so it
    ///   projects to `(0, 0)` = `[0x00,0x00,0x00, 0x00,0x00,0x00]`.
    /// - I062/185 Vx 100 m/s = `[0x01,0x90]`, Vy −50 m/s = `[0xFF,0x38]`.
    /// - I062/040 track #1 = `[0x00, 0x01]`.
    /// - I062/080 confirmed, fresh = `[0x00]`.
    /// - I062/290 PSR age 2 s = `[0x40, 0x08]`.
    /// - I062/500 APC 100 m = `[0x80, 0x00,0xC8, 0x00,0xC8]`.
    /// - record (36 bytes) wrapped: CAT 62 = 0x3E, LEN = 3 + 36 = 39 = 0x0027.
    ///
    /// REQ: FR-IO-003
    #[test]
    fn single_track_matches_reference_dump() {
        let encoder = Cat062Encoder::new(DataSourceId::new(0x19, 0x02), system_reference_point());
        let block = encoder.encode(Timestamp(12.0), &[track(1)]);

        let expected = vec![
            0x3E, // CAT 62
            0x00, 0x27, // LEN = 39
            0x9F, 0x0F, 0x40, // FSPEC {1, 4, 5, 6, 7, 12, 13, 14, 16}
            0x19, 0x02, // I062/010 SAC/SIC
            0x00, 0x06, 0x00, // I062/070 time = 1536 ticks
            0x00, 0x80, 0x00, 0x00, // I062/105 latitude 45°
            0x00, 0x20, 0x00, 0x00, // I062/105 longitude 11.25°
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // I062/100 X=0, Y=0 (reference point)
            0x01, 0x90, // I062/185 Vx = 100 m/s
            0xFF, 0x38, // I062/185 Vy = −50 m/s
            0x00, 0x01, // I062/040 track number 1
            0x00, // I062/080 confirmed, fresh
            0x40, 0x08, // I062/290 PSR age = 2 s
            0x80, 0x00, 0xC8, 0x00, 0xC8, // I062/500 APC = 100 m
        ];
        assert_eq!(block, expected);
    }

    /// LEN counts every byte of every record. Two tracks → two 36-byte records.
    /// REQ: FR-IO-003
    #[test]
    fn length_field_covers_all_records() {
        let encoder = Cat062Encoder::new(DataSourceId::new(1, 2), system_reference_point());
        let block = encoder.encode(Timestamp(0.0), &[track(1), track(2)]);

        assert_eq!(block[0], 0x3E, "category");
        let len = u16::from_be_bytes([block[1], block[2]]) as usize;
        assert_eq!(len, block.len(), "LEN equals the real block length");
        assert_eq!(len, 3 + 2 * 36, "header + two 36-byte records");
    }

    /// An empty scan still yields a valid, minimal data block (just the header).
    /// REQ: FR-IO-003
    #[test]
    fn empty_scan_is_a_valid_empty_block() {
        let encoder = Cat062Encoder::new(DataSourceId::new(1, 2), system_reference_point());
        let block = encoder.encode(Timestamp(0.0), &[]);
        assert_eq!(block, vec![0x3E, 0x00, 0x03]);
    }
}
