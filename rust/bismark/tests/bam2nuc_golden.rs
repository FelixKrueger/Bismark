//! Byte-identity goldens vs Perl `bam2nuc` v0.25.1, plus behavioral/exit-code
//! cells. The goldens + BAM/genome fixtures are produced by
//! `tests/data/bam2nuc/generate_goldens.sh` (which runs the real Perl + samtools); these
//! tests run the Rust `bam2nuc_rs` over the SAME fixtures and assert byte-for-byte
//! identity — hermetically (no Perl/samtools needed here).

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/bam2nuc")
}

fn golden(name: &str) -> Vec<u8> {
    std::fs::read(data_dir().join("goldens").join(name)).unwrap_or_else(|e| {
        panic!("missing golden {name}: {e} (run tests/data/bam2nuc/generate_goldens.sh)")
    })
}

/// Copy a committed genome fixture into a fresh writable tempdir (so the run's
/// cache write doesn't pollute the committed fixture). Returns the tempdir; the
/// genome lives at `<tmp>/genome`.
fn copy_genome(fixture: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let dst = tmp.path().join("genome");
    std::fs::create_dir_all(&dst).unwrap();
    for entry in std::fs::read_dir(data_dir().join(fixture)).unwrap() {
        let e = entry.unwrap();
        if e.file_type().unwrap().is_file() {
            std::fs::copy(e.path(), dst.join(e.file_name())).unwrap();
        }
    }
    tmp
}

fn assert_bytes_eq(actual: &[u8], expected: &[u8], label: &str) {
    assert!(
        actual == expected,
        "byte mismatch for {label}\n--- actual ({} bytes) ---\n{}\n--- expected ({} bytes) ---\n{}",
        actual.len(),
        String::from_utf8_lossy(actual),
        expected.len(),
        String::from_utf8_lossy(expected),
    );
}

fn bin() -> Command {
    Command::cargo_bin("bam2nuc").unwrap()
}

/// Run `--genomic_composition_only` against a genome fixture; return the cache.
fn run_genomic_composition_only(fixture: &str) -> Vec<u8> {
    let tmp = copy_genome(fixture);
    let genome = tmp.path().join("genome");
    bin()
        .arg("-g")
        .arg(&genome)
        .arg("--genomic_composition_only")
        .assert()
        .success();
    std::fs::read(genome.join("genomic_nucleotide_frequencies.txt")).unwrap()
}

/// Run stats for one BAM against a genome fixture; return (tempdir, stats bytes).
fn run_stats(fixture: &str, bam: &str) -> (TempDir, Vec<u8>) {
    let tmp = copy_genome(fixture);
    let genome = tmp.path().join("genome");
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let dir_arg = format!("{}/", out.display());
    bin()
        .arg("-g")
        .arg(&genome)
        .arg("--dir")
        .arg(&dir_arg)
        .arg(data_dir().join(bam))
        .assert()
        .success();
    let stem = Path::new(bam).file_stem().unwrap().to_str().unwrap();
    let stats = std::fs::read(out.join(format!("{stem}.nucleotide_stats.txt"))).unwrap();
    (tmp, stats)
}

// ── Cache goldens (--genomic_composition_only) ──

#[test]
fn cache_acgtn_byte_identical() {
    assert_bytes_eq(
        &run_genomic_composition_only("genome_acgtn"),
        &golden("cache_acgtn.golden"),
        "cache_acgtn",
    );
}

#[test]
fn cache_iupac_byte_identical() {
    // The sole guard for the count-everything-not-just-ACGT rule (R/CR/RG rows).
    assert_bytes_eq(
        &run_genomic_composition_only("genome_iupac"),
        &golden("cache_iupac.golden"),
        "cache_iupac",
    );
}

#[test]
fn cache_empty_genome_is_zero_bytes() {
    let cache = run_genomic_composition_only("genome_mus");
    assert!(
        cache.is_empty(),
        "Mus-only genome must yield a 0-byte cache"
    );
    assert_bytes_eq(&cache, &golden("cache_mus.golden"), "cache_mus");
}

// ── Stats goldens ──

#[test]
fn se_stats_and_cache_byte_identical() {
    let (tmp, stats) = run_stats("genome_acgtn", "se.bam");
    assert_bytes_eq(&stats, &golden("se_stats.golden"), "se_stats");
    // The cache computed during the SE run also matches.
    let cache =
        std::fs::read(tmp.path().join("genome/genomic_nucleotide_frequencies.txt")).unwrap();
    assert_bytes_eq(&cache, &golden("se_cache.golden"), "se_cache");
}

#[test]
fn pe_stats_byte_identical() {
    let (_t, stats) = run_stats("genome_acgtn", "pe.bam");
    assert_bytes_eq(&stats, &golden("pe_stats.golden"), "pe_stats");
}

#[test]
fn pe_noncanonical_flag_byte_identical() {
    // Proves the `elsif ($flag == 83 or 163)` always-true bug: flag 65 is
    // reverse-complemented (not treated as forward), matching Perl.
    let (_t, stats) = run_stats("genome_acgtn", "pe_noncanonical.bam");
    assert_bytes_eq(
        &stats,
        &golden("pe_noncanonical_stats.golden"),
        "pe_noncanonical",
    );
}

#[test]
fn cache_reuse_uses_planted_genomic_column() {
    // genome_reuse ships a planted ×1000 cache; a recompute-instead-of-reuse bug
    // would put the genome's real composition in the `count genomic` column and
    // fail this byte compare.
    let (_t, stats) = run_stats("genome_reuse", "se.bam");
    assert_bytes_eq(&stats, &golden("reuse_stats.golden"), "reuse_stats");
}

#[test]
fn two_input_files_each_get_stats_cache_reused() {
    let tmp = copy_genome("genome_acgtn");
    let genome = tmp.path().join("genome");
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let dir_arg = format!("{}/", out.display());
    bin()
        .arg("-g")
        .arg(&genome)
        .arg("--dir")
        .arg(&dir_arg)
        .arg(data_dir().join("se.bam"))
        .arg(data_dir().join("pe.bam"))
        .assert()
        .success();
    let se = std::fs::read(out.join("se.nucleotide_stats.txt")).unwrap();
    let pe = std::fs::read(out.join("pe.nucleotide_stats.txt")).unwrap();
    // Each file's output matches its solo golden (cache computed once for se.bam,
    // reused for pe.bam).
    assert_bytes_eq(&se, &golden("se_stats.golden"), "two-file se");
    assert_bytes_eq(&pe, &golden("pe_stats.golden"), "two-file pe");
    assert_ne!(
        se, pe,
        "the two stats files must differ (per-file %freqs reset)"
    );
}

// ── Behavioral / exit-code cells ──

#[test]
fn all_indel_sample_zerodivision_exits_one() {
    // Every read has an InDel → all skipped → sample mono total 0 → ZeroDivision
    // (Perl dies 255; Rust exits 1). The header IS written first (partial file).
    let tmp = copy_genome("genome_acgtn");
    let genome = tmp.path().join("genome");
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    let dir_arg = format!("{}/", out.display());
    bin()
        .arg("-g")
        .arg(&genome)
        .arg("--dir")
        .arg(&dir_arg)
        .arg(data_dir().join("all_indel.bam"))
        .assert()
        .failure()
        .code(1);
    // Partial stats file contains only the header (Perl parity: dies mid-routine).
    let partial = std::fs::read_to_string(out.join("all_indel.nucleotide_stats.txt")).unwrap();
    assert_eq!(
        partial,
        "(di-)nucleotide\tcount sample\tpercent sample\tcount genomic\tpercent genomic\tcoverage\n"
    );
}

#[test]
fn sam_input_is_rejected() {
    let tmp = copy_genome("genome_acgtn");
    let genome = tmp.path().join("genome");
    // A text SAM file: from_path sniffs the leading '@' → Sam → SamNotSupported.
    let sam = tmp.path().join("reads.sam");
    std::fs::write(
        &sam,
        b"@HD\tVN:1.6\n@SQ\tSN:chr1\tLN:17\nr\t0\tchr1\t1\t40\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII\n",
    )
    .unwrap();
    bin()
        .arg("-g")
        .arg(&genome)
        .arg(&sam)
        .assert()
        .failure()
        .stderr(predicates::str::contains("SAM"));
}

#[test]
fn cram_input_is_rejected() {
    let tmp = copy_genome("genome_acgtn");
    let genome = tmp.path().join("genome");
    // CRAM magic bytes are enough for the format sniff to classify Cram.
    let cram = tmp.path().join("reads.cram");
    std::fs::write(&cram, b"CRAM\x03\x00\x00\x00").unwrap();
    // Strengthened (L-2): assert exit code 1 AND the specific CramNotSupported
    // message ("not yet supported"), not merely any mention of CRAM.
    bin()
        .arg("-g")
        .arg(&genome)
        .arg(&cram)
        .assert()
        .failure()
        .code(1)
        .stderr(predicates::str::contains("not yet supported"));
}

#[test]
fn content_bam_named_sam_is_rejected_at_output_naming() {
    // T-1 / code-review O-6: a real BAM whose filename ends in `.sam`. The
    // content sniff (from_path) classifies it as BAM, so the format gate passes
    // and counting succeeds — but `derive_output_name("x.sam")` then fails
    // (case-sensitive trailing-`bam`/`cram` strip) → NotBamOrCram, exit 1.
    // Perl likewise errors (it reads the BAM bytes as text garbage, then dies at
    // the same name derivation); both produce no stats file.
    let tmp = copy_genome("genome_acgtn");
    let genome = tmp.path().join("genome");
    let mislabelled = tmp.path().join("x.sam");
    std::fs::copy(data_dir().join("se.bam"), &mislabelled).unwrap();
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    bin()
        .arg("-g")
        .arg(&genome)
        .arg("--dir")
        .arg(format!("{}/", out.display()))
        .arg(&mislabelled)
        .assert()
        .failure()
        .code(1)
        .stderr(predicates::str::contains("BAM or CRAM"));
    // No stats file was written.
    assert!(!out.join("x.nucleotide_stats.txt").exists());
}

#[test]
fn missing_genome_folder_errors() {
    bin()
        .arg(data_dir().join("se.bam"))
        .assert()
        .failure()
        .stderr(predicates::str::contains("genome folder"));
}

// ── Test-gap closure cells (handoff §2: robustness; none a byte-identity risk) ──

#[test]
fn version_flag_long_prints_version_and_exits_zero() {
    // e2e: `--version` short-circuits in main.rs (before run()), prints
    // version_string() to stdout, exits 0. Only version_string() itself was
    // unit-tested before; this covers the clap wiring + the main-fn branch.
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("bam2nuc (Bismark Rust suite) "))
        .stdout(predicates::str::contains(std::env::consts::OS));
}

#[test]
fn version_flag_short_prints_version_and_exits_zero() {
    bin()
        .arg("-V")
        .assert()
        .success()
        .stdout(predicates::str::contains("bam2nuc (Bismark Rust suite) "));
}

#[test]
fn non_bismark_pg_bam_is_se_pe_undetermined() {
    // A BAM whose @PG is bowtie2 (no ID:Bismark) → detect_paired_from_header == None
    // → SePeUndetermined. The error is raised inside count_reads_in_file, BEFORE the
    // output stats file is created, so no partial stats file is left behind (contrast
    // all_indel, where counting succeeds and a header-only partial IS written).
    let tmp = copy_genome("genome_acgtn");
    let genome = tmp.path().join("genome");
    let out = tmp.path().join("out");
    std::fs::create_dir_all(&out).unwrap();
    bin()
        .arg("-g")
        .arg(&genome)
        .arg("--dir")
        .arg(format!("{}/", out.display()))
        .arg(data_dir().join("no_bismark_pg.bam"))
        .assert()
        .failure()
        .code(1)
        .stderr(predicates::str::contains("single-end vs paired-end"));
    assert!(!out.join("no_bismark_pg.nucleotide_stats.txt").exists());
}

#[test]
fn se_sorted_stats_byte_identical() {
    // Coordinate-sorted SE BAM: samtools appends its @PG AFTER Bismark's, so SE/PE
    // detection still sees ID:Bismark (SE). bam2nuc tallies are order-independent, so
    // the stats match BOTH the Perl oracle's sorted golden AND the unsorted SE golden
    // byte-for-byte (the second assert proves the cell isn't comparing a file to itself).
    let (_t, stats) = run_stats("genome_acgtn", "se_sorted.bam");
    assert_bytes_eq(&stats, &golden("se_sorted_stats.golden"), "se_sorted");
    assert_eq!(
        stats,
        golden("se_stats.golden"),
        "sorted == unsorted SE stats"
    );
}
