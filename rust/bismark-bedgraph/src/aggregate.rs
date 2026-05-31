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

/// Per-chromosome metadata: the original (output) name and the bytewise
/// ordering key (the synthetic Perl temp filename).
struct ChrMeta {
    original: Box<str>,
    order_key: Vec<u8>,
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

    /// Intern a chromosome, recording its owner on first sight (never
    /// reassigned). Returns the interned id.
    fn intern(&mut self, chr: &str, source_basename: &str) -> u32 {
        if let Some(&id) = self.chr_ids.get(chr) {
            return id;
        }
        let id = self.chrs.len() as u32;
        self.chrs.push(ChrMeta {
            original: chr.into(),
            order_key: order_key(source_basename, chr),
        });
        self.chr_ids.insert(chr.into(), id);
        id
    }

    /// Record one call. `methylated` increments the methylated count, else
    /// the unmethylated count (Perl `:374-381`). `source_basename` only
    /// matters the first time a chromosome is seen (ownership).
    pub fn add(&mut self, chr: &str, pos: u32, methylated: bool, source_basename: &str) {
        let id = self.intern(chr, source_basename);
        let entry = self.counts.entry((id, pos)).or_insert((0, 0));
        if methylated {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }

    /// Consume the aggregator and yield chromosomes in Perl output order,
    /// each with its positions sorted ascending. Each tuple is
    /// `(pos, meth, unmeth)`.
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

        // Chromosome emission order: bytewise sort of the ordering keys
        // (Perl `sort @temp_files`).
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| chrs[a].order_key.cmp(&chrs[b].order_key));

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
}
