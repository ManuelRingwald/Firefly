//! Fuzz the legacy CAT001 target-report decoder — a trust boundary on the
//! `radar_asterix` socket (FEP.4): raw, unauthenticated UDP datagrams from a
//! legacy radar head, dispatched on the leading category octet. The
//! two-UAP indirection (plot vs track, selected per record by the TYP bit)
//! makes this decoder's skip logic uniquely stateful — exactly the kind of
//! surface fuzzing is for. Invariant: arbitrary bytes must never panic the
//! decoder; malformed input is an `Err`, not a crash. REQ: FR-IO-011
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = firefly_asterix::decode_legacy_reports(data);
});
