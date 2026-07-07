//! M-bias report parser + fill (SPEC §2.7d) — the trickiest section.
//!
//! THREE distinct facts (PLAN B5 / §8.4):
//! 1. `state` (header-derived: `paired` iff an `R2` context header is seen)
//!    drives the R2 **section** deletion in `template::build_report`.
//! 2. R2 **fill** runs only if R2 *data rows* exist (`has_r2_data`), which can
//!    diverge from `state`.
//! 3. The `{{mbias1_*}}` / `{{mbias2_*}}` data placeholders live in trailing
//!    `<script>` blocks OUTSIDE the deletable section spans, so any unfilled
//!    ones survive literally (all 24 with no report; the 12 `{{mbias2_*}}` for
//!    every SE sample).

use std::collections::HashMap;

use crate::report::reports::{join_with, report_lines, split_tab};
use crate::report::template::subst_all;

/// Single-end (R1 only) vs paired-end (an R2 header was seen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Single,
    Paired,
}

#[derive(Debug, Default)]
struct Series {
    perc_x: Vec<Vec<u8>>,
    perc_y: Vec<Vec<u8>>,
    cov_x: Vec<Vec<u8>>,
    cov_y: Vec<Vec<u8>>,
}

/// Captured M-bias series, keyed by context bytes (`CpG`/`CHG`/`CHH`).
#[derive(Debug)]
pub struct Mbias {
    pub state: State,
    r1: HashMap<Vec<u8>, Series>,
    r2: HashMap<Vec<u8>, Series>,
}

const CONTEXTS: [&[u8]; 3] = [b"CpG", b"CHG", b"CHH"];

/// Parse an M-bias report (`bismark2report:902-938`).
pub fn parse(data: &[u8]) -> Mbias {
    let mut r1: HashMap<Vec<u8>, Series> = HashMap::new();
    let mut r2: HashMap<Vec<u8>, Series> = HashMap::new();
    let mut context: Option<Vec<u8>> = None;
    let mut read_identity: u8 = 0;
    let mut state = State::Single;

    for line in report_lines(data) {
        // Context header: `^(C.{2}) context` — C + any 2 bytes + " context".
        if line.len() >= 3 && line[0] == b'C' && line[3..].starts_with(b" context") {
            context = Some(line[0..3].to_vec());
            if line.windows(2).any(|w| w == b"R2") {
                read_identity = 2;
                state = State::Paired;
            } else {
                read_identity = 1;
            }
        }
        // Data row: `^\d`.
        if line.first().is_some_and(u8::is_ascii_digit) {
            let Some(ctx) = &context else { continue };
            let f = split_tab(line); // pos, meth, unmeth, perc, coverage
            let pos = f.first().copied().unwrap_or(b"");
            let perc = f.get(3).copied().unwrap_or(b"");
            let coverage = f.get(4).copied().unwrap_or(b"");
            let series = match read_identity {
                1 => r1.entry(ctx.clone()).or_default(),
                2 => r2.entry(ctx.clone()).or_default(),
                _ => continue, // read identity unknown — Perl warns and skips
            };
            series.perc_x.push(pos.to_vec());
            series.perc_y.push(perc.to_vec());
            series.cov_x.push(pos.to_vec());
            series.cov_y.push(coverage.to_vec());
        }
    }

    Mbias { state, r1, r2 }
}

fn join_series(v: &[Vec<u8>]) -> Vec<u8> {
    let refs: Vec<&[u8]> = v.iter().map(Vec::as_slice).collect();
    join_with(&refs, b",")
}

/// Fill the M-bias data placeholders (Perl 940-1017). R1 is always filled (empty
/// joins allowed); R2 only if data rows exist. Unfilled placeholders survive.
pub fn fill(mut doc: Vec<u8>, m: &Mbias) -> Vec<u8> {
    doc = fill_read(doc, &m.r1, 1);
    if !m.r2.is_empty() {
        doc = fill_read(doc, &m.r2, 2);
    } else {
        // Perl `s/{{bm_mbias_2}}/false/g` — a no-op against the current template
        // (which has no such placeholder); reproduced for faithfulness.
        doc = subst_all(doc, b"{{bm_mbias_2}}", b"false");
    }
    doc
}

fn fill_read(mut doc: Vec<u8>, series: &HashMap<Vec<u8>, Series>, read: u8) -> Vec<u8> {
    let empty = Series::default();
    for ctx in CONTEXTS {
        let s = series.get(ctx).unwrap_or(&empty);
        let pre = format!("{{{{mbias{read}_{}_", std::str::from_utf8(ctx).unwrap());
        // meth_x, meth_y, coverage_x, coverage_y (Perl order)
        doc = subst_all(
            doc,
            format!("{pre}meth_x}}}}").as_bytes(),
            &join_series(&s.perc_x),
        );
        doc = subst_all(
            doc,
            format!("{pre}meth_y}}}}").as_bytes(),
            &join_series(&s.perc_y),
        );
        doc = subst_all(
            doc,
            format!("{pre}coverage_x}}}}").as_bytes(),
            &join_series(&s.cov_x),
        );
        doc = subst_all(
            doc,
            format!("{pre}coverage_y}}}}").as_bytes(),
            &join_series(&s.cov_y),
        );
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn se_state_fills_r1_only_r2_placeholders_survive() {
        let data = b"CpG context\n===\n1\t10\t5\t66.67\t15\n2\t12\t4\t75.00\t16\n";
        let m = parse(data);
        assert_eq!(m.state, State::Single);
        let doc = fill(
            b"[{{mbias1_CpG_meth_x}}|{{mbias1_CpG_meth_y}}|{{mbias2_CpG_meth_x}}]".to_vec(),
            &m,
        );
        // meth_x = positions, meth_y = % methylation; R2 placeholder survives.
        assert_eq!(doc, b"[1,2|66.67,75.00|{{mbias2_CpG_meth_x}}]");
    }

    #[test]
    fn pe_state_fills_both_reads() {
        let data = b"CpG context (R1)\n1\t10\t5\t66.67\t15\nCpG context (R2)\n1\t9\t6\t60.00\t15\n";
        let m = parse(data);
        assert_eq!(m.state, State::Paired);
        let doc = fill(
            b"[{{mbias1_CpG_meth_y}}|{{mbias2_CpG_meth_y}}]".to_vec(),
            &m,
        );
        assert_eq!(doc, b"[66.67|60.00]");
    }

    #[test]
    fn coverage_series_uses_coverage_column() {
        let data = b"CpG context\n1\t10\t5\t66.67\t15\n";
        let m = parse(data);
        let doc = fill(
            b"[{{mbias1_CpG_coverage_x}}|{{mbias1_CpG_coverage_y}}]".to_vec(),
            &m,
        );
        assert_eq!(doc, b"[1|15]");
    }
}
