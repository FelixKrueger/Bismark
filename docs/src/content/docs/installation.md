---
title: "Installation"
description: "Bismark is the Rust bisulfite aligner and methylation suite — a single binary, byte-identical to Perl v0.25.1. Install via conda/bioconda, cargo, a container image, or prebuilt binaries."
---

Bismark is the Rust bisulfite aligner and methylation suite, executed from the command line. It ships as a **single `bismark` binary**: run `bismark <subcommand>` (e.g. `bismark align`, `bismark extract`) or use the classic tool names (`deduplicate_bismark`, `bismark_methylation_extractor`, …), which are supported aliases of the same binary. Output is **byte-identical** to Perl Bismark `v0.25.1` on the faithful default path. The original Perl scripts are now archived as tagged legacy (see [Legacy: the Perl Bismark](#legacy-the-perl-bismark) below).

## The Bismark Rust suite

There are four ways to install the Rust suite.

### Install with `conda` / `mamba` (bioconda)

The simplest option — one command installs the `bismark` suite **and** its alignment backends (Bowtie 2, HISAT2, minimap2), so nothing else needs to be on your `PATH`:

```bash
conda install -c bioconda -c conda-forge bismark
# or, faster:
mamba install -c bioconda -c conda-forge bismark
```

You get the single `bismark` binary plus the classic tool-name aliases, with **no `samtools`** dependency (BAM/SAM/CRAM I/O is pure-Rust). To install the legacy Perl implementation instead, pin `bismark=0.25.1`.

### Install from source with `cargo`

Installs the single `bismark` binary (all tools, via subcommands + classic-name aliases) into `~/.cargo/bin` in one command (requires a Rust toolchain — see Prerequisites):

```bash
cargo install bismark
```

For the latest development build instead of the published release:

```bash
cargo install --git https://github.com/FelixKrueger/Bismark --branch master --locked bismark
```

**Updating:** re-run the `--branch` command and cargo picks up the newest commit automatically; re-running `cargo install bismark` is a no-op unless a newer version is published — add `--force` to reinstall in place.

**Prerequisites (source install):** a Rust toolchain (latest stable recommended; minimum supported Rust 1.89); a working C linker; and the alignment backend(s) on your `PATH` — **Bowtie 2** (+ `bowtie2-build`), or optionally **HISAT2** (+ `hisat2-build`) or **minimap2**. No `samtools` is required (BAM/SAM I/O is pure-Rust). Make sure `~/.cargo/bin` is on your `PATH`.

### Prebuilt binaries (no toolchain)

Each [release](https://github.com/FelixKrueger/Bismark/releases) attaches prebuilt binaries for common Linux/macOS platforms — download the archive for your platform, extract it, and put the contents on your `PATH`. The archive ships the single `bismark` binary plus the classic tool names as symlinks to it (`deduplicate_bismark`, `bismark_methylation_extractor`, …), so it is a drop-in for existing pipelines.

### Container image

A multi-arch image is published to the GitHub Container Registry, exposing the tools under their **canonical** names — so it is a drop-in for pipelines such as nf-core/methylseq:

```bash
docker pull ghcr.io/felixkrueger/bismark:latest        # latest release
docker pull ghcr.io/felixkrueger/bismark:3.0.0         # pinned
```

The Rust aligner also adds an opt-in, lower-memory [combined-index alignment mode](/Bismark/usage/alignment/) (one combined C→T + G→A index instead of separate per-strand instances) — see the Alignment page.

## Dependencies

Bismark requires [Bowtie 2](http://bowtie-bio.sourceforge.net/bowtie2) (or [HISAT2](https://ccb.jhu.edu/software/hisat2/index.shtml)) to be installed on your machine. Bismark will assume that the Bowtie 2/ HISAT2 executable is in your path unless the path to Bowtie/ HISAT2 is specified manually with:

```
--path_to_bowtie2 </../../bowtie2> or
--path_to_hisat2 </../../hisat2>
```

## Legacy: the Perl Bismark

Bismark began as a suite of Perl scripts. From the Rust general release the Perl implementation is in maintenance freeze (critical correctness and security fixes only) and is archived as tagged legacy on GitHub, following the precedent of Salmon's `cpp` branch. Because the Rust suite is byte-identical to Perl `v0.25.1` on the faithful default path, it is a drop-in replacement — existing pipelines need no change. If you specifically need the Perl scripts, check out the corresponding legacy tag from the [repository](https://github.com/FelixKrueger/Bismark).

## Hardware requirements

Bismark holds the reference genome in memory, and in addition to that runs up to four parallel instances of Bowtie 2. The memory usage is dependent on the size of the reference genome. For a large eukaryotic genome (human or mouse) we experienced a typical memory usage of around 12GB. We thus recommend running Bismark on a machine with 5 CPU cores and at least 12 GB of RAM. The memory requirements of Bowtie 2 are somewhat larger (possibly to allow gapped alignments). When running Bismark using Bowtie 2 we therefore recommend a system with at least 5 cores and > 16GB of RAM.

Alignment speed depends largely on the read length and alignment parameters used. Allowing many mismatches and using a short seed length tends to be fairly slow.

## BS-Seq test data set

A test BS-Seq data set is available for download from the Bismark project or Github pages. It contains 10,000 single- end shotgun BS reads from human ES cells in FastQ format (from SRR020138, Lister et al., 2009; trimmed to 50 bp; base call qualities are Sanger encoded Phred values (Phred33)).

### Bismark reports for the test data set

Please note that this has been run with a fairly early version however I wouldn't expect the numbers to change much.

#### Using Bowtie 2:

Running Bismark with the following options:

```bash
bismark --score-min L,0,-0.6 /data/public/Genomes/Human/GRCh37/ test_data.fastq
```

Should result in this mapping report:

```
Bismark report for: test_data.fastq (version: v0.7.8)
Option '--directional' specified: alignments to complementary strands will be ignored (i.e. not performed!)
Bowtie2 was run against the bisulfite genome of /data/public/Genomes/Human/GRCh37/ with the specified options: -q -- score-min L,0,-0.6 --ignore-quals

Final Alignment report
======================
Sequences analysed in total: 10000

Number of alignments with a unique best hit from the different alignments: 5658 Mapping efficiency: 56.6%
Sequences with no alignments under any condition: 2893
Sequences did not map uniquely: 1449
Sequences which were discarded because genomic sequence could not be extracted: 0
Number of alignments to (merely theoretical) complementary strands being rejected in total: 0

Number of sequences with unique best (first) alignment came from the bowtie output:

CT/CT: 2820 ((converted) top strand)
CT/GA: 2838 ((converted) bottom strand)
GA/CT: 0    (complementary to (converted) top strand)
GA/GA: 0    (complementary to (converted) bottom strand)

Final Cytosine Methylation Report
=================================
Total number of C's analysed: 45985

Total methylated C's in CpG context: 1550
Total methylated C's in CHG context: 34
Total methylated C's in CHH context: 126

Total C to T conversions in CpG context: 844
Total C to T conversions in CHG context: 11368
Total C to T conversions in CHH context:32063

C methylated in CpG context: 64.7%
C methylated in CHG context: 0.3%
C methylated in CHH context: 0.4%
```
