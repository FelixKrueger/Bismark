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
    // `--version` → the version banner (Phase-1 detection). Otherwise (alignment)
    // → a SAM header + one unmapped (flag 4) record per input read, read from the
    // `-U` converted file, so the Phase-4 merge has lockstep-matching qnames.
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""
for a in "$@"; do [ "$prev" = "-U" ] && inp="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp"
"#;
    write_exec(&dir.join("bowtie2"), script);
}

fn make_read(dir: &Path) -> std::path::PathBuf {
    let r = dir.join("reads.fq");
    fs::write(&r, b"@r1\nACGTACGT\n+\nIIIIIIII\n").unwrap();
    r
}

/// A fake `bowtie2` that reports a MAPPED alignment on the CT (`BS_CT`) index
/// (OT / index 0) and UNMAPPED on the GA index, so the merge yields a unique
/// best on the OT strand. The CT hit is a 6M alignment at chr1:1 with AS:i:0 /
/// MD:Z:6 — matching a 6 bp read against an 8 bp chr1 (window = read + 2).
#[cfg(unix)]
fn make_fake_bowtie2_mapped(dir: &Path) {
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
  *BS_CT*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t0\tchr1_CT_converted\t1\t42\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6" }' "$inp" ;;
  *)       awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
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
fn hisat2_is_accepted_not_deferred() {
    // --hisat2 is wired as of Phase 2a: selection succeeds and the run proceeds
    // past aligner selection (here failing later, on the missing read file) —
    // it must NOT short-circuit with the old "deferred" message.
    bin()
        .arg("--hisat2")
        .arg("some_genome")
        .arg("some_reads.fq")
        .assert()
        .failure()
        .code(1)
        .stderr(
            predicate::str::contains("does not exist")
                .and(predicate::str::contains("deferred").not()),
        );
}

#[test]
fn minimap2_is_accepted_not_deferred() {
    // --minimap2 is wired as of Phase 4 (SE): selection succeeds and the run
    // proceeds past aligner selection (here failing later, on the missing read
    // file) — it must NOT short-circuit with the old "deferred" message.
    bin()
        .arg("--minimap2")
        .arg("some_genome")
        .arg("some_reads.fq")
        .assert()
        .failure()
        .code(1)
        .stderr(
            predicate::str::contains("does not exist")
                .and(predicate::str::contains("deferred").not()),
        );
}

#[test]
fn minimap2_paired_end_is_rejected() {
    // PE-minimap2 is deferred out of v1.x (no trustworthy Perl oracle): a
    // paired-end --minimap2 run must fail loudly, not silently mis-align.
    let r1 = TempDir::new().unwrap();
    let m1 = r1.path().join("r1.fq");
    let m2 = r1.path().join("r2.fq");
    fs::write(&m1, b"@r/1\nACGT\n+\nIIII\n").unwrap();
    fs::write(&m2, b"@r/2\nACGT\n+\nIIII\n").unwrap();
    bin()
        .arg("--minimap2")
        .arg("some_genome")
        .arg("-1")
        .arg(&m1)
        .arg("-2")
        .arg(&m2)
        .assert()
        .failure()
        .code(1)
        .stderr(
            predicate::str::contains("paired-end")
                .and(predicate::str::contains("minimap2"))
                .and(predicate::str::contains("not supported")),
        );
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
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("resolved configuration")
                .and(predicate::str::contains(
                    "-q --score-min L,0,-0.2 --ignore-quals",
                ))
                .and(predicate::str::contains("single-end"))
                .and(predicate::str::contains("Bowtie 2 2.5.5"))
                // Phase 2: the C->T temp file is produced for the v1 spine.
                .and(predicate::str::contains("Created C->T converted"))
                // Phase 5: the pipeline ran end-to-end (fake bowtie2 emits unmapped).
                .and(predicate::str::contains("Mapping summary"))
                .and(predicate::str::contains("no alignment found:")),
        );
    // Phase 6: the C->T temp file is DELETED after the run (Perl 1974–1981).
    assert!(!temp.path().join("reads.fq_C_to_T.fastq").is_file());
    // Phase 5: the Bismark BAM was written to --output_dir (header-only here,
    // since the fake aligner reports every read unmapped).
    assert!(outdir.path().join("reads_bismark_bt2.bam").is_file());
    // Phase 6: the alignment report was written.
    assert!(
        outdir
            .path()
            .join("reads_bismark_bt2_SE_report.txt")
            .is_file()
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
    let temp = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(temp.path())
        // --nucleotide_coverage is still deferred (wired in a later phase);
        // --unmapped/--ambiguous/--ambig_bam are now ACTIVE (Phase 6).
        .arg("--nucleotide_coverage")
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("not yet active")
                .and(predicate::str::contains("--nucleotide_coverage")),
        );
}

#[cfg(unix)]
#[test]
fn mapped_read_writes_bam_record_end_to_end() {
    // Full Phase-5 path: a mapped read → genomic extraction → XM call → BAM
    // record, read back via bismark-io (noodles). chr1 is 8 bp; a 6 bp read at
    // pos 1 leaves room for the +2 context window.
    let genome = TempDir::new().unwrap();
    make_genome(genome.path()); // chr1 = ACGTACGT (8 bp)
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_mapped(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nACGTAC\n+\nFFFFFF\n").unwrap(); // 6 bp read
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   1"));

    // Read the written BAM back and assert the full record.
    let bam = outdir.path().join("reads_bismark_bt2.bam");
    assert!(bam.is_file());
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 0); // OT → FLAG 0
    assert_eq!(usize::from(r.alignment_start().unwrap()), 1);
    assert_eq!(u8::from(r.mapping_quality().unwrap()), 42); // calc_mapq(6,_,0,_) top leaf
    assert_eq!(r.sequence().as_ref(), b"ACGTAC"); // original read
    assert_eq!(r.quality_scores().as_ref(), &[37u8; 6]); // 'F'(70) → phred 37

    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    let v = |t: [u8; 2]| r.data().get(&Tag::from(t)).cloned();
    assert_eq!(v(*b"MD"), Some(Value::String("6".into())));
    assert_eq!(v(*b"XM"), Some(Value::String(".Z...Z".into()))); // Cs at pos 1,5 → CpG
    assert_eq!(v(*b"XR"), Some(Value::String("CT".into())));
    assert_eq!(v(*b"XG"), Some(Value::String("CT".into())));
}

/// Like `make_fake_bowtie2_mapped` but the CT hit is at chr1:3 (6M) — so the
/// +2 context window runs off the end of the 8 bp chr1 and the extraction's
/// 3'-edge guard fires (Perl 4390).
#[cfg(unix)]
fn make_fake_bowtie2_edge(dir: &Path) {
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
  *BS_CT*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t0\tchr1_CT_converted\t3\t42\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6" }' "$inp" ;;
  *)       awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

#[cfg(unix)]
#[test]
fn chromosome_edge_read_counted_but_not_written() {
    // §9 #14: a unique-best read whose +2 window falls off the chromosome end is
    // counted (unique_best + could-not-extract) but NOT written → header-only BAM.
    let genome = TempDir::new().unwrap();
    make_genome(genome.path()); // chr1 = ACGTACGT (8 bp)
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_edge(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nACGTAC\n+\nFFFFFF\n").unwrap(); // 6 bp at pos 3 → window needs pos 2..10
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("unique best alignments:   1")
                .and(predicate::str::contains("could not be extracted"))
                .and(predicate::str::contains("could-not-extract genomic:1")),
        );

    // BAM exists but has ZERO alignment records (header only).
    let bam = outdir.path().join("reads_bismark_bt2.bam");
    assert!(bam.is_file());
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    assert_eq!(reader.records().count(), 0);
}

/// CT instance reports a within-thread-ambiguous alignment (AS == XS); GA unmapped.
#[cfg(unix)]
fn make_fake_bowtie2_ambig(dir: &Path) {
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
  *BS_CT*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t0\tchr1_CT_converted\t1\t1\t6M\t*\t0\t0\tACGTAC\tIIIIII\tAS:i:0\tXS:i:0\tMD:Z:6" }' "$inp" ;;
  *)       awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

#[cfg(unix)]
#[test]
fn unmapped_routing_and_report_end_to_end() {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2(bins.path()); // every read unmapped
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nACGTACGT\n+\nIIIIIIII\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("--unmapped")
        .arg(&read)
        .assert()
        .success();

    // The unmapped read landed in the gzipped FastQ (un-stripped basename name).
    let un = outdir.path().join("reads.fq_unmapped_reads.fq.gz");
    assert!(un.is_file());
    let mut s = String::new();
    GzDecoder::new(fs::File::open(&un).unwrap())
        .read_to_string(&mut s)
        .unwrap();
    assert_eq!(s, "@r1\nACGTACGT\n+\nIIIIIIII\n");

    // The alignment report reflects the unmapped read + has the wall-clock line.
    let report = fs::read_to_string(outdir.path().join("reads_bismark_bt2_SE_report.txt")).unwrap();
    assert!(report.contains("Sequences analysed in total:\t1\n"));
    // 1 sequence, 0 unique → 0.0% (the bare "0%" is only the zero-sequences case).
    assert!(report.contains("Mapping efficiency:\t0.0%\n"));
    assert!(report.contains("Sequences with no alignments under any condition:\t1\n"));
    assert!(report.contains("Bismark completed in "));
}

#[cfg(unix)]
#[test]
fn ambiguous_and_ambig_bam_end_to_end() {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_ambig(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nACGTAC\n+\nIIIIII\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("--ambiguous")
        .arg("--ambig_bam")
        .arg(&read)
        .assert()
        .success();

    // The ambiguous read landed in the gzipped ambiguous FastQ.
    let amb = outdir.path().join("reads.fq_ambiguous_reads.fq.gz");
    assert!(amb.is_file());
    let mut s = String::new();
    GzDecoder::new(fs::File::open(&amb).unwrap())
        .read_to_string(&mut s)
        .unwrap();
    assert_eq!(s, "@r1\nACGTAC\n+\nIIIIII\n");

    // The --ambig_bam was produced and is non-empty (within-thread ambiguity).
    let ab = outdir.path().join("reads_bismark_bt2.ambig.bam");
    assert!(ab.is_file());
    assert!(fs::metadata(&ab).unwrap().len() > 0);

    // The report shows the read as "did not map uniquely".
    let report = fs::read_to_string(outdir.path().join("reads_bismark_bt2_SE_report.txt")).unwrap();
    assert!(report.contains("Sequences did not map uniquely:\t1\n"));
}

// ---- paired-end end-to-end (Phase 7) -----------------------------------------

/// A PE fake `bowtie2`: reads the `-1` (CT R1) temp file, derives the base id
/// (stripping the `/1/1` tag we add), and emits TWO SAM lines per pair (R1 `/1`,
/// R2 `/2` — mimicking Bowtie 2 clipping the outer tag). On the CT (`BS_CT`) index
/// it reports a mapped OT pair (flags 99/147 at chr1:1); on GA, an unmapped pair
/// (77/141) → the merge yields a unique best on OT.
#[cfg(unix)]
fn make_fake_bowtie2_pe(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-1" ] && m1="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
case "$idx" in
  *BS_CT*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t99\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      print id "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6" }' "$m1" ;;
  *)       awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI";
      print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$m1" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// A PE fake `bowtie2` that reports every pair UNMAPPED (77/141) on both indexes.
#[cfg(unix)]
fn make_fake_bowtie2_pe_unmapped(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""
for a in "$@"; do [ "$prev" = "-1" ] && m1="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
    print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI";
    print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$m1"
"#;
    write_exec(&dir.join("bowtie2"), script);
}

#[cfg(unix)]
#[test]
fn pe_mapped_writes_two_bam_records_end_to_end() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path()); // chr1 = ACGTACGT (8 bp)
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe(bins.path());
    let r1 = genome.path().join("reads_1.fq");
    let r2 = genome.path().join("reads_2.fq");
    fs::write(&r1, b"@r1\nACGTAC\n+\nFFFFFF\n").unwrap();
    fs::write(&r2, b"@r1\nACGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("paired-end")
                .and(predicate::str::contains("unique best alignments:   1")),
        );

    // The PE BAM (`_pe.bam`) holds BOTH mate records.
    let bam = outdir.path().join("reads_1_bismark_bt2_pe.bam");
    assert!(bam.is_file(), "expected {}", bam.display());
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 2, "two records per pair");
    let (m1, m2) = (recs[0].inner(), recs[1].inner());
    // index 0 (OT) FLAG pair.
    assert_eq!(u16::from(m1.flags()), 99);
    assert_eq!(u16::from(m2.flags()), 147);
    // both at POS 1; RNEXT '=' (mate tid == own tid); PNEXT = the mate's POS.
    assert_eq!(usize::from(m1.alignment_start().unwrap()), 1);
    assert_eq!(usize::from(m2.alignment_start().unwrap()), 1);
    assert_eq!(m1.mate_reference_sequence_id(), m1.reference_sequence_id());
    assert_eq!(usize::from(m1.mate_alignment_start().unwrap()), 1);
    // shared MAPQ.
    assert_eq!(u8::from(m1.mapping_quality().unwrap()), 42);
    assert_eq!(u8::from(m2.mapping_quality().unwrap()), 42);

    // The PE report exists and uses the paired-end wording.
    let report =
        fs::read_to_string(outdir.path().join("reads_1_bismark_bt2_PE_report.txt")).unwrap();
    assert!(report.contains("Bismark report for: "));
    assert!(report.contains("and"));
    assert!(report.contains("Sequence pairs analysed in total:\t1\n"));
    assert!(report.contains("Number of paired-end alignments with a unique best hit:\t1\n"));
}

#[cfg(unix)]
#[test]
fn pe_unmapped_routing_to_1_and_2_files() {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_unmapped(bins.path());
    let r1 = genome.path().join("reads_1.fq");
    let r2 = genome.path().join("reads_2.fq");
    fs::write(&r1, b"@r1\nACGTAC\n+\nFFFFFF\n").unwrap();
    fs::write(&r2, b"@r1\nTGCATG\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("--unmapped")
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success();

    // R1 → _1 file, R2 → _2 file (un-stripped basenames + mate suffix; gzipped).
    let un1 = outdir.path().join("reads_1.fq_unmapped_reads_1.fq.gz");
    let un2 = outdir.path().join("reads_2.fq_unmapped_reads_2.fq.gz");
    assert!(un1.is_file(), "expected {}", un1.display());
    assert!(un2.is_file(), "expected {}", un2.display());
    let read_gz = |p: &Path| {
        let mut s = String::new();
        GzDecoder::new(fs::File::open(p).unwrap())
            .read_to_string(&mut s)
            .unwrap();
        s
    };
    assert_eq!(read_gz(&un1), "@r1\nACGTAC\n+\nFFFFFF\n"); // R1 original (non-uc)
    assert_eq!(read_gz(&un2), "@r1\nTGCATG\n+\nFFFFFF\n"); // R2 original

    // The PE BAM is header-only (no pair mapped).
    let bam = outdir.path().join("reads_1_bismark_bt2_pe.bam");
    assert!(bam.is_file());
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    assert_eq!(reader.records().count(), 0);
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

// ===========================================================================
// Phase 8 — non-directional + pbat (the GA-reads complementary strands).
//
// 🔴 The pre-Phase-8 fakes emit a mapped hit ONLY on `*BS_CT*`, so a non-dir/pbat
// test would silently pass on all-unmapped (both plan reviewers). These fakes map
// the **G→A-converted reads** (`-U`/`-1` ending `_G_to_A`) onto a chosen index, so
// the first-live CTOT/CTOB (SE eff 2/3) and PE index-1/2 paths actually run, and
// we byte-assert FLAG/SEQ/XR/XG/XM. The directional-library oxy gate lands ~0 reads
// on these strands, so these integration tests — not the gate — are the proof.
// ===========================================================================

/// Like [`make_genome`] but with a caller-chosen chr1 sequence (the GA-branch
/// tests need a longer chr1 so the +2 context window fits around the alignment).
#[cfg(unix)]
fn make_genome_chr1(dir: &Path, seq: &[u8]) {
    make_genome(dir);
    let mut fa = b">chr1\n".to_vec();
    fa.extend_from_slice(seq);
    fa.push(b'\n');
    fs::write(dir.join("genome.fa"), fa).unwrap();
}

/// Fake `bowtie2` that maps the read on the **CT index** ONLY when the `-U` reads
/// file is the G→A-converted one (`*_G_to_A*`): pbat SE slot 0 / non-dir SE slot 2
/// → effective index 2 → **CTOT**. chr1:3 6M. Other instances report unmapped.
#[cfg(unix)]
fn make_fake_bowtie2_ga_reads_ct_index(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-U" ] && inp="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
hit=0
case "$idx" in *BS_CT*) case "$inp" in *_G_to_A*) hit=1;; esac;; esac
if [ "$hit" = 1 ]; then
  awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t0\tchr1_CT_converted\t3\t42\t6M\t*\t0\t0\tACATAC\tFFFFFF\tAS:i:0\tMD:Z:6" }' "$inp"
else
  awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp"
fi
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// Fake `bowtie2` that maps the read on the **GA index** ONLY when the `-U` reads
/// file is the G→A-converted one: pbat SE slot 1 / non-dir SE slot 3 → effective
/// index 3 → **CTOB**. chr1:3 6M (RNAME `chr1_GA_converted`).
#[cfg(unix)]
fn make_fake_bowtie2_ga_reads_ga_index(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-U" ] && inp="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
hit=0
case "$idx" in *BS_GA*) case "$inp" in *_G_to_A*) hit=1;; esac;; esac
if [ "$hit" = 1 ]; then
  awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t0\tchr1_GA_converted\t3\t42\t6M\t*\t0\t0\tACATAC\tFFFFFF\tAS:i:0\tMD:Z:6" }' "$inp"
else
  awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp"
fi
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// Read back a SAM string tag from a BAM record.
#[cfg(unix)]
fn rec_tag(r: &noodles_sam::alignment::RecordBuf, tag: [u8; 2]) -> Option<Vec<u8>> {
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    match r.data().get(&Tag::from(tag)) {
        Some(Value::String(s)) => Some(s.to_vec()),
        _ => None,
    }
}

#[cfg(unix)]
#[test]
fn pbat_se_ct_index_writes_ctot_record() {
    // pbat SE: BOTH instances read the G→A file; the CT-index hit lands at eff 2 →
    // CTOT (strand '-', GA/CT → FLAG 0, SEQ revcomp'd, XM reversed).
    let genome = TempDir::new().unwrap();
    make_genome_chr1(genome.path(), b"TTGCGTACTT"); // 10 bp
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_ga_reads_ct_index(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nGCGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--pbat")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   1"));

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 0); // CTOT → FLAG 0
    assert_eq!(usize::from(r.alignment_start().unwrap()), 3);
    assert_eq!(r.sequence().as_ref(), b"GTACGC"); // strand '-' → revcomp(read)
    assert_eq!(rec_tag(r, *b"XR").as_deref(), Some(&b"GA"[..]));
    assert_eq!(rec_tag(r, *b"XG").as_deref(), Some(&b"CT"[..]));
    assert_eq!(rec_tag(r, *b"XM").as_deref(), Some(&b".z...H"[..]));
    // pbat SE temp = the SINGLE G→A file; deleted after the run.
    assert!(!temp.path().join("reads.fq_G_to_A.fastq").is_file());
}

#[cfg(unix)]
#[test]
fn pbat_se_ga_index_writes_ctob_record() {
    // pbat SE: the GA-index hit lands at eff 3 → CTOB (strand '+', GA/GA → FLAG 16,
    // SEQ/XM NOT reoriented).
    let genome = TempDir::new().unwrap();
    make_genome_chr1(genome.path(), b"TTGCGTACTT");
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_ga_reads_ga_index(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nGCGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--pbat")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   1"));

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 16); // CTOB → FLAG 16
    assert_eq!(usize::from(r.alignment_start().unwrap()), 3);
    assert_eq!(r.sequence().as_ref(), b"GCGTAC"); // strand '+' → original read
    assert_eq!(rec_tag(r, *b"XR").as_deref(), Some(&b"GA"[..]));
    assert_eq!(rec_tag(r, *b"XG").as_deref(), Some(&b"GA"[..]));
    assert_eq!(rec_tag(r, *b"XM").as_deref(), Some(&b"H.Z..."[..]));
}

#[cfg(unix)]
#[test]
fn nondir_se_four_instances_ctot_no_rejection() {
    // non-dir SE spawns 4 instances (slots 0–3). The CT-index/G→A-reads hit lands
    // at slot 2 → eff 2 → CTOT — a path directional would REJECT but non-dir keeps
    // (a record is written; nothing rejected). Both C→T and G→A temps are cleaned up.
    let genome = TempDir::new().unwrap();
    make_genome_chr1(genome.path(), b"TTGCGTACTT");
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_ga_reads_ct_index(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nGCGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("unique best alignments:   1")
                .and(predicate::str::contains("directional-rejected:     0")),
        );

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(
        recs.len(),
        1,
        "the complementary-strand read is KEPT (not rejected)"
    );
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 0); // index 2 → CTOT → FLAG 0
    assert_eq!(rec_tag(r, *b"XR").as_deref(), Some(&b"GA"[..]));
    assert_eq!(rec_tag(r, *b"XG").as_deref(), Some(&b"CT"[..]));
    // non-dir SE temps: BOTH C→T and G→A deleted (rev1 A per-mode cleanup).
    assert!(!temp.path().join("reads.fq_C_to_T.fastq").is_file());
    assert!(!temp.path().join("reads.fq_G_to_A.fastq").is_file());
}

#[cfg(unix)]
#[test]
fn nondir_se_ga_index_ctob_record() {
    // non-dir SE: the GA-index/G→A-reads hit lands at slot 3 → eff 3 → CTOB.
    let genome = TempDir::new().unwrap();
    make_genome_chr1(genome.path(), b"TTGCGTACTT");
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_ga_reads_ga_index(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nGCGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   1"));

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 16); // index 3 → CTOB → FLAG 16
    assert_eq!(rec_tag(r, *b"XR").as_deref(), Some(&b"GA"[..]));
    assert_eq!(rec_tag(r, *b"XG").as_deref(), Some(&b"GA"[..]));
}

/// PE fake `bowtie2` that maps a pair on the **GA index** ONLY when `-1` is the
/// G→A-converted R1 (`*_G_to_A*`): pbat slot 1 / non-dir slot 1 → PE index 1 →
/// **CTOB** (FLAG 163/83). Both mates at chr1:5 6M, RNAME `chr1_GA_converted`.
#[cfg(unix)]
fn make_fake_bowtie2_pe_ga_index(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-1" ] && m1="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
hit=0
case "$idx" in *BS_GA*) case "$m1" in *_G_to_A*) hit=1;; esac;; esac
if [ "$hit" = 1 ]; then
  awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t99\tchr1_GA_converted\t5\t42\t6M\t=\t5\t6\tACATAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      print id "/2\t147\tchr1_GA_converted\t5\t42\t6M\t=\t5\t-6\tACATAC\tFFFFFF\tAS:i:0\tMD:Z:6" }' "$m1"
else
  awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI";
      print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$m1"
fi
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// PE fake `bowtie2` that maps a pair on the **CT index** ONLY when `-1` is the
/// G→A-converted R1: pbat slot 2 / non-dir slot 2 → PE index 2 → **CTOT** (FLAG
/// 147/99). Both mates at chr1:5 6M, RNAME `chr1_CT_converted`.
#[cfg(unix)]
fn make_fake_bowtie2_pe_ct_index_ga_reads(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-1" ] && m1="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
hit=0
case "$idx" in *BS_CT*) case "$m1" in *_G_to_A*) hit=1;; esac;; esac
if [ "$hit" = 1 ]; then
  awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t99\tchr1_CT_converted\t5\t42\t6M\t=\t5\t6\tACATAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      print id "/2\t147\tchr1_CT_converted\t5\t42\t6M\t=\t5\t-6\tACATAC\tFFFFFF\tAS:i:0\tMD:Z:6" }' "$m1"
else
  awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI";
      print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$m1"
fi
"#;
    write_exec(&dir.join("bowtie2"), script);
}

#[cfg(unix)]
#[test]
fn pbat_pe_ga_index_writes_ctob_pair() {
    // pbat PE populates slots 1 (GA idx) + 2 (CT idx); the GA-index hit → PE index 1
    // → CTOB: FLAG pair (163, 83), R1 XR GA / R2 XR CT, XG GA. pbat temps = G→A_1 +
    // C→T_2 (both deleted).
    let genome = TempDir::new().unwrap();
    make_genome_chr1(genome.path(), b"ACGTACGTACGTACGTACGT"); // 20 bp
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_ga_index(bins.path());
    let r1 = genome.path().join("reads_1.fq");
    let r2 = genome.path().join("reads_2.fq");
    fs::write(&r1, b"@r1\nGCGTAC\n+\nFFFFFF\n").unwrap();
    fs::write(&r2, b"@r1\nGCGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--pbat")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   1"));

    let bam = outdir.path().join("reads_1_bismark_bt2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 2);
    let (m1, m2) = (recs[0].inner(), recs[1].inner());
    assert_eq!(u16::from(m1.flags()), 163); // PE index 1 → (163, 83)
    assert_eq!(u16::from(m2.flags()), 83);
    assert_eq!(rec_tag(m1, *b"XR").as_deref(), Some(&b"GA"[..])); // R1 GA
    assert_eq!(rec_tag(m2, *b"XR").as_deref(), Some(&b"CT"[..])); // R2 CT
    assert_eq!(rec_tag(m1, *b"XG").as_deref(), Some(&b"GA"[..])); // XG shared GA
    assert_eq!(rec_tag(m2, *b"XG").as_deref(), Some(&b"GA"[..]));
    assert!(!temp.path().join("reads_1.fq_G_to_A.fastq").is_file());
    assert!(!temp.path().join("reads_2.fq_C_to_T.fastq").is_file());
}

#[cfg(unix)]
#[test]
fn pbat_pe_ct_index_writes_ctot_pair() {
    // pbat PE: the CT-index hit → PE index 2 → CTOT: FLAG pair (147, 99), XG CT.
    let genome = TempDir::new().unwrap();
    make_genome_chr1(genome.path(), b"ACGTACGTACGTACGTACGT");
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_ct_index_ga_reads(bins.path());
    let r1 = genome.path().join("reads_1.fq");
    let r2 = genome.path().join("reads_2.fq");
    fs::write(&r1, b"@r1\nGCGTAC\n+\nFFFFFF\n").unwrap();
    fs::write(&r2, b"@r1\nGCGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--pbat")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   1"));

    let bam = outdir.path().join("reads_1_bismark_bt2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 2);
    let (m1, m2) = (recs[0].inner(), recs[1].inner());
    assert_eq!(u16::from(m1.flags()), 147); // PE index 2 → (147, 99)
    assert_eq!(u16::from(m2.flags()), 99);
    assert_eq!(rec_tag(m1, *b"XG").as_deref(), Some(&b"CT"[..]));
}

#[cfg(unix)]
#[test]
fn nondir_pe_four_slots_index1_no_rejection() {
    // non-dir PE populates ALL 4 slots; the GA-index/G→A-R1 hit lands at slot 1 →
    // PE index 1 → CTOB, KEPT (directional would reject index 1/2). All 4 temps gone.
    let genome = TempDir::new().unwrap();
    make_genome_chr1(genome.path(), b"ACGTACGTACGTACGTACGT");
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_ga_index(bins.path());
    let r1 = genome.path().join("reads_1.fq");
    let r2 = genome.path().join("reads_2.fq");
    fs::write(&r1, b"@r1\nGCGTAC\n+\nFFFFFF\n").unwrap();
    fs::write(&r2, b"@r1\nGCGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("unique best alignments:   1")
                .and(predicate::str::contains("directional-rejected:     0")),
        );

    let bam = outdir.path().join("reads_1_bismark_bt2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 2, "the index-1 pair is KEPT (not rejected)");
    assert_eq!(u16::from(recs[0].inner().flags()), 163); // index 1 → (163, 83)
    assert_eq!(u16::from(recs[1].inner().flags()), 83);
    // non-dir PE temps: all 4 (C→T_1, G→A_1, C→T_2, G→A_2) deleted.
    assert!(!temp.path().join("reads_1.fq_C_to_T.fastq").is_file());
    assert!(!temp.path().join("reads_1.fq_G_to_A.fastq").is_file());
    assert!(!temp.path().join("reads_2.fq_C_to_T.fastq").is_file());
    assert!(!temp.path().join("reads_2.fq_G_to_A.fastq").is_file());
}

// ===========================================================================
// Phase 9a — FastA input (2-line records, synthesized Phred-40 QUAL).
//
// 🔴 The Phase-8 fakes parse the converted file with `awk 'NR%4==1 …
// sub(/^@/,…)'` — the 4-line FastQ shape. Fed a 2-line `.fa` they skip every
// other read and keep the `>`, so a FastA test would false-pass on all-unmapped
// (rev1 B C-1). These FastA-aware fakes use `NR%2==1` + `sub(/^>/,…)`. Every
// test byte-asserts the BAM record incl. **QUAL == Phred 40** (`'I'×len`, Perl
// check_results_*_end 2707/3271) — the FastA-specific proof.
// ===========================================================================

/// SE FastA fake: maps on the CT index (`>id` 2-line records), unmapped on GA.
#[cfg(unix)]
fn make_fake_bowtie2_fasta_mapped(dir: &Path) {
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
  *BS_CT*) awk 'NR%2==1 { id=$1; sub(/^>/,"",id); print id "\t0\tchr1_CT_converted\t1\t42\t6M\t*\t0\t0\tACGTAC\tIIIIII\tAS:i:0\tMD:Z:6" }' "$inp" ;;
  *)       awk 'NR%2==1 { id=$1; sub(/^>/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// SE FastA fake: maps on the GA index ONLY when `-U` is the G→A-converted `.fa`
/// (pbat slot 1 / non-dir slot 3 → effective index 3 → CTOB). chr1:3 6M.
#[cfg(unix)]
fn make_fake_bowtie2_fasta_ga_index(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-U" ] && inp="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
hit=0
case "$idx" in *BS_GA*) case "$inp" in *_G_to_A*) hit=1;; esac;; esac
if [ "$hit" = 1 ]; then
  awk 'NR%2==1 { id=$1; sub(/^>/,"",id); print id "\t0\tchr1_GA_converted\t3\t42\t6M\t*\t0\t0\tACATAC\tIIIIII\tAS:i:0\tMD:Z:6" }' "$inp"
else
  awk 'NR%2==1 { id=$1; sub(/^>/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp"
fi
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// SE FastA fake: every read unmapped (flag 4), 2-line aware.
#[cfg(unix)]
fn make_fake_bowtie2_fasta_unmapped(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""
for a in "$@"; do [ "$prev" = "-U" ] && inp="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%2==1 { id=$1; sub(/^>/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp"
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// PE FastA fake: maps a pair on the CT index reading the `-1` C→T_R1 `.fa`,
/// 2-line aware, strips the `/1/1` tag. chr1:1 6M, flags 99/147.
#[cfg(unix)]
fn make_fake_bowtie2_pe_fasta(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-1" ] && m1="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
case "$idx" in
  *BS_CT*) awk 'NR%2==1 { id=$1; sub(/^>/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t99\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tIIIIII\tAS:i:0\tMD:Z:6";
      print id "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tIIIIII\tAS:i:0\tMD:Z:6" }' "$m1" ;;
  *)       awk 'NR%2==1 { id=$1; sub(/^>/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI";
      print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$m1" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

#[cfg(unix)]
#[test]
fn fasta_se_directional_mapped_phred40_qual() {
    // SE FastA directional (OT, eff 0). FastA proof: SEQ = original read, QUAL =
    // Phred 40 (`'I'×len`), and the C→T XM call is byte-correct.
    let genome = TempDir::new().unwrap();
    make_genome(genome.path()); // chr1 = ACGTACGT (8 bp)
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_fasta_mapped(bins.path());
    // Reads must live OUTSIDE the genome dir — a `.fa` there is globbed as a genome
    // reference (unlike `.fq`).
    let reads_dir = TempDir::new().unwrap();
    let read = reads_dir.path().join("reads.fa");
    fs::write(&read, b">r1\nACGTAC\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("-f")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   1"));

    // FastA output name keeps `.fa` (strip_fastq_suffix is FastQ-only — Perl 1622).
    let bam = outdir.path().join("reads.fa_bismark_bt2.bam");
    assert!(bam.is_file(), "expected {}", bam.display());
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 0);
    assert_eq!(usize::from(r.alignment_start().unwrap()), 1);
    assert_eq!(r.sequence().as_ref(), b"ACGTAC");
    assert_eq!(r.quality_scores().as_ref(), &[40u8; 6]); // 🔴 FastA QUAL = Phred 40
    assert_eq!(rec_tag(r, *b"XR").as_deref(), Some(&b"CT"[..]));
    assert_eq!(rec_tag(r, *b"XG").as_deref(), Some(&b"CT"[..]));
    assert_eq!(rec_tag(r, *b"XM").as_deref(), Some(&b".Z...Z"[..]));
    assert!(!temp.path().join("reads.fa_C_to_T.fa").is_file()); // `.fa` temp cleaned
}

#[cfg(unix)]
#[test]
fn fasta_se_nondir_ga_index_writes_ctob_phred40() {
    // FastA NON-DIRECTIONAL: G→A reads, GA-index hit → slot 3 → eff 3 → CTOB
    // (FLAG 16, XR GA, XG GA), QUAL Phred 40. Proves the FastA-aware strand fake +
    // the GA branch on a complementary strand for FastA. (NB: pbat ⊕ -f DIES at
    // config — Perl 8155 — so non-directional is the FastA complementary-strand path.)
    let genome = TempDir::new().unwrap();
    make_genome_chr1(genome.path(), b"TTGCGTACTT");
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_fasta_ga_index(bins.path());
    let reads_dir = TempDir::new().unwrap();
    let read = reads_dir.path().join("reads.fa");
    fs::write(&read, b">r1\nGCGTAC\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("-f")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   1"));

    let bam = outdir.path().join("reads.fa_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 16); // CTOB
    assert_eq!(r.sequence().as_ref(), b"GCGTAC"); // strand '+' → original read
    assert_eq!(r.quality_scores().as_ref(), &[40u8; 6]); // FastA QUAL Phred 40
    assert_eq!(rec_tag(r, *b"XR").as_deref(), Some(&b"GA"[..]));
    assert_eq!(rec_tag(r, *b"XG").as_deref(), Some(&b"GA"[..]));
    assert_eq!(rec_tag(r, *b"XM").as_deref(), Some(&b"H.Z..."[..]));
    // non-dir SE FastA temps = C→T + G→A `.fa`, both cleaned up.
    assert!(!temp.path().join("reads.fa_C_to_T.fa").is_file());
    assert!(!temp.path().join("reads.fa_G_to_A.fa").is_file());
}

#[cfg(unix)]
#[test]
fn fasta_pe_directional_mapped_phred40() {
    // PE FastA directional → two records, FLAG (99,147), both QUAL Phred 40.
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_fasta(bins.path());
    let reads_dir = TempDir::new().unwrap();
    let r1 = reads_dir.path().join("reads_1.fa");
    let r2 = reads_dir.path().join("reads_2.fa");
    fs::write(&r1, b">r1\nACGTAC\n").unwrap();
    fs::write(&r2, b">r1\nACGTAC\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("-f")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   1"));

    let bam = outdir.path().join("reads_1.fa_bismark_bt2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 2);
    let (m1, m2) = (recs[0].inner(), recs[1].inner());
    assert_eq!(u16::from(m1.flags()), 99);
    assert_eq!(u16::from(m2.flags()), 147);
    assert_eq!(m1.quality_scores().as_ref(), &[40u8; 6]); // both mates Phred 40
    assert_eq!(m2.quality_scores().as_ref(), &[40u8; 6]);
    assert!(!temp.path().join("reads_1.fa_C_to_T.fa").is_file());
    assert!(!temp.path().join("reads_2.fa_G_to_A.fa").is_file());
}

#[cfg(unix)]
#[test]
fn fasta_se_unmapped_writes_2line_fa_aux() {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_fasta_unmapped(bins.path());
    let reads_dir = TempDir::new().unwrap();
    let read = reads_dir.path().join("reads.fa");
    fs::write(&read, b">r1\nACGTAC\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("-f")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("--unmapped")
        .arg(&read)
        .assert()
        .success();

    // Unmapped FastA read → 2-line `>id\nseq` in the `.fa.gz` aux, NOT 4-line FastQ.
    let un = outdir.path().join("reads.fa_unmapped_reads.fa.gz");
    assert!(un.is_file(), "expected {}", un.display());
    let mut s = String::new();
    GzDecoder::new(fs::File::open(&un).unwrap())
        .read_to_string(&mut s)
        .unwrap();
    assert_eq!(s, ">r1\nACGTAC\n");
}

// ===========================================================================
// Phase 9b — worker-count invariance: `--parallel N` == `--parallel 1`, byte-for-byte.
//
// 🔴 The gate's loudness rests on a CONTENT-ADDRESSED fake (decision keyed on the
// read ID, NOT on a line ordinal / `NR%4`): each chunk is a DIFFERENT converted
// file with reads at DIFFERENT ordinals, so an ordinal-keyed fake would align
// differently per chunk and could false-pass (the Phase-8/9a trap, rev1 A-Imp1/B-O2).
// Test inputs use a read count coprime-ish to {2,4,8} so a chunk boundary is straddled
// at every N, with each decision class (UniqueBest/Ambiguous/NoAlignment) on BOTH
// sides; outputs are asserted byte-identical (BAM decompressed records, report modulo
// the wall-clock line, aux RAW gz bytes AND decompressed) across N ∈ {1,2,4,8}.
// ===========================================================================

/// A CONTENT-ADDRESSED fake `bowtie2` (SE): per-read decision keyed on the read ID's
/// first char — `m`=mapped (unique on CT/OT), `a`=within-thread ambiguous (AS==XS on
/// CT), `u`=unmapped. The SAME read therefore aligns identically regardless of which
/// chunk/ordinal/converted-file it lands in (the property that makes the
/// worker-invariance test unable to false-pass). Works for directional/non-dir/pbat
/// alike (it maps `m`/`a` on the CT index whatever the `-U` file).
#[cfg(unix)]
fn make_fake_bowtie2_content_addressed(dir: &Path) {
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
  *BS_CT*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); c=substr(id,1,1);
      if (c=="m") print id "\t0\tchr1_CT_converted\t1\t42\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else if (c=="a") print id "\t0\tchr1_CT_converted\t1\t1\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tXS:i:0\tMD:Z:6";
      else print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp" ;;
  *) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// CONTENT-ADDRESSED PE fake (keyed on the R1 ID): `m`=mapped pair (99/147 on CT/OT),
/// `u`=unmapped pair. (Ambiguous + --ambig_bam are exercised by the SE cells.)
#[cfg(unix)]
fn make_fake_bowtie2_pe_content_addressed(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-1" ] && m1="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
case "$idx" in
  *BS_CT*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id); c=substr(id,1,1);
      if (c=="m") { print id "/1\t99\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
                    print id "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6"; }
      else { print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI"; } }' "$m1" ;;
  *) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$m1" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// Canonical, ORDER-PRESERVING view of a BAM's decompressed records (one `Debug`
/// string per record, in file order). Equality ⇔ byte-identical decompressed content
/// (the gate's semantics — not raw BGZF bytes).
///
/// Reads RAW `RecordBuf`s via `noodles_bam` (NOT `bismark_io::BamReader`, which would
/// validate `XR`/`XG`/`XM`) so it works on both the main BAM AND the tagless raw
/// `--ambig_bam` — the same raw-read the production merge uses.
#[cfg(unix)]
fn canon_bam(path: &Path) -> Vec<String> {
    let file = fs::File::open(path).unwrap();
    let mut reader = noodles_bam::io::Reader::new(std::io::BufReader::new(file));
    let header = reader.read_header().unwrap();
    reader
        .record_bufs(&header)
        .map(|r| format!("{:?}", r.unwrap()))
        .collect()
}

/// Decompress a gzip file to its raw bytes (the aux worker-invariance is on
/// DECOMPRESSED content — gz framing, like BGZF for the BAM, is an impl detail that
/// differs between the N==1 inline-incremental encoder and the N>1 bulk-merge encoder
/// at scale, but decompresses identically).
#[cfg(unix)]
fn read_gz(path: &Path) -> Vec<u8> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut v = Vec::new();
    GzDecoder::new(fs::File::open(path).unwrap())
        .read_to_end(&mut v)
        .unwrap();
    v
}

/// Read a report, dropping the env-specific trailing wall-clock line.
#[cfg(unix)]
fn report_minus_wallclock(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with("Bismark completed in "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build a `reads.fq` of `n` records cycling the decision classes `m`/`u`/`a` (so each
/// class straddles chunk boundaries), distinct IDs, identical 6 bp seq (chr1:1 window).
#[cfg(unix)]
fn write_mua_reads(path: &Path, n: usize) {
    let classes = ['m', 'u', 'a'];
    let mut data = String::new();
    for i in 1..=n {
        let c = classes[(i - 1) % 3];
        data.push_str(&format!("@{c}{i:04}\nACGTAC\n+\nFFFFFF\n"));
    }
    fs::write(path, data).unwrap();
}

/// Run an SE alignment at `--parallel n` (with `--unmapped --ambiguous --ambig_bam`)
/// and return `(bam-records, report-minus-wallclock, unmapped DECOMPRESSED, ambiguous
/// DECOMPRESSED, ambig-bam-records)`. The 5th element pins the `--ambig_bam` merge path
/// across N (the exact path the gate-found tagless-record bug lived in). Reads every
/// output into owned values before the temp dirs drop.
#[cfg(unix)]
fn run_se_parallel(
    genome: &Path,
    bins: &Path,
    read: &Path,
    extra: &[&str],
    n: u32,
) -> (Vec<String>, String, Vec<u8>, Vec<u8>, Vec<String>) {
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();
    let mut cmd = bin();
    cmd.arg("--genome")
        .arg(genome)
        .arg("--path_to_bowtie2")
        .arg(bins)
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("--parallel")
        .arg(n.to_string())
        .arg("--unmapped")
        .arg("--ambiguous")
        .arg("--ambig_bam");
    for a in extra {
        cmd.arg(a);
    }
    cmd.arg(read).assert().success();

    let fname = read.file_name().unwrap().to_string_lossy().into_owned(); // "reads.fq"
    let stem = fname.strip_suffix(".fq").unwrap_or(&fname).to_string(); // "reads"
    let bam = canon_bam(&outdir.path().join(format!("{stem}_bismark_bt2.bam")));
    let report = report_minus_wallclock(
        &outdir
            .path()
            .join(format!("{stem}_bismark_bt2_SE_report.txt")),
    );
    let un = read_gz(&outdir.path().join(format!("{fname}_unmapped_reads.fq.gz")));
    let am = read_gz(&outdir.path().join(format!("{fname}_ambiguous_reads.fq.gz")));
    let ambig = canon_bam(&outdir.path().join(format!("{stem}_bismark_bt2.ambig.bam")));
    (bam, report, un, am, ambig)
}

/// Assert SE worker-invariance: `--parallel {2,4,8}` byte-identical to `--parallel 1`
/// (BAM decompressed records, report modulo wall-clock, aux raw gz bytes).
#[cfg(unix)]
fn assert_se_worker_invariant(genome: &Path, bins: &Path, read: &Path, extra: &[&str]) {
    let base = run_se_parallel(genome, bins, read, extra, 1);
    for n in [2u32, 4, 8] {
        let got = run_se_parallel(genome, bins, read, extra, n);
        assert_eq!(
            got.0, base.0,
            "BAM records differ at --parallel {n} (extra={extra:?})"
        );
        assert_eq!(
            got.1, base.1,
            "report differs at --parallel {n} (extra={extra:?})"
        );
        assert_eq!(
            got.2, base.2,
            "unmapped decompressed content differs at --parallel {n} (extra={extra:?})"
        );
        assert_eq!(
            got.3, base.3,
            "ambiguous decompressed content differs at --parallel {n} (extra={extra:?})"
        );
        assert_eq!(
            got.4, base.4,
            "--ambig_bam records differ at --parallel {n} (extra={extra:?})"
        );
    }
}

#[cfg(unix)]
#[test]
fn worker_invariance_se_directional() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path()); // chr1 = ACGTACGT (8 bp)
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_content_addressed(bins.path());
    let reads_dir = TempDir::new().unwrap();
    let read = reads_dir.path().join("reads.fq");
    write_mua_reads(&read, 13); // 13 coprime to {2,4,8}
    assert_se_worker_invariant(genome.path(), bins.path(), &read, &[]);
}

#[cfg(unix)]
#[test]
fn worker_invariance_se_non_directional() {
    // Non-dir SE spawns 4 instances per chunk; `m`/`a` still map on CT (OT) — the
    // invariance must hold across the 4-instance fan-out under chunking.
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_content_addressed(bins.path());
    let reads_dir = TempDir::new().unwrap();
    let read = reads_dir.path().join("reads.fq");
    write_mua_reads(&read, 13);
    assert_se_worker_invariant(genome.path(), bins.path(), &read, &["--non_directional"]);
}

#[cfg(unix)]
#[test]
fn worker_invariance_se_pbat() {
    // pbat-FastQ (pbat ⊕ -f dies, so FastQ only). Both instances read the G→A file;
    // `m`/`a` map on CT → eff index 2 (CTOT). Invariance across chunks.
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_content_addressed(bins.path());
    let reads_dir = TempDir::new().unwrap();
    let read = reads_dir.path().join("reads.fq");
    write_mua_reads(&read, 13);
    assert_se_worker_invariant(genome.path(), bins.path(), &read, &["--pbat"]);
}

#[cfg(unix)]
#[test]
fn worker_invariance_se_empty_chunk_at_high_n() {
    // 3 reads over --parallel 4 → chunk 3 is EMPTY: its (header-only) per-chunk BAM
    // and empty plain aux must merge to nothing, byte-identical to --parallel 1.
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_content_addressed(bins.path());
    let reads_dir = TempDir::new().unwrap();
    let read = reads_dir.path().join("reads.fq");
    write_mua_reads(&read, 3); // 3 < 4 → trailing empty chunk
    let base = run_se_parallel(genome.path(), bins.path(), &read, &[], 1);
    let got = run_se_parallel(genome.path(), bins.path(), &read, &[], 4);
    assert_eq!(
        got.0, base.0,
        "BAM records differ with an empty trailing chunk"
    );
    assert_eq!(got.1, base.1, "report differs with an empty trailing chunk");
    assert_eq!(
        got.2, base.2,
        "unmapped decompressed content differs with an empty trailing chunk"
    );
    assert_eq!(
        got.3, base.3,
        "ambiguous decompressed content differs with an empty trailing chunk"
    );
    assert_eq!(
        got.4, base.4,
        "--ambig_bam records differ with an empty trailing chunk"
    );
}

/// Run a PE alignment at `--parallel n` (with `--unmapped --ambig_bam`) and return
/// `(pe-bam-records, report-minus-wallclock, _1.unmapped DECOMPRESSED, _2.unmapped
/// DECOMPRESSED, pe.ambig-bam-records)`. The 5th element pins the `--ambig_bam` merge across N.
#[cfg(unix)]
fn run_pe_parallel(
    genome: &Path,
    bins: &Path,
    r1: &Path,
    r2: &Path,
    extra: &[&str],
    n: u32,
) -> (Vec<String>, String, Vec<u8>, Vec<u8>, Vec<String>) {
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();
    let mut cmd = bin();
    cmd.arg("--genome")
        .arg(genome)
        .arg("--path_to_bowtie2")
        .arg(bins)
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("--parallel")
        .arg(n.to_string())
        .arg("--unmapped")
        .arg("--ambig_bam");
    for a in extra {
        cmd.arg(a);
    }
    cmd.arg("-1").arg(r1).arg("-2").arg(r2).assert().success();
    let bam = canon_bam(&outdir.path().join("reads_1_bismark_bt2_pe.bam"));
    let report = report_minus_wallclock(&outdir.path().join("reads_1_bismark_bt2_PE_report.txt"));
    let un1 = read_gz(&outdir.path().join("reads_1.fq_unmapped_reads_1.fq.gz"));
    let un2 = read_gz(&outdir.path().join("reads_2.fq_unmapped_reads_2.fq.gz"));
    let ambig = canon_bam(&outdir.path().join("reads_1_bismark_bt2_pe.ambig.bam"));
    (bam, report, un1, un2, ambig)
}

/// Assert PE worker-invariance: `--parallel {2,4,8}` byte-identical to `--parallel 1`.
#[cfg(unix)]
fn assert_pe_worker_invariant(genome: &Path, bins: &Path, r1: &Path, r2: &Path, extra: &[&str]) {
    let base = run_pe_parallel(genome, bins, r1, r2, extra, 1);
    for n in [2u32, 4, 8] {
        let got = run_pe_parallel(genome, bins, r1, r2, extra, n);
        assert_eq!(
            got.0, base.0,
            "PE BAM differs at --parallel {n} (extra={extra:?})"
        );
        assert_eq!(
            got.1, base.1,
            "PE report differs at --parallel {n} (extra={extra:?})"
        );
        assert_eq!(
            got.2, base.2,
            "PE _1 unmapped decompressed differs at --parallel {n} (extra={extra:?})"
        );
        assert_eq!(
            got.3, base.3,
            "PE _2 unmapped decompressed differs at --parallel {n} (extra={extra:?})"
        );
        assert_eq!(
            got.4, base.4,
            "PE --ambig_bam differs at --parallel {n} (extra={extra:?})"
        );
    }
}

/// Write a PE m/u read pair set (13 pairs, mate seqs distinct so the BAM records differ).
#[cfg(unix)]
fn write_pe_mu_reads(r1: &Path, r2: &Path) {
    let classes = ['m', 'u'];
    let (mut d1, mut d2) = (String::new(), String::new());
    for i in 1..=13 {
        let c = classes[(i - 1) % 2];
        d1.push_str(&format!("@{c}{i:04}\nACGTAC\n+\nFFFFFF\n"));
        d2.push_str(&format!("@{c}{i:04}\nACGTAC\n+\nFFFFFF\n"));
    }
    fs::write(r1, d1).unwrap();
    fs::write(r2, d2).unwrap();
}

#[cfg(unix)]
#[test]
fn worker_invariance_pe_directional() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_content_addressed(bins.path());
    let reads_dir = TempDir::new().unwrap();
    let (r1, r2) = (
        reads_dir.path().join("reads_1.fq"),
        reads_dir.path().join("reads_2.fq"),
    );
    write_pe_mu_reads(&r1, &r2);
    assert_pe_worker_invariant(genome.path(), bins.path(), &r1, &r2, &[]);
}

#[cfg(unix)]
#[test]
fn worker_invariance_pe_non_directional() {
    // Non-dir PE populates ALL 4 slots per chunk; `m` maps on CT/OT (slot 0). The
    // invariance must hold across the 4-instance PE fan-out under chunking (B-M2).
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_content_addressed(bins.path());
    let reads_dir = TempDir::new().unwrap();
    let (r1, r2) = (
        reads_dir.path().join("reads_1.fq"),
        reads_dir.path().join("reads_2.fq"),
    );
    write_pe_mu_reads(&r1, &r2);
    assert_pe_worker_invariant(genome.path(), bins.path(), &r1, &r2, &["--non_directional"]);
}

/// A fake `bowtie2` that succeeds on `--version` (so Phase-1 detection passes) but
/// **exits 1 on any alignment** — to drive a per-chunk worker error.
#[cfg(unix)]
fn make_fake_bowtie2_align_fails(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
exit 1
"#;
    write_exec(&dir.join("bowtie2"), script);
}

#[cfg(unix)]
#[test]
fn worker_error_propagates_no_hang() {
    // §9 #10: a chunk worker whose Bowtie 2 exits non-zero must surface a clean error
    // (not hang/deadlock, not a panic abort). `AlignerStream::finish` errors on the
    // non-zero exit → the chunk job returns Err → `collect_in_order` returns it →
    // `bismark_rs` exits non-zero. (If the scope deadlocked, assert_cmd would hang and
    // the test would time out instead of returning `.failure()`.)
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_align_fails(bins.path());
    let reads_dir = TempDir::new().unwrap();
    let read = reads_dir.path().join("reads.fq");
    write_mua_reads(&read, 13);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();
    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("--parallel")
        .arg("4")
        .arg(&read)
        .assert()
        .failure()
        .code(1);
}

// ---- HISAT2 backend (Phase 2a) -----------------------------------------------

/// A genome dir with a complete small HISAT2 bisulfite index (8 `.ht2` files per
/// converted genome, no `rev.*`) + one FASTA — the HISAT2 analogue of
/// [`make_genome`].
fn make_genome_ht2(dir: &Path) {
    let ct = dir.join("Bisulfite_Genome").join("CT_conversion");
    let ga = dir.join("Bisulfite_Genome").join("GA_conversion");
    fs::create_dir_all(&ct).unwrap();
    fs::create_dir_all(&ga).unwrap();
    for n in 1..=8 {
        fs::write(ct.join(format!("BS_CT.{n}.ht2")), b"x").unwrap();
        fs::write(ga.join(format!("BS_GA.{n}.ht2")), b"x").unwrap();
    }
    fs::write(dir.join("genome.fa"), b">chr1\nACGTACGT\n").unwrap();
}

/// A fake `hisat2` (banner `hisat2-align-s version 2.2.2`, reached via
/// `--path_to_hisat2`): on the CT (`BS_CT`) index it maps a 6 bp OT alignment at
/// chr1:1 (`AS:i:0`/`MD:Z:6`), UNMAPPED on GA → the merge yields a unique best on
/// the OT strand (the HISAT2 analogue of `make_fake_bowtie2_mapped`).
#[cfg(unix)]
fn make_fake_hisat2_mapped(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "hisat2-align-s version 2.2.2"; exit 0;; esac
inp=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-U" ] && inp="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
case "$idx" in
  *BS_CT*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t0\tchr1_CT_converted\t1\t42\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6" }' "$inp" ;;
  *)       awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("hisat2"), script);
}

/// Fake HISAT2 for the `--local` soft-clip path: emits a SOFT-CLIPPED CIGAR (`2S4M`,
/// the 8 bp test genome) on the CT instance — HISAT2-local drops `--no-softclip`, so the
/// aligner may soft-clip. Mirrors `make_fake_hisat2_mapped` but with `2S4M` + `MD:Z:4`.
fn make_fake_hisat2_local_softclip(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "hisat2-align-s version 2.2.2"; exit 0;; esac
inp=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-U" ] && inp="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
case "$idx" in
  *BS_CT*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t0\tchr1_CT_converted\t1\t42\t2S4M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:4" }' "$inp" ;;
  *)       awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("hisat2"), script);
}

/// `--hisat2 --local` end-to-end: the report echoes the HISAT2-local option delta
/// (`--score-min L,0,-0.2 … --omit-sec-seq`, **no `--local`, no `--no-softclip`**), and a
/// soft-clipped (`2S4M`) alignment round-trips through methylation calling into the BAM
/// (the `S` op is handled like `I`, `methylation.rs:174`) without crashing.
#[cfg(unix)]
#[test]
fn hisat2_local_softclip_roundtrip_and_options() {
    let genome = TempDir::new().unwrap();
    make_genome_ht2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_hisat2_local_softclip(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nACGTAC\n+\nIIIIII\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--hisat2")
        .arg("--local")
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    // Report echoes the HISAT2-local delta: L-form score-min + `--omit-sec-seq`, and
    // NEITHER `--local` NOR `--no-softclip`.
    let report =
        fs::read_to_string(outdir.path().join("reads_bismark_hisat2_SE_report.txt")).unwrap();
    assert!(report.contains("-q --score-min L,0,-0.2 --ignore-quals --omit-sec-seq"));
    assert!(!report.contains("--local"));
    assert!(!report.contains("--no-softclip"));

    // The soft-clipped CIGAR round-trips into the BAM (SEQ retains the full read for `S`).
    let bam = outdir.path().join("reads_bismark_hisat2.bam");
    assert!(bam.is_file());
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(r.sequence().as_ref(), b"ACGTAC"); // soft-clip retains SEQ
    use noodles_sam::alignment::record::cigar::op::Kind;
    let has_softclip = r
        .cigar()
        .as_ref()
        .iter()
        .any(|op| op.kind() == Kind::SoftClip);
    assert!(
        has_softclip,
        "the --local soft-clipped CIGAR (2S4M) must round-trip into the BAM"
    );
}

/// PE fake HISAT2 for the `--local` soft-clip path — a soft-clipped (`2S4M`) proper pair
/// (FLAG 99/147) on the CT instance. Mirrors `make_fake_hisat2_pe` with `2S4M` + `MD:Z:4`.
fn make_fake_hisat2_pe_local_softclip(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "hisat2-align-s version 2.2.2"; exit 0;; esac
m1=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-1" ] && m1="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
case "$idx" in
  *BS_CT*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t99\tchr1_CT_converted\t1\t42\t2S4M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tZS:i:-2\tMD:Z:4";
      print id "/2\t147\tchr1_CT_converted\t1\t42\t2S4M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tZS:i:-2\tMD:Z:4" }' "$m1" ;;
  *)       awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI";
      print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$m1" ;;
esac
"#;
    write_exec(&dir.join("hisat2"), script);
}

/// `--hisat2 --local` PAIRED-END: the PE report echoes the HISAT2-local option delta
/// (no `--local`, no `--no-softclip`; `--omit-sec-seq`), and a soft-clipped (`2S4M`) pair
/// round-trips through per-mate methylation calling into the `_pe.bam` (both mates).
#[cfg(unix)]
#[test]
fn hisat2_local_pe_softclip_roundtrip() {
    let genome = TempDir::new().unwrap();
    make_genome_ht2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_hisat2_pe_local_softclip(bins.path());
    let r1 = genome.path().join("reads_1.fq");
    let r2 = genome.path().join("reads_2.fq");
    fs::write(&r1, b"@r1\nACGTAC\n+\nFFFFFF\n").unwrap();
    fs::write(&r2, b"@r1\nACGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--hisat2")
        .arg("--local")
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success();

    let report =
        fs::read_to_string(outdir.path().join("reads_1_bismark_hisat2_PE_report.txt")).unwrap();
    assert!(report.contains(
        "-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --maxins 500 --omit-sec-seq"
    ));
    assert!(!report.contains("--local"));
    assert!(!report.contains("--no-softclip"));

    let bam = outdir.path().join("reads_1_bismark_hisat2_pe.bam");
    assert!(bam.is_file());
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 2, "two soft-clipped mate records");
    use noodles_sam::alignment::record::cigar::op::Kind;
    for r in &recs {
        let has_softclip = r
            .inner()
            .cigar()
            .as_ref()
            .iter()
            .any(|op| op.kind() == Kind::SoftClip);
        assert!(
            has_softclip,
            "each PE --local mate's 2S4M CIGAR must round-trip"
        );
    }
}

/// V7: `--hisat2` SE end-to-end — the output BAM + report carry the `hisat2`
/// naming token (not `bt2`) and the report says "Bismark was run with HISAT2"
/// and echoes the `--no-softclip --omit-sec-seq` option delta.
#[cfg(unix)]
#[test]
fn hisat2_se_mapped_names_and_report() {
    let genome = TempDir::new().unwrap();
    make_genome_ht2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_hisat2_mapped(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nACGTAC\n+\nIIIIII\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--hisat2")
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    // Naming token is `hisat2`, NOT `bt2`.
    assert!(outdir.path().join("reads_bismark_hisat2.bam").is_file());
    assert!(!outdir.path().join("reads_bismark_bt2.bam").exists());

    let report =
        fs::read_to_string(outdir.path().join("reads_bismark_hisat2_SE_report.txt")).unwrap();
    assert!(report.contains("Bismark was run with HISAT2 against"));
    // The HISAT2 option delta is echoed in the report's aligner_options line.
    assert!(report.contains("-q --score-min L,0,-0.2 --ignore-quals --no-softclip --omit-sec-seq"));
    assert!(report.contains("Sequences analysed in total:\t1\n"));
    assert!(report.contains("Mapping efficiency:\t100.0%\n"));
}

/// V8: `--hisat2 --no-spliced-alignment` wires the splice flag into the option
/// string (before the softclip delta), visible in the report's aligner_options line.
#[cfg(unix)]
#[test]
fn hisat2_no_spliced_alignment_echoed_in_report() {
    let genome = TempDir::new().unwrap();
    make_genome_ht2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_hisat2_mapped(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nACGTAC\n+\nIIIIII\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--hisat2")
        .arg("--no-spliced-alignment")
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    let report =
        fs::read_to_string(outdir.path().join("reads_bismark_hisat2_SE_report.txt")).unwrap();
    assert!(report.contains(
        "-q --score-min L,0,-0.2 --ignore-quals --no-spliced-alignment --no-softclip --omit-sec-seq"
    ));
}

/// GAP-2 RESOLVED — `--hisat2 --multicore N` is now SUPPORTED (Approach B-faithful, plan
/// `06132026_aligner-hisat2-multicore`): it routes to a SINGLE HISAT2 instance with
/// `-p N --reorder` (the fork model is not faithful for HISAT2 — splice discovery is not
/// chunk-invariant). The run succeeds, the report echoes `-p N --reorder`, and a
/// never-silent notice is emitted. The `--parallel` alias routes the same way.
#[cfg(unix)]
#[test]
fn multicore_with_hisat2_routes_to_p_threading() {
    let genome = TempDir::new().unwrap();
    make_genome_ht2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_hisat2_mapped(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nACGTAC\n+\nIIIIII\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    // --hisat2 --multicore 2 SUCCEEDS and prints the never-silent remap notice.
    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--hisat2")
        .arg("--multicore")
        .arg("2")
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("--hisat2").and(predicate::str::contains("-p 2 threading")),
        );

    // Single-instance output (the hisat2 token; no multicore-merged rename).
    assert!(outdir.path().join("reads_bismark_hisat2.bam").is_file());
    let report =
        fs::read_to_string(outdir.path().join("reads_bismark_hisat2_SE_report.txt")).unwrap();
    // The remapped `-p 2 --reorder` is echoed in the report's aligner_options line.
    assert!(report.contains("-p 2 --reorder"), "report: {report}");

    // The --parallel alias routes the same way (also succeeds).
    let temp2 = TempDir::new().unwrap();
    let outdir2 = TempDir::new().unwrap();
    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--hisat2")
        .arg("--parallel")
        .arg("4")
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp2.path())
        .arg("--output_dir")
        .arg(outdir2.path())
        .arg(&read)
        .assert()
        .success();
    assert!(outdir2.path().join("reads_bismark_hisat2.bam").is_file());
}

/// Single-core `--ambig_bam` + `--hisat2` IS supported and names the ambig BAM
/// with the `hisat2` token (Perl 1583-1586) — the counterpart to the reject above.
#[cfg(unix)]
#[test]
fn ambig_bam_single_core_hisat2_names_hisat2_token() {
    let genome = TempDir::new().unwrap();
    make_genome_ht2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_hisat2_mapped(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nACGTAC\n+\nIIIIII\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--hisat2")
        .arg("--ambig_bam")
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    // The ambig BAM exists with the hisat2 token (the read here maps uniquely, so
    // it is created empty-of-records but present — the naming is what we assert).
    assert!(
        outdir
            .path()
            .join("reads_bismark_hisat2.ambig.bam")
            .is_file()
    );
}

/// A PE fake `hisat2` (banner `hisat2-align-s version 2.2.2`, via `--path_to_hisat2`):
/// on the CT (`BS_CT`) index it maps an OT pair (99/147 at chr1:1) where **mate-1
/// carries a `ZS:i:` second-best** (HISAT2's tag) — exercising the read-1-`ZS`
/// mask end-to-end; UNMAPPED (77/141) on GA → a unique best on OT.
#[cfg(unix)]
fn make_fake_hisat2_pe(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "hisat2-align-s version 2.2.2"; exit 0;; esac
m1=""; prev=""; idx=""
for a in "$@"; do
  [ "$prev" = "-1" ] && m1="$a"
  [ "$prev" = "-x" ] && idx="$a"
  prev="$a"
done
printf '@HD\tVN:1.0\n'
case "$idx" in
  *BS_CT*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t99\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tZS:i:-2\tMD:Z:6";
      print id "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tZS:i:-2\tMD:Z:6" }' "$m1" ;;
  *)       awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI";
      print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$m1" ;;
esac
"#;
    write_exec(&dir.join("hisat2"), script);
}

/// V6 (Phase 2b): `--hisat2` PE end-to-end — the PE BAM + report carry the
/// `hisat2` token (`_bismark_hisat2_pe*`), the report says "run with HISAT2" and
/// echoes the PE HISAT2 option string (no `--dovetail`), and both mate records
/// are written. The mate-1 `ZS` is consumed by the merge (masked), not emitted.
#[cfg(unix)]
#[test]
fn hisat2_pe_mapped_names_and_report() {
    let genome = TempDir::new().unwrap();
    make_genome_ht2(genome.path()); // chr1 = ACGTACGT (8 bp)
    let bins = TempDir::new().unwrap();
    make_fake_hisat2_pe(bins.path());
    let r1 = genome.path().join("reads_1.fq");
    let r2 = genome.path().join("reads_2.fq");
    fs::write(&r1, b"@r1\nACGTAC\n+\nFFFFFF\n").unwrap();
    fs::write(&r2, b"@r1\nACGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--hisat2")
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success();

    // PE naming token is `hisat2`, NOT `bt2`.
    let bam = outdir.path().join("reads_1_bismark_hisat2_pe.bam");
    assert!(bam.is_file(), "expected {}", bam.display());
    assert!(!outdir.path().join("reads_1_bismark_bt2_pe.bam").exists());
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 2, "two records per pair");
    // MAPQ guard (review L-1): pins the end-to-end masked-path MAPQ. With the
    // read-1 `ZS` mask, sum_second = as1(0) + zs2(-2) = -2; reverting the mask
    // (read-1 zs1=-2 used) → sum_second = -4 → a different MAPQ. The unit tests
    // (merge::pe_hisat2_*) are the precise guard; this locks the integration path.
    let mapq = u8::from(recs[0].inner().mapping_quality().expect("mapq"));
    assert_eq!(mapq, 38, "read-1-ZS-masked MAPQ (sum_second=-2)");

    let report =
        fs::read_to_string(outdir.path().join("reads_1_bismark_hisat2_PE_report.txt")).unwrap();
    assert!(report.contains("Bismark was run with HISAT2 against"));
    assert!(
        report.contains("--no-mixed --no-discordant --maxins 500 --no-softclip --omit-sec-seq")
    );
    assert!(!report.contains("--dovetail"));
}

// ===========================================================================
// minimap2 (Phase 4) — SE only; positional `.mmi`, clean-slate options, `mm2`
// naming, max-length cutoff. PE-minimap2 is rejected (see
// `minimap2_paired_end_is_rejected` above).
// ===========================================================================

/// A genome dir with a complete minimap2 `.mmi` index (single file per converted
/// genome) + one FASTA (chr1 = `ACGTACGT`, 8 bp).
fn make_genome_mmi(dir: &Path) {
    let ct = dir.join("Bisulfite_Genome").join("CT_conversion");
    let ga = dir.join("Bisulfite_Genome").join("GA_conversion");
    fs::create_dir_all(&ct).unwrap();
    fs::create_dir_all(&ga).unwrap();
    fs::write(ct.join("BS_CT.mmi"), b"x").unwrap();
    fs::write(ga.join("BS_GA.mmi"), b"x").unwrap();
    fs::write(dir.join("genome.fa"), b">chr1\nACGTACGT\n").unwrap();
}

/// A fake `minimap2` (prints the BARE version `2.31-r1302`; reached via
/// `--path_to_minimap2`). It is invoked **positionally** — `<opts> <BS_*.mmi>
/// <input>`, with NO `-x`/`-U`/`--norc`/`--nofw` — so it locates the index by the
/// `.mmi` arg and the reads by the `.fastq` arg. On the CT (`BS_CT`) index it maps
/// a 6 bp OT alignment at chr1:1 with minimap2-style tags: a **positive** `AS:i:`
/// and a present-but-ignored `s2:i:` (the second-best chaining score Bismark
/// drops); UNMAPPED on GA → the merge yields a unique best on OT.
///
/// 🔴 Cannot false-pass on a wrong invocation: had the Rust used the Bowtie 2
/// shape (`-x BS_CT -U …`) there would be no `.mmi` arg → `$mmi` empty → every
/// read routes to the unmapped branch → the mapping-efficiency assertions fail.
#[cfg(unix)]
fn make_fake_minimap2_mapped(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "2.31-r1302"; exit 0;; esac
inp=""; mmi=""
for a in "$@"; do
  case "$a" in
    *.mmi) mmi="$a" ;;
    *.fastq|*.fq) inp="$a" ;;
  esac
done
printf '@HD\tVN:1.0\n'
case "$mmi" in
  *BS_CT*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t0\tchr1_CT_converted\t1\t60\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tNM:i:0\tms:i:12\tAS:i:12\tnn:i:0\ttp:A:P\ts1:i:10\ts2:i:0\tMD:Z:6" }' "$inp" ;;
  *)       awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("minimap2"), script);
}

/// V8: `--minimap2` SE end-to-end — the output BAM + report carry the `mm2`
/// naming token (not `bt2`/`hisat2`), the report says "Bismark was run with
/// minimap2" (lowercase, Perl 1725) and echoes the clean-slate minimap2 option
/// string (`-a --MD --secondary=no -t 2 -x map-ont -K 250K`). The fake's
/// present-but-ignored `s2:i:` exercises the merge-no-op end to end.
#[cfg(unix)]
#[test]
fn minimap2_se_mapped_names_and_report() {
    let genome = TempDir::new().unwrap();
    make_genome_mmi(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_minimap2_mapped(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nACGTAC\n+\nIIIIII\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--minimap2")
        .arg("--path_to_minimap2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    // Naming token is `mm2`, NOT `bt2`/`hisat2`.
    assert!(outdir.path().join("reads_bismark_mm2.bam").is_file());
    assert!(!outdir.path().join("reads_bismark_bt2.bam").exists());
    assert!(!outdir.path().join("reads_bismark_hisat2.bam").exists());

    let report = fs::read_to_string(outdir.path().join("reads_bismark_mm2_SE_report.txt")).unwrap();
    assert!(report.contains("Bismark was run with minimap2 against"));
    // The clean-slate minimap2 option string is echoed (NOT the Bowtie 2 `-q …`).
    assert!(report.contains("-a --MD --secondary=no -t 2 -x map-ont -K 250K"));
    assert!(!report.contains("-q --score-min"));
    assert!(report.contains("Sequences analysed in total:\t1\n"));
    assert!(report.contains("Mapping efficiency:\t100.0%\n"));

    // The single mapped record was written.
    let mut reader =
        bismark_io::BamReader::from_path(&outdir.path().join("reads_bismark_mm2.bam")).unwrap();
    assert_eq!(reader.records().count(), 1);
}

/// I-3 (review B): the `--mm2_maximum_length` drop interacts with the analysis
/// counter — a read longer than the cutoff is removed from the temp file (so
/// minimap2 never sees it), but it is STILL counted as "analysed" and lands in
/// "no alignment" (the original-read loop counts it; no aligner record matches).
/// Two reads: a 6 bp read that maps + a 101 bp read dropped by `--mm2_maximum_length
/// 100` → 2 analysed, 1 unique, 1 no-alignment (50.0% efficiency).
#[cfg(unix)]
#[test]
fn minimap2_max_length_drop_counts_as_no_alignment() {
    let genome = TempDir::new().unwrap();
    make_genome_mmi(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_minimap2_mapped(bins.path());
    let read = genome.path().join("reads.fq");
    let long_seq = "A".repeat(101);
    let long_qual = "I".repeat(101);
    fs::write(
        &read,
        format!("@r1\nACGTAC\n+\nIIIIII\n@long\n{long_seq}\n+\n{long_qual}\n"),
    )
    .unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--minimap2")
        .arg("--mm2_maximum_length")
        .arg("100")
        .arg("--path_to_minimap2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    let report = fs::read_to_string(outdir.path().join("reads_bismark_mm2_SE_report.txt")).unwrap();
    // Both reads analysed (the >cutoff read is NOT silently lost)…
    assert!(report.contains("Sequences analysed in total:\t2\n"));
    // …the dropped read is "no alignment", the 6 bp read maps → 50% efficiency.
    assert!(report.contains("Mapping efficiency:\t50.0%\n"));
    assert!(report.contains("Sequences with no alignments under any condition:\t1\n"));

    // Exactly one BAM record (the 6 bp read; the 101 bp read never aligned).
    let mut reader =
        bismark_io::BamReader::from_path(&outdir.path().join("reads_bismark_mm2.bam")).unwrap();
    assert_eq!(reader.records().count(), 1);
}

// ===========================================================================
// rammap (Phase 3) — the pure-Rust minimap2 reimplementation as a 4th backend
// (subprocess shape). minimap-like: `.mmi`, clean-slate `map-ont`, `rammap`
// naming, SE-only, concordance-gated (NOT byte-identical). PE rejected.
// ===========================================================================

/// A fake `rammap` (prints `rammap 1.1.1` — a BANNER prefix, unlike minimap2's bare
/// number; reached via `--path_to_rammap`). Invoked **positionally** like minimap2
/// (`<opts> <BS_*.mmi> <input>`, NO `-x`/`-U`/`--norc`/`--nofw`). On the CT index it
/// maps each read at chr1:1 with the minimap2/rammap tag set (positive `AS:i:`, the
/// ignored `s2:i:`, `MD:Z:`); UNMAPPED on GA. The read named `sup` ALSO emits a
/// trailing **supplementary** record (flag 2048, `SA:Z:`) — the primary comes FIRST
/// (rammap's order), so the merge must keep the primary and NOT emit an extra record.
///
/// 🔴 Cannot false-pass on a wrong invocation: had the Rust used the Bowtie 2 shape
/// (`-x BS_CT -U …`) there would be no `.mmi` arg → every read routes to unmapped →
/// the mapping-efficiency assertions fail.
#[cfg(unix)]
fn make_fake_rammap_mapped(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "rammap 1.1.1"; exit 0;; esac
inp=""; mmi=""
for a in "$@"; do
  case "$a" in
    *.mmi) mmi="$a" ;;
    *.fastq|*.fq) inp="$a" ;;
  esac
done
printf '@HD\tVN:1.0\n'
case "$mmi" in
  *BS_CT*) awk 'NR%4==1 {
             id=$1; sub(/^@/,"",id);
             print id "\t0\tchr1_CT_converted\t1\t60\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tNM:i:0\tms:i:12\tAS:i:12\tnn:i:0\ttp:A:P\ts1:i:10\ts2:i:0\tMD:Z:6";
             if (id == "sup") {
               # trailing supplementary (flag 2048, SA:Z) — must NOT displace the primary
               print id "\t2048\tchr1_CT_converted\t5\t60\t3M3S\t*\t0\t0\tACG\tFFF\tNM:i:0\tAS:i:-9\tSA:Z:chr1,1,+,6M,60,0;\tMD:Z:3";
             }
           }' "$inp" ;;
  *)       awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\t*\tI" }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("rammap"), script);
}

/// Phase 3 (T6): `--rammap` SE end-to-end — the BAM + report carry the `rammap`
/// naming token (not `bt2`/`hisat2`/`mm2`), the report says "Bismark was run with
/// rammap" and echoes the clean-slate `map-ont` option string; the never-silent
/// notice ("NOT byte-identical to minimap2") is on stderr; and the BAM record count
/// equals the reported unique-mapped count (catches a supplementary mis-pick — the
/// `sup` read's trailing flag-2048 record must NOT add a BAM record).
#[cfg(unix)]
#[test]
fn rammap_se_mapped_names_report_and_notice() {
    let genome = TempDir::new().unwrap();
    make_genome_mmi(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_rammap_mapped(bins.path());
    let read = genome.path().join("reads.fq");
    // two reads: a plain one + `sup` (which gets a trailing supplementary record).
    fs::write(&read, b"@r1\nACGTAC\n+\nIIIIII\n@sup\nACGTAC\n+\nIIIIII\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--rammap")
        .arg("--path_to_rammap")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        // never-silent opt-in notice (T5) on stderr.
        .stderr(
            predicate::str::contains(
                "--rammap uses the rammap pure-Rust minimap2 reimplementation",
            )
            .and(predicate::str::contains("NOT byte-identical to minimap2")),
        );

    // Naming token is `rammap`, NOT bt2/hisat2/mm2.
    let bam = outdir.path().join("reads_bismark_rammap.bam");
    assert!(bam.is_file());
    assert!(fs::metadata(&bam).unwrap().len() > 0);
    assert!(!outdir.path().join("reads_bismark_bt2.bam").exists());
    assert!(!outdir.path().join("reads_bismark_mm2.bam").exists());

    let report =
        fs::read_to_string(outdir.path().join("reads_bismark_rammap_SE_report.txt")).unwrap();
    assert!(report.contains("Bismark was run with rammap against"));
    // The clean-slate minimap2/rammap option string is echoed (NOT the Bowtie 2 `-q …`).
    assert!(report.contains("-a --MD --secondary=no -t 2 -x map-ont -K 250K"));
    assert!(!report.contains("-q --score-min"));
    assert!(report.contains("Sequences analysed in total:\t2\n"));
    // Both reads map uniquely (the `sup` supplementary does not perturb the count).
    assert!(report.contains(
        "Number of alignments with a unique best hit from the different alignments:\t2\n"
    ));
    assert!(report.contains("Mapping efficiency:\t100.0%\n"));

    // BAM record count == reported unique-mapped count (2): the trailing flag-2048
    // supplementary on `sup` must NOT have produced an extra record.
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    assert_eq!(reader.records().count(), 2);
}

/// Phase 3 (T3): paired-end `--rammap` is rejected loudly (SE-only, minimap-like) —
/// mirrors `minimap2_paired_end_is_rejected`, swapping `--minimap2` → `--rammap`. The
/// real temp `-1`/`-2` files take the layout past `check_exists` so the PE reject fires.
#[test]
fn rammap_paired_end_is_rejected() {
    let r1 = TempDir::new().unwrap();
    let m1 = r1.path().join("r1.fq");
    let m2 = r1.path().join("r2.fq");
    fs::write(&m1, b"@r/1\nACGT\n+\nIIII\n").unwrap();
    fs::write(&m2, b"@r/2\nACGT\n+\nIIII\n").unwrap();
    bin()
        .arg("--rammap")
        .arg("some_genome")
        .arg("-1")
        .arg(&m1)
        .arg("-2")
        .arg(&m2)
        .assert()
        .failure()
        .code(1)
        .stderr(
            predicate::str::contains("paired-end")
                .and(predicate::str::contains("rammap"))
                .and(predicate::str::contains("not supported")),
        );
}

// ===========================================================================
// `--combined_index` (v2) — opt-in combined-index alignment (PLAN 06072026 ph2).
// Hermetic: a fake `bowtie2` emits combined-style SAM (RNAME `_CT_converted`/
// `_GA_converted` suffix × FLAG orientation) so the classifier sees OT / OB /
// spurious / tie / miss deterministically — no real Bowtie 2 or index needed.
// ===========================================================================

/// A genome dir with the small CT/GA index, a 16 bp chr1, AND a complete combined
/// `Bisulfite_Genome/Combined/BS_combined.*.bt2` index.
#[cfg(unix)]
fn make_genome_combined(dir: &Path) {
    let ct = dir.join("Bisulfite_Genome").join("CT_conversion");
    let ga = dir.join("Bisulfite_Genome").join("GA_conversion");
    let comb = dir.join("Bisulfite_Genome").join("Combined");
    fs::create_dir_all(&ct).unwrap();
    fs::create_dir_all(&ga).unwrap();
    fs::create_dir_all(&comb).unwrap();
    for s in ["1", "2", "3", "4", "rev.1", "rev.2"] {
        fs::write(ct.join(format!("BS_CT.{s}.bt2")), b"x").unwrap();
        fs::write(ga.join(format!("BS_GA.{s}.bt2")), b"x").unwrap();
        fs::write(comb.join(format!("BS_combined.{s}.bt2")), b"x").unwrap();
    }
    fs::write(dir.join("genome.fa"), b">chr1\nACGTACGTACGTACGT\n").unwrap(); // 16 bp
}

/// A fake `bowtie2` for the COMBINED index (one both-strands instance). Per input
/// read it emits combined-style SAM keyed on the read id, so the combined
/// classifier sees a deterministic OT / OB / spurious / tie / miss.
#[cfg(unix)]
fn make_fake_bowtie2_combined(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""
for a in "$@"; do [ "$prev" = "-U" ] && inp="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%4==1 {
  id=$1; sub(/^@/,"",id);
  if (id=="r_ot")        print id "\t0\tchr1_CT_converted\t1\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
  else if (id=="r_ob")   print id "\t16\tchr1_GA_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
  else if (id=="r_spur") print id "\t0\tchr1_GA_converted\t1\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
  else if (id=="r_tie") { print id "\t0\tchr1_CT_converted\t1\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6"; print id "\t16\tchr1_GA_converted\t9\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6"; }
  else if (id=="r_keep") { print id "\t0\tchr1_CT_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6"; print id "\t16\tchr1_GA_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6"; }
  else print id "\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF";
}' "$inp"
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// Write a FastQ with the given read ids (each a 6 bp `ACGTAC` read).
fn write_reads_ids(dir: &Path, name: &str, ids: &[&str]) -> std::path::PathBuf {
    let r = dir.join(name);
    let mut s = String::new();
    for id in ids {
        s.push_str(&format!("@{id}\nACGTAC\n+\nFFFFFF\n"));
    }
    fs::write(&r, s).unwrap();
    r
}

/// A combined-index OT read → FLAG 0, XR:Z:CT, XG:Z:CT (the byte-frozen output
/// arm reused via synthetic index 0).
#[cfg(unix)]
#[test]
fn combined_index_ot_read_end_to_end() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_ot"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Combined-index mode (EXPERIMENTAL")
                .and(predicate::str::contains("unique best alignments:   1")),
        );

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 0); // OT → FLAG 0
    assert_eq!(usize::from(r.alignment_start().unwrap()), 1);

    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    let v = |t: [u8; 2]| r.data().get(&Tag::from(t)).cloned();
    assert_eq!(v(*b"XR"), Some(Value::String("CT".into())));
    assert_eq!(v(*b"XG"), Some(Value::String("CT".into())));
}

/// THE OB→1 REGRESSION: a combined-index OB read (rev + `_GA_converted`) must map
/// to the OB strand (FLAG 16, **XR:Z:CT**, XG:Z:GA). The DRAFT's wrong synthetic
/// index `OB→3` (CTOB) would emit XR:Z:GA — so `XR=="CT"` is the discriminator.
#[cfg(unix)]
#[test]
fn combined_index_ob_read_maps_to_ob_strand_with_xr_ct() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_ob"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   1"));

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 16); // OB → '-' strand → FLAG 16
    assert_eq!(usize::from(r.alignment_start().unwrap()), 5);

    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    let v = |t: [u8; 2]| r.data().get(&Tag::from(t)).cloned();
    assert_eq!(v(*b"XR"), Some(Value::String("CT".into()))); // OB→1 (not 3 → would be GA)
    assert_eq!(v(*b"XG"), Some(Value::String("GA".into())));
}

/// A spurious-only best (fwd + `_GA_converted`) → no alignment (counted, header-
/// only BAM), and the report carries the combined-mode spurious tally.
#[cfg(unix)]
#[test]
fn combined_index_spurious_only_is_no_alignment() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_spur"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("no alignment found:       1"));

    let report = fs::read_to_string(outdir.path().join("reads_bismark_bt2_SE_report.txt")).unwrap();
    assert!(report.contains(
        "Spurious alignments discarded (combined-index; wrong sub-genome for the orientation):\t1\n"
    ));
    // No valid alignment → header-only BAM.
    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    assert_eq!(reader.records().count(), 0);
}

/// An OT/OB cross-strand tie at the best AS → ambiguous (header-only BAM).
#[cfg(unix)]
#[test]
fn combined_index_cross_strand_tie_is_ambiguous() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_tie"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("ambiguous (unsuitable):   1"));

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    assert_eq!(reader.records().count(), 0); // ambiguous → not written (no --ambiguous)
}

/// Phase 3 — the SAME-POSITION KEEP path end to end: a read whose OT and OB hits
/// coincide at one locus (equal AS) is KEPT (not discarded as ambiguous), written
/// as the OB record (FLAG 16 / XR:Z:CT / XG:Z:GA) — the faithful `chr:pos`+`>=`
/// collapse. (The `r_tie` cell above uses pos 1 vs 9 = cross-location → Ambiguous,
/// so this is the only cell exercising the KEEP path.)
#[cfg(unix)]
#[test]
fn combined_index_same_position_collision_kept_as_ob() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_keep"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("unique best alignments:   1")
                .and(predicate::str::contains("ambiguous (unsuitable):   0")),
        );

    // ONE record, the OB winner of the same-position collision.
    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 16); // OB
    assert_eq!(usize::from(r.alignment_start().unwrap()), 5);
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    let v = |t: [u8; 2]| r.data().get(&Tag::from(t)).cloned();
    assert_eq!(v(*b"XR"), Some(Value::String("CT".into()))); // OB → index 1 (not CTOB)
    assert_eq!(v(*b"XG"), Some(Value::String("GA".into())));
}

/// Report totals + counter ownership (§9): a 4-read mix (OT, OB, spurious, miss)
/// → 2 unique / 2 no-alignment / 1 spurious / 4 analysed, and 2 BAM records.
/// Guards against `combined::select` writing a BAM but forgetting the counters.
#[cfg(unix)]
#[test]
fn combined_index_report_totals_and_counter_ownership() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined(bins.path());
    let read = write_reads_ids(
        genome.path(),
        "reads.fq",
        &["r_ot", "r_ob", "r_spur", "r_miss"],
    );
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   2"));

    let report = fs::read_to_string(outdir.path().join("reads_bismark_bt2_SE_report.txt")).unwrap();
    assert!(report.contains("Combined-index mode (experimental"));
    // Never-silent (L1): the report advertises the real options incl. `-k 2`.
    assert!(report.contains("-k 2"));
    assert!(report.contains("Sequences analysed in total:\t4\n"));
    assert!(report.contains(
        "Number of alignments with a unique best hit from the different alignments:\t2\n"
    ));
    assert!(report.contains("Sequences with no alignments under any condition:\t2\n"));
    assert!(report.contains(
        "Spurious alignments discarded (combined-index; wrong sub-genome for the orientation):\t1\n"
    ));

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    assert_eq!(reader.records().count(), 2); // r_ot + r_ob
}

/// `--ambig_bam` + `--combined_index`: never-silent note that ambig records are
/// not populated in this phase (the run still succeeds).
#[cfg(unix)]
#[test]
fn combined_index_ambig_bam_emits_note() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_ot"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--ambig_bam")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "combined-index mode does not populate --ambig_bam",
        ));
}

/// Missing combined index → a clear, actionable error (points at genome-prep).
#[test]
fn combined_index_missing_index_errors() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path()); // CT/GA + fasta, but NO Combined index
    let read = make_read(genome.path());
    bin()
        .arg("--combined_index")
        .arg("--genome")
        .arg(genome.path())
        .arg(&read)
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("no combined index was found")
                .and(predicate::str::contains("--combined_genome")),
        );
}

/// Scope guard (§3.1): every not-yet-supported combination is rejected loudly.
#[test]
fn combined_index_scope_guard_rejects_unsupported() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let read = make_read(genome.path());

    // (--pbat is now SUPPORTED — Phase 7; tested in the pbat e2e cells below.)

    // --minimap2 (combined-index unsupported). NB: --hisat2 SE is now ACCEPTED
    // (Phase 1, v2.x) — covered by config unit tests + the oxy concordance gate.
    bin()
        .arg("--combined_index")
        .arg("--minimap2")
        .arg("--genome")
        .arg(genome.path())
        .arg(&read)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not supported with --minimap2"));

    // C2 guard: the single-pass exec model stays Bowtie-2-only (HISAT2 non-dir
    // combined uses the default parallel model (a)).
    bin()
        .arg("--combined_index")
        .arg("--non_directional")
        .arg("--combined_index_single_pass")
        .arg("--hisat2")
        .arg("--genome")
        .arg(genome.path())
        .arg(&read)
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires Bowtie 2"));

    // --multicore
    bin()
        .arg("--combined_index")
        .arg("--multicore")
        .arg("4")
        .arg("--genome")
        .arg(genome.path())
        .arg(&read)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not supported with --multicore"));
}

// v2.x Phase 4: paired-end pbat `--combined_index` is now ACCEPTED for Bowtie 2 (one
// both-strands G→A pass → CTOT/CTOB — the non-dir G→A half standalone). Acceptance is
// covered end-to-end by `combined_index_pe_pbat_strands_end_to_end` (below) + the
// `config` gate unit tests (PE pbat Bowtie 2 ok; PE pbat HISAT2 still rejected). PE
// HISAT2 combined remains a later phase. (The old `combined_index_paired_end_pbat_is_
// rejected` cli test was removed — PE pbat no longer fails loud.)

// ===========================================================================
// `--combined_index` (v2.x Phase 2) paired-end directional end-to-end. Hermetic:
// a fake `bowtie2` reads the `-1` (CT R1) temp file and emits ONE both-strands
// combined-style PE pair per read id (OT 99/147 on `_CT_converted`, OB 83/163 on
// `_GA_converted`), so the combined PE driver (process_pe_chunk_combined →
// drive_merge_combined_pe → select_pe → route_pe_decision) runs end-to-end with no
// real Bowtie 2 or index.
// ===========================================================================

/// A combined-index PE fake `bowtie2` (ONE both-strands instance, `-1 CT_R1 -2
/// GA_R2`). Emits a pair per base id: `r_ot` → OT (R1 99 / R2 147 on
/// `_CT_converted` at chr1:1); `r_ob` → OB (R1 83 rev / R2 163 on `_GA_converted`
/// at chr1:5); anything else → an unmapped (77/141) pair. Strips the `/1/1` tag the
/// PE conversion adds, then re-emits `/1`,`/2` as Bowtie 2 would.
#[cfg(unix)]
fn make_fake_bowtie2_pe_combined(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""
for a in "$@"; do [ "$prev" = "-1" ] && m1="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
  if (id=="r_ot") {
    print id "/1\t99\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    print id "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
  } else if (id=="r_ob") {
    print id "/1\t83\tchr1_GA_converted\t5\t42\t6M\t=\t5\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    print id "/2\t163\tchr1_GA_converted\t5\t42\t6M\t=\t5\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
  } else {
    print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI";
    print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI";
  }
}' "$m1"
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// A combined-index directional PE OT pair → 2 BAM records, R1 FLAG 99 / R2 FLAG
/// 147 (the byte-frozen PE output arm reused via synthetic PE index 0).
#[cfg(unix)]
#[test]
fn combined_index_pe_ot_pair_end_to_end() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_combined(bins.path());
    let r1 = genome.path().join("r1.fq");
    let r2 = genome.path().join("r2.fq");
    // No mate suffix: the PE conversion inserts the `/1/1`,`/2/2` tag (Bowtie 2
    // strips the outer one). Both mates share the base id.
    fs::write(&r1, b"@r_ot\nACGTAC\n+\nFFFFFF\n").unwrap();
    fs::write(&r2, b"@r_ot\nACGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Combined-index mode, paired-end (EXPERIMENTAL")
                .and(predicate::str::contains("unique best alignments:   1")),
        );

    let bam = outdir.path().join("r1_bismark_bt2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 2); // two records per pair
    assert_eq!(u16::from(recs[0].inner().flags()), 99); // R1 → OT FLAG 99
    assert_eq!(u16::from(recs[1].inner().flags()), 147); // R2 → OT FLAG 147
}

/// THE OB→3 PE REGRESSION (end-to-end): a combined-index OB pair (R1 rev +
/// `_GA_converted`) must map to OB → PE synthetic index **3** → R1 FLAG **83** / R2
/// FLAG **163**. The SE `to_index` numbering (OB→1 = CTOB) would emit FLAG 163/83
/// instead — so R1 FLAG 83 is the discriminator that locks `to_index_pe`.
#[cfg(unix)]
#[test]
fn combined_index_pe_ob_pair_maps_to_ob_index_3() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_combined(bins.path());
    let r1 = genome.path().join("r1.fq");
    let r2 = genome.path().join("r2.fq");
    // No mate suffix: the PE conversion inserts the `/1/1`,`/2/2` tag.
    fs::write(&r1, b"@r_ob\nACGTAC\n+\nFFFFFF\n").unwrap();
    fs::write(&r2, b"@r_ob\nACGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   1"));

    let bam = outdir.path().join("r1_bismark_bt2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 2);
    // OB → PE index 3 → R1 FLAG 83, R2 FLAG 163 (NOT CTOB's 163/83).
    assert_eq!(u16::from(recs[0].inner().flags()), 83);
    assert_eq!(u16::from(recs[1].inner().flags()), 163);
}

/// A NON-DIRECTIONAL combined-index PE fake `bowtie2`: discriminates the TWO both-
/// strands passes by the **`-1` converted-input filename suffix** (`*_C_to_T*` = the
/// C→T-reads pass → OT/OB; `*_G_to_A*` = the G→A-reads pass → CTOT/CTOB) — NOT by `-x`
/// (both passes share the SAME combined index). Per base read id: `r_ot` → OT (99/147
/// on `_CT` at chr1:1) on the C→T pass; `r_ctot` → CTOT (R1 83 rev / R2 163 on `_CT` at
/// chr1:5) and `r_ctob` → CTOB (R1 99 / R2 147 on `_GA` at chr1:5) on the G→A pass;
/// everything else → an unmapped (77/141) pair on each pass.
#[cfg(unix)]
fn make_fake_bowtie2_pe_combined_nondir(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""
for a in "$@"; do [ "$prev" = "-1" ] && m1="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
case "$m1" in
  *_C_to_T*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      if (id=="r_ot") {
        print id "/1\t99\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
        print id "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      } else {
        print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI";
      } }' "$m1" ;;
  *_G_to_A*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      if (id=="r_ctot") {
        print id "/1\t83\tchr1_CT_converted\t5\t42\t6M\t=\t5\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
        print id "/2\t163\tchr1_CT_converted\t5\t42\t6M\t=\t5\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      } else if (id=="r_ctob") {
        print id "/1\t99\tchr1_GA_converted\t5\t42\t6M\t=\t5\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
        print id "/2\t147\tchr1_GA_converted\t5\t42\t6M\t=\t5\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      } else {
        print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI";
      } }' "$m1" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// `--combined_index --non_directional` PE end-to-end: the two both-strands passes
/// produce OT (C→T pass) + CTOT/CTOB (G→A pass) — the G→A-pass strands the directional
/// Phase-2 path could not exercise. Asserts the per-strand R1 FLAGs (PE output arm):
/// OT→99 (index 0), CTOT→147 (index 2), CTOB→163 (index 1).
#[cfg(unix)]
#[test]
fn combined_index_pe_nondir_strands_end_to_end() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_combined_nondir(bins.path());
    let r1 = genome.path().join("r1.fq");
    let r2 = genome.path().join("r2.fq");
    // 3 pairs (no mate suffix — conversion inserts /1/1,/2/2): r_ot (C→T pass → OT),
    // r_ctot + r_ctob (G→A pass → CTOT/CTOB).
    let reads =
        b"@r_ot\nACGTAC\n+\nFFFFFF\n@r_ctot\nACGTAC\n+\nFFFFFF\n@r_ctob\nACGTAC\n+\nFFFFFF\n";
    fs::write(&r1, reads).unwrap();
    fs::write(&r2, reads).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Combined-index mode, paired-end NON-DIRECTIONAL")
                .and(predicate::str::contains("unique best alignments:   3")),
        );

    let bam = outdir.path().join("r1_bismark_bt2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 6); // 3 pairs, in input order: r_ot, r_ctot, r_ctob
    // R1 FLAG per pair (the PE output-arm strand discriminator):
    assert_eq!(u16::from(recs[0].inner().flags()), 99); // r_ot   → OT   (index 0)
    assert_eq!(u16::from(recs[2].inner().flags()), 147); // r_ctot → CTOT (index 2)
    assert_eq!(u16::from(recs[4].inner().flags()), 163); // r_ctob → CTOB (index 1)
}

// ===========================================================================
// `--combined_index --pbat` (v2.x Phase 4) paired-end end-to-end. PBAT is the G→A-pass
// half of non-dir, STANDALONE: ONE both-strands pass with `-1 G→A_R1 -2 C→T_R2` →
// CTOT/CTOB. The fake is G→A-only (single pass) — it does NOT model a C→T pass, since
// `run_pe_combined_pbat` never spawns one (review B-I2: a C→T arm would be dead and
// could mask a wrong-pass bug). OT/OB are unreachable → must be absent from the output.
// ===========================================================================

/// A PBAT combined-index PE fake `bowtie2` (ONE both-strands instance, `-1 G→A_R1
/// -2 C→T_R2`). G→A-only: per base read id, `r_ctot` → CTOT (R1 83 rev / R2 163 on
/// `_CT_converted` at chr1:5) and `r_ctob` → CTOB (R1 99 / R2 147 on `_GA_converted`
/// at chr1:5); anything else → an unmapped (77/141) pair. The `-1` input is the
/// G→A-converted R1 (`*_G_to_A*`); the fake does not branch on it (single pass) but
/// reads it for the read ids. Strips the `/1/1` tag the PE conversion adds.
#[cfg(unix)]
fn make_fake_bowtie2_pe_combined_pbat(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""
for a in "$@"; do [ "$prev" = "-1" ] && m1="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
  if (id=="r_ctot") {
    print id "/1\t83\tchr1_CT_converted\t5\t42\t6M\t=\t5\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    print id "/2\t163\tchr1_CT_converted\t5\t42\t6M\t=\t5\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
  } else if (id=="r_ctob") {
    print id "/1\t99\tchr1_GA_converted\t5\t42\t6M\t=\t5\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    print id "/2\t147\tchr1_GA_converted\t5\t42\t6M\t=\t5\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
  } else {
    print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI";
  } }' "$m1"
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// `--combined_index --pbat` PE end-to-end: the single G→A pass produces CTOT + CTOB
/// (the G→A-pass strands), and OT/OB are absent (the PBAT signature). Asserts the
/// per-strand R1 FLAGs (PE output arm): CTOT→147 (index 2), CTOB→163 (index 1), and
/// that no record carries an OT (99) or OB (83) R1 FLAG.
#[cfg(unix)]
#[test]
fn combined_index_pe_pbat_strands_end_to_end() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_combined_pbat(bins.path());
    let r1 = genome.path().join("r1.fq");
    let r2 = genome.path().join("r2.fq");
    // 2 pairs (no mate suffix — conversion inserts /1/1,/2/2): r_ctot, r_ctob (both
    // map on the single G→A pass → CTOT/CTOB).
    let reads = b"@r_ctot\nACGTAC\n+\nFFFFFF\n@r_ctob\nACGTAC\n+\nFFFFFF\n";
    fs::write(&r1, reads).unwrap();
    fs::write(&r2, reads).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--pbat")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Combined-index mode, paired-end PBAT (EXPERIMENTAL")
                .and(predicate::str::contains("unique best alignments:   2")),
        );

    let bam = outdir.path().join("r1_bismark_bt2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 4); // 2 pairs, in input order: r_ctot, r_ctob
    // R1 FLAG per pair (the PE output-arm strand discriminator):
    assert_eq!(u16::from(recs[0].inner().flags()), 147); // r_ctot → CTOT (index 2)
    assert_eq!(u16::from(recs[2].inner().flags()), 163); // r_ctob → CTOB (index 1)
    // PBAT signature: OT/OB are unreachable → no PAIR leads with an OT (99) or OB (83)
    // record. (The per-pair FIRST record carries the strand identity; the second record
    // legitimately reuses 99/83 as the mate flag — OT=(99,147) vs CTOT=(147,99) share the
    // two values in opposite order, so check only the even-indexed first records.)
    for first in recs.iter().step_by(2) {
        let f = u16::from(first.inner().flags());
        assert_ne!(
            f, 99,
            "OT pair (leads with FLAG 99) must be absent under pbat"
        );
        assert_ne!(
            f, 83,
            "OB pair (leads with FLAG 83) must be absent under pbat"
        );
    }
}

// ===========================================================================
// `--combined_index --hisat2` (v2.x Phase 5) paired-end end-to-end. HISAT2 PE reuses
// the EXACT Bowtie 2 PE-combined machinery (the spawn runs the hisat2 binary; the PE
// argv shape is identical), so the only HISAT2-specific behaviour to exercise is the
// runner-up surfacing as a contiguous **secondary pair** (FLAG 0x100, with AS on both
// mates) — the combined gather must collect it and `select_core_pe` must consume it as
// the MAPQ runner-up (not write it as a spurious extra alignment). Uses a `.ht2`
// combined fixture (NOT `.bt2`) so discovery finds the HISAT2 combined index.
// ===========================================================================

/// A genome dir with a combined HISAT2 index (`.ht2`) — the HISAT2 analogue of
/// [`make_genome_combined`]. CT/GA/Combined each get 8 `.ht2` files (no `rev.*`).
fn make_genome_combined_hisat2(dir: &Path) {
    let ct = dir.join("Bisulfite_Genome").join("CT_conversion");
    let ga = dir.join("Bisulfite_Genome").join("GA_conversion");
    let comb = dir.join("Bisulfite_Genome").join("Combined");
    fs::create_dir_all(&ct).unwrap();
    fs::create_dir_all(&ga).unwrap();
    fs::create_dir_all(&comb).unwrap();
    for n in 1..=8 {
        fs::write(ct.join(format!("BS_CT.{n}.ht2")), b"x").unwrap();
        fs::write(ga.join(format!("BS_GA.{n}.ht2")), b"x").unwrap();
        fs::write(comb.join(format!("BS_combined.{n}.ht2")), b"x").unwrap();
    }
    fs::write(dir.join("genome.fa"), b">chr1\nACGTACGTACGTACGT\n").unwrap(); // 16 bp
}

/// A fake combined-index PE **`hisat2`** (banner `hisat2-align-s version 2.2.2`). Per
/// base read id it emits a primary pair followed by a CONTIGUOUS secondary pair (FLAG
/// `0x100` on both mates, with its own `AS:i:`) — the HISAT2 `-k 2` runner-up shape:
/// - `r_ot`   → primary OT (R1 99 / R2 147 on `_CT`, AS 0) + a lower-AS spurious secondary
///   (R1 355 / R2 403 on `_GA`, AS −6, `SEQ=*` as `--omit-sec-seq`, but `MD:Z:` present)
///   → UniqueBest OT, the secondary consumed as the MAPQ runner-up (NOT written).
/// - `r_md`   → primary OT (AS 0, MD present) + a **tied VALID OB** secondary (R1 339 /
///   R2 419 on `_GA`, AS 0, **MD ABSENT**) → both enter the best-sum MD path → the
///   secondary's missing MD aborts loud ("Failed to extract MD tag") — the PE C1 trip-wire.
/// - else     → unmapped (77/141).
#[cfg(unix)]
fn make_fake_hisat2_pe_combined(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "hisat2-align-s version 2.2.2"; exit 0;; esac
m1=""; prev=""
for a in "$@"; do [ "$prev" = "-1" ] && m1="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
  if (id=="r_ot") {
    print id "/1\t99\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tZS:i:-6\tMD:Z:6";
    print id "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tZS:i:-6\tMD:Z:6";
    print id "/1\t355\tchr1_GA_converted\t5\t42\t6M\t=\t5\t6\t*\tFFFFFF\tAS:i:-6\tMD:Z:6";
    print id "/2\t403\tchr1_GA_converted\t5\t42\t6M\t=\t5\t-6\t*\tFFFFFF\tAS:i:-6\tMD:Z:6";
  } else if (id=="r_md") {
    print id "/1\t99\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    print id "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    print id "/1\t339\tchr1_GA_converted\t9\t42\t6M\t=\t9\t-6\t*\tFFFFFF\tAS:i:0";
    print id "/2\t419\tchr1_GA_converted\t9\t42\t6M\t=\t9\t6\t*\tFFFFFF\tAS:i:0";
  } else {
    print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI";
  } }' "$m1"
"#;
    write_exec(&dir.join("hisat2"), script);
}

/// Phase 5: `--hisat2 --combined_index` PE (directional) runs end-to-end over the
/// combined `.ht2` index. The HISAT2 secondary pair (FLAG 0x100) is gathered + consumed
/// as the runner-up — the OT pair is the unique best (R1 FLAG 99), exactly 2 records
/// written (the secondary is NOT a spurious extra), and the report says HISAT2.
#[cfg(unix)]
#[test]
fn combined_index_pe_hisat2_directional_end_to_end() {
    let genome = TempDir::new().unwrap();
    make_genome_combined_hisat2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_hisat2_pe_combined(bins.path());
    let r1 = genome.path().join("r1.fq");
    let r2 = genome.path().join("r2.fq");
    fs::write(&r1, b"@r_ot\nACGTAC\n+\nFFFFFF\n").unwrap();
    fs::write(&r2, b"@r_ot\nACGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--hisat2")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Combined-index mode, paired-end")
                .and(predicate::str::contains("HISAT2"))
                .and(predicate::str::contains("unique best alignments:   1")),
        );

    let bam = outdir.path().join("r1_bismark_hisat2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 2); // ONLY the winning OT pair — the secondary is consumed, not written
    assert_eq!(u16::from(recs[0].inner().flags()), 99); // R1 → OT
    assert_eq!(u16::from(recs[1].inner().flags()), 147); // R2 → OT
}

/// Phase 5 C1 trip-wire: a HISAT2 secondary pair that is VALID and TIES the primary at
/// the best sum enters the MD-required path (`select_core_pe` extracts MD for every
/// best-sum pair, not just the winner). If that secondary's `MD:Z:` is absent the run
/// MUST abort loud — never silently drop it. (Real HISAT2 `--omit-sec-seq` keeps `MD:Z:`,
/// the gate confirms; this proves the never-silent failure if it ever did not.)
#[cfg(unix)]
#[test]
fn combined_index_pe_hisat2_tied_secondary_missing_md_aborts() {
    let genome = TempDir::new().unwrap();
    make_genome_combined_hisat2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_hisat2_pe_combined(bins.path());
    let r1 = genome.path().join("r1.fq");
    let r2 = genome.path().join("r2.fq");
    fs::write(&r1, b"@r_md\nACGTAC\n+\nFFFFFF\n").unwrap();
    fs::write(&r2, b"@r_md\nACGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--hisat2")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to extract MD tag"));
}

/// A NON-DIRECTIONAL combined-index PE fake **`hisat2`** — the HISAT2 analogue of
/// `make_fake_bowtie2_pe_combined_nondir`: discriminates the two both-strands passes by
/// the `-1` converted-input suffix (`*_C_to_T*` → OT; `*_G_to_A*` → CTOT/CTOB). Proves
/// the non-dir 2-stream driver threads `config.aligner = Hisat2` into BOTH spawns and
/// produces the G→A-pass strands under HISAT2.
#[cfg(unix)]
fn make_fake_hisat2_pe_combined_nondir(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "hisat2-align-s version 2.2.2"; exit 0;; esac
m1=""; prev=""
for a in "$@"; do [ "$prev" = "-1" ] && m1="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
case "$m1" in
  *_C_to_T*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      if (id=="r_ot") {
        print id "/1\t99\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
        print id "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      } else {
        print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI";
      } }' "$m1" ;;
  *_G_to_A*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      if (id=="r_ctot") {
        print id "/1\t83\tchr1_CT_converted\t5\t42\t6M\t=\t5\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
        print id "/2\t163\tchr1_CT_converted\t5\t42\t6M\t=\t5\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      } else if (id=="r_ctob") {
        print id "/1\t99\tchr1_GA_converted\t5\t42\t6M\t=\t5\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
        print id "/2\t147\tchr1_GA_converted\t5\t42\t6M\t=\t5\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      } else {
        print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI";
      } }' "$m1" ;;
esac
"#;
    write_exec(&dir.join("hisat2"), script);
}

/// Phase 5: `--hisat2 --combined_index --non_directional` PE end-to-end (parallel model
/// (a), two both-strands HISAT2 passes). Produces OT (C→T pass) + CTOT/CTOB (G→A pass) —
/// the same per-strand R1 FLAGs as the Bowtie 2 non-dir e2e (OT→99, CTOT→147, CTOB→163),
/// proving the 2-stream driver runs HISAT2 on both passes.
#[cfg(unix)]
#[test]
fn combined_index_pe_hisat2_nondir_strands_end_to_end() {
    let genome = TempDir::new().unwrap();
    make_genome_combined_hisat2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_hisat2_pe_combined_nondir(bins.path());
    let r1 = genome.path().join("r1.fq");
    let r2 = genome.path().join("r2.fq");
    let reads =
        b"@r_ot\nACGTAC\n+\nFFFFFF\n@r_ctot\nACGTAC\n+\nFFFFFF\n@r_ctob\nACGTAC\n+\nFFFFFF\n";
    fs::write(&r1, reads).unwrap();
    fs::write(&r2, reads).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--hisat2")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Combined-index mode, paired-end")
                .and(predicate::str::contains("HISAT2"))
                .and(predicate::str::contains("unique best alignments:   3")),
        );

    let bam = outdir.path().join("r1_bismark_hisat2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 6); // 3 pairs in input order: r_ot, r_ctot, r_ctob
    assert_eq!(u16::from(recs[0].inner().flags()), 99); // r_ot   → OT   (index 0)
    assert_eq!(u16::from(recs[2].inner().flags()), 147); // r_ctot → CTOT (index 2)
    assert_eq!(u16::from(recs[4].inner().flags()), 163); // r_ctob → CTOB (index 1)
}

// ===========================================================================
// `--combined_index --non_directional` end-to-end (phase 5, model (a)).
// TWO passes over the SAME `BS_combined` index: the fake `bowtie2` dispatches on
// the `-U` INPUT-FILE conversion tag (`*_C_to_T*` → OT/(none), `*_G_to_A*` →
// CTOT/CTOB) — NOT the index basename, since both passes share one index. This is
// the axis the directional `make_fake_bowtie2_combined` cannot model.
// ===========================================================================

/// A fake `bowtie2` for the non-directional combined run: the C→T pass aligns OT
/// reads; the G→A pass aligns CTOT (`rev+_CT_converted`) / CTOB (`fwd+_GA_converted`)
/// reads. Dispatch is on the converted-read input file (`_C_to_T` vs `_G_to_A`).
#[cfg(unix)]
fn make_fake_bowtie2_combined_nondir(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""
for a in "$@"; do [ "$prev" = "-U" ] && inp="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
case "$inp" in
  *_C_to_T*)
    awk 'NR%4==1 {
      id=$1; sub(/^@/,"",id);
      if (id=="r_ot")        print id "\t0\tchr1_CT_converted\t1\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else if (id=="r_keep") print id "\t0\tchr1_CT_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else print id "\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF";
    }' "$inp" ;;
  *_G_to_A*)
    awk 'NR%4==1 {
      id=$1; sub(/^@/,"",id);
      if (id=="r_ctob")      print id "\t0\tchr1_GA_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else if (id=="r_ctot") print id "\t16\tchr1_CT_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else if (id=="r_keep") print id "\t0\tchr1_GA_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else print id "\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF";
    }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// THE §4b telomeric KEEP cell: an OT hit (C→T pass) and a CTOB hit (G→A pass) at
/// the SAME locus/equal AS → KEPT (one record), won by **CTOB (index 3)** →
/// FLAG 16 / XR:Z:GA / XG:Z:GA. Report: unique_best=1, unsuitable=0.
#[cfg(unix)]
#[test]
fn combined_index_nondir_same_position_ot_ctob_kept_as_ctob() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_nondir(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_keep"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("NON-DIRECTIONAL")
                .and(predicate::str::contains("unique best alignments:   1"))
                .and(predicate::str::contains("ambiguous (unsuitable):   0")),
        );

    // ONE record: the CTOB winner of the OT×CTOB same-position collision.
    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 16); // CTOB (index 3) → FLAG 16
    assert_eq!(usize::from(r.alignment_start().unwrap()), 5);
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    let v = |t: [u8; 2]| r.data().get(&Tag::from(t)).cloned();
    assert_eq!(v(*b"XR"), Some(Value::String("GA".into()))); // CTOB → XR:GA
    assert_eq!(v(*b"XG"), Some(Value::String("GA".into()))); // CTOB → XG:GA
}

/// Strand-mix + counter ownership: OT (C→T pass), CTOT + CTOB (G→A pass), and a
/// both-passes-miss read → 3 unique / 1 no-alignment / 3 BAM records, and the NEW
/// non-dir counters (`GA/CT`=CTOT, `GA/GA`=CTOB) populated in the report. Also
/// exercises the common single-pass-hit path (each read hits exactly one pass).
#[cfg(unix)]
#[test]
fn combined_index_nondir_strand_mix_records_and_counters() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_nondir(bins.path());
    let read = write_reads_ids(
        genome.path(),
        "reads.fq",
        &["r_ot", "r_ctot", "r_ctob", "r_miss"],
    );
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   3"));

    let report = fs::read_to_string(outdir.path().join("reads_bismark_bt2_SE_report.txt")).unwrap();
    assert!(report.contains("Combined-index mode, non-directional"));
    assert!(report.contains("Sequences analysed in total:\t4\n"));
    // The 4-strand split: OT + CTOT + CTOB each once; OB none (non-dir newly
    // populates the GA/CT + GA/GA rows).
    assert!(report.contains("CT/CT:\t1\t((converted) top strand)"));
    assert!(report.contains("CT/GA:\t0\t((converted) bottom strand)"));
    assert!(report.contains("GA/CT:\t1\t(complementary to (converted) top strand)"));
    assert!(report.contains("GA/GA:\t1\t(complementary to (converted) bottom strand)"));
    assert!(report.contains("Sequences with no alignments under any condition:\t1\n"));

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    assert_eq!(reader.records().count(), 3); // OT + CTOT + CTOB
}

// ===========================================================================
// `--combined_index --combined_index_single_pass --non_directional` end-to-end
// (phase 8, model b). ONE Bowtie 2 pass over the conversion-TAGGED interleaved
// reads (one index load). The fake keys on the READ's `__CT`/`__GA` qname tag
// (one input file), preserving the tag in the output qname so the driver splits
// it back into the C→T (OT/OB) and G→A (CTOT/CTOB) groups → the SAME
// `select_nondir` union as model (a). NON-FAITHFUL (validated-accurate).
// ===========================================================================

/// A fake `bowtie2` for the model-(b) tagged run: one tagged interleaved input;
/// dispatch on each read's `__CT`/`__GA` qname tag (KEEP the tag in the output
/// qname). `__CT` → OT (`fwd+_CT_converted`); `__GA` → CTOT (`rev+_CT_converted`)
/// / CTOB (`fwd+_GA_converted`). Every tagged read gets a line (FLAG-4 miss
/// otherwise) — so both halves are always present, as real Bowtie 2 emits.
#[cfg(unix)]
fn make_fake_bowtie2_combined_nondir_tagged(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""
for a in "$@"; do [ "$prev" = "-U" ] && inp="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%4==1 {
  q=$1; sub(/^@/,"",q);
  base=q; sub(/__(CT|GA)$/,"",base);
  tag=substr(q, length(q)-1);
  if (tag=="CT") {
    if (base=="r_ot")        print q "\t0\tchr1_CT_converted\t1\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    else if (base=="r_keep") print q "\t0\tchr1_CT_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    else print q "\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF";
  } else {
    if (base=="r_ctob")      print q "\t0\tchr1_GA_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    else if (base=="r_ctot") print q "\t16\tchr1_CT_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    else if (base=="r_keep") print q "\t0\tchr1_GA_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    else print q "\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF";
  }
}' "$inp"
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// A lone OT read through model (b): one record, FLAG 0 / XR:Z:CT / XG:Z:CT, plus
/// the never-silent model-(b) banner + report marker.
#[cfg(unix)]
#[test]
fn combined_index_single_pass_ot_read_end_to_end() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_nondir_tagged(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_ot"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--combined_index_single_pass")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("SINGLE-PASS")
                .and(predicate::str::contains("model b"))
                .and(predicate::str::contains("unique best alignments:   1")),
        );

    let report = fs::read_to_string(outdir.path().join("reads_bismark_bt2_SE_report.txt")).unwrap();
    assert!(report.contains("SINGLE-PASS (model b"));

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 0); // OT (index 0) → FLAG 0
    assert_eq!(usize::from(r.alignment_start().unwrap()), 1);
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    let v = |t: [u8; 2]| r.data().get(&Tag::from(t)).cloned();
    assert_eq!(v(*b"XR"), Some(Value::String("CT".into()))); // OT → XR:CT
    assert_eq!(v(*b"XG"), Some(Value::String("CT".into()))); // OT → XG:CT
}

/// The §4b telomeric KEEP through model (b): the single tagged stream yields an OT
/// (`r_keep__CT`) and a CTOB (`r_keep__GA`) at the SAME locus/equal AS → KEPT (one
/// record), won by CTOB (index 3) → FLAG 16 / XR:Z:GA / XG:Z:GA. Same outcome as
/// model (a)'s two-stream KEEP — proves the split feeds `select_nondir` identically.
#[cfg(unix)]
#[test]
fn combined_index_single_pass_same_position_ot_ctob_kept_as_ctob() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_nondir_tagged(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_keep"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--combined_index_single_pass")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("unique best alignments:   1")
                .and(predicate::str::contains("ambiguous (unsuitable):   0")),
        );

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 16); // CTOB (index 3) → FLAG 16
    assert_eq!(usize::from(r.alignment_start().unwrap()), 5);
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    let v = |t: [u8; 2]| r.data().get(&Tag::from(t)).cloned();
    assert_eq!(v(*b"XR"), Some(Value::String("GA".into())));
    assert_eq!(v(*b"XG"), Some(Value::String("GA".into())));
}

/// Strand-mix through model (b): OT + CTOT + CTOB + a both-halves-miss read → 3
/// unique / 1 no-alignment / 3 BAM records, with the 4-strand non-dir report
/// counts + the model-(b) marker. Exercises the single-stream split for all four
/// classes (incl. the all-miss read whose __CT AND __GA halves are both FLAG-4).
#[cfg(unix)]
#[test]
fn combined_index_single_pass_strand_mix() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_nondir_tagged(bins.path());
    let read = write_reads_ids(
        genome.path(),
        "reads.fq",
        &["r_ot", "r_ctot", "r_ctob", "r_miss"],
    );
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--combined_index_single_pass")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   3"));

    let report = fs::read_to_string(outdir.path().join("reads_bismark_bt2_SE_report.txt")).unwrap();
    assert!(report.contains("SINGLE-PASS (model b"));
    assert!(report.contains("Sequences analysed in total:\t4\n"));
    assert!(report.contains("CT/CT:\t1\t((converted) top strand)")); // OT
    assert!(report.contains("GA/CT:\t1\t(complementary to (converted) top strand)")); // CTOT
    assert!(report.contains("GA/GA:\t1\t(complementary to (converted) bottom strand)")); // CTOB
    assert!(report.contains("Sequences with no alignments under any condition:\t1\n"));

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    assert_eq!(reader.records().count(), 3);
}

/// The model-(b) scope guard rejects loudly (never-silent): `--combined_index_single_pass`
/// requires `--combined_index --non_directional`. Runs before genome discovery, so a
/// plain genome suffices.
#[cfg(unix)]
#[test]
fn combined_index_single_pass_scope_guard_rejects() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let read = make_read(genome.path());

    // tagged WITHOUT --combined_index
    bin()
        .arg("--combined_index_single_pass")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg(&read)
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires --combined_index"));

    // tagged + --combined_index but DIRECTIONAL (no --non_directional)
    bin()
        .arg("--combined_index")
        .arg("--combined_index_single_pass")
        .arg("--genome")
        .arg(genome.path())
        .arg(&read)
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires --non_directional"));
}

/// A malformed fake `bowtie2` that emits ONLY the `__CT` half of every tagged read
/// (drops the `__GA` line a real Bowtie 2 would always emit). Used to TRIP the
/// model-(b) never-silent "missing half" guard (§3.3 (ii)) — the e2e fakes above
/// always emit both halves, so this is the only way to fire the hardened guard.
#[cfg(unix)]
fn make_fake_bowtie2_tagged_drop_ga_half(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""
for a in "$@"; do [ "$prev" = "-U" ] && inp="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%4==1 {
  q=$1; sub(/^@/,"",q);
  tag=substr(q, length(q)-1);
  # __CT → an OT hit; __GA → emit NOTHING (the dropped half).
  if (tag=="CT") print q "\t0\tchr1_CT_converted\t1\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
}' "$inp"
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// Never-silent: if the tagged stream is missing a read's `__GA` half (a desynced /
/// malformed aligner), the model-(b) driver dies loud rather than mis-calling on a
/// half-populated union (§3.3 (ii) — the contract the rev-1 plan-review hardened).
#[cfg(unix)]
#[test]
fn combined_index_single_pass_missing_half_dies_loud() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_tagged_drop_ga_half(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_x"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--combined_index_single_pass")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing its __CT or __GA half"));
}

// ===========================================================================
// `--combined_index --pbat` end-to-end (phase 7). ONE both-strands pass over
// `BS_combined` fed the G→A-converted reads → CTOT (rev+_CT_converted) / CTOB
// (fwd+_GA_converted). The fake dispatches on the `-U` G→A input (pbat's only
// converted file). Routes `pbat=false` + classify-supplied index 2/3.
// ===========================================================================

/// A fake `bowtie2` for the PBAT combined run: fed the G→A-converted reads, emits
/// CTOT (`rev+_CT_converted`) / CTOB (`fwd+_GA_converted`) lines.
#[cfg(unix)]
fn make_fake_bowtie2_combined_pbat(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""
for a in "$@"; do [ "$prev" = "-U" ] && inp="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
case "$inp" in
  *_G_to_A*)
    awk 'NR%4==1 {
      id=$1; sub(/^@/,"",id);
      if (id=="r_ctot")      print id "\t16\tchr1_CT_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else if (id=="r_ctob") print id "\t0\tchr1_GA_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else if (id=="r_keep") { print id "\t16\tchr1_CT_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6"; print id "\t0\tchr1_GA_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6"; }
      else print id "\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF";
    }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// A CTOB read (G→A pass, `fwd+_GA_converted`) → index 3 → FLAG 16 / XR:Z:GA /
/// XG:Z:GA. The never-silent banner names the PBAT combined mode; the report
/// carries the `--pbat` library line.
#[cfg(unix)]
#[test]
fn combined_index_pbat_ctob_read_end_to_end() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_pbat(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_ctob"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--pbat")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("Combined-index mode, PBAT")
                .and(predicate::str::contains("unique best alignments:   1")),
        );

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 16); // CTOB (index 3) → FLAG 16
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    let v = |t: [u8; 2]| r.data().get(&Tag::from(t)).cloned();
    assert_eq!(v(*b"XR"), Some(Value::String("GA".into()))); // CTOB → XR:GA
    assert_eq!(v(*b"XG"), Some(Value::String("GA".into()))); // CTOB → XG:GA

    let report = fs::read_to_string(outdir.path().join("reads_bismark_bt2_SE_report.txt")).unwrap();
    assert!(report.contains("Combined-index mode, PBAT"));
    assert!(report.contains("alignments to original strands (OT and OB)")); // pbat library line
}

/// A CTOT read (G→A pass, `rev+_CT_converted`) → index 2 → FLAG 0 / XR:Z:GA /
/// XG:Z:CT (the symmetric counterpart of the CTOB cell — review B-L2).
#[cfg(unix)]
#[test]
fn combined_index_pbat_ctot_read_end_to_end() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_pbat(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_ctot"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--pbat")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   1"));

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 0); // CTOT (index 2) → FLAG 0
    use noodles_sam::alignment::record::data::field::Tag;
    use noodles_sam::alignment::record_buf::data::field::Value;
    let v = |t: [u8; 2]| r.data().get(&Tag::from(t)).cloned();
    assert_eq!(v(*b"XR"), Some(Value::String("GA".into()))); // CTOT → XR:GA
    assert_eq!(v(*b"XG"), Some(Value::String("CT".into()))); // CTOT → XG:CT
}

/// The PBAT §4b analog: CTOT (rev+_CT_converted) and CTOB (fwd+_GA_converted) at the
/// SAME locus/equal AS → KEPT, won by CTOB (index 3).
#[cfg(unix)]
#[test]
fn combined_index_pbat_same_position_kept_as_ctob() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_pbat(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_keep"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--pbat")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("unique best alignments:   1")
                .and(predicate::str::contains("ambiguous (unsuitable):   0")),
        );

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 1);
    let r = recs[0].inner();
    assert_eq!(u16::from(r.flags()), 16); // CTOB wins the same-position tie
    assert_eq!(usize::from(r.alignment_start().unwrap()), 5);
}

/// Strand counts + report wording: a CTOT + a CTOB + a miss → 2 records; report
/// shows the `--pbat` line + GA/CT=1 (CTOT) + GA/GA=1 (CTOB) + CT/CT=0 (no OT).
#[cfg(unix)]
#[test]
fn combined_index_pbat_strand_counts() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_pbat(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_ctot", "r_ctob", "r_miss"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--pbat")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(predicate::str::contains("unique best alignments:   2"));

    let report = fs::read_to_string(outdir.path().join("reads_bismark_bt2_SE_report.txt")).unwrap();
    assert!(report.contains("alignments to original strands (OT and OB)")); // pbat library line
    assert!(report.contains("Sequences analysed in total:\t3\n"));
    assert!(report.contains("CT/CT:\t0\t((converted) top strand)")); // no OT
    assert!(report.contains("GA/CT:\t1\t(complementary to (converted) top strand)")); // CTOT
    assert!(report.contains("GA/GA:\t1\t(complementary to (converted) bottom strand)")); // CTOB
    assert!(report.contains("Sequences with no alignments under any condition:\t1\n"));

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    assert_eq!(reader.records().count(), 2); // CTOT + CTOB
}

// ===========================================================================
// `--combined_index --non_directional --combined_index_sequential` end-to-end
// (phase 9, model (a) SEQUENTIAL low-RSS variant). Same two-pass union as model
// (a), but pass 1 (C→T) is spilled to disk and its Bowtie 2 exits before pass 2
// (G→A) spawns. BYTE-IDENTICAL to parallel model (a) — both feed Bowtie 2 the SAME
// untagged converted files (exec-model spike C2). Reuses `make_fake_bowtie2_combined
// _nondir` (dispatches on the `-U` input tag — unchanged by the exec model).
// ===========================================================================

/// Collect a combined-non-dir BAM's alignment records (header/@PG excluded) for a
/// run with or without `--combined_index_sequential`, optionally `--upto`-limited.
#[cfg(unix)]
fn run_combined_nondir_records(
    sequential: bool,
    upto: Option<&str>,
    ids: &[&str],
) -> Vec<noodles_sam::alignment::RecordBuf> {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_nondir(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", ids);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    let mut cmd = bin();
    cmd.arg("--combined_index").arg("--non_directional");
    if sequential {
        cmd.arg("--combined_index_sequential");
    }
    if let Some(u) = upto {
        cmd.arg("--upto").arg(u);
    }
    cmd.arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    // No leftover spill temp file after a successful run (sequential cleans it up).
    if sequential {
        let leftover = fs::read_dir(temp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().ends_with(".ct_pass.sam"));
        assert!(!leftover, "sequential spill temp file must be cleaned up");
    }

    let bam = outdir.path().join("reads_bismark_bt2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    reader
        .records()
        .map(|r| r.unwrap().inner().clone())
        .collect()
}

/// THE byte-identity property: the sequential variant's BAM records are identical to
/// parallel model (a)'s on the same strand-mix inputs (OT + CTOT + CTOB; a both-miss
/// dropped). Decision-equivalence is structural (exec-model spike C2), so this is the
/// local proof the gate scales to real data.
#[cfg(unix)]
#[test]
fn combined_index_sequential_byte_identical_to_model_a() {
    let ids = ["r_ot", "r_ctot", "r_ctob", "r_miss"];
    let model_a = run_combined_nondir_records(false, None, &ids);
    let sequential = run_combined_nondir_records(true, None, &ids);
    assert_eq!(model_a.len(), 3); // OT + CTOT + CTOB; r_miss → no record
    assert_eq!(
        model_a, sequential,
        "sequential combined BAM must be byte-identical to parallel model (a)"
    );
}

/// Byte-identity holds under `--upto` early-break too (the sequential drive loop
/// leaves the file-backed C→T stream + the live G→A stream non-exhausted; the run
/// exits cleanly with the spill cleaned up).
#[cfg(unix)]
#[test]
fn combined_index_sequential_upto_early_break_matches_model_a() {
    let ids = ["r_ot", "r_ctot", "r_ctob", "r_miss"];
    let model_a = run_combined_nondir_records(false, Some("2"), &ids);
    let sequential = run_combined_nondir_records(true, Some("2"), &ids);
    assert_eq!(model_a.len(), 2); // first two reads (r_ot, r_ctot) → 2 records
    assert_eq!(
        model_a, sequential,
        "sequential --upto must match model (a) --upto byte-for-byte"
    );
}

/// The never-silent SEQUENTIAL banner (STDERR) + report marker + the 4-strand
/// non-dir counts (mirrors the model-(a) strand-mix cell).
#[cfg(unix)]
#[test]
fn combined_index_sequential_banner_and_report_marker() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_nondir(bins.path());
    let read = write_reads_ids(
        genome.path(),
        "reads.fq",
        &["r_ot", "r_ctot", "r_ctob", "r_miss"],
    );
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--non_directional")
        .arg("--combined_index_sequential")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("NON-DIRECTIONAL SEQUENTIAL")
                .and(predicate::str::contains(
                    "one combined index is resident at a time",
                ))
                .and(predicate::str::contains("unique best alignments:   3")),
        );

    let report = fs::read_to_string(outdir.path().join("reads_bismark_bt2_SE_report.txt")).unwrap();
    assert!(report.contains("non-directional SEQUENTIAL"));
    assert!(report.contains("byte-identical to the default parallel combined non-dir path"));
    assert!(report.contains("Sequences analysed in total:\t4\n"));
    assert!(report.contains("CT/CT:\t1\t((converted) top strand)"));
    assert!(report.contains("GA/CT:\t1\t(complementary to (converted) top strand)"));
    assert!(report.contains("GA/GA:\t1\t(complementary to (converted) bottom strand)"));
    assert!(report.contains("Sequences with no alignments under any condition:\t1\n"));
}

/// A read that misses BOTH passes → `NoAlignment` → 0 BAM records (the double-miss
/// edge; both passes emit a lone FLAG-4 line that `select_nondir` filters).
#[cfg(unix)]
#[test]
fn combined_index_sequential_double_miss_no_alignment() {
    let recs = run_combined_nondir_records(true, None, &["r_miss"]);
    assert!(recs.is_empty());
}

/// A fake `bowtie2` whose C→T (pass 1) invocation EXITS NON-ZERO (the G→A path is
/// normal). Used to prove the sequential driver fails closed at `ct.finish()?` —
/// before pass 2 is ever spawned.
#[cfg(unix)]
fn make_fake_bowtie2_combined_nondir_ct_fails(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""
for a in "$@"; do [ "$prev" = "-U" ] && inp="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
case "$inp" in
  *_C_to_T*) exit 3 ;;   # pass 1 (C->T) fails after emitting only the header
  *_G_to_A*)
    awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF"; }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// Never-silent fail-closed: a non-zero pass-1 (C→T) Bowtie 2 exit aborts the run at
/// `ct_stream.finish()?` (the §3 edge / RSS-boundary guard), not silently.
#[cfg(unix)]
#[test]
fn combined_index_sequential_pass1_nonzero_exit_dies() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_nondir_ct_fails(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_ot"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--non_directional")
        .arg("--combined_index_sequential")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .failure()
        .stderr(predicate::str::contains("exited unsuccessfully"));
}

/// Scope guard (never-silent): `--combined_index_sequential` requires `--combined_index
/// --non_directional` and is mutually exclusive with `--combined_index_single_pass`.
#[cfg(unix)]
#[test]
fn combined_index_sequential_scope_guard_rejects() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let read = make_read(genome.path());

    // sequential WITHOUT --combined_index
    bin()
        .arg("--combined_index_sequential")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg(&read)
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires --combined_index"));

    // sequential + --combined_index but DIRECTIONAL (no --non_directional)
    bin()
        .arg("--combined_index")
        .arg("--combined_index_sequential")
        .arg("--genome")
        .arg(genome.path())
        .arg(&read)
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires --non_directional"));

    // sequential + single_pass together → competing exec models
    bin()
        .arg("--combined_index")
        .arg("--non_directional")
        .arg("--combined_index_sequential")
        .arg("--combined_index_single_pass")
        .arg("--genome")
        .arg(genome.path())
        .arg(&read)
        .assert()
        .failure()
        .stderr(predicate::str::contains("competing"));
}

/// A fake `bowtie2` whose C→T (pass 1) invocation emits a record with a qname that
/// matches NO input read (the G→A path is a normal miss). Used to TRIP the inherited
/// desync guard through a DISK-REPLAYED `FileSamStream`: the spilled pass-1 record's
/// head qname won't equal the re-read input id.
#[cfg(unix)]
fn make_fake_bowtie2_combined_nondir_ct_wrong_qname(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
inp=""; prev=""
for a in "$@"; do [ "$prev" = "-U" ] && inp="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
case "$inp" in
  *_C_to_T*)
    awk 'NR%4==1 { print "ghost_qname\t0\tchr1_CT_converted\t1\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6"; }' "$inp" ;;
  *_G_to_A*)
    awk 'NR%4==1 { id=$1; sub(/^@/,"",id); print id "\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF"; }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// Never-silent: the desync guard in `drive_merge_combined_nondir` still fires loud
/// when the C→T stream is a DISK-REPLAYED `FileSamStream` (the sequential exec model),
/// not just a live process. The pass-1 fake spills a record whose qname ≠ the re-read
/// input id → the run dies loudly rather than silently mis-pairing the union.
#[cfg(unix)]
#[test]
fn combined_index_sequential_desync_dies_loud() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_combined_nondir_ct_wrong_qname(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_ot"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--non_directional")
        .arg("--combined_index_sequential")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("desync")
                .and(predicate::str::contains("C->T pass stream head")),
        );
}

// ===========================================================================
// `--combined_index` (v2.x) PAIRED-END NON-DIRECTIONAL low-RAM variants (PLAN 06102026
// phase 6): SEQUENTIAL (faithful, byte-identical to model (a)) + SINGLE-PASS model (b)
// (non-faithful tagged). Bowtie 2-only (the config guard enforces it). The SE analogs are
// #959 (sequential) + #958 (single-pass). The SEQUENTIAL byte-identity reuses
// `make_fake_bowtie2_pe_combined_nondir` (dispatches on the `-1` `*_C_to_T*`/`*_G_to_A*`
// tag — unchanged by the exec model); the SINGLE-PASS path needs a tagged-qname PE fake.
// ===========================================================================

/// Write a matched PE FastQ pair (R1 == R2, each `ACGTAC`) for the given base ids.
fn write_pe_reads_ids(dir: &Path, ids: &[&str]) -> (std::path::PathBuf, std::path::PathBuf) {
    let r1 = dir.join("reads_1.fq");
    let r2 = dir.join("reads_2.fq");
    let mut s = String::new();
    for id in ids {
        s.push_str(&format!("@{id}\nACGTAC\n+\nFFFFFF\n"));
    }
    fs::write(&r1, &s).unwrap();
    fs::write(&r2, &s).unwrap();
    (r1, r2)
}

/// Collect a combined-non-dir PE BAM's alignment records for a run with or without
/// `--combined_index_sequential`, optionally `--upto`-limited. Asserts the sequential
/// spill temp file is cleaned up.
#[cfg(unix)]
fn run_combined_pe_nondir_records(
    sequential: bool,
    upto: Option<&str>,
    ids: &[&str],
) -> Vec<noodles_sam::alignment::RecordBuf> {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_combined_nondir(bins.path());
    let (r1, r2) = write_pe_reads_ids(genome.path(), ids);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    let mut cmd = bin();
    cmd.arg("--combined_index").arg("--non_directional");
    if sequential {
        cmd.arg("--combined_index_sequential");
    }
    if let Some(u) = upto {
        cmd.arg("--upto").arg(u);
    }
    cmd.arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success();

    if sequential {
        let leftover = fs::read_dir(temp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().ends_with(".ct_pass.sam"));
        assert!(
            !leftover,
            "sequential PE spill temp file must be cleaned up"
        );
    }

    let bam = outdir.path().join("reads_1_bismark_bt2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    reader
        .records()
        .map(|r| r.unwrap().inner().clone())
        .collect()
}

/// THE byte-identity property (PE): the sequential variant's BAM records are identical to
/// parallel model (a)'s on the same strand-mix inputs (OT + CTOT + CTOB; a both-miss
/// dropped). Decision-equivalence is structural (same untagged converted files; exec-model
/// spike C2), so this is the local proof the oxy md5-gate scales to real data.
#[cfg(unix)]
#[test]
fn combined_index_pe_sequential_byte_identical_to_model_a() {
    let ids = ["r_ot", "r_ctot", "r_ctob", "r_miss"];
    let model_a = run_combined_pe_nondir_records(false, None, &ids);
    let sequential = run_combined_pe_nondir_records(true, None, &ids);
    assert_eq!(model_a.len(), 6); // 3 mapped pairs × 2 records; r_miss → none
    assert_eq!(
        model_a, sequential,
        "sequential PE combined BAM must be byte-identical to parallel model (a)"
    );
}

/// Byte-identity holds under `--upto` early-break too — the PE two-line-per-pair drive
/// loop leaves the file-backed C→T stream + the live G→A stream non-exhausted; the run
/// exits cleanly with the spill cleaned up (the SE `--upto` test's PE analog).
#[cfg(unix)]
#[test]
fn combined_index_pe_sequential_upto_early_break_matches_model_a() {
    let ids = ["r_ot", "r_ctot", "r_ctob", "r_miss"];
    let model_a = run_combined_pe_nondir_records(false, Some("2"), &ids);
    let sequential = run_combined_pe_nondir_records(true, Some("2"), &ids);
    assert_eq!(model_a.len(), 4); // first two pairs (r_ot, r_ctot) → 4 records
    assert_eq!(
        model_a, sequential,
        "sequential PE --upto must match model (a) --upto byte-for-byte"
    );
}

/// A pair that misses BOTH passes → `NoAlignment` → 0 BAM records (the double-miss edge;
/// both passes emit a lone (77,141) pair that `select_pe_nondir` filters).
#[cfg(unix)]
#[test]
fn combined_index_pe_sequential_double_miss_no_alignment() {
    let recs = run_combined_pe_nondir_records(true, None, &["r_miss"]);
    assert!(recs.is_empty());
}

/// The never-silent SEQUENTIAL banner (STDERR) + report marker + the 4-strand non-dir
/// counts (mirrors the SE sequential banner test, PE wording).
#[cfg(unix)]
#[test]
fn combined_index_pe_sequential_banner_and_report_marker() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_combined_nondir(bins.path());
    let (r1, r2) = write_pe_reads_ids(genome.path(), &["r_ot", "r_ctot", "r_ctob", "r_miss"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--non_directional")
        .arg("--combined_index_sequential")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("paired-end NON-DIRECTIONAL SEQUENTIAL")
                .and(predicate::str::contains(
                    "one combined index is resident at a time",
                ))
                .and(predicate::str::contains("unique best alignments:   3")),
        );

    let report =
        fs::read_to_string(outdir.path().join("reads_1_bismark_bt2_PE_report.txt")).unwrap();
    assert!(report.contains("non-directional SEQUENTIAL"));
    assert!(report.contains("byte-identical to the default parallel combined non-dir path"));
}

/// A PE fake `bowtie2` for the model-(b) tagged run: one tagged interleaved input pair
/// (`-1`/`-2`); per `-1` record strip the outer `/1/1` → `<base>__CT`/`<base>__GA`,
/// dispatch on the tag (KEEP the tag in the output qname so the driver splits it back).
/// `__CT` → OT (R1 99 / R2 147 on `_CT` @1); `__GA` → CTOT (`r_ctot`: R1 83 / R2 163 on
/// `_CT` @5) / CTOB (`r_ctob`: R1 99 / R2 147 on `_GA` @5). Everything else → a (77,141)
/// miss — so both halves are always present (as real Bowtie 2 emits).
#[cfg(unix)]
fn make_fake_bowtie2_pe_combined_nondir_tagged(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""
for a in "$@"; do [ "$prev" = "-1" ] && m1="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%4==1 {
  q=$1; sub(/^@/,"",q); sub(/\/1\/1$/,"",q);   # q = <base>__CT or <base>__GA
  base=q; sub(/__(CT|GA)$/,"",base);
  tag=substr(q, length(q)-1);
  if (tag=="CT") {
    if (base=="r_ot") {
      print q "/1\t99\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      print q "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    } else {
      print q "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print q "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI";
    }
  } else {
    if (base=="r_ctot") {
      print q "/1\t83\tchr1_CT_converted\t5\t42\t6M\t=\t5\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      print q "/2\t163\tchr1_CT_converted\t5\t42\t6M\t=\t5\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    } else if (base=="r_ctob") {
      print q "/1\t99\tchr1_GA_converted\t5\t42\t6M\t=\t5\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      print q "/2\t147\tchr1_GA_converted\t5\t42\t6M\t=\t5\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    } else {
      print q "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print q "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI";
    }
  }
}' "$m1"
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// `--combined_index --combined_index_single_pass --non_directional` PE end-to-end (model
/// b): ONE PE pass over the conversion-tagged interleaved reads (one index load). Split by
/// the qname tag → the SAME `select_pe_nondir` union as model (a) → R1 FLAGs OT 99 / CTOT
/// 147 / CTOB 163 (identical to the model-(a) e2e), plus the never-silent model-(b) banner.
#[cfg(unix)]
#[test]
fn combined_index_pe_single_pass_strands_end_to_end() {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_pe_combined_nondir_tagged(bins.path());
    let (r1, r2) = write_pe_reads_ids(genome.path(), &["r_ot", "r_ctot", "r_ctob"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--combined_index")
        .arg("--combined_index_single_pass")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("paired-end NON-DIRECTIONAL SINGLE-PASS")
                .and(predicate::str::contains("model b"))
                .and(predicate::str::contains("unique best alignments:   3")),
        );

    let report =
        fs::read_to_string(outdir.path().join("reads_1_bismark_bt2_PE_report.txt")).unwrap();
    assert!(report.contains("SINGLE-PASS (model b"));

    let bam = outdir.path().join("reads_1_bismark_bt2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let recs: Vec<_> = reader.records().map(|r| r.unwrap()).collect();
    assert_eq!(recs.len(), 6); // 3 pairs in input order: r_ot, r_ctot, r_ctob
    assert_eq!(u16::from(recs[0].inner().flags()), 99); // r_ot   → OT   (index 0)
    assert_eq!(u16::from(recs[2].inner().flags()), 147); // r_ctot → CTOT (index 2)
    assert_eq!(u16::from(recs[4].inner().flags()), 163); // r_ctob → CTOB (index 1)

    // M-1 (review A): the model-(b) `__CT`/`__GA` qname tag is INERT to output — every
    // record's read name is the ORIGINAL id (the route uses `identifier`, not the tagged
    // aligner qname), never a tagged form. Locks the documented deviation end-to-end.
    let name = |i: usize| recs[i].inner().name().map(|n| n.to_vec());
    assert_eq!(name(0), Some(b"r_ot".to_vec())); // R1
    assert_eq!(name(1), Some(b"r_ot".to_vec())); // R2 of the same pair
    assert_eq!(name(2), Some(b"r_ctot".to_vec()));
    assert_eq!(name(4), Some(b"r_ctob".to_vec()));
    for i in 0..6 {
        let n = name(i).unwrap();
        assert!(
            !n.windows(4).any(|w| w == b"__CT" || w == b"__GA"),
            "output qname must be tag-free, got: {}",
            String::from_utf8_lossy(&n)
        );
    }
}

/// Run the model-(b) PE path against a given fake and return the assertable command (for
/// the never-silent die tests). Always 1 base pair `r_ot`.
#[cfg(unix)]
fn run_pe_single_pass_with_fake(make_fake: fn(&Path)) -> assert_cmd::assert::Assert {
    let genome = TempDir::new().unwrap();
    make_genome_combined(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake(bins.path());
    let (r1, r2) = write_pe_reads_ids(genome.path(), &["r_ot"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();
    bin()
        .arg("--combined_index")
        .arg("--combined_index_single_pass")
        .arg("--non_directional")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
}

/// A tagged PE fake that emits UNTAGGED output qnames (`<base>/1`, `<base>/2` — the tag
/// dropped) → the driver's `strip_conv_tag` on the head `seq_id` fails loud (guard iii).
#[cfg(unix)]
fn make_fake_bowtie2_pe_tagged_untagged_output(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""
for a in "$@"; do [ "$prev" = "-1" ] && m1="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%4==1 {
  q=$1; sub(/^@/,"",q); sub(/\/1\/1$/,"",q); base=q; sub(/__(CT|GA)$/,"",base);
  print base "/1\t0\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
  print base "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
}' "$m1"
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// A tagged PE fake that emits ONLY the `__CT` half (skips `__GA`) → the base id is
/// missing its `__GA` half → the driver dies loud (guard ii).
#[cfg(unix)]
fn make_fake_bowtie2_pe_tagged_ct_half_only(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""
for a in "$@"; do [ "$prev" = "-1" ] && m1="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%4==1 {
  q=$1; sub(/^@/,"",q); sub(/\/1\/1$/,"",q); tag=substr(q, length(q)-1);
  if (tag=="CT") {
    print q "/1\t0\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
    print q "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
  }
}' "$m1"
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// A tagged PE fake that emits a GHOST base id (`ghost__CT`) for every pair → the head's
/// tag-stripped `seq_id` ≠ the re-read identifier → the desync guard fires loud (guard i).
#[cfg(unix)]
fn make_fake_bowtie2_pe_tagged_ghost_qname(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "fake-bowtie2 version 2.5.5"; exit 0;; esac
m1=""; prev=""
for a in "$@"; do [ "$prev" = "-1" ] && m1="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
awk 'NR%4==1 {
  print "ghost__CT/1\t0\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
  print "ghost__CT/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
}' "$m1"
"#;
    write_exec(&dir.join("bowtie2"), script);
}

/// Never-silent (model b, guard iii): an UNTAGGED output record → `strip_conv_tag` dies
/// loud rather than silently mis-partitioning the union.
#[cfg(unix)]
#[test]
fn combined_index_pe_single_pass_untagged_record_dies() {
    run_pe_single_pass_with_fake(make_fake_bowtie2_pe_tagged_untagged_output)
        .failure()
        .stderr(predicate::str::contains("__CT/__GA conversion tag"));
}

/// Never-silent (model b, guard ii): a base id missing its `__GA` half → die loud.
#[cfg(unix)]
#[test]
fn combined_index_pe_single_pass_missing_tag_half_dies() {
    run_pe_single_pass_with_fake(make_fake_bowtie2_pe_tagged_ct_half_only)
        .failure()
        .stderr(
            predicate::str::contains("missing its __CT or __GA half")
                .or(predicate::str::contains("missing its")),
        );
}

/// Never-silent (model b, guard i): a ghost head base id ≠ the re-read identifier → the
/// desync guard fires loud (the e2e of `assert_pe_tag_in_sync`).
#[cfg(unix)]
#[test]
fn combined_index_pe_single_pass_desync_dies_loud() {
    run_pe_single_pass_with_fake(make_fake_bowtie2_pe_tagged_ghost_qname)
        .failure()
        .stderr(
            predicate::str::contains("desync").and(predicate::str::contains("stream head base id")),
        );
}

// ===========================================================================
// `--combined_index --hisat2 --combined_index_sequential` (v2.x Phase 7) — the faithful
// sequential low-RAM model extended to HISAT2 (SE + PE). The sequential machinery is
// aligner-agnostic, so these assert HISAT2 sequential is BYTE-IDENTICAL to HISAT2
// parallel model (a) — INCLUDING round-tripping HISAT2's contiguous FLAG-0x100 SECONDARY
// records through the pass-1 (C→T) spill (the one HISAT2-specific record shape the Bowtie 2
// gate never exercised). Single-pass model (b) stays Bowtie 2-only (unchanged).
// ===========================================================================

/// Collect a combined-non-dir PE BAM's records for a HISAT2 run (model (a) or sequential),
/// using the given fake-maker. Asserts the sequential spill is cleaned up.
#[cfg(unix)]
fn run_combined_pe_hisat2_records(
    sequential: bool,
    upto: Option<&str>,
    make_fake: fn(&Path),
    ids: &[&str],
) -> Vec<noodles_sam::alignment::RecordBuf> {
    let genome = TempDir::new().unwrap();
    make_genome_combined_hisat2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake(bins.path());
    let (r1, r2) = write_pe_reads_ids(genome.path(), ids);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    let mut cmd = bin();
    cmd.arg("--combined_index")
        .arg("--hisat2")
        .arg("--non_directional");
    if sequential {
        cmd.arg("--combined_index_sequential");
    }
    if let Some(u) = upto {
        cmd.arg("--upto").arg(u);
    }
    cmd.arg("--genome")
        .arg(genome.path())
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success();

    if sequential {
        let leftover = fs::read_dir(temp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().ends_with(".ct_pass.sam"));
        assert!(!leftover, "sequential HISAT2 PE spill must be cleaned up");
    }

    let bam = outdir.path().join("reads_1_bismark_hisat2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    reader
        .records()
        .map(|r| r.unwrap().inner().clone())
        .collect()
}

/// SE analog of [`run_combined_pe_hisat2_records`].
#[cfg(unix)]
fn run_combined_se_hisat2_records(
    sequential: bool,
    make_fake: fn(&Path),
    ids: &[&str],
) -> Vec<noodles_sam::alignment::RecordBuf> {
    let genome = TempDir::new().unwrap();
    make_genome_combined_hisat2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", ids);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    let mut cmd = bin();
    cmd.arg("--combined_index")
        .arg("--hisat2")
        .arg("--non_directional");
    if sequential {
        cmd.arg("--combined_index_sequential");
    }
    cmd.arg("--genome")
        .arg(genome.path())
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    if sequential {
        let leftover = fs::read_dir(temp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().ends_with(".ct_pass.sam"));
        assert!(!leftover, "sequential HISAT2 SE spill must be cleaned up");
    }

    let bam = outdir.path().join("reads_bismark_hisat2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    reader
        .records()
        .map(|r| r.unwrap().inner().clone())
        .collect()
}

/// HISAT2 PE non-dir fake that, for `r_ot` on the C→T (pass-1, **spilled**) stream, emits
/// a primary OT pair FOLLOWED BY a contiguous lower-AS FLAG-0x100 SECONDARY pair (the
/// HISAT2 `-k 2` runner-up shape) — so the sequential spill/replay MUST preserve the
/// secondary or the MAPQ runner-up (and thus the BAM) diverges from model (a). The G→A
/// pass = the usual CTOT/CTOB; else a (77,141) miss. (vs `make_fake_hisat2_pe_combined_
/// nondir`, which emits no secondaries — this is the HISAT2-specific spill coverage.)
#[cfg(unix)]
fn make_fake_hisat2_pe_combined_nondir_secondary(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "hisat2-align-s version 2.2.2"; exit 0;; esac
m1=""; prev=""
for a in "$@"; do [ "$prev" = "-1" ] && m1="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
case "$m1" in
  *_C_to_T*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      if (id=="r_ot") {
        print id "/1\t99\tchr1_CT_converted\t1\t42\t6M\t=\t1\t6\tACGTAC\tFFFFFF\tAS:i:0\tZS:i:-6\tMD:Z:6";
        print id "/2\t147\tchr1_CT_converted\t1\t42\t6M\t=\t1\t-6\tACGTAC\tFFFFFF\tAS:i:0\tZS:i:-6\tMD:Z:6";
        print id "/1\t355\tchr1_GA_converted\t5\t42\t6M\t=\t5\t6\t*\tFFFFFF\tAS:i:-6\tMD:Z:6";
        print id "/2\t403\tchr1_GA_converted\t5\t42\t6M\t=\t5\t-6\t*\tFFFFFF\tAS:i:-6\tMD:Z:6";
      } else {
        print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI";
      } }' "$m1" ;;
  *_G_to_A*) awk 'NR%4==1 { id=$1; sub(/^@/,"",id); sub(/\/1\/1$/,"",id);
      if (id=="r_ctot") {
        print id "/1\t83\tchr1_CT_converted\t5\t42\t6M\t=\t5\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
        print id "/2\t163\tchr1_CT_converted\t5\t42\t6M\t=\t5\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      } else if (id=="r_ctob") {
        print id "/1\t99\tchr1_GA_converted\t5\t42\t6M\t=\t5\t6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
        print id "/2\t147\tchr1_GA_converted\t5\t42\t6M\t=\t5\t-6\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      } else {
        print id "/1\t77\t*\t0\t0\t*\t*\t0\t0\t*\tI"; print id "/2\t141\t*\t0\t0\t*\t*\t0\t0\t*\tI";
      } }' "$m1" ;;
esac
"#;
    write_exec(&dir.join("hisat2"), script);
}

/// HISAT2 **SE** non-dir fake (pass-discriminating on the `-U` `*_C_to_T*`/`*_G_to_A*`
/// suffix) — the SE analog of `make_fake_hisat2_pe_combined_nondir`. Did NOT exist before
/// Phase 7 (review B-3). `r_ot` → OT on the C→T pass; `r_ctot`/`r_ctob` → CTOT/CTOB on the
/// G→A pass; else a FLAG-4 miss.
#[cfg(unix)]
fn make_fake_hisat2_combined_nondir(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "hisat2-align-s version 2.2.2"; exit 0;; esac
inp=""; prev=""
for a in "$@"; do [ "$prev" = "-U" ] && inp="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
case "$inp" in
  *_C_to_T*)
    awk 'NR%4==1 { id=$1; sub(/^@/,"",id);
      if (id=="r_ot") print id "\t0\tchr1_CT_converted\t1\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else print id "\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF";
    }' "$inp" ;;
  *_G_to_A*)
    awk 'NR%4==1 { id=$1; sub(/^@/,"",id);
      if (id=="r_ctob")      print id "\t0\tchr1_GA_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else if (id=="r_ctot") print id "\t16\tchr1_CT_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else print id "\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF";
    }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("hisat2"), script);
}

/// HISAT2 SE non-dir fake with a contiguous FLAG-0x100 SECONDARY for `r_ot` on the C→T
/// (pass-1, **spilled**) stream — the SE secondary-spill-round-trip coverage (distinct
/// gather from PE; review A-I2).
#[cfg(unix)]
fn make_fake_hisat2_combined_nondir_secondary(dir: &Path) {
    let script = r#"#!/bin/sh
case "$*" in *--version*) echo "hisat2-align-s version 2.2.2"; exit 0;; esac
inp=""; prev=""
for a in "$@"; do [ "$prev" = "-U" ] && inp="$a"; prev="$a"; done
printf '@HD\tVN:1.0\n'
case "$inp" in
  *_C_to_T*)
    awk 'NR%4==1 { id=$1; sub(/^@/,"",id);
      if (id=="r_ot") {
        print id "\t0\tchr1_CT_converted\t1\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tZS:i:-6\tMD:Z:6";
        print id "\t256\tchr1_GA_converted\t5\t255\t6M\t*\t0\t0\t*\tFFFFFF\tAS:i:-6\tMD:Z:6";
      } else print id "\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF";
    }' "$inp" ;;
  *_G_to_A*)
    awk 'NR%4==1 { id=$1; sub(/^@/,"",id);
      if (id=="r_ctob")      print id "\t0\tchr1_GA_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else if (id=="r_ctot") print id "\t16\tchr1_CT_converted\t5\t255\t6M\t*\t0\t0\tACGTAC\tFFFFFF\tAS:i:0\tMD:Z:6";
      else print id "\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF";
    }' "$inp" ;;
esac
"#;
    write_exec(&dir.join("hisat2"), script);
}

/// THE byte-identity property for HISAT2 PE: sequential == parallel model (a) on the same
/// strand-mix (OT + CTOT + CTOB; a both-miss dropped) — the local proof the oxy md5-gate
/// scales (HISAT2 sequential feeds the same untagged converted files one pass at a time).
#[cfg(unix)]
#[test]
fn combined_index_pe_hisat2_sequential_byte_identical_to_model_a() {
    let ids = ["r_ot", "r_ctot", "r_ctob", "r_miss"];
    let model_a =
        run_combined_pe_hisat2_records(false, None, make_fake_hisat2_pe_combined_nondir, &ids);
    let sequential =
        run_combined_pe_hisat2_records(true, None, make_fake_hisat2_pe_combined_nondir, &ids);
    assert_eq!(model_a.len(), 6); // 3 mapped pairs × 2 records
    assert_eq!(
        model_a, sequential,
        "HISAT2 PE sequential must be byte-identical to model (a)"
    );
}

/// `--upto` early-break holds for HISAT2 PE sequential too.
#[cfg(unix)]
#[test]
fn combined_index_pe_hisat2_sequential_upto_matches_model_a() {
    let ids = ["r_ot", "r_ctot", "r_ctob", "r_miss"];
    let model_a =
        run_combined_pe_hisat2_records(false, Some("2"), make_fake_hisat2_pe_combined_nondir, &ids);
    let sequential =
        run_combined_pe_hisat2_records(true, Some("2"), make_fake_hisat2_pe_combined_nondir, &ids);
    assert_eq!(model_a.len(), 4); // first two pairs → 4 records
    assert_eq!(model_a, sequential);
}

/// THE HISAT2-specific coverage: a contiguous FLAG-0x100 secondary pair **on the C→T /
/// pass-1 / spilled stream** round-trips through the spill — sequential == model (a). A
/// dropped/reordered secondary would change the MAPQ runner-up → divergent BAM (review B-4).
#[cfg(unix)]
#[test]
fn combined_index_pe_hisat2_sequential_secondary_spill_round_trip() {
    let ids = ["r_ot", "r_ctot", "r_ctob"];
    let model_a = run_combined_pe_hisat2_records(
        false,
        None,
        make_fake_hisat2_pe_combined_nondir_secondary,
        &ids,
    );
    let sequential = run_combined_pe_hisat2_records(
        true,
        None,
        make_fake_hisat2_pe_combined_nondir_secondary,
        &ids,
    );
    assert_eq!(model_a.len(), 6); // OT + CTOT + CTOB (the secondary is consumed, not written)
    assert_eq!(
        model_a, sequential,
        "the C→T-pass FLAG-0x100 secondary pair must survive the spill (MAPQ runner-up)"
    );
}

/// The sequential banner + report marker say **HISAT2**, not "Bowtie 2" (the never-silent
/// SE-banner parametrization — the PE banner shares `config.aligner.name()`).
#[cfg(unix)]
#[test]
fn combined_index_pe_hisat2_sequential_banner_says_hisat2() {
    let genome = TempDir::new().unwrap();
    make_genome_combined_hisat2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_hisat2_pe_combined_nondir(bins.path());
    let (r1, r2) = write_pe_reads_ids(genome.path(), &["r_ot", "r_ctot", "r_ctob"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();
    bin()
        .arg("--combined_index")
        .arg("--hisat2")
        .arg("--non_directional")
        .arg("--combined_index_sequential")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("paired-end NON-DIRECTIONAL SEQUENTIAL")
                .and(predicate::str::contains("HISAT2"))
                .and(predicate::str::contains("Bowtie 2").not()),
        );
}

/// THE byte-identity property for HISAT2 SE: sequential == parallel model (a).
#[cfg(unix)]
#[test]
fn combined_index_se_hisat2_sequential_byte_identical_to_model_a() {
    let ids = ["r_ot", "r_ctot", "r_ctob", "r_miss"];
    let model_a = run_combined_se_hisat2_records(false, make_fake_hisat2_combined_nondir, &ids);
    let sequential = run_combined_se_hisat2_records(true, make_fake_hisat2_combined_nondir, &ids);
    assert_eq!(model_a.len(), 3); // OT + CTOT + CTOB; r_miss → no record
    assert_eq!(
        model_a, sequential,
        "HISAT2 SE sequential must be byte-identical to model (a)"
    );
}

/// SE secondary-spill round-trip (the SE gather is a distinct code path from PE).
#[cfg(unix)]
#[test]
fn combined_index_se_hisat2_sequential_secondary_spill_round_trip() {
    let ids = ["r_ot", "r_ctot", "r_ctob"];
    let model_a =
        run_combined_se_hisat2_records(false, make_fake_hisat2_combined_nondir_secondary, &ids);
    let sequential =
        run_combined_se_hisat2_records(true, make_fake_hisat2_combined_nondir_secondary, &ids);
    assert_eq!(model_a.len(), 3);
    assert_eq!(
        model_a, sequential,
        "SE C→T-pass FLAG-0x100 secondary must survive the spill"
    );
}

/// The SE sequential banner says **HISAT2** — directly locks the `lib.rs` SE-banner
/// parametrization (the literal "Bowtie 2" → `config.aligner.name()` fix).
#[cfg(unix)]
#[test]
fn combined_index_se_hisat2_sequential_banner_says_hisat2() {
    let genome = TempDir::new().unwrap();
    make_genome_combined_hisat2(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_hisat2_combined_nondir(bins.path());
    let read = write_reads_ids(genome.path(), "reads.fq", &["r_ot", "r_ctot", "r_ctob"]);
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();
    bin()
        .arg("--combined_index")
        .arg("--hisat2")
        .arg("--non_directional")
        .arg("--combined_index_sequential")
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_hisat2")
        .arg(bins.path())
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success()
        .stderr(
            predicate::str::contains("NON-DIRECTIONAL SEQUENTIAL")
                .and(predicate::str::contains("HISAT2"))
                .and(predicate::str::contains("Bowtie 2").not()),
        );
}
