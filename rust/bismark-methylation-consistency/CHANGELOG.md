# Changelog

All notable changes to `bismark-methylation-consistency` will be documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).

## [1.0.0-beta.1] — 2026-05-30

First **public pre-release** of `bismark-methylation-consistency` — a Rust port
of Bismark Perl v0.25.1's `methylation_consistency` script. Verified
**byte-identical** to Perl on real 10M-read WGBS data (single-end, paired-end,
and `--chh`). The binary installs as `methylation_consistency_rs` during the
v0.26 → v1.0 coexistence period; the `_rs` suffix is dropped once the Perl
scripts move to a `legacy/` directory. Tracking epic:
[#890](https://github.com/FelixKrueger/Bismark/issues/890); PR
[#896](https://github.com/FelixKrueger/Bismark/pull/896).

### Added

- **Read-level methylation-consistency split.** Reads a Bismark BAM and writes
  three BAMs by the consistency of each read's methylation calls, plus a text
  report:
  - `<root>_all_meth.bam` — `>= --upper_threshold`% methylated (default 90)
  - `<root>_all_unmeth.bam` — `<= --lower_threshold`% methylated (default 10)
  - `<root>_mixed_meth.bam` — in between
  - `<root>_consistency_report.txt` — bucket counts + `%.2f` percentages
  - `<root>` is the input path with one trailing `.bam` removed; outputs are
    written **adjacent to the input** (matches Perl line 186 — *not* dedup's
    basename-strip-to-CWD behaviour).
- **Single-end and paired-end** (`-s`/`-p`), auto-detected from the Bismark
  `@PG` line when neither is given; a BAM with no Bismark `@PG` falls through
  to single-end (matches Perl). For PE, R1+R2 calls are summed and both mates
  are written to the chosen bucket; the report counts **pairs**.
- **`--chh`** experimental context — counts `H`/`h` instead of `Z`/`z`, adds a
  `_CHH` filename infix and the `Too few CHHs` report label.
- **clap-derive CLI** with the Perl flag surface (underscored long names):
  `-p`/`--paired_end`, `-s`/`--single_end`, `--chh`, `--lower_threshold`
  (0–49), `--upper_threshold` (51–100), `-m`/`--min-count` (≥0), `-V`/`--version`.
  `--samtools_path` accepted for compatibility and ignored (pure-Rust I/O);
  `--quiet` (new) suppresses STDERR diagnostics.
- **Round-then-compare classification** — the percentage is rounded to one
  decimal (`sprintf("%.1f")` ⇔ Rust `{:.1}` on the pinned expression
  `meth/total*100`) **before** the threshold comparison, exactly matching Perl
  lines 266/272/282. A spike confirmed Rust/Perl parity including power-of-two
  ties (round-half-to-even on the identical `f64`).
- **Graceful-stop on missing `XM`** (Perl `last`): finalize the partial BAMs +
  report and exit 0, rather than aborting.
- **PE coordinate-sort guard** — rejects `@HD SO:coordinate` for paired-end
  input (the guard Perl *intended*; its own `/^@SO/` check is dead code).
  Single-end is not sort-checked (matches Perl).
- **48 unit + 20 integration tests**, including three live `perl_vs_rust_*`
  byte-identity tests that run the real Perl script (auto-skip if
  `perl`/`samtools` are absent).

### Design contract

- **No `samtools` subprocess**, no `htslib` C-link, no `unsafe`. All BAM I/O via
  `bismark-io` v1.0 (pure-Rust [noodles](https://github.com/zaeleus/noodles)).
- Single-threaded streaming; the input header is written **verbatim** (no `@PG`
  injection).

### Intentional divergences from Perl (documented)

- **Output BAM headers omit the `@PG ID:samtools*` provenance lines** that
  Perl's `samtools view -H` / `samtools view -b -S -` subprocesses inject — the
  noodles writer adds none. Alignment **records** are unaffected; the
  byte-identity contract compares records + `@HD`/`@SQ` + `@PG ID:Bismark` and
  excludes the samtools provenance.
- **Empty buckets are written as valid empty BAMs** (header + BGZF EOF),
  whereas Perl leaves a 0-byte, `samtools`-unreadable file.
- **Stricter record validation** — `bismark-io`'s `BismarkRecord` requires
  valid `XR`/`XG` tags and `XM.len() == seq.len()`; such a malformed record is
  fatal (aborts the file), where Perl would process it. Null on genuine Bismark
  BAMs.

### Validation — real-data byte-identity (Phase D, colossal, 2026-05-30)

Perl `methylation_consistency v0.25.1` vs the release Rust binary on the real
10M Bismark BAMs at `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/`. For
every case the `_consistency_report.txt` was **byte-identical** and each
populated bucket's records (`samtools view`, header omitted) matched exactly:

| Case | Records | Report | Buckets | Perl / Rust |
|------|---------|--------|---------|-------------|
| SE | 8,501,508 | byte-identical | all 3 record-md5 match | 16s / 6s |
| SE `--chh` | 8,501,508 | byte-identical | all 3 match (all_unmeth = 7.31M recs) | 40s / 40s |
| PE | 8,542,385 pairs | byte-identical | all 3 match | 32s / 15s |

Rust is ~2–2.7× faster on SE/PE. The non-empty, matching bucket md5s confirm
record-level byte-identity at scale.

### MSRV

Rust **1.89.0** (required by `bismark-io` v1.0 → `noodles-bam` 0.89).

### Out of scope (deferred)

- Multi-threaded BGZF (`ThreadedBam*`) / `mimalloc` — v1.0 is single-threaded.
- SAM/CRAM **input** beyond what `open_reader` gives for free (Perl is BAM-only
  in practice); output is always BAM.
- Byte-matching STDERR / `--help` / `--version` text.
- crates.io publication (path-dep usage is the supported in-workspace model).
