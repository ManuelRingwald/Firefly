//! Per-track vertical tracking (VERT.2).
//!
//! The horizontal tracker (IMM/JPDA) estimates position and ground velocity;
//! the **vertical** state — barometric altitude and rate of climb/descent —
//! has its own, much simpler dynamics and its own sensor characteristics, so
//! it gets its own small filter instead of inflating the horizontal state:
//!
//! - **Measurements** are Mode-C / flight-level reports (pressure altitude,
//!   quantised to 25 ft) from any SSR/Mode-S/ADS-B-equipped plot that
//!   associates with the track.
//! - **State** is a 2-vector `(altitude ft, rate ft/s)` with a constant-rate
//!   model and continuous white-noise acceleration as the manoeuvre budget —
//!   the level↔climb transition is exactly the manoeuvre this budget buys.
//! - **Gating** rejects Mode-C outliers (garbled replies, foreign
//!   transponders on the same code) by normalised innovation. A genuine
//!   level *jump* (which a gate would starve) is recovered by
//!   re-initialising after several consecutive rejects — the classical
//!   vertical-channel escape hatch.
//!
//! Everything is **data-time driven** (ADR 0003): the filter advances by the
//! measurement/prediction times in the data, never the wall clock. Pure and
//! serialisable — snapshot/restore (QW.4) carries it like the rest of the
//! track state.
//!
//! The filter works in **pressure-altitude space** (1013.25 hPa reference).
//! The QNH correction is an *output* concern (the server applies it via
//! `firefly-meteo` where a regional QNH is observed) — mixing reference
//! systems inside the filter would corrupt the rate estimate on every QNH
//! change.
//!
//! REQ: FR-TRK-042

/// Manoeuvre budget: continuous white-noise acceleration density, (ft/s²)²/Hz.
/// Sized for transport-category vertical transitions (~0.3–0.6 g-seconds of
/// onset): brisk enough to follow a 3000 ft/min climb starting within a few
/// updates, small enough to smooth 25-ft quantisation noise in level flight.
const ACCEL_PSD: f64 = 4.0;
/// Mode-C measurement standard deviation, ft. The reply is quantised to
/// 25 ft (σ_q = 25/√12 ≈ 7.2 ft); real replies add jitter and staircase
/// effects, so the assumed σ is deliberately a bit larger.
const MEASUREMENT_STD_FT: f64 = 12.0;
/// Innovation gate in standard deviations. Mode-C garbling produces errors
/// of hundreds to thousands of feet — far outside; genuine climbs stay
/// within because the rate state tracks them.
const GATE_SIGMAS: f64 = 5.0;
/// After this many consecutive gated-out measurements the filter
/// re-initialises onto the new level: a persistent "outlier" is not an
/// outlier but a real level change the gate would otherwise starve forever.
const REINIT_AFTER_REJECTS: u32 = 3;
/// Initial rate standard deviation, ft/s — a fresh track may be in a full
/// climb or descent (±6000 ft/min ≈ ±100 ft/s at 2σ).
const INITIAL_RATE_STD: f64 = 50.0;

/// The per-track vertical filter: barometric altitude + vertical rate.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VerticalFilter {
    /// Pressure altitude estimate, ft (1013.25 hPa reference).
    altitude_ft: f64,
    /// Vertical rate estimate, ft/s (positive = climb).
    rate_ft_s: f64,
    /// State covariance, row-major `[[p_hh, p_hv], [p_vh, p_vv]]`.
    covariance: [[f64; 2]; 2],
    /// Data time of the last accepted measurement.
    last_update_time: f64,
    /// Consecutive gated-out measurements (see [`REINIT_AFTER_REJECTS`]).
    consecutive_rejects: u32,
}

impl VerticalFilter {
    /// Initialise from the first Mode-C measurement: altitude = the reply,
    /// rate unknown (zero with a wide prior).
    pub fn from_first_measurement(altitude_ft: f64, time: f64) -> Self {
        Self {
            altitude_ft,
            rate_ft_s: 0.0,
            covariance: [
                [MEASUREMENT_STD_FT * MEASUREMENT_STD_FT, 0.0],
                [0.0, INITIAL_RATE_STD * INITIAL_RATE_STD],
            ],
            last_update_time: time,
            consecutive_rejects: 0,
        }
    }

    /// Advance the state and covariance to `time` (constant-rate prediction
    /// with the manoeuvre budget). No-op for non-forward times — out-of-order
    /// plots have already been filtered upstream by the tracker's watermark.
    fn predict_to(&mut self, time: f64) {
        let dt = time - self.last_update_time;
        if dt <= 0.0 {
            return;
        }
        self.altitude_ft += self.rate_ft_s * dt;
        let [[p_hh, p_hv], [_, p_vv]] = self.covariance;
        // F P Fᵀ for F = [[1, dt], [0, 1]] plus CWNA process noise.
        let n_hh = p_hh + 2.0 * dt * p_hv + dt * dt * p_vv + ACCEL_PSD * dt.powi(3) / 3.0;
        let n_hv = p_hv + dt * p_vv + ACCEL_PSD * dt * dt / 2.0;
        let n_vv = p_vv + ACCEL_PSD * dt;
        self.covariance = [[n_hh, n_hv], [n_hv, n_vv]];
        self.last_update_time = time;
    }

    /// Fold in one Mode-C / flight-level measurement (pressure altitude, ft)
    /// at data time `time`. Outliers are gated; persistent "outliers" (a real
    /// level jump) re-initialise the filter after
    /// [`REINIT_AFTER_REJECTS`] consecutive rejects.
    pub fn update(&mut self, measured_ft: f64, time: f64) {
        self.predict_to(time);

        let innovation = measured_ft - self.altitude_ft;
        let s = self.covariance[0][0] + MEASUREMENT_STD_FT * MEASUREMENT_STD_FT;
        if innovation * innovation > GATE_SIGMAS * GATE_SIGMAS * s {
            self.consecutive_rejects += 1;
            if self.consecutive_rejects >= REINIT_AFTER_REJECTS {
                *self = Self::from_first_measurement(measured_ft, time);
            }
            return;
        }
        self.consecutive_rejects = 0;

        // Standard 2-state Kalman update with scalar measurement H = [1, 0].
        let k_h = self.covariance[0][0] / s;
        let k_v = self.covariance[0][1] / s;
        self.altitude_ft += k_h * innovation;
        self.rate_ft_s += k_v * innovation;
        let [[p_hh, p_hv], [_, p_vv]] = self.covariance;
        self.covariance = [
            [(1.0 - k_h) * p_hh, (1.0 - k_h) * p_hv],
            [(1.0 - k_h) * p_hv, p_vv - k_v * p_hv],
        ];
        self.last_update_time = time;
    }

    /// The altitude estimate (pressure altitude, ft) **predicted to `time`**,
    /// without mutating the filter — the output-side read.
    pub fn altitude_ft_at(&self, time: f64) -> f64 {
        let dt = (time - self.last_update_time).max(0.0);
        self.altitude_ft + self.rate_ft_s * dt
    }

    /// The vertical-rate estimate, ft/min (positive = climb).
    pub fn rate_ft_min(&self) -> f64 {
        self.rate_ft_s * 60.0
    }

    /// Data time of the last **accepted** measurement — the freshness basis:
    /// a vertical estimate coasted for too long should be withheld rather
    /// than reported as current.
    pub fn last_update_time(&self) -> f64 {
        self.last_update_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Level flight with 25-ft quantisation noise converges to the level and
    /// a near-zero rate — the filter smooths what the raw reply staircases.
    /// REQ: FR-TRK-042
    #[test]
    fn level_flight_converges_and_smooths_quantisation() {
        let mut filter = VerticalFilter::from_first_measurement(35_000.0, 0.0);
        // Alternate the two quantisation steps around a true 35 012 ft.
        for i in 1..=30 {
            let z = if i % 2 == 0 { 35_000.0 } else { 35_025.0 };
            filter.update(z, i as f64);
        }
        assert!(
            (filter.altitude_ft_at(30.0) - 35_012.5).abs() < 15.0,
            "converged near the true level, got {}",
            filter.altitude_ft_at(30.0)
        );
        assert!(
            filter.rate_ft_min().abs() < 150.0,
            "level flight ⇒ near-zero rate, got {} ft/min",
            filter.rate_ft_min()
        );
    }

    /// A steady 3000 ft/min climb is followed: the rate estimate converges to
    /// the true rate and the altitude tracks without lagging away.
    /// REQ: FR-TRK-042
    #[test]
    fn steady_climb_is_followed() {
        let mut filter = VerticalFilter::from_first_measurement(10_000.0, 0.0);
        let rate_ft_s = 50.0; // 3000 ft/min
        for i in 1..=40 {
            let t = i as f64 * 4.0; // 4-s radar revisit
            let truth = 10_000.0 + rate_ft_s * t;
            let quantised = (truth / 25.0).round() * 25.0;
            filter.update(quantised, t);
        }
        assert!(
            (filter.rate_ft_min() - 3_000.0).abs() < 300.0,
            "rate converged, got {} ft/min",
            filter.rate_ft_min()
        );
        let t_end = 160.0;
        assert!(
            (filter.altitude_ft_at(t_end) - (10_000.0 + rate_ft_s * t_end)).abs() < 200.0,
            "altitude tracks the climb"
        );
    }

    /// A single garbled Mode-C reply (thousands of feet off) is gated out and
    /// does not disturb the estimate; the next good reply is accepted.
    /// REQ: FR-TRK-042
    #[test]
    fn garbled_reply_is_gated_out() {
        let mut filter = VerticalFilter::from_first_measurement(20_000.0, 0.0);
        for i in 1..=10 {
            filter.update(20_000.0, i as f64);
        }
        let before = filter.altitude_ft_at(10.0);
        filter.update(27_300.0, 11.0); // garble: bit flip worth 7300 ft
        assert!(
            (filter.altitude_ft_at(11.0) - before).abs() < 30.0,
            "outlier rejected"
        );
        filter.update(20_000.0, 12.0);
        assert!((filter.altitude_ft_at(12.0) - 20_000.0).abs() < 30.0);
    }

    /// A persistent new level (e.g. after a long coast or a genuine jump the
    /// gate would starve) re-initialises the filter instead of rejecting
    /// forever. REQ: FR-TRK-042
    #[test]
    fn persistent_new_level_reinitialises() {
        let mut filter = VerticalFilter::from_first_measurement(20_000.0, 0.0);
        for i in 1..=10 {
            filter.update(20_000.0, i as f64);
        }
        for i in 11..=13 {
            filter.update(26_000.0, i as f64);
        }
        assert!(
            (filter.altitude_ft_at(13.0) - 26_000.0).abs() < 30.0,
            "re-initialised onto the persistent new level, got {}",
            filter.altitude_ft_at(13.0)
        );
        assert_eq!(filter.rate_ft_min(), 0.0, "rate prior reset");
    }
}
