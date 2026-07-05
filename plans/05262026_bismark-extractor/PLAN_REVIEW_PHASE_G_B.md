# Plan Review — Phase G (`--bedGraph` + `--cytosine_report` subprocess chain)

**Reviewer:** B (independent)
**Plan under review:** `plans/05262026_bismark-extractor/PHASE_G_PLAN.md` rev 0 (2026-05-27)
**Commit base:** `4e5c691` (Phase C.2 merge)
**Sources cross-checked:** `bismark_methylation_extractor:320-428`, `bismark2bedGraph:30,196-206,420-451,630-651`, `coverage2cytosine:2005-2030`, `rust/bismark-extractor/src/{cli,error,state,output}.rs`, `rust/bismark-extractor/SPEC.md §6.6`.

The plan is solid overall: subprocess argv matrix matches Perl line-by-line, mutex preconditions are already enforced upstream, tee+ring-buffer architecture is reasonable, and the `finalize_with_empty_sweep` return-type change is a clean way to surface the kept set. Findings below focus on what I think is *missing* or *under-specified*, not on what is correct.

---

## 1. Logic review

### 1.1 `discover_subprocess` ordering does not match the plan's narrative

§3.2 step list says (1) `BISMARK_BIN`, (2) PATH, (3) `current_exe()` parent. But §10's "Open" table row "Subprocess discovery order" describes the order as **"PATH → `BISMARK_BIN` env → `current_exe()` parent"**. The same disagreement is embedded in §3.2 step 1's "log to stderr `BISMARK_BIN set but {path} not found/executable; falling back to PATH` and continue" — fallback to PATH only makes sense if PATH is tried *after* `BISMARK_BIN`, which agrees with §3.2 step 1 but contradicts §10. One of the two needs to be corrected; the §3.2 ordering (BISMARK_BIN first, explicit-override-wins) is the more defensible choice and is what reviewer comments below assume.

### 1.2 Subprocess CWD interacts with `--dir` semantics in a load-bearing way

Plan §3.3 says CWD is inherited (matches Perl `system()`). But Perl runs the extractor from whatever directory the user invoked it from; bismark2bedGraph itself then `chdir $output_dir` at line 142-144 of `bismark2bedGraph`. The Perl chain *relies* on this internal chdir to resolve the kept-file paths in `@sorting_files`, which Perl passes as bare basenames after stripping `$output_dir`. Plan §2.4.1's transcription **omits** the path-stripping step that Perl applies (the `@sorting_files` array contains paths created earlier without the `$output_dir` prefix — see Perl `:155-170` where `$sorting_files[$index]` is built from `$tempfile_basename` only).

**Concrete question:** does `kept_split_files` (the `Vec<PathBuf>` from `finalize_with_empty_sweep`) contain absolute paths, paths relative to `output_dir`, or bare basenames? Looking at `output.rs:256-291`, the entries are constructed from `self.files.drain()` whose `path` field is built at `OutputFileMap::new` time — almost certainly *full paths* (output_dir joined with basename), based on `eprintln!("{path_str} contains data ->\tkept")` matching Perl's full-path emission. If those full paths are then passed to bismark2bedGraph as positional args, bismark2bedGraph's `chdir $output_dir` followed by attempting to open a path *containing* `$output_dir` will:

- On a relative `--dir`, succeed only because the path is absolute.
- On `--dir .` with no input_dir prefix-stripping inside bismark2bedGraph, this gets stranger: `bismark2bedGraph:77-89` actually has special handling: if `$filename =~ /^(.*\/)(.*)$/`, it splits and `chdir`s — meaning bismark2bedGraph *expects* absolute or directory-prefixed input paths and copes with them.

This works, but the plan should explicitly document **which form `kept_split_files` takes** and pin a test that asserts the positional-tail elements are the same paths the user/Perl would see. As-is, an impl could regress to passing bare basenames (matching Perl's `$sorting_files[$index]` literally) and silently break.

### 1.3 `derive_bedgraph_filename` strip order — `.gz` is removed only when terminal, but `foo.bam.gz` exists

Plan §2.4.6 transcribes the Perl regex as: strip trailing `gz`, then `sam`, then `bam`, then `txt`, then append `bedGraph`. Walking through `foo.bam.gz`:
- `s/gz$//` → `foo.bam.`
- `s/sam$//` → `foo.bam.` (no match)
- `s/bam$//` → `foo.bam.` (no match — there's a trailing `.` between `bam` and the now-removed `gz`)
- `s/txt$//` → `foo.bam.` (no match)
- `s/$/bedGraph/` → `foo.bam.bedGraph`

So the Perl-correct output is `foo.bam.bedGraph` — the trailing dot is preserved (the regex strips literal `gz`, not `.gz`, so the dot stays). Same trap for `foo.txt.gz` → `foo.txt.bedGraph`. The plan's §2.4.6 prose says "strip trailing `.gz`, `.sam`, `.bam`, `.txt`" — note the leading dots. This is **wrong** relative to what Perl does. The unit test list (§5.4.1) includes `foo.bam.gz` and `foo.txt.gz` as cases but doesn't pin the expected output; the implementer could pick either interpretation. Pin the goldens in the plan, and make sure the Rust impl strips the **literal suffix without the dot** to match Perl exactly. This matters because Phase H byte-identity will compare the resulting bedGraph filename against Perl's emission.

### 1.4 Order-of-operations: empty-sweep runs *before* splitting-report and M-bias writes today

`state.rs:111-145` currently runs in this order: flush_all → finalize_with_empty_sweep → write_splitting_report → write_mbias_txt. Plan §4.5 says the chain runs "after M-bias write, BEFORE returning Ok(())". That is correct, but the kept-file `Vec<PathBuf>` needs to be captured from `finalize_with_empty_sweep`'s return at step 2 and *held* through steps 3 + 4 to feed step 5. The plan's snippet at §5.5 says `let kept_files = if !self.mbias_only { self.fhs.finalize_with_empty_sweep()? } else { Vec::new() };` but doesn't say this binding happens inside `ExtractState::finalize`. Since `finalize` currently takes `&mut self`, holding the `Vec<PathBuf>` across the subsequent calls is fine — but the plan should make the lexical scope explicit (the kept-set lives the rest of `finalize`'s body) to avoid an impl that splits it into a helper and loses the data.

### 1.5 `kept_split_files` ordering — does it match Perl's `@sorting_files`?

Perl `@sorting_files` has a *defined order*: CpG_OT, CpG_OB, CpG_CTOT, CpG_CTOB, CHG_OT, CHG_OB, CHG_CTOT, CHG_CTOB, CHH_OT, CHH_OB, CHH_CTOT, CHH_CTOB (constructed by nested context×strand loops at `bismark_methylation_extractor:127-145` or thereabouts; the order matters because `bismark2bedGraph` concatenates files in argv order before sorting, which doesn't matter for correctness but DOES matter for ASCII-identity of the intermediate sort input). `OutputFileMap::finalize_with_empty_sweep` today uses `self.files.drain()` — a `HashMap` drain, which is **unordered**. Plan §4.4 says the return type becomes `Vec<PathBuf>`, but if the iteration order is HashMap-random, `kept_split_files` will be non-deterministic across runs.

Implications:
- bismark2bedGraph internally sorts on chromosome+position before emission, so the bedGraph and `.bismark.cov.gz` outputs are deterministic regardless of input file order.
- But the kept-set `eprintln!("{path_str} contains data ->\tkept")` lines in §3.4 of finalize are *already* non-deterministic post-C.2 — verify whether C.2 tests assert any specific stderr line order. If not, Phase G needs to either (a) collect into a `BTreeMap` or sort the Vec before returning, or (b) accept non-determinism but document it.

**Recommendation:** sort `kept_split_files` by path before returning (deterministic argv → easier debugging + clean diff against Perl invocations).

### 1.6 `child.stdout` is left as `Stdio::inherit` but never explicitly set

Plan §3.3 says "stdout: inherited". `std::process::Command` defaults to `Stdio::inherit()` *only* when `stdin/stdout/stderr` are not touched — but once you call `.stderr(Stdio::piped())`, the other two stay at their defaults (inherit). This is fine, but the plan should call out explicitly that `Command::new(program).args(argv).stderr(Stdio::piped()).spawn()` is the production pattern; an impl could accidentally `.stdout(Stdio::null())` to "be tidy" and lose subprocess progress. Add a code-snippet showing the exact Command setup in §3.4.

### 1.7 Drain thread can deadlock on full pipe before main thread joins it

Plan §3.4 step 4 says "Main thread calls `child.wait()`; on success, returns `Ok(())`. On non-zero exit, joins the drain thread". On a *successful* exit, the drain thread is NOT explicitly joined. This is unsound:

1. Subprocess writes a final stderr burst then exits zero.
2. Some of that burst is still in the OS pipe buffer (drain thread hasn't read it yet).
3. `child.wait()` returns immediately on the zero exit.
4. Main thread returns `Ok(())`, dropping the drain thread's `JoinHandle` without joining.
5. The drain thread continues running, racing against the parent's own stderr output. Final stderr lines may print *after* "Finished BedGraph conversion" log lines emitted by Rust.

Even on success, the drain thread should be joined. Otherwise the live-tee UX has races. The fix is trivial: always `handle.join()` before returning — but the plan must say so. This is also relevant on Err paths where the user expects to see the full stderr live (not just the snapshot).

### 1.8 Pipe-buffer-full deadlock if subprocess writes >64 KiB before drain starts

User-question 2 in the prompt. POSIX pipe buffer is 64 KiB on Linux, 16 KiB on macOS. If the subprocess writes 100 KiB of stderr in a tight burst before the drain thread is scheduled, the subprocess blocks on `write(2)` waiting for the pipe to drain. If the main thread is also blocked in `child.wait()` and the drain thread hasn't started yet, the main thread waits forever for `child.wait()` while the child waits forever for pipe drain. The plan's "start drain thread immediately after spawn" handles this *if the drain thread is scheduled promptly*, but adversarial scheduling could still cause issues.

**Mitigation:** spawn the drain thread *before* the main thread calls `child.wait()`. The plan implies this but doesn't state it. Add an explicit ordering note: "thread::spawn must execute *before* child.wait()". Add a unit test using a fake subprocess that writes >128 KiB to stderr in a single burst and exits — verify the parent doesn't deadlock.

### 1.9 `current_exe()` under `cargo test`

Prompt's user-question 3. Confirmed behavior: `std::env::current_exe()` returns the test binary in `target/debug/deps/<test-name>-HASH`. The plan's discovery fallback (`current_exe()?.parent()?.join("bismark2bedGraph")`) would look in `target/debug/deps/` for `bismark2bedGraph` — which won't exist. Under normal `cargo test`, this falls through to step (4) and returns `SubprocessNotFound`, which is what the discovery unit tests expect.

**But:** the discovery test `discover_subprocess_falls_back_to_current_exe_dir` (§5.4.1) says "symlink the test binary alongside a fake tool". Symlinking the test binary itself is non-trivial because (a) `current_exe()` may canonicalize, (b) the test process holds an open file handle on its own exe so the symlink path's parent dir is the deps/ directory under cargo's lock. The test design is brittle. Suggest replacing with a temp-dir test that fakes a `current_exe` by setting an env-var override (e.g. `BISMARK_TEST_CURRENT_EXE_DIR=<path>`) consulted only under `#[cfg(test)]` — keeps the impl pure but makes the test deterministic.

### 1.10 `BISMARK_BIN` partial-match logic

Plan §3.2 step 1 says: if `BISMARK_BIN` is set but `<BISMARK_BIN>/<tool>` is not executable, log and fall through to PATH. This is a *silent permissive fallback* — if the user sets `BISMARK_BIN=/opt/old-bismark/` intending to pin a specific version, and a typo means the binary is at `/opt/old-bismark/bin/bismark2bedGraph`, the discovery silently falls back to PATH and runs whichever bismark2bedGraph happens to be there. This is the opposite of what most env-var pinning is for. Two reasonable behaviors:

- **Strict:** if `BISMARK_BIN` is set and the tool isn't there, **fail** with `SubprocessNotFound { tool, searched_paths: [bismark_bin_path] }`. The user explicitly asked for that directory.
- **Permissive (plan default):** fall through with a warning. Easier UX for partial-install dirs.

Pick one and document the rationale. Strict is safer for reproducibility. The plan's edge-case row "BISMARK_BIN set to a path containing only one tool" (§3.8) tacitly endorses permissive but doesn't explain why.

### 1.11 `--parent_dir` value in plan §2.4.2 matches Perl but plan does NOT explicitly include it in the new Rust argv builder description

Verified Perl `:404`: `push @args, "--parent_dir '$output_dir'"`. So Rust's `build_coverage2cytosine_argv` MUST push `--parent_dir <output_dir>`. The plan's §2.4.2 transcribes this correctly, but §4.3's function signature for `build_coverage2cytosine_argv` doesn't take `output_dir` as a separate "for the `--parent_dir` flag" param — it shows only one `output_dir: &Path` argument used for `--dir`. The plan is implicitly saying `--dir` and `--parent_dir` get the same value (which is what Perl does). Confirm this is intentional and add an explicit assertion in the c2c-argv unit tests:
- `build_coverage2cytosine_argv_passes_parent_dir_equal_to_dir`.

### 1.12 Auto-trigger gate when ONLY `--cytosine_report` is set + kept-set is empty

Plan §3.8 row "ALL split files swept empty" says "pass through to bismark2bedGraph anyway". Good. But what if then **`--cytosine_report` also set**? bismark2bedGraph produces an empty `.bismark.cov.gz`; coverage2cytosine reads it and traverses the entire genome producing a CpG_report.txt with all-zero counts. On a 3 GB genome this is a non-trivial run that produces useless output. Is this the desired UX, or should the chain short-circuit when kept-set is empty?

The plan defers to "let the subprocess decide". Important to surface explicitly in user docs (or stderr warning at chain entry): "extractor produced no methylation calls; chain will run anyway and may take significant time on the empty cov file". Cheap mitigation: an info-level eprintln before the c2c spawn when kept-set was empty.

### 1.13 SubprocessFailed `stderr_tail` type

Prompt's question 8. `String` (lossy UTF-8) vs `Vec<u8>` (preserves binary). Perl's subprocess output is plain ASCII English (`warn` statements), so the practical difference is near-zero. But `String::from_utf8_lossy` on a partial-UTF-8 sequence at the start of the snapshot (because the eviction policy cut a multi-byte codepoint in half) produces `U+FFFD` replacement chars. The ring buffer eviction is by *line* per §3.4 step 3 (`read_line` then push), so eviction always happens at line boundaries unless a single line is bigger than `CAP` (handled by test `ring_buffer_handles_lines_larger_than_capacity`). For that single-huge-line case, the eviction WILL cut mid-codepoint, producing `U+FFFD`.

**Recommendation:** store `Vec<u8>` internally; convert to `String::from_utf8_lossy` only at `Display`/error-formatting time. This way downstream tooling that wants to grep the raw stderr can recover. Low cost; future-proofs against subprocess updates that emit UTF-8 multibyte glyphs (e.g. progress bars).

### 1.14 Drain thread's `read_line` behavior on non-UTF-8 bytes

`BufReader::read_line` reads bytes into a `String`, which **errors** on non-UTF-8 input (returns `Err(InvalidData)`). The plan says "drain thread does no fallible work beyond UTF-8 write + Vec push" — but `read_line` IS fallible on non-UTF-8 stderr. If bismark2bedGraph or coverage2cytosine ever emits a non-UTF-8 byte (e.g. via a system error message in a non-UTF-8 locale), the drain thread errors mid-read.

**Fix:** use `BufReader::read_until(b'\n', &mut Vec<u8>)` instead of `read_line`. This is byte-safe and matches the `Vec<u8>` ring buffer suggestion in 1.13. Update §3.4 to use byte-level reads.

### 1.15 "Long-form flag names throughout" — verify against bismark2bedGraph GetOptions

Verified `bismark2bedGraph:637-651`: the spec accepts long forms `'remove_spaces'`, `'zero_based'`, `'CX_context'` (also abbreviated as `CX`), etc. ✓ Plan correct. coverage2cytosine GetOptions at `:2011-2029` also accepts long forms. ✓

### 1.16 `--genome_folder` PathBuf vs the c2c argv string

`ResolvedConfig::genome_folder: Option<PathBuf>`. The c2c argv builder needs to push `--genome <genome_folder>`. Plan §4.3 doesn't show the genome_folder parameter explicitly; it's implicit in `&ResolvedConfig`. The plan should pin a unit test confirming the **OsString round-trip** works on a path containing non-UTF-8 bytes (e.g. on Linux, a path like `b"/tmp/g\xff/"`). Typed argv via `OsString` handles this correctly; a careless `to_string_lossy()` would corrupt the path. Add: `build_coverage2cytosine_argv_preserves_non_utf8_genome_folder`.

---

## 2. Assumptions analysis

### 2.1 A5 — `which` crate availability

Plan A5 says "likely already a workspace dep via bismark-io or bismark-dedup; confirmed at impl-time". This is a *quick check*, not an assumption that needs deferral. Just run `cargo metadata | grep which` (or `rg "which =" rust/`) now and put the answer in the plan. As a reviewer I'd prefer a definitive answer in rev 1 rather than "we'll find out".

### 2.2 A6 — Windows out of scope

The plan locks Windows out for `current_exe()`. But does the crate **compile** on Windows? If not, the workspace's CI matrix needs to exclude Windows for bismark-extractor specifically. Verify (a) what targets workspace CI tests, and (b) whether `Stdio::piped()` + drain thread compile on Windows (they do — `std::process` is cross-platform). The real Windows gap is more nuanced: `BISMARK_BIN` discovery should append `.exe`, `which::which` does that automatically, but the discovery test `discover_subprocess_finds_on_path_when_present` needs to be Linux/macOS-cfg-gated if it uses a non-`.exe` fixture script.

### 2.3 A7 — UTF-8 lossy on stderr

See 1.13. The assumption "subprocess source confirms stderr is plain ASCII English text" is true today, but UTF-8 lossy is the wrong defensive choice if the cost of `Vec<u8>` is identical.

### 2.4 Implicit assumption: bismark2bedGraph's `--output` value is a *filename*, not a path

Perl `:367` pushes `--output $bedGraph_output` where `$bedGraph_output` is a *bare filename* (no directory). The `--dir` flag tells bismark2bedGraph where to write it. Plan §4.3's function signature takes `bedgraph_filename: &str` — interpreted as a filename only. Add an explicit assertion: `bedgraph_filename` must NOT contain `/` (debug_assert), or document that the implementer is responsible for filename derivation. Otherwise an impl could pass `output_dir.join(bedgraph_filename)` and emit a double-dir invocation.

### 2.5 Implicit assumption: kept_split_files paths are still valid by the time bismark2bedGraph runs

If the subprocess chain runs **after** `finalize_with_empty_sweep`, the kept files are still on disk (only the empty ones got removed). Good. But what if a concurrent process deletes them? Not a realistic concern for v1.0 but worth a one-line "no inter-process consistency guarantees" note in §8.

---

## 3. Efficiency analysis

### 3.1 `Mutex<RingBuffer>` lock contention — not a concern (because there is no Mutex)

The drain thread is the **sole writer** to the ring buffer; the main thread reads it only after `child.wait()` returns and the drain thread joins. So no Mutex is needed — the ring buffer can be owned by the drain thread and *moved* back via `JoinHandle::join()`'s returned value. Plan §3.4 doesn't actually require a Mutex — but the section's phrasing "accumulating a bounded tail" could lead the implementer to wrap it in `Arc<Mutex<RingBuffer>>` reflexively. Add a note: "Ring buffer is owned exclusively by the drain thread; returned via `JoinHandle::join()`. No Mutex needed."

### 3.2 64 KiB ring buffer eviction via `Vec::drain` from front is O(n)

Each line append could trigger `Vec::drain(0..delta)` which is O(remaining_len). For typical stderr volumes (<1 MiB total over a run), this is negligible. For pathological cases (gigabyte stderr → ~16,000 evictions each shifting ~64 KiB) you'd be O(n²). Realistic stderr is small, so this is fine — but a `VecDeque<u8>` would be O(1) per eviction and ~zero code change. Mention as a §6 efficiency note.

### 3.3 Drain thread per invocation

Two threads spawn-and-join per `--cytosine_report` run. Cheap (≤ 1 ms each). Not a concern.

### 3.4 Mock runner overhead

§5.4.2 uses `MockRunner` for 12 integration tests. Trait-object dispatch is dwarfed by argv-construction; no concern.

---

## 4. Validation sufficiency

### 4.1 Missing: end-to-end test of `RealRunner` (no fake-shell-script test)

Prompt's question 5. The unit tests in §5.4.1 cover the ring buffer + discovery, and §5.4.2's MockRunner exercises argv-building. But the **integration between** `std::process::Command::spawn`, the drain thread, the tee, and the ring buffer is exercised **only** by §5.4.3's `#[ignore]`'d real-subprocess smoke tests — which require Bismark Perl installed.

Recommendation: add a `tests/phase_g_real_runner.rs` that uses a tiny shell-script fixture (`#!/bin/sh; echo "line1" >&2; echo "line2" >&2; exit 0`) checked into `tests/fixtures/fake_bismark2bedgraph.sh`. The test invokes `RealRunner::run` against the script directly (bypassing `discover_subprocess`) and asserts (a) the tee target captured both lines, (b) the ring buffer snapshot contains both, (c) exit_status is success. A second variant exits non-zero and asserts `SubprocessFailed` bubbles with the right tail. This costs <50 LOC and exercises the actual production code path without depending on Perl Bismark.

### 4.2 Missing: parity test "Rust argv == Perl argv for a fixed config"

Prompt's question 5. The plan has *unit* tests for argv-builders (`build_bismark2bedgraph_argv_default_no_optional_flags`, etc) but no end-to-end "given this ResolvedConfig, the argv string matches what `bismark_methylation_extractor` would emit verbatim". A regression test of this shape would catch a future Phase F/G refactor that drops a flag.

Concrete proposal: a golden-file test under `tests/fixtures/phase_g_argv/`. Fixture per scenario: `{stem}.toml` with the input config + `{stem}.b2bg.argv` + `{stem}.c2c.argv`. The Rust test loads the config, builds argv, and `assert_eq!`s against the golden file. Goldens are generated initially by running real Perl with `system` replaced by `print join " ", @args; exit;` — a one-time scaffold step.

### 4.3 Missing: empty-kept-set integration test

§3.8 row "ALL split files swept empty" claims defensive behavior but no test pins it. Add: `phase_g_runs_bismark2bedgraph_with_empty_kept_set` — mock runner receives an argv whose positional tail is empty.

### 4.4 Missing: stderr live-tee assertion under integration tests

§5.4.2's integration tests use MockRunner, which doesn't exercise the tee path. The only `RealRunner` tee assertion is §5.4.3 (#[ignore]'d). Add a unit test under `src/subprocess.rs::tests` that uses `RealRunner` against a captive fake-shell-script (see 4.1), asserting both the tee target AND the ring buffer received the data.

### 4.5 Missing: assertion that drain thread is joined on success

Per finding 1.7. Add a `RealRunner` test that runs a subprocess emitting a *final* stderr line right before exit; assert the line appears in *both* the tee target and the ring buffer (i.e. drain didn't return early before reading the last line).

### 4.6 Missing: argv parity test for `--no_header` placement

Verify that `--no_header` is passed to bismark2bedGraph only when both extractor's `--no_header` AND `config.bedgraph` are set, AND that the extractor's `no_header` field flows through to the subprocess (not into a Rust-only flag that gets discarded). `ResolvedConfig::no_header` is the source of truth. Add: `build_bismark2bedgraph_argv_passes_no_header_when_set`.

### 4.7 No test for the `parent_dir == output_dir` invariant

See 1.11.

### 4.8 No test for `OutputFileMap::finalize_with_empty_sweep` returning kept-files in a stable order

See 1.5. If the plan accepts the recommendation to sort, add a test: `finalize_with_empty_sweep_returns_sorted_kept_paths`. If it doesn't, document the non-determinism.

### 4.9 `tests/phase_g_real.rs` opt-in mechanism

Prompt's question 5. `cargo test -- --ignored` runs ALL ignored tests across the crate — not selective. If other unrelated `#[ignore]`'d tests exist or are added later, opt-in would spuriously run those too. A `cfg(feature = "real-subprocess-tests")` feature gate is more selective. Or `#[ignore = "..."]` with a custom test runner attribute. For v1.0 just adding a separate test binary (`phase_g_real.rs`) named so that `cargo test -p bismark-extractor --test phase_g_real -- --ignored` selects it specifically is probably the minimum-effort path. The plan should state the opt-in invocation explicitly.

### 4.10 Phase F worker-count interaction test

§5.4.2 lists `phase_g_parallel_n_is_no_op_on_subprocess_chain`. Good. But this test asserts argv equality — it doesn't assert *order*: the subprocesses must run AFTER all workers have reduced and flushed. The order is enforced by `state.rs::finalize` calling the chain after `flush_all`, but there's no test that pins "if a worker is still writing to a split file at chain-spawn time, that's a bug". A simple way to test: parameterize the integration test with a single-record fixture and assert the kept-file mtime is before the subprocess-mock invocation timestamp. Probably overkill, but consider.

---

## 5. Alternatives

### 5.1 Consider chdir-into-output_dir before spawn

Plan §3.3 inherits CWD. Alternative: `Command::current_dir(&config.output_dir)`. Pros: bismark2bedGraph already chdir's into `$output_dir` itself; matching the parent's CWD eliminates a class of relative-path bugs. Cons: changes a subtle UX (`pwd` inside the subprocess is now `output_dir` instead of wherever the user ran the extractor from). The Perl-parity argument is "inherit". Acceptable as-is; document the choice.

### 5.2 Single-thread tee via select()/poll() instead of dedicated drain thread

A `std::process::Stdio::piped()` stderr can be read on the main thread using non-blocking reads + `poll`. Avoids the thread spawn. But Rust's stdlib doesn't expose a portable non-blocking pipe-read; you'd need `nix` or `mio`. The drain-thread approach is simpler and idiomatic. Plan's choice is right.

### 5.3 Alternative: emit subprocess command line to stderr before spawn

A debugging affordance: `eprintln!("$ bismark2bedGraph {}", argv.join(" "))` before `child.spawn`. Matches `bash -x` semantics. Costs nothing, helps debugging when subprocess fails. Plan doesn't mention; consider adding behind a `BISMARK_TRACE=1` env var or always-on (it's already on stderr; trivial).

### 5.4 Alternative: deferred BISMARK_BIN strict-vs-permissive

See 1.10. Could use a third mode: `BISMARK_BIN=/path/` → strict (fail if tool not there), `BISMARK_BIN_PREFER=/path/` → permissive (try, fall back). Probably over-engineered for v1.0.

### 5.5 Alternative for `finalize_with_empty_sweep` return-type change

Reviewer-attention magnet #5 in §11. Alternative: keep `finalize_with_empty_sweep` returning `()`, add a separate `kept_files() -> &[PathBuf]` accessor on `OutputFileMap` that the chain calls after finalize. But this requires `OutputFileMap` to retain the kept-set after `drain()` consumed it. Could keep a `kept: Vec<PathBuf>` field that's populated alongside the drain. This is arguably cleaner than the return-type change because the data is named ("kept files post-sweep") rather than implicit in a Vec. Worth weighing.

### 5.6 Alternative: pass kept files via env var or temp file to subprocess

Mentioned only to dismiss: Perl uses positional argv; matching Perl is right.

---

## 6. Action items

### Critical (block implementation)

1. **Fix `derive_bedgraph_filename` strip semantics** (§1.3). The plan's prose says "strip `.gz`, `.sam`, `.bam`, `.txt`" but Perl strips literal `gz`, `sam`, `bam`, `txt` (no leading dot). Pin the goldens for `foo.bam.gz` → `foo.bam.bedGraph` (trailing dot preserved between `bam` and `bedGraph`) in §2.4.6 + §5.4.1 unit test. Without this, Phase H byte-identity will fail on any input with a chained extension.

2. **Pin `kept_split_files` form (absolute paths? basenames?)** in §4.4 (§1.2). The implementer needs to know whether `OutputFileMap::finalize_with_empty_sweep` returns absolute paths (matches what `eprintln!` currently emits) or basenames (matches what Perl's `@sorting_files` contains). The bismark2bedGraph internal chdir means *either* works, but the argv-parity test (§4.2) and Phase H byte-identity will diverge if Rust's positional tail differs from Perl's. Recommend absolute paths + document.

3. **Always join the drain thread**, including on the success path (§1.7). On `Ok` exit, dropping the `JoinHandle` without joining races the drain against subsequent parent stderr output. Update §3.4 step 4 to "Main thread calls `child.wait()`. Regardless of exit status, join the drain thread. Then return Ok or SubprocessFailed."

4. **Use `read_until(b'\n', &mut Vec<u8>)` not `read_line`** in the drain thread (§1.14). `read_line` errors on non-UTF-8 input, killing the drain thread mid-run. Defer UTF-8 lossy conversion to error-display time.

### Important (resolve before/during implementation)

5. **Discovery ordering disagreement** between §3.2 (BISMARK_BIN first) and §10 (PATH first). Pick BISMARK_BIN-first (explicit env override semantics) and update §10 row.

6. **BISMARK_BIN strict-vs-permissive** decision (§1.10). Document the choice explicitly with rationale. Recommend strict: if BISMARK_BIN is set, the tool MUST be in there, else `SubprocessNotFound`. Edge-case row in §3.8 ("partial BISMARK_BIN") becomes "error" not "fallback".

7. **Stable order of `kept_split_files`** (§1.5). Either sort by path before returning, or document non-determinism + verify no test depends on sweep stderr line order.

8. **`stderr_tail` should be `Vec<u8>`** internally (§1.13). Convert to lossy String only at `Display` time. Update §3.5 error variant signature.

9. **Spawn drain thread BEFORE `child.wait()`** explicitly (§1.8). Add ordering note to §3.4. Add unit test: subprocess writes 128 KiB stderr-burst then exits; parent doesn't deadlock.

10. **Add fake-shell-script RealRunner test** (§4.1). Without it, the actual `Command::spawn` + tee + ring-buffer integration is only covered by `#[ignore]`'d tests. Recommend `tests/fixtures/fake_bismark2bedgraph.sh` and a non-ignored test that uses `RealRunner` directly. ~50 LOC.

11. **Add Rust↔Perl argv parity test** (§4.2). Golden-file test per scenario, generated once from real Perl with a print-and-exit shim.

12. **Add `--parent_dir == --dir` invariant test** for c2c argv (§1.11).

13. **`current_exe()` test design is brittle** (§1.9). Replace symlink-based test with an env-var-override hatch consulted only in `#[cfg(test)]`.

14. **Empty-kept-set integration test** (§4.3). Pin the §3.8 "let subprocess decide" claim with `phase_g_runs_bismark2bedgraph_with_empty_kept_set`.

15. **`which` crate availability check** — answer in rev 1, not at impl time (§2.1).

16. **Opt-in mechanism for `tests/phase_g_real.rs`** (§4.9). `cargo test -- --ignored` is not selective. State the exact invocation (`cargo test -p bismark-extractor --test phase_g_real -- --ignored`) in §5.4.3 + §5.8.

17. **`input_basename` storage on `ExtractState`** — `state.rs:49` doesn't currently store `input_basename` as a field; it's only used at construction for the splitting-report path. Plan §4.5 says "new (small) helper" but doesn't say whether the helper *recomputes* from `input_path` or whether `ExtractState` grows a new field. Specify the choice and add it to the LOC estimate.

18. **Empty kept-set + --cytosine_report UX warning** (§1.12). Document the "runs entire genome on empty cov" failure mode. Recommend a `eprintln!` info-warning at chain entry when kept-set is empty.

### Optional (polish)

19. **`VecDeque<u8>` instead of `Vec<u8>`** for the ring buffer (§3.2). O(1) eviction; trivial code change.

20. **Document explicitly: ring buffer is owned by drain thread, no Mutex** (§3.1).

21. **Always-on `eprintln!` of the subprocess command line** before spawn (§5.3). Cheap debugging affordance.

22. **Spell out the `Command::new(program).args(&argv).stderr(Stdio::piped()).spawn()` shape** in §3.4 to prevent accidental `Stdio::null` on stdout (§1.6).

23. **Non-UTF-8 path round-trip test** for `--genome_folder` (§1.16). Linux-only, fine to cfg-gate.

24. **`BISMARK_BIN` name vs alternatives** (`BISMARK_HOME`, `BISMARK_PATH`) — §10 lists this as an open question. Suggestion: pick `BISMARK_BIN` (matches `*_BIN` convention for binary install dirs); ship it.

25. **LOC estimate** — plan §2.2 says 895 LOC total. Findings 1, 2, 3, 4, 10, 13, 17 (and the additional tests in 4.x) push this up another ~150-200 LOC. Update §10 row G estimate to ~1000-1100.

26. **Document the Vec sort in `finalize_with_empty_sweep`** at the doc-comment level (per finding 1.5 recommendation).

27. **Add a `SubprocessTimeout` error variant?** — out of scope for v1.0 but worth a `// TODO(v1.x): timeout wrapper` comment. Long-running bismark2bedGraph on a hung NFS mount currently has no timeout.

28. **Module doc** for `src/subprocess.rs` should include the byte-identity invariant: "argv passed to Perl bismark2bedGraph from Rust MUST match Perl extractor's argv byte-for-byte (modulo long-form flag expansion documented in SPEC §6.6 / Phase G plan §2.4.4)".

---

## Overall verdict

**NEEDS-REVISIONS**

Four Critical findings (filename derivation correctness, kept-file path form, drain thread join-on-success, byte-safe stderr reads) each independently could cause silent byte-identity failures in Phase H or subtle production bugs. None require rethinking the architecture — the plan's shape is good and the Perl-parity analysis is largely correct. With Crit 1-4 fixed and Important 5-11 addressed, this plan should approve cleanly on rev 1. The "Reviewer-attention magnets" the author surfaced in §11 are mostly polish; the more material issues are deeper in the tee+ring-buffer mechanics and the kept-file plumbing.
