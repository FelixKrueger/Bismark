# PLAN_REVIEW_A — graceful no-alignment sample through extractor + c2c

**Reviewer:** A (independent) · **Date:** 2026-06-14
**Plan:** `plans/06142026_empty-sample-extractor-c2c/PLAN.md`
**Verdict:** APPROVE-WITH-CHANGES (Critical: 2, Important: 4, Optional: 4)

Reviewed against the actual source on `rust/extractor-empty-outputs` @ `~/Github/Bismark-dedup`.
Both crates build clean at baseline (`cargo build -p bismark-extractor -p bismark-coverage2cytosine` → Finished). The core strategy is sound and the fix sites are real, but the plan **mis-describes two mechanics** (lazy-open ⇒ "retain" is really "create"; the "empty" gate is total-calls, NOT the kept-set `usable` check) and **claims an existing-tests-unchanged property that is false** — two existing tests directly assert the behavior being inverted.

---

## Logic

### Extractor side — fix sites are real but the mechanics are mis-stated

- **The skip gate (`downstream_filenames.rs:280-287`) is keyed on the kept-set, not on total calls.** `usable` = (`--CX` → `!kept_split_files.is_empty()`) else (any kept basename starts with `"CpG"`). The plan's Behavior §"A run is 'empty' when zero methylation call strings were processed" is the RIGHT gate, but the plan's Implementation A.2 says "if `--bedGraph` requested AND **zero usable calls**" — *usable-calls* and *total-calls* are **different conditions**. A default-mode (non-`--CX`) run with only CHG/CHH calls has **non-zero total calls but zero usable (CpG) calls** → it hits the same `!usable` skip, yet must NOT get the empty-output treatment (it has real data, just not CpG). The implementation must gate on a **zero-total-calls** signal (the `Total number of methylation call strings processed` counter), distinct from `usable`. See Critical-1.

- **`write_outputs_from_sorted` already handles empty input — the plan is over-cautious.** `rust/bismark-bedgraph/src/output.rs:97-151`: with an empty `sorted` slice the `for` loop just doesn't execute; it still writes `track type=bedGraph\n` to the bedGraph and `finish_gz()`es both streams → a **valid `.bedGraph.gz` (1 header line) + a valid 0-row `.bismark.cov.gz`**. The plan's A.2 caveat "reuse the bedGraph writer's open+finish … do NOT invoke the full sort/aggregate which assumes ≥1 input" is unnecessary: the aggregate path does NOT assume ≥1 input. The simplest correct fix is to **let the existing chain run with the empty `sorted`/`kept`** (skip only the `!usable` early-return on the zero-total-calls path). This also resolves Open Q-1 (the exact bytes are "header line + 0 rows", deterministically, already).

- **"Retain the empty `.txt.gz`" is actually "force-create them" — the files don't exist.** This is the biggest logic gap. Writers are **lazy-opened** (`output.rs:380-386`): a context file is created on its *first* data row. On a zero-call run **no context file is ever created on disk** — every `OutputFileEntry.writer` is `None`. So `output.rs:526-544`'s `remove_file` already hits `NotFound` (no-op); there is nothing to "retain". To satisfy methylseq's `*.txt.gz` glob the implementation must **force-open + force-finish** each context writer (open writes `SPLIT_FILE_HEADER` = `"Bismark methylation extractor version v0.25.1\n"`, `output.rs:43`, then `finish()` seals a valid empty `.gz`). The plan's A.3 ("do not `remove_file` … still `finish()` their gzip trailers") describes finishing a writer that isn't there. Re-frame A.3 as **create**, not retain. See Critical-2.

- **Multicore convergence is a positive — confirm.** The methylseq command always passes `--multicore 4`. `parallel.rs:403` calls `state.finalize(config)?` and `parallel.rs:640` notes "empty input produces zero batches, matching N=1's header-only finalize"; the merged `state` carries the merged `bedgraph_aggregator`. So the single fix in `finalize`/`run_downstream_chain`/`finalize_with_empty_sweep` covers both `--multicore 1` and `--multicore 4` — no separate parallel-path change. Good; the plan should state this explicitly (it currently doesn't mention multicore at all, yet that's the only mode methylseq uses).

### c2c side — the fix is clean; "distinguish read-error from empty" is structurally already true

- **The empty guard is `report.rs:450` (single) / `report.rs:530` (split), fired when `cur_chr == None` after a clean read loop.** `EmptyCoverageInput` is raised *after* the read loop completes with zero parsed data lines — structurally **distinct** from a genuine failure: `File::open` failure surfaces as `io::Error` from `cov::open_cov` (`cov.rs:22-23`), and a corrupt-gz / mid-stream failure surfaces as `io::Error` from `read_until` (`report.rs:427`). So the plan's "distinguish genuine read failure from validly-read-but-empty" requirement is **already satisfied by construction** — the fix is simply to make the `None` arm fall through (write the report via the uncovered pass + `finish()` + summary) instead of `return Err`. `ReportWriter::finish()` (`report.rs:65-73`) explicitly produces a valid empty gzip on a zero-write encoder. Low risk.
- **The methylseq path is `run_single` only** (no `--split_by_chromosome`), `--gzip` → report = `{base}.CX_report.txt.gz` (or `.CpG_report.txt.gz` without `--CX`), both match methylseq's `*report.txt.gz` glob (`report.rs:587-651`); summary = `{base}.cytosine_context_summary.txt` matches `*cytosine_context_summary.txt`. The fix to `run_split:530` is for completeness/parity, not on the methylseq critical path.
- **The all-zero report only materializes at `threshold == 0 && !nome`** (`report.rs:465`). The uncovered-chromosome pass that produces every-cytosine-at-0/0 is gated by exactly that. The methylseq default (threshold 0, non-NOMe) hits it → correct. But see Important-2 (NOMe) and Optional-2 (threshold>0).

---

## Assumptions

1. **Divergence framing — verified correct.** Perl extractor skips bedGraph + deletes empties (matches the existing skip-gate doc-comment `downstream_filenames.rs:236-242`); Perl c2c dies (`error.rs:121-127` documents the Perl analog). Robustness-over-faithfulness, gated on the degenerate condition — consistent with beta.6 dedup.
2. **"Non-empty stays byte-identical" — true ONLY if the gate is zero-total-calls.** If the gate is mis-keyed to `usable` (Critical-1), default-mode non-CpG-only runs change behavior. With the correct gate, byte-identity holds (the existing `usable`/sweep paths are untouched for any run with ≥1 call).
3. **methylseq keys success on exit 0 + glob existence — verified** against `~/Github/methylseq/modules/nf-core/bismark/{methylationextractor,coverage2cytosine}/main.nf`. extractor: `*.bedGraph.gz`, `*.txt.gz`, `*.cov.gz`, `*_splitting_report.txt`, `*.M-bias.txt` all required. c2c: `*report.txt.gz` + `*cytosine_context_summary.txt` required, `*.cov.gz` optional. All satisfiable by the (corrected) fix.
4. **Empty/all-zero acceptable to MultiQC — unverified (the plan admits this).** Reasonable to defer to V-E2E, but see Validation §.

---

## Efficiency

Negligible, as the plan states. Extractor empty path: O(1) gzip open+finish per emitted file (now incl. force-creating the context `.txt.gz` set — still O(#contexts), trivial). c2c empty path: skips the (empty) coverage merge but does the normal O(genome) walk to emit the all-zero report — same as the plan says. No hot-path impact on non-empty runs (gated).

---

## Validation sufficiency

- **V3 is factually wrong as written.** "Non-empty extractor unchanged → existing tests stay green / unchanged" — but two existing tests assert the *current* skip/delete behavior on the path the fix changes:
  - `tests/phase2_inline.rs:819-855` `empty_input_skips_downstream_exit_zero` — a **zero-call BAM** with `--cytosine_report` asserts the skip message AND that `empty.bedGraph.gz` / `empty.bismark.cov.gz` / `empty.CpG_report.txt` **must NOT exist**. The fix inverts exactly this. **This test WILL fail and must be rewritten** (it becomes the V1 positive test). The plan must list this as a required test update, not a "stays green" regression.
  - `tests/phase2_inline.rs:857-881` `default_mode_no_cpg_calls_skips` — CHG/CHH-only, default mode, **non-zero total calls**, asserts skip + no bedGraph. This MUST stay green and is the canary that proves the gate is total-calls (Critical-1). The plan should cite it as the guard that the gate is correctly keyed.
  - `tests/output_phase_c2.rs:84` `empty_file_sweep_emits_perl_format_log_lines_on_stderr` — uses a **one-CpG-record** BAM (≥1 call), asserts 11 contexts swept/deleted. Stays green with the correct (zero-total-calls) gate — good, but it confirms the sweep must remain for any ≥1-call run.
- **V5 (c2c still errors on genuine read failure) is necessary but the "gz-without-.gz" sub-case is mis-modeled.** A `.gz` file fed without a `.gz` extension is read as PLAIN text (`cov.rs:24-34`); the gzip magic bytes then fail `parse_cov_line` (binary → not 6 tab fields, or non-numeric `start`) → `MalformedCovLine` (`cov.rs:56-64`), which is *already* a hard error **independent of** the empty-coverage fix. So V5 via gz-without-`.gz` validates `MalformedCovLine`, NOT the `EmptyCoverageInput`→fall-through boundary. To actually prove the fix doesn't mask corruption, V5 should ALSO cover a **truncated/corrupt gz given WITH `.gz`** (→ `io::Error` from `MultiGzDecoder` during `read_until`) and a **missing file** (→ `io::Error` from `File::open`). All three must remain non-zero exit.
- **V2 only covers `--CX` (non-NOMe).** The plan's own wall-2 trigger is `cytosine_report || nomeseq`, but no validation row exercises empty-coverage under `--nome`. See Important-2 — the NOMe empty path produces a *header-only* report (uncovered pass is `!nome`-gated), materially different from "all-zero genome-wide". If methylseq's nomeseq route can reach c2c with empty coverage, V2 must add a `--nome` row.
- **V-E2E as the only third-wall check is acceptable** given Felix's explicit scope call, BUT the report/summary/MultiQC contracts are cheap to scout statically now (the methylseq fork is checked out at `~/Github/methylseq`). Recommending a quick static scout (Important-4) de-risks the hard gate from discovering a third wall only at full pipeline runtime.
- **Missing: an inline-`--cytosine_report` empty test (Open Q-3).** The plan flags that the failing command uses the **separate** c2c module (no `--cytosine_report` on the extractor), so the inline path is NOT on the methylseq critical path — good to confirm — but the extractor inline c2c feed (`downstream_filenames.rs:332-343`) writes the empty `.cov.gz` then feeds it to in-process c2c, which would hit `EmptyCoverageInput` unless the c2c fix is in place. Since both share the c2c fix, add one inline `--bedGraph --cytosine_report` zero-call test to lock the in-process feed (currently `empty_input_skips_downstream_exit_zero` is the only inline-c2c-on-empty test and it's being inverted).

---

## Alternatives

The Rust-side emit-empty choice (vs methylseq-side `optional: true` on the 3 outputs) is Felix's call; not relitigating. Trade-off worth one line in the plan: emit-empty keeps the container a true drop-in (zero methylseq edits) and makes the binaries robust for any downstream consumer, at the cost of a deliberate, documented Perl divergence in two tools. The methylseq-side alternative would be byte-faithful to Perl but couples the fix to a specific pipeline and re-breaks on the next consumer. Emit-empty is the more durable choice; agreed.

---

## Action items

### Critical
- **C1 — Gate on zero-TOTAL-calls, not on the `usable` kept-set.** `downstream_filenames.rs:280` `usable` is kept-set-based; a default-mode non-CpG-only run has zero *usable* but non-zero *total* calls and must keep the existing skip. Thread the `Total number of methylation call strings processed` counter (it already exists for the splitting report) into `finalize`/`run_downstream_chain` and branch on `total_calls == 0`. The plan's Behavior §1 is right; Implementation A.2 ("zero usable calls") is wrong — fix the wording and the gate. (`downstream_filenames.rs:280-287`, `state.rs:207-228`)
- **C2 — Correct V3 + list the two test changes.** `tests/phase2_inline.rs:819` `empty_input_skips_downstream_exit_zero` **WILL fail** and must be rewritten to assert the new emit-empty behavior (it is effectively the V1 test). Add `tests/phase2_inline.rs:857` `default_mode_no_cpg_calls_skips` as the explicit guard that the gate is total-calls (must stay green unchanged). The plan's "existing tests stay green/unchanged" is false. (plan §C.7 + Validation V3)

### Important
- **I1 — Re-frame extractor A.3 from "retain" to "create".** Lazy-open means context `.txt.gz` files don't exist on a zero-call run (`output.rs:380-386`); the sweep already no-ops via `NotFound`. The fix must **force-open each context writer** (writes the header) **then `finish()`** to materialize valid empty `.gz` files. (`output.rs:521-544`)
- **I2 — Validate (or scope-out in writing) the `--nome` empty-coverage path.** The plan's wall-2 trigger names `nomeseq`, but in `--nome` mode `run_single`'s uncovered pass is skipped (`report.rs:465` `&& !config.nome`), so empty NOMe coverage yields a header-only report + (via `gpc.rs:41`, whose `:39` doc-comment "never sees an empty cov" becomes STALE after the fix) empty GpC report/cov — NOT an all-zero genome report. Either add a V2 `--nome` row or explicitly state NOMe-empty is out of scope and untested. Also update the `gpc.rs:39` doc comment. (`report.rs:465`, `gpc.rs:39`)
- **I3 — Strengthen V5 to cover the real error boundaries.** gz-without-`.gz` exercises `MalformedCovLine` (an *independent* pre-existing error), not the `EmptyCoverageInput` boundary. Add: (a) truncated/corrupt gz WITH `.gz` (→ `io::Error` in `read_until`), (b) missing file (→ `io::Error` in `File::open`). All three must keep non-zero exit so the fix can't mask corruption. (`cov.rs:22-64`, `report.rs:427`)
- **I4 — Statically scout report/summary/MultiQC contracts now** (cheap; the methylseq fork is at `~/Github/methylseq`) rather than discovering a third wall only at V-E2E runtime. Keep V-E2E as the hard gate, but pre-read the `bismark/report`, `bismark/summary`, and MultiQC-bismark module input declarations for any required field that an all-zero/empty sample wouldn't populate.

### Optional
- **O1 — Add an explicit "multicore convergence" note.** State that the fix lives in `finalize`/`run_downstream_chain` and is reached identically under `--multicore N` (`parallel.rs:403,640`), so no parallel-path change is needed — the methylseq command always uses `--multicore 4`.
- **O2 — Document the threshold>0 empty behavior.** At `--coverage_threshold > 0` (e.g. `--gc --coverage_threshold N`) on empty coverage, the uncovered pass is skipped (`report.rs:465`), so the report is header-only, not all-zero. Off the methylseq critical path, but the plan's "produce the genome-wide all-zero report" is only true at threshold 0; note the caveat.
- **O3 — Resolve Open Q-1 with the verified answer.** The empty bedGraph is deterministically `track type=bedGraph\n` + 0 rows; the `.cov.gz` is 0 rows — the existing `write_outputs_from_sorted` already emits exactly this on empty `sorted`. No "decide at implement time" needed.
- **O4 — Resolve Open Q-2 in favor of "create all contexts".** Force-create the full `mode_keys` context set (each with header line) — simplest and closest to "outputs exist"; matches what V1 should assert (≥1 `*.txt.gz`, in practice all of them).

---

## Verdict

**APPROVE-WITH-CHANGES** — Critical: 2, Important: 4.
The strategy is sound, both fix sites are real, the baseline builds, and the c2c "distinguish empty-from-error" requirement is already structurally satisfied. But the plan mis-describes two extractor mechanics — the gate must be **zero-total-calls** (not the kept-set `usable` check; C1) and "retain `.txt.gz`" is really **force-create** under lazy-open (I1) — and its V3 "existing tests stay green" claim is false: `empty_input_skips_downstream_exit_zero` asserts the exact behavior being inverted and must be rewritten (C2). Address C1+C2 before implementing; fold I1–I4 into the plan.
