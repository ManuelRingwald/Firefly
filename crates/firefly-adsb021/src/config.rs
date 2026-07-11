//! 12-factor configuration for the ADS-B ground-station adapter (FEP.3,
//! ADR 0003). Mirrors [`RadarConfig`](firefly_radar) in spirit; unlike the
//! radar there is **no site position** — CAT021 reports are geodetic
//! self-reports, the station's location is irrelevant to the measurement.

use std::net::Ipv4Addr;

use firefly_core::SensorId;

/// Default UDP port the CAT021 feed is expected on (no IANA assignment for
/// raw ASTERIX; a sensible, documented default next to the radar's 8048).
pub const DEFAULT_PORT: u16 = 8021;
/// Nominal update interval per aircraft, seconds — 1090ES position squitters
/// arrive roughly twice a second, but the CAT063 staleness window keys off a
/// conservative per-*station* liveness, not per aircraft.
pub const NOMINAL_UPDATE_SECS: f64 = 5.0;

/// Configuration for one ADS-B ground-station (CAT021) input listener.
#[derive(Debug, Clone, PartialEq)]
pub struct Adsb021Config {
    /// Whether to start listening at all. `FIREFLY_ADSB021_ENABLED`,
    /// default `false`.
    pub enabled: bool,
    /// System Area Code of the expected station (I021/010).
    /// `FIREFLY_ADSB021_SAC`.
    pub sac: u8,
    /// System Identification Code of the expected station.
    /// `FIREFLY_ADSB021_SIC`.
    pub sic: u8,
    /// [`SensorId`] stamped onto every plot. `FIREFLY_ADSB021_SENSOR_ID`,
    /// default `230` (distinct from OpenSky `200`, FLARM `210`, radar `220`).
    pub sensor_id: SensorId,
    /// Address to listen on. A multicast group (`224.0.0.0/4`) is **joined**;
    /// any other address is a plain unicast bind. `FIREFLY_ADSB021_GROUP`,
    /// default `0.0.0.0`.
    pub listen_group: Ipv4Addr,
    /// UDP port to receive ASTERIX on. `FIREFLY_ADSB021_PORT`,
    /// default [`DEFAULT_PORT`].
    pub listen_port: u16,
}

impl Default for Adsb021Config {
    fn default() -> Self {
        Self {
            enabled: false,
            sac: 0,
            sic: 0,
            sensor_id: SensorId(230),
            listen_group: Ipv4Addr::UNSPECIFIED,
            listen_port: DEFAULT_PORT,
        }
    }
}

impl Adsb021Config {
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
            enabled: get("FIREFLY_ADSB021_ENABLED")
                .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
                .unwrap_or(d.enabled),
            sac: byte("FIREFLY_ADSB021_SAC", d.sac),
            sic: byte("FIREFLY_ADSB021_SIC", d.sic),
            sensor_id: get("FIREFLY_ADSB021_SENSOR_ID")
                .and_then(|v| v.parse::<u16>().ok())
                .map(SensorId)
                .unwrap_or(d.sensor_id),
            listen_group: get("FIREFLY_ADSB021_GROUP")
                .and_then(|v| v.trim().parse::<Ipv4Addr>().ok())
                .unwrap_or(d.listen_group),
            listen_port: get("FIREFLY_ADSB021_PORT")
                .and_then(|v| v.parse::<u16>().ok())
                .filter(|&p| p > 0)
                .unwrap_or(d.listen_port),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Off by default; defaults are the documented ones. REQ: FR-NET-015
    #[test]
    fn empty_environment_yields_defaults() {
        let config = Adsb021Config::from_lookup(|_| None);
        assert_eq!(config, Adsb021Config::default());
        assert!(!config.enabled);
        assert_eq!(config.sensor_id, SensorId(230));
        assert_eq!(config.listen_port, DEFAULT_PORT);
        assert!(!config.is_multicast());
    }

    /// Valid values parse; garbage falls back per field. REQ: FR-NET-015
    #[test]
    fn values_parse_and_garbage_falls_back() {
        let config = Adsb021Config::from_lookup(|key| match key {
            "FIREFLY_ADSB021_ENABLED" => Some("true".into()),
            "FIREFLY_ADSB021_SAC" => Some("25".into()),
            "FIREFLY_ADSB021_SIC" => Some("30".into()),
            "FIREFLY_ADSB021_SENSOR_ID" => Some("231".into()),
            "FIREFLY_ADSB021_GROUP" => Some("239.255.0.21".into()),
            "FIREFLY_ADSB021_PORT" => Some("9021".into()),
            _ => None,
        });
        assert!(config.enabled);
        assert_eq!((config.sac, config.sic), (25, 30));
        assert_eq!(config.sensor_id, SensorId(231));
        assert!(config.is_multicast());
        assert_eq!(config.listen_port, 9021);

        let bad = Adsb021Config::from_lookup(|key| match key {
            "FIREFLY_ADSB021_PORT" => Some("0".into()),
            "FIREFLY_ADSB021_GROUP" => Some("garbage".into()),
            _ => None,
        });
        assert_eq!(bad.listen_port, DEFAULT_PORT);
        assert_eq!(bad.listen_group, Ipv4Addr::UNSPECIFIED);
    }
}
