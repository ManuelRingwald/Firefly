//! 12-factor configuration for the community-aggregator ADS-B adapter
//! (ADR 0031, ADR 0003).
//!
//! All settings come from environment variables so the adapter can be enabled,
//! tuned and pointed at a different provider without code changes or config
//! files. The standalone envs (`FIREFLY_ADSBAGG_*`) mirror the OpenSky adapter's
//! layout; in the orchestrated path the same fields arrive via `FIREFLY_SOURCES`
//! (source-input contract v1.5.0).

use firefly_core::SensorId;

/// Which ADSBExchange-v2-compatible community aggregator to poll. All providers
/// share the same response schema; they differ only in host and URL path, so one
/// adapter serves them all and the operator can switch on outage or throttling.
///
/// airplanes.live is a known third candidate but is deliberately **not** offered
/// yet: the unit of its `/v2/point` radius parameter is documented
/// inconsistently (km vs NM), and a wrong unit would silently halve the coverage
/// circle — an unacceptable, invisible surveillance gap (ADR 0031).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Provider {
    /// [adsb.lol](https://adsb.lol) — open data (ODbL), no auth, dynamic rate
    /// limits. The default.
    #[default]
    AdsbLol,
    /// [adsb.fi](https://adsb.fi) — open data, no auth, 1 request/second on the
    /// public endpoints.
    AdsbFi,
}

impl Provider {
    /// Parse the wire/env value (contract vocabulary, snake_case). Unknown
    /// values are `None` — the caller turns that into a hard config error
    /// rather than silently polling the wrong provider.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "adsb_lol" => Some(Provider::AdsbLol),
            "adsb_fi" => Some(Provider::AdsbFi),
            _ => None,
        }
    }

    /// The wire/env value this provider is addressed by.
    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::AdsbLol => "adsb_lol",
            Provider::AdsbFi => "adsb_fi",
        }
    }

    /// The provider's API base URL (everything before the version'd query path).
    pub fn base_url(&self) -> &'static str {
        match self {
            Provider::AdsbLol => "https://api.adsb.lol",
            Provider::AdsbFi => "https://opendata.adsb.fi/api",
        }
    }

    /// The point-query path for a circle around (`lat`, `lon`) with radius
    /// `dist_nm` **nautical miles** (both providers cap at 250 NM). adsb.lol
    /// serves the ADSBEx-compatible `/v2/...` layout, adsb.fi the same shape
    /// under `/v3/...`.
    pub fn point_path(&self, lat: f64, lon: f64, dist_nm: f64) -> String {
        match self {
            Provider::AdsbLol => format!("/v2/lat/{lat}/lon/{lon}/dist/{dist_nm}"),
            Provider::AdsbFi => format!("/v3/lat/{lat}/lon/{lon}/dist/{dist_nm}"),
        }
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Configuration for the community-aggregator poller.
#[derive(Debug, Clone, PartialEq)]
pub struct AdsbAggConfig {
    /// Whether to start polling at all. `FIREFLY_ADSBAGG_ENABLED`, default `false`.
    pub enabled: bool,
    /// Which aggregator to poll. `FIREFLY_ADSBAGG_PROVIDER` (`adsb_lol` |
    /// `adsb_fi`), default `adsb_lol`.
    pub provider: Provider,
    /// Override for the provider's base URL. `FIREFLY_ADSBAGG_BASE_URL`. Meant
    /// for tests and self-hosted aggregator instances (the adsb.lol stack is
    /// open source); default `None` → the provider's public base.
    pub base_url: Option<String>,
    /// Southern latitude bound of the coverage bounding box, degrees.
    /// `FIREFLY_ADSBAGG_LAT_MIN`, default `47.0` (matches the OpenSky adapter).
    pub lat_min: f64,
    /// Northern latitude bound, degrees. `FIREFLY_ADSBAGG_LAT_MAX`, default `55.0`.
    pub lat_max: f64,
    /// Western longitude bound, degrees. `FIREFLY_ADSBAGG_LON_MIN`, default `5.0`.
    pub lon_min: f64,
    /// Eastern longitude bound, degrees. `FIREFLY_ADSBAGG_LON_MAX`, default `16.0`.
    pub lon_max: f64,
    /// Seconds between polls. `FIREFLY_ADSBAGG_POLL_INTERVAL_SECS`, default `10`.
    /// The public endpoints allow ~1 request/second; the default stays a polite
    /// order of magnitude below that.
    pub poll_interval_secs: u64,
    /// The [`SensorId`] stamped onto every plot. `FIREFLY_ADSBAGG_SENSOR_ID`,
    /// default `230` (OpenSky 200, FLARM 210, radar 220 — one decade per
    /// adapter family).
    pub sensor_id: SensorId,
}

impl Default for AdsbAggConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: Provider::default(),
            base_url: None,
            lat_min: 47.0,
            lat_max: 55.0,
            lon_min: 5.0,
            lon_max: 16.0,
            poll_interval_secs: 10,
            sensor_id: SensorId(230),
        }
    }
}

impl AdsbAggConfig {
    /// Read configuration from the process environment.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Read configuration from an arbitrary key→value lookup (testable without
    /// touching the real process environment).
    ///
    /// Unset or unparseable values fall back to the defaults; an unknown
    /// `FIREFLY_ADSBAGG_PROVIDER` also falls back (the standalone/dev path is
    /// lenient — the orchestrated contract path in `firefly-server` is strict).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let d = Self::default();
        let enabled = get("FIREFLY_ADSBAGG_ENABLED")
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(d.enabled);
        let provider = get("FIREFLY_ADSBAGG_PROVIDER")
            .and_then(|v| Provider::parse(&v))
            .unwrap_or(d.provider);
        let base_url = get("FIREFLY_ADSBAGG_BASE_URL").filter(|s| !s.trim().is_empty());
        let parse_deg = |key: &str, default: f64| {
            get(key)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(default)
        };
        let lat_min = parse_deg("FIREFLY_ADSBAGG_LAT_MIN", d.lat_min);
        let lat_max = parse_deg("FIREFLY_ADSBAGG_LAT_MAX", d.lat_max);
        let lon_min = parse_deg("FIREFLY_ADSBAGG_LON_MIN", d.lon_min);
        let lon_max = parse_deg("FIREFLY_ADSBAGG_LON_MAX", d.lon_max);
        let poll_interval_secs = get("FIREFLY_ADSBAGG_POLL_INTERVAL_SECS")
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&s| s > 0)
            .unwrap_or(d.poll_interval_secs);
        let sensor_id = get("FIREFLY_ADSBAGG_SENSOR_ID")
            .and_then(|v| v.parse::<u16>().ok())
            .map(SensorId)
            .unwrap_or(d.sensor_id);
        Self {
            enabled,
            provider,
            base_url,
            lat_min,
            lat_max,
            lon_min,
            lon_max,
            poll_interval_secs,
            sensor_id,
        }
    }

    /// The effective base URL: the override when set, else the provider's
    /// public base. A trailing `/` is trimmed so path concatenation is uniform.
    pub fn effective_base_url(&self) -> String {
        self.base_url
            .as_deref()
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_else(|| self.provider.base_url().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unset environment → defaults apply; adapter is off by default.
    #[test]
    fn empty_environment_yields_defaults() {
        let config = AdsbAggConfig::from_lookup(|_| None);
        assert_eq!(config, AdsbAggConfig::default());
        assert!(!config.enabled, "adapter is off unless explicitly enabled");
        assert_eq!(
            config.provider,
            Provider::AdsbLol,
            "adsb.lol is the default"
        );
        assert_eq!(config.sensor_id, SensorId(230));
    }

    /// All env-vars parse correctly.
    #[test]
    fn valid_values_are_parsed() {
        let config = AdsbAggConfig::from_lookup(|key| match key {
            "FIREFLY_ADSBAGG_ENABLED" => Some("true".into()),
            "FIREFLY_ADSBAGG_PROVIDER" => Some("adsb_fi".into()),
            "FIREFLY_ADSBAGG_LAT_MIN" => Some("48.0".into()),
            "FIREFLY_ADSBAGG_LAT_MAX" => Some("54.0".into()),
            "FIREFLY_ADSBAGG_LON_MIN" => Some("6.0".into()),
            "FIREFLY_ADSBAGG_LON_MAX" => Some("15.0".into()),
            "FIREFLY_ADSBAGG_POLL_INTERVAL_SECS" => Some("20".into()),
            "FIREFLY_ADSBAGG_SENSOR_ID" => Some("231".into()),
            _ => None,
        });
        assert!(config.enabled);
        assert_eq!(config.provider, Provider::AdsbFi);
        assert!((config.lat_min - 48.0).abs() < 1e-12);
        assert!((config.lon_max - 15.0).abs() < 1e-12);
        assert_eq!(config.poll_interval_secs, 20);
        assert_eq!(config.sensor_id, SensorId(231));
    }

    /// Garbage values fall back to defaults rather than crashing; an unknown
    /// provider falls back on the lenient standalone path.
    #[test]
    fn invalid_values_fall_back_to_defaults() {
        let config = AdsbAggConfig::from_lookup(|key| match key {
            "FIREFLY_ADSBAGG_PROVIDER" => Some("adsbexchange".into()),
            "FIREFLY_ADSBAGG_LAT_MIN" => Some("not-a-number".into()),
            "FIREFLY_ADSBAGG_POLL_INTERVAL_SECS" => Some("0".into()), // zero → rejected
            _ => None,
        });
        let d = AdsbAggConfig::default();
        assert_eq!(config.provider, d.provider);
        assert!((config.lat_min - d.lat_min).abs() < 1e-12);
        assert_eq!(config.poll_interval_secs, d.poll_interval_secs);
    }

    /// Provider vocabulary round-trips and rejects unknown values.
    #[test]
    fn provider_parse_round_trips() {
        for p in [Provider::AdsbLol, Provider::AdsbFi] {
            assert_eq!(Provider::parse(p.as_str()), Some(p));
        }
        assert_eq!(Provider::parse(" ADSB_LOL "), Some(Provider::AdsbLol));
        assert_eq!(
            Provider::parse("airplanes_live"),
            None,
            "deferred (ADR 0031)"
        );
        assert_eq!(Provider::parse("opensky"), None);
    }

    /// Each provider builds its own documented point-query path.
    #[test]
    fn provider_point_paths_match_the_public_docs() {
        assert_eq!(
            Provider::AdsbLol.point_path(50.03, 8.57, 50.0),
            "/v2/lat/50.03/lon/8.57/dist/50"
        );
        assert_eq!(
            Provider::AdsbFi.point_path(60.3179, 24.9496, 25.0),
            "/v3/lat/60.3179/lon/24.9496/dist/25"
        );
    }

    /// The base-URL override wins over the provider default; trailing slashes
    /// are normalised away.
    #[test]
    fn base_url_override_wins_and_is_trimmed() {
        let mut config = AdsbAggConfig::default();
        assert_eq!(config.effective_base_url(), "https://api.adsb.lol");
        config.provider = Provider::AdsbFi;
        assert_eq!(config.effective_base_url(), "https://opendata.adsb.fi/api");
        config.base_url = Some("http://localhost:8080/".into());
        assert_eq!(config.effective_base_url(), "http://localhost:8080");
    }
}
