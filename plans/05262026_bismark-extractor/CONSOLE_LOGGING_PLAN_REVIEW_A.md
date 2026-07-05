# Plan Review A — extractor console progress/diagnostics logging

**Reviewer:** A (independent)
**Plan:** `CONSOLE_LOGGING_PLAN.md`
**Date:** 2026-05-29
**Verdict:** Sound in intent and channel choice; **two structural assumptions in
the plan are wrong as written** and must be corrected before implementation.
Both concern *where* emission happens, not *whether* it should. Neither affects
byte-identity. Details below.

---

## 1. Logic / correctness

### 1.1 Dual-dispatch claim is HALF right — and the half it gets wrong matters (Critical)

The plan asserts (§ Implementation outline step 3, and again in Verification
step 5) that the read loop "exists in BOTH `route.rs` (the `--parallel 1` path)
AND `parallel.rs` (the `--parallel N` worker path)", citing the #876/#879
dual-driver trap. **This is inaccurate and will misdirect implementation.**

Actual dispatch (verified `main.rs:89-114`):

- `main::run` dispatches **exclusively** to `extract_se_parallel` /
  `extract_pe_parallel` (`parallel.rs`) for *every* value of `--parallel`,
  **including N=1** (`config.parallel` defaults to 1, validated `>= 1`).
- The legacy single-threaded `extract_se` / `extract_pe` in `pipeline.rs` are
  **no longer on any runtime path**. `lib.rs:` explicitly documents them as the
  "byte-identity reference for the test suite" — they are only reached from
  `cargo test`, never from the binary.
- `route.rs::route_call` is the per-call writer used by the legacy
  `pipeline.rs` loops; in the parallel path the equivalent work lives in
  `parallel.rs::process_se` / `process_pe` (M-bias + counters) and
  `collector_loop` → `write_routed_call` (file writes).

**Consequence for the plan:**
- The progress counter does **NOT** belong in `route.rs`. `route.rs` is per-call,
  not per-record, so a counter there would count *calls* (Z/z/X/x/H/h bytes),
  not "lines"/records — wrong semantics.
- A counter added only to `pipeline.rs::extract_se/extract_pe` would **never
  fire at runtime** — those functions are test-only.
- The correct and **only** runtime site for a record/line progress counter is
  the **producer thread** in `parallel.rs::producer_loop` (`parallel.rs:315`),
  which is the single point that drives `records()` for both SE and PE at all N.

So the plan's "add to both" instruction is actively harmful here: following it
literally puts the counter in two test-only/per-call locations and misses the
one live site. **The dual-dispatch concern that DID apply to #876/#879 (which
touched both `pipeline.rs` and `parallel.rs` because both are exercised by
tests) does not generalize to "anything user-visible must be in both" — runtime
output only flows through `parallel.rs`.**

### 1.2 Progress counter in the producer is sound and avoids the atomic entirely (Important)

The plan agonizes over "aggregate `AtomicU64` vs per-worker" and worries about
interleaved/garbled output at high `--parallel`. **This is a non-problem if the
counter lives in the producer thread**, which is the natural home:

- The producer is **single-threaded** and reads every record exactly once in
  input order (`parallel.rs:315-467`). A plain `u64` local (`next_idx` already
  exists — `parallel.rs:320`) increments monotonically with zero contention and
  zero atomics.
- Emitting `Processed N lines` every 500k from the producer is inherently
  ordered (single thread, single `eprintln!` call site) — no interleaving, no
  garble, regardless of N.
- For PE, the producer increments `next_idx` once per *pair* (`parallel.rs:417`),
  which matches Perl's `sequences_count` "lines" semantics that the plan wants
  (§Assumptions: "PE: Perl counts 2 call strings/pair … match Perl's lines
  semantics"). NOTE: Perl's progress counter `:1553/:1850` counts the same
  per-record/per-pair unit as `sequences_count`, so producer `next_idx` is the
  right quantity. Confirm the exact Perl unit during implementation, but
  `next_idx` is structurally the closest match.

**Recommendation:** drop the `AtomicU64` design entirely. Put the counter in
`producer_loop`. This is simpler, contention-free, ordering-correct, and removes
the whole "aggregate vs per-worker" decision the plan defers to review. The only
caveat: the producer counts records *read*, not records *successfully written* —
but Perl's counter is also a read-side counter, so this is faithful.

### 1.3 Single emission chokepoint for banner/params/mode/header — does NOT exist where the plan says (Critical)

The plan (§Implementation outline step 2, §Assumptions) wants banner + params +
mode + header provenance emitted "AFTER the header is parsed and PE/SE resolved,
BEFORE the read loop … Likely in `pipeline.rs`". **`pipeline.rs` is the wrong
file (test-only, per §1.1).** The real control flow:

- PE/SE resolution happens in `main.rs::run` (`main.rs:90-113`) for the
  `AutoDetect` case — the header probe (`detect_paired_from_header`) runs there,
  then dispatches to `extract_{se,pe}_parallel`.
- The header is then **re-opened** inside `run_pipeline` (`parallel.rs:186`)
  where `chr_table` is built. The probe reader in `main.rs` is dropped
  (`main.rs:107`) before re-open.

There are therefore **two candidate chokepoints**, and they have a tradeoff the
plan does not address:

1. **`main.rs::run`** — the only place that knows the *resolved* SE/PE decision
   for all three `PairedMode` variants (Explicit S, Explicit P, AutoDetect) at a
   single point (`main.rs:90`). But the header object available here is the
   `probe` reader, and only in the AutoDetect arm; the explicit arms never open
   a reader in `run` (they go straight into `*_parallel`). So header provenance
   (@PG/@HD) is **not** uniformly reachable here.
2. **`run_pipeline` in `parallel.rs:177`** — single function for both SE and PE
   (called via `extract_se_parallel`/`extract_pe_parallel`), runs once per file,
   has the `reader`/`header` in hand (`parallel.rs:186`), and knows `is_paired`
   (its own arg). This is the **true single chokepoint** for banner + params +
   header provenance + mode. Recommend emitting here, right after
   `build_chr_name_table` (line 187) and before the worker/producer spawn.

The mode-detection *line* (the "auto-detected vs forced" wording) needs the
`PairedMode` enum which is in `config.paired_mode`, plus the resolved boolean.
`run_pipeline` receives `is_paired: bool` and has `config.paired_mode` — both
available — so it can print "Treating file(s) as paired-end (auto-detected from
@PG)" correctly. Good.

**Net:** emit everything in `run_pipeline` (parallel.rs), not `pipeline.rs`, and
not split across `main.rs`. This IS a single chokepoint — the plan's goal is
achievable — but the plan names the wrong function. One-line fix to the plan,
but a real trap for the implementer who trusts the plan's file reference.

### 1.4 Final-summary echo: values are in hand, but the reachable point is `state.finalize`, which already runs Phase G (Important)

The plan (§Implementation outline step 4) says emit the final summary "where the
splitting-report stats are finalized." Those stats live in
`state.report: SplittingReport` and are consumed by `write_splitting_report`
inside `ExtractState::finalize` (`state.rs:117-188`). The values
(`calls_total`, `calls_cpg_meth`, … and `SplittingReport::percent_meth` /
`percent_meth`) are indeed already computed — the echo is near-free, plan claim
**confirmed**. The percentage helper `percent_meth` (`output.rs:437`) is public
and reusable; `write_percent_or_fallback` matches Perl wording.

Caveats the plan should note:
- `finalize` runs the Phase G subprocess chain at its tail (`state.rs:176-185`)
  when `--bedGraph`/`--cytosine_report` — but those are **rejected** at
  `main.rs:73` today, so not reachable in this build. Still, the final-summary
  echo should be emitted **after `write_splitting_report` succeeds** and before
  (or after) M-bias, matching Perl's `warn` at `:2562` which is the in-loop
  report mirror. Pick a deterministic spot inside `finalize`.
- The kept/deleted lines (`output.rs:319/322`) are emitted by
  `finalize_with_empty_sweep`, called from `finalize` (`state.rs:144`) **before**
  `write_splitting_report`. So the natural stderr order is: kept/deleted sweep →
  (blank lines) → final summary. That matches Perl's ordering (sweep at
  `:607/615`, final report `warn` later). Good — but it means the quiet-gate has
  to thread into `output.rs::finalize_with_empty_sweep` too (plan step 5
  acknowledges this).

---

## 2. Assumptions

### 2.1 noodles `Header` @PG/@HD access — plan's "structural iteration" is feasible but the project already has a better idiom (Important)

The plan (§Investigation findings, §Implementation outline step 2) assumes it
will "iterate the noodles `Header` program (`@PG`) records + the `@HD` line; skip
the reference (`@SQ`) map." Verified against noodles-sam 0.85.0:

- `Header::programs()` → `&Programs`, where `Programs: AsRef<IndexMap<BString,
  Map<Program>>>` (`programs.rs:212`). So `header.programs().as_ref()` yields an
  iterable of `(id, Map<Program>)`. `Map<Program>` exposes `other_fields()` →
  `IndexMap<tag::Other, BString>` with `CL` (command line), `PN`, `VN`, `PP`
  tags (`program/tag.rs`). So reconstructing a `@PG` line *is* possible field by
  field.
- `Header::header()` → `Option<&Map<Header>>`; `@HD` has `VN` (version) +
  `SORT_ORDER`/`GROUP_ORDER` in `other_fields()` (`header/tag.rs`).
- BUT reconstructing the **original line text** field-by-field is fiddly and
  risks reordering tags (IndexMap preserves insertion order, so it's *usually*
  faithful, but the implementer must not re-sort).

**Strong recommendation (reuse, not reinvent):** `bismark-io` already solves the
exact "serialize the header back to SAM text and walk lines" problem in
`detect_paired_from_header` (`read.rs:649-677`). It does:
```rust
let mut buf = Vec::new();
let mut writer = noodles_sam::io::Writer::new(&mut buf);
writer.write_header(header)?;          // emits canonical @HD/@SQ/@PG text
let text = String::from_utf8_lossy(&buf);
for line in text.lines() { … }
```
This is the **stable, version-robust contract** (the read.rs comment at :650-653
explicitly chose text-serialization over the in-memory `Programs` type *because*
of noodles API churn). The console-logging feature should do the same: serialize
once, then filter lines by prefix — emit `@HD` and `@PG`, skip `@SQ` unless
`--verbose`. This is far simpler than field-by-field `Map<Program>` walking, is
byte-faithful to what Perl's `warn` would dump, and reuses a proven pattern.
**The plan should be updated to specify the serialize-and-filter approach and
ideally expose a small helper from `bismark-io` (or copy the ~8-line idiom).**
Consider extracting `bismark-io::header_lines(&Header) -> Vec<String>` to avoid
a third copy of the writer-buffer idiom (DRY with `detect_paired_from_header`).

### 2.2 Banner version source (Optional)

Plan step 1 wants `version <crate ver>`. Two version strings exist:
`version_string()` (`lib.rs`, the TG-style `name semver (os/arch)`) and
`BISMARK_VERSION = "v0.25.1"` (`output.rs:33`, the Perl-locked string for file
headers). The banner should use **`env!("CARGO_PKG_VERSION")`** (currently
`1.0.0-beta.1`) or `version_string()`, NOT `BISMARK_VERSION` — the latter is the
byte-identity-locked Perl version for *output files* and conflating them would
mislead users about which binary they ran. Plan says "crate ver" which is
correct; just flag the trap explicitly so the implementer doesn't grab
`BISMARK_VERSION` because it's the one already imported in the output path.

### 2.3 `--quiet`/`--verbose` threading (confirmed straightforward)

`Cli` (`cli.rs:31-221`) is a flat clap-derive struct; adding two `bool` fields
(`#[arg(short='q', long="quiet")]`, `#[arg(long="verbose")]`) and two
`ResolvedConfig` fields, threaded through `Cli::validate` (mechanical, mirrors
the existing `no_header`/`gzip` bools), is trivial. `ResolvedConfig` is already
`Clone` and is cloned into each worker (`parallel.rs:203`) — the quiet/verbose
flags will ride along for free. **One concern:** workers should NOT emit progress
(only the producer does, §1.2), so the flags being in the per-worker config copy
is harmless but the implementer must not wire progress into `worker_loop`.

### 2.4 `-q` short flag collision (Optional)

Verify `-q` doesn't collide with any existing short flag. Scanning `cli.rs`:
short flags in use are `-s`, `-p`, `-o`, `-g`, `-V`. `-q` is free. Good. (Also
note Perl has no `-q`; this is a Rust-only ergonomic addition — fine, but
document it as a divergence in the splitting-report/SPEC flag table so the
"35 Perl flags" mapping comment in `cli.rs:3` stays honest.)

---

## 3. Efficiency

- **Progress counter overhead:** with the producer-thread design (§1.2), it's a
  `u64` increment plus a `% 500_000 == 0` branch per record — negligible vs the
  BAM decode + channel send already happening per record. No atomic, no
  contention. The plan's worry about "atomic contention at high --parallel" is
  moot once the counter leaves the workers.
- **Header serialization:** `write_header` into a `Vec<u8>` once per run is O(header
  size) — trivial (microseconds), already paid by `detect_paired_from_header` in
  the AutoDetect path anyway. Doing it a second time for provenance display is
  fine; or thread the already-serialized text through if profiling ever cared
  (it won't).
- **eprintln! per progress tick:** stderr is line-buffered/unbuffered; 500k
  cadence means ~tens-to-hundreds of lines for a 55.7M-read file — trivial.

No efficiency concerns once §1.2 is adopted.

---

## 4. Validation sufficiency

### 4.1 phase_h_smoke.sh is sufficient to prove byte-identity is unaffected — confirmed (Important, reassuring)

I traced the harness end-to-end (`phase_h_smoke.sh`). The plan's claim that
stderr is outside the byte-identity invariant is **correct**:

- Both binaries run with `… 2>&1 | tail -3` (lines 188, 200). stderr is merged
  to stdout, truncated to 3 lines, and **only displayed** — never captured to a
  compared artifact.
- The file comparison loop (lines ~234-300) operates on files in
  `$PERL_OUT` / `$RUST_OUT` directories via `cmp` / `sort|md5sum` / `zcat|sort`.
  Console output is never written to those dirs. **Extra stderr volume cannot
  perturb any compared file.**
- Wall-clock parse uses anchored regex `^Perl: [0-9]+s$` / `^Rust: [0-9]+s$`
  against `diff_summary.txt` (matrix driver lines 288-289), which is **built by
  the script** from `$PERL_ELAPSED`/`$RUST_ELAPSED` (shell `date +%s` arithmetic),
  NOT from binary stderr. So new stderr lines cannot corrupt the timing parse.

**One subtle risk the plan does NOT mention (Important):** the `2>&1 | tail -3`
pipe means the *last 3 lines* of merged stdout+stderr are shown. Today the last
stderr lines are the kept/deleted sweep + two blank lines (`output.rs:319-331`).
With the new final-summary block emitted *after* the sweep, the `tail -3` will
now show the **final methylation summary lines instead of the kept/deleted
lines**. This does not affect PASS/FAIL (file compare is independent), but:
- It changes what a human sees in the matrix log (cosmetic).
- If any *downstream tooling* greps the smoke's displayed tail for the
  `was empty -> deleted` / `contains data -> kept` strings, it would break. I
  did not find such a grep in `phase_h_pe_matrix.sh` (it parses `diff_summary.txt`,
  not the tail), so this is **low risk** — but the plan should explicitly verify
  no consumer depends on the tail content, and consider whether the final
  summary should precede or follow the sweep to keep the most useful 3 lines
  visible.

### 4.2 Verification step 5 (counter at both N=1 and N=4) — restate per §1.1 (Important)

The plan's verification step 5 says "grep both route.rs and parallel.rs". Given
§1.1, the correct verification is: confirm the progress line appears at
`--parallel 1` AND `--parallel 4` (both go through `parallel.rs::producer_loop`,
so a single correct site covers both N) — the grep target is `parallel.rs`
only, NOT `route.rs`. Keep the runtime check (run at N=1 and N=4 and eyeball),
drop the route.rs grep.

### 4.3 Missing test: `--quiet` suppresses info but not errors (Important)

The plan's edge cases mention it but there's no concrete test named. Add at
least one integration-style assertion: run with `-q` against a bad path, assert
stderr still contains `error:` (from `main.rs:44`). And a unit test on the
quiet-gate helper: `Logger{quiet:true}.info(...)` is a no-op while
`.warn/.error(...)` always prints. Since the gate is "a tiny helper wrapping
eprintln!", give it a testable surface (e.g. write to a `&mut dyn Write` sink in
tests rather than hardcoding `eprintln!`) — otherwise the quiet behavior is
untestable without subprocess capture. **This is a design nudge: don't hardcode
`eprintln!` inside the helper; take a writer or at least gate a function that
tests can call.** (See §5.2.)

### 4.4 Empty-input / 0-record path (Optional, confirmed safe)

Plan edge case "Empty input / 0 reads: progress prints nothing; final summary
shows zeros." Confirmed reachable: `producer_loop` breaks immediately on
`None`, no progress tick fires; `SplittingReport::default()` is all zeros;
`write_percent_or_fallback` already handles the zero-denominator case
(`output.rs:524`, the "Can't determine percentage…" branch). The echo will show
zeros, no panic. Good — but add a smoke assertion on a 0-read BAM if one is
cheaply available.

---

## 5. Alternatives

### 5.1 `--quiet`/`--verbose` two-bool vs `-v` count / log-level enum (Optional)

The plan uses two independent bools. Tradeoffs:
- **Two bools (plan):** simplest, matches the narrow requirement (info on by
  default; `-q` silences info; `--verbose` adds only `@SQ`). Risk: `-q` +
  `--verbose` together is an undefined combination. Recommend a `conflicts_with`
  between them (clap one-liner) OR define precedence (e.g. `--quiet` wins,
  `--verbose` ignored). The plan does not address `-q --verbose`; **add a mutex
  or documented precedence** (Important-ish, cheap).
- **`-v` count / level enum:** more idiomatic for tools that may grow more
  verbosity tiers, but YAGNI here and diverges from Perl's all-or-nothing. Plan's
  choice is appropriate for the stated scope; just close the `-q --verbose` gap.

### 5.2 `log` + `env_logger` crate vs raw `eprintln!` gate (Optional)

- Plan keeps raw `eprintln!` via a gate helper — consistent with the existing
  codebase idiom (`output.rs` already uses `eprintln!`), no new dependency,
  honoring memory `project_bam_io_decision` minimal-dep ethos. **Endorsed.**
- The one thing `log`/`tracing` would buy is testability and env-var control;
  not worth a dependency for this. BUT the gate helper should still be designed
  for testability (§4.3): prefer a small struct method that can be pointed at a
  test sink, or factor the "format the line" logic from the "write it" so the
  formatting is unit-testable without capturing stderr.

### 5.3 Counter cadence configurability (Optional)

Perl hardcodes 500k (`:1553/:1850`). Plan matches. Fine. Don't add a flag for it
(YAGNI); a `const PROGRESS_INTERVAL: u64 = 500_000;` is enough and matches Perl
familiarity.

---

## Action items (prioritized)

### Critical (fix before implementation — plan is factually wrong here)
1. **§1.1 / §1.3 — Correct the dispatch model in the plan.** The runtime path is
   `parallel.rs` for ALL `--parallel` values including N=1; `pipeline.rs` is
   test-only and `route.rs` is per-call. Progress counter → producer thread in
   `parallel.rs::producer_loop`. Banner/params/mode/header → `run_pipeline`
   (`parallel.rs:177`, right after `build_chr_name_table`), NOT `pipeline.rs`.
   Remove all instructions to edit `route.rs`/`pipeline.rs` for runtime output.

### Important
2. **§1.2 — Drop the `AtomicU64`.** Use the producer's existing single-threaded
   `next_idx` (or a sibling local counter). Removes contention + interleaving +
   the "aggregate vs per-worker" open question entirely.
3. **§2.1 — Reuse the serialize-then-filter header idiom** from
   `bismark-io::detect_paired_from_header` (`read.rs:649`) for @PG/@HD
   provenance; consider extracting a shared `header_lines()` helper. Avoid
   field-by-field `Map<Program>` reconstruction.
4. **§4.1 — Note the `tail -3` display shift.** New final-summary lines will
   replace the kept/deleted lines in the smoke's displayed tail; confirm no
   consumer greps that tail (I found none) and decide summary-vs-sweep ordering.
5. **§4.2 — Restate verification step 5** to grep/run `parallel.rs` only (both N
   go through it); drop the route.rs grep.
6. **§4.3 / §5.2 — Make the quiet-gate helper testable.** Don't hardcode
   `eprintln!`; allow a test sink or factor formatting from writing. Add a test
   that `-q` silences info but `error:` still prints.
7. **§5.1 — Resolve `-q --verbose` interaction** (clap `conflicts_with` or
   documented precedence).

### Optional
8. **§2.2 — Banner uses `CARGO_PKG_VERSION`/`version_string()`, NOT
   `BISMARK_VERSION`** (the latter is the Perl-locked output-file version).
9. **§2.4 — Document `-q` as a Rust-only divergence** in the SPEC/flag-mapping
   so `cli.rs:3` "35 Perl flags" comment stays accurate.
10. **§5.3 — `const PROGRESS_INTERVAL = 500_000`** rather than a magic literal.
11. **§4.4 — Add a 0-read BAM smoke assertion** if cheap.

---

## Summary

The feature is well-motivated, the stderr channel choice is correct, and
byte-identity is genuinely unaffected (harness verified — console is `tail -3`
display only, never compared). **However the plan's central structural premise
is wrong: the live runtime path is `parallel.rs` for every `--parallel` value
including N=1 — `pipeline.rs` and `route.rs` are test-only / per-call, so the
"add to both drivers" instruction would put the progress counter in two
non-runtime locations and miss the one live site (the producer thread).** Once
relocated, the counter wants no atomic at all — the single-threaded producer's
existing `next_idx` is the correct, contention-free source. Banner/params/mode/
header all belong at one real chokepoint (`run_pipeline`), and the @PG/@HD dump
should reuse `bismark-io`'s proven serialize-and-filter idiom rather than
hand-walking `Map<Program>`. These are plan-text corrections, not feature
blockers.
