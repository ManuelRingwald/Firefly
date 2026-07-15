//! The auto-correlation engine (FPL.1) — the ADR 0038 rules as code.
//!
//! Safety shape: a **wrong** label is worse than a missing one (the Weeze
//! lesson: one plan on two tracks). Every rule below errs on the side of
//! *not* correlating; the manual command path (FPL.2) is the honest escape
//! hatch for everything the automatics decline.
//!
//! REQ: FR-TRK-047

use std::collections::BTreeMap;

use firefly_core::FlightPlanRef;

use crate::plan::FlightPlan;

/// The Mode-S conspicuity code (octal 1000): ambiguous **by design** in
/// Mode-S airspace and therefore never a correlation key (ADR 0038).
const CONSPICUITY_CODE: u16 = 0o1000;

/// Half-width of the temporal plausibility window (seconds) around a
/// plan's `expected_time` (ADR 0038 rule 3): generous enough for real
/// delays, tight enough that yesterday's plan does not label today's
/// traffic. Spatial volumes need route geometry the minimal plan set does
/// not carry yet — an honest limit, not a hidden one.
const TIME_WINDOW_S: f64 = 45.0 * 60.0;

/// Why the service did or did not correlate — the observability surface
/// (metrics/log; ADR 0038 rule 4).
#[derive(Debug, Clone, PartialEq)]
pub enum CorrelationOutcome {
    /// Correlated via the callsign — the primary key.
    ByCallsign(FlightPlanRef),
    /// Correlated via a unique, plausible squawk — the fallback key.
    BySquawk(FlightPlanRef),
    /// A squawk matched but was refused: duplicated among plans, the
    /// conspicuity code, or the track carries an identity conflict.
    SquawkRefused,
    /// Nothing matched.
    None,
}

/// The correlation service: indexed plans plus the ADR 0038 rules.
#[derive(Debug, Clone, Default)]
pub struct CorrelationService {
    plans: Vec<FlightPlan>,
    by_callsign: BTreeMap<String, usize>,
    /// Squawk → indices of plans carrying it (>1 ⇒ never auto-correlated).
    by_squawk: BTreeMap<u16, Vec<usize>>,
}

impl CorrelationService {
    pub fn new(plans: Vec<FlightPlan>) -> Self {
        let mut by_callsign = BTreeMap::new();
        let mut by_squawk: BTreeMap<u16, Vec<usize>> = BTreeMap::new();
        for (i, plan) in plans.iter().enumerate() {
            by_callsign.insert(plan.callsign_key(), i);
            if let Some(code) = plan.squawk {
                by_squawk.entry(code).or_default().push(i);
            }
        }
        Self {
            plans,
            by_callsign,
            by_squawk,
        }
    }

    /// Number of loaded plans — observability hook.
    pub fn plans_total(&self) -> usize {
        self.plans.len()
    }

    /// Apply the ADR 0038 rules to one track's identity at data time `now`.
    ///
    /// 1. **Callsign first:** a (normalised) callsign match correlates —
    ///    practically unique, no ORCAM problem.
    /// 2. **Squawk only when unique:** exactly one plan carries the code,
    ///    the code is not the conspicuity code, and the track does **not**
    ///    carry an identity conflict (SPEC.1) — a duplicated identity must
    ///    never auto-correlate by code.
    /// 3. **Temporal plausibility:** when the winning plan states an
    ///    `expected_time`, `now` must lie within the window — a plan far
    ///    outside its expected window does not label today's traffic.
    pub fn correlate(
        &self,
        callsign: Option<&str>,
        squawk: Option<u16>,
        identity_conflict: bool,
        now: f64,
    ) -> CorrelationOutcome {
        if let Some(cs) = callsign {
            let key = cs.trim().to_ascii_uppercase();
            if let Some(&i) = self.by_callsign.get(&key) {
                if self.plausible(i, now) {
                    return CorrelationOutcome::ByCallsign(self.reference(i));
                }
            }
        }
        if let Some(code) = squawk {
            match self.by_squawk.get(&code).map(Vec::as_slice) {
                Some([i]) => {
                    if code == CONSPICUITY_CODE || identity_conflict {
                        return CorrelationOutcome::SquawkRefused;
                    }
                    if self.plausible(*i, now) {
                        return CorrelationOutcome::BySquawk(self.reference(*i));
                    }
                }
                Some(_) => return CorrelationOutcome::SquawkRefused,
                None => {}
            }
        }
        CorrelationOutcome::None
    }

    fn plausible(&self, i: usize, now: f64) -> bool {
        match self.plans[i].expected_time {
            Some(t) => (now - t).abs() <= TIME_WINDOW_S,
            None => true,
        }
    }

    fn reference(&self, i: usize) -> FlightPlanRef {
        let plan = &self.plans[i];
        FlightPlanRef {
            callsign: plan.callsign_key(),
            departure: plan.departure.clone(),
            destination: plan.destination.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> CorrelationService {
        CorrelationService::new(vec![
            FlightPlan {
                callsign: "DLH123".into(),
                squawk: Some(0o1234),
                departure: Some("EDDF".into()),
                destination: Some("EDDM".into()),
                expected_time: Some(10_000.0),
            },
            FlightPlan {
                callsign: "BAW22".into(),
                squawk: Some(0o4321),
                departure: None,
                destination: None,
                expected_time: None,
            },
            // The Weeze constellation: the same squawk filed twice.
            FlightPlan {
                callsign: "KLM88".into(),
                squawk: Some(0o2000),
                departure: None,
                destination: None,
                expected_time: None,
            },
            FlightPlan {
                callsign: "RYR9X".into(),
                squawk: Some(0o2000),
                departure: None,
                destination: None,
                expected_time: None,
            },
            FlightPlan {
                callsign: "GAVFR".into(),
                squawk: Some(CONSPICUITY_CODE),
                departure: None,
                destination: None,
                expected_time: None,
            },
        ])
    }

    /// Callsign wins first, normalised (case/whitespace), and carries the
    /// display fields. REQ: FR-TRK-047
    #[test]
    fn callsign_first_and_normalised() {
        let s = service();
        match s.correlate(Some(" dlh123 "), None, false, 10_100.0) {
            CorrelationOutcome::ByCallsign(r) => {
                assert_eq!(r.callsign, "DLH123");
                assert_eq!(r.departure.as_deref(), Some("EDDF"));
            }
            other => panic!("expected callsign correlation, got {other:?}"),
        }
    }

    /// A unique squawk correlates as fallback — but the Weeze duplicate,
    /// the conspicuity code and an identity-conflicted track are refused,
    /// visibly (not silently uncorrelated). REQ: FR-TRK-047
    #[test]
    fn squawk_only_when_unique_and_clean() {
        let s = service();
        assert!(matches!(
            s.correlate(None, Some(0o4321), false, 0.0),
            CorrelationOutcome::BySquawk(r) if r.callsign == "BAW22"
        ));
        assert_eq!(
            s.correlate(None, Some(0o2000), false, 0.0),
            CorrelationOutcome::SquawkRefused,
            "duplicated squawk never auto-correlates"
        );
        assert_eq!(
            s.correlate(None, Some(CONSPICUITY_CODE), false, 0.0),
            CorrelationOutcome::SquawkRefused,
            "conspicuity code 1000 is never a key"
        );
        assert_eq!(
            s.correlate(None, Some(0o4321), true, 0.0),
            CorrelationOutcome::SquawkRefused,
            "identity conflict blocks code correlation"
        );
    }

    /// The temporal window gates both keys: a plan far outside its
    /// expected time does not label today's traffic. REQ: FR-TRK-047
    #[test]
    fn expected_time_window_gates_correlation() {
        let s = service();
        assert!(matches!(
            s.correlate(Some("DLH123"), None, false, 10_000.0 + TIME_WINDOW_S - 1.0),
            CorrelationOutcome::ByCallsign(_)
        ));
        assert_eq!(
            s.correlate(Some("DLH123"), None, false, 10_000.0 + TIME_WINDOW_S + 1.0),
            CorrelationOutcome::None,
            "outside the window"
        );
    }

    /// The callsign still wins even when the identity is conflicted — the
    /// conflict only blocks the *code* fallback (a callsign is not part of
    /// the ORCAM problem). REQ: FR-TRK-047
    #[test]
    fn identity_conflict_blocks_only_the_code_path() {
        let s = service();
        assert!(matches!(
            s.correlate(Some("BAW22"), Some(0o4321), true, 0.0),
            CorrelationOutcome::ByCallsign(_)
        ));
    }
}
