//! Identity-stability regression: each aircraft must keep **one** track id.
//!
//! This guards against a class of defect that the earlier quality test
//! (`metrics::single_target_quality_meets_thresholds`) could not see, because
//! that test deliberately zeroes the elevation noise and flies a single,
//! non-manoeuvring target. Two real effects were therefore untested and both
//! fractured tracks into a runaway sequence of new ids:
//!
//! 1. **Elevation noise → ground-range uncertainty.** At en-route altitude the
//!    1° elevation spread scatters the *ground* range by far more than the slant
//!    range. If the converted-measurement covariance ignores it (FR-TRK-002) the
//!    validation gate is far too tight, the legitimate plot is rejected, and a
//!    duplicate track is born.
//! 2. **Manoeuvre vs. process noise.** A constant-velocity filter tuned for
//!    straight flight cannot follow even a gentle turn; the gate misses the
//!    plot and the track fractures. The process noise must match the manoeuvre.
//!
//! The scenario below reproduces both conditions (high altitude, default radar
//! with elevation noise, one straight + one turning target) and asserts that the
//! tracker holds exactly one identity per target — no churn, no duplicates.
//!
//! REQ: FR-TRK-002, FR-TRK-006

use std::collections::BTreeSet;

use firefly_core::{Sensor, SensorId, TargetId};
use firefly_geo::{Enu, Wgs84};
use firefly_sim::{Leg, Radar, RadarParams, Scenario, State, Target};
use firefly_track::{ProcessNoise, SensorErrorModel, Tracker, TrackerConfig};

/// A straight, level, en-route aircraft (no manoeuvre) at 10 km.
fn eastbound() -> Target {
    Target {
        id: TargetId(1),
        initial: State {
            position: Enu::new(-70_000.0, -30_000.0, 10_000.0),
            speed: 240.0,
            heading: 90.0_f64.to_radians(),
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(180.0)],
        mode_3a: Some(0o1000),
        icao_address: Some(0x3C_00_01),
    }
}

/// A manoeuvring aircraft at 11 km: cruise, a gentle 1°/s turn, then cruise.
fn turning() -> Target {
    Target {
        id: TargetId(2),
        initial: State {
            position: Enu::new(40_000.0, -60_000.0, 11_000.0),
            speed: 210.0,
            heading: 0.0,
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(60.0), Leg::turn(90.0, 1.0), Leg::cruise(30.0)],
        mode_3a: Some(0o2000),
        icao_address: Some(0x3C_00_02),
    }
}

/// The tracker tuning that copes with en-route elevation noise and a gentle
/// turn — the same rationale the demo scene uses (elevation-aware error model +
/// manoeuvre-matched process noise).
fn tracker_config() -> TrackerConfig {
    let mut cfg = TrackerConfig::new(SensorErrorModel::from_polar_deg(50.0, 0.08, 1.0));
    cfg.process_noise = ProcessNoise::new(60.0);
    cfg
}

/// Drive the full two-target scene with **perfect detection** (so the run is
/// deterministic and every miss is the tracker's own fault) and assert that
/// exactly one track identity is created per target.
/// REQ: FR-TRK-002, FR-TRK-006
#[test]
fn two_aircraft_keep_one_identity_each_under_elevation_noise_and_a_turn() {
    let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);
    let radar = Radar::new(
        Sensor::new(SensorId(1), origin),
        RadarParams {
            prob_detection: 1.0,      // isolate tracker behaviour from detection luck
            ..RadarParams::default()  // keeps the 1° elevation noise that triggered the bug
        },
    );
    let scenario = Scenario::new(origin)
        .with_duration(180.0)
        .add_radar(radar)
        .add_target(eastbound())
        .add_target(turning());

    let mut tracker = Tracker::new(tracker_config());
    let plots = firefly_sim::run(&scenario);

    let mut distinct_ids: BTreeSet<u32> = BTreeSet::new();
    let mut max_concurrent = 0usize;

    let mut i = 0;
    while i < plots.len() {
        let time = plots[i].time;
        let start = i;
        while i < plots.len() && plots[i].time == time {
            i += 1;
        }
        tracker.process_scan(time, &plots[start..i]);

        max_concurrent = max_concurrent.max(tracker.tracks().len());
        for track in tracker.tracks() {
            distinct_ids.insert(track.id().0);
        }
    }

    // The decisive assertions: two targets ⇒ two ids, ever; never a spurious
    // third concurrent track. A regression (covariance or process noise) makes
    // both of these blow up immediately (we measured 24 ids / 4 concurrent).
    assert_eq!(
        distinct_ids.len(),
        2,
        "each aircraft must keep a single identity; got ids {distinct_ids:?}"
    );
    assert_eq!(
        max_concurrent, 2,
        "only the two real targets should ever be tracked at once"
    );

    // And both must actually be confirmed by the end (they are tracked, not lost).
    assert_eq!(
        tracker.confirmed_count(),
        2,
        "both aircraft should end as confirmed tracks"
    );
}
