//! 12-factor configuration for the QNH service (VERT.1, ADR 0003).
//!
//! `FIREFLY_METEO_QNH` carries the region list as a JSON array — settable by
//! an operator or by Wayfinder's orchestrator, refreshed externally on the
//! met update cycle. A malformed or implausible value is a **startup
//! error**: a deployment that *configured* a meteo source must not silently
//! run without one (the vertical chain would quietly degrade to standard
//! atmosphere everywhere).

use crate::qnh::{QnhRegion, QnhService};

/// Plausibility band for a configured QNH, hPa. The lowest sea-level
/// pressure ever recorded is ~870 hPa (typhoon core), the highest
/// ~1085 hPa; anything outside is a typo, not weather.
const QNH_PLAUSIBLE_HPA: std::ops::RangeInclusive<f64> = 870.0..=1085.0;

/// The meteo configuration: the validated QNH region list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeteoConfig {
    /// Validated QNH regions, in configuration order.
    pub regions: Vec<QnhRegion>,
}

/// Why a `FIREFLY_METEO_QNH` value could not be used. All variants are
/// startup configuration faults.
#[derive(Debug)]
pub enum MeteoConfigError {
    /// The value is not a JSON array of regions.
    Parse(serde_json::Error),
    /// A region carries an out-of-range or non-finite field.
    InvalidRegion {
        /// Index of the offending region in the configured list.
        index: usize,
        /// What is wrong with it.
        reason: &'static str,
    },
}

impl std::fmt::Display for MeteoConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeteoConfigError::Parse(e) => write!(f, "FIREFLY_METEO_QNH: invalid JSON: {e}"),
            MeteoConfigError::InvalidRegion { index, reason } => {
                write!(f, "FIREFLY_METEO_QNH[{index}]: {reason}")
            }
        }
    }
}

impl std::error::Error for MeteoConfigError {}

impl MeteoConfig {
    /// Parse and validate a `FIREFLY_METEO_QNH` JSON value (an array of
    /// `{name, lat, lon, radius_nm?, qnh_hpa}` objects).
    pub fn from_json(json: &str) -> Result<Self, MeteoConfigError> {
        let regions: Vec<QnhRegion> =
            serde_json::from_str(json).map_err(MeteoConfigError::Parse)?;
        for (index, region) in regions.iter().enumerate() {
            if region.name.trim().is_empty() {
                return Err(MeteoConfigError::InvalidRegion {
                    index,
                    reason: "name must not be empty",
                });
            }
            if !region.lat.is_finite() || !(-90.0..=90.0).contains(&region.lat) {
                return Err(MeteoConfigError::InvalidRegion {
                    index,
                    reason: "lat must be finite and within [-90,90]",
                });
            }
            if !region.lon.is_finite() || !(-180.0..=180.0).contains(&region.lon) {
                return Err(MeteoConfigError::InvalidRegion {
                    index,
                    reason: "lon must be finite and within [-180,180]",
                });
            }
            if let Some(radius) = region.radius_nm {
                if !radius.is_finite() || radius <= 0.0 {
                    return Err(MeteoConfigError::InvalidRegion {
                        index,
                        reason: "radius_nm must be finite and > 0",
                    });
                }
            }
            if !region.qnh_hpa.is_finite() || !QNH_PLAUSIBLE_HPA.contains(&region.qnh_hpa) {
                return Err(MeteoConfigError::InvalidRegion {
                    index,
                    reason: "qnh_hpa must be within the plausible band [870,1085]",
                });
            }
        }
        Ok(Self { regions })
    }

    /// Read the configuration from the process environment. Unset or empty
    /// `FIREFLY_METEO_QNH` yields an empty config (standard atmosphere
    /// everywhere — allowed, but the caller should say so in the log);
    /// a set-but-broken value is an error.
    pub fn from_env() -> Result<Self, MeteoConfigError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Read from an arbitrary key→value lookup (testable without touching
    /// the process environment).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, MeteoConfigError> {
        match get("FIREFLY_METEO_QNH") {
            Some(json) if !json.trim().is_empty() => Self::from_json(&json),
            _ => Ok(Self::default()),
        }
    }

    /// Build the lookup service from this configuration.
    pub fn into_service(self) -> QnhService {
        QnhService::new(self.regions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A valid region list parses field-exactly; unset env is an empty
    /// (standard-atmosphere) config, not an error. REQ: FR-TRK-041
    #[test]
    fn valid_json_parses_and_unset_is_empty() {
        let config = MeteoConfig::from_json(
            r#"[{"name":"EDDF","lat":50.03,"lon":8.57,"radius_nm":60,"qnh_hpa":1008},
                {"name":"COUNTRY","lat":51.0,"lon":10.0,"qnh_hpa":1015}]"#,
        )
        .expect("valid");
        assert_eq!(config.regions.len(), 2);
        assert_eq!(config.regions[0].name, "EDDF");
        assert_eq!(config.regions[0].radius_nm, Some(60.0));
        assert_eq!(config.regions[1].radius_nm, None, "radius is optional");

        let service = config.into_service();
        assert!(service.lookup(50.0, 8.6).is_observed());

        let empty = MeteoConfig::from_lookup(|_| None).expect("unset is fine");
        assert!(empty.regions.is_empty());
    }

    /// Malformed JSON and implausible values are startup errors — a
    /// configured meteo source must never be silently dropped.
    /// REQ: FR-TRK-041
    #[test]
    fn malformed_or_implausible_values_are_errors() {
        assert!(matches!(
            MeteoConfig::from_json("not json"),
            Err(MeteoConfigError::Parse(_))
        ));
        // QNH typo (108 instead of 1008) is caught by the plausibility band.
        assert!(matches!(
            MeteoConfig::from_json(r#"[{"name":"EDDF","lat":50.0,"lon":8.6,"qnh_hpa":108}]"#),
            Err(MeteoConfigError::InvalidRegion { index: 0, .. })
        ));
        assert!(matches!(
            MeteoConfig::from_json(r#"[{"name":"","lat":50.0,"lon":8.6,"qnh_hpa":1008}]"#),
            Err(MeteoConfigError::InvalidRegion { .. })
        ));
        assert!(matches!(
            MeteoConfig::from_json(r#"[{"name":"X","lat":95.0,"lon":8.6,"qnh_hpa":1008}]"#),
            Err(MeteoConfigError::InvalidRegion { .. })
        ));
        assert!(matches!(
            MeteoConfig::from_json(
                r#"[{"name":"X","lat":50.0,"lon":8.6,"radius_nm":0,"qnh_hpa":1008}]"#
            ),
            Err(MeteoConfigError::InvalidRegion { .. })
        ));
        // A set-but-empty env behaves like unset.
        let empty =
            MeteoConfig::from_lookup(|k| (k == "FIREFLY_METEO_QNH").then(|| "  ".to_string()))
                .expect("blank is unset");
        assert!(empty.regions.is_empty());
    }
}
