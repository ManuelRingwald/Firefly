//! Mapping from a decoded [`OgnPosition`] to a firefly-core [`Plot`] (ADR 0026 §5/§6).

use firefly_core::{ModeAC, Plot, SensorId, Timestamp};
use firefly_geo::Wgs84;

use crate::ogn::{AddressType, OgnPosition};

/// Metres per foot (altitude conversion).
const METRES_PER_FOOT: f64 = 0.3048;

/// Seconds in a UTC day (the OGN timestamp is a time-of-day; anchoring it needs
/// the day length).
const SECONDS_PER_DAY: f64 = 86_400.0;

/// Convert an OGN time-of-day (seconds since UTC midnight, from the APRS `HHMMSSh`
/// field) into a **full Unix-epoch timestamp**, anchored to the UTC day of
/// `now_unix_s` (the wall-clock receive time).
///
/// This is the fix for the combined ADS-B+FLARM feed (Wayfinder #120): the
/// tracker fuses all live sources under a **single, monotonic data-time
/// watermark** (`firefly-server::live`), and OpenSky already stamps plots with
/// Unix-epoch seconds (`resp.time`). FLARM previously stamped raw
/// seconds-since-midnight (0..86400), so once an OpenSky plot advanced the
/// watermark to epoch time (~1.7e9 s) every FLARM plot looked billions of seconds
/// "in the past" and was dropped as out-of-order — a combined feed silently lost
/// its FLARM (and could stall entirely). Emitting FLARM on the same epoch clock
/// aligns both sources.
///
/// A `None` time-of-day (a beacon without the `h` timestamp) falls back to the
/// receive time. A day-boundary skew (a beacon stamped just before midnight but
/// received just after, or vice versa) is corrected by snapping to the nearest
/// day so the epoch never lands ~24 h away from the receive instant.
fn ogn_epoch_seconds(time_of_day_s: Option<f64>, now_unix_s: f64) -> f64 {
    let tod = match time_of_day_s {
        Some(t) => t,
        None => return now_unix_s,
    };
    let midnight = now_unix_s - now_unix_s.rem_euclid(SECONDS_PER_DAY);
    let mut t = midnight + tod;
    if t - now_unix_s > SECONDS_PER_DAY / 2.0 {
        t -= SECONDS_PER_DAY; // beacon from the previous UTC day
    } else if now_unix_s - t > SECONDS_PER_DAY / 2.0 {
        t += SECONDS_PER_DAY; // beacon from the next UTC day (clock skew)
    }
    t
}

/// Convert an [`OgnPosition`] into a [`Plot`] the tracker can fuse directly.
///
/// The report is a **cooperative geodetic self-report** — like ADS-B — so it maps
/// to a [`Measurement::Geodetic`](firefly_core::Measurement) with an **isotropic**
/// 1σ accuracy (`sigma_pos_m`), and to [`Plot::adsb`]'s secondary detection kind
/// (never a primary skin paint).
///
/// Per ADR 0026 §4, the ICAO address field is populated **only** when the OGN
/// address type is [`AddressType::Icao`]; FLARM and OGN-tracker ids are private
/// ranges and must not masquerade as ICAO 24-bit addresses (that would corrupt
/// the tracker's ICAO-based pre-sort and the eventual `flarm` provenance, #30).
pub fn position_to_plot(
    pos: &OgnPosition,
    sensor: SensorId,
    sigma_pos_m: f64,
    now_unix_s: f64,
) -> Plot {
    let alt_m = pos
        .altitude_ft
        .map(|ft| ft * METRES_PER_FOOT)
        .unwrap_or(0.0);
    let position = Wgs84::from_degrees(pos.latitude_deg, pos.longitude_deg, alt_m);

    let icao_address = match pos.address_type {
        AddressType::Icao => pos.address,
        _ => None,
    };

    let mode_ac = ModeAC {
        mode_3a: None,
        flight_level_ft: pos.altitude_ft,
        icao_address,
        callsign: None,
    };

    // Data time = full Unix-epoch seconds, on the same clock OpenSky uses, so the
    // shared live data-time watermark fuses both sources (Wayfinder #120). The
    // OGN beacon carries only a time-of-day, anchored here to the receive day.
    let time = Timestamp(ogn_epoch_seconds(pos.time_of_day_s, now_unix_s));

    Plot::flarm(sensor, time, position, sigma_pos_m, mode_ac)
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::Measurement;

    fn position(address_type: AddressType, address: Option<u32>) -> OgnPosition {
        OgnPosition {
            source_call: "FLRDDE626".to_string(),
            time_of_day_s: Some(27948.0),
            latitude_deg: 51.18867,
            longitude_deg: -1.034,
            altitude_ft: Some(607.0),
            address,
            address_type,
            aircraft_type: Some(2),
        }
    }

    // A fixed wall-clock anchor for the tests: 2026-07-02T07:46:00Z. Its
    // seconds-since-midnight is 07*3600 + 46*60 = 27960, two seconds after the
    // sample beacon's 27948 (07:45:48).
    const NOW_UNIX_S: f64 = 1_782_027_960.0;

    #[test]
    fn flarm_address_does_not_populate_icao() {
        let plot = position_to_plot(
            &position(AddressType::Flarm, Some(0xDDE626)),
            SensorId(210),
            20.0,
            NOW_UNIX_S,
        );
        assert!(
            plot.mode_ac.icao_address.is_none(),
            "FLARM id must not become an ICAO address"
        );
        assert_eq!(plot.mode_ac.flight_level_ft, Some(607.0));
        assert_eq!(plot.sensor, SensorId(210));
    }

    #[test]
    fn icao_address_populates_icao_field() {
        let plot = position_to_plot(
            &position(AddressType::Icao, Some(0x4B4E68)),
            SensorId(210),
            20.0,
            NOW_UNIX_S,
        );
        assert_eq!(plot.mode_ac.icao_address, Some(0x4B4E68));
    }

    // Wayfinder #120: the plot time must be a full Unix-epoch timestamp on the
    // same clock as OpenSky, not raw seconds-since-midnight, so the shared live
    // data-time watermark fuses both sources instead of dropping FLARM.
    #[test]
    fn plot_time_is_unix_epoch_anchored_to_the_receive_day() {
        let plot = position_to_plot(
            &position(AddressType::Flarm, Some(1)),
            SensorId(210),
            20.0,
            NOW_UNIX_S,
        );
        // 27948 s-of-day anchored to NOW_UNIX_S's UTC day = NOW - 12 s.
        assert!((plot.time.as_secs() - (NOW_UNIX_S - 12.0)).abs() < 1e-6);
        assert!(
            plot.time.as_secs() > 1_700_000_000.0,
            "must be epoch, not ToD"
        );
    }

    #[test]
    fn missing_beacon_time_falls_back_to_receive_time() {
        let mut pos = position(AddressType::Flarm, Some(1));
        pos.time_of_day_s = None;
        let plot = position_to_plot(&pos, SensorId(210), 20.0, NOW_UNIX_S);
        assert_eq!(plot.time.as_secs(), NOW_UNIX_S);
    }

    #[test]
    fn day_boundary_beacon_before_midnight_received_after_snaps_back() {
        // Beacon stamped 23:59:50 (86390 s) but received 5 s after midnight.
        let just_after_midnight = NOW_UNIX_S - NOW_UNIX_S.rem_euclid(SECONDS_PER_DAY) + 5.0;
        let t = ogn_epoch_seconds(Some(86_390.0), just_after_midnight);
        // Correct epoch is 15 s before the receive instant (previous day), not ~24 h ahead.
        assert!((t - (just_after_midnight - 15.0)).abs() < 1e-6);
    }

    #[test]
    fn measurement_is_geodetic_with_the_configured_sigma() {
        let plot = position_to_plot(
            &position(AddressType::Flarm, Some(1)),
            SensorId(210),
            17.5,
            NOW_UNIX_S,
        );
        match plot.measurement {
            Measurement::Geodetic {
                sigma_pos_m,
                position,
            } => {
                assert!((sigma_pos_m - 17.5).abs() < 1e-12);
                // The stored WGS84 position round-trips out of a geodetic measurement
                // unchanged (the frame is ignored for an already-geodetic report).
                assert_eq!(
                    Measurement::Geodetic {
                        sigma_pos_m,
                        position
                    },
                    plot.measurement
                );
            }
            other => panic!("expected geodetic measurement, got {other:?}"),
        }
    }
}
