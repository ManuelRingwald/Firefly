//! A per-sensor **spatial clutter map** (SPEC.2, ADR 0037).
//!
//! Radar false returns are not uniform: wind farms, road traffic, bird
//! migration, weather and terrain reflections concentrate in space. The
//! single global clutter density `λ` of [`crate::pda::ClutterModel`] makes
//! two errors at once — it *under*-states clutter in hotspots (false plots
//! there birth ghost tracks too easily and skew association) and
//! *over*-states it in clean regions (real plots are treated with undue
//! suspicion). This map learns the local density per polar cell
//! (range ring × azimuth sector) from the plots that associate with
//! nothing, and hands PDA/JPDA the **cell-local λ** instead.
//!
//! **Learning signal:** plots that fall in *no* track's gate (the initiation
//! candidates). A real aircraft contributes at most its first plot or two
//! before its track exists — with the slow time constant that washes out,
//! while persistent hotspots accumulate. That approximation is documented
//! (an honest limit), not hidden.
//!
//! **Estimator:** per cell an exponentially-forgotten event rate — on an
//! event at data time `t`: `rate ← rate·e^{−Δt/τ} + 1/τ` (events/second
//! with time constant `τ`). Pure event-driven updates, O(1) per plot, no
//! global decay ticks; deterministic in data time (ADR 0003) and
//! serialisable (snapshot/restore).
//!
//! The reported density converts the rate to the units PDA expects
//! (expected false returns per m² per scan): `rate · scan_period /
//! cell_area`, clamped to a sane band around the configured default so a
//! single stray plot can neither silence nor lock out association.
//!
//! **Exposure bookkeeping (SPEC.2b):** events alone cannot prove
//! cleanliness — only *watching without events* can. The map therefore
//! accrues credited observation time via [`ClutterMap::mark_active`]
//! (called once per processed sensor batch; feed outages credit at most
//! [`MAX_GAP_CREDIT_S`], so downtime never counts as evidence). Once a
//! cell has been under observation for [`MATURITY_S`] — since its first
//! event, or since map start for event-free cells — the density floor
//! drops from `default` to `0.1·default`: the region has demonstrably
//! produced (almost) no unassociated plots, and association may honestly
//! trust it. Immature evidence keeps the conservative `default` floor,
//! which is what protects knife-edge associations around the founding
//! plots of real aircraft (the SPEC.2 regression).
//!
//! REQ: FR-TRK-046

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Radial extent of one range ring, metres. Coarse on purpose: the map
/// models the *environment* (a wind farm, a motorway), not single targets.
const RANGE_CELL_M: f64 = 5_000.0;
/// Number of azimuth sectors (5.625° each at 64).
const AZIMUTH_SECTORS: u16 = 64;
/// Forgetting time constant τ, seconds. Long enough that one aircraft's
/// founding plot washes out, short enough to follow weather-driven clutter
/// within tens of minutes.
const TIME_CONSTANT_S: f64 = 600.0;
/// Clamp ceiling: a learned cell may claim at most this factor *more*
/// clutter than the default.
const MAX_DENSITY_FACTOR: f64 = 100.0;
/// Credited observation time (seconds) after which absence of events counts
/// as evidence and the floor may drop below the default — two forgetting
/// time constants of watching.
const MATURITY_S: f64 = 2.0 * TIME_CONSTANT_S;
/// The mature floor: a demonstrably quiet cell may claim at most this
/// fraction of the default density — never zero, so association keeps a
/// residual clutter hypothesis everywhere.
const MATURE_MIN_FACTOR: f64 = 0.1;
/// A single activity gap credits at most this much exposure (seconds):
/// a feed outage is not observation, so downtime never matures the map.
const MAX_GAP_CREDIT_S: f64 = 30.0;

/// One cell's exponentially-forgotten event rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Cell {
    /// Estimated event rate, events/second, valid at `last_time`.
    rate_per_s: f64,
    /// Data time of the last observed event.
    last_time: f64,
    /// The map's credited exposure at this cell's first event — its
    /// maturity clock starts here (SPEC.2b).
    #[serde(default)]
    born_observed_s: f64,
}

/// The spatial clutter map of one radar sensor.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClutterMap {
    /// Learned cells, keyed by `(range ring, azimuth sector)`. Cells the map
    /// has never seen an event in are simply absent — they report the
    /// caller's default density. Serialised as a pair list because JSON maps
    /// require string keys (snapshot/restore, NFR-CLOUD-003).
    #[serde(with = "cells_as_pairs")]
    cells: BTreeMap<(u32, u16), Cell>,
    /// Credited observation time in seconds (SPEC.2b): grows with sensor
    /// activity, capped per gap — the evidence base for claiming quiet.
    #[serde(default)]
    observed_s: f64,
    /// Data time of the last credited activity.
    #[serde(default)]
    last_activity: Option<f64>,
}

/// Serde helper: tuple-keyed map ↔ list of `(key, cell)` pairs.
mod cells_as_pairs {
    use super::Cell;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<S: Serializer>(
        map: &BTreeMap<(u32, u16), Cell>,
        ser: S,
    ) -> Result<S::Ok, S::Error> {
        let pairs: Vec<(&(u32, u16), &Cell)> = map.iter().collect();
        pairs.serialize(ser)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        de: D,
    ) -> Result<BTreeMap<(u32, u16), Cell>, D::Error> {
        let pairs = Vec::<((u32, u16), Cell)>::deserialize(de)?;
        Ok(pairs.into_iter().collect())
    }
}

impl ClutterMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// The cell key for a polar position.
    fn key(range_m: f64, azimuth_rad: f64) -> (u32, u16) {
        let ring = (range_m.max(0.0) / RANGE_CELL_M) as u32;
        let two_pi = std::f64::consts::TAU;
        let az = azimuth_rad.rem_euclid(two_pi);
        let sector = ((az / two_pi) * f64::from(AZIMUTH_SECTORS)) as u16 % AZIMUTH_SECTORS;
        (ring, sector)
    }

    /// The area of a cell in ring `ring`, m² — an annulus slice.
    fn cell_area(ring: u32) -> f64 {
        let r_in = f64::from(ring) * RANGE_CELL_M;
        let r_out = r_in + RANGE_CELL_M;
        std::f64::consts::PI * (r_out * r_out - r_in * r_in) / f64::from(AZIMUTH_SECTORS)
    }

    /// Credit observation time (SPEC.2b): called once per processed batch
    /// of this sensor's plots. Each gap credits at most
    /// [`MAX_GAP_CREDIT_S`] seconds, so a feed outage — during which the
    /// sensor demonstrably watched nothing — never matures the map.
    pub fn mark_active(&mut self, time: f64) {
        if let Some(last) = self.last_activity {
            let dt = (time - last).max(0.0);
            self.observed_s += dt.min(MAX_GAP_CREDIT_S);
        }
        self.last_activity = Some(self.last_activity.unwrap_or(time).max(time));
    }

    /// Fold in one clutter-evidence event (a plot that associated with
    /// nothing) at polar position (`range_m`, `azimuth_rad`) and data time
    /// `time`. Out-of-order times decay by zero rather than negatively.
    pub fn observe(&mut self, range_m: f64, azimuth_rad: f64, time: f64) {
        let key = Self::key(range_m, azimuth_rad);
        let observed_s = self.observed_s;
        let cell = self.cells.entry(key).or_insert(Cell {
            rate_per_s: 0.0,
            last_time: time,
            born_observed_s: observed_s,
        });
        let dt = (time - cell.last_time).max(0.0);
        cell.rate_per_s = cell.rate_per_s * (-dt / TIME_CONSTANT_S).exp() + 1.0 / TIME_CONSTANT_S;
        cell.last_time = cell.last_time.max(time);
    }

    /// The local clutter density at (`range_m`, `azimuth_rad`) and data time
    /// `time`, in expected false returns per m² per scan of length
    /// `scan_period_s` — the units [`crate::pda::ClutterModel`] carries.
    /// Clamped to `[floor, 100·default]`, where the floor is `default`
    /// until the evidence is **mature** (SPEC.2b): a cell watched for
    /// [`MATURITY_S`] of credited exposure since its first event — or an
    /// event-free cell of a mature map — may honestly claim down to
    /// `0.1·default`. Immature evidence never drops below the default:
    /// "we only just started watching" is not "clean".
    pub fn density(
        &self,
        range_m: f64,
        azimuth_rad: f64,
        time: f64,
        scan_period_s: f64,
        default: f64,
    ) -> f64 {
        let key = Self::key(range_m, azimuth_rad);
        let Some(cell) = self.cells.get(&key) else {
            // Event-free cell: with a mature map the whole coverage was
            // demonstrably watched without an event here — the floor may
            // drop; a young map claims nothing beyond the default.
            return if self.observed_s >= MATURITY_S {
                default * MATURE_MIN_FACTOR
            } else {
                default
            };
        };
        // A cell's maturity clock starts at its first event: only credited
        // watching *since then* makes its low rate a claim of quiet.
        let floor = if self.observed_s - cell.born_observed_s >= MATURITY_S {
            default * MATURE_MIN_FACTOR
        } else {
            default
        };
        let dt = (time - cell.last_time).max(0.0);
        let rate = cell.rate_per_s * (-dt / TIME_CONSTANT_S).exp();
        let raw = rate * scan_period_s / Self::cell_area(key.0);
        raw.clamp(floor, default * MAX_DENSITY_FACTOR)
    }

    /// Number of cells that carry learned state — observability hook.
    pub fn cells_total(&self) -> usize {
        self.cells.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: f64 = 1e-9;
    const SCAN: f64 = 4.0;

    /// A cell the map never saw reports the default exactly — no cold-start
    /// bias in either direction. REQ: FR-TRK-046
    #[test]
    fn unlearned_cells_report_the_default() {
        let map = ClutterMap::new();
        assert_eq!(map.density(30_000.0, 1.0, 100.0, SCAN, DEFAULT), DEFAULT);
    }

    /// Persistent events in one cell raise its density well above the
    /// default while a neighbouring sector stays untouched — the point of a
    /// *spatial* map. REQ: FR-TRK-046
    #[test]
    fn hotspot_raises_only_its_own_cell() {
        let mut map = ClutterMap::new();
        // A false return every 4 s for 10 min in the same cell.
        for k in 0..150 {
            map.observe(30_000.0, 1.0, k as f64 * SCAN);
        }
        let hot = map.density(30_000.0, 1.0, 600.0, SCAN, DEFAULT);
        let neighbour = map.density(30_000.0, 1.5, 600.0, SCAN, DEFAULT);
        assert!(hot > 10.0 * DEFAULT, "hotspot learned, got {hot:e}");
        assert_eq!(neighbour, DEFAULT, "neighbouring sector untouched");
        assert_eq!(map.cells_total(), 1);
    }

    /// A hotspot that stops emitting decays back toward clean over a few
    /// time constants — yesterday's weather is not today's claim.
    /// REQ: FR-TRK-046
    #[test]
    fn stale_hotspots_decay() {
        let mut map = ClutterMap::new();
        for k in 0..150 {
            map.observe(30_000.0, 1.0, k as f64 * SCAN);
        }
        let fresh = map.density(30_000.0, 1.0, 600.0, SCAN, DEFAULT);
        let later = map.density(30_000.0, 1.0, 600.0 + 5.0 * TIME_CONSTANT_S, SCAN, DEFAULT);
        assert!(later < fresh / 10.0, "decayed: {fresh:e} → {later:e}");
        let ancient = map.density(30_000.0, 1.0, 600.0 + 50.0 * TIME_CONSTANT_S, SCAN, DEFAULT);
        assert_eq!(ancient, DEFAULT, "fully decayed cells rest at the default");
    }

    /// The clamp band holds at both ends: a learned cell never claims more
    /// than 100× nor less than 0.1× the default. REQ: FR-TRK-046
    #[test]
    fn density_is_clamped_to_the_band() {
        let mut map = ClutterMap::new();
        // Implausibly dense burst → must clamp at the ceiling.
        for k in 0..10_000 {
            map.observe(1_000.0, 0.1, k as f64 * 0.1);
        }
        let hot = map.density(1_000.0, 0.1, 1_000.0, SCAN, DEFAULT);
        assert_eq!(hot, DEFAULT * MAX_DENSITY_FACTOR);
        // Sparse evidence never claims cleaner-than-default: a single event
        // (e.g. a real aircraft's founding plot) leaves the density at the
        // default, so the map cannot skew association around real targets.
        let mut sparse = ClutterMap::new();
        sparse.observe(30_000.0, 2.0, 0.0);
        let cold = sparse.density(30_000.0, 2.0, 4.0, SCAN, DEFAULT);
        assert_eq!(cold, DEFAULT);
    }

    /// A learned map survives the JSON snapshot roundtrip byte-exactly
    /// (tuple keys via the pair-list representation). REQ: FR-TRK-046
    #[test]
    fn snapshot_roundtrip_preserves_cells() {
        let mut map = ClutterMap::new();
        for k in 0..20 {
            map.observe(30_000.0, 1.0, k as f64 * SCAN);
        }
        let json = serde_json::to_string(&map).expect("serialise");
        let back: ClutterMap = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back, map);
    }

    /// Exposure matures the floor (SPEC.2b): after enough credited
    /// watching, an ex-hotspot decays below the default and an event-free
    /// cell reports the mature floor. REQ: FR-TRK-046
    #[test]
    fn mature_exposure_lets_quiet_cells_drop_below_the_default() {
        let mut map = ClutterMap::new();
        map.observe(30_000.0, 1.0, 0.0);
        // Credit exposure in scan-sized steps well past maturity.
        let mut t = 0.0;
        while t < 3.0 * MATURITY_S {
            t += SCAN;
            map.mark_active(t);
        }
        // The single ancient event has decayed; the cell is mature → floor.
        let ex_hotspot = map.density(30_000.0, 1.0, t, SCAN, DEFAULT);
        assert_eq!(ex_hotspot, DEFAULT * MATURE_MIN_FACTOR);
        // An event-free cell of the mature map also claims quiet.
        let quiet = map.density(60_000.0, 2.0, t, SCAN, DEFAULT);
        assert_eq!(quiet, DEFAULT * MATURE_MIN_FACTOR);
    }

    /// Immature evidence never claims cleaner than the default — the rule
    /// that protects knife-edge associations around the founding plots of
    /// real aircraft (the SPEC.2 regression). REQ: FR-TRK-046
    #[test]
    fn immature_evidence_keeps_the_default_floor() {
        let mut map = ClutterMap::new();
        map.observe(30_000.0, 1.0, 0.0);
        for k in 1..=10 {
            map.mark_active(k as f64 * SCAN); // 40 s ≪ maturity
        }
        assert_eq!(map.density(30_000.0, 1.0, 40.0, SCAN, DEFAULT), DEFAULT);
        assert_eq!(map.density(60_000.0, 2.0, 40.0, SCAN, DEFAULT), DEFAULT);
    }

    /// A feed outage credits at most one gap allowance — downtime is not
    /// observation and never matures the map. REQ: FR-TRK-046
    #[test]
    fn outages_do_not_accrue_exposure() {
        let mut map = ClutterMap::new();
        map.mark_active(0.0);
        map.mark_active(2.0 * MATURITY_S); // one huge gap
                                           // Only MAX_GAP_CREDIT_S was credited → still immature.
        assert_eq!(map.density(60_000.0, 2.0, 3_000.0, SCAN, DEFAULT), DEFAULT);
    }

    /// Azimuth wraps: 2π−ε and +ε land in the same physical sector family
    /// (keying is stable under wrap-around). REQ: FR-TRK-046
    #[test]
    fn azimuth_wraps_cleanly() {
        let mut map = ClutterMap::new();
        map.observe(30_000.0, -0.01, 0.0);
        map.observe(30_000.0, std::f64::consts::TAU - 0.01, 4.0);
        assert_eq!(map.cells_total(), 1, "wrapped azimuths share the cell");
    }
}
