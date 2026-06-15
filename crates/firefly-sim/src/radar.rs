//! The sensor side of the simulation: turning true target positions into noisy,
//! sometimes-missing radar plots.

use firefly_core::{DetectionKind, ModeAC, Plot, Sensor, Timestamp};
use firefly_geo::{Enu, LocalFrame, Polar};

use crate::rng::Pcg32;
use crate::target::Target;

/// Error model and geometry limits of a simulated radar.
#[derive(Debug, Clone, Copy)]
pub struct RadarParams {
    /// Antenna revolution period, seconds (time between scans).
    pub scan_period: f64,
    /// Probability of detection for a target inside coverage, per scan.
    pub prob_detection: f64,
    /// Range measurement noise (1σ), metres.
    pub sigma_range: f64,
    /// Azimuth measurement noise (1σ), radians.
    pub sigma_azimuth: f64,
    /// Elevation measurement noise (1σ), radians. Large for a 2-D radar.
    pub sigma_elevation: f64,
    /// Maximum instrumented slant range, metres.
    pub max_range: f64,
    /// Minimum elevation seen (radar horizon / lowest beam), radians.
    pub min_elevation: f64,
    /// Whether this radar has a secondary (SSR) channel.
    pub has_ssr: bool,
}

impl Default for RadarParams {
    fn default() -> Self {
        // A plausible medium-range en-route surveillance radar.
        Self {
            scan_period: 4.0,
            prob_detection: 0.9,
            sigma_range: 50.0,
            sigma_azimuth: 0.08_f64.to_radians(),
            sigma_elevation: 1.0_f64.to_radians(),
            max_range: 200_000.0,
            min_elevation: 0.0,
            has_ssr: true,
        }
    }
}

/// A simulated radar: a [`Sensor`] paired with an error/geometry model.
#[derive(Debug, Clone, Copy)]
pub struct Radar {
    pub sensor: Sensor,
    pub params: RadarParams,
}

impl Radar {
    pub fn new(sensor: Sensor, params: RadarParams) -> Self {
        Self { sensor, params }
    }

    /// The noise-free polar position of a scenario-frame point as seen by this
    /// radar.
    pub fn true_polar(&self, scenario_frame: &LocalFrame, position: Enu) -> Polar {
        let geodetic = scenario_frame.enu_to_geodetic(&position);
        let local = self.sensor.frame().geodetic_to_enu(&geodetic);
        local.to_polar()
    }

    /// Attempt to detect a target over one antenna revolution starting at
    /// `scan_start`, returning a noisy plot if the target is in coverage and
    /// the detection roll succeeds.
    ///
    /// `position_at(t)` looks up the target's true scenario-frame position at
    /// scenario time `t` (or `None` once its script has ended).
    ///
    /// The plot's **data time is azimuth-dependent** (ADR 0013, Häppchen 13.6):
    /// the antenna sweeps through bearing over one revolution, so a target at
    /// true bearing θ (as seen at `scan_start`) is timestamped
    /// `scan_start + (θ / 2π) · scan_period`. The measurement itself is then
    /// re-derived from the target's position **at that data time**, not at
    /// `scan_start` — otherwise a plot's timestamp and its kinematic content
    /// would describe two different instants, up to one scan period apart for
    /// the slower radars. Every plot thus carries its own internally
    /// consistent time within the scan — the realistic asynchrony the
    /// per-plot tracker ([`firefly_track::Tracker::process_plots`]) consumes
    /// directly, and what time-separates two radars' views of one aircraft so
    /// they fuse instead of spawning a ghost.
    pub fn try_detect(
        &self,
        scenario_frame: &LocalFrame,
        position_at: impl Fn(f64) -> Option<Enu>,
        target: &Target,
        scan_start: f64,
        rng: &mut Pcg32,
    ) -> Option<Plot> {
        // The bearing at scan_start fixes where in the revolution this target
        // falls, and thus this plot's data time.
        let scan_start_position = position_at(scan_start)?;
        let scan_start_truth = self.true_polar(scenario_frame, scan_start_position);
        if scan_start_truth.range > self.params.max_range
            || scan_start_truth.elevation < self.params.min_elevation
        {
            return None;
        }
        let az = scan_start_truth.azimuth.rem_euclid(std::f64::consts::TAU);
        let plot_time = scan_start + (az / std::f64::consts::TAU) * self.params.scan_period;

        // Re-evaluate the truth at the plot's own data time: the target may
        // have moved on (or its script may have ended) between scan_start and
        // plot_time.
        let position = position_at(plot_time)?;
        let truth = self.true_polar(scenario_frame, position);
        if truth.range > self.params.max_range || truth.elevation < self.params.min_elevation {
            return None;
        }
        if !rng.bernoulli(self.params.prob_detection) {
            return None;
        }

        // Apply independent Gaussian measurement noise in the polar frame, which
        // is where a radar's errors actually live (range vs. angle, not x vs. y).
        let measurement = Polar {
            range: (truth.range + rng.next_normal(0.0, self.params.sigma_range)).max(0.0),
            azimuth: crate::target::wrap_angle(
                truth.azimuth + rng.next_normal(0.0, self.params.sigma_azimuth),
            ),
            elevation: truth.elevation + rng.next_normal(0.0, self.params.sigma_elevation),
        };

        // Decide what kind of plot this is and what SSR data rides along.
        let target_has_transponder = target.mode_3a.is_some() || target.icao_address.is_some();
        let (kind, mode_ac) = if self.params.has_ssr && target_has_transponder {
            // Geometric height of the target, used as a stand-in for Mode C
            // pressure altitude (no atmosphere model in M1).
            let geodetic = scenario_frame.enu_to_geodetic(&position);
            let flight_level_ft = geodetic.height / 0.3048;
            (
                DetectionKind::Combined,
                ModeAC {
                    mode_3a: target.mode_3a,
                    flight_level_ft: Some(flight_level_ft),
                    icao_address: target.icao_address,
                    callsign: target.callsign,
                },
            )
        } else {
            (DetectionKind::Primary, ModeAC::default())
        };

        Some(Plot {
            sensor: self.sensor.id,
            time: Timestamp(plot_time),
            measurement,
            kind,
            mode_ac,
        })
    }

    /// The scan **start** times of this radar within `[0, duration]`. All radars
    /// start at t = 0 (ADR 0013, Häppchen 13.6): asynchrony now comes from the
    /// per-plot azimuth timing (see [`try_detect`](Self::try_detect)) and the
    /// differing scan periods, not from a phase offset.
    pub fn scan_times(&self, duration: f64) -> Vec<f64> {
        let mut times = Vec::new();
        let mut t = 0.0;
        while t <= duration + 1e-9 {
            times.push(t);
            t += self.params.scan_period;
        }
        times
    }
}
