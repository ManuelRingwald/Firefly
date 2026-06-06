use firefly_geo::Polar;

use crate::ids::SensorId;
use crate::time::Timestamp;

/// Which radar channel produced a detection.
///
/// A *primary* radar (PSR) sees a skin reflection — it gives position but no
/// identity or barometric height. A *secondary* radar (SSR) interrogates a
/// transponder and gets Mode A/C/S replies. In practice a plot is often the
/// *combined* result of correlating a primary return with an SSR reply in the
/// same beam dwell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionKind {
    /// Primary surveillance radar only (skin paint).
    Primary,
    /// Secondary surveillance radar only (transponder reply).
    Secondary,
    /// Correlated primary + secondary in the same dwell.
    Combined,
}

impl DetectionKind {
    pub fn has_primary(self) -> bool {
        matches!(self, DetectionKind::Primary | DetectionKind::Combined)
    }

    pub fn has_secondary(self) -> bool {
        matches!(self, DetectionKind::Secondary | DetectionKind::Combined)
    }
}

/// Secondary-radar replies attached to a plot.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ModeAC {
    /// Mode 3/A identity code (the 4-digit octal "squawk"), if replied.
    pub mode_3a: Option<u16>,
    /// Mode C pressure altitude in feet (barometric, 1013.25 hPa datum), if replied.
    pub flight_level_ft: Option<f64>,
    /// Mode S 24-bit ICAO aircraft address, if available.
    pub icao_address: Option<u32>,
}

/// A single radar detection ("plot") for one target on one scan of one sensor.
///
/// The measurement is stored in the sensor's native polar frame; converting it
/// into a tracking frame is the tracker's job, because the choice of frame and
/// the measurement-noise model belong to the estimator, not to the detection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plot {
    /// Sensor that produced this plot.
    pub sensor: SensorId,
    /// Time of detection.
    pub time: Timestamp,
    /// Measured position in the sensor's polar frame.
    pub measurement: Polar,
    /// What kind of detection this is.
    pub kind: DetectionKind,
    /// Secondary-radar data, present when [`DetectionKind::has_secondary`].
    pub mode_ac: ModeAC,
}

impl Plot {
    /// Construct a bare primary plot (no SSR data).
    pub fn primary(sensor: SensorId, time: Timestamp, measurement: Polar) -> Self {
        Self {
            sensor,
            time,
            measurement,
            kind: DetectionKind::Primary,
            mode_ac: ModeAC::default(),
        }
    }
}
