# Code Review A — `bismark-summary` (Rust port of Perl `bismark2summary`)

**Reviewer:** A (independent, parallel with Reviewer B — no shared state)
**Date:** 2026-06-01
**Scope reviewed:** `rust/bismark-summary/src/{lib,cli,main,error,discovery,parse,txt,plot,html,assets,timestamp,fmt_g}.rs`, `src/summary_template.html`, `Cargo.toml`, `tests/{common/mod,perl_oracle,txt_golden,template_drift}.rs`, against `SPEC.md` rev 1 and Perl `bismark2summary` v0.25.1 (1722 LOC).
**Verdict:** **APPROVE WITH CHANGES.** The implementation is a careful, faithful port; all gates are green and the live Perl-oracle byte-identity tests pass for 5 fixture shapes. One genuine behavioral divergence (alignment-percentage division by zero) and several test-coverage gaps relative to the SPEC §7 matrix should be addressed. **No source files modified** (parallel dual review).

---

## Gate results (run from `/Users/fkrueger/Github/Bismark-summary/rust`)

- `cargo test -p bismark-summary` → **GREEN**: 49 unit + 5 oracle + 1 template-drift + 2 txt-golden = **57 passed, 0 failed**. The Perl oracle tests actually ran against Perl (output shows `perl_out.html`) — i.e. these are real byte-for-byte Perl↔Rust comparisons, not skipped.
- `cargo clippy -p bismark-summary --all-targets -- -D warnings` → **CLEAN**.
- `cargo fmt -p bismark-summary -- --check` → **CLEAN**.
- `#![forbid(unsafe_code)]` maintained (no `unsafe`); the UTC-timestamp deviation that avoids `libc::localtime_r` is documented in `timestamp.rs`.

I independently verified: template marker counts (single-occurrence markers → `replacen(..,1)`; `/g` markers → `.replace()` — all correct), heredoc extraction soundness (drift test logic + `</html>\n` tail), the case-folded glob sort, the `capture_dedup_total` greedy-colon semantics, the asymmetric `%.2f`/`%.15g` percentage pair, and the single-RRBS section-deletion divergence (numbers→dedup layout, percentages→raw layout — reproduced and byte-identical via the oracle).

---

## Findings by area

### Logic / byte-identity faithfulness — strong

Verified faithful against the Perl source:
- **Parsers** (`parse.rs`): PE/SE pattern split, `$`-anchored `total_c` vs unanchored meth/unmeth, last-match-wins (scan-all-overwrite), dedup `aligned_reads` overwrite via the wildcard-filename matcher, splitting `Total C to T conversions` overwrite, three independent `if` (not `elsif`) in dedup. The CRLF behavior (chomp keeps `\r` → anchored `$` fails) matches Perl and is unit-tested.
- **`.txt` assembly** (`txt.rs`): raw-before-mutation capture, lowercase `chgs` columns 12–13, 15 cols, header+rows `\n`-terminated, empty cells for unfound fields. The deterministic `txt_golden` test asserts exact bytes incl. the dedup-overwrite and splitting-overwrite precedence.
- **Plot assembly** (`plot.rs`): 0-defaulting set, `aligned_reads` blanking when `dup_reads ne ''`, plot-exclusion `next` on any zero-call context, `num_samples` = total (incl. excluded) vs plotted-subset arrays.
- **HTML fill order + the TWO independent deletion predicates** (`html.rs`): numbers gate `all_commas(dup_alignments)` (`:1430`), percentages gate `raw_mode = !aligned.is_empty()` (`:1577`), fill-then-delete order preserved (`{{aligned_seq}}` filled before its span may be deleted), `p_aligned_replace` only filled in raw mode. The single-RRBS divergence is reproduced and **confirmed byte-identical to Perl** by `oracle_single_rrbs_section_asymmetry`.
- **`all_commas` / `/^,{1,}$/`**: needs ≥1 comma (N=1 join is comma-free → false); `""` → false. Correct and unit-tested.
- **`%.15g` complement**: round-2dp → reparse → `100 - x` → `format_g15`; the `99.99 → 0.0100000000000051` artifact is unit-asserted.
- **CHH `total_CHG==0` latent Perl bug** (`:1662`): reproduced verbatim (Rust line 193 guards on `total_chg`, computes with `total_chh`); documented as dead for plotted samples.

### Errors / edge cases

**HIGH — H1. Alignment-percentage division by zero diverges from Perl.**
In `html.rs::pct2` (lines 224–226) the alignment-percentage loop computes `part / total * 100.0` with no zero guard. If a *plotted* sample has `total == 0` (raw mode: `aligned+no_seq+not_aligned+ambig == 0`; dedup mode: `unique+dup+no_seq+not_aligned+ambig == 0`), Rust yields `NaN`/`inf` → `format!("{:.2}", NaN)` = `"NaN"`, writes a **malformed `.html` and exits 0**. I verified directly: Rust `0.0/0.0*100 = NaN`, `5.0/0.0 = inf`; Perl `sprintf("%.2f", $p/0)` **dies** with `Illegal division by zero` (nonzero exit, no `.html`). This is a real byte-identity/behavior divergence. Reachability: plot-exclusion only guarantees the three *methylation* context totals > 0, NOT the alignment total, so a degenerate sample with methylation calls but zero alignment counts reaches it. Rare in real data but a crafted/edge input would silently produce wrong output where Perl aborts.
*Recommendation (do not auto-fix in this parallel review):* in the alignment-percentage loop, treat `total == 0` like Perl — return an error (reuse a div-by-zero / generic error variant) so the tool aborts before writing `.html`, matching Perl's exit behavior and the "fail loud" principle. (The methylation loop is already guarded by the `total_*==0` branches, so it is safe; the buggy-CHH branch shares this exposure only when unreachable.)

**LOW — L1. `read_report` lossy UTF-8.** `String::from_utf8_lossy` substitutes U+FFFD for invalid bytes; Perl reads raw bytes. Bismark reports are ASCII so this never bites in practice; acceptable and documented.

**LOW — L2. `\s` whitespace class width.** `capture_count` uses `is_ascii_whitespace()` = `[ \t\n\r\x0c]` (no vertical tab `\x0b`). Perl `\s` (default) also excludes VT historically; functionally equivalent for Bismark's tab-separated reports. Not actionable.

### Efficiency — acceptable

**LOW — L3. ~30 sequential `String::replace` passes over the ~3 MB document.** Each `.replace()` allocates a fresh `String` and scans the whole doc; after plot.ly injection the doc is ~3 MB, so this is ~30 full passes (tens of MB of transient allocation). For a once-per-run CLI this is negligible (the oracle suite completes in <6 s including spawning Perl). Asset normalization is correctly cached via `OnceLock`. No change needed; noting only for completeness.

### Structure / style — clean

- Module split is clear and each file's doc-comment cites the exact Perl line ranges. Naming mirrors the SPEC and Perl variables.
- `error.rs` variants map cleanly to Perl's `die` sites; `main.rs` writes `.txt` before `.html` so the mixed-types die leaves the `.txt` on disk (matches Perl; asserted by `oracle_mixed_types_die_writes_txt_not_html`).
- Minor duplication between `parse_alignment_report` and `parse_splitting_report` (shared meth labels) and between `inject_span`/`delete_span` — intentional and readable; not worth refactoring given the byte-identity stakes.

### Test coverage vs SPEC §7 fixture matrix — several gaps

The 5 oracle fixtures cover SPEC items #1 (multi-WGBS), #2 (all-RRBS raw), #3 (single-RRBS asymmetry), #5 (mixed die), #6 (plot-excluded). Missing **integration/oracle** coverage for fixtures the SPEC §7 lists as required/mandatory:

**MEDIUM — M1. Mixed-case glob row-order fixture (§7.8) — SPEC calls this "mandatory."** The case-folded sort is covered at the *unit* level (`glob_sort_is_case_folded_not_bytewise`, `glob_sort_case_only_tiebreak`), which does catch a bytewise regression. But there is no end-to-end oracle fixture (e.g. `apple_…`, `Mango_…`, `zebra_…`) asserting the row order propagates through both outputs against Perl. Recommend adding one.

**MEDIUM — M2. Non-trivial `%.15g` tail fixture (§7.9).** The asymmetric unmeth formatting is unit-tested in `fmt_g.rs`/`html.rs` (incl. `99.99→0.0100000000000051`), but no *integration* fixture pins a sample whose CpG counts force e.g. `87.7` (trailing-zero drop) through the full Perl-oracle pipeline. Recommend adding (cheap).

**MEDIUM — M3. `-o 0` / `--title 0` / `--title` with spaces / explicit-argv order (§7.10).** Truthiness fallback and argv order are unit-tested in `cli.rs`/`discovery.rs`; no end-to-end oracle assertion. Lower priority since the unit tests are convincing.

**LOW — L4. Single-WGBS (§7.4) and all-excluded (§7.7) oracle fixtures absent.** Single-WGBS is partially covered by the WGBS-two-sample path; all-excluded (zero plotted samples → empty joins → both deletions take the dedup `else`, percentage loops don't iterate) is a distinct edge worth one oracle fixture to confirm Perl emits an HTML with empty traces and Rust matches.

**LOW — L5. Stale-oracle tripwire (§7 final item) absent.** SPEC requested a unit test that greps the committed `docs/images/bismark_summary_report.html` (still the v0.15.2 Highcharts file, present on disk) for `Plotly` and asserts 0 matches, so the stale oracle can never be silently re-adopted as the gate. Not implemented. Cheap to add; low risk since the oracle harness regenerates from live Perl, but it is an explicit SPEC deliverable.

---

## Recommendations (prioritized)

| # | Pri | Finding | Action | Fix vs Recommend |
|---|-----|---------|--------|------------------|
| H1 | High | Alignment-% div-by-zero → Rust emits `NaN` HTML + exit 0; Perl dies | Add `total==0` guard → error before writing `.html` | **Recommend** (caller to apply) |
| M1 | Med | Mandatory mixed-case glob row-order oracle fixture (§7.8) missing | Add end-to-end oracle fixture | **Recommend** |
| M2 | Med | Non-trivial `%.15g`-tail oracle fixture (§7.9) missing | Add oracle fixture | **Recommend** |
| M3 | Med | `-o 0`/`--title 0`/argv-order oracle coverage (§7.10) only at unit level | Add oracle fixture(s) | **Recommend** |
| L4 | Low | Single-WGBS (§7.4) + all-excluded (§7.7) oracle fixtures missing | Add oracle fixtures | **Recommend** |
| L5 | Low | Stale-oracle tripwire (§7) test missing | Add `Plotly`-grep assertion on `docs/images` HTML | **Recommend** |
| L1 | Low | `read_report` lossy UTF-8 | None (documented, ASCII inputs) | Accept |
| L2 | Low | `\s` VT-width nuance | None | Accept |
| L3 | Low | ~30 full-doc `String::replace` passes | None (once-per-run CLI) | Accept |

**Bottom line:** The port is correct and byte-identical to Perl v0.25.1 across the 5 exercised oracle shapes, clippy/fmt/test all green, `#![forbid(unsafe_code)]` intact. Before tagging, address **H1** (the one genuine behavioral divergence) and close the SPEC §7 oracle-fixture gaps (M1 is flagged "mandatory" by the SPEC; M2/M3/L4/L5 round out the matrix). None of these block Phase A/B's core byte-identity claim; they harden the gate and fix a silent-NaN edge.
