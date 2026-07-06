//! `methylation_consistency` — one of the binaries installed by `cargo install bismark`.
//! Thin wrapper over [`bismark_methylation_consistency::run_main`]; byte-identical to that tool's own
//! binary (each sets the same multithreaded allocator).
use std::process::ExitCode;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    bismark_methylation_consistency::run_main()
}
