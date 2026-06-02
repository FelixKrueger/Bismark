# Changelog

All notable changes to `bismark-summary` will be documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).

## [1.0.0-beta.1] — 2026-06-01

Initial Rust port of Bismark Perl's `bismark2summary` (v0.25.1) — the
**project-level, multi-sample** aggregator (distinct from the per-sample
`bismark2report`). Binary installs as `bismark2summary_rs` during the
Perl→Rust coexistence period.

**Byte-identity:** the tab-delimited `bismark_summary_report.txt` is
byte-for-byte identical to Perl `bismark2summary` v0.25.1; the
`bismark_summary_report.html` is byte-for-byte identical **modulo the single
`localtime` timestamp line**. Confirmed by a Perl-oracle integration suite and
by a real-data gate on `oxy` against real Bismark report sets — 4 × paired-end
(dedup-mode) and 2 × single-end (raw-mode), `.txt` identical and `.html`
identical (modulo timestamp), ~3.15 MB each.

### Added

- New crate `bismark-summary` (library + `bismark2summary_rs` binary). It opens
  **no BAM** — it discovers Bismark BAMs by filename, derives each sample's
  text report names, parses per-sample metrics, and emits one project summary:
  `bismark_summary_report.txt` (15-column table, one row per sample) and
  `bismark_summary_report.html` (plot.ly stacked-area alignment graphs + three
  per-context methylation graphs).
- **BAM discovery** reproduced exactly: explicit positional BAMs in argv order,
  else four globs in Perl's fixed order (`*bismark_bt2.bam`,
  `*bismark_bt2_pe.bam`, `*bismark_hisat2.bam`, `*bismark_hisat2_pe.bam`), each
  sorted with Perl's **case-folded** glob collation (case-fold-primary,
  raw-bytes-secondary) — NOT bytewise. Spike-confirmed identical on macOS
  Perl 5.34 and Linux Perl 5.38, locale-invariant, including the case-only
  tiebreak (`plans/06012026_bismark2summary/SPIKE_glob_sort_order.md`).
- **Report parsers** (alignment, deduplication, splitting) with PE/SE pattern
  sets, the dedup `aligned_reads` overwrite, the splitting
  `Total C to T conversions` methylation overwrite, last-match-wins, and the
  `total_c`-anchored-vs-context-unanchored regex asymmetry.
- **HTML engine** filling the inline Perl heredoc template (embedded verbatim
  via `include_str!`, drift-guarded against the source) with `include_str!`'d
  plot.ly + logo assets normalized exactly as Perl `read_report_template`
  (`chomp` + strip-all-`\r` + per-line `\n`). The two **independent**
  section-deletion predicates are reproduced — numbers gated on
  `$dup_alignments =~ /^,{1,}$/`, percentages on `if ($aligned)` — including
  their genuine divergence for a single RRBS sample.
- **Faithful C `%.15g`** formatter (`fmt_g`, shared design with
  `bismark-bedgraph`) powering the asymmetric percentage output: methylated +
  alignment percentages are `%.2f` verbatim, while the six unmethylated arrays
  are `100 − <rounded %.2f>` re-stringified via `%.15g` (trailing zeros
  dropped) — bit-exact vs Perl, including the FP artifact
  `100 − 99.99 → 0.0100000000000051`.
- Full flag surface: `-o/--basename`, `--title`, `--verbose`, `--version`,
  `--help`/`--man`. Output `<basename>.txt` / `<basename>.html` (default
  basename `bismark_summary_report`); basename/title fall back to defaults on
  Perl truthiness (unset / `""` / `"0"`).
- Hidden `--__test_timestamp <EPOCH>` for byte-stable committed HTML goldens.
- Test suite: unit tests, a **Perl-oracle byte-identity** suite across the
  WGBS / all-RRBS / single-RRBS-asymmetry / plot-excluded / mixed-types-die /
  mixed-case-glob / `%.15g`-tail / argv-order / `-o 0` shapes (auto-skips if
  `perl` is absent), a template drift-guard, and a stale-oracle tripwire.

### Notes / intentional divergences from Perl

- **Timestamp formatted in UTC, not local time.** Perl uses scalar
  `localtime`; this port formats the one `{{report_timestamp}}` line in UTC
  (pure `std`, no `unsafe`, no new dependency — preserving
  `#![forbid(unsafe_code)]`). The acceptance gate normalizes this single line
  (Perl `localtime` cannot be pinned), so output byte-identity is unaffected;
  the only user-visible effect is the live timestamp reading UTC.
- **`--help`/`--man`/`--version` exit 0** (clap default); Perl's `exit 1`-on-help
  quirk is not reproduced. Help/version/diagnostic text is not byte-gated.
- A degenerate plotted sample with **zero total alignment reads** raises a typed
  error (reproducing Perl's "Illegal division by zero" die) rather than emitting
  a `NaN`/`inf` HTML. Unreachable on real Bismark data.
- The latent Perl bug at `bismark2summary:1662` (the CHH zero-check tests
  `$total_CHG`, not `$total_CHH`) is reproduced verbatim (dead for plotted
  samples).
- The checked-in `docs/images/bismark_summary_report.{txt,html}` are **stale**
  (v0.15.2 Highcharts-era HTML; `CpHs` column labels) and are NOT the oracle —
  a tripwire test guards against re-adopting them.

### Out of scope

- No per-sample `bismark2report` (a separate crate/port).
- STDOUT/STDERR diagnostics and `--help`/`--version` text are not byte-gated.
