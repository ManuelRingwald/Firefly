//! CAT065 framing and data items — **SDPS Service Status Messages**, the feed
//! *heartbeat*.
//!
//! Where [`cat062`](crate::cat062) carries the air picture (one record per
//! track), CAT065 carries the **liveness of the surveillance data processing
//! system (SDPS)** itself: a small, periodic "I am alive and operational"
//! message. It lets a consumer (the ASD/Wayfinder) tell the difference between
//! *"the sky is empty"* (a valid, track-less CAT062 block) and *"the feed is
//! dead"* (nothing arriving at all) — the basis for staleness detection and a
//! meaningful readiness signal (ADR 0018).
//!
//! A [`Cat065Encoder`] turns one status report into a single CAT065 *data
//! block*: a `[CAT=65][LEN][record]` envelope around one SDPS-Status record.
//! The record carries the data source (I065/010), the message type
//! (I065/000 = SDPS Status), the service identification (I065/015), the time of
//! day (I065/030) and the SDPS configuration/status octet (I065/040, whose NOGO
//! field reports operational vs. degraded).
//!
//! The heartbeat travels on the **same** multicast group/port as CAT062
//! (ADR 0018): the stream is self-describing, so a consumer dispatches on the
//! leading CAT octet (0x3E → tracks, 0x41 → status). This mirrors how a real
//! SDPS output (ARTAS, Phoenix) multiplexes categories on one feed.
//!
//! Items and bit layouts are taken from **EUROCONTROL
//! SUR.ET1.ST05.2000-STD-13-01** ("CAT065 SDPS Service Status Messages").
//!
//! REQ: FR-IO-006

use firefly_core::Timestamp;

use crate::cat062::DataSourceId;
use crate::fspec::{self, RecordBuilder};

/// The ASTERIX category number for SDPS service status messages.
const CATEGORY: u8 = 65;

/// I065/030 is counted in units of 1/128 second since midnight (as I062/070).
const TIME_LSB_SECONDS: f64 = 1.0 / 128.0;
/// Time of day wraps every 24 hours; fold the value into one day so the 24-bit
/// field never overflows.
const SECONDS_PER_DAY: f64 = 86_400.0;

/// The CAT065 UAP slots (field reference numbers) we encode. The FRNs follow
/// the standard EUROCONTROL CAT065 UAP, so a conforming third-party decoder
/// reads our heartbeat without a private profile. We emit the subset that makes
/// up a periodic **SDPS Status** message; the other standard FRNs are:
///
/// - FRN 5 — I065/020 Batch Number (only for an End-of-Batch message).
/// - FRN 7 — I065/050 Service Status Report (only for a Service Status Report).
mod uap {
    /// I065/010 — Data Source Identifier (SAC/SIC).
    pub const DATA_SOURCE_IDENTIFIER: u8 = 1;
    /// I065/000 — Message Type.
    pub const MESSAGE_TYPE: u8 = 2;
    /// I065/015 — Service Identification.
    pub const SERVICE_IDENTIFICATION: u8 = 3;
    /// I065/030 — Time of Day.
    pub const TIME_OF_DAY: u8 = 4;
    /// I065/020 — Batch Number (decoded but not emitted by the heartbeat).
    pub const BATCH_NUMBER: u8 = 5;
    /// I065/040 — SDPS Configuration and Status.
    pub const SDPS_CONFIGURATION_STATUS: u8 = 6;
    /// I065/050 — Service Status Report (decoded but not emitted).
    pub const SERVICE_STATUS_REPORT: u8 = 7;
}

/// I065/000 message type: a periodic SDPS status report — the heartbeat.
pub const MESSAGE_TYPE_SDPS_STATUS: u8 = 1;

/// I065/040 octet, bits 8/7 (NOGO): the operational release status. `00` means
/// operational; any other value means not fully operational. We report
/// operational as `0x00` and degraded as `0x40` (NOGO = `01`). Verified against
/// SUR.ET1.ST05.2000-STD-13-01.
const SDPS_STATUS_NOGO_MASK: u8 = 0xC0;
const SDPS_STATUS_OPERATIONAL: u8 = 0x00;
const SDPS_STATUS_DEGRADED: u8 = 0x40;

/// Encodes SDPS service status reports into CAT065 data blocks.
///
/// Holds the fixed [`DataSourceId`] (who is reporting) and the service
/// identification (which service); the time of day and operational flag are
/// supplied per call, since the heartbeat is a real-time liveness signal.
#[derive(Debug, Clone, Copy)]
pub struct Cat065Encoder {
    source: DataSourceId,
    service_id: u8,
}

impl Cat065Encoder {
    /// An encoder that stamps every status block with `source` as the data
    /// source and `service_id` as I065/015.
    pub fn new(source: DataSourceId, service_id: u8) -> Self {
        Self { source, service_id }
    }

    /// Encode one periodic **SDPS Status** message (the heartbeat), stamped
    /// with `time_of_day_secs` (seconds since UTC midnight) and an
    /// `operational` flag for the NOGO field of I065/040.
    pub fn encode_status(&self, time_of_day_secs: f64, operational: bool) -> Vec<u8> {
        let record = RecordBuilder::new()
            .item(
                uap::DATA_SOURCE_IDENTIFIER,
                vec![self.source.sac, self.source.sic],
            )
            .item(uap::MESSAGE_TYPE, vec![MESSAGE_TYPE_SDPS_STATUS])
            .item(uap::SERVICE_IDENTIFICATION, vec![self.service_id])
            .item(uap::TIME_OF_DAY, encode_time_of_day(time_of_day_secs))
            .item(
                uap::SDPS_CONFIGURATION_STATUS,
                vec![encode_sdps_status(operational)],
            )
            .finish();
        data_block(&record)
    }
}

/// I065/030 — time of day as a 24-bit count of 1/128-second ticks since
/// midnight, big-endian (same encoding as I062/070).
fn encode_time_of_day(secs: f64) -> Vec<u8> {
    let tod = secs.rem_euclid(SECONDS_PER_DAY);
    let ticks = (tod / TIME_LSB_SECONDS).round() as u32; // ≤ 11_059_200 < 2^24
    ticks.to_be_bytes()[1..4].to_vec()
}

/// I065/040 — SDPS Configuration and Status: NOGO field operational/degraded.
fn encode_sdps_status(operational: bool) -> u8 {
    if operational {
        SDPS_STATUS_OPERATIONAL
    } else {
        SDPS_STATUS_DEGRADED
    }
}

/// Wrap one encoded record in the CAT065 envelope: `[CAT][LEN][record]`, where
/// `LEN` is the total block length (header included), big-endian.
fn data_block(record: &[u8]) -> Vec<u8> {
    let total = 3 + record.len();
    let mut out = Vec::with_capacity(total);
    out.push(CATEGORY);
    out.extend_from_slice(&(total as u16).to_be_bytes());
    out.extend_from_slice(record);
    out
}

/// One decoded CAT065 SDPS status report — the inverse of
/// [`Cat065Encoder::encode_status`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DecodedStatus {
    /// I065/010 — the data source that produced this status.
    pub source: DataSourceId,
    /// I065/000 — message type ([`MESSAGE_TYPE_SDPS_STATUS`] for a heartbeat).
    pub message_type: u8,
    /// I065/015 — service identification.
    pub service_id: u8,
    /// I065/030 — time of day (wraps every 24 hours).
    pub time: Timestamp,
    /// I065/040 NOGO — `true` when the SDPS reports itself operational.
    pub operational: bool,
}

/// Errors that can occur while decoding a CAT065 data block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cat065DecodeError {
    /// The input ended before a complete block/record/item could be read.
    Truncated,
    /// The first octet was not [`CATEGORY`] (65).
    WrongCategory(u8),
    /// The `LEN` field did not match the actual input length.
    LengthMismatch { declared: usize, actual: usize },
    /// The FSPEC marked an FRN present that this decoder doesn't know.
    UnknownItem(u8),
    /// A record's FSPEC was missing an item a status report requires.
    MissingItem(u8),
}

impl std::fmt::Display for Cat065DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cat065DecodeError::Truncated => write!(f, "input ended before a complete item"),
            Cat065DecodeError::WrongCategory(cat) => write!(f, "expected CAT 65, got CAT {cat}"),
            Cat065DecodeError::LengthMismatch { declared, actual } => write!(
                f,
                "LEN field says {declared} bytes, but input is {actual} bytes"
            ),
            Cat065DecodeError::UnknownItem(frn) => {
                write!(f, "FSPEC marks unknown FRN {frn} present")
            }
            Cat065DecodeError::MissingItem(frn) => {
                write!(f, "record is missing required FRN {frn}")
            }
        }
    }
}

impl std::error::Error for Cat065DecodeError {}

/// Decode a CAT065 data block into its status reports — the inverse of
/// [`Cat065Encoder::encode_status`]. Robust against truncated input: it never
/// panics, returning a [`Cat065DecodeError`] instead.
pub fn decode_status_block(bytes: &[u8]) -> Result<Vec<DecodedStatus>, Cat065DecodeError> {
    if bytes.len() < 3 {
        return Err(Cat065DecodeError::Truncated);
    }
    if bytes[0] != CATEGORY {
        return Err(Cat065DecodeError::WrongCategory(bytes[0]));
    }
    let declared = u16::from_be_bytes([bytes[1], bytes[2]]) as usize;
    if declared < 3 || declared > bytes.len() {
        return Err(Cat065DecodeError::LengthMismatch {
            declared,
            actual: bytes.len(),
        });
    }

    let mut pos = 3;
    let mut reports = Vec::new();
    while pos < declared {
        let (status, next) = decode_record(bytes, pos, declared)?;
        reports.push(status);
        pos = next;
    }
    Ok(reports)
}

/// Decode one status record starting at `pos`, returning it and the offset just
/// past it. `end` is the block's declared length (the record may not read past
/// it).
fn decode_record(
    bytes: &[u8],
    pos: usize,
    end: usize,
) -> Result<(DecodedStatus, usize), Cat065DecodeError> {
    let (frns, consumed) = fspec::parse(&bytes[pos..end]);
    if consumed == 0 {
        return Err(Cat065DecodeError::Truncated);
    }
    let mut cur = pos + consumed;

    // Take the next `n` bytes within the record, or report truncation.
    let mut take = |n: usize| -> Result<&[u8], Cat065DecodeError> {
        if end - cur < n {
            return Err(Cat065DecodeError::Truncated);
        }
        let slice = &bytes[cur..cur + n];
        cur += n;
        Ok(slice)
    };

    let mut source = None;
    let mut message_type = None;
    let mut service_id = None;
    let mut time = None;
    let mut operational = None;

    for frn in frns {
        match frn {
            uap::DATA_SOURCE_IDENTIFIER => {
                let b = take(2)?;
                source = Some(DataSourceId::new(b[0], b[1]));
            }
            uap::MESSAGE_TYPE => message_type = Some(take(1)?[0]),
            uap::SERVICE_IDENTIFICATION => service_id = Some(take(1)?[0]),
            uap::TIME_OF_DAY => {
                let b = take(3)?;
                let ticks = u32::from_be_bytes([0, b[0], b[1], b[2]]);
                time = Some(Timestamp(ticks as f64 * TIME_LSB_SECONDS));
            }
            uap::BATCH_NUMBER => {
                let _ = take(1)?; // present in End-of-Batch messages; ignored here
            }
            uap::SDPS_CONFIGURATION_STATUS => {
                operational = Some(take(1)?[0] & SDPS_STATUS_NOGO_MASK == SDPS_STATUS_OPERATIONAL);
            }
            uap::SERVICE_STATUS_REPORT => {
                let _ = take(1)?; // only in Service Status Report messages; ignored
            }
            other => return Err(Cat065DecodeError::UnknownItem(other)),
        }
    }

    let status = DecodedStatus {
        source: source.ok_or(Cat065DecodeError::MissingItem(uap::DATA_SOURCE_IDENTIFIER))?,
        message_type: message_type.ok_or(Cat065DecodeError::MissingItem(uap::MESSAGE_TYPE))?,
        service_id: service_id
            .ok_or(Cat065DecodeError::MissingItem(uap::SERVICE_IDENTIFICATION))?,
        time: time.ok_or(Cat065DecodeError::MissingItem(uap::TIME_OF_DAY))?,
        operational: operational.ok_or(Cat065DecodeError::MissingItem(
            uap::SDPS_CONFIGURATION_STATUS,
        ))?,
    };
    Ok((status, cur))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoder() -> Cat065Encoder {
        Cat065Encoder::new(DataSourceId::new(0x19, 0x02), 1)
    }

    /// A heartbeat at midnight encodes to the exact reference bytes. This is the
    /// byte-level ground truth Wayfinder's decoder is verified against.
    #[test]
    fn status_matches_reference_dump() {
        let block = encoder().encode_status(0.0, true);
        // [CAT=65][LEN=12][FSPEC=0xF4][SAC,SIC][type=1][service=1][tod=0,0,0][status=0]
        assert_eq!(
            block,
            vec![0x41, 0x00, 0x0C, 0xF4, 0x19, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00]
        );
    }

    /// The time of day lands in I065/030 as 1/128-s ticks, big-endian.
    #[test]
    fn time_of_day_is_encoded_in_128th_seconds() {
        // 01:00:00 = 3600 s → 3600 * 128 = 460_800 = 0x07_0800.
        let block = encoder().encode_status(3600.0, true);
        assert_eq!(&block[8..11], &[0x07, 0x08, 0x00]);
    }

    /// A degraded heartbeat sets the NOGO field of I065/040.
    #[test]
    fn degraded_status_sets_nogo() {
        let block = encoder().encode_status(0.0, false);
        assert_eq!(*block.last().unwrap(), SDPS_STATUS_DEGRADED);
    }

    /// Encoding then decoding recovers every field (round-trip).
    #[test]
    fn round_trip_recovers_status() {
        for (secs, operational) in [(0.0, true), (3600.0, true), (86_399.0, false)] {
            let block = encoder().encode_status(secs, operational);
            let reports = decode_status_block(&block).expect("decodes");
            assert_eq!(reports.len(), 1);
            let r = reports[0];
            assert_eq!(r.source, DataSourceId::new(0x19, 0x02));
            assert_eq!(r.message_type, MESSAGE_TYPE_SDPS_STATUS);
            assert_eq!(r.service_id, 1);
            assert_eq!(r.operational, operational);
            assert!((r.time.as_secs() - secs).abs() < TIME_LSB_SECONDS);
        }
    }

    /// A wrong leading category is rejected, not misread.
    #[test]
    fn wrong_category_is_rejected() {
        let mut block = encoder().encode_status(0.0, true);
        block[0] = 0x3E; // CAT062
        assert_eq!(
            decode_status_block(&block),
            Err(Cat065DecodeError::WrongCategory(0x3E))
        );
    }

    /// Truncated input never panics — it returns an error.
    #[test]
    fn truncated_input_is_rejected() {
        let block = encoder().encode_status(0.0, true);
        for cut in 0..block.len() {
            // Must not panic; either Truncated or LengthMismatch is acceptable.
            let _ = decode_status_block(&block[..cut]);
        }
    }
}
