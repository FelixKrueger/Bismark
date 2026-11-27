//! `bismark align` exits 128 + N when an aligner is killed by signal N, and 1 otherwise.
//!
//! The fake `bowtie2` behaves like the real wrapper script: it runs an inner process,
//! and when that dies from a signal it prints `bowtie2-align died with signal N` and
//! exits 1. Each converted-genome instance (CT or GA index) gets its own behaviour.
#![cfg(unix)]

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

const TIMEOUT: Duration = Duration::from_secs(60);

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

fn write_reads(dir: &Path, n: usize) -> PathBuf {
    let seq = "ACGT".repeat(25);
    let qual = "I".repeat(seq.len());
    let mut fq = String::new();
    for i in 0..n {
        fq.push_str(&format!("@r{i}\n{seq}\n+\n{qual}\n"));
    }
    let p = dir.join("reads.fq");
    fs::write(&p, fq).unwrap();
    p
}

/// A wrapper-like fake `bowtie2`. `ct`/`ga` pick each instance's behaviour:
/// `ok` writes one unmapped record per read; `oom` has its inner process SIGKILLed;
/// `fail` has its inner process exit 1; `hang` writes the header and sleeps 3 s;
/// `selfkill` kills the wrapper itself with SIGKILL (no wrapper message).
fn make_fake_bowtie2(dir: &Path, ct: &str, ga: &str) {
    let script = format!(
        r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-U" ] && inp="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
case "$idx" in *BS_CT*) mode="{ct}";; *) mode="{ga}";; esac
printf '@HD\tVN:1.0\n'
case "$mode" in
  selfkill) kill -9 $$ ;;
  ok)   awk 'NR%4==1 {{ id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }}' "$inp" & ;;
  oom)  sh -c 'kill -9 $$' & ;;
  fail) sh -c 'exit 1' & ;;
  hang) sleep 3 & ;;
esac
wait $!
rc=$?
if [ $rc -gt 128 ]; then
  printf '(ERR): bowtie2-align died with signal %d (KILL) \n' $((rc - 128)) >&2
  exit 1
elif [ $rc -ne 0 ]; then
  printf '(ERR): bowtie2-align exited with value %d\n' $rc >&2
fi
exit $rc
"#
    );
    write_exec(&dir.join("bowtie2"), &script);
}

/// Run `bismark` against the fakes and return its exit code.
fn exit_code(ct: &str, ga: &str, extra: &[&str]) -> i32 {
    let genome = TempDir::new().unwrap();
    let bins = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    make_genome(genome.path());
    make_fake_bowtie2(bins.path(), ct, ga);
    let reads = write_reads(work.path(), 50);
    let mut child = Command::new(bin())
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .args(extra)
        .arg("--temp_dir")
        .arg(work.path())
        .arg("--output_dir")
        .arg(work.path())
        .arg(&reads)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status.code().expect("bismark itself exited normally");
        }
        if start.elapsed() > TIMEOUT {
            let _ = Command::new("kill")
                .args(["-KILL", "--", &format!("-{}", child.id())])
                .status();
            panic!("bismark did not finish within {TIMEOUT:?}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

const MODES: [&[&str]; 2] = [&[], &["--no_stream_converted"]];

#[test]
fn wrapper_reported_sigkill_exits_137() {
    for extra in MODES {
        assert_eq!(exit_code("oom", "ok", extra), 137, "{extra:?}");
    }
}

#[test]
fn aligner_killed_directly_exits_137() {
    for extra in MODES {
        assert_eq!(exit_code("selfkill", "ok", extra), 137, "{extra:?}");
    }
}

#[test]
fn sigkill_wins_over_the_teardown_of_its_sibling() {
    for extra in MODES {
        assert_eq!(exit_code("oom", "hang", extra), 137, "{extra:?}");
    }
}

#[test]
fn siblings_killed_in_teardown_do_not_count() {
    for extra in MODES {
        assert_eq!(exit_code("fail", "hang", extra), 1, "{extra:?}");
    }
}

#[test]
fn plain_failure_exits_1() {
    for extra in MODES {
        assert_eq!(exit_code("fail", "ok", extra), 1, "{extra:?}");
    }
}
