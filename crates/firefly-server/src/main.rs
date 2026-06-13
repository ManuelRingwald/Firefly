//! Binary entry point: build the demo scene, then serve it with graceful
//! shutdown and structured logging.

use std::sync::Arc;

use firefly_asterix::Cat062Encoder;
use firefly_multicast::MulticastConfig;
use firefly_server::{router, scene, AppState, Scene, ServerConfig};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    init_tracing();

    let config = ServerConfig::from_env();
    let frames = match config.scene {
        Scene::Demo => scene::demo_frames(),
        Scene::Frankfurt => scene::frankfurt_frames(),
    };
    tracing::info!(
        port = config.port,
        speed = config.speed,
        scene = ?config.scene,
        frames = frames.len(),
        "starting Firefly server"
    );

    spawn_cat062_multicast(config.speed, config.scene);

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

/// Spawn the CAT062 UDP-multicast feed alongside the web server, if enabled
/// (`FIREFLY_CAT062_ENABLED=true`). It replays the same scan stream the web
/// map shows — encoded as CAT062 and sent to the configured multicast group —
/// paced into wall-clock at the same `speed` (ADR 0006). Disabled by default,
/// so a plain `cargo run` never emits surprise network traffic.
fn spawn_cat062_multicast(speed: f64, scene: Scene) {
    let config = MulticastConfig::from_env();
    if !config.enabled {
        tracing::info!(
            "CAT062 multicast feed disabled (set FIREFLY_CAT062_ENABLED=true to enable)"
        );
        return;
    }

    let destination = config.destination();
    let encoder = Cat062Encoder::new(
        config.data_source(),
        config.reference_point,
        0.0, // TODO: make this configurable per scenario (ADR 0014: UTC Time-of-Day)
    );
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
        match firefly_multicast::run(&socket, destination, &encoder, &scans, speed).await {
            Ok(sent) => tracing::info!(sent, "CAT062 multicast feed complete"),
            Err(error) => tracing::error!(%error, "CAT062 multicast feed stopped"),
        }
    });
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
