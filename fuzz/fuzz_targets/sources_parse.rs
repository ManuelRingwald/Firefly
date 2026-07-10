//! Fuzz the `FIREFLY_SOURCES` contract parser — the orchestrator-facing
//! configuration boundary (ADR 0023). Invariant: any string must yield
//! `Ok`/`Err`, never a panic; a hostile or corrupted source list must not be
//! able to crash the server at startup. Only the pure parse step is fuzzed
//! (`parse_sources`); credential resolution reads the environment and stays
//! out of the fuzz loop. REQ: FR-NET-011
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    let _ = firefly_server::sources::parse_sources(data);
});
