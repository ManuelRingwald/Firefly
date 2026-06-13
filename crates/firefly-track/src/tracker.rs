//! The tracker: the per-scan loop that turns a plot stream into managed tracks.
//!
//! Each scan (a batch of plots sharing a time) drives one pure state
//! transition. The order matters:
//!
//! 1. **Predict** every existing track to the scan time.
//! 2. **Convert** each plot to a Cartesian measurement (Häppchen 2.1) and
//!    refresh the feed-cadence estimate (ADR 0012).
//! 3. **Associate** predicted tracks with measurements (JPDA, Häppchen
//!    M5.5–M5.9): **update** associated tracks (a *hit*) and **initiate** a
//!    new tentative track from each unassociated plot.
//! 4. **Confirm** tentative tracks that reach M-of-N hits within an adaptive
//!    time window (ADR 0012).
//! 5. **Delete** tracks that have coasted past their missed-revisit budget
//!    (ADR 0012).
//!
//! Determinism (ADR 0003): [`Tracker::process_scan`] is a pure function of the
//! current state, the scan time and the plots — no wall clock, no I/O — so the
//! whole run is replayable and the state is recoverable. The state
//! ([`Track`] list) is plain, serialisable data (NFR-CLOUD-001/002/003).

use std::collections::BTreeMap;

use firefly_core::{Plot, SensorId, SystemTrack, Timestamp, TrackId};
use firefly_geo::{Enu, LocalFrame};
use serde::{Deserialize, Serialize};

use crate::gating::Gate;
use crate::imm::ImmConfig;
use crate::jpda::joint_association_probabilities;
use crate::kalman::{LinearKalman, ProcessNoise};
use crate::measurement::{convert_plot, CartesianMeasurement, SensorErrorModel};
use crate::pda::ClutterModel;
use crate::track::{Track, TrackStatus};

/// Below this "no detection" probability `β_0`, a track is treated as having
/// at least one plot in its gate (a hit) rather than coasting.
const NO_DETECTION_EPSILON: f64 = 1e-9;

/// Assumed revisit interval (seconds) for a track that has not yet established
/// its own from a second hit — the bootstrap of the asynchronous,
/// time-continuous lifecycle (ADR 0013, Häppchen 13.2). A freshly born track is
/// protected for a few of these nominal revisits before it can be deleted (the
/// time-continuous analog of ADR 0012's cadence bootstrap). It is deliberately
/// a single, local constant for now; it becomes a 12-factor config knob when the
/// periodic output stage lands (Häppchen 13.4).
const NOMINAL_REVISIT_INTERVAL: f64 = 5.0;

/// Where one sensor sits and how noisy the tracker believes it is.
///
/// Central measurement fusion (ADR 0010) needs both per sensor: the [`LocalFrame`]
/// to lift that sensor's plots into the common tracking frame, and the
/// [`SensorErrorModel`] to weigh them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SensorModel {
    /// The sensor's own local ENU frame (anchored at the radar site).
    pub frame: LocalFrame,
    /// The tracker's assumed measurement noise for this sensor.
    pub error: SensorErrorModel,
}

/// Tunable parameters of the tracker plus the sensor geometry it fuses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackerConfig {
    /// The common frame the tracker reasons in (the system reference point).
    /// Every plot is lifted into this frame before gating/association, and the
    /// estimate is reported back out through it (ADR 0010).
    pub tracking_frame: LocalFrame,
    /// Per-sensor geometry and noise, keyed by sensor id. A plot whose sensor is
    /// not registered here cannot be geolocated and is dropped.
    pub sensors: BTreeMap<SensorId, SensorModel>,
    /// Process noise (the manoeuvre budget) for prediction.
    pub process_noise: ProcessNoise,
    /// The IMM bank recipe stamped onto every new track (motion models, Markov
    /// switching, prior model probabilities) — Häppchen M5.4.
    pub imm: ImmConfig,
    /// The validation gate used for **association**: which plots are folded
    /// into a track's estimate.
    pub gate: Gate,
    /// The (wider) gate used only to **suppress track initiation** (ADR 0011).
    /// A plot inside this gate of some existing track does not seed a *new*
    /// track — it is treated as that track's own (possibly outlier) detection,
    /// not a new aircraft. Made looser than [`gate`](Self::gate) so a rare
    /// 3σ-tail plot of an already-tracked target cannot spawn a confirmable
    /// "ghost"; association itself still uses the tighter [`gate`](Self::gate),
    /// keeping the state estimate precise.
    pub init_gate: Gate,
    /// The clutter environment for JPDA association (Häppchen M5.9):
    /// expected false-plot density and detection probability.
    pub clutter: ClutterModel,
    /// Confirmation needs `confirm_m` hits within the last `confirm_n` scans.
    pub confirm_m: usize,
    /// Window length for the M-of-N confirmation rule.
    pub confirm_n: usize,
    /// Delete a *tentative* track after this many consecutive misses.
    pub delete_misses_tentative: u32,
    /// Delete a *confirmed* track after this many consecutive misses.
    pub delete_misses_confirmed: u32,
    /// Initial 1σ velocity uncertainty for a newly born track, m/s.
    pub initial_velocity_std: f64,
}

impl TrackerConfig {
    /// An empty config anchored at a common tracking frame; add sensors with
    /// [`TrackerConfig::with_sensor`]. Defaults: confirm 3-of-5, delete tentative
    /// after 2 misses and confirmed after 4.
    pub fn new(tracking_frame: LocalFrame) -> Self {
        Self {
            tracking_frame,
            sensors: BTreeMap::new(),
            process_noise: ProcessNoise::new(0.5),
            // A civil rate-one turn is ~3°/s ≈ 0.052 rad/s.
            imm: ImmConfig::cv_and_turns(0.052),
            gate: Gate::from_probability(0.99),
            // The initiation-suppression gate is deliberately wider than the
            // association gate: a plot up to this far from an existing track is
            // its own outlier, not a new aircraft, so it must not seed a ghost.
            init_gate: Gate::from_probability(0.9999),
            // A sparse clutter environment: ~1 false plot per 10 km², detected
            // 95% of the time. Mainly relevant when two tracks' gates overlap
            // (JPDA exclusivity); for an isolated track and plot it leaves the
            // pre-JPDA behaviour essentially unchanged.
            clutter: ClutterModel::new(1.0e-7, 0.95),
            confirm_m: 3,
            confirm_n: 5,
            delete_misses_tentative: 2,
            delete_misses_confirmed: 4,
            initial_velocity_std: 200.0,
        }
    }

    /// Register one sensor (its frame and assumed noise). Chainable.
    pub fn with_sensor(mut self, id: SensorId, frame: LocalFrame, error: SensorErrorModel) -> Self {
        self.sensors.insert(id, SensorModel { frame, error });
        self
    }

    /// Single-sensor convenience: the tracking frame *is* the sensor's own frame
    /// (so the lift into the common frame is the identity). This reproduces the
    /// pre-M4 single-radar behaviour.
    pub fn single_sensor(id: SensorId, frame: LocalFrame, error: SensorErrorModel) -> Self {
        Self::new(frame).with_sensor(id, frame, error)
    }
}

/// A single-radar multi-target tracker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tracker {
    config: TrackerConfig,
    tracks: Vec<Track>,
    next_id: u32,
    /// Data time of the previous scan, for the inter-scan gap (ADR 0012).
    prev_scan_time: Option<f64>,
    /// Last data time each sensor delivered plots, to estimate its scan period.
    sensor_last_scan: BTreeMap<SensorId, f64>,
    /// Most recently observed scan period (seconds) per sensor. The slowest of
    /// these — or the inter-scan gap, whichever is larger — is the feed
    /// *cadence* that scales the adaptive lifecycle windows, so an asynchronous
    /// feed (small gaps between sensors, but each sensor revisiting every few
    /// seconds) keeps tracks alive across a single sensor's revisit.
    sensor_period: BTreeMap<SensorId, f64>,
}

impl Tracker {
    pub fn new(config: TrackerConfig) -> Self {
        Self {
            config,
            tracks: Vec::new(),
            next_id: 1,
            prev_scan_time: None,
            sensor_last_scan: BTreeMap::new(),
            sensor_period: BTreeMap::new(),
        }
    }

    /// All tracks the tracker currently maintains (tentative and confirmed).
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// The tracker's configuration, including per-sensor geometry
    /// ([`SensorModel::frame`]) needed to geolocate raw plots (M6.3).
    pub fn config(&self) -> &TrackerConfig {
        &self.config
    }

    /// Only the confirmed tracks — the air picture worth reporting.
    pub fn confirmed_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(|t| t.is_confirmed())
    }

    /// Number of confirmed tracks.
    pub fn confirmed_count(&self) -> usize {
        self.tracks.iter().filter(|t| t.is_confirmed()).count()
    }

    /// Project the current tracks into neutral, geodetic [`SystemTrack`]s.
    ///
    /// This is the tracker's **output port** (Ports & Adapters, NFR-INT-001/002).
    /// The internal estimate lives in the **common tracking frame** (ADR 0010);
    /// to report it to the outside world we lift each track's position back to
    /// WGS84 through that frame.
    ///
    /// Height is reported as the tracking-frame origin's height: the tracker is
    /// 2-D for now (no Mode-C), so it carries no independent vertical estimate.
    ///
    /// REQ: NFR-INT-001, NFR-INT-002
    pub fn system_tracks(&self) -> Vec<SystemTrack> {
        let frame = &self.config.tracking_frame;
        self.tracks
            .iter()
            .map(|track| {
                let p = track.position();
                let v = track.velocity();
                // The 2-D estimate sits on the tracking plane (up = 0), i.e. at
                // the tracking-frame origin's ellipsoidal height.
                let position = frame.enu_to_geodetic(&Enu::new(p[0], p[1], 0.0));
                SystemTrack {
                    id: track.id(),
                    time: Timestamp(track.last_time),
                    position,
                    v_east: v[0],
                    v_north: v[1],
                    confirmed: track.is_confirmed(),
                    coasting: track.is_coasting(),
                    update_age: track.update_age(),
                    position_uncertainty: track.estimate().position_uncertainty(),
                    mode_3a: track.mode_3a(),
                    icao_address: track.icao_address(),
                    contributing_sensors: track.contributing_sensors().iter().copied().collect(),
                }
            })
            .collect()
    }

    /// Process one scan: a batch of plots that share the time `time`.
    ///
    /// The batch may be empty (no detections this scan), in which case every
    /// track simply coasts. The plots may come from **several sensors** (central
    /// measurement fusion, ADR 0010). Each sensor is processed **sequentially**
    /// in a deterministic (sensor-id) order: its plots are converted with that
    /// sensor's noise model, lifted into the common tracking frame, then gated
    /// and associated against the *current* tracks (including any born earlier
    /// this scan). This is what fuses two radars seeing one aircraft into a
    /// single track instead of a ghost per sensor — a second sensor's plot
    /// associates to the track the first already updated, rather than spawning
    /// its own. Hit/miss is then booked **once per scan** (a track is a hit if
    /// *any* sensor saw it), so the M-of-N lifecycle is unchanged for a single
    /// sensor. A plot from an unregistered sensor cannot be geolocated and is
    /// dropped.
    ///
    /// REQ: FR-TRK-001, FR-TRK-006, FR-TRK-010
    pub fn process_scan(&mut self, time: Timestamp, plots: &[Plot]) {
        // Copy the cheap scalar tuning out so the rest can mutate `self.tracks`
        // without holding a borrow on `self.config` (which now owns a map).
        let process_noise = self.config.process_noise;
        let gate = self.config.gate;
        let init_gate = self.config.init_gate;
        let confirm_m = self.config.confirm_m;
        let confirm_n = self.config.confirm_n;
        let initial_velocity_std = self.config.initial_velocity_std;
        let delete_tentative = self.config.delete_misses_tentative;
        let delete_confirmed = self.config.delete_misses_confirmed;
        let tracking_frame = self.config.tracking_frame;
        let imm_config = self.config.imm.clone();
        let clutter = self.config.clutter;
        let t = time.as_secs();

        // 1. Predict every existing track's IMM forward to the scan time (mixing
        //    + per-model prediction), and clear last scan's sensor provenance
        //    (it is rebuilt below).
        for track in &mut self.tracks {
            let dt = t - track.last_time;
            if dt > 0.0 {
                track.imm.predict(dt, &process_noise);
                track.last_time = t;
            }
            track.reset_contributing_sensors();
        }

        // 2. Convert each plot to a Cartesian measurement in the *common tracking
        //    frame* (polar→Cartesian in the sensor's own frame, then lift position
        //    + covariance into the tracking frame) and group the survivors by
        //    sensor. The BTreeMap fixes a deterministic processing order; plots
        //    from an unregistered sensor are skipped.
        let mut by_sensor: BTreeMap<SensorId, Vec<(&Plot, CartesianMeasurement)>> = BTreeMap::new();
        for p in plots {
            if let Some(model) = self.config.sensors.get(&p.sensor) {
                let local = convert_plot(&p.measurement, &model.error);
                // `convert_plot` keeps only the ground-projected east/north; the
                // target's height above the sensor's tangent plane is
                // range·sin(elevation) — exactly the `up` it dropped. Passing it
                // lets `horizontal_from` lift the *full 3-D* point into the
                // tracking frame, so a second radar maps the same airborne target
                // to the same horizontal point instead of a height-offset ghost.
                let height = p.measurement.range * p.measurement.elevation.sin();
                let (z, r) = tracking_frame.horizontal_from(&model.frame, local.z, height, local.r);
                by_sensor
                    .entry(p.sensor)
                    .or_default()
                    .push((p, CartesianMeasurement { z, r }));
            }
        }

        // 2b. Refresh the feed-cadence estimate that the adaptive lifecycle
        //     (ADR 0012) scales its coast/confirm windows by: the larger of the
        //     slowest sensor's scan period and the gap since the previous scan.
        //     Capture the inter-scan gap first, then fold in this scan's
        //     per-sensor periods, so a single async radar revisiting every few
        //     seconds (not the much shorter gap between *different* sensors)
        //     governs how long a track may coast.
        let inter_scan = self.prev_scan_time.map_or(0.0, |p| t - p);
        for &sensor in by_sensor.keys() {
            if let Some(&last) = self.sensor_last_scan.get(&sensor) {
                let period = t - last;
                if period > 0.0 {
                    self.sensor_period.insert(sensor, period);
                }
            }
            self.sensor_last_scan.insert(sensor, t);
        }
        // Bootstrap: a scan that brings plots before *any* sensor has completed
        // a second scan gives no basis for a cadence estimate — `inter_scan`
        // may just be the (much shorter) gap between two *different*
        // asynchronous sensors' first scans, not a missed revisit. Treat the
        // cadence as unbounded in that case, so a track born in this window
        // isn't deleted before its founding sensor even gets a chance to
        // revisit it. A scan with no plots at all (pure coasting) is not part
        // of this bootstrap window — it falls back to the inter-scan gap, as
        // before.
        let cadence = if !by_sensor.is_empty() && self.sensor_period.is_empty() {
            f64::INFINITY
        } else {
            inter_scan.max(self.sensor_period.values().copied().fold(0.0, f64::max))
        };
        self.prev_scan_time = Some(t);

        // 3. Associate & update each sensor in turn with JPDA (Häppchen
        //    M5.5–M5.9): every track folds in *all* its gated plots at once via
        //    `Imm::update_pda`, weighted by joint association probabilities `β`
        //    that respect exclusivity — two tracks cannot both "claim" the same
        //    plot. A track is recorded as hit (this scan) when at least one plot
        //    fell in its gate (`β_0 < 1`); the gated plot with the largest `β`
        //    is used for identity bookkeeping (Mode 3/A, ICAO address).
        //
        //    Crucially, all sensors gate and associate against **one common
        //    reference frozen at scan start** (ADR 0011), not against each
        //    track's live estimate. Processing one sensor folds a plot into a
        //    track and *shrinks* its covariance; gating the next sensor against
        //    that tightened track would push the same aircraft's plot from a
        //    second radar out of the gate, and step 3's initiation would then
        //    spawn a duplicate ("ghost"). Freezing the reference removes that
        //    sequential tightening. State fusion stays sequential, which for
        //    independent measurements yields the same joint posterior.
        //
        //    Hits are recorded on the track immediately (`mark_hit`), which also
        //    refreshes its revisit-interval estimate; the lifecycle (steps 4–5)
        //    then reads those times. There is no per-scan miss booking — a miss
        //    is simply the absence of a hit, measured by the update age.
        // `reference[k]` is the gating/association estimate for `self.tracks[k]`:
        // the scan-start prediction for tracks that existed then (frozen for the
        // whole scan), or the fresh estimate of a track initiated *during* this
        // scan — appended so a later sensor associates to it instead of starting
        // its own (one aircraft, one track). It stays index-aligned with
        // `self.tracks`, both growing together as tracks are born.
        let mut reference: Vec<LinearKalman> = self.tracks.iter().map(|tr| tr.estimate()).collect();

        for (&sensor, items) in &by_sensor {
            let measurements: Vec<CartesianMeasurement> = items.iter().map(|(_, m)| *m).collect();
            let betas = joint_association_probabilities(&reference, &measurements, &gate, &clutter);

            for (ti, track_betas) in betas.iter().enumerate() {
                if track_betas[0] >= 1.0 - NO_DETECTION_EPSILON {
                    continue; // nothing in this track's gate from this sensor
                }

                // The gated plots and their association weights, in the order
                // they appear in `measurements`; remember the most likely one
                // for identity bookkeeping.
                let mut gated_measurements = Vec::new();
                let mut gated_betas = vec![track_betas[0]];
                let mut best: Option<(usize, f64)> = None;
                for (mi, &beta) in track_betas[1..].iter().enumerate() {
                    if beta > 0.0 {
                        gated_measurements.push(measurements[mi]);
                        gated_betas.push(beta);
                        if best.is_none_or(|(_, b)| beta > b) {
                            best = Some((mi, beta));
                        }
                    }
                }

                self.tracks[ti]
                    .imm
                    .update_pda(&gated_measurements, &gated_betas);
                self.tracks[ti].mark_hit(t);
                self.tracks[ti].record_hit_from(sensor);
                if let Some((mi, _)) = best {
                    self.tracks[ti].update_identity(&items[mi].0.mode_ac);
                }
            }

            // Initiate a new tentative track from each plot that fell in *no*
            // reference track's gate. The veto uses `reference` as frozen at the
            // *start* of this sensor's block, so two close-but-distinct plots
            // from the *same* sensor (a sensor reports each target at most once
            // per scan) do not veto one another — they are different targets and
            // must each seed a track. The new tracks are appended only *after*
            // the loop, so they veto the *next* sensor's plots (one aircraft seen
            // by two radars → one track), not their own siblings.
            let mut newborn = Vec::new();
            for (mi, m) in measurements.iter().enumerate() {
                // Suppress initiation with the wider `init_gate`: a plot near an
                // existing track is its own (outlier) detection, not a new
                // aircraft — this is what stops a 3σ-tail plot of a tracked
                // target from spawning a ghost (ADR 0011).
                if reference
                    .iter()
                    .any(|f| init_gate.accepts_measurement(f, m))
                {
                    continue;
                }
                let filter = LinearKalman::from_first_measurement(m, initial_velocity_std);
                let imm = imm_config.seed(filter);
                let id = TrackId(self.next_id);
                self.next_id += 1;
                let mut track = Track::new(id, imm, t);
                track.update_identity(&items[mi].0.mode_ac);
                track.record_hit_from(sensor);
                newborn.push(track.estimate());
                self.tracks.push(track);
            }
            reference.extend(newborn);
        }

        // 4. Confirm tentative tracks that have collected `confirm_m` hits within
        //    the last `confirm_n` revisit intervals (ADR 0012). The window is a
        //    *time* span scaled by each track's own cadence, so confirmation is
        //    robust whether one radar revisits every few seconds or several
        //    asynchronous radars deliver hits far more often.
        for track in &mut self.tracks {
            if track.status() == TrackStatus::Tentative {
                let window = confirm_n as f64 * track.coast_reference(cadence);
                if track.hits_within(window, t) >= confirm_m {
                    track.confirm();
                }
            }
        }

        // 5. Delete tracks that have coasted past their allowed number of missed
        //    revisits (ADR 0012): `update_age > budget · max(revisit, scan dt)`.
        //    Measuring the budget in *revisit intervals* (not raw seconds) keeps
        //    deletion governed by how many updates were missed, independent of
        //    the feed's absolute pace (NFR-CLOUD-004), while tolerating the
        //    interleaved misses an asynchronous multi-radar feed produces.
        self.tracks
            .retain(|track| !should_delete(track, delete_tentative, delete_confirmed, cadence));
    }

    /// Process a **single plot at its own data time** — the asynchronous,
    /// per-plot counterpart to [`process_scan`](Self::process_scan) (ADR 0013,
    /// Häppchen 13.1).
    ///
    /// Where `process_scan` folds a *batch* of same-time plots against one
    /// reference frozen at scan start (ADR 0011, so two radars seeing the same
    /// aircraft *at the same instant* fuse into one track instead of a ghost),
    /// this processes one measurement at the instant it was actually taken:
    ///
    /// 1. **Predict** every track to the plot's own time (not a shared scan
    ///    time).
    /// 2. **Associate** the plot against the tracks' **live** estimates (no
    ///    frozen reference). The sequential-tightening ghost that ADR 0011
    ///    guarded against does not arise here: asynchronous plots are separated
    ///    in time, so each track's covariance has already grown by prediction
    ///    before the next plot is gated against it.
    /// 3. **Update** every track the plot gates into (soft PDA over the single
    ///    measurement, with JPDA exclusivity across tracks), or **initiate** a
    ///    new tentative track when the plot falls in no track's (wider)
    ///    initiation gate.
    /// 4. **Confirm** / **delete** via the same time-scaled lifecycle.
    ///
    /// Determinism (ADR 0003) is preserved: this is a pure function of the
    /// state and the plot's data time and measurement — no wall clock.
    ///
    /// **Scope of Häppchen 13.1 (ADR 0013, Ansatz B — additive).** This is a
    /// *new* entry point that coexists with [`process_scan`](Self::process_scan);
    /// the batch path is unchanged and still drives the Player until the
    /// asynchronous output pipeline (13.4/13.5) switches over. The feed-cadence
    /// and contributing-sensor bookkeeping are kept deliberately minimal here —
    /// they are reworked onto true time-continuity in 13.2/13.4.
    ///
    /// The lifecycle (steps 4–5) is **time-continuous** (Häppchen 13.2): it is
    /// governed by each track's own measured revisit interval, not a
    /// globally-estimated feed cadence (see [`should_delete_continuous`]).
    ///
    /// REQ: FR-TRK-001, FR-TRK-006, FR-TRK-010, FR-TRK-022, FR-TRK-023
    pub fn process_plot(&mut self, plot: &Plot) {
        // Cheap scalar tuning copied out so the rest can mutate `self.tracks`
        // without holding a borrow on `self.config`.
        let process_noise = self.config.process_noise;
        let gate = self.config.gate;
        let init_gate = self.config.init_gate;
        let confirm_m = self.config.confirm_m;
        let confirm_n = self.config.confirm_n;
        let initial_velocity_std = self.config.initial_velocity_std;
        let delete_tentative = self.config.delete_misses_tentative;
        let delete_confirmed = self.config.delete_misses_confirmed;
        let tracking_frame = self.config.tracking_frame;
        let imm_config = self.config.imm.clone();
        let clutter = self.config.clutter;
        let t = plot.time.as_secs();

        // A plot from an unregistered sensor cannot be geolocated — drop it,
        // exactly as the batch path does.
        let Some(model) = self.config.sensors.get(&plot.sensor) else {
            return;
        };
        let sensor = plot.sensor;

        // Convert polar→Cartesian in the sensor's own frame, then lift the full
        // 3-D point into the common tracking frame (the height is carried
        // through `horizontal_from` so an airborne target maps to one
        // horizontal point regardless of which radar saw it — see
        // `process_scan`).
        let local = convert_plot(&plot.measurement, &model.error);
        let height = plot.measurement.range * plot.measurement.elevation.sin();
        let (z, r) = tracking_frame.horizontal_from(&model.frame, local.z, height, local.r);
        let measurement = CartesianMeasurement { z, r };

        // 1. Predict every track forward to *this plot's* time.
        for track in &mut self.tracks {
            let dt = t - track.last_time;
            if dt > 0.0 {
                track.imm.predict(dt, &process_noise);
                track.last_time = t;
            }
        }

        // The asynchronous lifecycle (steps 4–5) is governed purely by each
        // track's *own* revisit cadence (ADR 0013, Häppchen 13.2) — there is no
        // globally-estimated feed cadence on this path, so the
        // `sensor_period`/`sensor_last_scan` bookkeeping that `process_scan`
        // maintains is intentionally left untouched here.

        // 2./3. Associate the single measurement against the tracks' live
        //       estimates. `references` is captured once (post-prediction) so
        //       the initiation veto below sees the same estimates association
        //       did. With one measurement, JPDA degenerates to per-track PDA
        //       with cross-track exclusivity — `track_betas` is `[β_0, β_1]`,
        //       and a track is a hit when `β_0 < 1` (the plot fell in its gate).
        let references: Vec<LinearKalman> = self.tracks.iter().map(|tr| tr.estimate()).collect();
        let measurements = [measurement];
        let betas = joint_association_probabilities(&references, &measurements, &gate, &clutter);

        for (ti, track_betas) in betas.iter().enumerate() {
            if track_betas[0] >= 1.0 - NO_DETECTION_EPSILON {
                continue; // the plot is not in this track's gate
            }
            self.tracks[ti].imm.update_pda(&measurements, track_betas);
            self.tracks[ti].mark_hit(t);
            self.tracks[ti].record_hit_from(sensor);
            self.tracks[ti].update_identity(&plot.mode_ac);
        }

        // Initiate a new tentative track unless the plot fell in some existing
        // track's (wider) initiation gate — in which case it is that track's own
        // (possibly outlier) detection, not a new aircraft (ADR 0011).
        let suppressed = references
            .iter()
            .any(|f| init_gate.accepts_measurement(f, &measurement));
        if !suppressed {
            let filter = LinearKalman::from_first_measurement(&measurement, initial_velocity_std);
            let imm = imm_config.seed(filter);
            let id = TrackId(self.next_id);
            self.next_id += 1;
            let mut track = Track::new(id, imm, t);
            track.update_identity(&plot.mode_ac);
            track.record_hit_from(sensor);
            self.tracks.push(track);
        }

        // 4. Confirm tentative tracks that reached M-of-N hits within a time
        //    window scaled by the track's *own* expected revisit (Häppchen
        //    13.2), falling back to the nominal interval until it has one.
        for track in &mut self.tracks {
            if track.status() == TrackStatus::Tentative {
                let window = confirm_n as f64 * track.expected_revisit(NOMINAL_REVISIT_INTERVAL);
                if track.hits_within(window, t) >= confirm_m {
                    track.confirm();
                }
            }
        }

        // 5. Delete tracks that have coasted past their missed-revisit budget,
        //    counted in the track's own revisit intervals (time-continuous).
        self.tracks
            .retain(|track| !should_delete_continuous(track, delete_tentative, delete_confirmed));
    }
}

/// Whether a track has coasted past its missed-revisit budget, given its status.
///
/// The budget counts *missed revisit intervals*; the interval is the track's
/// adaptive [`coast_reference`](Track::coast_reference). A freshly hit track
/// (`update_age == 0`) is never deleted, even before any cadence is known.
fn should_delete(
    track: &Track,
    budget_tentative: u32,
    budget_confirmed: u32,
    cadence: f64,
) -> bool {
    let budget = match track.status() {
        TrackStatus::Tentative => budget_tentative,
        TrackStatus::Confirmed => budget_confirmed,
    } as f64;
    let age = track.update_age();
    age > 0.0 && age >= budget * track.coast_reference(cadence)
}

/// Whether a track should be deleted on the **asynchronous** path (ADR 0013,
/// Häppchen 13.2). Like [`should_delete`], but the missed-revisit budget counts
/// the track's **own** revisit intervals ([`Track::expected_revisit`]) rather
/// than a globally-estimated feed cadence: a track lives or dies purely by how
/// overdue *its* next update is. A track that has not yet measured its own
/// cadence falls back to [`NOMINAL_REVISIT_INTERVAL`]; a freshly hit track
/// (`update_age == 0`) is never deleted.
fn should_delete_continuous(track: &Track, budget_tentative: u32, budget_confirmed: u32) -> bool {
    let budget = match track.status() {
        TrackStatus::Tentative => budget_tentative,
        TrackStatus::Confirmed => budget_confirmed,
    } as f64;
    let age = track.update_age();
    age > 0.0 && age >= budget * track.expected_revisit(NOMINAL_REVISIT_INTERVAL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::{DetectionKind, ModeAC, Plot, SensorId};
    use firefly_geo::{Polar, Wgs84};

    /// The common test frame; the single test sensor sits here too.
    fn frame() -> LocalFrame {
        LocalFrame::new(Wgs84::from_degrees(47.0, 8.0, 500.0))
    }

    fn config() -> TrackerConfig {
        TrackerConfig::single_sensor(
            SensorId(1),
            frame(),
            SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08),
        )
    }

    /// A plot at a fixed polar position for a given time.
    fn plot(time: f64, range: f64, az: f64) -> Plot {
        Plot::primary(SensorId(1), Timestamp(time), Polar::new(range, az, 0.0))
    }

    /// A combined primary+secondary plot carrying an SSR identity reply.
    fn plot_with_identity(time: f64, range: f64, az: f64, mode_3a: u16, icao: u32) -> Plot {
        Plot {
            sensor: SensorId(1),
            time: Timestamp(time),
            measurement: Polar::new(range, az, 0.0),
            kind: DetectionKind::Combined,
            mode_ac: ModeAC {
                mode_3a: Some(mode_3a),
                flight_level_ft: Some(35_000.0),
                icao_address: Some(icao),
            },
        }
    }

    /// A new track is born tentative, then confirmed once M-of-N hits accrue.
    /// REQ: FR-TRK-001, FR-TRK-006
    #[test]
    fn track_is_born_tentative_then_confirmed() {
        let mut tracker = Tracker::new(config());
        let p = || plot(0.0, 50_000.0, 0.0);

        // Scan 0: birth.
        tracker.process_scan(Timestamp(0.0), &[p()]);
        assert_eq!(tracker.tracks().len(), 1);
        assert_eq!(tracker.tracks()[0].status(), TrackStatus::Tentative);

        // Scans 1 and 2: still the same track, now reaching 3 hits → confirmed.
        tracker.process_scan(Timestamp(4.0), &[plot(4.0, 50_000.0, 0.0)]);
        tracker.process_scan(Timestamp(8.0), &[plot(8.0, 50_000.0, 0.0)]);
        assert_eq!(tracker.tracks().len(), 1);
        assert_eq!(tracker.confirmed_count(), 1);
    }

    /// A confirmed track coasts through missed scans and is finally deleted.
    /// REQ: FR-TRK-006
    #[test]
    fn confirmed_track_coasts_then_dies() {
        let mut tracker = Tracker::new(config());
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_scan(Timestamp(t), &[plot(t, 50_000.0, 0.0)]);
        }
        assert_eq!(tracker.confirmed_count(), 1);

        // Now the target vanishes: feed empty scans. delete_misses_confirmed = 4.
        for k in 3..7 {
            let t = k as f64 * 4.0;
            tracker.process_scan(Timestamp(t), &[]);
        }
        assert_eq!(
            tracker.tracks().len(),
            0,
            "track should be deleted after 4 misses"
        );
    }

    /// A lone tentative track (e.g. a clutter plot) dies quickly when not seen again.
    /// REQ: FR-TRK-006
    #[test]
    fn tentative_track_dies_quickly() {
        let mut tracker = Tracker::new(config());
        tracker.process_scan(Timestamp(0.0), &[plot(0.0, 30_000.0, 1.0)]);
        assert_eq!(tracker.tracks().len(), 1);
        // Two empty scans (delete_misses_tentative = 2).
        tracker.process_scan(Timestamp(4.0), &[]);
        tracker.process_scan(Timestamp(8.0), &[]);
        assert_eq!(tracker.tracks().len(), 0);
    }

    /// Two well-separated plots create two distinct tracks.
    /// REQ: FR-TRK-001
    #[test]
    fn separated_plots_make_two_tracks() {
        let mut tracker = Tracker::new(config());
        tracker.process_scan(
            Timestamp(0.0),
            &[plot(0.0, 50_000.0, 0.0), plot(0.0, 50_000.0, 1.0)],
        );
        assert_eq!(tracker.tracks().len(), 2);
    }

    /// The geodetic output round-trips: a plot due east of the sensor becomes a
    /// `SystemTrack` whose WGS84 position projects back to the same local ENU
    /// offset (east ≈ range, north ≈ 0, up ≈ 0).
    /// REQ: NFR-INT-001, NFR-INT-002
    #[test]
    fn system_track_position_round_trips_through_wgs84() {
        let mut tracker = Tracker::new(config());
        // One exact plot at 40 km, azimuth 90° (due east): east = ρ, north = 0.
        tracker.process_scan(
            Timestamp(0.0),
            &[plot(0.0, 40_000.0, std::f64::consts::FRAC_PI_2)],
        );

        let sts = tracker.system_tracks();
        assert_eq!(sts.len(), 1);

        let back = frame().geodetic_to_enu(&sts[0].position);
        assert!((back.east - 40_000.0).abs() < 1.0, "east ≈ range");
        assert!(back.north.abs() < 1.0, "north ≈ 0");
        assert!(back.up.abs() < 1.0, "up ≈ 0 (2-D, on the tangent plane)");
    }

    /// The neutral port reports *both* statuses and tags each track: a long-lived
    /// track shows up confirmed, a freshly born one still tentative.
    /// REQ: NFR-INT-001
    #[test]
    fn system_tracks_carry_confirmation_status() {
        let mut tracker = Tracker::new(config());

        // Three scans confirm track A (due north, 50 km).
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_scan(Timestamp(t), &[plot(t, 50_000.0, 0.0)]);
        }
        // A fourth scan keeps A alive and spawns a still-tentative track B.
        tracker.process_scan(
            Timestamp(12.0),
            &[
                plot(12.0, 50_000.0, 0.0),
                plot(12.0, 30_000.0, std::f64::consts::FRAC_PI_2),
            ],
        );

        let mut sts = tracker.system_tracks();
        assert_eq!(sts.len(), 2);
        sts.sort_by_key(|s| s.id.0);
        assert!(sts[0].confirmed, "older track A is confirmed");
        assert!(!sts[1].confirmed, "fresh track B is still tentative");

        // A sits due north of the sensor: local north large, east ~0.
        let back = frame().geodetic_to_enu(&sts[0].position);
        assert!(back.north > 49_000.0, "A is far north of the sensor");
        assert!(back.east.abs() < 200.0, "A is ~due north (east ≈ 0)");
    }

    /// The safety-relevant status is reported and the tracker decides it: a
    /// just-hit track is fresh; a missed scan makes it coast (age grows,
    /// uncertainty grows); a fresh plot clears it again (age 0, uncertainty
    /// shrinks). REQ: FR-TRK-008
    #[test]
    fn system_tracks_report_coasting_age_and_uncertainty() {
        let mut tracker = Tracker::new(config());

        // Confirm a stationary track at 50 km due north.
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_scan(Timestamp(t), &[plot(t, 50_000.0, 0.0)]);
        }
        let fresh = tracker.system_tracks();
        assert_eq!(fresh.len(), 1);
        assert!(!fresh[0].coasting, "just-hit track is not coasting");
        assert!(fresh[0].update_age.abs() < 1e-9, "age 0 right after a hit");
        assert!(fresh[0].position_uncertainty > 0.0);
        let sigma_fresh = fresh[0].position_uncertainty;

        // One missed scan → the track coasts.
        tracker.process_scan(Timestamp(12.0), &[]);
        let coasted = tracker.system_tracks();
        assert!(coasted[0].coasting, "missed scan → coasting");
        assert!(
            (coasted[0].update_age - 4.0).abs() < 1e-9,
            "age = one scan interval"
        );
        assert!(
            coasted[0].position_uncertainty > sigma_fresh,
            "uncertainty grows while coasting"
        );

        // A fresh plot → no longer coasting, age resets, uncertainty shrinks.
        tracker.process_scan(Timestamp(16.0), &[plot(16.0, 50_000.0, 0.0)]);
        let rehit = tracker.system_tracks();
        assert!(!rehit[0].coasting);
        assert!(rehit[0].update_age.abs() < 1e-9);
        assert!(
            rehit[0].position_uncertainty < coasted[0].position_uncertainty,
            "update sharpens"
        );
    }

    /// A track that has only ever seen primary-only plots reports no
    /// identity. REQ: FR-TRK-009
    #[test]
    fn primary_only_track_has_no_identity() {
        let mut tracker = Tracker::new(config());
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_scan(Timestamp(t), &[plot(t, 50_000.0, 0.0)]);
        }

        let sts = tracker.system_tracks();
        assert_eq!(sts.len(), 1);
        assert_eq!(sts[0].mode_3a, None);
        assert_eq!(sts[0].icao_address, None);
    }

    /// An SSR identity reply is absorbed into the track and reported on the
    /// `SystemTrack`, surviving a subsequent primary-only (coasted) scan.
    /// REQ: FR-TRK-009
    #[test]
    fn ssr_identity_reaches_system_track_and_stays_sticky() {
        let mut tracker = Tracker::new(config());

        // Born and confirmed with an SSR-equipped plot.
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_scan(
                Timestamp(t),
                &[plot_with_identity(t, 50_000.0, 0.0, 0o2613, 0x0040_0123)],
            );
        }
        let sts = tracker.system_tracks();
        assert_eq!(sts[0].mode_3a, Some(0o2613));
        assert_eq!(sts[0].icao_address, Some(0x0040_0123));

        // A subsequent primary-only hit must not erase the known identity.
        tracker.process_scan(Timestamp(12.0), &[plot(12.0, 50_000.0, 0.0)]);
        let sts = tracker.system_tracks();
        assert_eq!(sts[0].mode_3a, Some(0o2613), "identity stays sticky");
        assert_eq!(
            sts[0].icao_address,
            Some(0x0040_0123),
            "identity stays sticky"
        );
    }

    /// A plot as one sensor would see a given geodetic target (its own polar).
    fn plot_seen_by(sensor: SensorId, sframe: &LocalFrame, time: f64, target: &Wgs84) -> Plot {
        let polar = sframe.geodetic_to_enu(target).to_polar();
        Plot::primary(sensor, Timestamp(time), polar)
    }

    /// Central fusion: two radars at different sites both see the *same*
    /// aircraft. After lifting each sensor's plot into the common tracking
    /// frame, the two measurements land on top of each other and associate to a
    /// **single** track — no ghost/duplicate — sitting at the true position.
    /// REQ: FR-TRK-010
    #[test]
    fn two_sensors_seeing_one_aircraft_make_one_track() {
        let tracking = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
        let frame_a = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0)); // = tracking
        let frame_b = LocalFrame::new(Wgs84::from_degrees(47.7, 11.6, 0.0)); // ~55 km SE
        let error = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08);
        let cfg = TrackerConfig::new(tracking)
            .with_sensor(SensorId(1), frame_a, error)
            .with_sensor(SensorId(2), frame_b, error);
        let mut tracker = Tracker::new(cfg);

        let target = Wgs84::from_degrees(48.1, 11.3, 0.0);
        for k in 0..4 {
            let t = k as f64 * 4.0;
            tracker.process_scan(
                Timestamp(t),
                &[
                    plot_seen_by(SensorId(1), &frame_a, t, &target),
                    plot_seen_by(SensorId(2), &frame_b, t, &target),
                ],
            );
        }

        assert_eq!(
            tracker.tracks().len(),
            1,
            "the two radars must fuse into one track, not two ghosts"
        );
        assert_eq!(tracker.confirmed_count(), 1);

        // The fused estimate sits at the true horizontal position (within the
        // tangent-plane/measurement tolerance over ~55 km).
        let expected = tracking.geodetic_to_enu(&target);
        let got = tracker.tracks()[0].position();
        assert!(
            (got[0] - expected.east).hypot(got[1] - expected.north) < 300.0,
            "fused position off: got [{}, {}], expected [{}, {}]",
            got[0],
            got[1],
            expected.east,
            expected.north
        );
    }

    /// `SystemTrack::contributing_sensors` reports exactly which sensor(s) hit
    /// a track in the most recent scan: both while the two radars both see the
    /// aircraft, and reduced to one when the second radar loses it, and empty
    /// while coasting with no detection at all.
    /// REQ: FR-TRK-010
    #[test]
    fn system_track_reports_contributing_sensors_per_scan() {
        let tracking = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
        let frame_a = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
        let frame_b = LocalFrame::new(Wgs84::from_degrees(47.7, 11.6, 0.0));
        let error = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08);
        let cfg = TrackerConfig::new(tracking)
            .with_sensor(SensorId(1), frame_a, error)
            .with_sensor(SensorId(2), frame_b, error);
        let mut tracker = Tracker::new(cfg);

        let target = Wgs84::from_degrees(48.1, 11.3, 0.0);

        // Scans 0..3: both sensors see the aircraft.
        for k in 0..4 {
            let t = k as f64 * 4.0;
            tracker.process_scan(
                Timestamp(t),
                &[
                    plot_seen_by(SensorId(1), &frame_a, t, &target),
                    plot_seen_by(SensorId(2), &frame_b, t, &target),
                ],
            );
        }
        let sts = tracker.system_tracks();
        assert_eq!(sts.len(), 1);
        assert_eq!(
            sts[0].contributing_sensors,
            vec![SensorId(1), SensorId(2)],
            "both radars contributed this scan"
        );

        // Scan 4: only sensor 1 still sees it.
        let t = 16.0;
        tracker.process_scan(
            Timestamp(t),
            &[plot_seen_by(SensorId(1), &frame_a, t, &target)],
        );
        let sts = tracker.system_tracks();
        assert_eq!(
            sts[0].contributing_sensors,
            vec![SensorId(1)],
            "only sensor 1 contributed this scan"
        );

        // Scan 5: neither sensor sees it (coasting) — no contributors.
        tracker.process_scan(Timestamp(20.0), &[]);
        let sts = tracker.system_tracks();
        assert!(
            sts[0].contributing_sensors.is_empty(),
            "coasting track has no contributing sensor this scan"
        );
    }

    /// End-to-end IMM payoff (Häppchen M5.4): a target flying a steady
    /// coordinated turn drives its track's **coordinated-turn** model
    /// probability above the constant-velocity one — the tracker "notices" the
    /// manoeuvre purely from the measurement likelihoods, with no separate
    /// manoeuvre detector. The target stays a single confirmed track throughout.
    /// REQ: FR-TRK-011, FR-TRK-012, FR-TRK-013
    #[test]
    fn imm_favours_the_turn_model_on_a_turning_target() {
        use crate::motion::MotionModel;
        use nalgebra::Vector4;

        let mut tracker = Tracker::new(config());
        let rate = 0.052_f64; // rad/s, a left (anticlockwise) rate-one turn
                              // Truth starts 40 km north of the sensor, heading east at 200 m/s.
        let x0 = Vector4::new(0.0, 40_000.0, 200.0, 0.0);
        let dt = 2.0;

        for k in 0..30 {
            let t = k as f64 * dt;
            // Truth at this scan from the coordinated-turn transition.
            let truth = MotionModel::CoordinatedTurn { rate }.transition(t) * x0;
            let polar = firefly_geo::Enu::new(truth[0], truth[1], 0.0).to_polar();
            tracker.process_scan(
                Timestamp(t),
                &[Plot::primary(SensorId(1), Timestamp(t), polar)],
            );
        }

        assert_eq!(tracker.confirmed_count(), 1, "one stable confirmed track");
        let track = &tracker.tracks()[0];
        let mu = track.imm.probabilities();
        let models = track.imm.models();

        // Find the probability of the left-turn model (rate ≈ +0.052) and of CV.
        let mut mu_cv = 0.0;
        let mut mu_left_turn = 0.0;
        for (m, &p) in models.iter().zip(mu) {
            match m {
                MotionModel::ConstantVelocity => mu_cv = p,
                MotionModel::CoordinatedTurn { rate: r } if *r > 0.0 => mu_left_turn = p,
                _ => {}
            }
        }
        assert!(
            mu_left_turn > mu_cv,
            "the matching turn model should dominate: μ = {mu:?}, models = {models:?}"
        );
    }

    /// JPDA payoff (Häppchen M5.5–M5.9): two targets flying parallel tracks
    /// close enough that their validation gates overlap each scan still end
    /// up as **two** distinct, confirmed tracks, each tracking its own
    /// target — the soft, joint association handles the shared plots without
    /// either track losing or swapping its identity, unlike a hard 1:1
    /// assignment that must arbitrarily pick one plot per track.
    /// REQ: FR-TRK-015, FR-TRK-016, FR-TRK-017, FR-TRK-018, FR-TRK-019
    #[test]
    fn jpda_keeps_two_close_parallel_tracks_distinct() {
        let mut tracker = Tracker::new(config());
        let speed = 150.0; // m/s due east
        let dt = 4.0;
        let separation = 100.0; // metres east, between the two targets

        // Target A starts 50 km north of the sensor; target B is 100 m east
        // of A. Both fly straight east at the same speed, so they stay 100 m
        // apart — close enough for their gates to overlap.
        for k in 0..10 {
            let t = k as f64 * dt;
            let east = speed * t;
            let polar_a = firefly_geo::Enu::new(east, 50_000.0, 0.0).to_polar();
            let polar_b = firefly_geo::Enu::new(east + separation, 50_000.0, 0.0).to_polar();
            tracker.process_scan(
                Timestamp(t),
                &[
                    Plot::primary(SensorId(1), Timestamp(t), polar_a),
                    Plot::primary(SensorId(1), Timestamp(t), polar_b),
                ],
            );
        }

        assert_eq!(tracker.tracks().len(), 2, "two distinct tracks");
        assert_eq!(tracker.confirmed_count(), 2, "both confirmed");

        // The two tracks remain distinguishable (have not coalesced into a
        // single position) even though their gates overlapped every scan.
        // Some convergence ("track coalescence") is a known characteristic of
        // JPDA for closely-spaced targets — the soft association *shares*
        // each plot between the tracks rather than handing it to one
        // exclusively — but the two estimates must not collapse onto each
        // other.
        let easts: Vec<f64> = tracker.tracks().iter().map(|tr| tr.position()[0]).collect();
        let diff = (easts[0] - easts[1]).abs();
        assert!(
            diff > 10.0,
            "the two tracks should remain distinguishable, got {diff} m apart: {easts:?}"
        );
        assert!(
            diff < separation,
            "some coalescence toward the shared plots is expected, got {diff} m: {easts:?}"
        );
    }

    /// A rare outlier plot of an already-tracked target — outside the tight
    /// association gate but still near the track — must not spawn a competing
    /// "ghost" track. The wider initiation-suppression gate (ADR 0011) absorbs
    /// it; with a tight initiation gate (== the association gate) the very same
    /// plot seeds a duplicate. This is the multi-radar ghost the Frankfurt
    /// showcase used to mask behind a widened association gate.
    /// REQ: FR-TRK-020
    #[test]
    fn outlier_plot_does_not_spawn_a_ghost() {
        // Confirm one track on a steady target due north at 50 km.
        let establish = |tracker: &mut Tracker| {
            for k in 0..5 {
                let t = k as f64 * 4.0;
                tracker.process_scan(Timestamp(t), &[plot(t, 50_000.0, 0.0)]);
            }
        };

        // An unassociated outlier ~250 m east of the track: a ~3.5σ cross-range
        // jump (≈ 3.8σ), outside the association gate but inside the wider
        // initiation gate.
        let az_offset = (400.0_f64 / 50_000.0).atan();
        let outlier = plot(20.0, 50_000.0, az_offset);

        // Control — initiation gate == association gate: the outlier spawns a
        // second (ghost) track.
        let mut tight = {
            let mut c = config();
            c.init_gate = c.gate;
            Tracker::new(c)
        };
        establish(&mut tight);
        let before = tight.tracks().len();
        tight.process_scan(Timestamp(20.0), std::slice::from_ref(&outlier));
        assert!(
            tight.tracks().len() > before,
            "control: a tight initiation gate lets the outlier seed a ghost"
        );

        // The shipped default — a wider initiation gate: no ghost.
        let mut wide = Tracker::new(config());
        assert!(
            wide_init_is_wider(&wide),
            "the default initiation gate must be wider than the association gate"
        );
        establish(&mut wide);
        let before = wide.tracks().len();
        wide.process_scan(Timestamp(20.0), std::slice::from_ref(&outlier));
        assert_eq!(
            wide.tracks().len(),
            before,
            "the wider initiation gate must suppress the outlier ghost"
        );
    }

    fn wide_init_is_wider(tracker: &Tracker) -> bool {
        tracker.config.init_gate.threshold > tracker.config.gate.threshold
    }

    /// A plot from a sensor the tracker does not know about cannot be geolocated
    /// and is dropped — it neither updates nor spawns a track.
    /// REQ: FR-TRK-010
    #[test]
    fn plot_from_unregistered_sensor_is_dropped() {
        let mut tracker = Tracker::new(config()); // only SensorId(1) is registered
        let stray = Plot::primary(SensorId(99), Timestamp(0.0), Polar::new(40_000.0, 0.5, 0.0));
        tracker.process_scan(Timestamp(0.0), &[stray]);
        assert_eq!(
            tracker.tracks().len(),
            0,
            "unregistered plot must be ignored"
        );
    }

    // --- ADR 0013, Häppchen 13.1: the asynchronous per-plot path ------------

    /// A single plot births a tentative track; feeding the same target a few
    /// more times confirms it — the per-plot path drives the M-of-N lifecycle
    /// just like the batch path. REQ: FR-TRK-001, FR-TRK-006, FR-TRK-022
    #[test]
    fn process_plot_births_then_confirms() {
        let mut tracker = Tracker::new(config());

        tracker.process_plot(&plot(0.0, 50_000.0, 0.0));
        assert_eq!(tracker.tracks().len(), 1);
        assert_eq!(tracker.tracks()[0].status(), TrackStatus::Tentative);

        tracker.process_plot(&plot(4.0, 50_000.0, 0.0));
        tracker.process_plot(&plot(8.0, 50_000.0, 0.0));
        assert_eq!(tracker.tracks().len(), 1, "still one track");
        assert_eq!(tracker.confirmed_count(), 1, "confirmed after 3 hits");
    }

    /// Two well-separated plots at the same instant seed two distinct tracks:
    /// the second plot is outside the first track's initiation gate, so it is a
    /// new aircraft, not that track's own detection. REQ: FR-TRK-001
    #[test]
    fn process_plot_separated_plots_make_two_tracks() {
        let mut tracker = Tracker::new(config());
        tracker.process_plot(&plot(0.0, 50_000.0, 0.0));
        tracker.process_plot(&plot(0.0, 50_000.0, 1.0));
        assert_eq!(tracker.tracks().len(), 2);
    }

    /// Each plot is processed at *its own* data time: a track updated by an
    /// off-grid plot reports that plot's time, proving the predict step runs to
    /// the plot time rather than a quantised scan time. REQ: FR-TRK-022
    #[test]
    fn process_plot_honours_each_plots_own_time() {
        let mut tracker = Tracker::new(config());
        tracker.process_plot(&plot(0.0, 50_000.0, 0.0));
        tracker.process_plot(&plot(2.5, 50_000.0, 0.0));

        let sts = tracker.system_tracks();
        assert_eq!(sts.len(), 1);
        assert!(
            (sts[0].time.as_secs() - 2.5).abs() < 1e-9,
            "track time should follow the plot's own time, got {}",
            sts[0].time.as_secs()
        );
    }

    /// A track that no later plot updates coasts: a subsequent (far-away) plot
    /// advances data time, the unseen track is predicted forward, and it reports
    /// `coasting` with an update age equal to the elapsed gap. REQ: FR-TRK-006,
    /// FR-TRK-008, FR-TRK-022
    #[test]
    fn process_plot_track_coasts_when_unseen() {
        let mut tracker = Tracker::new(config());
        // Confirm track A (id 1) due north at 50 km.
        for t in [0.0, 4.0, 8.0] {
            tracker.process_plot(&plot(t, 50_000.0, 0.0));
        }
        assert_eq!(tracker.confirmed_count(), 1);

        // A far-away plot (due east) at t = 12 advances time and seeds track B;
        // A is unseen this instant and must coast.
        tracker.process_plot(&plot(12.0, 30_000.0, std::f64::consts::FRAC_PI_2));

        let mut sts = tracker.system_tracks();
        sts.sort_by_key(|s| s.id.0);
        assert!(sts[0].coasting, "track A coasts when no plot updates it");
        assert!(
            (sts[0].update_age - 4.0).abs() < 1e-9,
            "age = gap since its last hit (12 - 8)"
        );
    }

    /// An SSR identity absorbed via the per-plot path reaches the `SystemTrack`
    /// and stays sticky across a later primary-only plot. REQ: FR-TRK-009
    #[test]
    fn process_plot_absorbs_and_keeps_identity() {
        let mut tracker = Tracker::new(config());
        for t in [0.0, 4.0, 8.0] {
            tracker.process_plot(&plot_with_identity(t, 50_000.0, 0.0, 0o2613, 0x0040_0123));
        }
        let sts = tracker.system_tracks();
        assert_eq!(sts[0].mode_3a, Some(0o2613));
        assert_eq!(sts[0].icao_address, Some(0x0040_0123));

        // A primary-only hit must not erase the known identity.
        tracker.process_plot(&plot(12.0, 50_000.0, 0.0));
        let sts = tracker.system_tracks();
        assert_eq!(sts[0].mode_3a, Some(0o2613), "identity stays sticky");
        assert_eq!(
            sts[0].icao_address,
            Some(0x0040_0123),
            "identity stays sticky"
        );
    }

    /// A lone tentative track (e.g. a clutter plot) that is never seen again is
    /// deleted once it is overdue by its budget of *nominal* revisits. The
    /// time-continuous lifecycle (Häppchen 13.2) scales deletion by the track's
    /// own expected revisit — here the nominal fallback, since it was never
    /// re-hit — not by the cadence of whichever other sensor drives time
    /// forward. REQ: FR-TRK-006, FR-TRK-022, FR-TRK-023
    #[test]
    fn process_plot_deletes_unseen_tentative() {
        let mut tracker = Tracker::new(config());
        // A lone tentative track (id 1) due north, born at t = 0.
        tracker.process_plot(&plot(0.0, 50_000.0, 0.0));

        // A different, far-away target (due east) drives time forward; the north
        // track is never seen again. delete_misses_tentative = 2 and the nominal
        // revisit is 5 s, so it survives until ~10 s overdue.
        let east = |t: f64| plot(t, 30_000.0, std::f64::consts::FRAC_PI_2);
        tracker.process_plot(&east(4.0));
        tracker.process_plot(&east(8.0)); // north age 8 s < 2 × 5 s: still alive
        assert!(
            tracker.tracks().iter().any(|tr| tr.id().0 == 1),
            "north track is still within its nominal-revisit budget at t = 8 s"
        );

        tracker.process_plot(&east(12.0)); // north age 12 s ≥ 2 × 5 s: deleted
        assert_eq!(tracker.tracks().len(), 1, "the unseen tentative is deleted");
        assert_eq!(
            tracker.tracks()[0].id().0,
            2,
            "the surviving track is the east one (id 2), not the deleted north one"
        );
    }

    /// A plot from an unregistered sensor cannot be geolocated and is dropped by
    /// the per-plot path too. REQ: FR-TRK-010
    #[test]
    fn process_plot_from_unregistered_sensor_is_dropped() {
        let mut tracker = Tracker::new(config()); // only SensorId(1) is registered
        let stray = Plot::primary(SensorId(99), Timestamp(0.0), Polar::new(40_000.0, 0.5, 0.0));
        tracker.process_plot(&stray);
        assert_eq!(
            tracker.tracks().len(),
            0,
            "unregistered plot must be ignored"
        );
    }

    /// Two co-located sensors report the *same* aircraft at interleaved times.
    /// Because the plots are separated in time, each is gated against the track
    /// predicted to that instant, so the second sensor's plot folds into the
    /// same track instead of spawning a ghost — the asynchronous path needs no
    /// frozen scan-start reference (ADR 0011). The interleaved hits keep one
    /// confirmed identity, exercising the time-continuous lifecycle across two
    /// sensors. REQ: FR-TRK-010, FR-TRK-022, FR-TRK-023
    #[test]
    fn process_plot_two_async_sensors_make_one_track() {
        let error = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08);
        let cfg = TrackerConfig::new(frame())
            .with_sensor(SensorId(1), frame(), error)
            .with_sensor(SensorId(2), frame(), error);
        let mut tracker = Tracker::new(cfg);

        // One stationary target 50 km due north; the two sensors alternate every
        // 2 s (each revisiting every 4 s, offset by 2 s).
        for k in 0..6 {
            let t = k as f64 * 2.0;
            let sensor = if k % 2 == 0 { SensorId(1) } else { SensorId(2) };
            tracker.process_plot(&Plot::primary(
                sensor,
                Timestamp(t),
                Polar::new(50_000.0, 0.0, 0.0),
            ));
        }

        assert_eq!(
            tracker.tracks().len(),
            1,
            "two async sensors seeing one aircraft must fuse into one track"
        );
        assert_eq!(tracker.confirmed_count(), 1, "and it confirms");
    }
}
