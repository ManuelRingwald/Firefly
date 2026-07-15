//! Prometheus `/metrics` endpoint (REQ NFR-OBS-001): exposes operational
//! counters/gauges for track throughput, the CAT062 multicast feed and
//! connected WebSocket clients.
//!
//! Counters are plain atomics rather than a metrics crate — the set of
//! metrics is small and fixed, and a hand-rolled exposition keeps the
//! dependency surface minimal.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Mutex;

/// Process-wide counters/gauges, shared via [`crate::AppState`] and the
/// CAT062 multicast task.
#[derive(Debug, Default)]
pub struct Metrics {
    /// Number of currently connected WebSocket clients (gauge).
    pub ws_clients_connected: AtomicI64,
    /// Total number of WebSocket clients that have ever connected (counter).
    pub ws_clients_total: AtomicU64,
    /// Total number of CAT062 data blocks (scans) sent over multicast (counter).
    pub cat062_scans_sent_total: AtomicU64,
    /// Total number of CAT062 multicast send errors (counter).
    pub cat062_send_errors_total: AtomicU64,
    /// Total number of CAT065 SDPS-status heartbeats sent (counter, ADR 0018).
    pub cat065_heartbeats_sent_total: AtomicU64,
    /// Number of confirmed tracks in the most recently sent CAT062 scan (gauge,
    /// SDPS-006). Updated after each successful multicast send via the on_scan
    /// callback in firefly_multicast::run. Stays at 0 until the first scan.
    pub tracks_active: AtomicU64,

    // --- Live-mode metrics (ADR 0020, AP9.4c-4) ---
    /// Whether the Live-mode server has received at least one successful
    /// OpenSky poll (i.e. at least one airborne aircraft was returned). Used
    /// by the `/ready` probe: the server is not ready until the first real
    /// air picture arrives. Always `false` in Replay mode.
    pub live_ready: AtomicBool,
    /// Total number of plots ingested by the live tracker (counter). Stays 0
    /// in Replay mode.
    pub live_plots_ingested_total: AtomicU64,
    /// Total number of `.ffplots` input-recording records written (counter).
    /// Stays 0 when recording is disabled or in Replay mode.
    pub plot_records_written_total: AtomicU64,
    /// Total number of OpenSky poll errors (HTTP / network failures, counter).
    /// Stays 0 in Replay mode (OpenSky poller not started).
    pub opensky_poll_errors_total: AtomicU64,
    /// Total number of OpenSky polls rejected with HTTP 429 (rate limit, counter).
    /// A **subset** of `opensky_poll_errors_total`, split out so a rate limit is
    /// distinguishable from generic failures; each 429 also triggers an
    /// exponential backoff of the poll loop. Stays 0 in Replay mode.
    pub opensky_rate_limited_total: AtomicU64,
    /// Total number of community-aggregator (adsb.lol/adsb.fi) poll errors
    /// (HTTP / network failures, counter; ADR 0031). Stays 0 without an
    /// `adsb_aggregator` source.
    pub adsbagg_poll_errors_total: AtomicU64,
    /// Total number of aggregator polls rejected with HTTP 429 (rate limit,
    /// counter) — a **subset** of `adsbagg_poll_errors_total`, split out like
    /// the OpenSky twin so a rate limit is distinguishable from generic
    /// failures. Each 429 also triggers the poll loop's exponential backoff.
    pub adsbagg_rate_limited_total: AtomicU64,
    /// Total number of FLARM/OGN position plots received from the APRS-IS stream
    /// (counter, ADR 0026). Stays 0 in Replay mode or without a `flarm_aprs`
    /// source.
    pub flarm_plots_received_total: AtomicU64,
    /// Total number of radar ASTERIX (CAT048) plots decoded from the UDP feed
    /// (counter, ADR 0028). Stays 0 in Replay mode or without a `radar_asterix`
    /// source.
    pub radar_plots_received_total: AtomicU64,
    /// Total number of ADS-B CAT021 reports decoded from the ground-station UDP
    /// feed that became plots (counter, FEP.3). Stays 0 without an
    /// `adsb_asterix` source.
    pub adsb021_reports_received_total: AtomicU64,
    /// Total number of WAM/MLAT CAT020 reports decoded from the UDP feed that
    /// became plots (counter, FEP.5). Stays 0 without a `mlat_asterix`
    /// source.
    pub mlat_reports_received_total: AtomicU64,

    // --- Sensor health metrics (Firefly #32, CAT063) ---
    /// Total number of CAT063 sensor status blocks sent over multicast (counter).
    pub cat063_status_sent_total: AtomicU64,
    /// Number of registered sensors known to the SDPS (gauge, static).
    pub sensors_total: AtomicU64,
    /// Number of sensors currently active — received a plot within their
    /// staleness window (gauge, updated on each CAT063 send).
    pub sensors_active: AtomicU64,

    // --- Live-pipeline hardening (Betriebs-Härtung: Lastfestigkeit) ---
    /// Total number of plot batches **dropped** because the source→tracker channel
    /// was full (counter). A non-zero, growing value means the tracker cannot keep
    /// up with the source rate and surveillance data is being lost to back-pressure
    /// — the operator's signal to scale up or throttle. Stays 0 outside Live mode.
    pub live_plot_batches_dropped_total: AtomicU64,
    /// Number of configured `adsb_opensky` sources for this instance (gauge, static
    /// per process). Lets an operator confirm at a glance that the instance is
    /// running the source mix the orchestrator intended (ADR 0023).
    pub sources_opensky: AtomicU64,
    /// Number of configured `adsb_aggregator` sources (gauge, static per
    /// process; ADR 0031).
    pub sources_adsbagg: AtomicU64,
    /// Number of configured `flarm_aprs` sources (gauge, static per process).
    pub sources_flarm: AtomicU64,
    /// Number of configured `radar_asterix` sources (gauge, static per process).
    pub sources_radar: AtomicU64,
    /// Number of configured `adsb_asterix` (CAT021 ground station) sources
    /// (gauge, static per process; FEP.3).
    pub sources_adsb021: AtomicU64,
    /// Number of configured `mlat_asterix` (WAM/MLAT) sources (gauge, static
    /// per process; FEP.5).
    pub sources_mlat: AtomicU64,
    /// Learned spatial clutter-map cells across all sensors (SPEC.2b,
    /// FR-TRK-046) — a growing value means the tracker is mapping hotspots.
    pub clutter_cells: AtomicU64,
    /// Loaded flight plans (FPL.1, FR-TRK-047).
    pub flight_plans: AtomicU64,
    /// Tracks correlated to a flight plan in the latest snapshot.
    pub tracks_correlated: AtomicU64,
    /// Squawk-correlation refusals in the latest snapshot (duplicate code,
    /// conspicuity 1000 or identity conflict) — the "needs manual
    /// correlation" signal (ADR 0038 rule 4).
    pub correlation_refused: AtomicU64,

    // --- Registration shadow monitor (REG.2a, ADR 0034) ---
    /// Total number of registration bias estimates produced by the shadow
    /// monitor (counter). Stays 0 unless `FIREFLY_REGISTRATION_ENABLED` is set
    /// and enough radar↔truth correspondences accumulate.
    pub registration_estimates_total: AtomicU64,
    /// Correspondences found by the most recent registration estimation
    /// attempt (gauge) — also updated when the attempt was refused for too
    /// little evidence, so an operator can see *why* no estimate appears.
    pub registration_correspondences: AtomicU64,
    /// Whether the latest registration estimate was fully observable (gauge,
    /// 0/1). 0 before the first estimate and while the sensor geometry is
    /// rank-deficient (e.g. co-located radars without a geodetic reference).
    pub registration_observable: AtomicBool,
    /// Latest per-radar bias estimates from the shadow monitor: sensor id →
    /// (range bias m, azimuth bias deg). Rendered as labelled gauges. A Mutex
    /// (not atomics) because the map is small, updated once per output tick
    /// and read once per /metrics scrape.
    pub registration_biases: Mutex<BTreeMap<u16, (f64, f64)>>,
    /// Whether a REG.2b registration correction is currently in effect (gauge,
    /// 0/1). Stays 0 without `FIREFLY_REGISTRATION_APPLY` or while the apply
    /// gate (observability, residual gain, plausibility) rejects every run.
    pub registration_apply_active: AtomicBool,
    /// The correction currently **applied** per radar (REG.2b): sensor id →
    /// (range bias m, azimuth bias deg). Distinct from `registration_biases`
    /// (the raw estimate) — this smoothed, gated value is what is actually
    /// subtracted from measurements. Rendered as labelled gauges.
    pub registration_applied_biases: Mutex<BTreeMap<u16, (f64, f64)>>,

    // --- Meteo/QNH service (VERT.1, SDPS-003 analogue) ---
    /// Number of configured QNH regions (gauge, static per process). 0 means
    /// the vertical chain works on the standard atmosphere everywhere.
    pub meteo_qnh_regions: AtomicU64,
    /// The configured QNH per region: region name → hPa. Rendered as
    /// labelled gauges; absent when no region is configured (an absent
    /// series is clearer than a misleading standard-atmosphere line).
    pub meteo_qnh: Mutex<BTreeMap<String, f64>>,

    // --- Radar service messages (FEP.1, CAT034) ---
    /// Total CAT034 north markers received across all radar sources (counter).
    /// Stays 0 without a `radar_asterix` source or when the radar sends no
    /// service messages.
    pub radar_north_markers_total: AtomicU64,
    /// The **measured** antenna scan period per radar (FEP.1): sensor id →
    /// seconds per revolution, estimated from north-marker intervals. Rendered
    /// as labelled gauges; absent until the first measurement.
    pub radar_scan_periods: Mutex<BTreeMap<u16, f64>>,
}

/// A guard that increments `ws_clients_connected` (and `ws_clients_total`) on
/// creation and decrements `ws_clients_connected` again when dropped, so a
/// client is counted as connected for exactly the lifetime of its WebSocket.
pub struct ConnectedClientGuard<'a> {
    metrics: &'a Metrics,
}

impl<'a> ConnectedClientGuard<'a> {
    pub fn new(metrics: &'a Metrics) -> Self {
        metrics.ws_clients_connected.fetch_add(1, Ordering::Relaxed);
        metrics.ws_clients_total.fetch_add(1, Ordering::Relaxed);
        Self { metrics }
    }
}

impl Drop for ConnectedClientGuard<'_> {
    fn drop(&mut self) {
        self.metrics
            .ws_clients_connected
            .fetch_sub(1, Ordering::Relaxed);
    }
}

/// Render `metrics` plus the static `frames_total` gauge as a Prometheus text
/// exposition (version 0.0.4).
pub fn render(metrics: &Metrics) -> String {
    let mut out = String::new();
    write_metric(
        &mut out,
        "firefly_ws_clients_connected",
        "gauge",
        "Number of currently connected WebSocket clients.",
        metrics.ws_clients_connected.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_ws_clients_total",
        "counter",
        "Total number of WebSocket clients that have ever connected.",
        metrics.ws_clients_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_cat062_scans_sent_total",
        "counter",
        "Total number of CAT062 data blocks sent over multicast.",
        metrics.cat062_scans_sent_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_cat062_send_errors_total",
        "counter",
        "Total number of CAT062 multicast send errors.",
        metrics.cat062_send_errors_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_cat065_heartbeats_sent_total",
        "counter",
        "Total number of CAT065 SDPS-status heartbeats sent over multicast.",
        metrics.cat065_heartbeats_sent_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_tracks_active",
        "gauge",
        "Number of tracks in the most recently sent CAT062 scan (SDPS-006).",
        metrics.tracks_active.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_live_plots_ingested_total",
        "counter",
        "Total number of plots ingested by the live tracker (Live mode only).",
        metrics.live_plots_ingested_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_plot_records_written_total",
        "counter",
        "Total number of .ffplots input-recording records written (Live mode only).",
        metrics.plot_records_written_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_opensky_poll_errors_total",
        "counter",
        "Total number of OpenSky REST API poll errors (Live mode only).",
        metrics.opensky_poll_errors_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_opensky_rate_limited_total",
        "counter",
        "Total number of OpenSky polls rejected with HTTP 429 (subset of poll errors; Live mode only).",
        metrics.opensky_rate_limited_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_adsbagg_poll_errors_total",
        "counter",
        "Total number of community-aggregator (adsb.lol/adsb.fi) poll errors (Live mode only).",
        metrics.adsbagg_poll_errors_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_adsbagg_rate_limited_total",
        "counter",
        "Total number of aggregator polls rejected with HTTP 429 (subset of poll errors; Live mode only).",
        metrics.adsbagg_rate_limited_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_flarm_plots_received_total",
        "counter",
        "Total number of FLARM/OGN plots received from APRS-IS (Live mode only).",
        metrics.flarm_plots_received_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_radar_plots_received_total",
        "counter",
        "Total number of radar ASTERIX (CAT048) plots decoded from UDP (Live mode only).",
        metrics.radar_plots_received_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_adsb021_reports_received_total",
        "counter",
        "Total number of ADS-B CAT021 reports decoded from the ground-station UDP feed (Live mode only).",
        metrics
            .adsb021_reports_received_total
            .load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_mlat_reports_received_total",
        "counter",
        "Total number of WAM/MLAT CAT020 reports decoded from the UDP feed (Live mode only).",
        metrics.mlat_reports_received_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_live_plot_batches_dropped_total",
        "counter",
        "Plot batches dropped because the source->tracker channel was full (back-pressure loss).",
        metrics
            .live_plot_batches_dropped_total
            .load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_sources_opensky",
        "gauge",
        "Number of configured adsb_opensky sources for this instance.",
        metrics.sources_opensky.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_sources_adsbagg",
        "gauge",
        "Number of configured adsb_aggregator sources for this instance.",
        metrics.sources_adsbagg.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_sources_flarm",
        "gauge",
        "Number of configured flarm_aprs sources for this instance.",
        metrics.sources_flarm.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_sources_radar",
        "gauge",
        "Number of configured radar_asterix sources for this instance.",
        metrics.sources_radar.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_sources_adsb021",
        "gauge",
        "Number of configured adsb_asterix (CAT021 ground station) sources for this instance.",
        metrics.sources_adsb021.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_flight_plans",
        "gauge",
        "Loaded flight plans (FPL.1).",
        metrics.flight_plans.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_tracks_correlated",
        "gauge",
        "Tracks correlated to a flight plan in the latest snapshot (FPL.1).",
        metrics.tracks_correlated.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_correlation_refused",
        "gauge",
        "Squawk-correlation refusals in the latest snapshot (duplicate/conspicuity/identity conflict).",
        metrics.correlation_refused.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_clutter_cells",
        "gauge",
        "Learned spatial clutter-map cells across all sensors (SPEC.2b).",
        metrics.clutter_cells.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_sources_mlat",
        "gauge",
        "Number of configured mlat_asterix (WAM/MLAT) sources for this instance.",
        metrics.sources_mlat.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_cat063_status_sent_total",
        "counter",
        "Total number of CAT063 sensor status blocks sent over multicast.",
        metrics.cat063_status_sent_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_sensors_total",
        "gauge",
        "Number of registered sensors known to the SDPS.",
        metrics.sensors_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_sensors_active",
        "gauge",
        "Number of sensors currently active (received a recent plot).",
        metrics.sensors_active.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_registration_estimates_total",
        "counter",
        "Total registration bias estimates produced by the shadow monitor (REG.2a).",
        metrics.registration_estimates_total.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_registration_correspondences",
        "gauge",
        "Correspondences found by the most recent registration estimation attempt.",
        metrics.registration_correspondences.load(Ordering::Relaxed) as f64,
    );
    write_metric(
        &mut out,
        "firefly_registration_observable",
        "gauge",
        "1 if the latest registration estimate was fully observable, else 0.",
        f64::from(metrics.registration_observable.load(Ordering::Relaxed)),
    );
    write_metric(
        &mut out,
        "firefly_registration_apply_active",
        "gauge",
        "1 if a REG.2b registration correction is currently applied to radar measurements, else 0.",
        f64::from(metrics.registration_apply_active.load(Ordering::Relaxed)),
    );
    write_bias_gauges(
        &mut out,
        "firefly_registration_bias",
        "estimated",
        &metrics
            .registration_biases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    write_bias_gauges(
        &mut out,
        "firefly_registration_applied_bias",
        "applied (subtracted from measurements)",
        &metrics
            .registration_applied_biases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    write_metric(
        &mut out,
        "firefly_meteo_qnh_regions",
        "gauge",
        "Number of configured QNH regions (VERT.1); 0 = standard atmosphere everywhere.",
        metrics.meteo_qnh_regions.load(Ordering::Relaxed) as f64,
    );
    // Labelled per-region QNH gauges, rendered by hand like the bias
    // gauges — absent when no region is configured.
    {
        let qnh = metrics
            .meteo_qnh
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !qnh.is_empty() {
            out.push_str(
                "# HELP firefly_meteo_qnh_hpa Configured QNH per region, hectopascal (VERT.1).\n# TYPE firefly_meteo_qnh_hpa gauge\n",
            );
            for (region, hpa) in qnh.iter() {
                out.push_str(&format!(
                    "firefly_meteo_qnh_hpa{{region=\"{region}\"}} {hpa}\n"
                ));
            }
        }
    }
    write_metric(
        &mut out,
        "firefly_radar_north_markers_total",
        "counter",
        "Total CAT034 north markers received across all radar sources (FEP.1).",
        metrics.radar_north_markers_total.load(Ordering::Relaxed) as f64,
    );
    // Labelled per-sensor gauges, rendered by hand like the bias gauges —
    // absent until a period has actually been measured.
    let periods = metrics
        .radar_scan_periods
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !periods.is_empty() {
        out.push_str(
            "# HELP firefly_radar_scan_period_seconds Measured antenna scan period per radar, seconds per revolution (from CAT034 north markers).\n# TYPE firefly_radar_scan_period_seconds gauge\n",
        );
        for (sensor, period) in periods.iter() {
            out.push_str(&format!(
                "firefly_radar_scan_period_seconds{{sensor=\"{sensor}\"}} {period}\n"
            ));
        }
    }
    out
}

/// Render a pair of labelled per-sensor bias gauges (`<base>_range_m` and
/// `<base>_azimuth_deg`) from a sensor → (range m, azimuth deg) map. Labels
/// are outside [`write_metric`]'s fixed layout, so these are rendered by hand
/// — and only when values exist: an absent series is clearer than a
/// misleading 0-bias line.
fn write_bias_gauges(
    out: &mut String,
    base: &str,
    qualifier: &str,
    biases: &BTreeMap<u16, (f64, f64)>,
) {
    if biases.is_empty() {
        return;
    }
    out.push_str(&format!(
        "# HELP {base}_range_m Latest {qualifier} range bias per radar sensor, metres.\n# TYPE {base}_range_m gauge\n"
    ));
    for (sensor, (range_m, _)) in biases.iter() {
        out.push_str(&format!(
            "{base}_range_m{{sensor=\"{sensor}\"}} {range_m}\n"
        ));
    }
    out.push_str(&format!(
        "# HELP {base}_azimuth_deg Latest {qualifier} azimuth bias per radar sensor, degrees.\n# TYPE {base}_azimuth_deg gauge\n"
    ));
    for (sensor, (_, azimuth_deg)) in biases.iter() {
        out.push_str(&format!(
            "{base}_azimuth_deg{{sensor=\"{sensor}\"}} {azimuth_deg}\n"
        ));
    }
}

fn write_metric(out: &mut String, name: &str, typ: &str, help: &str, value: f64) {
    out.push_str(&format!("# HELP {name} {help}\n"));
    out.push_str(&format!("# TYPE {name} {typ}\n"));
    out.push_str(&format!("{name} {value}\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exposition contains HELP/TYPE/value lines for every metric, in
    /// Prometheus text format. REQ: NFR-OBS-001
    #[test]
    fn render_includes_all_metrics() {
        let metrics = Metrics::default();
        metrics.ws_clients_connected.store(2, Ordering::Relaxed);
        metrics.ws_clients_total.store(5, Ordering::Relaxed);
        metrics.cat062_scans_sent_total.store(42, Ordering::Relaxed);
        metrics.cat062_send_errors_total.store(1, Ordering::Relaxed);
        metrics
            .cat065_heartbeats_sent_total
            .store(13, Ordering::Relaxed);
        metrics.tracks_active.store(7, Ordering::Relaxed);
        metrics
            .live_plots_ingested_total
            .store(100, Ordering::Relaxed);
        metrics
            .plot_records_written_total
            .store(100, Ordering::Relaxed);
        metrics
            .opensky_poll_errors_total
            .store(3, Ordering::Relaxed);
        metrics
            .opensky_rate_limited_total
            .store(2, Ordering::Relaxed);
        metrics
            .adsbagg_poll_errors_total
            .store(6, Ordering::Relaxed);
        metrics
            .adsbagg_rate_limited_total
            .store(5, Ordering::Relaxed);
        metrics
            .flarm_plots_received_total
            .store(11, Ordering::Relaxed);
        metrics
            .radar_plots_received_total
            .store(17, Ordering::Relaxed);
        metrics
            .adsb021_reports_received_total
            .store(23, Ordering::Relaxed);
        metrics
            .mlat_reports_received_total
            .store(29, Ordering::Relaxed);
        metrics
            .live_plot_batches_dropped_total
            .store(4, Ordering::Relaxed);
        metrics.sources_opensky.store(1, Ordering::Relaxed);
        metrics.sources_adsbagg.store(1, Ordering::Relaxed);
        metrics.sources_flarm.store(1, Ordering::Relaxed);
        metrics.sources_radar.store(2, Ordering::Relaxed);
        metrics.sources_adsb021.store(1, Ordering::Relaxed);
        metrics.sources_mlat.store(1, Ordering::Relaxed);
        metrics.cat063_status_sent_total.store(7, Ordering::Relaxed);
        metrics.sensors_total.store(3, Ordering::Relaxed);
        metrics.sensors_active.store(2, Ordering::Relaxed);
        metrics
            .registration_estimates_total
            .store(9, Ordering::Relaxed);
        metrics
            .registration_correspondences
            .store(24, Ordering::Relaxed);
        metrics
            .registration_observable
            .store(true, Ordering::Relaxed);
        metrics
            .registration_biases
            .lock()
            .unwrap()
            .insert(301, (150.25, 0.3));
        metrics
            .registration_apply_active
            .store(true, Ordering::Relaxed);
        metrics
            .registration_applied_biases
            .lock()
            .unwrap()
            .insert(301, (148.5, 0.29));
        metrics
            .radar_north_markers_total
            .store(120, Ordering::Relaxed);
        metrics.radar_scan_periods.lock().unwrap().insert(301, 4.7);
        metrics.meteo_qnh_regions.store(2, Ordering::Relaxed);
        metrics
            .meteo_qnh
            .lock()
            .unwrap()
            .insert("EDDF".into(), 1008.0);

        let text = render(&metrics);

        assert!(text.contains("firefly_ws_clients_connected 2"));
        assert!(text.contains("firefly_ws_clients_total 5"));
        assert!(text.contains("firefly_cat062_scans_sent_total 42"));
        assert!(text.contains("firefly_cat062_send_errors_total 1"));
        assert!(text.contains("firefly_cat065_heartbeats_sent_total 13"));
        assert!(text.contains("firefly_tracks_active 7"));
        assert!(text.contains("firefly_live_plots_ingested_total 100"));
        assert!(text.contains("firefly_plot_records_written_total 100"));
        assert!(text.contains("firefly_opensky_poll_errors_total 3"));
        assert!(text.contains("firefly_opensky_rate_limited_total 2"));
        assert!(text.contains("firefly_adsbagg_poll_errors_total 6"));
        assert!(text.contains("firefly_adsbagg_rate_limited_total 5"));
        assert!(text.contains("firefly_flarm_plots_received_total 11"));
        assert!(text.contains("firefly_radar_plots_received_total 17"));
        assert!(text.contains("firefly_adsb021_reports_received_total 23"));
        assert!(text.contains("firefly_mlat_reports_received_total 29"));
        assert!(text.contains("firefly_live_plot_batches_dropped_total 4"));
        assert!(text.contains("firefly_sources_opensky 1"));
        assert!(text.contains("firefly_sources_adsbagg 1"));
        assert!(text.contains("firefly_sources_flarm 1"));
        assert!(text.contains("firefly_sources_radar 2"));
        assert!(text.contains("# TYPE firefly_sources_radar gauge"));
        assert!(text.contains("firefly_sources_adsb021 1"));
        assert!(text.contains("firefly_sources_mlat 1"));
        assert!(text.contains("firefly_cat063_status_sent_total 7"));
        assert!(text.contains("firefly_sensors_total 3"));
        assert!(text.contains("firefly_sensors_active 2"));
        assert!(text.contains("# TYPE firefly_ws_clients_connected gauge"));
        assert!(text.contains("# TYPE firefly_cat062_scans_sent_total counter"));
        assert!(text.contains("# TYPE firefly_tracks_active gauge"));
        assert!(text.contains("# TYPE firefly_live_plots_ingested_total counter"));
        assert!(text.contains("# TYPE firefly_opensky_poll_errors_total counter"));
        assert!(text.contains("# TYPE firefly_cat063_status_sent_total counter"));
        assert!(text.contains("# TYPE firefly_sensors_total gauge"));
        assert!(text.contains("# TYPE firefly_sensors_active gauge"));
        assert!(text.contains("firefly_registration_estimates_total 9"));
        assert!(text.contains("firefly_registration_correspondences 24"));
        assert!(text.contains("firefly_registration_observable 1"));
        assert!(text.contains("firefly_registration_bias_range_m{sensor=\"301\"} 150.25"));
        assert!(text.contains("firefly_registration_bias_azimuth_deg{sensor=\"301\"} 0.3"));
        assert!(text.contains("firefly_registration_apply_active 1"));
        assert!(text.contains("firefly_registration_applied_bias_range_m{sensor=\"301\"} 148.5"));
        assert!(text.contains("firefly_registration_applied_bias_azimuth_deg{sensor=\"301\"} 0.29"));
        assert!(text.contains("# TYPE firefly_registration_estimates_total counter"));
        assert!(text.contains("# TYPE firefly_registration_bias_range_m gauge"));
        assert!(text.contains("# TYPE firefly_registration_applied_bias_range_m gauge"));
        assert!(text.contains("firefly_radar_north_markers_total 120"));
        assert!(text.contains("firefly_radar_scan_period_seconds{sensor=\"301\"} 4.7"));
        assert!(text.contains("# TYPE firefly_radar_scan_period_seconds gauge"));
        assert!(text.contains("firefly_meteo_qnh_regions 2"));
        assert!(text.contains("firefly_meteo_qnh_hpa{region=\"EDDF\"} 1008"));
        assert!(text.contains("# TYPE firefly_meteo_qnh_hpa gauge"));
    }

    /// Without any registration estimate the labelled bias gauges are absent —
    /// an empty series is clearer than a misleading 0-bias line. REG.2a/2b.
    #[test]
    fn registration_bias_gauges_absent_without_estimates() {
        let text = render(&Metrics::default());
        assert!(text.contains("firefly_registration_estimates_total 0"));
        assert!(text.contains("firefly_registration_apply_active 0"));
        assert!(!text.contains("firefly_registration_bias_range_m{"));
        assert!(!text.contains("firefly_registration_bias_azimuth_deg{"));
        assert!(!text.contains("firefly_registration_applied_bias_range_m{"));
        assert!(!text.contains("firefly_radar_scan_period_seconds{"));
        assert!(!text.contains("firefly_meteo_qnh_hpa{"));
    }

    /// The connected-client guard increments on creation and decrements again
    /// on drop, while the lifetime total keeps counting.
    #[test]
    fn connected_client_guard_tracks_lifetime() {
        let metrics = Metrics::default();

        {
            let _guard = ConnectedClientGuard::new(&metrics);
            assert_eq!(metrics.ws_clients_connected.load(Ordering::Relaxed), 1);
            assert_eq!(metrics.ws_clients_total.load(Ordering::Relaxed), 1);
        }

        assert_eq!(metrics.ws_clients_connected.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.ws_clients_total.load(Ordering::Relaxed), 1);
    }
}
