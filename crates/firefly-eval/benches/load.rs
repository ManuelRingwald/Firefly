//! Load benchmarks (CAP.1): how fast does the full tracker chew through
//! synthetic `N radars × M targets` traffic?
//!
//! Measured is the **production hot path** — `Tracker::process_plots`
//! over the complete plot stream of a scenario, with the tracker built
//! exactly as the live wiring builds it (`tracker_for`). Criterion
//! reports time per scenario run; the configured throughput turns that
//! into **plots/second**, the number the CAP.2 design limits will be
//! stated in.
//!
//! Run with `cargo bench -p firefly-eval`; results land in
//! `target/criterion/`. Scenario generation happens OUTSIDE the measured
//! closure — only tracking is timed.
//!
//! REQ: NFR-CAP-001

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use firefly_eval::{scenarios, tracker_for};

fn tracker_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("tracker_load");
    // Full-scenario runs are heavy; fewer samples keep `cargo bench`
    // under a coffee break while the relative error stays useful.
    group.sample_size(10);

    for (radars, targets) in [(1usize, 10usize), (1, 50), (2, 50), (3, 100)] {
        let scenario = scenarios::load_grid(radars, targets, 60.0);
        let plots = firefly_sim::run(&scenario);
        group.throughput(Throughput::Elements(plots.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{radars}r x {targets}t")),
            &(&scenario, &plots),
            |b, (scenario, plots)| {
                b.iter(|| {
                    let mut tracker = tracker_for(scenario);
                    tracker.process_plots(plots);
                    tracker
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, tracker_load);
criterion_main!(benches);
