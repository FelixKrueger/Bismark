# `bismark-extractor` Phase E — output mode dispatch + `--gzip` + `--mbias_only`

**Status:** rev 1 — awaiting implementation trigger.
**Date:** 2026-05-27 (rev 1 same-day after dual plan-review absorption).
**Slug:** `plans/05262026_bismark-extractor/PHASE_E_PLAN.md`.
**Phase target:** SPEC §10 row E — ~400 LOC.
**GitHub sub-issue:** [#854](https://github.com/FelixKrueger/Bismark/issues/854) (filed at work-start).
**Depends on:** PR [#853](https://github.com/FelixKrueger/Bismark/pull/853) (Phase D) → PR [#851](https://github.com/FelixKrueger/Bismark/pull/851) (Phase C) → PR [#849](https://github.com/FelixKrueger/Bismark/pull/849) (Phase B). Stacked branch `extractor-phase-e` based on `extractor-phase-d`.

## Epic linkage

- **Design contract:** `rust/bismark-extractor/SPEC.md` (in-repo, rev 3 after Phase D consolidation). Phase E covers SPEC §4.1 (5-mode filename topology), §6 (structural design — esp. §6.6 subprocess-vs-inline already locked for Phase G), §7.5 (`route_call` output dispatch table for all 5 modes), §11 (no remaining open questions for this scope).
- **GitHub umbrella:** issue [#798](https://github.com/FelixKrueger/Bismark/issues/798).
- **Prior phases:**
  - Phase A merged (commit `144ca2d`, PR #847).
  - Phase B in review (PR #849).
  - Phase C in review (PR #851, stacked on #849).
  - Phase D in review (PR #853, stacked on #851).

## 1. Goal

Unlock the four currently-rejected output modes (`--comprehensive`, `--merge_non_CpG`, `--yacht`, `--mbias_only`), wire `--gzip` to compress all output files, and add a small Phase B kernel residual (`mbias_only_silence` for InvalidXmByte). After Phase E, the binary handles the full output-shape surface at `--parallel 1`; only Phase F (multicore) and Phase G (bedGraph/cytosine_report subprocess chain) remain before Phase H's byte-identity gate.

Concretely:
- **5 output modes** × **plain or gzipped** = 10 distinct file-naming patterns honored.
- **`--mbias_only`** stops emitting per-context split files (Phase D's M-bias.txt writer is untouched — it already runs under the same `!mbias_off` gate).
- **InvalidXmByte under `--mbias_only`** silently skips per Perl `:2972/3054` (`die "..." unless ($mbias_only)`).

## 2. Scope decisions (locked)

| Decision | Choice | Reasoning |
|----------|--------|-----------|
| `OutputFileMap` mode dispatch | **Mode-aware key set + filename pattern computed at `OutputFileMap::new` time** from `config.output_mode`. Each mode has its own `(Vec<OutputKey>, Fn(OutputKey) -> filename)` pair. | Single eager-open code-path; mode-specific routing logic lives in the key/filename derivation, not the write hot path. Phase B's `route_call` short-circuit pattern (compute key, lookup writer, write line) stays unchanged. |
| Output mode → key shape | (see §4.1 detailed table) | Default = 12 keys `(context, strand)`; Comprehensive = 3 keys `(context)`; MergeNonCpG = 8 keys `(cpg_or_noncpg, strand)`; ComprehensiveMergeNonCpG = 2 keys `(cpg_or_noncpg)`; Yacht = 1 key `any_context`; MbiasOnly = 0 keys (no split files). |
| Filename infix correction | **Comprehensive filenames use `CpG_context_*` (NOT just `CpG_*`)** | SPEC §4.1 example says `CpG_{input}.txt`; Perl `:5333` actually emits `CpG_context_{input}.txt`. SPEC fix queued in §16. |
| `--gzip` writer wrapping | **`Box<dyn Write + Send>` inside `BufWriter`** | Allows the same `OutputFileMap::write_call` body to handle both plain `File` and `flate2::write::GzEncoder<File>` without an outer enum. `Box<dyn>` cost is amortized by the 8 KiB BufWriter; vtable hops are <1% of write latency. **The `+ Send` bound is forward-looking for Phase F**: per-worker `OutputFileMap`s will be moved between threads at join time (the writer itself stays in its worker — but moving the owning `OutputFileMap` value into a `thread::JoinHandle::join()` return requires the inner trait object to be `Send`). At Phase E parallel=1 the bound is trivially satisfied by `File` and `GzEncoder<File>`; locked now to avoid a Phase F signature churn. (rev 1 — Reviewer B Important-2 / O9 resolution.) |
| Filename `.gz` suffix | **Append `.gz` to every filename when `config.gzip`** | Matches Perl `:5066` (`$cytosine_output .= '.gz'`) etc. |
| Gzip library | **`flate2 = "=1.0.34"`** (workspace pin) — `flate2::write::GzEncoder` with `Compression::default()` | `noodles_bgzf` is BGZF (Bismark's `--gzip` is plain gzip, not BGZF). Phase B's `BufWriter::with_capacity(8 * 1024, ...)` capacity is fine for both. |
| `--mbias_only` route_call short-circuit | **Already wired in Phase B** (`route_call.rs:64` `if state.mbias_only { return Ok(()); }`). Phase E enables the path at main dispatch by removing the `PhaseNotYetImplemented` reject. | The short-circuit was pre-wired in Phase B per its §6 plan. Phase E flips the bit at `state.mbias_only` (currently always `false` in `ExtractState::new`). |
| `--mbias_only` interaction with `OutputFileMap` | **`OutputFileMap::new` skips eager-open entirely when `config.mbias_only == true`** | Perl `:5148-5151` etc. guard the `open(...)` with `unless($mbias_only)`. No empty header-only files emitted. |
| **`mbias_only_silence: bool` kernel param** (deferred from Phase B) | **Threaded through `extract_calls` from main**: under `--mbias_only`, invalid XM bytes silently skipped (no `InvalidXmByte` error). Mirrors Perl `:2972/3054` (`die "..." unless ($mbias_only)`). | Phase B's plan §9.2 #3 deferred this; Phase E is the natural home since `--mbias_only` becomes reachable. |
| **`--yacht` row format** | 8 tab-separated columns: `read_id<TAB>{+|-}<TAB>chr<TAB>ref_pos<TAB>xm_byte<TAB>read_start<TAB>read_end<TAB>read_orientation` | Perl `:4472, :4485, :4498` etc. — yacht appends 3 columns (start, end, orientation) per call. SE-only mode (`Cli::validate` already rejects `--yacht --paired-end` per Phase A). |
| `--yacht` `read_orientation` value | **`+` if `pair_strand` ∈ {OT, CTOB} (forward class); `-` otherwise** | Perl's `$strand` variable at yacht-print time carries the orientation classification. SE-only, so `record_strand` == `pair_strand`. |
| `is_paired` as `ExtractState` field (Phase D Reviewer B Low #2) | **Keep as field** | Phase E dispatch shape doesn't change the construction story (`extract_se` passes `false`; `extract_pe` passes `true`). Refactor to `finalize(config, is_paired)` would save 1 byte of state but complicate Phase F's reducer pattern (state lives across workers; per-worker bool would have to be reconciled). Locked. |
| Yacht header line | Same Perl version header as other modes when `!--no_header` (per Phase B convention) | Perl `:5077`: `print {$fhs{any_context}} "Bismark methylation extractor version $version\n";` |
| `route_call` strand routing dispatch table | **Inline `match` on `(state.mode, call.context, strand)`** | Closer to SPEC §7.5 pseudocode; Phase B's hardcoded Default branch becomes one match arm of five. Keeps the per-call code path branch-free except for the mode dispatch. |
| Per-context counters in splitting report | **Unchanged** — counts still accumulate per `(context, methylated)` regardless of output mode. Mode only affects WHICH file the call writes to, not the counter increment. | Perl `:4470 total_meCHG_count++` etc. — counters are mode-independent. |
| Cleanup-on-error scope | **Iterate the current mode's key set** (not hardcoded 12) | `OutputFileMap::cleanup_all` already iterates the internal HashMap; no change needed. |

## 3. Context

### 3.1 Source documents read end-to-end

- `rust/bismark-extractor/SPEC.md` §§4.1 (5-mode topology), 6.6 (subprocess/inline locked), 7.5 (route_call dispatch with all 5 modes), 10 (Phase E row), 11 (open questions table — no Phase E items remain).
- Phase B/C/D source: `output.rs` (eager-open 12-file map), `route.rs` (mode dispatch table currently hardcoded to Default), `state.rs` (`mode: OutputMode` field already carried; `mbias_only: bool` already a struct field).
- Perl `bismark_methylation_extractor`:
  - `:1037-1038` — `--mbias_only` × `--bedGraph` mutex (Phase A handles).
  - `:1328-1336` — `--yacht` SE-only enforcement (Phase A handles).
  - `:4465-4640` — yacht-mode per-call write with 3 extra columns.
  - `:5052-5079` — yacht-mode file open + header.
  - `:5082-5132` — `--comprehensive --merge_non_CpG` open (2 files).
  - `:5134-5325` — `--merge_non_CpG` open (8 files).
  - `:5330-5403` — `--comprehensive` open (3 files).
  - `:5405-5700+` — default mode open (12 files, Phase B already implements).
  - `:2972, :3054` — `die "unrecognised char" unless ($mbias_only)` (silent-skip gate).
- Phase B plan §9.2 #3 — `mbias_only_silence` deferred until Phase E.
- Phase D plan §2 row + §11 row "E" — confirms Phase D's writer is unchanged by Phase E; only the `route_call` short-circuit unlocks.

### 3.2 Code placement

All Phase E code lands inside `rust/bismark-extractor/`. **No bismark-io / bismark-dedup touches.**

- **New module:**
  - `rust/bismark-extractor/src/output_mode.rs` — `OutputKey` (mode-aware enum with discriminants for each mode's key shape), `mode_keys(config, basename, no_header)` (compute the per-mode (key, filename, header) triples), helpers for yacht's extra-column row format.
- **Modified modules:**
  - `rust/bismark-extractor/src/output.rs` — `OutputFileMap::new` becomes mode-aware (consumes `output_mode` from `ResolvedConfig`); `BufWriter<File>` → `BufWriter<Box<dyn Write>>`; `write_call` adds yacht-row branch for the 3 extra columns; `cleanup_all` unchanged (iterates whatever's in the map). Skip eager-open entirely when `config.mbias_only`. New private `derive_split_filename(mode, key, basename, gzip) -> String` helper.
  - `rust/bismark-extractor/src/route.rs` — replace the hardcoded `OutputKey { context, strand }` lookup with mode-aware key construction; route by `(state.mode, call.context, strand)` to the correct key shape. The `mbias_only` short-circuit (currently line 64-66) stays.
  - `rust/bismark-extractor/src/call.rs` — re-introduce `mbias_only_silence: bool` parameter on `extract_calls` (deferred from Phase B). Under `mbias_only_silence == true`, `classify_xm_byte` errors on InvalidXmByte are caught and the byte is skipped instead of propagated. Defaults to `false` for non-mbias-only paths.
  - `rust/bismark-extractor/src/pipeline.rs` — `extract_se` / `extract_pe` callsites of `extract_calls` pass `config.output_mode == OutputMode::MbiasOnly`.
  - `rust/bismark-extractor/src/state.rs` — `ExtractState::new` sets `mbias_only = config.output_mode == OutputMode::MbiasOnly` (was hardcoded `false` in Phase B as pre-wiring). No new fields.
  - `rust/bismark-extractor/src/main.rs::run` — **drop the `PhaseNotYetImplemented` rejections** for: `output_mode != Default`, `gzip == true`. Keep multicore, bedGraph/cytosine_report, multiple-inputs rejections.
  - `rust/bismark-extractor/src/lib.rs` — `pub mod output_mode;` + re-export `OutputKey` / `mode_keys` if useful for tests.
  - `rust/bismark-extractor/Cargo.toml` — version bump `1.0.0-alpha.4` → `1.0.0-alpha.5`; add `flate2 = "=1.0.34"` as a regular dep.
  - `rust/bismark-extractor/src/cli.rs` — `--include_overlap` / mutex checks unchanged; `--yacht` SE-only already enforced by Phase A.
  - `rust/bismark-extractor/SPEC.md` §4.1 — **rev 4 correction**: Comprehensive filename example fix (`CpG_{input}.txt` → `CpG_context_{input}.txt`). Per Phase D's convention of "fix SPEC prose in the same PR that surfaces it."
- **Tests:**
  - `rust/bismark-extractor/tests/output_modes_phase_e.rs` — new unit tests (one section per output mode + gzip × plain combinations).
  - `rust/bismark-extractor/tests/output_modes_phase_e_smoke.rs` — new end-to-end smoke (5 modes × spawn binary × assert filenames + content).
  - `rust/bismark-extractor/tests/se_phase_b.rs` — **modified** to pass new params (`mbias_only_silence=false`, `mode=Default`, `gzip=false`) at the 19 ripple sites enumerated in §7.3.
  - `rust/bismark-extractor/tests/pe_phase_c.rs`, `tests/mbias_writer_phase_d.rs`, and the three `*_smoke.rs` siblings: **unchanged** at the API level (they go through `extract_se` / `extract_pe` / `state::new`). May need recompilation but no source edits (rev 1 verification).

### 3.3 Crate versions

- `bismark-extractor`: `1.0.0-alpha.4` → `1.0.0-alpha.5`.
- `bismark-io`: unchanged (`1.0.0-beta.7`).
- `bismark-dedup`: unchanged.

### 3.4 Binary behaviour

After Phase E:
- `--comprehensive`: 3 files `CpG_context_{basename}.txt[.gz]`, `CHG_context_{basename}.txt[.gz]`, `CHH_context_{basename}.txt[.gz]`.
- `--merge_non_CpG`: 8 files (CpG × 4 strands + Non_CpG × 4 strands).
- `--comprehensive --merge_non_CpG`: 2 files `CpG_context_*` + `Non_CpG_context_*`.
- `--yacht`: 1 file `any_C_context_{basename}.txt[.gz]` with 8-col rows.
- `--mbias_only`: 0 split files; M-bias.txt + splitting-report only.
- `--gzip`: any of the above with `.gz` suffix and gzip-compressed content.

## 4. Behaviour specification

### 4.1 Per-mode filename + key set table

Concrete shape for each mode's `OutputFileMap` state. `basename` is from `pipeline::derive_basename` (single-suffix strip per Perl). When `config.gzip`, append `.gz` to each filename and wrap each writer in `GzEncoder<File>`.

| Mode | # files | Key shape | Filename pattern | Perl source |
|------|--------:|-----------|------------------|-------------|
| `Default` (Phase B status quo) | 12 | `(CytosineContext, BismarkStrand)` | `{Context}_{Strand}_{basename}.txt[.gz]` | `:5405-5700+` |
| `Comprehensive` | 3 | `CytosineContext` | `{Context}_context_{basename}.txt[.gz]` (note `_context_` infix) | `:5333, 5357, 5381` |
| `MergeNonCpG` | 8 | `(CpGOrNonCpG, BismarkStrand)` | `{CpG\|Non_CpG}_{Strand}_{basename}.txt[.gz]` | `:5139, 5161+` |
| `ComprehensiveMergeNonCpG` | 2 | `CpGOrNonCpG` | `{CpG\|Non_CpG}_context_{basename}.txt[.gz]` | `:5085, 5109` |
| `Yacht` | 1 | `()` (unit) | `any_C_context_{basename}.txt[.gz]` | `:5058` |
| `MbiasOnly` | 0 | — | — (no files) | `:5148-5151 unless($mbias_only)` |

### 4.2 Row format per mode

All modes except Yacht emit Phase B's 5-col row:
```
read_id<TAB>{+|-}<TAB>chr<TAB>ref_pos<TAB>xm_byte<LF>
```

**Yacht mode** emits 8 cols (3 extra appended):
```
read_id<TAB>{+|-}<TAB>chr<TAB>ref_pos<TAB>xm_byte<TAB>col6<TAB>col7<TAB>read_orientation<LF>
```

**Col-6 / col-7 derivation is strand-conditional** to match Perl `:4350, 4382-4384, 4403-4409, 4422-4447` byte-for-byte. Perl initialises `$end = $start` (line 4350), adjusts `$end` only when `$strand eq '+'` (lines 4382, 4406), and then adjusts `$start` upward when `$strand eq '-'` (lines 4427, 4442). The yacht print at lines 4472+ emits `($start, $end)` literally — so for `-` strand reads, the printed order is `(corrected_start, original_alignment_start)` where col-6 > col-7.

| pair_strand | col-6 ("start") | col-7 ("end") |
|------------|-----------------|---------------|
| OT (`+`)   | `record.alignment_start()` | `record.cigar().reference_end(alignment_start)` |
| CTOB (`+`) | `record.alignment_start()` | `record.cigar().reference_end(alignment_start)` |
| OB (`-`)   | `record.cigar().reference_end(alignment_start)` (= 3′-most ref pos) | `record.alignment_start()` (= original 5′ position) |
| CTOT (`-`) | `record.cigar().reference_end(alignment_start)` | `record.alignment_start()` |

In other words: forward-class emits `(small, large)`; reverse-class emits `(large, small)` because Perl literally swaps the semantic meanings of `$start`/`$end` for `-` reads. The downstream `NOMe_filtering` consumer relies on this polarity to classify fragment orientation, so we must mirror Perl exactly.

- `read_orientation` (col-8) = `+` for forward-class pair_strand (OT|CTOB), `-` for reverse-class (OB|CTOT). For SE (always reached because `--yacht` is SE-only), `pair_strand == record_strand`. Source is Perl `$strand` variable (`:1604, 1607, 1610, 1613` — the OT/CTOT/CTOB/OB classification chain that yields the literal `+`/`-` string), **not** the SAM flag bit 16.

Reference: Perl `:4350, 4382-4384, 4403-4409, 4422-4447, 4472, 4485, 4498, 4511, 4524, 4537, 4572` — start/end adjustment + the 8-tuple join.

### 4.3 `--gzip` writer wrapping

The Phase B `OutputFileMap`'s map structure is `HashMap<OutputKey, (PathBuf, BufWriter<File>)>`. Phase E changes the value type to `(PathBuf, BufWriter<Box<dyn Write + Send>>)`:

```rust
use flate2::Compression;
use flate2::write::GzEncoder;

fn open_writer(path: &Path, gzip: bool) -> Result<BufWriter<Box<dyn Write + Send>>, std::io::Error> {
    let file = File::create(path)?;
    let inner: Box<dyn Write + Send> = if gzip {
        Box::new(GzEncoder::new(file, Compression::default()))
    } else {
        Box::new(file)
    };
    Ok(BufWriter::with_capacity(8 * 1024, inner))
}
```

The outer `BufWriter::flush()` propagates to the inner `GzEncoder`, which writes the gzip footer on drop. Explicit `flush_all` (called from `state.finalize`) is sufficient — `GzEncoder::try_finish` is **not** called explicitly because `Drop` does it; an early-exit error path leaves an inconsistent gzip stream, but the cleanup-on-error path deletes the file anyway.

**Important invariant** (rev 0 finding): `GzEncoder::drop` flushes; if `flush_all` returned `Ok` then by definition the GzEncoder hasn't seen a write since then. The gzip footer is written when the inner BufWriter drops the GzEncoder. Tests must verify the resulting `.gz` is valid by round-trip decoding.

### 4.4 `--mbias_only` enablement

Three changes:

1. **`main.rs::run`**: remove the `PhaseNotYetImplemented` reject for `config.is_mbias_only()`.
2. **`OutputFileMap::new`**: when `config.is_mbias_only()`, skip eager-open entirely. The map is empty; `write_call` never gets called (route_call short-circuits earlier); `cleanup_all` is a no-op on the empty map; `flush_all` no-ops.
3. **`ExtractState::new`**: set `mbias_only = config.is_mbias_only()` instead of hardcoded `false`.

All three sites consult `ResolvedConfig::is_mbias_only()` (§5.5) — no independent re-derivation.

Phase D's M-bias.txt writer at `state.finalize` is unchanged — it runs whenever `!config.mbias_off`, regardless of `output_mode`. Under `--mbias_only`, the M-bias.txt + splitting-report are the only outputs.

### 4.5 `mbias_only_silence: bool` kernel param

Phase B's `extract_calls(record, ignore_5p, ignore_3p)` becomes `extract_calls(record, ignore_5p, ignore_3p, mbias_only_silence)`. Behavior:

- `mbias_only_silence == false` (Phases B/C/D default): InvalidXmByte returns `Err(BismarkExtractorError::InvalidXmByte { byte, ref_pos, read_id })`, propagated up.
- `mbias_only_silence == true` (Phase E `--mbias_only` path): InvalidXmByte is silently skipped (the offending byte produces no call, but the loop continues). Mirrors Perl `:2972/3054` (`die "..." unless ($mbias_only)`).

Implementation: wrap the `classify_xm_byte(...)?` in `extract_calls` with a `match` that branches on `mbias_only_silence`. **The silence arm matches only the `InvalidXmByte` variant explicitly** (rev 1 narrowing per reviewers A2 / Important-5) — Perl `:2972/3054` only suppresses the unrecognised-character `die`; other `classify_xm_byte` failure modes (if any are added later) must still propagate:

```rust
use crate::error::BismarkExtractorError;

let classification = classify_xm_byte(aligned.xm_byte, aligned.ref_pos, &read_id);
match classification {
    Ok(XmClassification::Call(context, methylated)) => calls.push(MethCall { ... }),
    Ok(XmClassification::SkipUnknownContext | XmClassification::SkipNonCytosine) => {}
    // Narrow: only InvalidXmByte is silenced, mirroring Perl `:2972/3054`
    // (`die "..." unless ($mbias_only)`). Any future error variants in
    // `classify_xm_byte` continue to propagate even under --mbias_only.
    Err(BismarkExtractorError::InvalidXmByte { .. }) if mbias_only_silence => {}
    Err(e) => return Err(e),
}
```

Note: `.`, `u`, `U` are returned by `classify_xm_byte` as `Ok(XmClassification::SkipNonCytosine)` / `Ok(SkipUnknownContext)` (Phase B), so they take the `Ok(Skip*)` arm regardless of `mbias_only_silence` — matching Perl's unconditional no-op behaviour for those bytes at `:2969-2971, 3051-3053`.

Pass through from `pipeline.rs`:
```rust
let mbias_only_silence = config.output_mode == OutputMode::MbiasOnly;
// ... in the loop:
let calls = extract_calls(&record, config.ignore_5p_r1, config.ignore_3p_r1, mbias_only_silence)?;
```

### 4.6 Edge cases

| Case | Handling |
|------|----------|
| Mode-flag combos already rejected at Phase A | `--mbias_only --bedGraph` (Perl `:1037-1038`), `--yacht --paired-end` (Perl `:1328-1336`), `--yacht --mbias_only` (Phase A `Cli::validate`) — Phase E doesn't relax these. |
| `--mbias_only` + empty BAM | No split files, M-bias.txt has header-only sections, splitting-report has zero counts. Matches Perl. |
| `--gzip` + empty BAM | 12 (or N for mode) header-only .gz files + .gz M-bias.txt. Each .gz file decompresses to the version header line + nothing. |
| Yacht + `--no_header` | Empty file (header suppressed); per-call rows still emitted. |
| Yacht record with `record.alignment_start() == None` | Defensive `InternalError` (same as Phase C cross-chr defensive check). Filtered upstream by bismark-io's unmapped-filter; shouldn't fire in practice. |
| Comprehensive + `--mbias_only` | Effectively `--mbias_only` (mbias_only short-circuits before mode-routing); no comprehensive files emitted. Phase A `Cli::validate` already enforces `--mbias_only` is mutex with `--bedGraph`/`--cytosine_report`/`--mbias_off` but NOT with mode flags. Test fixture covers. |
| `--gzip` + `cleanup_partial_outputs` on **clean error path** | `cleanup_all` removes the `.gz` files. Same removal mechanism (`std::fs::remove_file`). |
| `--gzip` + **panic** mid-write (rev 1) | Partial `.gz` files left on disk in possibly-truncated state. `cleanup_all` is only invoked by `main.rs::run`'s `Result::Err` handler — a Rust panic unwinds past that handler, so cleanup is **not** invoked. Drop impls flush silently, but the gzip stream may still be missing trailing data. Acknowledged limitation; not worth a `catch_unwind`/panic-hook at Phase E. Phase H byte-identity gate skips panic scenarios. (Reviewer B Important-6.) |
| Yacht under `--mbias_only` | Already rejected by Phase A. Defensive: Phase E's mode-dispatch + mbias_only short-circuit interact safely (yacht's any_context file isn't opened; mbias_only short-circuits before write attempt). |
| `--mbias_only` invalid XM byte | Silently skipped per Perl `:2972 die unless ($mbias_only)`. Test fixture exercises. |
| Yacht record with `record.alignment_start() == None` (rev 1) | `route_call` emits `BismarkExtractorError::InternalError` (not a silent `0` row). Filtered upstream by `bismark-io`'s unmapped-filter; shouldn't fire in practice. Snippet in §5.3 implements this explicitly. |
| Yacht record with `alignment_start` or `reference_end` overflowing `u32` (rev 1) | `u32::try_from` returns `InternalError`. Human/mouse positions fit comfortably in u32 (max ~250M); the guard exists for unusual contigs. |
| `--mbias_only --comprehensive` / `--mbias_only --merge_non_CpG` warning emission (rev 1) | Perl `:1043-1048` emits a `warn "..."` to stderr when these are combined; Phase E silently lets `--mbias_only` win. Functionally equivalent (no split files emitted either way) but stderr divergence; outside Phase H byte-identity scope (stderr is not gated). (Reviewer A G6.) |

## 5. Signatures (proposed)

### 5.1 `output_mode.rs` (NEW)

```rust
//! Mode-aware output-file key + filename generation for Phase E.

use bismark_io::BismarkStrand;
use crate::call::CytosineContext;
use crate::cli::OutputMode;

/// CpG vs Non-CpG categorisation used by `MergeNonCpG` modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CpGOrNonCpG {
    CpG,
    NonCpG,
}

impl CytosineContext {
    pub(crate) fn cpg_or_non_cpg(self) -> CpGOrNonCpG {
        if matches!(self, CytosineContext::CpG) { CpGOrNonCpG::CpG } else { CpGOrNonCpG::NonCpG }
    }
}

/// One output-file key. The enum discriminant is the mode; payload is
/// the mode's per-key shape (see §4.1 table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputKey {
    /// Default mode: 12 (context, strand) pairs.
    Default(CytosineContext, BismarkStrand),
    /// Comprehensive mode: 3 contexts only.
    Comprehensive(CytosineContext),
    /// MergeNonCpG mode: 8 (CpG_or_NonCpG, strand) pairs.
    MergeNonCpG(CpGOrNonCpG, BismarkStrand),
    /// Comprehensive + MergeNonCpG: 2 keys.
    ComprehensiveMergeNonCpG(CpGOrNonCpG),
    /// Yacht: 1 key.
    Yacht,
}

/// Build the per-mode (key, filename) list for `OutputFileMap::new`.
/// Returns empty Vec for `MbiasOnly` mode (no split files).
///
/// **ORDERING IS LOAD-BEARING.** Files are opened by `OutputFileMap::new`
/// in the order returned here, and `cleanup_partial_outputs` iterates the
/// resulting HashMap. The order MUST mirror Perl's per-mode `open(...)`
/// reading order (`:5082-5403`) so:
///   - Eager-open error messages report the same "failed at file N" as Perl.
///   - Phase H byte-identity diagnostics line up file-by-file.
///   - Cleanup-on-error deletes in a deterministic order that's stable
///     across cargo test invocations.
///
/// Documented order (matches Perl source-code order):
///   Default                  — CpG_OT, CpG_CTOT, CpG_CTOB, CpG_OB,
///                              CHG_OT, CHG_CTOT, CHG_CTOB, CHG_OB,
///                              CHH_OT, CHH_CTOT, CHH_CTOB, CHH_OB.
///   Comprehensive            — CpG_context, CHG_context, CHH_context.
///   MergeNonCpG              — CpG_OT, CpG_CTOT, CpG_CTOB, CpG_OB,
///                              Non_CpG_OT, Non_CpG_CTOT, Non_CpG_CTOB, Non_CpG_OB.
///   ComprehensiveMergeNonCpG — CpG_context, Non_CpG_context.
///   Yacht                    — any_C_context.
pub fn mode_keys(
    mode: OutputMode,
    basename: &str,
    gzip: bool,
) -> Vec<(OutputKey, String)>;

/// Yacht-mode row format (8 columns including read_start/end/orientation).
pub(crate) fn format_yacht_row(
    record_name: &[u8],
    chr: &str,
    call: &MethCall,
    record_start_1based: u32,
    record_end_1based: u32,
    pair_strand: BismarkStrand,
) -> Vec<u8>;
```

### 5.2 `output.rs` modifications

```rust
pub struct OutputFileMap {
    // Phase B's HashMap value type widens to support gzip + dyn dispatch.
    files: HashMap<OutputKey, (PathBuf, BufWriter<Box<dyn Write + Send>>)>,
    // Phase E adds:
    mode: OutputMode,
}

impl OutputFileMap {
    /// Eagerly open all per-mode files; skip entirely when mbias_only.
    pub fn new(
        output_dir: &Path,
        input_basename: &str,
        no_header: bool,
        mode: OutputMode,
        gzip: bool,
    ) -> Result<Self, std::io::Error>;

    /// Phase E: route by mode + write call. For yacht mode, uses the
    /// 8-column format; for other modes, the existing 5-column format.
    pub fn write_call(
        &mut self,
        record_name: &[u8],
        chr: &str,
        call: MethCall,
        strand: BismarkStrand,
        // Phase E additions for yacht-mode metadata:
        record_start_1based: u32,
        record_end_1based: u32,
    ) -> Result<(), BismarkExtractorError>;
}
```

### 5.3 `route.rs` — mode-aware key construction

```rust
pub fn route_call(
    state: &mut ExtractState,
    record: &BismarkRecord,
    chr: &str,
    strand: BismarkStrand,
    call: MethCall,
    read_identity: ReadIdentity,
) -> Result<(), BismarkExtractorError> {
    // 1. M-bias accumulation (unchanged from Phase B).
    if !state.mbias_off { /* ... */ }

    // 2. Splitting-report counter increment (unchanged).
    /* ... */

    // 3. mbias_only short-circuit (unchanged).
    if state.mbias_only {
        return Ok(());
    }

    // 4. Phase E: mode-aware key construction. Yacht passes extra
    //    metadata to write_call for the 3 extra columns. Non-yacht
    //    modes pass (0, 0) sentinels — write_call ignores them.
    let (yacht_col6, yacht_col7) = if state.mode == OutputMode::Yacht {
        let alignment_start: u32 = record
            .alignment_start()
            .ok_or(BismarkExtractorError::InternalError {
                message: "yacht record missing alignment_start (should be filtered upstream)".to_string(),
            })?
            .try_into()
            .map_err(|_| BismarkExtractorError::InternalError {
                message: "alignment_start overflows u32".to_string(),
            })?;
        let ref_end_usize = record.cigar().reference_end(alignment_start as usize);
        let ref_end: u32 = u32::try_from(ref_end_usize).map_err(|_| BismarkExtractorError::InternalError {
            message: "cigar reference_end overflows u32".to_string(),
        })?;
        // Strand-conditional polarity per Perl :4350, 4382, 4406, 4422, 4435, 4443.
        match strand {
            BismarkStrand::OT | BismarkStrand::CTOB => (alignment_start, ref_end),
            BismarkStrand::OB | BismarkStrand::CTOT => (ref_end, alignment_start),
        }
    } else {
        (0, 0)
    };
    let qname: &[u8] = record.inner().name().map_or(b"<unnamed>".as_slice(), |n| n.as_ref());
    state.fhs.write_call(qname, chr, call, strand, yacht_col6, yacht_col7)
}
```

### 5.4 `call.rs::extract_calls` — restore `mbias_only_silence` param

```rust
pub fn extract_calls(
    record: &BismarkRecord,
    ignore_5p: u32,
    ignore_3p: u32,
    mbias_only_silence: bool,  // NEW (deferred from Phase B per §9.2 #3)
) -> Result<Vec<MethCall>, BismarkExtractorError>;
```

### 5.5 `pipeline.rs` callsite updates

The `mbias_only` predicate is centralised on `ResolvedConfig` as a single method (rev 1, per Reviewer B Important-1) so the three derivation sites — `ExtractState::new`, `OutputFileMap::new`, and `pipeline.rs::extract_{se,pe}` — read the same source of truth and can never drift:

```rust
// In src/config.rs (or wherever ResolvedConfig lives):
impl ResolvedConfig {
    /// True when --mbias_only is in effect: skip per-context file writes and
    /// silence InvalidXmByte. M-bias.txt + splitting-report still produced.
    pub fn is_mbias_only(&self) -> bool {
        self.output_mode == OutputMode::MbiasOnly
    }
}
```

Call sites:

```rust
// extract_se body:
let mbias_only_silence = config.is_mbias_only();
let calls = extract_calls(&record, config.ignore_5p_r1, config.ignore_3p_r1, mbias_only_silence)?;

// extract_pe body (handle_one_pair) — same flag for both R1 and R2 (Phase B
// already enforces symmetry; rev 1 test `pe_mbias_only_silence_on_r2` confirms):
let r1_calls = extract_calls(pair.r1(), config.ignore_5p_r1, config.ignore_3p_r1, mbias_only_silence)?;
let r2_calls_raw = extract_calls(pair.r2(), config.ignore_5p_r2, config.ignore_3p_r2, mbias_only_silence)?;
```

### 5.6 `state.rs::ExtractState::new`

```rust
mbias_only: config.is_mbias_only(),  // was hardcoded false in Phase B pre-wiring
```

### 5.7 `main.rs::run`

```rust
// REMOVED Phase E unblocks:
// if config.output_mode != OutputMode::Default { return Err(PhaseNotYetImplemented { ... }) }
// if config.gzip { return Err(PhaseNotYetImplemented { ... }) }
// KEPT: --parallel > 1 (Phase F), --bedGraph / --cytosine_report (Phase G), --multiple input files.
```

## 6. Implementation outline

1. **SPEC fix** (per convention): edit `rust/bismark-extractor/SPEC.md` §4.1 to correct Comprehensive example from `CpG_{input}.txt` to `CpG_context_{input}.txt` (matches Perl `:5333`). One-line edit + Perl line cite.
2. **Add `flate2 = "=1.0.34"` dep** in `Cargo.toml`. Version bump `alpha.4` → `alpha.5`.
3. **Create `src/output_mode.rs`**: `CpGOrNonCpG`, `OutputKey`, `mode_keys`, `format_yacht_row`. ~150 LOC.
4. **Refactor `src/output.rs`**:
   - Change map value type to `(PathBuf, BufWriter<Box<dyn Write + Send>>)`.
   - `OutputFileMap::new` now takes `mode: OutputMode, gzip: bool`; consults `mode_keys(mode, basename, gzip)` to enumerate files to open; skips entirely when mode is `MbiasOnly`.
   - Helper `open_writer(path, gzip) -> Result<BufWriter<Box<dyn Write + Send>>>` factories the plain-vs-gz dispatch.
   - `write_call` accepts the two new yacht-metadata args; dispatches to `format_meth_line` (existing 5-col) or `format_yacht_row` (new 8-col) based on `self.mode == OutputMode::Yacht`.
5. **Refactor `src/route.rs`**: `route_call` now computes `record_start` + `record_end` + qname; passes to `write_call`. `mbias_only` short-circuit position unchanged.
6. **Restore `mbias_only_silence` in `src/call.rs::extract_calls`**: re-introduce the parameter; branch on it in the error path of `classify_xm_byte`. Add a test fixture for the silent-skip case.
7. **Update `src/pipeline.rs`** callsites: compute `mbias_only_silence` from `config.output_mode == OutputMode::MbiasOnly`; pass to `extract_calls`.
8. **Update `src/state.rs::ExtractState::new`**: set `mbias_only` from `config.output_mode == OutputMode::MbiasOnly`.
9. **Update `src/main.rs::run`**: drop the `OutputMode != Default` rejection; drop the `--gzip` rejection. Keep multicore + bedgraph/cytosine_report + multiple-files.
10. **Update `src/lib.rs`**: `pub mod output_mode`, re-export `OutputKey` / `mode_keys` if useful for tests.
11. **Tests** (§7).
12. **`cargo test -p bismark-extractor && cargo clippy --all-targets -- -D warnings && cargo fmt --check`**.

## 7. Tests

### 7.1 Unit tests (`tests/output_modes_phase_e.rs`)

| Test | Asserts |
|------|---------|
| `mode_keys_default_has_12_keys` | `mode_keys(Default, "x", false).len() == 12`; each key is `OutputKey::Default(_, _)`. |
| `mode_keys_comprehensive_has_3_keys` | `len == 3`; filenames are `CpG_context_x.txt`, `CHG_context_x.txt`, `CHH_context_x.txt`. |
| `mode_keys_merge_non_cpg_has_8_keys` | `len == 8`; CpG × 4 strands + Non_CpG × 4 strands. |
| `mode_keys_comprehensive_merge_non_cpg_has_2_keys` | `len == 2`; `CpG_context_x.txt` + `Non_CpG_context_x.txt`. |
| `mode_keys_yacht_has_1_key` | `len == 1`; filename `any_C_context_x.txt`. |
| `mode_keys_mbias_only_has_0_keys` | `len == 0` (no split files). |
| `mode_keys_gzip_appends_dot_gz_to_all_filenames` | For each mode, with `gzip=true`, every filename ends `.gz`. |
| `output_file_map_skips_eager_open_for_mbias_only` | `OutputFileMap::new(..., MbiasOnly, false)` → empty map; output dir has zero files; `flush_all()` and `cleanup_all()` both return `Ok` on the empty map (rev 1 — Reviewer A A5). |
| `output_file_map_comprehensive_creates_3_files_with_context_infix` | Eager-open produces `CpG_context_x.txt`, `CHG_context_x.txt`, `CHH_context_x.txt` on disk. |
| `output_file_map_merge_non_cpg_creates_8_files` | Eager-open produces CpG_OT/CTOT/CTOB/OB + Non_CpG_OT/CTOT/CTOB/OB. |
| `output_file_map_yacht_creates_1_file` | Eager-open produces `any_C_context_x.txt`. |
| `output_file_map_gzip_writes_valid_gz_content` | `OutputFileMap::new(..., Default, true)` + write one call + flush → decompress the file via `GzDecoder` → assert content matches the plain-mode equivalent byte-for-byte. |
| `output_file_map_default_mode_write_routes_to_correct_key` | Phase B regression: same as `route_call_default_mode_routes_to_strand_specific_file`. |
| `output_file_map_comprehensive_write_drops_strand_routing` | CpG-OT and CpG-OB calls both land in `CpG_context_x.txt` (one file per context, ignoring strand). |
| `output_file_map_merge_non_cpg_routes_X_to_non_cpg` | CHG-methylated (`X`) call lands in `Non_CpG_OT_x.txt` (not `CHG_OT_x.txt`). |
| `output_file_map_merge_non_cpg_routes_x_to_non_cpg` | CHG-unmethylated (`x`) call lands in `Non_CpG_OT_x.txt` (rev 1 — Reviewer B I4). |
| `output_file_map_merge_non_cpg_routes_H_to_non_cpg` | CHH-methylated (`H`) call lands in `Non_CpG_OT_x.txt` (rev 1 — Reviewer B I4). |
| `output_file_map_merge_non_cpg_routes_h_to_non_cpg` | CHH-unmethylated (`h`) call lands in `Non_CpG_OT_x.txt` (rev 1 — Reviewer B I4). |
| `output_file_map_comprehensive_merge_non_cpg_routes_chh_to_non_cpg` | CHH-meth call lands in `Non_CpG_context_x.txt`. |
| `format_yacht_row_forward_strand_has_8_columns` | OT-strand call produces `read1\t+\tchr1\t100\tZ\t90\t140\t+\n` (col-6 < col-7). |
| `format_yacht_row_reverse_strand_swaps_col6_col7` | **Critical-1 regression guard.** OB-strand call with `alignment_start=90`, `reference_end=140` produces `read1\t-\tchr1\t...\t140\t90\t-\n` — col-6 > col-7 per Perl `:4350, 4382, 4422-4447` (rev 1). |
| `yacht_row_orientation_plus_for_forward_class` | OT pair_strand → `+` orientation; CTOB → `+`. |
| `yacht_row_orientation_minus_for_reverse_class` | OB → `-`; CTOT → `-`. |
| `extract_calls_mbias_only_silence_skips_invalid_xm_byte` | XM containing `Q` with `mbias_only_silence=true` → returns `Ok` with skipped position; calls before and after the bad byte are preserved. |
| `extract_calls_mbias_only_silence_false_errors_on_invalid_xm_byte` | Same XM with `mbias_only_silence=false` → returns `Err(InvalidXmByte)`. |
| `extract_calls_mbias_only_silence_preserves_dot_and_u_paths` | XM `Z.uZ` with `mbias_only_silence=true` → both `.` and `u` still take `XmClassification::Skip*` arms (no error path); call count identical to `mbias_only_silence=false` on same XM (rev 1 — Reviewer B O1). |
| `pe_mbias_only_silence_on_r2` | PE record with invalid byte on R2's XM + `mbias_only_silence=true` → no error; R1 calls preserved (rev 1 — Reviewer A V3). |
| `extract_state_new_mbias_only_sets_mbias_only_true` | `ExtractState::new` with `config.output_mode == MbiasOnly` → `state.mbias_only == true`. |
| `extract_state_new_non_mbias_only_sets_mbias_only_false` | All other modes → `state.mbias_only == false`. |
| `main_accepts_comprehensive_no_longer_rejected` | `--comprehensive` no longer returns `PhaseNotYetImplemented` (passes phase-gate; fails downstream because tempfile isn't a real BAM, but the rejection text is absent). |
| `main_accepts_merge_non_CpG_no_longer_rejected` | Same shape. |
| `main_accepts_yacht_no_longer_rejected` | Same shape. |
| `main_accepts_mbias_only_no_longer_rejected` | Same shape. |
| `main_accepts_gzip_no_longer_rejected` | Same shape. |
| `main_still_rejects_multicore` | `--parallel 4` still returns `PhaseNotYetImplemented` (Phase F gate). |
| `main_still_rejects_bedgraph` | Phase G gate intact. |
| `main_still_rejects_multiple_input_files` | Two positional args still rejected. |

### 7.2 End-to-end smoke (`tests/output_modes_phase_e_smoke.rs`)

Synthetic ~5-record SE OT BAM (reused from Phase B smoke). Each smoke test:
1. Build the BAM with `BamWriter::from_path`.
2. Spawn the binary with the appropriate mode flag(s).
3. Assert exit 0, the expected file set on disk, content (gzip or plain), and splitting-report shape.

| Smoke test | Asserts |
|------------|---------|
| `smoke_comprehensive_emits_3_files` | `CpG_context_*.txt`, `CHG_context_*.txt`, `CHH_context_*.txt` exist; no `_OT_` / `_OB_` files. M-bias.txt + splitting-report still present. |
| `smoke_merge_non_cpg_emits_8_files` | CpG × 4 + Non_CpG × 4 files; CHG/CHH calls routed to Non_CpG. |
| `smoke_comprehensive_merge_non_cpg_emits_2_files` | `CpG_context_*.txt` + `Non_CpG_context_*.txt` only. |
| `smoke_yacht_emits_1_file_with_8_col_rows` | `any_C_context_*.txt` exists; each row has 8 tab-separated columns. |
| `smoke_mbias_only_emits_no_split_files` | M-bias.txt + splitting-report only; no per-context files; output dir has exactly 2 files. |
| `smoke_gzip_default_emits_12_gz_files_with_valid_content` | All 12 `.gz` files exist; each decompresses to header + content; byte-identical to non-gzip equivalent on the same input. |
| `smoke_gzip_comprehensive_emits_3_gz_files` | Same shape for 3-file comprehensive + gzip combo. |
| `smoke_gzip_mbias_only_emits_no_gz_files` | `--gzip --mbias_only`: output dir contains exactly 2 files (M-bias.txt + splitting-report); zero `.gz` artifacts (rev 1 — Reviewer A V1). |
| `smoke_yacht_gzip_emits_1_gz_file_with_8_col_rows` | `--yacht --gzip`: single `any_C_context_*.txt.gz`; decompressed content has 8-col rows including a reverse-strand row with col-6 > col-7 (rev 1 — Reviewer A V2 + Critical-1 end-to-end check). |
| `smoke_yacht_empty_bam_emits_header_only` | Empty SE BAM + `--yacht`: `any_C_context_*.txt` contains only the version header (rev 1 — Reviewer A V4). |
| `smoke_mbias_only_invalid_xm_byte_silently_skipped` | Synthetic BAM with `Q` in one XM string + `--mbias_only` → exit 0 (no `InvalidXmByte` error); M-bias.txt has counts that exclude the offending position. |
| `smoke_mbias_only_counters_match_default_mode` | Run same BAM under Default mode and under `--mbias_only`; assert splitting-report `(total_meCHG, total_unmeCHG, total_meCpG, ...)` counts are byte-identical. Verifies the route-level short-circuit lives **after** counter increments per Phase B `route.rs` (rev 1 — Reviewer B I5). |
| `smoke_gzip_cleanup_on_write_failure_removes_gz_files` | Inject a write failure mid-run (e.g. SIGPIPE-on-pipe or read-only output dir partway through), trigger `cleanup_partial_outputs`; assert no `.gz` artifacts remain in output dir. Proves §12 R2's claim (rev 1 — Reviewer A V5). |

### 7.3 Phase B/C/D regression

`cargo test -p bismark-extractor` should pass all 151 existing tests plus the new Phase E ones.

**Enumerated ripple sites** (verified by `grep` at rev 1 time — Reviewer A G2/G3):

`extract_calls` (11 test sites + 3 production sites):
- Tests — `tests/se_phase_b.rs:201, 214, 224, 246, 271, 282, 296, 311, 328, 343, 686`.
- Production — `src/pipeline.rs:140` (SE extract loop), `src/pipeline.rs:312` (PE R1), `src/pipeline.rs:313` (PE R2).
- Phase C/D tests (`pe_phase_c.rs`, `mbias_writer_phase_d.rs`, smoke files) go through `extract_se` / `extract_pe` higher-level entry points and **do not** touch `extract_calls` directly.

`OutputFileMap::new` (8 test sites + 1 production site):
- Tests — `tests/se_phase_b.rs:404, 432, 453, 468, 479, 499, 518, 538`.
- Production — `src/state.rs:63`.
- Phase D's `mbias_writer_phase_d.rs` constructs `OutputFileMap` only via `ExtractState::new`, so the ripple flows through that single call site.

**Signature param positions are locked at the end** so the touch is mechanical search-and-replace:
- `extract_calls(record, ignore_5p, ignore_3p)` → `extract_calls(record, ignore_5p, ignore_3p, mbias_only_silence)`.
- `OutputFileMap::new(output_dir, basename, no_header)` → `OutputFileMap::new(output_dir, basename, no_header, mode, gzip)`.

**Note**: per Phase D's review-hygiene precedent (Reviewer B I2), Phase E modifies `tests/se_phase_b.rs` in-place because PR #849 is still in review. The signature change is a structural ripple that any phase touching `extract_calls` or `OutputFileMap::new` would create — there's no way to avoid the touch. Document the ripple in the commit message and rebase Phase B's PR onto Phase E's branch if #849 picks up review feedback between now and merge.

## 8. Efficiency

- `OutputKey` enum is `Copy` and small (≤ 8 bytes per variant). HashMap lookup unchanged from Phase B.
- `Box<dyn Write>` adds one vtable call per write; offset by the 8 KiB BufWriter that amortizes ~10K rows per syscall. Phase F (multicore) is the relevant profiling pass.
- `GzEncoder::default()` uses level 6 (zlib default); ratio ~5-8× on typical methylation-call text. Hot for short reads; not a Phase E concern.
- `extract_calls` per-record path unchanged except for one extra arg in the err-branch match — zero-cost in the happy path.
- Yacht's `format_yacht_row` allocates one `Vec<u8>` per call vs. Phase B's incremental `write_all` calls. Acceptable for Phase E; revisit at Phase F if profiling reveals pressure.

Profile target (informational): Phase E shouldn't regress Phase D's throughput. Gzip path is expected to be 2-3× slower (compression CPU); that's the intended trade-off.

## 9. Assumptions + open questions

### 9.1 Locked assumptions

- **Comprehensive filenames use `_context_` infix**: Perl `:5333` (Phase D-style verification done by reading the actual source).
- **Yacht 8-col row format**: 5 base cols + `col6, col7, orientation`. Perl `:4472, 4485, 4498, ...`.
- **Yacht col-6/col-7 polarity is strand-conditional** (rev 1 correction): forward-class emits `(alignment_start, reference_end)`; reverse-class emits `(reference_end, alignment_start)`. Perl `:4350, 4382, 4406, 4422-4447`. The reverse-class swap is preserved because downstream `NOMe_filtering` reads col-6/col-7 to classify fragment orientation.
- **Yacht `read_orientation` value**: `+` for forward class (OT|CTOB), `-` for reverse (OB|CTOT). Phase C's `is_forward_pair_strand` helper.
- **`--mbias_only` skips eager-open**: Perl `:5148 unless($mbias_only)`. No empty header-only files in `--mbias_only` mode.
- **`mbias_only_silence` is plain-fallthrough on InvalidXmByte**: Perl `die "..." unless ($mbias_only)`. No counter increment for the offending position (it's silently skipped, not "treated as `.`").
- **`flate2` workspace pin**: `=1.0.34` (matches the version transitively pulled by `noodles_bgzf`). Avoids dep duplication. **Implementer MUST verify before committing** by running `cargo tree -p bismark-extractor | grep flate2` and confirming exactly one `flate2` line — if `noodles_bgzf` has bumped since the plan was written, re-pin to whichever version is already transitively present. (rev 1 — Reviewer A A3/R4, Reviewer B Important-7.)
- **`is_paired: bool` field stays on `ExtractState`** (Phase D Reviewer B Low #2 deferred recommendation). Phase E doesn't change this; Phase F may revisit.

### 9.2 Open questions

1. **(Open, low-risk)** SPEC §4.1 example column for Comprehensive says `CpG_{input}.txt`; should say `CpG_context_{input}.txt`. Phase E fixes inside its PR per the §16 convention.
2. **(Open, Phase F concern)** `Box<dyn Write + Send>` adds vtable dispatch; could be replaced with an enum `Either<File, GzEncoder<File>>` for static dispatch. Defer to Phase F profiling — the dyn cost is amortized by BufWriter at Phase E parallel=1.
3. **(Resolved)** `is_paired` stays as field per Phase D Reviewer B Low #2 deferred recommendation.

### 9.3 Critical questions

**None.** All decisions have defaults; nothing changes scope/behaviour such that pausing is mandatory.

## 10. Validation

| What to verify | How | Expected |
|----------------|-----|----------|
| Per-mode file count | `mode_keys_*_has_N_keys` unit tests | 12/3/8/2/1/0 keys per mode. |
| Per-mode filenames | `output_file_map_*_creates_*` tests | Exact filename strings on disk. |
| `_context_` infix in Comprehensive | `mode_keys_comprehensive_has_3_keys` + smoke | `CpG_context_*.txt` etc. |
| `Non_CpG_` prefix in MergeNonCpG | unit + smoke | CHG/CHH calls land in `Non_CpG_*` files. |
| Yacht 8-col row format | `format_yacht_row_has_8_columns` | Exact byte string. |
| Yacht orientation polarity | `yacht_row_orientation_plus_for_forward_class` + reverse mirror | `+` for OT/CTOB, `-` for OB/CTOT. |
| `--mbias_only` skips split files | `output_file_map_skips_eager_open_for_mbias_only` + smoke | Empty output dir except M-bias.txt + report. |
| `--mbias_only` silences InvalidXmByte | `extract_calls_mbias_only_silence_skips_invalid_xm_byte` + smoke | No error; bad byte skipped. |
| `--gzip` produces valid gzip | `output_file_map_gzip_writes_valid_gz_content` + smoke | Round-trip decoded content matches plain mode. |
| `--gzip` adds .gz suffix | smoke | Every output filename ends `.gz`. |
| Phase A-C-D regressions | `cargo test -p bismark-extractor` | All 151 prior tests + new Phase E tests pass. |
| Phase B/C/D unchanged ledger | grep extract_calls / OutputFileMap::new callsites | Signature ripple only; no behaviour change in those paths. |
| Phase-gate intact for F/G | `main_still_rejects_multicore` + `main_still_rejects_bedgraph` | Both still rejected. |
| Clippy + fmt | `cargo clippy -- -D warnings && cargo fmt --check` | Clean. |

## 11. Integration with later phases

| Phase | What Phase E leaves for it |
|-------|----------------------------|
| **F** (multicore) | Phase E doesn't change the per-record cost meaningfully (one extra arg passthrough; one mode-key match). Phase F's producer/consumer split needs to handle: (a) per-worker `OutputFileMap`s that merge at finalize, (b) gzip stream ordering (each worker writes its own .gz chunks; the merge concatenates them — but gzip streams concatenate trivially per RFC 1952 §2.2, so the merge is bytewise). Phase F may also revisit `Box<dyn Write>` → enum for static dispatch. |
| **G** (bedGraph + cytosine_report) | Phase E adds `--gzip`-honoring filenames; Phase G's subprocess chain reads from those filenames (passing `.gz` paths to Perl `bismark2bedGraph` which natively handles `.gz`). |
| **H** (byte-identity gate) | The new modes' byte-identity surface is now in scope. Yacht's 8-col format + Comprehensive's `_context_` infix + MergeNonCpG's filenames all gate at Phase H. |

## 12. Self-review

**Efficiency.** Mode dispatch adds one match per `route_call` invocation; Box<dyn Write> adds one vtable per write. Both are amortized by the BufWriter. Gzip compression is the dominant cost in the `--gzip` path; matches Perl's `gzip -c` subprocess in approximate CPU profile.

**Logic.** The mode-key + filename logic is centralised in `output_mode.rs`. Phase B/C/D code paths see the same flow (eager-open → route_call → write); only the key shape varies. No new error-cleanup paths needed (Phase B's `cleanup_partial_outputs` iterates the map; works for any mode's keys including the empty-set MbiasOnly case).

**Edge cases.** `--mbias_only` empty BAM, `--gzip` empty BAM, yacht record without alignment_start (defensive), mode + mbias_only interaction (only mbias_only effective), `mbias_only_silence` toggle gating only the InvalidXmByte error (not the U/u/`.` skips). All covered.

**Integration.** Three Phase B/C/D files get signature ripples (`extract_calls`, `OutputFileMap::new`, `route_call`'s `write_call` arg list). All test-side updates are mechanical. The plan's Phase B/C/D regression check passes with 151 + new Phase E tests.

**Risks remaining.**

- **R1**: `Box<dyn Write>` performance vs static dispatch. Phase F profiling target.
- **R2**: Gzip footer-on-drop semantic on **clean error paths** — if Phase E's cleanup-on-error path triggers between an `extract_calls` invocation and the next `flush_all`, the `.gz` files might be truncated. `cleanup_all` deletes the files entirely, so the truncation doesn't matter for byte-identity (the file is gone). Verified by smoke test `smoke_gzip_cleanup_on_write_failure_removes_gz_files` (rev 1).
- **R2b** (rev 1): Gzip files on the **panic path** are NOT cleaned up because `cleanup_all` only runs inside `main.rs::run`'s `Result::Err` handler, which is bypassed by unwinding panics. Documented in §4.6 row "panic mid-write"; out of scope for Phase E.
- **R3**: Yacht's `record_end_1based` reads `record.cigar().reference_end(record_start)` for every call in yacht mode — could be cached once per record. Minor; Phase F may collapse.

## 13. Revision history

- **rev 0** (2026-05-27): initial Phase E plan written. Awaiting manual review → dual plan-reviewer pass → implementation trigger.
- **rev 1** (2026-05-27, same day): absorbed dual plan-review findings (`PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md`). Critical fix: yacht reverse-strand col-6/col-7 polarity now mirrors Perl `:4350, 4382, 4422-4447` (forward-class emits `(start, end)`; reverse-class emits `(end, start)`). Importants absorbed: narrowed `mbias_only_silence` catch arm to `InvalidXmByte` only (Reviewer A A2 / Reviewer B I5); centralised `is_mbias_only()` predicate on `ResolvedConfig` (Reviewer B I1); tightened `record_start`/`record_end` snippet with `try_from` + explicit `InternalError` on `None` (Reviewer A G1); documented `mode_keys` Vec ordering as load-bearing for cleanup + Phase H diagnostics (Reviewer B I3); explained `+ Send` bound as forward-looking for Phase F's per-worker map move-at-join (Reviewer B I2/O9); added second SPEC §4.1 fix for the `--comprehensive --merge_non_CpG` row (Reviewer A G5); enumerated all 19 signature-ripple sites by grep (Reviewer A G2/G3); acknowledged panic-path gz cleanup gap (Reviewer B I6); locked `cargo tree` pre-commit verification for `flate2` pin (Reviewer A A3 / Reviewer B I7); added 10 new tests covering Critical-1 regression, mbias_only counter equivalence, MergeNonCpG `{x,X,h,H}` parametric routing, mbias_only `.`/`u` skip-path preservation, PE R2 silence path, gzip + mbias_only smoke, yacht + gzip smoke, empty-BAM yacht smoke, cleanup-on-gzip-failure smoke, and flush/cleanup on empty MbiasOnly map (Reviewers A V1-V5 + B I4/I5/O1).

## 14. Sub-issue (already filed)

[#854](https://github.com/FelixKrueger/Bismark/issues/854).

## 15. Branching strategy

- **Branch:** `extractor-phase-e` (off `extractor-phase-d` while #849/#851/#853 are in review).
- **PR target:** stacked on `extractor-phase-d`. When the upstream chain merges into `rust/iron-chancellor`, rebase Phase E onto fresh iron-chancellor (single `git rebase` after the chain completes).
- **Rebase risk:** Phase E touches `extract_calls` signature + `OutputFileMap::new` signature — if Phase D's review surfaces changes to either, rebase conflicts are likely. Same risk surface as Phases C/D had.

## 16. Follow-up tasks

Per the Phase D convention ("fix SPEC prose errors in the same PR that surfaces them"), Phase E rolls in **two SPEC fixes** (rev 1 — Reviewer A G5 caught a second wrong row):

- **SPEC §4.1 row "Comprehensive"** — example filename `CpG_{input}.txt[.gz]` → `CpG_context_{input}.txt[.gz]` per Perl `:5333` (`s/^/CpG_context_/`).
- **SPEC §4.1 row "Comprehensive + MergeNonCpG"** — example filenames `CpG_{input}.txt[.gz]` / `Non_CpG_{input}.txt[.gz]` → `CpG_context_{input}.txt[.gz]` / `Non_CpG_context_{input}.txt[.gz]` per Perl `:5085` (`s/^/CpG_context_/`) and `:5109` (`s/^/Non_CpG_context_/`).

Note: SPEC §4.1 row "MergeNonCpG" (8-file mode without comprehensive) already correctly omits the `_context_` infix — Perl `:5139` uses `s/^/CpG_OT_/` etc. directly. No fix needed there.

No other follow-up tasks.
