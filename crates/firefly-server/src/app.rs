//! The axum application: shared state, routes and the WebSocket frame pump.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use firefly_io::Frame;

use crate::metrics::{ConnectedClientGuard, Metrics};
use crate::pacing::due_at;

/// How the server obtains the frame stream served to WebSocket clients (ADR 0020).
///
/// In `Replay` mode the stream is pre-computed once at startup (deterministic,
/// data-time–driven, bit-exact reproducibility via the `.ffrec` recorder).  The
/// `Live` variant is the placeholder for the live-tracker snapshot path wired in
/// AP9.4c-2/3 — at that point a `tokio::sync::watch` receiver carries the
/// current [`Frame`] produced by the real-time tracker.
#[derive(Clone)]
pub enum FrameSource {
    /// Pre-computed scene replayed to every client at `speed` data-s / wall-s.
    Replay { frames: Arc<Vec<Frame>>, speed: f64 },
    /// Live tracker snapshot (ADR 0020, AP9.4c-2/3 — not yet wired).
    Live,
}

impl FrameSource {
    /// Number of pre-computed frames, for the Prometheus gauge. Zero in Live mode.
    pub fn frame_count(&self) -> usize {
        match self {
            FrameSource::Replay { frames, .. } => frames.len(),
            FrameSource::Live => 0,
        }
    }
}

/// Shared, read-only application state handed to every request.
///
/// The state is cheap to clone (an `Arc` bump inside [`FrameSource`]) as axum
/// requires.  Nothing here is mutated per request.
#[derive(Clone)]
pub struct AppState {
    /// Where frames come from: a pre-computed replay or a live tracker.
    pub source: FrameSource,
    /// Operational counters exposed via `/metrics` (NFR-OBS-001).
    pub metrics: Arc<Metrics>,
}

/// Build the router with every route wired to `state`.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics_handler))
        .route("/ws", get(ws_handler))
        .with_state(state)
}

/// Liveness probe: the process is up and serving. ADR 0003 (Kubernetes).
async fn health() -> impl IntoResponse {
    "ok"
}

/// Readiness probe: ready to accept load. The frame stream is built before the
/// listener binds, so answering at all means we are ready. ADR 0003.
async fn ready() -> impl IntoResponse {
    "ready"
}

/// Prometheus text exposition of operational counters (NFR-OBS-001).
async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    (
        [("Content-Type", "text/plain; version=0.0.4; charset=utf-8")],
        crate::metrics::render(&state.metrics, state.source.frame_count()),
    )
}

/// The MapLibre air-picture page (Häppchen 3.4), embedded at compile time so the
/// binary is self-contained (one-command demo, NFR-OPS-001).
async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

/// Upgrade the connection and start pumping frames to this client.
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| pump_frames(socket, state))
}

/// Wall-clock pause inserted when a client asks to see the "delay" demo
/// (Häppchen 3.5, NFR-CLOUD-004). Purely a delivery-edge effect — the frame
/// stream itself, and therefore every track id and position, is unchanged.
pub const DELAY_TRIGGER_PAUSE: Duration = Duration::from_secs(5);

/// Replay the whole frame stream to one client, paced by data-time.
///
/// All wall-clock waiting lives here, at the delivery edge — never in the
/// tracker (see [`crate::pacing`]). If the client goes away mid-stream the send
/// fails and we simply stop.
///
/// While waiting, the socket is also watched for an incoming `"delay"` text
/// message. On receipt, an [`event: "delay_triggered"`](DELAY_TRIGGER_PAUSE)
/// notice is sent and the wait is extended by [`DELAY_TRIGGER_PAUSE`] — a
/// deliberately paused *delivery* that demonstrates the tracks resume exactly
/// where data-time says they should (NFR-CLOUD-004).
async fn pump_frames(socket: WebSocket, state: AppState) {
    let _client_guard = ConnectedClientGuard::new(&state.metrics);

    match state.source {
        FrameSource::Replay { frames, speed } => {
            pump_replay(socket, frames, speed).await;
        }
        FrameSource::Live => {
            // Live-tracker snapshot not yet wired (ADR 0020, AP9.4c-2/3).
            tracing::warn!("Live mode not yet implemented; closing connection");
        }
    }
}

/// Pump a pre-computed Replay frame sequence to one WebSocket client.
async fn pump_replay(mut socket: WebSocket, frames: Arc<Vec<Frame>>, speed: f64) {
    tracing::info!(frames = frames.len(), "client connected; replaying scene");
    // Absolute pacing: every frame's due time is measured from this fixed
    // origin and the stream's first data-time, so a delivery hiccup is caught
    // up afterwards instead of shifting the whole schedule (see `crate::pacing`).
    let origin = Instant::now();
    let first = frames.first().map(|f| f.time.as_secs());
    let mut sent = 0usize;

    for frame in frames.iter() {
        let delay = match first {
            Some(first) => due_at(origin, first, frame.time.as_secs(), speed)
                .saturating_duration_since(Instant::now()),
            None => Duration::ZERO,
        };
        if !wait_or_handle_delay_trigger(&mut socket, delay, speed).await {
            tracing::info!(sent, "client disconnected mid-stream");
            return;
        }

        let json = match frame.to_json() {
            Ok(json) => json,
            Err(error) => {
                tracing::error!(%error, "failed to serialise frame; skipping");
                continue;
            }
        };

        if socket.send(Message::Text(json.into())).await.is_err() {
            tracing::info!(sent, "client disconnected mid-stream");
            return;
        }
        sent += 1;
    }

    tracing::info!(sent, "scene complete; closing");
    let _ = socket.send(Message::Close(None)).await;
}

/// Wait out `delay`, while watching for a `"delay"` trigger from the client.
///
/// Each time the trigger arrives, an `delay_triggered` event is sent and the
/// wait restarts from [`DELAY_TRIGGER_PAUSE`]. Returns `false` if the socket
/// closed or errored, in which case the caller should stop.
async fn wait_or_handle_delay_trigger(socket: &mut WebSocket, delay: Duration, speed: f64) -> bool {
    let sleep = tokio::time::sleep(delay);
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            _ = &mut sleep => return true,
            message = socket.recv() => match message {
                Some(Ok(Message::Text(text))) if text == "delay" => {
                    tracing::info!(pause_s = DELAY_TRIGGER_PAUSE.as_secs_f64(), "delay trigger received");
                    // The notice carries the playback speed so the client can
                    // dead-reckon the tracks forward over the pause at the same
                    // data-time rate the live stream would have advanced.
                    let notice = format!(
                        r#"{{"event":"delay_triggered","duration_s":{},"speed":{}}}"#,
                        DELAY_TRIGGER_PAUSE.as_secs_f64(),
                        speed,
                    );
                    if socket.send(Message::Text(notice.into())).await.is_err() {
                        return false;
                    }
                    sleep.as_mut().reset(tokio::time::Instant::now() + DELAY_TRIGGER_PAUSE);
                }
                Some(Ok(Message::Close(_))) | None => return false,
                Some(Err(_)) => return false,
                _ => {}
            },
        }
    }
}

const INDEX_HTML: &str = include_str!("../static/index.html");

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt; // for `oneshot`

    fn state() -> AppState {
        AppState {
            source: FrameSource::Replay {
                frames: Arc::new(crate::scene::demo_frames()),
                speed: 1.0,
            },
            metrics: Arc::new(crate::metrics::Metrics::default()),
        }
    }

    async fn get_status(uri: &str) -> StatusCode {
        let response = router(state())
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        response.status()
    }

    /// The liveness probe answers 200. REQ: FR-NET-001
    #[tokio::test]
    async fn health_probe_is_ok() {
        assert_eq!(get_status("/health").await, StatusCode::OK);
    }

    /// The readiness probe answers 200. REQ: FR-NET-001
    #[tokio::test]
    async fn ready_probe_is_ok() {
        assert_eq!(get_status("/ready").await, StatusCode::OK);
    }

    /// The metrics endpoint answers 200 with a Prometheus exposition of the
    /// scene's frame count. REQ: NFR-OBS-001
    #[tokio::test]
    async fn metrics_endpoint_exposes_frame_count() {
        let response = router(state())
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("firefly_scene_frames_total"));
    }

    /// The index page is served. REQ: FR-NET-001
    #[tokio::test]
    async fn index_page_is_served() {
        assert_eq!(get_status("/").await, StatusCode::OK);
    }

    /// The embedded page is the MapLibre air picture: it pulls in MapLibre, uses
    /// OpenStreetMap tiles (M6.2), includes airspace overlays (M6.2), raw plot
    /// markers (M6.3), and consumes the `/ws` stream. Guards against the
    /// embedded asset silently going missing or wrong. REQ: FR-UI-001
    #[test]
    fn index_html_is_the_maplibre_frontend() {
        assert!(INDEX_HTML.contains("maplibre-gl"), "loads MapLibre");
        assert!(
            INDEX_HTML.contains("tile.openstreetmap.org"),
            "uses OpenStreetMap tiles (M6.2)"
        );
        assert!(
            INDEX_HTML.contains("airspaces"),
            "includes airspace overlays (M6.2)"
        );
        assert!(
            INDEX_HTML.contains("plot-marker"),
            "includes raw plot visualization (M6.3)"
        );
        assert!(INDEX_HTML.contains("/ws"), "connects to the frame stream");
        // The safety-relevant status is rendered (ADR 0008).
        assert!(INDEX_HTML.contains("coasting"), "shows coasting state");
        assert!(
            INDEX_HTML.contains("position_uncertainty_m"),
            "draws the uncertainty ring"
        );
        // The delay demo (Häppchen 3.5, NFR-CLOUD-004) is wired up.
        assert!(
            INDEX_HTML.contains("delay-btn") && INDEX_HTML.contains("'delay'"),
            "offers the delay-trigger button"
        );
        assert!(
            INDEX_HTML.contains("delay_triggered"),
            "shows the delay banner on the server's notice"
        );
    }
}
