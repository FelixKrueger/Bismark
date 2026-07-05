# `bismark-extractor` Phase B — SE extraction loop + XM routing + output-file map + splitting-report skeleton

**Status:** rev 2 — implementation complete (91 tests green, clippy + fmt clean) with dual-code-review nits folded in. Phase B is ready for merge / for Phase C to begin.
**Date:** 2026-05-26.
**Slug:** `plans/05262026_bismark-extractor/PHASE_B_PLAN.md`.
**Phase target:** SPEC §10 row B — ~800 LOC (rev 2 actual: ~1,100 LOC including tests).

## Revision history

- **rev 0** (2026-05-26): initial Phase B plan.
- **rev 2** (2026-05-26): post-implementation close-out folding both code-reviewers' Medium/Low findings + plan-manager coverage gaps.
  - **Reviewer A L1 / Reviewer B L1 (Medium)** — renamed `strand_char` → `meth_char` in `OutputFileMap::write_call` and rewrote the doc comment to accurately label the `+`/`-` column as a methylation-state indicator (not a strand char). Behaviour was already byte-correct per Perl 2911-2961; only the label drifted from the truth.
  - **Reviewer A E1 (Medium)** — widened `OutputFileMap::write_call` return type from `Result<(), io::Error>` to `Result<(), BismarkExtractorError>`. The previously-unreachable `.expect("missing key")` panic is now a typed `InternalError`, matching the plan §5.3 contract. Propagated through `route_call`'s signature.
  - **Reviewer A E2 / Reviewer B Err2 (Low)** — replaced `pipeline.rs::extract_se`'s `reference_sequence_id().expect()` with a typed `InternalError` path (matches `bismark-dedup/src/pipeline.rs:224` precedent).
  - **Reviewer B E2 (Low)** — dropped `reader.header().clone()` in `extract_se`; now passes `reader.header()` by reference to `build_chr_name_table`. One Header clone saved per run.
  - **Reviewer B L4 / S4 (Nit)** — trimmed the speculative "so the report can name accurate file sizes" doc-comment on `OutputFileMap::flush_all` (it forward-promised a behaviour that won't ship).
  - **Plan-manager T-27 / Reviewer A S1 / Reviewer B L3 (Low) — RESOLVED.** Added `cleanup_partial_outputs_continues_past_one_failure` test in `tests/se_phase_b.rs` (pre-deletes one file so `cleanup_all` hits `Err` on that path; asserts the other 11 still removed). Closes the plan §7.1 gap.
  - **Plan-manager T-40 — DEVIATION DOCUMENTED.** Plan §7.1 promised `extract_se_two_records_route_to_different_files` as a unit test. Actual implementation covers this via the smoke test `smoke_se_directional_produces_all_12_files_and_report` (3 OT + 2 OB records exercising multi-record routing end-to-end through the binary). Smoke coverage is functionally stronger than the unit-level test would have been; no separate unit test added.
  - **Plan-manager Item 43 — DEVIATION DOCUMENTED.** Plan §3.2 / §7.3 listed `tests/data/regenerate.sh` + `tests/data/se_directional_phase_b.bam` + `tests/data/README.md` as committed deliverables. Actual implementation builds BAMs in-process via `bismark_io::BamWriter::from_path` from `tests/se_phase_b_smoke.rs`. **Rationale for deviation:** fewer binary blobs in the repo, fully reproducible from code, no toolchain dependency. The smoke test serves the same role (end-to-end binary exercise without Perl baseline) without the file-system artefacts. If Phase H needs a committed BAM for the 10M/55M byte-identity gate, it can add one then.
  - **Reviewer B Phase-F items (E1 lazy QNAME, L2 BismarkIo rename, E3 scratch-buffer write)** — out of scope for Phase B rev 2; tracked for Phase F profiling per the plan §13 follow-up list.
  - **Verification (rev 2):** `cargo test -p bismark-extractor` → 91 tests pass (40 lib + 4 sanity + 44 se_phase_b + 3 smoke). `cargo clippy --all-targets -- -D warnings` clean. `cargo fmt --check` clean.
- **rev 1** (2026-05-26): folded findings from dual plan-review.
  - **C1 (B, Critical) — RESOLVED.** Verified against Perl source lines 5140-5325 (`--merge_non_CpG` branch) + 5405-5700+ (default branch): Perl opens **all 12** strand×context files eagerly via `open(...) unless($mbias_only)` and writes the version header immediately via `print ... unless($no_header) unless($mbias_only)`. The SPEC §8.4 "0-byte (Perl) or absent (Rust)" framing was factually wrong about Perl. Phase B switches to **eager-open with immediate header write**, re-frames Alan's "spurious CTOT/CTOB" structural fix as `record_strand` correctness (no record's calls split across files), and flags SPEC §8.4 for a corrective edit (separate task, not Phase B implementation).
  - **I1 (B, Important).** `read_pos_5p` from `iter_aligned` **includes soft-clip positions in the count** (verified at `bismark-io::CigarExt::aligned_positions` cigar.rs:131-138, plus `iter_aligned` filter at record.rs:284). Plan §4.5 + §7.1 test prose updated.
  - **I3 (B, Important).** `mbias_only_silence` kernel param dropped from Phase B (deferred to Phase E). Reduces `extract_calls` signature to 3 args; the conditional `die`-vs-skip on invalid XM moves to Phase E.
  - **I4 (B, Important).** `route_call` reorders: M-bias → splitting-report counters → `mbias_only` short-circuit → file write. Documented as a SPEC §7.5 deviation (Perl truth: counters accumulate even under `--mbias_only`); follow-up task queued to fix SPEC §7.5.
  - **I5 (B, Important).** Added an end-to-end smoke test that does NOT require the Perl toolchain (synthetic BAM → run binary → assert file set + non-emptiness + parseable splitting report).
  - **A Important #1.** `record.inner().flags().bits()` → `u16::from(record.inner().flags())` (noodles API correctness).
  - **A Important #2 / B Optional O2 — RESOLVED.** Perl suffix-strip is case-sensitive single-extension (`s/sam$/txt/`, `s/bam$/txt/`, `s/cram$/txt/`); Rust `derive_basename` matches. No `.bam.gz` handling.
  - **A Important #3-#6.** Added missing tests: per-record PAIRED-flag rejection; `--multiple` files rejection; mixed-context routing on a single record (closes Alan's split-across-files bug at unit level); literal-header-bytes assertion.
  - **A Important #7.** `finalize`-failure invariant stated in §5.4.
  - **A Important #8.** `BismarkStrand`-from-XR/XG vs Perl strand-classification documented as a Phase H assumption.
  - **A Important #9.** Defensive comment for `extract_calls`: must use `aligned.xm_byte`, never `record.xm()[read_pos_5p]`.
  - **B Optional O1.** `build_chr_name_table` asserts ASCII chr names + errors loudly on non-ASCII.
  - **B Optional O4.** `OutputFileMap` collapses `fhs`+`paths` into one `HashMap<OutputKey, (PathBuf, BufWriter<File>)>`.
  - **B Optional O5.** `extract_calls` uses `Vec::new()` (no preallocation heuristic).
  - **B "output_dir missing"** — `OutputFileMap::new` calls `std::fs::create_dir_all` defensively (Perl `make_path` precedent).
  - **A Optional #14 / B confirmation.** Locked `detect_paired_from_header` as Phase C work (already exists in `bismark-dedup/src/pipeline.rs:137`; promote to `bismark-io` when Phase C wires PE auto-detect).
  - **Verdict change**: rev 0 split (A: APPROVE-WITH-NITS, B: NEEDS-REVISIONS) → rev 1 awaiting confirmation.

## Epic linkage

- **Design contract:** `rust/bismark-extractor/SPEC.md` (in-repo, rev 2). Note: SPEC §8.4 row "Directional library" has a factual error about Perl's empty-file behavior; SPEC fix is queued as a separate task once Phase B implementation lands.
- **GitHub umbrella:** issue [#798](https://github.com/FelixKrueger/Bismark/issues/798) (bismark-extractor port).
- **Prior phases:** Phase A (workspace scaffold + CLI) merged at commit `144ca2d` (PR #847, closes #846).
- **Phase B will close:** the placeholder gap for the SE / default-mode / single-core / non-gzip subset of the pipeline.

## 1. Goal

Wire the first end-to-end extraction path: read a Bismark-aligned BAM/SAM/CRAM, classify each record's XM-tag bytes into methylation calls, route those calls to per-(context × strand) split files on disk, and emit a Perl-shaped `_splitting_report.txt`. After Phase B, the binary produces real output for an SE directional BAM in `OutputMode::Default` on a single core; PE, non-default modes, gzip, multicore, and the bedGraph/cytosine_report chain remain stubbed (rejected at the resolved-config boundary).

## 2. Scope decisions (locked, rev 1)

| Decision | Choice | Reasoning |
|----------|--------|-----------|
| Library mode | **SE only** | PE is Phase C. |
| Output mode | **`OutputMode::Default` only** | Other modes are Phase E. |
| Compression | **No `--gzip`** | Phase E. |
| Parallelism | **`--parallel 1` only** | Phase F. |
| Downstream chain | **No `--bedGraph` / `--cytosine_report`** | Phase G. |
| M-bias **accumulator** | **In Phase B** (per SPEC §7.5 step 1) | Cheap; structurally clean; avoids rewriting `route_call` at Phase D. |
| M-bias **writer** (`M-bias.txt`) | **Phase D** | Per SPEC §10 row D. |
| Splitting-report content | **Functional Perl-shaped emission** | Byte-identity gated at Phase H. |
| **Output-file creation** | **Eager (rev 1)** — open all 12 strand×context files at `OutputFileMap::new()` time + write header line immediately | Matches Perl source 5405-5700+ (default branch) / 5140-5325 (merge_non_CpG branch). Lazy was a rev 0 bug. |
| **`mbias_only_silence` kernel param** | **Deferred to Phase E** | Phase B rejects `--mbias_only` at main dispatch; the param would be dead code. |
| **Splitting-report counter ordering** | **Increment BEFORE `mbias_only` short-circuit** in `route_call` | SPEC §7.5 pseudocode shows counter AFTER short-circuit, which would break Perl byte-identity under `--mbias_only`. SPEC fix is a separate task. |
| Auto-detect path (SE vs PE) | **Defensive per-record PAIRED-flag check** in SE loop | Phase C promotes `detect_paired_from_header` from `bismark-dedup/src/pipeline.rs:137` to `bismark-io`. |

## 3. Context

### 3.1 Source documents read end-to-end

- `rust/bismark-extractor/SPEC.md` §§2, 3, 4, 5, 6.1–6.3, 6.5, 7.1, 7.2, 7.5, 7.7, 8.1, 8.4, 10, 11, 12, 14 (rev 2). **Note: SPEC §8.4 "Directional library" row needs correction — see Revision history.**
- `rust/bismark-extractor/src/{lib,cli,error,params,main}.rs` (Phase A state).
- `rust/bismark-io/src/{record,read,strand,cigar,pair}.rs` — in particular `BismarkRecord::iter_aligned` (record.rs:240-312), `BismarkRecord::record_strand`, `open_reader`, `AnyReader::records`, `AlignedXmCall`, `CigarExt::aligned_positions` (cigar.rs:131-138 — confirms `SoftClip` increments `read_pos`).
- `rust/bismark-dedup/src/pipeline.rs` — precedent for reader-opening, header → chr-name resolution (line 51, 71, 93, 194), and `detect_paired_from_header` (line 137).
- **Perl `bismark_methylation_extractor` directly read** for rev 1:
  - 5405-5700+: default-mode eager file-open + header-write (all 12 strand×context files).
  - 5140-5325: `--merge_non_CpG`-mode eager file-open + header-write.
  - 5148, 5151, 5159: `open(...) unless($mbias_only)` and `print "Bismark methylation extractor version $version\n" unless($no_header) unless($mbias_only)`.
  - 1619-1650 (SE `--ignore` semantics), 2970-2972 / 3052-3054 (invalid-XM `die`), 4245-4267 (soft-clip handling).

### 3.2 Code placement

All Phase B code lands inside `rust/bismark-extractor/`:

- **New modules**:
  - `src/call.rs` — `MethCall`, `CytosineContext`, `classify_xm_byte`, `extract_calls`.
  - `src/mbias.rs` — `MbiasTable`, `MbiasPos`. Accumulators only.
  - `src/output.rs` — `OutputFileMap` (eager-open), `OutputKey`, `SplittingReport`, `write_splitting_report`, `format_meth_line`.
  - `src/state.rs` — `ExtractState`.
  - `src/route.rs` — `route_call`.
  - `src/pipeline.rs` — `extract_se` SE main loop.
  - `src/header.rs` — refID → chr-name lookup builder (with ASCII assertion).
- **Modified modules**:
  - `src/lib.rs` — `pub mod` declarations + re-exports.
  - `src/main.rs` — replace placeholder `run()` with config-dispatch; reject non-SE / non-default / multicore / gzip / bedGraph / cytosine_report / multiple-inputs with `PhaseNotYetImplemented`.
  - `src/error.rs` — add `PhaseNotYetImplemented`, `InvalidXmByte`, `IoWrite (#[from] std::io::Error)`, `BismarkIo (#[from] BismarkIoError)`, `InternalError`, `NonAsciiChromosomeName`.
  - `src/params.rs` — left untouched in Phase B (see §9.2 #5).
- **Tests**:
  - `tests/sanity.rs` — extend with one SE smoke.
  - `tests/se_phase_b.rs` — unit tests enumerated below.
  - `tests/se_phase_b_smoke.rs` — end-to-end smoke (synthetic BAM, no Perl baseline; rev 1 addition).
  - `tests/data/regenerate.sh` — fixture-generation script committed even if baseline gen is deferred.
  - `tests/data/se_directional_phase_b.bam` — synthetic SE input (~50 reads with CpG + CHG + CHH motifs).

### 3.3 Crate version

`1.0.0-alpha.1` → `1.0.0-alpha.2`.

### 3.4 Binary behaviour

Today: validates flags, prints stderr "Phase A" note, exits 0.

After Phase B: validates flags → dispatches on `ResolvedConfig`. Supported subset runs `extract_se`; unsupported subset returns `PhaseNotYetImplemented { feature }`.

## 4. Behaviour specification

### 4.1 Inputs

- One positional `PathBuf`. `len > 1` → `PhaseNotYetImplemented { feature: "multiple input files (--multiple equivalent)" }`.
- Resolved CLI subset.

### 4.2 Outputs (rev 1: eager-open)

**12 split files** are created **unconditionally** at `OutputFileMap::new()` time (matching Perl). Each file gets the version header line on creation, gated by `!config.no_header` (Phase B's main dispatch rejects `--mbias_only` so the second Perl guard is implicitly true).

The 12 keys are the cross product `{CpG, CHG, CHH} × {OT, CTOT, CTOB, OB}`:

| Filename pattern | Always created? | Initial content |
|------------------|-----------------|-----------------|
| `CpG_OT_{basename}.txt`<br>`CpG_CTOT_{basename}.txt`<br>`CpG_CTOB_{basename}.txt`<br>`CpG_OB_{basename}.txt` | Yes | `Bismark methylation extractor version v0.25.1\n` (unless `--no_header`) |
| `CHG_{OT|CTOT|CTOB|OB}_{basename}.txt` | Yes | (same) |
| `CHH_{OT|CTOT|CTOB|OB}_{basename}.txt` | Yes | (same) |

For directional SE input (Bismark default), CTOT/CTOB files remain at 1 line (header only) for byte-identity with Perl baseline.

`{basename}` = input file with **case-sensitive single-suffix stripped**: `.bam` / `.sam` / `.cram`. No `.bam.gz` handling (matches Perl `s/bam$/txt/` etc.).

**Header line text** (`!config.no_header` case): literal `Bismark methylation extractor version v0.25.1\n`. The `$version` Perl variable is the Bismark suite version (currently `v0.25.1`); for Phase B's port we hardcode `v0.25.1` to lock byte-identity. If Bismark's Perl version ever bumps, the Rust port follows suit in the same release.

Each post-header line is tab-separated:
```
read_id<TAB>strand_char<TAB>chr<TAB>ref_pos<TAB>xm_byte<LF>
```

### 4.3 Splitting report

Written at `state.finalize()` time to `{output_dir}/{basename}_splitting_report.txt` unless `config.emit_splitting_report == false`.

- Parameter-summary block (one line per relevant CLI flag).
- `--fasta` annotation line iff `config.fasta_annotation == true`.
- Per-context counts.
- Percentage methylation per context (`%.2f` format; zero-denominator → `0.00`).

Phase B aims for "Perl-shaped"; exact byte-identity is Phase H.

### 4.4 M-bias accumulator (no writer in Phase B)

For every routed `MethCall` (subject to `!config.mbias_off`), increment one cell of `state.mbias[0]` (SE/R1 index). M-bias writer is Phase D.

### 4.5 Edge cases (rev 1)

| Case | Handling |
|------|----------|
| Empty input BAM | All 12 split files exist, each containing only the header line (matches Perl exactly — fixes rev 0). Splitting report still written; exit 0. |
| Coordinate-sorted input | `bismark-io` rejects upstream; extractor propagates. |
| Single record at `alignment_start == 1` | Handled by `iter_aligned`. |
| Soft-clipped boundary (`5S95M`) | **Rev 1 correction**: `iter_aligned` only **emits** AlignedXmCall items for matches (soft-clip filtered by `filter_map(|ap| ap.ref_offset?)` in record.rs:284), but the `read_pos_5p` field for the first emitted item is **5** (not 0) — because `CigarExt::aligned_positions` increments `read_pos` through soft-clip operations (cigar.rs:131-138) and `iter_aligned` does NOT renumber. The XM tag at the soft-clipped positions contains `.` per Bismark convention, so a hypothetical `xm[0..5]` lookup would yield non-call bytes anyway. **Net Phase B behavior matches Perl byte-identically** (Perl operates on `substr(meth_call, ignore)` over the full XM including soft-clip-corresponding `.` bytes). The ignore-region check `read_pos_5p >= ignore_5p && read_pos_5p < seq_len - ignore_3p` works because both bounds operate on the full read length (XM length). |
| Insertion / Deletion | Handled by `iter_aligned`. |
| `N` base in read | XM byte at that position is `.` → `SkipNonCytosine` → no call. |
| `--ignore` > seq_len | Saturating arithmetic + early-out. |
| `U` / `u` / `.` XM bytes | Silently skipped. |
| Invalid XM byte (e.g. `Q`) | `classify_xm_byte` returns `Err(InvalidXmByte)`. SE loop calls `state.cleanup_partial_outputs()` before propagating. **No `mbias_only_silence` kernel param in Phase B** (deferred to Phase E). |
| Unmapped record | Filtered upstream by `bismark-io`. |
| **PE record (PAIRED flag set) reaches SE pipeline** | On first such record: `state.cleanup_partial_outputs()` + return `PhaseNotYetImplemented { feature: "paired-end extraction (input has PAIRED flag set)" }`. Check via `u16::from(record.inner().flags()) & 0x1 != 0` (rev 1 API correction). |
| `output_dir` doesn't exist on disk | `OutputFileMap::new` calls `std::fs::create_dir_all(output_dir)?` defensively (rev 1 addition; matches Perl `make_path`). |
| Non-ASCII chromosome name in @SQ | `build_chr_name_table` returns `Err(NonAsciiChromosomeName { name })`. Loud failure — no silent UTF-8 substitution (rev 1 addition). |

## 5. Signatures (proposed, rev 1)

### 5.1 `call.rs`

```rust
//! Methylation-call classification + per-record extraction kernel.

use bismark_io::{AlignedXmCall, BismarkRecord};
use crate::error::BismarkExtractorError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CytosineContext { CpG = 0, CHG = 1, CHH = 2 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethCall {
    pub ref_pos: u32,
    pub read_pos: u32,   // 0-based 5'-oriented; INCLUDES soft-clip positions in the count (see §4.5)
    pub context: CytosineContext,
    pub methylated: bool,
    pub xm_byte: u8,
}

pub(crate) enum XmClassification {
    Call(CytosineContext, /*methylated*/ bool),
    SkipUnknownContext,
    SkipNonCytosine,
}

pub(crate) fn classify_xm_byte(
    byte: u8,
    ref_pos: u32,
    read_id: &str,
) -> Result<XmClassification, BismarkExtractorError>;

/// Extract all `MethCall`s from one record. Delegates to `record.iter_aligned()`.
///
/// **Invariant** (defensive comment in implementation): use `aligned.xm_byte`
/// from the iterator. NEVER re-index `record.xm()[read_pos_5p]` — for `-` strand
/// reads the XM byte at `read_pos_5p` does NOT equal the BAM-stored XM[read_pos_5p].
/// `iter_aligned` carries the orientation-corrected XM byte alongside `read_pos_5p`.
///
/// **Phase B**: invalid XM byte always returns `Err(InvalidXmByte)`. Phase E will
/// add a `mbias_only_silence: bool` kernel param to mirror Perl's conditional die
/// `die "..." unless ($mbias_only)`.
pub fn extract_calls(
    record: &BismarkRecord,
    ignore_5p: u32,
    ignore_3p: u32,
) -> Result<Vec<MethCall>, BismarkExtractorError>;
```

### 5.2 `mbias.rs`

Same as rev 0. `MbiasPos { meth: u64, unmeth: u64 }`, `MbiasTable { cpg, chg, chh: Vec<MbiasPos> }`, `accumulate(context, position_1based, methylated)`.

### 5.3 `output.rs` (rev 1: eager-open + single map)

```rust
use bismark_io::BismarkStrand;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::call::{CytosineContext, MethCall};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct OutputKey {
    pub context: CytosineContext,
    pub strand: BismarkStrand,
}

pub(crate) struct OutputFileMap {
    // Single map (rev 1: combines old `fhs` + `paths` to remove drift risk).
    files: HashMap<OutputKey, (PathBuf, BufWriter<File>)>,
}

impl OutputFileMap {
    /// Eagerly opens all 12 (context × strand) split files in `output_dir`.
    /// Writes the version header line to each file unless `no_header == true`.
    /// Creates `output_dir` via `create_dir_all` if missing.
    pub fn new(
        output_dir: &Path,
        input_basename: &str,
        no_header: bool,
    ) -> Result<Self, std::io::Error>;

    /// Append a `MethCall` line to the appropriate split file. Key MUST exist
    /// (eager-open guarantees this for the 12 standard SE/PE keys). Missing
    /// key is an `InternalError`.
    pub fn write_call(
        &mut self,
        record_name: &[u8],
        chr: &str,
        call: MethCall,
        strand: BismarkStrand,
    ) -> Result<(), std::io::Error>;

    /// Flush every writer (called from `finalize`).
    pub fn flush_all(&mut self) -> Result<(), std::io::Error>;

    /// On error paths: drop all writers + delete all 12 files (even header-only ones).
    /// Best-effort: one failed remove doesn't prevent others.
    pub fn cleanup_all(&mut self);
}

pub(crate) fn write_splitting_report(
    path: &Path,
    config: &ResolvedConfig,
    report: &SplittingReport,
) -> Result<(), std::io::Error>;

#[derive(Debug, Default)]
pub(crate) struct SplittingReport {
    pub records_processed: u64,
    pub calls_total: u64,
    pub calls_cpg_meth: u64, pub calls_cpg_unmeth: u64,
    pub calls_chg_meth: u64, pub calls_chg_unmeth: u64,
    pub calls_chh_meth: u64, pub calls_chh_unmeth: u64,
}
```

### 5.4 `state.rs`

```rust
pub(crate) struct ExtractState {
    pub mode: OutputMode,
    pub mbias_off: bool,
    pub mbias_only: bool,   // always false in Phase B (rejected at main dispatch)
    pub mbias: [MbiasTable; 2],
    pub fhs: OutputFileMap,
    pub report: SplittingReport,
}

impl ExtractState {
    /// Constructs `OutputFileMap` eagerly (12 files + headers).
    pub fn new(config: &ResolvedConfig, input_basename: &str) -> Result<Self, std::io::Error>;

    /// Flush all writers, then write splitting report.
    ///
    /// **Invariant (rev 1)**: `finalize` failure leaves the already-written split
    /// files in place on disk. The caller does NOT invoke `cleanup_partial_outputs`
    /// after a `finalize` failure — the records had already been successfully
    /// routed; failure here means the report write or final flush hit an I/O error
    /// after the data was on disk. Matches Perl's "die after writing" semantics.
    pub fn finalize(&mut self, config: &ResolvedConfig) -> Result<(), BismarkExtractorError>;

    /// Drop writers + remove all 12 files. Called from `extract_se`'s pre-finalize
    /// error paths (InvalidXmByte, PAIRED-flag-on-SE, reader error, route I/O error).
    pub fn cleanup_partial_outputs(&mut self);
}
```

### 5.5 `route.rs` (rev 1: counter ordering fixed)

```rust
/// Phase B routing order (deliberately deviates from SPEC §7.5 pseudocode —
/// see plan §13 #2 + revision history). Order:
///   1. Increment M-bias counter (unless `state.mbias_off`).
///   2. Increment splitting-report counters (unconditional — matches Perl).
///   3. If `state.mbias_only`: return early. (Phase B: never; reserved for E.)
///   4. Write split-file line.
pub(crate) fn route_call(
    state: &mut ExtractState,
    record: &BismarkRecord,
    chr: &str,
    strand: BismarkStrand,
    call: MethCall,
    read_identity: ReadIdentity,
) -> Result<(), std::io::Error>;
```

### 5.6 `pipeline.rs`

```rust
pub fn extract_se(
    input: &Path,
    config: &ResolvedConfig,
) -> Result<(), BismarkExtractorError> {
    let mut reader = open_reader(input, /*cram_ref=*/ None)?;
    let header = reader.header().clone();
    let chr_table = build_chr_name_table(&header)?;       // rev 1: errors on non-ASCII

    let input_basename = derive_basename(input);
    let mut state = ExtractState::new(config, &input_basename)?;   // rev 1: eager-creates 12 files + headers

    for record_result in reader.records() {
        let record = match record_result {
            Ok(r) => r,
            Err(e) => { state.cleanup_partial_outputs(); return Err(e.into()); }
        };

        // Rev 1: defensive PAIRED-flag check via `u16::from(flags)`.
        let flags_bits: u16 = record.inner().flags().into();
        if flags_bits & 0x1 != 0 {
            state.cleanup_partial_outputs();
            return Err(BismarkExtractorError::PhaseNotYetImplemented {
                feature: "paired-end extraction (input has PAIRED flag set)".to_string(),
            });
        }

        let refid = record.inner().reference_sequence_id()
            .expect("mapped record must have reference_sequence_id");
        let chr = chr_table.get(refid).ok_or_else(|| {
            BismarkExtractorError::InternalError { message: format!(
                "record refid {refid} out of range vs header (count {})", chr_table.len()
            )}
        })?;

        let strand = record.record_strand();
        let read_identity = ReadIdentity::from_flags(flags_bits);

        let calls = match extract_calls(&record, config.ignore_5p_r1, config.ignore_3p_r1) {
            Ok(c) => c,
            Err(e) => { state.cleanup_partial_outputs(); return Err(e); }
        };

        for call in calls {
            if let Err(io_err) = route_call(&mut state, &record, chr, strand, call, read_identity) {
                state.cleanup_partial_outputs();
                return Err(io_err.into());
            }
        }
        state.report.records_processed += 1;
    }

    state.finalize(config)?;
    Ok(())
}
```

### 5.7 `header.rs` (rev 1: ASCII-asserts)

```rust
pub(crate) fn build_chr_name_table(
    header: &Header,
) -> Result<Vec<String>, BismarkExtractorError> {
    let mut out = Vec::with_capacity(header.reference_sequences().len());
    for (name, _ref_seq) in header.reference_sequences() {
        let bytes: &[u8] = name.as_ref();
        if !bytes.is_ascii() {
            return Err(BismarkExtractorError::NonAsciiChromosomeName {
                name: String::from_utf8_lossy(bytes).into_owned(),
            });
        }
        // Safe: just validated ASCII.
        out.push(String::from_utf8(bytes.to_vec()).expect("ASCII verified above"));
    }
    Ok(out)
}
```

### 5.8 New error variants

```rust
#[error("not yet implemented in this build: {feature}")]
PhaseNotYetImplemented { feature: String },

#[error("invalid XM byte {byte:#04x} ({byte_char}) in read {read_id} at ref_pos {ref_pos}")]
InvalidXmByte { byte: u8, byte_char: char, ref_pos: u32, read_id: String },

#[error("output write failed: {0}")]
IoWrite(#[from] std::io::Error),

#[error(transparent)]
BismarkIo(#[from] bismark_io::BismarkIoError),

#[error("internal invariant violated: {message}")]
InternalError { message: String },

#[error(
    "chromosome name in @SQ header contains non-ASCII bytes: {name}. \
     Bismark output filenames + bedGraph/cytosine_report subprocesses require ASCII chr names."
)]
NonAsciiChromosomeName { name: String },
```

## 6. Implementation outline (rev 1)

1. **Add error variants** in `src/error.rs` (six new ones — see §5.8).
2. **Create `src/call.rs`**: `CytosineContext`, `MethCall`, `XmClassification`, `classify_xm_byte`, `extract_calls`. Loop body uses `record.iter_aligned()` directly; `Vec::new()` for collected calls (no preallocation heuristic). **Defensive comment**: "use `aligned.xm_byte`; NEVER re-index `record.xm()[read_pos_5p]` — `read_pos_5p` is 5'-oriented post-orientation-correction, while `record.xm()` is BAM-stored." Invalid XM byte → `Err(InvalidXmByte)` unconditionally (Phase B; Phase E adds the conditional skip).
3. **Create `src/mbias.rs`**: `MbiasPos`, `MbiasTable`, `MbiasTable::accumulate`.
4. **Create `src/output.rs`**:
   - `OutputKey { context, strand }`.
   - `OutputFileMap { files: HashMap<OutputKey, (PathBuf, BufWriter<File>)> }` (single map — rev 1).
   - `OutputFileMap::new(output_dir, input_basename, no_header)`:
     1. `std::fs::create_dir_all(output_dir)?` (rev 1).
     2. For each of 12 `(context, strand)` keys: compute filename `{context}_{strand}_{basename}.txt`, open `BufWriter::with_capacity(8 * 1024, File::create(path)?)`, write header line (`Bismark methylation extractor version v0.25.1\n`) unless `no_header`, store in map.
   - `write_call`: look up key (must exist — eager-open guarantees), write tab-separated row.
   - `flush_all`: iterate, call `flush()`.
   - `cleanup_all`: iterate, drop writer, `let _ = std::fs::remove_file(path);` (best-effort; eprintln on failure).
   - `format_meth_line(record_name, chr, call, strand) -> String` — tab-separated row.
   - `SplittingReport` struct + `write_splitting_report` writer.
5. **Create `src/state.rs`**: `ExtractState`, `::new` (calls `OutputFileMap::new`), `::finalize` (flush_all → write_splitting_report), `::cleanup_partial_outputs` (delegates to `fhs.cleanup_all`).
6. **Create `src/route.rs`**: `route_call` with rev-1 ordering (M-bias → counters → `mbias_only` short-circuit → write).
7. **Create `src/header.rs`**: `build_chr_name_table` with ASCII assertion (§5.7).
8. **Create `src/pipeline.rs`**: `extract_se` (§5.6) + `derive_basename(path: &Path) -> String`. **`derive_basename`**: extract file stem from `path.file_name()`; strip ONE of the three suffixes `.bam`/`.sam`/`.cram` from the end of the resulting string (case-sensitive; do nothing if none match). No double-suffix handling. Matches Perl `s/sam$/txt/; s/bam$/txt/; s/cram$/txt/`.
9. **Update `src/lib.rs`**: `pub mod` + re-exports.
10. **Update `src/main.rs::run`**: dispatch on `ResolvedConfig`. Five `PhaseNotYetImplemented` paths + `extract_se` for the supported subset. AutoDetect treated as SE (per-record check catches PE).
11. **Update `Cargo.toml`**: `version = "1.0.0-alpha.2"`.
12. **Leave `src/params.rs` untouched** — `ExtractParams` deferred to Phase C/D (see §9.2 #5).
13. **Write tests** (§7 below).
14. **Run `cargo test -p bismark-extractor && cargo clippy -p bismark-extractor -- -D warnings && cargo fmt --check`**.

## 7. Tests (rev 1: folded both reviewers' additions)

### 7.1 Unit tests (in `tests/se_phase_b.rs`)

| Test | Asserts |
|------|---------|
| `classify_xm_byte_classifies_all_six_methylation_bytes` | `Z`/`z`/`X`/`x`/`H`/`h` → `Call(ctx, m)`. |
| `classify_xm_byte_skips_U_u_dot` | `U`/`u`/`.` → skips. |
| `classify_xm_byte_rejects_invalid` | `Q` → `Err(InvalidXmByte)`. |
| `extract_calls_classifies_all_six_methylation_bytes` | Synthetic OT record produces all 6 calls. |
| `extract_calls_respects_ignore_5p` | `ignore_5p=3` drops first 3 read positions. |
| `extract_calls_respects_ignore_3p` | `ignore_3p=3` drops last 3 read positions. |
| `extract_calls_walks_cigar_with_indels` | `5M2D5M`, `5M2I5M` → correct ref_pos for each call. |
| `extract_calls_walks_cigar_with_soft_clips` | **Rev 1**: `2S8M` CIGAR → first emitted call has `read_pos == 2` (not 0); `iter_aligned`'s `read_pos_5p` includes soft-clip in count. |
| `extract_calls_empty_xm_yields_empty_vec` | XM of all `.` → empty Vec. |
| `extract_calls_minus_strand_orients_5prime` | **Critical**: OB strand record; first emitted call's `xm_byte` equals the LAST BAM-stored XM byte (orientation invariant). |
| `extract_calls_rejects_invalid_xm_byte_with_error` | XM contains `Q` → `Err(InvalidXmByte { byte: b'Q', ... })`. |
| `mbias_accumulate_increments_meth_for_Z` | `MbiasTable::accumulate(CpG, 5, true)` → `cpg[5].meth == 1`. |
| `mbias_accumulate_increments_unmeth_for_z` | (mirror) |
| `mbias_accumulate_routes_to_chg_for_X` | **Closes Alan's missing-CHG bug**. |
| `mbias_accumulate_routes_to_chg_for_x` | (mirror) |
| `mbias_accumulate_routes_to_chh_for_H` | **Closes Alan's missing-CHH bug**. |
| `mbias_accumulate_routes_to_chh_for_h` | (mirror) |
| `mbias_R2_index_ready` | `state.mbias[1]` exists, zero-initialised. |
| `route_call_default_mode_routes_to_strand_specific_file` | CpG-meth + OT → `CpG_OT_*.txt` content matches `format_meth_line` output. |
| **`route_single_record_with_mixed_contexts_routes_to_one_strand_directory`** | **Rev 1 (A Important #5)**: single record with CpG + CHG + CHH calls, all on OT strand → all calls land in `*_OT_*` files; no call in `*_CTOT_*` / `*_CTOB_*` / `*_OB_*` non-header content. **Closes Alan's split-across-files bug structurally at unit level.** |
| `format_meth_line_exact_bytes` | Tab-separated line format byte-exact. |
| **`output_file_map_eagerly_creates_all_strand_files_for_default_mode`** | **Rev 1 (B Critical C1)**: `OutputFileMap::new` produces 12 files on disk before any `write_call`; each contains exactly the header line (when `no_header == false`). |
| **`output_file_map_omits_header_when_no_header_true`** | **Rev 1 (A Important #6)**: `OutputFileMap::new` with `no_header=true` produces 12 empty files. |
| **`output_file_header_matches_perl_format`** | **Rev 1 (A Important #6)**: literal first-line bytes of a freshly-opened `CpG_OT_*.txt` equal `"Bismark methylation extractor version v0.25.1\n"`. |
| `output_file_map_creates_output_dir_if_missing` | **Rev 1 (B)**: `OutputFileMap::new` on a non-existent `output_dir` creates the directory. |
| `cleanup_partial_outputs_removes_all_12_files` | **Rev 1 (B C1 follow-on)**: after `InvalidXmByte` mid-record, all 12 files (including header-only CTOT/CTOB) are absent. |
| `cleanup_partial_outputs_continues_past_one_failure` | **Rev 1 (A Optional)**: one remove failure doesn't prevent the other 11. |
| **`route_call_increments_counter_before_mbias_only_short_circuit`** | **Rev 1 (B I4)**: under a synthetic `state.mbias_only=true` (force-set in test, even though Phase B's main dispatch rejects), splitting-report counter still increments. Locks the rev-1 ordering. |
| `splitting_report_emits_per_context_counts` | After a 6-call synthetic record, the report file contains correct counts. |
| **`splitting_report_percentage_handles_zero_denominator`** | **Rev 1 (A Optional)**: empty CHH context → `C methylated in CHH context: 0.00%` (not NaN, not divide-by-zero panic). |
| **`build_chr_name_table_rejects_non_ascii`** | **Rev 1 (B O1)**: synthetic @SQ with non-ASCII chr name → `Err(NonAsciiChromosomeName { name })`. |
| `derive_basename_strips_known_suffixes` | `"a.bam"` → `"a"`, `"a.sam"` → `"a"`, `"a.cram"` → `"a"`, `"a"` → `"a"`, `"a.BAM"` → `"a.BAM"` (case-sensitive), `"a.bam.gz"` → `"a.bam"` (only strips final `.gz`-less? actually no `.gz`-strip — verify: `a.bam.gz` → `a.bam.gz` per Perl `s/bam$/txt/` not matching `.gz` suffix). **Rev 1 (A Important #2 / B O2)**: case-sensitive single-suffix only; no `.bam.gz` handling. |
| `extract_se_rejects_record_with_paired_flag_set` | **Rev 1 (A Important #3)**: synthetic SE record with FLAG `0x1` set → `Err(PhaseNotYetImplemented { feature: "paired-end extraction ..." })` after cleanup. |
| `main_rejects_paired_end_with_phase_error` | `--paired` → `PhaseNotYetImplemented`. |
| **`main_rejects_multiple_input_files`** | **Rev 1 (A Important #4)**: two positional file args → `PhaseNotYetImplemented { feature: "multiple input files ..." }`. |
| `main_rejects_multicore_with_phase_error` | `--parallel 4` → `PhaseNotYetImplemented`. |
| `main_rejects_gzip_with_phase_error` | `--gzip` → `PhaseNotYetImplemented`. |
| `main_rejects_comprehensive_with_phase_error` | `--comprehensive` → `PhaseNotYetImplemented`. |
| `main_rejects_bedgraph_with_phase_error` | `--bedGraph` → `PhaseNotYetImplemented`. |
| **`extract_se_two_records_route_to_different_files`** | **Rev 1 (B §4)**: two records (one OT, one OB) → calls land in `*_OT_*` and `*_OB_*` respectively; multi-record accumulator correctness. |
| `extract_se_empty_input_writes_only_header_files` | **Rev 1 (B C1 follow-on)**: empty BAM → all 12 files exist with header only; splitting report written; exit 0. |

### 7.2 End-to-end smoke (`tests/se_phase_b_smoke.rs`) — **rev 1 (B I5) addition**

Synthetic ~10-record SE directional BAM committed at `tests/data/se_directional_phase_b_smoke.bam` (generation script at `tests/data/regenerate_smoke.sh`). The smoke test does NOT require the Perl toolchain.

- Run binary via `assert_cmd::Command::cargo_bin("bismark-methylation-extractor-rs")`.
- Pass the synthetic BAM, an output dir under `tempfile::tempdir()`, and `--single`.
- Assertions:
  - Exit code 0.
  - All 12 split files exist on disk.
  - Each file's first line equals the version header.
  - At least one of `CpG_OT_*.txt`, `CHG_OT_*.txt`, `CHH_OT_*.txt` has more than 1 line (records actually routed).
  - `_splitting_report.txt` exists, parses (line count > 5), contains `Total methylated C's in CpG context:` substring.

Catches: binary panic, wrong output dir, missing flush in finalize, wrong basename, missing-CHG/CHH (one of the 3 OT files would be header-only and would NOT have a content line — assertion catches it).

### 7.3 Perl-baseline integration test — deferred to Phase H

Per SPEC §8.3 the byte-identity gate runs on 10M + 55M PE WGBS data with full Perl baseline. Phase B does NOT build the Perl baseline locally. The smoke test in §7.2 + the unit tests in §7.1 + Phase C's PE arrival + Phase H's gate together cover the validation pyramid.

Phase B still commits `tests/data/regenerate_smoke.sh` and a `tests/data/README.md` documenting how to regenerate the smoke BAM from a synthetic FASTA, so Phase H can extend the script if needed.

## 8. Efficiency

- One Vec allocation per record from `iter_aligned` (~1.1 KiB at 95 aligned positions).
- One `Vec<MethCall>` allocation per record from `extract_calls` (`Vec::new()`; amortised growth).
- 12-entry HashMap with `BufWriter<File>` (8 KiB each, ~100 KiB total).
- `MbiasTable` grows lazily to ~5 KiB total per record-identity.
- `SplittingReport` ~64 bytes.

Profile target (informational): SE extract on 10M PE WGBS at parallel=1 reaches ≥ 1.5× Perl. Hard gate: byte-identity (Phase H).

For 55M PE reads = ~110M allocations / ~200 MiB temp heap traffic — allocator hot-path at parallel=1; under rayon (Phase F) this compounds. Flag for Phase F: a `iter_aligned_into(&mut Vec<...>)` variant in `bismark-io` could halve this. Not a Phase B concern.

## 9. Assumptions + open questions (rev 1)

### 9.1 Locked assumptions

- **Header line bytes**: literal `"Bismark methylation extractor version v0.25.1\n"`. Verified at Perl 5159, 5182, 5205, 5228, 5429, 5452, 5475, 5498, ... All branches emit the same literal.
- **Suffix stripping**: case-sensitive, single-extension `.bam`/`.sam`/`.cram` only. Verified at Perl 5410-5412 etc.
- **Eager file-open**: all 12 strand×context files opened at `OutputFileMap::new` time. Verified at Perl 5405-5700+.
- **`output_dir` may be missing**: `create_dir_all` defensively. Perl `make_path` precedent.
- **AutoDetect paired-mode treated as SE** in Phase B; per-record PAIRED-flag check rejects on first PE record.
- **U/u XM bytes**: silently skipped.
- **BismarkStrand-from-XR/XG matches Perl's per-record strand-classification** for all 4 strands × all 3 contexts. **Phase H risk** (A Important #8): dedup's v1.0 gate didn't exercise the strand → split-file-filename mapping. Phase B's smoke test + Phase H's full WGBS gate are the catchers.
- **Chromosome names are ASCII**: enforced via `build_chr_name_table` (rev 1 addition). Errors loudly otherwise.

### 9.2 Open questions (non-blocking)

1. **(Open)** Splitting-report exact byte-identity: Phase B aims for "Perl-shaped", Phase H gates exact bytes. Acceptable risk.
2. **(Resolved rev 1)** `derive_basename`: case-sensitive single-suffix `.bam`/`.sam`/`.cram`. No `.bam.gz`. Verified at Perl 5410-5412.
3. **(Resolved rev 1 via B confirmation)** `detect_paired_from_header`: lives at `bismark-dedup/src/pipeline.rs:137`. Phase C promotes it to `bismark-io` (additive bump to v1.0.0-beta.7) and replaces Phase B's per-record PAIRED-flag check.
4. **(Open, low priority)** `ExtractParams` struct revival: defer to Phase C/D when arg count grows; Phase B's signatures stay below the threshold.

### 9.3 Critical questions

**None.**

## 10. Validation (rev 1)

| What to verify | How | Expected |
|----------------|-----|----------|
| Eager-open + header bytes match Perl | `output_file_map_eagerly_creates_all_strand_files_for_default_mode` + `output_file_header_matches_perl_format` | All 12 files exist after `OutputFileMap::new`; first line of each is the literal Perl header. |
| `-` strand orientation invariant | `extract_calls_minus_strand_orients_5prime` | First emitted call's `xm_byte` is the BAM-stored last byte. |
| Missing CHG/CHH context routes (Alan's bug) | 4 `mbias_accumulate_routes_to_{chg,chh}_for_{X,x,H,h}` tests + `route_single_record_with_mixed_contexts_routes_to_one_strand_directory` | All increments land correctly; one record routes to one strand directory. |
| Partial-output cleanup | `cleanup_partial_outputs_removes_all_12_files` + `cleanup_partial_outputs_continues_past_one_failure` | All 12 files absent after error; one failure tolerated. |
| Phase-gate rejections (5 unsupported + multiple-files = 6) | 6 `main_rejects_*` tests | Each unsupported config returns `PhaseNotYetImplemented`. |
| Per-record PAIRED-flag rejection | `extract_se_rejects_record_with_paired_flag_set` | Cleanup runs before error. |
| Counter ordering vs `mbias_only` | `route_call_increments_counter_before_mbias_only_short_circuit` | Splitting-report counter increments even when `mbias_only=true`. |
| Header-line format byte equality | `output_file_header_matches_perl_format` | Literal byte equality. |
| Soft-clip read_pos counting | `extract_calls_walks_cigar_with_soft_clips` | First emitted call has `read_pos == soft_clip_len`. |
| Non-ASCII chr name rejection | `build_chr_name_table_rejects_non_ascii` | Loud error. |
| `output_dir` autocreate | `output_file_map_creates_output_dir_if_missing` | Dir created; files placed inside. |
| End-to-end smoke (rev 1 addition) | `tests/se_phase_b_smoke.rs` | Exit 0; 12 files; non-empty OT files; splitting report parses. |
| Empty input | `extract_se_empty_input_writes_only_header_files` | 12 header-only files; report with zero counts. |
| Clippy + rustfmt | `cargo clippy -- -D warnings && cargo fmt --check` | Clean. |

## 11. Integration with later phases

(Unchanged from rev 0 — see PR conversation if you want detail. Brief recap: C wires PE + `detect_paired_from_header` promotion to bismark-io; D adds M-bias writer; E adds mode dispatch + gzip + `mbias_only_silence` kernel param revival; F adds rayon multicore; G adds bedGraph/cytosine_report subprocess; H runs byte-identity gate on 10M + 55M PE WGBS.)

## 12. Self-review (rev 1)

**Efficiency.** Eager-open is +12 file-create syscalls per run (small fixed cost). Header writes are 12 × ~50 bytes = 600 bytes — irrelevant. Per-call HashMap lookup unchanged. Memory profile unchanged. No regression vs rev 0.

**Logic.** rev 1's `route_call` ordering (M-bias → counter → mbias_only short-circuit → write) is the correct one per Perl. `cleanup_partial_outputs` now removes 12 files instead of 0..n — slightly more I/O on the error path but the correct semantic.

**Edge cases.** Empty input now produces 12 header-only files (matches Perl). Soft-clip prose corrected. PAIRED-flag rejection unchanged.

**Integration.** No bismark-io bump still required (Phase C will need v1.0.0-beta.7 for `detect_paired_from_header`). No new Cargo deps.

**Risks remaining.**

- **R1**: Splitting-report exact format may drift from Phase B's emission. Phase H bridge.
- **R2**: `BismarkStrand`-vs-Perl per-strand classification (A Important #8). Phase H exercises for the first time.
- **R3**: SPEC §8.4 needs editorial fix (separate task). Phase B's plan now overrides the SPEC's wrong description.
- **R4 (Phase E follow-up)**: SPEC §7.5 pseudocode counter-ordering bug; queue a SPEC fix.

## 13. Follow-up tasks queued by rev 1

1. **SPEC fix #1**: edit `rust/bismark-extractor/SPEC.md` §8.4 row "Directional library" — replace "0-byte (Perl) or absent (Rust)" with "all 12 strand×context files exist; CTOT/CTOB contain only the version header line for directional libraries". Cite Perl lines 5405-5700+.
2. **SPEC fix #2**: edit SPEC §7.5 pseudocode to put splitting-report counter increment BEFORE the `mbias_only` short-circuit. Cite Perl behavior + plan §5.5.
3. **bismark-io v1.0.0-beta.7** (Phase C): promote `detect_paired_from_header` from `bismark-dedup` into `bismark-io::read`. Additive bump.

These are tracked here so they don't get lost; none block Phase B implementation.

## 14. Sub-issue (GitHub)

To be filed by user as a child of #798 at work-start. Suggested title + body:

```sh
gh issue create \
  --title "feat(extractor): Phase B — SE extraction loop + XM routing + eager output-file map + splitting-report skeleton" \
  --body "$(cat <<'EOF'
Phase B of the bismark-extractor port (umbrella #798).

Scope (locked in plans/05262026_bismark-extractor/PHASE_B_PLAN.md rev 1):

- SE extraction loop + XM-byte classification (SPEC §7.1, §7.2).
- **Eager** output-file map: all 12 strand×context files opened at run-start with the Perl version header (SPEC §7.5 + Perl source 5405-5700+).
- Splitting-report skeleton (SPEC §4.3, §7.7).
- M-bias counter accumulator (writer deferred to Phase D).

Out of scope (deferred to later phases): PE, non-default output modes, --gzip, --parallel > 1, --bedGraph, --cytosine_report, --multiple inputs — all rejected at the resolved-config boundary with PhaseNotYetImplemented.

Plan revision history (full detail in the plan file):
- rev 0: initial draft.
- rev 1: folded dual plan-review findings. Critical fix: switch from lazy to eager file creation to match Perl byte-identity. Plus 14 other corrections (test additions, prose fixes, error-type widening).

Estimated ~800 LOC per SPEC §10 row B.
EOF
)" \
  --label "rust-rewrite" \
  --label "bismark-extractor"
```

(Run from outside the broken-gh environment — see PROGRESS.md for the macOS keychain workaround.)
