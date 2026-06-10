//! The neutral, geodetic output of the tracker: a `SystemTrack`.
//!
//! Internally the tracker reasons in a flat, sensor-local ENU frame (east/north
//! in metres) because there motion is almost linear and the Kalman maths is
//! cheap. But the *outside world* — an air-situation display, and later the
//! Phoenix ASD via ASTERIX CAT062 — speaks **geodetic WGS84** (latitude,
//! longitude). The `SystemTrack` is the boundary type that carries an estimated
//! track out of the core in world coordinates.
//!
//! It is deliberately *format- and transport-neutral* (Ports & Adapters,
//! NFR-INT-001/002): the core produces `SystemTrack`s; a downstream **adapter**
//! turns them into CAT062, JSON for the web map, or anything else. No transport
//! or wire format leaks into the core.
//!
//! Note on velocity: the components stay in the local ENU frame (`v_east`,
//! `v_north`, m/s). Over the short ranges of a single radar the local east/north
//! axes are an excellent approximation of the geographic ones, and ground speed
//! and heading derive directly from them — see [`SystemTrack::ground_speed`] and
//! [`SystemTrack::track_angle`].

use serde::{Deserialize, Serialize};

use crate::ids::TrackId;
use crate::time::Timestamp;
use firefly_geo::Wgs84;

/// One track as reported to the outside world, in geodetic coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SystemTrack {
    /// Stable identity of the track.
    pub id: TrackId,
    /// Data time of the estimate (the last scan that touched this track).
    pub time: Timestamp,
    /// Estimated position in geodetic WGS84.
    pub position: Wgs84,
    /// Eastward velocity component in the sensor-local frame, m/s.
    pub v_east: f64,
    /// Northward velocity component in the sensor-local frame, m/s.
    pub v_north: f64,
    /// Whether the track is confirmed (vs. still tentative). The neutral port
    /// reports both and leaves the publish/suppress policy to the adapter.
    pub confirmed: bool,
}

impl SystemTrack {
    /// Horizontal ground speed, m/s — the length of the velocity vector.
    pub fn ground_speed(&self) -> f64 {
        self.v_east.hypot(self.v_north)
    }

    /// Track angle (course over ground): the direction of motion measured
    /// **clockwise from true north**, radians in `[0, 2π)`. This matches the
    /// azimuth convention used throughout the geo crate.
    ///
    /// For a target heading due east the angle is `π/2`; due north it is `0`.
    /// When the track is (numerically) stationary the angle is defined as `0`.
    pub fn track_angle(&self) -> f64 {
        if self.v_east == 0.0 && self.v_north == 0.0 {
            return 0.0;
        }
        let mut angle = self.v_east.atan2(self.v_north);
        if angle < 0.0 {
            angle += std::f64::consts::TAU;
        }
        angle
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    fn track(v_east: f64, v_north: f64) -> SystemTrack {
        SystemTrack {
            id: TrackId(1),
            time: Timestamp(0.0),
            position: Wgs84::from_degrees(47.0, 8.0, 500.0),
            v_east,
            v_north,
            confirmed: true,
        }
    }

    /// Ground speed is the Euclidean length of the velocity vector.
    /// REQ: NFR-INT-002
    #[test]
    fn ground_speed_is_vector_length() {
        assert!((track(3.0, 4.0).ground_speed() - 5.0).abs() < 1e-12);
    }

    /// Track angle follows the compass convention: clockwise from true north.
    /// REQ: NFR-INT-002
    #[test]
    fn track_angle_follows_compass_convention() {
        assert!((track(0.0, 100.0).track_angle()).abs() < 1e-12, "north → 0");
        assert!(
            (track(100.0, 0.0).track_angle() - FRAC_PI_2).abs() < 1e-12,
            "east → π/2"
        );
        assert!(
            (track(0.0, -100.0).track_angle() - PI).abs() < 1e-12,
            "south → π"
        );
        assert!(
            (track(-100.0, 0.0).track_angle() - (TAU - FRAC_PI_2)).abs() < 1e-12,
            "west → 3π/2"
        );
    }

    /// A (numerically) stationary track has a defined angle of zero.
    #[test]
    fn stationary_track_angle_is_zero() {
        assert_eq!(track(0.0, 0.0).track_angle(), 0.0);
    }
}
