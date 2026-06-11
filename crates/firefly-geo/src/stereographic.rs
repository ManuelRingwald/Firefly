use std::f64::consts::FRAC_PI_4;

use serde::{Deserialize, Serialize};

use crate::ellipsoid::ECCENTRICITY_SQ;
use crate::wgs84::Wgs84;

/// Number of fixed-point iterations used to invert the conformal-latitude
/// mapping. The series converges extremely fast (sub-millimetre after 3-4
/// steps for any latitude); a fixed count avoids a convergence-check branch.
const INVERSE_ITERATIONS: usize = 5;

/// A *system-stereographic* projection: a conformal (angle-preserving) map of
/// the WGS84 ellipsoid onto a plane, tangent at one fixed reference point.
///
/// This is the "double" or "Gauss" stereographic construction used by
/// EUROCONTROL/ARTAS and by CAT062 Item I062/100: the ellipsoid is first
/// mapped *conformally* onto an auxiliary sphere (the "Gaussian sphere" of
/// curvature radius `R0`, fitted at the reference point), and that sphere is
/// then projected onto the tangent plane with the classical (spherical)
/// oblique stereographic projection (Snyder, *Map Projections — A Working
/// Manual*, formulae (3-1), (21-4), (21-8), (21-9), (21-14), (21-15)).
///
/// The projection is purely horizontal: it maps geodetic latitude/longitude
/// to plane X/Y in metres. Height is not involved and passes through
/// unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StereographicProjection {
    ref_lon: f64,
    /// Conformal latitude of the reference point, `χ0`.
    chi0: f64,
    sin_chi0: f64,
    cos_chi0: f64,
    /// Radius of the Gaussian conformal sphere fitted at the reference point.
    sphere_radius: f64,
}

impl StereographicProjection {
    /// Construct a projection tangent at `reference` (the system reference
    /// point, e.g. the tracking frame's origin). Only `reference`'s latitude
    /// and longitude are used; height does not affect a horizontal
    /// projection.
    pub fn new(reference: Wgs84) -> Self {
        let e = ECCENTRICITY_SQ.sqrt();
        let chi0 = conformal_latitude(reference.lat, e);
        let (sin_chi0, cos_chi0) = chi0.sin_cos();

        Self {
            ref_lon: reference.lon,
            chi0,
            sin_chi0,
            cos_chi0,
            sphere_radius: gaussian_sphere_radius(reference.lat),
        }
    }

    /// Project a geodetic position onto the system-stereographic plane,
    /// returning `(x, y)` in metres relative to the reference point. `x`
    /// increases eastward, `y` northward — matching CAT062 I062/100's
    /// X/Y-component convention.
    pub fn project(&self, p: Wgs84) -> (f64, f64) {
        let e = ECCENTRICITY_SQ.sqrt();
        let chi = conformal_latitude(p.lat, e);
        let (sin_chi, cos_chi) = chi.sin_cos();
        let dlon = p.lon - self.ref_lon;
        let (sin_dlon, cos_dlon) = dlon.sin_cos();

        let denom = 1.0 + self.sin_chi0 * sin_chi + self.cos_chi0 * cos_chi * cos_dlon;
        let k = 2.0 * self.sphere_radius / denom;

        let x = k * cos_chi * sin_dlon;
        let y = k * (self.cos_chi0 * sin_chi - self.sin_chi0 * cos_chi * cos_dlon);
        (x, y)
    }

    /// Invert the projection: given plane coordinates `(x, y)` in metres and
    /// a height (carried through unchanged, since the projection is purely
    /// horizontal), recover the geodetic position.
    pub fn unproject(&self, x: f64, y: f64, height: f64) -> Wgs84 {
        let e = ECCENTRICITY_SQ.sqrt();
        let rho = (x * x + y * y).sqrt();

        let (chi, lon) = if rho < 1e-9 {
            (self.chi0, self.ref_lon)
        } else {
            let c = 2.0 * (rho / (2.0 * self.sphere_radius)).atan();
            let (sin_c, cos_c) = c.sin_cos();

            let chi = (cos_c * self.sin_chi0 + (y * sin_c * self.cos_chi0) / rho).asin();
            let lon = self.ref_lon
                + (x * sin_c).atan2(rho * self.cos_chi0 * cos_c - y * self.sin_chi0 * sin_c);
            (chi, lon)
        };

        Wgs84::new(geodetic_latitude(chi, e), lon, height)
    }
}

/// Radius of the Gaussian conformal sphere fitted at geodetic latitude `phi`:
/// `R0 = sqrt(M * N)`, the geometric mean of the meridian radius of curvature
/// `M` and the prime-vertical radius of curvature `N`. This simplifies to
/// `a * sqrt(1 - e^2) / (1 - e^2 sin^2(phi))`.
fn gaussian_sphere_radius(phi: f64) -> f64 {
    use crate::ellipsoid::SEMI_MAJOR_AXIS;
    let sin_phi = phi.sin();
    SEMI_MAJOR_AXIS * (1.0 - ECCENTRICITY_SQ).sqrt() / (1.0 - ECCENTRICITY_SQ * sin_phi * sin_phi)
}

/// Conformal (isometric) latitude `χ` for geodetic latitude `phi` on the
/// WGS84 ellipsoid (eccentricity `e`): the latitude on the auxiliary sphere
/// that preserves angles under the ellipsoid-to-sphere mapping.
fn conformal_latitude(phi: f64, e: f64) -> f64 {
    let sin_phi = phi.sin();
    let isometric_factor = ((1.0 - e * sin_phi) / (1.0 + e * sin_phi)).powf(e / 2.0);
    2.0 * ((FRAC_PI_4 + phi / 2.0).tan() * isometric_factor).atan() - std::f64::consts::FRAC_PI_2
}

/// Inverse of [`conformal_latitude`]: recover geodetic latitude `phi` from
/// conformal latitude `chi`, by fixed-point iteration (the map is a
/// contraction for any latitude, so a handful of iterations suffice).
fn geodetic_latitude(chi: f64, e: f64) -> f64 {
    let mut phi = chi;
    for _ in 0..INVERSE_ITERATIONS {
        let sin_phi = phi.sin();
        let isometric_factor = ((1.0 + e * sin_phi) / (1.0 - e * sin_phi)).powf(e / 2.0);
        phi = 2.0 * ((FRAC_PI_4 + chi / 2.0).tan() * isometric_factor).atan()
            - std::f64::consts::FRAC_PI_2;
    }
    phi
}
