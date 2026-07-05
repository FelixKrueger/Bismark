# Phase D PLAN — Review B

**Reviewer:** Plan Reviewer B (independent, fresh context).
**Target:** `phase-d-merge-cpgs/PLAN.md` (rev 0) — `--merge_CpGs` (+ `--discordance_filter`) Rust port of Perl `combine_CpGs_to_single_CG_entity` (`coverage2cytosine:1753-1958`).
**Contract:** byte-identical file outputs vs Perl v0.25.1.

**Verdict: APPROVE WITH CHANGES.** The algorithm port is faithful — I verified the merged-cov golden, the chr-start resync (incl. consecutive short scaffolds), zero-based half-open coords, gz re-read, skip-zero, and the empty-but-created merged file **all against live repo Perl v0.25.1**, and they match the plan. But two items must be lifted from "Open/Non-blocking" to binding before implementation: (1) the discordance comparison MUST use the `%.6f`-rounded values, not raw f64 — I built a fixture where the naive raw-f64 path diverges from Perl and confirmed it; and (2) the plan's claimed reuse of `report::ReportWriter`/`report_path` glosses over that those items are **private** today. Plus an unhandled EOF-during-resync `die` path.

Everything below cites Perl line numbers and plan sections. All "verified live" claims were run against `/Users/fkrueger/Github/Bismark-c2c/coverage2cytosine`.

---

## 1. Logic review

### 1.1 The merge core (§3.2, §3.6) — correct, verified
Live Perl on the existing phase_b fixture (`chr1` pos2=403/400 `+`, pos3=5/0 `-`, `--merge_CpGs`):
```
merge.CpG_report.merged_CpG_evidence.cov  →  chr1\t2\t3\t50.495050\t408\t400
```
matches the plan's §2 empirical line exactly. Pooled `m=403+5=408`, `u=400+0=400`, `pct = 408/808*100 = 50.4950495… → %.6f = 50.495050`. The other (uncovered, 0,0) CpG pairs are absent (skip-zero, Perl `:1939`). **Q2 fully confirmed:** the merged `pct` is recomputed from pooled counts — and it *must* be, because the CpG report has **no percentage column** (`chr pos strand m u context tri`), so there is nothing to reuse. `408/808` and `12/22→54.545455` both verified live. ✔

### 1.2 Chr-start resync (§3.3) — correct, verified incl. the worst case
This is the highest-risk port (Perl bugs #98/#229). I exercised all three branches against live Perl:

- **Lone chr-start `+` then a real pair on the same chr** (Perl `:1875-1881`, default branch): report `chrA 1 +`, `chrA 5 +`, `chrA 6 -` → merged correctly pairs `(chrA5+, chrA6-)`. ✔
- **Consecutive short single-CpG scaffolds** (the literal Perl `:1846-1850` comment scenario): report `scafA 1 +`, `scafB 1 +`, `chrZ 3 +`, `chrZ 4 -` → merged `chrZ 3 4 100.000000 6 0`. The inner `while(<IN>)` slides the 2-row window forward until `chr1==chr2`, then the post-loop `if ($pos1 < 2)` recheck (`:1867`) correctly does **not** advance again (pos1=3). ✔
- **Zero-based** (`pos1 < 1` branch, Perl `:1810`): report written zero-based (`chrA 0 +`…); resync keys off the `pos1<1` threshold; merged half-open `chrA 4 6 …`. ✔

The plan's §3.3 prose mirrors Perl line-for-line, including the "read-until-chr-match" inner loop and the post-loop recheck. **No correctness gap in the happy/normal-degenerate paths.**

### 1.3 EOF-during-resync — **unhandled `die` path (gap)**
**Not covered by the plan.** I constructed a report ending in **two trailing lone chr-start rows on different chromosomes with nothing after** (two trailing short scaffolds):
```
scafA 1 + 5 0 CG CGC
scafB 1 + 7 0 CG CGT      <-- EOF
```
Live Perl: the inner `while(<IN>)` exhausts the file without a chr match, falls into the `if ($pos1 < 2)` recheck (`:1867`), does `$line2 = <IN>` → **undef**, then the sanity assert `:1887` (`$context2 eq 'CG'`) fires on the undef context → **`die "The context of the second line was not CG:"` (exit 255)**. The merged file is created but **empty (0 bytes)**.

The plan models `next_row() -> Option<ReportRow>` and the resync as "slide forward until `chr1 == chr2`," with **no specified behavior when `next_row()` returns `None` mid-resync**. The obvious Rust implementation (break on `None`, then `last unless line1 && line2`) would terminate **cleanly with an empty merged file and exit 0** — diverging from Perl's `die`/exit-255. The *file output* is byte-identical (empty in both), so this is not a file-byte divergence, but it is a success-vs-error behavioral divergence on a Perl `die` path the plan claims (§3.4) to reproduce as `MergeCpgSanityViolation`. The plan must explicitly decide: either (a) reproduce the `die` (an `Err(MergeCpgSanityViolation{…})` when resync hits EOF leaving `line2` empty), matching Perl's exit code; or (b) document it as an accepted divergence (clean empty-file exit), with rationale. Silence here will produce an unreviewed, arbitrary choice at implementation time. **Important** (degenerate/truncated input; file bytes still match).

### 1.4 Trailing odd lone row — correct but untested
A genuinely odd report (3 lines: one pair + a trailing lone `+` from an uncovered scaffold) — verified live: the pair merges, the trailing lone row is **silently dropped** by `last unless (line1 and line2)` (Perl `:1797`) *before* any resync or sanity assert. The plan's §3.3 ("terminate when fewer than two rows remain") covers it, but §9 has **no validation row** for the odd-trailing-lone-row case (distinct from V9 skip-zero). Minor — add a test. **Optional.**

---

## 2. The discordance numerics — Q1 is the headline finding (must be Critical, not "Open")

The plan files this as **§10 Q1 "Open / Non-blocking."** It is **load-bearing and must be a binding implementation directive.** I built the boundary fixture the plan's Q1 asks for and ran it live.

**Perl computes** (`:1911-1913`): `$percentage_top = sprintf("%.6f", m1/(m1+u1)*100)` (a STRING), same for bottom, then `abs($percentage_top - $percentage_bottom) > $disco`. Perl **numifies the two `%.6f` strings** and compares on the **6-dp-rounded** values, against an **integer** `$disco`, strictly `>`.

**Fixture:** `m1=1,u1=1` (top=50%), `m2=11,u2=9` (bottom=55%), `--discordance_filter 5`:
- Raw f64: `bottom = 11/20*100 = 55.000000000000007`; `abs(50.0 - 55.000…007) = 5.0000000000000071` → `> 5` is **TRUE** → naive raw-f64 path routes to **discordant**.
- Perl actual: `abs("50.000000" - "55.000000") = 5.0` exactly → `> 5` is **FALSE** → **merged**.

**Live Perl with N=5:** merged file = `chr1 2 3 54.545455 12 10`; discordant file = **empty**. So Perl merges. A raw-f64 implementation would wrongly emit to discordant and leave merged empty — a **byte-divergence in both output files**.

I then verified the plan's *recommended* approach in Rust: `format!("{:.6}", top)` → `parse::<f64>()` → `(tf - bf).abs() > N as f64` reproduces Perl exactly (FALSE → merges) across four boundary cases; the raw-f64 path diverged on **3 of 4**. So:
- (a) rounded comparison — **yes, required**;
- (b) strict `>` — yes;
- (c) 6-dp value vs integer N — yes.

**Action:** promote Q1 to a Critical implementation rule in §3.5/§8: discordance compares the `%.6f`-formatted-then-reparsed values (or equivalently round-to-6dp), **never** the raw products; add a dedicated boundary golden (e.g. the `1,1,11,9 / N=5` case → expect MERGED, empty discordant). The naive raw-f64 implementation is the *obvious default*, which is exactly why this needs to be stated as a rule, not an open question.

Note the subtlety the implementer must respect: Perl's numified `"50.000000"` parses to an *exact* `f64`; Rust's `"50.000000".parse::<f64>()` does too — verified equal. The `format!("{:.6}")` string itself is byte-identical to Perl `sprintf "%.6f"` (see §3 below), so reparsing it is safe.

---

## 3. `%.6f` formatting parity — verified (assumption #3 holds)

Plan assumption #3 (`{:.6}` == `sprintf "%.6f"`) re-confirmed beyond the percentages: I compared Rust `{:.6}` vs Perl `%.6f` on the real values (`50.495050`, `54.545455`, `33.333333`, `66.666667`) **and** half-even tie cases (`0.0000015→0.000002`, `0.0000025→0.000003`, `0.0000035→0.000003`) — **all byte-identical**. The discordant per-strand values (`75.000000`, `25.000000`) and pooled values verified live. ✔ No action.

---

## 4. Reuse / coupling — the plan understates a real prerequisite (Q3 partial)

§2 and §4 say Phase D "reuses `report::ReportWriter`," `report_path`, and the filename helpers. **As shipped, these are all private:**
- `enum ReportWriter` (report.rs:32) — no `pub`; `create`/`write_all`/`finish` — no `pub`.
- `fn report_path` (report.rs:453), `fn report_name` (report.rs:423) — private.
- `ReportWriter::create` hardcodes `Compression::default()`.

So "reuse" requires **promoting them to `pub(crate)`** (a small but real edit the plan should list as Task 0), or duplicating a tiny writer in `merge.rs`. Either is fine, but the plan presents reuse as free. Also: the merge needs the **report path** (`report_path(config, None)`) to *re-open* the just-written report — that helper is private too. **Important** (it's a concrete implementation prerequisite, not just style). Recommend: add a Task to `pub(crate)`-expose `ReportWriter`, `ReportWriter::{create,write_all,finish}`, and `report_path`, and reuse them verbatim (keeps the gz/plain + truncation logic single-sourced).

**`parse_report_row` coupling (Q3):** the report line is exactly 7 tab fields (`chr pos strand m u context tri`); Perl `split /\t/` into 6 vars drops the tri. The plan's `ReportRow` ignores tri — correct. `parse_report_row` should split on `\t` and require ≥6 fields (tri optional/ignored), strip trailing `\n`/`\r` (mirror `cov::parse_cov_line`'s CRLF handling for consistency — the report is written by this same crate so `\r` won't appear, but defensively matching is cheap). The strand field is a single byte `+`/`-`. One caveat: the report uses `pos - 1` under `--zero_based`, so `parse_report_row` must read whatever the report holds and the resync threshold (`pos1<2` vs `pos1<1`) keys off `config.zero_based` — verified the report-is-zero-based-too invariant live (single global config; the report and the merge share the same `--zero_based`, so no mismatch is possible). ✔

**Error name nit:** plan §4 adds `MergeCpgSanityViolation { detail }`; SPEC §10.6 names it `MergeCpGSanityViolation` (capital G). Pick one; the existing error enum uses `MergeCpgsWithCx` (lowercase `pgs`), so `MergeCpgSanityViolation` is consistent with the file. **Optional** (cosmetic).

---

## 5. Efficiency / streaming (§6, Q4) — sound, but the justification is wrong

The streaming decision (2-row window, no full-row buffer) is **correct** — O(report lines) time, O(1) memory. But §6's claim *"the resync only ever looks **a few rows ahead**"* is **false**. I verified live: on a genome of N consecutive short single-CpG scaffolds, a **single** `while(1)` iteration's inner resync loop reads to the **next real pair**, which can be the **end of the file** (6 lone-scaffold rows consumed in one go in my test; unbounded in principle). Memory stays O(1) (only `line1`/`line2` held), correctness is fine, but a reviewer or implementer who believes "a few rows ahead" might cap the read-ahead at a fixed window — which would **break** this real scaffold-heavy-genome case (the Perl `:1846` comment is explicitly about it). **Action:** correct §6 to "the resync slides a 2-row window forward, bounded only by EOF (it may consume the rest of the file in pathological all-short-scaffold genomes); memory is O(1) regardless — do NOT cap the read-ahead." **Important** (prevents a plausible mis-implementation).

---

## 6. Validation sufficiency (§9)

Strong table; goldens are the right mechanism (live Perl v0.25.1 is the oracle, confirmed runnable). Gaps:

- **Missing: discordance rounding boundary golden** (the Q1 case). V5 uses an obvious 80-vs-20 (Δ=80≫20) case that *any* implementation passes; it does **not** exercise the raw-vs-rounded divergence. Add a V-row: `m1=1,u1=1,m2=11,u2=9`, `--discordance_filter 5` → expect MERGED (`…54.545455 12 10`), discordant **empty**. This is the single most important test to add (catches the naive raw-f64 bug). **Critical** test gap.
- **Missing: EOF-during-resync** (§1.3) — two trailing lone chr-start rows on different chrs → Perl dies. Pin whatever behavior the plan chooses (§1.3 decision). **Important.**
- **Missing: odd trailing lone row** (§1.4) silently dropped. **Optional.**
- V8 (chr-start resync) is good but should explicitly include the **consecutive-short-scaffold** variant (the inner-while path), not just a single chr-start CpG — I verified both diverge in code path. Recommend splitting V8 into V8a (single chr-start) + V8b (≥2 consecutive short scaffolds). **Important.**
- V3/V4/V5/V7/V9 all match live-Perl behavior I reproduced. ✔
- V6 (both-measured gate, one strand 0,0 → pooled not discordant) matches Perl `:1904`. ✔
- V10 (sanity assert on corrupt report) good. Ensure it's distinct from the EOF case (§1.3).

---

## 7. Alternatives

- **Buffer-all-rows vs stream:** plan picks stream — correct (a human CpG report is tens of millions of lines; buffering doubles peak RAM atop the in-memory genome). No reason to reconsider.
- **Recompute pct in f64 vs integer-ratio formatting:** must match Perl's `m/(m+u)*100` f64 then `%.6f` — verified identical; no rational-arithmetic alternative needed.
- **Reuse vs duplicate the writer:** prefer `pub(crate)` reuse (single-sources the gz/truncation logic) over a `merge.rs`-local copy. Minor.

---

## 8. Action items

### Critical
- **C1 (numerics).** Promote §10 Q1 to a binding §3.5/§8 rule: the discordance comparison MUST use the `%.6f`-rounded values (`format!("{:.6}")` → reparse to f64 → `.abs() > N as f64`), **never** the raw `m/(m+u)*100` products. Verified live: raw-f64 routes `m1=1,u1=1 / m2=11,u2=9 / N=5` to discordant; Perl merges it (`54.545455 12 10`, empty discordant). The naive implementation is the default → must be stated as a rule. (Perl `:1911-1913`; plan §3.5, §10 Q1.)
- **C2 (test).** Add the boundary discordance golden for C1 (`1,1,11,9`, `--discordance_filter 5` → expect MERGED, empty discordant). The current V5 (Δ=80 vs 20) cannot catch the raw-vs-rounded bug. (plan §9 V5.)

### Important
- **I1 (reuse prerequisite).** Add an explicit task to make `report::ReportWriter` (+ `create`/`write_all`/`finish`) and `report_path` `pub(crate)` so Phase D can reuse them; today they are private (`report.rs:32,41,50,59,453`). The plan presents this reuse as free. (plan §2, §4.)
- **I2 (EOF-during-resync).** Specify behavior when `next_row()` returns `None` mid-resync (two trailing lone chr-start rows on different chrs). Perl **dies** via the `context2 eq 'CG'` sanity assert on an undef line (`:1867`,`:1887`; exit 255; empty merged file). Decide: reproduce as `Err(MergeCpgSanityViolation)` (matches exit code) or accept a clean empty-file exit-0 divergence (file bytes match either way). Pin with a test. (plan §3.3, §3.4, §9.)
- **I3 (efficiency claim).** Correct §6: the resync is bounded only by **EOF** (may consume the rest of the file on all-short-scaffold genomes — verified live), not "a few rows ahead." O(1) memory regardless; do not cap the read-ahead. (Perl `:1852-1865`, `:1846` comment; plan §6.)
- **I4 (test split).** Split V8 into single-chr-start vs ≥2-consecutive-short-scaffold variants (distinct Perl code paths, both verified). (plan §9 V8.)

### Optional
- **O1.** Add a test for a genuinely odd-line report (trailing lone `+` silently dropped by `last unless`). (Perl `:1797`; plan §9.)
- **O2.** Resolve the error-name casing: plan `MergeCpgSanityViolation` vs SPEC `MergeCpGSanityViolation`; the shipped enum uses `MergeCpgsWithCx`, so the plan's casing is the consistent choice — update SPEC §10.6, not the plan. (error.rs; SPEC §10.6.)
- **O3.** Have `parse_report_row` strip a trailing `\r` for symmetry with `cov::parse_cov_line` (defensive; the report is self-produced so `\r` won't occur). (cov.rs:43-50.)

---

## 9. Summary

The Phase D algorithm is a faithful, well-grounded port — I confirmed the merged golden, the full chr-start resync (incl. the consecutive-short-scaffold inner loop), zero-based half-open coords, gz re-read, skip-zero, and the empty-but-created merged file **all against live Perl v0.25.1**, and they match. The plan's two flagged risks (resync, discordance rounding) are real; the resync is handled correctly, but the **discordance rounding must move from "Open/Non-blocking" to a Critical rule with a dedicated boundary golden** — I demonstrated the naive raw-f64 path diverges from Perl on the exact boundary. Two smaller items: the "reuse `ReportWriter`/`report_path`" claim needs a `pub(crate)` task (they're private), and the EOF-during-resync `die` path is unspecified. Address C1/C2 (and ideally I1–I4) and this is implementation-ready.

**Verdict: APPROVE WITH CHANGES** (Critical: C1, C2).
