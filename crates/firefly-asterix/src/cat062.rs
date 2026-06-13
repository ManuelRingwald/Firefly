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

use std::collections::BTreeSet;

use firefly_core::{SystemTrack, Timestamp};
use firefly_geo::{StereographicProjection, Wgs84};

use crate::fspec::{self, RecordBuilder};

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
/// Holds the fixed [`DataSourceId`], the [`StereographicProjection`] used to
/// compute I062/100 (system-stereographic X/Y, ADR 0006), and the simulation
/// start time for correct Time-of-Day encoding (I062/070, ASTERIX). Everything
/// else comes from the tracks and the scan time passed to
/// [`encode`](Cat062Encoder::encode).
#[derive(Debug, Clone, Copy)]
pub struct Cat062Encoder {
    source: DataSourceId,
    system_projection: StereographicProjection,
    /// Seconds since UTC midnight at the start of the simulation.
    /// I062/070 is encoded as `(simulation_start_time_of_day + timestamp) % 86400`.
    simulation_start_time_of_day: f64,
}

impl Cat062Encoder {
    /// An encoder that stamps every block with `source` as the data source,
    /// projects positions onto the system-stereographic plane tangent at
    /// `system_reference_point` (the system reference point, e.g. the tracking
    /// frame's origin) for I062/100, and encodes Time-of-Day (I062/070) as
    /// `(simulation_start_time_of_day + timestamp) % 86400` seconds.
    pub fn new(
        source: DataSourceId,
        system_reference_point: Wgs84,
        simulation_start_time_of_day: f64,
    ) -> Self {
        Self {
            source,
            system_projection: StereographicProjection::new(system_reference_point),
            simulation_start_time_of_day,
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
            .item(
                uap::TIME_OF_TRACK_INFORMATION,
                encode_time_of_track(self.simulation_start_time_of_day, time),
            )
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
///
/// `simulation_start_time_of_day` is the UTC time (seconds since midnight)
/// at which the simulation started. The Time-of-Day is computed as
/// `(simulation_start_time_of_day + timestamp) % 86400`.
fn encode_time_of_track(simulation_start_time_of_day: f64, time: Timestamp) -> Vec<u8> {
    let tod = (simulation_start_time_of_day + time.as_secs()).rem_euclid(SECONDS_PER_DAY);
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

/// One decoded CAT062 record — the inverse of [`Cat062Encoder::record`].
///
/// Mirrors the fields of [`SystemTrack`], plus the wire-level extras
/// (`source`, `track_number`, `cartesian`) that don't have a place in the
/// neutral track but are useful to a recorder verifying the feed.
///
/// [`position`](Self::position) comes from I062/105 (WGS-84); its `height` is
/// always `0`, since CAT062 carries no height for system tracks.
/// [`cartesian`](Self::cartesian) is I062/100's X/Y in metres, *not yet*
/// converted back to latitude/longitude — that needs the system reference
/// point's [`StereographicProjection`] (a follow-up häppchen).
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedRecord {
    /// I062/010 — the data source that produced this track.
    pub source: DataSourceId,
    /// I062/070 — time of track, as a time-of-day (wraps every 24 hours).
    pub time: Timestamp,
    /// I062/105 — geodetic position (height always `0`).
    pub position: Wgs84,
    /// I062/100 — system-stereographic X/Y, in metres.
    pub cartesian: (f64, f64),
    /// I062/185 — eastward velocity, m/s.
    pub v_east: f64,
    /// I062/185 — northward velocity, m/s.
    pub v_north: f64,
    /// I062/040 — track number.
    pub track_number: u16,
    /// I062/080 CNF — `true` unless the track is still tentative.
    pub confirmed: bool,
    /// I062/080 CST — `true` while the track is coasting.
    pub coasting: bool,
    /// I062/290 PSR age, seconds.
    pub update_age: f64,
    /// I062/500 APC, 1σ position uncertainty in metres.
    pub position_uncertainty: f64,
    /// I062/060, if the track carries an SSR identity.
    pub mode_3a: Option<u16>,
    /// I062/380 Target Address, if the track carries a Mode S address.
    pub icao_address: Option<u32>,
}

/// Errors that can occur while decoding a CAT062 data block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// The input ended before a complete block/record/item could be read.
    Truncated,
    /// The first octet was not [`CATEGORY`] (62).
    WrongCategory(u8),
    /// The `LEN` field did not match the actual input length.
    LengthMismatch { declared: usize, actual: usize },
    /// The FSPEC marked an FRN present that this decoder doesn't know.
    UnknownItem(u8),
    /// A record's FSPEC was missing an item this decoder requires.
    MissingItem(u8),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Truncated => write!(f, "input ended before a complete item"),
            DecodeError::WrongCategory(cat) => write!(f, "expected CAT 62, got CAT {cat}"),
            DecodeError::LengthMismatch { declared, actual } => write!(
                f,
                "LEN field says {declared} bytes, but input is {actual} bytes"
            ),
            DecodeError::UnknownItem(frn) => write!(f, "FSPEC marks unknown FRN {frn} present"),
            DecodeError::MissingItem(frn) => write!(f, "record is missing required FRN {frn}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Decode a CAT062 data block (`[CAT][LEN][record…]`) into one
/// [`DecodedRecord`] per record — the inverse of
/// [`Cat062Encoder::encode`].
pub fn decode_data_block(bytes: &[u8]) -> Result<Vec<DecodedRecord>, DecodeError> {
    if bytes.len() < 3 {
        return Err(DecodeError::Truncated);
    }
    if bytes[0] != CATEGORY {
        return Err(DecodeError::WrongCategory(bytes[0]));
    }
    let declared = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    if declared != bytes.len() {
        return Err(DecodeError::LengthMismatch {
            declared,
            actual: bytes.len(),
        });
    }

    let mut cursor = Cursor::new(&bytes[3..]);
    let mut records = Vec::new();
    while cursor.remaining() > 0 {
        records.push(decode_record(&mut cursor)?);
    }
    Ok(records)
}

/// One record: FSPEC, then the present items in ascending-FRN (UAP) order —
/// the inverse of [`Cat062Encoder::record`].
fn decode_record(cursor: &mut Cursor) -> Result<DecodedRecord, DecodeError> {
    let frns = cursor.take_fspec()?;

    let mut source = None;
    let mut time = None;
    let mut position = None;
    let mut cartesian = None;
    let mut velocity = None;
    let mut track_number = None;
    let mut status = None;
    let mut update_age = None;
    let mut position_uncertainty = None;
    let mut mode_3a = None;
    let mut icao_address = None;

    for frn in frns {
        match frn {
            uap::DATA_SOURCE_IDENTIFIER => source = Some(decode_data_source(cursor.take(2)?)),
            uap::TIME_OF_TRACK_INFORMATION => time = Some(decode_time_of_track(cursor.take(3)?)),
            uap::POSITION_WGS84 => position = Some(decode_position(cursor.take(8)?)),
            uap::POSITION_CARTESIAN => cartesian = Some(decode_position_cartesian(cursor.take(6)?)),
            uap::VELOCITY_CARTESIAN => velocity = Some(decode_velocity(cursor.take(4)?)),
            uap::MODE_3A_CODE => mode_3a = Some(decode_mode_3a(cursor.take(2)?)),
            uap::AIRCRAFT_DERIVED_DATA => {
                icao_address = Some(decode_target_address(cursor.take(4)?))
            }
            uap::TRACK_NUMBER => track_number = Some(decode_track_number(cursor.take(2)?)),
            uap::TRACK_STATUS => status = Some(decode_track_status(cursor.take_track_status()?)),
            uap::UPDATE_AGES => update_age = Some(decode_update_ages(cursor.take(2)?)),
            uap::ESTIMATED_ACCURACIES => {
                position_uncertainty = Some(decode_accuracies(cursor.take(5)?))
            }
            other => return Err(DecodeError::UnknownItem(other)),
        }
    }

    let (confirmed, coasting) = status.ok_or(DecodeError::MissingItem(uap::TRACK_STATUS))?;
    let (v_east, v_north) = velocity.ok_or(DecodeError::MissingItem(uap::VELOCITY_CARTESIAN))?;

    Ok(DecodedRecord {
        source: source.ok_or(DecodeError::MissingItem(uap::DATA_SOURCE_IDENTIFIER))?,
        time: time.ok_or(DecodeError::MissingItem(uap::TIME_OF_TRACK_INFORMATION))?,
        position: position.ok_or(DecodeError::MissingItem(uap::POSITION_WGS84))?,
        cartesian: cartesian.ok_or(DecodeError::MissingItem(uap::POSITION_CARTESIAN))?,
        v_east,
        v_north,
        track_number: track_number.ok_or(DecodeError::MissingItem(uap::TRACK_NUMBER))?,
        confirmed,
        coasting,
        update_age: update_age.ok_or(DecodeError::MissingItem(uap::UPDATE_AGES))?,
        position_uncertainty: position_uncertainty
            .ok_or(DecodeError::MissingItem(uap::ESTIMATED_ACCURACIES))?,
        mode_3a,
        icao_address,
    })
}

/// I062/010 — the inverse of [`encode_data_source`].
fn decode_data_source(bytes: &[u8]) -> DataSourceId {
    DataSourceId::new(bytes[0], bytes[1])
}

/// I062/070 — the inverse of [`encode_time_of_track`]. The result is a
/// time-of-day in `[0, 86_400)` seconds; the original day is not recoverable
/// (CAT062 doesn't carry it).
fn decode_time_of_track(bytes: &[u8]) -> Timestamp {
    let ticks = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
    Timestamp(ticks as f64 * TIME_LSB_SECONDS)
}

/// I062/105 — the inverse of [`encode_position`]. Height is not part of
/// I062/105, so it comes back as `0`.
fn decode_position(bytes: &[u8]) -> Wgs84 {
    let lat = decode_wgs84_angle(&bytes[0..4]);
    let lon = decode_wgs84_angle(&bytes[4..8]);
    Wgs84::from_degrees(lat, lon, 0.0)
}

/// One WGS-84 angle (degrees) — the inverse of [`encode_wgs84_angle`].
fn decode_wgs84_angle(bytes: &[u8]) -> f64 {
    let ticks = i32::from_be_bytes(bytes.try_into().unwrap());
    ticks as f64 * POSITION_LSB_DEGREES
}

/// I062/100 — the inverse of [`encode_position_cartesian`]. Returns the
/// system-stereographic `(x, y)` in metres; converting back to a geodetic
/// position needs the system reference point's projection.
fn decode_position_cartesian(bytes: &[u8]) -> (f64, f64) {
    (
        decode_cartesian_component(&bytes[0..3]),
        decode_cartesian_component(&bytes[3..6]),
    )
}

/// One system-stereographic component — the inverse of
/// [`encode_cartesian_component`]: a 24-bit two's-complement count of
/// 0.5-metre steps, sign-extended to `i32`.
fn decode_cartesian_component(bytes: &[u8]) -> f64 {
    let mut ticks = i32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
    if ticks & 0x0080_0000 != 0 {
        ticks -= 0x0100_0000;
    }
    ticks as f64 * POSITION_CARTESIAN_LSB_METRES
}

/// Recover the geodetic position that I062/100's `cartesian` (X, Y in
/// metres) encodes — the inverse of the projection
/// [`encode_position_cartesian`] applies. `height` is carried through
/// unchanged (the projection is purely horizontal); I062/100 itself carries
/// no height, so callers typically pass `0.0` or the height decoded from
/// I062/105.
pub fn unproject_cartesian_position(
    cartesian: (f64, f64),
    height: f64,
    projection: &StereographicProjection,
) -> Wgs84 {
    projection.unproject(cartesian.0, cartesian.1, height)
}

/// I062/185 — the inverse of [`encode_velocity`].
fn decode_velocity(bytes: &[u8]) -> (f64, f64) {
    (
        decode_velocity_component(&bytes[0..2]),
        decode_velocity_component(&bytes[2..4]),
    )
}

/// One velocity component — the inverse of [`encode_velocity_component`].
fn decode_velocity_component(bytes: &[u8]) -> f64 {
    let ticks = i16::from_be_bytes(bytes.try_into().unwrap());
    ticks as f64 * VELOCITY_LSB_MPS
}

/// I062/060 — the inverse of [`encode_mode_3a`].
fn decode_mode_3a(bytes: &[u8]) -> u16 {
    u16::from_be_bytes(bytes.try_into().unwrap()) & MODE_3A_CODE_MASK
}

/// I062/380 — the inverse of [`encode_target_address`]. The leading
/// subfield octet (always [`ADR_TARGET_ADDRESS_PRESENT`] for our encoder) is
/// dropped.
fn decode_target_address(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([0, bytes[1], bytes[2], bytes[3]])
}

/// I062/040 — the inverse of [`encode_track_number`].
fn decode_track_number(bytes: &[u8]) -> u16 {
    u16::from_be_bytes(bytes.try_into().unwrap())
}

/// I062/080 — the inverse of [`encode_track_status`]: CNF from octet 1, CST
/// from octet 4 if the item was extended that far.
fn decode_track_status(bytes: &[u8]) -> (bool, bool) {
    let confirmed = bytes[0] & STATUS_CNF_TENTATIVE == 0;
    let coasting = bytes.len() >= 4 && bytes[3] & STATUS_CST_COASTING != 0;
    (confirmed, coasting)
}

/// I062/290 — the inverse of [`encode_update_ages`]: our encoder always
/// carries exactly the PSR-age subfield, so the second octet is the age.
fn decode_update_ages(bytes: &[u8]) -> f64 {
    bytes[1] as f64 * AGE_LSB_SECONDS
}

/// I062/500 — the inverse of [`encode_accuracies`]: our encoder always
/// carries the APC subfield with identical X and Y, so either suffices.
fn decode_accuracies(bytes: &[u8]) -> f64 {
    let sigma = u16::from_be_bytes([bytes[1], bytes[2]]);
    sigma as f64 * ACCURACY_LSB_METRES
}

/// A read cursor over a record's bytes, used by [`decode_record`] to consume
/// items in FSPEC order while reporting truncated input as
/// [`DecodeError::Truncated`].
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    /// Take the next `n` bytes, advancing the cursor.
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.remaining() < n {
            return Err(DecodeError::Truncated);
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    /// Parse the FSPEC at the cursor, returning the present FRNs and
    /// advancing past its octets.
    fn take_fspec(&mut self) -> Result<BTreeSet<u8>, DecodeError> {
        let (frns, consumed) = fspec::parse(&self.bytes[self.pos..]);
        if consumed == 0 {
            return Err(DecodeError::Truncated);
        }
        self.pos += consumed;
        Ok(frns)
    }

    /// Take I062/080's FX-chained octets: the first octet, plus any further
    /// octets while the FX bit (`0x01`) is set.
    fn take_track_status(&mut self) -> Result<&'a [u8], DecodeError> {
        let start = self.pos;
        loop {
            let octet = self.take(1)?[0];
            if octet & FX == 0 {
                break;
            }
        }
        Ok(&self.bytes[start..self.pos])
    }
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
            encode_time_of_track(0.0, Timestamp(12.0)),
            vec![0x00, 0x06, 0x00]
        );
        // One tick is 1/128 s; just under two ticks still rounds to one.
        assert_eq!(
            encode_time_of_track(0.0, Timestamp(1.0 / 128.0)),
            vec![0x00, 0x00, 0x01]
        );
    }

    /// Time of day folds into 24 hours so the field cannot overflow.
    #[test]
    fn time_of_track_wraps_at_one_day() {
        let just_past_midnight = encode_time_of_track(0.0, Timestamp(86_400.0 + 12.0));
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
        let encoder = Cat062Encoder::new(DataSourceId::new(0x19, 0x02), system_reference_point(), 0.0);

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
        let encoder = Cat062Encoder::new(DataSourceId::new(0x19, 0x02), system_reference_point(), 0.0);
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
        let encoder = Cat062Encoder::new(DataSourceId::new(1, 2), system_reference_point(), 0.0);
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
        let encoder = Cat062Encoder::new(DataSourceId::new(1, 2), system_reference_point(), 0.0);
        let block = encoder.encode(Timestamp(0.0), &[]);
        assert_eq!(block, vec![0x3E, 0x00, 0x03]);
    }

    /// `decode_data_block` is the inverse of `encode`: every item the encoder
    /// wrote comes back with its original (LSB-quantised) value. The reference
    /// track has no identity, is confirmed and not coasting.
    /// REQ: FR-IO-003
    #[test]
    fn decode_inverts_encode_for_a_plain_track() {
        let encoder = Cat062Encoder::new(DataSourceId::new(0x19, 0x02), system_reference_point(), 0.0);
        let block = encoder.encode(Timestamp(12.0), &[track(1)]);

        let records = decode_data_block(&block).unwrap();
        assert_eq!(records.len(), 1);
        let record = &records[0];

        assert_eq!(record.source, DataSourceId::new(0x19, 0x02));
        assert_eq!(record.time, Timestamp(12.0));
        assert!((record.position.lat_deg() - 45.0).abs() < 1e-9);
        assert!((record.position.lon_deg() - 11.25).abs() < 1e-9);
        assert_eq!(record.cartesian, (0.0, 0.0));
        assert_eq!(record.v_east, 100.0);
        assert_eq!(record.v_north, -50.0);
        assert_eq!(record.track_number, 1);
        assert!(record.confirmed);
        assert!(!record.coasting);
        assert_eq!(record.update_age, 2.0);
        assert_eq!(record.position_uncertainty, 100.0);
        assert_eq!(record.mode_3a, None);
        assert_eq!(record.icao_address, None);
    }

    /// A track with an SSR identity decodes I062/060 and I062/380 back into
    /// `mode_3a`/`icao_address`. REQ: FR-IO-003, FR-TRK-009
    #[test]
    fn decode_recovers_identity_when_present() {
        let encoder = Cat062Encoder::new(DataSourceId::new(0x19, 0x02), system_reference_point(), 0.0);
        let mut t = track(1);
        t.mode_3a = Some(0o2613);
        t.icao_address = Some(0x3C_65_AC);
        let block = encoder.encode(Timestamp(12.0), &[t]);

        let records = decode_data_block(&block).unwrap();
        assert_eq!(records[0].mode_3a, Some(0o2613));
        assert_eq!(records[0].icao_address, Some(0x3C_65_AC));
    }

    /// A tentative, coasting track extends I062/080 to four octets via FX;
    /// `decode_track_status` follows the chain and recovers both flags.
    /// REQ: FR-IO-003, FR-TRK-008
    #[test]
    fn decode_recovers_status_for_tentative_coasting_track() {
        let encoder = Cat062Encoder::new(DataSourceId::new(1, 2), system_reference_point(), 0.0);
        let mut t = track(1);
        t.confirmed = false;
        t.coasting = true;
        let block = encoder.encode(Timestamp(0.0), &[t]);

        let records = decode_data_block(&block).unwrap();
        assert!(!records[0].confirmed);
        assert!(records[0].coasting);
    }

    /// A block with two tracks decodes into two records, in order.
    /// REQ: FR-IO-003
    #[test]
    fn decode_handles_multiple_records() {
        let encoder = Cat062Encoder::new(DataSourceId::new(1, 2), system_reference_point(), 0.0);
        let block = encoder.encode(Timestamp(0.0), &[track(1), track(2)]);

        let records = decode_data_block(&block).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].track_number, 1);
        assert_eq!(records[1].track_number, 2);
    }

    /// An empty data block decodes to an empty list of records.
    /// REQ: FR-IO-003
    #[test]
    fn decode_handles_empty_block() {
        let encoder = Cat062Encoder::new(DataSourceId::new(1, 2), system_reference_point(), 0.0);
        let block = encoder.encode(Timestamp(0.0), &[]);

        assert_eq!(decode_data_block(&block).unwrap(), vec![]);
    }

    /// I062/100's `(0, 0)` — the plane origin — unprojects back to the system
    /// reference point itself. REQ: FR-IO-004, FR-GEO-004
    #[test]
    fn cartesian_origin_unprojects_to_reference_point() {
        let reference = system_reference_point();
        let projection = StereographicProjection::new(reference);

        let recovered = unproject_cartesian_position((0.0, 0.0), 0.0, &projection);
        assert!((recovered.lat_deg() - reference.lat_deg()).abs() < 1e-9);
        assert!((recovered.lon_deg() - reference.lon_deg()).abs() < 1e-9);
    }

    /// I062/100 and I062/105 are two independent encodings of the same
    /// position; decoding both and unprojecting I062/100 recovers a position
    /// that agrees with I062/105 to within both items' LSBs (sub-metre).
    /// REQ: FR-IO-004, FR-GEO-004
    #[test]
    fn position_cartesian_unprojects_close_to_position_wgs84() {
        let reference = system_reference_point();
        let projection = StereographicProjection::new(reference);
        let encoder = Cat062Encoder::new(DataSourceId::new(1, 2), reference, 0.0);

        let block = encoder.encode(Timestamp(0.0), &[track_at(1, 45.01, 11.26, 0.0, 0.0)]);
        let record = &decode_data_block(&block).unwrap()[0];

        let from_cartesian =
            unproject_cartesian_position(record.cartesian, record.position.height, &projection);

        // Compare in the projection plane (metres), where both items' LSBs
        // (0.5 m for I062/100, ~0.6 m for I062/105 at this latitude) live.
        let (x1, y1) = projection.project(record.position);
        let (x2, y2) = projection.project(from_cartesian);
        assert!((x1 - x2).abs() < 1.0, "x within 1 m: {x1} vs {x2}");
        assert!((y1 - y2).abs() < 1.0, "y within 1 m: {y1} vs {y2}");
    }

    /// Decoding rejects input that isn't a CAT062 block, whose LEN doesn't
    /// match, or that ends mid-item. REQ: FR-IO-003
    #[test]
    fn decode_rejects_malformed_input() {
        assert_eq!(decode_data_block(&[]), Err(DecodeError::Truncated));
        assert_eq!(
            decode_data_block(&[0x00, 0x00]),
            Err(DecodeError::Truncated)
        );
        assert_eq!(
            decode_data_block(&[0x01, 0x00, 0x03]),
            Err(DecodeError::WrongCategory(0x01))
        );
        assert_eq!(
            decode_data_block(&[0x3E, 0x00, 0x05]),
            Err(DecodeError::LengthMismatch {
                declared: 5,
                actual: 3
            })
        );

        // A valid header but a record cut short mid-FSPEC payload.
        let encoder = Cat062Encoder::new(DataSourceId::new(1, 2), system_reference_point(), 0.0);
        let mut block = encoder.encode(Timestamp(0.0), &[track(1)]);
        let truncated_len = (block.len() - 1) as u16;
        block.truncate(block.len() - 1);
        block[1..3].copy_from_slice(&truncated_len.to_be_bytes());
        assert_eq!(decode_data_block(&block), Err(DecodeError::Truncated));
    }
}
