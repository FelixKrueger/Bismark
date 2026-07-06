//! Binary entry point for `deduplicate_bismark` — thin wrapper over
//! [`bismark_dedup::run_main`] (shared with the `bismark` meta-crate's bin so
//! `cargo install bismark` and `cargo install bismark-dedup` behave
//! identically).
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`bismark_dedup::BismarkDedupError`]
//! - `2` — clap parse error (clap convention for usage errors)

use std::process::ExitCode;

fn main() -> ExitCode {
    bismark_dedup::run_main()
}
