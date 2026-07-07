//! The four embedded Plotly assets + the faithful `read_report_template`
//! line-normalizer (SPEC §2.6 / §8.2).
//!
//! Embedding strategy: `include_str!` from the crate-local `assets/` directory
//! (VENDORED copies of the repo `plotly/` files) so the crate is SELF-CONTAINED
//! and publishable to crates.io — a `.crate` tarball can only carry files inside
//! the crate root, so the earlier `../../plotly/` embed failed `cargo package`'s
//! verify-build (the files aren't in the tarball). The `embedded_assets_match_repo_plotly_files`
//! test asserts the vendored bytes still equal the canonical repo `plotly/` files
//! (drift guard — keeps the vendored copy in lockstep with Perl's source of truth).

use std::sync::OnceLock;

/// Raw `plotly_template.tpl` (the `{{placeholder}}` HTML skeleton).
const RAW_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/plotly_template.tpl"
));
/// Raw `plot.ly` (~3 MB plotly.js v1.48.3, already wrapped in `<script>`).
const RAW_PLOTLY: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/plot.ly"));
/// Raw `bismark.logo` (base64 `<img>` tag).
const RAW_BISMARK_LOGO: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/bismark.logo"));
/// Raw `bioinf.logo` (base64 `<img>` tag).
const RAW_BIOINF_LOGO: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/bioinf.logo"));

/// Faithful reproduction of Perl `read_report_template` (`bismark2report:1025-1038`):
/// read line-by-line, `chomp`, strip **all** `\r` (`s/\r//g`, not just a trailing
/// one), then append `\n` to every line. Consequences: the result is fully
/// LF-normalized and (for non-empty input) always ends in `\n`.
///
/// **Empty-input guard (PLAN/SPEC §8.2):** Perl's `while(<DOC>)` never iterates
/// an empty file, so `$doc` stays undefined (→ `""`), *not* `"\n"`. We special
/// -case empty input → `""`. (The four real assets are non-empty.)
pub fn normalize(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    // Perl reads records split on '\n'; a trailing '\n' does NOT yield an extra
    // empty record. `split('\n')` would, so drop the trailing empty element when
    // the input ends in '\n'.
    let mut parts: Vec<&str> = raw.split('\n').collect();
    if raw.ends_with('\n') {
        parts.pop();
    }
    let mut out = String::with_capacity(raw.len() + parts.len());
    for p in parts {
        out.push_str(&p.replace('\r', "")); // s/\r//g — strip ALL carriage returns
        out.push('\n');
    }
    out
}

/// Normalized template.
pub fn template() -> &'static str {
    static N: OnceLock<String> = OnceLock::new();
    N.get_or_init(|| normalize(RAW_TEMPLATE)).as_str()
}

/// Normalized `plot.ly` library.
pub fn plotly() -> &'static str {
    static N: OnceLock<String> = OnceLock::new();
    N.get_or_init(|| normalize(RAW_PLOTLY)).as_str()
}

/// Normalized Bismark logo.
pub fn bismark_logo() -> &'static str {
    static N: OnceLock<String> = OnceLock::new();
    N.get_or_init(|| normalize(RAW_BISMARK_LOGO)).as_str()
}

/// Normalized Babraham Bioinformatics logo.
pub fn bioinf_logo() -> &'static str {
    static N: OnceLock<String> = OnceLock::new();
    N.get_or_init(|| normalize(RAW_BIOINF_LOGO)).as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_empty_not_newline() {
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn trailing_newline_not_doubled() {
        assert_eq!(normalize("a\nb\n"), "a\nb\n");
    }

    #[test]
    fn final_line_without_newline_gains_one() {
        assert_eq!(normalize("a\nb"), "a\nb\n");
    }

    #[test]
    fn strips_all_carriage_returns_including_mid_line() {
        assert_eq!(normalize("a\r\nb\rc\n"), "a\nbc\n");
    }

    #[test]
    fn lone_newline() {
        assert_eq!(normalize("\n"), "\n");
    }

    #[test]
    fn assets_are_brace_and_cr_free_after_normalize() {
        // Literal value-substitution + greedy splice are byte-safe only if no
        // asset carries a live `{{` token or a stray `\r` (SPEC §8.13).
        for a in [plotly(), bismark_logo(), bioinf_logo()] {
            assert!(!a.contains("{{"), "asset unexpectedly contains a {{ token");
            assert!(!a.contains('\r'), "asset unexpectedly contains a CR");
        }
    }

    #[test]
    fn embedded_assets_match_repo_plotly_files() {
        // Drift guard: the vendored `assets/` bytes embedded via `include_str!`
        // must equal the CANONICAL repo `plotly/` files (Perl's source of truth),
        // so the publishable vendored copy can't silently drift. Runs only under
        // `cargo test` (workspace present) — it reads `../../plotly` at runtime, so
        // it does NOT affect `cargo package`'s verify-build.
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plotly");
        for (name, raw) in [
            ("plotly_template.tpl", RAW_TEMPLATE),
            ("plot.ly", RAW_PLOTLY),
            ("bismark.logo", RAW_BISMARK_LOGO),
            ("bioinf.logo", RAW_BIOINF_LOGO),
        ] {
            let on_disk = std::fs::read_to_string(base.join(name)).unwrap();
            assert_eq!(on_disk, raw, "embedded {name} drifted from plotly/{name}");
        }
    }
}
