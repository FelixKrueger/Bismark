# SPEC Review A — `bismark-report` (Rust port of Perl `bismark2report`)

**Reviewer:** Plan Reviewer A (fresh context)
**Date:** 2026-06-01
**Target:** `plans/06012026_bismark2report/SPEC.md` (DRAFT rev 0)
**Ground truth consulted:** `bismark2report` (1316 lines), `plotly/plotly_template.tpl` (1155 lines), `plotly/{plot.ly,bismark.logo,bioinf.logo,bismark_bt2_PE_report.html}`, sibling `plans/05302026_bismark-genome-preparation/SPEC.md`, `rust/Cargo.toml`.

**Overall verdict:** The SPEC is unusually strong — it captures the load-bearing mechanics (asset-injection order, greedy/dotall section deletion, the line-1256 `undef`-reset, the M-bias SE/PE asymmetry, the nucleotide fixed key order, the commented-out log2 ratio, the stale reference HTML) and I verified each of these against source. The byte-identity framing is correct and the proposed normalizer is **faithful** (empirically confirmed below). There are, however, **two correctness errors** (the fill-gate predicate, and a crate-name claim) and a handful of under-specified edge cases that should be nailed down before the PLAN. None is fatal; all are cheap to fix in rev 1.

---

## 1. Logic review

### 1.1 Fill-gate predicate is wrong: `defined`, not truthiness (CRITICAL)
SPEC §2.7a line 98 and §8 gotcha #6 state the alignment fill block runs "**only if** `$unique && $no_aln && $multiple && $no_genomic && $total_seqs` are all defined." Likewise §2.7b line 103 (`$dups && $total_seqs && $diff_pos && $leftover`) and §2.7c line 108 (`$meth_CpG && …`).

The Perl gates use **`defined`**, not boolean truthiness:
- `bismark2report:378` — `if (defined $unique and defined $no_aln and defined $multiple and defined $no_genomic and defined $total_seqs)`
- `bismark2report:551` — `if (defined $dups and defined $total_seqs and defined $diff_pos and defined $leftover)`
- `bismark2report:784` — `if (defined $meth_CpG and … and defined $unmeth_CHH)`

This is **not pedantry** — it changes output for a real, common input. A value of `0` is *defined* but *falsy* in Perl. Examples that occur in practice:
- `Sequence pairs which were discarded because genomic sequence could not be extracted: 0` → `$no_genomic = 0`.
- A sample with `Total number duplicated ... 0` → `$dups = 0`; or zero `different positions`.
- Zero methylated C's in some context.

Under `defined`, these pass the gate and the report is fully filled. Under the SPEC's `&&` wording, a Rust implementation would treat `0` as "missing", fail the gate, and emit a report with **literal `{{…}}` placeholders surviving** — a byte-divergence that the PE/SE happy-path fixtures (which rarely have a genuine `0`) might not catch. The SPEC prose even self-contradicts ("`$x && …` are all **defined**"). **Fix:** restate all three gates as "every field was *captured* (a value was assigned, including `0` / empty string)", and the Rust model should be `Option::is_some()`, never truthiness. Add a fixture with `no_genomic: 0` (and ideally a `dups: 0` dedup fixture) to lock this.

Note a Rust subtlety: Perl `(undef,$val) = split /\t/` on a line with no tab yields `undef` (verified) — i.e. "not captured". But a line `Key:\t` (trailing tab, empty value) yields the **empty string**, which *is* defined → passes the gate and injects `''`. The Rust capture type must distinguish "line absent" (`None`) from "line present, empty value after tab" (`Some("")`). Splitting and taking `.get(1)` and only assigning when the prefix matched reproduces this correctly; document it.

### 1.2 M-bias `$state` vs `%mbias_2` can diverge (IMPORTANT)
SPEC §2.7d/§2.3 step 9 says "if R2 data present, fills `{{mbias2_…}}`" and "`$state` drives step 9." But these are driven by **two different signals** in Perl:
- `$state = 'paired'` is set when a **header line** matches `/R2/` (`bismark2report:907-909`) — i.e. a `... context (R2)` section header exists.
- The R2 **fill** block runs only `if (%mbias_2)` (`:977`) — i.e. at least one R2 **data row** (`^\d`) was read while `$read_identity == 2`.

So for a pathological M-bias file that has an R2 *header* but **no R2 data rows**: `$state = 'paired'` → the main loop deletes only the R2 *markers* (keeps the R2 block, `:131`), but `%mbias_2` is empty → the `{{mbias2_*}}` placeholders are **never filled** → literal `{{mbias2_CpG_meth_x}}` etc. survive in the kept block. The SPEC's "if R2 data present, fills" wording conflates the two conditions and would lead an implementer to gate block-deletion on data presence. **Fix:** state explicitly that block-deletion is driven by the **header-derived `$state`**, and R2 *fill* is driven independently by **R2-data presence**; they are not the same predicate. (Real Bismark M-bias files always pair a header with rows, so this is a low-probability input — but it is a genuine contract, and the all-or-nothing philosophy elsewhere makes "placeholders survive" a deliberate behavior worth pinning. At minimum document it.)

Also: SPEC §2.7d says R1 fill has "No fill gate — missing arrays join to the empty string" — **correct** (`:940-974` are unconditional; verified). Good.

### 1.3 Section present/absent + injection mechanics — VERIFIED CORRECT
- Marker counts: every one of the 8 markers (`plotly_goes_here`, `bismark_logo_goes_here`, `bioinf_logo_goes_here`, `deduplication_section`, `cytosine_methylation_post_deduplication_section`, `nucleotide_coverage_section`, `mbias_r1_section`, `mbias_r2_section`) occurs **exactly twice** in the template (verified by `grep -c`). So §2.4's "first…last is unambiguous" holds and the splice approach is sound.
- The asset-injection order (§2.3 steps 2–4) matches `:66/:74/:79`; the `die` only guards plot.ly (`:70`); the logos' substitutions are not death-guarded (a no-op `if` without `else die`). SPEC §2.3 lists the order correctly and §2.4 describes the greedy/dotall first→last splice correctly.
- The greedy `{{m}}.*{{m}}/s` deletes first-marker-start through last-marker-end inclusive. SPEC §8 gotcha #3 and §2.4 describe this precisely. The "do not use a lazy match" warning is the right one to raise.
- **No `{{…}}` tokens exist in any of the three injected assets** (verified: `grep -c '{{' plot.ly bismark.logo bioinf.logo` = 0). This confirms SPEC §8 gotcha #13: a literal splice is safe; there is no risk that an asset re-introduces a placeholder that a later substitution would clobber. Good catch by the SPEC; now empirically backed.

### 1.4 The five parsers — regexes/branches VERIFIED
I checked every regex against source:
- PE/SE detection lines, the alternate "Total C to T conversions" phrasing for unmethylated (alignment `:298-313` accepts **both** phrasings via `or`; splitting `:745-760` accepts **only** "Total C to T conversions"), the `C methylated in Unknown context (CN or CHN):` (alignment `:330`) vs `C methylated in Unknown context:` (splitting `:777`) — SPEC §2.7a/§2.7c capture this difference correctly.
- Strand-origin PE vs SE patterns (`:339-373`) — SPEC §2.7a is correct; the SE patterns (`^CT/CT:` etc.) are distinct from PE (`^CT/GA/CT:`) and won't cross-match (anchored). §8 gotcha #10 is right.
- Dedup `s/\s.*//` trim — verified `12345 (6.78%)` → `12345` and a bare number is unchanged. Leftover regex `(\d+)` requires a digit (`:538`). SPEC §2.7b correct.
- Nucleotide header validation (`:587-600`): col 3 must equal `percent sample`, col 5 `percent genomic`. SPEC §2.7e says "col 3 … and col 5"; the **column indices** are right but note Perl validates `$observed` (the 3rd field, index 2) and `$expected` (5th field, index 4) — using 1-based "col 3/col 5". An implementer must not off-by-one this (0-based index 2 and 4). Minor; flag in PLAN.
- Nucleotide fixed key order `A,T,C,G,AC,…,AA` (`:632`) verified verbatim; the `sort` line above it is commented out (`:631`). SPEC §2.7e/§8 gotcha #9 correct.
- The log2 ratio: `$ratio` IS computed (`:649-655`) but the `$logratio = sprintf("%.2f",…)` and its injection are commented out (`:657-660`, `:692`). So **no float ever reaches output** — SPEC §2.7e/§4.7/§8 gotcha #7 correct. (The bare `$ratio` is never substituted into `$doc`; it's only used in a `warn` under `--verbose`.) Confirmed: the only `sprintf` that touches `$doc` is the timestamp.

### 1.5 Line-1256 `undef`-reset — VERIFIED, but note the nuc asymmetry (IMPORTANT)
SPEC §2.2/§8 gotcha #11 describe the line-1256 reset correctly: an explicit `--dedup_report X` applies to the first alignment report, then `$dedup_report = … = undef` (`:1256`) makes subsequent reports fall back to auto-detect. Verified.

**Undocumented asymmetry:** in `process_commandline`, the **nucleotide** branch uses `if (defined $nucleotide_coverage_report)` (`:1171`) while dedup/splitting/mbias use truthiness `if ($x)` (`:1142/:1202/:1229`). This only matters for an *empty-string* explicit value (`--nucleotide_report ''` vs `--dedup_report ''`): `defined('')` is true → nuc takes the user-specified branch and pushes `''` (skip); an empty `--dedup_report ''` is falsy → dedup takes the **auto-detect** branch. Obscure (who passes an empty string?), and clap may reject empty values anyway, but it is a real PE/SE-independent behavioral fork the SPEC doesn't mention. Either replicate or explicitly declare a deliberate divergence ("empty companion flag values are not supported"). Low priority.

### 1.6 `read_report_template` normalizer — VERIFIED FAITHFUL (the central byte risk)
This is the highest-risk byte mechanic, and I empirically validated the SPEC's proposed normalizer against Perl:
- **Assets do NOT end in a trailing newline.** `plot.ly`, `bismark.logo`, `bioinf.logo` all end in `>` (0x3e), `wc -l` = 0 for the logos and 9 for plot.ly. Only `plotly_template.tpl` ends in `\n`. Perl's `$doc .= $_."\n"` *unconditionally* appends `\n` per line, so `$doc` always ends in `\n` regardless. SPEC §2.6 states this correctly and it is **load-bearing** (a naive Rust `include_str!` + concat would drop the final newline on the logos/plotly and diverge on every report).
- **The proposed normalizer is faithful.** I ran Perl's `chomp; s/\r//g; $doc.=$_."\n"` vs the SPEC's "split on `\n`, drop trailing empty, replace `\r` per piece, rejoin `\n`, append final `\n`" on the real assets and on synthetic edge files (ends-in-`\n`, no-trailing-`\n`, internal blank line, single/double trailing blank lines). **All byte-identical.** So §8 gotcha #2's normalizer recipe is correct. One caveat to encode in the PLAN: the "drop the trailing empty element **if the content ended in `\n`**" must be implemented as *"drop exactly one trailing empty element produced by a final `\n`"* (i.e. mimic `lines()`-style splitting), NOT "drop all trailing empties" — my `t4.txt` (two trailing newlines → `a\n\n\n`) case confirms only the final empty is dropped and both blank lines are preserved. The Python prototype I used drops only `parts[-1]==""` once, which matches; make sure the Rust does the same and doesn't loop.
- **There are no `\r` bytes in any current asset** (CR-count = 0 everywhere). So the `s/\r//g` mid-line-CR stripping is presently a no-op on the shipped assets — but the SPEC is right to replicate it for robustness/faithfulness, and right that `str::lines()` would be wrong (only strips a trailing `\r`). Cheap to do correctly.

### 1.7 Output-filename derivation — VERIFIED
SPEC §2.5 matches `:43-55` exactly: strip dir, strip `.txt`, append `.html`, then `-o` overrides verbatim (only when `if ($manual_output_file)` is truthy — empty `-o` falls back to derived), then prefix `$output_dir`. `--dir` trailing-slash logic (`:1093-1099`) is captured in §2.2. Correct.

---

## 2. Assumptions

- **"Zero numeric reformatting" (§1, §2.7, §7, §8#7): VALIDATED.** The only `sprintf` reaching `$doc` is the timestamp; all captured values pass through as strings; the log2 float is commented out. So "byte-identity collapses to the timestamp line" is a sound claim. This is the single most reassuring property of the port and it holds up.
- **`include_str!` embedding (§3, §4.1): reasonable, but state the build-time path source.** The assets currently live at `plotly/` (repo root), not under `rust/bismark-report/`. The SPEC should specify the `include_str!` path (e.g. `include_str!("../../../plotly/plot.ly")` relative to the crate, or a copy vendored into the crate). A vendored copy risks drift from the canonical `plotly/`; a relative include couples the crate build to the repo layout. **Recommend:** relative `include_str!` into the canonical `plotly/` files (single source of truth) + a unit test asserting the embedded bytes equal a fresh read of those files, so any future template/asset edit that isn't re-embedded fails CI. Call this out in the PLAN.
- **Timestamp determinism (§4.5/§7): the decision is left open ("confirm the choice in review").** This needs a decision before the PLAN. See §4 below — I recommend a hidden test-only flag over `SOURCE_DATE_EPOCH`.
- **Exit codes (§6.1) — under-specified; I pinned them.** Verified from source: `--help`/`--man` → `print_helpfile()` → `exit 1` (`:1314`, so exit **1**). `--version` → bare `exit` → exit **0** (`:1089`). No-report path → `warn` + `print_helpfile()` → `exit 1` (`:1117-1120`). `die` paths (>1 companion match; >1 report with `-o`; bad nuc header; unreadable file) → exit **non-zero (255 in Perl)**. The SPEC §6.1 hedges ("confirm exit code in review — Perl `print_helpfile` exits 1"). **Decision needed:** clap conventionally exits **2** for usage errors and **0** for `--help`. Matching Perl's `--help`→1 and the no-report→1 exactly requires overriding clap. Since help/version text is explicitly **not gated** (§4.3), I'd accept clap's native codes and document the divergence — but the SPEC should *state the chosen codes*, not defer. Pin them in rev 1.
- **`-o` "single alignment report only" (§2.2, §6.1): VERIFIED.** `:1128-1131` — `die` only when `scalar @alignment_reports > 1 && defined $manual_output_file`. Note: this is checked against the count *after* glob/explicit resolution; an explicit single `--alignment_report` + `-o` is fine. Correct.
- **`bismark2summary` out of scope (§9): correct** — it is a separate multi-sample tool. No issue.

---

## 3. Efficiency analysis

Not a hotspot tool; the SPEC rightly de-prioritizes performance. Two minor notes:
- **3 MB binary inflation from `include_str!` plot.ly:** acceptable, as the SPEC says. The whole-program string lives in `.rodata`; one `String` copy per report during normalization (~3 MB) is trivial against a tool that writes a 3 MB file per sample anyway. No concern.
- **Slurping reports:** the input reports (alignment/dedup/splitting/mbias/nuc) are tiny (KBs); line-by-line streaming or full-slurp are both fine. The M-bias and nucleotide parsers are O(lines) with small constant per-line work. No concern.
- One micro-point: doing `replace(marker, "")` (all-occurrences) for the "present" case and a first..last splice for "absent" is O(n) each over a 3 MB doc, repeated ~25+ times across all `{{…}}` substitutions. That's ~75 MB of scanning per report — still microseconds-to-low-ms in Rust; not worth optimizing, but if an implementer is tempted to build a single-pass templater, **warn them off**: Perl's per-substitution `s///g` semantics (each pass sees the result of the previous) are what guarantees byte-identity, and a one-pass replace could subtly differ if any injected value contained a `{{…}}` token (it doesn't today, but the per-pass model is the safe contract). Keep the naive sequential-substitution approach.

---

## 4. Validation sufficiency

**The acceptance gate (§7) is well-conceived** (Perl-as-oracle from Phase A, timestamp-line normalization in *both* outputs, committed deterministic goldens for edges, real-data on oxy `#[ignore]`). The stale-reference warning (§7/§8#1) is correct and important — I verified `bismark_bt2_PE_report.html` reports `version v0.19.1` and `Data processed at 11:25 on 2018-08-16` (HH:MM, no seconds), whereas current `getLoggingTime` emits HH:MM:SS. **It must not be used as the oracle.** Good.

**Is timestamp-line normalization in BOTH outputs sound?** Yes, with one guard. The risk is masking a *real* divergence that happens to land on the timestamp line. Mitigation that the SPEC should adopt:
- Normalize by replacing **only** the `HH:MM:SS` and `YYYY-MM-DD` substrings with fixed tokens, anchored to the exact surrounding literal `Data processed at … on …</p>` (template `:146`), **not** by deleting/blanking the whole line. That way the fixed prefix/suffix bytes (`<p>Data processed at `, ` on `, `</p>`) are still compared. The SPEC §7.1 says "replace the HH:MM:SS and YYYY-MM-DD with fixed tokens in both files" — good, but make the regex anchored and assert it matched **exactly once** in each file (fail loudly if 0 or >1 matches, which would itself signal a divergence). Add this assertion to the harness.
- **Better still for committed goldens:** use the deterministic-timestamp hook so the golden has a *real* fixed timestamp and is compared with **zero** normalization. Reserve line-normalization only for the live Perl-vs-Rust diff (where Perl's clock can't be pinned). The SPEC proposes this (§4.5/§7) — endorse it and make it the primary edge-coverage mechanism.

**Timestamp-hook choice:** I'd pick a **hidden test-only flag** (e.g. `--_test_timestamp <epoch-or-string>`) over `SOURCE_DATE_EPOCH`. Reasons: (a) it's explicit and local to the test, no ambient-env surprises; (b) `SOURCE_DATE_EPOCH` is a *build* convention, semantically odd for a *runtime* timestamp; (c) the flag can inject the exact preformatted strings, sidestepping any TZ/`localtime`-vs-`gmtime` mismatch (Perl uses `localtime`, so a Rust golden built from an epoch must use the *same* TZ — a flag that takes the already-formatted `HH:MM:SS`/`YYYY-MM-DD` avoids that entirely). Decide in rev 1.

**Fixture coverage (§7.2) — gaps to close:**
1. **`no_genomic: 0` (and a zero-valued dedup `dups: 0`)** — the linchpin for the `defined`-vs-truthy bug (§1.1). Currently §7.2 lists PE/SE × optional-present/absent × Unknown × M-bias SE/PE × `--dir`/`-o` × multi-report × `none`, but **nothing forces a `0` value through a gate field.** Add it. **(Critical fixture gap.)**
2. **Fill-gate FAILURE fixtures** — §5.4 says unfilled placeholders are a contractual output state, but §7.2 has no fixture that *deliberately* fails a gate (e.g. an alignment report missing `Number of … unique best hit:`). Without one, the "placeholders survive" path is untested and an implementer could "helpfully" default it. Add one truncated/partial alignment report and one truncated dedup report. **(Important.)**
3. **Dedup leftover-fallback path** — a dedup report *without* `Total count of deduplicated leftover sequences:` but *with* `Total number of alignments` and `Total number duplicated`, exercising `$leftover = $total - $dups` (`:544-548`). Add a fixture; verify the integer-string output. **(Important.)**
4. **Unknown-context with partial fields** — a Bowtie2 alignment report where `meth_unknown` is defined but `unmeth_unknown`/`perc_unknown` are absent → the inject still fires (gated only on `$meth_unknown`, `:432`) and injects `''`/`N/A%` for the missing ones. Edge but cheap. **(Optional.)**
5. **Nucleotide missing-key (amplicon, issue #711)** — a nuc report missing some di-nucleotide keys → percentages default to `0`, counts/coverage become empty string. §8#9 flags it; §7.2 doesn't fixture it. Add a minimal amplicon-style nuc report. **(Optional but recommended — it's a documented real-world case.)**
6. **Bad nuc header → `die`** — a nuc report with a wrong line-1 header, asserting the error/exit. **(Optional.)**
7. **Multi-report companion `undef`-reset** — a directory with two `*E_report.txt` plus one explicit `--dedup_report X`, asserting report 1 uses X and report 2 auto-detects. Locks §8#11. **(Optional.)**

**Whitespace fixture:** §8#5's exact byte claims (header `<tr>`=5 spaces, `<th>`=32 spaces, `<td>`=4 spaces+4 tabs, `</tr>`=4 spaces+3 tabs) are **verified correct** against `:433-436`. A Bowtie2 (Unknown-present) fixture covers it; ensure the golden diff is byte-level (not whitespace-insensitive).

---

## 5. Alternatives & trade-offs

- **Crate name (§3): the SPEC's own footnote is factually wrong.** §3 says "*The sibling convention is the **full** Perl name as the crate name → `bismark2report` would be the strict analog.*" That is **not** the convention. All seven existing workspace crates are hyphenated `bismark-<tool>` (`bismark-dedup`, `bismark-extractor`, `bismark-bedgraph`, `bismark-coverage2cytosine`, `bismark-genome-preparation`, `bismark-methylation-consistency`, `bismark-io`) — none uses the full Perl name (`deduplicate_bismark`, `coverage2cytosine`, etc.). So `bismark-report` is **already consistent** with house style; there is no tension to "confirm." The *binary* name `bismark2report_rs` (Perl-name + `_rs`) matches the binary convention (`deduplicate_bismark_rs`, `bismark_genome_preparation_rs`). **Fix the footnote** so reviewers/the planner aren't misled into renaming the crate to `bismark2report`. (Verdict: keep `bismark-report` crate + `bismark2report_rs` binary.)
- **Asset shipping (embed vs read-from-`$RealBin`):** embedding (§4.1) is the right call for a drop-in single binary and matches the rewrite ethos; the only cost is the embedded-vs-canonical drift risk, mitigated by the byte-equality unit test recommended in §2 above.
- **Templating engine:** the SPEC implicitly uses raw string substitution (correct). Do **not** be tempted to introduce a real template crate (handlebars/tera) — `{{…}}` here is bespoke (markers appear twice, deletion semantics are greedy/dotall, values are injected literally with no escaping). A real engine would HTML-escape values and break byte-identity. The SPEC should add a one-line "no template crate — raw substitution only" guard rail (it's implied but worth stating, since the `{{}}` syntax superficially looks like handlebars).
- **`{{bm_mbias_2}}` dead substitution (§2.7d/§4.7):** `:1016` targets a placeholder absent from the current template → genuine no-op (verified: `grep` finds no `{{bm_mbias_2}}` in the template). The SPEC's "reproduce as a no-op (or omit; document either way)" is fine. **Recommend: omit it and document** — reproducing a no-op adds code for zero behavioral effect; a one-line comment "Perl `:1016` substitution is dead against the current template; omitted" is cleaner and equally faithful.

---

## 6. Action items (prioritized)

### Critical (correctness — fix before PLAN)
1. **Fix the fill-gate predicate everywhere** (§2.7a line 98, §2.7b line 103, §2.7c line 108, §8#6): gates are `defined`, **not** truthiness. Restate as "field was captured (incl. `0`/empty)"; model as `Option::is_some()`. A `0` value must pass. (Source: `bismark2report:378,551,784`.)
2. **Add a fixture that forces a `0` through a gate field** (e.g. `no_genomic: 0`, and a `dups: 0` dedup report) — this is the input that distinguishes `defined` from `&&` and is currently uncovered by §7.2.
3. **Correct the crate-name footnote in §3:** the house convention is hyphenated `bismark-<tool>` (all 7 existing crates), *not* the full Perl name. `bismark-report` is correct; no change needed beyond fixing the misleading note.

### Important (precision / coverage — resolve in rev 1)
4. **Disambiguate the M-bias R2 contract** (§2.7d/§2.3 step 9): block-deletion is driven by the **header-derived `$state`** (`:907-909`); R2 *fill* is driven independently by **R2-data presence** (`if (%mbias_2)`, `:977`). They can diverge (R2 header, no R2 rows → kept block with surviving `{{mbias2_*}}` placeholders). State both predicates separately.
5. **Pin the exit codes** (§6.1) instead of deferring: source has `--help`/no-report → exit 1, `--version` → exit 0, `die` → non-zero. Decide whether to match Perl's codes exactly or accept clap's native (2 for usage / 0 for help) — help/version text isn't gated, so I'd accept clap's and document the divergence — but *write the chosen codes down*.
6. **Decide the timestamp hook** (§4.5/§7): recommend a hidden `--_test_timestamp` flag (takes preformatted `HH:MM:SS`/`YYYY-MM-DD`) over `SOURCE_DATE_EPOCH`, to dodge TZ/`localtime` mismatch. Use it for zero-normalization committed goldens; reserve line-normalization for the live Perl diff.
7. **Anchor + assert the timestamp normalization** (§7.1): replace only the time/date substrings within the literal `Data processed at … on …</p>`, and assert exactly-one match per file (0 or >1 = failure). Keep the surrounding bytes in the comparison.
8. **Add fill-gate-FAILURE and dedup-leftover-fallback fixtures** (§7.2): one partial alignment report (placeholders must survive), one dedup report lacking the explicit leftover line (exercises `total - dups`).
9. **Specify the `include_str!` source path** and add a unit test asserting embedded bytes == a fresh read of the canonical `plotly/` files (guards against template/asset drift).

### Optional (robustness / nice-to-have)
10. Document (or deliberately diverge on) the **nuc-vs-dedup `defined`-vs-truthy asymmetry** in `process_commandline` for empty-string companion flags (`:1171` vs `:1142/:1202/:1229`).
11. Add the **amplicon/missing-nucleotide-key** fixture (issue #711): missing keys → `0` for percentages, empty string for counts/coverage (§8#9).
12. **Omit (don't reproduce) the dead `{{bm_mbias_2}}` substitution** and leave a one-line comment (§4.7).
13. Add a one-line guard rail: **raw substitution only, no template crate** (no HTML-escaping) — the `{{}}` syntax superficially resembles handlebars.
14. Note the nuc header-validation **column indexing** (1-based "col 3/col 5" = 0-based fields 2/4) so an implementer doesn't off-by-one (§2.7e).
15. Multi-report `undef`-reset fixture (§8#11) and the `total_C_count`-absent edge (filled with `''` inside the gate, `:409/:788`) if cheap.

---

## 7. Verified-correct (no action needed)
- Asset-injection order, the `die`-only-on-plot.ly guard, greedy/dotall first→last section deletion, all 8 markers occur exactly twice. ✔
- The proposed `read_report_template` normalizer is **byte-faithful** to Perl across the real assets and trailing-newline/blank-line edge cases. ✔ (assets have no trailing newline; no `\r` bytes present.)
- All five parsers' regexes/branches, the PE/SE label+strand split, the alternate "Total C to T conversions" phrasing, the splitting-only Unknown-context phrasing, the dedup `\s.*` trim + `(\d+)` leftover, the nucleotide fixed key order, the commented-out log2 ratio (no float ever reaches output), the Unknown-context inject whitespace bytes (5/32/4+4t/4+3t), the line-1256 `undef`-reset, `-o` single-report `die`, output-filename derivation, `--dir` trailing-slash. ✔
- The stale-reference HTML is genuinely stale (v0.19.1 / HH:MM / 2018) and correctly excluded as oracle. ✔
- "Zero numeric reformatting → byte-identity collapses to the timestamp line" is a sound, verified claim. ✔
- No `{{…}}` tokens in any embedded asset → literal splice is safe. ✔
