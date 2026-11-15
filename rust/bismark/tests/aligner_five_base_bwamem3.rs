//! `--illumina_5base --bwamem3` through the REAL bwa-mem3 on a synthetic genome with CpG islands.
//! Runs only with `BISMARK_TEST_BWAMEM3=1`; then a missing `bwa-mem3` fails the test.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::time::{Duration, Instant};

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

fn fastq_record(out: &mut Vec<u8>, name: &str, seq: &[u8]) {
    out.extend_from_slice(format!("@{name}\n").as_bytes());
    out.extend_from_slice(seq);
    out.extend_from_slice(b"\n+\n");
    out.extend(std::iter::repeat_n(b'I', seq.len()));
    out.push(b'\n');
}

const GENOME_LEN: usize = 1_000_000;
const ISLANDS: usize = 40;
const ISLAND_LEN: usize = 1_000;
const READ_LEN: usize = 150;
const UMI_LEN: usize = 5;

/// A random reference with 40 evenly spaced 1 kb CpG islands (a CG every 8 bp).
fn make_reference(rng: &mut Lcg) -> (Vec<u8>, Vec<usize>) {
    let mut r: Vec<u8> = (0..GENOME_LEN)
        .map(|_| b"ACGT"[rng.next(4) as usize])
        .collect();
    let spacing = GENOME_LEN / ISLANDS;
    let starts: Vec<usize> = (0..ISLANDS).map(|k| k * spacing + spacing / 2).collect();
    for &s in &starts {
        for i in (s..s + ISLAND_LEN).step_by(8) {
            r[i] = b'C';
            r[i + 1] = b'G';
        }
    }
    (r, starts)
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Strand {
    Ot,
    Ob,
}

struct Fragment {
    name: String,
    start: usize,
    end: usize,
    strand: Strand,
    methylated: bool,
    island: bool,
}

/// Genomic positions in `[start, end)` that carry a CpG call on `strand`: the C (OT) or the G (OB).
fn cpg_calls(reference: &[u8], start: usize, end: usize, strand: Strand) -> Vec<usize> {
    (start..end)
        .filter(|&p| match strand {
            Strand::Ot => reference[p] == b'C' && reference.get(p + 1) == Some(&b'G'),
            Strand::Ob => reference[p] == b'G' && p > 0 && reference[p - 1] == b'C',
        })
        .collect()
}

/// The fragment's top-strand sequence over `[from, to)` in 5-Base chemistry: a methylated
/// cytosine on the original strand reads as T (OT) or, seen from the top strand, as A (OB).
fn converted_top(
    reference: &[u8],
    from: usize,
    to: usize,
    strand: Strand,
    meth: &[usize],
) -> Vec<u8> {
    let mut top = reference[from..to].to_vec();
    for &p in meth {
        if (from..to).contains(&p) {
            top[p - from] = if strand == Strand::Ot { b'T' } else { b'A' };
        }
    }
    top
}

/// R1 and R2 of a directional pair whose top-strand sequence is `top`.
fn mates(top: &[u8], strand: Strand) -> (Vec<u8>, Vec<u8>) {
    let left = top[..READ_LEN].to_vec();
    let right = revcomp(&top[top.len() - READ_LEN..]);
    match strand {
        Strand::Ot => (left, right),
        Strand::Ob => (right, left),
    }
}

fn reads_of(reference: &[u8], f: &Fragment) -> (Vec<u8>, Vec<u8>) {
    let meth = if f.methylated {
        cpg_calls(reference, f.start, f.end, f.strand)
    } else {
        Vec::new()
    };
    mates(
        &converted_top(reference, f.start, f.end, f.strand, &meth),
        f.strand,
    )
}

/// 1,200 fragments, each written twice: fully CpG-methylated, then unmethylated.
fn make_twins(islands: &[usize], rng: &mut Lcg) -> Vec<Fragment> {
    let mut out = Vec::new();
    for i in 0..1_200 {
        let len = 250 + rng.next(201) as usize;
        let island = i % 2 == 0;
        let start = if island {
            islands[rng.next(islands.len() as u64) as usize]
                + rng.next((ISLAND_LEN - len) as u64) as usize
        } else {
            1_000 + rng.next((GENOME_LEN - len - 2_000) as u64) as usize
        };
        let strand = if rng.next(2) == 0 {
            Strand::Ot
        } else {
            Strand::Ob
        };
        for methylated in [true, false] {
            out.push(Fragment {
                name: format!("f{i}_{}", if methylated { "m" } else { "u" }),
                start,
                end: start + len,
                strand,
                methylated,
                island,
            });
        }
    }
    out
}

fn write_pairs(dir: &Path, stem: &str, pairs: &[(String, Vec<u8>, Vec<u8>)]) -> (PathBuf, PathBuf) {
    let (mut fq1, mut fq2) = (Vec::new(), Vec::new());
    for (name, r1, r2) in pairs {
        fastq_record(&mut fq1, name, r1);
        fastq_record(&mut fq2, name, r2);
    }
    let (p1, p2) = (
        dir.join(format!("{stem}_1.fq")),
        dir.join(format!("{stem}_2.fq")),
    );
    fs::write(&p1, fq1).unwrap();
    fs::write(&p2, fq2).unwrap();
    (p1, p2)
}

/// The genome folder and a bwa-mem3 meth index of the same FASTA, kept in separate folders.
struct Setup {
    genome: TempDir,
    index_dir: TempDir,
}

impl Setup {
    fn new(reference: &[u8]) -> Self {
        let mut fa = b">chr1\n".to_vec();
        for line in reference.chunks(60) {
            fa.extend_from_slice(line);
            fa.push(b'\n');
        }
        let genome = TempDir::new().unwrap();
        fs::write(genome.path().join("genome.fa"), &fa).unwrap();
        let index_dir = TempDir::new().unwrap();
        fs::write(index_dir.path().join("genome.fa"), &fa).unwrap();
        let t = Instant::now();
        let st = StdCommand::new("bwa-mem3")
            .args(["index", "--meth"])
            .arg(index_dir.path().join("genome.fa"))
            .output()
            .unwrap();
        assert!(
            st.status.success(),
            "bwa-mem3 index --meth failed: {}",
            String::from_utf8_lossy(&st.stderr)
        );
        assert!(
            t.elapsed() < Duration::from_secs(120),
            "index build too slow"
        );
        Self { genome, index_dir }
    }

    /// Run `bismark --illumina_5base --bwamem3` and return the output folder.
    fn run(&self, r1: &Path, r2: &Path, extra: &[&str]) -> TempDir {
        let out = TempDir::new().unwrap();
        Command::cargo_bin("bismark")
            .unwrap()
            .args(["--illumina_5base", "--bwamem3", "-p", "4"])
            .arg("--five_base_index")
            .arg(self.index_dir.path().join("genome.fa"))
            .arg("--genome")
            .arg(self.genome.path())
            .args(extra)
            .arg("--output_dir")
            .arg(out.path())
            .arg("-1")
            .arg(r1)
            .arg("-2")
            .arg(r2)
            .timeout(Duration::from_secs(120))
            .assert()
            .success();
        out
    }
}

fn report(out: &Path, stem: &str) -> String {
    fs::read_to_string(out.join(format!("{stem}_1_bismark_bwamem3_PE_report.txt"))).unwrap()
}

/// One BAM record: flag, 0-based aligned span, and the XM call at each aligned genomic position.
struct Mate {
    flag: u16,
    span: (usize, usize),
    calls: HashMap<usize, u8>,
    xm: Vec<u8>,
}

/// The BAM's mates in file order, grouped by read name.
fn read_bam(out: &Path, stem: &str) -> Vec<(String, Vec<Mate>)> {
    use noodles_sam::alignment::record::cigar::op::Kind::*;
    let bam = out.join(format!("{stem}_1_bismark_bwamem3_pe.bam"));
    let mut reader = bismark::io::BamReader::from_path(&bam).unwrap();
    let mut pairs: Vec<(String, Vec<Mate>)> = Vec::new();
    for rec in reader.records() {
        let rec = rec.unwrap();
        let inner = rec.inner();
        let name = String::from_utf8_lossy(inner.name().unwrap().as_ref()).into_owned();
        let xm = bismark::io::tags::xm(inner.data()).unwrap().to_vec();
        let start = usize::from(inner.alignment_start().unwrap()) - 1;
        let (mut ri, mut rp, mut calls) = (0usize, start, HashMap::new());
        for op in inner.cigar().as_ref() {
            match op.kind() {
                Match | SequenceMatch | SequenceMismatch => {
                    for _ in 0..op.len() {
                        calls.insert(rp, xm[ri]);
                        ri += 1;
                        rp += 1;
                    }
                }
                Insertion | SoftClip => ri += op.len(),
                Deletion | Skip => rp += op.len(),
                _ => {}
            }
        }
        let mate = Mate {
            flag: u16::from(inner.flags()),
            span: (start, rp),
            calls,
            xm,
        };
        match pairs.last_mut() {
            Some((n, m)) if *n == name => m.push(mate),
            _ => pairs.push((name, vec![mate])),
        }
    }
    pairs
}

#[test]
fn bwamem3_dense_methylated_reads_align_like_their_unmethylated_twins() {
    if !bwamem3_enabled() {
        eprintln!("skipping: BISMARK_TEST_BWAMEM3 unset (bwa-mem3 5-Base gate)");
        return;
    }
    let mut rng = Lcg(0xB3A3_5BA5_E000_1105);
    let (reference, islands) = make_reference(&mut rng);
    let setup = Setup::new(&reference);
    let reads = TempDir::new().unwrap();

    let frags = make_twins(&islands, &mut rng);
    let pairs: Vec<_> = frags
        .iter()
        .map(|f| {
            let (r1, r2) = reads_of(&reference, f);
            (f.name.clone(), r1, r2)
        })
        .collect();
    let (r1, r2) = write_pairs(reads.path(), "twins", &pairs);
    let t = Instant::now();
    let out = setup.run(&r1, &r2, &[]);
    eprintln!("bwa-mem3 5-Base run: {:.1} s", t.elapsed().as_secs_f64());

    let bam = read_bam(out.path(), "twins");
    let names: Vec<&str> = bam.iter().map(|(n, _)| n.as_str()).collect();
    let mut expected_order: Vec<&str> = frags.iter().map(|f| f.name.as_str()).collect();
    expected_order.retain(|n| names.contains(n));
    assert_eq!(names, expected_order, "BAM order is FASTQ order");
    let kept: HashMap<&str, &Vec<Mate>> = bam.iter().map(|(n, m)| (n.as_str(), m)).collect();
    assert!(
        kept.values().all(|m| m.len() == 2),
        "two mates per kept pair"
    );
    assert!(
        kept.len() * 100 >= frags.len() * 95,
        "kept {} of {} pairs",
        kept.len(),
        frags.len()
    );

    let rate = |meth: bool| {
        let set: Vec<_> = frags
            .iter()
            .filter(|f| f.island && f.methylated == meth)
            .collect();
        let k = set
            .iter()
            .filter(|f| kept.contains_key(f.name.as_str()))
            .count();
        (k as f64 / set.len() as f64, set.len())
    };
    let ((meth, n_meth), (unmeth, _)) = (rate(true), rate(false));
    assert!(
        meth >= 0.95,
        "dense methylated twins kept: {meth:.3} of {n_meth}"
    );
    assert!(
        (meth - unmeth).abs() <= 0.02,
        "dense methylated {meth:.3} vs unmethylated {unmeth:.3}"
    );

    let (mut truth, mut checked, mut wrong) = (0usize, 0usize, Vec::new());
    for f in &frags {
        let mut sites = cpg_calls(&reference, f.start, f.start + READ_LEN, f.strand);
        sites.extend(cpg_calls(&reference, f.end - READ_LEN, f.end, f.strand));
        sites.sort_unstable();
        sites.dedup();
        truth += sites.len();
        let Some(mates) = kept.get(f.name.as_str()) else {
            continue;
        };
        if !mates
            .iter()
            .all(|m| m.span.0 >= f.start && m.span.1 <= f.end)
        {
            continue;
        }
        let want = if f.methylated { b'Z' } else { b'z' };
        for &p in &sites {
            let got: Vec<u8> = mates
                .iter()
                .filter_map(|m| m.calls.get(&p).copied())
                .collect();
            if got.is_empty() {
                continue;
            }
            checked += 1;
            if got.iter().any(|&c| c != want) {
                wrong.push((f.name.clone(), p, got));
            }
        }
    }
    eprintln!(
        "kept {} of {}; dense twins kept: methylated {meth:.4}, unmethylated {unmeth:.4}; \
         CpGs checked {checked} of {truth}",
        kept.len(),
        frags.len()
    );
    assert!(
        wrong.is_empty(),
        "{} wrong CpG calls, first: {:?}",
        wrong.len(),
        &wrong[..wrong.len().min(5)]
    );
    assert!(
        checked * 100 >= truth * 90,
        "only {checked} of {truth} truth CpGs covered"
    );

    // --upto stops early without hanging.
    let upto = setup.run(&r1, &r2, &["--upto", "100"]);
    assert!(
        report(upto.path(), "twins").contains("Sequence pairs analysed in total:\t100\n"),
        "{}",
        report(upto.path(), "twins")
    );

    // Empty input.
    let (e1, e2) = write_pairs(reads.path(), "empty", &[]);
    let empty = setup.run(&e1, &e2, &[]);
    assert!(report(empty.path(), "empty").contains("Sequence pairs analysed in total:\t0\n"));
}

/// An inline UMI made of the genome's own flanking bases, C->T, aligns for free under neutral
/// scoring; `--five_base_umi_len` must still keep it out of the calls.
#[test]
fn bwamem3_inline_umi_is_never_called() {
    if !bwamem3_enabled() {
        eprintln!("skipping: BISMARK_TEST_BWAMEM3 unset (bwa-mem3 UMI gate)");
        return;
    }
    let mut rng = Lcg(0xB3A3_0000_0000_0005);
    let (reference, islands) = make_reference(&mut rng);
    let setup = Setup::new(&reference);
    let reads = TempDir::new().unwrap();

    let mut pairs = Vec::new();
    for i in 0..200 {
        let len = 250 + rng.next(201) as usize;
        let start = islands[i % ISLANDS] + rng.next((ISLAND_LEN - len) as u64) as usize;
        let (from, to) = (start - UMI_LEN, start + len + UMI_LEN);
        let flanks: Vec<usize> = (from..start)
            .chain(start + len..to)
            .filter(|&p| reference[p] == b'C')
            .collect();
        let top = converted_top(&reference, from, to, Strand::Ot, &flanks);
        let (r1, r2) = mates(&top, Strand::Ot);
        pairs.push((format!("u{i}"), r1, r2));
    }
    let (r1, r2) = write_pairs(reads.path(), "umi", &pairs);
    // Mates with any call in their first UMI_LEN read bases.
    let called_umis = |extra: &[&str]| {
        let out = setup.run(&r1, &r2, extra);
        let bam = read_bam(out.path(), "umi");
        assert!(
            bam.len() * 100 >= pairs.len() * 90,
            "kept {} of 200",
            bam.len()
        );
        bam.iter()
            .flat_map(|(_, m)| m)
            .filter(|m| {
                let umi = if m.flag & 0x10 != 0 {
                    &m.xm[m.xm.len() - UMI_LEN..]
                } else {
                    &m.xm[..UMI_LEN]
                };
                umi.iter().any(|&c| c != b'.')
            })
            .count()
    };
    let unmasked = called_umis(&[]);
    eprintln!("mates with UMI calls, unmasked: {unmasked}");
    assert!(
        unmasked > 0,
        "bwa-mem3 soft-clipped every UMI: the test is vacuous"
    );
    assert_eq!(called_umis(&["--five_base_umi_len", "5"]), 0);
}
