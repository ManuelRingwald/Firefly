//! A linear Kalman filter on a 2-D constant-velocity motion model.
//!
//! This is the smoothing heart of the tracker. It keeps a running estimate of a
//! target's **state** — position *and* velocity — and refines it scan by scan by
//! alternating two steps:
//!
//! - **Predict:** roll the state forward in time assuming constant velocity, and
//!   *grow* the uncertainty (the future is uncertain, and real aircraft
//!   manoeuvre — captured by the process noise `Q`).
//! - **Update:** fold in a new measurement (from [`crate::measurement`]),
//!   weighting it against the prediction by their relative uncertainties (the
//!   Kalman gain `K`), and *shrink* the uncertainty.
//!
//! State layout: `x = [east, north, v_east, v_north]` (metres, metres/second).
//! Only position is observed (`H`), velocity is inferred over time.
//!
//! Determinism (ADR 0003): `predict`/`update` are pure functions of their
//! inputs and the time step `dt` is *passed in* (data time, not the wall clock),
//! so a run is exactly replayable.

use nalgebra::{Matrix2, Matrix2x4, Matrix4, Vector2, Vector4};
use serde::{Deserialize, Serialize};

use crate::measurement::CartesianMeasurement;
use crate::motion::MotionModel;

/// Process-noise model: how much we let the target deviate from perfectly
/// constant velocity (the "manoeuvre budget").
///
/// Uses the standard *continuous white-noise acceleration* model, parameterised
/// by the power spectral density of the acceleration, `accel_psd` (m²/s³).
/// Larger ⇒ the filter trusts the constant-velocity assumption less and follows
/// measurements more eagerly (better in turns, noisier on straights).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProcessNoise {
    /// Acceleration power spectral density, m²/s³.
    pub accel_psd: f64,
}

impl ProcessNoise {
    pub fn new(accel_psd: f64) -> Self {
        Self { accel_psd }
    }

    /// The discrete process-noise covariance `Q` for a time step `dt`.
    ///
    /// Per axis (east and north, independent) the continuous white-noise
    /// acceleration model gives `q·[[dt³/3, dt²/2], [dt²/2, dt]]` over the
    /// (position, velocity) pair.
    pub fn covariance(&self, dt: f64) -> Matrix4<f64> {
        let q = self.accel_psd;
        let q_pp = q * dt * dt * dt / 3.0; // position–position
        let q_pv = q * dt * dt / 2.0; // position–velocity
        let q_vv = q * dt; // velocity–velocity

        // Layout [E, N, vE, vN]: east pairs (0,2), north pairs (1,3).
        Matrix4::new(
            q_pp, 0.0, q_pv, 0.0, //
            0.0, q_pp, 0.0, q_pv, //
            q_pv, 0.0, q_vv, 0.0, //
            0.0, q_pv, 0.0, q_vv,
        )
    }
}

/// A 2-D constant-velocity Kalman filter state.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinearKalman {
    /// State estimate `[east, north, v_east, v_north]`.
    pub x: Vector4<f64>,
    /// State covariance `P` (4×4).
    pub p: Matrix4<f64>,
}

impl LinearKalman {
    /// Measurement model `H`: we observe position only.
    fn measurement_matrix() -> Matrix2x4<f64> {
        Matrix2x4::new(
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0,
        )
    }

    /// The innovation `y = z − H·x` and its covariance `S = H·P·Hᵀ + R`.
    ///
    /// Shared by gating and the measurement update, so there is one source of
    /// truth for "how far is this plot from the prediction, and how uncertain
    /// is that offset?".
    pub fn innovation(&self, m: &CartesianMeasurement) -> (Vector2<f64>, Matrix2<f64>) {
        let h = Self::measurement_matrix();
        let y = m.z - h * self.x;
        let s = h * self.p * h.transpose() + m.r;
        (y, s)
    }

    /// Squared Mahalanobis distance of a measurement from the prediction:
    /// `d² = yᵀ·S⁻¹·y`. A single "how many sigmas away" number that respects
    /// the elliptical uncertainty — the basis of gating.
    ///
    /// REQ: FR-TRK-004
    pub fn mahalanobis_squared(&self, m: &CartesianMeasurement) -> f64 {
        let (y, s) = self.innovation(m);
        let s_inv = s
            .try_inverse()
            .expect("innovation covariance must be invertible (R is positive definite)");
        (y.transpose() * s_inv * y)[(0, 0)]
    }

    /// The Gaussian **likelihood** of a measurement given the current
    /// (predicted) state: `N(y; 0, S)` with `y`, `S` the innovation and its
    /// covariance. A plot that lands where the filter expected it (small
    /// innovation) scores high; a surprising one scores low.
    ///
    /// The IMM (Häppchen M5.3) uses this to weight each motion model by how
    /// well it predicted the plot — the model that fits the manoeuvre earns the
    /// higher likelihood and so the higher model probability. The measurement
    /// has `m = 2` components, so the normaliser is `2π·√|S|`.
    ///
    /// REQ: FR-TRK-013
    pub fn measurement_likelihood(&self, m: &CartesianMeasurement) -> f64 {
        let (y, s) = self.innovation(m);
        let s_inv = s
            .try_inverse()
            .expect("innovation covariance must be invertible (R is positive definite)");
        let exponent = -0.5 * (y.transpose() * s_inv * y)[(0, 0)];
        let normaliser = std::f64::consts::TAU * s.determinant().sqrt();
        exponent.exp() / normaliser
    }

    /// Initialise a fresh filter from the first measurement: position from the
    /// plot, velocity unknown (zero, with a large `initial_velocity_std`).
    ///
    /// REQ: FR-TRK-003
    pub fn from_first_measurement(m: &CartesianMeasurement, initial_velocity_std: f64) -> Self {
        let x = Vector4::new(m.z.x, m.z.y, 0.0, 0.0);
        let vv = initial_velocity_std * initial_velocity_std;
        // Position block = measurement covariance; velocity block = large.
        let p = Matrix4::new(
            m.r[(0, 0)],
            m.r[(0, 1)],
            0.0,
            0.0, //
            m.r[(1, 0)],
            m.r[(1, 1)],
            0.0,
            0.0, //
            0.0,
            0.0,
            vv,
            0.0, //
            0.0,
            0.0,
            0.0,
            vv,
        );
        Self { x, p }
    }

    /// Predict the state forward by `dt` seconds under the constant-velocity
    /// motion model (the M2 default).
    ///
    /// REQ: FR-TRK-003
    pub fn predict(&mut self, dt: f64, process: &ProcessNoise) {
        self.predict_with(&MotionModel::ConstantVelocity, dt, process);
    }

    /// Predict the state forward by `dt` seconds under an explicit motion model.
    ///
    /// Same prediction equations as [`predict`](Self::predict) — `x ← F·x`,
    /// `P ← F·P·Fᵀ + Q` — but the state-transition matrix `F` comes from
    /// `model` (constant velocity, a coordinated turn, …). This is the hook the
    /// IMM (Häppchen M5.2+) uses to run several motion hypotheses in parallel.
    ///
    /// REQ: FR-TRK-003, FR-TRK-011
    pub fn predict_with(&mut self, model: &MotionModel, dt: f64, process: &ProcessNoise) {
        let f = model.transition(dt);
        self.x = f * self.x;
        self.p = f * self.p * f.transpose() + process.covariance(dt);
    }

    /// Fold in a Cartesian measurement.
    ///
    /// Uses the Joseph form for the covariance update, which stays symmetric and
    /// positive definite even with finite-precision arithmetic — the kind of
    /// numerical care that matters for assurance (ADR 0004).
    ///
    /// REQ: FR-TRK-003
    pub fn update(&mut self, m: &CartesianMeasurement) {
        let h = Self::measurement_matrix();

        let (innovation, s) = self.innovation(m);
        let s_inv = s
            .try_inverse()
            .expect("innovation covariance must be invertible (R is positive definite)");
        let k = self.p * h.transpose() * s_inv; // Kalman gain (4×2)

        self.x += k * innovation;

        // Joseph form: P = (I - K H) P (I - K H)ᵀ + K R Kᵀ.
        let i = Matrix4::identity();
        let ikh = i - k * h;
        self.p = ikh * self.p * ikh.transpose() + k * m.r * k.transpose();
    }

    /// Fold in **several** candidate measurements at once, weighted by their
    /// PDA association probabilities `betas` (Häppchen M5.5,
    /// [`crate::pda::association_probabilities`]).
    ///
    /// `betas[0]` is the "no detection" weight `β_0`; `betas[1 + j]` is the
    /// weight of `measurements[j]`. The two must have matching lengths
    /// (`betas.len() == measurements.len() + 1`) and `betas` must sum to 1.
    ///
    /// The idea mirrors the IMM's combination step (M5.2): each candidate —
    /// "no detection" (the prediction itself) or "associate with measurement
    /// `j`" (the ordinary [`update`](Self::update) with that measurement) —
    /// is its own little hypothesis with its own `(x, P)`. The result is the
    /// `β`-weighted mean of these hypotheses, plus a **spread-of-the-means**
    /// term: hypotheses that disagree about where the target is make the
    /// blended estimate honestly *more* uncertain, not less. When there is
    /// only one hypothesis (e.g. an empty `measurements`, so `betas = [1.0]`),
    /// this reduces to leaving the state untouched, as expected.
    ///
    /// REQ: FR-TRK-016
    pub fn update_pda(&mut self, measurements: &[CartesianMeasurement], betas: &[f64]) {
        assert_eq!(
            betas.len(),
            measurements.len() + 1,
            "betas must have one entry per measurement plus one for 'no detection'"
        );

        // Hypothesis 0: no detection — the state stays at the prediction.
        let mut candidates: Vec<(Vector4<f64>, Matrix4<f64>)> =
            Vec::with_capacity(measurements.len() + 1);
        candidates.push((self.x, self.p));
        for m in measurements {
            let mut updated = *self;
            updated.update(m);
            candidates.push((updated.x, updated.p));
        }

        let mut x = Vector4::zeros();
        for (&beta, &(xi, _)) in betas.iter().zip(&candidates) {
            x += beta * xi;
        }

        let mut p = Matrix4::zeros();
        for (&beta, &(xi, pi)) in betas.iter().zip(&candidates) {
            let d = xi - x;
            p += beta * (pi + d * d.transpose());
        }

        self.x = x;
        self.p = p;
    }

    /// Estimated position `[east, north]`, metres.
    pub fn position(&self) -> Vector2<f64> {
        Vector2::new(self.x[0], self.x[1])
    }

    /// Estimated velocity `[v_east, v_north]`, m/s.
    pub fn velocity(&self) -> Vector2<f64> {
        Vector2::new(self.x[2], self.x[3])
    }

    /// The 2×2 position block of the state covariance.
    pub fn position_covariance(&self) -> Matrix2<f64> {
        self.p.fixed_view::<2, 2>(0, 0).into_owned()
    }

    /// The 1σ **semi-major axis** of the position error ellipse, in metres —
    /// a single honest scalar for "how uncertain is the position right now".
    ///
    /// The position covariance is anisotropic (the same cigar shape as the
    /// measurement and the gate), so we report its *longest* 1σ spread: the
    /// square root of the larger eigenvalue of the 2×2 position block. For a
    /// symmetric `[[a, b], [b, c]]` the eigenvalues have the closed form
    /// `(a+c)/2 ± sqrt(((a−c)/2)² + b²)` — exact, no iteration.
    pub fn position_uncertainty(&self) -> f64 {
        let p = self.position_covariance();
        let (a, b, c) = (p[(0, 0)], p[(0, 1)], p[(1, 1)]);
        let mean = 0.5 * (a + c);
        let half_diff = 0.5 * (a - c);
        let radius = (half_diff * half_diff + b * b).sqrt();
        (mean + radius).max(0.0).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measurement(east: f64, north: f64, var: f64) -> CartesianMeasurement {
        CartesianMeasurement {
            z: Vector2::new(east, north),
            r: Matrix2::new(var, 0.0, 0.0, var),
        }
    }

    /// Prediction advances position by velocity·dt, leaves velocity untouched,
    /// and increases positional uncertainty.
    /// REQ: FR-TRK-003
    #[test]
    fn predict_moves_position_and_grows_uncertainty() {
        let mut kf = LinearKalman {
            x: Vector4::new(0.0, 0.0, 100.0, 0.0), // moving east at 100 m/s
            p: Matrix4::identity() * 10.0,
        };
        let before = kf.position_covariance()[(0, 0)];
        kf.predict(10.0, &ProcessNoise::new(1.0));

        assert!((kf.position()[0] - 1000.0).abs() < 1e-9);
        assert!(kf.position()[1].abs() < 1e-9);
        assert!((kf.velocity()[0] - 100.0).abs() < 1e-9);
        assert!(
            kf.position_covariance()[(0, 0)] > before,
            "uncertainty should grow"
        );
    }

    /// An update reduces positional uncertainty.
    /// REQ: FR-TRK-003
    #[test]
    fn update_reduces_uncertainty() {
        let mut kf = LinearKalman::from_first_measurement(&measurement(0.0, 0.0, 2500.0), 300.0);
        kf.predict(4.0, &ProcessNoise::new(1.0));
        let before = kf.position_covariance()[(0, 0)];
        kf.update(&measurement(10.0, 0.0, 2500.0));
        assert!(
            kf.position_covariance()[(0, 0)] < before,
            "update should sharpen"
        );
    }

    /// The position uncertainty is the 1σ semi-major axis of the error ellipse:
    /// the square root of the *larger* eigenvalue of the 2×2 position block.
    /// REQ: FR-TRK-008
    #[test]
    fn position_uncertainty_is_semi_major_one_sigma() {
        // Anisotropic, axis-aligned: variances 100 (east) and 25 (north).
        let kf = LinearKalman {
            x: Vector4::zeros(),
            p: {
                let mut p = Matrix4::zeros();
                p[(0, 0)] = 100.0;
                p[(1, 1)] = 25.0;
                p
            },
        };
        // Larger eigenvalue is 100 → semi-major 1σ = 10 m (not sqrt(125)).
        assert!((kf.position_uncertainty() - 10.0).abs() < 1e-9);

        // Prediction grows the cigar, so the semi-major axis grows too.
        let mut moving = kf;
        let before = moving.position_uncertainty();
        moving.predict(10.0, &ProcessNoise::new(1.0));
        assert!(moving.position_uncertainty() > before);
    }

    /// A very precise measurement pulls the estimate almost onto it; a very
    /// vague one barely moves it. (The Kalman gain as a trust lever.)
    /// REQ: FR-TRK-003
    #[test]
    fn gain_respects_measurement_precision() {
        // Precise measurement (tiny variance).
        let mut sharp = LinearKalman {
            x: Vector4::new(0.0, 0.0, 0.0, 0.0),
            p: Matrix4::identity() * 1000.0,
        };
        sharp.update(&measurement(100.0, 0.0, 0.01));
        assert!(
            (sharp.position()[0] - 100.0).abs() < 1.0,
            "should snap to precise measurement"
        );

        // Vague measurement (huge variance).
        let mut vague = LinearKalman {
            x: Vector4::new(0.0, 0.0, 0.0, 0.0),
            p: Matrix4::identity() * 1000.0,
        };
        vague.update(&measurement(100.0, 0.0, 1.0e9));
        assert!(
            vague.position()[0].abs() < 1.0,
            "should barely move for vague measurement"
        );
    }

    /// The covariance stays symmetric and positive definite after an update.
    /// REQ: FR-TRK-003
    #[test]
    fn covariance_stays_valid() {
        let mut kf = LinearKalman::from_first_measurement(&measurement(5.0, -3.0, 2500.0), 200.0);
        kf.predict(4.0, &ProcessNoise::new(2.0));
        kf.update(&measurement(7.0, -2.0, 2500.0));
        let p = kf.p;
        // Symmetric.
        for i in 0..4 {
            for j in 0..4 {
                assert!((p[(i, j)] - p[(j, i)]).abs() < 1e-6, "P must be symmetric");
            }
        }
        // Positive definite diagonal and determinant.
        assert!((0..4).all(|i| p[(i, i)] > 0.0));
        assert!(p.determinant() > 0.0);
    }

    /// `betas = [1.0]` (no measurements, certainly "no detection") leaves the
    /// state untouched — coasting.
    /// REQ: FR-TRK-016
    #[test]
    fn pda_update_with_no_measurements_is_a_noop() {
        let mut kf = LinearKalman {
            x: Vector4::new(1.0, 2.0, 3.0, 4.0),
            p: Matrix4::identity() * 100.0,
        };
        let before = kf;
        kf.update_pda(&[], &[1.0]);
        assert_eq!(kf.x, before.x);
        assert_eq!(kf.p, before.p);
    }

    /// `betas = [0, 1]` (certainly the one measurement) matches the plain
    /// `update`.
    /// REQ: FR-TRK-016
    #[test]
    fn pda_update_with_certain_single_measurement_matches_plain_update() {
        let mut pda_kf =
            LinearKalman::from_first_measurement(&measurement(0.0, 0.0, 2500.0), 300.0);
        pda_kf.predict(4.0, &ProcessNoise::new(1.0));
        let mut plain_kf = pda_kf;

        let m = measurement(10.0, -5.0, 2500.0);
        pda_kf.update_pda(&[m], &[0.0, 1.0]);
        plain_kf.update(&m);

        assert!((pda_kf.x - plain_kf.x).norm() < 1e-9);
        assert!((pda_kf.p - plain_kf.p).norm() < 1e-9);
    }

    /// Two measurements pulling the estimate in different directions, each
    /// with weight 0.5: the result sits between them, and — because the
    /// hypotheses disagree — ends up *more* uncertain than either single
    /// candidate update alone (spread-of-the-means).
    /// REQ: FR-TRK-016
    #[test]
    fn pda_update_blends_disagreeing_measurements_and_inflates_uncertainty() {
        let mut kf = LinearKalman::from_first_measurement(&measurement(0.0, 0.0, 2500.0), 300.0);
        kf.predict(4.0, &ProcessNoise::new(1.0));

        let left = measurement(-50.0, 0.0, 2500.0);
        let right = measurement(50.0, 0.0, 2500.0);

        let mut single = kf;
        single.update(&left);

        kf.update_pda(&[left, right], &[0.0, 0.5, 0.5]);

        // The blended east position sits between the two measurements.
        assert!(kf.position()[0].abs() < 1e-6, "x = {}", kf.position()[0]);
        // Disagreement inflates the position uncertainty beyond either
        // single-measurement update.
        assert!(kf.position_covariance()[(0, 0)] > single.position_covariance()[(0, 0)]);
    }

    /// `betas` summing to 1 keeps the resulting covariance symmetric and
    /// positive definite.
    /// REQ: FR-TRK-016
    #[test]
    fn pda_update_keeps_covariance_valid() {
        let mut kf = LinearKalman::from_first_measurement(&measurement(0.0, 0.0, 2500.0), 300.0);
        kf.predict(4.0, &ProcessNoise::new(1.0));

        let m1 = measurement(10.0, 0.0, 2500.0);
        let m2 = measurement(-5.0, 8.0, 2500.0);
        kf.update_pda(&[m1, m2], &[0.2, 0.5, 0.3]);

        let p = kf.p;
        for i in 0..4 {
            for j in 0..4 {
                assert!((p[(i, j)] - p[(j, i)]).abs() < 1e-6, "P must be symmetric");
            }
        }
        assert!((0..4).all(|i| p[(i, i)] > 0.0));
        assert!(p.determinant() > 0.0);
    }
}
