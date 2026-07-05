# Plan review — Phase H sub-gate 1 PE (Reviewer B)

**Target:** `plans/05262026_bismark-extractor/PHASE_H_PE_PLAN.md` (rev 0, 2026-05-28)
**Reviewer:** B (independent; fresh context window)
**Companions consulted:** `PHASE_H_SE_PLAN.md` rev 3, `scripts/phase_h_se_matrix.sh` (660+ LOC), `scripts/phase_h_smoke.sh`, `RELEASE_CHECKLIST.md` (referenced), `PROGRESS.md`.

---

## Verdict (TL;DR)

**APPROVE-WITH-NITS.** 0 Criticals. 5 Importants worth folding before merge. The plan correctly inherits SE rev 3's hardening and is internally consistent; the gaps are around (a) one ambiguous fail-open default, (b) an unspecified file format that will end up freelancing, (c) one unstated decision about cross-N timing when a cell already failed, and (d) a structural inconsistency in cell-naming convention vs SE that hurts the v1.0 evidence-walk ergonomics.

---

## Critical findings

**None.** The plan is structurally sound: byte-identity gate is enforced by per-cell `phase_h_smoke.sh` (which already does the strict-cmp on M-bias + splitting-report), and the cross-N raw-byte check directly tests Phase F's contract on PE shape. The (D, N=1) byte-locked baselines (11,443 B M-bias; 875 B splitting-report) are explicit HARD-FAILs. No release-blocking gaps detected.

I deliberately re-checked the rev-3 absorptions and the M-bias gate logic (which was the SE plan's Critical/High consensus finding) — the PE plan picks them up verbatim with the right wording (§3.3.2 #10 explicitly notes `MBIAS_BASELINE_OK=0` fail-closed default; §10 references B-L1 with `R*100/P` direction). Nothing material is missing at Critical severity.

---

## Important findings

### B-Imp-1. `ROW_COUNT_OK` initialisation policy is unspecified, and the PE differential is harder to fail-close than SE's

**Location:** §3.3.2 (pre-flight initialisation), §3.3.5 (row-count differential block).

**Issue:** The plan explicitly initialises `MBIAS_BASELINE_OK=0` (fail-closed, per SE rev 3 absorption). It does NOT specify the initialisation policy for the row-count differential bookkeeping variable (call it `ROW_COUNT_OK`). The SE driver initialises `ROW_COUNT_OK=1` (fail-open) on the rationale that the gate-applies check guards against missing data. That works for SE because all four assertions are unidirectional (`<D`), so the all-zero-rows degenerate case is caught by `MBIAS_BASELINE_OK`'s 5712 B check.

PE adds an *additional* direction: `overlap > D`. Three assertions check `<D`; one checks `>D`. If the `overlap` cell's M-bias file is missing or unreadable (so `count_mbias_rows` returns empty / 0), the `>D` assertion will silently fail-open under SE's `ROW_COUNT_OK=1` init pattern — UNLESS the check is written to fail on empty. The plan §3.3.5 doesn't specify how empty/missing-file is handled for `overlap`, and §3.5 only documents the `overlap < D` failure mode (which is the opposite bug — overlap producing too few rows). The "overlap produced 0 rows because the file disappeared" mode is unhandled in the spec.

**Fix:** §3.3.5 should add one explicit clause: "If any of the five cells' M-bias file is missing or unreadable at row-count-time, `ROW_COUNT_OK` is forced to 0 with detail `[FAIL: <cell> M-bias.txt unreadable]`. This guards against the asymmetric `>D` failure mode." Alternatively: initialise `ROW_COUNT_OK=0` (fail-closed; flip to 1 only on positive completion of all four assertions). The second is cleaner and mirrors `MBIAS_BASELINE_OK`.

**Severity:** Important. Same kind of fail-open bug class as the SE rev-3 Critical that landed B-L2; PE's `overlap > D` direction reintroduces the symmetry SE doesn't have.

---

### B-Imp-2. `row_count_diff.txt` has no specified format

**Location:** §3.3.5 + §4.1 outputs + §9.2 colossal validation.

**Issue:** The plan emits `<OUT>/row_count_diff.txt` as a separate file (§4.1) and the §9.2 release-gate table tells the operator to "Read `row_count_diff.txt`" and confirm "All 4 directional assertions PASS". But §3.3.5 only enumerates the four assertions; it never specifies the file's serialisation format. Is it markdown table? Key=value? CSV? Free text? The SE driver emits the equivalent inline in `speedup_table.md` + `matrix_verdict.txt` and does NOT have a standalone `row_count_diff.txt` file (verified by scanning `phase_h_se_matrix.sh`). PE introduces a new artifact that SE didn't have, with no schema, no shape, and no example.

This is consequential because §9.2 makes it part of the release-gate manual verification step. A release engineer scanning the evidence on colossal needs to know what to grep for.

**Fix:** Pick one and spec it. Recommend a 6-line plain-text format consistent with `matrix_verdict.txt`'s style:

```
Phase H PE row-count differential (M-bias, N=1)
Generated: <ISO-8601>
D=142  r1_5p=132 (<D)  r2_5p=132 (<D)  r1r2_3p=122 (<D)  overlap=168 (>D)
Verdict: PASS (or FAIL: <cell> not <direction> D)
```

Or: drop the standalone file entirely and emit only inline in `speedup_table.md` + `matrix_verdict.txt`, matching SE's pattern. The latter is structurally simpler and avoids divergence from SE.

**Severity:** Important. Not a correctness issue but a "release engineer scanning evidence" ergonomics issue, and the file is referenced as a release-gate input.

---

### B-Imp-3. Cell-id naming convention diverges from SE without justification

**Location:** §3.2 (`cell_p<N>_<CELL_ID>` with `CELL_ID ∈ {D, r1_5p, r2_5p, r1r2_3p, overlap}`); §5.2 #7.

**Issue:** SE uses parameter-encoded directory names: `cell_p<N>_i<5p>_i3<3p>` (e.g., `cell_p1_i0_i30`, `cell_p4_i250_i0`). PE uses mnemonic names: `cell_p1_D`, `cell_p4_overlap`. Both are defensible in isolation, but the v1.0 release-walk evidence tree will contain *both* trees side-by-side on colossal — one SE matrix output and one PE matrix output. A release engineer reading the evidence side-by-side has to mentally translate between the two conventions.

Worse: the PE mnemonic `r1r2_3p` is arguably more readable than SE's `i0_i3_5p_5p` would have been, so this isn't a clean "make PE match SE" recommendation. The SE convention has its own pain (5712 B vs 11443 B is implicit from the dirname only if you know SE has no `--ignore_r2`).

**Fix (pick one):**
- **(a)** Document the convention asymmetry in §11 reviewer-attention magnets explicitly: "PE uses mnemonic cell-ids (`D`, `r1_5p`, `r2_5p`, `r1r2_3p`, `overlap`) where SE used parameter-encoded names (`i<5p>_i3<3p>`). Defensible because PE's flag space is 5-dimensional (including the bool `--include_overlap`), making parameter-encoded names unwieldy." This is a one-paragraph fold that closes the question.
- **(b)** Use parameter-encoded names for PE consistency: `cell_p<N>_i<r1>_ir2<r2>_i3<r1_3p>_i3r2<r2_3p>_ov<0|1>`. Much uglier. Not recommended.

I recommend (a) — it's a one-line documentation fold, not a code change.

**Severity:** Important. Not a correctness bug; matters for release-engineer ergonomics during the v1.0 evidence walk.

---

### B-Imp-4. Cross-N check behaviour when the cell already failed byte-identity is unspecified

**Location:** §3.6 step 2 sub-loop ("Cross-N comparison (§3.3.4) across the N values just run for this CELL_ID"); §3.3.4.

**Issue:** Per-cell sub-loop ordering means: run cell at N=1 → run cell at N=4 → cross-N compare. If the N=4 invocation FAILs the byte-identity check vs Perl (smoke exits 1), what happens to the cross-N step? Two reasonable behaviours:

- **(i)** Skip cross-N: the cell is already in FAIL state; cross-N adds no information. Cleaner verdict signal.
- **(ii)** Run cross-N anyway: a cross-N failure orthogonally confirms it's a Phase F regression (not just a Phase B-or-later issue). More diagnostic.

SE driver behaviour (verified by reading lines 200-300 of `phase_h_se_matrix.sh`): the cross-N loop runs unconditionally; cells with FAIL verdict still get cross-N-compared. That's behaviour (ii). The PE plan doesn't state which is intended. Given the plan claims "structural mirror of SE", behaviour (ii) is the implicit default — but a reviewer or implementer might reasonably pick (i).

**Fix:** §3.3.4 or §3.6 should add one explicit sentence: "Cross-N comparison runs unconditionally per cell, even if the cell's byte-identity verdict vs Perl is FAIL — this preserves diagnostic signal (a simultaneous cross-N failure points specifically at Phase F's worker-reduce path)."

**Severity:** Important. Affects the implementer's choice and could result in a behaviour-different-from-SE driver if the implementer picks (i).

---

### B-Imp-5. `--include_overlap` cell on a hypothetical non-overlapping BAM has unspecified failure semantics

**Location:** §11 Open finding #4 ("If a future BAM lacks such reads, this differential check would falsely fire"); §3.3.5 row-count differential; §3.4 #6.

**Issue:** §11 #4 correctly identifies the fragility: if the BAM has near-zero overlap fraction, `overlap` may equal `D` (not strictly `>D`), and the assertion fails. The plan calls this an "Open" reviewer-attention magnet but doesn't specify the matrix-verdict shape when this fires:

- Is it a "byte-identity PASS but differential FAIL" (exit 1)?
- Or is it a "differential check INAPPLICABLE on this BAM" (informational; exit 0)?

The plan's exit-code mapping (§3.3.6) only has 0/1/2/3 — there's no equivalent of "test inapplicable to this fixture". So a future operator running the matrix on a different PE BAM (e.g., a low-overlap exome panel) will get a hard exit 1 even though Rust + Perl actually agree perfectly on byte-identity.

This intersects with **B-Imp-1** (fail-open vs fail-closed) — the worst-case scenario is "byte-identity holds but the row-count differential check fires falsely due to BAM-shape, masking real byte-identity success".

**Fix:** Either:
- **(a)** Spec the matrix-verdict to differentiate: byte-identity status (PASS/FAIL) is the primary verdict; row-count differential is an *auxiliary* check whose FAIL state on the `overlap > D` direction triggers a specific message: "FAIL: row-count differential (overlap not >D) — verify input BAM has measurable overlap fraction; byte-identity itself is unaffected". Exit code still 1, but the operator can disambiguate.
- **(b)** Make the `overlap > D` direction conditional on a BAM-pre-flight check: "if `samtools view <BAM> | head -1M | awk` reports overlap pairs < some threshold, skip the `overlap > D` assertion with a warning".

Option (a) is the safer rev-1 fold (no extra pre-flight logic; just improved verdict messaging).

**Severity:** Important. The 10M PE BAM has the right shape for the current fixture, so this won't fire on the locked release-gate run — but the plan claims SPEC-grade reusability of the matrix driver, and the next BAM may differ.

---

## Optional / nits

### B-Opt-1. `Crate version bump: none.` claim is defensible but the SPEC §10 edit is a behaviour-contract change

**Location:** Header `Crate version bump: none.`, §5.1 SPEC §10 row H edit.

The plan ships `rust/bismark-extractor/SPEC.md` §8.3 PE matrix subsection + §10 row H PE update — both are SPEC edits documenting the byte-identity invariants now tested. Semantically, this is a documentation expansion, not a contract change (the contract was already implicit; the matrix codifies its assertion mechanism). Defensible as no version bump. But: under strict semver-for-contracts, adding a new tested assertion *is* a contract refinement. If Felix's project policy treats SPEC §10 rows as semver-significant, a patch bump would be defensible.

Recommendation: leave as-is (no version bump) and add a one-line note in the Revision History or §5.1: "SPEC edits are documentation of already-implicit guarantees, not new contract obligations. No crate version bump warranted."

**Severity:** Optional. Project policy call, not a bug.

---

### B-Opt-2. §3.5 edge-case table — three PE-specific cases worth adding

**Location:** §3.5 (lists 14 cases).

The table is good. Three additional cases worth folding (defensible to defer):

- **(a)** PE BAM with `@PG` line missing entirely. Smoke's auto-detect mechanism may default to SE behaviour or error. The plan §3.3.2 #5 PE-ness assertion would catch this, but the failure message is hardcoded to "expected PE BAM, smoke reports Library: SE" — it should differentiate "@PG missing" from "@PG says SE".
- **(b)** PE BAM whose `@PG` advertises `--paired` but the actual reads are SE (corrupt header). Rare but plausible from a misnamed BAM.
- **(c)** `--include_overlap` cell where Rust and Perl disagree on a single R2 base's call due to a Phase C.1 polarity off-by-one (positionally correct direction but wrong base). The byte-identity strict-cmp on M-bias would catch this, but the row-count differential would NOT (because row count is unchanged; only the per-position counts differ). This is OK — the M-bias cmp is the catcher — but worth documenting in §3.5 explicitly so the operator knows "row count == D in overlap cell does NOT mean polarity is correct; only M-bias strict-cmp catches per-base polarity".

**Severity:** Optional. The byte-identity check already catches (c); (a) + (b) are unlikely failure modes on the locked 10M PE fixture.

---

### B-Opt-3. §5.2 differences-from-SE list is a brief — implementer may need more

**Location:** §5.2 (7 numbered differences).

The 7 differences are correct as far as they go, but an implementer with no prior context might miss:

- The PE driver should NOT share `_phase_h_lib.sh` with SE (per Felix's 2026-05-28 directive — §2.4 mentions it but §5.2 doesn't restate, so an implementer who skim-reads §5.2 might "helpfully" refactor into a shared lib).
- The Rust binary discovery + on-demand build mechanism (§3.5 row "Rust binary not built") — the SE driver delegates this to `phase_h_smoke.sh`; the PE driver should too. §5.2 should explicitly say "delegate Rust binary discovery + build to `phase_h_smoke.sh` per SE driver pattern; do not re-implement".
- The pre-flight `--out` empty-check uses `find <OUT> -mindepth 1 -maxdepth 1 -print -quit` pattern in SE; §5.2 should mention "use the same empty-check pattern" to avoid an implementer using `[[ -z $(ls $OUT) ]]` which has subtle quoting bugs.

**Fix:** Expand §5.2 from 7 to ~10 numbered points, or add a "Implementer notes (mechanical copy-from-SE checklist)" sub-paragraph.

**Severity:** Optional. A diligent implementer reading the full plan would catch these; §5.2 alone is borderline-thin as an implementer brief.

---

### B-Opt-4. Test fixture stability assumptions are concentrated but never validated

**Location:** §A1 + §A8 + §A17, §11 R3.

The plan correctly identifies that the 11,443 B M-bias and 875 B splitting-report baselines, and the `overlap > D` row-count differential, all depend on the **specific** 10M PE BAM at `/weka/.../SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplicated.bam`. If colossal's data dir is re-staged from a fresh subset (e.g., different random seed) or if the file is replaced with a different 10M subset of SRR24827378, all three baselines drift simultaneously — and the matrix becomes a hard-fail until baselines are re-locked. The plan flags this in §11 R3 but doesn't propose a mitigation other than "documented assumption".

**Suggestion:** Add to §A1 / §11 R1 a one-line "lock-in mechanism": the matrix driver should record the input BAM's MD5 in `matrix_verdict.txt`. If a future run shows different MD5 than the previous reference, the verdict emits a "⚠️ BAM has changed since last reference run; baselines may need re-locking" advisory. Same evidence-friendly pattern as the existing `Generated: <timestamp>` lines.

**Severity:** Optional. Documenting MD5 alongside the baselines is cheap and protects against silent fixture-drift on long-running release-checklists.

---

### B-Opt-5. Realistic Phase C.2-era PE speedup expectation

**Location:** §3.3.6 exit code 3, §1 Out-of-scope, §11 R6.

CLAUDE.md records "Phase C.2 era measured 0.9× on default PE per CLAUDE.md". The plan correctly accepts exit 3 as non-blocking for v1.0 — byte-identity is the gate, not speedup. This is the right call. But the plan should be explicit that **v1.0 ships with the expectation that PE may not hit the 4× scaling target on first colossal run**, and that's intentional. §1's "Out-of-scope" mentions performance is non-blocking but a reader skim-scanning might miss that v1.0 *is* expected to land with exit 3. One sentence in §1 like "v1.0 may legitimately ship with PE matrix exiting 3 (perf-miss); the matrix's byte-identity verdict is the only release-blocking signal" would close the question.

**Severity:** Optional. Already documented across multiple places; consolidation would help.

---

## Logic / consistency spot-checks (passed)

- **B-L1 (R*100/P direction):** §3.3.5 "with `P=0` guard, per SE rev 3 B-L1" — verified.
- **A-L1 ≡ B-L2 (M-bias fail-closed):** §3.3.2 #10 — verified: `MBIAS_BASELINE_OK=0` init, "set to 1 only on positive `size==11443` match".
- **A-Er1/Er2 (bash 4.0 + `${ARR[@]+...}` defensive):** §3.3.2 #1 + Revision History — verified.
- **B-E3 (BSD/GNU `uptime` regex):** §3.3.2 #8 + §3.5 — verified.
- **SIGINT trap:** §3.3.2 #2 — verified.
- **`nproc` graceful skip:** §3.3.2 #8 — verified.
- **`tmux` warning:** §3.3.2 #9 — verified.
- **Cross-N byte-identity per CELL_ID:** §3.3.4 — verified, structurally matches SE rev 1 C1 absorption.
- **Perl-only scaling column:** §3.3.5 markdown — verified ("Perl scaling" + "Rust scaling" columns both present).
- **`--extra-rust`/`--extra-perl` array form:** §3.2 — implicit via smoke's post-#873 capability matrix in §2.4.
- **Perl version assertion:** §3.3.2 #6 — verified.
- **Matrix-level `--out` empty rejection:** §3.3.2 #4 — verified.
- **Exit-code mapping 0/1/2/3 with perf-miss exit 3:** §3.3.6 — verified.
- **Phase C.1 polarity guard mechanism:** §3.4 #6 — verified (implicit via M-bias byte-count at (D,N=1) + row-count differential `overlap > D`).
- **6 kept files for directional PE post-C.2:** §3.4 #2 + §2.5 — verified.

All SE rev-3 absorptions claimed in the Revision History are actually folded with concrete mechanisms in §3 / §4 / §5 (not just mentioned in §11). Strong consistency.

---

## Alternatives worth flagging (informational only — not action items)

1. **Single matrix driver for SE+PE with mode flag.** Felix already vetoed this on 2026-05-28 (per §2.4 directive). Re-stating just for completeness — the asymmetry of SE's `edge_clip` vs PE's `overlap`, and the cell-id naming divergence (B-Imp-3), is the cost of the independent-drivers decision. The decision is sound; the cost is mostly cosmetic.

2. **Add MD5 of input BAM to `matrix_verdict.txt`** (see B-Opt-4). Cheap fixture-drift detector.

3. **Move `row_count_diff.txt` content into `matrix_verdict.txt`** (see B-Imp-2). Drops a standalone artifact and matches SE driver's "evidence in two places: speedup_table.md + matrix_verdict.txt" pattern.

---

## Final verdict

**APPROVE-WITH-NITS.**

- **0 Criticals.** The plan correctly absorbs SE rev 3's findings, ships the right gates, and gets the high-stakes design decisions right (cross-N per cell, fail-closed M-bias gate, exit-code mapping with perf-miss exit 3, byte-identity baselines locked at (D, N=1)).
- **5 Importants** worth folding before merge (B-Imp-1 through B-Imp-5). B-Imp-1 (row-count fail-closed for the asymmetric `overlap > D` direction) is the strongest of the five; the others are about specification gaps, not correctness gaps.
- **5 Optional** items for rev-1 polish if Felix has the bandwidth; defensible to defer to a `polish(extractor):` follow-up.

Recommend: fold B-Imp-1 (one-line fail-closed init + missing-file guard) and B-Imp-2 (spec the file format or drop the standalone file) at minimum; the rest are negotiable.

— Reviewer B
