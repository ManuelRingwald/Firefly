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
//! - [`SensorHealthMonitor::new_replay`]: all sensors are pre-seeded as active
//!   (last-seen = creation time, timeout = 1 hour), so a deterministic Replay
//!   never falsely reports degradation.
//!
//! Designed for `Arc` sharing: `record_activity` and `snapshot` take `&self` via
//! interior mutability (`Mutex`), so the monitor can be shared between the plot
//! ingestion callback and the CAT063 sender task.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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

/// The health status of all registered sensors at a single instant.
#[derive(Debug, Clone)]
pub struct SensorHealthSnapshot {
    /// Total number of registered sensors.
    pub sensors_total: usize,
    /// Number of sensors that are currently active (received a recent plot).
    pub sensors_active: usize,
    /// Per-sensor operational flag (true = active).
    pub per_sensor: BTreeMap<SensorId, bool>,
}

/// Tracks the wall-clock liveness of each registered sensor.
///
/// Shared between the plot-ingestion path (which calls [`record_activity`]) and
/// the CAT063 sender (which calls [`snapshot`]).
pub struct SensorHealthMonitor {
    sensors: BTreeMap<SensorId, SensorEntry>,
    last_seen: Mutex<BTreeMap<SensorId, Instant>>,
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
            last_seen: Mutex::new(BTreeMap::new()),
        }
    }

    /// Create a monitor for **Replay mode**: all sensors are pre-seeded as
    /// active with a one-hour timeout, so they remain green throughout a
    /// deterministic replay without needing any wall-clock plot callbacks.
    pub fn new_replay(sensor_ids: impl IntoIterator<Item = SensorId>) -> Self {
        let now = Instant::now();
        let timeout = Duration::from_secs(3600);
        let sensors: BTreeMap<SensorId, SensorEntry> = sensor_ids
            .into_iter()
            .map(|id| (id, SensorEntry { timeout }))
            .collect();
        let last_seen = sensors.keys().map(|&id| (id, now)).collect();
        Self {
            sensors,
            last_seen: Mutex::new(last_seen),
        }
    }

    /// Record that `sensor_id` delivered a plot batch at wall-clock `now`.
    ///
    /// Unregistered sensor IDs are silently ignored — an unexpected sensor
    /// should not crash the health monitor.
    pub fn record_activity(&self, sensor_id: SensorId, now: Instant) {
        if self.sensors.contains_key(&sensor_id) {
            self.last_seen.lock().unwrap().insert(sensor_id, now);
        }
    }

    /// Compute the current health snapshot as of wall-clock `now`.
    ///
    /// A sensor is **active** if it has been seen and its last activity is
    /// within its configured timeout (`2.5 × scan_period`).
    pub fn snapshot(&self, now: Instant) -> SensorHealthSnapshot {
        let last_seen = self.last_seen.lock().unwrap();
        let mut sensors_active = 0usize;
        let mut per_sensor = BTreeMap::new();

        for (&id, entry) in &self.sensors {
            let active = last_seen
                .get(&id)
                .map(|&t| now.duration_since(t) <= entry.timeout)
                .unwrap_or(false);
            if active {
                sensors_active += 1;
            }
            per_sensor.insert(id, active);
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
        assert!(!snap.per_sensor[&sid(1)]);
    }

    /// A sensor that just sent a plot is active.
    #[test]
    fn recently_seen_sensor_is_active() {
        let m = SensorHealthMonitor::new_live([(sid(1), 4.0)]);
        let now = Instant::now();
        m.record_activity(sid(1), now);
        let snap = m.snapshot(now);
        assert_eq!(snap.sensors_active, 1);
        assert!(snap.per_sensor[&sid(1)]);
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
        assert!(!snap.per_sensor[&sid(1)]);
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
        assert!(snap.per_sensor[&sid(1)]);
        assert!(snap.per_sensor[&sid(2)]);
        assert!(!snap.per_sensor[&sid(3)]);
    }

    /// Sensors from different IDs are independent: activity for sid(1) does
    /// not affect the staleness of sid(2).
    #[test]
    fn sensor_isolation_is_maintained() {
        let m = SensorHealthMonitor::new_live([(sid(1), 4.0), (sid(2), 4.0)]);
        let now = Instant::now();
        m.record_activity(sid(1), now);
        let snap = m.snapshot(now);
        assert!(snap.per_sensor[&sid(1)], "sid(1) was seen");
        assert!(!snap.per_sensor[&sid(2)], "sid(2) was never seen");
    }

    /// Replay mode pre-seeds all sensors as active.
    #[test]
    fn replay_mode_all_sensors_start_active() {
        let m = SensorHealthMonitor::new_replay([sid(1), sid(2), sid(3)]);
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
}
