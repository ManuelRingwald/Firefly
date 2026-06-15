//! Prometheus `/metrics` endpoint (REQ NFR-OBS-001): exposes operational
//! counters/gauges for track throughput, the CAT062 multicast feed and
//! connected WebSocket clients.
//!
//! Counters are plain atomics rather than a metrics crate — the set of
//! metrics is small and fixed, and a hand-rolled exposition keeps the
//! dependency surface minimal.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

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
pub fn render(metrics: &Metrics, frames_total: usize) -> String {
    let mut out = String::new();
    write_metric(
        &mut out,
        "firefly_scene_frames_total",
        "gauge",
        "Number of frames in the currently loaded scene.",
        frames_total as f64,
    );
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
    out
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

        let text = render(&metrics, 9);

        assert!(text.contains("firefly_scene_frames_total 9"));
        assert!(text.contains("firefly_ws_clients_connected 2"));
        assert!(text.contains("firefly_ws_clients_total 5"));
        assert!(text.contains("firefly_cat062_scans_sent_total 42"));
        assert!(text.contains("firefly_cat062_send_errors_total 1"));
        assert!(text.contains("firefly_cat065_heartbeats_sent_total 13"));
        assert!(text.contains("# TYPE firefly_ws_clients_connected gauge"));
        assert!(text.contains("# TYPE firefly_cat062_scans_sent_total counter"));
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
