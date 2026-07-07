//! Alignment report parser + fill (SPEC §2.7a). The mandatory data source.

use crate::report::reports::{
    field1_owned, graph_value, join_with, report_lines, strip_first_percent, unknown_tr,
};
use crate::report::template::{rfind, subst_all};

/// Captured alignment-report fields (raw bytes, verbatim from the report).
#[derive(Debug, Default)]
pub struct Alignment {
    pub total_seqs: Option<Vec<u8>>,
    pub total_seq_text: Option<&'static [u8]>,
    pub unique: Option<Vec<u8>>,
    pub unique_text: Option<&'static [u8]>,
    pub no_aln: Option<Vec<u8>>,
    pub no_aln_text: Option<&'static [u8]>,
    pub multiple: Option<Vec<u8>>,
    pub multiple_text: Option<&'static [u8]>,
    pub no_genomic: Option<Vec<u8>>,
    pub input_filename: Option<Vec<u8>>,
    pub bismark_version: Option<Vec<u8>>,
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
    pub number_ot: Option<Vec<u8>>,
    pub number_ctot: Option<Vec<u8>>,
    pub number_ctob: Option<Vec<u8>>,
    pub number_ob: Option<Vec<u8>>,
}

/// Parse the alignment report. The branch chain mirrors Perl's `if/elsif` order
/// (`bismark2report:211-376`) — first match wins. PE vs SE is decided purely by
/// line text, which also sets the human `*_text` labels (byte-load-bearing).
pub fn parse(data: &[u8]) -> Alignment {
    let mut a = Alignment::default();
    for line in report_lines(data) {
        if line.starts_with(b"Sequence pairs analysed in total:") {
            a.total_seqs = field1_owned(line);
            a.total_seq_text = Some(b"Sequence pairs analysed in total");
        } else if line.starts_with(b"Sequences analysed in total:") {
            a.total_seqs = field1_owned(line);
            a.total_seq_text = Some(b"Sequences analysed in total");
        } else if line.starts_with(b"Bismark report for: ") {
            parse_report_for(line, &mut a);
        } else if line.starts_with(b"Number of paired-end alignments with a unique best hit:") {
            a.unique = field1_owned(line);
            a.unique_text = Some(b"Paired-end alignments with a unique best hit");
        } else if line.starts_with(b"Number of alignments with a unique best hit from") {
            a.unique = field1_owned(line);
            a.unique_text = Some(b"Single-end alignments with a unique best hit");
        } else if line.starts_with(b"Sequence pairs with no alignments under any condition:") {
            a.no_aln = field1_owned(line);
            a.no_aln_text = Some(b"Pairs without alignments under any condition");
        } else if line.starts_with(b"Sequences with no alignments under any condition:") {
            a.no_aln = field1_owned(line);
            a.no_aln_text = Some(b"Sequences without alignments under any condition");
        } else if line.starts_with(b"Sequence pairs did not map uniquely:") {
            a.multiple = field1_owned(line);
            a.multiple_text = Some(b"Pairs that did not map uniquely");
        } else if line.starts_with(b"Sequences did not map uniquely:") {
            a.multiple = field1_owned(line);
            a.multiple_text = Some(b"Sequences that did not map uniquely");
        } else if line.starts_with(
            b"Sequence pairs which were discarded because genomic sequence could not be extracted:",
        ) || line.starts_with(
            b"Sequences which were discarded because genomic sequence could not be extracted:",
        ) {
            // PE || SE — same field
            a.no_genomic = field1_owned(line);
        } else if line.starts_with(b"Total number of C") {
            a.total_c_count = field1_owned(line);
        } else if line.starts_with(b"Total methylated C's in CpG context:") {
            a.meth_cpg = field1_owned(line);
        } else if line.starts_with(b"Total methylated C's in CHG context:") {
            a.meth_chg = field1_owned(line);
        } else if line.starts_with(b"Total methylated C's in CHH context:") {
            a.meth_chh = field1_owned(line);
        } else if line.starts_with(b"Total methylated C's in Unknown context:") {
            a.meth_unknown = field1_owned(line);
        } else if line.starts_with(b"Total unmethylated C's in CpG context:")
            || line.starts_with(b"Total C to T conversions in CpG context:")
        {
            a.unmeth_cpg = field1_owned(line);
        } else if line.starts_with(b"Total unmethylated C's in CHG context:")
            || line.starts_with(b"Total C to T conversions in CHG context:")
        {
            a.unmeth_chg = field1_owned(line);
        } else if line.starts_with(b"Total unmethylated C's in CHH context:")
            || line.starts_with(b"Total C to T conversions in CHH context:")
        {
            a.unmeth_chh = field1_owned(line);
        } else if line.starts_with(b"Total unmethylated C's in Unknown context:")
            || line.starts_with(b"Total C to T conversions in Unknown context:")
        {
            a.unmeth_unknown = field1_owned(line);
        } else if line.starts_with(b"C methylated in CpG context:") {
            a.perc_cpg = field1_owned(line).map(|x| strip_first_percent(&x));
        } else if line.starts_with(b"C methylated in CHG context:") {
            a.perc_chg = field1_owned(line).map(|x| strip_first_percent(&x));
        } else if line.starts_with(b"C methylated in CHH context:") {
            a.perc_chh = field1_owned(line).map(|x| strip_first_percent(&x));
        } else if line.starts_with(b"C methylated in Unknown context (CN or CHN):") {
            a.perc_unknown = field1_owned(line).map(|x| strip_first_percent(&x));
        // Strand origin — each branch is PE-pattern || SE-pattern (same field).
        // The trailing `:` keeps the patterns mutually exclusive, and the group
        // order (OT, CTOT, CTOB, OB) matches Perl's `elsif` chain.
        } else if line.starts_with(b"CT/GA/CT:") || line.starts_with(b"CT/CT:") {
            a.number_ot = field1_owned(line);
        } else if line.starts_with(b"GA/CT/CT:") || line.starts_with(b"GA/CT:") {
            a.number_ctot = field1_owned(line);
        } else if line.starts_with(b"GA/CT/GA:") || line.starts_with(b"GA/GA:") {
            a.number_ctob = field1_owned(line);
        } else if line.starts_with(b"CT/GA/GA:") || line.starts_with(b"CT/GA:") {
            a.number_ob = field1_owned(line);
        }
    }
    a
}

/// `Bismark report for: <X> (version: <Y>)` — greedy `.*` for the filename (so
/// the split is at the LAST ` (version: `), version is between that and the
/// final `)`. Mirrors `bismark2report:226`.
fn parse_report_for(line: &[u8], a: &mut Alignment) {
    let rest = &line[b"Bismark report for: ".len()..];
    let sep = b" (version: ";
    if let Some(vpos) = rfind(rest, sep) {
        let fname = &rest[..vpos];
        let after = &rest[vpos + sep.len()..];
        // Perl's `\(version: (.*)\)` is greedy and NOT end-anchored: the version
        // is everything up to the LAST `)` within `after`, and any trailing bytes
        // are ignored. Requiring `)` to be the FINAL byte would drop both fields
        // on a CRLF-terminated report (`after` = "…)\r") — a byte divergence
        // (the rest of a CRLF report rides its trailing `\r` identically in both
        // Perl and Rust, so this branch was the only CRLF gap).
        if let Some(rparen) = rfind(after, b")") {
            a.input_filename = Some(fname.to_vec());
            a.bismark_version = Some(after[..rparen].to_vec());
        }
    }
}

/// Fill the alignment placeholders, in Perl's exact substitution order
/// (`bismark2report:382-498`). ALL-OR-NOTHING gate on the 5 core fields using
/// `is_some()` (Perl `defined`, NOT truthiness — `0` is valid and must pass).
/// On gate failure the placeholders survive verbatim (SPEC §5.4).
pub fn fill(mut doc: Vec<u8>, a: &Alignment) -> Vec<u8> {
    if !(a.unique.is_some()
        && a.no_aln.is_some()
        && a.multiple.is_some()
        && a.no_genomic.is_some()
        && a.total_seqs.is_some())
    {
        return doc; // "Am I missing something?" — placeholders left unfilled
    }

    let o = |x: &Option<Vec<u8>>| -> Vec<u8> { x.as_deref().unwrap_or(b"").to_vec() };
    let unique = o(&a.unique);
    let no_aln = o(&a.no_aln);
    let multiple = o(&a.multiple);
    let no_genomic = o(&a.no_genomic);

    doc = subst_all(doc, b"{{unique_seqs}}", &unique);
    doc = subst_all(doc, b"{{unique_seqs_text}}", a.unique_text.unwrap_or(b""));
    doc = subst_all(doc, b"{{no_alignments}}", &no_aln);
    doc = subst_all(doc, b"{{no_alignments_text}}", a.no_aln_text.unwrap_or(b""));
    doc = subst_all(doc, b"{{multiple_alignments}}", &multiple);
    doc = subst_all(
        doc,
        b"{{multiple_alignments_text}}",
        a.multiple_text.unwrap_or(b""),
    );
    doc = subst_all(doc, b"{{no_genomic}}", &no_genomic);
    doc = subst_all(doc, b"{{total_sequences_alignments}}", &o(&a.total_seqs));
    doc = subst_all(
        doc,
        b"{{sequences_analysed_in_total}}",
        a.total_seq_text.unwrap_or(b""),
    );
    doc = subst_all(doc, b"{{filename}}", &o(&a.input_filename));
    doc = subst_all(doc, b"{{bismark_version}}", &o(&a.bismark_version));

    // Alignment-stats pie: unique,no_aln,multiple,no_genomic
    let aln_stats = join_with(&[&unique, &no_aln, &multiple, &no_genomic], b",");
    doc = subst_all(doc, b"{{alignment_stats_plotly}}", &aln_stats);

    // Strand origin (any may be undef → empty in the join, as Perl interpolates)
    let ot = o(&a.number_ot);
    let ctot = o(&a.number_ctot);
    let ctob = o(&a.number_ctob);
    let ob = o(&a.number_ob);
    doc = subst_all(doc, b"{{number_OT}}", &ot);
    doc = subst_all(doc, b"{{number_CTOT}}", &ctot);
    doc = subst_all(doc, b"{{number_CTOB}}", &ctob);
    doc = subst_all(doc, b"{{number_OB}}", &ob);

    doc = subst_all(doc, b"{{total_C_count}}", &o(&a.total_c_count));

    let strand = join_with(&[&ot, &ctot, &ctob, &ob], b",");
    doc = subst_all(doc, b"{{strand_alignment_plotly}}", &strand);

    // Percent display (undef → "N/A") and graph value ("N/A" → "0").
    let perc_cpg_disp = a.perc_cpg.as_deref().unwrap_or(b"N/A");
    let perc_chg_disp = a.perc_chg.as_deref().unwrap_or(b"N/A");
    let perc_chh_disp = a.perc_chh.as_deref().unwrap_or(b"N/A");
    let perc_unknown_disp = a.perc_unknown.as_deref().unwrap_or(b"N/A");

    // Unknown-context rows: present only if meth_unknown is defined.
    let (meth_u, unmeth_u, perc_u): (Vec<u8>, Vec<u8>, Vec<u8>) = if a.meth_unknown.is_some() {
        (
            unknown_tr(
                b"Methylated C's in Unknown context",
                &o(&a.meth_unknown),
                b"",
            ),
            unknown_tr(
                b"Unmethylated C's in Unknown context",
                &o(&a.unmeth_unknown),
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
    doc = subst_all(doc, b"{{meth_unknown}}", &meth_u);
    doc = subst_all(doc, b"{{unmeth_unknown}}", &unmeth_u);
    doc = subst_all(doc, b"{{perc_unknown}}", &perc_u);

    doc = subst_all(doc, b"{{meth_CpG}}", &o(&a.meth_cpg));
    doc = subst_all(doc, b"{{meth_CHG}}", &o(&a.meth_chg));
    doc = subst_all(doc, b"{{meth_CHH}}", &o(&a.meth_chh));

    doc = subst_all(doc, b"{{unmeth_CpG}}", &o(&a.unmeth_cpg));
    doc = subst_all(doc, b"{{unmeth_CHG}}", &o(&a.unmeth_chg));
    doc = subst_all(doc, b"{{unmeth_CHH}}", &o(&a.unmeth_chh));

    doc = subst_all(doc, b"{{perc_CpG}}", perc_cpg_disp);
    doc = subst_all(doc, b"{{perc_CHG}}", perc_chg_disp);
    doc = subst_all(doc, b"{{perc_CHH}}", perc_chh_disp);

    // Context-methylation bar: "N/A" → "0" in the graph string only.
    let cyto = join_with(
        &[
            graph_value(perc_cpg_disp),
            graph_value(perc_chg_disp),
            graph_value(perc_chh_disp),
        ],
        b",",
    );
    doc = subst_all(doc, b"{{cytosine_methylation_plotly}}", &cyto);

    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 4 always-present gate fields (total/unique/no_aln/multiple) + `extra`.
    fn pe(extra: &str) -> Vec<u8> {
        format!(
            "Sequence pairs analysed in total:\t100\n\
             Number of paired-end alignments with a unique best hit:\t80\n\
             Sequence pairs with no alignments under any condition:\t15\n\
             Sequence pairs did not map uniquely:\t5\n\
             {extra}"
        )
        .into_bytes()
    }

    const NO_GENOMIC_ZERO: &str =
        "Sequence pairs which were discarded because genomic sequence could not be extracted:\t0\n";

    #[test]
    fn gate_passes_when_no_genomic_is_zero() {
        // `0` is defined-but-falsy; the `is_some` gate must PASS (not truthiness).
        let a = parse(&pe(NO_GENOMIC_ZERO));
        let doc = fill(b"[{{unique_seqs}}|{{no_genomic}}]".to_vec(), &a);
        assert_eq!(doc, b"[80|0]");
    }

    #[test]
    fn gate_fails_when_field_missing_placeholders_survive() {
        // No `no_genomic` line → gate fails → placeholders survive verbatim.
        let a = parse(&pe(""));
        let doc = fill(b"[{{unique_seqs}}]".to_vec(), &a);
        assert_eq!(doc, b"[{{unique_seqs}}]");
    }

    #[test]
    fn percent_na_in_table_but_zero_in_graph() {
        let a = parse(&pe(NO_GENOMIC_ZERO));
        let doc = fill(
            b"[{{perc_CpG}}|{{cytosine_methylation_plotly}}]".to_vec(),
            &a,
        );
        assert_eq!(doc, b"[N/A|0,0,0]");
    }

    #[test]
    fn pe_labels_are_lifted_verbatim() {
        let a = parse(&pe(NO_GENOMIC_ZERO));
        assert_eq!(
            a.unique_text,
            Some(&b"Paired-end alignments with a unique best hit"[..])
        );
        assert_eq!(
            a.total_seq_text,
            Some(&b"Sequence pairs analysed in total"[..])
        );
    }

    #[test]
    fn version_and_filename_parsed_greedily() {
        let a = parse(b"Bismark report for: r1.fq.gz and r2.fq.gz (version: v0.25.1)\n");
        assert_eq!(
            a.input_filename.as_deref(),
            Some(&b"r1.fq.gz and r2.fq.gz"[..])
        );
        assert_eq!(a.bismark_version.as_deref(), Some(&b"v0.25.1"[..]));
    }

    #[test]
    fn version_parsed_on_crlf_line_trailing_cr_ignored() {
        // CRLF report: chomp leaves a trailing `\r`; Perl's regex still matches.
        let a = parse(b"Bismark report for: s.fq.gz (version: v0.25.1)\r\n");
        assert_eq!(a.input_filename.as_deref(), Some(&b"s.fq.gz"[..]));
        assert_eq!(a.bismark_version.as_deref(), Some(&b"v0.25.1"[..]));
    }

    #[test]
    fn version_uses_last_paren_when_value_contains_paren() {
        let a = parse(b"Bismark report for: s.fq.gz (version: v0.25.1)beta)\n");
        assert_eq!(a.bismark_version.as_deref(), Some(&b"v0.25.1)beta"[..]));
    }
}
