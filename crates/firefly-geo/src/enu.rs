use nalgebra::{Matrix2, Vector2};
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

    /// Rotate a local ENU *direction* (no translation) into ECEF.
    ///
    /// Like [`LocalFrame::enu_to_ecef`] but for a free vector: it applies only
    /// the frame's rotation, not the origin offset — so a unit ENU axis becomes
    /// the corresponding ECEF direction.
    fn enu_dir_to_ecef(&self, e: f64, n: f64, u: f64) -> (f64, f64, f64) {
        let dx =
            -self.sin_lon * e - self.sin_lat * self.cos_lon * n + self.cos_lat * self.cos_lon * u;
        let dy =
            self.cos_lon * e - self.sin_lat * self.sin_lon * n + self.cos_lat * self.sin_lon * u;
        let dz = self.cos_lat * n + self.sin_lat * u;
        (dx, dy, dz)
    }

    /// Rotate an ECEF *direction* (no translation) into this local ENU frame.
    fn ecef_dir_to_enu(&self, dx: f64, dy: f64, dz: f64) -> (f64, f64, f64) {
        let e = -self.sin_lon * dx + self.cos_lon * dy;
        let n = -self.sin_lat * self.cos_lon * dx - self.sin_lat * self.sin_lon * dy
            + self.cos_lat * dz;
        let u =
            self.cos_lat * self.cos_lon * dx + self.cos_lat * self.sin_lon * dy + self.sin_lat * dz;
        (e, n, u)
    }

    /// The 2×2 rotation that re-expresses a *horizontal* ENU vector of `source`
    /// in this frame's horizontal (east, north) plane.
    ///
    /// Two local tangent planes anchored at different sites are rotated against
    /// each other (their norths point at slightly different ECEF directions —
    /// "grid convergence") and tilted (their up-axes differ). This matrix is the
    /// horizontal part of the exact ENU→ENU rotation `R_target · R_sourceᵀ`,
    /// obtained by mapping `source`'s east and north unit axes through ECEF into
    /// this frame and dropping the (small) vertical leakage. For nearby sensors
    /// it is very close to a pure rotation.
    ///
    /// Columns are the images of `source`'s east and north axes; rows are this
    /// frame's east and north components.
    pub fn horizontal_rotation_from(&self, source: &LocalFrame) -> Matrix2<f64> {
        // source east axis -> ECEF -> this frame.
        let (ex, ey, ez) = source.enu_dir_to_ecef(1.0, 0.0, 0.0);
        let (e_e, e_n, _) = self.ecef_dir_to_enu(ex, ey, ez);
        // source north axis -> ECEF -> this frame.
        let (nx, ny, nz) = source.enu_dir_to_ecef(0.0, 1.0, 0.0);
        let (n_e, n_n, _) = self.ecef_dir_to_enu(nx, ny, nz);
        // Column-major image of (east, north) axes.
        Matrix2::new(e_e, n_e, e_n, n_n)
    }

    /// Transform a horizontal radar measurement from `source`'s ENU frame into
    /// this frame: ground-projected position `z = [east, north]`, the target's
    /// `height` above the source frame's tangent plane, and the 2×2 covariance
    /// `r`. Returns the ground-projected `[east, north]` position and the rotated
    /// covariance, both expressed in this frame.
    ///
    /// **Why the height matters (multi-sensor fusion).** A radar plot converted
    /// by the tracker (`convert_plot`) gives the target's *ground-projected*
    /// east/north plus its height `h = range·sin(elevation)` above the sensor's
    /// tangent plane. Together `[z.x, z.y, h]` is the target's full 3-D position
    /// in the source frame. We round-trip that **3-D** point through
    /// ECEF/geodetic into this frame and only *then* drop its vertical part. That
    /// is the crucial detail: the 3-D point is the same physical location no
    /// matter which sensor measured it, so two overlapping radars map an airborne
    /// target to the *same* horizontal point here. Projecting to the ground in
    /// the *source* frame first (up = 0 there) would instead project along each
    /// sensor's own local vertical — directions that differ by the angle between
    /// the two sites, displacing a 10 km-high target by tens of metres between
    /// radars and spawning a duplicate ("ghost") track. Real ATC systems remove
    /// exactly this with height-dependent projection corrections.
    ///
    /// The covariance is rotated by [`LocalFrame::horizontal_rotation_from`]:
    /// `R' = T · R · Tᵀ`. For a ground target (`height = 0`) this reduces to the
    /// previous ground-plane transform; for `source == self` it is the identity
    /// for any height.
    ///
    /// This is the core building block of central measurement fusion (ADR 0010):
    /// every sensor's converted plot is lifted into one common tracking frame.
    pub fn horizontal_from(
        &self,
        source: &LocalFrame,
        z: Vector2<f64>,
        height: f64,
        r: Matrix2<f64>,
    ) -> (Vector2<f64>, Matrix2<f64>) {
        // Reconstruct the full 3-D point in the source frame, round-trip it
        // through geodetic into this frame, then drop the (now this-frame)
        // vertical part. Using the true height makes the horizontal result
        // sensor-independent — the fix for the multi-sensor height-projection
        // bias described above.
        let geodetic = source.enu_to_geodetic(&Enu::new(z.x, z.y, height));
        let here = self.geodetic_to_enu(&geodetic);
        let t = self.horizontal_rotation_from(source);
        (Vector2::new(here.east, here.north), t * r * t.transpose())
    }
}
