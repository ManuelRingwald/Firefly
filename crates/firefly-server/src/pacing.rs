//! Mapping data-time to wall-clock — the *only* place this happens.
//!
//! The tracker and the [`Player`](firefly_player::Player) are pure and
//! **data-time driven** (ADR 0003); they never look at a clock. To make the air
//! picture watchable, the server replays the frame stream at (a multiple of)
//! real time: before each frame it waits a wall-clock delay proportional to the
//! data-time gap to the previous frame, divided by the playback speed.
//!
//! Crucially, slowing down, speeding up or pausing *delivery* here changes
//! nothing about the tracks — those decisions already happened, deterministically,
//! upstream. That separation is what lets the demo (Häppchen 3.5) visibly delay
//! the stream without corrupting a single track (NFR-CLOUD-004).

use std::time::Duration;

/// Wall-clock delay to wait *before* sending a frame at data-time `current`,
/// given the previous frame's data-time and the playback `speed` (data-seconds
/// per wall-second, assumed strictly positive — see
/// [`ServerConfig`](crate::config::ServerConfig)).
///
/// The first frame (`prev == None`) goes out immediately. A non-positive gap
/// (duplicate or out-of-order data-time) yields no delay rather than a negative
/// one.
pub fn delay_before(prev: Option<f64>, current: f64, speed: f64) -> Duration {
    match prev {
        None => Duration::ZERO,
        Some(prev) => {
            let gap = (current - prev).max(0.0);
            Duration::from_secs_f64(gap / speed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first frame is sent without waiting.
    #[test]
    fn first_frame_has_no_delay() {
        assert_eq!(delay_before(None, 12.0, 1.0), Duration::ZERO);
    }

    /// At speed 1, the delay equals the data-time gap.
    #[test]
    fn real_time_matches_the_data_gap() {
        let d = delay_before(Some(8.0), 12.0, 1.0);
        assert!((d.as_secs_f64() - 4.0).abs() < 1e-9);
    }

    /// Speed scales the delay: 2× as fast → half the wait; 0.5× → double.
    #[test]
    fn speed_scales_the_delay() {
        assert!((delay_before(Some(8.0), 12.0, 2.0).as_secs_f64() - 2.0).abs() < 1e-9);
        assert!((delay_before(Some(8.0), 12.0, 0.5).as_secs_f64() - 8.0).abs() < 1e-9);
    }

    /// A non-positive gap never produces a negative delay.
    #[test]
    fn out_of_order_gap_is_clamped_to_zero() {
        assert_eq!(delay_before(Some(12.0), 8.0, 1.0), Duration::ZERO);
        assert_eq!(delay_before(Some(12.0), 12.0, 1.0), Duration::ZERO);
    }
}
