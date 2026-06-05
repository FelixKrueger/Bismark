# CODE_REVIEW_A — Phase 2b: HISAT2 paired-end read-1 `ZS` asymmetry fix

**Reviewer:** A (independent, fresh context)
**Date:** 2026-06-05
**Scope:** `git diff HEAD -- rust/bismark-aligner` on `rust/aligner-v1x` (HEAD `376a6d9` = shipped 2a). Changed: `src/merge.rs`, `src/lib.rs`, `src/report.rs`, `tests/cli.rs`.
**Oracle:** repo-root `bismark` v0.25.1.

## Verdict: APPROVE

The fix is correct, minimal, source-faithful, and well-localized. It reproduces Perl's PE read-1/read-2 `ZS`-capture asymmetry exactly. All four mate-tag unit tests are arithmetically correct against the Perl oracle, the wiring is complete via the single PE merge call site, and SE + Bowtie 2 paths are byte-frozen. Suite green (233 lib + 44 integration), clippy `-D warnings` clean, fmt clean.

**No Critical or High findings.** Two Low items (test-hardening + a pre-existing stray-file hygiene note), neither blocking.

---

## Verification performed

- Read the diff in full + surrounding `merge.rs` PE second-best logic (L598–656), the unique-best selection (L679–740), `insert_pair`/`StoredPair`.
- Cross-checked the Perl oracle: read-1 loop `bismark` 3372–3382 (`if AS / elsif XS / elsif MD` — **no `ZS` branch**), read-2 loop 3384–3403 (`else { $bowtie2 ? XS : ZS }`), SE loop 2775–2796 (top-level `elsif ZS` → SE captures `ZS` for any aligner).
- Confirmed `align.rs::parse` (L100–104) unifies `XS:i:`/`ZS:i:` into one `second_best` field for every record (last-wins), with its own tests — so the merge-level mask is the right and necessary chokepoint.
- Traced all four unit tests through the production logic by hand (results below).
- Confirmed the single `check_results_paired_end` call site (lib.rs:1231 in `drive_merge_pe`), reached by **both** single-core (`process_pe_chunk` lib.rs:952) and parallel (`parallel.rs:487`) paths → no `parallel.rs` edit needed.
- Confirmed `Aligner` is `Copy + PartialEq + Eq` and `config.aligner` exists.
- Confirmed `Aligner::Hisat2.name()` = `"HISAT2"`; PE header line-order matches existing PE Bowtie2 test.
- Ran `cargo test -p bismark-aligner` (233 + 44 pass), `cargo clippy --all-targets -- -D warnings` (clean), `cargo fmt --check` (clean).

---

## Issues by area

### 1. Mask correctness (`merge.rs` L598–618) — CORRECT

The mask `r1_second_best = if aligner == Aligner::Hisat2 { None } else { r1.second_best }` precisely reproduces the Perl asymmetry:

- **Perl read-1 loop (3372–3382)** has only `if AS / elsif XS / elsif MD` — no `ZS` branch. HISAT2 read-1 records carry `ZS` (not `XS`), so `$second_best_1` is **always undef** → backfilled to `$alignment_score_1`. Under Bowtie 2, read-1 carries `XS` → captured. The Rust mask drops read-1's `second_best` only for HISAT2; the existing `.or(Some(as1))` backfill then sets `sb1 = as1` — exactly Perl's undef→AS path.
- **Perl read-2 loop (3384–3403)** captures `ZS` for HISAT2 / `XS` for Bowtie 2 → read-2 keeps its second-best. The Rust code leaves `r2.second_best` untouched. Correct.
- **Per-instance:** the mask sits inside the `for &index in &SCAN_ORDER` loop where `(r1, r2)` are re-bound per instance (L535), so it applies to **every** slot's read-1 (matters for non-dir/pbat 4-slot scans), not just the first. Confirmed.
- **SE unaffected:** `check_results_single_end` (L172) takes no `aligner` arg and reads `rec.second_best` directly (L235) — untouched. Matches Perl SE loop 2780 (captures `ZS` for any aligner). Correct.
- **Bowtie 2 unaffected:** `aligner != Hisat2` → `r1.second_best` passed through verbatim → byte-identical to pre-2b. Confirmed.

The inline comment (L598–608) is accurate and well-cited.

### 2. The four mate-tag unit tests — ALL CORRECT

`pe_second_best_sum` extracts `BestAlignmentPaired.sum_of_alignment_scores_second_best`, which for a single-entry result (one slot, one pair) = `StoredPair.sum_second_best` (merge.rs L681–684). Hand-traced (all use `as1=as2=0`, `sum=0`):

- **(A) HISAT2, sb1=Some(-6) sb2=Some(-6):** mask→sb1=None; gate true→sb1=Some(0); `sum_second = 0 + (-6) = -6`. `0 != -6`→insert `Some(-6)`. **Asserts `Some(-6)`.** ✓ (not -12)
- **(B) Bowtie 2, same inputs:** sb1=Some(-6); `sum_second = -12`→insert `Some(-12)`. **Asserts `Some(-12)`.** ✓ (proves mask is HISAT2-only)
- **(C) HISAT2, sb1=Some(-6) sb2=None:** mask→sb1=None, sb2=None; gate `is_some()||is_some()`→**false**→no backfill→no-second-best branch→insert `None`. **Asserts `None`.** ✓ This correctly exercises the subtle gate-flip → MAPQ-ladder switch (the I-1/B case). Bowtie 2 contrast in the same test: sb1=Some(-6), sb2 backfills to Some(0)→`sum_second=-6`. **Asserts `Some(-6)`.** ✓
- **(D) HISAT2, sb1=None sb2=Some(-6):** sb1 was already None; gate true→sb1=Some(0); `sum_second = 0 + (-6) = -6`. **Asserts `Some(-6)`.** ✓

None trip `amb_same_thread` (in A/B/D `sum != sum_second`; C is no-second-best). All land in `entries.len()==1`. The `run_pe` helper preserves the existing Bowtie2 semantics by delegating to `run_pe_aln(.., Aligner::Bowtie2)`; the error-path test at L1703 was correctly updated with `Aligner::Bowtie2`.

### 3. Wiring (`lib.rs:1240`) — CORRECT & COMPLETE

`config.aligner` is threaded into the lone `check_results_paired_end` call inside `drive_merge_pe`. `grep` confirms this is the **only** call site (lib.rs:1231). Both the single-core (`process_pe_chunk` → `drive_merge_pe`) and `--multicore` (`parallel.rs:487` → `process_pe_chunk` → `drive_merge_pe`) paths converge here, so `parallel.rs` correctly needs no edit. (Moot for HISAT2 anyway — `--multicore + --hisat2` is hard-rejected per 2a, with a test at cli.rs:2044.)

### 4. PE integration fake/test — CORRECT (one hardening gap, Low)

`make_fake_hisat2_pe` is a faithful PE analogue of the proven `make_fake_bowtie2_pe`: same id-strip (`sub(/\/1\/1$/,"",id)`), flags 99/147, mapped-on-`BS_CT`/unmapped-on-GA split. It adds **`ZS:i:-2` on the mate-1 line** (the real HISAT2 read-1 tag), so it genuinely exercises the `ZS`→parser→mask path end-to-end. The test asserts the right things: `_bismark_hisat2_pe.bam` exists, `_bismark_bt2_pe.bam` does NOT, 2 records, "Bismark was run with HISAT2 against", the PE option string, no `--dovetail`, report named `_PE_report.txt`. Passes.

**Note (correct design split):** the unit tests use `XS:i:` tags + the `Aligner::Hisat2` arg, while the integration fake uses the real `ZS:i:` tag. This is intentional and sound — the production discriminator is the `aligner` arg (the parser unifies XS/ZS), so the unit tests validate the mask arithmetic, and the integration fake validates the real `ZS` tag flowing through the parser. No false-pass on tag-name handling.

See L-1 below for the one residual gap.

### 5. Report header test (`report.rs:550`) — CORRECT

`pe_header_hisat2_run_with_line` mirrors the existing `pe_header_two_files` (PE Bowtie2) with `Aligner::Hisat2` + the PE HISAT2 option string. `header_bytes` emits `h.aligner.name()` = "HISAT2" and the PE line-order (`was run with` `\n`, then directional `\n\n`). Expected string is exact and consistent. ✓

### 6. No regression — CONFIRMED

Full suite green at the new HEAD (233 lib + 44 integration, 0 failed); all pre-existing PE Bowtie2, SE, non-dir, pbat, FastA, worker-invariance tests pass unchanged. Bowtie 2 + SE paths are byte-frozen by construction (mask gated on `Aligner::Hisat2`; SE function untouched).

---

## Recommendations

### Low

- **L-1 (test hardening — integration test could false-pass on a silent mask revert).** `hisat2_pe_mapped_names_and_report` asserts naming/report/record-count but **not** the BAM MAPQ. For this fake's pair (`as1=as2=0`, both `ZS:i:-2`): masked → `sum_second = -2` → one MAPQ; unmasked (bug) → `sum_second = -4` → a different MAPQ. So the integration test would still pass if the mask were silently reverted — only the unit tests (A–D) would catch it. The unit tests DO cover the arithmetic, so this is not a coverage hole, but the plan's step 5 / V7 intended the integration test to "assert the BAM record's MAPQ matches the read-1-ZS-ignored expectation." Consider adding a `recs[0].mapping_quality()` assertion to make the integration test a true end-to-end guard of the fix (it's currently a naming/report guard). Non-blocking — recommend the orchestrator note it; the unit tests + the planned oxy gate cover the behavior.

- **L-2 (pre-existing, NOT introduced by 2b — flag so it isn't accidentally committed).** Two untracked test-output strays sit in the crate root: `rust/bismark-aligner/reads_bismark_bt2.bam` and `rust/bismark-aligner/reads_bismark_bt2_SE_report.txt` (dated today, from running the suite; a test defaulting to CWD instead of a TempDir). They are not part of this diff and not gitignored. Same class of stray the Phase-10 commit cleaned up. Do **not** `git add -A` these into the 2b commit; stage only the four source/test files. (Optionally, harden the offending test to write under a TempDir, but that is out of 2b scope.)

---

## Out-of-scope (per plan, correctly deferred)

The 🎯 PE oxy byte-identity gate (V8: PE dir/non-dir/pbat + FastA PE + a single-core `--ambig_bam` PE cell, 10k+1M vs Perl `--hisat2` + HISAT2 2.2.2) is explicitly listed as "NOT done here" in the plan's Implementation Notes. That is the correct scope boundary for this code-only phase; the at-scale byte-identity proof remains the next step.
