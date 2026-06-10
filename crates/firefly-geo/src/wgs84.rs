use serde::{Deserialize, Serialize};

use crate::ecef::Ecef;
use crate::ellipsoid::{ECCENTRICITY_SQ, SEMI_MAJOR_AXIS};

/// A geodetic position on the WGS84 ellipsoid.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Wgs84 {
    /// Geodetic latitude, radians (positive north).
    pub lat: f64,
    /// Geodetic longitude, radians (positive east).
    pub lon: f64,
    /// Height above the ellipsoid, metres.
    pub height: f64,
}

impl Wgs84 {
    /// Construct from radians.
    pub fn new(lat: f64, lon: f64, height: f64) -> Self {
        Self { lat, lon, height }
    }

    /// Construct from decimal degrees (latitude, longitude) and metres of height.
    pub fn from_degrees(lat_deg: f64, lon_deg: f64, height: f64) -> Self {
        Self {
            lat: lat_deg.to_radians(),
            lon: lon_deg.to_radians(),
            height,
        }
    }

    /// Latitude in decimal degrees.
    pub fn lat_deg(&self) -> f64 {
        self.lat.to_degrees()
    }

    /// Longitude in decimal degrees.
    pub fn lon_deg(&self) -> f64 {
        self.lon.to_degrees()
    }

    /// Convert to Earth-Centered, Earth-Fixed (ECEF) coordinates.
    pub fn to_ecef(&self) -> Ecef {
        let (sin_lat, cos_lat) = self.lat.sin_cos();
        let (sin_lon, cos_lon) = self.lon.sin_cos();

        // Prime vertical radius of curvature.
        let n = SEMI_MAJOR_AXIS / (1.0 - ECCENTRICITY_SQ * sin_lat * sin_lat).sqrt();

        let x = (n + self.height) * cos_lat * cos_lon;
        let y = (n + self.height) * cos_lat * sin_lon;
        let z = (n * (1.0 - ECCENTRICITY_SQ) + self.height) * sin_lat;

        Ecef { x, y, z }
    }
}
