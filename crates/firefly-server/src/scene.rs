//! A small built-in demo scene, so the server runs with no setup (NFR-OPS-001).
//!
//! This is a *placeholder* picture good enough to see tracks move; the polished
//! demonstration scenario — including the deliberate "delay" trigger that shows
//! timing robustness — lands in Häppchen 3.5.

use firefly_core::{Sensor, SensorId, TargetId};
use firefly_geo::{Enu, LocalFrame, Wgs84};
use firefly_io::Frame;
use firefly_player::Player;
use firefly_sim::{Leg, Radar, RadarParams, Scenario, State, Target};
use firefly_track::{ProcessNoise, SensorErrorModel, TrackerConfig};

/// Build the demo frame stream: two cruising aircraft seen by one radar.
pub fn demo_frames() -> Vec<Frame> {
    let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);
    let radar = Radar::new(Sensor::new(SensorId(1), origin), RadarParams::default());

    let scenario = Scenario::new(origin)
        .with_duration(180.0)
        .add_radar(radar)
        .add_target(eastbound())
        .add_target(turning());

    // Two pieces of tuning keep each aircraft on a single, stable track id:
    //
    // 1. The error model must mirror the radar's *elevation* noise: at 10–11 km
    //    altitude the 1° elevation spread dominates the ground-range
    //    uncertainty, so leaving it out makes the gate far too tight.
    // 2. The process noise must match the expected manoeuvre. The turning target
    //    pulls ≈3.7 m/s² (a gentle 1°/s turn); a constant-velocity filter tuned
    //    for straight flight (the default) cannot follow it and the track
    //    fractures. We size `accel_psd` to that manoeuvre (`q ≳ a²·Δt ≈ 54`).
    //    Strong manoeuvres ultimately want IMM (M5); this suffices for the demo.
    let mut tracker = TrackerConfig::single_sensor(
        SensorId(1),
        LocalFrame::new(origin),
        SensorErrorModel::from_polar_deg(50.0, 0.08, 1.0),
    );
    tracker.process_noise = ProcessNoise::new(60.0);
    Player::new(&scenario, tracker).frames()
}

/// An aircraft entering from the south-west, cruising due east.
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

/// An aircraft heading north, then sweeping into a slow right turn.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo scene produces a non-trivial, time-ordered frame stream with
    /// confirmed tracks — enough to show something move.
    #[test]
    fn demo_scene_is_non_trivial() {
        let frames = demo_frames();
        assert!(frames.len() > 10, "expected a stream of frames");
        assert!(
            frames.iter().any(|f| f.tracks.iter().any(|t| t.confirmed)),
            "expected at least one confirmed track"
        );
        for w in frames.windows(2) {
            assert!(w[0].time.as_secs() <= w[1].time.as_secs());
        }
    }

    /// The shipped demo shows exactly the two aircraft, each on a single stable
    /// id — no churn, no spurious third track. This guards the demo's tracker
    /// tuning (elevation-aware error model + manoeuvre-matched process noise);
    /// regressing either makes the id count climb and extra tracks appear.
    /// REQ: FR-TRK-002, FR-TRK-006, FR-UI-001
    #[test]
    fn demo_scene_keeps_one_identity_per_aircraft() {
        let frames = demo_frames();

        let mut distinct_ids = std::collections::BTreeSet::new();
        let mut max_per_frame = 0usize;
        for frame in &frames {
            max_per_frame = max_per_frame.max(frame.tracks.len());
            for track in &frame.tracks {
                distinct_ids.insert(track.id.0);
            }
        }

        assert_eq!(
            distinct_ids.len(),
            2,
            "two aircraft must yield two track ids over the whole demo; got {distinct_ids:?}"
        );
        assert!(
            max_per_frame <= 2,
            "no frame should ever show more than the two real targets (saw {max_per_frame})"
        );
    }
}
