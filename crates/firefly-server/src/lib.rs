//! The Firefly web server: the sources-driven live tracker behind an axum app.
//!
//! A long-lived [`Tracker`](firefly_track::Tracker) is fed by the configured
//! source adapters (`FIREFLY_SOURCES`, ADR 0023); each published snapshot is
//! served to browsers over a WebSocket and to operational consumers as CAT062
//! multicast. Around that it provides the cloud-native essentials:
//! health/readiness probes, 12-factor configuration and graceful shutdown
//! (ADR 0003), with structured logging and tracing (NFR-OBS-001). The earlier
//! replay/scene demo mode was removed (ADR 0030).
//!
//! The crate is split into small, testable pieces:
//!
//! - [`config`] — 12-factor [`ServerConfig`](config::ServerConfig).
//! - [`app`] — the axum router, state and WebSocket pump.
//! - [`live`] — the live-tracker runtime (ADR 0020): a long-lived tracker fed
//!   by sensor adapters, with input recording and a shared snapshot.
//! - [`replay`] — the deterministic `.ffplots` plot-replay engine
//!   (fault reproduction/regression, NFR-REPRO-001) — unrelated to the removed
//!   scene demo.
//! - [`metrics`] — the `/metrics` Prometheus endpoint.
//!
//! Networking lives only here; the tracker core stays pure and clock-free.

pub mod app;
pub mod config;
pub mod healthcheck;
pub mod live;
pub mod metrics;
pub mod replay;
pub mod snapshot;
pub mod sources;
pub mod standby;

pub use app::{router, AppState, CorrelationApi, FrameSource, SensorControl};
pub use config::ServerConfig;
pub use firefly_io::Frame;
pub use healthcheck::probe_local_health;
pub use live::{
    build_live_tracker, build_live_tracker_multi, live_system_reference_point,
    registration_enabled, resolve_plot_recorder, run_live_cat062, run_live_tracker,
    tracker_progress_stalled, LiveSnapshot, LiveTracker, ManualOverrides, PlotRecorder,
    RadarSensor, RegistrationTick, SensorGate, SnapshotRx, SnapshotTick,
};
pub use metrics::Metrics;
pub use snapshot::{config_fingerprint, RestoreDecision, SnapshotConfig};
pub use standby::{failover_timeout_from_env, role_from_env, wait_for_promotion, Role};

use tokio::net::TcpListener;

/// Serve the application on an already-bound listener until the stream ends or
/// the socket closes. A thin wrapper over [`axum::serve`] so callers (including
/// tests) need not depend on axum directly.
pub async fn serve(listener: TcpListener, state: AppState) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}
