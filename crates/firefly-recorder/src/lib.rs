//! The `.ffrec` recording file format for SDPS-005 Legal Recording & Replay.
//!
//! A `.ffrec` file captures raw UDP datagrams from the CAT062/CAT065 multicast
//! feed with wall-clock receive timestamps. Because Firefly processes by
//! data-time (deterministic), replaying the same datagrams reproduces the exact
//! bit-for-bit feed any consumer would have received live.
//!
//! ## File layout
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │  File header (16 bytes)                         │
//! │    magic:    8 bytes  = b"FFREC\x00\x00\x00"   │
//! │    version:  1 byte   = 0x01                    │
//! │    reserved: 7 bytes  = 0x00…                   │
//! ├─────────────────────────────────────────────────┤
//! │  Record 0                                       │
//! │    timestamp_unix_ns: u64 big-endian            │
//! │    length:            u16 big-endian            │
//! │    payload:           <length> bytes            │
//! ├─────────────────────────────────────────────────┤
//! │  Record 1 …                                     │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! The format is intentionally minimal: no index, no checksums, no
//! compression. Simplicity is an audit virtue — every byte is
//! straightforwardly accountable.
//!
//! ## Two recording layers (ADR 0020)
//!
//! This crate captures **two** complementary layers. They share the same record
//! framing (`timestamp_unix_ns` + `length` + `payload`) and are distinguished by
//! their file magic:
//!
//! - **`.ffrec` (output layer, SDPS-005):** raw CAT062/CAT065 UDP datagrams as
//!   they leave Firefly. Replaying reproduces the exact feed a consumer saw.
//!   See [`write_record`] / [`read_record`].
//! - **`.ffplots` (input layer, ADR 0020):** the [`Plot`] stream *entering* the
//!   tracker, serialised as JSON. Replaying reproduces the exact tracking run —
//!   the basis for reproducing production faults. See [`write_plot_record`] /
//!   [`read_plot_record`].
//!
//! The input layer is source-agnostic: ADS-B, PSR/SSR and FLARM plots all
//! serialise through the same [`Plot`] type, so no per-source replay logic is
//! needed.
//!
//! REQ: FR-OPS-005, FR-OPS-006

use std::io::{self, Read, Write};

use firefly_core::Plot;

/// File magic — identifies a `.ffrec` recording file (output layer).
pub const MAGIC: &[u8; 8] = b"FFREC\x00\x00\x00";

/// File magic — identifies a `.ffplots` recording file (input/plot layer, ADR 0020).
pub const PLOT_MAGIC: &[u8; 8] = b"FFPLOTS\x00";

/// Current format version stored in the header (shared by both layers).
pub const VERSION: u8 = 1;

/// Total file header length in bytes (magic + version + reserved).
pub const HEADER_LEN: usize = 16;

/// Maximum accepted datagram payload size.
///
/// 64 KiB is the hard upper bound of a UDP datagram. Any `.ffrec` record
/// claiming a larger payload is corrupt and is rejected by [`read_record`].
pub const MAX_DATAGRAM_BYTES: usize = 65535;

/// Errors that can occur while reading a `.ffrec` or `.ffplots` file.
#[derive(Debug)]
pub enum ReadError {
    /// An underlying I/O error (includes unexpected EOF mid-record).
    Io(io::Error),
    /// The file does not begin with the expected magic bytes.
    BadMagic,
    /// The format version is not supported by this implementation.
    UnsupportedVersion(u8),
    /// A record's declared payload length exceeds [`MAX_DATAGRAM_BYTES`].
    PayloadTooLarge(usize),
    /// A `.ffplots` record's JSON payload could not be deserialised into a
    /// [`Plot`] — the record is corrupt or was written by an incompatible
    /// version.
    PlotDeserialize(serde_json::Error),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Io(e) => write!(f, "I/O error: {e}"),
            ReadError::BadMagic => write!(f, "bad magic (not a recognised Firefly recording file)"),
            ReadError::UnsupportedVersion(v) => write!(f, "unsupported recording version {v}"),
            ReadError::PayloadTooLarge(n) => write!(f, "record payload too large: {n} bytes"),
            ReadError::PlotDeserialize(e) => write!(f, "malformed plot record: {e}"),
        }
    }
}

impl std::error::Error for ReadError {}

impl From<io::Error> for ReadError {
    fn from(e: io::Error) -> Self {
        ReadError::Io(e)
    }
}

/// Write a 16-byte file header carrying `magic` (+ current version) to `w`.
fn write_header_with_magic(w: &mut impl Write, magic: &[u8; 8]) -> io::Result<()> {
    let mut header = [0u8; HEADER_LEN];
    header[..8].copy_from_slice(magic);
    header[8] = VERSION;
    // bytes 9–15: reserved, already zero
    w.write_all(&header)
}

/// Read and validate a 16-byte file header against the expected `magic`.
fn read_header_with_magic(r: &mut impl Read, magic: &[u8; 8]) -> Result<(), ReadError> {
    let mut header = [0u8; HEADER_LEN];
    r.read_exact(&mut header).map_err(ReadError::Io)?;
    if &header[..8] != magic {
        return Err(ReadError::BadMagic);
    }
    let version = header[8];
    if version != VERSION {
        return Err(ReadError::UnsupportedVersion(version));
    }
    Ok(())
}

/// Write the 16-byte `.ffrec` file header (output layer) to `w`.
pub fn write_file_header(w: &mut impl Write) -> io::Result<()> {
    write_header_with_magic(w, MAGIC)
}

/// Read and validate the 16-byte `.ffrec` file header (output layer) from `r`.
///
/// Returns [`ReadError::BadMagic`] or [`ReadError::UnsupportedVersion`] on
/// format mismatch — both are fatal for the current file.
pub fn read_file_header(r: &mut impl Read) -> Result<(), ReadError> {
    read_header_with_magic(r, MAGIC)
}

/// Write the 16-byte `.ffplots` file header (input/plot layer) to `w`.
pub fn write_plot_file_header(w: &mut impl Write) -> io::Result<()> {
    write_header_with_magic(w, PLOT_MAGIC)
}

/// Read and validate the 16-byte `.ffplots` file header (input/plot layer)
/// from `r`. Same failure modes as [`read_file_header`].
pub fn read_plot_file_header(r: &mut impl Read) -> Result<(), ReadError> {
    read_header_with_magic(r, PLOT_MAGIC)
}

/// Append one datagram record to `w`.
///
/// `timestamp_unix_ns` is the wall-clock receive time (nanoseconds since
/// the Unix epoch). `payload` is the raw UDP datagram bytes, at most 65 535
/// bytes (the UDP maximum).
pub fn write_record(w: &mut impl Write, timestamp_unix_ns: u64, payload: &[u8]) -> io::Result<()> {
    assert!(
        payload.len() <= MAX_DATAGRAM_BYTES,
        "datagram exceeds UDP maximum ({} bytes)",
        payload.len()
    );
    w.write_all(&timestamp_unix_ns.to_be_bytes())?;
    w.write_all(&(payload.len() as u16).to_be_bytes())?;
    w.write_all(payload)
}

/// Read the next record from `r`.
///
/// Returns `Ok(None)` on a clean end-of-file (no bytes consumed),
/// `Ok(Some((timestamp_unix_ns, payload)))` for a valid record, or `Err` if
/// the read fails or the record is malformed.
pub fn read_record(r: &mut impl Read) -> Result<Option<(u64, Vec<u8>)>, ReadError> {
    let mut ts_buf = [0u8; 8];
    // A clean EOF at the start of a record means the file is complete.
    match r.read_exact(&mut ts_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(ReadError::Io(e)),
    }
    let timestamp_unix_ns = u64::from_be_bytes(ts_buf);

    let mut len_buf = [0u8; 2];
    r.read_exact(&mut len_buf)?;
    let length = u16::from_be_bytes(len_buf) as usize;
    if length > MAX_DATAGRAM_BYTES {
        return Err(ReadError::PayloadTooLarge(length));
    }

    let mut payload = vec![0u8; length];
    r.read_exact(&mut payload)?;
    Ok(Some((timestamp_unix_ns, payload)))
}

/// Append one [`Plot`] record to a `.ffplots` file (input/plot layer, ADR 0020).
///
/// `timestamp_unix_ns` is the wall-clock instant the plot entered the tracker
/// ingest channel — the replay schedule is reconstructed from these. The plot
/// itself is serialised as JSON; its framing is identical to [`write_record`],
/// so the same drift-free replay logic applies. A serialisation failure is
/// surfaced as an [`io::Error`] (it cannot happen for a well-formed [`Plot`],
/// whose fields are all plain numbers and enums).
pub fn write_plot_record(
    w: &mut impl Write,
    timestamp_unix_ns: u64,
    plot: &Plot,
) -> io::Result<()> {
    let payload = serde_json::to_vec(plot).map_err(io::Error::other)?;
    write_record(w, timestamp_unix_ns, &payload)
}

/// Read the next [`Plot`] record from a `.ffplots` file.
///
/// Returns `Ok(None)` on a clean end-of-file, `Ok(Some((timestamp_unix_ns,
/// plot)))` for a valid record, or [`ReadError::PlotDeserialize`] if the JSON
/// payload is not a valid [`Plot`].
pub fn read_plot_record(r: &mut impl Read) -> Result<Option<(u64, Plot)>, ReadError> {
    match read_record(r)? {
        None => Ok(None),
        Some((timestamp_unix_ns, payload)) => {
            let plot = serde_json::from_slice(&payload).map_err(ReadError::PlotDeserialize)?;
            Ok(Some((timestamp_unix_ns, plot)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn header_round_trip() {
        let mut buf = Vec::new();
        write_file_header(&mut buf).unwrap();
        assert_eq!(buf.len(), HEADER_LEN);
        read_file_header(&mut Cursor::new(&buf)).unwrap();
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut buf = vec![0u8; HEADER_LEN];
        // Overwrite first byte so magic is wrong.
        buf[0] = b'X';
        buf[8] = VERSION;
        assert!(matches!(
            read_file_header(&mut Cursor::new(&buf)),
            Err(ReadError::BadMagic)
        ));
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let mut buf = Vec::new();
        write_file_header(&mut buf).unwrap();
        buf[8] = 99;
        assert!(matches!(
            read_file_header(&mut Cursor::new(&buf)),
            Err(ReadError::UnsupportedVersion(99))
        ));
    }

    #[test]
    fn record_round_trip() {
        let payload = b"\x3e\x00\x0a\x01\x02\x03";
        let ts = 1_718_530_000_000_000_000u64;
        let mut buf = Vec::new();
        write_record(&mut buf, ts, payload).unwrap();
        // 8 (timestamp) + 2 (length) + 6 (payload) = 16 bytes
        assert_eq!(buf.len(), 16);
        let (got_ts, got_payload) = read_record(&mut Cursor::new(&buf)).unwrap().unwrap();
        assert_eq!(got_ts, ts);
        assert_eq!(got_payload.as_slice(), payload.as_ref());
    }

    #[test]
    fn clean_eof_returns_none() {
        assert!(read_record(&mut Cursor::new(b"")).unwrap().is_none());
    }

    #[test]
    fn full_file_round_trip() {
        let records: &[(u64, &[u8])] = &[
            (1_000_000_000, &[0x3e, 0x00, 0x05, 0xaa, 0xbb]),
            (2_000_000_000, &[0x41, 0x00, 0x06, 0x01, 0x02, 0x03]),
            (3_500_000_000, &[0xff]),
        ];

        let mut buf = Vec::new();
        write_file_header(&mut buf).unwrap();
        for (ts, payload) in records {
            write_record(&mut buf, *ts, payload).unwrap();
        }

        let mut r = Cursor::new(&buf);
        read_file_header(&mut r).unwrap();
        let mut decoded: Vec<(u64, Vec<u8>)> = Vec::new();
        while let Some(rec) = read_record(&mut r).unwrap() {
            decoded.push(rec);
        }
        assert_eq!(decoded.len(), records.len());
        for ((ts_got, p_got), (ts_exp, p_exp)) in decoded.iter().zip(records.iter()) {
            assert_eq!(ts_got, ts_exp);
            assert_eq!(p_got.as_slice(), *p_exp);
        }
    }

    // ---- Input layer (.ffplots, ADR 0020) ----

    use firefly_core::{Callsign, DetectionKind, ModeAC, Plot, SensorId, Timestamp};
    use firefly_geo::{Polar, Wgs84};

    fn sample_adsb_plot() -> Plot {
        Plot::adsb(
            SensorId(200),
            Timestamp(12.5),
            Wgs84::from_degrees(48.1, 11.2, 10_000.0),
            75.0,
            ModeAC {
                mode_3a: Some(0o1234),
                flight_level_ft: Some(35_000.0),
                icao_address: Some(0x3C_00_01),
                callsign: Some(Callsign::new("DLH401")),
                spi: false,
            },
        )
    }

    fn sample_primary_plot() -> Plot {
        Plot::primary(SensorId(1), Timestamp(8.0), Polar::new(80_000.0, 1.2, 0.05))
    }

    #[test]
    fn plot_header_round_trip() {
        let mut buf = Vec::new();
        write_plot_file_header(&mut buf).unwrap();
        assert_eq!(buf.len(), HEADER_LEN);
        read_plot_file_header(&mut Cursor::new(&buf)).unwrap();
    }

    #[test]
    fn plot_header_rejects_ffrec_magic() {
        // A .ffrec file must not be mistaken for a .ffplots file and vice versa.
        let mut buf = Vec::new();
        write_file_header(&mut buf).unwrap();
        assert!(matches!(
            read_plot_file_header(&mut Cursor::new(&buf)),
            Err(ReadError::BadMagic)
        ));
    }

    #[test]
    fn plot_record_round_trip_preserves_plot() {
        let plot = sample_adsb_plot();
        let ts = 1_718_530_000_000_000_000u64;
        let mut buf = Vec::new();
        write_plot_record(&mut buf, ts, &plot).unwrap();
        let (got_ts, got_plot) = read_plot_record(&mut Cursor::new(&buf)).unwrap().unwrap();
        assert_eq!(got_ts, ts);
        assert_eq!(got_plot, plot);
    }

    #[test]
    fn plot_full_file_round_trip_mixed_sources() {
        // The input layer is source-agnostic: ADS-B and radar plots coexist.
        let plots: &[(u64, Plot)] = &[
            (1_000_000_000, sample_adsb_plot()),
            (2_000_000_000, sample_primary_plot()),
        ];

        let mut buf = Vec::new();
        write_plot_file_header(&mut buf).unwrap();
        for (ts, plot) in plots {
            write_plot_record(&mut buf, *ts, plot).unwrap();
        }

        let mut r = Cursor::new(&buf);
        read_plot_file_header(&mut r).unwrap();
        let mut decoded = Vec::new();
        while let Some(rec) = read_plot_record(&mut r).unwrap() {
            decoded.push(rec);
        }
        assert_eq!(decoded.len(), plots.len());
        for ((ts_got, plot_got), (ts_exp, plot_exp)) in decoded.iter().zip(plots.iter()) {
            assert_eq!(ts_got, ts_exp);
            assert_eq!(plot_got, plot_exp);
        }
        // Cross-check the detection kinds survived the round-trip.
        assert_eq!(decoded[0].1.kind, DetectionKind::Secondary);
        assert_eq!(decoded[1].1.kind, DetectionKind::Primary);
    }

    #[test]
    fn plot_clean_eof_returns_none() {
        let mut buf = Vec::new();
        write_plot_file_header(&mut buf).unwrap();
        let mut r = Cursor::new(&buf);
        read_plot_file_header(&mut r).unwrap();
        assert!(read_plot_record(&mut r).unwrap().is_none());
    }

    #[test]
    fn plot_malformed_json_is_rejected() {
        // A record whose payload is not valid Plot JSON must be reported, not panic.
        let mut buf = Vec::new();
        write_record(&mut buf, 1, b"{not valid plot json}").unwrap();
        assert!(matches!(
            read_plot_record(&mut Cursor::new(&buf)),
            Err(ReadError::PlotDeserialize(_))
        ));
    }
}
