//! Binary entry point for `bismark2summary` — thin wrapper over
//! [`bismark::summary::run_main`] (shared with the `bismark` meta-crate's bin so
//! `cargo install bismark` and `cargo install bismark-summary` behave
//! identically).
//!
//! Exit codes:
//! - `0` — success (also `--version` / `--help` / `--man`)
//! - `1` — any [`bismark::summary::error::BismarkSummaryError`]
//! - `2` — clap parse error (clap convention for usage errors)

use std::process::ExitCode;

fn main() -> ExitCode {
    bismark::summary::run_main()
}
