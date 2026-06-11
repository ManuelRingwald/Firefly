//! The FSPEC mechanism: telling a decoder which data items a record carries.
//!
//! Every ASTERIX record starts with a **field specification** (FSPEC) — a string
//! of octets where each of the seven high bits flags one data item as *present*
//! or *absent*, and the lowest bit (the **FX**, field extension) says "another
//! FSPEC octet follows". The bit a given item occupies is fixed by the category's
//! **UAP** and identified by its **FRN** (field reference number): FRN 1 is the
//! most significant bit of the first octet, FRN 7 the second-least significant,
//! FRN 8 the most significant bit of the *second* octet, and so on.
//!
//! [`RecordBuilder`] hides this bookkeeping: callers add each present item by its
//! FRN together with its already-encoded bytes; [`RecordBuilder::finish`] then
//! computes the minimal FSPEC and appends the item payloads in ascending FRN
//! order (which is exactly UAP order).

use std::collections::{BTreeMap, BTreeSet};

/// Assembles one ASTERIX record: a set of present data items keyed by FRN.
///
/// A `BTreeMap` keeps the items ordered by FRN, so iterating yields them in UAP
/// order without any extra sorting at `finish` time.
#[derive(Default)]
pub struct RecordBuilder {
    items: BTreeMap<u8, Vec<u8>>,
}

impl RecordBuilder {
    /// An empty record.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one present data item: its `frn` (its slot in the UAP) and its
    /// already-encoded `bytes`. Adding the same FRN twice replaces it.
    pub fn item(mut self, frn: u8, bytes: Vec<u8>) -> Self {
        self.items.insert(frn, bytes);
        self
    }

    /// Render the record: the FSPEC for the present items, followed by their
    /// payloads in ascending FRN (UAP) order.
    pub fn finish(self) -> Vec<u8> {
        let frns: BTreeSet<u8> = self.items.keys().copied().collect();
        let mut out = fspec(&frns);
        for bytes in self.items.values() {
            out.extend_from_slice(bytes);
        }
        out
    }
}

/// Parse the FSPEC at the start of `bytes`: the set of FRNs it marks present,
/// and the number of FSPEC octets consumed (the inverse of [`fspec`]).
///
/// Reads octets while their FX bit (the lowest bit, `0x01`) is set, stopping
/// after the first octet whose FX bit is clear. An empty `bytes` yields an
/// empty FRN set and zero octets consumed — the caller decides whether that is
/// an error (e.g. truncated input).
pub fn parse(bytes: &[u8]) -> (BTreeSet<u8>, usize) {
    let mut frns = BTreeSet::new();
    let mut consumed = 0;

    for &octet in bytes {
        consumed += 1;
        for position in 0..7u8 {
            if octet & (1 << (7 - position)) != 0 {
                frns.insert((consumed - 1) as u8 * 7 + position + 1);
            }
        }
        if octet & 0x01 == 0 {
            break;
        }
    }

    (frns, consumed)
}

/// Compute the FSPEC octets for a set of present FRNs.
///
/// The result is just long enough to reach the highest present FRN: one octet per
/// group of seven FRNs, with the FX bit set on every octet but the last.
fn fspec(frns: &BTreeSet<u8>) -> Vec<u8> {
    let Some(&max) = frns.iter().next_back() else {
        return Vec::new();
    };
    let octets = max.div_ceil(7) as usize;
    let mut out = vec![0u8; octets];

    for &frn in frns {
        let octet = ((frn - 1) / 7) as usize;
        let position = (frn - 1) % 7; // 0 = bit 8 (MSB) … 6 = bit 2
        out[octet] |= 1 << (7 - position);
    }

    // Every octet except the last chains to the next via its FX bit.
    for octet in out.iter_mut().take(octets - 1) {
        *octet |= 0x01;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fspec_of(frns: &[u8]) -> Vec<u8> {
        fspec(&frns.iter().copied().collect())
    }

    /// No items → no FSPEC at all.
    #[test]
    fn empty_set_has_no_fspec() {
        assert!(fspec_of(&[]).is_empty());
    }

    /// FRN 1 is the most significant bit of the first octet; with nothing beyond
    /// it the octet stands alone (no FX).
    #[test]
    fn single_first_item_sets_the_top_bit() {
        assert_eq!(fspec_of(&[1]), vec![0x80]);
    }

    /// Several items inside the first seven share one octet, MSB-first, still no
    /// FX. FRN 1 → 0x80, FRN 4 → 0x10.
    #[test]
    fn items_within_one_octet_need_no_extension() {
        assert_eq!(fspec_of(&[1, 4]), vec![0x90]);
    }

    /// Reaching FRN 8+ forces a second octet, and the first octet's FX bit
    /// (0x01) must be set to announce it. FRN 12 sits at bit 8-… of octet two:
    /// (12-1) % 7 = 4 → bit (7-4)=3 → 0x08. So {1,4,12} → [0x91, 0x08].
    #[test]
    fn crossing_into_the_second_octet_sets_fx() {
        assert_eq!(fspec_of(&[1, 4, 12]), vec![0x91, 0x08]);
    }

    /// FRN 7 is the last data bit of octet one (0x02); FRN 8 is the first of
    /// octet two (0x80), which requires the FX on octet one.
    #[test]
    fn octet_boundary_is_at_frn_seven() {
        assert_eq!(fspec_of(&[7]), vec![0x02]);
        assert_eq!(fspec_of(&[8]), vec![0x01, 0x80]);
    }

    /// The builder concatenates FSPEC + payloads in ascending FRN order,
    /// regardless of the order items were added.
    #[test]
    fn builder_orders_payloads_by_frn() {
        let record = RecordBuilder::new()
            .item(12, vec![0xAA, 0xBB])
            .item(1, vec![0x11, 0x22])
            .finish();
        // FSPEC for {1,12} = [0x81, 0x08], then FRN 1's bytes, then FRN 12's.
        assert_eq!(record, vec![0x81, 0x08, 0x11, 0x22, 0xAA, 0xBB]);
    }

    /// `parse` is the inverse of `fspec`: for every FRN set we can build, parsing
    /// the resulting octets recovers the same set and consumes exactly those
    /// octets (nothing more).
    #[test]
    fn parse_inverts_fspec() {
        for frns in [
            vec![1u8],
            vec![1, 4],
            vec![1, 4, 12],
            vec![7],
            vec![8],
            vec![1, 4, 5, 6, 7, 12, 13, 14, 16],
        ] {
            let set: BTreeSet<u8> = frns.iter().copied().collect();
            let octets = fspec(&set);
            let (parsed, consumed) = parse(&octets);
            assert_eq!(parsed, set, "frns = {frns:?}");
            assert_eq!(consumed, octets.len(), "frns = {frns:?}");
        }
    }

    /// An empty input yields an empty FRN set and consumes nothing — the caller
    /// must treat that as truncated input, not as "no items present".
    #[test]
    fn parse_of_empty_input_consumes_nothing() {
        let (frns, consumed) = parse(&[]);
        assert!(frns.is_empty());
        assert_eq!(consumed, 0);
    }
}
