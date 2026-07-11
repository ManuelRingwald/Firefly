//! CAT020 **input** decode — Multilateration (WAM/MLAT) Target Reports
//! (FEP.5).
//!
//! A **W**ide **A**rea **M**ultilateration system computes aircraft positions
//! from the time-difference-of-arrival of transponder signals at several
//! ground receivers — **independent** surveillance (the aircraft cannot spoof
//! its own position, unlike ADS-B) in airspace where radar is expensive or
//! impossible (mountain valleys, airport surface/terminal areas). ARTAS
//! consumes WAM as CAT020 target reports plus CAT019 system status; this
//! decoder is the CAT020 half.
//!
//! ## What is decoded
//!
//! The track-relevant items: identity (I020/010), **full** time of day
//! (I020/140 — no truncation games like CAT001), the high-resolution WGS-84
//! position (I020/041, LSB 180/2²⁵ °; the Cartesian twin I020/042 is
//! skipped), Mode 3/A (I020/070), flight level (I020/090), geometric height
//! (I020/105), ICAO address (I020/220), callsign (I020/245), track number
//! (I020/161), the report descriptor flags (I020/020: RAB field monitor,
//! SIM/TST, GBS ground, SPI) and — the honest-quality core — the **per-report
//! position standard deviation** from I020/500 SDP (σx/σy, LSB 0.25 m), from
//! which the adapter derives the measurement σ instead of assuming one.
//! Every other item of the UAP is skipped length-correctly.
//!
//! ## Robustness (security-relevant input path)
//!
//! Same policy as the sibling decoders (charter §8): bounds-checked cursor,
//! truncated/corrupt records yield [`Cat020DecodeError`] (datagram dropped,
//! no panic), unknown FRNs and unexpected I020/500 subfield bits are hard
//! errors instead of silent mis-parses. Covered by the `cat020_decode` fuzz
//! target.
//!
//! Items and layouts follow **EUROCONTROL SUR.ET1.ST05.2000-STD-14**
//! ("ASTERIX Category 020 — Multilateration Target Reports", ed 1.x).
//!
//! REQ: FR-IO-012

use firefly_core::{Callsign, Timestamp};

use crate::fspec;

/// The ASTERIX category number for multilateration target reports.
const CATEGORY: u8 = 20;

/// I020/140 Time-of-Day LSB: 1/128 second (24-bit count since UTC midnight).
const TIME_LSB_SECONDS: f64 = 1.0 / 128.0;
/// I020/041 latitude/longitude LSB: 180/2²⁵ degrees (same as CAT062 I062/105).
const POSITION_LSB_DEGREES: f64 = 180.0 / 33_554_432.0;
/// I020/090 LSB: 1/4 flight level = 25 ft.
const FLIGHT_LEVEL_LSB_FT: f64 = 25.0;
/// I020/090 — the flight level lives in the low 14 bits (V/G masked off).
const FLIGHT_LEVEL_MASK: u16 = 0x3FFF;
/// I020/090 sign bit of the 14-bit two's-complement flight level.
const FLIGHT_LEVEL_SIGN: i32 = 0x2000;
/// I020/090 two's-complement modulus (2¹⁴).
const FLIGHT_LEVEL_MODULUS: i32 = 0x4000;
/// I020/105 Geometric Height LSB: 6.25 ft.
const GEOMETRIC_HEIGHT_LSB_FT: f64 = 6.25;
/// I020/070 — the Mode 3/A reply lives in the low 12 bits.
const MODE_3A_CODE_MASK: u16 = 0x0FFF;
/// I020/161 — the track number lives in the low 12 bits.
const TRACK_NUMBER_MASK: u16 = 0x0FFF;
/// I020/500 SDP σx/σy LSB: 0.25 m.
const SDP_SIGMA_LSB_M: f64 = 0.25;
/// I020/020 first extension — Report from field monitor (fixed transponder).
const TRD_EXT_RAB: u8 = 0x80;
/// I020/020 first extension — Special Position Identification.
const TRD_EXT_SPI: u8 = 0x40;
/// I020/020 first extension — Ground Bit Set (transponder on ground).
const TRD_EXT_GBS: u8 = 0x10;
/// I020/020 first extension — Simulated target report.
const TRD_EXT_SIM: u8 = 0x04;
/// I020/020 first extension — Test target report.
const TRD_EXT_TST: u8 = 0x02;
/// I020/500 subfield-spec bits (single spec octet, no FX in ed 1.x).
const ACC_DOP: u8 = 0x80;
const ACC_SDP: u8 = 0x40;
const ACC_SDH: u8 = 0x20;
/// FX bit (lowest bit of an octet): "another octet follows".
const FX: u8 = 0x01;

/// The CAT020 UAP field reference numbers (FRN → data item), per
/// SUR.ET1.ST05.2000-STD-14.
mod uap {
    /// I020/010 — Data Source Identifier (SAC/SIC).
    pub const DATA_SOURCE_IDENTIFIER: u8 = 1;
    /// I020/020 — Target Report Descriptor (technology + status flags).
    pub const TARGET_REPORT_DESCRIPTOR: u8 = 2;
    /// I020/140 — Time of Day.
    pub const TIME_OF_DAY: u8 = 3;
    /// I020/041 — Position in WGS-84 Coordinates (high resolution).
    pub const POSITION_WGS84: u8 = 4;
    /// I020/042 — Position in Cartesian Coordinates.
    pub const POSITION_CARTESIAN: u8 = 5;
    /// I020/161 — Track Number.
    pub const TRACK_NUMBER: u8 = 6;
    /// I020/170 — Track Status.
    pub const TRACK_STATUS: u8 = 7;
    /// I020/070 — Mode-3/A Code.
    pub const MODE_3A_CODE: u8 = 8;
    /// I020/202 — Calculated Track Velocity in Cartesian.
    pub const VELOCITY_CARTESIAN: u8 = 9;
    /// I020/090 — Flight Level in Binary Representation.
    pub const FLIGHT_LEVEL: u8 = 10;
    /// I020/100 — Mode-C Code (code + confidence).
    pub const MODE_C_CODE: u8 = 11;
    /// I020/220 — Target Address (24-bit ICAO).
    pub const TARGET_ADDRESS: u8 = 12;
    /// I020/245 — Target Identification (callsign).
    pub const TARGET_IDENTIFICATION: u8 = 13;
    /// I020/110 — Measured Height (local Cartesian).
    pub const MEASURED_HEIGHT: u8 = 14;
    /// I020/105 — Geometric Height (WGS-84).
    pub const GEOMETRIC_HEIGHT: u8 = 15;
    /// I020/210 — Calculated Acceleration.
    pub const ACCELERATION: u8 = 16;
    /// I020/300 — Vehicle Fleet Identification.
    pub const VEHICLE_FLEET: u8 = 17;
    /// I020/310 — Pre-programmed Message.
    pub const PREPROGRAMMED_MESSAGE: u8 = 18;
    /// I020/500 — Position Accuracy (compound: DOP/SDP/SDH).
    pub const POSITION_ACCURACY: u8 = 19;
    /// I020/400 — Contributing Receivers (repetitive, 1 octet each).
    pub const CONTRIBUTING_RECEIVERS: u8 = 20;
    /// I020/250 — Mode S MB Data (repetitive, 8 octets each).
    pub const MODE_S_MB_DATA: u8 = 21;
    /// I020/230 — Communications/ACAS Capability.
    pub const COMMS_ACAS: u8 = 22;
    /// I020/260 — ACAS Resolution Advisory Report.
    pub const ACAS_RA: u8 = 23;
    /// I020/030 — Warning/Error Conditions (FX-repetitive).
    pub const WARNING_ERROR: u8 = 24;
    /// I020/055 — Mode-1 Code.
    pub const MODE_1_CODE: u8 = 25;
    /// I020/050 — Mode-2 Code.
    pub const MODE_2_CODE: u8 = 26;
    /// RE — Reserved Expansion Field (explicit length).
    pub const RESERVED_EXPANSION: u8 = 27;
    /// SP — Special Purpose Field (explicit length).
    pub const SPECIAL_PURPOSE: u8 = 28;
}

/// How a CAT020 data item is laid out on the wire.
enum ItemFormat {
    /// A fixed number of octets.
    Fixed(usize),
    /// An FX-chained octet chain (variable items and the FX-repetitive
    /// I020/030 — byte-identical on the wire).
    Extended,
    /// A 1-octet repetition factor `REP`, then `REP × n` octets.
    Repetitive(usize),
    /// I020/500 — one subfield-spec octet, then the present subfields.
    CompoundAccuracy,
    /// A 1-octet length indicator giving the item's **total** length — RE/SP.
    Explicit,
}

/// The wire layout of each CAT020 UAP item. `None` for an FRN this decoder
/// does not know — a present unknown FRN cannot be skipped safely.
fn item_format(frn: u8) -> Option<ItemFormat> {
    use uap::*;
    Some(match frn {
        DATA_SOURCE_IDENTIFIER => ItemFormat::Fixed(2),
        TARGET_REPORT_DESCRIPTOR => ItemFormat::Extended,
        TIME_OF_DAY => ItemFormat::Fixed(3),
        POSITION_WGS84 => ItemFormat::Fixed(8),
        POSITION_CARTESIAN => ItemFormat::Fixed(6),
        TRACK_NUMBER => ItemFormat::Fixed(2),
        TRACK_STATUS => ItemFormat::Extended,
        MODE_3A_CODE => ItemFormat::Fixed(2),
        VELOCITY_CARTESIAN => ItemFormat::Fixed(4),
        FLIGHT_LEVEL => ItemFormat::Fixed(2),
        MODE_C_CODE => ItemFormat::Fixed(4),
        TARGET_ADDRESS => ItemFormat::Fixed(3),
        TARGET_IDENTIFICATION => ItemFormat::Fixed(7),
        MEASURED_HEIGHT => ItemFormat::Fixed(2),
        GEOMETRIC_HEIGHT => ItemFormat::Fixed(2),
        ACCELERATION => ItemFormat::Fixed(2),
        VEHICLE_FLEET => ItemFormat::Fixed(1),
        PREPROGRAMMED_MESSAGE => ItemFormat::Fixed(1),
        POSITION_ACCURACY => ItemFormat::CompoundAccuracy,
        CONTRIBUTING_RECEIVERS => ItemFormat::Repetitive(1),
        MODE_S_MB_DATA => ItemFormat::Repetitive(8),
        COMMS_ACAS => ItemFormat::Fixed(2),
        ACAS_RA => ItemFormat::Fixed(7),
        WARNING_ERROR => ItemFormat::Extended,
        MODE_1_CODE => ItemFormat::Fixed(1),
        MODE_2_CODE => ItemFormat::Fixed(2),
        RESERVED_EXPANSION => ItemFormat::Explicit,
        SPECIAL_PURPOSE => ItemFormat::Explicit,
        _ => return None,
    })
}

/// One decoded CAT020 multilateration report — the neutral product of
/// [`decode_mlat_reports`]. Carries the track-relevant items; everything
/// else in the record is skipped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodedMlatReport {
    /// I020/010 — System Area Code of the reporting MLAT system.
    pub sac: u8,
    /// I020/010 — System Identification Code of the reporting MLAT system.
    pub sic: u8,
    /// I020/140 — full time of day (1/128 s since UTC midnight), if present.
    pub time: Option<Timestamp>,
    /// I020/041 — computed position (latitude, longitude) in degrees, if
    /// present.
    pub position_deg: Option<(f64, f64)>,
    /// I020/090 — flight level in **feet**, if present.
    pub flight_level_ft: Option<f64>,
    /// I020/105 — geometric (WGS-84) height in **feet**, if present.
    pub geometric_height_ft: Option<f64>,
    /// I020/070 — Mode-3/A code (octal, low 12 bits), if present.
    pub mode_3a: Option<u16>,
    /// I020/220 — 24-bit ICAO aircraft address, if present.
    pub icao_address: Option<u32>,
    /// I020/245 — aircraft identification (callsign), if present.
    pub callsign: Option<Callsign>,
    /// I020/161 — MLAT track number, if present.
    pub track_number: Option<u16>,
    /// I020/500 SDP — the **per-report** 1σ position uncertainty in metres
    /// (the conservative max of σx/σy), if the system reports one. The honest
    /// quality signal the adapter prefers over any assumption.
    pub sigma_pos_m: Option<f64>,
    /// I020/020 ext RAB — report from a field monitor (fixed test
    /// transponder), never a real aircraft.
    pub field_monitor: bool,
    /// I020/020 ext SIM/TST — simulated or test target report.
    pub simulated_or_test: bool,
    /// I020/020 ext GBS — the transponder reports itself on the ground.
    pub ground_bit: bool,
    /// I020/020 ext SPI — special position identification pulse.
    pub spi: bool,
}

/// Errors that can occur while decoding a CAT020 data block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cat020DecodeError {
    /// The input ended before a complete block/record/item could be read.
    Truncated,
    /// The first octet was not [`CATEGORY`] (20).
    WrongCategory(u8),
    /// The `LEN` field did not match the actual input length.
    LengthMismatch {
        /// Length declared by the block's LEN field.
        declared: usize,
        /// Actual datagram length.
        actual: usize,
    },
    /// The FSPEC marked an FRN present that this decoder cannot length (and
    /// so cannot safely skip) — or I020/500 set a subfield bit outside
    /// DOP/SDP/SDH.
    UnknownItem(u8),
    /// A record's FSPEC was missing the required I020/010.
    MissingItem(u8),
    /// The FSPEC's FX chain ran past [`fspec::MAX_FSPEC_OCTETS`] — malformed.
    FspecTooLong,
}

impl std::fmt::Display for Cat020DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cat020DecodeError::Truncated => write!(f, "input ended before a complete item"),
            Cat020DecodeError::WrongCategory(cat) => write!(f, "expected CAT 20, got CAT {cat}"),
            Cat020DecodeError::LengthMismatch { declared, actual } => write!(
                f,
                "LEN field says {declared} bytes, but input is {actual} bytes"
            ),
            Cat020DecodeError::UnknownItem(frn) => {
                write!(f, "FSPEC marks unknown/unskippable FRN {frn} present")
            }
            Cat020DecodeError::MissingItem(frn) => {
                write!(f, "record is missing required FRN {frn}")
            }
            Cat020DecodeError::FspecTooLong => {
                write!(f, "FSPEC FX chain exceeds the supported FRN space")
            }
        }
    }
}

impl std::error::Error for Cat020DecodeError {}

/// Decode a CAT020 data block (`[CAT=20][LEN][record…]`) into one
/// [`DecodedMlatReport`] per record.
///
/// Returns [`Cat020DecodeError`] (and decodes nothing) on a wrong category,
/// length mismatch, truncation or an unknown present item — the caller drops
/// the datagram and keeps listening. Never panics on input.
pub fn decode_mlat_reports(bytes: &[u8]) -> Result<Vec<DecodedMlatReport>, Cat020DecodeError> {
    if bytes.len() < 3 {
        return Err(Cat020DecodeError::Truncated);
    }
    if bytes[0] != CATEGORY {
        return Err(Cat020DecodeError::WrongCategory(bytes[0]));
    }
    let declared = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    if declared != bytes.len() {
        return Err(Cat020DecodeError::LengthMismatch {
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

/// One record: FSPEC, then the present items in ascending-FRN (UAP) order.
fn decode_record(cursor: &mut Cursor) -> Result<DecodedMlatReport, Cat020DecodeError> {
    let frns = cursor.take_fspec()?;

    let mut sac_sic = None;
    let mut time = None;
    let mut position_deg = None;
    let mut flight_level_ft = None;
    let mut geometric_height_ft = None;
    let mut mode_3a = None;
    let mut icao_address = None;
    let mut callsign = None;
    let mut track_number = None;
    let mut sigma_pos_m = None;
    let mut field_monitor = false;
    let mut simulated_or_test = false;
    let mut ground_bit = false;
    let mut spi = false;

    for frn in frns {
        let format = item_format(frn).ok_or(Cat020DecodeError::UnknownItem(frn))?;
        let bytes = match format {
            ItemFormat::Fixed(n) => cursor.take(n)?,
            ItemFormat::Extended => cursor.take_extended()?,
            ItemFormat::Repetitive(per) => cursor.take_repetitive(per)?,
            ItemFormat::CompoundAccuracy => cursor.take_compound_accuracy()?,
            ItemFormat::Explicit => cursor.take_explicit()?,
        };
        match frn {
            uap::DATA_SOURCE_IDENTIFIER => sac_sic = Some((bytes[0], bytes[1])),
            uap::TARGET_REPORT_DESCRIPTOR => {
                // Octet 1 carries the technology bits (SSR/MS/HF/…); the
                // status flags live in the first extension octet.
                if let Some(ext1) = bytes.get(1) {
                    field_monitor = ext1 & TRD_EXT_RAB != 0;
                    spi = ext1 & TRD_EXT_SPI != 0;
                    ground_bit = ext1 & TRD_EXT_GBS != 0;
                    simulated_or_test = ext1 & (TRD_EXT_SIM | TRD_EXT_TST) != 0;
                }
            }
            uap::TIME_OF_DAY => {
                let ticks = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
                time = Some(Timestamp(ticks as f64 * TIME_LSB_SECONDS));
            }
            uap::POSITION_WGS84 => {
                let lat = i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                let lon = i32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                position_deg = Some((
                    lat as f64 * POSITION_LSB_DEGREES,
                    lon as f64 * POSITION_LSB_DEGREES,
                ));
            }
            uap::MODE_3A_CODE => {
                mode_3a = Some(u16::from_be_bytes([bytes[0], bytes[1]]) & MODE_3A_CODE_MASK);
            }
            uap::FLIGHT_LEVEL => {
                let raw = u16::from_be_bytes([bytes[0], bytes[1]]) & FLIGHT_LEVEL_MASK;
                let mut level = raw as i32;
                if level & FLIGHT_LEVEL_SIGN != 0 {
                    level -= FLIGHT_LEVEL_MODULUS;
                }
                flight_level_ft = Some(level as f64 * FLIGHT_LEVEL_LSB_FT);
            }
            uap::GEOMETRIC_HEIGHT => {
                let raw = i16::from_be_bytes([bytes[0], bytes[1]]);
                geometric_height_ft = Some(raw as f64 * GEOMETRIC_HEIGHT_LSB_FT);
            }
            uap::TARGET_ADDRESS => {
                icao_address = Some(u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]));
            }
            uap::TARGET_IDENTIFICATION => {
                // Octet 1 is STI + spares; octets 2–7 pack eight 6-bit IA-5
                // characters (same layout as CAT062 I062/245).
                callsign = Some(decode_target_identification(&bytes[1..7]));
            }
            uap::TRACK_NUMBER => {
                track_number = Some(u16::from_be_bytes([bytes[0], bytes[1]]) & TRACK_NUMBER_MASK);
            }
            uap::POSITION_ACCURACY => {
                sigma_pos_m = decode_accuracy_sigma(bytes);
            }
            _ => {} // present but not track-relevant — length already consumed
        }
    }

    let (sac, sic) = sac_sic.ok_or(Cat020DecodeError::MissingItem(uap::DATA_SOURCE_IDENTIFIER))?;

    Ok(DecodedMlatReport {
        sac,
        sic,
        time,
        position_deg,
        flight_level_ft,
        geometric_height_ft,
        mode_3a,
        icao_address,
        callsign,
        track_number,
        sigma_pos_m,
        field_monitor,
        simulated_or_test,
        ground_bit,
        spi,
    })
}

/// Extract the conservative 1σ position uncertainty from an already-delimited
/// I020/500 item: `max(σx, σy)` of the SDP subfield, `None` when SDP is
/// absent. `bytes` starts at the subfield-spec octet.
fn decode_accuracy_sigma(bytes: &[u8]) -> Option<f64> {
    let spec = bytes[0];
    let mut pos = 1;
    if spec & ACC_DOP != 0 {
        pos += 6;
    }
    if spec & ACC_SDP != 0 {
        let sigma_x = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as f64 * SDP_SIGMA_LSB_M;
        let sigma_y = u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]) as f64 * SDP_SIGMA_LSB_M;
        return Some(sigma_x.max(sigma_y));
    }
    None
}

/// I020/245 octets 2–7 — eight 6-bit IA-5 codes (same table as CAT048/062).
fn decode_target_identification(bytes: &[u8]) -> Callsign {
    let mut bits: u64 = 0;
    for &b in &bytes[0..6] {
        bits = (bits << 8) | b as u64;
    }
    let mut chars = [0u8; 8];
    for (i, c) in chars.iter_mut().enumerate() {
        let shift = (7 - i) * 6;
        *c = ia5_decode(((bits >> shift) & 0x3F) as u8);
    }
    Callsign(chars)
}

/// Decode one 6-bit ASTERIX IA-5 code to ASCII; undefined codes map to space.
fn ia5_decode(code: u8) -> u8 {
    match code {
        1..=26 => b'A' + (code - 1),
        48..=57 => b'0' + (code - 48),
        _ => b' ',
    }
}

/// A bounds-checked read cursor over a block's record bytes; every `take*`
/// returns [`Cat020DecodeError::Truncated`] rather than panicking.
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

    fn take(&mut self, n: usize) -> Result<&'a [u8], Cat020DecodeError> {
        if self.remaining() < n {
            return Err(Cat020DecodeError::Truncated);
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn take_fspec(&mut self) -> Result<std::collections::BTreeSet<u8>, Cat020DecodeError> {
        let slice = &self.bytes[self.pos..];
        let (frns, consumed) = fspec::parse(slice).map_err(|_| Cat020DecodeError::FspecTooLong)?;
        if consumed == 0 || slice[consumed - 1] & FX != 0 {
            return Err(Cat020DecodeError::Truncated);
        }
        self.pos += consumed;
        Ok(frns)
    }

    fn take_extended(&mut self) -> Result<&'a [u8], Cat020DecodeError> {
        let start = self.pos;
        loop {
            let octet = self.take(1)?[0];
            if octet & FX == 0 {
                break;
            }
        }
        Ok(&self.bytes[start..self.pos])
    }

    fn take_repetitive(&mut self, per: usize) -> Result<&'a [u8], Cat020DecodeError> {
        let start = self.pos;
        let rep = self.take(1)?[0] as usize;
        self.take(rep * per)?;
        Ok(&self.bytes[start..self.pos])
    }

    /// Take I020/500: one subfield-spec octet (DOP `0x80` = 6 octets, SDP
    /// `0x40` = 6, SDH `0x20` = 2; ed 1.x defines no FX chain here), then the
    /// present subfields. A set spare/FX bit cannot be lengthed → rejected.
    fn take_compound_accuracy(&mut self) -> Result<&'a [u8], Cat020DecodeError> {
        let start = self.pos;
        let spec = self.take(1)?[0];
        if spec & 0x1F != 0 {
            return Err(Cat020DecodeError::UnknownItem(uap::POSITION_ACCURACY));
        }
        let mut len = 0usize;
        if spec & ACC_DOP != 0 {
            len += 6;
        }
        if spec & ACC_SDP != 0 {
            len += 6;
        }
        if spec & ACC_SDH != 0 {
            len += 2;
        }
        self.take(len)?;
        Ok(&self.bytes[start..self.pos])
    }

    fn take_explicit(&mut self) -> Result<&'a [u8], Cat020DecodeError> {
        let start = self.pos;
        let total = self.take(1)?[0] as usize;
        if total == 0 {
            return Err(Cat020DecodeError::Truncated);
        }
        self.take(total - 1)?;
        Ok(&self.bytes[start..self.pos])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrap record bytes in a CAT020 block envelope.
    fn block(records: &[u8]) -> Vec<u8> {
        let mut out = vec![CATEGORY, 0x00, (3 + records.len()) as u8];
        out.extend_from_slice(records);
        out
    }

    /// A hand-built airborne WAM report decodes field-exactly — the CAT020
    /// reference vector. FSPEC {1,2,3,4,8,12} = [0xF1,0x11,0x00 → …]:
    /// FRN 1,2,3,4 in octet 1 (0xF0), FRN 8 in octet 2 (0x80), FRN 12 in
    /// octet 2 (0x08). Position 45°/11.25°; time 3600 s; Mode 3/A 1213₈.
    /// REQ: FR-IO-012
    #[test]
    fn airborne_report_matches_reference_vector() {
        let bytes = block(&[
            0xF1, 0x88, // FSPEC {1,2,3,4} + {8,12}
            0x19, 0x0A, // I020/010 SAC=25 SIC=10
            0x41, 0x00, // I020/020 MS + FX; ext1: no flags, FX clear
            0x07, 0x08, 0x00, // I020/140 time 3600 s
            0x00, 0x80, 0x00, 0x00, // I020/041 lat 45° (2²⁵/4 LSBs)
            0x00, 0x20, 0x00, 0x00, // I020/041 lon 11.25°
            0x02, 0x8B, // I020/070 Mode 3/A 1213₈
            0x3C, 0x65, 0x89, // I020/220 ICAO
        ]);
        let reports = decode_mlat_reports(&bytes).expect("decodes");
        assert_eq!(reports.len(), 1);
        let r = reports[0];
        assert_eq!((r.sac, r.sic), (25, 10));
        assert_eq!(r.time, Some(Timestamp(3600.0)));
        let (lat, lon) = r.position_deg.expect("position");
        assert!((lat - 45.0).abs() < 1e-6);
        assert!((lon - 11.25).abs() < 1e-6);
        assert_eq!(r.mode_3a, Some(0o1213));
        assert_eq!(r.icao_address, Some(0x3C_6589));
        assert!(!r.ground_bit && !r.simulated_or_test && !r.field_monitor);
    }

    /// I020/500 SDP yields the conservative per-report σ (max of σx/σy);
    /// DOP/SDH present are skipped length-correctly around it.
    /// REQ: FR-IO-012
    #[test]
    fn position_accuracy_yields_sigma() {
        // FSPEC {1,19}: FRN 19 lives in octet 3 (15..21) at bit 0x08.
        let bytes = block(&[
            0x81, 0x01, 0x08, // FSPEC {1,19}
            0x19, 0x0A, // I020/010
            0xE0, // I020/500 spec: DOP+SDP+SDH
            0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, // DOP (skipped)
            0x00, 0x50, 0x00, 0xC8, 0x00, 0x00, // SDP σx=20m σy=50m ρ=0
            0x12, 0x34, // SDH (skipped)
        ]);
        let reports = decode_mlat_reports(&bytes).expect("decodes");
        assert_eq!(reports[0].sigma_pos_m, Some(50.0), "max(20, 50)");

        // SDP absent → no σ claimed.
        let no_sdp = block(&[
            0x81, 0x01, 0x08, //
            0x19, 0x0A, //
            0x20, 0x12, 0x34, // I020/500 spec: SDH only
        ]);
        assert_eq!(
            decode_mlat_reports(&no_sdp).expect("decodes")[0].sigma_pos_m,
            None
        );

        // A spare/FX bit in the accuracy spec is a hard error.
        let bad_spec = block(&[0x81, 0x01, 0x08, 0x19, 0x0A, 0x41]);
        assert_eq!(
            decode_mlat_reports(&bad_spec),
            Err(Cat020DecodeError::UnknownItem(19))
        );
    }

    /// Identity, altitude and status flags decode: callsign (STI octet +
    /// 6-bit chars), FL, geometric height, track number, descriptor flags.
    /// REQ: FR-IO-012
    #[test]
    fn identity_altitude_and_flags_decode() {
        // FSPEC {1,2,6,10,13,15}: octet1 {1,2,6}=0xC4|FX, octet2 {10,13}=
        // FRN 8..14 → 10=0x20, 13=0x04, |FX; octet3 {15}=0x80.
        let bytes = block(&[
            0xC5, 0x25, 0x80, // FSPEC {1,2,6,10,13,15}
            0x19, 0x0A, // I020/010
            0x41, 0x52, // I020/020 MS + ext1: SPI|GBS|TST → 0x40|0x10|0x02
            0x00, 0x2A, // I020/161 track 42
            0x05, 0x78, // I020/090 FL350
            0x00, 0x10, 0xC2, 0x31, 0xCB, 0x38, 0x20, // I020/245 STI + "DLH123  "
            0x06, 0x40, // I020/105 1600 × 6.25 ft = 10 000 ft
        ]);
        let reports = decode_mlat_reports(&bytes).expect("decodes");
        let r = reports[0];
        assert_eq!(r.track_number, Some(42));
        assert_eq!(r.flight_level_ft, Some(35_000.0));
        assert_eq!(r.geometric_height_ft, Some(10_000.0));
        assert_eq!(r.callsign, Some(Callsign::new("DLH123")));
        assert!(r.spi && r.ground_bit && r.simulated_or_test);
        assert!(!r.field_monitor);
    }

    /// Unused items (Cartesian position/velocity, repetitive receivers +
    /// Mode S MB, FX chains, RE/SP) are skipped length-correctly: a following
    /// record still decodes. REQ: FR-IO-012
    #[test]
    fn skips_unused_items_length_correctly() {
        let bytes = block(&[
            // Record 1: {1,5,9} octet1 = 0x80|0x08|FX? FRN5=0x08, no: octet1
            // bits: 1=0x80,2=0x40,3=0x20,4=0x10,5=0x08,6=0x04,7=0x02.
            // {1,5} = 0x88; FRN 9 → octet2 0x40; FRN 20,21 → octet3:
            // 20=0x04, 21=0x02. Use {1,5,9,20,21}.
            0x89, 0x41, 0x06, // FSPEC {1,5,9,20,21}
            0x19, 0x0A, // I020/010
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, // I020/042 (skipped)
            0xAA, 0xBB, 0xCC, 0xDD, // I020/202 (skipped)
            0x02, 0x11, 0x22, // I020/400 REP=2 receivers (skipped)
            0x01, 0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22, 0x40, // I020/250 REP=1 (skipped)
            // Record 2: minimal.
            0x80, 0x19, 0x0A,
        ]);
        let reports = decode_mlat_reports(&bytes).expect("decodes");
        assert_eq!(reports.len(), 2, "skip kept the stream in sync");
        assert_eq!((reports[1].sac, reports[1].sic), (25, 10));
    }

    /// An unknown FRN is a hard error, not a silent mis-parse.
    /// REQ: FR-IO-012, NFR-SAFE-002
    #[test]
    fn unknown_frn_is_rejected() {
        // FRN 29 lives in octet 5 (29..35) at bit 0x80.
        let bytes = block(&[
            0x81, 0x01, 0x01, 0x01, 0x80, // FSPEC {1,29}
            0x19, 0x0A,
        ]);
        assert_eq!(
            decode_mlat_reports(&bytes),
            Err(Cat020DecodeError::UnknownItem(29))
        );
    }

    /// Wrong category / length lies / truncations / hostile FSPEC chains are
    /// rejected without panic. REQ: FR-IO-012, NFR-SAFE-002
    #[test]
    fn malformed_input_is_rejected_not_panicked() {
        assert_eq!(
            decode_mlat_reports(&[21, 0x00, 0x03]),
            Err(Cat020DecodeError::WrongCategory(21))
        );
        assert_eq!(
            decode_mlat_reports(&[CATEGORY, 0xFF, 0xFF, 0x00]),
            Err(Cat020DecodeError::LengthMismatch {
                declared: 0xFFFF,
                actual: 4
            })
        );

        let bytes = block(&[
            0xF1, 0x88, 0x19, 0x0A, 0x41, 0x00, 0x07, 0x08, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00,
            0x20, 0x00, 0x00, 0x02, 0x8B, 0x3C, 0x65, 0x89,
        ]);
        for cut in 0..bytes.len() {
            let mut shortened = bytes[..cut].to_vec();
            if shortened.len() >= 3 {
                shortened[1] = 0;
                shortened[2] = shortened.len() as u8;
            }
            let _ = decode_mlat_reports(&shortened);
        }

        let mut hostile = vec![CATEGORY, 0x00, 63];
        hostile.extend_from_slice(&[0xFF; 60]);
        assert_eq!(
            decode_mlat_reports(&hostile),
            Err(Cat020DecodeError::FspecTooLong)
        );
    }
}
