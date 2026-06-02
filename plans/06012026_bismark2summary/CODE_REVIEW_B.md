# Code Review B — `bismark-summary` (Rust port of Perl `bismark2summary`)

**Reviewer:** B (independent, fresh context — parallel to Reviewer A; no coordination)
**Date:** 2026-06-01
**Scope:** `rust/bismark-summary/src/{lib,cli,main,error,discovery,parse,txt,plot,html,assets,timestamp,fmt_g}.rs`, `summary_template.html`, `Cargo.toml`, and `tests/{common,perl_oracle,txt_golden,template_drift}.rs`. Contract: `.txt` byte-identical to Perl v0.25.1; `.html` byte-identical modulo the one `localtime` line.
**Verdict:** **APPROVE.** The port is a faithful, well-structured reproduction of the Perl. Every byte-identity-critical path I exercised against live Perl v0.25.1 — including several SPEC-required fixtures that are *missing from the committed test suite* — is byte-for-byte identical. No correctness defects found. Findings are one Low faithfulness gap (div-by-zero), a handful of test-coverage gaps vs SPEC §7, and minor nits. **I would fix none of the source; I recommend the caller close the test-coverage gaps.**

---

## Gate results (run from `rust/`, sandbox disabled — worktree is outside the sandbox)

| Gate | Result |
|---|---|
| `cargo test -p bismark-summary` | **GREEN** — 49 unit + 5 perl-oracle + 1 template-drift + 2 txt-golden = **57 pass, 0 fail**. The 5 Perl-oracle tests ran against **real Perl** (perl present) — i.e. the byte-identity gate is genuinely satisfied, not skipped. |
| `cargo clippy -p bismark-summary --all-targets -- -D warnings` | **GREEN** (exit 0, no warnings) |
| `cargo fmt -p bismark-summary -- --check` | **GREEN** (exit 0) |

### Independent manual Perl-vs-Rust diffs (beyond the committed oracle tests)
I hand-built and diffed five additional fixtures against live Perl (timestamp line normalized via `sed`). **All `.txt` and `.html` byte-identical:**

- **Single RRBS** (the §2.9 ⚠ numbers/percentage asymmetry): identical, 3,147,683 bytes each.
- **Mixed-case auto-glob** `apple/Mango/zebra` RRBS (SPEC's MANDATORY fixture #8): row order `apple, Mango, zebra` (case-folded), `.txt`+`.html` identical.
- **Non-trivial `%.15g` tail** (same dir): emitted `y: [0.0100000000000051,50,87.7]` — `100−99.99`, `100−50.00`, `100−12.30` — **bit-exact vs Perl**, confirming the asymmetric unmeth engine at the integration level.
- **Single WGBS** (dedup layout, `/^,{1,}$/`-needs-≥1-comma): identical.
- **All-excluded** (every sample drops a context → 0 plotted): identical.

This is the strongest possible evidence that the missing committed fixtures (below) are a *coverage* gap, not a *correctness* gap.

---

## Findings by area

### 1. Byte-identity faithfulness — PASS

- **Parsers (`parse.rs`).** PE/SE pattern split, `total_c` `$`-anchored vs the six unanchored meth patterns, last-match-wins (scan all lines + overwrite), dedup `aligned_reads` overwrite, splitting `Total C to T conversions` overwrite — all match Perl `:288-380` exactly. `capture_count`'s "require ≥1 whitespace immediately after the label, then leading digits, then (if anchored) nothing-may-follow" faithfully reproduces `^label\s+(\d+)[$]` including the Perl-backtracking edge (whitespace must start *immediately* after the label, else no match — verified by hand).
- **`capture_dedup_total`** (greedy `.+:` ⇒ last colon before `\s+\d+$`) verified against Perl on 4 edge cases (filename with embedded colons, empty `.+`, trailing-`$` anchor): identical (`MATCH 900/42/5`, `NO MATCH` for empty-`.+`).
- **`.txt` assembly (`txt.rs`).** Header verbatim incl. the lowercase `chgs` quirk (cols 12-13) and capitalised `CpGs`/`CHHs`; col 1 = raw `bam` (not stripped base); raw-before-mutation values (empty cells preserved); header + every row `\n`-terminated, plot-excluded rows present. Matches `:228-404`.
- **Plot assembly (`plot.rs`).** 0-defaulting of unaligned/ambig/no_seq + the six meth counts, the `dup_reads ne '' → aligned=""` blanking (dup/unique NOT defaulted), the three-context plot-exclusion `next`, and `num_samples = total` vs y-arrays = plotted. Matches `:406-456`.
- **HTML fill order + the TWO independent section-deletion predicates (`html.rs`).** This is the riskiest area and it is correct:
  - `{{aligned_seq}}` fill (`:1419`) **precedes** the raw-section deletion (`:1430`). In dedup mode the empty `aligned` is filled into `{{aligned_seq}}` (line 170 of the template), then the entire `{{raw_aligned_reads_section}}` span (lines 169-190, containing that placeholder) is deleted — net: no stray empty trace, no surviving `{{…}}`. The Rust preserves this exact statement order (html.rs:72 before :84).
  - Numbers deletion gates on `$dup_alignments =~ /^,{1,}$/` (`all_commas(dup_alignments)`); percentage deletion gates on `if ($aligned)` (`raw_mode = !aligned.is_empty()`). Different predicates, faithfully reproduced — and the single-RRBS divergence (numbers→dedup layout, percentages→raw layout) is **byte-identical to Perl** (oracle + my manual diff).
  - `{{p_aligned_replace}}` fill gated `if raw_mode` (never fires in dedup mode, where its span is already deleted) — matches `:1591`.
- **`%.2f`/`%.15g` asymmetry (`fmt_g.rs`/`html.rs::meth_pair`).** Meth + all alignment percentages are `format!("{:.2}")` verbatim; the six unmeth arrays are `100 − <reparsed %.2f>` re-stringified via `format_g15`. Bit-exact vs Perl (the `99.99 → 0.0100000000000051` artifact reproduced).
- **Asset normalizer (`assets.rs`).** `chomp` + `s/\r//g` (strips ALL `\r`, not just trailing) + append-`\n` per line, empty-input → empty guard. Matches `read_report_template` `:136-149`. plot.ly cached via `OnceLock` (normalized once).
- **Template fidelity.** `summary_template.html` is 882 lines (Perl :490-1371 = 882), head `<!DOCTYPE html>` / tail `</html>`; the `template_drift` test re-extracts the heredoc from the Perl source and asserts byte-equality (passes). I enumerated all 40 distinct `{{…}}` placeholders in the template and confirmed every one is handled by `build_html` (filled or span-deleted) with the correct global (`.replace`) vs single (`.replacen(…,1)`) semantics — all single-subst markers verified to occur exactly once; all span markers exactly twice.
- **Timestamp (`timestamp.rs`).** `format_ctime_utc` is **bit-exact vs Perl `gmtime`** across 13 epochs incl. leap-year 2000-02-29, year 9999, century 2100, and single-digit-day space-padding. (UTC-vs-localtime deviation is documented and gate-normalized — sound.)

### 2. Correctness / bugs — no defects

- `all_commas` correctly returns **false** for `""` (N=1 join, and all-excluded `""`) — the load-bearing `/^,{1,}$/`-needs-≥1-comma semantics (verified by unit test + my single-WGBS and all-excluded diffs).
- `strip_bam_suffix` (char-count `saturating_sub(4)`, `<4 chars → ""`) and `munge_name`/`strip_bismark_bam` (the `_bismark.bam$` any-char-wildcard at byte `n-4`, the `.fq.gz`/`_trimmed`/`_[12]` sequential strip) verified against Perl on 6 inputs incl. `_bismark.bam` → `""`, `x_bismarkZbam` → `x`. Char-boundary-safe (the matched bracket bytes are ASCII).
- `delete_span`/`inject_span` use first..**last** occurrence (state "last", not "second" — future-proof per SPEC gotcha 5) with a ≥2-marker guard returning false/no-op on a lone marker (matches Perl's `m.*m` requiring two markers).
- `num()` uses `i64` (read/cytosine counts exceed `u32`); FP math stays < 2^53 so `as f64` division is exact, matching Perl's double arithmetic.
- The CHH `total_CHG==0` latent Perl bug (`:1662` tests CHG not CHH) is reproduced verbatim (html.rs:193 `if total_chg == 0`) with a comment; dead for plotted samples.
- Mixed-types `die` (`:1488`) fires inside `build_html` **after** the `.txt` is on disk and **before** any `.html` — matches Perl (`.txt` present, `.html` absent on error; oracle test asserts both).

### 3. Errors / edge cases

- **`pct2` division-by-zero is a divergence (Low).** Perl `$x/0` **dies** ("Illegal division by zero", nonzero exit, no `.html`). Rust `pct2` computes `part/0.0*100.0` → `inf`/`NaN`, `format!("{:.2}")` → `"inf"`/`"NaN"`, and writes a *malformed `.html` with exit 0*. Reachable only in the **alignment percentage** loop when a plotted sample has `aligned+no_seq+not_aligned+ambig == 0` — i.e. a sample with methylation calls but zero reads in every alignment bucket. This is effectively impossible in real Bismark output (calls imply aligned reads), and the **meth** percentage path is already guarded (`total==0 → "NA"/"0"`). So it cannot fire on realistic data, but it is a genuine faithfulness gap: Perl errors, Rust silently emits `inf`/`NaN`. **Recommend** (not fix): either guard the alignment-percentage total (return `MixedSampleTypes`/a new error, or skip) or document the boundary in `pct2`'s doc-comment as an accepted divergence. I lean toward a one-line doc note since the path is unreachable in practice; a hard guard risks introducing a new non-Perl exit on a path Perl also dies on (behavior already differs only in the *kind* of failure).
- Empty/all-excluded inputs: handled (both numbers + percentage gates take the dedup `else`, loops don't iterate, joins `""`) — byte-identical to Perl (my all-excluded diff).
- `read_report` lossy-UTF8: reports are ASCII; captured values are digit-only runs, so replacement chars (if any) never reach the `.txt`/`.html`. The raw `bam` column is OS-string-lossy on both the argv and glob paths — consistent. No byte-identity risk.
- Exit codes: clap usage errors → 2; `BismarkSummaryError` → 1; success/help/version/man → 0. `--help` exit 0 (intentional non-reproduction of Perl's `exit 1`, SPEC §4.4).

### 4. Efficiency — acceptable

- ~25 sequential `String::replace`/`replacen` passes over the ~3 MB doc. Each is O(n); ~25n ≈ 75 MB of scanning for a one-shot CLI that runs once per project — negligible (the tool is I/O- and human-bound, not throughput-bound). The 3 MB plot.ly is `include_str!`'d and normalized once via `OnceLock`. No concern.
- `discover_bams` reads the dir once and filters per-suffix (4 passes over a small name list) — fine.

### 5. Structure / style — clean

- `#![forbid(unsafe_code)]` maintained (the UTC timestamp choice is *because* of it — documented in `timestamp.rs`). `#![warn(missing_docs)]` honored throughout.
- Module split (cli/discovery/parse/txt/plot/html/assets/timestamp/fmt_g/error) is clear and mirrors the Perl flow. Naming maps to Perl variables, easing audit.
- Documented deviations (assets embedded, template-as-asset + drift guard, UTC timestamp, help/version exit 0, hardcoded `0.25.1`) are all called out in doc-comments and match SPEC §4. `fmt_g.rs` doc-comment correctly cites `bismark2summary §2.9a` (per O2/Reviewer-B note), not bedGraph internals.
- Workspace member added (`rust/Cargo.toml` includes `bismark-summary`); `README.md` present (Cargo.toml `readme` resolves).

---

## Test-coverage gaps vs SPEC §7 (Medium — recommend, do not block)

The committed `perl_oracle.rs` has 5 fixtures (WGBS-2, all-RRBS, single-RRBS, plot-excluded, mixed-die). SPEC §7 mandates **10 fixtures + a stale-oracle tripwire**. Missing from the *oracle* suite:

1. **(SPEC #8) Mixed-case auto-glob — MANDATORY.** SPEC calls it "the only fixture that catches a bytewise regression." It is **not** in `perl_oracle.rs`. Mitigation: the `discovery::sort_glob` unit test pins the case-folded order, and **my manual diff confirms the end-to-end glob row order matches Perl** — so the implementation is correct; the *guard against future regression* is what's thin. **Recommend adding an oracle fixture** (or at least an end-to-end glob-order test on the built binary).
2. **(SPEC #7) All-excluded** oracle fixture — missing (my manual diff passes).
3. **(SPEC #4) Single-WGBS** oracle fixture — missing (my manual diff passes).
4. **(SPEC #9) Non-trivial `%.15g` tail** *integration*-level fixture — only covered at unit level (`fmt_g`/`meth_pair` tests); SPEC wants it end-to-end (my manual diff passes).
5. **(SPEC #10) `-o 0`/`--title 0`** truthiness + **explicit-`@ARGV` order** + **`--title` with spaces** — truthiness is in `cli.rs` unit tests and argv-order in `discovery.rs`, but none exercise the built binary end-to-end.
6. **(SPEC §7) Stale-oracle tripwire** — the test that greps the checked-in `docs/images/bismark_summary_report.html` for `Plotly` and asserts **0 matches** is **absent**. Cheap to add; prevents the stale Highcharts file from ever being silently adopted as the gate.

None of these indicates a bug (I verified #4/#7/#8/#9 byte-identical by hand), but they leave SPEC-required regressions unguarded.

---

## Nits (Low)

- **`is_ascii_whitespace()` ≠ Perl `\s`.** Rust's `is_ascii_whitespace` excludes vertical-tab `\x0b` (Perl `\s` includes it). Used in `capture_count`/`capture_dedup_total`. Only matters if a report put a `\x0b` between a label and its number — which Bismark never does (it uses a literal `\t`). Theoretical only; safe to leave, optional one-line doc note.
- **Explicit-BAM with a directory component** (`./sub/x.bam`) yields a doubled `././sub/...` report path via `Path::new(".").join("./sub/...")`. Functionally fine (`.exists()` resolves it) and the `.txt` col 1 is the raw bam, so byte-identical to Perl. Cosmetic.
- `cli.rs` `--man` is a separate bool from clap's auto `--help` (Perl unifies them as `help|man`). Functionally equivalent (both print help, exit 0); help text isn't byte-gated. Fine.

---

## Recommendations (prioritized)

| Priority | Item | Fix vs Recommend |
|---|---|---|
| **Low** | `pct2` div-by-zero: Perl dies, Rust emits `inf`/`NaN`+exit 0. Unreachable on real data (meth path already guarded). Add a doc note or a guard. | Recommend |
| **Medium** | Add the **mixed-case auto-glob** oracle fixture (SPEC §7 #8, MANDATORY) — verified correct by hand but unguarded against regression. | Recommend |
| **Medium** | Add the **stale-oracle tripwire** unit test (grep `docs/images/...html` for `Plotly`, assert 0). | Recommend |
| **Medium** | Add oracle/end-to-end fixtures for single-WGBS (#4), all-excluded (#7), integration `%.15g` tail (#9), `-o 0`/`--title 0`/argv-order/`--title`-spaces (#10). | Recommend |
| **Low** | Doc-note the `is_ascii_whitespace` vs Perl-`\s` (`\x0b`) and `././` path-doubling edges. | Recommend |

No source edits applied (parallel dual review — edits would race with Reviewer A). All findings above are for the caller to triage.
