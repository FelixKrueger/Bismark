# Bismark

[![CI](https://github.com/FelixKrueger/Bismark/actions/workflows/rust_ci.yml/badge.svg?branch=master)](https://github.com/FelixKrueger/Bismark/actions/workflows/rust_ci.yml)
[![Crates.io](https://img.shields.io/crates/v/bismark)](https://crates.io/crates/bismark)
[![install with bioconda](https://img.shields.io/badge/install%20with-bioconda-brightgreen.svg?style=flat)](http://bioconda.github.io/recipes/bismark/README.html)

> [!IMPORTANT]
> **Bismark is now the Rust suite, generally available.** The bisulfite aligner and
> methylation tools have been rewritten from Perl to Rust: **byte-identical** to Perl `v0.25.1` on the
> faithful default path, faster, and lower-memory — and this is the **supported default**. Get it via
> [Installation](#installation) (`mamba install -c bioconda bismark`, cargo, container, or prebuilt binaries) and see the
> **[Rust suite overview](https://felixkrueger.github.io/Bismark/rust/overview/)**.
> The original **Perl `v0.25.x`** (the scripts at this repo root) is now **legacy / maintenance-freeze**
> (critical fixes only; tagged [`v0.25.1`](https://github.com/FelixKrueger/Bismark/releases/tag/v0.25.1)).
> **New contributions should target the Rust suite** — see [CONTRIBUTING.md](CONTRIBUTING.md).

> **See the documentation**: <https://felixkrueger.github.io/Bismark>

Bismark is a program to map bisulfite treated sequencing reads to a genome of interest and perform methylation calls in a single step. The output can be easily imported into a genome viewer, such as [SeqMonk](http://www.bioinformatics.babraham.ac.uk/projects/seqmonk/), and enables a researcher to analyse the methylation levels of their samples straight away. Its main features are:

- Bisulfite mapping and methylation calling in one single step
- Supports single-end and paired-end read alignments
- Supports ungapped, gapped or spliced alignments
- Alignment seed length, number of mismatches etc. are adjustable
- Output discriminates between cytosine methylation in `CpG`, `CHG` and `CHH` context

## Documentation

The Bismark documentation can be found with the code in the [docs](docs) subfolder and can also be read online: <https://felixkrueger.github.io/Bismark/>

There is also an overview of the alignment modes that are currently supported by Bismark: [Bismark alignment modes](http://www.bioinformatics.babraham.ac.uk/projects/bismark/Bismark_alignment_modes.pdf) (pdf).

## Installation

Bismark is now the Rust suite. Pick one:

```bash
# 1. bioconda — also installs the aligners (bowtie2/hisat2/minimap2) for you
mamba install -c bioconda -c conda-forge bismark

# 2. crates.io — installs the whole suite (all tools) into ~/.cargo/bin
cargo install bismark

# 3. Container image (nothing to install; drop-in for nf-core/methylseq)
docker pull ghcr.io/felixkrueger/bismark:latest    # or pin a specific release, e.g. :3.0.0

# 4. Prebuilt binaries — download from the Releases page and put on your PATH
#    https://github.com/FelixKrueger/Bismark/releases
```

**External tools on your `PATH`:** an aligner — [Bowtie2](http://bowtie-bio.sourceforge.net/bowtie2/) (default), or optionally [HISAT2](https://ccb.jhu.edu/software/hisat2/index.shtml) or [minimap2](https://lh3.github.io/minimap2/minimap2.html). **No Samtools needed** — all BAM/SAM/CRAM I/O is pure-Rust. The container bundles the aligners for you. See [`rust/README.md`](rust/README.md#installing) for details and per-tool installs.

> The default bioconda `bismark` package is now the Rust suite (v3.0.0+); `mamba install bismark=0.25.1` still gets the legacy Perl.

### Legacy: the Perl Bismark (v0.25.x)

The original Perl scripts remain at this repo root (maintenance-freeze). To use them, download the [`v0.25.1` release](https://github.com/FelixKrueger/Bismark/releases/tag/v0.25.1) (or `mamba install bismark=0.25.1`); they need [Bowtie2](http://bowtie-bio.sourceforge.net/bowtie2/)/[HISAT2](https://ccb.jhu.edu/software/hisat2/index.shtml)/[minimap2](https://lh3.github.io/minimap2/minimap2.html) **and** [Samtools](http://www.htslib.org/) on the `PATH`.

## Links

- Bismark Publication: http://www.ncbi.nlm.nih.gov/pubmed/21493656
- Our review about primary data analysis in BS-Seq: http://www.ncbi.nlm.nih.gov/pubmed/22290186

## Credits

Bismark was written by Felix Krueger, part of the [Babraham Bioinformatics](http://www.bioinformatics.babraham.ac.uk/projects/bismark/) group.

## Licences

Bismark itself is free software, `bismark report` and `bismark summary` produce HTML graphs powered by [Plot.ly](https://plot.ly/javascript/) which are also free to use and look at!
