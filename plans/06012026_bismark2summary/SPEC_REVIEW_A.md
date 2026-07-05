# SPEC_REVIEW_A — `bismark-summary` (Rust port of Perl `bismark2summary`)

**Reviewer:** A (independent; fresh context; no coordination with Reviewer B)
**Date:** 2026-06-01
**SPEC reviewed:** `plans/06012026_bismark2summary/SPEC.md` rev 0
**Perl source of truth:** `bismark2summary` v0.25.1 (1722 LOC), read in full and exercised empirically.
**Method:** every numeric / ordering / branch claim was checked against the source by line number and, where a Rust↔Perl divergence was plausible, by running Perl (`dangerouslyDisableSandbox`) and a standalone `rustc` reimplementation of `format_g15`. Two end-to-end Perl runs were done on hand-built fixtures.

**Verdict:** The SPEC is unusually thorough and the hard parts (the `%.15g` percentage engine, the stale oracles, the section-deletion mechanics, the raw-vs-mutated `.txt` split, the methylation overwrite precedence) are correctly characterised and, where I could test them, **empirically confirmed faithful**. There is **one Critical correctness gap** (glob sort order — the proposed Rust approach is provably wrong) and **one Critical missed latent bug** (the single-raw-sample numbers/percentage section asymmetry, which the SPEC's labels actively mischaracterise). Both are byte-identity-breaking. With those fixed plus a handful of Important clarifications, the SPEC is sound and ready to proceed to PLAN.

---

## 1. Logic review

### 1.1 What I verified as CORRECT (with evidence)

- **§2.9a percentage engine — VERIFIED EXACT.** I copied `fmt_g::format_g15` verbatim into a standalone `rustc -O` binary and compared `format_g15(100.0 - "<%.2f>".parse::<f64>())` against a fresh Perl `100 - $pm` for **all 10,001** two-decimal values `0.00..=100.00`. **Zero mismatches**, including the floating-point-noise cases the round-2dp→reparse→subtract→`format_g15` recipe must reproduce: `100-"99.99" → "0.0100000000000051"` and `100-"99.98" → "0.019999999999996"` (Perl and Rust agree bit-for-bit). The asymmetry (meth/alignment `%.2f` verbatim with trailing zeros kept; the six unmeth `%.15g` with trailing zeros dropped: `"50"`, `"0"`, `"100"`, `"87.7"`) is correctly stated and reproduced. **`fmt_g::format_g15` is the right tool and the §2.9a recipe is faithful.** This was the SPEC's single largest numeric risk and it holds. (Spike A is therefore confirmatory-only, not blocking — as the SPEC already says.)

- **Stale oracles — VERIFIED.** `docs/images/bismark_summary_report.txt` header literally reads `Methylated CpHs` / `Unmethylated CpHs` in cols 12–13 (current source emits `Methylated chgs` / `Unmethylated chgs`, lines 240–241). `docs/images/bismark_summary_report.html` has **0** `Plotly` tokens, **16** `highcharts` tokens, footer `version 0.15.2`. Both §7 staleness claims are exactly right; the "worse trap than bismark2report" framing is justified.

- **`.txt` lowercase `chgs` quirk — VERIFIED** against a live Perl run (the generated `.txt` shows `Methylated chgs`/`Unmethylated chgs`; CpG and CHH stay capitalised). §2.6 captures it.

- **Parsers §2.5 — faithful.** The PE/SE pattern split (lines 290–303), the `$`-anchored `total_c`/`total_reads`/… vs the **unanchored** six meth/unmeth patterns (306–311 have no trailing `$`), the dedup `aligned_reads` overwrite (331), and the splitting overwrite using `Total C to T conversions` instead of `Total unmethylated C's` (377–379) are all correctly transcribed. Last-match-wins (scan all lines, overwrite) is correct.

- **`.txt` raw-vs-mutated split — faithful.** Row captured at 387–404 (raw, blanks kept) **before** the 0-defaulting + aligned-blanking at 412–424. The `if ($dup_reads ne '') { $aligned_reads = "" }` only affects the plot arrays; the `.txt` keeps the dedup "Total alignments analysed" count. I traced this end-to-end and confirmed the `.txt` for a single sample shows `Aligned Reads=900` with empty dup/unique columns. §2.6/§2.7.2 are correct.

- **`substr($bam,0,-4)` — confirmed**, including the documented non-`.bam` edge. Additionally: inputs **< 4 chars** yield `""` (Perl returns empty string, not undef) — a benign extra edge the SPEC could note but need not gate.

- **`name` label regex `s/_bismark.bam$//` (unescaped `.`) — confirmed** it matches any char and is a no-op on modern `*_bismark_bt2.bam` names (§2.7.1 is right).

- **Template marker counts — confirmed by grep on the heredoc (lines 490–1371):** `plotly_goes_here`×2, both logos×1 (Perl uses non-global `s///` — matches), `report_timestamp`×1, `page_title`×2, `num_samples`×2, and **every** `{{…_section}}` marker appears **exactly twice** — validating the "first…last marker splice" approach (§8.5). The data markers `p_aligned_replace`/`p_deduplicated_unique_alignments`/`p_duplicated_alignments` appear once each and Perl fills them non-globally — matches §2.9 step 9.

- **Empty-asset normalizer edge — confirmed.** A truly empty file leaves `$doc` **undef** (the `while(<DOC>)` never iterates). For byte output undef ≡ `""` in the later `s///` injection, so the §2.8/§8.3 "empty → empty" guard is correct and (for the three real, non-empty assets) defensive only.

- **RRBS+WGBS-mix `die` (1488–1490) and the latent CHH `total_CHG==0` bug (1662):** correctly identified. The CHH bug is dead for *plotted* samples (plot-exclusion at 427–440 guarantees all three context totals > 0), so reproducing the buggy branch verbatim is correct and safe.

### 1.2 CRITICAL — glob sort order is specified WRONG (byte-identity-breaking)

§2.3 / §4.8 / §8.6 instruct: *"Reproduce Perl's glob sort exactly (lexical/bytewise; use `LC_ALL=C`-equivalent ordering — a plain `Vec<String>` `sort()`)."* **This is incorrect.** Perl's `glob`/`<*…>` does **not** sort bytewise; it uses `File::Glob`'s default (`csh`-style, `GLOB_ALPHASORT` via case-folding collation). Empirically:

| Files present | Perl `<*bismark_bt2.bam>` | Rust `Vec::sort()` (bytewise) | `LC_ALL=C ls` |
|---|---|---|---|
| `apple`, `Mango` | `apple, Mango` | `Mango, apple` | `Mango, apple` |
| `aba`, `abc`, `abZ` | `aba, abc, abZ` | `abZ, aba, abc` | `abZ, aba, abc` |

(Both tested across `LC_ALL=C`, `C.UTF-8`, `en_US.UTF-8`, `POSIX` — Perl glob order is **invariant** and case-insensitive; uppercase `Z` (0x5A) sorts **after** lowercase `a`/`c`, the opposite of bytewise.) A plain Rust `.sort()` is bytewise and **diverges from Perl glob whenever sample names mix case** at a distinguishing position. Because row order in **both** the `.txt` and the `.html` (and the `categories`, all 13 y-arrays, the `x_values` count) is the discovery order, this is a hard byte-identity failure for any real multi-sample directory with mixed-case basenames (extremely common, e.g. `Sample`, `WT`, `KO`, `input` mixed with lowercase).

**Required fix:** replicate Perl `File::Glob`'s collation — fold ASCII case before comparing, with a deterministic tiebreak for case-only differences (on a case-insensitive macOS FS I couldn't isolate the case-only tiebreak; the implementer should determine it on a case-sensitive FS, e.g. Linux/oxy, or read the `bsd_glob`/`strcoll` behaviour). A safe portable approximation that matched every case I tried: sort by `(lowercased_key, original_bytes)`. **And the §7 fixture matrix MUST include a mixed-case multi-sample directory** (e.g. `apple_…`, `Mango_…`, `zebra_…`) whose Perl-vs-Rust row order would differ under a bytewise sort — otherwise the gate cannot catch this. Note the argv path (explicit BAMs, verbatim order) is fine; only the auto-glob path is affected.

### 1.3 CRITICAL — the single-raw-sample numbers/percentage section asymmetry is MISSED (and the SPEC labels mischaracterise it)

§2.9 step 8 says the **numbers** section deletion is *"gated on `$dup_alignments =~ /^,{1,}$/`"* and frames the two branches as **"all-commas (RAW / RRBS mode)"** vs **"else (DEDUP / WGBS mode)"**. The phrasing conflates "all-commas" with "RRBS mode". They are **not** the same, and for a **single raw (RRBS) sample** the script produces a genuinely inconsistent — but real and byte-significant — output that the Rust port must reproduce:

- The **numbers** section (line 1430) is gated on `$dup_alignments`. With one RRBS sample, `dup_alignments_arr = ("")` → `$dup_alignments = ""` → `"" !~ /^,{1,}$/` → takes the **`else` (DEDUP-layout) branch** (deletes the raw-aligned span at 1439, keeps the dedup+dup spans).
- The **percentage** section (lines 1486 / 1577) is gated on `if ($aligned)`. With one RRBS sample `$aligned = "900"` (truthy) → takes the **RAW branch** (`p_aligned_replace` filled = `90.00`; dedup/dup percentage spans deleted).

So numbers and percentages are built from **opposite mode branches**. I confirmed this **end-to-end**: running current Perl on a single RRBS-SE fixture, the NUMBERS `traces1` retains the Raw Aligned trace with `y:[]` AND the Deduplicated/Duplicate traces (also empty `y:[]`), while the PERCENTAGE `traces2` shows only `p_aligned=90.00` (raw). **No `{{…}}` placeholders survive.** With **≥2** RRBS samples, `$dup_alignments = ","` (or more) **does** match `/^,{1,}$/`, so the numbers section takes the raw branch and numbers/percentages agree — the single-sample case is the divergent one.

**Why this matters for the SPEC:**
1. The numbers gate uses `$dup_alignments`; the percentage gate uses `$aligned`. These are **two different variables** that can disagree. The SPEC step 9 says percentage deletion is a *"mirror of step 8"* — it is **not** a mirror; it keys off a different variable. An implementer trusting "mirror" will use the same predicate for both and diverge on the single-raw-sample case.
2. The "RAW/RRBS mode" vs "DEDUP/WGBS mode" labels are misleading: the numbers branch is selected by `$dup_alignments`-all-commas, not by RRBS-ness.

**Required fix:** restate §2.9 steps 8 and 9 to make explicit that (a) the **numbers** section deletion keys off `$dup_alignments =~ /^,{1,}$/` (line 1430), (b) the **percentage** section deletion keys off `if ($aligned)` truthiness (line 1577), (c) these are independent predicates, and (d) a **single raw/RRBS sample** produces a numbers section in the DEDUP layout but a percentage section in the RAW layout — and add this exact fixture to §7 (a directory with exactly ONE RRBS sample), generating the Perl oracle, so the gate pins it. Reproduce verbatim; do not "fix" the asymmetry.

### 1.4 Important — `$aligned =~ /^,{1,}$/` requires ≥1 comma; single-sample arrays never match

Confirmed: `join(",", ("")) = ""` and `"" !~ /^,{1,}$/`. So for a **single dedup sample**, `$aligned` (after blanking) is `""` not `","` — the line-1412 self-blank is a no-op and `$aligned` is already `""` (falsy) → percentage DEDUP branch. And `$dup_alignments = "50"` (the dup count, one element, no comma) → numbers DEDUP branch. So **single dedup (WGBS)** is internally **consistent** (both DEDUP). Good — but this confirms the regex semantics the SPEC relies on, and the asymmetry in §1.3 arises specifically because the single-RAW case has a truthy `$aligned` but a comma-less `$dup_alignments`. The SPEC should state the regex needs ≥1 comma (so N=1 never matches) because it is load-bearing for both the consistent (WGBS) and inconsistent (RRBS) single-sample cases.

### 1.5 Important — replacement must match the full `{{…}}` token (both braces)

I enumerated every `{{token}}` in the heredoc and checked for substring collisions. None of the full bracketed tokens is a substring of another (e.g. `{{no_seq}}` is **not** a substring of `{{p_no_seq_replace}}` because the closing `}}` differs; `{{deduplicated_unique_reads_percentage_section}}` vs the data marker `{{p_deduplicated_unique_alignments}}` are disjoint). Perl's `s/\{\{name\}\}/…/` anchors both braces, so a Rust `.replace("{{name}}", val)` is equivalent and safe. The SPEC implies this but never states it explicitly; add a one-line note that every replacement key includes both `{{` and `}}` (cheap insurance against a future implementer keying on a brace-less name).

### 1.6 Minor logic notes

- §2.9 step 4: `s/\{\{report_timestamp\}\}/…/g` is global but the marker occurs once (confirmed) — harmless; SPEC is fine.
- The fill order is correct and load-bearing: `{{x_values_alignment}}`/`{{filenames_replace}}`/`{{aligned_seq}}` etc. are filled (some inside spans that later get deleted) **before** the section deletions (1430+). The SPEC's ordered-mutation list (§2.9) preserves this; good. (A Rust impl that deleted-then-filled would *happen* to match here, but following Perl's order is the safe choice the SPEC already takes.)
- `num_samples` (total, incl. plot-excluded) vs y-array length (plotted only) mismatch in x-values: correctly flagged (§2.9 step 6, §8.10). Confirmed `@x_values = (1..$num_samples)` uses the total.

---

## 2. Assumptions

- **`include_str!` paths** `../../../plotly/{…}` (§3): assumes the crate lives at `rust/bismark-summary/` and `plotly/` is at the repo root. Confirmed `plotly/` exists at the repo root with `plot.ly`, `bismark.logo`, `bioinf.logo`. Reasonable; the build-time path should be asserted by the heredoc-extraction test anyway.
- **Heredoc extraction (lines 490–1371) byte-verbatim** with a `perl`-guarded drift test (§3, §8.4): sound and the right safety net. One caution: the SPEC cites the heredoc as lines **490–1371**; the `HTMLTEMPLATESTRING` open is line 489 and the closing terminator is line 1372 — the *content* is 490–1371 inclusive (matches), but note the single-quoted `<<'HTMLTEMPLATESTRING'` means **no interpolation** (so literal `$`/`@`/`{{…}}` survive) — the SPEC says this; good.
- **`unless ($report_basename)` / `unless ($page_title)` truthiness** (§2.2): confirmed `-o 0` / `--title 0` fall back to defaults (`"0"` is falsy in Perl). The Rust port must special-case the literal string `"0"` → default, not just empty. Edge correctly flagged (§8.13); make sure the PLAN's clap handling actually reproduces it (clap will give you `Some("0")`, which `is_some()` would treat as present — you need an explicit `value == "" || value == "0"` check).
- **Exit-code divergence on `--help`** (§4.4): Perl `print_helpfile` ends `exit 1` (line 123); the SPEC deliberately uses clap's exit 0. Consistent with the bismark2report decision; acceptable since help text isn't gated. Fine.
- **`{{bismark_version}}` hardcoded `0.25.1`** (§4.7, O1): correct for byte-identity — it is a literal constant in Perl (`$bismark_version`, line 25), not input-derived. Hardcode it; don't use the crate version. (Contrast bismark2report, where `{{bismark_version}}` is *input-derived*; reviewers should not carry that pattern over.)

---

## 3. Efficiency analysis

Not a concern and the SPEC rightly doesn't dwell on it. Inputs are a handful of tiny text reports; the only "large" object is the ~3 MB `plot.ly` asset, embedded once via `include_str!` and spliced once. The greedy/dotall section deletions are O(n) string splices. A whole-file `String` for the HTML is trivially fine. No streaming, no parallelism, no memory pressure. The only performance-adjacent correctness note: implement section deletion as a single `find`/`rfind` splice (as §8.5 says), not a regex with backtracking, to avoid pathological `.*` behaviour on the 3 MB body — but correctness, not speed, is the reason.

---

## 4. Validation sufficiency

The §7 matrix is good (PE/SE × WGBS/RRBS × splitting present/absent × plot-excluded × multi-sample × argv-vs-glob × mix-die). **Gaps that could let a divergent build pass the gate:**

1. **(Critical) Mixed-case multi-sample glob fixture is missing.** Without one (e.g. `apple_…`, `Mango_…`, `zebra_…` auto-globbed), the row-order bug in §1.2 passes every proposed test. **Add it.**
2. **(Critical) Single-RAW-sample fixture is missing.** The §7 matrix has "1 RRBS-SE" but does not pin that it be the **only** sample in its dir. A directory with exactly ONE RRBS sample exercises the numbers/percentage asymmetry (§1.3); a directory with TWO RRBS samples does **not** (they agree). **Add a one-RRBS-sample dir AND a two-RRBS-sample dir** and diff both against Perl.
3. **(Important) Single-WGBS-sample fixture.** Confirms the consistent-DEDUP single-sample path and guards the regex semantics (§1.4).
4. **(Important) The all-commas mode-detection boundary.** A fixture proving the N=1 vs N≥2 flip for `$dup_alignments` / `$aligned`. (Covered if #2 and #3 are both present and both diffed.)
5. **(Important) Plot-excluded sample in the MIDDLE of the list**, so the `num_samples`-vs-plotted x-array length mismatch is exercised with a non-trivial offset (x has N points, y has N−1). Assert the literal `{{x_values_*}}` count vs the y-array length in the golden.
6. **(Optional) `-o 0` / `--title 0`** truthiness-default fixture.
7. **(Optional) Non-`.bam` argv entry** (loses last 4 chars via `substr`) — documents the edge; low stakes.

**Oracle handling is correct** (§7): the checked-in `docs/images/*` are stale and must NOT be used; the oracle is a fresh current-Perl run, auto-skipped if `perl` absent. The hidden `--__test_timestamp` (UTC ctime) + single-anchored-timestamp-line normalization is the right bridge (mirrors bismark2report). One nit: §7 item 2 anchors on `<p>Report generated on {{report_timestamp}}</p>` → confirm the post-fill scalar-`localtime` format is the **space-padded** ctime `"Www Mmm DD HH:MM:SS YYYY"` (mday width-2 space-padded, e.g. `"Mon Jun  1 …"` with two spaces) — the SPEC says so (§2.9); the `--__test_timestamp` formatter must emit exactly that, including the double-space for single-digit mday, or the Rust↔Rust golden self-check (not the Perl bridge) will drift.

---

## 5. Alternatives

- **Glob crate vs hand-rolled.** The `glob = "0.3"` crate sorts its results **bytewise** by default (it does not replicate `File::Glob`'s case-folding) — so it would hit the §1.2 bug too. Whichever route (glob crate or `read_dir`), the implementer must apply the **case-folding collation** explicitly after collecting matches. Document this in the PLAN so it isn't lost.
- **Couple to `bismark-report` for the shared parsers** instead of duplicating. The SPEC (O2, Felix's decision) duplicates for v1.0 and notes promotion later — fine and lower-risk given `bismark-report` isn't merged. No objection.
- **Regex vs manual splice for section deletion.** Manual first/last-marker splice is preferred (§8.5) and matches the bismark2report decision. Agree.

---

## 6. Action items (prioritised)

### Critical (must fix before PLAN/implementation; byte-identity-breaking)
- **C1 — Glob sort order.** §2.3/§4.8/§8.6 are **wrong**: Perl `glob` uses case-folding collation, **not** bytewise/`LC_ALL=C`. Replace the "plain `Vec::sort()`" guidance with a case-folded collation (verified approximation: sort by `(ascii_lowercased, original_bytes)`; confirm the case-only tiebreak on a case-sensitive FS). Add a **mixed-case multi-sample glob fixture** to §7 (e.g. `apple/Mango/zebra`) whose order differs under bytewise sort. Evidence: Perl returns `apple, Mango`; bytewise returns `Mango, apple` (invariant across all locales tested).
- **C2 — Single-raw-sample numbers/percentage asymmetry.** §2.9 step 8/9 mischaracterise the branches. The **numbers** deletion keys off `$dup_alignments =~ /^,{1,}$/` (1430); the **percentage** deletion keys off `if ($aligned)` (1577) — **different variables, not a "mirror"**. For ONE RRBS sample the numbers section takes the DEDUP layout while percentages take the RAW layout (confirmed end-to-end against current Perl). Restate §2.9 explicitly and add a **single-RRBS-sample fixture** (plus a two-RRBS-sample fixture to show the flip). Reproduce verbatim; do not normalise the inconsistency.

### Important (clarify in SPEC / add to fixture matrix)
- **I1 — `/^,{1,}$/` needs ≥1 comma**, so N=1 arrays never match (load-bearing for both the consistent WGBS-single and inconsistent RRBS-single cases). State it in §2.9; add single-WGBS and single-RRBS fixtures (§4 above).
- **I2 — Full-token replacement.** State that every placeholder replacement matches both `{{` and `}}` (no substring collisions exist, verified — but make the contract explicit).
- **I3 — `-o 0` / `--title 0` truthiness.** clap yields `Some("0")`; the Rust default-fallback must test `value.is_empty() || value == "0"`, not just `is_none()`. Spell this out in §2.2/§6 so the PLAN implements it.
- **I4 — `--__test_timestamp` ctime format** must emit space-padded mday (double space for single-digit days) to match scalar `localtime`. Confirm in §7.
- **I5 — Plot-excluded-in-the-middle fixture** to exercise the x(N)-vs-y(N−1) length mismatch with a non-zero offset.

### Optional (document; low stakes)
- **O-a — `substr($bam,0,-4)` on <4-char input → `""`** (Perl returns empty string). Note alongside the non-`.bam` edge in §2.4.
- **O-b — Non-`.bam` argv entry** loses its last 4 chars; add a documenting note/fixture.
- **O-c — Heredoc line range** is content 490–1371 (open at 489, terminator at 1372); fine as written, just confirm the extraction test's slice boundaries.

---

## 7. Summary

The SPEC's hardest claim — the `%.15g` unmeth percentage engine — is **empirically exact** (10,001/10,001 values, including float-noise edges), and the stale-oracle, parser-precedence, raw-vs-mutated-`.txt`, marker-count, and normalizer claims all check out against the source and live Perl runs. Two Critical issues remain: the **glob sort order** is specified as bytewise when Perl uses **case-folding collation** (provably divergent on mixed-case names — a hard `.txt`/`.html` row-order failure), and the **single-raw-sample numbers/percentage section asymmetry** is missed and actively mischaracterised by the "mirror"/"RRBS mode" framing (the two deletions key off different variables and disagree for N=1 RRBS — confirmed end-to-end). Both need a SPEC restatement and a dedicated fixture before this proceeds. Everything else is Important-or-below polish.
