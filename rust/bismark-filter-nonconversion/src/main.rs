//! Binary entry point for `filter_non_conversion` — thin wrapper over
//! [`bismark_filter_nonconversion::run_main`] (shared with the `bismark`
//! meta-crate's bin so `cargo install bismark` and `cargo install
//! bismark-filter-nonconversion` behave identically).
//!
//! Exit codes: `0` success; `1` any [`BismarkFilterError`] or no input files;
//! `2` clap parse error (clap convention).
//!
//! `--help` is clap's default (exit 0 — a documented deviation from Perl's
//! `print_helpfile`/`exit 1`, SPEC §10.1); `--version` is handled in
//! `run_main` (clap's auto-version is disabled) and exits 0, matching Perl.

#![forbid(unsafe_code)]

use std::process::ExitCode;

// Multithreaded global allocator (free ~10% win, byte-neutral; same pin as
// the extractor #884 / bedgraph #915).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    bismark_filter_nonconversion::run_main()
}
