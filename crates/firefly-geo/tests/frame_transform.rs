//! Frame-to-frame transform of a horizontal measurement (position + covariance).
//!
//! The building block of central measurement fusion (ADR 0010): a plot converted
//! in one sensor's local ENU frame is lifted into a common tracking frame. The
//! transform is a tangent-plane operation; over realistic overlapping-radar
//! baselines (≤ ~150 km) its curvature/tilt error stays well below radar
//! measurement noise, which the tolerances below reflect.
//!
//! REQ: FR-GEO-003

use firefly_geo::{LocalFrame, Wgs84};
use nalgebra::{Matrix2, Vector2};

fn assert_close(a: f64, b: f64, tol: f64, what: &str) {
    assert!(
        (a - b).abs() <= tol,
        "{what}: expected {b}, got {a} (|Δ| = {})",
        (a - b).abs()
    );
}

/// Transforming into the *same* frame leaves a horizontal measurement unchanged.
#[test]
fn transform_into_same_frame_is_identity() {
    let frame = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
    let z = Vector2::new(12_345.0, -6_789.0);
    let r = Matrix2::new(2500.0, 400.0, 400.0, 900.0);

    let (z2, r2) = frame.horizontal_from(&frame, z, r);

    assert_close(z2.x, z.x, 1e-6, "east");
    assert_close(z2.y, z.y, 1e-6, "north");
    for (a, b) in r2.iter().zip(r.iter()) {
        assert_close(*a, *b, 1e-6, "covariance entry");
    }
}

/// A ground point measured in sensor B's frame lands, after transforming into
/// sensor A's frame, where A would have measured it directly — to within the
/// tangent-plane approximation over a realistic ~67 km baseline.
#[test]
fn position_matches_direct_measurement_in_target_frame() {
    let frame_a = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
    let frame_b = LocalFrame::new(Wgs84::from_degrees(47.5, 10.5, 0.0)); // ~67 km SW

    // A ground-level point both radars can see (height 0 keeps both ground-plane
    // projections consistent; airborne targets add the usual projection offset).
    let point = Wgs84::from_degrees(47.8, 10.8, 0.0);

    let in_b = frame_b.geodetic_to_enu(&point);
    let z_b = Vector2::new(in_b.east, in_b.north);
    let r_b = Matrix2::new(1600.0, 0.0, 0.0, 400.0);

    let (z_in_a, _r) = frame_a.horizontal_from(&frame_b, z_b, r_b);
    let in_a = frame_a.geodetic_to_enu(&point);

    assert_close(z_in_a.x, in_a.east, 2.0, "east in A");
    assert_close(z_in_a.y, in_a.north, 2.0, "north in A");
}

/// The transform is self-consistent and invertible: B → A → B recovers the
/// original measurement (to a few metres over ~67 km).
#[test]
fn transform_round_trips_between_frames() {
    let frame_a = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
    let frame_b = LocalFrame::new(Wgs84::from_degrees(47.5, 10.5, 0.0));

    let z_b = Vector2::new(20_000.0, -15_000.0);
    let r_b = Matrix2::new(2500.0, 400.0, 400.0, 900.0);

    let (z_a, r_a) = frame_a.horizontal_from(&frame_b, z_b, r_b);
    let (z_b2, r_b2) = frame_b.horizontal_from(&frame_a, z_a, r_a);

    assert_close(z_b2.x, z_b.x, 5.0, "east round trip");
    assert_close(z_b2.y, z_b.y, 5.0, "north round trip");
    for (a, b) in r_b2.iter().zip(r_b.iter()) {
        assert_close(*a, *b, 1.0, "covariance round trip");
    }
}

/// Over realistic baselines the covariance transform is essentially a rotation:
/// it stays symmetric and (approximately) preserves the total variance (trace)
/// and the error-ellipse area (determinant).
#[test]
fn covariance_transform_preserves_ellipse_shape() {
    let frame_a = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
    let frame_b = LocalFrame::new(Wgs84::from_degrees(47.5, 10.5, 0.0));

    let z = Vector2::new(8_000.0, 3_000.0);
    let r = Matrix2::new(2500.0, 600.0, 600.0, 900.0);

    let (_z2, r2) = frame_a.horizontal_from(&frame_b, z, r);

    assert_close(r2[(0, 1)], r2[(1, 0)], 1e-9, "symmetry");
    assert_close(r2.trace(), r.trace(), 1.0, "trace");
    // Determinant preserved to a tight relative tolerance.
    assert_close(
        r2.determinant(),
        r.determinant(),
        r.determinant() * 1e-3,
        "determinant",
    );
}

/// The rotation actually rotates: two frames offset in longitude have converging
/// norths ("grid convergence"), so an axis-aligned covariance picks up a
/// non-trivial off-diagonal in the other frame.
#[test]
fn converging_frames_rotate_the_covariance() {
    let frame_a = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
    let frame_b = LocalFrame::new(Wgs84::from_degrees(48.0, 13.0, 0.0)); // 2° east → ~1.5° convergence

    let z = Vector2::new(5_000.0, 0.0);
    let r = Matrix2::new(10_000.0, 0.0, 0.0, 100.0); // axis-aligned, anisotropic

    let (_z2, r2) = frame_a.horizontal_from(&frame_b, z, r);

    assert!(
        r2[(0, 1)].abs() > 1.0,
        "expected a non-trivial off-diagonal from grid convergence, got {}",
        r2[(0, 1)]
    );
}
