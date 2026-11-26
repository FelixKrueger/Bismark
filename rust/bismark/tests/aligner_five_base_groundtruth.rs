//! #787 Illumina 5-Base SYNTHETIC GROUND-TRUTH gates.
//!
//! A true "concordance vs DRAGEN" gate is impossible without DRAGEN (proprietary
//! FPGA, no reference output) and without a public raw 5-Base dataset. This is the
//! achievable, stronger substitute: we synthesize reads from a known reference with
//! a KNOWN methylation pattern (5mC -> T at chosen CpGs), align them to the UNCONVERTED
//! genome via `--illumina_5base`, and assert the pipeline recovers the correct
//! methylated (`Z`) vs unmethylated (`z`) CpG call at every aligned CpG.
//!
//! Each gate runs on the real bwa-mem3 (`_bwamem3`, only with `BISMARK_TEST_BWAMEM3=1`) and,
//! where it tests the driver rather than the aligner, on a placement-table fake (`_hermetic`,
//! every job). The check is PER GENOMIC POSITION (walks the BAM POS+CIGAR), so it is robust
//! to soft-clipped read ends (a clipped CpG is simply not checked) while still catching any
//! wrong-polarity call.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[path = "common/fake_bwamem3.rs"]
mod fake_bwamem3;
use fake_bwamem3::Placement;

/// Decompress the first FASTA contig of a `.fa.gz` into uppercase bytes.
fn load_first_contig_gz(path: &Path) -> Vec<u8> {
    let file = fs::File::open(path).unwrap();
    let mut s = String::new();
    flate2::read::MultiGzDecoder::new(file)
        .read_to_string(&mut s)
        .unwrap();
    let mut out = Vec::new();
    let mut seen_header = false;
    for line in s.lines() {
        if line.starts_with('>') {
            if seen_header {
                break; // only the first contig
            }
            seen_header = true;
            continue;
        }
        out.extend(line.trim().bytes().map(|b| b.to_ascii_uppercase()));
    }
    out
}

/// Per-read ground truth: qname → list of (genomic CpG position, expected methylated?).
type Truth = HashMap<String, Vec<(usize, bool)>>;

fn bin() -> Command {
    Command::cargo_bin("bismark").unwrap()
}

/// Runs only with `BISMARK_TEST_BWAMEM3=1`; then a missing `bwa-mem3` fails the test.
fn have_bwamem3() -> bool {
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

/// The 5-Base engine a control gate runs on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Engine {
    BwaMem3,
    /// bwa-mem3 with no engine flag: the index is found in `<genome>/Bwamem3_Index/`.
    BwaMem3Default,
}

impl Engine {
    fn available(self) -> bool {
        match self {
            Engine::BwaMem3 | Engine::BwaMem3Default => have_bwamem3(),
        }
    }

    /// The engine's output-name token (`<stem>_bismark_<token>_pe.bam`).
    fn token(self) -> &'static str {
        match self {
            Engine::BwaMem3 | Engine::BwaMem3Default => "bwamem3",
        }
    }

    /// The engine flag, if any; for bwa-mem3 also builds `<genome>/Bwamem3_Index/` from `genome.fa`.
    fn prepare(self, genome: &Path) -> Option<&'static str> {
        match self {
            Engine::BwaMem3 | Engine::BwaMem3Default => {
                let dir = genome.join("Bwamem3_Index");
                fs::create_dir_all(&dir).unwrap();
                fs::copy(genome.join("genome.fa"), dir.join("genome.fa")).unwrap();
                let st = StdCommand::new("bwa-mem3")
                    .args(["index", "--meth"])
                    .arg(dir.join("genome.fa"))
                    .output()
                    .unwrap();
                assert!(
                    st.status.success(),
                    "bwa-mem3 index --meth failed: {}",
                    String::from_utf8_lossy(&st.stderr)
                );
                (self == Engine::BwaMem3).then_some("--bwamem3")
            }
        }
    }
}

/// Deterministic pseudo-random ACGT reference (fixed LCG → stable CpG layout).
fn gen_reference(n: usize) -> Vec<u8> {
    gen_sequence(n, 0x2545_F491_4F6C_DD1D)
}

/// Deterministic pseudo-random ACGT sequence from `seed`.
fn gen_sequence(n: usize, seed: u64) -> Vec<u8> {
    let bases = [b'A', b'C', b'G', b'T'];
    let mut x: u64 = seed;
    (0..n)
        .map(|_| {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            bases[((x >> 33) & 3) as usize]
        })
        .collect()
}

/// 0-based positions `i` where `ref[i..i+2] == "CG"` (the CpG cytosine).
fn cpg_positions(reference: &[u8]) -> Vec<usize> {
    (0..reference.len().saturating_sub(1))
        .filter(|&i| reference[i] == b'C' && reference[i + 1] == b'G')
        .collect()
}

/// Genome dir: raw `genome.fa` and nothing else — the 5-Base path aligns against the FASTA
/// directly and requires no bisulfite index (#1099).
fn write_genome(dir: &Path, reference: &[u8]) {
    let mut fa = Vec::new();
    fa.extend_from_slice(b">chr1\n");
    fa.extend_from_slice(reference);
    fa.push(b'\n');
    fs::write(dir.join("genome.fa"), fa).unwrap();
}

/// Forward 5-Base reads (exact reference slices) with a KNOWN methylation pattern:
/// every even-indexed CpG inside a read's non-anchored core is "methylated" (its
/// cytosine written `T`, simulating 5mC->T); all other CpGs keep `C`. The 12 bp
/// flanks are kept exact so the aligner anchors and does not soft-clip the core.
///
/// Returns the FASTQ bytes and, per read qname, the ground truth:
/// `genomic CpG position (0-based) -> expected methylated?`.
fn make_methylated_reads(reference: &[u8], read_len: usize, n_reads: usize) -> (Vec<u8>, Truth) {
    let cpgs = cpg_positions(reference);
    let anchor = 12usize;
    let mut fastq = Vec::new();
    let mut truth: HashMap<String, Vec<(usize, bool)>> = HashMap::new();
    for r in 0..n_reads {
        let start = r * read_len;
        if start + read_len > reference.len() {
            break;
        }
        let qname = format!("read{r}");
        let mut read = reference[start..start + read_len].to_vec();
        let mut here = Vec::new();
        for (gi, &cpos) in cpgs.iter().enumerate() {
            if cpos < start || cpos >= start + read_len {
                continue; // CpG not in this read
            }
            let in_core = cpos >= start + anchor && cpos < start + read_len - anchor;
            let methylated = in_core && gi % 2 == 0;
            if methylated {
                read[cpos - start] = b'T'; // 5mC -> T
            }
            here.push((cpos, methylated)); // every CpG in the read, with its truth
        }
        truth.insert(qname.clone(), here);
        fastq.extend_from_slice(format!("@{qname}\n").as_bytes());
        fastq.extend_from_slice(&read);
        fastq.extend_from_slice(b"\n+\n");
        fastq.extend_from_slice(&vec![b'I'; read_len]);
        fastq.push(b'\n');
    }
    (fastq, truth)
}

/// The engine a synthetic ground-truth gate runs on.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Gt {
    /// Real bwa-mem3, with filler pairs on a second contig so it can estimate the insert size
    /// and flag the fixture's few pairs as proper.
    BwaMem3,
    /// The placement-table fake: the fixture's own truth, no filler.
    Hermetic,
}

impl Gt {
    fn available(self) -> bool {
        match self {
            Gt::BwaMem3 => have_bwamem3(),
            Gt::Hermetic => cfg!(unix),
        }
    }
}

/// Filler pairs on the `filler` contig of a real-bwa-mem3 run; each is its own single-strand
/// family.
const N_FILLER: usize = 40;

/// Append [`N_FILLER`] unmethylated OT pairs tiled over `filler` (fragments 120–198 bp, 60 bp
/// mates), each with its own dual UMI.
fn filler_pairs(filler: &[u8], fq1: &mut Vec<u8>, fq2: &mut Vec<u8>) {
    let umi = |v: usize| -> String {
        (0..8)
            .map(|i| ['A', 'C', 'G', 'T'][(v >> (2 * i)) & 3])
            .collect()
    };
    let rl = 60usize;
    for i in 0..N_FILLER {
        let (start, frag) = (i * 240, 120 + i * 2);
        let name = format!("fill{i}:{}+{}", umi(2 * i + 1000), umi(2 * i + 1001));
        emit_fastq(fq1, &name, &filler[start..start + rl]);
        emit_fastq(
            fq2,
            &name,
            &revcomp(&filler[start + frag - rl..start + frag]),
        );
    }
}

fn emit_fastq(fq: &mut Vec<u8>, name: &str, bytes: &[u8]) {
    fq.extend_from_slice(format!("@{name}\n").as_bytes());
    fq.extend_from_slice(bytes);
    fq.extend_from_slice(b"\n+\n");
    fq.extend_from_slice(&vec![b'I'; bytes.len()]);
    fq.push(b'\n');
}

/// The outputs of one synthetic run (reads `r1.fq`/`r2.fq`).
struct GtRun {
    _genome: TempDir,
    _bins: TempDir,
    out: TempDir,
    stderr: String,
}

impl GtRun {
    /// `<out>/r1_bismark_bwamem3_<suffix>`.
    fn file(&self, suffix: &str) -> PathBuf {
        self.out.path().join(format!("r1_bismark_bwamem3_{suffix}"))
    }
}

/// Align `fq1`/`fq2` against `reference` (`chr1`) on `engine` with `--illumina_5base` plus
/// `extra`. `placements` is where each pair of the fixture maps on `chr1`.
fn run_gt(
    engine: Gt,
    reference: &[u8],
    mut fq1: Vec<u8>,
    mut fq2: Vec<u8>,
    placements: &[Placement],
    extra: &[&str],
) -> GtRun {
    let (genome, bins, out, temp) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let mut cmd = bin();
    match engine {
        Gt::BwaMem3 => {
            let filler = gen_sequence(10_000, 0x5EED_F111_F111_0001);
            filler_pairs(&filler, &mut fq1, &mut fq2);
            write_genome_multi(genome.path(), &[("chr1", reference), ("filler", &filler)]);
            Engine::BwaMem3Default.prepare(genome.path());
        }
        Gt::Hermetic => {
            write_genome(genome.path(), reference);
            let prefix = bins.path().join("idx.fa");
            fake_bwamem3::install(
                bins.path(),
                &prefix,
                &[("chr1", reference.len())],
                placements,
            );
            cmd.arg("--five_base_index")
                .arg(&prefix)
                .arg("--path_to_bwamem3")
                .arg(bins.path());
        }
    }
    let (read1, read2) = (genome.path().join("r1.fq"), genome.path().join("r2.fq"));
    fs::write(&read1, &fq1).unwrap();
    fs::write(&read2, &fq2).unwrap();
    let done = cmd
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .args(extra)
        .arg("-1")
        .arg(&read1)
        .arg("-2")
        .arg(&read2)
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(out.path())
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&done.get_output().stderr).into_owned();
    GtRun {
        _genome: genome,
        _bins: bins,
        out,
        stderr,
    }
}

fn placed(qname: &str, r1: usize, r2: usize, r1_reverse: bool) -> Placement {
    Placement {
        qname: qname.to_string(),
        contig: "chr1".into(),
        r1,
        r2,
        r1_reverse,
    }
}

/// The CpG target inside the R1∩R2 overlap of a 140 bp fragment starting at `frag_start`.
fn pick_overlap_cpg(cpgs: &[usize], frag_start: usize) -> usize {
    *cpgs
        .iter()
        .find(|&&c| c >= frag_start + 45 && c < frag_start + 95)
        .expect("a CpG in the mate overlap")
}

/// One molecule as two FR pairs. TOP strand (+ref orientation) carries `T` at `t` (5mC or a
/// variant); the BOTTOM strand keeps `C` there unless `variant` (a homozygous C>T is on both
/// strands). OT pair: R1 forward-left + R2 reverse-right; OB pair: R1 reverse-right + R2
/// forward-left. `both_strands = false` emits the OT pair alone (a simplex family).
#[allow(clippy::too_many_arguments)]
fn molecule(
    reference: &[u8],
    (fq1, fq2, placements): (&mut Vec<u8>, &mut Vec<u8>, &mut Vec<Placement>),
    s: usize,
    t: usize,
    variant: bool,
    (ua, ub): (&str, &str),
    both_strands: bool,
) {
    let (frag, rl) = (140usize, 100usize);
    let mut top = reference[s..s + frag].to_vec();
    top[t - s] = b'T';
    let bot = if variant {
        top.clone()
    } else {
        reference[s..s + frag].to_vec()
    };
    let top_name = format!("top_{s}:{ua}+{ub}");
    emit_fastq(fq1, &top_name, &top[0..rl]);
    emit_fastq(fq2, &top_name, &revcomp(&top[frag - rl..frag]));
    placements.push(placed(&top_name, s, s + frag - rl, false));
    if both_strands {
        let bot_name = format!("bot_{s}:{ub}+{ua}");
        emit_fastq(fq1, &bot_name, &revcomp(&bot[frag - rl..frag]));
        emit_fastq(fq2, &bot_name, &bot[0..rl]);
        placements.push(placed(&bot_name, s + frag - rl, s, true));
    }
}

/// Whether a BAM record is on `chr1` (the fixture), not the real-aligner filler.
fn on_chr1(inner: &noodles_sam::alignment::RecordBuf) -> bool {
    inner.reference_sequence_id() == Some(0)
}

/// Read index of genomic position `gpos` in a record, walking POS + CIGAR.
fn read_index_at(inner: &noodles_sam::alignment::RecordBuf, gpos: usize) -> Option<usize> {
    let ref_start = usize::from(inner.alignment_start()?) - 1;
    let (mut ri, mut rp) = (0usize, ref_start);
    for op in inner.cigar().as_ref().iter() {
        use noodles_sam::alignment::record::cigar::op::Kind::*;
        let len = op.len();
        match op.kind() {
            Match | SequenceMatch | SequenceMismatch => {
                if gpos >= rp && gpos < rp + len {
                    return Some(ri + gpos - rp);
                }
                ri += len;
                rp += len;
            }
            Insertion | SoftClip => ri += len,
            Deletion | Skip => rp += len,
            _ => {}
        }
    }
    None
}

/// THE PAIRED-END DUPLEX GATE: each molecule is sequenced as TWO read-pairs (top-strand
/// pair + bottom-strand pair) with the dual UMI swapped in the read NAME, both mapping to
/// the SAME fragment. Run PE with `--five_base_umi_qname --five_base_duplex`. The two
/// pairs of a molecule must collapse into ONE duplex-paired family (keyed by fragment
/// span + canonical dual UMI), and the per-molecule reconciliation must separate the 5mC
/// molecule (methylation) from the homozygous C>T molecule (variant) — proving the PE
/// strand handling (molecule strand for pairing and reconciliation).
#[test]
fn five_base_pe_duplex_groundtruth_pairs_two_pairs_per_molecule_bwamem3() {
    duplex_gate(Gt::BwaMem3);
}

#[cfg(unix)]
#[test]
fn five_base_pe_duplex_groundtruth_pairs_two_pairs_per_molecule_hermetic() {
    duplex_gate(Gt::Hermetic);
}

fn duplex_gate(engine: Gt) {
    if !engine.available() {
        eprintln!("skipping: {engine:?} not available (PE duplex gate)");
        return;
    }
    let reference = gen_reference(900);
    let cpgs = cpg_positions(&reference);
    let (sm, sv) = (100usize, 500usize); // 5mC / variant molecule fragment starts
    let (mut fq1, mut fq2, mut placements) = (Vec::new(), Vec::new(), Vec::new());
    for (s, variant, umis) in [
        (sm, false, ("AACCGGTT", "TTGGCCAA")),
        (sv, true, ("GGGGAAAA", "CCCCTTTT")),
    ] {
        let t = pick_overlap_cpg(&cpgs, s);
        molecule(
            &reference,
            (&mut fq1, &mut fq2, &mut placements),
            s,
            t,
            variant,
            umis,
            true,
        );
    }
    let run = run_gt(
        engine,
        &reference,
        fq1,
        fq2,
        &placements,
        &["--five_base_umi_qname", "--five_base_duplex"],
    );

    let report = fs::read_to_string(run.file("pe.5base_duplex.txt")).unwrap();
    let families: Vec<Vec<String>> = report
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect();
    // Each molecule's two pairs (2 reads each) → one family with members "2+2".
    let paired: Vec<&Vec<String>> = families
        .iter()
        .filter(|f| f.len() >= 8 && f[4] == "2+2")
        .collect();
    assert_eq!(
        paired.len(),
        2,
        "PE: expected 2 duplex-paired families (2+2 members each)\nreport:\n{report}"
    );
    let variant_sites: u32 = paired.iter().map(|f| f[5].parse::<u32>().unwrap()).sum();
    let methyl_sites: u32 = paired.iter().map(|f| f[6].parse::<u32>().unwrap()).sum();
    assert!(
        variant_sites >= 1,
        "PE: the homozygous C>T molecule must flag a variant\nreport:\n{report}"
    );
    assert!(
        methyl_sites >= 1,
        "PE: the 5mC molecule must contribute a methylation site\nreport:\n{report}"
    );
    let fams_with_variant = paired
        .iter()
        .filter(|f| f[5].parse::<u32>().unwrap() >= 1)
        .count();
    assert_eq!(
        fams_with_variant, 1,
        "PE: only the variant molecule should flag a variant\nreport:\n{report}"
    );
    assert!(
        paired
            .iter()
            .any(|f| f[5] == "0" && f[6].parse::<u32>().unwrap() >= 1),
        "PE: the 5mC molecule flags no variant and a methylation site\nreport:\n{report}"
    );
    // Every CpG of a fragment is read by both molecules' pairs, so none lacks the other strand.
    assert!(
        paired.iter().all(|f| f[7] == "0"),
        "PE: no CpG should be undetermined\nreport:\n{report}"
    );
}

/// THE PE CONSENSUS GATE: same two-pairs-per-molecule fixture as the PE duplex gate, but
/// run with `--five_base_consensus`. The PE collapse must emit ONE consensus read per
/// duplex family into `<out>_pe.5base_consensus.bam`, with the 5mC CpG called `Z` and the
/// homozygous C>T CpG masked to `.` (the asymmetric reconciliation over all four reads).
///
/// FAITHFUL duplex fixture (molecule-strand, not coverage-strand): at a `+` CpG only the TOP
/// strand carries that cytosine's methylation, so every OT read shows `T` while every OB read
/// shows `C` (confirmed on real Illumina data); a homozygous C>T variant is on both strands.
#[test]
fn five_base_pe_consensus_groundtruth_collapses_and_masks_variant_bwamem3() {
    consensus_gate(Gt::BwaMem3);
}

#[cfg(unix)]
#[test]
fn five_base_pe_consensus_groundtruth_collapses_and_masks_variant_hermetic() {
    consensus_gate(Gt::Hermetic);
}

fn consensus_gate(engine: Gt) {
    if !engine.available() {
        eprintln!("skipping: {engine:?} not available (PE consensus gate)");
        return;
    }
    let reference = gen_reference(900);
    let cpgs = cpg_positions(&reference);
    let (sm, sv) = (100usize, 500usize);
    let (tm, tv) = (pick_overlap_cpg(&cpgs, sm), pick_overlap_cpg(&cpgs, sv));
    let (mut fq1, mut fq2, mut placements) = (Vec::new(), Vec::new(), Vec::new());
    molecule(
        &reference,
        (&mut fq1, &mut fq2, &mut placements),
        sm,
        tm,
        false,
        ("AACCGGTT", "TTGGCCAA"),
        true,
    );
    molecule(
        &reference,
        (&mut fq1, &mut fq2, &mut placements),
        sv,
        tv,
        true,
        ("GGGGAAAA", "CCCCTTTT"),
        true,
    );
    let run = run_gt(
        engine,
        &reference,
        fq1,
        fq2,
        &placements,
        &["--five_base_umi_qname", "--five_base_consensus"],
    );

    let mut reader =
        bismark::io::BamReader::from_path(&run.file("pe.5base_consensus.bam")).unwrap();
    let (mut n, mut z_at_tm, mut dot_at_tv) = (0usize, false, false);
    // The `-`-strand cytosine of the same CpG dinucleotide sits at tm+1 (a genomic G). It is
    // scored ONLY by the reverse consensus record (GA call); forward-only emission left it
    // uncalled. Assert it is now a real CpG call, proving both strands are emitted.
    let mut minus_called = false;
    for rec in reader.records() {
        let rec = rec.unwrap();
        let inner = rec.inner();
        if !on_chr1(inner) {
            continue;
        }
        n += 1;
        let xm = bismark::io::tags::xm(inner.data()).unwrap();
        if let Some(ri) = read_index_at(inner, tm)
            && xm[ri] == b'Z'
        {
            z_at_tm = true;
        }
        if let Some(ri) = read_index_at(inner, tm + 1)
            && (xm[ri] == b'Z' || xm[ri] == b'z')
        {
            minus_called = true;
        }
        if let Some(ri) = read_index_at(inner, tv) {
            if xm[ri] == b'.' {
                dot_at_tv = true;
            }
            assert_ne!(
                xm[ri], b'Z',
                "PE consensus: C>T variant must not be methylated"
            );
        }
    }
    // Two records per duplex family (forward `+` calls + reverse `-` calls) × 2 molecules.
    assert_eq!(
        n, 4,
        "PE: forward+reverse consensus record per duplex family"
    );
    assert!(
        minus_called,
        "PE consensus: the `-`-strand CpG must be scored by the reverse record"
    );
    assert!(z_at_tm, "PE consensus: the 5mC CpG must be Z");
    assert!(dot_at_tv, "PE consensus: the C>T CpG must be masked ('.')");
}

/// REGRESSION (real-data bug): real Illumina FastQ headers carry a comment after a
/// space, e.g. `@LH00757:...:ANCGTTG+NGGTGTA 1:N:0:GTAACTGAAG+TCNCGACTCC`. The aligner
/// truncates the SAM QNAME at the first whitespace, so the 5-Base lockstep must derive
/// its read identifier the same way (whitespace-truncated) regardless of `--icpc`;
/// otherwise every read desyncs. This gate feeds Illumina-style spaced headers through the
/// 5-Base PE path and asserts it runs and still recovers the methylation calls.
#[test]
fn five_base_groundtruth_illumina_spaced_header_no_desync_bwamem3() {
    spaced_header_gate(Gt::BwaMem3);
}

#[cfg(unix)]
#[test]
fn five_base_groundtruth_illumina_spaced_header_no_desync_hermetic() {
    spaced_header_gate(Gt::Hermetic);
}

fn spaced_header_gate(engine: Gt) {
    if !engine.available() {
        eprintln!("skipping: {engine:?} not available (spaced-header regression)");
        return;
    }
    let reference = gen_reference(900);
    let (read_len, n_reads) = (120usize, 5usize);
    let (plain, truth) = make_methylated_reads(&reference, read_len, n_reads);
    // Rewrite each `@<name>` header line to add the Illumina dual-index comment, producing R1
    // (` 1:N:0:...`) and R2 (` 2:N:0:...`). R2 is the reverse complement of R1 (FR, same span).
    let lines: Vec<&[u8]> = plain.split_inclusive(|&b| b == b'\n').collect();
    let (mut fq1, mut fq2, mut placements) = (Vec::new(), Vec::new(), Vec::new());
    for r in 0..lines.len() / 4 {
        let base = r * 4;
        let name = String::from_utf8_lossy(&lines[base][1..lines[base].len() - 1]).into_owned();
        let seq = &lines[base + 1][..lines[base + 1].len() - 1];
        emit_fastq(
            &mut fq1,
            &format!("{name} 1:N:0:GTAACTGAAG+TCNCGACTCC"),
            seq,
        );
        emit_fastq(
            &mut fq2,
            &format!("{name} 2:N:0:GTAACTGAAG+TCNCGACTCC"),
            &revcomp(seq),
        );
        placements.push(placed(&name, r * read_len, r * read_len, false));
    }
    let run = run_gt(engine, &reference, fq1, fq2, &placements, &[]);

    let mut reader = bismark::io::BamReader::from_path(&run.file("pe.bam")).unwrap();
    let (mut n_r1, mut checked_meth) = (0usize, 0usize);
    for rec in reader.records() {
        let rec = rec.unwrap();
        let inner = rec.inner();
        let flag = u16::from(inner.flags());
        // Only R1 records (R2 XM is in revcomp orientation), on the fixture's contig.
        if flag & 0x80 != 0 || !on_chr1(inner) {
            continue;
        }
        n_r1 += 1;
        // The BAM qname is whitespace-truncated (the comment dropped); matches truth keys.
        let qname = String::from_utf8_lossy(inner.name().unwrap().as_ref()).into_owned();
        assert!(!qname.contains(' '), "qname must not contain the comment");
        let Some(expect) = truth.get(&qname) else {
            continue;
        };
        let xm = bismark::io::tags::xm(inner.data()).unwrap();
        for &(cpg_pos, methylated) in expect {
            if let Some(ri) = read_index_at(inner, cpg_pos)
                && methylated
                && xm[ri] == b'Z'
            {
                checked_meth += 1;
            }
        }
    }
    assert_eq!(
        n_r1,
        truth.len(),
        "spaced-header lockstep desync: expected one R1 record per input pair"
    );
    assert!(
        checked_meth >= 1,
        "should still recover methylated CpGs with spaced headers"
    );
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

/// THE PE GATE: real bwa-mem3 in PAIRED mode. Read pairs are an FR fragment of the
/// reference (R1 = forward 5' end with injected 5mC->T methylation; R2 = revcomp of
/// the 3' end). After `--illumina_5base` PE alignment, every pair yields two records
/// (R1 FLAG 0x40 forward, R2 FLAG 0x80 reverse) and the R1 CpG calls match ground
/// truth at every aligned position (the OT/index-0 inverted call).
#[test]
fn five_base_pe_groundtruth_real_aligner_bwamem3() {
    if !have_bwamem3() {
        eprintln!("skipping: bwa-mem3 gates off (PE ground-truth gate)");
        return;
    }
    let reference = gen_reference(900);
    let (frag, rl, anchor) = (180usize, 90usize, 10usize);
    let cpgs = cpg_positions(&reference);
    let (mut fq1, mut fq2) = (Vec::new(), Vec::new());
    let mut truth: HashMap<String, Vec<(usize, bool)>> = HashMap::new();
    // The driver needs 2 genome bases past each mate, so no fragment ends at the contig end.
    for p in 0..(reference.len() - 2) / frag {
        let start = p * frag;
        let qname = format!("pair{p}");
        // R1: forward 5' end, methylation injected.
        let mut r1 = reference[start..start + rl].to_vec();
        let mut here = Vec::new();
        for (gi, &cpos) in cpgs.iter().enumerate() {
            if cpos >= start + anchor && cpos < start + rl - anchor {
                let methylated = gi % 2 == 0;
                if methylated {
                    r1[cpos - start] = b'T';
                }
                here.push((cpos, methylated));
            }
        }
        truth.insert(qname.clone(), here);
        emit_fastq(&mut fq1, &qname, &r1);
        // R2: reverse-complement of the fragment's 3' end (kept as reference).
        emit_fastq(
            &mut fq2,
            &qname,
            &revcomp(&reference[start + frag - rl..start + frag]),
        );
    }
    let run = run_gt(Gt::BwaMem3, &reference, fq1, fq2, &[], &[]);

    let mut reader = bismark::io::BamReader::from_path(&run.file("pe.bam")).unwrap();
    let (mut r1_records, mut r2_records, mut checked, mut checked_meth) = (0usize, 0usize, 0, 0);
    for rec in reader.records() {
        let rec = rec.unwrap();
        let inner = rec.inner();
        if !on_chr1(inner) {
            continue;
        }
        let flag = u16::from(inner.flags());
        if flag & 0x80 != 0 {
            r2_records += 1;
            continue; // R2 (reverse) — XM is in revcomp orientation; not position-checked here.
        }
        r1_records += 1;
        assert_eq!(flag & 0x10, 0, "R1 of a proper FR pair maps forward");
        let qname = String::from_utf8_lossy(inner.name().unwrap().as_ref()).into_owned();
        let Some(expect) = truth.get(&qname) else {
            continue;
        };
        let xm = bismark::io::tags::xm(inner.data()).unwrap();
        for &(cpg_pos, methylated) in expect {
            if let Some(ri) = read_index_at(inner, cpg_pos) {
                let call = xm[ri];
                if methylated {
                    assert_eq!(call, b'Z', "{qname}: methylated CpG at {cpg_pos} must be Z");
                    checked_meth += 1;
                } else {
                    assert_eq!(
                        call, b'z',
                        "{qname}: unmethylated CpG at {cpg_pos} must be z"
                    );
                }
                checked += 1;
            }
        }
    }
    assert_eq!(r1_records, truth.len(), "every fixture pair aligned");
    assert_eq!(r1_records, r2_records, "one R1 per R2");
    assert!(
        checked >= 3,
        "should validate several R1 CpGs, got {checked}"
    );
    assert!(
        checked_meth >= 1,
        "should confirm at least one methylated R1 CpG through real bwa-mem3 PE"
    );
}

// ===========================================================================
// #787 Illumina 5-Base SPIKE-IN CONTROL gate — lambda (unmethylated) + pUC19
// (CpG-methylated). The 5-Base kit spikes unmethylated lambda + fully-CpG-methylated
// pUC19 into every sample (Illumina GDMC control; DRAGEN reports their conversion in
// methyl_metrics.csv). Their methylation truth is KNOWN (lambda ~0% 5mC, pUC19 ~100%
// CpG 5mC) and the sequences are PUBLIC, so they give a fully REPRODUCIBLE concordance
// gate with NO proprietary data: the pipeline must recover ~0% from lambda and ~100%
// from pUC19 through the core read calls, the duplex consensus, AND the deconvolution.
// This is the concordance gate the experimental modes need to graduate out of preview.
// ===========================================================================

/// A committed public control fixture under repo-root `test_files/`.
/// `None` if absent.
fn control_genome_gz(file: &str) -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test_files")
        .join(file);
    p.exists().then_some(p)
}

/// Load both control sequences, or skip (fail loud in CI — must not pass vacuously).
fn load_controls_or_skip() -> Option<(Vec<u8>, Vec<u8>)> {
    match (
        control_genome_gz("lambda_NC_001416.fa.gz"),
        control_genome_gz("pUC19.fa.gz"),
    ) {
        (Some(l), Some(p)) => Some((load_first_contig_gz(&l), load_first_contig_gz(&p))),
        _ => {
            if std::env::var_os("CI").is_some() {
                panic!(
                    "5-Base control fixtures (lambda/pUC19) missing but $CI is set: the \
                     control concordance gate must not no-op in CI."
                );
            }
            None
        }
    }
}

/// Genome dir with MULTIPLE named contigs (extends [`write_genome`]).
fn write_genome_multi(dir: &Path, contigs: &[(&str, &[u8])]) {
    let mut fa = Vec::new();
    for (name, seq) in contigs {
        fa.extend_from_slice(format!(">{name}\n").as_bytes());
        fa.extend_from_slice(seq);
        fa.push(b'\n');
    }
    fs::write(dir.join("genome.fa"), fa).unwrap();
}

/// Count CpG calls (`Z` meth / `z` unmeth) in a BAM, bucketed by a key derived from the
/// qname (the control of origin). `key` maps a qname to its bucket. Returns
/// `key -> (meth, unmeth)`.
fn count_cpg_keyed<F: Fn(&str) -> String>(bam: &Path, key: F) -> HashMap<String, (u64, u64)> {
    let mut reader = bismark::io::BamReader::from_path(bam).unwrap();
    let mut out: HashMap<String, (u64, u64)> = HashMap::new();
    for rec in reader.records() {
        let rec = rec.unwrap();
        let inner = rec.inner();
        let qname = String::from_utf8_lossy(inner.name().unwrap().as_ref()).into_owned();
        let xm = bismark::io::tags::xm(inner.data()).unwrap();
        let e = out.entry(key(&qname)).or_default();
        for &c in xm.iter() {
            match c {
                b'Z' => e.0 += 1,
                b'z' => e.1 += 1,
                _ => {}
            }
        }
    }
    out
}

fn pct_meth(counts: Option<&(u64, u64)>) -> (f64, u64) {
    let (m, u) = counts.copied().unwrap_or((0, 0));
    let tot = m + u;
    (
        if tot == 0 {
            0.0
        } else {
            100.0 * m as f64 / tot as f64
        },
        tot,
    )
}

/// CORE control gate: the per-read 5-Base call must recover ~0% 5mC from unmethylated
/// lambda and ~100% CpG 5mC from CpG-methylated pUC19 (the kit's spike-in truth).
/// Uses PE duplex emission (same as the consensus gate) to exercise the PE path.
#[test]
fn five_base_controls_core_recovers_lambda_and_puc19_bwamem3() {
    controls_core(Engine::BwaMem3);
}

/// The documented default command: `--illumina_5base` with no engine flag and no index flag.
#[test]
fn five_base_controls_core_recovers_lambda_and_puc19_default_bwamem3() {
    controls_core(Engine::BwaMem3Default);
}

fn controls_core(engine: Engine) {
    if !engine.available() {
        eprintln!("skipping: {engine:?} not available (control gate)");
        return;
    }
    let Some((lambda, puc19)) = load_controls_or_skip() else {
        eprintln!("skipping: control fixtures absent");
        return;
    };
    // A sub-region of lambda is plenty of CpGs; pUC19 in full.
    let lambda_sub = &lambda[..6000.min(lambda.len())];
    let genome = TempDir::new().unwrap();
    write_genome_multi(genome.path(), &[("lambda", lambda_sub), ("pUC19", &puc19)]);

    // Emit PE duplex pairs (OT + OB) for each control (mirrors the consensus gate).
    let (mut fq1, mut fq2) = (Vec::new(), Vec::new());
    emit_pe_control_duplex(&mut fq1, &mut fq2, "lam", lambda_sub, false, 60, 40);
    emit_pe_control_duplex(&mut fq1, &mut fq2, "puc", &puc19, true, 30, 40);
    let (r1, r2) = (genome.path().join("r1.fq"), genome.path().join("r2.fq"));
    fs::write(&r1, &fq1).unwrap();
    fs::write(&r2, &fq2).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();
    let engine_flag = engine.prepare(genome.path());

    // Core path: no --five_base_consensus / --five_base_duplex.
    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .args(engine_flag)
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "aligned with bwa-mem3 to the UNCONVERTED",
        ))
        .stderr(predicate::str::contains("default engine").not());

    // Bucket by qname prefix (`lam`/`puc`) = the control of origin.
    let bam = outdir
        .path()
        .join(format!("r1_bismark_{}_pe.bam", engine.token()));
    let counts = count_cpg_keyed(&bam, |q| q.split('_').next().unwrap_or("").to_string());
    let (lam_pct, lam_n) = pct_meth(counts.get("lam"));
    let (puc_pct, puc_n) = pct_meth(counts.get("puc"));
    assert!(
        lam_n >= 50 && puc_n >= 50,
        "controls should yield many CpG calls (lambda={lam_n}, pUC19={puc_n})"
    );
    assert!(
        lam_pct < 2.0,
        "unmethylated lambda must read ~0% 5mC, got {lam_pct:.1}% ({lam_n} CpGs)"
    );
    assert!(
        puc_pct > 95.0,
        "CpG-methylated pUC19 must read ~100% 5mC, got {puc_pct:.1}% ({puc_n} CpGs)"
    );
}

/// Append PE duplex pairs (OT + OB with swapped dual UMI in the qname) tiled across `seq`.
/// A fully-CpG-methylated control shows T at every `+` CpG on the TOP strand and A at every
/// `-` CpG (a genomic G of a CpG) on the BOTTOM strand; unmethylated leaves both intact.
/// Mirrors the faithful PE-duplex `molecule` fixture.
fn emit_pe_control_duplex(
    fq1: &mut Vec<u8>,
    fq2: &mut Vec<u8>,
    prefix: &str,
    seq: &[u8],
    methylated: bool,
    step: usize,
    n_frags: usize,
) {
    const FRAG: usize = 140;
    const RL: usize = 100;
    let (frag, rl) = (FRAG, RL);
    let emit = |fq: &mut Vec<u8>, name: &str, bytes: &[u8]| {
        fq.extend_from_slice(format!("@{name}\n").as_bytes());
        fq.extend_from_slice(bytes);
        fq.extend_from_slice(b"\n+\n");
        fq.extend_from_slice(&vec![b'I'; bytes.len()]);
        fq.push(b'\n');
    };
    let umis = ["AACCGGTT", "GGTTCCAA", "TTGGAACC", "CCAATTGG"];
    let (mut s, mut k) = (0usize, 0usize);
    while s + frag <= seq.len() && k < n_frags {
        let frag_seq = &seq[s..s + frag];
        let (mut top, mut bot) = (frag_seq.to_vec(), frag_seq.to_vec());
        if methylated {
            for i in 0..frag {
                if frag_seq[i] == b'C' && i + 1 < frag && frag_seq[i + 1] == b'G' {
                    top[i] = b'T'; // + CpG 5mC -> T on the top strand
                }
                if frag_seq[i] == b'G' && i > 0 && frag_seq[i - 1] == b'C' {
                    bot[i] = b'A'; // - CpG 5mC -> A in +ref-forward (bottom strand)
                }
            }
        }
        let (ot_r1, ot_r2) = (top[0..rl].to_vec(), revcomp(&top[frag - rl..frag]));
        let (ob_r1, ob_r2) = (revcomp(&bot[frag - rl..frag]), bot[0..rl].to_vec());
        let (ua, ub) = (umis[k % umis.len()], umis[(k + 1) % umis.len()]);
        emit(fq1, &format!("{prefix}_{k}:{ua}+{ub}"), &ot_r1);
        emit(fq2, &format!("{prefix}_{k}:{ua}+{ub}"), &ot_r2);
        emit(fq1, &format!("{prefix}_{k}:{ub}+{ua}"), &ob_r1);
        emit(fq2, &format!("{prefix}_{k}:{ub}+{ua}"), &ob_r2);
        s += step;
        k += 1;
    }
}

/// CONSENSUS control gate: the duplex collapse must PRESERVE the controls' methylation
/// state — ~0% 5mC for unmethylated lambda, ~100% for CpG-methylated pUC19, on BOTH strands.
/// This is the exact property validated against DRAGEN, but here against KNOWN truth.
#[test]
fn five_base_controls_consensus_preserves_methylation_state_bwamem3() {
    controls_consensus(Engine::BwaMem3);
}

fn controls_consensus(engine: Engine) {
    if !engine.available() {
        eprintln!("skipping: {engine:?} not available (consensus control gate)");
        return;
    }
    let Some((lambda, puc19)) = load_controls_or_skip() else {
        eprintln!("skipping: control fixtures absent");
        return;
    };
    let lambda_sub = &lambda[..6000.min(lambda.len())];
    let genome = TempDir::new().unwrap();
    write_genome_multi(genome.path(), &[("lambda", lambda_sub), ("pUC19", &puc19)]);

    let (mut fq1, mut fq2) = (Vec::new(), Vec::new());
    emit_pe_control_duplex(&mut fq1, &mut fq2, "lam", lambda_sub, false, 70, 40);
    emit_pe_control_duplex(&mut fq1, &mut fq2, "puc", &puc19, true, 50, 40);
    let (r1, r2) = (genome.path().join("r1.fq"), genome.path().join("r2.fq"));
    fs::write(&r1, &fq1).unwrap();
    fs::write(&r2, &fq2).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();
    let engine_flag = engine.prepare(genome.path());

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .args(engine_flag)
        .arg("--five_base_umi_qname")
        .arg("--five_base_consensus")
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .assert()
        .success();

    // Consensus qname = `dpx:{chrom}:{start}-{end}:{umi}` → bucket by chrom.
    let counts = count_cpg_keyed(
        &outdir.path().join(format!(
            "r1_bismark_{}_pe.5base_consensus.bam",
            engine.token()
        )),
        |q| q.split(':').nth(1).unwrap_or("").to_string(),
    );
    let (lam_pct, lam_n) = pct_meth(counts.get("lambda"));
    let (puc_pct, puc_n) = pct_meth(counts.get("pUC19"));
    assert!(
        lam_n >= 20 && puc_n >= 20,
        "consensus should call CpGs for both controls (lambda={lam_n}, pUC19={puc_n})"
    );
    assert!(
        lam_pct < 5.0,
        "unmethylated lambda consensus must be ~0% 5mC, got {lam_pct:.1}% ({lam_n} CpGs)"
    );
    assert!(
        puc_pct > 90.0,
        "CpG-methylated pUC19 consensus must be ~100% 5mC, got {puc_pct:.1}% ({puc_n} CpGs)"
    );
}

/// DECONVOLUTION control gate: the variant-vs-5mC deconvolution must call NO `variant` on
/// the controls (they carry 5mC, not C>T SNVs), and recover pUC19 as ~100% methylation /
/// lambda as ~0%. Proves the deconvolution's specificity (no false variants) on known truth.
#[test]
fn five_base_controls_deconvolution_no_false_variants_bwamem3() {
    controls_deconvolution(Engine::BwaMem3);
}

fn controls_deconvolution(engine: Engine) {
    if !engine.available() {
        eprintln!("skipping: {engine:?} not available (deconvolution control gate)");
        return;
    }
    let Some((lambda, puc19)) = load_controls_or_skip() else {
        eprintln!("skipping: control fixtures absent");
        return;
    };
    let lambda_sub = &lambda[..6000.min(lambda.len())];
    let genome = TempDir::new().unwrap();
    write_genome_multi(genome.path(), &[("lambda", lambda_sub), ("pUC19", &puc19)]);

    let (mut fq1, mut fq2) = (Vec::new(), Vec::new());
    emit_pe_control_duplex(&mut fq1, &mut fq2, "lam", lambda_sub, false, 35, 80);
    emit_pe_control_duplex(&mut fq1, &mut fq2, "puc", &puc19, true, 35, 80);
    let (r1, r2) = (genome.path().join("r1.fq"), genome.path().join("r2.fq"));
    fs::write(&r1, &fq1).unwrap();
    fs::write(&r2, &fq2).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();
    let engine_flag = engine.prepare(genome.path());

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .args(engine_flag)
        .arg("--five_base_umi_qname")
        .arg("--five_base_deconvolution")
        .arg("-1")
        .arg(&r1)
        .arg("-2")
        .arg(&r2)
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .assert()
        .success();

    // Locate the deconvolution report (PE/SE naming-robust).
    let report_path = fs::read_dir(outdir.path())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("5base_deconvolution.txt"))
        })
        .expect("deconvolution report written");
    let report = fs::read_to_string(&report_path).unwrap();

    // columns: chrom\tpos\tstrand\tverdict\tmethylated\ttotal\tpercent
    let (mut variants, mut puc_meth, mut lam_meth) = (0u32, Vec::<f64>::new(), Vec::<f64>::new());
    for line in report.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 7 {
            continue;
        }
        if f[3] == "variant" {
            variants += 1;
        }
        if f[3] == "methylation"
            && let Ok(pct) = f[6].parse::<f64>()
        {
            match f[0] {
                "pUC19" => puc_meth.push(pct),
                "lambda" => lam_meth.push(pct),
                _ => {}
            }
        }
    }
    assert_eq!(
        variants, 0,
        "controls carry 5mC, NOT C>T variants — deconvolution must call zero 'variant', got {variants}"
    );
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    assert!(
        puc_meth.len() >= 10 && mean(&puc_meth) > 90.0,
        "pUC19 deconvolution methylation must be ~100%, got {:.1}% over {} CpGs",
        mean(&puc_meth),
        puc_meth.len()
    );
    assert!(
        lam_meth.len() >= 10 && mean(&lam_meth) < 5.0,
        "lambda deconvolution methylation must be ~0%, got {:.1}% over {} CpGs",
        mean(&lam_meth),
        lam_meth.len()
    );
}

/// `[#1104]` THE IN-RUN SIMPLEX GATE. Every other `--five_base_emit_multiplicity` test
/// drives the standalone `--five_base_consensus_from_bam` entry, so the in-run call site
/// — its `EmitPaths` wiring and the `_pe.5base_simplex.bam` suffix derivation — had no
/// coverage. The fixture holds one duplex molecule (both strands, distinct UMI) and one
/// SIMPLEX molecule (top strand only), so `both` mode must split them across the two
/// BAMs. Also cross-checks the emitted simplex count against the duplex pass's own
/// `singletons` figure, which is the independent count of the same families.
#[test]
fn five_base_in_run_both_mode_splits_duplex_and_simplex_bwamem3() {
    both_mode_gate(Gt::BwaMem3);
}

#[cfg(unix)]
#[test]
fn five_base_in_run_both_mode_splits_duplex_and_simplex_hermetic() {
    both_mode_gate(Gt::Hermetic);
}

fn both_mode_gate(engine: Gt) {
    if !engine.available() {
        eprintln!("skipping: {engine:?} not available (in-run simplex gate)");
        return;
    }
    let reference = gen_reference(900);
    let cpgs = cpg_positions(&reference);
    let (s_dup, s_spx) = (100usize, 500usize);
    let (mut fq1, mut fq2, mut placements) = (Vec::new(), Vec::new(), Vec::new());
    // `both_strands = false` emits the TOP strand alone: one molecule strand, so PASS 1
    // sees ot>0 && ob==0 and the family is simplex.
    for (s, umis, both) in [
        (s_dup, ("AACCGGTT", "TTGGCCAA"), true),
        (s_spx, ("GGGGAAAA", "CCCCTTTT"), false),
    ] {
        let t = pick_overlap_cpg(&cpgs, s);
        molecule(
            &reference,
            (&mut fq1, &mut fq2, &mut placements),
            s,
            t,
            false,
            umis,
            both,
        );
    }
    let run = run_gt(
        engine,
        &reference,
        fq1,
        fq2,
        &placements,
        &[
            "--five_base_umi_qname",
            "--five_base_consensus",
            "--five_base_emit_multiplicity",
            "both",
        ],
    );

    // The in-run suffix derivation is what this test exists to pin.
    let dpx_bam = run.file("pe.5base_consensus.bam");
    let spx_bam = run.file("pe.5base_simplex.bam");
    assert!(dpx_bam.exists(), "in-run `both` must write the duplex BAM");
    assert!(
        spx_bam.exists(),
        "in-run `both` must write _pe.5base_simplex.bam beside it; dir held: {:?}",
        fs::read_dir(run.out.path())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect::<Vec<_>>()
    );

    // (records on chr1, all records)
    let count = |p: &Path| -> (usize, usize) {
        let mut r = bismark::io::BamReader::from_path(p).unwrap();
        let (mut fixture, mut all) = (0usize, 0usize);
        for rec in r.records() {
            let rec = rec.unwrap(); // a malformed record must fail the test, not be counted
            all += 1;
            fixture += usize::from(on_chr1(rec.inner()));
        }
        (fixture, all)
    };
    assert_eq!(
        count(&dpx_bam),
        (2, 2),
        "the duplex molecule emits a forward AND a reverse consensus record"
    );
    let (spx_fixture, spx_all) = count(&spx_bam);
    assert_eq!(
        spx_fixture, 1,
        "the simplex molecule emits exactly one record, on its own strand"
    );

    // The duplex pass counts the same single-strand families independently.
    let txt = fs::read_to_string(run.file("pe.5base_duplex.txt")).unwrap();
    let singletons: u64 = txt
        .lines()
        .find(|l| l.starts_with("# families"))
        .and_then(|l| l.rsplit_once(' ').map(|(_, n)| n.to_string()))
        .expect("the duplex report's families line")
        .parse()
        .expect("singleton count");
    assert_eq!(
        singletons, spx_all as u64,
        "the duplex pass must count one single-strand family per simplex record; report:\n{txt}"
    );
    if engine == Gt::Hermetic {
        assert_eq!(singletons, 1, "report:\n{txt}");
    }
    assert!(
        run.stderr
            .contains(&format!("of {singletons} single-strand family(ies)")),
        "the simplex report's family total must agree with the duplex pass's singletons \
         ({singletons}); got:\n{}",
        run.stderr
    );
}
