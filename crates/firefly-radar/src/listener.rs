//! The radar ASTERIX UDP listener (ADR 0028 §1).
//!
//! Binds a UDP socket (joining the configured multicast group when one is given,
//! otherwise a plain unicast bind), receives datagrams, decodes each as a CAT048
//! data block and emits the resulting [`Plot`]s. A malformed datagram is dropped
//! and logged — the listener never stops on bad input, and a socket error
//! triggers a bounded retry rather than tearing down the server (availability
//! over completeness, like the OpenSky/FLARM adapters).

use std::net::{Ipv4Addr, SocketAddr};

use firefly_asterix::decode_target_reports;
use firefly_core::Plot;
use tokio::net::UdpSocket;

use crate::config::RadarConfig;
use crate::plot::target_report_to_plot;

/// The largest UDP payload we will read. A CAT048 block is far smaller, but a
/// generous buffer means an oversized datagram is read whole (and then rejected
/// by the length check) rather than silently truncated.
const RECV_BUFFER_BYTES: usize = 65_536;

/// Decode one received datagram into plots (pure: no I/O). A datagram that is not
/// a valid CAT048 block yields an empty vector — never a panic (the decoder is the
/// untrusted-input boundary, ADR 0028). Reports without a measurable position are
/// dropped by [`target_report_to_plot`].
pub fn datagram_to_plots(datagram: &[u8], config: &RadarConfig) -> Vec<Plot> {
    match decode_target_reports(datagram) {
        Ok(reports) => reports
            .iter()
            .filter_map(|r| target_report_to_plot(r, config.sensor_id))
            .collect(),
        Err(e) => {
            tracing::debug!(%e, "dropping malformed CAT048 datagram");
            Vec::new()
        }
    }
}

/// Bind a UDP socket for the configured listen endpoint, joining the multicast
/// group when one is configured.
pub async fn bind_socket(config: &RadarConfig) -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, config.listen_port)).await?;
    if config.is_multicast() {
        socket.join_multicast_v4(config.listen_group, Ipv4Addr::UNSPECIFIED)?;
    }
    Ok(socket)
}

/// Run the radar listener indefinitely: receive datagrams, decode each as
/// CAT048, and hand the resulting plots to `on_plots` (one call per datagram
/// that yields at least one plot). Never returns under normal operation; a bind
/// failure logs and returns (the other sources keep running).
pub async fn run<F>(config: &RadarConfig, mut on_plots: F)
where
    F: FnMut(Vec<Plot>),
{
    let socket = match bind_socket(config).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(%e, port = config.listen_port, "radar listener failed to bind; not starting");
            return;
        }
    };
    tracing::info!(
        sac = config.sac,
        sic = config.sic,
        sensor_id = config.sensor_id.0,
        port = config.listen_port,
        multicast = config.is_multicast(),
        "radar ASTERIX (CAT048) listener started (live mode)"
    );

    let mut buf = [0u8; RECV_BUFFER_BYTES];
    loop {
        let n = match socket.recv_from(&mut buf).await {
            Ok((n, _from)) => n,
            Err(e) => {
                tracing::warn!(%e, "radar UDP recv error");
                continue;
            }
        };
        let plots = datagram_to_plots(&buf[..n], config);
        if !plots.is_empty() {
            on_plots(plots);
        }
    }
}

/// The unspecified-IPv4 socket address for `port` — a small helper used by the
/// orchestration layer when it wants to log the listen endpoint.
pub fn listen_addr(config: &RadarConfig) -> SocketAddr {
    SocketAddr::from((config.listen_group, config.listen_port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::DetectionKind;

    /// A valid CAT048 datagram (one PSR-only report) decodes to one polar plot.
    /// REQ: FR-NET-013
    #[test]
    fn valid_datagram_yields_a_plot() {
        // FSPEC {1,2,3,4}: octet1 = 0x80|0x40|0x20|0x10 = 0xF0 (no FX, FRN ≤ 7).
        let record = [
            0xF0, // FSPEC {1,2,3,4}
            0x01, 0x04, // I048/010 SAC/SIC
            0x00, 0x06, 0x00, // I048/140 ToD = 12 s
            0x20, // I048/020 TYP=001 (single PSR)
            0x64, 0x00, 0x00, 0x00, // I048/040 RHO 100 NM, THETA 0
        ];
        let mut datagram = vec![48, 0x00, (3 + record.len()) as u8];
        datagram.extend_from_slice(&record);

        let config = RadarConfig::default();
        let plots = datagram_to_plots(&datagram, &config);
        assert_eq!(plots.len(), 1);
        assert_eq!(plots[0].kind, DetectionKind::Primary);
        assert_eq!(plots[0].sensor, config.sensor_id);
    }

    /// A non-CAT048 / garbage datagram yields no plots and does not panic.
    /// REQ: FR-NET-013
    #[test]
    fn garbage_datagram_yields_no_plots() {
        let config = RadarConfig::default();
        assert!(datagram_to_plots(&[], &config).is_empty());
        assert!(datagram_to_plots(&[62, 0, 3], &config).is_empty()); // wrong category
        assert!(datagram_to_plots(&[48, 0xFF, 0xFF, 0x00], &config).is_empty());
        // length lie
    }
}
