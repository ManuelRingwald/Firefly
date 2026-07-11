//! Fuzz the CAT034 service-message decoder — the second trust boundary on the
//! `radar_asterix` socket (FEP.1): the same raw, unauthenticated UDP datagrams
//! that carry CAT048, dispatched on the leading category octet. Invariant:
//! arbitrary bytes must never panic the decoder; malformed input is an `Err`,
//! not a crash. A panic here would let a single hostile datagram kill the
//! radar listener. REQ: FR-IO-009, FR-NET-014
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = firefly_asterix::decode_service_messages(data);
});
