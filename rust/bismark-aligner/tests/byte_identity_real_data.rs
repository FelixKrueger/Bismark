//! Rust-vs-Rust byte-identity oracle for `bismark_rs` (the aligner).
//!
//! This is the regression gate for the Apple Silicon performance epic
//! (`plans/06222026_aligner-apple-silicon/`): every optimization on the
//! faithful Bowtie 2 path must change **zero output bytes**. Unlike the
//! sibling crates' real-data gates (which compare against a Perl v0.25.1
//! baseline), this one compares the **current** binary against a *golden*
//! captured from the pre-optimization binary — a pure Rust-vs-Rust diff,
//! which is faster to reason about (no Perl in the loop, no sanctioned
//! timestamp/version exceptions) and is **order-preserving** (the BAM
//! record stream is compared in file order, so a record-order regression
//! is caught — the shell `run_gate.sh` sorts and cannot catch that).
//!
//! ## Why `#[ignore]`?
//!
//! Running the gate aligns the 10M-read dataset with Bowtie 2 (minutes),
//! needs an aligner + a prepared genome on the host, and needs the golden
//! captured first. It is not part of the `cargo test` unit loop.
//!
//! ## Setup + invocation
//!
//! 1. Capture the golden ONCE from a known byte-identical commit:
//!    ```sh
//!    cd rust && just aligner-golden          # writes aligner_golden/rrbs_10m/golden.{bam,report.txt}
//!    ```
//! 2. Run the oracle (RRBS fast loop):
//!    ```sh
//!    cd rust && cargo test -p bismark-aligner --release \
//!      -- --ignored --exact byte_identity_real_data_rrbs_10m
//!    ```
//!
//! The dataset root defaults to `/Users/benjamin/bismark_benchmarks` and is
//! overridable per dataset (`BISMARK_ALIGNER_REAL_DATA_DIR_RRBS` /
//! `…_WGBS`). If any input or the golden is missing, the test prints a clear
//! SKIP and returns success — a host without the dataset sees no false
//! failure.
//!
//! ## What is compared
//!
//! - **BAM record stream**, record-by-record in order, including every tag
//!   (`XM`/`XR`/`XG`/`NM`/`MD`). The BAM header is intentionally *not*
//!   compared: its `@PG CL:` line embeds the `-o` output dir, which differs
//!   by design between the golden capture and the test's temp dir. The
//!   record bodies carry no paths, so they must match exactly.
//! - **Alignment report**, with absolute-path lines filtered out (the
//!   output dir / temp dir differ by design; the numbers must match).

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

/// A dataset the aligner oracle can run against. The choice of physical
/// root directory is overridable via the per-dataset env var.
struct AlignerDataset {
    /// Short label for skip/diagnostic messages.
    label: &'static str,
    /// Env var that overrides the default root directory.
    env_var: &'static str,
    /// Default root if the env var is unset.
    default_base: &'static str,
    /// Prepared genome directory, relative to the root.
    genome_rel: &'static str,
    /// Mate 1 / mate 2 FastQ, relative to the root.
    r1_rel: &'static str,
    r2_rel: &'static str,
    /// Golden BAM + report (captured by `just aligner-golden`), relative to the root.
    golden_bam_rel: &'static str,
    golden_report_rel: &'static str,
}

/// Fast iteration loop: Olecka 2024 mouse RRBS PE (GRCm39).
const DATASET_RRBS_10M: AlignerDataset = AlignerDataset {
    label: "rrbs_10m",
    env_var: "BISMARK_ALIGNER_REAL_DATA_DIR_RRBS",
    default_base: "/Users/benjamin/bismark_benchmarks",
    genome_rel: "genomes/GRCm39",
    r1_rel: "RRBS_PE/SRR24766921_10M_1.fastq.gz",
    r2_rel: "RRBS_PE/SRR24766921_10M_2.fastq.gz",
    golden_bam_rel: "aligner_golden/rrbs_10m/golden.bam",
    golden_report_rel: "aligner_golden/rrbs_10m/golden.report.txt",
};

/// Authoritative gate (Phase 6): Buckberry 2023 human WGBS PE (GRCh38).
/// Requires GRCh38 to be `bismark_genome_preparation`-ed first.
const DATASET_WGBS_10M: AlignerDataset = AlignerDataset {
    label: "wgbs_10m",
    env_var: "BISMARK_ALIGNER_REAL_DATA_DIR_WGBS",
    default_base: "/Users/benjamin/bismark_benchmarks",
    genome_rel: "genomes/GRCh38",
    r1_rel: "WGBS_PE/SRR24827378_10M_1.fastq.gz",
    r2_rel: "WGBS_PE/SRR24827378_10M_2.fastq.gz",
    golden_bam_rel: "aligner_golden/wgbs_10m/golden.bam",
    golden_report_rel: "aligner_golden/wgbs_10m/golden.report.txt",
};

fn base_dir(ds: &AlignerDataset) -> PathBuf {
    std::env::var(ds.env_var)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(ds.default_base))
}

/// Find the single file in `dir` whose name ends with `suffix`.
fn find_one(dir: &Path, suffix: &str) -> PathBuf {
    std::fs::read_dir(dir)
        .expect("read output dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(suffix))
        })
        .unwrap_or_else(|| panic!("no file ending {suffix:?} in {}", dir.display()))
}

/// Drop the report lines that legitimately vary between two runs of the same
/// code and are NOT output content:
///
/// - absolute filesystem paths (output dir / temp dir differ by design between
///   the golden capture and this run),
/// - the `Bismark completed in 0d 0h 19m 20s` wall-clock DURATION line (a
///   per-run timing, the same class of sanctioned timestamp exception as
///   `localtime` in bismark2report; it differs run-to-run and across
///   `-p`/`--multicore`, but is not part of the byte-identity contract).
///
/// The remaining report content (the numbers) must be byte-identical.
fn normalize_report(s: &str) -> String {
    s.lines()
        .filter(|l| {
            !l.contains("/Users/")
                && !l.contains("/tmp/")
                && !l.contains("/var/folders/")
                && !l.contains("/private/")
                && !l.contains("Bismark completed in")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn run_oracle(ds: &AlignerDataset) {
    let base = base_dir(ds);
    let genome = base.join(ds.genome_rel);
    let r1 = base.join(ds.r1_rel);
    let r2 = base.join(ds.r2_rel);
    let golden_bam = base.join(ds.golden_bam_rel);
    let golden_report = base.join(ds.golden_report_rel);

    for (name, p) in [
        ("genome", &genome),
        ("R1", &r1),
        ("R2", &r2),
        ("golden BAM", &golden_bam),
        ("golden report", &golden_report),
    ] {
        if !p.exists() {
            eprintln!(
                "SKIP: aligner byte-identity oracle ({}) — missing {} at {}.\n\
                 Set {} to override the root, and run `just aligner-golden` to \
                 capture the golden from a known byte-identical binary.",
                ds.label,
                name,
                p.display(),
                ds.env_var,
            );
            return;
        }
    }

    let out = TempDir::new().expect("create temp output dir");
    eprintln!(
        "running bismark_rs on {} (root {})...",
        ds.label,
        base.display()
    );
    Command::cargo_bin("bismark_rs")
        .unwrap()
        .arg("--genome")
        .arg(&genome)
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .arg("-o")
        .arg(out.path())
        .assert()
        .success();

    let cur_bam = find_one(out.path(), "_bismark_bt2_pe.bam");
    let cur_report = find_one(out.path(), "_PE_report.txt");

    // 1) BAM record stream — streamed lockstep (bounded memory), order-preserving.
    let mut g_reader = bismark_io::open_reader(&golden_bam, None).expect("open golden BAM");
    let mut c_reader = bismark_io::open_reader(&cur_bam, None).expect("open current BAM");
    let mut g_it = g_reader.records();
    let mut c_it = c_reader.records();
    let mut idx: u64 = 0;
    loop {
        match (g_it.next(), c_it.next()) {
            (Some(g), Some(c)) => {
                let g = format!("{:?}", g.expect("decode golden record").inner());
                let c = format!("{:?}", c.expect("decode current record").inner());
                assert_eq!(c, g, "BAM record {idx} differs (current vs golden)");
                idx += 1;
            }
            (None, None) => break,
            (g, c) => panic!(
                "BAM record count differs at index {idx}: golden has more = {}, current has more = {}",
                g.is_some(),
                c.is_some()
            ),
        }
    }
    eprintln!("✓ {idx} BAM records byte-identical");

    // 2) Report numbers (path lines normalized out).
    let golden = normalize_report(&std::fs::read_to_string(&golden_report).expect("read golden report"));
    let current = normalize_report(&std::fs::read_to_string(&cur_report).expect("read current report"));
    assert_eq!(current, golden, "report numbers differ (current vs golden)");
    eprintln!("✓ report numbers byte-identical");

    eprintln!("byte-identity oracle PASSED ({}): {idx} records", ds.label);
}

/// RRBS fast-loop oracle — run after every optimization.
#[test]
#[ignore]
fn byte_identity_real_data_rrbs_10m() {
    run_oracle(&DATASET_RRBS_10M);
}

/// WGBS authoritative oracle (Phase 6; needs GRCh38 prepared + golden captured).
#[test]
#[ignore]
fn byte_identity_real_data_wgbs_10m() {
    run_oracle(&DATASET_WGBS_10M);
}
