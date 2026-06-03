# PLAN Review A — `bismark-report` (Rust port of Perl `bismark2report`)

**Reviewer:** Plan Reviewer A (fresh context)
**Target:** `plans/06012026_bismark2report/PLAN.md` (DRAFT rev 0, 2026-06-01)
**Contract:** `SPEC.md` rev 1 (+ folded `SPEC_REVIEW_A.md` / `SPEC_REVIEW_B.md`)
**Ground truth consulted:** Perl `bismark2report` (1316 lines), `plotly/{plotly_template.tpl, plot.ly, bismark.logo, bioinf.logo}`, sibling crate `rust/bismark-genome-preparation/` (Cargo.toml, `cli.rs`, `main.rs`), `rust/Cargo.toml`, `rust/bismark-extractor/src/logging.rs`.

---

## Verdict (summary)

The PLAN is **structurally sound and faithful to the SPEC**. The A→F phasing is buildable, the module decomposition is coherent, the pure-parse/fill split genuinely lets Phase B unit-test before Phase C exists, and every *named* SPEC requirement has a step. I empirically re-verified the three highest-risk byte claims and they hold: (a) the 24 M-bias data placeholders (`{{mbias1_*}}` lines 843–907, `{{mbias2_*}}` lines 997–1060) all sit **outside** the deletable `{{mbias_r*_section}}` spans (465–496); (b) all 8 section markers + 3 asset markers each occur **exactly 2×**; (c) none of the 4 assets contains `{{` and none currently contains `\r`, so literal splice + value-substitution are byte-safe.

However there are **three Critical coverage gaps** where a load-bearing byte requirement has *no concrete byte-spec in the PLAN* (the implementer would have to re-derive it from Perl), plus several Important ordering/precision items. None invalidate the architecture; all are cheap to fold into PLAN rev 1.

---

## 1. Logic review

### 1.1 The `*_text` human-label strings are byte-load-bearing but the PLAN never lifts the bytes — CRITICAL
PLAN **B1** says only: "PE/SE detection by exact line text (sets both the value and the `*_text` label)." It never enumerates the **eight literal label strings** that get substituted into `{{unique_seqs_text}}`, `{{no_alignments_text}}`, `{{multiple_alignments_text}}`, `{{sequences_analysed_in_total}}`. These differ between PE and SE and are emitted **verbatim into the HTML** — they are inside the byte-identity gate. From Perl:
- total: `'Sequence pairs analysed in total'` (PE, 218) / `'Sequences analysed in total'` (SE, 222)
- unique: `'Paired-end alignments with a unique best hit'` (236) / `'Single-end alignments with a unique best hit'` (241)
- no-aln: `'Pairs without alignments under any condition'` (247) / `'Sequences without alignments under any condition'` (252)
- multiple: `'Pairs that did not map uniquely'` (258) / `'Sequences that did not map uniquely'` (263)

SPEC Review B raised this against the SPEC (B §1.2) and recommended the PLAN carry the bytes; the PLAN did **not**. An implementer paraphrasing these (e.g. "Paired-end alignments with a unique best hit." with a stray period, or dropping "any") silently breaks the gate, and only the Phase-E PE-vs-SE diff would catch it — late. **Action: PLAN B1 must lift all 8 label strings verbatim (cite Perl 218/222/236/241/247/252/258/263), and a Phase-B unit test must assert each.**

### 1.2 Companion-discovery order is wrong in PLAN D1, and the `defined`-vs-truthiness asymmetry is omitted — IMPORTANT
PLAN **D1** lists companion resolution as "`deduplication_report.txt` / `splitting_report.txt` / `M-bias.txt` / `nucleotide_stats.txt`". The Perl `process_commandline` loop resolves them in a **different order**: dedup (1142) → **nucleotide (1171)** → splitting (1202) → mbias (1229). This matters for two reasons: (i) STDERR diagnostic ordering (not gated, low-stakes) and (ii) **`die`-precedence** when more than one companion has >1 match — Perl dies on the *first* over-matched companion in that order. Reproduce the Perl order so the first error surfaced is the same.

More consequentially, the four companions are **not** gated uniformly:
- dedup/splitting/mbias use `if ($X_report)` — **truthiness** (1142/1202/1229)
- nucleotide uses `if (defined $nucleotide_coverage_report)` — **defined-ness** (1171)

Consequence (SPEC Review B §2.2, never folded into the SPEC and absent from the PLAN): `--dedup_report ''` or `--dedup_report 0` is *falsy* → falls through to **auto-detect**, whereas `--nucleotide_report ''` is *defined* → pushes `''` (treated as absent downstream). These are pathological inputs, but the PLAN's D1 presents all four companions uniformly and clap won't reproduce Perl truthiness for free. **Action: PLAN D1 should (a) fix the resolution order to dedup→nuc→splitting→mbias, and (b) state the `defined`-vs-truthiness asymmetry as either replicated or an accepted, documented divergence.** I recommend documenting it as an accepted divergence (low-stakes) rather than replicating Perl truthiness — but the PLAN must make the call, not leave it silent.

### 1.3 `--man` / `--version` wiring contradicts the proven sibling it claims to mirror — IMPORTANT
PLAN **A2** says: "`--man` as a visible alias of `--help`. clap handles `--help`/`--version` → exit 0." But the sibling the PLAN explicitly mirrors (`bismark-genome-preparation`) does **not** use clap's auto `--version` or a clap alias for `--man`. It declares `disable_version_flag = true` and carries **separate `bool` fields** `man` and `version` (`cli.rs:96-102`), handled manually in `main.rs` (prints the Bismark provenance banner for `--version`, `print_long_help()` for `--man`, both returning `ExitCode::SUCCESS`). The PLAN's "clap auto-version" path would print clap's plain `bismark-report 1.0.0` rather than the Bismark banner, diverging from every sibling. Also, making `--man` a clap *alias* of the auto-generated `--help` flag is not directly supported by clap derive — the genomeprep `man: bool` + manual dispatch is the proven mechanism. **Action: align A2/A3 with the genomeprep precedent — `disable_version_flag = true`, `man: bool` + `version: bool` fields, manual dispatch in `main.rs`. The exit-0 outcome is unchanged; only the mechanism (and the banner text) needs correcting.** (Note SPEC §6.1 and §4.3 already accept that help/version *text* is not gated, so the banner divergence from Perl is fine — but the divergence from the *sibling pattern* should be intentional, not accidental.)

### 1.4 Splitting Unknown-context snippets are byte-identical to alignment's — PLAN B3 should say so explicitly — IMPORTANT
PLAN **B3** says splitting has "its own Unknown-context snippets." Verified: the splitting snippets (Perl 817–828) are **byte-for-byte identical** to the alignment snippets (433–444) — same `<th>Methylated C's in Unknown context</th>` / `<th>Unmethylated C's in Unknown context</th>` text, same 5sp/32sp/4sp+4tab/4sp+3tab indentation. The PLAN should make the shared-helper opportunity explicit (one `unknown_context_inject()` helper in `reports/mod.rs` producing the three snippets, reused by alignment + splitting) and assert byte-equality in tests — otherwise two independent transcriptions risk diverging (the dual-driver back-port trap). The only difference between the two parsers is the *target placeholder names* (`{{*_unknown}}` vs `{{*_unknown_splitting}}`) and the percent token (alignment's `$perc_unknown%` line 443 vs splitting's `$perc_unknown%` line 827 — same). **Action: PLAN B3 should note the snippets are identical to B1's and reuse one helper; add a byte-equality unit test.**

### 1.5 11-step orchestration order — VERIFIED CORRECT
PLAN §3 / **C4** reproduces Perl's mutation order exactly (template read → plotly inject → bismark.logo → bioinf.logo → timestamp → alignment → dedup → splitting → mbias → nucleotide → write). Cross-checked against Perl 59–156. The M-bias sub-ordering (collapse R1 markers + fill, *then* SE-excise-R2 / PE-collapse-R2 driven by `$state`) matches 119–138. Good. One nuance the PLAN gets right and is worth keeping explicit: the R1 markers are collapsed and R1 fill happens **before** the R2 `$state` decision (Perl 119–120 then 125–132), so the fill function runs inside `read_mbias_report` returning `(state, doc)` and the R2 section deletion happens *after* in `build_report` — PLAN C3/C4 thread this correctly.

### 1.6 Fill gates (`is_some`, incl. `0`-passes) — VERIFIED, well-covered
PLAN B1/B2/B3/B4/B5 correctly restate every gate as `is_some()` not truthiness, and B6 + E1(4) explicitly regression-test a `0`-through-gate (`no_genomic: 0`, `dups: 0`). Cross-checked: alignment gate Perl **378** (5 fields), dedup **551** (4 fields), splitting **784** (6 fields), nucleotide `looksOK` **617–622**. The dedup leftover-fallback (`total - dups`, signed) is Perl **544–548**; PLAN B2 + E1(5) cover it. Good — this was SPEC Review A's Critical item and it landed cleanly.

### 1.7 Greedy/dotall splice + collapse/excise — VERIFIED CORRECT
PLAN C1/C2 implement inject (first-index … last-occurrence-end) and excise (same) and collapse (`replace(marker,"")` all). I confirmed every marker occurs exactly 2× in the template, so "first…last" is unambiguous. The `inject_asset` "die if marker absent" matches Perl 69–71 (only plot.ly dies; bismark.logo/bioinf.logo injects are *not* guarded by `die` in Perl — 74/79 have no `else{die}`). **Minor: PLAN C1 says "Error if `marker` not found (mirror Perl `die`)" for all three injects, but Perl only `die`s for plot.ly. The bismark/bioinf injects silently no-op if the marker is missing (the `if` has no `else`).** Since the markers are always present this never bites, but for faithful mirroring C1 should note plot.ly = hard error, logos = best-effort (or document the deliberate divergence of erroring on all three). Low-stakes; flag as Optional.

### 1.8 Nucleotide parser — VERIFIED, one omission in PLAN B5
PLAN B5 captures the fixed 20-key order, header validation (col3/col5), missing-key (`0`/empty), and the distinct plot separators. Cross-checked against Perl 571–702: the fixed order (632), header check (587–600), `0`-default for `$nuc_obs`/`$nuc_exp` when undef (641–646) but counts/coverage stay undef → empty string (635–637, 665–667), separators `','` for y (675) and ` , ` for x (676–677), y wrapped in quotes (679). All correct. **One omission:** PLAN B5 says "col 3 == `percent sample`, col 5 == `percent genomic`" — but the Perl header check validates **only** column index 2 (`$observed`) and index 4 (`$expected`); it does **not** validate col 0/1/3 or require a specific column count. A header line with extra/fewer columns but the right values at idx 2 & 4 passes. The PLAN's phrasing is fine, but the test (B6) should assert that the validation keys on exactly those two positions (a row that has `percent sample`/`percent genomic` shifted to other columns must FAIL; a row with the right values at idx 2/4 but junk elsewhere must PASS). Minor precision point.

### 1.9 Strand-origin `elsif` order — PLAN B1 covers it, keep the SPEC §8.10 reasoning
PLAN B1 says "strand origin (PE vs SE patterns)" and SPEC §8.10 (folded) correctly notes the real reason mutual-exclusivity holds is **`elsif` first-match-wins**, not pure anchoring (`^CT/GA:` *is* a textual prefix of `^CT/GA/CT:`; only branch order saves it). The whole alignment parser is one `if/elsif` chain (Perl 211–376), so the Rust parser must be a single ordered match, not independent prefix tests. PLAN B1 should add one sentence making this explicit (it's implied by "mirror the exact regexes/branches" but the strand case is the one place it actually matters). Important-to-state, cheap.

---

## 2. Assumptions

- **§8 "assets are `{{`-free and `\r`-free":** I empirically confirmed `{{` count = 0 and CR-byte count = 0 in all four assets. The PLAN's A7 test (assert no live `{{`, no `\r`) is the right guard and will catch a future asset refresh that breaks the invariant. **Sound.**
- **Normalizer empty-input guard (A5):** Correctly folded from SPEC Review B. The four shipped assets are non-empty so this is a unit-test guard, not a runtime path — PLAN states this accurately.
- **`include_str!` path strategy (A5, §10 open item):** Labeled "Open (non-blocking)." Agreed it's non-blocking *for behavior*, but it is **not purely cosmetic**: `include_str!("../../../plotly/…")` couples the crate's build to a path three levels up outside the crate root (fragile under `cargo publish` / workspace moves), whereas copying the assets into the crate (e.g. `bismark-report/assets/`) duplicates 3 MB in git. Genomeprep ships no embedded data asset, so there's no in-repo precedent to copy. Recommend the PLAN pre-decide **copy-into-crate** (matches the "self-contained binary" ethos and avoids the `../../../` fragility) rather than leaving it to the implementer — it's a 2-minute decision that affects the directory layout in §2.1. Bump from "open" to "decided" in rev 1.
- **`chrono` vs `time` (A6, §10 open item):** Genuinely non-blocking. Both format UTC + local fine. One caveat the PLAN should pin: the default (no `--__test_timestamp`) path uses **local time**, and must format with Perl's exact `%04d-%02d-%02d` / `%02d:%02d:%02d` (zero-padded, 4-digit year). With `chrono`, `format("%Y-%m-%d %H:%M:%S")` happens to match, but the PLAN should pin the format string and unit-test a known epoch in UTC against the expected bytes (A7 mentions this; make it explicit that the test asserts exact zero-padding, e.g. epoch 0 → `1970-01-01` / `00:00:00`).
- **`v0.25.1` only in banner, never in HTML (A3):** Verified — `{{bismark_version}}` comes from the input report's `Bismark report for: X (version: Y)` (Perl 226–228, 397), not the script constant (23). Sound.
- **Stale `plotly/bismark_bt2_PE_report.html` never used as oracle:** PLAN §8 + §11 + E2/F1 correctly use the live Perl as oracle. Sound — this was SPEC's #1 trap.

---

## 3. Efficiency analysis

Appropriate and correctly scoped. One report → one ~3 MB HTML, run interactively; full-read into `String` + sequential `str::replace` is fine. No streaming/parallelism/mimalloc needed (PLAN §6). One micro-note, not a problem: the orchestration does ~80+ `doc.replace(...)` passes over a 3 MB string (each `replace` is O(n) and allocates a fresh `String`), so the whole run is ~80 × 3 MB ≈ 240 MB of transient allocation — utterly negligible at this scale and **must not** be "optimized" into a single-pass templating engine (see §5). The PLAN's self-review (§11) already reaches this conclusion. Sound.

---

## 4. Validation sufficiency

The §9 validation list + Phase-E fixtures cover the highest-risk byte-divergences well. The Perl-oracle harness design is sound: SPEC Review B independently verified that timestamp-line normalization **cannot mask a real diff** because `{{date}}`/`{{time}}` occur on exactly one template line (146, which I re-confirmed: only one `{{date}}`/`{{time}}` occurrence). The PLAN E2 correctly anchors the normalization on the full literal `Data processed at HH:MM:SS on YYYY-MM-DD` and asserts **exactly one match per file** — this is the right guard against an over-greedy normalizer. Good.

**Gaps / additions:**

1. **No `*_text`-label assertion (ties to §1.1).** The Phase-E PE and SE goldens *will* contain the label strings, so a byte-diff would eventually catch a wrong label — but only at Phase E, and only if the crafted fixture actually exercises both PE and SE labels. Add an explicit Phase-B unit assertion on the 8 label strings so the failure surfaces at parser-test time, not end-to-end.

2. **Fill-gate-FAILURE fixtures are listed only for alignment (E1.3).** SPEC §5.4 makes "placeholders survive" contractual for **alignment, splitting, dedup, and nucleotide** gates. PLAN E1 fixture (3) covers alignment-missing-`no_genomic`. There is **no** fixture that trips the **splitting** gate (missing one of 6), the **dedup** gate (missing one of 4 with no fallback possible, e.g. missing `diff_pos`), or the **nucleotide** `looksOK` failure. The amplicon fixture (E1.6) exercises *missing-key* but **not** a `looksOK=0` failure (a key present in `obs` but not `exp`, which is the only way `looksOK` flips — Perl 619). Recommend adding at least a splitting-gate-failure fixture and a dedup-gate-failure fixture; the nucleotide `looksOK` path is hard to trigger naturally (a row with a value at idx2 but blank at idx4) and can be a unit test rather than a full golden. **Important.**

3. **Multi-report companion reset (E1.7) — good, but make the assertion precise.** The fixture exists; the assertion should be: report #1 uses the explicit `--dedup_report X` *and* report #2 (a) auto-detects its own dedup if one matches its basename, or (b) gets `''` if none. The PLAN says "report #2 falls back to auto-detection" — confirm the fixture's report #2 has a basename-matching dedup file present so the *auto-detect actually fires* (otherwise the test only proves #2 ≠ X, not that auto-detect works). **Important.**

4. **Nucleotide empty-string-vs-`0` (E1.6) — the single most divergence-prone byte path.** A naive Rust impl will insert `"0"` or leave `{{nuc_AC_counts_obs}}` literal where Perl inserts `""`. The PLAN/SPEC have this right (`0` for percentages, empty string for counts/coverage). Ensure the golden for #711 actually contains a **missing** di-nucleotide key (e.g. genome with no `AC`) so the empty-string substitution is exercised on a real placeholder, and the test greps the output for the resulting `<td></td>` (empty) vs `<td>0</td>`. **Important — pin the exact expected bytes in the fixture's golden.**

5. **`-o` verbatim naming (D2) lacks an assertion that no `.html` is appended.** PLAN D4 tests "`-o` with 2 reports → error" but should also assert that `-o foo` produces a file literally named `foo` (no `.html`), prefixed by `--dir`. SPEC §8.12. Cheap to add. **Optional.**

---

## 5. Alternatives

- **Single-pass templating engine (e.g. one regex sweep over `{{\w+}}`) instead of ~80 sequential `str::replace`:** **Reject.** It would diverge. Perl's behavior is sequential `s///g` calls in a *fixed order*, and crucially **values can re-introduce `{{…}}`-shaped text** in principle, and the *order* of substitution is observable (PLAN §11 calls this "cross-call re-substitution"). A single-pass engine that resolves all placeholders simultaneously from a map would not reproduce order-dependent edge cases, and would also change behavior for a surviving placeholder that a later substitution might touch. The verified-safe approach is exactly what the PLAN chose: literal sequential replace in Perl's order. The 240 MB transient allocation cost is irrelevant. **PLAN's choice is correct; the PLAN should keep §11's note that the order is fixed and explain *why* a map-based engine is rejected** (currently it asserts the order matters but doesn't close the door on the alternative — a future "optimizer" might).
- **`glob` crate vs `std::fs::read_dir` + suffix filter (D1):** Either works. `read_dir` + filter + explicit lexical (C-locale byte) sort is more transparent and avoids a dependency; the PLAN allows both. Recommend `read_dir` to keep the dependency footprint minimal (matches "no flate2/noodles" ethos) and to make the C-locale sort explicit rather than relying on the `glob` crate's ordering. **Optional.**

---

## 6. Phase sequencing / dependencies

A→F is buildable with no forward dependencies:
- **A** (scaffold/CLI/assets/timestamp) depends on nothing.
- **B** (pure parsers + fill) — the `fill` functions call `doc.replace(...)` on an owned `String` and do **not** require the assembled template to exist; they can be unit-tested against synthetic docs containing just the relevant `{{…}}` tokens. The PLAN's claim that B is testable before C holds. ✅ Verified: `fill` is pure string→string; no template-assembly dependency.
- **C** (assembly/sections/M-bias wiring) consumes A's assets + B's parsers — correct order.
- **D** (discovery/loop/naming) consumes C's `build_report` — correct.
- **E** (Perl-oracle gate + fixtures + goldens) consumes the whole binary — correct.
- **F** (oxy real-data + docs + PR) last — correct.

One sequencing nit: PLAN C6 compares "a full crafted-PE end-to-end fill … to a committed golden generated with `--__test_timestamp`." But the committed golden is itself produced by the Rust binary (E3) — so C6's golden is self-referential until E2 validates Rust-vs-Perl. That's fine (C6 is a regression-lock, E2 is the truth-gate), but the PLAN should state that the **C6 golden must be regenerated/validated against Perl in E2**, otherwise a bug baked into the C6 golden would pass C6 forever. **Make the C6→E2 dependency explicit.** Minor.

---

## 7. Module decomposition & signatures (§2.1, §5)

Coherent and sufficient. `reports/mod.rs` holds the shared helpers (tab-split keeping 2nd field, `%`-strip, whole-doc subst, and — per §1.4 — the Unknown-context inject helper). `mbias::fill` returning `(State, String)` and `build_report` threading `state` into the R2 section decision is the right shape (matches Perl's `(my $state, $doc) = read_mbias_report(...)`). Two refinements:
- The `Captured` structs are per-parser and not shown; that's fine for a PLAN, but B1's `Captured` must hold the **8 `*_text` labels** (§1.1) and the N/A-vs-graph distinction for percentages (table shows `N/A`, graph string uses `0` — Perl 414–425, 469–498) — confirm the `fill` (not `parse`) is where `N/A`→`0` graph mapping happens, matching Perl (the mapping is in the fill block, 467–498). The PLAN's B1 mentions "N/A→`0` in graph only" — good, just ensure the struct carries both forms or the `fill` derives the graph form.
- `excise`/`collapse`/`inject_asset` taking `String` by value and returning `String` is fine (sequential ownership). No issue.

---

## 8. Action items (prioritized)

### Critical (fix in PLAN rev 1 before implementation)
- **C1.** PLAN **B1**: lift the **8 `*_text` label strings verbatim** (PE/SE × total/unique/no-aln/multiple) from Perl 218/222/236/241/247/252/258/263 into the implementation contract, and add a Phase-B unit test asserting each. *(§1.1 — byte-load-bearing, currently absent from both SPEC and PLAN.)*
- **C2.** PLAN **D1**: fix companion-resolution **order** to dedup→**nucleotide**→splitting→mbias (Perl 1142/1171/1202/1229), and **state the `defined`-vs-truthiness asymmetry** (nuc uses `defined`, others truthiness) as either replicated or an accepted documented divergence. *(§1.2.)*
- **C3.** PLAN **A2/A3**: reconcile the `--man`/`--version` wiring with the proven `bismark-genome-preparation` precedent — `disable_version_flag = true`, separate `man: bool` + `version: bool` fields, manual dispatch in `main.rs` printing the Bismark provenance banner (not clap's auto-version, not a clap alias). *(§1.3 — current A2 description diverges from the sibling it claims to mirror.)*

### Important (precision / coverage — resolve in rev 1)
- **I1.** PLAN **B3**: state that the splitting Unknown-context snippets are **byte-identical** to alignment's (Perl 817–828 == 433–444); reuse one `unknown_context_inject()` helper; add a byte-equality test. *(§1.4.)*
- **I2.** PLAN **E1**: add fill-gate-FAILURE fixtures for **splitting** (missing 1 of 6) and **dedup** (missing `diff_pos`, no fallback), plus a unit test for nucleotide `looksOK=0`. Currently only alignment-gate-failure is covered. *(§4.2.)*
- **I3.** PLAN **E1.6**: pin the exact expected bytes for the amplicon missing-key golden — `<td></td>` (empty) for counts/coverage, `0` for percentages — and grep for them. *(§4.4.)*
- **I4.** PLAN **E1.7**: make report #2 in the multi-report fixture have a basename-matching dedup file so the line-1256 reset's **auto-detect actually fires** (not merely "#2 ≠ X"). *(§4.3.)*
- **I5.** PLAN **A6/A7**: pin the timestamp `sprintf` format strings and unit-test a known UTC epoch → exact zero-padded bytes (e.g. epoch 0 → `1970-01-01` / `00:00:00`). *(§2.)*
- **I6.** PLAN **B1**: add one sentence that the alignment parser is a **single ordered `if/elsif` chain** (first-match-wins), not independent prefix tests — the strand-origin SE patterns are textual prefixes of PE patterns and only branch order disambiguates. *(§1.9 / SPEC §8.10.)*
- **I7.** PLAN **§10/§2.1**: promote the `include_str!` path strategy from "open" to **decided** — recommend copy-into-crate (`bismark-report/assets/`) to avoid the `../../../plotly/` build fragility; this affects the §2.1 layout. *(§2.)*

### Optional (robustness / nice-to-have)
- **O1.** PLAN **C1**: note that Perl only `die`s on the plot.ly inject (69–71); the bismark/bioinf logo injects silently no-op if the marker is absent (74/79 have no `else{die}`). Mirror or document the divergence. *(§1.7.)*
- **O2.** PLAN **D4**: add an assertion that `-o foo` yields a file literally named `foo` (no `.html`), `--dir`-prefixed. *(§4.5.)*
- **O3.** PLAN **B5/B6**: assert the nucleotide header validation keys on **exactly** column indices 2 and 4 (a shifted row fails; junk-elsewhere-but-right-at-2/4 passes). *(§1.8.)*
- **O4.** PLAN **C6**: state explicitly that the C6 self-generated golden must be validated against Perl in E2 (avoid a self-referential regression-lock baking in a bug). *(§6.)*
- **O5.** PLAN **§5/§11**: keep the sequential-replace design and add one line explaining *why* a single-pass/map-based templating engine is rejected (order-dependent `s///g` semantics), to inoculate against a future "optimization." *(§5.)*

---

## 9. Where the PLAN is simply correct (brief)
- 11-step orchestration order (§3/C4) matches Perl 59–156 exactly.
- All fill gates restated as `is_some()` incl. `0`-passes, with a dedicated regression fixture (B6/E1.4). ✅
- M-bias three-facts (state-driven deletion / `%mbias_2`-driven fill / 24 survivors outside the deletable spans) — empirically re-verified against the template (placeholders at 843–907 & 997–1060, spans at 465–496). ✅
- Greedy/dotall collapse vs excise (C1/C2) — each marker confirmed 2× in template. ✅
- Dedup leftover-fallback signed `i64` (B2/E1.5) matches Perl 544–548. ✅
- Asset normalizer faithful to `read_report_template` incl. empty-input guard (A5). ✅
- Perl-oracle harness with anchored single-match timestamp normalization (E2) cannot mask real diffs. ✅
- Phase A→F sequencing is buildable; pure parse/fill split genuinely permits B before C. ✅
- Efficiency scoping (no streaming/parallelism/mimalloc) is right. ✅

---

**Report path:** `/Users/fkrueger/Github/Bismark-report/plans/06012026_bismark2report/PLAN_REVIEW_A.md`
