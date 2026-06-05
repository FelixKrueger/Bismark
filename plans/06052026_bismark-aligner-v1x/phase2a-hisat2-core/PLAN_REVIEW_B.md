# PLAN_REVIEW_B — Phase 2: HISAT2 wrapper + byte-identity gate

**Reviewer:** B (independent, fresh context). **Target:** `phase2-hisat2-wrapper/PLAN.md` (rev 0).
**Oracle checked:** Perl `bismark` v0.25.1 (repo root) + crate seams under `rust/bismark-aligner/src/`.
**Verdict:** **Sound architecture, but TWO genuine byte-identity hazards the plan understates as "verify, not implement."** The PE-HISAT2 `ZS` read‑1 asymmetry (below) is a real divergence the current uniform parser will produce and the SE‑only spike could never have caught. Plus a wrong option‑order claim and unhandled splice flags.

---

## Logic — gaps, flawed assumptions, edge cases

### L1 (CRITICAL) — PE read‑1 `ZS` asymmetry: the uniform parser DIVERGES from Perl on PE‑HISAT2
The plan (Behavior #5, OQ §10, Self-Review "Risks") treats the `ZS` second‑best parse as **"already present (verify), not new logic."** That is true for SE and PE‑read‑2, but **false for PE read‑1**, and the divergence is invisible to the SE spike.

Perl's three parse loops are NOT symmetric:
- **SE** (`bismark` 2772–2796): `elsif ZS:i:` at **2780 captures ZS for any aligner**; the `else{if($bowtie2){XS||ZS}}` block is bowtie2‑only. → HISAT2 SE second_best comes from ZS. ✅ matches Rust.
- **PE read‑2** (3384–3403): `else{ if($bowtie2){XS} else{ZS} }` → HISAT2 read‑2 captures ZS. ✅ matches Rust.
- **PE read‑1** (3372–3382): `if AS / elsif XS:i: / elsif MD:Z:` — **there is NO `ZS` branch at all.** Under HISAT2, read‑1 emits `ZS:i:` (not `XS`), so Perl `$second_best_1` is **ALWAYS undef**, then backfilled to `$alignment_score_1` (3467–3469).

The Rust parser (`align.rs` 96–104) captures **both** `XS:i:` and `ZS:i:` uniformly for **every** record (verified: test `zs_tag_feeds_second_best` 500–503). `merge.rs` `check_results_paired_end` then reads `r1.second_best` (598) and combines `sum_second = s1 + s2` (605), feeding `calc_mapq` (3876 Perl / `Some(sum_second)` 620 Rust).

Concrete divergence for a PE‑HISAT2 read whose read‑1 carries `ZS:i:` (a multi‑mapping mate‑1):
- **Perl:** `sb1 = undef → as1`; `sum_second = as1 + sb2`.
- **Rust (current):** `sb1 = ZS_1`; `sum_second = ZS_1 + sb2` (with `ZS_1 ≤ as1`).
→ **different `sum_second` → different MAPQ → non‑byte‑identical BAM.**

This is real, not theoretical. The fix is to make the Rust PE path **ignore `ZS` on read‑1** (replicate the Perl quirk) — i.e. the read‑1 second‑best must be parsed XS‑only, NOT XS‑or‑ZS, while read‑2 and SE keep XS‑or‑ZS. The uniform `align.rs` parser cannot express that with a single code path; Phase 2 needs either a per‑mate parse flag or a post‑parse `r1.second_best = None` for HISAT2 PE before the merge. **The plan must add this as an implementation task and a dedicated PE‑HISAT2 multi‑mapper unit test; the V8 PE gate alone is not a reliable catch** (it only triggers if the real test data happens to contain a mate‑1 with a ZS tag — likely at 1M, not guaranteed at 10k).

### L2 (Important) — Behavior #3 / OQ‑2a state the WRONG option order
Behavior #3 says `--no-softclip --omit-sec-seq` goes "after `--ignore-quals`, before the PE tail," and OQ‑2a assumes "same relative position as SE." **Both are wrong.** Perl appends these flags in a dedicated late block `### ADDITIONAL ALIGNMENT OPTIONS WE NEED FOR HISAT2` at **8286–8317**, which runs **after** `--ignore-quals` (8012), the PE flags `--no-mixed`/`--no-discordant` (8044–8045), `--minins`/`--maxins` (8125/8131/8135), AND `--quiet` (8141). So the two flags are appended **dead last** to `@aligner_options`.

For default SE the spike string still lands `... --ignore-quals --no-softclip --omit-sec-seq` (no PE/maxins/quiet present), which is why the spike "confirmed" the SE form — but that masks the true position. The **PE** default HISAT2 string is:
```
-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --maxins 500 --no-softclip --omit-sec-seq
```
(NB: HISAT2 gets **no `--dovetail`** — Perl gates `--dovetail` behind `if($bowtie2)` at 8051–8059; the Rust `options.rs` 143–152 pushes `--dovetail` unconditionally on PE, so the HISAT2 PE branch must SUPPRESS it.) And if `--quiet` is set, it precedes the two flags. OQ‑2a's "Perl‑verified during impl" is the right instinct but the plan's stated assumption is the wrong answer — please correct the text so the implementer doesn't code the assumption.

### L3 (CRITICAL) — `--dovetail` must be suppressed for HISAT2 PE (corollary of L2)
Explicit because it is its own code change in a different file. `options.rs` `build_aligner_options` pushes `--dovetail` for every paired run (143–152). Perl only does so for Bowtie 2 (8051–8059; "HISAT2 doesn't have the concept of --dovetail"). The plan's `build_aligner_options(..., kind, ...)` signature change must gate the `--dovetail` push on `kind == Bowtie2`, or every PE‑HISAT2 alignment gets a spurious `--dovetail` → wrong `aligner_options` (visible in the report line too) and HISAT2 will likely reject the unknown flag → run fails. **Not mentioned anywhere in the plan.**

### L4 (Important) — `--no-spliced-alignment` / `--known-splicesite-infile` are unhandled
Both flags are already parsed (`cli.rs` 211–215, `known_splices`/`nosplice`) but handled **nowhere** (grep of `config.rs`/`options.rs`/`lib.rs`/`align.rs` = zero hits). Perl behavior (8287–8324):
- HISAT2 + `--no-spliced-alignment` → append `--no-spliced-alignment` (8295).
- HISAT2 + `--known-splicesite-infile <f>` → file‑exists check then append `--known-splicesite-infile <f>` (8298–8306).
- HISAT2 + both `nosplice` && `known_splices` → **die** (8290–8292).
- **non‑HISAT2** + either → **die** ("can only be selected in HISAT2 mode", 8319–8324).

The plan must decide and state: wire these into the HISAT2 option assembly (they change `aligner_options` → the report line → byte‑identity whenever a user passes them), AND add the Bowtie 2 hard‑reject (currently a silent no‑op vs Perl die — a pre‑existing Bowtie 2‑mode gap this phase is the natural place to close). At minimum, if deferring spliced‑junction wiring, **hard‑reject `--known-splicesite-infile` / `--no-spliced-alignment` fail‑loud** so the gate can't silently pass on a no‑op. The plan's "spliced‑N extraction parity" item (V6) addresses a *different* concern (the call walk) and does NOT cover these driver flags.

### L5 (Important) — `--ambig_bam` for HISAT2 is unconfirmed; naming token must NOT be applied to all aux files
Behavior #6 lists `<base>_bismark_hisat2{,_pe}.ambig.bam` and "`--unmapped`/`--ambiguous` aux (names per Perl)" under the per‑aligner token. Two problems:
- The Perl ambig‑BAM **temp** filenames are hardcoded `_bismark_bt2.ambig.bam` with the comment **"# only for Bowtie 2"** (656/661/715/720), while the per‑instance `$outfile` route (1575/1586 `s/sam$/ambig.bam/`) would yield `_bismark_hisat2.ambig.bam`. This internal Perl inconsistency means **`--ambig_bam` + HISAT2 may not be a supported/exercised path** — the plan must trace whether it's gated to Bowtie 2 (and if so, hard‑reject `--ambig_bam` in HISAT2 mode) rather than inventing a `_bismark_hisat2.ambig.bam` name that Perl may never emit.
- The **`_unmapped_reads*` / `_ambiguous_reads*` aux names carry NO aligner token** (687–785). The plan's `aligner_token(kind)` helper (Behavior #6, signature §4) must be threaded ONLY into the main BAM + report + (if supported) ambig.bam names — applying it to unmapped/ambiguous names would corrupt them. The plan should call this out explicitly so the refactor doesn't over‑tokenize.

### L6 (Minor) — spliced‑`N` genomic‑seq extraction: V6 is genuinely "verify, not implement" ✅
Confirmed against the oracle: Rust `methylation.rs` `N` op = `pos += len; // no indels` (190 and 362) is a faithful port of Perl 4372–4377 (`$pos += $len[$_]`, no `$indels`, no genomic seq appended). The `genomic_seq_for_MD_tag` correctly skips `N`. So the *extraction coordinate math across the skip* is correct and V6's "verify, not implement" framing is right for THIS sub‑concern. (The residual unknown is not the walk but whether HISAT2's emitted MD/CIGAR for a spliced read round‑trips through `hemming_dist`/MD‑regen byte‑identically — worth an explicit assertion on the 12‑record oxy path, which V6 does cover.)

---

## Assumptions

- **A‑good:** `calc_mapq` validity for HISAT2 — **holds.** Perl has no HISAT2‑specific MAPQ branch; `calc_mapq` (3923–4186) is driven by `score_min_intercept/slope` which for HISAT2 are the same `L,0,-0.2`. The Rust `mapq.rs` is a verbatim port and is aligner‑agnostic. The only HISAT2 MAPQ risk is the *input* `second_best` (see L1), not the formula.
- **A‑good:** version‑line parse — `parse_bowtie2_version` finds the first line containing `"version"` and splits on it; HISAT2's `hisat2-align-s version 2.2.2` satisfies this and the Perl regex `hisat2.*\s+version\s+(\d+\.\d+\.\d+)` (7086) agrees. The generalization is low‑risk; just add the unit test the plan promises (step 2).
- **A‑risky (L2):** "PE option order = same relative position as SE" — wrong; see L2.
- **A‑unstated:** the plan assumes the 2‑/4‑instance strand model + `--norc`/`--nofw` is identical for HISAT2. Confirmed in Perl (6371–6376 etc.) and consistent with the spike. ✅
- **A‑unstated:** HISAT2 emits `AS:i:` and `MD:Z:` on every aligned record (the merge dies without them, Perl 3405–3406 / Rust 545–566). The spike's tag survey (Q3) showed `NM MD XM XR XG` on the FINAL BAM but did not confirm HISAT2's RAW stream always carries `AS`/`MD` — worth a one‑line note (HISAT2 does, but it's an unverified assumption in the plan).

---

## Efficiency

No concerns — additive enum dispatch, reuses the proven pipeline. The plan's self‑assessment here is accurate. (The L1 fix is also zero‑cost: a per‑mate parse flag or a single `r1.second_best = None` masking step.)

---

## Validation sufficiency — could the gate pass while wrong?

**Yes, in two ways:**

1. **L1 (PE‑HISAT2 ZS asymmetry) can slip the gate.** The divergence only manifests on a PE read whose **mate‑1** carries a `ZS:i:` tag. Whether the 10k or even 1M oxy test data contains such a read is data‑dependent and uncontrolled. V5 ("ZS 2nd‑best parse — fake HISAT2 multi‑mapper") is specified as a *fake* that "merge selects/MAPQs identically" — but a fake built to the plan's (incorrect) mental model would assert the WRONG thing (it would likely give read‑1 a ZS and expect Rust to USE it, cementing the divergence). **Mandatory:** the V5 fake MUST include a PE pair where mate‑1 has `ZS:i:` and mate‑2 has `ZS:i:`, and assert the resulting MAPQ equals Perl's (mate‑1 ZS IGNORED). Add a dedicated unit test for the parser asymmetry independent of the gate.

2. **L4 (splice flags) silently no‑op.** If the gate argv never includes `--no-spliced-alignment`/`--known-splicesite-infile`, the gate passes while the flags are unwired — a latent regression for any real user who sets them. Add at least one gate cell (or unit test) exercising `--no-spliced-alignment` and assert the report `aligner_options` line carries it.

3. **V1 (Bowtie 2 byte‑frozen) is the right guard** and correctly placed before+after. Keep it; the L3 `--dovetail` change is exactly the kind of edit that could regress Bowtie 2 PE if mis‑gated, so V1 must include a Bowtie 2 **PE** cell (the plan says "full suite + gate" — ensure the suite has a PE‑dovetail assertion; `options.rs` test `paired_end_tail_and_default_maxins` 282–290 already pins it, so the kind‑gating must keep that test green).

4. **Report line gate:** V7 asserts the "run with HISAT2" line. Good — but note the report's `aligner_options` echo (`report.rs` 64–67) embeds the FULL option string, so L2/L3/L4 errors surface in BOTH the BAM and the report. Make V3 (PE option string) a *hard* pre‑gate unit assertion against the Perl‑derived literal, not just "byte‑match" hand‑wave.

**Overall:** the gate design is strong for the SE/naming/discovery surface but has a **blind spot exactly at the PE‑HISAT2 second‑best path** — the one place the SE‑only spike could not de‑risk.

---

## Alternatives

- **A1 — Split Phase 2.** The phase bundles: detection generalization, option assembly (+3 unmentioned sub‑deltas: dovetail‑suppress, splice flags, the late‑append order), discovery, a 6‑site naming refactor, the PE‑ZS fix, AND the full SE+PE+nondir+pbat+FastA gate. That is large and the PE‑ZS hazard deserves isolation. Recommend splitting: **2a = detection + options(all deltas) + discovery + naming + SE gate**; **2b = PE second‑best asymmetry + PE/nondir/pbat/FastA gate.** This puts the riskiest change (L1) in its own review/gate cycle. (Maps to the prompt's scope/sequencing probe — yes, it's too big as one plan given the hidden deltas.)
- **A2 — Don't generalize the `align.rs` parser; mask post‑parse.** Rather than threading a per‑mate XS/ZS flag through the parser, keep the uniform parser and, in the PE‑HISAT2 merge entry, set `r1.second_best = None` before the backfill. Smaller blast radius, leaves Bowtie 2 untouched, easy to test. Document whichever is chosen.
- **A3 — Fail‑loud the splice flags now, wire later.** If spliced‑junction support is genuinely out of the v1.x faithful scope, hard‑reject `--no-spliced-alignment`/`--known-splicesite-infile` in BOTH modes (matching Perl's non‑HISAT2 die) this phase, and defer the *append* wiring — cleaner than a silent no‑op and closes the pre‑existing Bowtie 2 gap.

---

## Action items — prioritized

### Critical (byte‑identity blockers; fix in the plan before implementation)
1. **L1 — PE read‑1 `ZS` asymmetry.** Add an explicit implementation task: under HISAT2 PE, read‑1's second‑best must be parsed **XS‑only** (Perl 3372–3382 has no ZS branch → always undef → backfilled to AS), while SE and PE read‑2 keep XS‑or‑ZS. Choose A2 (post‑parse `r1.second_best=None`) or a per‑mate parse flag. Add a dedicated unit test (PE pair, mate‑1 ZS present, assert MAPQ == Perl with mate‑1 ZS ignored). Do NOT rely on the V8 PE gate to catch it.
2. **L2 — correct the option‑order claim.** `--no-softclip --omit-sec-seq` are appended **last** (Perl 8286–8317), after `--ignore-quals`, the PE flags, `--minins/--maxins`, and `--quiet` — not "before the PE tail." Pin the exact PE literal in Behavior #3 and V3.
3. **L3 — suppress `--dovetail` for HISAT2 PE.** `options.rs` 143–152 pushes `--dovetail` for all PE; Perl gates it `if($bowtie2)` (8051–8059). The `kind` param must skip it for HISAT2. Add a PE‑HISAT2 option‑string unit test asserting NO `--dovetail`.

### Important
4. **L4 — decide + state the `--no-spliced-alignment` / `--known-splicesite-infile` handling** (wire into HISAT2 options + the both‑set die at 8290; hard‑reject in non‑HISAT2 mode per 8319–8324). At minimum fail‑loud, never silent no‑op. Add a gate cell or unit test.
5. **L5 — verify `--ambig_bam` + HISAT2 support before naming.** Trace whether Perl actually emits a HISAT2 ambig.bam (the "# only for Bowtie 2" temp‑name comment suggests not). If gated to Bowtie 2, hard‑reject `--ambig_bam` in HISAT2 mode; otherwise pin the exact name. Separately, ensure the `aligner_token` refactor does NOT touch `_unmapped_reads*` / `_ambiguous_reads*` (no token in Perl).
6. **V5 fake must encode the L1 truth** (mate‑1 ZS ignored), and add a `--no-spliced-alignment` validation cell (V‑new).

### Optional
7. **A1/A3 — consider splitting Phase 2** (SE+infra vs PE+second‑best+gate), or at least sequence the PE‑ZS fix + PE gate as the last, separately‑reviewed step.
8. Note the unverified assumption that HISAT2's raw stream always carries `AS:i:`/`MD:Z:` (the merge dies otherwise); a one‑line confirmation closes it.
9. Add the promised HISAT2 version‑line parse unit test (step 2) — low risk but currently only Bowtie 2 is tested (`aligner.rs` 116–118).
