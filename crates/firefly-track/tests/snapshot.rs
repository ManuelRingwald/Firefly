//! Snapshot & replay.
//!
//! Two distinct properties, tested separately and honestly:
//!
//! 1. **Determinism** (NFR-CLOUD-001/002): two identical runs produce a
//!    *bit-identical* state — no serialisation involved.
//! 2. **Recoverability** (NFR-CLOUD-003): a serialised snapshot restores the
//!    state to full numeric precision and then continues equivalently.
//!
//! Note on the snapshot format: JSON is a *text* format, and an `f64` text
//! round-trip is not guaranteed bit-exact down to the last unit-in-the-last-
//! place. We therefore compare restored state with a tight tolerance. A
//! byte-exact production snapshot would use a binary codec; the format is an
//! edge concern (the core only derives the serde traits and stays neutral).

use firefly_core::{Plot, Sensor, SensorId, TargetId, Timestamp};
use firefly_geo::{Enu, LocalFrame, Wgs84};
use firefly_sim::{Leg, Radar, RadarParams, Scenario, State, Target};
use firefly_track::{SensorErrorModel, Tracker, TrackerConfig};

/// Group a time-ordered plot stream into per-scan batches.
fn scans(plots: &[Plot]) -> Vec<(f64, Vec<Plot>)> {
    let mut out: Vec<(f64, Vec<Plot>)> = Vec::new();
    let mut i = 0;
    while i < plots.len() {
        let t = plots[i].time.as_secs();
        let mut batch = Vec::new();
        while i < plots.len() && (plots[i].time.as_secs() - t).abs() < 1e-9 {
            batch.push(plots[i]);
            i += 1;
        }
        out.push((t, batch));
    }
    out
}

fn demo_scenario() -> Scenario {
    let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);
    let radar = Radar::new(
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
    );
    let target = Target {
        id: TargetId(1),
        initial: State {
            position: Enu::new(20_000.0, -10_000.0, 3000.0),
            speed: 170.0,
            heading: 20.0_f64.to_radians(),
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(200.0)],
        mode_3a: None,
        icao_address: None,
    };
    Scenario::new(origin)
        .with_duration(200.0)
        .with_seed(20260609)
        .add_radar(radar)
        .add_target(target)
}

fn new_tracker() -> Tracker {
    let frame = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
    Tracker::new(TrackerConfig::single_sensor(
        SensorId(1),
        frame,
        SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08),
    ))
}

/// Same tracks, same ids/status, and positions/velocities within `tol` metres
/// (resp. m/s).
fn assert_close(a: &Tracker, b: &Tracker, tol: f64) {
    assert_eq!(a.tracks().len(), b.tracks().len(), "track count");
    for (ta, tb) in a.tracks().iter().zip(b.tracks()) {
        assert_eq!(ta.id(), tb.id(), "track id");
        assert_eq!(ta.status(), tb.status(), "track status");
        assert!(
            (ta.position() - tb.position()).norm() < tol,
            "position within {tol}"
        );
        assert!(
            (ta.velocity() - tb.velocity()).norm() < tol,
            "velocity within {tol}"
        );
    }
}

/// Two independent runs over the same scans end in a bit-identical state.
/// REQ: NFR-CLOUD-001, NFR-CLOUD-002
#[test]
fn replay_is_deterministic() {
    let plots = firefly_sim::run(&demo_scenario());
    let all = scans(&plots);

    let mut a = new_tracker();
    let mut b = new_tracker();
    for (t, batch) in &all {
        a.process_scan(Timestamp(*t), batch);
        b.process_scan(Timestamp(*t), batch);
    }
    assert_eq!(a, b, "deterministic processing must give identical state");
}

/// A serialised snapshot restores the state to full numeric precision.
/// REQ: NFR-CLOUD-003
#[test]
fn snapshot_roundtrip_recovers_state() {
    let plots = firefly_sim::run(&demo_scenario());
    let mut tracker = new_tracker();
    for (t, batch) in scans(&plots).into_iter().take(10) {
        tracker.process_scan(Timestamp(t), &batch);
    }

    let json = serde_json::to_string(&tracker).expect("serialise");
    let restored: Tracker = serde_json::from_str(&json).expect("deserialise");

    assert_close(&tracker, &restored, 1e-6);
}

/// A snapshot restored mid-run continues equivalently to the original.
/// REQ: NFR-CLOUD-003
#[test]
fn restored_snapshot_continues_equivalently() {
    let plots = firefly_sim::run(&demo_scenario());
    let all = scans(&plots);
    let split = all.len() / 2;

    let mut original = new_tracker();
    for (t, batch) in &all[..split] {
        original.process_scan(Timestamp(*t), batch);
    }

    let json = serde_json::to_string(&original).expect("serialise");
    let mut restored: Tracker = serde_json::from_str(&json).expect("deserialise");

    for (t, batch) in &all[split..] {
        original.process_scan(Timestamp(*t), batch);
        restored.process_scan(Timestamp(*t), batch);
    }

    assert_close(&original, &restored, 1e-6);
    assert_eq!(
        original.confirmed_count(),
        1,
        "sanity: the target is tracked"
    );
}
