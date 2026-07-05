# Code Review — Phase G (Reviewer B)

**Branch:** `extractor-phase-g` off `4e5c691`
**Plan:** `PHASE_G_PLAN.md` rev 2
**Verdict (preview):** APPROVE-WITH-NITS

---

## Summary

Phase G ships the Perl subprocess chain (`bismark2bedGraph` + `coverage2cytosine`) with a clean trait-based runner, tee/ring stderr handling, BISMARK_BIN-first strict discovery, and trailing-dot filename quirks pinned via 13 inline goldens. 299 tests pass; clippy clean. The implementation hews very close to the rev-1 plan and the deviations are documented sensibly.

This review focuses on the angles assigned (concurrency, OS-edge, security, sort-stability, error propagation, lifetime/forbid). I confirmed several non-issues (mutex isolation, lifetime safety, filename-derivation against actual Perl runs) and surfaced four design concerns worth tracking, two of which I recommend addressing pre-merge.

No Critical findings. 2 High, 4 Medium, several Low.

---

## Issues by area

### Logic

**H1 (High) — Sort order vs Perl's argv: kept files are NOT in Perl's `@sorting_files` push order.**
File: `src/output.rs:321` + `src/state.rs:172`. The Phase G plan rev 1 I7 calls for deterministic ordering; the implementation sorts `kept` lexicographically. Perl, however, pushes to `@sorting_files` in the **file-open order** at `bismark_methylation_extractor:5156, :5179, :5202, :5225, ...` — a deterministic but **non-lexicographic** order (CpG_OT, CpG_CTOT, CpG_CTOB, CpG_OB, CHG_OT, CHG_CTOT, CHG_CTOB, CHG_OB, CHH_*…). For `bismark2bedGraph` the order doesn't affect output (it merges + sorts internally), so this is benign for Phase H byte-identity. But the SPEC and module docstring claim "byte-for-byte argv parity modulo long-form flag expansion" — strictly that claim is **violated** for the positional file tail. Recommend either (a) restoring Perl's push-order via a static OutputKey ordering table, or (b) softening the SPEC claim to "argv parity modulo positional-tail file order, which `bismark2bedGraph` is insensitive to."

**H2 (High) — `finalize_with_empty_sweep` aborts mid-loop on first `remove_file` error, leaks remaining file handles.**
File: `src/output.rs:298-304`. The `drain()` iterator already consumed every entry into the loop body; if `remove_file` returns `Err`, the `?` propagates, but the loop body for the remaining entries has already been drained out of `self.files` and their `OutputFileEntry` values live inside the `entries: Vec` until that vec is dropped. The writers were already explicitly dropped on line 291 (so handles are closed) — but the loop will not log "kept" lines for the unprocessed entries and the `FinalizationReport` returned would only include partial state. Worse: subsequent code in `state.rs::finalize` (splitting-report write, M-bias write, Phase G chain) is skipped entirely because the error propagates up. This is a real partial-output hazard on a (rare) `remove_file` failure (read-only mount, EACCES, file vanished). Recommend: log the failure via `eprintln!` and continue the sweep, returning the report regardless; or at minimum, accumulate the error and continue so the FinalizationReport is complete. (Defensible alternative: this only runs on success path — but a transient FS error here loses the splitting report.)

**M1 (Medium) — Partial bedGraph/coverage files left on disk when `bismark2bedGraph` fails mid-run.**
File: `src/subprocess.rs:524-531`. When `bismark2bedGraph` exits non-zero after creating partial `.bedGraph` and/or `.bismark.cov.gz`, the orchestrator returns `SubprocessFailed` and never invokes `coverage2cytosine` (correct), but the partial files remain in `output_dir`. A re-run will silently overwrite them, so this isn't a correctness bug. But a downstream tool that scans `output_dir` for `.bismark.cov.gz` post-error would see a truncated gzipped file (no valid CRC trailer). Not blocking for v1.0; recommend a doc note in SPEC §6.6 + a Phase H harness check that error-path output_dirs are pruned before assertion.

**M2 (Medium) — `canonicalize`-then-`remove_file` race window on swept paths.**
File: `src/output.rs:297-304`. `canonicalize` is called on the still-existing file (good), but if the file is unlinked between `canonicalize` (line 297) and `remove_file` (line 304) by another process, `remove_file` returns `NotFound` → `?` propagates → sweep aborts (per H2). The window is microseconds and only matters if a watchdog or test harness is racing; in practice unreachable but worth swallowing `NotFound` specifically: `if let Err(e) = remove_file(&path) { if e.kind() != ErrorKind::NotFound { return Err(e); } }`. (Tied to H2.)

**M3 (Medium) — `eprintln!` in drain thread holds parent's stderr lock for the full subprocess lifetime.**
File: `src/subprocess.rs:439-454`. `io::stderr().lock()` is acquired ONCE outside the read loop and held until EOF on child stderr. For long-running subprocesses (multi-hour `bismark2bedGraph` sort on full datasets), this serializes the parent's stderr globally. In v1.0 this is fine because at Phase G time the rayon worker pool has joined — no other thread tries to write to stderr. But if a future panic-hook or signal handler attempts to write to stderr during the drain, it will block. The fix is trivial (lock per line via `writeln!(io::stderr(), ...)` or scope the lock per-line) but the cost is per-line lock contention; the current approach is the principled choice. Recommend: leave as-is, but add a doc note that `RealRunner` holds stderr exclusively while a subprocess runs.

### Errors / Robustness

**M4 (Medium) — `BISMARK_TEST_CURRENT_EXE_DIR` env-hatch is production-active (DEVIATED from plan §3.2).**
File: `src/subprocess.rs:240-251`. The implementation deliberately keeps the hatch always-on (not `#[cfg(test)]`-gated) — deviation #1 in the Implementation Notes — with the rationale "harmless in production." Security note: if an attacker can control the environment of the running process (e.g. a wrapping shell script), they can point `BISMARK_TEST_CURRENT_EXE_DIR` at a directory containing a binary named `bismark2bedGraph` or `coverage2cytosine`. Discovery would fall through `BISMARK_BIN` (not set) and `PATH` (not on PATH), find the planted binary, and spawn it with the parent's full env + inheritance. The attacker already has env control (otherwise the threat doesn't apply), so the practical attack surface is small — they could just set `PATH` instead. But the variable is undocumented in user-facing docs and looks like a `BISMARK_*` blessing of attacker-controlled discovery. Recommend: rename the env var to `BISMARK_TEST_ONLY_CURRENT_EXE_DIR` (signposting it's not for users) **or** gate behind a build feature (`#[cfg(feature = "test-hatch")]`) enabled only in dev-dependencies. The current state is defensible but worth re-deciding.

**L1 (Low) — `SubprocessSpawnFailed` test (`/nonexistent/path/...`) may match a shell-installed error fallback on rare configurations.**
File: `tests/phase_g_realrunner.rs:157-172`. On macOS/Linux, `Command::spawn` on a missing absolute path returns `io::ErrorKind::NotFound` synchronously — no spawn happens. The test passes. But on some exotic shells/runners (containers with custom `execve` shims), the kernel may return EACCES instead of ENOENT. The test only asserts `matches!(err, SubprocessSpawnFailed { tool: …, .. })`, which fires on any spawn error variant — so the test is platform-robust. No change needed; flagged for awareness.

**L2 (Low) — `discover_subprocess` step-3 lookup does NOT add the `current_exe()` parent path to `searched` when the env hatch returns `None` and `current_exe()` errors.**
File: `src/subprocess.rs:221-228`. If `current_exe_dir_for_lookup()` returns `None` (current_exe failed), no path is pushed to `searched_paths`, so the `SubprocessNotFound` error's `searched_paths` field misses the "tried step 3 but couldn't get the path" hint. Cosmetic; the user would see "tried `$PATH/<tool>`" and figure it out. Recommend: push a sentinel like `PathBuf::from("<current_exe parent unavailable>")` when current_exe fails.

### Efficiency

**L3 (Low) — `RingBuffer::push_bytes` `O(N²)` on cap evictions when small-line streams arrive.**
File: `src/subprocess.rs:99-109`. Each `pop_front` on a `VecDeque<u8>` is O(1), but the `while self.buf.len() + bytes.len() > self.cap { self.buf.pop_front(); }` loop pops one byte at a time. For typical subprocess stderr lines (~64-200 bytes) this is fine. But if a subprocess emits a single byte at a time (unbuffered character writes — rare), each push could iterate close to `cap` pops, giving O(N×cap) total cost. The fix is a batch `drain` instead of a loop: `self.buf.drain(..self.buf.len() + bytes.len() - self.cap)`. Not worth fixing for the v1.0 traffic profile but flagged.

**L4 (Low) — `build_*_argv` allocates `OsString` for every static string constant.**
File: `src/subprocess.rs:289-326, 340-365`. Each `.into()` on a `&str` literal clones the bytes into a new `OsString`. For ~15 flags per call, this is ~15 small allocations per subprocess invocation (twice — b2bg + c2c) per file. With v1.0 single-file inputs, this is 30 allocs/run. No measurable impact; flagged for completeness.

### Structure

**L5 (Low) — Two separate `ENV_LOCK` Mutex instances in `tests/phase_g.rs` and `tests/phase_g_discovery.rs`.**
These do NOT collide because each integration-test file compiles to a separate binary (Cargo's integration-test isolation model), so env vars are per-process and the two Mutexes serve different test-binary process spaces. Mentioned in the Reviewer B angles list as a "could race" concern — verified non-issue. Worth a one-line comment in each `ENV_LOCK` declaration noting the assumption (in case a future refactor consolidates).

**L6 (Low) — Plan deviation #5 (no `last_kept_files` field on `ExtractState`) actually IMPROVES the design.**
File: `src/state.rs:117-178`. The `finalization` value is local to `finalize()`; storing it as a struct field would have introduced lifetime questions about when to clear it. Inline is cleaner. The plan suggested the field but the impl found a better shape — exemplary.

**L7 (Low) — `derive_bedgraph_filename` strip-suffix verified against live Perl runs.**
I ran `s/gz$//; s/sam$//; s/bam$//; s/txt$//; s/$/bedGraph/` in Perl 5 against 6 chained-extension inputs (incl. `foo.bam.gz`, `foo.gz.bam`, `foo.bamgz`, `foo.gz.sam`, `foo.gzbam`, `foobam`). Rust output matches Perl in every case. The trailing-dot quirk in §2.4.6 of the plan is correctly captured. Confirmed against the implementation by tracing `strip_suffix` in order.

---

## Fixes applied

None applied this pass — every issue above is either a recommendation or a design-trade-off discussion. The implementation is correct in the absence of these scenarios.

---

## Recommendations (priority-ordered)

| # | Priority | Recommendation |
|---|---|---|
| H1 | High | Either restore Perl's `@sorting_files` push order via a static OutputKey ordering vector, **or** soften the SPEC's "argv byte-identity" claim to "argv parity modulo positional-tail order, which `bismark2bedGraph` is insensitive to." Pick one before Phase H lands; H requires byte-identity proofs that can be invalidated if reviewers expect strict argv equality. |
| H2 | High | Make `finalize_with_empty_sweep` resilient: on `remove_file` failure, log via `eprintln!` and continue instead of propagating with `?`. Splitting-report and M-bias-txt writes downstream must not be lost on a transient FS error in the sweep. |
| M1 | Medium | Document partial-file behaviour on bismark2bedGraph failure in SPEC §6.6 and add a Phase H harness pre-clean step. |
| M2 | Medium | Swallow `ErrorKind::NotFound` in the `remove_file` branch (tied to H2). |
| M3 | Medium | Add a doc note in `RealRunner::run` that the drain thread holds an exclusive stderr lock for the subprocess lifetime; future panic-hook authors should be aware. |
| M4 | Medium | Rename `BISMARK_TEST_CURRENT_EXE_DIR` → `BISMARK_TEST_ONLY_CURRENT_EXE_DIR` (or feature-gate). Even though the practical attack surface is tiny, the current naming makes a test-only hook look like an officially-supported user-facing env var. |
| L1-L7 | Low | See respective items. None blocking. |

---

## Verdict

**APPROVE-WITH-NITS** — H1 and H2 should be addressed pre-merge (or pre-Phase-H); they're small surgical changes and the latter prevents a real partial-output loss scenario. M1–M4 are doc/polish improvements that can ship in a follow-up. The Phase G implementation is high-quality, well-tested, and faithfully matches the rev-1 plan with all five deviations sensibly justified.

The Perl-fidelity work (trailing-dot quirk, long-form flag expansion, `--parent_dir == --dir`, `--buffer_size 2G` default) is correctly captured — I verified by running the actual Perl regexes against the test cases. The trait-based runner + tee/ring-buffer architecture is clean and testable. The 61 new tests are well-targeted, including the rev-1-cited drain-thread-deadlock test and the BISMARK_BIN edge cases.

Phase H is unblocked subject to addressing H1 (since H is byte-identity gate proper).
