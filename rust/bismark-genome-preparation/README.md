# bismark-genome-preparation

Rust port of the Perl `bismark_genome_preparation` script (part of the
[Bismark](https://github.com/FelixKrueger/Bismark) Rust rewrite).

Reads a genome directory of FASTA file(s) and writes two in-silico
bisulfite-converted references — a **C→T-converted** (top-strand) copy and a
**G→A-converted** (bottom-strand) copy — under
`<genome>/Bisulfite_Genome/{CT,GA}_conversion/`, then runs an external indexer
(`bowtie2-build` by default, or `hisat2-build` / `minimap2 -d`) on each.

The converted CT/GA FASTA files are **byte-identical** to Perl Bismark v0.25.1's
output (the acceptance gate); the external index build is delegated to the same
indexer and validated for equivalence rather than byte-reproduced.

## Usage

```bash
bismark_genome_preparation_rs [OPTIONS] <GENOME_FOLDER>
```

Key options (Perl-compatible spellings): `--bowtie2` (default) / `--hisat2` /
`--minimap2`, `--path_to_aligner <DIR>`, `--parallel <N>`, `--single_fasta`,
`--large-index`, `--genomic_composition`, `--slam` (deprecated), `--verbose`,
`--version`, `--help`.

`--genomic_composition` writes a genomic mono-/di-nucleotide frequency table
(`<genome>/genomic_nucleotide_frequencies.txt`) before the conversion —
byte-identical to Perl v0.25.1.

Bismark-Rust extension: `--combined_genome` additionally builds a single
combined CT+GA reference + index (opt-in; for a future Rust aligner).

> Full documentation (CHANGELOG, mkdocs page) is tracked in the docs sub-issue.
