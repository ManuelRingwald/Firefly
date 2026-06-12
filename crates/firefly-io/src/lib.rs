//! Output adapters: turning the tracker's neutral `SystemTrack`s into a wire
//! format a consumer can read.
//!
//! The tracker core stays format- and transport-neutral (Ports & Adapters,
//! NFR-INT-001). This crate is the first **adapter**: it bundles the tracks of
//! one scan into a [`Frame`] — a self-describing picture of the air situation at
//! one data time — and serialises it to **JSON**, the easy-to-debug format for
//! the M3 web map (ADR 0009). A second adapter (ASTERIX CAT062) will later sit
//! beside this one on the same `SystemTrack`, without touching the core.
//!
//! ## Why a separate wire shape
//!
//! A `SystemTrack` carries position as WGS84 in **radians** and velocity as
//! east/north components — convenient for the maths. A web map wants **degrees**
//! and a ready-made ground speed / course. Deciding that shape is the adapter's
//! job, not the core's: [`FrameTrack`] is built from a `SystemTrack` and presents
//! web-friendly units, decoupled from the core's internal field layout. If the
//! map's needs change, only this crate changes.

mod frame;

pub use frame::{Frame, FramePlot, FrameTrack};
