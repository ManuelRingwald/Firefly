//! A small end-to-end demonstration of the M1 simulator.
//!
//! Run with:
//!
//! ```text
//! cargo run --example demo -p firefly-sim
//! ```
//!
//! It builds a scenario with one radar near Munich and two aircraft — one
//! cruising straight, one flying a turn — then prints the first plots of the
//! resulting radar stream.

use firefly_core::{DetectionKind, Sensor, SensorId, TargetId};
use firefly_geo::{Enu, Wgs84};
use firefly_sim::{Leg, Radar, RadarParams, Scenario, State, Target};

fn main() {
    let origin = Wgs84::from_degrees(48.3538, 11.7861, 453.0); // ~Munich Airport

    let radar = Radar::new(
        Sensor::new(SensorId(1), origin),
        RadarParams {
            scan_period: 4.0,
            prob_detection: 0.95,
            ..RadarParams::default()
        },
    );

    // Aircraft A: cruising due east at FL350, full SSR fit.
    let alpha = Target {
        id: TargetId(1),
        initial: State {
            position: Enu::new(-40_000.0, 10_000.0, 10_668.0), // FL350 ≈ 10668 m
            speed: 250.0,
            heading: 90.0_f64.to_radians(),
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(300.0)],
        mode_3a: Some(0o1000),
        icao_address: Some(0x3C_65_AC),
        callsign: None,
    };

    // Aircraft B: climbing northbound, then a right turn — a primary-only
    // contact (no transponder), the harder case for the future tracker.
    let bravo = Target {
        id: TargetId(2),
        initial: State {
            position: Enu::new(20_000.0, -30_000.0, 3000.0),
            speed: 180.0,
            heading: 0.0,
            climb_rate: 0.0,
        },
        legs: vec![
            Leg::climb(120.0, 8.0),
            Leg::turn(60.0, 3.0),
            Leg::cruise(120.0),
        ],
        mode_3a: None,
        icao_address: None,
        callsign: None,
    };

    let scenario = Scenario::new(origin)
        .with_duration(300.0)
        .with_seed(20260606)
        .add_radar(radar)
        .add_target(alpha)
        .add_target(bravo);

    let plots = firefly_sim::run(&scenario);

    println!(
        "Firefly M1 simulator — scenario produced {} plots\n",
        plots.len()
    );
    println!(
        "{:>8}  {:>6}  {:>9}  {:>10}  {:>8}  {:>8}",
        "time[s]", "sensor", "kind", "range[km]", "az[deg]", "FL"
    );
    println!("{}", "-".repeat(60));

    for plot in plots.iter().take(16) {
        let fl = match plot.mode_ac.flight_level_ft {
            Some(ft) => format!("FL{:03.0}", ft / 100.0),
            None => "  —".to_string(),
        };
        let kind = match plot.kind {
            DetectionKind::Primary => "PSR",
            DetectionKind::Secondary => "SSR",
            DetectionKind::Combined => "PSR+SSR",
        };
        println!(
            "{:>8.1}  {:>6}  {:>9}  {:>10.2}  {:>8.2}  {:>8}",
            plot.time.as_secs(),
            plot.sensor.0,
            kind,
            plot.measurement.range / 1000.0,
            plot.measurement.azimuth_deg(),
            fl,
        );
    }
    if plots.len() > 16 {
        println!("... ({} more plots)", plots.len() - 16);
    }
}
