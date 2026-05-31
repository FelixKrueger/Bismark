# PLAN_REVIEW_B — Phase 1 (GpC report `--gc`/`--gc_context` + NOMe-Seq `--nome-seq`)

**Reviewer:** B (independent; no shared state with Reviewer A).
**Plan under review:** `phase1-gpc-report-nome-seq/PLAN.md` rev 0 (2026-05-31).
**Method:** read the plan + EPIC + v1.0 SPEC + the Perl `coverage2cytosine` v0.25.1 source (`generate_GC_context_report:751-1073`, the core-report NOMe hooks, `handle_filehandles:86-165`, `process_commandline:2138-2196`, main flow `:38-84`) + the shipped Rust `report.rs`/`cli.rs`/`lib.rs`/`error.rs`, and **ran the repo Perl v0.25.1 on 11 purpose-built fixtures** to verify every surprising claim from first principles (not on faith).

---

## TOP-LINE VERDICT: **APPROVE — implementation-ready.**

This is an unusually well-grounded plan. **Every** high-divergence-risk claim it makes (the bottom-then-top emit order; the three asymmetries — no `--zero_based` in GpC, the in-function threshold bump decoupled from the core, raw-`-o` filename doubling; the ACG/TCG upstream filter with `-`-strand revcomp; the NOMe `.cov` companion; the uncovered-chromosome skip; the CG-context GpC drop; the non-NOMe summary filename; the GCGC non-overlapping walk; the chromosome-edge guards; the `--CX`/`--merge_CpGs` dies; the no-`.cov`-for-non-NOMe-core; the empty-cov-can't-reach-gpc invariant) **reproduced byte-for-byte against live Perl v0.25.1 in my own fixtures.** I found **zero Critical** issues and **zero byte-identity divergences** in the plan's stated behavior. The findings below are all Important-or-lower: documentation gaps and validation-matrix tightenings that would harden the implementation, not correctness errors.

**Severity counts:** Critical 0 · Important 2 · Minor 4 · Nit 3.

---

## Live-Perl checks I ran (all PASS unless noted)

| Fixture | What it pins | Result |
|---|---|---|
| `AGCAGCGCATGCGGCATTAGCTAGC`, cov 2,3,6,7,8 `--gc` | GpC report+cov bytes; bottom-then-top within a dinucleotide; CG-GpC present | **matches §3 anchor exactly** (`6 + CGC`, `7 - CGC`, `8 + CAT`; cov `%.6f`) |
| `--gc --zero_based` vs `--gc` | Asymmetry (a): GpC has no `--zero_based` | **GpC report+cov byte-identical; core shifts 6→5,7→6** |
| `--gc` (no threshold) | Asymmetry (b): core@0 emits uncovered all-zero CpGs (12,13); GpC@≥1 drops them | **confirmed** |
| `--gc --coverage_threshold 3` | GpC `max(thr,1)`; core honors user thr (no uncovered pass) | **confirmed** (core: 6,7; GpC: 6,7,8) |
| `-o sample.CpG_report.txt --gc` | Asymmetry (c): GpC uses raw `-o` (suffix doubling); core dedup-strips | **`sample.CpG_report.txt.GpC_report.txt` + core `sample.CpG_report.txt`** |
| `GCAGCTTAGC`, cov 1,2,4,5 `--gc` | chromosome-edge guards (first/last GC dropped) | **only interior emits: `4 - CTG`, `5 + CTT`** |
| `AAGCGCAA`, cov 1–8 `--gc` | GCGC non-overlapping walk | **matches at j=2(pos4),j=4(pos6); no double-count** |
| `TTACGTTAGCATCGTT`, cov 4,5,13 `--nome-seq` | ACG/TCG upstream filter incl. `-`-strand revcomp; `.NOMe.CpG.cov` companion; summary no `.NOMe` | **kept 4(+,ACG),5(−,revcomp ACG),13(+,TCG); cov companion present; summary plain-named** |
| `--nome-seq` on `AGCAGC…` | NOMe core empty; NOMe GpC drops CG-GpCs, keeps only CHH | **confirmed (`8 + CAT` only)** |
| 2-chr split `--gc --split_by_chromosome` | `.chrchr1`/`.chrchr2` doubling; per-chr content; empty-last-chr file still created | **confirmed; even a GC-less last chr gets an empty `.GpC.cov`/`report`** |
| non-contiguous cov (chr1,chr2,chr1) `--gc` ± split | re-emit (single-file) / re-truncate (split) | **single-file re-emits chr1 twice; split truncates to 2nd segment** |
| empty cov `--gc` | dies in core before GpC | **`No last chromosome…` die; no GpC file** |
| `--nome-seq --CX` / `--merge_CpGs` / `--coverage_threshold 0` / `5` | mutexes + threshold honoring | **all match plan** |
| `--nome-seq --merge_CpGs --CX` (triple) | which die fires first | **MergeCpgsWithCx (the merge block precedes the nome block)** |
| `--nome-seq --zero_based` | core NOMe report+cov shift to `pos-1`; GpC cov does NOT | **confirmed dual-asymmetry** |
| plain run (no `--gc`/`--nome`) | no `.cov` written | **confirmed (only `.CpG_report.txt`+summary)** |

---

## Important

### I-1 — The GpC walk's **non-contiguous chromosome re-appearance** behavior is not explicitly stated, and it is a real byte-identity hazard if the implementer buffers per-chr across the whole file.
- **Plan section:** §3.3 ("buffer cov lines per-chromosome … flush each chromosome on `chr`-transition"), §3.5.
- **Issue:** Live Perl: in **single-file** mode, a cov file with chromosome order `chr1, chr2, chr1` (chr1 re-appearing) makes `generate_GC_context_report` re-walk the *entire* chr1 genome a **second time** using only the second segment's buffer, so chr1 GpC lines appear in two non-adjacent blocks (I verified: report had `chr1 3-, chr1 4+, chr2 3-, chr1 5-`). In **split** mode, the second chr1 segment **truncates** the first (`sample.chrchr1.GpC_report.txt` ended up containing only the 2nd-segment line `5 -`). This is identical to the *core* report's behavior (v1.0 SPEC §4 / §10.5), and the shipped `run_single`/`run_split` + `flush_split_chromosome` (report.rs:275-413) already do exactly this — **so reusing that pattern is correct** — but the plan never says "the GpC driver must flush purely on transition (re-emit) and reopen-truncate on every transition (no writer caching by name)". An implementer who, e.g., accumulates `HashMap<chr, buffer>` across the whole cov file (a natural-looking optimization for the "second pass") would silently produce a *coalesced* chr1 block in single-file mode and a *concatenated* chr1 file in split mode — both byte-divergent.
- **Why it matters:** This is the exact dual-driver/ordering trap the project memory warns about; the GpC walk is a *second independent driver* and is the natural place to get this wrong differently from the core.
- **Fix:** Add one sentence to §3.3 and §3.5: "Mirror `report.rs`'s flush-on-every-transition + reopen-truncate-on-every-transition contract exactly (no per-name buffer/writer caching) — a non-contiguous chromosome re-appearance re-walks the genome (single-file: re-emits; split: truncates the earlier file). Pinned by a non-contiguous-cov golden." Consider adding it to the validation matrix (see I-2).

### I-2 — The validation matrix omits a **non-contiguous-cov** golden and a **`--nome-seq --split_by_chromosome`** golden; both are exercised code paths the matrix should pin.
- **Plan section:** §9 (17 rows).
- **Issue:** (a) No row exercises a cov file whose chromosomes are non-contiguous (the I-1 hazard). V9 uses a clean 2-chr file in appearance order. (b) `--nome-seq --split_by_chromosome` is a real composition (NOMe ✗ merge, but NOMe composes with split) and is the **only** path where Perl's split-mode `close GCCOV` asymmetry (`:951-954`) actually *runs the `close GCCOV` branch* (it closes GCCOV at the transition only when `$nome`). The plan flags the quirk in §3.5 and Q-open #2 but V9 tests **non-NOMe** split only, so the `$nome`-true close branch is never differentially exercised against a golden. (c) `--gc --zero_based` is tested (V10) for GpC-identity, but no row pins that the *core* report still shifts under `--gc --zero_based` while GpC does not in the **same run** (V5 = core-under-`--gc`-equals-plain at threshold 0; V10 = GpC-identity; neither is the combined "core shifts + GpC frozen in one `--gc --zero_based` run" assertion).
- **Why it matters:** Phase 4's real-data gate is genome-driven and unlikely to feature non-contiguous cov; these are exactly the small-fixture edges that catch a buffering/lifecycle regression cheaply.
- **Fix:** Add V18 (non-contiguous cov, single-file + split, vs Perl golden), V19 (`--nome-seq --split_by_chromosome` 2-chr, vs Perl golden — pins the NOMe split GCCOV path), and tighten V10 to also diff the *core* `.CpG_report.txt` of the `--gc --zero_based` run against the zero-based plain core (proving the in-run asymmetry). I verified the non-contiguous and NOMe-split behaviors hold in live Perl, so the goldens will be stable.

---

## Minor

### M-1 — §3.6.3 / V7 over-states the GCGC double-counting risk; for the literal 2-char `GC` pattern, a naive `for j in 0..len-1` scan is byte-identical to `/(GC)/g`.
- **Plan section:** §3.6.3, §11 Risks (a).
- **Issue:** The plan warns that a `for j in 0..len-1` loop "would double-count `GCGC`" / "would wrongly also match at `j=1` if `seq[1..3]=='GC'`". That cannot happen for the fixed pattern `GC`: after a match at `j` (so `seq[j]='G', seq[j+1]='C'`), the next index `j+1` has `seq[j+1]='C'`, which can never start a `GC`; and stepping `j+=2` can never skip a valid `GC` (it would require `seq[j+1]='G'`, contradicting `seq[j+1]='C'`). So naive-scan ≡ non-overlapping `/(GC)/g` for *this* pattern (I confirmed `re.finditer("GC")` and Perl agree on `AAGCGCAA` → matches at j=2,4). The `j += 2` recommendation is harmless and fine; the *rationale* is incorrect, which could mislead a future maintainer porting a different (overlapping-capable) pattern.
- **Fix:** Soften §3.6.3 to "`GC` is a fixed 2-char pattern, so non-overlapping `/(GC)/g` and a per-index scan coincide; implement either (the `j += 2` step is the literal Perl `pos()` advancement). Pin with the GCGC golden anyway." Keep V7.

### M-2 — §3.1 step 4's phrase "the core report runs at threshold 0" is only the *no-explicit-threshold* sub-case.
- **Plan section:** §3.1 step 4, §10 (Q "Where does the `--gc` threshold bump apply").
- **Issue:** With `--gc --coverage_threshold 3` (a legal combo — `--coverage_threshold` is mutex only with `--merge_CpGs`), the core report runs at **3**, not 0, and skips the uncovered pass (I verified: core emitted only pos 6,7; GpC used `max(3,1)=3`). The plan's general rule (`config.threshold` for the core, `max(config.threshold,1)` for GpC) is correct, but the prose "for `--gc` without `--nome-seq`, the core report runs at threshold 0 (full uncovered genome)" reads as unconditional.
- **Fix:** Qualify: "…at the *user's* threshold (0 by default ⇒ full uncovered genome); only the GpC walk uses `max(config.threshold, 1)`." Add a V-row for `--gc --coverage_threshold N` (core@N, GpC@max(N,1)) — currently untested.

### M-3 — `gpc_threshold = max(config.threshold, 1)` vs Perl's `if ($threshold == 0) { $threshold = 1 }` — equivalent, but the plan mutates a *shared* `$threshold` in Perl whereas Rust must use a *local*.
- **Plan section:** §3.1 step 4, §3.3.
- **Issue:** Perl line 758-761 mutates the **package-global `$threshold`** inside `generate_GC_context_report`. Because the GpC function runs *last* (main flow `:82`), this mutation is harmless in Perl (nothing reads `$threshold` after). The plan correctly says "do **not** raise `ResolvedConfig.threshold`; the GpC walk computes its own `gpc_threshold`". This is the right call (`ResolvedConfig` is `&`-shared and immutable), and `max(threshold,1)` is numerically identical to Perl's `==0?1:threshold` for all `threshold ≥ 0`. Just flagging that the plan should keep this as a *local* in `run_gpc` and never touch the config — which §3.1 step 4 does say. No change strictly required; calling it out so the implementer doesn't "faithfully" mutate shared state.
- **Fix:** None required; optionally add "(local to `run_gpc`; never mutate `config`)" to §3.3.

### M-4 — The core NOMe `.cov` **zero-based** coordinate shape is a *point* `(pos-1,pos-1)`, not a half-open interval — worth pinning to avoid copying the merge cov's half-open convention.
- **Plan section:** §3.2 step 2, §8 assumption 4.
- **Issue:** I verified `--nome-seq --zero_based`: the core NOMe `.cov` writes `chr (pos-1) (pos-1) %.6f m u` (both start and end = `pos-1`; e.g. `chr1 3 3 75.000000 3 1`). Contrast the `--merge_CpGs --zero_based` cov which is **half-open** `chr pos pos+1` (v1.0 SPEC §9). An implementer reusing merge-cov code could accidentally emit `(pos-1, pos)`. The plan's §3.2 step 2 text ("`chr out_pos out_pos`…where `out_pos` honours `--zero_based (pos-1)`") is correct (both columns = `out_pos`), but the matrix has no `--nome-seq --zero_based` golden.
- **Fix:** Add a `--nome-seq --zero_based` golden row (core report + `.NOMe.CpG.cov` both shift; GpC cov does NOT shift in the same run — a clean triple-discriminator). I verified the expected bytes.

---

## Nit

### N-1 — §3.2 step 4 ("gate the uncovered pass on `config.threshold == 0 && !config.nome`") — Perl's structure is a 3-way `if($nome){skip} elsif($threshold>0){skip} else{process}`.
- The plan's combined boolean `threshold == 0 && !nome` is logically equivalent to Perl's branch outcome (process iff not-nome AND threshold==0) and the plan acknowledges the `!nome` is technically redundant (NOMe always has threshold ≥1). Fine; the explicit `!config.nome` is good defensive clarity. No change needed; just confirming the equivalence holds (verified: NOMe skips uncovered chr; threshold>0 skips uncovered chr; threshold==0 non-nome processes them).

### N-2 — §3 "Empirically observed" GpC anchor: the `sample.GpC_report.txt` shows `6 +` *before* `7 -`, which superficially contradicts "bottom before top".
- The ordering rule is **within a single GC dinucleotide** (bottom strand printed before top). `6 +` and `7 -` belong to *different* dinucleotides (j=4 pos=6 top; j=6 pos=8 → its bottom is pos 7). The plan states this correctly (§3 line "Ordering within a `GC`"), but a reader skimming the anchor block might misread. Consider annotating the anchor with the dinucleotide each line belongs to.

### N-3 — error.rs already has `MissingCovInput`, `MergeCpgsWithCx`, etc.; the two new variants `NomeWithCx`/`NomeWithMerge` are appropriately scoped, but confirm the `error_exit_code()` mapping (cli error → exit code) is extended for them.
- §4/§5 add `NomeWithCx`/`NomeWithMerge`. The shipped `error.rs` maps some variants to exit codes (e.g. `UnsupportedFlag` at :161). Ensure the two new CLI-validation errors map to the same "usage error" exit code as the existing mutexes (`MergeCpgsWithCx` etc.), not the default. STDERR text is exempt from byte-identity (SPEC §2), but the exit-code class should match the other validation dies. Trivial; just don't forget the mapping.

---

## Items I explicitly checked and found CORRECT (no action)

- **Bottom-then-top emit order within a dinucleotide** (Perl `:917-939` / `:1034-1059`) — correct.
- **`pos = j+2`; top-C at `pos`; bottom-C at `pos-1`; `tri_nt_top = seq[j+1..j+4]`; `tri_nt_bottom = revcomp(seq[j-2..j+1])`** — all correct; the `pos-4 = j-2` negative-wrap dropping the chromosome-start GC is correctly modeled via `perl_substr`.
- **Both `len<3` guards skip the WHOLE dinucleotide** (not just one strand) — correct (Perl `next`).
- **Unclassifiable context → `next` (skip whole dinucleotide)** — correct.
- **NOMe ACG/TCG filter is on the upstream trinucleotide, `-` strand revcomp'd** — verified (pos 5 `-` kept via revcomp upstream `ACG`).
- **Non-NOMe core writes NO `.cov`** (Perl opens `CYTCOV` only when `$nome`) — verified.
- **NOMe `.cov` filename `{stem}.NOMe.CpG.cov`; summary `{stem}.cytosine_context_summary.txt` (no `.NOMe`)** — verified (summary name taken at `handle_filehandles:115` before the `.NOMe` append at `:121`).
- **NOMe uncovered-chromosome skip** (Perl `:708-713`) — verified.
- **`--CX`/`--merge_CpGs` dies; triple-combo fires merge-die first** — verified; the plan's "add NOMe block after the merge-mutex checks" preserves the order.
- **`gpc_threshold ≥ 1` ⇒ no division-by-zero** (V16) — sound; `meth+nonmeth ≥ 1` guaranteed before the `%.6f` divide.
- **Empty cov can't reach `gpc.rs`** (dies in core `EmptyCoverageInput`) — verified.
- **`lib::run` order `report → merge → gpc`** matches Perl main flow `:44/:58/:82`; genome already loaded & passed by `&` (no reload) — verified against shipped `lib.rs`.
- **The two Open (non-critical) questions** (Phase-3 FFS composition; GCCOV-not-closed split quirk) — correctly classified as non-Critical; the FFS-vs-NOMe-`.cov` are mutually-exclusive Perl branches (`if($tetra){…}else{if($nome){…}}`), so v1.0-no-FFS emitting the NOMe `.cov` unconditionally inside the CpG branch is safe and Phase 3's later sibling-branch note is the right hand-off.
- **`--gc --gzip`** (decompressed identical; summary plain) and **`--gc --merge_CpGs`** (both merged + GpC files emitted) — verified.

---

## Recommendation

**APPROVE.** Fold I-1 (one-sentence non-contiguous/lifecycle contract in §3.3/§3.5) and I-2 (three matrix rows: non-contiguous cov, `--nome-seq --split`, in-run `--gc --zero_based` core-vs-GpC discriminator) before implementation; treat M-1…M-4 / N-1…N-3 as polish. None of these block implementation, and none indicate a byte-identity error in the plan's stated behavior — they harden the *implementation* against plausible mis-ports and tighten the *goldens* so a regression is caught on the tiny fixtures rather than only on the oxy gate. Recommend-only; no files were modified other than this report.
