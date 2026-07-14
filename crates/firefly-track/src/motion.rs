//! Motion models: *how* we expect a target to move between scans.
//!
//! The Kalman filter's **predict** step rolls the state forward with a
//! state-transition matrix `F(dt)`. Which `F` is right depends on what the
//! aircraft is doing:
//!
//! - **Constant velocity (CV):** straight, level flight — a straight line at
//!   constant speed. This is the model [`crate::LinearKalman`] has used since
//!   M2, and the right one for the long cruising stretches.
//! - **Coordinated turn (CT):** a steady turn at a constant turn rate `ω`
//!   (rad/s) — the aircraft banks and carves a circular arc at constant speed.
//!   A single CV filter lags badly in a turn (it keeps predicting straight
//!   ahead); a CT model that knows the turn rate follows the arc.
//!
//! Both models share the **same 4-D state** `x = [east, north, v_east,
//! v_north]`, so the same filter, gate and association machinery work with
//! either — and, crucially, several of them can run in parallel and be *mixed*
//! (the IMM, Häppchen M5.2+). The CT transition reduces *exactly* to the CV
//! one as `ω → 0`, so CV is just the zero-turn-rate special case.
//!
//! Determinism (ADR 0003): a model is a pure function of `dt` and its
//! parameters; no wall clock, no hidden state.

use nalgebra::Matrix4;
use serde::{Deserialize, Serialize};

/// Below this turn rate (rad/s) a "turn" is indistinguishable from straight
/// flight, and the coordinated-turn formulas divide by a vanishing `ω`. We
/// fall back to the constant-velocity transition, which is their exact limit.
const STRAIGHT_FLIGHT_RATE: f64 = 1e-6;

/// How a target is assumed to move between scans — the source of the Kalman
/// predict step's state-transition matrix `F(dt)`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MotionModel {
    /// Straight, level flight at constant velocity (the M2 model).
    ConstantVelocity,
    /// A steady coordinated turn at a constant turn rate `rate` (rad/s),
    /// positive = anticlockwise (to the left, mathematically positive in the
    /// east/north plane).
    CoordinatedTurn { rate: f64 },
    /// Uniform acceleration (VERT.4, ADR 0035) — only meaningful on the 6-D
    /// state ([`transition6`](Self::transition6)); restricted to the 4-D
    /// state it degenerates to constant velocity (there is no acceleration
    /// state to couple).
    ConstantAcceleration,
}

impl MotionModel {
    /// The state-transition matrix `F` advancing `x = [E, N, vE, vN]` by `dt`
    /// seconds under this model.
    ///
    /// For constant velocity this is the familiar
    /// `[[1,0,dt,0],[0,1,0,dt],[0,0,1,0],[0,0,0,1]]`. For a coordinated turn at
    /// rate `ω` it rotates the velocity vector by `ω·dt` and integrates the
    /// resulting arc into the position:
    ///
    /// ```text
    /// F = [1, 0,  sin(ωdt)/ω,   -(1-cos(ωdt))/ω]
    ///     [0, 1,  (1-cos(ωdt))/ω, sin(ωdt)/ω    ]
    ///     [0, 0,  cos(ωdt),      -sin(ωdt)      ]
    ///     [0, 0,  sin(ωdt),       cos(ωdt)      ]
    /// ```
    ///
    /// As `ω → 0` the position coefficients tend to `dt` and `0` and the
    /// velocity block to the identity, recovering constant velocity exactly.
    pub fn transition(&self, dt: f64) -> Matrix4<f64> {
        match self {
            MotionModel::ConstantVelocity => constant_velocity_transition(dt),
            MotionModel::CoordinatedTurn { rate } => {
                if rate.abs() < STRAIGHT_FLIGHT_RATE {
                    constant_velocity_transition(dt)
                } else {
                    coordinated_turn_transition(*rate, dt)
                }
            }
            // On a 4-D state CA has no acceleration to couple — it IS CV.
            MotionModel::ConstantAcceleration => constant_velocity_transition(dt),
        }
    }

    /// The state-transition matrix `F` advancing the 6-D state
    /// `x = [E, N, vE, vN, aE, aN]` by `dt` seconds under this model
    /// (VERT.4b, ADR 0035 Weg A — the bank runs every model on 6-D).
    /// See [`crate::kalman6`] for the per-hypothesis acceleration semantics.
    pub fn transition6(&self, dt: f64) -> nalgebra::Matrix6<f64> {
        match self {
            MotionModel::ConstantVelocity => crate::kalman6::constant_velocity_transition6(dt),
            MotionModel::CoordinatedTurn { rate } => {
                crate::kalman6::coordinated_turn_transition6(*rate, dt)
            }
            MotionModel::ConstantAcceleration => {
                crate::kalman6::constant_acceleration_transition(dt)
            }
        }
    }
}

/// The constant-velocity state-transition matrix for a step `dt`.
fn constant_velocity_transition(dt: f64) -> Matrix4<f64> {
    Matrix4::new(
        1.0, 0.0, dt, 0.0, //
        0.0, 1.0, 0.0, dt, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    )
}

/// The coordinated-turn state-transition matrix for turn rate `rate` (rad/s,
/// nonzero) over a step `dt`.
fn coordinated_turn_transition(rate: f64, dt: f64) -> Matrix4<f64> {
    let theta = rate * dt;
    let (sin, cos) = theta.sin_cos();
    let s_over_w = sin / rate; // → dt as rate → 0
    let one_minus_c_over_w = (1.0 - cos) / rate; // → 0  as rate → 0
    Matrix4::new(
        1.0,
        0.0,
        s_over_w,
        -one_minus_c_over_w, //
        0.0,
        1.0,
        one_minus_c_over_w,
        s_over_w, //
        0.0,
        0.0,
        cos,
        -sin, //
        0.0,
        0.0,
        sin,
        cos,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Vector4;

    /// A zero turn rate gives exactly the constant-velocity transition.
    /// REQ: FR-TRK-011
    #[test]
    fn zero_turn_rate_is_constant_velocity() {
        let dt = 4.0;
        let ct = MotionModel::CoordinatedTurn { rate: 0.0 }.transition(dt);
        let cv = MotionModel::ConstantVelocity.transition(dt);
        assert_eq!(ct, cv);
    }

    /// A tiny turn rate stays numerically close to constant velocity (the
    /// formulas do not blow up near `ω = 0`).
    /// REQ: FR-TRK-011
    #[test]
    fn small_turn_rate_is_close_to_constant_velocity() {
        let dt = 4.0;
        let ct = MotionModel::CoordinatedTurn { rate: 1e-7 }.transition(dt);
        let cv = MotionModel::ConstantVelocity.transition(dt);
        for i in 0..4 {
            for j in 0..4 {
                assert!((ct[(i, j)] - cv[(i, j)]).abs() < 1e-9);
            }
        }
    }

    /// A coordinated turn rotates the velocity vector by exactly `ω·dt` and
    /// preserves its speed: a target heading due east, turning left at `ω`, is
    /// heading due north after a quarter-circle (`ω·dt = π/2`).
    /// REQ: FR-TRK-011
    #[test]
    fn quarter_turn_rotates_velocity_by_ninety_degrees() {
        let speed = 200.0;
        let rate = 0.05; // rad/s, anticlockwise
        let dt = std::f64::consts::FRAC_PI_2 / rate; // ω·dt = π/2
        let f = MotionModel::CoordinatedTurn { rate }.transition(dt);
        // Start at the origin heading due east.
        let x0 = Vector4::new(0.0, 0.0, speed, 0.0);
        let x1 = f * x0;
        // Velocity now points due north, same speed.
        assert!(x1[2].abs() < 1e-9, "v_east ≈ 0 after a quarter turn");
        assert!((x1[3] - speed).abs() < 1e-9, "v_north ≈ speed");
        // Position sits on the quarter-circle arc of radius v/ω, centred due
        // north of the start: (E, N) = (v/ω, v/ω).
        let radius = speed / rate;
        assert!((x1[0] - radius).abs() < 1e-6, "east = v/ω");
        assert!((x1[1] - radius).abs() < 1e-6, "north = v/ω");
    }

    /// Speed is conserved through a coordinated turn for any angle.
    /// REQ: FR-TRK-011
    #[test]
    fn coordinated_turn_conserves_speed() {
        let rate = -0.03; // clockwise
        let dt = 7.0;
        let f = MotionModel::CoordinatedTurn { rate }.transition(dt);
        let x0 = Vector4::new(100.0, -50.0, 120.0, 90.0);
        let speed0 = f64::hypot(x0[2], x0[3]);
        let x1 = f * x0;
        let speed1 = f64::hypot(x1[2], x1[3]);
        assert!(
            (speed1 - speed0).abs() < 1e-9,
            "a turn does not change speed"
        );
    }
}
