//! Fuzz the CAT062 consumer-side decoder (used by the recorder/replay path and
//! as the ground truth Wayfinder's decoder is verified against). Invariant:
//! arbitrary bytes must never panic; malformed input is an `Err`. REQ: FR-IO-003
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = firefly_asterix::decode_data_block(data);
});
