//! `[#1104]` Integration gates for `--five_base_emit_multiplicity` (simplex consensus)
//! and the deterministic consensus emission order it builds on.
//!
//! All tests drive the standalone `--five_base_consensus_from_bam` entry over
//! hand-crafted BAMs (built from SAM text via samtools), so no aligner is needed.
//! The in-run entry point is covered in `aligner_five_base_groundtruth.rs`.
//!
//! Fixture geometry: a genome of repeated 22-bp blocks, each holding one CpG and
//! no other C/G anywhere — so every consensus XM is dots except the CpG calls,
//! and expected strings can be stated from first principles.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bismark_bin() -> PathBuf {
    // target/debug/deps/<test binary> -> target/debug/bismark
    let mut p = std::env::current_exe().expect("test exe path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("bismark")
}

fn samtools_available() -> bool {
    let present = Command::new("samtools")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    // Fail loud in CI so these gates never pass vacuously (same guard as the
    // #1095 suite).
    if !present && std::env::var_os("CI").is_some() {
        panic!("samtools not found but $CI is set: refusing to no-op.");
    }
    present
}

/// One 22-bp block: a 20-bp window whose only C/G is the CpG at window
/// offsets 6/7, plus a 2-bp spacer. Window `i` starts at genome offset
/// `10 + 22*i` (0-based).
const WINDOW: &str = "AATTAACGTTAATTAATTAA";
const LEAD: &str = "AATTAATTAA";

fn genome_seq(n_windows: usize) -> String {
    let mut s = String::from(LEAD);
    for _ in 0..n_windows {
        s.push_str(WINDOW);
        s.push_str("TT");
    }
    s
}

/// 0-based genome offset of window `i`'s first base.
fn window_start(i: usize) -> usize {
    LEAD.len() + 22 * i
}

/// Write `>chrT` + the sequence as the genome folder's FASTA.
fn write_genome(dir: &Path, n_windows: usize) {
    std::fs::write(
        dir.join("chrT.fa"),
        format!(">chrT\n{}\n", genome_seq(n_windows)),
    )
    .unwrap();
}

/// A proper FR read pair fully covering window `i`, as two SAM lines.
/// `ot`: molecule strand (true = OT: R1 forward; false = OB: R1 reverse).
/// `seq`: the +ref-oriented 20-bp SEQ both mates carry.
fn pair(qname: &str, i: usize, ot: bool, seq: &str, mapq_r1: u8, mapq_r2: u8) -> String {
    let pos = window_start(i) + 1; // SAM is 1-based
    let dots = ".".repeat(seq.len());
    // FLAG combos: OT = 99/147 (R1 fwd + R2 rev), OB = 83/163 (R1 rev + R2 fwd).
    let (f1, f2, t1, t2) = if ot {
        (99, 147, 20, -20)
    } else {
        (83, 163, -20, 20)
    };
    let mut s = String::new();
    for (flag, tlen, mapq, xr) in [(f1, t1, mapq_r1, "CT"), (f2, t2, mapq_r2, "GA")] {
        s.push_str(&format!(
            "{qname}\t{flag}\tchrT\t{pos}\t{mapq}\t20M\t=\t{pos}\t{tlen}\t{seq}\t\
             IIIIIIIIIIIIIIIIIIII\tXM:Z:{dots}\tXR:Z:{xr}\tXG:Z:{}\n",
            if ot { "CT" } else { "GA" }
        ));
    }
    s
}

/// Assemble a BAM from SAM record lines via samtools.
fn make_bam(dir: &Path, name: &str, n_windows: usize, records: &str) -> PathBuf {
    let sam = format!(
        "@HD\tVN:1.6\n@SQ\tSN:chrT\tLN:{}\n{records}",
        genome_seq(n_windows).len()
    );
    let sam_path = dir.join(format!("{name}.sam"));
    std::fs::write(&sam_path, sam).unwrap();
    let bam_path = dir.join(format!("{name}.bam"));
    let out = Command::new("samtools")
        .args([
            "view",
            "-b",
            "-o",
            &bam_path.to_string_lossy(),
            &sam_path.to_string_lossy(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "samtools view -b failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    bam_path
}

/// Run the standalone consensus over `bam` with extra args; assert success.
fn run_consensus(genome_dir: &Path, out_dir: &Path, bam: &Path, extra: &[&str]) -> String {
    let out = Command::new(bismark_bin())
        .arg("--illumina_5base")
        .arg("--five_base_consensus_from_bam")
        .arg(bam)
        .arg("--genome")
        .arg(genome_dir)
        .arg("--output_dir")
        .arg(out_dir)
        .args(extra)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "consensus run failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Full decompressed SAM text (header + records), order preserved, with `@PG`
/// lines dropped — they embed the run's own command line (per-run temp paths),
/// matching the suite's byte-gate convention.
fn sam_text(bam: &Path) -> String {
    let out = Command::new("samtools")
        .args(["view", "-h", &bam.to_string_lossy()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "samtools view failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.starts_with("@PG"))
        .map(|l| format!("{l}\n"))
        .collect()
}

/// A methylated-OT window SEQ: 5mC→T at the CpG C (window offset 6).
fn seq_ot_methylated() -> String {
    let mut s = WINDOW.to_string();
    s.replace_range(6..7, "T");
    s
}

// ---- step 0: deterministic emission order --------------------------------------------

/// Two runs of the SAME binary over a multi-family BAM must produce identical
/// consensus output. Before #1104's step 0 the emit loop followed HashMap
/// iteration order (random per process), so this was not true.
#[test]
fn consensus_emission_order_is_deterministic() {
    if !samtools_available() {
        eprintln!("skipping: samtools not on PATH");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let n = 6;
    write_genome(tmp.path(), n);
    // Six duplex families: an OT pair and an OB pair on each window.
    let mut recs = String::new();
    for i in 0..n {
        recs.push_str(&pair(
            &format!("ot{i}"),
            i,
            true,
            &seq_ot_methylated(),
            60,
            60,
        ));
        recs.push_str(&pair(&format!("ob{i}"), i, false, WINDOW, 60, 60));
    }
    let bam = make_bam(tmp.path(), "duplex6", n, &recs);

    let out_a = tempfile::tempdir().unwrap();
    let out_b = tempfile::tempdir().unwrap();
    run_consensus(tmp.path(), out_a.path(), &bam, &[]);
    run_consensus(tmp.path(), out_b.path(), &bam, &[]);
    let sam_a = sam_text(&out_a.path().join("five_base_consensus.bam"));
    let sam_b = sam_text(&out_b.path().join("five_base_consensus.bam"));
    assert!(
        sam_a.lines().filter(|l| !l.starts_with('@')).count() >= 2 * n,
        "fixture must yield at least {n} families x 2 records or the gate is vacuous"
    );
    assert_eq!(
        sam_a, sam_b,
        "consensus output must be byte-stable across runs of the same binary"
    );
    // The order itself is the sorted family order: qnames are dpx:{chrom}:{start}-{end}:NA.
    let starts: Vec<u32> = sam_a
        .lines()
        .filter(|l| l.starts_with("dpx:"))
        .map(|l| {
            l.split(':')
                .nth(2)
                .unwrap()
                .split('-')
                .next()
                .unwrap()
                .parse::<u32>()
                .unwrap()
        })
        .collect();
    let mut sorted = starts.clone();
    sorted.sort_unstable();
    assert_eq!(starts, sorted, "families must emit in ascending span order");
}
