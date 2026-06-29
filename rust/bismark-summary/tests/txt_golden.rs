//! Deterministic `.txt` golden test — runs the built binary end-to-end and
//! asserts the byte-exact table. Needs **no** Perl (the `.txt` is pure
//! pass-through), so it gates the table on every CI runner.

mod common;

use std::process::Command;

#[test]
fn wgbs_two_sample_txt_is_byte_exact() {
    let dir = tempfile::tempdir().unwrap();
    common::build_wgbs_two_sample(dir.path());

    let status = Command::new(env!("CARGO_BIN_EXE_bismark2summary"))
        .current_dir(dir.path())
        .args(["-o", "out", "--title", "Gate Test"])
        .status()
        .expect("run bismark2summary_rs");
    assert!(status.success(), "binary exited non-zero: {status:?}");

    let got = std::fs::read_to_string(dir.path().join("out.txt")).unwrap();

    // Discovery order: the four globs run SE-bt2 first, so s2 (SE) precedes
    // s1 (PE). Aligned = the dedup "analysed" count (overwrites the alignment
    // report's unique-best-hit). Methylation = the splitting report's values
    // (C-to-T unmethylated). Columns 12-13 are the lowercase `chgs` quirk.
    let expected = "\
File\tTotal Reads\tAligned Reads\tUnaligned Reads\tAmbiguously Aligned Reads\tNo Genomic Sequence\tDuplicate Reads (removed)\tUnique Reads (remaining)\tTotal Cs\tMethylated CpGs\tUnmethylated CpGs\tMethylated chgs\tUnmethylated chgs\tMethylated CHHs\tUnmethylated CHHs
s2_bismark_bt2.bam\t5000\t4000\t800\t200\t1\t1000\t3000\t200000\t4500\t40000\t450\t20000\t900\t150000
s1_bismark_bt2_pe.bam\t10000\t8000\t1500\t500\t0\t2000\t6000\t400000\t9000\t80000\t900\t40000\t1800\t300000
";
    assert_eq!(got, expected);
}

#[test]
fn no_bams_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_bismark2summary"))
        .current_dir(dir.path())
        .args(["-o", "out"])
        .status()
        .unwrap();
    assert!(!status.success(), "empty dir should error");
    assert!(!dir.path().join("out.txt").exists());
}
