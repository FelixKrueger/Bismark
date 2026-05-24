//! Output-filename derivation matching Perl `deduplicate_bismark`.
//!
//! The Perl script's derivation chain (lines 145, 225, 227-230, 576):
//!
//! 1. `$x =~ s/.*\///` — strip leading directory path.
//! 2. `$x =~ s/\.gz$//; $x =~ s/\.sam$//; $x =~ s/\.bam$//; $x =~ s/\.txt$//`
//!    — strip a single trailing extension. (Order matters: `.txt.gz` strips
//!    `.gz` first, leaving `.txt`, then strips `.txt`.)
//! 3. Append the appropriate suffix (`.deduplicated.bam`,
//!    `.multiple.deduplicated.bam`, etc.) — done by the caller.
//!
//! This module exposes [`derive_output_stem`] which performs steps 1 and 2.

use std::path::Path;

/// Derive the output filename stem from an input path or user-supplied
/// `--outfile` argument.
///
/// - If `user_outfile` is `Some(s)`, `s` is used as the source (matches
///   Perl's behaviour of using `$outfile` when `--outfile` is given).
/// - Otherwise the input path is used.
///
/// The source string is:
/// 1. **Path-stripped** (`s/.*\///` — only the final path component
///    survives). So `--outfile /tmp/sample.bam` yields stem `sample`, NOT
///    `/tmp/sample`.
/// 2. **Extension-stripped** — one trailing match from the ordered list
///    `[".gz", ".sam", ".bam", ".txt"]` is removed. Only one is removed
///    per call (`.txt.gz` becomes `.txt` after one call; the caller would
///    need to call again to strip further — matching Perl's sequential
///    `s///` chain at lines 227-230 / 578-581).
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use bismark_dedup::filename::derive_output_stem;
///
/// assert_eq!(derive_output_stem(Path::new("/path/sample.bam"), None), "sample");
/// assert_eq!(derive_output_stem(Path::new("sample.bam"), None), "sample");
/// assert_eq!(derive_output_stem(Path::new("/x/y/z.bam"), Some("/tmp/foo.bam")), "foo");
/// ```
#[must_use]
pub fn derive_output_stem(input: &Path, user_outfile: Option<&str>) -> String {
    // Step 1: pick source string. user_outfile wins if supplied.
    let raw = if let Some(u) = user_outfile {
        u.to_string()
    } else {
        // Treat `input` as a string; we need lossy conversion to keep
        // behaviour consistent on non-UTF-8 paths (Perl operates on raw
        // bytes; for our purposes a lossy fallback is fine — Bismark
        // BAMs come from FASTQs with ASCII-only paths in practice).
        input.to_string_lossy().into_owned()
    };

    // Step 2: basename strip — equivalent to Perl `s/.*\///`. Using
    // `Path::file_name()` here would NOT match Perl on inputs like
    // `foo/` (trailing slash); Perl's regex strips everything up to
    // the last `/`. Using rfind on the raw string is exact.
    let base = match raw.rfind('/') {
        Some(idx) => &raw[idx + 1..],
        None => &raw[..],
    };

    // Step 3: extension strip — one trailing match from the ordered list.
    // Perl's chain is sequential `s/\.gz$//; s/\.sam$//; s/\.bam$//; s/\.txt$//`;
    // each `s///` operates on the result of the previous.
    let mut stem = base.to_string();
    for ext in [".gz", ".sam", ".bam", ".txt"] {
        if let Some(stripped) = stem.strip_suffix(ext) {
            stem = stripped.to_string();
        }
    }
    stem
}

/// Append the dedup suffix to a stem.
///
/// Per Perl line 233-243 and 154/243:
/// - `(stem, multiple=false, sam=false)` → `<stem>.deduplicated.bam`
/// - `(stem, multiple=false, sam=true)`  → `<stem>.deduplicated.sam`
/// - `(stem, multiple=true,  sam=false)` → `<stem>.multiple.deduplicated.bam`
/// - `(stem, multiple=true,  sam=true)`  → `<stem>.multiple.deduplicated.sam`
#[must_use]
pub fn output_filename(stem: &str, multiple: bool, sam: bool) -> String {
    let infix = if multiple { ".multiple" } else { "" };
    let ext = if sam { "sam" } else { "bam" };
    format!("{stem}{infix}.deduplicated.{ext}")
}

/// Build the deduplication-report filename for the given stem.
///
/// Per Perl line 151/154:
/// - `(stem, multiple=false)` → `<stem>.deduplication_report.txt`
/// - `(stem, multiple=true)`  → `<stem>.multiple.deduplication_report.txt`
#[must_use]
pub fn report_filename(stem: &str, multiple: bool) -> String {
    let infix = if multiple { ".multiple" } else { "" };
    format!("{stem}{infix}.deduplication_report.txt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_directory_and_bam_extension() {
        assert_eq!(
            derive_output_stem(Path::new("/path/sample.bam"), None),
            "sample"
        );
    }

    #[test]
    fn strips_directory_only_when_no_known_extension() {
        assert_eq!(
            derive_output_stem(Path::new("/path/sample"), None),
            "sample"
        );
    }

    #[test]
    fn no_directory_no_extension_passthrough() {
        assert_eq!(derive_output_stem(Path::new("sample"), None), "sample");
    }

    #[test]
    fn strips_sam_extension() {
        assert_eq!(derive_output_stem(Path::new("sample.sam"), None), "sample");
    }

    #[test]
    fn strips_gz_then_txt_in_sequence_per_perl_regex_chain() {
        // Perl chain: s/.gz// then s/.txt// — both apply sequentially.
        assert_eq!(
            derive_output_stem(Path::new("sample.txt.gz"), None),
            "sample"
        );
    }

    #[test]
    fn strips_gz_then_sam_in_sequence() {
        assert_eq!(
            derive_output_stem(Path::new("sample.sam.gz"), None),
            "sample"
        );
    }

    #[test]
    fn strips_gz_then_bam_in_sequence() {
        // .bam.gz is unusual but the regex chain handles it
        assert_eq!(
            derive_output_stem(Path::new("sample.bam.gz"), None),
            "sample"
        );
    }

    #[test]
    fn user_outfile_takes_precedence() {
        // Per Perl line 218 (`$user_defined_outfile` controls source); per
        // line 225/576 (`s/.*\///` still applied to user-supplied paths).
        assert_eq!(
            derive_output_stem(Path::new("/in/foo.bam"), Some("/tmp/bar.bam")),
            "bar"
        );
    }

    #[test]
    fn user_outfile_strips_directory_prefix() {
        // The headline test for plan §10.12 / Reviewer A's C3 from rev 1.
        assert_eq!(
            derive_output_stem(Path::new("anything"), Some("/tmp/sample.bam")),
            "sample"
        );
    }

    #[test]
    fn user_outfile_with_no_extension_passthrough() {
        assert_eq!(
            derive_output_stem(Path::new("anything"), Some("my_thing")),
            "my_thing"
        );
    }

    #[test]
    fn unknown_extension_left_intact() {
        // .cram is not in Perl's strip list — left intact. The output-
        // filename derivation downstream will append `.deduplicated.bam`
        // (or `.cram` in the v1.0 CRAM-mirror path, handled elsewhere).
        assert_eq!(
            derive_output_stem(Path::new("sample.cram"), None),
            "sample.cram"
        );
    }

    #[test]
    fn nested_directories_handled() {
        assert_eq!(
            derive_output_stem(Path::new("/a/b/c/d/sample.bam"), None),
            "sample"
        );
    }

    #[test]
    fn output_filename_default() {
        assert_eq!(
            output_filename("sample", false, false),
            "sample.deduplicated.bam"
        );
    }

    #[test]
    fn output_filename_sam_mode() {
        assert_eq!(
            output_filename("sample", false, true),
            "sample.deduplicated.sam"
        );
    }

    #[test]
    fn output_filename_multiple_mode() {
        assert_eq!(
            output_filename("sample", true, false),
            "sample.multiple.deduplicated.bam"
        );
    }

    #[test]
    fn output_filename_multiple_sam() {
        assert_eq!(
            output_filename("sample", true, true),
            "sample.multiple.deduplicated.sam"
        );
    }

    #[test]
    fn report_filename_default() {
        assert_eq!(
            report_filename("sample", false),
            "sample.deduplication_report.txt"
        );
    }

    #[test]
    fn report_filename_multiple() {
        // Spelled out per Reviewer A's H5 from rev 1.
        assert_eq!(
            report_filename("sample", true),
            "sample.multiple.deduplication_report.txt"
        );
    }
}
