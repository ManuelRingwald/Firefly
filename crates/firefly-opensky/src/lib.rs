//! ADS-B adapter for Firefly: polls the [OpenSky Network] REST API and
//! converts each response into [`Plot`](firefly_core::Plot)s that the tracker
//! can fuse directly with its radar inputs (ADR 0019).
//!
//! # Overview
//!
//! The adapter runs as a background task in `firefly-server`, polling
//! `https://opensky-network.org/api/states/all?lamin=…&lomax=…` on a
//! configurable interval (default 10 s).  Each state vector is converted into
//! a [`Plot`] with `Measurement::Geodetic` (WGS84 position, isotropic
//! covariance `R = σ² · I₂`) and the aircraft's ICAO-24-bit address set in
//! `mode_ac.icao_address`.  The ICAO address enables the tracker's
//! ICAO-based pre-sort (AP9.3) to bypass kinematic gating for known tracks.
//!
//! # Configuration
//!
//! All settings come from environment variables (12-factor, ADR 0003).  See
//! [`OpenSkyConfig`] for the full list.
//!
//! # Security note
//!
//! ICAO-24-bit addresses are not cryptographically authenticated.  The tracker
//! trusts them at face value; network isolation (ADR 0017) is the primary
//! defence against spoofed ADS-B inputs.  A cross-check against kinematic
//! plausibility is noted as a future refinement in ADR 0019.
//!
//! [OpenSky Network]: https://opensky-network.org

mod api;
mod auth;
mod config;
mod poller;

pub use auth::AuthError;
pub use config::OpenSkyConfig;
pub use poller::{OpenSkyPoller, PollError};
