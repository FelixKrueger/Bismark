# bismark

The **Bismark** bisulfite-sequencing suite (Rust) — install every tool with one command:

```bash
cargo install bismark
```

This builds one `bismark` binary that runs every tool, as `bismark <subcommand>` (e.g. `bismark align`,
`bismark extract`) or under the classic tool names: the `bismark` aligner, `deduplicate_bismark`,
`bismark_methylation_extractor`, `bismark2bedGraph`, `coverage2cytosine`, `bismark_genome_preparation`,
`bam2nuc`, `NOMe_filtering`, `filter_non_conversion`, `methylation_consistency`, `bismark2report` and
`bismark2summary`. On the default Bowtie 2 path the output is byte-identical to Perl Bismark v0.25.1, apart from
the version string.

Everything ships in this one crate. The older per-tool crates (`bismark-aligner`, …) are no longer updated. The
library modules exist for the suite's own binaries and tests and are not a stable API.

**External tools:** only alignment and genome preparation call an external program on your `PATH`:
**Bowtie 2** (default), or optionally **HISAT2** or **minimap2**. `--taps`, `--illumina_5base --bwamem3` and
`bismark_genome_preparation --bwamem3` also need **bwa-mem3** 0.16.0 or later (0.18.0 is the pinned version):
`mamba install -c bioconda bwa-mem3`. All BAM/SAM/CRAM I/O is pure-Rust (`noodles`), so **no samtools** is needed.

The prebuilt binaries, the container image and the bioconda package are described in the
[installation guide](https://felixkrueger.github.io/Bismark/installation/); documentation is at
<https://github.com/FelixKrueger/Bismark>.
