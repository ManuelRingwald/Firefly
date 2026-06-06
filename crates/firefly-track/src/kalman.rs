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

use crate::measurement::CartesianMeasurement;

/// Process-noise model: how much we let the target deviate from perfectly
/// constant velocity (the "manoeuvre budget").
///
/// Uses the standard *continuous white-noise acceleration* model, parameterised
/// by the power spectral density of the acceleration, `accel_psd` (m²/s³).
/// Larger ⇒ the filter trusts the constant-velocity assumption less and follows
/// measurements more eagerly (better in turns, noisier on straights).
#[derive(Debug, Clone, Copy, PartialEq)]
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
#[derive(Debug, Clone, Copy, PartialEq)]
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

    /// State-transition matrix `F` for a time step `dt` (constant velocity).
    fn transition(dt: f64) -> Matrix4<f64> {
        Matrix4::new(
            1.0, 0.0, dt, 0.0, //
            0.0, 1.0, 0.0, dt, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0,
        )
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

    /// Predict the state forward by `dt` seconds.
    ///
    /// REQ: FR-TRK-003
    pub fn predict(&mut self, dt: f64, process: &ProcessNoise) {
        let f = Self::transition(dt);
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

        let innovation = m.z - h * self.x; // y = z - H x
        let s = h * self.p * h.transpose() + m.r; // innovation covariance
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
}
