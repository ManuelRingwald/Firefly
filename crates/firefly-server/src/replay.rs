//! Deterministic plot-replay engine (AP9.4c-5, ADR 0020).
//!
//! Re-runs a captured ADS-B tracking session from a `.ffplots` input
//! recording, producing the same [`SystemTrack`] snapshots as the original
//! live run. The replay is **data-time driven**: output ticks fire whenever
//! the tracker's latest data-time crosses the next output boundary — no wall
//! clock involved, so the result is bit-reproducible regardless of CPU load.
//!
//! Primary uses:
//!
//! - **Fault reproduction**: `firefly-replay-plots` replays a production
//!   `.ffplots` file and re-emits the CAT062 multicast stream, allowing
//!   Wayfinder to step through a suspect session without a live OpenSky
//!   connection.
//! - **Regression testing**: tests construct a synthetic `.ffplots` stream,
//!   replay it, and assert that the resulting tracks and their CAT062 encoding
//!   match known-good reference bytes.
//!
//! REQ: NFR-REPRO-001, FR-OPS-006

use std::io::Read;

use firefly_core::{Plot, SystemTrack, Timestamp};
use firefly_opensky::OpenSkyConfig;
use firefly_recorder::ReadError;

use crate::live::{build_live_tracker, LiveTracker};

/// Read all records from a `.ffplots` reader and group them into batches by
/// `recv_unix_ns`.
///
/// Plots with the same receive timestamp were delivered to the tracker in a
/// single [`LiveTracker::ingest`] call during the original live run; grouping
/// them here mirrors that exactly. The caller must have already consumed the
/// file header via [`firefly_recorder::read_plot_file_header`].
pub fn read_plot_batches<R: Read>(reader: &mut R) -> Result<Vec<(u64, Vec<Plot>)>, ReadError> {
    let mut records: Vec<(u64, Plot)> = Vec::new();
    while let Some(record) = firefly_recorder::read_plot_record(reader)? {
        records.push(record);
    }

    let mut batches: Vec<(u64, Vec<Plot>)> = Vec::new();
    let mut i = 0;
    while i < records.len() {
        let ts = records[i].0;
        let len = records[i..].partition_point(|(t, _)| *t == ts);
        let batch: Vec<Plot> = records[i..i + len].iter().map(|(_, p)| *p).collect();
        batches.push((ts, batch));
        i += len;
    }
    Ok(batches)
}

/// Replay a sequence of plot batches through a fresh live tracker, emitting
/// output snapshots on data-time boundaries.
///
/// `batches` is a sequence of `(recv_unix_ns, plots)` pairs — exactly the
/// output of [`read_plot_batches`]. Each batch is fed to
/// [`LiveTracker::ingest`] in order. A [`SystemTrack`] snapshot is emitted
/// via `on_snapshot` whenever the tracker's latest data-time advances past
/// the next multiple of `output_period_secs`. One additional snapshot is
/// emitted after all batches are consumed, if any tracks remain (covers
/// updates after the last scheduled tick).
///
/// `before_batch` is called with `recv_unix_ns` just before each batch is
/// ingested; the binary uses this hook to pace replay to wall-clock time.
/// Pass `|_| {}` to disable pacing.
///
/// Returns the total number of [`Plot`]s replayed.
pub fn replay_batches<B, F>(
    batches: &[(u64, Vec<Plot>)],
    config: &OpenSkyConfig,
    output_period_secs: f64,
    mut before_batch: B,
    mut on_snapshot: F,
) -> u64
where
    B: FnMut(u64),
    F: FnMut(Timestamp, Vec<SystemTrack>),
{
    let mut live = LiveTracker::new(build_live_tracker(config), None);
    let mut next_output_secs: Option<f64> = None;
    let mut total_plots: u64 = 0;

    for (recv_unix_ns, batch) in batches {
        before_batch(*recv_unix_ns);
        live.ingest(batch, *recv_unix_ns);
        total_plots += batch.len() as u64;

        // Emit a snapshot whenever data-time crosses the next output boundary.
        if let Some(data_time) = live.latest_data_time() {
            let t = data_time.as_secs();
            let emit = match next_output_secs {
                None => true,
                Some(next) => t >= next,
            };
            if emit {
                // Advance the boundary past the current data-time.
                if next_output_secs.is_none() {
                    next_output_secs = Some(t);
                }
                let next = next_output_secs.as_mut().unwrap();
                while *next <= t {
                    *next += output_period_secs;
                }
                on_snapshot(data_time, live.snapshot());
            }
        }
    }

    // Final snapshot for any tracks remaining after the last scheduled tick.
    if let Some(data_time) = live.latest_data_time() {
        let snapshot = live.snapshot();
        if !snapshot.is_empty() {
            on_snapshot(data_time, snapshot);
        }
    }

    total_plots
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use firefly_core::{Callsign, ModeAC, SensorId};
    use firefly_geo::Wgs84;

    fn default_config() -> OpenSkyConfig {
        OpenSkyConfig::default()
    }

    fn adsb(time_secs: f64, lat: f64, lon: f64, icao: u32) -> Plot {
        Plot::adsb(
            SensorId(200),
            Timestamp(time_secs),
            Wgs84::from_degrees(lat, lon, 10_000.0),
            75.0,
            ModeAC {
                mode_3a: Some(0o1234),
                flight_level_ft: Some(33_000.0),
                icao_address: Some(icao),
                callsign: Some(Callsign::new("DLH001")),
                spi: false,
                daps: firefly_core::Daps::default(),
            },
        )
    }

    /// Build a `.ffplots` byte buffer for a sequence of (recv_ns, plot) pairs.
    fn build_ffplots(records: &[(u64, Plot)]) -> Vec<u8> {
        let mut buf = Vec::new();
        firefly_recorder::write_plot_file_header(&mut buf).unwrap();
        for (ts, plot) in records {
            firefly_recorder::write_plot_record(&mut buf, *ts, plot).unwrap();
        }
        buf
    }

    #[test]
    fn read_plot_batches_groups_same_timestamp() {
        let t0 = 1_000_000_000u64;
        let records = vec![
            (t0, adsb(0.0, 51.0, 10.5, 0xAA)),
            (t0, adsb(0.0, 50.0, 11.0, 0xBB)), // same recv_ns → same batch
            (t0 + 1, adsb(10.0, 51.1, 10.5, 0xAA)), // different recv_ns → new batch
        ];
        let buf = build_ffplots(&records);
        let mut reader = Cursor::new(&buf);
        firefly_recorder::read_plot_file_header(&mut reader).unwrap();

        let batches = read_plot_batches(&mut reader).unwrap();
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].1.len(), 2);
        assert_eq!(batches[1].1.len(), 1);
    }

    #[test]
    fn replay_confirms_known_aircraft() {
        // Eight ADS-B polls, 10 s apart, one aircraft at the bbox midpoint.
        let base_ns: u64 = 1_718_000_000_000_000_000;
        let records: Vec<(u64, Plot)> = (0..8)
            .map(|k| {
                let t = k as f64 * 10.0;
                let recv = base_ns + k * 10_000_000_000;
                (recv, adsb(t, 51.0, 10.5 + k as f64 * 0.01, 0x3C_AB_CD))
            })
            .collect();

        let buf = build_ffplots(&records);
        let mut reader = Cursor::new(&buf);
        firefly_recorder::read_plot_file_header(&mut reader).unwrap();
        let batches = read_plot_batches(&mut reader).unwrap();

        let config = default_config();
        let mut snapshots: Vec<(Timestamp, Vec<SystemTrack>)> = Vec::new();
        let total = replay_batches(
            &batches,
            &config,
            60.0,
            |_| {},
            |t, tracks| {
                snapshots.push((t, tracks));
            },
        );

        assert_eq!(total, 8, "all eight plots replayed");
        assert!(!snapshots.is_empty(), "at least one snapshot emitted");

        // Find the final snapshot and verify the confirmed track.
        let (_, final_tracks) = snapshots.last().unwrap();
        let track = final_tracks
            .iter()
            .find(|t| t.icao_address == Some(0x3C_AB_CD));
        assert!(
            track.is_some(),
            "aircraft ICAO 0x3CABCD is in the final snapshot"
        );
        let track = track.unwrap();
        assert!(track.confirmed, "eight steady hits confirm the track");
        assert!(
            (track.position.lat_deg() - 51.0).abs() < 0.5,
            "track is near the expected latitude"
        );
    }

    #[test]
    fn replay_output_ticks_fire_on_data_time() {
        // Forty polls, 10 s apart → 390 s of data. With 60 s output period,
        // expect 6 or 7 snapshots (first at t≈0, then at 60, 120, 180, 240, 300,
        // 360, plus one final), depending on exact tick arithmetic.
        let base_ns: u64 = 1_718_000_000_000_000_000;
        let records: Vec<(u64, Plot)> = (0..40)
            .map(|k| {
                let t = k as f64 * 10.0;
                let recv = base_ns + k * 10_000_000_000;
                (recv, adsb(t, 51.0, 10.5, 0x3C_00_01))
            })
            .collect();

        let buf = build_ffplots(&records);
        let mut reader = Cursor::new(&buf);
        firefly_recorder::read_plot_file_header(&mut reader).unwrap();
        let batches = read_plot_batches(&mut reader).unwrap();

        let mut snapshot_count = 0usize;
        replay_batches(
            &batches,
            &default_config(),
            60.0,
            |_| {},
            |_, _| {
                snapshot_count += 1;
            },
        );

        // 390 s / 60 s period = 6 ticks, plus the final snapshot = 7 or 8.
        assert!(
            snapshot_count >= 6,
            "expected at least 6 snapshots, got {snapshot_count}"
        );
    }

    #[test]
    fn replay_deterministic_same_input_same_tracks() {
        let base_ns: u64 = 1_718_000_000_000_000_000;
        let records: Vec<(u64, Plot)> = (0..6)
            .map(|k| {
                let t = k as f64 * 10.0;
                let recv = base_ns + k * 10_000_000_000;
                (recv, adsb(t, 51.0, 10.5 + k as f64 * 0.01, 0x3C_01_01))
            })
            .collect();
        let buf = build_ffplots(&records);

        // Run twice; collect final snapshot ICAO addresses.
        let run = || {
            let mut reader = Cursor::new(&buf);
            firefly_recorder::read_plot_file_header(&mut reader).unwrap();
            let batches = read_plot_batches(&mut reader).unwrap();
            let mut last: Vec<SystemTrack> = Vec::new();
            replay_batches(
                &batches,
                &default_config(),
                120.0,
                |_| {},
                |_, tracks| {
                    last = tracks;
                },
            );
            last.iter().map(|t| t.icao_address).collect::<Vec<_>>()
        };

        assert_eq!(run(), run(), "two replay runs produce identical track sets");
    }
}
