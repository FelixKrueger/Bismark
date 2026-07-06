//! # Bismark — the whole bisulfite-sequencing suite (Rust), one install
//!
//! `cargo install bismark` builds and installs **every** Bismark Rust tool:
//! the `bismark` aligner plus `deduplicate_bismark`, `bismark_methylation_extractor`,
//! `bismark2bedGraph`, `coverage2cytosine`, `bismark_genome_preparation`, `bam2nuc`,
//! `NOMe_filtering`, `filter_non_conversion`, `methylation_consistency`,
//! `bismark2report`, and `bismark2summary`.
//!
//! This crate has **no library API** — it is a batteries-included installer. Each
//! binary is a thin wrapper over the corresponding tool crate's `run_main()` and is
//! byte-identical to installing that crate on its own (e.g. `cargo install
//! bismark-aligner`). To install a single tool, install its individual crate.
//!
//! The aligner and genome-preparation steps shell out to an external aligner on
//! `PATH` (Bowtie 2 by default; optionally HISAT2 or minimap2); all BAM/SAM/CRAM
//! I/O is pure-Rust (`noodles`) — no samtools required.
#![forbid(unsafe_code)]
