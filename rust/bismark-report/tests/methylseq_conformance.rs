//! methylseq CLI-surface conformance — `BISMARK_REPORT` (nf-core/methylseq **4.2.0**).
//!
//! Asserts the Rust `bismark2report` CLI accepts the command methylseq's report
//! module emits. See `plans/06122026_methylseq-cli-conformance/`.
//!
//! **Command template** (`modules/nf-core/bismark/report/main.nf`;
//! `conf/modules/bismark_report.config` sets `ext.args = ''`):
//! ```text
//!   bismark2report
//! ```
//! methylseq passes NO flags — `bismark2report` auto-discovers the staged report
//! files in the work dir. So the conformance assertion is simply that the bare
//! invocation parses (all flags must be optional).
//!
//! **Tiers:** Tier 1 parse only (`bismark-report` has no `Cli::validate()`).
//!
//! Re-scout on any methylseq version bump (pinned 4.2.0).

use bismark_report::cli::Cli;
use clap::Parser;

#[test]
fn methylseq_report_bare_invocation_parses() {
    let argv = ["bismark2report"];
    assert!(
        Cli::try_parse_from(argv).is_ok(),
        "BISMARK_REPORT runs `bismark2report` with no args — the bare invocation must \
         parse (all flags optional); a rejection here = a methylseq drop-in gap"
    );
}
