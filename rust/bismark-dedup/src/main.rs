//! Binary entry point for `deduplicate_bismark_rs`.
//!
//! Parses CLI via [`bismark_dedup::cli::Cli`], validates the flag
//! combinations into a [`bismark_dedup::cli::ResolvedConfig`], then
//! dispatches to [`bismark_dedup::pipeline::run_single`] (one file per
//! invocation, the default) or [`bismark_dedup::pipeline::run_multiple`]
//! (all positional inputs combined, `--multiple` flag).
//!
//! Exit codes:
//! - `0` — success
//! - `1` — any [`BismarkDedupError`]
//! - `2` — clap parse error (clap convention for usage errors)

use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use bismark_dedup::cli::Cli;
use bismark_dedup::cli::ResolvedConfig;
use bismark_dedup::error::BismarkDedupError;
use bismark_dedup::filename;
use bismark_dedup::pipeline;
use bismark_dedup::version_string;

fn main() -> ExitCode {
    let cli = Cli::parse();

    // `--version` / `-V` is handled here (clap auto-version is disabled
    // in src/cli.rs so we can emit our custom provenance string).
    if cli.version {
        println!("{}", version_string());
        return ExitCode::SUCCESS;
    }

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<(), BismarkDedupError> {
    let config = cli.validate()?;

    // Ensure output directory exists. Matches Perl line 1248.
    if !config.output_dir.as_os_str().is_empty() && config.output_dir != Path::new(".") {
        std::fs::create_dir_all(&config.output_dir)?;
    }

    // Phase B (v1.2): UMI-mode startup banner. Two distinct strings in
    // Perl `deduplicate_bismark`:
    //   line 167: warn "Deduplicating data in UMI mode\n";
    //   line 172: warn "Deduplicating data in bcl-convert UMI mode\n";
    // Perl picks based on `test_readIDs_for_bclconvert` auto-detect; the
    // Rust port skips that (deferred to v1.3) and uses the user's flag.
    // No leading `\n` — Perl's `warn` writes the string verbatim.
    match config.umi_mode {
        Some(bismark_dedup::cli::UmiMode::Barcode) => {
            eprintln!("Deduplicating data in UMI mode");
        }
        Some(bismark_dedup::cli::UmiMode::Bclconvert) => {
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
    report.write_to(&report_path)?;
    eprintln!("{}", report.format_stderr());
    Ok(())
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
