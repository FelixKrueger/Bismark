# Code Review A — `bismark-report` (Rust port of Perl `bismark2report`)

**Reviewer:** Code Reviewer A (fresh context)
**Date:** 2026-06-01
**Scope:** `rust/bismark-report/` crate vs. Perl `bismark2report` (v0.25.1, 1316 lines) + `plotly/plotly_template.tpl`. Acceptance gate = generated HTML **byte-for-byte identical** to live Perl, modulo the one `localtime` timestamp line.
**Constraint:** did NOT modify any source (parallel Reviewer B is editing the tree). All concrete fixes are under Recommendations.

---

## Summary

The port is **faithful and high quality.** I verified the parser branch order, prefix matching, the fill gates, the section collapse/excise/inject helpers, the M-bias `state`-vs-`r2.is_empty()` split, the Unknown-context `<tr>` byte layout, the `read_report_template` normalizer, the timestamp math, and `subst_all`/`find`/`rfind` against the Perl source line-by-line and with live Perl experiments. The 4 Perl-oracle byte-identity tests pass, plus 36 unit + 8 CLI tests (48 total); `cargo clippy --all-targets -- -D warnings` is clean.

**For all realistic Bismark-generated inputs, I found NO byte-identity-affecting bug** in the HTML *content*. The substitution engine, gates, section logic, M-bias survival, nucleotide missing-key handling, dedup fallback, and the greedy `Bismark report for:` split all reproduce Perl exactly on genuine reports.

I did find **three behavioral divergences**, all confined to artificial or manual-only invocations that the Bismark pipeline never issues:

1. **Glob sort order differs** (`File::Glob` collation vs Rust byte `sort()`) — can change WHICH auto-detected report is processed first, which (via the line-1256 first-report-only reset) can change which output file an explicit companion attaches to. **Medium** (can affect HTML bytes, narrow combination, untested).
2. **`-o ""` (empty output name)** — Perl derives the name (truthiness at line 50), Rust uses the empty string and errors. **Low** (degenerate flag, no content impact).
3. **`Bismark report for:` line with trailing text after the final `)`** — Rust fails to parse filename/version; Perl succeeds. **Low** (Bismark never emits trailing text; the line always ends in `)`).

None of these is reachable from the standard Bismark workflow (which always passes explicit `--alignment_report` + explicit companions, and never `-o ""`).

---

## Issues by area

### Logic

**L1 (Medium) — Glob sort order: `File::Glob` collation ≠ Rust byte `sort()`.**
`discovery.rs:180` (`out.sort()`) sorts `PathBuf`s in byte order. Perl's `<*E_report.txt>` (line 1114) and the companion globs (`<$basename*...>`, lines 1153/1182/1212/1240) use `File::Glob`'s built-in collation, which is **not** byte order and **not** locale `strcoll` (it's locale-independent here — verified under `LC_ALL=C`).

Verified divergence with files `1_ _ a_ B_ *E_report.txt`:
- Perl glob → `1_`, `_`, `a_`, `B_`
- Rust `sort()` (byte) → `1_`(0x31), `B_`(0x42), `_`(0x5F), `a_`(0x61)

Confirmed end-to-end: with two reports `a_PE_report.txt` and `B_PE_report.txt` and no `--alignment_report`, Perl processes `a_` first then `B_`; Rust processes `B_` first then `a_`.

Impact:
- **Common case (no explicit companions):** the *set* of output HTML files is identical and each file's content is identical; only the processing order differs → **no byte-identity impact.**
- **Byte-identity-affecting case:** auto-detect multiple reports **AND** pass an explicit companion flag (e.g. `--dedup_report X`). Per Perl's line-1256 reset, the explicit companion applies to the **first** report only. If "first" differs between Perl and Rust, the explicit companion attaches to a *different* output file → that file's HTML diverges (dedup section filled in one, excised in the other).

The module doc comment in `discovery.rs:1-9` asserts "the resolved files, and thus the HTML bytes, are identical regardless of order" — that is correct for *die-precedence* among the four companion kinds, but it overlooks that the glob **sort** itself (which alignment report is "first") interacts with the first-report-only reset. The claim is too strong.

Not covered by any test: the only multi-report test (`cli.rs:auto_detects_multiple_reports_produces_one_html_each`) uses `one_`/`two_`, which sort identically under both collations, so it cannot catch this.

**L2 (Low) — `-o ""` output-name asymmetry.** Perl uses `defined $manual_output_file` for the ">1 report" guard (line 1129) but `if ($manual_output_file)` (truthiness) when actually choosing the name (line 50). So Perl with `-o ""` + a single report **derives** `sample_PE_report.html`. Rust (`lib.rs:75`) does `match &cli.output { Some(o) => o.clone(), ... }`, so `-o ""` → `out_name = ""` → write fails. Verified live: Perl writes `sampleD_PE_report.html`; Rust exits 1 with `No such file or directory` and produces nothing. The `>1 report` guard itself (`lib.rs:62`, `is_some()`) correctly matches Perl's `defined`. Degenerate flag; no Bismark caller passes `-o ""`.

**L3 (Low) — `Bismark report for:` requires the line to END in `)`.** `alignment.rs:139` gates on `after.last() == Some(&b')')`. Perl's `/^Bismark report for: (.*) \(version: (.*)\)/` is **not** end-anchored, so trailing text after the final `)` is allowed. Verified: `"...foo (version: v0.25.1) trailing"` → Perl FILE=`foo` VER=`v0.25.1`; Rust → no match (both fields stay `None`). The greedy/last-`)` cases all match Perl correctly (verified `(version: 1) ... (version: v0.25.1)` → last wins; `v0.25.1)beta)` → VER=`v0.25.1)beta`; both correct). Real Bismark always ends this line with `)` (see `bismark:1007/1010/1642/1843`), so unreachable in practice.

**L4 (Informational, no fix) — nucleotide `looksOK` gate omitted, correctly.** Perl's `$looksOK` loop (lines 616–622) checks `defined $nucs{$key}->{obs}` / `{exp}` for each present key. Because the parse loop (lines 602–610) *always* assigns into `->{obs}->{percent}` etc., those nested hashrefs are autovivified (defined) for every key that exists in `%nucs`. So `$looksOK` is effectively always 1, and the Rust decision to always fill (`nucleotide.rs:84`, no gate) is faithful. Worth a one-line code comment but not a bug.

**L5 (verified OK) — M-bias header/data, `state` vs `r2.is_empty()`.** `mbias.rs:53` (`line[0]=='C' && line[3..].starts_with(b" context")`) faithfully reproduces `/^(C.{2}) context/` (newlines already stripped by `report_lines`); R2 detection via `windows(2).any(|w| w==b"R2")` matches Perl's whole-line `/R2/`; `read_identity` persists across blocks (matches Perl); the `state` (header-driven, drives R2 `<div>` excision in `template.rs:143-147`) vs `r2.is_empty()` (data-driven, drives R2 fill in `mbias.rs:93`) split is exactly the documented PLAN B5 three-fact model. The R2-header-without-data edge (`state=Paired` but `r2` empty → R2 div kept, `{{mbias2_*}}` survive) is handled correctly. `{{bm_mbias_2}}`→`false` is a verified no-op (token absent from template).

**L6 (verified OK) — `{{mbias2_*}}` survival.** Confirmed in the template: the 12 `{{mbias2_*}}` data placeholders live at lines 997–1060, **outside** the `{{mbias_r2_section}}` span (465–496). SE mode excises the R2 `<div>` but the script-block `{{mbias2_*}}` survive literally — matched against Perl by `se_r1_only_mbias_byte_identical`.

**L7 (verified OK) — branch-order / prefix overlaps.** Checked every alignment branch: `Total number of C` (line 80) precedes the meth branches (matches Perl 276<281) and shares no prefix with `Total methylated`/`Total unmethylated`. PE patterns are listed before SE in each pair; trailing `:` keeps strand patterns (`CT/GA/CT:` vs `CT/CT:`, etc.) mutually exclusive. `unmeth_*` correctly OR's `Total unmethylated C's…` || `Total C to T conversions…` (alignment 90-105 = Perl 298-313). Splitting uses only `Total C to T conversions…` and `C methylated in Unknown context:` (no `(CN or CHN)`) — matches Perl 745-781.

**L8 (verified OK) — `field1` / `split_tab` / `report_lines`.** `split_tab` drops *all* trailing empties (Perl `split /\t/` semantics); a mid-line empty field is preserved as `Some(b"")` (defined-but-empty, matches Perl). `report_lines` matches `while(<FH>)`: no extra trailing empty record, empty input → no lines, `\r` retained (only nucleotide strips it). `strip_first_percent` (`s/%//`, first only) and `before_first_ws` (`s/\s.*//`, ASCII whitespace set = Perl `\s`) are faithful.

**L9 (verified OK) — fill gates use `is_some()` not truthiness.** Alignment (5 fields), dedup (4), splitting (6) all gate on `is_some()` so a `0` count passes (Perl `defined`). On failure the placeholders survive verbatim (alignment returns `doc` unchanged; dedup/splitting return after the section markers were already collapsed — matches Perl, where `s/{{...}}//g` ran first at the call site). No value is substituted before the gate. Verified by `gate_passes_when_no_genomic_is_zero` and `gate_fails_when_field_missing_placeholders_survive`.

### Efficiency

**E1 (Low / informational) — O(passes × doc_size) substitution.** After plotly injection the doc is ~3 MB. Each `subst_all` (`template.rs:13`) allocates a fresh `Vec` and scans the whole doc; there are ~60 call sites (nucleotide loops 20× → ~107 actual passes), so a single report does on the order of 150–180 full-document passes (~0.5 GB scanned + reallocated). This mirrors Perl's per-`s///g` full-scan exactly and is entirely fine for a once-per-sample tool (4 full reports + Perl run in ~13 s in the gate). Not worth optimizing; flagging only so it's a conscious choice. `find`/`rfind` are correct (empty/oversized needle → `None`; off-by-one bounds `0..=h.len()-n.len()` correct), and `subst_all` does not re-scan replacement text (matches `s///g`, no double-substitution / infinite-loop risk).

### Errors

**ER1 (verified OK) — `inject_asset` requires 2 markers.** `template.rs:71` errors (→ exit 1) when a marker appears <2× — mirrors Perl's `die "Plot.ly injection not working…"`. The three real assets contain neither `{{` tokens nor `\r` (asserted in `assets.rs` test), so the greedy `find`/`rfind` splice is byte-safe and cannot accidentally swallow a placeholder.

**ER2 (verified OK) — timestamp.** `civil_from_epoch_utc` (Hinnant) verified against epoch 0, a known epoch, and a leap day; UTC formatting matches Perl `%04d-%02d-%02d` / `%02d:%02d:%02d`. The `localtime_r` `unsafe` block is sound: zeroed owned `tm`, `t` outlives the call, single-threaded, not byte-gated. Test path uses pure-std UTC for stable goldens.

**ER3 (verified OK) — nucleotide header validation.** `s/\r//` (first CR only) applied before split (`strip_first_cr` + `split_tab`); column-3 / column-5 checks (`f.get(2)`/`f.get(4)`) exactly mirror Perl's `$observed`/`$expected eq …` dies. Error text is STDERR-only (not gated).

### Structure

**S1 — Strong module decomposition.** parse/fill split per report kind keeps parsing unit-testable and concentrates Perl's sequential substitution order in `fill`. Naming maps cleanly to Perl. `Vec<u8>` doc (not `String`) correctly preserves non-UTF8 filenames byte-for-byte. Doc comments are unusually precise and cite Perl line numbers throughout — excellent for an audit. Clippy clean.

**S2 (Low) — overstated invariant in a doc comment.** See L1: `discovery.rs:1-9` claims order never affects HTML bytes; that's only true absent the glob-sort + explicit-companion interaction. The comment should be narrowed.

---

## Recommendations (prioritized)

### Critical
None.

### High
None. (No byte-identity bug reachable from genuine Bismark reports.)

### Medium
- **R1 (L1) — Match `File::Glob` ordering, or document the gap.** The Rust `out.sort()` byte order can disagree with Perl's glob collation, which—combined with the first-report-only explicit-companion reset—can change a specific output file's HTML in a multi-report + explicit-companion run. Options, in order of preference:
  1. Replicate `File::Glob`'s sort (it is locale-independent here) so multi-report processing order matches Perl exactly; **or**
  2. If exact-order parity is judged out of scope (Bismark always passes explicit `--alignment_report`), add a one-line caveat to the SPEC/`discovery.rs` docs ("auto-detected multi-report *ordering* may differ from Perl's `File::Glob`; this only affects which file an explicit companion attaches to") and **narrow the `discovery.rs:1-9` claim**.
  - Either way, add a regression fixture: ≥2 auto-detected reports with collation-divergent names (e.g. `a_`/`B_`) **plus** an explicit `--dedup_report`, asserting the Perl-oracle match — today's `one_`/`two_` test masks this.

### Low
- **R2 (L2) — `-o ""` should derive the name** (match Perl line-50 truthiness): in `lib.rs:75`, treat `Some(o)` where `o.is_empty()` the same as `None` (derive from the alignment report). Trivial; aligns the degenerate flag with Perl.
- **R3 (L3) — relax the `Bismark report for:` end-anchor.** `alignment.rs:133-143` requires the line to end in `)`. To match Perl's non-end-anchored regex, find the version as the substring between the last ` (version: ` and the last `)` *after* it (not necessarily end-of-line). Cosmetic for real reports (always end in `)`); only needed for full regex parity.
- **R4 (L4) — add a comment** at `nucleotide.rs:84` explaining the `looksOK` gate is provably always-true given autovivification, so its omission is intentional and faithful.

---

## Verification performed
- `cargo test -p bismark-report` → **48 passed** (36 unit, 8 CLI, 4 Perl-oracle byte-identity).
- `cargo clippy -p bismark-report --all-targets -- -D warnings` → clean.
- Live Perl experiments: `Bismark report for:` greedy/last-`)` cases (4+3 inputs); `-o ""` truthiness asymmetry; `File::Glob` collation under default and `LC_ALL=C`; end-to-end multi-report processing-order divergence (`a_`/`B_`).
- Template/asset audit: confirmed `{{date}}`/`{{time}}` absent from assets (so step-5 timestamp can't corrupt injected JS); `{{bm_mbias_2}}` absent (no-op faithful); `{{mbias2_*}}` placeholders sit outside the deletable R2 span; assets carry no `{{` or `\r`.
- Line-by-line parse/fill/order comparison of all five report parsers against Perl 211–1022.

---

**Report path:** `/Users/fkrueger/Github/Bismark-report/plans/06012026_bismark2report/CODE_REVIEW_A.md`
