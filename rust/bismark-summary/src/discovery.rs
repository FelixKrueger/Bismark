//! BAM discovery and per-BAM report-filename derivation.
//!
//! `bismark2summary` never opens a BAM — it uses the BAM **filename** to
//! derive the names of each sample's text report files. This module mirrors
//! Perl `bismark2summary:152-367`:
//!
//! - **Discovery** (`:152-205`): explicit `@ARGV` verbatim, else four globs
//!   in a fixed order, each sorted with Perl's **case-folded** glob
//!   collation (spike-confirmed: case-fold-primary, raw-bytes-secondary,
//!   locale/platform-invariant — `SPIKE_glob_sort_order.md`).
//! - **Derivation** (`:251-367`): `.bam` strip, `_pe` ⇒ PE, and the
//!   `_PE/_SE_report.txt`, `deduplication_report.txt`, and (dedup-existence
//!   dependent) `splitting_report.txt` filenames.
//!
//! ASCII-only filenames are assumed (Bismark never produces non-ASCII BAM
//! names); `to_ascii_lowercase` folds only `A–Z`. This is the documented
//! divergence boundary (SPEC §2.3 / §4.8).

use std::path::{Path, PathBuf};

use crate::error::BismarkSummaryError;

/// The four auto-detect glob suffixes, in Perl's fixed order
/// (`bismark2summary:159-197`): SE-bt2, PE-bt2, SE-hisat2, PE-hisat2.
const GLOB_SUFFIXES: [&str; 4] = [
    "bismark_bt2.bam",
    "bismark_bt2_pe.bam",
    "bismark_hisat2.bam",
    "bismark_hisat2_pe.bam",
];

/// Per-BAM derived report filenames (relative strings; resolved against the
/// scan/working dir by the caller). `splitting_report` is computed
/// separately because its name depends on whether `dedup_report` exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedNames {
    /// The `$base` after stripping `.bam` and any trailing `_pe`.
    pub base: String,
    /// `true` if the BAM basename ended in `_pe` (paired-end).
    pub paired: bool,
    /// Mandatory Bismark alignment report (`<base>_PE_report.txt` /
    /// `<base>_SE_report.txt`).
    pub alignment_report: String,
    /// Optional deduplication report.
    pub dedup_report: String,
}

impl DerivedNames {
    /// The splitting (methylation-extractor) report name. Depends on PE/SE
    /// **and** whether the dedup report exists (Perl `:352-367`).
    #[must_use]
    pub fn splitting_report(&self, dedup_exists: bool) -> String {
        match (self.paired, dedup_exists) {
            (true, true) => format!("{}_pe.deduplicated_splitting_report.txt", self.base),
            (true, false) => format!("{}_pe_splitting_report.txt", self.base),
            (false, true) => format!("{}.deduplicated_splitting_report.txt", self.base),
            (false, false) => format!("{}_splitting_report.txt", self.base),
        }
    }
}

/// Strip the trailing 4 chars (`.bam`) the way Perl `substr($bam, 0, -4)`
/// does. A string shorter than 4 chars yields `""` (Perl's negative-length
/// `substr` clamps to empty). Char-aware (no byte-boundary panic).
#[must_use]
pub fn strip_bam_suffix(bam: &str) -> String {
    let keep = bam.chars().count().saturating_sub(4);
    bam.chars().take(keep).collect()
}

/// Derive the alignment + dedup report names for one BAM string (Perl
/// `:251-324`).
#[must_use]
pub fn derive_names(bam: &str) -> DerivedNames {
    let mut base = strip_bam_suffix(bam);
    let paired = base.ends_with("_pe");
    if paired {
        // `$base =~ s/_pe$//` — "_pe" is 3 ASCII bytes.
        base.truncate(base.len() - 3);
    }
    let alignment_report = if paired {
        format!("{base}_PE_report.txt")
    } else {
        format!("{base}_SE_report.txt")
    };
    let dedup_report = if paired {
        format!("{base}_pe.deduplication_report.txt")
    } else {
        format!("{base}.deduplication_report.txt")
    };
    DerivedNames {
        base,
        paired,
        alignment_report,
        dedup_report,
    }
}

/// Sort filenames the way Perl's `glob` does: **case-fold-primary,
/// raw-ASCII-bytes-secondary** (spike-confirmed). NOT bytewise.
pub fn sort_glob(names: &mut [String]) {
    names.sort_by(|a, b| {
        a.to_ascii_lowercase()
            .cmp(&b.to_ascii_lowercase())
            .then_with(|| a.as_bytes().cmp(b.as_bytes()))
    });
}

/// Discover the BAM list (Perl `:152-205`).
///
/// `explicit` non-empty ⇒ returned verbatim in argv order (no globbing, no
/// existence check). Otherwise `dir` is scanned for the four globs in fixed
/// order, each sorted via [`sort_glob`], concatenated. Empty result ⇒
/// [`BismarkSummaryError::NoBamFiles`].
pub fn discover_bams(explicit: &[PathBuf], dir: &Path) -> Result<Vec<String>, BismarkSummaryError> {
    if !explicit.is_empty() {
        return Ok(explicit
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect());
    }

    // Read the directory once. A leading-dot name is skipped (Perl's `<*…>`
    // glob does not match dotfiles). An unreadable dir ⇒ no matches (Perl
    // glob returns empty rather than dying).
    let names: Vec<String> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.starts_with('.'))
            .collect(),
        Err(_) => Vec::new(),
    };

    let mut out = Vec::new();
    for suffix in GLOB_SUFFIXES {
        let mut matched: Vec<String> = names
            .iter()
            .filter(|n| n.ends_with(suffix))
            .cloned()
            .collect();
        sort_glob(&mut matched);
        out.append(&mut matched);
    }

    if out.is_empty() {
        return Err(BismarkSummaryError::NoBamFiles);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn strips_bam_suffix() {
        assert_eq!(
            strip_bam_suffix("sample_bismark_bt2.bam"),
            "sample_bismark_bt2"
        );
        assert_eq!(strip_bam_suffix("x.bam"), "x");
        // <4 chars → empty (Perl substr negative-length clamp).
        assert_eq!(strip_bam_suffix("ab"), "");
        // Non-.bam argv entry still loses its last 4 chars (Perl edge).
        assert_eq!(strip_bam_suffix("weird.txt"), "weird");
    }

    #[test]
    fn derives_se_names() {
        let d = derive_names("DRR_Sperm_trimmed.fq.gz_bismark_bt2.bam");
        assert!(!d.paired);
        assert_eq!(d.base, "DRR_Sperm_trimmed.fq.gz_bismark_bt2");
        assert_eq!(
            d.alignment_report,
            "DRR_Sperm_trimmed.fq.gz_bismark_bt2_SE_report.txt"
        );
        assert_eq!(
            d.dedup_report,
            "DRR_Sperm_trimmed.fq.gz_bismark_bt2.deduplication_report.txt"
        );
        assert_eq!(
            d.splitting_report(true),
            "DRR_Sperm_trimmed.fq.gz_bismark_bt2.deduplicated_splitting_report.txt"
        );
        assert_eq!(
            d.splitting_report(false),
            "DRR_Sperm_trimmed.fq.gz_bismark_bt2_splitting_report.txt"
        );
    }

    #[test]
    fn derives_pe_names() {
        let d = derive_names("WT_R1_val_1_bismark_bt2_pe.bam");
        assert!(d.paired);
        assert_eq!(d.base, "WT_R1_val_1_bismark_bt2");
        assert_eq!(d.alignment_report, "WT_R1_val_1_bismark_bt2_PE_report.txt");
        assert_eq!(
            d.dedup_report,
            "WT_R1_val_1_bismark_bt2_pe.deduplication_report.txt"
        );
        assert_eq!(
            d.splitting_report(true),
            "WT_R1_val_1_bismark_bt2_pe.deduplicated_splitting_report.txt"
        );
        assert_eq!(
            d.splitting_report(false),
            "WT_R1_val_1_bismark_bt2_pe_splitting_report.txt"
        );
    }

    #[test]
    fn glob_sort_is_case_folded_not_bytewise() {
        // Spike result: Perl glob = case-fold-primary, raw-bytes-secondary.
        let mut v = vec![
            "Mango".to_string(),
            "apple".to_string(),
            "Banana".to_string(),
            "a".to_string(),
            "zebra".to_string(),
            "b".to_string(),
        ];
        sort_glob(&mut v);
        assert_eq!(v, vec!["a", "apple", "b", "Banana", "Mango", "zebra"]);
    }

    #[test]
    fn glob_sort_case_only_tiebreak() {
        // Spike result (Linux): Apple, aPPle, apple (raw-byte tiebreak).
        let mut v = vec![
            "apple".to_string(),
            "Apple".to_string(),
            "aPPle".to_string(),
        ];
        sort_glob(&mut v);
        assert_eq!(v, vec!["Apple", "aPPle", "apple"]);
    }

    #[test]
    fn explicit_argv_is_verbatim_order() {
        let argv = vec![PathBuf::from("z.bam"), PathBuf::from("a.bam")];
        let got = discover_bams(&argv, Path::new("/nonexistent")).unwrap();
        assert_eq!(got, vec!["z.bam", "a.bam"]);
    }

    #[test]
    fn glob_discovery_fixed_order_and_exclusivity() {
        let dir = tempfile::tempdir().unwrap();
        let make = |n: &str| File::create(dir.path().join(n)).unwrap();
        // One of each kind + a dotfile that must be skipped.
        make("s_bismark_bt2.bam");
        make("p_bismark_bt2_pe.bam");
        make("s_bismark_hisat2.bam");
        make("p_bismark_hisat2_pe.bam");
        make(".hidden_bismark_bt2.bam");
        make("not_a_match.bam");

        let got = discover_bams(&[], dir.path()).unwrap();
        // Fixed glob order: SE-bt2, PE-bt2, SE-hisat2, PE-hisat2. Dotfile and
        // non-matching file excluded.
        assert_eq!(
            got,
            vec![
                "s_bismark_bt2.bam",
                "p_bismark_bt2_pe.bam",
                "s_bismark_hisat2.bam",
                "p_bismark_hisat2_pe.bam",
            ]
        );
    }

    #[test]
    fn glob_se_and_pe_are_disjoint() {
        let dir = tempfile::tempdir().unwrap();
        File::create(dir.path().join("x_bismark_bt2.bam")).unwrap();
        File::create(dir.path().join("x_bismark_bt2_pe.bam")).unwrap();
        let got = discover_bams(&[], dir.path()).unwrap();
        // SE glob must NOT also pick up the _pe file, and vice versa.
        assert_eq!(got, vec!["x_bismark_bt2.bam", "x_bismark_bt2_pe.bam"]);
    }

    #[test]
    fn no_bams_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let err = discover_bams(&[], dir.path()).unwrap_err();
        assert!(matches!(err, BismarkSummaryError::NoBamFiles));
    }
}
