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

/// One cell's exponentially-forgotten event rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Cell {
    /// Estimated event rate, events/second, valid at `last_time`.
    rate_per_s: f64,
    /// Data time of the last observed event.
    last_time: f64,
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

    /// Fold in one clutter-evidence event (a plot that associated with
    /// nothing) at polar position (`range_m`, `azimuth_rad`) and data time
    /// `time`. Out-of-order times decay by zero rather than negatively.
    pub fn observe(&mut self, range_m: f64, azimuth_rad: f64, time: f64) {
        let key = Self::key(range_m, azimuth_rad);
        let cell = self.cells.entry(key).or_insert(Cell {
            rate_per_s: 0.0,
            last_time: time,
        });
        let dt = (time - cell.last_time).max(0.0);
        cell.rate_per_s = cell.rate_per_s * (-dt / TIME_CONSTANT_S).exp() + 1.0 / TIME_CONSTANT_S;
        cell.last_time = cell.last_time.max(time);
    }

    /// The local clutter density at (`range_m`, `azimuth_rad`) and data time
    /// `time`, in expected false returns per m² per scan of length
    /// `scan_period_s` — the units [`crate::pda::ClutterModel`] carries.
    /// A cell the map has never seen an event in reports `default` exactly
    /// (no cold-start bias); learned cells are clamped to
    /// `[default, 100·default]`. The **floor is the default** on purpose:
    /// this estimator only records events, never exposure ("a scan passed
    /// with no false return"), so an absence of events is *not* evidence of
    /// a cleaner-than-default cell — the map may honestly raise the local
    /// density, never lower it. (Learning clean regions needs exposure
    /// bookkeeping — an explicit follow-up.)
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
            return default;
        };
        let dt = (time - cell.last_time).max(0.0);
        let rate = cell.rate_per_s * (-dt / TIME_CONSTANT_S).exp();
        let raw = rate * scan_period_s / Self::cell_area(key.0);
        raw.clamp(default, default * MAX_DENSITY_FACTOR)
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
