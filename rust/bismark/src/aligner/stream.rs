//! Streaming the converted reads to the aligner through FIFOs (#1120).
//!
//! The converted reads (`_C_to_T` / `_G_to_A`) have exactly one consumer, the
//! aligner (the methylation caller re-reads the original FastQ), so they can be
//! handed over through named pipes instead of temp files and never touch the disk.
//! This module creates and removes the pipes, runs one conversion pass per source,
//! and fans it out to one writer thread per aligner instance. It also provides the
//! in-memory variant used by the in-process rammap backend. The routing decision
//! lives in [`super::config`].
//!
//! Pipes are made with `mkfifo(1)` rather than `libc::mkfifo`, which avoids a
//! direct `libc` dependency and keeps the fallback to files (taken when `mkfifo`
//! is missing or fails) a live code path.
//!
//! HISAT2 rejects a FIFO (exit 255; it stats the input while sniffing the
//! format), so it keeps the file path.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::JoinHandle;

use crate::aligner::convert::{self, ConvKind, ConvertOptions, temp_dir_prefix};
use crate::aligner::error::{AlignerError, Result};

/// Create one named pipe at `path`.
///
/// Any existing entry at `path` is removed first: names are unique per process
/// and set ([`fifo_name`]), so one already there is a leftover of a dead run, and
/// `mkfifo` would fail `EEXIST` on it.
fn mkfifo(path: &Path) -> Result<()> {
    let _ = std::fs::remove_file(path);
    let status = Command::new("mkfifo").arg(path).status().map_err(|e| {
        AlignerError::Validation(format!("could not run mkfifo for {}: {e}", path.display()))
    })?;
    if !status.success() {
        // Names are length-capped ([`NAME_STEM_CAP`]), so the likely cause is a
        // temp dir that filled or went read-only after the start-up probe.
        return Err(AlignerError::Validation(format!(
            "mkfifo {} failed ({status}); re-run with --no_stream_converted to write the \
             converted reads as files instead",
            path.display()
        )));
    }
    Ok(())
}

/// A set of named pipes standing in for the converted temp files of one chunk.
///
/// One FIFO per aligner instance per mate, not one per converted file: two
/// readers cannot share a pipe, so the conversion pass is fanned out instead.
///
/// The pipes are unlinked on [`Drop`], including when `create` fails partway.
/// Unlinking does not disturb a process that already has the FIFO open.
#[derive(Debug)]
pub struct FifoSet {
    paths: Vec<PathBuf>,
}

impl FifoSet {
    /// Create one FIFO per entry of `names` under `temp_dir`, returning the set.
    ///
    /// `names` are file names, joined to the temp dir exactly as the converted
    /// temp files are (`temp_dir_prefix`). The caller makes each name unique; see
    /// [`fifo_name`].
    pub fn create(temp_dir: &Path, names: &[String]) -> Result<Self> {
        let prefix = temp_dir_prefix(temp_dir)?;
        let mut set = FifoSet {
            paths: Vec::with_capacity(names.len()),
        };
        for name in names {
            let path = PathBuf::from(format!("{prefix}{name}"));
            mkfifo(&path)?;
            // Push only after mkfifo succeeded, so Drop never unlinks a path we
            // did not create.
            set.paths.push(path);
        }
        Ok(set)
    }

    /// The created pipe paths, in `names` order.
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

impl Drop for FifoSet {
    fn drop(&mut self) {
        for p in &self.paths {
            // Best-effort, like the aligner's other temp cleanup.
            let _ = std::fs::remove_file(p);
        }
    }
}

/// How much of the converted name a FIFO name keeps.
///
/// The name only makes a stray pipe recognisable, so it is capped to keep the
/// whole name well inside the usual 255-byte path-component limit. Otherwise a
/// long sample name could pass the start-up probe and then fail `mkfifo` mid-run.
const NAME_STEM_CAP: usize = 64;

/// Hands out a distinct id to every [`FifoSet`] created in this process.
///
/// `--multicore N` runs chunks concurrently in one process, so pid and slot
/// repeat across chunks, and the part of the name that differs (`.temp.<i>`) is
/// at the end, where [`NAME_STEM_CAP`] truncates. Without this id two chunks could
/// share a path; since `mkfifo` unlinks first, both would then write into one
/// pipe and silently corrupt the output.
static NEXT_FIFO_SET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The FIFO file name standing in for converted temp file `converted_name` as
/// read by aligner instance `slot`.
///
/// Keeps up to [`NAME_STEM_CAP`] characters of the converted name for
/// recognisability. Uniqueness comes from `slot`, the pid and `set`
/// ([`NEXT_FIFO_SET`]), never from the truncated name.
///
/// The format extension stays last (`…_C_to_T.s0.4242.0.fifo.fastq`) for tools
/// that pick a format by suffix.
pub fn fifo_name(converted_name: &str, slot: usize, set: u64) -> String {
    let pid = std::process::id();
    let (stem, ext) = match converted_name.rsplit_once('.') {
        Some((stem, ext)) => (stem, Some(ext)),
        None => (converted_name, None),
    };
    // Truncate on a char boundary, so a non-ASCII sample name cannot panic here.
    let capped = match stem.char_indices().nth(NAME_STEM_CAP) {
        Some((byte_idx, _)) => &stem[..byte_idx],
        None => stem,
    };
    match ext {
        Some(ext) => format!("{capped}.s{slot}.{pid}.{set}.fifo.{ext}"),
        None => format!("{capped}.s{slot}.{pid}.{set}.fifo"),
    }
}

/// Claim the next [`FifoSet`] id for this process.
pub(crate) fn next_fifo_set() -> u64 {
    NEXT_FIFO_SET.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Remove FIFOs a killed Bismark run left in `temp_dir`.
///
/// A run stopped by a signal never drops its [`FifoSet`], so its pipes stay
/// behind. Each name carries the creating pid ([`fifo_name`], and the start-up
/// probe's `bismark_fifo_probe.<pid>.<n>`), so a pipe whose pid is no longer
/// running is removed. Only named pipes matching those patterns are touched, and
/// a pid that is running, whoever owns it, is left alone. Best-effort.
#[cfg(unix)]
pub fn sweep_stale_fifos(temp_dir: &Path) {
    use std::os::unix::fs::FileTypeExt;
    let Ok(entries) = std::fs::read_dir(temp_dir) else {
        return;
    };
    let me = std::process::id();
    for entry in entries.flatten() {
        let is_fifo = entry.file_type().is_ok_and(|t| t.is_fifo());
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(fifo_owner_pid) else {
            continue;
        };
        if is_fifo && pid != me && !pid_is_running(pid) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(not(unix))]
pub fn sweep_stale_fifos(_temp_dir: &Path) {}

/// The pid embedded in a name made by [`fifo_name`] or the start-up probe.
fn fifo_owner_pid(name: &str) -> Option<u32> {
    if let Some(rest) = name.strip_prefix("bismark_fifo_probe.") {
        return rest.split('.').next()?.parse().ok();
    }
    // `<stem>.s<slot>.<pid>.<set>.fifo[.<ext>]`
    let parts: Vec<&str> = name.split('.').collect();
    let f = parts.iter().rposition(|&p| p == "fifo")?;
    if f < 3 {
        return None;
    }
    let (slot, pid, set) = (parts[f - 3], parts[f - 2], parts[f - 1]);
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !(slot.strip_prefix('s').is_some_and(digits) && digits(set)) {
        return None;
    }
    pid.parse().ok()
}

/// Is `pid` a live process? `ps -p` answers for any owner; if `ps` cannot be run
/// the answer is "yes", so nothing is removed.
fn pid_is_running(pid: u32) -> bool {
    Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_or(true, |s| s.success())
}

/// Can `temp_dir` host a named pipe at all?
///
/// Answered by creating and unlinking one, since there is no portable capability
/// query; `false` on any failure (no FIFO support, or no `mkfifo` on `PATH`), in
/// which case the run falls back to files.
pub fn temp_dir_supports_fifo(temp_dir: &Path) -> bool {
    let name = format!(
        "bismark_fifo_probe.{}.{}",
        std::process::id(),
        next_fifo_set()
    );
    FifoSet::create(temp_dir, std::slice::from_ref(&name)).is_ok()
}

// ===========================================================================
// The streaming conversion itself: one conversion pass per source, fanned out
// over a bounded channel to one writer thread per consuming aligner instance.
//
//   R1.fq.gz ─> convert C→T ─┬─> [bounded chan] ─> writer ─> fifo_s0_m1 ─> bowtie2 --norc
//                            └─> [bounded chan] ─> writer ─> fifo_s3_m1 ─> bowtie2 --nofw
//
// CPU is one pass per source, as on the file path. Memory is bounded by the
// channel depth. The input is read once.
// ===========================================================================

/// How much converted output accumulates before it is handed to the writers.
///
/// Above the 64 KiB pipe buffer, so writes are pipe-sized rather than per record.
const BLOCK_BYTES: usize = 256 * 1024;

/// Blocks in flight per consumer. With [`BLOCK_BYTES`] this is ~1 MiB of slack
/// per aligner instance: enough that a briefly slower instance does not throttle
/// its sibling, and negligible next to the aligner's index. Not tuned for
/// instances with very different speeds.
const CHANNEL_DEPTH: usize = 4;

/// How a conversion pass cuts its output into blocks and how many blocks each
/// consumer may have queued before the converter waits for it.
#[derive(Clone, Copy)]
struct Shape {
    /// Blocks queued per consumer.
    depth: usize,
    /// `None`: cut a block every [`BLOCK_BYTES`]. `Some(n)`: cut one every `n`
    /// lines instead, so the slack is a record count whatever the read length.
    lines_per_block: Option<usize>,
}

/// The FIFO path: byte-sized blocks. Each aligner child reads its pipe
/// independently, so a little slack is enough.
const FIFO_SHAPE: Shape = Shape {
    depth: CHANNEL_DEPTH,
    lines_per_block: None,
};

/// Records per block on the in-memory path.
#[cfg(feature = "rammap-inprocess")]
const IN_MEMORY_BLOCK_RECORDS: usize = 256;

/// The in-memory path. The merge refills one consumer at a time by
/// [`crate::aligner::inprocess::CHUNK`] records while its siblings read nothing,
/// so each consumer must be able to queue a whole chunk (plus slack) or the
/// shared conversion pass stalls. Sized in records so long reads do not shrink it.
#[cfg(feature = "rammap-inprocess")]
const IN_MEMORY_SHAPE: Shape = Shape {
    depth: crate::aligner::inprocess::CHUNK.div_ceil(IN_MEMORY_BLOCK_RECORDS) + CHANNEL_DEPTH,
    // Four lines per FastQ record; the in-process backend reads FastQ only.
    lines_per_block: Some(4 * IN_MEMORY_BLOCK_RECORDS),
};

/// Writer lifecycle states, as seen from outside the writer thread.
///
/// A shared atomic rather than a channel, because both the reader handshake and
/// the teardown read it, and a channel message can be consumed only once.
mod writer_state {
    /// Still blocked in open-for-write; nothing has opened the pipe for read.
    pub const PARKED: u8 = 0;
    /// Past the open: a reader arrived.
    pub const OPENED: u8 = 1;
    /// The open itself failed; nobody will ever be its reader.
    pub const OPEN_FAILED: u8 = 2;
    /// Its reader left early; blocked in a second open-for-write so a late
    /// reader gets EOF. Still needs releasing at teardown.
    pub const REOPENING: u8 = 3;
}

/// One converted read stream to produce: an input file, a substitution, and the
/// paired-end read-number tag (`b""` for single-end).
///
/// The unit of conversion work, as distinct from a consumer (one aligner
/// instance's view of a source). Several consumers can share a source.
#[derive(Debug, Clone)]
pub(crate) struct ConvSource {
    /// The original read file to convert.
    pub input: PathBuf,
    /// C→T or G→A.
    pub kind: ConvKind,
    /// `/1/1`, `/2/2`, or empty for single-end (`convert::pe_id_suffix`).
    pub id_suffix: &'static [u8],
}

/// A [`Write`] that buffers into blocks and hands each block to every consumer
/// of one source.
///
/// Every consumer gets the same bytes the file path would have written.
struct FanOut {
    senders: Vec<SyncSender<Arc<[u8]>>>,
    buf: Vec<u8>,
    lines_per_block: Option<usize>,
    lines: usize,
    /// Set once a consumer has gone away mid-stream.
    lost_consumer: bool,
}

impl FanOut {
    fn new(senders: Vec<SyncSender<Arc<[u8]>>>, lines_per_block: Option<usize>) -> Self {
        FanOut {
            senders,
            buf: Vec::with_capacity(BLOCK_BYTES + 4096),
            lines_per_block,
            lines: 0,
            lost_consumer: false,
        }
    }

    fn send_block(&mut self) -> std::io::Result<()> {
        // One allocation per block, shared by every consumer via `Arc`.
        let block: Arc<[u8]> = Arc::from(std::mem::replace(
            &mut self.buf,
            Vec::with_capacity(BLOCK_BYTES + 4096),
        ));
        self.lines = 0;
        // A closed receiver means that consumer stopped reading (its aligner
        // died). Keep feeding the others whole records: cutting them off mid-record
        // would make a healthy sibling fail and hide the dead one's exit status.
        // The loss is reported once the pass ends.
        let before = self.senders.len();
        self.senders
            .retain(|tx| tx.send(Arc::clone(&block)).is_ok());
        if self.senders.len() < before {
            self.lost_consumer = true;
        }
        if self.senders.is_empty() {
            return Err(consumer_lost());
        }
        Ok(())
    }
}

fn consumer_lost() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "an aligner instance stopped reading its converted reads",
    )
}

impl Write for FanOut {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        let full = match self.lines_per_block {
            None => self.buf.len() >= BLOCK_BYTES,
            Some(max) => {
                self.lines += data.iter().filter(|&&b| b == b'\n').count();
                self.lines >= max
            }
        };
        if full {
            self.send_block()?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.buf.is_empty() {
            self.send_block()?;
        }
        Ok(())
    }
}

/// An in-flight streamed conversion: the FIFOs standing in for the converted temp
/// files, plus the threads filling them.
///
/// Construct with [`start`](Self::start), hand [`path`](Self::path) to each
/// aligner instance, then call [`finish`](Self::finish) once the merge has
/// drained every instance.
///
/// Opening a FIFO for write blocks until a reader opens it, so `start` leaves
/// every writer thread parked in `open`. The caller must spawn all aligner
/// children before priming any
/// ([`super::align::AlignerStream::spawn_unprimed`]); each child's open releases
/// its writer.
pub(crate) struct StreamedConversion {
    /// One FIFO path per consumer, in the caller's demand order.
    paths: Vec<PathBuf>,
    /// One converter thread per source; yields that source's record count.
    converters: Vec<JoinHandle<Result<u64>>>,
    /// One writer thread per consumer.
    writers: Vec<JoinHandle<Result<()>>>,
    /// Per consumer: one of [`writer_state`], published by that writer. Read by
    /// both the reader handshake and the teardown.
    opened: Vec<Arc<AtomicU8>>,
    /// Declared last so the pipes are unlinked *after* the thread handles are
    /// dropped (Rust drops fields in declaration order).
    _fifos: FifoSet,
}

impl StreamedConversion {
    /// Start converting `sources`, fanning each out to the consumers that want it.
    ///
    /// `demands[c]` is the index into `sources` that consumer `c` reads; the
    /// returned [`path`](Self::path)`(c)` is the FIFO to put in that instance's
    /// argv. Every source must have at least one consumer, so
    /// [`finish`](Self::finish)'s counts stay aligned with `sources`.
    ///
    /// `--gzip` is ignored: there is no file to shrink, so the pipe carries plain
    /// bytes and its name has no `.gz`.
    pub(crate) fn start(
        temp_dir: &Path,
        fasta: bool,
        opts: &ConvertOptions,
        sources: &[ConvSource],
        demands: &[usize],
    ) -> Result<Self> {
        // Name the FIFO after the file it replaces, minus the `.gz`.
        let mut plain = opts.clone();
        plain.gzip = false;

        // One id per set, so concurrent `--multicore` chunks get distinct paths.
        let set = next_fifo_set();
        let mut names = Vec::with_capacity(demands.len());
        for (consumer, &src) in demands.iter().enumerate() {
            let s = sources.get(src).ok_or_else(|| {
                AlignerError::Validation(format!(
                    "internal: consumer {consumer} demands converted source {src}, but only \
                     {} were planned",
                    sources.len()
                ))
            })?;
            let file_base = convert::file_base_for(s.kind);
            names.push(fifo_name(
                &convert::converted_name(&s.input, &plain, file_base, fasta)?,
                consumer,
                set,
            ));
        }

        let fifos = FifoSet::create(temp_dir, &names)?;
        let paths = fifos.paths().to_vec();

        let (receivers, converters) = spawn_converters(fasta, opts, sources, demands, FIFO_SHAPE)?;

        // One writer thread per consumer, each parked in its blocking open-for-write
        // until the matching aligner child opens the pipe for read.
        let mut writers = Vec::with_capacity(demands.len());
        let mut opened = Vec::with_capacity(demands.len());
        for (consumer, rx) in receivers.into_iter().enumerate() {
            let path = paths[consumer].clone();
            let state = Arc::new(AtomicU8::new(writer_state::PARKED));
            opened.push(Arc::clone(&state));
            writers.push(std::thread::spawn(move || writer_thread(path, rx, state)));
        }

        Ok(StreamedConversion {
            paths,
            converters,
            writers,
            opened,
            _fifos: fifos,
        })
    }

    /// The FIFO for aligner instance `consumer` (its `-U` / `-1` / `-2` path).
    pub(crate) fn path(&self, consumer: usize) -> &Path {
        &self.paths[consumer]
    }

    /// Block until every consumer has opened its pipe, or until one of them has
    /// died without doing so.
    ///
    /// This keeps a failed instance from hanging the run. The fan-out is bounded,
    /// so a consumer that never reads fills its channel after [`CHANNEL_DEPTH`]
    /// blocks and stalls the shared conversion pass, starving its siblings, which
    /// then block mid-merge with no timeout.
    ///
    /// `child_status` reports whether the child behind a consumer has exited
    /// (`UnprimedAlignerStream::try_wait`). It is polled rather than given a
    /// deadline, since loading a large index can take minutes.
    ///
    /// Only a non-zero exit before opening is an error. A child that exits cleanly
    /// without reading has its writer released at once and the run continues.
    pub(crate) fn await_readers<F>(&self, mut child_status: F) -> Result<()>
    where
        F: FnMut(usize) -> Result<Option<std::process::ExitStatus>>,
    {
        let mut pending: Vec<usize> = (0..self.opened.len()).collect();
        while !pending.is_empty() {
            let mut still_waiting = Vec::with_capacity(pending.len());
            for &c in &pending {
                if self.opened[c].load(Ordering::Acquire) != writer_state::PARKED {
                    continue; // reader arrived (or the open failed outright)
                }
                match child_status(c)? {
                    None => still_waiting.push(c), // still starting up
                    Some(status) => {
                        // It may have opened the pipe and exited between the two
                        // checks, so re-check before drawing any conclusion.
                        if self.opened[c].load(Ordering::Acquire) != writer_state::PARKED {
                            continue;
                        }
                        if !status.success() {
                            return Err(AlignerError::Validation(format!(
                                "an aligner instance exited ({status}) without reading its \
                                 converted reads from {}; it would have stalled the conversion \
                                 shared with the other instances",
                                self.paths[c].display()
                            )));
                        }
                        // Exited cleanly without reading. Release its writer now,
                        // or its channel fills and stalls the siblings. The child
                        // is gone, so nobody else can be reading this pipe.
                        if !self.writers[c].is_finished() {
                            release(&self.paths[c]);
                        }
                    }
                }
            }
            pending = still_waiting;
            if !pending.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        Ok(())
    }

    /// Join every writer and converter, returning the per-source record count in
    /// `sources` order (the `(N sequences)` the file path reports on its banner).
    ///
    /// Call this only after the merge has drained every aligner instance; joining
    /// earlier would wait for consumers that are not reading. On an error path,
    /// drop the value instead (see the [`Drop`] impl).
    pub(crate) fn finish(mut self) -> Result<Vec<u64>> {
        self.drain_unopened();
        let mut first_err = None;
        for h in std::mem::take(&mut self.writers) {
            if let Err(e) = join(h) {
                first_err.get_or_insert(e);
            }
        }
        let mut counts = Vec::with_capacity(self.converters.len());
        for h in std::mem::take(&mut self.converters) {
            match join(h) {
                Ok(n) => counts.push(n),
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(counts),
        }
    }
}

impl StreamedConversion {
    /// Unpark any writer whose aligner never opened its pipe, by opening it here
    /// and discarding what comes out.
    ///
    /// An aligner that exits without reading its input leaves its writer blocked
    /// in open-for-write, and joining that writer would hang the run.
    ///
    /// Reading the pipe here is safe because every aligner child has been reaped
    /// by the time this runs, so no live aligner can lose bytes to it. The
    /// streaming drivers (`run_se_instances_streamed` / `run_pe_instances_streamed`)
    /// spawn, merge and reap or kill every child before returning, on success and
    /// on error, so this holds both in [`finish`](Self::finish) and in
    /// [`Drop`].
    ///
    /// The `open` cannot block: it is reached only while the writer is (or is
    /// about to be) parked in open-for-write, and opposing opens release each
    /// other.
    fn drain_unopened(&mut self) {
        for ((state, path), writer) in self.opened.iter().zip(&self.paths).zip(&self.writers) {
            // Release only a writer still parked in open; one past it ends on its
            // own. A writer that has already returned (even by panicking before
            // publishing its state) will never open its end, so opening here
            // would block forever.
            let parked = matches!(
                state.load(Ordering::Acquire),
                writer_state::PARKED | writer_state::REOPENING
            );
            if !parked || writer.is_finished() {
                continue;
            }
            release(path);
        }
    }
}

/// Open `path` for read and discard everything written to it, on a detached
/// thread, so its writer can run to completion. Only for a pipe whose aligner
/// has exited.
fn release(path: &Path) {
    if let Ok(mut f) = std::fs::File::open(path) {
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut f, &mut std::io::sink());
        });
    }
}

impl Drop for StreamedConversion {
    fn drop(&mut self) {
        // The error path. Every aligner child has already been reaped by the
        // streaming driver, so release any parked writer (see `drain_unopened`)
        // and detach the threads rather than join them, so a failing chunk cannot
        // hang. Each released thread ends on its own, which matters because
        // `--multicore N` keeps sibling chunks running after a chunk error.
        //
        // After `finish()`, `writers` is empty and there is nothing to release.
        // `_fifos` drops after this, so the release runs before the unlink.
        if !self.writers.is_empty() {
            self.drain_unopened();
        }
    }
}

/// One receiver per consumer, plus one converter handle per source.
type Converters = (Vec<Receiver<Arc<[u8]>>>, Vec<JoinHandle<Result<u64>>>);

/// Start one converter thread per source, fanning each out to the consumers that
/// demand it, and hand back one receiver per consumer. Shared by
/// [`StreamedConversion`] and [`InMemoryConversion`].
///
/// A source with no consumer is an error rather than skipped, since it would
/// misalign the per-source counts.
fn spawn_converters(
    fasta: bool,
    opts: &ConvertOptions,
    sources: &[ConvSource],
    demands: &[usize],
    shape: Shape,
) -> Result<Converters> {
    let mut receivers = Vec::with_capacity(demands.len());
    let mut senders_by_source: Vec<Vec<SyncSender<Arc<[u8]>>>> = vec![Vec::new(); sources.len()];
    for (consumer, &src) in demands.iter().enumerate() {
        let s = sources.get(src).ok_or_else(|| {
            AlignerError::Validation(format!(
                "internal: consumer {consumer} demands converted source {src}, but only {} were \
                 planned",
                sources.len()
            ))
        })?;
        let _ = s;
        let (tx, rx) = sync_channel::<Arc<[u8]>>(shape.depth);
        senders_by_source[src].push(tx);
        receivers.push(rx);
    }

    let mut converters = Vec::with_capacity(sources.len());
    for (i, (s, senders)) in sources.iter().zip(senders_by_source).enumerate() {
        if senders.is_empty() {
            return Err(AlignerError::Validation(format!(
                "internal: converted source {i} ({}) has no aligner instance reading it",
                s.input.display()
            )));
        }
        let (input, kind, id_suffix) = (s.input.clone(), s.kind, s.id_suffix);
        let opts = opts.clone();
        converters.push(std::thread::spawn(move || {
            converter_thread(input, fasta, opts, kind, id_suffix, senders, shape)
        }));
    }
    Ok((receivers, converters))
}

/// The reading end of one consumer's channel, as a [`Read`].
///
/// Lets the in-process aligner consume converted reads with no file or pipe in
/// between; the bytes are those the file path would have written.
#[cfg(feature = "rammap-inprocess")]
pub(crate) struct ChannelReader {
    rx: Receiver<Arc<[u8]>>,
    block: Arc<[u8]>,
    pos: usize,
}

#[cfg(feature = "rammap-inprocess")]
impl std::io::Read for ChannelReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        while self.pos >= self.block.len() {
            match self.rx.recv() {
                Ok(next) => {
                    self.block = next;
                    self.pos = 0;
                }
                // Every sender dropped: the conversion finished. EOF.
                Err(_) => return Ok(0),
            }
        }
        let n = (self.block.len() - self.pos).min(out.len());
        out[..n].copy_from_slice(&self.block[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// An in-flight conversion delivered in memory, with no file and no FIFO, for the
/// in-process rammap backend, which reads through a
/// [`BufRead`](std::io::BufRead) in this same process. The conversion loop is the
/// file path's, so the bytes are unchanged.
#[cfg(feature = "rammap-inprocess")]
pub(crate) struct InMemoryConversion {
    converters: Vec<JoinHandle<Result<u64>>>,
}

#[cfg(feature = "rammap-inprocess")]
impl InMemoryConversion {
    /// Start converting `sources`, returning one reader per consumer (in `demands`
    /// order) alongside the handle used to collect the counts.
    pub(crate) fn start(
        fasta: bool,
        opts: &ConvertOptions,
        sources: &[ConvSource],
        demands: &[usize],
    ) -> Result<(Vec<ChannelReader>, Self)> {
        let (receivers, converters) =
            spawn_converters(fasta, opts, sources, demands, IN_MEMORY_SHAPE)?;
        let readers = receivers
            .into_iter()
            .map(|rx| ChannelReader {
                rx,
                block: Arc::from(Vec::new()),
                pos: 0,
            })
            .collect();
        Ok((readers, InMemoryConversion { converters }))
    }

    /// Join every converter, returning the per-source record count in `sources`
    /// order. Call it only after every reader has been drained or dropped.
    pub(crate) fn finish(self) -> Result<Vec<u64>> {
        let mut first_err = None;
        let mut counts = Vec::with_capacity(self.converters.len());
        for h in self.converters {
            match join(h) {
                Ok(n) => counts.push(n),
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(counts),
        }
    }
}

/// Join one worker, mapping a panic to a loud error rather than a silent loss.
fn join<T>(h: JoinHandle<Result<T>>) -> Result<T> {
    match h.join() {
        Ok(r) => r,
        Err(_) => Err(AlignerError::Validation(
            "a converted-read streaming thread panicked".to_string(),
        )),
    }
}

/// Fill one FIFO from its channel.
///
/// The `open` blocks until the aligner child opens the pipe for read.
fn writer_thread(path: PathBuf, rx: Receiver<Arc<[u8]>>, state: Arc<AtomicU8>) -> Result<()> {
    let fifo = std::fs::OpenOptions::new().write(true).open(&path);
    // Publish that the blocking open has returned, success or failure, before
    // anything else, so no observer sees this writer as parked once it is not.
    state.store(
        if fifo.is_ok() {
            writer_state::OPENED
        } else {
            writer_state::OPEN_FAILED
        },
        Ordering::Release,
    );
    let mut fifo = fifo?;
    while let Ok(block) = rx.recv() {
        if let Err(e) = fifo.write_all(&block) {
            // The reader went away mid-stream. Stop taking blocks so the
            // converter errors instead of stalling, then give any late reader an
            // empty stream: a wrapper may sniff the input and close it before the
            // real aligner opens the path, which with no writer would block.
            drop(rx);
            drop(fifo);
            reopen_and_close(&path, &state);
            // A broken pipe is reported by the aligner's own exit status (the
            // stream's `finish`) or the converter's error, not here.
            return match e.kind() {
                std::io::ErrorKind::BrokenPipe => Ok(()),
                _ => Err(e.into()),
            };
        }
    }
    fifo.flush()?;
    Ok(())
}

/// Open `path` for write once more and close it straight away, so a reader that
/// opens it later sees EOF rather than blocking.
///
/// Marked [`writer_state::REOPENING`] while it waits, so the teardown
/// (`drain_unopened`) releases it if no late reader ever comes.
fn reopen_and_close(path: &Path, state: &AtomicU8) {
    state.store(writer_state::REOPENING, Ordering::Release);
    let reopened = std::fs::OpenOptions::new().write(true).open(path);
    state.store(writer_state::OPENED, Ordering::Release);
    drop(reopened);
}

/// Run one conversion pass, fanning it out to every consumer of that source.
///
/// Uses [`convert::convert_fastq_into`] / [`convert::convert_fasta_into`], the
/// same loops the file path runs.
fn converter_thread(
    input: PathBuf,
    fasta: bool,
    opts: ConvertOptions,
    kind: ConvKind,
    id_suffix: &'static [u8],
    senders: Vec<SyncSender<Arc<[u8]>>>,
    shape: Shape,
) -> Result<u64> {
    let mut reader = convert::open_reader(&input)?;
    let mut out = FanOut::new(senders, shape.lines_per_block);
    let (count, _seqid_tabs) = if fasta {
        convert::convert_fasta_into(&mut *reader, &opts, kind, id_suffix, &mut out)?
    } else {
        convert::convert_fastq_into(&mut *reader, &opts, kind, id_suffix, &mut out)?
    };
    out.flush()?;
    let lost = out.lost_consumer;
    // Dropping the senders ends the writers' input, so the aligner sees EOF.
    drop(out);
    if lost {
        return Err(consumer_lost().into());
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// A FIFO is neither a regular file nor a directory; enough here without the
    /// unix-only `FileTypeExt`.
    fn is_fifo(p: &Path) -> bool {
        let md = std::fs::symlink_metadata(p).expect("fifo should exist");
        !md.is_file() && !md.is_dir() && !md.file_type().is_symlink()
    }

    #[test]
    fn create_then_unlink_round_trip() {
        let tmp = TempDir::new().unwrap();
        let names = vec![
            fifo_name("r1_C_to_T.fastq", 0, 0),
            fifo_name("r1_C_to_T.fastq", 1, 0),
        ];
        let paths: Vec<PathBuf>;
        {
            let set = FifoSet::create(tmp.path(), &names).unwrap();
            paths = set.paths().to_vec();
            assert_eq!(paths.len(), 2);
            assert_ne!(paths[0], paths[1], "one pipe per instance, not per file");
            for p in &paths {
                assert!(is_fifo(p), "{} should be a fifo", p.display());
            }
        }
        for p in &paths {
            assert!(!p.exists(), "{} should be unlinked on drop", p.display());
        }
    }

    #[test]
    fn create_is_atomic_on_failure() {
        // Second name is a path traversal into a directory that does not exist,
        // so its mkfifo fails; the first pipe must not survive the error.
        let tmp = TempDir::new().unwrap();
        let names = vec![
            fifo_name("ok.fastq", 0, 0),
            format!("no_such_dir/{}", fifo_name("bad.fastq", 1, 0)),
        ];
        assert!(FifoSet::create(tmp.path(), &names).is_err());
        assert!(!tmp.path().join(&names[0]).exists());
    }

    #[test]
    fn fifo_owner_pid_reads_both_name_shapes() {
        assert_eq!(fifo_owner_pid("r_C_to_T.s3.4242.7.fifo.fastq"), Some(4242));
        assert_eq!(fifo_owner_pid("r_C_to_T.s0.17.0.fifo"), Some(17));
        assert_eq!(fifo_owner_pid("bismark_fifo_probe.99.3"), Some(99));
        assert_eq!(fifo_owner_pid("reads.fastq"), None);
        assert_eq!(fifo_owner_pid("my.fifo"), None);
        assert_eq!(fifo_owner_pid("x.sA.1.2.fifo"), None);
    }

    /// A pipe left by a run that no longer exists is removed; one belonging to a
    /// live process, and anything that is not a pipe, is kept.
    #[cfg(unix)]
    #[test]
    fn sweep_removes_only_stale_pipes() {
        let tmp = TempDir::new().unwrap();
        // A pid that cannot be running: above the kernel's pid limit.
        let dead = fifo_name("r_C_to_T.fastq", 0, 0)
            .replace(&format!(".{}.", std::process::id()), ".4294967.");
        let live = fifo_name("r_C_to_T.fastq", 1, 0);
        let _set = FifoSet::create(tmp.path(), std::slice::from_ref(&live)).unwrap();
        mkfifo(&tmp.path().join(&dead)).unwrap();
        let plain = tmp.path().join("r.s0.4294967.0.fifo.fastq.txt");
        std::fs::write(&plain, b"").unwrap();
        let plain_named_like_a_fifo = tmp.path().join("q.s0.4294967.0.fifo.fastq");
        std::fs::write(&plain_named_like_a_fifo, b"").unwrap();

        sweep_stale_fifos(tmp.path());

        assert!(!tmp.path().join(&dead).exists(), "stale pipe must go");
        assert!(tmp.path().join(&live).exists(), "our own pipe must stay");
        assert!(plain.exists());
        assert!(plain_named_like_a_fifo.exists(), "only pipes are removed");
    }

    #[test]
    fn probe_succeeds_on_a_real_temp_dir() {
        let tmp = TempDir::new().unwrap();
        assert!(temp_dir_supports_fifo(tmp.path()));
        // and leaves nothing behind
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
    }

    #[test]
    fn probe_fails_when_temp_dir_cannot_hold_one() {
        // A regular file as `--temp_dir`: it cannot be created as a directory,
        // so nothing can be made inside it.
        let tmp = TempDir::new().unwrap();
        let not_a_dir = tmp.path().join("iam_a_file");
        std::fs::write(&not_a_dir, b"x").unwrap();
        assert!(!temp_dir_supports_fifo(&not_a_dir));
    }

    fn opts() -> ConvertOptions {
        ConvertOptions {
            prefix: None,
            gzip: false,
            skip: None,
            upto: None,
            icpc: false,
            maximum_length_cutoff: None,
        }
    }

    /// A fixture spanning several [`BLOCK_BYTES`] blocks, so the bounded channel
    /// fills and both consumers must be drained concurrently.
    fn write_fixture(dir: &Path) -> PathBuf {
        let path = dir.join("reads.fastq");
        let mut fq = Vec::new();
        for i in 0..20_000u32 {
            fq.extend_from_slice(format!("@read_{i} some description\n").as_bytes());
            fq.extend_from_slice(
                b"acgtACGTnnCCGGttttACGTACGTACGTAC\n+\nIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII\n",
            );
        }
        std::fs::write(&path, &fq).unwrap();
        path
    }

    /// Each consumer's FIFO carries exactly the bytes the file path would have
    /// written, for both consumers of one source, from one conversion pass.
    #[test]
    fn streamed_bytes_match_the_converted_file() {
        let tmp = TempDir::new().unwrap();
        let input = write_fixture(tmp.path());
        let o = opts();

        let expected = std::fs::read(
            crate::aligner::convert::bisulfite_convert_fastq_se(&input, tmp.path(), &o)
                .unwrap()
                .path,
        )
        .unwrap();
        assert!(
            expected.len() > BLOCK_BYTES * 2,
            "fixture must span several blocks"
        );

        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        let demands = [0usize, 0]; // two aligner instances read the same source
        let streamed =
            StreamedConversion::start(tmp.path(), false, &o, &sources, &demands).unwrap();

        // Stand in for the aligner children: open every pipe before draining any.
        let readers: Vec<_> = demands
            .iter()
            .enumerate()
            .map(|(c, _)| {
                let p = streamed.path(c).to_path_buf();
                std::thread::spawn(move || std::fs::read(p).unwrap())
            })
            .collect();

        let got: Vec<Vec<u8>> = readers.into_iter().map(|h| h.join().unwrap()).collect();
        let counts = streamed.finish().unwrap();

        assert_eq!(counts, vec![20_000]);
        for (c, bytes) in got.iter().enumerate() {
            assert_eq!(bytes, &expected, "consumer {c} received different bytes");
        }
    }

    /// An aligner that never reads its input must not hang `finish`: its writer
    /// is parked in open-for-write and must be released.
    #[test]
    fn finish_does_not_hang_when_nothing_reads_the_pipe() {
        let tmp = TempDir::new().unwrap();
        let input = write_fixture(tmp.path());
        let o = opts();
        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        // Two consumers, neither ever opened, called after every "aligner" has
        // exited (where `finish` runs).
        let streamed = StreamedConversion::start(tmp.path(), false, &o, &sources, &[0, 0]).unwrap();
        let counts = streamed.finish().unwrap();
        assert_eq!(counts, vec![20_000]);
    }

    /// An instance that dies before opening its pipe is reported as an error
    /// naming it, rather than stalling its siblings.
    ///
    /// No reader is attached to consumer 0: the stalled consumer 1 would starve it,
    /// so joining it would hang the test.
    #[test]
    fn await_readers_reports_an_instance_that_died_before_opening() {
        let tmp = TempDir::new().unwrap();
        let input = write_fixture(tmp.path());
        let o = opts();
        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        let streamed = StreamedConversion::start(tmp.path(), false, &o, &sources, &[0, 0]).unwrap();

        let dead = std::process::Command::new("false").status().unwrap();
        assert!(!dead.success());
        let err = streamed
            .await_readers(|consumer| Ok(if consumer == 1 { Some(dead) } else { None }))
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("without reading its converted reads"),
            "must name the failure mode: {msg}"
        );
        assert!(
            msg.contains(".fifo"),
            "must name which instance's pipe: {msg}"
        );
    }

    /// A child that exits cleanly without reading is not a failure (the shape of
    /// the `aligner_cli` fake aligners).
    #[test]
    fn await_readers_tolerates_an_instance_that_exits_cleanly_without_reading() {
        let tmp = TempDir::new().unwrap();
        let input = write_fixture(tmp.path());
        let o = opts();
        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        let streamed = StreamedConversion::start(tmp.path(), false, &o, &sources, &[0, 0]).unwrap();

        let ok = std::process::Command::new("true").status().unwrap();
        assert!(ok.success());
        // Neither pipe is opened and both "children" exited 0: must return Ok.
        streamed.await_readers(|_| Ok(Some(ok))).unwrap();
        streamed.finish().unwrap();
    }

    /// Every pipe opened and no child exited: `await_readers` returns.
    #[test]
    fn await_readers_returns_once_every_pipe_is_open() {
        let tmp = TempDir::new().unwrap();
        let input = write_fixture(tmp.path());
        let o = opts();
        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        let streamed = StreamedConversion::start(tmp.path(), false, &o, &sources, &[0, 0]).unwrap();
        let readers: Vec<_> = (0..2)
            .map(|c| {
                let p = streamed.path(c).to_path_buf();
                std::thread::spawn(move || std::fs::read(p).unwrap())
            })
            .collect();

        streamed.await_readers(|_| Ok(None)).unwrap();

        for h in readers {
            assert!(!h.join().unwrap().is_empty());
        }
        streamed.finish().unwrap();
    }

    /// Dropping without `finish` must not leave a parked writer behind.
    #[test]
    fn drop_releases_a_parked_writer() {
        let tmp = TempDir::new().unwrap();
        let input = write_fixture(tmp.path());
        let o = opts();
        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        let streamed = StreamedConversion::start(tmp.path(), false, &o, &sources, &[0]).unwrap();
        drop(streamed); // nothing ever opened the pipe
    }

    /// `--gzip` is a no-op when streaming: the pipe carries the plain bytes.
    #[test]
    fn gzip_is_a_no_op_on_the_streaming_path() {
        let tmp = TempDir::new().unwrap();
        let input = write_fixture(tmp.path());
        let plain = opts();
        let expected = std::fs::read(
            crate::aligner::convert::bisulfite_convert_fastq_se(&input, tmp.path(), &plain)
                .unwrap()
                .path,
        )
        .unwrap();

        let mut gz = opts();
        gz.gzip = true;
        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        let streamed = StreamedConversion::start(tmp.path(), false, &gz, &sources, &[0]).unwrap();
        assert!(
            !streamed.path(0).to_string_lossy().contains(".gz"),
            "a FIFO carrying plain bytes must not be named .gz — the aligner sniffs it"
        );
        let p = streamed.path(0).to_path_buf();
        let reader = std::thread::spawn(move || std::fs::read(p).unwrap());
        let got = reader.join().unwrap();
        streamed.finish().unwrap();
        assert_eq!(got, expected);
    }

    /// The in-memory path delivers exactly the bytes the file path would have
    /// written, for every consumer, from one conversion pass (the in-memory
    /// counterpart of `streamed_bytes_match_the_converted_file`).
    #[test]
    #[cfg(feature = "rammap-inprocess")]
    fn in_memory_bytes_match_the_converted_file() {
        use std::io::Read;

        let tmp = TempDir::new().unwrap();
        let input = write_fixture(tmp.path());
        let o = opts();
        let expected = std::fs::read(
            crate::aligner::convert::bisulfite_convert_fastq_se(&input, tmp.path(), &o)
                .unwrap()
                .path,
        )
        .unwrap();
        assert!(
            expected.len() > BLOCK_BYTES * 2,
            "fixture must span several blocks"
        );

        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        // Two consumers off one source, as directional single-end does.
        let (readers, conversion) =
            InMemoryConversion::start(false, &o, &sources, &[0, 0]).unwrap();

        // Drain both concurrently: the fan-out is bounded, so reading one to the
        // end before starting the other would stall the shared conversion pass.
        let handles: Vec<_> = readers
            .into_iter()
            .map(|mut r| {
                std::thread::spawn(move || {
                    let mut got = Vec::new();
                    r.read_to_end(&mut got).unwrap();
                    got
                })
            })
            .collect();
        let got: Vec<Vec<u8>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let counts = conversion.finish().unwrap();

        assert_eq!(counts, vec![20_000]);
        for (c, bytes) in got.iter().enumerate() {
            assert_eq!(bytes, &expected, "consumer {c} received different bytes");
        }
    }

    /// A reader dropped without being drained must not wedge the converter, so an
    /// aborted run can still be torn down.
    #[test]
    #[cfg(feature = "rammap-inprocess")]
    fn in_memory_readers_can_be_dropped_undrained() {
        let tmp = TempDir::new().unwrap();
        let input = write_fixture(tmp.path());
        let o = opts();
        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        let (readers, conversion) = InMemoryConversion::start(false, &o, &sources, &[0]).unwrap();
        drop(readers); // never read a byte
        // The converter sees its consumer go away and stops; `finish` returns.
        let _ = conversion.finish();
    }

    /// Run `f` on its own thread and fail if it has not returned within `secs`,
    /// so a regression shows up as a failure rather than a hung test suite.
    fn within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(std::time::Duration::from_secs(secs))
            .expect("hung (or panicked)")
    }

    /// Reads long enough that one in-process chunk is well over the FIFO path's
    /// ~1 MiB of per-consumer slack.
    #[cfg(feature = "rammap-inprocess")]
    fn write_long_read_fixture(dir: &Path, n: usize, len: usize) -> PathBuf {
        let path = dir.join("long_reads.fastq");
        let seq = "ACGT".repeat(len / 4);
        let qual = "I".repeat(seq.len());
        let mut fq = String::new();
        for i in 0..n {
            fq.push_str(&format!("@read_{i}\n{seq}\n+\n{qual}\n"));
        }
        std::fs::write(&path, fq).unwrap();
        path
    }

    /// The in-process merge refills one stream at a time by a whole `CHUNK` of
    /// records, so the in-memory shape must queue a whole chunk per consumer
    /// whatever the read length (byte-sized slack deadlocks on long reads).
    #[test]
    #[cfg(feature = "rammap-inprocess")]
    fn in_memory_survives_chunked_reads_of_long_records() {
        use crate::aligner::inprocess::CHUNK;
        use std::io::BufRead;

        let tmp = TempDir::new().unwrap();
        let n = CHUNK * 3 + 17;
        let input = write_long_read_fixture(tmp.path(), n, 1000);
        let o = opts();
        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        let (readers, conversion) =
            InMemoryConversion::start(false, &o, &sources, &[0, 0]).unwrap();

        let (records, counts) = within(120, move || {
            let mut readers: Vec<_> = readers.into_iter().map(std::io::BufReader::new).collect();
            let mut records = vec![0usize; readers.len()];
            let mut line = Vec::new();
            // Refill each consumer by a whole chunk in turn, as the merge does.
            loop {
                let mut progressed = false;
                for (c, r) in readers.iter_mut().enumerate() {
                    for _ in 0..CHUNK * 4 {
                        line.clear();
                        if r.read_until(b'\n', &mut line).unwrap() == 0 {
                            break;
                        }
                        progressed = true;
                        records[c] += 1;
                    }
                }
                if !progressed {
                    break;
                }
            }
            drop(readers);
            (records, conversion.finish().unwrap())
        });
        assert_eq!(counts, vec![n as u64]);
        assert_eq!(records, vec![n * 4, n * 4], "lines read per consumer");
    }

    /// An instance that exits cleanly without opening its pipe has its writer
    /// released at once; left parked, its channel fills after ~1 MiB and starves
    /// the sibling reading the same source.
    #[test]
    fn a_clean_exit_without_reading_does_not_starve_the_sibling() {
        let tmp = TempDir::new().unwrap();
        let input = write_fixture(tmp.path());
        assert!(
            std::fs::metadata(&input).unwrap().len() > (CHANNEL_DEPTH * BLOCK_BYTES) as u64,
            "fixture must exceed the per-consumer slack"
        );
        let o = opts();
        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        let streamed = StreamedConversion::start(tmp.path(), false, &o, &sources, &[0, 0]).unwrap();
        let p0 = streamed.path(0).to_path_buf();
        let reader = std::thread::spawn(move || std::fs::read(p0).unwrap().len());

        let counts = within(60, move || {
            let ok = std::process::Command::new("true").status().unwrap();
            // Consumer 0 is alive and reading; consumer 1 exited 0 unread.
            streamed
                .await_readers(|c| Ok(if c == 1 { Some(ok) } else { None }))
                .unwrap();
            let n = reader.join().unwrap();
            (n, streamed.finish().unwrap())
        });
        assert!(counts.0 > CHANNEL_DEPTH * BLOCK_BYTES);
        assert_eq!(counts.1, vec![20_000]);
    }

    /// One consumer quitting must not cut its sibling off mid-record: the
    /// sibling still receives the whole stream, and the loss is reported at the
    /// end rather than as a truncated read in the healthy instance.
    #[test]
    fn a_dead_consumer_does_not_truncate_its_sibling() {
        let tmp = TempDir::new().unwrap();
        let input = write_fixture(tmp.path());
        let o = opts();
        let expected = std::fs::read(
            crate::aligner::convert::bisulfite_convert_fastq_se(&input, tmp.path(), &o)
                .unwrap()
                .path,
        )
        .unwrap();
        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        let streamed = StreamedConversion::start(tmp.path(), false, &o, &sources, &[0, 0]).unwrap();
        let p0 = streamed.path(0).to_path_buf();
        let p1 = streamed.path(1).to_path_buf();

        let (got, finished) = within(60, move || {
            use std::io::Read;
            let sibling = std::thread::spawn(move || std::fs::read(p0).unwrap());
            let mut f = std::fs::File::open(&p1).unwrap();
            f.read_exact(&mut [0u8; 16]).unwrap();
            drop(f);
            (sibling.join().unwrap(), streamed.finish())
        });
        assert!(got == expected, "the sibling must receive every byte");
        assert!(finished.is_err(), "the lost consumer is still reported");
    }

    /// A reader that opens the pipe and closes it early (a wrapper sniffing the
    /// input) must not leave the real aligner blocked in `open` when it comes for
    /// the same path: it gets EOF, and the run fails rather than hangs.
    #[test]
    fn a_late_reader_after_an_early_close_gets_eof() {
        let tmp = TempDir::new().unwrap();
        let input = write_fixture(tmp.path());
        let o = opts();
        let sources = vec![ConvSource {
            input,
            kind: ConvKind::Ct,
            id_suffix: b"",
        }];
        let streamed = StreamedConversion::start(tmp.path(), false, &o, &sources, &[0]).unwrap();
        let path = streamed.path(0).to_path_buf();

        let result = within(60, move || {
            use std::io::Read;
            let mut sniff = [0u8; 16];
            let mut f = std::fs::File::open(&path).unwrap();
            f.read_exact(&mut sniff).unwrap();
            drop(f);
            while streamed.opened[0].load(Ordering::Acquire) != writer_state::REOPENING {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let late = std::fs::read(&path).unwrap();
            (late.len(), streamed.finish())
        });
        assert_eq!(result.0, 0, "the late reader sees an empty stream");
        assert!(
            result.1.is_err(),
            "a consumer that stopped reading fails the run"
        );
    }

    /// Paired-end shape: two sources (one per mate), two consumers each, one
    /// conversion pass per mate — four pipes, two passes.
    #[test]
    fn pe_fan_out_is_two_passes_four_pipes() {
        let tmp = TempDir::new().unwrap();
        let r1 = write_fixture(tmp.path());
        let r2 = {
            let p = tmp.path().join("reads_2.fastq");
            std::fs::copy(&r1, &p).unwrap();
            p
        };
        let o = opts();
        let sources = vec![
            ConvSource {
                input: r1.clone(),
                kind: ConvKind::Ct,
                id_suffix: b"/1/1",
            },
            ConvSource {
                input: r2.clone(),
                kind: ConvKind::Ga,
                id_suffix: b"/2/2",
            },
        ];
        // directional PE: slots 0 and 3, each reading (mate1 C→T, mate2 G→A).
        let demands = [0usize, 1, 0, 1];
        let streamed =
            StreamedConversion::start(tmp.path(), false, &o, &sources, &demands).unwrap();

        let readers: Vec<_> = (0..demands.len())
            .map(|c| {
                let p = streamed.path(c).to_path_buf();
                std::thread::spawn(move || std::fs::read(p).unwrap())
            })
            .collect();
        let got: Vec<Vec<u8>> = readers.into_iter().map(|h| h.join().unwrap()).collect();
        let counts = streamed.finish().unwrap();

        assert_eq!(
            counts,
            vec![20_000, 20_000],
            "one pass per mate, not per instance"
        );
        let m1 = std::fs::read(
            crate::aligner::convert::bisulfite_convert_fastq_pe(&r1, tmp.path(), &o, 1)
                .unwrap()
                .path,
        )
        .unwrap();
        let m2 = std::fs::read(
            crate::aligner::convert::bisulfite_convert_fastq_pe(&r2, tmp.path(), &o, 2)
                .unwrap()
                .path,
        )
        .unwrap();
        assert_eq!(got[0], m1);
        assert_eq!(got[1], m2);
        assert_eq!(got[2], m1);
        assert_eq!(got[3], m2);
    }

    #[test]
    fn fifo_names_are_slot_scoped_and_pid_scoped() {
        let a = fifo_name("x_C_to_T.fastq", 0, 0);
        let b = fifo_name("x_C_to_T.fastq", 3, 0);
        assert_ne!(a, b);
        assert!(a.starts_with("x_C_to_T."));
        assert!(a.contains(".fifo."));
        assert!(a.contains(&std::process::id().to_string()));
        assert!(
            a.ends_with(".fastq"),
            "the format extension must stay last, for tools that sniff by name: {a}"
        );
        assert!(fifo_name("noext", 0, 0).ends_with(".fifo"));
    }

    /// A long sample name must not push the pipe name past the filesystem's
    /// path-component limit.
    #[test]
    fn fifo_names_stay_short_for_any_input_name() {
        let long = format!("{}_C_to_T.fastq", "s".repeat(400));
        let name = fifo_name(&long, 3, 0);
        assert!(
            name.len() < 120,
            "capped name should stay well inside NAME_MAX, got {} bytes",
            name.len()
        );
        assert!(name.ends_with(".fastq"), "{name}");
        assert!(name.contains(".fifo."), "{name}");
        // Still recognisable: the leading run of the original name survives.
        assert!(name.starts_with("ssss"), "{name}");
    }

    /// Two concurrent chunks of the SAME long-named input must not agree on a path.
    ///
    /// Their names differ only by `.temp.<i>` at the end, which `NAME_STEM_CAP`
    /// truncates, so only the set id keeps the paths apart (see [`NEXT_FIFO_SET`]).
    #[test]
    fn concurrent_chunks_of_a_long_named_input_get_distinct_paths() {
        let long = "S".repeat(200);
        let chunk0 = format!("{long}.temp.0_C_to_T.fastq");
        let chunk1 = format!("{long}.temp.1_C_to_T.fastq");
        // Same slot, same process: only the set id can tell them apart.
        assert_eq!(
            fifo_name(&chunk0, 0, 7),
            fifo_name(&chunk1, 0, 7),
            "the truncated names DO collide; that is the premise of this test"
        );
        assert_ne!(
            fifo_name(&chunk0, 0, 7),
            fifo_name(&chunk1, 0, 8),
            "distinct sets must produce distinct paths regardless of the name"
        );
    }

    /// Every set id this process hands out is distinct, including across threads.
    #[test]
    fn fifo_set_ids_are_unique_per_process() {
        let handles: Vec<_> = (0..8)
            .map(|_| std::thread::spawn(|| (0..64).map(|_| next_fifo_set()).collect::<Vec<_>>()))
            .collect();
        let mut all: Vec<u64> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        let total = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), total, "set ids must never repeat");
    }

    /// The cap falls on a character boundary, so a non-ASCII name cannot panic.
    #[test]
    fn fifo_names_cap_on_a_char_boundary() {
        let long = format!("{}.fastq", "é".repeat(200));
        let name = fifo_name(&long, 0, 0);
        assert!(name.ends_with(".fastq"), "{name}");
        assert!(name.len() < 200, "got {} bytes", name.len());
    }
}
