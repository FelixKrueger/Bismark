//! In-memory aggregation of methylation calls into per-position counts,
//! plus chromosome ordering — the most byte-identity-load-bearing module
//! (SPEC §2.1, decision D3).
//!
//! Replaces Perl's pre-split-to-temp-files + per-file `sort -k4,4n` +
//! sequential tally (`bismark2bedGraph:160-506`) with a direct in-memory
//! equivalent. Output is identical because Perl aggregates all calls at a
//! given position regardless of the order equal-position lines were sorted
//! in, so we need only reproduce the per-position counts and the chromosome
//! ordering — not `sort`'s tie-breaking.
//!
//! ## Chromosome ordering (SPEC §2.1B, review finding C1) — TWO steps
//!
//! 1. **Ownership (process / argv order):** the first input file (in the
//!    order [`add`](Aggregator::add) is called — i.e. argv order) to emit a
//!    call for a chromosome *owns* it. `bismark2bedGraph` does NOT sort its
//!    input files; ownership tracks argv order, not a hardcoded strand
//!    order. The owner's basename forms the temp-filename prefix.
//! 2. **Output order:** chromosomes are emitted in bytewise-ascending order
//!    of the synthetic temp-filename string
//!    `{owner}.chr{transformed_chr}.methXtractor.temp` (Perl `sort
//!    @temp_files`, `:316`), where `transformed_chr` maps `|`/`/` → `_`
//!    (`:271-272`). The **output** chromosome name is the original
//!    (untransformed) string.
//!
//! ## Memory (SPEC §1.1 D3 / review finding I3)
//!
//! The counts map holds every covered `(chr, pos)` at once. For human/mouse
//! CpG (~28M positions) this is sub-GB. A full `--CX` WGBS run (~0.6–1.1 B
//! positions) is ~30–50 GB — and this is strictly more memory-hungry than
//! Perl's default, which spills to per-chr temp files. v1 does not spill
//! (external merge-sort is the documented future fix, SPEC §9). Note: true
//! allocator exhaustion aborts the process (Rust's default), so v1 cannot
//! turn OOM into a clean error — the ceiling is documented, not guarded.

use rustc_hash::FxHashMap;

/// One chromosome's emitted data: its (original, untransformed) name and its
/// `(pos, meth, unmeth)` rows in ascending position order.
pub type ChrPositions = (Box<str>, Vec<(u32, u32, u32)>);

/// Per-chromosome metadata: the original (output) name and the owning
/// per-context file basename. The bytewise ordering key is built lazily from
/// `owner` at [`Aggregator::into_sorted`] time (rather than eagerly at intern
/// time) so the owner can be revised by [`Aggregator::add_min_owner`] before
/// emission. The file-reading path never revises it (first-touch), so its
/// output is unchanged.
struct ChrMeta {
    original: Box<str>,
    owner: Box<str>,
}

/// Accumulates `(chr, pos) → (methylated, unmethylated)` counts and tracks
/// chromosome ownership for the byte-identity ordering.
#[derive(Default)]
pub struct Aggregator {
    /// chr string → interned id.
    chr_ids: FxHashMap<Box<str>, u32>,
    /// id-indexed metadata.
    chrs: Vec<ChrMeta>,
    /// (chr_id, pos) → (meth, unmeth).
    counts: FxHashMap<(u32, u32), (u32, u32)>,
}

/// Build the synthetic Perl temp-filename ordering key for a chromosome
/// owned by `source_basename`. Mirrors `bismark2bedGraph:276` with the
/// `|`/`/` → `_` transform (`:271-272`).
fn order_key(source_basename: &str, chr: &str) -> Vec<u8> {
    let mut transformed = String::with_capacity(chr.len());
    for c in chr.chars() {
        transformed.push(if c == '|' || c == '/' { '_' } else { c });
    }
    format!("{source_basename}.chr{transformed}.methXtractor.temp").into_bytes()
}

impl Aggregator {
    /// Fresh, empty aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a chromosome with **first-touch** ownership: the first file to
    /// emit a call owns it (never reassigned). Used by [`add`](Self::add) — the
    /// standalone file-reading path, where argv order is the user's and must be
    /// honored verbatim. Returns the interned id.
    fn intern_first(&mut self, chr: &str, source_basename: &str) -> u32 {
        if let Some(&id) = self.chr_ids.get(chr) {
            return id;
        }
        self.push_chr(chr, source_basename)
    }

    /// Intern a chromosome with **minimum-basename** ownership: the owner is
    /// revised whenever a lexicographically-smaller basename emits a call for
    /// it. Used by [`add_min_owner`](Self::add_min_owner) — the extractor's
    /// streaming tee, where calls arrive in BAM/read order but the per-context
    /// files are passed to bedGraph in lexicographically-sorted order, so the
    /// owner is the smallest basename regardless of arrival order. Returns the
    /// interned id.
    fn intern_min(&mut self, chr: &str, source_basename: &str) -> u32 {
        if let Some(&id) = self.chr_ids.get(chr) {
            let meta = &mut self.chrs[id as usize];
            if source_basename < meta.owner.as_ref() {
                meta.owner = source_basename.into();
            }
            return id;
        }
        self.push_chr(chr, source_basename)
    }

    /// Register a never-before-seen chromosome with the given initial owner.
    fn push_chr(&mut self, chr: &str, source_basename: &str) -> u32 {
        let id = self.chrs.len() as u32;
        self.chrs.push(ChrMeta {
            original: chr.into(),
            owner: source_basename.into(),
        });
        self.chr_ids.insert(chr.into(), id);
        id
    }

    /// Increment the methylated/unmethylated count for `(chr_id, pos)`
    /// (Perl `:374-381`).
    fn bump(&mut self, id: u32, pos: u32, methylated: bool) {
        let entry = self.counts.entry((id, pos)).or_insert((0, 0));
        if methylated {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }

    /// Record one call with **first-touch** chromosome ownership. This is the
    /// file-reading path ([`run`](crate::run)): `source_basename` only matters
    /// the first time a chromosome is seen.
    pub fn add(&mut self, chr: &str, pos: u32, methylated: bool, source_basename: &str) {
        let id = self.intern_first(chr, source_basename);
        self.bump(id, pos, methylated);
    }

    /// Record one call with **minimum-basename** chromosome ownership, for the
    /// extractor's in-process streaming tee. Counts are order-free. Ownership
    /// resolves to the lexicographically-smallest `source_basename` seen for the
    /// chromosome.
    ///
    /// This is **byte-identical to the file-reading path** as long as the caller
    /// passes its per-context files in lexicographically-sorted (basename) order
    /// — which the extractor guarantees (it sorts the kept set; SPEC D3/D6). The
    /// file path assigns ownership by *first-touch in argv/read order*, and
    /// reading basename-sorted files makes that first-touch owner the
    /// smallest-basename emitter — exactly what this method picks. The
    /// equivalence holds **even when one basename is a prefix of another**,
    /// because both paths order by the same basename byte-comparison (the file
    /// path via its sorted argv; this method via `<`) — not by the full
    /// `order_key`. Feeding calls in arbitrary BAM/read order is therefore safe.
    pub fn add_min_owner(&mut self, chr: &str, pos: u32, methylated: bool, source_basename: &str) {
        let id = self.intern_min(chr, source_basename);
        self.bump(id, pos, methylated);
    }

    /// Consume the aggregator and yield chromosomes in Perl output order,
    /// each with its positions sorted ascending. Each tuple is
    /// `(pos, meth, unmeth)`. The bytewise ordering key is built here (lazily)
    /// from each chromosome's resolved owner basename.
    #[must_use]
    pub fn into_sorted(self) -> Vec<ChrPositions> {
        let Aggregator { chrs, counts, .. } = self;
        let n = chrs.len();

        // Bucket positions by chromosome.
        let mut per_chr: Vec<Vec<(u32, u32, u32)>> = vec![Vec::new(); n];
        for ((chr_id, pos), (meth, unmeth)) in counts {
            per_chr[chr_id as usize].push((pos, meth, unmeth));
        }
        // Within each chromosome: ascending position (Perl `sort -k4,4n`).
        for v in per_chr.iter_mut() {
            v.sort_unstable_by_key(|&(pos, _, _)| pos);
        }

        // Build each chromosome's ordering key from its resolved owner, then
        // emit in bytewise key order (Perl `sort @temp_files`).
        let keys: Vec<Vec<u8>> = chrs
            .iter()
            .map(|m| order_key(&m.owner, &m.original))
            .collect();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| keys[a].cmp(&keys[b]));

        order
            .into_iter()
            .map(|i| (chrs[i].original.clone(), std::mem::take(&mut per_chr[i])))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(sorted: &[ChrPositions]) -> Vec<String> {
        sorted.iter().map(|(c, _)| c.to_string()).collect()
    }

    #[test]
    fn ascii_chr_ordering_chr10_before_chr2() {
        let mut agg = Aggregator::new();
        for chr in ["chr1", "chr2", "chr10", "chrMT", "chrX"] {
            agg.add(chr, 10, true, "CpG_OT_s.txt");
        }
        // ASCII order of "chr{n}": chr1, chr10, chr2, chrMT, chrX.
        assert_eq!(
            names(&agg.into_sorted()),
            vec!["chr1", "chr10", "chr2", "chrMT", "chrX"]
        );
    }

    #[test]
    fn cross_file_position_merge() {
        let mut agg = Aggregator::new();
        agg.add("chr1", 100, true, "CpG_OT_s.txt"); // + → meth
        agg.add("chr1", 100, false, "CpG_OB_s.txt"); // - → unmeth
        let sorted = agg.into_sorted();
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].1, vec![(100, 1, 1)]);
    }

    #[test]
    fn make_or_break_chr_only_in_later_file() {
        // C1, anchored to verified Perl ground truth (argv order
        // [CpG_OT, CpG_OB], Ensembl chr names "1","2","MT"):
        //   r1 + 1 5 Z | r2 + 2 5 Z   (CpG_OT)
        //   r3 - 2 6 z | r4 - MT 5 z   (CpG_OB)
        // Perl emits chromosome order MT, 1, 2 because the owner basename
        // prefixes the sort key and "CpG_OB…" < "CpG_OT…" (B < T) — so the
        // OB-owned chromosome (MT) sorts FIRST, not last. (1 and 2 are
        // owned by OT; 2's later OB call merely merges into 2.)
        let mut agg = Aggregator::new();
        agg.add("1", 5, true, "CpG_OT_test.txt"); // + → meth
        agg.add("2", 5, true, "CpG_OT_test.txt");
        agg.add("2", 6, false, "CpG_OB_test.txt"); // - → unmeth; 2 owned by OT
        agg.add("MT", 5, false, "CpG_OB_test.txt"); // - → unmeth; MT owned by OB
        let sorted = agg.into_sorted();
        assert_eq!(names(&sorted), vec!["MT", "1", "2"]);
        // Counts match the Perl coverage output exactly.
        assert_eq!(sorted[0].1, vec![(5, 0, 1)]); // MT
        assert_eq!(sorted[1].1, vec![(5, 1, 0)]); // 1
        assert_eq!(sorted[2].1, vec![(5, 1, 0), (6, 0, 1)]); // 2
    }

    #[test]
    fn ownership_is_argv_order_not_strand_order() {
        // Same inputs, reversed argv order [CpG_OB, CpG_OT]. Now OB owns
        // chr1; OT owns chrMT (only in OT here). Demonstrates ownership
        // tracks argv order, not a hardcoded OT/OB precedence.
        let mut agg = Aggregator::new();
        agg.add("chr1", 5, true, "CpG_OB_s.txt"); // OB owns chr1
        agg.add("chrMT", 5, true, "CpG_OT_s.txt"); // OT owns chrMT
        // Keys: "CpG_OB_s.txt.chr1..." vs "CpG_OT_s.txt.chrMT...".
        // "CpG_OB" < "CpG_OT", so chr1 first.
        assert_eq!(names(&agg.into_sorted()), vec!["chr1", "chrMT"]);
    }

    #[test]
    fn pipe_slash_chr_orders_transformed_but_outputs_original() {
        let mut agg = Aggregator::new();
        // "chr|1" orders as if "chr_1" but must output "chr|1".
        agg.add("chr|1", 5, true, "CpG_OT_s.txt");
        let sorted = agg.into_sorted();
        assert_eq!(sorted[0].0.as_ref(), "chr|1");
    }

    #[test]
    fn within_chromosome_positions_ascending() {
        let mut agg = Aggregator::new();
        for pos in [1000u32, 9, 100] {
            agg.add("chr1", pos, true, "CpG_OT_s.txt");
        }
        let sorted = agg.into_sorted();
        let positions: Vec<u32> = sorted[0].1.iter().map(|&(p, _, _)| p).collect();
        assert_eq!(positions, vec![9, 100, 1000]);
    }

    #[test]
    fn empty_aggregator_yields_nothing() {
        let agg = Aggregator::new();
        assert!(agg.into_sorted().is_empty());
    }

    #[test]
    fn counts_accumulate_per_position() {
        let mut agg = Aggregator::new();
        agg.add("chr1", 50, true, "f.txt");
        agg.add("chr1", 50, true, "f.txt");
        agg.add("chr1", 50, false, "f.txt");
        let sorted = agg.into_sorted();
        assert_eq!(sorted[0].1, vec![(50, 2, 1)]);
    }

    // ── add_min_owner (streaming tee) — SPEC D6, promoted from the spike ──

    #[test]
    fn add_min_owner_matches_basename_sorted_file_order() {
        // The extractor passes per-context files in lexicographic (basename)
        // order, so the file path reads CpG_OB before CpG_OT and the owner is
        // the MIN basename. The streaming tee must reproduce that even though
        // calls arrive in BAM/read order.
        //
        // ORACLE: add() (first-touch) in basename-sorted order [CpG_OB, CpG_OT].
        let mut oracle = Aggregator::new();
        oracle.add("2", 100, false, "CpG_OB_s.txt"); // OB read first → OB owns 2
        oracle.add("2", 100, true, "CpG_OT_s.txt");
        oracle.add("1", 50, true, "CpG_OT_s.txt"); // OT owns 1
        oracle.add("MT", 5, false, "CpG_OB_s.txt"); // OB owns MT
        let oracle_sorted = oracle.into_sorted();

        // TEE: add_min_owner in BAM/read order — chr "2" first seen via OT.
        let mut tee = Aggregator::new();
        tee.add_min_owner("2", 100, true, "CpG_OT_s.txt"); // OT first in read order
        tee.add_min_owner("2", 100, false, "CpG_OB_s.txt"); // OB later → becomes min owner
        tee.add_min_owner("1", 50, true, "CpG_OT_s.txt");
        tee.add_min_owner("MT", 5, false, "CpG_OB_s.txt");
        let tee_sorted = tee.into_sorted();

        // Byte-identical structure AND the same chromosome order.
        assert_eq!(tee_sorted, oracle_sorted);
        assert_eq!(names(&tee_sorted), vec!["2", "MT", "1"]);
    }

    #[test]
    fn add_min_owner_revises_owner_to_smaller_basename() {
        // X is first seen via OT, then via the smaller OB → ownership must flip
        // to OB. The order vs an OB-owned Y proves the flip happened.
        let mut agg = Aggregator::new();
        agg.add_min_owner("X", 10, true, "CpG_OT_s.txt"); // X first owned by OT
        agg.add_min_owner("X", 20, true, "CpG_OB_s.txt"); // smaller → X now owned by OB
        agg.add_min_owner("Y", 10, true, "CpG_OB_s.txt"); // Y owned by OB
        // With the flip both are OB-owned → chrX < chrY → [X, Y].
        // Without the flip X→OT (OT.chrX) would sort AFTER Y→OB (OB.chrY) → [Y, X].
        assert_eq!(names(&agg.into_sorted()), vec!["X", "Y"]);
    }

    #[test]
    fn first_touch_add_diverges_from_min_owner_in_read_order() {
        // The SAME read-order calls as the matches-file-order test, but via the
        // first-touch add(): chr "2" first seen via OT → OT owns 2, yielding a
        // DIFFERENT chromosome order. This is exactly why the streaming tee must
        // use add_min_owner, not add().
        let mut naive = Aggregator::new();
        naive.add("2", 100, true, "CpG_OT_s.txt"); // OT first-touch owns 2
        naive.add("2", 100, false, "CpG_OB_s.txt");
        naive.add("1", 50, true, "CpG_OT_s.txt");
        naive.add("MT", 5, false, "CpG_OB_s.txt");
        // OT owns 1 and 2; OB owns MT → keys OB.chrMT < OT.chr1 < OT.chr2.
        assert_eq!(names(&naive.into_sorted()), vec!["MT", "1", "2"]);
    }

    #[test]
    fn add_min_owner_prefix_basenames_match_basename_sorted_file_order() {
        // Even when one basename is a strict prefix of another — the case a
        // FULL-key comparison would order differently, since "!" (0x21) sorts
        // before "." (0x2e) — min-basename ownership matches the file path,
        // because the file path also reads files in basename byte-sorted order.
        // Oracle: add() over files in sorted order ["f", "f!"]; tee:
        // add_min_owner in reverse (read) order.
        let mut oracle = Aggregator::new();
        oracle.add("X", 1, true, "f"); // "f" sorts before "f!" → owns X
        oracle.add("X", 2, true, "f!");
        let oracle_sorted = oracle.into_sorted();

        let mut tee = Aggregator::new();
        tee.add_min_owner("X", 2, true, "f!"); // larger basename first (read order)
        tee.add_min_owner("X", 1, true, "f"); // smaller → owns X
        let tee_sorted = tee.into_sorted();

        assert_eq!(tee_sorted, oracle_sorted);
    }
}
