//! Command-line interface for `coverage2cytosine_rs`.
//!
//! [`Cli`] is the clap-derived parser; [`Cli::validate`] resolves it into a
//! [`ResolvedConfig`], reproducing every Perl `process_commandline`
//! (`coverage2cytosine:1990-2197`) validation rule and **rejecting** the v1.x
//! flags (`--gc`/`--nome-seq`/`--drach`/`--ffs`) rather than silently
//! accepting them.
//!
//! Two byte-identity-relevant subtleties (folded from Phase-A dual review):
//! - **Output-stem strip is context-conditional**: strip `.CX_report.txt` iff
//!   `--CX`, else `.CpG_report.txt` — never both (Perl `:107-112`).
//! - **`output_dir` defaults to `""`** (an empty path *prefix*), while
//!   **`parent_dir` defaults to `getcwd()`** (Perl `:2070-2071`, `:2108-2110`).

use std::path::PathBuf;

use clap::Parser;

use crate::error::BismarkC2cError;

/// Parsed command-line arguments. Use [`Cli::validate`] to convert to a
/// [`ResolvedConfig`].
#[derive(Parser, Debug)]
#[command(
    name = "coverage2cytosine_rs",
    about = "Generate a genome-wide cytosine methylation report from a Bismark coverage file",
    long_about = None,
    disable_version_flag = true
)]
pub struct Cli {
    /// Bismark coverage file (`*.bismark.cov[.gz]`). 1-based, tab-separated.
    #[arg(value_name = "COV_FILE")]
    pub cov_infile: Option<PathBuf>,

    /// Output basename (mandatory).
    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,

    /// Output directory (default: current directory).
    #[arg(long = "dir")]
    pub dir: Option<PathBuf>,

    /// Genome FASTA directory (mandatory; no hardcoded default).
    #[arg(short = 'g', long = "genome_folder")]
    pub genome_folder: Option<PathBuf>,

    /// Base directory to resolve relative paths against (default: cwd).
    #[arg(long = "parent_dir")]
    pub parent_dir: Option<PathBuf>,

    /// Emit 0-based coordinates instead of 1-based.
    #[arg(long = "zero_based")]
    pub zero_based: bool,

    /// Report every cytosine context (not just CpG).
    #[arg(long = "CX_context", visible_alias = "CX")]
    pub cx_context: bool,

    /// One output file per chromosome.
    #[arg(long = "split_by_chromosome")]
    pub split_by_chromosome: bool,

    /// Pool top/bottom CpG strands into a single dinucleotide cov file.
    #[arg(long = "merge_CpGs")]
    pub merge_cpgs: bool,

    /// (with --merge_CpGs) route discordant CpGs (Δ% > N) to a separate file.
    #[arg(long = "discordance_filter", value_name = "INT")]
    pub discordance: Option<u8>,

    /// Minimum coverage to report a position (default: 0 = report all).
    #[arg(
        long = "coverage_threshold",
        visible_alias = "threshold",
        value_name = "INT"
    )]
    pub threshold: Option<u32>,

    /// gzip-compress the report + cov outputs.
    #[arg(long = "gzip")]
    pub gzip: bool,

    /// Print version information and exit.
    #[arg(short = 'V', long = "version")]
    pub version: bool,

    // ── v1.x flags: declared so they parse, but rejected at validate() ──
    /// (v1.x, rejected) GpC-context report.
    #[arg(long = "gc", visible_aliases = ["GC", "GC_context", "gc_context"])]
    pub gc: bool,
    /// (v1.x, rejected) NOMe-Seq filtering.
    #[arg(long = "nome-seq")]
    pub nome_seq: bool,
    /// (v1.x, rejected) DRACH m6A filtering.
    #[arg(long = "drach", visible_alias = "m6A")]
    pub drach: bool,
    /// (v1.x, rejected) tetra/penta/hexamer context columns.
    #[arg(long = "ffs")]
    pub ffs: bool,
}

/// Validated, resolved configuration consumed by Phases B–E.
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// The positional coverage infile.
    pub cov_infile: PathBuf,
    /// Output basename with the context-appropriate report suffix stripped.
    pub output_stem: String,
    /// Output directory as a path *prefix* (empty string = cwd-relative;
    /// matches Perl's `"${output_dir}${file}"` concatenation).
    pub output_dir: String,
    /// Base directory for relative-path resolution (defaults to `getcwd()`).
    pub parent_dir: PathBuf,
    /// Genome FASTA directory.
    pub genome_folder: PathBuf,
    /// `true` when reporting CpG context only (i.e. `!cx_context`).
    pub cpg_only: bool,
    /// `--CX` requested.
    pub cx_context: bool,
    /// `--zero_based` requested.
    pub zero_based: bool,
    /// `--split_by_chromosome` requested.
    pub split_by_chromosome: bool,
    /// Minimum coverage to report (0 = report all).
    pub threshold: u32,
    /// `--gzip` requested.
    pub gzip: bool,
    /// `--merge_CpGs` requested.
    pub merge_cpgs: bool,
    /// `--discordance_filter` value, if any.
    pub discordance: Option<u8>,
}

impl Cli {
    /// Reject unsupported / conflicting flag combinations and resolve defaults.
    ///
    /// Rejections (in order, mirroring Perl `process_commandline`):
    /// v1.x flags → missing infile → missing `-o` → missing `-g` →
    /// `--merge_CpGs` mutexes (`--CX`, `--split_by_chromosome`,
    /// `--coverage_threshold`) → `--discordance_filter` without merge →
    /// discordance range `1..=100` → `--coverage_threshold 0`.
    pub fn validate(self) -> Result<ResolvedConfig, BismarkC2cError> {
        // v1.x flags rejected outright (not silently ignored).
        if self.gc {
            return Err(BismarkC2cError::UnsupportedFlag { flag: "--gc" });
        }
        if self.nome_seq {
            return Err(BismarkC2cError::UnsupportedFlag { flag: "--nome-seq" });
        }
        if self.drach {
            return Err(BismarkC2cError::UnsupportedFlag { flag: "--drach" });
        }
        if self.ffs {
            return Err(BismarkC2cError::UnsupportedFlag { flag: "--ffs" });
        }

        let cov_infile = self.cov_infile.ok_or(BismarkC2cError::MissingCovInput)?;
        let output = self.output.ok_or(BismarkC2cError::MissingOutput)?;
        let genome_folder = self
            .genome_folder
            .ok_or(BismarkC2cError::MissingGenomeFolder)?;

        if self.merge_cpgs && self.cx_context {
            return Err(BismarkC2cError::MergeCpgsWithCx);
        }
        if self.merge_cpgs && self.split_by_chromosome {
            return Err(BismarkC2cError::MergeCpgsWithSplit);
        }
        if self.merge_cpgs && self.threshold.is_some() {
            return Err(BismarkC2cError::MergeCpgsWithThreshold);
        }
        if self.discordance.is_some() && !self.merge_cpgs {
            return Err(BismarkC2cError::DiscordanceWithoutMerge);
        }
        if let Some(v) = self.discordance
            && !(1..=100).contains(&v)
        {
            return Err(BismarkC2cError::DiscordanceOutOfRange { value: v });
        }
        if self.threshold == Some(0) {
            return Err(BismarkC2cError::ThresholdNotPositive);
        }

        // ── Resolution ──
        let cpg_only = !self.cx_context;
        let threshold = self.threshold.unwrap_or(0);

        // C1: context-conditional stem strip — strip EXACTLY ONE suffix gated
        // on --CX (Perl handle_filehandles:107-112). Never both.
        let suffix = if self.cx_context {
            ".CX_report.txt"
        } else {
            ".CpG_report.txt"
        };
        let output_stem = output
            .strip_suffix(suffix)
            .unwrap_or(output.as_str())
            .to_string();

        // C2: output_dir defaults to "" (a path prefix); parent_dir to getcwd().
        let output_dir = resolve_output_dir(self.dir)?;
        let parent_dir = match self.parent_dir {
            Some(p) => p,
            None => std::env::current_dir()?,
        };

        Ok(ResolvedConfig {
            cov_infile,
            output_stem,
            output_dir,
            parent_dir,
            genome_folder,
            cpg_only,
            cx_context: self.cx_context,
            zero_based: self.zero_based,
            split_by_chromosome: self.split_by_chromosome,
            threshold,
            gzip: self.gzip,
            merge_cpgs: self.merge_cpgs,
            discordance: self.discordance,
        })
    }
}

/// Resolve `--dir` to an absolute path *prefix* with a trailing `/`. `None`
/// resolves to the empty string (cwd-relative), matching Perl's default
/// `$output_dir = ''` (`:2108-2110`). The directory need not exist yet
/// (`std::path::absolute` does not touch the filesystem); creation is a
/// Phase-B/C concern (no output is written in Phase A).
fn resolve_output_dir(dir: Option<PathBuf>) -> Result<String, BismarkC2cError> {
    match dir {
        None => Ok(String::new()),
        Some(d) => {
            let abs = std::path::absolute(&d)?;
            let mut s = abs.to_string_lossy().into_owned();
            if !s.ends_with('/') {
                s.push('/');
            }
            Ok(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        let mut full = vec!["coverage2cytosine_rs"];
        full.extend(args.iter().copied());
        Cli::try_parse_from(full)
    }

    fn cli(args: &[&str]) -> Cli {
        parse(args).unwrap()
    }

    // ── Task 3: parsing ──

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn cx_long_and_alias_both_parse() {
        assert!(cli(&["-o", "x", "-g", "gdir", "--CX_context", "in.cov"]).cx_context);
        assert!(cli(&["-o", "x", "-g", "gdir", "--CX", "in.cov"]).cx_context);
    }

    #[test]
    fn dash_cx_is_not_a_valid_short() {
        // `-CX` must NOT be accepted (clap would otherwise need bundled shorts
        // -C -X, which don't exist → parse error).
        assert!(parse(&["-o", "x", "-g", "gdir", "-CX", "in.cov"]).is_err());
    }

    #[test]
    fn parses_positional_cov_infile() {
        let cli = cli(&["-o", "x", "-g", "gdir", "sample.bismark.cov.gz"]);
        assert_eq!(
            cli.cov_infile.as_deref(),
            Some(std::path::Path::new("sample.bismark.cov.gz"))
        );
    }

    // ── Task 4: validation rejections ──

    #[test]
    fn rejects_v1x_flags() {
        for (flag, frag) in [
            ("--gc", "gc"),
            ("--nome-seq", "nome-seq"),
            ("--drach", "drach"),
            ("--ffs", "ffs"),
        ] {
            let e = cli(&["-o", "x", "-g", "g", flag, "in.cov"])
                .validate()
                .unwrap_err();
            assert!(
                matches!(e, BismarkC2cError::UnsupportedFlag { flag } if flag.contains(frag)),
                "flag {flag} did not reject as UnsupportedFlag containing {frag}: {e:?}"
            );
        }
    }

    #[test]
    fn rejects_missing_output() {
        let e = cli(&["-g", "g", "in.cov"]).validate().unwrap_err();
        assert!(matches!(e, BismarkC2cError::MissingOutput));
    }

    #[test]
    fn rejects_missing_genome() {
        let e = cli(&["-o", "x", "in.cov"]).validate().unwrap_err();
        assert!(matches!(e, BismarkC2cError::MissingGenomeFolder));
    }

    #[test]
    fn rejects_missing_cov_infile() {
        let e = cli(&["-o", "x", "-g", "g"]).validate().unwrap_err();
        assert!(matches!(e, BismarkC2cError::MissingCovInput));
    }

    #[test]
    fn rejects_merge_with_cx() {
        let e = cli(&["-o", "x", "-g", "g", "--merge_CpGs", "--CX", "in.cov"])
            .validate()
            .unwrap_err();
        assert!(matches!(e, BismarkC2cError::MergeCpgsWithCx));
    }

    #[test]
    fn rejects_merge_with_split() {
        let e = cli(&[
            "-o",
            "x",
            "-g",
            "g",
            "--merge_CpGs",
            "--split_by_chromosome",
            "in.cov",
        ])
        .validate()
        .unwrap_err();
        assert!(matches!(e, BismarkC2cError::MergeCpgsWithSplit));
    }

    #[test]
    fn rejects_merge_with_threshold() {
        let e = cli(&[
            "-o",
            "x",
            "-g",
            "g",
            "--merge_CpGs",
            "--coverage_threshold",
            "5",
            "in.cov",
        ])
        .validate()
        .unwrap_err();
        assert!(matches!(e, BismarkC2cError::MergeCpgsWithThreshold));
    }

    #[test]
    fn rejects_discordance_without_merge() {
        let e = cli(&["-o", "x", "-g", "g", "--discordance_filter", "20", "in.cov"])
            .validate()
            .unwrap_err();
        assert!(matches!(e, BismarkC2cError::DiscordanceWithoutMerge));
    }

    #[test]
    fn rejects_discordance_out_of_range() {
        for v in ["0", "101"] {
            let e = cli(&[
                "-o",
                "x",
                "-g",
                "g",
                "--merge_CpGs",
                "--discordance_filter",
                v,
                "in.cov",
            ])
            .validate()
            .unwrap_err();
            assert!(
                matches!(e, BismarkC2cError::DiscordanceOutOfRange { .. }),
                "discordance {v} should be out of range"
            );
        }
    }

    #[test]
    fn accepts_discordance_in_range() {
        let c = cli(&[
            "-o",
            "x",
            "-g",
            "g",
            "--merge_CpGs",
            "--discordance_filter",
            "20",
            "in.cov",
        ])
        .validate()
        .unwrap();
        assert_eq!(c.discordance, Some(20));
    }

    #[test]
    fn rejects_threshold_zero() {
        let e = cli(&["-o", "x", "-g", "g", "--coverage_threshold", "0", "in.cov"])
            .validate()
            .unwrap_err();
        assert!(matches!(e, BismarkC2cError::ThresholdNotPositive));
    }

    // ── Task 5: resolution (cpg_only, C1 stem strip, C2 dir defaults) ──

    #[test]
    fn cpg_only_coupling() {
        assert!(
            cli(&["-o", "x", "-g", "g", "in.cov"])
                .validate()
                .unwrap()
                .cpg_only
        );
        assert!(
            !cli(&["-o", "x", "-g", "g", "--CX", "in.cov"])
                .validate()
                .unwrap()
                .cpg_only
        );
    }

    #[test]
    fn output_stem_strip_is_context_conditional() {
        let stem = |a: &[&str]| cli(a).validate().unwrap().output_stem;
        // default (CpG) mode strips .CpG_report.txt
        assert_eq!(
            stem(&["-o", "foo.CpG_report.txt", "-g", "g", "in.cov"]),
            "foo"
        );
        // default mode + .CX_report.txt: NOT stripped
        assert_eq!(
            stem(&["-o", "foo.CX_report.txt", "-g", "g", "in.cov"]),
            "foo.CX_report.txt"
        );
        // --CX strips .CX_report.txt
        assert_eq!(
            stem(&["-o", "foo.CX_report.txt", "-g", "g", "--CX", "in.cov"]),
            "foo"
        );
        // --CX + .CpG_report.txt: NOT stripped
        assert_eq!(
            stem(&["-o", "foo.CpG_report.txt", "-g", "g", "--CX", "in.cov"]),
            "foo.CpG_report.txt"
        );
        // plain stem
        assert_eq!(stem(&["-o", "foo", "-g", "g", "in.cov"]), "foo");
    }

    #[test]
    fn dir_defaults_are_split() {
        let c = cli(&["-o", "x", "-g", "g", "in.cov"]).validate().unwrap();
        assert_eq!(c.output_dir, ""); // empty path prefix
        assert_eq!(c.parent_dir, std::env::current_dir().unwrap()); // getcwd()
    }

    #[test]
    fn given_dir_is_absolute_with_trailing_slash() {
        let c = cli(&["-o", "x", "-g", "g", "--dir", "some/out", "in.cov"])
            .validate()
            .unwrap();
        assert!(c.output_dir.ends_with('/'));
        assert!(std::path::Path::new(&c.output_dir).is_absolute());
    }

    #[test]
    fn threshold_none_defaults_zero() {
        assert_eq!(
            cli(&["-o", "x", "-g", "g", "in.cov"])
                .validate()
                .unwrap()
                .threshold,
            0
        );
    }
}
