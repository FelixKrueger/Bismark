# PLAN — graceful no-alignment sample through extractor + coverage2cytosine

**Slug:** `06142026_empty-sample-extractor-c2c` · **Crates:** `rust/bismark-extractor` (`bismark_methylation_extractor_rs`) + `rust/bismark-coverage2cytosine` (`coverage2cytosine_rs`)
**Branch / worktree:** `rust/extractor-empty-outputs` @ `~/Github/Bismark-dedup` (off `origin/rust/iron-chancellor` `b97a8e2`)
**Status:** PLAN — awaiting manual review → dual plan-review → explicit implement trigger. **No source edited yet.**

---

## Goal

Make a **no-alignment sample** (a header-only BAM, e.g. nothing aligned) flow cleanly through the
nf-core/methylseq post-dedup chain instead of crashing it. beta.6 fixed dedup (it now emits an empty
header-only BAM + exit 0), but the sample then hits two further walls:

1. **`BISMARK_METHYLATIONEXTRACTOR` fails** — `Missing output file(s) *.bedGraph.gz`. The Rust
   extractor exits 0 but, on zero methylation calls, **skips** the bedGraph/coverage steps and
   **deletes** all empty per-context `.txt.gz` files. methylseq's module declares
   `*.bedGraph.gz`, `*.txt.gz` (methylation_calls), and `*.cov.gz` as **required** outputs → 3 of
   5 are missing.
2. **`BISMARK_COVERAGE2CYTOSINE` would fail next** (it's in the DAG when `cytosine_report || nomeseq`)
   — on an empty `.cov`, the Rust c2c exits 1 with `error: no data found in the coverage file`.

**Fix (Felix-chosen strategy, 2026-06-14): Rust-side — emit empty outputs.** Make the Rust tools
*more robust than Perl* on this degenerate input so the pipeline survives, keeping the container a
self-contained drop-in (no nf-core module edits):
- **Extractor:** on zero total methylation calls, emit an empty `.bedGraph.gz` + empty `.cov.gz` and
  **retain** the empty per-context `.txt.gz` files (instead of skip + delete), exit 0.
- **c2c:** on an empty (but validly-read) coverage file, produce the genome-wide **all-zero** CX
  report + cytosine-context summary and exit 0 (instead of erroring).

### Intentional divergence from Perl — verified

This is a **deliberate divergence from Perl v0.25.1**, the same robustness-over-faithfulness call as
the beta.6 dedup fix. Empirically confirmed on the same tiny fixtures (`$TMPDIR/dedup_repro/`):
| Tool, empty input | Perl v0.25.1 | Rust (current) |
|---|---|---|
| extractor `--bedGraph --CX` | skips bedGraph (`bismark2bedGraph` bails "Please respecify"), deletes empty context files; outputs **splitting_report + M-bias only**; exit 0 | identical file set; warns "skipping the bedGraph/cytosine_report steps"; exit 0 |
| c2c `--CX` on empty `.cov` | **dies** exit 255 "No last chromosome was defined…"; leaves 0-byte report+summary | **dies** exit 1 "no data found in the coverage file" |

So neither Perl nor Rust currently survives a no-alignment sample here — methylseq has never handled
it. The fix makes Rust survive. **Non-empty inputs stay byte-identical to Perl** (the change is gated
on the zero-total-calls / empty-coverage condition only).

---

## Context

### The full cascade (verified locally, Rust vs Perl)
align → header-only BAM → **dedup** (empty BAM + zero report + exit 0; *fixed beta.6*) →
**extractor** (*wall 1*) → **c2c** if `cytosine_report||nomeseq` (*wall 2*) → report → summary →
MultiQC. The plan fixes walls 1 + 2; the end-to-end validation (V-E2E) must surface any further wall
(report/summary/MultiQC are expected to tolerate all-zero/empty inputs but are unverified — Felix
explicitly chose the targeted fix over the full-cascade scout, so we confirm the tail empirically).

### methylseq required-output contracts (from `~/Github/methylseq` fork modules)
- `bismark/methylationextractor`: `*.bedGraph.gz`, `*.txt.gz`, `*.cov.gz`, `*_splitting_report.txt`,
  `*.M-bias.txt` — **all required** (none optional).
- `bismark/coverage2cytosine`: `*report.txt.gz` (**required**, GZIPPED), `*cytosine_context_summary.txt`
  (**required**), `*.cov.gz` (already `optional: true`).
- The methylseq extractor command (from the failing run): `bismark_methylation_extractor <bam>
  --bedGraph --counts --gzip --report -s --CX --multicore 4 --buffer_size 70G`.

### Key source sites
- **Extractor skip guard:** `rust/bismark-extractor/src/downstream_filenames.rs:280-287` — when
  `config.bedgraph` and no usable split files, logs "skipping…" + `return Ok(())`. (The `usable`
  check at 273-279: `--CX` → any kept file; default → any `CpG*` file.)
- **Extractor empty-delete sweep:** `rust/bismark-extractor/src/output.rs:505-545` — opened-but-empty
  / never-opened context files are `remove_file`d ("was empty -> deleted") and recorded in `swept`;
  files with `records_written > 0` are `kept`. On zero calls, ALL are swept.
- **bedGraph/cov filenames:** `derive_bedgraph_filename(input_basename)` (`downstream_filenames.rs`),
  + the in-process `bismark2bedGraph` (`bismark_bedgraph` crate) writes `<base>.bedGraph.gz` +
  `<base>.bismark.cov.gz`.
- **c2c empty guard:** `rust/bismark-coverage2cytosine/src/...` — the "no data found in the coverage
  file" error (the exact site to find at implement time; it fires after the genome is read, when zero
  coverage lines were parsed). Perl analog: "No last chromosome was defined" die.

### Conventions
- Tests build BAM/cov fixtures without samtools where possible (extractor/c2c test patterns).
- Pre-push gate per crate: `cargo test -p <crate>`, `cargo clippy -p <crate> --all-targets -- -D
  warnings`, `cargo fmt -p <crate> -- --check`.
- Build from `~/Github/Bismark-dedup/rust/`; cargo/git-write in the worktree need
  `dangerouslyDisableSandbox`.

---

## Behavior (target)

### Extractor — zero-total-calls mode
A run is "empty" when **zero methylation call strings** were processed (the existing
`Total number of methylation call strings processed: 0`). On that condition, when `--bedGraph` (or
`--cytosine_report`) is requested:
1. **Do not skip** the bedGraph/coverage step. Emit a **valid empty** `<base>.bedGraph.gz` and
   `<base>.bismark.cov.gz` — i.e. a gzip stream with the correct header content but **zero data
   rows** (the bedGraph file may carry its `track`/header line as Perl would for a non-empty run;
   safest is a 0-data-row gzip — decide exact bytes at implement time, see Open Q-1).
2. **Retain** the empty per-context `.txt.gz` files instead of sweep-deleting them (so methylseq's
   `*.txt.gz` glob matches). Only the **zero-total-calls** path changes; a normal run (≥1 call) keeps
   the existing kept/deleted sweep **byte-identical**.
3. Exit 0. The splitting report + M-bias.txt are produced as today.
4. Emit one informational stderr line (e.g. "no methylation calls — writing empty bedGraph/coverage
   outputs and retaining empty per-context files for pipeline compatibility").

### c2c — empty-coverage mode
When the coverage file is read successfully but contains **zero data lines**:
1. **Do not error.** Treat it as zero coverage across the genome.
2. Produce the genome-wide report exactly as the normal path would, with **every cytosine at 0/0
   counts** (the report is genome-driven, so it enumerates all Cs regardless of coverage), honoring
   `--CX` (all contexts) and the same gzip behavior (→ `*report.txt.gz`).
3. Produce the `*cytosine_context_summary.txt` (all-zero counts).
4. Exit 0.

### Edge cases
- Normal (≥1 call / ≥1 coverage line): **unchanged, byte-identical** to Perl.
- `--bedGraph` not requested: extractor unchanged (no bedGraph/cov to emit).
- `--cytosine_report` requested inline in the extractor (vs the separate c2c module): the same
  empty-coverage handling must apply to the inline c2c feed too (the extractor drives c2c in-process
  for `--cytosine_report`).
- Multi-context (`--CX`) vs CpG-only: empty outputs are context-agnostic (no rows either way).

---

## Implementation outline

### A. Extractor (`rust/bismark-extractor`)

> **rev 1 (dual-review):** the TRIGGER is **zero *total* methylation calls** (the
> "Total number of methylation call strings processed: N" counter == 0), NOT the `usable`/kept-set
> check at `downstream_filenames.rs:280`. They diverge: a default-mode (CpG-only bedGraph) run can
> have **zero usable but non-zero total** calls (calls exist, just none in CpG context) — that case
> must KEEP the existing skip (byte-identical). Only a genuinely empty sample (zero total calls)
> takes the new path.

1. **Plumb the zero-total-calls condition** to `finalize` (the splitting-report counter already
   exists). The two new behaviors below fire ONLY when `total_calls == 0`.
2. **bedGraph/cov (in `downstream_filenames.rs`):** on `total_calls == 0` with `--bedGraph`/
   `--cytosine_report`, **let the empty input flow through `bismark_bedgraph::write_outputs_from_sorted`**
   instead of the skip early-return. **rev 1 (A-positive + B-I4): no bespoke empty-gz writer is
   needed** — verified that `write_outputs_from_sorted` on an empty slice already emits a valid
   `<base>.bedGraph.gz` (a `track type=bedGraph` line + 0 data rows) + a 0-row `<base>.bismark.cov.gz`.
   So the change is: skip the sort/aggregate (which assumes ≥1 input) but still call the writer with
   an empty sorted set. Keep the informational stderr line.
3. **methylation_calls `*.txt.gz` (in `output.rs`, the sweep ~`479-561`):** **rev 1 (A-I1):** because
   writers are **lazy-opened** (`output.rs:~380`), on a zero-call run the per-context `.txt.gz` files
   were **never created** — so "retain (don't delete)" is wrong. The fix must **force-create + `finish()`**
   the per-context `.txt.gz` (valid empty gzip streams) on the `total_calls == 0` path, so methylseq's
   `*.txt.gz` glob matches. Normal path (≥1 call) keeps the existing kept/swept behavior byte-identical.
4. Documented deviation comments at both sites pointing to this plan.

### B. coverage2cytosine (`rust/bismark-coverage2cytosine`)
5. The `EmptyCoverageInput` guard fires at **two sites** (`report.rs:~450` AND `~530` — rev 1 B-I1).
   **Distinguish** a genuine read failure (corrupt gzip, missing file, `.gz`-content-without-`.gz`-name,
   I/O error) from a **validly-read-but-empty** coverage file (clean read loop, zero data lines →
   `cur_chr == None`; A confirmed this distinction is already structural). Only the former stays an
   error.
6. **rev 1 (A-I2 + B-I2): SCOPE the relax to the plain report path.** The empty-but-valid coverage may
   fall through to the normal genome-walk all-zero report **only** for the standard path
   (`threshold == 0`, NOT `--nome-seq`, NOT `--gc`). Those modes *rely* on the `EmptyCoverageInput`
   guard (e.g. `report.rs:465 && !config.nome` skips the uncovered pass → a header-only, NOT all-zero,
   report; `--gc` reaches `gpc::run_gpc` which documents it relies on the guard firing first). Leave
   the guard intact for nome/gc/threshold. **This is sufficient for methylseq**, whose c2c module uses
   the standard CpG-only + `--gzip` path (B-bonus, resolves Q-3); nome/gc graceful-empty is out of
   scope (a follow-up if ever needed).
7. Documented deviation comment (divergence from Perl's "No last chromosome was defined" die).

### C. Tests
8. **Extractor (new graceful path):** a zero-call BAM (header-only) run with `--bedGraph --gzip`
   (methylseq's shape) → assert exit 0 AND `*.bedGraph.gz`, `*.cov.gz`, ≥1 `*.txt.gz`,
   `*_splitting_report.txt`, `*.M-bias.txt` all exist; bedGraph/cov decompress to 0 data rows.
9. **Extractor (rev 1, A-C2 — MUST rewrite an existing test):** `tests/phase2_inline.rs:~819`
   `empty_input_skips_downstream_exit_zero` currently asserts the *old* skip/no-files behavior the fix
   inverts → **rewrite it** to assert the new graceful outputs. **`default_mode_no_cpg_calls_skips`
   (~:857) MUST stay green** — the canary that the gate is `total_calls==0`, NOT `!usable` (a
   has-calls-but-no-CpG default-bedGraph run still legitimately skips). Add an explicit
   has-calls/none-CpG → still-skips regression if not already covered.
10. **c2c (new graceful path):** an empty `.cov.gz` + a small genome FASTA, standard path → assert
    exit 0 AND `*report.txt.gz` (all-zero rows, gzipped) + `*cytosine_context_summary.txt`; row count
    == genome cytosine count.
11. **c2c (rev 1, A-I3/4 — strengthen the error regression):** genuine read failures STILL error —
    add (a) a **corrupt** gzip *with* a `.gz` name, (b) a **missing** file (alongside the existing
    malformed-line case). Confirm `--nome-seq`/`--gc`/`threshold>0` on empty coverage are UNCHANGED
    (still guarded — out of scope for graceful-empty).
12. **methylseq conformance (both crates):** add a Tier-3 runtime row asserting the no-alignment
    command shape exits 0 + produces the module-required outputs (extractor: the 5 globs; c2c:
    `*report.txt.gz` + summary).

### D. Docs / release
10. `rust/README.md` Milestones line. On merge (Felix's go): cut **beta.7** (bump `rust/VERSION` +
    the 3 mirror literals → dry-run → publish) + bump the methylseq pin `:2.0.0-beta.6` →
    `:2.0.0-beta.7`.

---

## Efficiency
Negligible. The empty path does O(1) extra work (open+finish empty gzip streams; retain instead of
remove). The c2c empty path skips the coverage-merge but still does the normal genome walk (O(genome),
unchanged). No hot-path impact on normal runs.

## Integration
- **Reads/writes:** extractor adds empty `.bedGraph.gz`/`.cov.gz` + retained empty `.txt.gz`; c2c
  adds all-zero `report.txt.gz` + summary. All satisfy the methylseq module globs.
- **Downstream:** report/summary/MultiQC consume these — **expected** to tolerate all-zero/empty
  inputs; **V-E2E confirms** (any further wall = a follow-up, surfaced not assumed).
- **Non-empty byte-identity:** unchanged — the change is gated on zero-total-calls / empty-coverage.
  perl-oracle CI + existing extractor/c2c byte-identity tests stay green.
- **Two crates, possibly two bismark-io/bedgraph touchpoints** — keep changes additive; no version
  bumps mid-beta (per the no-bump convention).

## Assumptions
1. Felix-chosen: Rust-side emit-empty (more robust than Perl), not methylseq-side optional outputs.
2. Both fixes are deliberate divergences from Perl (verified: Perl extractor skips, Perl c2c dies);
   non-empty stays byte-identical.
3. methylseq keys success on exit 0 + the declared output globs existing; empty (0-row) gzip files
   and all-zero reports satisfy them.
4. The c2c all-zero report is acceptable to MultiQC / downstream (genome-driven, standard format).
5. report/summary/MultiQC tolerate the all-zero/empty case (to be confirmed by V-E2E, not assumed).

## Validation
| # | Verify | How | Expected |
|---|---|---|---|
| V1 | Extractor emits required outputs on empty | header-only BAM, `--bedGraph --CX --gzip` | exit 0; `*.bedGraph.gz`+`*.cov.gz`+≥1 `*.txt.gz`+splitting+M-bias all present; bedGraph/cov = 0 data rows |
| V2 | c2c graceful on empty `.cov` | empty `.cov.gz` + small genome, `--CX` | exit 0; `*report.txt.gz` (all-zero, gzipped) + `*cytosine_context_summary.txt`; rows == genome C count |
| V3 | Non-empty extractor unchanged (rev 1, A-C2) | `cargo test -p bismark-extractor` (full) — NOTE `phase2_inline.rs:~819` is **rewritten** (asserted the old skip), `default_mode_no_cpg_calls_skips:~857` **stays green** | green; the rewritten test asserts graceful outputs; the canary proves the gate is `total_calls==0` not `!usable` |
| V3b | has-calls-but-no-CpG default bedGraph still SKIPS | a BAM with non-CpG calls only, default `--bedGraph` (no `--CX`) | exit 0; **no** bedGraph/cov (legitimate skip preserved — the trigger boundary) |
| V4 | Non-empty c2c unchanged | `cargo test -p bismark-coverage2cytosine` (full, incl. byte-identity) | green |
| V5 | c2c still errors on genuine read failure (rev 1, A-I3/4) | (a) corrupt gzip *with* `.gz` name, (b) missing file, (c) malformed line | non-zero exit each (empty-but-valid is the ONLY new graceful case) |
| V5b | c2c nome/gc/threshold unchanged on empty (rev 1) | empty `.cov` + `--nome-seq` / `--gc` / `--threshold>0` | still guarded (unchanged) — graceful-empty is standard-path-only |
| V6 | Lint/fmt both crates | clippy `-D warnings` + `cargo fmt --check` | clean |
| V6b | scout report/summary/MultiQC contracts (rev 1, A-I4) | statically read the `bismark/report`, `bismark/summary` module output globs + MultiQC bismark module on all-zero inputs | identify any 3rd wall BEFORE V-E2E (cheap; reduces surprise beta.8) |
| **V-E2E** | **methylseq survives a no-alignment sample** (HARD gate) | real methylseq on the beta.7 image with the failing sample (Felix's Seqera env) | extractor + c2c + report + summary + MultiQC all complete; **surfaces any further wall** |

## Questions or ambiguities
- **(Open Q-1)** Exact bytes of the empty `.bedGraph.gz` (header `track` line + 0 rows, vs a
  0-row-no-header gzip) and `.bismark.cov.gz` (always 0 rows). Pick whatever the normal writer emits
  with an empty input set; confirm methylseq/MultiQC accept it (V-E2E). *Assumption: 0-data-row valid
  gzip; bedGraph header line included if the normal writer emits one.*
- **(Resolved — rev 1, Q-1)** Empty `.bedGraph.gz`/`.cov.gz` bytes: **no decision needed** —
  `bismark_bedgraph::write_outputs_from_sorted` on an empty slice already emits a `track type=bedGraph`
  line + 0 data rows (bedGraph) and a 0-row `.bismark.cov.gz`. Reuse it; no bespoke writer (B-I4).
- **(Resolved — rev 1, Q-2 → force-create, not retain)** Because writers are lazy-opened, the empty
  per-context `.txt.gz` don't exist on a zero-call run → the fix **force-creates + finishes** them
  (valid empty gzip). Keep all per-context files (simplest, satisfies the `*.txt.gz` glob); revisit
  only if MultiQC mis-parses an empty calls file (covered by V-E2E).
- **(Resolved — rev 1, Q-3, B-bonus)** methylseq uses the **standalone `coverage2cytosine`** module
  (CpG-only + `--gzip`), NOT the extractor's inline `--cytosine_report`. The c2c fix targets the
  standalone binary's standard path — exactly methylseq's path. (The inline feed shares the same code;
  covered, but not the methylseq trigger.)
- **(Open Q-4 / Critical-ish, partially addressed rev 1)** Are report/summary/MultiQC further walls?
  Now **statically scouted in V6b** (cheap, before the expensive V-E2E) AND confirmed end-to-end by
  V-E2E. If a wall appears it's a documented follow-up (beta.8), surfaced not silently ignored.

## Self-Review
- **Logic:** both fixes gated on the empty condition; normal paths untouched → byte-identity preserved.
  c2c must distinguish empty-but-valid from genuine-read-error (V5 guards the regression).
- **Edge cases:** `--bedGraph` absent, `--CX` vs CpG-only, inline-c2c vs module-c2c — all enumerated.
- **Integration:** the real risk is a *third* wall (report/summary/MultiQC) — explicitly made a hard
  end-to-end gate (V-E2E) rather than assumed away.
- **Divergence:** documented + verified against the Perl oracle for both tools; consistent with the
  beta.6 dedup philosophy.
- **Remaining risk:** empty-gzip/all-zero-report acceptability to MultiQC (Open Q-1/Q-4 → V-E2E).

## Implementation Notes (2026-06-14, rev 1 implemented)

**Implemented on `rust/extractor-empty-outputs`. All gates green** (verified directly): extractor
+ c2c `cargo test` all `ok` / 0 failed; clippy `-D warnings` clean both crates; `cargo fmt --check`
clean both crates.

**Production changes:**
- `bismark-extractor/src/state.rs::finalize` — `is_empty_run = self.report.calls_total == 0`; sweep
  called with `force_create_empty = is_empty_run && config.bedgraph`; `run_downstream_chain` gains
  `is_empty_run`. (Confirmed `parallel.rs:403` + `pipeline.rs:178/284` all route through this one
  `finalize` → covers methylseq's `--multicore 4`.)
- `bismark-extractor/src/output.rs::finalize_with_empty_sweep` — new `force_create_empty` param; on
  the empty arm, force-creates a valid empty (gzipped iff `self.gzip`) per-context file via
  `open_split_writer(&path, gzip).finish()` and KEEPs it, instead of deleting. Non-force path
  unchanged; test caller updated to `false`.
- `bismark-extractor/src/downstream_filenames.rs::run_downstream_chain` — skip guard
  `if !usable` → `if !usable && !is_empty_run` (a has-calls/no-CpG default-bedGraph run still skips;
  only a truly empty run falls through to `write_outputs_from_sorted` with empty `sorted`).
- `bismark-coverage2cytosine/src/report.rs` — `run_single` + `run_split`: the post-loop `None`
  (empty-but-valid coverage) arm errors ONLY off the standard path; on the standard path
  (`threshold==0 && !nome && !gc_context`) it falls through to the uncovered-chromosome pass →
  genome-wide all-zero report. `run_split` `last_summary_path` → `Option<PathBuf>`.

**Deviations (both sound):** (1) the c2c standard-path gate adds `&& !config.gc_context` — the plan's
rev-1 A-I2/B-I2 explicitly said to exclude `--gc` (gpc relies on the guard), so this fulfills the
intent (test `empty_coverage_gc_still_errors`). (2) A SECOND inverted test
(`phase3a_streaming.rs::empty_input_skips_downstream_exit_zero`, the `--cytosine_report` inline copy)
also had to be rewritten alongside the named `phase2_inline.rs` one.

**🎯 Full local cascade VERIFIED (the real proof, beyond unit tests):** dedup'd empty header-only BAM
→ extractor (`--bedGraph --counts --gzip --report -s --CX --multicore 2`) → **exit 0 + all 5
methylseq-required outputs** (12 empty `*.txt.gz`, `bedGraph.gz` = track+0 rows, `cov.gz` = 0 rows,
splitting report, M-bias) → c2c (`--genome_folder … --gzip`) → **exit 0 + `CpG_report.txt.gz`
(27 all-zero rows = every genome CpG cytosine) + `cytosine_context_summary.txt`**. Both walls cleared.

**Still outstanding before "done":** V6b (static scout of report/summary/MultiQC contracts) + V-E2E
(Felix's real methylseq run on the beta.7 image — the only thing that proves there's no 3rd wall).
Dual code-review + plan-manager pending.

## Revision History
- **rev 0 (2026-06-14):** initial plan. Both walls + Perl-oracle behavior established empirically
  (local samtools/perl/cargo on tiny fixtures). Strategy (Rust-side emit-empty) chosen by Felix.
- **rev 1 (2026-06-14):** folded dual plan-review (A: APPROVE-WITH-CHANGES 2C/4I; B: APPROVE-WITH-CHANGES
  0C/4I/5O; no contradictions). Changes: **(1)** trigger corrected to **zero TOTAL calls**, not
  `!usable` (a has-calls/no-CpG default-bedGraph run must keep skipping — A-C1/B-I3; new V3b canary);
  **(2)** dropped the bespoke empty-gz writer + signal-threading — `write_outputs_from_sorted` already
  emits valid empty bedGraph/cov (A-positive/B-I4; resolves Q-1, smaller diff); **(3)** `.txt.gz` must
  be **force-created+finished** (lazy-open → they never exist to "retain" — A-I1; resolves Q-2);
  **(4)** c2c relax **scoped to the standard path** (nome/gc/threshold rely on the guard — A-I2/B-I2;
  V5b); two guard sites `report.rs:~450`+`~530` (B-I1); **(5)** V3 corrected — `phase2_inline.rs:~819`
  `empty_input_skips_downstream_exit_zero` MUST be rewritten, `default_mode_no_cpg_calls_skips` is the
  canary (A-C2); strengthened V5 (corrupt-gz+missing-file) + added V6b (scout report/summary/MultiQC
  now). Q-3 resolved (methylseq uses the standalone c2c, CpG-only `--gzip` — B-bonus). Reviews:
  `PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`.
