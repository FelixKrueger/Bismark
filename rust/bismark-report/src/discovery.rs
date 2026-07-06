//! Alignment-report discovery + companion-report resolution (SPEC §2.2).
//!
//! Companion resolution order mirrors Perl: **dedup → nucleotide → splitting →
//! mbias** (`bismark2report:1141/1170/1201/1228`) — this *companion* order only
//! affects which `die` fires first when several companions are ambiguous; the
//! resolved files, and thus the HTML bytes, are identical regardless of it.
//! Nucleotide uses `defined` semantics for its explicit flag; the other three
//! use truthiness (matters only for an explicit empty-string argument). The
//! explicit flags apply to the **first** alignment report only (Perl's line-1256
//! reset) — so the *alignment-report* glob order IS byte-relevant when an
//! explicit companion is combined with auto-detected multiple reports, which is
//! why the glob is sorted with Perl's `File::Glob` collation (`glob_order_key`).

use std::path::{Path, PathBuf};

use crate::cli::Cli;
use crate::error::ReportError;

/// One alignment report + its resolved (optional) companions.
#[derive(Debug)]
pub struct Job {
    pub alignment: PathBuf,
    pub dedup: Option<PathBuf>,
    pub splitting: Option<PathBuf>,
    pub mbias: Option<PathBuf>,
    pub nuc: Option<PathBuf>,
}

/// Explicit `--alignment_report`, else glob the current directory for
/// `*E_report.txt` (lexically sorted). Empty → error (Perl prints help + exits).
pub fn find_alignment_reports(cli: &Cli) -> Result<Vec<PathBuf>, ReportError> {
    if let Some(a) = &cli.alignment_report {
        return Ok(vec![PathBuf::from(a)]);
    }
    let matches = glob_prefix_suffix("", "E_report.txt")?;
    if matches.is_empty() {
        return Err(ReportError::Validation(
            "Found no potential alignment reports in the current directory. Please specify a \
             single Bismark alignment report file using the option '--alignment_report FILE'"
                .into(),
        ));
    }
    Ok(matches)
}

/// Build one [`Job`] per alignment report, resolving companions.
pub fn resolve_companions(cli: &Cli, alignments: &[PathBuf]) -> Result<Vec<Job>, ReportError> {
    let mut jobs = Vec::with_capacity(alignments.len());
    for (i, aln) in alignments.iter().enumerate() {
        let basename = basename_of(aln);
        let first = i == 0; // explicit flags apply to the first report only
        // Resolution order is load-bearing for die-precedence (see module docs).
        let dedup = resolve_one(
            first,
            &cli.dedup_report,
            false,
            &basename,
            "deduplication_report.txt",
            "deduplication",
            "dedup_report",
        )?;
        let nuc = resolve_one(
            first,
            &cli.nucleotide_report,
            true,
            &basename,
            "nucleotide_stats.txt",
            "nucleotide coverage",
            "nucleotide_report",
        )?;
        let splitting = resolve_one(
            first,
            &cli.splitting_report,
            false,
            &basename,
            "splitting_report.txt",
            "methylation extractor splitting",
            "splitting_report",
        )?;
        let mbias = resolve_one(
            first,
            &cli.mbias_report,
            false,
            &basename,
            "M-bias.txt",
            "M-bias",
            "mbias_report",
        )?;
        jobs.push(Job {
            alignment: aln.clone(),
            dedup,
            splitting,
            mbias,
            nuc,
        });
    }
    Ok(jobs)
}

/// Strip `_PE_report.txt` / `_SE_report.txt` (Perl `^(.+)_(P|S)E_report.txt$`).
/// No match → empty basename (Perl undef → glob `*<suffix>`).
fn basename_of(aln: &Path) -> String {
    let s = aln.to_string_lossy();
    for suf in ["_PE_report.txt", "_SE_report.txt"] {
        if let Some(b) = s.strip_suffix(suf) {
            return b.to_string();
        }
    }
    String::new()
}

#[allow(clippy::too_many_arguments)]
fn resolve_one(
    first: bool,
    flag: &Option<String>,
    defined_semantics: bool,
    basename: &str,
    suffix: &str,
    kind: &str,
    flagname: &str,
) -> Result<Option<PathBuf>, ReportError> {
    let use_explicit = first
        && match (defined_semantics, flag) {
            (true, Some(_)) => true,            // nucleotide: Perl `defined`
            (false, Some(s)) => perl_truthy(s), // others: Perl truthiness ("" and "0" are false)
            (_, None) => false,
        };
    if use_explicit {
        let v = flag.as_ref().unwrap();
        if v.eq_ignore_ascii_case("none") {
            return Ok(None); // user opted out of this companion
        }
        return Ok(Some(PathBuf::from(v)));
    }
    let matches = glob_prefix_suffix(basename, suffix)?;
    if matches.len() > 1 {
        return Err(ReportError::Validation(format!(
            "Found {} potential {} reports with the same basename ({}) in the current directory. \
             Please specify a single report using the option '--{} FILE' or otherwise provide \
             filenames that are easier to figure out...",
            matches.len(),
            kind,
            basename,
            flagname
        )));
    }
    Ok(matches.into_iter().next())
}

/// Reproduce Perl's `<$basename*$suffix>` glob: read the directory implied by
/// `basename` and return entries whose file name starts with `basename`'s file
/// part and ends with `suffix`, lexically sorted. A bare basename globs the
/// current directory and returns bare names (matching Perl's output).
fn glob_prefix_suffix(basename: &str, suffix: &str) -> Result<Vec<PathBuf>, ReportError> {
    let bpath = Path::new(basename);
    let has_dir = bpath.parent().is_some_and(|d| !d.as_os_str().is_empty());
    let dir = if has_dir {
        bpath.parent().unwrap().to_path_buf()
    } else {
        PathBuf::from(".")
    };
    let prefix = bpath
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();

    let read = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => return Ok(Vec::new()), // Perl glob is lenient: no dir → no match
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for entry in read {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) && name.ends_with(suffix) {
            out.push(if has_dir {
                dir.join(&name)
            } else {
                PathBuf::from(&name)
            });
        }
    }
    // Match Perl `File::Glob` collation: case-insensitive (ASCII fold) primary,
    // raw bytes as tiebreak (verified vs Perl: `a2_, a_, B_, C_`). Byte-neutral
    // for independent reports, but load-bearing via the line-1256 first-report
    // reset — an explicit companion attaches to the first-sorted report.
    out.sort_by(|x, y| {
        glob_order_key(x)
            .cmp(&glob_order_key(y))
            .then_with(|| x.cmp(y))
    });
    Ok(out)
}

/// Perl `File::Glob` ordering key: ASCII-lowercased path bytes.
fn glob_order_key(p: &Path) -> Vec<u8> {
    p.as_os_str().as_encoded_bytes().to_ascii_lowercase()
}

/// Perl string truthiness (`if ($x)`): false for the empty string AND the
/// single-character string `"0"` (undef is handled by the `Option`). Used for
/// the `-o`/companion explicit-flag checks where Perl uses truthiness rather
/// than `defined`.
pub(crate) fn perl_truthy(s: &str) -> bool {
    !(s.is_empty() || s == "0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    #[test]
    fn basename_strips_pe_and_se_suffixes() {
        assert_eq!(basename_of(Path::new("sampleA_PE_report.txt")), "sampleA");
        assert_eq!(
            basename_of(Path::new("dir/sampleB_SE_report.txt")),
            "dir/sampleB"
        );
        assert_eq!(basename_of(Path::new("weird.txt")), ""); // no match → empty
    }

    #[test]
    fn explicit_companion_applies_to_first_report_only() {
        // Perl's line-1256 reset: an explicit `--dedup_report` applies to report
        // #0; report #1 falls back to auto-detect (no match in cwd → None).
        let cli = Cli::parse_from(["bismark2report", "--dedup_report", "explicit_companion.txt"]);
        let alns = vec![
            PathBuf::from("first_PE_report.txt"),
            PathBuf::from("second_PE_report.txt"),
        ];
        let jobs = resolve_companions(&cli, &alns).unwrap();
        assert_eq!(
            jobs[0].dedup.as_deref(),
            Some(Path::new("explicit_companion.txt"))
        );
        assert_eq!(jobs[1].dedup, None);
    }

    #[test]
    fn none_skips_a_companion() {
        let cli = Cli::parse_from(["bismark2report", "--mbias_report", "none"]);
        let jobs = resolve_companions(&cli, &[PathBuf::from("x_PE_report.txt")]).unwrap();
        assert_eq!(jobs[0].mbias, None);
    }

    #[test]
    fn glob_order_matches_perl_caseinsensitive_collation() {
        // Verified vs live Perl `<*E_report.txt>`: a2_, a_, B_, C_ (NOT byte order
        // B_, C_, a2_, a_). `a2_` precedes `a_` because '2'(0x32) < '_'(0x5F).
        let mut v: Vec<PathBuf> = [
            "B_PE_report.txt",
            "a_PE_report.txt",
            "a2_PE_report.txt",
            "C_PE_report.txt",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        v.sort_by(|x, y| {
            glob_order_key(x)
                .cmp(&glob_order_key(y))
                .then_with(|| x.cmp(y))
        });
        let got: Vec<String> = v.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        assert_eq!(
            got,
            [
                "a2_PE_report.txt",
                "a_PE_report.txt",
                "B_PE_report.txt",
                "C_PE_report.txt"
            ]
        );
    }

    #[test]
    fn perl_truthy_matches_perl_falsy_values() {
        // Perl: only "" and the exact string "0" are false.
        assert!(!perl_truthy(""));
        assert!(!perl_truthy("0"));
        assert!(perl_truthy("0.0"));
        assert!(perl_truthy("00"));
        assert!(perl_truthy("none"));
        assert!(perl_truthy("report.txt"));
    }
}
