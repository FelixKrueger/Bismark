#!/usr/bin/env python3
"""minimap2 PE concordance gate vs the byte-frozen Bowtie 2 PE oracle.

Simulates fully-unmethylated DIRECTIONAL (OT) WGBS paired-end reads from a random
genome, runs `bismark_rs --minimap2 -1/-2` and `bismark_rs --bowtie2 -1/-2` on the
SAME reads, and compares, on the pairs both map uniquely:
  - mapping to the simulated truth position,
  - XM methylation-string agreement (the shared extraction code -> must be identical).
Also runs a LONG-read PE set (minimap2 niche; bowtie2 can't) -> truth-position only.
"""
import os, random, subprocess, sys, tempfile, pysam

RS = os.path.abspath("target/release/bismark_rs")
PREP = os.path.abspath("target/release/bismark_genome_preparation_rs")
random.seed(20260623)

def revcomp(s):
    return s.translate(str.maketrans("ACGT", "TGCA"))[::-1]

def gen_genome(path, n=60000):
    # CpG-rich-ish random genome so XM has z/Z to compare.
    seq = "".join(random.choice("ACGT") for _ in range(n))
    with open(os.path.join(path, "genome.fa"), "w") as f:
        f.write(">chrSim\n")
        for i in range(0, len(seq), 70):
            f.write(seq[i:i+70] + "\n")
    return seq

def sim_directional_pe(seq, n, rlen, insert, fq1, fq2):
    """Fully-METHYLATED DIRECTIONAL (OT) reads = plain genomic substrings (C's kept =
    methylated; Bismark does the C->T/G->A conversion internally). read1 = top-strand 5'
    end, read2 = revcomp of the top-strand 3' end (the bottom-strand 5' end)."""
    truth = {}
    g = len(seq)
    with open(fq1, "w") as f1, open(fq2, "w") as f2:
        for i in range(n):
            p = random.randint(0, g - insert - 1)
            frag = seq[p:p+insert]
            r1 = frag[:rlen]
            r2 = revcomp(frag[insert-rlen:insert])
            name = f"sim{i:06d}"
            f1.write(f"@{name}/1\n{r1}\n+\n{'I'*rlen}\n")
            f2.write(f"@{name}/2\n{r2}\n+\n{'I'*rlen}\n")
            truth[name] = p + 1                       # 1-based leftmost (read1) POS
    return truth

def run(aligner_flag, genome, r1, r2, outdir, tag, extra=None):
    os.makedirs(outdir, exist_ok=True)
    cmd = [RS, "--genome", genome, aligner_flag, "--temp_dir", outdir,
           "--output_dir", outdir, "-1", r1, "-2", r2, "--basename", tag]
    if extra:
        cmd += extra
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        print(f"  !! {aligner_flag} FAILED\n{r.stderr[-2000:]}")
        sys.exit(1)
    return os.path.join(outdir, f"{tag}_pe.bam")

def base_id(qname):
    # bismark output QNAME = identifier = "<base>/1"; truth is keyed on "<base>".
    for suf in ("/1", "/2"):
        if qname.endswith(suf):
            return qname[:-len(suf)]
    return qname

def load_pairs(bam):
    """base_id -> (read1_pos_1based, read1_XM). Read1 = FLAG & 0x40."""
    out = {}
    f = pysam.AlignmentFile(bam, "rb", check_sq=False)
    for rec in f:
        if rec.is_unmapped or not rec.is_read1:
            continue
        xm = dict(rec.tags).get("XM", "")
        out[base_id(rec.query_name)] = (rec.reference_start + 1, xm)
    return out

def main():
    work = tempfile.mkdtemp(prefix="mm2pe_gate_")
    genome = os.path.join(work, "genome"); os.makedirs(genome)
    print(f"workdir: {work}")
    seq = gen_genome(genome)
    for al in ("--bowtie2", "--minimap2"):
        r = subprocess.run([PREP, al, genome], capture_output=True, text=True)
        if r.returncode != 0:
            print(f"genome prep {al} failed:\n{r.stderr[-1500:]}"); sys.exit(1)
    print("indexes built (bowtie2 + minimap2)\n")

    # ---- SHORT-read PE: minimap2 vs bowtie2 concordance ----
    r1 = os.path.join(work, "s_1.fq"); r2 = os.path.join(work, "s_2.fq")
    truth = sim_directional_pe(seq, 1000, 100, 320, r1, r2)
    bt = load_pairs(run("--bowtie2", genome, r1, r2, os.path.join(work, "bt"), "bt"))
    mm = load_pairs(run("--minimap2", genome, r1, r2, os.path.join(work, "mm"), "mm"))

    def acc(d):
        return sum(1 for n,(pos,_) in d.items() if truth.get(n) == pos)
    print(f"=== SHORT-read PE (1000 pairs, 100bp, insert 320) ===")
    print(f"bowtie2 : mapped {len(bt):4d}  at-truth {acc(bt):4d}")
    print(f"minimap2: mapped {len(mm):4d}  at-truth {acc(mm):4d}")
    common = set(bt) & set(mm)
    pos_agree = sum(1 for n in common if bt[n][0] == mm[n][0])
    xm_agree  = sum(1 for n in common if bt[n][1] == mm[n][1])
    print(f"common mapped pairs: {len(common)}")
    print(f"  position concordance: {pos_agree}/{len(common)} = {100*pos_agree/max(1,len(common)):.3f}%")
    print(f"  XM concordance:       {xm_agree}/{len(common)} = {100*xm_agree/max(1,len(common)):.3f}%")
    mm_truth = sum(1 for n in mm if truth.get(n)==mm[n][0])
    print(f"  minimap2 at-truth among its mapped: {mm_truth}/{len(mm)} = {100*mm_truth/max(1,len(mm)):.3f}%")

    # ---- LONG-read PE (minimap2 niche; bowtie2 can't): truth-position only ----
    lr1 = os.path.join(work, "l_1.fq"); lr2 = os.path.join(work, "l_2.fq")
    ltruth = sim_directional_pe(seq, 300, 600, 1800, lr1, lr2)
    lmm = load_pairs(run("--minimap2", genome, lr1, lr2, os.path.join(work, "lmm"), "lmm"))
    lmm_truth = sum(1 for n in lmm if ltruth.get(n)==lmm[n][0])
    print(f"\n=== LONG-read PE (300 pairs, 600bp, insert 1800) — minimap2 only ===")
    print(f"minimap2: mapped {len(lmm)}  at-truth {lmm_truth} = {100*lmm_truth/max(1,len(lmm)):.3f}%")

    # ---- determinism: minimap2 PE run twice -> identical BAM body ----
    mm_a = run("--minimap2", genome, r1, r2, os.path.join(work, "da"), "da")
    mm_b = run("--minimap2", genome, r1, r2, os.path.join(work, "db"), "db")
    da = subprocess.run(["samtools","view",mm_a], capture_output=True, text=True).stdout
    db = subprocess.run(["samtools","view",mm_b], capture_output=True, text=True).stdout
    print(f"\n=== determinism ===")
    print(f"minimap2 PE run1==run2 body: {da == db}")

    # ---- worker-invariance: --multicore 1 vs 4 -> identical BAM body ----
    mm_1 = run("--minimap2", genome, r1, r2, os.path.join(work, "w1"), "w1", ["--multicore","1"])
    mm_4 = run("--minimap2", genome, r1, r2, os.path.join(work, "w4"), "w4", ["--multicore","4"])
    w1 = subprocess.run(["samtools","view",mm_1], capture_output=True, text=True).stdout
    w4 = subprocess.run(["samtools","view",mm_4], capture_output=True, text=True).stdout
    print(f"\n=== worker-invariance ===")
    print(f"minimap2 PE --multicore 1 == 4 body: {w1 == w4}")

if __name__ == "__main__":
    main()
