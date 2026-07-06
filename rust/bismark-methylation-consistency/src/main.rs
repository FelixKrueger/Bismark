//! Binary entry point for `methylation_consistency` — thin wrapper over
//! [`bismark_methylation_consistency::run_main`] (shared with the `bismark`
//! meta-crate's bin so `cargo install bismark` and `cargo install
//! bismark-methylation-consistency` behave identically).
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`bismark_methylation_consistency::error::MethConsError`]
//! - `2` — clap parse error (clap convention for usage errors)

use std::process::ExitCode;

fn main() -> ExitCode {
    bismark_methylation_consistency::run_main()
}
