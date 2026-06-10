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

/// A minimal placeholder page that dumps the live stream as text. The real
/// MapLibre map replaces it in Häppchen 3.4.
async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

/// Upgrade the connection and start pumping frames to this client.
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| pump_frames(socket, state))
}

/// Replay the whole frame stream to one client, paced by data-time.
///
/// All wall-clock waiting lives here, at the delivery edge — never in the
/// tracker (see [`crate::pacing`]). If the client goes away mid-stream the send
/// fails and we simply stop.
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
        if delay > Duration::ZERO {
            tokio::time::sleep(delay).await;
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

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Firefly — live tracks</title>
<style>
  body { font-family: system-ui, sans-serif; margin: 2rem; }
  #log { white-space: pre; font-family: monospace; margin-top: 1rem; }
  #status { font-weight: bold; }
</style>
</head>
<body>
<h1>Firefly — live frame stream</h1>
<p>Placeholder view of the raw frame stream. The live map arrives in Häppchen 3.4.</p>
<div id="status">connecting…</div>
<div id="log"></div>
<script>
  const statusEl = document.getElementById('status');
  const logEl = document.getElementById('log');
  const ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onopen = () => { statusEl.textContent = 'connected'; };
  ws.onclose = () => { statusEl.textContent = 'stream complete'; };
  ws.onmessage = (ev) => {
    const f = JSON.parse(ev.data);
    const lines = f.tracks.map(t =>
      `  #${t.id}  ${t.lat_deg.toFixed(4)}, ${t.lon_deg.toFixed(4)}  ` +
      `${Math.round(t.ground_speed_mps)} m/s  ` +
      `${t.confirmed ? 'CNF' : 'tent'}${t.coasting ? ' CST' : ''}`);
    logEl.textContent =
      `t = ${f.time.toFixed(1)} s   sensor ${f.sensor}   tracks: ${f.tracks.length}\n` +
      lines.join('\n');
  };
</script>
</body>
</html>
"#;

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

    /// The placeholder index page is served. REQ: FR-NET-001
    #[tokio::test]
    async fn index_page_is_served() {
        assert_eq!(get_status("/").await, StatusCode::OK);
    }
}
