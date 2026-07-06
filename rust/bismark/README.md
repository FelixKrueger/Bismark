# bismark

The **Bismark** bisulfite-sequencing suite (Rust) — install every tool with one command:

```bash
cargo install bismark
```

This installs all suite binaries: the `bismark` aligner, `deduplicate_bismark`,
`bismark_methylation_extractor`, `bismark2bedGraph`, `coverage2cytosine`,
`bismark_genome_preparation`, `bam2nuc`, `NOMe_filtering`, `filter_non_conversion`,
`methylation_consistency`, `bismark2report`, and `bismark2summary`.

`bismark` is a meta / batteries-included crate — it has no library API; each binary
is a thin wrapper over the corresponding tool crate (byte-identical to installing
that crate directly). To install a single tool instead, install its crate, e.g.
`cargo install bismark-aligner`.

**External tools:** the aligner and genome-preparation steps shell out to an
aligner on your `PATH` — **Bowtie 2** (default) or optionally **HISAT2** / **minimap2**.
All BAM/SAM/CRAM I/O is pure-Rust (`noodles`) — **no samtools needed**.

See <https://github.com/FelixKrueger/Bismark> for documentation.
