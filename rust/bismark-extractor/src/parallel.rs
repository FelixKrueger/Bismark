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
//! - **Producer** (single thread): drives `open_reader().records()`, assigns
//!   monotonic `input_idx`, sends `WorkerInput::Se | Pe | Err` into a
//!   bounded MPMC channel (N×32). For PE the producer also pairs adjacent
//!   records via `BismarkPair::from_mates`.
//! - **Workers** (N rayon threads): receive records/pairs, run
//!   `extract_calls` + `drop_overlap` + per-worker M-bias/counter
//!   accumulation, emit `WorkerOutput::Ok | Err` into a second bounded
//!   channel (N×8). Workers detect end-of-stream via channel-disconnect
//!   (`Err(RecvError)`) — no sentinel messages.
//! - **Collector** (main thread): owns `ExtractState`. Reorders worker
//!   output by `input_idx` via a `BTreeMap` to emit in strict input
//!   order — this guarantees byte-identity to the legacy single-threaded
//!   path. Sums per-worker M-bias / SplittingReport deltas at end-of-stream.
//!   Deterministic Err selection: lowest `input_idx` wins.
//!
//! # Byte-identity invariant
//!
//! `--multicore N` output MUST equal `--multicore 1` output for any N ≥ 1
//! on the same input. This is the load-bearing test surface for Phase H.
//! The mechanisms that hold this invariant:
//!
//! 1. **Input ordering**: producer assigns `input_idx` monotonically; collector
//!    emits in strict `input_idx` order via `BTreeMap`.
//! 2. **M-bias merge**: `MbiasTable::add` is commutative + associative.
//! 3. **Counter merge**: `SplittingReport::add` is commutative + associative.
//! 4. **Err selection**: collector picks the lowest-`input_idx` Err, so
//!    stderr is byte-identical even on multi-error inputs.
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

use bismark_io::{BismarkPair, BismarkRecord, BismarkStrand, open_reader};
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

/// Producer → worker channel message. EOS is signaled by the producer
/// dropping its sender (channel-disconnect-as-EOS, plan rev 1) — no
/// sentinel variant needed.
pub(crate) enum WorkerInput {
    /// Single-end record at `input_idx`. Worker resolves the chromosome
    /// name via the shared `chr_table` keyed by `chr_id`.
    Se {
        input_idx: u64,
        record: BismarkRecord,
        chr_id: u32,
    },
    /// Paired-end pair at `input_idx` (one idx per pair). Pair is already
    /// validated by `BismarkPair::from_mates` (qname-eq + R1/R2 identity).
    /// `Box` keeps the `WorkerInput` enum size proportional to the smallest
    /// variant (clippy::large_enum_variant); BismarkPair is ~2× BismarkRecord.
    Pe {
        input_idx: u64,
        pair: Box<BismarkPair>,
        chr_id: u32,
    },
    /// Error encountered by the producer (read error, pairing error, etc.).
    /// `input_idx` is the index of the message where the error occurred —
    /// used by the collector for deterministic Err selection.
    Err {
        input_idx: u64,
        error: BismarkExtractorError,
    },
}

/// Worker → collector channel message.
pub(crate) enum WorkerOutput {
    /// Result of processing one `WorkerInput::Se` or `WorkerInput::Pe`.
    /// `routed_calls` is empty under `--mbias_only` (worker still
    /// accumulates M-bias + counters locally but doesn't ship calls).
    Ok {
        input_idx: u64,
        routed_calls: Vec<RoutedCall>,
    },
    /// Sent exactly once by each worker at exit (after `recv()` returns
    /// `Err(Disconnected)`). Carries this worker's accumulated counters.
    FinalDelta {
        mbias: [MbiasTable; 2],
        report: SplittingReport,
    },
    /// Error during extraction (`InvalidXmByte`, `drop_overlap` failure,
    /// etc.) OR a forwarded `WorkerInput::Err`. `input_idx` lets the
    /// collector pick the lowest-idx Err for stable stderr.
    Err {
        input_idx: u64,
        error: BismarkExtractorError,
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
    let n_workers = config.parallel.max(1);

    // Open the reader on the main thread to get the header (for chr_table)
    // before we hand the reader off to the producer thread.
    let reader = open_reader(input, /*cram_ref=*/ None)?;
    let chr_table: Arc<[String]> = Arc::from(build_chr_name_table(reader.header())?);

    let input_basename = derive_basename(input);
    let mut state = ExtractState::new(config, input, &input_basename, is_paired)?;

    // Bounded channels per SPEC §9.2.
    let (tx_input, rx_input) = bounded::<WorkerInput>(n_workers * 32);
    let (tx_output, rx_output) = bounded::<WorkerOutput>(n_workers * 8);

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
            producer_loop(reader, is_paired, producer_tx_input);
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

/// Producer loop: drive the reader's `records()` iterator, assign monotonic
/// `input_idx`, send to workers. EOS is signaled by dropping `tx_input`
/// (happens automatically when this function returns).
fn producer_loop(
    mut reader: bismark_io::AnyReader<std::io::BufReader<std::fs::File>, std::fs::File>,
    is_paired: bool,
    tx_input: Sender<WorkerInput>,
) {
    let mut next_idx: u64 = 0;
    let mut records_iter = reader.records();

    if !is_paired {
        // SE: one record per channel message.
        loop {
            let input_idx = next_idx;
            let record_result = records_iter.next();
            match record_result {
                Some(Ok(record)) => {
                    next_idx += 1;
                    // Resolve reference_sequence_id → u32 with defensive
                    // try_from (Reviewer B.H1: `as u32` would silently
                    // truncate above 2^32 contigs; matches the precedent
                    // in `compute_yacht_columns`).
                    let chr_id_result: Result<Option<u32>, _> = record
                        .inner()
                        .reference_sequence_id()
                        .map(u32::try_from)
                        .transpose();
                    let msg = match chr_id_result {
                        Ok(Some(chr_id)) => WorkerInput::Se {
                            input_idx,
                            record,
                            chr_id,
                        },
                        Ok(None) => WorkerInput::Err {
                            input_idx,
                            error: BismarkExtractorError::InternalError {
                                message: "mapped record has no reference_sequence_id; \
                                          bismark-io::records should have filtered this \
                                          as unmapped (FLAG & 0x4)"
                                    .to_string(),
                            },
                        },
                        Err(_) => WorkerInput::Err {
                            input_idx,
                            error: BismarkExtractorError::InternalError {
                                message: "reference_sequence_id overflows u32 \
                                          (>= 2^32 contigs in header)"
                                    .to_string(),
                            },
                        },
                    };
                    if tx_input.send(msg).is_err() {
                        // All workers gone — nothing more to do.
                        return;
                    }
                }
                Some(Err(e)) => {
                    let _ = tx_input.send(WorkerInput::Err {
                        input_idx,
                        error: e.into(),
                    });
                    return;
                }
                None => break, // clean EOF
            }
        }
    } else {
        // PE: take adjacent records, pair them on the producer thread.
        loop {
            let input_idx = next_idx;
            // R1
            let r1 = match records_iter.next() {
                Some(Ok(r)) => r,
                Some(Err(e)) => {
                    let _ = tx_input.send(WorkerInput::Err {
                        input_idx,
                        error: e.into(),
                    });
                    return;
                }
                None => break, // clean EOF
            };
            // R2
            let r2 = match records_iter.next() {
                Some(Ok(r)) => r,
                Some(Err(e)) => {
                    let _ = tx_input.send(WorkerInput::Err {
                        input_idx,
                        error: e.into(),
                    });
                    return;
                }
                None => {
                    let qname = r1
                        .inner()
                        .name()
                        .map(|n| String::from_utf8_lossy(n.as_ref()).into_owned());
                    let _ = tx_input.send(WorkerInput::Err {
                        input_idx,
                        error: BismarkExtractorError::UnpairedFinalRecord { qname },
                    });
                    return;
                }
            };
            next_idx += 1;
            // Pair-formation
            let pair = match BismarkPair::from_mates(r1, r2) {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx_input.send(WorkerInput::Err {
                        input_idx,
                        error: e.into(),
                    });
                    return;
                }
            };
            // Resolve R1 refid → u32 with defensive try_from (Reviewer B.H1).
            let chr_id: u32 = match pair.r1().inner().reference_sequence_id() {
                Some(r) => match u32::try_from(r) {
                    Ok(v) => v,
                    Err(_) => {
                        let _ = tx_input.send(WorkerInput::Err {
                            input_idx,
                            error: BismarkExtractorError::InternalError {
                                message: format!("PE R1 reference_sequence_id {r} overflows u32"),
                            },
                        });
                        return;
                    }
                },
                None => {
                    let _ = tx_input.send(WorkerInput::Err {
                        input_idx,
                        error: BismarkExtractorError::InternalError {
                            message: "PE R1 missing reference_sequence_id".to_string(),
                        },
                    });
                    return;
                }
            };
            if tx_input
                .send(WorkerInput::Pe {
                    input_idx,
                    pair: Box::new(pair),
                    chr_id,
                })
                .is_err()
            {
                return;
            }
        }
    }
    // tx_input drops as this function returns → channel disconnects → workers exit.
    drop(tx_input);
}

// ─── Worker ──────────────────────────────────────────────────────────────────

/// Worker loop: process WorkerInput messages, accumulate per-worker
/// M-bias + counters, emit WorkerOutput. Exits on channel-disconnect EOS
/// by emitting a FinalDelta.
fn worker_loop(
    rx_input: Receiver<WorkerInput>,
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
            Ok(WorkerInput::Se {
                input_idx,
                record,
                chr_id,
            }) => {
                let result = process_se(
                    &record,
                    chr_id,
                    &chr_table,
                    &config,
                    mbias_only,
                    mbias_only,
                    &mut mbias,
                    &mut report,
                );
                let msg = match result {
                    Ok(routed_calls) => WorkerOutput::Ok {
                        input_idx,
                        routed_calls,
                    },
                    Err(error) => WorkerOutput::Err { input_idx, error },
                };
                if tx_output.send(msg).is_err() {
                    return;
                }
            }
            Ok(WorkerInput::Pe {
                input_idx,
                pair,
                chr_id,
            }) => {
                let result = process_pe(
                    &pair,
                    chr_id,
                    &chr_table,
                    &config,
                    mbias_only,
                    mbias_only,
                    &mut mbias,
                    &mut report,
                );
                let msg = match result {
                    Ok(routed_calls) => WorkerOutput::Ok {
                        input_idx,
                        routed_calls,
                    },
                    Err(error) => WorkerOutput::Err { input_idx, error },
                };
                if tx_output.send(msg).is_err() {
                    return;
                }
            }
            Ok(WorkerInput::Err { input_idx, error }) => {
                // Forward error; continue draining channel (don't short-circuit)
                // so byte-identity for collector's Err selection is preserved.
                if tx_output
                    .send(WorkerOutput::Err { input_idx, error })
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

    // Records processed: +1 per SE record (matches Phase B).
    report.records_processed = report.records_processed.saturating_add(1);

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
        drop_overlap(r2_calls_raw, pair)?
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
    report.records_processed = report.records_processed.saturating_add(2);

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
    /// First (by lowest `input_idx`) Err the collector saw, if any. Excludes
    /// the synthetic "missing FinalDelta" error — that decision is the
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
    let mut reorder_buf: BTreeMap<u64, Vec<RoutedCall>> = BTreeMap::new();
    let mut next_emit_idx: u64 = 0;
    let mut finaldeltas_received: usize = 0;
    let mut best_err: Option<(u64, BismarkExtractorError)> = None;

    loop {
        match rx_output.recv() {
            Ok(WorkerOutput::Ok {
                input_idx,
                routed_calls,
            }) => {
                reorder_buf.insert(input_idx, routed_calls);
                // Drain in-order entries from the front of the buffer.
                while let Some(calls) = reorder_buf.remove(&next_emit_idx) {
                    for routed in &calls {
                        if let Err(e) = write_routed_call(state, routed, chr_table) {
                            // Stash the write error with the current emit idx.
                            update_best_err(&mut best_err, next_emit_idx, e);
                            // Continue draining — don't break the loop.
                        }
                    }
                    next_emit_idx += 1;
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
            Ok(WorkerOutput::Err { input_idx, error }) => {
                update_best_err(&mut best_err, input_idx, error);
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

/// Update `best_err` to keep the lowest-`input_idx` Err seen so far.
/// Deterministic across worker arrival order → byte-identical stderr.
fn update_best_err(
    best: &mut Option<(u64, BismarkExtractorError)>,
    candidate_idx: u64,
    candidate_err: BismarkExtractorError,
) {
    match best {
        None => *best = Some((candidate_idx, candidate_err)),
        Some((existing_idx, _)) if candidate_idx < *existing_idx => {
            *best = Some((candidate_idx, candidate_err));
        }
        Some(_) => {} // keep existing (smaller-or-equal idx)
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

    state.fhs.write_call(
        &routed.qname,
        chr,
        routed.call,
        routed.strand,
        routed.yacht_col6,
        routed.yacht_col7,
    )?;
    Ok(())
}

// ─── Unit tests (helpers + small invariants) ─────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_best_err_picks_lowest_input_idx() {
        let mut best: Option<(u64, BismarkExtractorError)> = None;

        // First error: input_idx=5
        update_best_err(
            &mut best,
            5,
            BismarkExtractorError::InternalError {
                message: "idx 5".to_string(),
            },
        );
        assert_eq!(best.as_ref().unwrap().0, 5);

        // Second error: input_idx=3 — should replace (lower wins).
        update_best_err(
            &mut best,
            3,
            BismarkExtractorError::InternalError {
                message: "idx 3".to_string(),
            },
        );
        assert_eq!(best.as_ref().unwrap().0, 3);

        // Third error: input_idx=7 — should NOT replace (higher).
        update_best_err(
            &mut best,
            7,
            BismarkExtractorError::InternalError {
                message: "idx 7".to_string(),
            },
        );
        assert_eq!(best.as_ref().unwrap().0, 3); // unchanged
    }

    #[test]
    fn update_best_err_equal_idx_keeps_existing() {
        let mut best: Option<(u64, BismarkExtractorError)> = None;
        update_best_err(
            &mut best,
            5,
            BismarkExtractorError::InternalError {
                message: "first".to_string(),
            },
        );
        update_best_err(
            &mut best,
            5,
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
        let mut full = vec!["bismark-methylation-extractor-rs"];
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

        let (tx_input, rx_input) = crossbeam_channel::bounded::<WorkerInput>(n_workers * 32);
        let (tx_output, rx_output) = crossbeam_channel::bounded::<WorkerOutput>(n_workers * 8);

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
}
