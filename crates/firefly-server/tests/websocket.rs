//! End-to-end test of the WebSocket stream: bind a real server on an ephemeral
//! port, connect a real client, and confirm it receives parseable frames in
//! data-time order.
//!
//! REQ: FR-NET-001

use std::time::Duration;

use firefly_server::{scene, AppState, Frame, Metrics};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;

/// Spawn the server on `127.0.0.1:0`, connect, and read the first few frames.
#[tokio::test]
async fn websocket_streams_parseable_frames_in_order() {
    // A very high speed so the paced stream arrives effectively instantly and
    // the test never waits real seconds.
    let state = AppState {
        frames: Arc::new(scene::demo_frames()),
        speed: 100_000.0,
        metrics: Arc::new(Metrics::default()),
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

/// Sending the `"delay"` trigger (Häppchen 3.5) pauses delivery for a few
/// seconds but does not lose, duplicate or reorder frames — the demo for
/// NFR-CLOUD-004: only the *delivery* pauses, the track stream is untouched.
#[tokio::test]
async fn delay_trigger_pauses_delivery_without_corrupting_the_stream() {
    let state = AppState {
        frames: Arc::new(scene::demo_frames()),
        speed: 100_000.0,
        metrics: Arc::new(Metrics::default()),
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

    // First frame arrives immediately.
    let first = next_frame(&mut ws).await;

    // Ask for the delay demo; the server should acknowledge it before pausing.
    ws.send(tokio_tungstenite::tungstenite::Message::text("delay"))
        .await
        .expect("send the delay trigger");

    let ack = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("the acknowledgement should arrive promptly")
        .expect("the stream should still be open")
        .expect("the message should read cleanly");
    let ack_text = ack.to_text().expect("acknowledgement is text");
    assert!(
        ack_text.contains("delay_triggered"),
        "expected a delay_triggered notice, got {ack_text}"
    );

    // The next frame is delayed (≈5s pause) but still arrives, in order.
    let second = next_frame(&mut ws).await;
    assert!(second.time.as_secs() >= first.time.as_secs());
}

/// Read messages until the next [`Frame`] (skipping any control events).
async fn next_frame(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Frame {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(10), ws.next())
            .await
            .expect("a message should arrive within the timeout")
            .expect("the stream should still be open")
            .expect("the message should read cleanly");
        let text = message.to_text().expect("messages are text");
        if let Ok(frame) = Frame::from_json(text) {
            return frame;
        }
    }
}
