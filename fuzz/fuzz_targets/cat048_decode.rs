//! Fuzz the CAT048 radar-input decoder — the primary trust boundary of the
//! `radar_asterix` source (ADR 0017/0028): raw, unauthenticated UDP datagrams.
//! Invariant: arbitrary bytes must never panic the decoder; malformed input is
//! an `Err`, not a crash. A panic here would be a single-datagram denial of
//! service on the air picture. REQ: FR-IO-005, FR-NET-013
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = firefly_asterix::decode_target_reports(data);
});
