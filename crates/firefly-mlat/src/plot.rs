//! Map a decoded CAT020 MLAT report to a tracker [`Plot`] (FEP.5).
//!
//! The semantic core of the adapter: it turns a [`DecodedMlatReport`] into a
//! **geodetic** plot with the measurement uncertainty taken from the
//! report's own **I020/500 SDP** (the standard deviation the MLAT system
//! computed for exactly this position solution) — the honest per-report
//! quality signal, analogous to FEP.3's NACp-derived σ for ADS-B.
//!
//! Drop rules (what must not enter the air picture):
//! - **No position or no time** — not a measurement.
//! - **Field monitor (RAB)** — a fixed test transponder for system
//!   calibration, never a real aircraft.
//! - **Simulated/test target (SIM/TST)** — Firefly carries no simulated
//!   traffic (FR-TRK-036).
//! - **Ground bit (GBS)** — a surface target; Firefly builds the *air*
//!   situation, and surface targets would pollute association near airports.
//!
//! MLAT positions are computed from **Mode S transponder signals**, so the
//! plots carry `SourceKind::ModeS` — honest about the underlying technology.
//! A dedicated MLAT provenance (own `SourceKind` variant + an I062/290 MLT
//! age subfield) would be an output-ICD change and is deliberately a
//! follow-up step.

use firefly_asterix::DecodedMlatReport;
use firefly_core::{Daps, ModeAC, Plot, SensorId, SourceKind};
use firefly_geo::Wgs84;

/// σ for a report without a usable I020/500 SDP: WAM accuracy varies from
/// tens of metres (terminal area) to a few hundred (wide-area fringe), so a
/// system that reports no quality gets a deliberately **conservative**
/// 150 m — it contributes weakly instead of being over-trusted.
pub const DEFAULT_SIGMA_POS_M: f64 = 150.0;

/// Feet → metres.
const FT_TO_M: f64 = 0.3048;

/// Turn one decoded CAT020 report into a [`Plot`], or `None` when it must
/// not become a measurement (see the module-level drop rules). The `sensor`
/// is the configured [`SensorId`] of the MLAT system.
pub fn mlat_report_to_plot(report: &DecodedMlatReport, sensor: SensorId) -> Option<Plot> {
    if report.field_monitor || report.simulated_or_test || report.ground_bit {
        return None;
    }
    let (lat_deg, lon_deg) = report.position_deg?;
    let time = report.time?;

    // Altitude: geometric (WGS-84) height when the system provides it,
    // otherwise the barometric level as an approximation, otherwise 0.
    let altitude_m = report
        .geometric_height_ft
        .or(report.flight_level_ft)
        .unwrap_or(0.0)
        * FT_TO_M;

    let mode_ac = ModeAC {
        mode_3a: report.mode_3a,
        flight_level_ft: report.flight_level_ft,
        icao_address: report.icao_address,
        callsign: report.callsign,
        spi: report.spi,
        daps: Daps::default(),
    };
    Some(Plot::geodetic(
        sensor,
        time,
        Wgs84::from_degrees(lat_deg, lon_deg, altitude_m),
        report.sigma_pos_m.unwrap_or(DEFAULT_SIGMA_POS_M),
        mode_ac,
        SourceKind::ModeS,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::{Callsign, DetectionKind, Measurement, Timestamp};

    fn report() -> DecodedMlatReport {
        DecodedMlatReport {
            sac: 25,
            sic: 10,
            time: Some(Timestamp(3600.0)),
            position_deg: Some((50.0, 8.5)),
            flight_level_ft: Some(9_800.0),
            geometric_height_ft: Some(10_000.0),
            mode_3a: Some(0o1213),
            icao_address: Some(0x3C_6589),
            callsign: Some(Callsign::new("DLH123")),
            track_number: Some(42),
            sigma_pos_m: Some(35.0),
            field_monitor: false,
            simulated_or_test: false,
            ground_bit: false,
            spi: true,
        }
    }

    /// A clean airborne report becomes a geodetic Mode-S plot: identity and
    /// SPI carried, altitude from the geometric height, σ from I020/500 SDP.
    /// REQ: FR-NET-017
    #[test]
    fn airborne_report_maps_to_geodetic_plot() {
        let plot = mlat_report_to_plot(&report(), SensorId(240)).expect("plot");
        assert_eq!(plot.sensor, SensorId(240));
        assert_eq!(plot.time, Timestamp(3600.0));
        assert_eq!(plot.kind, DetectionKind::Secondary);
        assert_eq!(plot.source, SourceKind::ModeS);
        let Measurement::Geodetic {
            position,
            sigma_pos_m,
        } = plot.measurement
        else {
            panic!("geodetic measurement expected");
        };
        assert!((position.lat_deg() - 50.0).abs() < 1e-9);
        assert!((position.lon_deg() - 8.5).abs() < 1e-9);
        assert!((position.height - 10_000.0 * FT_TO_M).abs() < 1e-6);
        assert_eq!(sigma_pos_m, 35.0, "per-report SDP σ");
        assert_eq!(plot.mode_ac.icao_address, Some(0x3C_6589));
        assert_eq!(plot.mode_ac.callsign, Some(Callsign::new("DLH123")));
        assert!(plot.mode_ac.spi);
    }

    /// Without a reported SDP the conservative default σ applies — a system
    /// that reports no quality is not over-trusted. REQ: FR-NET-017
    #[test]
    fn missing_sdp_gets_the_conservative_default() {
        let mut no_sigma = report();
        no_sigma.sigma_pos_m = None;
        let plot = mlat_report_to_plot(&no_sigma, SensorId(240)).unwrap();
        let Measurement::Geodetic { sigma_pos_m, .. } = plot.measurement else {
            unreachable!()
        };
        assert_eq!(sigma_pos_m, DEFAULT_SIGMA_POS_M);
    }

    /// Field-monitor, simulated/test, surface, position-less and time-less
    /// reports form no plot. REQ: FR-NET-017
    #[test]
    fn drop_rules_keep_the_air_picture_clean() {
        for mutate in [
            (|r: &mut DecodedMlatReport| r.field_monitor = true) as fn(&mut DecodedMlatReport),
            |r| r.simulated_or_test = true,
            |r| r.ground_bit = true,
            |r| r.position_deg = None,
            |r| r.time = None,
        ] {
            let mut r = report();
            mutate(&mut r);
            assert!(mlat_report_to_plot(&r, SensorId(240)).is_none());
        }
    }

    /// Without a geometric height the barometric level approximates the
    /// altitude; without either the plot sits at 0 m rather than being lost.
    /// REQ: FR-NET-017
    #[test]
    fn altitude_falls_back_from_geometric_to_baro_to_zero() {
        let mut baro_only = report();
        baro_only.geometric_height_ft = None;
        let plot = mlat_report_to_plot(&baro_only, SensorId(240)).unwrap();
        let Measurement::Geodetic { position, .. } = plot.measurement else {
            unreachable!()
        };
        assert!((position.height - 9_800.0 * FT_TO_M).abs() < 1e-6);

        let mut none = report();
        none.geometric_height_ft = None;
        none.flight_level_ft = None;
        let plot = mlat_report_to_plot(&none, SensorId(240)).unwrap();
        let Measurement::Geodetic { position, .. } = plot.measurement else {
            unreachable!()
        };
        assert_eq!(position.height, 0.0);
    }
}
