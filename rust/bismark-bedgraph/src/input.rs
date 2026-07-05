//! Input-file selection and methylation-call line parsing.
//!
//! Mirrors Perl `bismark2bedGraph`:
//! - File selection (`:73-112`): default keeps only basenames matching
//!   `^CpG`; `--CX` keeps all. **Argv order is preserved** — never sorted or
//!   glob-reordered, because chromosome ownership depends on it (SPEC §2.1B).
//! - Header skip (`:182`/`:244`): the first line of each file is dropped
//!   unconditionally unless `--no_header`.
//! - `^Bismark` skip: the operative loop uses the **no-space** `/^Bismark/`
//!   (`:454`), so we skip any line whose start is `Bismark` (SPEC §3, I1).
//! - Validation (`:558-588`): missing field → fatal (Perl `croak`);
//!   present-but-inconsistent → warn + skip (SPEC §2.2).

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;

use crate::aggregate::Aggregator;
use crate::error::BismarkBedgraphError;
use crate::validate::validate_call;

/// The basename of a path (everything after the last `/`), matching Perl's
/// `s/.*\///`. Used both for the `^CpG` selection test and as the temp-file
/// ownership prefix in the aggregator's ordering key.
#[must_use]
pub fn basename(path: &Path) -> String {
    let s = path.to_string_lossy();
    match s.rfind('/') {
        Some(i) => s[i + 1..].to_string(),
        None => s.into_owned(),
    }
}

/// Select the input files to process, preserving argv order.
///
/// Default (CpG-only) mode keeps files whose basename starts with `CpG`;
/// `--CX` keeps all. Default mode with zero surviving files →
/// [`BismarkBedgraphError::NoCpgFiles`] (Perl `:111`).
pub fn select_input_files(
    files: &[PathBuf],
    cx: bool,
) -> Result<Vec<PathBuf>, BismarkBedgraphError> {
    let selected: Vec<PathBuf> = if cx {
        files.to_vec()
    } else {
        files
            .iter()
            .filter(|p| basename(p).starts_with("CpG"))
            .cloned()
            .collect()
    };
    if selected.is_empty() {
        return Err(BismarkBedgraphError::NoCpgFiles);
    }
    Ok(selected)
}

/// Open a methylation-call file for reading, transparently gunzipping when
/// the path ends in `.gz` (Perl detects via `/gz$/`).
fn open_reader(path: &Path) -> Result<Box<dyn BufRead>, BismarkBedgraphError> {
    let file = File::open(path)?;
    if path.extension().and_then(|e| e.to_str()) == Some("gz") {
        Ok(Box::new(BufReader::new(GzDecoder::new(file))))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

/// Outcome of parsing a single data line.
enum LineOutcome<'a> {
    /// A consistent call: chromosome (original, untransformed), 1-based
    /// position, and methylated flag (`strand == "+"`).
    Call {
        chr: &'a str,
        pos: u32,
        methylated: bool,
    },
    /// Fields present but the (strand, call) combination is invalid → warn
    /// and skip (Perl `:369-372`).
    Inconsistent,
    /// The line could not be parsed → fatal (Perl `croak`, `:560`/`:562`).
    /// The `&'static str` is the specific reason, so the surfaced error is
    /// accurate about *which* problem occurred (review A3/B-L1).
    Malformed(&'static str),
}

/// Reason: a required field was absent (short line).
const REASON_MISSING_FIELD: &str =
    "missing strand or call field (expected 5 tab-separated columns)";
/// Reason: the position field was not a positive integer within `u32` range.
/// (`u32::MAX` ≈ 4.29 Gbp comfortably exceeds the largest known chromosome,
/// so this is unreachable for real extractor output — SPEC §7.)
const REASON_BAD_POSITION: &str = "position is not a positive integer within u32 range";

/// Parse a single (already header/`Bismark`-filtered) data line.
///
/// Fields (tab-separated): `id  strand  chr  pos  call`. The `id` field is
/// ignored (SPEC §3 `--remove_spaces`). Strand and call are compared as full
/// fields (Perl `eq` semantics; see [`crate::validate`]).
fn parse_call_line(line: &str) -> LineOutcome<'_> {
    let mut it = line.split('\t');
    let _id = it.next();
    let strand = match it.next() {
        Some(s) => s,
        None => return LineOutcome::Malformed(REASON_MISSING_FIELD),
    };
    let chr = match it.next() {
        Some(s) => s,
        None => return LineOutcome::Malformed(REASON_MISSING_FIELD),
    };
    let pos_field = match it.next() {
        Some(s) => s,
        None => return LineOutcome::Malformed(REASON_MISSING_FIELD),
    };
    let call = match it.next() {
        Some(s) => s,
        None => return LineOutcome::Malformed(REASON_MISSING_FIELD),
    };

    // Position must be a positive (1-based) integer. The extractor always
    // emits this; a non-numeric or zero position is non-physical, so we
    // fail loudly rather than emit a negative bedGraph coordinate.
    let pos: u32 = match pos_field.parse() {
        Ok(p) if p >= 1 => p,
        _ => return LineOutcome::Malformed(REASON_BAD_POSITION),
    };

    if !validate_call(strand, call) {
        return LineOutcome::Inconsistent;
    }
    LineOutcome::Call {
        chr,
        pos,
        methylated: strand == "+",
    }
}

/// Read every call in `path` into `agg`, attributing chromosome ownership to
/// `source_basename` (the file's basename, used in the ordering key).
///
/// Header handling: the first line is skipped unless `no_header`. Any line
/// starting with `Bismark` is skipped. A malformed line aborts with
/// [`BismarkBedgraphError::MalformedCallLine`]; an inconsistent line is
/// warned and skipped.
pub fn read_into(
    path: &Path,
    no_header: bool,
    source_basename: &str,
    agg: &mut Aggregator,
) -> Result<(), BismarkBedgraphError> {
    let mut reader = open_reader(path)?;
    let mut buf = String::new();
    let mut lineno: u64 = 0;
    let mut first = true;

    loop {
        buf.clear();
        let n = reader.read_line(&mut buf)?;
        if n == 0 {
            break;
        }
        lineno += 1;
        // Strip only the trailing '\n' (Perl `chomp`); a stray '\r' is left
        // on the line, which makes a CRLF last-field fail validation — the
        // same degrade Perl exhibits (SPEC §7 CRLF note).
        let line = buf.strip_suffix('\n').unwrap_or(buf.as_str());

        if first {
            first = false;
            if !no_header {
                continue; // drop the version header line
            }
        }
        if line.starts_with("Bismark") {
            continue;
        }

        match parse_call_line(line) {
            LineOutcome::Call {
                chr,
                pos,
                methylated,
            } => agg.add(chr, pos, methylated, source_basename),
            LineOutcome::Inconsistent => {
                eprintln!(
                    "Methylation state in file ({}) on line {} is inconsistent — skipping",
                    path.display(),
                    lineno
                );
            }
            LineOutcome::Malformed(reason) => {
                return Err(BismarkBedgraphError::MalformedCallLine {
                    file: path.to_path_buf(),
                    line: lineno,
                    reason,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_strips_path() {
        assert_eq!(basename(Path::new("/a/b/CpG_OT_s.txt")), "CpG_OT_s.txt");
        assert_eq!(basename(Path::new("CpG_OT_s.txt")), "CpG_OT_s.txt");
    }

    #[test]
    fn select_default_keeps_only_cpg() {
        let files = vec![
            PathBuf::from("CpG_OT_s.txt"),
            PathBuf::from("CHG_OT_s.txt"),
            PathBuf::from("CHH_OT_s.txt"),
            PathBuf::from("CpG_OB_s.txt"),
        ];
        let sel = select_input_files(&files, false).unwrap();
        assert_eq!(
            sel,
            vec![PathBuf::from("CpG_OT_s.txt"), PathBuf::from("CpG_OB_s.txt")]
        );
    }

    #[test]
    fn select_cx_keeps_all_in_order() {
        let files = vec![PathBuf::from("CHG_OT_s.txt"), PathBuf::from("CpG_OT_s.txt")];
        let sel = select_input_files(&files, true).unwrap();
        assert_eq!(sel, files);
    }

    #[test]
    fn select_default_no_cpg_errors() {
        let files = vec![PathBuf::from("CHG_OT_s.txt")];
        assert!(matches!(
            select_input_files(&files, false).unwrap_err(),
            BismarkBedgraphError::NoCpgFiles
        ));
    }

    fn parse(line: &str) -> LineOutcome<'_> {
        parse_call_line(line)
    }

    #[test]
    fn parse_valid_cpg_plus() {
        match parse("read1\t+\tchr1\t100\tZ") {
            LineOutcome::Call {
                chr,
                pos,
                methylated,
            } => {
                assert_eq!(chr, "chr1");
                assert_eq!(pos, 100);
                assert!(methylated);
            }
            _ => panic!("expected Call"),
        }
    }

    #[test]
    fn parse_valid_cpg_minus_unmethylated() {
        match parse("read1\t-\tchr1\t100\tz") {
            LineOutcome::Call { methylated, .. } => assert!(!methylated),
            _ => panic!("expected Call"),
        }
    }

    #[test]
    fn parse_inconsistent_is_skipped() {
        assert!(matches!(
            parse("r\t+\tchr1\t100\tz"),
            LineOutcome::Inconsistent
        ));
    }

    #[test]
    fn parse_missing_call_field_is_malformed_with_field_reason() {
        // Accurate reason: missing field (not a position complaint) — A3/B-L1.
        match parse("read1\t+\tchr1\t100") {
            LineOutcome::Malformed(r) => assert_eq!(r, REASON_MISSING_FIELD),
            _ => panic!("expected Malformed(missing field)"),
        }
    }

    #[test]
    fn parse_empty_line_is_malformed() {
        assert!(matches!(parse(""), LineOutcome::Malformed(_)));
    }

    #[test]
    fn parse_zero_position_is_malformed_with_position_reason() {
        match parse("r\t+\tchr1\t0\tZ") {
            LineOutcome::Malformed(r) => assert_eq!(r, REASON_BAD_POSITION),
            _ => panic!("expected Malformed(bad position)"),
        }
    }

    #[test]
    fn parse_non_numeric_position_is_malformed_with_position_reason() {
        // Fields are all PRESENT — the reason must be about the position,
        // not "missing field" (the misleading-message bug both reviewers hit).
        match parse("r\t+\tchr1\tNaN\tZ") {
            LineOutcome::Malformed(r) => assert_eq!(r, REASON_BAD_POSITION),
            _ => panic!("expected Malformed(bad position)"),
        }
    }

    #[test]
    fn header_skip_default_drops_first_line_no_header_keeps_it() {
        // Plan-required --no_header ON/OFF coverage (COVERAGE.md gap #1).
        // Header-LESS input whose first line is genuine data: under default
        // (no_header=false) the first line is dropped (data loss → 1:10 sees
        // only the unmeth `z` → 0/1); under --no_header it is kept (1 meth +
        // 1 unmeth). Mirrors the live Perl parity check.
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("CpG_OT_nohdr.txt");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "r\t+\t1\t10\tZ").unwrap(); // first line: a real methylated call
            writeln!(f, "r\t-\t1\t10\tz").unwrap();
        }

        let mut agg_default = Aggregator::new();
        read_into(&path, false, "CpG_OT_nohdr.txt", &mut agg_default).unwrap();
        assert_eq!(agg_default.into_sorted()[0].1, vec![(10, 0, 1)]);

        let mut agg_nohdr = Aggregator::new();
        read_into(&path, true, "CpG_OT_nohdr.txt", &mut agg_nohdr).unwrap();
        assert_eq!(agg_nohdr.into_sorted()[0].1, vec![(10, 1, 1)]);
    }

    #[test]
    fn bismark_header_line_skipped_no_space() {
        // The no-space `^Bismark` skip (I1): a "Bismark ..." line anywhere is
        // dropped even under --no_header (first line not consumed as header).
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("CpG_OT_h.txt");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            writeln!(f, "Bismark methylation extractor version v0.25.1").unwrap();
            writeln!(f, "r\t+\t1\t10\tZ").unwrap();
        }
        let mut agg = Aggregator::new();
        read_into(&path, true, "CpG_OT_h.txt", &mut agg).unwrap();
        // Only the real call counts; the Bismark line is skipped, not parsed.
        assert_eq!(agg.into_sorted()[0].1, vec![(10, 1, 0)]);
    }
}
