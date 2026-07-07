//! # Bismark — the whole bisulfite-sequencing suite (Rust): one crate, one binary.
//!
//! Every tool is a module here (`bismark::bam2nuc`, `bismark::dedup`, …), folded
//! from the former 14 crates (epic `plans/07062026_single-binary-suite/`). The
//! multicall CLI + classic-name aliases are wired in Phase 3; during Phase 2 the
//! 12 classic `[[bin]]` wrappers call `bismark::<module>::run_main()`.
//!
//! NOTE: crate-level lint attributes are intentionally NOT set here. Each tool
//! module carries its own `#![forbid(unsafe_code)]` / `#![warn(missing_docs)]`
//! inner attributes where it had them as a crate (the aligner / genome_prep /
//! report crates did not). Do NOT hoist those to the crate root — it would
//! over-apply `forbid(unsafe_code)` to modules that need `unsafe`.

pub mod aligner;
pub mod bam2nuc;
pub mod bedgraph;
pub mod cli;
pub mod coverage2cytosine;
pub mod dedup;
pub mod extractor;
pub mod filter_nonconversion;
pub mod genome_prep;
pub mod io;
pub mod meta;
pub mod methylation_consistency;
pub mod nome_filtering;
pub mod report;
pub mod summary;
