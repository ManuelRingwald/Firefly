//! Legacy CAT001/CAT002 handling on the radar feed (FEP.4).
//!
//! A legacy radar head pairs **CAT001** target reports with **CAT002** service
//! messages on the same feed. Two adapter concerns live here, both pure (no
//! I/O) so they are unit-testable:
//!
//! - **Time anchoring.** CAT001 records carry only a **truncated** time of day
//!   (I001/141, modulo 512 s); the full time classically arrives in the CAT002
//!   stream (I002/030). [`expand_truncated_tod`] reconstructs the full
//!   timestamp by picking the value congruent to the truncated one that lies
//!   **nearest to the anchor** — robust to the anchor lagging or leading the
//!   plot by up to half a cycle (±256 s), far beyond any real feed jitter.
//! - **Plot mapping.** [`legacy_datagram_to_plots`] decodes a CAT001 datagram,
//!   drops **simulated** reports (Firefly carries no simulated traffic,
//!   FR-TRK-036), expands each report's truncated time against the anchor and
//!   feeds the result through the *same* [`target_report_to_plot`] mapping as
//!   the CAT048 path — the tracker cannot tell the radar generations apart.
//!
//! Without an anchor (no CAT002 time seen yet) a report's full time cannot be
//! derived; it is dropped like any time-less report. Honest: better a plot
//! lost in the first antenna revolution than one carrying an invented time.

use firefly_asterix::{
    decode_legacy_reports, decode_legacy_service_messages, DecodedServiceMessage,
    TRUNCATED_TOD_CYCLE_SECS,
};
use firefly_core::{Plot, Timestamp};

use crate::config::RadarConfig;
use crate::plot::target_report_to_plot;

/// Seconds per day — the ASTERIX time-of-day counter wraps at UTC midnight.
const SECONDS_PER_DAY: f64 = 86_400.0;

/// Expand a truncated CAT001 time of day (`truncated_secs`, modulo 512 s) into
/// a full time of day, using `anchor_secs` (a full ToD from the CAT002 stream)
/// as the reference: the result is the value congruent to `truncated_secs`
/// (mod 512) **nearest** to the anchor. A result that would fall before
/// midnight (negative) wraps into the previous day's tail instead — both
/// counters reset at UTC midnight together.
pub fn expand_truncated_tod(truncated_secs: f64, anchor_secs: f64) -> f64 {
    let cycle = TRUNCATED_TOD_CYCLE_SECS;
    let mut diff = (truncated_secs - anchor_secs).rem_euclid(cycle);
    if diff > cycle / 2.0 {
        diff -= cycle;
    }
    let full = anchor_secs + diff;
    if full < 0.0 {
        full + SECONDS_PER_DAY
    } else {
        full
    }
}

/// Decode one CAT002 datagram into service messages (pure: no I/O). A
/// malformed block yields an empty vector — never a panic (untrusted-input
/// policy, same as the CAT034 path).
pub fn legacy_datagram_to_service(datagram: &[u8]) -> Vec<DecodedServiceMessage> {
    match decode_legacy_service_messages(datagram) {
        Ok(messages) => messages,
        Err(e) => {
            tracing::debug!(%e, "dropping malformed CAT002 datagram");
            Vec::new()
        }
    }
}

/// Decode one CAT001 datagram into plots (pure: no I/O). `anchor_secs` is the
/// last full time of day seen on the feed's service stream (CAT002/CAT034);
/// without it, reports carrying only a truncated time cannot be timestamped
/// and are dropped. Simulated reports never become plots.
pub fn legacy_datagram_to_plots(
    datagram: &[u8],
    config: &RadarConfig,
    anchor_secs: Option<f64>,
) -> Vec<Plot> {
    match decode_legacy_reports(datagram) {
        Ok(reports) => reports
            .into_iter()
            .filter_map(|r| {
                if r.simulated {
                    return None;
                }
                let time = match (r.truncated_time_secs, anchor_secs) {
                    (Some(truncated), Some(anchor)) => {
                        Some(Timestamp(expand_truncated_tod(truncated, anchor)))
                    }
                    _ => None,
                };
                target_report_to_plot(&r.into_target_report(time), config.sensor_id)
            })
            .collect(),
        Err(e) => {
            tracing::debug!(%e, "dropping malformed CAT001 datagram");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firefly_core::DetectionKind;

    /// The nearest congruent value wins: anchor mid-cycle, plot slightly
    /// before or after — including across a 512-s cycle boundary.
    /// REQ: FR-NET-016
    #[test]
    fn expansion_picks_the_nearest_congruent_time() {
        // Anchor 36 000 s (10:00): cycle base 36 000 - (36 000 mod 512).
        // Truncated 100 s → nearest congruent to 36 100? 36 000 mod 512 = 160.
        // diff = (100 - 36 000) mod 512 = (100 - 160) mod 512 = 452 → -60.
        assert_eq!(expand_truncated_tod(100.0, 36_000.0), 35_940.0);
        // A plot 10 s ahead of the anchor.
        assert_eq!(expand_truncated_tod(170.0, 36_000.0), 36_010.0);
        // Exactly congruent: the anchor itself.
        assert_eq!(expand_truncated_tod(160.0, 36_000.0), 36_000.0);
        // Cycle boundary: anchor at 36 090 (mod 512 = 250), truncated 500
        // → diff = 250 → within half-cycle, forward: 36 340.
        assert_eq!(expand_truncated_tod(500.0, 36_090.0), 36_340.0);
    }

    /// An anchor just after midnight with a plot from just before midnight
    /// wraps into the previous day's tail instead of going negative.
    /// REQ: FR-NET-016
    #[test]
    fn expansion_wraps_at_midnight() {
        // Anchor 10 s after midnight; truncated time 502 mod 512 ⇒ the plot
        // is 20 s older than the anchor ⇒ 10 - 20 = -10 → 86 390.
        let full = expand_truncated_tod(502.0 % 512.0, 10.0);
        assert_eq!(full, 86_390.0);
    }

    /// A CAT001 plot datagram with an anchor becomes a polar plot whose time
    /// is the expanded full ToD; without an anchor it is dropped.
    /// REQ: FR-NET-016
    #[test]
    fn cat001_datagram_yields_anchored_plot() {
        let datagram = [
            1, 0x00, 0x11, // CAT=1, LEN=17
            0xFA, // FSPEC {1,2,3,4,5,7}
            0x19, 0x07, // I001/010
            0x30, // I001/020 plot, combined
            0x32, 0x00, 0x40, 0x00, // I001/040 100 NM, 90°
            0x02, 0x8B, // I001/070
            0x05, 0x78, // I001/090 FL350
            0x32, 0x00, // I001/141 100 s (mod 512)
        ];
        let config = RadarConfig::default();

        // Anchor 36 000 s: 36 000 mod 512 = 160, truncated 100 → -60 ⇒ 35 940.
        let plots = legacy_datagram_to_plots(&datagram, &config, Some(36_000.0));
        assert_eq!(plots.len(), 1);
        assert_eq!(plots[0].kind, DetectionKind::Combined);
        assert_eq!(plots[0].time, Timestamp(35_940.0));
        assert_eq!(plots[0].sensor, config.sensor_id);
        assert_eq!(plots[0].mode_ac.mode_3a, Some(0o1213));

        // No anchor → no full time derivable → dropped, not invented.
        assert!(legacy_datagram_to_plots(&datagram, &config, None).is_empty());
        // Garbage never panics.
        assert!(legacy_datagram_to_plots(&[], &config, Some(0.0)).is_empty());
        assert!(legacy_datagram_to_plots(&[1, 0xFF, 0xFF, 0x00], &config, Some(0.0)).is_empty());
    }

    /// Simulated reports (I001/020 SIM) never become plots — Firefly carries
    /// no simulated traffic (FR-TRK-036). REQ: FR-NET-016
    #[test]
    fn simulated_reports_are_dropped() {
        let datagram = [
            1, 0x00, 0x0D, // CAT=1, LEN=13
            0xE8, // FSPEC {1,2,3,5}
            0x19, 0x07, // I001/010
            0x70, // I001/020 plot, SIM, combined
            0x32, 0x00, 0x40, 0x00, // I001/040
            0x05, 0x78, // I001/090
        ];
        let config = RadarConfig::default();
        assert!(legacy_datagram_to_plots(&datagram, &config, Some(36_000.0)).is_empty());
    }

    /// A CAT002 north-marker datagram decodes to a service message carrying
    /// the full ToD (the anchor source); garbage yields none.
    /// REQ: FR-NET-016
    #[test]
    fn cat002_datagram_yields_service_messages() {
        let datagram = [
            2, 0x00, 0x0A, // CAT=2, LEN=10
            0xD0, // FSPEC {1,2,4}
            0x19, 0x07, // I002/010
            0x01, // I002/000 north marker
            0x07, 0x08, 0x00, // I002/030 time 3600 s
        ];
        let messages = legacy_datagram_to_service(&datagram);
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].message_type,
            firefly_asterix::ServiceMessageType::NorthMarker
        );
        assert_eq!(messages[0].time, Some(firefly_core::Timestamp(3600.0)));

        assert!(legacy_datagram_to_service(&[]).is_empty());
        assert!(legacy_datagram_to_service(&[2, 0xFF, 0xFF]).is_empty());
    }
}
