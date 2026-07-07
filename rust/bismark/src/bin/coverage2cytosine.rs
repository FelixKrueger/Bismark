//! `coverage2cytosine` — one of the binaries installed by `cargo install bismark`.
//! Thin wrapper over [`bismark::coverage2cytosine::run_main`]; byte-identical to that tool's own
//! binary (each sets the same multithreaded allocator).
use std::process::ExitCode;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    bismark::coverage2cytosine::run_main()
}
