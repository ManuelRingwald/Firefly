//! 12-factor configuration for the FLARM/OGN adapter (ADR 0026, ADR 0003).
//!
//! Mirrors [`OpenSkyConfig`](firefly_opensky) in spirit: every setting comes from
//! an environment variable so a standalone instance can be tuned without code
//! changes. In the orchestrated path (ADR 0023) the equivalent `FlarmConfig` is
//! built from a `FIREFLY_SOURCES` entry instead — that wiring is Schritt C and
//! lives in `firefly-server`; this crate only owns the config *shape* and the
//! env fallback for standalone/dev use.

use firefly_core::SensorId;

/// Default APRS-IS host for the Open Glider Network (a DNS rotation).
pub const DEFAULT_SERVER: &str = "aprs.glidernet.org";
/// Default APRS-IS port for a user-defined server-side filter feed.
pub const DEFAULT_PORT: u16 = 14580;
/// Default 1σ horizontal position accuracy, metres (ADR 0026 §5): FLARM GPS is
/// good, but the APRS position encoding is rounded (~18 m base precision), so a
/// slightly conservative isotropic σ is honest.
pub const DEFAULT_SIGMA_POS_M: f64 = 20.0;

/// Configuration for the FLARM/OGN APRS-IS listener.
#[derive(Debug, Clone, PartialEq)]
pub struct FlarmConfig {
    /// Whether to start listening at all. `FIREFLY_FLARM_ENABLED`, default `false`.
    pub enabled: bool,
    /// Southern latitude bound of the area of interest, degrees.
    /// `FIREFLY_FLARM_LAT_MIN`, default `47.0`.
    pub lat_min: f64,
    /// Northern latitude bound, degrees. `FIREFLY_FLARM_LAT_MAX`, default `55.0`.
    pub lat_max: f64,
    /// Western longitude bound, degrees. `FIREFLY_FLARM_LON_MIN`, default `5.0`.
    pub lon_min: f64,
    /// Eastern longitude bound, degrees. `FIREFLY_FLARM_LON_MAX`, default `16.0`.
    pub lon_max: f64,
    /// APRS-IS server host. `FIREFLY_FLARM_SERVER`, default [`DEFAULT_SERVER`].
    pub server: String,
    /// APRS-IS server port. `FIREFLY_FLARM_PORT`, default [`DEFAULT_PORT`].
    pub port: u16,
    /// APRS-IS login callsign. `FIREFLY_FLARM_CALLSIGN`. Absent → a read-only
    /// pseudo-callsign is used (we never transmit; ADR 0026 §2).
    pub callsign: Option<String>,
    /// APRS-IS passcode. `FIREFLY_FLARM_PASSCODE`. Absent → `-1` (read-only,
    /// anonymous). A real passcode is only needed for a named account.
    pub passcode: Option<i32>,
    /// Application name announced in the login line (APRS-IS `vers`).
    pub app_name: String,
    /// Application version announced in the login line.
    pub app_version: String,
    /// [`SensorId`] stamped onto every FLARM plot. `FIREFLY_FLARM_SENSOR_ID`,
    /// default `210` (distinct from the OpenSky adapter's `200`).
    pub sensor_id: SensorId,
    /// 1σ horizontal position accuracy in metres for the geodetic measurement.
    /// `FIREFLY_FLARM_SIGMA_M`, default [`DEFAULT_SIGMA_POS_M`].
    pub sigma_pos_m: f64,
    /// Minimum reconnect backoff, seconds. `FIREFLY_FLARM_RECONNECT_MIN_SECS`,
    /// default `5`.
    pub reconnect_min_secs: u64,
    /// Maximum reconnect backoff, seconds (exponential cap).
    /// `FIREFLY_FLARM_RECONNECT_MAX_SECS`, default `300`.
    pub reconnect_max_secs: u64,
}

impl Default for FlarmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            lat_min: 47.0,
            lat_max: 55.0,
            lon_min: 5.0,
            lon_max: 16.0,
            server: DEFAULT_SERVER.to_string(),
            port: DEFAULT_PORT,
            callsign: None,
            passcode: None,
            app_name: "firefly-flarm".to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            sensor_id: SensorId(210),
            sigma_pos_m: DEFAULT_SIGMA_POS_M,
            reconnect_min_secs: 5,
            reconnect_max_secs: 300,
        }
    }
}

impl FlarmConfig {
    /// Read configuration from the process environment.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Read configuration from an arbitrary key→value lookup (testable without
    /// touching the real process environment). Unset or unparseable values fall
    /// back to the defaults.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let d = Self::default();
        let flag = |key: &str, default: bool| {
            get(key)
                .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
                .unwrap_or(default)
        };
        let degrees = |key: &str, default: f64| {
            get(key)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(default)
        };
        Self {
            enabled: flag("FIREFLY_FLARM_ENABLED", d.enabled),
            lat_min: degrees("FIREFLY_FLARM_LAT_MIN", d.lat_min),
            lat_max: degrees("FIREFLY_FLARM_LAT_MAX", d.lat_max),
            lon_min: degrees("FIREFLY_FLARM_LON_MIN", d.lon_min),
            lon_max: degrees("FIREFLY_FLARM_LON_MAX", d.lon_max),
            server: get("FIREFLY_FLARM_SERVER")
                .filter(|s| !s.is_empty())
                .unwrap_or(d.server),
            port: get("FIREFLY_FLARM_PORT")
                .and_then(|v| v.parse::<u16>().ok())
                .filter(|&p| p > 0)
                .unwrap_or(d.port),
            callsign: get("FIREFLY_FLARM_CALLSIGN").filter(|s| !s.is_empty()),
            passcode: get("FIREFLY_FLARM_PASSCODE").and_then(|v| v.trim().parse::<i32>().ok()),
            app_name: d.app_name,
            app_version: d.app_version,
            sensor_id: get("FIREFLY_FLARM_SENSOR_ID")
                .and_then(|v| v.parse::<u16>().ok())
                .map(SensorId)
                .unwrap_or(d.sensor_id),
            sigma_pos_m: get("FIREFLY_FLARM_SIGMA_M")
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(d.sigma_pos_m),
            reconnect_min_secs: get("FIREFLY_FLARM_RECONNECT_MIN_SECS")
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|&s| s > 0)
                .unwrap_or(d.reconnect_min_secs),
            reconnect_max_secs: get("FIREFLY_FLARM_RECONNECT_MAX_SECS")
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|&s| s > 0)
                .unwrap_or(d.reconnect_max_secs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_environment_yields_defaults() {
        let config = FlarmConfig::from_lookup(|_| None);
        assert_eq!(config, FlarmConfig::default());
        assert!(!config.enabled, "adapter is off unless explicitly enabled");
        assert_eq!(config.port, DEFAULT_PORT);
        assert!(config.callsign.is_none(), "anonymous read-only by default");
    }

    #[test]
    fn valid_values_are_parsed() {
        let config = FlarmConfig::from_lookup(|key| match key {
            "FIREFLY_FLARM_ENABLED" => Some("true".into()),
            "FIREFLY_FLARM_LAT_MIN" => Some("48.0".into()),
            "FIREFLY_FLARM_LON_MAX" => Some("9.5".into()),
            "FIREFLY_FLARM_SERVER" => Some("aprs.example.org".into()),
            "FIREFLY_FLARM_PORT" => Some("14501".into()),
            "FIREFLY_FLARM_CALLSIGN" => Some("EDXY".into()),
            "FIREFLY_FLARM_PASSCODE" => Some("12345".into()),
            "FIREFLY_FLARM_SENSOR_ID" => Some("211".into()),
            "FIREFLY_FLARM_SIGMA_M" => Some("15".into()),
            _ => None,
        });
        assert!(config.enabled);
        assert!((config.lat_min - 48.0).abs() < 1e-12);
        assert!((config.lon_max - 9.5).abs() < 1e-12);
        assert_eq!(config.server, "aprs.example.org");
        assert_eq!(config.port, 14501);
        assert_eq!(config.callsign.as_deref(), Some("EDXY"));
        assert_eq!(config.passcode, Some(12345));
        assert_eq!(config.sensor_id, SensorId(211));
        assert!((config.sigma_pos_m - 15.0).abs() < 1e-12);
    }

    #[test]
    fn invalid_values_fall_back_to_defaults() {
        let config = FlarmConfig::from_lookup(|key| match key {
            "FIREFLY_FLARM_LAT_MIN" => Some("not-a-number".into()),
            "FIREFLY_FLARM_PORT" => Some("0".into()), // zero → rejected
            "FIREFLY_FLARM_SIGMA_M" => Some("-3".into()), // non-positive → rejected
            _ => None,
        });
        let d = FlarmConfig::default();
        assert!((config.lat_min - d.lat_min).abs() < 1e-12);
        assert_eq!(config.port, d.port);
        assert!((config.sigma_pos_m - d.sigma_pos_m).abs() < 1e-12);
    }

    #[test]
    fn passcode_negative_one_is_accepted() {
        let config =
            FlarmConfig::from_lookup(|k| (k == "FIREFLY_FLARM_PASSCODE").then(|| "-1".into()));
        assert_eq!(config.passcode, Some(-1));
    }
}
