//! Quality metrics: scoring the tracker against a known ground truth.
//!
//! Up to here the tracker's quality was argued qualitatively ("two crossing
//! targets keep their identities"). This module turns that into **numbers** by
//! comparing estimated tracks against the simulator's known truth — which is
//! both a verification artifact (assurance, ED-109A) and the foundation of a
//! demo that *shows* the tracker's value.
//!
//! Two small, pure building blocks live here:
//!
//! - [`Rmse`] — the root-mean-square position error (how far off, typically).
//! - [`TrackContinuity`] — does one target keep one unbroken track identity?
//!
//! Both are deliberately independent of the simulator: the caller feeds in the
//! errors / assignments it observed, so the metrics can be reused anywhere
//! (tests today, a live demo dashboard later).

use firefly_core::TrackId;
use nalgebra::Vector2;
use serde::{Deserialize, Serialize};

/// Accumulates a **root-mean-square error** over a stream of samples.
///
/// RMSE is the square root of the mean of the *squared* errors. Squaring makes
/// it punish large outliers more than a plain average would — a tracker that is
/// usually spot-on but occasionally wild scores worse than one that is steadily
/// mediocre. The unit is the unit of the input error (here: metres).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Rmse {
    sum_sq: f64,
    count: usize,
}

impl Rmse {
    /// A fresh accumulator with no samples.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one scalar error (already a distance / residual).
    pub fn add(&mut self, error: f64) {
        self.sum_sq += error * error;
        self.count += 1;
    }

    /// Record the Euclidean distance between an estimate and the truth.
    pub fn add_point(&mut self, estimate: Vector2<f64>, truth: Vector2<f64>) {
        self.add((estimate - truth).norm());
    }

    /// Number of samples accumulated so far.
    pub fn count(&self) -> usize {
        self.count
    }

    /// The root-mean-square error, or `None` if no samples were recorded.
    pub fn value(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some((self.sum_sq / self.count as f64).sqrt())
        }
    }
}

/// Tracks the **continuity** of one true target's representation over a run:
/// how often it had *a* track at all, and how often that track's identity
/// changed under it.
///
/// Call [`observe`](Self::observe) once per scan with the track currently
/// representing the target (`Some(id)`), or `None` if no confirmed track did.
/// The ideal is full coverage with **zero** id switches: one target, one
/// unbroken track identity from cradle to grave.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackContinuity {
    scans: usize,
    covered: usize,
    id_switches: usize,
    last_id: Option<TrackId>,
}

impl TrackContinuity {
    /// A fresh continuity counter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe one scan: the track (if any) currently representing the target.
    ///
    /// A miss (`None`) does **not** forget the last id: re-acquiring the *same*
    /// id after a gap is continuity (the track coasted and survived), whereas a
    /// *different* id is a switch (the old track died and a new one was born, or
    /// an identity was swapped). Both are counted against continuity only when
    /// the id actually changes.
    pub fn observe(&mut self, assigned: Option<TrackId>) {
        self.scans += 1;
        if let Some(id) = assigned {
            self.covered += 1;
            if let Some(prev) = self.last_id {
                if prev != id {
                    self.id_switches += 1;
                }
            }
            self.last_id = Some(id);
        }
    }

    /// Number of scans observed.
    pub fn scans(&self) -> usize {
        self.scans
    }

    /// Fraction of scans in which the target had an assigned track, in `[0, 1]`.
    pub fn coverage(&self) -> f64 {
        if self.scans == 0 {
            0.0
        } else {
            self.covered as f64 / self.scans as f64
        }
    }

    /// Number of times the assigned track id changed (ideal: `0`).
    pub fn id_switches(&self) -> usize {
        self.id_switches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RMSE is the root of the mean of the squared errors.
    #[test]
    fn rmse_is_root_mean_square() {
        let mut r = Rmse::new();
        assert_eq!(r.value(), None, "empty accumulator has no value");
        r.add(3.0);
        r.add(4.0);
        // sqrt((9 + 16) / 2) = sqrt(12.5) ≈ 3.5355
        assert!((r.value().unwrap() - 12.5_f64.sqrt()).abs() < 1e-12);
        assert_eq!(r.count(), 2);
    }

    /// A constant error makes RMSE equal that error; outliers pull it up beyond
    /// the plain mean.
    #[test]
    fn rmse_punishes_outliers() {
        let mut steady = Rmse::new();
        for _ in 0..4 {
            steady.add(10.0);
        }
        assert!((steady.value().unwrap() - 10.0).abs() < 1e-12);

        // Same mean (10) but with an outlier: {1,1,1,37} → mean 10, RMSE > 10.
        let mut spiky = Rmse::new();
        for e in [1.0, 1.0, 1.0, 37.0] {
            spiky.add(e);
        }
        assert!(spiky.value().unwrap() > 18.0, "outlier should inflate RMSE");
    }

    /// `add_point` records the Euclidean distance between estimate and truth.
    #[test]
    fn rmse_from_points_uses_euclidean_distance() {
        let mut r = Rmse::new();
        r.add_point(Vector2::new(3.0, 4.0), Vector2::new(0.0, 0.0)); // distance 5
        assert!((r.value().unwrap() - 5.0).abs() < 1e-12);
    }

    /// Coverage is the fraction of scans with an assigned track; re-acquiring
    /// the *same* id after a miss is not a switch.
    #[test]
    fn continuity_counts_coverage_not_gaps_as_switches() {
        let mut c = TrackContinuity::new();
        c.observe(Some(TrackId(1)));
        c.observe(Some(TrackId(1)));
        c.observe(None); // a coast — target temporarily unassigned
        c.observe(Some(TrackId(1))); // same id returns
        assert_eq!(c.scans(), 4);
        assert!((c.coverage() - 0.75).abs() < 1e-12);
        assert_eq!(c.id_switches(), 0);
    }

    /// A *different* id taking over is an identity switch.
    #[test]
    fn continuity_counts_id_change_as_switch() {
        let mut c = TrackContinuity::new();
        c.observe(Some(TrackId(1)));
        c.observe(Some(TrackId(2))); // switch
        c.observe(None);
        c.observe(Some(TrackId(2))); // same as last → no further switch
        assert_eq!(c.id_switches(), 1);
        assert!((c.coverage() - 0.75).abs() < 1e-12);
    }

    /// The first assignment is never a switch (no previous id to differ from).
    #[test]
    fn continuity_first_assignment_is_not_a_switch() {
        let mut c = TrackContinuity::new();
        c.observe(None);
        c.observe(None);
        c.observe(Some(TrackId(7)));
        assert_eq!(c.id_switches(), 0);
        assert!((c.coverage() - (1.0 / 3.0)).abs() < 1e-12);
    }
}
