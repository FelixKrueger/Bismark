# SPEC Review — `bismark-extractor` SPEC rev 1 — Reviewer A

**Target:** `/Users/fkrueger/Github/Bismark/rust/bismark-extractor/SPEC.md` (rev 1, branch `rust/extractor-recon`, PR #841).
**Reviewer:** A (independent, fresh context).
**Verdict:** **APPROVE-WITH-NITS** — rev 1 closes every Critical finding from rev 0 with the right structural moves; remaining issues are local consistency drift and one stale row in §12. Implementation can proceed in parallel with the SPEC patch landing the nits.

---

## 1. Verification of the rev 1 fixes

### 1.1 §6.5 XM/CIGAR reversal (Critical, rev 0)

**Verdict: correct and well-reasoned.**

Perl source spot-checked at the cited lines:

- Line 1619-1621 (SE): `if ($strand eq '-') { $meth_call = reverse $meth_call; }` — confirmed.
- Line 2877-2886 (PE R2): `if ($strand eq '-') { ... @comp_cigar = reverse@comp_cigar; ... }` with the inline comment "the methylation call has already been reversed above" — confirms BOTH XM and CIGAR are reversed for `-` strand records.

The rev 1 text correctly identifies the original-read 5' orientation as the basis for M-bias accumulation (otherwise M-bias positions would be end-to-end-flipped for every `-` strand record). Option (b) — `iter_aligned() -> impl Iterator<Item = (read_pos_5p, ref_pos, xm_byte)>` — is the right choice over (a): consolidating the reversal in `bismark-io` (i) avoids two parallel `xm_read_oriented()` / `cigar_read_oriented()` accessors that consumers must remember to call together, (ii) leaves dedup (which doesn't read XM) untouched, and (iii) gives future consumers (bedGraph, coverage2cytosine) the same corrected stream.

### 1.2 §7.4 overlap comparator (Important, rev 0)

**Verdict: acceptable but suboptimal.**

The "Phase C verification blocker" note is fine in principle — but the polarity is resolvable RIGHT NOW from the Perl source I just read:

- Line 2905 (forward `+` strand, R2 downstream): `if ($start+$index+$pos_offset >= $end_read_1) { return; }` → SKIP test is `>=` → KEEP predicate is `<` (strict).
- Line 2989 (reverse `-` strand, R2 upstream): `if ($start-$index+$pos_offset <= $end_read_1) { return; }` → SKIP test is `<=` → KEEP predicate is `>` (strict).

Both polarities are **strict**, so SPEC §7.4's first pseudocode block (lines 357-368) is correct and the "Wait — re-read carefully" digression (lines 370-388) is muddled scratch-pad text that should not be in a SPEC. It contradicts itself within ~15 lines and ends with a confused "Hmm" punt. **Nit:** rewrite §7.4 to keep only the first pseudocode block, state the polarities are strict `<` and `>` for KEEP (or equivalently `>=` and `<=` for SKIP, matching Perl), and downgrade the "Phase C verification blocker" to a one-line "implementation must cite the Perl line in a comment next to the comparator." The current text reads like an unresolved internal monologue.

(The endpoint semantics — what `$end_read_1` actually means on the `-` strand — IS worth a Phase C verification gate, because Perl computes it via earlier corrections; but the comparator polarity itself is settled.)

### 1.3 §7.1 invalid XM byte (Important, rev 0)

**Verdict: sound.**

Perl lines 2972 and 3054 confirmed: `die "The methylation call string contained the following unrecognised character: $methylation_calls[$index]\n" unless($mbias_only);` — both occurrences fail loudly except in `--mbias_only` mode.

The rev 1 plan (`Err(InvalidXmByte)` + `?` propagation + `cleanup_partial_output_on_err` from Phase B dedup precedent) mirrors the Perl semantics correctly and is consistent with the dedup port's existing error-handling pattern.

**Nit:** the SPEC should explicitly state that `classify_xm_byte` returns `Ok(SkipUnknownContext)` (NOT `Err`) on `U`/`u` even outside `--mbias_only`, because Perl skips silently there (line 2971: `elsif (lc$methylation_calls[$index] eq 'u'){}`). The rev 1 §7.1 pseudocode at line 295 does this correctly; just confirm in the prose at line 285 that `--mbias_only` only affects the `die`-vs-silent decision for the `other` arm, not for `U`/`u`/`.`. (Minor wording polish.)

### 1.4 §3 corrections

All verified against the Perl source:

- `--fasta` at line 5040: `if ($genomic_fasta) { print REPORT "Genomic equivalent sequences will be printed out in FastA format\n"; }` — confirmed splitting-report-only.
- `--ignore_3prime` / `--ignore_3prime_r2` at lines 989-990: `'ignore_3prime=i' => \$ignore_3prime, 'ignore_3prime_r2=i' => \$ignore_3prime_r2,` — confirmed.
- Flag count 35: I counted 35 distinct `=>` GetOptions entries in lines 959-993. Confirmed (`grep -c '=>'` returns 35).
- `--samtools_path` accepted-with-stderr-warning — matches dedup port precedent.

### 1.5 §2 vs §3 `--CX_context` contradiction (Important, rev 0)

**Verdict: correct resolution.**

`--CX_context` is a flag passed through to `coverage2cytosine`, not a flag the extractor honours itself. §3 row 24 lists it as "`--cytosine_report` only; runtime ↑↑." which only makes sense if it's in-scope for v1.0 via subprocess pass-through. Declaring it in-scope (§2 rev 1 correction) resolves the contradiction with §3.

### 1.6 §8 test surface strengthening

**Verdict: strong — closes all three bug classes the rev 0 reviewers flagged.**

- **Alan's strand-routing bug** (one read split across files): closed structurally via §6.1 + the new `route_call_default_mode_routes_to_strand_specific_file` unit test + the integration test on the synthetic CHG/CHH-rich fixture + the new directional-library edge-case fixture asserting CTOT/CTOB files are 0-byte.
- **Missing CHG/CHH M-bias context tables**: closed by the four new `mbias_accumulate_routes_to_{chg,chh}_table_for_{X,x,H,h}_byte` unit tests + the `mbias_writer_emits_six_sections_for_pe` writer test + the `M-bias.txt` byte-equality assertion in §8.3.
- **XM-reversal drift**: closed by §6.5's `iter_aligned()` move (consumers can't accidentally skip the reversal), plus the §8.3 unsorted-byte-equality gate at N=1 (which would catch positions flipped end-to-end on `-` strand reads).

The §8.3 strengthening (unsorted byte-equality at N=1 + sorted-md5 at N=4 + explicit "N=4 byte-identical to N=1" assertion) is exactly right. Reviewer B's concern that sorted-md5-alone would hide line-reordering bugs is correctly addressed.

### 1.7 §8.4 edge cases

**Verdict: adequate.**

Five new edge cases (directional library, non-directional library, cross-chr pair, mixed-strand pair, invalid XM) all map to specific failure modes from rev 0's review. The directional-library fixture is the most valuable addition — it directly closes Alan's "spurious CTOT/CTOB files for directional data" bug at the integration level.

One **gap nit**: there's no edge-case fixture for `--mbias_only` on a BAM containing an invalid XM byte. Per Perl `die "..." unless($mbias_only);` — in `--mbias_only` mode the invalid byte should be silently skipped (no error). The Rust port's `classify_xm_byte` returns `Err(InvalidXmByte)` unconditionally; the caller must check the mode. This branch is testable and worth a single unit test (`classify_xm_byte_under_mbias_only_skips_invalid_silently` or similar).

### 1.8 §11 open questions

**Verdict: clean.** Buffering, samtools_path, CX_context resolved. `--genome_folder` policy locked. The remaining open items (subprocess-vs-inline, --fasta, M-bias PNG, SE/PE auto-detection, CHANGELOG strategy) are all real open decisions with sensible defaults.

**New open question NOT documented:** what does the Rust port emit on `U`/`u` XM bytes when `--mbias_only` is set vs unset? Perl silently skips in both modes (lines 2971, 3052). The §7.1 `classify_xm_byte` does the right thing (`Ok(SkipUnknownContext)` for both), but the SPEC should explicitly call this out — the rev 0 reviewers may have been confused about whether the rev 1 invalid-XM-byte error path swallows `U`/`u` too.

---

## 2. Cross-section consistency issues

### 2.1 §6.5 `iter_aligned()` does NOT match §7.1's pseudocode — **important nit**

§6.5 declares the v1.0 plan: `bismark-io 1.0.0-beta.6` adds `iter_aligned() -> impl Iterator<Item = (read_pos_5p, ref_pos, xm_byte)>` which yields already-orientation-corrected triples. §7.1's `extract_calls` pseudocode then…

…ignores `iter_aligned()` entirely. It still does its own CIGAR walk + XM indexing (`let b: u8 = xm[read_pos as usize]`), uses `record.xm()` (the raw, unreversed accessor), and has **no `-` strand reversal logic**.

If `iter_aligned()` is the locked plan, §7.1 should read approximately:

```text
for (read_pos_5p, ref_pos, b) in record.iter_aligned():
    if read_pos_5p >= lo && read_pos_5p < hi:
        match classify_xm_byte(b):
            Ok(MethylationCall(ctx, methylated)) => calls.push(MethCall { ref_pos, read_pos: read_pos_5p, context: ctx, methylated }),
            Ok(SkipUnknownContext) | Ok(SkipNonCytosine) => continue,
            Err(e) => return Err(e),
```

…and the CIGAR-walking machinery moves into `bismark-io::BismarkRecord::iter_aligned()`. As written, §7.1's pseudocode looks like it's reimplementing the very thing §6.5 says belongs in `bismark-io`. **Phase A must reconcile this** — either §6.5 punts the iterator to v1.x and §7.1 stays as-is plus an explicit "for `-` strand, walk XM in reverse" branch, OR §7.1 is rewritten around `iter_aligned()`.

### 2.2 §7.7 `MethCall { read_pos }` field shape

Minor: §6.5 (b) yields `read_pos_5p`; §7.7's `MethCall.read_pos` is documented as "0-based read position (post-CIGAR walk)" without specifying orientation. Once §6.5 lands, `read_pos` will inherently be 5p-oriented (because that's what `iter_aligned()` yields). The SPEC should clarify the comment in §7.7 to say "5' read position (0-based, in original sequenced-read orientation)" — otherwise an implementer might preserve the BAM-stored orientation.

### 2.3 §12 has a stale row contradicting §6.5 — **important nit, must fix before merge**

§12 (Structural pitfalls catalog) line 695:

> | CIGAR string reversal for `-` strand (risk of double-reverse) | Perl 1619-1621, 1933-1939, 2877-2886, 4422-4425 | §6.5: extractor MUST NOT reverse; reversal is reader-side. |

The "extractor MUST NOT reverse" text is the rev 0 claim that §6.5 explicitly retracted at line 212 ("The original rev 0 claim that 'the extractor MUST NOT reverse' was wrong"). The §12 row needs to be rewritten to match the new §6.5 plan, e.g.:

> | CIGAR string reversal for `-` strand (risk of double-reverse) | Perl 1619-1621, 1933-1939, 2877-2886, 4422-4425 | §6.5: `bismark-io::BismarkRecord::iter_aligned()` yields 5'-oriented `(read_pos, ref_pos, xm_byte)` triples; the extractor never sees the reversal directly. |

Leaving it as-is would perpetuate the very bug §6.5 just fixed if a future reader uses §12 as the canonical guidance.

### 2.4 Flag count drift — **nit**

Three places still say "34":

- §2 line 19: "All 34 Perl CLI flags (per §3 inventory)" — should be 35.
- §10 Phase A row line 660: "`--help` prints all 34 flags" — should be 35.

§3 line 36 + line 78 + §14 rev 1 entry all say 35. Self-inconsistent. Trivial s/34/35/g fix.

---

## 3. §14 revision history completeness

**Verdict: complete except for two omissions worth noting.**

The rev 1 entry covers all the substantive changes (§6.5, §7.1, §7.4, §3 corrections, §5 table row, §7.7 buffering, §8.1 new tests, §8.3 strengthening, §8.4 new edge cases, §11 resolutions, dedup-stripping).

**Undocumented in the rev 1 history entry:**

1. §2 line 19 "34 flags" and §10 Phase A "34 flags" mentions were NOT updated to 35 — the history entry says flag count was corrected, but two call-sites weren't fixed.
2. §12 "extractor MUST NOT reverse" row was NOT updated — the history entry says §6.5 was rewritten but doesn't mention the stale §12 cross-reference.

Neither is a logic bug; both are local consistency-drift items the rev 2 patch should sweep.

---

## 4. Action items

### Important (fix before SPEC freeze for implementation)

1. **§12 row 4 (CIGAR reversal pitfall)**: rewrite to match §6.5 — current text contradicts the §6.5 correction and would mislead future readers/implementers.
2. **§7.1 pseudocode vs §6.5 `iter_aligned()`**: reconcile. Either rewrite §7.1 around `iter_aligned()` or explicitly note §7.1's pseudocode pre-dates `iter_aligned()` and document the `-` strand reversal branch.
3. **§7.4 overlap polarity**: replace the muddled "Wait — re-read carefully" + alternate-pseudocode + "Hmm" punt with the resolved comparator (KEEP `<` for forward / KEEP `>` for reverse; equivalently SKIP `>=` / `<=` matching Perl 2905/2989). Keep the Phase C verification gate for the endpoint semantics (`$end_read_1` derivation), not for the polarity.

### Optional (nice-to-have polish)

4. §2 line 19 + §10 Phase A: s/34 flags/35 flags/.
5. §7.7 `MethCall.read_pos` comment: clarify it's the 5'-oriented read position (in original sequenced-read coordinates), not the BAM-stored orientation.
6. §7.1 prose at line 285: explicitly state that `U`/`u`/`.` are skipped silently in BOTH `--mbias_only` and normal mode (Perl 2971, 3052); only the `other` arm's behaviour depends on `--mbias_only`.
7. §8.1: add `classify_xm_byte_under_mbias_only_skips_invalid_silently` unit test for the Perl `die "..." unless($mbias_only)` branch.
8. §11: add an explicit "U/u XM byte semantics under --mbias_only" open question OR confirm in §7.1 prose that it's identical to normal mode.

### Critical

None. All rev 0 Critical findings are correctly closed structurally.

---

## 5. Summary

Rev 1 is a strong patch. The §6.5 reversal correction, §7.1 invalid-XM error path, §7.4 polarity note, §3 flag-count and `--fasta` corrections, §2 vs §3 `--CX_context` resolution, and §8 test-surface strengthening all address the rev 0 reviewers' Critical and Important findings with the right structural moves. Verification against Perl source confirms every cited line.

What's left is local consistency drift: §12 has a stale "extractor MUST NOT reverse" row that contradicts the new §6.5, §7.1's pseudocode wasn't updated to consume `iter_aligned()`, §7.4's overlap pseudocode includes an unresolved scratch-pad digression, and two call-sites still say "34 flags". None of these are logic bugs — they're SPEC hygiene items the rev 2 patch should sweep before implementation kicks off, so Phase A doesn't pick up the stale guidance and re-implement reversal in the extractor.

**Verdict: APPROVE-WITH-NITS.** Implementation can begin in parallel with the rev 2 patch.
