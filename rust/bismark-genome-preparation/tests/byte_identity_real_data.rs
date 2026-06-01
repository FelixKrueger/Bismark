//! Real-data byte-identity gate (Phase E, `#[ignore]`).
//!
//! Runs Perl `bismark_genome_preparation` v0.25.1 and
//! `bismark_genome_preparation_rs` on COPIES of the SAME real genome directory
//! and asserts the bisulfite-converted **CT + GA FASTA** are byte-identical
//! (the acceptance gate, SPEC §7). A **fake `bowtie2-build`** (exit 0) is used
//! for both so Step III completes instantly — the gate is on the (fast)
//! conversion, NOT the (slow, possibly hours) real index build, which is
//! validated separately (SPEC §7 secondary / E2 procedure).
//!
//! Skipped unless the genome dir is provided (so `cargo test --ignored` is safe
//! on a machine without the data / without Perl Bismark):
//!
//! ```sh
//! BISMARK_GENOMEPREP_REAL_GENOME_DIR=/path/to/genome_with_fasta \
//!   PERL_GP=~/miniforge3/envs/bioinf/bin/bismark_genome_preparation \
//!   cargo test -p bismark-genome-preparation --release -- --ignored byte_identity_real_data
//! ```
//!
//! On oxy: use a FRESH work dir; the test copies the genome into a `TempDir`
//! and never writes to the source. Point it at a SMALL genome (or a single
//! chromosome) — the conversion is linear in genome size and both tools read
//! it fully.

use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

const RUST_BIN: &str = env!("CARGO_BIN_EXE_bismark_genome_preparation_rs");

/// Create a fake `bowtie2-build` that exits 0, so Step III completes without a
/// real indexer (the gate is the converted FASTA, not the index). Returns the
/// bin dir to put on PATH (Perl) / in `BISMARK_BIN` (Rust).
fn fake_indexer_dir(parent: &Path) -> PathBuf {
    let bin = parent.join("fakebin");
    fs::create_dir_all(&bin).unwrap();
    for tool in ["bowtie2-build", "hisat2-build", "minimap2"] {
        let p = bin.join(tool);
        fs::write(&p, b"#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    bin
}

/// Stream-compare two (possibly multi-GB) files byte-for-byte without loading
/// them fully into memory. Returns `(equal, perl_len, rust_len)`.
fn files_equal(a: &Path, b: &Path) -> (bool, u64, u64) {
    let fa = fs::File::open(a).unwrap_or_else(|e| panic!("open {}: {e}", a.display()));
    let fb = fs::File::open(b).unwrap_or_else(|e| panic!("open {}: {e}", b.display()));
    let la = fa.metadata().unwrap().len();
    let lb = fb.metadata().unwrap().len();
    if la != lb {
        return (false, la, lb);
    }
    let mut ra = BufReader::with_capacity(1 << 20, fa);
    let mut rb = BufReader::with_capacity(1 << 20, fb);
    let mut buf_a = [0u8; 1 << 16];
    let mut buf_b = [0u8; 1 << 16];
    loop {
        let na = ra.read(&mut buf_a).unwrap();
        let nb = rb.read(&mut buf_b).unwrap();
        if na != nb {
            return (false, la, lb); // shouldn't happen (equal lengths) but be safe
        }
        if na == 0 {
            return (true, la, lb);
        }
        if buf_a[..na] != buf_b[..nb] {
            return (false, la, lb);
        }
    }
}

fn copy_genome(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for e in fs::read_dir(src).unwrap().filter_map(|e| e.ok()) {
        let p = e.path();
        // Only copy top-level FASTA inputs (skip any pre-existing Bisulfite_Genome/).
        if p.is_file() {
            fs::copy(&p, dst.join(e.file_name())).unwrap();
        }
    }
}

fn gate(label: &str, extra_args: &[&str], rel_outputs: &[&str]) {
    let Ok(genome) = std::env::var("BISMARK_GENOMEPREP_REAL_GENOME_DIR") else {
        eprintln!("skipping {label}: set BISMARK_GENOMEPREP_REAL_GENOME_DIR to run");
        return;
    };
    let genome = PathBuf::from(genome);
    let perl =
        std::env::var("PERL_GP").unwrap_or_else(|_| "bismark_genome_preparation".to_string());

    let tmp = TempDir::new().unwrap();
    let bin = fake_indexer_dir(tmp.path());
    let p_dir = tmp.path().join("perl_genome");
    let r_dir = tmp.path().join("rust_genome");
    copy_genome(&genome, &p_dir);
    copy_genome(&genome, &r_dir);

    // Perl: fake bowtie2-build via PATH.
    let path_env = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let p_status = Command::new(&perl)
        .args(extra_args)
        .arg(&p_dir)
        .env("PATH", &path_env)
        .env("LC_ALL", "C")
        .status()
        .unwrap_or_else(|e| panic!("spawn perl {perl}: {e}"));
    assert!(p_status.success(), "{label}: perl exited with {p_status}");

    // Rust: fake indexer via BISMARK_BIN.
    let r_status = Command::new(RUST_BIN)
        .args(extra_args)
        .arg(&r_dir)
        .env("BISMARK_BIN", &bin)
        .env("LC_ALL", "C")
        .status()
        .unwrap_or_else(|e| panic!("spawn rust bin: {e}"));
    assert!(r_status.success(), "{label}: rust exited with {r_status}");

    for rel in rel_outputs {
        let (eq, pl, rl) = files_equal(&p_dir.join(rel), &r_dir.join(rel));
        assert!(
            eq,
            "{label}: {rel} differs (perl {pl} bytes vs rust {rl} bytes)"
        );
    }
    eprintln!("{label}: converted FASTA byte-identical ✓");
}

#[test]
#[ignore = "requires BISMARK_GENOMEPREP_REAL_GENOME_DIR + Perl bismark_genome_preparation"]
fn byte_identity_real_data_mfa() {
    gate(
        "real-data MFA (default)",
        &[],
        &[
            "Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa",
            "Bisulfite_Genome/GA_conversion/genome_mfa.GA_conversion.fa",
        ],
    );
}

#[test]
#[ignore = "requires BISMARK_GENOMEPREP_REAL_GENOME_DIR + Perl bismark_genome_preparation"]
fn byte_identity_real_data_genomic_composition() {
    // The mono-/di-nucleotide frequency table lands in the genome folder root.
    gate(
        "real-data genomic_composition",
        &["--genomic_composition"],
        &["genomic_nucleotide_frequencies.txt"],
    );
}

#[test]
#[ignore = "requires BISMARK_GENOMEPREP_REAL_GENOME_DIR + Perl bismark_genome_preparation"]
fn byte_identity_real_data_single_fasta() {
    // Per-chromosome file set is compared by re-globbing the Perl output dir and
    // diffing each against the Rust counterpart.
    let Ok(genome) = std::env::var("BISMARK_GENOMEPREP_REAL_GENOME_DIR") else {
        eprintln!(
            "skipping real-data --single_fasta: set BISMARK_GENOMEPREP_REAL_GENOME_DIR to run"
        );
        return;
    };
    let genome = PathBuf::from(genome);
    let perl =
        std::env::var("PERL_GP").unwrap_or_else(|_| "bismark_genome_preparation".to_string());

    let tmp = TempDir::new().unwrap();
    let bin = fake_indexer_dir(tmp.path());
    let p_dir = tmp.path().join("perl_genome");
    let r_dir = tmp.path().join("rust_genome");
    copy_genome(&genome, &p_dir);
    copy_genome(&genome, &r_dir);

    let path_env = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    assert!(
        Command::new(&perl)
            .arg("--single_fasta")
            .arg(&p_dir)
            .env("PATH", &path_env)
            .env("LC_ALL", "C")
            .status()
            .unwrap()
            .success(),
        "perl --single_fasta failed"
    );
    assert!(
        Command::new(RUST_BIN)
            .arg("--single_fasta")
            .arg(&r_dir)
            .env("BISMARK_BIN", &bin)
            .env("LC_ALL", "C")
            .status()
            .unwrap()
            .success(),
        "rust --single_fasta failed"
    );

    for conv in ["CT_conversion", "GA_conversion"] {
        let p_conv = p_dir.join("Bisulfite_Genome").join(conv);
        let mut perl_files: Vec<String> = fs::read_dir(&p_conv)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.ends_with("_conversion.fa"))
            .collect();
        perl_files.sort();
        assert!(!perl_files.is_empty(), "no per-chr {conv} files from perl");
        let r_conv = r_dir.join("Bisulfite_Genome").join(conv);
        for f in &perl_files {
            let (eq, pl, rl) = files_equal(&p_conv.join(f), &r_conv.join(f));
            assert!(
                eq,
                "single_fasta {conv}/{f} differs (perl {pl} vs rust {rl})"
            );
        }
    }
    eprintln!("real-data --single_fasta: all per-chromosome converted FASTA byte-identical ✓");
}
