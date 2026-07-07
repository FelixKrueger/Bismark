//! Methylation-extractor splitting report parser + fill (SPEC §2.7c).
//!
//! Like the alignment context block but writes the `*_splitting` placeholders,
//! and the phrasing differs: unmethylated is **only** `Total C to T conversions
//! in … context:` (no `Total unmethylated …` alternate), and the Unknown
//! percentage line is `C methylated in Unknown context:` (no `(CN or CHN)`).

use crate::report::reports::{
    field1_owned, graph_value, join_with, report_lines, strip_first_percent, unknown_tr,
};
use crate::report::template::subst_all;

/// Captured splitting-report context-methylation fields.
#[derive(Debug, Default)]
pub struct Splitting {
    pub total_c_count: Option<Vec<u8>>,
    pub meth_cpg: Option<Vec<u8>>,
    pub meth_chg: Option<Vec<u8>>,
    pub meth_chh: Option<Vec<u8>>,
    pub meth_unknown: Option<Vec<u8>>,
    pub unmeth_cpg: Option<Vec<u8>>,
    pub unmeth_chg: Option<Vec<u8>>,
    pub unmeth_chh: Option<Vec<u8>>,
    pub unmeth_unknown: Option<Vec<u8>>,
    pub perc_cpg: Option<Vec<u8>>,
    pub perc_chg: Option<Vec<u8>>,
    pub perc_chh: Option<Vec<u8>>,
    pub perc_unknown: Option<Vec<u8>>,
}

/// Parse a splitting report (`bismark2report:719-782`).
pub fn parse(data: &[u8]) -> Splitting {
    let mut s = Splitting::default();
    for line in report_lines(data) {
        if line.starts_with(b"Total number of C") {
            s.total_c_count = field1_owned(line);
        } else if line.starts_with(b"Total methylated C's in CpG context:") {
            s.meth_cpg = field1_owned(line);
        } else if line.starts_with(b"Total methylated C's in CHG context:") {
            s.meth_chg = field1_owned(line);
        } else if line.starts_with(b"Total methylated C's in CHH context:") {
            s.meth_chh = field1_owned(line);
        } else if line.starts_with(b"Total methylated C's in Unknown context:") {
            s.meth_unknown = field1_owned(line);
        } else if line.starts_with(b"Total C to T conversions in CpG context:") {
            s.unmeth_cpg = field1_owned(line);
        } else if line.starts_with(b"Total C to T conversions in CHG context:") {
            s.unmeth_chg = field1_owned(line);
        } else if line.starts_with(b"Total C to T conversions in CHH context:") {
            s.unmeth_chh = field1_owned(line);
        } else if line.starts_with(b"Total C to T conversions in Unknown context:") {
            s.unmeth_unknown = field1_owned(line);
        } else if line.starts_with(b"C methylated in CpG context:") {
            s.perc_cpg = field1_owned(line).map(|x| strip_first_percent(&x));
        } else if line.starts_with(b"C methylated in CHG context:") {
            s.perc_chg = field1_owned(line).map(|x| strip_first_percent(&x));
        } else if line.starts_with(b"C methylated in CHH context:") {
            s.perc_chh = field1_owned(line).map(|x| strip_first_percent(&x));
        } else if line.starts_with(b"C methylated in Unknown context:") {
            s.perc_unknown = field1_owned(line).map(|x| strip_first_percent(&x));
        }
    }
    s
}

/// Fill the splitting placeholders in Perl order (`bismark2report:788-876`).
/// Gate = `is_some()` on the 6 meth/unmeth fields (Perl `defined`).
pub fn fill(mut doc: Vec<u8>, s: &Splitting) -> Vec<u8> {
    if !(s.meth_cpg.is_some()
        && s.meth_chg.is_some()
        && s.meth_chh.is_some()
        && s.unmeth_cpg.is_some()
        && s.unmeth_chg.is_some()
        && s.unmeth_chh.is_some())
    {
        return doc;
    }
    let o = |x: &Option<Vec<u8>>| -> Vec<u8> { x.as_deref().unwrap_or(b"").to_vec() };

    doc = subst_all(doc, b"{{total_C_count_splitting}}", &o(&s.total_c_count));

    doc = subst_all(doc, b"{{meth_CpG_splitting}}", &o(&s.meth_cpg));
    doc = subst_all(doc, b"{{meth_CHG_splitting}}", &o(&s.meth_chg));
    doc = subst_all(doc, b"{{meth_CHH_splitting}}", &o(&s.meth_chh));

    doc = subst_all(doc, b"{{unmeth_CpG_splitting}}", &o(&s.unmeth_cpg));
    doc = subst_all(doc, b"{{unmeth_CHG_splitting}}", &o(&s.unmeth_chg));
    doc = subst_all(doc, b"{{unmeth_CHH_splitting}}", &o(&s.unmeth_chh));

    let perc_cpg_disp = s.perc_cpg.as_deref().unwrap_or(b"N/A");
    let perc_chg_disp = s.perc_chg.as_deref().unwrap_or(b"N/A");
    let perc_chh_disp = s.perc_chh.as_deref().unwrap_or(b"N/A");
    let perc_unknown_disp = s.perc_unknown.as_deref().unwrap_or(b"N/A");

    let (meth_u, unmeth_u, perc_u): (Vec<u8>, Vec<u8>, Vec<u8>) = if s.meth_unknown.is_some() {
        (
            unknown_tr(
                b"Methylated C's in Unknown context",
                &o(&s.meth_unknown),
                b"",
            ),
            unknown_tr(
                b"Unmethylated C's in Unknown context",
                &o(&s.unmeth_unknown),
                b"",
            ),
            unknown_tr(
                b"Methylated C's in Unknown context",
                perc_unknown_disp,
                b"%",
            ),
        )
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };
    doc = subst_all(doc, b"{{meth_unknown_splitting}}", &meth_u);
    doc = subst_all(doc, b"{{unmeth_unknown_splitting}}", &unmeth_u);
    doc = subst_all(doc, b"{{perc_unknown_splitting}}", &perc_u);

    // Post-deduplication methylation bar ("N/A" → "0" in the graph only).
    let cyto = join_with(
        &[
            graph_value(perc_cpg_disp),
            graph_value(perc_chg_disp),
            graph_value(perc_chh_disp),
        ],
        b",",
    );
    doc = subst_all(
        doc,
        b"{{cytosine_methylation_post_duplication_plotly}}",
        &cyto,
    );

    doc = subst_all(doc, b"{{perc_CpG_splitting}}", perc_cpg_disp);
    doc = subst_all(doc, b"{{perc_CHG_splitting}}", perc_chg_disp);
    doc = subst_all(doc, b"{{perc_CHH_splitting}}", perc_chh_disp);

    doc
}
