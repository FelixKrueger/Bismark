//! Streamed conversion must not deadlock however the aligner instances pace their
//! reads and writes (#1268).
//!
//! The fakes pair an instance that reads all of its input before writing with one
//! that writes as it reads, have minimap2 write a header larger than a pipe buffer
//! before it opens its input, and have an instance write more than a pipe buffer
//! without ever opening its input.
#![cfg(unix)]

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// Long enough to finish easily, short enough that a deadlock fails the test.
const TIMEOUT: Duration = Duration::from_secs(60);

/// Well above the per-instance slack (channel blocks plus pipe buffers).
const MIN_INPUT_BYTES: usize = 8 << 20;

fn bin() -> PathBuf {
    assert_cmd::cargo::cargo_bin("bismark")
}

fn write_exec(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, content).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

/// A genome dir with stub Bowtie 2 and minimap2 bisulfite indexes.
fn make_genome(dir: &Path) {
    let ct = dir.join("Bisulfite_Genome").join("CT_conversion");
    let ga = dir.join("Bisulfite_Genome").join("GA_conversion");
    fs::create_dir_all(&ct).unwrap();
    fs::create_dir_all(&ga).unwrap();
    for s in ["1", "2", "3", "4", "rev.1", "rev.2"] {
        fs::write(ct.join(format!("BS_CT.{s}.bt2")), b"x").unwrap();
        fs::write(ga.join(format!("BS_GA.{s}.bt2")), b"x").unwrap();
    }
    fs::write(ct.join("BS_CT.mmi"), b"x").unwrap();
    fs::write(ga.join("BS_GA.mmi"), b"x").unwrap();
    fs::write(dir.join("genome.fa"), b">chr1\nACGTACGT\n").unwrap();
}

/// `n` 100 bp reads named `r0`, `r1`, …
fn fastq(n: usize) -> String {
    let seq = "ACGT".repeat(25);
    let qual = "I".repeat(seq.len());
    let mut fq = String::with_capacity(n * 220);
    for i in 0..n {
        fq.push_str(&format!("@r{i}\n{seq}\n+\n{qual}\n"));
    }
    fq
}

/// SE: the CT-index instance writes one unmapped record per read as it reads; the
/// GA-index instance reads its whole input before writing any.
fn make_fake_bowtie2_se_slurping(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-U" ] && inp="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
case "$idx" in
  *BS_CT*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI"; fflush() }' "$inp" ;;
  *)       awk 'NR%4==1 { id=$1; sub(/^@/,"",id); ids[n++]=id } END { for (i=0;i<n;i++) print ids[i] "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// PE: as the SE fake, one unmapped pair per read pair. The CT-index instance reads
/// `-2` alongside `-1`; the GA-index instance reads all of `-2`, then all of `-1`,
/// before writing.
fn make_fake_bowtie2_pe_slurping(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; m2=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-1" ] && m1="$a"
  [ "$prev" = "-2" ] && m2="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
pairs='NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
  print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI"; fflush() }'
case "$idx" in
  *BS_CT*) cat "$m2" > /dev/null & awk "$pairs" "$m1"; wait ;;
  *)       cat "$m2" > /dev/null
           awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id); ids[n++]=id }
             END { for (i=0;i<n;i++) { print ids[i] "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print ids[i] "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI" } }' "$m1" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// minimap2 writes its SAM header before it opens the reads. This fake writes one
/// larger than a pipe buffer, then one unmapped record per read.
fn make_fake_minimap2_big_header(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "2.31-r1302"; exit 0;; esac
inp=""
for a in "$@"; do case "$a" in *.fastq|*.fq) inp="$a" ;; esac; done
awk 'BEGIN { print "@HD\tVN:1.0"; for (i=0;i<3000;i++) printf "@SQ\tSN:contig_%05d_with_a_deliberately_long_name\tLN:1000\n", i }'
awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp"
"#;
    write_exec(&dir.join("minimap2"), script);
}

/// SE: the CT-index instance writes one unmapped record per read as it reads; the
/// GA-index instance writes the same records without opening its input, then exits.
fn make_fake_bowtie2_se_never_opens(dir: &Path, n: usize) {
    let script = format!(
        r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-U" ] && inp="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
case "$idx" in
  *BS_CT*) awk 'NR%4==1 {{ id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }}' "$inp" ;;
  *)       awk 'BEGIN {{ for (i = 0; i < {n}; i++) print "r" i "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }}' ;;
esac
"#
    );
    write_exec(&dir.join("bowtie2"), &script);
}

/// PE: as the SE fake, one unmapped pair per read pair; the GA-index instance opens
/// neither of its inputs.
fn make_fake_bowtie2_pe_never_opens(dir: &Path, n: usize) {
    let script = format!(
        r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; m2=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-1" ] && m1="$a"
  [ "$prev" = "-2" ] && m2="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
case "$idx" in
  *BS_CT*) cat "$m2" > /dev/null &
           awk 'NR%4==1 {{ id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
             print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI" }}' "$m1"; wait ;;
  *)       awk 'BEGIN {{ for (i = 0; i < {n}; i++) {{ print "r" i "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print "r" i "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI" }} }}' ;;
esac
"#
    );
    write_exec(&dir.join("bowtie2"), &script);
}

/// Run `bismark` with the converted reads streamed, assert it finishes and
/// streamed, and return its stderr.
///
/// The run gets its own process group, killed whole on timeout, so a hung run's
/// aligner children do not outlive the test.
fn assert_streamed_run(genome: &Path, aligner_args: &[&str], reads: &[&Path]) -> String {
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();
    let log = TempDir::new().unwrap();
    let stderr_path = log.path().join("stderr");
    let mut cmd = Command::new(bin());
    cmd.arg("--genome")
        .arg(genome)
        .args(aligner_args)
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path());
    match reads {
        [se] => {
            cmd.arg(se);
        }
        [r1, r2] => {
            cmd.arg("-1").arg(r1).arg("-2").arg(r2);
        }
        _ => unreachable!("one or two read files"),
    }
    let mut child = cmd
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    if status.is_none() {
        // SAFETY: the group leader is unreaped, so the pgid is still this run's.
        unsafe { libc::killpg(child.id() as libc::pid_t, libc::SIGKILL) };
    }
    let _ = child.wait();
    let stderr = fs::read_to_string(&stderr_path).unwrap_or_default();
    let Some(status) = status else {
        panic!("bismark did not finish within {TIMEOUT:?} (deadlock); stderr:\n{stderr}");
    };
    assert!(
        status.success(),
        "bismark failed ({status}); stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("Streaming C->T converted"),
        "the run did not stream; stderr:\n{stderr}"
    );
    stderr
}

/// Every read counted, by the per-pass banner or, across chunks, the total.
fn assert_all_reads(stderr: &str, n: usize) {
    let per_pass = format!("({n} sequences, no temp file written)");
    let total = stderr
        .lines()
        .filter_map(|l| l.strip_prefix("sequences analysed:"))
        .any(|v| v.trim() == n.to_string());
    assert!(
        stderr.contains(&per_pass) || total,
        "not every read was accounted for; stderr:\n{stderr}"
    );
}

fn se_case(library: &[&str], extra: &[&str]) {
    const N: usize = 45_000;
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_se_slurping(bins.path());
    let fq = fastq(N);
    assert!(
        fq.len() > MIN_INPUT_BYTES,
        "input must exceed the per-instance slack"
    );
    let read = genome.path().join("reads.fq");
    fs::write(&read, fq).unwrap();
    let bins_dir = bins.path().to_str().unwrap();
    let mut args = vec!["--path_to_bowtie2", bins_dir];
    args.extend_from_slice(library);
    args.extend_from_slice(extra);
    let stderr = assert_streamed_run(genome.path(), &args, &[&read]);
    assert_all_reads(&stderr, N);
}

fn pe_case(library: &[&str]) {
    const N: usize = 45_000;
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_slurping(bins.path());
    let fq = fastq(N);
    assert!(
        fq.len() > MIN_INPUT_BYTES,
        "input must exceed the per-instance slack"
    );
    let r1 = genome.path().join("reads_1.fq");
    let r2 = genome.path().join("reads_2.fq");
    fs::write(&r1, &fq).unwrap();
    fs::write(&r2, &fq).unwrap();
    let bins_dir = bins.path().to_str().unwrap();
    let mut args = vec!["--path_to_bowtie2", bins_dir];
    args.extend_from_slice(library);
    let stderr = assert_streamed_run(genome.path(), &args, &[&r1, &r2]);
    assert_all_reads(&stderr, N);
}

#[test]
fn se_directional_survives_an_instance_that_reads_ahead() {
    se_case(&[], &[]);
}

#[test]
fn se_non_directional_survives_an_instance_that_reads_ahead() {
    se_case(&["--non_directional"], &[]);
}

#[test]
fn se_parallel_chunks_survive_an_instance_that_reads_ahead() {
    se_case(&[], &["--parallel", "2"]);
}

#[test]
fn pe_directional_survives_an_instance_that_reads_ahead() {
    pe_case(&[]);
}

#[test]
fn pe_non_directional_survives_an_instance_that_reads_ahead() {
    pe_case(&["--non_directional"]);
}

#[test]
fn minimap2_header_larger_than_a_pipe_does_not_stall_start_up() {
    const N: usize = 200;
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_minimap2_big_header(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, fastq(N)).unwrap();
    let bins_dir = bins.path().to_str().unwrap();
    let stderr = assert_streamed_run(
        genome.path(),
        &["--minimap2", "--path_to_minimap2", bins_dir],
        &[&read],
    );
    assert_all_reads(&stderr, N);
}

#[test]
fn an_instance_that_writes_without_opening_its_input_does_not_stall_start_up() {
    const N: usize = 6_000;
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_se_never_opens(bins.path(), N);
    let read = genome.path().join("reads.fq");
    fs::write(&read, fastq(N)).unwrap();
    let bins_dir = bins.path().to_str().unwrap();
    let stderr = assert_streamed_run(genome.path(), &["--path_to_bowtie2", bins_dir], &[&read]);
    assert_all_reads(&stderr, N);
}

#[test]
fn a_pe_instance_that_writes_without_opening_its_inputs_does_not_stall_start_up() {
    const N: usize = 3_000;
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_never_opens(bins.path(), N);
    let fq = fastq(N);
    let r1 = genome.path().join("reads_1.fq");
    let r2 = genome.path().join("reads_2.fq");
    fs::write(&r1, &fq).unwrap();
    fs::write(&r2, &fq).unwrap();
    let bins_dir = bins.path().to_str().unwrap();
    let stderr = assert_streamed_run(genome.path(), &["--path_to_bowtie2", bins_dir], &[&r1, &r2]);
    assert_all_reads(&stderr, N);
}
