---
title: "Installation"
description: "Bismark is written in Perl and is executed from the command line. To install Bismark simply copy the bismarkv0.X.Y.tar.gz file into a Bismark installation…"
---

Bismark is written in Perl and is executed from the command line. To install Bismark simply copy the bismark_v0.X.Y.tar.gz file into a Bismark installation folder and extract all files by typing:

```bash
tar xzf bismark_v0.X.Y.tar.gz
```

## Bismark Rust suite (beta)

A faster, lower-memory reimplementation of the Bismark tools in Rust is available as a public beta, producing **byte-identical** output to Perl Bismark `v0.25.1`. There are three ways to install it.

### Prebuilt binaries (no toolchain)

Each [release](https://github.com/FelixKrueger/Bismark/releases) attaches prebuilt binaries for common Linux/macOS platforms — download the archive for your platform, extract it, and put the binaries on your `PATH`. The Rust tools carry an `_rs` suffix (e.g. `bismark_rs`, `deduplicate_bismark_rs`) so they sit alongside the Perl scripts on the same `PATH` without clashing.

### Container image

A multi-arch image is published to the GitHub Container Registry, exposing the tools under their **canonical** names — so it is a drop-in for pipelines such as nf-core/methylseq:

```bash
docker pull ghcr.io/felixkrueger/bismark:beta          # latest beta
docker pull ghcr.io/felixkrueger/bismark:2.0.0-beta.7  # pinned
```

### Install from source with `cargo`

Installs all 12 binaries into `~/.cargo/bin` in a single command (requires a Rust toolchain — see Prerequisites):

```bash
cargo install --git https://github.com/FelixKrueger/Bismark \
  --tag bismark-rust-v2.0.0-beta.7 --locked \
  bismark-genome-preparation bismark-aligner bismark-dedup bismark-extractor \
  bismark-bedgraph bismark-coverage2cytosine bismark-methylation-consistency \
  bismark-nome-filtering bismark-filter-nonconversion bismark-bam2nuc \
  bismark-report bismark-summary
```

For the latest development build instead of a pinned release, replace `--tag bismark-rust-v2.0.0-beta.7` with `--branch rust/iron-chancellor`. **Updating:** re-run the `--branch` command and cargo picks up the newest commit automatically; to move between pinned releases, bump the `--tag` (re-running the same `--tag` is a no-op — add `--force` to reinstall in place).

**Prerequisites (source install):** a Rust toolchain (latest stable recommended; minimum supported Rust 1.89, the one-command install verified on cargo 1.95); a working C linker; and the alignment backend(s) on your `PATH` — **Bowtie 2** (+ `bowtie2-build`), or optionally **HISAT2** (+ `hisat2-build`) or **minimap2**. No `samtools` is required (BAM/SAM I/O is pure-Rust). Make sure `~/.cargo/bin` is on your `PATH`.

> The `_rs` suffix on host installs lets the Rust binaries coexist with the Perl Bismark scripts; inside the container they are exposed under the canonical names. At the v1.0 release the `_rs` suffix is dropped and the Rust binaries become the defaults.

The Rust aligner also adds an opt-in, lower-memory [combined-index alignment mode](/Bismark/usage/alignment/) (one combined C→T + G→A index instead of separate per-strand instances) — see the Alignment page.

## Dependencies

Bismark requires a working of Perl and [Bowtie 2](http://bowtie-bio.sourceforge.net/bowtie2) (or [HISAT2](https://ccb.jhu.edu/software/hisat2/index.shtml)) to be installed on your machine. Bismark will assume that the Bowtie 2/ HISAT2 executable is in your path unless the path to Bowtie/ HISAT2 is specified manually with:

```
--path_to_bowtie2 </../../bowtie2> or
--path_to_hisat2 </../../hisat2>
```

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
