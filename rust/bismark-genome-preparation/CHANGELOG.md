# Changelog

All notable changes to `bismark-genome-preparation` will be documented in this file.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning: [SemVer](https://semver.org/spec/v2.0.0.html).

## [1.0.0-beta.1] — 2026-06-03

Version-coherence bump (no functional change): promoted to `beta` to reflect
shipped + byte-identical status.

## [1.0.0-alpha.2] — 2026-05-31

Implements **`--genomic_composition`** (previously accepted-and-ignored in
alpha.1). Tracking issue:
[#919](https://github.com/FelixKrueger/Bismark/issues/919).

### Added

- **`--genomic_composition`** — calculates the genome's **mono- and
  di-nucleotide frequencies** and writes
  `<genome>/genomic_nucleotide_frequencies.txt` (in the genome folder, not
  `Bisulfite_Genome/`). Runs **before** the bisulfite conversion, mirroring Perl
  (`get_genomic_frequencies` → `process_sequence_files`). The table is
  **byte-identical** to Perl `bismark_genome_preparation` v0.25.1.
- **NOT the conversion path** (load-bearing): this pass `uc`s the sequence but
  does **not** apply the conversion's `[^ATCGN]→N` mapping — so IUPAC ambiguity
  codes (`R`/`Y`/`S`/`W`/`K`/`M`/`B`/`D`/`H`/`V`) and stray bytes are counted as
  their own keys; only a literal `N` is skipped (mono), and a di-mer is skipped
  if **either** base is `N`. Di-mers span line boundaries but **not**
  chromosome/file boundaries.
- **Allocation-free counters** (`[u64; 256]` mono + flat `[u64; 65536]` di) — no
  per-base allocation on multi-Gbp genomes — emitted in **plain byte-lexical key
  order** (each mono key immediately before its di block), reproducing Perl's
  `sort keys %freqs` exactly. (Plain byte sort — *not* the case-folding glob
  order.)
- **Faithful edge behaviour:** the legacy `Mus_musculus.NCBIM37.fa` filename is
  excluded from counting (but not conversion); `chomp` then `s/\r//` removes only
  the **first** `\r`; a duplicate chromosome name / non-`>` first line errors
  **before** any table is written (Perl `die`s in `read_genome_into_memory`, so
  no orphan file is left); an empty / `N`-only genome yields a 0-byte file. Write
  failures are **non-fatal** (warn and skip, matching Perl).
- **Tests:** 20 unit tests (counter logic + all edge cases) + 2 binary
  end-to-end + a live `perl_vs_rust_genomic_composition` oracle + a `#[ignore]`
  real-data gate (`genomic_nucleotide_frequencies.txt` vs real Perl).

## [1.0.0-alpha.1] — 2026-05-31

First **public pre-release** of `bismark-genome-preparation` — a Rust port of
Bismark Perl v0.25.1's `bismark_genome_preparation` script. The converted CT/GA
FASTA are **byte-identical** to Perl (the acceptance gate). A **different shape**
from the post-alignment ports: FASTA in → bisulfite-converted FASTA out →
external indexer subprocess; there is **no BAM I/O** (the crate does not depend
on `bismark-io`). The binary installs as `bismark_genome_preparation_rs` during
the v0.26 → v1.0 coexistence period; the `_rs` suffix is dropped once the Perl
scripts move to `legacy/`. Tracking epic:
[#912](https://github.com/FelixKrueger/Bismark/issues/912); PR
[#913](https://github.com/FelixKrueger/Bismark/pull/913).

### Added

- **Bisulfite conversion.** For every input sequence, writes a **C→T-converted**
  (top-strand) copy and a **G→A-converted** (bottom-strand) copy under
  `<genome>/Bisulfite_Genome/{CT,GA}_conversion/`, then runs the external
  indexer on each. Per byte: uppercase → map anything not in `{A,T,C,G,N,\r,\n}`
  to `N` → `tr` (`C→T`/`G→A`). The transform operates on **raw line bytes
  including the terminator**, so exact line-wrapping is preserved — CRLF stays
  CRLF, a final line without a newline keeps none, and interior whitespace
  becomes `N` (all faithful to Perl).
- **Output modes:** combined MFA (default) — `genome_mfa.CT_conversion.fa` /
  `genome_mfa.GA_conversion.fa`; or `--single_fasta` for per-chromosome files.
- **FASTA discovery** with Perl's extension precedence (`.fa` → `.fa.gz` →
  `.fasta` → `.fasta.gz`, first non-empty group wins) and Perl's glob ordering
  (**case-insensitive**; see *Intentional divergences*). gzip input
  (`.fa.gz`/`.fasta.gz`) is read in-process via `flate2::MultiGzDecoder`.
- **External indexers:** `bowtie2-build` (default) / `hisat2-build` /
  `minimap2 -d` (`-k 20`), run **concurrently** for the CT and GA references
  (mirrors Perl's `fork`). Discovery tier `BISMARK_BIN → PATH → current_exe`;
  `--path_to_aligner <dir>` is validated **early** (before conversion) with no
  PATH fallback. `--parallel N` (≥2) passes `--threads N`/`-t N`; the threads
  flag is always emitted (N=1 default, Perl-faithful). `--large-index` passes
  through.
- **`--slam`** (deprecated) — T→C / A→G transitions instead of C→T / G→A; the
  converted-header suffix stays `_CT_converted`/`_GA_converted` (Perl never
  changed it). Emits a deprecation warning.
- **`--combined_genome`** — Bismark-Rust extension (opt-in, additive): also
  writes a single combined CT+GA reference + index under
  `Bisulfite_Genome/Combined/`, for a future Rust aligner to consume. Not
  byte-gated (no Perl counterpart); built from the converted stream so it is
  well-defined in both MFA and `--single_fasta` modes.
- **clap-derive CLI** with the Perl flag surface (underscored long names):
  `--bowtie2`/`--hisat2`/`--minimap2`(`--mm2`), `--path_to_aligner`,
  `--parallel`, `--single_fasta`, `--slam`, `--large-index`,
  `--genomic_composition`, `--verbose`, `-V`/`--version`, `--man`. Mutual
  exclusions match Perl (one aligner; minimap2 excludes
  `--single_fasta`/`--slam`/`--large-index`).
- **Chromosome name = exact Perl semantics:** first whitespace-delimited field
  after `>`, keeping a leading empty field — so a bare `>` and a
  leading-whitespace header both yield an **empty** name (not an error, not the
  next token); only a first line whose first byte is not `>` errors. Duplicate
  names across inputs are fatal.
- **39 unit + 10 integration tests + a `#[ignore]` real-data gate**, including
  live `perl_vs_rust_*` byte-identity tests that run the real Perl script
  (auto-skip if `perl` absent): MFA, `--single_fasta`, mixed-case glob order,
  CRLF, zero-sequence records, CR-only files, `--slam`, and gzip input.

### Design contract

- **Standalone crate** — `clap` + `flate2` + `which` + `anyhow`/`thiserror`; no
  `bismark-io`, no BAM machinery, no `unsafe`.
- **Raw line-streaming** (not `noodles-fasta`, which normalizes records and
  would discard the original line wrapping). Streams the conversion — never
  slurps — so human-scale genomes are fine.
- The **external indexer subprocess is required and inherent** (the indexer is
  not reimplementable) — the opposite of the BAM ports' "no samtools subprocess"
  rule.

### Intentional divergences from Perl (documented)

- **gzip input via `flate2`**, not a `gunzip -c` subprocess. Output bytes
  identical.
- **STDERR/STDOUT diagnostics** mirror Perl in spirit but are not byte-matched;
  `sleep` UX pauses are dropped. `--help`/`--man`/`--version` text is
  clap-generated.
- **`--path_to_aligner` is validated in Step I** (before conversion) so a bad
  path fails before any FASTA is written.
- **Glob order is case-insensitive** (Perl's `glob`/`<>` uses its bundled
  `File::Glob::bsd_glob` csh path, which case-folds on **both** Linux and macOS
  — *not* the platform libc `glob(3)`, and *not* `GLOB_NOCASE` which Perl sets
  only on Windows/VMS). CI-verified on Linux. Real genomes use all-lowercase
  `chrN.fa`, where fold == bytewise, so it never bites them.

### Validation — real-data byte-identity (Phase E, oxy `dockyard-oxy-0`, 2026-05-31)

Perl `bismark_genome_preparation v0.25.1` vs the Rust binary on **copies** of the
same genome (a fake `bowtie2-build` for both, so Step III is a no-op — the gate
is the **converted FASTA**, not the index). Converted CT + GA FASTA compared
byte-for-byte (`cmp` + md5):

| Genome | Mode | CT | GA |
|--------|------|----|----|
| E. coli `NC_010473.fa.gz` (4.75 MB, **gzipped**) | MFA | byte-identical | byte-identical |
| E. coli `NC_010473.fa.gz` | `--single_fasta` | byte-identical | byte-identical |
| Human GRCh38 primary_assembly (**3.15 GB** CT) | MFA | byte-identical | byte-identical |

The gzipped E. coli run also validates the `MultiGzDecoder` input path against
Perl's `gunzip -c`; the 3.15 GB human run confirms byte-identity **and** the
streaming implementation at genome scale (no slurp). The external index build is
deterministic given the same indexer version + identical converted FASTA, so it
is validated by re-running the indexer (secondary check) — **not** byte-reproduced.

### MSRV

Rust **1.89.0** (workspace `edition = "2024"`).

### Out of scope (deferred)

- **`--genomic_composition`** (the `genomic_nucleotide_frequencies.txt` table) —
  accepted-and-ignored with a note; tracked as a follow-up.
- **minimap2 via an in-process Rust engine** — the external `minimap2 -d`
  subprocess is kept; adopting a pure-Rust minimap2 engine is an aligner-layer
  decision tracked separately.
- **Combined-genome ALIGNMENT validation** (`--combined_genome` output is
  produced + structurally checked here; its use for mapping is the future Rust
  aligner's concern).
- **crates.io publication** — path-dep usage is the supported in-workspace model.
