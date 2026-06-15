//! End-to-end check: feed the simulator's noisy plots through the converted
//! measurement (2.1) and the Kalman filter (2.2), and verify the filter really
//! smooths — its position error beats the raw measurement error, and it
//! recovers the target's velocity, which the radar never measures directly.
//!
//! REQ: FR-TRK-003

use firefly_core::{Sensor, SensorId, TargetId};
use firefly_geo::{Enu, Wgs84};
use firefly_sim::{Leg, Radar, RadarParams, Scenario, State, Target};
use firefly_track::{convert_plot, LinearKalman, ProcessNoise, SensorErrorModel};

#[test]
fn filter_smooths_and_recovers_velocity() {
    // Anchor the scenario at the radar site so the sensor's local ENU frame and
    // the scenario frame coincide — the tracker's east/north then equals truth.
    let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);

    let sigma_range: f64 = 50.0;
    let sigma_azimuth_deg: f64 = 0.08;
    let radar = Radar::new(
        Sensor::new(SensorId(1), origin),
        RadarParams {
            scan_period: 4.0,
            prob_detection: 1.0,
            sigma_range,
            sigma_azimuth: sigma_azimuth_deg.to_radians(),
            sigma_elevation: 0.0, // isolate the 2-D CV behaviour from elevation noise
            has_ssr: false,
            ..RadarParams::default()
        },
    );

    // A target 20 km east, cruising due north at 150 m/s for 240 s.
    let east0 = 20_000.0;
    let speed = 150.0;
    let target = Target {
        id: TargetId(1),
        initial: State {
            position: Enu::new(east0, 0.0, 3000.0),
            speed,
            heading: 0.0,
            climb_rate: 0.0,
        },
        legs: vec![Leg::cruise(240.0)],
        mode_3a: None,
        icao_address: None,
        callsign: None,
    };

    let scenario = Scenario::new(origin)
        .with_duration(240.0)
        .with_seed(20260606)
        .add_radar(radar)
        .add_target(target);
    let plots = firefly_sim::run(&scenario);
    assert!(
        plots.len() > 30,
        "expected a decent run, got {}",
        plots.len()
    );

    let model = SensorErrorModel::from_range_and_azimuth_deg(sigma_range, sigma_azimuth_deg);
    let process = ProcessNoise::new(0.5);

    let mut kf: Option<LinearKalman> = None;
    let mut last_t = 0.0;

    let mut raw_sq_sum = 0.0;
    let mut filt_sq_sum = 0.0;
    let mut count = 0u32;

    for plot in &plots {
        let t = plot.time.as_secs();
        let meas = convert_plot(&plot.measurement, &model);

        match kf.as_mut() {
            None => {
                kf = Some(LinearKalman::from_first_measurement(&meas, 200.0));
            }
            Some(filter) => {
                filter.predict(t - last_t, &process);
                filter.update(&meas);

                // Truth at this instant (constant-velocity, due north).
                let n_truth = speed * t;
                let raw_err =
                    ((meas.east() - east0).powi(2) + (meas.north() - n_truth).powi(2)).sqrt();
                let filt_err = ((filter.position()[0] - east0).powi(2)
                    + (filter.position()[1] - n_truth).powi(2))
                .sqrt();
                raw_sq_sum += raw_err * raw_err;
                filt_sq_sum += filt_err * filt_err;
                count += 1;
            }
        }
        last_t = t;
    }

    let raw_rmse = (raw_sq_sum / count as f64).sqrt();
    let filt_rmse = (filt_sq_sum / count as f64).sqrt();

    // The filter must beat the raw measurements — that is the whole point.
    assert!(
        filt_rmse < raw_rmse,
        "filtered RMSE {filt_rmse:.1} m should beat raw RMSE {raw_rmse:.1} m"
    );

    // Velocity (never measured directly) should converge near truth (0, 150).
    let filter = kf.unwrap();
    let v = filter.velocity();
    assert!(v[0].abs() < 15.0, "v_east should be ~0, was {:.1}", v[0]);
    assert!(
        (v[1] - speed).abs() < 15.0,
        "v_north should be ~150, was {:.1}",
        v[1]
    );
}
