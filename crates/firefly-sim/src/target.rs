//! Ground-truth targets and their kinematics.
//!
//! A target flies through the scenario's local ENU frame following a sequence
//! of *legs*. Each leg holds a turn rate, a longitudinal acceleration and a
//! vertical speed held constant for its duration, which together cover straight
//! cruise, coordinated turns, accelerations and climbs/descents — enough to
//! exercise a tracker's motion models without a full flight-dynamics engine.

use firefly_core::{Callsign, TargetId};
use firefly_geo::Enu;

/// A constant-control segment of a target's trajectory.
#[derive(Debug, Clone, Copy)]
pub struct Leg {
    /// How long this leg lasts, seconds.
    pub duration: f64,
    /// Turn rate, radians per second. Positive turns clockwise (to the right),
    /// matching the azimuth (north-clockwise) convention.
    pub turn_rate: f64,
    /// Longitudinal acceleration along the velocity vector, m/s².
    pub accel: f64,
    /// Vertical speed (climb positive), m/s.
    pub climb_rate: f64,
}

impl Leg {
    /// Straight, level, constant-speed flight for `duration` seconds.
    pub fn cruise(duration: f64) -> Self {
        Self {
            duration,
            turn_rate: 0.0,
            accel: 0.0,
            climb_rate: 0.0,
        }
    }

    /// A coordinated turn at `turn_rate_deg_s` degrees per second.
    pub fn turn(duration: f64, turn_rate_deg_s: f64) -> Self {
        Self {
            duration,
            turn_rate: turn_rate_deg_s.to_radians(),
            accel: 0.0,
            climb_rate: 0.0,
        }
    }

    /// Level acceleration (positive) or deceleration (negative).
    pub fn accelerate(duration: f64, accel: f64) -> Self {
        Self {
            duration,
            turn_rate: 0.0,
            accel,
            climb_rate: 0.0,
        }
    }

    /// Straight climb (positive) or descent (negative) at a constant rate.
    pub fn climb(duration: f64, climb_rate: f64) -> Self {
        Self {
            duration,
            turn_rate: 0.0,
            accel: 0.0,
            climb_rate,
        }
    }

    pub fn with_climb(mut self, climb_rate: f64) -> Self {
        self.climb_rate = climb_rate;
        self
    }
}

/// The instantaneous kinematic state of a target in the scenario frame.
#[derive(Debug, Clone, Copy)]
pub struct State {
    /// Position in the scenario ENU frame, metres.
    pub position: Enu,
    /// Ground speed, m/s.
    pub speed: f64,
    /// Heading, radians clockwise from true north.
    pub heading: f64,
    /// Vertical speed (climb positive), m/s.
    pub climb_rate: f64,
}

impl State {
    /// Horizontal velocity components (east, north), m/s.
    pub fn horizontal_velocity(&self) -> (f64, f64) {
        let (sin_h, cos_h) = self.heading.sin_cos();
        (self.speed * sin_h, self.speed * cos_h)
    }
}

/// A ground-truth target: an identity, a flight plan, and (optionally) the SSR
/// equipment it carries.
#[derive(Debug, Clone)]
pub struct Target {
    pub id: TargetId,
    /// Initial kinematic state at scenario time zero.
    pub initial: State,
    /// The legs flown, in order.
    pub legs: Vec<Leg>,
    /// Mode 3/A squawk, if the aircraft has an SSR transponder.
    pub mode_3a: Option<u16>,
    /// Mode S 24-bit ICAO address, if equipped.
    pub icao_address: Option<u32>,
    /// Callsign / flight ID reported via Mode S, if equipped.
    pub callsign: Option<Callsign>,
}

impl Target {
    /// Advance a state by `dt` seconds under the controls of one leg.
    ///
    /// Uses an exact constant-turn-rate update for the heading and a midpoint
    /// average for the ground track, which keeps curved legs accurate even at
    /// the coarse internal step used by the simulator.
    pub fn step(state: &State, leg: &Leg, dt: f64) -> State {
        let new_speed = (state.speed + leg.accel * dt).max(0.0);
        let new_heading = wrap_angle(state.heading + leg.turn_rate * dt);

        // Average speed and heading over the step for the position increment.
        let avg_speed = 0.5 * (state.speed + new_speed);
        let avg_heading = state.heading + 0.5 * leg.turn_rate * dt;
        let (sin_h, cos_h) = avg_heading.sin_cos();

        let position = Enu {
            east: state.position.east + avg_speed * sin_h * dt,
            north: state.position.north + avg_speed * cos_h * dt,
            up: state.position.up + leg.climb_rate * dt,
        };

        State {
            position,
            speed: new_speed,
            heading: new_heading,
            climb_rate: leg.climb_rate,
        }
    }

    /// Total scripted flight time across all legs, seconds.
    pub fn scripted_duration(&self) -> f64 {
        self.legs.iter().map(|l| l.duration).sum()
    }
}

/// Wrap an angle into [0, 2π).
pub fn wrap_angle(mut a: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    a %= tau;
    if a < 0.0 {
        a += tau;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(speed: f64, heading_deg: f64) -> State {
        State {
            position: Enu::new(0.0, 0.0, 1000.0),
            speed,
            heading: heading_deg.to_radians(),
            climb_rate: 0.0,
        }
    }

    #[test]
    fn cruise_north_moves_north() {
        let s = state(100.0, 0.0);
        let next = Target::step(&s, &Leg::cruise(1.0), 1.0);
        assert!((next.position.north - 100.0).abs() < 1e-9);
        assert!(next.position.east.abs() < 1e-9);
    }

    #[test]
    fn cruise_east_moves_east() {
        let s = state(100.0, 90.0);
        let next = Target::step(&s, &Leg::cruise(1.0), 1.0);
        assert!((next.position.east - 100.0).abs() < 1e-9);
        assert!(next.position.north.abs() < 1e-9);
    }

    #[test]
    fn climb_increases_altitude() {
        let s = state(100.0, 0.0);
        let next = Target::step(&s, &Leg::climb(10.0, 5.0), 10.0);
        assert!((next.position.up - 1050.0).abs() < 1e-9);
    }

    #[test]
    fn quarter_turn_changes_heading_90_degrees() {
        // 3 deg/s for 30 s = 90 deg turn from north to east.
        let mut s = state(100.0, 0.0);
        let leg = Leg::turn(30.0, 3.0);
        for _ in 0..300 {
            s = Target::step(&s, &leg, 0.1);
        }
        assert!(
            (s.heading.to_degrees() - 90.0).abs() < 1e-6,
            "heading {}",
            s.heading.to_degrees()
        );
    }

    #[test]
    fn acceleration_increases_speed() {
        let s = state(100.0, 0.0);
        let next = Target::step(&s, &Leg::accelerate(10.0, 2.0), 10.0);
        assert!((next.speed - 120.0).abs() < 1e-9);
    }
}
