//! #787 Illumina 5-Base SYNTHETIC GROUND-TRUTH gate, using the REAL minimap2.
//!
//! A true "concordance vs DRAGEN" gate is impossible without DRAGEN (proprietary
//! FPGA, no reference output) and without a public raw 5-Base dataset. This is the
//! achievable, stronger substitute: we synthesize reads from a known reference with
//! a KNOWN methylation pattern (5mC -> T at chosen CpGs), align them with the REAL
//! `minimap2` to the UNCONVERTED genome via `--illumina_5base`, and assert the
//! pipeline recovers the correct methylated (`Z`) vs unmethylated (`z`) CpG call at
//! every aligned CpG. Validates the whole FROM-FASTQ chain on a real aligner (not
//! the hermetic fake in `cli.rs`).
//!
//! The check is PER GENOMIC POSITION (walks the BAM POS+CIGAR), so it is robust to
//! minimap2 soft-clipping read ends (a clipped CpG is simply not checked) while
//! still catching any wrong-polarity call. Gated on `minimap2` being on PATH; absent
//! -> no-op (CI without minimap2 stays green).

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use tempfile::TempDir;

/// The real downloaded reference genome (`test_files/NC_010473.fa.gz`, E. coli K-12),
/// resolved relative to this crate. `None` if absent (the gate then no-ops).
fn real_genome_gz() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test_files/NC_010473.fa.gz");
    p.exists().then_some(p)
}

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
    Command::cargo_bin("bismark_rs").unwrap()
}

fn have_minimap2() -> bool {
    StdCommand::new("minimap2")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Deterministic pseudo-random ACGT reference (fixed LCG → stable CpG layout).
fn gen_reference(n: usize) -> Vec<u8> {
    let bases = [b'A', b'C', b'G', b'T'];
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
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

/// Genome dir: raw `genome.fa` (the 5-Base path aligns against it) + dummy `BS_*.mmi`
/// so index discovery passes (the 5-Base path passes the FASTA directly, not the mmi).
fn write_genome(dir: &Path, reference: &[u8]) {
    let ct = dir.join("Bisulfite_Genome").join("CT_conversion");
    let ga = dir.join("Bisulfite_Genome").join("GA_conversion");
    fs::create_dir_all(&ct).unwrap();
    fs::create_dir_all(&ga).unwrap();
    fs::write(ct.join("BS_CT.mmi"), b"x").unwrap();
    fs::write(ga.join("BS_GA.mmi"), b"x").unwrap();
    let mut fa = Vec::new();
    fa.extend_from_slice(b">chr1\n");
    fa.extend_from_slice(reference);
    fa.push(b'\n');
    fs::write(dir.join("genome.fa"), fa).unwrap();
}

/// Forward 5-Base reads (exact reference slices) with a KNOWN methylation pattern:
/// every even-indexed CpG inside a read's non-anchored core is "methylated" (its
/// cytosine written `T`, simulating 5mC->T); all other CpGs keep `C`. The 12 bp
/// flanks are kept exact so minimap2 anchors and does not soft-clip the core.
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

/// THE GATE: real minimap2, known methylation in, recovered methylation out — checked
/// at every aligned CpG position (no wrong-polarity call; most CpGs recovered).
#[test]
fn five_base_groundtruth_real_minimap2_recovers_known_methylation() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (real-aligner ground-truth gate)");
        return;
    }

    let reference = gen_reference(600);
    let genome = TempDir::new().unwrap();
    write_genome(genome.path(), &reference);
    let (fastq, truth) = make_methylated_reads(&reference, 120, 5);
    let total_cpgs: usize = truth.values().map(|v| v.len()).sum();
    let total_meth: usize = truth.values().flatten().filter(|(_, m)| *m).count();
    assert!(
        total_meth >= 3 && total_cpgs - total_meth >= 3,
        "fixture should exercise several CpGs each way (me={total_meth}, total={total_cpgs})"
    );
    let read = genome.path().join("reads.fq");
    fs::write(&read, &fastq).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    let bam = outdir.path().join("reads_bismark_mm2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let mut checked = 0usize;
    let mut checked_meth = 0usize;
    let mut n_records = 0usize;

    for rec in reader.records() {
        let rec = rec.unwrap();
        n_records += 1;
        let inner = rec.inner();
        // Forward reads only (FLAG 0) → XM is in read order, aligned with the read.
        assert_eq!(
            u16::from(inner.flags()) & 0x10,
            0,
            "fixture reads are forward"
        );
        let qname = String::from_utf8_lossy(inner.name().unwrap().as_ref()).into_owned();
        let expect = truth
            .get(&qname)
            .expect("every record matches a fixture read");
        let xm = bismark_io::tags::xm(inner.data()).unwrap();
        // 1-based POS → 0-based reference start.
        let ref_start = usize::from(inner.alignment_start().unwrap()) - 1;

        // Walk the CIGAR to map each read index → genomic position (M/=/X consume
        // both; I/S consume read only; D/N consume reference only).
        let mut genomic_at = HashMap::<usize, usize>::new(); // read_idx -> genomic pos
        let (mut read_idx, mut ref_pos) = (0usize, ref_start);
        for op in inner.cigar().as_ref().iter() {
            let len = op.len();
            use noodles_sam::alignment::record::cigar::op::Kind::*;
            match op.kind() {
                Match | SequenceMatch | SequenceMismatch => {
                    for _ in 0..len {
                        genomic_at.insert(read_idx, ref_pos);
                        read_idx += 1;
                        ref_pos += 1;
                    }
                }
                Insertion | SoftClip => read_idx += len,
                Deletion | Skip => ref_pos += len,
                _ => {}
            }
        }
        // Invert read_idx -> genomic into genomic -> read_idx for lookup.
        let read_at: HashMap<usize, usize> = genomic_at.iter().map(|(&r, &g)| (g, r)).collect();

        for &(cpg_pos, methylated) in expect {
            if let Some(&ri) = read_at.get(&cpg_pos) {
                let call = xm[ri];
                if methylated {
                    assert_eq!(
                        call, b'Z',
                        "{qname}: methylated CpG at {cpg_pos} must be Z, got {}",
                        call as char
                    );
                    checked_meth += 1;
                } else {
                    assert_eq!(
                        call, b'z',
                        "{qname}: unmethylated CpG at {cpg_pos} must be z, got {}",
                        call as char
                    );
                }
                checked += 1;
            }
        }
    }

    assert!(
        n_records >= 1,
        "minimap2 should have mapped the synthetic reads"
    );
    // Most CpGs are recovered (a few near soft-clipped ends may drop out) and at least
    // several methylated ones were positively confirmed through the real aligner.
    assert!(
        checked * 10 >= total_cpgs * 7,
        "recovered too few CpG calls: {checked}/{total_cpgs}"
    );
    assert!(
        checked_meth >= 3,
        "should positively confirm several methylated (Z) CpGs, got {checked_meth}"
    );
}

/// THE DECONVOLUTION GATE: a homozygous C>T variant CpG vs a methylated CpG, both
/// covered by OT (forward) and OB (reverse) reads via real minimap2, then run
/// `--illumina_5base --five_base_deconvolution`. The variant CpG (cytosine gone on
/// BOTH strands → OB reads show `T`) must be called `variant`; the methylated CpG (OT
/// reads `T` from 5mC, OB reads intact `C`) must be called `methylation`. This proves
/// the SNP-aware caller distinguishes the two through the whole real pipeline.
#[test]
fn five_base_deconvolution_groundtruth_variant_vs_methylation() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (deconvolution ground-truth gate)");
        return;
    }
    let reference = gen_reference(450);
    let genome = TempDir::new().unwrap();
    write_genome(genome.path(), &reference);
    let cpgs = cpg_positions(&reference);
    // Two CpGs with room for a 100 bp window centred on each, well separated.
    let pick = |around: usize| -> usize {
        *cpgs
            .iter()
            .filter(|&&c| c >= 60 && c + 60 < reference.len())
            .min_by_key(|&&c| c.abs_diff(around))
            .expect("a usable CpG near the target")
    };
    let tv = pick(150); // variant CpG
    let tm = pick(330); // methylation CpG
    assert_ne!(tv, tm);

    let half = 50usize;
    let mut fq = Vec::new();
    let mut emit = |name: &str, bytes: &[u8]| {
        fq.extend_from_slice(format!("@{name}\n").as_bytes());
        fq.extend_from_slice(bytes);
        fq.extend_from_slice(b"\n+\n");
        fq.extend_from_slice(&vec![b'I'; bytes.len()]);
        fq.push(b'\n');
    };
    // Window around a target, with the target C optionally converted to T.
    let window = |t: usize, convert_target: bool| -> Vec<u8> {
        let mut w = reference[t - half..t + half].to_vec();
        if convert_target {
            w[half] = b'T'; // the target C → T
        }
        w
    };
    for i in 0..4 {
        // Variant CpG: BOTH strands carry the C→T (cytosine genuinely gone).
        emit(&format!("v_ot_{i}"), &window(tv, true));
        emit(&format!("v_ob_{i}"), &revcomp(&window(tv, true)));
        // Methylation CpG: OT carries 5mC (C→T); OB strand is intact (no conversion).
        emit(&format!("m_ot_{i}"), &window(tm, true));
        emit(&format!("m_ob_{i}"), &revcomp(&window(tm, false)));
    }
    let read = genome.path().join("reads.fq");
    fs::write(&read, &fq).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .arg("--five_base_deconvolution")
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    let report = fs::read_to_string(
        outdir
            .path()
            .join("reads_bismark_mm2.5base_deconvolution.txt"),
    )
    .unwrap();
    // Report positions are 1-based; the target C is the genomic position tv/tm.
    let verdict_at = |pos1: usize| -> Option<String> {
        report.lines().find_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() >= 4 && f[1] == pos1.to_string()).then(|| f[3].to_string())
        })
    };
    assert_eq!(
        verdict_at(tv + 1).as_deref(),
        Some("variant"),
        "the homozygous C>T CpG must deconvolute to a variant\nreport:\n{report}"
    );
    assert_eq!(
        verdict_at(tm + 1).as_deref(),
        Some("methylation"),
        "the 5mC CpG must stay methylation\nreport:\n{report}"
    );
}

/// THE DUPLEX GATE: two molecules, each sequenced as one OT (forward) read + one OB
/// (reverse) read carrying the SWAPPED (reverse-complement) UMI of the nonrandom-duplex
/// pair, both prefixed with an inline UMI. Molecule A is a 5mC CpG (OT shows T, OB
/// intact C); molecule B is a homozygous C>T variant CpG (both strands show T). After
/// `--illumina_5base --five_base_umi_len 8 --five_base_duplex`, the duplex report must
/// pair each molecule's two strands into ONE family and reconcile per molecule: the 5mC
/// family has a methylated call; the variant family flags a variant site. Proves the
/// family pairing (span + canonical swapped UMI) and per-molecule reconciliation run
/// through the whole real-minimap2 pipeline.
#[test]
fn five_base_duplex_groundtruth_pairs_strands_and_reconciles() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (duplex ground-truth gate)");
        return;
    }
    let reference = gen_reference(450);
    let genome = TempDir::new().unwrap();
    write_genome(genome.path(), &reference);
    let cpgs = cpg_positions(&reference);
    let pick = |around: usize| -> usize {
        *cpgs
            .iter()
            .filter(|&&c| c >= 60 && c + 60 < reference.len())
            .min_by_key(|&&c| c.abs_diff(around))
            .expect("a usable CpG near the target")
    };
    let tm = pick(150); // methylation molecule's target CpG
    let tv = pick(330); // variant molecule's target CpG
    assert_ne!(tm, tv);

    let half = 50usize;
    let window = |t: usize, convert_target: bool| -> Vec<u8> {
        let mut w = reference[t - half..t + half].to_vec();
        if convert_target {
            w[half] = b'T';
        }
        w
    };
    let mut fq = Vec::new();
    let mut emit = |name: &str, umi: &[u8], core: &[u8]| {
        let mut read = umi.to_vec();
        read.extend_from_slice(core);
        fq.extend_from_slice(format!("@{name}\n").as_bytes());
        fq.extend_from_slice(&read);
        fq.extend_from_slice(b"\n+\n");
        fq.extend_from_slice(&vec![b'I'; read.len()]);
        fq.push(b'\n');
    };
    // Distinct top-strand UMIs per molecule; each molecule's OB carries the revcomp UMI.
    let umi_m = b"AACCGGTT";
    let umi_v = b"TTGGCCAA";
    // Molecule A — 5mC: OT window has C->T at target; OB strand intact (revcomp of the
    // unconverted window). UMIs are swapped (OB = revcomp(OT UMI)).
    emit("m_ot", umi_m, &window(tm, true));
    emit("m_ob", &revcomp(umi_m), &revcomp(&window(tm, false)));
    // Molecule B — homozygous C>T variant: BOTH strands carry T at the target.
    emit("v_ot", umi_v, &window(tv, true));
    emit("v_ob", &revcomp(umi_v), &revcomp(&window(tv, true)));

    let read = genome.path().join("reads.fq");
    fs::write(&read, &fq).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .arg("--five_base_umi_len")
        .arg("8")
        .arg("--five_base_duplex")
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    let report =
        fs::read_to_string(outdir.path().join("reads_bismark_mm2.5base_duplex.txt")).unwrap();
    // Parse the per-family rows (skip `#` comments). Columns:
    // chrom start end umi members variant methylation undetermined
    let families: Vec<Vec<String>> = report
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect();
    // At least two duplex-paired families, each with a "1+1" member count (one OT, one OB).
    let paired: Vec<&Vec<String>> = families
        .iter()
        .filter(|f| f.len() >= 8 && f[4] == "1+1")
        .collect();
    assert!(
        paired.len() >= 2,
        "expected >=2 duplex-paired families (one per molecule)\nreport:\n{report}"
    );
    let variant_sites: u32 = paired.iter().map(|f| f[5].parse::<u32>().unwrap()).sum();
    let methyl_sites: u32 = paired.iter().map(|f| f[6].parse::<u32>().unwrap()).sum();
    assert!(
        variant_sites >= 1,
        "the homozygous C>T molecule must contribute a variant site\nreport:\n{report}"
    );
    assert!(
        methyl_sites >= 1,
        "the 5mC molecule must contribute a methylation site\nreport:\n{report}"
    );
    // Exactly one family carries the variant (the other molecule is intact at its target).
    let fams_with_variant = paired
        .iter()
        .filter(|f| f[5].parse::<u32>().unwrap() >= 1)
        .count();
    assert_eq!(
        fams_with_variant, 1,
        "only the variant molecule should flag a variant\nreport:\n{report}"
    );
}

/// THE QNAME-UMI DUPLEX GATE: like the duplex gate, but the dual UMI lives in the READ
/// NAME (`...:A+B`, the real Illumina 5-Base layout) instead of inline, with the duplex
/// partner carrying the halves swapped (`B+A`). Run with `--five_base_umi_qname`; the two
/// strands of each molecule must still pair into one family (canonical dual-UMI collapses
/// the swap) and the per-molecule verdicts must separate methylation from variant.
#[test]
fn five_base_duplex_groundtruth_qname_umi_pairs_strands() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (qname-UMI duplex gate)");
        return;
    }
    let reference = gen_reference(450);
    let genome = TempDir::new().unwrap();
    write_genome(genome.path(), &reference);
    let cpgs = cpg_positions(&reference);
    let pick = |around: usize| -> usize {
        *cpgs
            .iter()
            .filter(|&&c| c >= 60 && c + 60 < reference.len())
            .min_by_key(|&&c| c.abs_diff(around))
            .expect("a usable CpG near the target")
    };
    let tm = pick(150);
    let tv = pick(330);
    assert_ne!(tm, tv);

    let half = 50usize;
    let window = |t: usize, convert_target: bool| -> Vec<u8> {
        let mut w = reference[t - half..t + half].to_vec();
        if convert_target {
            w[half] = b'T';
        }
        w
    };
    let mut fq = Vec::new();
    // The dual UMI is the qname tail `:A+B`; the OB partner carries `B+A`. No inline UMI.
    let mut emit = |name: &str, umi_tail: &str, core: &[u8]| {
        fq.extend_from_slice(format!("@{name}:{umi_tail}\n").as_bytes());
        fq.extend_from_slice(core);
        fq.extend_from_slice(b"\n+\n");
        fq.extend_from_slice(&vec![b'I'; core.len()]);
        fq.push(b'\n');
    };
    emit("m_ot", "AACCGGTT+TTGGCCAA", &window(tm, true));
    emit("m_ob", "TTGGCCAA+AACCGGTT", &revcomp(&window(tm, false)));
    emit("v_ot", "GGGGAAAA+CCCCTTTT", &window(tv, true));
    emit("v_ob", "CCCCTTTT+GGGGAAAA", &revcomp(&window(tv, true)));

    let read = genome.path().join("reads.fq");
    fs::write(&read, &fq).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .arg("--five_base_umi_qname")
        .arg("--five_base_duplex")
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    let report =
        fs::read_to_string(outdir.path().join("reads_bismark_mm2.5base_duplex.txt")).unwrap();
    let families: Vec<Vec<String>> = report
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| l.split('\t').map(str::to_string).collect())
        .collect();
    let paired: Vec<&Vec<String>> = families
        .iter()
        .filter(|f| f.len() >= 8 && f[4] == "1+1")
        .collect();
    assert!(
        paired.len() >= 2,
        "qname-UMI: expected >=2 duplex-paired families\nreport:\n{report}"
    );
    let variant_sites: u32 = paired.iter().map(|f| f[5].parse::<u32>().unwrap()).sum();
    assert!(
        variant_sites >= 1,
        "qname-UMI: the C>T molecule must flag a variant\nreport:\n{report}"
    );
    // The canonical dual UMI is recorded (collapsed `A+B`/`B+A` into one key).
    assert!(
        report.contains("AACCGGTT+TTGGCCAA") || report.contains("TTGGCCAA+AACCGGTT"),
        "qname-UMI: the family key should carry the canonical dual UMI\nreport:\n{report}"
    );
}

/// THE CONSENSUS GATE: the same two duplex molecules (one 5mC, one homozygous C>T),
/// run with `--illumina_5base --five_base_umi_len 8 --five_base_consensus`. The collapse
/// must emit ONE consensus read per duplex family into `<out>.5base_consensus.bam`, and
/// the consensus methylation call must be Z at the 5mC target CpG and `.` (masked, NOT
/// methylated) at the homozygous C>T variant CpG — the asymmetric 5mC>T reconciliation
/// through the whole real-minimap2 pipeline.
#[test]
fn five_base_consensus_groundtruth_collapses_and_masks_variant() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (consensus ground-truth gate)");
        return;
    }
    let reference = gen_reference(450);
    let genome = TempDir::new().unwrap();
    write_genome(genome.path(), &reference);
    let cpgs = cpg_positions(&reference);
    let pick = |around: usize| -> usize {
        *cpgs
            .iter()
            .filter(|&&c| c >= 60 && c + 60 < reference.len())
            .min_by_key(|&&c| c.abs_diff(around))
            .expect("a usable CpG near the target")
    };
    let tm = pick(150); // 5mC molecule
    let tv = pick(330); // homozygous C>T variant molecule
    assert_ne!(tm, tv);

    let half = 50usize;
    let window = |t: usize, convert_target: bool| -> Vec<u8> {
        let mut w = reference[t - half..t + half].to_vec();
        if convert_target {
            w[half] = b'T';
        }
        w
    };
    let mut fq = Vec::new();
    let mut emit = |name: &str, umi: &[u8], core: &[u8]| {
        let mut read = umi.to_vec();
        read.extend_from_slice(core);
        fq.extend_from_slice(format!("@{name}\n").as_bytes());
        fq.extend_from_slice(&read);
        fq.extend_from_slice(b"\n+\n");
        fq.extend_from_slice(&vec![b'I'; read.len()]);
        fq.push(b'\n');
    };
    let umi_m = b"AACCGGTT";
    let umi_v = b"TTGGCCAA";
    emit("m_ot", umi_m, &window(tm, true));
    emit("m_ob", &revcomp(umi_m), &revcomp(&window(tm, false)));
    emit("v_ot", umi_v, &window(tv, true));
    emit("v_ob", &revcomp(umi_v), &revcomp(&window(tv, true)));

    let read = genome.path().join("reads.fq");
    fs::write(&read, &fq).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .arg("--five_base_umi_len")
        .arg("8")
        .arg("--five_base_consensus")
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    let bam = outdir.path().join("reads_bismark_mm2.5base_consensus.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let (mut n_records, mut z_at_tm, mut dot_at_tv) = (0usize, false, false);
    for rec in reader.records() {
        let rec = rec.unwrap();
        let inner = rec.inner();
        n_records += 1;
        // Consensus reads are forward (FLAG 0) → XM aligned with read order.
        assert_eq!(
            u16::from(inner.flags()) & 0x10,
            0,
            "consensus reads are forward"
        );
        let xm = bismark_io::tags::xm(inner.data()).unwrap();
        let ref_start = usize::from(inner.alignment_start().unwrap()) - 1;
        // pure-M consensus: read_idx = genomic - ref_start.
        let read_at = |gpos: usize| -> Option<usize> {
            let mut ri = 0usize;
            let mut rp = ref_start;
            for op in inner.cigar().as_ref().iter() {
                use noodles_sam::alignment::record::cigar::op::Kind::*;
                let len = op.len();
                match op.kind() {
                    Match | SequenceMatch | SequenceMismatch => {
                        for _ in 0..len {
                            if rp == gpos {
                                return Some(ri);
                            }
                            ri += 1;
                            rp += 1;
                        }
                    }
                    Insertion | SoftClip => ri += len,
                    Deletion | Skip => rp += len,
                    _ => {}
                }
            }
            None
        };
        if let Some(ri) = read_at(tm)
            && xm[ri] == b'Z'
        {
            z_at_tm = true;
        }
        if let Some(ri) = read_at(tv) {
            // Variant masked to N → methylation call is '.' (not Z/z).
            if xm[ri] == b'.' {
                dot_at_tv = true;
            }
            assert_ne!(
                xm[ri], b'Z',
                "the homozygous C>T variant must NOT be called methylated in the consensus"
            );
        }
    }
    // Two duplex families → two consensus reads (one per molecule).
    assert_eq!(n_records, 2, "one consensus read per duplex family");
    assert!(z_at_tm, "the 5mC CpG must be Z in the consensus read");
    assert!(
        dot_at_tv,
        "the homozygous C>T CpG must be masked ('.') in the consensus read"
    );
}

/// THE PAIRED-END DUPLEX GATE: each molecule is sequenced as TWO read-pairs (top-strand
/// pair + bottom-strand pair) with the dual UMI swapped in the read NAME, both mapping to
/// the SAME fragment. Run PE with `--five_base_umi_qname --five_base_duplex`. The two
/// pairs of a molecule must collapse into ONE duplex-paired family (keyed by fragment
/// span + canonical dual UMI), and the per-molecule reconciliation must separate the 5mC
/// molecule (methylation) from the homozygous C>T molecule (variant) — proving the PE
/// strand handling (molecule-strand for pairing, FLAG coverage-strand for reconciliation).
#[test]
fn five_base_pe_duplex_groundtruth_pairs_two_pairs_per_molecule() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (PE duplex gate)");
        return;
    }
    let reference = gen_reference(900);
    let genome = TempDir::new().unwrap();
    write_genome(genome.path(), &reference);
    let cpgs = cpg_positions(&reference);
    let (frag, rl) = (140usize, 100usize); // R1 covers [0,100), R2 covers [40,140)
    // Target CpG inside the R1∩R2 overlap [40,100) of each fragment.
    let pick = |frag_start: usize| -> usize {
        *cpgs
            .iter()
            .find(|&&c| c >= frag_start + 45 && c < frag_start + 95)
            .expect("a CpG in the mate overlap")
    };
    let sm = 100usize; // 5mC molecule fragment start
    let sv = 500usize; // variant molecule fragment start
    let tm = pick(sm);
    let tv = pick(sv);

    let mut fq1 = Vec::new();
    let mut fq2 = Vec::new();
    let emit = |fq: &mut Vec<u8>, name: &str, bytes: &[u8]| {
        fq.extend_from_slice(format!("@{name}\n").as_bytes());
        fq.extend_from_slice(bytes);
        fq.extend_from_slice(b"\n+\n");
        fq.extend_from_slice(&vec![b'I'; bytes.len()]);
        fq.push(b'\n');
    };
    // Build one molecule's two pairs. `convert_target` puts T at the + target (5mC or
    // variant on the + strand); `variant` also makes the - strand carry the variant.
    let mut molecule = |s: usize, t: usize, variant: bool, umi_a: &str, umi_b: &str| {
        // + strand as forward reads see it: reference with the target set to T.
        let mut g_plus = reference[s..s + frag].to_vec();
        g_plus[t - s] = b'T';
        // what reverse reads carry = revcomp of the - strand. For 5mC the - base at the
        // target is intact (G, from unconverted C); for a variant it is A (from + T).
        let minus_src = if variant {
            g_plus.clone()
        } else {
            reference[s..s + frag].to_vec()
        };
        let g_minus_rc = revcomp(&minus_src);
        let fwd_left = &g_plus[0..rl]; // covers [s, s+rl)
        let rev_right = &g_minus_rc[0..rl]; // revcomp of [s+frag-rl, s+frag)
        // Top-strand pair (molecule OT): R1 forward-left, R2 reverse-right. UMI A+B.
        emit(&mut fq1, &format!("top_{s}:{umi_a}+{umi_b}"), fwd_left);
        emit(&mut fq2, &format!("top_{s}:{umi_a}+{umi_b}"), rev_right);
        // Bottom-strand pair (molecule OB): R1 reverse-right, R2 forward-left. UMI B+A.
        emit(&mut fq1, &format!("bot_{s}:{umi_b}+{umi_a}"), rev_right);
        emit(&mut fq2, &format!("bot_{s}:{umi_b}+{umi_a}"), fwd_left);
    };
    molecule(sm, tm, false, "AACCGGTT", "TTGGCCAA");
    molecule(sv, tv, true, "GGGGAAAA", "CCCCTTTT");

    let read1 = genome.path().join("r1.fq");
    let read2 = genome.path().join("r2.fq");
    fs::write(&read1, &fq1).unwrap();
    fs::write(&read2, &fq2).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .arg("--five_base_umi_qname")
        .arg("--five_base_duplex")
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

    let report =
        fs::read_to_string(outdir.path().join("r1_bismark_mm2_pe.5base_duplex.txt")).unwrap();
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
    assert!(
        paired.len() >= 2,
        "PE: expected >=2 duplex-paired families (2+2 members each)\nreport:\n{report}"
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
}

/// THE PE CONSENSUS GATE: same two-pairs-per-molecule fixture as the PE duplex gate, but
/// run with `--five_base_consensus`. The PE collapse must emit ONE consensus read per
/// duplex family into `<out>_pe.5base_consensus.bam`, with the 5mC CpG called `Z` and the
/// homozygous C>T CpG masked to `.` (the asymmetric reconciliation over all four reads).
#[test]
fn five_base_pe_consensus_groundtruth_collapses_and_masks_variant() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (PE consensus gate)");
        return;
    }
    let reference = gen_reference(900);
    let genome = TempDir::new().unwrap();
    write_genome(genome.path(), &reference);
    let cpgs = cpg_positions(&reference);
    let (frag, rl) = (140usize, 100usize);
    let pick = |frag_start: usize| -> usize {
        *cpgs
            .iter()
            .find(|&&c| c >= frag_start + 45 && c < frag_start + 95)
            .expect("a CpG in the mate overlap")
    };
    let (sm, sv) = (100usize, 500usize);
    let (tm, tv) = (pick(sm), pick(sv));

    let mut fq1 = Vec::new();
    let mut fq2 = Vec::new();
    let emit = |fq: &mut Vec<u8>, name: &str, bytes: &[u8]| {
        fq.extend_from_slice(format!("@{name}\n").as_bytes());
        fq.extend_from_slice(bytes);
        fq.extend_from_slice(b"\n+\n");
        fq.extend_from_slice(&vec![b'I'; bytes.len()]);
        fq.push(b'\n');
    };
    let mut molecule = |s: usize, t: usize, variant: bool, ua: &str, ub: &str| {
        let mut g_plus = reference[s..s + frag].to_vec();
        g_plus[t - s] = b'T';
        let minus_src = if variant {
            g_plus.clone()
        } else {
            reference[s..s + frag].to_vec()
        };
        let g_minus_rc = revcomp(&minus_src);
        let (fwd_left, rev_right) = (&g_plus[0..rl], &g_minus_rc[0..rl]);
        emit(&mut fq1, &format!("top_{s}:{ua}+{ub}"), fwd_left);
        emit(&mut fq2, &format!("top_{s}:{ua}+{ub}"), rev_right);
        emit(&mut fq1, &format!("bot_{s}:{ub}+{ua}"), rev_right);
        emit(&mut fq2, &format!("bot_{s}:{ub}+{ua}"), fwd_left);
    };
    molecule(sm, tm, false, "AACCGGTT", "TTGGCCAA");
    molecule(sv, tv, true, "GGGGAAAA", "CCCCTTTT");

    let read1 = genome.path().join("r1.fq");
    let read2 = genome.path().join("r2.fq");
    fs::write(&read1, &fq1).unwrap();
    fs::write(&read2, &fq2).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .arg("--five_base_umi_qname")
        .arg("--five_base_consensus")
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

    let bam = outdir.path().join("r1_bismark_mm2_pe.5base_consensus.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let (mut n, mut z_at_tm, mut dot_at_tv) = (0usize, false, false);
    for rec in reader.records() {
        let rec = rec.unwrap();
        let inner = rec.inner();
        n += 1;
        let xm = bismark_io::tags::xm(inner.data()).unwrap();
        let ref_start = usize::from(inner.alignment_start().unwrap()) - 1;
        let read_at = |gpos: usize| -> Option<usize> {
            let (mut ri, mut rp) = (0usize, ref_start);
            for op in inner.cigar().as_ref().iter() {
                use noodles_sam::alignment::record::cigar::op::Kind::*;
                let len = op.len();
                match op.kind() {
                    Match | SequenceMatch | SequenceMismatch => {
                        for _ in 0..len {
                            if rp == gpos {
                                return Some(ri);
                            }
                            ri += 1;
                            rp += 1;
                        }
                    }
                    Insertion | SoftClip => ri += len,
                    Deletion | Skip => rp += len,
                    _ => {}
                }
            }
            None
        };
        if let Some(ri) = read_at(tm)
            && xm[ri] == b'Z'
        {
            z_at_tm = true;
        }
        if let Some(ri) = read_at(tv) {
            if xm[ri] == b'.' {
                dot_at_tv = true;
            }
            assert_ne!(
                xm[ri], b'Z',
                "PE consensus: C>T variant must not be methylated"
            );
        }
    }
    assert_eq!(n, 2, "PE: one consensus read per duplex family");
    assert!(z_at_tm, "PE consensus: the 5mC CpG must be Z");
    assert!(dot_at_tv, "PE consensus: the C>T CpG must be masked ('.')");
}

/// REGRESSION (real-data bug): real Illumina FastQ headers carry a comment after a
/// space, e.g. `@LH00757:...:ANCGTTG+NGGTGTA 1:N:0:GTAACTGAAG+TCNCGACTCC`. The aligner
/// truncates the SAM QNAME at the first whitespace, so the 5-Base lockstep must derive
/// its read identifier the same way (whitespace-truncated) regardless of `--icpc`;
/// otherwise every read desyncs ("expected ..._1:N:0:..., minimap2 emitted ..."). This
/// gate feeds Illumina-style spaced headers through the real-minimap2 5-Base path and
/// asserts it runs and still recovers the methylation calls.
#[test]
fn five_base_groundtruth_illumina_spaced_header_no_desync() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (spaced-header regression)");
        return;
    }
    let reference = gen_reference(600);
    let genome = TempDir::new().unwrap();
    write_genome(genome.path(), &reference);
    let (plain, truth) = make_methylated_reads(&reference, 120, 5);
    // Rewrite each `@<name>` header to `@<name> 1:N:0:GTAACTGAAG+TCNCGACTCC` (a real
    // Illumina dual-index comment), leaving seq/qual lines untouched.
    let mut fastq = Vec::new();
    for (i, line) in plain.split_inclusive(|&b| b == b'\n').enumerate() {
        if i % 4 == 0 {
            let trimmed = &line[..line.len() - 1]; // drop '\n'
            fastq.extend_from_slice(trimmed);
            fastq.extend_from_slice(b" 1:N:0:GTAACTGAAG+TCNCGACTCC\n");
        } else {
            fastq.extend_from_slice(line);
        }
    }
    let read = genome.path().join("reads.fq");
    fs::write(&read, &fastq).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    let bam = outdir.path().join("reads_bismark_mm2.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let (mut n, mut checked_meth) = (0usize, 0usize);
    for rec in reader.records() {
        let rec = rec.unwrap();
        n += 1;
        let inner = rec.inner();
        // The BAM qname is whitespace-truncated (the comment dropped) → matches truth keys.
        let qname = String::from_utf8_lossy(inner.name().unwrap().as_ref()).into_owned();
        assert!(!qname.contains(' '), "qname must not contain the comment");
        let Some(expect) = truth.get(&qname) else {
            continue;
        };
        let xm = bismark_io::tags::xm(inner.data()).unwrap();
        let ref_start = usize::from(inner.alignment_start().unwrap()) - 1;
        let (mut ri, mut rp) = (0usize, ref_start);
        let mut read_at = HashMap::<usize, usize>::new();
        for op in inner.cigar().as_ref().iter() {
            use noodles_sam::alignment::record::cigar::op::Kind::*;
            let len = op.len();
            match op.kind() {
                Match | SequenceMatch | SequenceMismatch => {
                    for _ in 0..len {
                        read_at.insert(rp, ri);
                        ri += 1;
                        rp += 1;
                    }
                }
                Insertion | SoftClip => ri += len,
                Deletion | Skip => rp += len,
                _ => {}
            }
        }
        for &(cpg_pos, methylated) in expect {
            if let Some(&ri) = read_at.get(&cpg_pos)
                && methylated
                && xm[ri] == b'Z'
            {
                checked_meth += 1;
            }
        }
    }
    assert!(
        n >= 1,
        "reads with Illumina spaced headers must align (no desync)"
    );
    assert!(
        checked_meth >= 1,
        "should still recover methylated CpGs with spaced headers"
    );
}

/// THE REAL-GENOME DUPLEX-CONSENSUS GATE: same construction as the synthetic consensus
/// gate, but the reference is the REAL downloaded genome (`test_files/NC_010473.fa.gz`,
/// E. coli K-12) — real CpG density and sequence complexity, aligned with the REAL
/// minimap2. Two duplex molecules (one 5mC, one homozygous C>T) are synthesized from
/// real subsequences; the collapse must emit one consensus read per family with the 5mC
/// CpG called `Z` and the C>T CpG masked to `.`. No-op if minimap2 or the genome file is
/// absent (CI stays green).
#[test]
fn five_base_consensus_groundtruth_real_reference_ecoli() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (real-reference consensus gate)");
        return;
    }
    let Some(gz) = real_genome_gz() else {
        eprintln!("skipping: test_files/NC_010473.fa.gz not found (real-reference gate)");
        return;
    };
    let reference = load_first_contig_gz(&gz);
    assert!(reference.len() > 200_000, "real genome should be large");
    let genome = TempDir::new().unwrap();
    write_genome(genome.path(), &reference);

    // Pick two well-separated CpG cytosines in the real sequence (cores of two windows).
    let half = 60usize;
    let cpg_near = |center: usize| -> usize {
        (center - 25..center + 25)
            .find(|&i| reference[i] == b'C' && reference[i + 1] == b'G')
            .expect("a CpG near the chosen center of the real genome")
    };
    let tm = cpg_near(100_000); // 5mC molecule
    let tv = cpg_near(105_000); // homozygous C>T molecule
    assert_ne!(tm, tv);

    let window = |t: usize, convert_target: bool| -> Vec<u8> {
        let mut w = reference[t - half..t + half].to_vec();
        if convert_target {
            w[half] = b'T';
        }
        w
    };
    let mut fq = Vec::new();
    let mut emit = |name: &str, umi: &[u8], core: &[u8]| {
        let mut read = umi.to_vec();
        read.extend_from_slice(core);
        fq.extend_from_slice(format!("@{name}\n").as_bytes());
        fq.extend_from_slice(&read);
        fq.extend_from_slice(b"\n+\n");
        fq.extend_from_slice(&vec![b'I'; read.len()]);
        fq.push(b'\n');
    };
    let umi_m = b"AACCGGTT";
    let umi_v = b"TTGGCCAA";
    emit("m_ot", umi_m, &window(tm, true));
    emit("m_ob", &revcomp(umi_m), &revcomp(&window(tm, false)));
    emit("v_ot", umi_v, &window(tv, true));
    emit("v_ob", &revcomp(umi_v), &revcomp(&window(tv, true)));

    let read = genome.path().join("reads.fq");
    fs::write(&read, &fq).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
        .arg("--five_base_umi_len")
        .arg("8")
        .arg("--five_base_consensus")
        .arg("--temp_dir")
        .arg(temp.path())
        .arg("--output_dir")
        .arg(outdir.path())
        .arg(&read)
        .assert()
        .success();

    let bam = outdir.path().join("reads_bismark_mm2.5base_consensus.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let (mut n_records, mut z_at_tm, mut dot_at_tv) = (0usize, false, false);
    for rec in reader.records() {
        let rec = rec.unwrap();
        let inner = rec.inner();
        n_records += 1;
        let xm = bismark_io::tags::xm(inner.data()).unwrap();
        let ref_start = usize::from(inner.alignment_start().unwrap()) - 1;
        let read_at = |gpos: usize| -> Option<usize> {
            let (mut ri, mut rp) = (0usize, ref_start);
            for op in inner.cigar().as_ref().iter() {
                use noodles_sam::alignment::record::cigar::op::Kind::*;
                let len = op.len();
                match op.kind() {
                    Match | SequenceMatch | SequenceMismatch => {
                        for _ in 0..len {
                            if rp == gpos {
                                return Some(ri);
                            }
                            ri += 1;
                            rp += 1;
                        }
                    }
                    Insertion | SoftClip => ri += len,
                    Deletion | Skip => rp += len,
                    _ => {}
                }
            }
            None
        };
        if let Some(ri) = read_at(tm)
            && xm[ri] == b'Z'
        {
            z_at_tm = true;
        }
        if let Some(ri) = read_at(tv) {
            if xm[ri] == b'.' {
                dot_at_tv = true;
            }
            assert_ne!(
                xm[ri], b'Z',
                "real-genome C>T variant must not be methylated"
            );
        }
    }
    assert_eq!(
        n_records, 2,
        "one consensus read per duplex family (real genome)"
    );
    assert!(
        z_at_tm,
        "the 5mC CpG must be Z in the consensus (real genome)"
    );
    assert!(
        dot_at_tv,
        "the homozygous C>T CpG must be masked ('.') in the consensus (real genome)"
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

/// THE PE GATE: real minimap2 in PAIRED mode. Read pairs are an FR fragment of the
/// reference (R1 = forward 5' end with injected 5mC->T methylation; R2 = revcomp of
/// the 3' end). After `--illumina_5base` PE alignment, every pair yields two records
/// (R1 FLAG 0x40 forward, R2 FLAG 0x80 reverse) and the R1 CpG calls match ground
/// truth at every aligned position (the OT/index-0 inverted call, via real minimap2 PE).
#[test]
fn five_base_pe_groundtruth_real_minimap2() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (PE ground-truth gate)");
        return;
    }
    let reference = gen_reference(900);
    let genome = TempDir::new().unwrap();
    write_genome(genome.path(), &reference);

    let (frag, rl, anchor) = (180usize, 90usize, 10usize);
    let cpgs = cpg_positions(&reference);
    let mut fq1 = Vec::new();
    let mut fq2 = Vec::new();
    let mut truth: HashMap<String, Vec<(usize, bool)>> = HashMap::new();
    let n_pairs = reference.len() / frag;
    for p in 0..n_pairs {
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
        // R2: reverse-complement of the fragment's 3' end (kept as reference).
        let r2 = revcomp(&reference[start + frag - rl..start + frag]);

        fq1.extend_from_slice(format!("@{qname}\n").as_bytes());
        fq1.extend_from_slice(&r1);
        fq1.extend_from_slice(b"\n+\n");
        fq1.extend_from_slice(&vec![b'I'; rl]);
        fq1.push(b'\n');
        fq2.extend_from_slice(format!("@{qname}\n").as_bytes());
        fq2.extend_from_slice(&r2);
        fq2.extend_from_slice(b"\n+\n");
        fq2.extend_from_slice(&vec![b'I'; rl]);
        fq2.push(b'\n');
    }
    let read1 = genome.path().join("reads_1.fq");
    let read2 = genome.path().join("reads_2.fq");
    fs::write(&read1, &fq1).unwrap();
    fs::write(&read2, &fq2).unwrap();
    let temp = TempDir::new().unwrap();
    let outdir = TempDir::new().unwrap();

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
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

    let bam = outdir.path().join("reads_1_bismark_mm2_pe.bam");
    let mut reader = bismark_io::BamReader::from_path(&bam).unwrap();
    let (mut r1_records, mut r2_records, mut checked, mut checked_meth) = (0usize, 0usize, 0, 0);
    for rec in reader.records() {
        let rec = rec.unwrap();
        let inner = rec.inner();
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
        let xm = bismark_io::tags::xm(inner.data()).unwrap();
        let ref_start = usize::from(inner.alignment_start().unwrap()) - 1;
        let mut read_at = HashMap::<usize, usize>::new();
        let (mut ri, mut rp) = (0usize, ref_start);
        for op in inner.cigar().as_ref().iter() {
            let len = op.len();
            use noodles_sam::alignment::record::cigar::op::Kind::*;
            match op.kind() {
                Match | SequenceMatch | SequenceMismatch => {
                    for _ in 0..len {
                        read_at.insert(rp, ri);
                        ri += 1;
                        rp += 1;
                    }
                }
                Insertion | SoftClip => ri += len,
                Deletion | Skip => rp += len,
                _ => {}
            }
        }
        for &(cpg_pos, methylated) in expect {
            if let Some(&ri) = read_at.get(&cpg_pos) {
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
    assert!(
        r1_records >= 1 && r2_records >= 1,
        "PE should emit both mates"
    );
    assert_eq!(r1_records, r2_records, "one R1 per R2");
    assert!(
        checked >= 3,
        "should validate several R1 CpGs, got {checked}"
    );
    assert!(
        checked_meth >= 1,
        "should confirm at least one methylated R1 CpG through real minimap2 PE"
    );
}
