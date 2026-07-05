# Plan Review B — extractor console progress/diagnostics logging

Reviewer: B (independent, fresh context)
Plan: `CONSOLE_LOGGING_PLAN.md`
Code grounded against: `cli.rs`, `main.rs`, `route.rs`, `parallel.rs`, `pipeline.rs`,
`output.rs`, `state.rs`, `lib.rs`, `subprocess.rs`, `bismark-io/src/read.rs`.

Verdict: **sound goal, lands cleanly off the beta.1 byte-identity path, but the plan
is under-specified in three load-bearing spots** — the progress-counter location, the
"@PG/@HD provenance" API (which does NOT exist as the plan assumes), and the
quiet-gate classification of existing `eprintln!` sites. None are fatal; all are
fixable before implementation. Details below.

---

## 1. Logic

### 1a. Progress-counter location — the plan's "dual-dispatch" framing is WRONG and will mislead the implementer (Critical)

The plan (§Implementation outline step 3) says the read loop "exists in BOTH
`route.rs` (the `--parallel 1` path) AND `parallel.rs` (the `--parallel N` worker
path)" and that the counter "MUST be added to both".

That is not what the code does. `route.rs` has **no read loop** — it's a per-call
router (`route_call`). The two record-iterating loops are:

- `pipeline.rs::extract_se` / `extract_pe` — the **legacy single-threaded** path.
- `parallel.rs::producer_loop` (reads records) + `worker_loop` (processes) — the
  **parallel** path.

Crucially, **`main.rs::run` always dispatches to `extract_se_parallel` /
`extract_pe_parallel`** (main.rs:91-111). The legacy `extract_se`/`extract_pe` in
`pipeline.rs` are only reachable from the test suite — lib.rs keeps them as the
"byte-identity reference". So:

- A user **never** hits `pipeline.rs`'s loop at any `--parallel` value, including
  `--parallel 1` (n_workers = parallel.max(1) = 1, still the parallel pipeline).
- Adding the counter to `pipeline.rs` would make it appear **in tests only**, never
  in the real binary — the opposite of the plan's intent.
- The counter belongs in the **parallel pipeline**, and the natural single
  serialization point is the **producer** (`producer_loop`), which sees every record
  exactly once, in input order, on one thread. That eliminates the atomic entirely
  (see §3 efficiency) and gives Perl-faithful monotonic cadence.

The plan's invocation of `[[feedback_dual_driver_back_port]]` is a good instinct but
misapplied: the #876/#879 fixes touched both because both are *live extraction logic*.
The progress counter is *pure stderr side-effect* and has exactly one production home.

Recommendation: rewrite step 3 to specify the **producer thread** as the counter site.
If the team wants the legacy path to log too (for parity when tests eyeball output),
say so explicitly — but flag it as cosmetic/test-only, not a correctness mirror.

### 1b. "Processed lines: N" — semantics and wording mismatch vs Perl (Critical for user trust)

(Reviewer-A-may-underweight item #1.) The Rust counters already encode Perl's exact
"lines" semantics, and the plan should *reuse them* rather than invent a new tally:

- `SplittingReport.records_processed` — SE: 1/record; PE: **1/pair** (output.rs:399-409,
  matches Perl `sequences_count` :2459, drives "Processed N lines in total").
- `SplittingReport.call_strings_processed` — SE: 1/record; PE: **2/pair** (Perl :2451).

Perl's `Processed lines: N` progress counter (line 1553/1850) counts **input SAM
lines**, i.e. for PE that is **2 per pair** (R1 line + R2 line), matching
`call_strings_processed`, NOT `records_processed`. The plan's Assumptions section
(line 102-104) is ambiguous: it says "PE: Perl counts 2 call strings/pair — match
Perl's 'lines' semantics" but then "exactness is NOT required". For a counter the
whole *point* of which is "does this match the Perl run I did yesterday", **exactness
in the per-tick semantics is the entire value**. If the Rust counter ticks per pair
but Perl ticks per SAM line, a 55.7M-pair PE run shows `~55.7M` in Rust vs `~111.4M`
in Perl — a 2× discrepancy that will generate "is the Rust port dropping half my
reads?" support tickets.

But there is a subtlety the plan misses: in the parallel producer, records are read
one-at-a-time (SE) or two-at-a-time (PE pair). The producer can tick:
- SE: +1 per record → matches Perl SAM-line count. Correct.
- PE: +2 per pair (R1+R2) → matches Perl SAM-line count. Correct.

So the producer naturally has access to the right granularity. Recommendation:
**define the counter as "input SAM records read" (+1 SE record, +2 PE pair) and label
it `Processed lines: N` to match Perl byte-for-byte.** Document this in the plan so
nobody "simplifies" it to per-pair later.

### 1c. Final summary — fine, but watch the `--mbias_off` and `--mbias_only` interaction

The final per-context summary draws from `self.report` (the `SplittingReport`),
which is populated **unconditionally** in `route_call` / `increment_counters` (route.rs:99,
parallel.rs:792) — even under `--mbias_only` (counters bump before the short-circuit).
So the summary is available in all modes. Good. The plan's edge-case note (line 112)
is correct that it must "guard against referencing files that mode didn't produce" —
but the *numbers* are always valid. The only real guard: under `--mbias_off` the
M-bias tables are empty but the splitting-report counts are still live, so the
summary should mirror `_splitting_report.txt` (which is itself still written under
`--mbias_off`), not the M-bias data. Plan should state the summary mirrors the
**splitting-report** figures specifically.

---

## 2. Assumptions

### 2a. "@PG/@HD provenance pulled structurally from the noodles-parsed header" — the assumed API does not exist as described (Important)

(Reviewer-A-may-underweight item #4.) The plan (§Behavior item 4, §Implementation
step 2) asserts the port "reads the header structurally via noodles" and can "iterate
the noodles `Header` program (`@PG`) records + the `@HD` line; skip the reference
(`@SQ`) map" as "free structural access — no re-read."

Reality check against `bismark-io/src/read.rs`:

- The only header-introspection helper is `detect_paired_from_header(header: &Header)
  -> Option<bool>` (read.rs:649). It does NOT expose `@PG`/`@HD`; it **serializes the
  whole header to SAM text** via `noodles_sam::io::Writer` and string-searches for the
  Bismark `@PG` line. The deliberate design comment (read.rs:650-653) says this is
  "robust to noodles API shape changes ... the SAM text format is the stable contract
  here, NOT the in-memory `Programs` type."
- In `main.rs::run`, the AutoDetect probe gets `probe.header()` and passes it to
  `detect_paired_from_header`, then **drops the reader** (main.rs:97-107) before
  `extract_*_parallel` re-opens the file. So the header the plan wants to print is
  *not* retained past detection — and for the `-s`/`-p` explicit paths the header is
  never inspected on the main thread at all (it's read inside `run_pipeline`,
  parallel.rs:186, used only for `chr_table`).

Consequences the plan must resolve:

1. **There is no `Header::programs()` iteration wired up today.** The implementer
   would need to either (a) add a new bismark-io helper that returns the `@PG`/`@HD`
   lines as text, or (b) reach into noodles' `Header::programs()` /
   `Header::header()` typed API directly in the extractor. Option (a) is consistent
   with the existing "SAM text is the contract" design; option (b) couples the
   extractor to noodles' in-memory types the project deliberately avoided. The plan
   picks neither — it just assumes the access is "free". It is not free; it's a small
   new API surface. Estimate it.

2. **Faithful `@PG ... CL:"..."` reproduction + multi-`@PG` ordering** (Reviewer item
   #4): if provenance is emitted by **re-serializing** the noodles header (the path
   `detect_paired_from_header` already uses), noodles preserves `@PG` records in
   header order and round-trips `CL:` verbatim *as long as it parsed them into
   `other_fields`*. The risk is real but bounded: noodles' SAM header writer emits
   `@PG` tags in a defined order (ID, PN, CL, PP, ...) which may **not** match the
   byte order samtools wrote (samtools commonly emits `ID PN VN CL` or `ID PN CL PP`).
   So the printed line can be a *semantically faithful but byte-reordered* version of
   the original. For a **diagnostic** (not a byte-compared output file) that is
   acceptable — but the plan should drop the word "faithfully reproduce" and instead
   say "emit the parsed @PG fields (ID/PN/VN/CL/PP); tag order may differ from the
   on-disk byte order." If the team wants the *exact* original bytes, the only safe
   route is the text-serialization path filtered to `@PG`/`@HD` lines — recommend
   that, since the helper already exists and it sidesteps the reordering question
   entirely. **This is the cleanest fix: filter the existing serialize-to-text output
   to lines starting `@PG`/`@HD` and print them as-is.** That reuses `detect_*`'s
   proven approach and gives byte-faithful provenance for free.

3. **Where to print it**: the header is available inside `run_pipeline` (parallel.rs:186)
   before workers spawn — that is the correct, single emit point for banner + params +
   mode + provenance. The plan's step 2 hedges ("Likely in `pipeline.rs` ... or wherever
   cli.rs:467-era resolution lands"). Pin it to **`parallel.rs::run_pipeline`, right
   after `open_reader` + `build_chr_name_table`, before the worker spawn loop**. That
   is also where `is_paired` is already known (passed in), so mode-detection wording is
   trivially available.

### 2b. "Detected from @PG" wording is only true on the AutoDetect path (Important)

Plan §Behavior item 2 proposes `(auto-detected from @PG)` vs `(forced via -s/-p)`. But
note `run_pipeline` receives only the resolved `is_paired: bool` and `config.paired_mode`.
The mode-source (explicit vs auto) **is** recoverable from `config.paired_mode`
(`SingleEnd`/`PairedEnd` = forced; `AutoDetect` = auto). Good — the data exists. Just
make sure the message is derived from `config.paired_mode`, not from `is_paired` (which
has lost the provenance). Minor, but a naive implementation that only has `is_paired`
will print the wrong suffix. Call this out in the plan.

### 2c. Banner version source

The banner (item 1) wants `<crate ver>`. `version_string()` (lib.rs:96) already exists
and reads `CARGO_PKG_VERSION`. Note there are **two** version strings in play:
`output.rs::BISMARK_VERSION = "v0.25.1"` (the Perl-compat string baked into output
files) and `CARGO_PKG_VERSION` (the Rust crate version). The banner should use the
**crate** version (it's a Rust-tool banner, "(Rust)"), but the plan should say which,
because a reviewer/implementer could reasonably reach for `BISMARK_VERSION` to "match
Perl". Specify: banner = crate version; output-file headers = `BISMARK_VERSION`
(unchanged).

---

## 3. Efficiency

### 3a. Drop the AtomicU64 — the producer is already a single thread (Important simplification)

Plan step 3 proposes "a shared `AtomicU64`; print on crossing each 500k boundary" and
flags "aggregate-atomic vs per-worker" as a review decision. Per §1a, the counter
belongs in the **producer**, which is single-threaded (parallel.rs:315 `producer_loop`).
A single thread needs **no atomic** — a plain `u64` local with `if n % 500_000 == 0`
(or boundary-crossing check) is correct, cheaper, and gives strictly monotonic,
deterministic output identical at every `--parallel N`. This also sidesteps the
"garbled interleaving" edge case (plan line 110) entirely — there is nothing to
interleave. Recommend: **plain local counter in `producer_loop`; no atomic, no shared
state.** This is simpler than either option the plan offered.

One caveat: the producer counts records *read*, which for an error-aborting run may
slightly lead the collector's *written* count. That's fine and matches Perl, whose
progress counter also ticks at read time, ahead of downstream processing.

### 3b. Per-line `Logger` quiet check — negligible

Gating each `eprintln!` behind `if !quiet` is free relative to the I/O. The progress
line fires every 500k records; the rest fire O(1) times. No concern. The `Logger`
struct (plan step 1) is fine, though for ~6 call sites a single `quiet: bool` threaded
into the two emit points (run_pipeline startup block + producer + finalize) may be
simpler than a new type. Either is acceptable; not worth a blocking note.

---

## 4. Quiet-gate completeness (Reviewer item #3)

I audited every `eprintln!` in the extractor (`grep -rn eprintln! src/`). Classification
of each as **info (gate under --quiet)** vs **warning/error (NEVER gate)**:

| Site | Text | Class | Plan handles it? |
|---|---|---|---|
| `output.rs:319` | `{path} was empty -> deleted` | INFO → gate | Yes (step 5) |
| `output.rs:322` | `{path} contains data -> kept` | INFO → gate | Yes (step 5) |
| `output.rs:330-331` | two blank lines (Perl :625) | INFO → gate (travels with kept/deleted) | Implied; **make explicit** |
| `output.rs:317` | `warning: failed to remove empty output file` | WARNING → never gate | Yes (edge case line 109) |
| `output.rs:369` | `warning: failed to remove partial output file` | WARNING → never gate | **NOT mentioned** — only the *empty*-file warning is called out |
| `main.rs:44` | `error: {e}` | ERROR → never gate | Yes (line 57) |
| `subprocess.rs:411` | `[bismark-extractor] spawning: …` | INFO (Phase G) | Plan says Phase G "out of scope" — but this is always-on info that `--quiet` users will reasonably expect silenced |
| `subprocess.rs:519` | `note: …cytosine_report will scan…` | WARNING-ish (Phase G) | Out of scope per plan |

Findings:

1. **`output.rs:369` (cleanup_all partial-file warning) is missed.** The plan's edge
   case (line 109) names only "the `failed to remove` warning" singular and cites the
   *empty*-file one. There are **two** such warnings. Both are genuine warnings and
   must NOT be gated. The plan's `Logger` must clearly express "these two lines bypass
   the gate" — confirm both. (Low risk of accidental gating since they're worded
   `warning:` and a careful implementer won't route them through the info gate, but
   the plan should enumerate both so it's not left to chance.)

2. **The two trailing blank lines (output.rs:330-331)** are part of the kept/deleted
   *block*. If kept/deleted are gated but the blanks are not, `--quiet` output gets two
   stray blank lines. Plan should state the blanks gate **together with** the
   kept/deleted lines.

3. **`subprocess.rs:411` "spawning:" is always-on info today.** The plan declares
   Phase G "out of scope" (line 132), which is defensible — but a `--quiet` user
   running `--bedGraph` will still see the `[bismark-extractor] spawning:` line and the
   sub-tool's inherited stderr. That's arguably a leak of the `--quiet` contract.
   Recommend the plan **explicitly acknowledge** that `--quiet` does not silence Phase G
   subprocess chatter (it's the child tools' own output + the pre-spawn audit line),
   and either (a) gate the `:411` audit line too for consistency, or (b) document the
   limitation. Don't leave it silently inconsistent.

The split the plan proposes (single `Logger` for info; warnings/errors use bare
`eprintln!`) does cleanly express the info-vs-warning divide — provided the
implementer routes only the four INFO lines through it and leaves the three
WARNING/ERROR lines on bare `eprintln!`. The risk is purely one of omission (missing
:369), not of structural inability.

---

## 5. Validation sufficiency

### 5a. Byte-identity guard — adequate in principle, but the smoke run is the weakest link (Important)

(Reviewer item #5.) The claim that console logging cannot affect byte-identical output
files is **correct and well-grounded**: all new output is `eprintln!` (stderr); the
output *files* are written by `OutputFileMap`/`write_splitting_report`/`write_mbias_txt`,
none of which the plan touches. The Phase H harness compares files (`cmp`/sorted-md5)
and only `tail -3`s the console. So the invariant holds **by construction**.

However:

- The plan's step-2 provenance work touches the **header read path** in
  `run_pipeline` (parallel.rs). If the implementer adds, say, a header re-serialization
  or a new bismark-io helper, there's a non-zero chance of perturbing the existing
  `build_chr_name_table` borrow or the `detect_*` path. The byte-identity guard
  (`phase_h_smoke.sh` on 1 SE + 1 PE cell) catches *output* divergence but would NOT
  catch a header-read regression that, e.g., changes SE/PE auto-detection. **Add a
  targeted assertion** that auto-detection still resolves correctly on an unflagged
  BAM (or rely on the existing `detect_paired_from_header` unit tests + run one
  AutoDetect cell in the smoke). The plan should name AutoDetect explicitly in the
  verification matrix.

- "One SE + one PE cell" smoke vs **full matrix**: for *output byte-identity*, one
  cell each is adequate because the change is structurally orthogonal to file writes —
  I agree with the plan that the full matrix is overkill here. BUT the *console* output
  itself is new and unverified by any automated check (it's eyeballed, per step 1).
  Recommend at least **one assertion-based test** that does not require colossal: a
  Rust integration test asserting (a) `--quiet` produces empty informational stderr
  while a forced error still prints, and (b) the banner/mode/params lines appear on a
  tiny synthetic BAM. The plan leans entirely on manual eyeballing on colossal (steps
  1-2), which won't regression-guard future refactors. This is the biggest validation
  gap.

### 5b. Progress-cadence test

Because the counter now lives in the producer (per §1a), a unit/integration test can
feed a synthetic ≥500k-record SAM and assert the `Processed lines:` line count. Without
this, the dual-dispatch verification the plan proposes (step 5: "grep both route.rs and
parallel.rs") is checking the *wrong* files (route.rs has no loop). Replace that
verification step with: "assert the counter is in `producer_loop` and fires at the
right cadence on a synthetic input."

### 5c. clap collision check (Reviewer item #2) — CLEARED

Audited `cli.rs`: `-V`/`--version` is taken (line 219); `disable_version_flag = true`
(line 29) frees clap's auto `-V`. clap's auto `-h`/`--help` is present (no
`disable_help_flag`). **`-v` is FREE**; `--verbose` and `--quiet` longs are free; `-q`
is free. No collision. The plan's `-q`/`--quiet` + `--verbose` (the plan does not
assign `-v`; if it wants a short for verbose, `-v` is available). Recommend the plan
**not** add `-v` unless asked — `--verbose` long-only avoids any future `-v`-means-
version confusion (Perl convention) and is the safer choice. Add a `clap_definition_is_valid`
coverage note: the existing `Cli::command().debug_assert()` test (cli.rs:536) will catch
any duplicate-short-flag panic at test time, so a collision can't ship silently.

---

## 6. Alternatives — default-on vs opt-in (Reviewer item #6)

**Argument for default-OFF (opt-in `--verbose`-style):** the tool may run in pipelines
(Nextflow/Snakemake) where stderr is captured into per-task logs; chatter inflates logs
and a 500k-cadence progress line on a 55.7M-pair run emits ~110 lines that bloat CI/log
storage. Many modern bioinformatics CLIs default to quiet and gate verbosity behind
`-v`.

**Argument for default-ON (the plan's choice):** (1) **Perl is default-on** — the entire
motivation (Felix's 2026-05-29 note) is that silence is indistinguishable from a hang
and silent SE/PE auto-detection bites users. Matching Perl's default is the
least-surprise path for users migrating from the Perl tool. (2) The volume is modest:
banner + params + provenance + final summary is O(20 lines); the only repeated line is
the 500k-cadence progress, which at one line per 500k records is **~110 lines for a
55.7M-pair run** — trivial. (3) stdout stays clean (plan is explicit), so piping is
unaffected; only stderr carries the log, which is the conventional channel for progress.

**My recommendation: keep default-ON, matching Perl + the stated motivation, BUT:**
- The progress line is the only high-volume item. Consider making **only the progress
  cadence** suppressible independently (it's already covered by `--quiet`, which is
  enough). 110 lines is not worth a third flag.
- Ensure `--quiet` is genuinely complete (see §4) so pipeline authors who *do* want
  silence have a clean single switch. This is the real mitigation for the opt-in
  argument — a fully-working `--quiet` makes default-on harmless.

So: plan's default-on is the right call given Perl-parity is the explicit goal;
the opt-in concern is fully addressed by a complete `--quiet`, which §4 must nail.

---

## Action items (prioritized)

### Critical (fix before implementation)
- **C1.** Rewrite Implementation step 3: the progress counter belongs in
  `parallel.rs::producer_loop` (single-threaded, no atomic), NOT in "route.rs +
  parallel.rs". `route.rs` has no read loop; `pipeline.rs`'s loop is test-only (main.rs
  always uses `*_parallel`). The "dual-dispatch trap" framing is misapplied here.
- **C2.** Pin the `Processed lines: N` semantics to **input SAM records** (+1 per SE
  record, +2 per PE pair) to byte-match Perl's per-SAM-line count; remove the
  "exactness not required" hedge — for a comparison-with-Perl counter the per-tick
  semantics ARE the value.

### Important (resolve in plan text before implementation)
- **I1.** Correct the "@PG/@HD structural access is free" assumption: no such API is
  wired up. Recommend **filtering the existing serialize-to-SAM-text path** (the one
  `detect_paired_from_header` already uses) to `@PG`/`@HD` lines and printing them
  as-is — byte-faithful, sidesteps tag-reordering, reuses proven code. Estimate the
  small new bismark-io helper.
- **I2.** Pin the single emit point for banner/params/mode/provenance to
  `parallel.rs::run_pipeline` after `open_reader`+`build_chr_name_table`, before worker
  spawn. Derive the mode-source suffix from `config.paired_mode` (not `is_paired`,
  which has lost provenance).
- **I3.** Quiet-gate audit: enumerate BOTH `failed to remove` warnings (output.rs:317
  AND :369) as never-gated; state the two trailing blank lines (:330-331) gate together
  with kept/deleted; explicitly decide+document whether the Phase G `[bismark-extractor]
  spawning:` line (subprocess.rs:411) is gated or an acknowledged `--quiet` limitation.
- **I4.** Add at least one **automated** test (not colossal eyeballing): assert
  `--quiet` silences info but not a forced error, and assert banner/mode/params + the
  progress cadence on a tiny synthetic BAM. Replace verification step 5's
  "grep route.rs and parallel.rs" with "assert counter in producer_loop + cadence
  test". Add an AutoDetect cell to the byte-identity smoke to guard the header-read
  path.

### Optional / nits
- **O1.** Banner uses `CARGO_PKG_VERSION` (via `version_string()` style), NOT
  `output.rs::BISMARK_VERSION` — state this so nobody "matches Perl" by mistake.
- **O2.** Final summary mirrors the **splitting-report** figures (live even under
  `--mbias_off`/`--mbias_only`), not the M-bias tables; say so.
- **O3.** Keep `--verbose` long-only (no `-v`) to avoid future `-v`==version confusion;
  the existing `Cli::command().debug_assert()` test will catch any short-flag collision
  at test time.
- **O4.** Default-ON is the right choice (Perl parity + the stated motivation); the
  opt-in concern is fully neutralized by a complete `--quiet` (see I3). No third flag
  needed for the ~110-line progress volume.
