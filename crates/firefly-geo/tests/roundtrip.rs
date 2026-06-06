use firefly_geo::{Ecef, Enu, LocalFrame, Wgs84};

fn assert_close(a: f64, b: f64, tol: f64, what: &str) {
    assert!(
        (a - b).abs() <= tol,
        "{what}: expected {b}, got {a} (|Δ| = {})",
        (a - b).abs()
    );
}

#[test]
fn wgs84_ecef_roundtrip() {
    // A spread of geodetic positions including high latitude and altitude.
    let cases = [
        Wgs84::from_degrees(48.1372, 11.5756, 519.0),   // Munich
        Wgs84::from_degrees(0.0, 0.0, 0.0),             // equator / prime meridian
        Wgs84::from_degrees(-33.8688, 151.2093, 58.0),  // Sydney
        Wgs84::from_degrees(78.2232, 15.6469, 10000.0), // Svalbard, airborne
    ];

    for p in cases {
        let back = p.to_ecef().to_wgs84();
        assert_close(back.lat_deg(), p.lat_deg(), 1e-9, "lat");
        assert_close(back.lon_deg(), p.lon_deg(), 1e-9, "lon");
        assert_close(back.height, p.height, 1e-4, "height");
    }
}

#[test]
fn ecef_matches_known_reference() {
    // Equator at the prime meridian sits on +X at one equatorial radius.
    let p = Wgs84::from_degrees(0.0, 0.0, 0.0).to_ecef();
    assert_close(p.x, firefly_geo::ellipsoid::SEMI_MAJOR_AXIS, 1e-3, "x");
    assert_close(p.y, 0.0, 1e-6, "y");
    assert_close(p.z, 0.0, 1e-6, "z");

    // North pole sits on +Z at one polar radius.
    let pole = Wgs84::from_degrees(90.0, 0.0, 0.0).to_ecef();
    assert_close(pole.z, firefly_geo::ellipsoid::SEMI_MINOR_AXIS, 1e-3, "z");
}

#[test]
fn enu_origin_is_zero() {
    let origin = Wgs84::from_degrees(48.1372, 11.5756, 519.0);
    let frame = LocalFrame::new(origin);
    let enu = frame.geodetic_to_enu(&origin);
    assert_close(enu.east, 0.0, 1e-6, "east");
    assert_close(enu.north, 0.0, 1e-6, "north");
    assert_close(enu.up, 0.0, 1e-6, "up");
}

#[test]
fn enu_axes_point_the_right_way() {
    let origin = Wgs84::from_degrees(0.0, 0.0, 0.0);
    let frame = LocalFrame::new(origin);

    // A little further east in longitude -> +East, ~0 North.
    let east_pt = frame.geodetic_to_enu(&Wgs84::from_degrees(0.0, 0.01, 0.0));
    assert!(east_pt.east > 0.0, "east component should be positive");
    assert_close(east_pt.north, 0.0, 1.0, "north");

    // A little further north in latitude -> +North, ~0 East.
    let north_pt = frame.geodetic_to_enu(&Wgs84::from_degrees(0.01, 0.0, 0.0));
    assert!(north_pt.north > 0.0, "north component should be positive");
    assert_close(north_pt.east, 0.0, 1.0, "east");

    // Straight up in height -> +Up.
    let up_pt = frame.geodetic_to_enu(&Wgs84::from_degrees(0.0, 0.0, 1000.0));
    assert_close(up_pt.up, 1000.0, 1e-3, "up");
}

#[test]
fn enu_geodetic_roundtrip() {
    let frame = LocalFrame::new(Wgs84::from_degrees(48.1372, 11.5756, 519.0));
    let target = Wgs84::from_degrees(48.3537, 11.7861, 6000.0); // ~30 km away, airborne
    let enu = frame.geodetic_to_enu(&target);
    let back = frame.enu_to_geodetic(&enu);
    assert_close(back.lat_deg(), target.lat_deg(), 1e-9, "lat");
    assert_close(back.lon_deg(), target.lon_deg(), 1e-9, "lon");
    assert_close(back.height, target.height, 1e-3, "height");
}

#[test]
fn enu_polar_roundtrip() {
    // Place a point due east and up; check polar then back to ENU.
    let enu = Enu::new(3000.0, 4000.0, 1200.0);
    let polar = enu.to_polar();
    // Range = sqrt(3000^2 + 4000^2 + 1200^2).
    assert_close(
        polar.range,
        (3000f64.powi(2) + 4000f64.powi(2) + 1200f64.powi(2)).sqrt(),
        1e-6,
        "range",
    );
    let back = polar.to_enu();
    assert_close(back.east, enu.east, 1e-6, "east");
    assert_close(back.north, enu.north, 1e-6, "north");
    assert_close(back.up, enu.up, 1e-6, "up");
}

#[test]
fn azimuth_conventions() {
    // Due north -> azimuth 0.
    assert_close(
        Enu::new(0.0, 100.0, 0.0).to_polar().azimuth_deg(),
        0.0,
        1e-9,
        "north az",
    );
    // Due east -> 90.
    assert_close(
        Enu::new(100.0, 0.0, 0.0).to_polar().azimuth_deg(),
        90.0,
        1e-9,
        "east az",
    );
    // Due south -> 180.
    assert_close(
        Enu::new(0.0, -100.0, 0.0).to_polar().azimuth_deg(),
        180.0,
        1e-9,
        "south az",
    );
    // Due west -> 270.
    assert_close(
        Enu::new(-100.0, 0.0, 0.0).to_polar().azimuth_deg(),
        270.0,
        1e-9,
        "west az",
    );
}

#[test]
fn ecef_constructor_roundtrip() {
    let p = Ecef::new(4_177_000.0, 855_000.0, 4_727_000.0);
    let back = p.to_wgs84().to_ecef();
    assert_close(back.x, p.x, 1e-3, "x");
    assert_close(back.y, p.y, 1e-3, "y");
    assert_close(back.z, p.z, 1e-3, "z");
}
