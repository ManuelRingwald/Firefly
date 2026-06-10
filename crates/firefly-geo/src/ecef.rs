use serde::{Deserialize, Serialize};

use crate::ellipsoid::{ECCENTRICITY_SQ, SEMI_MAJOR_AXIS, SEMI_MINOR_AXIS};
use crate::wgs84::Wgs84;

/// A position in the Earth-Centered, Earth-Fixed (ECEF) Cartesian frame.
///
/// The origin is the Earth's centre of mass, +X points to the intersection of
/// the equator and the prime meridian, +Z points to the (geodetic) north pole,
/// and +Y completes the right-handed frame. Units are metres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ecef {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Ecef {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Convert back to geodetic WGS84 coordinates.
    ///
    /// Uses Bowring's closed-form solution, which is accurate to well below a
    /// millimetre for any altitude of interest to air surveillance and needs no
    /// iteration.
    pub fn to_wgs84(&self) -> Wgs84 {
        let Ecef { x, y, z } = *self;

        let lon = y.atan2(x);

        let a = SEMI_MAJOR_AXIS;
        let b = SEMI_MINOR_AXIS;
        // Second eccentricity squared.
        let ep_sq = (a * a - b * b) / (b * b);

        let p = (x * x + y * y).sqrt();

        // Bowring's auxiliary parametric latitude.
        let theta = (z * a).atan2(p * b);
        let (sin_theta, cos_theta) = theta.sin_cos();

        let lat =
            (z + ep_sq * b * sin_theta.powi(3)).atan2(p - ECCENTRICITY_SQ * a * cos_theta.powi(3));
        let (sin_lat, cos_lat) = lat.sin_cos();

        let n = a / (1.0 - ECCENTRICITY_SQ * sin_lat * sin_lat).sqrt();

        // Choose the numerically better-conditioned height formula depending on
        // how close we are to the poles.
        let height = if cos_lat.abs() > 1e-9 {
            p / cos_lat - n
        } else {
            z / sin_lat - n * (1.0 - ECCENTRICITY_SQ)
        };

        Wgs84 { lat, lon, height }
    }
}
