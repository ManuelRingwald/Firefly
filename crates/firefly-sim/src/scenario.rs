//! Assembling a scenario: a reference frame, some radars, and some targets.

use firefly_geo::{LocalFrame, Wgs84};

use crate::radar::Radar;
use crate::target::Target;

/// A complete simulation scenario.
///
/// All target motion happens in one shared *scenario* ENU frame anchored at
/// [`Scenario::origin`]; each radar then re-projects truth into its own polar
/// frame. This keeps target scripting intuitive (flat-earth-ish local metres)
/// while the per-sensor geometry stays geodetically correct.
///
/// **Time:** [`Timestamp`] is "seconds since the start of the simulation"
/// (e.g., frame i runs at time 0, 4 seconds, 8 seconds, etc.). But each
/// `Timestamp` is interpreted relative to [`simulation_start_time_of_day`] —
/// e.g., if the simulation starts at 06:00 UTC (21600 seconds), then
/// `Timestamp(3600.0)` becomes 07:00:00 UTC in the wire output. This allows
/// the CAT062 encoder to emit correct Time-of-Day (ASTERIX I062/070) without
/// conversion, while the simulator itself stays deterministic (same input →
/// same output stream).
#[derive(Debug, Clone)]
pub struct Scenario {
    origin: Wgs84,
    frame: LocalFrame,
    radars: Vec<Radar>,
    targets: Vec<Target>,
    duration: f64,
    /// Internal integration step for ground truth, seconds.
    truth_step: f64,
    /// Master RNG seed.
    seed: u64,
    /// Seconds since UTC midnight at the start of the simulation.
    /// For example, 21600.0 = 06:00 UTC. Default: 0.0 (midnight).
    simulation_start_time_of_day: f64,
}

impl Scenario {
    /// Start building a scenario anchored at the given geodetic origin.
    /// By default, the simulation starts at UTC midnight (00:00:00).
    pub fn new(origin: Wgs84) -> Self {
        Self {
            origin,
            frame: LocalFrame::new(origin),
            radars: Vec::new(),
            targets: Vec::new(),
            duration: 60.0,
            truth_step: 0.1,
            seed: 0xF15E_F1A0_u64,
            simulation_start_time_of_day: 0.0,
        }
    }

    pub fn with_duration(mut self, seconds: f64) -> Self {
        self.duration = seconds;
        self
    }

    pub fn with_truth_step(mut self, seconds: f64) -> Self {
        self.truth_step = seconds;
        self
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    pub fn with_simulation_start_time_of_day(mut self, seconds: f64) -> Self {
        self.simulation_start_time_of_day = seconds;
        self
    }

    pub fn add_radar(mut self, radar: Radar) -> Self {
        self.radars.push(radar);
        self
    }

    pub fn add_target(mut self, target: Target) -> Self {
        self.targets.push(target);
        self
    }

    pub fn origin(&self) -> Wgs84 {
        self.origin
    }

    pub fn frame(&self) -> &LocalFrame {
        &self.frame
    }

    pub fn radars(&self) -> &[Radar] {
        &self.radars
    }

    pub fn targets(&self) -> &[Target] {
        &self.targets
    }

    pub fn duration(&self) -> f64 {
        self.duration
    }

    pub fn truth_step(&self) -> f64 {
        self.truth_step
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn simulation_start_time_of_day(&self) -> f64 {
        self.simulation_start_time_of_day
    }
}
