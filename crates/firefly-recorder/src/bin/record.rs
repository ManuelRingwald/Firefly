//! `firefly-record` — capture the CAT062/CAT065 multicast feed to a `.ffrec` file.
//!
//! The recorder joins the same multicast group as any other consumer (Wayfinder,
//! an EFS …) and writes every arriving datagram with its wall-clock receive
//! timestamp to a binary `.ffrec` file (see [`firefly_recorder`] for the format).
//! Because Firefly is deterministic after data-time, replaying this file with
//! `firefly-replay` reproduces the exact feed that any consumer would have seen.
//!
//! ## Configuration (environment variables)
//!
//! | Variable | Default | Meaning |
//! |----------|---------|---------|
//! | `FIREFLY_CAT062_GROUP` | `239.255.0.62` | Multicast group to listen on |
//! | `FIREFLY_CAT062_PORT`  | `8600`          | UDP port |
//! | `FIREFLY_RECORD_OUTPUT`| `recording.ffrec` | Output file path |
//! | `RUST_LOG`             | `info`          | Log filter (tracing) |
//!
//! Stop with Ctrl+C; the file is flushed and closed cleanly.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::net::Ipv4Addr;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::net::UdpSocket;
use tokio::signal;

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
    let output =
        std::env::var("FIREFLY_RECORD_OUTPUT").unwrap_or_else(|_| "recording.ffrec".to_string());

    // Join the multicast group on all interfaces — same recipe as Wayfinder's
    // receiver and firefly-multicast's receiver_socket.
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    socket.join_multicast_v4(group, Ipv4Addr::UNSPECIFIED)?;

    let file =
        File::create(&output).map_err(|e| format!("cannot create output file {output:?}: {e}"))?;
    let mut writer = BufWriter::new(file);
    firefly_recorder::write_file_header(&mut writer)?;

    tracing::info!(%group, port, output, "recording started — Ctrl+C to stop");

    let mut buf = [0u8; firefly_recorder::MAX_DATAGRAM_BYTES];
    let mut count = 0u64;

    loop {
        tokio::select! {
            biased;
            _ = signal::ctrl_c() => {
                tracing::info!(count, "Ctrl+C received — flushing and stopping");
                break;
            }
            result = socket.recv_from(&mut buf) => {
                let (n, _peer) = result?;
                let ts_ns = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system clock before UNIX epoch")
                    .as_nanos() as u64;
                if let Err(e) = firefly_recorder::write_record(&mut writer, ts_ns, &buf[..n]) {
                    tracing::error!(%e, "failed to write record — stopping");
                    return Err(e.into());
                }
                count += 1;
                if count.is_multiple_of(100) {
                    tracing::debug!(count, "datagrams recorded");
                }
            }
        }
    }

    writer.flush()?;
    tracing::info!(count, output, "recording complete");
    Ok(())
}
