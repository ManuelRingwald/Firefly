//! The live-tracker runtime (ADR 0020, AP9.4c-2).
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
//! 2. **Shared snapshot.** After each output tick the task publishes the current
//!    air picture as a fresh `Vec<SystemTrack>` over a [`watch`] channel. The WS
//!    pump and the CAT062 feed (wired in AP9.4c-3) read the latest snapshot
//!    without ever blocking the tracker.
//!
//! This module deliberately contains **no** new tracking logic: the tracker core
//! ([`firefly_track`]) can already be fed live (`process_plots`).

use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use firefly_core::{Plot, SystemTrack, Timestamp};
use firefly_geo::{LocalFrame, Wgs84};
use firefly_opensky::OpenSkyConfig;
use firefly_track::{ProcessNoise, SensorErrorModel, Tracker, TrackerConfig};
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

/// The air-picture snapshot shared from the live tracker to its readers.
///
/// An `Arc` so publishing and reading are an atomic pointer swap — the tracker
/// is never blocked by a slow consumer, and consumers always see a consistent
/// whole picture (never a half-updated one).
pub type SnapshotRx = watch::Receiver<Arc<Vec<SystemTrack>>>;

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

/// Build the [`Tracker`] for the live ADS-B feed (ADR 0020).
///
/// The tracking frame is centred on the **midpoint of the configured OpenSky
/// bounding box** (ADR 0020, decided question 3) — a sensible system reference
/// point for the area being watched. A single sensor is registered under the
/// adapter's [`SensorId`](firefly_core::SensorId) so its plots are accepted.
///
/// ADS-B plots carry their own *geodetic* position and an isotropic,
/// NACp-derived covariance, so the polar [`SensorErrorModel`] is **unused** for
/// them (see [`firefly_track::tracking_measurement`]); a placeholder model
/// satisfies the API. The configured scan period (the poll interval) floors the
/// deletion cadence so a track is not churned away between polls.
pub fn build_live_tracker(config: &OpenSkyConfig) -> Tracker {
    let lat = 0.5 * (config.lat_min + config.lat_max);
    let lon = 0.5 * (config.lon_min + config.lon_max);
    let origin = Wgs84::from_degrees(lat, lon, 0.0);
    let frame = LocalFrame::new(origin);

    // Placeholder polar model — irrelevant for the geodetic ADS-B path.
    let placeholder_error = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.1);
    let scan_period = config.poll_interval_secs as f64;

    let mut tracker_config =
        TrackerConfig::single_sensor(config.sensor_id, frame, placeholder_error, scan_period);
    tracker_config.process_noise = ProcessNoise::new(LIVE_PROCESS_NOISE);
    Tracker::new(tracker_config)
}

/// A live tracker plus its input recorder: the synchronous core driven by the
/// async [`run_live_tracker`] task. Kept free of any timing/IO scheduling so it
/// is fully unit-testable.
pub struct LiveTracker {
    tracker: Tracker,
    recorder: Option<PlotRecorder>,
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
            latest_data_time: None,
            plots_ingested: 0,
        }
    }

    /// Ingest a batch of plots that arrived at wall-clock `recv_unix_ns`.
    ///
    /// Each plot is **recorded first** (so the `.ffplots` log faithfully mirrors
    /// the tracker's input), then the whole batch is handed to the tracker by
    /// data-time. If recording fails, the recorder is dropped and a warning is
    /// logged — tracking continues, because the live air picture must not stop
    /// when the disk fills (availability over recording).
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

        self.tracker.process_plots(plots);
        self.plots_ingested += plots.len() as u64;

        let newest = plots
            .iter()
            .map(|p| p.time.as_secs())
            .fold(f64::NEG_INFINITY, f64::max);
        self.latest_data_time = Some(
            self.latest_data_time
                .map_or(newest, |prev| prev.max(newest)),
        );
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
}

/// Run the live-tracker task until its plot input closes.
///
/// Two events drive the loop:
///
/// - **A batch of plots** arrives on `plots_rx`: it is recorded and fed to the
///   tracker ([`LiveTracker::ingest`]), stamped with the current wall-clock time.
/// - **The output ticker fires** every `output_period`: the current snapshot is
///   published over `snapshot_tx`. A send error (no receivers yet) is ignored —
///   the snapshot is simply the latest value any future reader will see.
///
/// When every sender on `plots_rx` is dropped the recorder is flushed and the
/// task returns, so a clean shutdown loses no recorded plots.
pub async fn run_live_tracker(
    mut live: LiveTracker,
    mut plots_rx: mpsc::Receiver<Vec<Plot>>,
    snapshot_tx: watch::Sender<Arc<Vec<SystemTrack>>>,
    output_period: Duration,
) {
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
                let _ = snapshot_tx.send(Arc::new(live.snapshot()));
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
            },
        )
    }

    /// A fresh live tracker has no picture until a plot arrives.
    #[test]
    fn snapshot_is_empty_before_any_plot() {
        let mut live = LiveTracker::new(build_live_tracker(&config()), None);
        assert!(live.snapshot().is_empty());
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

    /// The async task publishes a snapshot after the first output tick and stops
    /// cleanly when its input channel closes. Uses paused time so the test is
    /// deterministic and instant.
    #[tokio::test(start_paused = true)]
    async fn task_publishes_snapshot_then_stops_on_close() {
        let (plots_tx, plots_rx) = mpsc::channel(8);
        let (snapshot_tx, snapshot_rx) = watch::channel(Arc::new(Vec::new()));
        let live = LiveTracker::new(build_live_tracker(&config()), None);
        let handle = tokio::spawn(run_live_tracker(
            live,
            plots_rx,
            snapshot_tx,
            Duration::from_millis(100),
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
        assert_eq!(snapshot.len(), 1, "the confirmed track is published");

        // Closing the input ends the task.
        drop(plots_tx);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(handle.await.is_ok(), "task returns after input closes");
    }
}
