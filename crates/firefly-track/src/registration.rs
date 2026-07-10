//! **Sensor registration**: estimating each radar's systematic measurement
//! errors (biases) so central measurement fusion (ADR 0010) can subtract them
//! before association — the ARTAS practice Firefly adopts with ADR 0034.
//!
//! Why: real radars are never perfectly calibrated. A constant **range
//! offset** (the radar reports every target, say, 150 m too far) and a
//! constant **azimuth offset** (the antenna's north alignment is a few tenths
//! of a degree off) shift *everything* a sensor sees. Unbiased random noise is
//! what the Kalman filter absorbs; a *systematic* offset instead makes two
//! radars see the same aircraft at two slightly different places — the fusion
//! then either splits the aircraft into two tracks ("Doppelbild") or smears
//! one track. The simulator has no biases, which is exactly why this never
//! showed up before real radars.
//!
//! ## The estimation model (REG.1: offline, range + azimuth)
//!
//! A sensor `S` measures `(r, θ)` with `measured = true + bias`, so the true
//! position is recovered from the measurement by subtracting the bias. Let
//! `lift_S(r, θ)` be the ground position of a polar measurement in the
//! **common frame** (the tracker's frame; sensor site frame → WGS84 → common
//! frame, the same path the tracker itself lifts plots through). To first
//! order in the (small) biases `b_S = (Δr, Δθ)`:
//!
//! ```text
//! true position ≈ lift_S(measured) − J_S · b_S
//! ```
//!
//! where `J_S = ∂lift_S/∂(r, θ)` is the 2×2 Jacobian at the measurement. Two
//! sightings of the **same aircraft at the same instant** (paired by the
//! globally unique Mode-S/ICAO address, FR-TRK-031) must agree on the true
//! position, so each correspondence `k` between sides `a` and `b` yields two
//! linear equations in the stacked bias vector `x`:
//!
//! ```text
//! d_k := lift_a(meas_a) − lift_b(meas_b) = J_a·b_a − J_b·b_b
//! ```
//!
//! A **geodetic** sighting (an ADS-B self-report — the aircraft's own WGS84
//! position) has no polar bias: it contributes its position as reference
//! truth and no unknowns. Stacking all correspondences gives an overdetermined
//! `H·x = d`, solved by least squares via SVD; the singular-value spectrum
//! doubles as the **observability diagnostic** (e.g. two co-located radars
//! seeing only each other are rank-deficient: their biases can cancel).
//!
//! The Jacobian is computed **numerically** (central differences on the exact
//! lift) rather than hand-derived: it is exact to O(h²) including the
//! frame-rotation terms between distant sites, and trivially testable against
//! the flat-geometry analytic form.
//!
//! ## Deliberate REG.1 limits (honest boundaries, ADR 0034)
//!
//! - **Offline**: this module estimates from a collected set of
//!   correspondences; feeding the estimate back into the live fusion is REG.2.
//! - **Range + azimuth only**: a systematic **time-stamp offset** between
//!   sensors displaces targets *along track* (`v·Δt`) and is deliberately a
//!   follow-up — correspondences here assume time-aligned sightings, and the
//!   pairing window must be kept tight (see
//!   [`correspondences_by_identity`]).
//! - **Unweighted** least squares: all correspondences count equally; noise
//!   weighting is a refinement for when real sensor mixes demand it.
//!
//! REQ: FR-TRK-037

use std::collections::{BTreeMap, BTreeSet};

use firefly_core::{Measurement, Plot, SensorId};
use firefly_geo::{LocalFrame, Polar, Wgs84};
use nalgebra::{DMatrix, DVector, Matrix2, Vector2};
use serde::{Deserialize, Serialize};

/// A radar's systematic measurement offsets: `measured = true + bias`.
/// Subtracting the bias from a measurement (REG.2) removes the systematic
/// error before fusion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SensorBias {
    /// Constant range offset, metres.
    pub range_m: f64,
    /// Constant azimuth offset, radians (clockwise-from-north convention,
    /// matching [`Polar`]).
    pub azimuth_rad: f64,
}

impl SensorBias {
    /// The azimuth offset in degrees, for display/logging.
    pub fn azimuth_deg(&self) -> f64 {
        self.azimuth_rad.to_degrees()
    }
}

/// One side of a correspondence: how a sensor saw the aircraft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sighting {
    /// A polar measurement from a radar whose bias is **unknown** (an
    /// estimation target). The sensor must have a site frame registered with
    /// the estimator.
    Radar {
        /// The measuring radar.
        sensor: SensorId,
        /// The raw (bias-carrying) polar measurement in the radar's frame.
        measurement: Polar,
    },
    /// A bias-free geodetic sighting — an ADS-B/FLARM self-report. Serves as
    /// reference truth and contributes no unknowns.
    Geodetic {
        /// The reported WGS84 position.
        position: Wgs84,
    },
}

/// Two sightings of the **same aircraft at the same instant** (identity-paired
/// via the Mode-S/ICAO address). The estimator assumes time alignment; the
/// pairing helper enforces a tight window (see module docs on the deliberate
/// exclusion of time-offset estimation from REG.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Correspondence {
    /// First sighting.
    pub a: Sighting,
    /// Second sighting.
    pub b: Sighting,
}

/// The result of a registration estimation run.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistrationSolution {
    /// Estimated bias per radar sensor that appeared in the correspondences.
    pub biases: BTreeMap<SensorId, SensorBias>,
    /// RMS of the correspondence residuals **before** bias correction, metres
    /// — how far apart the paired sightings were.
    pub rms_before_m: f64,
    /// RMS of the residuals **after** subtracting the estimated biases, metres
    /// — what remains is (ideally) just measurement noise.
    pub rms_after_m: f64,
    /// Whether the geometry made every bias component observable. `false`
    /// means the system was rank-deficient (e.g. two co-located radars with no
    /// geodetic reference: a common-mode bias cancels in every residual) and
    /// the returned estimate is the minimum-norm solution — do **not** apply
    /// it operationally.
    pub observable: bool,
}

/// Relative singular-value cutoff for the observability diagnostic: a bias
/// direction whose singular value is this far below the largest is treated as
/// unobserved. The units of the two bias components differ (metres vs.
/// radians), so a benign spread of ~measurement range (≈10⁵) between singular
/// values is expected; genuine rank deficiency sits many orders below that.
const OBSERVABILITY_CUTOFF: f64 = 1e-9;

/// Step sizes for the central-difference Jacobian of the lift: well below any
/// plausible bias, well above f64 rounding at 10⁵-metre scales.
const JACOBIAN_STEP_RANGE_M: f64 = 0.25;
const JACOBIAN_STEP_AZIMUTH_RAD: f64 = 1e-6;

/// Ground position (east, north) of a polar measurement in the common frame:
/// sensor frame → WGS84 → common frame, the same lift the tracker applies to
/// radar plots (ADR 0010).
fn lift(site: &LocalFrame, common: &LocalFrame, measurement: &Polar) -> Vector2<f64> {
    let geodetic = site.enu_to_geodetic(&measurement.to_enu());
    let enu = common.geodetic_to_enu(&geodetic);
    Vector2::new(enu.east, enu.north)
}

/// `∂lift/∂(r, θ)` at the measurement, by central differences on the exact
/// lift — includes the inter-frame rotation terms without hand-derivation.
fn lift_jacobian(site: &LocalFrame, common: &LocalFrame, m: &Polar) -> Matrix2<f64> {
    let dr = JACOBIAN_STEP_RANGE_M;
    let dt = JACOBIAN_STEP_AZIMUTH_RAD;
    let r_plus = lift(
        site,
        common,
        &Polar::new(m.range + dr, m.azimuth, m.elevation),
    );
    let r_minus = lift(
        site,
        common,
        &Polar::new(m.range - dr, m.azimuth, m.elevation),
    );
    let t_plus = lift(
        site,
        common,
        &Polar::new(m.range, m.azimuth + dt, m.elevation),
    );
    let t_minus = lift(
        site,
        common,
        &Polar::new(m.range, m.azimuth - dt, m.elevation),
    );
    let d_range = (r_plus - r_minus) / (2.0 * dr);
    let d_azimuth = (t_plus - t_minus) / (2.0 * dt);
    Matrix2::new(d_range.x, d_azimuth.x, d_range.y, d_azimuth.y)
}

/// The common-frame ground position of one sighting, or `None` for a radar
/// sighting whose sensor has no registered site frame (it cannot be lifted —
/// the whole correspondence is skipped).
fn sighting_position(
    sighting: &Sighting,
    sites: &BTreeMap<SensorId, LocalFrame>,
    common: &LocalFrame,
) -> Option<Vector2<f64>> {
    match sighting {
        Sighting::Radar {
            sensor,
            measurement,
        } => sites
            .get(sensor)
            .map(|site| lift(site, common, measurement)),
        Sighting::Geodetic { position } => {
            let enu = common.geodetic_to_enu(position);
            Some(Vector2::new(enu.east, enu.north))
        }
    }
}

/// Estimate per-sensor range/azimuth biases from identity-paired
/// correspondences by linearized least squares (see module docs).
///
/// `sites` maps each radar sensor to its site frame (the same geometry the
/// tracker is configured with). Returns `None` when no correspondence
/// involves a radar with a known site frame — there is nothing to estimate.
/// Correspondences referencing a radar without a site frame are skipped.
///
/// REQ: FR-TRK-037
pub fn estimate_biases(
    common: &LocalFrame,
    sites: &BTreeMap<SensorId, LocalFrame>,
    correspondences: &[Correspondence],
) -> Option<RegistrationSolution> {
    // Which sensors actually appear (and are liftable) → unknown layout.
    let mut sensors: BTreeSet<SensorId> = BTreeSet::new();
    let usable: Vec<&Correspondence> = correspondences
        .iter()
        .filter(|c| {
            let liftable = |s: &Sighting| match s {
                Sighting::Radar { sensor, .. } => sites.contains_key(sensor),
                Sighting::Geodetic { .. } => true,
            };
            liftable(&c.a) && liftable(&c.b)
        })
        .collect();
    for c in &usable {
        for side in [&c.a, &c.b] {
            if let Sighting::Radar { sensor, .. } = side {
                sensors.insert(*sensor);
            }
        }
    }
    if sensors.is_empty() || usable.is_empty() {
        return None;
    }
    let column: BTreeMap<SensorId, usize> = sensors
        .iter()
        .enumerate()
        .map(|(i, &s)| (s, 2 * i))
        .collect();

    // Assemble H·x = d: two rows (east, north) per correspondence.
    let rows = 2 * usable.len();
    let cols = 2 * sensors.len();
    let mut h = DMatrix::<f64>::zeros(rows, cols);
    let mut d = DVector::<f64>::zeros(rows);
    for (k, c) in usable.iter().enumerate() {
        let pos_a = sighting_position(&c.a, sites, common).expect("filtered liftable");
        let pos_b = sighting_position(&c.b, sites, common).expect("filtered liftable");
        let delta = pos_a - pos_b;
        d[2 * k] = delta.x;
        d[2 * k + 1] = delta.y;
        // d = J_a·b_a − J_b·b_b (see module docs), so side a enters with +J,
        // side b with −J; a geodetic side contributes no columns.
        for (side, sign) in [(&c.a, 1.0), (&c.b, -1.0)] {
            if let Sighting::Radar {
                sensor,
                measurement,
            } = side
            {
                let j = lift_jacobian(&sites[sensor], common, measurement) * sign;
                let col = column[sensor];
                h[(2 * k, col)] += j[(0, 0)];
                h[(2 * k, col + 1)] += j[(0, 1)];
                h[(2 * k + 1, col)] += j[(1, 0)];
                h[(2 * k + 1, col + 1)] += j[(1, 1)];
            }
        }
    }

    let rms = |v: &DVector<f64>| (v.norm_squared() / usable.len() as f64).sqrt();
    let rms_before_m = rms(&d);

    let svd = h.clone().svd(true, true);
    let s_max = svd.singular_values.max();
    let s_min = svd.singular_values.min();
    let observable = s_max > 0.0 && s_min > s_max * OBSERVABILITY_CUTOFF;
    // Minimum-norm least squares; the epsilon zeroes unobserved directions
    // instead of amplifying them.
    let x = svd
        .solve(&d, s_max * OBSERVABILITY_CUTOFF)
        .unwrap_or_else(|_| DVector::zeros(cols));
    let rms_after_m = rms(&(&d - &h * &x));

    let biases = column
        .iter()
        .map(|(&sensor, &col)| {
            (
                sensor,
                SensorBias {
                    range_m: x[col],
                    azimuth_rad: x[col + 1],
                },
            )
        })
        .collect();

    Some(RegistrationSolution {
        biases,
        rms_before_m,
        rms_after_m,
        observable,
    })
}

/// Build correspondences from a batch of plots by **Mode-S/ICAO identity**
/// (recommendation (a) of ADR 0034): two plots with the same globally unique
/// 24-bit address are the same aircraft, no kinematic gating needed.
///
/// For every polar plot of a sensor in `radar_sensors`, the nearest-in-time
/// plot of the same address from a **different** sensor within `max_dt_secs`
/// becomes its counterpart — a geodetic plot (ADS-B/FLARM self-report) as
/// reference truth, another listed radar as a second unknown. Plots without
/// an ICAO address are skipped (kinematic pairing is a follow-up). Keep
/// `max_dt_secs` tight: the estimator assumes time alignment, and an airliner
/// covers ~250 m per second of mismatch (module docs).
///
/// Radar↔radar pairs are only emitted from the lower-numbered sensor's side
/// so the same pair is not counted twice.
///
/// REQ: FR-TRK-037
pub fn correspondences_by_identity(
    plots: &[Plot],
    radar_sensors: &BTreeSet<SensorId>,
    max_dt_secs: f64,
) -> Vec<Correspondence> {
    // Group plot indices by ICAO address.
    let mut by_icao: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, p) in plots.iter().enumerate() {
        if let Some(icao) = p.mode_ac.icao_address {
            by_icao.entry(icao).or_default().push(i);
        }
    }

    let as_sighting = |p: &Plot| -> Option<Sighting> {
        match p.measurement {
            Measurement::Polar(m) if radar_sensors.contains(&p.sensor) => Some(Sighting::Radar {
                sensor: p.sensor,
                measurement: m,
            }),
            Measurement::Geodetic { position, .. } => Some(Sighting::Geodetic { position }),
            // A polar plot from an unlisted sensor is neither an unknown nor
            // trustworthy reference truth — unusable for registration.
            Measurement::Polar(_) => None,
        }
    };

    let mut out = Vec::new();
    for indices in by_icao.values() {
        for &i in indices {
            let plot = &plots[i];
            let Measurement::Polar(measurement) = plot.measurement else {
                continue;
            };
            if !radar_sensors.contains(&plot.sensor) {
                continue;
            }
            // Nearest-in-time usable counterpart from a different sensor.
            let counterpart = indices
                .iter()
                .filter(|&&j| j != i && plots[j].sensor != plot.sensor)
                .filter(|&&j| (plots[j].time.as_secs() - plot.time.as_secs()).abs() <= max_dt_secs)
                .filter(|&&j| as_sighting(&plots[j]).is_some())
                .min_by(|&&x, &&y| {
                    let dx = (plots[x].time.as_secs() - plot.time.as_secs()).abs();
                    let dy = (plots[y].time.as_secs() - plot.time.as_secs()).abs();
                    dx.partial_cmp(&dy)
                        .unwrap()
                        .then(plots[x].sensor.0.cmp(&plots[y].sensor.0))
                })
                .copied();
            let Some(j) = counterpart else { continue };
            let other = as_sighting(&plots[j]).expect("filtered usable");
            // Emit radar↔radar only from the lower-numbered sensor's side.
            if matches!(other, Sighting::Radar { sensor, .. } if plot.sensor.0 >= sensor.0) {
                continue;
            }
            out.push(Correspondence {
                a: Sighting::Radar {
                    sensor: plot.sensor,
                    measurement,
                },
                b: other,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::{ModeAC, Timestamp};
    use firefly_geo::Enu;

    /// Common (tracking) frame used by every test.
    fn common() -> LocalFrame {
        LocalFrame::new(Wgs84::from_degrees(50.0, 8.0, 0.0))
    }

    /// A radar site frame `east_m`/`north_m` from the common origin.
    fn site_at(common: &LocalFrame, east_m: f64, north_m: f64) -> LocalFrame {
        LocalFrame::new(common.enu_to_geodetic(&Enu::new(east_m, north_m, 0.0)))
    }

    /// Deterministic quasi-noise in [-1, 1] — no RNG dependency, same sequence
    /// every run (NFR-REPRO-001 spirit).
    fn noise(seed: u64) -> f64 {
        let x = (seed as f64 * 12.9898).sin() * 43_758.545;
        2.0 * (x - x.floor()) - 1.0
    }

    /// What a biased radar reports for a target truly at `truth` (common-frame
    /// ENU ground position): the true polar in its own frame plus the bias
    /// plus optional noise.
    fn biased_measurement(
        site: &LocalFrame,
        common: &LocalFrame,
        truth: Vector2<f64>,
        bias: SensorBias,
        noise_r: f64,
        noise_az: f64,
    ) -> Polar {
        let geodetic = common.enu_to_geodetic(&Enu::new(truth.x, truth.y, 0.0));
        let true_polar = site.geodetic_to_enu(&geodetic).to_polar();
        Polar::new(
            true_polar.range + bias.range_m + noise_r,
            true_polar.azimuth + bias.azimuth_rad + noise_az,
            true_polar.elevation,
        )
    }

    /// A ring of ground-truth targets around the common origin — azimuth
    /// diversity is what makes range and azimuth biases separable.
    fn target_ring(count: usize, radius_m: f64) -> Vec<Vector2<f64>> {
        (0..count)
            .map(|i| {
                let angle = i as f64 / count as f64 * std::f64::consts::TAU;
                Vector2::new(radius_m * angle.cos(), radius_m * angle.sin())
            })
            .collect()
    }

    /// The numerical lift Jacobian matches the analytic polar→ENU form when
    /// the site *is* the common frame (rotation ≈ identity). REQ: FR-TRK-037
    #[test]
    fn jacobian_matches_analytic_in_flat_geometry() {
        let frame = common();
        let m = Polar::new(40_000.0, 1.1, 0.0);
        let j = lift_jacobian(&frame, &frame, &m);
        // east = r·sinθ, north = r·cosθ (azimuth clockwise from north).
        let analytic = Matrix2::new(
            m.azimuth.sin(),
            m.range * m.azimuth.cos(),
            m.azimuth.cos(),
            -m.range * m.azimuth.sin(),
        );
        assert!(
            (j - analytic).norm() / analytic.norm() < 1e-4,
            "numerical {j} vs analytic {analytic}"
        );
    }

    /// A single radar's range and azimuth biases are recovered against
    /// geodetic (ADS-B) reference truth, despite measurement noise, and the
    /// corrected residuals shrink to noise level. REQ: FR-TRK-037
    #[test]
    fn single_radar_bias_recovered_against_geodetic_truth() {
        let common = common();
        let site = site_at(&common, 30_000.0, 0.0);
        let truth_bias = SensorBias {
            range_m: 150.0,
            azimuth_rad: 0.3_f64.to_radians(),
        };

        let mut correspondences = Vec::new();
        for (i, truth) in target_ring(48, 60_000.0).iter().enumerate() {
            let measurement = biased_measurement(
                &site,
                &common,
                *truth,
                truth_bias,
                50.0 * noise(i as u64),
                0.08_f64.to_radians() * noise(1000 + i as u64),
            );
            correspondences.push(Correspondence {
                a: Sighting::Radar {
                    sensor: SensorId(7),
                    measurement,
                },
                b: Sighting::Geodetic {
                    position: common.enu_to_geodetic(&Enu::new(truth.x, truth.y, 0.0)),
                },
            });
        }

        let sites = BTreeMap::from([(SensorId(7), site)]);
        let solution = estimate_biases(&common, &sites, &correspondences).expect("estimable");
        assert!(solution.observable, "ring geometry is fully observable");
        let est = solution.biases[&SensorId(7)];
        assert!(
            (est.range_m - truth_bias.range_m).abs() < 15.0,
            "range bias: estimated {} m, injected {} m",
            est.range_m,
            truth_bias.range_m
        );
        assert!(
            (est.azimuth_deg() - truth_bias.azimuth_deg()).abs() < 0.02,
            "azimuth bias: estimated {}°, injected {}°",
            est.azimuth_deg(),
            truth_bias.azimuth_deg()
        );
        assert!(
            solution.rms_after_m < solution.rms_before_m / 2.0,
            "correction shrinks residuals: before {} m, after {} m",
            solution.rms_before_m,
            solution.rms_after_m
        );
    }

    /// Two radars at different sites seeing the same targets: both biases are
    /// recovered from radar↔radar correspondences alone — the residual field
    /// of a bias depends on the site geometry, so distinct sites separate.
    /// REQ: FR-TRK-037
    #[test]
    fn two_radar_biases_recovered_from_mutual_sightings() {
        let common = common();
        let site_a = site_at(&common, -40_000.0, 0.0);
        let site_b = site_at(&common, 40_000.0, 10_000.0);
        let bias_a = SensorBias {
            range_m: 200.0,
            azimuth_rad: 0.25_f64.to_radians(),
        };
        let bias_b = SensorBias {
            range_m: -120.0,
            azimuth_rad: -0.15_f64.to_radians(),
        };

        let mut correspondences = Vec::new();
        for truth in target_ring(60, 70_000.0) {
            correspondences.push(Correspondence {
                a: Sighting::Radar {
                    sensor: SensorId(1),
                    measurement: biased_measurement(&site_a, &common, truth, bias_a, 0.0, 0.0),
                },
                b: Sighting::Radar {
                    sensor: SensorId(2),
                    measurement: biased_measurement(&site_b, &common, truth, bias_b, 0.0, 0.0),
                },
            });
        }

        let sites = BTreeMap::from([(SensorId(1), site_a), (SensorId(2), site_b)]);
        let solution = estimate_biases(&common, &sites, &correspondences).expect("estimable");
        assert!(solution.observable, "distinct sites separate the biases");
        let est_a = solution.biases[&SensorId(1)];
        let est_b = solution.biases[&SensorId(2)];
        assert!(
            (est_a.range_m - bias_a.range_m).abs() < 5.0
                && (est_a.azimuth_deg() - bias_a.azimuth_deg()).abs() < 0.01,
            "sensor 1: estimated ({} m, {}°)",
            est_a.range_m,
            est_a.azimuth_deg()
        );
        assert!(
            (est_b.range_m - bias_b.range_m).abs() < 5.0
                && (est_b.azimuth_deg() - bias_b.azimuth_deg()).abs() < 0.01,
            "sensor 2: estimated ({} m, {}°)",
            est_b.range_m,
            est_b.azimuth_deg()
        );
        assert!(solution.rms_after_m < 1.0, "noiseless → near-exact fit");
    }

    /// Bias-free sensors estimate to (numerically) zero. REQ: FR-TRK-037
    #[test]
    fn zero_bias_yields_zero_estimates() {
        let common = common();
        let site = site_at(&common, 20_000.0, -15_000.0);
        let correspondences: Vec<Correspondence> = target_ring(24, 50_000.0)
            .into_iter()
            .map(|truth| Correspondence {
                a: Sighting::Radar {
                    sensor: SensorId(3),
                    measurement: biased_measurement(
                        &site,
                        &common,
                        truth,
                        SensorBias::default(),
                        0.0,
                        0.0,
                    ),
                },
                b: Sighting::Geodetic {
                    position: common.enu_to_geodetic(&Enu::new(truth.x, truth.y, 0.0)),
                },
            })
            .collect();

        let sites = BTreeMap::from([(SensorId(3), site)]);
        let solution = estimate_biases(&common, &sites, &correspondences).expect("estimable");
        let est = solution.biases[&SensorId(3)];
        assert!(est.range_m.abs() < 1.0, "range ≈ 0, got {}", est.range_m);
        assert!(
            est.azimuth_deg().abs() < 0.005,
            "azimuth ≈ 0, got {}°",
            est.azimuth_deg()
        );
    }

    /// Two **co-located** radars pairing only with each other are
    /// rank-deficient — a common-mode bias cancels in every residual — and the
    /// solution is flagged unobservable. REQ: FR-TRK-037
    #[test]
    fn colocated_radars_without_reference_are_flagged_unobservable() {
        let common = common();
        let site = site_at(&common, 10_000.0, 0.0);
        let bias = SensorBias {
            range_m: 100.0,
            azimuth_rad: 0.1_f64.to_radians(),
        };
        let correspondences: Vec<Correspondence> = target_ring(30, 50_000.0)
            .into_iter()
            .map(|truth| Correspondence {
                a: Sighting::Radar {
                    sensor: SensorId(1),
                    measurement: biased_measurement(&site, &common, truth, bias, 0.0, 0.0),
                },
                b: Sighting::Radar {
                    sensor: SensorId(2),
                    measurement: biased_measurement(&site, &common, truth, bias, 0.0, 0.0),
                },
            })
            .collect();

        let sites = BTreeMap::from([(SensorId(1), site), (SensorId(2), site)]);
        let solution = estimate_biases(&common, &sites, &correspondences).expect("estimable");
        assert!(
            !solution.observable,
            "identical geometry cannot separate the two sensors' biases"
        );
    }

    /// No radar involvement → nothing to estimate. REQ: FR-TRK-037
    #[test]
    fn no_radar_correspondences_yield_none() {
        let common = common();
        let geodetic = Sighting::Geodetic {
            position: common.origin(),
        };
        assert!(estimate_biases(
            &common,
            &BTreeMap::new(),
            &[Correspondence {
                a: geodetic,
                b: geodetic
            }],
        )
        .is_none());
        assert!(estimate_biases(&common, &BTreeMap::new(), &[]).is_none());
    }

    // --- correspondence pairing -------------------------------------------

    fn radar_plot(sensor: u16, time: f64, icao: Option<u32>) -> Plot {
        Plot {
            sensor: SensorId(sensor),
            time: Timestamp(time),
            measurement: Measurement::Polar(Polar::new(50_000.0, 0.5, 0.0)),
            kind: firefly_core::DetectionKind::Secondary,
            source: firefly_core::SourceKind::ModeS,
            mode_ac: ModeAC {
                icao_address: icao,
                ..ModeAC::default()
            },
        }
    }

    fn adsb_plot(time: f64, icao: u32) -> Plot {
        Plot {
            sensor: SensorId(200),
            time: Timestamp(time),
            measurement: Measurement::Geodetic {
                position: Wgs84::from_degrees(50.1, 8.1, 10_000.0),
                sigma_pos_m: 75.0,
            },
            kind: firefly_core::DetectionKind::Secondary,
            source: firefly_core::SourceKind::AdsB,
            mode_ac: ModeAC {
                icao_address: Some(icao),
                ..ModeAC::default()
            },
        }
    }

    /// Pairing matches same-ICAO plots across sensors, picks the nearest in
    /// time within the window, and skips identity-less plots. REQ: FR-TRK-037
    #[test]
    fn pairing_matches_by_identity_nearest_in_time() {
        let radars = BTreeSet::from([SensorId(7)]);
        let plots = vec![
            radar_plot(7, 10.0, Some(0x3C_6589)),
            adsb_plot(10.4, 0x3C_6589), // nearest counterpart (|dt| = 0.4)
            adsb_plot(11.4, 0x3C_6589), // further away
            adsb_plot(10.1, 0xAB_CDEF), // different aircraft
            radar_plot(7, 20.0, None),  // no identity → skipped
        ];
        let pairs = correspondences_by_identity(&plots, &radars, 1.0);
        assert_eq!(pairs.len(), 1);
        assert!(matches!(pairs[0].a, Sighting::Radar { sensor, .. } if sensor == SensorId(7)));
        assert!(matches!(pairs[0].b, Sighting::Geodetic { .. }));
    }

    /// Counterparts outside the time window are not paired — the estimator
    /// assumes time alignment (module docs). REQ: FR-TRK-037
    #[test]
    fn pairing_respects_the_time_window() {
        let radars = BTreeSet::from([SensorId(7)]);
        let plots = vec![
            radar_plot(7, 10.0, Some(0x3C_6589)),
            adsb_plot(13.0, 0x3C_6589), // |dt| = 3.0 > window
        ];
        assert!(correspondences_by_identity(&plots, &radars, 1.0).is_empty());
    }

    /// A radar↔radar pair is emitted exactly once (from the lower-numbered
    /// sensor's side), not twice. REQ: FR-TRK-037
    #[test]
    fn radar_pairs_are_not_double_counted() {
        let radars = BTreeSet::from([SensorId(1), SensorId(2)]);
        let plots = vec![
            radar_plot(1, 10.0, Some(0x3C_6589)),
            radar_plot(2, 10.2, Some(0x3C_6589)),
        ];
        let pairs = correspondences_by_identity(&plots, &radars, 1.0);
        assert_eq!(pairs.len(), 1);
        assert!(
            matches!(pairs[0].a, Sighting::Radar { sensor, .. } if sensor == SensorId(1)),
            "emitted from the lower-numbered sensor"
        );
    }
}
