//! Shared fixture builders for the integration tests.
//!
//! Each fixture is a set of Bismark report files written into a fresh temp
//! dir. The BAM files themselves are empty (bismark2summary never opens
//! them — it derives report names from the BAM filename).

#![allow(dead_code)]

use std::fs;
use std::path::Path;

/// Write an empty BAM placeholder (only its name matters).
pub fn bam(dir: &Path, name: &str) {
    fs::write(dir.join(name), b"").unwrap();
}

/// A single-end Bismark alignment report.
pub fn se_alignment(
    dir: &Path,
    name: &str,
    total: u64,
    aligned: u64,
    unaligned: u64,
    ambig: u64,
    no_seq: u64,
) {
    fs::write(
        dir.join(name),
        format!(
            "Sequences analysed in total:\t{total}\n\
             Number of alignments with a unique best hit from the different alignments:\t{aligned}\n\
             Sequences with no alignments under any condition:\t{unaligned}\n\
             Sequences did not map uniquely:\t{ambig}\n\
             Sequences which were discarded because genomic sequence could not be extracted:\t{no_seq}\n\
             Total number of C's analysed:\t1000\n\
             Total methylated C's in CpG context:\t100\n\
             Total methylated C's in CHG context:\t10\n\
             Total methylated C's in CHH context:\t20\n\
             Total unmethylated C's in CpG context:\t900\n\
             Total unmethylated C's in CHG context:\t490\n\
             Total unmethylated C's in CHH context:\t980\n"
        ),
    )
    .unwrap();
}

/// A paired-end Bismark alignment report.
pub fn pe_alignment(
    dir: &Path,
    name: &str,
    total: u64,
    aligned: u64,
    unaligned: u64,
    ambig: u64,
    no_seq: u64,
) {
    fs::write(
        dir.join(name),
        format!(
            "Sequence pairs analysed in total:\t{total}\n\
             Number of paired-end alignments with a unique best hit:\t{aligned}\n\
             Sequence pairs with no alignments under any condition:\t{unaligned}\n\
             Sequence pairs did not map uniquely:\t{ambig}\n\
             Sequence pairs which were discarded because genomic sequence could not be extracted:\t{no_seq}\n\
             Total number of C's analysed:\t1000\n\
             Total methylated C's in CpG context:\t100\n\
             Total methylated C's in CHG context:\t10\n\
             Total methylated C's in CHH context:\t20\n\
             Total unmethylated C's in CpG context:\t900\n\
             Total unmethylated C's in CHG context:\t490\n\
             Total unmethylated C's in CHH context:\t980\n"
        ),
    )
    .unwrap();
}

/// A deduplication report. `bam_label` is echoed into the "analysed in" line.
pub fn dedup(dir: &Path, name: &str, bam_label: &str, analysed: u64, dups: u64, leftover: u64) {
    fs::write(
        dir.join(name),
        format!(
            "Total number of alignments analysed in {bam_label} in total:\t{analysed}\n\
             Total number duplicated alignments removed:\t{dups} (25.00%)\n\
             Total count of deduplicated leftover sequences:\t{leftover} (75.00% of total)\n"
        ),
    )
    .unwrap();
}

/// A methylation-extractor splitting report (C-to-T unmethylated counts).
#[allow(clippy::too_many_arguments)]
pub fn splitting(
    dir: &Path,
    name: &str,
    total_c: u64,
    m_cpg: u64,
    m_chg: u64,
    m_chh: u64,
    u_cpg: u64,
    u_chg: u64,
    u_chh: u64,
) {
    fs::write(
        dir.join(name),
        format!(
            "Total number of C's analysed:\t{total_c}\n\
             Total methylated C's in CpG context:\t{m_cpg}\n\
             Total methylated C's in CHG context:\t{m_chg}\n\
             Total methylated C's in CHH context:\t{m_chh}\n\
             Total C to T conversions in CpG context:\t{u_cpg}\n\
             Total C to T conversions in CHG context:\t{u_chg}\n\
             Total C to T conversions in CHH context:\t{u_chh}\n"
        ),
    )
    .unwrap();
}

/// A complete single-end WGBS sample (alignment + dedup + deduplicated
/// splitting) named `<prefix>_bismark_bt2.bam`, with standard nonzero
/// all-context methylation (never plot-excluded). Used to build multi-sample
/// directories where only the BAM names (ordering) matter.
pub fn wgbs_se_sample(dir: &Path, prefix: &str) {
    bam(dir, &format!("{prefix}_bismark_bt2.bam"));
    se_alignment(
        dir,
        &format!("{prefix}_bismark_bt2_SE_report.txt"),
        5000,
        4000,
        800,
        200,
        0,
    );
    dedup(
        dir,
        &format!("{prefix}_bismark_bt2.deduplication_report.txt"),
        &format!("{prefix}_bismark_bt2.bam"),
        4000,
        1000,
        3000,
    );
    splitting(
        dir,
        &format!("{prefix}_bismark_bt2.deduplicated_splitting_report.txt"),
        200000,
        4500,
        450,
        900,
        40000,
        20000,
        150000,
    );
}

/// Build the canonical 2-sample WGBS fixture (1 SE + 1 PE, both deduplicated
/// with deduplicated splitting reports) in `dir`.
pub fn build_wgbs_two_sample(dir: &Path) {
    // PE sample
    bam(dir, "s1_bismark_bt2_pe.bam");
    pe_alignment(
        dir,
        "s1_bismark_bt2_PE_report.txt",
        10000,
        8000,
        1500,
        500,
        0,
    );
    dedup(
        dir,
        "s1_bismark_bt2_pe.deduplication_report.txt",
        "s1_bismark_bt2_pe.bam",
        8000,
        2000,
        6000,
    );
    splitting(
        dir,
        "s1_bismark_bt2_pe.deduplicated_splitting_report.txt",
        400000,
        9000,
        900,
        1800,
        80000,
        40000,
        300000,
    );
    // SE sample
    bam(dir, "s2_bismark_bt2.bam");
    se_alignment(dir, "s2_bismark_bt2_SE_report.txt", 5000, 4000, 800, 200, 1);
    dedup(
        dir,
        "s2_bismark_bt2.deduplication_report.txt",
        "s2_bismark_bt2.bam",
        4000,
        1000,
        3000,
    );
    splitting(
        dir,
        "s2_bismark_bt2.deduplicated_splitting_report.txt",
        200000,
        4500,
        450,
        900,
        40000,
        20000,
        150000,
    );
}
