//! Binary entry point for `coverage2cytosine` — thin wrapper over
//! [`bismark::coverage2cytosine::run_main`] (shared with the `bismark`
//! meta-crate's bin so `cargo install bismark` and
//! `cargo install bismark-coverage2cytosine` behave identically).
//!
//! Exit codes: `0` success · `1` any [`bismark::coverage2cytosine::error::BismarkC2cError`] ·
//! `2` clap parse error (clap convention, emitted by `Cli::parse`).

use std::process::ExitCode;

fn main() -> ExitCode {
    bismark::coverage2cytosine::run_main()
}
