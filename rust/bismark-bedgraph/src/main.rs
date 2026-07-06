//! Binary entry point for `bismark2bedGraph` — thin wrapper over
//! [`bismark_bedgraph::run_main`] (shared with the `bismark` meta-crate's bin
//! so `cargo install bismark` and `cargo install bismark-bedgraph` behave
//! identically).
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`bismark_bedgraph::error::BismarkBedgraphError`]
//! - `2` — clap parse error (clap convention for usage errors)

use std::process::ExitCode;

// Multithreaded global allocator. The parallel per-file parse (`parallel.rs`)
// is hashmap-insert-bound — i.e. allocation-heavy (per-thread map growth).
// Under the default system allocator, concurrent maps block on shared arena
// locks, so `--parallel N>1` ran SLOWER than N=1 on a full `--CX` gate (system
// allocator: p1 973s, p3 1790s, p6 1508s — anti-scaling). mimalloc's
// per-thread heaps remove that contention — the same fix that eliminated the
// extractor's parallel anti-scaling (#884, `8a2a147`). Allocator choice does
// not affect computed output; decompressed-content byte-identity (SPEC D1) is
// unchanged (guarded by the N-invariance tests + the real-data gate). Kept in
// the binary crate root (each binary — this one and the meta-crate's — sets
// its own).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    bismark_bedgraph::run_main()
}
