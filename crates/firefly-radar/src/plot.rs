//! Map a decoded CAT048 target report to a tracker [`Plot`] (ADR 0028).
//!
//! This is the adapter's semantic core: it turns the neutral
//! [`DecodedTargetReport`] (range/azimuth, identity, detection kind) into a
//! [`firefly_core::Plot`] with a **polar** measurement and the right
//! [`DetectionKind`]/[`SourceKind`] derived from the report's TYP — so the
//! tracker's fusion and the per-track provenance (ADR 0027) see a real radar
//! exactly as they would the simulator's polar plots.

use firefly_asterix::{DecodedTargetReport, Detection};
use firefly_core::{DetectionKind, Measurement, ModeAC, Plot, SensorId, SourceKind};

/// Derive the tracker's `(DetectionKind, SourceKind)` from a CAT048 detection
/// type. Returns `None` for a no-detection record (no plot to form).
fn detection_kinds(detection: Detection) -> Option<(DetectionKind, SourceKind)> {
    Some(match detection {
        Detection::NoDetection => return None,
        Detection::Psr => (DetectionKind::Primary, SourceKind::Psr),
        Detection::Ssr => (DetectionKind::Secondary, SourceKind::Ssr),
        Detection::SsrPsr => (DetectionKind::Combined, SourceKind::Ssr),
        Detection::ModeSAllCall | Detection::ModeSRollCall => {
            (DetectionKind::Secondary, SourceKind::ModeS)
        }
        Detection::ModeSAllCallPsr | Detection::ModeSRollCallPsr => {
            (DetectionKind::Combined, SourceKind::ModeS)
        }
    })
}

/// Turn one decoded target report into a [`Plot`], or `None` when it cannot
/// become a measurement: no measured position (a track-only update), no time of
/// day, or a no-detection record. The `sensor` is the configured [`SensorId`]
/// of the radar (CAT048 carries SAC/SIC, not the tracker's sensor identity).
pub fn target_report_to_plot(report: &DecodedTargetReport, sensor: SensorId) -> Option<Plot> {
    let measurement = report.position?;
    let time = report.time?;
    let (kind, source) = detection_kinds(report.detection)?;
    let mode_ac = ModeAC {
        mode_3a: report.mode_3a,
        flight_level_ft: report.flight_level_ft,
        icao_address: report.icao_address,
        callsign: report.callsign,
        spi: report.spi,
    };
    Some(Plot {
        sensor,
        time,
        measurement: Measurement::Polar(measurement),
        kind,
        source,
        mode_ac,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::{Callsign, Timestamp};
    use firefly_geo::Polar;

    fn report(detection: Detection) -> DecodedTargetReport {
        DecodedTargetReport {
            sac: 1,
            sic: 4,
            time: Some(Timestamp(10.0)),
            detection,
            position: Some(Polar::new(50_000.0, 0.0, 0.0)),
            mode_3a: Some(0o1200),
            flight_level_ft: Some(35_000.0),
            icao_address: Some(0x3C_6589),
            callsign: Some(Callsign::new("DLH123")),
            track_number: Some(42),
            spi: false,
        }
    }

    /// A combined SSR+PSR report becomes a `Combined`/`Ssr` polar plot carrying
    /// the full identity. REQ: FR-NET-013
    #[test]
    fn combined_report_maps_to_combined_ssr_polar_plot() {
        let plot = target_report_to_plot(&report(Detection::SsrPsr), SensorId(220)).unwrap();
        assert_eq!(plot.sensor, SensorId(220));
        assert_eq!(plot.kind, DetectionKind::Combined);
        assert_eq!(plot.source, SourceKind::Ssr);
        assert!(matches!(plot.measurement, Measurement::Polar(_)));
        assert_eq!(plot.mode_ac.mode_3a, Some(0o1200));
        assert_eq!(plot.mode_ac.icao_address, Some(0x3C_6589));
        assert_eq!(plot.time, Timestamp(10.0));
    }

    /// Each detection class maps to the documented kind/source pair (ADR 0028 §3).
    /// REQ: FR-NET-013
    #[test]
    fn detection_classes_map_to_kind_and_source() {
        let cases = [
            (Detection::Psr, DetectionKind::Primary, SourceKind::Psr),
            (Detection::Ssr, DetectionKind::Secondary, SourceKind::Ssr),
            (Detection::SsrPsr, DetectionKind::Combined, SourceKind::Ssr),
            (
                Detection::ModeSRollCall,
                DetectionKind::Secondary,
                SourceKind::ModeS,
            ),
            (
                Detection::ModeSRollCallPsr,
                DetectionKind::Combined,
                SourceKind::ModeS,
            ),
        ];
        for (det, kind, source) in cases {
            let plot = target_report_to_plot(&report(det), SensorId(220)).unwrap();
            assert_eq!(plot.kind, kind, "kind for {det:?}");
            assert_eq!(plot.source, source, "source for {det:?}");
        }
    }

    /// A no-detection record, or one without a measured position or time, forms
    /// no plot. REQ: FR-NET-013
    #[test]
    fn unmeasurable_reports_form_no_plot() {
        assert!(target_report_to_plot(&report(Detection::NoDetection), SensorId(220)).is_none());

        let mut no_position = report(Detection::Psr);
        no_position.position = None;
        assert!(target_report_to_plot(&no_position, SensorId(220)).is_none());

        let mut no_time = report(Detection::Psr);
        no_time.time = None;
        assert!(target_report_to_plot(&no_time, SensorId(220)).is_none());
    }
}
