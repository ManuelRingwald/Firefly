//! The tracker: the per-scan loop that turns a plot stream into managed tracks.
//!
//! Each scan (a batch of plots sharing a time) drives one pure state
//! transition. The order matters:
//!
//! 1. **Predict** every existing track to the scan time.
//! 2. **Convert** each plot to a Cartesian measurement (Häppchen 2.1).
//! 3. **Associate** predicted tracks with measurements (gating + GNN, 2.3/2.4).
//! 4. **Update** associated tracks (a *hit*); **coast** the rest (a *miss*).
//! 5. **Confirm** tentative tracks that reach M-of-N hits.
//! 6. **Delete** tracks that have missed too often.
//! 7. **Initiate** a new tentative track from each unassociated plot.
//!
//! Determinism (ADR 0003): [`Tracker::process_scan`] is a pure function of the
//! current state, the scan time and the plots — no wall clock, no I/O — so the
//! whole run is replayable and the state is recoverable. The state
//! ([`Track`] list) is plain, serialisable data (NFR-CLOUD-001/002/003).

use firefly_core::{Plot, Timestamp, TrackId};

use crate::association::associate;
use crate::gating::Gate;
use crate::kalman::{LinearKalman, ProcessNoise};
use crate::measurement::{convert_plot, SensorErrorModel};
use crate::track::{Track, TrackStatus};

/// Tunable parameters of the tracker.
#[derive(Debug, Clone, Copy)]
pub struct TrackerConfig {
    /// The tracker's assumed sensor noise (used to convert plots).
    pub sensor_error_model: SensorErrorModel,
    /// Process noise (the manoeuvre budget) for prediction.
    pub process_noise: ProcessNoise,
    /// The validation gate.
    pub gate: Gate,
    /// Confirmation needs `confirm_m` hits within the last `confirm_n` scans.
    pub confirm_m: usize,
    /// Window length for the M-of-N confirmation rule.
    pub confirm_n: usize,
    /// Delete a *tentative* track after this many consecutive misses.
    pub delete_misses_tentative: u32,
    /// Delete a *confirmed* track after this many consecutive misses.
    pub delete_misses_confirmed: u32,
    /// Initial 1σ velocity uncertainty for a newly born track, m/s.
    pub initial_velocity_std: f64,
}

impl TrackerConfig {
    /// Sensible defaults around a given sensor error model: confirm 3-of-5,
    /// delete tentative after 2 misses and confirmed after 4.
    pub fn new(sensor_error_model: SensorErrorModel) -> Self {
        Self {
            sensor_error_model,
            process_noise: ProcessNoise::new(0.5),
            gate: Gate::from_probability(0.99),
            confirm_m: 3,
            confirm_n: 5,
            delete_misses_tentative: 2,
            delete_misses_confirmed: 4,
            initial_velocity_std: 200.0,
        }
    }
}

/// A single-radar multi-target tracker.
#[derive(Debug, Clone)]
pub struct Tracker {
    config: TrackerConfig,
    tracks: Vec<Track>,
    next_id: u32,
}

impl Tracker {
    pub fn new(config: TrackerConfig) -> Self {
        Self {
            config,
            tracks: Vec::new(),
            next_id: 1,
        }
    }

    /// All tracks the tracker currently maintains (tentative and confirmed).
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// Only the confirmed tracks — the air picture worth reporting.
    pub fn confirmed_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(|t| t.is_confirmed())
    }

    /// Number of confirmed tracks.
    pub fn confirmed_count(&self) -> usize {
        self.tracks.iter().filter(|t| t.is_confirmed()).count()
    }

    /// Process one scan: a batch of plots that share the time `time`.
    ///
    /// The batch may be empty (no detections this scan), in which case every
    /// track simply coasts.
    ///
    /// REQ: FR-TRK-001, FR-TRK-006
    pub fn process_scan(&mut self, time: Timestamp, plots: &[Plot]) {
        let cfg = self.config;
        let t = time.as_secs();

        // 1. Predict every existing track forward to the scan time.
        for track in &mut self.tracks {
            let dt = t - track.last_time;
            if dt > 0.0 {
                track.filter.predict(dt, &cfg.process_noise);
                track.last_time = t;
            }
        }

        // 2. Convert plots to Cartesian measurements.
        let measurements: Vec<_> = plots
            .iter()
            .map(|p| convert_plot(&p.measurement, &cfg.sensor_error_model))
            .collect();

        // 3. Associate predicted tracks with measurements.
        let filters: Vec<LinearKalman> = self.tracks.iter().map(|tr| tr.filter).collect();
        let assoc = associate(&filters, &measurements, &cfg.gate);

        // 4a. Update associated tracks (a hit).
        for &(ti, mi) in &assoc.pairs {
            self.tracks[ti].filter.update(&measurements[mi]);
            self.tracks[ti].observe(true, cfg.confirm_n);
        }
        // 4b. Coast unassociated tracks (a miss).
        for &ti in &assoc.unassigned_tracks {
            self.tracks[ti].observe(false, cfg.confirm_n);
        }

        // 5. Confirm tentative tracks that have reached M-of-N.
        for track in &mut self.tracks {
            if track.status() == TrackStatus::Tentative && track.hits_in_window() >= cfg.confirm_m {
                track.confirm();
            }
        }

        // 6. Delete tracks that have missed too often.
        self.tracks.retain(|track| !should_delete(track, &cfg));

        // 7. Initiate a new tentative track from each unassociated plot.
        for &mi in &assoc.unassigned_measurements {
            let filter =
                LinearKalman::from_first_measurement(&measurements[mi], cfg.initial_velocity_std);
            let mut track = Track::new(TrackId(self.next_id), filter, t);
            self.next_id += 1;
            track.observe(true, cfg.confirm_n); // the founding plot is a hit
            self.tracks.push(track);
        }
    }
}

/// Whether a track has missed often enough to be deleted, given its status.
fn should_delete(track: &Track, cfg: &TrackerConfig) -> bool {
    let limit = match track.status() {
        TrackStatus::Tentative => cfg.delete_misses_tentative,
        TrackStatus::Confirmed => cfg.delete_misses_confirmed,
    };
    track.consecutive_misses() >= limit
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::{Plot, SensorId};
    use firefly_geo::Polar;

    fn config() -> TrackerConfig {
        TrackerConfig::new(SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08))
    }

    /// A plot at a fixed polar position for a given time.
    fn plot(time: f64, range: f64, az: f64) -> Plot {
        Plot::primary(SensorId(1), Timestamp(time), Polar::new(range, az, 0.0))
    }

    /// A new track is born tentative, then confirmed once M-of-N hits accrue.
    /// REQ: FR-TRK-001, FR-TRK-006
    #[test]
    fn track_is_born_tentative_then_confirmed() {
        let mut tracker = Tracker::new(config());
        let p = || plot(0.0, 50_000.0, 0.0);

        // Scan 0: birth.
        tracker.process_scan(Timestamp(0.0), &[p()]);
        assert_eq!(tracker.tracks().len(), 1);
        assert_eq!(tracker.tracks()[0].status(), TrackStatus::Tentative);

        // Scans 1 and 2: still the same track, now reaching 3 hits → confirmed.
        tracker.process_scan(Timestamp(4.0), &[plot(4.0, 50_000.0, 0.0)]);
        tracker.process_scan(Timestamp(8.0), &[plot(8.0, 50_000.0, 0.0)]);
        assert_eq!(tracker.tracks().len(), 1);
        assert_eq!(tracker.confirmed_count(), 1);
    }

    /// A confirmed track coasts through missed scans and is finally deleted.
    /// REQ: FR-TRK-006
    #[test]
    fn confirmed_track_coasts_then_dies() {
        let mut tracker = Tracker::new(config());
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_scan(Timestamp(t), &[plot(t, 50_000.0, 0.0)]);
        }
        assert_eq!(tracker.confirmed_count(), 1);

        // Now the target vanishes: feed empty scans. delete_misses_confirmed = 4.
        for k in 3..7 {
            let t = k as f64 * 4.0;
            tracker.process_scan(Timestamp(t), &[]);
        }
        assert_eq!(
            tracker.tracks().len(),
            0,
            "track should be deleted after 4 misses"
        );
    }

    /// A lone tentative track (e.g. a clutter plot) dies quickly when not seen again.
    /// REQ: FR-TRK-006
    #[test]
    fn tentative_track_dies_quickly() {
        let mut tracker = Tracker::new(config());
        tracker.process_scan(Timestamp(0.0), &[plot(0.0, 30_000.0, 1.0)]);
        assert_eq!(tracker.tracks().len(), 1);
        // Two empty scans (delete_misses_tentative = 2).
        tracker.process_scan(Timestamp(4.0), &[]);
        tracker.process_scan(Timestamp(8.0), &[]);
        assert_eq!(tracker.tracks().len(), 0);
    }

    /// Two well-separated plots create two distinct tracks.
    /// REQ: FR-TRK-001
    #[test]
    fn separated_plots_make_two_tracks() {
        let mut tracker = Tracker::new(config());
        tracker.process_scan(
            Timestamp(0.0),
            &[plot(0.0, 50_000.0, 0.0), plot(0.0, 50_000.0, 1.0)],
        );
        assert_eq!(tracker.tracks().len(), 2);
    }
}
