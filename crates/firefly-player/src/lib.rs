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

use firefly_core::{Plot, SensorId, SystemTrack, Timestamp};
use firefly_io::{Frame, FramePlot};
use firefly_sim::Scenario;
use firefly_track::{Tracker, TrackerConfig};

/// Runs a [`Scenario`] through a [`Tracker`] and produces the [`Frame`] stream.
pub struct Player {
    sensor: SensorId,
    plots: Vec<Plot>,
    tracker: Tracker,
    /// Default period (seconds) for the **decoupled periodic output**
    /// (ADR 0013, Häppchen 13.4): the smallest radar scan period in the
    /// scenario, so the heartbeat is at least as fast as the quickest sensor.
    default_output_period: f64,
}

impl Player {
    /// Set up a player for `scenario`, simulating its plot stream up front.
    ///
    /// The tracker's `config` carries the common tracking frame and the
    /// per-sensor geometry (ADR 0010); the player just feeds it the simulated
    /// plots. The reported [`SensorId`] is that of the scenario's first radar —
    /// a placeholder until multi-sensor provenance lands (Häppchen 4.A.4). A
    /// scenario without a radar yields an empty frame stream.
    pub fn new(scenario: &Scenario, config: TrackerConfig) -> Self {
        let sensor = scenario
            .radars()
            .first()
            .map(|radar| radar.sensor.id)
            .unwrap_or(SensorId(0));

        // The default output heartbeat is the fastest sensor's scan period; with
        // no radar it falls back to a plausible 4 s so the value is always sane.
        let default_output_period = scenario
            .radars()
            .iter()
            .map(|radar| radar.params.scan_period)
            .fold(f64::INFINITY, f64::min);
        let default_output_period = if default_output_period.is_finite() {
            default_output_period
        } else {
            4.0
        };

        Self {
            sensor,
            plots: firefly_sim::run(scenario),
            tracker: Tracker::new(config),
            default_output_period,
        }
    }

    /// Run the whole scenario, returning the raw [`SystemTrack`]s per scan time.
    ///
    /// Plots that share a scan time are batched into a single
    /// [`Tracker::process_scan`] call, exactly as a live feed would deliver
    /// them; the resulting tracks are returned **unconverted** — neutral
    /// `SystemTrack`s, not yet shaped for any wire format. Scan times with no
    /// plots at all (every target out of range or undetected) produce no entry.
    ///
    /// This is the shared, deterministic core that every output adapter builds
    /// on: the JSON [`frames`](Player::frames) for the web map and the CAT062
    /// encoder for the multicast feed each translate **these same**
    /// `SystemTrack`s independently, neither depending on the other (ADR 0006,
    /// Nachtrag M3.X.4 — adapters stay independent).
    pub fn scans(mut self) -> Vec<(Timestamp, Vec<SystemTrack>)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.plots.len() {
            let time = self.plots[i].time;
            let start = i;
            while i < self.plots.len() && self.plots[i].time == time {
                i += 1;
            }
            self.tracker.process_scan(time, &self.plots[start..i]);
            out.push((time, self.tracker.system_tracks()));
        }
        out
    }

    /// Run the whole scenario, returning one [`Frame`] per scan time.
    ///
    /// A thin JSON-adapter projection over [`scans`](Player::scans): each scan's
    /// raw plots and `SystemTrack`s are bundled into a [`Frame`] (web-friendly
    /// wire shape, M6.3). Each plot's polar measurement is lifted into WGS84
    /// via its sensor's [`LocalFrame`](firefly_geo::LocalFrame) (FR-IO-001); a
    /// plot whose sensor is not registered in the tracker's config is dropped
    /// (it could not be geolocated and would not be tracked either).
    pub fn frames(mut self) -> Vec<Frame> {
        let sensor = self.sensor;
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.plots.len() {
            let time = self.plots[i].time;
            let start = i;
            while i < self.plots.len() && self.plots[i].time == time {
                i += 1;
            }
            let frame_plots: Vec<FramePlot> = self.plots[start..i]
                .iter()
                .filter_map(|p| {
                    let sensor_model = self.tracker.config().sensors.get(&p.sensor)?;
                    let position = sensor_model.frame.enu_to_geodetic(&p.measurement.to_enu());
                    Some(FramePlot::from_plot(
                        position.lat_deg(),
                        position.lon_deg(),
                        p.kind.has_secondary(),
                    ))
                })
                .collect();
            self.tracker.process_scan(time, &self.plots[start..i]);
            out.push(Frame::new(
                time,
                sensor,
                &frame_plots,
                &self.tracker.system_tracks(),
            ));
        }
        out
    }

    /// The default output heartbeat period (seconds): the fastest radar's scan
    /// period (ADR 0013, Häppchen 13.4). The caller (server) may override it via
    /// configuration (`FIREFLY_OUTPUT_PERIOD`) at the cut-over (Häppchen 13.7).
    pub fn default_output_period(&self) -> f64 {
        self.default_output_period
    }

    /// Run the whole scenario on the **asynchronous** path and report the air
    /// picture on a **fixed output heartbeat** of `t_out` seconds (ADR 0013,
    /// Häppchen 13.4) — the periodic counterpart to [`scans`](Player::scans).
    ///
    /// Each output window's plots are fed in data-time order through
    /// [`Tracker::process_plots`], which groups genuinely coincident plots into
    /// joint measurement opportunities (ADR 0011 ghost suppression + JPDA
    /// exclusivity, Häppchen 13.5a) while keeping time-separated plots as their
    /// own opportunities; at each tick `k · t_out` the tracks are reported via
    /// [`Tracker::snapshot_at`] (a read-only projection to that instant). The
    /// result is a regular, predictable stream independent of the irregular
    /// multi-radar input cadence — what a real SDPS emits — while staying a
    /// pure, deterministic function of the scenario (NFR-CLOUD-002,
    /// NFR-REPRO-001). Ticks with no fresh plots still produce a snapshot
    /// (coasting tracks), which is exactly the stable heartbeat the ASD wants.
    ///
    /// This is the shared periodic core both output adapters build on; the
    /// server/CAT062 cut-over to it is Häppchen 13.7.
    pub fn periodic_snapshots(mut self, t_out: f64) -> Vec<(Timestamp, Vec<SystemTrack>)> {
        let mut out = Vec::new();
        if t_out <= 0.0 || self.plots.is_empty() {
            return out;
        }
        let horizon = self.plots.last().unwrap().time.as_secs();
        let mut i = 0;
        let mut tick = t_out;
        while tick <= horizon + 1e-9 {
            // Feed every plot up to this tick as one batch, so coincident plots
            // are associated jointly (Häppchen 13.5a).
            let start = i;
            while i < self.plots.len() && self.plots[i].time.as_secs() <= tick + 1e-9 {
                i += 1;
            }
            self.tracker.process_plots(&self.plots[start..i]);
            let ts = Timestamp(tick);
            out.push((ts, self.tracker.snapshot_at(ts)));
            tick += t_out;
        }
        out
    }

    /// Periodic [`Frame`] stream: the JSON-adapter projection over
    /// [`periodic_snapshots`](Player::periodic_snapshots) (ADR 0013, Häppchen
    /// 13.4). Each tick's [`Frame`] carries the track snapshot at that instant
    /// plus the **raw plots that arrived in this output window** (since the
    /// previous tick), lifted to WGS84 — keeping the web view's plot overlay
    /// meaningful at the fixed heartbeat. A plot whose sensor is unregistered is
    /// dropped (it could not be geolocated or tracked).
    pub fn periodic_frames(mut self, t_out: f64) -> Vec<Frame> {
        let sensor = self.sensor;
        let mut out = Vec::new();
        if t_out <= 0.0 || self.plots.is_empty() {
            return out;
        }
        let horizon = self.plots.last().unwrap().time.as_secs();
        let mut i = 0;
        let mut tick = t_out;
        while tick <= horizon + 1e-9 {
            let start = i;
            while i < self.plots.len() && self.plots[i].time.as_secs() <= tick + 1e-9 {
                i += 1;
            }
            self.tracker.process_plots(&self.plots[start..i]);
            let frame_plots: Vec<FramePlot> = self.plots[start..i]
                .iter()
                .filter_map(|p| {
                    let sensor_model = self.tracker.config().sensors.get(&p.sensor)?;
                    let position = sensor_model.frame.enu_to_geodetic(&p.measurement.to_enu());
                    Some(FramePlot::from_plot(
                        position.lat_deg(),
                        position.lon_deg(),
                        p.kind.has_secondary(),
                    ))
                })
                .collect();
            let ts = Timestamp(tick);
            let snapshot = self.tracker.snapshot_at(ts);
            out.push(Frame::new(ts, sensor, &frame_plots, &snapshot));
            tick += t_out;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::{Sensor, SensorId as Sid, TargetId};
    use firefly_geo::{Enu, LocalFrame, Wgs84};
    use firefly_sim::{Leg, Radar, RadarParams, State, Target};
    use firefly_track::SensorErrorModel;

    fn config() -> TrackerConfig {
        let frame = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
        TrackerConfig::single_sensor(
            Sid(1),
            frame,
            SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08),
        )
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

    /// `scans` and `frames` describe the same scan times in the same order:
    /// `frames` is just the JSON projection of the raw `scans`. The CAT062
    /// adapter consumes the raw `scans`. REQ: FR-IO-002, FR-IO-003
    #[test]
    fn scans_and_frames_share_the_same_scan_times() {
        let scans = Player::new(&scenario(), config()).scans();
        let frames = Player::new(&scenario(), config()).frames();

        assert_eq!(scans.len(), frames.len());
        for ((scan_time, tracks), frame) in scans.iter().zip(&frames) {
            assert_eq!(*scan_time, frame.time, "same scan time");
            assert_eq!(tracks.len(), frame.tracks.len(), "same track count");
        }
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

    // --- ADR 0013, Häppchen 13.4: decoupled periodic output ----------------

    /// The default output heartbeat is the fastest radar's scan period (4 s
    /// here). REQ: FR-IO-005
    #[test]
    fn default_output_period_is_the_fastest_scan_period() {
        let player = Player::new(&scenario(), config());
        assert!((player.default_output_period() - 4.0).abs() < 1e-9);
    }

    /// `periodic_snapshots` emits on a fixed heartbeat regardless of the input
    /// cadence: ticks at t_out, 2·t_out, … up to the last data time.
    /// REQ: FR-IO-005
    #[test]
    fn periodic_snapshots_emit_on_a_fixed_heartbeat() {
        let snaps = Player::new(&scenario(), config()).periodic_snapshots(4.0);
        // duration 40 s, perfect radar → scans at t = 0..40, but each plot's
        // data time is azimuth-shifted by +1 s (Häppchen 13.6) for this
        // due-north target, so the last plot lands at t = 41 ⇒ ticks 4 … 44.
        assert_eq!(snaps.len(), 11);
        for (k, (time, _)) in snaps.iter().enumerate() {
            let expected = (k as f64 + 1.0) * 4.0;
            assert!(
                (time.as_secs() - expected).abs() < 1e-9,
                "tick {k} should be at {expected} s, was {}",
                time.as_secs()
            );
        }
    }

    /// The target confirms within the periodic stream. REQ: FR-IO-005
    #[test]
    fn periodic_snapshots_confirm_a_track() {
        let snaps = Player::new(&scenario(), config()).periodic_snapshots(4.0);
        let confirmed = snaps
            .iter()
            .any(|(_, tracks)| tracks.iter().any(|t| t.confirmed));
        assert!(
            confirmed,
            "the target should confirm on the heartbeat stream"
        );
    }

    /// A finer heartbeat out-ticks the input scans: snapshots between plots show
    /// coasting tracks, the stable picture an ASD wants. REQ: FR-IO-005
    #[test]
    fn finer_heartbeat_emits_more_ticks_than_input_scans() {
        let scans = Player::new(&scenario(), config()).scans().len();
        let snaps = Player::new(&scenario(), config())
            .periodic_snapshots(1.0)
            .len();
        assert!(
            snaps > scans,
            "1 s heartbeat ({snaps}) should out-tick the 4 s scans ({scans})"
        );
    }

    /// `periodic_frames` is a pure, deterministic function of the scenario.
    /// REQ: FR-IO-005, NFR-REPRO-001
    #[test]
    fn periodic_frames_are_deterministic() {
        let a = Player::new(&scenario(), config()).periodic_frames(4.0);
        let b = Player::new(&scenario(), config()).periodic_frames(4.0);
        assert_eq!(a, b);
    }

    /// Each periodic [`Frame`] bundles the raw plots of its output window plus
    /// the track snapshot; across the stream every scan's plot is accounted for.
    /// REQ: FR-IO-005
    #[test]
    fn periodic_frames_bundle_window_plots_and_tracks() {
        let frames = Player::new(&scenario(), config()).periodic_frames(4.0);
        assert!(!frames.is_empty());
        // Perfect radar: one plot per scan, 11 scans (t = 0 … 40); each lands in
        // exactly one output window.
        let total_plots: usize = frames.iter().map(|f| f.plots.len()).sum();
        assert_eq!(total_plots, 11, "every scan's plot lands in one window");
        assert!(
            frames.iter().any(|f| !f.tracks.is_empty()),
            "tracks reported"
        );
    }
}
