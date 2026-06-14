//! End-to-end quality check: run a known scenario through the full `Tracker`
//! and score its output against the simulator's ground truth.
//!
//! This is the quantitative counterpart to the qualitative lifecycle tests: it
//! puts numbers on "how good is the tracker?" — position RMSE and track
//! continuity (coverage + id switches) — which is both a verification artifact
//! and the basis of a demo that shows the tracker's value.
//!
//! REQ: FR-TRK-007

use firefly_core::{Sensor, SensorId, TargetId};
use firefly_geo::{Enu, LocalFrame, Wgs84};
use firefly_sim::{Leg, Radar, RadarParams, Scenario, State, Target};
use firefly_track::{Rmse, SensorErrorModel, TrackContinuity, Tracker, TrackerConfig};

/// A single target cruising due north is tracked with a small position error
/// and perfect continuity: one unbroken track identity, high coverage.
/// REQ: FR-TRK-007
#[test]
fn single_target_quality_meets_thresholds() {
    // Anchor the scenario at the radar so the sensor's local ENU frame and the
    // scenario frame coincide — the tracker's east/north then equals truth.
    let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);

    let sigma_range: f64 = 50.0;
    let sigma_azimuth_deg: f64 = 0.08;
    let scan_period: f64 = 4.0;
    let radar = Radar::new(
        Sensor::new(SensorId(1), origin),
        RadarParams {
            scan_period,
            prob_detection: 1.0,
            sigma_range,
            sigma_azimuth: sigma_azimuth_deg.to_radians(),
            sigma_elevation: 0.0, // isolate 2-D behaviour from elevation noise
            has_ssr: false,
            ..RadarParams::default()
        },
    );

    // 20 km east, due north at 150 m/s for 240 s.
    let east0: f64 = 20_000.0;
    let speed: f64 = 150.0;
    let duration: f64 = 240.0;
    let target = Target {
        id: TargetId(1),
        initial: State {
            position: Enu::new(east0, 0.0, 3000.0),
            speed,
            heading: 0.0,
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(duration)],
        mode_3a: None,
        icao_address: None,
    };

    let scenario = Scenario::new(origin)
        .with_duration(duration)
        .with_seed(20260610)
        .add_radar(radar)
        .add_target(target);
    let plots = firefly_sim::run(&scenario);

    let mut tracker = Tracker::new(TrackerConfig::single_sensor(
        SensorId(1),
        LocalFrame::new(origin),
        SensorErrorModel::from_range_and_azimuth_deg(sigma_range, sigma_azimuth_deg),
    ));

    let mut rmse = Rmse::new();
    let mut continuity = TrackContinuity::new();

    // Walk the plots one at a time, each at its own azimuth-dependent data
    // time (ADR 0013, Häppchen 13.6) — the asynchronous per-plot path.
    for p in &plots {
        tracker.process_plot(p);

        // Truth at this instant: constant-velocity, due north from `east0`.
        let t = p.time.as_secs();
        let truth_e = east0;
        let truth_n = speed * t;

        // The confirmed track nearest to truth represents the target this scan.
        let nearest = tracker.confirmed_tracks().min_by(|a, b| {
            let da = (a.position()[0] - truth_e).powi(2) + (a.position()[1] - truth_n).powi(2);
            let db = (b.position()[0] - truth_e).powi(2) + (b.position()[1] - truth_n).powi(2);
            da.partial_cmp(&db).unwrap()
        });

        match nearest {
            Some(track) => {
                let p = track.position();
                let err = ((p[0] - truth_e).powi(2) + (p[1] - truth_n).powi(2)).sqrt();
                rmse.add(err);
                continuity.observe(Some(track.id()));
            }
            None => continuity.observe(None),
        }
    }

    let position_rmse = rmse.value().expect("the target should be tracked");

    // The filtered position error must be small — well below the raw cross-range
    // spread (range·σ_az ≈ 20 km · 0.0014 rad ≈ 28 m) combined with σ_range = 50 m.
    assert!(
        position_rmse < 40.0,
        "position RMSE {position_rmse:.1} m is too high"
    );

    // Continuity: one unbroken identity, and coverage near 1 (only the M-of-N
    // confirmation warmup at the start is uncovered).
    assert_eq!(
        continuity.id_switches(),
        0,
        "the single target should keep one track identity"
    );
    assert!(
        continuity.coverage() > 0.9,
        "coverage {:.2} should be near 1",
        continuity.coverage()
    );
}
