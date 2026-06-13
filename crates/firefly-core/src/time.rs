use serde::{Deserialize, Serialize};

/// A point in time, expressed as seconds since the start of the simulation.
///
/// Within the simulation, this is a relative offset (frame i at t=0, i+1 at t=Δt, etc.).
/// But the wire output (CAT062 I062/070, ASTERIX Time-of-Day) interprets it relative
/// to the scenario's `simulation_start_time_of_day` — e.g., if the scenario starts at
/// 06:00 UTC (21600 seconds), `Timestamp(3600.0)` becomes 07:00:00 UTC on the wire.
/// This preserves simulator determinism (same input → same output stream) while
/// emitting correct Time-of-Day without conversion. We keep it as `f64` because
/// sub-scan timing (fractions of a second) matters for prediction, and the dynamic
/// range is tiny compared to `f64` precision.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Timestamp(pub f64);

impl Timestamp {
    pub const ZERO: Timestamp = Timestamp(0.0);

    /// Seconds as a raw float.
    pub fn as_secs(self) -> f64 {
        self.0
    }

    /// Elapsed seconds from `earlier` to `self`. Negative if `self` precedes it.
    pub fn since(self, earlier: Timestamp) -> f64 {
        self.0 - earlier.0
    }

    /// A new timestamp advanced by `dt` seconds.
    pub fn advanced_by(self, dt: f64) -> Timestamp {
        Timestamp(self.0 + dt)
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "t={:.3}s", self.0)
    }
}
