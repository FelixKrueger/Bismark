# Plan Review — Phase H sub-gate 1 SE (Reviewer B)

**Plan file:** `plans/05262026_bismark-extractor/PHASE_H_SE_PLAN.md` (rev 0)
**Reviewer:** B (independent of Reviewer A; no shared state)
**Date:** 2026-05-28
**Pre-resolved Criticals (per §10):** manual `RELEASE_CHECKLIST.md` for CI integration; independent drivers for #871 + #872. Not re-flagged below; the HOW of each is critiqued.

My angle (per the calling caller's brief): colossal infrastructure assumptions, release-checklist operational realities, matrix runtime under contention, edge cases under operational reality, strategic alternatives, #871-fidelity check, #872 coupling, and matrix-design premature-optimization risk. I deliberately avoid re-tracing Reviewer A's checklist.

---

## 1. Logic review

The plan is internally coherent. The 8-cell matrix, the per-cell smoke routing, the speedup-table emission, and the exit-code taxonomy all fit together; nothing in §3 contradicts §5 or §4. The non-code nature of this plan (bash + markdown only) keeps the logic surface small.

The main logic-level concerns I surface (sorted by severity):

- **L1.** §3.6 says the matrix PASSes iff every cell's smoke exits 0 + self-determinism passes + the default cell's M-bias.txt matches the 5712 B baseline. But §3.3.5's exit-code 0 says "all cells PASS + Rust scaling ≥ 4×". These two definitions of "matrix PASS" disagree: §3.6's verdict is byte-identity-only (the M-bias byte count is part of byte-identity); §3.3.5 folds perf-target into a separate exit code. The driver author needs to know which wins. Most likely §3.6 governs the `matrix_verdict.txt` PASS/FAIL string and §3.3.5 governs the exit code, but the plan should say so explicitly.

- **L2.** §3.4 point 4 self-determinism is described as "two consecutive Rust runs at the same `--parallel N` produce raw-byte-identical output." Phase F's invariant is **stronger**: N=1 and N=4 both produce byte-identical output to each other (see SPEC §8.3 row "`--multicore 4` byte-identity vs `--multicore 1` Rust output", line 730). Phase H's matrix has cells at both N=1 and N=4, so a cross-N byte-identity check between, e.g., cell 1 (p=1,i=0,i3=0) and cell 5 (p=4,i=0,i3=0) — same flags, different N — is a stronger Phase F regression guard than per-cell self-determinism. The plan does per-cell self-determinism (re-run same N) but does NOT cross-check N=1 vs N=4 byte-identity. This is a real gap.

- **L3.** §3.2 says "for SE mode + the matrix cells, both lists are identical (`--ignore N --ignore_3prime M`)." But Perl's `bismark_methylation_extractor` uses `--multicore` not `--parallel`. The smoke script already handles that translation in §4.2 / the existing code (lines 144-152 of the current script feed `--multicore "$PARALLEL"` to Perl). For `--ignore` + `--ignore_3prime` the flag names are identical between Perl and Rust per the Phase G argv tests + #872's body. Confirm before implementation; if Phase G actually renamed any flag, the matrix driver's claim of identical `--extra-perl` and `--extra-rust` payloads breaks.

- **L4.** §5.3 point 3 says "assert byte count == 5712 OR document the new baseline in a clearly-flagged warning." OR-with-warning sounds permissive but in §3.6 the M-bias baseline match is a hard PASS gate. Decide: hard FAIL on byte-count drift, or warning. Felix's §3.5 row says "investigate before declaring matrix PASS" — that reads as hard FAIL. §5.3's "or warning" loosens it. Pick one.

- **L5.** §6 estimates Perl `--multicore 1` ≈ 12 min on 10M SE. The number 12 min is the PE figure from CLAUDE.md profiling (10M PE — full pipeline 104 min on 55M PE, scaled). SE on 10M is generally faster than PE on 10M because there's one read per pair, not two. The runtime estimate is probably 1.5-2× too high. This isn't a correctness problem — just an overestimate that the user should be aware of, plus it affects the operational-cost framing (see §2 below).

- **L6.** §5.4 step 4d says wall-clock is "parsed from the smoke's `diff_summary.txt`". The current smoke writes `Perl: ${PERL_ELAPSED}s` + `Rust: ${RUST_ELAPSED}s` (lines 167-168). The integer-seconds resolution is acceptable for ~minute-long runs but the speedup ratio (e.g. "0.88×") implies 2-decimal precision. The smoke's existing `SPEEDUP10=$(( PERL_ELAPSED * 10 / RUST_ELAPSED ))` is 1-decimal integer arithmetic. The driver's per-N aggregation will lose precision when integer-dividing already-integer seconds. Decide: emit integer seconds at higher resolution (e.g. `date +%s.%N` for sub-second), or be honest about ~1% precision in the speedup table.

---

## 2. Assumptions — colossal-infrastructure (my primary angle)

The plan §2.5 + §8.1 A1/A2/A3 stack three UNVERIFIED assumptions:

- **A1 (BAM path).** Plan assumes `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/10M_SE/directional_10M_R1_val_1_bismark_bt2.bam`. Per `reference_colossal_access.md` line 24: "Do NOT assume the oxy paths port verbatim — `ls` ... to discover the actual layout before running."
- **A2 (micromamba env).** Plan assumes `bioinf` env activates correctly. Per memory line 32-40: activation pattern + `MAMBA_ROOT_PREFIX` + the micromamba binary path are TBD on colossal.
- **A3 (cores).** Plan assumes ≥ 4 cores. Per `reference_colossal_access.md`: nothing said. Colossal is a shared box; core count + immediate availability are not pinned.

**The failure mode if any of these is wrong on first colossal use:**

- A1 wrong → `[[ ! -f "$BAM" ]]` check in the smoke (line 76) triggers; exit 2 with "BAM not found". This is LOUD and recoverable — release-engineer fixes the path and re-runs.
- A2 wrong → micromamba activation silently leaves the wrong Perl on PATH (system perl or oxy-era `bismark-test` env if it's still around) or `bismark_methylation_extractor` is not found. The smoke script's `if [[ ! -x "$PERL_BIN" ]]` check helps (line 92), but only if `PERL_BIN` is unset and the default `$REPO_ROOT/bismark_methylation_extractor` is missing. If the user accidentally has the wrong-version `bismark_methylation_extractor` (e.g. v0.24 instead of v0.25.1), the binary IS executable, the script proceeds, and produces a different-from-locked M-bias.txt — failing only via the byte-count regression guard. This is a SLOW + CONFUSING failure path.
- A3 wrong (e.g. colossal has 4 cores but they're all busy) → `--parallel 4` still runs but the rayon threads contend with other users' threads; wall-clock inflates non-deterministically; speedup measurement is meaningless. NO loud failure; the speedup just looks bad and the user blames the Rust port.

**Recommendation:** Make A2 + A3 LOUD on first colossal use:

- §5.4 pre-flight should `which bismark_methylation_extractor` + extract its version (`bismark_methylation_extractor --version`) + assert it matches v0.25.1 (the version the SPEC + plan target). If absent or version mismatch, exit 2 with a clear message.
- §5.4 pre-flight should `nproc` (or `getconf _NPROCESSORS_ONLN`) + warn if the available cores < the requested `--parallel` × 2 (room for Perl + Rust contention). Not a hard fail, but visible in stderr.
- §3.5 should add a "Perl version drifts from v0.25.1" row to the edge-case table.

---

## 3. Release-checklist operational realities

§5.5 + §4.3 specify `RELEASE_CHECKLIST.md` as the v1.0 gate. The plan's checklist (§4.3) is structurally OK as a starting template but operationally thin for a one-person release-engineer process.

**Operational gaps:**

- **G1. Who runs the matrix, when?** The checklist doesn't say. Felix is the implied release manager (per A12) but a future maintainer takeover or a CI handoff isn't planned for. The checklist should name "release manager runs this; if delegated, the delegate logs PASS in #871 + #872 as a comment with the speedup_table.md attached."
- **G2. How does the release manager communicate completion?** The plan doesn't say. Suggestion: gate the v1.0 tag commit on the speedup_table.md being committed to the repo (e.g. `docs/release-evidence/v1.0/se_speedup_table.md` + the PE counterpart). This makes the evidence durable + auditable. Otherwise the PASS is ephemeral (lives in `<OUT>/speedup_table.md` on the release engineer's `/tmp/`).
- **G3. Mid-checklist regression discovery — what happens?** The checklist says "If exit 1: investigate the failing cell. Do NOT tag v1.0 until resolved." Investigate by whom + how? File a `bug(extractor):` sub-issue with the failing-cell evidence attached. The checklist should explicitly say so + give the template.
- **G4. Partial-checklist re-run.** §3.5 has "Cell output dir already exists" → exit 2. This is good, but the checklist doesn't say "if you fix the issue and want to re-run, use `--out <NEW_DIR>` or `rm -rf <OLD_OUT>` first." Trivial but worth documenting; the release engineer might be a different person from the implementer + lacks bash habits.
- **G5. Implicit "multiple stakeholders" reading.** The PE counterpart in §4.3 says "(Same shape; populated by #872's plan)." This implies #872's plan adds checklist content. But what happens if #872 lands BEFORE #871? Does the PE section get added to RELEASE_CHECKLIST.md without the SE section? The plan should specify a merge-order independence by including a PE TODO stub in this PR's checklist (which §5.5 step 2b does, good) but should also explicitly say "if #872 merges first, #872's PR adds the PE section; this PR adds the SE section; merge conflicts are trivial." This is the dependency-direction issue I'm flagging more pointedly in §7 below.
- **G6. RELEASE_CHECKLIST.md ownership.** Top-level repo file is fine, but Bismark's existing top-level is Perl-era. A new top-level Rust-era file may surprise a Perl-only contributor. The plan should add a one-line section header clarifying scope: "**Scope: bismark-extractor + bismark-bedgraph (Rust). Perl Bismark v0.25.1 release process is unchanged.**"

---

## 4. Matrix runtime under colossal load (efficiency)

§6 estimates ~2.7 hours total for the SE matrix. The estimate assumes a quiescent colossal. Under contention:

- **Weka is a shared filesystem.** Other users may be reading/writing concurrently; per-file I/O latency variance is real on Weka (depending on object placement + replica health). 10M SE input BAM is ~650 MB and the output is ~1.2 GB; both are non-trivial I/O.
- **CPU contention.** colossal is multi-user; other workloads may pin cores. Even if `--parallel 4` schedules 4 rayon threads, scheduling latency under contention makes per-cell wall-clock noisier.
- **Speedup ratios are MORE sensitive to contention than wall-clock.** A 5-min cell vs a 10-min cell give a 2× ratio; one slow cell from contention can swing the ratio out of the SPEC §9.7 4× target band. Hence exit code 3 (perf-target miss) could fire purely from contention, not from a real Rust-side regression.

**Recommendations (sorted by cost):**

- **R1 (cheap).** Add a §3.5 row "colossal under contention" → recommend running the matrix at low-traffic hours (overnight / weekend) OR using `nice -n 19` on both Perl and Rust to deprioritize against interactive users. Not a hard requirement; an advisory.
- **R2 (medium).** Run each cell TWICE (separate from the self-determinism check — that just re-runs Rust at the same N for byte-identity); average the wall-clock. Doubles runtime to ~5.5 hours but materially improves the speedup-ratio reliability. Could be a `--repeat N` flag on the driver, defaulting to 1.
- **R3 (advisory).** §9.2 should add a row: "If exit code 3 fires, re-run during quieter hours before filing `perf(extractor):` — exit 3 may be contention noise, not a real Rust regression."

Without one of these, the plan's exit code 3 may produce false `perf(extractor):` sub-issues that waste investigation time. R3 is the minimum; R2 is the right answer if Felix values clean perf data.

---

## 5. Edge cases under operational reality (additions to §3.5)

The plan's §3.5 has 10 cases; my candidates for ADDITIONS:

- **E1. Network disconnect from `dcli ssh colossal` mid-matrix.** The matrix takes ~2.7 hours; a laptop sleeping or a VPN drop mid-cell will SIGHUP the bash process. Recommendation: run inside `tmux` or `screen`; document this in §4.3 and §5.5.
- **E2. Weka NFS-like latency variance making per-cell wall-clock unreliable.** Already partially addressed by R3 above; add an explicit edge-case row.
- **E3. User re-runs a partial matrix after fixing an environment issue.** §3.5 already has "Cell output dir already exists → USAGE-ERROR." Verify this rejects a non-empty `<OUT>` correctly AT THE MATRIX LEVEL (not per-cell). If `<OUT>/cell_p1_i0_i30/` exists but `<OUT>/cell_p4_i5_i35/` doesn't (partial previous run), does the driver bail at the first cell or at the matrix pre-flight? Pre-flight at the matrix level should reject — cell-level rejection makes for a confusing error.
- **E4. Perl binary version drift.** Already covered in my §2 above (A2). The plan should add a row: "Perl `bismark_methylation_extractor --version` ≠ v0.25.1 → USAGE-ERROR; M-bias baseline assumes v0.25.1 reference Perl." This is critical because the plan's locked 5712 B M-bias is specific to v0.25.1.
- **E5. samtools missing on colossal.** The smoke uses `samtools view -H` for PE auto-detect (line 123 of `oxy_phase_h_smoke.sh`). If samtools is absent, `command -v samtools` returns non-zero → silently treats input as SE (PE_FLAG stays empty). For an SE matrix this happens to be correct, but if Phase H is ever used on PE input (out of scope here but #872 will), a missing samtools would silently mis-classify. Worth noting now.
- **E6. SE BAM is actually PE.** If the SE BAM was mislabeled (or the user passes a PE BAM by accident), `@PG` auto-detect picks PE; the matrix runs as PE; the SE-specific 6-kept-files assertion FAILs (PE has the same 6 directional after C.2's empty-sweep, so this might silently pass — see §6 below). Worth a sanity-check before the matrix loop.

---

## 6. Validation sufficiency

The plan's validation is OK for byte-identity but has gaps for the speedup-measurement contract:

- **V1.** §9.2's "Speedup at N=4: Rust scaling ≥ 4×" — only checked against §9.7's 4× target. But §9.7 explicitly references the 5.4 min/12.3 min PE figure → 2.3× expectation FOR PERL. The 4× target is for Rust. The plan correctly distinguishes these but a release manager reading only the table risks confusing the two. The speedup_table.md template in §3.3.4 includes a "Per-N aggregate" section with `Avg Rust/Perl` — this should be relabeled `Rust(N) / Perl(N)` to make the cross-binary axis distinct from the within-binary scaling axis.
- **V2.** Self-determinism is per-cell but not per-N (see L2 above).
- **V3.** No validation that Perl and Rust both processed the SAME records. If one binary silently dropped records (e.g. due to a malformed header on a re-run), the sorted-MD5 might still match for some files but the line counts would differ. The smoke's existing per-file diff doesn't compare line counts; consider a per-file `wc -l` cross-check.
- **V4.** The SE-specific 6-kept-files assertion (per §3.4 point 3) is hard-coded for **directional SE**. Non-directional SE would have 12 files, mirroring CTOT/CTOB. The plan doesn't handle non-directional. Per the open question in §10 ("`phase_h_smoke.sh`'s SE-specific 6-file assertion be configurable") — the plan's answer is "directional SE is the only realistic case at v1.0." But the 10M SE BAM's library type isn't documented in the plan; if it happens to be non-directional (unlikely but possible for some test fixtures), the assertion will FAIL. Recommend a quick `samtools view -H "$BAM" | grep '@PG.*--non_directional'` sanity check in pre-flight.
- **V5.** The speedup table doesn't capture Perl-baseline scaling (`Perl-N4 / Perl-N1`). #871's body explicitly mentions this: "Perl multicore speedup: Perl-N4 / Perl-N1 (expected ~2.3× per CLAUDE.md profiling)". The plan's table template (§3.3.4) has an "Avg Perl (s)" column but doesn't compute Perl scaling. Add it; it's free (already have the data).

---

## 7. Alternatives at the strategic level

The brief asked me to consider simpler alternatives. I argue:

- **Alt-1. Skip speedup measurement entirely from Phase H.** The plan §1 says "Phase H gates on byte-identity, NOT speedup." If perf is informational, why bake it into the same matrix? Pros of splitting: (a) byte-identity matrix becomes single-pass (no Perl runs needed at higher N), trimming ~50% of runtime; (b) perf measurement gets a properly-designed harness with repeats/averages/contention-controls (per §4 R2 above); (c) RELEASE_CHECKLIST.md becomes cleaner — one tickbox per gate. Cons: more scripts, more files, splits work that's naturally one matrix into two. **My verdict:** keep them together for v1.0 (Felix has made this call implicitly with the matrix design) but flag this as a follow-up cleanup post-v1.0. Add a §11 magnet.

- **Alt-2. Single-cell minimum-viable harness.** The brief raised this. Argument: SE byte-identity is already PASSING (per the existing Phase F + C.2 smoke runs); the ignore-flag paths are exercised by unit tests in `tests/phase_g_realrunner.rs`-style. So matrix expansion may be over-testing the wrong layer. **Counter:** unit tests use synthetic ≤100-record fixtures; real-data 10M SE catches things synthetic tests can't (e.g. BAM-header oddities, large-scale order effects, real CIGAR strings with long soft-clips). The 8-cell matrix is moderate, not excessive. **My verdict:** keep the matrix but consider whether 4 cells suffice (see Alt-3).

- **Alt-3. Reduced matrix (4 cells instead of 8).** The 8 cells are the cartesian product of {N=1, N=4} × {ignore=0,5} × {ignore_3prime=0,5}. The 4 cells of {(ignore=0, ignore_3=0), (ignore=5, ignore_3=0), (ignore=0, ignore_3=5), (ignore=5, ignore_3=5)} × {N=1 only} would test byte-identity equivalent to 8 cells if Phase F's N-invariance holds (which it does, per cross-N self-determinism above). Then add a single Rust N=4 vs Rust N=1 comparison cell purely for speedup. Total: 5 cells instead of 8 ≈ 1.7 hours instead of 2.7. **My verdict:** worth raising. The brief asked specifically about this; #872 took a similar reduction (5 cells of 64 possible). #871 could match: justify the 8-cell choice OR reduce to 5. The plan's §11 magnet 5 mentioned a 9th cell ("`--ignore 250`") but didn't consider going below 8. Inconsistent depth.

- **Alt-4. Use Bismark's official test data.** Bismark ships small test fixtures (search the repo for `test_dataset` or `test_data`). If those exist + are small, they're more portable than the colossal-only 10M SE. **Quick check needed:** does Bismark ship a SE WGBS test BAM? If yes, the plan could run a 3-cell pre-merge subset on the test data + the full 8-cell matrix on colossal. Worth a 5-line investigation. **My verdict:** include this in §9.1 pre-merge if test data exists; otherwise the 10M SE on local Mac (Felix's Desktop) is the fallback, which §5.8 already plans.

- **Alt-5. Bake the matrix into `cargo test`.** Rejected by Felix per §7.3 (CI integration = manual checklist). Move on.

---

## 8. #871-fidelity check

I cross-checked the plan against #871's body:

- ✅ SE-only scope — matches plan.
- ✅ 8-cell matrix (parallel × ignore × ignore_3prime) — matches.
- ✅ Byte-identity contract per SPEC §8.3 rev 3 — matches §3.4.
- ✅ `--ignore 5 --ignore_3prime 5` boundary check called out — plan honors via the (5,5) cells.
- ⚠️ **Perl multicore speedup** (Perl-N4 / Perl-N1) — #871 explicitly says to compute this. The plan's §3.3.4 template doesn't include Perl-only scaling, only Rust-only scaling + Rust/Perl ratios. **Gap.** (See §6 V5 above.)
- ⚠️ **Rust-vs-Perl at N=4 as a "confirmation whether SE mode also runs slower than expected or whether the bottleneck is PE-specific"** — #871 body explicitly calls this out. The plan computes per-cell Rust/Perl ratio (good) but doesn't explicitly mark that ratio as a flagged data point for the cross-port-mode comparison. **Minor gap.** Adding a one-line "comparison-with-PE-era figure" annotation in the speedup table satisfies it.
- ✅ Self-determinism — matches (with the per-cell vs per-N nuance noted in L2).
- ✅ `*.M-bias.txt` 5712 B locked baseline — matches.
- ✅ Sub-gate 2 out-of-scope — matches.
- ✅ Speedup-target-miss does NOT block Phase H — matches.

**Net:** Plan is 95% faithful to #871. The two ⚠️ marks above are non-Critical but worth catching in rev 1.

---

## 9. #871/#872 coupling

The plan asserts independence (§2.6) per Felix's directive. But:

- **Coupling-1.** RELEASE_CHECKLIST.md couples them: "v1.0 tag requires both PASS." This is correct + desired.
- **Coupling-2.** PROGRESS.md couples them (row 21 still shows H sub-gate 1 PE as `⏸ sub-issue filed; plan TBD`).
- **Decoupling concern.** What happens if #871 is ready to land + merge but #872 hasn't been planned/implemented? **Plan answer:** §2.6 + §5.5 step 2b explicitly handle this — the PE checklist section is a TODO stub. So #871 merges, RELEASE_CHECKLIST.md has SE filled + PE TODO. Later #872 merges + fills in PE. v1.0 tag waits for both. **This is correct + well-designed.** No blocking issue.
- **Decoupling concern (reverse).** What if #872 lands BEFORE #871? §5.5 doesn't handle this. The plan implicitly assumes SE-first order. **Minor risk** — should add one sentence: "If #872 merges first, this PR rebases on top of it + adds the SE section without conflict."

---

## 10. Matrix design — premature optimization risk

The brief asked about cartesian-product redundancy. Let me think about it:

- The 4 cells `{(p=1, i=0, i3=0), (p=1, i=5, i3=0), (p=1, i=0, i3=5), (p=1, i=5, i3=5)}` are NOT redundant under each other; they exercise distinct code paths in `extract_calls` (`--ignore N` filters 5' positions, `--ignore_3prime M` filters 3' positions; the joint cell exercises both filters in the same read). The hypothesis "if `--ignore 5` succeeds + `--ignore_3prime 5` succeeds, the joint succeeds" is plausible but NOT guaranteed because the joint case can hit a corner where both filters compound to an empty middle region.
- However, the N=4 row `{(p=4, ...)}` IS approximately redundant given Phase F's N-invariance. If Rust's output bytes are identical for N=1 and N=4 on the same input + flags (per §8.3 row 8 in SPEC), then the cell at (p=4, i=5, i3=5) PASSes the byte-identity gate iff (p=1, i=5, i3=5) PASSes + the N-invariance holds. **The matrix is over-testing IF you trust the existing N-invariance unit tests.**
- Counter: real-data 10M-record N-invariance has only been smoke-tested at default flags (pre-Phase-H smoke). The matrix's N=4 row IS the right place to catch a regression where N-invariance breaks for non-default ignore-flag values (e.g. if a thread-local counter for `--ignore` gets out of sync per-worker).
- **Verdict:** the 8-cell design is defensible but the plan should articulate WHY (the above N-invariance × ignore-flag cross-validation argument). Currently the plan just states the matrix without justifying the cartesian choice vs a reduced design. This is the same critique as Alt-3 above, more pointed.

---

## 11. Action items

### Critical
None. The two Felix-resolved Criticals stand. No new Criticals from my pass.

### Important

1. **(L1)** Clarify §3.6 (matrix PASS verdict) vs §3.3.5 (exit code) interaction. Make explicit: byte-identity governs PASS string; perf miss only escalates the exit code.
2. **(L2 / V2)** Add cross-N byte-identity check (e.g. N=1 cell vs N=4 cell at same flags) as a stronger Phase F regression guard than per-cell self-determinism. Currently a real gap.
3. **(§2 A2 / E4 / V4)** Pre-flight should assert `bismark_methylation_extractor --version == 0.25.1` (the locked-baseline reference) + record the version in speedup_table.md. Without this, the M-bias baseline 5712 B is meaningless if the Perl binary drifts.
4. **(§2 A3 / §4)** Pre-flight should `nproc` + warn if available cores < requested parallel × 2 (Perl + Rust contention). Plus a §3.5 row for "colossal under contention; consider `nice -n 19` and/or low-traffic hours."
5. **(§6 V5 + #871 fidelity)** Add Perl-only scaling (`Perl-N4 / Perl-N1`) to speedup_table.md. #871 body explicitly requests it.
6. **(§3.5 / E1)** Add edge-case row + checklist guidance: run the matrix inside `tmux` or `screen` (mitigates SSH disconnect mid-2.7-hour-matrix). Trivial but easy to forget.
7. **(§3.5 / E3)** Verify pre-flight rejection of non-empty `<OUT>` is at the **matrix level**, not per-cell. Cell-level rejection mid-run produces a confusing partial-state.
8. **(§4.3)** Flesh out the release-checklist's operational specifics: who runs it, how PASS is communicated (commit the speedup_table.md as evidence under `docs/release-evidence/v1.0/`), what to do mid-checklist on regression. The current skeleton implies multiple stakeholders without naming them.
9. **(§2.6 / Coupling-3)** Add one sentence to §2.6: "If #872 merges first, this PR's RELEASE_CHECKLIST.md addition is a no-op-or-rebase on the SE section; no conflict expected."
10. **(L5)** §6's Perl `--multicore 1` ≈ 12 min estimate is the PE figure; SE will be faster. Update the runtime estimate to be SE-specific (or mark "≤ 12 min").
11. **(L4 / §5.3 point 3)** Reconcile "byte count == 5712 OR document with warning" vs §3.6's hard-PASS-on-match. Pick hard FAIL on byte-count drift; "investigate before declaring matrix PASS" is hard FAIL.
12. **(§10 / matrix justification)** Add a paragraph to §3.1 or §11 justifying the 8-cell cartesian over a 5-cell reduced matrix. #872 reduced explicitly (5 of 64); #871 should match the rigor and either reduce or articulate why not.

### Optional

- **(L3)** Add a pre-flight argv-parity check between Perl + Rust for the flag-names this plan uses (`--ignore`, `--ignore_3prime`). Phase G's tests should already cover this but a runtime check is cheap insurance.
- **(L6)** Use `date +%s.%N` (or `time -p`) for sub-second wall-clock precision; current integer-seconds gives 1% noise floor that doesn't match the 2-decimal display in speedup_table.md.
- **(V3)** Add a per-file `wc -l` cross-check (Perl vs Rust line counts) on top of sorted-MD5 — catches the silent-record-drop case.
- **(V4 / §3.5)** Add `samtools view -H "$BAM" | grep '@PG.*--non_directional'` sanity check in pre-flight; abort with USAGE-ERROR if the SE BAM is non-directional (the 6-file assertion will FAIL otherwise + the error message will be unhelpful).
- **(Alt-1 §11 magnet)** Add §11 magnet: "Should perf measurement live in a separate sub-issue post-v1.0?" Felix may want to revisit after seeing the v1.0 matrix output.
- **(Alt-3 §11 magnet)** Add §11 magnet: "Could the matrix be reduced to 5 cells (matching #872's reduction)?" Currently the plan goes 8 cells without justifying vs reduction.
- **(Alt-4)** Investigate whether Bismark ships official SE test data; if yes, use it for pre-merge smoke alongside the local Desktop 10M SE.
- **(§7.3)** Add to "Deliberately NOT implemented": "Repeat-run-averaging for speedup measurement — single-run wall-clock is noisy; could add `--repeat N` flag in a follow-up if Felix wants cleaner perf numbers."
- **(§4.3 / G6)** Add a scope header at the top of `RELEASE_CHECKLIST.md`: "Scope: bismark-extractor + bismark-bedgraph (Rust workspace). Perl Bismark v0.25.1 release process is unchanged."
- **(§5.4 step 4e)** Self-determinism check is `cmp` against the **same N**'s previous run, not against a different-N run. Per L2, the cross-N check is stronger; consider replacing per-cell self-determinism with cross-N self-determinism at the matrix level (one check, not 8).

---

## 12. Overall verdict

**APPROVE-WITH-NITS.**

The plan is well-structured, faithful to #871, internally consistent, and respects the two Felix-resolved Criticals. The bash-+-markdown nature keeps the risk surface small; no Rust code changes means no regression risk to the Phase G 303-test baseline.

The Important items above (12 of them) are nits in the sense that they don't block implementation — they harden the plan against operational realities (colossal contention, Perl-version drift, SSH disconnect, etc.) and close two real gaps (Perl scaling per #871 body; cross-N byte-identity vs per-cell self-determinism). I'd ask Felix to fold in items 1, 2, 3, 5, 8, 11 before implementation; the rest can land in rev 1 as bash defensive-coding decisions during the code-implementation phase.

No Criticals; no NEEDS-REVISIONS. Proceed once Felix has chosen which Importants to fold into rev 1.

---

**Reviewer B's verdict summary (≤200 words):**
APPROVE-WITH-NITS. Plan is structurally sound; the 2.7h, 8-cell, bash-only design respects Felix's manual-checklist + independent-driver directives. Twelve Important items: (1) clarify the matrix PASS verdict vs exit-code interaction (§3.6 vs §3.3.5); (2) add cross-N byte-identity check — stronger Phase F regression guard than per-cell self-determinism; (3) pre-flight Perl version assertion (binary drift silently invalidates the 5712 B M-bias baseline); (4) pre-flight `nproc` + contention advisory; (5) add Perl-only scaling to speedup_table — #871 body asks for it; (6) document `tmux` for 2.7h-matrix SSH-disconnect; (7) verify pre-flight rejection of partial-output dirs is matrix-level; (8) flesh out RELEASE_CHECKLIST operational details (who, how PASS is recorded, mid-regression escalation); (9) handle reverse-merge-order with #872; (10) SE Perl runtime estimate (§6) overstated — Perl 12 min is the PE figure; (11) reconcile hard-FAIL vs warning on M-bias byte drift; (12) justify the 8-cell cartesian vs #872's 5-cell reduction or match the reduction. No Criticals. Ready for rev 1 after Felix selects which Importants to fold.

**Report path:** `/Users/fkrueger/Github/Bismark/plans/05262026_bismark-extractor/PLAN_REVIEW_PHASE_H_SE_B.md`
