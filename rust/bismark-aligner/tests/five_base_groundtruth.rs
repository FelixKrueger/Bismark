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
use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use tempfile::TempDir;

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
