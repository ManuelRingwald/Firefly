//! Fuzz the CAT019 MLAT system-status decoder — the companion trust boundary
//! to CAT020 on the `mlat_asterix` socket (FEP.5). Its status messages feed
//! the CAT063 liveness path, so a hostile datagram reaching this decoder is
//! a certainty on a WAM feed. Invariant: arbitrary bytes must never panic
//! the decoder; malformed input is an `Err`, not a crash. REQ: FR-IO-012
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = firefly_asterix::decode_mlat_status(data);
});
