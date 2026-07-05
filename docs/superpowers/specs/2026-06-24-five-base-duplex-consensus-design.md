# 5-Base duplex-consensus — design (#787)

**Branch:** `research/illumina-5base` · **PR:** #1015 · **Date:** 2026-06-24
**Crate:** `bismark-aligner` (+ a tiny `bismark-io` helper)
**Status:** design approved (architecture A; two commits; SE-first)

## Goal

Add DRAGEN-style **duplex-consensus** base reconciliation to the opt-in 5-Base path:
group the two strands of one original molecule into a *duplex family*, then reconcile
the asymmetric 5mC→T signal **per molecule** instead of over a population pileup. This
is distinct from the already-shipped UMI-position dedup (`--five_base_umi_len`, which
collapses PCR copies of ONE strand) and from the already-shipped population
deconvolution (`--five_base_deconvolution`, which pileups across molecules).

Like every 5-Base feature this is **opt-in, never-silent, ground-truth-gated**, and
leaves the byte-frozen Bowtie2/HISAT2/minimap2 bisulfite paths untouched. Perl
v0.25.1 has no 5-Base path, so byte-identity-to-Perl does not apply.

## Honest scope on "vs DRAGEN"

A literal DRAGEN concordance test is **impossible here** and is NOT attempted:
DRAGEN is a proprietary FPGA with no reproducible reference output, and there is no
public raw 5-Base dataset (launched 2025-10-15; gated BaseSpace demo only). See
`plans/06232026_illumina-5base-support/GATE.md`. The validation is the established
substitute: a **synthetic ground-truth gate** against the real minimap2 (pinned
2.31-r1302). The external DRAGEN gate stays documented as PENDING with an exact
runbook for when a dataset is in hand.

## Decisions (locked in brainstorming)

1. **Architecture A** — post-alignment pure module mirroring `five_base_deconv.rs`;
   re-read the 5-Base BAM, group, reconcile. (Rejected: inline read-walk grouping;
   separate subcommand.)
2. **Two commits** — (1) duplex family pairing + duplex-aware deconvolution; (2)
   consensus collapse → consensus BAM.
3. **Duplex key** — `(chrom, ref_start, ref_end, canonical_umi)` with one OT + one OB
   member; UMI canonicalized for the nonrandom-duplex swap. UMI length reuses
   `--five_base_umi_len`.
4. **SE first**, PE a documented follow-up.
5. **Combined +/- XM string** (DRAGEN's undocumented quirk) is deliberately **NOT**
   reproduced; the consensus read carries a standard single-strand Bismark XM so the
   extractor/bedGraph/coverage2cytosine consume it unchanged.

## SE limitation (explicit)

In SE, the OT and OB members of one molecule sequence opposite ends of the fragment
and overlap only partially. Per-base reconciliation applies **only** to positions
covered by both members; positions seen by one strand fall back to single-strand
semantics (`Undetermined`). This is why duplex is naturally PE (the PE follow-up gets
both ends from R1/R2). The synthetic SE gate uses members long enough to overlap the
CpGs under test.

---

## Module: `five_base_duplex.rs` (pure, no I/O)

Feature-independent, no I/O; the BAM walk (driver) fills the counters. Mirrors
`five_base_deconv.rs`. Reuses `StrandPileup`/`classify` for the per-family verdict.

```rust
/// nonrandom-duplex UMI swap model. Default RevComp (top/bottom strands carry
/// complementary UMIs); Identity = same UMI on both members.
enum UmiSwap { Identity, RevComp }

/// Canonical UMI: min(umi, transform(umi)) byte-wise, so both duplex members hash equal.
fn canonical_umi(umi: &[u8], swap: UmiSwap) -> Vec<u8>;

struct DuplexKey { chrom: String, start: u32, end: u32, canon_umi: Vec<u8> }

/// One CpG observation a member contributes (reuses the deconv CIGAR walk).
struct SiteObs { pos0: u32, plus: bool, t_equivalent: bool }

struct DuplexFamily { ot: Vec<SiteObs>, ob: Vec<SiteObs> }  // by strand member

struct DuplexFamilies { fams: BTreeMap<DuplexKey, DuplexFamily> }
impl DuplexFamilies {
    fn observe(&mut self, key: DuplexKey, is_ot: bool, obs: SiteObs);
    /// Per family, per site covered by BOTH strands → StrandPileup → classify.
    fn reconcile(&self, min_opp_depth, variant_opp_frac) -> DuplexSummary;
    fn write_report<W: Write>(&self, w, ...) -> io::Result<DuplexSummary>;
}
```

`DuplexSummary { total_families, duplex_paired, singletons, variant_sites,
methylation_sites, undetermined_sites, methylated_calls, total_calls }`.

A family with ≥1 OT and ≥1 OB is **duplex-paired**; else a **singleton** (kept as
single-strand evidence, flagged). Per-site verdict reuses the existing asymmetric rule
(`StrandPileup::classify`) but the two strands now come from the SAME molecule.

## Carrying the UMI to the BAM pass — `RX` tag

The duplex pass re-reads the BAM, but the UMI lives in the original read (first
`umi_len` bases), not in the BAM today. Decision: when `--five_base_umi_len > 0`, write
the raw UMI as a standard **`RX:Z:`** aux tag on each 5-Base record at emit time. This
is the SAM-standard raw-UMI tag, makes the existing UMI dedup inspectable, and is a
byte-additive change on an already-non-byte-identical path.

- `bismark-io` gains a minimal `BismarkRecord::set_aux_string_tag(tag, value)` (or an
  `inner_mut()` escape hatch) to insert `RX` before write.
- `five_base_emit_record` / the PE emit gain an `Option<&[u8]> umi` param; when set,
  insert `RX`.
- Duplex pass reads `RX` from `inner.data()`; absent → span-only key + never-silent
  notice that multi-molecule collisions are possible.

---

## Commit 1 — `--five_base_duplex`

**CLI / config.** New `--five_base_duplex` (alias `--five_base_duplex_consensus`),
requires `--illumina_5base` (scope guard like the other 5-Base flags). PE + duplex →
clear "duplex PE is a follow-up (#787)" error. New `RunConfig.five_base_duplex: bool`.

**Behaviour.**
- `--five_base_duplex` alone → write `<out>.5base_duplex.txt`: header, then per
  duplex-paired family `chrom start end canon_umi members verdict methylated total %`,
  plus a summary footer (families, duplex-paired, singletons).
- `--five_base_duplex --five_base_deconvolution` → deconvolution aggregates **per
  family** (each paired family contributes one verdict per site) instead of the
  population pileup. The `<out>.5base_deconvolution.txt` column format is unchanged; a
  `# duplex-aware` header line records the provenance.

**Driver wiring.** New `run_five_base_duplex(genome, bam_path, report_path, umi_len,
swap, …)` next to the existing `run_five_base_deconvolution` call in `run_se_five_base`.
Same BAM walk + CIGAR traversal as deconv, but routes each obs into `DuplexFamilies`
keyed by span + `RX`.

**Tests (TDD).**
- Unit: `canonical_umi` (RevComp/Identity collapse the two members equal); family
  grouping (one OT + one OB at same key → paired; mismatched UMI → two singletons);
  per-family reconcile (5mC family → Methylation; homozygous C>T family → Variant;
  one-strand-only site → Undetermined).
- Driver/CLI: flag parse + alias + scope guard; PE+duplex rejection; `RX` written when
  `umi_len>0`.
- **Ground-truth gate** (`tests/five_base_groundtruth.rs`, real minimap2, no-op if
  absent): synthesize a molecule as two opposite-strand members with swapped UMIs
  (one 5mC CpG, one C>T CpG), align with `--illumina_5base --five_base_umi_len N
  --five_base_duplex --five_base_deconvolution`, assert the family pairs and the 5mC
  site → methylation while the C>T site → variant.

---

## Commit 2 — `--five_base_consensus`

**CLI / config.** New `--five_base_consensus`, **implies** `--five_base_duplex`
(grouping), requires `--illumina_5base`. SE only this PR. `RunConfig.five_base_consensus`.

**Behaviour.** A second pass emits ONE Bismark record per duplex-paired family into a
separate `<out>.5base_consensus.bam` (the per-read BAM stays as the default output):
- Consensus SEQ over the union of the two members' aligned span; at a position covered
  by both, reconcile bases (agreement → that base; disagreement → higher base-quality,
  tie → `N`); base quality = combined.
- Methylation: standard inverted 5-Base call on the consensus, with the **duplex
  verdict** applied — sites the family calls `Variant` are forced unmethylated (lower-
  case / masked), so genetic C>T does not leak into methylation. Singleton families
  are emitted (optionally) as single-strand records or skipped (flagged); default skip
  with a count.
- XM/XR/XG standard Bismark convention; XR/XG from the OT member. **No** combined +/-
  XM string (see Decisions §5).

**Driver wiring.** `run_five_base_consensus(genome, bam_path, consensus_bam_path, …)`
after `run_five_base_duplex`. Reuses the family grouping from commit 1 (shared module
fn); builds each consensus `BismarkRecord` and writes via `bismark-io`.

**Tests (TDD).**
- Unit: base reconciliation (agree/disagree/tie→N, quality combine); consensus XM with
  a `Variant` site forced unmethylated.
- **Ground-truth gate**: the same synthetic duplex molecule → exactly one consensus
  record per family; consensus SEQ matches the reconciled truth; the 5mC CpG → `Z`,
  the C>T CpG → not `Z` (masked); record count = number of paired families.

---

## Non-goals / follow-ups (documented, not built)

- **PE duplex** (both ends from R1/R2; the natural home for full per-base
  reconciliation) — clearly-scoped follow-up.
- **Combined +/- XM string** — deliberately not reproduced (undocumented ordering).
- **External DRAGEN concordance gate** — PENDING a real dataset; runbook in GATE.md.
- `--multicore` for the duplex passes; FASTA input.

## Files touched

- `rust/bismark-aligner/src/five_base_duplex.rs` (new) — pure module.
- `rust/bismark-aligner/src/lib.rs` — `run_five_base_duplex`, `run_five_base_consensus`,
  `RX` on emit, wiring in `run_se_five_base`.
- `rust/bismark-aligner/src/config.rs`, `src/options.rs`, `src/cli.rs` — flags, guards.
- `rust/bismark-io/src/record.rs` — minimal `set_aux_string_tag` / `inner_mut`.
- `rust/bismark-aligner/tests/five_base_groundtruth.rs`, `tests/cli.rs` — gates.
- `plans/06232026_illumina-5base-support/GATE.md` — status + honest DRAGEN note.
```
