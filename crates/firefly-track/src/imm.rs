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

use nalgebra::{Matrix6, Vector6};
use serde::{Deserialize, Serialize};

use crate::kalman::{LinearKalman, ProcessNoise};
use crate::kalman6::{JerkNoise, LinearKalman6};
use crate::measurement::CartesianMeasurement;
use crate::motion::MotionModel;

/// Below this probability a model is treated as effectively dead and its
/// mixing column is left untouched, avoiding a division by a vanishing
/// normaliser. (A model probability is only ever this small if every path into
/// it has near-zero probability, in which case its mixed state is irrelevant.)
const MIN_MODEL_PROBABILITY: f64 = 1e-12;

/// Prior standard deviation (m/s²) for the acceleration state when a bank is
/// seeded from a 4-D estimate (ADR 0035): generous enough to cover the civil
/// longitudinal *and* centripetal envelope, so the CA model is not starved at
/// birth.
const ACCEL_PRIOR_STD: f64 = 5.0;

/// Tiny PSD floor (m²/s⁵) on the acceleration diagonal of the CV/CT process
/// noise — pure numerical hygiene: CV's zero acceleration rows would
/// otherwise collapse that covariance block to exactly 0.
const ACCEL_Q_FLOOR_PSD: f64 = 1e-6;

/// Default jerk PSD (m²/s⁵) for the CA model. Sized for civil transitions of
/// ~1 m/s² acceleration change over ~10 s (`q ≈ Δa²/Δt`): brisk enough to
/// catch the onset of a take-off roll or approach deceleration, calm enough
/// not to chase measurement noise.
const DEFAULT_JERK_PSD: f64 = 0.1;

fn default_jerk_psd() -> f64 {
    DEFAULT_JERK_PSD
}

/// A bank of Kalman filters under different motion models, with the Markov
/// switching model that couples them — the state an IMM carries between scans.
///
/// Invariants (checked by [`Imm::new`]): the four vectors share the same length
/// `r` (the number of models), `probabilities` sums to 1, and each row of
/// `transition` sums to 1 (it is *row-stochastic*).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Imm {
    /// The motion model each filter in the bank assumes.
    models: Vec<MotionModel>,
    /// Per-model Kalman state `(x_i, P_i)` on the 6-D state (ADR 0035).
    filters: Vec<LinearKalman6>,
    /// Model probabilities `μ_i`, summing to 1.
    probabilities: Vec<f64>,
    /// Row-stochastic Markov transition matrix: `transition[i][j]` is the
    /// probability of switching from model `i` to model `j`.
    transition: Vec<Vec<f64>>,
    /// Jerk PSD (m²/s⁵) driving the CA model's process noise (ADR 0035).
    #[serde(default = "default_jerk_psd")]
    jerk_psd: f64,
}

/// The recipe for building a track's IMM bank: which motion models, how they
/// switch (the Markov matrix), and the prior model probabilities a newborn
/// track starts with. One of these lives in the [`TrackerConfig`](crate::TrackerConfig)
/// and is stamped onto every new track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImmConfig {
    /// The motion models in the bank.
    pub models: Vec<MotionModel>,
    /// Row-stochastic Markov transition matrix (`r × r`).
    pub transition: Vec<Vec<f64>>,
    /// Prior model probabilities for a freshly born track (sum to 1).
    pub initial_probabilities: Vec<f64>,
    /// Jerk PSD (m²/s⁵) for the CA model's process noise (ADR 0035);
    /// irrelevant for banks without a CA model.
    #[serde(default = "default_jerk_psd")]
    pub jerk_psd: f64,
}

impl ImmConfig {
    /// The default civil-aviation bank: constant velocity plus a symmetric pair
    /// of coordinated turns at `±turn_rate` (rad/s) — covering straight flight,
    /// a left turn and a right turn. The transition matrix is **sticky** (each
    /// model persists with probability 0.9, with the remaining 0.1 split evenly
    /// across the others), and a newborn track starts mostly in CV (aircraft
    /// cruise straight far more than they turn).
    ///
    /// A typical civil "rate-one" turn is 3°/s ≈ 0.052 rad/s.
    pub fn cv_and_turns(turn_rate: f64) -> Self {
        let models = vec![
            MotionModel::ConstantVelocity,
            MotionModel::CoordinatedTurn { rate: turn_rate },
            MotionModel::CoordinatedTurn { rate: -turn_rate },
        ];
        let transition = vec![
            vec![0.95, 0.025, 0.025],
            vec![0.05, 0.90, 0.05],
            vec![0.05, 0.05, 0.90],
        ];
        let initial_probabilities = vec![0.9, 0.05, 0.05];
        Self {
            models,
            transition,
            initial_probabilities,
            jerk_psd: DEFAULT_JERK_PSD,
        }
    }

    /// The VERT.4b civil bank: CV, a symmetric pair of coordinated turns and
    /// a **constant-acceleration** model (ADR 0035) — covering straight
    /// flight, both turns and the longitudinal-acceleration phases (take-off
    /// roll, climb acceleration, approach deceleration) that CV+CT can only
    /// absorb as process noise, lagging.
    ///
    /// Tuning: CV is the stickiest model (aircraft cruise most of the time)
    /// and the manoeuvre hypotheses are entered sparingly — a fourth model
    /// otherwise taxes the straight-flight accuracy (the classic IMM trade),
    /// measurably in the single-target RMSE scenario test.
    pub fn cv_turns_and_ca(turn_rate: f64) -> Self {
        let models = vec![
            MotionModel::ConstantVelocity,
            MotionModel::CoordinatedTurn { rate: turn_rate },
            MotionModel::CoordinatedTurn { rate: -turn_rate },
            MotionModel::ConstantAcceleration,
        ];
        let transition = vec![
            vec![0.94, 0.02, 0.02, 0.02],
            vec![0.03, 0.91, 0.03, 0.03],
            vec![0.03, 0.03, 0.91, 0.03],
            vec![0.05, 0.02, 0.02, 0.91],
        ];
        let initial_probabilities = vec![0.88, 0.045, 0.045, 0.03];
        Self {
            models,
            transition,
            initial_probabilities,
            jerk_psd: DEFAULT_JERK_PSD,
        }
    }

    /// Build the IMM bank for a track whose every model starts from the same
    /// freshly initialised filter `seed` (e.g. from
    /// [`LinearKalman::from_first_measurement`]).
    pub fn seed(&self, seed: LinearKalman) -> Imm {
        let n = self.models.len();
        let mut imm = Imm::new(
            self.models.clone(),
            vec![seed; n],
            self.initial_probabilities.clone(),
            self.transition.clone(),
        );
        imm.jerk_psd = self.jerk_psd;
        imm
    }
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
            // ADR 0035 (Weg A): the bank runs on the 6-D state internally;
            // 4-D seeds are embedded with a zero-mean acceleration prior.
            filters: filters
                .iter()
                .map(|f| LinearKalman6::from_kalman4(f, ACCEL_PRIOR_STD))
                .collect(),
            probabilities,
            transition,
            jerk_psd: DEFAULT_JERK_PSD,
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

    /// The per-model filter states (6-D since VERT.4b, ADR 0035).
    pub fn filters(&self) -> &[LinearKalman6] {
        &self.filters
    }

    /// The per-model process noise on the 6-D state: CV/CT keep the CWNA
    /// manoeuvre budget on the (p, v) block (plus a tiny floor on the
    /// acceleration diagonal for conditioning); CA runs on white-noise jerk.
    fn model_noise(&self, model: &MotionModel, dt: f64, process: &ProcessNoise) -> Matrix6<f64> {
        match model {
            MotionModel::ConstantAcceleration => JerkNoise::new(self.jerk_psd).covariance(dt),
            _ => {
                let q4 = process.covariance(dt);
                let mut q = Matrix6::zeros();
                for i in 0..4 {
                    for j in 0..4 {
                        q[(i, j)] = q4[(i, j)];
                    }
                }
                q[(4, 4)] = ACCEL_Q_FLOOR_PSD * dt;
                q[(5, 5)] = ACCEL_Q_FLOOR_PSD * dt;
                q
            }
        }
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
    pub fn mixed_initial_conditions(&self) -> Vec<LinearKalman6> {
        let r = self.len();
        let weights = self.mixing_probabilities();
        (0..r)
            .map(|j| {
                let w = &weights[j];

                // Mixed mean.
                let mut x0 = Vector6::zeros();
                for (&wi, f) in w.iter().zip(&self.filters) {
                    x0 += wi * f.x;
                }

                // Mixed covariance with the spread-of-the-means correction.
                let mut p0 = Matrix6::zeros();
                for (&wi, f) in w.iter().zip(&self.filters) {
                    let d = f.x - x0;
                    p0 += wi * (f.p + d * d.transpose());
                }

                LinearKalman6 { x: x0, p: p0 }
            })
            .collect()
    }

    /// The **combined estimate** `(x, P)` the IMM reports to the outside world:
    /// the probability-weighted blend of the per-model filters,
    /// - `x = Σ_j μ_j · x_j`
    /// - `P = Σ_j μ_j · [P_j + (x_j − x)(x_j − x)ᵀ]`
    ///
    /// As with mixing, the covariance carries a spread-of-the-means term, so a
    /// bank that disagrees about the state reports an honestly larger
    /// uncertainty. This is what the tracker hands on as the track's position
    /// and velocity (Häppchen M5.4).
    pub fn combined_estimate(&self) -> LinearKalman {
        // ADR 0035 (Weg A): combine on 6-D, hand downstream the exact
        // Gaussian marginal over (position, velocity) — the fusion core's
        // 4-D contract is untouched.
        self.combined_estimate6().to_kalman4()
    }

    /// The combined estimate on the full 6-D state (VERT.4b) — same
    /// probability-weighted blend with the spread-of-the-means term.
    pub fn combined_estimate6(&self) -> LinearKalman6 {
        let mut x = Vector6::zeros();
        for (&mu, f) in self.probabilities.iter().zip(&self.filters) {
            x += mu * f.x;
        }
        let mut p = Matrix6::zeros();
        for (&mu, f) in self.probabilities.iter().zip(&self.filters) {
            let d = f.x - x;
            p += mu * (f.p + d * d.transpose());
        }
        LinearKalman6 { x, p }
    }

    /// The bank's combined acceleration `(a_east, a_north)` in m/s² — the
    /// filter *state* that feeds I062/210 since VERT.4b: centripetal in a
    /// turn (from CT), longitudinal while accelerating (from CA), zero in
    /// cruise (from CV).
    pub fn combined_acceleration(&self) -> (f64, f64) {
        self.combined_estimate6().acceleration()
    }

    /// **Predict** stage of the IMM cycle (mixing + per-model prediction).
    ///
    /// Mixes the bank (stages 1) and rolls each model forward by `dt` under its
    /// own motion model (stage 2, prediction only). The model probabilities
    /// become the Markov-predicted `c_j` — already correct if this scan turns
    /// out to be a coast; if a measurement follows, [`update`](Self::update)
    /// re-weights from here. Returns the predicted combined estimate, which the
    /// tracker uses for gating and association.
    ///
    /// Splitting predict from update mirrors [`LinearKalman`] and lets the
    /// tracker predict every track, *then* associate, *then* update — its
    /// existing per-scan flow (Häppchen M5.4).
    ///
    /// REQ: FR-TRK-013
    pub fn predict(&mut self, dt: f64, process: &ProcessNoise) -> LinearKalman {
        debug_assert!(
            dt >= 0.0,
            "backward prediction dt={dt:.6} s — caller must guard dt > 0"
        );
        let predicted = self.predicted_model_probabilities();
        let mixed = self.mixed_initial_conditions();
        for (j, mut f) in mixed.into_iter().enumerate() {
            let transition = self.models[j].transition6(dt);
            let noise = self.model_noise(&self.models[j], dt, process);
            f.predict(&transition, &noise);
            self.filters[j] = f;
        }
        self.probabilities = predicted;
        self.combined_estimate()
    }

    /// **Update** stage of the IMM cycle: fold a measurement into each model,
    /// score its likelihood and re-weight the model probabilities
    /// `μ_j ∝ c_j · Λ_j` (the `c_j` being the post-[`predict`](Self::predict)
    /// probabilities). Returns the updated combined estimate.
    ///
    /// Call this only after [`predict`](Self::predict). A scan with no
    /// associated plot (a coast) simply skips it: the probabilities already
    /// hold the Markov-predicted values.
    ///
    /// REQ: FR-TRK-013
    pub fn update(&mut self, measurement: &CartesianMeasurement) -> LinearKalman {
        let r = self.len();
        let mut likelihoods = vec![1.0; r];
        for (j, f) in self.filters.iter_mut().enumerate() {
            likelihoods[j] = f.measurement_likelihood(measurement);
            f.update(measurement);
        }
        let unnormalised: Vec<f64> = (0..r)
            .map(|j| self.probabilities[j] * likelihoods[j])
            .collect();
        let total: f64 = unnormalised.iter().sum();
        if total > MIN_MODEL_PROBABILITY {
            self.probabilities = unnormalised.iter().map(|&u| u / total).collect();
        }
        // else: every model found the plot vanishingly unlikely; keep the
        // Markov-predicted probabilities rather than producing NaNs.
        self.combined_estimate()
    }

    /// **PDA-weighted update** (Häppchen M5.7): instead of a single
    /// measurement, fold in several gated candidates at once, weighted by
    /// their PDA association probabilities `betas`
    /// ([`crate::pda::association_probabilities`] for one track, or the joint
    /// version for several).
    ///
    /// `betas[0]` is the "no detection" weight `β_0`; `betas[1 + j]` is the
    /// weight of `measurements[j]`. Each weight names a complete *branch* of
    /// the IMM cycle:
    /// - branch 0 ("no detection"): the bank stays exactly as
    ///   [`predict`](Self::predict) left it — Markov-predicted probabilities,
    ///   predicted filter states.
    /// - branch `1 + j` ("associate with `measurements[j]`"): the ordinary
    ///   [`update`](Self::update) on a clone of the bank, with its own
    ///   re-weighted model probabilities.
    ///
    /// The new bank is the `β`-weighted blend of these branches: per-model
    /// states blend with the usual spread-of-the-means term (as in
    /// [`combined_estimate`](Self::combined_estimate)), and model
    /// probabilities blend by simple weighted averaging, renormalised.
    ///
    /// Call this only after [`predict`](Self::predict). With `betas = [1.0]`
    /// (no measurements) it is a no-op; with `betas = [0, 1]` and one
    /// measurement it reduces exactly to [`update`](Self::update).
    ///
    /// REQ: FR-TRK-017
    pub fn update_pda(
        &mut self,
        measurements: &[CartesianMeasurement],
        betas: &[f64],
    ) -> LinearKalman {
        assert_eq!(
            betas.len(),
            measurements.len() + 1,
            "betas must have one entry per measurement plus one for 'no detection'"
        );
        let r = self.len();

        // Branch 0: no detection — the bank stays exactly as predicted.
        let mut branch_filters: Vec<Vec<LinearKalman6>> =
            Vec::with_capacity(measurements.len() + 1);
        let mut branch_probs: Vec<Vec<f64>> = Vec::with_capacity(measurements.len() + 1);
        branch_filters.push(self.filters.clone());
        branch_probs.push(self.probabilities.clone());

        // Branches 1..: associate with measurement j — a full IMM update on a clone.
        for m in measurements {
            let mut clone = self.clone();
            clone.update(m);
            branch_filters.push(clone.filters);
            branch_probs.push(clone.probabilities);
        }

        // Blend per-model states across branches (spread-of-the-means).
        let mut new_filters = Vec::with_capacity(r);
        for j in 0..r {
            let mut x = Vector6::zeros();
            for (&beta, bf) in betas.iter().zip(&branch_filters) {
                x += beta * bf[j].x;
            }
            let mut p = Matrix6::zeros();
            for (&beta, bf) in betas.iter().zip(&branch_filters) {
                let d = bf[j].x - x;
                p += beta * (bf[j].p + d * d.transpose());
            }
            new_filters.push(LinearKalman6 { x, p });
        }

        // Blend model probabilities across branches and renormalise.
        let mut new_probs = vec![0.0; r];
        for (&beta, bp) in betas.iter().zip(&branch_probs) {
            for (new_p, &p) in new_probs.iter_mut().zip(bp) {
                *new_p += beta * p;
            }
        }
        let total: f64 = new_probs.iter().sum();
        if total > MIN_MODEL_PROBABILITY {
            for p in &mut new_probs {
                *p /= total;
            }
        }

        self.filters = new_filters;
        self.probabilities = new_probs;
        self.combined_estimate()
    }

    /// Run one full IMM cycle ([`predict`](Self::predict) then, on a hit,
    /// [`update`](Self::update)) and return the combined estimate. Convenience
    /// for using the IMM as a standalone filter; the tracker calls the two
    /// stages separately.
    ///
    /// REQ: FR-TRK-013
    pub fn step(
        &mut self,
        dt: f64,
        process: &ProcessNoise,
        measurement: Option<&CartesianMeasurement>,
    ) -> LinearKalman {
        self.predict(dt, process);
        match measurement {
            Some(m) => self.update(m),
            None => self.combined_estimate(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measurement::CartesianMeasurement;
    use nalgebra::{Matrix2, Matrix4, Vector2, Vector4};

    fn position_measurement(east: f64, north: f64, std: f64) -> CartesianMeasurement {
        CartesianMeasurement {
            z: Vector2::new(east, north),
            r: Matrix2::identity() * std * std,
        }
    }

    /// The VERT.4b core claim: on a take-off-strength acceleration the CA
    /// model takes over and the bank's combined acceleration *state* — the
    /// quantity I062/210 now reports — carries the manoeuvre.
    ///
    /// Honest calibration note: the combined state is the MMSE blend, so its
    /// magnitude is shrunk by the CV/CT share of the model probability while
    /// the evidence is ambiguous. At 4-s scans with σ = 30 m a gentle
    /// 1 m/s² produces only ~8 m of per-scan CV lag (0.27σ) — the posterior
    /// genuinely stays mixed. A real take-off roll (~2.5 m/s²) separates.
    /// REQ: FR-TRK-044
    #[test]
    fn ca_model_wins_on_an_accelerating_target_and_reports_the_state() {
        let accel = 2.5; // m/s² eastward — a jet's take-off roll
        let v0 = 10.0;
        let dt = 4.0;
        let process = ProcessNoise::new(0.5);

        let seed =
            LinearKalman::from_first_measurement(&position_measurement(0.0, 0.0, 30.0), 100.0);
        let mut bank = ImmConfig::cv_turns_and_ca(0.052).seed(seed);

        let ca_index = 3;
        for k in 1..=25 {
            let t = k as f64 * dt;
            let truth_east = v0 * t + 0.5 * accel * t * t;
            bank.predict(dt, &process);
            bank.update(&position_measurement(truth_east, 0.0, 30.0));
        }

        assert!(
            bank.probabilities()[ca_index] > 0.7,
            "CA dominates on sustained acceleration, mu = {:?}",
            bank.probabilities()
        );
        // The mode-matched CA filter itself nails the truth…
        let (ca_east, _) = bank.filters()[ca_index].acceleration();
        assert!(
            (ca_east - accel).abs() < 0.3,
            "CA filter state near {accel} m/s², got {ca_east}"
        );
        // …and the reported MMSE blend carries most of it.
        let (a_east, a_north) = bank.combined_acceleration();
        assert!(
            (a_east - accel).abs() < 0.25 * accel,
            "combined acceleration within 25 % of {accel} m/s², got {a_east}"
        );
        assert!(
            a_north.abs() < 0.3,
            "no phantom cross-track component, got {a_north}"
        );
    }

    /// In a steady coordinated turn the bank's combined acceleration is the
    /// **centripetal** value (from the CT hypothesis' acceleration rows) —
    /// not a spurious zero. REQ: FR-TRK-044
    #[test]
    fn steady_turn_reports_centripetal_acceleration() {
        let rate = 0.052; // rad/s, left turn
        let speed = 150.0;
        let dt = 4.0;
        let process = ProcessNoise::new(0.5);

        let seed =
            LinearKalman::from_first_measurement(&position_measurement(0.0, 0.0, 30.0), 100.0);
        let mut bank = ImmConfig::cv_turns_and_ca(rate).seed(seed);

        // Truth: a circle of radius v/ω around (0, R), starting east-bound…
        // parameterise position directly: p(t) = R·(sin ωt, 1 − cos ωt).
        let radius = speed / rate;
        for k in 1..=40 {
            let theta = rate * k as f64 * dt;
            bank.predict(dt, &process);
            bank.update(&position_measurement(
                radius * theta.sin(),
                radius * (1.0 - theta.cos()),
                30.0,
            ));
        }

        let (a_east, a_north) = bank.combined_acceleration();
        let a_norm = (a_east * a_east + a_north * a_north).sqrt();
        let centripetal = rate * speed; // ≈ 7.8 m/s²
        assert!(
            (a_norm - centripetal).abs() < 0.15 * centripetal,
            "|a| ≈ ω·v = {centripetal:.2} m/s² in a steady turn, got {a_norm:.2}"
        );
    }

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

    /// A position measurement at `(east, north)` with isotropic 1σ = 10 m.
    fn measurement(east: f64, north: f64) -> CartesianMeasurement {
        CartesianMeasurement {
            z: Vector2::new(east, north),
            r: Matrix2::new(100.0, 0.0, 0.0, 100.0),
        }
    }

    /// A two-model IMM (CV + a CT at `rate`) started from one shared estimate,
    /// equal priors and a sticky transition matrix.
    fn cv_ct_imm(rate: f64, start: LinearKalman) -> Imm {
        Imm::new(
            vec![
                MotionModel::ConstantVelocity,
                MotionModel::CoordinatedTurn { rate },
            ],
            vec![start, start],
            vec![0.5, 0.5],
            vec![vec![0.95, 0.05], vec![0.05, 0.95]],
        )
    }

    /// The combined estimate is the probability-weighted blend of the models.
    /// REQ: FR-TRK-013
    #[test]
    fn combined_estimate_is_the_weighted_blend() {
        let imm = Imm::new(
            vec![
                MotionModel::ConstantVelocity,
                MotionModel::CoordinatedTurn { rate: 0.05 },
            ],
            vec![filter(0.0, 0.0, 100.0), filter(100.0, 0.0, 100.0)],
            vec![0.25, 0.75],
            vec![vec![0.9, 0.1], vec![0.1, 0.9]],
        );
        let est = imm.combined_estimate();
        // x = 0.25·0 + 0.75·100 = 75.
        assert!((est.x[0] - 75.0).abs() < 1e-12);
    }

    /// A `step` keeps the model probabilities a proper distribution.
    /// REQ: FR-TRK-013
    #[test]
    fn step_keeps_probabilities_normalised() {
        let start = LinearKalman {
            x: Vector4::new(0.0, 0.0, 100.0, 0.0),
            p: Matrix4::identity() * 100.0,
        };
        let mut imm = cv_ct_imm(0.05, start);
        imm.step(4.0, &ProcessNoise::new(0.5), Some(&measurement(400.0, 0.0)));
        let sum: f64 = imm.probabilities().iter().sum();
        assert!((sum - 1.0).abs() < 1e-12);
        assert!(imm.probabilities().iter().all(|&p| p >= 0.0));
    }

    /// Feeding measurements drawn from a **straight** track makes the
    /// constant-velocity model win: its probability climbs above the turn
    /// model's.
    /// REQ: FR-TRK-013
    #[test]
    fn constant_velocity_model_wins_on_a_straight_track() {
        let speed = 100.0; // m/s due east
        let dt = 4.0;
        let start = LinearKalman {
            x: Vector4::new(0.0, 0.0, speed, 0.0),
            p: Matrix4::identity() * 100.0,
        };
        let mut imm = cv_ct_imm(0.05, start);

        // Truth marches straight east; feed the exact positions.
        for k in 1..=8 {
            let east = speed * dt * k as f64;
            imm.step(dt, &ProcessNoise::new(0.5), Some(&measurement(east, 0.0)));
        }
        let p = imm.probabilities();
        assert!(
            p[0] > p[1],
            "CV should dominate on a straight track: μ = {p:?}"
        );
        assert!(p[0] > 0.8, "CV should be clearly favoured: μ = {p:?}");
    }

    /// Feeding measurements drawn from a **coordinated turn** at rate `ω` makes
    /// the matching CT model win over the constant-velocity model.
    /// REQ: FR-TRK-013
    #[test]
    fn coordinated_turn_model_wins_on_a_turning_track() {
        let speed = 100.0;
        let rate = 0.05; // rad/s, the truth's turn rate
        let dt = 2.0;
        let start = LinearKalman {
            x: Vector4::new(0.0, 0.0, speed, 0.0),
            p: Matrix4::identity() * 100.0,
        };
        let mut imm = cv_ct_imm(rate, start);

        // Truth carves a circle of radius v/ω centred due north of the start.
        // Position after turning angle θ = ω·t (starting east-bound at origin):
        //   east  = (v/ω) sin θ
        //   north = (v/ω)(1 − cos θ)
        let radius = speed / rate;
        for k in 1..=8 {
            let theta = rate * dt * k as f64;
            let east = radius * theta.sin();
            let north = radius * (1.0 - theta.cos());
            imm.step(dt, &ProcessNoise::new(0.5), Some(&measurement(east, north)));
        }
        let p = imm.probabilities();
        assert!(
            p[1] > p[0],
            "CT should dominate on a turning track: μ = {p:?}"
        );
    }

    /// `betas = [1.0]` (no measurements, certainly "no detection") leaves the
    /// bank exactly as `predict` left it — coasting.
    /// REQ: FR-TRK-017
    #[test]
    fn pda_update_with_no_measurements_is_a_noop() {
        let start = LinearKalman {
            x: Vector4::new(0.0, 0.0, 100.0, 0.0),
            p: Matrix4::identity() * 100.0,
        };
        let mut imm = cv_ct_imm(0.05, start);
        imm.predict(4.0, &ProcessNoise::new(0.5));
        let before_probs = imm.probabilities().to_vec();
        let before_filters = imm.filters().to_vec();

        imm.update_pda(&[], &[1.0]);

        assert_eq!(imm.probabilities(), before_probs.as_slice());
        for (after, before) in imm.filters().iter().zip(&before_filters) {
            assert!((after.x - before.x).norm() < 1e-12);
            assert!((after.p - before.p).norm() < 1e-12);
        }
    }

    /// `betas = [0, 1]` (certainly the one measurement) matches a plain
    /// `update`.
    /// REQ: FR-TRK-017
    #[test]
    fn pda_update_with_certain_single_measurement_matches_plain_update() {
        let start = LinearKalman {
            x: Vector4::new(0.0, 0.0, 100.0, 0.0),
            p: Matrix4::identity() * 100.0,
        };
        let mut pda_imm = cv_ct_imm(0.05, start);
        pda_imm.predict(4.0, &ProcessNoise::new(0.5));
        let mut plain_imm = pda_imm.clone();

        let m = measurement(400.0, 10.0);
        pda_imm.update_pda(&[m], &[0.0, 1.0]);
        plain_imm.update(&m);

        assert!((pda_imm.combined_estimate().x - plain_imm.combined_estimate().x).norm() < 1e-9);
        for (a, b) in pda_imm
            .probabilities()
            .iter()
            .zip(plain_imm.probabilities())
        {
            assert!((a - b).abs() < 1e-12);
        }
    }

    /// `update_pda` always leaves the model probabilities a proper
    /// distribution, whatever the betas.
    /// REQ: FR-TRK-017
    #[test]
    fn pda_update_keeps_probabilities_normalised() {
        let start = LinearKalman {
            x: Vector4::new(0.0, 0.0, 100.0, 0.0),
            p: Matrix4::identity() * 100.0,
        };
        let mut imm = cv_ct_imm(0.05, start);
        imm.predict(4.0, &ProcessNoise::new(0.5));

        let m1 = measurement(400.0, 0.0);
        let m2 = measurement(380.0, 30.0);
        imm.update_pda(&[m1, m2], &[0.2, 0.5, 0.3]);

        let sum: f64 = imm.probabilities().iter().sum();
        assert!((sum - 1.0).abs() < 1e-12);
        assert!(imm.probabilities().iter().all(|&p| p >= 0.0));
    }

    /// Coasting (no measurement) relaxes the probabilities toward the
    /// Markov-predicted ones rather than re-weighting by evidence.
    /// REQ: FR-TRK-013
    #[test]
    fn coasting_relaxes_toward_markov_prediction() {
        let start = LinearKalman {
            x: Vector4::new(0.0, 0.0, 100.0, 0.0),
            p: Matrix4::identity() * 100.0,
        };
        let mut imm = cv_ct_imm(0.05, start);
        // Priors (0.5, 0.5) with this transition predict c = (0.5, 0.5); a coast
        // leaves them there.
        let est = imm.step(4.0, &ProcessNoise::new(0.5), None);
        assert!((imm.probabilities()[0] - 0.5).abs() < 1e-12);
        assert!((imm.probabilities()[1] - 0.5).abs() < 1e-12);
        // And it still returns a usable combined estimate.
        assert!(est.x[0] > 0.0, "the coasted estimate moved east");
    }
}
