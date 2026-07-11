//! Regional QNH lookup (VERT.1).
//!
//! A **QNH region** is a point (typically an airfield or a met region's
//! centre) with the QNH observed there and an optional applicability radius.
//! [`QnhService::lookup`] picks the **nearest applicable** region for a
//! position; when none applies, the result is the ICAO **standard
//! atmosphere** — explicitly flagged as such, because "no data" and
//! "measured 1013 hPa" are operationally different statements.

use crate::altitude::ISA_STANDARD_PRESSURE_HPA;

/// Mean Earth radius, metres (IUGG) — for the great-circle distance.
const EARTH_RADIUS_M: f64 = 6_371_008.8;
/// Nautical mile in metres.
const METRES_PER_NM: f64 = 1852.0;

/// One QNH region: a centre point, the QNH observed there and an optional
/// applicability radius (absent = applies at any distance, competing only by
/// proximity).
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct QnhRegion {
    /// Human-readable region name (e.g. the airfield ICAO code) — used as
    /// the metric label and in logs.
    pub name: String,
    /// Region centre latitude, degrees.
    pub lat: f64,
    /// Region centre longitude, degrees.
    pub lon: f64,
    /// Applicability radius in nautical miles; absent = unbounded.
    #[serde(default)]
    pub radius_nm: Option<f64>,
    /// The QNH observed for this region, hectopascal.
    pub qnh_hpa: f64,
}

/// Where a looked-up QNH value came from — the consumer must be able to tell
/// a measured regional value from the standard-atmosphere fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QnhSource {
    /// A configured region's observed QNH (index into the service's regions).
    Region(usize),
    /// No region applied — ICAO standard atmosphere (1013.25 hPa). NOT a
    /// measurement; below the transition altitude this means the altitude
    /// stays an uncorrected pressure altitude.
    StandardAtmosphere,
}

/// A looked-up QNH: the value and, honestly, where it came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Qnh {
    /// The QNH in hectopascal.
    pub hpa: f64,
    /// Provenance of the value.
    pub source: QnhSource,
}

impl Qnh {
    /// Whether this is a real regional observation (vs the standard-
    /// atmosphere fallback).
    pub fn is_observed(&self) -> bool {
        matches!(self.source, QnhSource::Region(_))
    }
}

/// The regional QNH lookup service. Pure and immutable once built — the
/// env-driven provider (VERT.1) constructs it at startup; a live provider
/// would rebuild/swap it on each update cycle.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct QnhService {
    regions: Vec<QnhRegion>,
}

impl QnhService {
    /// Build the service over a validated region list (see
    /// [`MeteoConfig`](crate::MeteoConfig) for the validating constructor).
    pub fn new(regions: Vec<QnhRegion>) -> Self {
        Self { regions }
    }

    /// The configured regions, in configuration order.
    pub fn regions(&self) -> &[QnhRegion] {
        &self.regions
    }

    /// Whether any region is configured at all.
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// Look up the QNH applicable at a position: the **nearest** region
    /// whose applicability radius (if any) contains the position. No
    /// applicable region → the standard atmosphere, flagged as such.
    pub fn lookup(&self, lat_deg: f64, lon_deg: f64) -> Qnh {
        let nearest = self
            .regions
            .iter()
            .enumerate()
            .filter_map(|(index, region)| {
                let distance_m = great_circle_m(lat_deg, lon_deg, region.lat, region.lon);
                let applies = match region.radius_nm {
                    Some(radius) => distance_m <= radius * METRES_PER_NM,
                    None => true,
                };
                applies.then_some((index, distance_m))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));

        match nearest {
            Some((index, _)) => Qnh {
                hpa: self.regions[index].qnh_hpa,
                source: QnhSource::Region(index),
            },
            None => Qnh {
                hpa: ISA_STANDARD_PRESSURE_HPA,
                source: QnhSource::StandardAtmosphere,
            },
        }
    }
}

/// Great-circle distance between two WGS84 points, metres (haversine on the
/// mean sphere — metre-level exactness is irrelevant for region selection).
fn great_circle_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (phi1, phi2) = (lat1.to_radians(), lat2.to_radians());
    let d_phi = (lat2 - lat1).to_radians();
    let d_lambda = (lon2 - lon1).to_radians();
    let a = (d_phi / 2.0).sin().powi(2) + phi1.cos() * phi2.cos() * (d_lambda / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(name: &str, lat: f64, lon: f64, radius_nm: Option<f64>, qnh_hpa: f64) -> QnhRegion {
        QnhRegion {
            name: name.into(),
            lat,
            lon,
            radius_nm,
            qnh_hpa,
        }
    }

    /// The nearest applicable region wins; its provenance names the region.
    /// REQ: FR-TRK-041
    #[test]
    fn nearest_applicable_region_wins() {
        let service = QnhService::new(vec![
            region("EDDF", 50.03, 8.57, Some(60.0), 1008.0),
            region("EDDK", 50.87, 7.14, Some(60.0), 1011.0),
        ]);

        // Close to Frankfurt.
        let qnh = service.lookup(50.0, 8.6);
        assert_eq!(qnh.hpa, 1008.0);
        assert_eq!(qnh.source, QnhSource::Region(0));
        assert!(qnh.is_observed());

        // Close to Cologne.
        let qnh = service.lookup(50.9, 7.2);
        assert_eq!(qnh.hpa, 1011.0);
        assert_eq!(qnh.source, QnhSource::Region(1));
    }

    /// Outside every applicability radius the standard atmosphere applies —
    /// flagged, never claimed as an observation. REQ: FR-TRK-041
    #[test]
    fn outside_all_radii_falls_back_to_standard_atmosphere() {
        let service = QnhService::new(vec![region("EDDF", 50.03, 8.57, Some(60.0), 1008.0)]);
        // Munich is ~160 NM from Frankfurt — outside the 60 NM radius.
        let qnh = service.lookup(48.35, 11.79);
        assert_eq!(qnh.hpa, ISA_STANDARD_PRESSURE_HPA);
        assert_eq!(qnh.source, QnhSource::StandardAtmosphere);
        assert!(!qnh.is_observed());

        // An empty service always answers with the standard atmosphere.
        let empty = QnhService::default();
        assert!(empty.is_empty());
        assert!(!empty.lookup(50.0, 8.0).is_observed());
    }

    /// A region without a radius applies everywhere but still competes by
    /// proximity with bounded regions. REQ: FR-TRK-041
    #[test]
    fn unbounded_region_applies_everywhere_but_proximity_decides() {
        let service = QnhService::new(vec![
            // A country-wide unbounded default plus a bounded local region.
            region("COUNTRY", 51.0, 10.0, None, 1015.0),
            region("EDDF", 50.03, 8.57, Some(60.0), 1008.0),
        ]);

        // Far from Frankfurt: the unbounded region answers.
        let qnh = service.lookup(53.5, 10.0);
        assert_eq!(qnh.hpa, 1015.0);

        // On the Frankfurt doorstep the local region is nearer.
        let qnh = service.lookup(50.0, 8.6);
        assert_eq!(qnh.hpa, 1008.0);
    }
}
