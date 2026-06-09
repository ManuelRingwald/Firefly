//! Strongly-typed identifiers.
//!
//! Wrapping the raw integers keeps a sensor id from ever being mistaken for a
//! track id at a call site, which is exactly the kind of mix-up that is painful
//! to debug once plots from several radars are flowing through one pipeline.

use serde::{Deserialize, Serialize};

/// Identifies a physical sensor (radar site / channel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SensorId(pub u16);

/// Identifies a confirmed or tentative track maintained by the tracker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TrackId(pub u32);

/// Identifies a ground-truth target inside the simulator. This never leaves the
/// simulation domain — the tracker must rediscover targets as tracks on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TargetId(pub u32);

impl std::fmt::Display for SensorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SEN{:03}", self.0)
    }
}

impl std::fmt::Display for TrackId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TRK{:04}", self.0)
    }
}

impl std::fmt::Display for TargetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TGT{:04}", self.0)
    }
}
