//! Data association: deciding which plot belongs to which track.
//!
//! After gating (Häppchen 2.3) each track has a set of plausible plots, and each
//! plot may be plausible for several tracks. We must pick a consistent **1:1
//! assignment**. A *greedy* choice (every track grabs its nearest plot) can be
//! globally wrong — it tends to swap identities when targets cross. **Global
//! Nearest Neighbour (GNN)** instead minimises the *total* cost over all tracks
//! and plots at once.
//!
//! The cost of pairing track *i* with plot *j* is their squared Mahalanobis
//! distance (gated: pairs outside the gate are forbidden). Finding the
//! minimum-cost 1:1 matching is the classic **assignment problem**, solved
//! exactly by the **Hungarian algorithm** (Kuhn–Munkres) in `O(n³)`.
//!
//! Unequal counts and leftovers are handled with *dummy* options: each track may
//! choose "no plot" and each plot "no track", both at a cost equal to the gate
//! threshold `γ`. A gated real pair (cost `d² ≤ γ`) is therefore always
//! preferred over leaving things unassigned, while forbidden pairs never win.
//!
//! This module operates on tracks already **predicted** to the scan time and on
//! the measurements of one scan; orchestrating a whole scan (predict, associate,
//! update, handle leftovers) is Häppchen 2.5.

use crate::gating::Gate;
use crate::kalman::LinearKalman;
use crate::measurement::CartesianMeasurement;

/// The outcome of associating a set of tracks with a set of measurements.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Association {
    /// Matched `(track_index, measurement_index)` pairs.
    pub pairs: Vec<(usize, usize)>,
    /// Tracks that received no plot (candidates for coasting).
    pub unassigned_tracks: Vec<usize>,
    /// Plots assigned to no track (candidates for new tracks, or clutter).
    pub unassigned_measurements: Vec<usize>,
}

/// A cost standing in for "forbidden" (outside the gate). Large but finite, so
/// the solver's arithmetic never touches infinities.
const FORBIDDEN: f64 = 1.0e12;

/// Globally optimal assignment of tracks to measurements via the Hungarian
/// algorithm, with gating and dummy options for leftovers.
///
/// REQ: FR-TRK-005
pub fn associate(
    tracks: &[LinearKalman],
    measurements: &[CartesianMeasurement],
    gate: &Gate,
) -> Association {
    let t = tracks.len();
    let p = measurements.len();

    if t == 0 {
        return Association {
            unassigned_measurements: (0..p).collect(),
            ..Default::default()
        };
    }
    if p == 0 {
        return Association {
            unassigned_tracks: (0..t).collect(),
            ..Default::default()
        };
    }

    // Square cost matrix of size (T + P): real block, two dummy blocks, and a
    // dummy–dummy block, so a perfect matching always exists.
    let n = t + p;
    let gamma = gate.threshold;
    let mut cost = vec![vec![FORBIDDEN; n]; n];

    // Real track×plot costs (gated).
    for (i, track) in tracks.iter().enumerate() {
        for (j, m) in measurements.iter().enumerate() {
            let d2 = track.mahalanobis_squared(m);
            if gate.accepts(d2) {
                cost[i][j] = d2;
            }
        }
    }
    // Track i may go unassigned (its own dummy column p+i) at cost γ.
    for i in 0..t {
        cost[i][p + i] = gamma;
    }
    // Plot j may go unassigned (its own dummy row t+j) at cost γ.
    for j in 0..p {
        cost[t + j][j] = gamma;
    }
    // Dummy rows ↔ dummy columns are free.
    for j in 0..p {
        for l in 0..t {
            cost[t + j][p + l] = 0.0;
        }
    }

    let assignment = solve_assignment(&cost);

    let mut pairs = Vec::new();
    let mut unassigned_tracks = Vec::new();
    let mut measurement_taken = vec![false; p];
    for (i, &col) in assignment.iter().take(t).enumerate() {
        if col < p {
            pairs.push((i, col));
            measurement_taken[col] = true;
        } else {
            unassigned_tracks.push(i);
        }
    }
    let unassigned_measurements = (0..p).filter(|&j| !measurement_taken[j]).collect();

    Association {
        pairs,
        unassigned_tracks,
        unassigned_measurements,
    }
}

/// Solve the square assignment problem (minimise total cost) with the
/// `O(n³)` Hungarian algorithm. Returns, for each row, the column assigned to it.
fn solve_assignment(cost: &[Vec<f64>]) -> Vec<usize> {
    let n = cost.len();
    if n == 0 {
        return Vec::new();
    }
    let m = n; // square

    // Potentials and matching, 1-indexed with a sentinel at index 0.
    let mut u = vec![0.0f64; n + 1];
    let mut v = vec![0.0f64; m + 1];
    let mut p = vec![0usize; m + 1]; // p[j] = row matched to column j (0 = none)
    let mut way = vec![0usize; m + 1];

    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0usize;
        let mut min_value = vec![f64::INFINITY; m + 1];
        let mut used = vec![false; m + 1];

        // Grow an alternating tree until we reach an unmatched column.
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = f64::INFINITY;
            let mut j1 = 0usize;

            for j in 1..=m {
                if !used[j] {
                    let cur = cost[i0 - 1][j - 1] - u[i0] - v[j];
                    if cur < min_value[j] {
                        min_value[j] = cur;
                        way[j] = j0;
                    }
                    if min_value[j] < delta {
                        delta = min_value[j];
                        j1 = j;
                    }
                }
            }

            for j in 0..=m {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    min_value[j] -= delta;
                }
            }

            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }

        // Augment along the path recorded in `way`.
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }

    let mut assignment = vec![0usize; n];
    for j in 1..=m {
        if p[j] != 0 {
            assignment[p[j] - 1] = j - 1;
        }
    }
    assignment
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{Matrix2, Matrix4, Vector2, Vector4};

    fn track_at(east: f64, north: f64, pos_var: f64) -> LinearKalman {
        LinearKalman {
            x: Vector4::new(east, north, 0.0, 0.0),
            p: Matrix4::from_diagonal(&Vector4::new(pos_var, pos_var, 1.0e6, 1.0e6)),
        }
    }

    fn meas_at(east: f64, north: f64, var: f64) -> CartesianMeasurement {
        CartesianMeasurement {
            z: Vector2::new(east, north),
            r: Matrix2::new(var, 0.0, 0.0, var),
        }
    }

    /// The Hungarian solver finds the globally cheapest matching, not the greedy
    /// one. Here row 0's nearest column is 0 (cost 1), but taking it forces row 1
    /// onto cost 9 (total 10); the optimum is the "crossed" pairing (total 4).
    /// REQ: FR-TRK-005
    #[test]
    fn hungarian_beats_greedy() {
        let cost = vec![vec![1.0, 2.0], vec![2.0, 9.0]];
        let assignment = solve_assignment(&cost);
        assert_eq!(assignment, vec![1, 0]); // row0→col1, row1→col0
    }

    /// A known 3×3 case.
    /// REQ: FR-TRK-005
    #[test]
    fn hungarian_3x3() {
        let cost = vec![
            vec![4.0, 1.0, 3.0],
            vec![2.0, 0.0, 5.0],
            vec![3.0, 2.0, 2.0],
        ];
        // Optimal: row0→col1 (1), row1→col0 (2), row2→col2 (2), total 5.
        let assignment = solve_assignment(&cost);
        let total: f64 = assignment
            .iter()
            .enumerate()
            .map(|(i, &j)| cost[i][j])
            .sum();
        assert!((total - 5.0).abs() < 1e-9, "total was {total}");
    }

    /// Each track is matched to its corresponding (gated) plot, even when the
    /// plots are listed in the "crossed" order.
    /// REQ: FR-TRK-005
    #[test]
    fn associate_matches_gated_plots() {
        let tracks = [track_at(0.0, 0.0, 2500.0), track_at(10_000.0, 0.0, 2500.0)];
        // m0 is near track 1; m1 is near track 0.
        let measurements = [meas_at(9990.0, 0.0, 2500.0), meas_at(20.0, 0.0, 2500.0)];
        let gate = Gate::from_probability(0.99);

        let a = associate(&tracks, &measurements, &gate);
        assert_eq!(a.pairs, vec![(0, 1), (1, 0)]);
        assert!(a.unassigned_tracks.is_empty());
        assert!(a.unassigned_measurements.is_empty());
    }

    /// A plot outside every gate is left unassigned (clutter / new-track candidate).
    /// REQ: FR-TRK-005
    #[test]
    fn associate_leaves_ungated_plot_unassigned() {
        let tracks = [track_at(0.0, 0.0, 2500.0)];
        let measurements = [meas_at(10.0, 0.0, 2500.0), meas_at(80_000.0, 0.0, 2500.0)];
        let gate = Gate::from_probability(0.99);

        let a = associate(&tracks, &measurements, &gate);
        assert_eq!(a.pairs, vec![(0, 0)]);
        assert_eq!(a.unassigned_measurements, vec![1]);
        assert!(a.unassigned_tracks.is_empty());
    }

    /// A track with no gated plot is left unassigned (will coast in 2.5).
    /// REQ: FR-TRK-005
    #[test]
    fn associate_leaves_starved_track_unassigned() {
        let tracks = [track_at(0.0, 0.0, 2500.0), track_at(50_000.0, 0.0, 2500.0)];
        let measurements = [meas_at(15.0, 0.0, 2500.0)];
        let gate = Gate::from_probability(0.99);

        let a = associate(&tracks, &measurements, &gate);
        assert_eq!(a.pairs, vec![(0, 0)]);
        assert_eq!(a.unassigned_tracks, vec![1]);
        assert!(a.unassigned_measurements.is_empty());
    }

    /// Empty inputs are handled gracefully.
    /// REQ: FR-TRK-005
    #[test]
    fn associate_handles_empty_inputs() {
        let gate = Gate::from_probability(0.99);
        let only_plots = associate(&[], &[meas_at(0.0, 0.0, 1.0)], &gate);
        assert_eq!(only_plots.unassigned_measurements, vec![0]);

        let only_tracks = associate(&[track_at(0.0, 0.0, 1.0)], &[], &gate);
        assert_eq!(only_tracks.unassigned_tracks, vec![0]);
    }
}
