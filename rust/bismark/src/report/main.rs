//! Binary entry point for `bismark2report` — thin wrapper over
//! [`bismark::report::run_main`] (shared with the `bismark` meta-crate's bin so
//! `cargo install bismark` and `cargo install bismark-report` behave
//! identically).
//!
//! Exit codes: `0` success · `1` any [`ReportError`] · `2` clap parse error.
//! `--help`/`--man`/`--version` all exit `0` (Perl's exit-1-on-help quirk is
//! intentionally NOT reproduced — PLAN §6.1).

use std::process::ExitCode;

fn main() -> ExitCode {
    bismark::report::run_main()
}
