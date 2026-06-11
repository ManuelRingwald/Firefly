use firefly_geo::{StereographicProjection, Wgs84};

fn assert_close(a: f64, b: f64, tol: f64, what: &str) {
    assert!(
        (a - b).abs() <= tol,
        "{what}: expected {b}, got {a} (|Δ| = {})",
        (a - b).abs()
    );
}

#[test]
fn reference_point_projects_to_origin() {
    let reference = Wgs84::from_degrees(48.1372, 11.5756, 519.0); // Munich
    let projection = StereographicProjection::new(reference);

    let (x, y) = projection.project(reference);
    assert_close(x, 0.0, 1e-6, "x");
    assert_close(y, 0.0, 1e-6, "y");
}

#[test]
fn projection_roundtrips() {
    let reference = Wgs84::from_degrees(48.1372, 11.5756, 0.0); // Munich
    let projection = StereographicProjection::new(reference);

    let cases = [
        Wgs84::from_degrees(48.1372, 11.5756, 519.0), // reference itself, with height
        Wgs84::from_degrees(48.3537, 11.7861, 6000.0), // ~30 km NE, airborne
        Wgs84::from_degrees(47.5, 8.5, 11000.0),      // ~150 km away
        Wgs84::from_degrees(50.0, 14.0, 3000.0),      // ~250 km away
    ];

    for p in cases {
        let (x, y) = projection.project(p);
        let back = projection.unproject(x, y, p.height);
        assert_close(back.lat_deg(), p.lat_deg(), 1e-9, "lat");
        assert_close(back.lon_deg(), p.lon_deg(), 1e-9, "lon");
        assert_close(back.height, p.height, 1e-9, "height");
    }
}

#[test]
fn east_and_north_directions_match_enu_near_reference() {
    // Close to the reference point, the stereographic plane is nearly
    // identical to the local ENU tangent plane: a point slightly north has
    // y > 0 and x ~ 0, a point slightly east has x > 0 and y ~ 0.
    let reference = Wgs84::from_degrees(0.0, 0.0, 0.0);
    let projection = StereographicProjection::new(reference);

    let (x_east, y_east) = projection.project(Wgs84::from_degrees(0.0, 0.01, 0.0));
    assert!(x_east > 0.0, "east point should have x > 0");
    assert_close(y_east, 0.0, 1.0, "y of east point");

    let (x_north, y_north) = projection.project(Wgs84::from_degrees(0.01, 0.0, 0.0));
    assert!(y_north > 0.0, "north point should have y > 0");
    assert_close(x_north, 0.0, 1.0, "x of north point");
}

#[test]
fn distance_from_reference_matches_great_circle_for_short_ranges() {
    // For a point ~10 km north of the reference, the stereographic distance
    // should match the geodesic distance to within metres (the projection's
    // distortion grows with distance from the tangent point but is
    // negligible at this range).
    let reference = Wgs84::from_degrees(48.0, 11.0, 0.0);
    let projection = StereographicProjection::new(reference);

    // ~0.09 deg latitude is roughly 10 km.
    let target = Wgs84::from_degrees(48.09, 11.0, 0.0);
    let (x, y) = projection.project(target);
    let distance = (x * x + y * y).sqrt();

    assert_close(distance, 10_000.0, 50.0, "distance");
}
