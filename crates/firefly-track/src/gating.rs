//! Gating: a cheap plausibility test before data association.
//!
//! When many plots and several tracks coexist (plus clutter), the tracker must
//! decide which plot belongs to which track. Before the full assignment
//! (Häppchen 2.4), gating rules out the obviously implausible: for each track,
//! only plots inside a **validation region** around its prediction are
//! considered.
//!
//! The region is *not* a circle. It must account for uncertainty — the track's
//! and the measurement's — so it has the same tilted "cigar" shape as the
//! innovation covariance `S` (from the filter). We measure a plot's distance
//! from the prediction with the **squared Mahalanobis distance** `d² = yᵀ S⁻¹ y`
//! (see [`crate::LinearKalman::mahalanobis_squared`]) and accept it when
//! `d² ≤ γ`.
//!
//! Choosing `γ`: if the models are correct, `d²` follows a chi-squared (χ²)
//! distribution with as many degrees of freedom as the measurement has
//! dimensions — here **2** (east/north). For 2 degrees of freedom the χ²
//! quantile has a closed form, `γ = −2·ln(1 − P_G)`, where `P_G` is the gate
//! probability (the chance a *true* plot lands inside the gate). No statistics
//! library required.

use crate::kalman::LinearKalman;
use crate::measurement::CartesianMeasurement;

/// A validation gate: accept a plot whose squared Mahalanobis distance from a
/// track's prediction does not exceed `threshold` (the χ² value `γ`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gate {
    /// The χ² threshold `γ`.
    pub threshold: f64,
}

impl Gate {
    /// A gate with an explicit χ² threshold.
    pub fn new(threshold: f64) -> Self {
        Self { threshold }
    }

    /// A gate sized by its gate probability `P_G` (e.g. 0.99), using the
    /// closed-form χ² quantile for **2 degrees of freedom**: `γ = −2·ln(1−P_G)`.
    ///
    /// This is exact for our 2-D (east/north) position measurement; a different
    /// measurement dimension would need the general χ² quantile.
    ///
    /// REQ: FR-TRK-004
    pub fn from_probability(p_gate: f64) -> Self {
        assert!(
            p_gate > 0.0 && p_gate < 1.0,
            "gate probability must be in (0, 1), got {p_gate}"
        );
        Self {
            threshold: -2.0 * (1.0 - p_gate).ln(),
        }
    }

    /// Whether a given squared Mahalanobis distance falls inside the gate.
    ///
    /// REQ: FR-TRK-004
    pub fn accepts(&self, mahalanobis_squared: f64) -> bool {
        mahalanobis_squared <= self.threshold
    }

    /// Whether a measurement is plausible for the given filter's prediction.
    ///
    /// REQ: FR-TRK-004
    pub fn accepts_measurement(&self, kf: &LinearKalman, m: &CartesianMeasurement) -> bool {
        self.accepts(kf.mahalanobis_squared(m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{Matrix2, Matrix4, Vector2, Vector4};

    fn filter_with_position_variance(var_east: f64, var_north: f64) -> LinearKalman {
        LinearKalman {
            x: Vector4::new(0.0, 0.0, 0.0, 0.0),
            p: Matrix4::from_diagonal(&Vector4::new(var_east, var_north, 1.0e6, 1.0e6)),
        }
    }

    fn measurement(east: f64, north: f64, var: f64) -> CartesianMeasurement {
        CartesianMeasurement {
            z: Vector2::new(east, north),
            r: Matrix2::new(var, 0.0, 0.0, var),
        }
    }

    /// The closed-form χ² thresholds for 2 DOF match the textbook values.
    /// REQ: FR-TRK-004
    #[test]
    fn threshold_matches_chi_squared_2dof() {
        assert!((Gate::from_probability(0.95).threshold - 5.991).abs() < 1e-2);
        assert!((Gate::from_probability(0.99).threshold - 9.210).abs() < 1e-2);
        assert!((Gate::from_probability(0.999).threshold - 13.816).abs() < 1e-2);
    }

    /// A plot exactly on the prediction has distance zero and is always accepted.
    /// REQ: FR-TRK-004
    #[test]
    fn plot_on_prediction_is_accepted() {
        let kf = filter_with_position_variance(2500.0, 2500.0);
        let gate = Gate::from_probability(0.99);
        assert!(gate.accepts_measurement(&kf, &measurement(0.0, 0.0, 2500.0)));
    }

    /// The decisive concept: the same Euclidean offset is accepted along the
    /// uncertain axis but rejected across the certain one (Mahalanobis ≠
    /// Euclidean). East is uncertain (σ=100 m), north is certain (σ=10 m).
    /// REQ: FR-TRK-004
    #[test]
    fn gate_is_anisotropic() {
        let kf = filter_with_position_variance(10_000.0, 100.0);
        let gate = Gate::from_probability(0.99);
        // Tiny measurement noise so S is dominated by the track uncertainty.
        let along_uncertain = measurement(50.0, 0.0, 1.0); // d² ≈ 2500/10001 ≈ 0.25
        let across_certain = measurement(0.0, 50.0, 1.0); // d² ≈ 2500/101 ≈ 24.75
        assert!(gate.accepts_measurement(&kf, &along_uncertain));
        assert!(!gate.accepts_measurement(&kf, &across_certain));
    }

    /// An offset of n·σ along an axis yields a squared Mahalanobis distance of n².
    /// REQ: FR-TRK-004
    #[test]
    fn mahalanobis_scales_in_sigma() {
        // σ = 10 m ⇒ variance 100; zero measurement noise.
        let kf = filter_with_position_variance(100.0, 100.0);
        let d2 = kf.mahalanobis_squared(&measurement(30.0, 0.0, 0.0)); // 3σ east
        assert!((d2 - 9.0).abs() < 1e-9);
    }
}
