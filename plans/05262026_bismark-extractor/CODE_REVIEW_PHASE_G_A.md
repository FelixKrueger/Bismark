# Code Review — Phase G (Reviewer A)

**Branch under review:** `extractor-phase-g` off `rust/iron-chancellor` (HEAD `4e5c691`).
**Plan:** `plans/05262026_bismark-extractor/PHASE_G_PLAN.md` rev 2.

## Summary

The Phase G implementation is well-scoped, well-documented, and the rev-1 critical
absorption is largely faithful: filename derivation preserves Perl's
trailing-dot quirk; the stderr drain thread is spawned before `wait()` and
uses `read_until` (byte-safe); `BISMARK_BIN` discovery is strict; the
`FinalizationReport` flows kept-paths into the b2bg positional tail with
sorted absolute paths; argv builders use the long-form flag names. Tests are
thorough (61 new; 299 total) and include real-process fake-shell tests that
exercise the actual drain/wait code path.

I found **two correctness bugs**:

- A **Critical** byte-identity divergence vs Perl in the production wiring:
  the `input_basename` plumbed into `run_phase_g_chain` is the **`.bam`/`.sam`/`.cram`-stripped** value (because `derive_basename` strips them before construction in pipeline.rs/parallel.rs), whereas Perl `:325-330` operates on the **raw** input basename (only path split, no extension strip). All `.bam`-input cases therefore lose the trailing dot before `bedGraph`/`CpG_report.txt` filename derivation.
- A **High** issue in `RealRunner::run`: on `child.wait()` failure, the drain thread is not joined (early `?`-return), contradicting the rev-1 I1 "always join" guarantee and leaking a thread + producing a defective `stderr_tail`.

Other findings are smaller (style / docstring) and are listed below.

## Issues by area

### Logic

#### L1 (Critical) — Production wiring passes `.bam`/`.sam`/`.cram`-stripped basename into Phase G; Perl byte-identity broken for the common case.

`src/state.rs` stores `self.input_basename` from the ctor (`input_basename: &str` parameter). That parameter is supplied by callers in `src/pipeline.rs:80,224` and `src/parallel.rs:189` as:

```rust
let input_basename = derive_basename(input);
let mut state = ExtractState::new(config, input, &input_basename, …)?;
```

`derive_basename` (`pipeline.rs:51-64`) strips exactly one of `.bam` / `.sam` / `.cram` (with leading dot) and returns the result. That is correct for the Phase D M-bias / splitting-report naming and for the `_splitting_report.txt` suffix concatenation (Perl `:4984-4988` strips `.sam`/`.bam`/`.cram`/`.txt`).

But Phase G's `derive_bedgraph_filename` mirrors a **different** Perl regex pipeline at `bismark_methylation_extractor:325-330` which strips the **literal letters** `gz`/`sam`/`bam`/`txt` (no leading dot anchor) from the **raw** input basename (Perl: `(split (/\//,$filename))[-1]` — path split only, no extension peel). That algorithm relies on the trailing `bam`/`sam`/`txt` still being present in its input.

Trace for the most common real-world case — `sample.fastq_bismark_bt2_pe.deduplicated.bam`:

| Step | Perl | Rust (as wired today) |
|---|---|---|
| Raw basename | `sample.fastq_bismark_bt2_pe.deduplicated.bam` | (same) |
| Pre-Phase-G normalisation | none (Perl passes raw to `:325-330`) | `derive_basename` strips `.bam` → `sample.fastq_bismark_bt2_pe.deduplicated` |
| `s/bam$//` (Perl) / `derive_bedgraph_filename` strip-loop | `sample.fastq_bismark_bt2_pe.deduplicated.` | (no `gz`/`sam`/`bam`/`txt` suffix → no strip) `sample.fastq_bismark_bt2_pe.deduplicated` |
| Append `bedGraph` | `sample.fastq_bismark_bt2_pe.deduplicated.bedGraph` ✅ | `sample.fastq_bismark_bt2_pe.deduplicatedbedGraph` ❌ |

The same `.`-vs-no-dot difference cascades to the `.bismark.cov.gz` coverage file and the `CpG_report.txt` / `CX_report.txt` cytosine filenames.

**Why the in-suite tests don't catch it.** Both
`tests/phase_g.rs` and the inline unit tests for `derive_bedgraph_filename`
pass literals like `"foo.bam"`, `"foo.bam.gz"`, or
`"sample.fastq_bismark_bt2_pe.deduplicated.bam"` **directly into the
orchestrator / function** — bypassing `derive_basename`. The unit test
`derive_bedgraph_filename_real_bismark_pe_naming` documents the expected
Perl output for `sample.fastq_bismark_bt2_pe.deduplicated.bam`, and that
function passes. But in production, that exact filename never reaches
`derive_bedgraph_filename` with the `.bam` intact — it has already been
stripped to `sample.fastq_bismark_bt2_pe.deduplicated` by `derive_basename`.

This is **exactly** the kind of seam-vs-unit mismatch Phase H's
byte-identity gate exists to catch — but Phase G ships before Phase H, and
this implementation note (deviation #5: "kept paths consumed inline, no
need to persist") didn't surface the basename plumbing question.

**Suggested fix.** Phase G needs a dedicated `raw_basename: String` on
`ExtractState` (or derive it on demand from `state.input_path`) that is
**just** `input.file_name().to_string_lossy()` with no extension stripping
— mirroring Perl's `(split (/\//,$filename))[-1]`. Pass that into
`run_phase_g_chain`, not the `derive_basename`-stripped value. The unit
tests for `derive_bedgraph_filename` are already correctly authored for
this raw input; only the production wiring needs updating.

A complementary regression-guard test: invoke `extract_se` (or its
parallel form) end-to-end with `--bedGraph` + a `MockRunner`-friendly hook
on a real `.bam`-named input fixture, and assert the captured argv's
`--output` value equals `"<stem>.bedGraph"` (with the dot).

Affected sites:
- `src/state.rs:51` (`input_basename` field) and `:69-92` (ctor).
- `src/state.rs:168-174` (`run_phase_g_chain` call site).
- `src/pipeline.rs:80-81, 224-225` (callers).
- `src/parallel.rs:189-190`.

This is the only Critical I found and it should block merge.

---

#### L2 (Medium) — `RealRunner` pre-spawn audit log: log-once vs every-spawn.

`subprocess.rs:411-418` unconditionally emits `[bismark-extractor] spawning: …`
on every invocation. For a 2-subprocess chain at `--multicore N`, that's
2N noise lines on stderr above the Perl tools' own progress. Plan §1 / rev
1 O2 framed it as "debugging affordance" — keeping it for v1.0 is defensible,
but consider gating behind `RUST_LOG`-like env var or a `--verbose` plumbing
once added. Not a bug; flag for future polish.

---

#### L3 (Low) — `discover_subprocess` returns the `BISMARK_BIN/<tool>` candidate from `searched_paths` only in the failure branch, but `PATH` failure pushes a synthetic `"$PATH/<tool>"` marker string into `searched_paths`. The two list elements aren't structurally comparable.

`subprocess.rs:217`:
```rust
Err(_) => searched.push(PathBuf::from(format!("$PATH/{tool_name}"))),
```

That string in a `Vec<PathBuf>` is purely a UX marker for the error message;
it would never round-trip as a real path. Fine as a diagnostic but worth a
comment noting the marker-string convention. Recommend a one-line comment
above the `push` to spell out the intent (and to direct anyone parsing the
error to not interpret the string as a filesystem path).

---

#### L4 (Low) — `derive_bedgraph_filename`'s strip loop allocates a `String` per stripped iteration.

`subprocess.rs:146-152`:
```rust
let mut s = input_basename.to_string();
for ext in &["gz", "sam", "bam", "txt"] {
    if let Some(stripped) = s.strip_suffix(ext) {
        s = stripped.to_string();   // ← reallocates each successful strip
    }
}
```

Each successful strip allocates a fresh `String` from a `&str` slice of the
old one. With max 4 successful strips per call and short names, this is
negligible — but a `String::truncate(s.len() - ext.len())` avoids the
re-allocation entirely. Recommend nothing; flag for context.

### Efficiency

#### E1 (Low) — `discover_subprocess` is called twice per chain (once for b2bg, once for c2c).

Each call re-reads `BISMARK_BIN` and re-resolves `PATH`. For a chain that
runs after a multi-minute extractor pass, the cost is negligible. No action
needed; mentioned only for completeness.

#### E2 (Low) — `BufReader::new(stderr)` uses default 8 KiB capacity.

`subprocess.rs:437`. For very chatty subprocesses that's adequate; the ring
buffer bound elsewhere caps memory regardless. No action.

### Errors

#### ER1 (High) — `RealRunner::run` does NOT join the drain thread on `child.wait()` failure.

`subprocess.rs:456-458`:
```rust
let exit_status = child
    .wait()
    .map_err(|source| BismarkExtractorError::SubprocessSpawnFailed { tool, source })?;
```

If `wait()` returns `Err`, `?` returns the error **before** the join code at
`:461-473` runs. The drain thread is orphaned (continues reading until EOF
on the subprocess's stderr — typically OK because the child is exiting —
but the join is skipped and no `stderr_tail` is attached to the eventual
`SubprocessSpawnFailed`). This contradicts the rev-1 I1 promise that the
drain is **always** joined regardless of exit status, and the corresponding
docstring at lines 396-400 says "always joins the drain thread before
returning (prevents stderr-tail races + thread leaks)".

In practice `child.wait()` failure on Unix is extremely rare (mostly EINTR
or EBADF — neither plausible here), so the user-visible impact is small.
But the contract is documented and the test
`realrunner_drain_thread_joined_on_err_path` only exercises the non-zero-exit
path, not the `wait()`-fails path — which is unreachable from the fake-shell
fixtures.

**Suggested fix.** Refactor to:

```rust
let wait_result = child.wait();
let drain_join = drain_handle.join();
let exit_status = wait_result
    .map_err(|source| BismarkExtractorError::SubprocessSpawnFailed { tool, source })?;
let stderr_tail = match drain_join { … };
```

So both outcomes are bound before any early return; the drain join is
unconditional. Alternative: wrap `wait()` and `join()` in a small RAII guard
that joins on drop. The first form is simpler.

This is independent of the L1 fix and should land alongside it. Recommend
adding a test that asserts the drain thread can't be left orphaned via a
deliberately-broken `wait()` simulation (or at minimum, a comment near the
`?` clarifying that EINTR is the only realistic Unix failure mode and the
parent stderr capture is in a best-effort regime here).

---

#### ER2 (Medium) — `eprintln!` in the drain thread's loop uses `stderr_lock` for `write_all`, but `eprintln!` from other threads (e.g., the pre-spawn audit at :411 in the parent) does NOT.

`subprocess.rs:439-440` lock+drain pattern is correct for the drain thread.
But `eprintln!` from non-drain code (`run_phase_g_chain`'s
"note: extractor produced no methylation calls" warning, `cleanup_all`'s
remove-failure warnings, the empty-sweep's `was empty -> deleted` lines)
goes through `io::stderr()` without coordinating with the drain thread's
lock. On a chatty multi-line drain interleaved with parent-side `eprintln!`,
lines may interleave at sub-line granularity if the parent emits during
the child's drain.

In practice, `eprintln!` line-buffers and acquires the global stderr lock
per macro invocation, so true byte-level interleaving is unlikely. But the
drain thread holds the lock continuously across multiple `write_all`
+`push_bytes` cycles, which can block parent `eprintln!`s until a child
line ends. That's fine for ordering but means the audit/diagnostic
`eprintln!`s might block briefly during a chatty subprocess.

Not a bug; flagged for awareness. No action needed unless
log-interleaving comes up in Phase H golden comparison.

---

#### ER3 (Low) — `SubprocessFailed` Display computes `String::from_utf8_lossy` on every render.

`error.rs:236-241`. Each `Display::fmt` call constructs a new
`Cow<str>` from the `Vec<u8>`. Negligible cost; no action.

### Structure

#### S1 (Low) — `current_exe_dir_for_lookup` env-hatch deviation is well-documented in code but the deviation note in the plan §"Implementation Notes" #1 could be cross-referenced from the function's doc comment.

`subprocess.rs:236-251` documents the always-on rationale clearly. Minor:
the deviation log entry says "Documented inline with rationale" — the code
comment satisfies that. Just confirming the cross-reference is sound.

---

#### S2 (Low) — `RingBuffer::push_bytes` uses `pop_front` in a loop for the "evict-some-from-front" branch.

`subprocess.rs:105-107`:
```rust
while self.buf.len() + bytes.len() > self.cap {
    self.buf.pop_front();
}
```

For a 64 KiB ring buffer and a 500-byte input that needs ~500 evictions, the
worst case is `O(n)` `pop_front`s. `VecDeque::drain(0..n)` would be `O(1)`
amortised for the whole eviction. Not a hot path (line-rate stderr is at
most a few MB/s); no action needed, but a small optimisation for very chatty
subprocesses.

---

#### S3 (Low) — Two near-identical `with_env` helpers across `tests/phase_g.rs` and `tests/phase_g_discovery.rs`.

The bodies are identical (`ENV_LOCK`, prior-capture, set/remove, run,
restore). Common pattern would be `tests/common/env.rs`. Not worth the
refactor right now; flag for the next time test infrastructure grows.

---

#### S4 (Nit) — `OutputFileMap::finalize_with_empty_sweep` doc string for `kept` says "sorted lexicographically so the argv ordering … is deterministic". The implementation sorts both `kept` AND `swept`. Doc only mentions `kept`. Pure docstring polish.

`src/output.rs:55-61` + `:321-322`. Either drop the swept sort or document it.

---

#### S5 (Low) — `state.rs::finalize` deviates from the plan §4.5's `last_kept_files: Option<Vec<PathBuf>>` design.

The deviation is correct and noted in the plan's Implementation Notes #5.
Inline consumption is cleaner; the alternative shape would only matter if
some future code path consumed `kept` outside `finalize`. No action.

## Fixes applied

None — all findings are recommendations. The Critical (L1) needs a design
call between (a) plumbing the raw basename through to Phase G alongside
the existing stripped basename, vs (b) recomputing the raw basename
on-demand inside `state.finalize` from `self.input_path.file_name()`.
Option (b) is a one-liner and avoids the extra field; option (a) is
clearer if the raw basename will be reused elsewhere. I'll defer to the
caller / B reviewer on the preferred shape, but the bug itself is
unambiguous and blocks merge.

## Recommendations

| Priority | ID | Action |
|---|---|---|
| Critical | L1 | Pass the **raw** (un-stripped) input basename into `run_phase_g_chain`, not the `.bam`/`.sam`/`.cram`-stripped value. Add an integration test that exercises the wiring with a real `.bam`-named input fixture and a MockRunner; assert the captured argv's `--output` filename has the trailing dot before `bedGraph` (i.e., matches the `derive_bedgraph_filename_real_bismark_pe_naming` unit-test golden). Without this fix, Phase H byte-identity will fail on the most common production case. |
| High | ER1 | Refactor `RealRunner::run` so the drain-thread join happens unconditionally, even on `child.wait()` failure. Bind `wait_result` and `drain_join` before any early return. Update test or add a comment noting the EINTR-only realistic failure mode. |
| Medium | L2 | Consider gating the pre-spawn `[bismark-extractor] spawning: …` audit log behind a verbosity env var; track for v1.x. |
| Medium | ER2 | If Phase H log-comparison fails on interleaved drain/parent eprintln, coordinate the parent-side eprintlns through the same stderr lock. Not blocking. |
| Low | L3 | One-line comment above `subprocess.rs:217` clarifying the `$PATH/<tool>` marker-string convention. |
| Low | L4 | Optional `String::truncate` instead of `.to_string()` reallocations in the strip loop. |
| Low | S2 | Optional `VecDeque::drain(0..n)` for ring-buffer eviction. |
| Low | S3 | Optional: extract `with_env` helper to `tests/common/env.rs`. |
| Low | S4 | Docstring polish on `FinalizationReport.kept` sort comment. |
| Low | E1, E2, ER3, S1, S5 | Informational; no action. |

## Verdict

**NEEDS-REVISIONS.**

L1 is a real production byte-identity bug on the most common input shape
(`.bam` files, which are Bismark's primary output). The fix is a small
plumbing change but unambiguously required before merge — the entire
purpose of Phase G is to land Phase H byte-identity for these outputs,
and as-shipped the bedGraph / coverage / CpG_report filenames are
off-by-one-dot vs Perl for `.bam` / `.sam` / `.cram` inputs.

ER1 is a smaller defect but also a documented-contract violation. I'd
land both in the same revision; they are independent.

Once L1 + ER1 are addressed (with regression tests for L1 covering at
minimum `sample.fastq_bismark_bt2_pe.deduplicated.bam` end-to-end through
the wired `state.finalize` path), I'd give an APPROVE-WITH-NITS on the
remaining findings.
