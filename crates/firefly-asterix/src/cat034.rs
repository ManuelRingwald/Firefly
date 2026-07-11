//! CAT034 **input** decode — Monoradar Service Messages (FEP.1).
//!
//! A real radar head interleaves its target reports (CAT048) with **service
//! messages** (CAT034) on the same feed: a **north marker** once per antenna
//! revolution and a **sector crossing** message at each sector boundary
//! (typically 32 per revolution). They carry no targets — they describe the
//! *sensor*: where the antenna points and how fast it turns.
//!
//! Firefly consumes them for two operational reasons (ARTAS's FEP function —
//! sensor supervision from the data stream itself):
//!
//! - **Measured scan period.** The time between consecutive north markers is
//!   the radar's *actual* rotation period. Safety-relevant thresholds (track
//!   deletion cadence, CAT063 liveness) key off the scan period; measuring it
//!   beats trusting a static config value that drifts with the antenna motor.
//! - **Liveness without traffic.** Sector messages prove the sensor alive many
//!   times per revolution, independent of targets — "empty sky" and "dead
//!   radar" become distinguishable on the *input* side, mirroring what
//!   CAT065/063 provide on the output side.
//!
//! ## Robustness (security-relevant input path)
//!
//! Like [`cat048`](crate::cat048), this decoder reads **untrusted network
//! datagrams** (charter §8): every read goes through a bounds-checked
//! [`Cursor`], a truncated/corrupt record yields a [`Cat034DecodeError`] (the
//! datagram is dropped, the listener continues), and a present-but-unknown FRN
//! — or a set **spare** bit in a compound item's subfield spec — is a hard
//! error rather than a silent mis-parse. Covered by the `cat034_decode` fuzz
//! target.
//!
//! ## Scope
//!
//! The **service-relevant** items are decoded (I034/010, /000, /030, /020,
//! /041); every other standard CAT034 item is **skipped length-correctly**
//! via a complete per-FRN format model (fixed / repetitive / compound /
//! explicit).
//!
//! Items and layouts follow **EUROCONTROL SUR.ET1.ST05.2000-STD-02b**
//! ("ASTERIX Category 034 — Transmission of Monoradar Service Messages").
//!
//! REQ: FR-IO-009

use firefly_core::Timestamp;

use crate::fspec;

/// The ASTERIX category number for monoradar service messages.
const CATEGORY: u8 = 34;

/// I034/030 Time-of-Day LSB: 1/128 second (24-bit count since UTC midnight).
const TIME_LSB_SECONDS: f64 = 1.0 / 128.0;
/// I034/020 Sector Number LSB: 360/2⁸ degrees (a full turn over one octet).
const SECTOR_LSB_DEGREES: f64 = 360.0 / 256.0;
/// I034/041 Antenna Rotation Period LSB: 1/128 second.
const ROTATION_LSB_SECONDS: f64 = 1.0 / 128.0;
/// FX bit (lowest bit of an octet): "another octet follows".
const FX: u8 = 0x01;

/// The CAT034 UAP field reference numbers (FRN → data item), per
/// SUR.ET1.ST05.2000-STD-02b.
mod uap {
    /// I034/010 — Data Source Identifier (SAC/SIC).
    pub const DATA_SOURCE_IDENTIFIER: u8 = 1;
    /// I034/000 — Message Type (north marker, sector crossing, …).
    pub const MESSAGE_TYPE: u8 = 2;
    /// I034/030 — Time of Day.
    pub const TIME_OF_DAY: u8 = 3;
    /// I034/020 — Sector Number.
    pub const SECTOR_NUMBER: u8 = 4;
    /// I034/041 — Antenna Rotation Period (as reported by the radar).
    pub const ANTENNA_ROTATION_PERIOD: u8 = 5;
    /// I034/050 — System Configuration and Status (compound).
    pub const SYSTEM_CONFIGURATION: u8 = 6;
    /// I034/060 — System Processing Mode (compound).
    pub const SYSTEM_PROCESSING_MODE: u8 = 7;
    /// I034/070 — Message Count Values (repetitive, 2 octets per entry).
    pub const MESSAGE_COUNT_VALUES: u8 = 8;
    /// I034/100 — Generic Polar Window.
    pub const GENERIC_POLAR_WINDOW: u8 = 9;
    /// I034/110 — Data Filter.
    pub const DATA_FILTER: u8 = 10;
    /// I034/120 — 3D-Position of Data Source.
    pub const POSITION_3D: u8 = 11;
    /// I034/090 — Collimation Error.
    pub const COLLIMATION_ERROR: u8 = 12;
    /// I034/RE — Reserved Expansion Field (explicit length).
    pub const RESERVED_EXPANSION: u8 = 13;
    /// I034/SP — Special Purpose Field (explicit length).
    pub const SPECIAL_PURPOSE: u8 = 14;
}

/// Per-position subfield lengths of I034/050 (System Configuration and
/// Status): COM (1 octet), two spares, PSR (1), SSR (1), MDS (2), spare.
/// `None` marks a spare position — a datagram that sets it cannot be lengthed
/// and is rejected.
const I034_050_SUBFIELDS: [Option<usize>; 7] =
    [Some(1), None, None, Some(1), Some(1), Some(2), None];
/// Per-position subfield lengths of I034/060 (System Processing Mode):
/// COM (1), two spares, PSR (1), SSR (1), MDS (1), spare.
const I034_060_SUBFIELDS: [Option<usize>; 7] =
    [Some(1), None, None, Some(1), Some(1), Some(1), None];

/// How a CAT034 data item is laid out on the wire, so the decoder can consume
/// (and, for the items it doesn't interpret, *skip*) exactly its bytes.
enum ItemFormat {
    /// A fixed number of octets.
    Fixed(usize),
    /// A 1-octet repetition factor `REP`, then `REP × n` octets.
    Repetitive(usize),
    /// An FX-chained subfield-spec octet chain, then one subfield per set
    /// presence bit with the given per-position length (`None` = spare).
    Compound(&'static [Option<usize>]),
    /// A 1-octet length indicator giving the item's **total** length
    /// (including the indicator) — the SP/RE fields.
    Explicit,
}

/// The wire layout of each CAT034 UAP item. `None` for an FRN this decoder
/// does not know — a present unknown FRN cannot be skipped safely (no
/// length), so it is reported as [`Cat034DecodeError::UnknownItem`].
fn item_format(frn: u8) -> Option<ItemFormat> {
    use uap::*;
    Some(match frn {
        DATA_SOURCE_IDENTIFIER => ItemFormat::Fixed(2),
        MESSAGE_TYPE => ItemFormat::Fixed(1),
        TIME_OF_DAY => ItemFormat::Fixed(3),
        SECTOR_NUMBER => ItemFormat::Fixed(1),
        ANTENNA_ROTATION_PERIOD => ItemFormat::Fixed(2),
        SYSTEM_CONFIGURATION => ItemFormat::Compound(&I034_050_SUBFIELDS),
        SYSTEM_PROCESSING_MODE => ItemFormat::Compound(&I034_060_SUBFIELDS),
        MESSAGE_COUNT_VALUES => ItemFormat::Repetitive(2),
        GENERIC_POLAR_WINDOW => ItemFormat::Fixed(8),
        DATA_FILTER => ItemFormat::Fixed(1),
        POSITION_3D => ItemFormat::Fixed(8),
        COLLIMATION_ERROR => ItemFormat::Fixed(2),
        RESERVED_EXPANSION => ItemFormat::Explicit,
        SPECIAL_PURPOSE => ItemFormat::Explicit,
        _ => return None,
    })
}

/// What kind of service message a CAT034 record carries (I034/000).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceMessageType {
    /// Type 1 — the antenna crossed north: one per revolution, the basis of
    /// the measured scan period.
    NorthMarker,
    /// Type 2 — the antenna crossed a sector boundary: sub-revolution
    /// liveness, independent of traffic.
    SectorCrossing,
    /// Type 3 — geographical filtering message.
    GeographicalFiltering,
    /// Type 4 — jamming strobe message.
    JammingStrobe,
    /// A type this decoder does not interpret — tolerated (forward
    /// compatibility), carried through for logging/counting.
    Other(u8),
}

impl ServiceMessageType {
    fn from_code(code: u8) -> Self {
        match code {
            1 => ServiceMessageType::NorthMarker,
            2 => ServiceMessageType::SectorCrossing,
            3 => ServiceMessageType::GeographicalFiltering,
            4 => ServiceMessageType::JammingStrobe,
            other => ServiceMessageType::Other(other),
        }
    }
}

/// One decoded CAT034 service message — the neutral product of
/// [`decode_service_messages`]. Carries the **service-relevant** items;
/// everything else in the record is skipped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodedServiceMessage {
    /// I034/010 — System Area Code of the reporting radar.
    pub sac: u8,
    /// I034/010 — System Identification Code of the reporting radar.
    pub sic: u8,
    /// I034/000 — what this message announces.
    pub message_type: ServiceMessageType,
    /// I034/030 — time of day (1/128 s since UTC midnight), if present.
    /// Mandatory per spec for north-marker and sector messages; kept optional
    /// so a nonconforming record degrades gracefully instead of erroring.
    pub time: Option<Timestamp>,
    /// I034/020 — antenna azimuth of the sector boundary, degrees clockwise
    /// from north, if present.
    pub sector_deg: Option<f64>,
    /// I034/041 — the rotation period the radar itself reports, seconds, if
    /// present. Informational: Firefly *measures* the period from north
    /// markers rather than trusting this self-report.
    pub antenna_rotation_secs: Option<f64>,
}

/// Errors that can occur while decoding a CAT034 data block. Mirrors
/// [`Cat048DecodeError`](crate::Cat048DecodeError) — distinct type, distinct
/// UAP and required-item rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cat034DecodeError {
    /// The input ended before a complete block/record/item could be read.
    Truncated,
    /// The first octet was not [`CATEGORY`] (34).
    WrongCategory(u8),
    /// The `LEN` field did not match the actual input length.
    LengthMismatch {
        /// Length declared by the block's LEN field.
        declared: usize,
        /// Actual datagram length.
        actual: usize,
    },
    /// The FSPEC marked an FRN present that this decoder cannot length (and
    /// so cannot safely skip) — or a compound item set a spare subfield bit.
    UnknownItem(u8),
    /// A record's FSPEC was missing a required item (I034/010 or I034/000).
    MissingItem(u8),
    /// The FSPEC's FX chain ran past [`fspec::MAX_FSPEC_OCTETS`] — malformed.
    FspecTooLong,
}

impl std::fmt::Display for Cat034DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cat034DecodeError::Truncated => write!(f, "input ended before a complete item"),
            Cat034DecodeError::WrongCategory(cat) => write!(f, "expected CAT 34, got CAT {cat}"),
            Cat034DecodeError::LengthMismatch { declared, actual } => write!(
                f,
                "LEN field says {declared} bytes, but input is {actual} bytes"
            ),
            Cat034DecodeError::UnknownItem(frn) => {
                write!(f, "FSPEC marks unknown/unskippable FRN {frn} present")
            }
            Cat034DecodeError::MissingItem(frn) => {
                write!(f, "record is missing required FRN {frn}")
            }
            Cat034DecodeError::FspecTooLong => {
                write!(f, "FSPEC FX chain exceeds the supported FRN space")
            }
        }
    }
}

impl std::error::Error for Cat034DecodeError {}

/// Decode a CAT034 data block (`[CAT=34][LEN][record…]`) into one
/// [`DecodedServiceMessage`] per record.
///
/// Returns [`Cat034DecodeError`] (and decodes nothing) on a wrong category,
/// length mismatch, truncation or an unknown present item — the caller drops
/// the datagram and keeps listening. Never panics on input.
pub fn decode_service_messages(
    bytes: &[u8],
) -> Result<Vec<DecodedServiceMessage>, Cat034DecodeError> {
    if bytes.len() < 3 {
        return Err(Cat034DecodeError::Truncated);
    }
    if bytes[0] != CATEGORY {
        return Err(Cat034DecodeError::WrongCategory(bytes[0]));
    }
    let declared = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    if declared != bytes.len() {
        return Err(Cat034DecodeError::LengthMismatch {
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
fn decode_record(cursor: &mut Cursor) -> Result<DecodedServiceMessage, Cat034DecodeError> {
    let frns = cursor.take_fspec()?;

    let mut sac_sic = None;
    let mut message_type = None;
    let mut time = None;
    let mut sector_deg = None;
    let mut antenna_rotation_secs = None;

    for frn in frns {
        let format = item_format(frn).ok_or(Cat034DecodeError::UnknownItem(frn))?;
        let bytes = match format {
            ItemFormat::Fixed(n) => cursor.take(n)?,
            ItemFormat::Repetitive(per) => cursor.take_repetitive(per)?,
            ItemFormat::Compound(subfields) => cursor.take_compound(frn, subfields)?,
            ItemFormat::Explicit => cursor.take_explicit()?,
        };
        match frn {
            uap::DATA_SOURCE_IDENTIFIER => sac_sic = Some((bytes[0], bytes[1])),
            uap::MESSAGE_TYPE => message_type = Some(ServiceMessageType::from_code(bytes[0])),
            uap::TIME_OF_DAY => {
                let ticks = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
                time = Some(Timestamp(ticks as f64 * TIME_LSB_SECONDS));
            }
            uap::SECTOR_NUMBER => sector_deg = Some(bytes[0] as f64 * SECTOR_LSB_DEGREES),
            uap::ANTENNA_ROTATION_PERIOD => {
                let ticks = u16::from_be_bytes([bytes[0], bytes[1]]);
                antenna_rotation_secs = Some(ticks as f64 * ROTATION_LSB_SECONDS);
            }
            _ => {} // present but not service-relevant — length already consumed
        }
    }

    let (sac, sic) = sac_sic.ok_or(Cat034DecodeError::MissingItem(uap::DATA_SOURCE_IDENTIFIER))?;
    let message_type = message_type.ok_or(Cat034DecodeError::MissingItem(uap::MESSAGE_TYPE))?;

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
/// returns [`Cat034DecodeError::Truncated`] rather than panicking.
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

    fn take(&mut self, n: usize) -> Result<&'a [u8], Cat034DecodeError> {
        if self.remaining() < n {
            return Err(Cat034DecodeError::Truncated);
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn take_fspec(&mut self) -> Result<std::collections::BTreeSet<u8>, Cat034DecodeError> {
        let slice = &self.bytes[self.pos..];
        let (frns, consumed) = fspec::parse(slice).map_err(|_| Cat034DecodeError::FspecTooLong)?;
        if consumed == 0 || slice[consumed - 1] & FX != 0 {
            return Err(Cat034DecodeError::Truncated);
        }
        self.pos += consumed;
        Ok(frns)
    }

    fn take_repetitive(&mut self, per: usize) -> Result<&'a [u8], Cat034DecodeError> {
        let start = self.pos;
        let rep = self.take(1)?[0] as usize;
        self.take(rep * per)?;
        Ok(&self.bytes[start..self.pos])
    }

    /// Take a CAT034 compound item: an FX-chained subfield-spec octet chain,
    /// then one subfield per set presence bit with the per-position length
    /// from `subfields`. A set **spare** position (`None`) or a position past
    /// the table cannot be lengthed → the record is rejected rather than
    /// mis-parsed.
    fn take_compound(
        &mut self,
        frn: u8,
        subfields: &[Option<usize>],
    ) -> Result<&'a [u8], Cat034DecodeError> {
        let start = self.pos;
        let mut data_len = 0usize;
        let mut position = 0usize;
        loop {
            let octet = self.take(1)?[0];
            for bit in 0..7 {
                if octet & (0x80 >> bit) != 0 {
                    match subfields.get(position + bit) {
                        Some(Some(len)) => data_len += len,
                        _ => return Err(Cat034DecodeError::UnknownItem(frn)),
                    }
                }
            }
            position += 7;
            if octet & FX == 0 {
                break;
            }
        }
        self.take(data_len)?;
        Ok(&self.bytes[start..self.pos])
    }

    fn take_explicit(&mut self) -> Result<&'a [u8], Cat034DecodeError> {
        let start = self.pos;
        let total = self.take(1)?[0] as usize;
        if total == 0 {
            return Err(Cat034DecodeError::Truncated);
        }
        self.take(total - 1)?;
        Ok(&self.bytes[start..self.pos])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrap record bytes in a CAT034 block envelope.
    fn block(records: &[u8]) -> Vec<u8> {
        let mut out = vec![CATEGORY, 0x00, (3 + records.len()) as u8];
        out.extend_from_slice(records);
        out
    }

    /// A hand-built north-marker message decodes field-exactly — the CAT034
    /// reference vector. FSPEC {1,2,3} = 0xE0; SAC/SIC 25/7; type 1; time
    /// 01:00:00 (3600 s × 128 = 460 800 = 0x07_0800). REQ: FR-IO-009
    #[test]
    fn north_marker_matches_reference_vector() {
        let bytes = block(&[
            0xE0, // FSPEC: FRN 1+2+3
            0x19, 0x07, // I034/010 SAC=25 SIC=7
            0x01, // I034/000 message type 1 (north marker)
            0x07, 0x08, 0x00, // I034/030 time 3600 s
        ]);
        let messages = decode_service_messages(&bytes).expect("decodes");
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

    /// A sector-crossing message with sector number and reported rotation
    /// period: sector 64 × 1.40625° = 90°; rotation 512 ticks / 128 = 4 s.
    /// REQ: FR-IO-009
    #[test]
    fn sector_crossing_decodes_sector_and_rotation() {
        let bytes = block(&[
            0xF8, // FSPEC: FRN 1+2+3+4+5
            0x19, 0x07, // I034/010
            0x02, // I034/000 type 2 (sector crossing)
            0x00, 0x00, 0x80, // I034/030 time 1 s
            0x40, // I034/020 sector 64 → 90°
            0x02, 0x00, // I034/041 rotation 512 ticks → 4 s
        ]);
        let messages = decode_service_messages(&bytes).expect("decodes");
        assert_eq!(messages.len(), 1);
        let m = messages[0];
        assert_eq!(m.message_type, ServiceMessageType::SectorCrossing);
        assert_eq!(m.sector_deg, Some(90.0));
        assert_eq!(m.antenna_rotation_secs, Some(4.0));
        assert_eq!(m.time, Some(Timestamp(1.0)));
    }

    /// Two records in one block (sector then north marker) both decode — each
    /// record is self-delimiting via its FSPEC. REQ: FR-IO-009
    #[test]
    fn multiple_records_in_one_block_decode() {
        let bytes = block(&[
            0xF0, 0x19, 0x07, 0x02, 0x00, 0x00, 0x80, 0x40, // sector, no rotation
            0xE0, 0x19, 0x07, 0x01, 0x00, 0x01, 0x00, // north marker
        ]);
        let messages = decode_service_messages(&bytes).expect("decodes");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].message_type, ServiceMessageType::SectorCrossing);
        assert_eq!(messages[1].message_type, ServiceMessageType::NorthMarker);
    }

    /// The compound items I034/050 and I034/060 are skipped length-correctly:
    /// a record carrying them still yields the service fields, and a following
    /// record stays in sync. REQ: FR-IO-009
    #[test]
    fn compound_items_are_skipped_length_correctly() {
        let bytes = block(&[
            // Record 1: north marker + I034/050 (COM+PSR+SSR+MDS = 1+1+1+2 = 5
            // octets) + I034/060 (COM = 1 octet).
            0xE6, // FSPEC: FRN 1+2+3 (0xE0) + FRN 6+7 (0x04|0x02)
            0x19, 0x07, // I034/010
            0x01, // I034/000 north marker
            0x00, 0x00, 0x80, // I034/030
            0x9C, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, // I034/050: COM+PSR+SSR+MDS set
            0x80, 0x11, // I034/060: COM set
            // Record 2: plain north marker — must still parse.
            0xE0, 0x19, 0x07, 0x01, 0x00, 0x01, 0x00,
        ]);
        let messages = decode_service_messages(&bytes).expect("decodes");
        assert_eq!(messages.len(), 2, "compound skip kept the stream in sync");
        assert_eq!(messages[0].message_type, ServiceMessageType::NorthMarker);
        assert_eq!(messages[1].time, Some(Timestamp(2.0)));
    }

    /// A compound item that sets a **spare** subfield bit cannot be lengthed
    /// and is rejected instead of mis-parsed. REQ: FR-IO-009, NFR-SAFE-002
    #[test]
    fn compound_spare_bit_is_rejected() {
        let bytes = block(&[
            0xE4, // FSPEC: FRN 1+2+3+6
            0x19, 0x07, 0x01, 0x00, 0x00, 0x80, // identity, type, time
            0x40, 0x00, // I034/050: spare bit 7 set
        ]);
        assert_eq!(
            decode_service_messages(&bytes),
            Err(Cat034DecodeError::UnknownItem(6))
        );
    }

    /// An unknown message-type code decodes as `Other` (forward tolerance)
    /// rather than erroring. REQ: FR-IO-009
    #[test]
    fn unknown_message_type_is_tolerated() {
        let bytes = block(&[0xC0, 0x19, 0x07, 0x09]);
        let messages = decode_service_messages(&bytes).expect("decodes");
        assert_eq!(messages[0].message_type, ServiceMessageType::Other(9));
        assert_eq!(messages[0].time, None);
    }

    /// Missing mandatory items (I034/010, I034/000) are hard errors.
    /// REQ: FR-IO-009
    #[test]
    fn missing_identity_or_type_is_rejected() {
        // Type + time but no I034/010.
        let no_identity = block(&[0x60, 0x01, 0x00, 0x00, 0x80]);
        assert_eq!(
            decode_service_messages(&no_identity),
            Err(Cat034DecodeError::MissingItem(1))
        );
        // Identity + time but no I034/000.
        let no_type = block(&[0xA0, 0x19, 0x07, 0x00, 0x00, 0x80]);
        assert_eq!(
            decode_service_messages(&no_type),
            Err(Cat034DecodeError::MissingItem(2))
        );
    }

    /// Wrong category / length lies are rejected without panic.
    /// REQ: FR-IO-009, NFR-SAFE-002
    #[test]
    fn wrong_category_and_length_mismatch_are_rejected() {
        assert_eq!(
            decode_service_messages(&[48, 0x00, 0x03]),
            Err(Cat034DecodeError::WrongCategory(48))
        );
        assert_eq!(
            decode_service_messages(&[CATEGORY, 0xFF, 0xFF, 0x00]),
            Err(Cat034DecodeError::LengthMismatch {
                declared: 0xFFFF,
                actual: 4
            })
        );
    }

    /// Every truncation of a valid block errors instead of panicking.
    /// REQ: NFR-SAFE-002
    #[test]
    fn truncations_never_panic() {
        let bytes = block(&[0xF8, 0x19, 0x07, 0x02, 0x00, 0x00, 0x80, 0x40, 0x02, 0x00]);
        for cut in 0..bytes.len() {
            let mut shortened = bytes[..cut].to_vec();
            if shortened.len() >= 3 {
                // Keep the LEN field honest so the truncation reaches the parser.
                shortened[1] = 0;
                shortened[2] = shortened.len() as u8;
            }
            let _ = decode_service_messages(&shortened);
        }
    }

    /// A hostile record whose FSPEC chains FX octets past the supported FRN
    /// space is rejected, not panicked on (QW.2 parity). REQ: NFR-SAFE-002
    #[test]
    fn overlong_fspec_chain_is_rejected_not_panicked() {
        let mut bytes = vec![CATEGORY, 0x00, 63];
        bytes.extend_from_slice(&[0xFF; 60]);
        assert_eq!(
            decode_service_messages(&bytes),
            Err(Cat034DecodeError::FspecTooLong)
        );
    }
}
