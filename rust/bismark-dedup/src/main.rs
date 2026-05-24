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

    let report = pipeline::run_single(
        input,
        &out_path,
        config.cram_ref.as_deref(),
        is_paired,
        file_label,
    )?;
    report.write_to(&report_path)?;
    eprintln!("{}", report.format());
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

    let report = pipeline::run_multiple(
        &config.files,
        &out_path,
        config.cram_ref.as_deref(),
        is_paired,
        file_label,
    )?;
    report.write_to(&report_path)?;
    eprintln!("{}", report.format());
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
    // cheap (the header is small and decompressed once).
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
