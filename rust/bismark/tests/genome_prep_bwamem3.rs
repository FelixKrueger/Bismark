//! `bismark_genome_preparation --bwamem3` through the REAL bwa-mem3, then `bismark --taps` finding
//! that index with no `--five_base_index`. Runs only with `BISMARK_TEST_BWAMEM3=1`; then a
//! missing `bwa-mem3` fails the test.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::time::Duration;

use assert_cmd::Command;
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

fn bwamem3_enabled() -> bool {
    if std::env::var_os("BISMARK_TEST_BWAMEM3").is_none() {
        return false;
    }
    let ok = StdCommand::new("bwa-mem3")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    assert!(
        ok,
        "BISMARK_TEST_BWAMEM3 is set but `bwa-mem3 version` does not run"
    );
    true
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/aligner/taps")
        .join(name);
    assert!(p.is_file(), "missing TAPS fixture {}", p.display());
    p
}

/// `spikes.fa` split into one file per contig, so genome prep concatenates them.
fn split_genome(dir: &Path) {
    let text = fs::read_to_string(fixture("spikes.fa")).unwrap();
    let starts: Vec<usize> = text.match_indices('>').map(|(i, _)| i).collect();
    assert!(starts.len() >= 2, "spikes.fa has fewer than two contigs");
    for (n, w) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(text.len());
        fs::write(dir.join(format!("part{n}.fa")), &text[*w..end]).unwrap();
    }
}

fn taps(genome: &Path, index: Option<&Path>, out: &Path) -> String {
    let mut c = Command::cargo_bin("bismark").unwrap();
    c.arg("--taps").args(["-p", "4"]);
    if let Some(i) = index {
        c.arg("--five_base_index").arg(i);
    }
    let o = c
        .arg("--genome")
        .arg(genome)
        .arg("--output_dir")
        .arg(out)
        .arg("-1")
        .arg(fixture("taps_R1.fq.gz"))
        .arg("-2")
        .arg(fixture("taps_R2.fq.gz"))
        .timeout(Duration::from_secs(300))
        .assert()
        .success();
    String::from_utf8_lossy(&o.get_output().stderr).into_owned()
}

fn records(bam: &Path) -> Vec<String> {
    bismark::io::BamReader::from_path(bam)
        .unwrap()
        .records()
        .map(|r| format!("{:?}", r.unwrap().inner()))
        .collect()
}

#[test]
fn genome_prep_index_matches_a_manual_build_and_is_found_by_taps() {
    if !bwamem3_enabled() {
        eprintln!("skipping: BISMARK_TEST_BWAMEM3 unset (genome prep bwa-mem3 gate)");
        return;
    }
    let genome = TempDir::new().unwrap();
    split_genome(genome.path());
    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .args(["--bwamem3", "--parallel", "2"])
        .arg(genome.path())
        .timeout(Duration::from_secs(300))
        .assert()
        .success();

    let manual = TempDir::new().unwrap();
    let fa = manual.path().join("spikes.fa");
    fs::copy(fixture("spikes.fa"), &fa).unwrap();
    let st = StdCommand::new("bwa-mem3")
        .args(["index", "--meth", "-t", "2"])
        .arg(&fa)
        .output()
        .unwrap();
    assert!(
        st.status.success(),
        "bwa-mem3 index --meth failed: {}",
        String::from_utf8_lossy(&st.stderr)
    );

    let built = genome.path().join("Bwamem3_Index");
    let mut names: Vec<String> = fs::read_dir(&built)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut want: Vec<String> = SEVEN.iter().map(|s| format!("genome.fa.{s}")).collect();
    want.sort();
    assert_eq!(names, want);
    for s in SEVEN {
        assert_eq!(
            fs::read(built.join(format!("genome.fa.{s}"))).unwrap(),
            fs::read(format!("{}.{s}", fa.display())).unwrap(),
            "genome.fa.{s} differs from the manual build"
        );
    }

    let auto = TempDir::new().unwrap();
    let stderr = taps(genome.path(), None, auto.path());
    assert!(stderr.contains("Using the bwa-mem3 index"), "{stderr}");
    let explicit = TempDir::new().unwrap();
    taps(genome.path(), Some(&fa), explicit.path());
    let bam = |d: &Path| d.join("taps_R1_bismark_bwamem3_pe.bam");
    let a = records(&bam(auto.path()));
    assert!(!a.is_empty());
    assert_eq!(a, records(&bam(explicit.path())));
}

/// The common layout: one gzipped FASTA, linked rather than copied.
#[test]
fn single_gz_genome_matches_a_manual_build() {
    if !bwamem3_enabled() {
        eprintln!("skipping: BISMARK_TEST_BWAMEM3 unset (genome prep bwa-mem3 gate)");
        return;
    }
    let genome = TempDir::new().unwrap();
    let gz = genome.path().join("spikes.fa.gz");
    let mut e = flate2::write::GzEncoder::new(
        fs::File::create(&gz).unwrap(),
        flate2::Compression::default(),
    );
    std::io::Write::write_all(&mut e, &fs::read(fixture("spikes.fa")).unwrap()).unwrap();
    e.finish().unwrap();
    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .arg("--bwamem3")
        .arg(genome.path())
        .timeout(Duration::from_secs(300))
        .assert()
        .success();

    let manual = TempDir::new().unwrap();
    let fa = manual.path().join("spikes.fa");
    fs::copy(fixture("spikes.fa"), &fa).unwrap();
    let st = StdCommand::new("bwa-mem3")
        .args(["index", "--meth"])
        .arg(&fa)
        .output()
        .unwrap();
    assert!(
        st.status.success(),
        "{}",
        String::from_utf8_lossy(&st.stderr)
    );
    for s in SEVEN {
        assert_eq!(
            fs::read(genome.path().join(format!("Bwamem3_Index/genome.fa.{s}"))).unwrap(),
            fs::read(format!("{}.{s}", fa.display())).unwrap(),
            "genome.fa.{s} differs from the manual build"
        );
    }
}

/// Sidecars next to a user's `genome.fa` never reach the index or the alignments.
#[test]
fn sidecars_beside_the_genome_are_ignored() {
    if !bwamem3_enabled() {
        eprintln!("skipping: BISMARK_TEST_BWAMEM3 unset (genome prep bwa-mem3 gate)");
        return;
    }
    const MARK: &str = "BISMARK_SIDECAR_MARKER";
    let genome = TempDir::new().unwrap();
    let g = genome.path();
    fs::copy(fixture("spikes.fa"), g.join("genome.fa")).unwrap();
    fs::write(
        g.join("genome.fa.alt"),
        format!("{MARK}\t0\t*\t0\t0\t*\t*\t0\t0\t*\t*\n"),
    )
    .unwrap();
    fs::write(g.join("genome.fa.hdr"), format!("@CO\t{MARK}\n")).unwrap();
    fs::write(g.join("genome.dict"), format!("@HD\tVN:1.6\n@CO\t{MARK}\n")).unwrap();
    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .arg("--bwamem3")
        .arg(g)
        .timeout(Duration::from_secs(300))
        .assert()
        .success();
    let mut names: Vec<String> = fs::read_dir(g.join("Bwamem3_Index"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut want: Vec<String> = SEVEN.iter().map(|s| format!("genome.fa.{s}")).collect();
    want.sort();
    assert_eq!(names, want);

    let out = TempDir::new().unwrap();
    taps(g, None, out.path());
    let mut bam = Vec::new();
    std::io::Read::read_to_end(
        &mut flate2::read::MultiGzDecoder::new(
            fs::File::open(out.path().join("taps_R1_bismark_bwamem3_pe.bam")).unwrap(),
        ),
        &mut bam,
    )
    .unwrap();
    assert!(
        !String::from_utf8_lossy(&bam).contains(MARK),
        "sidecar content reached the BAM"
    );
}

/// bwa-mem3's memory pre-check still exits 3, which Bismark reports as a memory error.
#[test]
fn too_small_a_budget_is_a_memory_error() {
    if !bwamem3_enabled() {
        eprintln!("skipping: BISMARK_TEST_BWAMEM3 unset (genome prep bwa-mem3 gate)");
        return;
    }
    let genome = TempDir::new().unwrap();
    fs::copy(fixture("spikes.fa"), genome.path().join("spikes.fa")).unwrap();
    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .args(["--bwamem3", "--bwamem3_max_memory", "1K"])
        .arg(genome.path())
        .timeout(Duration::from_secs(120))
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "the --bwamem3_max_memory you gave (1K)",
        ));
    let names: Vec<String> = fs::read_dir(genome.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["spikes.fa"]);
}

/// A CRLF genome builds, matches a manual build of the same file, and `--taps` aligns against it.
#[test]
fn crlf_genome_builds_and_aligns() {
    if !bwamem3_enabled() {
        eprintln!("skipping: BISMARK_TEST_BWAMEM3 unset (genome prep bwa-mem3 gate)");
        return;
    }
    let crlf = fs::read_to_string(fixture("spikes.fa"))
        .unwrap()
        .replace('\n', "\r\n");
    let genome = TempDir::new().unwrap();
    fs::write(genome.path().join("spikes.fa"), &crlf).unwrap();
    Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .arg("--bwamem3")
        .arg(genome.path())
        .timeout(Duration::from_secs(300))
        .assert()
        .success();

    let manual = TempDir::new().unwrap();
    let fa = manual.path().join("spikes.fa");
    fs::write(&fa, &crlf).unwrap();
    let st = StdCommand::new("bwa-mem3")
        .args(["index", "--meth"])
        .arg(&fa)
        .output()
        .unwrap();
    assert!(
        st.status.success(),
        "{}",
        String::from_utf8_lossy(&st.stderr)
    );
    for s in SEVEN {
        assert_eq!(
            fs::read(genome.path().join(format!("Bwamem3_Index/genome.fa.{s}"))).unwrap(),
            fs::read(format!("{}.{s}", fa.display())).unwrap(),
            "genome.fa.{s} differs from the manual build"
        );
    }

    let out = TempDir::new().unwrap();
    taps(genome.path(), None, out.path());
    assert!(!records(&out.path().join("taps_R1_bismark_bwamem3_pe.bam")).is_empty());
}
