//! Bounding-box ↔ query-circle geometry (ADR 0031).
//!
//! The source-input contract (and Wayfinder's operator UI) describe coverage as
//! a WGS84 **bounding box** — the same shape every other area source uses. The
//! aggregator APIs, however, query a **circle**: centre point plus radius in
//! nautical miles, capped at 250 NM. This module bridges the two:
//!
//! - [`circle_for_bbox`] computes the circumscribed circle — centred on the bbox
//!   midpoint, radius = great-circle distance to the farthest corner — so the
//!   circle always **covers** the whole box (plus surplus area at the edges).
//! - [`BBoxDeg::contains`] then filters the response back down to the exact box,
//!   so the surplus never leaks extra aircraft into the tracker and the adapter
//!   behaves exactly like the bbox-native OpenSky adapter.
//!
//! A box too large for the 250 NM cap is **clamped** — the circle covers as much
//! as allowed, centred on the box. The clamp is reported so the caller can log a
//! prominent warning: silent partial surveillance coverage would be an invisible
//! operational gap.

/// Mean Earth radius, metres (IUGG spherical approximation — plenty for a
/// coverage radius that is deliberately rounded *up* to the enclosing circle).
const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// One nautical mile in metres (exact, by definition).
const METRES_PER_NM: f64 = 1_852.0;

/// The providers' documented maximum query radius (both adsb.lol and adsb.fi
/// cap `dist` at 250 NM).
pub(crate) const MAX_RADIUS_NM: f64 = 250.0;

/// Floor for the query radius: a degenerate (point-like) bbox still needs a
/// non-zero circle to catch aircraft near it.
const MIN_RADIUS_NM: f64 = 1.0;

/// A WGS84 bounding box in degrees. Field layout mirrors the contract's `bbox`
/// object; validation (finite, in-range, `min <= max`) happens at the contract
/// boundary in `firefly-server`, so this type trusts its values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBoxDeg {
    pub lat_min: f64,
    pub lat_max: f64,
    pub lon_min: f64,
    pub lon_max: f64,
}

impl BBoxDeg {
    /// Whether (`lat`, `lon`) lies inside the box (inclusive edges). Used to
    /// trim the circle query's surplus back to the configured coverage.
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        lat >= self.lat_min && lat <= self.lat_max && lon >= self.lon_min && lon <= self.lon_max
    }
}

/// The point query derived from a bbox: centre + radius (NM), plus whether the
/// 250 NM provider cap truncated the requested coverage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueryCircle {
    pub lat: f64,
    pub lon: f64,
    pub radius_nm: f64,
    /// True when the circumscribed circle exceeded [`MAX_RADIUS_NM`] and was
    /// clamped — the configured bbox is then only partially covered.
    pub clamped: bool,
}

/// Great-circle distance between two WGS84 points, metres (haversine on the
/// mean sphere). The ~0.5 % spherical error is irrelevant here: the radius is
/// rounded up to the enclosing circle anyway, and results are re-filtered to
/// the exact bbox.
fn great_circle_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (phi1, phi2) = (lat1.to_radians(), lat2.to_radians());
    let dphi = (lat2 - lat1).to_radians();
    let dlambda = (lon2 - lon1).to_radians();
    let a = (dphi / 2.0).sin().powi(2) + phi1.cos() * phi2.cos() * (dlambda / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().atan2((1.0 - a).sqrt())
}

/// The circumscribed query circle for `bbox`: centred on the midpoint, radius =
/// the farthest corner's great-circle distance (floored at 1 NM, clamped at the
/// provider cap of 250 NM).
pub(crate) fn circle_for_bbox(bbox: &BBoxDeg) -> QueryCircle {
    let lat = 0.5 * (bbox.lat_min + bbox.lat_max);
    let lon = 0.5 * (bbox.lon_min + bbox.lon_max);
    let corners = [
        (bbox.lat_min, bbox.lon_min),
        (bbox.lat_min, bbox.lon_max),
        (bbox.lat_max, bbox.lon_min),
        (bbox.lat_max, bbox.lon_max),
    ];
    let radius_m = corners
        .iter()
        .map(|&(clat, clon)| great_circle_m(lat, lon, clat, clon))
        .fold(0.0, f64::max);
    let radius_nm = (radius_m / METRES_PER_NM).max(MIN_RADIUS_NM);
    let clamped = radius_nm > MAX_RADIUS_NM;
    QueryCircle {
        lat,
        lon,
        radius_nm: radius_nm.min(MAX_RADIUS_NM),
        clamped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ~1°×1.5° box around Frankfurt: the circle sits on the midpoint and the
    /// radius covers the corners (roughly 55–60 NM here) without hitting the cap.
    #[test]
    fn frankfurt_box_yields_covering_uncapped_circle() {
        let bbox = BBoxDeg {
            lat_min: 49.5,
            lat_max: 50.5,
            lon_min: 7.8,
            lon_max: 9.3,
        };
        let c = circle_for_bbox(&bbox);
        assert!((c.lat - 50.0).abs() < 1e-12);
        assert!((c.lon - 8.55).abs() < 1e-12);
        assert!(!c.clamped);
        // The radius must reach every corner: re-measure and compare.
        for (clat, clon) in [(49.5, 7.8), (49.5, 9.3), (50.5, 7.8), (50.5, 9.3)] {
            let d_nm = great_circle_m(c.lat, c.lon, clat, clon) / METRES_PER_NM;
            assert!(
                d_nm <= c.radius_nm + 1e-9,
                "corner ({clat},{clon}) at {d_nm} NM outside radius {} NM",
                c.radius_nm
            );
        }
        assert!(
            c.radius_nm > 40.0 && c.radius_nm < 80.0,
            "plausible magnitude, got {} NM",
            c.radius_nm
        );
    }

    /// The whole-Germany default box (47–55°N, 5–16°E) exceeds 250 NM and is
    /// clamped — and says so.
    #[test]
    fn oversized_box_is_clamped_and_flagged() {
        let bbox = BBoxDeg {
            lat_min: 47.0,
            lat_max: 55.0,
            lon_min: 5.0,
            lon_max: 16.0,
        };
        let c = circle_for_bbox(&bbox);
        assert!(c.clamped, "corner distance ~290 NM must trip the cap");
        assert_eq!(c.radius_nm, MAX_RADIUS_NM);
    }

    /// A degenerate (point) bbox still queries a non-zero circle.
    #[test]
    fn degenerate_box_gets_the_minimum_radius() {
        let bbox = BBoxDeg {
            lat_min: 50.0,
            lat_max: 50.0,
            lon_min: 8.0,
            lon_max: 8.0,
        };
        let c = circle_for_bbox(&bbox);
        assert_eq!(c.radius_nm, MIN_RADIUS_NM);
        assert!(!c.clamped);
    }

    /// The bbox filter trims the circle's surplus: inside stays, outside goes,
    /// edges are inclusive.
    #[test]
    fn bbox_contains_is_inclusive() {
        let bbox = BBoxDeg {
            lat_min: 49.5,
            lat_max: 50.5,
            lon_min: 7.8,
            lon_max: 9.3,
        };
        assert!(bbox.contains(50.0, 8.5), "interior");
        assert!(bbox.contains(49.5, 7.8), "min corner is inclusive");
        assert!(bbox.contains(50.5, 9.3), "max corner is inclusive");
        assert!(!bbox.contains(50.0, 9.4), "east of the box");
        assert!(!bbox.contains(49.4, 8.5), "south of the box");
    }

    /// Haversine sanity: one degree of latitude is ~60 NM (the definition of
    /// the nautical mile, up to the spherical approximation).
    #[test]
    fn one_degree_latitude_is_about_sixty_nm() {
        let d_nm = great_circle_m(50.0, 8.0, 51.0, 8.0) / METRES_PER_NM;
        assert!((d_nm - 60.0).abs() < 0.2, "got {d_nm} NM");
    }
}
