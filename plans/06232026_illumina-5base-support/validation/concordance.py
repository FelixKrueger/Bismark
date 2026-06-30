#!/usr/bin/env python3
"""5-Base real-data concordance vs DRAGEN (#787 GA graduation harness).

Two subcommands, both pure stdlib (gzip + math), reading plain or .gz:

  methyl  --ours <cytosine_report> --dragen <CX_report.txt.gz>
          Per-CpG methylation concordance. Both inputs are Bismark-format CX
          cytosine reports (DRAGEN emits one too):
              chrom  pos(1-based)  strand  count_meth  count_unmeth  context  tri
          CpG rows only (context == "CpG"). Reports, at coverage >= 1/5/10 in
          BOTH: Pearson r, coverage-weighted r, mean |delta %|, call-agreement
          at the 50% threshold, and a 5x5 methylation-bin confusion matrix.

  deconv  --variants <our_*.5base_deconvolution.txt> --vcf <dragen.vcf[.gz]>
          Variant-vs-methylation precision/recall vs DRAGEN germline SNVs. Our
          report columns: chrom  pos(1-based)  strand  verdict  meth  total  pct
          (verdict in {variant, methylation, undetermined}). A CpG-disrupting
          SNV is C>T on a '+' CpG cytosine and G>A on a '-' CpG cytosine.
          Precision = our 'variant' CpGs that coincide with any DRAGEN C>T/G>A
          SNV. Recall = DRAGEN homozygous CpG-disrupting SNVs (at loci we cover)
          that we flagged 'variant'.

  --selftest   Build tiny synthetic inputs, run both, assert expected metrics.

This is a sanity/reproducibility tool for VALIDATION_REAL_DATA.md, not a CI
gate (the deterministic gate is the lambda/pUC19 controls in
tests/five_base_groundtruth.rs). The runs themselves are operator-driven.
"""

import argparse
import gzip
import io
import math
import os
import sys
import tempfile


def _open(path):
    """Open plain or gzip text by magic bytes (not extension)."""
    with open(path, "rb") as probe:
        magic = probe.read(2)
    if magic == b"\x1f\x8b":
        return io.TextIOWrapper(gzip.open(path, "rb"))
    return open(path, "r")


def parse_cx(path):
    """Bismark/DRAGEN CX cytosine report -> {(chrom,pos,strand): (meth, cov)} for CpG."""
    out = {}
    with _open(path) as fh:
        for line in fh:
            if not line or line[0] == "#":
                continue
            f = line.rstrip("\n").split("\t")
            if len(f) < 6:
                continue
            context = f[5]
            if context != "CpG":
                continue
            meth, unmeth = int(f[3]), int(f[4])
            cov = meth + unmeth
            if cov == 0:
                continue
            out[(f[0], f[1], f[2])] = (meth, cov)
    return out


def pearson(xs, ys):
    n = len(xs)
    if n < 2:
        return float("nan")
    mx, my = sum(xs) / n, sum(ys) / n
    sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    sxx = sum((x - mx) ** 2 for x in xs)
    syy = sum((y - my) ** 2 for y in ys)
    if sxx == 0 or syy == 0:
        return float("nan")
    return sxy / math.sqrt(sxx * syy)


def weighted_pearson(xs, ys, ws):
    sw = sum(ws)
    if sw == 0 or len(xs) < 2:
        return float("nan")
    mx = sum(w * x for w, x in zip(ws, xs)) / sw
    my = sum(w * y for w, y in zip(ws, ys)) / sw
    sxy = sum(w * (x - mx) * (y - my) for w, x, y in zip(ws, xs, ys))
    sxx = sum(w * (x - mx) ** 2 for w, x in zip(ws, xs))
    syy = sum(w * (y - my) ** 2 for w, y in zip(ws, ys))
    if sxx == 0 or syy == 0:
        return float("nan")
    return sxy / math.sqrt(sxx * syy)


def _bin5(pct):
    # 5 bins of 20 percentage points; 100% lands in the top bin.
    return min(4, int(pct // 20))


def cmd_methyl(args):
    ours = parse_cx(args.ours)
    drg = parse_cx(args.dragen)
    shared = ours.keys() & drg.keys()
    covs = [int(c) for c in args.covs.split(",")]
    print(f"shared CpGs (any cov): {len(shared)}")
    print(f"{'cov>=':>6} {'n':>12} {'pearson_r':>10} {'cov_wtd_r':>10} "
          f"{'mean|d%|':>9} {'call_agree@50':>14}")
    result = {}
    for t in covs:
        xs, ys, ws, agree = [], [], [], 0
        confusion = [[0] * 5 for _ in range(5)]
        for k in shared:
            om, oc = ours[k]
            dm, dc = drg[k]
            if oc < t or dc < t:
                continue
            ox, dy = 100.0 * om / oc, 100.0 * dm / dc
            xs.append(ox)
            ys.append(dy)
            ws.append(min(oc, dc))
            if (ox >= 50.0) == (dy >= 50.0):
                agree += 1
            confusion[_bin5(ox)][_bin5(dy)] += 1
        n = len(xs)
        if n == 0:
            print(f"{t:>6} {0:>12} {'NA':>10} {'NA':>10} {'NA':>9} {'NA':>14}")
            continue
        r = pearson(xs, ys)
        wr = weighted_pearson(xs, ys, ws)
        mad = sum(abs(x - y) for x, y in zip(xs, ys)) / n
        ca = 100.0 * agree / n
        print(f"{t:>6} {n:>12} {r:>10.4f} {wr:>10.4f} {mad:>8.2f}% {ca:>13.2f}%")
        result[t] = {"n": n, "r": r, "wr": wr, "mad": mad, "call_agree": ca,
                     "confusion": confusion}
    return result


def parse_our_variants(path):
    """Our deconvolution report -> dict {(chrom,pos): (strand, verdict)} for all CpGs."""
    out = {}
    with _open(path) as fh:
        for line in fh:
            if not line or line[0] == "#":
                continue
            f = line.rstrip("\n").split("\t")
            if len(f) < 4:
                continue
            out[(f[0], f[1])] = (f[2], f[3])
    return out


def parse_dragen_cpg_snvs(path):
    """DRAGEN VCF -> dict {(chrom,pos): (ref, alt, is_hom)} for PASS C>T and G>A SNVs."""
    out = {}
    with _open(path) as fh:
        for line in fh:
            if not line or line[0] == "#":
                continue
            f = line.rstrip("\n").split("\t")
            if len(f) < 7:
                continue
            chrom, pos, _id, ref, alt, _qual, flt = f[:7]
            if flt not in ("PASS", "."):
                continue
            if (ref, alt) not in (("C", "T"), ("G", "A")):
                continue
            is_hom = False
            if len(f) >= 10:
                gt = f[9].split(":")[0].replace("|", "/")
                is_hom = gt in ("1/1",)
            out[(chrom, pos)] = (ref, alt, is_hom)
    return out


def cmd_deconv(args):
    ours = parse_our_variants(args.variants)
    snvs = parse_dragen_cpg_snvs(args.vcf)
    our_variants = {k for k, (_s, v) in ours.items() if v == "variant"}

    # Precision: our 'variant' CpGs that coincide with any DRAGEN C>T/G>A SNV
    # (strand-consistent: '+' CpG wants C>T, '-' CpG wants G>A).
    def strand_consistent(k):
        strand = ours[k][0]
        hit = snvs.get(k)
        if hit is None:
            return False
        ref, _alt, _hom = hit
        return (strand == "+" and ref == "C") or (strand == "-" and ref == "G")

    tp_prec = sum(1 for k in our_variants if strand_consistent(k))
    precision = tp_prec / len(our_variants) if our_variants else float("nan")

    # Recall: DRAGEN homozygous CpG-disrupting SNVs at loci we cover, that we
    # called 'variant'.
    covered_hom = [k for k, (_r, _a, hom) in snvs.items() if hom and k in ours]
    tp_rec = sum(1 for k in covered_hom if ours[k][1] == "variant")
    recall = tp_rec / len(covered_hom) if covered_hom else float("nan")

    print(f"our 'variant' CpGs:        {len(our_variants)}")
    print(f"  precision (any DRAGEN C>T/G>A):  {tp_prec}/{len(our_variants)} "
          f"= {precision * 100:.1f}%")
    print(f"DRAGEN hom CpG-SNVs we cover: {len(covered_hom)}")
    print(f"  recall:                          {tp_rec}/{len(covered_hom)} "
          f"= {recall * 100:.1f}%")
    return {"precision": precision, "recall": recall,
            "our_variants": len(our_variants), "covered_hom": len(covered_hom)}


def _selftest():
    d = tempfile.mkdtemp(prefix="5base_concordance_selftest_")
    # methyl: 4 CpGs; 3 concordant (low/low, high/high, mid/mid), 1 discordant.
    ours_cx = os.path.join(d, "ours.CX_report.txt")
    drg_cx = os.path.join(d, "dragen.CX_report.txt.gz")
    ours_rows = [
        ("chr1", "100", "+", 0, 20, "CpG"),   # 0%
        ("chr1", "200", "+", 19, 1, "CpG"),   # 95%
        ("chr1", "300", "+", 10, 10, "CpG"),  # 50%
        ("chr1", "400", "+", 18, 2, "CpG"),   # 90% (DRAGEN says 0% -> discordant)
        ("chr1", "500", "+", 5, 5, "CHH"),    # non-CpG, ignored
    ]
    drg_rows = [
        ("chr1", "100", "+", 1, 19, "CpG"),   # 5%  (call low, agree)
        ("chr1", "200", "+", 20, 0, "CpG"),   # 100% (call high, agree)
        ("chr1", "300", "+", 11, 9, "CpG"),   # 55% (call high vs our 50% high, agree)
        ("chr1", "400", "+", 0, 20, "CpG"),   # 0%  (call low vs our high, DISagree)
    ]
    with open(ours_cx, "w") as fh:
        for r in ours_rows:
            fh.write("\t".join(str(x) for x in r) + "\ttri\n")
    with gzip.open(drg_cx, "wt") as fh:
        for r in drg_rows:
            fh.write("\t".join(str(x) for x in r) + "\ttri\n")

    res = cmd_methyl(argparse.Namespace(ours=ours_cx, dragen=drg_cx, covs="1"))
    assert res[1]["n"] == 4, res
    # 3 of 4 agree at the 50% call threshold.
    assert abs(res[1]["call_agree"] - 75.0) < 1e-6, res
    assert res[1]["r"] > 0.0, res

    # deconv: 3 CpGs. c1 variant + DRAGEN C>T hom (TP both). c2 variant, no SNV
    # (precision FP). c3 methylation, DRAGEN C>T hom (recall miss).
    var = os.path.join(d, "ours.5base_deconvolution.txt")
    vcf = os.path.join(d, "dragen.vcf")
    with open(var, "w") as fh:
        fh.write("# header\n# columns: ...\n")
        fh.write("chr1\t100\t+\tvariant\t0\t0\tNA\n")
        fh.write("chr1\t200\t+\tvariant\t0\t0\tNA\n")
        fh.write("chr1\t300\t+\tmethylation\t10\t10\t100.00\n")
    with open(vcf, "w") as fh:
        fh.write("##fileformat=VCFv4.2\n")
        fh.write("#CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO\tFORMAT\tS\n")
        fh.write("chr1\t100\t.\tC\tT\t50\tPASS\t.\tGT\t1/1\n")  # matches c1
        fh.write("chr1\t300\t.\tC\tT\t50\tPASS\t.\tGT\t1/1\n")  # c3 hom, we missed
    dres = cmd_deconv(argparse.Namespace(variants=var, vcf=vcf))
    assert abs(dres["precision"] - 0.5) < 1e-9, dres   # 1 of 2 variants matched
    assert abs(dres["recall"] - 0.5) < 1e-9, dres      # 1 of 2 covered homs flagged
    print("\nselftest: OK")


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd")

    m = sub.add_parser("methyl", help="per-CpG methylation concordance vs DRAGEN CX")
    m.add_argument("--ours", required=True)
    m.add_argument("--dragen", required=True)
    m.add_argument("--covs", default="1,5,10")

    dv = sub.add_parser("deconv", help="deconvolution precision/recall vs DRAGEN VCF")
    dv.add_argument("--variants", required=True)
    dv.add_argument("--vcf", required=True)

    ap.add_argument("--selftest", action="store_true", help="run the embedded self-test")
    args = ap.parse_args()

    if args.selftest:
        _selftest()
        return
    if args.cmd == "methyl":
        cmd_methyl(args)
    elif args.cmd == "deconv":
        cmd_deconv(args)
    else:
        ap.print_help()
        sys.exit(2)


if __name__ == "__main__":
    main()
