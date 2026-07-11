//! The ADS-B ground-station UDP listener (FEP.3).
//!
//! Binds a UDP socket (joining the configured multicast group when one is
//! given, otherwise a plain unicast bind), receives datagrams, decodes each
//! as a CAT021 data block and emits the resulting [`Plot`]s. A malformed
//! datagram is dropped and logged — the listener never stops on bad input
//! (availability over completeness, like the radar/OpenSky/FLARM adapters).

use std::net::Ipv4Addr;

use firefly_asterix::decode_adsb_reports;
use firefly_core::Plot;
use tokio::net::UdpSocket;

use crate::config::Adsb021Config;
use crate::plot::adsb_report_to_plot;

/// The largest UDP payload we will read; oversized datagrams are read whole
/// (then rejected by the length check) rather than silently truncated.
const RECV_BUFFER_BYTES: usize = 65_536;

/// Decode one received datagram into plots (pure: no I/O). A datagram that
/// is not a valid CAT021 block yields an empty vector — never a panic (the
/// decoder is the untrusted-input boundary). Reports failing the drop rules
/// (surface/simulated/test, no position/time) are filtered by
/// [`adsb_report_to_plot`].
pub fn datagram_to_plots(datagram: &[u8], config: &Adsb021Config) -> Vec<Plot> {
    match decode_adsb_reports(datagram) {
        Ok(reports) => reports
            .iter()
            .filter_map(|r| adsb_report_to_plot(r, config.sensor_id))
            .collect(),
        Err(e) => {
            tracing::debug!(%e, "dropping malformed CAT021 datagram");
            Vec::new()
        }
    }
}

/// Bind a UDP socket for the configured listen endpoint, joining the
/// multicast group when one is configured.
pub async fn bind_socket(config: &Adsb021Config) -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, config.listen_port)).await?;
    if config.is_multicast() {
        socket.join_multicast_v4(config.listen_group, Ipv4Addr::UNSPECIFIED)?;
    }
    Ok(socket)
}

/// Run the listener indefinitely: receive datagrams, decode each as CAT021,
/// hand the resulting plots to `on_plots` (one call per datagram yielding at
/// least one plot). A bind failure logs and returns (the other sources keep
/// running).
pub async fn run<F>(config: &Adsb021Config, mut on_plots: F)
where
    F: FnMut(Vec<Plot>),
{
    let socket = match bind_socket(config).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(%e, port = config.listen_port, "ADS-B CAT021 listener failed to bind; not starting");
            return;
        }
    };
    tracing::info!(
        sac = config.sac,
        sic = config.sic,
        sensor_id = config.sensor_id.0,
        port = config.listen_port,
        multicast = config.is_multicast(),
        "ADS-B ground-station (CAT021) listener started (live mode)"
    );

    let mut buf = [0u8; RECV_BUFFER_BYTES];
    loop {
        let n = match socket.recv_from(&mut buf).await {
            Ok((n, _from)) => n,
            Err(e) => {
                tracing::warn!(%e, "ADS-B CAT021 UDP recv error");
                continue;
            }
        };
        let plots = datagram_to_plots(&buf[..n], config);
        if !plots.is_empty() {
            on_plots(plots);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::DetectionKind;

    /// A valid CAT021 datagram (identity + hi-res position + address + time)
    /// decodes to one geodetic plot; garbage yields none and never panics.
    /// REQ: FR-NET-015
    #[test]
    fn valid_datagram_yields_a_plot_and_garbage_none() {
        let datagram = [
            21, 0x00, 0x15, // CAT=21, LEN=21
            0x83, 0x18, // FSPEC {1, 7, 11, 12}
            0x19, 0x0A, // I021/010
            0x10, 0x00, 0x00, 0x00, // I021/131 lat 45°
            0x04, 0x00, 0x00, 0x00, // I021/131 lon 11.25°
            0x3C, 0x65, 0x89, // I021/080
            0x07, 0x08, 0x00, // I021/073 time 3600 s
        ];
        let config = Adsb021Config::default();
        let plots = datagram_to_plots(&datagram, &config);
        assert_eq!(plots.len(), 1);
        assert_eq!(plots[0].kind, DetectionKind::Secondary);
        assert_eq!(plots[0].sensor, config.sensor_id);
        assert_eq!(plots[0].mode_ac.icao_address, Some(0x3C_6589));

        assert!(datagram_to_plots(&[], &config).is_empty());
        assert!(datagram_to_plots(&[48, 0, 3], &config).is_empty()); // wrong CAT
        assert!(datagram_to_plots(&[21, 0xFF, 0xFF, 0x00], &config).is_empty());
    }
}
