//! The axum application: shared state, routes and the WebSocket frame pump.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use firefly_core::SensorId;
use firefly_io::Frame;

use crate::live::SnapshotRx;
use crate::metrics::{ConnectedClientGuard, Metrics};
use crate::pacing::due_at;

/// How the server obtains the frame stream served to WebSocket clients (ADR 0020).
///
/// In `Replay` mode the stream is pre-computed once at startup (deterministic,
/// data-time–driven, bit-exact reproducibility via the `.ffrec` recorder).  In
/// `Live` mode each newly published [`LiveSnapshot`](crate::live::LiveSnapshot)
/// from the live tracker is forwarded to every connected client as a [`Frame`].
#[derive(Clone)]
pub enum FrameSource {
    /// Pre-computed scene replayed to every client at `speed` data-s / wall-s.
    Replay { frames: Arc<Vec<Frame>>, speed: f64 },
    /// Live tracker: each new snapshot from the watch channel becomes a Frame.
    /// `sensor` is stamped into the Frame as the reporting sensor id.
    Live {
        snapshots: SnapshotRx,
        sensor: SensorId,
    },
}

impl FrameSource {
    /// Number of pre-computed frames, for the Prometheus gauge. Zero in Live mode.
    pub fn frame_count(&self) -> usize {
        match self {
            FrameSource::Replay { frames, .. } => frames.len(),
            FrameSource::Live { .. } => 0,
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
    /// If `Some`, the `/ws` endpoint requires `Authorization: Bearer <token>`
    /// or `?token=<value>`. Missing or wrong token → 401 (NFR-SEC-001).
    pub ws_token: Option<String>,
    /// If `Some`, the `Origin` header on the `/ws` upgrade must exactly match
    /// this value. Wrong origin → 403 (NFR-SEC-001, ADR 0017).
    pub ws_allowed_origin: Option<String>,
}

/// Query parameters accepted on the `/ws` upgrade request.
///
/// The browser's `WebSocket` API does not support custom request headers, so
/// token-based auth is also available via the `?token=` query parameter.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct WsQuery {
    token: Option<String>,
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

/// Readiness probe: ready to accept load.
///
/// In Replay mode the frame stream is fully built at startup, so the server is
/// always ready. In Live mode the server is only ready after the first
/// successful OpenSky poll has delivered at least one airborne aircraft — until
/// then `/ready` returns 503 so Kubernetes will not route traffic to a pod that
/// has no air picture yet (ADR 0020, AP9.4c-4).
async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let is_ready = match &state.source {
        FrameSource::Replay { .. } => true,
        FrameSource::Live { .. } => state
            .metrics
            .live_ready
            .load(std::sync::atomic::Ordering::Relaxed),
    };
    if is_ready {
        (StatusCode::OK, "ready")
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "not ready: waiting for first ADS-B poll",
        )
    }
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

/// Validate a WebSocket upgrade request against the configured auth rules.
///
/// Returns `Ok(())` when auth is satisfied (or not configured), `Err(StatusCode)`
/// when the request should be rejected. Extracted so the pure auth logic can be
/// unit-tested without a live hyper connection (NFR-SEC-001, ADR 0017).
fn authorize_ws(
    headers: &HeaderMap,
    token_query: Option<&str>,
    state: &AppState,
) -> Result<(), StatusCode> {
    if let Some(required) = &state.ws_token {
        let from_header = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        let provided = from_header.or(token_query);
        if provided != Some(required.as_str()) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    if let Some(allowed) = &state.ws_allowed_origin {
        let origin = headers.get("origin").and_then(|v| v.to_str().ok());
        if origin != Some(allowed.as_str()) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(())
}

/// Upgrade the connection and start pumping frames to this client.
///
/// Auth is checked before the WebSocket upgrade via [`authorize_ws`]:
/// - If [`AppState::ws_token`] is set, the request must carry it via
///   `Authorization: Bearer <token>` or `?token=<value>` → 401 if absent/wrong.
/// - If [`AppState::ws_allowed_origin`] is set, the `Origin` header must match
///   exactly → 403 if wrong (NFR-SEC-001, ADR 0017).
async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    Query(query): Query<WsQuery>,
    State(state): State<AppState>,
) -> Response {
    if let Err(status) = authorize_ws(&headers, query.token.as_deref(), &state) {
        return status.into_response();
    }
    ws.on_upgrade(move |socket| pump_frames(socket, state))
        .into_response()
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
        FrameSource::Replay { frames, speed } => pump_replay(socket, frames, speed).await,
        FrameSource::Live { snapshots, sensor } => pump_live(socket, snapshots, sensor).await,
    }
}

/// Stream live-tracker snapshots to one WebSocket client (ADR 0020, AP9.4c-3).
///
/// On each new [`LiveSnapshot`](crate::live::LiveSnapshot) published by the
/// tracker task, build a [`Frame`] (empty plot list — ADS-B plots are not
/// available in the browser overlay in Live mode) and send it as JSON. The loop
/// ends when the tracker shuts down (watch sender dropped) or the client
/// disconnects.
async fn pump_live(mut socket: WebSocket, mut rx: SnapshotRx, sensor: SensorId) {
    tracing::info!("client connected; live tracker mode");
    loop {
        if rx.changed().await.is_err() {
            tracing::info!("live tracker stopped; closing client");
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
        let snapshot = rx.borrow_and_update().clone();
        if snapshot.tracks.is_empty() {
            continue; // no tracks yet; wait for the first ADS-B poll
        }
        let frame = Frame::new(snapshot.time, sensor, &[], &snapshot.tracks);
        let json = match frame.to_json() {
            Ok(j) => j,
            Err(error) => {
                tracing::error!(%error, "failed to serialise live frame; skipping");
                continue;
            }
        };
        if socket.send(Message::Text(json.into())).await.is_err() {
            tracing::info!("live client disconnected");
            return;
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
            ws_token: None,
            ws_allowed_origin: None,
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

    /// The readiness probe answers 200 in Replay mode. REQ: FR-NET-001
    #[tokio::test]
    async fn ready_probe_is_ok() {
        assert_eq!(get_status("/ready").await, StatusCode::OK);
    }

    /// In Live mode, `/ready` returns 503 until the first ADS-B poll succeeds
    /// (ADR 0020, AP9.4c-4). The server should not report ready to Kubernetes
    /// while it has no air picture yet.
    #[tokio::test]
    async fn ready_probe_returns_503_in_live_mode_before_first_poll() {
        use tokio::sync::watch;
        let (_tx, rx) = watch::channel(crate::live::LiveSnapshot::empty());
        let metrics = Arc::new(crate::metrics::Metrics::default());
        // live_ready is false by default
        let live_state = AppState {
            source: FrameSource::Live {
                snapshots: rx,
                sensor: firefly_core::SensorId(200),
            },
            metrics,
            ws_token: None,
            ws_allowed_origin: None,
        };
        let response = router(live_state)
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// Once `live_ready` is set the probe returns 200 (ADR 0020, AP9.4c-4).
    #[tokio::test]
    async fn ready_probe_returns_ok_in_live_mode_after_first_poll() {
        use std::sync::atomic::Ordering;
        use tokio::sync::watch;
        let (_tx, rx) = watch::channel(crate::live::LiveSnapshot::empty());
        let metrics = Arc::new(crate::metrics::Metrics::default());
        metrics.live_ready.store(true, Ordering::Relaxed);
        let live_state = AppState {
            source: FrameSource::Live {
                snapshots: rx,
                sensor: firefly_core::SensorId(200),
            },
            metrics,
            ws_token: None,
            ws_allowed_origin: None,
        };
        let response = router(live_state)
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
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

    // ---------------------------------------------------------------------------
    // WebSocket auth tests (NFR-SEC-001, ADR 0017)
    //
    // These test `authorize_ws` directly rather than going through the full
    // HTTP stack. `WebSocketUpgrade` requires `hyper::upgrade::OnUpgrade` in
    // request extensions, which only exists in a live hyper connection — not in
    // `tower::ServiceExt::oneshot`. Auth is pure logic; a direct function call
    // is the right level to test it.
    // ---------------------------------------------------------------------------

    fn state_with_token(token: &str) -> AppState {
        AppState {
            source: FrameSource::Replay {
                frames: Arc::new(crate::scene::demo_frames()),
                speed: 1.0,
            },
            metrics: Arc::new(crate::metrics::Metrics::default()),
            ws_token: Some(token.to_string()),
            ws_allowed_origin: None,
        }
    }

    fn state_with_origin(origin: &str) -> AppState {
        AppState {
            source: FrameSource::Replay {
                frames: Arc::new(crate::scene::demo_frames()),
                speed: 1.0,
            },
            metrics: Arc::new(crate::metrics::Metrics::default()),
            ws_token: None,
            ws_allowed_origin: Some(origin.to_string()),
        }
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut m = HeaderMap::new();
        for &(k, v) in pairs {
            m.insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                axum::http::HeaderValue::from_str(v).unwrap(),
            );
        }
        m
    }

    /// No auth configured → any request is allowed. REQ: NFR-SEC-001
    #[test]
    fn ws_no_auth_configured_accepts_upgrade() {
        assert!(authorize_ws(&HeaderMap::new(), None, &state()).is_ok());
    }

    /// Token configured, no token provided → 401. REQ: NFR-SEC-001
    #[test]
    fn ws_missing_token_is_rejected_with_401() {
        assert_eq!(
            authorize_ws(&HeaderMap::new(), None, &state_with_token("secret")),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    /// Token configured, wrong Authorization header → 401. REQ: NFR-SEC-001
    #[test]
    fn ws_wrong_token_is_rejected_with_401() {
        let h = headers(&[("authorization", "Bearer wrong")]);
        assert_eq!(
            authorize_ws(&h, None, &state_with_token("secret")),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    /// Token configured, correct Authorization header → allowed. REQ: NFR-SEC-001
    #[test]
    fn ws_correct_bearer_token_is_accepted() {
        let h = headers(&[("authorization", "Bearer secret")]);
        assert!(authorize_ws(&h, None, &state_with_token("secret")).is_ok());
    }

    /// Token via query param `?token=` is also accepted. REQ: NFR-SEC-001
    #[test]
    fn ws_correct_query_token_is_accepted() {
        assert!(authorize_ws(
            &HeaderMap::new(),
            Some("secret"),
            &state_with_token("secret")
        )
        .is_ok());
    }

    /// Origin configured, wrong Origin header → 403. REQ: NFR-SEC-001
    #[test]
    fn ws_wrong_origin_is_rejected_with_403() {
        let h = headers(&[("origin", "https://evil.example.com")]);
        assert_eq!(
            authorize_ws(&h, None, &state_with_origin("https://app.example.com")),
            Err(StatusCode::FORBIDDEN)
        );
    }

    /// Origin configured, correct Origin header → allowed. REQ: NFR-SEC-001
    #[test]
    fn ws_correct_origin_is_accepted() {
        let h = headers(&[("origin", "https://app.example.com")]);
        assert!(authorize_ws(&h, None, &state_with_origin("https://app.example.com")).is_ok());
    }
}
