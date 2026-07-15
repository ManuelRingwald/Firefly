//! The neutral, geodetic output of the tracker: a `SystemTrack`.
//!
//! Internally the tracker reasons in a flat, sensor-local ENU frame (east/north
//! in metres) because there motion is almost linear and the Kalman maths is
//! cheap. But the *outside world* — an air-situation display, and later the
//! Phoenix ASD via ASTERIX CAT062 — speaks **geodetic WGS84** (latitude,
//! longitude). The `SystemTrack` is the boundary type that carries an estimated
//! track out of the core in world coordinates.
//!
//! It is deliberately *format- and transport-neutral* (Ports & Adapters,
//! NFR-INT-001/002): the core produces `SystemTrack`s; a downstream **adapter**
//! turns them into CAT062, JSON for the web map, or anything else. No transport
//! or wire format leaks into the core.
//!
//! Note on velocity: the components stay in the local ENU frame (`v_east`,
//! `v_north`, m/s). Over the short ranges of a single radar the local east/north
//! axes are an excellent approximation of the geographic ones, and ground speed
//! and heading derive directly from them — see [`SystemTrack::ground_speed`] and
//! [`SystemTrack::track_angle`].

use serde::{Deserialize, Serialize};

use crate::ids::{SensorId, TrackId};
use crate::plot::{Callsign, Daps};
use crate::time::Timestamp;
use firefly_geo::Wgs84;

/// A technology counts as *fresh* for provenance when its update age is within
/// this many seconds (ADR 0027) — matches the ADS-B freshness Wayfinder already
/// uses for its badge.
pub const PROVENANCE_FRESH_S: f64 = 30.0;

/// Per-technology update ages of a track, seconds (ADR 0027). `Some(age)` once a
/// plot of that technology has contributed (age measured at the report time);
/// `None` otherwise. Drives the CAT062 I062/290 per-source age subfields and the
/// derived [`Provenance`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceAges {
    /// Primary surveillance radar.
    pub psr: Option<f64>,
    /// Secondary surveillance radar (Mode A/C).
    pub ssr: Option<f64>,
    /// Mode S selective interrogation.
    pub mode_s: Option<f64>,
    /// ADS-B (1090 Extended Squitter).
    pub adsb: Option<f64>,
    /// FLARM / Open Glider Network (ADR 0026).
    pub flarm: Option<f64>,
}

/// The dominant surveillance provenance of a track, **derived** from
/// [`SourceAges`] (ADR 0027): the technology that most recently contributed, or
/// `Combined` when several are fresh together. Authoritative replacement for the
/// consumer-side heuristic (Wayfinder `provenance.js`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// No technology age recorded yet.
    Unknown,
    /// Primary radar only.
    Psr,
    /// Secondary radar (Mode A/C).
    Ssr,
    /// Mode S.
    ModeS,
    /// ADS-B. Serialises as the conventional token `"adsb"`.
    #[serde(rename = "adsb")]
    AdsB,
    /// FLARM.
    Flarm,
    /// Two or more technologies fresh together (true fusion).
    Combined,
}

/// Transversal (course) trend of a track — I062/200 TRANS (VERT.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CourseTrend {
    /// Constant course (straight flight).
    Constant,
    /// Turning right (clockwise over ground).
    RightTurn,
    /// Turning left (anticlockwise over ground).
    LeftTurn,
    /// No determination possible.
    Undetermined,
}

/// Longitudinal (groundspeed) trend of a track — I062/200 LONG (VERT.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SpeedTrend {
    /// Constant groundspeed.
    Constant,
    /// Increasing groundspeed.
    Increasing,
    /// Decreasing groundspeed.
    Decreasing,
    /// No determination possible.
    Undetermined,
}

/// Vertical trend of a track — I062/200 VERT (VERT.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum VerticalTrend {
    /// Level flight.
    Level,
    /// Climbing.
    Climb,
    /// Descending.
    Descent,
    /// No determination possible.
    Undetermined,
}

/// A reference to the flight plan a track is correlated with (FPL.1,
/// ADR 0038): the minimal field set a label/strip needs today; grows
/// additively as the EFS requirements land (Wayfinder #244).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightPlanRef {
    /// The plan's callsign — also the primary correlation key.
    pub callsign: String,
    /// Departure aerodrome (ICAO locator), when the plan carries it.
    #[serde(default)]
    pub departure: Option<String>,
    /// Destination aerodrome (ICAO locator), when the plan carries it.
    #[serde(default)]
    pub destination: Option<String>,
}

/// The track's **mode of movement** (I062/200): what the aircraft is doing in
/// each of the three axes, as far as the tracker can honestly tell —
/// `Undetermined` where it cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModeOfMovement {
    /// Course trend (constant / right turn / left turn).
    pub course: CourseTrend,
    /// Groundspeed trend (constant / increasing / decreasing).
    pub speed: SpeedTrend,
    /// Vertical trend (level / climb / descent).
    pub vertical: VerticalTrend,
}

impl ModeOfMovement {
    /// Whether at least one axis carries a determination — an all-
    /// undetermined mode says nothing and is not worth wire bytes.
    pub fn is_determined(&self) -> bool {
        self.course != CourseTrend::Undetermined
            || self.speed != SpeedTrend::Undetermined
            || self.vertical != VerticalTrend::Undetermined
    }
}

/// One track as reported to the outside world, in geodetic coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemTrack {
    /// Stable identity of the track.
    pub id: TrackId,
    /// The 16-bit **wire** track number (ASTERIX CAT062 I062/040). Unlike
    /// `id` — which is process-unique and never repeats — this lives in the
    /// managed 16-bit space of the wire contract: allocated at track birth,
    /// quarantined on deletion, and only then reusable (FR-TRK-035). Encoders
    /// must report *this* value, never a truncation of `id`, or the wire
    /// identity silently collides after 65 536 track births.
    #[serde(default)]
    pub track_number: u16,
    /// Data time of the estimate (the last scan that touched this track).
    pub time: Timestamp,
    /// Estimated position in geodetic WGS84.
    pub position: Wgs84,
    /// Eastward velocity component in the sensor-local frame, m/s.
    pub v_east: f64,
    /// Northward velocity component in the sensor-local frame, m/s.
    pub v_north: f64,
    /// Whether the track is confirmed (vs. still tentative). The neutral port
    /// reports both and leaves the publish/suppress policy to the adapter.
    /// Maps to the CNF bit of ASTERIX CAT062 I062/080.
    pub confirmed: bool,
    /// Whether the track is currently *coasting* — extrapolated from prediction
    /// because it had no fresh measurement this scan. Maps to the CST bit of
    /// CAT062 I062/080. A track may be both `confirmed` and `coasting`.
    pub coasting: bool,
    /// Whether the track is currently supported by **at most one** distinct
    /// sensor: no second source cross-checks the estimate (weaker against
    /// ghosts and sensor bias — the operator should see that). Derived from
    /// the per-sensor hit times within the provenance freshness window
    /// ([`PROVENANCE_FRESH_S`]); a long-coasting track (no fresh sensor at
    /// all) also reports `true`. Maps to the MON bit of CAT062 I062/080.
    #[serde(default)]
    pub monosensor: bool,
    /// Whether the most recent associated report carried the **SPI** pulse
    /// (the pilot's "ident", [`crate::ModeAC::spi`]). Transient, not sticky —
    /// it reflects the *last* report only. Maps to the SPI bit of CAT062
    /// I062/080.
    #[serde(default)]
    pub spi: bool,
    /// **Fresh** Downlink Aircraft Parameters (Mode S EHS, FEP.2): populated
    /// only while the track received DAP-carrying reports within the
    /// provenance freshness window — stale values are withheld rather than
    /// shown as current (the wire then omits the subfields, absence over a
    /// stale claim). Maps to CAT062 I062/380 MHG/SAL/IAR/MAC.
    #[serde(default)]
    pub daps: Daps,
    /// Whether this is the **final** report for the track — it has just been
    /// deleted from the tracker's live set, and this record carries its last
    /// known state once so a consumer can remove it deterministically instead
    /// of waiting for a timeout. Maps to the TSE (*Track Service End*) bit of
    /// CAT062 I062/080 (ADR 0016, FR-TRK-029). `false` for every live track;
    /// `true` appears exactly once, in the heartbeat following deletion.
    pub ended: bool,
    /// Update age: data-time since the last real measurement, seconds. `0` right
    /// after a hit, growing while coasting. Maps to CAT062 I062/290 (track ages).
    pub update_age: f64,
    /// Position uncertainty: the 1σ semi-major axis of the error ellipse, metres
    /// — the tracker's honest "how sure am I about this position right now".
    /// Maps to CAT062 I062/500 (estimated accuracies).
    pub position_uncertainty: f64,
    /// Most recently reported Mode 3/A code ("squawk"), if any SSR-equipped
    /// plot has ever associated with this track. `None` for a primary-only
    /// track. Encoded as CAT062 I062/060 when present (FR-TRK-009).
    pub mode_3a: Option<u16>,
    /// Most recently reported Mode S 24-bit ICAO aircraft address, if any
    /// SSR-equipped plot has ever associated with this track. `None` for a
    /// primary-only track. Encoded as the Target Address (ADR) subfield of
    /// CAT062 I062/380 when present, and is the eventual correlation key for
    /// multi-radar fusion.
    pub icao_address: Option<u32>,
    /// Most recently reported barometric flight level, in **feet** (Mode C
    /// pressure altitude, 1013.25 hPa datum), if any SSR-equipped plot has ever
    /// associated with this track. `None` for a primary-only track. Like the
    /// identity fields it is sticky — a primary-only detection does not clear
    /// the last known level. Encoded as CAT062 I062/136 (Measured Flight Level)
    /// when present. The tracker carries no independent vertical *estimate* yet
    /// (no vertical Kalman state); this is the last measured value, passed
    /// through (FR-TRK-027).
    pub flight_level_ft: Option<f64>,
    /// Most recently reported callsign / flight ID (Mode S target
    /// identification), if any SSR-equipped plot has ever associated with this
    /// track. `None` for a primary-only track. Sticky, like `mode_3a`: a
    /// primary-only detection does not clear the last known callsign. Encoded
    /// as CAT062 I062/245 (Target Identification) when present, passed through
    /// from the Mode S downlink reply (FR-TRK-028).
    pub callsign: Option<Callsign>,
    /// Sensors that contributed a hit to this track in the **most recent
    /// scan** (ADR 0010, central measurement fusion). Empty while coasting —
    /// no sensor saw it this scan. Sorted by [`SensorId`] for determinism.
    /// Replaces the single-sensor `update_age` simplification for CAT062
    /// I062/290 (track ages are reported per contributing sensor).
    pub contributing_sensors: Vec<SensorId>,
    /// Data-time elapsed since the last ADS-B (Extended Squitter) hit, seconds.
    /// `None` if the track has never been updated by an ADS-B measurement.
    /// When `Some`, drives the ES-Age subfield of CAT062 I062/290 (ICD 2.4.0,
    /// AP9.5/AP9.6); its presence signals "this track has an ADS-B contribution"
    /// to Wayfinder. Equals `source_ages.adsb` (kept as a back-compat alias).
    pub adsb_age_s: Option<f64>,
    /// Per-technology update ages (ADR 0027): the authoritative, source-resolved
    /// provenance data. Drives the I062/290 PSR/SSR/Mode-S/ES/FLARM age subfields
    /// and [`SystemTrack::provenance`]. Defaults to all-`None` for back-compat.
    #[serde(default)]
    pub source_ages: SourceAges,
    /// The **filtered** barometric altitude, feet (VERT.2). Pressure altitude
    /// (1013.25 hPa) as produced by the tracker; the output side may replace
    /// it with a QNH-corrected value and then sets
    /// [`barometric_qnh_corrected`](Self::barometric_qnh_corrected). Absent
    /// when the vertical filter has no fresh estimate. Encoded as CAT062
    /// I062/135 when present.
    #[serde(default)]
    pub barometric_altitude_ft: Option<f64>,
    /// Whether [`barometric_altitude_ft`](Self::barometric_altitude_ft) has
    /// been corrected to a **regional QNH** (I062/135 QNH bit). `false` means
    /// the value is an uncorrected pressure altitude — honest absence of a
    /// correction, never a silent standard-atmosphere claim.
    #[serde(default)]
    pub barometric_qnh_corrected: bool,
    /// The smoothed **geometric** (WGS-84) altitude, feet (VERT.2), from
    /// genuinely geometric source heights only. Absent without a fresh
    /// geometric contribution. Encoded as CAT062 I062/130 when present.
    #[serde(default)]
    pub geometric_altitude_ft: Option<f64>,
    /// Rate of climb/descent, ft/min, positive = climb (VERT.2). From the
    /// vertical filter; absent when it has no fresh estimate. Encoded as
    /// CAT062 I062/220 when present.
    #[serde(default)]
    pub rocd_ft_min: Option<f64>,
    /// Estimated horizontal acceleration `(a_east, a_north)`, m/s² (VERT.3).
    /// Absent without a fresh estimate. Encoded as CAT062 I062/210 when
    /// present.
    #[serde(default)]
    pub acceleration_mps2: Option<(f64, f64)>,
    /// The mode of movement (VERT.3), absent when nothing can be determined.
    /// Encoded as CAT062 I062/200 when present.
    #[serde(default)]
    pub mode_of_movement: Option<ModeOfMovement>,
    /// Whether another live track currently carries the same ICAO address or
    /// Mode 3/A code (SPEC.1, exported with FPL.1 — the correlation is its
    /// first consumer): a duplicated identity must never auto-correlate by
    /// code, and a display may flag it. Additive WS-JSON field; no CAT062
    /// impact.
    #[serde(default)]
    pub identity_conflict: bool,
    /// The correlated flight plan (FPL.1, ADR 0038), absent while the track
    /// is uncorrelated. Filled centrally at the output stage — one
    /// association for every consumer. Additive WS-JSON field; the wire item
    /// (I062/390) follows in FPL.2.
    #[serde(default)]
    pub flight_plan: Option<FlightPlanRef>,
}

impl SystemTrack {
    /// Horizontal ground speed, m/s — the length of the velocity vector.
    pub fn ground_speed(&self) -> f64 {
        self.v_east.hypot(self.v_north)
    }

    /// Track angle (course over ground): the direction of motion measured
    /// **clockwise from true north**, radians in `[0, 2π)`. This matches the
    /// azimuth convention used throughout the geo crate.
    ///
    /// For a target heading due east the angle is `π/2`; due north it is `0`.
    /// When the track is (numerically) stationary the angle is defined as `0`.
    pub fn track_angle(&self) -> f64 {
        if self.v_east == 0.0 && self.v_north == 0.0 {
            return 0.0;
        }
        let mut angle = self.v_east.atan2(self.v_north);
        if angle < 0.0 {
            angle += std::f64::consts::TAU;
        }
        angle
    }

    /// Derive the dominant [`Provenance`] from the per-technology ages (ADR 0027).
    /// A technology counts as *fresh* within [`PROVENANCE_FRESH_S`]; two or more
    /// fresh technologies → [`Provenance::Combined`]; otherwise the single fresh
    /// technology, or — if none is within the freshness window — the most recently
    /// seen one, or [`Provenance::Unknown`] if none has ever contributed.
    pub fn provenance(&self) -> Provenance {
        let a = &self.source_ages;
        let entries = [
            (Provenance::Psr, a.psr),
            (Provenance::Ssr, a.ssr),
            (Provenance::ModeS, a.mode_s),
            (Provenance::AdsB, a.adsb),
            (Provenance::Flarm, a.flarm),
        ];
        let fresh: Vec<Provenance> = entries
            .iter()
            .filter(|(_, age)| age.is_some_and(|s| s <= PROVENANCE_FRESH_S))
            .map(|(p, _)| *p)
            .collect();
        match fresh.as_slice() {
            [] => entries
                .iter()
                .filter_map(|(p, age)| age.map(|s| (*p, s)))
                .min_by(|x, y| x.1.total_cmp(&y.1))
                .map(|(p, _)| p)
                .unwrap_or(Provenance::Unknown),
            [single] => *single,
            _ => Provenance::Combined,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    fn track(v_east: f64, v_north: f64) -> SystemTrack {
        SystemTrack {
            id: TrackId(1),
            track_number: 1,
            time: Timestamp(0.0),
            position: Wgs84::from_degrees(47.0, 8.0, 500.0),
            v_east,
            v_north,
            confirmed: true,
            coasting: false,
            monosensor: false,
            spi: false,
            daps: Daps::default(),
            ended: false,
            update_age: 0.0,
            position_uncertainty: 0.0,
            mode_3a: None,
            icao_address: None,
            flight_level_ft: None,
            callsign: None,
            contributing_sensors: Vec::new(),
            adsb_age_s: None,
            source_ages: SourceAges::default(),
            barometric_altitude_ft: None,
            barometric_qnh_corrected: false,
            geometric_altitude_ft: None,
            rocd_ft_min: None,
            acceleration_mps2: None,
            mode_of_movement: None,
            identity_conflict: false,
            flight_plan: None,
        }
    }

    /// Ground speed is the Euclidean length of the velocity vector.
    /// REQ: NFR-INT-002
    #[test]
    fn ground_speed_is_vector_length() {
        assert!((track(3.0, 4.0).ground_speed() - 5.0).abs() < 1e-12);
    }

    /// Track angle follows the compass convention: clockwise from true north.
    /// REQ: NFR-INT-002
    #[test]
    fn track_angle_follows_compass_convention() {
        assert!((track(0.0, 100.0).track_angle()).abs() < 1e-12, "north → 0");
        assert!(
            (track(100.0, 0.0).track_angle() - FRAC_PI_2).abs() < 1e-12,
            "east → π/2"
        );
        assert!(
            (track(0.0, -100.0).track_angle() - PI).abs() < 1e-12,
            "south → π"
        );
        assert!(
            (track(-100.0, 0.0).track_angle() - (TAU - FRAC_PI_2)).abs() < 1e-12,
            "west → 3π/2"
        );
    }

    /// A (numerically) stationary track has a defined angle of zero.
    #[test]
    fn stationary_track_angle_is_zero() {
        assert_eq!(track(0.0, 0.0).track_angle(), 0.0);
    }

    fn track_with_ages(ages: SourceAges) -> SystemTrack {
        let mut t = track(0.0, 0.0);
        t.source_ages = ages;
        t
    }

    /// With no technology age recorded, provenance is `Unknown`. REQ: FR-TRK-034
    #[test]
    fn provenance_unknown_without_any_age() {
        assert_eq!(
            track_with_ages(SourceAges::default()).provenance(),
            Provenance::Unknown
        );
    }

    /// A single fresh technology yields exactly that provenance. REQ: FR-TRK-034
    #[test]
    fn provenance_single_fresh_technology() {
        assert_eq!(
            track_with_ages(SourceAges {
                adsb: Some(2.0),
                ..SourceAges::default()
            })
            .provenance(),
            Provenance::AdsB
        );
    }

    /// Two or more fresh technologies fuse to `Combined`. REQ: FR-TRK-034
    #[test]
    fn provenance_two_fresh_technologies_combine() {
        assert_eq!(
            track_with_ages(SourceAges {
                psr: Some(1.0),
                ssr: Some(2.0),
                ..SourceAges::default()
            })
            .provenance(),
            Provenance::Combined
        );
    }

    /// When nothing is within the freshness window, the most recently seen
    /// technology wins (no false `Combined`). REQ: FR-TRK-034
    #[test]
    fn provenance_falls_back_to_most_recent_when_all_stale() {
        let stale = PROVENANCE_FRESH_S + 10.0;
        assert_eq!(
            track_with_ages(SourceAges {
                psr: Some(stale + 5.0),
                flarm: Some(stale),
                ..SourceAges::default()
            })
            .provenance(),
            Provenance::Flarm,
            "flarm is the freshest of the stale set"
        );
    }

    /// A technology exactly at the freshness boundary still counts as fresh.
    /// REQ: FR-TRK-034
    #[test]
    fn provenance_boundary_age_counts_as_fresh() {
        assert_eq!(
            track_with_ages(SourceAges {
                ssr: Some(PROVENANCE_FRESH_S),
                ..SourceAges::default()
            })
            .provenance(),
            Provenance::Ssr
        );
    }
}
