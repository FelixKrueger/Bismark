//! SE + PE extraction pipelines (Phase B + Phase C).
//!
//! Per SPEC §7.2 (SE) + §7.3 (PE): record/pair at a time, classify XM bytes,
//! route to split files + accumulate M-bias counters + bump splitting-report
//! counters.
//!
//! Multicore (Phase F), gzip (Phase E), bedGraph/cytosine_report subprocess
//! (Phase G), and non-default output modes (Phase E) are rejected at
//! `main::run`'s config-dispatch boundary, not here.
//!
//! ## Phase B → C duplication note
//!
//! Phase C's plan §6 step 6 anticipated a `run_extraction<F>` helper to
//! share scaffolding between `extract_se` and `extract_pe`. Per the
//! contingency in the plan (Phase B PR #849 still in review at Phase C
//! implementation time), `extract_pe` duplicates `extract_se`'s scaffolding
//! rather than refactoring it concurrently with Phase B's review. The
//! `run_extraction` helper extraction lands as a follow-up PR once Phase
//! B merges.

use std::path::Path;

use crate::io::{
    AnyReader, BismarkPair, BismarkRecord, ReadIdentity, open_reader,
    open_reader_without_sort_check,
};

use crate::extractor::call::extract_calls;
use crate::extractor::cli::ResolvedConfig;
use crate::extractor::error::BismarkExtractorError;
use crate::extractor::header::build_chr_name_table;
use crate::extractor::overlap::{drop_overlap, drop_overlap_generic};
use crate::extractor::pair_class::{PairClass, classify_pair};
use crate::extractor::route::route_call;
use crate::extractor::state::ExtractState;

/// Strip a single Bismark-recognised suffix from the input path's basename.
///
/// Matches Perl `s/sam$/txt/; s/bam$/txt/; s/cram$/txt/` semantics:
/// **case-sensitive**, **single-extension only**. `foo.bam.gz` is NOT
/// transformed (Perl wouldn't either — `s/bam$/txt/` doesn't match `.gz`).
/// `foo.BAM` (uppercase) is left as `foo.BAM` (Perl regex is case-sensitive).
///
/// **Distinct from [`crate::extractor::mbias_writer::derive_mbias_basename`]** (Phase D):
/// that helper strips `bam`/`sam`/`cram`/`txt`/`gz` WITHOUT the leading dot,
/// preserving the trailing `.` for M-bias.txt filenames. This one strips
/// `.bam`/`.sam`/`.cram` WITH the dot for split-file basenames. The
/// divergence mirrors Perl's distinct regex chains for the two filename
/// styles.
///
/// # Panics
///
/// Panics if `path` has no filename component — caller guarantees a real
/// input file (validated at `Cli::validate`).
pub fn derive_basename(path: &Path) -> String {
    let filename = path
        .file_name()
        .expect("input path validated by Cli::validate must have a filename")
        .to_string_lossy()
        .into_owned();
    // Strip exactly one of the three known extensions.
    for ext in [".bam", ".sam", ".cram"] {
        if let Some(stem) = filename.strip_suffix(ext) {
            return stem.to_string();
        }
    }
    filename
}

/// SE extraction main loop.
///
/// Opens the input, builds the chr-name table + state (which eagerly
/// creates 12 split files + writes headers), then iterates records:
/// extract calls → route each call → tally records. On any error before
/// `finalize`, runs `state.cleanup_partial_outputs()` to remove all 12
/// files before propagating.
///
/// Uses `open_reader_without_sort_check`: single-end calls are
/// order-independent, so coordinate-sorted input is valid (faithful to
/// Perl, which only sort-checks paired-end). Mirrors the SE arm of
/// `parallel.rs::run_pipeline`.
pub fn extract_se(input: &Path, config: &ResolvedConfig) -> Result<(), BismarkExtractorError> {
    let mut reader = open_reader_without_sort_check(input, /*cram_ref=*/ None)?;
    // `--allow_discordant` in SE mode enables only the io-layer 0x900 skip
    // (a general aligner's SE output has supplementaries too). Default no-op.
    if config.allow_discordant {
        reader.set_filter(crate::io::RecordFilter {
            drop_secondary: true,
            drop_supplementary: true,
        });
    }
    // Rev 2: build chr_table from `&reader.header()` directly — no Header
    // clone (Reviewer B E2). The borrow is released before `reader.records()`
    // takes its own mutable borrow further down.
    let chr_table = build_chr_name_table(reader.header())?;

    let input_basename = derive_basename(input);
    let mut state = ExtractState::new(config, input, &input_basename, /*is_paired=*/ false)?;

    for record_result in reader.records() {
        let record = match record_result {
            Ok(r) => r,
            Err(e) => {
                state.cleanup_partial_outputs();
                return Err(e.into());
            }
        };

        // Defensive PAIRED-flag check: SE pipeline must not silently accept
        // PE input. Rev 1: use `u16::from(flags)` per the noodles convention
        // (bismark-io read.rs:585, dedup pipeline.rs:1186).
        let flags_bits: u16 = record.inner().flags().into();
        if flags_bits & 0x1 != 0 {
            state.cleanup_partial_outputs();
            return Err(BismarkExtractorError::PhaseNotYetImplemented {
                feature: "paired-end extraction (input has PAIRED flag set); \
                          PE arrives in Phase C"
                    .to_string(),
            });
        }

        // Resolve chr name. bismark-io filters unmapped records (FLAG & 0x4)
        // at the iterator layer, so mapped records normally always have a
        // reference_sequence_id. Rev 2 (Reviewer A E2 / Reviewer B Err2):
        // convert the previous `.expect()` to a typed `InternalError` for
        // consistency with the dedup precedent and a graceful failure mode
        // should the upstream invariant ever regress.
        let refid = match record.inner().reference_sequence_id() {
            Some(r) => r,
            None => {
                state.cleanup_partial_outputs();
                return Err(BismarkExtractorError::InternalError {
                    message: "mapped record has no reference_sequence_id; \
                              bismark-io::records should have filtered this \
                              as unmapped (FLAG & 0x4)"
                        .to_string(),
                });
            }
        };
        let chr = match chr_table.get(refid) {
            Some(name) => name.as_str(),
            None => {
                state.cleanup_partial_outputs();
                return Err(BismarkExtractorError::InternalError {
                    message: format!(
                        "record refid {} out of range vs header (count {})",
                        refid,
                        chr_table.len()
                    ),
                });
            }
        };

        let strand = record.record_strand();
        let read_identity = ReadIdentity::from_flags(flags_bits);

        let calls = match extract_calls(
            &record,
            config.ignore_5p_r1,
            config.ignore_3p_r1,
            config.is_mbias_only(),
        ) {
            Ok(c) => c,
            Err(e) => {
                state.cleanup_partial_outputs();
                return Err(e);
            }
        };

        for call in calls {
            // Rev 2: `route_call` now returns `BismarkExtractorError` directly
            // (was io::Error), which captures both write_call's IoWrite path
            // and the (unreachable-in-practice) InternalError for missing
            // OutputFileMap keys.
            if let Err(err) = route_call(&mut state, &record, chr, strand, call, read_identity) {
                state.cleanup_partial_outputs();
                return Err(err);
            }
        }
        // SE: records_processed and call_strings_processed both increment
        // by 1 per record (Perl `sequences_count` == `methylation_call_strings`
        // for SE input). Phase C.2 (#864) addition.
        state.report.records_processed = state.report.records_processed.saturating_add(1);
        state.report.call_strings_processed = state.report.call_strings_processed.saturating_add(1);
    }

    // Fold the io-layer skip counts (secondary/supplementary) into the report
    // before finalize. Zero unless `--allow_discordant` set the filter.
    fold_skip_counts(&mut state, reader.skip_counts());

    // Post-loop: no `cleanup_partial_outputs` on finalize failure — the data
    // is already on disk, and the contract (state.rs::finalize doc) is that
    // post-finalize errors don't trigger cleanup.
    state.finalize(config)?;
    Ok(())
}

/// Fold io-layer [`crate::io::SkipCounts`] into a state's splitting report.
fn fold_skip_counts(state: &mut ExtractState, sc: crate::io::SkipCounts) {
    state.report.secondary_skipped = state.report.secondary_skipped.saturating_add(sc.secondary);
    state.report.supplementary_skipped = state
        .report
        .supplementary_skipped
        .saturating_add(sc.supplementary);
}

/// PE extraction main loop (Phase C).
///
/// Pairs adjacent records (R1 then R2) via [`BismarkPair::from_mates`],
/// which enforces qname-equality and pairs by file order (R1 = first-in-file;
/// the SAM R1/R2 FLAG bits are NOT consulted — Bismark swaps them for
/// non-directional CTOT/CTOB pairs, see #1030). R2 calls overlapping
/// R1's reference span are dropped via [`drop_overlap`] when
/// `config.no_overlap` is true (PE default; `--include_overlap` flips it).
/// Per-mate ignore-region trims (`--ignore_r2`, `--ignore_3prime_r2`) are
/// applied via the same per-record kernel as SE.
///
/// Per SPEC §7.3 + §6.1: routing keys on the **pair-strand** (R1's
/// `record_strand`), NOT each mate's `record_strand`. This closes the
/// "one pair split across multiple files" bug class structurally.
///
/// # Splitting-report counters
///
/// **Phase C.2 correction (#864):** increments two counters per pair:
///
/// - `records_processed += 1` per pair — matches Perl
///   `bismark_methylation_extractor:2459` / `$counting{sequences_count}`
///   which drives the line-2482 report literal
///   `"Processed N lines in total"`. For PE, N = pair count, NOT 2×pairs.
///
/// - `call_strings_processed += 2` per pair — matches Perl
///   `bismark_methylation_extractor:2451`
///   (`$methylation_call_strings_processed += 2`) which drives the
///   line-2483 report literal `"Total number of methylation call strings
///   processed: 2N"`.
///
/// Rev 0 of Phase B (and rev 1 carried it forward) added 2 per pair to
/// `records_processed` citing Perl line 2451, but `:2451` is the
/// `methylation_call_strings` counter, not `sequences_count`. Phase C.2
/// splits the two counters and fixes both increment sites here AND in
/// `parallel.rs::worker_loop`.
///
/// # Errors
///
/// - [`BismarkExtractorError::UnpairedFinalRecord`] — odd-numbered record count.
/// - [`BismarkExtractorError::MateChromosomeMismatch`] — R1/R2 on different chromosomes.
/// - [`BismarkExtractorError::BismarkIo`] — `BismarkPair::from_mates` qname/identity failure.
/// - Any error from [`extract_calls`] (invalid XM byte) or [`route_call`] (I/O failure).
///
/// On any pre-finalize error, runs [`ExtractState::cleanup_partial_outputs`]
/// to remove all 12 partial files before propagating.
pub fn extract_pe(input: &Path, config: &ResolvedConfig) -> Result<(), BismarkExtractorError> {
    let mut reader = open_reader(input, /*cram_ref=*/ None)?;
    if config.allow_discordant {
        reader.set_filter(crate::io::RecordFilter {
            drop_secondary: true,
            drop_supplementary: true,
        });
    }
    let chr_table = build_chr_name_table(reader.header())?;

    let input_basename = derive_basename(input);
    let mut state = ExtractState::new(config, input, &input_basename, /*is_paired=*/ true)?;

    if config.allow_discordant {
        if let Err(e) = extract_pe_discordant(&mut reader, &mut state, &chr_table, config) {
            state.cleanup_partial_outputs();
            return Err(e);
        }
        fold_skip_counts(&mut state, reader.skip_counts());
        state.finalize(config)?;
        return Ok(());
    }

    let mut records = reader.records();
    loop {
        // Take R1.
        let r1 = match records.next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => {
                state.cleanup_partial_outputs();
                return Err(e.into());
            }
            None => break, // clean end of BAM
        };

        // Take R2.
        let r2 = match records.next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => {
                state.cleanup_partial_outputs();
                return Err(e.into());
            }
            None => {
                let qname = r1
                    .inner()
                    .name()
                    .map(|n| String::from_utf8_lossy(n.as_ref()).into_owned());
                state.cleanup_partial_outputs();
                return Err(BismarkExtractorError::UnpairedFinalRecord { qname });
            }
        };

        // Construct the pair (qname-eq enforced by bismark-io; paired by file
        // order, not the R1/R2 FLAG bits — #1030).
        let pair = match BismarkPair::from_mates(r1, r2) {
            Ok(p) => p,
            Err(e) => {
                state.cleanup_partial_outputs();
                return Err(e.into());
            }
        };

        if let Err(e) = handle_one_pair(&pair, &mut state, &chr_table, config) {
            state.cleanup_partial_outputs();
            return Err(e);
        }

        // Phase C.2 (#864): two split counters.
        // - records_processed += 1 per pair (Perl :2459, `sequences_count`)
        //   drives "Processed N lines in total" with N = pair count.
        // - call_strings_processed += 2 per pair (Perl :2451) drives
        //   "Total number of methylation call strings processed: 2N".
        state.report.records_processed = state.report.records_processed.saturating_add(1);
        state.report.call_strings_processed = state.report.call_strings_processed.saturating_add(2);
    }

    state.finalize(config)?;
    Ok(())
}

/// Per-pair handler: resolve chr, extract calls from both mates, drop
/// overlap if configured, route to split files.
///
/// Rev 1 (Reviewer B L3): chr name resolved once (after the
/// `MateChromosomeMismatch` defensive check) and reused for both R1 and R2
/// routing. The defensive check guarantees R1 and R2 share a refid by the
/// time we look up the chr name.
fn handle_one_pair(
    pair: &BismarkPair,
    state: &mut ExtractState,
    chr_table: &[String],
    config: &ResolvedConfig,
) -> Result<(), BismarkExtractorError> {
    // Cross-chr defensive check. Both refids resolved here so we can name
    // them in the error message; same convention as Phase B's SE refid path.
    let r1_refid = pair.r1().inner().reference_sequence_id().ok_or_else(|| {
        BismarkExtractorError::InternalError {
            message: "PE R1 missing reference_sequence_id; bismark-io::records should have \
                      filtered this as unmapped (FLAG & 0x4)"
                .to_string(),
        }
    })?;
    let r2_refid = pair.r2().inner().reference_sequence_id().ok_or_else(|| {
        BismarkExtractorError::InternalError {
            message: "PE R2 missing reference_sequence_id; bismark-io::records should have \
                      filtered this as unmapped (FLAG & 0x4)"
                .to_string(),
        }
    })?;
    if r1_refid != r2_refid {
        let qname = pair
            .r1()
            .inner()
            .name()
            .map(|n| String::from_utf8_lossy(n.as_ref()).into_owned())
            .unwrap_or_else(|| "<unnamed>".to_string());
        return Err(BismarkExtractorError::MateChromosomeMismatch {
            qname,
            r1_refid,
            r2_refid,
        });
    }

    let chr = chr_table
        .get(r1_refid)
        .ok_or_else(|| BismarkExtractorError::InternalError {
            message: format!(
                "pair refid {} out of range vs header (count {})",
                r1_refid,
                chr_table.len()
            ),
        })?
        .as_str();

    let pair_strand = pair.pair_strand();

    let mbias_only_silence = config.is_mbias_only();
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

    for call in r1_calls {
        route_call(state, pair.r1(), chr, pair_strand, call, ReadIdentity::R1)?;
    }
    for call in r2_calls {
        route_call(state, pair.r2(), chr, pair_strand, call, ReadIdentity::R2)?;
    }
    Ok(())
}

/// `--allow_discordant` single-threaded PE driver (the byte-identity reference
/// for `parallel.rs`'s discordant producer). Orphan-tolerant pairing (qname
/// pre-compare + one-slot pushback) + geometric classification, dispatching to
/// [`handle_one_pair`] (Concordant), [`handle_one_pair_independent`], or
/// [`handle_one_orphan`]. Counter semantics mirror `parallel.rs`'s
/// `process_pe` / `process_pe_independent` / `process_orphan` exactly.
fn extract_pe_discordant(
    reader: &mut AnyReader<std::io::BufReader<std::fs::File>, std::fs::File>,
    state: &mut ExtractState,
    chr_table: &[String],
    config: &ResolvedConfig,
) -> Result<(), BismarkExtractorError> {
    let mut records = reader.records();
    let mut pending: Option<BismarkRecord> = None;
    loop {
        // R1: a pushed-back record or the next read.
        let r1 = match pending.take() {
            Some(r) => r,
            None => match records.next() {
                Some(Ok(r)) => r,
                Some(Err(e)) => return Err(e.into()),
                None => break, // clean EOF
            },
        };
        // R2.
        let r2 = match records.next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => {
                // R1 is a valid mapped primary → call it as an orphan, then
                // propagate the read error.
                handle_one_orphan(&r1, state, chr_table, config)?;
                return Err(e.into());
            }
            None => {
                // EOF after R1: the historical `UnpairedFinalRecord` is now an
                // orphan (its mate is simply absent).
                handle_one_orphan(&r1, state, chr_table, config)?;
                break;
            }
        };

        // qname pre-compare via the shared pairing predicate (the same one
        // `from_mates` uses). A mismatch → R1's mate was filtered/unmapped, so
        // R1 is an orphan and R2 belongs to the next template.
        if !BismarkPair::qnames_match(&r1, &r2) {
            handle_one_orphan(&r1, state, chr_table, config)?;
            pending = Some(r2);
            continue;
        }

        let r1_refid = resolve_refid(r1.inner().reference_sequence_id(), "R1")?;
        let r2_refid = resolve_refid(r2.inner().reference_sequence_id(), "R2")?;
        let pair = BismarkPair::from_mates(r1, r2)?;
        match classify_pair(&pair, r1_refid, r2_refid) {
            PairClass::Concordant => {
                handle_one_pair(&pair, state, chr_table, config)?;
                state.report.records_processed = state.report.records_processed.saturating_add(1);
                state.report.call_strings_processed =
                    state.report.call_strings_processed.saturating_add(2);
            }
            PairClass::Independent => {
                handle_one_pair_independent(&pair, state, chr_table, config, r1_refid, r2_refid)?;
            }
        }
    }
    Ok(())
}

/// Resolve a record's `reference_sequence_id` (`Option<usize>`) or fail with an
/// `InternalError` (a mapped primary always has one).
fn resolve_refid(refid: Option<usize>, mate: &str) -> Result<usize, BismarkExtractorError> {
    refid.ok_or_else(|| BismarkExtractorError::InternalError {
        message: format!("PE {mate} missing reference_sequence_id"),
    })
}

/// Resolve a chromosome name from the table or fail with an `InternalError`.
fn chr_name(chr_table: &[String], refid: usize) -> Result<&str, BismarkExtractorError> {
    chr_table
        .get(refid)
        .map(String::as_str)
        .ok_or_else(|| BismarkExtractorError::InternalError {
            message: format!(
                "refid {} out of range vs header (count {})",
                refid,
                chr_table.len()
            ),
        })
}

/// `--allow_discordant` Independent-pair handler (single-threaded reference for
/// `parallel.rs::process_pe_independent`). Each mate called independently,
/// routed by its own `record_strand`, against its own chromosome; same-chr
/// overlaps deduped generically (keep R1) via [`drop_overlap_generic`].
fn handle_one_pair_independent(
    pair: &BismarkPair,
    state: &mut ExtractState,
    chr_table: &[String],
    config: &ResolvedConfig,
    r1_refid: usize,
    r2_refid: usize,
) -> Result<(), BismarkExtractorError> {
    let r1_chr = chr_name(chr_table, r1_refid)?;
    let r2_chr = chr_name(chr_table, r2_refid)?;
    let r1_strand = pair.r1().record_strand();
    let r2_strand = pair.r2().record_strand();
    let mbias_only_silence = config.is_mbias_only();

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
    let r2_calls = if r1_refid == r2_refid && config.no_overlap {
        drop_overlap_generic(&r1_calls, r2_calls_raw)
    } else {
        r2_calls_raw
    };

    for call in r1_calls {
        route_call(state, pair.r1(), r1_chr, r1_strand, call, ReadIdentity::R1)?;
    }
    for call in r2_calls {
        route_call(state, pair.r2(), r2_chr, r2_strand, call, ReadIdentity::R2)?;
    }

    state.report.records_processed = state.report.records_processed.saturating_add(1);
    state.report.call_strings_processed = state.report.call_strings_processed.saturating_add(2);
    if r1_refid == r2_refid {
        state.report.pairs_same_chr_independent =
            state.report.pairs_same_chr_independent.saturating_add(1);
    } else {
        state.report.pairs_cross_chr_independent =
            state.report.pairs_cross_chr_independent.saturating_add(1);
    }
    Ok(())
}

/// `--allow_discordant` orphan handler (single-threaded reference for
/// `parallel.rs::process_orphan`). Called single-end style: trims + M-bias slot
/// by read identity, routed by the record's own `record_strand`.
fn handle_one_orphan(
    record: &BismarkRecord,
    state: &mut ExtractState,
    chr_table: &[String],
    config: &ResolvedConfig,
) -> Result<(), BismarkExtractorError> {
    let identity = record.read_identity();
    let refid = resolve_refid(record.inner().reference_sequence_id(), "orphan")?;
    let chr = chr_name(chr_table, refid)?;
    let strand = record.record_strand();
    let (ignore_5p, ignore_3p) = match identity {
        ReadIdentity::R2 => (config.ignore_5p_r2, config.ignore_3p_r2),
        ReadIdentity::R1 | ReadIdentity::Single => (config.ignore_5p_r1, config.ignore_3p_r1),
    };
    let calls = extract_calls(record, ignore_5p, ignore_3p, config.is_mbias_only())?;
    for call in calls {
        // `route_call` selects the M-bias slot from `identity` (R2 → 1, else 0),
        // matching `process_orphan`.
        route_call(state, record, chr, strand, call, identity)?;
    }
    state.report.records_processed = state.report.records_processed.saturating_add(1);
    state.report.call_strings_processed = state.report.call_strings_processed.saturating_add(1);
    state.report.orphan_reads_called = state.report.orphan_reads_called.saturating_add(1);
    Ok(())
}
