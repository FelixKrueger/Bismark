//! Binary entry point for `bam2nuc` — thin wrapper over
//! [`bismark_bam2nuc::run_main`] (shared with the `bismark` meta-crate's bin so
//! `cargo install bismark` and `cargo install bismark-bam2nuc` behave identically).
//!
//! Exit codes: `0` success · `1` any [`bismark_bam2nuc::BismarkBam2nucError`] ·
//! `2` clap parse error.

use std::process::ExitCode;

// Multithreaded allocator (#884/#915 sibling precedent). Allocator-only — the
// per-read counting loop allocates a span Vec per read; mimalloc trims the
// malloc cost. Output is byte-identical. Kept in the binary crate root (each
// binary — this one and the meta-crate's — sets its own).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    bismark_bam2nuc::run_main()
}
