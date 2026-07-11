//! Mode S EHS **BDS register** decode (FEP.2) — the 56-bit Comm-B message
//! fields delivered in CAT048 I048/250.
//!
//! A Mode S Enhanced Surveillance radar interrogates specific transponder
//! registers ("Comm-B Data Selector", BDS) during roll-call and forwards
//! their raw 7-octet contents. This module turns the three EHS registers into
//! [`Daps`] fields, bit-exactly per **ICAO Doc 9871 / Annex 10**:
//!
//! - **BDS 4,0** — Selected vertical intention: the MCP/FCU **selected
//!   altitude** (what the crew dialled into the autopilot).
//! - **BDS 5,0** — Track and turn report: roll angle, true track angle,
//!   ground speed, true airspeed.
//! - **BDS 6,0** — Heading and speed report: magnetic heading, IAS, Mach,
//!   barometric altitude rate.
//!
//! **Status-bit discipline (the correctness point).** Every field in these
//! registers is prefixed by a 1-bit status flag: only fields the transponder
//! marked **valid** are decoded — a cleared status bit yields `None`, never a
//! zero. This is what keeps a partially-equipped or degraded avionics fit
//! from injecting fake zeros into the picture.
//!
//! Unknown BDS registers decode to an empty [`Daps`] (skipped — the CAT048
//! repetitive framing already delimits them). Pure bit arithmetic over a
//! fixed 7-byte array: no allocation, no panic on any input.
//!
//! REQ: FR-TRK-040

use firefly_core::Daps;

/// BDS register 4,0 — Selected vertical intention.
const BDS_4_0: u8 = 0x40;
/// BDS register 5,0 — Track and turn report.
const BDS_5_0: u8 = 0x50;
/// BDS register 6,0 — Heading and speed report.
const BDS_6_0: u8 = 0x60;

/// BDS 4,0 MCP/FCU selected altitude LSB: 16 ft.
const SELECTED_ALTITUDE_LSB_FT: f64 = 16.0;
/// BDS 5,0 roll angle LSB: 45/256 degrees.
const ROLL_LSB_DEG: f64 = 45.0 / 256.0;
/// BDS 5,0/6,0 angle LSB (true track, magnetic heading): 90/512 degrees.
const ANGLE_LSB_DEG: f64 = 90.0 / 512.0;
/// BDS 5,0 speed LSB (ground speed, TAS): 2 kt.
const SPEED_LSB_KT: f64 = 2.0;
/// BDS 6,0 IAS LSB: 1 kt.
const IAS_LSB_KT: f64 = 1.0;
/// BDS 6,0 Mach LSB: 2.048/512 = 0.004.
const MACH_LSB: f64 = 2.048 / 512.0;
/// BDS 6,0 barometric altitude rate LSB: 32 ft/min.
const VERTICAL_RATE_LSB_FT_MIN: f64 = 32.0;

/// Decode one I048/250 repetition — a 7-octet MB field tagged with its BDS
/// register number (`BDS1` high nibble, `BDS2` low nibble). Registers other
/// than 4,0 / 5,0 / 6,0 yield an empty [`Daps`].
pub(crate) fn decode_register(bds: u8, mb: &[u8; 7]) -> Daps {
    match bds {
        BDS_4_0 => decode_bds_4_0(mb),
        BDS_5_0 => decode_bds_5_0(mb),
        BDS_6_0 => decode_bds_6_0(mb),
        _ => Daps::default(),
    }
}

/// BDS 4,0 (ICAO Doc 9871 A.2.4): bit 1 status, bits 2–13 the MCP/FCU
/// selected altitude in 16-ft steps. (FMS selected altitude and barometric
/// pressure setting follow; not consumed today.)
fn decode_bds_4_0(mb: &[u8; 7]) -> Daps {
    Daps {
        selected_altitude_ft: guarded(mb, 1, || bits(mb, 2, 12) as f64 * SELECTED_ALTITUDE_LSB_FT),
        ..Daps::default()
    }
}

/// BDS 5,0 (A.2.5): roll angle (status 1, signed bits 2–11), true track
/// angle (status 12, signed bits 13–23), ground speed (status 24, bits
/// 25–34), track angle rate (status 35 — not consumed), true airspeed
/// (status 46, bits 47–56).
fn decode_bds_5_0(mb: &[u8; 7]) -> Daps {
    Daps {
        roll_angle_deg: guarded(mb, 1, || signed_bits(mb, 2, 10) as f64 * ROLL_LSB_DEG),
        true_track_deg: guarded(mb, 12, || {
            wrap_degrees(signed_bits(mb, 13, 11) as f64 * ANGLE_LSB_DEG)
        }),
        ground_speed_kt: guarded(mb, 24, || bits(mb, 25, 10) as f64 * SPEED_LSB_KT),
        true_airspeed_kt: guarded(mb, 46, || bits(mb, 47, 10) as f64 * SPEED_LSB_KT),
        ..Daps::default()
    }
}

/// BDS 6,0 (A.2.6): magnetic heading (status 1, signed bits 2–12), IAS
/// (status 13, bits 14–23), Mach (status 24, bits 25–34), barometric
/// altitude rate (status 35, signed bits 36–45), inertial vertical velocity
/// (status 46 — not consumed).
fn decode_bds_6_0(mb: &[u8; 7]) -> Daps {
    Daps {
        magnetic_heading_deg: guarded(mb, 1, || {
            wrap_degrees(signed_bits(mb, 2, 11) as f64 * ANGLE_LSB_DEG)
        }),
        ias_kt: guarded(mb, 13, || bits(mb, 14, 10) as f64 * IAS_LSB_KT),
        mach: guarded(mb, 24, || bits(mb, 25, 10) as f64 * MACH_LSB),
        barometric_vertical_rate_ft_min: guarded(mb, 35, || {
            signed_bits(mb, 36, 10) as f64 * VERTICAL_RATE_LSB_FT_MIN
        }),
        ..Daps::default()
    }
}

/// Evaluate `value` only when the status bit at `status_bit` is set — the
/// per-field validity discipline of the EHS registers.
fn guarded(mb: &[u8; 7], status_bit: usize, value: impl Fn() -> f64) -> Option<f64> {
    (bits(mb, status_bit, 1) == 1).then(value)
}

/// Extract `len` bits starting at 1-based bit position `start` (bit 1 = MSB
/// of the first octet — the ICAO register bit-numbering convention).
fn bits(mb: &[u8; 7], start: usize, len: usize) -> u32 {
    debug_assert!(start >= 1 && len >= 1 && start + len - 1 <= 56);
    let mut value = 0u32;
    for bit in start..start + len {
        let byte = (bit - 1) / 8;
        let shift = 7 - ((bit - 1) % 8);
        value = (value << 1) | ((mb[byte] >> shift) & 1) as u32;
    }
    value
}

/// Extract `len` bits as a two's-complement signed value (the first bit is
/// the sign — the ICAO encoding for signed register fields).
fn signed_bits(mb: &[u8; 7], start: usize, len: usize) -> i32 {
    let raw = bits(mb, start, len) as i32;
    if raw & (1 << (len - 1)) != 0 {
        raw - (1 << len)
    } else {
        raw
    }
}

/// Fold a signed angle (±180°) into [0, 360) for track/heading fields.
fn wrap_degrees(deg: f64) -> f64 {
    deg.rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a 7-byte MB field from 1-based (bit, len, value) writes.
    fn mb(fields: &[(usize, usize, u32)]) -> [u8; 7] {
        let mut out = [0u8; 7];
        for &(start, len, value) in fields {
            for i in 0..len {
                let bit = start + i;
                let byte = (bit - 1) / 8;
                let shift = 7 - ((bit - 1) % 8);
                let v = (value >> (len - 1 - i)) & 1;
                out[byte] |= (v as u8) << shift;
            }
        }
        out
    }

    /// BDS 4,0: selected altitude 35 008 ft = 2188 × 16 ft, status set —
    /// a hand-computed reference vector. REQ: FR-TRK-040
    #[test]
    fn bds_4_0_decodes_selected_altitude() {
        let field = mb(&[(1, 1, 1), (2, 12, 2188)]);
        let daps = decode_register(0x40, &field);
        assert_eq!(daps.selected_altitude_ft, Some(35_008.0));
        assert!(daps.magnetic_heading_deg.is_none());
    }

    /// A cleared status bit yields None even when the value bits are set —
    /// the status-bit discipline that keeps fake zeros (and stale garbage)
    /// out of the picture. REQ: FR-TRK-040
    #[test]
    fn cleared_status_bit_yields_none() {
        let field = mb(&[(2, 12, 2188)]); // value present, status 0
        assert_eq!(decode_register(0x40, &field).selected_altitude_ft, None);

        // BDS 6,0: heading valid, IAS status cleared despite value bits.
        let field = mb(&[(1, 1, 1), (2, 11, 512), (14, 10, 250)]);
        let daps = decode_register(0x60, &field);
        assert!(daps.magnetic_heading_deg.is_some());
        assert_eq!(daps.ias_kt, None, "status bit 13 is clear");
    }

    /// BDS 5,0: all four consumed fields decode with their LSBs; a negative
    /// roll (left bank) comes out signed via two's complement.
    /// REQ: FR-TRK-040
    #[test]
    fn bds_5_0_decodes_track_and_turn() {
        // roll = -20° → ticks = round(-20/0.17578) = -114 → two's complement
        // in 10 bits = 1024 - 114 = 910.
        let field = mb(&[
            (1, 1, 1),
            (2, 10, 910), // roll -114 ticks
            (12, 1, 1),
            (13, 11, 512), // true track 512 × 90/512 = 90°
            (24, 1, 1),
            (25, 10, 220), // ground speed 440 kt
            (46, 1, 1),
            (47, 10, 230), // TAS 460 kt
        ]);
        let daps = decode_register(0x50, &field);
        assert!((daps.roll_angle_deg.unwrap() - (-114.0 * 45.0 / 256.0)).abs() < 1e-9);
        assert!((daps.roll_angle_deg.unwrap() - (-20.0)).abs() < 0.1);
        assert_eq!(daps.true_track_deg, Some(90.0));
        assert_eq!(daps.ground_speed_kt, Some(440.0));
        assert_eq!(daps.true_airspeed_kt, Some(460.0));
    }

    /// BDS 6,0: heading/IAS/Mach/vertical rate decode with their LSBs; a
    /// westerly heading (negative in the signed encoding) wraps into
    /// [0, 360), and a descent has a negative rate. REQ: FR-TRK-040
    #[test]
    fn bds_6_0_decodes_heading_and_speed() {
        // heading -90° → ticks = -512 → two's complement in 11 bits = 1536.
        // vertical rate -1024 ft/min → -32 ticks → 10-bit two's compl. = 992.
        let field = mb(&[
            (1, 1, 1),
            (2, 11, 1536), // heading -90° → 270°
            (13, 1, 1),
            (14, 10, 250), // IAS 250 kt
            (24, 1, 1),
            (25, 10, 195), // Mach 0.78
            (35, 1, 1),
            (36, 10, 992), // -1024 ft/min
        ]);
        let daps = decode_register(0x60, &field);
        assert_eq!(daps.magnetic_heading_deg, Some(270.0));
        assert_eq!(daps.ias_kt, Some(250.0));
        assert!((daps.mach.unwrap() - 0.78).abs() < 1e-9);
        assert_eq!(daps.barometric_vertical_rate_ft_min, Some(-1024.0));
    }

    /// An unknown BDS register yields an empty Daps — skipped, not guessed.
    /// REQ: FR-TRK-040
    #[test]
    fn unknown_register_yields_empty_daps() {
        let field = mb(&[(1, 1, 1), (2, 12, 2188)]);
        assert!(decode_register(0x30, &field).is_empty());
        assert!(decode_register(0x00, &field).is_empty());
    }
}
