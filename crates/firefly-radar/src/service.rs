//! Scan-period estimation from CAT034 north markers (FEP.1).
//!
//! A radar's configured scan period is a *nominal* value; the antenna's real
//! rotation drifts (motor load, wind, maintenance). Safety-relevant thresholds
//! key off the scan period — the CAT063 liveness window, the provenance
//! freshness window — so measuring the true period from the data stream beats
//! trusting the config. The measurement is simple and robust: the **time
//! between consecutive north markers** *is* one antenna revolution.
//!
//! [`ScanPeriodEstimator`] is pure and **data-time driven** (ADR 0003): it is
//! fed the I034/030 time-of-day of each north marker and never reads a clock,
//! so a replay reproduces the same estimates. Robustness against a hostile or
//! glitchy feed (the CAT034 path is untrusted input, charter §8):
//!
//! - Deltas outside a plausible antenna band are rejected outright — a bogus
//!   timestamp cannot poison the estimate.
//! - Once an estimate exists, a delta far from it (a **missed marker**: one
//!   lost datagram doubles the delta) is rejected rather than averaged in.
//! - Accepted deltas update the estimate through exponential smoothing, so a
//!   single noisy revolution nudges rather than jerks the value.
//!
//! REQ: FR-NET-014

/// No real surveillance antenna turns faster than this (seconds/revolution);
/// smaller deltas are duplicates or timestamp glitches.
const MIN_PLAUSIBLE_PERIOD_S: f64 = 1.0;
/// No real surveillance antenna turns slower than this; larger deltas mean
/// the feed was interrupted, not that the antenna slowed down.
const MAX_PLAUSIBLE_PERIOD_S: f64 = 60.0;
/// Relative deviation from the current estimate beyond which a delta is
/// treated as a missed-marker artefact (a single lost north marker doubles
/// the observed delta — far outside this band).
const OUTLIER_RELATIVE: f64 = 0.5;
/// Exponential smoothing factor per accepted revolution.
const SMOOTHING_ALPHA: f64 = 0.25;
/// The ASTERIX day length: I034/030 wraps to 0 at UTC midnight.
const SECONDS_PER_DAY: f64 = 86_400.0;

/// Measures a radar's true antenna rotation period from the data-times of its
/// CAT034 north markers. One estimator per radar sensor.
#[derive(Debug, Default)]
pub struct ScanPeriodEstimator {
    /// Time-of-day of the previous north marker, if any.
    last_north_tod: Option<f64>,
    /// The smoothed period estimate, once at least one plausible revolution
    /// has been observed.
    period_secs: Option<f64>,
    /// North markers observed (accepted or not), for the metrics counter.
    north_markers: u64,
    /// Deltas rejected as implausible or missed-marker artefacts.
    rejected: u64,
}

impl ScanPeriodEstimator {
    /// A fresh estimator with no measurement yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one north marker's time of day (seconds since UTC midnight,
    /// I034/030). Returns the current period estimate — updated when this
    /// marker completed a plausible revolution.
    pub fn observe_north_marker(&mut self, tod_secs: f64) -> Option<f64> {
        self.north_markers += 1;
        let Some(last) = self.last_north_tod.replace(tod_secs) else {
            return self.period_secs;
        };

        // A north marker just after midnight follows one just before it: the
        // wrapped delta is the true revolution time (ICD §6 semantics).
        let mut delta = tod_secs - last;
        if delta < 0.0 {
            delta += SECONDS_PER_DAY;
        }

        if !(MIN_PLAUSIBLE_PERIOD_S..=MAX_PLAUSIBLE_PERIOD_S).contains(&delta) {
            self.rejected += 1;
            return self.period_secs;
        }
        match self.period_secs {
            None => self.period_secs = Some(delta),
            Some(current) if (delta - current).abs() <= OUTLIER_RELATIVE * current => {
                self.period_secs = Some(current + SMOOTHING_ALPHA * (delta - current));
            }
            Some(_) => self.rejected += 1, // missed marker(s): keep the estimate
        }
        self.period_secs
    }

    /// The current smoothed period estimate, seconds per revolution.
    pub fn period_secs(&self) -> Option<f64> {
        self.period_secs
    }

    /// Total north markers observed.
    pub fn north_markers(&self) -> u64 {
        self.north_markers
    }

    /// Deltas rejected as implausible or missed-marker artefacts.
    pub fn rejected(&self) -> u64 {
        self.rejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A steady 4-s antenna yields a 4-s estimate after two markers, and the
    /// estimate stays put over many revolutions. REQ: FR-NET-014
    #[test]
    fn steady_rotation_is_measured_exactly() {
        let mut est = ScanPeriodEstimator::new();
        assert_eq!(
            est.observe_north_marker(100.0),
            None,
            "one marker: no delta"
        );
        for k in 1..=20 {
            let period = est
                .observe_north_marker(100.0 + k as f64 * 4.0)
                .expect("estimate exists");
            assert!((period - 4.0).abs() < 1e-9, "revolution {k}: {period}");
        }
        assert_eq!(est.north_markers(), 21);
        assert_eq!(est.rejected(), 0);
    }

    /// A drifting antenna is followed smoothly: the estimate moves toward the
    /// new period without jumping to it. REQ: FR-NET-014
    #[test]
    fn drifting_rotation_is_followed_smoothly() {
        let mut est = ScanPeriodEstimator::new();
        let mut t = 0.0;
        for _ in 0..10 {
            t += 4.0;
            est.observe_north_marker(t);
        }
        // The antenna slows to 4.8 s (within the 50 % outlier band).
        let before = est.period_secs().unwrap();
        t += 4.8;
        let after = est.observe_north_marker(t).unwrap();
        assert!(
            after > before && after < 4.8,
            "smoothed, not jumped: {after}"
        );
        for _ in 0..40 {
            t += 4.8;
            est.observe_north_marker(t);
        }
        assert!(
            (est.period_secs().unwrap() - 4.8).abs() < 0.05,
            "converges to the new period: {}",
            est.period_secs().unwrap()
        );
    }

    /// A missed north marker (lost datagram → doubled delta) does not poison
    /// the estimate, and the stream recovers afterwards. REQ: FR-NET-014
    #[test]
    fn missed_marker_is_rejected_not_averaged() {
        let mut est = ScanPeriodEstimator::new();
        let mut t = 0.0;
        for _ in 0..5 {
            t += 4.0;
            est.observe_north_marker(t);
        }
        t += 8.0; // one marker lost: doubled delta
        est.observe_north_marker(t);
        assert!(
            (est.period_secs().unwrap() - 4.0).abs() < 1e-9,
            "unpoisoned"
        );
        assert_eq!(est.rejected(), 1);
        t += 4.0; // next revolution is normal again
        assert!((est.observe_north_marker(t).unwrap() - 4.0).abs() < 1e-9);
    }

    /// The midnight wrap of I034/030 (a marker just before 86 400 s followed
    /// by one just after 0) measures the true revolution, not a negative or
    /// day-long delta. REQ: FR-NET-014
    #[test]
    fn midnight_wrap_measures_the_true_revolution() {
        let mut est = ScanPeriodEstimator::new();
        est.observe_north_marker(86_398.0);
        let period = est.observe_north_marker(2.0).expect("wrapped delta");
        assert!((period - 4.0).abs() < 1e-9, "wrap-corrected: {period}");
    }

    /// Implausible deltas — a duplicate marker (0 s) or a feed gap (hours) —
    /// are rejected outright, before and after an estimate exists.
    /// REQ: FR-NET-014
    #[test]
    fn implausible_deltas_are_rejected() {
        let mut est = ScanPeriodEstimator::new();
        est.observe_north_marker(10.0);
        assert_eq!(
            est.observe_north_marker(10.0),
            None,
            "duplicate: no estimate"
        );
        assert_eq!(est.observe_north_marker(8_000.0), None, "gap: no estimate");
        assert_eq!(est.rejected(), 2);
        // A plausible pair afterwards still initialises the estimate.
        est.observe_north_marker(8_004.0);
        assert!((est.period_secs().unwrap() - 4.0).abs() < 1e-9);
    }
}
