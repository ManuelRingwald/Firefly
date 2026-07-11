//! The live-tracker runtime (ADR 0020, AP9.4c-2/3).
//!
//! In **Live** mode Firefly is no longer a deterministic pre-computed replay: a
//! long-lived [`Tracker`] runs in its own task, fed a stream of [`Plot`]s by the
//! sensor adapters (OpenSky ADS-B today; PSR/SSR and FLARM later). Plots arrive
//! wall-clock-driven over an `mpsc` channel, but the tracker itself stays
//! **data-time driven** (ADR 0013): every plot carries its own time, and the
//! tracker is a deterministic function of that plot sequence.
//!
//! Two things make the non-deterministic *arrival* reproducible nonetheless
//! (ADR 0020, NFR-REPRO-001):
//!
//! 1. **Input recording.** Every ingested plot is written to a `.ffplots` file
//!    *before* it reaches the tracker, via [`PlotRecorder`]. Replaying that file
//!    re-runs the exact tracking session — the basis for reproducing a
//!    production fault. Recording is source-agnostic: any [`Plot`] serialises
//!    the same way (ADR 0020).
//! 2. **Shared snapshot.** After each output tick the task publishes a
//!    [`LiveSnapshot`] over a [`watch`] channel. Both the WS pump and the CAT062
//!    live sender (AP9.4c-3) read the latest value without ever blocking the
//!    tracker. The snapshot carries the data-time so consumers can build a
//!    correctly-timestamped [`firefly_io::Frame`] or CAT062 block.
//!
//! This module deliberately contains **no** new tracking logic: the tracker core
//! ([`firefly_track`]) can already be fed live (`process_plots`).

use std::io::{self, BufWriter, Write};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use firefly_asterix::Cat062Encoder;
use firefly_core::{Plot, SensorId, SystemTrack, Timestamp};
use firefly_geo::{LocalFrame, Wgs84};
use firefly_meteo::QnhService;
use firefly_opensky::OpenSkyConfig;
use firefly_track::{
    ProcessNoise, RegistrationApplier, RegistrationMonitor, SensorErrorModel, Tracker,
    TrackerConfig,
};
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

/// The air-picture snapshot published by the live tracker after each output tick
/// (ADR 0020, AP9.4c-3).
///
/// Carrying the data-time alongside the tracks lets both the WS pump and the
/// CAT062 live sender build correctly-timestamped output without querying the
/// tracker separately. Cheap to clone: the `Arc` only bumps a reference count.
#[derive(Clone, Debug)]
pub struct LiveSnapshot {
    /// The latest data-time seen by the tracker — the instant the air picture
    /// is projected to. `Timestamp(0.0)` in the initial empty value before the
    /// first ADS-B poll arrives.
    pub time: Timestamp,
    /// The current confirmed and tentative tracks, plus any track-ended records
    /// drained from the tracker's ended-buffer (carrying `ended = true` for the
    /// CAT062 TSE signal, ADR 0016).
    pub tracks: Arc<Vec<SystemTrack>>,
}

impl LiveSnapshot {
    /// The initial, empty snapshot used to seed the `watch` channel before the
    /// first ADS-B poll arrives.
    pub fn empty() -> Self {
        Self {
            time: Timestamp(0.0),
            tracks: Arc::new(Vec::new()),
        }
    }
}

/// Receiver half of the watch channel that carries [`LiveSnapshot`]s from the
/// tracker task to its consumers (WS pump, CAT062 live sender).
pub type SnapshotRx = watch::Receiver<LiveSnapshot>;

/// The process-noise budget for the live ADS-B tracker (m²/s³-ish PSD knob),
/// matching the showcase tuning ([`crate::scene`]). Airliners manoeuvre gently;
/// this lets the constant-velocity/turn IMM follow them without fracturing.
const LIVE_PROCESS_NOISE: f64 = 60.0;

/// Writes every ingested [`Plot`] to a `.ffplots` input-recording file (ADR
/// 0020). The framing is owned by [`firefly_recorder`]; this is the thin,
/// buffered, count-keeping adapter the live task drives.
///
/// Recording is **best-effort relative to availability**: if a write fails (a
/// full disk, say), the live picture must not stop — the caller drops the
/// recorder and keeps tracking (see [`LiveTracker::ingest`]).
pub struct PlotRecorder {
    writer: BufWriter<Box<dyn Write + Send>>,
    written: u64,
}

impl PlotRecorder {
    /// Create a recorder writing to a new `.ffplots` file at `path`, truncating
    /// any existing file. Writes the file header immediately.
    pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = std::fs::File::create(path)?;
        Self::with_writer(Box::new(file))
    }

    /// Create a recorder over an arbitrary sink (used in tests). Writes the
    /// `.ffplots` file header immediately.
    pub fn with_writer(sink: Box<dyn Write + Send>) -> io::Result<Self> {
        let mut writer = BufWriter::new(sink);
        firefly_recorder::write_plot_file_header(&mut writer)?;
        Ok(Self { writer, written: 0 })
    }

    /// Append one plot record, stamped with the wall-clock receive time.
    pub fn record(&mut self, timestamp_unix_ns: u64, plot: &Plot) -> io::Result<()> {
        firefly_recorder::write_plot_record(&mut self.writer, timestamp_unix_ns, plot)?;
        self.written += 1;
        Ok(())
    }

    /// Flush buffered records to the underlying sink (call after each batch so a
    /// crash loses at most the most recent batch).
    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    /// Number of plot records written so far.
    pub fn written(&self) -> u64 {
        self.written
    }
}

/// Resolve the optional input recorder from the `FIREFLY_PLOT_RECORD_PATH`
/// value (`path`): unset, empty or whitespace → **no** recording (the default);
/// otherwise a `.ffplots` recorder truncating/creating a file at that path (ADR
/// 0020). Recording the tracker's *input* plot stream is the recovery path a
/// live instance replays after a restart — the groundwork the ARTAS-gap
/// roadmap (QW.4 → SDPS-002/HA) needs wired in the live path, not just
/// unit-tested.
///
/// A creation failure (unwritable path, missing directory) is **non-fatal**: it
/// logs a warning and returns `None`, because opening a recording file must
/// never stop the live air picture from starting (availability over recording,
/// the same policy [`LiveTracker::ingest`] applies to write failures).
pub fn resolve_plot_recorder(path: Option<&str>) -> Option<PlotRecorder> {
    let path = path.map(str::trim).filter(|s| !s.is_empty())?;
    match PlotRecorder::create(path) {
        Ok(recorder) => {
            info!(path, "recording ingested plots to .ffplots (ADR 0020)");
            Some(recorder)
        }
        Err(error) => {
            warn!(
                %error,
                path,
                "could not open plot recording file; continuing without recording"
            );
            None
        }
    }
}

/// Resolve the **system reference point** for live/plot-replay mode (ADR 0021):
/// the single geodetic origin shared by the tracking frame *and* the CAT062
/// I062/100 projection, so both are coherent.
///
/// `FIREFLY_SYSTEM_REF_LAT` / `FIREFLY_SYSTEM_REF_LON` (degrees) override it;
/// otherwise it defaults to the **midpoint of the configured OpenSky bounding
/// box** (ADR 0020, decided question 3) — a sensible reference for the watched
/// area. Unset or unparseable values fall back to the bounding-box midpoint.
pub fn live_system_reference_point(config: &OpenSkyConfig) -> Wgs84 {
    let lat_default = 0.5 * (config.lat_min + config.lat_max);
    let lon_default = 0.5 * (config.lon_min + config.lon_max);
    resolve_system_reference_point(
        std::env::var("FIREFLY_SYSTEM_REF_LAT").ok().as_deref(),
        std::env::var("FIREFLY_SYSTEM_REF_LON").ok().as_deref(),
        lat_default,
        lon_default,
    )
}

/// Pure resolver behind [`live_system_reference_point`], split from the
/// environment lookup so it is testable without touching the process env.
fn resolve_system_reference_point(
    lat_env: Option<&str>,
    lon_env: Option<&str>,
    lat_default: f64,
    lon_default: f64,
) -> Wgs84 {
    let lat = lat_env
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(lat_default);
    let lon = lon_env
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(lon_default);
    Wgs84::from_degrees(lat, lon, 0.0)
}

/// Build the [`Tracker`] for the live ADS-B feed (ADR 0020).
///
/// The tracking frame is centred on the **system reference point**
/// ([`live_system_reference_point`]) — by default the midpoint of the configured
/// OpenSky bounding box, overridable via `FIREFLY_SYSTEM_REF_*` (ADR 0021). A
/// single sensor is registered under the adapter's
/// [`SensorId`](firefly_core::SensorId) so its plots are accepted.
///
/// ADS-B plots carry their own *geodetic* position and an isotropic,
/// NACp-derived covariance, so the polar [`SensorErrorModel`] is **unused** for
/// them (see [`firefly_track::tracking_measurement`]); a placeholder model
/// satisfies the API. The configured scan period (the poll interval) floors the
/// deletion cadence so a track is not churned away between polls.
pub fn build_live_tracker(config: &OpenSkyConfig) -> Tracker {
    build_live_tracker_multi(
        live_system_reference_point(config),
        std::iter::once((config.sensor_id, config.poll_interval_secs as f64)),
        std::iter::empty(),
    )
}

/// A polar (radar) live sensor: unlike the geodetic adapters it has its **own**
/// site frame and a real polar error model, because CAT048 plots are polar
/// relative to the radar (ADR 0028). Built from a `radar_asterix` source.
pub struct RadarSensor {
    /// The radar's tracker [`SensorId`].
    pub id: SensorId,
    /// The radar site position (anchors this sensor's local frame).
    pub position: Wgs84,
    /// 1σ slant-range measurement noise, metres.
    pub sigma_range_m: f64,
    /// 1σ azimuth measurement noise, degrees.
    pub sigma_azimuth_deg: f64,
    /// Antenna revolution (scan) period, seconds.
    pub scan_period: f64,
}

/// Build the live [`Tracker`] anchored at `reference`, registering **every** live
/// source sensor so the tracker accepts plots from all adapters (FR-TRK-010).
///
/// `geodetic_sensors` (OpenSky ADS-B, FLARM/OGN) carry their own world position
/// and an isotropic covariance, so they share the common tracking frame and a
/// placeholder polar error model (unused for the geodetic path). `radar_sensors`
/// (ADR 0028) are **polar**: each registers with its **own site frame** and a
/// real [`SensorErrorModel`], so range/azimuth plots lift correctly into the
/// tracking frame. The slowest registered scan period floors the deletion cadence
/// (ADR 0013), so a track is not churned away between a slow sensor's updates.
pub fn build_live_tracker_multi(
    reference: Wgs84,
    geodetic_sensors: impl IntoIterator<Item = (SensorId, f64)>,
    radar_sensors: impl IntoIterator<Item = RadarSensor>,
) -> Tracker {
    let frame = LocalFrame::new(reference);
    // Placeholder polar model — irrelevant for the geodetic ADS-B/FLARM path.
    let placeholder_error = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.1);

    let mut tracker_config = TrackerConfig::new(frame);
    for (id, scan_period) in geodetic_sensors {
        tracker_config = tracker_config.with_sensor(id, frame, placeholder_error, scan_period);
    }
    for r in radar_sensors {
        let site_frame = LocalFrame::new(r.position);
        let error =
            SensorErrorModel::from_range_and_azimuth_deg(r.sigma_range_m, r.sigma_azimuth_deg);
        tracker_config = tracker_config.with_sensor(r.id, site_frame, error, r.scan_period);
    }
    tracker_config.process_noise = ProcessNoise::new(LIVE_PROCESS_NOISE);
    Tracker::new(tracker_config)
}

/// Is the registration shadow monitor opt-in flag set? Accepts the value of
/// `FIREFLY_REGISTRATION_ENABLED`: `1`/`true`/`yes` (case-insensitive,
/// whitespace-trimmed) enable it; anything else — including unset — leaves it
/// off. Same convention as the other boolean env knobs. REG.2a, REQ: FR-TRK-038
pub fn registration_enabled(value: Option<&str>) -> bool {
    value.is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

/// The registration shadow monitor's state as handed to the `on_tick` callback
/// (REG.2a): everything the metrics endpoint exports, snapshotted once per
/// output tick. `None` when no monitor is configured.
#[derive(Clone, Debug)]
pub struct RegistrationTick {
    /// How many estimates the monitor has produced so far (counter).
    pub estimates_total: u64,
    /// Correspondences found by the most recent estimation attempt (gauge).
    pub last_pair_count: usize,
    /// Whether the latest estimate was fully observable (`false` before the
    /// first estimate).
    pub observable: bool,
    /// Per-sensor bias estimates from the latest run: `(sensor, range bias in
    /// metres, azimuth bias in degrees)`. Empty before the first estimate.
    pub biases: Vec<(SensorId, f64, f64)>,
    /// Whether a REG.2b correction is currently in effect (always `false`
    /// without an applier attached).
    pub apply_active: bool,
    /// The correction currently **applied** per sensor (REG.2b): `(sensor,
    /// range bias in metres, azimuth bias in degrees)`. Distinct from
    /// `biases` (the latest raw estimate) — this is the smoothed, gated value
    /// actually subtracted from measurements. Empty without an applier or
    /// while no correction is engaged.
    pub applied: Vec<(SensorId, f64, f64)>,
}

/// Correct a track's barometric altitude to the regional QNH at its position
/// (VERT.2). Only an **observed** regional QNH corrects; the
/// standard-atmosphere fallback leaves the pressure altitude untouched and
/// the flag cleared — I062/135 then honestly reports an uncorrected value.
fn apply_qnh(track: &mut SystemTrack, meteo: &QnhService) {
    let Some(pressure_altitude_ft) = track.barometric_altitude_ft else {
        return;
    };
    let qnh = meteo.lookup(track.position.lat_deg(), track.position.lon_deg());
    if qnh.is_observed() {
        track.barometric_altitude_ft = Some(firefly_meteo::pressure_altitude_to_qnh_altitude(
            pressure_altitude_ft,
            qnh.hpa,
        ));
        track.barometric_qnh_corrected = true;
    }
}

/// A live tracker plus its input recorder: the synchronous core driven by the
/// async [`run_live_tracker`] task. Kept free of any timing/IO scheduling so it
/// is fully unit-testable.
pub struct LiveTracker {
    tracker: Tracker,
    recorder: Option<PlotRecorder>,
    /// The optional registration shadow monitor (REG.2a): observes the same
    /// plot stream the tracker ingests but never changes it — its estimates
    /// go to logs/metrics only, until REG.2b defines an application policy.
    registration: Option<RegistrationMonitor>,
    /// The optional registration applier (REG.2b): when attached, the applied
    /// per-sensor bias is subtracted from radar polar measurements **before**
    /// they reach the tracker, gated by its [`ApplyPolicy`](firefly_track::ApplyPolicy)
    /// and advanced once per monitor estimation run.
    applier: Option<RegistrationApplier>,
    /// The regional QNH service (VERT.1/VERT.2): applied to each snapshot's
    /// barometric altitudes at the track position. `None` (or an empty
    /// service) leaves the pressure altitudes uncorrected — with the I062/135
    /// QNH bit honestly cleared.
    meteo: Option<QnhService>,
    /// The freshest plot data-time seen, the instant snapshots are projected to.
    latest_data_time: Option<f64>,
    /// Total plots handed to the tracker (for metrics, AP9.4c-4).
    plots_ingested: u64,
}

impl LiveTracker {
    /// Wrap a tracker and an optional input recorder.
    pub fn new(tracker: Tracker, recorder: Option<PlotRecorder>) -> Self {
        Self {
            tracker,
            recorder,
            registration: None,
            applier: None,
            meteo: None,
            latest_data_time: None,
            plots_ingested: 0,
        }
    }

    /// Attach a registration shadow monitor (REG.2a). The monitor observes
    /// every ingested batch; on its own it has no effect on the tracker's
    /// picture.
    pub fn with_registration(mut self, monitor: RegistrationMonitor) -> Self {
        self.registration = Some(monitor);
        self
    }

    /// Attach the QNH service (VERT.2): each published snapshot's barometric
    /// altitude is corrected to the regional QNH at the track position —
    /// where one is **observed**; otherwise the pressure altitude passes
    /// through with the QNH flag cleared (never a silent standard-atmosphere
    /// claim).
    pub fn with_meteo(mut self, service: QnhService) -> Self {
        self.meteo = Some(service);
        self
    }

    /// Attach a registration applier (REG.2b): its applied correction is
    /// subtracted from radar measurements before tracking. The applier only
    /// *advances* through the monitor's estimation runs — without a monitor
    /// attached its correction stays frozen at its current state.
    pub fn with_registration_apply(mut self, applier: RegistrationApplier) -> Self {
        self.applier = Some(applier);
        self
    }

    /// Ingest a batch of plots that arrived at wall-clock `recv_unix_ns`.
    ///
    /// Each plot is **recorded first** (so the `.ffplots` log faithfully
    /// mirrors the tracker's input — deliberately the **raw** stream, so a
    /// replay re-runs the same correction logic instead of double-correcting),
    /// then the batch — bias-corrected if a REG.2b applier is active — is
    /// handed to the tracker by data-time. If recording fails, the recorder is
    /// dropped and a warning is logged — tracking continues, because the live
    /// air picture must not stop when the disk fills (availability over
    /// recording).
    pub fn ingest(&mut self, plots: &[Plot], recv_unix_ns: u64) {
        if plots.is_empty() {
            return;
        }

        if let Some(recorder) = self.recorder.as_mut() {
            let result = plots
                .iter()
                .try_for_each(|plot| recorder.record(recv_unix_ns, plot))
                .and_then(|()| recorder.flush());
            if let Err(error) = result {
                warn!(%error, "plot recording failed; continuing without recording");
                self.recorder = None;
            }
        }

        // Registration correction (REG.2b): subtract each radar's applied bias
        // before the measurements reach the tracker. A pass-through when no
        // applier is attached or no correction is in effect.
        match self.applier.as_ref().filter(|a| a.active()) {
            Some(applier) => {
                let corrected: Vec<Plot> = plots.iter().map(|p| applier.correct(p)).collect();
                self.tracker.process_plots(&corrected);
            }
            None => self.tracker.process_plots(plots),
        }
        self.plots_ingested += plots.len() as u64;

        let newest = plots
            .iter()
            .map(|p| p.time.as_secs())
            .fold(f64::NEG_INFINITY, f64::max);
        self.latest_data_time = Some(
            self.latest_data_time
                .map_or(newest, |prev| prev.max(newest)),
        );

        // Registration estimation (REG.2a): observe the same batch, driven by
        // the same data-time watermark. Deliberately after the tracker ingest,
        // and deliberately on the RAW plots even while corrections are applied
        // — the estimate then stays the *full* bias and the applied correction
        // is a pure low-pass of it (no integrator in the loop, nothing to
        // oscillate; see firefly_track::RegistrationApplier).
        if let Some(monitor) = self.registration.as_mut() {
            let now = self.latest_data_time.unwrap_or(newest);
            let runs_before = monitor.runs_total();
            let fresh = monitor.observe(plots, now).cloned();
            if let Some(solution) = &fresh {
                let biases: Vec<String> = solution
                    .biases
                    .iter()
                    .map(|(id, b)| {
                        format!(
                            "sensor {}: dr={:+.1} m, dtheta={:+.4} deg",
                            id.0,
                            b.range_m,
                            b.azimuth_deg()
                        )
                    })
                    .collect();
                info!(
                    pairs = monitor.last_pair_count(),
                    rms_before_m = format!("{:.1}", solution.rms_before_m).as_str(),
                    rms_after_m = format!("{:.1}", solution.rms_after_m).as_str(),
                    observable = solution.observable,
                    applying = self.applier.is_some(),
                    biases = biases.join("; ").as_str(),
                    "registration estimate"
                );
            }
            // Advance the applier exactly once per estimation run — refused
            // runs count too (they feed the hold/decay policy).
            if monitor.runs_total() > runs_before {
                if let Some(applier) = self.applier.as_mut() {
                    let was_active = applier.active();
                    applier.update(fresh.as_ref());
                    if applier.active() != was_active {
                        info!(
                            active = applier.active(),
                            "registration correction {} (REG.2b)",
                            if applier.active() {
                                "engaged"
                            } else {
                                "disengaged"
                            }
                        );
                    }
                }
            }
        }
    }

    /// The current air picture, projected to the latest data-time and appended
    /// with any tracks that **ended** since the last snapshot (drained, carrying
    /// `ended = true` for the CAT062 TSE signal, ADR 0016).
    ///
    /// Empty until the first plot arrives. Draining the ended buffer here means
    /// each ended track is included in exactly one snapshot; the AP9.4c-3
    /// consumers read on the same heartbeat, so the one-shot TSE is preserved.
    pub fn snapshot(&mut self) -> Vec<SystemTrack> {
        let Some(t) = self.latest_data_time else {
            return Vec::new();
        };
        let mut tracks = self.tracker.snapshot_at(Timestamp(t));
        tracks.extend(self.tracker.take_ended_tracks());
        if let Some(meteo) = &self.meteo {
            for track in &mut tracks {
                apply_qnh(track, meteo);
            }
        }
        tracks
    }

    /// Flush the input recorder, if any (called on shutdown).
    pub fn flush(&mut self) {
        if let Some(recorder) = self.recorder.as_mut() {
            if let Err(error) = recorder.flush() {
                warn!(%error, "failed to flush plot recording on shutdown");
            }
        }
    }

    /// Total plots handed to the tracker so far.
    pub fn plots_ingested(&self) -> u64 {
        self.plots_ingested
    }

    /// Total plot records written to the `.ffplots` file (0 if not recording).
    pub fn records_written(&self) -> u64 {
        self.recorder.as_ref().map_or(0, PlotRecorder::written)
    }

    /// Snapshot of the registration state for the `on_tick` callback
    /// (REG.2a/2b metrics export). `None` when neither a monitor nor an
    /// applier is configured.
    pub fn registration_tick(&self) -> Option<RegistrationTick> {
        if self.registration.is_none() && self.applier.is_none() {
            return None;
        }
        let monitor = self.registration.as_ref();
        let latest = monitor.and_then(|m| m.latest());
        Some(RegistrationTick {
            estimates_total: monitor.map_or(0, RegistrationMonitor::estimates_total),
            last_pair_count: monitor.map_or(0, RegistrationMonitor::last_pair_count),
            observable: latest.is_some_and(|s| s.observable),
            biases: latest.map_or_else(Vec::new, |s| {
                s.biases
                    .iter()
                    .map(|(id, b)| (*id, b.range_m, b.azimuth_deg()))
                    .collect()
            }),
            apply_active: self
                .applier
                .as_ref()
                .is_some_and(RegistrationApplier::active),
            applied: self.applier.as_ref().map_or_else(Vec::new, |a| {
                a.applied()
                    .iter()
                    .map(|(id, b)| (*id, b.range_m, b.azimuth_deg()))
                    .collect()
            }),
        })
    }

    /// The latest data-time seen by the tracker, or `None` before the first
    /// plot arrives. Used by [`run_live_tracker`] to populate the data-time
    /// field of the published [`LiveSnapshot`].
    pub fn latest_data_time(&self) -> Option<Timestamp> {
        self.latest_data_time.map(Timestamp)
    }
}

/// Run the live-tracker task until its plot input closes.
///
/// Two events drive the loop:
///
/// - **A batch of plots** arrives on `plots_rx`: it is recorded and fed to the
///   tracker ([`LiveTracker::ingest`]), stamped with the current wall-clock time.
/// - **The output ticker fires** every `output_period`: a [`LiveSnapshot`] (data
///   time + tracks) is published over `snapshot_tx`. A send error (no receivers
///   yet) is ignored — the snapshot is the latest value any future reader sees.
///   Before the first poll the snapshot carries the empty sentinel.
///   `on_tick` is called after each tick with `(plots_ingested,
///   records_written, registration_tick)` so callers can update Prometheus
///   counters (AP9.4c-4; the registration snapshot is `None` without a
///   shadow monitor, REG.2a).
///
/// When every sender on `plots_rx` is dropped the recorder is flushed and the
/// task returns, so a clean shutdown loses no recorded plots.
pub async fn run_live_tracker<F>(
    mut live: LiveTracker,
    mut plots_rx: mpsc::Receiver<Vec<Plot>>,
    snapshot_tx: watch::Sender<LiveSnapshot>,
    output_period: Duration,
    on_tick: F,
) where
    F: Fn(u64, u64, Option<RegistrationTick>),
{
    let mut ticker = tokio::time::interval(output_period);
    // A delayed tick should not fire a burst of catch-up ticks afterwards.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    info!(
        output_period_s = output_period.as_secs_f64(),
        "live tracker started"
    );

    loop {
        tokio::select! {
            maybe_plots = plots_rx.recv() => match maybe_plots {
                Some(plots) => live.ingest(&plots, now_unix_ns()),
                None => {
                    live.flush();
                    info!(
                        plots = live.plots_ingested(),
                        records = live.records_written(),
                        "live tracker input closed; stopping"
                    );
                    return;
                }
            },
            _ = ticker.tick() => {
                let time = live.latest_data_time().unwrap_or(Timestamp(0.0));
                let tracks = live.snapshot();
                let _ = snapshot_tx.send(LiveSnapshot { time, tracks: Arc::new(tracks) });
                on_tick(live.plots_ingested(), live.records_written(), live.registration_tick());
            }
        }
    }
}

/// Run the live CAT062 multicast sender until the snapshot channel closes.
///
/// On every new [`LiveSnapshot`] published by [`run_live_tracker`], encode the
/// tracks as a CAT062 data block and send it to `destination`. Empty snapshots
/// (before the first ADS-B poll) are skipped. A send error stops the feed; the
/// caller (a spawned task) decides how to react.
///
/// `on_scan` is called after each successful send with the number of tracks in
/// that scan — callers use this to update `tracks_active` and
/// `cat062_scans_sent_total` Prometheus gauges/counters.
pub async fn run_live_cat062<F: Fn(usize)>(
    socket: &UdpSocket,
    destination: SocketAddr,
    encoder: &Cat062Encoder,
    snapshot_rx: &mut SnapshotRx,
    on_scan: F,
) -> std::io::Result<()> {
    loop {
        if snapshot_rx.changed().await.is_err() {
            info!("live snapshot channel closed; stopping CAT062 live feed");
            return Ok(());
        }
        let snapshot = snapshot_rx.borrow_and_update().clone();
        if snapshot.tracks.is_empty() {
            continue;
        }
        let block = encoder.encode(snapshot.time, &snapshot.tracks);
        match socket.send_to(&block, destination).await {
            Ok(bytes) => {
                tracing::debug!(
                    bytes,
                    tracks = snapshot.tracks.len(),
                    %destination,
                    "sent live CAT062 data block"
                );
                on_scan(snapshot.tracks.len());
            }
            Err(error) => {
                tracing::error!(%destination, %error, "failed to send live CAT062 data block");
                return Err(error);
            }
        }
    }
}

/// The current wall-clock time as Unix nanoseconds (0 if the clock predates the
/// epoch, which cannot happen in practice).
fn now_unix_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::Mutex;

    use firefly_core::{Callsign, ModeAC, SensorId};

    /// A `Write` sink that shares its buffer, so a test can read back what the
    /// recorder wrote after the recorder still owns its (boxed) writer.
    #[derive(Clone)]
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// The default-bbox config (bbox midpoint ≈ 51°N, 10.5°E).
    fn config() -> OpenSkyConfig {
        OpenSkyConfig::default()
    }

    /// With no override, the system reference point is the bounding-box midpoint;
    /// a valid override wins; garbage falls back to the default. REQ: FR-GEO-005
    #[test]
    fn system_reference_point_resolves_override_then_bbox_midpoint() {
        // No override → bbox midpoint (default config: 47–55°N, 5–16°E).
        let mid = resolve_system_reference_point(None, None, 51.0, 10.5);
        assert!((mid.lat_deg() - 51.0).abs() < 1e-12);
        assert!((mid.lon_deg() - 10.5).abs() < 1e-12);

        // Explicit override wins.
        let ovr = resolve_system_reference_point(Some("50.0379"), Some("8.5622"), 51.0, 10.5);
        assert!((ovr.lat_deg() - 50.0379).abs() < 1e-9);
        assert!((ovr.lon_deg() - 8.5622).abs() < 1e-9);

        // Garbage / non-finite falls back to the default per axis.
        let bad = resolve_system_reference_point(Some("nonsense"), Some("inf"), 51.0, 10.5);
        assert!((bad.lat_deg() - 51.0).abs() < 1e-12);
        assert!((bad.lon_deg() - 10.5).abs() < 1e-12);
    }

    /// The plot recorder is opt-in: unset, empty and whitespace-only paths all
    /// mean "no recording"; a real path opens a `.ffplots` writer whose header
    /// is on disk immediately. QW.4. REQ: FR-OPS-006
    #[test]
    fn plot_recorder_resolves_opt_in_path() {
        assert!(
            resolve_plot_recorder(None).is_none(),
            "unset → no recording"
        );
        assert!(resolve_plot_recorder(Some("")).is_none(), "empty → none");
        assert!(
            resolve_plot_recorder(Some("   ")).is_none(),
            "whitespace → none"
        );

        let dir = std::env::temp_dir().join(format!("firefly-qw4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("live.ffplots");
        let mut recorder =
            resolve_plot_recorder(path.to_str()).expect("a real path opens a recorder");
        recorder.flush().unwrap();
        // The file exists and carries the .ffplots header (8-byte magic).
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"FFPLOTS\0"), "header written on create");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A path that cannot be opened (a file inside a non-existent directory) is
    /// non-fatal: the resolver logs and returns `None` rather than aborting the
    /// live picture. QW.4. REQ: FR-OPS-006
    #[test]
    fn plot_recorder_unwritable_path_is_non_fatal() {
        let bogus = std::env::temp_dir()
            .join("firefly-qw4-does-not-exist")
            .join("nested")
            .join("live.ffplots");
        assert!(
            resolve_plot_recorder(bogus.to_str()).is_none(),
            "unopenable path → None, not a panic/abort"
        );
    }

    /// An ADS-B plot for one aircraft at a geodetic position and data-time.
    fn adsb(time: f64, lat: f64, lon: f64, icao: u32) -> Plot {
        Plot::adsb(
            SensorId(200),
            Timestamp(time),
            Wgs84::from_degrees(lat, lon, 10_000.0),
            75.0,
            ModeAC {
                mode_3a: Some(0o1234),
                flight_level_ft: Some(33_000.0),
                icao_address: Some(icao),
                callsign: Some(Callsign::new("DLH123")),
                spi: false,
                geometric_height_ft: None,
                daps: firefly_core::Daps::default(),
            },
        )
    }

    /// A fresh live tracker has no picture until a plot arrives.
    #[test]
    fn snapshot_is_empty_before_any_plot() {
        let mut live = LiveTracker::new(build_live_tracker(&config()), None);
        assert!(live.snapshot().is_empty());
    }

    /// The QNH correction applies only where a regional QNH is **observed**;
    /// outside every region (or without a barometric estimate) the track
    /// passes through untouched with the flag honestly cleared (VERT.2).
    /// REQ: FR-TRK-042
    #[test]
    fn apply_qnh_corrects_only_observed_regions() {
        let service = firefly_meteo::MeteoConfig::from_json(
            r#"[{"name":"EDDF","lat":50.03,"lon":8.57,"radius_nm":60,"qnh_hpa":983}]"#,
        )
        .unwrap()
        .into_service();

        let mut inside = sample_track_at(50.0, 8.6, Some(3_000.0));
        apply_qnh(&mut inside, &service);
        assert!(inside.barometric_qnh_corrected);
        let corrected = inside.barometric_altitude_ft.unwrap();
        assert!(
            (2_050.0..=2_300.0).contains(&corrected),
            "983 hPa lowers 3000 ft by ≈830 ft, got {corrected}"
        );

        // Munich lies outside the 60-NM radius: untouched, flag cleared.
        let mut outside = sample_track_at(48.35, 11.79, Some(3_000.0));
        apply_qnh(&mut outside, &service);
        assert_eq!(outside.barometric_altitude_ft, Some(3_000.0));
        assert!(!outside.barometric_qnh_corrected);

        // No barometric estimate → nothing to correct, no claim made.
        let mut no_baro = sample_track_at(50.0, 8.6, None);
        apply_qnh(&mut no_baro, &service);
        assert_eq!(no_baro.barometric_altitude_ft, None);
        assert!(!no_baro.barometric_qnh_corrected);
    }

    fn sample_track_at(lat: f64, lon: f64, baro_ft: Option<f64>) -> SystemTrack {
        use firefly_core::{SourceAges, TrackId};
        SystemTrack {
            id: TrackId(1),
            track_number: 1,
            time: Timestamp(0.0),
            position: Wgs84::from_degrees(lat, lon, 0.0),
            v_east: 0.0,
            v_north: 0.0,
            confirmed: true,
            coasting: false,
            monosensor: false,
            spi: false,
            daps: firefly_core::Daps::default(),
            ended: false,
            update_age: 0.0,
            position_uncertainty: 100.0,
            mode_3a: None,
            icao_address: None,
            flight_level_ft: None,
            callsign: None,
            contributing_sensors: Vec::new(),
            adsb_age_s: None,
            source_ages: SourceAges::default(),
            barometric_altitude_ft: baro_ft,
            barometric_qnh_corrected: false,
            geometric_altitude_ft: None,
            rocd_ft_min: None,
        }
    }

    /// Repeated ADS-B hits for one aircraft yield one tracked, confirmed target
    /// in the snapshot, positioned near the reported geodetic location.
    #[test]
    fn repeated_hits_confirm_one_track() {
        let mut live = LiveTracker::new(build_live_tracker(&config()), None);
        // Eight polls, 10 s apart, drifting east at the bbox midpoint latitude.
        for k in 0..8 {
            let t = k as f64 * 10.0;
            let lon = 10.5 + k as f64 * 0.01;
            live.ingest(&[adsb(t, 51.0, lon, 0x3C_AB_CD)], now_unix_ns());
        }

        let snapshot = live.snapshot();
        assert_eq!(snapshot.len(), 1, "one aircraft → one track");
        assert!(snapshot[0].confirmed, "steady hits confirm the track");
        assert_eq!(snapshot[0].icao_address, Some(0x3C_AB_CD));
        // The track sits near the latest reported position.
        assert!((snapshot[0].position.lat_deg() - 51.0).abs() < 0.2);
        assert_eq!(live.plots_ingested(), 8);
    }

    /// Two distinct ICAO addresses produce two separate tracks.
    #[test]
    fn two_aircraft_make_two_tracks() {
        let mut live = LiveTracker::new(build_live_tracker(&config()), None);
        for k in 0..6 {
            let t = k as f64 * 10.0;
            live.ingest(
                &[
                    adsb(t, 51.0, 10.5 + k as f64 * 0.01, 0x3C_00_01),
                    adsb(t, 50.0, 11.5 - k as f64 * 0.01, 0x3C_00_02),
                ],
                now_unix_ns(),
            );
        }
        let snapshot = live.snapshot();
        assert_eq!(snapshot.len(), 2);
    }

    /// Ingesting with a recorder writes one `.ffplots` record per plot, and the
    /// file replays back to exactly the plots that were ingested (the
    /// reproducibility guarantee, ADR 0020).
    #[test]
    fn recorder_captures_every_ingested_plot() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let recorder = PlotRecorder::with_writer(Box::new(SharedBuf(buf.clone()))).unwrap();
        let mut live = LiveTracker::new(build_live_tracker(&config()), Some(recorder));

        let plots = vec![
            adsb(0.0, 51.0, 10.5, 0x3C_00_01),
            adsb(0.0, 50.0, 11.5, 0x3C_00_02),
        ];
        live.ingest(&plots, 1_718_000_000_000_000_000);
        live.ingest(
            &[adsb(10.0, 51.0, 10.6, 0x3C_00_01)],
            1_718_000_010_000_000_000,
        );

        assert_eq!(live.records_written(), 3);

        // Replay the recorded file and compare to the ingested plots.
        let bytes = buf.lock().unwrap().clone();
        let mut cursor = Cursor::new(bytes);
        firefly_recorder::read_plot_file_header(&mut cursor).unwrap();
        let mut replayed = Vec::new();
        while let Some((_ts, plot)) = firefly_recorder::read_plot_record(&mut cursor).unwrap() {
            replayed.push(plot);
        }
        let expected: Vec<Plot> = plots
            .into_iter()
            .chain(std::iter::once(adsb(10.0, 51.0, 10.6, 0x3C_00_01)))
            .collect();
        assert_eq!(replayed, expected);
    }

    /// The opt-in flag follows the boolean env convention: `1`/`true`/`yes`
    /// (case-insensitive, trimmed) enable, everything else — including unset —
    /// stays off. REG.2a. REQ: FR-TRK-038
    #[test]
    fn registration_flag_parses_the_boolean_convention() {
        for on in ["1", "true", "yes", " TRUE ", "Yes"] {
            assert!(registration_enabled(Some(on)), "{on:?} enables");
        }
        for off in [
            None,
            Some(""),
            Some("0"),
            Some("false"),
            Some("no"),
            Some("on"),
        ] {
            assert!(!registration_enabled(off), "{off:?} stays off");
        }
    }

    /// The registration monitor is a **shadow**: with and without it attached,
    /// the same plot stream yields the identical air picture, and the tick
    /// snapshot only reports monitor state. REG.2a. REQ: FR-TRK-038
    #[test]
    fn registration_shadow_mode_does_not_change_the_picture() {
        use std::collections::BTreeMap;
        let radar_site = LocalFrame::new(Wgs84::from_degrees(51.5, 10.0, 0.0));
        let monitor = firefly_track::RegistrationMonitor::new(
            LocalFrame::new(Wgs84::from_degrees(51.0, 10.5, 0.0)),
            BTreeMap::from([(SensorId(300), radar_site)]),
            firefly_track::RegistrationConfig::default(),
        );

        let mut with_monitor =
            LiveTracker::new(build_live_tracker(&config()), None).with_registration(monitor);
        let mut without = LiveTracker::new(build_live_tracker(&config()), None);

        for k in 0..8 {
            let t = k as f64 * 10.0;
            let plots = vec![
                adsb(t, 51.0, 10.5 + k as f64 * 0.01, 0x3C_00_01),
                adsb(t, 50.0, 11.5 - k as f64 * 0.01, 0x3C_00_02),
            ];
            with_monitor.ingest(&plots, now_unix_ns());
            without.ingest(&plots, now_unix_ns());
        }

        assert_eq!(
            with_monitor.snapshot(),
            without.snapshot(),
            "the shadow monitor must not alter the air picture"
        );

        let tick = with_monitor
            .registration_tick()
            .expect("a configured monitor reports tick state");
        assert_eq!(tick.estimates_total, 0, "no radar plots, no estimate");
        assert!(!tick.observable);
        assert!(tick.biases.is_empty());
        assert!(
            without.registration_tick().is_none(),
            "no monitor, no tick state"
        );
    }

    /// End-to-end proof of REG.2b through the real server path: with an
    /// applier converged on the radar's bias attached, the tracked position
    /// sits on the truth; the identical stream without correction carries the
    /// full bias displacement. REQ: FR-TRK-039
    #[test]
    fn registration_apply_corrects_the_radar_picture() {
        use firefly_track::{ApplyPolicy, RegistrationSolution, SensorBias};
        use std::collections::BTreeMap;

        let reference = Wgs84::from_degrees(51.0, 10.5, 0.0);
        let site_pos = Wgs84::from_degrees(51.2, 10.0, 0.0);
        let radar_id = SensorId(301);
        let radar = || RadarSensor {
            id: radar_id,
            position: site_pos,
            sigma_range_m: 30.0,
            sigma_azimuth_deg: 0.05,
            scan_period: 10.0,
        };
        let bias = SensorBias {
            range_m: 800.0,
            azimuth_rad: 0.0,
        };

        // An applier honestly converged on the bias: 30 accepted runs of a
        // clean, observable, residual-explaining solution.
        let mut applier = RegistrationApplier::new(ApplyPolicy::default());
        let solution = RegistrationSolution {
            biases: BTreeMap::from([(radar_id, bias)]),
            rms_before_m: 800.0,
            rms_after_m: 40.0,
            observable: true,
        };
        for _ in 0..30 {
            applier.update(Some(&solution));
        }

        let build =
            || build_live_tracker_multi(reference, std::iter::empty(), std::iter::once(radar()));
        let mut with_apply = LiveTracker::new(build(), None).with_registration_apply(applier);
        let mut without = LiveTracker::new(build(), None);

        // One aircraft drifting east, seen only by the biased radar: the
        // radar reports every target 800 m too far out.
        let site = LocalFrame::new(site_pos);
        let truth_at = |k: f64| Wgs84::from_degrees(51.0, 10.5 + k * 0.01, 9_000.0);
        for k in 0..8 {
            let t = k as f64 * 10.0;
            let true_polar = site.geodetic_to_enu(&truth_at(k as f64)).to_polar();
            let measured = firefly_geo::Polar::new(
                true_polar.range + bias.range_m,
                true_polar.azimuth,
                true_polar.elevation,
            );
            let plot = Plot {
                sensor: radar_id,
                time: Timestamp(t),
                measurement: firefly_core::Measurement::Polar(measured),
                kind: firefly_core::DetectionKind::Secondary,
                source: firefly_core::SourceKind::ModeS,
                mode_ac: ModeAC {
                    icao_address: Some(0x3C_00_01),
                    ..ModeAC::default()
                },
            };
            with_apply.ingest(&[plot], now_unix_ns());
            without.ingest(&[plot], now_unix_ns());
        }

        let frame = LocalFrame::new(reference);
        let truth = frame.geodetic_to_enu(&truth_at(7.0));
        let horizontal_error = |live: &mut LiveTracker| {
            let snapshot = live.snapshot();
            assert_eq!(snapshot.len(), 1, "one aircraft, one track");
            let e = frame.geodetic_to_enu(&snapshot[0].position);
            ((e.east - truth.east).powi(2) + (e.north - truth.north).powi(2)).sqrt()
        };

        let corrected = horizontal_error(&mut with_apply);
        let raw = horizontal_error(&mut without);
        assert!(
            corrected < 200.0,
            "corrected picture sits on the truth: {corrected:.0} m off"
        );
        assert!(
            raw > 500.0,
            "uncorrected picture carries the bias: only {raw:.0} m off"
        );

        // The tick reports the engaged correction for the metrics export.
        let tick = with_apply.registration_tick().expect("applier reports");
        assert!(tick.apply_active);
        assert_eq!(tick.applied.len(), 1);
        let (sensor, applied_range, _) = tick.applied[0];
        assert_eq!(sensor, radar_id);
        assert!((applied_range - bias.range_m).abs() < 1.0);
    }

    /// The async task publishes a snapshot after the first output tick and stops
    /// cleanly when its input channel closes. Uses paused time so the test is
    /// deterministic and instant.
    #[tokio::test(start_paused = true)]
    async fn task_publishes_snapshot_then_stops_on_close() {
        let (plots_tx, plots_rx) = mpsc::channel(8);
        let (snapshot_tx, snapshot_rx) = watch::channel(LiveSnapshot::empty());
        let live = LiveTracker::new(build_live_tracker(&config()), None);
        let handle = tokio::spawn(run_live_tracker(
            live,
            plots_rx,
            snapshot_tx,
            Duration::from_millis(100),
            |_, _, _| {},
        ));

        // Feed enough hits to confirm a track.
        for k in 0..6 {
            let t = k as f64 * 10.0;
            plots_tx
                .send(vec![adsb(t, 51.0, 10.5 + k as f64 * 0.01, 0x3C_AB_CD)])
                .await
                .unwrap();
        }

        // Let the ingests and at least one output tick run.
        tokio::time::sleep(Duration::from_millis(250)).await;

        let snapshot = snapshot_rx.borrow().clone();
        assert_eq!(snapshot.tracks.len(), 1, "the confirmed track is published");
        assert!(
            snapshot.time.as_secs() > 0.0,
            "snapshot carries the data-time"
        );

        // Closing the input ends the task.
        drop(plots_tx);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(handle.await.is_ok(), "task returns after input closes");
    }
}
