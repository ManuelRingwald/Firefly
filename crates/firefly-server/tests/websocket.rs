//! End-to-end test of the WebSocket stream: bind a real server on an ephemeral
//! port, connect a real client, and confirm it receives parseable frames in
//! data-time order as the live tracker publishes snapshots (the only frame
//! source since the scene replay mode was removed, ADR 0030).
//!
//! REQ: FR-NET-001

use std::sync::Arc;
use std::time::Duration;

use firefly_core::{Callsign, SourceAges, SystemTrack, Timestamp, TrackId};
use firefly_geo::Wgs84;
use firefly_server::{AppState, Frame, FrameSource, LiveSnapshot, Metrics};
use futures_util::StreamExt;
use tokio::sync::watch;

/// A confirmed track at a known position; the exact fields do not matter beyond
/// giving the pump something concrete to serialise.
fn track(id: u32, time: f64) -> SystemTrack {
    SystemTrack {
        id: TrackId(id),
        track_number: id as u16,
        time: Timestamp(time),
        position: Wgs84::from_degrees(48.1, 11.2, 10_000.0),
        v_east: 100.0,
        v_north: -50.0,
        confirmed: true,
        coasting: false,
        ended: false,
        update_age: 0.0,
        position_uncertainty: 100.0,
        mode_3a: Some(0o1000),
        icao_address: Some(0x3C_00_01),
        flight_level_ft: Some(35_000.0),
        callsign: Some(Callsign::new("DLH123")),
        contributing_sensors: Vec::new(),
        adsb_age_s: None,
        source_ages: SourceAges::default(),
    }
}

fn snapshot(time: f64) -> LiveSnapshot {
    LiveSnapshot {
        time: Timestamp(time),
        tracks: Arc::new(vec![track(1, time)]),
    }
}

/// Spawn the server on `127.0.0.1:0`, connect, publish snapshots one by one
/// (each awaited by the client before the next is sent, so the watch channel's
/// last-value-wins coalescing cannot drop any) and confirm the client receives
/// them as parseable frames in data-time order.
#[tokio::test]
async fn websocket_streams_parseable_frames_in_order() {
    let (tx, rx) = watch::channel(LiveSnapshot::empty());
    let state = AppState {
        source: FrameSource::Live {
            snapshots: rx,
            sensor: firefly_core::SensorId(1),
        },
        metrics: Arc::new(Metrics::default()),
        ws_token: None,
        ws_allowed_origin: None,
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
    for round in 0..5 {
        let time = 10.0 + round as f64 * 4.0;
        tx.send(snapshot(time)).expect("publish a live snapshot");

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
        assert_eq!(frame.tracks.len(), 1, "the published track is carried");
        last_time = frame.time.as_secs();
    }
}
