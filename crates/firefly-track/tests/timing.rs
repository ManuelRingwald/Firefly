//! Timing robustness: Firefly must not throw tracks away because of *time*.
//!
//! The tracker is driven purely by **data-time**; its lifecycle counts misses
//! (scans without a plot), never elapsed wall-clock or data-time. Two pointed
//! demonstrations of `NFR-CLOUD-004`:
//!
//! 1. A long gap in the scan times, *with* a plot before and after, keeps the
//!    track and its identity — a delayed-but-present scan is a non-event.
//! 2. Deletion is governed by the **number** of consecutive misses, not by how
//!    much time those misses span — a slow feed does not prematurely kill a
//!    track.
//!
//! REQ: NFR-CLOUD-004

use firefly_core::{Plot, SensorId, Timestamp};
use firefly_geo::{LocalFrame, Polar, Wgs84};
use firefly_track::{SensorErrorModel, Tracker, TrackerConfig};

fn config() -> TrackerConfig {
    let frame = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
    TrackerConfig::single_sensor(
        SensorId(1),
        frame,
        SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08),
    )
}

/// A primary plot at a given polar position for a given time.
fn plot(time: f64, range: f64, az: f64) -> Plot {
    Plot::primary(SensorId(1), Timestamp(time), Polar::new(range, az, 0.0))
}

/// A long gap between two scans — with a plot on both sides — keeps the track
/// alive and keeps its identity: the late plot is associated to the existing
/// track, not spawned as a new one. REQ: NFR-CLOUD-004
#[test]
fn long_gap_with_data_keeps_track_identity() {
    let mut tracker = Tracker::new(config());

    // A target due north (azimuth 0), so range == north position, moving north
    // at 150 m/s from 50 km out. Plots are exact (no noise) for full control.
    let moving = |t: f64| plot(t, 50_000.0 + 150.0 * t, 0.0);

    // Confirm over three regular scans.
    for k in 0..3 {
        let t = k as f64 * 4.0;
        tracker.process_scan(Timestamp(t), &[moving(t)]);
    }
    assert_eq!(tracker.confirmed_count(), 1);
    let id_before = tracker.tracks()[0].id();

    // Now a *long* gap: the next scan arrives only at t = 24 s (16 s instead of
    // 4 s). The plot sits where constant-velocity prediction expects it.
    tracker.process_scan(Timestamp(24.0), &[moving(24.0)]);

    // The track survived the gap with the *same* identity, and no spurious new
    // track was born from the "late" plot.
    assert_eq!(
        tracker.tracks().len(),
        1,
        "no spurious track from the late plot"
    );
    assert!(tracker.tracks()[0].is_confirmed(), "track stays confirmed");
    assert_eq!(
        tracker.tracks()[0].id(),
        id_before,
        "identity preserved across the gap"
    );

    // It still tracks the northbound velocity.
    let v = tracker.tracks()[0].velocity();
    assert!(
        v[0].abs() < 20.0 && (v[1] - 150.0).abs() < 20.0,
        "velocity kept"
    );
}

/// Deletion depends on the *number* of consecutive misses, not on how much time
/// they span: closely- and widely-spaced misses delete after the same count.
/// REQ: NFR-CLOUD-004
#[test]
fn deletion_is_governed_by_miss_count_not_elapsed_time() {
    // Confirm a stationary track, then feed empty scans `spacing` apart and
    // count how many it takes to delete it.
    fn misses_until_deletion(spacing: f64) -> usize {
        let mut tracker = Tracker::new(config());
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_scan(Timestamp(t), &[plot(t, 50_000.0, 0.0)]);
        }
        assert_eq!(tracker.confirmed_count(), 1);

        let mut t = 8.0;
        let mut misses = 0;
        loop {
            t += spacing;
            tracker.process_scan(Timestamp(t), &[]);
            misses += 1;
            if tracker.tracks().is_empty() {
                return misses;
            }
            assert!(misses <= 100, "track was never deleted");
        }
    }

    let tight = misses_until_deletion(4.0);
    let sparse = misses_until_deletion(100.0); // 25× the elapsed time

    assert_eq!(
        tight, sparse,
        "deletion is by miss count, independent of elapsed time"
    );
    assert_eq!(tight, 4, "delete_misses_confirmed default");
}
