//! Document assembly: byte-level helpers + the 11-step build (SPEC §2.3).
//!
//! The document is a `Vec<u8>` (not a `String`) so report-derived values — most
//! importantly `{{filename}}`, which can be non-UTF8 — round-trip byte-for-byte
//! like Perl. The embedded template/assets are ASCII (`&str`).

use crate::discovery::Job;
use crate::error::ReportError;
use crate::logging::Logger;
use crate::{assets, reports, timestamp};

/// Replace ALL non-overlapping occurrences of `needle` with `repl` (Perl `s///g`).
pub fn subst_all(doc: Vec<u8>, needle: &[u8], repl: &[u8]) -> Vec<u8> {
    if needle.is_empty() {
        return doc;
    }
    let mut out = Vec::with_capacity(doc.len());
    let mut i = 0;
    while i < doc.len() {
        if doc[i..].starts_with(needle) {
            out.extend_from_slice(repl);
            i += needle.len();
        } else {
            out.push(doc[i]);
            i += 1;
        }
    }
    out
}

pub(crate) fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    if n.is_empty() || n.len() > h.len() {
        return None;
    }
    (0..=h.len() - n.len()).find(|&i| &h[i..i + n.len()] == n)
}

pub(crate) fn rfind(h: &[u8], n: &[u8]) -> Option<usize> {
    if n.is_empty() || n.len() > h.len() {
        return None;
    }
    (0..=h.len() - n.len())
        .rev()
        .find(|&i| &h[i..i + n.len()] == n)
}

/// "Section present": remove ALL occurrences of `marker` (Perl `s/{{m}}//g`).
pub fn collapse(doc: Vec<u8>, marker: &[u8]) -> Vec<u8> {
    subst_all(doc, marker, b"")
}

/// "Section absent": greedy/dotall delete from the FIRST to the LAST occurrence
/// of `marker`, inclusive (Perl `s/{{m}}.*{{m}}//s`). Needs ≥ 2 occurrences (as
/// in the template); with fewer the Perl regex would not match → no change.
pub fn excise(doc: Vec<u8>, marker: &[u8]) -> Vec<u8> {
    match (find(&doc, marker), rfind(&doc, marker)) {
        (Some(f), Some(l)) if l > f => {
            let end = l + marker.len();
            let mut out = Vec::with_capacity(doc.len() - (end - f));
            out.extend_from_slice(&doc[..f]);
            out.extend_from_slice(&doc[end..]);
            out
        }
        _ => doc,
    }
}

/// Inject an asset: replace `{{marker}} … {{marker}}` (greedy/dotall, inclusive)
/// with `asset` (Perl `s/{{m}}.*{{m}}/$asset/s`). Errors if the marker is not
/// present at least twice — mirrors Perl's `die`.
pub fn inject_asset(doc: Vec<u8>, marker: &[u8], asset: &[u8]) -> Result<Vec<u8>, ReportError> {
    match (find(&doc, marker), rfind(&doc, marker)) {
        (Some(f), Some(l)) if l > f => {
            let end = l + marker.len();
            let mut out = Vec::with_capacity(doc.len() - (end - f) + asset.len());
            out.extend_from_slice(&doc[..f]);
            out.extend_from_slice(asset);
            out.extend_from_slice(&doc[end..]);
            Ok(out)
        }
        _ => Err(ReportError::AssetInjection(
            String::from_utf8_lossy(marker).into_owned(),
        )),
    }
}

/// Build one HTML report — the 11-step orchestration (SPEC §2.3). The mutation
/// ORDER is load-bearing.
pub fn build_report(
    job: &Job,
    test_epoch: Option<i64>,
    log: &Logger,
) -> Result<Vec<u8>, ReportError> {
    // 1. normalized template → doc bytes
    let mut doc: Vec<u8> = assets::template().as_bytes().to_vec();
    // 2-4. inject the three assets (greedy/dotall)
    doc = inject_asset(doc, b"{{plotly_goes_here}}", assets::plotly().as_bytes())?;
    log.info("Plot.ly injection successful!");
    doc = inject_asset(
        doc,
        b"{{bismark_logo_goes_here}}",
        assets::bismark_logo().as_bytes(),
    )?;
    doc = inject_asset(
        doc,
        b"{{bioinf_logo_goes_here}}",
        assets::bioinf_logo().as_bytes(),
    )?;
    // 5. timestamp
    let (date, time) = timestamp::timestamp(test_epoch);
    doc = subst_all(doc, b"{{date}}", date.as_bytes());
    doc = subst_all(doc, b"{{time}}", time.as_bytes());
    // 6. alignment (mandatory)
    log.note(&format!(
        "Using the following alignment report:\t\t> {} <",
        job.alignment.display()
    ));
    let aln = std::fs::read(&job.alignment)?;
    doc = reports::alignment::fill(doc, &reports::alignment::parse(&aln));
    // 7. deduplication (optional)
    if let Some(p) = &job.dedup {
        doc = collapse(doc, b"{{deduplication_section}}");
        let b = std::fs::read(p)?;
        doc = reports::dedup::fill(doc, &reports::dedup::parse(&b));
    } else {
        doc = excise(doc, b"{{deduplication_section}}");
    }
    // 8. splitting (optional)
    if let Some(p) = &job.splitting {
        doc = collapse(doc, b"{{cytosine_methylation_post_deduplication_section}}");
        let b = std::fs::read(p)?;
        doc = reports::splitting::fill(doc, &reports::splitting::parse(&b));
    } else {
        doc = excise(doc, b"{{cytosine_methylation_post_deduplication_section}}");
    }
    // 9. M-bias (optional) — state drives R2 section deletion; fill drives R2 data
    if let Some(p) = &job.mbias {
        doc = collapse(doc, b"{{mbias_r1_section}}");
        let b = std::fs::read(p)?;
        let parsed = reports::mbias::parse(&b);
        let state = parsed.state;
        doc = reports::mbias::fill(doc, &parsed);
        if state == reports::mbias::State::Single {
            doc = excise(doc, b"{{mbias_r2_section}}");
        } else {
            doc = collapse(doc, b"{{mbias_r2_section}}");
        }
    } else {
        doc = excise(doc, b"{{mbias_r1_section}}");
        doc = excise(doc, b"{{mbias_r2_section}}");
    }
    // 10. nucleotide coverage (optional)
    if let Some(p) = &job.nuc {
        doc = collapse(doc, b"{{nucleotide_coverage_section}}");
        let b = std::fs::read(p)?;
        let parsed = reports::nucleotide::parse(&b)?; // may Err on a bad header
        doc = reports::nucleotide::fill(doc, &parsed);
    } else {
        doc = excise(doc, b"{{nucleotide_coverage_section}}");
    }
    // 11. caller writes `doc` verbatim.
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subst_replaces_all() {
        assert_eq!(subst_all(b"a{{x}}b{{x}}".to_vec(), b"{{x}}", b"Z"), b"aZbZ");
    }

    #[test]
    fn collapse_removes_markers_keeps_content() {
        assert_eq!(collapse(b"[{{s}}KEEP{{s}}]".to_vec(), b"{{s}}"), b"[KEEP]");
    }

    #[test]
    fn excise_removes_first_to_last_inclusive() {
        assert_eq!(excise(b"[{{s}}DROP{{s}}]".to_vec(), b"{{s}}"), b"[]");
    }

    #[test]
    fn excise_single_marker_is_noop() {
        assert_eq!(excise(b"[{{s}}x]".to_vec(), b"{{s}}"), b"[{{s}}x]");
    }

    #[test]
    fn inject_replaces_span_with_asset() {
        let got = inject_asset(b"<{{m}}old{{m}}>".to_vec(), b"{{m}}", b"NEW").unwrap();
        assert_eq!(got, b"<NEW>");
    }

    #[test]
    fn inject_errors_without_two_markers() {
        assert!(inject_asset(b"<{{m}}>".to_vec(), b"{{m}}", b"NEW").is_err());
    }
}
