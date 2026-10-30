#!/usr/bin/env python3
"""Score several 5-Base engine arms against DRAGEN on shared CpG sets.

Pure stdlib, like concordance.py. Inputs are Bismark/DRAGEN-format cytosine
reports (chrom pos strand meth unmeth context [tri]); only CpG rows on
autosomes + X are used, with 'chr' prefixes stripped.

  compare   --dragen <CX> --arm NAME=<CpG_report> ... --baseline NAME --out <tsv>
            Per arm and cov >= 1/5/10: own set (arm & DRAGEN), common set
            (cov >= 10 in every arm and DRAGEN) and gain set (arm & DRAGEN
            at cov >= 10 where the baseline is below 10). Also per-contig
            joined counts and the fraction of DRAGEN CpGs the arm covers.
  refcheck  --dragen <CX> --fasta <fa> [--n N]
            Fraction of sampled DRAGEN CpG positions that are CpGs in the FASTA.
  --selftest
"""

import argparse
import math
import os
import random
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from concordance import _norm_chrom, _open  # noqa: E402

CONTIGS = [str(i) for i in range(1, 23)] + ["X"]
CONTIG_IDX = {c: i for i, c in enumerate(CONTIGS)}
COVS = (1, 5, 10)


def key(chrom, pos, strand):
    return (CONTIG_IDX[chrom] << 33) | (int(pos) << 1) | (strand == "-")


def contig_of(k):
    return CONTIGS[k >> 33]


def load_cpg(path):
    """Cytosine report -> {key: meth << 32 | cov} for covered CpGs on autosomes + X."""
    out = {}
    with _open(path) as fh:
        for line in fh:
            f = line.split("\t", 6)
            if len(f) < 6 or f[5] not in ("CG", "CpG"):
                continue
            c = _norm_chrom(f[0])
            if c not in CONTIG_IDX:
                continue
            meth, unmeth = int(f[3]), int(f[4])
            cov = meth + unmeth
            if cov:
                out[key(c, f[1], f[2])] = (meth << 32) | cov
    if not out:
        raise SystemExit(f"five_base_compare: no CpGs on autosomes/X in {path}")
    return out


class Acc:
    """Single-pass Pearson, coverage-weighted Pearson, mean |d|, mean d, call agreement."""

    __slots__ = ("n", "sx", "sy", "sxx", "syy", "sxy", "sw", "swx", "swy",
                 "swxx", "swyy", "swxy", "sad", "sd", "agree")

    def __init__(self):
        for s in self.__slots__:
            setattr(self, s, 0.0)

    def add(self, ov, dv):
        om, oc = ov >> 32, ov & 0xFFFFFFFF
        dm, dc = dv >> 32, dv & 0xFFFFFFFF
        x, y, w = 100.0 * om / oc, 100.0 * dm / dc, float(min(oc, dc))
        self.n += 1
        self.sx += x; self.sy += y
        self.sxx += x * x; self.syy += y * y; self.sxy += x * y
        self.sw += w; self.swx += w * x; self.swy += w * y
        self.swxx += w * x * x; self.swyy += w * y * y; self.swxy += w * x * y
        self.sad += abs(x - y); self.sd += x - y
        if (x >= 50.0) == (y >= 50.0):
            self.agree += 1

    @staticmethod
    def _r(n, sx, sy, sxx, syy, sxy):
        if n < 2:
            return float("nan")
        cov = sxy - sx * sy / n
        vx, vy = sxx - sx * sx / n, syy - sy * sy / n
        return cov / math.sqrt(vx * vy) if vx > 0 and vy > 0 else float("nan")

    def row(self):
        n = int(self.n)
        if n == 0:
            return [0, "NA", "NA", "NA", "NA", "NA"]
        r = self._r(self.n, self.sx, self.sy, self.sxx, self.syy, self.sxy)
        wr = self._r(self.sw, self.swx, self.swy, self.swxx, self.swyy, self.swxy)
        return [n, f"{r:.6f}", f"{wr:.6f}", f"{self.sad / n:.4f}",
                f"{self.sd / n:.4f}", f"{100.0 * self.agree / n:.4f}"]


def cov_of(v):
    return v & 0xFFFFFFFF


def cmd_compare(args):
    arms = []
    for spec in args.arm:
        name, _, path = spec.partition("=")
        if not path:
            raise SystemExit(f"--arm expects NAME=path, got {spec!r}")
        arms.append((name, path))
    names = [n for n, _ in arms]
    if args.baseline not in names:
        raise SystemExit(f"--baseline {args.baseline!r} is not one of the arms {names}")
    drg = load_cpg(args.dragen)
    data = {n: load_cpg(p) for n, p in arms}
    rows = []
    hdr = ["arm", "set", "cov", "n", "r", "cov_wtd_r", "mean_abs_d", "mean_signed_d",
           "call_agree_50"]

    for n in names:
        ours = data[n]
        for t in COVS:
            a = Acc()
            for k, ov in ours.items():
                dv = drg.get(k)
                if dv is not None and cov_of(ov) >= t and cov_of(dv) >= t:
                    a.add(ov, dv)
            rows.append([n, "own", t] + a.row())

    common = [k for k, dv in drg.items() if cov_of(dv) >= 10
              and all(cov_of(data[n].get(k, 0)) >= 10 for n in names)]
    for n in names:
        a = Acc()
        for k in common:
            a.add(data[n][k], drg[k])
        rows.append([n, "common", 10] + a.row())

    base = data[args.baseline]
    for n in names:
        if n == args.baseline:
            continue
        a = Acc()
        for k, ov in data[n].items():
            dv = drg.get(k)
            if dv is not None and cov_of(ov) >= 10 and cov_of(dv) >= 10 \
                    and cov_of(base.get(k, 0)) < 10:
                a.add(ov, dv)
        rows.append([n, "gain", 10] + a.row())

    with open(args.out, "w") as fh:
        fh.write("\t".join(hdr) + "\n")
        for r in rows:
            fh.write("\t".join(map(str, r)) + "\n")

    drg_cov1 = sum(1 for v in drg.values() if cov_of(v) >= 1)
    with open(args.out + ".contigs", "w") as fh:
        fh.write("arm\tcontig\tjoined_cov10\tdragen_cov10\n")
        for n in names:
            per = dict.fromkeys(CONTIGS, 0)
            dper = dict.fromkeys(CONTIGS, 0)
            for k, dv in drg.items():
                if cov_of(dv) >= 10:
                    c = contig_of(k)
                    dper[c] += 1
                    if cov_of(data[n].get(k, 0)) >= 10:
                        per[c] += 1
            for c in CONTIGS:
                fh.write(f"{n}\t{c}\t{per[c]}\t{dper[c]}\n")
    with open(args.out + ".joined", "w") as fh:
        fh.write("arm\tdragen_cpgs\tjoined_cpgs\tjoined_fraction\n")
        for n in names:
            j = sum(1 for k in drg if k in data[n])
            fh.write(f"{n}\t{drg_cov1}\t{j}\t{j / drg_cov1:.6f}\n")
    print(f"wrote {args.out} (+ .contigs, .joined); common set = {len(common)} CpGs")


def read_fasta(path, keep):
    seqs, name, buf = {}, None, []
    with _open(path) as fh:
        for line in fh:
            if line.startswith(">"):
                if name in keep:
                    seqs[name] = "".join(buf).upper()
                name, buf = _norm_chrom(line[1:].split()[0]), []
            elif name in keep:
                buf.append(line.strip())
    if name in keep:
        seqs[name] = "".join(buf).upper()
    return seqs


def cmd_refcheck(args):
    rng = random.Random(1134)
    sample, seen = [], 0
    with _open(args.dragen) as fh:
        for line in fh:
            f = line.split("\t", 6)
            if len(f) < 6 or f[5] not in ("CG", "CpG"):
                continue
            c = _norm_chrom(f[0])
            if c not in CONTIG_IDX:
                continue
            seen += 1
            item = (c, int(f[1]), f[2])
            if len(sample) < args.n:
                sample.append(item)
            else:
                j = rng.randrange(seen)
                if j < args.n:
                    sample[j] = item
    if not sample:
        raise SystemExit("refcheck: no DRAGEN CpG rows on autosomes/X")
    seqs = read_fasta(args.fasta, set(CONTIG_IDX))
    missing = sorted({c for c, _, _ in sample} - seqs.keys())
    if missing:
        raise SystemExit(f"refcheck: contigs absent from FASTA: {missing}")
    ok = 0
    for c, pos, strand in sample:
        s = seqs[c]
        if strand == "+":
            ok += s[pos - 1:pos + 1] == "CG"
        else:
            ok += pos >= 2 and s[pos - 2:pos] == "CG"
    frac = ok / len(sample)
    print(f"refcheck: {ok}/{len(sample)} sampled DRAGEN CpGs are CpGs in the FASTA "
          f"({100 * frac:.4f}%)")
    if frac < args.min_fraction:
        raise SystemExit(f"refcheck: below the {100 * args.min_fraction:.1f}% threshold")


def _selftest():
    d = tempfile.mkdtemp(prefix="fbc_selftest_")

    def write(name, rows):
        p = os.path.join(d, name)
        with open(p, "w") as fh:
            for r in rows:
                fh.write("\t".join(map(str, r)) + "\tCGA\n")
        return p

    # chrom pos strand meth unmeth context
    drg = write("drg.txt", [
        ("chr1", 10, "+", 10, 0, "CG"),   # in A and B at cov >= 10 -> common
        ("chr1", 11, "-", 0, 12, "CG"),   # in A and B at cov >= 10 -> common
        ("chr1", 20, "+", 6, 6, "CG"),    # B only at cov >= 10 -> gain for B
        ("chr2", 5, "+", 3, 0, "CG"),     # low DRAGEN cov -> no cov10 set
        ("chrM", 1, "+", 9, 9, "CG"),     # excluded contig
        ("chr1", 30, "+", 9, 9, "CHG"),   # not CpG
    ])
    a = write("a.txt", [
        ("1", 10, "+", 10, 0, "CG"),
        ("1", 11, "-", 1, 11, "CG"),
        ("1", 20, "+", 2, 2, "CG"),
        ("2", 5, "+", 3, 0, "CG"),
    ])
    b = write("b.txt", [
        ("1", 10, "+", 9, 1, "CG"),
        ("1", 11, "-", 0, 10, "CG"),
        ("1", 20, "+", 5, 5, "CG"),
    ])
    out = os.path.join(d, "m.tsv")
    cmd_compare(argparse.Namespace(dragen=drg, arm=[f"A={a}", f"B={b}"],
                                   baseline="A", out=out))
    rows = {}
    with open(out) as fh:
        next(fh)
        for line in fh:
            f = line.rstrip("\n").split("\t")
            rows[(f[0], f[1], f[2])] = f
    assert rows[("A", "common", "10")][3] == "2", rows[("A", "common", "10")]
    assert rows[("B", "common", "10")][3] == "2"
    assert rows[("B", "gain", "10")][3] == "1", rows[("B", "gain", "10")]
    assert rows[("A", "own", "1")][3] == "4"
    assert rows[("B", "own", "10")][3] == "3"
    # A common: arm 100, 8.33 vs DRAGEN 100, 0 -> mean signed d = 4.1667
    assert abs(float(rows[("A", "common", "10")][7]) - 100.0 / 24) < 1e-3
    joined = open(out + ".joined").read()
    assert "A\t4\t4\t1.000000" in joined and "B\t4\t3\t0.750000" in joined, joined

    fa = os.path.join(d, "g.fa")
    with open(fa, "w") as fh:
        fh.write(">1\n" + "A" * 9 + "CG" + "A" * 7 + "CG" + "A" * 10 + "\n>2\nAAAACGAA\n")
    refcheck_drg = write("rc.txt", [("chr1", 10, "+", 1, 0, "CG"), ("chr1", 11, "-", 1, 0, "CG"),
                                    ("chr1", 19, "+", 1, 0, "CG"), ("chr2", 5, "+", 1, 0, "CG")])
    cmd_refcheck(argparse.Namespace(dragen=refcheck_drg, fasta=fa, n=10, min_fraction=0.999))
    print("selftest OK")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd")
    c = sub.add_parser("compare")
    c.add_argument("--dragen", required=True)
    c.add_argument("--arm", action="append", required=True, help="NAME=cytosine_report")
    c.add_argument("--baseline", required=True)
    c.add_argument("--out", required=True)
    r = sub.add_parser("refcheck")
    r.add_argument("--dragen", required=True)
    r.add_argument("--fasta", required=True)
    r.add_argument("--n", type=int, default=1_000_000)
    r.add_argument("--min-fraction", type=float, default=0.999)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        _selftest()
    elif args.cmd == "compare":
        cmd_compare(args)
    elif args.cmd == "refcheck":
        cmd_refcheck(args)
    else:
        ap.print_help()
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
