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
    /// I063/RE — Reserved Expansion Field (explicit length). Firefly carries the
    /// per-source failure reason here (ADR 0033).
    pub const RESERVED_EXPANSION: u8 = 13;
}

/// The I063/RE Reserved Expansion Field layout Firefly emits for a degraded
/// sensor (ADR 0033). The RE field is **self-delimiting** via its leading length
/// octet, so a decoder that does not understand it can still skip it (the
/// forward-compatibility contract, ICD §3):
///
/// ```text
/// [LEN = 0x03] [SUBFIELD_SPEC = 0x80] [SRC-REASON]
/// ```
///
/// - `LEN` counts the whole field including itself (3 octets).
/// - `SUBFIELD_SPEC` is a presence octet in the RE's own sub-field space: bit 8
///   (`0x80`) marks the `SRC-REASON` sub-field present; bit 1 is FX (clear).
/// - `SRC-REASON` is the [`SensorReason`] code (1 octet).
const RE_LEN: u8 = 0x03;
const RE_SUBFIELD_SPEC: u8 = 0x80;
/// Bit marking the `SRC-REASON` sub-field present in the RE sub-field spec octet.
const RE_SRC_REASON_BIT: u8 = 0x80;

/// Why a sensor is currently degraded — the per-source failure reason carried in
/// the CAT063 I063/RE Reserved Expansion Field (ADR 0033).
///
/// It lets the operator tell **unreachable** (network/firewall — credentials, if
/// any, are fine) from **auth** (bad or missing credentials, HTTP 401/403) from
/// **rate-limited** (throttled, HTTP 429), instead of guessing why a source went
/// dark and blindly re-typing credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SensorReason {
    /// No failure recorded — the sensor is, or was last, healthy. **Not** encoded
    /// on the wire (an operational sensor carries no RE field).
    #[default]
    Ok,
    /// The source could not be reached: DNS/connect/timeout, or a 5xx server
    /// error. Credentials are not the problem.
    Unreachable,
    /// Authentication/authorisation failed (HTTP 401/403) — bad or missing
    /// credentials.
    Auth,
    /// The source rate-limited the request (HTTP 429).
    RateLimited,
}

impl SensorReason {
    /// The `SRC-REASON` code carried in the I063/RE sub-field. `Ok` is `0` and is
    /// never actually put on the wire (it means "no RE field").
    pub fn code(self) -> u8 {
        match self {
            SensorReason::Ok => 0,
            SensorReason::Unreachable => 1,
            SensorReason::Auth => 2,
            SensorReason::RateLimited => 3,
        }
    }

    /// Inverse of [`code`](Self::code): decode a `SRC-REASON` octet. An unknown
    /// non-zero code maps to `Unreachable` (a conservative "degraded for some
    /// reason") rather than being rejected — forward tolerance for a future code.
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => SensorReason::Ok,
            2 => SensorReason::Auth,
            3 => SensorReason::RateLimited,
            _ => SensorReason::Unreachable,
        }
    }
}

/// One sensor's status to encode into a CAT063 record: which sensor (`sic`),
/// whether it is operational, and — when degraded — why ([`SensorReason`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorReport {
    /// I063/050 SIC of this sensor (the SAC is the encoder's `sensor_sac`).
    pub sic: u8,
    /// I063/060 CON: `true` = operational, `false` = degraded.
    pub operational: bool,
    /// The failure reason, emitted in I063/RE **only** when degraded and not
    /// [`SensorReason::Ok`]. An operational sensor carries no RE field.
    pub reason: SensorReason,
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
    /// `sensors` is one [`SensorReport`] per sensor, in the order they should
    /// appear in the block. An empty slice produces a valid but empty block
    /// (three-byte header only).
    ///
    /// A **degraded** sensor with a known [`SensorReason`] (not
    /// [`SensorReason::Ok`]) additionally carries the reason in an I063/RE
    /// Reserved Expansion Field; an operational sensor carries no RE field
    /// (ADR 0033, additive — the record stays 9 octets in the common case).
    pub fn encode(&self, time_of_day_secs: f64, sensors: &[SensorReport]) -> Vec<u8> {
        let mut records_bytes: Vec<u8> = Vec::new();
        for report in sensors {
            let mut builder = RecordBuilder::new()
                .item(
                    uap::DATA_SOURCE_IDENTIFIER,
                    vec![self.data_source.sac, self.data_source.sic],
                )
                .item(uap::TIME_OF_MESSAGE, encode_time_of_day(time_of_day_secs))
                .item(uap::SENSOR_IDENTIFIER, vec![self.sensor_sac, report.sic])
                .item(
                    uap::SENSOR_CONFIGURATION_STATUS,
                    vec![encode_con(report.operational)],
                );
            // Attach the failure reason only for a degraded sensor with a known
            // reason: an operational (or reason-unknown) sensor stays a plain
            // 9-octet record, so the RE field is genuinely additive.
            if !report.operational && report.reason != SensorReason::Ok {
                builder = builder.item(
                    uap::RESERVED_EXPANSION,
                    vec![RE_LEN, RE_SUBFIELD_SPEC, report.reason.code()],
                );
            }
            records_bytes.extend_from_slice(&builder.finish());
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
    /// I063/RE `SRC-REASON` — the per-source failure reason when degraded
    /// (ADR 0033). [`SensorReason::Ok`] when no RE field is present.
    pub reason: SensorReason,
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
    /// The FSPEC's FX chain ran past [`crate::fspec::MAX_FSPEC_OCTETS`] —
    /// malformed (no CAT063 item lives that far into the UAP). Fuzzing
    /// regression, QW.2.
    FspecTooLong,
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
            Cat063DecodeError::FspecTooLong => {
                write!(f, "FSPEC FX chain exceeds the supported FRN space")
            }
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

    let (frns, consumed) =
        fspec::parse(&bytes[pos..end]).map_err(|_| Cat063DecodeError::FspecTooLong)?;
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
    // Absent RE field ⇒ no recorded reason ⇒ Ok.
    let mut reason = SensorReason::Ok;

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
            uap::RESERVED_EXPANSION => {
                // I063/RE is explicit-length: the first octet is the field length
                // (counting itself). Read it, then the sub-field spec octet, and
                // — if the SRC-REASON bit is set — the reason code. Any remaining
                // octets (a future sub-field) are skipped by length, so the RE
                // field never desynchronises the parse.
                let re_len = take(1)?[0] as usize;
                if re_len < 1 {
                    return Err(Cat063DecodeError::Truncated);
                }
                let body = take(re_len - 1)?;
                if !body.is_empty() && body[0] & RE_SRC_REASON_BIT != 0 && body.len() >= 2 {
                    reason = SensorReason::from_code(body[1]);
                }
            }
            // A non-self-delimiting item at an FRN we do not consume cannot be
            // skipped without its length, so surface it rather than mis-parse.
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
        reason,
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

    /// An operational sensor report (no RE field).
    fn ok(sic: u8) -> SensorReport {
        SensorReport {
            sic,
            operational: true,
            reason: SensorReason::Ok,
        }
    }

    /// A degraded sensor report with no known reason (no RE field).
    fn degraded(sic: u8) -> SensorReport {
        SensorReport {
            sic,
            operational: false,
            reason: SensorReason::Ok,
        }
    }

    /// A degraded sensor report carrying a failure reason (RE field emitted).
    fn degraded_reason(sic: u8, reason: SensorReason) -> SensorReport {
        SensorReport {
            sic,
            operational: false,
            reason,
        }
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
        let block = encoder().encode(0.0, &[ok(1)]);
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
        let block = encoder().encode(0.0, &[degraded(1)]);
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
        let block = encoder().encode(0.0, &[ok(1), degraded(2)]);
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
        let block = encoder().encode(3600.0, &[ok(1)]);
        // block[3]=FSPEC, [4..6]=I063/010, [6..9]=I063/030
        assert_eq!(&block[6..9], &[0x07, 0x08, 0x00]);
    }

    /// Encoding then decoding recovers every field (round-trip), including the
    /// separate SDPS (I063/010) and sensor (I063/050) identities.
    #[test]
    fn round_trip_recovers_all_fields() {
        let sensors = &[ok(1), degraded(2), ok(3)];
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

        // No RE field was emitted (reasons all Ok), so every reason decodes Ok.
        for r in &records {
            assert_eq!(r.reason, SensorReason::Ok);
        }
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
        let block = encoder().encode(0.0, &[ok(1)]);
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
        let mut block = encoder().encode(0.0, &[ok(1)]);
        block[0] = 0x3E; // CAT062
        assert_eq!(
            decode_sensor_block(&block),
            Err(Cat063DecodeError::WrongCategory(0x3E))
        );
    }

    /// Truncated input at any prefix never panics; it returns an error.
    #[test]
    fn truncated_input_is_safe() {
        let block = encoder().encode(0.0, &[ok(1), degraded(2)]);
        for cut in 0..block.len() {
            let _ = decode_sensor_block(&block[..cut]);
        }
    }

    /// A degraded sensor with a known reason appends the I063/RE field, exact
    /// bytes. FSPEC grows to two octets (FRN 1+3+4+5 + FX, then FRN 13); the RE
    /// field is `[LEN=3][SUBFIELD=0x80][SRC-REASON]`. This is the byte-level
    /// ground truth Wayfinder's RE decoder (H4) must match.
    #[test]
    fn degraded_sensor_with_reason_appends_re_field() {
        let block = encoder().encode(0.0, &[degraded_reason(1, SensorReason::Unreachable)]);
        assert_eq!(
            block,
            vec![
                0x3F, 0x00, 0x10, // CAT=63, LEN=16 (3 + 13)
                0xB9, 0x04, // FSPEC: FRN 1+3+4+5 (+FX) then FRN 13 (RE)
                0x19, 0x02, // I063/010 SDPS 25/2
                0x00, 0x00, 0x00, // I063/030 time=0
                0x00, 0x01, // I063/050 sensor 0/1
                0x40, // I063/060 CON=01 (degraded)
                0x03, 0x80, 0x01, // I063/RE: LEN=3, SUBFIELD=0x80, SRC-REASON=1 (unreachable)
            ]
        );
    }

    /// Each reason code round-trips through encode → decode for a degraded sensor.
    #[test]
    fn re_reason_round_trips() {
        for reason in [
            SensorReason::Unreachable,
            SensorReason::Auth,
            SensorReason::RateLimited,
        ] {
            let block = encoder().encode(0.0, &[degraded_reason(1, reason)]);
            let records = decode_sensor_block(&block).expect("decodes");
            assert_eq!(records.len(), 1);
            assert!(!records[0].operational);
            assert_eq!(records[0].reason, reason, "reason {reason:?} round-trips");
        }
    }

    /// An operational sensor never carries an RE field even if a reason is set on
    /// the report — the RE is gated on degradation, so operational records stay
    /// the plain 9-octet form.
    #[test]
    fn operational_sensor_never_emits_re() {
        let report = SensorReport {
            sic: 1,
            operational: true,
            reason: SensorReason::Auth, // should be ignored
        };
        let block = encoder().encode(0.0, &[report]);
        assert_eq!(block[2], 0x0C, "LEN=12, no RE field");
        assert_eq!(block[3], 0xB8, "single-octet FSPEC, FRN 13 not set");
        let records = decode_sensor_block(&block).expect("decodes");
        assert_eq!(records[0].reason, SensorReason::Ok);
    }

    /// A degraded sensor with reason Ok (reason unknown, e.g. a silent FLARM
    /// sensor) also carries no RE field — we do not fabricate a reason.
    #[test]
    fn degraded_without_reason_emits_no_re() {
        let block = encoder().encode(0.0, &[degraded(1)]);
        assert_eq!(block[2], 0x0C, "LEN=12, no RE field");
        assert_eq!(block[3], 0xB8);
    }

    /// An unknown non-zero SRC-REASON code decodes to `Unreachable` (forward
    /// tolerance) rather than being rejected.
    #[test]
    fn unknown_reason_code_maps_to_unreachable() {
        assert_eq!(SensorReason::from_code(9), SensorReason::Unreachable);
        assert_eq!(SensorReason::from_code(0), SensorReason::Ok);
    }

    /// A hostile record whose FSPEC chains FX octets past the supported FRN
    /// space is rejected, not panicked on — frozen fuzzing find (QW.2): the
    /// unbounded FRN arithmetic used to overflow `u8`. REQ: NFR-SAFE-002
    #[test]
    fn overlong_fspec_chain_is_rejected_not_panicked() {
        let mut block = vec![CATEGORY, 0x00, 63];
        block.extend_from_slice(&[0xFF; 60]);
        assert_eq!(
            decode_sensor_block(&block),
            Err(Cat063DecodeError::FspecTooLong)
        );
    }
}
