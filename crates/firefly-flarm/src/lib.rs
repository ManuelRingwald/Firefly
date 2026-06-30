//! FLARM/OGN adapter for Firefly: consumes the Open Glider Network APRS-IS
//! stream and converts each position beacon into a [`Plot`](firefly_core::Plot)
//! the tracker can fuse alongside its radar and ADS-B inputs (ADR 0026).
//!
//! # Why
//!
//! Gliders, ultralights, helicopters and low general aviation are frequently
//! invisible to ADS-B/OpenSky but carry **FLARM**; the **Open Glider Network**
//! re-broadcasts those beacons publicly over **APRS-IS**. This adapter taps that
//! stream as a complementary surveillance source — the second live input adapter
//! after OpenSky (ADR 0019), reusing the same shape: a background task that emits
//! [`Plot`](firefly_core::Plot)s with [`Measurement::Geodetic`](firefly_core::Measurement).
//!
//! # Structure
//!
//! - [`FlarmConfig`] — 12-factor configuration (env / orchestrated source).
//! - [`parse_position`] — robust OGN-flavoured APRS parser (untrusted input path).
//! - [`position_to_plot`] — map a decoded report to a tracker [`Plot`](firefly_core::Plot).
//! - [`run`] / [`run_stream`] — the APRS-IS listener and its testable stream core.
//!
//! # Security note
//!
//! APRS-IS data is **public and unauthenticated** (like ADS-B, ADR 0019); the
//! tracker trusts positions at face value. Network/source isolation (ADR 0017) is
//! the primary defence against spoofed inputs. The parser never panics on input
//! (ADR 0026 §"Sicherheit & Robustheit").
//!
//! This crate owns the adapter only; wiring it into the live tracker
//! (`FIREFLY_SOURCES` → `FlarmConfig`, spawning the listener into the plot channel)
//! is Schritt C in `firefly-server`.

mod aprsis;
mod config;
mod ogn;
mod plot;

pub use aprsis::{area_filter, login_line, run, run_stream};
pub use config::{FlarmConfig, DEFAULT_PORT, DEFAULT_SERVER, DEFAULT_SIGMA_POS_M};
pub use ogn::{parse_position, AddressType, OgnPosition};
pub use plot::position_to_plot;
