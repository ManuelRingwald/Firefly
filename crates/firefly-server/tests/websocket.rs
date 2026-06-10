//! End-to-end test of the WebSocket stream: bind a real server on an ephemeral
//! port, connect a real client, and confirm it receives parseable frames in
//! data-time order.
//!
//! REQ: FR-NET-001

use std::time::Duration;

use firefly_server::{scene, AppState, Frame};
use futures_util::StreamExt;
use std::sync::Arc;

/// Spawn the server on `127.0.0.1:0`, connect, and read the first few frames.
#[tokio::test]
async fn websocket_streams_parseable_frames_in_order() {
    // A very high speed so the paced stream arrives effectively instantly and
    // the test never waits real seconds.
    let state = AppState {
        frames: Arc::new(scene::demo_frames()),
        speed: 100_000.0,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        firefly_server::serve(listener, state).await.unwrap();
    });

    let url = format!("ws://{addr}/ws");
    let (mut ws, _response) = tokio_tungstenite::connect_async(url)
        .await
        .expect("connect to the websocket");

    let mut last_time = f64::NEG_INFINITY;
    let mut seen = 0;
    for _ in 0..5 {
        let message = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("a frame should arrive within the timeout")
            .expect("the stream should still be open")
            .expect("the message should read cleanly");

        let text = message.to_text().expect("frames are text messages");
        let frame = Frame::from_json(text).expect("each message is a valid Frame JSON");

        assert!(
            frame.time.as_secs() >= last_time,
            "frames arrive in data-time order"
        );
        last_time = frame.time.as_secs();
        seen += 1;
    }

    assert_eq!(seen, 5, "received five frames");
}
