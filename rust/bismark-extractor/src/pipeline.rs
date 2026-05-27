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

use bismark_io::{BismarkPair, ReadIdentity, open_reader};

use crate::call::extract_calls;
use crate::cli::ResolvedConfig;
use crate::error::BismarkExtractorError;
use crate::header::build_chr_name_table;
use crate::overlap::drop_overlap;
use crate::route::route_call;
use crate::state::ExtractState;

/// Strip a single Bismark-recognised suffix from the input path's basename.
///
/// Matches Perl `s/sam$/txt/; s/bam$/txt/; s/cram$/txt/` semantics:
/// **case-sensitive**, **single-extension only**. `foo.bam.gz` is NOT
/// transformed (Perl wouldn't either — `s/bam$/txt/` doesn't match `.gz`).
/// `foo.BAM` (uppercase) is left as `foo.BAM` (Perl regex is case-sensitive).
///
/// **Distinct from [`crate::mbias_writer::derive_mbias_basename`]** (Phase D):
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
pub fn extract_se(input: &Path, config: &ResolvedConfig) -> Result<(), BismarkExtractorError> {
    let mut reader = open_reader(input, /*cram_ref=*/ None)?;
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

        let calls = match extract_calls(&record, config.ignore_5p_r1, config.ignore_3p_r1) {
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
        state.report.records_processed = state.report.records_processed.saturating_add(1);
    }

    // Post-loop: no `cleanup_partial_outputs` on finalize failure — the data
    // is already on disk, and the contract (state.rs::finalize doc) is that
    // post-finalize errors don't trigger cleanup.
    state.finalize(config)?;
    Ok(())
}

/// PE extraction main loop (Phase C).
///
/// Pairs adjacent records (R1 then R2) via [`BismarkPair::from_mates`],
/// which enforces qname-equality and R1/R2 identity. R2 calls overlapping
/// R1's reference span are dropped via [`drop_overlap`] when
/// `config.no_overlap` is true (PE default; `--include_overlap` flips it).
/// Per-mate ignore-region trims (`--ignore_r2`, `--ignore_3prime_r2`) are
/// applied via the same per-record kernel as SE.
///
/// Per SPEC §7.3 + §6.1: routing keys on the **pair-strand** (R1's
/// `record_strand`), NOT each mate's `record_strand`. This closes the
/// "one pair split across multiple files" bug class structurally.
///
/// # Splitting-report counter
///
/// Increments `state.report.records_processed` by **2 per pair** to match
/// Perl `bismark_methylation_extractor:2451` (`$methylation_call_strings_processed += 2`)
/// and the line-2479 report literal `"Processed N lines in total"`. The
/// counter name `records_processed` reflects lines-in-BAM, not pair-count.
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
    let chr_table = build_chr_name_table(reader.header())?;

    let input_basename = derive_basename(input);
    let mut state = ExtractState::new(config, input, &input_basename, /*is_paired=*/ true)?;

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

        // Construct the pair (qname-eq + R1/R2 identity enforced by bismark-io).
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

        // Two BAM lines processed per iteration (rev 1 Reviewer A §1.5 /
        // Reviewer B L1: Perl line 2451 increments by 2 per pair).
        state.report.records_processed = state.report.records_processed.saturating_add(2);
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

    let r1_calls = extract_calls(pair.r1(), config.ignore_5p_r1, config.ignore_3p_r1)?;
    let r2_calls_raw = extract_calls(pair.r2(), config.ignore_5p_r2, config.ignore_3p_r2)?;

    let r2_calls = if config.no_overlap {
        drop_overlap(r2_calls_raw, pair)?
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
