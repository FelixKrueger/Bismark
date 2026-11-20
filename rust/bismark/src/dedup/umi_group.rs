//! Mismatch-tolerant UMI grouping for `--umi_mismatches N`.
//!
//! Pass 1 feeds every unit's [`UmiDedupKey`] to a [`UmiGroupCollector`];
//! [`UmiGroupCollector::resolve`] groups the UMIs at each position with the
//! UMI-tools "directional" rule and returns which units pass 2 keeps.

use rustc_hash::FxHashMap;

use crate::dedup::UmiDedupKey;

/// True if `a` and `b` have the same length and differ at no more than `max` bytes.
#[must_use]
pub fn hamming_within(a: &[u8], b: &[u8], max: u8) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u32;
    for (x, y) in a.iter().zip(b) {
        if x != y {
            diff += 1;
            if diff > u32::from(max) {
                return false;
            }
        }
    }
    true
}

/// Group the UMIs seen at one position; returns, for each input UMI, the index of its group's seed.
///
/// A UMI `a` absorbs `b` when they are within `max` mismatches and
/// `count(a) >= 2 * count(b) - 1`. Seeds are taken by descending count, ties by
/// UMI bytes, so the result does not depend on the input order.
#[must_use]
pub fn group_position(umis: &[(&[u8], u32)], max: u8) -> Vec<usize> {
    let n = umis.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| {
        umis[j]
            .1
            .cmp(&umis[i].1)
            .then_with(|| umis[i].0.cmp(umis[j].0))
    });
    let mut seed_of = vec![usize::MAX; n];
    let mut queue = Vec::new();
    for &seed in &order {
        if seed_of[seed] != usize::MAX {
            continue;
        }
        seed_of[seed] = seed;
        queue.clear();
        queue.push(seed);
        while let Some(node) = queue.pop() {
            let node_count = u64::from(umis[node].1);
            for &other in &order {
                if seed_of[other] != usize::MAX {
                    continue;
                }
                let other_count = u64::from(umis[other].1);
                if node_count + 1 >= 2 * other_count
                    && hamming_within(umis[node].0, umis[other].0, max)
                {
                    seed_of[other] = seed;
                    queue.push(other);
                }
            }
        }
    }
    seed_of
}

/// Fixed-size bitset over unit indices (one bit per SE record or PE pair).
#[derive(Debug, Clone, Default)]
pub struct KeepSet {
    words: Vec<u64>,
    len: u64,
}

impl KeepSet {
    /// A set of `len` cleared bits.
    #[must_use]
    pub fn new(len: u64) -> Self {
        let words = usize::try_from(len.div_ceil(64)).expect("unit count fits in usize");
        Self {
            words: vec![0; words],
            len,
        }
    }

    /// Set bit `i`.
    pub fn insert(&mut self, i: u64) {
        let word = usize::try_from(i / 64).expect("index fits in usize");
        self.words[word] |= 1 << (i % 64);
    }

    /// True if bit `i` is set; false for indices past the end.
    #[must_use]
    pub fn contains(&self, i: u64) -> bool {
        if i >= self.len {
            return false;
        }
        let word = usize::try_from(i / 64).expect("index fits in usize");
        self.words[word] & (1 << (i % 64)) != 0
    }

    /// Number of units the set was built for.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.len
    }

    /// True if the set covers no units.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Pass-1 accumulator: per key, how many units carry it and the index of the first.
#[derive(Debug, Default)]
pub struct UmiGroupCollector {
    keys: FxHashMap<UmiDedupKey, (u32, u64)>,
    units: u64,
}

/// What grouping decided: the units to keep plus the numbers for the report and stderr.
#[derive(Debug)]
pub struct UmiGrouping {
    /// Units (SE records or PE pairs) pass 2 writes.
    pub keep: KeepSet,
    /// Units seen in pass 1.
    pub units: u64,
    /// Units removed as duplicates (`units` minus the number of groups).
    pub removed: u64,
    /// Groups with more than one unit (the report's "different position(s)").
    pub duplicate_groups: usize,
    /// Distinct UMIs that joined another UMI's group.
    pub umis_merged: u64,
    /// Groups that contain more than one distinct UMI.
    pub merged_groups: u64,
    /// Positions with at least one such group.
    pub merged_positions: u64,
}

impl UmiGroupCollector {
    /// An empty collector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the next unit's key.
    pub fn add(&mut self, key: UmiDedupKey) {
        let index = self.units;
        let entry = self.keys.entry(key).or_insert((0, index));
        entry.0 = entry.0.saturating_add(1);
        self.units += 1;
    }

    /// Units recorded so far.
    #[must_use]
    pub fn units(&self) -> u64 {
        self.units
    }

    /// Group the UMIs at every position and decide which units to keep.
    #[must_use]
    pub fn resolve(self, max_mismatches: u8) -> UmiGrouping {
        let units = self.units;
        // Borrowed entries keep the sort buffer at 24 bytes per key, next to the map.
        let keys = self.keys;
        let mut entries: Vec<(&UmiDedupKey, u32, u64)> =
            keys.iter().map(|(k, &(c, f))| (k, c, f)).collect();
        entries.sort_unstable_by(|a, b| {
            (a.0.strand as u8, a.0.chr_id, a.0.start, a.0.end)
                .cmp(&(b.0.strand as u8, b.0.chr_id, b.0.start, b.0.end))
                .then_with(|| a.0.umi.as_slice().cmp(b.0.umi.as_slice()))
        });

        let mut keep = KeepSet::new(units);
        let mut groups = 0u64;
        let mut duplicate_groups = 0usize;
        let mut umis_merged = 0u64;
        let mut merged_groups = 0u64;
        let mut merged_positions = 0u64;

        let same_position = |a: &UmiDedupKey, b: &UmiDedupKey| {
            a.strand == b.strand && a.chr_id == b.chr_id && a.start == b.start && a.end == b.end
        };
        let mut start = 0;
        while start < entries.len() {
            let mut end = start + 1;
            while end < entries.len() && same_position(entries[start].0, entries[end].0) {
                end += 1;
            }
            let run = &entries[start..end];
            if let [(_, count, first)] = run {
                groups += 1;
                keep.insert(*first);
                if *count > 1 {
                    duplicate_groups += 1;
                }
                start = end;
                continue;
            }
            let umis: Vec<(&[u8], u32)> =
                run.iter().map(|(k, c, _)| (k.umi.as_slice(), *c)).collect();
            let seed_of = group_position(&umis, max_mismatches);

            // Per seed: (total units, lowest first index, distinct UMIs).
            let mut per_seed: FxHashMap<usize, (u64, u64, u64)> = FxHashMap::default();
            for (i, &seed) in seed_of.iter().enumerate() {
                let slot = per_seed.entry(seed).or_insert((0, u64::MAX, 0));
                slot.0 += u64::from(run[i].1);
                slot.1 = slot.1.min(run[i].2);
                slot.2 += 1;
            }
            let mut position_merged = false;
            for &(total, first, distinct) in per_seed.values() {
                groups += 1;
                keep.insert(first);
                if total > 1 {
                    duplicate_groups += 1;
                }
                if distinct > 1 {
                    merged_groups += 1;
                    umis_merged += distinct - 1;
                    position_merged = true;
                }
            }
            if position_merged {
                merged_positions += 1;
            }
            start = end;
        }

        UmiGrouping {
            keep,
            units,
            removed: units - groups,
            duplicate_groups,
            umis_merged,
            merged_groups,
            merged_positions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::BismarkStrand;
    use smallvec::SmallVec;

    fn groups(umis: &[(&str, u32)], max: u8) -> Vec<Vec<String>> {
        let input: Vec<(&[u8], u32)> = umis.iter().map(|(u, c)| (u.as_bytes(), *c)).collect();
        let seed_of = group_position(&input, max);
        let mut by_seed: FxHashMap<usize, Vec<String>> = FxHashMap::default();
        for (i, &s) in seed_of.iter().enumerate() {
            by_seed.entry(s).or_default().push(umis[i].0.to_string());
        }
        let mut out: Vec<Vec<String>> = by_seed
            .into_values()
            .map(|mut g| {
                g.sort();
                g
            })
            .collect();
        out.sort();
        out
    }

    fn key(start: u32, umi: &str) -> UmiDedupKey {
        UmiDedupKey::se(
            BismarkStrand::OT,
            0,
            start,
            SmallVec::from_slice(umi.as_bytes()),
        )
    }

    #[test]
    fn hamming_counts_mismatches_and_rejects_length_differences() {
        assert!(hamming_within(b"AAAA", b"AAAA", 0));
        assert!(!hamming_within(b"AAAA", b"AAAT", 0));
        assert!(hamming_within(b"AAAA", b"AAAT", 1));
        assert!(!hamming_within(b"AAAA", b"AATT", 1));
        assert!(!hamming_within(b"AAAA", b"AAA", 3));
        assert!(!hamming_within(b"AAAA", b"AANA", 0));
        assert!(hamming_within(b"AANA", b"AANA", 0));
    }

    #[test]
    fn abundant_umi_absorbs_rare_neighbour() {
        assert_eq!(
            groups(&[("AAAA", 3), ("AAAT", 1)], 1),
            vec![vec!["AAAA".to_string(), "AAAT".to_string()]]
        );
    }

    #[test]
    fn exact_mode_keeps_every_umi_apart() {
        assert_eq!(groups(&[("AAAA", 3), ("AAAT", 1)], 0).len(), 2);
    }

    #[test]
    fn equal_counts_merge_only_at_one() {
        assert_eq!(groups(&[("AAAA", 1), ("AAAT", 1)], 1).len(), 1);
        assert_eq!(groups(&[("AAAA", 2), ("AAAT", 2)], 1).len(), 2);
    }

    #[test]
    fn count_rule_boundary() {
        // 2*2-1 = 3 <= 3 joins; 2*3-1 = 5 > 4 stays apart
        assert_eq!(groups(&[("AAAA", 3), ("AAAT", 2)], 1).len(), 1);
        assert_eq!(groups(&[("AAAA", 4), ("AAAT", 3)], 1).len(), 2);
    }

    #[test]
    fn large_max_does_not_override_counts() {
        assert_eq!(groups(&[("AAAA", 5), ("TTTT", 5)], 8).len(), 2);
    }

    #[test]
    fn chain_is_followed_from_the_seed() {
        // AAAA(9) -> AAAT(4) -> AATT(1): AATT is 2 from the seed but 1 from AAAT
        assert_eq!(groups(&[("AAAA", 9), ("AAAT", 4), ("AATT", 1)], 1).len(), 1);
    }

    #[test]
    fn more_abundant_seed_claims_a_shared_neighbour_first() {
        let g = groups(&[("AAAA", 10), ("TTTT", 6), ("AAAT", 1), ("TTTA", 1)], 1);
        assert_eq!(
            g,
            vec![
                vec!["AAAA".to_string(), "AAAT".to_string()],
                vec!["TTTA".to_string(), "TTTT".to_string()],
            ]
        );
    }

    #[test]
    fn n_is_an_ordinary_base() {
        assert_eq!(groups(&[("AAAA", 3), ("AANA", 1)], 1).len(), 1);
        assert_eq!(groups(&[("NNNN", 3), ("AAAA", 1)], 1).len(), 2);
    }

    #[test]
    fn umis_of_different_length_never_join() {
        assert_eq!(groups(&[("AAAA", 3), ("AAAAA", 1)], 4).len(), 2);
    }

    #[test]
    fn dual_umis_compare_with_the_separator_in_place() {
        assert_eq!(groups(&[("AAAA+CCCC", 3), ("AAAA+CCCG", 1)], 1).len(), 1);
        assert_eq!(groups(&[("AAAA+CCCC", 3), ("AAAAC+CCC", 1)], 1).len(), 2);
    }

    #[test]
    fn membership_does_not_depend_on_input_order() {
        let base = [
            ("ACGT", 5),
            ("ACGA", 2),
            ("ACGG", 2),
            ("TCGT", 1),
            ("GGGG", 2),
            ("GGGC", 1),
        ];
        let expected = groups(&base, 1);
        let mut rotated = base.to_vec();
        for _ in 0..base.len() {
            rotated.rotate_left(1);
            assert_eq!(groups(&rotated, 1), expected);
            let mut reversed = rotated.clone();
            reversed.reverse();
            assert_eq!(groups(&reversed, 1), expected);
        }
    }

    #[test]
    fn keep_set_sets_and_reads_bits() {
        let mut k = KeepSet::new(130);
        k.insert(0);
        k.insert(64);
        k.insert(129);
        assert!(k.contains(0) && k.contains(64) && k.contains(129));
        assert!(!k.contains(1) && !k.contains(128) && !k.contains(130));
        assert_eq!(k.len(), 130);
    }

    #[test]
    fn resolve_keeps_the_first_unit_of_each_group() {
        let mut c = UmiGroupCollector::new();
        // unit 0..: A x3 at pos 10, A' x1 at pos 10, B x1 at pos 20
        c.add(key(10, "AAAT")); // 0
        c.add(key(10, "AAAA")); // 1
        c.add(key(20, "CCCC")); // 2
        c.add(key(10, "AAAA")); // 3
        c.add(key(10, "AAAA")); // 4
        let g = c.resolve(1);
        assert_eq!(g.units, 5);
        assert_eq!(g.removed, 3);
        assert_eq!(g.duplicate_groups, 1);
        assert_eq!(g.umis_merged, 1);
        assert_eq!(g.merged_groups, 1);
        assert_eq!(g.merged_positions, 1);
        let kept: Vec<u64> = (0..5).filter(|&i| g.keep.contains(i)).collect();
        assert_eq!(kept, vec![0, 2]);
    }

    #[test]
    fn resolve_with_zero_mismatches_matches_exact_dedup_counts() {
        let mut c = UmiGroupCollector::new();
        for (s, u) in [(10, "AAAA"), (10, "AAAT"), (10, "AAAA"), (20, "AAAA")] {
            c.add(key(s, u));
        }
        let g = c.resolve(0);
        assert_eq!((g.units, g.removed, g.duplicate_groups), (4, 1, 1));
        assert_eq!(g.umis_merged, 0);
        let kept: Vec<u64> = (0..4).filter(|&i| g.keep.contains(i)).collect();
        assert_eq!(kept, vec![0, 1, 3]);
    }

    #[test]
    fn resolve_on_empty_input() {
        let g = UmiGroupCollector::new().resolve(1);
        assert_eq!((g.units, g.removed, g.duplicate_groups), (0, 0, 0));
        assert!(g.keep.is_empty());
    }
}
