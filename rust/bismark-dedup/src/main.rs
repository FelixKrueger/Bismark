//! Binary entry point for `deduplicate_bismark_rs`.
//!
//! Phase A scaffolding: parses `--version` and prints the provenance string.
//! Any other invocation prints a "not yet implemented" notice and exits with a
//! non-zero status. Actual dedup logic lands in Phases B through F per
//! `~/.claude/plans/05242026_bismark-dedup-v1/PLAN.md`.

use std::process::ExitCode;

use bismark_dedup::version_string;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", version_string());
        return ExitCode::SUCCESS;
    }

    eprintln!(
        "bismark-dedup: not yet implemented (Phase A scaffolding only). \
         See https://github.com/FelixKrueger/Bismark/issues/794 for status."
    );
    ExitCode::from(64)
}
