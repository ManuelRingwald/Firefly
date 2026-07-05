//! ADS-B adapter for Firefly: polls an **ADSBExchange-v2-compatible community
//! aggregator** (adsb.lol, adsb.fi) and converts each response into
//! [`Plot`](firefly_core::Plot)s the tracker can fuse with its other inputs
//! (ADR 0031).
//!
//! # Why a second ADS-B adapter?
//!
//! OpenSky (ADR 0019/0024) requires OAuth2 credentials and drops connections
//! from many datacenter IP ranges, which makes it unusable from cloud dev
//! environments. The community aggregators serve the same crowdsourced ADS-B
//! picture **without any authentication** and without such blocks — a second,
//! independent procurement path for the same surveillance data. The operator
//! chooses the provider per source; OpenSky remains fully supported alongside.
//!
//! # Overview
//!
//! The adapter runs as a background task in `firefly-server`, polling the
//! provider's point-query endpoint (centre + radius, derived from the
//! configured bounding box — see [`geometry`](crate::geometry) internals) on a
//! configurable interval (default 10 s). Each usable aircraft becomes a
//! [`Plot`] with `Measurement::Geodetic` (WGS84 position, isotropic covariance)
//! and, where present, the ICAO 24-bit address for the tracker's ICAO pre-sort.
//!
//! # Configuration
//!
//! All settings come from environment variables (12-factor, ADR 0003) — see
//! [`AdsbAggConfig`] — or, orchestrated, from the `FIREFLY_SOURCES` contract
//! (v1.5.0, type `adsb_aggregator`).
//!
//! # Security note
//!
//! Community aggregator data is crowdsourced and unauthenticated end-to-end;
//! like OpenSky it is a research/hobby-grade source, not a certified
//! surveillance feed. ICAO addresses are trusted at face value (same trust
//! boundary as ADR 0019; network isolation per ADR 0017).

mod api;
mod config;
mod geometry;
mod poller;

pub use config::{AdsbAggConfig, Provider};
pub use poller::{AdsbAggPoller, PollError};
