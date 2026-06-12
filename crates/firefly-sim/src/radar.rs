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
    /// Phase offset of the first scan, seconds. Lets several radars be
    /// deliberately un-synchronised.
    pub scan_offset: f64,
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
            scan_offset: 0.0,
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

    /// Attempt to detect a target at a true scenario-frame position, returning a
    /// noisy plot if the target is in coverage and the detection roll succeeds.
    pub fn try_detect(
        &self,
        scenario_frame: &LocalFrame,
        position: Enu,
        target: &Target,
        time: Timestamp,
        rng: &mut Pcg32,
    ) -> Option<Plot> {
        let truth = self.true_polar(scenario_frame, position);

        // Coverage gating: out of range or below the lowest beam → no detection.
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
                },
            )
        } else {
            (DetectionKind::Primary, ModeAC::default())
        };

        Some(Plot {
            sensor: self.sensor.id,
            time,
            measurement,
            kind,
            mode_ac,
        })
    }

    /// The scan times of this radar within `[0, duration]`.
    pub fn scan_times(&self, duration: f64) -> Vec<f64> {
        let mut times = Vec::new();
        let mut t = self.params.scan_offset;
        if t < 0.0 {
            t = 0.0;
        }
        while t <= duration + 1e-9 {
            if t >= 0.0 {
                times.push(t);
            }
            t += self.params.scan_period;
        }
        times
    }
}
