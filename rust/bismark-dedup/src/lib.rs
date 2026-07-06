//! `bismark-dedup` — Rust port of Bismark Perl's `deduplicate_bismark` script.
//!
//! This crate is the first downstream binary of the Bismark Rust rewrite, built
//! on top of [`bismark_io`] for all BAM/SAM/CRAM I/O. The binary is installed as
//! `deduplicate_bismark` during the v0.26 → v1.0 coexistence period.
//!
//! See `~/.claude/plans/05242026_bismark-dedup-v1/PLAN.md` (rev 3) for the
//! design contract, behaviour specification, and phased implementation plan.
//!
//! ## Status
//!
//! **Phase C in progress.** Public API surface so far:
//!
//! - [`DedupKey`] — the value used to detect duplicates (SE = 3-tuple,
//!   PE = 4-tuple). Stable 16-byte `#[repr(C)]` layout.
//! - [`DedupState`] — accumulates the seen-set, duplicate-positions set,
//!   and running counters. [`DedupState::observe`] is the one-record
//!   entry point.
//! - [`DedupReport`] — byte-equal-to-Perl dedup report formatter.
//! - [`pipeline::run_single`] / [`pipeline::run_multiple`] — the
//!   end-to-end dedup pipelines for one input file or several inputs
//!   concatenated. Both wire [`bismark_io`] reader/writer to
//!   [`DedupState::observe`].
//! - [`filename`] — Perl-compatible output-stem derivation.
//! - [`BismarkDedupError`] — typed errors raised at the orchestration
//!   layer.
//!
//! CLI surface, integration tests on seeded-dup fixtures, and the 10M PE
//! WGBS byte-identity gate land in Phases D through G as separate
//! sub-issues with their own dual-review cycle.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod dedup;
pub mod error;
pub mod filename;
pub mod pipeline;
pub mod report;

pub use cli::UmiMode;
pub use dedup::{DedupKey, DedupState, UmiDedupKey, UmiDedupState};
pub use error::BismarkDedupError;
pub use report::DedupReport;

use std::path::Path;
use std::path::PathBuf;

use crate::cli::Cli;
use crate::cli::ResolvedConfig;

/// The uniform suite `--version` one-liner via [`bismark_meta::version_line`]:
/// `deduplicate_bismark (Bismark Rust suite) v<version> (<hash> — <os>/<arch> — built <ts>)`.
#[must_use]
pub fn version_string() -> String {
    bismark_meta::version_line("deduplicate_bismark")
}

/// Binary entry point — shared by this crate's own `main.rs` and the `bismark`
/// meta-crate's `deduplicate_bismark` bin (so `cargo install bismark` and
/// `cargo install bismark-dedup` behave identically). Parses the CLI, handles
/// `--version`, then dispatches to [`run`]. Exit: `0` ok · `1` error (clap
/// handles `2` parse errors before this). The `#[global_allocator]` stays in
/// each binary crate root.
#[must_use]
pub fn run_main() -> std::process::ExitCode {
    use clap::Parser;
    let cli = Cli::parse();

    // `--version` / `-V` is handled here (clap auto-version is disabled
    // in src/cli.rs so we can emit our custom provenance string).
    if cli.version {
        println!("{}", version_string());
        return std::process::ExitCode::SUCCESS;
    }

    match run(cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<(), BismarkDedupError> {
    let config = cli.validate()?;

    // Ensure output directory exists. Matches Perl line 1248.
    if !config.output_dir.as_os_str().is_empty() && config.output_dir != Path::new(".") {
        std::fs::create_dir_all(&config.output_dir)?;
    }

    // v1.2.1-beta.1: bcl-convert auto-detect (closes #842, resolves the
    // v1.2 deferral noted in #792's closing comment).
    //
    // Perl `test_readIDs_for_bclconvert` (deduplicate_bismark:915-995) peeks
    // the first record's qname and matches against the bcl-convert internal
    // UMI regex. Perl gates the call on `$rrbs` (set by ANY of `--barcode`,
    // `--umi`, OR `--bclconvert`), so Perl runs the test under all three
    // UMI flags — including `--bclconvert`, where on barcode-format qnames
    // Perl takes the harmless-success branch and emits the standard
    // "UMI mode" banner (not "bcl-convert UMI mode").
    //
    // The Rust port narrows the gate to `--barcode` / `--umi` only. Under
    // `--bclconvert` the auto-detect is skipped; on barcode-format qnames
    // the failure surfaces later via `UmiExtractionFailed` (the extractor
    // can't match the bcl-convert regex against tail-only qnames). End
    // result for the user is the same (informative error), but the STDERR
    // sequence diverges from Perl (Rust emits "bcl-convert UMI mode"
    // banner first, then fails; Perl would emit "UMI mode" then fail).
    // Pure-rust strict-byte-identity on STDERR is not in scope for the
    // v1.2.1-beta.1 patch — tracked as a v1.x polish item.
    //
    // The auto-detect ALSO operates on the first MAPPED record (noodles
    // silently filters `flags & 0x4` unmapped reads), whereas Perl tests
    // the first non-header record regardless of mapping flag. Harmless in
    // practice (uniform qnames across all records of a Bismark BAM) but
    // documented for future-Felix.
    if config.umi_mode == Some(crate::cli::UmiMode::Barcode) {
        for input in &config.files {
            check_bclconvert_format_conflict(input, &config)?;
        }
    }

    // Phase B (v1.2): UMI-mode startup banner. Two distinct strings in
    // Perl `deduplicate_bismark`:
    //   line 167: warn "Deduplicating data in UMI mode\n";
    //   line 172: warn "Deduplicating data in bcl-convert UMI mode\n";
    // No leading `\n` — Perl's `warn` writes the string verbatim.
    match config.umi_mode {
        Some(crate::cli::UmiMode::Barcode) => {
            eprintln!("Deduplicating data in UMI mode");
        }
        Some(crate::cli::UmiMode::Bclconvert) => {
            eprintln!("Deduplicating data in bcl-convert UMI mode");
        }
        None => {}
    }

    // Soft warning for --parallel values past the measured saturation
    // point. The Phase D oxy benchmark on 10M PE WGBS showed N=8 gave
    // zero additional speedup over N=4 (both at ~4.88× vs N=1) — the
    // dedup state itself is single-threaded, so only BGZF
    // (de)compression scales. Anything past N=4 is unlikely to help
    // and may add scheduling overhead. We don't block: hardware and
    // input characteristics differ and a user on much-bigger metal may
    // legitimately want to probe past the typical sweet spot.
    if config.parallel > 4 {
        eprintln!(
            "warning: --parallel {} exceeds the typical sweet spot (N ≤ 4); measured \
             saturation at N=4 on the 10M PE WGBS benchmark (N=8 gave zero additional \
             speedup) — additional workers will probably give diminishing or no further \
             benefit on this BGZF-bound workload",
            config.parallel
        );
    }

    if config.multiple {
        process_multiple(&config)?;
    } else {
        for input in &config.files {
            process_one(input, &config)?;
        }
    }
    Ok(())
}

/// Process a single positional input. Used for the default (non-`--multiple`)
/// path: each file is deduplicated independently with its own report.
fn process_one(input: &Path, config: &ResolvedConfig) -> Result<(), BismarkDedupError> {
    let is_paired = resolve_paired_mode(input, config)?;
    let (out_path, report_path, file_label) = derive_output_paths(input, config, false)?;

    eprintln!("Output file is: {}", out_path.display());

    // v1.1: dispatch on AlignmentKind for the threaded path. BAM with
    // parallel > 1 → run_single_parallel; CRAM with parallel > 1 →
    // warn-and-fall-back to single-threaded; everything else →
    // existing single-threaded run_single.
    let kind = bismark_io::AlignmentKind::from_path(input)?;
    let use_parallel = config.parallel > 1 && matches!(kind, bismark_io::AlignmentKind::Bam);

    if config.parallel > 1 && matches!(kind, bismark_io::AlignmentKind::Cram) {
        eprintln!(
            "warning: --parallel {} is currently BAM-only; \
             CRAM input/output runs single-threaded in this release",
            config.parallel
        );
    }

    let report = match (use_parallel, config.umi_mode) {
        (true, Some(umi_mode)) => {
            let parallel = std::num::NonZero::new(config.parallel)
                .expect("Cli::validate rejects parallel == 0");
            eprintln!("BGZF threading: {} worker(s) per reader/writer", parallel);
            pipeline::run_single_parallel_umi(
                input, &out_path, is_paired, file_label, parallel, umi_mode,
            )?
        }
        (true, None) => {
            let parallel = std::num::NonZero::new(config.parallel)
                .expect("Cli::validate rejects parallel == 0");
            eprintln!("BGZF threading: {} worker(s) per reader/writer", parallel);
            pipeline::run_single_parallel(input, &out_path, is_paired, file_label, parallel)?
        }
        (false, Some(umi_mode)) => pipeline::run_single_umi(
            input,
            &out_path,
            config.cram_ref.as_deref(),
            is_paired,
            file_label,
            umi_mode,
        )?,
        (false, None) => pipeline::run_single(
            input,
            &out_path,
            config.cram_ref.as_deref(),
            is_paired,
            file_label,
        )?,
    };
    if report.count() == 0 {
        eprintln!(
            "Input contained no alignments — wrote an empty (header-only) deduplicated file \
             and a zero-count report (exit 0)."
        );
    }
    report.write_to(&report_path)?;
    eprintln!("{}", report.format_stderr());
    Ok(())
}

/// Process all positional inputs as one combined sample (`--multiple` mode).
fn process_multiple(config: &ResolvedConfig) -> Result<(), BismarkDedupError> {
    let primary = &config.files[0];
    let is_paired = resolve_paired_mode(primary, config)?;
    let (out_path, report_path, file_label) = derive_output_paths(primary, config, true)?;

    eprintln!(
        "Multiple Input files for the same sample selected — all input files treated as one big single file."
    );
    for f in &config.files {
        eprintln!("  {}", f.display());
    }
    eprintln!();
    eprintln!("Output file is: {}", out_path.display());

    // v1.1: parallel dispatch. All-BAM inputs + parallel > 1 →
    // run_multiple_parallel. Any CRAM with parallel > 1 → warn and fall
    // back to single-threaded.
    let kinds: Result<Vec<_>, _> = config
        .files
        .iter()
        .map(|p| bismark_io::AlignmentKind::from_path(p))
        .collect();
    let kinds = kinds?;
    let all_bam = kinds.iter().all(|k| *k == bismark_io::AlignmentKind::Bam);
    let any_cram = kinds.contains(&bismark_io::AlignmentKind::Cram);

    let use_parallel = config.parallel > 1 && all_bam;

    if config.parallel > 1 && any_cram {
        eprintln!(
            "warning: --parallel {} is currently BAM-only; \
             CRAM input/output runs single-threaded in this release",
            config.parallel
        );
    }

    let report = match (use_parallel, config.umi_mode) {
        (true, Some(umi_mode)) => {
            let parallel = std::num::NonZero::new(config.parallel)
                .expect("Cli::validate rejects parallel == 0");
            eprintln!("BGZF threading: {} worker(s) per reader/writer", parallel);
            pipeline::run_multiple_parallel_umi(
                &config.files,
                &out_path,
                is_paired,
                file_label,
                parallel,
                umi_mode,
            )?
        }
        (true, None) => {
            let parallel = std::num::NonZero::new(config.parallel)
                .expect("Cli::validate rejects parallel == 0");
            eprintln!("BGZF threading: {} worker(s) per reader/writer", parallel);
            pipeline::run_multiple_parallel(
                &config.files,
                &out_path,
                is_paired,
                file_label,
                parallel,
            )?
        }
        (false, Some(umi_mode)) => pipeline::run_multiple_umi(
            &config.files,
            &out_path,
            config.cram_ref.as_deref(),
            is_paired,
            file_label,
            umi_mode,
        )?,
        (false, None) => pipeline::run_multiple(
            &config.files,
            &out_path,
            config.cram_ref.as_deref(),
            is_paired,
            file_label,
        )?,
    };
    if report.count() == 0 {
        eprintln!(
            "Input contained no alignments — wrote an empty (header-only) deduplicated file \
             and a zero-count report (exit 0)."
        );
    }
    report.write_to(&report_path)?;
    eprintln!("{}", report.format_stderr());
    Ok(())
}

/// v1.2.1-beta.1: peek the first record's qname and reject if it looks
/// like bcl-convert format while the user is in `--barcode`/`--umi` mode.
/// Closes #842; mirrors Perl `test_readIDs_for_bclconvert`
/// (`deduplicate_bismark:915-995`).
///
/// Cost: one extra reader-open + one record read per input file. Cheap
/// for BAM (microseconds) and SAM; slightly more for CRAM (needs the
/// FASTA reference repository built twice). Acceptable for the safety
/// gain.
fn check_bclconvert_format_conflict(
    input: &Path,
    config: &ResolvedConfig,
) -> Result<(), BismarkDedupError> {
    let mut reader = bismark_io::open_reader(input, config.cram_ref.as_deref())?;
    // We only need the first record's qname — peek and discard.
    let first_record = reader.records().next();
    let first_qname: Vec<u8> = match first_record {
        Some(Ok(rec)) => rec
            .inner()
            .name()
            .map(|n| AsRef::<[u8]>::as_ref(n).to_vec())
            .unwrap_or_default(),
        Some(Err(e)) => return Err(e.into()),
        None => return Ok(()), // Empty input — downstream handles it gracefully (zero-count report).
    };

    if bismark_io::umi::extract_bclconvert(&first_qname).is_some() {
        // Mirror Perl `test_readIDs_for_bclconvert` narration at
        // `deduplicate_bismark:976-980`: emit the parsed bcl-convert
        // barcode + i7 index to stderr BEFORE the fatal error, so
        // users see what we found. Re-run extraction here to capture
        // both groups (the extractor returns only group 1 normally).
        // Captured-group regex equivalent of Perl's
        // `/:([CAGTN\+]+)_\d:N:\d:([CAGTN\+]+)$/`.
        let qname_str = String::from_utf8_lossy(&first_qname);
        if let Some((bcl_umi, i7)) = parse_bclconvert_groups(&qname_str) {
            eprintln!("\nTwo barcodes found in read ID (>>{qname_str}<<):");
            eprintln!("Barcode 1: {bcl_umi} (suspected bcl-convert UMI)");
            eprintln!("Barcode 2: {i7} (suspected multiplexing index)\n");
        }
        return Err(BismarkDedupError::BclconvertFormatWithBarcodeFlag {
            qname: qname_str.into_owned(),
        });
    }
    Ok(())
}

/// Parse the `:UMI_<mate>:N:<d>:<i7>` tail of a qname into the two
/// captured groups `(bcl_umi, i7)`. Returns `None` if the qname doesn't
/// match. Mirrors Perl's captured-group regex at
/// `deduplicate_bismark:971` (`:([CAGTN\+]+)_\d:N:\d:([CAGTN\+]+)$`).
fn parse_bclconvert_groups(qname: &str) -> Option<(&str, &str)> {
    // Find `:N:<d>:` substring; the i7 is everything after it.
    let n_idx = qname.find(":N:")?;
    let after_n = &qname[n_idx + 3..]; // strip ":N:"
    // Skip one digit + ":"
    let mut chars = after_n.char_indices();
    let (_, c) = chars.next()?;
    if !c.is_ascii_digit() {
        return None;
    }
    let (i_after_digit, c2) = chars.next()?;
    if c2 != ':' {
        return None;
    }
    let i7 = &after_n[i_after_digit + 1..];

    // Now find the umi: the segment before `:N:` is `...:UMI_<mate>`
    let before_n = &qname[..n_idx];
    let umi_underscore = before_n.rfind('_')?;
    let umi_colon = before_n[..umi_underscore].rfind(':')?;
    let umi = &before_n[umi_colon + 1..umi_underscore];
    if umi.is_empty() || i7.is_empty() {
        return None;
    }
    Some((umi, i7))
}

/// Resolve the SE/PE mode: explicit flag wins, else auto-detect from the
/// input's `@PG ID:Bismark` header line.
fn resolve_paired_mode(input: &Path, config: &ResolvedConfig) -> Result<bool, BismarkDedupError> {
    if let Some(mode) = config.explicit_mode {
        return Ok(mode);
    }
    // Auto-detect: open the reader briefly to inspect the header. The
    // file will be opened again by the pipeline; the redundant open is
    // cheap for BAM/SAM (microseconds) but **non-trivial for CRAM**
    // (must build the FASTA reference repository twice). For v1.0,
    // accepted as a known overhead; v1.1 will refactor auto-detect into
    // the pipeline itself to share the reader. Users who care can pass
    // `-s`/`-p` explicitly to skip auto-detect.
    let reader = bismark_io::open_reader(input, config.cram_ref.as_deref())?;
    match pipeline::detect_paired_from_header(reader.header()) {
        Some(is_paired) => {
            eprintln!(
                "Auto-detected library type from @PG line: {}",
                if is_paired {
                    "paired-end"
                } else {
                    "single-end"
                }
            );
            Ok(is_paired)
        }
        None => Err(BismarkDedupError::CannotAutoDetectMode {
            input: input.to_path_buf(),
        }),
    }
}

/// Derive the output BAM/SAM path, report path, and file_label string
/// for the given input. The `multiple` flag selects the `.multiple.`
/// infix on both filenames.
fn derive_output_paths(
    input: &Path,
    config: &ResolvedConfig,
    multiple: bool,
) -> Result<(PathBuf, PathBuf, String), BismarkDedupError> {
    let stem = filename::derive_output_stem(input, config.outfile.as_deref());
    let out_name = filename::output_filename(&stem, multiple, config.sam_output);
    let report_name = filename::report_filename(&stem, multiple);
    let out_path = config.output_dir.join(out_name);
    let report_path = config.output_dir.join(report_name);
    // File label for the report content: Perl echoes $ARGV[i] verbatim,
    // so use the input path as the user supplied it.
    let file_label = input.to_string_lossy().into_owned();
    Ok((out_path, report_path, file_label))
}
