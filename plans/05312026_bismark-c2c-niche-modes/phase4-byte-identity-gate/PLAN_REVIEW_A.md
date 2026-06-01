# Phase 4 PLAN review — Reviewer A

**Target:** `plans/05312026_bismark-c2c-niche-modes/phase4-byte-identity-gate/PLAN.md` (rev 0)
**Reviewer:** A (independent; no shared state with Reviewer B)
**Date:** 2026-06-01
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` (branch `rust/c2c-v1x`, HEAD `a971183`)
**Method:** Read the plan + the shipped harness `scripts/c2c_byte_identity_matrix.sh` + Phase-1/2/3 source (`gpc.rs`/`drach.rs`/`report.rs`) + goldens + EPIC §4. Then **empirically ran the committed Rust binary (`rust/target/debug/coverage2cytosine_rs`) AND live Perl `./coverage2cytosine` v0.25.1** with each niche flag on the Phase-1/2/3 fixtures to verify the file names, the `ffs_nome` 0-byte trap, the `drach` standalone file-set, and the differential premises.

---

## TOP-LINE VERDICT: **APPROVE-WITH-CHANGES**

The plan is well-grounded: every output file name it lists is **empirically correct** (verified Rust≡Perl, with `-o c2c`), the `ffs_nome` 0-byte-cov trap is **real and correctly handled** (kept out of `REQUIRE_NONEMPTY`, still validated three ways), the `drach` standalone claim is **true** (no `.CpG_report.txt`/summary on either binary), and four of the five differentials are sound and falsifiable. The harness-integration discipline (cx-first, purge-on-pass, `find`-not-glob, "both cells ran" guards, gz decompress-compare) is correctly described.

The **changes** are: (1) one **require-nonempty over-assertion risk** — the `nome` cell lists the NOMe **GpC** streams as required-nonempty, but the plan only *justifies* the CpG streams (Assumption 8); on a WGBS-derived cov the GpC streams' non-emptiness is plausible-but-unverified and could fail-CLOSED the gate spuriously; (2) the plan does not call out that **differential #4 (ffs 10-col) requires a brand-new stash mechanism** (column count) the v1.0 harness has no precedent for; (3) the `nome` line-count differential is stated against `LINES_DEFAULT` but the relationship `<` is asymptotically-true-not-guaranteed and should be `!=` + `<=` framed carefully. None are Critical; the gate stays fail-CLOSED throughout.

---

## Checks I ran (empirical)

| # | Check | Method | Result |
|---|-------|--------|--------|
| C1 | `--gc` output file names | Rust `-o c2c --gc` on `g_gcgc`/`gcgc.cov`; ls | `c2c.GpC_report.txt`, `c2c.GpC.cov`, `c2c.CpG_report.txt`, `c2c.cytosine_context_summary.txt` — **matches plan §3.1/§3.2 exactly** |
| C2 | `--nome-seq` names | Rust `-o c2c --nome-seq` on `g_nome`/`nome.cov` | `c2c.NOMe.CpG_report.txt`, `c2c.NOMe.CpG.cov`, `c2c.NOMe.GpC_report.txt`, `c2c.NOMe.GpC.cov`, `c2c.cytosine_context_summary.txt` (summary has NO `.NOMe`) — **matches plan exactly** |
| C3 | `--drach` names + underscore | Rust `-o c2c --drach` on `g_top`/`top.cov` | `c2c_DRACH_report.txt`, `c2c_DRACH.cov` — **underscore-prefixed (not dot), matches plan; standalone (no CpG report / no summary)** |
| C4 | `--ffs` names + 10 columns | Rust+Perl `-o c2c --ffs` on `g_gcgc`/`gcgc.cov` | `c2c.CpG_report.txt` (NF=**10**) + summary; default NF=**7**; **lines equal (2==2)** — differential #4 premise holds |
| C5 | `--ffs --CX --gzip` names | Rust+Perl | `c2c.CX_report.txt.gz` (NF=10 decompressed) + summary — **matches plan** |
| C6 | **`ffs_nome` 0-byte cov on BOTH** | Rust+Perl `-o c2c --ffs --nome-seq` on `g_nome`/`nome.cov` | `c2c.NOMe.CpG.cov` = **0B on Rust AND 0B on Perl**; `c2c.NOMe.CpG_report.txt` = 110B (non-empty). **Confirmed — the rev-3 Critical is real and 0==0 byte-compare passes** |
| C7 | Full Rust≡Perl byte parity (file-set + per-file) | `cmp` Rust vs live Perl for `default`/`gc`/`ffs`/`nome`/`ffs_nome`/`drach` | **All file-name-sets IDENTICAL; all common files byte-identical** EXCEPT raw `.gz` (see C8) |
| C8 | `ffs_cx` raw `.gz` differs? | raw `cmp` then `gzip -dc` compare | raw `.gz` **differs in the gzip header only** (Perl writes mtime+OS byte; Rust zeros); **decompressed bytes IDENTICAL**. The harness decompress-compares gz → **PASS**. Non-finding for the plan (harness already correct) but proves `ffs_cx` *must* stay on the `.gz` path |
| C9 | `drach` standalone (differential #3) | Perl `--drach` file-set | only `c2c_DRACH_report.txt` + `c2c_DRACH.cov`; **NO `.CpG_report.txt`, NO summary** — differential #3 sound + falsifiable |
| C10 | gc-core == default (differential #1) | `cmp` gc `c2c.CpG_report.txt` vs default | **identical** — `--gc` adds GpC without altering core; premise holds |
| C11 | **nome GpC streams emptiness** | size of `nome` `c2c.NOMe.GpC_report.txt` / `.NOMe.GpC.cov` on `nome.cov` | **0B / 0B** on this fixture (no covered GpC-context Cs in the tiny cov) — see Important finding A-I1 |
| C12 | Perl version | `./coverage2cytosine --version` | `coverage2cytosine Version: v0.25.1` ✓ (matches harness pre-flight) |

---

## Findings by area

### Logic

- **L-OK1 (differentials 1/3/4/5 are sound + falsifiable).** Empirically: gc-core==default (C10), drach-has-DRACH-not-CpG (C9), ffs-10-col + lines==default (C4), ffs_nome-0-byte-cov-both-sides (C6). Each catches a distinct "both binaries no-op the flag" that the per-cell Rust≡Perl compare cannot. None is tautological.
- **L-Important (A-I2): differential #2 (`nome` lines `< default`) — the `<` is data-true but not flag-logic-guaranteed; frame as `!=` + monotone.** The NOMe ACG/TCG-upstream filter drops CpGs that lack an ACG/TCG upstream trinucleotide, so `LINES_NOME_CORE <= LINES_DEFAULT` always, and `< ` whenever *any* CpG fails the filter (essentially certain at hg38 scale). The differential is still a good "did the filter fire" gate, but if a pathological genome had every covered CpG in ACG/TCG context, `<` would FAIL while the flag worked correctly. Recommend the plan state the assertion as **`LINES_NOME_CORE != LINES_DEFAULT` AND `LINES_NOME_CORE < LINES_DEFAULT`** with a one-line note that the strict `<` relies on Assumption 8 (real hg38 has non-ACG/TCG covered CpGs). Low risk at scale; worth a sentence so a future maintainer doesn't read `<` as a hard invariant.
- **L-Optional (A-O1): differential #5's "while the NOMe core report is non-empty 10-col" sub-clause.** §3.3.5 asserts the `ffs_nome` core report is non-empty AND 10-col. That non-empty assertion is the same risk as A-I1 (depends on covered ACG/TCG CpGs). It is justified by Assumption 8 (CpG side), so it is sound — just note it shares the Assumption-8 dependency.

### Assumptions

- **A-Critical-adjacent → Important (A-I1): the `nome` cell's `REQUIRE_NONEMPTY` lists the NOMe *GpC* streams, but the plan only justifies the *CpG* streams.** `REQUIRE_NONEMPTY[nome] = "...NOMe.GpC_report.txt ...NOMe.GpC.cov..."` (§3.2). Assumption 8 only argues "covered ACG/TCG **CpGs** → the NOMe **core report + .NOMe.CpG.cov** are non-empty." It says nothing about the **GpC** streams. Empirically (C11) the GpC streams are 0-byte on the small WGBS-style fixture because no covered position lands on a GpC-context C. At full hg38 scale with the v1.0 **`--CX` cov.gz** (every covered C in every context, ~all GpC dinucleotides represented) the GpC streams will *almost certainly* be non-empty — but this is **unverified at scale and unjustified in the plan**, and a require-nonempty miss fails-CLOSED (good direction) but would **spuriously FAIL the v1.x gate** on a perfectly-correct run. **Action:** either (a) extend Assumption 8 to explicitly justify the NOMe GpC streams' non-emptiness for the `--CX` cov.gz input (the safe and likely-correct path — the cov is CX so GpC-context Cs are covered), or (b) downgrade `c2c.NOMe.GpC_report.txt`/`c2c.NOMe.GpC.cov` to existence-only (drop from `REQUIRE_NONEMPTY[nome]`) and rely on the file-set + byte compare (which still catch a missing/extra/diverging file). Same applies to `REQUIRE_NONEMPTY[gc]`'s `c2c.GpC.cov`/`c2c.GpC_report.txt` — though for the **non-NOMe `gc` cell** every GC dinucleotide is eligible (no ACG/TCG filter), so GpC non-emptiness on a CX cov is even safer; still, justify it. This is the single most important finding.
- **A-OK (Assumption 9 verified): `ffs_nome` `.NOMe.CpG.cov` is 0-byte — required-EMPTY, NOT in `REQUIRE_NONEMPTY`.** Empirically confirmed 0B on both binaries (C6). The plan's handling (file-set + 0==0 byte-compare + explicit differential #5) is sound and fail-CLOSED. The `⚠️` callout in §3.2 is correct and important.
- **A-OK (Assumption 7, cov reuse): sound** — same inputs as the v1.0 gate; the recipe-to-regenerate fallback is documented. Operational, resolved first oxy session (§10).

### Efficiency / Disk

- **E-OK: `ffs_cx --gzip` is the right disk mitigation and is correct.** C5/C8 confirm gzipped CX is byte-identical (decompressed) Rust≡Perl. cx-first + purge-on-pass keeps the working set to one cell. With the new GpC/NOMe streams (~CpG-report-sized each) and one gzipped CX cell, the ~87 GB headroom is adequate — the v1.0 9-cell gate already handled an un-gzipped ~40 GB CX with the same discipline.
- **E-Optional (A-O2): the `nome` cell writes FOUR full-genome streams uncompressed** (`NOMe.CpG_report.txt`, `NOMe.CpG.cov`, `NOMe.GpC_report.txt`, `NOMe.GpC.cov`) ×2 binaries simultaneously in one cell dir before purge. At hg38 scale the two report files are the large ones (covered positions only, so smaller than a CX report, but still GB-scale ×2 binaries ×2 report-files = up to ~tens of GB transiently). It fits under purge-on-pass, but if disk is tight on oxy this is the second-largest cell after `ffs_cx`. Consider noting it, or optionally allow `--gzip` on the `nome` cell too (gzip parity is local-golden-proven). Not gating.

### Validation-sufficiency

- **V-OK: the lean-6 cell set + 5 differentials is a reasonable scale-gate** given Phases 1–3 are already local-golden byte-identical and the split/gzip/zero per-mode permutations are golden-covered. The §3.5 mandatory fail-CLOSED self-test (broken-output → FAIL; no-op → FAIL; non-empty-ffs_nome-cov → FAIL) is the right pre-oxy gate and mirrors v1.0.
- **V-Important (A-I3): the `ffs` differential (#4) needs a NEW stash mechanism the v1.0 harness has no precedent for — the plan should say so explicitly.** The v1.0 harness stashes hashes/line-counts/nonempty-flags (`HASH_*`/`LINES_*`/`*_NONEMPTY`/`SPLIT_FILE_COUNT`) — there is **no column-count stash** today (verified: no `NF`/`awk -F` anywhere in the script). Differential #4 introduces `COLS_FFS` (e.g. `awk -F'\t' 'NR==1{print NF}'`) + `LINES_FFS`, plus `HASH_GC_CORE`, `LINES_NOME_CORE`, and a `drach` file-set capture. The plan's §5 step 1 mentions "differential stash vars" generically but does not flag that #4 is a *new kind* of stash (column count) — the implementer should add the var to the stash block (line ~234), capture it in `run_cell`'s `case` (like the existing stashes, BEFORE purge), and add the `diff_check` guarded on `ran ffs && ran default`. Worth one explicit sentence so the column-count capture isn't bolted on incorrectly (e.g. capturing after purge → empty var → silent skip). **Note the trailing-empty-field robustness is fine:** I verified `awk -F'\t' '{print NF}'` counts the 3 trailing empty FFS columns (a CHH line `...CTT\t\t\t`) as NF=10, so the column check is robust.
- **V-Optional (A-O3): `nome_split`/`drach_split` deferral is acceptable.** Per-chr writer lifecycle is local-golden-covered (`gc_split`, `nome_split`, `split` goldens exist). The plan correctly flags adding them as nice-to-have (§3.1 / §10 Open). Agree with the default (lean 6); the v1.0 `split` cell already exercises the per-chr lifecycle at scale as a regression.

### Alternatives

- **None blocking.** The "extend the shipped harness, no new script" decision is correct (additive cells + differentials + checklist). An alternative — a separate v1.x harness — would duplicate the fail-CLOSED machinery and risk drift; the plan rightly rejects it implicitly by reusing `c2c_byte_identity_matrix.sh`.

---

## Action items (prioritized)

### Critical
- *(none)* — no fail-OPEN hole, no wrong `REQUIRE_NONEMPTY` name (all names empirically verified Rust≡Perl), no tautological differential. The gate stays fail-CLOSED.

### Important
1. **A-I1 — justify or relax the NOMe/GpC `REQUIRE_NONEMPTY` entries.** `REQUIRE_NONEMPTY[nome]` includes `c2c.NOMe.GpC_report.txt` + `c2c.NOMe.GpC.cov`; `REQUIRE_NONEMPTY[gc]` includes `c2c.GpC_report.txt` + `c2c.GpC.cov`. Assumption 8 only justifies the *CpG* streams. Either extend Assumption 8 to argue the GpC streams are non-empty for the `--CX` cov.gz input (the safe, likely-correct path — a CX cov covers GpC-context Cs), or drop the GpC streams to existence-only (file-set + byte-compare still gate them). Otherwise a correct run could spuriously FAIL the v1.x tag.
2. **A-I3 — call out that differential #4 needs a new column-count stash.** The v1.0 harness has no `NF`/column-count stash precedent. Add an explicit §5 note: capture `COLS_FFS` (and `LINES_FFS`) in `run_cell`'s stash `case` **before purge**, guard the `diff_check` on `ran ffs && ran default`.
3. **A-I2 — frame the `nome` line differential as `!=` + `<` with the Assumption-8 caveat**, not a hard `<` invariant.

### Optional
4. **A-O2 — note (or optionally `--gzip`) the `nome` cell's four full-genome streams** as the second-largest disk consumer after `ffs_cx`.
5. **A-O1 / A-O3 — minor**: note that #5's non-empty-core sub-clause shares the Assumption-8 dependency; confirm the lean-6 vs +split decision with Felix (already flagged §10).

---

## Summary

**Verdict: APPROVE-WITH-CHANGES** — Critical: 0, Important: 3, Optional: 2.

I empirically ran the committed Rust binary AND live Perl v0.25.1 with every niche flag: **all output file names the plan lists are correct** (`c2c.GpC_report.txt`/`.GpC.cov`; the `.NOMe.*`; the underscore-prefixed `c2c_DRACH_report.txt`/`_DRACH.cov`), `--drach` is genuinely **standalone** (no CpG report/summary), `--ffs` is **10-column with lines==default**, the `ffs_cx` gz is byte-identical once decompressed (the harness already decompress-compares), and the **`ffs_nome` `.NOMe.CpG.cov` is 0-byte on BOTH binaries** with the core report non-empty — the rev-3 Critical is real and correctly handled (kept out of `REQUIRE_NONEMPTY`, still triple-validated). Four of five differentials are sound and falsifiable. The single most important finding: **`REQUIRE_NONEMPTY` lists the NOMe/GpC streams as required-nonempty, but the plan only justifies the CpG streams** — on the WGBS `--CX` cov this is likely fine, but it is unverified-at-scale and unjustified, and a miss would spuriously fail-CLOSED the v1.x gate; extend Assumption 8 to cover the GpC streams or downgrade them to existence-only. No fail-OPEN holes, no wrong file names, no tautological differentials.
