//! OpenSky Network REST API response types and state-vector parsing.
//!
//! Each "state vector" from the `/api/states/all` endpoint is a JSON array of
//! mixed types; we parse the relevant indices and convert them into a
//! [`Plot`](firefly_core::Plot) that the tracker can consume directly.

use firefly_core::{Callsign, ModeAC, Plot, SensorId, Timestamp};
use firefly_geo::Wgs84;
use serde::Deserialize;

/// Metres → feet conversion factor.
const FEET_PER_METRE: f64 = 3.280_839_895;

/// Raw response from `GET /api/states/all`.
///
/// `states` is `None` when the bounding box contains no aircraft.
#[derive(Debug, Deserialize)]
pub(crate) struct StatesResponse {
    /// Unix timestamp (server-side, seconds since epoch).
    pub time: i64,
    /// One entry per transponder in the bounding box. Each entry is a
    /// JSON array of mixed types — we index into it by position.
    pub states: Option<Vec<serde_json::Value>>,
}

/// Map `position_source` (index 16 in the state vector) to a 1σ position
/// accuracy in metres.
///
/// Source codes (EUROCONTROL/OpenSky convention):
/// - `0` — ADS-B (NACp ≥ 8 typical, < 75 m HAL)
/// - `1` — ASTERIX (network-fused, typically 200 m)
/// - `2` — MLAT (multilateration, typically 200 m)
///
/// ADR 0019 NaCp→σ table.
pub(crate) fn sigma_from_source(position_source: i64) -> f64 {
    match position_source {
        0 => 75.0,
        1 | 2 => 200.0,
        _ => 300.0,
    }
}

/// Parse one OpenSky state vector (a JSON array) into a [`Plot`].
///
/// Returns `None` if mandatory fields are absent or the aircraft is on the
/// ground (ground vehicles are excluded from the tracker).
pub(crate) fn parse_state(
    state: &serde_json::Value,
    sensor: SensorId,
    timestamp_unix: f64,
) -> Option<Plot> {
    let arr = state.as_array()?;

    // --- Mandatory fields ---
    let icao24_str = arr.first()?.as_str()?;
    let icao: u32 = u32::from_str_radix(icao24_str.trim(), 16).ok()?;

    let lon = arr.get(5)?.as_f64()?;
    let lat = arr.get(6)?.as_f64()?;

    // Skip ground vehicles and parked aircraft.
    let on_ground = arr.get(8).and_then(|v| v.as_bool()).unwrap_or(true);
    if on_ground {
        return None;
    }

    // --- Optional fields ---
    // Barometric altitude (metres above 1013.25 hPa datum) → flight level (feet).
    let baro_alt_m = arr.get(7).and_then(|v| v.as_f64());
    let flight_level_ft = baro_alt_m.map(|m| m * FEET_PER_METRE);

    // Geometric altitude (WGS84 metres) preferred for position; fall back to
    // barometric, then to 0 (tracker operates in the horizontal plane).
    let geo_alt_m = arr
        .get(13)
        .and_then(|v| v.as_f64())
        .or(baro_alt_m)
        .unwrap_or(0.0);

    // Position source → σ (NaCp mapping, ADR 0019).
    let position_source = arr.get(16).and_then(|v| v.as_i64()).unwrap_or(3);
    let sigma_pos_m = sigma_from_source(position_source);

    // Callsign (space-padded, trim before use).
    let callsign = arr
        .get(1)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(Callsign::new);

    // Mode 3/A squawk: OpenSky gives it as a decimal string of the octal code
    // (e.g. "7700" → parse as base-8 → 0o7700 = 4032).
    let mode_3a = arr
        .get(14)
        .and_then(|v| v.as_str())
        .and_then(|s| u16::from_str_radix(s.trim(), 8).ok());

    let position = Wgs84::from_degrees(lat, lon, geo_alt_m);
    let mode_ac = ModeAC {
        mode_3a,
        flight_level_ft,
        icao_address: Some(icao),
        callsign,
    };

    // OpenSky timestamp is Unix epoch; the tracker works with seconds since
    // scenario start or UTC midnight. We pass the raw Unix timestamp here;
    // the integration layer in firefly-server must translate to the tracker's
    // time basis before handing the plot to the tracker.
    Some(Plot::adsb(
        sensor,
        Timestamp(timestamp_unix),
        position,
        sigma_pos_m,
        mode_ac,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[allow(clippy::too_many_arguments)]
    fn make_state(
        icao24: &str,
        callsign: Option<&str>,
        lon: Option<f64>,
        lat: Option<f64>,
        baro_alt: Option<f64>,
        on_ground: bool,
        squawk: Option<&str>,
        position_source: i64,
    ) -> serde_json::Value {
        json!([
            icao24, // 0: icao24
            callsign
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null), // 1: callsign
            "Germany", // 2: origin_country
            serde_json::Value::Null, // 3: time_position
            1_700_000_000i64, // 4: last_contact
            lon.map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null), // 5: longitude
            lat.map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null), // 6: latitude
            baro_alt
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null), // 7: baro_altitude
            on_ground, // 8: on_ground
            serde_json::Value::Null, // 9: velocity
            serde_json::Value::Null, // 10: true_track
            serde_json::Value::Null, // 11: vertical_rate
            serde_json::Value::Null, // 12: sensors
            serde_json::Value::Null, // 13: geo_altitude
            squawk
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null), // 14: squawk
            false,  // 15: spi
            position_source, // 16: position_source
        ])
    }

    /// A well-formed airborne state vector produces a valid Plot with the
    /// correct ICAO address, position, sigma, and callsign.
    #[test]
    fn well_formed_state_produces_plot() {
        let state = make_state(
            "3c5bad",
            Some("DLH123  "),
            Some(8.5),
            Some(50.0),
            Some(10_000.0),
            false,
            Some("1234"),
            0,
        );
        let plot = parse_state(&state, SensorId(200), 1_700_000_000.0).expect("should parse");

        assert_eq!(plot.mode_ac.icao_address, Some(0x3c5bad));
        assert_eq!(plot.mode_ac.callsign, Some(Callsign::new("DLH123")));
        // Mode 3/A: "1234" octal → 0o1234 = 668
        assert_eq!(plot.mode_ac.mode_3a, Some(0o1234));
        // Flight level: 10_000 m * 3.28084 ≈ 32808 ft
        let fl = plot.mode_ac.flight_level_ft.expect("flight level present");
        assert!((fl - 10_000.0 * FEET_PER_METRE).abs() < 1.0);
    }

    /// On-ground aircraft are filtered out.
    #[test]
    fn on_ground_state_is_rejected() {
        let state = make_state(
            "aabbcc",
            None,
            Some(8.5),
            Some(50.0),
            Some(0.0),
            true,
            None,
            0,
        );
        assert!(parse_state(&state, SensorId(200), 0.0).is_none());
    }

    /// Missing latitude or longitude → no plot (we cannot locate the aircraft).
    #[test]
    fn missing_position_is_rejected() {
        let state = make_state("aabbcc", None, None, Some(50.0), None, false, None, 0);
        assert!(parse_state(&state, SensorId(200), 0.0).is_none(), "no lon");

        let state2 = make_state("aabbcc", None, Some(8.5), None, None, false, None, 0);
        assert!(parse_state(&state2, SensorId(200), 0.0).is_none(), "no lat");
    }

    /// Invalid ICAO24 hex string → no plot.
    #[test]
    fn invalid_icao24_is_rejected() {
        let state = make_state("GGGGGG", None, Some(8.5), Some(50.0), None, false, None, 0);
        assert!(parse_state(&state, SensorId(200), 0.0).is_none());
    }

    /// sigma_from_source maps position_source codes to the ADR 0019 table.
    #[test]
    fn sigma_from_source_follows_adr0019_table() {
        assert_eq!(sigma_from_source(0), 75.0, "ADS-B → 75 m");
        assert_eq!(sigma_from_source(1), 200.0, "ASTERIX → 200 m");
        assert_eq!(sigma_from_source(2), 200.0, "MLAT → 200 m");
        assert_eq!(sigma_from_source(99), 300.0, "unknown → 300 m");
    }

    /// Empty or whitespace-only callsign produces None (no spurious label).
    #[test]
    fn empty_callsign_produces_none() {
        let state = make_state(
            "aabbcc",
            Some("        "),
            Some(8.5),
            Some(50.0),
            None,
            false,
            None,
            0,
        );
        let plot = parse_state(&state, SensorId(200), 0.0).expect("should parse");
        assert!(
            plot.mode_ac.callsign.is_none(),
            "whitespace callsign → None"
        );
    }
}
