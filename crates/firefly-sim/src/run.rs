//! Running a scenario into a time-ordered stream of plots.

use firefly_core::Plot;
use firefly_geo::Enu;

use crate::rng::Pcg32;
use crate::scenario::Scenario;
use crate::target::{State, Target};

/// A precomputed ground-truth trajectory for one target, sampled at the
/// scenario's truth step. Positions in between are linearly interpolated.
struct Trajectory<'a> {
    target: &'a Target,
    step: f64,
    samples: Vec<Enu>,
    end_time: f64,
}

impl<'a> Trajectory<'a> {
    fn build(target: &'a Target, step: f64) -> Self {
        let mut samples = vec![target.initial.position];
        let mut state: State = target.initial;
        for leg in &target.legs {
            let mut remaining = leg.duration;
            while remaining > 1e-9 {
                let dt = remaining.min(step);
                state = Target::step(&state, leg, dt);
                samples.push(state.position);
                remaining -= dt;
            }
        }
        let end_time = target.scripted_duration();
        Self {
            target,
            step,
            samples,
            end_time,
        }
    }

    /// True position at scenario time `t`, or `None` once the script has ended.
    fn position_at(&self, t: f64) -> Option<Enu> {
        if t < 0.0 || t > self.end_time + 1e-9 {
            return None;
        }
        if self.samples.len() == 1 {
            return Some(self.samples[0]);
        }
        let idx_f = t / self.step;
        let i = (idx_f.floor() as usize).min(self.samples.len() - 1);
        let j = (i + 1).min(self.samples.len() - 1);
        let frac = (idx_f - i as f64).clamp(0.0, 1.0);
        let a = self.samples[i];
        let b = self.samples[j];
        Some(Enu {
            east: a.east + (b.east - a.east) * frac,
            north: a.north + (b.north - a.north) * frac,
            up: a.up + (b.up - a.up) * frac,
        })
    }
}

/// Run a scenario, returning every plot every radar produced, sorted by time
/// (ties broken by sensor id then by target order) — exactly the kind of stream
/// a live tracker consumes.
pub fn run(scenario: &Scenario) -> Vec<Plot> {
    let trajectories: Vec<Trajectory> = scenario
        .targets()
        .iter()
        .map(|t| Trajectory::build(t, scenario.truth_step()))
        .collect();

    let mut plots: Vec<(f64, u16, Plot)> = Vec::new();

    for (radar_idx, radar) in scenario.radars().iter().enumerate() {
        // Give each radar its own RNG stream so adding or removing one radar
        // does not perturb another's noise sequence.
        let mut rng = Pcg32::new(scenario.seed(), radar_idx as u64);

        for scan_start in radar.scan_times(scenario.duration()) {
            for traj in &trajectories {
                if let Some(plot) = radar.try_detect(
                    scenario.frame(),
                    |t| traj.position_at(t),
                    traj.target,
                    scan_start,
                    &mut rng,
                ) {
                    // Each plot carries its own azimuth-dependent data time.
                    let plot_time = plot.time.as_secs();
                    plots.push((plot_time, radar.sensor.id.0, plot));
                }
            }
        }
    }

    plots.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
    plots.into_iter().map(|(_, _, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radar::{Radar, RadarParams};
    use crate::target::{Leg, State};
    use firefly_core::{Callsign, DetectionKind, Sensor, SensorId, TargetId};
    use firefly_geo::Wgs84;

    fn straight_north_target() -> Target {
        Target {
            id: TargetId(1),
            initial: State {
                position: Enu::new(0.0, 0.0, 3000.0),
                speed: 200.0,
                heading: 0.0,
                climb_rate: 0.0,
            },
            legs: vec![Leg::cruise(100.0)],
            mode_3a: Some(0o1234),
            icao_address: Some(0x3C_65_AC),
            callsign: Some(Callsign::new("DLH123")),
        }
    }

    fn perfect_radar() -> Radar {
        let sensor = Sensor::new(SensorId(1), Wgs84::from_degrees(48.0, 11.0, 0.0));
        Radar::new(
            sensor,
            RadarParams {
                scan_period: 5.0,
                prob_detection: 1.0,
                sigma_range: 0.0,
                sigma_azimuth: 0.0,
                sigma_elevation: 0.0,
                ..RadarParams::default()
            },
        )
    }

    #[test]
    fn perfect_radar_detects_every_scan() {
        let scenario = Scenario::new(Wgs84::from_degrees(48.0, 11.0, 0.0))
            .with_duration(100.0)
            .add_radar(perfect_radar())
            .add_target(straight_north_target());
        let plots = run(&scenario);
        // 100 s / 5 s scan = 21 scans (t = 0,5,...,100), target alive the whole time.
        assert_eq!(plots.len(), 21);
    }

    #[test]
    fn plots_are_time_ordered() {
        let scenario = Scenario::new(Wgs84::from_degrees(48.0, 11.0, 0.0))
            .with_duration(100.0)
            .add_radar(perfect_radar())
            .add_target(straight_north_target());
        let plots = run(&scenario);
        for w in plots.windows(2) {
            assert!(w[0].time.as_secs() <= w[1].time.as_secs());
        }
    }

    #[test]
    fn equipped_target_yields_combined_plots_with_ssr() {
        let scenario = Scenario::new(Wgs84::from_degrees(48.0, 11.0, 0.0))
            .with_duration(20.0)
            .add_radar(perfect_radar())
            .add_target(straight_north_target());
        let plots = run(&scenario);
        assert!(!plots.is_empty());
        for p in &plots {
            assert_eq!(p.kind, DetectionKind::Combined);
            assert_eq!(p.mode_ac.mode_3a, Some(0o1234));
            assert!(p.mode_ac.flight_level_ft.unwrap() > 9000.0); // 3000 m ≈ 9842 ft
        }
    }

    #[test]
    fn detection_probability_is_respected() {
        let mut radar = perfect_radar();
        radar.params.prob_detection = 0.5;
        radar.params.scan_period = 0.5;
        // 600 s at 200 m/s = 120 km of travel, comfortably inside the 200 km
        // instrumented range, so every scan is a fair Bernoulli trial.
        let scenario = Scenario::new(Wgs84::from_degrees(48.0, 11.0, 0.0))
            .with_duration(600.0)
            .add_radar(radar)
            .add_target(Target {
                legs: vec![Leg::cruise(600.0)],
                ..straight_north_target()
            });
        let plots = run(&scenario);
        let scans = 600.0 / 0.5 + 1.0;
        let rate = plots.len() as f64 / scans;
        assert!((rate - 0.5).abs() < 0.05, "detection rate was {rate}");
    }

    #[test]
    fn target_disappears_after_script_ends() {
        let scenario = Scenario::new(Wgs84::from_degrees(48.0, 11.0, 0.0))
            .with_duration(200.0)
            .add_radar(perfect_radar())
            .add_target(straight_north_target()); // 100 s script
        let plots = run(&scenario);
        // No plot should appear after the 100 s scripted lifetime.
        assert!(plots.iter().all(|p| p.time.as_secs() <= 100.0 + 1e-6));
    }
}
