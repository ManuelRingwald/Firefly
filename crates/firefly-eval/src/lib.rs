//! Surveillance-quality evaluation harness (HA.4).
//!
//! The quantitative answer to "how good is the tracker?": run a scenario
//! with **known ground truth** through the full [`Tracker`] and score its
//! output with the standard surveillance metrics — measured, not believed.
//! Every future change (tuning, optimisation, refactor) becomes measurably
//! better or worse instead of anecdotally so.
//!
//! **Metric definitions follow the EUROCONTROL specification for ATM
//! surveillance system performance (ESASSP)** in naming and intent — the
//! *authoritative* part of an evaluation is the public metric definition,
//! not the tool that computes it (see `docs/milestones/HA4-…md` for the
//! mapping table). The honest counterpart: this harness measures against
//! *simulated* truth, which is methodically clean (the truth is exact) but
//! only covers what the simulator models; an independent cross-check with
//! a third-party tool over the real CAT062 output is the follow-up bite.
//!
//! Determinism (NFR-CLOUD-001) makes the report reproducible: the same
//! scenario and seed produce byte-identical JSON.
//!
//! REQ: FR-TRK-051

pub mod harness;
pub mod report;
pub mod scenarios;

pub use harness::{evaluate, evaluate_against, tracker_for, EvalConfig};
pub use report::{AggregateReport, EvalReport, TargetReport};

pub use firefly_track::Tracker;
