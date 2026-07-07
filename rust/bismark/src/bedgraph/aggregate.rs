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

/// Per-chromosome metadata: the original (output) name, the owning
/// per-context file basename, and that owner's **creation rank** (its index
/// in the extractor's `mode_keys` creation order, which equals Perl's file
/// creation order `OT, CTOT, CTOB, OB` per context). The bytewise ordering
/// key is built lazily from `owner` at [`Aggregator::into_sorted`] time
/// (rather than eagerly at intern time) so the owner can be revised by
/// [`Aggregator::add_ranked`] before emission: a chromosome is owned by the
/// LOWEST-rank file that emits a call for it (matching Perl's first-in-creation-
/// order ownership). The file-reading path ([`Aggregator::add`]) never revises
/// the owner (first-touch), so its output is unchanged.
struct ChrMeta {
    original: Box<str>,
    /// Creation rank of the current owner — the `mode_keys` index of the
    /// owning file. Used only by [`Aggregator::add_ranked`] to decide whether
    /// a later (lower-rank) file should take over ownership. `u32::MAX` for
    /// chromosomes interned via the first-touch [`Aggregator::add`] path
    /// (no rank is supplied there; ownership is never revised anyway).
    owner_rank: u32,
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
    /// honored verbatim. The owner rank is `u32::MAX` (unused: ownership is
    /// never revised on this path). Returns the interned id.
    fn intern_first(&mut self, chr: &str, source_basename: &str) -> u32 {
        if let Some(&id) = self.chr_ids.get(chr) {
            return id;
        }
        self.push_chr(chr, u32::MAX, source_basename)
    }

    /// Intern a chromosome with **minimum creation-rank** ownership: the owner
    /// is revised whenever a file with a strictly lower creation rank emits a
    /// call for it. Used by [`add_ranked`](Self::add_ranked) — the extractor's
    /// streaming tee, where calls arrive in BAM/read order but the destination
    /// files are *created* in Perl's order (`OT, CTOT, CTOB, OB` per context).
    /// Perl owns a both-strand chromosome by the file FIRST in creation order,
    /// so the owner is the lowest-rank emitter regardless of arrival order or
    /// basename. Returns the interned id.
    fn intern_min_rank(&mut self, chr: &str, rank: u32, source_basename: &str) -> u32 {
        if let Some(&id) = self.chr_ids.get(chr) {
            let meta = &mut self.chrs[id as usize];
            if rank < meta.owner_rank {
                meta.owner_rank = rank;
                meta.owner = source_basename.into();
            }
            return id;
        }
        self.push_chr(chr, rank, source_basename)
    }

    /// Register a never-before-seen chromosome with the given initial owner
    /// (basename + creation rank).
    fn push_chr(&mut self, chr: &str, owner_rank: u32, source_basename: &str) -> u32 {
        let id = self.chrs.len() as u32;
        self.chrs.push(ChrMeta {
            original: chr.into(),
            owner_rank,
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
    /// file-reading path ([`run`](crate::bedgraph::run)): `source_basename` only matters
    /// the first time a chromosome is seen.
    pub fn add(&mut self, chr: &str, pos: u32, methylated: bool, source_basename: &str) {
        let id = self.intern_first(chr, source_basename);
        self.bump(id, pos, methylated);
    }

    /// Record one call with **minimum creation-rank** chromosome ownership, for
    /// the extractor's in-process streaming tee. Counts are order-free.
    /// Ownership resolves to the file with the lowest `rank` (its index in the
    /// extractor's `mode_keys` creation order) seen for the chromosome.
    ///
    /// This is **byte-identical to Perl's file-reading path** because Perl hands
    /// `bismark2bedGraph` the per-context files in *creation* order (`OT, CTOT,
    /// CTOB, OB` per context; `bismark_methylation_extractor:5156-5225`, NO
    /// sort) and assigns ownership by first-touch in that argv order. The
    /// extractor's `mode_keys` order *is* Perl's creation order, so the lowest
    /// rank a chromosome is touched by here equals the first-in-creation-order
    /// file Perl would intern from. (The earlier `add_min_owner` rule used the
    /// *minimum basename* instead — `CpG_OB` < `CpG_OT` — which diverged from
    /// Perl, since `CpG_OB` is created *last*, not first. See the crate
    /// CHANGELOG.) Feeding calls in arbitrary BAM/read order is safe: the rank
    /// comparison is order-independent.
    pub fn add_ranked(
        &mut self,
        chr: &str,
        pos: u32,
        methylated: bool,
        rank: u32,
        source_basename: &str,
    ) {
        let id = self.intern_min_rank(chr, rank, source_basename);
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

    // ── add_ranked (streaming tee) — ownership = minimum creation rank ──
    //
    // The extractor tees calls with the destination file's `mode_keys`
    // creation rank (CpG_OT=0, CpG_CTOT=1, CpG_CTOB=2, CpG_OB=3 for the CpG
    // block under Default mode — Perl's file creation order). A chromosome is
    // owned by the LOWEST-rank file that emits a call for it, regardless of
    // basename or BAM arrival order. This matches Perl, which hands
    // bismark2bedGraph the files in creation order and owns by first-touch.

    #[test]
    fn add_ranked_owns_by_min_rank_not_min_basename() {
        // chr "2" is first seen via CpG_OB (rank 3) in BAM order, then via
        // CpG_OT (rank 0). Min-RANK ownership picks CpG_OT (rank 0) — the
        // OPPOSITE of the old min-basename rule, which would have picked
        // CpG_OB ("CpG_OB" < "CpG_OT"). The Perl-correct ORACLE reads files in
        // creation order [CpG_OT, CpG_OB] with first-touch add().
        //
        // ORACLE: add() (first-touch) in CREATION order [CpG_OT, CpG_OB].
        let mut oracle = Aggregator::new();
        oracle.add("1", 50, true, "CpG_OT_s.txt"); // OT owns 1
        oracle.add("2", 100, true, "CpG_OT_s.txt"); // OT first-touch owns 2
        oracle.add("2", 100, false, "CpG_OB_s.txt"); // OB later → 2 stays OT-owned
        oracle.add("MT", 5, false, "CpG_OB_s.txt"); // OB owns MT
        let oracle_sorted = oracle.into_sorted();

        // TEE: add_ranked in BAM/read order — chr "2" first seen via OB (rank
        // 3) but a later OT (rank 0) call must take over ownership.
        let mut tee = Aggregator::new();
        tee.add_ranked("2", 100, false, 3, "CpG_OB_s.txt"); // OB (rank 3) first in read order
        tee.add_ranked("2", 100, true, 0, "CpG_OT_s.txt"); // OT (rank 0) later → becomes owner
        tee.add_ranked("1", 50, true, 0, "CpG_OT_s.txt");
        tee.add_ranked("MT", 5, false, 3, "CpG_OB_s.txt");
        let tee_sorted = tee.into_sorted();

        // Byte-identical structure AND the same chromosome order.
        // Owners: 1→CpG_OT, 2→CpG_OT (min rank 0), MT→CpG_OB → keys
        // CpG_OB.chrMT < CpG_OT.chr1 < CpG_OT.chr2 → order [MT, 1, 2].
        assert_eq!(tee_sorted, oracle_sorted);
        assert_eq!(names(&tee_sorted), vec!["MT", "1", "2"]);
    }

    #[test]
    fn add_ranked_revises_owner_to_lower_rank() {
        // X is first seen via the HIGHER-rank CpG_OB (3), then via the
        // LOWER-rank CpG_OT (0) → ownership must flip to CpG_OT despite
        // "CpG_OT" being the LARGER basename. The order vs an OB-only Y
        // (rank 3) proves the flip happened.
        let mut agg = Aggregator::new();
        agg.add_ranked("X", 10, true, 3, "CpG_OB_s.txt"); // X first owned by OB (rank 3)
        agg.add_ranked("X", 20, true, 0, "CpG_OT_s.txt"); // lower rank → X now owned by OT
        agg.add_ranked("Y", 10, true, 3, "CpG_OB_s.txt"); // Y owned by OB
        // With the flip: X→CpG_OT (key CpG_OT.chrX), Y→CpG_OB (key CpG_OB.chrY)
        // → "CpG_OB…" < "CpG_OT…" → [Y, X].
        // Without the flip X would stay CpG_OB → CpG_OB.chrX < CpG_OB.chrY →
        // [X, Y]. The min-rank flip therefore yields [Y, X].
        assert_eq!(names(&agg.into_sorted()), vec!["Y", "X"]);
    }

    #[test]
    fn add_ranked_does_not_revise_to_higher_rank() {
        // Mirror of the matches-file-order test via the first-touch add():
        // chr "2" first seen via CpG_OT (rank 0) and a later higher-rank
        // CpG_OB (rank 3) must NOT take over. Demonstrates that arrival of a
        // HIGHER-rank file never revises ownership.
        let mut agg = Aggregator::new();
        agg.add_ranked("2", 100, true, 0, "CpG_OT_s.txt"); // OT (rank 0) owns 2
        agg.add_ranked("2", 100, false, 3, "CpG_OB_s.txt"); // OB (rank 3) later → ignored
        agg.add_ranked("1", 50, true, 0, "CpG_OT_s.txt");
        agg.add_ranked("MT", 5, false, 3, "CpG_OB_s.txt");
        // OT owns 1 and 2; OB owns MT → keys CpG_OB.chrMT < CpG_OT.chr1 <
        // CpG_OT.chr2 → [MT, 1, 2].
        assert_eq!(names(&agg.into_sorted()), vec!["MT", "1", "2"]);
    }

    #[test]
    fn add_ranked_prefix_basenames_owned_by_min_rank() {
        // Even when one basename is a strict prefix of another — the case a
        // basename byte-comparison would order differently, since "!" (0x21)
        // sorts before "." (0x2e) — ownership follows the creation RANK, not
        // the basename. X is first seen via "f!" at rank 1, then via "f" at
        // rank 0 → owner is "f" (rank 0). The ORACLE reads files in creation
        // order [f (rank 0), f! (rank 1)] with first-touch add().
        let mut oracle = Aggregator::new();
        oracle.add("X", 1, true, "f"); // rank-0 file read first → owns X
        oracle.add("X", 2, true, "f!");
        let oracle_sorted = oracle.into_sorted();

        let mut tee = Aggregator::new();
        tee.add_ranked("X", 2, true, 1, "f!"); // higher-rank file first (read order)
        tee.add_ranked("X", 1, true, 0, "f"); // lower rank → owns X
        let tee_sorted = tee.into_sorted();

        assert_eq!(tee_sorted, oracle_sorted);
        // Sanity: the resolved owner is "f" (rank 0), so the order key is
        // "f.chrX…", not "f!.chrX…".
        assert_eq!(names(&tee_sorted), vec!["X"]);
    }
}
