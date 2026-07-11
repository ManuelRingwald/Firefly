//! CAT001 **input** decode — legacy Monoradar Target Reports (FEP.4).
//!
//! CAT001 is the **predecessor** of CAT048: the same job (one record per
//! radar plot/track), an older wire format. A large share of fielded radar
//! heads still emit CAT001 (paired with CAT002 service messages), so an SDPS
//! that only speaks CAT048 cannot connect them. This decoder makes Firefly
//! attachable to that legacy generation; the record product converts into the
//! same neutral [`DecodedTargetReport`] the CAT048 path produces, so the
//! adapter and tracker are format-agnostic.
//!
//! ## The two UAPs (the legacy trap)
//!
//! Unlike every later category, CAT001 has **two** User Application Profiles:
//! one for **plots**, one for **tracks**. The same FSPEC bit means a
//! *different item* depending on the record's kind — e.g. FRN 3 is I001/040
//! (position) in the plot UAP but I001/161 (track number) in the track UAP.
//! The selector is the **TYP bit** of I001/020 (FRN 2, shared by both UAPs):
//! `0` = plot, `1` = track. A record that marks any FRN ≥ 3 present without
//! carrying I001/020 is therefore undecodable and rejected — guessing the
//! UAP would be a silent mis-parse of every following byte.
//!
//! ## Truncated time (I001/141)
//!
//! CAT001 records carry no full time of day. I001/141 is a **truncated**
//! 16-bit counter in 1/128 s — it wraps every 512 s
//! ([`TRUNCATED_TOD_CYCLE_SECS`]). The full time classically comes from the
//! CAT002 service stream (I002/030); the *adapter* anchors the truncated
//! value to it. This decoder stays honest: it reports the truncated seconds
//! as-is and never invents a full timestamp.
//!
//! ## Robustness (security-relevant input path)
//!
//! Same policy as the sibling decoders (charter §8): bounds-checked cursor,
//! truncated/corrupt records yield [`Cat001DecodeError`] (datagram dropped,
//! no panic), unknown/spare FRNs — and the never-used **RFS** (Random Field
//! Sequencing) indicator, which cannot be skipped without interpreting it —
//! are hard errors instead of silent mis-parses. Covered by the
//! `cat001_decode` fuzz target.
//!
//! Items and layouts follow **EUROCONTROL SUR.ET1.ST05.2000-STD-02a**
//! ("ASTERIX Category 001 — Transmission of Monoradar Target Reports").
//!
//! REQ: FR-IO-011

use firefly_core::Timestamp;
use firefly_geo::Polar;

use crate::cat048::{DecodedTargetReport, Detection};
use crate::fspec;

/// The ASTERIX category number for legacy monoradar target reports.
const CATEGORY: u8 = 1;

/// I001/141 Truncated Time of Day LSB: 1/128 second.
const TIME_LSB_SECONDS: f64 = 1.0 / 128.0;
/// The period after which the 16-bit truncated time of day (I001/141) wraps:
/// 2¹⁶ × 1/128 s. The adapter needs a full-time anchor (CAT002 I002/030) to
/// expand it.
pub const TRUNCATED_TOD_CYCLE_SECS: f64 = 512.0;
/// I001/040 RHO LSB: 1/128 nautical mile (coarser than CAT048's 1/256).
const RHO_LSB_NM: f64 = 1.0 / 128.0;
/// Nautical mile in metres.
const NM_TO_METRES: f64 = 1852.0;
/// I001/040 THETA LSB: 360/2¹⁶ degrees.
const THETA_LSB_DEGREES: f64 = 360.0 / 65536.0;
/// I001/070 — the Mode 3/A reply lives in the low 12 bits.
const MODE_3A_CODE_MASK: u16 = 0x0FFF;
/// I001/161 — the track number lives in the low 12 bits.
const TRACK_NUMBER_MASK: u16 = 0x0FFF;
/// I001/090 — the flight level lives in the low 14 bits (V/G masked off).
const FLIGHT_LEVEL_MASK: u16 = 0x3FFF;
/// I001/090 sign bit of the 14-bit two's-complement flight level.
const FLIGHT_LEVEL_SIGN: i32 = 0x2000;
/// I001/090 two's-complement modulus (2¹⁴).
const FLIGHT_LEVEL_MODULUS: i32 = 0x4000;
/// I001/090 LSB: 1/4 flight level = 25 ft.
const FLIGHT_LEVEL_LSB_FT: f64 = 25.0;
/// I001/020 octet 1 — TYP (0 = plot record, 1 = track record): the UAP selector.
const TRD_TYP: u8 = 0x80;
/// I001/020 octet 1 — SIM (simulated target report).
const TRD_SIM: u8 = 0x40;
/// I001/020 octet 1 — the 2-bit SSR/PSR detection field (bits 6–5).
const TRD_SSRPSR_SHIFT: u8 = 4;
/// I001/020 octet 1 — SPI (special position identification).
const TRD_SPI: u8 = 0x04;
/// FX bit (lowest bit of an octet): "another octet follows".
const FX: u8 = 0x01;

/// Which of CAT001's two UAPs applies to a record (I001/020 TYP bit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Uap {
    Plot,
    Track,
}

/// The FRNs this decoder interprets, per UAP. The numeric values differ
/// between the two UAPs — that is the whole point of the split (see the
/// module docs).
mod frn {
    /// I001/010 — Data Source Identifier (both UAPs).
    pub const DATA_SOURCE_IDENTIFIER: u8 = 1;
    /// I001/020 — Target Report Descriptor (both UAPs; carries the UAP
    /// selector TYP).
    pub const TARGET_REPORT_DESCRIPTOR: u8 = 2;

    /// I001/040 — Measured Position Polar (plot UAP).
    pub const PLOT_POSITION: u8 = 3;
    /// I001/070 — Mode-3/A Code (plot UAP).
    pub const PLOT_MODE_3A: u8 = 4;
    /// I001/090 — Mode-C Code (plot UAP).
    pub const PLOT_MODE_C: u8 = 5;
    /// I001/141 — Truncated Time of Day (plot UAP).
    pub const PLOT_TRUNCATED_TOD: u8 = 7;

    /// I001/161 — Track/Plot Number (track UAP).
    pub const TRACK_NUMBER: u8 = 3;
    /// I001/040 — Measured Position Polar (track UAP).
    pub const TRACK_POSITION: u8 = 4;
    /// I001/070 — Mode-3/A Code (track UAP).
    pub const TRACK_MODE_3A: u8 = 7;
    /// I001/090 — Mode-C Code (track UAP).
    pub const TRACK_MODE_C: u8 = 8;
    /// I001/141 — Truncated Time of Day (track UAP).
    pub const TRACK_TRUNCATED_TOD: u8 = 9;
}

/// How a CAT001 data item is laid out on the wire.
enum ItemFormat {
    /// A fixed number of octets.
    Fixed(usize),
    /// An FX-chained octet chain (1 octet, repeat while its low bit is set).
    /// Covers both the spec's "variable" items (I001/020, /170) and its
    /// FX-repetitive items (I001/030, /130, /210) — byte-identical on the wire.
    Extended,
    /// A 1-octet length indicator giving the item's **total** length — the SP
    /// field.
    Explicit,
}

/// The wire layout of each FRN in the **plot** UAP, per
/// SUR.ET1.ST05.2000-STD-02a. `None` for spare FRNs (16–19) and the RFS
/// indicator (21) — present-but-unskippable, reported as
/// [`Cat001DecodeError::UnknownItem`].
fn plot_item_format(frn: u8) -> Option<ItemFormat> {
    Some(match frn {
        1 => ItemFormat::Fixed(2),  // I001/010
        2 => ItemFormat::Extended,  // I001/020
        3 => ItemFormat::Fixed(4),  // I001/040
        4 => ItemFormat::Fixed(2),  // I001/070
        5 => ItemFormat::Fixed(2),  // I001/090
        6 => ItemFormat::Extended,  // I001/130 (FX-repetitive)
        7 => ItemFormat::Fixed(2),  // I001/141
        8 => ItemFormat::Fixed(2),  // I001/050
        9 => ItemFormat::Fixed(1),  // I001/120
        10 => ItemFormat::Fixed(1), // I001/131
        11 => ItemFormat::Fixed(2), // I001/080
        12 => ItemFormat::Fixed(4), // I001/100
        13 => ItemFormat::Fixed(2), // I001/060
        14 => ItemFormat::Extended, // I001/030 (FX-repetitive)
        15 => ItemFormat::Fixed(1), // I001/150
        20 => ItemFormat::Explicit, // SP
        _ => return None,           // 16–19 spare, 21 RFS
    })
}

/// The wire layout of each FRN in the **track** UAP. `None` for the RFS
/// indicator (21).
fn track_item_format(frn: u8) -> Option<ItemFormat> {
    Some(match frn {
        1 => ItemFormat::Fixed(2),  // I001/010
        2 => ItemFormat::Extended,  // I001/020
        3 => ItemFormat::Fixed(2),  // I001/161
        4 => ItemFormat::Fixed(4),  // I001/040
        5 => ItemFormat::Fixed(4),  // I001/042
        6 => ItemFormat::Fixed(4),  // I001/200
        7 => ItemFormat::Fixed(2),  // I001/070
        8 => ItemFormat::Fixed(2),  // I001/090
        9 => ItemFormat::Fixed(2),  // I001/141
        10 => ItemFormat::Extended, // I001/130 (FX-repetitive)
        11 => ItemFormat::Fixed(1), // I001/131
        12 => ItemFormat::Fixed(1), // I001/120
        13 => ItemFormat::Extended, // I001/170
        14 => ItemFormat::Extended, // I001/210 (FX-repetitive)
        15 => ItemFormat::Fixed(2), // I001/050
        16 => ItemFormat::Fixed(2), // I001/080
        17 => ItemFormat::Fixed(4), // I001/100
        18 => ItemFormat::Fixed(2), // I001/060
        19 => ItemFormat::Extended, // I001/030 (FX-repetitive)
        20 => ItemFormat::Explicit, // SP
        22 => ItemFormat::Fixed(1), // I001/150
        _ => return None,           // 21 RFS
    })
}

/// One decoded CAT001 target report — the neutral product of
/// [`decode_legacy_reports`]. Converts into a [`DecodedTargetReport`] via
/// [`Self::into_target_report`] once the adapter has expanded the truncated
/// time against its CAT002 anchor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodedLegacyReport {
    /// I001/010 — System Area Code of the reporting radar.
    pub sac: u8,
    /// I001/010 — System Identification Code of the reporting radar.
    pub sic: u8,
    /// I001/141 — truncated time of day in seconds, **modulo 512 s**
    /// ([`TRUNCATED_TOD_CYCLE_SECS`]), if present. NOT a full timestamp — the
    /// adapter expands it against the CAT002 time anchor.
    pub truncated_time_secs: Option<f64>,
    /// I001/020 SSR/PSR — the detection kind, mapped onto the CAT048
    /// [`Detection`] vocabulary (CAT001 predates Mode S, so only the
    /// PSR/SSR/combined variants occur).
    pub detection: Detection,
    /// I001/020 SIM — a simulated target report. Firefly carries no simulated
    /// traffic (FR-TRK-036); the adapter drops these.
    pub simulated: bool,
    /// I001/040 — measured position in polar coordinates (range in
    /// **metres**, azimuth in radians clockwise from north), if present.
    pub position: Option<Polar>,
    /// I001/070 — Mode-3/A code (octal, low 12 bits), if present.
    pub mode_3a: Option<u16>,
    /// I001/090 — Mode-C flight level in **feet**, if present.
    pub flight_level_ft: Option<f64>,
    /// I001/161 — radar track number (track UAP only), if present.
    pub track_number: Option<u16>,
    /// I001/020 SPI — special position identification pulse.
    pub spi: bool,
}

impl DecodedLegacyReport {
    /// Convert into the CAT048-shaped [`DecodedTargetReport`] with the given
    /// **full** time of day, so the legacy path feeds the exact same plot
    /// mapping as the modern one. `time` is the adapter-expanded timestamp
    /// (`None` when no anchor was available — the plot mapping then drops the
    /// report, a time-less measurement is not a measurement).
    pub fn into_target_report(self, time: Option<Timestamp>) -> DecodedTargetReport {
        DecodedTargetReport {
            sac: self.sac,
            sic: self.sic,
            time,
            detection: self.detection,
            position: self.position,
            mode_3a: self.mode_3a,
            flight_level_ft: self.flight_level_ft,
            // CAT001 predates Mode S: no ICAO address, no callsign, no DAPs.
            icao_address: None,
            callsign: None,
            track_number: self.track_number,
            spi: self.spi,
            daps: firefly_core::Daps::default(),
        }
    }
}

/// Errors that can occur while decoding a CAT001 data block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cat001DecodeError {
    /// The input ended before a complete block/record/item could be read.
    Truncated,
    /// The first octet was not [`CATEGORY`] (1).
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
    /// A record's FSPEC was missing a required item: I001/010, or I001/020
    /// when any FRN ≥ 3 is present (no UAP selector → the following bytes
    /// cannot be interpreted).
    MissingItem(u8),
    /// The FSPEC's FX chain ran past [`fspec::MAX_FSPEC_OCTETS`] — malformed.
    FspecTooLong,
}

impl std::fmt::Display for Cat001DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cat001DecodeError::Truncated => write!(f, "input ended before a complete item"),
            Cat001DecodeError::WrongCategory(cat) => write!(f, "expected CAT 1, got CAT {cat}"),
            Cat001DecodeError::LengthMismatch { declared, actual } => write!(
                f,
                "LEN field says {declared} bytes, but input is {actual} bytes"
            ),
            Cat001DecodeError::UnknownItem(frn) => {
                write!(f, "FSPEC marks unknown/unskippable FRN {frn} present")
            }
            Cat001DecodeError::MissingItem(frn) => {
                write!(f, "record is missing required FRN {frn}")
            }
            Cat001DecodeError::FspecTooLong => {
                write!(f, "FSPEC FX chain exceeds the supported FRN space")
            }
        }
    }
}

impl std::error::Error for Cat001DecodeError {}

/// Decode a CAT001 data block (`[CAT=1][LEN][record…]`) into one
/// [`DecodedLegacyReport`] per record, selecting each record's UAP (plot vs
/// track) from its own I001/020 TYP bit.
///
/// Returns [`Cat001DecodeError`] (and decodes nothing) on a wrong category,
/// length mismatch, truncation or an unknown present item — the caller drops
/// the datagram and keeps listening. Never panics on input.
pub fn decode_legacy_reports(bytes: &[u8]) -> Result<Vec<DecodedLegacyReport>, Cat001DecodeError> {
    if bytes.len() < 3 {
        return Err(Cat001DecodeError::Truncated);
    }
    if bytes[0] != CATEGORY {
        return Err(Cat001DecodeError::WrongCategory(bytes[0]));
    }
    let declared = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    if declared != bytes.len() {
        return Err(Cat001DecodeError::LengthMismatch {
            declared,
            actual: bytes.len(),
        });
    }

    let mut cursor = Cursor::new(&bytes[3..]);
    let mut reports = Vec::new();
    while cursor.remaining() > 0 {
        reports.push(decode_record(&mut cursor)?);
    }
    Ok(reports)
}

/// One record: FSPEC, I001/010 + I001/020 (shared FRNs 1–2), then — with the
/// UAP now known from TYP — the remaining present items in ascending-FRN
/// order.
fn decode_record(cursor: &mut Cursor) -> Result<DecodedLegacyReport, Cat001DecodeError> {
    let frns = cursor.take_fspec()?;

    let mut sac_sic = None;
    let mut uap = None;
    let mut detection = Detection::NoDetection;
    let mut simulated = false;
    let mut spi = false;
    let mut truncated_time_secs = None;
    let mut position = None;
    let mut mode_3a = None;
    let mut flight_level_ft = None;
    let mut track_number = None;

    for frn in frns {
        // FRNs 1–2 are shared; everything beyond needs the UAP selector.
        match frn {
            frn::DATA_SOURCE_IDENTIFIER => {
                let bytes = cursor.take(2)?;
                sac_sic = Some((bytes[0], bytes[1]));
            }
            frn::TARGET_REPORT_DESCRIPTOR => {
                let bytes = cursor.take_extended()?;
                uap = Some(if bytes[0] & TRD_TYP != 0 {
                    Uap::Track
                } else {
                    Uap::Plot
                });
                simulated = bytes[0] & TRD_SIM != 0;
                spi = bytes[0] & TRD_SPI != 0;
                detection = match (bytes[0] >> TRD_SSRPSR_SHIFT) & 0b11 {
                    0b00 => Detection::NoDetection,
                    0b01 => Detection::Psr,
                    0b10 => Detection::Ssr,
                    _ => Detection::SsrPsr,
                };
            }
            _ => {
                // The record marks an item whose meaning depends on the UAP —
                // without I001/020 the bytes cannot be interpreted at all.
                let uap = uap.ok_or(Cat001DecodeError::MissingItem(
                    frn::TARGET_REPORT_DESCRIPTOR,
                ))?;
                let format = match uap {
                    Uap::Plot => plot_item_format(frn),
                    Uap::Track => track_item_format(frn),
                }
                .ok_or(Cat001DecodeError::UnknownItem(frn))?;
                let bytes = match format {
                    ItemFormat::Fixed(n) => cursor.take(n)?,
                    ItemFormat::Extended => cursor.take_extended()?,
                    ItemFormat::Explicit => cursor.take_explicit()?,
                };
                match (uap, frn) {
                    (Uap::Plot, frn::PLOT_POSITION) | (Uap::Track, frn::TRACK_POSITION) => {
                        position = Some(decode_polar(bytes));
                    }
                    (Uap::Plot, frn::PLOT_MODE_3A) | (Uap::Track, frn::TRACK_MODE_3A) => {
                        mode_3a =
                            Some(u16::from_be_bytes([bytes[0], bytes[1]]) & MODE_3A_CODE_MASK);
                    }
                    (Uap::Plot, frn::PLOT_MODE_C) | (Uap::Track, frn::TRACK_MODE_C) => {
                        flight_level_ft = Some(decode_flight_level(bytes));
                    }
                    (Uap::Plot, frn::PLOT_TRUNCATED_TOD)
                    | (Uap::Track, frn::TRACK_TRUNCATED_TOD) => {
                        let ticks = u16::from_be_bytes([bytes[0], bytes[1]]);
                        truncated_time_secs = Some(ticks as f64 * TIME_LSB_SECONDS);
                    }
                    (Uap::Track, frn::TRACK_NUMBER) => {
                        track_number =
                            Some(u16::from_be_bytes([bytes[0], bytes[1]]) & TRACK_NUMBER_MASK);
                    }
                    _ => {} // present but not plot-relevant — length already consumed
                }
            }
        }
    }

    let (sac, sic) = sac_sic.ok_or(Cat001DecodeError::MissingItem(frn::DATA_SOURCE_IDENTIFIER))?;

    Ok(DecodedLegacyReport {
        sac,
        sic,
        truncated_time_secs,
        detection,
        simulated,
        position,
        mode_3a,
        flight_level_ft,
        track_number,
        spi,
    })
}

/// I001/040 — RHO (1/128 NM → metres) and THETA (360/2¹⁶ ° → radians).
fn decode_polar(bytes: &[u8]) -> Polar {
    let rho_m = u16::from_be_bytes([bytes[0], bytes[1]]) as f64 * RHO_LSB_NM * NM_TO_METRES;
    let theta_deg = u16::from_be_bytes([bytes[2], bytes[3]]) as f64 * THETA_LSB_DEGREES;
    Polar::new(rho_m, theta_deg.to_radians(), 0.0)
}

/// I001/090 — a 14-bit two's-complement flight level in 1/4-FL (25-ft) steps,
/// the V/G bits masked off and the sign extended (same layout as CAT048).
fn decode_flight_level(bytes: &[u8]) -> f64 {
    let raw = u16::from_be_bytes([bytes[0], bytes[1]]) & FLIGHT_LEVEL_MASK;
    let mut level = raw as i32;
    if level & FLIGHT_LEVEL_SIGN != 0 {
        level -= FLIGHT_LEVEL_MODULUS;
    }
    level as f64 * FLIGHT_LEVEL_LSB_FT
}

/// A bounds-checked read cursor over a block's record bytes; every `take*`
/// returns [`Cat001DecodeError::Truncated`] rather than panicking.
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

    fn take(&mut self, n: usize) -> Result<&'a [u8], Cat001DecodeError> {
        if self.remaining() < n {
            return Err(Cat001DecodeError::Truncated);
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn take_fspec(&mut self) -> Result<std::collections::BTreeSet<u8>, Cat001DecodeError> {
        let slice = &self.bytes[self.pos..];
        let (frns, consumed) = fspec::parse(slice).map_err(|_| Cat001DecodeError::FspecTooLong)?;
        if consumed == 0 || slice[consumed - 1] & FX != 0 {
            return Err(Cat001DecodeError::Truncated);
        }
        self.pos += consumed;
        Ok(frns)
    }

    /// Take an FX-chained octet chain: octets while each one's low bit (FX)
    /// is set, ending with the first octet whose FX is clear.
    fn take_extended(&mut self) -> Result<&'a [u8], Cat001DecodeError> {
        let start = self.pos;
        loop {
            let octet = self.take(1)?[0];
            if octet & FX == 0 {
                break;
            }
        }
        Ok(&self.bytes[start..self.pos])
    }

    fn take_explicit(&mut self) -> Result<&'a [u8], Cat001DecodeError> {
        let start = self.pos;
        let total = self.take(1)?[0] as usize;
        if total == 0 {
            return Err(Cat001DecodeError::Truncated);
        }
        self.take(total - 1)?;
        Ok(&self.bytes[start..self.pos])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrap record bytes in a CAT001 block envelope.
    fn block(records: &[u8]) -> Vec<u8> {
        let mut out = vec![CATEGORY, 0x00, (3 + records.len()) as u8];
        out.extend_from_slice(records);
        out
    }

    /// A hand-built plot-UAP record decodes field-exactly — the CAT001
    /// reference vector. FSPEC {1,2,3,4,5,7} = 0xFA; combined detection;
    /// RHO 12 800 × 1/128 NM = 100 NM; THETA 16 384 → 90°; Mode 3/A 1213₈;
    /// FL350 (1400 quarter-FL); truncated time 12 800 ticks = 100 s.
    /// REQ: FR-IO-011
    #[test]
    fn plot_record_matches_reference_vector() {
        let bytes = block(&[
            0xFA, // FSPEC {1,2,3,4,5,7}
            0x19, 0x07, // I001/010 SAC=25 SIC=7
            0x30, // I001/020 TYP=0 (plot), SSR/PSR=11 (combined)
            0x32, 0x00, 0x40, 0x00, // I001/040 RHO=12800, THETA=16384
            0x02, 0x8B, // I001/070 Mode 3/A 1213 octal
            0x05, 0x78, // I001/090 1400 quarter-FL = FL350
            0x32, 0x00, // I001/141 12800 ticks = 100 s
        ]);
        let reports = decode_legacy_reports(&bytes).expect("decodes");
        assert_eq!(reports.len(), 1);
        let r = reports[0];
        assert_eq!((r.sac, r.sic), (25, 7));
        assert_eq!(r.detection, Detection::SsrPsr);
        assert!(!r.simulated && !r.spi);
        let position = r.position.expect("position");
        assert!(
            (position.range - 100.0 * NM_TO_METRES).abs() < 1e-6,
            "100 NM"
        );
        assert!((position.azimuth - 90.0_f64.to_radians()).abs() < 1e-9);
        assert_eq!(r.mode_3a, Some(0o1213));
        assert_eq!(r.flight_level_ft, Some(35_000.0));
        assert_eq!(r.truncated_time_secs, Some(100.0));
        assert_eq!(r.track_number, None, "plot UAP has no track number");
    }

    /// A track-UAP record: FRN 3 is the track number, position sits on FRN 4,
    /// the (skipped) polar velocity on FRN 6 and the truncated time on FRN 9.
    /// REQ: FR-IO-011
    #[test]
    fn track_record_reads_the_track_uap() {
        let bytes = block(&[
            0xF5, 0x40, // FSPEC {1,2,3,4,6,9}
            0x19, 0x07, // I001/010
            0xA0, // I001/020 TYP=1 (track), SSR/PSR=10 (sole secondary)
            0x00, 0x2A, // I001/161 track number 42
            0x32, 0x00, 0x40, 0x00, // I001/040
            0xAA, 0xBB, 0xCC, 0xDD, // I001/200 velocity — skipped
            0x19, 0x00, // I001/141 6400 ticks = 50 s
        ]);
        let reports = decode_legacy_reports(&bytes).expect("decodes");
        assert_eq!(reports.len(), 1);
        let r = reports[0];
        assert_eq!(r.detection, Detection::Ssr);
        assert_eq!(r.track_number, Some(42));
        assert!(r.position.is_some());
        assert_eq!(r.truncated_time_secs, Some(50.0));
    }

    /// The same FSPEC bits mean different items depending on TYP: FRN 3 is a
    /// 4-octet position in a plot record but a 2-octet track number in a
    /// track record — two such records in one block both stay in sync.
    /// REQ: FR-IO-011
    #[test]
    fn typ_bit_selects_the_uap_per_record() {
        let bytes = block(&[
            // Record 1: plot, FSPEC {1,2,3} → I001/040 (4 octets).
            0xE0, 0x19, 0x07, 0x10, 0x32, 0x00, 0x40, 0x00,
            // Record 2: track, FSPEC {1,2,3} → I001/161 (2 octets).
            0xE0, 0x19, 0x07, 0xA0, 0x00, 0x2A,
        ]);
        let reports = decode_legacy_reports(&bytes).expect("decodes");
        assert_eq!(reports.len(), 2, "per-record UAP selection kept sync");
        assert!(reports[0].position.is_some());
        assert_eq!(reports[0].track_number, None);
        assert_eq!(reports[1].track_number, Some(42));
        assert!(reports[1].position.is_none());
    }

    /// SIM and SPI decode from the descriptor; an FX-extended descriptor is
    /// consumed whole. REQ: FR-IO-011
    #[test]
    fn descriptor_flags_and_extension_decode() {
        let bytes = block(&[
            0xC0, // FSPEC {1,2}
            0x19, 0x07, //
            0x75, 0x00, // I001/020 SIM+SSR/PSR=11+SPI, FX → 1 extension octet
        ]);
        let reports = decode_legacy_reports(&bytes).expect("decodes");
        let r = reports[0];
        assert!(r.simulated);
        assert!(r.spi);
        assert_eq!(r.detection, Detection::SsrPsr);
    }

    /// A record marking any FRN ≥ 3 without I001/020 has no UAP selector —
    /// the bytes cannot be interpreted, so it is rejected instead of guessed.
    /// REQ: FR-IO-011
    #[test]
    fn item_without_uap_selector_is_rejected() {
        let bytes = block(&[
            0xA0, // FSPEC {1,3} — no FRN 2
            0x19, 0x07, //
            0x32, 0x00, 0x40, 0x00,
        ]);
        assert_eq!(
            decode_legacy_reports(&bytes),
            Err(Cat001DecodeError::MissingItem(2))
        );
    }

    /// A spare FRN (plot 16–19) or the RFS indicator (21) cannot be skipped
    /// and is a hard error, not a silent mis-parse. REQ: FR-IO-011, NFR-SAFE-002
    #[test]
    fn spare_and_rfs_frns_are_rejected() {
        // Plot record with FRN 16 (spare): FSPEC needs 3 octets
        // (16 lives in octet 3: FRN 15..21 → bit for 16 is 0x40).
        let spare = block(&[
            0xC1, 0x01, 0x40, // FSPEC {1,2,16}
            0x19, 0x07, 0x10,
        ]);
        assert_eq!(
            decode_legacy_reports(&spare),
            Err(Cat001DecodeError::UnknownItem(16))
        );

        // Track record with FRN 21 (RFS): bit 0x02 in octet 3.
        let rfs = block(&[
            0xC1, 0x01, 0x02, // FSPEC {1,2,21}
            0x19, 0x07, 0xA0,
        ]);
        assert_eq!(
            decode_legacy_reports(&rfs),
            Err(Cat001DecodeError::UnknownItem(21))
        );
    }

    /// Unused standard items (extended and explicit formats) are skipped
    /// length-correctly: a following record still decodes. REQ: FR-IO-011
    #[test]
    fn skips_unused_items_length_correctly() {
        let bytes = block(&[
            // Record 1: plot with I001/130 (FRN 6, FX chain of 2 octets) and
            // SP (FRN 20, octet 3 bit 0x04; total length 3).
            0xC5, 0x01, 0x04, // FSPEC {1,2,6,20}
            0x19, 0x07, // I001/010
            0x10, // I001/020 plot, PSR
            0xA1, 0xA0, // I001/130: FX then final octet
            0x03, 0xAA, 0xBB, // SP: total 3 octets
            // Record 2: minimal plot record.
            0xC0, 0x19, 0x07, 0x10,
        ]);
        let reports = decode_legacy_reports(&bytes).expect("decodes");
        assert_eq!(reports.len(), 2, "skip kept the stream in sync");
        assert_eq!(reports[1].detection, Detection::Psr);
    }

    /// Wrong category / length lies are rejected without panic.
    /// REQ: FR-IO-011, NFR-SAFE-002
    #[test]
    fn wrong_category_and_length_mismatch_are_rejected() {
        assert_eq!(
            decode_legacy_reports(&[48, 0x00, 0x03]),
            Err(Cat001DecodeError::WrongCategory(48))
        );
        assert_eq!(
            decode_legacy_reports(&[CATEGORY, 0xFF, 0xFF, 0x00]),
            Err(Cat001DecodeError::LengthMismatch {
                declared: 0xFFFF,
                actual: 4
            })
        );
    }

    /// Every truncation of a valid block errors instead of panicking.
    /// REQ: NFR-SAFE-002
    #[test]
    fn truncations_never_panic() {
        let bytes = block(&[
            0xFA, 0x19, 0x07, 0x30, 0x32, 0x00, 0x40, 0x00, 0x02, 0x8B, 0x05, 0x78, 0x32, 0x00,
        ]);
        for cut in 0..bytes.len() {
            let mut shortened = bytes[..cut].to_vec();
            if shortened.len() >= 3 {
                shortened[1] = 0;
                shortened[2] = shortened.len() as u8;
            }
            let _ = decode_legacy_reports(&shortened);
        }
    }

    /// A hostile FSPEC chaining FX octets past the supported FRN space is
    /// rejected, not panicked on (QW.2 parity). REQ: NFR-SAFE-002
    #[test]
    fn overlong_fspec_chain_is_rejected_not_panicked() {
        let mut bytes = vec![CATEGORY, 0x00, 63];
        bytes.extend_from_slice(&[0xFF; 60]);
        assert_eq!(
            decode_legacy_reports(&bytes),
            Err(Cat001DecodeError::FspecTooLong)
        );
    }

    /// The conversion into the CAT048-shaped report carries every field and
    /// stamps the adapter-expanded time. REQ: FR-IO-011
    #[test]
    fn conversion_into_target_report_carries_fields() {
        let bytes = block(&[
            0xFA, 0x19, 0x07, 0x30, 0x32, 0x00, 0x40, 0x00, 0x02, 0x8B, 0x05, 0x78, 0x32, 0x00,
        ]);
        let legacy = decode_legacy_reports(&bytes).expect("decodes")[0];
        let report = legacy.into_target_report(Some(Timestamp(36_100.0)));
        assert_eq!(report.time, Some(Timestamp(36_100.0)));
        assert_eq!(report.detection, Detection::SsrPsr);
        assert_eq!(report.mode_3a, Some(0o1213));
        assert_eq!(report.flight_level_ft, Some(35_000.0));
        assert!(report.icao_address.is_none() && report.callsign.is_none());
        assert!(report.daps.is_empty());
    }
}
