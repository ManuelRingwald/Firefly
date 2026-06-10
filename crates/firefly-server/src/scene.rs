//! A small built-in demo scene, so the server runs with no setup (NFR-OPS-001).
//!
//! This is a *placeholder* picture good enough to see tracks move; the polished
//! demonstration scenario — including the deliberate "delay" trigger that shows
//! timing robustness — lands in Häppchen 3.5.

use firefly_core::{Sensor, SensorId, TargetId};
use firefly_geo::{Enu, Wgs84};
use firefly_io::Frame;
use firefly_player::Player;
use firefly_sim::{Leg, Radar, RadarParams, Scenario, State, Target};
use firefly_track::{SensorErrorModel, TrackerConfig};

/// Build the demo frame stream: two cruising aircraft seen by one radar.
pub fn demo_frames() -> Vec<Frame> {
    let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);
    let radar = Radar::new(Sensor::new(SensorId(1), origin), RadarParams::default());

    let scenario = Scenario::new(origin)
        .with_duration(180.0)
        .add_radar(radar)
        .add_target(eastbound())
        .add_target(turning());

    let tracker = TrackerConfig::new(SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08));
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
}
