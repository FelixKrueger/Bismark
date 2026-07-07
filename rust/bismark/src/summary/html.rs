//! HTML report assembly — the string-templating engine.
//!
//! Reproduces Perl `bismark2summary:1376-1719` mutation-for-mutation, in
//! order (§2.9). The two riskiest behaviors are pinned here:
//!
//! - **Section deletion uses TWO independent predicates** (§2.9 ⚠ box,
//!   Reviewer A C2): the *numbers* sections key off `$dup_alignments =~
//!   /^,{1,}$/` (`:1430`); the *percentage* sections key off `if ($aligned)`
//!   (`:1577`). They diverge for a single RRBS sample.
//! - **Percentage formatting is asymmetric** (§2.9a): methylated + alignment
//!   percentages are `%.2f` verbatim; the six **unmethylated** arrays are
//!   `100 - <rounded %.2f>` re-stringified via `%.15g` ([`format_g15`]).

use crate::summary::BISMARK_VERSION;
use crate::summary::assets;
use crate::summary::error::BismarkSummaryError;
use crate::summary::fmt_g::format_g15;
use crate::summary::parse::SampleMetrics;
use crate::summary::plot::{self, num};

/// The inline HTML template, lifted verbatim from the Perl heredoc
/// (`bismark2summary:490-1371`). A drift-guard test (`tests/template_drift`)
/// re-extracts it from the Perl source and asserts byte-equality.
pub const TEMPLATE: &str = include_str!("summary_template.html");

/// Build the complete HTML report. `timestamp` is the already-formatted
/// `{{report_timestamp}}` string; `page_title` fills `{{page_title}}`.
pub fn build_html(
    samples: &[SampleMetrics],
    page_title: &str,
    timestamp: &str,
) -> Result<String, BismarkSummaryError> {
    let a = plot::assemble(samples);
    let mut doc = TEMPLATE.to_string();

    // 1. plot.ly injection — greedy/dotall first..last marker splice (`:1378`).
    if !inject_span(&mut doc, "{{plotly_goes_here}}", assets::plotly()) {
        return Err(BismarkSummaryError::PlotlyInjectionFailed);
    }
    // 2-3. logos (single subst each, `:1384-1385`).
    doc = doc.replacen("{{bismark_logo_goes_here}}", assets::bismark_logo(), 1);
    doc = doc.replacen("{{bioinf_logo_goes_here}}", assets::bioinf_logo(), 1);
    // 4-5. timestamp + page_title (`/g`, `:1386-1387`).
    doc = doc.replace("{{report_timestamp}}", timestamp);
    doc = doc.replace("{{page_title}}", page_title);
    // num_samples (`/g`, `:1391`) = TOTAL discovered count.
    doc = doc.replace("{{num_samples}}", &a.num_samples.to_string());
    // x-values 1..num_samples (`/g`, `:1393-1399`).
    let x_values = (1..=a.num_samples)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    doc = doc.replace("{{x_values_alignment}}", &x_values);
    doc = doc.replace("{{x_values_methylation}}", &x_values);
    // filenames / categories (`/g`, `:1403`).
    doc = doc.replace("{{filenames_replace}}", &a.categories.join(","));
    // bismark_version (`/g`, `:1405`) = the hardcoded constant (SPEC O1).
    doc = doc.replace("{{bismark_version}}", BISMARK_VERSION);

    // ── Alignment numbers section (`:1411-1455`) ─────────────────────────
    let mut aligned = a.aligned.join(",");
    let no_seq = a.no_seq.join(",");
    let not_aligned = a.not_aligned.join(",");
    let ambig = a.ambig.join(",");
    let mut dup_alignments = a.dup.join(",");
    let mut unique_alignments = a.unique.join(",");

    if all_commas(&aligned) {
        aligned = String::new(); // `:1412-1415`
    }
    // Fills FIRST (`:1419-1426`), before the section deletions below.
    doc = doc.replace("{{aligned_seq}}", &aligned);
    doc = doc.replace("{{no_seq}}", &no_seq);
    doc = doc.replace("{{not_aligned}}", &not_aligned);
    doc = doc.replace("{{ambig_aligned}}", &ambig);

    // Numbers section deletion gated on `$dup_alignments` (`:1430-1442`).
    if all_commas(&dup_alignments) {
        delete_span(&mut doc, "{{deduplicated_unique_reads_section}}");
        delete_span(&mut doc, "{{duplicated_reads_section}}");
        doc = doc.replace("{{raw_aligned_reads_section}}", ""); // keep raw trace
        dup_alignments = String::new();
    } else {
        delete_span(&mut doc, "{{raw_aligned_reads_section}}");
        doc = doc.replace("{{deduplicated_unique_reads_section}}", "");
        doc = doc.replace("{{duplicated_reads_section}}", "");
    }
    doc = doc.replace("{{dup_alignments}}", &dup_alignments); // `:1443`

    if all_commas(&unique_alignments) {
        unique_alignments = String::new(); // `:1447-1453`
    }
    doc = doc.replace("{{unique_alignments}}", &unique_alignments); // `:1454`

    // ── Alignment percentages (`:1458-1599`) ─────────────────────────────
    // `$aligned` truthiness (after the all-commas blanking) selects raw vs
    // dedup — a DIFFERENT predicate from the numbers section above.
    let raw_mode = !aligned.is_empty();
    let (mut p_aligned, mut p_unique, mut p_dup) = (Vec::new(), Vec::new(), Vec::new());
    let (mut p_no_seq, mut p_unal, mut p_ambig) = (Vec::new(), Vec::new(), Vec::new());

    for i in 0..a.aligned.len() {
        let total: f64 = if raw_mode {
            if a.aligned[i].is_empty() {
                // A blanked (deduplicated) sample in raw mode → mixed types.
                return Err(BismarkSummaryError::MixedSampleTypes); // `:1488-1490`
            }
            (num(&a.aligned[i]) + num(&a.no_seq[i]) + num(&a.not_aligned[i]) + num(&a.ambig[i]))
                as f64
        } else {
            (num(&a.unique[i])
                + num(&a.dup[i])
                + num(&a.no_seq[i])
                + num(&a.not_aligned[i])
                + num(&a.ambig[i])) as f64
        };

        // Perl divides by `$total` next (`:1506-1515`); a zero total is its
        // "Illegal division by zero" die. Unreachable on real data (a plotted
        // sample has alignments), but reproduce the failure rather than emit a
        // `NaN`/`inf` HTML. Raised here, AFTER the `.txt` was written.
        if total == 0.0 {
            return Err(BismarkSummaryError::ZeroAlignmentTotal);
        }

        if raw_mode {
            p_aligned.push(pct2(num(&a.aligned[i]) as f64, total));
        } else {
            p_unique.push(pct2(num(&a.unique[i]) as f64, total));
            p_dup.push(pct2(num(&a.dup[i]) as f64, total));
        }
        p_no_seq.push(pct2(num(&a.no_seq[i]) as f64, total));
        p_unal.push(pct2(num(&a.not_aligned[i]) as f64, total));
        p_ambig.push(pct2(num(&a.ambig[i]) as f64, total));
    }

    // Percentage section deletion gated on `$aligned` (`:1577-1588`).
    if raw_mode {
        delete_span(&mut doc, "{{deduplicated_unique_reads_percentage_section}}");
        delete_span(&mut doc, "{{duplicated_reads_percentage_section}}");
        doc = doc.replace("{{raw_unique_reads_percentage_section}}", "");
    } else {
        delete_span(&mut doc, "{{raw_unique_reads_percentage_section}}");
        doc = doc.replace("{{duplicated_reads_percentage_section}}", "");
        doc = doc.replace("{{deduplicated_unique_reads_percentage_section}}", "");
    }
    // Fills (single subst, `:1590-1599`).
    if raw_mode {
        doc = doc.replacen("{{p_aligned_replace}}", &p_aligned.join(","), 1);
    } else {
        doc = doc.replacen(
            "{{p_deduplicated_unique_alignments}}",
            &p_unique.join(","),
            1,
        );
        doc = doc.replacen("{{p_duplicated_alignments}}", &p_dup.join(","), 1);
    }
    doc = doc.replacen("{{p_no_seq_replace}}", &p_no_seq.join(","), 1);
    doc = doc.replacen("{{p_unal_replace}}", &p_unal.join(","), 1);
    doc = doc.replacen("{{p_ambig_replace}}", &p_ambig.join(","), 1);

    // ── Methylation raw count strings (`/g`, `:1607-1618`) ────────────────
    doc = doc.replace("{{meth_cpg_string}}", &a.meth_cpg.join(","));
    doc = doc.replace("{{unmeth_cpg_string}}", &a.unmeth_cpg.join(","));
    doc = doc.replace("{{meth_chg_string}}", &a.meth_chg.join(","));
    doc = doc.replace("{{unmeth_chg_string}}", &a.unmeth_chg.join(","));
    doc = doc.replace("{{meth_chh_string}}", &a.meth_chh.join(","));
    doc = doc.replace("{{unmeth_chh_string}}", &a.unmeth_chh.join(","));

    // ── Methylation percentages (`:1628-1711`) ───────────────────────────
    let (mut p_cpg_m, mut p_cpg_u) = (Vec::new(), Vec::new());
    let (mut p_chg_m, mut p_chg_u) = (Vec::new(), Vec::new());
    let (mut p_chh_m, mut p_chh_u) = (Vec::new(), Vec::new());

    for i in 0..a.meth_cpg.len() {
        let total_cpg = num(&a.meth_cpg[i]) + num(&a.unmeth_cpg[i]);
        let total_chg = num(&a.meth_chg[i]) + num(&a.unmeth_chg[i]);
        let total_chh = num(&a.meth_chh[i]) + num(&a.unmeth_chh[i]);

        // CpG: total 0 → "NA"/"NA"; else %.2f meth, 100 - rounded via %.15g.
        if total_cpg == 0 {
            p_cpg_m.push("NA".to_string());
            p_cpg_u.push("NA".to_string());
        } else {
            let (m, u) = meth_pair(num(&a.meth_cpg[i]), total_cpg);
            p_cpg_m.push(m);
            p_cpg_u.push(u);
        }
        // CHG: total 0 → "0"/"0".
        if total_chg == 0 {
            p_chg_m.push("0".to_string());
            p_chg_u.push("0".to_string());
        } else {
            let (m, u) = meth_pair(num(&a.meth_chg[i]), total_chg);
            p_chg_m.push(m);
            p_chg_u.push(u);
        }
        // CHH: Perl BUG — tests `total_CHG`, not `total_CHH` (`:1662`).
        // Reproduce verbatim (dead for plotted samples: exclusion guarantees
        // all three context totals > 0).
        if total_chg == 0 {
            p_chh_m.push("0".to_string());
            p_chh_u.push("0".to_string());
        } else {
            let (m, u) = meth_pair(num(&a.meth_chh[i]), total_chh);
            p_chh_m.push(m);
            p_chh_u.push(u);
        }
    }
    doc = doc.replacen("{{p_CpG_m_replace}}", &p_cpg_m.join(","), 1);
    doc = doc.replacen("{{p_CpG_u_replace}}", &p_cpg_u.join(","), 1);
    doc = doc.replacen("{{p_CHG_m_replace}}", &p_chg_m.join(","), 1);
    doc = doc.replacen("{{p_CHG_u_replace}}", &p_chg_u.join(","), 1);
    doc = doc.replacen("{{p_CHH_m_replace}}", &p_chh_m.join(","), 1);
    doc = doc.replacen("{{p_CHH_u_replace}}", &p_chh_u.join(","), 1);

    Ok(doc)
}

/// The methylated/unmethylated percentage pair for one context (§2.9a):
/// `meth%` = `sprintf("%.2f", meth/total*100)` (verbatim, keeps `.00`);
/// `unmeth%` = `100 - <that rounded value>` stringified via C `%.15g`
/// (drops trailing zeros). `total` is guaranteed > 0 by the caller.
fn meth_pair(meth: i64, total: i64) -> (String, String) {
    let m = pct2(meth as f64, total as f64);
    let u = format_g15(100.0 - m.parse::<f64>().unwrap_or(0.0));
    (m, u)
}

/// Perl `sprintf("%.2f", part/total*100)`. Callers guarantee `total > 0` for
/// all reachable (plotted) inputs.
fn pct2(part: f64, total: f64) -> String {
    format!("{:.2}", part / total * 100.0)
}

/// Perl `$x =~ /^,{1,}$/`: true iff `s` is non-empty and entirely commas.
fn all_commas(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b == b',')
}

/// Replace `marker.*marker` (greedy, dotall) with `replacement` — Perl
/// `s/marker.*marker/replacement/s`. Needs ≥2 occurrences (like Perl's
/// `m.*m`); returns `false` if the marker is absent or appears only once.
fn inject_span(doc: &mut String, marker: &str, replacement: &str) -> bool {
    let (Some(first), Some(last)) = (doc.find(marker), doc.rfind(marker)) else {
        return false;
    };
    if last == first {
        return false;
    }
    let end = last + marker.len();
    doc.replace_range(first..end, replacement);
    true
}

/// Delete `marker.*marker` (greedy, dotall) — Perl `s/marker.*marker//s`.
/// Needs ≥2 occurrences; no-op otherwise.
fn delete_span(doc: &mut String, marker: &str) {
    let (Some(first), Some(last)) = (doc.find(marker), doc.rfind(marker)) else {
        return;
    };
    if last == first {
        return;
    }
    let end = last + marker.len();
    doc.replace_range(first..end, "");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_commas_needs_at_least_one_comma() {
        assert!(!all_commas("")); // empty → false (N=1 join)
        assert!(all_commas(","));
        assert!(all_commas(",,,"));
        assert!(!all_commas("400"));
        assert!(!all_commas(",400"));
    }

    #[test]
    fn span_helpers_handle_two_markers() {
        let mut s = String::from("A{{m}}xxx{{m}}B");
        delete_span(&mut s, "{{m}}");
        assert_eq!(s, "AB");

        let mut s2 = String::from("A{{m}}xxx{{m}}B");
        assert!(inject_span(&mut s2, "{{m}}", "ZZ"));
        assert_eq!(s2, "AZZB");
    }

    #[test]
    fn span_helpers_noop_on_single_marker() {
        // Perl's `m.*m` requires two markers; a lone marker is left intact.
        let mut s = String::from("A{{m}}B");
        delete_span(&mut s, "{{m}}");
        assert_eq!(s, "A{{m}}B");
        let mut s2 = String::from("A{{m}}B");
        assert!(!inject_span(&mut s2, "{{m}}", "ZZ"));
    }

    #[test]
    fn pct2_matches_sprintf_two_dp() {
        assert_eq!(pct2(1.0, 3.0), "33.33");
        assert_eq!(pct2(8000.0, 10000.0), "80.00");
        assert_eq!(pct2(1.0, 8.0), "12.50");
    }

    #[test]
    fn meth_pair_is_asymmetric() {
        // meth keeps %.2f; unmeth drops trailing zeros (100 - rounded, %.15g).
        assert_eq!(meth_pair(1, 2), ("50.00".into(), "50".into()));
        assert_eq!(meth_pair(1, 1), ("100.00".into(), "0".into()));
        // 9000/89000 → 10.11; 100-10.11 = 89.89
        assert_eq!(meth_pair(9000, 89000), ("10.11".into(), "89.89".into()));
    }

    #[test]
    fn zero_alignment_total_errors_like_perl_die() {
        // Raw mode (no dedup), every read category zero, but methylation
        // present → plotted → alignment total 0. Perl dies ("Illegal division
        // by zero"); Rust returns ZeroAlignmentTotal instead of NaN/inf HTML.
        let mut m = SampleMetrics::new("z_bismark_bt2.bam");
        m.aligned_reads = "0".into();
        m.unaligned = "0".into();
        m.ambig_reads = "0".into();
        m.no_seq_reads = "0".into();
        m.meth_cpg = "1".into();
        m.unmeth_cpg = "1".into();
        m.meth_chg = "1".into();
        m.unmeth_chg = "1".into();
        m.meth_chh = "1".into();
        m.unmeth_chh = "1".into();
        let err = build_html(&[m], "T", "Mon Jun  1 00:00:00 2026").unwrap_err();
        assert!(matches!(err, BismarkSummaryError::ZeroAlignmentTotal));
    }

    #[test]
    fn plotly_injects_and_no_section_markers_survive_on_a_basic_render() {
        let mut m = SampleMetrics::new("s_bismark_bt2.bam");
        m.aligned_reads = "800".into();
        m.dup_reads = "200".into();
        m.unique_reads = "600".into();
        m.unaligned = "150".into();
        m.ambig_reads = "50".into();
        m.no_seq_reads = "0".into();
        m.meth_cpg = "9000".into();
        m.unmeth_cpg = "80000".into();
        m.meth_chg = "900".into();
        m.unmeth_chg = "40000".into();
        m.meth_chh = "1800".into();
        m.unmeth_chh = "300000".into();
        // two dedup samples → consistent dedup mode
        let html = build_html(&[m.clone(), m], "Test", "Mon Jun  1 00:00:00 2026").unwrap();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.ends_with("</html>\n"));
        // plot.ly injected (the inline 3 MB library token).
        assert!(html.contains("Plotly"));
        // No template placeholders / section markers survive a complete render.
        assert!(!html.contains("{{plotly_goes_here}}"));
        assert!(!html.contains("_section}}"));
        assert!(!html.contains("{{report_timestamp}}"));
        assert!(!html.contains("{{p_CpG_u_replace}}"));
    }
}
