//! Flight-plan input and correlation (FPL.1, ADR 0038).
//!
//! The correlation marries a surveillance track to its flight plan ("this
//! target **is** DLH123") — the basis for strips, clearances and conflict
//! logic. Per ADR 0038 it runs **once, centrally, in the SDPS**: one
//! association for every working position and every consumer.
//!
//! This crate provides the two halves that stay wire-free in FPL.1:
//!
//! - [`FplConfig`]: the environment-driven flight-plan input
//!   (`FIREFLY_FLIGHT_PLANS`, mirroring the honest `FIREFLY_METEO_QNH`
//!   provider pattern — a configured-but-broken source is a startup error,
//!   never a silent degradation; a live FDPS feed is an explicit follow-up
//!   with its own ADR).
//! - [`CorrelationService`]: the auto-correlation rules from the Weeze
//!   operational lessons, binding per ADR 0038 — callsign first; squawk
//!   only when unique and never for an identity-conflicted track, never
//!   code 1000; time-window plausibilisation when the plan carries times.
//!
//! The wire item (I062/390) and the manual correlate/decorrelate commands
//! follow in FPL.2.
//!
//! REQ: FR-TRK-047

mod config;
mod correlation;
mod plan;

pub use config::FplConfig;
pub use correlation::{CorrelationOutcome, CorrelationService};
pub use plan::FlightPlan;
