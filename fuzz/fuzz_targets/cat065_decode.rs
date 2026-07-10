//! Fuzz the CAT065 SDPS-heartbeat decoder (consumer side). Invariant:
//! arbitrary bytes must never panic; malformed input is an `Err`.
//! REQ: FR-IO-006
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = firefly_asterix::decode_status_block(data);
});
