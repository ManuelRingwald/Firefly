//! CAT063 framing and data items — **Sensor Status Messages**, the per-sensor
//! health signal.
//!
//! Where CAT065 tells the consumer "the SDPS is alive" (a single, global
//! heartbeat), CAT063 tells it "here is the status of each individual sensor":
//! which radars are currently operational and which have gone silent. This lets
//! the ASD distinguish three operational states:
//!
//! - **All sensors operational** — the normal, green picture.
//! - **Some sensors degraded** — the SDPS is running, but at least one radar
//!   has stopped contributing plots (yellow / partial coverage).
//! - **No heartbeat at all** — the feed is dead (detected via CAT065 staleness).
//!
//! In Firefly's architecture the SDPS acts as proxy: it knows which sensors are
//! registered and monitors their plot activity, then sends CAT063 blocks on the
//! **same** multicast group/port as CAT062 and CAT065, so a single listener
//! receives the full picture.
//!
//! Each CAT063 **data block** carries one record per sensor. Each record uses
//! the sensor's own Data Source Identifier (I063/010: SAC/SIC) so the consumer
//! can correlate records back to specific radars. The record also carries the
//! time of day (I063/030) and a one-byte Configuration and Status field
//! (I063/060) whose two NOGO bits report operational vs. degraded.
//!
//! Items and bit layouts are taken from **EUROCONTROL SUR.ET1.ST05.2000-STD-01-01**
//! ("ASTERIX Category 063 — Sensor Status Messages").
//!
//! REQ: FR-IO-007

use crate::cat062::DataSourceId;
use crate::fspec::RecordBuilder;

/// The ASTERIX category number for Sensor Status Messages.
pub(crate) const CATEGORY: u8 = 63;

/// I063/030 — counted in 1/128-second ticks since UTC midnight (same LSB as
/// I062/070 and I065/030).
const TIME_LSB_SECONDS: f64 = 1.0 / 128.0;
/// Fold the time-of-day value into one day so the 24-bit counter never wraps.
const SECONDS_PER_DAY: f64 = 86_400.0;

/// I063/060 NOGO bits (Bits 8–7 of the first status octet).
///
/// `00` = operational (all subsystems nominal).  `01` = degraded (sensor alive
/// but not fully contributing).  `10` = not connected.  `11` = not initialised.
const NOGO_OPERATIONAL: u8 = 0x00;
const NOGO_DEGRADED: u8 = 0x40;

/// UAP field reference numbers for the three items we emit.
mod uap {
    /// I063/010 — Data Source Identifier (SAC/SIC of the reporting sensor).
    pub const DATA_SOURCE_IDENTIFIER: u8 = 1;
    /// I063/030 — Time of Day.
    pub const TIME_OF_DAY: u8 = 2;
    /// I063/060 — Sensor Configuration and Status.
    pub const SENSOR_CONFIGURATION_STATUS: u8 = 3;
}

/// Encodes Sensor Status records into a CAT063 data block.
///
/// The encoder holds one fixed parameter: the **sensor SAC** (System Area Code)
/// shared by all local sensors.  In the standard Firefly deployment all sensors
/// are on SAC 0; the sensor SIC distinguishes them.  The SDPS sends one record
/// per sensor in a single block, acting as proxy for each radar.
#[derive(Debug, Clone, Copy)]
pub struct Cat063Encoder {
    sensor_sac: u8,
}

impl Cat063Encoder {
    /// An encoder that stamps every sensor record with `sensor_sac` as the SAC
    /// component of I063/010.  For Firefly deployments pass `0`.
    pub fn new(sensor_sac: u8) -> Self {
        Self { sensor_sac }
    }

    /// Encode a CAT063 data block carrying one status record per sensor.
    ///
    /// `time_of_day_secs` is seconds since UTC midnight (I063/030).
    /// `sensors` is a slice of `(sensor_sic, operational)` pairs — one entry per
    /// sensor, in the order they should appear in the block.  An empty slice
    /// produces a valid but empty block (three-byte header only).
    pub fn encode(&self, time_of_day_secs: f64, sensors: &[(u8, bool)]) -> Vec<u8> {
        let mut records_bytes: Vec<u8> = Vec::new();
        for &(sic, operational) in sensors {
            let source = DataSourceId::new(self.sensor_sac, sic);
            let record = RecordBuilder::new()
                .item(uap::DATA_SOURCE_IDENTIFIER, vec![source.sac, source.sic])
                .item(uap::TIME_OF_DAY, encode_time_of_day(time_of_day_secs))
                .item(
                    uap::SENSOR_CONFIGURATION_STATUS,
                    vec![encode_nogo(operational)],
                )
                .finish();
            records_bytes.extend_from_slice(&record);
        }
        data_block(&records_bytes)
    }
}

/// I063/030 — time of day as a 24-bit count of 1/128-second ticks since
/// midnight, big-endian (same encoding as I062/070 and I065/030).
fn encode_time_of_day(secs: f64) -> Vec<u8> {
    let tod = secs.rem_euclid(SECONDS_PER_DAY);
    let ticks = (tod / TIME_LSB_SECONDS).round() as u32;
    ticks.to_be_bytes()[1..4].to_vec()
}

/// I063/060 first octet — NOGO bits encode operational vs. degraded.
fn encode_nogo(operational: bool) -> u8 {
    if operational {
        NOGO_OPERATIONAL
    } else {
        NOGO_DEGRADED
    }
}

/// Wrap the concatenated record bytes in the CAT063 block envelope:
/// `[CAT=63][LEN big-endian u16][records…]`, where LEN includes the 3-byte
/// header.
fn data_block(records: &[u8]) -> Vec<u8> {
    let total = 3 + records.len();
    let mut out = Vec::with_capacity(total);
    out.push(CATEGORY);
    out.extend_from_slice(&(total as u16).to_be_bytes());
    out.extend_from_slice(records);
    out
}

/// One decoded CAT063 sensor status record — the inverse of one record inside
/// a block produced by [`Cat063Encoder::encode`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodedSensorStatus {
    /// I063/010 — the data source that produced this record (identifies the
    /// sensor: SAC/SIC).
    pub source: DataSourceId,
    /// I063/030 — time of day (wraps at midnight).
    pub time_of_day_secs: f64,
    /// I063/060 NOGO — `true` when the sensor reports itself operational.
    pub operational: bool,
}

/// Errors that can occur while decoding a CAT063 data block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cat063DecodeError {
    /// Input ended before a complete item could be read.
    Truncated,
    /// The first byte was not CAT 63.
    WrongCategory(u8),
    /// The LEN field did not match the actual slice length.
    LengthMismatch { declared: usize, actual: usize },
    /// The FSPEC indicated an unknown FRN.
    UnknownFrn(u8),
    /// A mandatory field was absent from a record.
    MissingItem(u8),
}

impl std::fmt::Display for Cat063DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cat063DecodeError::Truncated => write!(f, "input truncated"),
            Cat063DecodeError::WrongCategory(c) => write!(f, "expected CAT 63, got CAT {c}"),
            Cat063DecodeError::LengthMismatch { declared, actual } => {
                write!(f, "LEN={declared} but input is {actual} bytes")
            }
            Cat063DecodeError::UnknownFrn(frn) => write!(f, "unknown FRN {frn}"),
            Cat063DecodeError::MissingItem(frn) => write!(f, "missing required FRN {frn}"),
        }
    }
}

impl std::error::Error for Cat063DecodeError {}

/// Decode a CAT063 data block into its sensor status records.
///
/// This is the inverse of [`Cat063Encoder::encode`]; it is also the ground
/// truth Wayfinder's CAT063 decoder must match. Robust: never panics on
/// malformed input.
pub fn decode_sensor_block(bytes: &[u8]) -> Result<Vec<DecodedSensorStatus>, Cat063DecodeError> {
    if bytes.len() < 3 {
        return Err(Cat063DecodeError::Truncated);
    }
    if bytes[0] != CATEGORY {
        return Err(Cat063DecodeError::WrongCategory(bytes[0]));
    }
    let declared = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    if declared < 3 || declared > bytes.len() {
        return Err(Cat063DecodeError::LengthMismatch {
            declared,
            actual: bytes.len(),
        });
    }

    let mut pos = 3;
    let mut records = Vec::new();
    while pos < declared {
        let (record, next) = decode_record(bytes, pos, declared)?;
        records.push(record);
        pos = next;
    }
    Ok(records)
}

/// Decode one record starting at byte `pos`, within the block that ends at
/// `end`.  Returns the decoded record and the offset just past its last byte.
fn decode_record(
    bytes: &[u8],
    pos: usize,
    end: usize,
) -> Result<(DecodedSensorStatus, usize), Cat063DecodeError> {
    use crate::fspec;

    let (frns, consumed) = fspec::parse(&bytes[pos..end]);
    if consumed == 0 {
        return Err(Cat063DecodeError::Truncated);
    }
    let mut cur = pos + consumed;

    let mut take = |n: usize| -> Result<&[u8], Cat063DecodeError> {
        if end - cur < n {
            return Err(Cat063DecodeError::Truncated);
        }
        let s = &bytes[cur..cur + n];
        cur += n;
        Ok(s)
    };

    let mut source: Option<DataSourceId> = None;
    let mut time_of_day_secs: Option<f64> = None;
    let mut operational: Option<bool> = None;

    for frn in frns {
        match frn {
            uap::DATA_SOURCE_IDENTIFIER => {
                let b = take(2)?;
                source = Some(DataSourceId::new(b[0], b[1]));
            }
            uap::TIME_OF_DAY => {
                let b = take(3)?;
                let ticks = u32::from_be_bytes([0, b[0], b[1], b[2]]);
                time_of_day_secs = Some(ticks as f64 * TIME_LSB_SECONDS);
            }
            uap::SENSOR_CONFIGURATION_STATUS => {
                let b = take(1)?;
                operational = Some(b[0] & 0xC0 == NOGO_OPERATIONAL);
            }
            other => return Err(Cat063DecodeError::UnknownFrn(other)),
        }
    }

    let record = DecodedSensorStatus {
        source: source.ok_or(Cat063DecodeError::MissingItem(uap::DATA_SOURCE_IDENTIFIER))?,
        time_of_day_secs: time_of_day_secs
            .ok_or(Cat063DecodeError::MissingItem(uap::TIME_OF_DAY))?,
        operational: operational.ok_or(Cat063DecodeError::MissingItem(
            uap::SENSOR_CONFIGURATION_STATUS,
        ))?,
    };
    Ok((record, cur))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoder() -> Cat063Encoder {
        Cat063Encoder::new(0)
    }

    /// A single operational sensor at time=0 encodes to the exact reference
    /// byte sequence. This is the byte-level ground truth Wayfinder's CAT063
    /// decoder must match.
    ///
    /// Expected:
    /// - [0x3F]           CAT=63
    /// - [0x00, 0x0A]     LEN=10
    /// - [0xE0]           FSPEC: FRN1+FRN2+FRN3 present
    /// - [0x00, 0x01]     I063/010: SAC=0, SIC=1
    /// - [0x00, 0x00, 0x00] I063/030: time=0
    /// - [0x00]           I063/060: NOGO=00 (operational)
    #[test]
    fn single_operational_sensor_matches_reference_dump() {
        let block = encoder().encode(0.0, &[(1, true)]);
        assert_eq!(
            block,
            vec![0x3F, 0x00, 0x0A, 0xE0, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]
        );
    }

    /// A single degraded sensor sets the NOGO bits of I063/060.
    ///
    /// Expected last byte: 0x40 (NOGO=01).
    #[test]
    fn single_degraded_sensor_matches_reference_dump() {
        let block = encoder().encode(0.0, &[(1, false)]);
        assert_eq!(
            block,
            vec![0x3F, 0x00, 0x0A, 0xE0, 0x00, 0x01, 0x00, 0x00, 0x00, 0x40]
        );
    }

    /// Two sensors (one operational, one degraded) in one block.
    ///
    /// Expected:
    /// - [0x3F, 0x00, 0x11]  CAT=63, LEN=17 (3 + 7 + 7)
    /// - record1: SIC=1 operational
    /// - record2: SIC=2 degraded
    #[test]
    fn two_sensors_in_one_block_matches_reference_dump() {
        let block = encoder().encode(0.0, &[(1, true), (2, false)]);
        assert_eq!(
            block,
            vec![
                0x3F, 0x00, 0x11, // header
                0xE0, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, // sensor 1 operational
                0xE0, 0x00, 0x02, 0x00, 0x00, 0x00, 0x40, // sensor 2 degraded
            ]
        );
    }

    /// The time of day is encoded in 1/128-second ticks (same as I062/070).
    ///
    /// Record layout: [FSPEC(1)][I063/010 SAC,SIC(2)][I063/030 tod(3)][I063/060(1)]
    /// Block layout:  [CAT(1)][LEN(2)][record…]
    /// Time bytes: block[6..9]
    #[test]
    fn time_of_day_is_encoded_in_128th_seconds() {
        // 01:00:00 = 3600 s → 3600 * 128 = 460_800 = 0x07_0800
        let block = encoder().encode(3600.0, &[(1, true)]);
        // block[0]=CAT, [1..3]=LEN, [3]=FSPEC, [4..6]=I063/010, [6..9]=I063/030
        assert_eq!(&block[6..9], &[0x07, 0x08, 0x00]);
    }

    /// Encoding then decoding recovers every field (round-trip).
    #[test]
    fn round_trip_recovers_all_fields() {
        let sensors = &[(1u8, true), (2u8, false), (3u8, true)];
        let block = encoder().encode(3600.0, sensors);
        let records = decode_sensor_block(&block).expect("decodes");
        assert_eq!(records.len(), 3);

        assert_eq!(records[0].source, DataSourceId::new(0, 1));
        assert!(records[0].operational);
        assert!((records[0].time_of_day_secs - 3600.0).abs() < TIME_LSB_SECONDS);

        assert_eq!(records[1].source, DataSourceId::new(0, 2));
        assert!(!records[1].operational);

        assert_eq!(records[2].source, DataSourceId::new(0, 3));
        assert!(records[2].operational);
    }

    /// An empty sensor list produces a valid three-byte header-only block.
    #[test]
    fn empty_sensor_list_produces_minimal_block() {
        let block = encoder().encode(0.0, &[]);
        assert_eq!(block, vec![0x3F, 0x00, 0x03]);
        assert_eq!(decode_sensor_block(&block).unwrap(), vec![]);
    }

    /// A wrong leading category byte is rejected without panic.
    #[test]
    fn wrong_category_is_rejected() {
        let mut block = encoder().encode(0.0, &[(1, true)]);
        block[0] = 0x3E; // CAT062
        assert_eq!(
            decode_sensor_block(&block),
            Err(Cat063DecodeError::WrongCategory(0x3E))
        );
    }

    /// Truncated input at any prefix never panics; it returns an error.
    #[test]
    fn truncated_input_is_safe() {
        let block = encoder().encode(0.0, &[(1, true), (2, false)]);
        for cut in 0..block.len() {
            let _ = decode_sensor_block(&block[..cut]);
        }
    }
}
