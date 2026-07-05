# Phase G Plan Review — Reviewer A

**Plan reviewed:** `plans/05262026_bismark-extractor/PHASE_G_PLAN.md` (rev 0, 2026-05-27)
**Reviewer:** A (independent of Reviewer B)
**Date:** 2026-05-27
**Verdict at a glance:** NEEDS-REVISIONS — two internal inconsistencies (one Critical: contradictory discovery-order spec; one Critical: missing data needed for the wire-up), several Important gaps in test coverage and Perl-fidelity, and a handful of polish items. None of the findings change the *goal* of Phase G; they all change either correctness, the implementation surface, or test coverage.

---

## 1. Logic review

### 1.1 §3.2 discovery order CONTRADICTS §10 (Critical)

§3.2 (the normative behavior section) lists discovery in this order:
1. BISMARK_BIN env var
2. PATH (via `which::which`)
3. `current_exe()?.parent()?`

§10's "Open" table row labelled "Subprocess discovery order" says:
> "PATH → BISMARK_BIN env → current_exe() parent."

…and the rev-0 entry at the top of §0 says:
> "subprocess discovery via PATH + BISMARK_BIN env override"

…which is ambiguous, and §11 (Reviewer-attention magnet #2) says:
> "§3.2 discovery order PATH-first, BISMARK_BIN-second — alternatives: BISMARK_BIN-first…"

So three different parts of the same plan disagree on what §3.2 actually says. Either §3.2 is right (BISMARK_BIN first) and §10/§11 are stale, or §10/§11 are right (PATH first) and §3.2 inverts them. This MUST be reconciled before implementation: it changes user behavior (does a system-installed Bismark on PATH win over an explicit override, or vice versa) and it changes the test in §5.4.1 `discover_subprocess_uses_bismark_bin_env_override` (which under §3.2's order would pass trivially without exercising the precedence rule).

Recommendation: **BISMARK_BIN should win**. That's the conventional UNIX semantics for `_BIN`/`_HOME`/`_PATH` env vars (explicit user intent overrides ambient PATH); it's what the rev-0 self-review intuited; it's what §3.2 ended up writing. Fix §10 + §11 to match §3.2, AND add a test that asserts BISMARK_BIN wins even when PATH also contains a copy.

### 1.2 `ExtractState::input_basename()` does not exist (Critical)

§4.5 says the wire-up at `state.rs::ExtractState::finalize` will call:

```rust
crate::subprocess::run_phase_g_chain(
    config,
    /* input_basename = */ self.input_basename(),
    ...
);
```

…and then in passing: "`input_basename()` is a new (small) helper on `ExtractState` that returns the stem already computed for the splitting-report filename".

Looking at the current `state.rs`, the basename is **not stored on `ExtractState`**. What IS stored is `splitting_report_path: PathBuf` (line 44). The basename is a local `let input_basename = derive_basename(input);` inside `pipeline.rs::extract_se` / `extract_pe` that is *passed to* `ExtractState::new` but not retained.

To make §4.5 work, the plan must also add an `input_basename: String` field to `ExtractState` (or, alternatively, recover the stem from `splitting_report_path` by stripping the `_splitting_report.txt` suffix — fragile). This is missing from §2.2's "files touched" inventory and from §5.5. The LOC delta is tiny (~3 lines) but the omission means an implementer who follows the plan literally will discover at compile-time that the helper has nothing to read.

Recommendation: amend §4.5 to specify "add `input_basename: String` field to `ExtractState`; populate from the same value passed to `ExtractState::new`; expose via `pub fn input_basename(&self) -> &str`". Add the field to §2.2's LOC estimate. Add it to §7.1 Write surface.

### 1.3 `--buffer_size` Perl-fidelity gap when neither flag is set (Important)

Plan §2.4.1 shows Perl pushes `--buffer_size $sort_size` whenever `!$ample_mem`. Perl `:1306` defaults `$sort_size = '2G'` when the user didn't pass `--buffer_size`. So Perl ALWAYS passes a `--buffer_size <SIZE>` argv element to bismark2bedGraph when `!ample_memory`.

Plan §4.3 (`build_bismark2bedgraph_argv` signature) and §3.7 (gzip matrix) do not specify what the Rust arg-builder does when `config.buffer_size.is_none() && !config.ample_memory`. The plan stores `buffer_size: Option<String>` and (per cli.rs comments at lines 152-158) treats `None` as "let bismark2bedGraph use its own default ('2G')".

bismark2bedGraph's own default is also `2G` (verified at `bismark2bedGraph:771`), so the **data byte-identity is fine**. But the **subprocess argv differs**: Rust would invoke `bismark2bedGraph` with no `--buffer_size` flag; Perl invokes it with `--buffer_size 2G`. This may affect:

- bismark2bedGraph's stderr greeting line (`bismark2bedGraph:63` warns the sort buffer size); Phase H byte-identity gates that inspect bismark2bedGraph stderr would diverge.
- Any downstream tooling that consumes the bismark2bedGraph command-line echo from `_splitting_report.txt` or stderr capture.
- Test parity (a Phase G "Rust argv == Perl argv for this config" parity test, if added per §5 below, would fail).

The plan should resolve this explicitly. Two valid options:
(a) In the arg-builder, when `buffer_size.is_none() && !ample_memory`, push `--buffer_size 2G` to mirror Perl's defaulting precisely.
(b) Acknowledge the divergence in SPEC §6.6 and assert at the test layer that Rust deliberately omits the flag.

(a) is the byte-identity-safest choice and matches the plan's stated goal of "lockstep with Perl ensures byte-identity at the bedGraph output level" (§10 row `--ample_memory` pass-through).

### 1.4 `phase_g_real_*` smoke tests can land in an `--include-ignored` CI matrix and silently pass on missing tools (Important)

§5.4.3's two `#[ignore]`'d real-subprocess tests are guarded by `#[ignore = "requires bismark2bedGraph on PATH; opt-in via cargo test -- --ignored"]`. If oxy CI ever runs `cargo test -- --ignored` without bismark2bedGraph available, the test will:
- Discover-fail with `SubprocessNotFound`, OR
- Discover-succeed but the subprocess will fail with its own error.

Either way, the test will register as a failure (good). But the test as sketched has no concrete assertion *after* a successful subprocess run — it says only `/* small fixture; check output exists */`. "Check output exists" is a weak gate: bismark2bedGraph touches output files early; "exists" passes even on a corrupt/empty output. Specify:
- Open the produced `.bismark.cov.gz`, read at least one row, assert column count == 6.
- Open the produced `.bedGraph` (or `.bedGraph.gz`), assert non-empty and has the `track type=bedGraph` header line (unless `--no_header` was passed in the fixture).

### 1.5 `phase_g_passes_genome_folder_to_c2c_quoted_via_typed_argv` (Important, naming)

This test name implies it checks that the genome folder gets shell-quoted ("…quoted via typed argv…"). In reality the assertion is the *opposite*: typed argv means no quoting happens because the OS gets the raw `OsStr`. Rename to something like `phase_g_passes_genome_folder_with_spaces_without_shell_quoting`; otherwise readers will assume it tests something the test does not test.

### 1.6 Coverage2cytosine c2c flag surface — completeness audit (Important)

I verified `coverage2cytosine:2011-2029` (the GetOptions block). c2c accepts these flags that the plan does NOT pass through from the extractor:

| Flag | Plan covers? | Comment |
|---|---|---|
| `merge_CpGs` | No | Not exposed at the extractor CLI; SPEC §3 / §6.6 don't mention it. Correct to omit. |
| `GC\|GC_context` | No | Not exposed; NOMe-Seq specific (`--yacht` adjacent). Correct to omit. |
| `ffs` | No | Tetra-context; out of scope for v1.0. Correct to omit. |
| `nome-seq` | No | NOMe-Seq mode. Out of scope. Correct to omit. |
| `discordance_filter=i` | No | Not in extractor CLI. Correct to omit. |
| `threshold\|coverage_threshold=i` | **No** | Extractor has `--cutoff` (line 130 cli.rs); Perl `:401-417` does NOT forward to c2c either (only to bismark2bedGraph). Plan correctly mirrors — but is silent on it; the implementer might wonder. |
| `drach\|m6A` | No | Out of scope. Correct to omit. |

This is mostly fine but the plan should add a one-line note: "`coverage2cytosine` exposes additional flags (`--merge_CpGs`, `--GC_context`, `--ffs`, `--nome-seq`, `--discordance_filter`, `--threshold`, `--drach`) that the Phase G extractor wiring deliberately does not pass through. Mirrors Perl `:388-424`." Otherwise a future contributor will assume an oversight.

### 1.7 Stderr tee drain-thread join semantics on success path (Important)

§3.4 says (step 4): "Main thread calls `child.wait()`; on success, returns `Ok(())`. On non-zero exit, joins the drain thread (so the ring buffer is final)…".

This means on the **success path** the drain thread is NOT joined. Two problems:

(a) When `RealRunner::run` returns `Ok(RunOutcome { exit_status, stderr_tail })`, what's in `stderr_tail`? The thread may still be reading the last bytes after `child.wait()` returns (Unix close-on-exec races; small but real). The ring buffer snapshot must be deterministic on the success path too — join before returning even on success.

(b) The drain thread is owned by something; without joining, it leaks until the OS reaps it. If `RealRunner::run` is called twice (e.g. bismark2bedGraph then coverage2cytosine), the first drain thread may still be alive when the second spawns. Benign but messy.

Fix: join the drain thread unconditionally before returning (`Ok` or `Err`). Capture any drain-thread panic via `thread::Result` and either bubble it as `SubprocessSpawnFailed` or log to stderr. Add a unit test that pushes a final line *just before* EOF and asserts the snapshot contains it — would catch the success-path race.

### 1.8 Ring buffer "lines larger than CAP" behavior not specified clearly (Important)

§3.4 says "RingBuffer<65536>" with "oldest bytes evicted on overflow" and §5.4.1's test `ring_buffer_handles_lines_larger_than_capacity` asserts "snapshot is some bounded substring". The actual behavior under a single 100 KiB write needs to be deterministic, because the test asserts "some bounded substring" — which is too loose.

Two reasonable behaviors:
(a) Drain N bytes from the front to make room (memmove or `Vec::drain(..n)`), so the snapshot is the LAST 64 KiB of the line. Loses the line-prefix.
(b) Store byte-aligned chunks, and on overflow truncate the snapshot at the most recent newline so the snapshot is a complete line tail.

(a) is the simpler impl. Pick one and write the test as a tight assertion (`assert_eq!(snapshot.len(), 65536); assert_eq!(snapshot.as_bytes()[0], expected_byte);`). The current test phrasing risks vacuous green.

### 1.9 Drain-thread architecture assumes line-buffered stderr (Important)

§3.4 step 3 uses `read_line` for each loop iteration. `read_line` blocks until a `\n` or EOF. If the subprocess writes partial lines without flushing (e.g. a progress indicator `\rN%` carriage-return-only updates), `read_line` will not return until the subprocess closes stderr. The user sees nothing live during such writes.

bismark2bedGraph's source uses `warn` (which appends `\n`), so in practice this is fine. But the plan's claim "live to user's stderr" is conditional on "subprocess flushes newline-terminated lines". Document this; consider using `BufReader::read_until(b'\n', …)` with a fallback timeout or `read` (bytes) drain to make it independent of subprocess line discipline. At minimum, add a SPEC §6.6 caveat.

### 1.10 `--counts` flag has `default_value_t = true` in CLI but is never used (Important, nit-on-the-nit)

Looking at `cli.rs:139-140`: `pub counts: bool` defaults to `true`. ResolvedConfig forwards it (line 496). The plan §2.4.4 correctly says Rust does NOT push `--counts` to bismark2bedGraph (mirroring Perl's commented-out line). So `ResolvedConfig::counts` is dead-field after Phase G. Either:

(a) Remove the field from ResolvedConfig + CLI (best — true to "field is not load-bearing"), OR
(b) Leave it and add a SPEC §3 row note clarifying that the flag is accepted-and-ignored.

Plan §10 row "Sleep(1) after bismark2bedGraph" notes Perl-isms not ported; do the same for `--counts` here. Minor but documented dead-flag is a clarity tax.

### 1.11 Subprocess exit-status semantics on Unix vs Windows (Optional/Important)

§3.5's `SubprocessFailed` error renders `exit_status: std::process::ExitStatus` via `Display`. On Unix `ExitStatus` may signal-terminated (no exit code, just signal). On Windows it's always an `i32` code. The error message template `"subprocess `{tool}` exited with status {exit_status}: …"` will produce platform-dependent strings; tests that assert on the error message will be flaky cross-platform.

Assumption A6 says "Windows is out of scope for v1.0", so this may be acceptable. But the assertion fixtures should be tightened (`assert!(matches!(err, BismarkExtractorError::SubprocessFailed { .. }))` rather than string contains).

### 1.12 §3.8 "subprocess produces enormous stderr (gigabytes)" is not actually exercised (Important)

The edge-case row says "Bounded by the 64 KiB ring buffer". §5.4.1 has `ring_buffer_evicts_oldest_when_capacity_exceeded` which pushes 100 KiB. That tests the ring buffer in isolation but does NOT test the full `RealRunner` end-to-end with a high-volume-stderr subprocess. Without that integration, the claim that the *runner* doesn't grow memory linearly with stderr volume is asserted but not validated.

A cheap test: write a tiny shell script that does `python -c 'for i in range(200000): print(f"line {i}", file=__import__("sys").stderr)' ` and assert `RealRunner::run` completes with a `stderr_tail` of bounded length. Skip on Windows.

### 1.13 What happens to `child.stdin`? (Optional)

§3.3 says "stdin: closed (no input piped). Perl's chain feeds via file paths in argv, not stdin." But `Command::default()` inherits stdin from the parent. So if the parent's stdin is a terminal, the subprocess inherits the terminal — which may cause weird behavior (terminal-mode subprocess wanting input, scrambling the TTY). Explicit `Stdio::null()` is safer.

The plan claims "stdin: closed" but doesn't say WHERE this happens in §4 (the `RealRunner::run` body sketch only mentions stderr). Make it explicit: `.stdin(Stdio::null())`.

---

## 2. Assumption review

### 2.1 A1 (subprocess presence) — well-handled (✓)

`discover_subprocess` failure path is explicit; error is typed.

### 2.2 A2 (long-form flag names) — verified (✓)

I cross-checked `bismark2bedGraph:637-651` and `coverage2cytosine:2011-2029` against the matrix in §2.4.4. Both subprocesses accept the long-form names as listed. Tip: also note that `--CX_context` is acceptable to both as either `--CX` or `--CX_context` (`"CX|CX_context"` GetOptions key). The plan defaults to passing the long form (good for grep-ability).

### 2.3 A3 (locale / env inheritance) — implicit risk (Important)

The assumption "locale settings for the internal UNIX sort step are the user's responsibility" is fine — but note that the **byte-identity gate** in Phase H may break across hosts with different `LC_ALL` settings (e.g. Perl wrapper on macOS with `LC_ALL=en_US.UTF-8` sorts differently than Linux with `LC_ALL=C`). This is the user's problem, but Phase H needs to **pin** an LC_ALL when invoking both Rust and Perl. Add as a Phase H pre-req; flag it as a downstream gotcha here.

### 2.4 A5 (`which` crate) — UNVERIFIED at plan time (Important)

I grepped the workspace Cargo.tomls: `which` is **NOT** in `rust/bismark-extractor/Cargo.toml`, `rust/bismark-io/Cargo.toml`, or `rust/bismark-dedup/Cargo.toml`. So the plan's "likely already present" is wrong — adding `which` is a NEW dependency for the bismark-extractor crate.

This is a small thing (3 lines + a fresh `Cargo.lock` change), but:
- It's a license-audit moment. `which` is currently dual MIT/Apache (good), no transitive issues.
- The plan's §5.2 step 1 instructs the implementer to "verify at impl time" — but since the answer is firmly "no", the plan should commit to adding it now and skip the impl-time uncertainty. Or use `env::var("PATH")` + manual `Path::new(dir).join(name)` + `is_file()` to avoid the dep entirely (~15 LOC; not worth shipping a dep for IMO).

Recommendation: either commit to adding `which` (and update §5.2 / §7.1 dep list accordingly), or commit to inline PATH walking. Don't leave this open at the plan stage; it'll re-surface and waste implementer time.

### 2.5 A6 (current_exe reliability) — sufficient (✓)

Linux + macOS are the deploy targets per CLAUDE.md. Windows out-of-scope.

### 2.6 A10 "238 tests preserved" — needs a number-check (Optional)

The plan asserts "Existing 238 tests preserved". SESSION_HANDOFF.md says "All 238 tests passing". The Vec<PathBuf> return-type change touches `tests/output_phase_c2.rs`. Verify there are no callers of `finalize_with_empty_sweep` in `tests/` other than what §5.6 lists; if there are, the LOC delta in §2.2 misses them.

---

## 3. Efficiency analysis

### 3.1 Drain thread per subprocess invocation — acceptable

§6 correctly identifies that subprocess work dwarfs Rust overhead. The drain thread is a `std::thread::spawn` (~5-50 µs creation on Linux); negligible relative to a multi-minute sort step.

### 3.2 Ring buffer 64 KiB is reasonable but justification is thin

§3.4: "Why 64 KiB: large enough to capture a multi-paragraph 'I died because…' message…; small enough not to surprise on memory."

A multi-paragraph error from a Perl `die "..."` is typically <2 KiB. 64 KiB is wildly generous. The actual concern is non-error stderr (progress + warnings); on a successful run with `--ample_memory` over a 55M-read sample, bismark2bedGraph emits perhaps 50-200 lines (~10-20 KiB). 64 KiB is sufficient with margin.

That said: the cap is per-subprocess (so 128 KiB peak during the chain), not per-process. Even at 256 KiB cap × N invocations, the memory is irrelevant. Tuning is fine as-is; the §11 magnet entry is correct that this is bike-shedding territory.

### 3.3 No back-pressure between drain thread and main thread — fine

Drain thread is bounded-write-bounded-read; can't outpace stderr. Main thread blocks in `child.wait()`. No deadlock condition I can identify.

### 3.4 `build_*_argv` complexity — fine

`Vec<OsString>` of < 30 elements, called twice per phase-G run. O(1) per call practically.

---

## 4. Validation sufficiency

### 4.1 Mocked-runner tests do NOT exercise `RealRunner` (Important)

§5.4.2's tests use a `MockRunner` (§4.1). This is great for testing `run_phase_g_chain`'s **decision logic** (which tool runs, what argv, order). But it does NOT exercise:

- `RealRunner::run`'s `Stdio::piped()` setup
- Drain thread spawn + join logic
- `child.wait()` happy path
- Ring buffer accumulation under real concurrent writes

The two `#[ignore]`'d real-subprocess smoke tests in §5.4.3 do exercise these — but they require external dependencies (Perl bismark2bedGraph on PATH) and are CI-skipped. The plan therefore has **zero CI-running tests that touch `RealRunner`**.

Reviewer-attention magnet §11.6 already acknowledges this with the "fake shell script" alternative. I think this is closer to Important than Optional — the `RealRunner` is precisely where the tricky concurrency lives (drain thread + child.wait), and shipping it untested in CI means regressions land silently.

**Concrete recommendation**: add a non-`#[ignore]` test in `tests/phase_g.rs` that:
1. Generates a tiny shell script in a tempdir (e.g. `#!/bin/sh; echo "noise" >&2; touch "$2"; exit 0`).
2. Marks it executable.
3. Sets `BISMARK_BIN` to the tempdir so discovery finds the script.
4. Calls `RealRunner::run` directly (not through `run_phase_g_chain`).
5. Asserts the touched file exists AND the `RunOutcome::stderr_tail` contains "noise".

This skips on Windows (where `#!` doesn't work) — gate with `#[cfg(unix)]`. It exercises the actual `std::process::Command` code paths without needing Perl Bismark.

### 4.2 No test asserts argv parity vs Perl for a fixed config (Important)

The user's reviewer guidance asks specifically: "Is there a parity test that asserts Rust's argv matches Perl's argv for a fixed config?"

The plan does NOT have one. The closest tests are golden argv assertions per-config (§5.4.1) but they're checking against the plan's expected Rust argv, not against a captured Perl argv. The plan should add: pick 2-3 representative configs (default, `--CX_context --gzip --cytosine_report --ample_memory`, `--ucsc --remove_spaces --zero_based`), run Perl extractor with `system` traced (or read the Perl source line ranges), and check the Rust `build_*_argv` output against the literal Perl `@args` for the same config. This is the cheapest insurance against "I forgot a flag" regressions.

### 4.3 Drain-thread panic test is missing (Important)

§3.8 says "Drain thread panics: documented as unexpected — drain thread does no fallible work beyond UTF-8 write + Vec push." But UTF-8 write *can* fail (parent stderr closed, e.g. piped to `head -1` then closed). `eprintln!`-style writes panic on EPIPE in some configurations.

§5.4.1 has no test for drain-thread panic. Add: a `RealRunner::run` invocation where `tee_target` is a `Vec<u8>` that returns `Err(ErrorKind::BrokenPipe)` on write. Assert the main thread completes (`child.wait()` returns) and that the resulting error is either an `Ok(RunOutcome)` (drain swallowed the write error) or a clearly-typed `SubprocessSpawnFailed` (drain panicked, was joined, panic was caught).

### 4.4 No test for `BISMARK_BIN` pointing to a non-executable (Important)

§3.2 step 1: "If the file exists and is executable, return it. Otherwise log to stderr… and continue."

§5.4.1's `discover_subprocess_uses_bismark_bin_env_override` only tests the happy path. Add a sibling test where `BISMARK_BIN=/some/dir`, dir contains a non-executable file named `bismark2bedGraph`, AND PATH does NOT contain the tool: assert the function logs the fallback message AND returns `SubprocessNotFound` (since no other path supplies it).

Also test the "exists but not executable" path on Unix specifically: `std::fs::set_permissions(path, Permissions::from_mode(0o644))` then assert. Skip on Windows.

### 4.5 No test for `--parallel N` × subprocess interaction at the runtime layer (Optional → Important)

§5.4.2's `phase_g_parallel_n_is_no_op_on_subprocess_chain` checks argv-equality across `--parallel 1` vs `--parallel 4`. Good. But the *real* concern (mentioned in §1's out-of-scope) is whether the rayon worker pool has fully reduced before `ExtractState::finalize` calls `run_phase_g_chain`. The test as sketched doesn't actually trigger the parallel pipeline (it's a unit-style test calling `run_phase_g_chain` directly with a mock).

Add an integration test (using a real tiny BAM + mock runner) that runs the FULL `--parallel 4` pipeline and asserts: (a) the mock runner is called exactly once per subprocess, (b) the argv positional tail contains the post-merge split files (not the per-worker temp files).

### 4.6 No test for `--mbias_only` short-circuit (Optional)

§3.8 says "double-guard" for `--mbias_only`. Add: `phase_g_skipped_when_mbias_only` integration test that sets `--mbias_only` + (impossible-because-mutex-but-bypassing-validation) `--bedGraph`, calls `run_phase_g_chain`, and asserts the runner is never invoked. The validation already rejects this at the CLI layer, but defensive testing at the unit layer documents the gate.

### 4.7 No test for `OutputFileMap::finalize_with_empty_sweep` returning the right files (Important)

§4.4 changes the return from `()` to `Vec<PathBuf>` of *kept* files. This is the load-bearing data feeding bismark2bedGraph. The plan should add (or call out an addition to) `tests/output_phase_c2.rs`:

- Write to 3 keys, leave 1 untouched (empty), call `finalize_with_empty_sweep`, assert returned Vec has length 3 AND contains the 3 paths AND does NOT contain the swept-empty path.

The §5.4 test list has nothing in this slot.

---

## 5. Alternatives worth considering

### 5.1 Drop the tee complexity for v1.0 — use `Stdio::inherit()` (Alternative, partial-rejection)

The user invited reviewer pushback on the LOC jump (400 → 900). About 250 LOC of that is the tee + ring buffer + drain thread + runner trait + mock infrastructure.

A simpler v1.0 design: `Stdio::inherit()` on stderr; don't capture; on subprocess failure, the error variant is `SubprocessFailed { tool, exit_status }` with NO `stderr_tail`. The user has already seen the stderr live. Downstream tooling that parses error context just reads the bubbled exit status.

Pros:
- ~150 LOC saved.
- No drain thread → no concurrency concerns → no concurrency bugs.
- No mock runner needed for stderr testing.
- Matches what `Stdio::inherit()` does in 95% of production CLI tools.

Cons:
- The bubbled error contains less context for log-parsing tools.
- Mismatches the user's resolved-Critical (TEE was Felix-approved).

The user already chose tee. But I'm flagging this because the LOC jump is non-trivial and a future maintainer asking "why is there a drain thread here" deserves a defensible answer. **Recommendation: keep tee per the user's call, but explicitly note in SPEC §6.6 the maintenance-cost tradeoff so the v1.x decision (rust-bedgraph inline) inherits the analysis.**

### 5.2 Alternative discovery model: configure-time vs runtime (Optional)

Right now `discover_subprocess` runs at every `run_phase_g_chain` call (twice if cytosine_report). Alternative: discover both tools at validation time (`Cli::validate`) and store the resolved `PathBuf` in `ResolvedConfig`. Failure surfaces earlier (before the pipeline runs); the pipeline is more deterministic.

Cost: two more fields in ResolvedConfig (`bismark2bedgraph_path: Option<PathBuf>`, `coverage2cytosine_path: Option<PathBuf>`). Saves nothing perf-wise; gains clearer error timing.

Not worth blocking on; flag for v1.x consideration.

### 5.3 `--rust-bedgraph` deferral — defer it harder (Alternative, no-op recommendation)

§11.4 asks whether to lock the CLI shape now. I think the plan's choice (defer) is right. Adding `--rust-bedgraph` now as a NotYetImplemented stub IS a CLI-shape lock-in cost (you can't change the flag spelling once shipped), but more importantly, the implementation might not want a flag — it might switch on a feature-cargo flag or a runtime env var. Defer wholesale; revisit when the inline implementation lands.

### 5.4 `finalize_with_empty_sweep` return-type change — alternative: separate method (Important)

§11.5 already raises this. My take: the in-place change is acceptable BUT the semantic of "an empty sweep that *also* returns the kept-set" is unfortunate. Naming convention would suggest:

```rust
pub fn finalize_with_empty_sweep(&mut self) -> Result<(), io::Error>;
pub fn kept_paths(&self) -> &[PathBuf];  // new, populated by finalize_with_empty_sweep
```

…or:

```rust
pub fn finalize_with_empty_sweep(&mut self) -> Result<FinalizationReport, io::Error>;
// where FinalizationReport { kept: Vec<PathBuf>, swept: Vec<PathBuf> }
```

The latter is nicer: it gives Phase H/I a richer data feed without a second API change. The plan's in-place `Vec<PathBuf>` works but signals "we'll change the type again". Recommend the struct return; it's a 5-line addition.

---

## 6. Action items

### Critical (must-fix before implementation)

1. **§3.2 vs §10 discovery-order contradiction.** Pick one (recommended: BISMARK_BIN-first, matching §3.2). Update the other plan sections + §11 magnet entry to match. Add a precedence test asserting BISMARK_BIN beats PATH when both contain the tool.

2. **`ExtractState::input_basename()` helper has nothing to read.** Add `input_basename: String` field to `ExtractState`; populate from constructor argument; expose via `pub fn input_basename(&self) -> &str`. Update §2.2 LOC table + §4.5 + §7.1 Write surface.

### Important (should-fix before implementation)

3. **`--buffer_size` Perl-fidelity gap.** When `buffer_size.is_none() && !ample_memory`, decide and document whether Rust pushes `--buffer_size 2G` (Perl-faithful) or omits the flag (subprocess uses its own '2G' default). Recommend Perl-faithful for byte-identity at the argv level. Add a unit test.

4. **`which` crate not yet a workspace dep.** Either commit to adding it (and update §5.2 + §7.1) or switch to inline PATH walking. Don't leave it conditional on impl-time discovery.

5. **No CI-running test touches `RealRunner`.** Add a `#[cfg(unix)]` integration test that uses a tempdir shell-script as a fake subprocess (per §4.1 above). Exercises drain thread + child.wait + stderr capture without external deps.

6. **No argv-parity test vs Perl.** Add 2-3 fixed-config parity assertions: Rust `build_*_argv` output equals (modulo flag-name de-abbreviation) the Perl `@args` for the same config. Cheapest insurance against silent flag drift.

7. **Drain thread not joined on `Ok` path.** §3.4 step 4 only joins on failure. Always join before returning; document drain-thread panic handling explicitly. Add a test for drain-thread panic (write-to-`tee_target` returns `BrokenPipe`).

8. **Ring-buffer "line larger than CAP" behavior under-specified.** Pick a deterministic policy (drain-from-front recommended). Tighten the §5.4.1 test from "some bounded substring" to an exact-length + first-byte assertion.

9. **No test for `BISMARK_BIN` set-but-not-executable / set-but-empty path.** Add. Skip on Windows.

10. **No test for `finalize_with_empty_sweep` return-content correctness.** Add: assert returned Vec contains kept paths and not swept paths.

11. **Smoke tests' "check output exists" assertions are too weak.** Tighten to "open output, read header, assert column count" / "assert non-zero rows".

12. **§3.8 "gigabyte stderr" claim not validated end-to-end.** Add a `RealRunner`-level high-volume-stderr test (cheap shell-script generator).

13. **A3 / LC_ALL pinning is a Phase H pre-req.** Add a one-liner in SPEC §6.6 noting the locale dependency for the byte-identity gate.

14. **`stdin` handling not explicit.** Set `.stdin(Stdio::null())` in `RealRunner::run`; mention in §3.3 + §4.

15. **§4.4 return-type change: prefer a `FinalizationReport` struct over a bare `Vec<PathBuf>`.** Gives Phase H room to add kept/swept/error counts without another API churn.

16. **`coverage2cytosine` un-forwarded flags need a one-line "deliberately omitted" note.** Pre-empts future "why doesn't the Rust port pass --merge_CpGs" tickets.

### Optional (polish)

17. **`--counts` field is dead.** Remove from CLI + ResolvedConfig, or add a "accepted-and-ignored" note.

18. **Rename `phase_g_passes_genome_folder_to_c2c_quoted_via_typed_argv`** to clarify the test asserts the *absence* of shell quoting.

19. **§5.2 step 2 "version bump 1.0.0-alpha.8 → 1.0.0-alpha.9"** — ok, but also bump `description` to mention the subprocess chain explicitly. Already noted in the plan.

20. **§5.4.1 `discover_subprocess_falls_back_to_current_exe_dir` requires symlinking the test binary** — that's awkward in a unit-test context. Consider testing the `current_exe()`-fallback function in isolation by injecting a `current_exe()`-stub (e.g. an `impl` of a `BinaryLocator` trait). Otherwise this test will be platform-flaky.

21. **§5.4.1 `discover_subprocess_finds_on_path_when_present`** — `which` crate behaviour on a tempdir PATH prefix is mostly reliable but verify the test handles macOS's PATH munging by SIP/Apple-Silicon shells.

22. **`SubprocessTool` Display impl returns the script name** — confirm casing (`bismark2bedGraph` has lowercase b and lowercase b-graph G; commonly typoed). Add a `Display` round-trip test.

23. **Plan §5.4.2 has 12 named tests; §9.2 says "~12 integration tests".** Plan §5.4.1 has 23 named tests; §9.1 says "~25 unit tests". Counts mismatch by small numbers — reconcile or note the rounding.

24. **§7.3 "Captured `--counts` flag forwarding": "Captured" is the wrong word.** It's "comments out" or "skips". Wordsmith.

25. **Test naming consistency**: `phase_g_*` test names vary in length/specificity (`phase_g_runs_both_tools_in_order_when_cytosine_report_set` is 9 words; `phase_g_subprocess_failed_first_tool_does_not_run_second` is 7). Tighten.

---

## 7. Overall verdict

**NEEDS-REVISIONS.**

The plan is well-structured, accurately mirrors the Perl source (I verified §2.4.1 and §2.4.2 against `bismark_methylation_extractor:320-428` and the flag matrix against `bismark2bedGraph:637-651` + `coverage2cytosine:2011-2029`), and the resolved Critical (tee + ring buffer) is sound in design. The Perl-isms it catches (long-form vs abbreviated flags, no `--counts` to b2bg, no `--gzip` to b2bg, the always-gzip-cov-output behaviour) are correct.

But it ships two internal contradictions (discovery order, missing input_basename plumbing) that block clean implementation; several test-coverage gaps that risk silent regressions in CI (no `RealRunner` exercise, no argv-parity vs Perl, no drain-thread-failure test); and a Perl-fidelity gap on `--buffer_size` defaulting that could trip the Phase H byte-identity gate at the *argv* layer.

None of the Critical/Important items require structural plan changes — they're additions and reconciliations to an otherwise solid plan. A rev 1 folding the items above should be straightforward; recommend a 2-3 hour pass before implementation triggers.

---

**Report path:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_PHASE_G_A.md`
