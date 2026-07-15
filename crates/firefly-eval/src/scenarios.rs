//! Built-in benchmark scenarios (HA.4): small, deterministic scenes with
//! known truth, shared by the CLI and the regression tests so a number in
//! CI and a number on an operator's screen mean the same thing.
//!
//! REQ: FR-TRK-051

use firefly_core::{Sensor, SensorId, TargetId};
use firefly_geo::{Enu, Wgs84};
use firefly_sim::{Leg, Radar, RadarParams, Scenario, State, Target};

/// The scenario origin shared by the built-ins.
fn origin() -> Wgs84 {
    Wgs84::from_degrees(48.0, 11.0, 0.0)
}

/// A medium-range radar at the origin (PSR-only, tuned like the historic
/// quality test so the numbers stay comparable).
fn radar(scan_period: f64) -> Radar {
    Radar::new(
        Sensor::new(SensorId(1), origin()),
        RadarParams {
            scan_period,
            prob_detection: 1.0,
            sigma_range: 50.0,
            sigma_azimuth: 0.08_f64.to_radians(),
            sigma_elevation: 0.0,
            has_ssr: false,
            ..RadarParams::default()
        },
    )
}

fn cruiser(id: u32, east0: f64, north0: f64, heading: f64, duration: f64) -> Target {
    Target {
        id: TargetId(id),
        initial: State {
            position: Enu::new(east0, north0, 3000.0),
            speed: 150.0,
            heading,
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(duration)],
        mode_3a: None,
        icao_address: None,
        callsign: None,
    }
}

/// One aircraft cruising due north for 240 s — the canonical single-target
/// quality benchmark (the historic `single_target_quality_meets_thresholds`
/// shape).
pub fn single_target() -> Scenario {
    single_target_with_detection(1.0)
}

/// The single-target benchmark with a configurable per-scan probability of
/// detection — deliberately public so a test can prove the PD metric
/// *bites*: a degraded sensor must show up as a measurably lower score.
pub fn single_target_with_detection(prob_detection: f64) -> Scenario {
    let duration = 240.0;
    let mut r = radar(4.0);
    r.params.prob_detection = prob_detection;
    Scenario::new(origin())
        .with_duration(duration)
        .with_seed(20260610)
        .add_radar(r)
        .add_target(cruiser(1, 20_000.0, 0.0, 0.0, duration))
}

/// Two well-separated aircraft cruising in parallel — the multi-target
/// bookkeeping benchmark (each aircraft must be exactly one track, no
/// ghosts).
pub fn parallel_pair() -> Scenario {
    let duration = 240.0;
    Scenario::new(origin())
        .with_duration(duration)
        .with_seed(20260610)
        .add_radar(radar(4.0))
        .add_target(cruiser(1, 20_000.0, 0.0, 0.0, duration))
        .add_target(cruiser(2, 30_000.0, 5_000.0, 0.0, duration))
}

/// The built-in benchmark set: `(name, scenario)`.
pub fn builtin() -> Vec<(&'static str, Scenario)> {
    vec![("single", single_target()), ("pair", parallel_pair())]
}
