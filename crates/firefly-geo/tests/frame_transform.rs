//! Frame-to-frame transform of a horizontal measurement (position + covariance).
//!
//! The building block of central measurement fusion (ADR 0010): a plot converted
//! in one sensor's local ENU frame is lifted into a common tracking frame. The
//! transform round-trips the *full 3-D* point (height included) so that two
//! overlapping radars map the same airborne target to the same horizontal point
//! — see the multi-sensor regression test below. Over realistic baselines
//! (≤ ~150 km) its residual curvature/tilt error stays well below radar
//! measurement noise, which the tolerances reflect.
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

/// Transforming into the *same* frame leaves a horizontal measurement unchanged
/// — for any height, since the 3-D round trip is the identity there.
#[test]
fn transform_into_same_frame_is_identity() {
    let frame = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
    let z = Vector2::new(12_345.0, -6_789.0);
    let r = Matrix2::new(2500.0, 400.0, 400.0, 900.0);

    // Ground target: exact to numerical precision.
    let (z0, r0) = frame.horizontal_from(&frame, z, 0.0, r);
    assert_close(z0.x, z.x, 1e-6, "east");
    assert_close(z0.y, z.y, 1e-6, "north");
    for (a, b) in r0.iter().zip(r.iter()) {
        assert_close(*a, *b, 1e-6, "covariance entry");
    }

    // Airborne target: still the identity in the horizontal plane (the height is
    // round-tripped and then dropped); a hair looser for the geodetic iteration.
    let (z_high, _) = frame.horizontal_from(&frame, z, 10_000.0, r);
    assert_close(z_high.x, z.x, 1e-3, "east (airborne)");
    assert_close(z_high.y, z.y, 1e-3, "north (airborne)");
}

/// An **airborne** point measured in sensor B's frame lands, after transforming
/// into sensor A's frame, where A would have measured it directly — because the
/// transform round-trips the full 3-D point (height included) rather than
/// projecting on each sensor's own ground plane.
#[test]
fn position_matches_direct_measurement_in_target_frame() {
    let frame_a = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
    let frame_b = LocalFrame::new(Wgs84::from_degrees(47.5, 10.5, 0.0)); // ~67 km SW

    // A point both radars can see, at 10 km altitude.
    let point = Wgs84::from_degrees(47.8, 10.8, 10_000.0);

    let in_b = frame_b.geodetic_to_enu(&point);
    let z_b = Vector2::new(in_b.east, in_b.north);
    let r_b = Matrix2::new(1600.0, 0.0, 0.0, 400.0);

    let (z_in_a, _r) = frame_a.horizontal_from(&frame_b, z_b, in_b.up, r_b);
    let in_a = frame_a.geodetic_to_enu(&point);

    // Lands on A's ground-projected east/north of the same physical point.
    assert_close(z_in_a.x, in_a.east, 2.0, "east in A");
    assert_close(z_in_a.y, in_a.north, 2.0, "north in A");
}

/// The fix for the multi-sensor **height-projection bias**: two radars at
/// different sites, both seeing one airborne target, map it to the *same*
/// horizontal point in a common tracking frame — so the fused tracker sees one
/// target, not a ghost per sensor. The naïve ground projection (height = 0)
/// instead disagrees by tens of metres, enough to fall outside a converged
/// track's gate.
#[test]
fn airborne_target_maps_to_one_point_from_two_sensors() {
    let tracking = LocalFrame::new(Wgs84::from_degrees(50.0379, 8.5622, 0.0)); // Frankfurt
    let radar_1 = LocalFrame::new(Wgs84::from_degrees(50.0379, 8.5622, 0.0)); // co-located
    let radar_2 = LocalFrame::new(Wgs84::from_degrees(50.4, 9.3, 0.0)); // ~60 km NE

    // One physical target at 10 km altitude.
    let target = Wgs84::from_degrees(50.2, 9.0, 10_000.0);
    let in_1 = radar_1.geodetic_to_enu(&target);
    let in_2 = radar_2.geodetic_to_enu(&target);
    let z1 = Vector2::new(in_1.east, in_1.north);
    let z2 = Vector2::new(in_2.east, in_2.north);
    let cov = Matrix2::identity();

    // Height-aware lift: both sensors agree to well under a metre.
    let (p1, _) = tracking.horizontal_from(&radar_1, z1, in_1.up, cov);
    let (p2, _) = tracking.horizontal_from(&radar_2, z2, in_2.up, cov);
    let agree = (p1 - p2).norm();
    assert!(
        agree < 1.0,
        "height-aware lift should agree to < 1 m, got {agree} m"
    );

    // Naïve ground projection (height = 0) reveals the bias: tens of metres.
    let (g1, _) = tracking.horizontal_from(&radar_1, z1, 0.0, cov);
    let (g2, _) = tracking.horizontal_from(&radar_2, z2, 0.0, cov);
    let naive_gap = (g1 - g2).norm();
    assert!(
        naive_gap > 10.0,
        "ground projection should expose the bias (> 10 m), got {naive_gap} m"
    );
    assert!(
        naive_gap > 10.0 * agree,
        "height-aware lift must be far tighter than the naïve projection \
         (agree {agree} m vs gap {naive_gap} m)"
    );
}

/// The transform is self-consistent and invertible: B → A → B recovers the
/// original ground measurement (to a few metres over ~67 km).
#[test]
fn transform_round_trips_between_frames() {
    let frame_a = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
    let frame_b = LocalFrame::new(Wgs84::from_degrees(47.5, 10.5, 0.0));

    let z_b = Vector2::new(20_000.0, -15_000.0);
    let r_b = Matrix2::new(2500.0, 400.0, 400.0, 900.0);

    let (z_a, r_a) = frame_a.horizontal_from(&frame_b, z_b, 0.0, r_b);
    let (z_b2, r_b2) = frame_b.horizontal_from(&frame_a, z_a, 0.0, r_a);

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

    let (_z2, r2) = frame_a.horizontal_from(&frame_b, z, 0.0, r);

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

    let (_z2, r2) = frame_a.horizontal_from(&frame_b, z, 0.0, r);

    assert!(
        r2[(0, 1)].abs() > 1.0,
        "expected a non-trivial off-diagonal from grid convergence, got {}",
        r2[(0, 1)]
    );
}
