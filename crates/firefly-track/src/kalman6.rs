//! The 6-D state foundation for the constant-acceleration IMM bank (VERT.4a).
//!
//! The 4-D bank ([`crate::imm`]) treats longitudinal acceleration as process
//! noise, so the position estimate lags the target on take-off roll, climb
//! acceleration and the deceleration on final. VERT.4 fixes that by running
//! the whole bank on a **6-D state** `x = [E, N, vE, vN, aE, aN]` (ADR 0035,
//! Weg A): every model becomes 6-D *inside* the bank, and only the bank's rim
//! projects back to the 4-D `(position, velocity)` estimate the rest of the
//! fusion core consumes — gating, JPDA and registration stay untouched.
//!
//! This module is that foundation, deliberately **not yet wired** into the
//! bank (that is VERT.4b): the 6-D filter twin of [`crate::kalman`], the 6-D
//! transition matrices of the three model families, the white-noise-**jerk**
//! process noise (the CWNA analogue one derivative up), and the two Weg-A
//! boundary maps (4-D → 6-D embedding, 6-D → 4-D marginal).
//!
//! What each hypothesis claims about the acceleration state:
//!
//! - **Constant acceleration:** `a` is free and couples into `v` and `p`
//!   (`p' = p + v·dt + a·dt²/2`), driven by white-noise jerk.
//! - **Constant velocity:** "there is no acceleration" — the transition's
//!   acceleration rows are zero, so a mixed-in acceleration is honestly
//!   zeroed under this hypothesis (not carried along as dead weight).
//! - **Coordinated turn:** the manoeuvre lives in the velocity rotation; the
//!   acceleration state is set to the **centripetal** value `a' = ω·J·v'`
//!   (linear in the state!), so a turning track reports its true lateral
//!   acceleration instead of a spurious zero.
//!
//! Numerics mirror [`crate::kalman::LinearKalman`] exactly: position-only
//! measurement matrix, shared innovation, Gaussian likelihood with the
//! `2π·√|S|` normaliser, Joseph-form covariance update. Determinism
//! (ADR 0003): every function is pure in its inputs; `dt` is data time.
//!
//! REQ: FR-TRK-044

use nalgebra::{Matrix2, Matrix2x6, Matrix6, Vector2, Vector6};
use serde::{Deserialize, Serialize};

use crate::kalman::LinearKalman;
use crate::measurement::CartesianMeasurement;

/// Below this turn rate (rad/s) a coordinated turn is numerically a straight
/// line and the CV transition is used instead (mirrors `motion.rs`).
const STRAIGHT_FLIGHT_RATE: f64 = 1e-6;

/// The constant-acceleration state-transition matrix for a step `dt`:
/// per axis `p' = p + v·dt + a·dt²/2`, `v' = v + a·dt`, `a' = a`.
///
/// East couples the state indices (0, 2, 4), north (1, 3, 5).
pub fn constant_acceleration_transition(dt: f64) -> Matrix6<f64> {
    let half_dt2 = 0.5 * dt * dt;
    let mut f = Matrix6::identity();
    f[(0, 2)] = dt;
    f[(1, 3)] = dt;
    f[(0, 4)] = half_dt2;
    f[(1, 5)] = half_dt2;
    f[(2, 4)] = dt;
    f[(3, 5)] = dt;
    f
}

/// The constant-velocity hypothesis on the 6-D state: the familiar CV block
/// on `(p, v)`, and **zero acceleration rows** — this model claims the target
/// is not accelerating, so it maps any mixed-in acceleration to 0 rather than
/// carrying it along. (`F` is deliberately singular; a Kalman transition need
/// not be invertible.)
pub fn constant_velocity_transition6(dt: f64) -> Matrix6<f64> {
    let mut f = Matrix6::zeros();
    f[(0, 0)] = 1.0;
    f[(1, 1)] = 1.0;
    f[(2, 2)] = 1.0;
    f[(3, 3)] = 1.0;
    f[(0, 2)] = dt;
    f[(1, 3)] = dt;
    f
}

/// The coordinated-turn hypothesis on the 6-D state: the 4-D CT block on
/// `(p, v)` (velocity rotated by `ω·dt`, arc integrated into position), and
/// acceleration rows set to the **centripetal** acceleration of the turn,
/// `a' = ω·J·v'` with `J` the +90° rotation — i.e. `a'E = -ω·v'N`,
/// `a'N = ω·v'E`. That is linear in the old velocity, so it lives in `F`:
/// a track riding the CT model reports its true lateral acceleration.
pub fn coordinated_turn_transition6(rate: f64, dt: f64) -> Matrix6<f64> {
    if rate.abs() < STRAIGHT_FLIGHT_RATE {
        return constant_velocity_transition6(dt);
    }
    let theta = rate * dt;
    let (sin, cos) = theta.sin_cos();
    let s_over_w = sin / rate; // → dt as rate → 0
    let one_minus_c_over_w = (1.0 - cos) / rate; // → 0  as rate → 0

    let mut f = Matrix6::zeros();
    // Position block (identical to the 4-D CT).
    f[(0, 0)] = 1.0;
    f[(1, 1)] = 1.0;
    f[(0, 2)] = s_over_w;
    f[(0, 3)] = -one_minus_c_over_w;
    f[(1, 2)] = one_minus_c_over_w;
    f[(1, 3)] = s_over_w;
    // Velocity block: rotation by θ.
    f[(2, 2)] = cos;
    f[(2, 3)] = -sin;
    f[(3, 2)] = sin;
    f[(3, 3)] = cos;
    // Acceleration rows: a' = ω·J·v' = ω·J·R(θ)·v, expanded onto the old v.
    f[(4, 2)] = -rate * sin;
    f[(4, 3)] = -rate * cos;
    f[(5, 2)] = rate * cos;
    f[(5, 3)] = -rate * sin;
    f
}

/// Process-noise model for the constant-acceleration hypothesis: how much the
/// target's acceleration itself may wander (the "jerk budget") — the CWNA
/// model of [`crate::kalman::ProcessNoise`] taken one derivative up.
///
/// Parameterised by the power spectral density of the **jerk**, `jerk_psd`
/// (m²/s⁵). Larger ⇒ the filter trusts the constant-acceleration assumption
/// less and follows acceleration changes more eagerly.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct JerkNoise {
    /// Jerk power spectral density, m²/s⁵.
    pub jerk_psd: f64,
}

impl JerkNoise {
    pub fn new(jerk_psd: f64) -> Self {
        Self { jerk_psd }
    }

    /// The discrete process-noise covariance `Q` for a time step `dt`.
    ///
    /// Per axis the continuous white-noise jerk model gives, over the
    /// (position, velocity, acceleration) triple:
    ///
    /// ```text
    /// q · [dt⁵/20  dt⁴/8  dt³/6]
    ///     [dt⁴/8   dt³/3  dt²/2]
    ///     [dt³/6   dt²/2  dt   ]
    /// ```
    pub fn covariance(&self, dt: f64) -> Matrix6<f64> {
        let q = self.jerk_psd;
        let dt2 = dt * dt;
        let dt3 = dt2 * dt;
        let dt4 = dt3 * dt;
        let dt5 = dt4 * dt;
        let q_pp = q * dt5 / 20.0;
        let q_pv = q * dt4 / 8.0;
        let q_pa = q * dt3 / 6.0;
        let q_vv = q * dt3 / 3.0;
        let q_va = q * dt2 / 2.0;
        let q_aa = q * dt;

        // East couples indices (0, 2, 4), north (1, 3, 5), independently.
        let mut m = Matrix6::zeros();
        for axis in 0..2 {
            let (p, v, a) = (axis, axis + 2, axis + 4);
            m[(p, p)] = q_pp;
            m[(v, v)] = q_vv;
            m[(a, a)] = q_aa;
            m[(p, v)] = q_pv;
            m[(v, p)] = q_pv;
            m[(p, a)] = q_pa;
            m[(a, p)] = q_pa;
            m[(v, a)] = q_va;
            m[(a, v)] = q_va;
        }
        m
    }
}

/// A 2-D Kalman filter state on the 6-D `[E, N, vE, vN, aE, aN]` layout —
/// the twin of [`LinearKalman`] the VERT.4b bank will run every model on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinearKalman6 {
    /// State estimate `[east, north, v_east, v_north, a_east, a_north]`.
    pub x: Vector6<f64>,
    /// State covariance `P` (6×6).
    pub p: Matrix6<f64>,
}

impl LinearKalman6 {
    /// Measurement model `H`: we observe position only, exactly as in 4-D.
    fn measurement_matrix() -> Matrix2x6<f64> {
        let mut h = Matrix2x6::zeros();
        h[(0, 0)] = 1.0;
        h[(1, 1)] = 1.0;
        h
    }

    /// **Embed** a 4-D estimate into the 6-D state (the Weg-A input map, used
    /// to seed the 6-D bank): position/velocity and their covariance carry
    /// over, the acceleration starts at 0 with a diagonal prior of
    /// `accel_std²` and no cross-covariance — we claim nothing about how the
    /// unknown acceleration correlates with the known state.
    pub fn from_kalman4(k: &LinearKalman, accel_std: f64) -> Self {
        let mut x = Vector6::zeros();
        let mut p = Matrix6::zeros();
        for i in 0..4 {
            x[i] = k.x[i];
            for j in 0..4 {
                p[(i, j)] = k.p[(i, j)];
            }
        }
        let aa = accel_std * accel_std;
        p[(4, 4)] = aa;
        p[(5, 5)] = aa;
        Self { x, p }
    }

    /// **Project** to the 4-D estimate (the Weg-A output map): the exact
    /// Gaussian marginal over `(position, velocity)` — first four state
    /// entries, top-left 4×4 covariance block. This is what keeps gating,
    /// JPDA and registration on their unchanged 4-D contract.
    pub fn to_kalman4(&self) -> LinearKalman {
        let mut x = nalgebra::Vector4::zeros();
        let mut p = nalgebra::Matrix4::zeros();
        for i in 0..4 {
            x[i] = self.x[i];
            for j in 0..4 {
                p[(i, j)] = self.p[(i, j)];
            }
        }
        LinearKalman { x, p }
    }

    /// The estimated acceleration `(a_east, a_north)` in m/s² — the state
    /// VERT.4b will report into I062/210 instead of the derived VERT.3
    /// estimate.
    pub fn acceleration(&self) -> (f64, f64) {
        (self.x[4], self.x[5])
    }

    /// The innovation `y = z − H·x` and its covariance `S = H·P·Hᵀ + R` —
    /// one source of truth for update and likelihood, as in 4-D.
    pub fn innovation(&self, m: &CartesianMeasurement) -> (Vector2<f64>, Matrix2<f64>) {
        let h = Self::measurement_matrix();
        let y = m.z - h * self.x;
        let s = h * self.p * h.transpose() + m.r;
        (y, s)
    }

    /// The Gaussian likelihood `N(y; 0, S)` of a measurement given the
    /// current (predicted) state — the IMM's model-weighting signal
    /// (normaliser `2π·√|S|` for the 2-component measurement, as in 4-D).
    pub fn measurement_likelihood(&self, m: &CartesianMeasurement) -> f64 {
        let (y, s) = self.innovation(m);
        let s_inv = s
            .try_inverse()
            .expect("innovation covariance must be invertible (R is positive definite)");
        let exponent = -0.5 * (y.transpose() * s_inv * y)[(0, 0)];
        let normaliser = std::f64::consts::TAU * s.determinant().sqrt();
        exponent.exp() / normaliser
    }

    /// Predict the state forward: `x ← F·x`, `P ← F·P·Fᵀ + Q`. The transition
    /// and process noise are passed in — the model wiring (which hypothesis
    /// produces which `F`/`Q`) is the bank's job in VERT.4b.
    pub fn predict(&mut self, f: &Matrix6<f64>, q: &Matrix6<f64>) {
        self.x = f * self.x;
        self.p = f * self.p * f.transpose() + q;
    }

    /// Fold in a Cartesian measurement, Joseph form for the covariance
    /// (symmetric and positive semi-definite under finite precision — the
    /// same numerical care as the 4-D filter, ADR 0004).
    pub fn update(&mut self, m: &CartesianMeasurement) {
        let h = Self::measurement_matrix();

        let (innovation, s) = self.innovation(m);
        let s_inv = s
            .try_inverse()
            .expect("innovation covariance must be invertible (R is positive definite)");
        let k = self.p * h.transpose() * s_inv; // Kalman gain (6×2)

        self.x += k * innovation;

        // Joseph form: P = (I - K H) P (I - K H)ᵀ + K R Kᵀ.
        let i = Matrix6::identity();
        let ikh = i - k * h;
        self.p = ikh * self.p * ikh.transpose() + k * m.r * k.transpose();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::MotionModel;
    use nalgebra::{Matrix2, Vector2};

    fn measurement(east: f64, north: f64, std: f64) -> CartesianMeasurement {
        CartesianMeasurement {
            z: Vector2::new(east, north),
            r: Matrix2::identity() * std * std,
        }
    }

    /// The CA transition reproduces uniform-acceleration kinematics exactly
    /// and composes over time steps (`F(a+b) = F(b)·F(a)`). REQ: FR-TRK-044
    #[test]
    fn ca_transition_predicts_uniform_acceleration_exactly() {
        let x0 = Vector6::new(100.0, -50.0, 200.0, 10.0, 0.5, -0.25);
        let dt = 4.0;
        let x1 = constant_acceleration_transition(dt) * x0;
        // p' = p + v·dt + a·dt²/2, v' = v + a·dt, a' = a — per axis.
        assert!((x1[0] - (100.0 + 200.0 * dt + 0.5 * 0.5 * dt * dt)).abs() < 1e-12);
        assert!((x1[1] - (-50.0 + 10.0 * dt - 0.25 * 0.5 * dt * dt)).abs() < 1e-12);
        assert!((x1[2] - (200.0 + 0.5 * dt)).abs() < 1e-12);
        assert!((x1[3] - (10.0 - 0.25 * dt)).abs() < 1e-12);
        assert!((x1[4] - 0.5).abs() < 1e-12 && (x1[5] + 0.25).abs() < 1e-12);

        // Semigroup: two small steps equal one big step.
        let two_steps =
            constant_acceleration_transition(1.5) * constant_acceleration_transition(2.5);
        let one_step = constant_acceleration_transition(4.0);
        assert!((two_steps - one_step).norm() < 1e-12);
    }

    /// CV6/CT6 embed their 4-D counterparts exactly on the (p, v) block; the
    /// CV hypothesis zeroes the acceleration state. REQ: FR-TRK-044
    #[test]
    fn cv6_and_ct6_embed_their_4d_counterparts() {
        let dt = 4.0;
        let rate = 0.052; // a civil rate-one turn, ≈3°/s

        let cv4 = MotionModel::ConstantVelocity.transition(dt);
        let ct4 = MotionModel::CoordinatedTurn { rate }.transition(dt);
        let cv6 = constant_velocity_transition6(dt);
        let ct6 = coordinated_turn_transition6(rate, dt);

        for i in 0..4 {
            for j in 0..4 {
                assert!((cv6[(i, j)] - cv4[(i, j)]).abs() < 1e-15, "CV ({i},{j})");
                assert!((ct6[(i, j)] - ct4[(i, j)]).abs() < 1e-15, "CT ({i},{j})");
            }
        }

        // CV claims "no acceleration": its acceleration rows are zero.
        let x = Vector6::new(0.0, 0.0, 200.0, 0.0, 3.0, -2.0);
        let x1 = cv6 * x;
        assert_eq!((x1[4], x1[5]), (0.0, 0.0));

        // A vanishing rate degenerates to CV6 (numerical guard).
        let straight = coordinated_turn_transition6(1e-9, dt);
        assert!((straight - cv6).norm() < 1e-12);
    }

    /// The CT hypothesis reports the true centripetal acceleration:
    /// `|a| = ω·|v|`, perpendicular to the new velocity, pointing into the
    /// turn (left for mathematically positive ω). REQ: FR-TRK-044
    #[test]
    fn ct6_reports_centripetal_acceleration() {
        let rate = 0.052;
        let dt = 4.0;
        let speed = 200.0;
        let x0 = Vector6::new(0.0, 0.0, speed, 0.0, 0.0, 0.0);
        let x1 = coordinated_turn_transition6(rate, dt) * x0;

        let v = Vector2::new(x1[2], x1[3]);
        let a = Vector2::new(x1[4], x1[5]);
        // Magnitude ω·|v|, perpendicular to v.
        assert!((a.norm() - rate * speed).abs() < 1e-9);
        assert!(v.dot(&a).abs() < 1e-9);
        // Left turn: a = ω·J·v points 90° anticlockwise of v.
        let left_of_v = Vector2::new(-v[1], v[0]) * rate;
        assert!((a - left_of_v).norm() < 1e-9);
    }

    /// The white-noise-jerk Q is symmetric, positive definite, axis-decoupled
    /// and linear in the PSD. REQ: FR-TRK-044
    #[test]
    fn jerk_q_is_symmetric_positive_definite_and_scales() {
        let q1 = JerkNoise::new(1.0).covariance(4.0);
        assert!((q1 - q1.transpose()).norm() < 1e-15, "symmetric");
        assert!(q1.cholesky().is_some(), "positive definite");
        // East/north are independent: no cross-axis coupling.
        for (i, j) in [(0, 1), (0, 3), (0, 5), (2, 1), (2, 5), (4, 1), (4, 3)] {
            assert_eq!(q1[(i, j)], 0.0, "axis coupling ({i},{j})");
        }
        let q3 = JerkNoise::new(3.0).covariance(4.0);
        assert!((q3 - q1 * 3.0).norm() < 1e-12, "linear in the PSD");
    }

    /// The flagship claim of VERT.4: fed position-only measurements of a
    /// uniformly accelerating target, the 6-D filter estimates velocity *and*
    /// acceleration as state — no differentiation involved. REQ: FR-TRK-044
    #[test]
    fn filter_estimates_acceleration_as_state() {
        let accel = 0.5; // m/s² eastward
        let v0 = 150.0;
        let dt = 4.0;
        let noise = JerkNoise::new(0.01);

        let seed4 = LinearKalman::from_first_measurement(&measurement(0.0, 0.0, 30.0), 100.0);
        let mut filter = LinearKalman6::from_kalman4(&seed4, 3.0);

        for k in 1..=25 {
            let t = k as f64 * dt;
            let truth_east = v0 * t + 0.5 * accel * t * t;
            let f = constant_acceleration_transition(dt);
            let q = noise.covariance(dt);
            filter.predict(&f, &q);
            filter.update(&measurement(truth_east, 0.0, 30.0));
        }

        let (a_east, a_north) = filter.acceleration();
        assert!(
            (a_east - accel).abs() < 0.05,
            "acceleration converged to {accel} m/s², got {a_east}"
        );
        assert!(a_north.abs() < 0.05, "no phantom cross-track acceleration");
        let v_final = v0 + accel * 25.0 * dt;
        assert!(
            (filter.x[2] - v_final).abs() < 2.0,
            "velocity tracks the accelerating target, got {} vs {v_final}",
            filter.x[2]
        );
    }

    /// The Weg-A boundary maps are consistent: embed → project is the
    /// identity on (p, v), and the embedding claims a zero-mean acceleration
    /// with the given prior. REQ: FR-TRK-044
    #[test]
    fn embedding_and_projection_are_consistent() {
        let mut k4 = LinearKalman::from_first_measurement(&measurement(500.0, -200.0, 25.0), 80.0);
        k4.x[2] = 120.0; // give it a non-trivial velocity state
        k4.x[3] = -15.0;

        let k6 = LinearKalman6::from_kalman4(&k4, 3.0);
        assert_eq!(k6.acceleration(), (0.0, 0.0));
        assert_eq!(k6.p[(4, 4)], 9.0);
        assert_eq!(k6.p[(4, 0)], 0.0, "no claimed cross-covariance");

        let back = k6.to_kalman4();
        assert_eq!(back.x, k4.x);
        assert_eq!(back.p, k4.p);
    }

    /// Update mirrors the 4-D numerics: a near measurement scores a higher
    /// likelihood than a far one, the update shrinks position uncertainty and
    /// the Joseph form keeps P symmetric. REQ: FR-TRK-044
    #[test]
    fn update_shrinks_uncertainty_and_keeps_p_symmetric() {
        let seed4 = LinearKalman::from_first_measurement(&measurement(0.0, 0.0, 30.0), 100.0);
        let mut filter = LinearKalman6::from_kalman4(&seed4, 3.0);
        let f = constant_acceleration_transition(4.0);
        let q = JerkNoise::new(0.01).covariance(4.0);
        filter.predict(&f, &q);

        let near = measurement(5.0, -5.0, 30.0);
        let far = measurement(500.0, 500.0, 30.0);
        assert!(filter.measurement_likelihood(&near) > filter.measurement_likelihood(&far));

        let before = filter.p[(0, 0)] + filter.p[(1, 1)];
        filter.update(&near);
        let after = filter.p[(0, 0)] + filter.p[(1, 1)];
        assert!(after < before, "position uncertainty shrinks");
        assert!(
            (filter.p - filter.p.transpose()).norm() < 1e-9,
            "Joseph form keeps P symmetric"
        );
    }
}
