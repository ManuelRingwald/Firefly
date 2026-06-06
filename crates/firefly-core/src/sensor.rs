use firefly_geo::{LocalFrame, Wgs84};

use crate::ids::SensorId;

/// A surveillance sensor: a radar site with a known geodetic position and a
/// local tangent-plane frame in which its measurements are expressed.
#[derive(Debug, Clone, Copy)]
pub struct Sensor {
    pub id: SensorId,
    position: Wgs84,
    frame: LocalFrame,
}

impl Sensor {
    /// Create a sensor anchored at a geodetic position.
    pub fn new(id: SensorId, position: Wgs84) -> Self {
        Self {
            id,
            position,
            frame: LocalFrame::new(position),
        }
    }

    /// Geodetic position of the sensor site.
    pub fn position(&self) -> Wgs84 {
        self.position
    }

    /// The local ENU frame anchored at this sensor.
    pub fn frame(&self) -> &LocalFrame {
        &self.frame
    }
}
