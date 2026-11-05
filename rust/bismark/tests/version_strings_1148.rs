//! #1148: every version string in an output names the suite version, never `0.25.1`.

mod common;
#[path = "common/version.rs"]
mod version;

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use bismark::meta::SUITE_VERSION;
use tempfile::TempDir;
use version::{Form, perl_version_for};

fn assert_no_perl_version(what: &str, text: &str) {
    assert!(
        !text.contains("0.25.1"),
        "{what} still carries 0.25.1:\n{text}"
    );
}

fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

// ── helper self-test ──

#[test]
fn helper_rewrites_each_form_to_perl() {
    let rust = "Bismark methylation extractor version v3.1.0\n\
                Bismark Extractor Version: v3.1.0\n\
                (version 3.1.0) - a tool";
    let perl = "Bismark methylation extractor version v0.25.1\n\
                Bismark Extractor Version: v0.25.1\n\
                (version 0.25.1) - a tool";
    let all = [
        (Form::SplitFileHeader, 1),
        (Form::SplittingReport, 1),
        (Form::SummaryHtml, 1),
    ];
    assert_eq!(perl_version_for(rust, "3.1.0", &all), perl);
}

#[test]
fn helper_leaves_lookalikes_alone() {
    let s = "tool v3.1.0 (version 3.1.0-beta)\n";
    assert_eq!(perl_version_for(s, "3.1.0", &[(Form::SummaryHtml, 0)]), s);
}

#[test]
#[should_panic(expected = "expected 1")]
fn helper_panics_on_a_missed_emitter() {
    perl_version_for(
        "Bismark Extractor Version: v0.25.1\n",
        "3.1.0",
        &[(Form::SplittingReport, 1)],
    );
}

#[test]
fn helper_is_a_no_op_at_the_perl_version() {
    let s = "Bismark Extractor Version: v0.25.1\n";
    assert_eq!(
        perl_version_for(s, "0.25.1", &[(Form::SplittingReport, 1)]),
        s
    );
}

// ── outputs ──

#[test]
fn version_is_never_unknown() {
    assert_ne!(SUITE_VERSION, "unknown");
}

#[cfg(unix)]
fn write_exec(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A fake `bowtie2`: one mapped OT hit on the CT index, unmapped on GA.
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

fn make_genome(dir: &Path) {
    for (sub, pre) in [("CT_conversion", "BS_CT"), ("GA_conversion", "BS_GA")] {
        let d = dir.join("Bisulfite_Genome").join(sub);
        fs::create_dir_all(&d).unwrap();
        for s in ["1", "2", "3", "4", "rev.1", "rev.2"] {
            fs::write(d.join(format!("{pre}.{s}.bt2")), b"x").unwrap();
        }
    }
    fs::write(dir.join("genome.fa"), b">chr1\nACGTACGT\n").unwrap();
}

#[cfg(unix)]
#[test]
fn aligner_report_and_pg_carry_the_suite_version() {
    let genome = TempDir::new().unwrap();
    make_genome(genome.path());
    let bins = TempDir::new().unwrap();
    make_fake_bowtie2_mapped(bins.path());
    let read = genome.path().join("reads.fq");
    fs::write(&read, b"@r1\nACGTAC\n+\nFFFFFF\n").unwrap();
    let temp = TempDir::new().unwrap();
    let out = TempDir::new().unwrap();

    Command::cargo_bin("bismark")
        .unwrap()
        .arg("--genome")
        .arg(genome.path())
        .arg("--path_to_bowtie2")
        .arg(bins.path())
        .arg("--no_stream_converted")
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(out.path())
        .arg(&read)
        .assert()
        .success();

    let report = fs::read_to_string(out.path().join("reads_bismark_bt2_SE_report.txt")).unwrap();
    let line1 = report.lines().next().unwrap();
    assert!(
        line1.ends_with(&format!("(version: v{SUITE_VERSION})")),
        "report line 1: {line1}"
    );
    assert_no_perl_version("aligner report", &report);

    let bam = out.path().join("reads_bismark_bt2.bam");
    let reader = bismark::io::BamReader::from_path(&bam).unwrap();
    let mut buf = Vec::new();
    noodles_sam::io::Writer::new(&mut buf)
        .write_header(reader.header())
        .unwrap();
    let header = String::from_utf8(buf).unwrap();
    assert!(
        header.contains(&format!("@PG\tID:Bismark\tVN:v{SUITE_VERSION}\t")),
        "BAM header:\n{header}"
    );
    assert_no_perl_version("BAM header", &header);
}

#[test]
fn extractor_outputs_carry_the_suite_version() {
    let out = TempDir::new().unwrap();
    Command::cargo_bin("bismark_methylation_extractor")
        .unwrap()
        .arg(data_dir().join("extractor/nondir_pe_1030.bam"))
        .arg("--paired-end")
        .arg("--output_dir")
        .arg(out.path())
        .assert()
        .success();

    let mut split_files = 0;
    for entry in fs::read_dir(out.path()).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.starts_with("CpG_") || name.starts_with("CHG_") || name.starts_with("CHH_") {
            let text = fs::read_to_string(&path).unwrap();
            assert_eq!(
                text.lines().next().unwrap(),
                format!("Bismark methylation extractor version v{SUITE_VERSION}"),
                "{name} line 1"
            );
            assert_no_perl_version(&name, &text);
            split_files += 1;
        }
    }
    assert!(split_files > 0, "no split files written");

    let report =
        fs::read_to_string(out.path().join("nondir_pe_1030_splitting_report.txt")).unwrap();
    assert!(
        report.contains(&format!("Bismark Extractor Version: v{SUITE_VERSION}\n")),
        "splitting report:\n{report}"
    );
    assert_no_perl_version("splitting report", &report);
}

#[test]
fn summary_html_carries_the_suite_version() {
    let dir = TempDir::new().unwrap();
    common::build_wgbs_two_sample(dir.path());
    Command::cargo_bin("bismark2summary")
        .unwrap()
        .args(["-o", "out"])
        .current_dir(dir.path())
        .assert()
        .success();
    let html = fs::read_to_string(dir.path().join("out.html")).unwrap();
    assert!(
        html.contains(&format!("(version {SUITE_VERSION})")),
        "summary HTML footer lacks the suite version"
    );
    assert_no_perl_version("summary HTML", &html);
}

#[cfg(unix)]
#[test]
fn genome_prep_banner_carries_the_suite_version() {
    let tmp = TempDir::new().unwrap();
    let genome = tmp.path().join("genome");
    fs::create_dir_all(&genome).unwrap();
    fs::write(genome.join("g.fa"), b">chr1\nACGT\n").unwrap();
    let bin = tmp.path().join("fakebin");
    fs::create_dir_all(&bin).unwrap();
    write_exec(&bin.join("bowtie2-build"), "#!/bin/sh\nexit 0\n");

    let out = Command::cargo_bin("bismark_genome_preparation")
        .unwrap()
        .env("BISMARK_BIN", &bin)
        .arg(&genome)
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let banner = format!("Bisulfite Genome Indexer version v{SUITE_VERSION}\n");
    assert!(text.contains(&banner), "banner missing:\n{text}");
    assert!(
        !text.contains("last modified"),
        "banner still dated:\n{text}"
    );
    assert_no_perl_version("genome-prep output", &text);
}
