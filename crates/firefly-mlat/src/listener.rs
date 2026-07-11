//! The WAM/MLAT UDP listener (FEP.5).
//!
//! Binds a UDP socket (joining the configured multicast group when one is
//! given, otherwise a plain unicast bind), receives datagrams and dispatches
//! on the leading category octet: **CAT020** target reports become
//! [`Plot`]s, **CAT019** status messages go to the status callback (system
//! liveness without traffic, like a radar's service messages). A malformed
//! datagram is dropped and logged — the listener never stops on bad input
//! (availability over completeness, like the other adapters).

use std::net::Ipv4Addr;

use firefly_asterix::{decode_mlat_reports, decode_mlat_status, DecodedMlatStatus};
use firefly_core::Plot;
use tokio::net::UdpSocket;

use crate::config::MlatConfig;
use crate::plot::mlat_report_to_plot;

/// The largest UDP payload we will read; oversized datagrams are read whole
/// (then rejected by the length check) rather than silently truncated.
const RECV_BUFFER_BYTES: usize = 65_536;

/// The leading category octet of a CAT019 status block: the listener
/// dispatches on it, everything else goes down the CAT020 plot path.
const CAT019: u8 = 19;

/// Decode one received datagram into plots (pure: no I/O). A datagram that
/// is not a valid CAT020 block yields an empty vector — never a panic (the
/// decoder is the untrusted-input boundary). Reports failing the drop rules
/// (field monitor / simulated / test / surface, no position/time) are
/// filtered by [`mlat_report_to_plot`].
pub fn datagram_to_plots(datagram: &[u8], config: &MlatConfig) -> Vec<Plot> {
    match decode_mlat_reports(datagram) {
        Ok(reports) => reports
            .iter()
            .filter_map(|r| mlat_report_to_plot(r, config.sensor_id))
            .collect(),
        Err(e) => {
            tracing::debug!(%e, "dropping malformed CAT020 datagram");
            Vec::new()
        }
    }
}

/// Decode one received datagram into CAT019 status messages (pure: no I/O).
/// A malformed block yields an empty vector — never a panic.
pub fn datagram_to_status(datagram: &[u8]) -> Vec<DecodedMlatStatus> {
    match decode_mlat_status(datagram) {
        Ok(messages) => messages,
        Err(e) => {
            tracing::debug!(%e, "dropping malformed CAT019 datagram");
            Vec::new()
        }
    }
}

/// Bind a UDP socket for the configured listen endpoint, joining the
/// multicast group when one is configured.
pub async fn bind_socket(config: &MlatConfig) -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, config.listen_port)).await?;
    if config.is_multicast() {
        socket.join_multicast_v4(config.listen_group, Ipv4Addr::UNSPECIFIED)?;
    }
    Ok(socket)
}

/// Run the listener indefinitely: receive datagrams, dispatch on the leading
/// category octet — CAT019 status to `on_status`, everything else down the
/// CAT020 plot path to `on_plots` (one call per datagram yielding at least
/// one message/plot). A bind failure logs and returns (the other sources
/// keep running).
pub async fn run<F, G>(config: &MlatConfig, mut on_plots: F, mut on_status: G)
where
    F: FnMut(Vec<Plot>),
    G: FnMut(Vec<DecodedMlatStatus>),
{
    let socket = match bind_socket(config).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(%e, port = config.listen_port, "WAM/MLAT listener failed to bind; not starting");
            return;
        }
    };
    tracing::info!(
        sac = config.sac,
        sic = config.sic,
        sensor_id = config.sensor_id.0,
        port = config.listen_port,
        multicast = config.is_multicast(),
        "WAM/MLAT (CAT020 + CAT019) listener started (live mode)"
    );

    let mut buf = [0u8; RECV_BUFFER_BYTES];
    loop {
        let n = match socket.recv_from(&mut buf).await {
            Ok((n, _from)) => n,
            Err(e) => {
                tracing::warn!(%e, "WAM/MLAT UDP recv error");
                continue;
            }
        };
        let datagram = &buf[..n];
        if datagram.first() == Some(&CAT019) {
            let messages = datagram_to_status(datagram);
            if !messages.is_empty() {
                on_status(messages);
            }
        } else {
            let plots = datagram_to_plots(datagram, config);
            if !plots.is_empty() {
                on_plots(plots);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::DetectionKind;

    /// A valid CAT020 datagram (identity + position + address + time)
    /// decodes to one geodetic plot; garbage yields none and never panics.
    /// REQ: FR-NET-017
    #[test]
    fn valid_datagram_yields_a_plot_and_garbage_none() {
        let datagram = [
            20, 0x00, 0x19, // CAT=20, LEN=25
            0xF1, 0x88, // FSPEC {1,2,3,4,8,12}
            0x19, 0x0A, // I020/010
            0x41, 0x00, // I020/020 MS, no flags
            0x07, 0x08, 0x00, // I020/140 time 3600 s
            0x00, 0x80, 0x00, 0x00, // I020/041 lat 45°
            0x00, 0x20, 0x00, 0x00, // I020/041 lon 11.25°
            0x02, 0x8B, // I020/070
            0x3C, 0x65, 0x89, // I020/220
        ];
        let config = MlatConfig::default();
        let plots = datagram_to_plots(&datagram, &config);
        assert_eq!(plots.len(), 1);
        assert_eq!(plots[0].kind, DetectionKind::Secondary);
        assert_eq!(plots[0].sensor, config.sensor_id);
        assert_eq!(plots[0].mode_ac.icao_address, Some(0x3C_6589));

        assert!(datagram_to_plots(&[], &config).is_empty());
        assert!(datagram_to_plots(&[48, 0, 3], &config).is_empty()); // wrong CAT
        assert!(datagram_to_plots(&[20, 0xFF, 0xFF, 0x00], &config).is_empty());
    }

    /// A valid CAT019 datagram decodes to a status message (the liveness
    /// signal); garbage yields none. REQ: FR-NET-017
    #[test]
    fn cat019_datagram_yields_status() {
        let datagram = [
            19, 0x00, 0x0B, // CAT=19, LEN=11
            0xF0, // FSPEC {1,2,3,4}
            0x19, 0x0A, // I019/010
            0x02, // I019/000 periodic
            0x07, 0x08, 0x00, // I019/140
            0x00, // I019/550 operational
        ];
        let messages = datagram_to_status(&datagram);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].operational, Some(true));

        assert!(datagram_to_status(&[]).is_empty());
        assert!(datagram_to_status(&[19, 0xFF, 0xFF]).is_empty());
    }
}
