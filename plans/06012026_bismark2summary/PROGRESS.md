# PROGRESS — `bismark-summary` (Rust port of `bismark2summary`)

**Plan:** `SPEC.md` rev 1 (dual-reviewed + glob-sort spike folded). Standalone crate (not an epic).
**Branch / worktree:** `rust/bismark2summary` @ `~/Github/Bismark-summary`.
**Status (2026-06-01):** Phases A + B **IMPLEMENTED & byte-identity-validated locally**. Phase C (oxy real-data gate) pending. NOT committed, NOT merged.

## Phase status

| Phase | Scope | Gate | Status |
|-------|-------|------|--------|
| **A** | scaffold + crate + clap `Cli`/`validate` + error enum + BAM discovery + report-name derivation + 3 parsers + `.txt` table | `.txt` byte-identical to Perl | ✅ DONE — `cmp` clean on the WGBS fixture |
| **B** | `.html`: embedded plot.ly/logo assets + normalizer + verbatim heredoc template + fill engine + raw/dedup section deletion + `%.2f`/`%.15g` percentages + timestamp + hidden `--__test_timestamp` | `.html` byte-identical modulo the timestamp line | ✅ DONE — 3,149,130-byte HTML byte-identical to Perl across WGBS / all-RRBS / single-RRBS / plot-excluded / mixed-die |
| **C** | real-data byte-identity gate on **oxy** + RELEASE checklist + docs/CHANGELOG | Perl≡Rust on a real multi-sample dir | ✅ **GATE PASSED 2026-06-01** (oxy). Docs/CHANGELOG + tag still to do. |

### Phase C real-data gate (oxy, 2026-06-01) — PASSED
Built release on oxy (cargo 1.96.0) from pushed commit `0ee6a56`; Perl v0.25.1 + Rust on identical real Bismark report sets from `~/bismark_benchmarks`:
- **Gate 1** — 4 × RRBS_PE samples (alignment + dedup reports → dedup-mode PE): `.txt` byte-identical, `.html` byte-identical modulo timestamp (3,154,150 B both).
- **Gate 2** — 2 × SE samples (10M_SE directional + full-size human SRR24827373; no dedup → raw-mode SE): `.txt` byte-identical, `.html` byte-identical modulo timestamp (3,150,184 B both).
- Real-world magnitudes (e.g. 168,790,344 total Cs); both dedup-mode and raw-mode exercised. oxy worktree + staging purged on pass.

## Test surface (all green — 66 tests)
- **50 unit tests** — discovery/derivation, glob case-fold sort (+ case-only tiebreak), parsers (PE/SE, overwrites, last-match-wins, CRLF-anchored), `.txt` table, plot assembly (defaulting/blanking/exclusion), `fmt_g` (`%.15g` + the `100−rounded` complement), ctime, html span helpers + render, **`ZeroAlignmentTotal` guard**.
- **12 Perl-oracle integration tests** (`tests/perl_oracle.rs`, auto-skip if perl/source/plotly absent) — WGBS-2-sample, all-RRBS raw mode, single-RRBS section asymmetry (Reviewer A C2), plot-excluded, mixed-types die, **mixed-case glob row-order (§7.8 mandatory), non-trivial `%.15g` tail (§7.9), plot-excluded-in-middle (§7.6), single-WGBS (§7.4), all-excluded (§7.7), explicit-argv-order (§7.10), `-o 0` truthiness (§7.10)**. `.txt` raw-byte + `.html` timestamp-normalized.
- **1 template-drift test** — embedded `summary_template.html` ≡ the Perl heredoc.
- **1 stale-oracle tripwire** — asserts `docs/images/…html` has 0 `Plotly` tokens (Reviewer B 4.5).
- **2 txt-golden tests** — deterministic `.txt` (no perl); no-BAMs error path.
- **clippy** `-D warnings` clean; **`cargo fmt --check`** clean.

## Post-review gap closure (dual code-review + plan-manager, 2026-06-01)
Verdict: code-review A APPROVE-with-changes, B APPROVE, plan-manager COMPLETE-on-behavior (40/41 DONE, 1 PARTIAL §7 fixtures, 1 DEVIATED=UTC accepted). Both agreed gaps **closed**:
- **`pct2` division-by-zero** (both reviewers): added `BismarkSummaryError::ZeroAlignmentTotal` — a plotted sample with zero alignment total now errors (after the `.txt` is written), reproducing Perl's "Illegal division by zero" die instead of emitting a `NaN`/`inf` HTML. Unreachable on real data; unit-tested.
- **§7 fixture coverage** (all three agents): added the 7 missing oracle fixtures (incl. the mandatory mixed-case glob) + the stale-oracle tripwire. All 7 new oracle fixtures ran against live Perl and are byte-identical.

## Implementation notes / deviations
- **Timestamp = UTC, not local (documented deviation).** Perl uses scalar `localtime`; this port formats the `{{report_timestamp}}` line in **UTC** (pure `std`, no `unsafe`, zero new deps — keeps `#![forbid(unsafe_code)]`). Only affects the single timestamp line, which the gate normalizes and which is not byte-gated. Switching to local time would need `unsafe libc::localtime_r` or a heavier dep. See `src/timestamp.rs`.
- **`fmt_g.rs` copied** from `bismark-bedgraph` (duplicate-not-couple, SPEC O2); doc-comment retargeted to `bismark2summary §2.9a`.
- **HTML template** lifted verbatim from the Perl heredoc into `src/summary_template.html` (`include_str!`); a drift-guard test pins it.
- **Glob sort** = case-fold-primary / raw-bytes-secondary per the spike (NOT bytewise). `(to_ascii_lowercase, bytes)` comparator.
- **Two independent section-deletion predicates** (numbers→`$dup_alignments`, percentages→`$aligned`) faithfully reproduced incl. the single-RRBS divergence (Reviewer A C2).
- **Latent Perl CHH `total_CHG==0` bug** reproduced verbatim (dead for plotted samples).
- The post-implementation **dual code-review + plan-manager** audit is the next workflow step.
