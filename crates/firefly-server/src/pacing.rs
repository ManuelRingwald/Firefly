//! Mapping data-time to wall-clock — the *only* place this happens.
//!
//! The tracker and the [`Player`](firefly_player::Player) are pure and
//! **data-time driven** (ADR 0003); they never look at a clock. To make the air
//! picture watchable, the server replays the frame stream at (a multiple of)
//! real time.
//!
//! Pacing is **absolute**: every frame's due time is pinned to the start of the
//! replay, not to its predecessor. Normal playback is identical to a
//! gap-by-gap scheme, but the behaviour after a hiccup differs and that
//! difference is the point. When delivery falls behind — because the client
//! asked for the "delay" demo (Häppchen 3.5), which sleeps a few seconds at the
//! delivery edge — the frames that piled up are already past due and go out
//! back-to-back until the schedule is caught up again. The picture therefore
//! **catches up to the present** instead of running permanently late: the
//! aircraft does not hang in the air for the duration of the delay, it jumps
//! forward to where it really is.
//!
//! Crucially, none of this touches the tracks: slowing, pausing or catching up
//! *delivery* changes nothing about the deterministic stream computed upstream
//! (NFR-CLOUD-004).

use std::time::{Duration, Instant};

/// Wall-clock instant by which the frame at data-time `current` is due, paced
/// from `origin` (when the replay started) and the stream's first data-time
/// `first`, at `speed` data-seconds per wall-second (assumed strictly positive
/// — see [`ServerConfig`](crate::config::ServerConfig)).
///
/// Because the due time is measured from the fixed `origin`, a frame whose due
/// time already lies in the past (delivery fell behind) is "due now" and the
/// caller sends it without waiting — that is the catch-up. A non-positive
/// data-time gap from `first` (duplicate or out-of-order) is clamped so the due
/// time never precedes `origin`.
pub fn due_at(origin: Instant, first: f64, current: f64, speed: f64) -> Instant {
    let elapsed = (current - first).max(0.0) / speed;
    origin + Duration::from_secs_f64(elapsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first frame (`current == first`) is due at the origin: no wait.
    #[test]
    fn first_frame_is_due_at_the_origin() {
        let origin = Instant::now();
        assert_eq!(due_at(origin, 12.0, 12.0, 1.0), origin);
    }

    /// At speed 1, the schedule offset equals the data-time elapsed since start.
    #[test]
    fn real_time_matches_the_data_elapsed_from_start() {
        let origin = Instant::now();
        let due = due_at(origin, 8.0, 12.0, 1.0);
        assert!((due.duration_since(origin).as_secs_f64() - 4.0).abs() < 1e-9);
    }

    /// Speed scales the schedule: 2× as fast → half the offset; 0.5× → double.
    #[test]
    fn speed_scales_the_schedule() {
        let origin = Instant::now();
        let fast = due_at(origin, 8.0, 12.0, 2.0)
            .duration_since(origin)
            .as_secs_f64();
        let slow = due_at(origin, 8.0, 12.0, 0.5)
            .duration_since(origin)
            .as_secs_f64();
        assert!((fast - 2.0).abs() < 1e-9);
        assert!((slow - 8.0).abs() < 1e-9);
    }

    /// Out-of-order data-time never schedules a frame before the origin.
    #[test]
    fn out_of_order_data_time_is_clamped_to_the_origin() {
        let origin = Instant::now();
        assert_eq!(due_at(origin, 12.0, 8.0, 1.0), origin);
    }
}
