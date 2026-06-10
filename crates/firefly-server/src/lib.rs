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
//!
//! Networking lives only here; the tracker core stays pure and clock-free.

pub mod app;
pub mod config;
pub mod pacing;
pub mod scene;

pub use app::{router, AppState};
pub use config::ServerConfig;
pub use firefly_io::Frame;

use tokio::net::TcpListener;

/// Serve the application on an already-bound listener until the stream ends or
/// the socket closes. A thin wrapper over [`axum::serve`] so callers (including
/// tests) need not depend on axum directly.
pub async fn serve(listener: TcpListener, state: AppState) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}
