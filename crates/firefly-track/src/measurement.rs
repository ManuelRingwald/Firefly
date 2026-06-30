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
//! polar errors form a diagonal covariance `R_polar = diag(σ_ρ², σ_θ²)`, which
//! we transport through the polar→Cartesian map using that map's Jacobian `J`,
//! giving `R = J · R_polar · Jᵀ`. (A small long-range bias correction — the
//! "unbiased" variant — is a documented future refinement.)
//!
//! Elevation matters even for a 2-D tracker. The ground range is
//! `ρ = r · cos φ` (slant range `r`, elevation `φ`), so the *radial* (ground)
//! uncertainty has **two** sources — the slant-range noise *and* the elevation
//! noise — both projected onto the ground plane:
//! `σ_ρ² = (cos φ · σ_r)² + (r · sin φ · σ_φ)²`. For a target well above the
//! horizon the second term dominates (`r·sin φ` is a long lever arm), so
//! ignoring it badly under-estimates the radial uncertainty and makes the
//! validation gate far too tight. Because the elevation noise enters east/north
//! *only* through `ρ`, folding it into `σ_ρ²` and transporting `(ρ, θ)` through
//! `J` is exactly equivalent to the full 3→2 transport — no extra approximation.
//!
//! Frame note: the measurement lives in the **sensor's own** local east/north
//! frame. That is exactly right for the single-radar tracker of M2; a common
//! frame for several radars is a later (M4) concern.

use firefly_core::{Measurement, Plot};
use firefly_geo::{LocalFrame, Polar};
use nalgebra::{Matrix2, Vector2};
use serde::{Deserialize, Serialize};

/// The tracker's *assumed* model of a sensor's measurement noise.
///
/// Deliberately separate from the simulator's ground-truth noise: a real
/// tracker never knows the true error, it only believes a model (from a data
/// sheet or configuration). Keeping the two apart lets us later study what
/// happens when the believed model and reality disagree.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SensorErrorModel {
    /// Assumed slant-range noise (1σ), metres.
    pub sigma_range: f64,
    /// Assumed azimuth noise (1σ), radians.
    pub sigma_azimuth: f64,
    /// Assumed elevation noise (1σ), radians. Feeds the slant→ground projection
    /// and therefore the *radial* part of the converted covariance; set it to 0
    /// when the sensor reports no (or noise-free) elevation.
    pub sigma_elevation: f64,
}

impl SensorErrorModel {
    /// Full polar error model, all sigmas in SI (metres / radians).
    pub fn new(sigma_range: f64, sigma_azimuth: f64, sigma_elevation: f64) -> Self {
        Self {
            sigma_range,
            sigma_azimuth,
            sigma_elevation,
        }
    }

    /// Convenience constructor taking the azimuth sigma in degrees and assuming
    /// **no elevation noise** (`σ_φ = 0`). Suitable when the sensor delivers a
    /// clean elevation (or none, e.g. a target near the horizon); for a noisy
    /// elevation that feeds the ground projection use [`Self::from_polar_deg`].
    pub fn from_range_and_azimuth_deg(sigma_range: f64, sigma_azimuth_deg: f64) -> Self {
        Self {
            sigma_range,
            sigma_azimuth: sigma_azimuth_deg.to_radians(),
            sigma_elevation: 0.0,
        }
    }

    /// Full polar error model with the angle sigmas in degrees: slant-range
    /// (metres), azimuth (degrees), elevation (degrees).
    ///
    /// Use this when the radar reports a *noisy* elevation that feeds the
    /// slant→ground projection (e.g. en-route targets well above the horizon),
    /// so the converted covariance reflects the ground-range spread the
    /// elevation noise actually causes.
    pub fn from_polar_deg(
        sigma_range: f64,
        sigma_azimuth_deg: f64,
        sigma_elevation_deg: f64,
    ) -> Self {
        Self {
            sigma_range,
            sigma_azimuth: sigma_azimuth_deg.to_radians(),
            sigma_elevation: sigma_elevation_deg.to_radians(),
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
    let (sin_el, cos_el) = measurement.elevation.sin_cos();
    // Project the slant range onto the ground plane for 2-D horizontal tracking.
    let rho = measurement.range * cos_el;

    // Position: east = ρ sinθ, north = ρ cosθ.
    let z = Vector2::new(rho * sin_az, rho * cos_az);

    // Jacobian of (ρ, θ) -> (east, north):
    //   ∂east/∂ρ =  sinθ      ∂east/∂θ =  ρ cosθ
    //   ∂north/∂ρ = cosθ      ∂north/∂θ = -ρ sinθ
    // Matrix2::new is row-major: (m11, m12, m21, m22).
    let j = Matrix2::new(sin_az, rho * cos_az, cos_az, -rho * sin_az);

    // Radial (ground-range) variance, with both contributing noise sources
    // projected onto the ground plane (the elevation term via ρ = r cos φ):
    //   σ_ρ² = (cos φ · σ_r)² + (r · sin φ · σ_φ)².
    let var_ground_range = (cos_el * model.sigma_range).powi(2)
        + (measurement.range * sin_el * model.sigma_elevation).powi(2);

    // Polar errors are independent: diag(σ_ρ², σ_θ²).
    let r_polar = Matrix2::new(
        var_ground_range,
        0.0,
        0.0,
        model.sigma_azimuth * model.sigma_azimuth,
    );

    // Transport the uncertainty into Cartesian: R = J · R_polar · Jᵀ.
    let r = j * r_polar * j.transpose();

    CartesianMeasurement { z, r }
}

/// Turn **any** plot into a Cartesian measurement expressed in the common
/// `tracking_frame`, dispatching on its [`Measurement`] source.
///
/// - A **radar** polar reading goes through [`convert_plot`] in the sensor's own
///   frame (range-vs-angle covariance), then is lifted into the tracking frame
///   with [`LocalFrame::horizontal_from`] (height-correct multi-sensor fusion,
///   ADR 0010). This reproduces exactly the pre-ADS-B path.
/// - An **ADS-B** geodetic self-report is converted **directly** from WGS84 into
///   the tracking frame (no polar math, no sensor geometry): its position is
///   already world-referenced, and its uncertainty is *isotropic*
///   `R = σ² · I₂` from the reported NACp accuracy — not a tilted radar ellipse.
///   The sensor `frame`/`model` are unused for this variant.
///
/// REQ: FR-TRK-002, FR-TRK-030
pub fn tracking_measurement(
    plot: &Plot,
    sensor_frame: &LocalFrame,
    model: &SensorErrorModel,
    tracking_frame: &LocalFrame,
) -> CartesianMeasurement {
    match &plot.measurement {
        Measurement::Polar(polar) => {
            let local = convert_plot(polar, model);
            // `convert_plot` keeps only the ground-projected east/north; the
            // target's height above the sensor's tangent plane is
            // range·sin(elevation) — passing it lets `horizontal_from` lift the
            // full 3-D point into the tracking frame (no height-offset ghost).
            let height = polar.range * polar.elevation.sin();
            let (z, r) = tracking_frame.horizontal_from(sensor_frame, local.z, height, local.r);
            CartesianMeasurement { z, r }
        }
        Measurement::Geodetic {
            position,
            sigma_pos_m,
        } => {
            let enu = tracking_frame.geodetic_to_enu(position);
            let z = Vector2::new(enu.east, enu.north);
            let var = sigma_pos_m * sigma_pos_m;
            // Isotropic horizontal covariance: ADS-B position error is (to first
            // order) the same in every horizontal direction, unlike a radar's
            // range/azimuth split.
            let r = Matrix2::new(var, 0.0, 0.0, var);
            CartesianMeasurement { z, r }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::{DetectionKind, ModeAC, SensorId, SourceKind, Timestamp};
    use firefly_geo::Wgs84;
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

    /// Elevation noise inflates the *radial* (ground-range) variance for a
    /// target above the horizon, via ρ = r·cos φ. For a due-north target the
    /// radial direction is north, so the north-variance must grow by exactly the
    /// elevation term `(r·sin φ·σ_φ)²` on top of the projected range term.
    /// REQ: FR-TRK-002
    #[test]
    fn elevation_noise_inflates_radial_variance() {
        let sigma_range = 50.0;
        let sigma_elev_deg = 1.0;
        let model = SensorErrorModel::from_polar_deg(sigma_range, 0.08, sigma_elev_deg);

        let r = 73_000.0;
        let elev = 8.0_f64.to_radians();
        let cm = convert_plot(&Polar::new(r, 0.0, elev), &model); // due north
        let var_north = cm.r[(1, 1)]; // radial direction for az = 0

        let sigma_elev = sigma_elev_deg.to_radians();
        let expected = (elev.cos() * sigma_range).powi(2) + (r * elev.sin() * sigma_elev).powi(2);
        assert!((var_north - expected).abs() < 1e-6);

        // The elevation term dominates here: ~175 m vs ~49 m of slant projection,
        // so the modelled radial sigma is far larger than σ_range alone.
        assert!(var_north.sqrt() > 3.0 * sigma_range);

        // With no elevation noise the radial variance collapses to the projected
        // slant-range term (cos φ · σ_r)² — strictly less than σ_range².
        let no_elev = SensorErrorModel::from_range_and_azimuth_deg(sigma_range, 0.08);
        let var_north_clean = convert_plot(&Polar::new(r, 0.0, elev), &no_elev).r[(1, 1)];
        assert!((var_north_clean - (elev.cos() * sigma_range).powi(2)).abs() < 1e-6);
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

    fn adsb_plot(position: Wgs84, sigma_pos_m: f64) -> Plot {
        Plot {
            sensor: SensorId(200),
            time: Timestamp(0.0),
            measurement: Measurement::Geodetic {
                position,
                sigma_pos_m,
            },
            kind: DetectionKind::Secondary,
            source: SourceKind::AdsB,
            mode_ac: ModeAC::default(),
        }
    }

    /// ADS-B at the tracking frame's own origin → position (0, 0), covariance σ²·I₂.
    /// REQ: FR-TRK-030
    #[test]
    fn geodetic_at_origin_gives_zero_position_and_isotropic_covariance() {
        let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);
        let frame = LocalFrame::new(origin);
        let sigma = 30.0_f64;
        let plot = adsb_plot(origin, sigma);
        // sensor_frame and model are irrelevant for the Geodetic path
        let dummy_model = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.1);

        let cm = tracking_measurement(&plot, &frame, &dummy_model, &frame);

        assert!(
            cm.east().abs() < 1e-6,
            "east should be 0, got {}",
            cm.east()
        );
        assert!(
            cm.north().abs() < 1e-6,
            "north should be 0, got {}",
            cm.north()
        );
        let expected_var = sigma * sigma;
        assert!((cm.r[(0, 0)] - expected_var).abs() < 1e-9);
        assert!((cm.r[(1, 1)] - expected_var).abs() < 1e-9);
        // Isotropic: no east/north correlation.
        assert!(cm.r[(0, 1)].abs() < 1e-9);
    }

    /// A target 0.1° north of the tracking origin maps to positive north, ~0 east.
    /// REQ: FR-TRK-030
    #[test]
    fn geodetic_position_maps_north() {
        let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);
        let frame = LocalFrame::new(origin);
        // 0.1° latitude north ≈ 11.1 km
        let north_pos = Wgs84::from_degrees(48.1, 11.0, 0.0);
        let plot = adsb_plot(north_pos, 30.0);
        let dummy_model = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.1);

        let cm = tracking_measurement(&plot, &frame, &dummy_model, &frame);

        assert!(
            cm.north() > 10_000.0,
            "expected ~11 km north, got {:.0}",
            cm.north()
        );
        assert!(
            cm.east().abs() < 100.0,
            "east should be ~0 for same longitude, got {:.1}",
            cm.east()
        );
    }

    /// The Geodetic path ignores sensor_frame and model — two calls with
    /// different dummies must produce identical output.
    /// REQ: FR-TRK-030
    #[test]
    fn geodetic_output_independent_of_sensor_frame_and_model() {
        let origin = Wgs84::from_degrees(48.0, 11.0, 0.0);
        let frame = LocalFrame::new(origin);
        let target = Wgs84::from_degrees(48.05, 11.05, 1000.0);
        let plot = adsb_plot(target, 75.0);

        let frame_a = LocalFrame::new(Wgs84::from_degrees(47.0, 10.0, 0.0));
        let frame_b = LocalFrame::new(Wgs84::from_degrees(50.0, 15.0, 500.0));
        let model_a = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.1);
        let model_b = SensorErrorModel::from_range_and_azimuth_deg(200.0, 1.5);

        let cm_a = tracking_measurement(&plot, &frame_a, &model_a, &frame);
        let cm_b = tracking_measurement(&plot, &frame_b, &model_b, &frame);

        assert!((cm_a.east() - cm_b.east()).abs() < 1e-9);
        assert!((cm_a.north() - cm_b.north()).abs() < 1e-9);
        assert!((cm_a.r[(0, 0)] - cm_b.r[(0, 0)]).abs() < 1e-9);
    }
}
