# `bismark-extractor` Phase D — M-bias.txt writer + SE/PE section ordering

**Status:** rev 2 — implementation complete + dual code-review folded. 566 tests pass across 3 crates, clippy + fmt clean. Ready to commit + PR.
**Date:** 2026-05-26.
**Slug:** `plans/05262026_bismark-extractor/PHASE_D_PLAN.md`.
**Phase target:** SPEC §10 row D — ~500 LOC.
**GitHub sub-issue:** [#852](https://github.com/FelixKrueger/Bismark/issues/852) (filed at work-start).
**Depends on:** PR [#851](https://github.com/FelixKrueger/Bismark/pull/851) (Phase C) → PR [#849](https://github.com/FelixKrueger/Bismark/pull/849) (Phase B). Stacked branch `extractor-phase-d` based on `extractor-phase-c`.

## Revision history

- **rev 0** (2026-05-26): initial Phase D plan.
- **rev 2** (2026-05-26): post-implementation close-out folding dual code-review + plan-manager findings. Reviewer A returned APPROVE (cleanest verdict any phase has gotten), Reviewer B returned APPROVE-WITH-NITS (1 Medium + 4 Lows), plan-manager returned COMPLETE (53 DONE / 0 PARTIAL / 0 MISSING / 0 DEVIATED — first phase with zero gaps). No Critical or High findings; all algorithm + byte-identity surfaces verified against Perl by both reviewers. Tight fix-up applied:
  - **Reviewer A M1 — RESOLVED.** `lib.rs` status block updated from "Phase C / 1.0.0-alpha.3" to "Phase D / 1.0.0-alpha.4" + M-bias.txt writer mention + new pub re-export line for `mbias_writer::write_mbias_txt`.
  - **Reviewer B Low #1 — RESOLVED.** `derive_mbias_basename` docstring strengthened with explicit "trailing-dot stop semantic" paragraph + two new test cases (`foo.txt.bam` → `foo.txt.`; `foo.bam.txt` → `foo.bam.`). Locks the Perl-faithful single-strip-per-segment behaviour so a future maintainer doesn't "fix" the loop to peel until fixed-point.
  - **Reviewer B Low #4 — RESOLVED.** Tightened §8.4 Perl line-cite from `:5405-5700+` (300-line range) to `:5405-5430` (default-mode `open` + header `print` block) + `:5140-5325` (`--merge_non_CpG` mirror). Narrower references help future readers locate the exact byte-identity surface.
  - **Deferred to follow-up issues / Phase E:**
    - **Reviewer B M1** (PE smoke fixture R2-routing coverage): the 1-pair smoke fully overlaps R2 into drop_overlap, so binary-level mbias[1]-routing isn't exercised. Phase C's unit test `route_call_r2_goes_to_mbias_index_1` covers the routing at unit level. Strengthening with a 2-pair fixture where R2 survives overlap is a follow-up (low cost; Phase E or post-merge issue).
    - **Reviewer A L1-L3** (const CONTEXTS array, saturating_add stylistic, test-helper duplication): all cosmetic, no behaviour impact. Phase E may revisit.
    - **Reviewer B L2** (document `is_paired` alternative in PHASE_E_PLAN): Phase E concern; not in Phase D's scope.
    - **Reviewer B L3** (codify SPEC-fix convention in CLAUDE.md): organizational convention; track as separate documentation issue.
- **rev 1** (2026-05-26): folded dual plan-review reports.
  - **Reviewer B C1 (Critical) — RESOLVED.** `finalize` step order was inverted vs Perl. Perl emits splitting-report at `:2463` (inline in process_X_read_file) BEFORE M-bias.txt at `:314` (after the function returns). Rev 0's `flush → M-bias.txt → splitting_report` is wrong; rev 1's correct order is **`flush → splitting_report → M-bias.txt`**. Updated §4.5, §5.3, and §6 step 3. Important failure-semantic consequence: if `write_mbias_txt` errors on disk-full, the splitting-report is already on disk (matches Perl); rev 0 would have lost it.
  - **Reviewer B C2 (Critical) — RESOLVED.** SPEC §4.2 "4-col" fix lifted into Phase D's PR scope (was deferred to follow-up §16). One-line edit to `rust/bismark-extractor/SPEC.md` §4.2 to say "5-col" matching Perl `:729`. Added to §3.2 modified modules + §6 step list.
  - **Reviewer A Important #1-#2 — RESOLVED.** Filename tests extended to cover `sample.txt` (Perl `:637` strips `txt$`) and `sample.bam.gz` (Perl `:633` strips `gz$` first, leaving `sample.bam.`). Added to §7.1.
  - **Reviewer A Important #3 / Reviewer B Optional O5 — RESOLVED.** New side-by-side test `derive_basename_vs_derive_mbias_basename_lock_divergence` asserting `derive_basename("sample.bam") == "sample"` and `derive_mbias_basename("sample.bam") == "sample."` in the same test body. Plus doc-comment cross-references on both functions.
  - **Reviewer A Important #4 — RESOLVED.** New test `mbias_table_max_position_only_slot_0_returns_zero` constructing a `MbiasTable` with `cpg: vec![MbiasPos::default()]` (length 1, slot 0 only) and asserting `max_position() == 0`. Locks the edge invariant.
  - **Reviewer A Important nit (helper doc precision) — RESOLVED.** Reworded `derive_mbias_basename` docstring to "5 sequential strip attempts (`gz`, `sam`, `bam`, `cram`, `txt`), in that order; each attempt is run exactly once, replacing the running string if it matches."
  - **Reviewer B I1 (Important) — DOCUMENTED.** Added §9.2 row noting the `is_paired`-as-finalize-arg alternative (vs storing as `ExtractState` field). Reason for current choice: `finalize` is called from many sites and doesn't know the caller; field is the simpler abstraction for Phase D. Phase E's mode-dispatch revisit may collapse it.
  - **Reviewer B I2 (Important) — RESOLVED.** Smoke-test extension strategy changed: instead of modifying `tests/se_phase_b_smoke.rs` and `tests/pe_phase_c_smoke.rs` (which belong to in-review PRs #849/#851), add a NEW `tests/mbias_writer_phase_d_smoke.rs` that exercises both SE and PE binaries and asserts M-bias.txt content. Leaves the upstream PRs' files untouched — clean review surface.
  - **Reviewer B I3 (Important) — RESOLVED.** §6 step 7 now quantifies the ripple count: 5 sites in `tests/se_phase_b.rs` + 2 sites in `src/pipeline.rs` = 7 total. 0 sites in `tests/pe_phase_c.rs`, `tests/sanity.rs`, `tests/se_phase_b_smoke.rs`.
  - **Reviewer A Optional + Reviewer B O2 (debug_assert) — RESOLVED.** Added `debug_assert!(position_1based >= 1)` to `MbiasTable::accumulate` in §5.1. Locks the slot-0-unused invariant that `write_mbias_txt` relies on; zero cost in release builds; surfaces a regression loud in `cargo test` if a future kernel change passes 0-based position.
  - **Reviewer B O3 (rounding precision) — TEST ADDED.** New test `write_mbias_txt_percent_rounding_matches_perl_at_midpoint` exercises `(meth, un)` pairs that hit midpoint percent values (`0.125`-style edge). If Rust's `{:.2}` rounding diverges from Perl's libc rounding, Phase D catches it pre-Phase-H rather than at the byte-identity gate.
  - **Reviewer A Optional + Reviewer B O5 (doc cross-references) — RESOLVED.** Both `pipeline::derive_basename` and `mbias_writer::derive_mbias_basename` get doc-comments cross-referencing the other and explaining the divergence (single-suffix-with-dot vs Perl-style-without-dot).
  - **Deferred for follow-up:** Reviewer A Optional `Path::join` discussion in §9.1 (low priority — `ResolvedConfig::output_dir` is already a `PathBuf`; `Path::join` handles separator normalization correctly).

## Epic linkage

- **Design contract:** `rust/bismark-extractor/SPEC.md` (in-repo, rev 2). Phase D covers SPEC §4.2 (M-bias outputs) + §10 row D.
- **GitHub umbrella:** issue [#798](https://github.com/FelixKrueger/Bismark/issues/798).
- **Prior phases:**
  - Phase A merged (commit `144ca2d`, PR #847).
  - Phase B in review (PR #849, closes #848).
  - Phase C in review (PR #851, closes #850).
- **Phase D adds:** the writer that consumes the `[MbiasTable; 2]` accumulator already populated by Phases B + C.

## 1. Goal

Emit a Perl-byte-identical `M-bias.txt` file after the extraction loop finishes (when `!--mbias_off`). The accumulator already lives in `state.mbias[0]` (R1/SE) and `state.mbias[1]` (R2, populated by Phase C). Phase D adds the writer + the `state.finalize()` integration; no kernel changes.

After Phase D, the binary produces the canonical Bismark output set: 12 per-context split files (Phase B/C) + `_splitting_report.txt` (Phase B) + **`{basename}.M-bias.txt`** (this phase), gated by `--mbias_off`. PNG plots (`M-bias_R1.png` / `M-bias_R2.png`) remain deferred to v1.x — no clean Rust equivalent to Perl's `GD::Graph`.

## 2. Scope decisions (locked)

| Decision | Choice | Reasoning |
|----------|--------|-----------|
| Writer trigger | After the extraction loop, before `splitting_report`, inside `state.finalize`. Gated on `!config.mbias_off`. | Matches Perl `bismark_methylation_extractor:314` (`unless ($mbias_off) { produce_mbias_plots(...) }`). |
| SE vs PE section count | **3 sections (SE)** vs **6 sections (PE)** — pass `is_paired: bool` through `ExtractState::new` from the caller (`extract_se` sets false; `extract_pe` sets true). | Perl `:721-728` branches on `if ($paired)`. Empty R2 mbias accumulator alone isn't enough to decide (empty PE BAM would yield empty mbias[1] too). |
| Section header (SE) | Literal `"{context} context\n===========\n"` (11 `=` signs) | Perl `:726`. |
| Section header (PE R1) | Literal `"{context} context (R1)\n================\n"` (16 `=` signs) | Perl `:722`. |
| Section header (PE R2) | Literal `"{context} context (R2)\n================\n"` (16 `=` signs) | Perl `:825`. |
| Column header | Literal `"position\tcount methylated\tcount unmethylated\t% methylation\tcoverage\n"` (5 columns) | Perl `:729`. **Note:** SPEC §4.2 said "4-col" but Perl actually emits 5; SPEC §4.2 needs corrective edit (queued as follow-up task). |
| Per-position row | `"{pos}\t{meth}\t{un}\t{percent}\t{coverage}\n"` | Perl `:746`. |
| Percent format | `%.2f` if `meth + un > 0`; **empty string** otherwise (yields `\t\t` between un and coverage) | Perl `:740-743`. |
| Coverage column | `meth + un`, always emitted (including 0) | Perl `:744`. |
| Position range | `1..max_length` where `max_length` = highest 1-based position across all 3 contexts in that read-identity slot. Each context iterates the full range (zero-padded rows for positions with no calls in that context). | Perl `:647-661, :731, :827`. |
| `max_length` calculation | One scan over `mbias[0].{cpg,chg,chh}` for `max_length_1`; same for `mbias[1].*` for `max_length_2` (PE only). | Perl `:647-661`. |
| Section iteration order | `[CpG, CHG, CHH]` always; for PE, R1's 3 sections fully written before R2's 3 sections begin. | Perl `:718-820`. |
| Trailing blank line | `\n` after each section (yields a blank line between sections). | Perl `:762`. |
| **Filename** | `{output_dir}{basename_for_mbias}M-bias.txt` where `basename_for_mbias` derives from input filename with `s/gz$//; s/sam$//; s/bam$//; s/cram$//; s/txt$//;` applied — **keeps the trailing `.`!** E.g. `sample.bam` → `sample.M-bias.txt`. | Perl `:632-642`. **Different from Phase B's `derive_basename`**, which strips single suffix without dot-preservation. Need a separate `derive_mbias_basename` helper. |
| PNG plot files | **Deferred to v1.x** (no Rust equivalent to `GD::Graph`). | SPEC §4.2 + plan locked at recon time. |
| `--mbias_only` flag | Phase D does NOT enable `--mbias_only` (still rejected with `PhaseNotYetImplemented` at main dispatch). Phase E enables it by adding the route_call short-circuit. M-bias.txt writer itself is unchanged — Phase D's writer would be reached identically under `--mbias_only` in Phase E. | Splits the toggle work; Phase D's writer doesn't care about the route_call flow. |
| `--mbias_off` enables Phase D's path | Currently `mbias_off` IS in `ResolvedConfig` (Phase A); Phases B/C respect it in `route_call` (skip accumulation). Phase D adds the writer-skip when `mbias_off`. | Single-flag toggle; consistent with Perl gate at `:314`. |

## 3. Context

### 3.1 Source documents read end-to-end

- `rust/bismark-extractor/SPEC.md` §§4.2 (M-bias outputs), 6.2 (`[MbiasTable; 2]` structural decision), 10 (Phase D row), 14 (revision history).
- Phase B + C source: `state.rs` (ExtractState), `mbias.rs` (MbiasTable + MbiasPos + accumulate), `pipeline.rs` (extract_se + extract_pe).
- Perl `bismark_methylation_extractor` lines 314 (mbias_off guard), 628-836 (the `produce_mbias_plots` sub, despite the name — also emits M-bias.txt). Critically: lines 632-642 (filename construction), 647-661 (max_length scan), 718-763 (R1 sections), 820-836+ (R2 sections).

### 3.2 Code placement

All Phase D code lands inside `rust/bismark-extractor/`:

- **New modules**:
  - `rust/bismark-extractor/src/mbias_writer.rs` — `write_mbias_txt` function + private helpers. Kept separate from `mbias.rs` (accumulator-only) so the writer + accumulator concerns stay independent.
- **Modified modules**:
  - `rust/bismark-extractor/src/mbias.rs` — extend `MbiasTable` with a `max_position()` method. Also add `debug_assert!(position_1based >= 1)` to `accumulate` (rev 1 Reviewer A Optional / Reviewer B O2) to lock the slot-0-unused invariant.
  - `rust/bismark-extractor/src/state.rs` — add `is_paired: bool` field to `ExtractState`; set by callers via `ExtractState::new(config, input, basename, is_paired)`. **Rev 1 (Reviewer B C1) corrected `finalize` order**: `flush_all → write_splitting_report → write_mbias_txt` (was reversed in rev 0; Perl emits splitting-report at `:2463` BEFORE M-bias.txt at `:314`). Update `state.rs` doc-comment to mention the new writer step.
  - `rust/bismark-extractor/src/pipeline.rs` — `extract_se` constructs `ExtractState::new(..., is_paired=false)`; `extract_pe` constructs with `is_paired=true`. Two-line change.
  - `rust/bismark-extractor/src/lib.rs` — `pub mod mbias_writer;` + re-export `write_mbias_txt`.
  - `rust/bismark-extractor/src/pipeline.rs::derive_basename` — add doc-comment cross-reference to `mbias_writer::derive_mbias_basename` (rev 1 Reviewer A Optional / Reviewer B O5).
  - `rust/bismark-extractor/Cargo.toml` — version bump `1.0.0-alpha.3` → `1.0.0-alpha.4`. No new dependencies.
  - `rust/bismark-extractor/SPEC.md` — **rev 1 (Reviewer B C2 + Felix retroactive decision):** roll in all three known SPEC prose errors surfaced across Phases B/C/D, in one consolidated edit:
    - **§4.2** "4-col table" → "5-col table" (Phase D surfaced). Perl `:729` emits `position\tcount methylated\tcount unmethylated\t% methylation\tcoverage`.
    - **§7.4 "Edge case: disjoint pair → no-op"** corrected: actually drops all R2 calls downstream of `r1_ref_end` (Phase C surfaced; verified against Perl `:2905-2906`'s early-exit `return`).
    - **§8.4 "Directional library: 0-byte (Perl) or absent (Rust)"** corrected: Perl actually emits header-only files for CTOT/CTOB on directional input (Phase B surfaced; verified against Perl `:5405-5700+`). Rust now emits the same eager-open header-only files.
- **Modified modules (no logic change, signature only)**:
  - The `ExtractState::new` signature change ripples through tests that construct `ExtractState` directly: **5 sites in `tests/se_phase_b.rs`** (`route_call_*` tests via `test_config(...)` + `ExtractState::new`) + **2 sites in `src/pipeline.rs`** (`extract_se` + `extract_pe`) = **7 total**. Zero sites in `tests/pe_phase_c.rs`, `tests/sanity.rs`, `tests/se_phase_b_smoke.rs`, `tests/pe_phase_c_smoke.rs` (all go through the binary).
- **Tests**:
  - `rust/bismark-extractor/tests/mbias_writer_phase_d.rs` — new file housing the Phase D unit tests enumerated in §7.1.
  - `rust/bismark-extractor/tests/mbias_writer_phase_d_smoke.rs` — **rev 1 (Reviewer B I2)**: NEW end-to-end smoke file (was rev 0's plan to extend `se_phase_b_smoke.rs` + `pe_phase_c_smoke.rs`, which would touch in-review PRs #849/#851's surface and cause review-hygiene noise). Exercises both SE and PE binaries; asserts M-bias.txt content on each.

### 3.3 Crate version

- `bismark-extractor`: `1.0.0-alpha.3` → `1.0.0-alpha.4` (additive within alpha line).
- `bismark-io`: unchanged (`1.0.0-beta.7`).
- `bismark-dedup`: unchanged.

### 3.4 Binary behaviour

After Phase D:
- Default run (SE or PE): emits the 12 split files + `_splitting_report.txt` + **`{basename}.M-bias.txt`**.
- `--mbias_off`: emits the 12 split files + `_splitting_report.txt` only; no `M-bias.txt`.
- `--mbias_only`: still rejected at main dispatch (Phase E enables).
- All other phase-gates unchanged from Phase C.

## 4. Behaviour specification

### 4.1 Filename construction

```rust
/// Derives the M-bias.txt basename per Perl `:632-642`:
/// strip path → strip trailing `gz`/`sam`/`bam`/`cram`/`txt` (one at a time,
/// in that order) → returns the result with whatever trailing `.` survives.
///
/// **Differs from `pipeline::derive_basename`**: that one strips a single
/// `.bam`/`.sam`/`.cram` extension (including the dot); this one strips
/// without the dot, mirroring Perl's `s/bam$//` behaviour.
///
/// Examples:
/// - `sample.bam`     → `sample.`     → file `sample.M-bias.txt`
/// - `sample.bam.gz`  → `sample.bam.` → file `sample.bam.M-bias.txt`
/// - `sample.sam`     → `sample.`     → file `sample.M-bias.txt`
/// - `sample` (no ext) → `sample`     → file `sampleM-bias.txt`
fn derive_mbias_basename(path: &Path) -> String;

/// Full M-bias.txt path: `{output_dir}{basename}M-bias.txt`.
fn mbias_txt_path(output_dir: &Path, input: &Path) -> PathBuf;
```

### 4.2 Section format

For each `(context, read_identity)` section the writer emits, in order:

1. **Section header** (one line + one equals-line):
   - SE: `"{context} context\n===========\n"` (11 `=`)
   - PE R1: `"{context} context (R1)\n================\n"` (16 `=`)
   - PE R2: `"{context} context (R2)\n================\n"` (16 `=`)
   
2. **Column header** (one line):
   ```
   position<TAB>count methylated<TAB>count unmethylated<TAB>% methylation<TAB>coverage<LF>
   ```

3. **Per-position rows**, position `pos = 1..=max_length`:
   ```
   {pos}<TAB>{meth}<TAB>{un}<TAB>{percent}<TAB>{coverage}<LF>
   ```
   - `meth`, `un`: counts from `mbias[idx].{cpg|chg|chh}[pos]`; 0 if vec is shorter than pos or cell is default.
   - `percent`: `format!("{:.2}", 100.0 * meth as f64 / (meth + un) as f64)` if `meth + un > 0`; otherwise the empty string `""` (the row literally has two consecutive tabs between `un` and `coverage`).
   - `coverage`: `meth + un`, always emitted (including 0).

4. **Trailing blank line**: a single `\n`.

### 4.3 `max_length` calculation

For each read-identity slot (R1/SE = index 0; R2 = index 1), `max_length` is the maximum 1-based position observed across all three context vectors. Implementation:

```rust
impl MbiasTable {
    pub fn max_position(&self) -> u32 {
        let m1 = self.cpg.len().saturating_sub(1) as u32;
        let m2 = self.chg.len().saturating_sub(1) as u32;
        let m3 = self.chh.len().saturating_sub(1) as u32;
        m1.max(m2).max(m3)
    }
}
```

Returns 0 if all three vecs are empty. The writer then iterates `1..=max_position` per context. If `max_position == 0`, the writer **still emits the section header + column header**, but no per-position rows (matches Perl's behaviour at `:731` `foreach my $pos (1..0)` — an empty loop). Then the blank-line separator.

**Verification**: Perl with an empty M-bias accumulator (e.g. all-`.` XM tag in every record) would write empty per-position rows. The Rust port matches.

### 4.4 SE vs PE branching

```rust
pub fn write_mbias_txt(
    path: &Path,
    mbias: &[MbiasTable; 2],
    is_paired: bool,
) -> Result<(), std::io::Error> {
    let mut w = BufWriter::with_capacity(8 * 1024, File::create(path)?);
    let max_1 = mbias[0].max_position();
    write_mbias_sections(&mut w, &mbias[0], max_1, ReadIdentitySection::R1_or_SE { is_paired })?;
    if is_paired {
        let max_2 = mbias[1].max_position();
        write_mbias_sections(&mut w, &mbias[1], max_2, ReadIdentitySection::R2)?;
    }
    w.flush()
}
```

The `ReadIdentitySection` enum picks the header text + equals-line length:
- `R1_or_SE { is_paired: false }` → `"CpG context\n===========\n"` etc.
- `R1_or_SE { is_paired: true }`  → `"CpG context (R1)\n================\n"` etc.
- `R2`                            → `"CpG context (R2)\n================\n"` etc.

### 4.5 `--mbias_off` gate

`state.finalize(&config)` is the entry point. Phase B already takes `&ResolvedConfig`. Phase D adds:

```rust
pub fn finalize(&mut self, config: &ResolvedConfig) -> Result<(), BismarkExtractorError> {
    self.fhs.flush_all()?;
    // Rev 1 fix (Reviewer B C1): Perl emits splitting-report at :2463
    // (inline in process_X_read_file) BEFORE M-bias.txt at :314 (called
    // after the function returns). Order must be:
    //   flush_all → write_splitting_report → write_mbias_txt
    // Rev 0 had M-bias.txt before splitting-report; that would cause two
    // problems vs Perl:
    //   1. mtime / on-disk ordering differs.
    //   2. If write_mbias_txt fails (disk-full), splitting-report would
    //      be lost — but Perl writes it first, so a real Perl run would
    //      preserve the splitting-report in that failure mode.
    if self.emit_splitting_report {
        write_splitting_report(...)?;
    }
    if !config.mbias_off {
        let mbias_path = mbias_txt_path(&config.output_dir, &self.input_path);
        write_mbias_txt(&mbias_path, &self.mbias, self.is_paired)?;
    }
    Ok(())
}
```

Order: `flush → splitting_report → M-bias.txt`. Matches Perl `bismark_methylation_extractor:2463` (splitting-report inline in process_X_read_file) followed by `:314-317` (`unless ($mbias_off) { produce_mbias_plots(...) }`).

### 4.6 `is_paired` field threading

```rust
pub struct ExtractState {
    pub mode: OutputMode,
    pub mbias_off: bool,
    pub mbias_only: bool,
    pub is_paired: bool,         // NEW in Phase D
    pub mbias: [MbiasTable; 2],
    pub fhs: OutputFileMap,
    pub report: SplittingReport,
    // (private fields unchanged)
}

impl ExtractState {
    pub fn new(
        config: &ResolvedConfig,
        input_path: &Path,
        input_basename: &str,
        is_paired: bool,         // NEW parameter
    ) -> Result<Self, BismarkExtractorError>;
}
```

Callers:
- `extract_se`: `ExtractState::new(config, input, &basename, /*is_paired=*/ false)`
- `extract_pe`: `ExtractState::new(config, input, &basename, /*is_paired=*/ true)`

### 4.7 Edge cases

| Case | Handling |
|------|----------|
| Empty input BAM | All three mbias vecs empty → `max_position == 0` → section header + column header emitted, no per-position rows. 3 sections (SE) or 6 sections (PE). Matches Perl's empty-loop behaviour. |
| `--mbias_off` set | `M-bias.txt` not written. `state.finalize` skips the call. |
| Single context populated (e.g. only CpG hits) | `max_position` computed from CpG's vec. CHG and CHH sections still emitted with `0\t0\t\t0` rows for every position. Matches Perl. |
| Max position 1000+ (long-read or PacBio-like) | Loop iterates `1..=max_position`; no upper cap. ~120 KB output per section at 5000 positions — well within practical limits. |
| Position 0 in the mbias vec | The accumulator allocates slot 0 lazily (per `MbiasTable::accumulate`) but `route_call` always passes `pos_1based = call.read_pos + 1` (>= 1). Position 0 should never have non-zero counts. **Defensive note**: the writer's `1..=max_length` loop ignores position 0 by design. |
| Filename collision with split files | Phase B's split files use pattern `{Context}_{Strand}_{basename}.txt`; M-bias.txt uses `{basename}.M-bias.txt` (or `{basename}M-bias.txt` for unsuffixed inputs). The `M-bias.txt` literal is distinct enough that no collision is possible. |
| Output directory missing | `state.finalize` → `File::create` returns `NotFound` → propagated as `IoWrite`. `OutputFileMap::new` (Phase B) already `create_dir_all`'s the output dir at extraction start, so this would only trigger if the dir was deleted mid-run (extremely unlikely; not worth special handling). |

## 5. Signatures

### 5.1 `mbias.rs` additions

```rust
impl MbiasTable {
    /// Highest 1-based position observed across all three context vectors.
    /// Returns 0 if all vecs are empty (interpretable by the writer as
    /// "emit headers only, no per-position rows"). Also returns 0 if only
    /// slot 0 is allocated — see test `mbias_table_max_position_only_slot_0_returns_zero`.
    pub fn max_position(&self) -> u32;
}

// In `MbiasTable::accumulate` (existing Phase B function), rev 1 adds
// a defensive debug-assert:
impl MbiasTable {
    pub fn accumulate(&mut self, context: CytosineContext, position_1based: u32, methylated: bool) {
        // Rev 1 (Reviewer A Optional / Reviewer B O2): lock the slot-0-unused
        // invariant that `write_mbias_txt` relies on. Phase B/C's route_call
        // always passes `pos_1based = call.read_pos + 1` (>= 1). If a future
        // kernel change ever passes 0, the writer would silently drop the
        // slot-0 data — this assert surfaces the regression at unit-test time.
        debug_assert!(position_1based >= 1, "M-bias position must be 1-based; got 0");
        // ... rest of existing implementation ...
    }
}
```

### 5.2 `mbias_writer.rs`

```rust
//! M-bias.txt writer (Phase D).
//!
//! Consumes the [`MbiasTable; 2`] accumulator populated by Phases B + C.
//! Byte-identity-targeted at Perl `bismark_methylation_extractor` lines
//! 628-836 (`produce_mbias_plots` — name is historical; the sub writes
//! both M-bias.txt and the optional PNG plots).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::call::CytosineContext;
use crate::mbias::{MbiasPos, MbiasTable};

/// Derive the M-bias.txt path from an input file path.
///
/// Mirrors Perl `:632-642`: strip path → strip trailing `gz`/`sam`/`bam`/
/// `cram`/`txt` (one suffix at a time, in that order) → append `M-bias.txt`.
/// The trailing `.` (if any) is preserved — different from `derive_basename`
/// in `pipeline.rs`.
pub fn mbias_txt_path(output_dir: &Path, input: &Path) -> PathBuf;

/// Internal helper: M-bias basename without `M-bias.txt` suffix.
fn derive_mbias_basename(input: &Path) -> String;

/// Identity slot for section header text selection.
#[derive(Debug, Clone, Copy)]
enum ReadIdentitySection {
    /// R1-or-SE slot. When `is_paired`, headers read `"{ctx} context (R1)\n================\n"`.
    /// When not paired, headers read `"{ctx} context\n===========\n"`.
    R1OrSe { is_paired: bool },
    /// R2 slot (PE only). Headers read `"{ctx} context (R2)\n================\n"`.
    R2,
}

/// Write the full M-bias.txt file at `path`.
///
/// Emits 3 sections for SE (`is_paired = false`) or 6 sections for PE
/// (`is_paired = true`). Section iteration: `[CpG, CHG, CHH]` for R1/SE,
/// then (PE only) `[CpG, CHG, CHH]` for R2.
///
/// Returns `Ok(())` on success; propagates `std::io::Error` on disk
/// failure. Caller decides whether to error out or continue (Phase D's
/// `state.finalize` propagates).
pub fn write_mbias_txt(
    path: &Path,
    mbias: &[MbiasTable; 2],
    is_paired: bool,
) -> Result<(), std::io::Error>;
```

### 5.3 `state.rs` modifications

```rust
pub struct ExtractState {
    pub mode: OutputMode,
    pub mbias_off: bool,
    pub mbias_only: bool,
    /// **NEW in Phase D**: true iff this run is paired-end (set by extract_pe).
    /// Decides whether M-bias.txt has 3 or 6 sections.
    pub is_paired: bool,
    pub mbias: [MbiasTable; 2],
    pub fhs: OutputFileMap,
    pub report: SplittingReport,
    // (private fields unchanged)
}

impl ExtractState {
    pub fn new(
        config: &ResolvedConfig,
        input_path: &Path,
        input_basename: &str,
        is_paired: bool,           // NEW parameter
    ) -> Result<Self, BismarkExtractorError>;

    /// Phase D: finalize now also writes M-bias.txt when !config.mbias_off.
    pub fn finalize(&mut self, config: &ResolvedConfig) -> Result<(), BismarkExtractorError>;

    pub fn cleanup_partial_outputs(&mut self);
}
```

### 5.4 `pipeline.rs` callsite changes

```rust
// extract_se body — 1-line addition:
let mut state = ExtractState::new(config, input, &input_basename, /*is_paired=*/ false)?;

// extract_pe body — 1-line addition:
let mut state = ExtractState::new(config, input, &input_basename, /*is_paired=*/ true)?;
```

## 6. Implementation outline (rev 1)

1. **SPEC fixes** (rev 1 Reviewer B C2 + Felix retroactive decision): edit `rust/bismark-extractor/SPEC.md` to fix three known prose errors in one consolidated edit:
   - §4.2 "4-col" → "5-col" (Phase D surfaced; Perl `:729`).
   - §7.4 "disjoint pair → no-op" → describe actual strict-`<` polarity drops downstream R2 calls (Phase C surfaced; Perl `:2905-2906`).
   - §8.4 "0-byte (Perl) or absent (Rust)" → "all 12 strand×context files exist eagerly; CTOT/CTOB header-only for directional input" (Phase B surfaced; Perl `:5405-5700+`).
   Each is a single-line/paragraph edit with a Perl line citation. Total ~10 SPEC LOC.
2. **Add `MbiasTable::max_position`** to `mbias.rs`. ~5 LOC. Also add `debug_assert!(position_1based >= 1)` to `accumulate` (rev 1).
3. **Modify `state.rs`** (rev 1 Reviewer B C1 fix):
   - Add `is_paired: bool` field to `ExtractState`.
   - Add `is_paired: bool` parameter to `ExtractState::new`.
   - **Reorder `finalize`**: `flush_all → write_splitting_report → write_mbias_txt` (was reversed in rev 0). Matches Perl `:2463` (splitting-report inline) followed by `:314` (M-bias.txt after function returns).
   - `write_mbias_txt` call is gated on `!config.mbias_off`.
4. **Create `mbias_writer.rs`**:
   - `derive_mbias_basename(path: &Path) -> String` — mirrors Perl `:632-637` regex chain. Doc-comment cross-references `pipeline::derive_basename` and explains the divergence (single-suffix-with-dot vs Perl-style-without-dot).
   - `mbias_txt_path(output_dir: &Path, input: &Path) -> PathBuf` — appends `M-bias.txt`.
   - `ReadIdentitySection` enum.
   - `write_mbias_sections` helper — iterates 3 contexts, emits each section.
   - `write_one_section` private — writes one (context, identity) section.
   - `write_mbias_txt` public entry point.
   - Approximately 220 LOC including docs.
5. **Modify `pipeline.rs::derive_basename`** doc-comment: add cross-reference to `mbias_writer::derive_mbias_basename` so a future maintainer searching for "basename" sees both helpers.
6. **Modify `pipeline.rs`** callsites: `extract_se` passes `is_paired=false`; `extract_pe` passes `true`. 2-line touch.
7. **Modify `lib.rs`**: `pub mod mbias_writer;` + re-export `write_mbias_txt`.
8. **Modify `Cargo.toml`**: version bump `1.0.0-alpha.3` → `1.0.0-alpha.4`.
9. **Update existing tests** for the new `ExtractState::new` signature: **5 sites in `tests/se_phase_b.rs`** (all the `route_call_*` tests that build state via `test_config(...)`). 2 production callers (`extract_se`, `extract_pe`) updated in step 6. **0 sites elsewhere** (`tests/pe_phase_c.rs`, `tests/sanity.rs`, smoke files all go through the binary).
10. **Write Phase D unit tests** in `tests/mbias_writer_phase_d.rs` (§7.1 below).
11. **Write Phase D smoke** in `tests/mbias_writer_phase_d_smoke.rs` (§7.2 — NEW file per rev 1 Reviewer B I2; does NOT modify `se_phase_b_smoke.rs` or `pe_phase_c_smoke.rs` to avoid touching files owned by in-review PRs #849/#851).
12. **Run `cargo test -p bismark-extractor && cargo clippy && cargo fmt --check`**.

## 7. Tests

### 7.1 Unit tests (in `tests/mbias_writer_phase_d.rs`)

| Test | Asserts |
|------|---------|
| `derive_mbias_basename_strips_known_suffixes` | `sample.bam` → `sample.`; `sample.sam.gz` → `sample.sam.`; `sample.cram` → `sample.`; `sample` → `sample`; `sample.txt` → `sample.` (**rev 1 add per Reviewer A I1**); `sample.bam.gz` → `sample.bam.` (**rev 1 add per Reviewer A I2** — `gz$` strips first, leaving `sample.bam.`; subsequent `bam$` doesn't match because the new tail is `.`). Preserves trailing dot per Perl. |
| **`derive_basename_vs_derive_mbias_basename_lock_divergence`** | **Rev 1 (Reviewer A I3 / Reviewer B O5):** for input `sample.bam`, assert `pipeline::derive_basename(input) == "sample"` AND `mbias_writer::derive_mbias_basename(input) == "sample."` in the same test body. Locks the divergence between the two helpers so a future maintainer can't accidentally swap them. Repeats the assertion for `sample.sam`, `sample.cram`, `sample.bam.gz`. |
| `mbias_txt_path_appends_to_basename_in_output_dir` | `(output_dir=/tmp, input=/abs/sample.bam)` → `/tmp/sample.M-bias.txt`. |
| `mbias_table_max_position_empty` | Empty table → `max_position == 0`. |
| `mbias_table_max_position_single_context` | Only CpG populated up to position 100 → `max_position == 100`. |
| `mbias_table_max_position_max_across_contexts` | CpG to 50, CHG to 100, CHH to 75 → `max_position == 100`. |
| **`mbias_table_max_position_only_slot_0_returns_zero`** | **Rev 1 (Reviewer A I4):** construct a `MbiasTable` directly with `cpg: vec![MbiasPos::default()]` (length 1, slot 0 only) → `max_position() == 0`. Locks the "slot-0-only means no real data" edge invariant. Should never arise in production because Phase B/C's `route_call` always passes `pos_1based >= 1` (now also debug_assert'd in rev 1), but pins the writer's `1..=0` empty-loop behaviour against regressions. |
| **`mbias_accumulate_position_zero_debug_panics`** | **Rev 1 (Reviewer A Optional / Reviewer B O2):** in a debug build, `MbiasTable::accumulate(CpG, 0, true)` panics via `debug_assert!`. Phase B/C never trip this; future-bug catcher. Wrapped in `#[cfg(debug_assertions)]` so release builds skip the test. |
| `write_mbias_txt_se_emits_3_sections` | Synthetic R1 mbias with calls in all 3 contexts; SE mode → output has exactly 3 section headers (`CpG context`, `CHG context`, `CHH context` with 11-equals separator). No R2 sections. |
| `write_mbias_txt_pe_emits_6_sections` | Synthetic R1 + R2 mbias; PE mode → output has 6 section headers, R1 then R2, with `(R1)` / `(R2)` suffixes and 16-equals separators. |
| `write_mbias_txt_se_section_header_format_bytes` | Byte-exact check on `CpG context\n===========\n` for SE (11 equals). |
| `write_mbias_txt_pe_section_header_format_bytes` | Byte-exact check on `CpG context (R1)\n================\n` for PE R1 (16 equals); same for R2. |
| `write_mbias_txt_column_header_bytes_exact` | Column header equals `"position\tcount methylated\tcount unmethylated\t% methylation\tcoverage\n"`. |
| `write_mbias_txt_per_position_row_with_calls` | Section with `cpg[5] = MbiasPos { meth: 30, unmeth: 70 }` → row at pos=5 is `"5\t30\t70\t30.00\t100\n"`. |
| `write_mbias_txt_per_position_row_zero_coverage_empty_percent` | Section with `cpg[5] = MbiasPos::default()` and max_position=10 → row at pos=5 is `"5\t0\t0\t\t0\n"` (note `\t\t` between unmeth and coverage — percent is empty string for 0-coverage). |
| `write_mbias_txt_iterates_all_positions_up_to_max` | Max position 10; CpG populated only at positions 3 and 7 → 10 rows emitted (positions 1-10), with non-zero only at 3 and 7. |
| `write_mbias_txt_blank_line_between_sections` | Verify the byte after each section's last row is `\n\n` (one ending newline + one blank line). |
| `write_mbias_txt_empty_mbias_emits_headers_only` | All three context vecs empty; SE mode → output has 3 section headers + 3 column headers, no per-position rows, blank lines between. |
| `write_mbias_txt_pe_empty_r2_section_still_emitted` | R1 has calls at positions 1-10; R2 entirely empty → output has 6 sections; R2 sections have headers + column header only, no per-position rows. |
| `write_mbias_txt_percent_precision_2dp` | `meth=1, unmeth=2` → percent = `33.33` (3 chars + 2 decimals); `meth=1, unmeth=3` → `25.00`; `meth=2, unmeth=1` → `66.67`. |
| **`write_mbias_txt_percent_rounding_matches_perl_at_midpoint`** | **Rev 1 (Reviewer B O3):** exercises `(meth, un)` pairs designed to hit `.5` percent midpoints — e.g. `meth=1, un=7` (12.5%), `meth=125, un=875` (12.5%), `meth=3, un=5` (37.5%). Asserts Rust's `format!("{:.2}", ...)` produces a result. If Rust's banker's-rounding diverges from Perl's libc rounding at any midpoint, Phase D catches it pre-Phase-H rather than at byte-identity gate time. The test fixture is a regression-snapshot: if the assertion ever fails, switch to a manual half-away-from-zero rounding helper before Phase H. |
| `mbias_table_accumulate_grows_vec_lazily_to_position` | Phase B regression: `accumulate(CpG, 100, true)` grows `cpg` to length 101 (indices 0..100); positions 1..99 remain `MbiasPos::default()`. |
| `extract_state_new_se_sets_is_paired_false` | `ExtractState::new(config, ..., is_paired=false)` → state.is_paired == false. |
| `extract_state_new_pe_sets_is_paired_true` | Mirror. |
| `extract_state_finalize_writes_mbias_txt_when_not_mbias_off` | Construct state via `ExtractState::new`, accumulate some calls, call `finalize`, assert M-bias.txt exists on disk. |
| `extract_state_finalize_skips_mbias_txt_when_mbias_off` | Same but with `config.mbias_off = true` → M-bias.txt does NOT exist on disk. |

### 7.2 End-to-end smoke (`tests/mbias_writer_phase_d_smoke.rs`)

**Rev 1 (Reviewer B I2):** all M-bias.txt smoke assertions live in a NEW file rather than extending `tests/se_phase_b_smoke.rs` or `tests/pe_phase_c_smoke.rs`. Those files are owned by in-review PRs #849/#851; modifying them in Phase D's PR would create review-hygiene churn (PR-base reviewers see Phase D edits that aren't theirs; any change requested upstream conflicts with Phase D).

New file exercises both SE and PE binaries:

- `smoke_mbias_se_directional_produces_se_format_mbias_txt`:
  - Build the same synthetic SE BAM as `se_phase_b_smoke.rs`'s `write_se_directional_bam`.
  - Spawn binary with `--single-end`.
  - Assert `{basename}.M-bias.txt` exists.
  - Assert content contains `CpG context\n===========\n` (3 SE sections with 11-equals).
  - Assert content does NOT contain `(R1)` or `(R2)` (those are PE-only).
- `smoke_mbias_pe_auto_detect_produces_pe_format_mbias_txt`:
  - Build the same PE BAM as `pe_phase_c_smoke.rs`'s `write_pe_directional_bam` (10 OT pairs, includes a Bismark @PG line for auto-detect).
  - Spawn binary (no `--paired-end` — let auto-detect dispatch).
  - Assert `{basename}.M-bias.txt` exists.
  - Assert content contains both `CpG context (R1)\n================\n` and `CpG context (R2)\n================\n` (6 PE sections with 16-equals).
- `smoke_mbias_txt_absent_with_mbias_off`:
  - Build any synthetic BAM (SE is fine).
  - Spawn binary with `--mbias_off`.
  - Assert `{basename}.M-bias.txt` does NOT exist on disk.
  - Splitting-report + split files still exist (Phase B behaviour unchanged).

### 7.3 Phase B/C regression

`cargo test -p bismark-extractor` should pass all existing tests after the `ExtractState::new` signature change. The change is additive (one new parameter); existing callers must pass `false` (SE) or `true` (PE).

## 8. Efficiency

The writer runs **once per run** at finalize time. Cost:

- One `MbiasTable::max_position` scan per slot: O(1) (length lookup on 3 vecs).
- Per-section: O(max_length) writes; ~100 positions per typical Illumina read.
- 3 contexts × 100 positions × ~30 bytes/row ≈ 9 KB per R1 section. PE total ≈ 60 KB. Negligible.
- Single 8-KiB `BufWriter<File>` ammortizes the writes; one syscall on flush.

No per-record cost. Phase F (multicore) doesn't change anything here — the M-bias.txt writer runs after the parallel extraction loop completes.

## 9. Assumptions + open questions

### 9.1 Locked assumptions

- **Filename pattern** (`{basename}.M-bias.txt` or `{basename}M-bias.txt`): mirrors Perl exactly. Verified against Perl `:632-642`.
- **Section format** byte-identity: verified against Perl `:722, :725-728, :729, :746, :825`.
- **`%.2f` for non-zero coverage, empty string for zero coverage**: verified Perl `:740-743`.
- **`coverage = meth + un`**: verified Perl `:744`.
- **Iteration order `[CpG, CHG, CHH]`**: verified Perl `:718` (`qw(CpG CHG CHH)`).
- **Trailing blank line after each section**: verified Perl `:762` (`print MBIAS "\n";`).
- **PNG plots deferred to v1.x**: SPEC §4.2 + plan locked.
- **`is_paired` threaded via `ExtractState::new`** (not derived from `state.mbias[1]` length): more explicit; empty PE BAMs would otherwise be misclassified as SE.

### 9.2 Open questions (rev 1)

1. **(Resolved rev 1)** SPEC §4.2 "4-col" vs Perl's 5-col: fixed inside Phase D's PR (rev 1 Reviewer B C2). One-line edit to `SPEC.md`.
2. **(Open, low-risk)** Position 0 in the mbias vec: the accumulator allocates slot 0 via `Vec::resize`, but `route_call` always passes `pos_1based >= 1` (now also `debug_assert`'d in rev 1). The writer starts iteration at position 1. The unused slot 0 is wasted space (16 bytes); not worth specializing.
3. **(Resolved)** `--mbias_only` deferred to Phase E. Phase D's writer is unchanged by `--mbias_only`; the toggle only affects the route_call short-circuit.
4. **(Resolved)** Filename has different basename derivation from Phase B's split files — separate `derive_mbias_basename` helper, locked by side-by-side divergence test (rev 1).
5. **(Open, Phase E consideration)** `is_paired` as `ExtractState` field vs as direct `finalize` arg (rev 1 Reviewer B I1). Current plan stores as a field. Alternative: `finalize(config, is_paired)` accepts it as a parameter from the caller. Pros of alternative: reduces field count; field count + lifetime story is simpler. Pros of current choice: `finalize` is called from one site per pipeline (extract_se or extract_pe), and threading the bool through state at construction-time mirrors how Phase B/C threaded `mbias_off`/`emit_splitting_report`. Phase E's mode-dispatch revisit may collapse this.
6. **(Open, Phase H concern)** `format!("{:.2}", ...)` uses Rust's banker's rounding (round-half-to-even); Perl's `sprintf("%.2f", ...)` uses libc rounding (typically round-half-away-from-zero on macOS/glibc). At midpoint percent values (`.125%`-like), the two may differ by 1 in the last digit. New test `write_mbias_txt_percent_rounding_matches_perl_at_midpoint` exercises midpoints; if any diverges, switch to a manual half-away-from-zero rounding helper before Phase H gates.

### 9.3 Critical questions

**None.** All design choices have defaults; nothing changes goal/scope/behaviour such that pausing is mandatory.

## 10. Validation

| What to verify | How | Expected |
|----------------|-----|----------|
| Section header bytes (SE) | `write_mbias_txt_se_section_header_format_bytes` | Exactly `"CpG context\n===========\n"` (11 equals). |
| Section header bytes (PE R1, PE R2) | `write_mbias_txt_pe_section_header_format_bytes` | Exactly `"CpG context (R1)\n================\n"` (16 equals). |
| Column header bytes | `write_mbias_txt_column_header_bytes_exact` | Exactly `"position\tcount methylated\tcount unmethylated\t% methylation\tcoverage\n"`. |
| Per-position row with calls | `write_mbias_txt_per_position_row_with_calls` | `"{pos}\t{meth}\t{un}\t{percent}\t{coverage}\n"` with `%.2f` percent. |
| Per-position row with zero coverage | `write_mbias_txt_per_position_row_zero_coverage_empty_percent` | `"{pos}\t0\t0\t\t0\n"` (note `\t\t` between un and coverage). |
| Iterates 1..max_length per context | `write_mbias_txt_iterates_all_positions_up_to_max` | All positions emitted, even with zero counts. |
| Trailing blank line between sections | `write_mbias_txt_blank_line_between_sections` | Section-end is `\n\n` (last row newline + blank-line newline). |
| Empty mbias → headers only | `write_mbias_txt_empty_mbias_emits_headers_only` | 3 section headers + 3 column headers, no per-position rows. |
| Empty R2 PE section | `write_mbias_txt_pe_empty_r2_section_still_emitted` | 6 sections; R2 sections header-only. |
| `--mbias_off` skips writer | `extract_state_finalize_skips_mbias_txt_when_mbias_off` | No M-bias.txt on disk. |
| `--mbias_off=false` enables writer | `extract_state_finalize_writes_mbias_txt_when_not_mbias_off` | M-bias.txt exists on disk. |
| SE smoke produces SE M-bias.txt | `smoke_se_directional_produces_all_12_files_and_report` (extended) | M-bias.txt contains `CpG context\n===========\n`. |
| PE smoke produces PE M-bias.txt | `smoke_pe_auto_detect_produces_all_12_files_and_report` (extended) | M-bias.txt contains `CpG context (R1)\n` and `CpG context (R2)\n`. |
| Phase B + C regression | `cargo test -p bismark-extractor` | All 122 existing tests still green after `ExtractState::new` sig change. |
| Cross-crate regression | `cargo test -p bismark-io -p bismark-dedup` | All green (Phase D doesn't touch bismark-io or bismark-dedup). |
| Clippy + fmt | `cargo clippy -p bismark-extractor --all-targets -- -D warnings && cargo fmt --check` | Clean. |

## 11. Integration with later phases

| Phase | What Phase D leaves for it |
|-------|----------------------------|
| **E** (modes + gzip + `--mbias_only`) | Phase D writer is unchanged by output mode (the M-bias accumulator is mode-agnostic — counts every methylation call regardless of where it routes). Phase E adds the `--mbias_only` toggle that short-circuits the split-file write in `route_call` without changing the M-bias.txt writer call site. |
| **F** (multicore) | M-bias accumulators in per-worker scratch state get merged via element-wise addition (`MbiasTable + MbiasTable` reducer) at the end of the parallel loop, then the single `write_mbias_txt` runs once on the main thread. |
| **G** (bedGraph + cytosine_report) | No M-bias interaction. |
| **H** (byte-identity gate) | M-bias.txt enters the 10M + 55M PE WGBS byte-identity comparison. Endpoint verification: each section's per-position rows must match Perl byte-for-byte including the empty-percent edge case at zero-coverage positions. |

## 12. Self-review

**Efficiency.** ~9 KB per R1 section × 6 sections PE = ~60 KB output. Single 8-KiB `BufWriter<File>` writes the entire file. Negligible cost. The `max_position` scan is O(1) (length lookup on 3 vecs). No per-record overhead.

**Logic.** The writer is a one-shot function called from `state.finalize`. No iterator-shared mutable state. SE vs PE selection happens via the `is_paired` parameter threaded through `ExtractState::new` — both bools (extract_se passes false, extract_pe passes true) — no inference, no ambiguity.

**Edge cases.** Empty mbias, empty R2 PE section, zero-coverage positions, single-context populated, max_position 0, large max_position — all covered by tests in §7.1.

**Integration.** Three small touchpoints: `mbias.rs` (one new method), `state.rs` (one new field + one new param + finalize body), `pipeline.rs` (two-line touch). New file `mbias_writer.rs` is self-contained. No new dependencies. No cross-crate changes.

**Risks remaining.**

- **R1**: Filename byte-identity. Perl's `s/bam$//` regex chain is subtle (preserves trailing dot, applies in order). Mistake here → Phase H byte-identity miss on filename. Mitigated by `derive_mbias_basename_strips_known_suffixes` test covering 4-5 input shapes.
- **R2**: Zero-coverage row's empty-percent `\t\t` is easy to miss in a hand-rolled writer. Mitigated by explicit `write_mbias_txt_per_position_row_zero_coverage_empty_percent` test asserting byte-exact.
- **R3**: SE vs PE section count drift if `is_paired` is wrong. Mitigated by Phase B/C callers explicitly setting it.

## 13. Revision history

- **rev 0** (2026-05-26): initial Phase D plan written. Awaiting manual review → dual plan-reviewer pass → implementation trigger.

## 14. Sub-issue (already filed)

[#852](https://github.com/FelixKrueger/Bismark/issues/852).

## 15. Branching strategy

- **Branch:** `extractor-phase-d` (created off `extractor-phase-c` while Phase B PR #849 and Phase C PR #851 are in review).
- **PR target:** Phase D's PR will be stacked on `extractor-phase-c`. When upstream PRs merge into `rust/iron-chancellor`, rebase Phase D onto fresh iron-chancellor.
- **Rebase risk:** Phase D is short and isolated to `mbias_writer.rs` + small touches to `state.rs`/`pipeline.rs`/`lib.rs`. If Phase B/C revisions touch `state.rs` or `pipeline.rs`, rebase conflicts are likely small (the `is_paired` field addition + one new method call).

## 16. Follow-up tasks

**Rev 1 resolved:** all three known SPEC prose errors (§4.2 surfaced in Phase D, §7.4 surfaced in Phase C, §8.4 surfaced in Phase B) are now in Phase D's PR scope per Reviewer B C2 + retroactive consolidation decision. No follow-up SPEC tasks remain.

If Phase H byte-identity reveals other unflagged surface drift, queue follow-ups then.

**Convention going forward:** if a phase's implementation or review surfaces a SPEC prose error, fix it in the same PR that surfaces it. Avoids the rev-2 prose-drift problem we hit when SPEC §7.4/§8.4 got queued as "follow-ups" that never landed until Phase D consolidated them.
