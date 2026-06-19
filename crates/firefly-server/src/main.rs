//! Binary entry point: build the demo scene or start the live tracker, then
//! serve it with graceful shutdown and structured logging.

use std::sync::Arc;
use std::time::Duration;

use firefly_asterix::Cat062Encoder;
use firefly_core::Plot;
use firefly_geo::Wgs84;
use firefly_multicast::MulticastConfig;
use firefly_opensky::{OpenSkyConfig, OpenSkyPoller};
use firefly_server::{
    build_live_tracker, live_system_reference_point, router, run_live_cat062, run_live_tracker,
    scene, scene_reference_point, AppState, FrameSource, LiveSnapshot, LiveTracker, Metrics, Scene,
    ServerConfig, ServerMode, SnapshotRx,
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
    let opensky_config = OpenSkyConfig::from_env();
    let sensor_id = opensky_config.sensor_id;
    let output_period = Duration::from_secs(opensky_config.poll_interval_secs);

    tracing::info!(
        lat_min = opensky_config.lat_min,
        lat_max = opensky_config.lat_max,
        lon_min = opensky_config.lon_min,
        lon_max = opensky_config.lon_max,
        poll_interval_secs = opensky_config.poll_interval_secs,
        sensor_id = sensor_id.0,
        "live mode: starting ADS-B tracker"
    );

    // Channel: OpenSky poller → live tracker (bounded; drop batches if the
    // tracker is busy rather than blocking the network callback).
    let (plots_tx, plots_rx) = mpsc::channel::<Vec<Plot>>(32);

    // Channel: live tracker → WS/CAT062 consumers ("last value wins" watch).
    let (snapshot_tx, snapshot_rx) = watch::channel(LiveSnapshot::empty());

    // Build and spawn the live tracker task.
    let tracker = build_live_tracker(&opensky_config);
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

    // The system reference point (ADR 0021): one origin for both the tracking
    // frame (above) and the I062/100 projection (below). FIREFLY_SYSTEM_REF_*
    // overrides it; otherwise it is the OpenSky bounding-box midpoint.
    let reference = live_system_reference_point(&opensky_config);

    // Spawn the OpenSky poller, feeding plots into the tracker via mpsc.
    spawn_opensky_poller_live(opensky_config, plots_tx, Arc::clone(&metrics));

    // Spawn the live CAT062 feed, if enabled.
    spawn_cat062_live(Arc::clone(&metrics), reference, snapshot_rx.clone());

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

/// Spawn the OpenSky poller in Live mode: every batch of plots is sent into
/// `plots_tx` so the tracker can consume it. If the channel is full, the batch
/// is dropped and a warning is logged — availability over back-pressure.
///
/// Sets `metrics.live_ready = true` on the first successful poll (AP9.4c-4):
/// until then `/ready` returns 503. Increments `opensky_poll_errors_total` on
/// each HTTP/network failure.
fn spawn_opensky_poller_live(
    config: OpenSkyConfig,
    plots_tx: mpsc::Sender<Vec<Plot>>,
    metrics: Arc<Metrics>,
) {
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
// Shared: CAT065 heartbeat, tracing, shutdown
// ---------------------------------------------------------------------------

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
