//! Fuzz the CAT020 multilateration target-report decoder — a trust boundary
//! on the `mlat_asterix` socket (FEP.5): raw, unauthenticated UDP datagrams
//! from a WAM system, dispatched on the leading category octet. The
//! I020/500 accuracy compound and the two repetitive items make its skip
//! logic stateful — exactly the kind of surface fuzzing is for. Invariant:
//! arbitrary bytes must never panic the decoder; malformed input is an
//! `Err`, not a crash. REQ: FR-IO-012
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = firefly_asterix::decode_mlat_reports(data);
});
