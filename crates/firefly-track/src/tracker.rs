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

use firefly_core::{
    ModeOfMovement, Plot, SensorId, SourceAges, SystemTrack, Timestamp, TrackId, PROVENANCE_FRESH_S,
};
use firefly_geo::{Enu, LocalFrame};
use serde::{Deserialize, Serialize};

use crate::gating::Gate;
use crate::imm::ImmConfig;
use crate::jpda::joint_association_probabilities;
use crate::kalman::{LinearKalman, ProcessNoise};
use crate::measurement::{tracking_measurement, CartesianMeasurement, SensorErrorModel};
use crate::pda::ClutterModel;
use crate::track::{Track, TrackStatus};
use crate::track_number::TrackNumberPool;

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

/// Plots whose data times fall within this window (seconds) are treated as one
/// **simultaneous measurement opportunity** on the asynchronous path (ADR 0013,
/// Häppchen 13.5): they are associated *jointly* against one reference frozen at
/// the window's time — ADR 0011 ghost suppression plus JPDA exclusivity
/// (FR-TRK-018/019/020) — instead of one plot at a time. It is short enough that
/// the kinematic spread within the window is negligible (≤0.5 s ⇒ ≲100 m at jet
/// speed, within measurement noise), yet long enough to bind genuinely
/// coincident plots: two crossing targets a single radar reports at nearly the
/// same azimuth (so nearly the same time), or two radars that happen to paint a
/// target at the same instant. Plots further apart in time are separate
/// opportunities, each gated against the track predicted forward to it — there
/// the time gap itself regrows the covariance, so no frozen reference is needed.
///
/// This generalises the batch path's exact-equal-time scan (`process_scan`,
/// where the window is effectively zero) to the realistic case of
/// azimuth-spread plot times (Häppchen 13.6).
const SIMULTANEITY_WINDOW: f64 = 0.5;

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
    /// This sensor's antenna revolution period (scan period), seconds.
    ///
    /// A real radar's revolution period is a known operational parameter, not
    /// something to be discovered at runtime (ARTAS and comparable multi-sensor
    /// trackers carry it in their sensor declaration data). The asynchronous
    /// lifecycle ([`should_delete_continuous`]) uses the slowest configured
    /// period across all registered sensors as its **cadence floor** (ADR 0013,
    /// Häppchen 13.5d) — replacing the online estimate from 13.5c, which a
    /// rotating radar's azimuth-spread plot times (Häppchen 13.6) made too
    /// short (it captured sub-revolution plot gaps, not the revolution itself).
    pub scan_period: f64,
    /// Inner (minimum) detection range, metres.  Zero means no inner dead zone.
    ///
    /// Purely informational — the tracker does not use it for gating.  It is
    /// carried here so that operators and display systems (e.g. Wayfinder's
    /// coverage-ring overlay) have access to the full sensor geometry without
    /// requiring a second configuration source.
    #[serde(default)]
    pub min_range_m: f64,
    /// Outer (maximum) detection range, metres.  Zero means unspecified.
    ///
    /// Same informational role as [`min_range_m`]: the tracker ignores it for
    /// gating; it exists so the declared sensor geometry is self-documenting.
    #[serde(default)]
    pub max_range_m: f64,
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

    /// Register one sensor (its frame, assumed noise, and scan period).
    /// Chainable.
    pub fn with_sensor(
        mut self,
        id: SensorId,
        frame: LocalFrame,
        error: SensorErrorModel,
        scan_period: f64,
    ) -> Self {
        self.sensors.insert(
            id,
            SensorModel {
                frame,
                error,
                scan_period,
                min_range_m: 0.0,
                max_range_m: 0.0,
            },
        );
        self
    }

    /// Set the inner/outer detection range on an already-registered sensor.
    ///
    /// These values are informational (the tracker does not use them for
    /// gating); they let display systems read the full sensor geometry from one
    /// place.  Silently no-ops if `id` is not registered.  Chainable.
    pub fn with_sensor_coverage(
        mut self,
        id: SensorId,
        min_range_m: f64,
        max_range_m: f64,
    ) -> Self {
        if let Some(sensor) = self.sensors.get_mut(&id) {
            sensor.min_range_m = min_range_m;
            sensor.max_range_m = max_range_m;
        }
        self
    }

    /// Single-sensor convenience: the tracking frame *is* the sensor's own frame
    /// (so the lift into the common frame is the identity). This reproduces the
    /// pre-M4 single-radar behaviour.
    ///
    /// Range fields default to 0.0 (unspecified); call
    /// [`TrackerConfig::with_sensor_coverage`] afterwards to set them.
    pub fn single_sensor(
        id: SensorId,
        frame: LocalFrame,
        error: SensorErrorModel,
        scan_period: f64,
    ) -> Self {
        Self::new(frame).with_sensor(id, frame, error, scan_period)
    }
}

/// A single-radar multi-target tracker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tracker {
    config: TrackerConfig,
    tracks: Vec<Track>,
    next_id: u32,
    /// Pool for the 16-bit **wire** track numbers (CAT062 I062/040,
    /// FR-TRK-035): allocated at track birth, quarantined on deletion, reused
    /// only after the quarantine — so the wire number space never silently
    /// collides the way a truncated `next_id` would after 65 536 births.
    track_numbers: TrackNumberPool,
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
    /// Final reports for tracks deleted since the last drain (ADR 0016): each is
    /// the deleted track's last known state with `ended = true`, to be emitted
    /// once as a CAT062 I062/080 TSE record. Filled at the deletion sites,
    /// drained by the output stage via [`take_ended_tracks`](Self::take_ended_tracks).
    ended_tracks: Vec<SystemTrack>,
    /// High-water mark of processed data time (seconds). Any scan or plot group
    /// at or before this instant is stale and must be dropped rather than fused
    /// without a proper forward prediction — backward Kalman prediction is
    /// undefined and would corrupt the track state (Robustheit Paket 5).
    /// `None` before the first input. Updated by both [`process_scan`] and
    /// [`process_plots`].
    data_time_watermark: Option<f64>,
}

impl Tracker {
    pub fn new(config: TrackerConfig) -> Self {
        Self {
            config,
            tracks: Vec::new(),
            next_id: 1,
            track_numbers: TrackNumberPool::new(),
            prev_scan_time: None,
            sensor_last_scan: BTreeMap::new(),
            sensor_period: BTreeMap::new(),
            ended_tracks: Vec::new(),
            data_time_watermark: None,
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
                system_track_from(
                    track,
                    &track.estimate(),
                    track.last_time,
                    track.is_coasting(),
                    track.update_age(),
                    frame,
                )
            })
            .collect()
    }

    /// Project **all** tracks onto a single common output time `t`, **without
    /// mutating the tracker** — the read-only counterpart to
    /// [`system_tracks`](Self::system_tracks).
    ///
    /// For the periodic output stage (ADR 0013, Häppchen 13.3 → 13.4) the air
    /// picture must carry one consistent time stamp regardless of when each
    /// track was last updated. Each track's IMM bank is therefore predicted **on
    /// a clone** to `t` (IMM mixing + dead reckoning), so the tracker's own
    /// state is untouched: `snapshot_at` never advances a track, books a hit, or
    /// deletes anything. This is what later lets the output run on a fixed
    /// heartbeat decoupled from the asynchronous input.
    ///
    /// A track already at or after `t` is reported as-is (no backward
    /// prediction). Status is evaluated at `t`: `update_age = t − last_hit_time`,
    /// and the track is `coasting` once it is overdue by a full expected revisit
    /// interval — i.e. the output time has reached the moment its next hit was
    /// due. A track that is still being updated on cadence is therefore *not*
    /// reported coasting, even though the fixed output heartbeat (ADR 0013) lands
    /// a fraction of a second after its last hit.
    ///
    /// REQ: NFR-INT-001, NFR-INT-002, FR-TRK-024
    pub fn snapshot_at(&self, t: Timestamp) -> Vec<SystemTrack> {
        let frame = &self.config.tracking_frame;
        let process_noise = self.config.process_noise;
        let t = t.as_secs();
        self.tracks
            .iter()
            .map(|track| {
                // Predict to `t` on a clone so the live track is not advanced.
                let dt = t - track.last_time;
                let estimate = if dt > 0.0 {
                    let mut imm = track.imm.clone();
                    imm.predict(dt, &process_noise);
                    imm.combined_estimate()
                } else {
                    track.estimate()
                };
                let update_age = (t - track.last_hit_time).max(0.0);
                // A track coasts once the output time has reached the moment its
                // next hit was due, i.e. it has missed at least one expected
                // revisit. Comparing against the track's expected revisit
                // interval (not against a bare `t > last_hit_time`) is essential:
                // the periodic output runs on a fixed heartbeat decoupled from
                // the asynchronous input (ADR 0013), so the output time is almost
                // always a sliver past the last hit even for a perfectly healthy,
                // freshly-updated track. Flagging every such track as coasting
                // would make the whole air picture coast permanently.
                let coasting = update_age >= track.expected_revisit(NOMINAL_REVISIT_INTERVAL);
                system_track_from(track, &estimate, t, coasting, update_age, frame)
            })
            .collect()
    }

    /// Take and clear the buffered **final reports** for tracks deleted since
    /// the last call (ADR 0016, FR-TRK-029).
    ///
    /// Each returned [`SystemTrack`] carries the deleted track's last known
    /// state with `ended = true` (→ the CAT062 I062/080 TSE bit). The output
    /// stage calls this once per heartbeat and appends the results to the air
    /// picture, so a deleted track is reported **exactly once** more — letting a
    /// consumer remove it deterministically — and then never again. Draining is
    /// idempotent: a second call before the next deletion returns empty.
    pub fn take_ended_tracks(&mut self) -> Vec<SystemTrack> {
        std::mem::take(&mut self.ended_tracks)
    }

    /// Buffer the final state of every track matching `should_delete`, then drop
    /// those tracks from the live set (ADR 0016). The captured [`SystemTrack`]s
    /// carry `ended = true` and are appended in live-set order, so the buffer is
    /// deterministic (NFR-CLOUD-001). Shared by both deletion sites (batch
    /// `process_scan` and the time-continuous `process_plots`).
    ///
    /// `now` is the deletion's data time: each deleted track's wire number is
    /// released into the pool's quarantine at this instant (FR-TRK-035), so it
    /// cannot be reborn on a new track before every consumer has processed the
    /// TSE report.
    fn delete_and_buffer_ended(&mut self, now: f64, should_delete: impl Fn(&Track) -> bool) {
        let frame = &self.config.tracking_frame;
        let mut newly_ended: Vec<SystemTrack> = self
            .tracks
            .iter()
            .filter(|track| should_delete(track))
            .map(|track| {
                let mut final_track = system_track_from(
                    track,
                    &track.estimate(),
                    track.last_time,
                    track.is_coasting(),
                    track.update_age(),
                    frame,
                );
                final_track.ended = true;
                final_track
            })
            .collect();
        for ended in &newly_ended {
            self.track_numbers.release(ended.track_number, now);
        }
        self.tracks.retain(|track| !should_delete(track));
        self.ended_tracks.append(&mut newly_ended);
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
        let confirm_m = self.config.confirm_m;
        let confirm_n = self.config.confirm_n;
        let delete_tentative = self.config.delete_misses_tentative;
        let delete_confirmed = self.config.delete_misses_confirmed;
        let tracking_frame = self.config.tracking_frame;
        let t = time.as_secs();

        // Guard: a scan that does not advance data time cannot be correctly
        // integrated — backward Kalman prediction is undefined, and equal-time
        // scans are idempotent noise. Drop with a warning (Robustheit Paket 5).
        if let Some(watermark) = self.data_time_watermark {
            if t <= watermark {
                tracing::warn!(
                    scan_time = t,
                    watermark,
                    "late scan dropped: data time did not advance (out-of-order input)"
                );
                return;
            }
        }
        self.data_time_watermark = Some(t);

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
                // Dispatch on the plot's measurement source (radar polar vs.
                // ADS-B geodetic) to produce a Cartesian measurement already in
                // the common tracking frame (Häppchen AP9.2).
                let cm = tracking_measurement(p, &model.frame, &model.error, &tracking_frame);
                by_sensor.entry(p.sensor).or_default().push((p, cm));
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
        // Fuse this scan's plots into the tracks against one reference frozen
        // here, with ADR 0011 ghost suppression and JPDA exclusivity. Every
        // track was predicted to `t` and had its provenance reset above, so the
        // shared core can be used verbatim by the asynchronous path too (see
        // [`fuse_simultaneous_plots`]).
        self.fuse_simultaneous_plots(t, &by_sensor);

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
        self.delete_and_buffer_ended(t, |track| {
            should_delete(track, delete_tentative, delete_confirmed, cadence)
        });
    }

    /// Process a **single plot at its own data time** — the asynchronous,
    /// per-plot counterpart to [`process_scan`](Self::process_scan) (ADR 0013,
    /// Häppchen 13.1). A thin convenience over
    /// [`process_plots`](Self::process_plots) for a one-element batch.
    ///
    /// REQ: FR-TRK-001, FR-TRK-006, FR-TRK-010, FR-TRK-022, FR-TRK-023
    pub fn process_plot(&mut self, plot: &Plot) {
        self.process_plots(std::slice::from_ref(plot));
    }

    /// Process a batch of asynchronous plots, each carrying its **own** data
    /// time (ADR 0013, Häppchen 13.5) — the asynchronous input port that the
    /// periodic output stage drives.
    ///
    /// Unlike [`process_scan`](Self::process_scan), the plots do **not** share a
    /// time: a real rotating radar spreads its detections across the antenna
    /// revolution by azimuth (Häppchen 13.6), and several radars are
    /// unsynchronised. Processing each plot strictly one at a time, however,
    /// loses the joint reasoning that keeps the air picture clean when plots
    /// *are* near-coincident — two crossing targets one radar reports at almost
    /// the same azimuth, or two radars painting a target at the same instant.
    ///
    /// So the batch is split into **simultaneous measurement opportunities**:
    /// plots whose times fall within [`SIMULTANEITY_WINDOW`] of the group's
    /// start are fused *jointly* against one reference frozen at the group's
    /// time (ADR 0011 ghost suppression + JPDA exclusivity, FR-TRK-018/019/020),
    /// exactly as a scan is — this is the shared [`fuse_simultaneous_plots`].
    /// Groups further apart in time are independent opportunities; the elapsed
    /// gap regrows each track's covariance by prediction, so a later sensor's
    /// plot of the same aircraft still folds into the existing track rather than
    /// spawning a ghost, with no frozen reference needed across the gap.
    ///
    /// For each opportunity the steps mirror the batch path: **predict** every
    /// track to the group's time, **fuse** the plots, then run the
    /// **time-continuous** confirm/delete lifecycle (Häppchen 13.2) — governed
    /// by each track's own measured revisit interval, floored by the slowest
    /// **configured** sensor scan period (Häppchen 13.5d, see
    /// [`should_delete_continuous`]).
    ///
    /// Determinism (ADR 0003): the plots are processed in a total order (data
    /// time, then sensor id) independent of their input order, so the same set
    /// of plots always yields the same state — no wall clock, no I/O.
    ///
    /// REQ: FR-TRK-001, FR-TRK-006, FR-TRK-010, FR-TRK-022, FR-TRK-023,
    /// FR-TRK-025
    pub fn process_plots(&mut self, plots: &[Plot]) {
        if plots.is_empty() {
            return;
        }
        let process_noise = self.config.process_noise;
        let confirm_m = self.config.confirm_m;
        let confirm_n = self.config.confirm_n;
        let delete_tentative = self.config.delete_misses_tentative;
        let delete_confirmed = self.config.delete_misses_confirmed;
        let tracking_frame = self.config.tracking_frame;

        // Deletion **cadence floor** (ADR 0013, Häppchen 13.5d): the slowest
        // configured sensor scan period across the whole feed. A track is never
        // deleted faster than the slowest radar would revisit it, no matter how
        // briefly a faster radar's azimuth-spread plots make its own revisit
        // estimate look. Configured rather than estimated (13.5c estimated it
        // from inter-plot gaps, which 13.6's azimuth spreading made too short).
        let cadence = self
            .config
            .sensors
            .values()
            .map(|s| s.scan_period)
            .fold(0.0, f64::max);

        // Total, input-order-independent processing order: by data time, ties
        // broken by sensor id (determinism, ADR 0003).
        let mut order: Vec<usize> = (0..plots.len()).collect();
        order.sort_by(|&a, &b| {
            plots[a]
                .time
                .as_secs()
                .partial_cmp(&plots[b].time.as_secs())
                .unwrap()
                .then(plots[a].sensor.0.cmp(&plots[b].sensor.0))
        });

        let mut gi = 0;
        while gi < order.len() {
            // Grow a simultaneity group: consecutive plots within the window of
            // the group's first (earliest) plot.
            let group_start = plots[order[gi]].time.as_secs();
            let mut gj = gi + 1;
            while gj < order.len()
                && plots[order[gj]].time.as_secs() - group_start <= SIMULTANEITY_WINDOW + 1e-9
            {
                gj += 1;
            }
            // Represent the opportunity at its latest instant — predicting a
            // track forward (never backward) to the freshest plot in the group.
            let t = plots[order[gj - 1]].time.as_secs();

            // Guard: a group strictly in the past cannot be correctly integrated
            // — the tracks have already been predicted forward past this instant,
            // and backward Kalman prediction is undefined. Drop the group with a
            // warning (Robustheit Paket 5). Equal-time groups are allowed: they
            // have dt = 0 for all tracks, prediction is a no-op, and the
            // measurement is still folded in (simultaneous multi-sensor hit).
            if let Some(watermark) = self.data_time_watermark {
                if t < watermark {
                    tracing::warn!(
                        plots = gj - gi,
                        group_time = t,
                        watermark,
                        "late plots dropped: data time went backward (out-of-order input)"
                    );
                    gi = gj;
                    continue;
                }
            }
            self.data_time_watermark = Some(t);

            // Convert each plot to a Cartesian measurement in the common frame
            // and group by sensor (deterministic order); drop plots from an
            // unregistered sensor, which cannot be geolocated.
            let mut by_sensor: BTreeMap<SensorId, Vec<(&Plot, CartesianMeasurement)>> =
                BTreeMap::new();
            for &idx in &order[gi..gj] {
                let p = &plots[idx];
                if let Some(model) = self.config.sensors.get(&p.sensor) {
                    let cm = tracking_measurement(p, &model.frame, &model.error, &tracking_frame);
                    by_sensor.entry(p.sensor).or_default().push((p, cm));
                }
            }

            // 1. Predict every track to this opportunity's time; refresh the
            //    per-opportunity sensor provenance.
            for track in &mut self.tracks {
                let dt = t - track.last_time;
                if dt > 0.0 {
                    track.imm.predict(dt, &process_noise);
                    track.last_time = t;
                }
                track.reset_contributing_sensors();
            }

            // 2./3. Fuse with ADR 0011 ghost suppression + JPDA exclusivity.
            self.fuse_simultaneous_plots(t, &by_sensor);

            // 4. Confirm via the time-continuous M-of-N window.
            for track in &mut self.tracks {
                if track.status() == TrackStatus::Tentative {
                    let window =
                        confirm_n as f64 * track.expected_revisit(NOMINAL_REVISIT_INTERVAL);
                    if track.hits_within(window, t) >= confirm_m {
                        track.confirm();
                    }
                }
            }

            // 5. Delete via the time-continuous missed-revisit budget, floored
            //    by the slowest observed sensor cadence (Häppchen 13.5c).
            self.delete_and_buffer_ended(t, |track| {
                should_delete_continuous(track, delete_tentative, delete_confirmed, cadence)
            });

            gi = gj;
        }
    }

    /// Fold a batch of plots taken at (near-)one instant `time` into the current
    /// tracks, with ADR 0011 ghost suppression and JPDA exclusivity
    /// (FR-TRK-018/019/020). The **shared association core** of the batch path
    /// ([`process_scan`](Self::process_scan)) and the asynchronous path
    /// ([`process_plots`](Self::process_plots)), so the safety-critical fusion
    /// logic exists exactly once.
    ///
    /// Pre-conditions the caller must establish: every track is already
    /// predicted to `time` and has had its contributing-sensor set reset.
    ///
    /// **ICAO pre-sort (FR-TRK-031):** before the kinematic JPDA step, any plot
    /// whose `mode_ac.icao_address` matches a live track's known address is
    /// directly associated with that track — no gate check. The Mode S ICAO-24
    /// address is a globally unique aircraft identifier, so an address match is
    /// authoritative; ADS-B position self-reports are typically far more precise
    /// than a kinematic gate would demand. Matched plots are excluded from the
    /// JPDA pool. Tracks updated by ICAO pre-sort still appear in the frozen
    /// `reference` with their **pre-update** estimates, so ghost-suppression for
    /// the remaining JPDA plots is unaffected (ADR 0011).
    ///
    /// **Security note (ADR 0019):** ICAO addresses are not cryptographically
    /// authenticated here. Network-level isolation (ADR 0017) is the primary
    /// defence; spoofing-resistant cross-checks are a planned future refinement.
    ///
    /// Sensors are fused in deterministic id order against one reference
    /// **frozen here**: folding a plot tightens a track's covariance, so gating
    /// the next sensor against the tightened estimate would push a second
    /// radar's plot of the same aircraft out of gate and let initiation spawn a
    /// ghost; freezing the reference removes that sequential tightening, while
    /// state fusion stays sequential (the same joint posterior for independent
    /// measurements). New tentative tracks are appended only after each sensor's
    /// block, so they veto the *next* sensor's plots (one aircraft, two radars →
    /// one track) but not their own siblings from the same sensor. A track is
    /// recorded as hit when at least one plot fell in its gate (`β_0 < 1`); the
    /// gated plot with the largest `β` carries the identity bookkeeping. The
    /// caller owns the lifecycle (confirm/delete) and any cadence bookkeeping.
    fn fuse_simultaneous_plots(
        &mut self,
        time: f64,
        by_sensor: &BTreeMap<SensorId, Vec<(&Plot, CartesianMeasurement)>>,
    ) {
        let gate = self.config.gate;
        let init_gate = self.config.init_gate;
        let initial_velocity_std = self.config.initial_velocity_std;
        let imm_config = self.config.imm.clone();
        let clutter = self.config.clutter;
        let t = time;

        // `reference[k]` is the gating/association estimate for `self.tracks[k]`:
        // the prediction frozen at `time` for tracks that existed then, or the
        // fresh estimate of a track initiated *during* this opportunity —
        // appended so a later sensor associates to it instead of starting its
        // own. It stays index-aligned with `self.tracks`, both growing together.
        let mut reference: Vec<LinearKalman> = self.tracks.iter().map(|tr| tr.estimate()).collect();

        // ICAO pre-sort (FR-TRK-031): build a per-sensor, per-plot boolean mask
        // that marks plots already directly associated via ICAO address. These
        // are excluded from the JPDA pool below. We do this AFTER building
        // `reference` so that the frozen reference (used for ghost suppression)
        // reflects the pre-update state of all tracks — including those about to
        // receive an ICAO-matched update.
        //
        // A plot is matched if *both* conditions hold:
        //   (a) its mode_ac.icao_address is Some(icao), and
        //   (b) exactly one live track carries that icao address.
        // If two tracks somehow share the same ICAO (should not happen for valid
        // data, but is defensive), the plot falls through to JPDA rather than
        // being silently mis-associated.
        let mut icao_handled: BTreeMap<SensorId, Vec<bool>> = by_sensor
            .keys()
            .map(|&s| (s, vec![false; by_sensor[&s].len()]))
            .collect();

        for (&sensor, items) in by_sensor {
            let handled = icao_handled.get_mut(&sensor).unwrap();
            for (mi, (plot, cm)) in items.iter().enumerate() {
                let Some(icao) = plot.mode_ac.icao_address else {
                    continue;
                };
                // Find the (unique) track with this ICAO address.
                let matching: Vec<usize> = self
                    .tracks
                    .iter()
                    .enumerate()
                    .filter(|(_, tr)| tr.icao_address() == Some(icao))
                    .map(|(ti, _)| ti)
                    .collect();
                let [ti] = matching[..] else {
                    // Zero matches (unknown aircraft) or >1 (ambiguous) — skip
                    // and let JPDA sort it out kinematically.
                    continue;
                };
                // Certain association: β_0 = 0 (no-detection), β_1 = 1.0.
                self.tracks[ti].imm.update_pda(&[*cm], &[0.0, 1.0]);
                self.tracks[ti].mark_hit(t);
                self.tracks[ti].record_hit_from(sensor, t);
                self.tracks[ti].update_identity(&plot.mode_ac, t);
                self.tracks[ti].record_source_hit(plot.source, t, plot.kind.has_primary());
                handled[mi] = true;
            }
        }

        for (&sensor, items) in by_sensor {
            let handled = &icao_handled[&sensor];

            // Build the JPDA measurement list from the non-ICAO-handled plots,
            // keeping their original index in `items` for identity look-up.
            let non_icao: Vec<(usize, CartesianMeasurement)> = items
                .iter()
                .enumerate()
                .filter(|(mi, _)| !handled[*mi])
                .map(|(mi, (_, cm))| (mi, *cm))
                .collect();
            let measurements: Vec<CartesianMeasurement> =
                non_icao.iter().map(|(_, m)| *m).collect();

            if measurements.is_empty() {
                continue;
            }

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
                self.tracks[ti].record_hit_from(sensor, t);
                if let Some((mi, _)) = best {
                    // Translate JPDA measurement index → original items index.
                    let orig_mi = non_icao[mi].0;
                    let plot = &items[orig_mi].0;
                    self.tracks[ti].update_identity(&plot.mode_ac, t);
                    // Per-technology provenance bookkeeping (ADR 0027): book the
                    // associated plot's technology (and PSR for a combined dwell).
                    self.tracks[ti].record_source_hit(plot.source, t, plot.kind.has_primary());
                }
            }

            // Initiate a new tentative track from each plot that fell in *no*
            // reference track's gate. The veto uses `reference` as frozen at the
            // *start* of this sensor's block, so two close-but-distinct plots
            // from the *same* sensor do not veto one another — they are
            // different targets and must each seed a track. New tracks are
            // appended only *after* the loop, so they veto the *next* sensor's
            // plots, not their own siblings.
            let mut newborn = Vec::new();
            for &(orig_mi, ref m) in &non_icao {
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
                // Allocate the wire track number first (FR-TRK-035): with the
                // whole 16-bit space in use or quarantined there is no honest
                // number to report this track under, so initiation is declined
                // rather than emitting a colliding I062/040.
                let Some(number) = self.track_numbers.allocate(t) else {
                    tracing::warn!(
                        time = t,
                        "track number pool exhausted: track initiation declined"
                    );
                    continue;
                };
                let filter = LinearKalman::from_first_measurement(m, initial_velocity_std);
                let imm = imm_config.seed(filter);
                let id = TrackId(self.next_id);
                self.next_id += 1;
                let mut track = Track::new(id, number, imm, t);
                let founding = &items[orig_mi].0;
                track.update_identity(&founding.mode_ac, t);
                track.record_hit_from(sensor, t);
                // Per-technology provenance bookkeeping (ADR 0027): the founding
                // plot's technology (and PSR if it is a combined dwell).
                track.record_source_hit(founding.source, t, founding.kind.has_primary());
                newborn.push(track.estimate());
                self.tracks.push(track);
            }
            reference.extend(newborn);
        }
    }
}

/// Assemble a neutral [`SystemTrack`] from a track and a (possibly predicted)
/// estimate, lifting the position back to WGS84 through `frame`. Shared by
/// [`Tracker::system_tracks`] (current estimate, per-track last time) and
/// [`Tracker::snapshot_at`] (estimate predicted to a common output time), so the
/// two output ports cannot drift apart. The 2-D estimate sits on the tracking
/// plane (up = 0), i.e. at the tracking-frame origin's ellipsoidal height (the
/// tracker carries no independent vertical estimate yet).
fn system_track_from(
    track: &Track,
    estimate: &LinearKalman,
    time: f64,
    coasting: bool,
    update_age: f64,
    frame: &LocalFrame,
) -> SystemTrack {
    let p = estimate.position();
    let v = estimate.velocity();
    let position = frame.enu_to_geodetic(&Enu::new(p[0], p[1], 0.0));
    // Per-technology ages at the report time (ADR 0027): age = report time minus
    // that technology's last-hit time, clamped at 0; `None` if it never contributed.
    let age = |hit: Option<f64>| hit.map(|h| (time - h).max(0.0));
    let sh = track.source_hits();
    let source_ages = SourceAges {
        psr: age(sh.psr),
        ssr: age(sh.ssr),
        mode_s: age(sh.mode_s),
        adsb: age(sh.adsb),
        flarm: age(sh.flarm),
    };
    SystemTrack {
        id: track.id(),
        track_number: track.track_number(),
        time: Timestamp(time),
        position,
        v_east: v[0],
        v_north: v[1],
        confirmed: track.is_confirmed(),
        coasting,
        // Mono/multisensor over the provenance freshness window (FR-TRK-036):
        // the per-scan contributing set would flap on an asynchronous
        // multi-radar feed whose sensors rarely land in one opportunity.
        monosensor: track.fresh_sensor_count(time, PROVENANCE_FRESH_S) <= 1,
        spi: track.spi(),
        // DAPs only while fresh (FR-TRK-040): a stale selected altitude shown
        // as current would mislead — withheld, and the wire then omits the
        // I062/380 subfields.
        daps: track.fresh_daps(time, PROVENANCE_FRESH_S),
        ended: false,
        update_age,
        position_uncertainty: estimate.position_uncertainty(),
        mode_3a: track.mode_3a(),
        icao_address: track.icao_address(),
        flight_level_ft: track.flight_level_ft(),
        callsign: track.callsign(),
        contributing_sensors: track.contributing_sensors().iter().copied().collect(),
        adsb_age_s: source_ages.adsb,
        source_ages,
        // Vertical chain (VERT.2): filtered pressure altitude + RoCD, and the
        // separate geometric altitude — each only while fresh (a coasted
        // vertical estimate is withheld, like stale DAPs). QNH correction is
        // the output side's job; the tracker stays in pressure-altitude space.
        barometric_altitude_ft: track
            .vertical_estimate(time, PROVENANCE_FRESH_S)
            .map(|(altitude_ft, _)| altitude_ft),
        barometric_qnh_corrected: false,
        geometric_altitude_ft: track.geometric_altitude_ft(time, PROVENANCE_FRESH_S),
        rocd_ft_min: track
            .vertical_estimate(time, PROVENANCE_FRESH_S)
            .map(|(_, rocd)| rocd),
        // Kinematics chain (VERT.3): fresh acceleration, and the mode of
        // movement only when at least one axis carries a determination.
        acceleration_mps2: track.acceleration_mps2(time, PROVENANCE_FRESH_S),
        mode_of_movement: Some(track.mode_of_movement(time, PROVENANCE_FRESH_S))
            .filter(ModeOfMovement::is_determined),
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
/// Häppchen 13.2 + 13.5d). The missed-revisit budget counts the track's **own**
/// revisit intervals ([`Track::expected_revisit`]), floored by the slowest
/// **configured** sensor scan period `cadence`: a track lives or dies by how
/// overdue *its* next update is, but is never deleted faster than the feed's
/// slowest radar would revisit it. This is the time-continuous analog of
/// [`should_delete`]'s `coast_reference` — it stops a track that a fast sensor
/// saw briefly (short own-revisit estimate) from being churned away across the
/// longer gap until a slow radar revisits it. Häppchen 13.5c floored this by an
/// *online-estimated* sensor period, which 13.6's azimuth-spread plot times made
/// too short (it picked up sub-revolution gaps between sensors instead of a
/// sensor's own revolution); 13.5d floors it by the *configured*
/// [`SensorModel::scan_period`] instead. A track that has not yet measured its
/// own cadence falls back to [`NOMINAL_REVISIT_INTERVAL`]; a freshly hit track
/// (`update_age == 0`) is never deleted.
fn should_delete_continuous(
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
    let interval = track
        .expected_revisit(NOMINAL_REVISIT_INTERVAL)
        .max(cadence);
    age > 0.0 && age >= budget * interval
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::{
        DetectionKind, Measurement, ModeAC, Plot, Provenance, SensorId, SourceKind,
    };
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
            4.0,
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
            measurement: firefly_core::Measurement::Polar(Polar::new(range, az, 0.0)),
            kind: DetectionKind::Combined,
            source: SourceKind::ModeS,
            mode_ac: ModeAC {
                mode_3a: Some(mode_3a),
                flight_level_ft: Some(35_000.0),
                icao_address: Some(icao),
                callsign: None,
                spi: false,
                geometric_height_ft: None,
                daps: firefly_core::Daps::default(),
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

    /// While a track is alive it is never reported as ended, and the live output
    /// carries `ended = false`. ADR 0016. REQ: FR-TRK-029
    #[test]
    fn live_track_is_never_ended() {
        let mut tracker = Tracker::new(config());
        tracker.process_scan(Timestamp(0.0), &[plot(0.0, 50_000.0, 0.0)]);
        assert!(
            tracker.take_ended_tracks().is_empty(),
            "a live track must not be buffered as ended"
        );
        assert!(
            tracker.system_tracks().iter().all(|st| !st.ended),
            "live system tracks carry ended = false"
        );
    }

    /// When a track is deleted it is reported **exactly once** as an ended
    /// `SystemTrack` (TSE), carrying its identity and last known state; a second
    /// drain returns nothing. ADR 0016. REQ: FR-TRK-029
    #[test]
    fn deleted_track_is_buffered_once_with_ended_flag() {
        let mut tracker = Tracker::new(config());
        // Confirm a track carrying an SSR identity.
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_scan(
                Timestamp(t),
                &[plot_with_identity(t, 50_000.0, 0.0, 0o1234, 0xABCDEF)],
            );
        }
        let id = tracker.tracks()[0].id();
        assert!(tracker.take_ended_tracks().is_empty(), "still alive");

        // Starve it until deletion (delete_misses_confirmed = 4).
        for k in 3..7 {
            let t = k as f64 * 4.0;
            tracker.process_scan(Timestamp(t), &[]);
        }
        assert_eq!(tracker.tracks().len(), 0, "track should be deleted");

        let ended = tracker.take_ended_tracks();
        assert_eq!(ended.len(), 1, "exactly one final report");
        assert!(ended[0].ended, "final report carries the TSE/ended flag");
        assert_eq!(ended[0].id, id, "it is the deleted track");
        assert_eq!(
            ended[0].mode_3a,
            Some(0o1234),
            "the final report carries the track's last known identity"
        );

        // Draining is one-shot: nothing is reported a second time.
        assert!(
            tracker.take_ended_tracks().is_empty(),
            "an ended track is reported exactly once, then never again"
        );
    }

    /// The wire track number (I062/040) is pool-managed (FR-TRK-035): a deleted
    /// track's number is quarantined — a newborn cannot claim it while a
    /// consumer might still associate it with the ended track — and only after
    /// the quarantine may it be reused. With the whole space in use or
    /// quarantined, initiation is declined instead of emitting a duplicate.
    /// REQ: FR-TRK-035
    #[test]
    fn track_number_is_quarantined_after_deletion_then_reused() {
        use crate::track_number::{TrackNumberPool, TRACK_NUMBER_QUARANTINE_SECS};

        let mut tracker = Tracker::new(config());
        // Shrink the number space to a single number so the test reaches the
        // reuse/exhaustion behaviour without 65 535 track births.
        tracker.track_numbers = TrackNumberPool::with_fresh_limit(1);

        // Birth at t = 0: the track carries wire number 1, and so does its
        // system-track projection.
        tracker.process_scan(Timestamp(0.0), &[plot(0.0, 30_000.0, 1.0)]);
        assert_eq!(tracker.tracks()[0].track_number(), 1);
        assert_eq!(tracker.system_tracks()[0].track_number, 1);

        // Starve the tentative track to deletion (t = 8); the final TSE report
        // still carries the number the consumer knows it by.
        tracker.process_scan(Timestamp(4.0), &[]);
        tracker.process_scan(Timestamp(8.0), &[]);
        let ended = tracker.take_ended_tracks();
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0].track_number, 1, "TSE report keeps the number");

        // A new target inside the quarantine window: the only number is still
        // quarantined, so initiation is declined — no track under a number the
        // consumer may still attribute to the ended one.
        tracker.process_scan(Timestamp(12.0), &[plot(12.0, 50_000.0, 0.0)]);
        assert!(
            tracker.tracks().is_empty(),
            "initiation declined while the pool is exhausted"
        );

        // After the quarantine the number returns to circulation.
        let t = 8.0 + TRACK_NUMBER_QUARANTINE_SECS + 2.0;
        tracker.process_scan(Timestamp(t), &[plot(t, 50_000.0, 0.0)]);
        assert_eq!(tracker.tracks().len(), 1);
        assert_eq!(
            tracker.tracks()[0].track_number(),
            1,
            "reused after quarantine"
        );
    }

    /// The MON flag follows the track's *fresh* sensor support: one sensor →
    /// monosensor; a second sensor joining → multisensor; that sensor falling
    /// silent past the freshness window → monosensor again. This is windowed
    /// on the per-sensor hit book, not the per-scan contributing set, so an
    /// asynchronous feed whose sensors rarely share an opportunity does not
    /// flap the flag. REQ: FR-TRK-036
    #[test]
    fn monosensor_flag_follows_fresh_sensor_support() {
        let error = || SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08);
        let config = TrackerConfig::new(frame())
            .with_sensor(SensorId(1), frame(), error(), 4.0)
            .with_sensor(SensorId(2), frame(), error(), 4.0);
        let mut tracker = Tracker::new(config);

        // Only sensor 1 sees the target: no cross-check → MON.
        tracker.process_scan(Timestamp(0.0), &[plot(0.0, 50_000.0, 0.0)]);
        assert!(tracker.system_tracks()[0].monosensor, "single sensor");

        // Sensor 2 joins the same target: cross-checked → multisensor.
        let p2 = Plot::primary(SensorId(2), Timestamp(4.0), Polar::new(50_000.0, 0.0, 0.0));
        tracker.process_scan(Timestamp(4.0), &[plot(4.0, 50_000.0, 0.0), p2]);
        assert!(!tracker.system_tracks()[0].monosensor, "two fresh sensors");

        // Sensor 2 falls silent; sensor 1 keeps the track alive. Once sensor
        // 2's last hit (at t = 4) ages *strictly* past the freshness window
        // the track is MON again — scan until its age exceeds the window.
        let mut t = 8.0;
        while t - 4.0 <= PROVENANCE_FRESH_S + 4.0 {
            tracker.process_scan(Timestamp(t), &[plot(t, 50_000.0, 0.0)]);
            t += 4.0;
        }
        assert_eq!(tracker.tracks().len(), 1, "kept alive by sensor 1");
        assert!(
            tracker.system_tracks()[0].monosensor,
            "sensor 2 aged out of the freshness window"
        );
    }

    /// The SPI ("ident") pulse of the last associated report reaches the
    /// system track — and clears again on the next report without it.
    /// REQ: FR-TRK-036
    #[test]
    fn spi_reflects_the_last_associated_report() {
        let mut tracker = Tracker::new(config());
        let with_spi = |time: f64, spi: bool| {
            let mut p = plot_with_identity(time, 50_000.0, 0.0, 0o1234, 0xABCDEF);
            p.mode_ac.spi = spi;
            p
        };

        tracker.process_scan(Timestamp(0.0), &[with_spi(0.0, false)]);
        assert!(!tracker.system_tracks()[0].spi);

        tracker.process_scan(Timestamp(4.0), &[with_spi(4.0, true)]);
        assert!(tracker.system_tracks()[0].spi, "ident pressed");

        tracker.process_scan(Timestamp(8.0), &[with_spi(8.0, false)]);
        assert!(!tracker.system_tracks()[0].spi, "transient: cleared again");
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

    /// The measured flight level (Mode C) reaches the system track and is
    /// sticky in the same way as the identity: a later primary-only hit (no
    /// Mode C reply) keeps the last known level. REQ: FR-TRK-027
    #[test]
    fn measured_flight_level_reaches_system_track_and_stays_sticky() {
        let mut tracker = Tracker::new(config());

        // Born and confirmed with SSR-equipped plots (flight level = 35000 ft).
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_scan(
                Timestamp(t),
                &[plot_with_identity(t, 50_000.0, 0.0, 0o2613, 0x0040_0123)],
            );
        }
        assert_eq!(tracker.system_tracks()[0].flight_level_ft, Some(35_000.0));

        // A primary-only hit must not erase the known level.
        tracker.process_scan(Timestamp(12.0), &[plot(12.0, 50_000.0, 0.0)]);
        assert_eq!(
            tracker.system_tracks()[0].flight_level_ft,
            Some(35_000.0),
            "flight level stays sticky through a primary-only hit"
        );
    }

    /// A primary-only track records only a PSR age and derives `Psr`
    /// provenance; the other technology ages stay `None`. REQ: FR-TRK-034
    #[test]
    fn primary_only_track_records_psr_age_and_provenance() {
        let mut tracker = Tracker::new(config());
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_scan(Timestamp(t), &[plot(t, 50_000.0, 0.0)]);
        }
        let st = &tracker.system_tracks()[0];
        assert!(st.source_ages.psr.is_some(), "PSR age recorded");
        assert_eq!(st.source_ages.ssr, None);
        assert_eq!(st.source_ages.mode_s, None);
        assert_eq!(st.source_ages.adsb, None);
        assert_eq!(st.source_ages.flarm, None);
        assert_eq!(st.provenance(), Provenance::Psr);
    }

    /// A combined PSR+Mode-S dwell books **both** technologies (FR-TRK-034,
    /// ADR 0027 §1): the plot carries `kind.has_primary()` *and* a Mode-S
    /// `source`, so the track records a PSR age and a Mode-S age, and the
    /// derived provenance is `Combined` (two fresh technologies). REQ: FR-TRK-034
    #[test]
    fn combined_mode_s_dwell_books_psr_and_mode_s_and_combines() {
        let mut tracker = Tracker::new(config());
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_scan(
                Timestamp(t),
                &[plot_with_identity(t, 50_000.0, 0.0, 0o2613, 0x0040_0123)],
            );
        }
        let st = &tracker.system_tracks()[0];
        assert!(st.source_ages.psr.is_some(), "primary half of the dwell");
        assert!(st.source_ages.mode_s.is_some(), "Mode S half of the dwell");
        assert_eq!(st.provenance(), Provenance::Combined);
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
            .with_sensor(SensorId(1), frame_a, error, 4.0)
            .with_sensor(SensorId(2), frame_b, error, 4.0);
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
            .with_sensor(SensorId(1), frame_a, error, 4.0)
            .with_sensor(SensorId(2), frame_b, error, 4.0);
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
            .with_sensor(SensorId(1), frame(), error, 4.0)
            .with_sensor(SensorId(2), frame(), error, 4.0);
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

    // --- ADR 0013, Häppchen 13.5: joint association over near-simultaneous --
    //     plots on the asynchronous path -----------------------------------

    /// Two radars at different sites paint the *same* aircraft at the *same*
    /// instant, fed as one simultaneous opportunity through `process_plots`.
    /// The simultaneity window binds them into one joint association against a
    /// frozen reference (ADR 0011), so they fuse into a single track instead of
    /// the second radar's plot spawning a ghost — the asynchronous counterpart
    /// to `two_sensors_seeing_one_aircraft_make_one_track`. REQ: FR-TRK-010,
    /// FR-TRK-025
    #[test]
    fn process_plots_fuse_simultaneous_two_radars_into_one_track() {
        let tracking = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
        let frame_a = LocalFrame::new(Wgs84::from_degrees(48.0, 11.0, 0.0));
        let frame_b = LocalFrame::new(Wgs84::from_degrees(47.7, 11.6, 0.0)); // ~55 km SE
        let error = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08);
        let cfg = TrackerConfig::new(tracking)
            .with_sensor(SensorId(1), frame_a, error, 4.0)
            .with_sensor(SensorId(2), frame_b, error, 4.0);
        let mut tracker = Tracker::new(cfg);

        let target = Wgs84::from_degrees(48.1, 11.3, 0.0);
        for k in 0..4 {
            let t = k as f64 * 4.0;
            tracker.process_plots(&[
                plot_seen_by(SensorId(1), &frame_a, t, &target),
                plot_seen_by(SensorId(2), &frame_b, t, &target),
            ]);
        }

        assert_eq!(
            tracker.tracks().len(),
            1,
            "two simultaneous radars must fuse into one track, not a ghost"
        );
        assert_eq!(tracker.confirmed_count(), 1);
    }

    /// Two targets cross in azimuth as a single radar reports them at nearly the
    /// same instant. Processed one plot at a time their shared, overlapping
    /// gates would let either track grab either plot and the identities could
    /// swap or coalesce; the simultaneity window instead associates the pair
    /// *jointly* with exclusivity (FR-TRK-018/019/020), so each track keeps its
    /// own target — verified by both the surviving velocity directions and the
    /// SSR identities not swapping. REQ: FR-TRK-018, FR-TRK-019, FR-TRK-020,
    /// FR-TRK-025
    #[test]
    fn process_plots_keep_crossing_targets_distinct() {
        let mut tracker = Tracker::new(config());

        // One radar at the origin. Target A starts west of due-north and flies
        // east (squawk 0o1111); target B starts east and flies west (0o2222),
        // 120 m further north so they pass close but never collide. Each round
        // both are reported at the same instant.
        let crossing_plot = |t: f64, east: f64, north: f64, squawk: u16| Plot {
            sensor: SensorId(1),
            time: Timestamp(t),
            measurement: firefly_core::Measurement::Polar(
                firefly_geo::Enu::new(east, north, 0.0).to_polar(),
            ),
            kind: DetectionKind::Combined,
            source: SourceKind::Ssr,
            mode_ac: ModeAC {
                mode_3a: Some(squawk),
                flight_level_ft: Some(35_000.0),
                icao_address: None,
                callsign: None,
                spi: false,
                geometric_height_ft: None,
                daps: firefly_core::Daps::default(),
            },
        };

        let speed = 150.0; // m/s
        let dt = 3.0;
        for k in 0..12 {
            let t = k as f64 * dt;
            let a = crossing_plot(t, -2500.0 + speed * t, 50_000.0, 0o1111);
            let b = crossing_plot(t, 2500.0 - speed * t, 50_120.0, 0o2222);
            tracker.process_plots(&[a, b]);
        }

        assert_eq!(tracker.tracks().len(), 2, "two distinct tracks survive");
        assert_eq!(tracker.confirmed_count(), 2, "both confirmed");

        // One track ends moving east, the other west — they did not coalesce or
        // both follow the same target.
        let mut tracks: Vec<&Track> = tracker.tracks().iter().collect();
        tracks.sort_by(|x, y| x.velocity()[0].partial_cmp(&y.velocity()[0]).unwrap());
        let westbound = tracks[0];
        let eastbound = tracks[1];
        assert!(
            westbound.velocity()[0] < 0.0 && eastbound.velocity()[0] > 0.0,
            "tracks keep opposite east-velocities: {} and {}",
            westbound.velocity()[0],
            eastbound.velocity()[0]
        );

        // And identity did not swap: the eastbound target was A (0o1111), the
        // westbound was B (0o2222).
        assert_eq!(
            eastbound.mode_3a(),
            Some(0o1111),
            "eastbound keeps A's squawk"
        );
        assert_eq!(
            westbound.mode_3a(),
            Some(0o2222),
            "westbound keeps B's squawk"
        );
    }

    /// A track briefly seen by a fast sensor settles a short own-revisit
    /// estimate; once the fast sensor drops out, only a slow (12 s) sensor still
    /// covers it. The **cadence floor** (Häppchen 13.5d, configured sensor scan
    /// periods) keeps it alive across the long gap instead of churning it
    /// away — without the floor the short revisit estimate (≈2 s, confirmed
    /// budget 4) would delete it after ≈8 s of coast. REQ: FR-TRK-023,
    /// FR-TRK-026
    #[test]
    fn process_plots_cadence_floor_survives_a_slow_sensor_gap() {
        let error = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08);
        let cfg = TrackerConfig::new(frame())
            .with_sensor(SensorId(1), frame(), error, 2.0) // fast sensor
            .with_sensor(SensorId(2), frame(), error, 12.0); // slow sensor
        let mut tracker = Tracker::new(cfg);

        let fast =
            |t: f64| Plot::primary(SensorId(1), Timestamp(t), Polar::new(50_000.0, 0.0, 0.0));
        let slow =
            |t: f64| Plot::primary(SensorId(2), Timestamp(t), Polar::new(50_000.0, 0.0, 0.0));

        // Both sensors register a first observation at t = 0.
        tracker.process_plots(&[fast(0.0), slow(0.0)]);
        // Fast sensor hits every 2 s up to t = 12 (own revisit settles ≈2 s);
        // the slow sensor re-hits at t = 12, establishing its 12 s period — so
        // the cadence floor becomes 12 s.
        for k in 1..=6 {
            tracker.process_plots(&[fast(k as f64 * 2.0)]);
        }
        tracker.process_plots(&[slow(12.0)]);
        assert_eq!(tracker.confirmed_count(), 1);
        let id = tracker.tracks()[0].id();

        // Fast sensor drops out. A far-away plot at t = 20 advances data time by
        // 8 s without touching the north track, which now coasts with age = 8 s —
        // past budget × own-revisit (4 × 2 s) but well inside budget × cadence
        // floor (4 × 12 s), so the floor must keep it alive.
        let decoy = Plot::primary(
            SensorId(1),
            Timestamp(20.0),
            Polar::new(30_000.0, std::f64::consts::FRAC_PI_2, 0.0),
        );
        tracker.process_plots(&[decoy]);

        assert!(
            tracker
                .tracks()
                .iter()
                .any(|tr| tr.id() == id && tr.is_confirmed()),
            "the slowly-revisited track must survive the gap via the cadence floor"
        );
    }

    /// When plots are well separated in time (more than the simultaneity
    /// window), `process_plots` must behave exactly like feeding them one at a
    /// time through `process_plot`: micro-batching only binds genuinely
    /// coincident plots, and the result is independent of how the stream is
    /// chunked (determinism, ADR 0003). REQ: FR-TRK-025
    #[test]
    fn process_plots_equals_repeated_process_plot_when_well_separated() {
        let plots: Vec<Plot> = (0..6)
            .map(|k| plot(k as f64 * 4.0, 50_000.0, 0.0))
            .collect();

        let mut one_at_a_time = Tracker::new(config());
        for p in &plots {
            one_at_a_time.process_plot(p);
        }

        let mut in_one_call = Tracker::new(config());
        in_one_call.process_plots(&plots);

        assert_eq!(
            one_at_a_time, in_one_call,
            "chunking a time-separated stream must not change the tracker state"
        );
        assert_eq!(in_one_call.confirmed_count(), 1);
    }

    // --- ADR 0013, Häppchen 13.3: read-only time snapshot ------------------

    /// `snapshot_at` predicts every track forward to a common output time and
    /// must leave the tracker's own state untouched: a far-future snapshot
    /// neither advances nor deletes the live track, and repeating a snapshot
    /// yields the same result. REQ: FR-TRK-024
    #[test]
    fn snapshot_at_predicts_forward_and_is_read_only() {
        let mut tracker = Tracker::new(config());
        // A target moving due east at 200 m/s, seen every 4 s.
        let speed = 200.0;
        for k in 0..5 {
            let t = k as f64 * 4.0;
            let polar = firefly_geo::Enu::new(speed * t, 50_000.0, 0.0).to_polar();
            tracker.process_plot(&Plot::primary(SensorId(1), Timestamp(t), polar));
        }
        assert_eq!(tracker.confirmed_count(), 1);

        // Current estimate (at last update t = 16) and the state we must not touch.
        let east_now = frame()
            .geodetic_to_enu(&tracker.system_tracks()[0].position)
            .east;
        let pos_before = tracker.tracks()[0].position();
        let n_before = tracker.tracks().len();

        // Snapshot 4 s into the future: the track is predicted along its velocity
        // and sits well ahead of its last update.
        let snap = tracker.snapshot_at(Timestamp(20.0));
        assert_eq!(snap.len(), 1);
        let east_future = frame().geodetic_to_enu(&snap[0].position).east;
        assert!(
            east_future > east_now + 400.0,
            "predicted east {east_future} should be ahead of {east_now}"
        );
        assert!(
            (snap[0].time.as_secs() - 20.0).abs() < 1e-9,
            "snapshot carries t"
        );

        // Read-only: even a far-future snapshot must not delete or mutate the
        // live track, and a repeated snapshot is identical.
        let _ = tracker.snapshot_at(Timestamp(10_000.0));
        assert_eq!(tracker.tracks().len(), n_before, "snapshot must not delete");
        assert_eq!(
            tracker.tracks()[0].position(),
            pos_before,
            "snapshot must not mutate the live estimate"
        );
        let east_again = frame()
            .geodetic_to_enu(&tracker.snapshot_at(Timestamp(20.0))[0].position)
            .east;
        assert!(
            (east_again - east_future).abs() < 1e-9,
            "snapshot is deterministic"
        );
    }

    /// `snapshot_at` evaluates the safety status at the *output* time, not the
    /// last update time: a track fresh at its last hit shows up coasting with a
    /// matching update age when projected into the future. REQ: FR-TRK-024
    #[test]
    fn snapshot_at_reports_age_and_coasting_at_output_time() {
        let mut tracker = Tracker::new(config());
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_plot(&plot(t, 50_000.0, 0.0));
        }
        // At the last update (t = 8) the track is fresh.
        let now = tracker.system_tracks();
        assert!(!now[0].coasting);
        assert!(now[0].update_age.abs() < 1e-9);

        // Projected 4 s later it coasts, with a 4 s update age — no plot needed.
        let snap = tracker.snapshot_at(Timestamp(12.0));
        assert!(snap[0].coasting, "past the last hit → coasting");
        assert!(
            (snap[0].update_age - 4.0).abs() < 1e-9,
            "update age = t − last hit"
        );
        assert!((snap[0].time.as_secs() - 12.0).abs() < 1e-9);
    }

    /// A track sampled by the periodic output a sliver after its last hit is
    /// **not** coasting: the fixed output heartbeat (ADR 0013) is decoupled from
    /// the asynchronous input, so it almost always lands just past the last hit.
    /// Regression guard for the bug where every track coasted permanently because
    /// the test was merely `t > last_hit_time`.
    #[test]
    fn snapshot_at_does_not_coast_a_freshly_updated_track() {
        let mut tracker = Tracker::new(config());
        // Hits every 4 s → revisit interval settles to 4 s.
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_plot(&plot(t, 50_000.0, 0.0));
        }
        // Output heartbeat lands 0.1 s after the last hit (t = 8.0): the track is
        // well within its expected revisit, so it must report as live.
        let snap = tracker.snapshot_at(Timestamp(8.1));
        assert!(
            !snap[0].coasting,
            "fresh track sampled mid-cadence must not coast"
        );
        assert!((snap[0].update_age - 0.1).abs() < 1e-9);
    }

    /// An empty tracker snapshots to an empty air picture.
    #[test]
    fn snapshot_at_on_empty_tracker_is_empty() {
        let tracker = Tracker::new(config());
        assert!(tracker.snapshot_at(Timestamp(5.0)).is_empty());
    }

    /// A plot whose ICAO address matches a live track is directly associated
    /// even when it lies well outside the kinematic gate — the identity match
    /// is authoritative, bypassing the Mahalanobis distance check.
    /// REQ: FR-TRK-031
    #[test]
    fn icao_match_bypasses_kinematic_gate() {
        let mut tracker = Tracker::new(config());
        let icao: u32 = 0x3C_65_AC;

        // Build a confirmed northbound track with a known ICAO address.
        for k in 0..3 {
            let t = k as f64 * 4.0;
            tracker.process_scan(
                Timestamp(t),
                &[plot_with_identity(t, 50_000.0, 0.0, 0o1234, icao)],
            );
        }
        assert_eq!(tracker.confirmed_count(), 1);
        let track_id = tracker.tracks()[0].id();

        // Send a plot ~111 km away (far outside the kinematic gate) but carrying
        // the matching ICAO address. Using range=100 km due east (az = π/2).
        let far_plot =
            plot_with_identity(12.0, 100_000.0, std::f64::consts::PI / 2.0, 0o1234, icao);
        tracker.process_scan(Timestamp(12.0), &[far_plot]);

        // The ICAO pre-sort must have associated the far plot directly:
        // track still alive, same id, not coasting.
        assert_eq!(
            tracker.tracks().len(),
            1,
            "no ghost should appear from the ICAO-handled plot"
        );
        assert_eq!(
            tracker.tracks()[0].id(),
            track_id,
            "original track survives"
        );
        assert!(
            !tracker.tracks()[0].is_coasting(),
            "ICAO-matched plot counts as a hit"
        );
    }

    /// A plot whose ICAO address does not match any live track falls through
    /// to the normal JPDA pool and can initiate a new tentative track.
    /// REQ: FR-TRK-031
    #[test]
    fn icao_no_match_falls_through_to_jpda() {
        let mut tracker = Tracker::new(config());

        // ADS-B geodetic plot with an ICAO address that no track knows yet.
        let adsb_plot = Plot {
            sensor: SensorId(1),
            time: Timestamp(0.0),
            measurement: Measurement::Geodetic {
                position: Wgs84::from_degrees(47.09, 8.0, 500.0), // ~10 km north
                sigma_pos_m: 30.0,
            },
            kind: DetectionKind::Secondary,
            source: SourceKind::AdsB,
            mode_ac: ModeAC {
                icao_address: Some(0xAB_CD_EF),
                ..ModeAC::default()
            },
        };
        tracker.process_scan(Timestamp(0.0), &[adsb_plot]);

        // No matching track → JPDA initiates a tentative track as normal.
        assert_eq!(
            tracker.tracks().len(),
            1,
            "ADS-B with unknown ICAO should still initiate a track"
        );
        assert_eq!(
            tracker.tracks()[0].icao_address(),
            Some(0xAB_CD_EF),
            "ICAO address propagated to the new track"
        );
    }
}
