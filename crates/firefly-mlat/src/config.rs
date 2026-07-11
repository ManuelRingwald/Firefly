//! 12-factor configuration for the WAM/MLAT adapter (FEP.5, ADR 0003).
//! Mirrors [`Adsb021Config`](firefly_adsb021) in spirit; like the ADS-B
//! ground station there is **no site position** — CAT020 positions are
//! geodetic (computed by the MLAT system in WGS-84), the system's location
//! is irrelevant to the measurement.

use std::net::Ipv4Addr;

use firefly_core::SensorId;

/// Default UDP port the CAT020/019 feed is expected on (no IANA assignment
/// for raw ASTERIX; a sensible, documented default next to the radar's 8048
/// and the ADS-B station's 8021).
pub const DEFAULT_PORT: u16 = 8020;
/// Nominal update interval per aircraft, seconds — WAM systems typically
/// update once per second, but the CAT063 staleness window keys off a
/// conservative per-*system* liveness (CAT019 status arrives periodically
/// regardless of traffic).
pub const NOMINAL_UPDATE_SECS: f64 = 5.0;

/// Configuration for one WAM/MLAT (CAT020/019) input listener.
#[derive(Debug, Clone, PartialEq)]
pub struct MlatConfig {
    /// Whether to start listening at all. `FIREFLY_MLAT_ENABLED`,
    /// default `false`.
    pub enabled: bool,
    /// System Area Code of the expected MLAT system (I020/010).
    /// `FIREFLY_MLAT_SAC`.
    pub sac: u8,
    /// System Identification Code of the expected MLAT system.
    /// `FIREFLY_MLAT_SIC`.
    pub sic: u8,
    /// [`SensorId`] stamped onto every plot. `FIREFLY_MLAT_SENSOR_ID`,
    /// default `240` (distinct from OpenSky `200`, FLARM `210`, radar `220`,
    /// ADS-B station `230`).
    pub sensor_id: SensorId,
    /// Address to listen on. A multicast group (`224.0.0.0/4`) is **joined**;
    /// any other address is a plain unicast bind. `FIREFLY_MLAT_GROUP`,
    /// default `0.0.0.0`.
    pub listen_group: Ipv4Addr,
    /// UDP port to receive ASTERIX on. `FIREFLY_MLAT_PORT`,
    /// default [`DEFAULT_PORT`].
    pub listen_port: u16,
}

impl Default for MlatConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sac: 0,
            sic: 0,
            sensor_id: SensorId(240),
            listen_group: Ipv4Addr::UNSPECIFIED,
            listen_port: DEFAULT_PORT,
        }
    }
}

impl MlatConfig {
    /// Whether [`listen_group`](Self::listen_group) is a multicast group (and
    /// so should be *joined*), versus a unicast bind address.
    pub fn is_multicast(&self) -> bool {
        self.listen_group.is_multicast()
    }

    /// Read configuration from the process environment.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Read configuration from an arbitrary key→value lookup (testable
    /// without touching the process environment). Unset or unparseable
    /// values fall back to the defaults.
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let d = Self::default();
        let byte = |key: &str, default: u8| {
            get(key)
                .and_then(|v| v.trim().parse::<u8>().ok())
                .unwrap_or(default)
        };
        Self {
            enabled: get("FIREFLY_MLAT_ENABLED")
                .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
                .unwrap_or(d.enabled),
            sac: byte("FIREFLY_MLAT_SAC", d.sac),
            sic: byte("FIREFLY_MLAT_SIC", d.sic),
            sensor_id: get("FIREFLY_MLAT_SENSOR_ID")
                .and_then(|v| v.parse::<u16>().ok())
                .map(SensorId)
                .unwrap_or(d.sensor_id),
            listen_group: get("FIREFLY_MLAT_GROUP")
                .and_then(|v| v.trim().parse::<Ipv4Addr>().ok())
                .unwrap_or(d.listen_group),
            listen_port: get("FIREFLY_MLAT_PORT")
                .and_then(|v| v.parse::<u16>().ok())
                .filter(|&p| p > 0)
                .unwrap_or(d.listen_port),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Off by default; defaults are the documented ones. REQ: FR-NET-017
    #[test]
    fn empty_environment_yields_defaults() {
        let config = MlatConfig::from_lookup(|_| None);
        assert_eq!(config, MlatConfig::default());
        assert!(!config.enabled);
        assert_eq!(config.sensor_id, SensorId(240));
        assert_eq!(config.listen_port, DEFAULT_PORT);
        assert!(!config.is_multicast());
    }

    /// Valid values parse; garbage falls back per field. REQ: FR-NET-017
    #[test]
    fn values_parse_and_garbage_falls_back() {
        let config = MlatConfig::from_lookup(|key| match key {
            "FIREFLY_MLAT_ENABLED" => Some("true".into()),
            "FIREFLY_MLAT_SAC" => Some("25".into()),
            "FIREFLY_MLAT_SIC" => Some("40".into()),
            "FIREFLY_MLAT_SENSOR_ID" => Some("241".into()),
            "FIREFLY_MLAT_GROUP" => Some("239.255.0.20".into()),
            "FIREFLY_MLAT_PORT" => Some("9020".into()),
            _ => None,
        });
        assert!(config.enabled);
        assert_eq!((config.sac, config.sic), (25, 40));
        assert_eq!(config.sensor_id, SensorId(241));
        assert!(config.is_multicast());
        assert_eq!(config.listen_port, 9020);

        let bad = MlatConfig::from_lookup(|key| match key {
            "FIREFLY_MLAT_PORT" => Some("0".into()),
            "FIREFLY_MLAT_GROUP" => Some("garbage".into()),
            _ => None,
        });
        assert_eq!(bad.listen_port, DEFAULT_PORT);
        assert_eq!(bad.listen_group, Ipv4Addr::UNSPECIFIED);
    }
}
