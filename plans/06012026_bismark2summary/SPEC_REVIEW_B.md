# SPEC Review B — `bismark-summary` (Rust port of Perl `bismark2summary`)

**Reviewer:** B (independent, fresh context; no coordination with Reviewer A)
**Date:** 2026-06-01
**SPEC under review:** `plans/06012026_bismark2summary/SPEC.md` (rev 0)
**Perl source of truth:** `bismark2summary` v0.25.1 (1722 lines), read in full.
**Verdict:** **APPROVE WITH CHANGES.** The SPEC is unusually faithful to the source — every load-bearing claim I spot-checked against the Perl (and several I empirically re-ran in Perl + Rust) holds. The byte-identity contract is essentially complete and correct. The findings below are a small number of precision/edge gaps plus test-matrix reinforcements; none invalidate the design, but three are worth pinning before the PLAN is written.

Where I cite Perl line numbers they are from the source I read; where I ran an experiment I say so.

---

## 1. Logic review

### 1.1 What I empirically validated (all PASS)

I ran four checks because they are the highest-risk byte-identity claims:

1. **The `100 − sprintf("%.2f")` → `%.15g` engine (§2.9a) is faithful — including the worst FP-artifact case.**
   - Perl `100 - "99.99"` stringifies (default scalar) to `0.0100000000000051`, and C `printf("%.15g")` of the same NV gives the identical string. Confirmed in Perl.
   - I compiled a Rust probe: `100.0_f64 - "99.99".parse::<f64>()` produces IEEE bits `3f847ae147ae2000` — **bit-identical** to Perl's `pack("d", 100 - "99.99")` (`3f847ae147ae2000`). So the SPEC's prescription "round to `%.2f` string → re-parse → subtract from `100.0` → `format_g15`" reproduces Perl exactly, *including* this artifact. (`format_g15` shows 15 sig-figs → `0.0100000000000051`.)
   - **This is the single biggest numeric risk in the port, and the SPEC's reproduction recipe is correct.** Good. Keep Spike A — it should explicitly include the `99.99`/`0.01` artifact pair, not only the clean `.00`/`.X0` values §8.2 lists.

2. **Glob mutual-exclusivity + lexical order (§2.3) confirmed in Perl.** `<*bismark_bt2.bam>` does **not** match `c_bismark_bt2_pe.bam` (literal suffix), and the four globs return matches in ASCII-sorted order (`a,b,z`). The SPEC's per-glob-sort + glob-order-concat model is correct.

3. **Mixed literal/`%.2f`/`%.15g` join (§2.9) confirmed.** A sample with 100% CpG meth yields `p_CpG_m = "100.00"` but `p_CpG_u = "0"` in the *same* index — the asymmetry the SPEC flags. Zero-context CpG → `'NA','NA'`; zero-context CHG/CHH → `'0','0'`. All join verbatim. SPEC §2.9/§2.9a capture this exactly.

4. **Regex anchoring asymmetry (§2.5) confirmed against source** (lines 305–311, 330/334/338, 373/377): `total_c` and the dedup "alignments analysed" line are `$`-anchored; the six context meth/unmeth lines and the two dedup count lines are **not**. The SPEC calls this out correctly.

5. **Both `docs/images/` oracles confirmed STALE** (§7): the HTML has **0** `Plotly` tokens and **16** `highchart` tokens (Highcharts era); the `.txt` header reads `Methylated CpHs / Unmethylated CpHs`, while the current source (lines 240–241) emits lowercase `chgs`. The SPEC's "do not use the checked-in oracles; regenerate from current Perl" instruction is correct and load-bearing.

### 1.2 Parsing fidelity (§2.5) — complete, two small precision notes

- The PE/SE pattern table, the dedup `aligned_reads` overwrite (line 331), and the splitting `Total C to T conversions` overwrite (lines 377–379) are all captured accurately, including last-match-wins (`$x = $1 if /.../` scanned over every line). **No field or pattern missed.** I cross-checked all 5 alignment fields × 2 (PE/SE) + 7 context lines + 3 dedup lines + 7 splitting lines against source line-by-line.
- **Precision note (Important):** the SPEC says the context-methylation regexes are "the six meth/unmeth patterns." There are actually **seven** capture lines in each methylation block (`total_c` + 3 meth + 3 unmeth). The prose count "six" (§2.5(a) header note) is loose; the table itself is complete. Cosmetic — but a PLAN author copying the prose could omit `total_c`'s anchoring. Tighten to "the six *context* meth/unmeth patterns are unanchored; `total_c` is `$`-anchored."
- **Precision note (Optional):** §2.5(b) describes the dedup overwrite but does not explicitly state that the dedup block uses three **independent `if`** statements (lines 330, 334, 338) — not `elsif`. This matters only if a single line could match two patterns (it cannot), so behavior is unaffected; worth a one-word note for the implementer to use independent matches.

### 1.3 `.txt` assembly (§2.6) — complete

- Column set/order (15 cols) matches `@csvrow` (387–403) exactly.
- Lowercase `chgs` quirk (cols 12–13) captured; the stale-oracle `CpHs` divergence flagged.
- The **raw-captured-before-mutation** ordering (row appended at 404, *before* 0-defaulting at 412–424) is captured precisely in §2.6/§8.8 — this is the easiest thing to get wrong and the SPEC nails it.
- Trailing `\n` per row incl. header (line 246 seeds `...."\n"`, line 404 appends `."\n"`) captured.
- **No issues.** This is the cheap Phase-A win and the contract is airtight.

### 1.4 HTML assembly order (§2.9) — faithful and complete

I walked the 13-step ordered mutation list against source lines 1378–1711:
- plot.ly greedy/dotall inject + die (1378–1383) ✔
- logos (1384–1385, single subst, no `/g`) ✔
- timestamp `/g` (1386) ✔
- page_title/num_samples/x-values/filenames/version `/g` (1387–1405) ✔
- alignment numbers + `^,{1,}$` raw-vs-dedup section deletion (1412–1454) ✔
- alignment percentages + mirror section deletion (1458–1599) ✔
- methylation raw strings into comment placeholders (1607–1618) ✔
- methylation percentages (1628–1711) ✔

The ordering, the six `{{…_section}}` greedy/dotall deletions, the raw-vs-dedup `^,{1,}$` detection, and the plot.ly die-if-missing are all represented correctly. **One omission flagged below (1.5).**

### 1.5 GAP — `{{aligned_seq}}` / `{{p_aligned_replace}}` survival when `$aligned` is empty (Important)

This is the one place the SPEC is silent and a Rust port could silently diverge.

- In **dedup (WGBS) mode**, `$aligned` is set to `''` (line 1414), and the *raw* trace sections are deleted via the greedy/dotall `{{raw_aligned_reads_section}}.*{{raw_aligned_reads_section}}` (line 1439) and `{{raw_unique_reads_percentage_section}}.*…` (line 1585). **But** the `{{aligned_seq}}` placeholder (line 659, *inside* the raw section) is deleted with the section, while `s/{{aligned_seq}}/$aligned/g` at line 1419 still runs (on a now-absent placeholder → no-op). Likewise `{{p_aligned_replace}}` (line 832) sits inside the deleted raw percentage section, and the fill at line 1591 is **gated by `if ($aligned)`** so it never runs in dedup mode.
- Net effect in Perl: in dedup mode, `{{aligned_seq}}` and `{{p_aligned_replace}}` placeholders are *gone* (deleted with their sections) — no surviving literal `{{…}}`. In raw mode they are filled. **A Rust implementation that does the fills before the deletions, or that fills `{{aligned_seq}}` unconditionally with an empty string in dedup mode, would leave a stray empty `y: []` trace OR a surviving `{{aligned_seq}}` literal** — both byte-divergent.
- **The SPEC's §2.9 step 8 says "Fill `{{aligned_seq}}` … `/g`" without noting that in dedup mode this is a no-op because the placeholder was already deleted, and that the deletion happens at lines 1430–1442 which is *after* line 1419.** Re-read the source order: 1419 (`{{aligned_seq}}` fill) runs **before** 1432–1434 (raw section deletion). So in dedup mode the fill at 1419 *does* fire first (replacing `{{aligned_seq}}` with the empty `$aligned`), and *then* the whole raw section (now containing `y: []`) is deleted at 1434. The end state is the same (no raw trace), but **the mutation order is fill-then-delete, not delete-then-fill.** Because the deletion span (`{{raw_aligned_reads_section}}.*{{raw_aligned_reads_section}}`) is matched by marker text that is NOT affected by the `{{aligned_seq}}` fill, the order is benign here — but the SPEC should state this explicitly so the implementer preserves the exact Perl statement order rather than reasoning about it. **Action:** add a sentence to §2.9 step 8: "the `{{aligned_seq}}` fill (1419) precedes the raw-section deletion (1430–1442); preserve this order. In dedup mode the fill replaces the marker with empty string and the deletion then removes the whole trace — net no raw trace, no surviving literal." Same for `{{p_aligned_replace}}` / `{{unique_alignments}}` (note `{{unique_alignments}}` IS filled `/g` at 1454 in both modes, set to `''` in raw mode at 1449 — captured by §2.9 step 8 last bullet; good).

### 1.6 Edge case — the RRBS+WGBS-mix `die` (line 1489) reachability (Optional, but verify)

The SPEC (§5, §6, §8.12) treats line 1489 as a reachable `die` path and adds a fixture for it. Confirmed reachable: it fires when `$aligned` is truthy (≥1 RRBS/raw sample present in the joined string) **and** some plotted sample has `$aligned_arr[$index] eq ''` (a WGBS/dedup sample whose raw aligned count was blanked at line 417). The `$aligned` truthiness gate at 1486 is the *joined* string `$aligned` (non-empty, non-all-commas), and the per-index check at 1488 catches the blanked WGBS entries. **The SPEC is correct that this die is reachable and needs a fixture.** Good — many ports would mistake it for dead code.

### 1.7 Latent CHH `total_CHG==0` bug (line 1662) — correctly handled

§2.9 step 11 and §8.11 both flag that line 1662 tests `total_CHG` (not `total_CHH`) and instruct reproducing it verbatim. The SPEC's reasoning that it is effectively dead (plot-exclusion at 433–437 guarantees all three totals > 0 for plotted samples) is **correct**: a plotted sample passed the CHG-exclusion gate (line 433) so `total_CHG > 0`, so the buggy branch's condition is false and the `else` always runs. Reproduce verbatim anyway (free, and protects against a future where exclusion logic changes). Well handled.

---

## 2. Assumptions

### 2.1 Validated stated assumptions

- **"No BAM is ever opened"** — correct; only `substr($bam,0,-4)` filename math (line 251). The standalone, no-`bismark-io`/noodles decision (§3) is right.
- **"Plot-exclusion affects graphs only; `.txt` row already written"** — correct (row at 404, `next` at 431/435/439). Captured in §2.7/§5.1.
- **`num_samples` (total, incl. excluded) vs y-array length (plotted)** — correct: `$num_samples = scalar @bam_files` (247) is set before the loop and never decremented; x-values uses it (1394) while y-arrays are built only for non-`next`-ed samples. The SPEC flags the resulting length mismatch (§2.9 step 6, §8.10) — faithful to Perl, reproduce as-is.

### 2.2 Assumption needing an explicit decision — glob sort locale (Important)

§2.3 says "use `LC_ALL=C`-equivalent ordering — a plain `Vec<String>` `sort()`." I confirmed Perl's `glob` returns **ASCII/codepoint** order (not locale-collated) on the test box. A Rust `Vec<String>::sort()` is bytewise (codepoint for `&str`) which matches **provided filenames are ASCII** (Bismark BAM names always are). **But** Perl's `glob`/`csh`-style sorting is actually locale-sensitive in some Perl builds (it uses `Sort_csh`/`Sort_words` under `bsd_glob`, which can honor `LC_COLLATE`). On the benchmark box this is moot (ASCII names, C-ish locale), but the SPEC should **pin the assumption**: "Bismark BAM filenames are ASCII; Rust bytewise sort == Perl glob order under any C/POSIX-or-ASCII locale. We do NOT attempt to replicate locale-collated glob order (would only matter for non-ASCII filenames, which Bismark never produces)." This converts an implicit assumption into a documented, defensible divergence boundary. Low practical stakes, but it is exactly the kind of thing that bit genomeprep (per the memory note) and should be explicit.

### 2.3 Assumption to surface — `substr($bam,0,-4)` on a non-`.bam` argv entry (Optional)

§2.4 documents that `substr($bam,0,-4)` strips the last 4 chars unconditionally, so a non-`.bam` explicit argv entry "loses its last 4 chars." Correct. Worth one fixture asserting an explicit-argv path with an oddly-named file, but this is genuinely a user-error path; low priority. The `.txt` row uses `$bam` verbatim (column 1, line 388) — *not* `$base` — so the file column is unaffected by the strip; only the *report-filename derivation* uses `$base`. The SPEC implies this but does not state that column 1 is the raw `$bam`. **Add to §2.6:** "column 1 (`File`) is the raw `$bam` string (line 388), not the stripped `$base`." Minor.

---

## 3. Efficiency analysis

Not a concern for this port. Inputs are a handful of tiny text reports; the 3 MB plot.ly is `include_str!`'d once. The only loops are over samples (tens, not millions) and over plot.ly bytes (one greedy regex equivalent). The SPEC's "first-index…last-index-of-second-marker splice" for section deletion (§8.5) is O(n) over the document per marker (6 markers × ~3 MB) = trivial. **No scalability or memory concerns.** The `format_g15` helper is the only hot-ish path and runs ≤ `6 × num_samples` times — negligible.

One micro-note (Optional): the SPEC copies `fmt_g.rs` wholesale. That file's doc-comment references bedGraph internals (`:399`/`:601`) and "2M+ fractions" — irrelevant here. Fine to copy verbatim (duplicate-not-couple per O2), but update the module doc-comment to reference `bismark2summary §2.9a` so a future reader isn't confused. Cosmetic.

---

## 4. Validation sufficiency (§7) — strong, with three reinforcements

The matrix (PE/SE × WGBS-dedup/RRBS-raw × splitting present/absent × plot-excluded × multi-sample × argv-vs-glob × `--title` spaces × RRBS+WGBS-mix-die) is **comprehensive and covers every branch I traced.** The stale-oracle handling (regenerate from current Perl, auto-skip if absent) is correct. Reinforcements:

### 4.1 (Important) Add a fixture that exercises the `%.15g` artifact, not just clean percentages
The §7 fixtures describe "2 WGBS-PE, 1 WGBS-SE, 1 RRBS-SE, 1 zero-CHH." None is specified to produce a meth percentage whose `100 − %.2f` triggers a **non-trivial `%.15g` tail** (e.g. `meth/total` giving `99.99` → unmeth `0.0100000000000051`, or `12.30` → `87.7` trailing-zero drop). A fixture could pass with all-clean `.00`/`.50` values and still hide a `format_g15` wiring bug. **Action:** pin at least one sample's CpG counts so `p_CpG_m` is e.g. `99.99` or `12.30`, asserting the unmeth array shows the dropped-trailing-zero / FP-artifact form. (Spike A covers the unit level; this ensures the *integration* golden also exercises it.)

### 4.2 (Important) Add an all-RRBS (raw-mode) golden AND assert section *presence*, not just `.txt`
The matrix has "1 RRBS-SE" mixed with WGBS, but the **mix dir hits the die**. To exercise the **raw-mode HTML section deletion** (lines 1434/1581 — keep raw trace, delete dedup+dup) you need a directory that is **all-RRBS / all-raw** (no dedup reports anywhere) so `$aligned` stays truthy and the report actually renders. Without it, the raw-mode `{{raw_aligned_reads_section}}`/`{{p_aligned_replace}}`/`{{raw_unique_reads_percentage_section}}` fill paths and the dedup-section *deletions* are never byte-checked. **Action:** add an "all-RRBS, ≥2 samples" fixture and assert (a) the raw aligned trace is present, (b) the dedup/dup sections are deleted, (c) `{{p_aligned_replace}}` is filled. Symmetrically, the existing WGBS fixtures cover dedup mode; make sure at least one WGBS golden asserts the **raw** sections were deleted and the dedup+dup sections kept.

### 4.3 (Optional) Assert the x-values/y-array length mismatch survives in a plot-excluded golden
The zero-CHH fixture should assert that `{{num_samples}}` and `{{x_values_*}}` reflect the **total** sample count while `categories`/y-arrays reflect the **plotted** (smaller) count — i.e. the mismatch the SPEC promises to reproduce (§2.9 step 6). A naive implementation that derives `num_samples` from the plotted array would pass every *other* test but fail here. The fixture exists; just make the assertion explicit.

### 4.4 (Optional) Empty/degenerate inputs
- **Zero plotted samples but ≥1 `.txt` row** (every sample excluded for a missing context): `@aligned_arr` etc. are empty → joins are `""` → `^,{1,}$` does **not** match `""` (it requires ≥1 comma) → falls into the **dedup-mode `else`** branch (1437) regardless. Then the percentage loop `0..$#aligned_arr` over an empty array doesn't iterate. Worth one fixture to confirm the Rust port takes the same branch on empty joins (the `^,{1,}$` vs empty-string distinction is a classic off-by-one in regex porting). **Verify the Rust `^,{1,}$` equivalent returns false for `""`** (it should: one-or-more commas).
- Single-sample dir: `categories`/arrays length 1, all joins comma-free → `^,{1,}$` false → dedup branch. Cheap to add.

### 4.5 Stale-oracle handling — correct, but add a guard test
§7/§8.1 correctly say "regenerate from current Perl, auto-skip if perl absent." **Add a CI/test assertion that fails loudly if someone wires the committed `docs/images/` files in as the oracle** (e.g. a test that greps the committed `.html` for `Plotly` and asserts 0 → so nobody mistakes it for current). This is the single biggest trap per §8.1; a tripwire is cheap insurance.

---

## 5. Alternatives

1. **`format_g15` reproduction vs. a direct `%.15g` via a C-FFI/`libc`.** The SPEC copies the validated Rust `fmt_g`. This is the right call — it's already validated against C across 2M fractions, avoids an FFI dependency, and I independently confirmed it matches Perl on the FP-artifact case. No change recommended.

2. **Section deletion: regex crate vs. manual index splice.** The SPEC chooses manual first/last index splice (§8.5) to mirror Perl's greedy/dotall `s/m.*m//s`. This is correct and matches the `bismark2report` decision. An alternative — the `regex` crate with `(?s)` dotall — would also work and be more obviously faithful, but pulls a dependency and risks `.*` catastrophic-ish backtracking on 3 MB (unlikely but real). **Manual splice is the better choice; keep it.** Just ensure the splice uses **last** occurrence of the second marker (greedy), not the second occurrence — for these markers (exactly 2 each) they're the same, but state "last" so a future template with 3 markers doesn't silently change behavior.

3. **Duplicate parsers vs. couple to `bismark-report` (O2).** SPEC duplicates (Felix's decision). Given `bismark-report` is unmerged and the parsers are ~30 lines, duplication is right for v1.0. The promotion note is recorded. Agree.

4. **Embedded template vs. read-from-`$RealBin` (§4.2).** Embedding the heredoc as a checked-in `.html` + a source-extraction drift-guard test is the right pattern and matches all prior ports. One alternative worth a sentence: rather than checking in a *separate* extracted `.html`, the drift-guard test could `include_str!` the Perl source directly and slice lines 490–1371 at test time — eliminating the second copy entirely. Minor; either is fine.

---

## 6. Action items

### Critical
*(none — the byte-identity contract is sound; no finding blocks proceeding to PLAN.)*

### Important
1. **§2.9 step 8 — pin the fill-then-delete order for `{{aligned_seq}}` / `{{p_aligned_replace}}`** (finding 1.5). State that the `{{aligned_seq}}` fill (line 1419) precedes the raw-section deletion (1430–1442), and that `{{p_aligned_replace}}` fill is gated `if ($aligned)` (1591) so it never fires in dedup mode. Instruct the implementer to preserve exact Perl statement order rather than reasoning that order is benign. This is the one place a reasonable Rust implementation could leave a stray empty trace or a surviving literal.
2. **§7 — add an all-RRBS (raw-mode) multi-sample golden** that asserts raw-section *presence* and dedup/dup-section *deletion* (finding 4.2). The current matrix only renders raw-mode via the die path or mixed with dedup; the pure raw-mode HTML deletion branch (1434/1581) is otherwise never byte-checked.
3. **§7 — pin a fixture whose `100 − %.2f` produces a non-trivial `%.15g` tail** (e.g. `99.99` → `0.0100000000000051`, `12.30` → `87.7`) so the integration golden exercises the asymmetric unmeth formatting, not just clean `.00`/`.50` values (finding 4.1).
4. **§2.3 — make the glob-sort-locale assumption explicit** (finding 2.2): Bismark filenames are ASCII; Rust bytewise sort == Perl glob order under C/POSIX/ASCII; we do not replicate locale-collated glob order. Documented divergence boundary.

### Optional
5. **§2.5 — tighten the "six patterns" prose** to "six *context* meth/unmeth patterns are unanchored; `total_c` is `$`-anchored" so a PLAN author can't drop `total_c`'s anchor (finding 1.2). Note the dedup block uses three independent `if`s, not `elsif`.
6. **§2.6 — state that column 1 (`File`) is the raw `$bam`, not the stripped `$base`** (finding 2.3).
7. **§7 — add a tripwire test** that greps the committed `docs/images/*.html` for `Plotly` and asserts 0, so the stale oracle can never be silently re-adopted (finding 4.5).
8. **§7 — add single-sample and all-excluded (zero plotted) fixtures**; verify the Rust `^,{1,}$` equivalent returns false for the empty string and thus takes the dedup-mode `else` branch (finding 4.4).
9. **§3 — update the copied `fmt_g.rs` module doc-comment** to reference `bismark2summary §2.9a` instead of bedGraph internals (finding 3, last para).
10. **§8.5 — say "last occurrence of the second marker" (greedy)** explicitly for the splice, future-proofing against a template with >2 markers (finding §5.2).

---

## 7. Summary

The SPEC is high quality and faithful to the source — materially stronger grounding than typical. I empirically confirmed the three riskiest claims in Perl + Rust: the `100 − %.2f` → `%.15g` engine is **bit-exact** between Perl and the proposed Rust recipe (including the `99.99 → 0.0100000000000051` FP artifact); glob mutual-exclusivity and lexical ordering hold; and both checked-in oracles are genuinely stale (Highcharts HTML, `CpHs` txt). The one logic gap worth pinning is the **fill-then-delete order of `{{aligned_seq}}`/`{{p_aligned_replace}}`** in dedup mode (§2.9 step 8) — benign in Perl only because of statement order, so the PLAN must mandate preserving that order. The test matrix is comprehensive but should add a **pure raw-mode (all-RRBS) HTML golden** and a fixture that **exercises a non-trivial `%.15g` tail**, plus an explicit glob-sort-locale assumption. No Critical blockers; APPROVE WITH CHANGES.
