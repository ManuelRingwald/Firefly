//! The **Interacting Multiple Model** (IMM) filter — running several motion
//! hypotheses at once and letting the data decide which one fits.
//!
//! A single Kalman filter commits to *one* motion model (e.g. constant
//! velocity). That is fine on a long straight leg but lags in a turn; crank the
//! process noise up to follow turns and it jitters on the straights. The IMM
//! resolves the dilemma by keeping a **bank** of filters — here a
//! constant-velocity one and one or more [`coordinated-turn`](MotionModel)
//! ones — each with its own state and a **model probability** `μ` saying how
//! well it currently explains the measurements. The reported estimate is the
//! probability-weighted blend, so the tracker rides the straights on the CV
//! model and the turns on the CT model, switching smoothly as `μ` shifts.
//!
//! One IMM cycle has four stages (Blom & Bar-Shalom):
//!
//! 1. **Interaction / mixing** *(this Häppchen, M5.2)* — before each model
//!    filters on its own, it starts not from its *own* last estimate but from a
//!    **blend** of all models' estimates, weighted by how likely a target was
//!    to switch *into* this model. This is what couples the filters: a model
//!    that was unlikely last scan still inherits a sensible state if the target
//!    just switched to it.
//! 2. **Model-conditioned filtering** *(M5.3)* — each model runs predict+update
//!    from its mixed start, yielding a new estimate and a **likelihood**.
//! 3. **Model-probability update** *(M5.3)* — the likelihoods re-weight `μ`.
//! 4. **Combination** *(M5.3)* — blend the per-model estimates into the output.
//!
//! The switching itself is modelled as a **Markov chain**: `transition[i][j]`
//! is the probability that a target in model `i` is in model `j` the next scan.
//!
//! Determinism (ADR 0003): every stage is a pure function of the bank state and
//! the inputs — no wall clock, no hidden state.

use nalgebra::{Matrix4, Vector4};

use crate::kalman::LinearKalman;
use crate::motion::MotionModel;

/// Below this probability a model is treated as effectively dead and its
/// mixing column is left untouched, avoiding a division by a vanishing
/// normaliser. (A model probability is only ever this small if every path into
/// it has near-zero probability, in which case its mixed state is irrelevant.)
const MIN_MODEL_PROBABILITY: f64 = 1e-12;

/// A bank of Kalman filters under different motion models, with the Markov
/// switching model that couples them — the state an IMM carries between scans.
///
/// Invariants (checked by [`Imm::new`]): the four vectors share the same length
/// `r` (the number of models), `probabilities` sums to 1, and each row of
/// `transition` sums to 1 (it is *row-stochastic*).
#[derive(Debug, Clone, PartialEq)]
pub struct Imm {
    /// The motion model each filter in the bank assumes.
    models: Vec<MotionModel>,
    /// Per-model Kalman state `(x_i, P_i)`.
    filters: Vec<LinearKalman>,
    /// Model probabilities `μ_i`, summing to 1.
    probabilities: Vec<f64>,
    /// Row-stochastic Markov transition matrix: `transition[i][j]` is the
    /// probability of switching from model `i` to model `j`.
    transition: Vec<Vec<f64>>,
}

impl Imm {
    /// Assemble an IMM bank. `models`, `filters` and `probabilities` must share
    /// the same length `r`; `transition` must be `r × r`.
    ///
    /// # Panics
    /// If the lengths disagree, `probabilities` does not sum to ~1, or any
    /// `transition` row does not sum to ~1 — these are programming errors in the
    /// bank's construction, not runtime data conditions.
    pub fn new(
        models: Vec<MotionModel>,
        filters: Vec<LinearKalman>,
        probabilities: Vec<f64>,
        transition: Vec<Vec<f64>>,
    ) -> Self {
        let r = models.len();
        assert!(r > 0, "an IMM needs at least one model");
        assert_eq!(filters.len(), r, "one filter per model");
        assert_eq!(probabilities.len(), r, "one probability per model");
        assert_eq!(transition.len(), r, "transition matrix must be r×r");
        assert!(
            (probabilities.iter().sum::<f64>() - 1.0).abs() < 1e-9,
            "model probabilities must sum to 1"
        );
        for row in &transition {
            assert_eq!(row.len(), r, "transition matrix must be r×r");
            assert!(
                (row.iter().sum::<f64>() - 1.0).abs() < 1e-9,
                "each transition row must sum to 1 (row-stochastic)"
            );
        }
        Self {
            models,
            filters,
            probabilities,
            transition,
        }
    }

    /// The number of models in the bank.
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Whether the bank is empty (never true after [`Imm::new`], which requires
    /// at least one model — present so Clippy is happy alongside [`len`]).
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// The current model probabilities `μ_i`.
    pub fn probabilities(&self) -> &[f64] {
        &self.probabilities
    }

    /// The per-model filter states.
    pub fn filters(&self) -> &[LinearKalman] {
        &self.filters
    }

    /// The motion model assumed by each filter.
    pub fn models(&self) -> &[MotionModel] {
        &self.models
    }

    /// **Predicted model probabilities** `c_j = Σ_i π_ij · μ_i`: how likely the
    /// target is to be in each model *next* scan, before seeing the next
    /// measurement. Also the normaliser for the mixing weights below.
    pub fn predicted_model_probabilities(&self) -> Vec<f64> {
        let r = self.len();
        (0..r)
            .map(|j| {
                (0..r)
                    .map(|i| self.transition[i][j] * self.probabilities[i])
                    .sum()
            })
            .collect()
    }

    /// **Mixing probabilities** `μ_{i|j} = π_ij · μ_i / c_j`, returned indexed
    /// `[j][i]`: given that the target is in model `j` next scan, how much does
    /// model `i`'s current estimate contribute to model `j`'s starting point?
    /// Each row `j` sums to 1 (it is a proper weighting over the source models).
    pub fn mixing_probabilities(&self) -> Vec<Vec<f64>> {
        let r = self.len();
        let c = self.predicted_model_probabilities();
        (0..r)
            .map(|j| {
                (0..r)
                    .map(|i| {
                        if c[j] < MIN_MODEL_PROBABILITY {
                            // Dead target model: weighting is irrelevant; keep
                            // it a proper distribution by deferring to the prior.
                            self.probabilities[i]
                        } else {
                            self.transition[i][j] * self.probabilities[i] / c[j]
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// **Mixed initial conditions** `(x_0j, P_0j)` for each model `j` — the
    /// blended state each model's filter should start the next cycle from.
    ///
    /// For target model `j` with mixing weights `μ_{i|j}`:
    /// - `x_0j = Σ_i μ_{i|j} · x_i`
    /// - `P_0j = Σ_i μ_{i|j} · [P_i + (x_i − x_0j)(x_i − x_0j)ᵀ]`
    ///
    /// The covariance carries an extra **spread-of-the-means** term: when the
    /// models disagree on the state, the mixed start is more uncertain, exactly
    /// as it should be. This is the step that couples the otherwise independent
    /// filters (M5.2).
    pub fn mixed_initial_conditions(&self) -> Vec<LinearKalman> {
        let r = self.len();
        let weights = self.mixing_probabilities();
        (0..r)
            .map(|j| {
                let w = &weights[j];

                // Mixed mean.
                let mut x0 = Vector4::zeros();
                for (&wi, f) in w.iter().zip(&self.filters) {
                    x0 += wi * f.x;
                }

                // Mixed covariance with the spread-of-the-means correction.
                let mut p0 = Matrix4::zeros();
                for (&wi, f) in w.iter().zip(&self.filters) {
                    let d = f.x - x0;
                    p0 += wi * (f.p + d * d.transpose());
                }

                LinearKalman { x: x0, p: p0 }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Matrix4;

    /// Two distinct filter states for a two-model bank.
    fn filter(east: f64, north: f64, var: f64) -> LinearKalman {
        LinearKalman {
            x: Vector4::new(east, north, 0.0, 0.0),
            p: Matrix4::identity() * var,
        }
    }

    fn two_model_bank(transition: Vec<Vec<f64>>, probabilities: Vec<f64>) -> Imm {
        Imm::new(
            vec![
                MotionModel::ConstantVelocity,
                MotionModel::CoordinatedTurn { rate: 0.05 },
            ],
            vec![filter(0.0, 0.0, 100.0), filter(100.0, 0.0, 100.0)],
            probabilities,
            transition,
        )
    }

    /// Predicted model probabilities sum to 1 and follow `c = Πᵀ μ`.
    /// REQ: FR-TRK-012
    #[test]
    fn predicted_model_probabilities_follow_the_markov_chain() {
        let imm = two_model_bank(vec![vec![0.9, 0.1], vec![0.2, 0.8]], vec![0.6, 0.4]);
        let c = imm.predicted_model_probabilities();
        // c0 = 0.9·0.6 + 0.2·0.4 = 0.62 ; c1 = 0.1·0.6 + 0.8·0.4 = 0.38.
        assert!((c[0] - 0.62).abs() < 1e-12);
        assert!((c[1] - 0.38).abs() < 1e-12);
        assert!((c.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }

    /// Each row of the mixing weights is a proper distribution (sums to 1).
    /// REQ: FR-TRK-012
    #[test]
    fn mixing_weights_are_proper_distributions() {
        let imm = two_model_bank(vec![vec![0.9, 0.1], vec![0.2, 0.8]], vec![0.6, 0.4]);
        for row in imm.mixing_probabilities() {
            assert!((row.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        }
    }

    /// An identity transition matrix means no switching: each model mixes only
    /// with itself, so the mixed initial conditions equal the originals.
    /// REQ: FR-TRK-012
    #[test]
    fn identity_transition_leaves_states_unmixed() {
        let imm = two_model_bank(vec![vec![1.0, 0.0], vec![0.0, 1.0]], vec![0.5, 0.5]);
        let mixed = imm.mixed_initial_conditions();
        for (m, f) in mixed.iter().zip(imm.filters()) {
            assert!((m.x - f.x).norm() < 1e-12);
            assert!((m.p - f.p).norm() < 1e-12);
        }
    }

    /// The mixed mean is the weighted average of the model means, and mixing
    /// two states that disagree inflates the covariance (spread-of-means term).
    /// REQ: FR-TRK-012
    #[test]
    fn mixing_blends_means_and_inflates_covariance() {
        // Fully mixing transition; equal priors → both models mix with weights
        // (0.5, 0.5), so each mixed mean is the midpoint (50, 0, …).
        let imm = two_model_bank(vec![vec![0.5, 0.5], vec![0.5, 0.5]], vec![0.5, 0.5]);
        let mixed = imm.mixed_initial_conditions();
        for m in &mixed {
            assert!((m.x[0] - 50.0).abs() < 1e-12, "mean east is the midpoint");
            assert!(m.x[1].abs() < 1e-12);
            // Each model's own P had east-variance 100; the means differ by 100
            // in east, so the spread term adds 0.5·50² + 0.5·50² = 2500 → 2600.
            assert!(
                (m.p[(0, 0)] - 2600.0).abs() < 1e-9,
                "covariance grows by the spread of the means, got {}",
                m.p[(0, 0)]
            );
        }
    }

    /// When both models hold the *same* estimate, mixing is a no-op regardless
    /// of the switching probabilities.
    /// REQ: FR-TRK-012
    #[test]
    fn mixing_identical_states_is_a_noop() {
        let imm = Imm::new(
            vec![
                MotionModel::ConstantVelocity,
                MotionModel::CoordinatedTurn { rate: 0.05 },
            ],
            vec![filter(10.0, -5.0, 200.0), filter(10.0, -5.0, 200.0)],
            vec![0.3, 0.7],
            vec![vec![0.6, 0.4], vec![0.1, 0.9]],
        );
        let mixed = imm.mixed_initial_conditions();
        for m in &mixed {
            assert!((m.x[0] - 10.0).abs() < 1e-12);
            assert!((m.x[1] + 5.0).abs() < 1e-12);
            // No disagreement → no spread term → covariance unchanged.
            assert!((m.p[(0, 0)] - 200.0).abs() < 1e-9);
        }
    }
}
