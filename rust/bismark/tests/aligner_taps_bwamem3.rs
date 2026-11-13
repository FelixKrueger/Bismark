//! `--taps` through the REAL bwa-mem3 on public TAPS spike-in reads (nf-core/test-datasets #2370;
//! see `tests/fixtures/aligner/taps/README.md`). Runs only with `BISMARK_TEST_BWAMEM3=1`; then a
//! missing `bwa-mem3` or fixture fails the test.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::time::Duration;

use assert_cmd::Command;
use tempfile::TempDir;

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

/// (contig, strand, context) -> (methylated, unmethylated), from `XM`/`XG` of the PE BAM.
type Calls = BTreeMap<(String, char, char), (u64, u64)>;

fn call_counts(bam: &Path) -> Calls {
    let mut reader = bismark::io::BamReader::from_path(bam).unwrap();
    let names: Vec<String> = reader
        .header()
        .reference_sequences()
        .keys()
        .map(|k| k.to_string())
        .collect();
    let mut calls = Calls::new();
    for rec in reader.records() {
        let rec = rec.unwrap();
        let inner = rec.inner();
        let contig = names[inner.reference_sequence_id().expect("mapped record")].clone();
        let strand = match bismark::io::tags::xg(inner.data()).unwrap() {
            b"CT" => '+',
            b"GA" => '-',
            other => panic!("unexpected XG {}", String::from_utf8_lossy(other)),
        };
        for &c in rec.xm() {
            let context = match c.to_ascii_uppercase() {
                b'Z' => 'Z',
                b'X' => 'X',
                b'H' => 'H',
                _ => continue,
            };
            let e = calls.entry((contig.clone(), strand, context)).or_default();
            if c.is_ascii_uppercase() {
                e.0 += 1;
            } else {
                e.1 += 1;
            }
        }
    }
    calls
}

/// `expected.tsv`: `report<TAB>label<TAB>value` and
/// `calls<TAB>contig<TAB>strand<TAB>context<TAB>meth<TAB>unmeth` rows.
fn expected() -> (Vec<(String, String)>, Calls) {
    let text = fs::read_to_string(fixture("expected.tsv")).unwrap();
    let (mut report, mut calls) = (Vec::new(), Calls::new());
    for line in text
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let f: Vec<&str> = line.split('\t').collect();
        match f[0] {
            "report" => report.push((f[1].to_string(), f[2].to_string())),
            "calls" => {
                calls.insert(
                    (
                        f[1].to_string(),
                        f[2].chars().next().unwrap(),
                        f[3].chars().next().unwrap(),
                    ),
                    (f[4].parse().unwrap(), f[5].parse().unwrap()),
                );
            }
            other => panic!("bad expected.tsv row kind {other}"),
        }
    }
    (report, calls)
}

fn run(mode: &[&str], genome: &Path, index: &Path, out: &Path) {
    Command::cargo_bin("bismark")
        .unwrap()
        .args(mode)
        .args(["-p", "4"])
        .arg("--five_base_index")
        .arg(index)
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
}

fn records(bam: &Path) -> Vec<String> {
    bismark::io::BamReader::from_path(bam)
        .unwrap()
        .records()
        .map(|r| format!("{:?}", r.unwrap().inner()))
        .collect()
}

fn header_text(bam: &Path) -> String {
    let file = fs::File::open(bam).unwrap();
    let header = noodles_bam::io::Reader::new(std::io::BufReader::new(file))
        .read_header()
        .unwrap();
    let mut buf = Vec::new();
    noodles_sam::io::Writer::new(&mut buf)
        .write_header(&header)
        .unwrap();
    String::from_utf8(buf).unwrap()
}

fn fraction(calls: &Calls, contig: &str, strands: &[char], context: char) -> f64 {
    let (m, u) = strands.iter().fold((0u64, 0u64), |acc, s| {
        let (m, u) = calls
            .get(&(contig.to_string(), *s, context))
            .copied()
            .unwrap_or((0, 0));
        (acc.0 + m, acc.1 + u)
    });
    assert!(m + u > 0, "no {context} calls on {contig} {strands:?}");
    m as f64 / (m + u) as f64
}

#[test]
fn taps_spike_ins_polarity_counts_and_identity() {
    if !bwamem3_enabled() {
        eprintln!("skipping: BISMARK_TEST_BWAMEM3 unset (TAPS gate)");
        return;
    }
    let genome = TempDir::new().unwrap();
    fs::copy(fixture("spikes.fa"), genome.path().join("spikes.fa")).unwrap();
    let idx = TempDir::new().unwrap();
    let fa = idx.path().join("spikes.fa");
    fs::copy(fixture("spikes.fa"), &fa).unwrap();
    let st = StdCommand::new("bwa-mem3")
        .args(["index", "--meth"])
        .arg(&fa)
        .output()
        .unwrap();
    assert!(
        st.status.success(),
        "bwa-mem3 index --meth failed: {}",
        String::from_utf8_lossy(&st.stderr)
    );

    let taps = TempDir::new().unwrap();
    run(&["--taps"], genome.path(), &fa, taps.path());
    let five = TempDir::new().unwrap();
    run(
        &["--illumina_5base", "--bwamem3"],
        genome.path(),
        &fa,
        five.path(),
    );
    let bam = |d: &Path| d.join("taps_R1_bismark_bwamem3_pe.bam");

    // --meth=taps and --meth=5base give the same records.
    let taps_records = records(&bam(taps.path()));
    assert!(!taps_records.is_empty());
    assert_eq!(taps_records, records(&bam(five.path())));

    // The real engine ran, in TAPS mode.
    let rep =
        fs::read_to_string(taps.path().join("taps_R1_bismark_bwamem3_PE_report.txt")).unwrap();
    assert!(
        rep.contains("Bismark was run with bwa-mem3 against the unconverted genome"),
        "{rep}"
    );
    assert!(rep.contains("mem --meth=taps "), "{rep}");
    assert!(
        rep.contains("\nLibrary chemistry: TAPS (mC->T)\n\n"),
        "{rep}"
    );
    let text = header_text(&bam(taps.path()));
    assert!(
        text.lines()
            .any(|l| l.starts_with("@PG\tID:bwa-mem3\t")
                && l.contains("\tCL:bwa-mem3 mem --meth=taps ")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.starts_with("@PG\tID:Bismark\t") && l.contains("--taps")),
        "{text}"
    );

    // Exact counts recorded when the fixture was built.
    let (want_report, want_calls) = expected();
    assert!(!want_report.is_empty() && !want_calls.is_empty());
    for (label, value) in &want_report {
        assert!(
            rep.contains(&format!("{label}\t{value}\n")),
            "report line {label:?} should be {value}:\n{rep}"
        );
    }
    let calls = call_counts(&bam(taps.path()));
    assert_eq!(calls, want_calls);

    // Untrimmed spike-in biology.
    let lambda = "bacteriophage_lambda_CpG";
    let unmod = "2kb_3_Unmodified";
    assert!(fraction(&calls, lambda, &['+', '-'], 'Z') >= 0.95);
    assert!(fraction(&calls, unmod, &['+', '-'], 'Z') <= 0.01);
    for contig in [lambda, unmod] {
        assert!(
            fraction(&calls, contig, &['+', '-'], 'H') <= 0.01,
            "{contig} CHH"
        );
    }
    let (ot, ob) = (
        fraction(&calls, lambda, &['+'], 'Z'),
        fraction(&calls, lambda, &['-'], 'Z'),
    );
    assert!((ot - ob).abs() <= 0.02, "lambda OT {ot} vs OB {ob}");
}
