//! The Firefly CAT062 **UDP-multicast** transport adapter.
//!
//! Where [`firefly-server`](../firefly_server/index.html) serialises the
//! tracker's output to JSON and streams it to a browser, this crate is the
//! *operational* transport: it takes the same neutral
//! [`SystemTrack`](firefly_core::SystemTrack)s, encodes each scan into an
//! **ASTERIX CAT062** data block (via
//! [`Cat062Encoder`](firefly_asterix::Cat062Encoder)) and sends it as a UDP
//! datagram to a **multicast group**. Consumers — the Phoenix ASD, an EFS, a
//! recorder — join that group and listen independently; the sender never learns
//! who is out there (ADR 0006, ED-109A-style distribution; ADR 0003 decoupling).
//!
//! ## Shape of the adapter
//!
//! - [`MulticastConfig`] — 12-factor configuration (group, port, SAC/SIC, the
//!   system reference point for I062/100).
//! - [`sender_socket`] — bind a UDP socket suitable for sending multicast.
//! - [`run`] — pace a list of scans into wall-clock time and send one CAT062
//!   block per scan to a destination.
//! - [`receiver`] — the consumer side: join the multicast group and decode the
//!   datagrams back into [`firefly_asterix::DecodedRecord`]s (Häppchen D).
//!
//! Sending to a multicast group is, at the socket level, just sending to a
//! particular destination address; [`run`] is therefore destination-agnostic
//! and the multicast group is supplied by the caller (from
//! [`MulticastConfig::destination`]). That also makes it directly testable
//! against an ordinary loopback receiver.
//!
//! All wall-clock waiting lives in [`pacing`] — the delivery edge — never in the
//! tracker, which stays pure and data-time driven (ADR 0003).
//!
//! REQ: FR-IO-003

pub mod config;
pub mod heartbeat;
pub mod pacing;
pub mod receiver;

use std::net::{Ipv4Addr, SocketAddr};

use firefly_asterix::Cat062Encoder;
use firefly_core::{SystemTrack, Timestamp};
use tokio::net::UdpSocket;

pub use config::MulticastConfig;
pub use heartbeat::run_heartbeat;

/// Bind a UDP socket suitable for *sending* multicast datagrams.
///
/// We bind to an ephemeral local port on all interfaces; the destination
/// (multicast group + port) is chosen per send by [`run`]. The kernel's default
/// multicast TTL is `1`, which keeps the traffic on the local subnet — the
/// right scope for an ED-109A-style intra-site distribution and a safe default
/// for a demo.
pub async fn sender_socket() -> std::io::Result<UdpSocket> {
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await
}

/// Send one CAT062 data block per scan to `destination`, paced by data-time.
///
/// `scans` is the deterministic per-scan output of the tracker (e.g. from
/// [`Player::scans`](../firefly_player/struct.Player.html#method.scans)): each
/// entry is a scan time and the [`SystemTrack`]s at that time. Before each scan
/// we wait the wall-clock delay that [`pacing::delay_before`] derives from the
/// data-time gap and `speed` (data-seconds per wall-second), then encode the
/// scan with `encoder` and send the bytes.
///
/// Returns the number of datagrams sent. A send error stops the run and is
/// returned — the caller (a spawned task) decides how to react.
pub async fn run(
    socket: &UdpSocket,
    destination: SocketAddr,
    encoder: &Cat062Encoder,
    scans: &[(Timestamp, Vec<SystemTrack>)],
    speed: f64,
) -> std::io::Result<usize> {
    let mut prev: Option<f64> = None;
    let mut sent = 0usize;

    for (time, tracks) in scans {
        let now = time.as_secs();
        let delay = pacing::delay_before(prev, now, speed);
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }

        let block = encoder.encode(*time, tracks);
        match socket.send_to(&block, destination).await {
            Ok(bytes) => {
                tracing::debug!(
                    time = now,
                    bytes,
                    tracks = tracks.len(),
                    %destination,
                    "sent CAT062 data block"
                );
            }
            Err(error) => {
                tracing::error!(time = now, %destination, %error, "failed to send CAT062 data block");
                return Err(error);
            }
        }

        prev = Some(now);
        sent += 1;
    }

    Ok(sent)
}
