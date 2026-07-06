//! Binary entry point for `bismark` — thin wrapper over
//! [`bismark_aligner::run_main`] (shared with the `bismark` meta-crate's bin so
//! `cargo install bismark` and `cargo install bismark-aligner` behave identically).
//!
//! Exit codes: `0` success · `1` any [`bismark_aligner::AlignerError`] ·
//! `2` clap parse error.

use std::process::ExitCode;

// Multithreaded global allocator (Apple Silicon perf epic, 06222026). Relieves
// system-allocator arena-lock contention on the aligner's per-record String/Vec
// churn (bowtie2-output parse, conversion loop, methylation/tag path). Allocator
// -only: output is byte-identical (guarded by tests/byte_identity_real_data.rs +
// `just reproduce`). Kept in the binary crate root (each binary — this one and
// the meta-crate's — sets its own).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    bismark_aligner::run_main()
}
