//! Probabilistic Data Association (PDA): soft assignment under clutter.
//!
//! GNN ([`crate::association`]) picks a single "best" plot per track — a hard
//! 0/1 decision. In dense traffic, several plots can plausibly belong to the
//! same track (overlapping gates), and a wrong hard pick can throw the filter
//! off badly. **PDA** instead keeps *all* gated plots and weighs each one by
//! how likely it is to be the true return, versus the possibility that none
//! of them is (clutter, or a missed detection).
//!
//! For a track with `m` plots inside its gate, PDA computes `m + 1`
//! **association probabilities** `β`:
//!
//! - `β_0`: probability that *none* of the gated plots came from this target
//!   (a missed detection, or all gated plots are clutter).
//! - `β_j` (`j = 1..=m`): probability that plot `j` is the true return.
//!
//! These sum to 1 and are derived from each plot's likelihood under the
//! track's prediction (how well it matches, [`LinearKalman::measurement_likelihood`])
//! versus a **clutter density** `λ` — the expected number of false plots per
//! unit area. A plot that fits the prediction well, in an area with little
//! clutter, earns a high `β_j`; in dense clutter, `β_0` grows because "this is
//! just noise" becomes more plausible.
//!
//! REQ: FR-TRK-015

use serde::{Deserialize, Serialize};

use crate::gating::Gate;
use crate::kalman::LinearKalman;
use crate::measurement::CartesianMeasurement;

/// The clutter environment a track lives in: how often false plots occur, and
/// how reliably the sensor detects a real target.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClutterModel {
    /// Spatial density of false plots `λ`, in returns per square metre. Small
    /// numbers: a busy radar might produce a handful of false plots spread
    /// over tens of square kilometres.
    pub density: f64,
    /// Probability `P_D` that the sensor detects the true target at all,
    /// given that it is in range (independent of clutter).
    pub detection_probability: f64,
}

impl ClutterModel {
    pub fn new(density: f64, detection_probability: f64) -> Self {
        assert!(density >= 0.0, "clutter density must not be negative");
        assert!(
            detection_probability > 0.0 && detection_probability <= 1.0,
            "detection probability must be in (0, 1], got {detection_probability}"
        );
        Self {
            density,
            detection_probability,
        }
    }
}

/// Compute the PDA association probabilities for one track and the plots that
/// fell inside its gate this scan.
///
/// Returns a vector of length `gated.len() + 1`: index 0 is `β_0` (no
/// detection), indices `1..` are `β_j` for `gated[j - 1]`. The result always
/// sums to 1.
///
/// `gate` supplies the gate probability `P_G` (recovered from its χ²
/// threshold via `P_G = 1 − e^{−γ/2}`, the inverse of
/// [`Gate::from_probability`] for 2 degrees of freedom) — the chance that a
/// *true* plot lands inside the gate at all. Plots outside the gate are
/// missed entirely, which is why a smaller `P_G` makes "no detection" (`β_0`)
/// relatively more likely even for a perfectly matching plot.
///
/// REQ: FR-TRK-015
pub fn association_probabilities(
    track: &LinearKalman,
    gated: &[CartesianMeasurement],
    gate: &Gate,
    clutter: &ClutterModel,
) -> Vec<f64> {
    let p_gate = 1.0 - (-gate.threshold / 2.0).exp();
    let b = clutter.density * (1.0 - clutter.detection_probability * p_gate)
        / clutter.detection_probability;

    let likelihoods: Vec<f64> = gated
        .iter()
        .map(|m| track.measurement_likelihood(m))
        .collect();
    let sum: f64 = likelihoods.iter().sum();
    let denom = b + sum;

    if denom <= 0.0 {
        // No clutter modelled and nothing in the gate: certainly no detection.
        let mut betas = vec![0.0; gated.len() + 1];
        betas[0] = 1.0;
        return betas;
    }

    let mut betas = Vec::with_capacity(gated.len() + 1);
    betas.push(b / denom);
    betas.extend(likelihoods.iter().map(|&l| l / denom));
    betas
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{Matrix2, Matrix4, Vector2, Vector4};

    fn track_at(east: f64, north: f64) -> LinearKalman {
        LinearKalman {
            x: Vector4::new(east, north, 0.0, 0.0),
            p: Matrix4::from_diagonal(&Vector4::new(2500.0, 2500.0, 1.0e6, 1.0e6)),
        }
    }

    fn meas_at(east: f64, north: f64) -> CartesianMeasurement {
        CartesianMeasurement {
            z: Vector2::new(east, north),
            r: Matrix2::new(2500.0, 0.0, 0.0, 2500.0),
        }
    }

    /// With nothing in the gate, "no detection" is certain.
    /// REQ: FR-TRK-015
    #[test]
    fn empty_gate_means_no_detection() {
        let track = track_at(0.0, 0.0);
        let gate = Gate::from_probability(0.99);
        let clutter = ClutterModel::new(0.0, 0.95);

        let betas = association_probabilities(&track, &[], &gate, &clutter);
        assert_eq!(betas, vec![1.0]);
    }

    /// The betas always sum to one, whatever the inputs.
    /// REQ: FR-TRK-015
    #[test]
    fn betas_sum_to_one() {
        let track = track_at(0.0, 0.0);
        let gate = Gate::from_probability(0.99);
        let clutter = ClutterModel::new(1.0e-6, 0.9);
        let gated = [meas_at(5.0, 0.0), meas_at(0.0, 5.0), meas_at(-5.0, -5.0)];

        let betas = association_probabilities(&track, &gated, &gate, &clutter);
        assert_eq!(betas.len(), gated.len() + 1);
        let total: f64 = betas.iter().sum();
        assert!((total - 1.0).abs() < 1e-12, "betas sum to {total}");
    }

    /// A single plot landing exactly on the prediction, with negligible
    /// clutter, dominates: β_1 close to 1, β_0 close to 0.
    /// REQ: FR-TRK-015
    #[test]
    fn perfect_plot_with_little_clutter_dominates() {
        let track = track_at(0.0, 0.0);
        let gate = Gate::from_probability(0.99);
        let clutter = ClutterModel::new(1.0e-9, 0.9);

        let betas = association_probabilities(&track, &[meas_at(0.0, 0.0)], &gate, &clutter);
        assert!(betas[1] > 0.99, "beta_1 = {}", betas[1]);
        assert!(betas[0] < 0.01, "beta_0 = {}", betas[0]);
    }

    /// More clutter makes "no detection" relatively more likely, even for the
    /// same plot.
    /// REQ: FR-TRK-015
    #[test]
    fn more_clutter_increases_no_detection_probability() {
        let track = track_at(0.0, 0.0);
        let gate = Gate::from_probability(0.99);
        let plot = [meas_at(0.0, 0.0)];

        let low_clutter =
            association_probabilities(&track, &plot, &gate, &ClutterModel::new(1.0e-9, 0.9));
        let high_clutter =
            association_probabilities(&track, &plot, &gate, &ClutterModel::new(1.0e-4, 0.9));

        assert!(high_clutter[0] > low_clutter[0]);
    }

    /// Of two candidate plots, the one closer to the prediction earns the
    /// larger association probability.
    /// REQ: FR-TRK-015
    #[test]
    fn closer_plot_gets_more_weight() {
        let track = track_at(0.0, 0.0);
        let gate = Gate::from_probability(0.99);
        let clutter = ClutterModel::new(1.0e-6, 0.9);
        // m1 close to the prediction, m2 further away but still gated.
        let gated = [meas_at(5.0, 0.0), meas_at(80.0, 0.0)];

        let betas = association_probabilities(&track, &gated, &gate, &clutter);
        assert!(betas[1] > betas[2], "betas = {betas:?}");
    }
}
