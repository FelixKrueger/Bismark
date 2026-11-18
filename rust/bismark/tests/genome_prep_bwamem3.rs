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
