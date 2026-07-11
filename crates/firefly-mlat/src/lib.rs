//! WAM/MLAT input adapter for Firefly: receives **ASTERIX CAT020**
//! (Multilateration Target Reports) and **CAT019** (system status) over UDP
//! and converts each report into a geodetic [`Plot`](firefly_core::Plot) the
//! tracker fuses alongside its radar and ADS-B inputs (FEP.5).
//!
//! # Why
//!
//! Wide Area Multilateration is the third operational surveillance
//! technology beside radar and ADS-B: ground receivers time the arrival of
//! transponder signals and triangulate the position — **independent**
//! surveillance (the aircraft cannot spoof its own position, unlike ADS-B)
//! in airspace where radar is impractical. ARTAS consumes WAM as
//! CAT020/CAT019; FEP.5 completes Firefly's classic input quartet: radar
//! (CAT048/034, CAT001/002), ADS-B (CAT021) and WAM (CAT020/019) all arrive
//! over the production transport.
//!
//! # Structure (mirrors `firefly-adsb021`)
//!
//! - [`MlatConfig`] — 12-factor configuration (env / orchestrated source).
//! - [`mlat_report_to_plot`] — map a decoded report to a geodetic `Plot`,
//!   with the per-report σ from I020/500 SDP and the drop rules
//!   (field monitor / simulated / test / ground).
//! - [`datagram_to_plots`] / [`run`] — the UDP listener and its pure core,
//!   dispatching CAT020 (plots) and CAT019 (status → liveness) on the
//!   leading category octet.
//!
//! The CAT020/CAT019 **decoders** live in `firefly-asterix` beside the other
//! codecs (shared FSPEC machinery); this crate owns the **transport +
//! mapping**.
//!
//! # Security note
//!
//! ASTERIX-over-UDP is **unauthenticated**; network/source isolation
//! (ADR 0017) is the defence against spoofed feeds. The decoders never panic
//! on input (charter §8), and the mapping drops what must not enter the air
//! picture (field-monitor, simulated, test and surface targets).

mod config;
mod listener;
mod plot;

pub use config::{MlatConfig, DEFAULT_PORT, NOMINAL_UPDATE_SECS};
pub use listener::{bind_socket, datagram_to_plots, datagram_to_status, run};
pub use plot::{mlat_report_to_plot, DEFAULT_SIGMA_POS_M};
