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

/// A correspondence whose two sightings are further apart than this (metres)
/// is identity contamination (duplicate ICAO/squawk pairing two different
/// airframes), not bias evidence — real registration errors are orders of
/// magnitude smaller (SPEC.1, FR-TRK-045).
const MAX_CORRESPONDENCE_SEPARATION_M: f64 = 5_000.0;

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
    // SPEC.1 (FR-TRK-045): duplicate-identity guard, kinematic and at the
    // right layer. Squawks are not globally unique and even ICAO addresses
    // duplicate in the field (transponder misconfiguration; ORCAM reuse at
    // borders — the Weeze case): an identity pairing can then join sightings
    // of two *different* airframes. A genuine registration error is metres
    // to a few hundred metres — a correspondence whose lifted positions lie
    // kilometres apart is identity contamination, not bias evidence, and is
    // discarded before it can poison the least squares.
    let usable: Vec<&Correspondence> = usable
        .into_iter()
        .filter(|c| {
            let pos_a = sighting_position(&c.a, sites, common).expect("filtered liftable");
            let pos_b = sighting_position(&c.b, sites, common).expect("filtered liftable");
            (pos_a - pos_b).norm() <= MAX_CORRESPONDENCE_SEPARATION_M
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

/// Tuning of the online [`RegistrationMonitor`] (REG.2a, ADR 0034). The
/// defaults are conservative: a couple of minutes of correspondences smooth
/// measurement noise, the tight pairing window keeps the time-alignment
/// assumption honest, and the minimum pair count refuses estimates from too
/// little evidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegistrationConfig {
    /// Sliding data-time window of retained plots, seconds.
    pub window_secs: f64,
    /// Pairing window handed to [`correspondences_by_identity`], seconds.
    pub pairing_max_dt_secs: f64,
    /// Minimum data time between two estimation runs, seconds.
    pub estimate_period_secs: f64,
    /// Minimum number of correspondences before an estimate is attempted.
    pub min_correspondences: usize,
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            window_secs: 120.0,
            pairing_max_dt_secs: 1.0,
            estimate_period_secs: 10.0,
            min_correspondences: 20,
        }
    }
}

/// **Online** registration monitor (REG.2a, ADR 0034): watches the live plot
/// stream, keeps a sliding data-time window of identity-carrying sightings,
/// and periodically re-runs the REG.1 estimator over them.
///
/// **Shadow mode by design:** the monitor only *observes* — it never touches
/// the plots on their way into the tracker. Its output feeds metrics and logs
/// so the operator can judge stability and plausibility of the estimates on
/// real data *before* REG.2b is allowed to feed them back into the fusion.
/// Like the tracker itself it is driven by **data time** (deterministic,
/// replayable, ADR 0003) — no wall clock.
///
/// REQ: FR-TRK-038
pub struct RegistrationMonitor {
    common: LocalFrame,
    sites: BTreeMap<SensorId, LocalFrame>,
    radar_sensors: BTreeSet<SensorId>,
    config: RegistrationConfig,
    /// Identity-carrying, registration-usable plots inside the window, in
    /// arrival order (data time is monotonic through the tracker's watermark).
    window: Vec<Plot>,
    last_estimate_at: Option<f64>,
    latest: Option<RegistrationSolution>,
    last_pair_count: usize,
    runs_total: u64,
    estimates_total: u64,
}

impl RegistrationMonitor {
    /// A monitor over the given radar geometry. `sites` maps each unknown-bias
    /// radar to its site frame; `common` is the tracker's frame.
    pub fn new(
        common: LocalFrame,
        sites: BTreeMap<SensorId, LocalFrame>,
        config: RegistrationConfig,
    ) -> Self {
        let radar_sensors = sites.keys().copied().collect();
        Self {
            common,
            sites,
            radar_sensors,
            config,
            window: Vec::new(),
            last_estimate_at: None,
            latest: None,
            last_pair_count: 0,
            runs_total: 0,
            estimates_total: 0,
        }
    }

    /// Feed a batch of ingested plots at data time `now` (the batch's newest
    /// plot time). Returns `Some` **only when this call produced a fresh
    /// estimate** — the caller logs/exports exactly once per run.
    pub fn observe(&mut self, plots: &[Plot], now: f64) -> Option<&RegistrationSolution> {
        // Retain only what registration can use: identity-carrying plots that
        // are either a listed radar's polar measurement or geodetic truth.
        self.window.extend(plots.iter().filter(|p| {
            p.mode_ac.icao_address.is_some()
                && match p.measurement {
                    Measurement::Polar(_) => self.radar_sensors.contains(&p.sensor),
                    Measurement::Geodetic { .. } => true,
                }
        }));
        let horizon = now - self.config.window_secs;
        self.window.retain(|p| p.time.as_secs() >= horizon);

        let due = self
            .last_estimate_at
            .is_none_or(|last| now - last >= self.config.estimate_period_secs);
        if !due {
            return None;
        }
        self.last_estimate_at = Some(now);
        self.runs_total += 1;

        let pairs = correspondences_by_identity(
            &self.window,
            &self.radar_sensors,
            self.config.pairing_max_dt_secs,
        );
        self.last_pair_count = pairs.len();
        if pairs.len() < self.config.min_correspondences {
            return None;
        }
        let solution = estimate_biases(&self.common, &self.sites, &pairs)?;
        self.estimates_total += 1;
        self.latest = Some(solution);
        self.latest.as_ref()
    }

    /// The most recent estimate, if any run has succeeded yet.
    pub fn latest(&self) -> Option<&RegistrationSolution> {
        self.latest.as_ref()
    }

    /// Correspondences found by the most recent estimation attempt (also set
    /// when the attempt was refused for too little evidence).
    pub fn last_pair_count(&self) -> usize {
        self.last_pair_count
    }

    /// How many estimates have been produced so far.
    pub fn estimates_total(&self) -> u64 {
        self.estimates_total
    }

    /// How many estimation **runs** have been attempted so far — due instants
    /// that went through pairing, whether or not they produced an estimate.
    /// Lets a caller drive per-run consumers (the REG.2b applier) exactly once
    /// per run, including refused ones.
    pub fn runs_total(&self) -> u64 {
        self.runs_total
    }
}

/// When is a registration estimate good enough to **apply** to the live
/// measurements (REG.2b)? The gate is deliberately conservative — a correction
/// feeds straight into the safety-relevant fusion path, so an estimate must
/// prove itself on every criterion or nothing is applied:
///
/// - **Observable**: rank-deficient geometries yield minimum-norm solutions
///   whose split between sensors is arbitrary — never applied.
/// - **Explains the residuals**: the corrected RMS must be a real improvement
///   over the raw RMS (`rms_after ≤ max_rms_ratio · rms_before`). An estimate
///   that barely shrinks the residuals is fitting noise, not a bias — and when
///   there *is* no bias, this criterion correctly keeps the correction at zero.
/// - **Plausible magnitude**: real calibration errors are tens–hundreds of
///   metres and tenths of a degree. A kilometre-scale "bias" is a data or
///   geometry fault; applying it would be worse than the disease.
///
/// Transitions are **smoothed** (`smoothing_alpha` per estimation run): the
/// applied correction is a low-pass of the accepted estimates, so a fresh
/// estimate never steps the air picture. When runs stop passing the gate the
/// correction is **held** for `hold_runs` runs (transient dropouts — a thin
/// traffic minute — should not unwind a good calibration), then decays back
/// toward zero at the same smoothing rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ApplyPolicy {
    /// Maximum plausible |range bias|, metres. Larger estimates are rejected.
    pub max_range_bias_m: f64,
    /// Maximum plausible |azimuth bias|, degrees. Larger estimates are rejected.
    pub max_azimuth_bias_deg: f64,
    /// Required residual improvement: accept only if
    /// `rms_after ≤ max_rms_ratio · rms_before`.
    pub max_rms_ratio: f64,
    /// Exponential smoothing factor per estimation run, in (0, 1]: the applied
    /// correction moves this fraction of the way toward the accepted estimate.
    pub smoothing_alpha: f64,
    /// Consecutive gate-failing runs tolerated before the applied correction
    /// starts decaying toward zero.
    pub hold_runs: u32,
}

impl Default for ApplyPolicy {
    fn default() -> Self {
        Self {
            max_range_bias_m: 1_000.0,
            max_azimuth_bias_deg: 1.0,
            max_rms_ratio: 0.5,
            smoothing_alpha: 0.3,
            hold_runs: 3,
        }
    }
}

/// Applied-bias magnitudes below these are treated as zero and dropped — they
/// are far beneath any sensor's noise floor, and dropping them lets
/// [`RegistrationApplier::active`] report an honest "no correction in effect".
const APPLIED_EPSILON_RANGE_M: f64 = 0.01;
const APPLIED_EPSILON_AZIMUTH_RAD: f64 = 1e-8;

/// **Applies** registration estimates to live measurements (REG.2b, ADR 0034):
/// keeps a smoothed per-sensor correction governed by an [`ApplyPolicy`] and
/// subtracts it from radar polar measurements before they reach the tracker.
///
/// Control-loop stability by construction: the estimator (the
/// [`RegistrationMonitor`]) keeps observing the **raw** stream, so its estimate
/// is the *full* bias, independent of what is currently applied. The applied
/// correction is then a pure low-pass of that estimate — there is no
/// integrator in the loop and nothing to oscillate. (Feeding the *corrected*
/// stream back into the estimator would instead require integrating residual
/// estimates, a genuinely feedback-coupled loop — deliberately avoided.)
///
/// Like the monitor this is pure and deterministic: state advances only via
/// [`update`](Self::update) calls, driven by the data-time estimation cadence.
///
/// REQ: FR-TRK-039
pub struct RegistrationApplier {
    policy: ApplyPolicy,
    applied: BTreeMap<SensorId, SensorBias>,
    gate_failures: u32,
}

impl RegistrationApplier {
    /// An applier with no correction in effect.
    pub fn new(policy: ApplyPolicy) -> Self {
        Self {
            policy,
            applied: BTreeMap::new(),
            gate_failures: 0,
        }
    }

    /// Would this solution pass the application gate?
    fn gate_accepts(&self, solution: &RegistrationSolution) -> bool {
        solution.observable
            && solution.rms_after_m <= self.policy.max_rms_ratio * solution.rms_before_m
            && solution.biases.values().all(|b| {
                b.range_m.abs() <= self.policy.max_range_bias_m
                    && b.azimuth_deg().abs() <= self.policy.max_azimuth_bias_deg
            })
    }

    /// Advance the applied correction by **one estimation run**: `outcome` is
    /// the run's fresh estimate, or `None` if the run was refused (thin
    /// evidence) or produced nothing. Call exactly once per monitor run.
    ///
    /// An accepted estimate pulls each sensor's applied correction a
    /// `smoothing_alpha` step toward it (sensors absent from the solution hold
    /// their current value — no pairs this window is not evidence of zero
    /// bias). A rejected/absent outcome counts toward `hold_runs`; beyond the
    /// hold, all applied corrections decay toward zero at the same rate.
    pub fn update(&mut self, outcome: Option<&RegistrationSolution>) {
        let alpha = self.policy.smoothing_alpha;
        match outcome.filter(|s| self.gate_accepts(s)) {
            Some(solution) => {
                self.gate_failures = 0;
                for (sensor, target) in &solution.biases {
                    let current = self.applied.entry(*sensor).or_default();
                    current.range_m += alpha * (target.range_m - current.range_m);
                    current.azimuth_rad += alpha * (target.azimuth_rad - current.azimuth_rad);
                }
            }
            None => {
                self.gate_failures = self.gate_failures.saturating_add(1);
                if self.gate_failures > self.policy.hold_runs {
                    for bias in self.applied.values_mut() {
                        bias.range_m -= alpha * bias.range_m;
                        bias.azimuth_rad -= alpha * bias.azimuth_rad;
                    }
                }
            }
        }
        self.applied.retain(|_, b| {
            b.range_m.abs() > APPLIED_EPSILON_RANGE_M
                || b.azimuth_rad.abs() > APPLIED_EPSILON_AZIMUTH_RAD
        });
    }

    /// The correction currently in effect, per sensor.
    pub fn applied(&self) -> &BTreeMap<SensorId, SensorBias> {
        &self.applied
    }

    /// Is any correction currently in effect?
    pub fn active(&self) -> bool {
        !self.applied.is_empty()
    }

    /// Subtract this sensor's applied bias from a radar plot's polar
    /// measurement (`measured = true + bias` ⇒ `true = measured − bias`).
    /// Plots from sensors without a correction, and geodetic plots, pass
    /// through unchanged.
    pub fn correct(&self, plot: &Plot) -> Plot {
        let Some(bias) = self.applied.get(&plot.sensor) else {
            return *plot;
        };
        let Measurement::Polar(m) = plot.measurement else {
            return *plot;
        };
        let mut corrected = *plot;
        corrected.measurement = Measurement::Polar(Polar::new(
            m.range - bias.range_m,
            (m.azimuth - bias.azimuth_rad).rem_euclid(std::f64::consts::TAU),
            m.elevation,
        ));
        corrected
    }
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

    // --- online monitor (REG.2a) ------------------------------------------

    /// One time step of the synthetic live stream: a biased radar plot and a
    /// truth-carrying ADS-B plot of the same aircraft at the same instant.
    fn stream_step(
        common: &LocalFrame,
        site: &LocalFrame,
        sensor: SensorId,
        t: f64,
        truth: Vector2<f64>,
        bias: SensorBias,
    ) -> Vec<Plot> {
        let icao = 0x3C_6589;
        let position = common.enu_to_geodetic(&Enu::new(truth.x, truth.y, 0.0));
        let identity = ModeAC {
            icao_address: Some(icao),
            ..ModeAC::default()
        };
        vec![
            Plot {
                sensor,
                time: Timestamp(t),
                measurement: Measurement::Polar(biased_measurement(
                    site, common, truth, bias, 0.0, 0.0,
                )),
                kind: firefly_core::DetectionKind::Secondary,
                source: firefly_core::SourceKind::ModeS,
                mode_ac: identity,
            },
            Plot {
                sensor: SensorId(200),
                time: Timestamp(t),
                measurement: Measurement::Geodetic {
                    position,
                    sigma_pos_m: 75.0,
                },
                kind: firefly_core::DetectionKind::Secondary,
                source: firefly_core::SourceKind::AdsB,
                mode_ac: identity,
            },
        ]
    }

    fn monitor_under_test(common: &LocalFrame, site: LocalFrame) -> RegistrationMonitor {
        RegistrationMonitor::new(
            *common,
            BTreeMap::from([(SensorId(7), site)]),
            RegistrationConfig::default(),
        )
    }

    /// Fed a synthetic live stream with an injected bias, the monitor produces
    /// an estimate once enough correspondences have accumulated — and recovers
    /// the bias. Shadow mode end-to-end. REQ: FR-TRK-038
    #[test]
    fn monitor_estimates_injected_bias_from_the_stream() {
        let common = common();
        let site = site_at(&common, 30_000.0, 0.0);
        let bias = SensorBias {
            range_m: 150.0,
            azimuth_rad: 0.3_f64.to_radians(),
        };
        let mut monitor = monitor_under_test(&common, site);

        // One aircraft flying around the ring: one pair per second — azimuth
        // diversity accumulates until the pair minimum and cadence allow a run.
        let ring = target_ring(48, 60_000.0);
        let mut produced = None;
        for (i, truth) in ring.iter().enumerate().take(30) {
            let t = i as f64;
            let batch = stream_step(&common, &site, SensorId(7), t, *truth, bias);
            if let Some(solution) = monitor.observe(&batch, t) {
                produced = Some(solution.clone());
            }
        }

        let solution = produced.expect("an estimate after enough evidence");
        assert!(solution.observable);
        let est = solution.biases[&SensorId(7)];
        assert!(
            (est.range_m - bias.range_m).abs() < 5.0,
            "range: {} m",
            est.range_m
        );
        assert!(
            (est.azimuth_deg() - bias.azimuth_deg()).abs() < 0.01,
            "azimuth: {}°",
            est.azimuth_deg()
        );
        assert_eq!(monitor.estimates_total(), 1, "cadence allows one run here");
    }

    /// Between two due instants the monitor does not re-estimate, no matter
    /// how much data arrives — the cadence is data-time driven.
    /// REQ: FR-TRK-038
    #[test]
    fn monitor_respects_the_estimation_cadence() {
        let common = common();
        let site = site_at(&common, 30_000.0, 0.0);
        let bias = SensorBias::default();
        let mut monitor = monitor_under_test(&common, site);

        let ring = target_ring(48, 60_000.0);
        for (i, truth) in ring.iter().enumerate().take(25) {
            let t = i as f64;
            monitor.observe(
                &stream_step(&common, &site, SensorId(7), t, *truth, bias),
                t,
            );
        }
        let runs = monitor.estimates_total();
        assert!(runs >= 1, "warm-up produced an estimate");

        // 5 s later (< estimate_period_secs = 10): plenty of data, no new run.
        let again = monitor.observe(
            &stream_step(&common, &site, SensorId(7), 29.0, ring[29], bias),
            29.0,
        );
        assert!(again.is_none(), "not due yet");
        assert_eq!(monitor.estimates_total(), runs);
    }

    /// Plots older than the sliding window are evicted: after a long silence
    /// the accumulated evidence is gone and an estimate is refused.
    /// REQ: FR-TRK-038
    #[test]
    fn monitor_evicts_stale_plots_and_refuses_thin_evidence() {
        let common = common();
        let site = site_at(&common, 30_000.0, 0.0);
        let bias = SensorBias::default();
        let mut monitor = monitor_under_test(&common, site);

        let ring = target_ring(48, 60_000.0);
        for (i, truth) in ring.iter().enumerate().take(25) {
            let t = i as f64;
            monitor.observe(
                &stream_step(&common, &site, SensorId(7), t, *truth, bias),
                t,
            );
        }
        assert!(monitor.estimates_total() >= 1);

        // Long gap: the window (120 s) has emptied; a lone new pair is far too
        // little evidence, so the due estimation run is refused.
        let runs = monitor.estimates_total();
        let t = 500.0;
        let none = monitor.observe(
            &stream_step(&common, &site, SensorId(7), t, ring[0], bias),
            t,
        );
        assert!(none.is_none());
        assert_eq!(monitor.estimates_total(), runs, "refused, not run");
        assert_eq!(monitor.last_pair_count(), 1, "only the fresh pair remains");
    }

    // --- application policy (REG.2b) ---------------------------------------

    /// A solution with the given bias for sensor 7 that comfortably passes the
    /// residual-improvement criterion.
    fn passing_solution(bias: SensorBias) -> RegistrationSolution {
        RegistrationSolution {
            biases: BTreeMap::from([(SensorId(7), bias)]),
            rms_before_m: 500.0,
            rms_after_m: 40.0,
            observable: true,
        }
    }

    /// The gate rejects unobservable solutions, insufficient residual
    /// improvement, and implausible magnitudes — each alone keeps the applied
    /// correction at zero. REQ: FR-TRK-039
    #[test]
    fn applier_gate_rejects_each_criterion_alone() {
        let bias = SensorBias {
            range_m: 150.0,
            azimuth_rad: 0.3_f64.to_radians(),
        };

        let unobservable = RegistrationSolution {
            observable: false,
            ..passing_solution(bias)
        };
        let no_improvement = RegistrationSolution {
            rms_before_m: 100.0,
            rms_after_m: 90.0,
            ..passing_solution(bias)
        };
        let implausible = passing_solution(SensorBias {
            range_m: 5_000.0,
            azimuth_rad: 0.0,
        });

        for rejected in [&unobservable, &no_improvement, &implausible] {
            let mut applier = RegistrationApplier::new(ApplyPolicy::default());
            applier.update(Some(rejected));
            assert!(
                !applier.active(),
                "gate must reject: observable={}, rms {}→{}, biases {:?}",
                rejected.observable,
                rejected.rms_before_m,
                rejected.rms_after_m,
                rejected.biases
            );
        }

        // Control: the passing solution engages a correction.
        let mut applier = RegistrationApplier::new(ApplyPolicy::default());
        applier.update(Some(&passing_solution(bias)));
        assert!(applier.active(), "the clean solution engages");
    }

    /// The applied correction approaches an accepted estimate exponentially —
    /// each run moves `alpha` of the remaining distance, so a fresh estimate
    /// never steps the correction. REQ: FR-TRK-039
    #[test]
    fn applier_smooths_toward_the_accepted_estimate() {
        let bias = SensorBias {
            range_m: 200.0,
            azimuth_rad: 0.2_f64.to_radians(),
        };
        let policy = ApplyPolicy::default();
        let mut applier = RegistrationApplier::new(policy);

        applier.update(Some(&passing_solution(bias)));
        let first = applier.applied()[&SensorId(7)];
        assert!(
            (first.range_m - policy.smoothing_alpha * bias.range_m).abs() < 1e-9,
            "first step is alpha of the target: {} m",
            first.range_m
        );

        for _ in 0..30 {
            applier.update(Some(&passing_solution(bias)));
        }
        let converged = applier.applied()[&SensorId(7)];
        assert!(
            (converged.range_m - bias.range_m).abs() < 0.1
                && (converged.azimuth_rad - bias.azimuth_rad).abs() < 1e-6,
            "converged to the estimate: {} m / {}°",
            converged.range_m,
            converged.azimuth_deg()
        );
    }

    /// Gate-failing runs first HOLD the correction (transient dropouts must
    /// not unwind a good calibration), then decay it toward zero — and the
    /// applier reports inactive once it is numerically gone. REQ: FR-TRK-039
    #[test]
    fn applier_holds_then_decays_after_sustained_gate_failures() {
        let bias = SensorBias {
            range_m: 150.0,
            azimuth_rad: 0.0,
        };
        let policy = ApplyPolicy::default();
        let mut applier = RegistrationApplier::new(policy);
        for _ in 0..30 {
            applier.update(Some(&passing_solution(bias)));
        }
        let held = applier.applied()[&SensorId(7)].range_m;

        // Within the hold budget: the correction stays put.
        for _ in 0..policy.hold_runs {
            applier.update(None);
        }
        assert_eq!(
            applier.applied()[&SensorId(7)].range_m,
            held,
            "held through transient dropouts"
        );

        // Beyond the hold: decay sets in and eventually reaches zero/inactive.
        for _ in 0..80 {
            applier.update(None);
        }
        assert!(
            !applier.active(),
            "sustained gate failure decays the correction away"
        );
    }

    /// `correct` subtracts the applied bias from a known radar's polar
    /// measurement and leaves geodetic plots and unknown sensors untouched.
    /// REQ: FR-TRK-039
    #[test]
    fn correct_subtracts_bias_only_for_known_radar_measurements() {
        let bias = SensorBias {
            range_m: 150.0,
            azimuth_rad: 0.3_f64.to_radians(),
        };
        let mut applier = RegistrationApplier::new(ApplyPolicy {
            smoothing_alpha: 1.0, // jump straight to the target for this test
            ..ApplyPolicy::default()
        });
        applier.update(Some(&passing_solution(bias)));

        let radar = radar_plot(7, 10.0, Some(0x3C_6589));
        let corrected = applier.correct(&radar);
        let (Measurement::Polar(raw), Measurement::Polar(cor)) =
            (radar.measurement, corrected.measurement)
        else {
            panic!("polar in, polar out");
        };
        assert!((cor.range - (raw.range - bias.range_m)).abs() < 1e-9);
        assert!((cor.azimuth - (raw.azimuth - bias.azimuth_rad)).abs() < 1e-9);
        assert_eq!(cor.elevation, raw.elevation);

        let other = radar_plot(8, 10.0, Some(0x3C_6589));
        assert_eq!(applier.correct(&other), other, "unknown sensor untouched");
        let truth = adsb_plot(10.0, 0x3C_6589);
        assert_eq!(applier.correct(&truth), truth, "geodetic untouched");
    }

    /// Closed chain (monitor → gate → smoothing → correction) over the
    /// synthetic live stream: the applied correction converges to the injected
    /// bias without oscillating, and correcting a fresh biased measurement
    /// lands it on the truth. REQ: FR-TRK-039
    #[test]
    fn closed_chain_converges_to_the_injected_bias() {
        let common = common();
        let site = site_at(&common, 30_000.0, 0.0);
        let bias = SensorBias {
            range_m: 150.0,
            azimuth_rad: 0.3_f64.to_radians(),
        };
        let mut monitor = monitor_under_test(&common, site);
        let mut applier = RegistrationApplier::new(ApplyPolicy::default());

        // Drive the live loop exactly as the server does: observe raw plots,
        // update the applier once per estimation run. Track the applied range
        // bias to prove monotone (non-oscillating) convergence.
        let ring = target_ring(48, 60_000.0);
        let mut last_applied = 0.0_f64;
        for i in 0..150 {
            let t = i as f64;
            let truth = ring[i % ring.len()];
            let batch = stream_step(&common, &site, SensorId(7), t, truth, bias);
            let runs_before = monitor.runs_total();
            let fresh = monitor.observe(&batch, t).cloned();
            if monitor.runs_total() > runs_before {
                applier.update(fresh.as_ref());
                let now_applied = applier
                    .applied()
                    .get(&SensorId(7))
                    .map_or(0.0, |b| b.range_m);
                assert!(
                    now_applied >= last_applied - 1e-6,
                    "no oscillation: applied fell from {last_applied} to {now_applied}"
                );
                last_applied = now_applied;
            }
        }

        let applied = applier.applied()[&SensorId(7)];
        assert!(
            (applied.range_m - bias.range_m).abs() < 5.0
                && (applied.azimuth_deg() - bias.azimuth_deg()).abs() < 0.01,
            "converged: {} m / {}° vs injected {} m / {}°",
            applied.range_m,
            applied.azimuth_deg(),
            bias.range_m,
            bias.azimuth_deg()
        );

        // The proof of the pudding: a fresh biased measurement, corrected,
        // lifts to (numerically) the true position.
        let truth = ring[0];
        let batch = stream_step(&common, &site, SensorId(7), 150.0, truth, bias);
        let corrected = applier.correct(&batch[0]);
        let lifted = lift(
            &site,
            &common,
            &match corrected.measurement {
                Measurement::Polar(m) => m,
                _ => unreachable!(),
            },
        );
        assert!(
            (lifted - truth).norm() < 10.0,
            "corrected measurement sits {} m from truth",
            (lifted - truth).norm()
        );
    }
}
