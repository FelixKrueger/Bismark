//! End-to-end CLI tests for Phase 1 (`bismark_rs`): parse → discover → detect →
//! options → resolved-config summary, plus the deferral/validation error paths.
//!
//! Aligner detection runs `bowtie2 --version`; tests that reach detection use a
//! tiny **fake** `bowtie2` (reports `version 2.5.5`) via `--path_to_bowtie2`, so
//! they are hermetic and do not require a real Bowtie 2 install.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn bin() -> Command {
    Command::cargo_bin("bismark_rs").unwrap()
}

#[cfg(unix)]
fn write_exec(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, content).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

/// A genome dir with a complete small Bowtie 2 bisulfite index + one FASTA.
fn make_genome(dir: &Path) {
    let ct = dir.join("Bisulfite_Genome").join("CT_conversion");
    let ga = dir.join("Bisulfite_Genome").join("GA_conversion");
    fs::create_dir_all(&ct).unwrap();
    fs::create_dir_all(&ga).unwrap();
    for s in ["1", "2", "3", "4", "rev.1", "rev.2"] {
        fs::write(ct.join(format!("BS_CT.{s}.bt2")), b"x").unwrap();
        fs::write(ga.join(format!("BS_GA.{s}.bt2")), b"x").unwrap();
    }
    fs::write(dir.join("genome.fa"), b">chr1\nACGTACGT\n").unwrap();
}

#[cfg(unix)]
fn make_fake_bowtie2(dir: &Path) {
    write_exec(
        &dir.join("bowtie2"),
        "#!/bin/sh\necho \"fake-bowtie2 version 2.5.5\"\n",
    );
}

fn make_read(dir: &Path) -> std::path::PathBuf {
    let r = dir.join("reads.fq");
    fs::write(&r, b"@r1\nACGTACGT\n+\nIIIIIIII\n").unwrap();
    r
}

#[test]
fn version_flag_prints_banner() {
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("Bismark Aligner (Rust port)"));
}

#[test]
fn no_genome_errors() {
    bin()
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("No genome folder specified"));
}

#[test]
fn hisat2_is_deferred() {
    bin()
        .arg("--hisat2")
        .arg("some_genome")
        .arg("some_reads.fq")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("HISAT2").and(predicate::str::contains("deferred")));
}

#[test]
fn minimap2_is_deferred() {
    bin()
        .arg("--minimap2")
        .arg("some_genome")
        .arg("some_reads.fq")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("minimap2").and(predicate::str::contains("deferred")));
}

#[test]
fn missing_input_file_errors() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("/no/such/read.fq")
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("does not exist"));
}

#[cfg(unix)]
#[test]
fn happy_path_resolves_and_prints_config() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2(bins.path());
    let read = make_read(genome.path());

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("resolved configuration")
                .and(predicate::str::contains(
                    "-q --score-min L,0,-0.2 --ignore-quals",
                ))
                .and(predicate::str::contains("single-end"))
                .and(predicate::str::contains("Bowtie 2 2.5.5")),
        );
}

#[cfg(unix)]
#[test]
fn missing_index_errors() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    fs::remove_file(
        genome
            .path()
            .join("Bisulfite_Genome")
            .join("CT_conversion")
            .join("BS_CT.3.bt2"),
    )
    .unwrap();
    let read = make_read(genome.path());

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg(&read)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("faulty or non-existant"));
}

#[cfg(unix)]
#[test]
fn sam_output_is_deferred() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2(bins.path());
    let read = make_read(genome.path());

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--sam")
        .arg(&read)
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("SAM output is not yet supported"));
}

// ---- paired-end layout validation (resolves before discovery; hermetic) ----

#[test]
fn pe_mate_count_mismatch_errors() {
    bin()
        .args(["--genome", "g", "-1", "a.fq,b.fq", "-2", "c.fq"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("same amount of mate1 and mate2"));
}

#[test]
fn pe_same_file_errors() {
    bin()
        .args(["--genome", "g", "-1", "same.fq", "-2", "same.fq"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("exact same file"));
}

#[test]
fn se_pe_conflict_errors() {
    bin()
        .args([
            "--genome",
            "g",
            "--single_end",
            "r.fq",
            "-1",
            "a.fq",
            "-2",
            "b.fq",
        ])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("cannot set --single_end"));
}

#[test]
fn mate2_without_mate1_errors() {
    bin()
        .args(["--genome", "g", "-2", "b.fq"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("Paired-end mapping requires"));
}

#[test]
fn multicore_zero_errors() {
    bin()
        .args(["--genome", "g", "--multicore", "0", "r.fq"])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "Core usage needs to be set to 1 or more",
        ));
}

#[cfg(unix)]
#[test]
fn deferred_flag_emits_notice() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2(bins.path());
    let read = make_read(genome.path());

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--unmapped")
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("not yet active").and(predicate::str::contains("--unmapped")),
        );
}

#[cfg(unix)]
#[test]
fn pbat_genome_as_positional_resolves() {
    // genome given positionally (not via --genome), pbat library type.
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2(bins.path());
    let read = make_read(genome.path());

    bin()
        .arg("--pbat")
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg(genome.path()) // positional genome
        .arg(&read) // positional single-end read
        .assert()
        .success()
        .stderr(predicate::str::contains("pbat"));
}
