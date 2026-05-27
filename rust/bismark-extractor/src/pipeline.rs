//! SE extraction pipeline (Phase B).
//!
//! Per SPEC §7.2: one record at a time, classify XM bytes, route to split
//! files + accumulate M-bias counters + bump splitting-report counters.
//!
//! PE (Phase C), multicore (Phase F), gzip (Phase E), bedGraph/cytosine_report
//! subprocess (Phase G), and non-default output modes (Phase E) are rejected
//! at `main::run`'s config-dispatch boundary, not here.

use std::path::Path;

use bismark_io::{ReadIdentity, open_reader};

use crate::call::extract_calls;
use crate::cli::ResolvedConfig;
use crate::error::BismarkExtractorError;
use crate::header::build_chr_name_table;
use crate::route::route_call;
use crate::state::ExtractState;

/// Strip a single Bismark-recognised suffix from the input path's basename.
///
/// Matches Perl `s/sam$/txt/; s/bam$/txt/; s/cram$/txt/` semantics:
/// **case-sensitive**, **single-extension only**. `foo.bam.gz` is NOT
/// transformed (Perl wouldn't either — `s/bam$/txt/` doesn't match `.gz`).
/// `foo.BAM` (uppercase) is left as `foo.BAM` (Perl regex is case-sensitive).
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
    let mut state = ExtractState::new(config, input, &input_basename)?;

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
