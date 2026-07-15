//! The evaluation report (HA.4): one row per truth aircraft plus the
//! aggregate — serialisable for CI trends, displayable for humans.
//!
//! REQ: FR-TRK-051

use serde::Serialize;

/// Quality scores for one truth aircraft.
#[derive(Debug, Clone, Serialize)]
pub struct TargetReport {
    /// The simulator's target id.
    pub target: u64,
    /// Evaluation ticks while this target's truth existed.
    pub truth_ticks: usize,
    /// Ticks covered by a confirmed track inside the association gate.
    pub covered_ticks: usize,
    /// **Track probability of detection** (ESASSP intent: fraction of the
    /// aircraft's existence covered by a system track): covered / truth.
    pub track_pd: f64,
    /// **Horizontal position RMS error** (metres) over the covered ticks —
    /// the ESASSP horizontal-accuracy metric. `None` when never covered.
    pub position_rmse_m: Option<f64>,
    /// Distinct confirmed track identities that represented this aircraft —
    /// 1 is perfect; more means fragmentation (track died and was reborn)
    /// or an identity swap.
    pub track_ids: usize,
    /// Identity changes mid-flight (ESASSP continuity intent: a system
    /// track should accompany the aircraft as ONE track).
    pub id_switches: usize,
    /// Seconds from the target's first truth instant until a confirmed
    /// track first covered it — the initiation latency. `None` = never.
    pub confirmation_latency_s: Option<f64>,
}

/// Scores over the whole scenario.
#[derive(Debug, Clone, Serialize)]
pub struct AggregateReport {
    /// Covered ticks / truth ticks over all aircraft.
    pub track_pd: f64,
    /// RMS over every covered tick of every aircraft, metres.
    pub position_rmse_m: Option<f64>,
    /// Sum of per-aircraft identity switches.
    pub id_switches: usize,
    /// Confirmed tracks that never represented any truth aircraft — false
    /// (ghost) tracks. With clutter-free simulation this must be 0; every
    /// value above is tracker-made (coalescence, reflections, splits).
    pub false_tracks: usize,
    /// All confirmed track identities seen during the run.
    pub confirmed_tracks_total: usize,
}

/// The full evaluation result.
#[derive(Debug, Clone, Serialize)]
pub struct EvalReport {
    /// Scenario label (for humans and CI trend lines).
    pub scenario: String,
    /// Evaluation tick, seconds.
    pub tick_s: f64,
    /// Association gate radius, metres (truth ↔ track pairing).
    pub gate_m: f64,
    /// Evaluation ticks executed.
    pub ticks: usize,
    pub targets: Vec<TargetReport>,
    pub aggregate: AggregateReport,
}

impl EvalReport {
    /// The machine-readable form (stable field names — CI trend lines hang
    /// off them).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("report serialises")
    }
}

impl std::fmt::Display for EvalReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Evaluation '{}' — {} ticks à {} s, gate {} m",
            self.scenario, self.ticks, self.tick_s, self.gate_m
        )?;
        writeln!(
            f,
            "{:>8} {:>10} {:>10} {:>12} {:>9} {:>9} {:>12}",
            "target", "PD", "RMSE [m]", "track ids", "switches", "covered", "latency [s]"
        )?;
        for t in &self.targets {
            writeln!(
                f,
                "{:>8} {:>10.3} {:>10} {:>12} {:>9} {:>9} {:>12}",
                t.target,
                t.track_pd,
                t.position_rmse_m
                    .map_or("-".to_string(), |v| format!("{v:.1}")),
                t.track_ids,
                t.id_switches,
                format!("{}/{}", t.covered_ticks, t.truth_ticks),
                t.confirmation_latency_s
                    .map_or("-".to_string(), |v| format!("{v:.1}")),
            )?;
        }
        writeln!(
            f,
            "aggregate: PD {:.3} · RMSE {} m · {} id switch(es) · {} false track(s) of {} confirmed",
            self.aggregate.track_pd,
            self.aggregate
                .position_rmse_m
                .map_or("-".to_string(), |v| format!("{v:.1}")),
            self.aggregate.id_switches,
            self.aggregate.false_tracks,
            self.aggregate.confirmed_tracks_total,
        )
    }
}
