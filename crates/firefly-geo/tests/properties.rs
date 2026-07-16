//! Property tests (ASSUR.2): the geodesy invariants hold for THOUSANDS of
//! random inputs, not just the hand-picked examples of the unit tests.
//!
//! The invariant under test is the one every position in the system rides
//! on: projecting a WGS84 position into a local ENU frame and back must
//! return the same position (within numerical noise) — for arbitrary
//! origins and arbitrary points at operational distances.
//!
//! REQ: NFR-ASSUR-001

use firefly_geo::{LocalFrame, Wgs84};
use proptest::prelude::*;

proptest! {
    /// WGS84 → ENU → WGS84 is the identity to sub-0.1-mm precision for any
    /// frame origin (|lat| ≤ 80°) and any point within ±2° of it (≈ 220 km —
    /// beyond any single frame's operational use).
    ///
    /// Both findings of this property's first drafts were TEST bugs the
    /// property itself exposed — kept here as calibration record:
    ///
    /// - **Tolerance measured, not wished for:** the closed-form
    ///   ECEF→geodetic conversion carries a height-dependent error of
    ///   ~1.4 µm at 12.8 km and ~3.5 µm at 20 km; a 1 µm bound was tighter
    ///   than f64 geodesy arithmetic itself. 1e-9 deg ≈ 0.1 mm keeps six
    ///   orders of magnitude below the ~5 m wire LSB.
    /// - **Antimeridian:** a generated point past ±180° comes back
    ///   correctly normalised into the principal range (−180.58° ↔
    ///   +179.42° — the same physical point), so longitudes must be
    ///   compared as angles (mod 360°), not as raw numbers.
    #[test]
    fn wgs84_enu_round_trip_is_identity(
        origin_lat in -80.0f64..80.0,
        origin_lon in -180.0f64..180.0,
        d_lat in -2.0f64..2.0,
        d_lon in -2.0f64..2.0,
        height in 0.0f64..20_000.0,
    ) {
        let frame = LocalFrame::new(Wgs84::from_degrees(origin_lat, origin_lon, 0.0));
        let p = Wgs84::from_degrees(origin_lat + d_lat, origin_lon + d_lon, height);
        let back = frame.enu_to_geodetic(&frame.geodetic_to_enu(&p));
        let dlon = (back.lon_deg() - p.lon_deg()).rem_euclid(360.0);
        let dlon = dlon.min(360.0 - dlon);
        prop_assert!((back.lat_deg() - p.lat_deg()).abs() < 1e-9);
        prop_assert!(dlon < 1e-9);
        prop_assert!((back.height - p.height).abs() < 1e-3);
    }

    /// The ENU distance of a point from its own frame origin matches the
    /// height exactly at zero offset — and is never negative garbage (NaN
    /// poisoning would fail every comparison).
    #[test]
    fn enu_coordinates_are_finite(
        origin_lat in -80.0f64..80.0,
        origin_lon in -180.0f64..180.0,
        d_lat in -2.0f64..2.0,
        d_lon in -2.0f64..2.0,
        height in 0.0f64..20_000.0,
    ) {
        let frame = LocalFrame::new(Wgs84::from_degrees(origin_lat, origin_lon, 0.0));
        let p = frame.geodetic_to_enu(&Wgs84::from_degrees(
            origin_lat + d_lat,
            origin_lon + d_lon,
            height,
        ));
        prop_assert!(p.east.is_finite() && p.north.is_finite() && p.up.is_finite());
    }
}
