//! CAT019 **input** decode — Multilateration System Status Messages (FEP.5).
//!
//! The service companion of CAT020: a WAM/MLAT system announces its own
//! health on the same feed — a **start-of-update-cycle** marker, periodic
//! status and event-triggered status, carrying the system's NOGO state
//! (operational / degraded / not-usable). Firefly consumes these for the
//! same reason it consumes CAT034/CAT002 from a radar: **liveness without
//! traffic** — "empty sky" and "dead MLAT system" stay distinguishable on
//! the input side, feeding the CAT063 per-sensor supervision.
//!
//! ## Robustness (security-relevant input path)
//!
//! Same policy as the sibling decoders (charter §8): bounds-checked cursor,
//! truncated/corrupt records yield [`Cat019DecodeError`] (datagram dropped,
//! no panic), unknown/spare FRNs are hard errors instead of silent
//! mis-parses. Covered by the `cat019_decode` fuzz target.
//!
//! Items and layouts follow **EUROCONTROL SUR.ET1.ST05.2000-STD-18**
//! ("ASTERIX Category 019 — Multilateration System Status Messages").
//!
//! REQ: FR-IO-012

use firefly_core::Timestamp;

use crate::fspec;

/// The ASTERIX category number for multilateration system status messages.
const CATEGORY: u8 = 19;

/// I019/140 Time-of-Day LSB: 1/128 second.
const TIME_LSB_SECONDS: f64 = 1.0 / 128.0;
/// I019/550 — the 2-bit NOGO field sits in the top two bits.
const STATUS_NOGO_SHIFT: u8 = 6;
/// FX bit (lowest bit of an octet): "another octet follows".
const FX: u8 = 0x01;

/// The CAT019 UAP field reference numbers (FRN → data item), per
/// SUR.ET1.ST05.2000-STD-18.
mod uap {
    /// I019/010 — Data Source Identifier (SAC/SIC).
    pub const DATA_SOURCE_IDENTIFIER: u8 = 1;
    /// I019/000 — Message Type.
    pub const MESSAGE_TYPE: u8 = 2;
    /// I019/140 — Time of Day.
    pub const TIME_OF_DAY: u8 = 3;
    /// I019/550 — System Status (NOGO/OVL/TSV/TTF).
    pub const SYSTEM_STATUS: u8 = 4;
    /// I019/551 — Tracking Processor Detailed Status.
    pub const TRACKING_PROCESSOR_STATUS: u8 = 5;
    /// I019/552 — Remote Sensor Detailed Status (repetitive, 2 octets each).
    pub const REMOTE_SENSOR_STATUS: u8 = 6;
    /// I019/553 — Reference Transponder Detailed Status (FX chain).
    pub const REFERENCE_TRANSPONDER_STATUS: u8 = 7;
    /// I019/600 — Position of the MLT System Reference Point (WGS-84).
    pub const REFERENCE_POSITION: u8 = 8;
    /// I019/610 — Height of the MLT System Reference Point.
    pub const REFERENCE_HEIGHT: u8 = 9;
    /// I019/620 — WGS-84 Undulation.
    pub const UNDULATION: u8 = 10;
    /// RE — Reserved Expansion Field (explicit length). FRNs 11/12 are spare.
    pub const RESERVED_EXPANSION: u8 = 13;
    /// SP — Special Purpose Field (explicit length).
    pub const SPECIAL_PURPOSE: u8 = 14;
}

/// How a CAT019 data item is laid out on the wire.
enum ItemFormat {
    /// A fixed number of octets.
    Fixed(usize),
    /// An FX-chained octet chain.
    Extended,
    /// A 1-octet repetition factor `REP`, then `REP × n` octets.
    Repetitive(usize),
    /// A 1-octet length indicator giving the item's **total** length — RE/SP.
    Explicit,
}

/// The wire layout of each CAT019 UAP item. `None` for the spare FRNs
/// (11/12) and everything past the UAP.
fn item_format(frn: u8) -> Option<ItemFormat> {
    use uap::*;
    Some(match frn {
        DATA_SOURCE_IDENTIFIER => ItemFormat::Fixed(2),
        MESSAGE_TYPE => ItemFormat::Fixed(1),
        TIME_OF_DAY => ItemFormat::Fixed(3),
        SYSTEM_STATUS => ItemFormat::Fixed(1),
        TRACKING_PROCESSOR_STATUS => ItemFormat::Fixed(1),
        REMOTE_SENSOR_STATUS => ItemFormat::Repetitive(2),
        REFERENCE_TRANSPONDER_STATUS => ItemFormat::Extended,
        REFERENCE_POSITION => ItemFormat::Fixed(8),
        REFERENCE_HEIGHT => ItemFormat::Fixed(2),
        UNDULATION => ItemFormat::Fixed(1),
        RESERVED_EXPANSION => ItemFormat::Explicit,
        SPECIAL_PURPOSE => ItemFormat::Explicit,
        _ => return None, // 11/12 spare
    })
}

/// What kind of status message a CAT019 record carries (I019/000).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MlatStatusType {
    /// Type 1 — start of update cycle: the MLAT twin of a radar's north
    /// marker, one per update cycle.
    StartOfUpdateCycle,
    /// Type 2 — periodic status message.
    Periodic,
    /// Type 3 — event-triggered status message.
    Event,
    /// A type this decoder does not interpret — tolerated (forward
    /// compatibility), carried through for logging/counting.
    Other(u8),
}

impl MlatStatusType {
    fn from_code(code: u8) -> Self {
        match code {
            1 => MlatStatusType::StartOfUpdateCycle,
            2 => MlatStatusType::Periodic,
            3 => MlatStatusType::Event,
            other => MlatStatusType::Other(other),
        }
    }
}

/// One decoded CAT019 status message — the neutral product of
/// [`decode_mlat_status`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodedMlatStatus {
    /// I019/010 — System Area Code of the reporting MLAT system.
    pub sac: u8,
    /// I019/010 — System Identification Code of the reporting MLAT system.
    pub sic: u8,
    /// I019/000 — what this message announces.
    pub message_type: MlatStatusType,
    /// I019/140 — time of day (1/128 s since UTC midnight), if present.
    pub time: Option<Timestamp>,
    /// I019/550 NOGO — `Some(true)` when the system declares itself
    /// operational (NOGO = 0), `Some(false)` on degraded/NOGO/undefined,
    /// `None` when I019/550 is absent (no claim either way).
    pub operational: Option<bool>,
}

/// Errors that can occur while decoding a CAT019 data block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cat019DecodeError {
    /// The input ended before a complete block/record/item could be read.
    Truncated,
    /// The first octet was not [`CATEGORY`] (19).
    WrongCategory(u8),
    /// The `LEN` field did not match the actual input length.
    LengthMismatch {
        /// Length declared by the block's LEN field.
        declared: usize,
        /// Actual datagram length.
        actual: usize,
    },
    /// The FSPEC marked an FRN present that this decoder cannot length (and
    /// so cannot safely skip): a spare position or past the UAP.
    UnknownItem(u8),
    /// A record's FSPEC was missing a required item (I019/010 or I019/000).
    MissingItem(u8),
    /// The FSPEC's FX chain ran past [`fspec::MAX_FSPEC_OCTETS`] — malformed.
    FspecTooLong,
}

impl std::fmt::Display for Cat019DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cat019DecodeError::Truncated => write!(f, "input ended before a complete item"),
            Cat019DecodeError::WrongCategory(cat) => write!(f, "expected CAT 19, got CAT {cat}"),
            Cat019DecodeError::LengthMismatch { declared, actual } => write!(
                f,
                "LEN field says {declared} bytes, but input is {actual} bytes"
            ),
            Cat019DecodeError::UnknownItem(frn) => {
                write!(f, "FSPEC marks unknown/unskippable FRN {frn} present")
            }
            Cat019DecodeError::MissingItem(frn) => {
                write!(f, "record is missing required FRN {frn}")
            }
            Cat019DecodeError::FspecTooLong => {
                write!(f, "FSPEC FX chain exceeds the supported FRN space")
            }
        }
    }
}

impl std::error::Error for Cat019DecodeError {}

/// Decode a CAT019 data block (`[CAT=19][LEN][record…]`) into one
/// [`DecodedMlatStatus`] per record.
///
/// Returns [`Cat019DecodeError`] (and decodes nothing) on a wrong category,
/// length mismatch, truncation or an unknown present item — the caller drops
/// the datagram and keeps listening. Never panics on input.
pub fn decode_mlat_status(bytes: &[u8]) -> Result<Vec<DecodedMlatStatus>, Cat019DecodeError> {
    if bytes.len() < 3 {
        return Err(Cat019DecodeError::Truncated);
    }
    if bytes[0] != CATEGORY {
        return Err(Cat019DecodeError::WrongCategory(bytes[0]));
    }
    let declared = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    if declared != bytes.len() {
        return Err(Cat019DecodeError::LengthMismatch {
            declared,
            actual: bytes.len(),
        });
    }

    let mut cursor = Cursor::new(&bytes[3..]);
    let mut messages = Vec::new();
    while cursor.remaining() > 0 {
        messages.push(decode_record(&mut cursor)?);
    }
    Ok(messages)
}

/// One record: FSPEC, then the present items in ascending-FRN (UAP) order.
fn decode_record(cursor: &mut Cursor) -> Result<DecodedMlatStatus, Cat019DecodeError> {
    let frns = cursor.take_fspec()?;

    let mut sac_sic = None;
    let mut message_type = None;
    let mut time = None;
    let mut operational = None;

    for frn in frns {
        let format = item_format(frn).ok_or(Cat019DecodeError::UnknownItem(frn))?;
        let bytes = match format {
            ItemFormat::Fixed(n) => cursor.take(n)?,
            ItemFormat::Extended => cursor.take_extended()?,
            ItemFormat::Repetitive(per) => cursor.take_repetitive(per)?,
            ItemFormat::Explicit => cursor.take_explicit()?,
        };
        match frn {
            uap::DATA_SOURCE_IDENTIFIER => sac_sic = Some((bytes[0], bytes[1])),
            uap::MESSAGE_TYPE => message_type = Some(MlatStatusType::from_code(bytes[0])),
            uap::TIME_OF_DAY => {
                let ticks = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
                time = Some(Timestamp(ticks as f64 * TIME_LSB_SECONDS));
            }
            uap::SYSTEM_STATUS => {
                // NOGO (2 bits): 0 = operational, 1 = degraded, 2 = NOGO,
                // 3 = undefined — only 0 is an operational claim.
                operational = Some((bytes[0] >> STATUS_NOGO_SHIFT) & 0b11 == 0);
            }
            _ => {} // present but not status-relevant — length already consumed
        }
    }

    let (sac, sic) = sac_sic.ok_or(Cat019DecodeError::MissingItem(uap::DATA_SOURCE_IDENTIFIER))?;
    let message_type = message_type.ok_or(Cat019DecodeError::MissingItem(uap::MESSAGE_TYPE))?;

    Ok(DecodedMlatStatus {
        sac,
        sic,
        message_type,
        time,
        operational,
    })
}

/// A bounds-checked read cursor over a block's record bytes; every `take*`
/// returns [`Cat019DecodeError::Truncated`] rather than panicking.
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

    fn take(&mut self, n: usize) -> Result<&'a [u8], Cat019DecodeError> {
        if self.remaining() < n {
            return Err(Cat019DecodeError::Truncated);
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn take_fspec(&mut self) -> Result<std::collections::BTreeSet<u8>, Cat019DecodeError> {
        let slice = &self.bytes[self.pos..];
        let (frns, consumed) = fspec::parse(slice).map_err(|_| Cat019DecodeError::FspecTooLong)?;
        if consumed == 0 || slice[consumed - 1] & FX != 0 {
            return Err(Cat019DecodeError::Truncated);
        }
        self.pos += consumed;
        Ok(frns)
    }

    fn take_extended(&mut self) -> Result<&'a [u8], Cat019DecodeError> {
        let start = self.pos;
        loop {
            let octet = self.take(1)?[0];
            if octet & FX == 0 {
                break;
            }
        }
        Ok(&self.bytes[start..self.pos])
    }

    fn take_repetitive(&mut self, per: usize) -> Result<&'a [u8], Cat019DecodeError> {
        let start = self.pos;
        let rep = self.take(1)?[0] as usize;
        self.take(rep * per)?;
        Ok(&self.bytes[start..self.pos])
    }

    fn take_explicit(&mut self) -> Result<&'a [u8], Cat019DecodeError> {
        let start = self.pos;
        let total = self.take(1)?[0] as usize;
        if total == 0 {
            return Err(Cat019DecodeError::Truncated);
        }
        self.take(total - 1)?;
        Ok(&self.bytes[start..self.pos])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrap record bytes in a CAT019 block envelope.
    fn block(records: &[u8]) -> Vec<u8> {
        let mut out = vec![CATEGORY, 0x00, (3 + records.len()) as u8];
        out.extend_from_slice(records);
        out
    }

    /// A hand-built periodic status message decodes field-exactly — the
    /// CAT019 reference vector. FSPEC {1,2,3,4} = 0xF0; operational.
    /// REQ: FR-IO-012
    #[test]
    fn periodic_status_matches_reference_vector() {
        let bytes = block(&[
            0xF0, // FSPEC {1,2,3,4}
            0x19, 0x0A, // I019/010 SAC=25 SIC=10
            0x02, // I019/000 periodic
            0x07, 0x08, 0x00, // I019/140 time 3600 s
            0x00, // I019/550 NOGO=0 (operational)
        ]);
        let messages = decode_mlat_status(&bytes).expect("decodes");
        assert_eq!(
            messages,
            vec![DecodedMlatStatus {
                sac: 25,
                sic: 10,
                message_type: MlatStatusType::Periodic,
                time: Some(Timestamp(3600.0)),
                operational: Some(true),
            }]
        );
    }

    /// NOGO ≠ 0 (degraded/NOGO/undefined) is NOT an operational claim; an
    /// absent I019/550 makes no claim either way. Start-of-update-cycle and
    /// unknown types map correctly. REQ: FR-IO-012
    #[test]
    fn nogo_and_message_types_decode() {
        for (nogo, expect) in [(0x40u8, false), (0x80, false), (0xC0, false)] {
            let bytes = block(&[0xF0, 0x19, 0x0A, 0x01, 0x00, 0x00, 0x80, nogo]);
            let m = decode_mlat_status(&bytes).expect("decodes")[0];
            assert_eq!(m.message_type, MlatStatusType::StartOfUpdateCycle);
            assert_eq!(m.operational, Some(expect));
        }

        let absent = block(&[0xC0, 0x19, 0x0A, 0x09]);
        let m = decode_mlat_status(&absent).expect("decodes")[0];
        assert_eq!(m.message_type, MlatStatusType::Other(9));
        assert_eq!(m.operational, None);
    }

    /// Unused items (repetitive remote-sensor status, FX chains, reference
    /// point, RE/SP) are skipped length-correctly: a following record still
    /// decodes. REQ: FR-IO-012
    #[test]
    fn skips_unused_items_length_correctly() {
        let bytes = block(&[
            // Record 1: {1,2,5,6,7} octet1: 1=0x80,2=0x40,5=0x08,6=0x04,
            // 7=0x02 → 0xCE; + {8,9,10} octet2: 8=0x80,9=0x40,10=0x20 → 0xE0.
            0xCF, 0xE0, // FSPEC {1,2,5,6,7,8,9,10}
            0x19, 0x0A, // I019/010
            0x02, // I019/000
            0x11, // I019/551 (skipped)
            0x02, 0xAA, 0x01, 0xBB, 0x02, // I019/552 REP=2 × 2 octets (skipped)
            0x81, 0x00, // I019/553 FX chain of 2 (skipped)
            0x10, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, // I019/600 (skipped)
            0x01, 0xF4, // I019/610 (skipped)
            0x2D, // I019/620 (skipped)
            // Record 2: minimal periodic message.
            0xC0, 0x19, 0x0A, 0x02,
        ]);
        let messages = decode_mlat_status(&bytes).expect("decodes");
        assert_eq!(messages.len(), 2, "skip kept the stream in sync");
        assert_eq!(messages[1].message_type, MlatStatusType::Periodic);
    }

    /// Spare FRNs (11/12) are hard errors; missing mandatory items too.
    /// REQ: FR-IO-012, NFR-SAFE-002
    #[test]
    fn spare_frns_and_missing_items_are_rejected() {
        // FRN 11 lives in octet 2 (8..14) at bit 0x10.
        let spare = block(&[0xC1, 0x10, 0x19, 0x0A, 0x02]);
        assert_eq!(
            decode_mlat_status(&spare),
            Err(Cat019DecodeError::UnknownItem(11))
        );

        let no_type = block(&[0x80, 0x19, 0x0A]);
        assert_eq!(
            decode_mlat_status(&no_type),
            Err(Cat019DecodeError::MissingItem(2))
        );
        let no_identity = block(&[0x40, 0x02]);
        assert_eq!(
            decode_mlat_status(&no_identity),
            Err(Cat019DecodeError::MissingItem(1))
        );
    }

    /// Wrong category / length lies / truncations / hostile FSPEC chains are
    /// rejected without panic. REQ: FR-IO-012, NFR-SAFE-002
    #[test]
    fn malformed_input_is_rejected_not_panicked() {
        assert_eq!(
            decode_mlat_status(&[20, 0x00, 0x03]),
            Err(Cat019DecodeError::WrongCategory(20))
        );
        assert_eq!(
            decode_mlat_status(&[CATEGORY, 0xFF, 0xFF, 0x00]),
            Err(Cat019DecodeError::LengthMismatch {
                declared: 0xFFFF,
                actual: 4
            })
        );

        let bytes = block(&[0xF0, 0x19, 0x0A, 0x02, 0x07, 0x08, 0x00, 0x00]);
        for cut in 0..bytes.len() {
            let mut shortened = bytes[..cut].to_vec();
            if shortened.len() >= 3 {
                shortened[1] = 0;
                shortened[2] = shortened.len() as u8;
            }
            let _ = decode_mlat_status(&shortened);
        }

        let mut hostile = vec![CATEGORY, 0x00, 63];
        hostile.extend_from_slice(&[0xFF; 60]);
        assert_eq!(
            decode_mlat_status(&hostile),
            Err(Cat019DecodeError::FspecTooLong)
        );
    }
}
