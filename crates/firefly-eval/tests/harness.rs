//! Regression gates + instrument tests for the evaluation harness (HA.4).
//!
//! Two kinds of test live here, deliberately separated:
//!
//! - **Regression gates** pin the tracker's measured quality on the
//!   built-in benchmarks (thresholds calibrated on the honest as-is state,
//!   not polished). A future change that degrades PD/RMSE/continuity
//!   fails CI with a number attached.
//! - **Instrument tests** prove the measuring device itself bites: a
//!   metric that cannot get worse under degraded input measures nothing
//!   (the SPEC.1 non-biting-test lesson, applied to the harness).
//!
//! REQ: FR-TRK-051

use firefly_eval::{evaluate, evaluate_against, scenarios, EvalConfig};

fn cfg(label: &str) -> EvalConfig {
    EvalConfig {
        label: label.to_string(),
        ..EvalConfig::default()
    }
}

/// Regression gate: the single-target benchmark. Thresholds calibrated on
/// the 2026-07-15 state (PD 0.967, RMSE 45.6 m, latency 9 s) with head
/// room for noise-seed sensitivity, NOT for regressions. REQ: FR-TRK-051
#[test]
fn single_target_benchmark_holds() {
    let report = evaluate(&scenarios::single_target(), &cfg("single"));
    let t = &report.targets[0];
    assert!(t.track_pd >= 0.95, "track PD {} too low", t.track_pd);
    let rmse = t.position_rmse_m.expect("covered");
    assert!(rmse < 60.0, "position RMSE {rmse:.1} m too high");
    assert_eq!(t.track_ids, 1, "one aircraft = one track identity");
    assert_eq!(t.id_switches, 0);
    let latency = t.confirmation_latency_s.expect("confirmed");
    assert!(latency <= 15.0, "confirmation latency {latency} s too high");
    assert_eq!(report.aggregate.false_tracks, 0);
}

/// Regression gate: the parallel-pair benchmark — clean multi-target
/// bookkeeping (each aircraft exactly one identity, no ghosts).
/// REQ: FR-TRK-051
#[test]
fn parallel_pair_benchmark_holds() {
    let report = evaluate(&scenarios::parallel_pair(), &cfg("pair"));
    assert_eq!(report.targets.len(), 2);
    for t in &report.targets {
        assert!(t.track_pd >= 0.95, "track PD {} too low", t.track_pd);
        assert_eq!(t.track_ids, 1);
        assert_eq!(t.id_switches, 0);
    }
    assert_eq!(report.aggregate.confirmed_tracks_total, 2);
    assert_eq!(report.aggregate.false_tracks, 0);
}

/// Instrument test: the PD metric bites — a sensor that misses half its
/// scans must score a measurably lower PD than a perfect one, and the
/// report must not paper over it. REQ: FR-TRK-051
#[test]
fn pd_metric_drops_under_degraded_detection() {
    let perfect = evaluate(&scenarios::single_target(), &cfg("pd=1.0"));
    let degraded = evaluate(
        &scenarios::single_target_with_detection(0.5),
        &cfg("pd=0.5"),
    );
    assert!(
        degraded.aggregate.track_pd < perfect.aggregate.track_pd - 0.02,
        "degraded detection must show up: perfect {} vs degraded {}",
        perfect.aggregate.track_pd,
        degraded.aggregate.track_pd
    );
}

/// Instrument test: the false-track metric bites — withhold one of two
/// real aircraft from the truth set, and its (correct) track must be
/// reported as a false track. REQ: FR-TRK-051
#[test]
fn false_track_metric_counts_unmatched_tracks() {
    let scenario = scenarios::parallel_pair();
    let truths = &scenario.targets()[..1];
    let report = evaluate_against(&scenario, &cfg("withheld-truth"), truths);
    assert_eq!(report.targets.len(), 1);
    assert_eq!(
        report.aggregate.false_tracks, 1,
        "the withheld aircraft's track must be counted as false"
    );
    assert_eq!(report.aggregate.confirmed_tracks_total, 2);
}

/// The CAP.1 load-scenario generator produces a trackable picture: on a
/// small grid every aircraft is confirmed as exactly one track and no
/// ghosts appear — the property that makes the load benchmarks
/// meaningful (a generator the tracker cannot follow would benchmark
/// garbage). REQ: NFR-CAP-001
#[test]
fn load_grid_scenario_tracks_all_aircraft() {
    let scenario = scenarios::load_grid(1, 10, 120.0);
    let report = evaluate(&scenario, &cfg("load-1x10"));
    assert_eq!(report.targets.len(), 10);
    assert!(
        report.aggregate.track_pd >= 0.9,
        "aggregate PD {} too low",
        report.aggregate.track_pd
    );
    assert_eq!(report.aggregate.confirmed_tracks_total, 10);
    assert_eq!(report.aggregate.false_tracks, 0);
    assert_eq!(report.aggregate.id_switches, 0);
}

/// Determinism (NFR-CLOUD-001): the same scenario produces a byte-identical
/// JSON report — the property that makes CI trend lines meaningful.
/// REQ: FR-TRK-051
#[test]
fn reports_are_deterministic() {
    let a = evaluate(&scenarios::parallel_pair(), &cfg("pair")).to_json();
    let b = evaluate(&scenarios::parallel_pair(), &cfg("pair")).to_json();
    assert_eq!(a, b);
}
