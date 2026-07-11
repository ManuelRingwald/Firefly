//! ADS-B ground-station input adapter for Firefly: receives **ASTERIX
//! CAT021** (ADS-B Target Reports) over UDP and converts each report into a
//! geodetic [`Plot`](firefly_core::Plot) the tracker fuses alongside its
//! radar, OpenSky and FLARM inputs (FEP.3).
//!
//! # Why
//!
//! Firefly's ADS-B so far comes from **internet REST services** (OpenSky,
//! community aggregators): polled, seconds of latency, rate-limited, an
//! external dependency. A production deployment receives ADS-B from its
//! **own ground station** as CAT021 over UDP — the same transport class as
//! the radar feed: local, sub-second, push instead of poll, and carrying
//! **NACp quality indicators** from which the measurement uncertainty is
//! derived honestly instead of assumed. This is how ARTAS consumes ADS-B;
//! FEP.3 closes the last big input building block: all three operational
//! sensor classes (radar CAT048/034, ADS-B CAT021) arrive over the
//! production transport.
//!
//! # Structure (mirrors `firefly-radar`)
//!
//! - [`Adsb021Config`] — 12-factor configuration (env / orchestrated source).
//! - [`adsb_report_to_plot`] — map a decoded report to a geodetic `Plot`,
//!   with the NACp→σ derivation and the drop rules (ground/simulated/test).
//! - [`datagram_to_plots`] / [`run`] — the UDP listener and its pure core.
//!
//! The CAT021 **decoder** lives in `firefly-asterix` beside the other codecs
//! (shared FSPEC machinery); this crate owns the **transport + mapping**.
//!
//! # Security note
//!
//! ASTERIX-over-UDP is **unauthenticated**; network/source isolation
//! (ADR 0017) is the defence against spoofed feeds. The decoder never panics
//! on input (charter §8), and the mapping drops what must not enter the air
//! picture (surface, simulated and test targets).

mod config;
mod listener;
mod plot;

pub use config::{Adsb021Config, DEFAULT_PORT, NOMINAL_UPDATE_SECS};
pub use listener::{bind_socket, datagram_to_plots, run};
pub use plot::{adsb_report_to_plot, sigma_from_nacp, DEFAULT_SIGMA_POS_M};
