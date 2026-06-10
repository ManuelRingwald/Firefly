use serde::{Deserialize, Serialize};

use crate::ecef::Ecef;
use crate::polar::Polar;
use crate::wgs84::Wgs84;

/// A position in a local East-North-Up tangent-plane frame.
///
/// The frame is defined by a [`LocalFrame`] anchored at some reference point
/// (typically a radar site). +E points east, +N points (true) north, +U points
/// up along the local ellipsoidal normal. Units are metres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Enu {
    pub east: f64,
    pub north: f64,
    pub up: f64,
}

impl Enu {
    pub fn new(east: f64, north: f64, up: f64) -> Self {
        Self { east, north, up }
    }

    /// Convert this local position to a polar measurement as seen from the
    /// frame origin (slant range, azimuth from true north clockwise, elevation).
    pub fn to_polar(&self) -> Polar {
        let Enu { east, north, up } = *self;
        let range = (east * east + north * north + up * up).sqrt();
        // Azimuth measured clockwise from true north, in [0, 2π).
        let mut azimuth = east.atan2(north);
        if azimuth < 0.0 {
            azimuth += std::f64::consts::TAU;
        }
        let elevation = if range > 0.0 {
            (up / range).asin()
        } else {
            0.0
        };
        Polar {
            range,
            azimuth,
            elevation,
        }
    }
}

/// A local East-North-Up tangent-plane frame anchored at a geodetic reference
/// point. Holds the precomputed rotation between ECEF and ENU so that repeated
/// conversions (one per radar plot) are cheap.
#[derive(Debug, Clone, Copy)]
pub struct LocalFrame {
    origin_geodetic: Wgs84,
    origin_ecef: Ecef,
    // Rotation rows (ECEF -> ENU). Stored as the trig terms of the origin.
    sin_lat: f64,
    cos_lat: f64,
    sin_lon: f64,
    cos_lon: f64,
}

impl LocalFrame {
    /// Anchor a new local frame at the given geodetic origin.
    pub fn new(origin: Wgs84) -> Self {
        let (sin_lat, cos_lat) = origin.lat.sin_cos();
        let (sin_lon, cos_lon) = origin.lon.sin_cos();
        Self {
            origin_geodetic: origin,
            origin_ecef: origin.to_ecef(),
            sin_lat,
            cos_lat,
            sin_lon,
            cos_lon,
        }
    }

    /// The geodetic origin of this frame.
    pub fn origin(&self) -> Wgs84 {
        self.origin_geodetic
    }

    /// Project an ECEF position into this local ENU frame.
    pub fn ecef_to_enu(&self, p: &Ecef) -> Enu {
        let dx = p.x - self.origin_ecef.x;
        let dy = p.y - self.origin_ecef.y;
        let dz = p.z - self.origin_ecef.z;

        let east = -self.sin_lon * dx + self.cos_lon * dy;
        let north = -self.sin_lat * self.cos_lon * dx - self.sin_lat * self.sin_lon * dy
            + self.cos_lat * dz;
        let up =
            self.cos_lat * self.cos_lon * dx + self.cos_lat * self.sin_lon * dy + self.sin_lat * dz;

        Enu { east, north, up }
    }

    /// Lift a local ENU position back to ECEF (the transpose of the ENU rotation).
    pub fn enu_to_ecef(&self, p: &Enu) -> Ecef {
        let dx = -self.sin_lon * p.east - self.sin_lat * self.cos_lon * p.north
            + self.cos_lat * self.cos_lon * p.up;
        let dy = self.cos_lon * p.east - self.sin_lat * self.sin_lon * p.north
            + self.cos_lat * self.sin_lon * p.up;
        let dz = self.cos_lat * p.north + self.sin_lat * p.up;

        Ecef {
            x: self.origin_ecef.x + dx,
            y: self.origin_ecef.y + dy,
            z: self.origin_ecef.z + dz,
        }
    }

    /// Convenience: geodetic position -> local ENU.
    pub fn geodetic_to_enu(&self, p: &Wgs84) -> Enu {
        self.ecef_to_enu(&p.to_ecef())
    }

    /// Convenience: local ENU -> geodetic position.
    pub fn enu_to_geodetic(&self, p: &Enu) -> Wgs84 {
        self.enu_to_ecef(p).to_wgs84()
    }
}
