//! End-to-end lifecycle tests: drive the full tracker with the simulator and
//! check it births, confirms and maintains tracks — including two crossing
//! targets that must keep their separate identities (the payoff of GNN).
//!
//! REQ: FR-TRK-001, FR-TRK-006

use firefly_core::{Plot, Sensor, SensorId, TargetId};
use firefly_geo::{Enu, Wgs84};
use firefly_sim::{Leg, Radar, RadarParams, Scenario, State, Target};
use firefly_track::{SensorErrorModel, Tracker, TrackerConfig};

/// A clean, deterministic radar: every target in coverage is seen every scan,
/// with range/azimuth noise but no elevation noise (2-D focus).
fn clean_radar(origin: Wgs84) -> Radar {
    Radar::new(
        Sensor::new(SensorId(1), origin),
        RadarParams {
            scan_period: 4.0,
            prob_detection: 1.0,
            sigma_range: 50.0,
            sigma_azimuth: 0.08_f64.to_radians(),
            sigma_elevation: 0.0,
            has_ssr: false,
            ..RadarParams::default()
        },
    )
}

/// Run a scenario's plot stream through the tracker, scan by scan.
fn run_tracker(scenario: &Scenario) -> Tracker {
    let plots = firefly_sim::run(scenario);
    let model = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08);
    let mut tracker = Tracker::new(TrackerConfig::new(model));

    let mut i = 0;
    while i < plots.len() {
        let t = plots[i].time;
        let mut batch: Vec<Plot> = Vec::new();
        while i < plots.len() && (plots[i].time.as_secs() - t.as_secs()).abs() < 1e-9 {
            batch.push(plots[i]);
            i += 1;
        }
        tracker.process_scan(t, &batch);
    }
    tracker
}

#[test]
fn single_target_yields_one_confirmed_track() {
    let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);
    let target = Target {
        id: TargetId(1),
        initial: State {
            position: Enu::new(20_000.0, 0.0, 3000.0),
            speed: 150.0,
            heading: 0.0, // due north
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(160.0)],
        mode_3a: None,
        icao_address: None,
    };
    let scenario = Scenario::new(origin)
        .with_duration(160.0)
        .with_seed(20260606)
        .add_radar(clean_radar(origin))
        .add_target(target);

    let tracker = run_tracker(&scenario);

    assert_eq!(
        tracker.confirmed_count(),
        1,
        "expected exactly one confirmed track"
    );
    let track = tracker.confirmed_tracks().next().unwrap();
    let v = track.velocity();
    assert!(v[0].abs() < 15.0, "v_east ~0, was {:.1}", v[0]);
    assert!((v[1] - 150.0).abs() < 15.0, "v_north ~150, was {:.1}", v[1]);
}

/// Two targets cross (their east order swaps) while staying 1 km apart in north.
/// The tracker must hold exactly two confirmed tracks with opposite east
/// velocities — no identity swap, no spurious or lost tracks.
#[test]
fn two_crossing_targets_keep_their_identities() {
    let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);

    // Eastbound, north = 40 km.
    let eastbound = Target {
        id: TargetId(1),
        initial: State {
            position: Enu::new(-30_000.0, 40_000.0, 3000.0),
            speed: 150.0,
            heading: 90.0_f64.to_radians(),
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(360.0)],
        mode_3a: None,
        icao_address: None,
    };
    // Westbound, north = 41 km (1 km apart, so always distinguishable).
    let westbound = Target {
        id: TargetId(2),
        initial: State {
            position: Enu::new(30_000.0, 41_000.0, 3000.0),
            speed: 150.0,
            heading: 270.0_f64.to_radians(),
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(360.0)],
        mode_3a: None,
        icao_address: None,
    };

    let scenario = Scenario::new(origin)
        .with_duration(360.0)
        .with_seed(20260606)
        .add_radar(clean_radar(origin))
        .add_target(eastbound)
        .add_target(westbound);

    let tracker = run_tracker(&scenario);

    assert_eq!(
        tracker.confirmed_count(),
        2,
        "expected exactly two confirmed tracks"
    );

    let velocities: Vec<_> = tracker.confirmed_tracks().map(|t| t.velocity()).collect();
    let has_eastbound = velocities.iter().any(|v| v[0] > 100.0 && v[1].abs() < 30.0);
    let has_westbound = velocities
        .iter()
        .any(|v| v[0] < -100.0 && v[1].abs() < 30.0);
    assert!(
        has_eastbound,
        "expected an eastbound track, got {velocities:?}"
    );
    assert!(
        has_westbound,
        "expected a westbound track, got {velocities:?}"
    );
}
