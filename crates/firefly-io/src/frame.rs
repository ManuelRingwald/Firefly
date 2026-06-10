//! The neutral output frame and its JSON wire form.
//!
//! REQ: FR-IO-001

use serde::{Deserialize, Serialize};

use firefly_core::{SensorId, SystemTrack, Timestamp, TrackId};

/// One track in the wire form, in web-map-friendly units.
///
/// Built from a [`SystemTrack`] (see [`FrameTrack::from_system_track`]): position
/// in **degrees** (the map speaks degrees, the core speaks radians), ground speed
/// and course **derived** from the velocity components, and the safety-relevant
/// status (`confirmed`, `coasting`, `update_age_s`, `position_uncertainty_m`)
/// carried through verbatim — the tracker decides, the consumer only renders
/// (ADR 0008).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrameTrack {
    /// Stable track identity. Serialises as a bare number.
    pub id: TrackId,
    /// Latitude in decimal degrees (positive north).
    pub lat_deg: f64,
    /// Longitude in decimal degrees (positive east).
    pub lon_deg: f64,
    /// Height above the WGS84 ellipsoid, metres.
    pub height_m: f64,
    /// Horizontal ground speed, m/s.
    pub ground_speed_mps: f64,
    /// Course over ground, degrees clockwise from true north in `[0, 360)`.
    pub track_angle_deg: f64,
    /// Confirmed (vs. still tentative).
    pub confirmed: bool,
    /// Currently coasting (extrapolated, no fresh measurement this scan).
    pub coasting: bool,
    /// Data-time since the last real measurement, seconds.
    pub update_age_s: f64,
    /// 1σ semi-major axis of the position error ellipse, metres.
    pub position_uncertainty_m: f64,
}

impl FrameTrack {
    /// Project a neutral [`SystemTrack`] into the web-friendly wire form.
    pub fn from_system_track(track: &SystemTrack) -> Self {
        Self {
            id: track.id,
            lat_deg: track.position.lat_deg(),
            lon_deg: track.position.lon_deg(),
            height_m: track.position.height,
            ground_speed_mps: track.ground_speed(),
            track_angle_deg: track.track_angle().to_degrees(),
            confirmed: track.confirmed,
            coasting: track.coasting,
            update_age_s: track.update_age,
            position_uncertainty_m: track.position_uncertainty,
        }
    }
}

/// A complete picture of the air situation at one data time, ready to send out.
///
/// This is the unit the M3 server will push over a WebSocket: the data time, the
/// reporting sensor, and the tracks at that time. It is built from the tracker's
/// neutral output and serialises losslessly to and from JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// Data time of this picture (seconds; ASTERIX time-of-day later).
    pub time: Timestamp,
    /// The reporting sensor. Single-radar for M2/M3; multi-radar provenance
    /// generalises in M4.
    pub sensor: SensorId,
    /// The tracks at this data time, in wire form.
    pub tracks: Vec<FrameTrack>,
}

impl Frame {
    /// Bundle the tracks of one scan into a frame, converting each to wire form.
    pub fn new(time: Timestamp, sensor: SensorId, tracks: &[SystemTrack]) -> Self {
        Self {
            time,
            sensor,
            tracks: tracks.iter().map(FrameTrack::from_system_track).collect(),
        }
    }

    /// Serialise to a compact JSON string (one line, for the wire).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialise to indented JSON (for logs and human inspection).
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Parse a frame back from JSON (used in tests and by any consumer).
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_geo::Wgs84;

    /// A system track at a known geodetic position, moving due east at 150 m/s,
    /// confirmed and fresh.
    fn sample_track(id: u32) -> SystemTrack {
        SystemTrack {
            id: TrackId(id),
            time: Timestamp(12.0),
            position: Wgs84::from_degrees(47.5, 8.25, 500.0),
            v_east: 150.0,
            v_north: 0.0,
            confirmed: true,
            coasting: false,
            update_age: 0.0,
            position_uncertainty: 42.0,
            mode_3a: None,
            icao_address: None,
            contributing_sensors: Vec::new(),
        }
    }

    /// The wire form presents degrees and derived kinematics, not the core's
    /// internal radians/components. REQ: FR-IO-001
    #[test]
    fn wire_form_uses_degrees_and_derived_kinematics() {
        let wire = FrameTrack::from_system_track(&sample_track(7));

        assert!((wire.lat_deg - 47.5).abs() < 1e-9, "latitude in degrees");
        assert!((wire.lon_deg - 8.25).abs() < 1e-9, "longitude in degrees");
        assert!((wire.height_m - 500.0).abs() < 1e-9);
        assert!(
            (wire.ground_speed_mps - 150.0).abs() < 1e-9,
            "speed is the velocity vector length"
        );
        // Due east is π/2 rad == 90° clockwise from north.
        assert!(
            (wire.track_angle_deg - 90.0).abs() < 1e-9,
            "course over ground in degrees"
        );
        // Status carried through verbatim.
        assert!(wire.confirmed && !wire.coasting);
        assert!((wire.update_age_s).abs() < 1e-12);
        assert!((wire.position_uncertainty_m - 42.0).abs() < 1e-12);
    }

    /// A frame survives a JSON round-trip unchanged. REQ: FR-IO-001
    #[test]
    fn frame_round_trips_through_json() {
        let tracks = [sample_track(1), sample_track(2)];
        let frame = Frame::new(Timestamp(12.0), SensorId(1), &tracks);

        let json = frame.to_json().expect("serialise");
        let back = Frame::from_json(&json).expect("deserialise");

        assert_eq!(frame, back, "round-trip is lossless");
    }

    /// The JSON is flat and self-describing: the field names a consumer relies on
    /// are present, and the newtypes serialise as bare numbers. REQ: FR-IO-001
    #[test]
    fn json_is_self_describing() {
        let frame = Frame::new(Timestamp(12.0), SensorId(3), &[sample_track(5)]);
        let json = frame.to_json().expect("serialise");

        for key in [
            "\"time\"",
            "\"sensor\"",
            "\"tracks\"",
            "\"id\"",
            "\"lat_deg\"",
            "\"lon_deg\"",
            "\"coasting\"",
            "\"position_uncertainty_m\"",
        ] {
            assert!(json.contains(key), "JSON should contain {key}: {json}");
        }
        // Newtypes serialise as bare scalars, not nested objects.
        assert!(
            json.contains("\"sensor\":3"),
            "sensor is a bare number: {json}"
        );
        assert!(
            json.contains("\"id\":5"),
            "track id is a bare number: {json}"
        );
    }

    /// An empty scan yields a valid frame with no tracks. REQ: FR-IO-001
    #[test]
    fn empty_frame_has_no_tracks() {
        let frame = Frame::new(Timestamp(0.0), SensorId(1), &[]);
        let json = frame.to_json().expect("serialise");
        let back = Frame::from_json(&json).expect("deserialise");
        assert!(back.tracks.is_empty());
    }
}
