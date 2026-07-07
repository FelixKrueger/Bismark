//! The Bismark alignment report (`<name>_bismark_bt2_SE_report.txt`) — a port of
//! the report header (Perl `start_methylation_call_procedure_single_ends`
//! 1642/1711–1729) and `print_final_analysis_report_single_end` (1964–2144),
//! plus the trailing wall-clock line (926–927).
//!
//! **Byte-identity gate** (Phase 6): the report text must match Perl v0.25.1
//! **modulo the trailing `Bismark completed in …` line**, which is wall-clock-
//! dependent and is normalised out of both sides of the gate (as Phase 5 does
//! for the samtools `@PG`). Two byte traps: the embedded genome path is the
//! absolute, **trailing-`/`** form (the caller renders it that way), and the
//! `Total number of C's analysed` line **excludes the Unknown buckets** (2053).
//
// The report is a byte-exact text generator; explicit trailing `\n`s (vs
// `writeln!`) keep each line's terminator visible + auditable against the Perl,
// so the `write_with_newline` style lint is allowed here.
#![allow(clippy::write_with_newline)]

use std::io::Write;

use crate::aligner::config::{Aligner, LibraryType};
use crate::aligner::error::Result;
use crate::aligner::merge::Counters;

/// Inputs for the report header (the three lines before the per-read loop).
pub struct ReportHeader<'a> {
    /// The read-file argument, verbatim (Perl `$sequence_file`, 1642 / R1 at 1843).
    pub sequence_file: &'a str,
    /// The second (read-2) file for paired-end — `Some` ⇒ the header reads
    /// `for: <f1> and <f2>` (Perl 1843); `None` ⇒ single-end.
    pub sequence_file2: Option<&'a str>,
    /// The genome folder — **absolute, with a trailing `/`** (Perl absolutizes
    /// `$genome_folder` + forces a trailing slash, 7619–7629). The caller renders it.
    pub genome_folder: &'a str,
    /// The base aligner option string (`config.aligner_options`).
    pub aligner_options: &'a str,
    /// Which aligner — selects the "Bismark was run with …" wording (Perl
    /// 1722 Bowtie 2 / 1728 HISAT2 SE; 1846/1849 PE).
    pub aligner: Aligner,
    /// Library type (only `Directional` is wired in v1; pbat/non-dir = Phase 8).
    pub library: LibraryType,
}

/// Write the report header. 🔴 SE and PE differ in BOTH line order AND trailing
/// newlines (the block always ends with a blank line, so the LAST line gets `\n\n`):
/// - **SE** (Perl 1642/1712/1722): report-for, library-line (`\n`), `was run with` (`\n\n`).
/// - **PE** (Perl 1843/1846/1941): report-for, `was run with` (`\n`), library-line (`\n\n`).
///
/// The library-line TEXT also differs for pbat between SE (1715) and PE (1944).
pub fn write_report_header<W: Write>(w: &mut W, h: &ReportHeader) -> Result<()> {
    let paired = h.sequence_file2.is_some();
    // line 1: "Bismark report for: ..." (PE names both files, Perl 1843).
    match h.sequence_file2 {
        Some(f2) => write!(
            w,
            "Bismark report for: {} and {} (version: {})\n",
            h.sequence_file,
            f2,
            crate::aligner::BISMARK_VERSION
        )?,
        None => write!(
            w,
            "Bismark report for: {} (version: {})\n",
            h.sequence_file,
            crate::aligner::BISMARK_VERSION
        )?,
    }
    let run_with = format!(
        "Bismark was run with {} against the bisulfite genome of {} with the specified options: {}",
        h.aligner.name(),
        h.genome_folder,
        h.aligner_options
    );
    let library_line = library_line(h.library, paired);
    if paired {
        // PE order: `was run with` (`\n`), then the library line (`\n\n`).
        write!(w, "{run_with}\n")?;
        write!(w, "{library_line}\n\n")?;
    } else {
        // SE order: the library line (`\n`), then `was run with` (`\n\n`).
        write!(w, "{library_line}\n")?;
        write!(w, "{run_with}\n\n")?;
    }
    Ok(())
}

/// The library-type line (Perl SE 1712/1715/1718 vs PE 1941/1944/1947). Only the
/// pbat wording differs between SE ("(OT and OB) strands") and PE ("(OT, OB)").
fn library_line(library: LibraryType, paired: bool) -> &'static str {
    match (library, paired) {
        (LibraryType::Directional, _) => {
            "Option '--directional' specified (default mode): alignments to complementary strands (CTOT, CTOB) were ignored (i.e. not performed)"
        }
        (LibraryType::Pbat, false) => {
            "Option '--pbat' specified: alignments to original strands (OT and OB) strands were ignored (i.e. not performed)"
        }
        (LibraryType::Pbat, true) => {
            "Option '--pbat' specified: alignments to original strands (OT, OB) were ignored (i.e. not performed)"
        }
        (LibraryType::NonDirectional, _) => {
            "Option '--non_directional' specified: alignments to all strands were being performed (OT, OB, CTOT, CTOB)"
        }
    }
}

/// Write the final analysis (Perl `print_final_analysis_report_single_end`,
/// 2004–2144 — the `print REPORT` lines only). Written after the read loop.
pub fn print_final_analysis_report_single_end<W: Write>(
    w: &mut W,
    c: &Counters,
    directional: bool,
) -> Result<()> {
    write!(w, "Final Alignment report\n{}\n", "=".repeat(22))?;
    write!(w, "Sequences analysed in total:\t{}\n", c.sequences_count)?;

    // Mapping efficiency: `unique*100/seq` as %.1f, or the integer 0 when no
    // sequences (Perl 2017–2025; the 0 case prints "0%", not "0.0%").
    let efficiency = if c.sequences_count == 0 {
        "0".to_string()
    } else {
        format!(
            "{:.1}",
            (c.unique_best_alignment_count as f64) * 100.0 / (c.sequences_count as f64)
        )
    };
    write!(
        w,
        "Number of alignments with a unique best hit from the different alignments:\t{}\nMapping efficiency:\t{}%\n",
        c.unique_best_alignment_count, efficiency
    )?;

    write!(
        w,
        "Sequences with no alignments under any condition:\t{}\n",
        c.no_single_alignment_found
    )?;
    write!(
        w,
        "Sequences did not map uniquely:\t{}\n",
        c.unsuitable_sequence_count
    )?;
    write!(
        w,
        "Sequences which were discarded because genomic sequence could not be extracted:\t{}\n\n",
        c.genomic_sequence_could_not_be_extracted_count
    )?;
    write!(
        w,
        "Number of sequences with unique best (first) alignment came from the bowtie output:\n"
    )?;
    write!(
        w,
        "CT/CT:\t{}\t((converted) top strand)\nCT/GA:\t{}\t((converted) bottom strand)\nGA/CT:\t{}\t(complementary to (converted) top strand)\nGA/GA:\t{}\t(complementary to (converted) bottom strand)\n\n",
        c.ct_ct_count, c.ct_ga_count, c.ga_ct_count, c.ga_ga_count
    )?;

    if directional {
        write!(
            w,
            "Number of alignments to (merely theoretical) complementary strands being rejected in total:\t{}\n\n",
            c.alignments_rejected_count
        )?;
    }

    write_cytosine_report(w, c)?;
    // The `seqID_contains_tabs` warning (2140–2143) never fires in v1 SE-directional
    // (`fix_id` strips tabs before the check), so it is intentionally not emitted.
    Ok(())
}

/// Write the final analysis for **paired-end** (Perl `print_final_analysis_report_paired_ends`,
/// 2185–2312 — the `print REPORT` lines only). Differs from SE in the wording
/// ("Sequence pairs …"), the 3-token strand labels, and the trailing-space quirk
/// on the mapping-efficiency line (2205); the cytosine half is byte-identical.
pub fn print_final_analysis_report_paired_ends<W: Write>(
    w: &mut W,
    c: &Counters,
    directional: bool,
) -> Result<()> {
    write!(w, "Final Alignment report\n{}\n", "=".repeat(22))?;
    write!(
        w,
        "Sequence pairs analysed in total:\t{}\n",
        c.sequences_count
    )?;

    let efficiency = if c.sequences_count == 0 {
        "0".to_string()
    } else {
        format!(
            "{:.1}",
            (c.unique_best_alignment_count as f64) * 100.0 / (c.sequences_count as f64)
        )
    };
    // 🔴 REPORT line 2205 has a trailing space after `%` then a SINGLE `\n`
    // (`…% \n`) — do NOT copy the STDOUT twin at 2204 (`%\n\n`, no trailing space).
    write!(
        w,
        "Number of paired-end alignments with a unique best hit:\t{}\nMapping efficiency:\t{}% \n",
        c.unique_best_alignment_count, efficiency
    )?;

    write!(
        w,
        "Sequence pairs with no alignments under any condition:\t{}\n",
        c.no_single_alignment_found
    )?;
    write!(
        w,
        "Sequence pairs did not map uniquely:\t{}\n",
        c.unsuitable_sequence_count
    )?;
    write!(
        w,
        "Sequence pairs which were discarded because genomic sequence could not be extracted:\t{}\n\n",
        c.genomic_sequence_could_not_be_extracted_count
    )?;
    write!(
        w,
        "Number of sequence pairs with unique best (first) alignment came from the bowtie output:\n"
    )?;
    // 🔴 Join order is 0,2,1,3 (Perl 2218 join) — NOT field-declaration / scan order.
    write!(
        w,
        "CT/GA/CT:\t{}\t((converted) top strand)\nGA/CT/CT:\t{}\t(complementary to (converted) top strand)\nGA/CT/GA:\t{}\t(complementary to (converted) bottom strand)\nCT/GA/GA:\t{}\t((converted) bottom strand)\n\n",
        c.ct_ga_ct_count, c.ga_ct_ct_count, c.ga_ct_ga_count, c.ct_ga_ga_count
    )?;

    if directional {
        write!(
            w,
            "Number of alignments to (merely theoretical) complementary strands being rejected in total:\t{}\n\n",
            c.alignments_rejected_count
        )?;
    }

    write_cytosine_report(w, c)?;
    Ok(())
}

/// The Final Cytosine Methylation Report (Perl 2052–2136 SE == 2226–2312 PE),
/// byte-identical between SE and PE → shared.
fn write_cytosine_report<W: Write>(w: &mut W, c: &Counters) -> Result<()> {
    write!(w, "Final Cytosine Methylation Report\n{}\n", "=".repeat(33))?;
    // Total EXCLUDES the Unknown buckets (Perl 2053/2229).
    let total_c = c.total_me_cpg
        + c.total_me_chg
        + c.total_me_chh
        + c.total_unme_cpg
        + c.total_unme_chg
        + c.total_unme_chh;
    write!(w, "Total number of C's analysed:\t{total_c}\n\n")?;

    write!(
        w,
        "Total methylated C's in CpG context:\t{}\n",
        c.total_me_cpg
    )?;
    write!(
        w,
        "Total methylated C's in CHG context:\t{}\n",
        c.total_me_chg
    )?;
    write!(
        w,
        "Total methylated C's in CHH context:\t{}\n",
        c.total_me_chh
    )?;
    write!(
        w,
        "Total methylated C's in Unknown context:\t{}\n\n",
        c.total_me_c_unknown
    )?;

    write!(
        w,
        "Total unmethylated C's in CpG context:\t{}\n",
        c.total_unme_cpg
    )?;
    write!(
        w,
        "Total unmethylated C's in CHG context:\t{}\n",
        c.total_unme_chg
    )?;
    write!(
        w,
        "Total unmethylated C's in CHH context:\t{}\n",
        c.total_unme_chh
    )?;
    write!(
        w,
        "Total unmethylated C's in Unknown context:\t{}\n\n",
        c.total_unme_c_unknown
    )?;

    write_percentage(w, "CpG context", c.total_me_cpg, c.total_unme_cpg)?;
    write_percentage(w, "CHG context", c.total_me_chg, c.total_unme_chg)?;
    write_percentage(w, "CHH context", c.total_me_chh, c.total_unme_chh)?;
    write_percentage(
        w,
        "Unknown context (CN or CHN)",
        c.total_me_c_unknown,
        c.total_unme_c_unknown,
    )?;

    write!(w, "\n\n")?;
    Ok(())
}

/// One methylation-percentage line (Perl 2078–2136). Gate is `(me+unme) > 0`,
/// NOT "percentage non-zero": an all-unmethylated bucket prints `0.0%`, not the
/// "Can't determine" literal.
fn write_percentage<W: Write>(w: &mut W, label: &str, me: u64, unme: u64) -> Result<()> {
    if me + unme > 0 {
        let pct = format!("{:.1}", 100.0 * (me as f64) / ((me + unme) as f64));
        write!(w, "C methylated in {label}:\t{pct}%\n")?;
    } else {
        write!(
            w,
            "Can't determine percentage of methylated Cs in {label} if value was 0\n"
        )?;
    }
    Ok(())
}

/// The trailing wall-clock line (Perl 926–927). Wall-clock-dependent → the gate
/// filters `^Bismark completed in ` from both sides; this line's presence + format
/// match Perl, its value does not.
pub fn write_completion_line<W: Write>(w: &mut W, elapsed_secs: u64) -> Result<()> {
    let days = elapsed_secs / 86400;
    let hours = (elapsed_secs / 3600) % 24;
    let mins = (elapsed_secs / 60) % 60;
    let secs = elapsed_secs % 60;
    write!(w, "Bismark completed in {days}d {hours}h {mins}m {secs}s\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(w: &[u8]) -> String {
        String::from_utf8(w.to_vec()).unwrap()
    }

    fn header_bytes(h: &ReportHeader) -> Vec<u8> {
        let mut v = Vec::new();
        write_report_header(&mut v, h).unwrap();
        v
    }
    fn report_bytes(c: &Counters, directional: bool) -> Vec<u8> {
        let mut v = Vec::new();
        print_final_analysis_report_single_end(&mut v, c, directional).unwrap();
        v
    }

    #[test]
    fn header_directional_exact() {
        let h = ReportHeader {
            sequence_file: "reads.fq",
            sequence_file2: None,
            genome_folder: "/abs/genome/", // absolute + trailing slash
            aligner_options: "-q --score-min L,0,-0.2 --ignore-quals",
            aligner: Aligner::Bowtie2,
            library: LibraryType::Directional,
        };
        assert_eq!(
            s(&header_bytes(&h)),
            "Bismark report for: reads.fq (version: v0.25.1)\n\
             Option '--directional' specified (default mode): alignments to complementary strands (CTOT, CTOB) were ignored (i.e. not performed)\n\
             Bismark was run with Bowtie 2 against the bisulfite genome of /abs/genome/ with the specified options: -q --score-min L,0,-0.2 --ignore-quals\n\n"
        );
    }

    #[test]
    fn header_hisat2_run_with_line() {
        // V7: the SE HISAT2 header reads "Bismark was run with HISAT2 …" (Perl 1728).
        let h = ReportHeader {
            sequence_file: "reads.fq",
            sequence_file2: None,
            genome_folder: "/abs/genome/",
            aligner_options: "-q --score-min L,0,-0.2 --ignore-quals --no-softclip --omit-sec-seq",
            aligner: Aligner::Hisat2,
            library: LibraryType::Directional,
        };
        assert_eq!(
            s(&header_bytes(&h)),
            "Bismark report for: reads.fq (version: v0.25.1)\n\
             Option '--directional' specified (default mode): alignments to complementary strands (CTOT, CTOB) were ignored (i.e. not performed)\n\
             Bismark was run with HISAT2 against the bisulfite genome of /abs/genome/ with the specified options: -q --score-min L,0,-0.2 --ignore-quals --no-softclip --omit-sec-seq\n\n"
        );
    }

    fn counters_full() -> Counters {
        Counters {
            sequences_count: 10000,
            unique_best_alignment_count: 8402,
            unsuitable_sequence_count: 12,
            no_single_alignment_found: 1585,
            alignments_rejected_count: 1,
            ct_ct_count: 4163,
            ct_ga_count: 4239,
            ga_ct_count: 0,
            ga_ga_count: 0,
            genomic_sequence_could_not_be_extracted_count: 1,
            total_me_cpg: 100,
            total_me_chg: 5,
            total_me_chh: 7,
            total_me_c_unknown: 2,
            total_unme_cpg: 900,
            total_unme_chg: 95,
            total_unme_chh: 993,
            total_unme_c_unknown: 8,
            ..Default::default()
        }
    }

    #[test]
    fn final_analysis_exact_directional() {
        let c = counters_full();
        let expected = "Final Alignment report\n\
======================\n\
Sequences analysed in total:\t10000\n\
Number of alignments with a unique best hit from the different alignments:\t8402\n\
Mapping efficiency:\t84.0%\n\
Sequences with no alignments under any condition:\t1585\n\
Sequences did not map uniquely:\t12\n\
Sequences which were discarded because genomic sequence could not be extracted:\t1\n\n\
Number of sequences with unique best (first) alignment came from the bowtie output:\n\
CT/CT:\t4163\t((converted) top strand)\n\
CT/GA:\t4239\t((converted) bottom strand)\n\
GA/CT:\t0\t(complementary to (converted) top strand)\n\
GA/GA:\t0\t(complementary to (converted) bottom strand)\n\n\
Number of alignments to (merely theoretical) complementary strands being rejected in total:\t1\n\n\
Final Cytosine Methylation Report\n\
=================================\n\
Total number of C's analysed:\t2100\n\n\
Total methylated C's in CpG context:\t100\n\
Total methylated C's in CHG context:\t5\n\
Total methylated C's in CHH context:\t7\n\
Total methylated C's in Unknown context:\t2\n\n\
Total unmethylated C's in CpG context:\t900\n\
Total unmethylated C's in CHG context:\t95\n\
Total unmethylated C's in CHH context:\t993\n\
Total unmethylated C's in Unknown context:\t8\n\n\
C methylated in CpG context:\t10.0%\n\
C methylated in CHG context:\t5.0%\n\
C methylated in CHH context:\t0.7%\n\
C methylated in Unknown context (CN or CHN):\t20.0%\n\n\n";
        assert_eq!(s(&report_bytes(&c, true)), expected);
        // Total excludes the Unknown buckets: 100+5+7 + 900+95+993 = 2100.
    }

    #[test]
    fn zero_sequences_mapping_efficiency_is_bare_zero() {
        let c = Counters::default();
        let out = s(&report_bytes(&c, true));
        assert!(out.contains("Mapping efficiency:\t0%\n")); // not "0.0%"
    }

    #[test]
    fn all_unmethylated_bucket_prints_zero_point_zero() {
        // me=0, unme>0 → "0.0%", NOT "Can't determine".
        let c = Counters {
            total_me_cpg: 0,
            total_unme_cpg: 50,
            ..Default::default()
        };
        let out = s(&report_bytes(&c, true));
        assert!(out.contains("C methylated in CpG context:\t0.0%\n"));
        assert!(!out.contains("Can't determine percentage of methylated Cs in CpG context"));
    }

    #[test]
    fn empty_bucket_prints_cant_determine() {
        // me+unme==0 → "Can't determine".
        let c = Counters::default();
        let out = s(&report_bytes(&c, true));
        assert!(out.contains(
            "Can't determine percentage of methylated Cs in CpG context if value was 0\n"
        ));
        assert!(out.contains(
            "Can't determine percentage of methylated Cs in Unknown context (CN or CHN) if value was 0\n"
        ));
    }

    #[test]
    fn all_unknown_total_is_zero() {
        // Only Unknown buckets nonzero → Total C's analysed == 0 (Unknown excluded).
        let c = Counters {
            total_me_c_unknown: 4,
            total_unme_c_unknown: 6,
            ..Default::default()
        };
        let out = s(&report_bytes(&c, true));
        assert!(out.contains("Total number of C's analysed:\t0\n"));
        assert!(out.contains("C methylated in Unknown context (CN or CHN):\t40.0%\n"));
        // CpG/CHG/CHH all "Can't determine"
        assert!(out.contains("Can't determine percentage of methylated Cs in CpG context"));
    }

    #[test]
    fn mapping_efficiency_half_boundary_rounding() {
        // 1/8 = 12.5 exactly — pin the formatter vs Perl `printf` before the gate.
        let c = Counters {
            sequences_count: 8,
            unique_best_alignment_count: 1,
            ..Default::default()
        };
        let out = s(&report_bytes(&c, true));
        assert!(out.contains("Mapping efficiency:\t12.5%\n"), "got: {out}");
    }

    #[test]
    fn non_directional_omits_rejected_line() {
        let c = counters_full();
        let out = s(&report_bytes(&c, false));
        assert!(!out.contains("complementary strands being rejected"));
    }

    #[test]
    fn completion_line_format() {
        let mut v = Vec::new();
        write_completion_line(&mut v, 3661).unwrap(); // 1h 1m 1s
        assert_eq!(s(&v), "Bismark completed in 0d 1h 1m 1s\n");
    }

    // ---- paired-end report (Phase 7) ---------------------------------------

    #[test]
    fn pe_header_two_files() {
        let h = ReportHeader {
            sequence_file: "r_1.fq",
            sequence_file2: Some("r_2.fq"),
            genome_folder: "/abs/genome/",
            aligner_options: "-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --dovetail --maxins 500",
            aligner: Aligner::Bowtie2,
            library: LibraryType::Directional,
        };
        // 🔴 PE order (Perl 1843/1846/1941): report-for, `was run with` (single `\n`),
        // THEN the `--directional` line (`\n\n`) — the reverse of SE, with the `\n\n`
        // on the last line.
        assert_eq!(
            s(&header_bytes(&h)),
            "Bismark report for: r_1.fq and r_2.fq (version: v0.25.1)\n\
             Bismark was run with Bowtie 2 against the bisulfite genome of /abs/genome/ with the specified options: -q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --dovetail --maxins 500\n\
             Option '--directional' specified (default mode): alignments to complementary strands (CTOT, CTOB) were ignored (i.e. not performed)\n\n"
        );
    }

    #[test]
    fn pe_header_hisat2_run_with_line() {
        // V6 (Phase 2b): the PE HISAT2 header reads "Bismark was run with HISAT2 …"
        // with the PE line-order (the option string has NO `--dovetail`, 2a).
        let h = ReportHeader {
            sequence_file: "r_1.fq",
            sequence_file2: Some("r_2.fq"),
            genome_folder: "/abs/genome/",
            aligner_options: "-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --maxins 500 --no-softclip --omit-sec-seq",
            aligner: Aligner::Hisat2,
            library: LibraryType::Directional,
        };
        assert_eq!(
            s(&header_bytes(&h)),
            "Bismark report for: r_1.fq and r_2.fq (version: v0.25.1)\n\
             Bismark was run with HISAT2 against the bisulfite genome of /abs/genome/ with the specified options: -q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --maxins 500 --no-softclip --omit-sec-seq\n\
             Option '--directional' specified (default mode): alignments to complementary strands (CTOT, CTOB) were ignored (i.e. not performed)\n\n"
        );
    }

    fn counters_pe() -> Counters {
        Counters {
            sequences_count: 5000,
            unique_best_alignment_count: 4000,
            unsuitable_sequence_count: 200,
            no_single_alignment_found: 800,
            alignments_rejected_count: 0,
            ct_ga_ct_count: 2000,
            ga_ct_ct_count: 0,
            ga_ct_ga_count: 0,
            ct_ga_ga_count: 2000,
            genomic_sequence_could_not_be_extracted_count: 0,
            total_me_cpg: 100,
            total_me_chg: 5,
            total_me_chh: 7,
            total_me_c_unknown: 2,
            total_unme_cpg: 900,
            total_unme_chg: 95,
            total_unme_chh: 993,
            total_unme_c_unknown: 8,
            ..Default::default()
        }
    }

    fn pe_report_bytes(c: &Counters, directional: bool) -> Vec<u8> {
        let mut v = Vec::new();
        print_final_analysis_report_paired_ends(&mut v, c, directional).unwrap();
        v
    }

    #[test]
    fn pe_final_analysis_exact_directional() {
        let expected = "Final Alignment report\n\
======================\n\
Sequence pairs analysed in total:\t5000\n\
Number of paired-end alignments with a unique best hit:\t4000\n\
Mapping efficiency:\t80.0% \n\
Sequence pairs with no alignments under any condition:\t800\n\
Sequence pairs did not map uniquely:\t200\n\
Sequence pairs which were discarded because genomic sequence could not be extracted:\t0\n\n\
Number of sequence pairs with unique best (first) alignment came from the bowtie output:\n\
CT/GA/CT:\t2000\t((converted) top strand)\n\
GA/CT/CT:\t0\t(complementary to (converted) top strand)\n\
GA/CT/GA:\t0\t(complementary to (converted) bottom strand)\n\
CT/GA/GA:\t2000\t((converted) bottom strand)\n\n\
Number of alignments to (merely theoretical) complementary strands being rejected in total:\t0\n\n\
Final Cytosine Methylation Report\n\
=================================\n\
Total number of C's analysed:\t2100\n\n\
Total methylated C's in CpG context:\t100\n\
Total methylated C's in CHG context:\t5\n\
Total methylated C's in CHH context:\t7\n\
Total methylated C's in Unknown context:\t2\n\n\
Total unmethylated C's in CpG context:\t900\n\
Total unmethylated C's in CHG context:\t95\n\
Total unmethylated C's in CHH context:\t993\n\
Total unmethylated C's in Unknown context:\t8\n\n\
C methylated in CpG context:\t10.0%\n\
C methylated in CHG context:\t5.0%\n\
C methylated in CHH context:\t0.7%\n\
C methylated in Unknown context (CN or CHN):\t20.0%\n\n\n";
        assert_eq!(s(&pe_report_bytes(&counters_pe(), true)), expected);
    }

    #[test]
    fn pe_mapping_efficiency_has_trailing_space() {
        // The REPORT line (Perl 2205) is `…% \n` — a space then ONE newline.
        let out = s(&pe_report_bytes(&counters_pe(), true));
        assert!(out.contains("Mapping efficiency:\t80.0% \n"));
        assert!(!out.contains("Mapping efficiency:\t80.0%\n")); // NOT the no-space form
    }

    #[test]
    fn pe_non_directional_omits_rejected_line() {
        let out = s(&pe_report_bytes(&counters_pe(), false));
        assert!(!out.contains("complementary strands being rejected"));
    }

    #[test]
    fn pe_zero_pairs_mapping_efficiency_bare_zero() {
        let out = s(&pe_report_bytes(&Counters::default(), true));
        assert!(out.contains("Mapping efficiency:\t0% \n")); // bare 0 + trailing space
    }
}
