//! Hermetic `--illumina_5base --bwamem3` tests against a fake `bwa-mem3` script.
//!
//! The fake answers `version` with `0.16.0` and otherwise prints a SAM header plus one
//! R1 (FLAG 99) / R2 (FLAG 147) pair per FASTQ read pair, all mapped to `chr1:1`.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn bin() -> Command {
    Command::cargo_bin("bismark").unwrap()
}

#[cfg(unix)]
fn write_exec(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A genome folder with one contig, `chr1` = `ACGTACGT`.
fn make_genome(dir: &Path) {
    fs::write(dir.join("genome.fa"), b">chr1\nACGTACGT\n").unwrap();
}

/// A stub bwa-mem3 meth index at `prefix`: seven empty files plus an `.ann` describing `contigs`.
fn make_stub_bwamem3_index(prefix: &Path, contigs: &[(&str, u64)]) {
    for s in [
        "amb",
        "pac",
        "bwt.2bit.64",
        "meth.amb",
        "meth.ann",
        "meth.pac",
        "meth.bwt.2bit.64",
    ] {
        fs::write(format!("{}.{s}", prefix.display()), b"").unwrap();
    }
    let total: u64 = contigs.iter().map(|c| c.1).sum();
    let mut ann = format!("{total} {} 11\n", contigs.len());
    let mut off = 0;
    for (name, len) in contigs {
        ann.push_str(&format!("0 {name} (null)\n{off} {len} 0\n"));
        off += len;
    }
    fs::write(format!("{}.ann", prefix.display()), ann).unwrap();
}

/// The fake engine. `sq` is the literal `@SQ` header text it prints. `$FAKE_ENV_DUMP` and
/// `$FAKE_ARGV_DUMP`, when set, receive its environment and argv.
#[cfg(unix)]
fn make_fake_bwamem3(dir: &Path, sq: &str) {
    let script = format!(
        r#"#!/bin/sh
[ "$1" = version ] && {{ echo 0.16.0; echo "Compiler: gcc 15.2.0"; exit 0; }}
[ -n "$FAKE_ENV_DUMP" ] && env > "$FAKE_ENV_DUMP"
[ -n "$FAKE_ARGV_DUMP" ] && echo "$@" > "$FAKE_ARGV_DUMP"
n=$#
eval r1=\${{$((n-1))}}
printf '@HD\tVN:1.5\n{sq}@PG\tID:bwa-mem3\tPN:bwa-mem3\tVN:0.16.0\n'
awk 'NR%4==1 {{ id=$1; sub(/^@/,"",id) }}
     NR%4==2 {{
       print id "\t99\tchr1\t1\t60\t6M\t=\t1\t6\t" $0 "\tFFFFFF\tAS:i:6"
       print id "\t147\tchr1\t1\t60\t6M\t=\t1\t-6\t" $0 "\tFFFFFF\tAS:i:6"
     }}' "$r1"
"#
    );
    write_exec(&dir.join("bwa-mem3"), &script);
}

/// `@SQ` text matching [`make_genome`].
#[allow(dead_code)]
const SQ_OK: &str = "@SQ\\tSN:chr1\\tLN:8\\n";

/// Paths for one fake-engine run: genome, index prefix, fake-binary dir, reads and outputs.
#[allow(dead_code)]
struct FakeRun {
    genome: TempDir,
    bins: TempDir,
    out: TempDir,
    index: PathBuf,
    r1: PathBuf,
    r2: PathBuf,
}

/// Lay out a fake run with the given reads (one FASTQ per mate), index contigs and `@SQ` text.
#[cfg(unix)]
#[allow(dead_code)]
fn fake_run(reads: &str, contigs: &[(&str, u64)], sq: &str) -> FakeRun {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let index = genome.path().join("idx.fa");
    make_stub_bwamem3_index(&index, contigs);
    let bins = TempDir::new().unwrap();
    make_fake_bwamem3(bins.path(), sq);
    let r1 = genome.path().join("r_1.fq");
    let r2 = genome.path().join("r_2.fq");
    fs::write(&r1, reads).unwrap();
    fs::write(&r2, reads).unwrap();
    FakeRun {
        genome,
        bins,
        out: TempDir::new().unwrap(),
        index,
        r1,
        r2,
    }
}

/// The `bismark --illumina_5base --bwamem3` command for `run`, before any extra arguments.
#[allow(dead_code)]
fn bwamem3_cmd(run: &FakeRun) -> Command {
    let mut c = bin();
    c.arg("--illumina_5base")
        .arg("--bwamem3")
        .arg("--five_base_index")
        .arg(&run.index)
        .arg("--path_to_bwamem3")
        .arg(run.bins.path())
        .arg("--genome")
        .arg(run.genome.path())
        .arg("--output_dir")
        .arg(run.out.path())
        .arg("-1")
        .arg(&run.r1)
        .arg("-2")
        .arg(&run.r2);
    c
}

/// One read pair whose genome window is `ACGTAC`: the CpG C at 2 reads T (methylated), at 6 C.
const ONE_PAIR: &str = "@p\nATGTAC\n+\nIIIIII\n";

/// Through `resolve()`: the report carries the bwa-mem3 string, never the Bowtie 2 one.
#[cfg(unix)]
#[test]
fn bwamem3_report_carries_the_bwamem3_option_string_not_bowtie2() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    bwamem3_cmd(&run)
        .args(["--multicore", "4"])
        .assert()
        .success();
    let rep = fs::read_to_string(run.out.path().join("r_1_bismark_bwamem3_PE_report.txt")).unwrap();
    assert!(
        rep.contains("Bismark was run with bwa-mem3 against"),
        "{rep}"
    );
    assert!(
        rep.contains(
            "mem --meth=5base --meth-scoring neutral --meth-context all \
             --meth-seed-prune=spec30 --meth-tags none -K 100000000 -t 4"
        ),
        "{rep}"
    );
    assert!(
        !rep.contains("--score-min") && !rep.contains("-q "),
        "Bowtie 2 options leaked: {rep}"
    );
}

#[cfg(unix)]
#[test]
fn bwamem3_child_env_has_no_seed_prune_override() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    let dump = run.out.path().join("env.txt");
    bwamem3_cmd(&run)
        .env("BWAMEM3_METH_SEED_PRUNE", "off")
        .env("FAKE_ENV_DUMP", &dump)
        .assert()
        .success()
        .stderr(predicate::str::contains("BWAMEM3_METH_SEED_PRUNE is set"));
    let env = fs::read_to_string(&dump).unwrap();
    assert!(
        !env.contains("BWAMEM3_METH_SEED_PRUNE"),
        "the override reached the engine"
    );
}

#[test]
fn bwamem3_flags_are_in_help() {
    bin()
        .args(["align", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--bwamem3"))
        .stdout(predicate::str::contains("--path_to_bwamem3"));
}

/// Same contig name, different length (hg19-vs-hg38 style): fail before any alignment.
#[cfg(unix)]
#[test]
fn bwamem3_wrong_assembly_index_fails_before_alignment() {
    let run = fake_run(ONE_PAIR, &[("chr1", 9)], SQ_OK);
    let argv = run.out.path().join("argv.txt");
    bwamem3_cmd(&run)
        .env("FAKE_ARGV_DUMP", &argv)
        .assert()
        .failure()
        .stderr(predicate::str::contains("chr1:9"))
        .stderr(predicate::str::contains("chr1:8"));
    assert!(!argv.exists(), "the engine was spawned");
    assert!(!run.out.path().join("r_1_bismark_bwamem3_pe.bam").exists());
}

/// The engine's own `@SQ` must match the genome too (an index built from another FASTA).
#[cfg(unix)]
#[test]
fn bwamem3_engine_sq_mismatch_fails() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], "@SQ\\tSN:chr1\\tLN:9\\n");
    bwamem3_cmd(&run)
        .assert()
        .failure()
        .stderr(predicate::str::contains("bwa-mem3 output header"));
}

/// Replace the fake engine with one whose alignment output is `body`, a shell fragment run
/// after the header; `$r1` holds the R1 FASTQ path.
#[cfg(unix)]
fn custom_fake(run: &FakeRun, body: &str) {
    let script = format!(
        r#"#!/bin/sh
[ "$1" = version ] && {{ echo 0.16.0; exit 0; }}
n=$#
eval r1=\${{$((n-1))}}
printf '@HD\tVN:1.5\n@SQ\tSN:chr1\tLN:8\n'
{body}
"#
    );
    write_exec(&run.bins.path().join("bwa-mem3"), &script);
}

/// One mapped 99/147 pair per R1 read, as awk.
const PAIRS_AWK: &str = r#"awk 'NR%4==1 { id=$1; sub(/^@/,"",id) }
     NR%4==2 {
       print id "\t99\tchr1\t1\t60\t6M\t=\t1\t6\t" $0 "\tFFFFFF\tAS:i:6"
       print id "\t147\tchr1\t1\t60\t6M\t=\t1\t-6\t" $0 "\tFFFFFF\tAS:i:6"
     }' "$r1""#;

fn many_pairs(n: usize) -> String {
    (0..n)
        .map(|i| format!("@p{i}\nATGTAC\n+\nIIIIII\n"))
        .collect()
}

/// An engine writing far more than a pipe buffer must not hang an `--upto` run.
#[cfg(unix)]
#[test]
fn bwamem3_upto_terminates_with_a_large_engine_output() {
    let run = fake_run(&many_pairs(5000), &[("chr1", 8)], SQ_OK);
    custom_fake(&run, PAIRS_AWK);
    bwamem3_cmd(&run)
        .args(["--upto", "10"])
        .timeout(std::time::Duration::from_secs(60))
        .assert()
        .success();
    let rep = fs::read_to_string(run.out.path().join("r_1_bismark_bwamem3_PE_report.txt")).unwrap();
    assert!(
        rep.contains("Sequence pairs analysed in total:\t10\n"),
        "{rep}"
    );
}

#[cfg(unix)]
#[test]
fn bwamem3_trailing_records_after_last_pair_are_an_error() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    let body = format!(
        "{PAIRS_AWK}\nprintf 'extra\\t99\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t6\\tATGTAC\\tIIIIII\\n\
         extra\\t147\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t-6\\tATGTAC\\tIIIIII\\n'"
    );
    custom_fake(&run, &body);
    bwamem3_cmd(&run)
        .assert()
        .failure()
        .stderr(predicate::str::contains("more records than read pairs"));
}

#[cfg(unix)]
#[test]
fn bwamem3_r2_with_fewer_reads_than_r1_is_an_error() {
    let run = fake_run(&many_pairs(3), &[("chr1", 8)], SQ_OK);
    fs::write(&run.r2, many_pairs(2)).unwrap();
    custom_fake(&run, PAIRS_AWK);
    bwamem3_cmd(&run)
        .assert()
        .failure()
        .stderr(predicate::str::contains("fewer reads"));
}

/// An engine killed mid-stream (e.g. out of memory) is reported with its exit status.
#[cfg(unix)]
#[test]
fn bwamem3_killed_engine_reports_exit_status() {
    let run = fake_run(&many_pairs(3), &[("chr1", 8)], SQ_OK);
    let body = format!("{PAIRS_AWK} | head -n 2\nkill -9 $$");
    custom_fake(&run, &body);
    bwamem3_cmd(&run)
        .assert()
        .failure()
        .stderr(predicate::str::contains("bwa-mem3"))
        .stderr(predicate::str::contains("signal: 9").or(predicate::str::contains("137")));
}

#[cfg(unix)]
#[test]
fn bwamem3_notice_names_bwamem3() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    bwamem3_cmd(&run)
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "aligned with bwa-mem3 to the UNCONVERTED",
        ))
        .stderr(predicate::str::contains("aligned with minimap2").not());
}
