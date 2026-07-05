//! The four embedded Plotly assets + the faithful `read_report_template`
//! line-normalizer (SPEC §2.6 / §8.2).
//!
//! Embedding strategy (PLAN A5): `include_str!` via `CARGO_MANIFEST_DIR` so the
//! bytes come from the repo's `plotly/` directory at compile time (no 3 MB
//! duplication into the crate); the inline `embedded_assets_match_repo_plotly_files` test asserts the
//! embedded bytes still equal the on-disk files. No workspace crate embeds
//! assets, so this is a new pattern — *not* a genomeprep mirror.

use std::sync::OnceLock;

/// Raw `plotly_template.tpl` (the `{{placeholder}}` HTML skeleton).
const RAW_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../plotly/plotly_template.tpl"
));
/// Raw `plot.ly` (~3 MB plotly.js v1.48.3, already wrapped in `<script>`).
const RAW_PLOTLY: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../plotly/plot.ly"));
/// Raw `bismark.logo` (base64 `<img>` tag).
const RAW_BISMARK_LOGO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../plotly/bismark.logo"
));
/// Raw `bioinf.logo` (base64 `<img>` tag).
const RAW_BIOINF_LOGO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../plotly/bioinf.logo"
));

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
        // Drift guard (PLAN A5): the `include_str!`-embedded bytes must equal the
        // on-disk `plotly/` files, so the embed can't silently go stale.
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
