# PLAN_REVIEW_B — graceful no-alignment sample through extractor + c2c

**Reviewer:** B (independent) · **Date:** 2026-06-14
**Target:** `plans/06142026_empty-sample-extractor-c2c/PLAN.md` (rev 0)
**Verdict:** APPROVE-WITH-CHANGES — **0 Critical, 4 Important, 5 Optional**

Source verified against `~/Github/Bismark-dedup` working tree (extractor + c2c +
the `~/Github/methylseq` fork modules/subworkflow). The plan's strategy is sound
and the two walls are real and correctly diagnosed. No blocker that would only
surface at V-E2E was found, but several line refs are stale and the design is
slightly over-engineered relative to what the existing code already does.

---

## Logic

The two-wall diagnosis is **correct and verified**:

- **Wall 1 (extractor).** Confirmed the skip is at
  `downstream_filenames.rs:280-287` (`!usable → logger.note(skip) → return Ok`).
  The `usable` test (273-279) is `--CX → !kept.is_empty()`, else `any kept
  basename starts with "CpG"`. On a zero-call run the empty sweep
  (`output.rs:479-561`, **not** 505-545 as the plan says — see I-1) removes all
  12 context files, so `finalization.kept` is empty (`state.rs:225`), so `usable`
  is `false`, so the chain skips → no `.bedGraph.gz`/`.cov.gz`, and the context
  files are already deleted. Exactly the 3-of-5-missing failure the plan claims.

- **Wall 2 (c2c).** Confirmed `BismarkC2cError::EmptyCoverageInput` is raised at
  `report.rs:450` (`run_single`) and `report.rs:530` (`run_split`), at the
  `match cur_chr.take() { None => return Err(EmptyCoverageInput) ... }` site —
  i.e. when zero coverage **data** lines were parsed. It fires **before** the
  uncovered-chromosome pass (`report.rs:465-474` / `:535-543`). That uncovered
  pass is *exactly* the all-zero-report machinery the plan wants: it already
  walks `genome.names_sorted()` for chromosomes not in `seen` and emits 0/0 rows
  via `chromosome_report_bytes(..., accumulate_summary=false, ...)`. So the c2c
  fix is genuinely small: replace the `None => Err(EmptyCoverageInput)` arm with
  "fall through to the uncovered pass over the whole genome" — but **only at
  `threshold == 0 && !nome`** (the existing gate on the uncovered pass). See I-2.

**A correctness subtlety the plan misses (Important, I-2):** the uncovered pass
is gated `if config.threshold == 0 && !config.nome`. If the empty-coverage relax
just deletes the error and falls through, then under `--coverage_threshold N>0`
(c2c `--gc` etc.) **or** `--nome-seq` the genome walk is skipped entirely → the
report writer is `finish()`d with **zero bytes written** → a valid-but-empty
`*report.txt.gz` and an empty summary. That is arguably still acceptable for the
pipeline (files exist, exit 0), but it is a *different* output than "all-zero
genome-wide report", and the plan's Behavior §2 ("Produce the genome-wide report
… with every cytosine at 0/0") is only true for the default `threshold==0,
!nome` path. The methylseq c2c module command (verified below) is CpG-only +
`--gzip` with **no** `--CX`/`--coverage_threshold`/`--nome-seq`, so it lands on
the happy path — but the plan should state the threshold/nome behaviour
explicitly rather than imply a full all-zero report in every mode.

**The `--gzip` requirement is real and satisfiable.** `report.rs` writes the
report through `ReportWriter::Gz` when `config.gzip`, and there is already a unit
test (`report_writer_gz_empty_is_valid_stream`, `report.rs:1112-1125`) proving a
zero-write gz encoder produces a valid empty-gzip stream. So an all-zero (or even
zero-byte) `*report.txt.gz` will be a readable gzip — the methylseq `path(
"*report.txt.gz")` glob will match.

---

## Assumptions

1. **"Zero-total-calls signal must be threaded" (outline A.1) is unnecessary.**
   The plan proposes threading a new zero-calls signal to the finalize path. But
   the signal *already exists two ways*: (a) `report.call_strings_processed`
   (the counter behind the "Total number of methylation call strings processed"
   line, `logging.rs:235`), and (b) — more directly — `finalization.kept`/
   `kept_split_files` is empty **iff** zero call rows were written (the same calls
   feed both the split files and the aggregator; the existing `usable` pre-check
   already relies on this exact equivalence, documented at
   `downstream_filenames.rs:238-242`). So the extractor fix needs **no new signal
   plumbing**: it can hang entirely off `!usable` at the existing skip site. This
   is a simplification, not a defect — see Optional O-1. (Caveat: there is a
   theoretical case where calls exist but none are CpG and `--CX` is off — see
   I-3 — where `kept` is non-empty yet bedGraph still has no usable input; that
   is the one place the two signals diverge.)

2. **`write_outputs_from_sorted` on an empty `sorted` slice is already safe.**
   Verified `bismark-bedgraph/src/output.rs:97-151`: it unconditionally writes
   `track type=bedGraph` to the bedGraph stream (line 109), opens the coverage
   stream, then `for (chr, positions) in sorted { ... }` — an empty slice simply
   skips the loop and `finish_gz`es both. **Result: a bedGraph.gz containing just
   the `track` header line, and a truly empty coverage.gz** — both valid gzip,
   no panic, no "≥1 input" assumption. This means the plan's emphasis on "do NOT
   invoke the full sort/aggregate which assumes ≥1 input" (outline A.2) is based
   on a **false premise** — the normal writer already handles empty input
   gracefully. The simplest correct extractor fix is: at the `!usable` site, when
   `--bedGraph`/`--cytosine_report` is requested, **call the existing chain with
   an empty `sorted` + empty `kept`** instead of returning early. This auto-
   resolves Open Q-1 (the bytes are exactly what the normal writer emits on empty
   input). See I-4 / O-2.

3. **methylseq output contracts — verified accurate.** The extractor module
   (`~/Github/methylseq/.../bismark/methylationextractor/main.nf:15-20`) declares
   all 5 outputs required (no `optional: true`): `*.bedGraph.gz`, `*.txt.gz`,
   `*.cov.gz`, `*_splitting_report.txt`, `*.M-bias.txt`. The c2c module
   (`.../coverage2cytosine/main.nf:16-18`) declares `*report.txt.gz` + `*cytosine
   _context_summary.txt` required, `*.cov.gz` optional. **Plan §Context is
   correct.**

4. **The two crates are channel-coupled (the plan under-states this).** Verified
   `subworkflows/nf-core/fastq_align_dedup_bismark/main.nf:78-101`: c2c is fed
   `BISMARK_METHYLATIONEXTRACTOR.out.coverage` (the `.cov.gz`). So the extractor's
   emitted **empty `.cov.gz` is the literal input to the c2c empty path**. The two
   fixes are not independent — they form one pipe. The extractor's empty `.cov.gz`
   must be openable by `cov::open_cov` (it is: `.gz` extension → `MultiGzDecoder`)
   and yield zero parsed lines (it will: empty gz → zero bytes → loop never
   enters). Good — but the V-E2E gate is the only place this seam is exercised
   end-to-end; a cheap integration test feeding the extractor's empty `.cov.gz`
   straight into the standalone c2c would de-risk it (O-3).

5. **Open Q-3 is answerable now, not at implement time.** The methylseq c2c
   module command (verified `coverage2cytosine/main.nf:25-32`) is:
   `coverage2cytosine <cov> --genome <index> --output <prefix> --gzip ${args}`.
   It uses the **standalone** binary (not the extractor's inline `--cytosine_report`
   feed), is **CpG-only** (no `--CX`), and uses `--genome` (the container's c2c
   alias, not `--genome_folder`). So: the inline path is NOT exercised by
   methylseq's failing run; only the standalone binary is. The c2c fix lives in
   `report.rs::run_single`/`run_split`, which **both** the standalone `run()` and
   the inline feed call — so both are covered by one change. The plan should
   resolve Q-3 with this finding rather than defer it.

---

## Efficiency

Negligible, as the plan says, and the simplified design (assumptions 1-2) makes
it even cheaper: the extractor change is a branch at the existing skip site, and
the c2c change is replacing one `match` arm. No hot-path impact on normal runs;
both fixes are gated on the empty condition. **No concern.**

---

## Validation sufficiency

The V1-V6 table is well-targeted. Gaps:

- **V5 (genuine read failure stays red) is the load-bearing regression and the
  current code makes it *subtle* — Important, I-5.** `cov::open_cov`
  (`cov.rs:22-35`) only uses `MultiGzDecoder` when the path ends `.gz`; a
  gz-without-`.gz` file is opened as **plain text**. Reading raw gzip bytes as
  text does NOT cleanly yield "zero data lines" — it yields either a
  `MalformedCovLine` (binary bytes split on `\t` → `parse_u32` fails) or, if the
  gzip body happens to contain no `\n`, a single unparseable "line". So
  "gz-without-`.gz`" mostly trips `MalformedCovLine`, a *different* error than
  `EmptyCoverageInput`. **This is good news for V5**: relaxing only the
  `EmptyCoverageInput → all-zero report` arm leaves `MalformedCovLine` (and raw
  `io::Error` from `open_cov`, e.g. missing file = ENOENT) still fatal. **But the
  plan must be explicit that ONLY `EmptyCoverageInput` is relaxed — not the
  error path generally** — and V5 should test *both* a missing file (ENOENT) and
  a gz-without-`.gz` (MalformedCovLine) to prove the relax is surgical. The
  current plan text ("Distinguish a genuine read failure … Only the former stays
  an error") is right in spirit but the cleanest implementation is "the empty
  case is already a *distinct* error variant; just stop raising it" — no new
  distinguishing logic needed (O-4).

- **Missing regression: `--gc`/`--nome-seq`/`threshold>0` empty behaviour
  (Important, part of I-2).** If the relax falls through unconditionally, a
  `--nome-seq` empty run now reaches `gpc::run_gpc` on empty input. The GpC code
  *documents* it relies on `EmptyCoverageInput` having fired first
  (`gpc.rs:39`). Relaxing the report guard removes that precondition; `gpc`'s own
  `cur_chr.take()` returns `None` → it silently writes no GpC report (its writers
  open lazily per covered chr). Not a crash, but a **silent behaviour change in a
  mode the plan claims is byte-identical**. Add a V-row asserting `--nome-seq` /
  `--gc` empty-cov behaviour, or gate the relax so non-default modes are out of
  scope (recommended: keep the relax confined to the methylseq-exercised
  CpG-only/`--gzip` shape and leave `EmptyCoverageInput` intact for nome/gc/
  threshold>0, documenting that as a follow-up).

- **V-E2E third-wall risk is acceptably bounded (Optional, O-5).** Adversarial
  point #6: `BISMARK_REPORT` (`bismark2report_rs`) and `BISMARK_SUMMARY`
  (`bismark2summary_rs`) are **faithful byte-identical Perl ports** (per the
  project status journal), and Perl methylseq's report/summary already handle a
  header-only / zero-read sample (the splitting report + M-bias are produced with
  zero counts, which Perl's report parser tolerates). So the third wall is
  *unlikely* and deferring it to V-E2E is reasonable. The cheap pre-check worth
  doing: confirm `bismark-report`'s splitting-report parser has no
  divide-by-`sequences_analysed` that panics at 0 — I scanned `bismark-report/src`
  and found no raw division/`unwrap` on report counts, consistent with a faithful
  Perl port. Low risk; V-E2E is sufficient.

- **V2 tests `--CX` but methylseq uses CpG-only `--gzip`.** Keep the `--CX` test
  (good coverage of the all-context walk) but **add a CpG-only `--gzip` test
  matching the real module command** so V2 mirrors production, not just a
  superset (O-5).

---

## Alternatives

- **(Considered, rejected by Felix) methylseq-side `optional: true`.** The plan
  correctly notes this was Felix's call; Rust-side keeps the container a drop-in.
  Agree.
- **(Simplification, recommended) Lean on existing empty-tolerant primitives.**
  As assumptions 1-2 show, both `write_outputs_from_sorted` (empty `sorted` → just
  the header) and the c2c uncovered-pass (already emits all-zero rows) already
  do the heavy lifting. The implementation should be *smaller* than the outline
  suggests: extractor = "on `!usable`, instead of skip, run the chain with empty
  `sorted` + retain the swept files"; c2c = "stop raising `EmptyCoverageInput` on
  the default path; fall through to the existing uncovered pass". This removes
  the proposed new signal-threading (A.1) and the proposed bespoke empty-gzip
  writer (A.2). Net: fewer new code paths = less byte-identity risk.
- **(Sequencing) Retain-then-skip ordering.** Note the empty sweep
  (`output.rs:479-561`) currently *deletes* the files **before**
  `run_downstream_chain` is reached. If the fix retains them in the sweep, but
  the chain is then driven with empty `kept`, the retained context files are NOT
  passed as bedGraph positionals (correct — they're empty), and they survive on
  disk to satisfy `*.txt.gz`. The two sub-fixes (retain in sweep + emit empty
  bedGraph/cov in chain) are **independent** and can be implemented/tested
  separately. Good for incremental verification.

---

## Action items

### Critical
*(none)*

### Important
- **I-1 — Stale line refs (fix before implement).** The empty-delete sweep is at
  `output.rs:479-561` (the `finalize_with_empty_sweep` fn), **not** `505-545` as
  stated in §Context and outline A.3. The kept/swept match arms are at
  `output.rs:514` (kept, `records_written > 0`) and `:526-544` (swept). The
  extractor skip guard at `downstream_filenames.rs:280-287` **is** accurate. The
  c2c guard is `BismarkC2cError::EmptyCoverageInput` raised at `report.rs:450`
  (`run_single`) **and** `report.rs:530` (`run_split`) — the plan must fix **both
  sites** (`report.rs:530` is unmentioned). Defined in `error.rs:121-127`.
- **I-2 — c2c relax interacts with `threshold>0` / `--nome-seq` / `--gc`
  (`report.rs:465`, `:535`, `gpc.rs:39`).** The uncovered all-zero pass is gated
  `threshold == 0 && !nome`. Falling through unconditionally produces an empty
  (not all-zero) report under those modes and reaches `gpc::run_gpc` on empty
  input (whose doc-comment relies on `EmptyCoverageInput` firing first). Either
  scope the relax to the default CpG/`--gzip` path (recommended) or add explicit
  validation for nome/gc/threshold>0 empty behaviour. Plan §Behavior currently
  over-claims "genome-wide all-zero report" for all modes.
- **I-3 — The "zero-total-calls" gate vs the `--CX`/CpG-only `usable` split
  (`downstream_filenames.rs:273-279`).** "Zero methylation calls" and "no usable
  bedGraph input" are NOT identical: under CpG-only bedGraph, a run with calls
  but **no CpG context** has non-empty `kept` yet `usable == false`. Decide which
  condition triggers empty-emission. If the trigger is `!usable` (simplest), then
  a has-calls-but-no-CpG run *also* emits empty bedGraph/cov + retains files —
  which is arguably correct (methylseq still needs the 5 outputs) but is a
  **wider divergence than "zero calls"**. State the exact trigger; do not leave
  it as "zero total calls" in prose while implementing `!usable`.
- **I-4 / Open Q-1 — empty-gzip bytes are already determined.** Don't "decide
  exact bytes at implement time": driving the existing
  `write_outputs_from_sorted` (`bismark-bedgraph/src/output.rs:97`) with an empty
  `sorted` deterministically yields a bedGraph.gz = the single line
  `track type=bedGraph\n` and a 0-row coverage.gz. Reuse that path verbatim
  (matches a normal run's header behaviour) rather than hand-rolling an empty-gz
  writer. Closes Open Q-1.

### Optional
- **O-1 — Drop outline step A.1** (new signal threading); use the existing
  `!usable` branch / `finalization.kept.is_empty()` — the signal already exists
  (`report.call_strings_processed` / the kept-set equivalence documented at
  `downstream_filenames.rs:238-242`).
- **O-2 — Drop outline step A.2's bespoke empty-gz writer**; call the existing
  chain with empty `sorted`/`kept` (assumption 2). Smaller diff, less
  byte-identity risk.
- **O-3 — Add a cross-crate seam test:** feed the extractor's emitted empty
  `.cov.gz` directly into the standalone c2c (the real methylseq pipe) and assert
  exit 0 + the two required outputs. De-risks the channel coupling (assumption 4)
  without a full nextflow run.
- **O-4 — Make the c2c relax surgical:** the cleanest implementation is "stop
  raising `EmptyCoverageInput`" (it is already a distinct variant) — no new
  empty-vs-error distinguishing logic needed. Confirm V5 covers BOTH ENOENT
  (missing file, `open_cov` `io::Error`) AND gz-without-`.gz`
  (`MalformedCovLine`), since those are the two real failure shapes and **both
  remain fatal** after the relax (`cov.rs:22-35`, `:42-64`).
- **O-5 — Mirror the real module command in tests:** add a CpG-only `--gzip`
  c2c V-row (the production shape; `coverage2cytosine/main.nf:25-32`) alongside
  the existing `--CX` test; and note the third-wall (report/summary/MultiQC)
  risk is low because those are faithful Perl ports that already handle
  zero-read samples (no divide-by-count found in `bismark-report/src`).

---

## Verdict

**APPROVE-WITH-CHANGES** — 0 Critical, 4 Important, 5 Optional. The strategy is
correct, both walls are real and accurately located, and nothing was found that
would fail *only* at the expensive V-E2E gate. The required changes are: fix the
stale line refs and add the second c2c site (I-1); scope/validate the c2c relax
against `threshold>0`/`--nome-seq`/`--gc` (I-2); pin the exact empty-emission
trigger vs the `--CX`/CpG-only `usable` split (I-3); and adopt the already-
empty-safe primitives instead of new plumbing (I-4 + O-1/O-2), which both
shrinks the diff and answers Open Q-1/Q-3 now.
