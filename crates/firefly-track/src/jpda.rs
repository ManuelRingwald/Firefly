//! Joint Probabilistic Data Association (JPDA): the "joint" complement to
//! per-track PDA (Häppchen M5.5, [`crate::pda`]).
//!
//! Per-track PDA computes association probabilities `β` for one track in
//! isolation, as if no other track existed. That is exactly right when gates
//! do not overlap. But in dense traffic two tracks' gates can share a plot —
//! and per-track PDA would happily let *both* tracks claim a large `β` for
//! the very same plot, double-counting it. **JPDA** removes this by reasoning
//! about all tracks in a shared cluster *together*: it enumerates every
//! **feasible joint event** — an assignment of plots to tracks where each
//! track gets at most one plot and each plot goes to at most one track (the
//! **exclusivity constraint**) — weighs each event by how well it explains
//! the data, and then marginalises (sums) over events to get each track's
//! `β`.
//!
//! ## Clusters
//!
//! Tracks and plots whose gates do not touch any other track's gate are
//! independent: a track with no gated plot trivially has `β_0 = 1`, and two
//! tracks with disjoint gated plot sets never compete. Only tracks (and
//! plots) connected — directly or transitively — through shared gated plots
//! form a **cluster** that needs joint reasoning. Splitting the problem into
//! clusters keeps the combinatorics local: in normal traffic, clusters are
//! one or two tracks wide.
//!
//! ## Event weights
//!
//! For a cluster with tracks `{i}` and plots `{j}`, a joint event assigns
//! each track either "no detection" or one plot, with no plot used twice. Its
//! (unnormalised) weight is the product, over all tracks, of:
//! - `Λ_ij` (the [measurement likelihood](crate::LinearKalman::measurement_likelihood))
//!   if the event assigns plot `j` to track `i`, or
//! - `b` (the same clutter term as [`crate::pda::association_probabilities`])
//!   if the event leaves track `i` unassigned.
//!
//! `β_ij` is the sum of the weights of all events that assign plot `j` to
//! track `i`, divided by the sum over *all* events in the cluster — exactly
//! the marginal probability of that pairing. For a single track this collapses
//! to the per-track PDA formula; for two tracks competing over one plot, the
//! events where *both* would want it never occur (only one can have it),
//! which is precisely the exclusivity that per-track PDA lacks.
//!
//! REQ: FR-TRK-018

use std::collections::BTreeMap;

use crate::gating::Gate;
use crate::kalman::LinearKalman;
use crate::measurement::CartesianMeasurement;
use crate::pda::ClutterModel;

/// Find the representative of `x`'s set, with path compression.
fn find(parent: &mut [usize], x: usize) -> usize {
    if parent[x] != x {
        parent[x] = find(parent, parent[x]);
    }
    parent[x]
}

/// Compute the **joint** association probabilities for every track against
/// every plot, accounting for gates shared between tracks.
///
/// Returns one row per track, each of length `measurements.len() + 1`: index
/// 0 is `β_i0` ("no detection" for track `i`), index `1 + j` is `β_ij`. Every
/// row sums to 1.
///
/// REQ: FR-TRK-018
pub fn joint_association_probabilities(
    tracks: &[LinearKalman],
    measurements: &[CartesianMeasurement],
    gate: &Gate,
    clutter: &ClutterModel,
) -> Vec<Vec<f64>> {
    let t = tracks.len();
    let m = measurements.len();

    if t == 0 {
        return Vec::new();
    }
    if m == 0 {
        return vec![vec![1.0]; t];
    }

    let p_gate = 1.0 - (-gate.threshold / 2.0).exp();
    let b = clutter.density * (1.0 - clutter.detection_probability * p_gate)
        / clutter.detection_probability;

    // Validation matrix and per-pair likelihoods.
    let mut valid = vec![vec![false; m]; t];
    let mut lambda = vec![vec![0.0; m]; t];
    for (i, track) in tracks.iter().enumerate() {
        for (j, meas) in measurements.iter().enumerate() {
            let d2 = track.mahalanobis_squared(meas);
            if gate.accepts(d2) {
                valid[i][j] = true;
                lambda[i][j] = track.measurement_likelihood(meas);
            }
        }
    }

    // Union-find over t tracks (nodes 0..t) and m plots (nodes t..t+m),
    // joining a track and a plot whenever the plot is gated for that track.
    let mut parent: Vec<usize> = (0..t + m).collect();
    for (i, row) in valid.iter().enumerate() {
        for (j, &v) in row.iter().enumerate() {
            if v {
                let ri = find(&mut parent, i);
                let rj = find(&mut parent, t + j);
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    // Group each cluster's tracks and plots by their root.
    let mut clusters: BTreeMap<usize, (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    for i in 0..t {
        clusters.entry(find(&mut parent, i)).or_default().0.push(i);
    }
    for j in 0..m {
        let root = find(&mut parent, t + j);
        if let Some(entry) = clusters.get_mut(&root) {
            entry.1.push(j);
        }
    }

    let mut result = vec![vec![0.0; m + 1]; t];
    for (track_idxs, meas_idxs) in clusters.into_values() {
        if meas_idxs.is_empty() {
            // No track in this cluster has any plot in its gate.
            for &i in &track_idxs {
                result[i][0] = 1.0;
            }
            continue;
        }

        let mut enumerator = ClusterEnumerator {
            track_idxs: &track_idxs,
            meas_idxs: &meas_idxs,
            valid: &valid,
            lambda: &lambda,
            b,
            weights: vec![vec![0.0; meas_idxs.len() + 1]; track_idxs.len()],
            total: 0.0,
        };
        let mut assignment = vec![usize::MAX; track_idxs.len()];
        let mut used = vec![false; meas_idxs.len()];
        enumerator.recurse(0, &mut assignment, &mut used);

        for (li, &i) in track_idxs.iter().enumerate() {
            if enumerator.total > 0.0 {
                result[i][0] = enumerator.weights[li][0] / enumerator.total;
                for (lj, &j) in meas_idxs.iter().enumerate() {
                    result[i][1 + j] = enumerator.weights[li][1 + lj] / enumerator.total;
                }
            } else {
                result[i][0] = 1.0;
            }
        }
    }

    result
}

/// Enumerates every feasible joint event for one cluster by backtracking over
/// its tracks, accumulating the (unnormalised) weight of each `(track, plot)`
/// pairing and the total weight over all events.
///
/// Cluster sizes in realistic traffic are tiny (a handful of tracks sharing a
/// gate), so the worst-case `O((plots+1)^tracks)` enumeration is in practice
/// just a handful of events.
struct ClusterEnumerator<'a> {
    track_idxs: &'a [usize],
    meas_idxs: &'a [usize],
    valid: &'a [Vec<bool>],
    lambda: &'a [Vec<f64>],
    b: f64,
    /// `weights[local_track][0]` = accumulated "no detection" weight;
    /// `weights[local_track][1 + local_plot]` = accumulated weight of that pairing.
    weights: Vec<Vec<f64>>,
    total: f64,
}

impl ClusterEnumerator<'_> {
    /// `assignment[local_track]` is `usize::MAX` (unassigned) or the local
    /// plot index it is tentatively assigned to in this branch.
    fn recurse(&mut self, depth: usize, assignment: &mut [usize], used: &mut [bool]) {
        if depth == self.track_idxs.len() {
            self.record_event(assignment);
            return;
        }

        let i = self.track_idxs[depth];

        // Option 1: this track is undetected.
        assignment[depth] = usize::MAX;
        self.recurse(depth + 1, assignment, used);

        // Option 2: assign one of its gated, not-yet-used plots.
        for (lj, &j) in self.meas_idxs.iter().enumerate() {
            if self.valid[i][j] && !used[lj] {
                used[lj] = true;
                assignment[depth] = lj;
                self.recurse(depth + 1, assignment, used);
                used[lj] = false;
            }
        }
    }

    /// Compute the weight of one complete event and fold it into the totals.
    fn record_event(&mut self, assignment: &[usize]) {
        let mut weight = 1.0;
        for (li, &i) in self.track_idxs.iter().enumerate() {
            weight *= match assignment[li] {
                usize::MAX => self.b,
                lj => self.lambda[i][self.meas_idxs[lj]],
            };
        }
        self.total += weight;
        for (li, &a) in assignment.iter().enumerate() {
            match a {
                usize::MAX => self.weights[li][0] += weight,
                lj => self.weights[li][1 + lj] += weight,
            }
        }
    }
}

/// Statistical-resolvability threshold for the coalescence guard, as a
/// squared Mahalanobis distance between two tracks' positions under their
/// combined position covariance. Pairs closer than this are at risk of
/// **track coalescence** — the known structural JPDA weakness where two
/// unresolved tracks share every plot probabilistically and drift onto a
/// common midpoint (SPEC.1, ADR 0036).
const COALESCENCE_D2: f64 = 4.0; // 2σ combined

/// **Coalescence guard** (SPEC.1): decouple track pairs that are too close
/// to be statistically resolved.
///
/// JPDA's probability-weighted sharing is exactly right for an *occasional*
/// ambiguous plot, but for a persistently unresolved pair it mixes both
/// targets' plots into both tracks every scan, and the two estimates
/// collapse onto their midpoint. The standard remedy family is hypothesis
/// pruning; this guard implements its simplest deterministic member: for
/// every pair of tracks whose predicted positions lie within
/// [`COALESCENCE_D2`] of each other (combined covariance), each **shared**
/// measurement is kept only by the track that claims it more strongly — the
/// other track's β for that measurement is moved to its no-detection weight
/// `β_0`. Rows keep summing to 1; unshared measurements and distant track
/// pairs are untouched, so the guard is a no-op in ordinary traffic.
///
/// Ties (equal β) resolve to the lower track index — deterministic
/// (ADR 0003).
///
/// REQ: FR-TRK-045
pub fn decouple_coalescing_pairs(reference: &[LinearKalman], betas: &mut [Vec<f64>]) {
    let n = reference.len().min(betas.len());
    for i in 0..n {
        for j in (i + 1)..n {
            let d = reference[i].position() - reference[j].position();
            let s = reference[i].position_covariance() + reference[j].position_covariance();
            let Some(s_inv) = s.try_inverse() else {
                continue; // degenerate covariance: leave the pair alone
            };
            let d2 = (d.transpose() * s_inv * d)[(0, 0)];
            if d2 >= COALESCENCE_D2 {
                continue; // statistically resolved — JPDA sharing is fine
            }
            // Unresolvable pair: every shared measurement goes exclusively
            // to the stronger claimant.
            let m_count = betas[i].len().min(betas[j].len());
            for m in 1..m_count {
                if betas[i][m] > 0.0 && betas[j][m] > 0.0 {
                    let loser = if betas[j][m] > betas[i][m] { i } else { j };
                    let moved = betas[loser][m];
                    betas[loser][m] = 0.0;
                    betas[loser][0] += moved;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pda::association_probabilities;
    use nalgebra::{Matrix2, Matrix4, Vector2, Vector4};

    fn track_at(east: f64, north: f64) -> LinearKalman {
        LinearKalman {
            x: Vector4::new(east, north, 0.0, 0.0),
            p: Matrix4::from_diagonal(&Vector4::new(2500.0, 2500.0, 1.0e6, 1.0e6)),
        }
    }

    fn meas_at(east: f64, north: f64) -> CartesianMeasurement {
        CartesianMeasurement {
            z: Vector2::new(east, north),
            r: Matrix2::new(2500.0, 0.0, 0.0, 2500.0),
        }
    }

    /// Every track's row sums to 1, whatever the configuration.
    /// REQ: FR-TRK-018
    #[test]
    fn rows_sum_to_one() {
        let tracks = [track_at(0.0, 0.0), track_at(50.0, 0.0)];
        let measurements = [
            meas_at(0.0, 0.0),
            meas_at(50.0, 0.0),
            meas_at(5_000.0, 5_000.0),
        ];
        let gate = Gate::from_probability(0.99);
        let clutter = ClutterModel::new(1.0e-6, 0.9);

        let betas = joint_association_probabilities(&tracks, &measurements, &gate, &clutter);
        assert_eq!(betas.len(), tracks.len());
        for row in &betas {
            assert_eq!(row.len(), measurements.len() + 1);
            let total: f64 = row.iter().sum();
            assert!((total - 1.0).abs() < 1e-9, "row sums to {total}: {row:?}");
        }
    }

    /// A single track with a single gated plot reduces exactly to the
    /// per-track PDA result.
    /// REQ: FR-TRK-018
    #[test]
    fn single_track_matches_per_track_pda() {
        let track = track_at(0.0, 0.0);
        let measurements = [meas_at(5.0, 0.0), meas_at(-5.0, 5.0)];
        let gate = Gate::from_probability(0.99);
        let clutter = ClutterModel::new(1.0e-6, 0.9);

        let joint = joint_association_probabilities(&[track], &measurements, &gate, &clutter);
        let solo = association_probabilities(&track, &measurements, &gate, &clutter);
        for (a, b) in joint[0].iter().zip(&solo) {
            assert!((a - b).abs() < 1e-12, "joint = {joint:?}, solo = {solo:?}");
        }
    }

    /// A track with nothing in its gate is certain "no detection", and does
    /// not interact with other tracks/plots.
    /// REQ: FR-TRK-018
    #[test]
    fn track_with_no_gated_plot_is_independent() {
        let lonely = track_at(100_000.0, 100_000.0);
        let busy = track_at(0.0, 0.0);
        let measurements = [meas_at(0.0, 0.0)];
        let gate = Gate::from_probability(0.99);
        let clutter = ClutterModel::new(1.0e-6, 0.9);

        let betas =
            joint_association_probabilities(&[lonely, busy], &measurements, &gate, &clutter);
        assert_eq!(betas[0], vec![1.0, 0.0]);
        assert!(betas[1][1] > 0.0, "the busy track still claims its plot");
    }

    /// Two tracks both gated to the *same single* plot must compete for it:
    /// the joint `β` for that pairing is strictly smaller than the per-track
    /// PDA value would be (which ignores the other track entirely), because
    /// events where the other track takes the plot are excluded.
    /// REQ: FR-TRK-018
    #[test]
    fn overlapping_tracks_split_a_shared_plot() {
        // Two tracks close together, one plot exactly between them — gated by
        // both.
        let track_a = track_at(-10.0, 0.0);
        let track_b = track_at(10.0, 0.0);
        let measurements = [meas_at(0.0, 0.0)];
        let gate = Gate::from_probability(0.99);
        let clutter = ClutterModel::new(1.0e-6, 0.9);

        let joint =
            joint_association_probabilities(&[track_a, track_b], &measurements, &gate, &clutter);
        let solo_a = association_probabilities(&track_a, &measurements, &gate, &clutter);

        // By symmetry both tracks see the same beta for the shared plot.
        assert!((joint[0][1] - joint[1][1]).abs() < 1e-12);
        // Joint reasoning gives track A a *smaller* claim on the plot than it
        // would get if track B did not exist.
        assert!(
            joint[0][1] < solo_a[1],
            "joint beta {} should be less than solo beta {}",
            joint[0][1],
            solo_a[1]
        );
        // And both rows still sum to 1.
        assert!((joint[0].iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!((joint[1].iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }

    /// With no plots at all, every track is certainly undetected.
    /// REQ: FR-TRK-018
    #[test]
    fn no_measurements_means_certain_no_detection() {
        let tracks = [track_at(0.0, 0.0), track_at(50.0, 0.0)];
        let gate = Gate::from_probability(0.99);
        let clutter = ClutterModel::new(1.0e-6, 0.9);

        let betas = joint_association_probabilities(&tracks, &[], &gate, &clutter);
        assert_eq!(betas, vec![vec![1.0], vec![1.0]]);
    }
}

#[cfg(test)]
mod coalescence_tests {
    use super::*;
    use nalgebra::{Matrix2, Matrix4, Vector2, Vector4};

    fn track_at(east: f64, pos_var: f64) -> LinearKalman {
        LinearKalman {
            x: Vector4::new(east, 0.0, 0.0, 0.0),
            p: Matrix4::identity() * pos_var,
        }
    }

    fn measurement(east: f64) -> CartesianMeasurement {
        CartesianMeasurement {
            z: Vector2::new(east, 0.0),
            r: Matrix2::identity() * 100.0,
        }
    }

    /// An unresolvable pair sharing two plots is decoupled: each track keeps
    /// exclusively the plot it claims more strongly, the surrendered mass
    /// moves to β₀, rows still sum to 1. REQ: FR-TRK-045
    #[test]
    fn unresolvable_pair_gets_exclusive_assignments() {
        // 100 m apart with σ_pos = 300 m each → d² ≈ 0.056 ≪ 4.
        let reference = vec![track_at(0.0, 90_000.0), track_at(100.0, 90_000.0)];
        let gate = crate::Gate::from_probability(0.9999);
        let clutter = ClutterModel::new(1e-9, 0.95);
        let measurements = vec![measurement(0.0), measurement(100.0)];
        let mut betas = joint_association_probabilities(&reference, &measurements, &gate, &clutter);
        // Before: both tracks share both plots.
        assert!(betas[0][1] > 0.0 && betas[0][2] > 0.0);
        assert!(betas[1][1] > 0.0 && betas[1][2] > 0.0);

        decouple_coalescing_pairs(&reference, &mut betas);

        // After: plot 1 belongs to track 0, plot 2 to track 1, exclusively.
        assert!(betas[0][1] > 0.0 && betas[1][1] == 0.0, "plot 1 → track 0");
        assert!(betas[1][2] > 0.0 && betas[0][2] == 0.0, "plot 2 → track 1");
        for row in &betas {
            assert!((row.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        }
    }

    /// A well-separated pair is untouched — the guard is a no-op in
    /// ordinary traffic. REQ: FR-TRK-045
    #[test]
    fn resolved_pair_is_left_alone() {
        // 5 km apart with σ_pos = 100 m each → d² ≫ 4.
        let reference = vec![track_at(0.0, 10_000.0), track_at(5_000.0, 10_000.0)];
        let gate = crate::Gate::from_probability(0.9999);
        let clutter = ClutterModel::new(1e-9, 0.95);
        let measurements = vec![measurement(0.0), measurement(5_000.0)];
        let mut betas = joint_association_probabilities(&reference, &measurements, &gate, &clutter);
        let before = betas.clone();
        decouple_coalescing_pairs(&reference, &mut betas);
        assert_eq!(betas, before);
    }
}
