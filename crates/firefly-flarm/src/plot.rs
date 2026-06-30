//! Mapping from a decoded [`OgnPosition`] to a firefly-core [`Plot`] (ADR 0026 §5/§6).

use firefly_core::{ModeAC, Plot, SensorId, Timestamp};
use firefly_geo::Wgs84;

use crate::ogn::{AddressType, OgnPosition};

/// Metres per foot (altitude conversion).
const METRES_PER_FOOT: f64 = 0.3048;

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
pub fn position_to_plot(pos: &OgnPosition, sensor: SensorId, sigma_pos_m: f64) -> Plot {
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

    // Data time = seconds since UTC midnight (the tracker's UTC-ToD basis); the
    // integration layer reconciles the time origin (Schritt C).
    let time = Timestamp(pos.time_of_day_s.unwrap_or(0.0));

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

    #[test]
    fn flarm_address_does_not_populate_icao() {
        let plot = position_to_plot(
            &position(AddressType::Flarm, Some(0xDDE626)),
            SensorId(210),
            20.0,
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
        );
        assert_eq!(plot.mode_ac.icao_address, Some(0x4B4E68));
    }

    #[test]
    fn measurement_is_geodetic_with_the_configured_sigma() {
        let plot = position_to_plot(&position(AddressType::Flarm, Some(1)), SensorId(210), 17.5);
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
