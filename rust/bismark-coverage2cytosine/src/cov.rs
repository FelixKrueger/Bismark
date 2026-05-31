//! Bismark coverage-file (`*.bismark.cov[.gz]`) reading + line parsing.
//!
//! Mirrors Perl `generate_genome_wide_cytosine_report:184-209`: gz-aware open,
//! tab-split, **column 4 (percentage) and column 3 (end) discarded**, 1-based
//! `start` as the lookup key. Parse policy folded from Phase-B review (B-I1/2/3):
//! strip a trailing `\r` (CRLF), skip blank lines, strict `u32` fields → a
//! typed `MalformedCovLine` (accepted divergence from Perl's lenient coercion;
//! cannot occur on real `bismark2bedGraph` output).

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::error::BismarkC2cError;

/// One parsed coverage record: `(chr, start, meth, nonmeth)` — column 2 (end)
/// and column 3 (percentage) are discarded.
pub type CovRecord = (Vec<u8>, u32, u32, u32);

/// Open the coverage file, transparently decompressing `.gz` (plain gzip via
/// `MultiGzDecoder`, matching Perl's `gunzip -c`).
pub fn open_cov(path: &Path) -> Result<Box<dyn BufRead>, BismarkC2cError> {
    let file = File::open(path)?;
    let is_gz = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".gz"));
    if is_gz {
        Ok(Box::new(BufReader::new(flate2::read::MultiGzDecoder::new(
            file,
        ))))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

/// Parse one coverage line into `(chr, start, meth, nonmeth)`.
///
/// Returns `Ok(None)` for a blank line (skipped — no phantom chromosome).
/// A trailing `\n` and/or `\r` is stripped first. Fewer than 6 tab fields, or
/// a non-numeric `start`/`meth`/`nonmeth`, yields `MalformedCovLine`.
pub fn parse_cov_line(line: &[u8], line_no: usize) -> Result<Option<CovRecord>, BismarkC2cError> {
    // Strip trailing \n then \r (CRLF-safe).
    let mut line = line;
    if line.last() == Some(&b'\n') {
        line = &line[..line.len() - 1];
    }
    if line.last() == Some(&b'\r') {
        line = &line[..line.len() - 1];
    }
    if line.is_empty() {
        return Ok(None);
    }

    let fields: Vec<&[u8]> = line.split(|&b| b == b'\t').collect();
    if fields.len() < 6 {
        return Err(BismarkC2cError::MalformedCovLine { line_no });
    }
    let chr = fields[0].to_vec();
    let start = parse_u32(fields[1], line_no)?;
    // fields[2] (end) and fields[3] (percentage) are discarded.
    let meth = parse_u32(fields[4], line_no)?;
    let nonmeth = parse_u32(fields[5], line_no)?;
    Ok(Some((chr, start, meth, nonmeth)))
}

fn parse_u32(field: &[u8], line_no: usize) -> Result<u32, BismarkC2cError> {
    std::str::from_utf8(field)
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or(BismarkC2cError::MalformedCovLine { line_no })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_strips_crlf_and_reads_fields() {
        let (chr, start, m, u) = parse_cov_line(b"chr1\t3\t3\t100\t5\t0\r", 1)
            .unwrap()
            .unwrap();
        assert_eq!((chr.as_slice(), start, m, u), (b"chr1".as_slice(), 3, 5, 0));
    }

    #[test]
    fn parse_handles_trailing_newline() {
        let r = parse_cov_line(b"chr1\t3\t3\t100\t5\t0\n", 1)
            .unwrap()
            .unwrap();
        assert_eq!(r.1, 3);
    }

    #[test]
    fn parse_blank_line_is_skipped() {
        assert!(parse_cov_line(b"", 1).unwrap().is_none());
        assert!(parse_cov_line(b"\r", 1).unwrap().is_none());
        assert!(parse_cov_line(b"\n", 1).unwrap().is_none());
    }

    #[test]
    fn parse_malformed_errors() {
        assert!(matches!(
            parse_cov_line(b"chr1\tNOTNUM\t3\t100\t5\t0", 7),
            Err(BismarkC2cError::MalformedCovLine { line_no: 7 })
        ));
        assert!(matches!(
            parse_cov_line(b"chr1\t3", 2),
            Err(BismarkC2cError::MalformedCovLine { line_no: 2 })
        ));
    }
}
