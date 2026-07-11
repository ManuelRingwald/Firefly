//! Meteo/QNH service for Firefly — the SDPS-003 analogue (VERT.1).
//!
//! # Why
//!
//! Mode-C / flight levels are **pressure altitudes** referenced to the ICAO
//! standard atmosphere (1013.25 hPa). Below the transition altitude, traffic
//! flies on **QNH** — the local, weather-dependent sea-level pressure.
//! Without a QNH correction the displayed altitude is wrong by roughly
//! 27–30 ft per hPa; a strong low (e.g. 983 hPa) puts the error above
//! 800 ft — safety-relevant in the approach/departure environment. ARTAS
//! carries a **meteo function (SDPS-003)** for exactly this: a QNH source
//! with regions and an update cycle. This crate is that foundation; the
//! vertical tracking that consumes it (QNH-corrected altitude → I062/135)
//! is VERT.2.
//!
//! # What this crate provides
//!
//! - [`QnhService`] — a set of **QNH regions**; [`QnhService::lookup`]
//!   returns the applicable QNH for a position, or — honestly flagged —
//!   the **standard atmosphere** when no region applies. A made-up QNH is
//!   never invented.
//! - [`pressure_altitude_to_qnh_altitude`] — the ICAO barometric
//!   conversion from a 1013.25-referenced pressure altitude to a
//!   QNH-referenced altitude (exact formula, not the linear rule of thumb).
//! - [`MeteoConfig`] — 12-factor configuration: `FIREFLY_METEO_QNH` carries
//!   the region list as JSON. A malformed value is a **startup error** — a
//!   configured-but-broken meteo source must not be silently dropped.
//!
//! # Honest boundaries
//!
//! The env-driven provider is deliberately the first step: an operator (or
//! Wayfinder's orchestrator) sets the regional QNH values and refreshes them
//! externally. A live provider (periodic METAR fetch) needs a deployment
//! network-policy decision and its own ADR — a follow-up step, not silently
//! included here.

mod altitude;
mod config;
mod qnh;

pub use altitude::{pressure_altitude_to_qnh_altitude, ISA_STANDARD_PRESSURE_HPA};
pub use config::{MeteoConfig, MeteoConfigError};
pub use qnh::{Qnh, QnhRegion, QnhService, QnhSource};
