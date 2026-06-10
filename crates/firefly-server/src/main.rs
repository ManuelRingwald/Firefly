//! Binary entry point: build the demo scene, then serve it with graceful
//! shutdown and structured logging.

use std::sync::Arc;

use firefly_server::{router, scene, AppState, ServerConfig};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    init_tracing();

    let config = ServerConfig::from_env();
    let frames = scene::demo_frames();
    tracing::info!(
        port = config.port,
        speed = config.speed,
        frames = frames.len(),
        "starting Firefly server"
    );

    let state = AppState {
        frames: Arc::new(frames),
        speed: config.speed,
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
