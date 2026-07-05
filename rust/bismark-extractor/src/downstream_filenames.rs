//! Phase 2 — in-process file-coupled orchestration of `bismark2bedGraph` +
//! `coverage2cytosine`.
//!
//! After extraction writes the per-context split files, the extractor drives
//! the bedGraph + cytosine-report tools **in-process** (no fork/exec, no Perl):
//! it builds the argv each standalone binary would receive, feeds it to that
//! crate's own `Cli::try_parse_from(..).validate()`, and calls its `run()`.
//! This supersedes the scaffolded Phase G Perl-subprocess chain (the
//! discovery/spawn/tee machinery was deleted in Phase 2).
//!
//! The module retains two responsibilities:
//!
//! 1. **Filename derivation** (`derive_bedgraph_filename` /
//!    `derive_coverage_filename` / `derive_cytosine_filename`) — mirrors Perl
//!    `bismark_methylation_extractor:325-330, :392-399, :419-420` to compute
//!    the output basenames the downstream tools should use. These are
//!    byte-identity-load-bearing (see the trailing-dot quirk below).
//! 2. **Argv construction** (`build_bismark2bedgraph_argv` /
//!    `build_coverage2cytosine_argv`) — emits the flags + positionals that the
//!    **Rust** `bismark_bedgraph::Cli` / `bismark_coverage2cytosine::Cli`
//!    accept. (Phase 2 T1 reconciled these from the old Perl-CLI spellings; the
//!    notable fix was c2c `--genome` → `--genome_folder`.)
//!
//! ## Byte-identity invariant (filenames)
//!
//! Perl `:325-330` strips the **literal** trailing letters `gz`, `sam`, `bam`,
//! `txt` (no leading dot). Chained extensions therefore preserve a trailing
//! dot: `foo.bam.gz` → `foo.bam.bedGraph`. No-extension inputs produce no
//! leading dot: `foo` → `foobedGraph`. The `derive_*` functions below mirror
//! Perl's regex pipeline step-by-step.
//!
//! ## c2c cov-path gotcha (Phase 2 NEW-1)
//!
//! Rust `coverage2cytosine` **ignores `--parent_dir`** and opens the positional
//! coverage file **verbatim from the CWD** (`report.rs` → `cov.rs::open_cov`).
//! The in-process orchestrator therefore passes the cov positional as an
//! **absolute** path (`output_dir.join(coverage_filename)`); a bare basename
//! would ENOENT whenever the extractor's CWD differs from `--output_dir`.
//! Output naming is unaffected (c2c derives report names from `--output`).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::Parser;

use crate::cli::ResolvedConfig;
use crate::error::BismarkExtractorError;

// ─── Filename derivation (Perl :325-330, :392-399, :419-420) ────────────────

/// Derive the bedGraph output filename from the input basename.
///
/// Mirrors Perl `bismark_methylation_extractor:325-330` step-by-step:
///
/// ```text
/// $out = basename($input);
/// $out =~ s/gz$//;   # strip literal "gz"  (no leading dot)
/// $out =~ s/sam$//;  # strip literal "sam"
/// $out =~ s/bam$//;  # strip literal "bam"
/// $out =~ s/txt$//;  # strip literal "txt"
/// $out =~ s/$/bedGraph/;  # append "bedGraph"
/// ```
///
/// Trailing-dot preservation is load-bearing for chained-extension inputs:
///
/// | Input | Output |
/// |-------|--------|
/// | `foo.bam` | `foo.bedGraph` |
/// | `foo.bam.gz` | **`foo.bam.bedGraph`** (trailing dot preserved) |
/// | `foo` (no ext) | **`foobedGraph`** (no leading dot) |
/// | `sample.fastq_bismark_bt2_pe.deduplicated.bam` | `sample.fastq_bismark_bt2_pe.deduplicated.bedGraph` |
pub fn derive_bedgraph_filename(input_basename: &str) -> String {
    let mut s = input_basename.to_string();
    for ext in &["gz", "sam", "bam", "txt"] {
        if let Some(stripped) = s.strip_suffix(ext) {
            s = stripped.to_string();
        }
    }
    s.push_str("bedGraph");
    s
}

/// Derive the `.bismark.cov.gz` coverage filename from the bedGraph filename.
/// Mirrors Perl `:419-420`: `s/bedGraph$/bismark.cov.gz/`.
pub fn derive_coverage_filename(bedgraph_filename: &str) -> String {
    if let Some(prefix) = bedgraph_filename.strip_suffix("bedGraph") {
        format!("{prefix}bismark.cov.gz")
    } else {
        // bedGraph filename produced by `derive_bedgraph_filename` always ends
        // in "bedGraph" by construction; this branch is defensive.
        format!("{bedgraph_filename}.bismark.cov.gz")
    }
}

/// Derive the cytosine-report filename from the bedGraph filename. Mirrors
/// Perl `:392-399`: strip `bedGraph` suffix; append `CpG_report.txt` (default)
/// or `CX_report.txt` (when `--CX`).
pub fn derive_cytosine_filename(bedgraph_filename: &str, cx_context: bool) -> String {
    let stem = bedgraph_filename
        .strip_suffix("bedGraph")
        .unwrap_or(bedgraph_filename);
    if cx_context {
        format!("{stem}CX_report.txt")
    } else {
        format!("{stem}CpG_report.txt")
    }
}

// ─── Argv builders (target the RUST bedGraph / c2c CLIs; Phase 2 T1) ─────────

/// Build the argv list (excluding argv[0]) for the in-process
/// `bismark_bedgraph::Cli`. Every emitted flag is one the **Rust**
/// `bismark2bedGraph` CLI accepts (`bismark-bedgraph/src/cli.rs`). Phase 2 T1
/// reconciled this from the old Perl-CLI spellings; the bedGraph side already
/// used the long forms (`--remove_spaces`, `--zero_based`) the Rust CLI exposes
/// directly.
///
/// `kept_split_files` are appended verbatim as positional arguments (the
/// post-empty-sweep set, absolute paths, sorted lexicographically).
pub fn build_bismark2bedgraph_argv(
    config: &ResolvedConfig,
    kept_split_files: &[PathBuf],
    bedgraph_filename: &str,
    output_dir: &Path,
) -> Vec<OsString> {
    let mut argv: Vec<OsString> = Vec::new();

    if config.remove_spaces {
        argv.push("--remove_spaces".into());
    }
    if config.cx_context {
        argv.push("--CX_context".into());
    }
    if config.no_header {
        argv.push("--no_header".into());
    }
    if config.gazillion {
        argv.push("--gazillion".into());
    }
    if config.ample_memory {
        argv.push("--ample_memory".into());
    } else {
        // Per Perl :347-352: when !ample_memory, ALWAYS push --buffer_size.
        // Default is "2G" matching Perl's $sort_size default at :1305.
        let size: &str = config.buffer_size.as_deref().unwrap_or("2G");
        argv.push("--buffer_size".into());
        argv.push(size.into());
    }
    if config.ucsc {
        argv.push("--ucsc".into());
    }
    if config.zero_based {
        argv.push("--zero_based".into());
    }
    argv.push("--cutoff".into());
    argv.push(config.cutoff.to_string().into());
    argv.push("--output".into());
    argv.push(bedgraph_filename.into());
    argv.push("--dir".into());
    argv.push(output_dir.as_os_str().to_owned());
    for f in kept_split_files {
        argv.push(f.as_os_str().to_owned());
    }
    argv
}

/// Build the argv for the in-process `bismark_coverage2cytosine::Cli`. Every
/// emitted flag is one the **Rust** `coverage2cytosine` CLI accepts
/// (`bismark-coverage2cytosine/src/cli.rs`).
///
/// Phase 2 T1: the genome flag is `--genome_folder` (the Rust CLI has no
/// `--genome` and does not infer abbreviations). `--parent_dir` is still passed
/// for argv shape but is **inert** in the Rust port (stored, never read).
///
/// Phase 2 NEW-1: the caller MUST pass `coverage_input_path` as an **absolute**
/// path — the Rust c2c opens the cov positional verbatim from the CWD, so a
/// bare basename ENOENTs whenever CWD ≠ `output_dir`.
pub fn build_coverage2cytosine_argv(
    config: &ResolvedConfig,
    coverage_input_path: &Path,
    cytosine_output_filename: &str,
    output_dir: &Path,
    genome_folder: &Path,
) -> Vec<OsString> {
    // Header section is unconditional; use vec![] for clippy + readability.
    // --parent_dir == --dir per Perl :404 (rev 1 I13). It is inert in the Rust
    // c2c (ignored at runtime) but kept for argv-shape parity.
    let mut argv: Vec<OsString> = vec![
        "--output".into(),
        cytosine_output_filename.into(),
        "--dir".into(),
        output_dir.as_os_str().to_owned(),
        "--genome_folder".into(),
        genome_folder.as_os_str().to_owned(),
        "--parent_dir".into(),
        output_dir.as_os_str().to_owned(),
    ];
    if config.zero_based {
        argv.push("--zero_based".into());
    }
    if config.cx_context {
        argv.push("--CX_context".into());
    }
    if config.split_by_chromosome {
        argv.push("--split_by_chromosome".into());
    }
    if config.gzip {
        argv.push("--gzip".into());
    }
    // Positional: the .bismark.cov.gz file, as an ABSOLUTE path (NEW-1).
    argv.push(coverage_input_path.as_os_str().to_owned());
    argv
}

// ─── In-process orchestrator ─────────────────────────────────────────────────

/// Basename of `p` as a `&str` (lossy-safe). Used by the no-CpG pre-check.
fn basename_str(p: &Path) -> std::borrow::Cow<'_, str> {
    p.file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or(std::borrow::Cow::Borrowed(""))
}

/// Drive `bismark2bedGraph` then (optionally) `coverage2cytosine` in-process.
///
/// Invoked from [`crate::state::ExtractState::finalize`] after the split files +
/// splitting report + M-bias.txt have been written, when `config.bedgraph` is
/// true. Runs AFTER the M-bias write so the console-output ordering after the
/// M-bias summary is preserved.
///
/// Steps:
/// 1. If `config.bedgraph` is false: no-op return `Ok(())`.
/// 2. **No-CpG / empty-input pre-check (Phase 2 T5, preserved in Phase 3a /
///    F3)**: if there is no usable input for bedGraph (under `--CX`, the `kept`
///    set is empty; otherwise no kept file's basename starts with `"CpG"`),
///    warn via the logger and **skip** the downstream steps, returning `Ok(())`
///    (exit 0). This matches Perl's *net output*: Perl `bismark2bedGraph:111` /
///    `coverage2cytosine:473` die on such input, but the Perl extractor's
///    `system()` calls are unchecked, so it finishes (exit 0, no downstream
///    files). We deliberately do NOT add a `sorted.is_empty()` branch — the
///    kept-set pre-check is the proven gate (the kept set is empty exactly when
///    the aggregator is empty, since the same calls feed both).
/// 3. Build + parse + validate the in-process `bismark2bedGraph` config, then
///    write the `.bedGraph`/`.cov.gz` (+ optional UCSC) **from the in-memory
///    `sorted` records** (Phase 3a / F2) — NOT by re-reading the kept files.
/// 4. If `config.cytosine_report`: build + parse + validate + run the in-process
///    `coverage2cytosine`, feeding it the `.bismark.cov.gz` written in step 3
///    (as an ABSOLUTE path — NEW-1). c2c still reads the on-disk `.cov.gz` (D4,
///    unchanged from Phase 2).
///
/// `sorted` is the `bismark_bedgraph::aggregate::ChrPositions` slice produced by
/// the extraction-time tee's `Aggregator::into_sorted()` (SPEC §4.3.3). It is
/// the authoritative input to the `.cov.gz`/`.bedGraph` writers; the kept files
/// remain in `b2bg_cfg.files` (used only by `validate()` to derive
/// filenames/cutoff/ucsc — `write_outputs_from_sorted` ignores them).
///
/// All filename derivations preserve Perl's trailing-dot quirk per
/// [`derive_bedgraph_filename`].
pub fn run_downstream_chain(
    config: &ResolvedConfig,
    input_basename: &str,
    output_dir: &Path,
    kept_split_files: &[PathBuf],
    sorted: &[bismark_bedgraph::aggregate::ChrPositions],
    is_empty_run: bool,
) -> Result<(), BismarkExtractorError> {
    if !config.bedgraph {
        return Ok(());
    }

    // ── Pre-check: usable bedGraph input? (T5 / F3) ──
    // Default (CpG-only) bedGraph reads ONLY files whose basename starts with
    // "CpG"; --CX reads all kept files. If nothing is usable, warn + skip.
    let usable = if config.cx_context {
        !kept_split_files.is_empty()
    } else {
        kept_split_files
            .iter()
            .any(|p| basename_str(p).starts_with("CpG"))
    };
    // Plan 06142026_empty-sample-extractor-c2c — DELIBERATE divergence from
    // Perl: a truly empty run (zero TOTAL methylation calls, `is_empty_run`)
    // must NOT skip — it falls through so `write_outputs_from_sorted(&_, [])`
    // emits a valid empty `<base>.bedGraph.gz` (a `track type=bedGraph` line +
    // 0 rows) + a 0-row `<base>.bismark.cov.gz`, and the c2c block below feeds
    // that empty `.cov` to coverage2cytosine — so methylseq's required output
    // globs match. RATIONALE for `&& !is_empty_run` (do NOT regress): a
    // has-calls-but-no-CpG default-bedGraph run has `is_empty_run == false` and
    // `usable == false`, so it MUST still skip (the legitimate Perl-faithful
    // boundary, guarded by `default_mode_no_cpg_calls_skips`).
    if !usable && !is_empty_run {
        let logger = crate::logging::Logger::from_config(config);
        logger.note(
            "Warning: no methylation calls usable for bedGraph were produced; \
             skipping the bedGraph/cytosine_report steps.",
        );
        return Ok(());
    }

    // ── Step 1: bismark2bedGraph config (in-process) ──
    let bedgraph_filename = derive_bedgraph_filename(input_basename);
    let b2bg_argv =
        build_bismark2bedgraph_argv(config, kept_split_files, &bedgraph_filename, output_dir);
    let mut b2bg_full: Vec<OsString> = vec!["bismark2bedGraph".into()];
    b2bg_full.extend(b2bg_argv);
    let b2bg_cli = bismark_bedgraph::Cli::try_parse_from(&b2bg_full).map_err(|e| {
        BismarkExtractorError::Downstream {
            tool: "bismark2bedGraph",
            message: e.to_string(),
        }
    })?;
    let b2bg_cfg = b2bg_cli
        .validate()
        .map_err(|e| BismarkExtractorError::Downstream {
            tool: "bismark2bedGraph",
            message: e.to_string(),
        })?;
    // Phase 3a (F2): write the `.bedGraph`/`.cov.gz` from the in-memory tee's
    // `sorted` records — replacing (NOT wrapping) the old
    // `bismark_bedgraph::run(&b2bg_cfg)` file-read so the `.cov.gz` is written
    // EXACTLY once (R-4). The cutoff is applied inside
    // `write_outputs_from_sorted` (R3), so the `.cov.gz` is the authoritative
    // post-cutoff set that c2c then reads (D4). `run()`'s only non-file-read
    // side effect was `create_dir_all(output_dir)`, already covered (the dir
    // was created at `OutputFileMap::new` during extraction).
    bismark_bedgraph::output::write_outputs_from_sorted(&b2bg_cfg, sorted).map_err(|e| {
        BismarkExtractorError::Downstream {
            tool: "bismark2bedGraph",
            message: e.to_string(),
        }
    })?;
    // UCSC post-pass re-reads the just-written `.bedGraph` (E7); `run()` orders
    // write_outputs → write_ucsc, and we preserve that order here.
    if b2bg_cfg.ucsc {
        bismark_bedgraph::ucsc::write_ucsc(&b2bg_cfg).map_err(|e| {
            BismarkExtractorError::Downstream {
                tool: "bismark2bedGraph",
                message: e.to_string(),
            }
        })?;
    }

    // ── Step 2: coverage2cytosine (in-process; if engaged) ──
    if config.cytosine_report {
        let coverage_filename = derive_coverage_filename(&bedgraph_filename);
        let cytosine_filename = derive_cytosine_filename(&bedgraph_filename, config.cx_context);
        let genome_folder = config
            .genome_folder
            .as_ref()
            .expect("CLI validation guarantees genome_folder is Some when cytosine_report is set");
        // NEW-1: the cov positional MUST be absolute — Rust c2c opens it
        // verbatim from CWD and ignores --parent_dir.
        let coverage_input_path = output_dir.join(&coverage_filename);
        let c2c_argv = build_coverage2cytosine_argv(
            config,
            &coverage_input_path,
            &cytosine_filename,
            output_dir,
            genome_folder,
        );
        let mut c2c_full: Vec<OsString> = vec!["coverage2cytosine".into()];
        c2c_full.extend(c2c_argv);
        let c2c_cli = bismark_coverage2cytosine::Cli::try_parse_from(&c2c_full).map_err(|e| {
            BismarkExtractorError::Downstream {
                tool: "coverage2cytosine",
                message: e.to_string(),
            }
        })?;
        let c2c_cfg = c2c_cli
            .validate()
            .map_err(|e| BismarkExtractorError::Downstream {
                tool: "coverage2cytosine",
                message: e.to_string(),
            })?;
        bismark_coverage2cytosine::run(&c2c_cfg).map_err(|e| {
            BismarkExtractorError::Downstream {
                tool: "coverage2cytosine",
                message: e.to_string(),
            }
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{OutputMode, PairedMode};

    // ─── Helper: build a minimal ResolvedConfig for argv-builder tests ──

    fn default_config() -> ResolvedConfig {
        ResolvedConfig {
            files: vec![PathBuf::from("input.bam")],
            paired_mode: PairedMode::SingleEnd,
            output_mode: OutputMode::Default,
            ignore_5p_r1: 0,
            ignore_3p_r1: 0,
            ignore_5p_r2: 0,
            ignore_3p_r2: 0,
            no_overlap: false,
            output_dir: PathBuf::from("/out"),
            no_header: false,
            gzip: false,
            emit_splitting_report: true,
            fasta_annotation: false,
            mbias_off: false,
            bedgraph: true,
            cytosine_report: false,
            cutoff: 1,
            remove_spaces: false,
            counts: true,
            zero_based: false,
            cx_context: false,
            split_by_chromosome: false,
            ucsc: false,
            buffer_size: None,
            gazillion: false,
            ample_memory: false,
            genome_folder: None,
            parallel: 1,
            quiet: false,
            verbose: false,
        }
    }

    // ─── Filename derivation (rev 1 C3 — trailing-dot quirk) ────────────

    #[test]
    fn derive_bedgraph_filename_foo_bam() {
        assert_eq!(derive_bedgraph_filename("foo.bam"), "foo.bedGraph");
    }

    #[test]
    fn derive_bedgraph_filename_foo_sam() {
        assert_eq!(derive_bedgraph_filename("foo.sam"), "foo.bedGraph");
    }

    #[test]
    fn derive_bedgraph_filename_foo_txt() {
        assert_eq!(derive_bedgraph_filename("foo.txt"), "foo.bedGraph");
    }

    #[test]
    fn derive_bedgraph_filename_foo_bam_gz_preserves_trailing_dot() {
        // rev 1 C3 critical guard. Perl s/gz$// strips "gz" not ".gz", so
        // foo.bam.gz becomes "foo.bam." → "foo.bam.bedGraph".
        assert_eq!(derive_bedgraph_filename("foo.bam.gz"), "foo.bam.bedGraph");
    }

    #[test]
    fn derive_bedgraph_filename_foo_txt_gz_preserves_trailing_dot() {
        assert_eq!(derive_bedgraph_filename("foo.txt.gz"), "foo.txt.bedGraph");
    }

    #[test]
    fn derive_bedgraph_filename_no_extension_has_no_leading_dot() {
        // rev 1 C3: input "foo" with no recognised extension → "foobedGraph".
        // The leading dot is NOT introduced because Perl's regex doesn't
        // strip anything (no match), and "s/$/bedGraph/" appends literally.
        assert_eq!(derive_bedgraph_filename("foo"), "foobedGraph");
    }

    #[test]
    fn derive_bedgraph_filename_real_bismark_pe_naming() {
        assert_eq!(
            derive_bedgraph_filename("sample.fastq_bismark_bt2_pe.deduplicated.bam"),
            "sample.fastq_bismark_bt2_pe.deduplicated.bedGraph"
        );
    }

    #[test]
    fn derive_bedgraph_filename_real_bismark_pe_gz_naming() {
        // Chained-extension case on real Bismark output names.
        assert_eq!(
            derive_bedgraph_filename("sample.fastq_bismark_bt2_pe.deduplicated.bam.gz"),
            "sample.fastq_bismark_bt2_pe.deduplicated.bam.bedGraph"
        );
    }

    #[test]
    fn derive_coverage_filename_basic() {
        assert_eq!(
            derive_coverage_filename("foo.bedGraph"),
            "foo.bismark.cov.gz"
        );
    }

    #[test]
    fn derive_coverage_filename_preserves_trailing_dot_for_chained_extensions() {
        // foo.bam.bedGraph (from foo.bam.gz input) → foo.bam.bismark.cov.gz.
        assert_eq!(
            derive_coverage_filename("foo.bam.bedGraph"),
            "foo.bam.bismark.cov.gz"
        );
    }

    #[test]
    fn derive_cytosine_filename_cpg_default() {
        assert_eq!(
            derive_cytosine_filename("foo.bedGraph", false),
            "foo.CpG_report.txt"
        );
    }

    #[test]
    fn derive_cytosine_filename_cx_context_when_flag_set() {
        assert_eq!(
            derive_cytosine_filename("foo.bedGraph", true),
            "foo.CX_report.txt"
        );
    }

    #[test]
    fn derive_cytosine_filename_preserves_trailing_dot_for_chained_extensions() {
        // The whole chain: foo.bam.gz → foo.bam.bedGraph → foo.bam.CpG_report.txt.
        assert_eq!(
            derive_cytosine_filename("foo.bam.bedGraph", false),
            "foo.bam.CpG_report.txt"
        );
    }

    // ─── bismark2bedGraph argv-builder ──────────────────────────────────

    #[test]
    fn build_bismark2bedgraph_argv_default_no_optional_flags() {
        let cfg = default_config();
        let kept: Vec<PathBuf> = Vec::new();
        let argv = build_bismark2bedgraph_argv(&cfg, &kept, "foo.bedGraph", Path::new("/out"));
        // Default config: !remove_spaces, !cx, !no_header, !gazillion,
        // !ample_memory (so buffer_size 2G), !ucsc, !zero_based.
        // Always: --buffer_size 2G, --cutoff 1, --output, --dir.
        assert_eq!(
            argv,
            vec![
                OsString::from("--buffer_size"),
                OsString::from("2G"),
                OsString::from("--cutoff"),
                OsString::from("1"),
                OsString::from("--output"),
                OsString::from("foo.bedGraph"),
                OsString::from("--dir"),
                OsString::from("/out"),
            ]
        );
    }

    #[test]
    fn build_bismark2bedgraph_argv_all_optional_flags_set() {
        let mut cfg = default_config();
        cfg.remove_spaces = true;
        cfg.cx_context = true;
        cfg.no_header = true;
        cfg.gazillion = true;
        // ample_memory mutex with buffer_size — set ample_memory, leave buffer_size None.
        cfg.ample_memory = true;
        cfg.ucsc = true;
        cfg.zero_based = true;
        cfg.cutoff = 5;
        let kept: Vec<PathBuf> = Vec::new();
        let argv = build_bismark2bedgraph_argv(&cfg, &kept, "foo.bedGraph", Path::new("/out"));
        assert_eq!(
            argv,
            vec![
                OsString::from("--remove_spaces"),
                OsString::from("--CX_context"),
                OsString::from("--no_header"),
                OsString::from("--gazillion"),
                OsString::from("--ample_memory"),
                OsString::from("--ucsc"),
                OsString::from("--zero_based"),
                OsString::from("--cutoff"),
                OsString::from("5"),
                OsString::from("--output"),
                OsString::from("foo.bedGraph"),
                OsString::from("--dir"),
                OsString::from("/out"),
            ]
        );
    }

    #[test]
    fn build_bismark2bedgraph_argv_uses_long_form_remove_spaces() {
        let mut cfg = default_config();
        cfg.remove_spaces = true;
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        // rev 1 §2.4.4: Perl pushes "--remove" (GetOptions prefix abbrev);
        // Rust pushes the long form "--remove_spaces" explicitly.
        assert!(argv.contains(&OsString::from("--remove_spaces")));
        assert!(!argv.contains(&OsString::from("--remove")));
    }

    #[test]
    fn build_bismark2bedgraph_argv_uses_long_form_zero_based() {
        let mut cfg = default_config();
        cfg.zero_based = true;
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        assert!(argv.contains(&OsString::from("--zero_based")));
        assert!(!argv.contains(&OsString::from("--zero")));
    }

    #[test]
    fn build_bismark2bedgraph_argv_passes_buffer_size_2g_default_when_neither_flag_set() {
        // rev 1 I5: when both buffer_size and ample_memory unset, ALWAYS push
        // --buffer_size 2G (Perl :347-352 in the else branch, with $sort_size
        // defaulting to "2G" at :1305).
        let cfg = default_config();
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        let idx = argv
            .iter()
            .position(|a| a == &OsString::from("--buffer_size"))
            .expect("--buffer_size should be present");
        assert_eq!(argv[idx + 1], OsString::from("2G"));
    }

    #[test]
    fn build_bismark2bedgraph_argv_passes_explicit_buffer_size_when_set() {
        let mut cfg = default_config();
        cfg.buffer_size = Some("4G".to_string());
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        let idx = argv
            .iter()
            .position(|a| a == &OsString::from("--buffer_size"))
            .unwrap();
        assert_eq!(argv[idx + 1], OsString::from("4G"));
    }

    #[test]
    fn build_bismark2bedgraph_argv_passes_ample_memory_instead_of_buffer_size() {
        let mut cfg = default_config();
        cfg.ample_memory = true;
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        assert!(argv.contains(&OsString::from("--ample_memory")));
        assert!(!argv.contains(&OsString::from("--buffer_size")));
    }

    #[test]
    fn build_bismark2bedgraph_argv_omits_counts_flag() {
        // Perl :362-364 comments out the --counts push. Rust mirrors:
        // --counts is never in the argv regardless of config.counts.
        let mut cfg = default_config();
        cfg.counts = true; // forced; Perl ON by default
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        assert!(!argv.contains(&OsString::from("--counts")));
    }

    #[test]
    fn build_bismark2bedgraph_argv_appends_kept_files_as_positional_tail() {
        let cfg = default_config();
        let kept = vec![
            PathBuf::from("/out/CpG_OT_input.txt"),
            PathBuf::from("/out/CpG_OB_input.txt"),
        ];
        let argv = build_bismark2bedgraph_argv(&cfg, &kept, "foo.bedGraph", Path::new("/out"));
        // The two paths must be the final argv entries, in input order
        // (the caller sorts; this fn preserves).
        assert_eq!(
            argv[argv.len() - 2],
            OsString::from("/out/CpG_OT_input.txt")
        );
        assert_eq!(
            argv[argv.len() - 1],
            OsString::from("/out/CpG_OB_input.txt")
        );
    }

    #[test]
    fn build_bismark2bedgraph_argv_does_not_pass_gzip() {
        // bismark2bedGraph has no --gzip flag (`bismark-bedgraph/src/cli.rs`).
        let mut cfg = default_config();
        cfg.gzip = true;
        let argv = build_bismark2bedgraph_argv(&cfg, &[], "foo.bedGraph", Path::new("/out"));
        assert!(!argv.contains(&OsString::from("--gzip")));
    }

    // ─── coverage2cytosine argv-builder ────────────────────────────────

    #[test]
    fn build_coverage2cytosine_argv_default_cpg_only() {
        let cfg = default_config();
        let argv = build_coverage2cytosine_argv(
            &cfg,
            Path::new("/out/foo.bismark.cov.gz"),
            "foo.CpG_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        // Order: header (--output --dir --genome_folder --parent_dir) +
        // conditional flags (none) + absolute cov positional.
        // Phase 2 T1: --genome_folder (not --genome). NEW-1: absolute cov path.
        assert_eq!(
            argv,
            vec![
                OsString::from("--output"),
                OsString::from("foo.CpG_report.txt"),
                OsString::from("--dir"),
                OsString::from("/out"),
                OsString::from("--genome_folder"),
                OsString::from("/genome"),
                OsString::from("--parent_dir"),
                OsString::from("/out"),
                OsString::from("/out/foo.bismark.cov.gz"),
            ]
        );
    }

    #[test]
    fn build_coverage2cytosine_argv_uses_genome_folder_not_genome() {
        // Phase 2 T1 (C-1): the Rust c2c CLI rejects --genome; we must emit
        // --genome_folder.
        let cfg = default_config();
        let argv = build_coverage2cytosine_argv(
            &cfg,
            Path::new("/out/foo.bismark.cov.gz"),
            "foo.CpG_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        assert!(argv.contains(&OsString::from("--genome_folder")));
        assert!(!argv.contains(&OsString::from("--genome")));
    }

    #[test]
    fn build_coverage2cytosine_argv_with_cx_context_flag() {
        let mut cfg = default_config();
        cfg.cx_context = true;
        let argv = build_coverage2cytosine_argv(
            &cfg,
            Path::new("/out/foo.bismark.cov.gz"),
            "foo.CX_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        assert!(argv.contains(&OsString::from("--CX_context")));
    }

    #[test]
    fn build_coverage2cytosine_argv_with_split_by_chromosome() {
        let mut cfg = default_config();
        cfg.split_by_chromosome = true;
        let argv = build_coverage2cytosine_argv(
            &cfg,
            Path::new("/out/foo.bismark.cov.gz"),
            "foo.CpG_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        assert!(argv.contains(&OsString::from("--split_by_chromosome")));
    }

    #[test]
    fn build_coverage2cytosine_argv_with_gzip() {
        let mut cfg = default_config();
        cfg.gzip = true;
        let argv = build_coverage2cytosine_argv(
            &cfg,
            Path::new("/out/foo.bismark.cov.gz"),
            "foo.CpG_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        assert!(argv.contains(&OsString::from("--gzip")));
    }

    #[test]
    fn build_coverage2cytosine_argv_positional_is_absolute_coverage_file() {
        // NEW-1: the cov positional is the ABSOLUTE path, not a bare basename.
        let cfg = default_config();
        let argv = build_coverage2cytosine_argv(
            &cfg,
            Path::new("/out/foo.bismark.cov.gz"),
            "foo.CpG_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        assert_eq!(
            argv.last(),
            Some(&OsString::from("/out/foo.bismark.cov.gz"))
        );
    }

    #[test]
    fn build_coverage2cytosine_argv_passes_parent_dir_equal_to_dir() {
        // rev 1 I13: --parent_dir takes the SAME value as --dir per Perl :404
        // (inert in the Rust port, but kept for argv-shape parity).
        let cfg = default_config();
        let argv = build_coverage2cytosine_argv(
            &cfg,
            Path::new("/some/output/dir/foo.bismark.cov.gz"),
            "foo.CpG_report.txt",
            Path::new("/some/output/dir"),
            Path::new("/genome"),
        );
        let dir_idx = argv
            .iter()
            .position(|a| a == &OsString::from("--dir"))
            .unwrap();
        let parent_idx = argv
            .iter()
            .position(|a| a == &OsString::from("--parent_dir"))
            .unwrap();
        assert_eq!(argv[dir_idx + 1], argv[parent_idx + 1]);
        assert_eq!(argv[dir_idx + 1], OsString::from("/some/output/dir"));
    }

    // ─── Phase 2 T1: argv parses cleanly via each Rust crate's Cli ──────

    #[test]
    fn argv_parses_into_each_rust_cli() {
        // Representative config exercising the flags the builders emit.
        let mut cfg = default_config();
        cfg.cytosine_report = true;
        cfg.cx_context = true;
        cfg.zero_based = true;
        cfg.split_by_chromosome = true;
        cfg.gzip = true;
        cfg.remove_spaces = true;
        cfg.no_header = true;
        cfg.ucsc = true;
        cfg.cutoff = 3;
        cfg.genome_folder = Some(PathBuf::from("/genome"));

        // bedGraph side: needs at least one positional file to validate.
        let kept = vec![PathBuf::from("/out/CpG_OT_input.txt")];
        let b2bg_argv = build_bismark2bedgraph_argv(&cfg, &kept, "foo.bedGraph", Path::new("/out"));
        let mut b2bg_full: Vec<OsString> = vec!["bismark2bedGraph".into()];
        b2bg_full.extend(b2bg_argv);
        bismark_bedgraph::Cli::try_parse_from(&b2bg_full)
            .expect("bedGraph argv must parse into bismark_bedgraph::Cli");

        // c2c side.
        let c2c_argv = build_coverage2cytosine_argv(
            &cfg,
            Path::new("/out/foo.bismark.cov.gz"),
            "foo.CX_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        let mut c2c_full: Vec<OsString> = vec!["coverage2cytosine".into()];
        c2c_full.extend(c2c_argv);
        bismark_coverage2cytosine::Cli::try_parse_from(&c2c_full)
            .expect("c2c argv must parse into bismark_coverage2cytosine::Cli");
    }

    #[test]
    fn argv_parses_into_each_rust_cli_minimal() {
        // Minimal (all optional flags off) — bedGraph default + c2c CpG-only.
        let mut cfg = default_config();
        cfg.cytosine_report = true;
        cfg.genome_folder = Some(PathBuf::from("/genome"));

        let kept = vec![PathBuf::from("/out/CpG_OT_input.txt")];
        let b2bg_argv = build_bismark2bedgraph_argv(&cfg, &kept, "foo.bedGraph", Path::new("/out"));
        let mut b2bg_full: Vec<OsString> = vec!["bismark2bedGraph".into()];
        b2bg_full.extend(b2bg_argv);
        bismark_bedgraph::Cli::try_parse_from(&b2bg_full).expect("minimal bedGraph argv parses");

        let c2c_argv = build_coverage2cytosine_argv(
            &cfg,
            Path::new("/out/foo.bismark.cov.gz"),
            "foo.CpG_report.txt",
            Path::new("/out"),
            Path::new("/genome"),
        );
        let mut c2c_full: Vec<OsString> = vec!["coverage2cytosine".into()];
        c2c_full.extend(c2c_argv);
        bismark_coverage2cytosine::Cli::try_parse_from(&c2c_full).expect("minimal c2c argv parses");
    }
}
