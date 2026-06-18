//! 12-factor configuration for the OpenSky Network adapter (ADR 0019, ADR 0003).
//!
//! All settings come from environment variables so the adapter can be enabled,
//! tuned and authenticated without code changes or config files.

use firefly_core::SensorId;

/// Configuration for the OpenSky Network poller.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenSkyConfig {
    /// Whether to start polling at all. `FIREFLY_OPENSKY_ENABLED`, default `false`.
    pub enabled: bool,
    /// Southern latitude bound of the bounding box, degrees. `FIREFLY_OPENSKY_LAT_MIN`,
    /// default `47.0` (southern Germany / Swiss border).
    pub lat_min: f64,
    /// Northern latitude bound, degrees. `FIREFLY_OPENSKY_LAT_MAX`, default `55.0`
    /// (northern Germany / Danish border).
    pub lat_max: f64,
    /// Western longitude bound, degrees. `FIREFLY_OPENSKY_LON_MIN`, default `5.0`.
    pub lon_min: f64,
    /// Eastern longitude bound, degrees. `FIREFLY_OPENSKY_LON_MAX`, default `16.0`.
    pub lon_max: f64,
    /// Seconds between polls. `FIREFLY_OPENSKY_POLL_INTERVAL_SECS`, default `10`.
    /// Anonymous access is rate-limited to approximately one request per 10 s;
    /// authenticated access allows ~5 s.
    pub poll_interval_secs: u64,
    /// Optional HTTP Basic-Auth username. `FIREFLY_OPENSKY_USERNAME`.
    pub username: Option<String>,
    /// Optional HTTP Basic-Auth password. `FIREFLY_OPENSKY_PASSWORD`.
    pub password: Option<String>,
    /// The [`SensorId`] stamped onto every ADS-B plot. `FIREFLY_OPENSKY_SENSOR_ID`,
    /// default `200`. Allows the tracker to attribute plots to the OpenSky adapter
    /// and to apply a dedicated noise model (ADR 0019).
    pub sensor_id: SensorId,
}

impl Default for OpenSkyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            lat_min: 47.0,
            lat_max: 55.0,
            lon_min: 5.0,
            lon_max: 16.0,
            poll_interval_secs: 10,
            username: None,
            password: None,
            sensor_id: SensorId(200),
        }
    }
}

impl OpenSkyConfig {
    /// Read configuration from the process environment.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Read configuration from an arbitrary key→value lookup (testable without
    /// touching the real process environment).
    ///
    /// Unset or unparseable values fall back to the defaults.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let d = Self::default();
        let enabled = get("FIREFLY_OPENSKY_ENABLED")
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(d.enabled);
        let lat_min = get("FIREFLY_OPENSKY_LAT_MIN")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(d.lat_min);
        let lat_max = get("FIREFLY_OPENSKY_LAT_MAX")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(d.lat_max);
        let lon_min = get("FIREFLY_OPENSKY_LON_MIN")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(d.lon_min);
        let lon_max = get("FIREFLY_OPENSKY_LON_MAX")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(d.lon_max);
        let poll_interval_secs = get("FIREFLY_OPENSKY_POLL_INTERVAL_SECS")
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&s| s > 0)
            .unwrap_or(d.poll_interval_secs);
        let username = get("FIREFLY_OPENSKY_USERNAME").filter(|s| !s.is_empty());
        let password = get("FIREFLY_OPENSKY_PASSWORD").filter(|s| !s.is_empty());
        let sensor_id = get("FIREFLY_OPENSKY_SENSOR_ID")
            .and_then(|v| v.parse::<u16>().ok())
            .map(SensorId)
            .unwrap_or(d.sensor_id);
        Self {
            enabled,
            lat_min,
            lat_max,
            lon_min,
            lon_max,
            poll_interval_secs,
            username,
            password,
            sensor_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unset environment → defaults apply; adapter is off by default.
    #[test]
    fn empty_environment_yields_defaults() {
        let config = OpenSkyConfig::from_lookup(|_| None);
        assert_eq!(config, OpenSkyConfig::default());
        assert!(!config.enabled, "adapter is off unless explicitly enabled");
    }

    /// All env-vars parse correctly.
    #[test]
    fn valid_values_are_parsed() {
        let config = OpenSkyConfig::from_lookup(|key| match key {
            "FIREFLY_OPENSKY_ENABLED" => Some("true".into()),
            "FIREFLY_OPENSKY_LAT_MIN" => Some("48.0".into()),
            "FIREFLY_OPENSKY_LAT_MAX" => Some("54.0".into()),
            "FIREFLY_OPENSKY_LON_MIN" => Some("6.0".into()),
            "FIREFLY_OPENSKY_LON_MAX" => Some("15.0".into()),
            "FIREFLY_OPENSKY_POLL_INTERVAL_SECS" => Some("5".into()),
            "FIREFLY_OPENSKY_USERNAME" => Some("alice".into()),
            "FIREFLY_OPENSKY_PASSWORD" => Some("s3cret".into()),
            "FIREFLY_OPENSKY_SENSOR_ID" => Some("201".into()),
            _ => None,
        });
        assert!(config.enabled);
        assert!((config.lat_min - 48.0).abs() < 1e-12);
        assert!((config.lat_max - 54.0).abs() < 1e-12);
        assert_eq!(config.poll_interval_secs, 5);
        assert_eq!(config.username.as_deref(), Some("alice"));
        assert_eq!(config.password.as_deref(), Some("s3cret"));
        assert_eq!(config.sensor_id, SensorId(201));
    }

    /// Garbage values fall back to defaults rather than crashing.
    #[test]
    fn invalid_values_fall_back_to_defaults() {
        let config = OpenSkyConfig::from_lookup(|key| match key {
            "FIREFLY_OPENSKY_LAT_MIN" => Some("not-a-number".into()),
            "FIREFLY_OPENSKY_POLL_INTERVAL_SECS" => Some("0".into()), // zero → rejected
            _ => None,
        });
        let d = OpenSkyConfig::default();
        assert!((config.lat_min - d.lat_min).abs() < 1e-12);
        assert_eq!(config.poll_interval_secs, d.poll_interval_secs);
    }

    /// The enabled flag accepts common truthy spellings.
    #[test]
    fn enabled_flag_accepts_common_spellings() {
        for truthy in ["1", "true", "TRUE", "Yes"] {
            let c = OpenSkyConfig::from_lookup(|k| {
                (k == "FIREFLY_OPENSKY_ENABLED").then(|| truthy.to_string())
            });
            assert!(c.enabled, "{truthy:?} should enable the adapter");
        }
        for falsy in ["0", "false", "off", ""] {
            let c = OpenSkyConfig::from_lookup(|k| {
                (k == "FIREFLY_OPENSKY_ENABLED").then(|| falsy.to_string())
            });
            assert!(!c.enabled, "{falsy:?} should leave the adapter off");
        }
    }
}
