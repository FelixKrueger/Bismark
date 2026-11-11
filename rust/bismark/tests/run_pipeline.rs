//! `bismark run`: one command from reads to coverage.
//!
//! Tier 1 (always runs) checks the CLI. Tier 2 (runs where Bowtie 2 is
//! installed) runs `bismark run --skip_trim` on the E. coli fixture, then runs
//! `bismark dedup` and `bismark extract` by hand on the aligned BAM, and requires
//! the same deduplicated records and the same coverage files: streaming the
//! records between steps must change nothing. Without `bowtie2` on `PATH` it
//! prints a SKIP and passes.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

#[test]
fn bare_run_shows_its_help() {
    Command::cargo_bin("bismark")
        .unwrap()
        .arg("run")
        .assert()
        .code(2)
        .stderr(contains("--preset"));
}

#[test]
fn an_option_shared_with_another_meaning_asks_for_the_long_name() {
    Command::cargo_bin("bismark")
        .unwrap()
        .args(["run", "--skip_trim", "--genome", "G", "-q", "r.fq"])
        .assert()
        .failure()
        .stderr(contains("--fastq (align)").and(contains("use the long name")));
}

fn on_path(bin: &str) -> bool {
    StdCommand::new(bin)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the manifest lives two levels below the repo root")
        .to_path_buf()
}

/// Prepare (once) a Bowtie 2 bisulfite index of the E. coli fixture under `target/`.
fn prepared_genome() -> Option<PathBuf> {
    let dir = repo_root().join("rust/target/tests/run_pipeline_genome");
    let marker = dir.join(".prepared");
    if marker.exists() {
        return Some(dir);
    }
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::copy(
        repo_root().join("test_files/NC_010473.fa.gz"),
        dir.join("ecoli.fa.gz"),
    )
    .ok()?;
    let ok = Command::cargo_bin("bismark")
        .ok()?
        .args(["prepare", "--bowtie2", dir.to_str()?])
        .timeout(std::time::Duration::from_secs(900))
        .output()
        .ok()?
        .status
        .success();
    if !ok {
        return None;
    }
    std::fs::write(&marker, b"").ok()?;
    Some(dir)
}

fn records(bam: &Path) -> Vec<String> {
    let mut reader = bismark::io::BamReader::from_path(bam).expect("open BAM");
    reader
        .records()
        .map(|r| format!("{:?}", r.expect("record")))
        .collect()
}

fn gunzip(path: &Path) -> String {
    let mut s = String::new();
    flate2::read::MultiGzDecoder::new(
        std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
    )
    .read_to_string(&mut s)
    .unwrap();
    s
}

#[test]
fn streamed_run_matches_the_steps_run_one_by_one() {
    if !on_path("bowtie2") || !on_path("bowtie2-build") {
        eprintln!("SKIP streamed_run_matches_the_steps_run_one_by_one: bowtie2 not on PATH");
        return;
    }
    let Some(genome) = prepared_genome() else {
        eprintln!("SKIP streamed_run_matches_the_steps_run_one_by_one: genome prep failed");
        return;
    };
    let g = genome.to_str().unwrap();
    let root = repo_root();
    let (r1, r2) = (
        root.join("test_files/test_R1.fastq.gz"),
        root.join("test_files/test_R2.fastq.gz"),
    );
    let tmp = TempDir::new().unwrap();
    let run = tmp.path().join("run");
    let steps = tmp.path().join("steps");
    let bismark = || {
        let mut c = Command::cargo_bin("bismark").unwrap();
        c.timeout(std::time::Duration::from_secs(900));
        c
    };

    bismark()
        .args(["run", "--skip_trim", "--parallel", "2", "--genome", g])
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .arg("-o")
        .arg(&run)
        .assert()
        .success();

    std::fs::create_dir_all(&steps).unwrap();
    std::fs::copy(run.join("test_R1_pe.bam"), steps.join("test_R1_pe.bam")).unwrap();
    bismark()
        .args(["dedup", "--parallel", "2", "--output_dir"])
        .arg(&steps)
        .arg(steps.join("test_R1_pe.bam"))
        .assert()
        .success();
    bismark()
        .args([
            "extract",
            "--gzip",
            "--bedGraph",
            "--cytosine_report",
            "--ignore_r2",
            "2",
        ])
        .args(["--parallel", "2", "--genome_folder", g, "-o"])
        .arg(&steps)
        .arg(steps.join("test_R1_pe.deduplicated.bam"))
        .assert()
        .success();

    let dedup = records(&run.join("test_R1_pe.deduplicated.bam"));
    assert!(!dedup.is_empty(), "the fixture must actually align");
    assert_eq!(dedup, records(&steps.join("test_R1_pe.deduplicated.bam")));
    for f in [
        "test_R1_pe.deduplicated.bismark.cov.gz",
        "test_R1_pe.deduplicated.CpG_report.txt.gz",
        "CpG_OT_test_R1_pe.deduplicated.txt.gz",
    ] {
        assert_eq!(gunzip(&run.join(f)), gunzip(&steps.join(f)), "{f} differs");
    }
    assert_eq!(
        std::fs::read(run.join("test_R1_pe.deduplicated.M-bias.txt")).unwrap(),
        std::fs::read(steps.join("test_R1_pe.deduplicated.M-bias.txt")).unwrap(),
    );
    assert!(
        run.join("test_R1_PE_report.html").exists(),
        "the HTML report is written"
    );
}
