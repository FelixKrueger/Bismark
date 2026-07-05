//! Output-filename derivation matching Perl `bismark2bedGraph` v0.25.1.
//!
//! All four derivations operate on the **normalized** bedGraph output name
//! (the `-o` value with `.gz` guaranteed appended). The Perl quirks here
//! are byte-identity-affecting and are replicated verbatim — including the
//! `.zero.cov` filename that is almost certainly a latent Perl bug (SPEC
//! §4.3). Do NOT "fix" these.

/// Normalize the user's `-o`/`--output` value: append `.gz` unless already
/// present. Perl `bismark2bedGraph:733-735`.
///
/// ```
/// use bismark_bedgraph::filename::normalize_bedgraph_name;
/// assert_eq!(normalize_bedgraph_name("foo.bedGraph"), "foo.bedGraph.gz");
/// assert_eq!(normalize_bedgraph_name("foo.bedGraph.gz"), "foo.bedGraph.gz");
/// assert_eq!(normalize_bedgraph_name("sample"), "sample.gz");
/// ```
#[must_use]
pub fn normalize_bedgraph_name(output: &str) -> String {
    if output.ends_with(".gz") {
        output.to_string()
    } else {
        format!("{output}.gz")
    }
}

/// Derive the coverage filename from the normalized bedGraph name.
/// Perl `bismark2bedGraph:118-121`:
/// `s/bedGraph\.gz$/bismark.cov.gz/` else append `.bismark.cov.gz`.
///
/// ```
/// use bismark_bedgraph::filename::coverage_name;
/// assert_eq!(coverage_name("foo.bedGraph.gz"), "foo.bismark.cov.gz");
/// // No `bedGraph.gz` suffix → fallback append (Reviewer B's -o sample case):
/// assert_eq!(coverage_name("sample.gz"), "sample.gz.bismark.cov.gz");
/// ```
#[must_use]
pub fn coverage_name(bedgraph_name: &str) -> String {
    match bedgraph_name.strip_suffix("bedGraph.gz") {
        Some(prefix) => format!("{prefix}bismark.cov.gz"),
        None => format!("{bedgraph_name}.bismark.cov.gz"),
    }
}

/// Derive the zero-based coverage filename from the normalized bedGraph
/// name. Perl `bismark2bedGraph:126-133`: `s/bedGraph$/bismark.zero.cov/`
/// else append `.bismark.zero.cov`.
///
/// ⚠️ Because the normalized name **always ends in `.gz`**, the
/// `bedGraph$` (end-anchored) match never fires in practice, so this
/// always appends → `{bedgraph_name}.bismark.zero.cov`
/// (e.g. `foo.bedGraph.gz.bismark.zero.cov`). This is a latent Perl
/// filename quirk reproduced for byte-identity (SPEC §4.3). The full
/// conditional is implemented so the function is also correct if ever
/// called on a non-normalized name.
#[must_use]
pub fn zero_name(bedgraph_name: &str) -> String {
    match bedgraph_name.strip_suffix("bedGraph") {
        Some(prefix) => format!("{prefix}bismark.zero.cov"),
        None => format!("{bedgraph_name}.bismark.zero.cov"),
    }
}

/// Derive the UCSC bedGraph filename from the normalized bedGraph name.
/// Perl `bismark2bedGraph:524-526`: strip trailing `.gz`, append
/// `_UCSC.bedGraph.gz`.
///
/// ```
/// use bismark_bedgraph::filename::ucsc_name;
/// assert_eq!(ucsc_name("foo.bedGraph.gz"), "foo.bedGraph_UCSC.bedGraph.gz");
/// assert_eq!(ucsc_name("sample.gz"), "sample_UCSC.bedGraph.gz");
/// ```
#[must_use]
pub fn ucsc_name(bedgraph_name: &str) -> String {
    let base = bedgraph_name.strip_suffix(".gz").unwrap_or(bedgraph_name);
    format!("{base}_UCSC.bedGraph.gz")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── normalize ────────────────────────────────────────────────────
    #[test]
    fn normalize_appends_gz_when_absent() {
        assert_eq!(normalize_bedgraph_name("foo.bedGraph"), "foo.bedGraph.gz");
    }

    #[test]
    fn normalize_leaves_existing_gz() {
        assert_eq!(
            normalize_bedgraph_name("foo.bedGraph.gz"),
            "foo.bedGraph.gz"
        );
    }

    #[test]
    fn normalize_no_token_appends_gz() {
        // Reviewer B's -o sample case: normalizes to sample.gz.
        assert_eq!(normalize_bedgraph_name("sample"), "sample.gz");
    }

    // ── coverage ─────────────────────────────────────────────────────
    #[test]
    fn coverage_replaces_bedgraph_gz_suffix() {
        assert_eq!(coverage_name("foo.bedGraph.gz"), "foo.bismark.cov.gz");
    }

    #[test]
    fn coverage_fallback_append_for_no_token() {
        // -o sample → sample.gz → coverage regex fails → fallback append.
        assert_eq!(coverage_name("sample.gz"), "sample.gz.bismark.cov.gz");
    }

    // ── zero ─────────────────────────────────────────────────────────
    #[test]
    fn zero_always_appends_for_normalized_names() {
        // Normalized name ends `.gz`, so `bedGraph$` never matches → append.
        assert_eq!(
            zero_name("foo.bedGraph.gz"),
            "foo.bedGraph.gz.bismark.zero.cov"
        );
        assert_eq!(zero_name("sample.gz"), "sample.gz.bismark.zero.cov");
    }

    #[test]
    fn zero_conditional_branch_fires_on_bare_bedgraph() {
        // Defensive: the `bedGraph$` branch only fires on a non-normalized
        // name ending exactly in `bedGraph`.
        assert_eq!(zero_name("foo.bedGraph"), "foo.bismark.zero.cov");
    }

    // ── ucsc ─────────────────────────────────────────────────────────
    #[test]
    fn ucsc_strips_gz_and_appends() {
        assert_eq!(
            ucsc_name("foo.bedGraph.gz"),
            "foo.bedGraph_UCSC.bedGraph.gz"
        );
    }

    #[test]
    fn ucsc_no_token_case() {
        assert_eq!(ucsc_name("sample.gz"), "sample_UCSC.bedGraph.gz");
    }

    // ── full chain for the two headline inputs ───────────────────────
    #[test]
    fn full_chain_foo_bedgraph() {
        let bg = normalize_bedgraph_name("foo.bedGraph");
        assert_eq!(bg, "foo.bedGraph.gz");
        assert_eq!(coverage_name(&bg), "foo.bismark.cov.gz");
        assert_eq!(zero_name(&bg), "foo.bedGraph.gz.bismark.zero.cov");
        assert_eq!(ucsc_name(&bg), "foo.bedGraph_UCSC.bedGraph.gz");
    }

    #[test]
    fn full_chain_o_sample_no_token() {
        let bg = normalize_bedgraph_name("sample");
        assert_eq!(bg, "sample.gz");
        assert_eq!(coverage_name(&bg), "sample.gz.bismark.cov.gz");
        assert_eq!(zero_name(&bg), "sample.gz.bismark.zero.cov");
        assert_eq!(ucsc_name(&bg), "sample_UCSC.bedGraph.gz");
    }
}
