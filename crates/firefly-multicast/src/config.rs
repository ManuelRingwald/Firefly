//! 12-factor configuration for the CAT062 multicast sender.
//!
//! Everything the sender needs comes from environment variables (ADR 0003,
//! 12-factor), so it runs with no flags and no config file. Parsing is split
//! from the environment lookup so it can be tested without touching the real
//! process environment, exactly like [`ServerConfig`](../../firefly_server/config).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use firefly_asterix::{Cat065Encoder, DataSourceId};
use firefly_geo::Wgs84;

/// Configuration of the CAT062 multicast feed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MulticastConfig {
    /// Whether to emit the feed at all. `FIREFLY_CAT062_ENABLED`, default
    /// `false` — a demo should not blast UDP onto the network unless asked.
    pub enabled: bool,
    /// Multicast group to send to. `FIREFLY_CAT062_GROUP`, default
    /// `239.255.0.62` (administratively-scoped, site-local; `.62` nods to
    /// CAT062). Consumers (ASD, EFS, recorder) join this group to listen.
    pub group: Ipv4Addr,
    /// UDP port. `FIREFLY_CAT062_PORT`, default `8600`.
    pub port: u16,
    /// System Area Code stamped into I062/010. `FIREFLY_CAT062_SAC`, default `25`.
    pub sac: u8,
    /// System Identification Code stamped into I062/010. `FIREFLY_CAT062_SIC`,
    /// default `2`.
    pub sic: u8,
    /// The system reference point that I062/100 (system-stereographic X/Y) is
    /// measured from. `FIREFLY_CAT062_REF_LAT` / `FIREFLY_CAT062_REF_LON` in
    /// degrees, default `48.0, 11.0` (the demo scene origin).
    pub reference_point: Wgs84,
    /// Whether to emit the CAT065 SDPS-status heartbeat alongside the tracks
    /// (ADR 0018). `FIREFLY_CAT065_ENABLED`, default `true` — when the feed is
    /// on, a consumer should be able to tell "alive but empty" from "dead".
    /// Only takes effect when [`enabled`](Self::enabled) is also set.
    pub heartbeat_enabled: bool,
    /// Heartbeat period in **wall-clock** seconds. `FIREFLY_CAT065_PERIOD`,
    /// default `1.0` (the de-facto CAT065 status rate). The heartbeat is a
    /// real-time liveness signal, so it is paced by the wall clock, not by
    /// data-time like the track feed.
    pub heartbeat_period_secs: f64,
    /// Service identification stamped into I065/015. `FIREFLY_CAT065_SERVICE_ID`,
    /// default `1`.
    pub service_id: u8,
}

impl Default for MulticastConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            group: Ipv4Addr::new(239, 255, 0, 62),
            port: 8600,
            sac: 25,
            sic: 2,
            reference_point: Wgs84::from_degrees(48.0, 11.0, 0.0),
            heartbeat_enabled: true,
            heartbeat_period_secs: 1.0,
            service_id: 1,
        }
    }
}

impl MulticastConfig {
    /// Read configuration from the process environment.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Read configuration from an arbitrary key→value lookup.
    ///
    /// Unset or unparseable values fall back to the defaults — a misconfigured
    /// variable should not silently send garbage or crash the demo.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let default = Self::default();
        let enabled = get("FIREFLY_CAT062_ENABLED")
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(default.enabled);
        let group = get("FIREFLY_CAT062_GROUP")
            .and_then(|v| v.parse().ok())
            .filter(Ipv4Addr::is_multicast)
            .unwrap_or(default.group);
        let port = get("FIREFLY_CAT062_PORT")
            .and_then(|v| v.parse().ok())
            .unwrap_or(default.port);
        let sac = get("FIREFLY_CAT062_SAC")
            .and_then(|v| v.parse().ok())
            .unwrap_or(default.sac);
        let sic = get("FIREFLY_CAT062_SIC")
            .and_then(|v| v.parse().ok())
            .unwrap_or(default.sic);
        let ref_lat = get("FIREFLY_CAT062_REF_LAT")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite());
        let ref_lon = get("FIREFLY_CAT062_REF_LON")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite());
        // Only rebuild the reference point when an override is actually given,
        // so the untouched default stays bit-for-bit exact (no degree↔radian
        // round-trip rounding).
        let reference_point = match (ref_lat, ref_lon) {
            (None, None) => default.reference_point,
            (lat, lon) => Wgs84::from_degrees(
                lat.unwrap_or(default.reference_point.lat_deg()),
                lon.unwrap_or(default.reference_point.lon_deg()),
                0.0,
            ),
        };
        let heartbeat_enabled = get("FIREFLY_CAT065_ENABLED")
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(default.heartbeat_enabled);
        let heartbeat_period_secs = get("FIREFLY_CAT065_PERIOD")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(default.heartbeat_period_secs);
        let service_id = get("FIREFLY_CAT065_SERVICE_ID")
            .and_then(|v| v.parse().ok())
            .unwrap_or(default.service_id);
        Self {
            enabled,
            group,
            port,
            sac,
            sic,
            reference_point,
            heartbeat_enabled,
            heartbeat_period_secs,
            service_id,
        }
    }

    /// The socket address datagrams are sent to: the multicast group and port.
    pub fn destination(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(self.group), self.port)
    }

    /// The data-source identifier (SAC/SIC) for I062/010.
    pub fn data_source(&self) -> DataSourceId {
        DataSourceId::new(self.sac, self.sic)
    }

    /// A CAT065 heartbeat encoder using this config's data source and service id.
    pub fn cat065_encoder(&self) -> Cat065Encoder {
        Cat065Encoder::new(self.data_source(), self.service_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With nothing set, the defaults apply (feed off, site-local group).
    #[test]
    fn empty_environment_yields_defaults() {
        let config = MulticastConfig::from_lookup(|_| None);
        assert_eq!(config, MulticastConfig::default());
        assert!(!config.enabled, "the feed is off unless explicitly enabled");
        assert!(config.group.is_multicast());
    }

    /// Valid values are parsed, including the boolean and the reference point.
    #[test]
    fn valid_values_are_parsed() {
        let config = MulticastConfig::from_lookup(|key| match key {
            "FIREFLY_CAT062_ENABLED" => Some("true".to_string()),
            "FIREFLY_CAT062_GROUP" => Some("239.1.2.3".to_string()),
            "FIREFLY_CAT062_PORT" => Some("9000".to_string()),
            "FIREFLY_CAT062_SAC" => Some("16".to_string()),
            "FIREFLY_CAT062_SIC" => Some("32".to_string()),
            "FIREFLY_CAT062_REF_LAT" => Some("45.0".to_string()),
            "FIREFLY_CAT062_REF_LON" => Some("11.25".to_string()),
            _ => None,
        });
        assert!(config.enabled);
        assert_eq!(config.group, Ipv4Addr::new(239, 1, 2, 3));
        assert_eq!(config.port, 9000);
        assert_eq!(config.sac, 16);
        assert_eq!(config.sic, 32);
        assert!((config.reference_point.lat_deg() - 45.0).abs() < 1e-12);
        assert!((config.reference_point.lon_deg() - 11.25).abs() < 1e-12);
        assert_eq!(config.destination(), "239.1.2.3:9000".parse().unwrap());
        assert_eq!(config.data_source(), DataSourceId::new(16, 32));
    }

    /// Garbage values fall back to the defaults rather than crashing, and a
    /// non-multicast group is rejected (you cannot accidentally unicast-blast a
    /// single host by typo).
    #[test]
    fn invalid_values_fall_back_to_defaults() {
        let config = MulticastConfig::from_lookup(|key| match key {
            "FIREFLY_CAT062_GROUP" => Some("8.8.8.8".to_string()), // not multicast
            "FIREFLY_CAT062_PORT" => Some("not-a-port".to_string()),
            "FIREFLY_CAT062_REF_LAT" => Some("nonsense".to_string()),
            _ => None,
        });
        let default = MulticastConfig::default();
        assert_eq!(config.group, default.group, "non-multicast group rejected");
        assert_eq!(config.port, default.port);
        assert_eq!(
            config.reference_point.lat_deg(),
            default.reference_point.lat_deg()
        );
    }

    /// The heartbeat is on by default and configurable; a non-positive or
    /// garbage period falls back to the default rate.
    #[test]
    fn heartbeat_config_is_parsed_with_safe_defaults() {
        let default = MulticastConfig::default();
        assert!(default.heartbeat_enabled, "heartbeat on by default");
        assert_eq!(default.heartbeat_period_secs, 1.0);
        assert_eq!(default.service_id, 1);

        let custom = MulticastConfig::from_lookup(|key| match key {
            "FIREFLY_CAT065_ENABLED" => Some("false".to_string()),
            "FIREFLY_CAT065_PERIOD" => Some("2.5".to_string()),
            "FIREFLY_CAT065_SERVICE_ID" => Some("7".to_string()),
            _ => None,
        });
        assert!(!custom.heartbeat_enabled);
        assert_eq!(custom.heartbeat_period_secs, 2.5);
        assert_eq!(custom.service_id, 7);

        let garbage = MulticastConfig::from_lookup(|key| match key {
            "FIREFLY_CAT065_PERIOD" => Some("-1".to_string()),
            _ => None,
        });
        assert_eq!(garbage.heartbeat_period_secs, default.heartbeat_period_secs);
    }

    /// The enabled flag accepts the common truthy spellings and treats anything
    /// else as off.
    #[test]
    fn enabled_flag_accepts_common_spellings() {
        for truthy in ["1", "true", "TRUE", "Yes", " yes "] {
            let c = MulticastConfig::from_lookup(|k| {
                (k == "FIREFLY_CAT062_ENABLED").then(|| truthy.to_string())
            });
            assert!(c.enabled, "{truthy:?} should enable the feed");
        }
        for falsy in ["0", "false", "off", ""] {
            let c = MulticastConfig::from_lookup(|k| {
                (k == "FIREFLY_CAT062_ENABLED").then(|| falsy.to_string())
            });
            assert!(!c.enabled, "{falsy:?} should leave the feed off");
        }
    }
}
