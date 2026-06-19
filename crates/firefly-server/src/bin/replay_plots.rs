//! `firefly-replay-plots` — replay a `.ffplots` input recording (AP9.4c-5, ADR 0020).
//!
//! Reads a `.ffplots` file recorded during a live ADS-B session and re-runs the
//! same tracking session deterministically. For each output tick a CAT062 data
//! block is encoded and, if the multicast feed is enabled, sent to the configured
//! multicast group. This lets Wayfinder (or any other CAT062 consumer) replay a
//! production session without a live OpenSky connection.
//!
//! Timing: when `FIREFLY_REPLAY_PLOTS_SPEED` is positive, each batch is released
//! at the wall-clock instant proportional to its original `recv_unix_ns` offset
//! (drift-free: anchored to the first batch's timestamp). Set the speed to `0` to
//! run as fast as possible — useful for CI and batch analysis.
//!
//! ## Configuration (environment variables)
//!
//! | Variable | Default | Meaning |
//! |----------|---------|---------|
//! | `FIREFLY_REPLAY_PLOTS_INPUT` | *(required)* | Path to `.ffplots` input file |
//! | `FIREFLY_REPLAY_PLOTS_SPEED` | `1.0` | Playback speed (0 = unconstrained) |
//! | `FIREFLY_REPLAY_PLOTS_OUTPUT_PERIOD_SECS` | `10.0` | Data-time interval between CAT062 outputs |
//! | `FIREFLY_CAT062_ENABLED` | `false` | Set `true` to actually send UDP datagrams |
//! | `FIREFLY_CAT062_GROUP` | `239.255.0.62` | Multicast group |
//! | `FIREFLY_CAT062_PORT` | `8600` | UDP port |
//! | `FIREFLY_CAT062_SAC` / `_SIC` | `25` / `2` | Data-source identity for I062/010 |
//! | `FIREFLY_OPENSKY_LAT_MIN/MAX` | `47.0/55.0` | Bbox south/north (sets tracker reference point) |
//! | `FIREFLY_OPENSKY_LON_MIN/MAX` | `5.0/16.0` | Bbox west/east |
//! | `FIREFLY_OPENSKY_SENSOR_ID` | `200` | Sensor ID recorded in the plots file |
//! | `RUST_LOG` | `info` | Log filter |

use std::fs::File;
use std::io::BufReader;
use std::net::{Ipv4Addr, UdpSocket};
use std::time::{Duration, Instant};

use firefly_asterix::Cat062Encoder;
use firefly_multicast::MulticastConfig;
use firefly_opensky::OpenSkyConfig;
use firefly_server::replay::{read_plot_batches, replay_batches};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".parse().unwrap()),
        )
        .init();

    let input = std::env::var("FIREFLY_REPLAY_PLOTS_INPUT")
        .map_err(|_| "FIREFLY_REPLAY_PLOTS_INPUT must be set")?;
    let speed: f64 = std::env::var("FIREFLY_REPLAY_PLOTS_SPEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&s: &f64| s.is_finite() && s >= 0.0)
        .unwrap_or(1.0);
    let output_period_secs: f64 = std::env::var("FIREFLY_REPLAY_PLOTS_OUTPUT_PERIOD_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&s: &f64| s.is_finite() && s > 0.0)
        .unwrap_or(10.0);

    let opensky_config = OpenSkyConfig::from_env();
    let mc_config = MulticastConfig::from_env();
    let destination = mc_config.destination();
    let encoder = Cat062Encoder::new(mc_config.data_source(), mc_config.reference_point, 0.0);

    // Blocking UDP socket — fine for a replay/batch tool.
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0u16))?;

    let file = File::open(&input).map_err(|e| format!("cannot open input file {input:?}: {e}"))?;
    let mut reader = BufReader::new(file);
    firefly_recorder::read_plot_file_header(&mut reader)
        .map_err(|e| format!("invalid .ffplots file: {e}"))?;

    let batches = read_plot_batches(&mut reader)
        .map_err(|e| format!("failed to read .ffplots batches: {e}"))?;

    tracing::info!(
        input,
        batches = batches.len(),
        speed,
        output_period_secs,
        multicast_enabled = mc_config.enabled,
        %destination,
        "replay-plots started"
    );

    if batches.is_empty() {
        tracing::warn!("input file contains no plot records; nothing to replay");
        return Ok(());
    }

    // Drift-free pacing: anchor to the first batch's recv_ns so all subsequent
    // target times are computed relative to a single origin (no per-gap drift).
    let first_recv_ns = batches[0].0;
    let wall_start = Instant::now();
    let mut snapshots_sent: u64 = 0;
    let mut blocks_skipped: u64 = 0;

    let total_plots = replay_batches(
        &batches,
        &opensky_config,
        output_period_secs,
        |recv_unix_ns| {
            if speed > 0.0 {
                // Compute wall-clock target for this batch.
                let data_offset_ns = recv_unix_ns.saturating_sub(first_recv_ns);
                let wall_target_ns = (data_offset_ns as f64 / speed) as u64;
                let target = wall_start + Duration::from_nanos(wall_target_ns);
                let now = Instant::now();
                if now < target {
                    std::thread::sleep(target - now);
                }
            }
        },
        |time, tracks| {
            if tracks.is_empty() {
                return;
            }
            let block = encoder.encode(time, &tracks);
            if mc_config.enabled {
                match socket.send_to(&block, destination) {
                    Ok(_) => {
                        snapshots_sent += 1;
                        tracing::debug!(
                            tracks = tracks.len(),
                            bytes = block.len(),
                            "sent CAT062 block"
                        );
                    }
                    Err(e) => {
                        tracing::error!(%e, %destination, "failed to send CAT062 block");
                        blocks_skipped += 1;
                    }
                }
            } else {
                // Log even when multicast is disabled (dry-run / testing).
                snapshots_sent += 1;
                tracing::info!(
                    tracks = tracks.len(),
                    bytes = block.len(),
                    "CAT062 block encoded (multicast disabled — set FIREFLY_CAT062_ENABLED=true to send)"
                );
            }
        },
    );

    tracing::info!(
        total_plots,
        snapshots_sent,
        blocks_skipped,
        "replay-plots complete"
    );
    Ok(())
}
