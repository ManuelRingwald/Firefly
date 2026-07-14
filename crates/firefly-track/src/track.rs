//! A single track and its lifecycle state.
//!
//! A track is a Kalman filter ([`LinearKalman`]) plus the bookkeeping that
//! decides whether it is real and whether it should stay alive:
//!
//! - **Tentative:** just born from an unassociated plot — might be clutter.
//! - **Confirmed:** seen enough times (M-of-N) to be trusted and reported.
//! - *Deleted* is not a state but removal from the track list.
//!
//! The track records its recent association outcomes (hit/miss) so the
//! [`crate::Tracker`] can apply M-of-N confirmation and miss-based deletion.

use std::collections::{BTreeMap, BTreeSet};

use firefly_core::{
    Callsign, CourseTrend, Daps, ModeAC, ModeOfMovement, SensorId, SourceKind, SpeedTrend, TrackId,
    VerticalTrend,
};
use nalgebra::Vector2;
use serde::{Deserialize, Serialize};

use crate::imm::Imm;
use crate::kalman::LinearKalman;

/// EWMA weight for the per-track revisit-interval estimate: how strongly the
/// latest inter-hit gap pulls the running estimate. `0.5` adapts within a couple
/// of revisits while smoothing single missed detections.
const REVISIT_EWMA: f64 = 0.5;

/// Upper bound on remembered hit times — the confirmation window never needs
/// more than a handful, so a small cap keeps the per-track state bounded.
const MAX_RECENT_HITS: usize = 16;
/// Groundspeed-trend threshold (VERT.3): along-track accelerations below this
/// magnitude (m/s²) read as "constant groundspeed". 0.2 m/s² ≈ 0.4 kt/s —
/// well above the smoothed estimator's noise floor, well below any real
/// speed change an ATCO cares about.
const SPEED_TREND_THRESHOLD_MPS2: f64 = 0.2;
/// Below this groundspeed (m/s) the along-track direction is too unstable
/// for a speed-trend claim.
const SPEED_TREND_MIN_SPEED_MPS: f64 = 5.0;
/// Vertical-trend threshold (VERT.3): |RoCD| below this (ft/min) reads as
/// level flight — the conventional label threshold.
const VERTICAL_TREND_THRESHOLD_FT_MIN: f64 = 300.0;
/// A turn model must carry this much probability mass before the track is
/// called turning — below it, "constant course" is the honest default
/// (the IMM priors alone must not paint every fresh track as turning).
const TURN_PROBABILITY_THRESHOLD: f64 = 0.5;

/// EWMA gain for the geometric-altitude smoother (VERT.2): geometric heights
/// arrive already fairly clean (GNSS), so a light smoothing that follows
/// climbs within a few reports beats a heavy filter.
const GEOMETRIC_EWMA_ALPHA: f64 = 0.3;

/// Lifecycle status of a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackStatus {
    /// On probation — not yet trusted.
    Tentative,
    /// Confirmed as a real target.
    Confirmed,
}

/// Per-technology last-hit data times (seconds), one slot per surveillance
/// technology (ADR 0027). Drives the per-source ages of CAT062 I062/290 and the
/// derived track provenance. `None` until a plot of that technology has
/// contributed; keeps the **latest** time per technology.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct SourceHits {
    pub psr: Option<f64>,
    pub ssr: Option<f64>,
    pub mode_s: Option<f64>,
    pub adsb: Option<f64>,
    pub flarm: Option<f64>,
}

impl SourceHits {
    /// Record a contribution of `source` at data time `time`. `has_primary`
    /// additionally bumps the PSR age (a combined PSR+SSR/Mode-S dwell feeds both
    /// technologies). Monotonic per slot — an older time never overwrites a newer.
    fn record(&mut self, source: SourceKind, time: f64, has_primary: bool) {
        fn bump(slot: &mut Option<f64>, time: f64) {
            if slot.is_none_or(|t| time > t) {
                *slot = Some(time);
            }
        }
        if has_primary {
            bump(&mut self.psr, time);
        }
        match source {
            SourceKind::Psr => bump(&mut self.psr, time),
            SourceKind::Ssr => bump(&mut self.ssr, time),
            SourceKind::ModeS => bump(&mut self.mode_s, time),
            SourceKind::AdsB => bump(&mut self.adsb, time),
            SourceKind::Flarm => bump(&mut self.flarm, time),
        }
    }
}

/// A maintained track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    id: TrackId,
    /// The 16-bit **wire** track number (CAT062 I062/040), allocated from the
    /// tracker's [`TrackNumberPool`](crate::track_number::TrackNumberPool) at
    /// birth and returned to it (via quarantine) on deletion. Deliberately
    /// separate from `id`: the internal id stays process-unique for
    /// association/bookkeeping, while the number lives in the managed 16-bit
    /// space a consumer keys its picture by (FR-TRK-035).
    number: u16,
    status: TrackStatus,
    /// The track's **IMM** bank (constant velocity + coordinated turns,
    /// Häppchen M5.4). Its [`combined_estimate`](Imm::combined_estimate) is the
    /// single state the rest of the tracker reasons with.
    pub(crate) imm: Imm,
    /// Data time of the last predict/update, seconds.
    pub(crate) last_time: f64,
    /// Data time of the last *real* measurement (hit), seconds. Drives the
    /// update age — how long the track has been running on prediction alone.
    pub(crate) last_hit_time: f64,
    /// Data times of recent hits (most recent at the back), bounded. The
    /// confirmation rule counts how many fall inside an adaptive time window
    /// (ADR 0012) — asynchronous sensors deliver hits at irregular instants, so
    /// a *time* window is robust where a fixed scan-count window is not.
    recent_hits: Vec<f64>,
    /// Estimated **revisit interval** (seconds): an EWMA of the data-time gaps
    /// between this track's hits. With several asynchronous radars a track is
    /// hit far more often than any single radar revisits, so this adapts the
    /// lifecycle to *this* track's actual update cadence (ADR 0012). `0` until
    /// the first inter-hit gap is seen.
    revisit_interval: f64,
    /// Most recently reported Mode 3/A code ("squawk"), if any SSR-equipped
    /// plot has ever associated with this track. Sticky: a plot without an
    /// SSR reply (e.g. a primary-only detection) does not clear it.
    mode_3a: Option<u16>,
    /// Most recently reported Mode S 24-bit ICAO address, if any SSR-equipped
    /// plot has ever associated with this track. Sticky, like `mode_3a`.
    icao_address: Option<u32>,
    /// Most recently reported barometric flight level (Mode C pressure
    /// altitude, feet), if any SSR-equipped plot has ever associated with this
    /// track. Sticky, like `mode_3a`: a primary-only detection (no Mode C
    /// reply) does not clear the last known level. Unlike identity it does
    /// change over time as the aircraft climbs/descends — we simply keep the
    /// latest reported value (no vertical tracking filter yet).
    flight_level_ft: Option<f64>,
    /// Most recently reported callsign / flight ID (Mode S target
    /// identification), if any SSR-equipped plot has ever associated with this
    /// track. Sticky, like `mode_3a`.
    callsign: Option<Callsign>,
    /// Sensors that contributed a hit (founded or updated this track) in the
    /// **most recent scan** (ADR 0010). Replaced wholesale each scan — unlike
    /// `mode_3a`/`icao_address` this is not sticky, since it answers "who sees
    /// it *right now*", not "who has ever seen it".
    contributing_sensors: BTreeSet<SensorId>,
    /// Per-technology last-hit data times (PSR/SSR/Mode S/ADS-B/FLARM), driving
    /// the per-source ages of CAT062 I062/290 and the derived provenance (ADR
    /// 0027). Generalises the former single `adsb_last_hit_time`.
    #[serde(default)]
    pub(crate) source_hits: SourceHits,
    /// Last-hit data time per **distinct sensor** (not technology). Unlike the
    /// per-scan `contributing_sensors` — which is empty between opportunities
    /// and would make the mono/multisensor status flap on an asynchronous
    /// multi-radar feed — this persists, so "supported by how many sensors?"
    /// is answered over a freshness window (CAT062 I062/080 MON, FR-TRK-036).
    #[serde(default)]
    sensor_hits: BTreeMap<SensorId, f64>,
    /// Whether the most recent associated report carried the SPI ("ident")
    /// pulse. Deliberately **not** sticky (unlike the identity fields): SPI
    /// describes the last reply only, and every associated plot overwrites it
    /// (CAT062 I062/080 SPI, FR-TRK-036).
    #[serde(default)]
    spi: bool,
    /// Downlink Aircraft Parameters (Mode S EHS, FEP.2): merged per field
    /// from every DAP-carrying associated report — different BDS registers
    /// arrive in different reports, so a per-field merge keeps the freshest
    /// valid value of each. Reported outward only while fresh (see
    /// [`fresh_daps`](Self::fresh_daps)).
    #[serde(default)]
    daps: Daps,
    /// Data time of the most recent DAP-carrying report, the freshness basis
    /// for [`fresh_daps`](Self::fresh_daps).
    #[serde(default)]
    daps_time: Option<f64>,
    /// The per-track vertical filter (VERT.2): barometric altitude + rate of
    /// climb/descent, fed by every Mode-C/FL-carrying associated report.
    /// `None` until the first such report.
    #[serde(default)]
    vertical: Option<crate::vertical::VerticalFilter>,
    /// Smoothed **geometric** (WGS-84) altitude, feet — EWMA over the
    /// genuinely geometric heights of associated reports (ADS-B I021/140,
    /// MLAT I020/105). Kept strictly separate from the barometric chain:
    /// the two use different references and must never be mixed.
    #[serde(default)]
    geometric_altitude_ft: Option<f64>,
    /// Data time of the last geometric-height report — the freshness basis
    /// for reporting [`geometric_altitude_ft`](Self::geometric_altitude_ft).
    #[serde(default)]
    geometric_time: Option<f64>,
    /// The per-track horizontal acceleration estimator (VERT.3), fed with the
    /// IMM combined velocity on every associated report. `None` until the
    /// first sample.
    #[serde(default)]
    acceleration: Option<crate::acceleration::AccelerationEstimator>,
}

impl Track {
    /// Create a fresh tentative track from an initialised IMM bank.
    pub(crate) fn new(id: TrackId, number: u16, imm: Imm, time: f64) -> Self {
        Self {
            id,
            number,
            status: TrackStatus::Tentative,
            imm,
            last_time: time,
            last_hit_time: time, // the founding plot is a hit
            recent_hits: vec![time],
            revisit_interval: 0.0,
            mode_3a: None,
            icao_address: None,
            flight_level_ft: None,
            callsign: None,
            contributing_sensors: BTreeSet::new(),
            source_hits: SourceHits::default(),
            sensor_hits: BTreeMap::new(),
            spi: false,
            daps: Daps::default(),
            daps_time: None,
            vertical: None,
            geometric_altitude_ft: None,
            geometric_time: None,
            acceleration: None,
        }
    }

    /// Track identifier.
    pub fn id(&self) -> TrackId {
        self.id
    }

    /// The 16-bit wire track number (CAT062 I062/040), pool-managed
    /// (FR-TRK-035).
    pub fn track_number(&self) -> u16 {
        self.number
    }

    /// Lifecycle status.
    pub fn status(&self) -> TrackStatus {
        self.status
    }

    /// Whether the track is confirmed.
    pub fn is_confirmed(&self) -> bool {
        self.status == TrackStatus::Confirmed
    }

    /// Whether the track is currently *coasting* — running on prediction alone
    /// because the most recent scan brought no fresh measurement for it.
    pub fn is_coasting(&self) -> bool {
        self.last_hit_time < self.last_time
    }

    /// Update age: data-time elapsed since the last real measurement, seconds.
    /// `0` right after a hit; grows by one scan interval per coasted scan.
    pub fn update_age(&self) -> f64 {
        self.last_time - self.last_hit_time
    }

    /// The IMM's combined estimate — the single Kalman state the tracker reads
    /// position, velocity and uncertainty from.
    pub(crate) fn estimate(&self) -> LinearKalman {
        self.imm.combined_estimate()
    }

    /// Estimated position `[east, north]`, metres.
    pub fn position(&self) -> Vector2<f64> {
        self.estimate().position()
    }

    /// Estimated velocity `[v_east, v_north]`, m/s.
    pub fn velocity(&self) -> Vector2<f64> {
        self.estimate().velocity()
    }

    /// Record a hit at data time `time`: refresh the revisit-interval estimate
    /// from the gap since the last hit and remember the hit time. Idempotent
    /// within one scan — a second sensor hitting the same track at the same
    /// `time` neither double-counts nor distorts the cadence estimate.
    pub(crate) fn mark_hit(&mut self, time: f64) {
        if time <= self.last_hit_time {
            return;
        }
        let gap = time - self.last_hit_time;
        // EWMA of inter-hit gaps; seed it with the first gap we ever see.
        self.revisit_interval = if self.revisit_interval <= 0.0 {
            gap
        } else {
            REVISIT_EWMA * gap + (1.0 - REVISIT_EWMA) * self.revisit_interval
        };
        self.last_hit_time = time;
        self.recent_hits.push(time);
        if self.recent_hits.len() > MAX_RECENT_HITS {
            let excess = self.recent_hits.len() - MAX_RECENT_HITS;
            self.recent_hits.drain(0..excess);
        }
    }

    /// The cadence the adaptive lifecycle windows are scaled by:
    /// `max(revisit interval, feed cadence)`. The `max` keeps a track alive for
    /// the larger of "several of *its own* revisits" and "several of the feed's
    /// scan intervals", so neither a many-sensor feed (a young track whose
    /// revisit estimate is still short) nor a slow replay deletes it early. The
    /// `cadence` is supplied by the tracker (the slowest radar's scan period, or
    /// the gap between scans when that is larger — ADR 0012).
    pub(crate) fn coast_reference(&self, cadence: f64) -> f64 {
        self.revisit_interval.max(cadence)
    }

    /// The revisit interval the **time-continuous** lifecycle scales by (ADR
    /// 0013, Häppchen 13.2): the track's own measured [`revisit_interval`] once
    /// it exists, else the `nominal` fallback for a freshly born track not yet
    /// seen a second time. Unlike [`coast_reference`](Self::coast_reference) it
    /// does **not** depend on a globally-estimated feed cadence — the
    /// asynchronous per-plot path governs a track purely by how often it is
    /// actually updated.
    pub(crate) fn expected_revisit(&self, nominal: f64) -> f64 {
        if self.revisit_interval > 0.0 {
            self.revisit_interval
        } else {
            nominal
        }
    }

    /// How many recent hits fall within the last `window` seconds (up to `now`).
    pub(crate) fn hits_within(&self, window: f64, now: f64) -> usize {
        debug_assert!(
            self.recent_hits.windows(2).all(|w| w[0] <= w[1]),
            "recent_hits must be sorted ascending; invariant violated"
        );
        let cutoff = now - window;
        self.recent_hits.iter().filter(|&&h| h >= cutoff).count()
    }

    /// Promote to confirmed.
    pub(crate) fn confirm(&mut self) {
        self.status = TrackStatus::Confirmed;
    }

    /// Most recently reported Mode 3/A code ("squawk"), if known.
    pub fn mode_3a(&self) -> Option<u16> {
        self.mode_3a
    }

    /// Most recently reported Mode S 24-bit ICAO address, if known.
    pub fn icao_address(&self) -> Option<u32> {
        self.icao_address
    }

    /// Most recently reported barometric flight level (feet), if known.
    pub fn flight_level_ft(&self) -> Option<f64> {
        self.flight_level_ft
    }

    /// Most recently reported callsign / flight ID, if known.
    pub fn callsign(&self) -> Option<Callsign> {
        self.callsign
    }

    /// The **filtered** vertical state (VERT.2), but only while fresh: the
    /// filter's last accepted Mode-C measurement lies within `window`
    /// seconds of `now`. Returns `(pressure altitude ft — predicted to
    /// `now` —, rate of climb/descent ft/min)`; a vertical estimate coasted
    /// beyond the window is withheld rather than reported as current
    /// (absence over a stale claim, like the DAPs).
    pub fn vertical_estimate(&self, now: f64, window: f64) -> Option<(f64, f64)> {
        let filter = self.vertical.as_ref()?;
        if now - filter.last_update_time() > window {
            return None;
        }
        Some((filter.altitude_ft_at(now), filter.rate_ft_min()))
    }

    /// The smoothed **geometric** (WGS-84) altitude in feet (VERT.2), but
    /// only while fresh (last geometric report within `window` of `now`).
    pub fn geometric_altitude_ft(&self, now: f64, window: f64) -> Option<f64> {
        match self.geometric_time {
            Some(t) if now - t <= window => self.geometric_altitude_ft,
            _ => None,
        }
    }

    /// The estimated horizontal acceleration `(a_east, a_north)` in m/s²
    /// (VERT.3), but only while fresh (last velocity sample within `window`
    /// of `now`).
    pub fn acceleration_mps2(&self, now: f64, window: f64) -> Option<(f64, f64)> {
        let estimator = self.acceleration.as_ref()?;
        if now - estimator.last_time() > window {
            return None;
        }
        estimator.acceleration_mps2()
    }

    /// The course trend (VERT.3, I062/200 TRANS) from the IMM model
    /// probabilities: a coordinated-turn model must carry a dominant share
    /// ([`TURN_PROBABILITY_THRESHOLD`]) before the track is called turning.
    /// A mathematically **positive** turn rate (anticlockwise in the
    /// east/north plane) is a **left** turn. A bank without turn models can
    /// determine nothing.
    pub fn course_trend(&self) -> CourseTrend {
        let mut left = 0.0;
        let mut right = 0.0;
        let mut has_turn_model = false;
        for (model, mu) in self.imm.models().iter().zip(self.imm.probabilities()) {
            if let crate::MotionModel::CoordinatedTurn { rate } = model {
                if rate.abs() > f64::EPSILON {
                    has_turn_model = true;
                    if *rate > 0.0 {
                        left += mu;
                    } else {
                        right += mu;
                    }
                }
            }
        }
        if !has_turn_model {
            return CourseTrend::Undetermined;
        }
        if left > TURN_PROBABILITY_THRESHOLD && left >= right {
            CourseTrend::LeftTurn
        } else if right > TURN_PROBABILITY_THRESHOLD {
            CourseTrend::RightTurn
        } else {
            CourseTrend::Constant
        }
    }

    /// The full mode of movement (VERT.3, I062/200) at `now`: course from
    /// the IMM turn-model probabilities, groundspeed trend from the
    /// along-track component of the acceleration estimate, vertical trend
    /// from the vertical filter's rate — each axis `Undetermined` where the
    /// tracker cannot honestly tell.
    pub fn mode_of_movement(&self, now: f64, window: f64) -> ModeOfMovement {
        let course = self.course_trend();

        let speed = match self.acceleration_mps2(now, window) {
            Some((ax, ay)) => {
                let v = self.imm.combined_estimate().velocity();
                let speed_mps = v[0].hypot(v[1]);
                if speed_mps < SPEED_TREND_MIN_SPEED_MPS {
                    SpeedTrend::Undetermined
                } else {
                    let along = (ax * v[0] + ay * v[1]) / speed_mps;
                    if along > SPEED_TREND_THRESHOLD_MPS2 {
                        SpeedTrend::Increasing
                    } else if along < -SPEED_TREND_THRESHOLD_MPS2 {
                        SpeedTrend::Decreasing
                    } else {
                        SpeedTrend::Constant
                    }
                }
            }
            None => SpeedTrend::Undetermined,
        };

        let vertical = match self.vertical_estimate(now, window) {
            Some((_, rocd_ft_min)) => {
                if rocd_ft_min > VERTICAL_TREND_THRESHOLD_FT_MIN {
                    VerticalTrend::Climb
                } else if rocd_ft_min < -VERTICAL_TREND_THRESHOLD_FT_MIN {
                    VerticalTrend::Descent
                } else {
                    VerticalTrend::Level
                }
            }
            None => VerticalTrend::Undetermined,
        };

        ModeOfMovement {
            course,
            speed,
            vertical,
        }
    }

    /// Sensors that contributed a hit in the most recent scan.
    pub fn contributing_sensors(&self) -> &BTreeSet<SensorId> {
        &self.contributing_sensors
    }

    /// Data time of the last ADS-B hit, seconds, if any.
    pub fn adsb_last_hit_time(&self) -> Option<f64> {
        self.source_hits.adsb
    }

    /// Per-technology last-hit times for this track (ADR 0027), driving the
    /// I062/290 ages and the derived provenance.
    pub(crate) fn source_hits(&self) -> SourceHits {
        self.source_hits
    }

    /// Clear the contributing-sensor set at the start of a new scan; sensors
    /// that hit this track again will re-add themselves via
    /// [`Track::record_hit_from`].
    pub(crate) fn reset_contributing_sensors(&mut self) {
        self.contributing_sensors.clear();
    }

    /// Record that `sensor` contributed a hit (founded or updated this track)
    /// in the current scan, at data time `time`. Besides the per-scan
    /// contributing set this books the sensor's last-hit time (monotonic per
    /// sensor), which drives the mono/multisensor status (FR-TRK-036).
    pub(crate) fn record_hit_from(&mut self, sensor: SensorId, time: f64) {
        self.contributing_sensors.insert(sensor);
        let last = self.sensor_hits.entry(sensor).or_insert(time);
        if time > *last {
            *last = time;
        }
    }

    /// How many **distinct sensors** have hit this track within the last
    /// `window` seconds before `now` (data time). `<= 1` means the track is
    /// currently *monosensor*: no second source cross-checks the estimate
    /// (CAT062 I062/080 MON, FR-TRK-036). A long-coasting track counts `0`.
    pub(crate) fn fresh_sensor_count(&self, now: f64, window: f64) -> usize {
        self.sensor_hits
            .values()
            .filter(|&&hit| now - hit <= window)
            .count()
    }

    /// Whether the most recent associated report carried the SPI pulse.
    pub fn spi(&self) -> bool {
        self.spi
    }

    /// Record a per-technology contribution (ADR 0027): a hit of `source` at data
    /// time `time`, with `has_primary` flagging a combined dwell that also feeds
    /// PSR. Updates the per-source ages behind I062/290 and the provenance.
    pub(crate) fn record_source_hit(&mut self, source: SourceKind, time: f64, has_primary: bool) {
        self.source_hits.record(source, time, has_primary);
    }

    /// Absorb the SSR-derived attributes (if any) of an associated plot: the
    /// identity (Mode 3/A, Mode S address) and the measured flight level.
    ///
    /// Sticky: a present value overwrites the stored one, but a `None` (e.g.
    /// from a primary-only detection) leaves the last known value in place —
    /// losing one SSR reply should not erase what we already know. (The flight
    /// level is "sticky" in the same sense; it still tracks climbs/descents
    /// because every Mode C reply overwrites it.)
    pub(crate) fn update_identity(&mut self, mode_ac: &ModeAC, time: f64) {
        // SPI is the one deliberately NON-sticky attribute: it describes the
        // last reply only, so every associated plot overwrites it — set and
        // cleared alike (FR-TRK-036).
        self.spi = mode_ac.spi;
        if mode_ac.mode_3a.is_some() {
            self.mode_3a = mode_ac.mode_3a;
        }
        if mode_ac.icao_address.is_some() {
            self.icao_address = mode_ac.icao_address;
        }
        if mode_ac.flight_level_ft.is_some() {
            self.flight_level_ft = mode_ac.flight_level_ft;
        }
        if mode_ac.callsign.is_some() {
            self.callsign = mode_ac.callsign;
        }
        // DAPs merge per field (different BDS registers arrive in different
        // reports); the report's data time stamps the freshness basis
        // (FR-TRK-040).
        if !mode_ac.daps.is_empty() {
            self.daps.merge_from(&mode_ac.daps);
            self.daps_time = Some(self.daps_time.map_or(time, |prev| prev.max(time)));
        }
        // Vertical chain (VERT.2): every Mode-C/FL report feeds the vertical
        // filter (which gates its own outliers); a genuinely geometric height
        // updates the separate geometric EWMA. Deliberately per-report, not
        // sticky — freshness is judged at read time.
        if let Some(level_ft) = mode_ac.flight_level_ft {
            match &mut self.vertical {
                Some(filter) => filter.update(level_ft, time),
                None => {
                    self.vertical = Some(crate::vertical::VerticalFilter::from_first_measurement(
                        level_ft, time,
                    ))
                }
            }
        }
        if let Some(geo_ft) = mode_ac.geometric_height_ft {
            self.geometric_altitude_ft = Some(match self.geometric_altitude_ft {
                Some(prev) => prev + GEOMETRIC_EWMA_ALPHA * (geo_ft - prev),
                None => geo_ft,
            });
            self.geometric_time = Some(time);
        }
        // Acceleration chain (VERT.3): sample the IMM combined velocity on
        // every associated report; the estimator differentiates and smooths.
        let v = self.imm.combined_estimate().velocity();
        match &mut self.acceleration {
            Some(estimator) => estimator.update(v[0], v[1], time),
            None => {
                self.acceleration = Some(
                    crate::acceleration::AccelerationEstimator::from_first_sample(v[0], v[1], time),
                )
            }
        }
    }

    /// The track's DAPs, but only while **fresh**: the most recent
    /// DAP-carrying report lies within `window` seconds of `now`. Stale
    /// intent data is worse than none — an hour-old selected altitude shown
    /// as current would mislead the controller — so staleness yields the
    /// empty set and the wire omits the subfields (FR-TRK-040).
    pub fn fresh_daps(&self, now: f64, window: f64) -> Daps {
        match self.daps_time {
            Some(t) if now - t <= window => self.daps,
            _ => Daps::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imm::ImmConfig;
    use crate::kalman::LinearKalman;
    use crate::measurement::{convert_plot, SensorErrorModel};
    use firefly_geo::Polar;

    fn fresh_track() -> Track {
        let model = SensorErrorModel::from_range_and_azimuth_deg(50.0, 0.08);
        let measurement = convert_plot(&Polar::new(50_000.0, 0.0, 0.0), &model);
        let filter = LinearKalman::from_first_measurement(&measurement, 200.0);
        let imm = ImmConfig::cv_and_turns(0.052).seed(filter);
        Track::new(TrackId(1), 1, imm, 0.0)
    }

    /// A fresh track has no known identity yet.
    /// REQ: FR-TRK-009
    #[test]
    fn fresh_track_has_no_identity() {
        let track = fresh_track();
        assert_eq!(track.mode_3a(), None);
        assert_eq!(track.icao_address(), None);
    }

    /// An SSR reply on an associated plot is absorbed into the track.
    /// REQ: FR-TRK-009
    #[test]
    fn identity_is_absorbed_from_ssr_reply() {
        let mut track = fresh_track();
        track.update_identity(
            &ModeAC {
                mode_3a: Some(0o2613),
                flight_level_ft: Some(35_000.0),
                icao_address: Some(0x0040_0123),
                callsign: Some(Callsign::new("DLH123")),
                spi: false,
                geometric_height_ft: None,
                daps: Daps::default(),
            },
            0.0,
        );
        assert_eq!(track.mode_3a(), Some(0o2613));
        assert_eq!(track.icao_address(), Some(0x0040_0123));
        assert_eq!(track.callsign(), Some(Callsign::new("DLH123")));
    }

    /// DAPs from different BDS registers (arriving in different reports)
    /// merge per field; a DAP-less report does not clear them; and once the
    /// last DAP-carrying report ages beyond the freshness window they are
    /// withheld — stale intent data shown as current would mislead.
    /// REQ: FR-TRK-040
    #[test]
    fn daps_merge_across_reports_and_report_only_while_fresh() {
        let mut track = fresh_track();

        let mut bds40 = ModeAC::default();
        bds40.daps.selected_altitude_ft = Some(35_008.0);
        track.update_identity(&bds40, 100.0);

        let mut bds60 = ModeAC::default();
        bds60.daps.magnetic_heading_deg = Some(270.0);
        track.update_identity(&bds60, 105.0);

        let fresh = track.fresh_daps(110.0, 30.0);
        assert_eq!(
            fresh.selected_altitude_ft,
            Some(35_008.0),
            "merged across reports"
        );
        assert_eq!(fresh.magnetic_heading_deg, Some(270.0));

        // A DAP-less report (e.g. a plain SSR reply) does not clear them…
        track.update_identity(&ModeAC::default(), 120.0);
        assert_eq!(
            track.fresh_daps(125.0, 30.0).selected_altitude_ft,
            Some(35_008.0)
        );

        // …but staleness withholds the whole set.
        assert!(track.fresh_daps(200.0, 30.0).is_empty(), "stale → withheld");
    }

    /// The vertical chain (VERT.2): Mode-C reports feed the filter (altitude
    /// and rate reported while fresh, withheld when stale) and geometric
    /// heights the separate EWMA, never mixed with the barometric value.
    /// REQ: FR-TRK-042
    #[test]
    fn vertical_chain_reports_fresh_and_withholds_stale() {
        let mut track = fresh_track();

        // A steady climb at 50 ft/s across six Mode-C reports.
        for i in 0..6 {
            let t = i as f64 * 5.0;
            let reply = ModeAC {
                flight_level_ft: Some(10_000.0 + 50.0 * t),
                ..ModeAC::default()
            };
            track.update_identity(&reply, t);
        }
        let (altitude_ft, rocd) = track.vertical_estimate(25.0, 30.0).expect("fresh");
        assert!(
            (altitude_ft - 11_250.0).abs() < 150.0,
            "filtered altitude tracks the climb, got {altitude_ft}"
        );
        assert!(rocd > 1_000.0, "climb visible in the rate, got {rocd}");

        // A genuinely geometric height feeds the separate EWMA…
        let geo = ModeAC {
            geometric_height_ft: Some(11_600.0),
            ..ModeAC::default()
        };
        track.update_identity(&geo, 26.0);
        assert_eq!(track.geometric_altitude_ft(27.0, 30.0), Some(11_600.0));
        // …and does NOT disturb the barometric filter (different reference).
        let (after_geo, _) = track.vertical_estimate(27.0, 30.0).unwrap();
        assert!((after_geo - altitude_ft).abs() < 150.0);

        // Staleness withholds both — absence over a stale claim.
        assert!(track.vertical_estimate(100.0, 30.0).is_none());
        assert!(track.geometric_altitude_ft(100.0, 30.0).is_none());
    }

    /// The mode of movement (VERT.3) reads honestly off the chains: a fresh
    /// track with balanced IMM priors is "constant course", the vertical
    /// trend follows the vertical filter, and axes without evidence stay
    /// undetermined. REQ: FR-TRK-043
    #[test]
    fn mode_of_movement_derives_from_the_chains() {
        let mut track = fresh_track();

        // Balanced turn priors (µ_left = µ_right < 0.5) ⇒ constant course.
        assert_eq!(track.course_trend(), CourseTrend::Constant);

        // No acceleration samples, no Mode-C yet ⇒ both axes undetermined,
        // and an all-undetermined course still yields a struct the caller
        // can filter on.
        let before = track.mode_of_movement(0.0, 30.0);
        assert_eq!(before.speed, SpeedTrend::Undetermined);
        assert_eq!(before.vertical, VerticalTrend::Undetermined);

        // A steady climb in Mode-C makes the vertical trend read "climb"…
        for i in 0..6 {
            let t = i as f64 * 5.0;
            let reply = ModeAC {
                flight_level_ft: Some(10_000.0 + 50.0 * t),
                ..ModeAC::default()
            };
            track.update_identity(&reply, t);
        }
        let mom = track.mode_of_movement(25.0, 30.0);
        assert_eq!(mom.vertical, VerticalTrend::Climb);
        assert!(mom.is_determined());

        // …and staleness pulls it back to undetermined.
        assert_eq!(
            track.mode_of_movement(100.0, 30.0).vertical,
            VerticalTrend::Undetermined
        );
    }

    /// A primary-only plot (no SSR reply) does not erase a previously known
    /// identity — losing one reply should not wipe out what we already know.
    /// REQ: FR-TRK-009
    #[test]
    fn missing_ssr_reply_does_not_clear_known_identity() {
        let mut track = fresh_track();
        track.update_identity(
            &ModeAC {
                mode_3a: Some(0o2613),
                flight_level_ft: None,
                icao_address: Some(0x0040_0123),
                callsign: Some(Callsign::new("DLH123")),
                spi: false,
                geometric_height_ft: None,
                daps: Daps::default(),
            },
            0.0,
        );

        track.update_identity(&ModeAC::default(), 0.0);

        assert_eq!(track.mode_3a(), Some(0o2613), "squawk stays sticky");
        assert_eq!(
            track.icao_address(),
            Some(0x0040_0123),
            "ICAO address stays sticky"
        );
        assert_eq!(
            track.callsign(),
            Some(Callsign::new("DLH123")),
            "callsign stays sticky"
        );
    }

    /// A new SSR reply overwrites the previously known identity (e.g. the
    /// pilot was assigned a new squawk).
    /// REQ: FR-TRK-009
    #[test]
    fn new_ssr_reply_overwrites_known_identity() {
        let mut track = fresh_track();
        track.update_identity(
            &ModeAC {
                mode_3a: Some(0o2613),
                flight_level_ft: None,
                icao_address: Some(0x0040_0123),
                callsign: None,
                spi: false,
                geometric_height_ft: None,
                daps: Daps::default(),
            },
            0.0,
        );
        track.update_identity(
            &ModeAC {
                mode_3a: Some(0o7000),
                flight_level_ft: None,
                icao_address: Some(0x0040_0123),
                callsign: None,
                spi: false,
                geometric_height_ft: None,
                daps: Daps::default(),
            },
            0.0,
        );

        assert_eq!(track.mode_3a(), Some(0o7000));
    }

    /// SPI is transient, not sticky: every associated reply overwrites it, so
    /// it describes the *last* report only — while the identity fields stay
    /// sticky through the same update. REQ: FR-TRK-036
    #[test]
    fn spi_is_transient_not_sticky() {
        let mut track = fresh_track();
        track.update_identity(
            &ModeAC {
                mode_3a: Some(0o2613),
                flight_level_ft: None,
                icao_address: None,
                callsign: None,
                spi: true,
                geometric_height_ft: None,
                daps: Daps::default(),
            },
            0.0,
        );
        assert!(track.spi(), "SPI set by the reply that carried it");

        track.update_identity(&ModeAC::default(), 0.0);
        assert!(!track.spi(), "next reply without SPI clears it");
        assert_eq!(track.mode_3a(), Some(0o2613), "identity stays sticky");
    }

    /// The per-sensor hit book answers "how many distinct sensors support this
    /// track right now?" over a freshness window — the basis of the I062/080
    /// MON bit. REQ: FR-TRK-036
    #[test]
    fn fresh_sensor_count_windows_per_sensor_hits() {
        let mut track = fresh_track();
        track.record_hit_from(SensorId(1), 0.0);
        track.record_hit_from(SensorId(2), 5.0);
        assert_eq!(track.fresh_sensor_count(10.0, 30.0), 2, "both fresh");
        assert_eq!(
            track.fresh_sensor_count(32.0, 30.0),
            1,
            "sensor 1 aged out (hit at 0 → age 32 > 30); sensor 2 still fresh"
        );
        assert_eq!(track.fresh_sensor_count(100.0, 30.0), 0, "all stale");
    }

    /// The time-continuous lifecycle interval (Häppchen 13.2) falls back to the
    /// nominal until a second hit establishes the track's own revisit, then
    /// follows the measured cadence. REQ: FR-TRK-023
    #[test]
    fn expected_revisit_falls_back_to_nominal_until_established() {
        let mut track = fresh_track(); // one founding hit at t = 0, no gap yet
        assert_eq!(
            track.expected_revisit(5.0),
            5.0,
            "no second hit yet → nominal fallback"
        );

        track.mark_hit(4.0); // first inter-hit gap = 4 s → revisit established
        assert_eq!(
            track.expected_revisit(5.0),
            4.0,
            "own measured revisit takes over once known"
        );
    }
}
