//! Tripwire: the checked-in `docs/images/bismark_summary_report.html` is the
//! STALE v0.15.2 Highcharts-era report (zero `Plotly` tokens). The acceptance
//! oracle must be a fresh run of the current Perl `bismark2summary` — never
//! this file. This test fails loudly if that committed HTML ever gains a
//! `Plotly` token (i.e. someone mistakes a current render for the stale
//! oracle, or refreshes it in place), per SPEC §7 / Reviewer B 4.5.
//!
//! Auto-skips if the file is absent (e.g. a sparse checkout).

use std::path::Path;

#[test]
fn committed_docs_html_is_the_stale_highcharts_oracle_not_current() {
    let html =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/images/bismark_summary_report.html");
    if !html.exists() {
        eprintln!("skipping: {} absent", html.display());
        return;
    }
    let content = std::fs::read_to_string(&html).unwrap();
    let plotly = content.matches("Plotly").count();
    assert_eq!(
        plotly, 0,
        "docs/images/bismark_summary_report.html now contains {plotly} `Plotly` token(s) — \
         it is the STALE Highcharts oracle and must NOT be used/refreshed as the current \
         byte-identity reference (regenerate the oracle from current Perl instead)"
    );
}
