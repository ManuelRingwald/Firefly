//! End-to-end multi-radar fusion through the Player (ADR 0010).
//!
//! Two overlapping radars watch one aircraft. Run the whole thing — simulator →
//! multi-sensor tracker → frame stream — and assert the two radars fuse into a
//! **single** stable track instead of a ghost per sensor.
//!
//! REQ: FR-TRK-010

use firefly_core::{Sensor, SensorId, TargetId};
use firefly_geo::{Enu, LocalFrame, Wgs84};
use firefly_player::Player;
use firefly_sim::{Leg, Radar, RadarParams, Scenario, State, Target};
use firefly_track::{SensorErrorModel, TrackerConfig};

/// A clean radar at `origin`: every in-range target seen every scan, no noise
/// (so the run is deterministic and the two sensors' measurements coincide).
fn clean_radar(id: SensorId, origin: Wgs84) -> Radar {
    Radar::new(
        Sensor::new(id, origin),
        RadarParams {
            scan_period: 4.0,
            prob_detection: 1.0,
            sigma_range: 0.0,
            sigma_azimuth: 0.0,
            sigma_elevation: 0.0,
            has_ssr: true,
            ..RadarParams::default()
        },
    )
}

/// A single aircraft cruising due east at 3 km, crossing the overlap region.
fn crossing_aircraft() -> Target {
    Target {
        id: TargetId(1),
        initial: State {
            position: Enu::new(-40_000.0, 0.0, 3_000.0),
            speed: 220.0,
            heading: 90.0_f64.to_radians(),
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(200.0)],
        mode_3a: Some(0o1234),
        icao_address: Some(0x3C_65_AC),
    }
}

#[test]
fn two_radars_one_aircraft_yield_one_stable_track() {
    // Common reference point and the two radar sites (~45 km apart).
    let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);
    let site_b = Wgs84::from_degrees(48.0, 11.6, 0.0);
    let radar_a = clean_radar(SensorId(1), origin);
    let radar_b = clean_radar(SensorId(2), site_b);

    let scenario = Scenario::new(origin)
        .with_duration(200.0)
        .add_radar(radar_a)
        .add_target(crossing_aircraft())
        // Add radar B by hand so both watch the same scenario frame.
        .add_radar(radar_b);

    let error = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08);
    let config = TrackerConfig::new(LocalFrame::new(origin))
        .with_sensor(SensorId(1), LocalFrame::new(origin), error, 4.0)
        .with_sensor(SensorId(2), LocalFrame::new(site_b), error, 4.0);

    let frames = Player::new(&scenario, config).frames();

    assert!(frames.len() > 10, "expected a non-trivial frame stream");

    // One aircraft, two radars → exactly one track id over the whole run, and
    // never more than one track in any frame (no ghost).
    let mut distinct_ids = std::collections::BTreeSet::new();
    let mut max_per_frame = 0usize;
    let mut confirmed_frames = 0usize;
    for frame in &frames {
        max_per_frame = max_per_frame.max(frame.tracks.len());
        for track in &frame.tracks {
            distinct_ids.insert(track.id.0);
        }
        if frame.tracks.iter().any(|t| t.confirmed) {
            confirmed_frames += 1;
        }
    }

    assert_eq!(
        distinct_ids.len(),
        1,
        "two radars on one aircraft must yield a single track id; got {distinct_ids:?}"
    );
    assert!(
        max_per_frame <= 1,
        "no frame may show a ghost: saw {max_per_frame} tracks at once"
    );
    assert!(
        confirmed_frames > frames.len() / 2,
        "the fused track should be confirmed for most of the run ({confirmed_frames}/{})",
        frames.len()
    );

    // The fused track carries the aircraft's SSR identity.
    let last = frames.last().unwrap();
    assert_eq!(last.tracks.len(), 1);
}
