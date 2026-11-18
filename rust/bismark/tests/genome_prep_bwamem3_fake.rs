//! Hermetic `bismark_genome_preparation --bwamem3` tests against a fake `bwa-mem3` script.
//!
//! The fake answers `version` with `$FAKE_VERSION` (default `0.17.0`). For `index` it writes the
//! nine files `bwa-mem3 index --meth` writes, with an `.ann` listing the input's contigs.
//! `$FAKE_EXIT` (a code, or `kill`), `$FAKE_OMIT` (a suffix not to write) and `$FAKE_ANN_NAME`
//! (rename the first contig) inject failures; `$FAKE_ARGV_DUMP` receives its argv.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const SEVEN: [&str; 7] = [
    "amb",
    "ann",
    "pac",
    "meth.amb",
    "meth.ann",
    "meth.pac",
    "meth.bwt.2bit.64",
];

const FAKE: &str = r#"#!/bin/sh
if [ "$1" = version ]; then echo "${FAKE_VERSION:-0.17.0}"; echo "Compiler: gcc"; exit 0; fi
[ -n "$FAKE_ARGV_DUMP" ] && echo "$@" > "$FAKE_ARGV_DUMP"
for a in "$@"; do in=$a; done
[ "$FAKE_EXIT" = kill ] && kill -9 $$
[ -n "$FAKE_EXIT" ] && exit "$FAKE_EXIT"
for s in amb pac bwt.2bit.64 meth.amb meth.ann meth.pac meth.bwt.2bit.64 meth.fa; do
  [ "$s" = "$FAKE_OMIT" ] || : > "$in.$s"
done
awk -v rn="$FAKE_ANN_NAME" '/^>/ { n++; name[n] = substr($1, 2) }
  END { if (rn != "") name[1] = rn
        print n * 4, n, 11
        for (i = 1; i <= n; i++) { print 0, name[i], "(null)"; print (i - 1) * 4, 4, 0 } }' \
  "$in" > "$in.ann"
"#;

/// Write an executable script; retry until exec no longer reports text file busy.
fn write_exec(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    // A test thread forking while the script was open for writing makes exec fail briefly.
    for _ in 0..100 {
        match std::process::Command::new(path).arg("version").output() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            _ => break,
        }
    }
}

struct Setup {
    _tmp: TempDir,
    genome: PathBuf,
    bins: PathBuf,
    argv: PathBuf,
}

/// A two-file genome (`chrA.fa`, `chrB.fa`) and the fake `bwa-mem3`.
fn setup() -> Setup {
    let tmp = TempDir::new().unwrap();
    let genome = tmp.path().join("genome");
    fs::create_dir(&genome).unwrap();
    fs::write(genome.join("chrA.fa"), b">chrA desc\nACGT\n").unwrap();
    fs::write(genome.join("chrB.fa"), b">chrB\nGGCC").unwrap();
    let bins = tmp.path().join("bin");
    fs::create_dir(&bins).unwrap();
    write_exec(&bins.join("bwa-mem3"), FAKE);
    let argv = tmp.path().join("argv.txt");
    Setup {
        genome: fs::canonicalize(&genome).unwrap(),
        bins,
        argv,
        _tmp: tmp,
    }
}

fn prep(s: &Setup) -> Command {
    let mut c = Command::cargo_bin("bismark_genome_preparation").unwrap();
    c.env("BISMARK_BIN", &s.bins)
        .env("FAKE_ARGV_DUMP", &s.argv)
        .env_remove("FAKE_EXIT")
        .env_remove("FAKE_OMIT")
        .env_remove("FAKE_ANN_NAME")
        .env_remove("FAKE_VERSION")
        .arg("--bwamem3");
    c
}

fn sorted_names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn seven_files() -> Vec<String> {
    let mut v: Vec<String> = SEVEN.iter().map(|s| format!("genome.fa.{s}")).collect();
    v.sort();
    v
}

#[test]
fn builds_the_pruned_index_and_nothing_else() {
    let s = setup();
    prep(&s)
        .args(["--parallel", "4", "--bwamem3_max_memory", "8G"])
        .arg(&s.genome)
        .assert()
        .success()
        .stderr(predicate::str::contains("Bwamem3_Index"));
    assert_eq!(
        sorted_names(&s.genome),
        ["Bwamem3_Index", "chrA.fa", "chrB.fa"]
    );
    assert_eq!(sorted_names(&s.genome.join("Bwamem3_Index")), seven_files());
    let ann = fs::read_to_string(s.genome.join("Bwamem3_Index/genome.fa.ann")).unwrap();
    assert!(ann.contains(" chrA ") && ann.contains(" chrB "), "{ann}");

    let argv = fs::read_to_string(&s.argv).unwrap();
    let prefix = format!(
        "index --meth -t 4 --max-memory 8G --tmp-dir {}/Bwamem3_Index.partial.",
        s.genome.display()
    );
    assert!(
        argv.starts_with(&prefix) && argv.trim_end().ends_with("/tmp genome.fa"),
        "{argv}"
    );
    assert!(!argv.contains(" -p "), "{argv}");
}

#[test]
fn without_parallel_bwamem3_picks_its_threads() {
    let s = setup();
    prep(&s).arg(&s.genome).assert().success();
    let argv = fs::read_to_string(&s.argv).unwrap();
    assert!(
        argv.starts_with("index --meth --tmp-dir ") && !argv.contains("-t "),
        "{argv}"
    );
}

#[test]
fn a_rebuild_replaces_the_old_index() {
    let s = setup();
    let old = s.genome.join("Bwamem3_Index");
    fs::create_dir(&old).unwrap();
    fs::write(old.join("stale"), b"").unwrap();
    prep(&s)
        .arg(&s.genome)
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Replacing the existing bwa-mem3 index",
        ));
    assert_eq!(sorted_names(&old), seven_files());
    assert_eq!(
        sorted_names(&s.genome),
        ["Bwamem3_Index", "chrA.fa", "chrB.fa"]
    );
}

/// Each failure leaves an existing index and the genome folder as they were.
#[test]
fn failures_keep_the_existing_index() {
    let cases: [(&str, &str, &str); 6] = [
        ("FAKE_EXIT", "3", "--bwamem3_max_memory"),
        ("FAKE_EXIT", "1", "exit status: 1"),
        ("FAKE_EXIT", "kill", "signal"),
        ("FAKE_OMIT", "meth.pac", "did not write genome.fa.meth.pac"),
        (
            "FAKE_ANN_NAME",
            "chrZ",
            "`chrZ` in the index, `chrA` in the genome",
        ),
        ("FAKE_VERSION", "0.15.0", "too old"),
    ];
    for (var, value, want) in cases {
        let s = setup();
        let old = s.genome.join("Bwamem3_Index");
        fs::create_dir(&old).unwrap();
        fs::write(old.join("keep"), b"").unwrap();
        prep(&s)
            .env(var, value)
            .arg(&s.genome)
            .assert()
            .failure()
            .stderr(predicate::str::contains(want));
        assert_eq!(sorted_names(&old), ["keep"], "{var}={value}");
        assert_eq!(
            sorted_names(&s.genome),
            ["Bwamem3_Index", "chrA.fa", "chrB.fa"],
            "{var}={value}"
        );
    }
}

#[test]
fn failure_names_the_genome_folder() {
    let s = setup();
    prep(&s)
        .env("FAKE_EXIT", "1")
        .arg(&s.genome)
        .assert()
        .failure()
        .stderr(predicate::str::contains(s.genome.display().to_string()))
        .stderr(predicate::str::contains(".partial").not());
}

#[test]
fn old_version_fails_before_writing() {
    let s = setup();
    prep(&s)
        .env("FAKE_VERSION", "0.15.0")
        .arg(&s.genome)
        .assert()
        .failure()
        .stderr(predicate::str::contains("needs bwa-mem3 >= 0.16.0"));
    assert!(!s.argv.exists(), "index ran");
    assert_eq!(sorted_names(&s.genome), ["chrA.fa", "chrB.fa"]);
}

#[test]
fn unvalidated_version_warns() {
    let s = setup();
    prep(&s)
        .env("FAKE_VERSION", "0.18.0")
        .arg(&s.genome)
        .assert()
        .success()
        .stderr(predicate::str::contains("detected bwa-mem3 0.18.0"));
}

#[test]
fn another_runs_partial_folder_is_left_alone() {
    let s = setup();
    let foreign = s.genome.join("Bwamem3_Index.partial.999999");
    fs::create_dir(&foreign).unwrap();
    prep(&s)
        .arg(&s.genome)
        .assert()
        .success()
        .stderr(predicate::str::contains("left alone"));
    assert!(foreign.is_dir());
}

#[test]
fn bad_genomes_fail_before_indexing() {
    for (file, content, want) in [
        ("chrC.fa", &b"> chrC\nACGT\n"[..], "no sequence name"),
        ("chrC.fa", &b">chrA\nACGT\n"[..], "already exists"),
        (
            "genome.fa.meth.fa",
            &b">fchrA\nATAT\n"[..],
            "bwa-mem3 index file",
        ),
    ] {
        let s = setup();
        fs::write(s.genome.join(file), content).unwrap();
        prep(&s)
            .arg(&s.genome)
            .assert()
            .failure()
            .stderr(predicate::str::contains(want));
        assert!(!s.argv.exists(), "{file}: index ran");
        assert!(
            sorted_names(&s.genome)
                .iter()
                .all(|n| !n.starts_with("Bwamem3_Index")),
            "{file}"
        );
    }
}

#[test]
fn writes_no_bisulfite_genome_and_keeps_genomic_composition() {
    let s = setup();
    prep(&s)
        .arg("--genomic_composition")
        .arg(&s.genome)
        .assert()
        .success();
    assert!(!s.genome.join("Bisulfite_Genome").exists());
    assert!(
        s.genome
            .join("genomic_nucleotide_frequencies.txt")
            .is_file()
    );
}

#[test]
fn incompatible_flags_are_rejected() {
    let s = setup();
    for extra in [
        "--single_fasta",
        "--slam",
        "--large-index",
        "--combined_genome",
    ] {
        prep(&s)
            .arg(extra)
            .arg(&s.genome)
            .assert()
            .failure()
            .stderr(predicate::str::contains("bwa-mem3 mode"));
    }
}
