//! The evaluation core (HA.4): run a scenario through the tracker and
//! score the output against the simulator's exact ground truth.
//!
//! The subtle part is the **truth ↔ track association**: which system
//! track "is" which aircraft, robust across fragmentation and identity
//! swaps. We pair greedily by distance inside a gate, exclusively on both
//! sides per tick — a truth aircraft is represented by at most one track
//! and a track represents at most one aircraft. Identity bookkeeping
//! (switch/fragment counting) then rides on the existing
//! [`TrackContinuity`] semantics.
//!
//! REQ: FR-TRK-051

use std::collections::BTreeSet;

use firefly_geo::LocalFrame;
use firefly_sim::{Scenario, Target, TruthTrajectory};
use firefly_track::{Rmse, SensorErrorModel, TrackContinuity, Tracker, TrackerConfig};

use crate::report::{AggregateReport, EvalReport, TargetReport};

/// Evaluation knobs.
#[derive(Debug, Clone)]
pub struct EvalConfig {
    /// Label carried into the report (CI trend lines hang off it).
    pub label: String,
    /// Evaluation tick, seconds: the picture is scored this often.
    pub tick_s: f64,
    /// Association gate radius (metres): a track farther than this from a
    /// truth position never represents it. Generous relative to sensor
    /// noise, small relative to target separation.
    pub gate_m: f64,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            label: "scenario".to_string(),
            tick_s: 1.0,
            gate_m: 500.0,
        }
    }
}

/// Build the tracker exactly as the live wiring would (`firefly-server`'s
/// multi-sensor setup): every scenario radar registers with its **own
/// site frame** and its true error model — the harness measures the real
/// production configuration, not a bespoke one.
pub fn tracker_for(scenario: &Scenario) -> Tracker {
    let mut config = TrackerConfig::new(LocalFrame::new(scenario.origin()));
    for radar in scenario.radars() {
        let error = SensorErrorModel::from_range_and_azimuth_deg(
            radar.params.sigma_range,
            radar.params.sigma_azimuth.to_degrees(),
        );
        config = config.with_sensor(
            radar.sensor.id,
            LocalFrame::new(radar.sensor.position()),
            error,
            radar.params.scan_period,
        );
    }
    Tracker::new(config)
}

/// Evaluate `scenario` against **all** of its targets.
pub fn evaluate(scenario: &Scenario, cfg: &EvalConfig) -> EvalReport {
    evaluate_against(scenario, cfg, scenario.targets())
}

/// Evaluate `scenario` against an explicit truth set. Deliberately public:
/// scoring against a **subset** of the simulated targets lets a test prove
/// that the false-track metric bites (the withheld aircraft's track must
/// show up as "false").
pub fn evaluate_against(
    scenario: &Scenario,
    cfg: &EvalConfig,
    truth_targets: &[Target],
) -> EvalReport {
    let mut tracker = tracker_for(scenario);
    let plots = firefly_sim::run(scenario);

    struct TruthState {
        target: u32,
        trajectory: TruthTrajectory,
        rmse: Rmse,
        continuity: TrackContinuity,
        truth_ticks: usize,
        covered_ticks: usize,
        ids: BTreeSet<u32>,
        first_covered_s: Option<f64>,
    }
    let mut truths: Vec<TruthState> = truth_targets
        .iter()
        .map(|t| TruthState {
            target: t.id.0,
            trajectory: TruthTrajectory::build(t, scenario.truth_step()),
            rmse: Rmse::new(),
            continuity: TrackContinuity::new(),
            truth_ticks: 0,
            covered_ticks: 0,
            ids: BTreeSet::new(),
            first_covered_s: None,
        })
        .collect();

    let mut aggregate_rmse = Rmse::new();
    let mut confirmed_ever: BTreeSet<u32> = BTreeSet::new();
    let mut assigned_ever: BTreeSet<u32> = BTreeSet::new();

    let frame = LocalFrame::new(scenario.origin());
    let mut next_plot = 0usize;
    let mut ticks = 0usize;
    let mut t = cfg.tick_s;
    while t <= scenario.duration() + 1e-9 {
        // Feed everything the sensors produced up to this instant, in one
        // batch — `process_plots` applies the same simultaneity handling
        // the live path uses (ADR 0013).
        let start = next_plot;
        while next_plot < plots.len() && plots[next_plot].time.as_secs() <= t {
            next_plot += 1;
        }
        if next_plot > start {
            tracker.process_plots(&plots[start..next_plot]);
        }
        ticks += 1;

        // The confirmed picture at this tick, PROJECTED to tick time — the
        // same forward prediction the operational output path applies
        // (`snapshot_at`, ADR 0013). Scoring the last-updated filter state
        // instead would charge the tracker up to a full scan of motion as
        // fake position error; ESASSP measures the *output* picture.
        let tracks: Vec<(u32, f64, f64)> = tracker
            .snapshot_at(firefly_core::Timestamp(t))
            .into_iter()
            .filter(|st| st.confirmed && !st.ended)
            .map(|st| {
                let enu = frame.geodetic_to_enu(&st.position);
                (st.id.0, enu.east, enu.north)
            })
            .collect();
        for (id, _, _) in &tracks {
            confirmed_ever.insert(*id);
        }

        // Candidate pairs inside the gate, then greedy nearest-first with
        // exclusivity on both sides (deterministic: distance, then ids).
        let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
        let mut truth_pos: Vec<Option<(f64, f64)>> = Vec::with_capacity(truths.len());
        for (ti, truth) in truths.iter().enumerate() {
            let pos = truth.trajectory.position_at(t).map(|p| (p.east, p.north));
            if let Some((te, tn)) = pos {
                for (ki, (_, e, n)) in tracks.iter().enumerate() {
                    let d = ((e - te).powi(2) + (n - tn).powi(2)).sqrt();
                    if d <= cfg.gate_m {
                        pairs.push((d, ti, ki));
                    }
                }
            }
            truth_pos.push(pos);
        }
        pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().then(a.1.cmp(&b.1)));
        let mut truth_taken = vec![false; truths.len()];
        let mut track_taken = vec![false; tracks.len()];
        let mut assignment: Vec<Option<(usize, f64)>> = vec![None; truths.len()];
        for (d, ti, ki) in pairs {
            if !truth_taken[ti] && !track_taken[ki] {
                truth_taken[ti] = true;
                track_taken[ki] = true;
                assignment[ti] = Some((ki, d));
            }
        }

        for (ti, truth) in truths.iter_mut().enumerate() {
            if truth_pos[ti].is_none() {
                continue; // the aircraft's script has ended — nothing to score
            }
            truth.truth_ticks += 1;
            match assignment[ti] {
                Some((ki, d)) => {
                    let id = tracks[ki].0;
                    truth.covered_ticks += 1;
                    truth.rmse.add(d);
                    aggregate_rmse.add(d);
                    truth.ids.insert(id);
                    truth.continuity.observe(Some(firefly_core::TrackId(id)));
                    truth.first_covered_s.get_or_insert(t);
                    assigned_ever.insert(id);
                }
                None => truth.continuity.observe(None),
            }
        }

        t += cfg.tick_s;
    }

    let targets: Vec<TargetReport> = truths
        .iter()
        .map(|s| TargetReport {
            target: u64::from(s.target),
            truth_ticks: s.truth_ticks,
            covered_ticks: s.covered_ticks,
            track_pd: if s.truth_ticks == 0 {
                0.0
            } else {
                s.covered_ticks as f64 / s.truth_ticks as f64
            },
            position_rmse_m: s.rmse.value(),
            track_ids: s.ids.len(),
            id_switches: s.continuity.id_switches(),
            confirmation_latency_s: s.first_covered_s,
        })
        .collect();

    let truth_ticks_total: usize = targets.iter().map(|r| r.truth_ticks).sum();
    let covered_total: usize = targets.iter().map(|r| r.covered_ticks).sum();
    let aggregate = AggregateReport {
        track_pd: if truth_ticks_total == 0 {
            0.0
        } else {
            covered_total as f64 / truth_ticks_total as f64
        },
        position_rmse_m: aggregate_rmse.value(),
        id_switches: targets.iter().map(|r| r.id_switches).sum(),
        false_tracks: confirmed_ever.difference(&assigned_ever).count(),
        confirmed_tracks_total: confirmed_ever.len(),
    };

    EvalReport {
        scenario: cfg.label.clone(),
        tick_s: cfg.tick_s,
        gate_m: cfg.gate_m,
        ticks,
        targets,
        aggregate,
    }
}
