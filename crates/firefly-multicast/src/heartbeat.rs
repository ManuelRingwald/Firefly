//! The CAT065 **heartbeat** sender — the SDPS-liveness counterpart to the
//! CAT062 track sender ([`crate::run`]).
//!
//! A real surveillance feed multiplexes a periodic *service status* report onto
//! the same output a consumer already listens to, so the consumer can tell a
//! genuinely empty sky from a dead feed (ADR 0018). This module paces one
//! CAT065 SDPS-Status data block onto the multicast group every
//! `period` — by the **wall clock**, since liveness is a real-time property
//! (unlike the data-time-paced track feed).
//!
//! The time of day stamped into each heartbeat (I065/030) is supplied by a
//! caller-provided clock closure, both to keep this loop testable and to make
//! explicit that the heartbeat reads the wall clock at the delivery edge — the
//! tracker core stays clock-free (ADR 0003).

use std::net::SocketAddr;
use std::time::Duration;

use firefly_asterix::Cat065Encoder;
use tokio::net::UdpSocket;

/// Send one CAT065 SDPS-Status heartbeat to `destination` every `period`.
///
/// `now_time_of_day` returns the current time of day in seconds since UTC
/// midnight; it is called once per heartbeat to stamp I065/030. The SDPS is
/// reported as operational (a degraded report would set the NOGO field).
///
/// The loop runs until a send fails — the caller (a spawned task) decides how
/// to react. It does not return on its own, so in practice it lives as long as
/// the server; `on_sent` is invoked after each successful send (e.g. to bump a
/// metric).
pub async fn run_heartbeat(
    socket: &UdpSocket,
    destination: SocketAddr,
    encoder: &Cat065Encoder,
    period: Duration,
    mut now_time_of_day: impl FnMut() -> f64,
    mut on_sent: impl FnMut(),
) -> std::io::Result<()> {
    let mut ticker = tokio::time::interval(period);
    loop {
        ticker.tick().await;
        let block = encoder.encode_status(now_time_of_day(), true);
        match socket.send_to(&block, destination).await {
            Ok(bytes) => {
                tracing::debug!(bytes, %destination, "sent CAT065 heartbeat");
                on_sent();
            }
            Err(error) => {
                tracing::error!(%destination, %error, "failed to send CAT065 heartbeat");
                return Err(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_asterix::{decode_status_block, DataSourceId, MESSAGE_TYPE_SDPS_STATUS};
    use std::net::Ipv4Addr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A loopback receiver gets paced, decodable heartbeats, and `on_sent` fires
    /// once per heartbeat.
    #[tokio::test]
    async fn heartbeats_are_sent_and_decodable() {
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let destination = receiver.local_addr().unwrap();
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();

        let encoder = Cat065Encoder::new(DataSourceId::new(0x19, 0x02), 1);
        let sent = Arc::new(AtomicUsize::new(0));
        let sent_in_task = Arc::clone(&sent);

        // A tiny period so two heartbeats arrive almost immediately.
        let handle = tokio::spawn(async move {
            let _ = run_heartbeat(
                &sender,
                destination,
                &encoder,
                Duration::from_millis(5),
                || 3600.0,
                move || {
                    sent_in_task.fetch_add(1, Ordering::Relaxed);
                },
            )
            .await;
        });

        let mut buf = [0u8; 64];
        for _ in 0..2 {
            let (n, _) = receiver.recv_from(&mut buf).await.unwrap();
            let reports = decode_status_block(&buf[..n]).expect("decodes");
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].message_type, MESSAGE_TYPE_SDPS_STATUS);
            assert!(reports[0].operational);
        }
        handle.abort();
        assert!(
            sent.load(Ordering::Relaxed) >= 2,
            "on_sent fired per heartbeat"
        );
    }
}
