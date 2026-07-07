//! Output-filename derivation, faithful to Perl `bam2nuc` `:147-152`.
//!
//! ```text
//! $outfile = basename($infile);                 # s/.*\///
//! die unless $outfile =~ s/(bam|cram)$/nucleotide_stats.txt/;
//! ```
//!
//! Two quirks replicated exactly:
//! - **No dot anchor:** the regex strips a trailing `bam`/`cram` token, NOT
//!   `.bam`/`.cram` — so `foosubbam` → `foosubnucleotide_stats.txt`, and
//!   `a.bam.bam` → `a.bam.nucleotide_stats.txt` (only the LAST `bam` goes).
//! - **Case-sensitive:** `weird.BAM` does NOT match `(bam|cram)$` → error
//!   (Perl `die`). The Rust port matches the lowercase token exactly.

use std::path::Path;

use crate::bam2nuc::error::BismarkBam2nucError;

/// Derive the `*.nucleotide_stats.txt` output basename from an input path.
///
/// # Errors
/// [`BismarkBam2nucError::NotBamOrCram`] if the basename does not end in the
/// exact lowercase token `bam` or `cram` (Perl's case-sensitive substitution
/// failure → `die`).
pub fn derive_output_name(infile: &Path) -> Result<String, BismarkBam2nucError> {
    let base = infile
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Perl regex `(bam|cram)$` tries `bam` then `cram`; both case-sensitive,
    // no dot anchor. strip_suffix mirrors this exactly.
    for token in ["bam", "cram"] {
        if let Some(stem) = base.strip_suffix(token) {
            return Ok(format!("{stem}nucleotide_stats.txt"));
        }
    }
    Err(BismarkBam2nucError::NotBamOrCram { name: base })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn derive(p: &str) -> Result<String, BismarkBam2nucError> {
        derive_output_name(Path::new(p))
    }

    #[test]
    fn bam_basic() {
        assert_eq!(derive("sample.bam").unwrap(), "sample.nucleotide_stats.txt");
    }

    #[test]
    fn strips_path_to_basename() {
        assert_eq!(derive("/path/to/x.bam").unwrap(), "x.nucleotide_stats.txt");
    }

    #[test]
    fn cram_basic() {
        assert_eq!(derive("y.cram").unwrap(), "y.nucleotide_stats.txt");
    }

    #[test]
    fn sam_is_rejected() {
        assert!(matches!(
            derive("z.sam").unwrap_err(),
            BismarkBam2nucError::NotBamOrCram { name } if name == "z.sam"
        ));
    }

    #[test]
    fn no_dot_anchor_quirk() {
        // Perl `s/(bam|cram)$/.../` has no preceding dot — a trailing `bam`
        // without a dot is still stripped.
        assert_eq!(derive("foosubbam").unwrap(), "foosubnucleotide_stats.txt");
    }

    #[test]
    fn only_trailing_token_stripped() {
        // `a.bam.bam` → only the LAST `bam` is replaced.
        assert_eq!(derive("a.bam.bam").unwrap(), "a.bam.nucleotide_stats.txt");
    }

    #[test]
    fn case_sensitive_uppercase_rejected() {
        // Perl's regex is case-sensitive: `.BAM` does NOT match → die.
        assert!(matches!(
            derive("weird.BAM").unwrap_err(),
            BismarkBam2nucError::NotBamOrCram { .. }
        ));
    }
}
