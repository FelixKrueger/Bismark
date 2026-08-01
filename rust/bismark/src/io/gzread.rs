//! Transparent gzip/BGZF input, parallel-decoded where the input allows it.
//!
//! Every `.gz` **input** in the suite goes through [`open_read`] /
//! [`open_read_with_threads`]. The decoded byte stream is identical to the
//! previous `flate2::read::MultiGzDecoder` path (and to Perl's `gunzip -c`), so
//! the byte-identity gates are untouched; only the decode is now spread across
//! cores.
//!
//! # Dispatch ladder
//!
//! 1. No `.gz` suffix → plain [`BufReader<File>`].
//! 2. `.gz` **and** a regular file **and** the gzip framing validates →
//!    [`rapidgzip_core::DecoderReader`] (parallel).
//! 3. Anything else → `MultiGzDecoder` (sequential).
//!
//! Step 3 is not optional. [`rapidgzip_core::ReadAt`] requires *positional*
//! reads against a source whose length is stable for the whole decode, so a
//! FIFO, a process substitution (`<(zcat x.gz)`), or `/dev/stdin` cannot be fed
//! to it. Falling back also keeps the pre-existing error text for a file that is
//! named `.gz` but is not gzip, because that error is then raised by
//! `MultiGzDecoder` exactly as before.
//!
//! # Thread budget
//!
//! `rapidgzip_core`'s own default is `available_parallelism()` **per reader**,
//! which is wrong for Bismark: the aligner's `--parallel N` runs N in-process
//! workers ([`crate::aligner::parallel`]) and each opens its own read stream, so
//! the naive default would ask for `N * streams * cores` decoder threads.
//!
//! The budget is therefore *process-global* and resolved exactly once, rather
//! than threaded through the 21 migrated call sites. Call sites simply use
//! [`open_read`]; the aligner, which is the only tool that opens read streams
//! concurrently, calls [`set_stream_budget`] once at start-up with
//! [`per_stream_threads`] computed from its already-resolved effective
//! `--parallel` value. One value, one writer, no way for two layers to
//! disagree.
//!
//! Resolution order, highest priority first:
//!
//! 1. `BISMARK_GUNZIP_THREADS` (benchmarking and escape hatch;
//!    `BISMARK_GUNZIP_THREADS=1` restores a fully sequential decode with no
//!    rebuild).
//! 2. The budget installed by [`set_stream_budget`], if any.
//! 3. `min(8, available_parallelism())`.
//!
//! Whatever the source, a resolved budget below `MIN_PARALLEL_THREADS` (4)
//! selects the sequential decoder, which at that size is measurably the faster
//! of the two. So `BISMARK_GUNZIP_THREADS=1` (or `2`, or `3`) all mean
//! "decode sequentially", and the parallel path can never be slower than the
//! `MultiGzDecoder` it replaced.
//!
//! Note that none of this affects the *bytes* produced: the decode is
//! thread-budget invariant, so the suite's byte-identity and worker-invariance
//! gates are unaffected by any of these knobs.

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use flate2::read::MultiGzDecoder;

/// Environment variable overriding the decoder-thread budget suite-wide.
pub const GUNZIP_THREADS_ENV: &str = "BISMARK_GUNZIP_THREADS";

/// Process-global per-stream decoder budget; `0` means "not installed".
static STREAM_BUDGET: AtomicUsize = AtomicUsize::new(0);

/// Upper bound on the per-stream decoder budget.
///
/// This is a *budget* decision, not a throughput plateau: decoding keeps scaling
/// past 8 threads (2783 MB/s at 8 to 3850 MB/s at 12 on a 16-core M4 Max,
/// `gzip -6`). The cap exists because decoding faster than the aligner
/// subprocess (Bowtie 2 / HISAT2 / minimap2) can consume buys nothing, while the
/// threads spent doing it are taken from the aligner, which is the real
/// bottleneck in a Bismark run. `BISMARK_GUNZIP_THREADS` bypasses the cap for
/// decode-bound workloads and for benchmarking.
const MAX_STREAM_THREADS: usize = 8;

/// Minimum budget at which the parallel decoder is actually worth using.
///
/// **This gate is load-bearing, not a micro-optimisation.** `rapidgzip`'s
/// speculative marker/window decode does redundant work that only amortises
/// once enough workers are running, so at a small budget it is *slower* than the
/// sequential `MultiGzDecoder`. Measured with the gate removed (639 MB FastQ,
/// 16-core M4 Max, best of 3), throughput relative to sequential:
///
/// | threads | `gzip -6` | `gzip -1` |
/// |---------|-----------|-----------|
/// | 2       | **0.71x** | **0.92x** |
/// | 3       | 1.06x     | 1.35x     |
/// | 4       | 1.38x     | 1.80x     |
/// | 8       | 1.80x     | 3.44x     |
///
/// At 2 threads that is a 29 % regression on ordinary `gzip -6` input; 3 is
/// break-even within noise. 4 is the first budget that pays on both compression
/// levels, so below it [`open_read_with_threads`] takes the sequential path and
/// this change can never make decoding slower than it was.
const MIN_PARALLEL_THREADS: usize = 4;

/// Returns `true` when `path` is treated as gzip-compressed.
///
/// Matches the suffix test used throughout the suite (and Perl's `/gz$/`), so a
/// migrated call site keeps its exact previous routing.
#[must_use]
pub fn is_gzipped(path: &Path) -> bool {
    path.to_string_lossy().ends_with(".gz")
}

/// The unconstrained budget: `min(8, available_parallelism())`, at least 1.
#[must_use]
fn machine_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, MAX_STREAM_THREADS)
}

/// Resolves the decoder-thread budget for one read stream.
///
/// See the module docs for the resolution order. Always returns at least 1.
#[must_use]
pub fn default_threads() -> usize {
    if let Some(forced) = env_threads() {
        return forced;
    }
    match STREAM_BUDGET.load(Ordering::Relaxed) {
        0 => machine_threads(),
        installed => installed,
    }
}

/// Installs the process-global per-stream budget.
///
/// Intended to be called **once**, before any read stream is opened, by a tool
/// that opens several concurrently (in practice: the aligner, from its resolved
/// effective `--parallel`). `threads` is clamped to at least 1. A later call
/// overwrites the value; already-open readers keep the budget they were built
/// with, which is harmless because the decoded bytes do not depend on it.
pub fn set_stream_budget(threads: usize) {
    STREAM_BUDGET.store(threads.max(1), Ordering::Relaxed);
}

/// Divides the machine's parallelism across `workers * streams_per_worker`
/// concurrently open read streams.
///
/// The aligner calls this with its **effective** `--parallel` value and the
/// number of streams a worker holds open at once (1 for SE, 2 for PE), so the
/// total decoder-thread demand stays bounded by the core count instead of
/// multiplying by it. `BISMARK_GUNZIP_THREADS` overrides the result. Always
/// returns at least 1: a budget of 0 would be a configuration error in
/// `rapidgzip_core`, and 1 is simply the sequential decode.
#[must_use]
pub fn per_stream_threads(workers: usize, streams_per_worker: usize) -> usize {
    if let Some(forced) = env_threads() {
        return forced;
    }
    let concurrent = workers.max(1).saturating_mul(streams_per_worker.max(1));
    (machine_threads() / concurrent).max(1)
}

fn env_threads() -> Option<usize> {
    std::env::var(GUNZIP_THREADS_ENV)
        .ok()?
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0)
}

/// Opens `path` for reading, transparently decompressing gzip/BGZF.
///
/// Uses [`default_threads`] for the decoder budget. See the module docs for the
/// dispatch ladder.
///
/// # Errors
///
/// Returns the underlying [`io::Error`] when `path` cannot be opened. A file
/// that is named `.gz` but is not gzip opens successfully here and fails on the
/// first read, exactly as it did on the previous `MultiGzDecoder` path.
pub fn open_read(path: &Path) -> io::Result<Box<dyn BufRead + Send>> {
    open_read_with_threads(path, default_threads())
}

/// Opens `path` for reading with an explicit decoder-thread budget.
///
/// `threads` below [`MIN_PARALLEL_THREADS`] selects the sequential decoder (it
/// is faster there); the value is ignored entirely for uncompressed input and
/// for the sequential fallback.
///
/// # Errors
///
/// As [`open_read`].
pub fn open_read_with_threads(path: &Path, threads: usize) -> io::Result<Box<dyn BufRead + Send>> {
    let file = File::open(path)?;

    if !is_gzipped(path) {
        return Ok(Box::new(BufReader::new(file)));
    }

    // rapidgzip needs positional reads over a stable-length source: regular
    // files only. A FIFO / process substitution / character device takes the
    // sequential path.
    // Below MIN_PARALLEL_THREADS the sequential decoder is measurably faster, so
    // the parallel path is not merely skipped for tidiness: engaging it would be
    // a regression. See the constant's docs for the numbers.
    let regular = file.metadata().map(|m| m.is_file()).unwrap_or(false);
    if regular
        && threads >= MIN_PARALLEL_THREADS
        && let Some(reader) = try_parallel(path, threads)
    {
        return Ok(reader);
    }

    Ok(Box::new(BufReader::new(MultiGzDecoder::new(file))))
}

/// Builds the parallel reader, or `None` when the decoder declines the input.
///
/// A `None` here is never fatal: the caller falls through to `MultiGzDecoder`,
/// which reproduces the previous behaviour including its error text.
fn try_parallel(path: &Path, threads: usize) -> Option<Box<dyn BufRead + Send>> {
    let decoder = rapidgzip_core::Decoder::builder()
        .decoder_threads(threads)
        .build()
        .ok()?;
    let reader = decoder.open(path).ok()?;
    Some(Box::new(BufReader::new(reader)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    use flate2::Compression;
    use flate2::write::GzEncoder;

    fn gz(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn write_temp(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn read_all(path: &Path) -> Vec<u8> {
        let mut out = Vec::new();
        open_read(path).unwrap().read_to_end(&mut out).unwrap();
        out
    }

    /// Four FastQ records, large enough to cross rapidgzip's 1 MiB grid spacing
    /// so the parallel path is genuinely exercised rather than trivially
    /// short-circuited.
    fn big_fastq() -> Vec<u8> {
        let mut out = Vec::new();
        for i in 0..40_000u32 {
            out.extend_from_slice(format!("@read_{i}\n").as_bytes());
            out.extend_from_slice(b"ACGTACGTACGTACGTACGTACGTACGTACGTACGTACGT\n+\n");
            out.extend_from_slice(b"IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII\n");
        }
        out
    }

    #[test]
    fn plain_file_passes_through() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_temp(&dir, "reads.fastq", b"@r\nACGT\n+\nIIII\n");
        assert_eq!(read_all(&path), b"@r\nACGT\n+\nIIII\n");
    }

    #[test]
    fn single_member_gzip_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let payload = big_fastq();
        let path = write_temp(&dir, "reads.fastq.gz", &gz(&payload));
        assert_eq!(read_all(&path), payload);
    }

    /// The decisive parity case: `MultiGzDecoder` reads *all* concatenated
    /// members where `GzDecoder` stops after the first. rapidgzip must match
    /// `MultiGzDecoder`.
    #[test]
    fn concatenated_members_are_all_read() {
        let dir = tempfile::tempdir().unwrap();
        let first = big_fastq();
        let second = b"@tail\nTTTT\n+\nIIII\n".to_vec();
        let mut raw = gz(&first);
        raw.extend_from_slice(&gz(&second));
        let path = write_temp(&dir, "concat.fastq.gz", &raw);

        let mut expected = first;
        expected.extend_from_slice(&second);
        assert_eq!(read_all(&path), expected);
    }

    #[test]
    fn empty_gzip_member_yields_no_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_temp(&dir, "empty.fastq.gz", &gz(b""));
        assert!(read_all(&path).is_empty());
    }

    /// Truncated input must surface as an `io::Error` on read, not as silent
    /// short output: a truncated FastQ that decoded quietly would drop reads.
    #[test]
    fn truncated_gzip_errors_on_read() {
        let dir = tempfile::tempdir().unwrap();
        let mut raw = gz(&big_fastq());
        raw.truncate(raw.len() / 2);
        let path = write_temp(&dir, "trunc.fastq.gz", &raw);

        let mut sink = Vec::new();
        let result = open_read(&path).unwrap().read_to_end(&mut sink);
        assert!(result.is_err(), "truncated gzip decoded without error");
    }

    /// A `.gz` name over non-gzip bytes must still fail, and must fail at read
    /// time (the pre-migration `MultiGzDecoder` behaviour), not at open time.
    #[test]
    fn non_gzip_bytes_named_gz_error_on_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_temp(&dir, "bogus.fastq.gz", b"@not gzip at all\n");
        let mut reader = open_read(&path).expect("open must succeed");
        let mut sink = Vec::new();
        assert!(reader.read_to_end(&mut sink).is_err());
    }

    #[test]
    fn missing_file_errors_at_open() {
        let dir = tempfile::tempdir().unwrap();
        assert!(open_read(&dir.path().join("nope.fastq.gz")).is_err());
    }

    #[test]
    fn single_thread_budget_uses_sequential_path_with_identical_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let payload = big_fastq();
        let path = write_temp(&dir, "reads.fastq.gz", &gz(&payload));

        let mut sequential = Vec::new();
        open_read_with_threads(&path, 1)
            .unwrap()
            .read_to_end(&mut sequential)
            .unwrap();
        assert_eq!(sequential, payload);
        assert_eq!(sequential, read_all(&path));
    }

    /// Worker-invariance for the decode itself: the decoded bytes must not
    /// depend on the thread budget.
    #[test]
    fn decode_is_thread_budget_invariant() {
        let dir = tempfile::tempdir().unwrap();
        let payload = big_fastq();
        let path = write_temp(&dir, "reads.fastq.gz", &gz(&payload));

        for threads in [1usize, 2, 3, 8] {
            let mut out = Vec::new();
            open_read_with_threads(&path, threads)
                .unwrap()
                .read_to_end(&mut out)
                .unwrap();
            assert_eq!(out, payload, "mismatch at {threads} decoder threads");
        }
    }

    #[test]
    fn is_gzipped_matches_suffix_only() {
        assert!(is_gzipped(Path::new("reads.fastq.gz")));
        assert!(is_gzipped(Path::new("/tmp/x.gz")));
        assert!(!is_gzipped(Path::new("reads.fastq")));
        assert!(!is_gzipped(Path::new("reads.gz.fastq")));
    }

    #[test]
    fn per_stream_threads_never_returns_zero() {
        // 64 streams on a 4-thread cap would floor to 0 without the clamp.
        assert_eq!(per_stream_threads(32, 2), 1);
        assert_eq!(per_stream_threads(0, 0), default_threads().max(1));
        assert!(per_stream_threads(1, 1) >= 1);
    }

    #[test]
    fn machine_threads_is_positive_and_capped() {
        let n = machine_threads();
        assert!((1..=MAX_STREAM_THREADS).contains(&n));
    }

    /// `default_threads` must never return 0, whatever the global budget is.
    /// (The budget is process-global and other tests in this binary may have
    /// installed one, so only the invariant is asserted here.)
    #[test]
    fn default_threads_is_positive() {
        assert!(default_threads() >= 1);
    }

    #[test]
    fn set_stream_budget_is_honoured_and_clamped() {
        set_stream_budget(3);
        assert_eq!(STREAM_BUDGET.load(Ordering::Relaxed), 3);
        set_stream_budget(0);
        assert_eq!(
            STREAM_BUDGET.load(Ordering::Relaxed),
            1,
            "0 must clamp to 1"
        );
        // Leave the global unset so the rest of the binary sees the default.
        STREAM_BUDGET.store(0, Ordering::Relaxed);
    }

    /// A FIFO is not a regular file, so it must take the sequential fallback
    /// rather than being handed to `ReadAt`.
    #[cfg(unix)]
    #[test]
    fn fifo_falls_back_to_sequential() {
        use std::os::unix::fs::FileTypeExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stream.fastq.gz");
        // The module forbids `unsafe`, so `mkfifo(3)` is reached via the tool
        // rather than via `libc`.
        let status = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success());
        assert!(std::fs::metadata(&path).unwrap().file_type().is_fifo());

        let payload = b"@r\nACGT\n+\nIIII\n".to_vec();
        let raw = gz(&payload);
        let writer_path = path.clone();
        let writer = std::thread::spawn(move || {
            std::fs::write(&writer_path, &raw).unwrap();
        });

        let mut out = Vec::new();
        open_read(&path).unwrap().read_to_end(&mut out).unwrap();
        writer.join().unwrap();
        assert_eq!(out, payload);
    }
}
