//! Fuzz the CAT021 ADS-B ground-station decoder — the trust boundary of the
//! `adsb_asterix` source (FEP.3): raw, unauthenticated UDP datagrams.
//! Invariant: arbitrary bytes must never panic the decoder; malformed input
//! is an `Err`, not a crash. A panic here would let a single hostile
//! datagram kill the ADS-B listener. REQ: FR-IO-010, FR-NET-015
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = firefly_asterix::decode_adsb_reports(data);
});
