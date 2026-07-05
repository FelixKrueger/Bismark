//! Output-filename derivation matching Perl `methylation_consistency`.
//!
//! Perl line 186: `my $file_root = $file; $file_root =~ s/\.bam$//;`. That
//! is the *entire* derivation — strip a **single trailing `.bam`** and keep
//! everything else (including the full directory prefix) verbatim. Outputs
//! are written **adjacent to the input file**, NOT to the current directory.
//!
//! ⚠ This is deliberately **NOT** how `bismark-dedup/src/filename.rs` works
//! (it basename-strips with `s/.*\///` and writes to `--output_dir`). Do not
//! "unify" the two — methcons has no `--output_dir` and must preserve the
//! input's directory (Reviewer B's #1 trap, SPEC §2.7 / PLAN A4).

use std::path::{Path, PathBuf};

use crate::classify::Bucket;

/// Derive the output root from the input path: strip one trailing `.bam`
/// only (Perl `s/\.bam$//`). The directory prefix and the rest of the
/// basename are preserved verbatim.
#[must_use]
pub fn output_root(input: &Path) -> String {
    let raw = input.to_string_lossy();
    match raw.strip_suffix(".bam") {
        Some(stem) => stem.to_string(),
        None => raw.into_owned(),
    }
}

/// The `_CHH` filename infix used in CHH mode (Perl `$chh_status`), else `""`.
#[must_use]
fn chh_infix(chh: bool) -> &'static str {
    if chh { "_CHH" } else { "" }
}

/// Path of the output BAM for a given consistency bucket.
///
/// `{root}{_CHH?}_all_meth.bam` / `_all_unmeth.bam` / `_mixed_meth.bam`
/// (Perl lines 196–198).
#[must_use]
pub fn bucket_path(root: &str, chh: bool, bucket: Bucket) -> PathBuf {
    let suffix = match bucket {
        Bucket::AllMeth => "_all_meth.bam",
        Bucket::AllUnmeth => "_all_unmeth.bam",
        Bucket::Mixed => "_mixed_meth.bam",
    };
    PathBuf::from(format!("{root}{}{suffix}", chh_infix(chh)))
}

/// Path of the consistency report: `{root}{_CHH?}_consistency_report.txt`
/// (Perl line 199).
#[must_use]
pub fn report_path(root: &str, chh: bool) -> PathBuf {
    PathBuf::from(format!("{root}{}_consistency_report.txt", chh_infix(chh)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_root_strips_single_trailing_bam_keeps_directory() {
        assert_eq!(output_root(Path::new("/a/b/c.bam")), "/a/b/c");
    }

    #[test]
    fn output_root_no_directory() {
        assert_eq!(output_root(Path::new("sample.bam")), "sample");
    }

    #[test]
    fn output_root_no_bam_extension_left_intact() {
        assert_eq!(output_root(Path::new("noext")), "noext");
    }

    #[test]
    fn output_root_dotted_basename_only_strips_bam() {
        assert_eq!(output_root(Path::new("/a/b/s.sorted.bam")), "/a/b/s.sorted");
    }

    #[test]
    fn output_root_strips_only_one_bam() {
        // Perl `s/\.bam$//` removes exactly one trailing `.bam`.
        assert_eq!(output_root(Path::new("x.bam.bam")), "x.bam");
    }

    #[test]
    fn output_root_does_not_strip_non_bam_extension() {
        // Only `.bam` is stripped; `.sam` is kept (Perl strips only `.bam`).
        assert_eq!(output_root(Path::new("sample.sam")), "sample.sam");
    }

    #[test]
    fn bucket_paths_cpg() {
        assert_eq!(
            bucket_path("/a/b/c", false, Bucket::AllMeth),
            PathBuf::from("/a/b/c_all_meth.bam")
        );
        assert_eq!(
            bucket_path("/a/b/c", false, Bucket::AllUnmeth),
            PathBuf::from("/a/b/c_all_unmeth.bam")
        );
        assert_eq!(
            bucket_path("/a/b/c", false, Bucket::Mixed),
            PathBuf::from("/a/b/c_mixed_meth.bam")
        );
    }

    #[test]
    fn bucket_paths_chh_gain_infix() {
        assert_eq!(
            bucket_path("/a/b/c", true, Bucket::AllMeth),
            PathBuf::from("/a/b/c_CHH_all_meth.bam")
        );
    }

    #[test]
    fn report_paths() {
        assert_eq!(
            report_path("/a/b/c", false),
            PathBuf::from("/a/b/c_consistency_report.txt")
        );
        assert_eq!(
            report_path("/a/b/c", true),
            PathBuf::from("/a/b/c_CHH_consistency_report.txt")
        );
    }

    #[test]
    fn outputs_land_in_the_input_directory_not_cwd() {
        // The headline path-preservation guard (PLAN A4): every output's
        // parent directory equals the input's parent directory.
        let input = Path::new("/some/nested/dir/sample.bam");
        let root = output_root(input);
        let input_dir = input.parent().unwrap();
        for bucket in [Bucket::AllMeth, Bucket::AllUnmeth, Bucket::Mixed] {
            assert_eq!(
                bucket_path(&root, false, bucket).parent().unwrap(),
                input_dir
            );
        }
        assert_eq!(report_path(&root, false).parent().unwrap(), input_dir);
    }
}
