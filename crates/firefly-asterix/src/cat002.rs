//! CAT002 **input** decode — legacy Monoradar Service Messages (FEP.4).
//!
//! CAT002 is the **predecessor** of CAT034: the service-message companion of
//! the legacy CAT001 target reports. A legacy radar head interleaves both on
//! the same feed — CAT002 carries the north marker / sector crossings and,
//! crucially, the **full time of day** (I002/030) that the CAT001 records
//! themselves lack (their I001/141 is truncated modulo 512 s). Decoding
//! CAT002 therefore serves the same two purposes as CAT034 (measured scan
//! period + liveness without traffic) **plus** it anchors the legacy plots'
//! timestamps.
//!
//! The product is the same [`DecodedServiceMessage`] the CAT034 decoder
//! yields, so the scan-period estimator and the sensor-health path are
//! format-agnostic. Message-type codes differ between the categories, so the
//! mapping is explicit here: CAT002 type 1 = north marker, 2 = sector
//! crossing; everything else (south marker, blind-zone filtering) decodes as
//! `Other` — tolerated, not interpreted.
//!
//! ## Robustness (security-relevant input path)
//!
//! Same policy as the sibling decoders (charter §8): bounds-checked cursor,
//! truncated/corrupt records yield [`Cat002DecodeError`] (datagram dropped,
//! no panic), unknown/spare FRNs and the never-used RFS indicator are hard
//! errors instead of silent mis-parses. Covered by the `cat002_decode` fuzz
//! target.
//!
//! Items and layouts follow **EUROCONTROL SUR.ET1.ST05.2000-STD-02b**
//! ("ASTERIX Category 002 — Transmission of Monoradar Service Messages").
//!
//! REQ: FR-IO-011

use firefly_core::Timestamp;

use crate::cat034::{DecodedServiceMessage, ServiceMessageType};
use crate::fspec;

/// The ASTERIX category number for legacy monoradar service messages.
const CATEGORY: u8 = 2;

/// I002/030 Time-of-Day LSB: 1/128 second (24-bit count since UTC midnight).
const TIME_LSB_SECONDS: f64 = 1.0 / 128.0;
/// I002/020 Sector Number LSB: 360/2⁸ degrees.
const SECTOR_LSB_DEGREES: f64 = 360.0 / 256.0;
/// I002/041 Antenna Rotation Period LSB: 1/128 second.
const ROTATION_LSB_SECONDS: f64 = 1.0 / 128.0;
/// FX bit (lowest bit of an octet): "another octet follows".
const FX: u8 = 0x01;

/// CAT002 message-type codes (I002/000) this decoder interprets.
const MSG_NORTH_MARKER: u8 = 1;
const MSG_SECTOR_CROSSING: u8 = 2;

/// The CAT002 UAP field reference numbers (FRN → data item), per
/// SUR.ET1.ST05.2000-STD-02b.
mod uap {
    /// I002/010 — Data Source Identifier (SAC/SIC).
    pub const DATA_SOURCE_IDENTIFIER: u8 = 1;
    /// I002/000 — Message Type.
    pub const MESSAGE_TYPE: u8 = 2;
    /// I002/020 — Sector Number.
    pub const SECTOR_NUMBER: u8 = 3;
    /// I002/030 — Time of Day.
    pub const TIME_OF_DAY: u8 = 4;
    /// I002/041 — Antenna Rotation Period.
    pub const ANTENNA_ROTATION_PERIOD: u8 = 5;
    /// I002/050 — Station Configuration Status (FX chain).
    pub const STATION_CONFIGURATION: u8 = 6;
    /// I002/060 — Station Processing Mode (FX chain).
    pub const STATION_PROCESSING_MODE: u8 = 7;
    /// I002/070 — Plot Count Values (repetitive, 2 octets per entry).
    pub const PLOT_COUNT_VALUES: u8 = 8;
    /// I002/100 — Dynamic Window.
    pub const DYNAMIC_WINDOW: u8 = 9;
    /// I002/090 — Collimation Error.
    pub const COLLIMATION_ERROR: u8 = 10;
    /// I002/080 — Warning/Error Conditions (FX chain).
    pub const WARNING_ERROR_CONDITIONS: u8 = 11;
    /// SP — Special Purpose Field (explicit length). FRN 12 is spare and
    /// FRN 14 the RFS indicator; both are unskippable → rejected.
    pub const SPECIAL_PURPOSE: u8 = 13;
}

/// How a CAT002 data item is laid out on the wire.
enum ItemFormat {
    /// A fixed number of octets.
    Fixed(usize),
    /// An FX-chained octet chain (1 octet, repeat while its low bit is set).
    Extended,
    /// A 1-octet repetition factor `REP`, then `REP × n` octets.
    Repetitive(usize),
    /// A 1-octet length indicator giving the item's **total** length — SP.
    Explicit,
}

/// The wire layout of each CAT002 UAP item. `None` for the spare FRN (12),
/// the RFS indicator (14) and everything past the UAP — present-but-
/// unskippable, reported as [`Cat002DecodeError::UnknownItem`].
fn item_format(frn: u8) -> Option<ItemFormat> {
    use uap::*;
    Some(match frn {
        DATA_SOURCE_IDENTIFIER => ItemFormat::Fixed(2),
        MESSAGE_TYPE => ItemFormat::Fixed(1),
        SECTOR_NUMBER => ItemFormat::Fixed(1),
        TIME_OF_DAY => ItemFormat::Fixed(3),
        ANTENNA_ROTATION_PERIOD => ItemFormat::Fixed(2),
        STATION_CONFIGURATION => ItemFormat::Extended,
        STATION_PROCESSING_MODE => ItemFormat::Extended,
        PLOT_COUNT_VALUES => ItemFormat::Repetitive(2),
        DYNAMIC_WINDOW => ItemFormat::Fixed(8),
        COLLIMATION_ERROR => ItemFormat::Fixed(2),
        WARNING_ERROR_CONDITIONS => ItemFormat::Extended,
        SPECIAL_PURPOSE => ItemFormat::Explicit,
        _ => return None, // 12 spare, 14 RFS
    })
}

/// Errors that can occur while decoding a CAT002 data block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cat002DecodeError {
    /// The input ended before a complete block/record/item could be read.
    Truncated,
    /// The first octet was not [`CATEGORY`] (2).
    WrongCategory(u8),
    /// The `LEN` field did not match the actual input length.
    LengthMismatch {
        /// Length declared by the block's LEN field.
        declared: usize,
        /// Actual datagram length.
        actual: usize,
    },
    /// The FSPEC marked an FRN present that this decoder cannot length (and
    /// so cannot safely skip): a spare position or the RFS indicator.
    UnknownItem(u8),
    /// A record's FSPEC was missing a required item (I002/010 or I002/000).
    MissingItem(u8),
    /// The FSPEC's FX chain ran past [`fspec::MAX_FSPEC_OCTETS`] — malformed.
    FspecTooLong,
}

impl std::fmt::Display for Cat002DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cat002DecodeError::Truncated => write!(f, "input ended before a complete item"),
            Cat002DecodeError::WrongCategory(cat) => write!(f, "expected CAT 2, got CAT {cat}"),
            Cat002DecodeError::LengthMismatch { declared, actual } => write!(
                f,
                "LEN field says {declared} bytes, but input is {actual} bytes"
            ),
            Cat002DecodeError::UnknownItem(frn) => {
                write!(f, "FSPEC marks unknown/unskippable FRN {frn} present")
            }
            Cat002DecodeError::MissingItem(frn) => {
                write!(f, "record is missing required FRN {frn}")
            }
            Cat002DecodeError::FspecTooLong => {
                write!(f, "FSPEC FX chain exceeds the supported FRN space")
            }
        }
    }
}

impl std::error::Error for Cat002DecodeError {}

/// Decode a CAT002 data block (`[CAT=2][LEN][record…]`) into one
/// [`DecodedServiceMessage`] per record — the same product as the CAT034
/// decoder, so the consumers are format-agnostic.
///
/// Returns [`Cat002DecodeError`] (and decodes nothing) on a wrong category,
/// length mismatch, truncation or an unknown present item — the caller drops
/// the datagram and keeps listening. Never panics on input.
pub fn decode_legacy_service_messages(
    bytes: &[u8],
) -> Result<Vec<DecodedServiceMessage>, Cat002DecodeError> {
    if bytes.len() < 3 {
        return Err(Cat002DecodeError::Truncated);
    }
    if bytes[0] != CATEGORY {
        return Err(Cat002DecodeError::WrongCategory(bytes[0]));
    }
    let declared = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    if declared != bytes.len() {
        return Err(Cat002DecodeError::LengthMismatch {
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
fn decode_record(cursor: &mut Cursor) -> Result<DecodedServiceMessage, Cat002DecodeError> {
    let frns = cursor.take_fspec()?;

    let mut sac_sic = None;
    let mut message_type = None;
    let mut time = None;
    let mut sector_deg = None;
    let mut antenna_rotation_secs = None;

    for frn in frns {
        let format = item_format(frn).ok_or(Cat002DecodeError::UnknownItem(frn))?;
        let bytes = match format {
            ItemFormat::Fixed(n) => cursor.take(n)?,
            ItemFormat::Extended => cursor.take_extended()?,
            ItemFormat::Repetitive(per) => cursor.take_repetitive(per)?,
            ItemFormat::Explicit => cursor.take_explicit()?,
        };
        match frn {
            uap::DATA_SOURCE_IDENTIFIER => sac_sic = Some((bytes[0], bytes[1])),
            uap::MESSAGE_TYPE => {
                // CAT002's code table differs from CAT034's — map explicitly
                // (type 3 here is the SOUTH marker, not geographical
                // filtering; it stays `Other`).
                message_type = Some(match bytes[0] {
                    MSG_NORTH_MARKER => ServiceMessageType::NorthMarker,
                    MSG_SECTOR_CROSSING => ServiceMessageType::SectorCrossing,
                    other => ServiceMessageType::Other(other),
                });
            }
            uap::SECTOR_NUMBER => sector_deg = Some(bytes[0] as f64 * SECTOR_LSB_DEGREES),
            uap::TIME_OF_DAY => {
                let ticks = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
                time = Some(Timestamp(ticks as f64 * TIME_LSB_SECONDS));
            }
            uap::ANTENNA_ROTATION_PERIOD => {
                let ticks = u16::from_be_bytes([bytes[0], bytes[1]]);
                antenna_rotation_secs = Some(ticks as f64 * ROTATION_LSB_SECONDS);
            }
            _ => {} // present but not service-relevant — length already consumed
        }
    }

    let (sac, sic) = sac_sic.ok_or(Cat002DecodeError::MissingItem(uap::DATA_SOURCE_IDENTIFIER))?;
    let message_type = message_type.ok_or(Cat002DecodeError::MissingItem(uap::MESSAGE_TYPE))?;

    Ok(DecodedServiceMessage {
        sac,
        sic,
        message_type,
        time,
        sector_deg,
        antenna_rotation_secs,
    })
}

/// A bounds-checked read cursor over a block's record bytes; every `take*`
/// returns [`Cat002DecodeError::Truncated`] rather than panicking.
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

    fn take(&mut self, n: usize) -> Result<&'a [u8], Cat002DecodeError> {
        if self.remaining() < n {
            return Err(Cat002DecodeError::Truncated);
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn take_fspec(&mut self) -> Result<std::collections::BTreeSet<u8>, Cat002DecodeError> {
        let slice = &self.bytes[self.pos..];
        let (frns, consumed) = fspec::parse(slice).map_err(|_| Cat002DecodeError::FspecTooLong)?;
        if consumed == 0 || slice[consumed - 1] & FX != 0 {
            return Err(Cat002DecodeError::Truncated);
        }
        self.pos += consumed;
        Ok(frns)
    }

    fn take_extended(&mut self) -> Result<&'a [u8], Cat002DecodeError> {
        let start = self.pos;
        loop {
            let octet = self.take(1)?[0];
            if octet & FX == 0 {
                break;
            }
        }
        Ok(&self.bytes[start..self.pos])
    }

    fn take_repetitive(&mut self, per: usize) -> Result<&'a [u8], Cat002DecodeError> {
        let start = self.pos;
        let rep = self.take(1)?[0] as usize;
        self.take(rep * per)?;
        Ok(&self.bytes[start..self.pos])
    }

    fn take_explicit(&mut self) -> Result<&'a [u8], Cat002DecodeError> {
        let start = self.pos;
        let total = self.take(1)?[0] as usize;
        if total == 0 {
            return Err(Cat002DecodeError::Truncated);
        }
        self.take(total - 1)?;
        Ok(&self.bytes[start..self.pos])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrap record bytes in a CAT002 block envelope.
    fn block(records: &[u8]) -> Vec<u8> {
        let mut out = vec![CATEGORY, 0x00, (3 + records.len()) as u8];
        out.extend_from_slice(records);
        out
    }

    /// A hand-built north-marker message decodes field-exactly — the CAT002
    /// reference vector. FSPEC {1,2,4} = 0xD0; time 01:00:00. Note the UAP
    /// difference vs CAT034: the sector sits on FRN 3 and the time on FRN 4.
    /// REQ: FR-IO-011
    #[test]
    fn north_marker_matches_reference_vector() {
        let bytes = block(&[
            0xD0, // FSPEC {1,2,4}
            0x19, 0x07, // I002/010 SAC=25 SIC=7
            0x01, // I002/000 north marker
            0x07, 0x08, 0x00, // I002/030 time 3600 s
        ]);
        let messages = decode_legacy_service_messages(&bytes).expect("decodes");
        assert_eq!(
            messages,
            vec![DecodedServiceMessage {
                sac: 25,
                sic: 7,
                message_type: ServiceMessageType::NorthMarker,
                time: Some(Timestamp(3600.0)),
                sector_deg: None,
                antenna_rotation_secs: None,
            }]
        );
    }

    /// A sector-crossing message with sector number and rotation period:
    /// sector 64 × 1.40625° = 90°; rotation 512 ticks / 128 = 4 s.
    /// REQ: FR-IO-011
    #[test]
    fn sector_crossing_decodes_sector_and_rotation() {
        let bytes = block(&[
            0xF8, // FSPEC {1,2,3,4,5}
            0x19, 0x07, // I002/010
            0x02, // I002/000 sector crossing
            0x40, // I002/020 sector 64 → 90°
            0x00, 0x00, 0x80, // I002/030 time 1 s
            0x02, 0x00, // I002/041 rotation 512 ticks → 4 s
        ]);
        let messages = decode_legacy_service_messages(&bytes).expect("decodes");
        let m = messages[0];
        assert_eq!(m.message_type, ServiceMessageType::SectorCrossing);
        assert_eq!(m.sector_deg, Some(90.0));
        assert_eq!(m.antenna_rotation_secs, Some(4.0));
        assert_eq!(m.time, Some(Timestamp(1.0)));
    }

    /// CAT002's type 3 is the SOUTH marker — deliberately `Other`, never
    /// mistaken for CAT034's geographical-filtering code 3. Blind-zone codes
    /// (8/9) equally. REQ: FR-IO-011
    #[test]
    fn south_marker_and_blind_zone_codes_stay_other() {
        for code in [3u8, 8, 9] {
            let bytes = block(&[0xC0, 0x19, 0x07, code]);
            let messages = decode_legacy_service_messages(&bytes).expect("decodes");
            assert_eq!(messages[0].message_type, ServiceMessageType::Other(code));
        }
    }

    /// Unused items (FX chains, repetitive plot counts, SP) are skipped
    /// length-correctly: a following record still decodes. REQ: FR-IO-011
    #[test]
    fn skips_unused_items_length_correctly() {
        let bytes = block(&[
            // Record 1: north marker + I002/050 (FX chain of 2) + I002/070
            // (REP=2 → 1+4 octets) + SP (FRN 13, octet 2 bit 0x04; total 3).
            0xD5, 0x84, // FSPEC {1,2,4,6,8,13}
            0x19, 0x07, // I002/010
            0x01, // I002/000
            0x00, 0x00, 0x80, // I002/030
            0x81, 0x00, // I002/050: FX then final octet
            0x02, 0xAA, 0xBB, 0xCC, 0xDD, // I002/070: REP=2, 2×2 octets
            0x03, 0xEE, 0xFF, // SP: total 3 octets
            // Record 2: minimal north marker.
            0xC0, 0x19, 0x07, 0x01,
        ]);
        let messages = decode_legacy_service_messages(&bytes).expect("decodes");
        assert_eq!(messages.len(), 2, "skip kept the stream in sync");
        assert_eq!(messages[0].time, Some(Timestamp(1.0)));
        assert_eq!(messages[1].message_type, ServiceMessageType::NorthMarker);
    }

    /// The spare FRN (12) and the RFS indicator (14) cannot be skipped and
    /// are hard errors. REQ: FR-IO-011, NFR-SAFE-002
    #[test]
    fn spare_and_rfs_frns_are_rejected() {
        // FRN 12 lives in octet 2 (FRN 8..14): bit 0x08.
        let spare = block(&[0xC1, 0x08, 0x19, 0x07, 0x01]);
        assert_eq!(
            decode_legacy_service_messages(&spare),
            Err(Cat002DecodeError::UnknownItem(12))
        );
        // FRN 14: bit 0x02 in octet 2.
        let rfs = block(&[0xC1, 0x02, 0x19, 0x07, 0x01]);
        assert_eq!(
            decode_legacy_service_messages(&rfs),
            Err(Cat002DecodeError::UnknownItem(14))
        );
    }

    /// Missing mandatory items (I002/010, I002/000) are hard errors.
    /// REQ: FR-IO-011
    #[test]
    fn missing_identity_or_type_is_rejected() {
        let no_identity = block(&[0x50, 0x01, 0x00, 0x00, 0x80]);
        assert_eq!(
            decode_legacy_service_messages(&no_identity),
            Err(Cat002DecodeError::MissingItem(1))
        );
        let no_type = block(&[0x90, 0x19, 0x07, 0x00, 0x00, 0x80]);
        assert_eq!(
            decode_legacy_service_messages(&no_type),
            Err(Cat002DecodeError::MissingItem(2))
        );
    }

    /// Wrong category / length lies are rejected without panic.
    /// REQ: FR-IO-011, NFR-SAFE-002
    #[test]
    fn wrong_category_and_length_mismatch_are_rejected() {
        assert_eq!(
            decode_legacy_service_messages(&[34, 0x00, 0x03]),
            Err(Cat002DecodeError::WrongCategory(34))
        );
        assert_eq!(
            decode_legacy_service_messages(&[CATEGORY, 0xFF, 0xFF, 0x00]),
            Err(Cat002DecodeError::LengthMismatch {
                declared: 0xFFFF,
                actual: 4
            })
        );
    }

    /// Every truncation of a valid block errors instead of panicking.
    /// REQ: NFR-SAFE-002
    #[test]
    fn truncations_never_panic() {
        let bytes = block(&[0xF8, 0x19, 0x07, 0x02, 0x40, 0x00, 0x00, 0x80, 0x02, 0x00]);
        for cut in 0..bytes.len() {
            let mut shortened = bytes[..cut].to_vec();
            if shortened.len() >= 3 {
                shortened[1] = 0;
                shortened[2] = shortened.len() as u8;
            }
            let _ = decode_legacy_service_messages(&shortened);
        }
    }

    /// A hostile FSPEC chaining FX octets past the supported FRN space is
    /// rejected, not panicked on (QW.2 parity). REQ: NFR-SAFE-002
    #[test]
    fn overlong_fspec_chain_is_rejected_not_panicked() {
        let mut bytes = vec![CATEGORY, 0x00, 63];
        bytes.extend_from_slice(&[0xFF; 60]);
        assert_eq!(
            decode_legacy_service_messages(&bytes),
            Err(Cat002DecodeError::FspecTooLong)
        );
    }
}
