//! Shared domain types for the Firefly radar tracker.
//!
//! These types are the vocabulary spoken between the data producers
//! (simulator, ASTERIX decoder) and the consumers (tracker, server). Keeping
//! them in a dependency-light crate lets every other crate agree on what a
//! "plot" or a "sensor" is without pulling in heavy machinery.

mod ids;
mod plot;
mod sensor;
mod system_track;
mod time;

pub use ids::{SensorId, TargetId, TrackId};
pub use plot::{Callsign, Daps, DetectionKind, Measurement, ModeAC, Plot, SourceKind};
pub use sensor::Sensor;
pub use system_track::{Provenance, SourceAges, SystemTrack, PROVENANCE_FRESH_S};
pub use time::Timestamp;
