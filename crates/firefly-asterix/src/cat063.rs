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
//! Each CAT063 **data block** carries one record per sensor. A record follows
//! the **standard EUROCONTROL CAT063 UAP** (ADR 0032 — UAP standardisation,
//! mirroring the CAT062 UAP fix of ADR 0015):
//!
//! | FRN | Item     | Meaning                                        |
//! |-----|----------|------------------------------------------------|
//! | 1   | I063/010 | Data Source Identifier — the **SDPS** (SAC/SIC)|
//! | 3   | I063/030 | Time of Message (1/128 s since UTC midnight)   |
//! | 4   | I063/050 | Sensor Identifier — the **sensor** (SAC/SIC)   |
//! | 5   | I063/060 | Sensor Configuration and Status (CON + GO/NOGO)|
//!
//! The full UAP also defines I063/015 (FRN2, Service Identification), the radar
//! bias items (I063/070/080/081/090/091/092, FRN6–11) and the Reserved
//! Expansion / Special Purpose fields at high FRNs; Firefly transmits only the
//! subset above (transmitting a subset of UAP items is standard-conformant — the
//! FSPEC marks which items are present). The standard FSPEC for our subset is
//! `0xB8` (FRN 1 + 3 + 4 + 5, FX clear).
//!
//! **I063/010 vs I063/050 (the key correctness point).** In CAT063, I063/010
//! identifies **who reports** (the SDPS that produces the message — the same
//! SAC/SIC Firefly stamps into I062/010 and I065/010), while I063/050 identifies
//! **which sensor** the record is about. Earlier Firefly editions conflated the
//! two by putting the sensor identity into I063/010 with a compacted, non-standard
//! FRN numbering; ADR 0032 corrects both.
//!
//! **I063/060 first octet** (`CON`, bits 8–7; EUROCONTROL values): `0` =
//! operational, `1` = degraded, `2` = initialisation, `3` = not currently
//! connected. Bits 6–2 are the PSR/SSR/MDS/ADS/MLT GO/NOGO flags; bit 1 is FX
//! (the item is variable-length). Firefly currently emits only `operational`
//! (CON=0, `0x00`) and `degraded` (CON=1, `0x40`) with FX clear.
//!
//! Items and bit layouts are taken from **EUROCONTROL SUR.ET1.ST05.2000-STD-04-01**
//! ("ASTERIX Category 063 — Sensor Status Messages") and verified against the
//! CroatiaControl ASTERIX CAT063 reference definition (ed. 1.3).
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

/// I063/060 `CON` connection-status field (Bits 8–7 of the first status octet),
/// with the **standard EUROCONTROL values**:
///
/// - `0` (`0x00`) = **operational** — all subsystems nominal.
/// - `1` (`0x40`) = **degraded** — sensor alive but not fully contributing.
/// - `2` (`0x80`) = **initialisation**.
/// - `3` (`0xC0`) = **not currently connected**.
///
/// Firefly emits `operational`/`degraded` today; `INITIALISATION`/`NOT_CONNECTED`
/// are defined for completeness and future use (e.g. a per-source `unreachable`
/// state mapping onto `not connected`, ADR 0033).
const CON_OPERATIONAL: u8 = 0x00;
const CON_DEGRADED: u8 = 0x40;
#[allow(dead_code)]
const CON_INITIALISATION: u8 = 0x80;
#[allow(dead_code)]
const CON_NOT_CONNECTED: u8 = 0xC0;
/// Mask selecting the 2-bit `CON` field.
const CON_MASK: u8 = 0xC0;
/// FX bit of the (variable-length) I063/060 item.
const I063_060_FX: u8 = 0x01;

/// UAP field reference numbers for the items we emit — the **standard** CAT063
/// UAP positions (ADR 0032).
mod uap {
    /// I063/010 — Data Source Identifier (SAC/SIC of the **SDPS**).
    pub const DATA_SOURCE_IDENTIFIER: u8 = 1;
    /// I063/030 — Time of Message.
    pub const TIME_OF_MESSAGE: u8 = 3;
    /// I063/050 — Sensor Identifier (SAC/SIC of the **sensor** the record is about).
    pub const SENSOR_IDENTIFIER: u8 = 4;
    /// I063/060 — Sensor Configuration and Status.
    pub const SENSOR_CONFIGURATION_STATUS: u8 = 5;
}

/// Encodes Sensor Status records into a CAT063 data block.
///
/// The encoder holds the **SDPS** data-source identity (stamped into I063/010 of
/// every record — the same SAC/SIC Firefly uses for I062/010 and I065/010) and
/// the **sensor SAC** shared by all local sensors (the per-record SIC
/// distinguishes them in I063/050). The SDPS sends one record per sensor in a
/// single block, acting as proxy for each radar.
#[derive(Debug, Clone, Copy)]
pub struct Cat063Encoder {
    data_source: DataSourceId,
    sensor_sac: u8,
}

impl Cat063Encoder {
    /// An encoder reporting on behalf of the SDPS `data_source` (I063/010) and
    /// stamping `sensor_sac` as the SAC of every sensor identity (I063/050). For
    /// Firefly deployments the sensor SAC is `0`; the SDPS identity comes from
    /// `FIREFLY_CAT062_SAC`/`_SIC` (default 25/2).
    pub fn new(data_source: DataSourceId, sensor_sac: u8) -> Self {
        Self {
            data_source,
            sensor_sac,
        }
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
            let record = RecordBuilder::new()
                .item(
                    uap::DATA_SOURCE_IDENTIFIER,
                    vec![self.data_source.sac, self.data_source.sic],
                )
                .item(uap::TIME_OF_MESSAGE, encode_time_of_day(time_of_day_secs))
                .item(uap::SENSOR_IDENTIFIER, vec![self.sensor_sac, sic])
                .item(
                    uap::SENSOR_CONFIGURATION_STATUS,
                    vec![encode_con(operational)],
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

/// I063/060 first octet — the `CON` field encodes operational vs. degraded, with
/// the GO/NOGO bits and FX left clear (single-octet, unextended item).
fn encode_con(operational: bool) -> u8 {
    if operational {
        CON_OPERATIONAL
    } else {
        CON_DEGRADED
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
    /// I063/010 — the **SDPS** data source that produced this record (SAC/SIC).
    pub data_source: DataSourceId,
    /// I063/030 — time of day (wraps at midnight).
    pub time_of_day_secs: f64,
    /// I063/050 — the **sensor** this record is about (SAC/SIC).
    pub sensor: DataSourceId,
    /// I063/060 `CON` — `true` when the sensor reports itself operational (CON=0).
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

    let mut data_source: Option<DataSourceId> = None;
    let mut time_of_day_secs: Option<f64> = None;
    let mut sensor: Option<DataSourceId> = None;
    let mut operational: Option<bool> = None;

    for frn in frns {
        match frn {
            uap::DATA_SOURCE_IDENTIFIER => {
                let b = take(2)?;
                data_source = Some(DataSourceId::new(b[0], b[1]));
            }
            uap::TIME_OF_MESSAGE => {
                let b = take(3)?;
                let ticks = u32::from_be_bytes([0, b[0], b[1], b[2]]);
                time_of_day_secs = Some(ticks as f64 * TIME_LSB_SECONDS);
            }
            uap::SENSOR_IDENTIFIER => {
                let b = take(2)?;
                sensor = Some(DataSourceId::new(b[0], b[1]));
            }
            uap::SENSOR_CONFIGURATION_STATUS => {
                // I063/060 is variable-length (FX): read the first octet, then
                // skip any extension octets while the FX bit stays set, so an
                // extended status item never desynchronises the record parse.
                let first = take(1)?[0];
                operational = Some(first & CON_MASK == CON_OPERATIONAL);
                let mut octet = first;
                while octet & I063_060_FX != 0 {
                    octet = take(1)?[0];
                }
            }
            // Forward tolerance: an item at an FRN we do not consume (a future
            // additive item, or the RE/SP fields) must not fail the block. But a
            // non-self-delimiting unknown item cannot be skipped without its
            // length, so we surface it rather than mis-parse silently. RE/SP are
            // handled by the consumer (Wayfinder) which reads their length octet.
            other => return Err(Cat063DecodeError::UnknownFrn(other)),
        }
    }

    let record = DecodedSensorStatus {
        data_source: data_source
            .ok_or(Cat063DecodeError::MissingItem(uap::DATA_SOURCE_IDENTIFIER))?,
        time_of_day_secs: time_of_day_secs
            .ok_or(Cat063DecodeError::MissingItem(uap::TIME_OF_MESSAGE))?,
        sensor: sensor.ok_or(Cat063DecodeError::MissingItem(uap::SENSOR_IDENTIFIER))?,
        operational: operational.ok_or(Cat063DecodeError::MissingItem(
            uap::SENSOR_CONFIGURATION_STATUS,
        ))?,
    };
    Ok((record, cur))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference encoder: SDPS SAC/SIC = 25/2 (Firefly's default I062/010
    /// identity), sensor SAC = 0.
    fn encoder() -> Cat063Encoder {
        Cat063Encoder::new(DataSourceId::new(25, 2), 0)
    }

    /// A single operational sensor at time=0 encodes to the exact reference
    /// byte sequence. This is the byte-level ground truth Wayfinder's CAT063
    /// decoder must match — the **standard** CAT063 UAP (ADR 0032).
    ///
    /// Expected:
    /// - [0x3F]              CAT=63
    /// - [0x00, 0x0C]        LEN=12
    /// - [0xB8]              FSPEC: FRN 1 + 3 + 4 + 5 present (I063/010/030/050/060)
    /// - [0x19, 0x02]        I063/010: SDPS SAC=25, SIC=2
    /// - [0x00, 0x00, 0x00]  I063/030: time=0
    /// - [0x00, 0x01]        I063/050: sensor SAC=0, SIC=1
    /// - [0x00]              I063/060: CON=00 (operational)
    #[test]
    fn single_operational_sensor_matches_reference_dump() {
        let block = encoder().encode(0.0, &[(1, true)]);
        assert_eq!(
            block,
            vec![0x3F, 0x00, 0x0C, 0xB8, 0x19, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00]
        );
    }

    /// A single degraded sensor sets the CON field of I063/060 to `01`.
    ///
    /// Expected last byte: 0x40 (CON=01, degraded).
    #[test]
    fn single_degraded_sensor_matches_reference_dump() {
        let block = encoder().encode(0.0, &[(1, false)]);
        assert_eq!(
            block,
            vec![0x3F, 0x00, 0x0C, 0xB8, 0x19, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x40]
        );
    }

    /// Two sensors (one operational, one degraded) in one block. Each record is
    /// 9 octets (FSPEC 1 + I063/010 2 + I063/030 3 + I063/050 2 + I063/060 1).
    ///
    /// Expected:
    /// - [0x3F, 0x00, 0x15]  CAT=63, LEN=21 (3 + 9 + 9)
    /// - record1: sensor SIC=1 operational
    /// - record2: sensor SIC=2 degraded
    #[test]
    fn two_sensors_in_one_block_matches_reference_dump() {
        let block = encoder().encode(0.0, &[(1, true), (2, false)]);
        assert_eq!(
            block,
            vec![
                0x3F, 0x00, 0x15, // header, LEN=21
                0xB8, 0x19, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, // sensor 1 operational
                0xB8, 0x19, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x40, // sensor 2 degraded
            ]
        );
    }

    /// The time of day is encoded in 1/128-second ticks (same as I062/070).
    ///
    /// Record layout: [FSPEC(1)][I063/010(2)][I063/030 tod(3)][I063/050(2)][I063/060(1)]
    /// Block layout:  [CAT(1)][LEN(2)][record…]
    /// Time bytes: block[6..9]
    #[test]
    fn time_of_day_is_encoded_in_128th_seconds() {
        // 01:00:00 = 3600 s → 3600 * 128 = 460_800 = 0x07_0800
        let block = encoder().encode(3600.0, &[(1, true)]);
        // block[3]=FSPEC, [4..6]=I063/010, [6..9]=I063/030
        assert_eq!(&block[6..9], &[0x07, 0x08, 0x00]);
    }

    /// Encoding then decoding recovers every field (round-trip), including the
    /// separate SDPS (I063/010) and sensor (I063/050) identities.
    #[test]
    fn round_trip_recovers_all_fields() {
        let sensors = &[(1u8, true), (2u8, false), (3u8, true)];
        let block = encoder().encode(3600.0, sensors);
        let records = decode_sensor_block(&block).expect("decodes");
        assert_eq!(records.len(), 3);

        // Every record carries the same SDPS identity (I063/010 = 25/2)…
        for r in &records {
            assert_eq!(r.data_source, DataSourceId::new(25, 2));
        }
        // …and its own sensor identity (I063/050, SAC 0 + the SIC).
        assert_eq!(records[0].sensor, DataSourceId::new(0, 1));
        assert!(records[0].operational);
        assert!((records[0].time_of_day_secs - 3600.0).abs() < TIME_LSB_SECONDS);

        assert_eq!(records[1].sensor, DataSourceId::new(0, 2));
        assert!(!records[1].operational);

        assert_eq!(records[2].sensor, DataSourceId::new(0, 3));
        assert!(records[2].operational);
    }

    /// An empty sensor list produces a valid three-byte header-only block.
    #[test]
    fn empty_sensor_list_produces_minimal_block() {
        let block = encoder().encode(0.0, &[]);
        assert_eq!(block, vec![0x3F, 0x00, 0x03]);
        assert_eq!(decode_sensor_block(&block).unwrap(), vec![]);
    }

    /// The FSPEC uses the standard UAP positions: FRN 1 + 3 + 4 + 5 → 0xB8, FX
    /// clear (a single FSPEC octet). Guards against a regression to the old
    /// non-standard compacted numbering (0xE0).
    #[test]
    fn fspec_uses_standard_uap_positions() {
        let block = encoder().encode(0.0, &[(1, true)]);
        assert_eq!(block[3], 0xB8, "FRN 1+3+4+5 present, FX clear");
    }

    /// A CON field with the FX bit set (a hypothetical extended I063/060) is
    /// parsed without desynchronising: the extension octet is skipped and the
    /// following record still decodes.
    #[test]
    fn extended_i063_060_is_skipped_via_fx() {
        // Hand-build a one-record block with an extended I063/060 (two octets:
        // 0x01 sets FX, 0x00 clears it) followed by nothing.
        let record = vec![
            0xB8, // FSPEC FRN 1+3+4+5
            0x19, 0x02, // I063/010
            0x00, 0x00, 0x00, // I063/030
            0x00, 0x07, // I063/050 sensor 0/7
            0x01, 0x00, // I063/060: first octet FX=1, extension octet FX=0
        ];
        let mut block = vec![0x3F, 0x00, (3 + record.len()) as u8];
        block.extend_from_slice(&record);
        let records = decode_sensor_block(&block).expect("decodes with extended I063/060");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].sensor, DataSourceId::new(0, 7));
        // CON bits of the first octet are 00 → operational.
        assert!(records[0].operational);
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
