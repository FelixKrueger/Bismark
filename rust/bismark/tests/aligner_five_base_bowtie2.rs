//! #1125 Illumina 5-Base paired-end alignment through the REAL bowtie2, multi-threaded,
//! with variable-length reads. Every pair must come back, in FASTQ order, with `--strandID`
//! tagging both mates. Gated on `bowtie2` + `bowtie2-build` on PATH (fails loud in CI).

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use tempfile::TempDir;

fn have_bowtie2() -> bool {
    let ok = |bin: &str| {
        StdCommand::new(bin)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    let present = ok("bowtie2") && ok("bowtie2-build");
    if !present && std::env::var_os("CI").is_some() {
        panic!(
            "bowtie2/bowtie2-build not found but $CI is set: refusing to skip the 5-Base bowtie2 gate."
        );
    }
    present
}

/// Deterministic LCG, so the reference and read layout are stable across runs.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }
}

fn revcomp(seq: &[u8]) -> Vec<u8> {
    seq.iter()
        .rev()
        .map(|&b| match b {
            b'A' => b'T',
            b'C' => b'G',
            b'G' => b'C',
            b'T' => b'A',
            o => o,
        })
        .collect()
}

fn fastq_record(out: &mut Vec<u8>, header: &str, seq: &[u8]) {
    out.extend_from_slice(format!("@{header}\n").as_bytes());
    out.extend_from_slice(seq);
    out.extend_from_slice(b"\n+\n");
    out.extend(std::iter::repeat_n(b'I', seq.len()));
    out.push(b'\n');
}

#[test]
fn five_base_pe_bowtie2_multithreaded_keeps_every_pair_in_order() {
    if !have_bowtie2() {
        eprintln!("skipping: bowtie2 not on PATH (5-Base bowtie2 PE gate)");
        return;
    }
    let mut rng = Lcg(0x1125_5BA5_E000_0001);
    let reference: Vec<u8> = (0..60_000).map(|_| b"ACGT"[rng.next(4) as usize]).collect();

    let genome = TempDir::new().unwrap();
    let mut fa = b">chr1\n".to_vec();
    fa.extend_from_slice(&reference);
    fa.push(b'\n');
    fs::write(genome.path().join("genome.fa"), fa).unwrap();
    let index = genome.path().join("genome");
    let built = StdCommand::new("bowtie2-build")
        .arg("-q")
        .arg(genome.path().join("genome.fa"))
        .arg(&index)
        .status()
        .unwrap();
    assert!(built.success(), "bowtie2-build failed");

    // Variable mate lengths (Trim Galore-like) and Illumina-style comments on each header.
    let n_pairs = 3000;
    let (mut fq1, mut fq2) = (Vec::new(), Vec::new());
    for p in 0..n_pairs {
        let frag = 200 + rng.next(120) as usize;
        let start = rng.next((reference.len() - frag) as u64) as usize;
        let (l1, l2) = (60 + rng.next(66) as usize, 60 + rng.next(66) as usize);
        let r1 = &reference[start..start + l1];
        let r2 = revcomp(&reference[start + frag - l2..start + frag]);
        fastq_record(&mut fq1, &format!("pair{p} 1:N:0:ACGT"), r1);
        fastq_record(&mut fq2, &format!("pair{p} 2:N:0:ACGT"), &r2);
    }
    let read1 = genome.path().join("reads_1.fq");
    let read2 = genome.path().join("reads_2.fq");
    fs::write(&read1, &fq1).unwrap();
    fs::write(&read2, &fq2).unwrap();
    let outdir = TempDir::new().unwrap();
    let temp = TempDir::new().unwrap();

    Command::cargo_bin("bismark")
        .unwrap()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .arg("--bowtie2")
        .arg("--five_base_index")
        .arg(&index)
        .args(["-p", "4", "--strandID"])
        .arg("-1")
        .arg(&read1)
        .arg("-2")
        .arg(&read2)
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .assert()
        .success();

    let bam: &Path = &outdir.path().join("reads_1_bismark_bt2_pe.bam");
    let mut reader = bismark::io::BamReader::from_path(bam).unwrap();
    let mut r1_names = Vec::new();
    let mut records = 0usize;
    for rec in reader.records() {
        let rec = rec.unwrap();
        let inner = rec.inner();
        records += 1;
        let ys = inner
            .data()
            .get(&noodles_sam::alignment::record::data::field::Tag::from(
                *b"YS",
            ))
            .map(|v| format!("{v:?}"));
        assert!(
            ys.as_deref()
                .is_some_and(|s| s.contains("OT") || s.contains("OB")),
            "every mate carries YS:Z:OT|OB, got {ys:?}"
        );
        if u16::from(inner.flags()) & 0x40 != 0 {
            r1_names.push(String::from_utf8_lossy(inner.name().unwrap().as_ref()).into_owned());
        }
    }
    let expected: Vec<String> = (0..n_pairs).map(|p| format!("pair{p}")).collect();
    assert_eq!(
        records,
        2 * n_pairs,
        "every pair written, none silently dropped"
    );
    assert_eq!(r1_names, expected, "pairs come back in FASTQ order");
}

/// A random 10 kb `chr1` with its Bowtie 2 index.
struct SmallGenome {
    dir: TempDir,
    reference: Vec<u8>,
}

impl SmallGenome {
    fn new(seed: u64) -> Self {
        let mut rng = Lcg(seed);
        let reference: Vec<u8> = (0..10_000).map(|_| b"ACGT"[rng.next(4) as usize]).collect();
        let dir = TempDir::new().unwrap();
        let mut fa = b">chr1\n".to_vec();
        fa.extend_from_slice(&reference);
        fa.push(b'\n');
        fs::write(dir.path().join("genome.fa"), fa).unwrap();
        assert!(
            StdCommand::new("bowtie2-build")
                .arg("-q")
                .arg(dir.path().join("genome.fa"))
                .arg(dir.path().join("genome"))
                .status()
                .unwrap()
                .success()
        );
        Self { dir, reference }
    }

    /// Align named pairs through `--illumina_5base --bowtie2` and return the BAM records.
    fn align(
        &self,
        pairs: &[(&str, &[u8], &[u8])],
        extra: &[&str],
    ) -> Vec<bismark::io::BismarkRecord> {
        let (mut fq1, mut fq2) = (Vec::new(), Vec::new());
        for (name, r1, r2) in pairs {
            fastq_record(&mut fq1, name, r1);
            fastq_record(&mut fq2, name, r2);
        }
        let reads = TempDir::new().unwrap();
        let read1 = reads.path().join("reads_1.fq");
        let read2 = reads.path().join("reads_2.fq");
        fs::write(&read1, fq1).unwrap();
        fs::write(&read2, fq2).unwrap();
        let outdir = TempDir::new().unwrap();
        let temp = TempDir::new().unwrap();
        Command::cargo_bin("bismark")
            .unwrap()
            .arg("--illumina_5base")
            .arg("--bowtie2")
            .arg("--genome")
            .arg(self.dir.path())
            .arg("--five_base_index")
            .arg(self.dir.path().join("genome"))
            .args(["-p", "2"])
            .args(extra)
            .arg("-1")
            .arg(&read1)
            .arg("-2")
            .arg(&read2)
            .arg("--output_dir")
            .arg(outdir.path())
            .arg("--temp_dir")
            .arg(temp.path())
            .assert()
            .success();
        let mut reader =
            bismark::io::BamReader::from_path(&outdir.path().join("reads_1_bismark_bt2_pe.bam"))
                .unwrap();
        reader.records().map(Result::unwrap).collect()
    }
}

#[test]
fn five_base_pe_bowtie2_default_max_insert_is_800() {
    if !have_bowtie2() {
        eprintln!("skipping: bowtie2 not on PATH (5-Base insert-size gate)");
        return;
    }
    let g = SmallGenome::new(0x1134_5BA5_E000_0001);
    let fragment = |start: usize, len: usize| {
        (
            g.reference[start..start + 90].to_vec(),
            revcomp(&g.reference[start + len - 90..start + len]),
        )
    };
    let (long1, long2) = fragment(1000, 1000);
    let (mid1, mid2) = fragment(3000, 600);
    for (r1, r2, extra, tlen) in [
        (&long1, &long2, &[][..], None),
        (&long1, &long2, &["-X", "1200"][..], Some(1000)),
        (&mid1, &mid2, &[][..], Some(600)),
        (&mid1, &mid2, &["-X", "500"][..], None),
    ] {
        let records = g.align(&[("pair", r1, r2)], extra);
        assert_eq!(
            records.len(),
            if tlen.is_some() { 2 } else { 0 },
            "{extra:?}"
        );
        for rec in records {
            assert_ne!(u16::from(rec.inner().flags()) & 0x2, 0);
            assert_eq!(Some(rec.inner().template_length().abs()), tlen);
        }
    }
}

#[test]
fn five_base_pe_bowtie2_drops_dovetailed_pairs() {
    if !have_bowtie2() {
        eprintln!("skipping: bowtie2 not on PATH (5-Base dovetail gate)");
        return;
    }
    let g = SmallGenome::new(0x1134_D0E7_A110_0001);
    let rf = &g.reference;
    let (c1, c2) = (rf[6000..6100].to_vec(), revcomp(&rf[6100..6200]));
    // OT: R2 (reverse) starts 20 bp before R1; OB: R1 (reverse) starts 20 bp before R2.
    let (ot1, ot2) = (rf[2020..2120].to_vec(), revcomp(&rf[2000..2100]));
    let (ob1, ob2) = (revcomp(&rf[4000..4100]), rf[4020..4120].to_vec());
    let pairs: [(&str, &[u8], &[u8]); 3] = [
        ("control", &c1, &c2),
        ("dovetail_ot", &ot1, &ot2),
        ("dovetail_ob", &ob1, &ob2),
    ];
    for extra in [&[][..], &["--dovetail"][..], &["--no_dovetail"][..]] {
        let records = g.align(&pairs, extra);
        assert_eq!(records.len(), 2, "{extra:?}: only the control pair is kept");
        for (rec, (flag, tlen)) in records.iter().zip([(99, 200), (147, -200)]) {
            let name = rec.inner().name().map(|n| n.to_vec());
            assert_eq!(name.as_deref(), Some(&b"control"[..]), "{extra:?}");
            assert_eq!(u16::from(rec.inner().flags()), flag);
            assert_eq!(rec.inner().template_length(), tlen);
        }
    }
}
