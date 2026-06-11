//! The axum application: shared state, routes and the WebSocket frame pump.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use firefly_io::Frame;

use crate::pacing::delay_before;

/// Shared, read-only application state handed to every request.
///
/// The frame stream is built **once** at startup (deterministically, by the
/// Player) and replayed to each client; nothing here is mutated per request, so
/// the state is cheap to clone (an `Arc` bump) as axum requires.
#[derive(Clone)]
pub struct AppState {
    /// The precomputed frame stream to replay.
    pub frames: Arc<Vec<Frame>>,
    /// Playback speed: data-seconds per wall-second.
    pub speed: f64,
}

/// Build the router with every route wired to `state`.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/ready", get(ready))
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
async fn pump_frames(mut socket: WebSocket, state: AppState) {
    tracing::info!(
        frames = state.frames.len(),
        "client connected; replaying scene"
    );
    let mut prev: Option<f64> = None;
    let mut sent = 0usize;

    for frame in state.frames.iter() {
        let now = frame.time.as_secs();
        let delay = delay_before(prev, now, state.speed);
        if !wait_or_handle_delay_trigger(&mut socket, delay).await {
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
        prev = Some(now);
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
async fn wait_or_handle_delay_trigger(socket: &mut WebSocket, delay: Duration) -> bool {
    let sleep = tokio::time::sleep(delay);
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            _ = &mut sleep => return true,
            message = socket.recv() => match message {
                Some(Ok(Message::Text(text))) if text == "delay" => {
                    tracing::info!(pause_s = DELAY_TRIGGER_PAUSE.as_secs_f64(), "delay trigger received");
                    let notice = r#"{"event":"delay_triggered","duration_s":5.0}"#;
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
            frames: Arc::new(crate::scene::demo_frames()),
            speed: 1.0,
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

    /// The index page is served. REQ: FR-NET-001
    #[tokio::test]
    async fn index_page_is_served() {
        assert_eq!(get_status("/").await, StatusCode::OK);
    }

    /// The embedded page is the MapLibre air picture: it pulls in MapLibre, uses
    /// OpenStreetMap tiles (M6.2), includes airspace overlays (M6.2), and
    /// consumes the `/ws` stream. Guards against the embedded asset silently
    /// going missing or wrong. REQ: FR-UI-001
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
