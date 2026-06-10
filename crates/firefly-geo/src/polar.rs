use serde::{Deserialize, Serialize};

use crate::enu::Enu;

/// A polar measurement as produced by a radar, referenced to its site.
///
/// This is the natural coordinate system of a rotating surveillance radar: a
/// slant range from the antenna, an azimuth (the antenna's pointing angle), and
/// an elevation angle (available from 3-D radars; primary 2-D radars leave it
/// effectively unknown).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Polar {
    /// Slant range from the sensor, metres.
    pub range: f64,
    /// Azimuth measured clockwise from true north, radians in [0, 2π).
    pub azimuth: f64,
    /// Elevation angle above the local horizon, radians.
    pub elevation: f64,
}

impl Polar {
    pub fn new(range: f64, azimuth: f64, elevation: f64) -> Self {
        Self {
            range,
            azimuth,
            elevation,
        }
    }

    /// Azimuth in decimal degrees, [0, 360).
    pub fn azimuth_deg(&self) -> f64 {
        self.azimuth.to_degrees()
    }

    /// Elevation in decimal degrees.
    pub fn elevation_deg(&self) -> f64 {
        self.elevation.to_degrees()
    }

    /// Convert back to a local ENU position relative to the sensor site.
    pub fn to_enu(&self) -> Enu {
        let (sin_az, cos_az) = self.azimuth.sin_cos();
        let (sin_el, cos_el) = self.elevation.sin_cos();
        let horizontal = self.range * cos_el;
        Enu {
            east: horizontal * sin_az,
            north: horizontal * cos_az,
            up: self.range * sin_el,
        }
    }
}
