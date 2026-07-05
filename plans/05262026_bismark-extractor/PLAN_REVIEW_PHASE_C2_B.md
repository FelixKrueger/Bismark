# Plan review — Phase C.2 (#864, #865, #863 won't-fix) — Reviewer B

**Plan under review:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PHASE_C2_PLAN.md` (rev 0)
**Reviewer:** B (independent of Reviewer A; fresh context)
**Verdict:** Conditional GO. The plan is structurally sound but contains **three correctness bugs** in the byte-identity-critical write order (one fence-post, one trailing-newline over-count, one log-stream misroute) and **misidentifies the SPEC section being rewritten**. All four must be fixed before implementation, otherwise the byte-identity gate will fail in obvious ways the plan explicitly claims to pass.

---

## 1. Logic review

### 1.1 The §3.1 26-step write order vs Perl

I read Perl `bismark_methylation_extractor` lines 4979–5048 (header) and 2480–2558 (body) directly and compared step-by-step to the plan's §3.1. The structural skeleton (line 1 = basename-with-extension, parameter block, conditional ignore/overlap/fasta/merge lines, methylated trio, "C to T conversions" trio, percentage trio, three trailing newlines) matches Perl's overall shape. The conditional matrix in §2.3.2 is faithful.

**However, three byte-level bugs in the write order will cause `cmp` to fail on byte 1 of the post-header region:**

#### Bug 1 (CRITICAL): missing extra blank line between header block and "Processed" line

Perl line 5047 emits `print REPORT "\n";` (closes the header block with one `\n`).
Perl line 2482 then emits `print REPORT "\nProcessed $counting{sequences_count} lines in total\n";` — note the **leading `\n`** in the format string.

So between the last header-block line (e.g. `Methylation in CHG and CHH context …\n` or `No overlapping methylation calls specified\n`) and `Processed N lines in total\n`, Perl emits:

```
…specified\n   ← end of last header conditional
\n             ← line 5047 close-header
\n             ← leading \n of line 2482
Processed …\n
```

That is **two** blank lines (three consecutive `\n` bytes) between the last header-block content line and the `Processed` line. The plan's §2.4 "TARGET" excerpt confirms two blank lines:

```
No overlapping methylation calls specified
                                            ← blank
                                            ← blank
Processed 7699136 lines in total
```

The plan's §3.1 step 12 writes a single `\n` (one blank line) and step 13 immediately writes `Processed …`. That yields one blank line, not two. The 875-byte target will not match.

**Fix:** Either (a) emit step 12 as `\n\n` (two newlines = two blank lines), or (b) keep step 12 as `\n` and prepend a `\n` to the body's first line (matching Perl's leading-`\n` in line 2482). Either is fine; document the choice in §3.1.

#### Bug 2 (CRITICAL): over-counted trailing newlines

Perl line 2553 emits the CHH percentage line as `"C methylated in CHH context:\t${percent_meCHH}%\n\n\n"` — that's the content line **plus** three `\n` bytes total (`%`, then `\n\n\n`). The zero-denominator CHH fallback at line 2556 also ends with `\n\n\n`. So the file ends with the content character followed by **three** `\n` bytes — one closes the CHH line, two are blank-line padding.

The plan §3.1 step 24 writes each percentage line with `\n`, then step 25 separately writes `"\n\n\n"`. This yields after CHH: content + `\n` (step 24) + `\n\n\n` (step 25) = **four** trailing `\n` bytes. The Perl file ends with **three**. Off-by-one fence-post; `cmp` reports a mismatch at the last byte.

This is consistent across the three-context (default) branch and the merge-non-CpG branch (Perl line 2534 emits `\n\n\n` after non-CpG percentage; line 2537 same for fallback).

**Fix:** Either (a) merge step 25 into step 24's last iteration (CHH writes `\n\n\n` as part of its row); (b) write step 24's last row WITHOUT a trailing `\n` and let step 25 supply all three; (c) drop step 25 and have each percentage row write the correct number of `\n`s by index. Option (a) mirrors Perl's structure most cleanly.

#### Bug 3 (CRITICAL): empty-sweep log lines must go to STDERR, not STDOUT

Perl emits the kept/deleted log lines via `warn`, which writes to **STDERR**. From `bismark_methylation_extractor:607,615`:

```perl
warn "$sorting_files[$index] contains data ->\tkept\n";
…
warn "$sorting_files[$index] was empty ->\tdeleted\n";
```

`warn` in Perl is documented to print to STDERR. The plan §3.3 and §10 ("Where to put empty-sweep stdout log lines: `println!` (stdout)") explicitly route the new Rust messages to STDOUT, citing "matches Perl exactly" and "the user's earlier direct `bismark_methylation_extractor` run output showed `contains data -> kept` lines in captured shell output". The captured run almost certainly captured `2>&1` or interleaved streams; that is not evidence of STDOUT.

If Rust uses `println!`, downstream tooling that splits stdout/stderr will see the messages on a different stream than Perl. The Phase H harness uses `case`-suffix dispatch, not stream comparison, so this won't surface as a harness FAIL — but the plan's own "downstream tooling consuming stdout log lines now matches Perl" claim in §7.2 is wrong and any nf-core pipeline that distinguishes streams will diverge.

**Fix:** Use `eprintln!` for both the `kept` and `deleted` lines. Update §3.3, §4.2 doc comment, §5.3 step 3, §7.2, §10 default, and the §5.5.2 test that captures the lines.

### 1.2 SPEC §9.7 — wrong section identified

The plan §3.4.1 claims "Current SPEC §9.7 (presumed)" is the byte-identity invariant statement and proposes to rewrite §9.7 with the 6-point invariant. I read `rust/bismark-extractor/SPEC.md`:

- **§9 heading (line 688)**: "Parallelism model — byte-identity invariant" — this is where the `--multicore N` byte-identity invariant ("MUST produce output byte-identical to `--multicore 1` for any N ≥ 1") is stated.
- **§9.7 (line 725)**: "Speedup expectation" — talks about the 4× speedup target at N=4 on the 10M PE WGBS dataset. Nothing about byte-identity.
- **§8.3 (line 653)**: "Real-data byte-identity gate (10M + 55M PE WGBS)" — this is where the per-file `cmp` byte-identity gate lives.

The plan's "presumed" §9.7 text — "Rust output must be byte-identical to Perl …" — does not appear at §9.7. It is closest in spirit to §9's headline invariant ("byte-identical to `--multicore 1`") and §8.3's per-file gate. The harness PASS-criteria the plan wants to relax actually lives in §8.3, not §9.7.

If §9.7 is rewritten as the plan proposes, the speedup-expectation text is silently destroyed and **the actual byte-identity invariant (§9 header line) is unchanged** — the plan accomplishes nothing of what it intends.

**Fix:**
- Identify the real target. Probably **§8.3 row 1** (`Each of 12 split files unsorted byte equality at --multicore 1`) needs revision: split into strict-byte for splitting-report + M-bias and sorted-content for data files.
- The `--multicore N == --multicore 1` invariant in §9 (line 690) remains the right shape (Rust still must satisfy this) — confirm/keep.
- §9.7 (Speedup expectation) is a separate section and should not be touched by this PR.
- If the plan wants a single canonical "byte-identity invariant" statement, add a new sub-section (e.g. §8.4 or §9.7-renumber-with-current-§9.7-becoming-§9.8) and explicitly state where it is. Do not silently replace §9.7.

The plan §11 self-review claims the 6-point rewrite "preserve[s] the original intent" — without reading the actual §9.7 text. This is a precision failure.

### 1.3 `records_processed` audit

The plan §3.2 / §5.2 step 2 says "audit during impl" whether the current Rust code counts records-or-pairs for PE. I verified directly:

- `src/pipeline.rs:254`: `state.report.records_processed = state.report.records_processed.saturating_add(2);` — bumps **+2 per pair** in the sequential path.
- `src/parallel.rs:770`: `report.records_processed = report.records_processed.saturating_add(2);` — same in the parallel path.

So **both paths currently count 2× pairs**. The plan's planned rewrite (count pairs for PE) is correct and not a no-op. It must touch both files. The plan's §5.2 step 2 says "fix to count pairs AND increment `call_strings_processed += 2`" — the wording is correct, but explicitly call out **both** call sites so neither is missed during implementation (the plan only mentions `src/run.rs` and `src/parallel.rs`; the actual file is `pipeline.rs`, not `run.rs`).

This also means a downstream effect the plan doesn't mention: the Phase F parallel-vs-sequential parity tests assert sums over `SplittingReport`. If both `records_processed` (semantic change to pairs-for-PE) and `call_strings_processed` (new field) are added in the same commit, any existing snapshot or expected-value test on the old `records_processed` field will break and must be updated. Plan §5.8 step 4 hand-waves this — be explicit.

### 1.4 `Bismark result file: ... (SAM format)` literal

I verified Perl line 5000 always writes `(SAM format)` regardless of BAM/SAM/CRAM input — same literal. Plan assumption A4 is correct.

### 1.5 33-char `=` separator

Perl line 2510: `'='x33` — verified. Plan §3.1 step 17 emits exactly 33 `=` chars. ✅

### 1.6 Zero-denominator fallback newlines vary by context

The plan §3.1 step 24 says "if `meth + unmeth > 0` then `C methylated in {ctx} context:\t{pct:.1}%\n`; else `Can't determine percentage of methylated Cs in {ctx} context if value was 0\n`". Reviewer note: the trailing-newline count depends on context AND on the merge_non_CpG mode:

- CpG content line: `\n` (Perl line 2525)
- CpG fallback: `\n` (Perl line 2528)
- merge_non_CpG → non-CpG content: `\n\n\n` (Perl line 2534)
- merge_non_CpG → non-CpG fallback: `\n\n\n` (Perl line 2537)
- 3-context → CHG content: `\n` (Perl line 2545)
- 3-context → CHG fallback: `\n` (Perl line 2548)
- 3-context → CHH content: `\n\n\n` (Perl line 2553)
- 3-context → CHH fallback: `\n\n\n` (Perl line 2556)

The plan treats step 24 as emitting one `\n` per row plus step 25's `\n\n\n` block. This entangles the per-context fence-post bug (Bug 2) with the fallback case. The fix must handle all eight cases above explicitly. Best implementation: a small `write_percent_or_fallback(w, ctx_name, meth, unmeth, is_last)` helper where `is_last == true` emits `\n\n\n`, else `\n`. Or build it from the ground up to mirror the per-line Perl semantics.

### 1.7 Empty-file detection criterion

Perl's empty-file check at line 595–605 reads each per-strand sort-tempfile and considers it "empty" iff every line matches `^Bismark` (i.e. only the header). The plan replaces this with `records_written == 0` — a counter-based check. These are equivalent **as long as**:

- The Rust output file's only header line is the `Bismark methylation extractor version v0.25.1` line that is written by `OutputFileMap::new` (verified at `output.rs:88-90`).
- The `records_written` counter is bumped **only** by successful `write_call` invocations for a call (not for the header write).
- No other writer path writes non-header non-call bytes between header write and finalize.

I verified the first two from `output.rs`. The third needs a defensive assertion — if a future PR adds a non-call write to the file (e.g. a footer comment), the counter check would falsely mark the file as empty and unlink it. Document this constraint in `finalize_with_empty_sweep`'s doc comment so future hackers don't break it.

### 1.8 Sweep call-site ordering

The plan §3.3 / §5.3 step 4 says: run `finalize_with_empty_sweep` AFTER `flush_all` and BEFORE `write_splitting_report`. This is correct **and** the plan also calls `drop(writer)` inside the sweep before `remove_file` to satisfy Windows semantics — fine.

One missing concern: `flush_all` on gzip writers does NOT write the gzip trailer (the trailer goes out at drop time). So between `flush_all` and the sweep, files with `records_written > 0` still need their `GzEncoder` dropped to seal the gzip stream. The sweep currently `drop(writer)`s only inside the loop, which drops every writer (kept files too). After the sweep, the entries vec is consumed and the writers are gone, so kept files are correctly sealed. ✅ But the plan should explicitly state that `finalize_with_empty_sweep` is the **terminal** lifecycle method (after it, no more `write_call` is possible because the map is empty). Currently §3.3 says "subsequent calls are no-ops (defensive)" which is fine; just emphasise that the splitting report write that follows is the only further I/O.

### 1.9 Harness `case` block — semantics

The plan's `case` arms:
- `*_splitting_report.txt|*.M-bias.txt` → strict cmp.
- `*)` → sorted-md5 fallback.

Bismark's actual filenames:
- Splitting report: `{basename}_splitting_report.txt` — matches `*_splitting_report.txt` ✅
- M-bias: `{basename}.M-bias.txt` — matches `*.M-bias.txt` ✅
- Data files default: `CpG_OT_{basename}.txt`, `CHG_OT_{basename}.txt`, etc. — fall through to `*)` ✅
- Data files gzip: `*.txt.gz` — fall through to `*)` ✅ (but sorted-md5 on gzip is not meaningful; would need `gunzip -c | sort | md5sum`)

**Gap:** Gzip-mode harness comparison is broken — `sort` over compressed bytes is nonsense. The plan §2.5 closure for #863 specifies "sorted-content MD5 check per data file", and §3.4.2's bash uses raw `LC_ALL=C sort` against `$PERL_OUT/$f` directly. If a file is `.txt.gz`, this fails or produces wrong output. Either decompress in the comparison (`zcat "$PERL_OUT/$f" | LC_ALL=C sort | md5sum`) or document that the harness only covers `--gzip`-OFF mode (which it currently does — the 10M PE harness invokes with default flags, no `--gzip`). Add a glob arm like `*.gz)` that calls `zcat` first, OR document the limitation in the harness comment block.

The plan §3.5 edge-case table claims gzip mode is handled by sweep — but the harness comparison path isn't gzip-aware. These are different layers; the sweep is fine, the comparison is not.

### 1.10 SE-mode "No overlapping methylation calls specified" line

The plan §3.1 step 9 says: "emit ONLY when `config.paired_mode` is paired AND `config.no_overlap` is true". I verified Perl 5037: `if ($no_overlap) { print REPORT "No overlapping methylation calls specified\n"; }` — only checks `$no_overlap`, not paired-end. However, Perl sets `$no_overlap=1` automatically for PE (verified in §3.5 plan note), and SE never sets it. So the SE check via `paired_mode == paired AND no_overlap == true` is functionally equivalent to Perl's `if ($no_overlap)`. Fine, BUT the simpler equivalent is just `if (config.no_overlap)`. The plan's belt-and-braces check is defensive but adds a code path that won't be exercised. Document the equivalence or simplify.

---

## 2. Assumptions

### 2.1 Stated assumptions worth challenging

- **A5 (banker's rounding)**: Plan notes Rust's `format!("{:.1}", x)` uses round-half-to-even; Perl's `sprintf("%.1f", x)` uses round-half-away-from-zero. This is a real divergence at exact-half boundaries.
  - The plan proposes a 50/50 fixture (`splitting_report_format_one_decimal_precision`) to disambiguate. **This is insufficient.** 50% from a 100/100 split is `50.0` exactly — both rounding modes produce `50.0`. The fixture won't expose the bug.
  - To actually disambiguate, need a fixture where `100 × meth / total` lands on `x.x5` (i.e., the exact half between two `0.1`-step buckets). Example: 5 meth + 35 unmeth → 5/40 × 100 = 12.5 exactly. Rust banker's: 12.5 → 12 (round-half-to-even, 2 is even). Perl: 12.5 → 13. Actually wait — Rust's `format!("{:.1}", 12.5)` doesn't round to one decimal place that way. Let me be precise: `12.5_f64` with `{:.1}` formats to `"12.5"` (no rounding needed). The actual divergence appears when there's a hidden trailing digit beyond machine precision. The classic test case: `0.05_f64`. Rust `format!("{:.1}", 0.05)` → `"0.1"` (because `0.05` actually stores as `0.05000…0027…`). Perl `sprintf("%.1f", 0.05)` → `"0.1"` (round-half-up). Both agree by accident.
  - The actual divergence on percentages is rare but present. The plan should add a fixture with **known-divergent values** (e.g. meth=1, unmeth=199 → 0.5 exactly → Rust may print `0.5`, Perl `0.5` — same in this case too).
  - **Recommended fix**: Audit the real-data test corpus. If 10M PE produces percentages like `80.4` / `2.6` / `1.2` and these match Perl byte-for-byte today (after the `.1` switch), the divergence is theoretical only. The 875-byte byte-identity test against the actual harness output will catch a real divergence; defer the synthetic exact-half fixture to a future hardening PR.
  - At minimum, flag this as an unresolved risk and add a TODO test case with known divergent inputs.

- **A6 (call-strings always 2×sequences for PE)**: Verified at Perl line 2451 (`$methylation_call_strings_processed += 2` per pair). Singleton-orphan edge case: Perl PE only processes when both reads are present (the `<IN>` read at line 1888 assumes R2 follows R1). The Rust pipeline guarantees this via the bismark-io PE pair-reader. ✅

- **A8 (records_processed audit)**: Confirmed above — current Rust counts +2 per pair; needs to count +1.

### 2.2 Unstated assumptions

- **Trailing whitespace / line endings**: Perl is invoked on POSIX. The plan assumes `\n` (LF). Rust's `writeln!` also uses LF on Unix. ✅ On Windows the macro emits CRLF; this is a known footgun. The integration tests in §5.5.3 should pin to `write_all(b"\n")` not `writeln!` for byte-identity-critical bytes. The plan §5.2 step 5 already notes `write!(w, "\n\n\n")` for the EOF block — good. Audit step 3's "26 numbered steps" to ensure every newline is an explicit `\n` byte and not a `writeln!` (which may LF-or-CRLF). The current code at `output.rs:315-380` uses `writeln!` heavily; the rewrite should not.

- **`config.paired_mode` type**: Plan assumes a `paired_mode` field exists. Search for it in `cli.rs` to confirm.

- **Input basename derivation**: Perl line 4979 uses `(split (/\//,$filename))[-1]` — the last `/`-separated segment of the path. The plan §3.1 step 2 says "basename only, no directory prefix" — equivalent via `path.file_name()` in Rust. BUT: Perl does **not** strip extensions for the line-1 output (`$output_filename` at line 4995 still has `.bam`/`.sam`/`.cram` extension). The plan §2.4 confirms this in the TARGET (`SRR..._pe.deduplicated.bam`). Plan §3.1 step 2 is OK but does not explicitly call out "with extension preserved" — make it explicit to avoid an implementor stripping the extension.

- **`config.mode` enum variants**: Plan assumes specific variant names (`OutputMode::MergeNonCpG`, etc.) — verify naming in `cli.rs::OutputMode` during impl.

---

## 3. Efficiency

The plan's §6 efficiency table is correct: all new work is bounded and off the hot path. The `records_written += 1` per call is negligible (one `u64::saturating_add` per record, already inside a `write_all`-dominated loop). The `finalize_with_empty_sweep` is O(N_files ≤ 12). The harness sorted-md5 fallback is O(M log M) per file but only on `cmp` failure, and only on real-data harness runs (not in CI).

**One efficiency note:** The plan §4.2's per-file error handling says "Per-file failures are collected and the first one surfaced; the sweep continues across all entries before returning". This is a behavioural commitment in the doc-comment but not actually implemented in §3.3's code sketch (the sketched code uses `?` which short-circuits on first error). Either:
- Drop the doc-comment "continues across all entries" claim, or
- Actually collect errors into a Vec, run unlink for every entry, then return the first.

The latter is the safer Unix behaviour (no orphan files in a partial-cleanup scenario) and matches the existing `cleanup_all` (`output.rs:205-224`) which uses `eprintln!` warnings on individual failures. **Recommend matching `cleanup_all`'s pattern**: log per-file failures via `eprintln!`, return `Ok(())` regardless. The user-facing surface is just that the report directory may contain stale empty files in pathological cases; not catastrophic.

---

## 4. Validation sufficiency

### 4.1 What's tested

- §5.5.1: 12 unit tests covering each conditional in the splitting-report rewrite. ✅
- §5.5.2: 5 unit tests for the sweep. ✅
- §5.5.3: 4 integration tests covering the end-to-end pipeline + golden byte-match. ✅
- §9.3: real-data oxy harness rerun. ✅

### 4.2 Gaps

- **No test for Bug 1 (extra blank line) explicitly**: the byte-equality assertion in `splitting_report_format_pe_default_no_overlap` would catch it, but only if the golden buffer is correct. Make the golden buffer be a literal `include_bytes!("expected_splitting_report.txt")` from a 875-byte fixture captured from Perl, NOT a hand-crafted Rust string literal (which would replicate any bugs in the implementor's mental model).
- **No test for `--mbias_only` no-op sweep + report still emitted**: Plan §3.5 says report is still emitted with zero call counts; add `splitting_report_format_mbias_only_emits_zero_calls`.
- **No test for `--gzip` empty-file sweep**: Plan §3.3 says "the gz-encoded files contain only the gzip header + gzip trailer when no data was written … records_written == 0 check is per-record, not per-byte; treat as empty; unlink. ✓ correct." But there's no test specifically for this. §5.5.2 has `output_file_map_empty_sweep_gzip_empty_is_deleted` — re-reading, yes it IS there. ✅
- **No Phase F parity test**: Plan §5.8 step 4 says "verified: `parallel_phase_f.rs` tests still pass" but doesn't list a SPECIFIC test for the new `call_strings_processed` field summing under parallel. Add: a Phase F-style test that runs the same input under N=1 and N=4 and asserts `SplittingReport.call_strings_processed` is invariant.
- **No test for stdout vs stderr stream of log lines**: Critical because the plan misroutes them. If Bug 3 is fixed, the test `output_file_map_empty_sweep_stdout_log_lines` becomes `output_file_map_empty_sweep_stderr_log_lines` and must capture stderr (`assert_cmd::Command::output().stderr`, not `.stdout`).
- **No test for the exact "two blank lines after header block" gap**: this is the same as Bug 1 above; the golden-buffer integration test catches it but a focused unit test would localise the failure.
- **No test for the "three trailing newlines" being THREE not FOUR**: same as Bug 2; the byte-equality + size assertion (`assert_eq!(buf.len(), 875)`) catches it. Add an explicit `assert_eq!(buf[buf.len() - 4..], *b"%\n\n\n")` or similar.
- **No test for the `Bismark result file: paired-end (SAM format)` literal regardless of input format**: The plan claims Perl line 5000 is unconditional — add a test that runs Rust against a BAM input and asserts the line emits `(SAM format)`, not `(BAM format)`.

### 4.3 Golden-file approach

The integration tests in §5.5.3 should not hand-craft expected byte buffers. They should capture Perl's output once (on a fixed seed of synthetic input) and `include_bytes!` it. This is the same approach the plan implicitly takes for M-bias byte-identity in earlier phases. State this explicitly.

---

## 5. Alternatives considered

### 5.1 Build the splitting report from a single `format!` template

Instead of 26 numbered `write!` calls, build the whole report as a `String` via one `format!` macro (with conditional inserts via `if`-let chains or a `Vec<String>` joined). This:
- Is more compact and harder to fence-post (one place to compare against Perl).
- Loses streaming (the report is small — irrelevant for performance).
- Makes the conditional branches harder to test individually (one big `format!` vs 8 helpers).

The plan's 26-step approach is fine but should be implemented as a helper-function-per-section to enable per-conditional unit testing. The `write_percent_line` helper in §5.2 step 4 hints at this.

### 5.2 Parse Perl source as the test oracle

For maximum confidence, have the integration test invoke Perl `bismark_methylation_extractor` on a fixed BAM and compare Rust's output byte-for-byte against fresh Perl output. This catches drift if Perl is ever updated. Not currently in the plan; consider for a future hardening PR. For C.2, the snapshot-against-golden approach is sufficient.

### 5.3 Skip the `contains data -> kept` line for non-empty files

Perl emits this line; the plan mirrors it. **Alternative**: only emit `was empty -> deleted` and leave `kept` files silent. This breaks parity with Perl but matches the principle of "less stdout/stderr chatter is better". Reviewer recommendation: stay with Perl parity (the plan's default) for downstream-tool compatibility. ✅ keep as-is.

### 5.4 #863 sorted-content vs raw-byte: defer Perl parity to a `--legacy-perl-output-order` flag

The plan accepts sorted-content equivalence and declines to mimic Perl's multicore output order. **Alternative**: implement a `--legacy-perl-output-order` flag that re-shards the collector output into Perl-multicore-modulo-shape, gated behind a flag. This would let users who need bit-for-bit Perl parity opt in without slowing the default path.

This is significant additional implementation work and the plan correctly identifies that Rust's BAM-input-order is a stronger guarantee. **Defer indefinitely**. Document in SPEC §9 / §8.3 as future work IF user-feedback ever demands raw-byte parity.

### 5.5 SPEC §9.7 vs §8.3 — which section to revise

The plan picks §9.7. Reviewer recommendation: **revise §8.3 row 1** (the per-file gate) and update §9 (the parallelism byte-identity invariant) to clarify that the gate is sorted-content-equivalence for data files. Leave §9.7 (Speedup expectation) alone. Add a cross-link from the closure comment on #863 pointing at the revised §8.3.

---

## 6. Action items

### Critical (must fix before implementation)

1. **§3.1 step 12 → emit `\n\n` (two blank lines), not `\n`** — to match Perl's two-blank-line gap before the `Processed N lines in total` line. Without this, every Phase H harness run fails on byte ~200.
2. **§3.1 step 24/25 → emit three trailing `\n` after CHH percentage line, not four** — the per-row `\n` of step 24's last iteration plus step 25's `\n\n\n` produces four. Either merge step 25 into step 24's last row (CHH writes `\n\n\n`) or write step 24 without trailing `\n` and let step 25 emit all three.
3. **§3.3 / §4.2 / §5.3 step 3 / §10 → log lines must go to STDERR via `eprintln!`, not STDOUT via `println!`** — Perl uses `warn` (stderr). Update the doc comment, test assertion (capture stderr), and §7.2's downstream-impact claim.
4. **§3.4.1 → identify the actual SPEC section being rewritten** — currently §9.7 is "Speedup expectation", not the byte-identity invariant. The real target is §8.3 (real-data byte-identity gate) and/or the §9 heading invariant statement. Do not silently overwrite §9.7. State in the plan exactly which section text is being replaced verbatim.

### Important (fix before implementation but won't break byte-identity directly)

5. **§5.2 step 2 → audit results pre-baked**: current Rust at `pipeline.rs:254` and `parallel.rs:770` adds 2 per pair. The rewrite must change both to +1, and add `call_strings_processed += 2` at the same call sites. Plan refers to `src/run.rs` — actual file is `src/pipeline.rs`. Correct the file references.
6. **§3.1 step 2 → add "with extension preserved" explicit note** to prevent an implementor stripping `.bam`/`.sam`/`.cram` from the line-1 output. Perl writes the basename **with** extension.
7. **§3.4.2 harness → add gzip-aware comparison arm** (`*.gz)` calling `zcat | sort | md5sum`) or explicitly document that the harness covers `--gzip`-OFF mode only. The current 10M PE harness invocation does not use `--gzip`, so this is informational; but if anyone later runs the harness with `--gzip`, the sort-on-binary will silently produce nonsense MD5s.
8. **§3.3 / §4.2 → match `cleanup_all`'s per-file `eprintln!`-on-failure pattern** instead of `?`-short-circuiting on first error. Either implement what the doc-comment promises (continue across all entries) or update the doc-comment to match the `?`-based fail-fast actually sketched.
9. **§5.5.3 integration tests → use `include_bytes!` against captured Perl output, not hand-crafted Rust string literals** for the golden buffers. This prevents the implementor from baking their own buggy interpretation of Perl's format into the test.
10. **A5 banker's-rounding fixture → use a value that actually triggers divergence**, not 50/50. The 50/50 case produces `50.0` under both rounding modes. Audit the test cases in `splitting_report_format_one_decimal_precision` and add a fixture with known-divergent inputs (or document that real-data byte-identity is the de facto gate and the synthetic case is deferred).
11. **§5.5.2 → rename `output_file_map_empty_sweep_stdout_log_lines` to `..._stderr_log_lines`** once Bug 3 is fixed, and capture stderr in the assertion.
12. **§5.8 step 4 → add an explicit Phase F-style parity test** that asserts `SplittingReport.call_strings_processed` and `records_processed` are invariant between N=1 and N=4 on the same input. The plan §11 R5 flags this risk; the test makes it tangible.

### Optional (defer to follow-up or implementation-time judgement)

13. **§3.1 → factor into helper functions per section** (`write_header_block`, `write_counts_block`, `write_percent_block`) for per-conditional unit testability and to make the byte-shape easier to inspect in code review. The 26-step write-everything-in-one-function approach works but is harder to audit.
14. **§3.3 — defensive assertion** that no non-header non-call bytes are written to per-strand files between `OutputFileMap::new` and `finalize_with_empty_sweep`. A comment-level invariant is enough; a `debug_assert!` is overkill.
15. **§3.5 SE no_overlap edge case → simplify to `if (config.no_overlap)`** instead of the belt-and-braces `paired_mode == paired AND no_overlap == true`. SE never sets `no_overlap`; the simpler form mirrors Perl line 5037 directly.
16. **§5.5.3 → add a Perl-as-oracle integration test** that runs `bismark_methylation_extractor` against the same synthetic BAM and asserts byte-for-byte equality. Catches Perl-side drift. Out of scope for C.2 but useful for the v1.0 gate.
17. **§3.4.2 harness — make the sorted-content check default-off** (gated behind an env var like `BISMARK_ACCEPT_SORTED_EQUIV=1`) so that future strict-byte gates default to FAIL on sorted-equivalence rather than silently passing. The §9.7 / §8.3 rewrite formalises this as the v1.0 invariant; this is just a paranoid runtime opt-in.

---

## 7. Summary

The plan is fundamentally sound: it correctly identifies the 15 byte-level differences between current Rust and Perl, correctly designs the empty-file sweep, and correctly closes #863 with a stronger invariant (BAM-input-order is N-invariant; Perl's `--multicore N` output is not). The implementation outline §5 is well-scoped to ~400 LOC.

But the byte-identity sections contain **three correctness bugs** (extra blank-line off-by-one, trailing-newline over-count, log-stream misroute) and **one structural error** (rewriting the wrong SPEC section). All four are introducing failures the plan explicitly claims to eliminate. With those four fixed plus the Important items #5–#12, the plan is implementation-ready.

Recommendation: **revise to rev 1** addressing the four Critical items, then proceed to dual code-review and implementation. The fixes are small and well-localised — no architectural change needed.

---

**Report file:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_PHASE_C2_B.md`
