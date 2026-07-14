//! Per-track horizontal acceleration estimation (VERT.3).
//!
//! The IMM bank estimates position and velocity on a 4-D state; the
//! **acceleration** for I062/210 and the groundspeed trend for I062/200 come
//! from this small dedicated estimator instead of a 6-D constant-acceleration
//! IMM model: extending the bank to 6-D would cut through the entire fusion
//! core (the linear filter, gating, JPDA and registration are all built on
//! the 4-D state) for a quantity that a smoothed derivative of the already-
//! filtered velocity serves with far less risk. The CA-model bank extension
//! stays an explicitly deferred follow-up (see the VERT.3 milestone notes).
//!
//! The estimator differentiates consecutive **combined-estimate** velocity
//! samples (data-time deltas) and smooths the result with an EWMA — the
//! velocity is already Kalman-filtered, so a light smoothing suffices to keep
//! the sign of the along-track component stable for the LONG trend.
//! Deterministic (data-time driven, ADR 0003) and serialisable
//! (snapshot/restore).
//!
//! REQ: FR-TRK-043

/// EWMA gain for the differentiated acceleration. The input velocity is
/// already filtered; this mainly suppresses the derivative's amplification of
/// residual estimate jitter.
const ACCEL_EWMA_ALPHA: f64 = 0.3;
/// Velocity samples closer together than this (seconds) are skipped: the
/// difference quotient over a near-zero baseline amplifies noise without
/// adding information (several sensors can hit a track within milliseconds).
const MIN_SAMPLE_SPACING_S: f64 = 0.5;

/// The per-track horizontal acceleration estimator.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AccelerationEstimator {
    /// Data time of the last accepted velocity sample.
    last_time: f64,
    /// The last accepted velocity sample, m/s (east, north).
    last_velocity: (f64, f64),
    /// The smoothed acceleration estimate, m/s² (east, north); `None` until
    /// two samples with usable spacing have been seen.
    smoothed: Option<(f64, f64)>,
}

impl AccelerationEstimator {
    /// Start from the first velocity sample (no acceleration claim yet).
    pub fn from_first_sample(v_east: f64, v_north: f64, time: f64) -> Self {
        Self {
            last_time: time,
            last_velocity: (v_east, v_north),
            smoothed: None,
        }
    }

    /// Fold in the next velocity sample at data time `time`. Samples too
    /// close to the previous one are skipped (see [`MIN_SAMPLE_SPACING_S`]);
    /// non-forward times are ignored (out-of-order input is filtered
    /// upstream by the tracker's watermark).
    pub fn update(&mut self, v_east: f64, v_north: f64, time: f64) {
        let dt = time - self.last_time;
        if dt < MIN_SAMPLE_SPACING_S {
            return;
        }
        let raw = (
            (v_east - self.last_velocity.0) / dt,
            (v_north - self.last_velocity.1) / dt,
        );
        self.smoothed = Some(match self.smoothed {
            Some((ax, ay)) => (
                ax + ACCEL_EWMA_ALPHA * (raw.0 - ax),
                ay + ACCEL_EWMA_ALPHA * (raw.1 - ay),
            ),
            None => raw,
        });
        self.last_time = time;
        self.last_velocity = (v_east, v_north);
    }

    /// The smoothed acceleration `(a_east, a_north)` in m/s², or `None`
    /// before the second usable sample.
    pub fn acceleration_mps2(&self) -> Option<(f64, f64)> {
        self.smoothed
    }

    /// Data time of the last accepted sample — the freshness basis (a stale
    /// acceleration is withheld like every other derived quantity).
    pub fn last_time(&self) -> f64 {
        self.last_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constant velocity yields (near-)zero acceleration; a steady speed-up
    /// converges to the true along-track acceleration. REQ: FR-TRK-043
    #[test]
    fn constant_velocity_zero_and_speedup_converges() {
        let mut level = AccelerationEstimator::from_first_sample(200.0, 0.0, 0.0);
        for i in 1..=10 {
            level.update(200.0, 0.0, i as f64 * 4.0);
        }
        let (ax, ay) = level.acceleration_mps2().expect("estimate");
        assert!(ax.abs() < 1e-9 && ay.abs() < 1e-9);

        // 0.5 m/s² eastward speed-up sampled every 4 s.
        let mut accel = AccelerationEstimator::from_first_sample(200.0, 0.0, 0.0);
        for i in 1..=15 {
            let t = i as f64 * 4.0;
            accel.update(200.0 + 0.5 * t, 0.0, t);
        }
        let (ax, _) = accel.acceleration_mps2().unwrap();
        assert!((ax - 0.5).abs() < 0.05, "converged to 0.5 m/s², got {ax}");
    }

    /// Near-simultaneous samples (multi-sensor hits) are skipped instead of
    /// amplifying jitter through a near-zero baseline. REQ: FR-TRK-043
    #[test]
    fn near_simultaneous_samples_are_skipped() {
        let mut est = AccelerationEstimator::from_first_sample(200.0, 0.0, 0.0);
        est.update(200.0, 0.0, 4.0);
        let before = est.acceleration_mps2();
        // A second sensor's estimate 10 ms later with 2 m/s jitter would
        // read as 200 m/s² through the raw quotient — it must be skipped.
        est.update(202.0, 0.0, 4.01);
        assert_eq!(est.acceleration_mps2(), before);
        assert_eq!(est.last_time(), 4.0, "skipped sample leaves state alone");
    }
}
