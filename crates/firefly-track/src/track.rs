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

use std::collections::BTreeSet;

use firefly_core::{ModeAC, SensorId, TrackId};
use nalgebra::Vector2;
use serde::{Deserialize, Serialize};

use crate::imm::Imm;
use crate::kalman::LinearKalman;

/// Lifecycle status of a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackStatus {
    /// On probation — not yet trusted.
    Tentative,
    /// Confirmed as a real target.
    Confirmed,
}

/// A maintained track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    id: TrackId,
    status: TrackStatus,
    /// The track's **IMM** bank (constant velocity + coordinated turns,
    /// Häppchen M5.4). Its [`combined_estimate`](Imm::combined_estimate) is the
    /// single state the rest of the tracker reasons with.
    pub(crate) imm: Imm,
    /// Data time of the last predict/update, seconds.
    pub(crate) last_time: f64,
    /// Data time of the last *real* measurement (hit), seconds. Drives the
    /// update age — how long the track has been running on prediction alone.
    pub(crate) last_hit_time: f64,
    /// Recent association outcomes (true = hit), capped to the confirmation
    /// window; most recent at the back.
    recent: Vec<bool>,
    /// Consecutive misses since the last hit.
    consecutive_misses: u32,
    /// Most recently reported Mode 3/A code ("squawk"), if any SSR-equipped
    /// plot has ever associated with this track. Sticky: a plot without an
    /// SSR reply (e.g. a primary-only detection) does not clear it.
    mode_3a: Option<u16>,
    /// Most recently reported Mode S 24-bit ICAO address, if any SSR-equipped
    /// plot has ever associated with this track. Sticky, like `mode_3a`.
    icao_address: Option<u32>,
    /// Sensors that contributed a hit (founded or updated this track) in the
    /// **most recent scan** (ADR 0010). Replaced wholesale each scan — unlike
    /// `mode_3a`/`icao_address` this is not sticky, since it answers "who sees
    /// it *right now*", not "who has ever seen it".
    contributing_sensors: BTreeSet<SensorId>,
}

impl Track {
    /// Create a fresh tentative track from an initialised IMM bank.
    pub(crate) fn new(id: TrackId, imm: Imm, time: f64) -> Self {
        Self {
            id,
            status: TrackStatus::Tentative,
            imm,
            last_time: time,
            last_hit_time: time, // the founding plot is a hit
            recent: Vec::new(),
            consecutive_misses: 0,
            mode_3a: None,
            icao_address: None,
            contributing_sensors: BTreeSet::new(),
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

    /// Whether the track is currently *coasting* — running on prediction alone
    /// because its last association outcome was a miss (no fresh measurement).
    pub fn is_coasting(&self) -> bool {
        self.consecutive_misses > 0
    }

    /// Update age: data-time elapsed since the last real measurement, seconds.
    /// `0` right after a hit; grows by one scan interval per coasted scan.
    pub fn update_age(&self) -> f64 {
        self.last_time - self.last_hit_time
    }

    /// The IMM's combined estimate — the single Kalman state the tracker reads
    /// position, velocity and uncertainty from.
    pub(crate) fn estimate(&self) -> LinearKalman {
        self.imm.combined_estimate()
    }

    /// Estimated position `[east, north]`, metres.
    pub fn position(&self) -> Vector2<f64> {
        self.estimate().position()
    }

    /// Estimated velocity `[v_east, v_north]`, m/s.
    pub fn velocity(&self) -> Vector2<f64> {
        self.estimate().velocity()
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

    /// Most recently reported Mode 3/A code ("squawk"), if known.
    pub fn mode_3a(&self) -> Option<u16> {
        self.mode_3a
    }

    /// Most recently reported Mode S 24-bit ICAO address, if known.
    pub fn icao_address(&self) -> Option<u32> {
        self.icao_address
    }

    /// Sensors that contributed a hit in the most recent scan.
    pub fn contributing_sensors(&self) -> &BTreeSet<SensorId> {
        &self.contributing_sensors
    }

    /// Clear the contributing-sensor set at the start of a new scan; sensors
    /// that hit this track again will re-add themselves via
    /// [`Track::record_hit_from`].
    pub(crate) fn reset_contributing_sensors(&mut self) {
        self.contributing_sensors.clear();
    }

    /// Record that `sensor` contributed a hit (founded or updated this track)
    /// in the current scan.
    pub(crate) fn record_hit_from(&mut self, sensor: SensorId) {
        self.contributing_sensors.insert(sensor);
    }

    /// Absorb the SSR identity (if any) of an associated plot.
    ///
    /// Sticky: a present value overwrites the stored one, but a `None` (e.g.
    /// from a primary-only detection) leaves the last known identity in
    /// place — losing one SSR reply should not erase what we already know.
    pub(crate) fn update_identity(&mut self, mode_ac: &ModeAC) {
        if mode_ac.mode_3a.is_some() {
            self.mode_3a = mode_ac.mode_3a;
        }
        if mode_ac.icao_address.is_some() {
            self.icao_address = mode_ac.icao_address;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imm::ImmConfig;
    use crate::kalman::LinearKalman;
    use crate::measurement::{convert_plot, SensorErrorModel};
    use firefly_geo::Polar;

    fn fresh_track() -> Track {
        let model = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08);
        let measurement = convert_plot(&Polar::new(50_000.0, 0.0, 0.0), &model);
        let filter = LinearKalman::from_first_measurement(&measurement, 200.0);
        let imm = ImmConfig::cv_and_turns(0.052).seed(filter);
        Track::new(TrackId(1), imm, 0.0)
    }

    /// A fresh track has no known identity yet.
    /// REQ: FR-TRK-009
    #[test]
    fn fresh_track_has_no_identity() {
        let track = fresh_track();
        assert_eq!(track.mode_3a(), None);
        assert_eq!(track.icao_address(), None);
    }

    /// An SSR reply on an associated plot is absorbed into the track.
    /// REQ: FR-TRK-009
    #[test]
    fn identity_is_absorbed_from_ssr_reply() {
        let mut track = fresh_track();
        track.update_identity(&ModeAC {
            mode_3a: Some(0o2613),
            flight_level_ft: Some(35_000.0),
            icao_address: Some(0x0040_0123),
        });
        assert_eq!(track.mode_3a(), Some(0o2613));
        assert_eq!(track.icao_address(), Some(0x0040_0123));
    }

    /// A primary-only plot (no SSR reply) does not erase a previously known
    /// identity — losing one reply should not wipe out what we already know.
    /// REQ: FR-TRK-009
    #[test]
    fn missing_ssr_reply_does_not_clear_known_identity() {
        let mut track = fresh_track();
        track.update_identity(&ModeAC {
            mode_3a: Some(0o2613),
            flight_level_ft: None,
            icao_address: Some(0x0040_0123),
        });

        track.update_identity(&ModeAC::default());

        assert_eq!(track.mode_3a(), Some(0o2613), "squawk stays sticky");
        assert_eq!(
            track.icao_address(),
            Some(0x0040_0123),
            "ICAO address stays sticky"
        );
    }

    /// A new SSR reply overwrites the previously known identity (e.g. the
    /// pilot was assigned a new squawk).
    /// REQ: FR-TRK-009
    #[test]
    fn new_ssr_reply_overwrites_known_identity() {
        let mut track = fresh_track();
        track.update_identity(&ModeAC {
            mode_3a: Some(0o2613),
            flight_level_ft: None,
            icao_address: Some(0x0040_0123),
        });
        track.update_identity(&ModeAC {
            mode_3a: Some(0o7000),
            flight_level_ft: None,
            icao_address: Some(0x0040_0123),
        });

        assert_eq!(track.mode_3a(), Some(0o7000));
    }
}
