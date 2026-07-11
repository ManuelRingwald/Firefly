//! CAT021 **input** decode — ADS-B Target Reports from a ground station
//! (FEP.3).
//!
//! Firefly's ADS-B so far comes from internet REST services (OpenSky,
//! community aggregators): polled, seconds of latency, rate-limited, an
//! external dependency. The production-grade path is the own **ADS-B ground
//! station**, which emits **ASTERIX CAT021** over UDP — the same transport
//! class as the radar feed: local, sub-second, push instead of poll, and
//! carrying **quality indicators** (NACp) from which the measurement
//! uncertainty is derived honestly instead of assumed. This is how ARTAS
//! consumes its ADS-B.
//!
//! ## Edition
//!
//! The UAP follows **EUROCONTROL ASTERIX Part 12 Category 021 Edition 2.x**
//! (SUR.ET1.ST05.2000-STD-12-01). The legacy edition 0.26 has a *different*
//! UAP and is deliberately not supported — a station speaking 0.26 will fail
//! the decode loudly (unknown FRN layout) rather than being mis-parsed.
//!
//! ## Robustness (security-relevant input path)
//!
//! Like [`cat048`](crate::cat048)/[`cat034`](crate::cat034), this decoder
//! reads **untrusted network datagrams** (charter §8): bounds-checked
//! [`Cursor`], truncated/corrupt records yield a [`Cat021DecodeError`] (the
//! datagram is dropped, the listener continues), and a present-but-unknown
//! FRN is a hard error rather than a silent mis-parse. Covered by the
//! `cat021_decode` fuzz target.
//!
//! ## Scope
//!
//! The **plot-relevant** items are decoded (identity, time, WGS-84 position,
//! altitude, Mode 3/A, callsign, quality, report descriptor flags); every
//! other standard item is **skipped length-correctly** via a complete
//! per-FRN format model (fixed / extended / repetitive / compound /
//! explicit).
//!
//! REQ: FR-IO-010

use firefly_core::{Callsign, Timestamp};

use crate::fspec;

/// The ASTERIX category number for ADS-B target reports.
const CATEGORY: u8 = 21;

/// Time-of-day LSB of I021/071/073/077: 1/128 second since UTC midnight.
const TIME_LSB_SECONDS: f64 = 1.0 / 128.0;
/// I021/130 position LSB: 180/2²³ degrees (24-bit two's complement).
const POSITION_LSB_DEGREES: f64 = 180.0 / 8_388_608.0;
/// I021/131 high-resolution position LSB: 180/2³⁰ degrees (32-bit).
const POSITION_HIRES_LSB_DEGREES: f64 = 180.0 / 1_073_741_824.0;
/// I021/140 geometric height LSB: 6.25 ft.
const GEOMETRIC_HEIGHT_LSB_FT: f64 = 6.25;
/// I021/145 flight level LSB: 1/4 FL = 25 ft.
const FLIGHT_LEVEL_LSB_FT: f64 = 25.0;
/// I021/070 carries the Mode 3/A reply in its low 12 bits.
const MODE_3A_CODE_MASK: u16 = 0x0FFF;
/// FX bit (lowest bit of an octet): "another octet follows".
const FX: u8 = 0x01;
/// I021/040 first extension: GBS — ground bit set (surface report).
const TRD_GBS: u8 = 0x40;
/// I021/040 first extension: SIM — simulated target report.
const TRD_SIM: u8 = 0x20;
/// I021/040 first extension: TST — test target report.
const TRD_TST: u8 = 0x10;
/// I021/090 first extension: NACp lives in bits 5–2.
const QI_NACP_MASK: u8 = 0x3C;

/// The CAT021 edition 2.x UAP field reference numbers, per ASTERIX Part 12.
mod uap {
    pub const DATA_SOURCE_IDENTIFICATION: u8 = 1; // I021/010
    pub const TARGET_REPORT_DESCRIPTOR: u8 = 2; // I021/040
    pub const TRACK_NUMBER: u8 = 3; // I021/161
    pub const SERVICE_IDENTIFICATION: u8 = 4; // I021/015
    pub const TIME_OF_APPLICABILITY_POSITION: u8 = 5; // I021/071
    pub const POSITION_WGS84: u8 = 6; // I021/130
    pub const POSITION_WGS84_HIRES: u8 = 7; // I021/131
    pub const TIME_OF_APPLICABILITY_VELOCITY: u8 = 8; // I021/072
    pub const AIR_SPEED: u8 = 9; // I021/150
    pub const TRUE_AIR_SPEED: u8 = 10; // I021/151
    pub const TARGET_ADDRESS: u8 = 11; // I021/080
    pub const TIME_OF_RECEPTION_POSITION: u8 = 12; // I021/073
    pub const TIME_OF_RECEPTION_POSITION_HIGH: u8 = 13; // I021/074
    pub const TIME_OF_RECEPTION_VELOCITY: u8 = 14; // I021/075
    pub const TIME_OF_RECEPTION_VELOCITY_HIGH: u8 = 15; // I021/076
    pub const GEOMETRIC_HEIGHT: u8 = 16; // I021/140
    pub const QUALITY_INDICATORS: u8 = 17; // I021/090
    pub const MOPS_VERSION: u8 = 18; // I021/210
    pub const MODE_3A_CODE: u8 = 19; // I021/070
    pub const ROLL_ANGLE: u8 = 20; // I021/230
    pub const FLIGHT_LEVEL: u8 = 21; // I021/145
    pub const MAGNETIC_HEADING: u8 = 22; // I021/152
    pub const TARGET_STATUS: u8 = 23; // I021/200
    pub const BAROMETRIC_VERTICAL_RATE: u8 = 24; // I021/155
    pub const GEOMETRIC_VERTICAL_RATE: u8 = 25; // I021/157
    pub const AIRBORNE_GROUND_VECTOR: u8 = 26; // I021/160
    pub const TRACK_ANGLE_RATE: u8 = 27; // I021/165
    pub const TIME_OF_REPORT_TRANSMISSION: u8 = 28; // I021/077
    pub const TARGET_IDENTIFICATION: u8 = 29; // I021/170
    pub const EMITTER_CATEGORY: u8 = 30; // I021/020
    pub const MET_INFORMATION: u8 = 31; // I021/220
    pub const SELECTED_ALTITUDE: u8 = 32; // I021/146
    pub const FINAL_STATE_SELECTED_ALTITUDE: u8 = 33; // I021/148
    pub const TRAJECTORY_INTENT: u8 = 34; // I021/110
    pub const SERVICE_MANAGEMENT: u8 = 35; // I021/016
    pub const AIRCRAFT_OPERATIONAL_STATUS: u8 = 36; // I021/008
    pub const SURFACE_CAPABILITIES: u8 = 37; // I021/271
    pub const MESSAGE_AMPLITUDE: u8 = 38; // I021/132
    pub const MODE_S_MB_DATA: u8 = 39; // I021/250
    pub const ACAS_RA_REPORT: u8 = 40; // I021/260
    pub const RECEIVER_ID: u8 = 41; // I021/400
    pub const DATA_AGES: u8 = 42; // I021/295
    pub const RESERVED_EXPANSION: u8 = 48; // I021/RE
    pub const SPECIAL_PURPOSE: u8 = 49; // I021/SP
}

/// How a CAT021 data item is laid out on the wire.
enum ItemFormat {
    /// A fixed number of octets.
    Fixed(usize),
    /// An FX-chained sequence of octets.
    Extended,
    /// A 1-octet repetition factor, then `rep × n` octets.
    Repetitive(usize),
    /// I021/220 Met Information: 1-octet primary subfield (no FX chain in
    /// ed 2.x), subfields WS(2) WD(2) TMP(2) TRB(1) per set bit 8..5.
    CompoundMet,
    /// I021/110 Trajectory Intent: 1-octet primary subfield — TIS (bit 8, an
    /// FX-chained octet) and TID (bit 7, a 15-octet repetitive).
    CompoundTrajectory,
    /// I021/295 Data Ages: FX-chained primary subfield chain; every defined
    /// age subfield is exactly 1 octet.
    CompoundAges,
    /// A 1-octet length indicator giving the item's total length (SP/RE).
    Explicit,
}

/// The wire layout of each CAT021 UAP item; `None` for spare FRNs (43–47) and
/// anything outside the UAP — a present unknown FRN cannot be skipped safely.
fn item_format(frn: u8) -> Option<ItemFormat> {
    use uap::*;
    Some(match frn {
        DATA_SOURCE_IDENTIFICATION => ItemFormat::Fixed(2),
        TARGET_REPORT_DESCRIPTOR => ItemFormat::Extended,
        TRACK_NUMBER => ItemFormat::Fixed(2),
        SERVICE_IDENTIFICATION => ItemFormat::Fixed(1),
        TIME_OF_APPLICABILITY_POSITION => ItemFormat::Fixed(3),
        POSITION_WGS84 => ItemFormat::Fixed(6),
        POSITION_WGS84_HIRES => ItemFormat::Fixed(8),
        TIME_OF_APPLICABILITY_VELOCITY => ItemFormat::Fixed(3),
        AIR_SPEED => ItemFormat::Fixed(2),
        TRUE_AIR_SPEED => ItemFormat::Fixed(2),
        TARGET_ADDRESS => ItemFormat::Fixed(3),
        TIME_OF_RECEPTION_POSITION => ItemFormat::Fixed(3),
        TIME_OF_RECEPTION_POSITION_HIGH => ItemFormat::Fixed(4),
        TIME_OF_RECEPTION_VELOCITY => ItemFormat::Fixed(3),
        TIME_OF_RECEPTION_VELOCITY_HIGH => ItemFormat::Fixed(4),
        GEOMETRIC_HEIGHT => ItemFormat::Fixed(2),
        QUALITY_INDICATORS => ItemFormat::Extended,
        MOPS_VERSION => ItemFormat::Fixed(1),
        MODE_3A_CODE => ItemFormat::Fixed(2),
        ROLL_ANGLE => ItemFormat::Fixed(2),
        FLIGHT_LEVEL => ItemFormat::Fixed(2),
        MAGNETIC_HEADING => ItemFormat::Fixed(2),
        TARGET_STATUS => ItemFormat::Fixed(1),
        BAROMETRIC_VERTICAL_RATE => ItemFormat::Fixed(2),
        GEOMETRIC_VERTICAL_RATE => ItemFormat::Fixed(2),
        AIRBORNE_GROUND_VECTOR => ItemFormat::Fixed(4),
        TRACK_ANGLE_RATE => ItemFormat::Fixed(2),
        TIME_OF_REPORT_TRANSMISSION => ItemFormat::Fixed(3),
        TARGET_IDENTIFICATION => ItemFormat::Fixed(6),
        EMITTER_CATEGORY => ItemFormat::Fixed(1),
        MET_INFORMATION => ItemFormat::CompoundMet,
        SELECTED_ALTITUDE => ItemFormat::Fixed(2),
        FINAL_STATE_SELECTED_ALTITUDE => ItemFormat::Fixed(2),
        TRAJECTORY_INTENT => ItemFormat::CompoundTrajectory,
        SERVICE_MANAGEMENT => ItemFormat::Fixed(1),
        AIRCRAFT_OPERATIONAL_STATUS => ItemFormat::Fixed(1),
        SURFACE_CAPABILITIES => ItemFormat::Extended,
        MESSAGE_AMPLITUDE => ItemFormat::Fixed(1),
        MODE_S_MB_DATA => ItemFormat::Repetitive(8),
        ACAS_RA_REPORT => ItemFormat::Fixed(7),
        RECEIVER_ID => ItemFormat::Fixed(1),
        DATA_AGES => ItemFormat::CompoundAges,
        RESERVED_EXPANSION => ItemFormat::Explicit,
        SPECIAL_PURPOSE => ItemFormat::Explicit,
        _ => return None,
    })
}

/// One decoded CAT021 ADS-B target report — the neutral product of
/// [`decode_adsb_reports`]. The adapter (`firefly-adsb021`) turns it into a
/// geodetic [`Plot`](firefly_core::Plot).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodedAdsbReport {
    /// I021/010 — System Area Code of the reporting ground station.
    pub sac: u8,
    /// I021/010 — System Identification Code of the reporting ground station.
    pub sic: u8,
    /// The report's data time: I021/073 (time of message reception for
    /// position) preferred, falling back to I021/071 (time of applicability)
    /// and I021/077 (time of report transmission).
    pub time: Option<Timestamp>,
    /// WGS-84 position `(lat, lon)` in degrees: I021/131 (high resolution)
    /// preferred over I021/130.
    pub position_deg: Option<(f64, f64)>,
    /// I021/140 — geometric (WGS-84) height, **feet**, if present.
    pub geometric_height_ft: Option<f64>,
    /// I021/145 — barometric flight level, **feet**, if present.
    pub flight_level_ft: Option<f64>,
    /// I021/080 — 24-bit ICAO aircraft address.
    pub icao_address: Option<u32>,
    /// I021/170 — target identification (callsign), if present.
    pub callsign: Option<Callsign>,
    /// I021/070 — Mode 3/A code (low 12 bits), if present.
    pub mode_3a: Option<u16>,
    /// I021/090 — NACp (Navigation Accuracy Category — Position) from the
    /// first extension octet, if the item extends that far. Drives the
    /// adapter's measurement-uncertainty derivation.
    pub nacp: Option<u8>,
    /// I021/040 first extension GBS — the target reports itself **on the
    /// ground** (surface report; the adapter drops it from the air picture).
    pub ground_bit: bool,
    /// I021/040 first extension SIM/TST — simulated or test target (the
    /// adapter drops it; Firefly carries no simulated traffic, FR-TRK-036).
    pub simulated_or_test: bool,
}

/// Errors that can occur while decoding a CAT021 data block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cat021DecodeError {
    /// The input ended before a complete block/record/item could be read.
    Truncated,
    /// The first octet was not [`CATEGORY`] (21).
    WrongCategory(u8),
    /// The `LEN` field did not match the actual input length.
    LengthMismatch {
        /// Length declared by the block's LEN field.
        declared: usize,
        /// Actual datagram length.
        actual: usize,
    },
    /// The FSPEC marked an FRN present that this decoder cannot length —
    /// a spare FRN, or (most likely) a station speaking the legacy 0.26 UAP.
    UnknownItem(u8),
    /// A record was missing the mandatory I021/010.
    MissingItem(u8),
    /// The FSPEC's FX chain ran past [`fspec::MAX_FSPEC_OCTETS`] — malformed.
    FspecTooLong,
}

impl std::fmt::Display for Cat021DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cat021DecodeError::Truncated => write!(f, "input ended before a complete item"),
            Cat021DecodeError::WrongCategory(cat) => write!(f, "expected CAT 21, got CAT {cat}"),
            Cat021DecodeError::LengthMismatch { declared, actual } => write!(
                f,
                "LEN field says {declared} bytes, but input is {actual} bytes"
            ),
            Cat021DecodeError::UnknownItem(frn) => {
                write!(
                    f,
                    "FSPEC marks unknown FRN {frn} present (edition mismatch?)"
                )
            }
            Cat021DecodeError::MissingItem(frn) => {
                write!(f, "record is missing required FRN {frn}")
            }
            Cat021DecodeError::FspecTooLong => {
                write!(f, "FSPEC FX chain exceeds the supported FRN space")
            }
        }
    }
}

impl std::error::Error for Cat021DecodeError {}

/// Decode a CAT021 data block (`[CAT=21][LEN][record…]`) into one
/// [`DecodedAdsbReport`] per record. Never panics on input.
pub fn decode_adsb_reports(bytes: &[u8]) -> Result<Vec<DecodedAdsbReport>, Cat021DecodeError> {
    if bytes.len() < 3 {
        return Err(Cat021DecodeError::Truncated);
    }
    if bytes[0] != CATEGORY {
        return Err(Cat021DecodeError::WrongCategory(bytes[0]));
    }
    let declared = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    if declared != bytes.len() {
        return Err(Cat021DecodeError::LengthMismatch {
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
fn decode_record(cursor: &mut Cursor) -> Result<DecodedAdsbReport, Cat021DecodeError> {
    let frns = cursor.take_fspec()?;

    let mut sac_sic = None;
    let mut time_reception = None;
    let mut time_applicability = None;
    let mut time_transmission = None;
    let mut position_deg = None;
    let mut position_hires_deg = None;
    let mut geometric_height_ft = None;
    let mut flight_level_ft = None;
    let mut icao_address = None;
    let mut callsign = None;
    let mut mode_3a = None;
    let mut nacp = None;
    let mut ground_bit = false;
    let mut simulated_or_test = false;

    for frn in frns {
        let format = item_format(frn).ok_or(Cat021DecodeError::UnknownItem(frn))?;
        let bytes = match format {
            ItemFormat::Fixed(n) => cursor.take(n)?,
            ItemFormat::Extended => cursor.take_extended()?,
            ItemFormat::Repetitive(per) => cursor.take_repetitive(per)?,
            ItemFormat::CompoundMet => cursor.take_compound_met()?,
            ItemFormat::CompoundTrajectory => cursor.take_compound_trajectory()?,
            ItemFormat::CompoundAges => cursor.take_compound_ages()?,
            ItemFormat::Explicit => cursor.take_explicit()?,
        };
        match frn {
            uap::DATA_SOURCE_IDENTIFICATION => sac_sic = Some((bytes[0], bytes[1])),
            uap::TARGET_REPORT_DESCRIPTOR => {
                if let Some(&ext1) = bytes.get(1) {
                    ground_bit = ext1 & TRD_GBS != 0;
                    simulated_or_test = ext1 & (TRD_SIM | TRD_TST) != 0;
                }
            }
            uap::TIME_OF_APPLICABILITY_POSITION => {
                time_applicability = Some(decode_time_of_day(bytes))
            }
            uap::POSITION_WGS84 => {
                let lat = i24_to_i32(&bytes[0..3]) as f64 * POSITION_LSB_DEGREES;
                let lon = i24_to_i32(&bytes[3..6]) as f64 * POSITION_LSB_DEGREES;
                position_deg = Some((lat, lon));
            }
            uap::POSITION_WGS84_HIRES => {
                let lat = i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as f64
                    * POSITION_HIRES_LSB_DEGREES;
                let lon = i32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as f64
                    * POSITION_HIRES_LSB_DEGREES;
                position_hires_deg = Some((lat, lon));
            }
            uap::TARGET_ADDRESS => {
                icao_address = Some(u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]))
            }
            uap::TIME_OF_RECEPTION_POSITION => time_reception = Some(decode_time_of_day(bytes)),
            uap::GEOMETRIC_HEIGHT => {
                let ticks = i16::from_be_bytes([bytes[0], bytes[1]]);
                geometric_height_ft = Some(ticks as f64 * GEOMETRIC_HEIGHT_LSB_FT);
            }
            uap::QUALITY_INDICATORS => {
                // NACp sits in the FIRST extension octet (bits 5–2); the item
                // may legally end after octet 1, then no NACp is reported.
                if let Some(&ext1) = bytes.get(1) {
                    nacp = Some((ext1 & QI_NACP_MASK) >> 2);
                }
            }
            uap::MODE_3A_CODE => {
                mode_3a = Some(u16::from_be_bytes([bytes[0], bytes[1]]) & MODE_3A_CODE_MASK)
            }
            uap::FLIGHT_LEVEL => {
                let ticks = i16::from_be_bytes([bytes[0], bytes[1]]);
                flight_level_ft = Some(ticks as f64 * FLIGHT_LEVEL_LSB_FT);
            }
            uap::TIME_OF_REPORT_TRANSMISSION => time_transmission = Some(decode_time_of_day(bytes)),
            uap::TARGET_IDENTIFICATION => callsign = Some(decode_target_identification(bytes)),
            _ => {} // present but not plot-relevant — length already consumed
        }
    }

    let (sac, sic) = sac_sic.ok_or(Cat021DecodeError::MissingItem(
        uap::DATA_SOURCE_IDENTIFICATION,
    ))?;

    Ok(DecodedAdsbReport {
        sac,
        sic,
        // Reception time is the station's honest stamp; applicability and
        // transmission time are the standard fallbacks.
        time: time_reception.or(time_applicability).or(time_transmission),
        // High-resolution position wins when both are present.
        position_deg: position_hires_deg.or(position_deg),
        geometric_height_ft,
        flight_level_ft,
        icao_address,
        callsign,
        mode_3a,
        nacp,
        ground_bit,
        simulated_or_test,
    })
}

/// A 24-bit time-of-day count of 1/128-second ticks since UTC midnight.
fn decode_time_of_day(bytes: &[u8]) -> Timestamp {
    let ticks = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
    Timestamp(ticks as f64 * TIME_LSB_SECONDS)
}

/// Sign-extend a 24-bit two's-complement big-endian value.
fn i24_to_i32(bytes: &[u8]) -> i32 {
    let raw = i32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
    if raw & 0x0080_0000 != 0 {
        raw - 0x0100_0000
    } else {
        raw
    }
}

/// I021/170 — six octets packing eight 6-bit IA-5 codes (identical layout to
/// CAT048 I048/240; kept local so the two decoders stay independent).
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

/// A bounds-checked read cursor; every `take*` returns
/// [`Cat021DecodeError::Truncated`] rather than panicking.
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

    fn take(&mut self, n: usize) -> Result<&'a [u8], Cat021DecodeError> {
        if self.remaining() < n {
            return Err(Cat021DecodeError::Truncated);
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn take_fspec(&mut self) -> Result<std::collections::BTreeSet<u8>, Cat021DecodeError> {
        let slice = &self.bytes[self.pos..];
        let (frns, consumed) = fspec::parse(slice).map_err(|_| Cat021DecodeError::FspecTooLong)?;
        if consumed == 0 || slice[consumed - 1] & FX != 0 {
            return Err(Cat021DecodeError::Truncated);
        }
        self.pos += consumed;
        Ok(frns)
    }

    fn take_extended(&mut self) -> Result<&'a [u8], Cat021DecodeError> {
        let start = self.pos;
        loop {
            let octet = self.take(1)?[0];
            if octet & FX == 0 {
                break;
            }
        }
        Ok(&self.bytes[start..self.pos])
    }

    fn take_repetitive(&mut self, per: usize) -> Result<&'a [u8], Cat021DecodeError> {
        let start = self.pos;
        let rep = self.take(1)?[0] as usize;
        self.take(rep * per)?;
        Ok(&self.bytes[start..self.pos])
    }

    /// I021/220 Met Information: a single primary octet whose bits 8..5 mark
    /// the subfields WS(2), WD(2), TMP(2), TRB(1).
    fn take_compound_met(&mut self) -> Result<&'a [u8], Cat021DecodeError> {
        let start = self.pos;
        let primary = self.take(1)?[0];
        let mut len = 0usize;
        for (bit, sublen) in [(0x80, 2), (0x40, 2), (0x20, 2), (0x10, 1)] {
            if primary & bit != 0 {
                len += sublen;
            }
        }
        self.take(len)?;
        Ok(&self.bytes[start..self.pos])
    }

    /// I021/110 Trajectory Intent: primary octet — TIS (bit 8, an FX-chained
    /// status octet) and TID (bit 7, a 15-octet repetitive).
    fn take_compound_trajectory(&mut self) -> Result<&'a [u8], Cat021DecodeError> {
        let start = self.pos;
        let primary = self.take(1)?[0];
        if primary & 0x80 != 0 {
            self.take_extended()?; // TIS
        }
        if primary & 0x40 != 0 {
            self.take_repetitive(15)?; // TID
        }
        Ok(&self.bytes[start..self.pos])
    }

    /// I021/295 Data Ages: an FX-chained primary subfield chain; every
    /// defined age subfield is exactly 1 octet.
    fn take_compound_ages(&mut self) -> Result<&'a [u8], Cat021DecodeError> {
        let start = self.pos;
        let mut subfields = 0usize;
        loop {
            let octet = self.take(1)?[0];
            subfields += (octet & 0xFE).count_ones() as usize;
            if octet & FX == 0 {
                break;
            }
            // The spec defines 4 spec octets of ages; a longer chain is hostile.
            if self.pos - start >= 4 {
                return Err(Cat021DecodeError::Truncated);
            }
        }
        self.take(subfields)?;
        Ok(&self.bytes[start..self.pos])
    }

    fn take_explicit(&mut self) -> Result<&'a [u8], Cat021DecodeError> {
        let start = self.pos;
        let total = self.take(1)?[0] as usize;
        if total == 0 {
            return Err(Cat021DecodeError::Truncated);
        }
        self.take(total - 1)?;
        Ok(&self.bytes[start..self.pos])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrap record bytes in a CAT021 block envelope.
    fn block(records: &[u8]) -> Vec<u8> {
        let mut out = vec![CATEGORY, 0x00, (3 + records.len()) as u8];
        out.extend_from_slice(records);
        out
    }

    /// A hand-built airborne report decodes field-exactly — the CAT021
    /// reference vector. FSPEC {1, 7, 11, 12} = ADS-B identity + hi-res
    /// position + address + reception time: octet1 FRN1(0x80)+FRN7(0x02)
    /// +FX(0x01)=0x83; octet2 FRN11(0x10)+FRN12(0x08)=0x18.
    /// Position 45°/11.25° in hi-res LSB: 45/(180/2³⁰)=268 435 456 =
    /// 0x1000_0000; 11.25° = 0x0400_0000. Time 3600 s = 0x070800.
    /// REQ: FR-IO-010
    #[test]
    fn airborne_report_matches_reference_vector() {
        let bytes = block(&[
            0x83, 0x18, // FSPEC {1, 7, 11, 12}
            0x19, 0x0A, // I021/010 SAC=25 SIC=10
            0x10, 0x00, 0x00, 0x00, // I021/131 lat 45°
            0x04, 0x00, 0x00, 0x00, // I021/131 lon 11.25°
            0x3C, 0x65, 0x89, // I021/080 ICAO
            0x07, 0x08, 0x00, // I021/073 time 3600 s
        ]);
        let reports = decode_adsb_reports(&bytes).expect("decodes");
        assert_eq!(reports.len(), 1);
        let r = reports[0];
        assert_eq!((r.sac, r.sic), (25, 10));
        let (lat, lon) = r.position_deg.expect("position present");
        assert!((lat - 45.0).abs() < 1e-9 && (lon - 11.25).abs() < 1e-9);
        assert_eq!(r.icao_address, Some(0x3C_6589));
        assert_eq!(r.time, Some(Timestamp(3600.0)));
        assert!(!r.ground_bit && !r.simulated_or_test);
        assert_eq!(r.nacp, None, "no quality item in this record");
    }

    /// The low-resolution position (I021/130, LSB 180/2²³) decodes including
    /// negative (south/west) coordinates, and the high-resolution item wins
    /// when both are present. REQ: FR-IO-010
    #[test]
    fn position_prefers_high_resolution_and_signs_via_twos_complement() {
        // I021/130 only: lat -45° → -2 097 152 ticks = 0xE00000; lon -11.25°
        // → -524 288 = 0xF80000.
        let bytes = block(&[
            0x84, // FSPEC {1, 6}
            0x19, 0x0A, // I021/010
            0xE0, 0x00, 0x00, 0xF8, 0x00, 0x00, // I021/130
        ]);
        let r = decode_adsb_reports(&bytes).expect("decodes")[0];
        let (lat, lon) = r.position_deg.unwrap();
        assert!((lat - (-45.0)).abs() < 1e-9 && (lon - (-11.25)).abs() < 1e-9);

        // Both present: 131 (45°) beats 130 (-45°).
        let bytes = block(&[
            0x86, // FSPEC {1, 6, 7}
            0x19, 0x0A, // I021/010
            0xE0, 0x00, 0x00, 0xF8, 0x00, 0x00, // I021/130 (-45, -11.25)
            0x10, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, // I021/131 (45, 11.25)
        ]);
        let r = decode_adsb_reports(&bytes).expect("decodes")[0];
        let (lat, _) = r.position_deg.unwrap();
        assert!((lat - 45.0).abs() < 1e-9, "hi-res wins");
    }

    /// Identity, quality and altitude items decode at their documented LSBs:
    /// callsign, Mode 3/A, NACp from the quality extension, flight level and
    /// geometric height. REQ: FR-IO-010
    #[test]
    fn identity_quality_and_altitude_decode() {
        let bytes = block(&[
            // FSPEC: oct1 FRN1+FX = 0x81; oct2 (FRN 8–14) empty +FX = 0x01;
            // oct3 (FRN 15–21): FRN16=0x40, FRN17=0x20, FRN19=0x08,
            // FRN21=0x02 → 0x6A.
            0x81, 0x01, 0x6A, // FSPEC {1, 16, 17, 19, 21}
            0x19, 0x0A, // I021/010
            0x06, 0x40, // I021/140 geometric height 1600 ticks × 6.25 = 10 000 ft
            0x01, 0x2C, // I021/090: oct1 FX set; ext1 NACp=(0x2C&0x3C)>>2 = 11
            0x02, 0x8B, // I021/070 Mode 3/A 0o1213
            0x05, 0x78, // I021/145 FL: 1400 × 25 ft = 35 000 ft
        ]);
        let r = decode_adsb_reports(&bytes).expect("decodes")[0];
        assert_eq!(r.geometric_height_ft, Some(10_000.0));
        assert_eq!(r.nacp, Some(11));
        assert_eq!(r.mode_3a, Some(0o1213));
        assert_eq!(r.flight_level_ft, Some(35_000.0));
    }

    /// The callsign packs eight 6-bit IA-5 codes into six octets (same layout
    /// as CAT048 I048/240). REQ: FR-IO-010
    #[test]
    fn target_identification_decodes_callsign() {
        // FSPEC {1, 29}: oct1 FRN1+FX; octs 2–4 empty +FX; oct5 (FRN 29–35)
        // FRN29 = 0x80.
        let bytes = block(&[
            0x81, 0x01, 0x01, 0x01, 0x80, // FSPEC {1, 29}
            0x19, 0x0A, // I021/010
            0x10, 0xC2, 0x31, 0xCB, 0x38, 0x20, // "DLH123  "
        ]);
        let r = decode_adsb_reports(&bytes).expect("decodes")[0];
        assert_eq!(r.callsign, Some(Callsign::new("DLH123")));
    }

    /// The target report descriptor's first extension carries GBS (surface)
    /// and SIM/TST (simulated/test) — the flags the adapter drops on.
    /// REQ: FR-IO-010
    #[test]
    fn report_descriptor_flags_ground_and_test_targets() {
        // FSPEC {1, 2}: 0xC0.
        let ground = block(&[
            0xC0, 0x19, 0x0A, // FSPEC + I021/010
            0x01, 0x40, // I021/040: oct1 FX set; ext1 GBS
        ]);
        let r = decode_adsb_reports(&ground).expect("decodes")[0];
        assert!(r.ground_bit && !r.simulated_or_test);

        let test_target = block(&[
            0xC0, 0x19, 0x0A, //
            0x01, 0x10, // ext1 TST
        ]);
        let r = decode_adsb_reports(&test_target).expect("decodes")[0];
        assert!(!r.ground_bit && r.simulated_or_test);

        // Single-octet descriptor (no extension): both flags stay clear.
        let plain = block(&[0xC0, 0x19, 0x0A, 0x00]);
        let r = decode_adsb_reports(&plain).expect("decodes")[0];
        assert!(!r.ground_bit && !r.simulated_or_test);
    }

    /// Unconsumed standard items — including the three compound layouts and
    /// an explicit RE field — are skipped length-correctly, and a second
    /// record still decodes. REQ: FR-IO-010
    #[test]
    fn skips_unused_standard_items_length_correctly() {
        // Record 1 FSPEC {1, 31, 34, 42, 48}: oct1 FRN1+FX = 0x81; octs 2–4
        // empty +FX; oct5 (29–35): FRN31=0x20, FRN34=0x04, +FX = 0x25;
        // oct6 (36–42): FRN42=0x02, +FX = 0x03; oct7 (43–49): FRN48=0x04.
        let mut rec = vec![
            0x81, 0x01, 0x01, 0x01, 0x25, 0x03, 0x04, // FSPEC {1,31,34,42,48}
            0x19, 0x0A, // I021/010
        ];
        // I021/220 Met: primary 0xA0 = WS(2) + TMP(2) → 4 data octets.
        rec.extend_from_slice(&[0xA0, 0x01, 0x02, 0x03, 0x04]);
        // I021/110 Trajectory: primary 0xC0 = TIS + TID; TIS extended
        // (0x01 FX-chained → final 0x00), TID repetitive REP=1 × 15 octets.
        rec.extend_from_slice(&[0xC0, 0x01, 0x00, 0x01]);
        rec.extend_from_slice(&[0u8; 15]);
        // I021/295 Ages: two spec octets (FX), 3 set age bits → 3 octets.
        rec.extend_from_slice(&[0x81, 0x60, 0x11, 0x22, 0x33]);
        // I021/RE: LEN=3 (incl. itself) + 2 payload octets.
        rec.extend_from_slice(&[0x03, 0xEE, 0xFF]);
        // Record 2: a plain identity-only record.
        rec.extend_from_slice(&[0x80, 0x19, 0x0B]);
        let reports = decode_adsb_reports(&block(&rec)).expect("decodes");
        assert_eq!(reports.len(), 2, "compound skips kept the stream in sync");
        assert_eq!(reports[1].sic, 0x0B);
    }

    /// Missing I021/010 is a hard error; wrong category and length lies are
    /// rejected; every truncation errors instead of panicking.
    /// REQ: FR-IO-010, NFR-SAFE-002
    #[test]
    fn malformed_input_is_rejected_not_panicked() {
        let no_identity = block(&[0x40, 0x01, 0x00]); // FRN2 only
        assert_eq!(
            decode_adsb_reports(&no_identity),
            Err(Cat021DecodeError::MissingItem(1))
        );
        assert_eq!(
            decode_adsb_reports(&[48, 0x00, 0x03]),
            Err(Cat021DecodeError::WrongCategory(48))
        );
        assert_eq!(
            decode_adsb_reports(&[CATEGORY, 0xFF, 0xFF, 0x00]),
            Err(Cat021DecodeError::LengthMismatch {
                declared: 0xFFFF,
                actual: 4
            })
        );

        let valid = block(&[
            0x83, 0x18, 0x19, 0x0A, 0x10, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x3C, 0x65,
            0x89, 0x07, 0x08, 0x00,
        ]);
        for cut in 0..valid.len() {
            let mut shortened = valid[..cut].to_vec();
            if shortened.len() >= 3 {
                shortened[1] = 0;
                shortened[2] = shortened.len() as u8;
            }
            let _ = decode_adsb_reports(&shortened);
        }
    }

    /// A spare FRN (43–47) marked present cannot be lengthed and is rejected
    /// — the loud failure mode for a legacy-edition (0.26) station.
    /// REQ: FR-IO-010
    #[test]
    fn spare_frn_is_rejected() {
        // FRN 43 sits in oct7 (43..49): 1<<(7-((43-1)%7)) = 1<<7 = 0x80.
        let bytes = block(&[
            0x81, 0x01, 0x01, 0x01, 0x01, 0x01, 0x80, // FSPEC {1, 43}
            0x19, 0x0A, 0x00,
        ]);
        assert_eq!(
            decode_adsb_reports(&bytes),
            Err(Cat021DecodeError::UnknownItem(43))
        );
    }

    /// A hostile record whose FSPEC chains FX octets past the supported FRN
    /// space is rejected, not panicked on (QW.2 parity). REQ: NFR-SAFE-002
    #[test]
    fn overlong_fspec_chain_is_rejected_not_panicked() {
        let mut bytes = vec![CATEGORY, 0x00, 63];
        bytes.extend_from_slice(&[0xFF; 60]);
        assert_eq!(
            decode_adsb_reports(&bytes),
            Err(Cat021DecodeError::FspecTooLong)
        );
    }
}
