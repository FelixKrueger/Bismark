//! Binary entry point for `NOMe_filtering`.
//!
//! Parses the CLI via [`bismark_nome_filtering::cli::Cli`], handles `--version`
//! (clap's auto-version is disabled so we can emit a custom provenance string),
//! then dispatches to [`bismark_nome_filtering::run`].
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`bismark_nome_filtering::error::BismarkNomeError`]
//! - `2` — clap parse error (clap convention for usage errors)

use std::process::ExitCode;

use clap::Parser;

use bismark_nome_filtering::cli::Cli;
use bismark_nome_filtering::version_string;

fn main() -> ExitCode {
    let cli = Cli::parse();

    // `--version` / `-V` is handled here (clap auto-version is disabled in
    // src/cli.rs so we can emit our custom provenance string).
    if cli.version {
        println!("{}", version_string());
        return ExitCode::SUCCESS;
    }

    match bismark_nome_filtering::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}
