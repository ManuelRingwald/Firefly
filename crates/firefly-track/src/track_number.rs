//! The wire-facing **track number pool** behind CAT062 I062/040.
//!
//! Why this exists: the tracker's internal [`TrackId`](firefly_core::TrackId)
//! is a process-unique `u32` that never repeats, but the ASTERIX track number
//! on the wire is only 16 bits. Truncating the id (the pre-FR-TRK-035
//! behaviour) silently wraps after 65 536 track births: a newborn then shares
//! its wire number with an unrelated track that may still be alive — the
//! consumer sees two aircraft under one identity, and a TSE for one deletes
//! the other. Real SDPS (ARTAS) therefore *manage* the number space instead of
//! deriving it: a number freed by a deleted track is quarantined before it may
//! be reused, so a consumer can never confuse the old track's final (TSE)
//! report with a newborn carrying the same number.
//!
//! Allocation policy:
//!
//! 1. **Fresh first** — never-used numbers ascending from 1, maximising the
//!    time before any number is ever seen twice. Number 0 is never allocated;
//!    it stays free for consumers that treat 0 as a sentinel.
//! 2. **Then FIFO reuse** — once the fresh space is exhausted, the number that
//!    finished its quarantine longest ago is reused first (largest possible
//!    gap between the old track's TSE and the new track's birth).
//! 3. **Honest exhaustion** — with every number in use or still quarantined
//!    (> 65 535 concurrent tracks), allocation returns `None` and the tracker
//!    declines to initiate; far beyond any real capacity, but defined instead
//!    of undefined.
//!
//! Time is **data time** (seconds), the same monotonic watermark clock the
//! tracker runs on — deterministic and replayable (ADR 0003), no wall clock.
//!
//! REQ: FR-TRK-035

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// How long (data-time seconds) a freed track number stays quarantined before
/// it may be reused. Long enough that every consumer has processed the ended
/// track's final TSE report (output heartbeats are on the order of seconds)
/// and stale state for the number has been dropped.
pub(crate) const TRACK_NUMBER_QUARANTINE_SECS: f64 = 60.0;

/// The lowest track number ever allocated. 0 is reserved as a sentinel.
const FIRST_TRACK_NUMBER: u16 = 1;

/// A freed number waiting out its quarantine.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Quarantined {
    number: u16,
    /// Data time from which the number may be allocated again.
    reusable_at: f64,
}

/// Allocator for the 16-bit CAT062 track numbers (see module docs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct TrackNumberPool {
    /// Next never-allocated number, ascending; `None` once the fresh space
    /// `FIRST_TRACK_NUMBER..=max_fresh` is exhausted and the pool lives off
    /// quarantine-expired reuse alone.
    next_fresh: Option<u16>,
    /// Upper bound of the fresh space. `u16::MAX` in production; tests shrink
    /// it to exercise the reuse and exhaustion paths cheaply.
    max_fresh: u16,
    /// Freed numbers in release order. Data time is monotonic (the tracker's
    /// watermark), so the queue is sorted by `reusable_at` by construction —
    /// the front is always the first number to leave quarantine.
    quarantine: VecDeque<Quarantined>,
}

impl TrackNumberPool {
    pub(crate) fn new() -> Self {
        Self::with_fresh_limit(u16::MAX)
    }

    /// A pool whose fresh space ends at `max_fresh` instead of `u16::MAX`.
    /// Production always uses [`TrackNumberPool::new`]; this exists so tests
    /// can reach the reuse/exhaustion behaviour without 65 535 allocations.
    pub(crate) fn with_fresh_limit(max_fresh: u16) -> Self {
        debug_assert!(max_fresh >= FIRST_TRACK_NUMBER);
        Self {
            next_fresh: Some(FIRST_TRACK_NUMBER),
            max_fresh,
            quarantine: VecDeque::new(),
        }
    }

    /// Allocate a track number at data time `now`, or `None` if every number
    /// is in use or still quarantined (see module docs for the policy).
    pub(crate) fn allocate(&mut self, now: f64) -> Option<u16> {
        if let Some(fresh) = self.next_fresh {
            self.next_fresh = fresh.checked_add(1).filter(|&n| n <= self.max_fresh);
            return Some(fresh);
        }
        match self.quarantine.front() {
            Some(q) if q.reusable_at <= now => self.quarantine.pop_front().map(|q| q.number),
            _ => None,
        }
    }

    /// Return `number` to the pool at data time `now`; it becomes allocatable
    /// again once [`TRACK_NUMBER_QUARANTINE_SECS`] of data time have passed.
    pub(crate) fn release(&mut self, number: u16, now: f64) {
        debug_assert!(
            self.quarantine
                .back()
                .is_none_or(|q| q.reusable_at <= now + TRACK_NUMBER_QUARANTINE_SECS),
            "releases must arrive in nondecreasing data time (watermark invariant)"
        );
        self.quarantine.push_back(Quarantined {
            number,
            reusable_at: now + TRACK_NUMBER_QUARANTINE_SECS,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fresh numbers are handed out ascending from 1; 0 is never allocated.
    /// REQ: FR-TRK-035
    #[test]
    fn fresh_numbers_ascend_from_one() {
        let mut pool = TrackNumberPool::new();
        assert_eq!(pool.allocate(0.0), Some(1));
        assert_eq!(pool.allocate(0.0), Some(2));
        assert_eq!(pool.allocate(0.0), Some(3));
    }

    /// A freed number is not reusable before its quarantine has elapsed, and
    /// fresh numbers are preferred over reusable ones. REQ: FR-TRK-035
    #[test]
    fn freed_number_waits_out_quarantine() {
        let mut pool = TrackNumberPool::with_fresh_limit(2);
        assert_eq!(pool.allocate(0.0), Some(1));
        pool.release(1, 10.0);

        // Fresh number 2 is preferred even though 1 is in quarantine.
        assert_eq!(pool.allocate(10.0), Some(2));

        // Fresh space exhausted; 1 is still quarantined → nothing available.
        assert_eq!(
            pool.allocate(10.0 + TRACK_NUMBER_QUARANTINE_SECS - 1.0),
            None
        );

        // Quarantine over → 1 comes back.
        assert_eq!(pool.allocate(10.0 + TRACK_NUMBER_QUARANTINE_SECS), Some(1));
    }

    /// After the fresh space, numbers are reused strictly FIFO — the number
    /// freed longest ago comes back first. REQ: FR-TRK-035
    #[test]
    fn reuse_is_fifo_by_release_time() {
        let mut pool = TrackNumberPool::with_fresh_limit(3);
        for _ in 0..3 {
            pool.allocate(0.0);
        }
        pool.release(3, 1.0);
        pool.release(1, 2.0);
        pool.release(2, 3.0);

        let after = 3.0 + TRACK_NUMBER_QUARANTINE_SECS;
        assert_eq!(pool.allocate(after), Some(3), "freed first, reused first");
        assert_eq!(pool.allocate(after), Some(1));
        assert_eq!(pool.allocate(after), Some(2));
        assert_eq!(pool.allocate(after), None, "pool drained");
    }

    /// With every number in use or quarantined the pool reports exhaustion
    /// instead of handing out a duplicate. REQ: FR-TRK-035
    #[test]
    fn exhausted_pool_never_duplicates() {
        let mut pool = TrackNumberPool::with_fresh_limit(1);
        assert_eq!(pool.allocate(0.0), Some(1));
        assert_eq!(pool.allocate(0.0), None, "in use → exhausted");
        pool.release(1, 5.0);
        assert_eq!(pool.allocate(5.0), None, "quarantined → still exhausted");
    }

    /// The full fresh space really ends at `u16::MAX` (no off-by-one, no wrap
    /// to 0). REQ: FR-TRK-035
    #[test]
    fn fresh_space_ends_at_u16_max() {
        let mut pool = TrackNumberPool::new();
        pool.next_fresh = Some(u16::MAX);
        assert_eq!(pool.allocate(0.0), Some(u16::MAX));
        assert_eq!(pool.allocate(0.0), None, "no wrap-around past u16::MAX");
    }
}
