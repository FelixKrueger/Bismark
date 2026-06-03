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
