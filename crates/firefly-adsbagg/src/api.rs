//! ADSBExchange-v2-compatible response types and aircraft parsing (ADR 0031).
//!
//! adsb.lol and adsb.fi both serve the readsb/ADSBEx v2 JSON shape: a top-level
//! `now` timestamp plus an `ac` array of aircraft objects with named fields
//! (unlike OpenSky's positional state vectors). Each usable aircraft becomes a
//! [`Plot`](firefly_core::Plot) the tracker can consume directly.
//!
//! Robustness rules (untrusted input — never panic, drop what cannot be used):
//! - `alt_baro` is a number in **feet** *or* the literal string `"ground"`;
//!   ground traffic is excluded from the tracker (like OpenSky's `on_ground`).
//! - `hex` may carry a `~` prefix marking a non-ICAO (TIS-B track-file)
//!   address; the plot is kept but without an ICAO identity.
//! - `seen_pos` is the age of the last position fix; stale fixes are dropped
//!   rather than fed to the tracker as if current.

use firefly_core::{Callsign, ModeAC, Plot, SensorId, Timestamp};
use firefly_geo::Wgs84;
use serde::Deserialize;

use crate::geometry::BBoxDeg;

/// Feet → metres (exact, by definition).
const METRES_PER_FOOT: f64 = 0.3048;

/// Positions older than this (seconds, from `seen_pos`) are dropped: the API
/// keeps coasted aircraft in its response for a while, and feeding a
/// minutes-old fix to the tracker as "now" would corrupt the picture.
const STALE_POSITION_SECS: f64 = 30.0;

/// Raw v2/v3 point-query response. Unknown fields are ignored (forward
/// compatible); a missing `ac` means an empty sky.
#[derive(Debug, Deserialize)]
pub(crate) struct AircraftResponse {
    /// Server timestamp. The aggregator APIs send Unix **milliseconds**; plain
    /// readsb `aircraft.json` sends seconds. Normalised by [`now_unix_secs`].
    #[serde(default)]
    pub now: f64,
    #[serde(default)]
    pub ac: Vec<Aircraft>,
}

/// One aircraft record — only the fields the tracker consumes. Every field is
/// optional: the schema varies with the receiver mix and message age.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Aircraft {
    /// 24-bit address as lowercase hex; `~`-prefixed for non-ICAO (TIS-B).
    pub hex: Option<String>,
    /// Message-source class (`adsb_icao`, `mlat`, `tisb_icao`, …). Drives the
    /// position-accuracy sigma, like OpenSky's `position_source`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Callsign, space-padded like the raw Mode S field.
    pub flight: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    /// Barometric altitude in **feet**, or the literal string `"ground"`.
    pub alt_baro: Option<serde_json::Value>,
    /// Geometric (WGS84) altitude, feet.
    pub alt_geom: Option<f64>,
    /// Mode 3/A squawk as a 4-digit octal string.
    pub squawk: Option<String>,
    /// Seconds since the last position update.
    pub seen_pos: Option<f64>,
}

/// Normalise the response `now` to Unix seconds. Values above `10^11` can only
/// be milliseconds (10^11 s is the year 5138); everything else is already
/// seconds. Non-finite or non-positive values yield `None` so the caller can
/// fall back to the wall clock instead of stamping plots at the epoch.
pub(crate) fn now_unix_secs(now: f64) -> Option<f64> {
    if !now.is_finite() || now <= 0.0 {
        return None;
    }
    Some(if now > 1e11 { now / 1000.0 } else { now })
}

/// Map the message-source class to a 1σ position accuracy in metres, mirroring
/// the OpenSky adapter's ADR 0019 table: direct ADS-B (and its rebroadcast
/// variants) ≈ 75 m, MLAT ≈ 200 m, everything else a conservative 300 m.
pub(crate) fn sigma_from_kind(kind: Option<&str>) -> f64 {
    match kind {
        Some(k) if k.starts_with("adsb") || k.starts_with("adsr") => 75.0,
        Some("mlat") => 200.0,
        _ => 300.0,
    }
}

/// Parse one aircraft record into a [`Plot`].
///
/// Returns `None` when the record is unusable: no position, on the ground,
/// position fix older than [`STALE_POSITION_SECS`], or outside `bbox` (the
/// query circle circumscribes the box, so edge surplus is expected and
/// silently trimmed).
pub(crate) fn parse_aircraft(
    ac: &Aircraft,
    sensor: SensorId,
    response_time_unix: f64,
    bbox: &BBoxDeg,
) -> Option<Plot> {
    let lat = ac.lat.filter(|v| v.is_finite())?;
    let lon = ac.lon.filter(|v| v.is_finite())?;
    if !bbox.contains(lat, lon) {
        return None;
    }

    // Ground traffic is excluded (ground vehicles and parked aircraft), like
    // the OpenSky adapter's on_ground filter.
    let baro_alt_ft = match &ac.alt_baro {
        Some(serde_json::Value::String(s)) if s == "ground" => return None,
        Some(v) => v.as_f64(),
        None => None,
    };

    // A stale position fix is not a current measurement — drop it.
    let seen_pos = ac.seen_pos.filter(|v| v.is_finite()).unwrap_or(0.0);
    if seen_pos > STALE_POSITION_SECS {
        return None;
    }

    // `~`-prefixed hex is a non-ICAO (TIS-B) address: keep the plot, drop the
    // identity — the tracker must not book it as an ICAO 24-bit address.
    let icao = match ac.hex.as_deref().map(str::trim) {
        Some(h) if !h.starts_with('~') => Some(u32::from_str_radix(h, 16).ok()?),
        _ => None,
    };

    let callsign = ac
        .flight
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(Callsign::new);

    // Squawk: 4-digit octal string, like OpenSky (e.g. "7700" → 0o7700).
    let mode_3a = ac
        .squawk
        .as_deref()
        .and_then(|s| u16::from_str_radix(s.trim(), 8).ok());

    // Geometric altitude preferred for the position; fall back to barometric,
    // then 0 (the tracker operates in the horizontal plane). Both are feet.
    let alt_m = ac
        .alt_geom
        .filter(|v| v.is_finite())
        .or(baro_alt_ft)
        .unwrap_or(0.0)
        * METRES_PER_FOOT;

    let position = Wgs84::from_degrees(lat, lon, alt_m);
    let sigma_pos_m = sigma_from_kind(ac.kind.as_deref());
    let mode_ac = ModeAC {
        mode_3a,
        // alt_baro is already feet — no conversion (OpenSky sends metres!).
        flight_level_ft: baro_alt_ft,
        icao_address: icao,
        callsign,
        spi: false,
        daps: firefly_core::Daps::default(),
    };

    // Data-time: the server timestamp minus this aircraft's position age, so
    // the tracker sees each fix at its measurement time (data-time driven,
    // ADR 0013), not the poll time.
    Some(Plot::adsb(
        sensor,
        Timestamp(response_time_unix - seen_pos),
        position,
        sigma_pos_m,
        mode_ac,
    ))
}

/// Parse a whole response into plots, trimming the circle surplus to `bbox`.
pub(crate) fn parse_response(
    resp: &AircraftResponse,
    sensor: SensorId,
    fallback_now_unix: f64,
    bbox: &BBoxDeg,
) -> Vec<Plot> {
    let now = now_unix_secs(resp.now).unwrap_or(fallback_now_unix);
    resp.ac
        .iter()
        .filter_map(|ac| parse_aircraft(ac, sensor, now, bbox))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed fixture from a real `api.adsb.lol/v2/lat/50.03/lon/8.57/dist/50`
    /// response (2026-07-05, Frankfurt area) — the ground truth this adapter is
    /// built against — plus synthetic edge cases: a ground vehicle, a stale
    /// fix, a `~` (non-ICAO) address and an out-of-box position.
    const FIXTURE: &str = r#"{
        "ac": [
            {"hex":"4d2255","type":"adsb_icao","flight":"RYR8FY  ","r":"9H-QDI","t":"B738",
             "alt_baro":28650,"alt_geom":29850,"gs":401.5,"track":291.32,"squawk":"1000",
             "category":"A3","lat":50.2,"lon":8.4,"seen_pos":0.4,"seen":0.1,"nic":8,"rc":186},
            {"hex":"3c65aa","type":"adsb_icao","flight":"DLH39A  ",
             "alt_baro":"ground","gs":12.5,"lat":50.033,"lon":8.57,"seen_pos":1.0},
            {"hex":"3c1234","type":"adsb_icao","alt_baro":11000,
             "lat":50.1,"lon":8.6,"seen_pos":120.5},
            {"hex":"~2e9fd1","type":"tisb_trackfile","alt_baro":4200,
             "lat":49.9,"lon":8.5,"seen_pos":2.0},
            {"hex":"3c9999","type":"mlat","alt_baro":9000,
             "lat":52.5,"lon":8.5,"seen_pos":1.0}
        ],
        "total": 5,
        "now": 1783267200000,
        "ctime": 1783267200000,
        "ptime": 12
    }"#;

    fn bbox() -> BBoxDeg {
        BBoxDeg {
            lat_min: 49.5,
            lat_max: 50.5,
            lon_min: 7.8,
            lon_max: 9.3,
        }
    }

    /// The reference fixture parses; exactly the usable aircraft survive:
    /// RYR8FY (airborne, fresh, in-box) and the `~` TIS-B target (kept, but
    /// without ICAO identity). Ground, stale and out-of-box records drop.
    #[test]
    fn fixture_yields_the_usable_plots() {
        let resp: AircraftResponse = serde_json::from_str(FIXTURE).expect("fixture parses");
        assert_eq!(resp.ac.len(), 5);
        let plots = parse_response(&resp, SensorId(230), 0.0, &bbox());
        assert_eq!(plots.len(), 2, "ground + stale + out-of-box are dropped");

        let ryr = &plots[0];
        assert_eq!(ryr.mode_ac.icao_address, Some(0x4d2255));
        assert_eq!(ryr.mode_ac.callsign, Some(Callsign::new("RYR8FY")));
        assert_eq!(ryr.mode_ac.mode_3a, Some(0o1000));
        // alt_baro is already feet — 28 650 ft, NOT converted like OpenSky metres.
        let fl = ryr.mode_ac.flight_level_ft.expect("flight level");
        assert!((fl - 28_650.0).abs() < 1e-9);
        // Geodetic measurement at the reported position with the ADS-B sigma.
        match &ryr.measurement {
            firefly_core::Measurement::Geodetic {
                position,
                sigma_pos_m,
            } => {
                assert!((position.lat_deg() - 50.2).abs() < 1e-9);
                assert!((position.lon_deg() - 8.4).abs() < 1e-9);
                assert_eq!(*sigma_pos_m, 75.0, "adsb_icao → 75 m (ADR 0019 analogue)");
            }
            other => panic!("expected a geodetic measurement, got {other:?}"),
        }

        let tisb = &plots[1];
        assert_eq!(tisb.mode_ac.icao_address, None, "~hex carries no ICAO");
    }

    /// `now` in milliseconds is normalised to seconds, and the per-aircraft
    /// data-time subtracts the position age.
    #[test]
    fn timestamps_normalise_ms_and_subtract_position_age() {
        let resp: AircraftResponse = serde_json::from_str(FIXTURE).unwrap();
        let plots = parse_response(&resp, SensorId(230), 0.0, &bbox());
        // now = 1783267200000 ms → 1783267200 s; RYR8FY seen_pos = 0.4.
        assert!((plots[0].time.as_secs() - (1_783_267_200.0 - 0.4)).abs() < 1e-6);
    }

    /// `now` heuristic: seconds pass through, milliseconds divide, garbage is None.
    #[test]
    fn now_unix_secs_handles_seconds_ms_and_garbage() {
        assert_eq!(now_unix_secs(1_751_731_200.0), Some(1_751_731_200.0));
        assert_eq!(now_unix_secs(1_751_731_200_000.0), Some(1_751_731_200.0));
        assert_eq!(now_unix_secs(0.0), None);
        assert_eq!(now_unix_secs(f64::NAN), None);
        assert_eq!(now_unix_secs(-5.0), None);
    }

    /// A response without `now` falls back to the caller's wall clock.
    #[test]
    fn missing_now_uses_the_fallback_clock() {
        let resp: AircraftResponse = serde_json::from_str(
            r#"{"ac":[{"hex":"4d2255","alt_baro":10000,"lat":50.0,"lon":8.5,"seen_pos":1.0}]}"#,
        )
        .unwrap();
        let plots = parse_response(&resp, SensorId(230), 1_000_000.0, &bbox());
        assert_eq!(plots.len(), 1);
        assert!((plots[0].time.as_secs() - 999_999.0).abs() < 1e-9);
    }

    /// Missing position, non-finite position, or "ground" → no plot.
    #[test]
    fn unusable_records_are_dropped_not_panicked() {
        let cases = [
            r#"{"hex":"3c1234"}"#,
            r#"{"hex":"3c1234","lat":50.0}"#,
            r#"{"hex":"3c1234","lat":null,"lon":8.5}"#,
            r#"{"hex":"3c1234","lat":50.0,"lon":8.5,"alt_baro":"ground"}"#,
        ];
        for json in cases {
            let ac: Aircraft = serde_json::from_str(json).unwrap();
            assert!(
                parse_aircraft(&ac, SensorId(230), 0.0, &bbox()).is_none(),
                "should drop: {json}"
            );
        }
    }

    /// An invalid (non-hex) ICAO address drops the record — a corrupt identity
    /// must not enter the tracker.
    #[test]
    fn invalid_hex_is_rejected() {
        let ac = Aircraft {
            hex: Some("GGGGGG".into()),
            lat: Some(50.0),
            lon: Some(8.5),
            ..Aircraft::default()
        };
        assert!(parse_aircraft(&ac, SensorId(230), 0.0, &bbox()).is_none());
    }

    /// The sigma table mirrors ADR 0019: ADS-B (and rebroadcast) 75 m, MLAT
    /// 200 m, anything else 300 m.
    #[test]
    fn sigma_from_kind_follows_the_adr0019_analogue() {
        assert_eq!(sigma_from_kind(Some("adsb_icao")), 75.0);
        assert_eq!(sigma_from_kind(Some("adsb_icao_nt")), 75.0);
        assert_eq!(sigma_from_kind(Some("adsr_icao")), 75.0);
        assert_eq!(sigma_from_kind(Some("mlat")), 200.0);
        assert_eq!(sigma_from_kind(Some("tisb_trackfile")), 300.0);
        assert_eq!(sigma_from_kind(None), 300.0);
    }

    /// Whitespace-only callsign produces no spurious label.
    #[test]
    fn empty_callsign_produces_none() {
        let ac = Aircraft {
            hex: Some("3c1234".into()),
            flight: Some("        ".into()),
            lat: Some(50.0),
            lon: Some(8.5),
            alt_baro: Some(serde_json::json!(10_000)),
            ..Aircraft::default()
        };
        let plot = parse_aircraft(&ac, SensorId(230), 0.0, &bbox()).expect("parses");
        assert!(plot.mode_ac.callsign.is_none());
    }
}
