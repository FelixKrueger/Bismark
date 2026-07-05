//! Embedded `plotly/` assets + the `read_report_template` line normalizer.
//!
//! The HTML template itself is the inline Perl heredoc, extracted verbatim
//! into `summary_template.html` (see [`crate::html::TEMPLATE`]). The three
//! companion assets — `plot.ly` (~3 MB), `bismark.logo`, `bioinf.logo` — are
//! `include_str!`'d here and normalized exactly as Perl `read_report_template`
//! (`bismark2summary:136-149`) does: per source line, `chomp` then `s/\r//g`
//! (strip **all** `\r`, not just a trailing CR), then append `"\n"`.
//!
//! Consequences (matching Perl): the result is LF-normalized, every line is
//! `\n`-terminated, and **non-empty** input always ends in `\n`. An **empty**
//! asset yields `""` (Perl's `while(<DOC>)` never iterates) — guarded.

use std::sync::OnceLock;

// Vendored copies of the repo `plotly/` files live in the crate-local `assets/`
// so the crate is SELF-CONTAINED and publishable to crates.io (a `.crate` tarball
// can only carry files inside the crate root; the earlier `../../../plotly/` embed
// failed `cargo package`'s verify-build). The drift-guard test below keeps them
// byte-identical to the canonical repo `plotly/` files.
const PLOTLY_RAW: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/plot.ly"));
const BISMARK_LOGO_RAW: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/bismark.logo"));
const BIOINF_LOGO_RAW: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/bioinf.logo"));

/// Normalize an asset the way Perl `read_report_template` does. Pure, total.
#[must_use]
pub fn normalize_asset(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    // Perl `while (<DOC>)` reads records ending in `\n`; the final record may
    // lack a `\n` but is still read. `split('\n')` yields a trailing "" when
    // `raw` ends in `\n` — that empty is NOT a record Perl read, so drop it.
    let mut parts: Vec<&str> = raw.split('\n').collect();
    if raw.ends_with('\n') {
        parts.pop();
    }
    let mut out = String::with_capacity(raw.len() + parts.len());
    for line in parts {
        if line.contains('\r') {
            out.extend(line.chars().filter(|&c| c != '\r'));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// The normalized plot.ly library (cached; normalized once per process).
#[must_use]
pub fn plotly() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| normalize_asset(PLOTLY_RAW))
}

/// The normalized Bismark logo (base64 `<img>` markup).
#[must_use]
pub fn bismark_logo() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| normalize_asset(BISMARK_LOGO_RAW))
}

/// The normalized Babraham Bioinformatics logo.
#[must_use]
pub fn bioinf_logo() -> &'static str {
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(|| normalize_asset(BIOINF_LOGO_RAW))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_asset_stays_empty() {
        assert_eq!(normalize_asset(""), "");
    }

    #[test]
    fn strips_all_cr_and_lf_terminates_each_line() {
        // Mid-line \r removed (not just CRLF terminators); trailing-\n input
        // does not gain an extra blank line.
        assert_eq!(normalize_asset("a\r\nb\rc\n"), "a\nbc\n");
        // No trailing \n on input → output still ends in \n.
        assert_eq!(normalize_asset("x"), "x\n");
        // Interior blank line preserved.
        assert_eq!(normalize_asset("a\n\nb\n"), "a\n\nb\n");
    }

    #[test]
    fn embedded_assets_are_present_and_lf_terminated() {
        assert!(plotly().len() > 1_000_000, "plot.ly should be ~3 MB");
        assert!(plotly().ends_with('\n'));
        assert!(bismark_logo().contains("img") || bismark_logo().contains("base64"));
        assert!(bioinf_logo().ends_with('\n'));
        // No stray carriage returns survive normalization.
        assert!(!plotly().contains('\r'));
    }

    #[test]
    fn vendored_assets_match_repo_plotly_files() {
        // Drift guard: the vendored `assets/` bytes embedded via `include_str!`
        // must equal the CANONICAL repo `plotly/` files (Perl's source of truth),
        // so the publishable vendored copy can't silently drift. Runs only under
        // `cargo test` (workspace present) — reads `../../plotly` at runtime, so
        // it does NOT affect `cargo package`'s verify-build.
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plotly");
        for (name, raw) in [
            ("plot.ly", PLOTLY_RAW),
            ("bismark.logo", BISMARK_LOGO_RAW),
            ("bioinf.logo", BIOINF_LOGO_RAW),
        ] {
            let on_disk = std::fs::read_to_string(base.join(name)).unwrap();
            assert_eq!(on_disk, raw, "vendored {name} drifted from plotly/{name}");
        }
    }
}
