//! Binary entry point: build the demo scene or start the live tracker, then
//! serve it with graceful shutdown and structured logging.

use std::sync::Arc;
use std::time::Duration;

use firefly_asterix::Cat062Encoder;
use firefly_core::{Plot, SensorId};
use firefly_flarm::FlarmConfig;
use firefly_geo::Wgs84;
use firefly_multicast::{MulticastConfig, SensorHealthMonitor};
use firefly_opensky::{OpenSkyConfig, OpenSkyPoller};
use firefly_radar::RadarConfig;
use firefly_server::sources;
use firefly_server::{
    build_live_tracker_multi, live_system_reference_point, router, run_live_cat062,
    run_live_tracker, scene, scene_reference_point, AppState, FrameSource, LiveSnapshot,
    LiveTracker, Metrics, RadarSensor, Scene, ServerConfig, ServerMode, SnapshotRx,
};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};

#[tokio::main]
async fn main() {
    init_tracing();

    let config = ServerConfig::from_env();
    tracing::info!(
        port = config.port,
        mode = ?config.mode,
        "starting Firefly server"
    );

    let metrics = Arc::new(Metrics::default());

    // WebSocket access control (NFR-SEC-001, ADR 0017).
    // Both vars are opt-in: unset = no auth (suitable for local demo/dev).
    let ws_token = std::env::var("FIREFLY_WS_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    let ws_allowed_origin = std::env::var("FIREFLY_WS_ALLOWED_ORIGIN")
        .ok()
        .filter(|s| !s.is_empty());
    if ws_token.is_some() {
        tracing::info!("WebSocket token auth enabled (FIREFLY_WS_TOKEN is set)");
    }
    if ws_allowed_origin.is_some() {
        tracing::info!(
            origin = ws_allowed_origin.as_deref().unwrap_or(""),
            "WebSocket origin check enabled"
        );
    }

    // CAT065 heartbeat runs in both modes: wall-clock liveness signal.
    spawn_cat065_heartbeat(Arc::clone(&metrics));

    let state = match config.mode {
        ServerMode::Replay => build_replay_state(config, metrics, ws_token, ws_allowed_origin),
        ServerMode::Live => build_live_state(config, metrics, ws_token, ws_allowed_origin).await,
    };

    let listener = match TcpListener::bind(("0.0.0.0", config.port)).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(%error, port = config.port, "failed to bind");
            std::process::exit(1);
        }
    };
    match listener.local_addr() {
        Ok(addr) => tracing::info!(%addr, "listening; open http://{addr} in a browser"),
        Err(_) => tracing::info!("listening"),
    }

    let app = router(state);
    if let Err(error) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        tracing::error!(%error, "server error");
    }
    tracing::info!("shutdown complete");
}

// ---------------------------------------------------------------------------
// Replay mode
// ---------------------------------------------------------------------------

/// Build the `AppState` for the deterministic Replay mode: load the pre-computed
/// scene, spawn the replay CAT062 feed, and optionally start the OpenSky poller
/// in log-only mode if `FIREFLY_OPENSKY_ENABLED=true`.
fn build_replay_state(
    config: ServerConfig,
    metrics: Arc<Metrics>,
    ws_token: Option<String>,
    ws_allowed_origin: Option<String>,
) -> AppState {
    let frames = match config.scene {
        Scene::Demo => scene::demo_frames(),
        Scene::Frankfurt => scene::frankfurt_frames(),
    };
    tracing::info!(
        speed = config.speed,
        scene = ?config.scene,
        frames = frames.len(),
        "replay mode"
    );

    // The system reference point for I062/100 is the scene origin (ADR 0021),
    // so the multicast feed's stereographic projection is coherent with the
    // tracking frame the scene was computed in.
    let reference = scene_reference_point(config.scene);
    spawn_cat062_multicast(config.speed, config.scene, reference, Arc::clone(&metrics));
    spawn_opensky_poller_log_only();

    // CAT063 sensor status — all sensors pre-seeded as active in replay mode.
    let sensor_ids = scene::scene_sensor_ids(config.scene);
    let sensor_monitor = Arc::new(SensorHealthMonitor::new_replay(sensor_ids));
    spawn_cat063_sensor_sender(sensor_monitor, Arc::clone(&metrics));

    AppState {
        source: FrameSource::Replay {
            frames: Arc::new(frames),
            speed: config.speed,
        },
        metrics,
        ws_token,
        ws_allowed_origin,
    }
}

/// Spawn the CAT062 UDP-multicast feed for Replay mode, if enabled
/// (`FIREFLY_CAT062_ENABLED=true`). Replays the same scan stream the web map
/// shows, paced into wall-clock at `speed` (ADR 0006). Disabled by default.
fn spawn_cat062_multicast(speed: f64, scene: Scene, reference: Wgs84, metrics: Arc<Metrics>) {
    let config = MulticastConfig::from_env();
    if !config.enabled {
        tracing::info!(
            "CAT062 multicast feed disabled (set FIREFLY_CAT062_ENABLED=true to enable)"
        );
        return;
    }

    let destination = config.destination();
    let encoder = Cat062Encoder::new(config.data_source(), reference, 0.0);
    let scans = match scene {
        Scene::Demo => scene::demo_scans(),
        Scene::Frankfurt => scene::frankfurt_scans(),
    };
    tracing::info!(%destination, scans = scans.len(), "CAT062 multicast feed enabled");

    tokio::spawn(async move {
        let socket = match firefly_multicast::sender_socket().await {
            Ok(socket) => socket,
            Err(error) => {
                tracing::error!(%error, "failed to open CAT062 multicast socket");
                return;
            }
        };
        let metrics_scan = Arc::clone(&metrics);
        match firefly_multicast::run(&socket, destination, &encoder, &scans, speed, move |n| {
            metrics_scan
                .tracks_active
                .store(n as u64, std::sync::atomic::Ordering::Relaxed);
        })
        .await
        {
            Ok(sent) => {
                metrics
                    .cat062_scans_sent_total
                    .fetch_add(sent as u64, std::sync::atomic::Ordering::Relaxed);
                tracing::info!(sent, "CAT062 multicast feed complete");
            }
            Err(error) => {
                metrics
                    .cat062_send_errors_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tracing::error!(%error, "CAT062 multicast feed stopped");
            }
        }
    });
}

/// Optionally start the OpenSky poller in log-only mode (Replay mode only).
/// Disabled unless `FIREFLY_OPENSKY_ENABLED=true`. Plots are logged but not
/// fed into a tracker — this path is for diagnostics and manual inspection.
fn spawn_opensky_poller_log_only() {
    let config = OpenSkyConfig::from_env();
    if !config.enabled {
        tracing::info!(
            "OpenSky ADS-B poller disabled \
             (set FIREFLY_OPENSKY_ENABLED=true to enable, \
              or use FIREFLY_MODE=live to start the live tracker)"
        );
        return;
    }

    tracing::info!(
        lat_min = config.lat_min,
        lat_max = config.lat_max,
        lon_min = config.lon_min,
        lon_max = config.lon_max,
        poll_interval_secs = config.poll_interval_secs,
        sensor_id = config.sensor_id.0,
        "OpenSky ADS-B poller enabled (log-only; FIREFLY_MODE=live to track)"
    );

    tokio::spawn(async move {
        let poller = OpenSkyPoller::new(config);
        poller
            .run(
                |plots| {
                    tracing::info!(count = plots.len(), "OpenSky plots received (log-only)");
                },
                |e| {
                    tracing::warn!(%e, "OpenSky poll error (log-only mode)");
                },
            )
            .await;
    });
}

// ---------------------------------------------------------------------------
// Live mode
// ---------------------------------------------------------------------------

/// Build the `AppState` for Live mode: create the tracker + channels, spawn the
/// OpenSky poller (unconditional in Live mode — it is the plot source), spawn
/// the live tracker task, and optionally start the live CAT062 feed.
async fn build_live_state(
    _config: ServerConfig,
    metrics: Arc<Metrics>,
    ws_token: Option<String>,
    ws_allowed_origin: Option<String>,
) -> AppState {
    // Resolve the live source set (ADR 0023): FIREFLY_SOURCES — the orchestrated
    // contract — takes precedence; otherwise fall back to the single
    // FIREFLY_OPENSKY_* config (standalone/dev). A FIREFLY_SOURCES config fault is
    // fatal, so the orchestrator sees the container fail rather than run
    // mis-sourced.
    let resolved = resolve_live_sources();
    let opensky = resolved.opensky;
    let flarm = resolved.flarm;
    let radar = resolved.radar;

    // Representative across all sources: union bbox (tracking-frame origin), output
    // cadence, and a placeholder sensor id. FLARM and radar are folded in
    // (ADR 0026/0028).
    let representative = sources::representative_config(&opensky, &flarm, &radar);
    let sensor_id = representative.sensor_id;
    let output_period = Duration::from_secs(representative.poll_interval_secs);

    // Geodetic source sensors (OpenSky, FLARM) and their scan periods: OpenSky
    // uses its poll interval, FLARM a nominal (it is a push stream). These share
    // the common tracking frame (geodetic path).
    let geodetic_sensors: Vec<(SensorId, f64)> = opensky
        .iter()
        .map(|c| (c.sensor_id, c.poll_interval_secs as f64))
        .chain(
            flarm
                .iter()
                .map(|c| (c.sensor_id, sources::FLARM_NOMINAL_SCAN_SECS)),
        )
        .collect();

    // Radar (polar) sensors register with their **own** site frame and a real
    // polar error model (ADR 0028) — CAT048 plots are polar relative to the radar.
    let radar_sensors: Vec<RadarSensor> = radar
        .iter()
        .map(|c| RadarSensor {
            id: c.sensor_id,
            position: Wgs84::from_degrees(c.lat_deg, c.lon_deg, c.height_m),
            sigma_range_m: c.sigma_range_m,
            sigma_azimuth_deg: c.sigma_azimuth_deg,
            scan_period: c.scan_period_secs,
        })
        .collect();

    // The sensor-health monitor (CAT063) tracks every source sensor — geodetic and
    // radar alike — so no adapter's plots go unmonitored (FR-TRK-010).
    let monitored_sensors: Vec<(SensorId, f64)> = geodetic_sensors
        .iter()
        .copied()
        .chain(radar.iter().map(|c| (c.sensor_id, c.scan_period_secs)))
        .collect();

    tracing::info!(
        opensky_sources = opensky.len(),
        flarm_sources = flarm.len(),
        radar_sources = radar.len(),
        lat_min = representative.lat_min,
        lat_max = representative.lat_max,
        lon_min = representative.lon_min,
        lon_max = representative.lon_max,
        output_period_secs = representative.poll_interval_secs,
        "live mode: starting tracker over all live sources"
    );

    // Channel: source adapters → live tracker (bounded; drop batches if the
    // tracker is busy rather than blocking a network callback). Each source clones
    // the sender into this shared channel.
    let (plots_tx, plots_rx) = mpsc::channel::<Vec<Plot>>(32);

    // Channel: live tracker → WS/CAT062 consumers ("last value wins" watch).
    let (snapshot_tx, snapshot_rx) = watch::channel(LiveSnapshot::empty());

    // The system reference point (ADR 0021): one origin for both the tracking
    // frame and the I062/100 projection. FIREFLY_SYSTEM_REF_* overrides it;
    // otherwise it is the union bounding-box midpoint.
    let reference = live_system_reference_point(&representative);

    // Build and spawn the live tracker task, registering every source sensor —
    // geodetic adapters on the shared frame, radar sensors on their own site frame.
    let tracker =
        build_live_tracker_multi(reference, geodetic_sensors.iter().copied(), radar_sensors);
    let live = LiveTracker::new(tracker, None); // recorder wired in AP9.4c-4
    {
        let m = Arc::clone(&metrics);
        tokio::spawn(run_live_tracker(
            live,
            plots_rx,
            snapshot_tx,
            output_period,
            move |plots, records| {
                m.live_plots_ingested_total
                    .store(plots, std::sync::atomic::Ordering::Relaxed);
                m.plot_records_written_total
                    .store(records, std::sync::atomic::Ordering::Relaxed);
            },
        ));
    }

    // Sensor health monitor over all source sensors (CAT063 per-sensor liveness).
    let sensor_monitor = Arc::new(SensorHealthMonitor::new_live(monitored_sensors));

    // Spawn one adapter per source, all feeding the shared plot channel and
    // notifying the sensor monitor for their own sensor.
    for cfg in opensky {
        spawn_opensky_poller_live(
            cfg,
            plots_tx.clone(),
            Arc::clone(&metrics),
            Arc::clone(&sensor_monitor),
        );
    }
    for cfg in flarm {
        spawn_flarm_listener_live(
            cfg,
            plots_tx.clone(),
            Arc::clone(&metrics),
            Arc::clone(&sensor_monitor),
        );
    }
    for cfg in radar {
        spawn_radar_listener_live(
            cfg,
            plots_tx.clone(),
            Arc::clone(&metrics),
            Arc::clone(&sensor_monitor),
        );
    }

    // Spawn the live CAT062 feed, if enabled.
    spawn_cat062_live(Arc::clone(&metrics), reference, snapshot_rx.clone());

    // CAT063 sensor status for live mode.
    spawn_cat063_sensor_sender(sensor_monitor, Arc::clone(&metrics));

    AppState {
        source: FrameSource::Live {
            snapshots: snapshot_rx,
            sensor: sensor_id,
        },
        metrics,
        ws_token,
        ws_allowed_origin,
    }
}

/// Resolve the live source set (ADR 0023). `FIREFLY_SOURCES` — the orchestrated
/// source-input contract — wins when set; otherwise the single `FIREFLY_OPENSKY_*`
/// config (standalone/dev) is used, preserving the pre-contract behaviour.
///
/// A `FIREFLY_SOURCES` parse or config fault is **fatal**: the process exits so
/// the orchestrator sees the container fail rather than run with the wrong (or no)
/// sources. Reserved types without an adapter yet are logged and skipped; an empty
/// effective set runs an idle tracker (the instance still serves an empty sky).
fn resolve_live_sources() -> sources::ResolvedSources {
    let json = match std::env::var("FIREFLY_SOURCES") {
        Ok(j) if !j.trim().is_empty() => j,
        _ => {
            // Standalone/dev: OpenSky is the implicit primary source in Live mode;
            // FLARM/OGN (ADR 0026) and radar ASTERIX (ADR 0028) are opt-in via
            // FIREFLY_FLARM_ENABLED / FIREFLY_RADAR_ENABLED.
            let flarm_cfg = FlarmConfig::from_env();
            let flarm = if flarm_cfg.enabled {
                vec![flarm_cfg]
            } else {
                Vec::new()
            };
            let radar_cfg = RadarConfig::from_env();
            let radar = if radar_cfg.enabled {
                vec![radar_cfg]
            } else {
                Vec::new()
            };
            return sources::ResolvedSources {
                opensky: vec![OpenSkyConfig::from_env()],
                flarm,
                radar,
                skipped: Vec::new(),
            };
        }
    };
    let specs = sources::parse_sources(&json).unwrap_or_else(|e| {
        tracing::error!(error = %e, "FIREFLY_SOURCES invalid; aborting");
        std::process::exit(1);
    });
    let resolved =
        sources::resolve_sources(&specs, |k| std::env::var(k).ok()).unwrap_or_else(|e| {
            tracing::error!(error = %e, "FIREFLY_SOURCES unusable; aborting");
            std::process::exit(1);
        });
    for source_type in &resolved.skipped {
        tracing::warn!(
            ?source_type,
            "FIREFLY_SOURCES: no adapter for this source type yet; skipping"
        );
    }
    if resolved.opensky.is_empty() && resolved.flarm.is_empty() && resolved.radar.is_empty() {
        tracing::warn!("FIREFLY_SOURCES: no live source adapter active (empty sky)");
    }
    resolved
}

/// Spawn the OpenSky poller in Live mode: every batch of plots is sent into
/// `plots_tx` so the tracker can consume it. If the channel is full, the batch
/// is dropped and a warning is logged — availability over back-pressure.
///
/// Sets `metrics.live_ready = true` on the first successful poll (AP9.4c-4):
/// until then `/ready` returns 503. Increments `opensky_poll_errors_total` on
/// each HTTP/network failure.
///
/// Also notifies `sensor_monitor` on each successful batch so it can track
/// the liveness of the single OpenSky sensor (Firefly #32).
fn spawn_opensky_poller_live(
    config: OpenSkyConfig,
    plots_tx: mpsc::Sender<Vec<Plot>>,
    metrics: Arc<Metrics>,
    sensor_monitor: Arc<SensorHealthMonitor>,
) {
    let sensor_id = config.sensor_id;
    tracing::info!(
        lat_min = config.lat_min,
        lat_max = config.lat_max,
        poll_interval_secs = config.poll_interval_secs,
        "OpenSky ADS-B poller started (live mode)"
    );
    tokio::spawn(async move {
        let poller = OpenSkyPoller::new(config);
        let metrics_err = Arc::clone(&metrics);
        poller
            .run(
                move |plots| {
                    tracing::info!(count = plots.len(), "OpenSky plots received");
                    // Mark the server as ready on the first successful poll.
                    metrics
                        .live_ready
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    // Notify the sensor health monitor: this sensor is alive.
                    sensor_monitor.record_activity(sensor_id, std::time::Instant::now());
                    if let Err(e) = plots_tx.try_send(plots) {
                        tracing::warn!("plot channel full; dropping batch: {e}");
                    }
                },
                move |e| {
                    tracing::warn!(%e, "OpenSky poll error (counted in metrics)");
                    metrics_err
                        .opensky_poll_errors_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                },
            )
            .await;
    });
}

/// Spawn the FLARM/OGN APRS-IS listener in Live mode (ADR 0026): every decoded
/// position plot is sent into `plots_tx` so the tracker can fuse it with the
/// ADS-B/radar inputs. If the channel is full the plot is dropped with a warning
/// (availability over back-pressure).
///
/// Sets `metrics.live_ready = true` on the first plot (AP9.4c-4) and counts plots
/// in `flarm_plots_received_total`. Notifies `sensor_monitor` on each plot so the
/// FLARM sensor's liveness is tracked (CAT063). The listener reconnects with
/// backoff internally and never returns.
fn spawn_flarm_listener_live(
    config: FlarmConfig,
    plots_tx: mpsc::Sender<Vec<Plot>>,
    metrics: Arc<Metrics>,
    sensor_monitor: Arc<SensorHealthMonitor>,
) {
    let sensor_id = config.sensor_id;
    tracing::info!(
        server = %config.server,
        port = config.port,
        sensor_id = sensor_id.0,
        anonymous = config.callsign.is_none(),
        "FLARM/OGN APRS-IS listener started (live mode)"
    );
    tokio::spawn(async move {
        firefly_flarm::run(&config, move |plot| {
            metrics
                .live_ready
                .store(true, std::sync::atomic::Ordering::Relaxed);
            metrics
                .flarm_plots_received_total
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            sensor_monitor.record_activity(sensor_id, std::time::Instant::now());
            if let Err(e) = plots_tx.try_send(vec![plot]) {
                tracing::warn!("plot channel full; dropping FLARM plot: {e}");
            }
        })
        .await;
    });
}

/// Spawn the radar ASTERIX (CAT048) UDP listener in Live mode (ADR 0028): each
/// decoded datagram's plots are sent into `plots_tx` so the tracker fuses them
/// with the ADS-B/FLARM inputs. A full channel drops the batch with a warning
/// (availability over back-pressure).
///
/// Sets `metrics.live_ready = true` on the first batch (AP9.4c-4) and counts plots
/// in `radar_plots_received_total`. Notifies `sensor_monitor` so the radar
/// sensor's liveness is tracked (CAT063). The listener never returns under normal
/// operation; a bind failure logs and returns (other sources keep running).
fn spawn_radar_listener_live(
    config: RadarConfig,
    plots_tx: mpsc::Sender<Vec<Plot>>,
    metrics: Arc<Metrics>,
    sensor_monitor: Arc<SensorHealthMonitor>,
) {
    let sensor_id = config.sensor_id;
    tracing::info!(
        sac = config.sac,
        sic = config.sic,
        sensor_id = sensor_id.0,
        port = config.listen_port,
        multicast = config.is_multicast(),
        "radar ASTERIX (CAT048) listener starting (live mode)"
    );
    tokio::spawn(async move {
        firefly_radar::run(&config, move |plots| {
            metrics
                .live_ready
                .store(true, std::sync::atomic::Ordering::Relaxed);
            metrics
                .radar_plots_received_total
                .fetch_add(plots.len() as u64, std::sync::atomic::Ordering::Relaxed);
            sensor_monitor.record_activity(sensor_id, std::time::Instant::now());
            if let Err(e) = plots_tx.try_send(plots) {
                tracing::warn!("plot channel full; dropping radar batch: {e}");
            }
        })
        .await;
    });
}

/// Spawn the live CAT062 multicast feed, if enabled (`FIREFLY_CAT062_ENABLED`).
/// Reads the watch channel published by the live tracker and encodes each
/// snapshot as one CAT062 data block (ADR 0020, AP9.4c-3).
fn spawn_cat062_live(metrics: Arc<Metrics>, reference: Wgs84, mut snapshot_rx: SnapshotRx) {
    let config = MulticastConfig::from_env();
    if !config.enabled {
        tracing::info!(
            "CAT062 live multicast feed disabled (set FIREFLY_CAT062_ENABLED=true to enable)"
        );
        return;
    }

    let destination = config.destination();
    let encoder = Cat062Encoder::new(config.data_source(), reference, 0.0);
    tracing::info!(%destination, "CAT062 live multicast feed enabled");

    tokio::spawn(async move {
        let socket = match firefly_multicast::sender_socket().await {
            Ok(socket) => socket,
            Err(error) => {
                tracing::error!(%error, "failed to open CAT062 live multicast socket");
                return;
            }
        };
        let result = run_live_cat062(&socket, destination, &encoder, &mut snapshot_rx, |n| {
            metrics
                .tracks_active
                .store(n as u64, std::sync::atomic::Ordering::Relaxed);
            metrics
                .cat062_scans_sent_total
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        })
        .await;
        match result {
            Ok(()) => tracing::info!("CAT062 live feed stopped"),
            Err(error) => {
                metrics
                    .cat062_send_errors_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tracing::error!(%error, "CAT062 live feed error");
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Shared: CAT063 sensor status, CAT065 heartbeat, tracing, shutdown
// ---------------------------------------------------------------------------

/// Spawn the CAT063 sensor status sender, if `FIREFLY_CAT062_ENABLED` and
/// `FIREFLY_CAT065_ENABLED` are both set (Firefly #32). Sends one block per
/// `FIREFLY_CAT063_PERIOD` (default 5 s), one record per registered sensor.
fn spawn_cat063_sensor_sender(monitor: Arc<SensorHealthMonitor>, metrics: Arc<Metrics>) {
    let config = MulticastConfig::from_env();
    if !config.enabled || !config.heartbeat_enabled {
        return;
    }

    let destination = config.destination();
    let encoder = config.cat063_encoder();
    let period = Duration::from_secs_f64(config.cat063_period_secs);
    // Set sensors_total once at startup (static count).
    metrics.sensors_total.store(
        monitor.sensors_total() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    tracing::info!(
        %destination,
        period_s = config.cat063_period_secs,
        sensors_total = monitor.sensors_total(),
        "CAT063 sensor status sender enabled"
    );

    tokio::spawn(async move {
        let socket = match firefly_multicast::sender_socket().await {
            Ok(s) => s,
            Err(error) => {
                tracing::error!(%error, "failed to open CAT063 sender socket");
                return;
            }
        };
        let result = firefly_multicast::run_cat063_sender(
            &socket,
            destination,
            &encoder,
            monitor,
            period,
            utc_time_of_day_secs,
            |active, total| {
                metrics
                    .sensors_active
                    .store(active as u64, std::sync::atomic::Ordering::Relaxed);
                metrics
                    .sensors_total
                    .store(total as u64, std::sync::atomic::Ordering::Relaxed);
                metrics
                    .cat063_status_sent_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            },
        )
        .await;
        if let Err(error) = result {
            tracing::error!(%error, "CAT063 sensor status sender stopped");
        }
    });
}

/// Spawn the CAT065 SDPS-status heartbeat alongside the track feed, if both
/// `FIREFLY_CAT062_ENABLED` and `FIREFLY_CAT065_ENABLED` (default on) are set
/// (ADR 0018). Runs in both Replay and Live modes.
fn spawn_cat065_heartbeat(metrics: Arc<Metrics>) {
    let config = MulticastConfig::from_env();
    if !config.enabled || !config.heartbeat_enabled {
        return;
    }

    let destination = config.destination();
    let encoder = config.cat065_encoder();
    let period = Duration::from_secs_f64(config.heartbeat_period_secs);
    tracing::info!(%destination, period_s = config.heartbeat_period_secs, "CAT065 heartbeat enabled");

    tokio::spawn(async move {
        let socket = match firefly_multicast::sender_socket().await {
            Ok(socket) => socket,
            Err(error) => {
                tracing::error!(%error, "failed to open CAT065 heartbeat socket");
                return;
            }
        };
        let result = firefly_multicast::run_heartbeat(
            &socket,
            destination,
            &encoder,
            period,
            utc_time_of_day_secs,
            || {
                metrics
                    .cat065_heartbeats_sent_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            },
        )
        .await;
        if let Err(error) = result {
            tracing::error!(%error, "CAT065 heartbeat stopped");
        }
    });
}

/// The current UTC time of day in seconds since midnight, for I065/030.
fn utc_time_of_day_secs() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64().rem_euclid(86_400.0))
        .unwrap_or(0.0)
}

/// Initialise structured logging/tracing. Verbosity follows `RUST_LOG`
/// (default `info`). REQ: NFR-OBS-001
fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).init();
}

/// Resolve when the process is asked to stop (Ctrl-C or, on Unix, SIGTERM),
/// so Kubernetes can drain the pod cleanly. ADR 0003.
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("shutdown signal received");
}
