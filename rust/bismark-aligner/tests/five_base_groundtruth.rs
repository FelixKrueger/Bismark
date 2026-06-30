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

fn have_minimap2() -> bool {
    let present = StdCommand::new("minimap2")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    // Fail loud in CI: these real-aligner gates must not pass vacuously (#787 review).
    // Locally (no $CI) a missing minimap2 just skips the gate.
    if !present && std::env::var_os("CI").is_some() {
        panic!(
            "minimap2 not found but $CI is set: the 5-Base ground-truth gates require \
             minimap2 on PATH in CI (install it in the workflow) — refusing to no-op."
        );
    }
    present
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
    // FAITHFUL duplex fixture (molecule-strand, not coverage-strand). A real 5-Base duplex
    // is two complementary strands of ONE molecule. At a `+` CpG, only the TOP strand carries
    // that cytosine's methylation (the bottom strand has G there), so EVERY read of the OT
    // molecule shows `T` at `t` while EVERY read of the OB molecule shows the unconverted `C`
    // — confirmed on real Illumina data (OB forward reads are 0.8% T at `+` CpGs vs 49% for
    // OT). A homozygous C>T variant, by contrast, is present on BOTH strands, so the OB reads
    // also show `T`. The consensus must therefore key own/opposite on MOLECULE strand.
    let mut molecule = |s: usize, t: usize, variant: bool, ua: &str, ub: &str| {
        // TOP strand in +ref orientation: 5mC (or variant) → T at t.
        let mut top = reference[s..s + frag].to_vec();
        top[t - s] = b'T';
        // BOTTOM strand in +ref orientation: unconverted C at t, UNLESS a homozygous variant.
        let bot = if variant {
            top.clone()
        } else {
            reference[s..s + frag].to_vec()
        };
        // Proper FR pairs. OT molecule: R1 fwd (left) + R2 rev (right), both TOP strand.
        // OB molecule: R1 rev (right) + R2 fwd (left), both BOTTOM strand.
        let (ot_r1, ot_r2) = (top[0..rl].to_vec(), revcomp(&top[frag - rl..frag]));
        let (ob_r1, ob_r2) = (revcomp(&bot[frag - rl..frag]), bot[0..rl].to_vec());
        emit(&mut fq1, &format!("top_{s}:{ua}+{ub}"), &ot_r1);
        emit(&mut fq2, &format!("top_{s}:{ua}+{ub}"), &ot_r2);
        emit(&mut fq1, &format!("bot_{s}:{ub}+{ua}"), &ob_r1);
        emit(&mut fq2, &format!("bot_{s}:{ub}+{ua}"), &ob_r2);
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
    // The `-`-strand cytosine of the same CpG dinucleotide sits at tm+1 (a genomic G). It is
    // scored ONLY by the reverse consensus record (GA call); forward-only emission left it
    // uncalled. Assert it is now a real CpG call, proving both strands are emitted.
    let mut minus_called = false;
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
        if let Some(ri) = read_at(tm + 1)
            && (xm[ri] == b'Z' || xm[ri] == b'z')
        {
            minus_called = true; // `-`-strand CpG now scored by the reverse record
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
    // Two records per duplex family now (forward `+` calls + reverse `-` calls) × 2 molecules.
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
    let ct = dir.join("Bisulfite_Genome").join("CT_conversion");
    let ga = dir.join("Bisulfite_Genome").join("GA_conversion");
    fs::create_dir_all(&ct).unwrap();
    fs::create_dir_all(&ga).unwrap();
    fs::write(ct.join("BS_CT.mmi"), b"x").unwrap();
    fs::write(ga.join("BS_GA.mmi"), b"x").unwrap();
    let mut fa = Vec::new();
    for (name, seq) in contigs {
        fa.extend_from_slice(format!(">{name}\n").as_bytes());
        fa.extend_from_slice(seq);
        fa.push(b'\n');
    }
    fs::write(dir.join("genome.fa"), fa).unwrap();
}

/// Append forward SE reads tiled across `seq`; if `methylated`, convert every `+` CpG
/// C→T (the 5mC→T signal). The qname `prefix` records the control of origin (truth).
fn emit_se_control(
    fq: &mut Vec<u8>,
    prefix: &str,
    seq: &[u8],
    methylated: bool,
    read_len: usize,
    step: usize,
) {
    let (mut i, mut n) = (0usize, 0usize);
    while i + read_len <= seq.len() {
        let mut read = seq[i..i + read_len].to_vec();
        if methylated {
            for k in 0..read_len.saturating_sub(1) {
                if read[k] == b'C' && read[k + 1] == b'G' {
                    read[k] = b'T'; // 5mC -> T at a + CpG
                }
            }
        }
        fq.extend_from_slice(format!("@{prefix}_{n}\n").as_bytes());
        fq.extend_from_slice(&read);
        fq.extend_from_slice(b"\n+\n");
        fq.extend_from_slice(&vec![b'I'; read_len]);
        fq.push(b'\n');
        i += step;
        n += 1;
    }
}

/// Count CpG calls (`Z` meth / `z` unmeth) in a BAM, bucketed by a key derived from the
/// qname (the control of origin). `key` maps a qname to its bucket. Returns
/// `key -> (meth, unmeth)`.
fn count_cpg_keyed<F: Fn(&str) -> String>(bam: &Path, key: F) -> HashMap<String, (u64, u64)> {
    let mut reader = bismark_io::BamReader::from_path(bam).unwrap();
    let mut out: HashMap<String, (u64, u64)> = HashMap::new();
    for rec in reader.records() {
        let rec = rec.unwrap();
        let inner = rec.inner();
        let qname = String::from_utf8_lossy(inner.name().unwrap().as_ref()).into_owned();
        let xm = bismark_io::tags::xm(inner.data()).unwrap();
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
#[test]
fn five_base_controls_core_recovers_lambda_and_puc19() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (control gate)");
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

    let mut fq = Vec::new();
    emit_se_control(&mut fq, "lam", lambda_sub, false, 120, 60); // unmethylated
    emit_se_control(&mut fq, "puc", &puc19, true, 120, 30); // CpG-methylated
    let read = genome.path().join("reads.fq");
    fs::write(&read, &fq).unwrap();
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

    // Bucket by qname prefix (`lam`/`puc`) = the control of origin.
    let counts = count_cpg_keyed(&outdir.path().join("reads_bismark_mm2.bam"), |q| {
        q.split('_').next().unwrap_or("").to_string()
    });
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
fn five_base_controls_consensus_preserves_methylation_state() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (consensus control gate)");
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

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
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
        &outdir.path().join("r1_bismark_mm2_pe.5base_consensus.bam"),
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
fn five_base_controls_deconvolution_no_false_variants() {
    if !have_minimap2() {
        eprintln!("skipping: minimap2 not on PATH (deconvolution control gate)");
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

    bin()
        .arg("--genome")
        .arg(genome.path())
        .arg("--illumina_5base")
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
