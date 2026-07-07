//! Output-filename derivation matching Perl `filter_non_conversion`
//! (`process_file`, lines 85–98).
//!
//! The Perl chain for each output is:
//! ```text
//! my $outfile = $infile;        # the CLI arg verbatim — NO directory strip
//! $outfile =~ s/\.bam$//;       # strip a single trailing ".bam" (dot-anchored)
//! $outfile =~ s/$/.<suffix>/;   # append the suffix
//! ```
//!
//! Crucially there is **no `s/.*\///` directory strip** (unlike
//! `deduplicate_bismark`), so outputs land **next to the input** with the
//! full path preserved. And the strip is dot-anchored `\.bam$`, so an input
//! that merely ends in `bam` (e.g. `foobam`, which still passes the top
//! `=~ /bam$/` gate) has nothing stripped.

/// Strip a single trailing `.bam` (dot-anchored) from the input path string.
/// Returns the input unchanged if it does not end in `.bam`.
#[must_use]
pub fn strip_bam_suffix(infile: &str) -> &str {
    infile.strip_suffix(".bam").unwrap_or(infile)
}

/// `<stem>.nonCG_filtered.bam` — the kept-reads output (Perl lines 85–87).
#[must_use]
pub fn kept_bam_name(infile: &str) -> String {
    format!("{}.nonCG_filtered.bam", strip_bam_suffix(infile))
}

/// `<stem>.nonCG_removed_seqs.bam` — the removed-reads output (Perl lines 90–92).
#[must_use]
pub fn removed_bam_name(infile: &str) -> String {
    format!("{}.nonCG_removed_seqs.bam", strip_bam_suffix(infile))
}

/// `<stem>.non-conversion_filtering.txt` — the report (Perl lines 95–97).
#[must_use]
pub fn report_name(infile: &str) -> String {
    format!("{}.non-conversion_filtering.txt", strip_bam_suffix(infile))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_bam_no_directory() {
        assert_eq!(
            kept_bam_name("/p/foo.bam"),
            "/p/foo.nonCG_filtered.bam",
            "path must be preserved (no basename strip)"
        );
        assert_eq!(
            removed_bam_name("/p/foo.bam"),
            "/p/foo.nonCG_removed_seqs.bam"
        );
        assert_eq!(
            report_name("/p/foo.bam"),
            "/p/foo.non-conversion_filtering.txt"
        );
    }

    #[test]
    fn bare_filename() {
        assert_eq!(kept_bam_name("foo.bam"), "foo.nonCG_filtered.bam");
    }

    #[test]
    fn non_dotted_bam_suffix_not_stripped() {
        // `foobam` passes the top `bam$` gate but has no `.bam` to strip.
        assert_eq!(kept_bam_name("foobam"), "foobam.nonCG_filtered.bam");
        assert_eq!(report_name("foobam"), "foobam.non-conversion_filtering.txt");
    }

    #[test]
    fn dots_in_directory_preserved() {
        assert_eq!(
            kept_bam_name("/path/with.dots/sample.bam"),
            "/path/with.dots/sample.nonCG_filtered.bam"
        );
    }

    #[test]
    fn only_one_bam_suffix_stripped() {
        // Perl's single `s/\.bam$//` strips exactly one trailing `.bam`.
        assert_eq!(kept_bam_name("x.bam.bam"), "x.bam.nonCG_filtered.bam");
    }

    #[test]
    fn relative_path_preserved() {
        assert_eq!(
            report_name("./sub/dir/s.bam"),
            "./sub/dir/s.non-conversion_filtering.txt"
        );
    }
}
