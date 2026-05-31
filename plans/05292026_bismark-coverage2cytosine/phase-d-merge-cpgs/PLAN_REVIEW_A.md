# Phase D PLAN — Plan Review A

**Reviewer:** Plan Reviewer A (independent, fresh context).
**Target:** `phase-d-merge-cpgs/PLAN.md` (rev 0) — `--merge_CpGs` (+ `--discordance_filter`) for the Rust port of Perl `coverage2cytosine` v0.25.1.
**Method:** read the plan + SPEC (rev 3) + EPIC + Perl `combine_CpGs_to_single_CG_entity` (`:1753-1958`) line-by-line + shipped Phase B/C Rust (`report.rs`, `cov.rs`, `cli.rs`, `lib.rs`, `error.rs`); **ran live Perl v0.25.1** on six purpose-built fixtures (chr-start orphan, consecutive short scaffolds via 3-bp `CGT` scaffolds, EOF-mid-slide, discordance Δ=N boundary, both-measured gate, zero_based half-open, gzip filenames, round-half `%.6f`).

## Verdict

**APPROVE WITH CHANGES.** The core algorithm description (pairing, resync, discordance, pooling, skip-zero, filenames, zero_based half-open, gzip) is faithful to Perl and I confirmed every claimed output byte-for-byte against live v0.25.1. There is **one Critical** correctness gap (an EOF-mid-resync case where Perl *dies* on a well-formed genome, which the plan's "stop when <2 rows remain" + "asserts never fire on a well-formed report" framing actively contradicts), **two Important** gaps (a validation fixture that does not exercise the branch it claims to; private-visibility reuse not accounted for), and a few Optional polish items. None require re-architecting; all are localizable edits to the plan before implementation.

---

## What I verified against live Perl (all PASS)

| Claim (plan §) | Live-Perl result | Verdict |
|---|---|---|
| Merged line `chr1 2 3 50.495050 408 400` from the phase_b fixture (§2, V3) | exact | ✓ |
| Filename `merge.CpG_report.merged_CpG_evidence.cov`; report basename-derived | exact | ✓ |
| `--gzip` → `…merged_CpG_evidence.cov.gz` + `…discordant…cov.gz`; report `.txt.gz`; summary plain (§3.7, V2/V4) | exact | ✓ |
| gz report re-read by merge (Q2) | works; merge reads `*.CpG_report.txt.gz` fine | ✓ |
| Discordance Δ **exactly = N → NOT discordant** (strict `>`, §3.5, V-gap) | pair Δ=20 @ N=20 went to **merged**, not discordant | ✓ |
| Discordant 1-based both rows `chr pos pos pct m u` (§3.5) | exact (`chr1 2 2 90.000000 9 1` / `chr1 3 3 10.000000 1 9`) | ✓ |
| Both-measured gate (one strand 0,0 + big Δ → pooled, not discordant) (§3.5, V6) | exact (pooled `chr1 2 3 100.000000 10 0`) | ✓ |
| Skip-zero: uncovered pairs absent from merged (§3.6, V9) | exact (only the one non-zero pair emitted) | ✓ |
| `--zero_based` merged half-open `pos1 pos2+1`; discordant `pos pos+1` (§3.5/3.6, V7) | exact (`chr1 5 7 …`; disco `chr1 1 2 …`/`chr1 2 3 …`) | ✓ |
| zero_based resync threshold `pos1 < 1` (chr-start reported as 0) (§3.3) | exact | ✓ |
| chr-start resync, `chr1==chr2` ELSE branch (single advance) (§3.3) | exact, 3 merged lines match | ✓ |
| consecutive-short-scaffold SLIDE branch (`chr1 != chr2`, read-until-match + 1 advance) (§3.3) | exact; verified with **two** and **three** consecutive 3-bp `CGT` orphans | ✓ |
| `%.6f` parity Rust `{:.6}` vs Perl `sprintf` (50.495050, 12.5, 6.25, 66.666667, 33.333333) (§8.3, Q1) | byte-identical | ✓ |

The plan's empirically-observed section (§2) is accurate, and the resync port description in §3.3 matches Perl's actual control flow on every case I could construct **except** the one in C1 below.

---

## Logic review

### C1 (Critical) — EOF-mid-resync makes Perl **die**; the plan says asserts never fire on a well-formed report

The plan's §3.4 states: *"These never fire on a well-formed report; they guard against a desync bug."* and §3.2/§3.3 model termination as *"stop when fewer than two rows remain."* Both are **wrong for a real genome** with trailing single-CpG scaffolds.

I built a genome `>chr1 ACGTACGCGTA` + `>scafA CGT` + `>scafB CGT` (two ≥3-bp scaffolds whose only CpG is the chr-start `+`, with **no `-` partner**, appearing as the **last two report rows**). The report ends:
```
…
scafA   1   +   5   5   CG   CGT
scafB   1   +   4   6   CG   CGT      <- EOF after this
```
Live Perl trace (`:1844-1873`): the `scafA/scafB` pair hits `pos1<2`, `chr1 ne chr2` → SLIDE branch. `while(<IN>)` returns undef immediately (EOF) so the body never runs; `line1/line2` stay `scafA/scafB`. The post-slide `if ($pos1 < 2)` (`:1867`) is TRUE → `:1868-1872` does `$line1=$line2(scafB); $line2=<IN>(undef)`. `split /\t/, undef` → `$context2` undef → **sanity assert `:1887` dies**:
```
The context of the second line was not CG:    at coverage2cytosine line 1887
```
**Perl exit code 255.** I confirmed this empirically. Critically, the `chr1 6 7 55.000000 11 9` line written *before* the die **is present in the merged cov** (Perl flushes file handles on exit).

Why this matters:
- This is **NOT** a desync/corrupt-report bug — it is a perfectly valid Bismark genome (fragmented assemblies routinely have many short scaffolds; two adjacent single-CpG ones at the genome tail is entirely plausible). The plan frames the sanity asserts as guarding only against "a desync bug" and the termination as graceful — so an implementer following the plan would likely make `next_row() → None` terminate cleanly (producing a merged cov **without** the partial line being a die), diverging from Perl.
- For byte-identity the Rust port must **reproduce Perl's behavior**: error out (non-zero) *after* the already-written merged lines are on disk. The SPEC §10.6 partial-output-cleanup posture (`cleanup_partial_output_on_err`) would, if applied here, **delete** the partial merged cov — the opposite of Perl, which leaves it. (Note: c2c does not yet implement that cleanup, so this is a latent trap, not a current bug — but the plan should pin the intended behavior.)
- A naive Rust `next_row()` that the resync code `.unwrap()`s or that the sanity-assert path reads as `None` could also **panic** instead of returning a typed error — another divergence (panic vs Perl's `die` is acceptable for byte-identity of *files* but the Phase-E gate compares exit behavior implicitly; a panic also bypasses any flush).

**Action:** §3.3/§3.4 must explicitly state that (a) when the resync's read-ahead (`next_row()`) hits EOF, the subsequent field extraction yields empty/absent fields and the sanity asserts **legitimately fire** → `MergeCpgSanityViolation` (a non-zero exit), even on a well-formed genome; (b) the merged/discordant cov bytes written **before** that error must remain on disk (do NOT clean them up) to match Perl; (c) the resync's `next_row()` must be modeled as returning `Option` and the assert path must treat `None`/short-field rows as a sanity violation, never panic. Add a validation row that builds this exact genome and asserts the partial merged cov + the typed error.

### Resync is otherwise faithful (PASS)

The `chr1==chr2` ELSE branch (`:1875`, single advance) and the `chr1!=chr2` SLIDE branch (`:1852-1873`, read-until-chr-match then a single conditional `pos1<2` advance) both reproduced live Perl exactly, including the consecutive-short-scaffold case with **two and three** adjacent `CGT` orphans. The plan's prose in §3.3 ("slide forward until `chr1 == chr2` … then if still `pos1 < 2`, advance once more") is an accurate description of `:1852-1873`. Good.

One nuance worth a sentence in the plan: the post-slide advance at `:1867-1873` is a **single `if`, not a loop** — Perl does not re-loop if the landed chromosome's first row is itself a chr-start orphan. (It happens to be safe because there is at most one chr-start CpG per chromosome, so a single advance always clears `pos1<2` for the landed chr — *unless* the landed "chromosome" is itself a lone-orphan scaffold, which is the C1 EOF case or would otherwise be consumed by the slide.) Documenting this prevents an implementer from "helpfully" wrapping it in a `while`.

### Covered chr-start orphan's counts are silently discarded (note)

In every resync case, the chr-start `+` orphan's coverage (e.g. `chr1 1 + 9 1`) is **dropped** — its 9 methylated counts never reach the merged cov, because the orphan has no `-` partner to pool with. This matches Perl (the orphan `line1` is overwritten during resync). The plan does not call this out; it is correct behavior but worth one explanatory sentence so it is not later mistaken for a bug. (Confirmed live: the `9,1` orphan never appears in any merged line.)

### EOF / odd-row handling (PASS, with the C1 exception)

A report with an **odd** number of rows ending in a *single* lone orphan terminates cleanly: the trailing orphan becomes `line1` with `line2 = <IN> = undef` → `last unless (line1 and line2)` drops it with no die. Confirmed live (`scafZ CGT` as the sole trailing orphan → clean exit, orphan dropped, no merged line). The plan's "stop when <2 rows remain" handles this correctly. The dangerous case is specifically **≥2 consecutive trailing orphans that enter the SLIDE branch and exhaust the file** (C1).

---

## Assumptions

- **§8.3 `%.6f` parity** — verified byte-identical on six values incl. round-half (12.500000, 6.250000). Solid.
- **Q1 discordance compares the `%.6f`-rounded values** — correct: Perl stores `sprintf("%.6f",…)` *strings* in `$percentage_top/$bottom`, then `abs($a - $b)` numifies the strings → the rounded values are compared. The plan's recommendation (parse the formatted strings back to f64) is exactly right. The boundary risk (raw vs rounded Δ straddling integer N) is **negligible** with integer counts (Δ must fall within ~5e-7 of an integer N — measure-zero in practice) — keep as Optional, golden-verify.
- **Assumption 2 (report is genome-ordered, consecutive `+`/`-` except chr-start)** — correct, but should explicitly include "and except trailing/consecutive single-CpG scaffolds that have a `+` with no `-`" (the C1 driver).
- **Plan §2 "reuses `report::ReportWriter` … `report::report_path(config, None)`"** — see I2: those items are currently **private**.

---

## Efficiency

§6's streaming 2-row sliding window with occasional read-ahead is the right call and matches Perl's `<IN>` line-at-a-time model — O(report lines), no full buffering. No concerns. The resync only ever looks a bounded number of rows ahead (until the next chr-match), so a simple `next_row()` over a gz-aware `BufRead` is sufficient. Agreed.

---

## Validation sufficiency

Good coverage overall; the goldens are generatable from the in-repo Perl (I reproduced the workflow). Gaps:

### I1 (Important) — V8's "2-bp `CG` short scaffold" does **not** exercise the SLIDE branch it targets

I confirmed live: a **2-bp `CG` scaffold produces NO report row** — its single CpG `C` at pos1 has trinucleotide `CG` (len 2 < 3 → guard 1 drops it), and the `-` partner `G` at pos2 is the last base (guard 2). So a 2-bp scaffold is invisible to the merge pass and can never trigger the `chr1 != chr2` `while(<IN>)` slide. V8 as written only exercises the `chr1 == chr2` ELSE branch (the main chromosome's own chr-start orphan).

To genuinely test the SLIDE branch (the historical #98/#229 path) you need a **≥3-bp scaffold whose chr-start CpG IS emitted**, e.g. `>scaf CGT` (emits exactly one row `scaf 1 + … CG CGT`), and ideally **two consecutive** such scaffolds between two real chromosomes. **Action:** rewrite V8 to use `CGT`-type lone-orphan scaffolds (≥1, ideally 2+ consecutive) so the slide loop, the chr-match break, and the post-slide single advance are all covered. (I have a verified fixture: `>chr1 CGTACGCGTA` + `>scafA CGT` + `>scafB CGT` + `>chr2 CGAACGT` → merged `chr1 5 6 80.000000 8 2`, `chr1 7 8 30.000000 3 7`, `chr2 5 6 35.000000 7 13`.)

### Other validation gaps (Important/Optional)

- (Important, ties to C1) **Add a V for the EOF-mid-slide die**: two trailing consecutive `CGT` orphans → assert `MergeCpgSanityViolation` (non-zero) **and** the partial merged cov retains the pre-error lines.
- (Optional) **Multi-CpG, multi-line merged file**: V3 only emits one merged line (the phase_b fixture has one non-zero pair). Add a fixture with ≥2 non-zero pairs across ≥2 chromosomes (I verified `chr1 …`, `chr2 …` multi-line output) to catch a per-line/iteration bug that a single-line golden would miss.
- (Optional) **Discordance Δ=N boundary golden** (strict `>`): the plan describes it (§3.5) and Q1 flags golden-verify, but no V row pins it. Add a V: a pair with Δ exactly = N → must land in **merged**, not discordant. (I verified live with N=20, Δ=20.)
- (Optional) **Covered chr-start orphan dropped**: a V asserting the orphan's counts do not leak into any merged line.

---

## Alternatives

- **Reuse vs duplicate the report-line parser.** The plan adds a fresh `parse_report_row`. That is reasonable (the merge needs only 6 of 7 fields and treats them as `Vec<u8>`/`u32`), but consider asserting it stays in sync with the Phase-B *writer* format via a round-trip unit test (write a `ReportRow` through the Phase-B emit format, parse it back) so a Phase-B column change is caught locally rather than only at the sanity assert / golden. Optional.
- **Modeling the resync.** A clean Rust shape is a small state machine over an iterator with one-row peek/read-ahead, where `next_row() -> Option<ReportRow>` and the resync calls it explicitly; the sanity-assert step treats a `None` (or a row that fails to split into ≥6 fields) as `MergeCpgSanityViolation` rather than `unwrap`. This directly addresses C1 and keeps the EOF semantics identical to Perl's `<IN>`-returns-undef.

---

## Action items

### Critical
1. **C1 — EOF-mid-resync die.** §3.3/§3.4 must state that when the resync read-ahead hits EOF on a *well-formed* genome (≥2 consecutive trailing single-CpG `≥3bp` scaffolds), Perl's sanity asserts **legitimately fire** → exit non-zero (`MergeCpgSanityViolation`), and the merged/discordant lines written **before** the error must **remain on disk** (do not apply partial-output cleanup here — Perl leaves them). Model `next_row()` as `Option`; the assert path must convert `None`/short rows to the typed error, never panic. (Perl `:1854-1872`, `:1886-1897`; confirmed live, exit 255, partial `chr1 6 7 55.000000 11 9` retained.)

### Important
2. **I1 — fix V8.** A 2-bp `CG` scaffold emits no report row and cannot trigger the `chr1 != chr2` SLIDE branch. Rewrite V8 to use `≥3-bp` (`CGT`) lone-orphan scaffolds — ideally two consecutive between two chromosomes — to actually exercise the slide + chr-match break + post-slide advance. (Perl `:1852-1873`; confirmed live.)
3. **I2 — private-visibility reuse.** §2/§4 say Phase D "reuses `report::ReportWriter`" and calls `report::report_path(config, None)`. In the shipped code `ReportWriter` (enum), `report_path`, `summary_path`, and `report_name` are all **private** (no `pub`/`pub(crate)`). The plan must add a task to bump them to `pub(crate)` (and ideally reuse `report_name(output_raw, output_stem, None, cx, gz)` to derive the merge basename, since the merge filenames are built from the report **basename**, then re-prefixed with `output_dir` — matching Perl `$global_cyt_report` being a basename `:152`). Also: `cleanup_partial_output_on_err` (SPEC §10.6) is **not yet implemented** in c2c — the plan should not assume it exists.

### Optional
4. Add validation rows: EOF-mid-slide die (with partial-file-retained assert), a multi-line/multi-chromosome merged golden, the Δ=N discordance boundary, and the covered-chr-start-orphan-dropped check (all have verified live-Perl fixtures available).
5. Add one sentence to §3.3 noting the post-slide advance (`:1867`) is a single `if`, not a loop, and one sentence that the covered chr-start orphan's counts are intentionally discarded (no `-` partner).
6. Add a round-trip unit test tying `parse_report_row` to the Phase-B emit format so a future Phase-B column change surfaces locally.
7. Keep Q1 (rounded-vs-raw discordance at the integer-N boundary) as a golden-verified Optional — negligible with integer counts, plan's recommendation is correct.

---

*Recommend-only; the plan was not edited.*
