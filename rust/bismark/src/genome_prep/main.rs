//! Binary entry point for `bismark_genome_preparation` — thin wrapper over
//! [`bismark::genome_prep::run_main`] (shared with the `bismark`
//! meta-crate's bin so `cargo install bismark` and
//! `cargo install bismark-genome-preparation` behave identically).
//!
//! Exit codes: `0` success · `1` any [`bismark::genome_prep::GenomePrepError`] ·
//! `2` clap parse error.

use std::process::ExitCode;

fn main() -> ExitCode {
    bismark::genome_prep::run_main()
}
