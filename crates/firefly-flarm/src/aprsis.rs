//! APRS-IS client for the OGN feed (ADR 0026 §1/§2).
//!
//! Connects to an APRS-IS server, sends a login line with a server-side **area
//! filter** derived from the configured bounding box, then reads the pushed
//! position stream line by line and turns each aircraft beacon into a [`Plot`].
//!
//! The pure helpers ([`area_filter`], [`login_line`]) and the stream processor
//! ([`run_stream`]) are unit-testable without a network; [`run`] adds the TCP
//! connect + reconnect-with-backoff loop around them.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use firefly_core::{Plot, SensorId};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tracing::{debug, info, warn};

use crate::config::FlarmConfig;
use crate::ogn::parse_position;
use crate::plot::position_to_plot;

/// Placeholder login callsign for read-only access (passcode `-1`). APRS-IS
/// accepts any callsign for a read-only client; `N0CALL` is the conventional
/// "no real station" placeholder.
const READ_ONLY_CALLSIGN: &str = "N0CALL";

/// Build the APRS-IS server-side **area filter** for a bounding box.
///
/// The APRS-IS `a/` filter is `a/latNorth/lonWest/latSouth/lonEast` — note the
/// order (north, west, south, east), which is **not** min/max order.
pub fn area_filter(lat_min: f64, lat_max: f64, lon_min: f64, lon_max: f64) -> String {
    format!("a/{lat_max:.4}/{lon_min:.4}/{lat_min:.4}/{lon_max:.4}")
}

/// Build the APRS-IS login line for a config (terminated with CRLF).
///
/// Without a configured callsign/passcode this is a **read-only** login
/// (`N0CALL`, passcode `-1`); we never transmit (ADR 0026 §2).
pub fn login_line(cfg: &FlarmConfig) -> String {
    let call = cfg
        .callsign
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(READ_ONLY_CALLSIGN);
    let pass = cfg.passcode.unwrap_or(-1);
    let filter = area_filter(cfg.lat_min, cfg.lat_max, cfg.lon_min, cfg.lon_max);
    format!(
        "user {call} pass {pass} vers {} {} filter {filter}\r\n",
        cfg.app_name, cfg.app_version
    )
}

/// Read an APRS-IS line stream to its end, emitting one [`Plot`] per aircraft
/// position beacon. Server comments (`#`), blank lines and unparseable/non-aircraft
/// lines are skipped. Returns when the stream ends or errors.
pub async fn run_stream<R, F>(
    reader: R,
    sensor: SensorId,
    sigma_pos_m: f64,
    mut on_plot: F,
) -> std::io::Result<()>
where
    R: AsyncBufRead + Unpin,
    F: FnMut(Plot),
{
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim_end();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue; // server comment / keep-alive
        }
        match parse_position(trimmed) {
            // The OGN beacon carries only a time-of-day; anchor it to the
            // wall-clock receive instant so the plot time is full Unix-epoch
            // seconds on the same clock as OpenSky (Wayfinder #120).
            Some(pos) => on_plot(position_to_plot(&pos, sensor, sigma_pos_m, unix_now_s())),
            None => debug!(line = %trimmed, "APRS line skipped (not an aircraft position)"),
        }
    }
    Ok(())
}

/// Current wall-clock time as Unix-epoch seconds. Live FLARM beacons are a push
/// stream received in near-real-time, so the receive instant is the day anchor
/// for the beacon's time-of-day (see [`position_to_plot`]). A clock before the
/// epoch (never in practice) clamps to 0.
fn unix_now_s() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Connect, log in, and stream until the connection ends. One attempt.
async fn connect_once<F>(cfg: &FlarmConfig, on_plot: &mut F) -> std::io::Result<()>
where
    F: FnMut(Plot),
{
    let stream = TcpStream::connect((cfg.server.as_str(), cfg.port)).await?;
    let (read_half, mut write_half) = stream.into_split();
    write_half.write_all(login_line(cfg).as_bytes()).await?;
    write_half.flush().await?;
    let reader = BufReader::new(read_half);
    run_stream(reader, cfg.sensor_id, cfg.sigma_pos_m, on_plot).await
}

/// Run the APRS-IS listener indefinitely: connect, log in, stream plots to
/// `on_plot`, and **reconnect with exponential backoff** on any disconnect or
/// error (availability over completeness; the backoff resets after a clean stream
/// end). This method never returns.
pub async fn run<F>(cfg: &FlarmConfig, mut on_plot: F)
where
    F: FnMut(Plot),
{
    let min = cfg.reconnect_min_secs.max(1);
    let max = cfg.reconnect_max_secs.max(min);
    let mut backoff = min;
    info!(
        server = %cfg.server,
        port = cfg.port,
        sensor = %cfg.sensor_id,
        "FLARM/OGN APRS-IS listener started"
    );
    loop {
        match connect_once(cfg, &mut on_plot).await {
            Ok(()) => {
                info!(server = %cfg.server, "FLARM/OGN stream ended; reconnecting");
                backoff = min;
            }
            Err(e) => warn!(server = %cfg.server, error = %e, "FLARM/OGN connection failed"),
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = backoff.saturating_mul(2).min(max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLR_LINE: &str =
        "FLRDDE626>APRS,qAS,EGHL:/074548h5111.32N/00102.04W'086/007/A=000607 id0ADDE626 -019fpm";
    const ICA_LINE: &str =
        "ICA4B4E68>APRS,qAS,Letzi:/152339h4726.50N/00814.20E'260/059/A=002253 !W65!";

    #[test]
    fn area_filter_uses_north_west_south_east_order() {
        assert_eq!(
            area_filter(47.0, 55.0, 5.0, 16.0),
            "a/55.0000/5.0000/47.0000/16.0000"
        );
    }

    #[test]
    fn anonymous_login_is_read_only() {
        let cfg = FlarmConfig::default();
        let line = login_line(&cfg);
        assert!(line.contains("user N0CALL pass -1"), "{line:?}");
        assert!(line.contains("filter a/"), "{line:?}");
        assert!(line.contains("vers firefly-flarm"), "{line:?}");
        assert!(line.ends_with("\r\n"));
    }

    #[test]
    fn account_login_uses_callsign_and_passcode() {
        let cfg = FlarmConfig {
            callsign: Some("EDXY".to_string()),
            passcode: Some(12345),
            ..FlarmConfig::default()
        };
        let line = login_line(&cfg);
        assert!(line.contains("user EDXY pass 12345"), "{line:?}");
    }

    #[tokio::test]
    async fn stream_emits_one_plot_per_aircraft_line() {
        let data =
            format!("# aprsc keep-alive\r\n{FLR_LINE}\r\nnot a packet\r\n{ICA_LINE}\r\n\r\n");
        let mut plots = Vec::new();
        run_stream(BufReader::new(data.as_bytes()), SensorId(210), 20.0, |p| {
            plots.push(p)
        })
        .await
        .expect("stream ok");

        assert_eq!(plots.len(), 2, "two aircraft beacons → two plots");
        // First is FLARM → no ICAO; second is ICAO → ICAO address set.
        assert!(plots[0].mode_ac.icao_address.is_none());
        assert_eq!(plots[1].mode_ac.icao_address, Some(0x4B4E68));
    }
}
