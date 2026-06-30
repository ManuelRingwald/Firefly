//! 12-factor configuration for the radar ASTERIX adapter (ADR 0028, ADR 0003).
//!
//! Mirrors [`FlarmConfig`](firefly_flarm) / `OpenSkyConfig` in spirit: every
//! setting comes from an environment variable so a standalone instance can be
//! tuned without code changes. In the orchestrated path (ADR 0023) the
//! equivalent `RadarConfig` is built from a `FIREFLY_SOURCES` entry instead —
//! that wiring lives in `firefly-server`; this crate owns the config *shape* and
//! the env fallback for standalone/dev use.
//!
//! Unlike the geodetic adapters, a radar source must carry the **radar site
//! position** (`lat`/`lon`/`height_m`): CAT048 reports are polar **relative to
//! the radar** and the datagram never carries the site, so the tracker needs it
//! to lift polar plots into the tracking frame (ADR 0028 §4).

use std::net::Ipv4Addr;

use firefly_core::SensorId;

/// Default UDP port the radar feed is expected on (no IANA-assigned port for
/// raw ASTERIX; a sensible, documented default).
pub const DEFAULT_PORT: u16 = 8048;
/// Default antenna revolution (scan) period, seconds — a typical en-route radar.
pub const DEFAULT_SCAN_PERIOD_SECS: f64 = 4.0;
/// Default 1σ slant-range measurement noise, metres.
pub const DEFAULT_SIGMA_RANGE_M: f64 = 50.0;
/// Default 1σ azimuth measurement noise, degrees.
pub const DEFAULT_SIGMA_AZIMUTH_DEG: f64 = 0.1;

/// Configuration for one radar ASTERIX (CAT048) input listener.
#[derive(Debug, Clone, PartialEq)]
pub struct RadarConfig {
    /// Whether to start listening at all. `FIREFLY_RADAR_ENABLED`, default `false`.
    pub enabled: bool,
    /// System Area Code of the expected radar (I048/010). `FIREFLY_RADAR_SAC`.
    pub sac: u8,
    /// System Identification Code of the expected radar. `FIREFLY_RADAR_SIC`.
    pub sic: u8,
    /// [`SensorId`] stamped onto every radar plot. `FIREFLY_RADAR_SENSOR_ID`,
    /// default `220` (distinct from OpenSky `200` and FLARM `210`).
    pub sensor_id: SensorId,
    /// Radar site latitude, degrees. `FIREFLY_RADAR_LAT`.
    pub lat_deg: f64,
    /// Radar site longitude, degrees. `FIREFLY_RADAR_LON`.
    pub lon_deg: f64,
    /// Radar site height above the WGS84 ellipsoid, metres. `FIREFLY_RADAR_HEIGHT_M`,
    /// default `0`.
    pub height_m: f64,
    /// Address to listen on. A multicast group (`224.0.0.0/4`) is **joined**;
    /// any other address is treated as a plain unicast bind. `FIREFLY_RADAR_GROUP`,
    /// default `0.0.0.0` (unicast on all interfaces).
    pub listen_group: Ipv4Addr,
    /// UDP port to receive ASTERIX on. `FIREFLY_RADAR_PORT`, default [`DEFAULT_PORT`].
    pub listen_port: u16,
    /// Antenna revolution period, seconds (drives the tracker's revisit budget
    /// and the CAT063 staleness window). `FIREFLY_RADAR_SCAN_SECS`,
    /// default [`DEFAULT_SCAN_PERIOD_SECS`].
    pub scan_period_secs: f64,
    /// Assumed 1σ slant-range noise, metres. `FIREFLY_RADAR_SIGMA_RANGE_M`,
    /// default [`DEFAULT_SIGMA_RANGE_M`].
    pub sigma_range_m: f64,
    /// Assumed 1σ azimuth noise, degrees. `FIREFLY_RADAR_SIGMA_AZ_DEG`,
    /// default [`DEFAULT_SIGMA_AZIMUTH_DEG`].
    pub sigma_azimuth_deg: f64,
}

impl Default for RadarConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sac: 0,
            sic: 0,
            sensor_id: SensorId(220),
            lat_deg: 0.0,
            lon_deg: 0.0,
            height_m: 0.0,
            listen_group: Ipv4Addr::UNSPECIFIED,
            listen_port: DEFAULT_PORT,
            scan_period_secs: DEFAULT_SCAN_PERIOD_SECS,
            sigma_range_m: DEFAULT_SIGMA_RANGE_M,
            sigma_azimuth_deg: DEFAULT_SIGMA_AZIMUTH_DEG,
        }
    }
}

impl RadarConfig {
    /// Whether [`listen_group`](Self::listen_group) is a multicast group (and so
    /// should be *joined*), versus a unicast bind address.
    pub fn is_multicast(&self) -> bool {
        self.listen_group.is_multicast()
    }

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
        let byte = |key: &str, default: u8| {
            get(key)
                .and_then(|v| v.trim().parse::<u8>().ok())
                .unwrap_or(default)
        };
        let real = |key: &str, default: f64| {
            get(key)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(default)
        };
        let positive = |key: &str, default: f64| {
            get(key)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(default)
        };
        Self {
            enabled: flag("FIREFLY_RADAR_ENABLED", d.enabled),
            sac: byte("FIREFLY_RADAR_SAC", d.sac),
            sic: byte("FIREFLY_RADAR_SIC", d.sic),
            sensor_id: get("FIREFLY_RADAR_SENSOR_ID")
                .and_then(|v| v.parse::<u16>().ok())
                .map(SensorId)
                .unwrap_or(d.sensor_id),
            lat_deg: real("FIREFLY_RADAR_LAT", d.lat_deg),
            lon_deg: real("FIREFLY_RADAR_LON", d.lon_deg),
            height_m: real("FIREFLY_RADAR_HEIGHT_M", d.height_m),
            listen_group: get("FIREFLY_RADAR_GROUP")
                .and_then(|v| v.trim().parse::<Ipv4Addr>().ok())
                .unwrap_or(d.listen_group),
            listen_port: get("FIREFLY_RADAR_PORT")
                .and_then(|v| v.parse::<u16>().ok())
                .filter(|&p| p > 0)
                .unwrap_or(d.listen_port),
            scan_period_secs: positive("FIREFLY_RADAR_SCAN_SECS", d.scan_period_secs),
            sigma_range_m: positive("FIREFLY_RADAR_SIGMA_RANGE_M", d.sigma_range_m),
            sigma_azimuth_deg: positive("FIREFLY_RADAR_SIGMA_AZ_DEG", d.sigma_azimuth_deg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_environment_yields_defaults() {
        let config = RadarConfig::from_lookup(|_| None);
        assert_eq!(config, RadarConfig::default());
        assert!(!config.enabled, "adapter is off unless explicitly enabled");
        assert_eq!(config.sensor_id, SensorId(220));
        assert_eq!(config.listen_port, DEFAULT_PORT);
        assert!(!config.is_multicast(), "0.0.0.0 is a unicast bind");
    }

    #[test]
    fn valid_values_are_parsed() {
        let config = RadarConfig::from_lookup(|key| match key {
            "FIREFLY_RADAR_ENABLED" => Some("true".into()),
            "FIREFLY_RADAR_SAC" => Some("1".into()),
            "FIREFLY_RADAR_SIC" => Some("4".into()),
            "FIREFLY_RADAR_SENSOR_ID" => Some("221".into()),
            "FIREFLY_RADAR_LAT" => Some("50.03".into()),
            "FIREFLY_RADAR_LON" => Some("8.57".into()),
            "FIREFLY_RADAR_HEIGHT_M" => Some("111.0".into()),
            "FIREFLY_RADAR_GROUP" => Some("239.255.0.48".into()),
            "FIREFLY_RADAR_PORT" => Some("8048".into()),
            "FIREFLY_RADAR_SCAN_SECS" => Some("5".into()),
            "FIREFLY_RADAR_SIGMA_RANGE_M" => Some("70".into()),
            "FIREFLY_RADAR_SIGMA_AZ_DEG" => Some("0.08".into()),
            _ => None,
        });
        assert!(config.enabled);
        assert_eq!((config.sac, config.sic), (1, 4));
        assert_eq!(config.sensor_id, SensorId(221));
        assert!((config.lat_deg - 50.03).abs() < 1e-12);
        assert!((config.lon_deg - 8.57).abs() < 1e-12);
        assert_eq!(config.listen_group, Ipv4Addr::new(239, 255, 0, 48));
        assert!(config.is_multicast(), "239.x is a multicast group → joined");
        assert!((config.scan_period_secs - 5.0).abs() < 1e-12);
        assert!((config.sigma_azimuth_deg - 0.08).abs() < 1e-12);
    }

    #[test]
    fn invalid_values_fall_back_to_defaults() {
        let config = RadarConfig::from_lookup(|key| match key {
            "FIREFLY_RADAR_LAT" => Some("not-a-number".into()),
            "FIREFLY_RADAR_PORT" => Some("0".into()), // zero → rejected
            "FIREFLY_RADAR_SCAN_SECS" => Some("-2".into()), // non-positive → rejected
            "FIREFLY_RADAR_GROUP" => Some("garbage".into()), // unparseable → default
            _ => None,
        });
        let d = RadarConfig::default();
        assert!((config.lat_deg - d.lat_deg).abs() < 1e-12);
        assert_eq!(config.listen_port, d.listen_port);
        assert!((config.scan_period_secs - d.scan_period_secs).abs() < 1e-12);
        assert_eq!(config.listen_group, d.listen_group);
    }
}
