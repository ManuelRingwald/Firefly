use firefly_geo::{LocalFrame, Polar, Wgs84};
use serde::{Deserialize, Serialize};

use crate::ids::SensorId;
use crate::time::Timestamp;

/// Which radar channel produced a detection.
///
/// A *primary* radar (PSR) sees a skin reflection — it gives position but no
/// identity or barometric height. A *secondary* radar (SSR) interrogates a
/// transponder and gets Mode A/C/S replies. In practice a plot is often the
/// *combined* result of correlating a primary return with an SSR reply in the
/// same beam dwell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionKind {
    /// Primary surveillance radar only (skin paint).
    Primary,
    /// Secondary surveillance radar only (transponder reply).
    Secondary,
    /// Correlated primary + secondary in the same dwell.
    Combined,
}

impl DetectionKind {
    pub fn has_primary(self) -> bool {
        matches!(self, DetectionKind::Primary | DetectionKind::Combined)
    }

    pub fn has_secondary(self) -> bool {
        matches!(self, DetectionKind::Secondary | DetectionKind::Combined)
    }
}

/// Which **surveillance technology** produced a plot (ADR 0027).
///
/// [`DetectionKind`] describes the radar-dwell correlation (primary/secondary);
/// `SourceKind` is orthogonal and names the *technology family*, so a fused track
/// can report its honest per-technology provenance (PSR / SSR / Mode S / ADS-B /
/// FLARM) instead of the consumer guessing from field presence. The cooperative
/// geodetic sources (ADS-B vs FLARM) are otherwise indistinguishable — both are
/// `Measurement::Geodetic` + `DetectionKind::Secondary` — so the producer tags
/// them explicitly here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SourceKind {
    /// Primary surveillance radar skin paint.
    #[default]
    Psr,
    /// Secondary surveillance radar Mode A/C reply (no Mode S address).
    Ssr,
    /// Mode S selective interrogation reply (carries a 24-bit ICAO address).
    ModeS,
    /// ADS-B (1090 Extended Squitter) geodetic self-report.
    AdsB,
    /// FLARM / Open Glider Network geodetic report (ADR 0026).
    Flarm,
}

/// An aircraft callsign / flight identification (Mode S "target identification").
///
/// Stored as up to 8 ASCII characters, space-padded — the same shape as the
/// wire representation in CAT062 I062/245 (8 × 6-bit IA-5 characters).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Callsign(pub [u8; 8]);

impl Callsign {
    /// Build a callsign from a string, space-padding or truncating to 8 ASCII characters.
    pub fn new(s: &str) -> Self {
        let mut bytes = [b' '; 8];
        for (i, b) in s.bytes().take(8).enumerate() {
            bytes[i] = b;
        }
        Callsign(bytes)
    }

    /// The callsign as a string, with trailing spaces trimmed.
    pub fn as_str(&self) -> &str {
        let len = self.0.iter().rposition(|&b| b != b' ').map_or(0, |i| i + 1);
        std::str::from_utf8(&self.0[..len]).unwrap_or("")
    }
}

/// **Downlink Aircraft Parameters** (DAPs) from Mode S EHS BDS registers
/// (FEP.2): what the aircraft itself reports about its state and *intent*.
/// A Mode S EHS radar interrogates the transponder's BDS 4,0 / 5,0 / 6,0
/// registers and delivers them in CAT048 I048/250; the decoder populates only
/// fields whose **status bit** the transponder set — every field here is
/// individually optional, and `None` means "not validly reported", never 0.
///
/// The operational crown jewel is `selected_altitude_ft` (BDS 4,0): the level
/// dialled into the autopilot — the basis of level-bust detection (does the
/// crew's intent match the clearance, *before* the aircraft moves?).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Daps {
    /// BDS 4,0 — MCP/FCU selected altitude, feet: what is dialled into the
    /// autopilot.
    pub selected_altitude_ft: Option<f64>,
    /// BDS 5,0 — roll angle, degrees (positive = right bank).
    pub roll_angle_deg: Option<f64>,
    /// BDS 5,0 — true track angle, degrees [0, 360).
    pub true_track_deg: Option<f64>,
    /// BDS 5,0 — ground speed, knots.
    pub ground_speed_kt: Option<f64>,
    /// BDS 5,0 — true airspeed, knots.
    pub true_airspeed_kt: Option<f64>,
    /// BDS 6,0 — magnetic heading, degrees [0, 360).
    pub magnetic_heading_deg: Option<f64>,
    /// BDS 6,0 — indicated airspeed, knots.
    pub ias_kt: Option<f64>,
    /// BDS 6,0 — Mach number.
    pub mach: Option<f64>,
    /// BDS 6,0 — barometric altitude rate, feet per minute.
    pub barometric_vertical_rate_ft_min: Option<f64>,
}

impl Daps {
    /// True when no field carries a value.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Per-field merge: every field `newer` carries overwrites this one's —
    /// fields `newer` lacks keep their previous value. Different BDS
    /// registers arrive in different reports; merging keeps the freshest
    /// valid value of each parameter without one register wiping another.
    pub fn merge_from(&mut self, newer: &Daps) {
        macro_rules! merge {
            ($($field:ident),+) => {
                $(if newer.$field.is_some() { self.$field = newer.$field; })+
            };
        }
        merge!(
            selected_altitude_ft,
            roll_angle_deg,
            true_track_deg,
            ground_speed_kt,
            true_airspeed_kt,
            magnetic_heading_deg,
            ias_kt,
            mach,
            barometric_vertical_rate_ft_min
        );
    }
}

/// Secondary-radar replies attached to a plot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ModeAC {
    /// Mode 3/A identity code (the 4-digit octal "squawk"), if replied.
    pub mode_3a: Option<u16>,
    /// Mode C pressure altitude in feet (barometric, 1013.25 hPa datum), if replied.
    pub flight_level_ft: Option<f64>,
    /// Mode S 24-bit ICAO aircraft address, if available.
    pub icao_address: Option<u32>,
    /// Mode S "target identification" (callsign / flight ID), if available.
    pub callsign: Option<Callsign>,
    /// **SPI** — Special Position Identification: the transponder's "ident"
    /// pulse, pressed by the pilot on controller request. Deliberately
    /// **transient** (unlike the sticky identity fields above): it describes
    /// *this* reply, and the track carries it only until the next update
    /// (CAT062 I062/080 SPI = "present in the last report"). Today only the
    /// CAT048 radar path delivers it (I048/020); the ADS-B/FLARM adapters
    /// leave it `false`. `serde(default)` keeps pre-SPI `.ffplots` readable.
    #[serde(default)]
    pub spi: bool,
    /// **Geometric** (WGS-84) height in feet, if the source genuinely
    /// measured one (VERT.2): ADS-B I021/140, MLAT I020/105 — never a
    /// barometric value smuggled in under a geometric label. Kept separate
    /// from `flight_level_ft` because the two use different references and
    /// must never be mixed in the vertical chain. `serde(default)` keeps
    /// older `.ffplots` readable.
    #[serde(default)]
    pub geometric_height_ft: Option<f64>,
    /// Downlink Aircraft Parameters from Mode S EHS (FEP.2). Populated only
    /// by the CAT048 radar path (I048/250, BDS 4,0/5,0/6,0); empty from the
    /// ADS-B/FLARM adapters. `serde(default)` keeps older `.ffplots` readable.
    #[serde(default)]
    pub daps: Daps,
}

/// Where a plot's position measurement comes from, and in which frame it lives.
///
/// A *radar* detection is **polar** in the sensor's own frame (range, azimuth,
/// elevation): converting it into the tracking frame needs the sensor's geometry
/// and a range-vs-angle noise model (the "cigar"). An *ADS-B* / Mode S Extended
/// Squitter report is the aircraft's **own** WGS84 position — already
/// world-referenced — with a self-declared accuracy derived from its NACp
/// (Navigation Accuracy Category — Position). It needs no polar conversion and
/// carries an isotropic position uncertainty, not a tilted radar ellipse.
///
/// Keeping the source in the plot (rather than splitting into two plot types)
/// lets one association/fusion path serve both: the tracker dispatches on the
/// variant only when turning the plot into a Cartesian measurement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Measurement {
    /// Radar polar reading (range/azimuth/elevation) in the sensor's local frame.
    Polar(Polar),
    /// ADS-B geodetic self-report: WGS84 position plus its 1σ horizontal
    /// position accuracy (metres), derived from the transmitter's NACp.
    Geodetic {
        /// Reported WGS84 position.
        position: Wgs84,
        /// 1σ horizontal position accuracy, metres.
        sigma_pos_m: f64,
    },
}

impl Measurement {
    /// The geodetic position of this measurement. A polar reading is lifted out
    /// of the given sensor `frame`; an ADS-B report is already geodetic, so the
    /// frame is ignored.
    pub fn to_wgs84(&self, frame: &LocalFrame) -> Wgs84 {
        match self {
            Measurement::Polar(p) => frame.enu_to_geodetic(&p.to_enu()),
            Measurement::Geodetic { position, .. } => *position,
        }
    }
}

/// A single detection ("plot") for one target on one scan of one sensor.
///
/// The measurement is stored in its native form ([`Measurement`]); converting it
/// into the tracking frame is the tracker's job, because the choice of frame and
/// the measurement-noise model belong to the estimator, not to the detection.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Plot {
    /// Sensor that produced this plot.
    pub sensor: SensorId,
    /// Time of detection.
    pub time: Timestamp,
    /// Measured position and its native frame/source.
    pub measurement: Measurement,
    /// What kind of detection this is.
    pub kind: DetectionKind,
    /// Which surveillance technology produced this plot (ADR 0027). Defaults to
    /// [`SourceKind::Psr`] for backward compatibility with `.ffplots` files
    /// recorded before this field existed (ADR 0020 / FR-OPS-006).
    #[serde(default)]
    pub source: SourceKind,
    /// Secondary-radar data, present when [`DetectionKind::has_secondary`].
    pub mode_ac: ModeAC,
}

impl Plot {
    /// Construct a bare primary radar plot (polar, no SSR data).
    pub fn primary(sensor: SensorId, time: Timestamp, measurement: Polar) -> Self {
        Self {
            sensor,
            time,
            measurement: Measurement::Polar(measurement),
            kind: DetectionKind::Primary,
            source: SourceKind::Psr,
            mode_ac: ModeAC::default(),
        }
    }

    /// Construct an ADS-B plot: a geodetic self-report from a transponder with
    /// its NACp-derived 1σ position accuracy and Mode S identity. ADS-B is a
    /// secondary (transponder) source — never a primary skin paint — so its
    /// detection kind is [`DetectionKind::Secondary`].
    pub fn adsb(
        sensor: SensorId,
        time: Timestamp,
        position: Wgs84,
        sigma_pos_m: f64,
        mode_ac: ModeAC,
    ) -> Self {
        Self::geodetic(
            sensor,
            time,
            position,
            sigma_pos_m,
            mode_ac,
            SourceKind::AdsB,
        )
    }

    /// Construct a FLARM/OGN plot (ADR 0026/0027): like [`Plot::adsb`] a
    /// cooperative geodetic self-report (secondary, isotropic accuracy), but
    /// tagged [`SourceKind::Flarm`] so the tracker reports its provenance
    /// honestly rather than confusing it with ADS-B.
    pub fn flarm(
        sensor: SensorId,
        time: Timestamp,
        position: Wgs84,
        sigma_pos_m: f64,
        mode_ac: ModeAC,
    ) -> Self {
        Self::geodetic(
            sensor,
            time,
            position,
            sigma_pos_m,
            mode_ac,
            SourceKind::Flarm,
        )
    }

    /// Shared constructor for a cooperative **geodetic** self-report (ADS-B or
    /// FLARM): `Measurement::Geodetic`, `DetectionKind::Secondary`, with an
    /// explicit [`SourceKind`].
    pub fn geodetic(
        sensor: SensorId,
        time: Timestamp,
        position: Wgs84,
        sigma_pos_m: f64,
        mode_ac: ModeAC,
        source: SourceKind,
    ) -> Self {
        Self {
            sensor,
            time,
            measurement: Measurement::Geodetic {
                position,
                sigma_pos_m,
            },
            kind: DetectionKind::Secondary,
            source,
            mode_ac,
        }
    }
}
