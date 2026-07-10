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

/// Longest FX chain [`parse`] accepts. 36 octets cover FRNs 1..=252 — already
/// several times the largest real UAP handled here (CAT062 ends at FRN 35) —
/// and keep every FRN inside `u8`. The bound exists because the FX mechanism
/// itself is unbounded: a hostile datagram can chain FX octets indefinitely,
/// and the unbounded FRN arithmetic then overflows `u8` (panic with debug
/// assertions, silently wrapped — i.e. misread — FRNs in release). Found by
/// fuzzing (QW.2). REQ: NFR-SAFE-002
pub const MAX_FSPEC_OCTETS: usize = 36;

/// Error: the FSPEC's FX chain ran past [`MAX_FSPEC_OCTETS`] — no category
/// decoded here has items that far into the UAP, so the record is malformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FspecTooLong;

/// Parse the FSPEC at the start of `bytes`: the set of FRNs it marks present,
/// and the number of FSPEC octets consumed (the inverse of [`fspec`]).
///
/// Reads octets while their FX bit (the lowest bit, `0x01`) is set, stopping
/// after the first octet whose FX bit is clear. An empty `bytes` yields an
/// empty FRN set and zero octets consumed — the caller decides whether that is
/// an error (e.g. truncated input). A chain longer than [`MAX_FSPEC_OCTETS`]
/// is rejected as [`FspecTooLong`] rather than overflowing the FRN space.
pub fn parse(bytes: &[u8]) -> Result<(BTreeSet<u8>, usize), FspecTooLong> {
    let mut frns = BTreeSet::new();
    let mut consumed = 0usize;

    for &octet in bytes {
        if consumed == MAX_FSPEC_OCTETS {
            return Err(FspecTooLong);
        }
        consumed += 1;
        for position in 0..7usize {
            if octet & (1 << (7 - position)) != 0 {
                // consumed ≤ MAX_FSPEC_OCTETS keeps this ≤ 252, so it fits u8.
                frns.insert(((consumed - 1) * 7 + position + 1) as u8);
            }
        }
        if octet & 0x01 == 0 {
            break;
        }
    }

    Ok((frns, consumed))
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
            let (parsed, consumed) = parse(&octets).expect("well-formed FSPEC");
            assert_eq!(parsed, set, "frns = {frns:?}");
            assert_eq!(consumed, octets.len(), "frns = {frns:?}");
        }
    }

    /// An empty input yields an empty FRN set and consumes nothing — the caller
    /// must treat that as truncated input, not as "no items present".
    #[test]
    fn parse_of_empty_input_consumes_nothing() {
        let (frns, consumed) = parse(&[]).expect("empty input is not too long");
        assert!(frns.is_empty());
        assert_eq!(consumed, 0);
    }

    /// The longest accepted chain parses with correct FRN arithmetic at the
    /// very edge: octet 36, data bit 7 → FRN 252 (fits u8, no overflow).
    /// Fuzzing regression (QW.2). REQ: NFR-SAFE-002
    #[test]
    fn parse_accepts_maximum_chain_with_exact_frn() {
        // 35 pure-FX octets, then a final octet with its last data bit set.
        let mut chain = vec![0x01u8; MAX_FSPEC_OCTETS - 1];
        chain.push(0x02); // bit 2 = position 6 → FRN (36-1)*7 + 6 + 1 = 252
        let (frns, consumed) = parse(&chain).expect("36 octets are allowed");
        assert_eq!(consumed, MAX_FSPEC_OCTETS);
        assert_eq!(frns.into_iter().collect::<Vec<_>>(), vec![252]);
    }

    /// A hostile FX chain running past the cap is rejected instead of
    /// overflowing the u8 FRN space (panic under debug assertions, silently
    /// wrapped FRNs in release). Fuzzing regression (QW.2). REQ: NFR-SAFE-002
    #[test]
    fn parse_rejects_overlong_fx_chain() {
        let chain = vec![0xFFu8; MAX_FSPEC_OCTETS + 1];
        assert_eq!(parse(&chain), Err(FspecTooLong));
        // Far longer chains (the fuzzer's original find) are equally rejected.
        assert_eq!(parse(&[0xFF; 4096]), Err(FspecTooLong));
    }
}
