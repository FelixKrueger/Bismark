//! Maps the suite version in Rust output back to Perl's `0.25.1` (#1148), so a
//! Perl-oracle or golden comparison differs from Perl only by that token.
//!
//! Include with `#[path = "common/version.rs"] mod version;`.

#![allow(dead_code)]

/// The Perl Bismark version the oracles and goldens carry.
pub const PERL_VERSION: &str = "0.25.1";

/// A version-bearing form in a Rust output.
#[derive(Clone, Copy, Debug)]
pub enum Form {
    /// Split-file line 1: `extractor version v<S>`.
    SplitFileHeader,
    /// Splitting report: `Extractor Version: v<S>`.
    SplittingReport,
    /// Summary HTML footer: `(version <S>)`.
    SummaryHtml,
}

impl Form {
    fn token(self, version: &str) -> String {
        match self {
            Form::SplitFileHeader => format!("extractor version v{version}\n"),
            Form::SplittingReport => format!("Extractor Version: v{version}\n"),
            Form::SummaryHtml => format!("(version {version})"),
        }
    }
}

/// Rewrite each `expected` form from `suite` to [`PERL_VERSION`], after asserting
/// it occurs exactly the expected number of times.
pub fn perl_version_for(s: &str, suite: &str, expected: &[(Form, usize)]) -> String {
    let mut out = s.to_string();
    for &(form, count) in expected {
        let from = form.token(suite);
        let found = out.matches(&from).count();
        assert_eq!(
            found, count,
            "expected {count} × {from:?} in the Rust output, found {found}"
        );
        out = out.replace(&from, &form.token(PERL_VERSION));
    }
    out
}

/// [`perl_version_for`] at this build's suite version.
pub fn perl_version(s: &str, expected: &[(Form, usize)]) -> String {
    perl_version_for(s, bismark::meta::SUITE_VERSION, expected)
}
