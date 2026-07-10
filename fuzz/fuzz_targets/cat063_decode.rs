//! Fuzz the CAT063 sensor-status decoder (consumer side, incl. the RE-field
//! walk of ICD 3.1.0). Invariant: arbitrary bytes must never panic; malformed
//! input is an `Err`. REQ: FR-IO-007
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = firefly_asterix::decode_sensor_block(data);
});
