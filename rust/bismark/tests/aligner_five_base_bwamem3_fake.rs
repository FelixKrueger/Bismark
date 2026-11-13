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
        .stderr(predicate::str::contains("bwa-mem3 exited with signal 9"))
        .stderr(predicate::str::contains("ran out of memory"));
}

/// The likeliest crash, running out of memory while loading the index, happens before the header.
#[cfg(unix)]
#[test]
fn bwamem3_killed_before_the_header_is_an_engine_failure_not_an_index_mismatch() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    write_exec(
        &run.bins.path().join("bwa-mem3"),
        "#!/bin/sh\n[ \"$1\" = version ] && { echo 0.16.0; exit 0; }\nkill -9 $$\n",
    );
    bwamem3_cmd(&run)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "bwa-mem3 ended before writing its SAM header",
        ))
        .stderr(predicate::str::contains("bwa-mem3 exited with signal 9"))
        .stderr(predicate::str::contains("does not match").not());
}

/// A plain non-zero exit names the status and gives no out-of-memory hint.
#[cfg(unix)]
#[test]
fn bwamem3_nonzero_exit_names_the_status_without_an_oom_hint() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    custom_fake(&run, &format!("{PAIRS_AWK}\nexit 3"));
    bwamem3_cmd(&run)
        .assert()
        .failure()
        .stderr(predicate::str::contains("bwa-mem3 exited with status 3"))
        .stderr(predicate::str::contains("exit status:").not())
        .stderr(predicate::str::contains("out of memory").not());
}

#[cfg(unix)]
#[test]
fn bwamem3_r2_with_more_reads_than_r1_is_an_error() {
    let run = fake_run(&many_pairs(2), &[("chr1", 8)], SQ_OK);
    fs::write(&run.r2, many_pairs(3)).unwrap();
    custom_fake(&run, PAIRS_AWK);
    bwamem3_cmd(&run)
        .assert()
        .failure()
        .stderr(predicate::str::contains("has more reads than"));
}

#[cfg(unix)]
#[test]
fn bwamem3_empty_version_output_is_named() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    write_exec(&run.bins.path().join("bwa-mem3"), "#!/bin/sh\nexit 0\n");
    bwamem3_cmd(&run)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "`bwa-mem3 version` printed no version",
        ));
}

/// The override notice is printed once per run, not once per mate-file pair.
#[cfg(unix)]
#[test]
fn bwamem3_seed_prune_notice_is_printed_once_per_run() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    let (r1b, r2b) = (
        run.genome.path().join("s_1.fq"),
        run.genome.path().join("s_2.fq"),
    );
    fs::write(&r1b, ONE_PAIR).unwrap();
    fs::write(&r2b, ONE_PAIR).unwrap();
    let out = bin()
        .env("BWAMEM3_METH_SEED_PRUNE", "off")
        .args(["--illumina_5base", "--bwamem3", "--five_base_index"])
        .arg(&run.index)
        .arg("--path_to_bwamem3")
        .arg(run.bins.path())
        .arg("--genome")
        .arg(run.genome.path())
        .arg("--output_dir")
        .arg(run.out.path())
        .arg("-1")
        .arg(format!("{},{}", run.r1.display(), r1b.display()))
        .arg("-2")
        .arg(format!("{},{}", run.r2.display(), r2b.display()))
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert_eq!(
        stderr.matches("BWAMEM3_METH_SEED_PRUNE is set").count(),
        1,
        "{stderr}"
    );
}

/// A bwa-mem3 index left in the genome folder would be read as genome; it is named instead.
#[cfg(unix)]
#[test]
fn bwamem3_index_fasta_in_the_genome_folder_is_named() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    fs::write(
        run.genome.path().join("genome.fa.meth.fa"),
        ">chr1\nATATATAT\n",
    )
    .unwrap();
    bwamem3_cmd(&run)
        .assert()
        .failure()
        .stderr(predicate::str::contains("genome.fa.meth.fa"))
        .stderr(predicate::str::contains("outside the --genome folder"));
}

/// Every pair lands in one standard report bucket, so they add up to the pairs analysed.
#[cfg(unix)]
#[test]
fn bwamem3_report_buckets_add_up_with_rejected_pairs() {
    let reads = "@good\nATGTAC\n+\nIIIIII\n@unmapped\nATGTAC\n+\nIIIIII\n\
                 @samestrand\nATGTAC\n+\nIIIIII\n@outward\nATGTAC\n+\nIIIIII\n";
    let run = fake_run(reads, &[("chr1", 8)], SQ_OK);
    custom_fake(
        &run,
        "printf 'good\\t99\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t6\\tATGTAC\\tIIIIII\\tAS:i:6\\n\
         good\\t147\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t-6\\tATGTAC\\tIIIIII\\tAS:i:6\\n\
         unmapped\\t73\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t0\\tATGTAC\\tIIIIII\\n\
         unmapped\\t133\\tchr1\\t1\\t0\\t*\\t=\\t1\\t0\\tATGTAC\\tIIIIII\\n\
         samestrand\\t67\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t6\\tATGTAC\\tIIIIII\\tAS:i:6\\n\
         samestrand\\t131\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t-6\\tATGTAC\\tIIIIII\\tAS:i:6\\n\
         outward\\t99\\tchr1\\t5\\t60\\t4M2S\\t=\\t1\\t0\\tATGTAC\\tIIIIII\\tAS:i:4\\n\
         outward\\t147\\tchr1\\t1\\t60\\t4S2M\\t=\\t5\\t0\\tATGTAC\\tIIIIII\\tAS:i:2\\n'",
    );
    bwamem3_cmd(&run).assert().success();
    let rep = fs::read_to_string(run.out.path().join("r_1_bismark_bwamem3_PE_report.txt")).unwrap();
    let n = |label: &str| -> u64 {
        rep.lines()
            .find(|l| l.starts_with(label))
            .and_then(|l| l.rsplit('\t').next())
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or_else(|| panic!("no '{label}' line:\n{rep}"))
    };
    let analysed = n("Sequence pairs analysed in total:");
    let unique = n("Number of paired-end alignments with a unique best hit:");
    let none = n("Sequence pairs with no alignments under any condition:");
    let ambiguous = n("Sequence pairs did not map uniquely:");
    let discarded =
        n("Sequence pairs which were discarded because genomic sequence could not be extracted:");
    assert_eq!((analysed, unique, none), (4, 1, 3), "{rep}");
    assert_eq!(unique + none + ambiguous + discarded, analysed, "{rep}");
    assert_eq!(n("Proper pairs rejected"), 2, "{rep}");
    assert_eq!(n("Dovetail overhang bases not called:"), 0, "{rep}");
}

/// A rejected pair does not claim a UMI key, so a later good pair with the same key is kept.
#[cfg(unix)]
#[test]
fn bwamem3_rejected_pair_does_not_make_a_good_pair_a_umi_duplicate() {
    let reads = "@bad\nATGTAC\n+\nIIIIII\n@good\nATGTAC\n+\nIIIIII\n";
    let run = fake_run(reads, &[("chr1", 8)], SQ_OK);
    custom_fake(
        &run,
        "printf 'bad\\t67\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t6\\tATGTAC\\tIIIIII\\tAS:i:6\\n\
         bad\\t131\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t-6\\tATGTAC\\tIIIIII\\tAS:i:6\\n\
         good\\t99\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t6\\tATGTAC\\tIIIIII\\tAS:i:6\\n\
         good\\t147\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t-6\\tATGTAC\\tIIIIII\\tAS:i:6\\n'",
    );
    bwamem3_cmd(&run)
        .args(["--five_base_umi_len", "2"])
        .assert()
        .success()
        .stderr(predicate::str::contains("removed 0 duplicate pair(s)"));
    assert_eq!(bam_records(&run).len(), 2);
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

fn read_bam_header(path: &Path) -> noodles_sam::Header {
    let file = fs::File::open(path).unwrap();
    noodles_bam::io::Reader::new(std::io::BufReader::new(file))
        .read_header()
        .unwrap()
}

fn header_text(header: &noodles_sam::Header) -> String {
    let mut buf = Vec::new();
    noodles_sam::io::Writer::new(&mut buf)
        .write_header(header)
        .unwrap();
    String::from_utf8(buf).unwrap()
}

#[cfg(unix)]
#[test]
fn bwamem3_bam_header_has_engine_pg_and_stays_paired() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    bwamem3_cmd(&run).assert().success();
    let header = read_bam_header(&run.out.path().join("r_1_bismark_bwamem3_pe.bam"));
    let text = header_text(&header);
    let pg = text
        .lines()
        .find(|l| l.starts_with("@PG\tID:bwa-mem3\t"))
        .unwrap_or_else(|| panic!("no bwa-mem3 @PG:\n{text}"));
    assert!(pg.contains("\tPN:bwa-mem3"), "{pg}");
    assert!(pg.contains("\tPP:Bismark"), "{pg}");
    assert!(pg.contains("\tVN:0.16.0"), "{pg}");
    assert!(pg.contains("\tCL:bwa-mem3 mem --meth=5base "), "{pg}");
    assert!(text.contains("@PG\tID:Bismark\t"), "{text}");
    assert_eq!(bismark::io::detect_paired_from_header(&header), Some(true));
    let rep = fs::read_to_string(run.out.path().join("r_1_bismark_bwamem3_PE_report.txt")).unwrap();
    assert!(rep.contains("bwa-mem3 version: 0.16.0\n"), "{rep}");
}

fn bam_records(run: &FakeRun) -> Vec<bismark::io::BismarkRecord> {
    let path = run.out.path().join("r_1_bismark_bwamem3_pe.bam");
    let mut reader = bismark::io::BamReader::from_path(&path).unwrap();
    reader.records().map(|r| r.unwrap()).collect()
}

fn report(run: &FakeRun) -> String {
    fs::read_to_string(run.out.path().join("r_1_bismark_bwamem3_PE_report.txt")).unwrap()
}

/// A read T at the genomic CpG C (position 2) is methylated in 5-Base polarity.
#[cfg(unix)]
#[test]
fn bwamem3_end_to_end_inverts_polarity() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    bwamem3_cmd(&run).assert().success();
    let recs = bam_records(&run);
    assert_eq!(recs.len(), 2);
    let r1 = recs
        .iter()
        .find(|r| u16::from(r.inner().flags()) & 0x40 != 0)
        .expect("R1");
    assert_eq!(bismark::io::tags::xm(r1.inner().data()).unwrap(), b".Z...z");
}

#[cfg(unix)]
#[test]
fn bwamem3_supplementary_between_mates_is_skipped() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    custom_fake(
        &run,
        "printf 'p\\t99\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t6\\tATGTAC\\tFFFFFF\\tAS:i:6\\n\
         p\\t2147\\tchr1\\t3\\t60\\t4M2S\\t=\\t1\\t6\\tATGTAC\\tFFFFFF\\tAS:i:4\\n\
         p\\t147\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t-6\\tATGTAC\\tFFFFFF\\tAS:i:6\\n'",
    );
    bwamem3_cmd(&run).assert().success();
    assert_eq!(bam_records(&run).len(), 2);
}

#[cfg(unix)]
#[test]
fn bwamem3_unmapped_mate_is_dropped() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    custom_fake(
        &run,
        "printf 'p\\t73\\tchr1\\t1\\t60\\t6M\\t=\\t1\\t0\\tATGTAC\\tFFFFFF\\tAS:i:6\\n\
         p\\t133\\tchr1\\t1\\t0\\t*\\t=\\t1\\t0\\tATGTAC\\tFFFFFF\\n'",
    );
    bwamem3_cmd(&run).assert().success();
    assert!(bam_records(&run).is_empty());
    let rep = report(&run);
    assert!(
        rep.contains("Sequence pairs with no alignments under any condition:\t1\n"),
        "{rep}"
    );
}

#[cfg(unix)]
#[test]
fn bwamem3_empty_input_writes_empty_outputs() {
    let run = fake_run("", &[("chr1", 8)], SQ_OK);
    bwamem3_cmd(&run).assert().success();
    assert!(bam_records(&run).is_empty());
    let rep = report(&run);
    assert!(
        rep.contains("Sequence pairs analysed in total:\t0\n"),
        "{rep}"
    );
}

#[cfg(unix)]
#[test]
fn bwamem3_missing_meth_files_fail_before_spawn() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    fs::remove_file(format!("{}.meth.bwt.2bit.64", run.index.display())).unwrap();
    let dump = run.out.path().join("argv.txt");
    bwamem3_cmd(&run)
        .env("FAKE_ARGV_DUMP", &dump)
        .assert()
        .failure()
        .stderr(predicate::str::contains("meth.bwt.2bit.64"));
    assert!(!dump.exists(), "bwa-mem3 was spawned");
}

/// An index from nf-core's `bwamem3/index --meth` (or methylseq's `--bwamem3_index`) has no plain
/// `.bwt.2bit.64`, which `mem --meth` never reads.
#[cfg(unix)]
#[test]
fn bwamem3_index_without_the_plain_bwt_runs() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    fs::remove_file(format!("{}.bwt.2bit.64", run.index.display())).unwrap();
    bwamem3_cmd(&run).assert().success();
    assert_eq!(bam_records(&run).len(), 2);
}

/// The plain `.ann` is still required: `mem` opens it, and Bismark checks contigs against it.
#[cfg(unix)]
#[test]
fn bwamem3_index_without_the_plain_ann_fails_before_spawn() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    fs::remove_file(format!("{}.ann", run.index.display())).unwrap();
    let dump = run.out.path().join("argv.txt");
    bwamem3_cmd(&run)
        .env("FAKE_ARGV_DUMP", &dump)
        .assert()
        .failure()
        .stderr(predicate::str::contains("idx.fa.ann is missing"));
    assert!(!dump.exists(), "bwa-mem3 was spawned");
}

/// 5-Base is concordance-validated, so another bwa-mem3 version warns without promising byte-identity.
#[cfg(unix)]
#[test]
fn bwamem3_other_version_warns_and_runs() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    let fake = run.bins.path().join("bwa-mem3");
    let script = fs::read_to_string(&fake)
        .unwrap()
        .replace("echo 0.16.0", "echo 0.17.0");
    write_exec(&fake, &script);
    bwamem3_cmd(&run)
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Warning: detected bwa-mem3 0.17.0; Bismark's 5-Base engine is validated against 0.16.0.",
        ))
        .stderr(predicate::str::contains("byte-identity").not());
}

/// `bismark --taps` for `run`, before any extra arguments.
#[cfg(unix)]
fn taps_cmd(run: &FakeRun) -> Command {
    let mut c = bin();
    c.arg("--taps")
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

#[test]
fn taps_flag_is_in_help_and_marker_is_not() {
    let out = bin().args(["align", "--help"]).output().unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("--taps"), "{help}");
    assert!(help.contains("TAPS (mC->T)"), "{help}");
    assert!(!help.contains("taps_expanded"), "{help}");
    bin()
        .args(["align", "--taps_expanded"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unexpected argument '--taps_expanded'",
        ));
}

#[cfg(unix)]
#[test]
fn taps_report_has_chemistry_line_after_version() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    taps_cmd(&run)
        .assert()
        .success()
        .stderr(predicate::str::contains(
            ">>> Writing TAPS mapping results to ",
        ));
    let rep = report(&run);
    assert!(
        rep.contains("bwa-mem3 version: 0.16.0\nLibrary chemistry: TAPS (mC->T)\n\n"),
        "{rep}"
    );
    assert!(
        rep.contains("Bismark was run with bwa-mem3 against the unconverted genome"),
        "{rep}"
    );
}

#[cfg(unix)]
#[test]
fn taps_passes_meth_taps_and_pg_records_it() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    let argv = run.out.path().join("argv.txt");
    taps_cmd(&run)
        .env("FAKE_ARGV_DUMP", &argv)
        .assert()
        .success();
    let a = fs::read_to_string(&argv).unwrap();
    assert!(a.starts_with("mem --meth=taps "), "{a}");
    let text = header_text(&read_bam_header(
        &run.out.path().join("r_1_bismark_bwamem3_pe.bam"),
    ));
    let bwa = text
        .lines()
        .find(|l| l.starts_with("@PG\tID:bwa-mem3\t"))
        .unwrap_or_else(|| panic!("no bwa-mem3 @PG:\n{text}"));
    assert!(bwa.contains("\tCL:bwa-mem3 mem --meth=taps "), "{bwa}");
    let bis = text
        .lines()
        .find(|l| l.starts_with("@PG\tID:Bismark\t"))
        .unwrap_or_else(|| panic!("no Bismark @PG:\n{text}"));
    assert!(bis.contains("--taps"), "{bis}");
    assert!(
        run.out
            .path()
            .join("r_1_bismark_bwamem3_PE_report.txt")
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn taps_records_equal_the_five_base_run() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    taps_cmd(&run).assert().success();
    let taps: Vec<_> = bam_records(&run)
        .iter()
        .map(|r| format!("{:?}", r.inner()))
        .collect();
    let twin = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    bwamem3_cmd(&twin).assert().success();
    let five: Vec<_> = bam_records(&twin)
        .iter()
        .map(|r| format!("{:?}", r.inner()))
        .collect();
    assert_eq!(taps.len(), 2);
    assert_eq!(taps, five);
    let rep5 = report(&twin);
    assert!(!rep5.contains("Library chemistry"), "{rep5}");
    assert!(rep5.contains("bwa-mem3 version: 0.16.0\n\n"), "{rep5}");
    assert!(rep5.contains("mem --meth=5base "), "{rep5}");
}

#[cfg(unix)]
#[test]
fn taps_notice_and_summary_say_taps() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    taps_cmd(&run)
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Note: --taps is the TAPS (mC->T) mode",
        ))
        .stderr(predicate::str::contains("TAPS index:"))
        .stderr(predicate::str::contains("--illumina_5base").not())
        .stderr(predicate::str::contains("5-Base").not());
}

#[cfg(unix)]
#[test]
fn taps_rejections_spawn_nothing_and_write_nothing() {
    for (extra, why) in [
        (
            &["--minimap2"][..],
            "--taps aligns with bwa-mem3 only; drop --minimap2.",
        ),
        (
            &["--five_base_consensus_from_bam", "x.bam"][..],
            "--five_base_consensus_from_bam is validated for Illumina 5-Base only, not TAPS.",
        ),
    ] {
        let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
        let argv = run.bins.path().join("argv.txt");
        let out_dir = run.out.path().join("never");
        let mut c = bin();
        c.arg("--taps")
            .args(extra)
            .arg("--five_base_index")
            .arg(&run.index)
            .arg("--path_to_bwamem3")
            .arg(run.bins.path())
            .arg("--genome")
            .arg(run.genome.path())
            .arg("--output_dir")
            .arg(&out_dir)
            .arg("-1")
            .arg(&run.r1)
            .arg("-2")
            .arg(&run.r2)
            .env("FAKE_ARGV_DUMP", &argv);
        c.assert().failure().stderr(predicate::str::contains(why));
        assert!(!argv.exists(), "{extra:?}: the engine was spawned");
        assert!(!out_dir.exists(), "{extra:?}: output dir created");
    }
}

#[cfg(unix)]
#[test]
fn taps_version_warning_names_taps() {
    let run = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    let fake = run.bins.path().join("bwa-mem3");
    let script = fs::read_to_string(&fake)
        .unwrap()
        .replace("echo 0.16.0", "echo 0.17.0");
    write_exec(&fake, &script);
    taps_cmd(&run)
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Warning: detected bwa-mem3 0.17.0; Bismark's TAPS engine is validated against 0.16.0.",
        ));
}

/// bismark2report renders a TAPS report exactly like its 5-Base twin.
#[cfg(unix)]
#[test]
fn taps_report_html_matches_five_base_twin() {
    let html = |run: &FakeRun| {
        Command::cargo_bin("bismark2report")
            .unwrap()
            .current_dir(run.out.path())
            .args([
                "--alignment_report",
                "r_1_bismark_bwamem3_PE_report.txt",
                "--__test_timestamp",
                "0",
            ])
            .assert()
            .success();
        fs::read_to_string(run.out.path().join("r_1_bismark_bwamem3_PE_report.html")).unwrap()
    };
    let norm = |s: String, run: &FakeRun| {
        s.replace(&run.out.path().display().to_string(), "OUT")
            .replace(&run.genome.path().display().to_string(), "G")
            .replace("--meth=taps", "--meth=5base")
    };
    let taps = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    taps_cmd(&taps).assert().success();
    let five = fake_run(ONE_PAIR, &[("chr1", 8)], SQ_OK);
    bwamem3_cmd(&five).assert().success();
    let (t, f) = (norm(html(&taps), &taps), norm(html(&five), &five));
    assert!(t.len() > 1000, "empty HTML");
    assert_eq!(t, f);
}

/// `--five_base_baseq` reaches the TAPS call: a low-quality T at the CpG C is not called.
#[cfg(unix)]
#[test]
fn taps_five_base_baseq_masks_low_quality_calls() {
    let r1 = |recs: &[bismark::io::BismarkRecord]| {
        recs.iter()
            .find(|r| u16::from(r.inner().flags()) & 0x40 != 0)
            .expect("R1")
            .xm()
            .to_vec()
    };
    let unmasked = fake_run("@p\nATGTAC\n+\nI#IIII\n", &[("chr1", 8)], SQ_OK);
    taps_cmd(&unmasked).assert().success();
    assert_eq!(r1(&bam_records(&unmasked)), b".Z...z");
    let masked = fake_run("@p\nATGTAC\n+\nI#IIII\n", &[("chr1", 8)], SQ_OK);
    taps_cmd(&masked)
        .args(["--five_base_baseq", "30"])
        .assert()
        .success();
    assert_eq!(r1(&bam_records(&masked)), b".....z");
}
