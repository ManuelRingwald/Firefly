//! Periodic state snapshots + restart restore (HA.1, ADR 0040).
//!
//! A restarted Firefly must not be blind for minutes while every track
//! re-confirms from scratch (SDPS-002). The live tracker therefore writes
//! its working state to disk periodically and reads it back at startup:
//! the air picture — tracks, filter states, wire track numbers, clutter
//! maps, and the manual correlation pins (FPL.2) — is back within one
//! output tick instead of several antenna revolutions.
//!
//! Honesty rules, mirrored from the meteo/flight-plan config pattern:
//!
//! - A **misconfigured** knob fails the start (never a silent half-run);
//!   an unset path simply disables snapshots.
//! - A snapshot is restored **only** when its format version matches, the
//!   **source configuration fingerprint** matches (a tracker state built
//!   for a different sensor set must not be resurrected against it), and
//!   it is **younger** than the staleness threshold — a stale air picture
//!   is more dangerous than an empty start. Every rejection is loud.
//! - Writes are **atomic** (temp file + rename): a crash mid-write leaves
//!   the previous good snapshot, never a torn file. Write failures are
//!   non-fatal (availability over persistence, like the plot recorder).
//!
//! The input recording (`.ffplots`, ADR 0020) stays the *forensic* replay
//! path; the snapshot is the *fast-restart* path.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use firefly_core::SensorId;
use firefly_geo::Wgs84;
use firefly_track::Tracker;
use serde::{Deserialize, Serialize};

use crate::live::RadarSensor;

/// On-disk layout version. Bumped whenever the serialized tracker layout
/// changes (e.g. the 6-D IMM break of VERT.4b); an old snapshot is then
/// rejected loudly instead of silently mis-deserialized.
pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// Environment knobs (12-factor, ADR 0003).
pub const SNAPSHOT_PATH_ENV: &str = "FIREFLY_SNAPSHOT_PATH";
pub const SNAPSHOT_PERIOD_ENV: &str = "FIREFLY_SNAPSHOT_PERIOD";
pub const SNAPSHOT_MAX_AGE_ENV: &str = "FIREFLY_SNAPSHOT_MAX_AGE";

/// Default write cadence (wall-clock seconds): frequent enough that a
/// restart loses at most a few output ticks, cheap enough to be negligible.
pub const DEFAULT_SNAPSHOT_PERIOD_S: f64 = 10.0;
/// Default staleness threshold (wall-clock seconds). After 5 minutes of
/// outage the tracks would be coasted/deleted on the first ticks anyway —
/// restoring them would only parade stale traffic in front of the operator.
pub const DEFAULT_SNAPSHOT_MAX_AGE_S: f64 = 300.0;

/// The resolved snapshot configuration. `None` from
/// [`config_from_env`] means snapshots are off (path unset).
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotConfig {
    /// Where the snapshot file lives (the `.tmp` sibling is transient).
    pub path: PathBuf,
    /// Write cadence (wall-clock).
    pub period: Duration,
    /// Maximum acceptable snapshot age at restore time, seconds.
    pub max_age_s: f64,
}

/// Read the snapshot knobs. Unset/empty path ⇒ `Ok(None)` (feature off,
/// INFO is the caller's job); a set path with a malformed period or
/// max-age ⇒ `Err` — the caller treats this as fatal (meteo pattern).
pub fn config_from_env() -> Result<Option<SnapshotConfig>, String> {
    let path = match std::env::var(SNAPSHOT_PATH_ENV) {
        Ok(p) if !p.trim().is_empty() => PathBuf::from(p.trim()),
        _ => return Ok(None),
    };
    let period = positive_secs(SNAPSHOT_PERIOD_ENV, DEFAULT_SNAPSHOT_PERIOD_S)?;
    let max_age_s = positive_secs(SNAPSHOT_MAX_AGE_ENV, DEFAULT_SNAPSHOT_MAX_AGE_S)?;
    Ok(Some(SnapshotConfig {
        path,
        period: Duration::from_secs_f64(period),
        max_age_s,
    }))
}

/// Read one positive-seconds env knob with a default.
fn positive_secs(name: &str, default: f64) -> Result<f64, String> {
    parse_positive_secs(name, std::env::var(name).ok().as_deref(), default)
}

/// Parse one positive-seconds value with a default; a set but unparsable
/// or non-positive value is a hard error. Separated from the env read so
/// it is directly testable (env vars are process-global).
fn parse_positive_secs(name: &str, raw: Option<&str>, default: f64) -> Result<f64, String> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => match raw.parse::<f64>() {
            Ok(v) if v.is_finite() && v > 0.0 => Ok(v),
            _ => Err(format!(
                "{name}: expected a positive number of seconds, got {raw:?}"
            )),
        },
        None => Ok(default),
    }
}

/// A stable fingerprint of the *tracker-shaping* configuration: the system
/// reference point plus every registered sensor (id, scan period, and for
/// radars the site/error model). A snapshot written under a different
/// fingerprint is rejected at restore — a tracker state built for another
/// sensor set must not be resurrected against this one.
pub fn config_fingerprint(
    reference: Wgs84,
    geodetic: &[(SensorId, f64)],
    radars: &[RadarSensor],
) -> String {
    use std::fmt::Write as _;
    let mut out = format!(
        "v{SNAPSHOT_FORMAT_VERSION}|ref:{:.6},{:.6}",
        reference.lat_deg(),
        reference.lon_deg()
    );
    for (id, period) in geodetic {
        let _ = write!(out, "|geo:{}@{period:.3}", id.0);
    }
    for r in radars {
        let _ = write!(
            out,
            "|radar:{}@{:.3}:{:.6},{:.6},{:.1}:{:.1},{:.4}",
            r.id.0,
            r.scan_period,
            r.position.lat_deg(),
            r.position.lon_deg(),
            r.position.height,
            r.sigma_range_m,
            r.sigma_azimuth_deg,
        );
    }
    out
}

/// Serialize-only view over the live state — borrows everything, so a
/// periodic write clones nothing. Field names must match
/// [`SnapshotEnvelope`] (the owned read-back twin).
#[derive(Serialize)]
pub struct SnapshotView<'a> {
    pub format_version: u32,
    /// Wall-clock write time (Unix seconds) — the basis of the staleness
    /// check at restore. Wall-clock deliberately: the outage we measure is
    /// real elapsed time, not data time.
    pub written_unix_s: u64,
    pub config_fingerprint: &'a str,
    /// The tracker's latest data time (see `LiveTracker::latest_data_time`).
    pub data_time: Option<f64>,
    pub tracker: &'a Tracker,
    /// The manual correlation pins (FPL.2) — restored so a restart does not
    /// silently drop a controller's decision.
    pub manual_overrides: &'a BTreeMap<u16, Option<String>>,
}

/// The owned envelope read back at startup — the deserialize twin of
/// [`SnapshotView`].
#[derive(Debug, Deserialize)]
pub struct SnapshotEnvelope {
    pub format_version: u32,
    pub written_unix_s: u64,
    pub config_fingerprint: String,
    #[serde(default)]
    pub data_time: Option<f64>,
    pub tracker: Tracker,
    #[serde(default)]
    pub manual_overrides: BTreeMap<u16, Option<String>>,
}

/// Write the snapshot **atomically**: serialize to a `.tmp` sibling, sync,
/// then rename over the target. A crash at any point leaves either the old
/// snapshot or none — never a torn file.
pub fn write_atomic(path: &Path, view: &SnapshotView<'_>) -> io::Result<()> {
    let tmp = tmp_path(path);
    {
        let mut file = fs::File::create(&tmp)?;
        serde_json::to_writer(&mut file, view).map_err(io::Error::other)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)
}

/// The transient sibling used by [`write_atomic`].
fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// The startup restore decision — every branch is loud at the call site.
#[derive(Debug)]
pub enum RestoreDecision {
    /// A valid, fresh, configuration-matching snapshot: restore it.
    Restored(Box<SnapshotEnvelope>),
    /// No snapshot file exists (a normal first start).
    NoSnapshot,
    /// A snapshot exists but must not be used; the reason is for the log.
    /// The process starts empty — honest over convenient.
    Rejected(String),
}

/// Load and validate the snapshot at `path` against the current
/// configuration (`expected_fingerprint`), the layout version and the
/// staleness threshold. Never panics on file content — a corrupt snapshot
/// is a rejection, not a crash (untrusted-input discipline, Charta §8).
pub fn load(
    path: &Path,
    expected_fingerprint: &str,
    max_age_s: f64,
    now_unix_s: u64,
) -> RestoreDecision {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return RestoreDecision::NoSnapshot,
        Err(e) => return RestoreDecision::Rejected(format!("unreadable snapshot file: {e}")),
    };
    let envelope: SnapshotEnvelope = match serde_json::from_slice(&bytes) {
        Ok(e) => e,
        Err(e) => return RestoreDecision::Rejected(format!("malformed snapshot: {e}")),
    };
    if envelope.format_version != SNAPSHOT_FORMAT_VERSION {
        return RestoreDecision::Rejected(format!(
            "format version {} (this build expects {})",
            envelope.format_version, SNAPSHOT_FORMAT_VERSION
        ));
    }
    if envelope.config_fingerprint != expected_fingerprint {
        return RestoreDecision::Rejected(
            "source configuration changed since the snapshot was written".to_string(),
        );
    }
    let age_s = now_unix_s.saturating_sub(envelope.written_unix_s) as f64;
    if age_s > max_age_s {
        return RestoreDecision::Rejected(format!(
            "snapshot is {age_s:.0} s old (max {max_age_s:.0} s) — stale traffic is more \
             dangerous than an empty start"
        ));
    }
    RestoreDecision::Restored(Box::new(envelope))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::build_live_tracker;
    use firefly_opensky::OpenSkyConfig;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "firefly-snapshot-test-{}-{name}",
            std::process::id()
        ))
    }

    fn tracker() -> Tracker {
        build_live_tracker(&OpenSkyConfig::default())
    }

    fn view<'a>(
        tracker: &'a Tracker,
        fingerprint: &'a str,
        overrides: &'a BTreeMap<u16, Option<String>>,
        written_unix_s: u64,
    ) -> SnapshotView<'a> {
        SnapshotView {
            format_version: SNAPSHOT_FORMAT_VERSION,
            written_unix_s,
            config_fingerprint: fingerprint,
            data_time: Some(1234.5),
            tracker,
            manual_overrides: overrides,
        }
    }

    /// The full cycle: an atomic write round-trips through `load` (version,
    /// fingerprint and age all pass), carrying tracker, data time and the
    /// manual pins; no `.tmp` file is left behind. REQ: FR-TRK-049
    #[test]
    fn write_and_load_round_trip() {
        let path = scratch("roundtrip");
        let t = tracker();
        let mut overrides = BTreeMap::new();
        overrides.insert(7u16, Some("DLH123".to_string()));
        write_atomic(&path, &view(&t, "fp", &overrides, 1_000)).expect("write");
        assert!(!tmp_path(&path).exists(), "no torn tmp file left behind");

        match load(&path, "fp", 300.0, 1_010) {
            RestoreDecision::Restored(env) => {
                assert_eq!(env.data_time, Some(1234.5));
                assert_eq!(env.manual_overrides.get(&7), Some(&Some("DLH123".into())));
                assert_eq!(env.tracker, t, "tracker state survives byte-exactly");
            }
            other => panic!("expected restore, got {other:?}"),
        }
        let _ = fs::remove_file(&path);
    }

    /// Every rejection path is a loud, reasoned refusal — wrong version,
    /// wrong configuration fingerprint, stale age, corrupt content — and a
    /// missing file is a normal first start. REQ: FR-TRK-049
    #[test]
    fn rejections_are_loud_and_missing_file_is_normal() {
        let path = scratch("rejections");
        let t = tracker();
        let overrides = BTreeMap::new();

        assert!(matches!(
            load(&path, "fp", 300.0, 0),
            RestoreDecision::NoSnapshot
        ));

        write_atomic(&path, &view(&t, "fp", &overrides, 1_000)).expect("write");
        // Stale: written at 1000, now 2000, max 300.
        assert!(matches!(
            load(&path, "fp", 300.0, 2_000),
            RestoreDecision::Rejected(r) if r.contains("old")
        ));
        // Configuration changed.
        assert!(matches!(
            load(&path, "other-fp", 300.0, 1_010),
            RestoreDecision::Rejected(r) if r.contains("configuration")
        ));
        // Corrupt content is a rejection, never a panic.
        fs::write(&path, b"{not json").expect("corrupt");
        assert!(matches!(
            load(&path, "fp", 300.0, 1_010),
            RestoreDecision::Rejected(r) if r.contains("malformed")
        ));
        // Wrong layout version.
        let json = serde_json::to_string(&view(&t, "fp", &overrides, 1_000)).unwrap();
        fs::write(
            &path,
            json.replace("\"format_version\":1", "\"format_version\":99"),
        )
        .unwrap();
        assert!(matches!(
            load(&path, "fp", 300.0, 1_010),
            RestoreDecision::Rejected(r) if r.contains("version")
        ));
        let _ = fs::remove_file(&path);
    }

    /// The fingerprint reacts to every tracker-shaping knob: reference
    /// point, sensor set, scan periods, radar geometry. REQ: FR-TRK-049
    #[test]
    fn fingerprint_tracks_the_configuration() {
        let reference = Wgs84::from_degrees(50.0, 8.0, 0.0);
        let geo = vec![(SensorId(200), 10.0)];
        let radar = vec![RadarSensor {
            id: SensorId(301),
            position: Wgs84::from_degrees(50.1, 8.1, 100.0),
            sigma_range_m: 50.0,
            sigma_azimuth_deg: 0.1,
            scan_period: 4.7,
        }];
        let base = config_fingerprint(reference, &geo, &radar);
        assert_eq!(base, config_fingerprint(reference, &geo, &radar));
        assert_ne!(
            base,
            config_fingerprint(Wgs84::from_degrees(51.0, 8.0, 0.0), &geo, &radar),
            "reference point moves"
        );
        assert_ne!(
            base,
            config_fingerprint(
                reference,
                &[(SensorId(200), 10.0), (SensorId(201), 5.0)],
                &radar
            ),
            "sensor added"
        );
        assert_ne!(
            base,
            config_fingerprint(reference, &geo, &[]),
            "radar removed"
        );
    }

    /// Config knobs: unset/empty falls back to the default, garbage or a
    /// non-positive value is a hard error (meteo honesty pattern). Uses the
    /// pure parse helper — env vars are process-global and tests run in
    /// parallel. REQ: FR-TRK-049
    #[test]
    fn period_parsing_accepts_defaults_and_rejects_garbage() {
        assert_eq!(parse_positive_secs("X", None, 10.0), Ok(10.0));
        assert_eq!(parse_positive_secs("X", Some("  "), 10.0), Ok(10.0));
        assert_eq!(parse_positive_secs("X", Some("2.5"), 10.0), Ok(2.5));
        assert!(parse_positive_secs("X", Some("soon"), 10.0).is_err());
        assert!(parse_positive_secs("X", Some("0"), 10.0).is_err());
        assert!(parse_positive_secs("X", Some("-3"), 10.0).is_err());
        assert!(parse_positive_secs("X", Some("NaN"), 10.0).is_err());
    }
}
