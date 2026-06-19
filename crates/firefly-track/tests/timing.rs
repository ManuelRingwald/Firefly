//! Timing robustness: Firefly must not throw tracks away because of *time*.
//!
//! The tracker is driven purely by **data-time**; its lifecycle counts misses
//! (scans without a plot), never elapsed wall-clock or data-time. Demonstrations:
//!
//! 1. A long gap in the scan times, *with* a plot before and after, keeps the
//!    track and its identity — a delayed-but-present scan is a non-event.
//! 2. Deletion is governed by the **number** of consecutive misses, not by how
//!    much time those misses span — a slow feed does not prematurely kill a
//!    track.
//! 3. A plot batch fed in reverse time order produces the same result as one
//!    fed in forward order — `process_plots` sorts internally (FR-TRK-033).
//! 4. A late plot batch (data time < watermark) is dropped gracefully without
//!    corrupting the track state (FR-TRK-033).
//! 5. A backward scan (data time ≤ previous scan) is dropped gracefully
//!    without corrupting the track state (FR-TRK-033).
//!
//! REQ: NFR-CLOUD-004, FR-TRK-033

use firefly_core::{Plot, SensorId, Timestamp};
use firefly_geo::{LocalFrame, Polar, Wgs84};
use firefly_track::{SensorErrorModel, Tracker, TrackerConfig};

fn config() -> TrackerConfig {
    let frame = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
    TrackerConfig::single_sensor(
        SensorId(1),
        frame,
        SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08),
        4.0,
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

/// `process_plots` sorts plots by data time before processing, so input order
/// must not affect the result. A batch fed in reverse time order must confirm
/// the same single track as the same batch fed in forward order.
///
/// REQ: FR-TRK-033
#[test]
fn unsorted_plot_batch_gives_same_result_as_sorted_batch() {
    let moving = |t: f64| plot(t, 50_000.0 + 150.0 * t, 0.0);
    let plots: Vec<Plot> = (0..4).map(|k| moving(k as f64 * 4.0)).collect();
    let reversed: Vec<Plot> = plots.iter().cloned().rev().collect();

    let mut forward = Tracker::new(config());
    forward.process_plots(&plots);

    let mut backward = Tracker::new(config());
    backward.process_plots(&reversed);

    assert_eq!(
        forward.confirmed_count(),
        backward.confirmed_count(),
        "same number of confirmed tracks regardless of input order"
    );
    assert_eq!(forward.confirmed_count(), 1, "exactly one confirmed track");

    let vf = forward.tracks()[0].velocity();
    let vb = backward.tracks()[0].velocity();
    assert!(
        (vf[0] - vb[0]).abs() < 1.0 && (vf[1] - vb[1]).abs() < 1.0,
        "velocity estimate identical regardless of input order"
    );
}

/// A plot batch whose data time is strictly earlier than the tracker's
/// high-water mark must be dropped without touching the existing tracks.
/// The confirmed track keeps its identity, count and confirmed status.
///
/// REQ: FR-TRK-033
#[test]
fn late_plot_batch_is_dropped_gracefully() {
    let mut tracker = Tracker::new(config());

    // Confirm a stationary track over three regular updates.
    for k in 0..3 {
        let t = k as f64 * 4.0;
        tracker.process_plots(&[plot(t, 50_000.0, 0.0)]);
    }
    assert_eq!(
        tracker.confirmed_count(),
        1,
        "track confirmed before late batch"
    );
    let id_before = tracker.tracks()[0].id();
    let track_count_before = tracker.tracks().len();

    // Feed a plot at T = 2 — strictly before the current watermark of T = 8.
    // This must be silently dropped: no new track, no state change.
    tracker.process_plots(&[plot(2.0, 50_000.0, 0.0)]);

    assert_eq!(
        tracker.tracks().len(),
        track_count_before,
        "no new track spawned from late plot"
    );
    assert_eq!(
        tracker.tracks()[0].id(),
        id_before,
        "existing track identity preserved"
    );
    assert!(
        tracker.tracks()[0].is_confirmed(),
        "track stays confirmed after late batch"
    );
}

/// A scan whose data time does not exceed the previous scan's time must be
/// dropped without corrupting the existing track state.
///
/// REQ: FR-TRK-033
#[test]
fn backward_scan_is_dropped_gracefully() {
    let mut tracker = Tracker::new(config());

    // Confirm a stationary track over three regular scans.
    for k in 0..3 {
        let t = k as f64 * 4.0;
        tracker.process_scan(Timestamp(t), &[plot(t, 50_000.0, 0.0)]);
    }
    assert_eq!(
        tracker.confirmed_count(),
        1,
        "track confirmed before backward scan"
    );
    let id_before = tracker.tracks()[0].id();
    let track_count_before = tracker.tracks().len();

    // Feed a scan at T = 4 — earlier than the current watermark of T = 8.
    tracker.process_scan(Timestamp(4.0), &[plot(4.0, 50_000.0, 0.0)]);

    assert_eq!(
        tracker.tracks().len(),
        track_count_before,
        "no new track spawned from backward scan"
    );
    assert_eq!(
        tracker.tracks()[0].id(),
        id_before,
        "existing track identity preserved after backward scan"
    );
    assert!(
        tracker.tracks()[0].is_confirmed(),
        "track stays confirmed after backward scan"
    );
}
