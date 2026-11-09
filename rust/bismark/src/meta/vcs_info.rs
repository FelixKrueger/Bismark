// Pure parser for the `.cargo_vcs_info.json` that `cargo package`/`cargo publish`
// embeds at a crate's root, lifted out of `build.rs` so it is covered by the
// `cargo test` harness (build scripts are not part of the test harness).
//
// `build.rs` `include!`s this file to share the exact logic; the library compiles
// it as `meta::vcs_info` purely so the unit tests below run. It is otherwise unused
// at runtime (the resolved hash reaches the binary via the `GIT_SHORT_HASH`
// build-time env const), hence the `dead_code` allow for non-test lib builds.
//
// NOTE: plain `//` (not `//!`) comments on purpose. `build.rs` `include!`s this
// file mid-module, where an inner `//!` doc comment would be a compile error.

/// Extract the short commit hash from the raw `.cargo_vcs_info.json` content. The
/// file (at the crate root) looks like
/// `{"git":{"sha1":"<40-hex>"},"path_in_vcs":"rust/bismark"}`; this is a minimal
/// hand-parse (no serde in the build graph) tolerant of surrounding whitespace.
/// Returns `None` on any malformed input, so provenance degrades safely to
/// `unknown` rather than surfacing garbage on `-V`.
///
/// The hash is pinned to 7 chars (git's default minimum for `--short`). In a large
/// repo `git rev-parse --short` can auto-lengthen past 7 for uniqueness, so a
/// registry build (this path) may report one char fewer than a git-checkout build
/// of the same commit. That is purely cosmetic: the `-V` line is not byte-gated,
/// and 7 hex chars unambiguously identify the commit for a human.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn parse_cargo_vcs_short_hash(content: &str) -> Option<String> {
    let after_key = content.split("\"sha1\"").nth(1)?;
    let after_colon = after_key.split(':').nth(1)?;
    let sha = after_colon
        .trim()
        .trim_start_matches('"')
        .split('"')
        .next()?
        .trim();
    let short: String = sha.chars().take(7).collect();
    if short.len() == 7 && short.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(short)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::parse_cargo_vcs_short_hash;

    #[test]
    fn parses_real_vcs_info() {
        // The exact shape `cargo package` writes.
        let json = r#"{"git":{"sha1":"1fac90fabc1234567890deadbeef0987654321aa"},"path_in_vcs":"rust/bismark"}"#;
        assert_eq!(parse_cargo_vcs_short_hash(json).as_deref(), Some("1fac90f"));
    }

    #[test]
    fn tolerates_whitespace_and_pretty_printing() {
        let json =
            "{\n  \"git\": {\n    \"sha1\": \"abcdef0123456789abcdef0123456789abcdef01\"\n  }\n}\n";
        assert_eq!(parse_cargo_vcs_short_hash(json).as_deref(), Some("abcdef0"));
    }

    #[test]
    fn degrades_to_none_on_malformed_input() {
        // No sha1 key, empty, junk, non-hex, and a too-short sha all → None
        // (so build.rs falls through to the `unknown` tier, never garbage).
        for bad in [
            "",
            "{}",
            "not json at all",
            r#"{"git":{"sha1":""}}"#,
            r#"{"git":{"sha1":"zzzzzzz1234"}}"#, // non-hex
            r#"{"git":{"sha1":"abc"}}"#,         // fewer than 7 chars
        ] {
            assert_eq!(
                parse_cargo_vcs_short_hash(bad),
                None,
                "expected None for malformed input {bad:?}"
            );
        }
    }
}
