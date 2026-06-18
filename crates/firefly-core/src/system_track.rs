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

use crate::ids::{SensorId, TrackId};
use crate::plot::Callsign;
use crate::time::Timestamp;
use firefly_geo::Wgs84;

/// One track as reported to the outside world, in geodetic coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Maps to the CNF bit of ASTERIX CAT062 I062/080.
    pub confirmed: bool,
    /// Whether the track is currently *coasting* — extrapolated from prediction
    /// because it had no fresh measurement this scan. Maps to the CST bit of
    /// CAT062 I062/080. A track may be both `confirmed` and `coasting`.
    pub coasting: bool,
    /// Whether this is the **final** report for the track — it has just been
    /// deleted from the tracker's live set, and this record carries its last
    /// known state once so a consumer can remove it deterministically instead
    /// of waiting for a timeout. Maps to the TSE (*Track Service End*) bit of
    /// CAT062 I062/080 (ADR 0016, FR-TRK-029). `false` for every live track;
    /// `true` appears exactly once, in the heartbeat following deletion.
    pub ended: bool,
    /// Update age: data-time since the last real measurement, seconds. `0` right
    /// after a hit, growing while coasting. Maps to CAT062 I062/290 (track ages).
    pub update_age: f64,
    /// Position uncertainty: the 1σ semi-major axis of the error ellipse, metres
    /// — the tracker's honest "how sure am I about this position right now".
    /// Maps to CAT062 I062/500 (estimated accuracies).
    pub position_uncertainty: f64,
    /// Most recently reported Mode 3/A code ("squawk"), if any SSR-equipped
    /// plot has ever associated with this track. `None` for a primary-only
    /// track. Encoded as CAT062 I062/060 when present (FR-TRK-009).
    pub mode_3a: Option<u16>,
    /// Most recently reported Mode S 24-bit ICAO aircraft address, if any
    /// SSR-equipped plot has ever associated with this track. `None` for a
    /// primary-only track. Encoded as the Target Address (ADR) subfield of
    /// CAT062 I062/380 when present, and is the eventual correlation key for
    /// multi-radar fusion.
    pub icao_address: Option<u32>,
    /// Most recently reported barometric flight level, in **feet** (Mode C
    /// pressure altitude, 1013.25 hPa datum), if any SSR-equipped plot has ever
    /// associated with this track. `None` for a primary-only track. Like the
    /// identity fields it is sticky — a primary-only detection does not clear
    /// the last known level. Encoded as CAT062 I062/136 (Measured Flight Level)
    /// when present. The tracker carries no independent vertical *estimate* yet
    /// (no vertical Kalman state); this is the last measured value, passed
    /// through (FR-TRK-027).
    pub flight_level_ft: Option<f64>,
    /// Most recently reported callsign / flight ID (Mode S target
    /// identification), if any SSR-equipped plot has ever associated with this
    /// track. `None` for a primary-only track. Sticky, like `mode_3a`: a
    /// primary-only detection does not clear the last known callsign. Encoded
    /// as CAT062 I062/245 (Target Identification) when present, passed through
    /// from the Mode S downlink reply (FR-TRK-028).
    pub callsign: Option<Callsign>,
    /// Sensors that contributed a hit to this track in the **most recent
    /// scan** (ADR 0010, central measurement fusion). Empty while coasting —
    /// no sensor saw it this scan. Sorted by [`SensorId`] for determinism.
    /// Replaces the single-sensor `update_age` simplification for CAT062
    /// I062/290 (track ages are reported per contributing sensor).
    pub contributing_sensors: Vec<SensorId>,
    /// Data-time elapsed since the last ADS-B (Extended Squitter) hit, seconds.
    /// `None` if the track has never been updated by an ADS-B measurement.
    /// When `Some`, drives the ES-Age subfield of CAT062 I062/290 (ICD 2.4.0,
    /// AP9.5/AP9.6); its presence signals "this track has an ADS-B contribution"
    /// to Wayfinder.
    pub adsb_age_s: Option<f64>,
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
            coasting: false,
            ended: false,
            update_age: 0.0,
            position_uncertainty: 0.0,
            mode_3a: None,
            icao_address: None,
            flight_level_ft: None,
            callsign: None,
            contributing_sensors: Vec::new(),
            adsb_age_s: None,
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
