//! Output writers: the sorted bedGraph (`.gz`), the coverage file
//! (`.bismark.cov.gz`), and the optional plain `.zero.cov`. Mirrors Perl
//! `bismark2bedGraph` `generate_output` (`:590-618`) and the header/line
//! layouts (`:116`, `:406`/`:607`, `:409`/`:610`, `:413`/`:614`).
//!
//! Byte-identity is at the **decompressed-content** level (SPEC §1.1 D1):
//! the two large gzip streams (bedGraph + coverage) are written with `gzp`
//! **parallel block-gzip** — the runtime bottleneck is DEFLATE (~70% per the
//! PR #893 flamegraph), and Perl wins by compressing in a parallel `gzip`
//! subprocess; gzp matches that with in-process worker threads. Under Cargo
//! feature unification with our `flate2/zlib-rs` feature, gzp's flate2 codec
//! resolves to the zlib-rs backend (parallel zlib-rs). The emitted gzip
//! stream decompresses to identical bytes, so D1 holds. The plain `.zero.cov`
//! is uncompressed (Perl `:132`).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use flate2::Compression;
use gzp::ZWriter;
use gzp::deflate::Gzip;
use gzp::par::compress::{ParCompress, ParCompressBuilder};

use crate::aggregate::{Aggregator, ChrPositions};
use crate::cli::ResolvedConfig;
use crate::error::BismarkBedgraphError;
use crate::fmt_g::format_g15;

/// gzip compression level — matches Perl's `gzip -c` default (6) and the
/// prior single-threaded writer. Parallelism does not change output size.
const GZIP_LEVEL: u32 = 6;

/// A bedGraph/coverage writer: small `writeln!`s are batched by a `BufWriter`
/// in front of the parallel gzip encoder.
type GzWriter = BufWriter<ParCompress<Gzip>>;

/// Build a `gzp` parallel-gzip writer at [`GZIP_LEVEL`] with `threads`
/// workers, fronted by a 64-KiB `BufWriter` to coalesce the per-row writes.
fn open_par_gz(path: PathBuf, threads: usize) -> Result<GzWriter, BismarkBedgraphError> {
    let file = File::create(path)?;
    let par = ParCompressBuilder::<Gzip>::new()
        .num_threads(threads)
        .map_err(std::io::Error::other)?
        .compression_level(Compression::new(GZIP_LEVEL))
        .from_writer(file);
    Ok(BufWriter::with_capacity(64 * 1024, par))
}

/// Flush a [`GzWriter`] and finalize the gzip stream (flush remaining blocks,
/// join the worker threads, write the gzip trailer), surfacing any I/O error as
/// `BismarkBedgraphError::Io` instead of panicking (#939). Must be called —
/// drop alone does not finalize a `ParCompress`.
///
/// `flush()` pushes the buffered bytes into the `ParCompress` (gzp `flush` is
/// `flush_last(false)` — no footer); then `get_mut().finish()` writes the footer
/// and joins the workers. On the **error** path gzp 0.11.3's `finish()` returns
/// from `flush_last(true)?` BEFORE it `take()`s its channels/handle, so the
/// `ParCompress`'s `Drop` would re-run `finish().unwrap()` → PANIC
/// (`par/compress.rs:312`). gzp exposes no way to disarm that `Drop`, so we
/// `mem::forget` the writer on error to suppress it — leaking an already-dead
/// worker thread on the catastrophic-I/O abort path (e.g. ENOSPC) where the run
/// is failing anyway; the success path drops normally. We use `flush()` +
/// `get_mut().finish()` rather than `into_inner()` because a failed
/// `into_inner()` strands the un-finished `ParCompress` in the `IntoInnerError`,
/// whose drop re-introduces the same panic.
///
/// Mirrors the sibling fix `bismark-extractor::output::SplitWriter::Gzip::finish`
/// (regression-tested there by `split_writer_gzip_finish_surfaces_error_not_panic`).
fn finish_gz(mut w: GzWriter) -> Result<(), BismarkBedgraphError> {
    let flush_res = w.flush();
    let finish_res = w.get_mut().finish().map_err(std::io::Error::other);
    let result: std::io::Result<()> = flush_res.and(finish_res);
    if result.is_err() {
        std::mem::forget(w);
    }
    result.map_err(BismarkBedgraphError::Io)
}

/// Write the bedGraph + coverage (+ optional zero) outputs from a populated
/// aggregator. Thin wrapper over [`write_outputs_from_sorted`]; the
/// file-reading path ([`crate::run`]) uses this.
pub fn write_outputs(cfg: &ResolvedConfig, agg: Aggregator) -> Result<(), BismarkBedgraphError> {
    let sorted = agg.into_sorted();
    write_outputs_from_sorted(cfg, &sorted)
}

/// Write the bedGraph + coverage (+ optional zero) outputs from already-sorted
/// chromosome data. Positions whose total coverage is below `cfg.cutoff` are
/// dropped (Perl `:399`/`:601`) — applied **here**, so the `.cov.gz` is the
/// authoritative post-cutoff set (SPEC R3).
///
/// Exposed for the extractor's in-process streaming path: it produces `sorted`
/// once via [`Aggregator::into_sorted`] and reuses it to write these files
/// without re-reading the per-context call files. (c2c is then fed from the
/// `.cov.gz` written here — SPEC D4 Phase 3a.)
pub fn write_outputs_from_sorted(
    cfg: &ResolvedConfig,
    sorted: &[ChrPositions],
) -> Result<(), BismarkBedgraphError> {
    // Per-stream compression workers. Two large streams each get their own
    // pool; cap modestly so CI / small hosts don't oversubscribe.
    let threads = std::thread::available_parallelism()
        .map(|n| n.get().min(4))
        .unwrap_or(1);

    let mut bedgraph = open_par_gz(cfg.output_dir.join(&cfg.bedgraph_name), threads)?;
    // Header line — always written (Perl `:116`).
    writeln!(bedgraph, "track type=bedGraph")?;

    let mut coverage = open_par_gz(cfg.output_dir.join(&cfg.coverage_name), threads)?;

    // Zero-based coverage is plain text (NOT gzipped) — Perl `:132`.
    let mut zero = if cfg.zero_based {
        Some(BufWriter::new(File::create(
            cfg.output_dir.join(&cfg.zero_name),
        )?))
    } else {
        None
    };

    for (chr, positions) in sorted {
        for &(pos, meth, unmeth) in positions {
            let total = meth + unmeth;
            if total < cfg.cutoff {
                continue;
            }
            // Same string for every file (Perl reuses `$meth_percentage`).
            let pct = format_g15(meth as f64 / total as f64 * 100.0);
            let start = pos - 1; // bedGraph start is 0-based (pos ≥ 1 enforced at parse).

            // bedGraph: chr, 0-based start, 1-based end, pct.
            writeln!(bedgraph, "{chr}\t{start}\t{pos}\t{pct}")?;
            // coverage: chr, 1-based start = end, pct, meth, unmeth.
            writeln!(coverage, "{chr}\t{pos}\t{pos}\t{pct}\t{meth}\t{unmeth}")?;
            // zero: chr, 0-based start, 1-based end, pct, meth, unmeth.
            if let Some(z) = zero.as_mut() {
                writeln!(z, "{chr}\t{start}\t{pos}\t{pct}\t{meth}\t{unmeth}")?;
            }
        }
    }

    // Finalize the parallel gzip streams (order matters: flush BufWriter →
    // finish gzp), then flush the plain zero file.
    finish_gz(bedgraph)?;
    finish_gz(coverage)?;
    if let Some(mut z) = zero {
        z.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sink that fails every write/flush. `ParCompress` type-erases its sink
    /// (a worker thread owns it), so a `GzWriter` can wrap one — letting us
    /// prove `finish_gz` returns `Err` (gzp surfaces the sink error as
    /// `GzpError::Io`) instead of panicking via `ParCompress`'s `Drop`-time
    /// `finish().unwrap()` (#939). Sibling of `bismark-extractor`'s
    /// `split_writer_gzip_finish_surfaces_error_not_panic`.
    struct FailingWriter;
    impl Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("sink write failed"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::other("sink flush failed"))
        }
    }

    #[test]
    fn finish_gz_surfaces_error_not_panic() {
        let par = ParCompressBuilder::<Gzip>::new()
            .num_threads(1)
            .expect("1 is nonzero")
            .compression_level(Compression::new(GZIP_LEVEL))
            .from_writer(FailingWriter);
        let mut w: GzWriter = BufWriter::with_capacity(64 * 1024, par);
        // Buffer a bedGraph-shaped row so the gzip worker has data to push to
        // the failing sink.
        let _ = w.write_all(b"chr1\t0\t1\t100.0\n");
        // #939: must return Err (gzp surfaces the sink error), NOT panic via
        // ParCompress's Drop-time finish().unwrap().
        assert!(
            finish_gz(w).is_err(),
            "finish_gz over a failing sink must return Err, not panic"
        );
    }
}
