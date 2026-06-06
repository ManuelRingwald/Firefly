//! Turning a polar radar plot into a Cartesian measurement with a covariance.
//!
//! A radar measures *polar* (slant range, azimuth, elevation), but the tracker
//! estimates motion in a *flat* local Cartesian frame (east = x, north = y),
//! where straight, level flight is a straight line and a linear Kalman filter
//! works cleanly.
//!
//! Converting the *position* is just trigonometry. The hard, important part is
//! converting the *uncertainty*. A radar is precise in range but coarse in
//! angle, and a small angular error becomes a large sideways error far away.
//! The resulting uncertainty is a tilted "cigar": narrow along the line of
//! sight, wide across it. The Kalman filter needs this as a 2×2 covariance
//! matrix `R` in the same Cartesian frame it works in, or it will weight
//! measurements wrongly.
//!
//! We obtain `R` with the classic **converted-measurement** approach: the
//! polar errors form a simple diagonal covariance `R_polar = diag(σ_ρ², σ_θ²)`,
//! which we transport through the polar→Cartesian map using that map's Jacobian
//! `J`, giving `R = J · R_polar · Jᵀ`. (A small long-range bias correction —
//! the "unbiased" variant — is a documented future refinement.)
//!
//! Frame note: the measurement lives in the **sensor's own** local east/north
//! frame. That is exactly right for the single-radar tracker of M2; a common
//! frame for several radars is a later (M4) concern.

use firefly_geo::Polar;
use nalgebra::{Matrix2, Vector2};

/// The tracker's *assumed* model of a sensor's measurement noise.
///
/// Deliberately separate from the simulator's ground-truth noise: a real
/// tracker never knows the true error, it only believes a model (from a data
/// sheet or configuration). Keeping the two apart lets us later study what
/// happens when the believed model and reality disagree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SensorErrorModel {
    /// Assumed ground-range noise (1σ), metres.
    pub sigma_range: f64,
    /// Assumed azimuth noise (1σ), radians.
    pub sigma_azimuth: f64,
}

impl SensorErrorModel {
    pub fn new(sigma_range: f64, sigma_azimuth: f64) -> Self {
        Self {
            sigma_range,
            sigma_azimuth,
        }
    }

    /// Convenience constructor taking the azimuth sigma in degrees.
    pub fn from_range_and_azimuth_deg(sigma_range: f64, sigma_azimuth_deg: f64) -> Self {
        Self {
            sigma_range,
            sigma_azimuth: sigma_azimuth_deg.to_radians(),
        }
    }
}

/// A radar plot expressed as a Cartesian position with its uncertainty.
///
/// `z` is the measured position `[east, north]` in metres; `r` is its 2×2
/// measurement-noise covariance in the same frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CartesianMeasurement {
    /// Measured position `[east, north]`, metres.
    pub z: Vector2<f64>,
    /// Measurement covariance `R`, m².
    pub r: Matrix2<f64>,
}

impl CartesianMeasurement {
    /// East component of the measured position, metres.
    pub fn east(&self) -> f64 {
        self.z.x
    }

    /// North component of the measured position, metres.
    pub fn north(&self) -> f64 {
        self.z.y
    }
}

/// Convert a polar measurement into a Cartesian measurement with covariance.
///
/// The polar reading is projected onto the local ground plane: ground range
/// `ρ = range · cos(elevation)`, azimuth `θ`. Then
/// `east = ρ·sin θ`, `north = ρ·cos θ`, and the covariance is propagated via
/// the Jacobian of that map.
///
/// REQ: FR-TRK-002
pub fn convert_plot(measurement: &Polar, model: &SensorErrorModel) -> CartesianMeasurement {
    let (sin_az, cos_az) = measurement.azimuth.sin_cos();
    // Project the slant range onto the ground plane for 2-D horizontal tracking.
    let rho = measurement.range * measurement.elevation.cos();

    // Position: east = ρ sinθ, north = ρ cosθ.
    let z = Vector2::new(rho * sin_az, rho * cos_az);

    // Jacobian of (ρ, θ) -> (east, north):
    //   ∂east/∂ρ =  sinθ      ∂east/∂θ =  ρ cosθ
    //   ∂north/∂ρ = cosθ      ∂north/∂θ = -ρ sinθ
    // Matrix2::new is row-major: (m11, m12, m21, m22).
    let j = Matrix2::new(sin_az, rho * cos_az, cos_az, -rho * sin_az);

    // Polar errors are independent: diag(σ_ρ², σ_θ²).
    let r_polar = Matrix2::new(
        model.sigma_range * model.sigma_range,
        0.0,
        0.0,
        model.sigma_azimuth * model.sigma_azimuth,
    );

    // Transport the uncertainty into Cartesian: R = J · R_polar · Jᵀ.
    let r = j * r_polar * j.transpose();

    CartesianMeasurement { z, r }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn model() -> SensorErrorModel {
        // 50 m range, 0.1° azimuth — a plausible radar.
        SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.1)
    }

    /// A target due north lands on the +north axis; due east on the +east axis.
    /// REQ: FR-TRK-002
    #[test]
    fn position_matches_geo_conventions() {
        let m = model();
        // Due north, 50 km, level.
        let north = convert_plot(&Polar::new(50_000.0, 0.0, 0.0), &m);
        assert!(north.east().abs() < 1e-6);
        assert!((north.north() - 50_000.0).abs() < 1e-6);

        // Due east, 50 km, level.
        let east = convert_plot(&Polar::new(50_000.0, PI / 2.0, 0.0), &m);
        assert!((east.east() - 50_000.0).abs() < 1e-6);
        assert!(east.north().abs() < 1e-6);
    }

    /// Slant range is projected onto the ground via the elevation angle.
    /// REQ: FR-TRK-002
    #[test]
    fn elevation_projects_to_ground_range() {
        let m = model();
        let elev = 30.0_f64.to_radians();
        let cm = convert_plot(&Polar::new(10_000.0, 0.0, elev), &m);
        // Ground range = 10000 * cos(30°) ≈ 8660.25 m, all in north.
        assert!((cm.north() - 10_000.0 * elev.cos()).abs() < 1e-6);
    }

    /// The "cigar": across the line of sight (cross-range) the uncertainty is
    /// far larger than along it (range). For a northbound target the radial
    /// direction is north, so north-variance is small, east-variance is large.
    /// REQ: FR-TRK-002
    #[test]
    fn covariance_is_cigar_shaped() {
        let m = model();
        let cm = convert_plot(&Polar::new(100_000.0, 0.0, 0.0), &m);
        let var_east = cm.r[(0, 0)];
        let var_north = cm.r[(1, 1)];

        // Range variance ≈ σ_range² = 2500 m².
        assert!((var_north - 50.0 * 50.0).abs() < 1e-6);
        // Cross-range variance ≈ (ρ·σ_az)² = (100000 · 0.1°)² ≈ (174.5 m)².
        let expected_cross = (100_000.0 * 0.1_f64.to_radians()).powi(2);
        assert!((var_east - expected_cross).abs() < 1e-3);
        // Cross-range dominates.
        assert!(var_east > 10.0 * var_north);
        // Axis-aligned for a due-north target ⇒ no correlation.
        assert!(cm.r[(0, 1)].abs() < 1e-6);
    }

    /// Cross-range positional uncertainty grows with range (∝ range²).
    /// REQ: FR-TRK-002
    #[test]
    fn cross_range_variance_grows_with_range() {
        let m = model();
        let near = convert_plot(&Polar::new(50_000.0, 0.0, 0.0), &m).r[(0, 0)];
        let far = convert_plot(&Polar::new(100_000.0, 0.0, 0.0), &m).r[(0, 0)];
        // Doubling the range quadruples the cross-range variance.
        assert!((far / near - 4.0).abs() < 1e-6);
    }

    /// `R` must be a valid covariance: symmetric and positive definite.
    /// REQ: FR-TRK-002
    #[test]
    fn covariance_is_symmetric_and_positive_definite() {
        let m = model();
        // An off-axis bearing so the ellipse is tilted (non-zero correlation).
        let cm = convert_plot(&Polar::new(80_000.0, 1.0, 0.2), &m);
        // Symmetry.
        assert!((cm.r[(0, 1)] - cm.r[(1, 0)]).abs() < 1e-9);
        assert!(
            cm.r[(0, 1)].abs() > 0.0,
            "tilted ellipse should correlate x,y"
        );
        // Positive definite: positive diagonal and positive determinant.
        assert!(cm.r[(0, 0)] > 0.0 && cm.r[(1, 1)] > 0.0);
        assert!(cm.r.determinant() > 0.0);
    }
}
