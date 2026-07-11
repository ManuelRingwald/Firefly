//! End-to-end test of the multicast sender over a real UDP socket.
//!
//! Sending to a multicast group is, at the socket layer, just `send_to` with a
//! particular destination address. To keep the test deterministic and free of
//! multicast-routing assumptions in CI, we send to an ordinary loopback
//! receiver: the send path, the pacing and the byte content are exactly what a
//! multicast destination would carry.

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use firefly_asterix::{Cat062Encoder, DataSourceId};
use firefly_core::{Callsign, SourceAges, SystemTrack, Timestamp, TrackId};
use firefly_geo::Wgs84;
use firefly_multicast::{run, sender_socket};
use tokio::net::UdpSocket;

/// A confirmed track at a known position; the exact fields do not matter beyond
/// giving the encoder something concrete to serialise.
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
        monosensor: false,
        spi: false,
        daps: firefly_core::Daps::default(),
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
        Wgs84::from_degrees(48.0, 11.0, 0.0),
        0.0, // simulation starts at UTC midnight
    )
}

/// Each scan is sent as exactly one CAT062 data block, byte-for-byte equal to
/// what the encoder produces, in scan order. REQ: FR-IO-003
#[tokio::test]
async fn sends_one_cat062_block_per_scan() {
    let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let destination: SocketAddr = receiver.local_addr().unwrap();
    let sender = sender_socket().await.unwrap();

    let scans = vec![
        (Timestamp(0.0), vec![track(1)]),
        (Timestamp(4.0), vec![track(1), track(2)]),
    ];

    // The expected bytes, computed independently of the sender.
    let enc = encoder();
    let expected: Vec<Vec<u8>> = scans
        .iter()
        .map(|(t, tracks)| enc.encode(*t, tracks))
        .collect();

    // Huge speed → pacing delay is negligible, so the test does not wait 4 s.
    let send =
        tokio::spawn(async move { run(&sender, destination, &enc, &scans, 1.0e9, |_| {}).await });

    let mut buf = [0u8; 2048];
    for expected_block in &expected {
        let (n, _) = tokio::time::timeout(Duration::from_secs(5), receiver.recv_from(&mut buf))
            .await
            .expect("a datagram should arrive promptly")
            .unwrap();
        assert_eq!(
            &buf[..n],
            expected_block.as_slice(),
            "datagram matches the encoded block"
        );
        // Each datagram is a self-contained CAT062 block: CAT 62, LEN == length.
        assert_eq!(buf[0], 0x3E, "category 62");
        let len = u16::from_be_bytes([buf[1], buf[2]]) as usize;
        assert_eq!(len, n, "the block's LEN field equals the datagram length");
    }

    let sent = send.await.unwrap().unwrap();
    assert_eq!(sent, 2, "two scans → two datagrams");
}

/// An empty scan list sends nothing and is not an error. REQ: FR-IO-003
#[tokio::test]
async fn empty_scan_list_sends_nothing() {
    let sender = sender_socket().await.unwrap();
    let destination: SocketAddr = (Ipv4Addr::LOCALHOST, 9).into(); // discard port
    let sent = run(&sender, destination, &encoder(), &[], 1.0, |_| {})
        .await
        .unwrap();
    assert_eq!(sent, 0);
}
