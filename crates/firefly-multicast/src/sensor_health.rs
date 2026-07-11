//! Runtime per-sensor health monitoring (Firefly #32).
//!
//! The tracker knows which sensors are *registered*, but not whether they are
//! currently *alive*: a sensor that stops sending plots is indistinguishable
//! from an empty-sky period until the SDPS (Firefly) notices the silence.
//!
//! [`SensorHealthMonitor`] tracks the wall-clock time of the most recent plot
//! from each sensor. A sensor is **active** if its last plot arrived within
//! `2.5 × scan_period` seconds; otherwise it is **degraded**. The result is
//! exposed as a [`SensorHealthSnapshot`] that the CAT063 sender turns into
//! per-sensor status records on the wire.
//!
//! Two construction modes:
//!
//! - [`SensorHealthMonitor::new_live`]: each sensor starts with no activity
//!   recorded; activity is reported by calling [`SensorHealthMonitor::record_activity`]
//!   on each plot batch. For OpenSky Live mode.
//! - [`SensorHealthMonitor::new_preseeded`]: all sensors are pre-seeded as active
//!   (last-seen = creation time, timeout = 1 hour), so a deterministic Replay
//!   never falsely reports degradation.
//!
//! Designed for `Arc` sharing: `record_activity` and `snapshot` take `&self` via
//! interior mutability (`Mutex`), so the monitor can be shared between the plot
//! ingestion callback and the CAT063 sender task.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use firefly_asterix::SensorReason;
use firefly_core::SensorId;

/// Multiplier applied to a sensor's scan period to define the staleness
/// threshold: a sensor that has not delivered a plot within
/// `STALE_FACTOR × scan_period` seconds is considered degraded.
const STALE_FACTOR: f64 = 2.5;

/// Per-sensor configuration held by the monitor.
#[derive(Debug, Clone, Copy)]
struct SensorEntry {
    /// Wall-clock timeout after which a sensor is considered degraded, derived
    /// as `STALE_FACTOR × scan_period_secs` at construction.
    timeout: Duration,
}

/// The mutable per-sensor runtime state behind the monitor's lock: the last plot
/// time (drives liveness) and the last recorded failure reason (drives the
/// CAT063 I063/RE reason, ADR 0033).
#[derive(Debug, Clone, Copy, Default)]
struct SensorLive {
    /// Wall-clock time of the most recent plot, or `None` if never seen.
    last_seen: Option<Instant>,
    /// The most recent failure reason. Reset to [`SensorReason::Ok`] on every
    /// successful plot; set by [`SensorHealthMonitor::record_failure`].
    reason: SensorReason,
    /// A staleness timeout derived from the **measured** scan period (FEP.1,
    /// CAT034 north markers) — overrides the configured timeout when set,
    /// because the antenna's real rotation beats the nominal config value.
    timeout_override: Option<Duration>,
}

/// One sensor's health at a single instant: whether it is active and, when
/// degraded, why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorHealth {
    /// True when the sensor delivered a plot within its staleness timeout.
    pub active: bool,
    /// The last recorded failure reason ([`SensorReason::Ok`] if none). Only
    /// meaningful for a degraded sensor; the encoder emits an RE field only when
    /// the sensor is degraded and the reason is not `Ok`.
    pub reason: SensorReason,
}

/// The health status of all registered sensors at a single instant.
#[derive(Debug, Clone)]
pub struct SensorHealthSnapshot {
    /// Total number of registered sensors.
    pub sensors_total: usize,
    /// Number of sensors that are currently active (received a recent plot).
    pub sensors_active: usize,
    /// Per-sensor health (active flag + failure reason).
    pub per_sensor: BTreeMap<SensorId, SensorHealth>,
}

/// Tracks the wall-clock liveness and last failure reason of each registered
/// sensor.
///
/// Shared between the plot-ingestion path (which calls [`record_activity`] on a
/// plot and [`record_failure`] on a poll error) and the CAT063 sender (which
/// calls [`snapshot`]).
pub struct SensorHealthMonitor {
    sensors: BTreeMap<SensorId, SensorEntry>,
    live: Mutex<BTreeMap<SensorId, SensorLive>>,
}

impl SensorHealthMonitor {
    /// Create a monitor for **Live mode**: sensors start with no activity
    /// recorded (they appear degraded until the first plot arrives).
    ///
    /// `sensors` maps each `SensorId` to its scan period in seconds.
    pub fn new_live(sensors: impl IntoIterator<Item = (SensorId, f64)>) -> Self {
        let sensors = sensors
            .into_iter()
            .map(|(id, period_secs)| {
                let secs = (period_secs * STALE_FACTOR).max(1.0);
                (
                    id,
                    SensorEntry {
                        timeout: Duration::from_secs_f64(secs),
                    },
                )
            })
            .collect();
        Self {
            sensors,
            live: Mutex::new(BTreeMap::new()),
        }
    }

    /// Create a monitor whose sensors are all **pre-seeded as active** with a
    /// one-hour timeout — for tests and static sensor sets that produce no
    /// wall-clock plot callbacks. (Formerly `new_preseeded`; the scene replay
    /// mode itself was removed, ADR 0030.)
    pub fn new_preseeded(sensor_ids: impl IntoIterator<Item = SensorId>) -> Self {
        let now = Instant::now();
        let timeout = Duration::from_secs(3600);
        let sensors: BTreeMap<SensorId, SensorEntry> = sensor_ids
            .into_iter()
            .map(|id| (id, SensorEntry { timeout }))
            .collect();
        let live = sensors
            .keys()
            .map(|&id| {
                (
                    id,
                    SensorLive {
                        last_seen: Some(now),
                        reason: SensorReason::Ok,
                        timeout_override: None,
                    },
                )
            })
            .collect();
        Self {
            sensors,
            live: Mutex::new(live),
        }
    }

    /// Record that `sensor_id` delivered a plot batch at wall-clock `now`.
    ///
    /// A successful plot also **clears** any recorded failure reason back to
    /// [`SensorReason::Ok`] — the sensor is demonstrably reachable again.
    ///
    /// Unregistered sensor IDs are silently ignored — an unexpected sensor
    /// should not crash the health monitor.
    pub fn record_activity(&self, sensor_id: SensorId, now: Instant) {
        if self.sensors.contains_key(&sensor_id) {
            let mut live = self.live.lock().unwrap();
            let entry = live.entry(sensor_id).or_default();
            entry.last_seen = Some(now);
            entry.reason = SensorReason::Ok;
        }
    }

    /// Update `sensor_id`'s staleness threshold from a **measured** scan
    /// period (seconds per antenna revolution, FEP.1): the timeout becomes
    /// `STALE_FACTOR × period`, replacing the one derived from the configured
    /// nominal period. Driven by the CAT034 north-marker estimator — the
    /// antenna's real rotation is the honest basis for "how long may this
    /// sensor be silent before it counts as degraded".
    ///
    /// Non-finite or non-positive periods and unregistered sensor IDs are
    /// silently ignored.
    pub fn update_scan_period(&self, sensor_id: SensorId, period_secs: f64) {
        if !period_secs.is_finite() || period_secs <= 0.0 {
            return;
        }
        if self.sensors.contains_key(&sensor_id) {
            let secs = (period_secs * STALE_FACTOR).max(1.0);
            let mut live = self.live.lock().unwrap();
            live.entry(sensor_id).or_default().timeout_override =
                Some(Duration::from_secs_f64(secs));
        }
    }

    /// Record that a poll attempt for `sensor_id` **failed** with `reason`.
    ///
    /// This updates only the failure reason, not the last-seen time: a failed
    /// poll is not activity, so it does not keep the sensor "alive". Once the
    /// sensor goes stale (no plot within its timeout) the snapshot reports it as
    /// degraded *with this reason*, which the CAT063 sender turns into an
    /// I063/RE `SRC-REASON` sub-field (ADR 0033). A later successful plot resets
    /// the reason via [`record_activity`].
    ///
    /// Unregistered sensor IDs are silently ignored.
    pub fn record_failure(&self, sensor_id: SensorId, reason: SensorReason) {
        if self.sensors.contains_key(&sensor_id) {
            let mut live = self.live.lock().unwrap();
            live.entry(sensor_id).or_default().reason = reason;
        }
    }

    /// Compute the current health snapshot as of wall-clock `now`.
    ///
    /// A sensor is **active** if it has been seen and its last activity is
    /// within its configured timeout (`2.5 × scan_period`).
    pub fn snapshot(&self, now: Instant) -> SensorHealthSnapshot {
        let live = self.live.lock().unwrap();
        let mut sensors_active = 0usize;
        let mut per_sensor = BTreeMap::new();

        for (&id, entry) in &self.sensors {
            let state = live.get(&id).copied().unwrap_or_default();
            // The measured scan period (FEP.1), when available, defines the
            // staleness window; the configured nominal period is the fallback.
            let timeout = state.timeout_override.unwrap_or(entry.timeout);
            let active = state
                .last_seen
                .map(|t| now.duration_since(t) <= timeout)
                .unwrap_or(false);
            if active {
                sensors_active += 1;
            }
            // An active sensor has no failure reason to report; only surface a
            // reason for a degraded one (the encoder gates the RE field on this).
            let reason = if active {
                SensorReason::Ok
            } else {
                state.reason
            };
            per_sensor.insert(id, SensorHealth { active, reason });
        }

        SensorHealthSnapshot {
            sensors_total: self.sensors.len(),
            sensors_active,
            per_sensor,
        }
    }

    /// The total number of registered sensors (static, never changes after
    /// construction).
    pub fn sensors_total(&self) -> usize {
        self.sensors.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sid(n: u8) -> SensorId {
        SensorId(n as u16)
    }

    /// An unknown sensor (no registered sensors at all) → everything zero.
    #[test]
    fn empty_monitor_gives_zero_counts() {
        let m = SensorHealthMonitor::new_live([]);
        let snap = m.snapshot(Instant::now());
        assert_eq!(snap.sensors_total, 0);
        assert_eq!(snap.sensors_active, 0);
    }

    /// A live-mode sensor that has never been seen is inactive.
    #[test]
    fn never_seen_sensor_is_inactive_in_live_mode() {
        let m = SensorHealthMonitor::new_live([(sid(1), 4.0)]);
        let snap = m.snapshot(Instant::now());
        assert_eq!(snap.sensors_total, 1);
        assert_eq!(snap.sensors_active, 0);
        assert!(!snap.per_sensor[&sid(1)].active);
    }

    /// A sensor that just sent a plot is active.
    #[test]
    fn recently_seen_sensor_is_active() {
        let m = SensorHealthMonitor::new_live([(sid(1), 4.0)]);
        let now = Instant::now();
        m.record_activity(sid(1), now);
        let snap = m.snapshot(now);
        assert_eq!(snap.sensors_active, 1);
        assert!(snap.per_sensor[&sid(1)].active);
    }

    /// A sensor whose last plot is older than 2.5 × scan_period is inactive.
    #[test]
    fn stale_sensor_becomes_inactive() {
        let scan_period = 4.0f64;
        let m = SensorHealthMonitor::new_live([(sid(1), scan_period)]);
        // Activity was recorded long ago (3 × scan_period in the past).
        let past = Instant::now() - Duration::from_secs_f64(3.0 * scan_period);
        m.record_activity(sid(1), past);
        let snap = m.snapshot(Instant::now());
        assert_eq!(snap.sensors_active, 0, "sensor should be stale");
        assert!(!snap.per_sensor[&sid(1)].active);
    }

    /// Activity at exactly the threshold edge: 2.4 × scan_period → still active.
    #[test]
    fn sensor_within_threshold_is_active() {
        let scan_period = 4.0f64;
        let m = SensorHealthMonitor::new_live([(sid(1), scan_period)]);
        let just_within = Instant::now() - Duration::from_secs_f64(2.4 * scan_period);
        m.record_activity(sid(1), just_within);
        let snap = m.snapshot(Instant::now());
        assert_eq!(snap.sensors_active, 1);
    }

    /// Three sensors: two active, one never seen.  Totals match.
    #[test]
    fn multi_sensor_counts_are_correct() {
        let m = SensorHealthMonitor::new_live([(sid(1), 4.0), (sid(2), 10.0), (sid(3), 12.0)]);
        let now = Instant::now();
        m.record_activity(sid(1), now);
        m.record_activity(sid(2), now);
        // sensor 3 never records activity

        let snap = m.snapshot(now);
        assert_eq!(snap.sensors_total, 3);
        assert_eq!(snap.sensors_active, 2);
        assert!(snap.per_sensor[&sid(1)].active);
        assert!(snap.per_sensor[&sid(2)].active);
        assert!(!snap.per_sensor[&sid(3)].active);
    }

    /// Sensors from different IDs are independent: activity for sid(1) does
    /// not affect the staleness of sid(2).
    #[test]
    fn sensor_isolation_is_maintained() {
        let m = SensorHealthMonitor::new_live([(sid(1), 4.0), (sid(2), 4.0)]);
        let now = Instant::now();
        m.record_activity(sid(1), now);
        let snap = m.snapshot(now);
        assert!(snap.per_sensor[&sid(1)].active, "sid(1) was seen");
        assert!(!snap.per_sensor[&sid(2)].active, "sid(2) was never seen");
    }

    /// Replay mode pre-seeds all sensors as active.
    #[test]
    fn replay_mode_all_sensors_start_active() {
        let m = SensorHealthMonitor::new_preseeded([sid(1), sid(2), sid(3)]);
        let snap = m.snapshot(Instant::now());
        assert_eq!(snap.sensors_total, 3);
        assert_eq!(snap.sensors_active, 3);
    }

    /// An unregistered sensor ID in record_activity is silently ignored.
    #[test]
    fn unregistered_sensor_id_is_ignored() {
        let m = SensorHealthMonitor::new_live([(sid(1), 4.0)]);
        // Record activity for a sensor that was never registered — must not panic.
        m.record_activity(sid(99), Instant::now());
        let snap = m.snapshot(Instant::now());
        assert_eq!(snap.sensors_total, 1);
        assert!(!snap.per_sensor.contains_key(&sid(99)));
    }

    /// sensors_total() matches the number of sensors passed at construction.
    #[test]
    fn sensors_total_is_stable() {
        let m = SensorHealthMonitor::new_live([(sid(1), 4.0), (sid(2), 10.0)]);
        assert_eq!(m.sensors_total(), 2);
        m.record_activity(sid(1), Instant::now());
        assert_eq!(m.sensors_total(), 2);
    }

    /// A recorded failure surfaces as the degraded sensor's reason once it goes
    /// stale (the failure itself does not keep it "alive").
    #[test]
    fn recorded_failure_becomes_degraded_reason() {
        let scan_period = 4.0f64;
        let m = SensorHealthMonitor::new_live([(sid(1), scan_period)]);
        let past = Instant::now() - Duration::from_secs_f64(3.0 * scan_period);
        // A plot long ago, then a failure: the sensor is stale now.
        m.record_activity(sid(1), past);
        m.record_failure(sid(1), SensorReason::Auth);
        let snap = m.snapshot(Instant::now());
        assert!(!snap.per_sensor[&sid(1)].active, "sensor is stale");
        assert_eq!(snap.per_sensor[&sid(1)].reason, SensorReason::Auth);
    }

    /// An active sensor reports no failure reason even if a failure was recorded
    /// while it was still within its timeout window.
    #[test]
    fn active_sensor_reports_ok_reason() {
        let m = SensorHealthMonitor::new_live([(sid(1), 100.0)]);
        let now = Instant::now();
        m.record_activity(sid(1), now);
        m.record_failure(sid(1), SensorReason::RateLimited);
        let snap = m.snapshot(now);
        assert!(snap.per_sensor[&sid(1)].active);
        assert_eq!(
            snap.per_sensor[&sid(1)].reason,
            SensorReason::Ok,
            "an active sensor has no failure reason on the wire"
        );
    }

    /// A measured scan period (FEP.1) replaces the configured staleness
    /// window in both directions: a *slower* real antenna keeps an otherwise
    /// stale sensor active; a *faster* one degrades a silent sensor sooner.
    /// REQ: FR-NET-014
    #[test]
    fn measured_scan_period_overrides_the_configured_timeout() {
        // Configured 4 s (timeout 10 s); last plot 20 s ago → stale by config.
        let m = SensorHealthMonitor::new_live([(sid(1), 4.0)]);
        let past = Instant::now() - Duration::from_secs(20);
        m.record_activity(sid(1), past);
        assert!(!m.snapshot(Instant::now()).per_sensor[&sid(1)].active);

        // The antenna really turns every 10 s (timeout 25 s) → active again.
        m.update_scan_period(sid(1), 10.0);
        assert!(m.snapshot(Instant::now()).per_sensor[&sid(1)].active);

        // Measured faster than configured: 12 s silence, config 10 s (25 s
        // timeout) says active — the real 2 s antenna (5 s timeout) says stale.
        let m = SensorHealthMonitor::new_live([(sid(2), 10.0)]);
        m.record_activity(sid(2), Instant::now() - Duration::from_secs(12));
        assert!(m.snapshot(Instant::now()).per_sensor[&sid(2)].active);
        m.update_scan_period(sid(2), 2.0);
        assert!(!m.snapshot(Instant::now()).per_sensor[&sid(2)].active);
    }

    /// Garbage periods (0, negative, NaN) and unknown sensors are ignored —
    /// the untrusted CAT034 path must not corrupt the health thresholds.
    /// REQ: FR-NET-014
    #[test]
    fn implausible_measured_periods_are_ignored() {
        let m = SensorHealthMonitor::new_live([(sid(1), 4.0)]);
        m.record_activity(sid(1), Instant::now());
        m.update_scan_period(sid(1), 0.0);
        m.update_scan_period(sid(1), -3.0);
        m.update_scan_period(sid(1), f64::NAN);
        m.update_scan_period(sid(99), 8.0); // unregistered
        assert!(m.snapshot(Instant::now()).per_sensor[&sid(1)].active);
    }

    /// A successful plot clears a previously recorded failure reason.
    #[test]
    fn success_clears_failure_reason() {
        let scan_period = 4.0f64;
        let m = SensorHealthMonitor::new_live([(sid(1), scan_period)]);
        let past = Instant::now() - Duration::from_secs_f64(3.0 * scan_period);
        m.record_activity(sid(1), past);
        m.record_failure(sid(1), SensorReason::Unreachable);
        // Recovery: a fresh plot resets liveness and the reason.
        let now = Instant::now();
        m.record_activity(sid(1), now);
        let snap = m.snapshot(now);
        assert!(snap.per_sensor[&sid(1)].active);
        assert_eq!(snap.per_sensor[&sid(1)].reason, SensorReason::Ok);
    }
}
