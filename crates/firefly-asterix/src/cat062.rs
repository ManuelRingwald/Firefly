//! CAT062 framing and data items.
//!
//! A [`Cat062Encoder`] turns the tracks of one scan into a single CAT062 *data
//! block*: a `[CAT][LEN][record…]` envelope (see [`data_block`]) around one
//! [`record`](Cat062Encoder::record) per track. The record currently carries the
//! data source (I062/010), time of track (I062/070), WGS-84 position (I062/105),
//! Cartesian velocity (I062/185) and track number (I062/040); the safety status
//! items (I062/080, /290, /500) join them in 3.X.3.
//!
//! REQ: FR-IO-003

use firefly_core::{SystemTrack, Timestamp};

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
    /// I062/185 — Calculated Track Velocity (Cartesian).
    pub const VELOCITY_CARTESIAN: u8 = 7;
    /// I062/040 — Track Number.
    pub const TRACK_NUMBER: u8 = 12;
}

/// I062/070 is counted in units of 1/128 second since midnight.
const TIME_LSB_SECONDS: f64 = 1.0 / 128.0;
/// Time of day wraps every 24 hours; we fold the timestamp into one day so the
/// 24-bit field never overflows.
const SECONDS_PER_DAY: f64 = 86_400.0;
/// I062/105 stores latitude and longitude as signed counts of 180/2²⁵ degrees.
const POSITION_LSB_DEGREES: f64 = 180.0 / (1u32 << 25) as f64;
/// I062/185 stores each velocity component as a signed count of 0.25 m/s.
const VELOCITY_LSB_MPS: f64 = 0.25;

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
/// Holds only the fixed [`DataSourceId`]; everything else comes from the tracks
/// and the scan time passed to [`encode`](Cat062Encoder::encode).
#[derive(Debug, Clone, Copy)]
pub struct Cat062Encoder {
    source: DataSourceId,
}

impl Cat062Encoder {
    /// An encoder that stamps every block with `source` as the data source.
    pub fn new(source: DataSourceId) -> Self {
        Self { source }
    }

    /// Encode all `tracks` of one scan (taken at data-time `time`) into a single
    /// CAT062 data block. With no tracks the block is a valid, empty envelope.
    pub fn encode(&self, time: Timestamp, tracks: &[SystemTrack]) -> Vec<u8> {
        let records: Vec<Vec<u8>> = tracks.iter().map(|t| self.record(time, t)).collect();
        data_block(&records)
    }

    /// One CAT062 record for one track: FSPEC + the present items in UAP order.
    fn record(&self, time: Timestamp, track: &SystemTrack) -> Vec<u8> {
        RecordBuilder::new()
            .item(
                uap::DATA_SOURCE_IDENTIFIER,
                encode_data_source(&self.source),
            )
            .item(uap::TIME_OF_TRACK_INFORMATION, encode_time_of_track(time))
            .item(uap::POSITION_WGS84, encode_position(track))
            .item(uap::VELOCITY_CARTESIAN, encode_velocity(track))
            .item(uap::TRACK_NUMBER, encode_track_number(track))
            .finish()
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

/// I062/040 — the 16-bit track number, big-endian. CAT062 track numbers are
/// 16-bit, so we carry the low 16 bits of the (wider) internal track id.
fn encode_track_number(track: &SystemTrack) -> Vec<u8> {
    let number = track.id.0 as u16;
    number.to_be_bytes().to_vec()
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
        }
    }

    /// The reference track used by the dump tests: deliberately chosen so its
    /// position and velocity land on round LSB counts. 45° = 2²⁵/4 steps,
    /// 11.25° = 2²⁵/16 steps; 100 m/s = 400 steps, −50 m/s = −200 steps.
    fn track(id: u32) -> SystemTrack {
        track_at(id, 45.0, 11.25, 100.0, -50.0)
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

    /// Velocity scales by 0.25 m/s and signs via two's complement.
    /// 100 m/s = 400 = 0x0190; −50 m/s = −200 = 0xFF38. REQ: FR-IO-003
    #[test]
    fn velocity_scales_to_quarter_mps_and_signs_via_twos_complement() {
        let t = track_at(1, 0.0, 0.0, 100.0, -50.0);
        assert_eq!(encode_velocity(&t), vec![0x01, 0x90, 0xFF, 0x38]);
    }

    /// One track encodes to a fully known byte string — the reference dump.
    ///
    /// Hand derivation (present FRNs {1, 4, 5, 7, 12}):
    /// - FSPEC: octet 1 = FRN1·0x80 + FRN4·0x10 + FRN5·0x08 + FRN7·0x02 + FX·0x01
    ///   = `0x9B`; octet 2 = FRN12·0x08 = `0x08`.
    /// - I062/010 SAC/SIC = `[0x19, 0x02]`.
    /// - I062/070 at 12.0 s = 1536 ticks = `[0x00, 0x06, 0x00]`.
    /// - I062/105 lat 45° = `[0x00,0x80,0x00,0x00]`, lon 11.25° = `[0x00,0x20,0x00,0x00]`.
    /// - I062/185 Vx 100 m/s = `[0x01,0x90]`, Vy −50 m/s = `[0xFF,0x38]`.
    /// - I062/040 track #1 = `[0x00, 0x01]`.
    /// - record (21 bytes) wrapped: CAT 62 = 0x3E, LEN = 3 + 21 = 24 = 0x0018.
    ///
    /// REQ: FR-IO-003
    #[test]
    fn single_track_matches_reference_dump() {
        let encoder = Cat062Encoder::new(DataSourceId::new(0x19, 0x02));
        let block = encoder.encode(Timestamp(12.0), &[track(1)]);

        let expected = vec![
            0x3E, // CAT 62
            0x00, 0x18, // LEN = 24
            0x9B, 0x08, // FSPEC {1, 4, 5, 7, 12}
            0x19, 0x02, // I062/010 SAC/SIC
            0x00, 0x06, 0x00, // I062/070 time = 1536 ticks
            0x00, 0x80, 0x00, 0x00, // I062/105 latitude 45°
            0x00, 0x20, 0x00, 0x00, // I062/105 longitude 11.25°
            0x01, 0x90, // I062/185 Vx = 100 m/s
            0xFF, 0x38, // I062/185 Vy = −50 m/s
            0x00, 0x01, // I062/040 track number 1
        ];
        assert_eq!(block, expected);
    }

    /// LEN counts every byte of every record. Two tracks → two 21-byte records.
    /// REQ: FR-IO-003
    #[test]
    fn length_field_covers_all_records() {
        let encoder = Cat062Encoder::new(DataSourceId::new(1, 2));
        let block = encoder.encode(Timestamp(0.0), &[track(1), track(2)]);

        assert_eq!(block[0], 0x3E, "category");
        let len = u16::from_be_bytes([block[1], block[2]]) as usize;
        assert_eq!(len, block.len(), "LEN equals the real block length");
        assert_eq!(len, 3 + 2 * 21, "header + two 21-byte records");
    }

    /// An empty scan still yields a valid, minimal data block (just the header).
    /// REQ: FR-IO-003
    #[test]
    fn empty_scan_is_a_valid_empty_block() {
        let encoder = Cat062Encoder::new(DataSourceId::new(1, 2));
        let block = encoder.encode(Timestamp(0.0), &[]);
        assert_eq!(block, vec![0x3E, 0x00, 0x03]);
    }
}
