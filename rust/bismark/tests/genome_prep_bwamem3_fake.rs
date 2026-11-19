//! Hermetic `bismark_genome_preparation --bwamem3` tests against a fake `bwa-mem3` script.
//!
//! The fake answers `version` with `$FAKE_VERSION` (default `0.17.0`). For `index` it writes the
//! nine files `bwa-mem3 index --meth` writes, or with `--meth-only` the seven (rejected below
//! 0.18.0, as the real binary does), with an `.ann` listing the input's contigs.
//! `$FAKE_EXIT` (a code, or `kill`), `$FAKE_OMIT` (a suffix not to write), `$FAKE_EXTRA` (a suffix
//! to write as well) and `$FAKE_ANN_NAME` (rename the first contig) inject failures; `$FAKE_LOCK_GENOME` makes the genome folder read-only
//! after a successful build; `$FAKE_ARGV_DUMP` receives its argv. Every `.ann` contig is
//! 4 bp long, as in [`setup`]'s genome.
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
mo=; for a in "$@"; do in=$a; [ "$a" = --meth-only ] && mo=1; done
[ "$FAKE_EXIT" = kill ] && kill -9 $$
[ -n "$FAKE_EXIT" ] && exit "$FAKE_EXIT"
files="amb pac bwt.2bit.64 meth.amb meth.ann meth.pac meth.bwt.2bit.64 meth.fa"
if [ -n "$mo" ]; then
  case "${FAKE_VERSION:-0.17.0}" in 0.16.*|0.17.*) echo "unrecognized option '--meth-only'" >&2; exit 1;; esac
  files="amb pac meth.amb meth.ann meth.pac meth.bwt.2bit.64"
fi
for s in $files; do
  [ "$s" = "$FAKE_OMIT" ] || : > "$in.$s"
done
[ -n "$FAKE_EXTRA" ] && : > "$in.$FAKE_EXTRA"
awk -v rn="$FAKE_ANN_NAME" '/^>/ { n++; name[n] = substr($1, 2) }
  END { if (rn != "") name[1] = rn
        print n * 4, n, 11
        for (i = 1; i <= n; i++) { print 0, name[i], "(null)"; print (i - 1) * 4, 4, 0 } }' \
  "$in" > "$in.ann"
[ -n "$FAKE_LOCK_GENOME" ] && chmod 555 ..
exit 0
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
        .env_remove("FAKE_EXTRA")
        .env_remove("FAKE_ANN_NAME")
        .env_remove("FAKE_VERSION")
        .env_remove("FAKE_LOCK_GENOME")
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

/// From 0.18.0 the plain FM-index is never built: `--meth-only` writes the seven files directly.
#[test]
fn bwamem3_0_18_0_builds_with_meth_only() {
    let s = setup();
    prep(&s)
        .env("FAKE_VERSION", "0.18.0")
        .args(["--parallel", "4"])
        .arg(&s.genome)
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "`bwa-mem3 index --meth --meth-only`",
        ))
        .stderr(predicate::str::contains("about 35 GB of free disk"));
    assert_eq!(sorted_names(&s.genome.join("Bwamem3_Index")), seven_files());
    let argv = fs::read_to_string(&s.argv).unwrap();
    assert!(
        argv.starts_with("index --meth --meth-only -t 4 --tmp-dir "),
        "{argv}"
    );
}

/// Before 0.18.0, the full build runs and is pruned to the seven files.
#[test]
fn bwamem3_0_17_0_builds_then_prunes() {
    let s = setup();
    prep(&s)
        .env("FAKE_VERSION", "0.17.0")
        .arg(&s.genome)
        .assert()
        .success()
        .stderr(predicate::str::contains("`bwa-mem3 index --meth`."))
        .stderr(predicate::str::contains("up to 70 GB of free disk"));
    assert_eq!(sorted_names(&s.genome.join("Bwamem3_Index")), seven_files());
    let argv = fs::read_to_string(&s.argv).unwrap();
    assert!(!argv.contains("--meth-only"), "{argv}");
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
    let cases: [(&str, &str, &str); 8] = [
        ("FAKE_EXIT", "3", "the detected RAM less a reserve"),
        ("FAKE_EXIT", "1", "exit status: 1"),
        ("FAKE_EXIT", "5", "exit status: 5"),
        ("FAKE_EXIT", "kill", "signal"),
        ("FAKE_OMIT", "meth.pac", "did not write genome.fa.meth.pac"),
        (
            "FAKE_ANN_NAME",
            "chrZ",
            "`chrZ` (4 bp) in the index, `chrA` (4 bp) in the genome",
        ),
        ("FAKE_VERSION", "0.15.0", "too old"),
        (
            "FAKE_EXTRA",
            "alt",
            "does not expect in the index: genome.fa.alt",
        ),
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

#[test]
fn memory_error_names_the_budget_the_user_gave() {
    let s = setup();
    prep(&s)
        .env("FAKE_EXIT", "3")
        .args(["--bwamem3_max_memory", "8G"])
        .arg(&s.genome)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "the --bwamem3_max_memory you gave (8G)",
        ))
        .stderr(predicate::str::contains("detected RAM").not());
}

#[test]
fn a_failed_build_writes_no_genomic_composition() {
    let s = setup();
    prep(&s)
        .env("FAKE_EXIT", "3")
        .arg("--genomic_composition")
        .arg(&s.genome)
        .assert()
        .failure();
    assert!(!s.genome.join("genomic_nucleotide_frequencies.txt").exists());
}

#[test]
fn parallel_one_is_bwamem3s_thread_count() {
    let s = setup();
    prep(&s)
        .args(["--parallel", "1"])
        .arg(&s.genome)
        .assert()
        .success();
    let argv = fs::read_to_string(&s.argv).unwrap();
    assert!(argv.starts_with("index --meth -t 1 "), "{argv}");
}

#[test]
fn a_read_only_genome_folder_names_the_path() {
    use std::os::unix::fs::PermissionsExt;
    let s = setup();
    fs::set_permissions(&s.genome, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::write(s.genome.join("probe"), b"").is_ok() {
        fs::set_permissions(&s.genome, fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("skipping: permission bits do not stop this user (root)");
        return;
    }
    let out = prep(&s).arg(&s.genome).assert().failure();
    fs::set_permissions(&s.genome, fs::Permissions::from_mode(0o755)).unwrap();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert!(
        stderr.contains(&format!("{}/Bwamem3_Index.partial.", s.genome.display())),
        "{stderr}"
    );
}

#[test]
fn a_previous_index_left_by_an_interrupted_rebuild_is_named() {
    let s = setup();
    let old = s.genome.join("Bwamem3_Index.old.999999");
    fs::create_dir(&old).unwrap();
    prep(&s)
        .arg(&s.genome)
        .assert()
        .success()
        .stderr(predicate::str::contains("interrupted rebuild"));
    assert!(old.is_dir());
}

#[test]
fn parallel_zero_fails_before_anything_is_read() {
    let s = setup();
    prep(&s)
        .args(["--parallel", "0"])
        .arg(&s.genome)
        .assert()
        .failure()
        .stderr(predicate::str::contains("1 or more with --bwamem3"));
    assert!(!s.argv.exists(), "index ran");
    assert_eq!(sorted_names(&s.genome), ["chrA.fa", "chrB.fa"]);
}

/// A complete, checked build survives a failed swap; the error and the next run say where it is.
#[test]
fn a_verified_build_is_kept_when_it_cannot_be_moved_into_place() {
    use std::os::unix::fs::PermissionsExt;
    let s = setup();
    let probe = s.genome.join("probe");
    fs::set_permissions(&s.genome, fs::Permissions::from_mode(0o555)).unwrap();
    let root = fs::write(&probe, b"").is_ok();
    fs::set_permissions(&s.genome, fs::Permissions::from_mode(0o755)).unwrap();
    if root {
        fs::remove_file(&probe).unwrap();
        eprintln!("skipping: permission bits do not stop this user (root)");
        return;
    }
    let old = s.genome.join("Bwamem3_Index");
    fs::create_dir(&old).unwrap();
    fs::write(old.join("keep"), b"").unwrap();
    let out = prep(&s)
        .env("FAKE_LOCK_GENOME", "1")
        .arg(&s.genome)
        .assert()
        .failure()
        .stderr(predicate::str::contains("is complete in"))
        .stderr(predicate::str::contains("Replace"))
        .stderr(predicate::str::contains("could not build").not());
    fs::set_permissions(&s.genome, fs::Permissions::from_mode(0o755)).unwrap();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).into_owned();
    assert_eq!(sorted_names(&old), ["keep"], "previous index untouched");
    let partial: Vec<String> = sorted_names(&s.genome)
        .into_iter()
        .filter(|n| n.starts_with("Bwamem3_Index.partial."))
        .collect();
    assert_eq!(partial.len(), 1, "{stderr}");
    assert!(stderr.contains(&partial[0]), "{stderr}");
    assert_eq!(sorted_names(&s.genome.join(&partial[0])), seven_files());

    // The next run names it as a finished index and leaves it alone.
    prep(&s)
        .arg(&s.genome)
        .assert()
        .success()
        .stderr(predicate::str::contains("is a complete bwa-mem3 index"));
    assert!(s.genome.join(&partial[0]).is_dir());
}

#[test]
fn whitespace_in_sequence_lines_fails_before_indexing() {
    for (content, want) in [
        (&b">chrC\r\nACGT\r\r\n"[..], "a carriage return"),
        (&b">chrC\nAC GT\n"[..], "a space"),
        (&b">chrC\nAC\tGT\n"[..], "a tab"),
    ] {
        let s = setup();
        fs::write(s.genome.join("chrC.fa"), content).unwrap();
        prep(&s)
            .arg(&s.genome)
            .assert()
            .failure()
            .stderr(predicate::str::contains(format!(
                "chrC.fa: contig chrC, line 2: the sequence contains {want}"
            )));
        assert!(!s.argv.exists(), "{want}: index ran");
        assert!(
            sorted_names(&s.genome)
                .iter()
                .all(|n| !n.starts_with("Bwamem3_Index")),
            "{want}"
        );
    }
}
