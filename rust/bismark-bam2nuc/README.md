# bismark-bam2nuc

A Rust port of Bismark's Perl `bam2nuc` — the **mono- and di-nucleotide coverage**
QC module. The binary is installed as `bam2nuc_rs`.

For every aligned read with a clean CIGAR (no insertion / deletion / soft-clip /
skip), `bam2nuc` tallies the **genomic** sequence at the read's mapped span — the
reference bases the read covers, *not* the read's own bases — reverse-complementing
for reverse-strand reads. It then compares the read-derived composition to the
whole-genome composition. The result (`<sample>.nucleotide_stats.txt`) is picked up
and plotted by `bismark2report`.

## Status

v1.0.0-alpha.1. **Byte-identical to Perl `bam2nuc` v0.25.1** for both output files
(`*.nucleotide_stats.txt` and `genomic_nucleotide_frequencies.txt`), verified by a
local Perl-oracle golden suite (`tests/golden.rs` + `tests/data/generate_goldens.sh`)
and an oxy real-data gate (`scripts/bam2nuc_byte_identity.sh`).

## Usage

```
bam2nuc_rs -g <genome_dir> [--dir <out_dir>] <input.bam> [more.bam ...]
bam2nuc_rs -g <genome_dir> --genomic_composition_only
```

| Option | Meaning |
|---|---|
| `-g`, `--genome_folder <PATH>` | Genome FASTA directory (mandatory). `.fa`/`.fasta`, optionally gzipped. |
| `--dir <PATH>` | Output directory (default: current directory). |
| `--genomic_composition_only` | Compute + write the genome composition cache, then exit. |
| `--parent_dir <PATH>` | Accepted for Perl compatibility (the Rust port does not `chdir`). |
| `--samtools_path <PATH>` | Accepted but **ignored** — reading is pure-Rust (no samtools subprocess). |
| `-V`, `--version` | Print version. |

The output filename is derived from each input's basename (`sample.bam` →
`sample.nucleotide_stats.txt`). The whole-genome composition is cached as
`genomic_nucleotide_frequencies.txt` in the genome folder (or, if that isn't
writable, the output directory) and reused on subsequent runs.

## Differences from the Perl `bam2nuc`

These are intentional and do not affect the byte-identity of the output files on
real Bismark data:

- **No samtools subprocess** — BAM is read with pure-Rust `noodles`. `--samtools_path`
  is accepted but ignored (and, unlike Perl, not validated for existence).
- **Input is BAM only in v1.0.** `.sam` is rejected (Perl can read it but then dies
  deriving the output name); `.cram` is rejected (deferred to a later release).
- **`--version`** prints a one-line provenance string, not Perl's banner.
- Progress messages go to stderr and are not byte-reproduced (only the output files
  are the contract).
- Two latent Perl behaviours are **replicated on purpose** for byte-identity: in
  paired-end mode any FLAG other than 99/147 is reverse-complemented (Perl's
  `elsif ($flag == 83 or 163)` is always true), and the output-name match on a
  trailing `bam`/`cram` is case-sensitive with no dot anchor.
- A degenerate input (no usable reads, or a genome missing a nucleotide) makes both
  Perl and this port stop with a division-by-zero error (this port exits 1).

## Regenerating the test goldens

The goldens are produced from the real Perl `bam2nuc` + samtools:

```
bash tests/data/generate_goldens.sh   # needs Perl + samtools on PATH
cargo test -p bismark-bam2nuc          # runs hermetically against the committed goldens
```
