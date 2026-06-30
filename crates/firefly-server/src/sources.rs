//! Env-driven source-input contract (`FIREFLY_SOURCES`, ADR 0023 +
//! `docs/source-input-contract.md`).
//!
//! An orchestrator (Wayfinder's auto-orchestration, ADR 0012 there) configures a
//! Firefly instance's live sources by setting `FIREFLY_SOURCES` — a JSON array,
//! one entry per source — plus, for each credentialled source, a separate named
//! env carrying the secret value (referenced by name, never inlined).
//!
//! This module parses and validates that list and maps an `adsb_opensky` entry
//! onto the existing [`OpenSkyConfig`] the poller already consumes. Wiring the
//! resulting configs into the live tracker is the caller's job (`main.rs`);
//! keeping parsing/mapping here makes it fully unit-testable without spawning
//! pollers or touching the process environment.

use std::fmt;

use firefly_core::SensorId;
use firefly_flarm::FlarmConfig;
use firefly_opensky::OpenSkyConfig;
use serde::Deserialize;

/// Nominal scan period (seconds) attributed to a `flarm_aprs` source. FLARM/OGN
/// is a push stream with no poll interval, but the tracker needs a per-sensor
/// scan period for its asynchronous deletion-cadence floor (ADR 0013) and the
/// sensor-health monitor (CAT063). FLARM beacons typically arrive every few
/// seconds; this is a sensible, slightly conservative nominal.
pub const FLARM_NOMINAL_SCAN_SECS: f64 = 5.0;

/// Closed source-type vocabulary (mirrors Wayfinder's `source_config`). An
/// unknown string fails deserialization → a startup configuration error, never a
/// silently ignored source (ADR 0023).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    AdsbOpensky,
    FlarmAprs,
    RadarAsterix,
}

/// WGS84 bounding box. Field names match Wayfinder's `source_config` so the
/// contract is near pass-through.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct BBox {
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_lat: f64,
    pub max_lon: f64,
}

/// One entry of `FIREFLY_SOURCES`. `cred_env` names the env that carries the
/// credential *value* (never the value itself).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SourceSpec {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    #[serde(default)]
    pub bbox: Option<BBox>,
    #[serde(default)]
    pub sac: Option<u16>,
    #[serde(default)]
    pub sic: Option<u16>,
    #[serde(default)]
    pub sensor_id: Option<u16>,
    #[serde(default)]
    pub cred_env: Option<String>,
}

/// Why a `FIREFLY_SOURCES` value could not be turned into runnable sources. All
/// variants are startup configuration faults (the operator/orchestrator set
/// something inconsistent), not runtime errors.
#[derive(Debug)]
pub enum SourceError {
    /// Malformed JSON, or an unknown `type` outside the closed vocabulary.
    Parse(serde_json::Error),
    /// An area source (e.g. `adsb_opensky`) is missing its required `bbox`.
    MissingBBox { index: usize },
    /// `bbox` is not finite, out of WGS84 range, or has `min > max`.
    InvalidBBox { index: usize, reason: &'static str },
    /// `cred_env` names an env that is unset or empty.
    MissingCredential { index: usize, env: String },
    /// The credential value is not in `client_id:client_secret` form (no `:`
    /// separator).
    MalformedCredential { index: usize, env: String },
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::Parse(e) => write!(f, "FIREFLY_SOURCES: invalid JSON: {e}"),
            SourceError::MissingBBox { index } => {
                write!(f, "FIREFLY_SOURCES[{index}]: area source requires a bbox")
            }
            SourceError::InvalidBBox { index, reason } => {
                write!(f, "FIREFLY_SOURCES[{index}]: invalid bbox: {reason}")
            }
            SourceError::MissingCredential { index, env } => write!(
                f,
                "FIREFLY_SOURCES[{index}]: cred_env {env} is unset or empty"
            ),
            SourceError::MalformedCredential { index, env } => write!(
                f,
                "FIREFLY_SOURCES[{index}]: credential in {env} is malformed (expected two colon-separated parts)"
            ),
        }
    }
}

impl std::error::Error for SourceError {}

/// Parse the `FIREFLY_SOURCES` JSON array into typed specs. Malformed JSON or an
/// unknown `type` is a hard error (startup config fault), per ADR 0023.
pub fn parse_sources(json: &str) -> Result<Vec<SourceSpec>, SourceError> {
    serde_json::from_str(json).map_err(SourceError::Parse)
}

/// Build an [`OpenSkyConfig`] from an `adsb_opensky` spec at list position
/// `index`. `get_env` resolves the spec's `cred_env` to its
/// `client_id:client_secret` value (split at the **first** `:` — OAuth2 client ids
/// contain no `:`; ADR 0024). The poll interval keeps the [`OpenSkyConfig`]
/// default (anonymous-safe); bbox, sensor id and credentials come from the spec. A
/// missing `cred_env` yields anonymous access (no OAuth2 credentials).
///
/// The caller guarantees `spec.source_type == SourceType::AdsbOpensky`.
pub fn opensky_config_from_spec(
    spec: &SourceSpec,
    index: usize,
    get_env: impl Fn(&str) -> Option<String>,
) -> Result<OpenSkyConfig, SourceError> {
    let bbox = spec.bbox.ok_or(SourceError::MissingBBox { index })?;
    validate_bbox(&bbox, index)?;

    let mut cfg = OpenSkyConfig {
        enabled: true,
        lat_min: bbox.min_lat,
        lat_max: bbox.max_lat,
        lon_min: bbox.min_lon,
        lon_max: bbox.max_lon,
        ..OpenSkyConfig::default()
    };
    if let Some(sid) = spec.sensor_id {
        cfg.sensor_id = SensorId(sid);
    }
    if let Some(env_name) = &spec.cred_env {
        let raw = get_env(env_name).filter(|s| !s.is_empty()).ok_or_else(|| {
            SourceError::MissingCredential {
                index,
                env: env_name.clone(),
            }
        })?;
        let (client_id, client_secret) =
            raw.split_once(':')
                .ok_or_else(|| SourceError::MalformedCredential {
                    index,
                    env: env_name.clone(),
                })?;
        cfg.client_id = Some(client_id.to_string());
        cfg.client_secret = Some(client_secret.to_string());
    }
    Ok(cfg)
}

/// Build a [`FlarmConfig`] from a `flarm_aprs` spec at list position `index`
/// (ADR 0026). `bbox` is required (the APRS-IS server-side area filter); a
/// `cred_env` is optional and, when present, carries `callsign:passcode` (split
/// at the **first** `:` — APRS-IS callsigns contain no `:`). A missing `cred_env`
/// yields read-only anonymous access. A malformed bbox or credential is a hard
/// error — a configured source that cannot run must not be silently dropped.
///
/// The caller guarantees `spec.source_type == SourceType::FlarmAprs`.
pub fn flarm_config_from_spec(
    spec: &SourceSpec,
    index: usize,
    get_env: impl Fn(&str) -> Option<String>,
) -> Result<FlarmConfig, SourceError> {
    let bbox = spec.bbox.ok_or(SourceError::MissingBBox { index })?;
    validate_bbox(&bbox, index)?;

    let mut cfg = FlarmConfig {
        enabled: true,
        lat_min: bbox.min_lat,
        lat_max: bbox.max_lat,
        lon_min: bbox.min_lon,
        lon_max: bbox.max_lon,
        ..FlarmConfig::default()
    };
    if let Some(sid) = spec.sensor_id {
        cfg.sensor_id = SensorId(sid);
    }
    if let Some(env_name) = &spec.cred_env {
        let raw = get_env(env_name).filter(|s| !s.is_empty()).ok_or_else(|| {
            SourceError::MissingCredential {
                index,
                env: env_name.clone(),
            }
        })?;
        let (callsign, passcode) =
            raw.split_once(':')
                .ok_or_else(|| SourceError::MalformedCredential {
                    index,
                    env: env_name.clone(),
                })?;
        let passcode =
            passcode
                .trim()
                .parse::<i32>()
                .map_err(|_| SourceError::MalformedCredential {
                    index,
                    env: env_name.clone(),
                })?;
        cfg.callsign = Some(callsign.to_string());
        cfg.passcode = Some(passcode);
    }
    Ok(cfg)
}

/// Reject a bbox that is non-finite, outside WGS84 range, or inverted — config
/// faults that would otherwise silently yield an empty OpenSky query window.
fn validate_bbox(b: &BBox, index: usize) -> Result<(), SourceError> {
    let finite = b.min_lat.is_finite()
        && b.min_lon.is_finite()
        && b.max_lat.is_finite()
        && b.max_lon.is_finite();
    if !finite {
        return Err(SourceError::InvalidBBox {
            index,
            reason: "coordinates must be finite",
        });
    }
    if !(-90.0..=90.0).contains(&b.min_lat) || !(-90.0..=90.0).contains(&b.max_lat) {
        return Err(SourceError::InvalidBBox {
            index,
            reason: "latitude out of range [-90,90]",
        });
    }
    if !(-180.0..=180.0).contains(&b.min_lon) || !(-180.0..=180.0).contains(&b.max_lon) {
        return Err(SourceError::InvalidBBox {
            index,
            reason: "longitude out of range [-180,180]",
        });
    }
    if b.min_lat > b.max_lat || b.min_lon > b.max_lon {
        return Err(SourceError::InvalidBBox {
            index,
            reason: "min must be <= max",
        });
    }
    Ok(())
}

/// Runnable adapters resolved from a parsed source list.
pub struct ResolvedSources {
    /// One [`OpenSkyConfig`] per `adsb_opensky` source, in list order.
    pub opensky: Vec<OpenSkyConfig>,
    /// One [`FlarmConfig`] per `flarm_aprs` source, in list order (ADR 0026).
    pub flarm: Vec<FlarmConfig>,
    /// Reserved types present but without an adapter yet — the caller logs a WARN
    /// and skips them (availability over completeness, ADR 0023).
    pub skipped: Vec<SourceType>,
}

/// Resolve a parsed source list into runnable adapter configs. An `adsb_opensky`
/// entry becomes an [`OpenSkyConfig`] and a `flarm_aprs` entry a [`FlarmConfig`]
/// (credentials resolved via `get_env`); `radar_asterix` goes to `skipped`
/// (reserved, no adapter yet). A malformed entry (missing/invalid bbox, bad
/// credential) is a hard error — a configured source that cannot run must not be
/// silently dropped.
pub fn resolve_sources(
    specs: &[SourceSpec],
    get_env: impl Fn(&str) -> Option<String>,
) -> Result<ResolvedSources, SourceError> {
    let mut opensky = Vec::new();
    let mut flarm = Vec::new();
    let mut skipped = Vec::new();
    for (index, spec) in specs.iter().enumerate() {
        match spec.source_type {
            SourceType::AdsbOpensky => {
                opensky.push(opensky_config_from_spec(spec, index, &get_env)?)
            }
            SourceType::FlarmAprs => flarm.push(flarm_config_from_spec(spec, index, &get_env)?),
            other => skipped.push(other),
        }
    }
    Ok(ResolvedSources {
        opensky,
        flarm,
        skipped,
    })
}

/// A representative config across all live sources (OpenSky **and** FLARM), used
/// for the tracking-frame origin (the **union** bbox midpoint, unless
/// `FIREFLY_SYSTEM_REF_*` overrides) and the tracker's output cadence (the fastest
/// OpenSky poll interval, or a FLARM nominal when there is no OpenSky source). The
/// sensor id is the first OpenSky source's, else the first FLARM source's — a
/// placeholder for the geodetic path. Falls back to the default with no source.
pub fn representative_config(opensky: &[OpenSkyConfig], flarm: &[FlarmConfig]) -> OpenSkyConfig {
    let mut rep = opensky.first().cloned().unwrap_or_default();

    // With no OpenSky source, anchor the representative on the first FLARM source
    // so a FLARM-only feed still has a meaningful sensor id and output tick.
    if opensky.is_empty() {
        if let Some(f) = flarm.first() {
            rep.sensor_id = f.sensor_id;
            rep.poll_interval_secs = FLARM_NOMINAL_SCAN_SECS as u64;
        }
    }

    // Union bbox over every *present* source; leave the default bbox untouched
    // when there is no source at all.
    if !opensky.is_empty() || !flarm.is_empty() {
        rep.lat_min = opensky
            .iter()
            .map(|c| c.lat_min)
            .chain(flarm.iter().map(|c| c.lat_min))
            .fold(f64::INFINITY, f64::min);
        rep.lat_max = opensky
            .iter()
            .map(|c| c.lat_max)
            .chain(flarm.iter().map(|c| c.lat_max))
            .fold(f64::NEG_INFINITY, f64::max);
        rep.lon_min = opensky
            .iter()
            .map(|c| c.lon_min)
            .chain(flarm.iter().map(|c| c.lon_min))
            .fold(f64::INFINITY, f64::min);
        rep.lon_max = opensky
            .iter()
            .map(|c| c.lon_max)
            .chain(flarm.iter().map(|c| c.lon_max))
            .fold(f64::NEG_INFINITY, f64::max);
    }

    // Output cadence: publish at least as often as the fastest OpenSky poll.
    if let Some(min) = opensky.iter().map(|c| c.poll_interval_secs).min() {
        rep.poll_interval_secs = min;
    }

    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn parses_a_mixed_source_list() {
        let json = r#"[
            {"type":"adsb_opensky","bbox":{"min_lat":48.0,"min_lon":7.0,"max_lat":50.0,"max_lon":9.0},
             "sensor_id":201,"cred_env":"FIREFLY_SOURCE_0_SECRET"},
            {"type":"flarm_aprs","bbox":{"min_lat":48.0,"min_lon":7.0,"max_lat":50.0,"max_lon":9.0}},
            {"type":"radar_asterix","sac":1,"sic":4}
        ]"#;
        let specs = parse_sources(json).expect("valid");
        assert_eq!(specs.len(), 3);
        assert_eq!(specs[0].source_type, SourceType::AdsbOpensky);
        assert_eq!(specs[0].sensor_id, Some(201));
        assert_eq!(
            specs[0].cred_env.as_deref(),
            Some("FIREFLY_SOURCE_0_SECRET")
        );
        assert_eq!(specs[1].source_type, SourceType::FlarmAprs);
        assert_eq!(specs[2].source_type, SourceType::RadarAsterix);
        assert_eq!(specs[2].sac, Some(1));
        assert_eq!(specs[2].sic, Some(4));
    }

    #[test]
    fn unknown_type_is_a_parse_error() {
        let json =
            r#"[{"type":"mlat_secret","bbox":{"min_lat":0,"min_lon":0,"max_lat":1,"max_lon":1}}]"#;
        assert!(matches!(parse_sources(json), Err(SourceError::Parse(_))));
    }

    #[test]
    fn malformed_json_is_a_parse_error() {
        assert!(matches!(
            parse_sources("not json"),
            Err(SourceError::Parse(_))
        ));
    }

    #[test]
    fn opensky_config_maps_bbox_and_sensor_id() {
        let spec = SourceSpec {
            source_type: SourceType::AdsbOpensky,
            bbox: Some(BBox {
                min_lat: 48.0,
                min_lon: 7.0,
                max_lat: 50.0,
                max_lon: 9.0,
            }),
            sac: None,
            sic: None,
            sensor_id: Some(201),
            cred_env: None,
        };
        let cfg = opensky_config_from_spec(&spec, 0, no_env).expect("valid");
        assert!(cfg.enabled);
        assert_eq!(cfg.lat_min, 48.0);
        assert_eq!(cfg.lat_max, 50.0);
        assert_eq!(cfg.lon_min, 7.0);
        assert_eq!(cfg.lon_max, 9.0);
        assert_eq!(cfg.sensor_id, SensorId(201));
        assert!(cfg.client_id.is_none() && cfg.client_secret.is_none());
    }

    #[test]
    fn missing_sensor_id_keeps_the_default() {
        let spec = adsb_spec(None, None);
        let cfg = opensky_config_from_spec(&spec, 0, no_env).expect("valid");
        assert_eq!(cfg.sensor_id, OpenSkyConfig::default().sensor_id);
    }

    #[test]
    fn cred_env_is_split_into_client_id_and_secret() {
        let spec = adsb_spec(Some("SECRET"), None);
        let cfg = opensky_config_from_spec(&spec, 0, |k| {
            (k == "SECRET").then(|| "client-abc:s3cr3t".to_string())
        })
        .expect("valid");
        assert_eq!(cfg.client_id.as_deref(), Some("client-abc"));
        assert_eq!(cfg.client_secret.as_deref(), Some("s3cr3t"));
    }

    #[test]
    fn cred_split_uses_the_first_colon() {
        // A client secret may contain a colon; the client id never does (OAuth2).
        let spec = adsb_spec(Some("SECRET"), None);
        let cfg = opensky_config_from_spec(&spec, 0, |_| Some("client-abc:pa:ss".to_string()))
            .expect("valid");
        assert_eq!(cfg.client_id.as_deref(), Some("client-abc"));
        assert_eq!(cfg.client_secret.as_deref(), Some("pa:ss"));
    }

    #[test]
    fn cred_env_unset_or_empty_is_an_error() {
        let spec = adsb_spec(Some("SECRET"), None);
        assert!(matches!(
            opensky_config_from_spec(&spec, 2, no_env),
            Err(SourceError::MissingCredential { index: 2, .. })
        ));
        assert!(matches!(
            opensky_config_from_spec(&spec, 0, |_| Some(String::new())),
            Err(SourceError::MissingCredential { .. })
        ));
    }

    #[test]
    fn cred_without_colon_is_malformed() {
        let spec = adsb_spec(Some("SECRET"), None);
        assert!(matches!(
            opensky_config_from_spec(&spec, 0, |_| Some("nodelimiter".to_string())),
            Err(SourceError::MalformedCredential { .. })
        ));
    }

    #[test]
    fn missing_bbox_is_an_error() {
        let spec = SourceSpec {
            source_type: SourceType::AdsbOpensky,
            bbox: None,
            sac: None,
            sic: None,
            sensor_id: None,
            cred_env: None,
        };
        assert!(matches!(
            opensky_config_from_spec(&spec, 1, no_env),
            Err(SourceError::MissingBBox { index: 1 })
        ));
    }

    #[test]
    fn inverted_or_out_of_range_bbox_is_rejected() {
        let inverted = adsb_spec_with_bbox(BBox {
            min_lat: 50.0,
            min_lon: 7.0,
            max_lat: 48.0,
            max_lon: 9.0,
        });
        assert!(matches!(
            opensky_config_from_spec(&inverted, 0, no_env),
            Err(SourceError::InvalidBBox { .. })
        ));
        let out_of_range = adsb_spec_with_bbox(BBox {
            min_lat: 48.0,
            min_lon: 7.0,
            max_lat: 95.0,
            max_lon: 9.0,
        });
        assert!(matches!(
            opensky_config_from_spec(&out_of_range, 0, no_env),
            Err(SourceError::InvalidBBox { .. })
        ));
    }

    #[test]
    fn resolve_splits_opensky_from_reserved_types() {
        let specs = vec![
            adsb_spec(None, Some(201)),
            SourceSpec {
                source_type: SourceType::FlarmAprs,
                bbox: Some(BBox {
                    min_lat: 48.0,
                    min_lon: 7.0,
                    max_lat: 50.0,
                    max_lon: 9.0,
                }),
                sac: None,
                sic: None,
                sensor_id: None,
                cred_env: None,
            },
            SourceSpec {
                source_type: SourceType::RadarAsterix,
                bbox: None,
                sac: Some(1),
                sic: Some(4),
                sensor_id: None,
                cred_env: None,
            },
            adsb_spec(None, Some(202)),
        ];
        let resolved = resolve_sources(&specs, no_env).expect("valid");
        assert_eq!(resolved.opensky.len(), 2, "two adsb_opensky configs");
        assert_eq!(resolved.opensky[0].sensor_id, SensorId(201));
        assert_eq!(resolved.opensky[1].sensor_id, SensorId(202));
        assert_eq!(resolved.flarm.len(), 1, "one flarm_aprs config");
        assert!(resolved.flarm[0].enabled);
        assert_eq!(
            resolved.skipped,
            vec![SourceType::RadarAsterix],
            "only radar_asterix has no adapter yet"
        );
    }

    #[test]
    fn flarm_config_maps_bbox_sensor_and_credentials() {
        let spec = SourceSpec {
            source_type: SourceType::FlarmAprs,
            bbox: Some(BBox {
                min_lat: 48.0,
                min_lon: 7.0,
                max_lat: 50.0,
                max_lon: 9.0,
            }),
            sac: None,
            sic: None,
            sensor_id: Some(212),
            cred_env: Some("FLARM_SECRET".into()),
        };
        let cfg = flarm_config_from_spec(&spec, 0, |k| {
            (k == "FLARM_SECRET").then(|| "EDXY:12345".to_string())
        })
        .expect("valid");
        assert!(cfg.enabled);
        assert_eq!(cfg.lat_min, 48.0);
        assert_eq!(cfg.lon_max, 9.0);
        assert_eq!(cfg.sensor_id, SensorId(212));
        assert_eq!(cfg.callsign.as_deref(), Some("EDXY"));
        assert_eq!(cfg.passcode, Some(12345));
    }

    #[test]
    fn flarm_without_cred_is_read_only_anonymous() {
        let spec = SourceSpec {
            source_type: SourceType::FlarmAprs,
            bbox: Some(BBox {
                min_lat: 48.0,
                min_lon: 7.0,
                max_lat: 50.0,
                max_lon: 9.0,
            }),
            sac: None,
            sic: None,
            sensor_id: None,
            cred_env: None,
        };
        let cfg = flarm_config_from_spec(&spec, 0, no_env).expect("valid");
        assert!(cfg.callsign.is_none() && cfg.passcode.is_none());
    }

    #[test]
    fn flarm_missing_bbox_and_bad_cred_are_errors() {
        let no_bbox = SourceSpec {
            source_type: SourceType::FlarmAprs,
            bbox: None,
            sac: None,
            sic: None,
            sensor_id: None,
            cred_env: None,
        };
        assert!(matches!(
            flarm_config_from_spec(&no_bbox, 3, no_env),
            Err(SourceError::MissingBBox { index: 3 })
        ));

        let bad_cred = SourceSpec {
            source_type: SourceType::FlarmAprs,
            bbox: Some(BBox {
                min_lat: 48.0,
                min_lon: 7.0,
                max_lat: 50.0,
                max_lon: 9.0,
            }),
            sac: None,
            sic: None,
            sensor_id: None,
            cred_env: Some("C".into()),
        };
        // Passcode part is not an integer → malformed.
        assert!(matches!(
            flarm_config_from_spec(&bad_cred, 0, |_| Some("EDXY:notanumber".to_string())),
            Err(SourceError::MalformedCredential { .. })
        ));
    }

    #[test]
    fn resolve_propagates_a_bad_adsb_entry() {
        let specs = vec![SourceSpec {
            source_type: SourceType::AdsbOpensky,
            bbox: None, // missing → hard error, not skipped
            sac: None,
            sic: None,
            sensor_id: None,
            cred_env: None,
        }];
        assert!(matches!(
            resolve_sources(&specs, no_env),
            Err(SourceError::MissingBBox { index: 0 })
        ));
    }

    #[test]
    fn representative_unions_bboxes_and_takes_min_interval() {
        let mut a = opensky_config_from_spec(
            &adsb_spec_with_bbox(BBox {
                min_lat: 48.0,
                min_lon: 7.0,
                max_lat: 50.0,
                max_lon: 9.0,
            }),
            0,
            no_env,
        )
        .unwrap();
        a.poll_interval_secs = 10;
        let mut b = opensky_config_from_spec(
            &adsb_spec_with_bbox(BBox {
                min_lat: 49.0,
                min_lon: 6.0,
                max_lat: 52.0,
                max_lon: 8.0,
            }),
            1,
            no_env,
        )
        .unwrap();
        b.poll_interval_secs = 5;

        let rep = representative_config(&[a, b], &[]);
        assert_eq!(rep.lat_min, 48.0);
        assert_eq!(rep.lat_max, 52.0);
        assert_eq!(rep.lon_min, 6.0);
        assert_eq!(rep.lon_max, 9.0);
        assert_eq!(rep.poll_interval_secs, 5, "fastest source's cadence");
    }

    #[test]
    fn representative_of_empty_is_the_default() {
        assert_eq!(representative_config(&[], &[]), OpenSkyConfig::default());
    }

    #[test]
    fn representative_covers_a_flarm_only_feed() {
        let flarm = flarm_config_from_spec(
            &SourceSpec {
                source_type: SourceType::FlarmAprs,
                bbox: Some(BBox {
                    min_lat: 49.0,
                    min_lon: 6.0,
                    max_lat: 52.0,
                    max_lon: 8.0,
                }),
                sac: None,
                sic: None,
                sensor_id: Some(213),
                cred_env: None,
            },
            0,
            no_env,
        )
        .unwrap();

        let rep = representative_config(&[], &[flarm]);
        // Union bbox comes from the FLARM source, not the OpenSky default.
        assert_eq!(rep.lat_min, 49.0);
        assert_eq!(rep.lat_max, 52.0);
        assert_eq!(rep.lon_min, 6.0);
        assert_eq!(rep.lon_max, 8.0);
        assert_eq!(
            rep.sensor_id,
            SensorId(213),
            "flarm sensor anchors the frame"
        );
        assert_eq!(rep.poll_interval_secs, FLARM_NOMINAL_SCAN_SECS as u64);
    }

    // --- helpers ---------------------------------------------------------

    fn adsb_spec(cred_env: Option<&str>, sensor_id: Option<u16>) -> SourceSpec {
        SourceSpec {
            source_type: SourceType::AdsbOpensky,
            bbox: Some(BBox {
                min_lat: 48.0,
                min_lon: 7.0,
                max_lat: 50.0,
                max_lon: 9.0,
            }),
            sac: None,
            sic: None,
            sensor_id,
            cred_env: cred_env.map(str::to_string),
        }
    }

    fn adsb_spec_with_bbox(bbox: BBox) -> SourceSpec {
        SourceSpec {
            source_type: SourceType::AdsbOpensky,
            bbox: Some(bbox),
            sac: None,
            sic: None,
            sensor_id: None,
            cred_env: None,
        }
    }
}
