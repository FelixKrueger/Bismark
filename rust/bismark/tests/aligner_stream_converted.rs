//! `#1120`: streaming the converted reads to the aligner through FIFOs.
//!
//! Tier 1 (always runs, no fixtures) checks the routing matrix: a backend or
//! alignment model that cannot take a pipe must fall back to files and say why.
//!
//! Tier 2 (runs where Bowtie 2 is installed) aligns the same input streamed and
//! with `--no_stream_converted`, requires the BAM records to match, and checks
//! the streaming arm leaves no `_C_to_T` / `_G_to_A` file or FIFO behind. Without
//! `bowtie2`/`bowtie2-build` on `PATH` it prints a SKIP and passes. The E. coli
//! index is cached under `target/` so `bowtie2-build` runs once.

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use bismark::aligner::cli::Cli;
use bismark::aligner::config::{Aligner, resolve_stream_converted};
use clap::Parser;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Tier 1 — the routing matrix
// ---------------------------------------------------------------------------

#[test]
fn streaming_is_the_default() {
    let s = resolve_stream_converted(false, Aligner::Bowtie2, false, false, false);
    assert!(s.enabled, "streaming must be the default for Bowtie 2");
    assert!(s.reason.is_none());
}

#[test]
fn no_stream_converted_forces_files_without_a_complaint() {
    let s = resolve_stream_converted(true, Aligner::Bowtie2, false, false, false);
    assert!(!s.enabled);
    assert!(
        s.reason.is_none(),
        "an explicit --no_stream_converted needs no explanation; only automatic fallbacks do"
    );
}

/// HISAT2 rejects a FIFO (exit 255; it stats the file while sniffing the
/// format), so it keeps files and says so.
#[test]
fn hisat2_falls_back_to_files_and_says_why() {
    let s = resolve_stream_converted(false, Aligner::Hisat2, false, false, false);
    assert!(!s.enabled);
    let reason = s
        .reason
        .expect("a silent fallback would hide the disk cost");
    assert!(reason.contains("HISAT2"), "{reason}");
}

/// Every mode that reads converted files must explain its fallback.
#[test]
fn every_unsupported_mode_explains_itself() {
    for (label, s) in [
        (
            "combined index",
            resolve_stream_converted(false, Aligner::Bowtie2, true, false, false),
        ),
        (
            "the subprocess rammap backend",
            resolve_stream_converted(false, Aligner::Rammap, false, false, false),
        ),
    ] {
        assert!(!s.enabled, "{label} cannot stream yet");
        assert!(
            s.reason.is_some_and(|r| !r.is_empty()),
            "{label} must explain its fallback"
        );
    }
}

/// 5-Base aligns raw reads to the unconverted genome, so there is nothing to
/// stream and nothing to explain.
#[test]
fn five_base_has_nothing_to_stream() {
    let s = resolve_stream_converted(false, Aligner::Minimap2, false, true, false);
    assert!(!s.enabled);
    assert!(s.reason.is_none());
}

/// The in-process rammap backend takes the converted reads in memory (no temp
/// file, no FIFO); the subprocess backend keeps files.
#[test]
fn in_process_rammap_takes_its_reads_in_memory() {
    let in_memory = resolve_stream_converted(false, Aligner::Rammap, false, false, true);
    assert!(
        in_memory.enabled,
        "the in-process backend needs no temp file"
    );
    assert!(in_memory.reason.is_none());

    let subprocess = resolve_stream_converted(false, Aligner::Rammap, false, false, false);
    assert!(!subprocess.enabled);
    assert!(
        subprocess
            .reason
            .is_some_and(|r| r.contains("subprocess rammap")),
        "the fallback must say which rammap backend it means"
    );
}

/// `--no_stream_converted` also applies to the in-process backend.
#[test]
fn no_stream_converted_also_turns_off_the_in_memory_path() {
    let s = resolve_stream_converted(true, Aligner::Rammap, false, false, true);
    assert!(!s.enabled);
    assert!(s.reason.is_none());
}

#[test]
fn minimap2_streams() {
    assert!(resolve_stream_converted(false, Aligner::Minimap2, false, false, false).enabled);
}

#[test]
fn the_flag_parses_and_defaults_off() {
    assert!(!Cli::parse_from(["bismark"]).no_stream_converted);
    assert!(Cli::parse_from(["bismark", "--no_stream_converted"]).no_stream_converted);
}

// ---------------------------------------------------------------------------
// Tier 2 — the end-to-end byte-identity A/B
// ---------------------------------------------------------------------------

fn on_path(bin: &str) -> bool {
    StdCommand::new(bin)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/rust/bismark
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the manifest lives two levels below the repo root")
        .to_path_buf()
}

/// Prepare (once) a Bowtie 2 bisulfite index of the repo's E. coli fixture,
/// cached under `target/`. E. coli aligns ~51% of the test reads, so the
/// comparison exercises real alignments.
fn prepared_genome() -> Option<PathBuf> {
    let dir = repo_root().join("rust/target/tests/stream_converted_genome");
    let marker = dir.join(".prepared");
    if marker.exists() {
        return Some(dir);
    }
    std::fs::create_dir_all(&dir).ok()?;
    // Copied gzipped: bismark_genome_preparation reads `.fa.gz` directly, so the
    // test crate needs no decompressor of its own.
    let fa = dir.join("ecoli.fa.gz");
    if !fa.exists() {
        std::fs::copy(repo_root().join("test_files/NC_010473.fa.gz"), &fa).ok()?;
    }
    let ok = Command::cargo_bin("bismark_genome_preparation")
        .ok()?
        .args(["--bowtie2", dir.to_str()?])
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

/// Every mapped record, rendered for comparison. The header is not compared: its
/// `@PG CL:` line is the argv, which differs between the two arms.
fn records(bam: &Path) -> Vec<String> {
    let mut reader = bismark::io::BamReader::from_path(bam).expect("open BAM");
    reader
        .records()
        .map(|r| format!("{:?}", r.expect("record")))
        .collect()
}

fn only_bam(dir: &Path) -> PathBuf {
    let mut bams: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("output dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "bam"))
        .collect();
    assert_eq!(bams.len(), 1, "expected exactly one BAM in {dir:?}");
    bams.pop().unwrap()
}

/// Align `args` twice — streamed and file-based — and require the two BAMs to
/// agree, with nothing left in the streaming arm's temp dir.
fn assert_ab_identical(genome: &Path, label: &str, args: &[&str]) {
    let tmp = TempDir::new().unwrap();
    let mut arms = Vec::new();
    for arm in ["stream", "files"] {
        let out = tmp.path().join(format!("{label}_{arm}_out"));
        let temp = tmp.path().join(format!("{label}_{arm}_tmp"));
        std::fs::create_dir_all(&out).unwrap();
        std::fs::create_dir_all(&temp).unwrap();
        let mut cmd = Command::cargo_bin("bismark").unwrap();
        cmd.args(["--genome", genome.to_str().unwrap()])
            .args(args)
            .args(["-o", out.to_str().unwrap()])
            .args(["--temp_dir", temp.to_str().unwrap()])
            .timeout(std::time::Duration::from_secs(900));
        if arm == "files" {
            cmd.arg("--no_stream_converted");
        }
        let assert = cmd.assert().success();
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr).into_owned();
        arms.push((out, temp, stderr));
    }

    let (stream_out, stream_temp, stream_err) = &arms[0];
    let (files_out, _files_temp, files_err) = &arms[1];

    let a = records(&only_bam(stream_out));
    let b = records(&only_bam(files_out));
    assert!(!a.is_empty(), "[{label}] the fixture must actually align");
    assert_eq!(a.len(), b.len(), "[{label}] record count differs");
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        assert_eq!(x, y, "[{label}] record {i} differs");
    }

    // Nothing converted reaches the disk on the streaming arm.
    let leftovers: Vec<String> = std::fs::read_dir(stream_temp)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("_C_to_T") || n.contains("_G_to_A") || n.contains(".fifo"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "[{label}] streaming arm left {leftovers:?} in the temp dir"
    );

    // Both arms announce which path they took.
    assert!(
        stream_err.contains("no temp file written"),
        "[{label}] the streaming arm must say it streamed"
    );
    assert!(
        files_err.contains("Created") && files_err.contains("converted version"),
        "[{label}] the file arm must keep its original banner"
    );
}

#[test]
fn streamed_and_file_paths_produce_identical_bams() {
    if !on_path("bowtie2") || !on_path("bowtie2-build") {
        eprintln!("SKIP streamed_and_file_paths_produce_identical_bams: bowtie2 not on PATH");
        return;
    }
    let Some(genome) = prepared_genome() else {
        eprintln!("SKIP streamed_and_file_paths_produce_identical_bams: genome prep failed");
        return;
    };
    let root = repo_root();
    let r1 = root.join("test_files/test_R1.fastq.gz");
    let r2 = root.join("test_files/test_R2.fastq.gz");
    let (r1, r2) = (r1.to_str().unwrap(), r2.to_str().unwrap());

    // Directional SE and PE, plus non-directional PE: 1, 2 and 4 converted
    // streams respectively.
    assert_ab_identical(&genome, "se", &["--single_end", r1]);
    assert_ab_identical(&genome, "pe", &["-1", r1, "-2", r2]);
    assert_ab_identical(
        &genome,
        "pe_nondir",
        &["-1", r1, "-2", r2, "--non_directional"],
    );
}
