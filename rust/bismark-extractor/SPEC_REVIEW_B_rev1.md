# SPEC_REVIEW_B (rev 1) — `bismark-extractor` SPEC

Reviewer: B (independent, fresh context)
Target: `rust/bismark-extractor/SPEC.md` on branch `rust/extractor-recon`, PR #841, **rev 1** (727 lines).
Cross-reference: my rev 0 review at `rust/bismark-extractor/SPEC_REVIEW_B.md`.
Perl source spot-checked: `bismark_methylation_extractor` lines 959-993, 989-990, 2905, 2989, 5040, plus `rust/bismark-dedup/src/cli.rs` lines 225-231 for the samtools_path precedent.

## Verdict: **NEEDS-REVISIONS (minor)**

Rev 1 addresses **the substance** of every rev 0 finding I raised, and Reviewer A's §6.5 rewrite landed cleanly. The structural §8 + §9 duplication is gone, the `--fasta` row is corrected, invalid-XM-byte gets `Err`, the `(epic §)` placeholders carry real Perl line numbers, the flag-count narrative is corrected, the byte-identity gate has unsorted equality at N=1 + cross-N=1/N=4 equality, and the collector-reorder unit test is in §8.1.

What blocks an APPROVE today is a **small cluster of editorial leftovers** introduced by the rev 1 edits — not architecture. The most consequential is a **factually-inverted claim** about the dedup `--samtools_path` precedent (rev 1 says "matches dedup"; dedup is actually silent). The second is §7.4, which now contains a literal "Hmm, this is getting confused" debug breadcrumb and two contradictory comparator code blocks. The third is a stale §12 row that still says "extractor MUST NOT reverse" — directly contradicting the rev 1 §6.5 rewrite. Two stale "34 flags" mentions linger in §2 and Phase A.

These are 30 minutes of mechanical fixes; the design is sound. Below is the rev 0 → rev 1 audit followed by the new findings.

---

## Rev 0 finding audit

| Rev 0 ID | Finding | Rev 1 status |
|----------|---------|--------------|
| C1 | Duplicate §8 + §9 | **Fixed.** `grep "^## 8\|^## 9\|^## 10" SPEC.md` returns exactly one of each. Section numbering is monotonic. |
| C2 | `--fasta` factually wrong | **Fixed correctly.** §3 row 4 now describes the splitting-report line at Perl 5040 verbatim. I confirmed against Perl: line 5040 emits exactly the one report line and nothing else when `$genomic_fasta` is set. |
| C3 | Invalid XM byte silently skipped | **Fixed.** §5 has the new row; §7.1 has `classify_xm_byte -> Result<…, BismarkExtractorError>` with `Err(InvalidXmByte)`; §8.4 has the fixture; §8.1 has `extract_calls_rejects_invalid_xm_byte_with_error`. Solid. |
| C4 | `(epic §)` citations | **Fixed.** Rows 7 + 8 now read `989` / `990`. Verified against `bismark_methylation_extractor` lines 989-990. |
| C5 | 34 vs 35 flag count | **Mostly fixed.** §3 narrative and table corrected (35 GetOptions entries). But §2 line 19 still says "All 34 Perl CLI flags" and Phase A line 660 still says "`--help` prints all 34 flags". Two leftover occurrences — see N1 below. |
| C6 | Sorted-md5 hides ordering | **Fixed.** §8.3 now has unsorted-byte-equality at N=1, sorted-md5 at N=4, AND a cross-N=1/N=4 equality assertion. This is stronger than I asked for. |
| C7 | Collector-reorder unit test | **Fixed.** §8.1 has `collector_reorders_worker_output_under_skew` with a clear contract ("simulate 4 workers emitting out of order; assert collector emits in strict input_idx order"). |
| I1 | CHG/CHH M-bias tests | **Fixed.** Four new tests (X→chg/meth, x→chg/unmeth, H→chh/meth, h→chh/unmeth) + two M-bias writer section-count tests. Directly closes the missing-CHG/CHH bug class at the unit level. |
| I2 | Directional vs non-directional fixture | **Fixed.** §8.4 has both fixtures with the directional one asserting CTOT/CTOB are empty/absent. |
| I3 | Cross-chr + mixed-strand pair | **Fixed.** Both edge cases added to §8.4. |
| I4 | Zero-length XM | **Partial.** `extract_calls_empty_xm_yields_empty_vec` exists in §8.1 but covers "no methylation bytes", not literal `seq_len == 0`. Not a blocker — the invariant `read_pos == seq_len` after the loop trivially holds for `seq_len == 0`. Defer to Phase B. |
| I5 | `--CX_context` scope contradiction | **Fixed.** §2 rev 1 note explicitly removes the "deferred" claim; §11 logs it as Resolved (in scope via subprocess). |
| I6 | `--samtools_path` warning | **Substantively fixed but factually wrong about the precedent** — see new finding NB1 below. |
| I7 | `--genome_folder` validation | **Mostly fixed.** §3 row 22 + §11 specify the rejection. But the error message is in §11 only (`--cytosine_report requires --genome_folder <PATH-TO-BISMARK-GENOME-DIR>; the Perl default mouse path is not honoured in the Rust port`). No corresponding §8.1 unit test (`cli_validate_rejects_cytosine_report_without_genome_folder`). Minor — see N2. |
| I8 | BufWriter buffering | **Fixed.** §7.7 "Buffering policy" subsection: 8-KiB `BufWriter<File>`; `BufWriter<GzEncoder<File>>` for `--gzip`. §11 logs as Resolved. |
| I9 | `--parallel > 4` warning policy | Not addressed. Not in §11 open questions. Not a blocker for SPEC approval but should be on the Phase F TODO. Optional. |

Net: 7/7 critical and 7/9 important findings fully addressed; one important (I6) is substantively addressed but with a factual error in the rationale; two (I4, I7, I9) are minor leftovers.

## Reviewer A's §6.5 rewrite

Rev 1 §6.5 commits to `bismark-io 1.0.0-beta.6` + the `iter_aligned() -> impl Iterator<Item = (read_pos_5p, ref_pos, xm_byte)>` adapter (option b). I think this is the right call: it consolidates the orientation correction in `bismark-io` (so dedup's existing code is unaffected; future consumers like `bismark2bedGraph` inherit the same corrected stream), and the iterator API is simpler at the call site than two parallel `xm_read_oriented()` + `cigar_read_oriented()` accessors. The §6.5 rationale (BAM stores `-` strand reads reverse-complemented, so `XM[0]` is the 3' end of the sequenced read; M-bias needs 5'-end-of-read-cycle positions) is correctly stated and matches Perl 1619-1621 / 2877-2886.

**One open implementation note**: `iter_aligned()` is going to want to interact with `--ignore` (5') and `--ignore_3prime` (3') boundaries that operate in **5'-read coordinates**. The §7.1 pseudocode currently keeps `read_pos` as the post-CIGAR-walk index into the raw `xm` buffer — if `iter_aligned()` yields already-5'-oriented positions, `read_pos` semantics change and the §7.1 loop needs revisiting. Not a blocker (SPEC says "fleshed out in implementation phases") but Phase B should land §7.1 + the `iter_aligned()` API together.

## §7.4 overlap comparator deferral

§7.4 noting the comparator polarity as a Phase C verification blocker would be **acceptable as a deferral** if the SPEC said it cleanly. It doesn't. The current text:

1. Sets up the comparator with `<`/`>` (strict keep) in the first code block.
2. Mid-section breaks into prose: *"Wait — re-read carefully. … Hmm, this is getting confused."*
3. Shows a near-identical second code block.
4. Then says "TODO for Phase C implementation: write the comparator predicates against the actual Perl source byte-for-byte."

That's a debug-stream-of-consciousness leaked into the SPEC. **For an SPEC-level document this needs cleanup** before approval. The correct content is:

- Perl skip-test (line 2905, forward): `if R2_pos >= R1_end then return` → KEEP predicate is `R2_pos < R1_end` (strict).
- Perl skip-test (line 2989, reverse): `if R2_pos <= R1_end then return` → KEEP predicate is `R2_pos > R1_end` (strict).

I verified both against `bismark_methylation_extractor` lines 2905 and 2989. So the original §7.4 strict `<`/`>` was correct. The "Corrected in rev 1" prose claiming `<=`/`>=` (inclusive) appears to be **wrong** — it's the SKIP predicate that's inclusive, not the keep predicate. Rev 1 has confused itself into thinking it was wrong when it was actually right.

**Fix**: pick one polarity (the strict-`<`/`>` keep predicate, matching what I just re-verified against Perl), drop the "Wait" / "Hmm" prose, drop the second redundant code block, drop the "Corrected in rev 1" note. The §7.4 unit tests (`drop_overlap_forward_pair_drops_r2_at_or_before_r1_end`, `..._at_or_after_r1_start`) describe the SKIP behaviour — those are fine.

## New unit-test bug-class coverage (rev 1 §8.1)

The 8 new tests close the right bug classes:

- `mbias_accumulate_routes_to_chg_table_for_X_byte` + `_for_x_byte` → closes Alan's missing-CHG bug at the unit level. **Sufficient.**
- `mbias_accumulate_routes_to_chh_table_for_H_byte` + `_for_h_byte` → closes Alan's missing-CHH bug. **Sufficient.**
- `mbias_writer_emits_six_sections_for_pe` + `_three_sections_for_se` → asserts the M-bias output structure (section count + order). **Sufficient.**
- `extract_calls_rejects_invalid_xm_byte_with_error` → locks Perl's `die` semantics. **Sufficient.**
- `collector_reorders_worker_output_under_skew` → closes §9.4 invariant. **Sufficient.**

**Bug classes I'd still want unit-tested but aren't (Phase B/F to-do, not blockers):**
- M-bias merge under worker skew (the §9.3 commutativity claim) — `mbias_merge_under_worker_skew_is_commutative`.
- `--mbias_only` skips split-file writes but still accumulates M-bias.
- `--mbias_off` skips M-bias but still writes split files.
- Directional library produces 0-byte CTOT/CTOB FHs (this is integration-level in §8.4, but a unit-level FH-creation test would help).

These are all good Phase D/F additions, not SPEC-level blockers.

## New rev 1 issues (independently found)

### NB1 — `--samtools_path` "matches dedup precedent" is factually inverted (Critical editorial)

§3 row 27 rev 1 says `--samtools_path` *"emits one-line stderr warning … Matches the dedup port's precedent."* §11 Resolved row says *"matches dedup precedent."*

This is **factually wrong**. I checked `rust/bismark-dedup/src/cli.rs` lines 225-231:

```rust
// --samtools_path is silently accepted and ignored (no warning;
// bismark-io is pure-Rust). --parallel is honoured in v1.1.
let _ = self.samtools_path;
```

Dedup is **silently** accepted. The extractor SPEC's "matches dedup precedent" claim is the opposite of the truth.

**The choice itself is fine** — emitting a stderr warning may actually be the better policy (it's user-visible that the flag does nothing). But the rationale is wrong. Either:
- **(a)** Change rationale: "Diverges from dedup's silent acceptance; one-line stderr warning is the better UX going forward, and v1.1 dedup can adopt it later."
- **(b)** Match dedup: silently accept with no warning.

**My recommendation**: pick (a) — keep the warning, fix the rationale. The dedup silence was arguably a miss; standardising on a warning here sets the right precedent for future ports. But the SPEC must accurately describe the state of the world.

### NB2 — §7.4 "Hmm, this is getting confused" + two redundant code blocks (Critical editorial)

Detailed under "§7.4 overlap comparator deferral" above. SPEC document has working prose visible at lines 369-388. Must be cleaned.

### NB3 — §12 row 4 "extractor MUST NOT reverse" contradicts §6.5 rewrite (Important)

§12 row 4 "CIGAR string reversal for `-` strand (risk of double-reverse)" Prevention column says: *"§6.5: extractor MUST NOT reverse; reversal is reader-side."*

§6.5 rev 1 says exactly the opposite: Perl reverses **both** XM and CIGAR, and the Rust port plans to do the same via `bismark-io::iter_aligned()` (i.e. reversal IS reader-side, but the orientation correction IS happening). The §12 row 4 wording is **the rev 0 wording** that the rev 1 §6.5 explicitly says was wrong.

**Fix**: rewrite §12 row 4 Prevention to: *"§6.5: `bismark-io::iter_aligned()` yields 5'-end-of-read-oriented `(read_pos, ref_pos, xm_byte)` triples; the extractor consumes these directly. Reversal is fully encapsulated in `bismark-io`; the extractor never sees raw XM or CIGAR for `-` strand reads."*

### NB4 — Stale "34 flags" mentions in §2 and Phase A (Important)

After the rev 1 "34 → 35" correction, two leftover occurrences:
- Line 19 (§2 Scope): *"All 34 Perl CLI flags (per §3 inventory)…"*
- Line 660 (Phase A): *"`--help` prints all 34 flags."*

Should be **35** in both. The §3 narrative and table are correct; only these two outliers need updating.

### NB5 — `--samtools_path` warning text quotes itself (Optional)

§3 row 27 includes the exact warning string verbatim in the table cell. That's locked-in by Phase B implementation tests (the warning text will need to be byte-stable across versions or a regex). Worth a one-line note that the literal string is the contract.

### NB6 — §14 Revision history is accurate (Verified)

The §14 rev 1 entry lists all the major changes. I spot-checked: §6.5 corrected (✓), §7.1 corrected (✓), §3 row 4 corrected (✓), §3 ignore_3prime citations (✓), flag count 34→35 (✓ in §3, ✗ in §2 + Phase A — see NB4), §3 row 27 samtools_path with warning (✓, but rationale wrong — see NB1), §2 CX_context in scope (✓), §5 invalid-XM row (✓), §7.7 buffering (✓), §8.1 8 new tests (✓), §8.3 strengthened (✓), §8.4 5 new edge cases (✓), §11 resolutions (✓), stripped duplicate §8+§9 (✓).

§14 does NOT mention the NB3 §12 row 4 stale rev 0 wording — because the rev 1 author didn't catch it. That's the silent omission. Otherwise §14 is accurate.

## §3 row 4 (`--fasta`) verification

Rev 1: *"Splitting-report-only annotation; no FASTA output produced. When set, adds one line to `_splitting_report.txt`: `Genomic equivalent sequences will be printed out in FastA format\n`."*

I verified against Perl lines 5035-5045: the only effect of `$genomic_fasta` being true is `print REPORT "Genomic equivalent sequences will be printed out in FastA format\n";` on line 5041. No other code path uses `$genomic_fasta`. **The SPEC's claim is the FULL behavior.** ✓

## §7.1 `XmClassification` shape

`Result<XmClassification, BismarkExtractorError>` with variants `MethylationCall`, `SkipUnknownContext`, `SkipNonCytosine`.

**Is splitting `Skip*` into two variants right?** Two perspectives:

- **Conflate to `Skip`**: caller doesn't care WHY a byte was skipped, just that it was. Simpler match arm at the call site.
- **Keep split**: the splitting-report counters track `U`/`u` separately from `.` in some Perl code paths (e.g. line 2970 for `U`, the `.` falls into "non-cytosine which we don't count"). If the splitting report needs per-category counts, the split is useful; if it needs only "skip count", conflate.

Spot-checking Perl: I don't see per-category split for U/u/. in `_splitting_report.txt`. The Perl bytes are silently skipped (the `else { next }` arms at lines 2970-2972 don't increment a counter for `U`/`u` vs `.`). So **conflate would be sufficient**. But keeping the split is harmless and gives flexibility if a future SPEC needs per-byte-type counts.

**My recommendation**: keep the rev 1 split. It's slightly more typing in the match arm but documents the intent. Not a blocker either way.

## §11 `--genome_folder` rejection message specification

Rev 1 §11 has the message verbatim: `--cytosine_report requires --genome_folder <PATH-TO-BISMARK-GENOME-DIR>; the Perl default mouse path is not honoured in the Rust port`.

**Is that clear enough for Phase A?** Yes. The wording is unambiguous and the test contract is straightforward: `cli_validate_rejects_cytosine_report_without_genome_folder` should assert the error message contains the substring `"--cytosine_report requires --genome_folder"`. (The §8.1 unit-test entry for this is missing — see I7 leftover above.)

---

## Action items (prioritised)

**Critical editorial (block APPROVE):**
- **NB1**: Fix `--samtools_path` rationale — drop "matches dedup precedent" claim (dedup is silent, not warning). Either keep the warning and rewrite the rationale as "diverges from dedup; better UX going forward", or match dedup and be silent.
- **NB2**: Clean up §7.4 — pick one comparator polarity (strict `<` / `>` keep is correct against Perl 2905 + 2989), drop the second code block, drop "Hmm, this is getting confused", drop the "Corrected in rev 1 to <=/>= (inclusive)" claim (the keep predicate is strict; the skip predicate is inclusive).
- **NB3**: §12 row 4 Prevention text — replace "extractor MUST NOT reverse; reversal is reader-side" with the rev 1 §6.5 wording (`iter_aligned()` adapter encapsulates orientation correction in `bismark-io`).
- **NB4**: §2 line 19 + Phase A line 660 — change "34" to "35".

**Important (before Phase A start):**
- I7 leftover: Add `cli_validate_rejects_cytosine_report_without_genome_folder` to §8.1 unit tests; assert the error message contains the §11 substring.
- I9 leftover: Add to §11 open questions whether `--parallel > 4` inherits dedup's soft warning. Decision can wait for Phase F but should be tracked.

**Optional:**
- I4 leftover: Add `extract_calls_zero_length_xm_yields_empty_vec_without_panic` to §8.1.
- NB5: Note that §3 row 27's literal warning string is the byte-stable contract.
- Future Phase D/F unit tests: `mbias_merge_under_worker_skew_is_commutative`; `--mbias_only` / `--mbias_off` path-specific tests.

---

## Closing note

Rev 1 is a substantial improvement over rev 0 — every architectural / correctness finding from both reviewers was addressed at the design level. The remaining issues are editorial leftovers from a rev that touched a lot of text in one pass. A 30-minute cleanup pass + a single re-read of §7.4 and §12 row 4 against the rev 1 §6.5 + §3 narrative should land an APPROVE-clean rev 2. The architecture is locked.
