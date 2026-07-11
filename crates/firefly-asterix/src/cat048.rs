//! CAT048 **input** decode — Monoradar Target Reports (ADR 0028).
//!
//! Where [`cat062`](crate::cat062) *encodes* the tracker's output, this module
//! *decodes* the **input** of a real monoradar sensor: ASTERIX **CAT048**
//! ("Monoradar Target Reports", EUROCONTROL SUR.ET1.ST05.2000-STD-04-01). A
//! CAT048 data block carries one record per detection; each record is a polar
//! plot (RHO/THETA relative to the radar) plus optional Mode-3/A, Mode-C flight
//! level and Mode-S identity — exactly the raw material the tracker consumes
//! (the simulator's `radar.rs` produces the same shape internally).
//!
//! ## Robustness (security-relevant input path)
//!
//! The decoder reads **untrusted network datagrams** (charter §8). It is
//! length-checked end to end and **never panics on input**: every read goes
//! through a bounds-checked [`Cursor`], a truncated/corrupt record yields a
//! [`Cat048DecodeError`] (the datagram is dropped, the listener continues), and
//! a present-but-unknown FRN is a hard error rather than a silent mis-parse.
//!
//! ## Scope
//!
//! The **plot-relevant** items are decoded (I048/010, /140, /020, /040, /070,
//! /090, /220, /240, /161); every other standard CAT048 item is **skipped
//! length-correctly** via a complete per-FRN format model (fixed / extended /
//! repetitive / compound / explicit). Firefly does not need their contents to
//! form a plot (ADR 0028). A vocabulary-foreign FRN → [`Cat048DecodeError::UnknownItem`].
//!
//! REQ: FR-NET-013, FR-IO-005

use std::collections::BTreeSet;

use firefly_core::{Callsign, Daps, Timestamp};
use firefly_geo::Polar;

use crate::bds;
use crate::fspec;

/// The ASTERIX category number for monoradar target reports.
const CATEGORY: u8 = 48;

/// I048/140 Time-of-Day LSB: 1/128 second (24-bit count since UTC midnight).
const TIME_LSB_SECONDS: f64 = 1.0 / 128.0;
/// I048/040 RHO LSB: 1/256 nautical mile.
const RHO_LSB_NM: f64 = 1.0 / 256.0;
/// Nautical mile in metres (RHO is reported in NM, the tracker works in metres).
const NM_TO_METRES: f64 = 1852.0;
/// I048/040 THETA LSB: 360/2¹⁶ degrees (a full turn over the 16-bit range).
const THETA_LSB_DEGREES: f64 = 360.0 / 65536.0;
/// I048/090 Flight Level LSB: 1/4 FL. One FL is 100 ft, so the LSB is 25 ft.
const FLIGHT_LEVEL_LSB_FT: f64 = 25.0;
/// I048/070 carries the Mode 3/A reply in its low 12 bits (4 octal digits); the
/// upper bits (V/G/L/spare) are status flags, masked off here.
const MODE_3A_CODE_MASK: u16 = 0x0FFF;
/// I048/161 carries the track number in its low 12 bits.
const TRACK_NUMBER_MASK: u16 = 0x0FFF;
/// I048/090 level field occupies the low 14 bits (bits 16/15 are V/G).
const FLIGHT_LEVEL_MASK: u16 = 0x3FFF;
/// Sign bit of the 14-bit two's-complement flight level.
const FLIGHT_LEVEL_SIGN: i32 = 0x2000;
/// Modulus to sign-extend the 14-bit flight level into `i32`.
const FLIGHT_LEVEL_MODULUS: i32 = 0x4000;
/// FX bit (lowest bit of an octet): "another octet follows".
const FX: u8 = 0x01;
/// I048/020 octet 1, bit 3: **SPI** — Special Position Identification, the
/// transponder "ident" pulse (octet layout: TYP bits 8–6, SIM 5, RDP 4,
/// SPI 3, RAB 2, FX 1).
const TRD_SPI: u8 = 0x04;

/// The CAT048 UAP field reference numbers (FRN → data item), per
/// SUR.ET1.ST05.2000-STD-04-01. Named in one place so the format model and the
/// decode dispatch agree on the bit layout.
mod uap {
    /// I048/010 — Data Source Identifier (SAC/SIC).
    pub const DATA_SOURCE_IDENTIFIER: u8 = 1;
    /// I048/140 — Time of Day.
    pub const TIME_OF_DAY: u8 = 2;
    /// I048/020 — Target Report Descriptor (TYP/SIM/RDP/SPI/RAB…).
    pub const TARGET_REPORT_DESCRIPTOR: u8 = 3;
    /// I048/040 — Measured Position in Polar Coordinates (RHO/THETA).
    pub const MEASURED_POSITION_POLAR: u8 = 4;
    /// I048/070 — Mode-3/A Code in Octal.
    pub const MODE_3A_CODE: u8 = 5;
    /// I048/090 — Flight Level in Binary (Mode C).
    pub const FLIGHT_LEVEL: u8 = 6;
    /// I048/130 — Radar Plot Characteristics (compound).
    pub const RADAR_PLOT_CHARACTERISTICS: u8 = 7;
    /// I048/220 — Aircraft Address (24-bit ICAO).
    pub const AIRCRAFT_ADDRESS: u8 = 8;
    /// I048/240 — Aircraft Identification (callsign).
    pub const AIRCRAFT_IDENTIFICATION: u8 = 9;
    /// I048/250 — Mode S MB Data (repetitive).
    pub const MODE_S_MB_DATA: u8 = 10;
    /// I048/161 — Track Number.
    pub const TRACK_NUMBER: u8 = 11;
    /// I048/042 — Calculated Position in Cartesian.
    pub const CALCULATED_POSITION_CARTESIAN: u8 = 12;
    /// I048/200 — Calculated Track Velocity in Polar.
    pub const CALCULATED_VELOCITY_POLAR: u8 = 13;
    /// I048/170 — Track Status (extended).
    pub const TRACK_STATUS: u8 = 14;
    /// I048/210 — Track Quality.
    pub const TRACK_QUALITY: u8 = 15;
    /// I048/030 — Warning/Error Conditions (extended).
    pub const WARNING_ERROR_CONDITIONS: u8 = 16;
    /// I048/080 — Mode-3/A Confidence.
    pub const MODE_3A_CONFIDENCE: u8 = 17;
    /// I048/100 — Mode-C Code and Confidence.
    pub const MODE_C_CODE_CONFIDENCE: u8 = 18;
    /// I048/110 — Height Measured by 3D Radar.
    pub const HEIGHT_3D: u8 = 19;
    /// I048/120 — Radial Doppler Speed (compound).
    pub const RADIAL_DOPPLER_SPEED: u8 = 20;
    /// I048/230 — Communications / ACAS Capability.
    pub const COMMS_ACAS_CAPABILITY: u8 = 21;
    /// I048/260 — ACAS Resolution Advisory Report.
    pub const ACAS_RA_REPORT: u8 = 22;
    /// I048/055 — Mode-1 Code.
    pub const MODE_1_CODE: u8 = 23;
    /// I048/050 — Mode-2 Code.
    pub const MODE_2_CODE: u8 = 24;
    /// I048/065 — Mode-1 Confidence.
    pub const MODE_1_CONFIDENCE: u8 = 25;
    /// I048/060 — Mode-2 Confidence.
    pub const MODE_2_CONFIDENCE: u8 = 26;
    /// I048/SP — Special Purpose Field (explicit length).
    pub const SPECIAL_PURPOSE: u8 = 27;
    /// I048/RE — Reserved Expansion Field (explicit length).
    pub const RESERVED_EXPANSION: u8 = 28;
}

/// How a CAT048 data item is laid out on the wire, so the decoder can consume
/// (and, for the items it doesn't interpret, *skip*) exactly its bytes.
enum ItemFormat {
    /// A fixed number of octets.
    Fixed(usize),
    /// An FX-chained sequence of octets (each but the last sets the FX bit).
    Extended,
    /// A 1-octet repetition factor `REP`, then `REP × n` octets.
    Repetitive(usize),
    /// I048/130: an FX-chained primary subfield, then one octet per set
    /// presence bit (all subfields are a single octet).
    Compound130,
    /// I048/120: a 1-octet primary subfield — CAL (2 octets) if bit 8 is set,
    /// then RDS (a `REP`-octet repetition of 6-octet groups) if bit 7 is set.
    Compound120,
    /// A 1-octet length indicator giving the item's **total** length (including
    /// the indicator), as used by the Special Purpose / Reserved Expansion fields.
    Explicit,
}

/// The wire layout of each CAT048 UAP item. `None` for an FRN this decoder does
/// not know — a present unknown FRN cannot be skipped safely (no length), so it
/// is reported as [`Cat048DecodeError::UnknownItem`].
fn item_format(frn: u8) -> Option<ItemFormat> {
    use uap::*;
    Some(match frn {
        DATA_SOURCE_IDENTIFIER => ItemFormat::Fixed(2),
        TIME_OF_DAY => ItemFormat::Fixed(3),
        TARGET_REPORT_DESCRIPTOR => ItemFormat::Extended,
        MEASURED_POSITION_POLAR => ItemFormat::Fixed(4),
        MODE_3A_CODE => ItemFormat::Fixed(2),
        FLIGHT_LEVEL => ItemFormat::Fixed(2),
        RADAR_PLOT_CHARACTERISTICS => ItemFormat::Compound130,
        AIRCRAFT_ADDRESS => ItemFormat::Fixed(3),
        AIRCRAFT_IDENTIFICATION => ItemFormat::Fixed(6),
        MODE_S_MB_DATA => ItemFormat::Repetitive(8),
        TRACK_NUMBER => ItemFormat::Fixed(2),
        CALCULATED_POSITION_CARTESIAN => ItemFormat::Fixed(4),
        CALCULATED_VELOCITY_POLAR => ItemFormat::Fixed(4),
        TRACK_STATUS => ItemFormat::Extended,
        TRACK_QUALITY => ItemFormat::Fixed(4),
        WARNING_ERROR_CONDITIONS => ItemFormat::Extended,
        MODE_3A_CONFIDENCE => ItemFormat::Fixed(2),
        MODE_C_CODE_CONFIDENCE => ItemFormat::Fixed(4),
        HEIGHT_3D => ItemFormat::Fixed(2),
        RADIAL_DOPPLER_SPEED => ItemFormat::Compound120,
        COMMS_ACAS_CAPABILITY => ItemFormat::Fixed(2),
        ACAS_RA_REPORT => ItemFormat::Fixed(7),
        MODE_1_CODE => ItemFormat::Fixed(1),
        MODE_2_CODE => ItemFormat::Fixed(2),
        MODE_1_CONFIDENCE => ItemFormat::Fixed(1),
        MODE_2_CONFIDENCE => ItemFormat::Fixed(2),
        SPECIAL_PURPOSE => ItemFormat::Explicit,
        RESERVED_EXPANSION => ItemFormat::Explicit,
        _ => return None,
    })
}

/// The kind of detection a CAT048 record reports, decoded from I048/020's TYP
/// field (octet 1, bits 8–6). Carries enough to derive the tracker's
/// `DetectionKind`/`SourceKind` (done by the radar adapter — this crate stays
/// free of the mapping policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detection {
    /// TYP=000 — no detection (e.g. a track-only update). Not a plot.
    NoDetection,
    /// TYP=001 — single PSR (primary skin paint).
    Psr,
    /// TYP=010 — single SSR (Mode A/C reply).
    Ssr,
    /// TYP=011 — combined SSR + PSR dwell.
    SsrPsr,
    /// TYP=100 — Mode S All-Call.
    ModeSAllCall,
    /// TYP=101 — Mode S Roll-Call.
    ModeSRollCall,
    /// TYP=110 — Mode S All-Call + PSR.
    ModeSAllCallPsr,
    /// TYP=111 — Mode S Roll-Call + PSR.
    ModeSRollCallPsr,
}

impl Detection {
    /// Decode the 3-bit TYP field (the top three bits of I048/020 octet 1).
    fn from_typ(typ: u8) -> Self {
        match typ & 0b111 {
            0b000 => Detection::NoDetection,
            0b001 => Detection::Psr,
            0b010 => Detection::Ssr,
            0b011 => Detection::SsrPsr,
            0b100 => Detection::ModeSAllCall,
            0b101 => Detection::ModeSRollCall,
            0b110 => Detection::ModeSAllCallPsr,
            _ => Detection::ModeSRollCallPsr,
        }
    }

    /// Whether the dwell included a **primary** (PSR) detection.
    pub fn has_primary(self) -> bool {
        matches!(
            self,
            Detection::Psr
                | Detection::SsrPsr
                | Detection::ModeSAllCallPsr
                | Detection::ModeSRollCallPsr
        )
    }

    /// Whether the dwell included a **secondary** (SSR / Mode S) reply.
    pub fn has_secondary(self) -> bool {
        !matches!(self, Detection::NoDetection | Detection::Psr)
    }

    /// Whether the reply was a **Mode S** selective interrogation (vs. classic SSR).
    pub fn is_mode_s(self) -> bool {
        matches!(
            self,
            Detection::ModeSAllCall
                | Detection::ModeSRollCall
                | Detection::ModeSAllCallPsr
                | Detection::ModeSRollCallPsr
        )
    }
}

/// One decoded CAT048 target report — the neutral product of [`decode_target_reports`].
///
/// Carries the **plot-relevant** items; everything else in the record is
/// skipped. The radar adapter ([`firefly-radar`](../firefly_radar/index.html))
/// turns this into a [`firefly_core::Plot`] using the configured radar site
/// position (CAT048 is polar relative to the radar and carries no site).
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedTargetReport {
    /// I048/010 — System Area Code of the reporting radar.
    pub sac: u8,
    /// I048/010 — System Identification Code of the reporting radar.
    pub sic: u8,
    /// I048/140 — time of day (1/128 s since UTC midnight), if present.
    pub time: Option<Timestamp>,
    /// I048/020 — the detection kind (TYP).
    pub detection: Detection,
    /// I048/040 — measured position in polar coordinates (range in **metres**,
    /// azimuth in radians clockwise from north), if present. Absent on a
    /// track-only update.
    pub position: Option<Polar>,
    /// I048/070 — Mode-3/A code (octal, low 12 bits), if present.
    pub mode_3a: Option<u16>,
    /// I048/090 — Mode-C flight level in **feet**, if present.
    pub flight_level_ft: Option<f64>,
    /// I048/220 — 24-bit ICAO aircraft address, if present.
    pub icao_address: Option<u32>,
    /// I048/240 — aircraft identification (callsign), if present.
    pub callsign: Option<Callsign>,
    /// I048/161 — radar track number, if present.
    pub track_number: Option<u16>,
    /// I048/020 SPI — Special Position Identification ("ident" pulse) present
    /// in this report. `false` when I048/020 is absent.
    pub spi: bool,
    /// I048/250 — Downlink Aircraft Parameters decoded from the report's
    /// Mode S EHS BDS registers (4,0 / 5,0 / 6,0; FEP.2). Empty when
    /// I048/250 is absent or carries only other registers.
    pub daps: Daps,
}

/// Errors that can occur while decoding a CAT048 data block. Mirrors
/// [`crate::DecodeError`] (CAT062) but is a distinct type — the two categories
/// have different UAPs and required-item rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cat048DecodeError {
    /// The input ended before a complete block/record/item could be read.
    Truncated,
    /// The first octet was not [`CATEGORY`] (48).
    WrongCategory(u8),
    /// The `LEN` field did not match the actual input length.
    LengthMismatch {
        /// Length declared by the block's LEN field.
        declared: usize,
        /// Actual datagram length.
        actual: usize,
    },
    /// The FSPEC marked an FRN present that this decoder cannot length (and so
    /// cannot safely skip).
    UnknownItem(u8),
    /// A record's FSPEC was missing an item this decoder requires (I048/010).
    MissingItem(u8),
    /// The FSPEC's FX chain ran past [`fspec::MAX_FSPEC_OCTETS`] — malformed
    /// (no CAT048 item lives that far into the UAP). Fuzzing regression, QW.2.
    FspecTooLong,
}

impl std::fmt::Display for Cat048DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cat048DecodeError::Truncated => write!(f, "input ended before a complete item"),
            Cat048DecodeError::WrongCategory(cat) => write!(f, "expected CAT 48, got CAT {cat}"),
            Cat048DecodeError::LengthMismatch { declared, actual } => write!(
                f,
                "LEN field says {declared} bytes, but input is {actual} bytes"
            ),
            Cat048DecodeError::UnknownItem(frn) => {
                write!(f, "FSPEC marks unknown FRN {frn} present")
            }
            Cat048DecodeError::MissingItem(frn) => {
                write!(f, "record is missing required FRN {frn}")
            }
            Cat048DecodeError::FspecTooLong => {
                write!(f, "FSPEC FX chain exceeds the supported FRN space")
            }
        }
    }
}

impl std::error::Error for Cat048DecodeError {}

/// Decode a CAT048 data block (`[CAT=48][LEN][record…]`) into one
/// [`DecodedTargetReport`] per record.
///
/// Returns [`Cat048DecodeError`] (and decodes nothing) on a wrong category,
/// length mismatch, truncation or an unknown present item — the caller drops
/// the datagram and keeps listening. Never panics on input.
pub fn decode_target_reports(bytes: &[u8]) -> Result<Vec<DecodedTargetReport>, Cat048DecodeError> {
    if bytes.len() < 3 {
        return Err(Cat048DecodeError::Truncated);
    }
    if bytes[0] != CATEGORY {
        return Err(Cat048DecodeError::WrongCategory(bytes[0]));
    }
    let declared = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    if declared != bytes.len() {
        return Err(Cat048DecodeError::LengthMismatch {
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

/// One record: FSPEC, then the present items in ascending-FRN (UAP) order. Items
/// in the plot-relevant subset are decoded; all others are skipped by their
/// format. A present FRN with no known format is a hard error.
fn decode_record(cursor: &mut Cursor) -> Result<DecodedTargetReport, Cat048DecodeError> {
    let frns = cursor.take_fspec()?;

    let mut sac_sic = None;
    let mut time = None;
    let mut detection = None;
    let mut position = None;
    let mut mode_3a = None;
    let mut flight_level_ft = None;
    let mut icao_address = None;
    let mut callsign = None;
    let mut track_number = None;
    let mut spi = false;
    let mut daps = Daps::default();

    for frn in frns {
        let format = item_format(frn).ok_or(Cat048DecodeError::UnknownItem(frn))?;
        let bytes = match format {
            ItemFormat::Fixed(n) => cursor.take(n)?,
            ItemFormat::Extended => cursor.take_extended()?,
            ItemFormat::Repetitive(per) => cursor.take_repetitive(per)?,
            ItemFormat::Compound130 => cursor.take_compound_130()?,
            ItemFormat::Compound120 => cursor.take_compound_120()?,
            ItemFormat::Explicit => cursor.take_explicit()?,
        };
        match frn {
            uap::DATA_SOURCE_IDENTIFIER => sac_sic = Some((bytes[0], bytes[1])),
            uap::TIME_OF_DAY => time = Some(decode_time_of_day(bytes)),
            uap::TARGET_REPORT_DESCRIPTOR => {
                detection = Some(Detection::from_typ(bytes[0] >> 5));
                spi = bytes[0] & TRD_SPI != 0;
            }
            uap::MEASURED_POSITION_POLAR => position = Some(decode_polar(bytes)),
            uap::MODE_3A_CODE => mode_3a = Some(decode_mode_3a(bytes)),
            uap::FLIGHT_LEVEL => flight_level_ft = Some(decode_flight_level(bytes)),
            uap::AIRCRAFT_ADDRESS => icao_address = Some(decode_aircraft_address(bytes)),
            uap::AIRCRAFT_IDENTIFICATION => callsign = Some(decode_aircraft_id(bytes)),
            uap::TRACK_NUMBER => {
                track_number = Some(u16::from_be_bytes([bytes[0], bytes[1]]) & TRACK_NUMBER_MASK)
            }
            uap::MODE_S_MB_DATA => {
                // I048/250: [REP] then REP × (7-octet MB field + 1-octet BDS
                // register number). Each EHS register merges its valid fields
                // into the report's DAPs (FEP.2); unknown registers decode to
                // nothing. The repetitive framing already delimited `bytes`.
                for entry in bytes[1..].chunks_exact(8) {
                    let mb: [u8; 7] = entry[..7].try_into().expect("chunk is 8 octets");
                    daps.merge_from(&bds::decode_register(entry[7], &mb));
                }
            }
            _ => {} // present but not plot-relevant — length already consumed
        }
    }

    let (sac, sic) = sac_sic.ok_or(Cat048DecodeError::MissingItem(uap::DATA_SOURCE_IDENTIFIER))?;

    Ok(DecodedTargetReport {
        sac,
        sic,
        time,
        // A record with no I048/020 is treated as "no detection" rather than an
        // error — the adapter then forms no plot.
        detection: detection.unwrap_or(Detection::NoDetection),
        position,
        mode_3a,
        flight_level_ft,
        icao_address,
        callsign,
        track_number,
        spi,
        daps,
    })
}

/// I048/140 — 24-bit count of 1/128-second ticks since UTC midnight.
fn decode_time_of_day(bytes: &[u8]) -> Timestamp {
    let ticks = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
    Timestamp(ticks as f64 * TIME_LSB_SECONDS)
}

/// I048/040 — RHO (1/256 NM → metres) and THETA (360/2¹⁶ ° → radians).
fn decode_polar(bytes: &[u8]) -> Polar {
    let rho_m = u16::from_be_bytes([bytes[0], bytes[1]]) as f64 * RHO_LSB_NM * NM_TO_METRES;
    let theta_deg = u16::from_be_bytes([bytes[2], bytes[3]]) as f64 * THETA_LSB_DEGREES;
    Polar::new(rho_m, theta_deg.to_radians(), 0.0)
}

/// I048/070 — the Mode 3/A reply in the low 12 bits (V/G/L flags masked off).
fn decode_mode_3a(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]]) & MODE_3A_CODE_MASK
}

/// I048/090 — a 14-bit two's-complement flight level in 1/4-FL (25-ft) steps,
/// the V/G bits masked off and the sign extended.
fn decode_flight_level(bytes: &[u8]) -> f64 {
    let raw = u16::from_be_bytes([bytes[0], bytes[1]]) & FLIGHT_LEVEL_MASK;
    let mut level = raw as i32;
    if level & FLIGHT_LEVEL_SIGN != 0 {
        level -= FLIGHT_LEVEL_MODULUS;
    }
    level as f64 * FLIGHT_LEVEL_LSB_FT
}

/// I048/220 — a 24-bit ICAO address, zero-extended into `u32`.
fn decode_aircraft_address(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]])
}

/// I048/240 — six octets packing eight 6-bit IA-5 codes (no leading STI octet,
/// unlike CAT062 I062/245).
fn decode_aircraft_id(bytes: &[u8]) -> Callsign {
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

/// Decode one 6-bit ASTERIX IA-5 code to ASCII; undefined codes map to space
/// (a foreign/garbled datagram must not panic). Mirrors CAT062's table.
fn ia5_decode(code: u8) -> u8 {
    match code {
        1..=26 => b'A' + (code - 1),
        48..=57 => b'0' + (code - 48),
        _ => b' ',
    }
}

/// A bounds-checked read cursor over a record's bytes. Every `take*` method
/// returns [`Cat048DecodeError::Truncated`] rather than panicking when the input
/// is too short — the foundation of the "no panic on input" guarantee.
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
    fn take(&mut self, n: usize) -> Result<&'a [u8], Cat048DecodeError> {
        if self.remaining() < n {
            return Err(Cat048DecodeError::Truncated);
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    /// Parse the FSPEC at the cursor, returning the present FRNs. A trailing FX
    /// bit with no following octet means the FSPEC was cut short (truncated).
    fn take_fspec(&mut self) -> Result<BTreeSet<u8>, Cat048DecodeError> {
        let slice = &self.bytes[self.pos..];
        let (frns, consumed) = fspec::parse(slice).map_err(|_| Cat048DecodeError::FspecTooLong)?;
        if consumed == 0 || slice[consumed - 1] & FX != 0 {
            return Err(Cat048DecodeError::Truncated);
        }
        self.pos += consumed;
        Ok(frns)
    }

    /// Take an FX-chained item: octets while the FX bit stays set (at least one).
    fn take_extended(&mut self) -> Result<&'a [u8], Cat048DecodeError> {
        let start = self.pos;
        loop {
            let octet = self.take(1)?[0];
            if octet & FX == 0 {
                break;
            }
        }
        Ok(&self.bytes[start..self.pos])
    }

    /// Take a repetitive item: a 1-octet repetition factor, then `rep × per` octets.
    fn take_repetitive(&mut self, per: usize) -> Result<&'a [u8], Cat048DecodeError> {
        let start = self.pos;
        let rep = self.take(1)?[0] as usize;
        self.take(rep * per)?;
        Ok(&self.bytes[start..self.pos])
    }

    /// Take I048/130: an FX-chained primary subfield, then one octet per set
    /// presence bit (the seven high bits of each primary octet; bit 1 is FX).
    fn take_compound_130(&mut self) -> Result<&'a [u8], Cat048DecodeError> {
        let start = self.pos;
        let mut subfields = 0usize;
        loop {
            let octet = self.take(1)?[0];
            subfields += (octet & 0xFE).count_ones() as usize;
            if octet & FX == 0 {
                break;
            }
        }
        self.take(subfields)?;
        Ok(&self.bytes[start..self.pos])
    }

    /// Take I048/120: a 1-octet primary subfield — CAL (2 octets) when bit 8 is
    /// set, then RDS (a `rep`-fold repetition of 6-octet groups) when bit 7 is set.
    fn take_compound_120(&mut self) -> Result<&'a [u8], Cat048DecodeError> {
        let start = self.pos;
        let primary = self.take(1)?[0];
        if primary & 0x80 != 0 {
            self.take(2)?; // CAL — Calculated Doppler Speed
        }
        if primary & 0x40 != 0 {
            let rep = self.take(1)?[0] as usize;
            self.take(rep * 6)?; // RDS — Raw Doppler Speed, 6 octets each
        }
        Ok(&self.bytes[start..self.pos])
    }

    /// Take an explicit-length item: a 1-octet indicator giving the item's total
    /// length (including the indicator).
    fn take_explicit(&mut self) -> Result<&'a [u8], Cat048DecodeError> {
        let start = self.pos;
        let total = self.take(1)?[0] as usize;
        if total == 0 {
            return Err(Cat048DecodeError::Truncated);
        }
        self.take(total - 1)?;
        Ok(&self.bytes[start..self.pos])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-built CAT048 block: one combined SSR+PSR report (SAC 0x19, SIC
    /// 0x02) at range 100 NM, azimuth 90°, ToD 12 s, squawk 1200, FL350, with a
    /// Mode-S address and the callsign "DLH123". Every field lands on a clean
    /// LSB count so the bytes are predictable. FRNs present: 1,2,3,4,5,6,8,9 →
    /// FSPEC [0xFD, 0xC0].
    fn reference_block() -> Vec<u8> {
        let record = [
            0xFD, 0xC0, // FSPEC: FRN 1-6, 8, 9
            0x19, 0x02, // I048/010 SAC/SIC
            0x00, 0x06, 0x00, // I048/140 ToD: 1536 ticks = 12 s
            0x60, // I048/020 TYP=011 (SSR+PSR), no FX
            0x64, 0x00, 0x40, 0x00, // I048/040 RHO=25600 (100 NM), THETA=16384 (90°)
            0x02, 0x80, // I048/070 Mode-3/A = 0o1200
            0x05, 0x78, // I048/090 FL = 1400 quarter-FL = FL350
            0x3C, 0x65, 0x89, // I048/220 ICAO 0x3C6589
            0x10, 0xC2, 0x31, 0xCB, 0x38, 0x20, // I048/240 "DLH123  "
        ];
        let mut block = vec![CATEGORY, 0x00, (3 + record.len()) as u8];
        block.extend_from_slice(&record);
        block
    }

    /// The reference block decodes to one report with every field recovered at
    /// its documented LSB. REQ: FR-NET-013, FR-IO-005
    #[test]
    fn reference_block_decodes_all_fields() {
        let reports = decode_target_reports(&reference_block()).expect("decodes");
        assert_eq!(reports.len(), 1);
        let r = &reports[0];
        assert_eq!((r.sac, r.sic), (0x19, 0x02));
        assert_eq!(r.time, Some(Timestamp(12.0)));
        assert_eq!(r.detection, Detection::SsrPsr);
        assert!(r.detection.has_primary() && r.detection.has_secondary());
        assert!(!r.detection.is_mode_s());
        let polar = r.position.expect("measured position present");
        assert!((polar.range - 100.0 * NM_TO_METRES).abs() < 1e-6, "100 NM");
        assert!((polar.azimuth_deg() - 90.0).abs() < 1e-6, "90 degrees");
        assert_eq!(r.mode_3a, Some(0o1200));
        assert_eq!(r.flight_level_ft, Some(35_000.0));
        assert_eq!(r.icao_address, Some(0x3C_6589));
        assert_eq!(r.callsign, Some(Callsign::new("DLH123")));
    }

    /// TYP decodes to the right detection class across the eight values, and the
    /// primary/secondary/Mode-S predicates follow. REQ: FR-NET-013
    #[test]
    fn target_report_descriptor_typ_maps_to_detection() {
        let cases = [
            (0b000, Detection::NoDetection, false, false, false),
            (0b001, Detection::Psr, true, false, false),
            (0b010, Detection::Ssr, false, true, false),
            (0b011, Detection::SsrPsr, true, true, false),
            (0b100, Detection::ModeSAllCall, false, true, true),
            (0b101, Detection::ModeSRollCall, false, true, true),
            (0b110, Detection::ModeSAllCallPsr, true, true, true),
            (0b111, Detection::ModeSRollCallPsr, true, true, true),
        ];
        for (typ, want, prim, sec, modes) in cases {
            let d = Detection::from_typ(typ);
            assert_eq!(d, want, "TYP {typ:03b}");
            assert_eq!(d.has_primary(), prim, "primary for {want:?}");
            assert_eq!(d.has_secondary(), sec, "secondary for {want:?}");
            assert_eq!(d.is_mode_s(), modes, "mode-s for {want:?}");
        }
    }

    /// I048/250 with BDS 4,0 and BDS 6,0 entries decodes and MERGES the EHS
    /// DAPs (FEP.2): selected altitude from 4,0; heading/IAS/Mach/vertical
    /// rate from 6,0; an interleaved unknown register contributes nothing.
    /// REQ: FR-TRK-040
    #[test]
    fn mode_s_mb_data_decodes_and_merges_ehs_daps() {
        let record = [
            0x81, 0x20, // FSPEC {1, 10}: I048/010 + I048/250
            0x19, 0x02, // I048/010
            0x03, // I048/250 REP=3
            // BDS 4,0: status=1, selected altitude 2188 × 16 ft = 35 008 ft.
            0xC4, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40,
            // BDS 1,0 (unknown to the DAP decoder): ignored.
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x10,
            // BDS 6,0: heading -90° (→270°), IAS 250 kt, Mach 0.78,
            // vertical rate -1024 ft/min.
            0xE0, 0x09, 0xF5, 0x30, 0xFF, 0x00, 0x00, 0x60,
        ];
        let mut block = vec![CATEGORY, 0x00, (3 + record.len()) as u8];
        block.extend_from_slice(&record);

        let reports = decode_target_reports(&block).expect("decodes");
        assert_eq!(reports.len(), 1);
        let daps = reports[0].daps;
        assert_eq!(daps.selected_altitude_ft, Some(35_008.0));
        assert_eq!(daps.magnetic_heading_deg, Some(270.0));
        assert_eq!(daps.ias_kt, Some(250.0));
        assert!((daps.mach.unwrap() - 0.78).abs() < 1e-9);
        assert_eq!(daps.barometric_vertical_rate_ft_min, Some(-1024.0));
        assert!(daps.roll_angle_deg.is_none(), "no BDS 5,0 in this report");
    }

    /// A negative (below-datum) flight level uses the 14-bit two's complement.
    /// REQ: FR-NET-013
    #[test]
    fn flight_level_decodes_negative_via_twos_complement() {
        // -40 quarter-FL = -1000 ft; 14-bit two's complement of -40 = 0x3FD8.
        assert_eq!(decode_flight_level(&[0x3F, 0xD8]), -1000.0);
        // The V/G status bits (top two) must be ignored.
        assert_eq!(decode_flight_level(&[0xC5, 0x78]), 35_000.0);
    }

    /// Unmodelled-but-standard items (here I048/161 track number, FRN 11, and
    /// I048/170 track status, FRN 14, an FX item) are skipped length-correctly,
    /// and the plot-relevant items around them still decode. REQ: FR-NET-013
    #[test]
    fn skips_unused_standard_items_length_correctly() {
        // FSPEC for {1, 4, 11, 14}: octet1 FRN1(0x80)+FRN4(0x10)+FX(0x01)=0x91;
        // octet2 FRN11(0x10)+FRN14(0x02), no FX = 0x12.
        let record = [
            0x91, 0x12, // FSPEC {1,4,11,14}
            0x19, 0x02, // I048/010
            0x64, 0x00, 0x40, 0x00, // I048/040
            0x0A, 0xBC, // I048/161 track number 0x0ABC (masked → 0x0ABC)
            0x81, 0x00, // I048/170 track status: octet with FX set, then a final octet
        ];
        let mut block = vec![CATEGORY, 0x00, (3 + record.len()) as u8];
        block.extend_from_slice(&record);

        let reports = decode_target_reports(&block).expect("decodes");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].track_number, Some(0x0ABC & TRACK_NUMBER_MASK));
        assert!(
            reports[0].position.is_some(),
            "I048/040 decoded past the skip"
        );
    }

    /// A wrong leading category octet is rejected, not mis-decoded.
    #[test]
    fn wrong_category_is_rejected() {
        let mut block = reference_block();
        block[0] = 62;
        assert_eq!(
            decode_target_reports(&block),
            Err(Cat048DecodeError::WrongCategory(62))
        );
    }

    /// A LEN field that disagrees with the datagram length is rejected.
    #[test]
    fn length_mismatch_is_rejected() {
        let mut block = reference_block();
        let actual = block.len();
        block[2] = block[2].wrapping_add(2); // claim two more bytes than present
        assert_eq!(
            decode_target_reports(&block),
            Err(Cat048DecodeError::LengthMismatch {
                declared: actual + 2,
                actual,
            })
        );
    }

    /// A present FRN with no known length cannot be skipped safely and is a hard
    /// error (no silent mis-parse). FRN 7 (I048/130) is known; we use a fake high
    /// FRN by setting a bit the decoder has no format for is impossible within the
    /// 28-FRN UAP, so instead assert the inverse: every standard FRN has a format.
    #[test]
    fn every_standard_frn_has_a_format() {
        for frn in 1..=28u8 {
            assert!(item_format(frn).is_some(), "FRN {frn} must have a format");
        }
        assert!(item_format(29).is_none(), "FRN 29 is outside the UAP");
    }

    /// Truncating the reference block at every length never panics and always
    /// yields a clean error (the "no panic on input" guarantee). REQ: FR-NET-013
    #[test]
    fn truncations_never_panic() {
        let full = reference_block();
        for n in 0..full.len() {
            // Each prefix is a deliberately malformed datagram. We only require
            // that decoding returns (Ok or Err) without panicking. To keep the
            // LEN check from trivially catching everything, also patch LEN to the
            // prefix length so the decoder walks into the (short) body.
            let mut prefix = full[..n].to_vec();
            let _ = decode_target_reports(&prefix);
            if prefix.len() >= 3 {
                let len = prefix.len() as u16;
                prefix[1..3].copy_from_slice(&len.to_be_bytes());
                let _ = decode_target_reports(&prefix);
            }
        }
    }

    /// Flipping any single byte of the reference block never panics — fuzz-style
    /// assurance for the untrusted input path. REQ: FR-NET-013
    #[test]
    fn single_byte_mutations_never_panic() {
        let base = reference_block();
        for i in 0..base.len() {
            for delta in 1u8..=255 {
                let mut m = base.clone();
                m[i] = m[i].wrapping_add(delta);
                // Keep LEN consistent so mutations exercise the body, not just
                // the length guard.
                let len = m.len() as u16;
                m[1..3].copy_from_slice(&len.to_be_bytes());
                let _ = decode_target_reports(&m);
            }
        }
    }

    /// The SPI ("ident") bit of I048/020 octet 1 is decoded; the reference
    /// block (0x60, no SPI) reports `false`, setting bit 3 (0x64) reports
    /// `true`. REQ: FR-TRK-036
    #[test]
    fn spi_bit_of_target_report_descriptor_is_decoded() {
        let block = reference_block();
        assert!(!decode_target_reports(&block).unwrap()[0].spi);

        let mut with_spi = block;
        with_spi[10] |= TRD_SPI; // I048/020 sits after CAT+LEN+FSPEC+010+140
        assert!(decode_target_reports(&with_spi).unwrap()[0].spi);
    }

    /// A hostile record whose FSPEC chains FX octets past the supported FRN
    /// space is rejected, not panicked on — frozen fuzzing find (QW.2): the
    /// unbounded FRN arithmetic used to overflow `u8`. REQ: NFR-SAFE-002
    #[test]
    fn overlong_fspec_chain_is_rejected_not_panicked() {
        let mut block = vec![CATEGORY, 0x00, 63];
        block.extend_from_slice(&[0xFF; 60]);
        assert_eq!(
            decode_target_reports(&block),
            Err(Cat048DecodeError::FspecTooLong)
        );
    }
}
