//! Phase F: rayon-based `--multicore N` pipeline (byte-identical to `--multicore 1`).
//!
//! Architecture per `SPEC.md §6.4 + §9` and `plans/05262026_bismark-extractor/PHASE_F_PLAN.md` rev 1:
//!
//! ```text
//!                 ┌─ worker 1 ─┐
//!  input BAM ──▶ producer ──▶ worker 2  ──▶ collector ──▶ split files + M-bias + report
//!                 └─ worker N ─┘             (main thread)
//! ```
//!
//! - **Producer** (single thread): drives the reader's `records()` (BAM uses a
//!   fixed 2-thread parallel-BGZF `ThreadedBamReader`, #884 R3; SAM/CRAM single-threaded),
//!   accumulates `WorkerInputItem::Se | Pe | Err` into batches of up to
//!   `BATCH_SIZE` (#884 R1), tags each batch with a monotonic `batch_seq`, and
//!   sends one `InputBatch` per batch into a bounded MPMC channel. For PE the
//!   producer also pairs adjacent records via `BismarkPair::from_mates`.
//! - **Workers** (N std::thread threads): receive a whole `InputBatch`, run
//!   `extract_calls` + `drop_overlap` + per-worker M-bias/counter
//!   accumulation for each item (one `WorkerOutputItem::Ok | Err` per input
//!   item — never short-circuiting), emit one `WorkerOutput::Batch` into a
//!   second bounded channel. Workers detect end-of-stream via channel-
//!   disconnect (`Err(RecvError)`) → emit `FinalDelta` — no sentinel messages.
//! - **Collector** (main thread): owns `ExtractState`. Reorders worker
//!   output by `batch_seq` via a `BTreeMap` and emits each batch's items in
//!   `Vec` order — globally a `(batch_seq, within_idx)` total order isomorphic
//!   to the old per-record `input_idx`, so byte-identity to the legacy
//!   single-threaded path is preserved. Sums per-worker M-bias / SplittingReport
//!   deltas at end-of-stream. Deterministic Err selection: lowest
//!   `(batch_seq, within_idx)` wins.
//!
//! # Byte-identity invariant
//!
//! `--multicore N` output MUST equal `--multicore 1` output for any N ≥ 1
//! on the same input. This is the load-bearing test surface for Phase H.
//! The mechanisms that hold this invariant:
//!
//! 1. **Input ordering**: producer fills batches in strict input order with a
//!    monotonic `batch_seq`; collector emits in `batch_seq` order via `BTreeMap`,
//!    items within a batch in `Vec` order. `(batch_seq, within_idx)` is
//!    order-isomorphic to the old per-record `input_idx` (#884 R1: batching
//!    changes only the reorder *granularity*, not the order).
//! 2. **M-bias merge**: `MbiasTable::add` is commutative + associative.
//! 3. **Counter merge**: `SplittingReport::add` is commutative + associative.
//! 4. **Err selection**: collector picks the lowest-`(batch_seq, within_idx)`
//!    Err, so stderr is byte-identical even on multi-error inputs.
//! 5. **Single-writer-per-file**: the collector is single-threaded; each
//!    output file is touched by only one writer. Gzip footers land in a
//!    contiguous stream per file.
//!
//! # Deviation from `PHASE_F_PLAN.md` rev 1 §2: `std::thread::spawn` workers
//!
//! Rev 1 specified rayon for the worker pool. During implementation a
//! deadlock surfaced: `rayon::ThreadPool::scope()` consumes one of the
//! pool's threads to run its closure, so with `num_threads(N)` the scope
//! closure occupies one thread and only N-1 workers can run concurrently
//! with it. At N=1 (the `--multicore` default), this means **zero workers
//! can run while the collector is alive inside the scope closure**.
//! Result: deadlock — workers wait for input, producer waits to send,
//! collector waits for output that never arrives.
//!
//! The implementation uses `std::thread::spawn` for workers instead of
//! `rayon::ThreadPool`. Functionally identical for our pattern (N managed
//! threads with panic propagation via `JoinHandle`); we don't use rayon's
//! work-stealing or `par_iter`. The rayon dep was added then **removed**
//! during the post-implementation review pass — both code-reviewers
//! independently flagged the dep as dead. If a future Phase F polish
//! needs rayon (e.g. `par_iter` over per-file collector workers), it can
//! be re-added then.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use bismark_io::{
    AlignmentKind, BismarkIoError, BismarkPair, BismarkRecord, BismarkStrand, ThreadedBamReader,
    open_reader, open_reader_without_sort_check,
};
use crossbeam_channel::{Receiver, RecvError, Sender, bounded};

use crate::call::{CytosineContext, MethCall, extract_calls};
use crate::cli::ResolvedConfig;
use crate::error::BismarkExtractorError;
use crate::header::build_chr_name_table;
use crate::mbias::MbiasTable;
use crate::output::SplittingReport;
use crate::overlap::drop_overlap;
use crate::pipeline::derive_basename;
use crate::route::compute_yacht_columns;
use crate::state::ExtractState;

// ─── Channel message types ───────────────────────────────────────────────────

/// #884 R1: number of records (SE) / pairs (PE) accumulated per channel
/// message. Borrowed from TG-OE's proven FASTQ-batching pattern. This is a
/// **coordination / throughput knob, NOT a correctness one**: output bytes are
/// independent of the value because the collector emits batches in `batch_seq`
/// order and records within a batch in `Vec` order — together a total order
/// isomorphic to the old per-record `input_idx` order. Tunable; 4096 is a safe
/// default. NOTE (rev 1 reviewers): TG-OE batches a single FASTQ stream, so the
/// *pattern* transfers, not its exact per-batch memory profile — our payload is
/// a `Vec<RoutedCall>` fanning to ≤12 output files.
pub(crate) const BATCH_SIZE: usize = 4096;

/// #884 R3: decode worker threads for the parallel-BGZF BAM reader.
///
/// **Fixed at 2, decoupled from `--parallel` by design.** Single-threaded BGZF
/// decode is the pipeline's ~19 s ceiling — the extract workers sit idle behind
/// it (CPU probe: ~2.8 cores used regardless of `--parallel`). An oxy trial
/// (10M PE) measured **2 decode threads as the sweet spot**: `--mbias_only`
/// 18.8→12.3 s, plain `.txt` 20.0→17.6 s; 3–4 threads add nothing (even regress).
/// Fixing it at 2 (vs tying to `--parallel`) lets the common `--parallel 1`
/// default benefit — same rationale shape as `output.rs::GZIP_COMPRESS_THREADS`.
/// Applies to BAM only (BGZF); SAM/CRAM keep the single-threaded reader.
const DECODE_THREADS: std::num::NonZeroUsize = std::num::NonZeroUsize::new(2).unwrap();

/// One unit of producer→worker work: a single SE record, a single PE pair, or
/// a producer-side error that keeps its input slot. EOS is signaled by the
/// producer dropping its sender (channel-disconnect-as-EOS, plan rev 1) — no
/// sentinel variant needed.
pub(crate) enum WorkerInputItem {
    /// Single-end record. Worker resolves the chromosome name via the shared
    /// `chr_table` keyed by `chr_id`.
    Se { record: BismarkRecord, chr_id: u32 },
    /// Paired-end pair (one item per pair). Pair is already validated by
    /// `BismarkPair::from_mates` (qname-eq; paired by file order, not the R1/R2
    /// FLAG bits — #1030). `Box` keeps the
    /// enum size proportional to the smallest variant
    /// (clippy::large_enum_variant); BismarkPair is ~2× BismarkRecord.
    Pe { pair: Box<BismarkPair>, chr_id: u32 },
    /// Error encountered by the producer (read error, unpaired final record,
    /// pairing error, refid overflow / missing). Carried as a per-item result
    /// so it keeps its within-batch slot for deterministic Err selection.
    Err { error: BismarkExtractorError },
}

/// Producer → worker channel message: a batch of up to [`BATCH_SIZE`] items,
/// tagged with a monotonic `batch_seq`. The collector reorders by `batch_seq`.
pub(crate) struct InputBatch {
    pub batch_seq: u64,
    pub items: Vec<WorkerInputItem>,
}

/// Per-item worker result. The worker emits **exactly one** of these per input
/// item (`results.len() == items.len()`) — it never short-circuits a batch, so
/// within-batch indices line up 1:1 between input and output (the invariant
/// that makes `(batch_seq, within_idx)` order-isomorphic to the old
/// `input_idx`).
pub(crate) enum WorkerOutputItem {
    /// Result of processing one `WorkerInputItem::Se` or `::Pe`.
    /// `routed_calls` is empty under `--mbias_only` (worker still accumulates
    /// M-bias + counters locally but doesn't ship calls).
    Ok { routed_calls: Vec<RoutedCall> },
    /// Error during extraction (`InvalidXmByte`, `drop_overlap` failure, etc.)
    /// OR a forwarded `WorkerInputItem::Err`. The collector selects the
    /// lowest-`(batch_seq, within_idx)` Err for stable stderr.
    Err { error: BismarkExtractorError },
}

/// Worker → collector channel message.
pub(crate) enum WorkerOutput {
    /// Results for one [`InputBatch`], tagged with the same `batch_seq`.
    /// `results[k]` corresponds to `items[k]` (1:1, in order).
    Batch {
        batch_seq: u64,
        results: Vec<WorkerOutputItem>,
    },
    /// Sent exactly once by each worker at exit (after `recv()` returns
    /// `Err(Disconnected)`). Carries this worker's accumulated counters.
    /// Semantics UNCHANGED from the per-record design.
    FinalDelta {
        mbias: [MbiasTable; 2],
        report: SplittingReport,
    },
}

/// Pre-routed call ready for the collector to write. Per-record qname is
/// shared across the record's calls via `Arc` (pointer-clone, not byte-
/// copy); chr is sent as an id and resolved at the collector via the
/// shared `chr_table`.
///
/// Note: the `OutputKey` is NOT carried explicitly. The collector dispatches
/// via `OutputFileMap::write_call` which internally routes by `(self.mode,
/// call.context, strand)` — same path the single-threaded writer takes.
/// Pre-computing the key in the worker would only save a tiny match per
/// call and would duplicate routing logic; defer until profiling justifies.
pub(crate) struct RoutedCall {
    pub call: MethCall,
    pub strand: BismarkStrand,
    pub yacht_col6: u32,
    pub yacht_col7: u32,
    pub qname: Arc<[u8]>,
    pub chr_id: u32,
}

// ─── Public entry points ─────────────────────────────────────────────────────

/// SE extraction with N rayon workers. `config.parallel` selects N
/// (validated `>= 1` in `Cli::validate`). Byte-identical to the
/// legacy single-threaded [`crate::extract_se`] for any N.
pub fn extract_se_parallel(
    input: &Path,
    config: &ResolvedConfig,
) -> Result<(), BismarkExtractorError> {
    run_pipeline(input, config, /*is_paired=*/ false)
}

/// PE extraction with N rayon workers. Pair-formation happens in the
/// producer thread (workers receive pre-formed `BismarkPair` messages).
pub fn extract_pe_parallel(
    input: &Path,
    config: &ResolvedConfig,
) -> Result<(), BismarkExtractorError> {
    run_pipeline(input, config, /*is_paired=*/ true)
}

// ─── Pipeline driver ─────────────────────────────────────────────────────────

fn run_pipeline(
    input: &Path,
    config: &ResolvedConfig,
    is_paired: bool,
) -> Result<(), BismarkExtractorError> {
    // #884 R3: BAM decode uses the fixed-2-thread parallel-BGZF reader (always);
    // SAM/CRAM (not BGZF) keep the single-threaded reader. Sniffed once, reused.
    let is_bam = matches!(AlignmentKind::from_path(input)?, AlignmentKind::Bam);

    // #884 R3 (perf gate, oxy 10M PE): floor BAM extract workers at 2 — a single
    // extract worker can't drain the 2-thread parallel decode (`--parallel 1` on
    // BAM measured ~18.5 s plain / ~16 s `--mbias_only`, vs ~17.6 s / ~12.3 s with
    // ≥2 workers), so the common `--parallel 1` default benefits fully. SAM/CRAM
    // (single-threaded decode) keep `max(1)` — extra workers can't beat serial decode.
    // Output is byte-identical across worker counts (batch_seq reorder), so this
    // floor changes timing only, not bytes.
    let n_workers = config.parallel.max(if is_bam { 2 } else { 1 });

    // Open the reader on the main thread to get the header (for chr_table)
    // before we hand the reader off to the producer thread.
    //
    // #884 R3: BAM decode uses a fixed-2-thread parallel-BGZF reader
    // (`ThreadedBamReader`), ALWAYS — independent of `--parallel` (see
    // `DECODE_THREADS`). Single-threaded BGZF decode was the ~19 s pipeline
    // ceiling; 2 decode threads drop the `--mbias_only` wall 18.8→12.3 s and the
    // plain `.txt` wall 20.0→17.6 s, so even `--parallel 1` benefits. SAM/CRAM are not
    // BGZF → keep the single-threaded reader.
    //
    // Coordinate-sort policy (v1.x): the SE/PE distinction IS the policy —
    // `is_paired` is already in scope here. For PAIRED-END we use the
    // checking constructors (`ThreadedBamReader::from_path` / `open_reader`):
    // coordinate-sorted PE input breaks adjacent-mate pairing and is rejected
    // with `UnsortedInput`. For SINGLE-END we use the `*_without_sort_check`
    // constructors: SE methylation calls are order-independent, so
    // coordinate-sorted input is valid — faithful to Perl
    // `bismark_methylation_extractor`, which only sort-checks paired-end input
    // (`test_positional_sorting` is gated `if ($paired)`).
    let reader = if is_bam {
        if is_paired {
            ProducerReader::Threaded(ThreadedBamReader::from_path(input, DECODE_THREADS)?)
        } else {
            ProducerReader::Threaded(ThreadedBamReader::from_path_without_sort_check(
                input,
                DECODE_THREADS,
            )?)
        }
    } else if is_paired {
        ProducerReader::Any(open_reader(input, /*cram_ref=*/ None)?)
    } else {
        ProducerReader::Any(open_reader_without_sort_check(
            input, /*cram_ref=*/ None,
        )?)
    };
    let chr_table: Arc<[String]> = Arc::from(build_chr_name_table(reader.header())?);

    // Console diagnostics (#882) — emit once on the main thread while we still
    // hold the reader (it is moved into the producer below). All to stderr,
    // gated by --quiet. The final methylation summary is emitted later, inside
    // `state.finalize`, where the SplittingReport counts are finalized.
    let logger = crate::logging::Logger::from_config(config);
    logger.banner();
    logger.parameters(config, is_paired);
    logger.header_provenance(reader.header());

    let input_basename = derive_basename(input);
    let mut state = ExtractState::new(config, input, &input_basename, is_paired)?;

    // Bounded channels per SPEC §9.2. #884 R1: capacities are now in **batches**
    // (each ≤ BATCH_SIZE records), not records. The pre-batching depths
    // (input n*32, output n*8) measured records, so with 4096-record batches
    // they would buffer ~131k/~33k records per worker — far more than needed.
    // Retuned to small batch counts (n*4 each): at N=8 that bounds in-flight
    // input to ≈ 8*4*4096 ≈ 131k records and output similarly, while keeping
    // the N=1 no-deadlock property (capacity ≥ 1; topology unchanged —
    // dedicated producer + std::thread workers + collector on main).
    let (tx_input, rx_input) = bounded::<InputBatch>(n_workers * 4);
    let (tx_output, rx_output) = bounded::<WorkerOutput>(n_workers * 4);

    // Spawn N worker threads via std::thread::spawn (NOT rayon — see
    // module docs: rayon::ThreadPool::scope() deadlocks at N=1 because
    // the scope closure occupies a pool thread).
    let worker_handles: Vec<std::thread::JoinHandle<()>> = (0..n_workers)
        .map(|i| {
            let rx_input = rx_input.clone();
            let tx_output = tx_output.clone();
            let config_clone = config.clone();
            let chr_table_clone = Arc::clone(&chr_table);
            std::thread::Builder::new()
                .name(format!("bismark-extractor-worker-{i}"))
                .spawn(move || {
                    worker_loop(rx_input, tx_output, config_clone, chr_table_clone);
                })
                .map_err(|e| BismarkExtractorError::InternalError {
                    message: format!("failed to spawn worker thread {i}: {e}"),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Drop the main thread's copies of the channel handles so that
    // when the producer + all workers finish, channels disconnect cleanly.
    drop(rx_input);
    drop(tx_output);

    // Spawn the producer on a dedicated thread.
    let producer_tx_input = tx_input.clone();
    let producer_handle = std::thread::Builder::new()
        .name("bismark-extractor-producer".to_string())
        .spawn(move || {
            producer_loop(reader, is_paired, producer_tx_input, logger);
        })
        .map_err(|e| BismarkExtractorError::InternalError {
            message: format!("failed to spawn producer thread: {e}"),
        })?;
    drop(tx_input); // main thread's copy — producer owns the survivor.

    // Run collector loop on main thread. Returns a `CollectorOutcome`
    // (not Result) so the join-sequence below can pick the most-specific
    // error message — see Reviewer B C2 fix.
    let collector_outcome = collector_loop(&mut state, rx_output, &chr_table, n_workers);

    // Join workers BEFORE producer so worker panic info is available when
    // we decide error precedence. Capture the first worker panic message.
    let mut worker_panic: Option<String> = None;
    for handle in worker_handles {
        if let Err(panic_payload) = handle.join()
            && worker_panic.is_none()
        {
            worker_panic = Some(format!("worker thread panicked: {panic_payload:?}"));
        }
    }

    // Join producer last (it was almost certainly already done by the time
    // collector exited — its tx_input drop is what signaled EOS).
    let producer_join_result = producer_handle.join();

    // Apply error precedence (Reviewer B C2 fix):
    //   1. Collector caught an explicit Err → use it (most specific —
    //      e.g. `InvalidXmByte` from `extract_calls`).
    //   2. Workers' FinalDeltas are missing AND a worker panicked →
    //      worker panic is the root cause of the missing delta; use it.
    //   3. Workers' FinalDeltas are missing AND no panic detected →
    //      synthetic "missing FinalDelta" error (no better info).
    //   4. Producer panicked → use producer panic.
    //   5. Worker panicked (collector clean) → use worker panic.
    //   6. Else Ok.
    let missing_deltas = collector_outcome.finaldeltas_received < n_workers;
    let pipeline_result = if let Some(e) = collector_outcome.best_err {
        Err(e)
    } else if missing_deltas {
        if let Some(msg) = worker_panic {
            // Root cause for the missing-FinalDelta: a worker panicked.
            Err(BismarkExtractorError::InternalError { message: msg })
        } else if let Err(panic_payload) = producer_join_result {
            // Or the producer panicked, taking workers with it.
            Err(BismarkExtractorError::InternalError {
                message: format!("producer thread panicked: {panic_payload:?}"),
            })
        } else {
            // No panic detected — surface the synthetic error (shouldn't
            // happen in practice but defensive).
            Err(BismarkExtractorError::InternalError {
                message: format!(
                    "collector received only {} of {n_workers} FinalDeltas before \
                     channel disconnect — a worker exited unexpectedly without \
                     a JoinHandle::join() error",
                    collector_outcome.finaldeltas_received
                ),
            })
        }
    } else if let Some(msg) = worker_panic {
        // Clean shutdown but a worker panicked AFTER its FinalDelta (rare
        // but possible if a worker does work in Drop).
        Err(BismarkExtractorError::InternalError { message: msg })
    } else if let Err(panic_payload) = producer_join_result {
        Err(BismarkExtractorError::InternalError {
            message: format!("producer thread panicked: {panic_payload:?}"),
        })
    } else {
        Ok(())
    };

    match pipeline_result {
        Err(e) => {
            state.cleanup_partial_outputs();
            Err(e)
        }
        Ok(()) => {
            state.finalize(config)?;
            Ok(())
        }
    }
}

// ─── Producer ────────────────────────────────────────────────────────────────

/// Producer-side reader (#884 R3): the single-threaded [`AnyReader`] (SAM/CRAM)
/// or the fixed-2-thread parallel-BGZF [`ThreadedBamReader`] (BAM). Both expose
/// `header()` + `records()` yielding the same `Result<BismarkRecord, _>`; this
/// enum lets `run_pipeline` and `producer_loop` own and drive either uniformly,
/// so neither body changes vs the single-reader version.
enum ProducerReader {
    Any(bismark_io::AnyReader<std::io::BufReader<std::fs::File>, std::fs::File>),
    Threaded(ThreadedBamReader),
}

impl ProducerReader {
    fn header(&self) -> &noodles_sam::Header {
        match self {
            ProducerReader::Any(r) => r.header(),
            ProducerReader::Threaded(r) => r.header(),
        }
    }

    fn records(&mut self) -> Box<dyn Iterator<Item = Result<BismarkRecord, BismarkIoError>> + '_> {
        match self {
            ProducerReader::Any(r) => Box::new(r.records()),
            ProducerReader::Threaded(r) => Box::new(r.records()),
        }
    }
}

/// Producer loop: drive the reader's `records()` iterator, accumulate
/// [`WorkerInputItem`]s into batches of up to [`BATCH_SIZE`] (in strict input
/// order, tagged with a monotonic `batch_seq`), and send one [`InputBatch`] per
/// full batch. The partial final batch is flushed at clean EOF. EOS is signaled
/// by dropping `tx_input` (happens automatically when this function returns).
///
/// #884 R1 error handling (rev 1, both reviewers Critical): on any producer-side
/// error the `Err` item is appended to the CURRENT (partial) batch and that
/// batch is flushed before the producer returns — so the good lower-index items
/// already buffered in the partial batch still ship (today they are emitted
/// before the producer short-circuits; dropping them would break byte-identity
/// on error inputs). The producer still `return`s after the first error.
fn producer_loop(
    mut reader: ProducerReader,
    is_paired: bool,
    tx_input: Sender<InputBatch>,
    logger: crate::logging::Logger,
) {
    // `lines_read` counts every SAM record consumed (one per `records_iter.next()`
    // that yields a record): +1 per SE record, +2 per PE pair (R1 then R2). This
    // matches Perl's read-side `$line_count` (warned every 500k at
    // `bismark_methylation_extractor:1553`) byte-for-byte. Single producer
    // thread → plain local counter, no atomic. (#882) The tick stays
    // per-SAM-record-read regardless of batch boundaries — moving it per-batch
    // would change the stderr cadence and break byte-identity.
    let mut lines_read: u64 = 0;
    fn tick(logger: &crate::logging::Logger, lines_read: &mut u64) {
        *lines_read += 1;
        if (*lines_read).is_multiple_of(500_000) {
            logger.progress(*lines_read);
        }
    }

    // Batch accumulation state. `batch_seq` is the monotonic reorder key; item k
    // of batch s corresponds to the old `input_idx = s*BATCH_SIZE + k`.
    let mut batch_seq: u64 = 0;
    let mut items: Vec<WorkerInputItem> = Vec::with_capacity(BATCH_SIZE);

    // Send the current batch (taking ownership of `items`, leaving a fresh
    // empty Vec) under the current `batch_seq`. Returns `false` if the channel
    // is gone. Does NOT bump `batch_seq` — terminal (error/EOF) flushes are
    // immediately followed by `return`, so bumping there would be a dead store
    // (clippy -D warnings). The two mid-loop full-batch flushes that continue
    // the loop bump `batch_seq` explicitly at their call site.
    macro_rules! flush_batch {
        () => {{
            let batch = InputBatch {
                batch_seq,
                items: std::mem::take(&mut items),
            };
            tx_input.send(batch).is_ok()
        }};
    }

    let mut records_iter = reader.records();

    if !is_paired {
        // SE: one item per record.
        loop {
            match records_iter.next() {
                Some(Ok(record)) => {
                    tick(&logger, &mut lines_read); // +1 SAM line (#882)
                    // Resolve reference_sequence_id → u32 with defensive
                    // try_from (Reviewer B.H1: `as u32` would silently
                    // truncate above 2^32 contigs; matches the precedent
                    // in `compute_yacht_columns`).
                    let chr_id_result: Result<Option<u32>, _> = record
                        .inner()
                        .reference_sequence_id()
                        .map(u32::try_from)
                        .transpose();
                    match chr_id_result {
                        Ok(Some(chr_id)) => {
                            items.push(WorkerInputItem::Se { record, chr_id });
                        }
                        Ok(None) => {
                            // Producer-side error: append to current partial
                            // batch, flush it, then stop.
                            items.push(WorkerInputItem::Err {
                                error: BismarkExtractorError::InternalError {
                                    message: "mapped record has no reference_sequence_id; \
                                              bismark-io::records should have filtered this \
                                              as unmapped (FLAG & 0x4)"
                                        .to_string(),
                                },
                            });
                            let _ = flush_batch!();
                            return;
                        }
                        Err(_) => {
                            items.push(WorkerInputItem::Err {
                                error: BismarkExtractorError::InternalError {
                                    message: "reference_sequence_id overflows u32 \
                                              (>= 2^32 contigs in header)"
                                        .to_string(),
                                },
                            });
                            let _ = flush_batch!();
                            return;
                        }
                    }
                    if items.len() >= BATCH_SIZE {
                        if !flush_batch!() {
                            return; // all workers gone
                        }
                        batch_seq += 1; // mid-loop flush continues → next seq
                    }
                }
                Some(Err(e)) => {
                    items.push(WorkerInputItem::Err { error: e.into() });
                    let _ = flush_batch!();
                    return;
                }
                None => break, // clean EOF
            }
        }
    } else {
        // PE: take adjacent records, pair them on the producer thread; one item
        // per pair.
        loop {
            // R1
            let r1 = match records_iter.next() {
                Some(Ok(r)) => {
                    tick(&logger, &mut lines_read); // +1 SAM line (R1) (#882)
                    r
                }
                Some(Err(e)) => {
                    items.push(WorkerInputItem::Err { error: e.into() });
                    let _ = flush_batch!();
                    return;
                }
                None => break, // clean EOF
            };
            // R2
            let r2 = match records_iter.next() {
                Some(Ok(r)) => {
                    tick(&logger, &mut lines_read); // +1 SAM line (R2) (#882)
                    r
                }
                Some(Err(e)) => {
                    items.push(WorkerInputItem::Err { error: e.into() });
                    let _ = flush_batch!();
                    return;
                }
                None => {
                    let qname = r1
                        .inner()
                        .name()
                        .map(|n| String::from_utf8_lossy(n.as_ref()).into_owned());
                    items.push(WorkerInputItem::Err {
                        error: BismarkExtractorError::UnpairedFinalRecord { qname },
                    });
                    let _ = flush_batch!();
                    return;
                }
            };
            // Pair-formation
            let pair = match BismarkPair::from_mates(r1, r2) {
                Ok(p) => p,
                Err(e) => {
                    items.push(WorkerInputItem::Err { error: e.into() });
                    let _ = flush_batch!();
                    return;
                }
            };
            // Resolve R1 refid → u32 with defensive try_from (Reviewer B.H1).
            let chr_id: u32 = match pair.r1().inner().reference_sequence_id() {
                Some(r) => match u32::try_from(r) {
                    Ok(v) => v,
                    Err(_) => {
                        items.push(WorkerInputItem::Err {
                            error: BismarkExtractorError::InternalError {
                                message: format!("PE R1 reference_sequence_id {r} overflows u32"),
                            },
                        });
                        let _ = flush_batch!();
                        return;
                    }
                },
                None => {
                    items.push(WorkerInputItem::Err {
                        error: BismarkExtractorError::InternalError {
                            message: "PE R1 missing reference_sequence_id".to_string(),
                        },
                    });
                    let _ = flush_batch!();
                    return;
                }
            };
            items.push(WorkerInputItem::Pe {
                pair: Box::new(pair),
                chr_id,
            });
            if items.len() >= BATCH_SIZE {
                if !flush_batch!() {
                    return; // all workers gone
                }
                batch_seq += 1; // mid-loop flush continues → next seq
            }
        }
    }

    // Flush the partial final batch at clean EOF (skip an empty trailing batch —
    // empty input produces zero batches, matching N=1's header-only finalize).
    if !items.is_empty() {
        let _ = flush_batch!();
    }
    // tx_input drops as this function returns → channel disconnects → workers exit.
    drop(tx_input);
}

// ─── Worker ──────────────────────────────────────────────────────────────────

/// Worker loop: process [`InputBatch`] messages, accumulate per-worker
/// M-bias + counters, emit one [`WorkerOutput::Batch`] per input batch. Exits
/// on channel-disconnect EOS by emitting a `FinalDelta`.
///
/// #884 R1: a batch is processed as a whole — for each item the worker produces
/// exactly one [`WorkerOutputItem`] (`results.len() == items.len()`), **never
/// short-circuiting on a per-item Err**. This 1:1 correspondence keeps
/// within-batch indices aligned input↔output, the invariant behind byte-identity
/// (lowest-`(batch_seq, within_idx)` Err == lowest old `input_idx`).
fn worker_loop(
    rx_input: Receiver<InputBatch>,
    tx_output: Sender<WorkerOutput>,
    config: ResolvedConfig,
    chr_table: Arc<[String]>,
) {
    let mut mbias = [MbiasTable::default(), MbiasTable::default()];
    let mut report = SplittingReport::default();
    // One source of truth (Reviewer A.M / Reviewer B.H4: rev 0 had the
    // value duplicated into two bindings — `mbias_only_silence` and
    // `mbias_only` — that always held the same value but had different
    // semantic meanings at their call sites).
    //
    // The flag drives two distinct behaviours under `--mbias_only`:
    //   1. `extract_calls(..., /*mbias_only_silence=*/ mbias_only)` —
    //      kernel silences `InvalidXmByte` errors (Phase E semantics).
    //   2. `process_se` / `process_pe` skip RoutedCall emission to save
    //      channel traffic (Phase F G7 optimisation).
    // Both behaviours fire iff `OutputMode::MbiasOnly`, so one binding
    // is correct today. If the semantics ever diverge (e.g. a future
    // `--silence_invalid_xm_only` flag), introduce a second binding here.
    let mbias_only = config.is_mbias_only();

    loop {
        match rx_input.recv() {
            Ok(InputBatch { batch_seq, items }) => {
                let mut results: Vec<WorkerOutputItem> = Vec::with_capacity(items.len());
                for item in items {
                    let result_item = match item {
                        WorkerInputItem::Se { record, chr_id } => {
                            match process_se(
                                &record,
                                chr_id,
                                &chr_table,
                                &config,
                                mbias_only,
                                mbias_only,
                                &mut mbias,
                                &mut report,
                            ) {
                                Ok(routed_calls) => WorkerOutputItem::Ok { routed_calls },
                                Err(error) => WorkerOutputItem::Err { error },
                            }
                        }
                        WorkerInputItem::Pe { pair, chr_id } => {
                            match process_pe(
                                &pair,
                                chr_id,
                                &chr_table,
                                &config,
                                mbias_only,
                                mbias_only,
                                &mut mbias,
                                &mut report,
                            ) {
                                Ok(routed_calls) => WorkerOutputItem::Ok { routed_calls },
                                Err(error) => WorkerOutputItem::Err { error },
                            }
                        }
                        // Forward a producer-side error; never short-circuit the
                        // batch (preserves within-batch index alignment).
                        WorkerInputItem::Err { error } => WorkerOutputItem::Err { error },
                    };
                    results.push(result_item);
                }
                if tx_output
                    .send(WorkerOutput::Batch { batch_seq, results })
                    .is_err()
                {
                    return;
                }
            }
            Err(RecvError) => {
                // Channel disconnected (EOS). Emit FinalDelta and exit.
                let _ = tx_output.send(WorkerOutput::FinalDelta { mbias, report });
                return;
            }
        }
    }
}

/// Process a single SE record on the worker thread. Accumulates M-bias +
/// counters into the worker-local `mbias` / `report`; returns `RoutedCall`s
/// (empty under `--mbias_only`).
#[allow(clippy::too_many_arguments)]
fn process_se(
    record: &BismarkRecord,
    chr_id: u32,
    chr_table: &Arc<[String]>,
    config: &ResolvedConfig,
    mbias_only_silence: bool,
    mbias_only: bool,
    mbias: &mut [MbiasTable; 2],
    report: &mut SplittingReport,
) -> Result<Vec<RoutedCall>, BismarkExtractorError> {
    // Defensive PAIRED-flag check, matching Phase B's extract_se behaviour.
    let flags_bits: u16 = record.inner().flags().into();
    if flags_bits & 0x1 != 0 {
        return Err(BismarkExtractorError::PhaseNotYetImplemented {
            feature: "paired-end extraction (input has PAIRED flag set); \
                      PE arrives in Phase C"
                .to_string(),
        });
    }

    // Sanity-check chr_id against chr_table (collector will error otherwise
    // too, but checking here gives a more specific error message).
    if (chr_id as usize) >= chr_table.len() {
        return Err(BismarkExtractorError::InternalError {
            message: format!(
                "chr_id {} out of range vs header (count {})",
                chr_id,
                chr_table.len()
            ),
        });
    }

    let strand = record.record_strand();
    let calls = extract_calls(
        record,
        config.ignore_5p_r1,
        config.ignore_3p_r1,
        mbias_only_silence,
    )?;

    let mode = config.output_mode;
    let qname_arc: Arc<[u8]> = qname_arc_for(record);
    let mut routed_calls: Vec<RoutedCall> = if mbias_only {
        Vec::new()
    } else {
        Vec::with_capacity(calls.len())
    };

    for call in calls {
        // M-bias accumulate (idx 0 for SE).
        if !config.mbias_off {
            let pos_1based = call.read_pos.saturating_add(1);
            mbias[0].accumulate(call.context, pos_1based, call.methylated);
        }
        increment_counters(report, call);

        if mbias_only {
            // Plan rev 1 §2: under --mbias_only, worker skips RoutedCall emission.
            continue;
        }

        let (yacht_col6, yacht_col7) = compute_yacht_columns(mode, record, strand)?;
        routed_calls.push(RoutedCall {
            call,
            strand,
            yacht_col6,
            yacht_col7,
            qname: Arc::clone(&qname_arc),
            chr_id,
        });
    }

    // SE: records_processed and call_strings_processed both increment by 1
    // per record (Perl `sequences_count` == `methylation_call_strings` for
    // SE). Phase C.2 (#864) addition for the second counter.
    report.records_processed = report.records_processed.saturating_add(1);
    report.call_strings_processed = report.call_strings_processed.saturating_add(1);

    Ok(routed_calls)
}

/// Process a single PE pair on the worker thread.
#[allow(clippy::too_many_arguments)]
fn process_pe(
    pair: &BismarkPair,
    chr_id: u32,
    chr_table: &Arc<[String]>,
    config: &ResolvedConfig,
    mbias_only_silence: bool,
    mbias_only: bool,
    mbias: &mut [MbiasTable; 2],
    report: &mut SplittingReport,
) -> Result<Vec<RoutedCall>, BismarkExtractorError> {
    // Cross-chr defensive check (matches Phase C).
    let r2_refid = pair.r2().inner().reference_sequence_id().ok_or_else(|| {
        BismarkExtractorError::InternalError {
            message: "PE R2 missing reference_sequence_id".to_string(),
        }
    })?;
    if r2_refid as u32 != chr_id {
        let qname = pair
            .r1()
            .inner()
            .name()
            .map(|n| String::from_utf8_lossy(n.as_ref()).into_owned())
            .unwrap_or_else(|| "<unnamed>".to_string());
        return Err(BismarkExtractorError::MateChromosomeMismatch {
            qname,
            r1_refid: chr_id as usize,
            r2_refid,
        });
    }

    if (chr_id as usize) >= chr_table.len() {
        return Err(BismarkExtractorError::InternalError {
            message: format!(
                "PE chr_id {} out of range vs header (count {})",
                chr_id,
                chr_table.len()
            ),
        });
    }

    let pair_strand = pair.pair_strand();
    let r1_calls = extract_calls(
        pair.r1(),
        config.ignore_5p_r1,
        config.ignore_3p_r1,
        mbias_only_silence,
    )?;
    let r2_calls_raw = extract_calls(
        pair.r2(),
        config.ignore_5p_r2,
        config.ignore_3p_r2,
        mbias_only_silence,
    )?;
    let r2_calls = if config.no_overlap {
        drop_overlap(r2_calls_raw, pair, config.ignore_3p_r1)?
    } else {
        r2_calls_raw
    };

    let mode = config.output_mode;
    let r1_qname_arc: Arc<[u8]> = qname_arc_for(pair.r1());
    let r2_qname_arc: Arc<[u8]> = qname_arc_for(pair.r2());

    let mut routed_calls: Vec<RoutedCall> = if mbias_only {
        Vec::new()
    } else {
        Vec::with_capacity(r1_calls.len() + r2_calls.len())
    };

    // R1 — M-bias idx 0
    for call in r1_calls {
        if !config.mbias_off {
            let pos_1based = call.read_pos.saturating_add(1);
            mbias[0].accumulate(call.context, pos_1based, call.methylated);
        }
        increment_counters(report, call);

        if mbias_only {
            continue;
        }

        let (yacht_col6, yacht_col7) = compute_yacht_columns(mode, pair.r1(), pair_strand)?;
        routed_calls.push(RoutedCall {
            call,
            strand: pair_strand,
            yacht_col6,
            yacht_col7,
            qname: Arc::clone(&r1_qname_arc),
            chr_id,
        });
    }

    // R2 — M-bias idx 1
    for call in r2_calls {
        if !config.mbias_off {
            let pos_1based = call.read_pos.saturating_add(1);
            mbias[1].accumulate(call.context, pos_1based, call.methylated);
        }
        increment_counters(report, call);

        if mbias_only {
            continue;
        }

        let (yacht_col6, yacht_col7) = compute_yacht_columns(mode, pair.r2(), pair_strand)?;
        routed_calls.push(RoutedCall {
            call,
            strand: pair_strand,
            yacht_col6,
            yacht_col7,
            qname: Arc::clone(&r2_qname_arc),
            chr_id,
        });
    }

    // Records processed: +2 per PE pair (matches Phase C / Perl :2451).
    // Phase C.2 (#864): split counters per Perl semantics:
    //   - `records_processed += 1` per pair → matches Perl
    //     `$counting{sequences_count}` (line 2459), which drives the
    //     report line 2482 "Processed N lines in total" with N = pair
    //     count for PE.
    //   - `call_strings_processed += 2` per pair → matches Perl
    //     `$methylation_call_strings_processed += 2` (line 2451), which
    //     drives report line 2483 "Total number of methylation call
    //     strings processed: 2N".
    // Pre-C.2 incorrectly used `records_processed += 2` citing Perl line
    // 2451; the citation was for the wrong counter. Same fix applied at
    // `pipeline.rs:275-276` for the legacy single-threaded PE path.
    report.records_processed = report.records_processed.saturating_add(1);
    report.call_strings_processed = report.call_strings_processed.saturating_add(2);

    Ok(routed_calls)
}

/// Bump the per-context counters for one call. Used by both SE + PE worker paths.
fn increment_counters(report: &mut SplittingReport, call: MethCall) {
    report.calls_total = report.calls_total.saturating_add(1);
    match (call.context, call.methylated) {
        (CytosineContext::CpG, true) => {
            report.calls_cpg_meth = report.calls_cpg_meth.saturating_add(1);
        }
        (CytosineContext::CpG, false) => {
            report.calls_cpg_unmeth = report.calls_cpg_unmeth.saturating_add(1);
        }
        (CytosineContext::CHG, true) => {
            report.calls_chg_meth = report.calls_chg_meth.saturating_add(1);
        }
        (CytosineContext::CHG, false) => {
            report.calls_chg_unmeth = report.calls_chg_unmeth.saturating_add(1);
        }
        (CytosineContext::CHH, true) => {
            report.calls_chh_meth = report.calls_chh_meth.saturating_add(1);
        }
        (CytosineContext::CHH, false) => {
            report.calls_chh_unmeth = report.calls_chh_unmeth.saturating_add(1);
        }
    }
}

/// Build an `Arc<[u8]>` of the record's QNAME bytes. Used once per record
/// at worker time; subsequent calls from the same record clone the Arc
/// (atomic-inc) rather than the bytes.
pub(crate) fn qname_arc_for(record: &BismarkRecord) -> Arc<[u8]> {
    let bytes: &[u8] = record
        .inner()
        .name()
        .map(|n| n.as_ref())
        .unwrap_or(b"<unnamed>");
    Arc::from(bytes)
}

// ─── Collector ───────────────────────────────────────────────────────────────

/// Result returned by `collector_loop`. The `finaldeltas_received` field
/// lets the caller distinguish "clean shutdown with all N FinalDeltas" from
/// "channel disconnect before all N arrived" — which is usually a worker
/// panic. The caller (`run_pipeline`) inspects this together with the
/// per-thread join results to pick the most-specific error message
/// (Reviewer B C2 fix: previously the collector synthesised a generic
/// "missing FinalDelta" error which then overrode the actual worker
/// panic payload via merge_results — losing diagnostic info).
pub(crate) struct CollectorOutcome {
    /// Number of `WorkerOutput::FinalDelta` messages the collector observed
    /// before the channel disconnected. Equals `n_workers` on a clean
    /// shutdown; less means at least one worker exited without notifying.
    pub finaldeltas_received: usize,
    /// First (by lowest `(batch_seq, within_idx)`) Err the collector saw, if
    /// any. That tuple order is isomorphic to the old `input_idx` order, so the
    /// selected Err — hence stderr — is byte-identical to the per-record path.
    /// Excludes the synthetic "missing FinalDelta" error — that decision is the
    /// caller's because they can see worker panic info too.
    pub best_err: Option<BismarkExtractorError>,
}

/// Collector loop: receive WorkerOutput, reorder by input_idx, write to
/// OutputFileMap in strict input order. Merges per-worker M-bias +
/// SplittingReport deltas at end-of-stream.
///
/// **Returns `CollectorOutcome` (not `Result`) so the caller can pick the
/// most-specific error message** when both a worker panic and a missing-
/// FinalDelta condition are present. See `CollectorOutcome` doc.
fn collector_loop(
    state: &mut ExtractState,
    rx_output: Receiver<WorkerOutput>,
    chr_table: &Arc<[String]>,
    n_workers: usize,
) -> CollectorOutcome {
    // #884 R1: reorder by `batch_seq`. Each entry is a whole batch's worth of
    // per-item results; within a batch we emit in `Vec` order. `(batch_seq,
    // within_idx)` is the global ordering token (isomorphic to old `input_idx`).
    let mut reorder_buf: BTreeMap<u64, Vec<WorkerOutputItem>> = BTreeMap::new();
    let mut next_emit_seq: u64 = 0;
    let mut finaldeltas_received: usize = 0;
    let mut best_err: Option<((u64, usize), BismarkExtractorError)> = None;

    loop {
        match rx_output.recv() {
            Ok(WorkerOutput::Batch { batch_seq, results }) => {
                reorder_buf.insert(batch_seq, results);
                // Drain in-order batches from the front of the buffer.
                while let Some(items) = reorder_buf.remove(&next_emit_seq) {
                    for (within_idx, item) in items.into_iter().enumerate() {
                        match item {
                            WorkerOutputItem::Ok { routed_calls } => {
                                for routed in &routed_calls {
                                    if let Err(e) = write_routed_call(state, routed, chr_table) {
                                        // Stash the write error keyed by its
                                        // global position; continue draining.
                                        update_best_err(
                                            &mut best_err,
                                            (next_emit_seq, within_idx),
                                            e,
                                        );
                                    }
                                }
                            }
                            WorkerOutputItem::Err { error } => {
                                // Err item keeps its slot; feed to selection,
                                // do not write. (Producer- or worker-side.)
                                update_best_err(&mut best_err, (next_emit_seq, within_idx), error);
                            }
                        }
                    }
                    next_emit_seq += 1;
                }
            }
            Ok(WorkerOutput::FinalDelta { mbias, report }) => {
                finaldeltas_received += 1;
                // Sum-merge M-bias (commutative + associative — order-independent).
                let [m0, m1] = mbias;
                state.mbias[0].add(&m0);
                state.mbias[1].add(&m1);
                // Sum-merge the SplittingReport (8 saturating sums).
                state.report.add(&report);

                if finaldeltas_received >= n_workers {
                    // All workers have emitted FinalDelta. We're done.
                    break;
                }
            }
            Err(RecvError) => {
                // Channel disconnected before we got all FinalDeltas — some
                // worker exited without notifying (likely panicked). DON'T
                // synthesise an Err here; the caller has better information
                // (worker JoinHandle results) and can pick the right
                // precedence. Just exit the loop.
                break;
            }
        }
    }

    CollectorOutcome {
        finaldeltas_received,
        best_err: best_err.map(|(_, e)| e),
    }
}

/// Update `best_err` to keep the lowest-`(batch_seq, within_idx)` Err seen so
/// far. The tuple's derived lexicographic `Ord` is isomorphic to the old
/// `input_idx` order, so "lowest wins" reproduces today's lowest-`input_idx`
/// choice. Deterministic across worker arrival order → byte-identical stderr.
fn update_best_err(
    best: &mut Option<((u64, usize), BismarkExtractorError)>,
    candidate_key: (u64, usize),
    candidate_err: BismarkExtractorError,
) {
    match best {
        None => *best = Some((candidate_key, candidate_err)),
        Some((existing_key, _)) if candidate_key < *existing_key => {
            *best = Some((candidate_key, candidate_err));
        }
        Some(_) => {} // keep existing (smaller-or-equal key)
    }
}

/// Write one `RoutedCall` to the output map. Resolves chr_id to chromosome
/// name via the shared `chr_table`.
fn write_routed_call(
    state: &mut ExtractState,
    routed: &RoutedCall,
    chr_table: &Arc<[String]>,
) -> Result<(), BismarkExtractorError> {
    let chr = chr_table
        .get(routed.chr_id as usize)
        .map(|s| s.as_str())
        .ok_or_else(|| BismarkExtractorError::InternalError {
            message: format!(
                "chr_id {} out of range vs chr_table (count {}); \
                 producer-collector chr_table mismatch",
                routed.chr_id,
                chr_table.len()
            ),
        })?;

    // Phase 3a (F1/F6 + D5): this is the COLLECTOR-thread write funnel in
    // `--parallel` mode (workers never tee). Disjoint field borrow so the
    // bedGraph aggregator rides alongside the `&mut OutputFileMap`. `chr`
    // borrows `chr_table` (not `state`), so it does not conflict with the
    // split borrow of `state`.
    let ExtractState {
        fhs,
        bedgraph_aggregator,
        bedgraph_cx,
        ..
    } = state;
    fhs.write_call(
        &routed.qname,
        chr,
        routed.call,
        routed.strand,
        routed.yacht_col6,
        routed.yacht_col7,
        bedgraph_aggregator.as_mut(),
        *bedgraph_cx,
    )?;
    Ok(())
}

// ─── Unit tests (helpers + small invariants) ─────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_best_err_picks_lowest_global_key() {
        let mut best: Option<((u64, usize), BismarkExtractorError)> = None;

        // First error: (batch 1, idx 0)
        update_best_err(
            &mut best,
            (1, 0),
            BismarkExtractorError::InternalError {
                message: "b1i0".to_string(),
            },
        );
        assert_eq!(best.as_ref().unwrap().0, (1, 0));

        // Second error: (batch 0, idx 2) — earlier batch wins even though its
        // within-idx is larger (lexicographic: batch_seq dominates).
        update_best_err(
            &mut best,
            (0, 2),
            BismarkExtractorError::InternalError {
                message: "b0i2".to_string(),
            },
        );
        assert_eq!(best.as_ref().unwrap().0, (0, 2));

        // Third error: (batch 0, idx 5) — same batch, higher idx → NOT replace.
        update_best_err(
            &mut best,
            (0, 5),
            BismarkExtractorError::InternalError {
                message: "b0i5".to_string(),
            },
        );
        assert_eq!(best.as_ref().unwrap().0, (0, 2)); // unchanged

        // Fourth error: (batch 0, idx 1) — same batch, lower idx → replace.
        update_best_err(
            &mut best,
            (0, 1),
            BismarkExtractorError::InternalError {
                message: "b0i1".to_string(),
            },
        );
        assert_eq!(best.as_ref().unwrap().0, (0, 1));
    }

    #[test]
    fn update_best_err_equal_key_keeps_existing() {
        let mut best: Option<((u64, usize), BismarkExtractorError)> = None;
        update_best_err(
            &mut best,
            (5, 3),
            BismarkExtractorError::InternalError {
                message: "first".to_string(),
            },
        );
        update_best_err(
            &mut best,
            (5, 3),
            BismarkExtractorError::InternalError {
                message: "second".to_string(),
            },
        );
        // First wins on equality.
        if let Some((_, BismarkExtractorError::InternalError { message })) = &best {
            assert_eq!(message, "first");
        } else {
            panic!("expected InternalError");
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // #878 Tests 2–3: parallel worker M-bias accumulator slots are rebased.
    // Guards parallel.rs:711 (SE → mbias[0]), :815 (PE R1 → mbias[0]),
    // :838 (PE R2 → mbias[1], using ignore_r2). pos_1based = read_pos + 1, so
    // a rebased first call lands at slot 1; reverting call.rs:204 shifts it to
    // slot ignore+1. These call the private process_se/process_pe directly.
    // ─────────────────────────────────────────────────────────────────────
    use bstr::BString;
    use noodles_core::Position;
    use noodles_sam::alignment::record::Flags;
    use noodles_sam::alignment::record::cigar::Op;
    use noodles_sam::alignment::record::cigar::op::Kind;
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    use noodles_sam::alignment::record_buf::{Cigar, RecordBuf, Sequence};

    /// Minimal single-`M` `BismarkRecord` (mirrors `tests/parallel_phase_f.rs::synth_record`).
    fn synth_rec(
        qname: &[u8],
        xr: &[u8],
        xg: &[u8],
        xm: &[u8],
        start: usize,
        flags: u16,
    ) -> BismarkRecord {
        let mut record = RecordBuf::default();
        *record.flags_mut() = Flags::from(flags);
        *record.sequence_mut() = Sequence::from(vec![b'A'; xm.len()]);
        *record.alignment_start_mut() = Some(Position::try_from(start).unwrap());
        *record.reference_sequence_id_mut() = Some(0);
        *record.cigar_mut() = Cigar::from(vec![Op::new(Kind::Match, xm.len())]);
        *record.name_mut() = Some(BString::from(qname.to_vec()));
        record
            .data_mut()
            .insert(Tag::from(*b"XR"), Value::String(BString::from(xr.to_vec())));
        record
            .data_mut()
            .insert(Tag::from(*b"XG"), Value::String(BString::from(xg.to_vec())));
        record
            .data_mut()
            .insert(Tag::from(*b"XM"), Value::String(BString::from(xm.to_vec())));
        BismarkRecord::from_noodles_record(record).expect("synth BismarkRecord")
    }

    /// Build a `ResolvedConfig` from CLI args (idiom from `parallel.rs:1173/1263`).
    /// Needs a real `.bam` path arg for the parser; `process_*` never reads it.
    fn config_with(extra: &[&str]) -> ResolvedConfig {
        use crate::cli::Cli;
        use clap::Parser;
        let tmp = tempfile::Builder::new().suffix(".bam").tempfile().unwrap();
        std::fs::write(tmp.path(), b"x").unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        let mut args = vec!["bismark_methylation_extractor_rs"];
        args.extend(extra.iter().copied());
        args.push(&path);
        let cfg = Cli::try_parse_from(&args).unwrap().validate().unwrap();
        drop(tmp);
        cfg
    }

    /// Sum of all (meth+unmeth) counts across the 3 contexts of one table.
    fn mbias_total(t: &MbiasTable) -> u64 {
        t.cpg
            .iter()
            .chain(&t.chg)
            .chain(&t.chh)
            .map(|p| p.meth + p.unmeth)
            .sum()
    }

    /// #878 Test 2 — SE worker M-bias slots are rebased (`parallel.rs:711`).
    #[test]
    fn parallel_se_worker_m_bias_rebased() {
        // OT 6bp: '.'@0,1,2 ; Z(CpG,meth)@3 ; x(CHG,unmeth)@4 ; h(CHH,unmeth)@5.
        let rec = synth_rec(b"r", b"CT", b"CT", b"...Zxh", 100, 0);
        let config = config_with(&["--single-end", "--ignore", "3", "--mbias_only"]);
        let chr_table: Arc<[String]> = Arc::from(vec!["chr1".to_string()].into_boxed_slice());
        let mut mbias = [MbiasTable::default(), MbiasTable::default()];
        let mut report = SplittingReport::default();
        process_se(
            &rec,
            0,
            &chr_table,
            &config,
            true,
            true,
            &mut mbias,
            &mut report,
        )
        .expect("process_se");

        // Rebased: read_pos 0,1,2 → 1-based slots 1,2,3 (reverted would be 4,5,6).
        assert_eq!(mbias[0].cpg[1].meth, 1, "CpG meth at rebased slot 1");
        assert_eq!(mbias[0].chg[2].unmeth, 1, "CHG unmeth at rebased slot 2");
        assert_eq!(mbias[0].chh[3].unmeth, 1, "CHH unmeth at rebased slot 3");
        // The absolute (reverted) slots must be empty.
        assert_eq!(
            mbias[0].cpg.get(4).map_or(0, |p| p.meth + p.unmeth),
            0,
            "no CpG at absolute slot 4"
        );
        assert_eq!(
            mbias[0].chg.get(5).map_or(0, |p| p.meth + p.unmeth),
            0,
            "no CHG at absolute slot 5"
        );
        assert_eq!(
            mbias[0].chh.get(6).map_or(0, |p| p.meth + p.unmeth),
            0,
            "no CHH at absolute slot 6"
        );
        // SE → nothing in the R2 table.
        assert_eq!(mbias_total(&mbias[1]), 0, "SE leaves mbias[1] empty");
    }

    /// #878 Test 3 — PE worker uses `--ignore_r2` for R2 + the R2 table
    /// (`parallel.rs:838`); `--ignore` for R1 (`:815`). R2 of an OT pair is
    /// CTOT (`-`-strand, reversed: read_pos_5p = seq_len-1-BAM), so its call
    /// is placed at BAM index `seq_len-1-ignore_r2`. R1/R2 are non-overlapping
    /// so `drop_overlap` keeps R2.
    #[test]
    fn parallel_pe_worker_m_bias_uses_r2_ignore_for_r2() {
        // R1: OT (CT,CT) '+'-strand, 6bp, CpG 'Z'@BAM3 → read_pos_5p=3;
        //     --ignore 3 → rebased 0 → mbias[0].cpg slot 1.
        let r1 = synth_rec(b"pair", b"CT", b"CT", b"...Z..", 100, 0x41);
        // R2: CTOT (GA,CT) '-'-strand reversed, 9bp, CpG 'Z'@BAM1 →
        //     read_pos_5p = 9-1-1 = 7; --ignore_r2 7 → rebased 0 → mbias[1].cpg slot 1.
        //     start=200 (non-overlapping with R1@100) so drop_overlap keeps it.
        let r2 = synth_rec(b"pair", b"GA", b"CT", b".Z.......", 200, 0x81);
        let pair = BismarkPair::from_mates(r1, r2).expect("valid OT pair");

        let config = config_with(&["-p", "--ignore", "3", "--ignore_r2", "7", "--mbias_only"]);
        let chr_table: Arc<[String]> = Arc::from(vec!["chr1".to_string()].into_boxed_slice());
        let mut mbias = [MbiasTable::default(), MbiasTable::default()];
        let mut report = SplittingReport::default();
        process_pe(
            &pair,
            0,
            &chr_table,
            &config,
            true,
            true,
            &mut mbias,
            &mut report,
        )
        .expect("process_pe");

        // R1 → mbias[0] slot 1 (rebased by --ignore 3); R2 → mbias[1] slot 1 (by --ignore_r2 7).
        assert_eq!(
            mbias[0].cpg[1].meth, 1,
            "R1 CpG meth at mbias[0] rebased slot 1"
        );
        assert_eq!(
            mbias[1].cpg[1].meth, 1,
            "R2 CpG meth at mbias[1] rebased slot 1"
        );
        // Absolute (reverted) slots empty; no cross-table leakage.
        assert_eq!(
            mbias[0].cpg.get(4).map_or(0, |p| p.meth + p.unmeth),
            0,
            "R1 not at absolute slot 4"
        );
        assert_eq!(
            mbias[1].cpg.get(8).map_or(0, |p| p.meth + p.unmeth),
            0,
            "R2 not at absolute slot 8"
        );
        assert_eq!(
            mbias_total(&mbias[0]),
            1,
            "mbias[0] holds exactly R1's call"
        );
        assert_eq!(
            mbias_total(&mbias[1]),
            1,
            "mbias[1] holds exactly R2's call"
        );
    }

    /// Plan rev 1 §7.1 + Reviewer A & B both flagged: prove that a
    /// producer panic does NOT deadlock workers. With channel-disconnect-
    /// as-EOS, when the producer's thread closure panics, its `tx_input`
    /// Sender is dropped during unwind → workers see
    /// `Err(RecvError::Disconnected)` → emit `FinalDelta` → exit cleanly.
    ///
    /// Validated by spawning a fake "producer" that panics immediately,
    /// running real workers, and asserting all workers emit FinalDelta
    /// within a 5-second deadline. If the panic-disconnect-cleanup chain
    /// is broken, the workers block forever on `rx_input.recv()` and the
    /// test times out.
    #[test]
    fn producer_panic_does_not_deadlock_workers() {
        use crate::cli::Cli;
        use clap::Parser;

        // Build a minimal ResolvedConfig — the workers won't actually
        // process records (the panicking producer sends nothing), so
        // most config fields don't matter. We just need a syntactically
        // valid one with parallel=4.
        let tmpfile = tempfile::Builder::new().suffix(".bam").tempfile().unwrap();
        std::fs::write(tmpfile.path(), b"x").unwrap();
        let mut full = vec!["bismark_methylation_extractor_rs"];
        let input_path = tmpfile.path().to_str().unwrap().to_string();
        full.extend(
            ["--single-end", "--parallel", "4", &input_path]
                .iter()
                .copied(),
        );
        let cli = Cli::try_parse_from(&full).unwrap();
        let config = cli.validate().unwrap();

        let n_workers = 4;
        let chr_table: Arc<[String]> = Arc::from(vec!["chr1".to_string()].into_boxed_slice());

        let (tx_input, rx_input) = crossbeam_channel::bounded::<InputBatch>(n_workers * 4);
        let (tx_output, rx_output) = crossbeam_channel::bounded::<WorkerOutput>(n_workers * 4);

        // Spawn N workers (real worker_loop).
        let mut worker_handles = Vec::new();
        for _ in 0..n_workers {
            let rx_input = rx_input.clone();
            let tx_output = tx_output.clone();
            let cfg = config.clone();
            let ct = Arc::clone(&chr_table);
            worker_handles.push(std::thread::spawn(move || {
                worker_loop(rx_input, tx_output, cfg, ct);
            }));
        }
        // Drop main thread's clones so channels disconnect when other
        // ends drop too.
        drop(rx_input);
        drop(tx_output);

        // Spawn the "producer" — panics before sending anything.
        let producer_handle = std::thread::spawn(move || {
            let _own_tx = tx_input;
            panic!("intentional panic for producer-deadlock test");
        });

        // Wait for all N FinalDelta messages on rx_output, with a deadline.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut finaldeltas_received = 0usize;
        while finaldeltas_received < n_workers {
            if std::time::Instant::now() >= deadline {
                panic!(
                    "DEADLOCK: only {finaldeltas_received} of {n_workers} workers \
                     emitted FinalDelta within 5s after producer panicked"
                );
            }
            match rx_output.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(WorkerOutput::FinalDelta { .. }) => {
                    finaldeltas_received += 1;
                }
                Ok(_) => {
                    // Some other message — workers shouldn't produce
                    // anything else with no input, but tolerate.
                }
                Err(_timeout_or_disconnect) => {
                    // Continue polling until deadline.
                }
            }
        }

        // All workers exited cleanly. Producer panicked as expected.
        assert!(
            producer_handle.join().is_err(),
            "producer should have panicked"
        );
        // Workers should NOT have panicked.
        for h in worker_handles {
            h.join()
                .expect("worker should exit cleanly via channel disconnect");
        }
    }

    /// #884 R1 (plan §7, rev-1 Critical C1/C4): a producer-side `Err`
    /// encountered mid-batch must flush the CURRENT partial batch with its
    /// already-accumulated good lower-index items — the `Err` keeps its slot at
    /// the end, and the worker preserves both order and the 1:1
    /// `results.len() == items.len()` invariant. This guards byte-identity on
    /// error inputs: the good lower-index items that today get written before
    /// the producer short-circuits must still ship.
    ///
    /// We exercise the worker directly with a hand-built partial batch
    /// (`[Err, Err, Err]`-shaped via producer-side errors are awkward to forge
    /// from real records, so we use the always-available `WorkerInputItem::Err`
    /// forwarding path which models exactly the producer→worker contract for
    /// producer-side errors) interleaved with no real records — the key
    /// properties under test are *order preservation* and *slot retention*,
    /// both of which are record-independent.
    #[test]
    fn worker_preserves_order_and_keeps_err_slots_in_partial_batch() {
        use crate::cli::Cli;
        use clap::Parser;

        let tmpfile = tempfile::Builder::new().suffix(".bam").tempfile().unwrap();
        std::fs::write(tmpfile.path(), b"x").unwrap();
        let input_path = tmpfile.path().to_str().unwrap().to_string();
        let cli = Cli::try_parse_from(
            [
                "bismark_methylation_extractor_rs",
                "--single-end",
                &input_path,
            ]
            .iter()
            .copied(),
        )
        .unwrap();
        let config = cli.validate().unwrap();
        let chr_table: Arc<[String]> = Arc::from(vec!["chr1".to_string()].into_boxed_slice());

        let (tx_input, rx_input) = crossbeam_channel::bounded::<InputBatch>(4);
        let (tx_output, rx_output) = crossbeam_channel::bounded::<WorkerOutput>(4);

        // A partial batch carrying three producer-side errors at distinct
        // within-batch slots (mirrors a producer that buffered items then hit
        // an error and flushed the partial batch).
        let items = vec![
            WorkerInputItem::Err {
                error: BismarkExtractorError::InternalError {
                    message: "slot0".to_string(),
                },
            },
            WorkerInputItem::Err {
                error: BismarkExtractorError::InternalError {
                    message: "slot1".to_string(),
                },
            },
            WorkerInputItem::Err {
                error: BismarkExtractorError::InternalError {
                    message: "slot2".to_string(),
                },
            },
        ];
        let n_items = items.len();
        tx_input
            .send(InputBatch {
                batch_seq: 7,
                items,
            })
            .unwrap();
        drop(tx_input); // disconnect → worker emits its Batch then FinalDelta

        let cfg = config.clone();
        let ct = Arc::clone(&chr_table);
        let worker = std::thread::spawn(move || {
            worker_loop(rx_input, tx_output, cfg, ct);
        });

        // First message must be the Batch with results preserving order +
        // every Err keeping its slot (results.len() == items.len()).
        match rx_output.recv().unwrap() {
            WorkerOutput::Batch { batch_seq, results } => {
                assert_eq!(batch_seq, 7);
                assert_eq!(
                    results.len(),
                    n_items,
                    "results.len() must equal items.len() (1:1 slot mapping)"
                );
                for (idx, item) in results.iter().enumerate() {
                    match item {
                        WorkerOutputItem::Err {
                            error: BismarkExtractorError::InternalError { message },
                        } => {
                            assert_eq!(
                                message,
                                &format!("slot{idx}"),
                                "Err at within-idx {idx} must keep its original slot/order"
                            );
                        }
                        _ => panic!("expected forwarded Err at within-idx {idx}"),
                    }
                }
            }
            _ => panic!("expected WorkerOutput::Batch first"),
        }

        // Then the FinalDelta at EOS.
        match rx_output.recv().unwrap() {
            WorkerOutput::FinalDelta { .. } => {}
            _ => panic!("expected WorkerOutput::FinalDelta at EOS"),
        }

        worker.join().expect("worker should exit cleanly");
    }
}
