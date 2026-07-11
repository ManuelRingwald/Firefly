//! Fuzz the legacy CAT002 service-message decoder — the companion trust
//! boundary to CAT001 on the `radar_asterix` socket (FEP.4). Its I002/030
//! time of day is the anchor for expanding CAT001's truncated timestamps, so
//! a hostile datagram reaching this decoder is a certainty on a legacy feed.
//! Invariant: arbitrary bytes must never panic the decoder; malformed input
//! is an `Err`, not a crash. REQ: FR-IO-011
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = firefly_asterix::decode_legacy_service_messages(data);
});
