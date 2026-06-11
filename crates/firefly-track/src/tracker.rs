//! The tracker: the per-scan loop that turns a plot stream into managed tracks.
//!
//! Each scan (a batch of plots sharing a time) drives one pure state
//! transition. The order matters:
//!
//! 1. **Predict** every existing track to the scan time.
//! 2. **Convert** each plot to a Cartesian measurement (Häppchen 2.1).
//! 3. **Associate** predicted tracks with measurements (gating + GNN, 2.3/2.4).
//! 4. **Update** associated tracks (a *hit*); **coast** the rest (a *miss*).
//! 5. **Confirm** tentative tracks that reach M-of-N hits.
//! 6. **Delete** tracks that have missed too often.
//! 7. **Initiate** a new tentative track from each unassociated plot.
//!
//! Determinism (ADR 0003): [`Tracker::process_scan`] is a pure function of the
//! current state, the scan time and the plots — no wall clock, no I/O — so the
//! whole run is replayable and the state is recoverable. The state
//! ([`Track`] list) is plain, serialisable data (NFR-CLOUD-001/002/003).

use std::collections::{BTreeMap, BTreeSet};

use firefly_core::{Plot, SensorId, SystemTrack, Timestamp, TrackId};
use firefly_geo::{Enu, LocalFrame};
use serde::{Deserialize, Serialize};

use crate::association::associate;
use crate::gating::Gate;
use crate::imm::ImmConfig;
use crate::kalman::{LinearKalman, ProcessNoise};
use crate::measurement::{convert_plot, CartesianMeasurement, SensorErrorModel};
use crate::track::{Track, TrackStatus};

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
    /// The validation gate.
    pub gate: Gate,
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
}

impl Tracker {
    pub fn new(config: TrackerConfig) -> Self {
        Self {
            config,
            tracks: Vec::new(),
            next_id: 1,
        }
    }

    /// All tracks the tracker currently maintains (tentative and confirmed).
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
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
        let confirm_m = self.config.confirm_m;
        let confirm_n = self.config.confirm_n;
        let initial_velocity_std = self.config.initial_velocity_std;
        let delete_tentative = self.config.delete_misses_tentative;
        let delete_confirmed = self.config.delete_misses_confirmed;
        let tracking_frame = self.config.tracking_frame;
        let imm_config = self.config.imm.clone();
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
                let (z, r) = tracking_frame.horizontal_from(&model.frame, local.z, local.r);
                by_sensor
                    .entry(p.sensor)
                    .or_default()
                    .push((p, CartesianMeasurement { z, r }));
            }
        }

        // 3. Sequentially associate & update, one sensor at a time. A track is
        //    recorded as hit (this scan) when any sensor's plot associates to it
        //    or founds it.
        let mut hit_ids: BTreeSet<TrackId> = BTreeSet::new();
        for (&sensor, items) in &by_sensor {
            let measurements: Vec<CartesianMeasurement> = items.iter().map(|(_, m)| *m).collect();
            // Gate and associate against each track's IMM combined estimate.
            let filters: Vec<LinearKalman> = self.tracks.iter().map(|tr| tr.estimate()).collect();
            let assoc = associate(&filters, &measurements, &gate);

            // Update associated tracks (a hit): fold the plot into the IMM,
            // which re-weights its motion models by how well each predicted it.
            for &(ti, mi) in &assoc.pairs {
                self.tracks[ti].imm.update(&measurements[mi]);
                self.tracks[ti].last_hit_time = t;
                self.tracks[ti].update_identity(&items[mi].0.mode_ac);
                self.tracks[ti].record_hit_from(sensor);
                hit_ids.insert(self.tracks[ti].id());
            }
            // Initiate a new tentative track from each unassociated measurement;
            // it becomes visible to the *next* sensor's association this scan.
            for &mi in &assoc.unassigned_measurements {
                let filter =
                    LinearKalman::from_first_measurement(&measurements[mi], initial_velocity_std);
                let imm = imm_config.seed(filter);
                let id = TrackId(self.next_id);
                self.next_id += 1;
                let mut track = Track::new(id, imm, t);
                track.update_identity(&items[mi].0.mode_ac);
                track.record_hit_from(sensor);
                self.tracks.push(track);
                hit_ids.insert(id);
            }
        }

        // 4. Book one hit/miss per track for this scan, then run the lifecycle.
        for track in &mut self.tracks {
            track.observe(hit_ids.contains(&track.id()), confirm_n);
        }

        // 5. Confirm tentative tracks that have reached M-of-N.
        for track in &mut self.tracks {
            if track.status() == TrackStatus::Tentative && track.hits_in_window() >= confirm_m {
                track.confirm();
            }
        }

        // 6. Delete tracks that have missed too often.
        self.tracks
            .retain(|track| !should_delete(track, delete_tentative, delete_confirmed));
    }
}

/// Whether a track has missed often enough to be deleted, given its status.
fn should_delete(track: &Track, delete_tentative: u32, delete_confirmed: u32) -> bool {
    let limit = match track.status() {
        TrackStatus::Tentative => delete_tentative,
        TrackStatus::Confirmed => delete_confirmed,
    };
    track.consecutive_misses() >= limit
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
}
