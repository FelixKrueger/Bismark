//! FIFO plumbing for streaming the converted reads to the aligner (#1120).
//!
//! The in-silico converted reads (`_C_to_T` / `_G_to_A`) are today written to
//! `--temp_dir` in full before the aligner is spawned, which is the binding
//! constraint on a disk-limited scratch. They have exactly **one** consumer (the
//! aligner — the methylation caller re-reads the *original* FastQ,
//! [`super::mod`] `:3084`), so they can be handed over as FIFOs instead of files
//! and never touch the disk.
//!
//! This module owns only the named pipes themselves: create them under the run's
//! temp dir, hand out the paths the aligner argv needs, and unlink them again.
//! The converter/writer threads that fill them live in [`super::convert`]'s
//! streaming sibling; the routing decision lives in [`super::config`].
//!
//! **`mkfifo(1)`, not `libc::mkfifo`** (PLAN D-7): `libc` is only a transitive
//! dependency today, so declaring it would touch `rust/THIRD-PARTY-NOTICES.md`
//! — and a missing or refusing `mkfifo` is precisely the condition the
//! never-silent fallback to files exists to detect, so shelling out keeps that
//! fallback live code rather than dead code.
//!
//! **Not every backend can be streamed.** HISAT2 rejects a FIFO outright (exit
//! 255 — it stats/seeks while sniffing the format), so it keeps the file path.
//! See `plans/09112026_stream-converted/SPIKE.md`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::JoinHandle;

use crate::aligner::convert::{self, ConvKind, ConvertOptions, temp_dir_prefix};
use crate::aligner::error::{AlignerError, Result};

/// Create one named pipe at `path`.
///
/// Any pre-existing entry at `path` is removed first: the names are PID-scoped
/// (see [`FifoSet::create`]), so an entry already sitting there is a leftover of
/// a run that died before its [`FifoSet`] dropped, and `mkfifo` would otherwise
/// fail `EEXIST` on it forever.
fn mkfifo(path: &Path) -> Result<()> {
    let _ = std::fs::remove_file(path);
    let status = Command::new("mkfifo").arg(path).status().map_err(|e| {
        AlignerError::Validation(format!("could not run mkfifo for {}: {e}", path.display()))
    })?;
    if !status.success() {
        return Err(AlignerError::Validation(format!(
            "mkfifo {} failed ({status})",
            path.display()
        )));
    }
    Ok(())
}

/// A set of named pipes standing in for the converted temp files of one chunk.
///
/// One FIFO per *aligner instance per mate* — never one per converted file. Each
/// converted file has two concurrent readers in every library mode
/// (`se_instance_plan`, `pe_instance_plan`), and two readers cannot share one
/// pipe; the conversion pass is fanned out to them instead.
///
/// The pipes are unlinked on [`Drop`], including on the error path — a partially
/// created set drops as soon as `create` returns `Err`, so no caller has to
/// unwind it. Unlinking a FIFO does not disturb a process that already has it
/// open, so dropping early is safe.
#[derive(Debug)]
pub struct FifoSet {
    paths: Vec<PathBuf>,
}

impl FifoSet {
    /// Create one FIFO per entry of `names` under `temp_dir`, returning the set.
    ///
    /// `names` are file names, not paths; they are concatenated onto the
    /// normalised temp dir exactly as the converted temp files are
    /// (`temp_dir_prefix`), so the FIFOs land beside the files they replace. The
    /// caller is responsible for making each name unique within the run — see
    /// [`fifo_name`], which scopes them by PID so two Bismark runs sharing one
    /// `--temp_dir` cannot collide.
    pub fn create(temp_dir: &Path, names: &[String]) -> Result<Self> {
        let prefix = temp_dir_prefix(temp_dir)?;
        let mut set = FifoSet {
            paths: Vec::with_capacity(names.len()),
        };
        for name in names {
            let path = PathBuf::from(format!("{prefix}{name}"));
            mkfifo(&path)?;
            // Push only after mkfifo succeeded, so Drop never unlinks a path we
            // did not create (and so the failing path is not silently removed
            // out from under the diagnostic).
            set.paths.push(path);
        }
        Ok(set)
    }

    /// The created pipe paths, in `names` order — what goes into the aligner argv.
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

impl Drop for FifoSet {
    fn drop(&mut self) {
        for p in &self.paths {
            // Best-effort, as every other temp cleanup in the aligner is: a
            // failure here costs an empty inode, not correctness.
            let _ = std::fs::remove_file(p);
        }
    }
}

/// The FIFO file name standing in for converted temp file `converted_name` as
/// read by aligner instance `slot`.
///
/// Keeps the converted name visible (so a stray pipe is self-explaining) and
/// scopes it by slot and PID, because the same converted file feeds two or four
/// instances and each needs its own pipe.
///
/// The format extension stays **last** (`…_C_to_T.s0.4242.fifo.fastq`, not
/// `…_C_to_T.fastq.s0.4242.fifo`). Bowtie 2 and minimap2 sniff content rather
/// than names, but plenty of surrounding tooling does not — Bismark's own
/// converter picks its decompressor off the input's suffix — so a pipe that still
/// looks like the file it replaces is the safer shape for no extra cost.
pub fn fifo_name(converted_name: &str, slot: usize) -> String {
    let pid = std::process::id();
    match converted_name.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}.s{slot}.{pid}.fifo.{ext}"),
        None => format!("{converted_name}.s{slot}.{pid}.fifo"),
    }
}

/// Can `temp_dir` host a named pipe at all?
///
/// A `--temp_dir` on a filesystem without FIFO support, or a `mkfifo(1)` that is
/// not on `PATH`, is the never-silent fallback-to-files condition (PLAN D-2).
/// Answered by actually creating and unlinking one, because no portable
/// filesystem-capability query exists; `false` on any failure.
pub fn temp_dir_supports_fifo(temp_dir: &Path) -> bool {
    let name = format!("bismark_fifo_probe.{}", std::process::id());
    FifoSet::create(temp_dir, std::slice::from_ref(&name)).is_ok()
}

// ===========================================================================
// The streaming conversion itself: one conversion pass per source, fanned out
// over a bounded channel to one writer thread per consuming aligner instance.
//
//   R1.fq.gz ─> convert C→T ─┬─> [bounded chan] ─> writer ─> fifo_s0_m1 ─> bowtie2 --norc
//                            └─> [bounded chan] ─> writer ─> fifo_s3_m1 ─> bowtie2 --nofw
//
// CPU is one pass per source — exactly what the file path does today. Memory is
// bounded by the channel depth. The input is read once, so a non-seekable input
// behaves as before.
// ===========================================================================

/// How much converted output accumulates before it is handed to the writers.
///
/// Comfortably above the 64 KiB pipe buffer, so a writer's `write_all` is a
/// couple of pipe-sized transfers rather than per-record syscalls.
const BLOCK_BYTES: usize = 256 * 1024;

/// Blocks in flight per consumer. With [`BLOCK_BYTES`] this is ~1 MiB of slack
/// per aligner instance — enough that a momentarily slower instance does not
/// throttle its sibling, and small enough that the memory cost is noise next to
/// the aligner's index.
///
/// ponytail: a fixed depth, not a tuned one. The asymmetric-consumer wall-time
/// case (a `-p 1` instance beside a `-p 4` one) is unmeasured — proven
/// deadlock-free and byte-identical, not proven fast. This constant is the knob
/// if that measurement ever says so (PLAN D-6).
const CHANNEL_DEPTH: usize = 4;

/// One converted read stream to produce: an input file, a substitution, and the
/// paired-end read-number tag (`b""` for single-end).
///
/// The unit of *conversion work* — distinct from a consumer, which is one aligner
/// instance's view of one source. Two or four consumers share each source.
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
/// Every consumer gets the same bytes, so the aligner sees byte-for-byte what the
/// file path would have written for it.
struct FanOut {
    senders: Vec<SyncSender<Vec<u8>>>,
    buf: Vec<u8>,
}

impl FanOut {
    fn new(senders: Vec<SyncSender<Vec<u8>>>) -> Self {
        FanOut {
            senders,
            buf: Vec::with_capacity(BLOCK_BYTES + 4096),
        }
    }

    fn send_block(&mut self) -> std::io::Result<()> {
        let block = std::mem::replace(&mut self.buf, Vec::with_capacity(BLOCK_BYTES + 4096));
        for tx in &self.senders {
            // A closed receiver means that instance's writer thread gave up —
            // which only happens when its aligner died. Surface it; the child's
            // exit status is the useful half of the diagnostic and is reported
            // by `AlignerStream::finish`.
            tx.send(block.clone()).map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "an aligner instance stopped reading its converted reads",
                )
            })?;
        }
        Ok(())
    }
}

impl Write for FanOut {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        if self.buf.len() >= BLOCK_BYTES {
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
/// **Lifecycle, and why the order is not negotiable.** Opening a FIFO for write
/// blocks until a reader opens it, so [`start`](Self::start) leaves every writer
/// thread parked in `open`. The caller must then spawn *all* aligner children
/// before priming any of them ([`super::align::AlignerStream::spawn_unprimed`]);
/// each child's open releases its writer, and only then can bytes flow.
pub(crate) struct StreamedConversion {
    /// One FIFO path per consumer, in the caller's demand order.
    paths: Vec<PathBuf>,
    /// One converter thread per source; yields that source's record count.
    converters: Vec<JoinHandle<Result<u64>>>,
    /// One writer thread per consumer.
    writers: Vec<JoinHandle<Result<()>>>,
    /// Per consumer: receives one message once that writer's blocking
    /// open-for-write has returned — i.e. once something opened the pipe for
    /// read. Nothing received means the writer is still parked in `open`, which
    /// is what [`drain_unopened`](Self::drain_unopened) exists to resolve.
    opened: Vec<Receiver<()>>,
    /// Declared last so the pipes are unlinked *after* the thread handles are
    /// dropped (Rust drops fields in declaration order).
    _fifos: FifoSet,
}

impl StreamedConversion {
    /// Start converting `sources`, fanning each out to the consumers that want it.
    ///
    /// `demands[c]` is the index into `sources` that consumer `c` reads; the
    /// returned [`path`](Self::path)`(c)` is the FIFO to put in that instance's
    /// argv. Every source must have at least one consumer — one that does not is
    /// rejected rather than skipped, so [`finish`](Self::finish)'s counts stay
    /// aligned with `sources`.
    ///
    /// `--gzip` is deliberately ignored here: it exists to shrink the converted
    /// *file*, and there is no file. Compressing bytes that go straight into a
    /// pipe would cost CPU for nothing, so the FIFO names carry no `.gz` either.
    pub(crate) fn start(
        temp_dir: &Path,
        fasta: bool,
        opts: &ConvertOptions,
        sources: &[ConvSource],
        demands: &[usize],
    ) -> Result<Self> {
        // The FIFO is named after the file it replaces, minus the `.gz` (see above).
        let mut plain = opts.clone();
        plain.gzip = false;

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
            ));
        }

        let fifos = FifoSet::create(temp_dir, &names)?;
        let paths = fifos.paths().to_vec();

        // One writer thread per consumer, each parked in its blocking open-for-write
        // until the matching aligner child opens the pipe for read.
        let mut writers = Vec::with_capacity(demands.len());
        let mut opened = Vec::with_capacity(demands.len());
        let mut senders_by_source: Vec<Vec<SyncSender<Vec<u8>>>> = vec![Vec::new(); sources.len()];
        for (consumer, &src) in demands.iter().enumerate() {
            let (tx, rx) = sync_channel::<Vec<u8>>(CHANNEL_DEPTH);
            senders_by_source[src].push(tx);
            let path = paths[consumer].clone();
            let (opened_tx, opened_rx) = sync_channel::<()>(1);
            opened.push(opened_rx);
            writers.push(std::thread::spawn(move || {
                writer_thread(path, rx, opened_tx)
            }));
        }

        // One converter thread per source — every source, in order, so `finish`'s
        // counts line up with `sources` for the caller's banner. A source nobody
        // reads is a planning bug, not something to quietly skip: it would shift
        // every later count onto the wrong source.
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
                converter_thread(input, fasta, opts, kind, id_suffix, senders)
            }));
        }

        Ok(StreamedConversion {
            paths,
            converters,
            writers,
            opened,
            _fifos: fifos,
        })
    }

    /// The FIFO to hand aligner instance `consumer` — its `-U` / `-1` / `-2` path.
    pub(crate) fn path(&self, consumer: usize) -> &Path {
        &self.paths[consumer]
    }

    /// Join every writer and converter, returning the per-source record count in
    /// `sources` order (the `(N sequences)` the file path reports on its banner).
    ///
    /// Call this only after the merge has drained every aligner instance —
    /// joining earlier would wait for consumers that are not reading. On any
    /// error path, drop the value instead: the threads are detached and the pipes
    /// unlinked, and the process is on its way out.
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
    /// An aligner that exits without reading its input — a stub, a wrapper, or a
    /// real one that dies during start-up — leaves its writer blocked in
    /// open-for-write forever, and joining that writer would hang the whole run.
    /// It is safe to read the pipe ourselves at both call sites, because by then
    /// every aligner child has been reaped: there is no longer anyone the bytes
    /// could be stolen from, and the child's own exit status is what reports the
    /// failure.
    ///
    /// The `open` below cannot block: it is only reached when nothing has opened
    /// the pipe for read, which means the writer is (or is about to be) parked in
    /// open-for-write, and two opposing opens release each other.
    fn drain_unopened(&mut self) {
        for (rx, path) in self.opened.iter().zip(&self.paths) {
            if rx.try_recv().is_ok() {
                continue; // that writer is past its open; it will end on its own
            }
            if let Ok(mut f) = std::fs::File::open(path) {
                std::thread::spawn(move || {
                    let _ = std::io::copy(&mut f, &mut std::io::sink());
                });
            }
        }
    }
}

impl Drop for StreamedConversion {
    fn drop(&mut self) {
        // The error path: release any parked writer as above, then detach rather
        // than join. Detaching is sound because every caller propagates its error
        // up to `main`, which exits, and a finished `finish()` leaves nothing to
        // release (`writers` is empty). `_fifos` drops after this, unlinking the
        // pipes — which is why the release happens here and not afterwards.
        if !self.writers.is_empty() {
            self.drain_unopened();
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
/// The `open` blocks until the aligner child opens the pipe for read — that is the
/// handshake the whole ordering discipline exists to satisfy.
fn writer_thread(path: PathBuf, rx: Receiver<Vec<u8>>, opened: SyncSender<()>) -> Result<()> {
    let fifo = std::fs::OpenOptions::new().write(true).open(&path);
    // Report that the blocking open is behind us — success or failure — so
    // `drain_unopened` knows this writer no longer needs releasing.
    let _ = opened.send(());
    let mut fifo = fifo?;
    while let Ok(block) = rx.recv() {
        match fifo.write_all(&block) {
            Ok(()) => {}
            // The aligner stopped reading. Its exit status is the real
            // diagnostic (reported by the stream's `finish`), so end quietly
            // rather than burying it under an I/O error.
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => return Ok(()),
            Err(e) => return Err(e.into()),
        }
    }
    fifo.flush()?;
    Ok(())
}

/// Run one conversion pass, fanning it out to every consumer of that source.
///
/// Reuses [`convert::convert_fastq_into`] / [`convert::convert_fasta_into`]
/// verbatim — the same loop the file path runs — so the two paths cannot drift.
fn converter_thread(
    input: PathBuf,
    fasta: bool,
    opts: ConvertOptions,
    kind: ConvKind,
    id_suffix: &'static [u8],
    senders: Vec<SyncSender<Vec<u8>>>,
) -> Result<u64> {
    let mut reader = convert::open_reader(&input)?;
    let mut out = FanOut::new(senders);
    let (count, _seqid_tabs) = if fasta {
        convert::convert_fasta_into(&mut *reader, &opts, kind, id_suffix, &mut out)?
    } else {
        convert::convert_fastq_into(&mut *reader, &opts, kind, id_suffix, &mut out)?
    };
    out.flush()?;
    // Dropping the senders is what tells the writers there is no more input, so
    // they can close their end and let the aligner see EOF.
    drop(out);
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// A FIFO is neither a regular file nor a directory — enough to tell it from
    /// a `File::create` without pulling in the unix-only `FileTypeExt`.
    fn is_fifo(p: &Path) -> bool {
        let md = std::fs::symlink_metadata(p).expect("fifo should exist");
        !md.is_file() && !md.is_dir() && !md.file_type().is_symlink()
    }

    #[test]
    fn create_then_unlink_round_trip() {
        let tmp = TempDir::new().unwrap();
        let names = vec![
            fifo_name("r1_C_to_T.fastq", 0),
            fifo_name("r1_C_to_T.fastq", 1),
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
            fifo_name("ok.fastq", 0),
            format!("no_such_dir/{}", fifo_name("bad.fastq", 1)),
        ];
        assert!(FifoSet::create(tmp.path(), &names).is_err());
        assert!(!tmp.path().join(&names[0]).exists());
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

    /// A fixture big enough to need several [`BLOCK_BYTES`] blocks, so the
    /// bounded channel actually fills and both consumers have to be drained
    /// concurrently — the condition a single writer thread would deadlock on.
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

    /// Phase 2 gate: the bytes a consumer reads off its FIFO are exactly the
    /// bytes the file path would have written for it — for BOTH consumers of the
    /// same source, from ONE conversion pass.
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

        // Stand in for the aligner children: open every pipe for read BEFORE
        // draining any of them, exactly as the real ordering discipline requires.
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

    /// An aligner that never reads its input must not hang the run.
    ///
    /// A writer blocks in open-for-write until something opens the pipe for read,
    /// so a stub aligner (or a real one that dies at start-up) would park it
    /// forever and `finish` would wait on it for ever. This is the regression
    /// test for that hang — it deadlocked the whole `aligner_cli` suite, whose
    /// fake `bowtie2` prints SAM records without reading a byte.
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
        // Two consumers, NEITHER of which is ever opened — as two stub aligners
        // leave them. NB the fan-out couples consumers: one that never reads
        // eventually fills its channel and stalls the shared conversion pass, so
        // it starves its siblings too. That is inherent to a bounded fan-out and
        // harmless for real aligners, which all read their input. The hang this
        // guards against is the one that survives even after every aligner has
        // exited, which is where `finish` is called.
        let streamed = StreamedConversion::start(tmp.path(), false, &o, &sources, &[0, 0]).unwrap();
        let counts = streamed.finish().unwrap();
        assert_eq!(counts, vec![20_000]);
    }

    /// The same hang, on the error path: dropping without `finish` must not leave
    /// a parked writer behind either.
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

    /// `--gzip` shrinks a temp *file*; with no file it is a no-op, and the pipe
    /// carries the same plain bytes an ungzipped run would have written.
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
        let a = fifo_name("x_C_to_T.fastq", 0);
        let b = fifo_name("x_C_to_T.fastq", 3);
        assert_ne!(a, b);
        assert!(a.starts_with("x_C_to_T."));
        assert!(a.contains(".fifo."));
        assert!(a.contains(&std::process::id().to_string()));
        assert!(
            a.ends_with(".fastq"),
            "the format extension must stay last, for tools that sniff by name: {a}"
        );
        assert!(fifo_name("noext", 0).ends_with(".fifo"));
    }
}
