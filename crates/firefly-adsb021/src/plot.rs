//! Map a decoded CAT021 ADS-B report to a tracker [`Plot`] (FEP.3).
//!
//! The semantic core of the adapter: it turns a [`DecodedAdsbReport`] into a
//! **geodetic** plot (the aircraft's own WGS-84 self-report) with a
//! measurement uncertainty derived from the report's **NACp** — the honest
//! quality signal a real ground station provides, in contrast to the fixed
//! assumption the internet adapters must make.
//!
//! Drop rules (what must not enter the air picture):
//! - **No position or no time** — not a measurement.
//! - **Ground bit set** — a surface report; Firefly builds the *air*
//!   situation, and surface targets would pollute association near airports.
//! - **Simulated/test target** — Firefly carries no simulated traffic
//!   (FR-TRK-036: the CAT062 SIM flag is honestly always 0).

use firefly_asterix::DecodedAdsbReport;
use firefly_core::{Daps, ModeAC, Plot, SensorId};
use firefly_geo::Wgs84;

/// σ for a report without a usable NACp (absent item or NACp 0 = "unknown
/// accuracy"): deliberately **worse** than the internet-feed assumption
/// (75 m) — a station that reports no quality earns less trust — but still
/// finite, so the plot contributes weakly instead of being discarded.
pub const DEFAULT_SIGMA_POS_M: f64 = 250.0;

/// Feet → metres.
const FT_TO_M: f64 = 0.3048;

/// Derive the 1σ horizontal position uncertainty (metres) from a NACp value
/// (ED-102A/DO-260B): each NACp bounds the 95 % Estimated Position
/// Uncertainty (EPU); σ ≈ EPU/2. Unknown or NACp 0 →
/// [`DEFAULT_SIGMA_POS_M`].
pub fn sigma_from_nacp(nacp: Option<u8>) -> f64 {
    match nacp {
        Some(11) => 1.5,
        Some(10) => 5.0,
        Some(9) => 15.0,
        Some(8) => 46.3,
        Some(7) => 92.6,
        Some(6) => 277.8,
        Some(5) => 463.0,
        Some(4) => 926.0,
        Some(3) => 1_852.0,
        Some(2) => 3_704.0,
        Some(1) => 9_260.0,
        _ => DEFAULT_SIGMA_POS_M,
    }
}

/// Turn one decoded CAT021 report into a [`Plot`], or `None` when it must
/// not become a measurement (see the module-level drop rules). The `sensor`
/// is the configured [`SensorId`] of the ground station.
pub fn adsb_report_to_plot(report: &DecodedAdsbReport, sensor: SensorId) -> Option<Plot> {
    if report.ground_bit || report.simulated_or_test {
        return None;
    }
    let (lat_deg, lon_deg) = report.position_deg?;
    let time = report.time?;

    // Altitude: geometric (WGS-84) height when the station provides it,
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
        spi: false,
        // I021/140 is a genuinely geometric (WGS-84) height — the vertical
        // chain (VERT.2) keeps it strictly separate from the barometric level.
        geometric_height_ft: report.geometric_height_ft,
        daps: Daps::default(),
    };
    Some(Plot::adsb(
        sensor,
        time,
        Wgs84::from_degrees(lat_deg, lon_deg, altitude_m),
        sigma_from_nacp(report.nacp),
        mode_ac,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::{Callsign, DetectionKind, Measurement, SourceKind, Timestamp};

    fn report() -> DecodedAdsbReport {
        DecodedAdsbReport {
            sac: 25,
            sic: 10,
            time: Some(Timestamp(3600.0)),
            position_deg: Some((50.0, 8.5)),
            geometric_height_ft: Some(10_000.0),
            flight_level_ft: Some(9_800.0),
            icao_address: Some(0x3C_6589),
            callsign: Some(Callsign::new("DLH123")),
            mode_3a: Some(0o1213),
            nacp: Some(9),
            ground_bit: false,
            simulated_or_test: false,
        }
    }

    /// A clean airborne report becomes a geodetic ADS-B plot: identity carried,
    /// altitude from the geometric height, σ from the NACp table.
    /// REQ: FR-NET-015
    #[test]
    fn airborne_report_maps_to_geodetic_plot() {
        let plot = adsb_report_to_plot(&report(), SensorId(230)).expect("plot");
        assert_eq!(plot.sensor, SensorId(230));
        assert_eq!(plot.time, Timestamp(3600.0));
        assert_eq!(plot.kind, DetectionKind::Secondary);
        assert_eq!(plot.source, SourceKind::AdsB);
        let Measurement::Geodetic {
            position,
            sigma_pos_m,
        } = plot.measurement
        else {
            panic!("geodetic measurement expected");
        };
        assert!((position.lat_deg() - 50.0).abs() < 1e-9);
        assert!((position.lon_deg() - 8.5).abs() < 1e-9);
        assert!((sigma_pos_m - 15.0).abs() < 1e-9, "NACp 9 → 15 m");
        assert_eq!(plot.mode_ac.icao_address, Some(0x3C_6589));
        assert_eq!(plot.mode_ac.callsign, Some(Callsign::new("DLH123")));
        assert_eq!(plot.mode_ac.mode_3a, Some(0o1213));
    }

    /// The NACp→σ table follows DO-260B (σ ≈ EPU/2); unknown or NACp 0 gets
    /// the conservative default — worse than the internet-feed assumption.
    /// REQ: FR-NET-015
    #[test]
    fn nacp_table_derives_sigma() {
        assert!((sigma_from_nacp(Some(11)) - 1.5).abs() < 1e-9);
        assert!((sigma_from_nacp(Some(8)) - 46.3).abs() < 1e-9);
        assert!((sigma_from_nacp(Some(1)) - 9_260.0).abs() < 1e-9);
        assert_eq!(sigma_from_nacp(Some(0)), DEFAULT_SIGMA_POS_M);
        // No quality signal → less trust than the internet-feed assumption
        // (75 m); asserted through the function, not the constant.
        assert!(sigma_from_nacp(None) > 75.0);
    }

    /// Surface, simulated/test, position-less and time-less reports form no
    /// plot. REQ: FR-NET-015
    #[test]
    fn drop_rules_keep_the_air_picture_clean() {
        let mut ground = report();
        ground.ground_bit = true;
        assert!(adsb_report_to_plot(&ground, SensorId(230)).is_none());

        let mut sim = report();
        sim.simulated_or_test = true;
        assert!(adsb_report_to_plot(&sim, SensorId(230)).is_none());

        let mut no_position = report();
        no_position.position_deg = None;
        assert!(adsb_report_to_plot(&no_position, SensorId(230)).is_none());

        let mut no_time = report();
        no_time.time = None;
        assert!(adsb_report_to_plot(&no_time, SensorId(230)).is_none());
    }

    /// Without a geometric height the barometric level approximates the
    /// altitude; without either the plot sits at 0 m rather than being lost.
    /// REQ: FR-NET-015
    #[test]
    fn altitude_falls_back_from_geometric_to_baro_to_zero() {
        let mut baro_only = report();
        baro_only.geometric_height_ft = None;
        let plot = adsb_report_to_plot(&baro_only, SensorId(230)).unwrap();
        let Measurement::Geodetic { position, .. } = plot.measurement else {
            unreachable!()
        };
        assert!((position.height - 9_800.0 * FT_TO_M).abs() < 1e-6);

        let mut none = report();
        none.geometric_height_ft = None;
        none.flight_level_ft = None;
        let plot = adsb_report_to_plot(&none, SensorId(230)).unwrap();
        let Measurement::Geodetic { position, .. } = plot.measurement else {
            unreachable!()
        };
        assert_eq!(position.height, 0.0);
    }
}
