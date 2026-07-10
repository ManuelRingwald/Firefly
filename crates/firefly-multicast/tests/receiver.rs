//! End-to-end test of the multicast receiver: decode what the sender sends.
//!
//! As in `tests/sender.rs`, we avoid relying on multicast routing in CI by
//! sending to an ordinary loopback socket — the receive-and-decode path is
//! identical to what a real multicast listener would do with
//! [`receiver::recv_records`]/[`receiver::run`].

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use firefly_asterix::{Cat062Encoder, DataSourceId};
use firefly_core::{Callsign, SourceAges, SystemTrack, Timestamp, TrackId};
use firefly_geo::Wgs84;
use firefly_multicast::{receiver, run, sender_socket};
use tokio::net::UdpSocket;

const SYSTEM_REFERENCE_POINT: (f64, f64) = (48.0, 11.0);

/// A confirmed track with an SSR identity at a known position.
fn track(id: u32) -> SystemTrack {
    SystemTrack {
        id: TrackId(id),
        track_number: id as u16,
        time: Timestamp(0.0),
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

fn encoder() -> Cat062Encoder {
    Cat062Encoder::new(
        DataSourceId::new(25, 2),
        Wgs84::from_degrees(SYSTEM_REFERENCE_POINT.0, SYSTEM_REFERENCE_POINT.1, 0.0),
        0.0, // simulation starts at UTC midnight
    )
}

/// The receiver decodes exactly what the sender sent: one [`DecodedRecord`]
/// per track, per scan, with the original kinematics, identity and the
/// system-stereographic position recoverable to within I062/100's LSB.
/// REQ: FR-IO-004, FR-NET-002
#[tokio::test]
async fn receiver_decodes_what_sender_sends() {
    let receiver_endpoint = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let destination: SocketAddr = receiver_endpoint.local_addr().unwrap();
    let sender = sender_socket().await.unwrap();

    let scans = vec![
        (Timestamp(0.0), vec![track(1)]),
        (Timestamp(4.0), vec![track(1), track(2)]),
    ];

    let enc = encoder();
    // Huge speed → pacing delay is negligible, so the test does not wait 4 s.
    let send =
        tokio::spawn(async move { run(&sender, destination, &enc, &scans, 1.0e9, |_| {}).await });

    let mut received_scans = Vec::new();
    let receive = tokio::time::timeout(Duration::from_secs(5), async {
        receiver::run(&receiver_endpoint, |records| {
            received_scans.push(records);
            received_scans.len() < 2
        })
        .await
    })
    .await
    .expect("two datagrams should arrive promptly")
    .unwrap();

    assert_eq!(receive, 2, "two scans → two received datagrams");
    assert_eq!(send.await.unwrap().unwrap(), 2);

    // First scan: one track.
    assert_eq!(received_scans[0].len(), 1);
    // Second scan: two tracks, in order.
    assert_eq!(received_scans[1].len(), 2);
    assert_eq!(received_scans[1][0].track_number, 1);
    assert_eq!(received_scans[1][1].track_number, 2);

    let record = &received_scans[0][0];
    assert_eq!(record.source, DataSourceId::new(25, 2));
    assert_eq!(record.time, Timestamp(0.0));
    assert!(record.confirmed);
    assert!(!record.coasting);
    assert_eq!(record.v_east, 100.0);
    assert_eq!(record.v_north, -50.0);
    assert_eq!(record.mode_3a, Some(0o1000));
    assert_eq!(record.icao_address, Some(0x3C_00_01));

    // I062/105 and I062/100 are independent encodings of the same position;
    // both decode close to the original.
    assert!((record.position.lat_deg() - 48.1).abs() < 1e-5);
    assert!((record.position.lon_deg() - 11.2).abs() < 1e-5);

    let projection = firefly_geo::StereographicProjection::new(Wgs84::from_degrees(
        SYSTEM_REFERENCE_POINT.0,
        SYSTEM_REFERENCE_POINT.1,
        0.0,
    ));
    let from_cartesian =
        firefly_asterix::unproject_cartesian_position(record.cartesian, 0.0, &projection);
    assert!((from_cartesian.lat_deg() - 48.1).abs() < 1e-5);
    assert!((from_cartesian.lon_deg() - 11.2).abs() < 1e-5);
}

/// `receiver_socket` binds and joins the multicast group without error — the
/// same recipe a real consumer uses to listen for the CAT062 feed.
/// REQ: FR-NET-002
#[tokio::test]
async fn receiver_socket_joins_the_multicast_group() {
    let group = Ipv4Addr::new(239, 255, 0, 62);
    receiver::receiver_socket(group, 0)
        .await
        .expect("binding and joining the multicast group should succeed");
}
