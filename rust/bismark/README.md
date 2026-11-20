# bismark

The **Bismark** bisulfite-sequencing suite (Rust) — install every tool with one command:

```bash
cargo install bismark
```

This installs `bismark` and the eleven classic tool names into `~/.cargo/bin`. They are all the same program, so a
tool runs either as `bismark <subcommand>` (e.g. `bismark align`, `bismark extract`, `bismark prepare`) or under its
classic name: the `bismark` aligner, `deduplicate_bismark`, `bismark_methylation_extractor`, `bismark2bedGraph`,
`coverage2cytosine`, `bismark_genome_preparation`, `bam2nuc`, `NOMe_filtering`, `filter_non_conversion`,
`methylation_consistency`, `bismark2report` and `bismark2summary`. Output is byte-identical to Perl Bismark v0.25.1
on the faithful default path, apart from the version string.

Everything ships in this one crate. The older per-tool crates (`bismark-aligner`, …) are no longer updated. The
library modules exist for the suite's own binaries and tests and are not a stable API.

**Optional features:** the release binaries and the container image are built with the in-process rammap backend
for `--rammap` and with BINSEQ (`.vbq`/`.cbq`) input. A plain `cargo install bismark` leaves both out, so `--rammap`
runs an external `rammap` binary and BINSEQ input is rejected. To include them:

```bash
cargo install bismark --features rammap-inprocess,binseq-input
```

**External tools:** only alignment and genome preparation call an external program on your `PATH`:
**Bowtie 2** (default), or optionally **HISAT2** or **minimap2**. `--taps`, `--illumina_5base --bwamem3` and
`bismark prepare --bwamem3` also need **bwa-mem3** 0.16.0 or later (CI and the container use 0.18.0). The bioconda
package (from 3.3.0) and the container include it; with `cargo install`, add it with
`mamba install -c bioconda -c conda-forge bwa-mem3`. All BAM/SAM/CRAM I/O is pure-Rust (`noodles`), so
**no samtools** is needed.

Documentation, including the prebuilt binaries, the container image and the bioconda package:
<https://felixkrueger.github.io/Bismark/>. Source: <https://github.com/FelixKrueger/Bismark>.
