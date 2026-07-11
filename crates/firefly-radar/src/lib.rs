//! Radar ASTERIX input adapter for Firefly: receives a real monoradar's
//! **ASTERIX CAT048** ("Monoradar Target Reports") over UDP and converts each
//! target report into a [`Plot`](firefly_core::Plot) the tracker fuses alongside
//! its ADS-B and FLARM inputs (ADR 0028).
//!
//! # Why
//!
//! ADS-B (OpenSky, ADR 0019) and FLARM (OGN, ADR 0026) are **cooperative,
//! geodetic** self-reports. The classic surveillance source of an ATC system is a
//! **radar**, whose detections are **polar** (range/azimuth) relative to the
//! antenna and arrive as ASTERIX CAT048. This adapter closes that gap — the third
//! and last reserved source type of the input contract (ADR 0023) — and makes
//! Firefly a true multi-sensor tracker (PSR/SSR/Mode S fused with ADS-B/FLARM).
//!
//! # Structure
//!
//! - [`RadarConfig`] — 12-factor configuration (env / orchestrated source),
//!   including the **radar site position** (CAT048 is polar and carries no site).
//! - [`target_report_to_plot`] — map a decoded [`DecodedTargetReport`](firefly_asterix::DecodedTargetReport)
//!   to a tracker [`Plot`](firefly_core::Plot) with the right detection/source kind.
//! - [`datagram_to_plots`] / [`run`] — the UDP listener and its pure decode core.
//!
//! The CAT048 **decoder** itself lives in `firefly-asterix` beside the CAT062/063/065
//! codecs (shared FSPEC machinery); this crate owns the **transport + mapping**.
//!
//! # Security note
//!
//! ASTERIX-over-UDP is **unauthenticated** (like ADS-B). The tracker trusts
//! positions at face value; network/source isolation (ADR 0017) is the defence
//! against spoofed feeds. The decoder never panics on input (ADR 0028
//! §"Sicherheit & Robustheit").
//!
//! This crate owns the adapter only; wiring it into the live tracker
//! (`FIREFLY_SOURCES` → `RadarConfig`, registering the radar sensor with its site
//! frame, spawning the listener into the plot channel) is Schritt D in
//! `firefly-server`.

mod config;
mod listener;
mod plot;
mod service;

pub use config::{
    RadarConfig, DEFAULT_PORT, DEFAULT_SCAN_PERIOD_SECS, DEFAULT_SIGMA_AZIMUTH_DEG,
    DEFAULT_SIGMA_RANGE_M,
};
pub use listener::{bind_socket, datagram_to_plots, datagram_to_service, listen_addr, run};
pub use plot::target_report_to_plot;
pub use service::ScanPeriodEstimator;
