//! The axum application: shared state, routes and the WebSocket frame pump.

use std::sync::Arc;

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

/// How the server obtains the frame stream served to WebSocket clients.
///
/// Since the replay/scene demo mode was removed (ADR 0030) there is exactly one
/// source: the live tracker. Each newly published
/// [`LiveSnapshot`](crate::live::LiveSnapshot) is forwarded to every connected
/// client as a [`Frame`]. The enum shape is kept so the wiring stays explicit
/// at the construction site.
#[derive(Clone)]
pub enum FrameSource {
    /// Live tracker: each new snapshot from the watch channel becomes a Frame.
    /// `sensor` is stamped into the Frame as the reporting sensor id.
    Live {
        snapshots: SnapshotRx,
        sensor: SensorId,
    },
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
/// Ready once the first source plot has arrived (`live_ready`, ADR 0020,
/// AP9.4c-4) — until then 503, so Kubernetes will not route traffic to a pod
/// that has no air picture yet. An instance deliberately running with **no**
/// sources is ready immediately (main pre-sets the flag): its empty sky IS the
/// complete picture (ADR 0030).
async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let is_ready = state
        .metrics
        .live_ready
        .load(std::sync::atomic::Ordering::Relaxed);
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
        crate::metrics::render(&state.metrics),
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

/// Pump the live frame stream to one connected WebSocket client.
async fn pump_frames(socket: WebSocket, state: AppState) {
    let _client_guard = ConnectedClientGuard::new(&state.metrics);
    let FrameSource::Live { snapshots, sensor } = state.source;
    pump_live(socket, snapshots, sensor).await
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

const INDEX_HTML: &str = include_str!("../static/index.html");

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt; // for `oneshot`

    /// A live-source state whose readiness flag is pre-set — the shape every
    /// route test needs since live is the only mode (ADR 0030). The watch
    /// sender is dropped deliberately: no test here pumps frames.
    fn state() -> AppState {
        use std::sync::atomic::Ordering;
        use tokio::sync::watch;
        let (_tx, rx) = watch::channel(crate::live::LiveSnapshot::empty());
        let metrics = Arc::new(crate::metrics::Metrics::default());
        metrics.live_ready.store(true, Ordering::Relaxed);
        AppState {
            source: FrameSource::Live {
                snapshots: rx,
                sensor: firefly_core::SensorId(1),
            },
            metrics,
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

    /// The readiness probe answers 200 once `live_ready` is set (here pre-set
    /// by the test state, as main does for a deliberately source-less
    /// instance — ADR 0030). REQ: FR-NET-001
    #[tokio::test]
    async fn ready_probe_is_ok() {
        assert_eq!(get_status("/ready").await, StatusCode::OK);
    }

    /// `/ready` returns 503 until the first source plot arrives (ADR 0020,
    /// AP9.4c-4). The server should not report ready to Kubernetes while it
    /// has no air picture yet.
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

    /// The metrics endpoint answers 200 with a Prometheus exposition.
    /// REQ: NFR-OBS-001
    #[tokio::test]
    async fn metrics_endpoint_renders_prometheus() {
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
        assert!(text.contains("firefly_ws_clients_connected"));
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
        let mut s = state();
        s.ws_token = Some(token.to_string());
        s
    }

    fn state_with_origin(origin: &str) -> AppState {
        let mut s = state();
        s.ws_allowed_origin = Some(origin.to_string());
        s
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
