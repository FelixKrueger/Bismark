//! `bismark-genome-preparation` — Rust port of the Perl `bismark_genome_preparation`.
//!
//! Reads a genome directory of FASTA file(s) and writes two in-silico
//! bisulfite-converted references — a **C→T-converted** (top-strand) copy and a
//! **G→A-converted** (bottom-strand) copy — under
//! `<genome>/Bisulfite_Genome/{CT,GA}_conversion/`, then runs an external
//! indexer (`bowtie2-build` / `hisat2-build` / `minimap2 -d`) on each.
//!
//! **Acceptance gate:** the converted CT/GA FASTA files are **byte-identical**
//! to Perl Bismark v0.25.1's output. The external index build is a *secondary*
//! check. This crate does **no** BAM I/O (it does not depend on `bismark-io`).
//!
//! Pipeline (mirrors the Perl steps):
//! 1. **Step I** — discover FASTA (extension precedence + lexical order),
//!    validate `--path_to_aligner` if given, create the output tree.
//! 2. **Step II** — bisulfite-convert each sequence into the CT and GA files
//!    (the byte-identity core — see [`convert`]).
//! 3. **Step III** — run the external indexer on each converted reference
//!    (concurrently, mirroring Perl's `fork`).
//! 4. **(opt) Step IV** — `--combined_genome`: also write a single combined
//!    CT+GA reference + build a combined index (Bismark-Rust extension).

pub mod cli;
pub mod combined;
pub mod composition;
pub mod convert;
pub mod discovery;
pub mod error;
pub mod folders;
pub mod indexer;
pub mod logging;
pub mod pipeline;

pub use error::GenomePrepError;
pub use pipeline::run;

/// The Bismark version string this port reproduces in diagnostic banners.
/// It is *not* injected into any FASTA bytes (FASTA carries no version).
pub const BISMARK_VERSION: &str = "v0.25.1";

/// `--version` banner. Reports the SUITE version (via `bismark_meta`, single
/// source `rust/VERSION`); not part of the byte-identity gate.
pub fn version_string() -> String {
    format!(
        "\n          Bismark - Bisulfite Mapper and Methylation Caller.\n\n          \
         Bismark Genome Preparation (Rust port) Version: {}\n        \
         Copyright 2010-25, Felix Krueger, Altos Bioinformatics\n\n               \
         https://github.com/FelixKrueger/Bismark\n",
        bismark_meta::SUITE_VERSION
    )
}
