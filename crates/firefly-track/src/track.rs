//! A single track and its lifecycle state.
//!
//! A track is a Kalman filter ([`LinearKalman`]) plus the bookkeeping that
//! decides whether it is real and whether it should stay alive:
//!
//! - **Tentative:** just born from an unassociated plot — might be clutter.
//! - **Confirmed:** seen enough times (M-of-N) to be trusted and reported.
//! - *Deleted* is not a state but removal from the track list.
//!
//! The track records its recent association outcomes (hit/miss) so the
//! [`crate::Tracker`] can apply M-of-N confirmation and miss-based deletion.

use firefly_core::TrackId;
use nalgebra::Vector2;

use crate::kalman::LinearKalman;

/// Lifecycle status of a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackStatus {
    /// On probation — not yet trusted.
    Tentative,
    /// Confirmed as a real target.
    Confirmed,
}

/// A maintained track.
#[derive(Debug, Clone)]
pub struct Track {
    id: TrackId,
    status: TrackStatus,
    pub(crate) filter: LinearKalman,
    /// Data time of the last predict/update, seconds.
    pub(crate) last_time: f64,
    /// Recent association outcomes (true = hit), capped to the confirmation
    /// window; most recent at the back.
    recent: Vec<bool>,
    /// Consecutive misses since the last hit.
    consecutive_misses: u32,
}

impl Track {
    /// Create a fresh tentative track from an initialised filter.
    pub(crate) fn new(id: TrackId, filter: LinearKalman, time: f64) -> Self {
        Self {
            id,
            status: TrackStatus::Tentative,
            filter,
            last_time: time,
            recent: Vec::new(),
            consecutive_misses: 0,
        }
    }

    /// Track identifier.
    pub fn id(&self) -> TrackId {
        self.id
    }

    /// Lifecycle status.
    pub fn status(&self) -> TrackStatus {
        self.status
    }

    /// Whether the track is confirmed.
    pub fn is_confirmed(&self) -> bool {
        self.status == TrackStatus::Confirmed
    }

    /// Estimated position `[east, north]`, metres.
    pub fn position(&self) -> Vector2<f64> {
        self.filter.position()
    }

    /// Estimated velocity `[v_east, v_north]`, m/s.
    pub fn velocity(&self) -> Vector2<f64> {
        self.filter.velocity()
    }

    /// Record one association outcome, keeping only the last `window` of them.
    pub(crate) fn observe(&mut self, hit: bool, window: usize) {
        self.recent.push(hit);
        if self.recent.len() > window {
            let excess = self.recent.len() - window;
            self.recent.drain(0..excess);
        }
        if hit {
            self.consecutive_misses = 0;
        } else {
            self.consecutive_misses += 1;
        }
    }

    /// Number of hits within the current window.
    pub(crate) fn hits_in_window(&self) -> usize {
        self.recent.iter().filter(|&&hit| hit).count()
    }

    /// Consecutive misses since the last hit.
    pub(crate) fn consecutive_misses(&self) -> u32 {
        self.consecutive_misses
    }

    /// Promote to confirmed.
    pub(crate) fn confirm(&mut self) {
        self.status = TrackStatus::Confirmed;
    }
}
