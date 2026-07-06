//! Binary entry point for `NOMe_filtering` — thin wrapper over
//! [`bismark_nome_filtering::run_main`] (shared with the `bismark` meta-crate's
//! bin so `cargo install bismark` and `cargo install bismark-nome-filtering`
//! behave identically).
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`bismark_nome_filtering::error::BismarkNomeError`]
//! - `2` — clap parse error (clap convention for usage errors)

use std::process::ExitCode;

fn main() -> ExitCode {
    bismark_nome_filtering::run_main()
}
