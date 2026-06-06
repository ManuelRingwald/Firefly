//! Geodesy and coordinate transforms for the Firefly radar tracker.
//!
//! Radar measurements are inherently *polar* (slant range, azimuth, elevation)
//! and referenced to a sensor site, while the world is naturally described in
//! *geodetic* WGS84 coordinates (latitude, longitude, height). Tracking, on the
//! other hand, is most convenient in a *local Cartesian* frame where motion is
//! (approximately) linear.
//!
//! This crate provides the three conversions that tie those frames together:
//!
//! ```text
//!   WGS84  <->  ECEF  <->  ENU (local tangent plane)  <->  Polar (radar)
//! ```
//!
//! All angles are in **radians** and all distances in **metres** unless a type
//! or field name says otherwise.

mod ecef;
mod enu;
mod polar;
mod wgs84;

pub use ecef::Ecef;
pub use enu::{Enu, LocalFrame};
pub use polar::Polar;
pub use wgs84::Wgs84;

/// WGS84 reference ellipsoid parameters.
pub mod ellipsoid {
    /// Semi-major axis (equatorial radius), metres.
    pub const SEMI_MAJOR_AXIS: f64 = 6_378_137.0;
    /// Flattening factor.
    pub const FLATTENING: f64 = 1.0 / 298.257_223_563;
    /// Semi-minor axis (polar radius), metres.
    pub const SEMI_MINOR_AXIS: f64 = SEMI_MAJOR_AXIS * (1.0 - FLATTENING);
    /// First eccentricity squared, `e^2 = f * (2 - f)`.
    pub const ECCENTRICITY_SQ: f64 = FLATTENING * (2.0 - FLATTENING);
}
