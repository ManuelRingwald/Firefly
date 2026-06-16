//! `firefly-replay` — replay a `.ffrec` recording onto the CAT062 multicast feed.
//!
//! The replayer reads a `.ffrec` file written by `firefly-record` and re-sends
//! each datagram to the configured multicast group, preserving the original
//! inter-packet timing (optionally scaled by `FIREFLY_REPLAY_SPEED`). Any
//! consumer listening on that group — Wayfinder, an EFS, another recorder —
//! receives the exact bytes that were on the wire during the original session.
//!
//! Timing accuracy: rather than computing per-gap delays (which accumulate
//! drift), the replayer anchors to the wall-clock start time and computes each
//! datagram's absolute target time from the first record's timestamp. This keeps
//! the replay schedule drift-free for arbitrarily long files.
//!
//! ## Configuration (environment variables)
//!
//! | Variable | Default | Meaning |
//! |----------|---------|---------|
//! | `FIREFLY_CAT062_GROUP` | `239.255.0.62` | Multicast group to send to |
//! | `FIREFLY_CAT062_PORT`  | `8600`          | UDP port |
//! | `FIREFLY_REPLAY_INPUT` | `recording.ffrec` | Input file path |
//! | `FIREFLY_REPLAY_SPEED` | `1.0`           | Playback speed (2.0 = double speed) |
//! | `RUST_LOG`             | `info`          | Log filter (tracing) |

use std::fs::File;
use std::io::BufReader;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".parse().unwrap()),
        )
        .init();

    let group: Ipv4Addr = std::env::var("FIREFLY_CAT062_GROUP")
        .unwrap_or_else(|_| "239.255.0.62".to_string())
        .parse()
        .map_err(|e| format!("FIREFLY_CAT062_GROUP: {e}"))?;
    let port: u16 = std::env::var("FIREFLY_CAT062_PORT")
        .unwrap_or_else(|_| "8600".to_string())
        .parse()
        .map_err(|e| format!("FIREFLY_CAT062_PORT: {e}"))?;
    let speed: f64 = std::env::var("FIREFLY_REPLAY_SPEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&s: &f64| s.is_finite() && s > 0.0)
        .unwrap_or(1.0);
    let input =
        std::env::var("FIREFLY_REPLAY_INPUT").unwrap_or_else(|_| "recording.ffrec".to_string());

    let file = File::open(&input).map_err(|e| format!("cannot open input file {input:?}: {e}"))?;
    let mut reader = BufReader::new(file);
    firefly_recorder::read_file_header(&mut reader)?;

    let destination = SocketAddr::new(IpAddr::V4(group), port);
    // Bind to an ephemeral local port — sending multicast is just sending to
    // a special destination address (no socket option needed for the sender).
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0u16)).await?;

    tracing::info!(input, %destination, speed, "replay started");

    let wall_start = Instant::now();
    let mut first_ts: Option<u64> = None;
    let mut count = 0u64;

    while let Some((ts_ns, payload)) = firefly_recorder::read_record(&mut reader)? {
        // Compute the wall-clock instant at which this datagram should be sent,
        // anchored to wall_start (no per-gap drift accumulation).
        let origin = *first_ts.get_or_insert(ts_ns);
        let elapsed_recording_ns = ts_ns.saturating_sub(origin);
        let target_wall_ns = (elapsed_recording_ns as f64 / speed) as u64;
        let target = wall_start + Duration::from_nanos(target_wall_ns);
        tokio::time::sleep_until(target).await;

        match socket.send_to(&payload, destination).await {
            Ok(bytes) => {
                tracing::debug!(count, bytes, %destination, "sent datagram");
            }
            Err(e) => {
                tracing::error!(count, %e, %destination, "failed to send datagram");
                return Err(e.into());
            }
        }
        count += 1;
    }

    tracing::info!(count, "replay complete");
    Ok(())
}
