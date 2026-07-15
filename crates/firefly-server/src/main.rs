//! Binary entry point: start the sources-driven live tracker and serve it with
//! graceful shutdown and structured logging. The tracker's inputs come from
//! `FIREFLY_SOURCES` (orchestrated contract, ADR 0023) or the standalone
//! `FIREFLY_OPENSKY_*`/`FIREFLY_FLARM_*`/`FIREFLY_RADAR_*` env config; with no
//! active source the instance serves an empty sky and still emits the CAT065
//! heartbeat. The earlier replay/scene demo mode was removed (ADR 0030).

use std::sync::Arc;
use std::time::Duration;

use firefly_adsb021::Adsb021Config;
use firefly_adsbagg::{AdsbAggConfig, AdsbAggPoller};
use firefly_asterix::{Cat062Encoder, SensorReason};
use firefly_core::{Plot, SensorId};
use firefly_flarm::FlarmConfig;
use firefly_geo::{LocalFrame, Wgs84};
use firefly_mlat::MlatConfig;
use firefly_multicast::{MulticastConfig, SensorHealthMonitor};
use firefly_opensky::{OpenSkyConfig, OpenSkyPoller};
use firefly_radar::RadarConfig;
use firefly_server::sources;
use firefly_server::{
    build_live_tracker_multi, live_system_reference_point, registration_enabled,
    resolve_plot_recorder, router, run_live_cat062, run_live_tracker, AppState, FrameSource,
    LiveSnapshot, LiveTracker, Metrics, RadarSensor, ServerConfig, SnapshotRx,
};
use firefly_track::{ApplyPolicy, RegistrationApplier, RegistrationConfig, RegistrationMonitor};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};

#[tokio::main]
async fn main() {
    init_tracing();

    let config = ServerConfig::from_env();
    tracing::info!(
        port = config.port,
        "starting Firefly server (sources-driven live tracker)"
    );

    // The replay/scene demo mode was removed (ADR 0030). Tolerate the old knobs
    // loudly instead of failing, per the 12-factor fallback rule.
    for legacy in ["FIREFLY_MODE", "FIREFLY_SCENE", "FIREFLY_SPEED"] {
        if std::env::var_os(legacy).is_some() {
            tracing::warn!(
                var = legacy,
                "deprecated and ignored since ADR 0030 (replay/scene mode removed; the server always runs the live tracker)"
            );
        }
    }

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

    // CAT065 heartbeat: wall-clock liveness signal, independent of the sources —
    // it is what lets the ASD tell "empty sky" from "dead feed" (ADR 0018).
    spawn_cat065_heartbeat(Arc::clone(&metrics));

    let state = build_live_state(metrics, ws_token, ws_allowed_origin).await;

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
// Live tracker (the only mode, ADR 0030)
// ---------------------------------------------------------------------------

/// Build the `AppState`: create the tracker + channels, spawn one adapter per
/// configured source, spawn the live tracker task, and optionally start the
/// live CAT062 feed.
async fn build_live_state(
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

    // Meteo/QNH service (VERT.1, SDPS-003 analogue): parse and validate the
    // regional QNH configuration at startup — a configured-but-broken value
    // is fatal (the vertical chain must not silently degrade to standard
    // atmosphere). The service itself is consumed by the vertical tracking
    // (VERT.2); here it is resolved, surfaced as metrics and logged.
    let meteo = firefly_meteo::MeteoConfig::from_env().unwrap_or_else(|e| {
        tracing::error!(error = %e, "FIREFLY_METEO_QNH invalid; aborting");
        std::process::exit(1);
    });
    if meteo.regions.is_empty() {
        tracing::info!(
            "no QNH regions configured (FIREFLY_METEO_QNH unset); Mode-C altitudes will use the standard atmosphere"
        );
    } else {
        tracing::info!(
            regions = meteo.regions.len(),
            "meteo/QNH service configured (VERT.1)"
        );
    }
    metrics.meteo_qnh_regions.store(
        meteo.regions.len() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    {
        let mut gauges = metrics
            .meteo_qnh
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        gauges.clear();
        gauges.extend(meteo.regions.iter().map(|r| (r.name.clone(), r.qnh_hpa)));
    }
    let opensky = resolved.opensky;
    let adsbagg = resolved.adsbagg;
    let flarm = resolved.flarm;
    let radar = resolved.radar;
    let adsb021 = resolved.adsb021;
    let mlat = resolved.mlat;

    // With no active source adapter the tracker idles and the instance serves a
    // deliberate EMPTY SKY (plus the CAT065 heartbeat). That is the complete
    // picture then, so report ready immediately — live_ready is otherwise set
    // by the first plot of an adapter, which will never come (ADR 0030).
    if opensky.is_empty()
        && adsbagg.is_empty()
        && flarm.is_empty()
        && radar.is_empty()
        && adsb021.is_empty()
        && mlat.is_empty()
    {
        metrics
            .live_ready
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    // Representative across all sources: union bbox (tracking-frame origin), output
    // cadence, and a placeholder sensor id. FLARM and radar are folded in
    // (ADR 0026/0028).
    let representative = sources::representative_config(&opensky, &adsbagg, &flarm, &radar);
    let sensor_id = representative.sensor_id;
    let output_period = Duration::from_secs(representative.poll_interval_secs);

    // Geodetic source sensors (OpenSky, aggregator, FLARM, CAT021 station) and
    // their scan periods: the polled adapters use their poll interval, the push
    // streams (FLARM, CAT021) a nominal. These share the common tracking frame
    // (geodetic path).
    let geodetic_sensors: Vec<(SensorId, f64)> = opensky
        .iter()
        .map(|c| (c.sensor_id, c.poll_interval_secs as f64))
        .chain(
            adsbagg
                .iter()
                .map(|c| (c.sensor_id, c.poll_interval_secs as f64)),
        )
        .chain(
            flarm
                .iter()
                .map(|c| (c.sensor_id, sources::FLARM_NOMINAL_SCAN_SECS)),
        )
        .chain(
            adsb021
                .iter()
                .map(|c| (c.sensor_id, firefly_adsb021::NOMINAL_UPDATE_SECS)),
        )
        .chain(
            mlat.iter()
                .map(|c| (c.sensor_id, firefly_mlat::NOMINAL_UPDATE_SECS)),
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
        adsbagg_sources = adsbagg.len(),
        flarm_sources = flarm.len(),
        radar_sources = radar.len(),
        adsb021_sources = adsb021.len(),
        mlat_sources = mlat.len(),
        lat_min = representative.lat_min,
        lat_max = representative.lat_max,
        lon_min = representative.lon_min,
        lon_max = representative.lon_max,
        output_period_secs = representative.poll_interval_secs,
        "live mode: starting tracker over all live sources"
    );

    // Expose the configured source mix as gauges (Betriebs-Härtung): an operator
    // can confirm at a glance that the instance runs the sources the orchestrator
    // intended (ADR 0023). Set before the spawn loops consume the config vecs.
    use std::sync::atomic::Ordering;
    metrics
        .sources_opensky
        .store(opensky.len() as u64, Ordering::Relaxed);
    metrics
        .sources_adsbagg
        .store(adsbagg.len() as u64, Ordering::Relaxed);
    metrics
        .sources_flarm
        .store(flarm.len() as u64, Ordering::Relaxed);
    metrics
        .sources_radar
        .store(radar.len() as u64, Ordering::Relaxed);
    metrics
        .sources_adsb021
        .store(adsb021.len() as u64, Ordering::Relaxed);
    metrics
        .sources_mlat
        .store(mlat.len() as u64, Ordering::Relaxed);

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

    // Opt-in registration shadow monitor (REG.2a, ADR 0034): observes the same
    // plot stream as the tracker and estimates per-radar biases from
    // radar↔truth correspondences — logs/metrics only, no fusion feedback
    // (that application policy is REG.2b). Needs at least one radar source;
    // without one the flag is honoured but a no-op, and we say so.
    let registration = if registration_enabled(
        std::env::var("FIREFLY_REGISTRATION_ENABLED")
            .ok()
            .as_deref(),
    ) {
        if radar.is_empty() {
            tracing::warn!(
                "FIREFLY_REGISTRATION_ENABLED is set but no radar_asterix source is configured; the registration monitor has nothing to estimate"
            );
            None
        } else {
            let sites: std::collections::BTreeMap<SensorId, LocalFrame> = radar
                .iter()
                .map(|c| {
                    (
                        c.sensor_id,
                        LocalFrame::new(Wgs84::from_degrees(c.lat_deg, c.lon_deg, c.height_m)),
                    )
                })
                .collect();
            tracing::info!(
                radars = sites.len(),
                "registration shadow monitor enabled (REG.2a): estimating radar biases, not applying them"
            );
            Some(RegistrationMonitor::new(
                LocalFrame::new(reference),
                sites,
                RegistrationConfig::default(),
            ))
        }
    } else {
        None
    };

    // Build and spawn the live tracker task, registering every source sensor —
    // geodetic adapters on the shared frame, radar sensors on their own site frame.
    let tracker =
        build_live_tracker_multi(reference, geodetic_sensors.iter().copied(), radar_sensors);
    // Opt-in input recording (ADR 0020): FIREFLY_PLOT_RECORD_PATH names a
    // .ffplots file the ingested plot stream is written to, the restart-replay
    // recovery path (QW.4 → SDPS-002/HA). Unset → no recording; an unwritable
    // path is non-fatal (logged, tracking continues).
    let record_path = std::env::var("FIREFLY_PLOT_RECORD_PATH").ok();
    let recorder = resolve_plot_recorder(record_path.as_deref());

    // Opt-in registration APPLICATION (REG.2b, ADR 0034): subtract the gated,
    // smoothed bias estimate from radar measurements before fusion. A second,
    // deliberate switch on top of the monitor — closing a control loop into
    // the fusion path is opted into explicitly, never implied.
    let apply = registration_enabled(std::env::var("FIREFLY_REGISTRATION_APPLY").ok().as_deref());
    let mut live = LiveTracker::new(tracker, recorder);
    // VERT.2: attach the QNH service so published barometric altitudes are
    // corrected where a regional QNH is observed (I062/135 QNH bit).
    if !meteo.regions.is_empty() {
        live = live.with_meteo(meteo.into_service());
    }
    match (registration, apply) {
        (Some(monitor), true) => {
            tracing::info!(
                "registration correction enabled (REG.2b): gated bias estimates are subtracted before fusion"
            );
            live = live
                .with_registration(monitor)
                .with_registration_apply(RegistrationApplier::new(ApplyPolicy::default()));
        }
        (Some(monitor), false) => live = live.with_registration(monitor),
        (None, true) => tracing::warn!(
            "FIREFLY_REGISTRATION_APPLY is set but the registration monitor is not running (set FIREFLY_REGISTRATION_ENABLED and configure a radar_asterix source); nothing will be applied"
        ),
        (None, false) => {}
    }
    {
        let m = Arc::clone(&metrics);
        tokio::spawn(run_live_tracker(
            live,
            plots_rx,
            snapshot_tx,
            output_period,
            move |plots, records, registration, clutter_cells| {
                m.live_plots_ingested_total
                    .store(plots, std::sync::atomic::Ordering::Relaxed);
                m.plot_records_written_total
                    .store(records, std::sync::atomic::Ordering::Relaxed);
                m.clutter_cells
                    .store(clutter_cells, std::sync::atomic::Ordering::Relaxed);
                if let Some(tick) = registration {
                    m.registration_estimates_total
                        .store(tick.estimates_total, std::sync::atomic::Ordering::Relaxed);
                    m.registration_correspondences.store(
                        tick.last_pair_count as u64,
                        std::sync::atomic::Ordering::Relaxed,
                    );
                    m.registration_observable
                        .store(tick.observable, std::sync::atomic::Ordering::Relaxed);
                    let mut biases = m
                        .registration_biases
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    biases.clear();
                    biases.extend(
                        tick.biases
                            .iter()
                            .map(|(id, range_m, azimuth_deg)| (id.0, (*range_m, *azimuth_deg))),
                    );
                    drop(biases);
                    m.registration_apply_active
                        .store(tick.apply_active, std::sync::atomic::Ordering::Relaxed);
                    let mut applied = m
                        .registration_applied_biases
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    applied.clear();
                    applied.extend(
                        tick.applied
                            .iter()
                            .map(|(id, range_m, azimuth_deg)| (id.0, (*range_m, *azimuth_deg))),
                    );
                }
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
    for cfg in adsbagg {
        spawn_adsbagg_poller_live(
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
    for cfg in adsb021 {
        spawn_adsb021_listener_live(
            cfg,
            plots_tx.clone(),
            Arc::clone(&metrics),
            Arc::clone(&sensor_monitor),
        );
    }
    for cfg in mlat {
        spawn_mlat_listener_live(
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
            // Standalone/dev: every adapter is opt-in via its _ENABLED flag —
            // OpenSky (FIREFLY_OPENSKY_ENABLED), FLARM/OGN (ADR 0026,
            // FIREFLY_FLARM_ENABLED) and radar ASTERIX (ADR 0028,
            // FIREFLY_RADAR_ENABLED). Since the live tracker became the ONLY
            // mode (ADR 0030) a bare start must not poll external services
            // unasked: nothing enabled → idle tracker, empty sky + heartbeat.
            let opensky_cfg = OpenSkyConfig::from_env();
            let opensky = if opensky_cfg.enabled {
                vec![opensky_cfg]
            } else {
                Vec::new()
            };
            let adsbagg_cfg = AdsbAggConfig::from_env();
            let adsbagg = if adsbagg_cfg.enabled {
                vec![adsbagg_cfg]
            } else {
                Vec::new()
            };
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
            let adsb021_cfg = Adsb021Config::from_env();
            let adsb021 = if adsb021_cfg.enabled {
                vec![adsb021_cfg]
            } else {
                Vec::new()
            };
            let mlat_cfg = MlatConfig::from_env();
            let mlat = if mlat_cfg.enabled {
                vec![mlat_cfg]
            } else {
                Vec::new()
            };
            return sources::ResolvedSources {
                opensky,
                adsbagg,
                flarm,
                radar,
                adsb021,
                mlat,
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
    if resolved.opensky.is_empty()
        && resolved.adsbagg.is_empty()
        && resolved.flarm.is_empty()
        && resolved.radar.is_empty()
        && resolved.adsb021.is_empty()
        && resolved.mlat.is_empty()
    {
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
        // A second monitor handle for the error path: the success closure moves
        // `sensor_monitor`, so the failure classifier needs its own clone.
        let monitor_err = Arc::clone(&sensor_monitor);
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
                        metrics
                            .live_plot_batches_dropped_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        tracing::warn!("plot channel full; dropping batch: {e}");
                    }
                },
                move |e| {
                    tracing::warn!(%e, "OpenSky poll error (counted in metrics)");
                    metrics_err
                        .opensky_poll_errors_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    // A 429 is also counted separately so a rate limit is
                    // distinguishable from generic poll failures (ADR 0029 follow-up).
                    if e.is_rate_limited() {
                        metrics_err
                            .opensky_rate_limited_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    // Record the failure reason so a degraded sensor carries it in
                    // CAT063 I063/RE (ADR 0033): unreachable vs. auth vs. rate-limit.
                    monitor_err.record_failure(
                        sensor_id,
                        classify_poll_reason(e.is_rate_limited(), e.is_auth()),
                    );
                },
            )
            .await;
    });
}

/// Spawn the community-aggregator ADS-B poller in Live mode (ADR 0031): every
/// batch of plots is sent into `plots_tx` so the tracker can consume it. If the
/// channel is full, the batch is dropped and a warning is logged — availability
/// over back-pressure.
///
/// Sets `metrics.live_ready = true` on the first successful poll; increments
/// `adsbagg_poll_errors_total` on each failure and `adsbagg_rate_limited_total`
/// on the 429 subset (mirrors the OpenSky poller, #49). Notifies
/// `sensor_monitor` on each successful batch (CAT063 per-sensor liveness).
fn spawn_adsbagg_poller_live(
    config: AdsbAggConfig,
    plots_tx: mpsc::Sender<Vec<Plot>>,
    metrics: Arc<Metrics>,
    sensor_monitor: Arc<SensorHealthMonitor>,
) {
    let sensor_id = config.sensor_id;
    let poller = AdsbAggPoller::new(config.clone());
    let (lat, lon, radius_nm) = poller.query_summary();
    tracing::info!(
        provider = %config.provider,
        lat,
        lon,
        radius_nm,
        poll_interval_secs = config.poll_interval_secs,
        "community-aggregator ADS-B poller started (live mode)"
    );
    tokio::spawn(async move {
        let metrics_err = Arc::clone(&metrics);
        // Own monitor handle for the error path (the success closure moves the
        // original `sensor_monitor`).
        let monitor_err = Arc::clone(&sensor_monitor);
        poller
            .run(
                move |plots| {
                    tracing::info!(count = plots.len(), "aggregator plots received");
                    // Mark the server as ready on the first successful poll.
                    metrics
                        .live_ready
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    // Notify the sensor health monitor: this sensor is alive.
                    sensor_monitor.record_activity(sensor_id, std::time::Instant::now());
                    if let Err(e) = plots_tx.try_send(plots) {
                        metrics
                            .live_plot_batches_dropped_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        tracing::warn!("plot channel full; dropping batch: {e}");
                    }
                },
                move |e| {
                    tracing::warn!(%e, "aggregator poll error (counted in metrics)");
                    metrics_err
                        .adsbagg_poll_errors_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if e.is_rate_limited() {
                        metrics_err
                            .adsbagg_rate_limited_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    // Record the degradation reason for CAT063 I063/RE (ADR 0033).
                    monitor_err.record_failure(
                        sensor_id,
                        classify_poll_reason(e.is_rate_limited(), e.is_auth()),
                    );
                },
            )
            .await;
    });
}

/// Map a poll error's `(rate_limited, auth)` flags to a [`SensorReason`] for
/// CAT063 I063/RE (ADR 0033). Priority: a rate limit is the most specific signal,
/// then an auth failure; anything else (transport/DNS/timeout/5xx) is
/// `Unreachable`. Shared by the OpenSky and aggregator error paths so both
/// classify identically.
fn classify_poll_reason(is_rate_limited: bool, is_auth: bool) -> SensorReason {
    if is_rate_limited {
        SensorReason::RateLimited
    } else if is_auth {
        SensorReason::Auth
    } else {
        SensorReason::Unreachable
    }
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
                metrics
                    .live_plot_batches_dropped_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
        let plot_metrics = Arc::clone(&metrics);
        let plot_monitor = Arc::clone(&sensor_monitor);
        // The scan-period estimator (FEP.1): measures the antenna's true
        // rotation from CAT034 north markers, feeding the CAT063 liveness
        // threshold and the per-sensor metric. Owned by this task — one
        // estimator per radar source.
        let mut estimator = firefly_radar::ScanPeriodEstimator::new();
        firefly_radar::run(
            &config,
            move |plots| {
                plot_metrics
                    .live_ready
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                plot_metrics
                    .radar_plots_received_total
                    .fetch_add(plots.len() as u64, std::sync::atomic::Ordering::Relaxed);
                plot_monitor.record_activity(sensor_id, std::time::Instant::now());
                if let Err(e) = plots_tx.try_send(plots) {
                    plot_metrics
                        .live_plot_batches_dropped_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    tracing::warn!("plot channel full; dropping radar batch: {e}");
                }
            },
            move |messages| {
                // Any service message proves the sensor alive — that is the
                // point of FEP.1: liveness independent of traffic.
                sensor_monitor.record_activity(sensor_id, std::time::Instant::now());
                for message in &messages {
                    if message.message_type != firefly_asterix::ServiceMessageType::NorthMarker {
                        continue;
                    }
                    metrics
                        .radar_north_markers_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(time) = message.time else { continue };
                    let before = estimator.period_secs();
                    if let Some(period) = estimator.observe_north_marker(time.as_secs()) {
                        sensor_monitor.update_scan_period(sensor_id, period);
                        metrics
                            .radar_scan_periods
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .insert(sensor_id.0, period);
                        if before.is_none() {
                            tracing::info!(
                                sensor_id = sensor_id.0,
                                period_s = format!("{period:.2}").as_str(),
                                "radar scan period measured from CAT034 north markers (FEP.1)"
                            );
                        }
                    }
                }
            },
        )
        .await;
    });
}

/// Spawn the ADS-B ground-station (CAT021) UDP listener in Live mode (FEP.3):
/// each decoded datagram's plots are sent into `plots_tx` so the tracker fuses
/// them with the radar/internet-ADS-B inputs. A full channel drops the batch
/// with a warning (availability over back-pressure).
///
/// Sets `metrics.live_ready = true` on the first batch and counts reports in
/// `adsb021_reports_received_total`. Notifies `sensor_monitor` so the station's
/// liveness is tracked (CAT063). The listener never returns under normal
/// operation; a bind failure logs and returns (other sources keep running).
fn spawn_adsb021_listener_live(
    config: Adsb021Config,
    plots_tx: mpsc::Sender<Vec<Plot>>,
    metrics: Arc<Metrics>,
    sensor_monitor: Arc<SensorHealthMonitor>,
) {
    let sensor_id = config.sensor_id;
    tokio::spawn(async move {
        firefly_adsb021::run(&config, move |plots| {
            metrics
                .live_ready
                .store(true, std::sync::atomic::Ordering::Relaxed);
            metrics
                .adsb021_reports_received_total
                .fetch_add(plots.len() as u64, std::sync::atomic::Ordering::Relaxed);
            sensor_monitor.record_activity(sensor_id, std::time::Instant::now());
            if let Err(e) = plots_tx.try_send(plots) {
                metrics
                    .live_plot_batches_dropped_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tracing::warn!("plot channel full; dropping ADS-B CAT021 batch: {e}");
            }
        })
        .await;
    });
}

/// Spawn the WAM/MLAT (CAT020/019) UDP listener in Live mode (FEP.5): each
/// decoded datagram's plots are sent into `plots_tx` so the tracker fuses
/// them with the radar/ADS-B inputs. A full channel drops the batch with a
/// warning (availability over back-pressure).
///
/// Sets `metrics.live_ready = true` on the first batch and counts reports in
/// `mlat_reports_received_total`. Notifies `sensor_monitor` on plots **and**
/// on CAT019 status messages — the MLAT system stays visibly alive under an
/// empty sky (liveness without traffic, like a radar's service messages).
fn spawn_mlat_listener_live(
    config: MlatConfig,
    plots_tx: mpsc::Sender<Vec<Plot>>,
    metrics: Arc<Metrics>,
    sensor_monitor: Arc<SensorHealthMonitor>,
) {
    let sensor_id = config.sensor_id;
    tokio::spawn(async move {
        let status_monitor = Arc::clone(&sensor_monitor);
        firefly_mlat::run(
            &config,
            move |plots| {
                metrics
                    .live_ready
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                metrics
                    .mlat_reports_received_total
                    .fetch_add(plots.len() as u64, std::sync::atomic::Ordering::Relaxed);
                sensor_monitor.record_activity(sensor_id, std::time::Instant::now());
                if let Err(e) = plots_tx.try_send(plots) {
                    metrics
                        .live_plot_batches_dropped_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    tracing::warn!("plot channel full; dropping WAM/MLAT batch: {e}");
                }
            },
            move |messages| {
                // Any status message proves the MLAT system alive — liveness
                // independent of traffic. A NOGO/degraded self-report is
                // surfaced in the log; the per-sensor staleness still keys
                // off actual reception.
                status_monitor.record_activity(sensor_id, std::time::Instant::now());
                if messages.iter().any(|m| m.operational == Some(false)) {
                    tracing::warn!(
                        sensor_id = sensor_id.0,
                        "WAM/MLAT system reports itself degraded/NOGO (CAT019)"
                    );
                }
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
        // The applied registration corrections (REG.2b) travel to the wire via
        // the tick-updated metrics map — the CAT063 record then tells consumers
        // what the picture is corrected by (REG.3, ADR 0034). Empty (no items
        // emitted) unless FIREFLY_REGISTRATION_APPLY engaged a correction.
        let bias_metrics = Arc::clone(&metrics);
        let result = firefly_multicast::run_cat063_sender(
            &socket,
            destination,
            &encoder,
            monitor,
            period,
            utc_time_of_day_secs,
            move || {
                bias_metrics
                    .registration_applied_biases
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .iter()
                    .map(|(sensor, (range_m, azimuth_deg))| {
                        (
                            *sensor,
                            firefly_asterix::SsrBias {
                                range_gain: 0.0,
                                range_bias_m: *range_m,
                                azimuth_bias_deg: *azimuth_deg,
                            },
                        )
                    })
                    .collect()
            },
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
