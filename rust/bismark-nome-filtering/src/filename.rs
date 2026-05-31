//! Output-filename derivation matching Perl `NOMe_filtering` (`:464-468` plus
//! the force-`.gz` at `:74-76`).
//!
//! The Perl chain is:
//! ```text
//! $out = $infile;
//! $out =~ s/\.gz$//;          # strip ONE trailing .gz
//! $out =~ s/\.txt$//;         # strip ONE trailing .txt
//! $out =~ s/$/.manOwar.txt/;  # append
//! # then, at write time:
//! $out .= '.gz' unless $out =~ /\.gz$/;   # always true here → force .gz
//! ```
//!
//! Two deliberate differences from `bismark-dedup`'s `derive_output_stem`:
//! 1. **No leading-directory strip** — Perl `NOMe_filtering` does NOT do
//!    `s/.*\///`; it operates on the raw infile string. (Real callers pass a
//!    bare filename via `--dir`; a path-qualified infile is untested.)
//! 2. **Only `.gz` then `.txt`, each at most once** — do NOT reuse dedup's
//!    `.gz/.sam/.bam/.txt` strip loop.

/// Derive the `.manOwar.txt.gz` output filename from the raw infile string.
///
/// # Examples
/// ```
/// use bismark_nome_filtering::filename::derive_manowar_name;
/// assert_eq!(derive_manowar_name("x.txt.gz"), "x.manOwar.txt.gz");
/// assert_eq!(derive_manowar_name("x.gz.gz"), "x.gz.manOwar.txt.gz");
/// assert_eq!(derive_manowar_name("x.cov"), "x.cov.manOwar.txt.gz");
/// ```
#[must_use]
pub fn derive_manowar_name(infile: &str) -> String {
    let mut s = infile.to_string();
    if let Some(t) = s.strip_suffix(".gz") {
        s = t.to_string(); // strip ONE .gz
    }
    if let Some(t) = s.strip_suffix(".txt") {
        s = t.to_string(); // strip ONE .txt
    }
    s.push_str(".manOwar.txt"); // append
    s.push_str(".gz"); // force .gz (s never ends in .gz here)
    s
}

#[cfg(test)]
mod tests {
    use super::derive_manowar_name;

    #[test]
    fn txt_gz() {
        assert_eq!(derive_manowar_name("x.txt.gz"), "x.manOwar.txt.gz");
    }

    #[test]
    fn gz_only() {
        assert_eq!(derive_manowar_name("x.gz"), "x.manOwar.txt.gz");
    }

    #[test]
    fn txt_only() {
        assert_eq!(derive_manowar_name("x.txt"), "x.manOwar.txt.gz");
    }

    #[test]
    fn no_ext() {
        assert_eq!(derive_manowar_name("x"), "x.manOwar.txt.gz");
    }

    #[test]
    fn gz_gz_single_strip() {
        // SPEC P15: only ONE .gz is stripped.
        assert_eq!(derive_manowar_name("x.gz.gz"), "x.gz.manOwar.txt.gz");
    }

    #[test]
    fn txt_txt_single_strip() {
        // SPEC P15: only ONE .txt is stripped.
        assert_eq!(derive_manowar_name("x.txt.txt"), "x.txt.manOwar.txt.gz");
    }

    #[test]
    fn other_ext_kept() {
        assert_eq!(derive_manowar_name("x.cov"), "x.cov.manOwar.txt.gz");
    }

    #[test]
    fn realistic_yacht_basename() {
        assert_eq!(
            derive_manowar_name("CpG_OT_sample.txt.gz"),
            "CpG_OT_sample.manOwar.txt.gz"
        );
    }
}
