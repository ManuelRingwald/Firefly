//! Barometric altitude conversion (VERT.1).
//!
//! A Mode-C reply / flight level is a **pressure altitude**: the altitude in
//! the ICAO standard atmosphere at which the measured static pressure
//! occurs, referenced to 1013.25 hPa. Below the transition altitude the
//! operationally relevant quantity is the **QNH altitude** — the same
//! pressure read against the local sea-level pressure. The conversion goes
//! through the pressure itself (exact ICAO troposphere formula), not the
//! linear ~27 ft/hPa rule of thumb, so it stays correct at altitude and for
//! large pressure deviations.

/// ICAO standard atmosphere sea-level pressure, hectopascal.
pub const ISA_STANDARD_PRESSURE_HPA: f64 = 1013.25;
/// ISA sea-level temperature, kelvin.
const ISA_T0_K: f64 = 288.15;
/// ISA tropospheric lapse rate, kelvin per metre.
const ISA_LAPSE_K_PER_M: f64 = 0.0065;
/// The ISA exponent R·L/g₀ for the troposphere (dimensionless).
const ISA_EXPONENT: f64 = 0.190_263_2;
/// Feet per metre.
const FT_PER_M: f64 = 1.0 / 0.3048;

/// Convert a 1013.25-referenced **pressure altitude** (ft — a Mode-C reply /
/// flight level × 100) into the **QNH-referenced altitude** (ft) for the
/// given QNH (hPa), via the exact ICAO troposphere relation:
///
/// 1. pressure altitude → static pressure:
///    `P = 1013.25 · (1 − L·Hp/T0)^(1/κ)`
/// 2. static pressure → altitude in a QNH-based atmosphere:
///    `H = (T0/L) · (1 − (P/QNH)^κ)`
///
/// With `QNH == 1013.25` the result is the input (bit-exact up to floating
/// point). The linear approximation (~27.3 ft/hPa near sea level) emerges
/// naturally for small deviations; this exact form keeps large deviations
/// and higher altitudes honest.
pub fn pressure_altitude_to_qnh_altitude(pressure_altitude_ft: f64, qnh_hpa: f64) -> f64 {
    let hp_m = pressure_altitude_ft / FT_PER_M;
    let pressure_hpa = ISA_STANDARD_PRESSURE_HPA
        * (1.0 - ISA_LAPSE_K_PER_M * hp_m / ISA_T0_K).powf(1.0 / ISA_EXPONENT);
    let h_m = (ISA_T0_K / ISA_LAPSE_K_PER_M) * (1.0 - (pressure_hpa / qnh_hpa).powf(ISA_EXPONENT));
    h_m * FT_PER_M
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With the standard QNH the conversion is the identity — a flight level
    /// above the transition altitude stays what it is. REQ: FR-TRK-041
    #[test]
    fn standard_qnh_is_the_identity() {
        for alt in [0.0, 3_000.0, 10_000.0, 35_000.0] {
            let converted = pressure_altitude_to_qnh_altitude(alt, ISA_STANDARD_PRESSURE_HPA);
            assert!((converted - alt).abs() < 1e-6, "{alt} ft drifted");
        }
    }

    /// The correction direction and magnitude match the ATC rule of thumb
    /// (~27–28 ft per hPa near sea level): low QNH → true altitude BELOW the
    /// pressure altitude, high QNH → above. REQ: FR-TRK-041
    #[test]
    fn correction_matches_the_rule_of_thumb_near_sea_level() {
        // 30.25 hPa low (983.0): expect ≈ −30.25 × ~27.5 ft ≈ −830 ft.
        let low = pressure_altitude_to_qnh_altitude(3_000.0, 983.0);
        let delta_low = low - 3_000.0;
        assert!(delta_low < 0.0, "low QNH must lower the altitude");
        assert!(
            (-950.0..=-750.0).contains(&delta_low),
            "≈27–30 ft/hPa expected, got {delta_low} ft"
        );

        // 20 hPa high (1033.25): expect ≈ +550 ft.
        let high = pressure_altitude_to_qnh_altitude(3_000.0, 1_033.25);
        let delta_high = high - 3_000.0;
        assert!(
            (480.0..=620.0).contains(&delta_high),
            "≈27–30 ft/hPa expected, got {delta_high} ft"
        );
    }

    /// The conversion is monotonic in QNH and consistent (higher QNH ⇒
    /// higher indicated altitude for the same pressure). REQ: FR-TRK-041
    #[test]
    fn conversion_is_monotonic_in_qnh() {
        let mut last = f64::NEG_INFINITY;
        for qnh in [960.0, 980.0, 1000.0, 1013.25, 1030.0, 1050.0] {
            let alt = pressure_altitude_to_qnh_altitude(5_000.0, qnh);
            assert!(alt > last, "QNH {qnh} broke monotonicity");
            last = alt;
        }
    }
}
