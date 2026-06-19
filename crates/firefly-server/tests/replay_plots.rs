//! Integration test: `.ffplots` replay → correct tracks and CAT062 output
//! (AP9.4c-5, ADR 0020, NFR-REPRO-001).
//!
//! Verifies that replaying a synthetic `.ffplots` recording through the live
//! tracker produces the same confirmed tracks as the original live run, and that
//! re-encoding those tracks as CAT062 gives a deterministic, non-empty result.

use std::io::Cursor;

use firefly_asterix::Cat062Encoder;
use firefly_core::{Callsign, ModeAC, Plot, SensorId, Timestamp};
use firefly_geo::Wgs84;
use firefly_multicast::MulticastConfig;
use firefly_opensky::OpenSkyConfig;
use firefly_server::replay::{read_plot_batches, replay_batches};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_opensky() -> OpenSkyConfig {
    OpenSkyConfig::default()
}

fn adsb(time_secs: f64, lat: f64, lon: f64, icao: u32, callsign: &str) -> Plot {
    Plot::adsb(
        SensorId(200),
        Timestamp(time_secs),
        Wgs84::from_degrees(lat, lon, 10_000.0),
        75.0,
        ModeAC {
            mode_3a: Some(0o7700),
            flight_level_ft: Some(35_000.0),
            icao_address: Some(icao),
            callsign: Some(Callsign::new(callsign)),
        },
    )
}

/// Build an in-memory `.ffplots` byte vector.
fn build_ffplots(records: &[(u64, Plot)]) -> Vec<u8> {
    let mut buf = Vec::new();
    firefly_recorder::write_plot_file_header(&mut buf).unwrap();
    for (ts, plot) in records {
        firefly_recorder::write_plot_record(&mut buf, *ts, plot).unwrap();
    }
    buf
}

/// Replay the given `.ffplots` buffer; return all emitted snapshots.
fn replay(buf: &[u8], output_period_secs: f64) -> Vec<(Timestamp, Vec<firefly_core::SystemTrack>)> {
    let mut reader = Cursor::new(buf);
    firefly_recorder::read_plot_file_header(&mut reader).unwrap();
    let batches = read_plot_batches(&mut reader).unwrap();

    let mut snapshots = Vec::new();
    replay_batches(
        &batches,
        &default_opensky(),
        output_period_secs,
        |_| {},
        |t, tracks| snapshots.push((t, tracks)),
    );
    snapshots
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A steady stream of ADS-B hits for one aircraft confirms a single track.
#[test]
fn replay_confirms_one_aircraft() {
    let base_ns: u64 = 1_718_000_000_000_000_000;
    let records: Vec<(u64, Plot)> = (0..8)
        .map(|k| {
            let recv = base_ns + k * 10_000_000_000u64;
            let t = k as f64 * 10.0;
            (
                recv,
                adsb(t, 51.0, 10.5 + k as f64 * 0.01, 0x3C_AB_CD, "DLH401 "),
            )
        })
        .collect();
    let buf = build_ffplots(&records);

    let snapshots = replay(&buf, 120.0);
    assert!(!snapshots.is_empty(), "at least one snapshot emitted");

    let (_, final_tracks) = snapshots.last().unwrap();
    let track = final_tracks
        .iter()
        .find(|t| t.icao_address == Some(0x3C_AB_CD))
        .expect("aircraft ICAO 0x3CABCD must appear in final snapshot");
    assert!(track.confirmed, "eight hits confirm the track");
    assert!(
        (track.position.lat_deg() - 51.0).abs() < 0.5,
        "track latitude near 51°N"
    );
    assert!(
        (track.position.lon_deg() - 10.5).abs() < 0.5,
        "track longitude near 10.5°E"
    );
}

/// Two aircraft with distinct ICAO addresses produce two independent tracks.
#[test]
fn replay_two_aircraft_two_tracks() {
    let base_ns: u64 = 1_718_000_000_000_000_000;
    let records: Vec<(u64, Plot)> = (0..6)
        .flat_map(|k| {
            let recv = base_ns + k * 10_000_000_000u64;
            let t = k as f64 * 10.0;
            [
                (recv, adsb(t, 51.0, 10.5, 0x3C_00_01, "DLH001 ")),
                (recv, adsb(t, 50.0, 11.5, 0x3C_00_02, "LH002  ")),
            ]
        })
        .collect();
    let buf = build_ffplots(&records);

    let snapshots = replay(&buf, 120.0);
    let (_, final_tracks) = snapshots.last().unwrap();

    let icaos: std::collections::BTreeSet<u32> =
        final_tracks.iter().filter_map(|t| t.icao_address).collect();
    assert!(icaos.contains(&0x3C_00_01), "first aircraft in tracks");
    assert!(icaos.contains(&0x3C_00_02), "second aircraft in tracks");
}

/// Replaying the same `.ffplots` file twice produces identical track sets.
/// Verifies the determinism guarantee (NFR-REPRO-001).
#[test]
fn replay_is_deterministic() {
    let base_ns: u64 = 1_718_000_000_000_000_000;
    let records: Vec<(u64, Plot)> = (0..8)
        .map(|k| {
            let recv = base_ns + k * 10_000_000_000u64;
            let t = k as f64 * 10.0;
            (
                recv,
                adsb(t, 51.0, 10.5 + k as f64 * 0.01, 0x3C_11_22, "BAW777 "),
            )
        })
        .collect();
    let buf = build_ffplots(&records);

    let run = || {
        let snapshots = replay(&buf, 120.0);
        let (_, tracks) = snapshots.last().unwrap().clone();
        tracks
            .iter()
            .map(|t| (t.icao_address, t.confirmed))
            .collect::<Vec<_>>()
    };

    assert_eq!(run(), run(), "two replay runs produce identical tracks");
}

/// CAT062 encoding of the replayed snapshot is non-empty and has the correct
/// leading category byte (`0x3E`).
#[test]
fn replayed_snapshot_encodes_to_valid_cat062() {
    let base_ns: u64 = 1_718_000_000_000_000_000;
    let records: Vec<(u64, Plot)> = (0..8)
        .map(|k| {
            let recv = base_ns + k * 10_000_000_000u64;
            let t = k as f64 * 10.0;
            (recv, adsb(t, 51.0, 10.5, 0x3C_AA_BB, "EZY123 "))
        })
        .collect();
    let buf = build_ffplots(&records);

    let mc = MulticastConfig::from_env(); // uses defaults
                                          // I062/100 reference = system reference point (ADR 0021): same origin the
                                          // plots are replayed in (bbox midpoint of the default OpenSky config).
    let reference = firefly_server::live_system_reference_point(&default_opensky());
    let encoder = Cat062Encoder::new(mc.data_source(), reference, 0.0);

    let mut cat062_blocks: Vec<Vec<u8>> = Vec::new();
    let mut reader = Cursor::new(&buf);
    firefly_recorder::read_plot_file_header(&mut reader).unwrap();
    let batches = read_plot_batches(&mut reader).unwrap();
    replay_batches(
        &batches,
        &default_opensky(),
        120.0,
        |_| {},
        |time, tracks| {
            if !tracks.is_empty() {
                cat062_blocks.push(encoder.encode(time, &tracks));
            }
        },
    );

    assert!(
        !cat062_blocks.is_empty(),
        "at least one CAT062 block encoded"
    );
    for block in &cat062_blocks {
        assert!(!block.is_empty(), "block must not be empty");
        assert_eq!(
            block[0], 0x3E,
            "leading byte must be CAT062 category (0x3E)"
        );
        // LEN field (bytes 1–2) must match actual block length.
        let declared_len = u16::from_be_bytes([block[1], block[2]]) as usize;
        assert_eq!(
            declared_len,
            block.len(),
            "declared LEN matches actual block size"
        );
    }
}

/// An empty `.ffplots` file (header only) replays zero plots and emits no snapshots.
#[test]
fn empty_file_replays_cleanly() {
    let mut buf = Vec::new();
    firefly_recorder::write_plot_file_header(&mut buf).unwrap();

    let snapshots = replay(&buf, 10.0);
    assert!(snapshots.is_empty(), "no snapshots for empty recording");
}
