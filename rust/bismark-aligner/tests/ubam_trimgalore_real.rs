//! #1025 Phase 1 — real-data regression guard for the uBAM transcoder.
//!
//! Uses genuine **Trim Galore 2.2.0 `--output-format ubam`** output (subsampled to 200
//! single-end reads / 200 collated paired-end pairs from the 10K `test_files/` BAMs) and
//! asserts the in-process transcode (`ubam::transcode_ubam_to_fastq_se`/`_pe`) is
//! **byte-identical to the committed `samtools fastq` golden**.
//!
//! Why this complements the existing tests. `ubam_transcode.rs` validates the transcoder
//! against *synthetic* noodles records, and the oxy gate validated it end-to-end but only at
//! the *alignment* level (Bismark trims read IDs at whitespace, so a FASTQ-header difference
//! could pass there). This test instead checks the **FASTQ bytes directly** on **real** Trim
//! Galore records — a tighter, hermetic guard (no samtools / genome / bowtie2 at test time;
//! the golden is committed). The fixtures + goldens were generated once with samtools 1.21 via
//! `samtools fastq tg_se.bam` (SE) and `samtools fastq -1 … -2 … -0 /dev/null -s /dev/null tg_pe.bam` (PE).

use std::path::{Path, PathBuf};

use bismark_aligner::ubam;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn se_trimgalore_ubam_transcodes_byte_identical_to_samtools_fastq() {
    let dir = tempfile::tempdir().unwrap();
    let out =
        ubam::transcode_ubam_to_fastq_se(&fixture("tg_se_trimgalore.bam"), dir.path()).unwrap();
    assert_eq!(
        read(&out),
        read(&fixture("tg_se_trimgalore.expected.fastq")),
        "SE Trim Galore uBAM transcode must be byte-identical to the `samtools fastq` golden"
    );
}

#[test]
fn pe_trimgalore_ubam_splits_byte_identical_to_samtools_fastq() {
    let dir = tempfile::tempdir().unwrap();
    let (r1, r2) =
        ubam::transcode_ubam_to_fastq_pe(&fixture("tg_pe_trimgalore.bam"), dir.path()).unwrap();
    assert_eq!(
        read(&r1),
        read(&fixture("tg_pe_trimgalore.expected_1.fastq")),
        "PE Trim Galore uBAM R1 must be byte-identical to the `samtools fastq -1` golden"
    );
    assert_eq!(
        read(&r2),
        read(&fixture("tg_pe_trimgalore.expected_2.fastq")),
        "PE Trim Galore uBAM R2 must be byte-identical to the `samtools fastq -2` golden"
    );
}
