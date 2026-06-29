//! The Firefly M3 web server.
//!
//! It takes the deterministic **frame stream** produced by the
//! [`Player`](firefly_player::Player) and serves it to a browser over a
//! WebSocket, paced into wall-clock time at the delivery edge (Häppchen 3.3,
//! ADR 0009). Around the stream it provides the cloud-native essentials:
//! health/readiness probes, 12-factor configuration and graceful shutdown
//! (ADR 0003), with structured logging and tracing (NFR-OBS-001).
//!
//! The crate is split into small, testable pieces:
//!
//! - [`config`] — 12-factor [`ServerConfig`](config::ServerConfig).
//! - [`pacing`] — the single data-time→wall-clock mapping.
//! - [`scene`] — a built-in demo frame stream.
//! - [`app`] — the axum router, state and WebSocket pump.
//! - [`live`] — the live-tracker runtime (ADR 0020): a long-lived tracker fed
//!   by sensor adapters, with input recording and a shared snapshot.
//! - [`metrics`] — the `/metrics` Prometheus endpoint.
//!
//! Networking lives only here; the tracker core stays pure and clock-free.

pub mod app;
pub mod config;
pub mod live;
pub mod metrics;
pub mod pacing;
pub mod replay;
pub mod scene;
pub mod sources;

pub use app::{router, AppState, FrameSource};
pub use config::{Scene, ServerConfig, ServerMode};
pub use firefly_io::Frame;
pub use live::{
    build_live_tracker, live_system_reference_point, run_live_cat062, run_live_tracker,
    LiveSnapshot, LiveTracker, PlotRecorder, SnapshotRx,
};
pub use metrics::Metrics;
pub use scene::scene_reference_point;

use tokio::net::TcpListener;

/// Serve the application on an already-bound listener until the stream ends or
/// the socket closes. A thin wrapper over [`axum::serve`] so callers (including
/// tests) need not depend on axum directly.
pub async fn serve(listener: TcpListener, state: AppState) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}
