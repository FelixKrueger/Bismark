//! Binary entry point for `bismark_methylation_extractor` — thin wrapper over
//! [`bismark_extractor::run_main`] (shared with the `bismark` meta-crate's bin
//! so `cargo install bismark` and `cargo install bismark-extractor` behave
//! identically).
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`bismark_extractor::BismarkExtractorError`]
//! - `2` — clap parse error (clap convention)

use std::process::ExitCode;

// Multithreaded global allocator (#884). The parallel pipeline's worker threads
// allocate heavily per record (record parsing, call Vecs, batch Vecs); under the
// default system allocator they blocked on arena locks, making `--parallel N>1`
// run ~2x SLOWER than N=1 (`top` showed only ~364% CPU at `--parallel 8` — i.e.
// blocking-bound, not CPU-bound). mimalloc removes that contention: default N=4
// dropped 155.8s -> 23.5s and the anti-scaling vanished. Allocator choice does
// not affect computed output — byte-identity to the system allocator holds
// (guarded by the `parallel_phase_f` N≡1 tests + the Phase H matrix). Kept in
// the binary crate root (each binary — this one and the meta-crate's — sets its
// own).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    bismark_extractor::run_main()
}
