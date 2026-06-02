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

use crate::config::LibraryType;
use crate::error::Result;
use crate::merge::Counters;

/// Inputs for the report header (the three lines before the per-read loop).
pub struct ReportHeader<'a> {
    /// The read-file argument, verbatim (Perl `$sequence_file`, 1642).
    pub sequence_file: &'a str,
    /// The genome folder — **absolute, with a trailing `/`** (Perl absolutizes
    /// `$genome_folder` + forces a trailing slash, 7619–7629). The caller renders it.
    pub genome_folder: &'a str,
    /// The base Bowtie 2 option string (`config.aligner_options`).
    pub aligner_options: &'a str,
    /// Library type (only `Directional` is wired in v1; pbat/non-dir = Phase 8).
    pub library: LibraryType,
}

/// Write the report header (Perl 1642 + 1711–1729). Written before the read loop.
pub fn write_report_header<W: Write>(w: &mut W, h: &ReportHeader) -> Result<()> {
    write!(
        w,
        "Bismark report for: {} (version: {})\n",
        h.sequence_file,
        crate::BISMARK_VERSION
    )?;
    match h.library {
        LibraryType::Directional => write!(
            w,
            "Option '--directional' specified (default mode): alignments to complementary strands (CTOT, CTOB) were ignored (i.e. not performed)\n"
        )?,
        // pbat / non-directional lines (1714–1718) are Phase 8.
        LibraryType::Pbat => write!(
            w,
            "Option '--pbat' specified: alignments to original strands (OT and OB) strands were ignored (i.e. not performed)\n"
        )?,
        LibraryType::NonDirectional => write!(
            w,
            "Option '--non_directional' specified: alignments to all strands were being performed (OT, OB, CTOT, CTOB)\n"
        )?,
    }
    write!(
        w,
        "Bismark was run with Bowtie 2 against the bisulfite genome of {} with the specified options: {}\n\n",
        h.genome_folder, h.aligner_options
    )?;
    Ok(())
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

    write!(w, "Final Cytosine Methylation Report\n{}\n", "=".repeat(33))?;
    // Total EXCLUDES the Unknown buckets (Perl 2053).
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
    // The `seqID_contains_tabs` warning (2140–2143) never fires in v1 SE-directional
    // (`fix_id` strips tabs before the check), so it is intentionally not emitted.
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
            genome_folder: "/abs/genome/", // absolute + trailing slash
            aligner_options: "-q --score-min L,0,-0.2 --ignore-quals",
            library: LibraryType::Directional,
        };
        assert_eq!(
            s(&header_bytes(&h)),
            "Bismark report for: reads.fq (version: v0.25.1)\n\
             Option '--directional' specified (default mode): alignments to complementary strands (CTOT, CTOB) were ignored (i.e. not performed)\n\
             Bismark was run with Bowtie 2 against the bisulfite genome of /abs/genome/ with the specified options: -q --score-min L,0,-0.2 --ignore-quals\n\n"
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
}
