//! The "Player": wires the simulator (M1), the tracker (M2) and the JSON
//! output adapter (M3 Häppchen 3.1) into one **frame stream**.
//!
//! ```text
//! Scenario --(firefly_sim::run)--> Plot stream --(Tracker::process_scan)-->
//!     tracks --(Tracker::system_tracks)--> SystemTrack --(Frame::new)--> Frame
//! ```
//!
//! [`Player::frames`] turns a [`Scenario`] into the complete, ordered
//! [`Frame`] stream — one frame per scan time. This is the **pure core** of
//! M3: it contains no networking, no clock and no pacing. The M3 server
//! (Häppchen 3.3) wraps this stream and decides *how fast* to send it; that
//! separation is what later lets the demo (3.5) replay the very same
//! deterministic stream at any speed without changing a single tracker
//! decision (ADR 0003, NFR-CLOUD-002).
//!
//! ## Determinism
//!
//! [`Player::frames`] is a pure function of the scenario (and its seed,
//! NFR-REPRO-001): same input, same `Vec<Frame>`, byte for byte. There is no
//! wall-clock dependency anywhere in this crate.

use firefly_core::{Plot, SensorId};
use firefly_geo::LocalFrame;
use firefly_io::Frame;
use firefly_sim::Scenario;
use firefly_track::{Tracker, TrackerConfig};

/// Runs a [`Scenario`] through a [`Tracker`] and produces the [`Frame`] stream.
pub struct Player {
    frame: LocalFrame,
    sensor: SensorId,
    plots: Vec<Plot>,
    tracker: Tracker,
}

impl Player {
    /// Set up a player for `scenario`, simulating its plot stream up front.
    ///
    /// The reported [`SensorId`] is that of the scenario's first radar — M3
    /// targets a single radar (NFR-INT-002 already documents the multi-sensor
    /// generalisation as M4 work). A scenario without a radar yields an empty
    /// frame stream.
    pub fn new(scenario: &Scenario, config: TrackerConfig) -> Self {
        let sensor = scenario
            .radars()
            .first()
            .map(|radar| radar.sensor.id)
            .unwrap_or(SensorId(0));

        Self {
            frame: *scenario.frame(),
            sensor,
            plots: firefly_sim::run(scenario),
            tracker: Tracker::new(config),
        }
    }

    /// Run the whole scenario, returning one [`Frame`] per scan time.
    ///
    /// Plots that share a scan time are batched into a single
    /// [`Tracker::process_scan`] call, exactly as a live feed would deliver
    /// them; the resulting tracks are then projected to a [`Frame`]. Scan
    /// times with no plots at all (every target out of range or undetected)
    /// produce no frame — there is nothing new to report.
    pub fn frames(mut self) -> Vec<Frame> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.plots.len() {
            let time = self.plots[i].time;
            let start = i;
            while i < self.plots.len() && self.plots[i].time == time {
                i += 1;
            }
            self.tracker.process_scan(time, &self.plots[start..i]);
            let tracks = self.tracker.system_tracks(&self.frame);
            out.push(Frame::new(time, self.sensor, &tracks));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::{Sensor, SensorId as Sid, TargetId};
    use firefly_geo::{Enu, Wgs84};
    use firefly_sim::{Leg, Radar, RadarParams, State, Target};
    use firefly_track::SensorErrorModel;

    fn config() -> TrackerConfig {
        TrackerConfig::new(SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08))
    }

    fn northbound_target() -> Target {
        Target {
            id: TargetId(1),
            initial: State {
                position: Enu::new(0.0, 50_000.0, 3000.0),
                speed: 200.0,
                heading: 0.0,
                climb_rate: 0.0,
            },
            legs: vec![Leg::cruise(60.0)],
            mode_3a: Some(0o7000),
            icao_address: None,
        }
    }

    fn perfect_radar() -> Radar {
        let sensor = Sensor::new(Sid(1), Wgs84::from_degrees(48.0, 11.0, 0.0));
        Radar::new(
            sensor,
            RadarParams {
                scan_period: 4.0,
                prob_detection: 1.0,
                sigma_range: 0.0,
                sigma_azimuth: 0.0,
                sigma_elevation: 0.0,
                ..RadarParams::default()
            },
        )
    }

    fn scenario() -> Scenario {
        Scenario::new(Wgs84::from_degrees(48.0, 11.0, 0.0))
            .with_duration(40.0)
            .add_radar(perfect_radar())
            .add_target(northbound_target())
    }

    /// One frame is emitted per scan time, in time order, carrying the
    /// scenario's sensor id. REQ: FR-IO-002
    #[test]
    fn one_frame_per_scan_time_in_order() {
        let frames = Player::new(&scenario(), config()).frames();

        // 40 s / 4 s scan period = 11 scans (t = 0, 4, ..., 40).
        assert_eq!(frames.len(), 11);
        for w in frames.windows(2) {
            assert!(w[0].time.as_secs() < w[1].time.as_secs());
        }
        assert!(frames.iter().all(|f| f.sensor == SensorId(1)));
    }

    /// A target stays visible long enough to confirm; the confirmed flag
    /// shows up in the frame stream once the lifecycle reaches it. REQ:
    /// FR-IO-002
    #[test]
    fn confirmed_track_appears_in_frame_stream() {
        let frames = Player::new(&scenario(), config()).frames();

        let confirmed_somewhere = frames.iter().any(|f| f.tracks.iter().any(|t| t.confirmed));
        assert!(confirmed_somewhere, "track should confirm within 11 scans");
    }

    /// The frame stream is a pure function of the scenario: running it twice
    /// yields byte-identical output. REQ: NFR-REPRO-001, NFR-CLOUD-002
    #[test]
    fn frame_stream_is_deterministic() {
        let a = Player::new(&scenario(), config()).frames();
        let b = Player::new(&scenario(), config()).frames();
        assert_eq!(a, b);
    }

    /// A scenario without a radar produces no plots and thus no frames.
    #[test]
    fn scenario_without_radar_yields_no_frames() {
        let scenario = Scenario::new(Wgs84::from_degrees(48.0, 11.0, 0.0))
            .with_duration(40.0)
            .add_target(northbound_target());
        let frames = Player::new(&scenario, config()).frames();
        assert!(frames.is_empty());
    }
}
