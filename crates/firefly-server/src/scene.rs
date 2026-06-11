//! A small built-in demo scene, so the server runs with no setup (NFR-OPS-001).
//!
//! This is a *placeholder* picture good enough to see tracks move; the polished
//! demonstration scenario — including the deliberate "delay" trigger that shows
//! timing robustness — lands in Häppchen 3.5.

use firefly_core::{Sensor, SensorId, SystemTrack, TargetId, Timestamp};
use firefly_geo::{Enu, LocalFrame, Wgs84};
use firefly_io::Frame;
use firefly_player::Player;
use firefly_sim::{Leg, Radar, RadarParams, Scenario, State, Target};
use firefly_track::{Gate, ProcessNoise, SensorErrorModel, TrackerConfig};

/// The geodetic origin of the demo scene — also the system reference point the
/// CAT062 multicast feed measures I062/100 from.
pub const DEMO_ORIGIN: (f64, f64) = (48.0, 11.0);

/// Build the demo [`Player`]: two aircraft (one cruising, one turning) seen by
/// one radar, with the tracker tuning that keeps each on a single stable id.
///
/// Both output adapters build on this same deterministic player — the web map
/// via [`demo_frames`] and the CAT062 multicast feed via [`demo_scans`] — so
/// they show byte-for-byte the same air picture (ADR 0006).
fn demo_player() -> Player {
    let origin = Wgs84::from_degrees(DEMO_ORIGIN.0, DEMO_ORIGIN.1, 0.0);
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
    Player::new(&scenario, tracker)
}

/// Build the demo frame stream (JSON adapter, for the web map).
pub fn demo_frames() -> Vec<Frame> {
    demo_player().frames()
}

/// Build the demo scan stream (raw `SystemTrack`s per scan, for the CAT062
/// multicast adapter). Same deterministic player as [`demo_frames`].
pub fn demo_scans() -> Vec<(Timestamp, Vec<SystemTrack>)> {
    demo_player().scans()
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

/// The geodetic origin of the Frankfurt showcase scene — the Frankfurt
/// Airport reference point, and the tracking frame's origin.
pub const FRANKFURT_ORIGIN: (f64, f64) = (50.0379, 8.5622);

/// Build the Frankfurt showcase [`Player`]: three radars with overlapping
/// coverage and eight aircraft — two arrivals, two departures, two
/// overflights (one SSR-equipped, one primary-only), a holding pattern and a
/// north-side arrival — a "busy day" picture for the M6 demonstration.
///
/// Two of the eight (the parallel west arrivals) fly only ~150 m apart, on
/// purpose: their tracker gates overlap, which is exactly the situation JPDA
/// (M5.5–M5.9) was built to keep apart without swapping identities.
fn frankfurt_player() -> Player {
    let origin = Wgs84::from_degrees(FRANKFURT_ORIGIN.0, FRANKFURT_ORIGIN.1, 0.0);
    let frame = LocalFrame::new(origin);

    // Three radar sites around Frankfurt, given as ENU offsets from the
    // airport reference point and converted to geodetic coordinates.
    let site_center = origin;
    let site_west = frame.enu_to_geodetic(&Enu::new(-55_000.0, 5_000.0, 0.0));
    let site_northeast = frame.enu_to_geodetic(&Enu::new(45_000.0, 35_000.0, 0.0));

    let radar_center = Radar::new(
        Sensor::new(SensorId(1), site_center),
        RadarParams {
            max_range: 120_000.0,
            ..RadarParams::default()
        },
    );
    let radar_west = Radar::new(
        Sensor::new(SensorId(2), site_west),
        RadarParams {
            max_range: 100_000.0,
            ..RadarParams::default()
        },
    );
    let radar_northeast = Radar::new(
        Sensor::new(SensorId(3), site_northeast),
        RadarParams {
            max_range: 100_000.0,
            ..RadarParams::default()
        },
    );

    let scenario = Scenario::new(origin)
        .with_duration(240.0)
        .add_radar(radar_center)
        .add_radar(radar_west)
        .add_radar(radar_northeast)
        .add_target(arrival_west_a())
        .add_target(arrival_west_b())
        .add_target(departure_straight())
        .add_target(departure_turning())
        .add_target(overflight_ssr())
        .add_target(overflight_primary())
        .add_target(holding_pattern())
        .add_target(arrival_north());

    let error = SensorErrorModel::from_polar_deg(50.0, 0.08, 1.0);
    let mut tracker = TrackerConfig::new(LocalFrame::new(origin))
        .with_sensor(SensorId(1), LocalFrame::new(site_center), error)
        .with_sensor(SensorId(2), LocalFrame::new(site_west), error)
        .with_sensor(SensorId(3), LocalFrame::new(site_northeast), error);
    tracker.process_noise = ProcessNoise::new(60.0);
    tracker.gate = Gate::from_probability(0.999);

    Player::new(&scenario, tracker)
}

/// Build the Frankfurt frame stream (JSON adapter, for the web map).
pub fn frankfurt_frames() -> Vec<Frame> {
    frankfurt_player().frames()
}

/// Build the Frankfurt scan stream (raw `SystemTrack`s per scan, for the
/// CAT062 multicast adapter). Same deterministic player as
/// [`frankfurt_frames`].
pub fn frankfurt_scans() -> Vec<(Timestamp, Vec<SystemTrack>)> {
    frankfurt_player().scans()
}

/// West arrival, aircraft A: descending into Frankfurt from the west.
///
/// Flies a parallel track ~150 m north of [`arrival_west_b`] at the same
/// speed and heading — the deliberate JPDA close-pair.
fn arrival_west_a() -> Target {
    Target {
        id: TargetId(10),
        initial: State {
            position: Enu::new(-95_000.0, -8_000.0, 3_000.0),
            speed: 130.0,
            heading: 75.0_f64.to_radians(),
            climb_rate: -3.0,
        },
        legs: vec![Leg::cruise(240.0).with_climb(-3.0)],
        mode_3a: Some(0o2001),
        icao_address: Some(0x3C_10_01),
    }
}

/// West arrival, aircraft B: ~150 m south of, and parallel to,
/// [`arrival_west_a`] — together they form the JPDA close-pair showcase.
fn arrival_west_b() -> Target {
    Target {
        id: TargetId(11),
        initial: State {
            position: Enu::new(-95_000.0, -8_150.0, 3_000.0),
            speed: 130.0,
            heading: 75.0_f64.to_radians(),
            climb_rate: -3.0,
        },
        legs: vec![Leg::cruise(240.0).with_climb(-3.0)],
        mode_3a: Some(0o2002),
        icao_address: Some(0x3C_10_02),
    }
}

/// A straight-out departure to the north, accelerating and climbing.
fn departure_straight() -> Target {
    Target {
        id: TargetId(12),
        initial: State {
            position: Enu::new(2_000.0, -1_000.0, 0.0),
            speed: 80.0,
            heading: 0.0,
            climb_rate: 10.0,
        },
        legs: vec![
            Leg::accelerate(60.0, 2.0).with_climb(10.0),
            Leg::cruise(180.0).with_climb(10.0),
        ],
        mode_3a: Some(0o2003),
        icao_address: Some(0x3C_10_03),
    }
}

/// A departure to the east that climbs out, then turns onto its outbound
/// heading at 2°/s — an IMM showcase (manoeuvre detection, M5.1–M5.4).
fn departure_turning() -> Target {
    Target {
        id: TargetId(13),
        initial: State {
            position: Enu::new(-2_000.0, 1_000.0, 0.0),
            speed: 80.0,
            heading: 90.0_f64.to_radians(),
            climb_rate: 8.0,
        },
        legs: vec![
            Leg::accelerate(60.0, 2.0).with_climb(8.0),
            Leg::turn(60.0, 2.0).with_climb(8.0),
            Leg::cruise(120.0).with_climb(8.0),
        ],
        mode_3a: Some(0o2004),
        icao_address: Some(0x3C_10_04),
    }
}

/// An SSR-equipped overflight transiting south-east through the coverage.
fn overflight_ssr() -> Target {
    Target {
        id: TargetId(14),
        initial: State {
            position: Enu::new(-100_000.0, 50_000.0, 10_000.0),
            speed: 230.0,
            heading: 135.0_f64.to_radians(),
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(240.0)],
        mode_3a: Some(0o2005),
        icao_address: Some(0x3C_10_05),
    }
}

/// A primary-only overflight (no transponder) transiting north-west — the
/// raw-plot transparency layer (M6.3) will show this aircraft as plots only,
/// since the tracker never gets an SSR identity for it.
fn overflight_primary() -> Target {
    Target {
        id: TargetId(15),
        initial: State {
            position: Enu::new(80_000.0, -70_000.0, 9_000.0),
            speed: 220.0,
            heading: 315.0_f64.to_radians(),
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(240.0)],
        mode_3a: None,
        icao_address: None,
    }
}

/// An aircraft flying a racetrack holding pattern north-east of the field —
/// two 180° turns at 3°/s, separated by straight legs.
fn holding_pattern() -> Target {
    Target {
        id: TargetId(16),
        initial: State {
            position: Enu::new(20_000.0, 60_000.0, 6_000.0),
            speed: 120.0,
            heading: 90.0_f64.to_radians(),
            climb_rate: 0.0,
        },
        legs: vec![
            Leg::cruise(30.0),
            Leg::turn(60.0, 3.0),
            Leg::cruise(30.0),
            Leg::turn(60.0, 3.0),
            Leg::cruise(60.0),
        ],
        mode_3a: Some(0o2007),
        icao_address: Some(0x3C_10_07),
    }
}

/// An arrival descending from the far north at 8 km, crossing from the
/// north-east radar's coverage into the centre radar's — a multi-radar
/// **handover** showcase. Because the aircraft is already tracked (and tightly
/// gated) by the north-east radar when it enters the centre radar's range, this
/// is exactly the late-entry-at-altitude case that used to spawn a duplicate
/// "ghost" track via the height-projection bias; with the height-aware frame
/// lift (FR-GEO-003) it stays a single track across the handover.
fn arrival_north() -> Target {
    Target {
        id: TargetId(17),
        initial: State {
            position: Enu::new(10_000.0, 115_000.0, 8_000.0),
            speed: 200.0,
            heading: 180.0_f64.to_radians(),
            climb_rate: -2.0,
        },
        legs: vec![Leg::cruise(240.0).with_climb(-2.0)],
        mode_3a: Some(0o2010),
        icao_address: Some(0x3C_10_08),
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

    /// The Frankfurt showcase produces a non-trivial, time-ordered frame
    /// stream with confirmed tracks across its full 240 s.
    #[test]
    fn frankfurt_scene_is_non_trivial() {
        let frames = frankfurt_frames();
        assert!(frames.len() > 30, "expected a long stream of frames");
        assert!(
            frames.iter().any(|f| f.tracks.iter().any(|t| t.confirmed)),
            "expected at least one confirmed track"
        );
        for w in frames.windows(2) {
            assert!(w[0].time.as_secs() <= w[1].time.as_secs());
        }
    }

    /// The Frankfurt showcase shows eight aircraft, each kept on a single
    /// stable track id over the run — including the close JPDA pair, which
    /// must not coalesce into one id or swap identities.
    /// REQ: FR-TRK-002, FR-TRK-006, FR-TRK-010, FR-TRK-018, FR-TRK-019, FR-UI-001
    #[test]
    fn frankfurt_scene_keeps_one_identity_per_aircraft() {
        let frames = frankfurt_frames();

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
            8,
            "eight aircraft must yield eight track ids over the whole run; got {distinct_ids:?}"
        );
        assert!(
            max_per_frame <= 8,
            "no frame should ever show more than the eight real targets (saw {max_per_frame})"
        );
    }
}
