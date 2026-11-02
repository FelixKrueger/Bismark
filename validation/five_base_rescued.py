#!/usr/bin/env python3
"""Rescued-pair, duplex and verdict checks for the 5-Base Bowtie 2 defaults run.

Pure stdlib, like five_base_compare.py; BAMs are read through `samtools view`.
Pre-registration: plans/11012026_5base-bowtie2-defaults/PREREGISTRATION.md.

  rescued    --fastq <R1> --old <B-old _pe.bam> --new <B-new _pe.bam>
             --dragen <CpG-only CX> --out <dir> [--seed 1146]
             Joins both raw BAMs in FASTQ order into rescued / changed / rest
             pairs and writes <dir>/rescued.json: per-set geometry, methylation,
             R1 3' M-bias, overhang-vs-control tallies (R4), per-call DRAGEN
             agreement (R5) and duplex-family partner fractions (D1).
  consensus  --bam <five_base_consensus.bam> --log <consensus log>
             --dragen <CpG-only CX> --out <json>
  verdict    --sample S --rescued <json> --old-dir <B-old cell> --new-dir <B-new cell>
             --score <score.tsv> [--old-consensus <json> --new-consensus <json>]
             --out <tsv>
             Applies the pre-registered criteria; exits 1 unless every one passes.
  --selftest
"""

import argparse
import heapq
import json
import os
import random
import re
import subprocess
import sys
import tempfile
from array import array

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from five_base_compare import CONTIG_IDX, key as site_key  # noqa: E402
from concordance import _norm_chrom, _open  # noqa: E402

CIGAR_RE = re.compile(r"(\d+)([MIDNSHP=X])")
MD_RE = re.compile(r"(\d+)|\^([A-Z]+)|([A-Z])")
# Rest pairs below these priorities feed the R4 control and the R5 sample pool.
CONTROL_FRAC = 0.01
POOL_FRAC = 0.05
MBIAS_CYCLES = 10
MIN_N = 10_000
SETS = ("rescued", "changed", "rest")


# ---------------------------------------------------------------- SAM parsing

class Mate:
    __slots__ = ("name", "flag", "rname", "pos", "mapq", "cigar", "pnext", "tlen",
                 "seqlen", "xm", "md", "rx", "_span")

    def __init__(self, line):
        f = line.rstrip("\n").split("\t")
        self.name = f[0]
        self.flag = int(f[1])
        self.rname = f[2]
        self.pos = int(f[3])
        self.mapq = int(f[4])
        self.cigar = f[5]
        self.pnext = int(f[7])
        self.tlen = int(f[8])
        self.seqlen = len(f[9])
        self.xm = self.md = self.rx = None
        for t in f[11:]:
            tag = t[:2]
            if tag == "XM":
                self.xm = t[5:]
            elif tag == "MD":
                self.md = t[5:]
            elif tag == "RX":
                self.rx = t[5:]
        if self.xm is None:
            raise SystemExit(f"five_base_rescued: record {self.name} has no XM tag")
        self._span = None

    @property
    def reverse(self):
        return self.flag & 0x10 != 0

    def span(self):
        """1-based inclusive reference span."""
        if self._span is None:
            c = self.cigar
            if c[:-1].isdigit() and c[-1] == "M":
                n = int(c[:-1])
            else:
                n = sum(int(k) for k, op in CIGAR_RE.findall(c) if op in "MDN=X")
            self._span = (self.pos, self.pos + n - 1)
        return self._span

    def aligned(self):
        """(read index, 1-based reference position) for every aligned base."""
        c = self.cigar
        if c[:-1].isdigit() and c[-1] == "M":
            return zip(range(self.seqlen), range(self.pos, self.pos + self.seqlen))
        out, i, r = [], 0, self.pos
        for k, op in CIGAR_RE.findall(c):
            k = int(k)
            if op in "M=X":
                out.extend(zip(range(i, i + k), range(r, r + k)))
                i += k
                r += k
            elif op in "IS":
                i += k
            elif op in "DN":
                r += k
        return out

    def mismatches(self):
        """1-based reference positions of MD mismatches."""
        if self.md is None:
            raise SystemExit(f"five_base_rescued: record {self.name} has no MD tag")
        out, r = set(), self.pos
        for num, dele, mm in MD_RE.findall(self.md):
            if num:
                r += int(num)
            elif dele:
                r += len(dele)
            else:
                out.add(r)
                r += 1
        return out

    def cycle(self, i):
        """Read position counted from the read's 3' end (0 = last sequenced base)."""
        return i if self.reverse else self.seqlen - 1 - i


def sam_pairs(bam):
    """Yield (R1, R2) mates of a Bismark PE BAM in file order."""
    p = subprocess.Popen(["samtools", "view", "-@", "2", bam], stdout=subprocess.PIPE,
                         text=True, bufsize=1 << 20)
    it = iter(p.stdout)
    for line in it:
        a = Mate(line)
        try:
            b = Mate(next(it))
        except StopIteration:
            raise SystemExit(f"five_base_rescued: {bam} ends with an unpaired record {a.name}")
        if a.name != b.name or not (a.flag & 0x40 and b.flag & 0x80):
            raise SystemExit(f"five_base_rescued: {bam}: records {a.name}/{b.name} are not an "
                             "R1/R2 pair")
        yield a, b
    if p.wait() != 0:
        raise SystemExit(f"five_base_rescued: samtools view {bam} exited {p.returncode}")


def fastq_names(path):
    """Read names of a FASTQ (gz or plain), cut at whitespace, '@' and /1 removed."""
    with _open(path) as fh:
        for n, line in enumerate(fh):
            if n % 4 == 0:
                name = line[1:].split(None, 1)[0]
                if name.endswith("/1"):
                    name = name[:-2]
                yield name


# ---------------------------------------------------------------- per-pair work

def geometry(r1, r2):
    """('dovetailed'|'contained'|'normal', overhang length) in reference coordinates."""
    f, r = (r2, r1) if r1.reverse else (r1, r2)
    (fs, fe), (rs, re_) = f.span(), r.span()
    if rs < fs or fe > re_:
        over = sum(1 for _, p in f.aligned() if p > re_) + sum(1 for _, p in r.aligned() if p < fs)
        return "dovetailed", over
    if (fs <= rs and re_ <= fe) or (rs <= fs and fe <= re_):
        return "contained", 0
    return "normal", 0


def overhang_bases(r1, r2):
    """Per mate: the set of read indices that are overhang bases."""
    f, r = (r2, r1) if r1.reverse else (r1, r2)
    (fs, _), (_, re_) = f.span(), r.span()
    fo = {i for i, p in f.aligned() if p > re_}
    ro = {i for i, p in r.aligned() if p < fs}
    return (ro, fo) if r1.reverse else (fo, ro)


class CycleTally:
    """Per-cycle CHG/CHH calls and uncalled-base mismatches."""

    FIELDS = ("chg_m", "chg_t", "chh_m", "chh_t", "mm", "unc")

    def __init__(self):
        self.t = {k: {} for k in self.FIELDS}

    def _inc(self, field, c):
        d = self.t[field]
        d[c] = d.get(c, 0) + 1

    def add(self, mate, keep):
        """Tally the bases of `mate` whose read index passes `keep`."""
        mm = mate.mismatches()
        xm = mate.xm
        for i, p in mate.aligned():
            if not keep(i):
                continue
            c = mate.cycle(i)
            x = xm[i]
            if x == "X" or x == "x":
                self._inc("chg_t", c)
                if x == "X":
                    self._inc("chg_m", c)
            elif x == "H" or x == "h":
                self._inc("chh_t", c)
                if x == "H":
                    self._inc("chh_m", c)
            elif x == ".":
                self._inc("unc", c)
                if p in mm:
                    self._inc("mm", c)

    def to_json(self):
        return {k: {str(c): n for c, n in sorted(v.items())} for k, v in self.t.items()}


class SetStats:
    def __init__(self):
        self.pairs = 0
        self.shape = {"dovetailed": 0, "contained": 0, "normal": 0}
        self.overhang = array("I")
        self.calls = dict.fromkeys("ZzXxHh", 0)
        self.mbias = [[0, 0] for _ in range(MBIAS_CYCLES)]  # [methylated, total] CpG

    def add(self, r1, r2):
        self.pairs += 1
        shape, over = geometry(r1, r2)
        self.shape[shape] += 1
        if shape == "dovetailed":
            self.overhang.append(over)
        for m in (r1, r2):
            for ch in "ZzXxHh":
                self.calls[ch] += m.xm.count(ch)
        tail = r1.xm[:MBIAS_CYCLES] if r1.reverse else r1.xm[-MBIAS_CYCLES:][::-1]
        for c, x in enumerate(tail):
            if x == "Z":
                self.mbias[c][0] += 1
                self.mbias[c][1] += 1
            elif x == "z":
                self.mbias[c][1] += 1

    def to_json(self):
        ov = sorted(self.overhang)

        def q(f):
            return ov[min(len(ov) - 1, int(len(ov) * f))] if ov else None
        return {"pairs": self.pairs, "shape": self.shape,
                "overhang": {"n": len(ov), "p10": q(0.1), "p50": q(0.5), "p90": q(0.9),
                             "max": ov[-1] if ov else None},
                "calls": self.calls, "mbias_r1_3p": self.mbias}


def cpg_calls(r1, r2, out):
    """Append each CpG call of the pair to `out` as site_key << 1 | methylated."""
    chrom = _norm_chrom(r1.rname)
    if chrom not in CONTIG_IDX:
        return
    strand = "-" if r1.reverse else "+"
    for m in (r1, r2):
        xm = m.xm
        if "Z" not in xm and "z" not in xm:
            continue
        for i, p in m.aligned():
            x = xm[i]
            if x == "Z" or x == "z":
                out.append(site_key(chrom, p, strand) << 1 | (x == "Z"))


def family_key(r1, r2):
    """The duplex family key of five_base_duplex.rs: outer span + canonical RX."""
    start = min(r1.pos, r2.pos) - 1
    umi = (r1.rx or "").upper()
    if "+" in umi:
        a, b = umi.split("+", 1)
        umi = min(umi, f"{b}+{a}")
    return f"{r1.rname}\t{start}\t{start + abs(r1.tlen)}\t{umi}"


def same_alignment(a, b):
    return all(x.pos == y.pos and x.cigar == y.cigar for x, y in zip(a, b))


# ---------------------------------------------------------------- rescued

def cmd_rescued(args):
    os.makedirs(args.out, exist_ok=True)
    stats = {s: SetStats() for s in SETS}
    over_t, ctrl_t = CycleTally(), CycleTally()
    rescued_calls = array("q")
    pool_calls, pool = array("q"), []  # pool: (priority, start, end) into pool_calls
    rng = random.Random(args.seed)
    lost = new_pairs = old_pairs = fastq_pairs = 0
    fam_path = os.path.join(args.out, "families.unsorted")
    old_it, new_it = sam_pairs(args.old), sam_pairs(args.new)
    old, new = next(old_it, None), next(new_it, None)
    with open(fam_path, "w") as fam:
        for name in fastq_names(args.fastq):
            fastq_pairs += 1
            o = old if old is not None and old[0].name == name else None
            n = new if new is not None and new[0].name == name else None
            if o is not None:
                old_pairs += 1
                old = next(old_it, None)
            if n is None:
                if o is not None:
                    lost += 1
                continue
            new_pairs += 1
            new = next(new_it, None)
            r1, r2 = n
            if o is None:
                label = "rescued"
            elif not same_alignment(o, n):
                label = "changed"
            else:
                label = "rest"
            stats[label].add(r1, r2)
            fam.write(f"{family_key(r1, r2)}\t{'OT' if not r1.reverse else 'OB'}\t{label}\n")
            if label == "rescued":
                for m, ob in zip(n, overhang_bases(r1, r2)):
                    if ob:
                        over_t.add(m, ob.__contains__)
                cpg_calls(r1, r2, rescued_calls)
            elif label == "rest":
                pr = rng.random()
                if pr < CONTROL_FRAC:
                    for m, ob in zip(n, overhang_bases(r1, r2)):
                        ctrl_t.add(m, lambda i, ob=ob: i not in ob)
                if pr < POOL_FRAC:
                    s = len(pool_calls)
                    cpg_calls(r1, r2, pool_calls)
                    pool.append((pr, s, len(pool_calls)))
    if old is not None or new is not None:
        left = (old or new)[0].name
        raise SystemExit(f"five_base_rescued: BAM record {left} is not in the FASTQ, or the BAMs "
                         "are not in FASTQ order")

    # R5: per-call agreement, rescued vs an equal-sized random rest sample.
    k = stats["rescued"].pairs
    if k > len(pool):
        raise SystemExit(f"five_base_rescued: {k} rescued pairs exceed the {len(pool)}-pair rest "
                         f"pool (POOL_FRAC {POOL_FRAC})")
    sample_calls = array("q")
    for _, s, e in heapq.nsmallest(k, pool):
        sample_calls.extend(pool_calls[s:e])
    del pool_calls, pool
    r5 = per_call_agreement(args.dragen, {"rescued": rescued_calls, "rest_sample": sample_calls})

    families = family_partners(fam_path, args.out)
    os.remove(fam_path)
    out = {"seed": args.seed, "control_frac": CONTROL_FRAC, "pool_frac": POOL_FRAC,
           "fastq_pairs": fastq_pairs, "old_pairs": old_pairs, "new_pairs": new_pairs,
           "lost_pairs": lost, "sets": {s: stats[s].to_json() for s in SETS},
           "r4": {"overhang": over_t.to_json(), "control": ctrl_t.to_json()},
           "r5": r5, "families": families}
    with open(os.path.join(args.out, "rescued.json"), "w") as fh:
        json.dump(out, fh, indent=1)
    print(f"rescued {k} / {new_pairs} pairs; changed {stats['changed'].pairs}; lost {lost}")


def per_call_agreement(dragen, call_sets):
    """Mean of p (methylated call) or 1 - p (unmethylated) at DRAGEN cov >= 10 sites."""
    needed = set()
    for calls in call_sets.values():
        needed.update(c >> 1 for c in calls)
    p = {}
    with _open(dragen) as fh:
        for line in fh:
            f = line.split("\t", 6)
            if len(f) < 6 or f[5] not in ("CG", "CpG"):
                continue
            c = _norm_chrom(f[0])
            if c not in CONTIG_IDX:
                continue
            m, u = int(f[3]), int(f[4])
            if m + u >= 10:
                k = site_key(c, f[1], f[2])
                if k in needed:
                    p[k] = m / (m + u)
    out = {}
    for name, calls in call_sets.items():
        n, s = 0, 0.0
        for c in calls:
            v = p.get(c >> 1)
            if v is not None:
                n += 1
                s += v if c & 1 else 1.0 - v
        out[name] = {"calls": len(calls), "n": n, "mean_score": s / n if n else None}
    return out


def family_partners(path, outdir):
    """Families from the per-pair key lines; per-set fraction of pairs with a partner."""
    srt = path + ".sorted"
    env = dict(os.environ, LC_ALL="C")
    subprocess.run(["sort", "-S", "4G", "--parallel", "4", "-T", outdir, "-o", srt, path],
                   check=True, env=env)
    fam = {"total": 0, "duplex": 0, "simplex": 0}
    part = {s: [0, 0] for s in SETS}  # [partnered pairs, pairs]

    def close(labels, strands):
        if not labels:
            return
        fam["total"] += 1
        paired = len(strands) == 2
        fam["duplex" if paired else "simplex"] += 1
        for lab in labels:
            part[lab][1] += 1
            part[lab][0] += paired

    cur, labels, strands = None, [], set()
    with open(srt) as fh:
        for line in fh:
            k, strand, lab = line.rstrip("\n").rsplit("\t", 2)
            if k != cur:
                close(labels, strands)
                cur, labels, strands = k, [], set()
            labels.append(lab)
            strands.add(strand)
    close(labels, strands)
    os.remove(srt)
    return {"families": fam,
            "partnered": {s: {"partnered": a, "pairs": b, "fraction": a / b if b else None}
                          for s, (a, b) in part.items()}}


# ---------------------------------------------------------------- consensus

def cmd_consensus(args):
    calls = {"+": dict.fromkeys("ZzXxHh", 0), "-": dict.fromkeys("ZzXxHh", 0)}
    records, names = 0, set()
    p = subprocess.Popen(["samtools", "view", args.bam], stdout=subprocess.PIPE, text=True)
    for line in p.stdout:
        f = line.split("\t")
        records += 1
        names.add(f[0])
        xm = next((t[5:].rstrip("\n") for t in f[11:] if t.startswith("XM:Z:")), None)
        if xm is None:
            raise SystemExit(f"five_base_rescued: consensus record {f[0]} has no XM tag")
        strand = "-" if int(f[1]) & 0x10 else "+"
        for ch in "ZzXxHh":
            calls[strand][ch] += xm.count(ch)
    if p.wait() != 0:
        raise SystemExit(f"five_base_rescued: samtools view {args.bam} exited {p.returncode}")
    log = open(args.log).read()
    dpx = re.search(r"duplex consensus \(PE\): (\d+) consensus read\(s\) emitted, (\d+) "
                    r"family\(ies\) skipped", log)
    spx = re.search(r"simplex consensus \(PE\): (\d+) consensus read\(s\) emitted, (\d+) "
                    r"family\(ies\) skipped, of (\d+) single-strand", log)
    if not dpx or not spx:
        raise SystemExit(f"five_base_rescued: no duplex and simplex summary lines in {args.log}")
    drg = {"+": [0, 0], "-": [0, 0]}
    with _open(args.dragen) as fh:
        for line in fh:
            f = line.split("\t", 6)
            if len(f) >= 6 and f[5] in ("CG", "CpG") and f[2] in drg \
                    and _norm_chrom(f[0]) in CONTIG_IDX:
                drg[f[2]][0] += int(f[3])
                drg[f[2]][1] += int(f[3]) + int(f[4])

    def pct(m, u):
        return 100.0 * m / (m + u) if m + u else None
    tot = {ch: calls["+"][ch] + calls["-"][ch] for ch in "ZzXxHh"}
    out = {"records": records, "families_in_bam": len(names),
           "tool_duplex_families": int(dpx.group(1)) + int(dpx.group(2)),
           "tool_duplex_skipped": int(dpx.group(2)),
           "tool_simplex_families": int(spx.group(3)),
           "calls": calls,
           "cpg_pct": pct(tot["Z"], tot["z"]), "chg_pct": pct(tot["X"], tot["x"]),
           "chh_pct": pct(tot["H"], tot["h"]),
           "cpg_pct_strand": {s: pct(calls[s]["Z"], calls[s]["z"]) for s in "+-"},
           "dragen_cpg_pct_strand": {s: pct(drg[s][0], drg[s][1] - drg[s][0]) for s in "+-"}}
    with open(args.out, "w") as fh:
        json.dump(out, fh, indent=1)
    print(f"consensus: {records} records, {out['tool_duplex_families']} duplex families")


# ---------------------------------------------------------------- verdict

def report_counts(path):
    """Methylated / unmethylated C counts per context from a Bismark PE report."""
    out = {}
    for line in open(path):
        m = re.match(r"Total (methylated|unmethylated) C's in (CpG|CHG|CHH) context:\t(\d+)", line)
        if m:
            out[(m.group(2), m.group(1))] = int(m.group(3))
    if len(out) != 6:
        raise SystemExit(f"five_base_rescued: incomplete methylation counts in {path}")
    return {c: 100.0 * out[(c, "methylated")] / (out[(c, "methylated")] +
                                                 out[(c, "unmethylated")])
            for c in ("CpG", "CHG", "CHH")}


def deconv_counts(path):
    text = open(path).read()
    p = re.search(r"precision \(any DRAGEN C>T/G>A\):\s+(\d+)/(\d+)", text)
    r = re.search(r"recall:\s+(\d+)/(\d+)", text)
    if not p or not r:
        raise SystemExit(f"five_base_rescued: no precision/recall in {path}")
    return 100.0 * int(p.group(1)) / int(p.group(2)), 100.0 * int(r.group(1)) / int(r.group(2))


def metrics(path):
    rows = [line.rstrip("\n").split("\t") for line in open(path)]
    return dict(zip(rows[0], rows[1]))


def one(pattern, d):
    hits = [os.path.join(d, f) for f in sorted(os.listdir(d)) if re.search(pattern, f)]
    if len(hits) != 1:
        raise SystemExit(f"five_base_rescued: expected one {pattern} in {d}, found {hits}")
    return hits[0]


def pct_of(calls, m, u):
    return 100.0 * calls[m] / (calls[m] + calls[u]) if calls[m] + calls[u] else float("nan")


def matched(tally_o, tally_c, num, den):
    """Overhang rate and the control rate re-weighted to the overhang cycle mix."""
    on, od, cw, dropped = 0, 0, 0.0, 0
    for c, d in tally_o[den].items():
        cd = tally_c[den].get(c, 0)
        if cd == 0:
            dropped += d
            continue
        on += tally_o[num].get(c, 0)
        od += d
        cw += d * tally_c[num].get(c, 0) / cd
    if od == 0:
        return float("nan"), float("nan"), 0, dropped
    return 100.0 * on / od, 100.0 * cw / od, od, dropped


def criteria(sample, rj, old_dir, new_dir, score, old_cons=None, new_cons=None):
    """[(criterion, value, threshold, status)] for one sample."""
    rows = []

    def add(name, value, threshold, ok, n_ok=True):
        rows.append((name, value, threshold,
                     "PASS" if ok and n_ok else ("INSUFFICIENT" if not n_ok else "FAIL")))

    S = rj["sets"]
    res, rest = S["rescued"], S["rest"]
    frac = 100.0 * res["pairs"] / rj["new_pairs"]
    add("R1 rescued % of B-new pairs", f"{frac:.4f}", "0.1-3", 0.1 <= frac <= 3.0)
    for ctx, (m, u) in (("CHG", "Xx"), ("CHH", "Hh")):
        a, b = pct_of(res["calls"], m, u), pct_of(rest["calls"], m, u)
        add(f"R2 rescued {ctx} %", f"{a:.4f} vs rest {b:.4f}", "<= rest + 0.5", a <= b + 0.5)
    a, b = pct_of(res["calls"], "Z", "z"), pct_of(rest["calls"], "Z", "z")
    add("R3 rescued CpG %", f"{a:.4f} vs rest {b:.4f}", "within 5", abs(a - b) <= 5.0)
    o, c = rj["r4"]["overhang"], rj["r4"]["control"]
    o = {k: {int(x): n for x, n in v.items()} for k, v in o.items()}
    c = {k: {int(x): n for x, n in v.items()} for k, v in c.items()}
    for ctx, num, den in (("CHG", "chg_m", "chg_t"), ("CHH", "chh_m", "chh_t")):
        ov, cv, n, drop = matched(o, c, num, den)
        add(f"R4 overhang {ctx} %", f"{ov:.4f} vs control {cv:.4f} (n {n}, unmatched {drop})",
            "<= control + 1.0", ov <= cv + 1.0, n >= MIN_N)
    ov, cv, n, drop = matched(o, c, "mm", "unc")
    add("R4 overhang mismatch %", f"{ov:.4f} vs control {cv:.4f} (n {n}, unmatched {drop})",
        "<= 2x control", ov <= 2 * cv, n >= MIN_N)
    r5 = rj["r5"]
    a, b = r5["rescued"]["mean_score"], r5["rest_sample"]["mean_score"]
    n = r5["rescued"]["n"]
    add("R5 per-call DRAGEN agreement", f"{a} vs rest sample {b} (n {n})", ">= rest - 0.02",
        a is not None and b is not None and a >= b - 0.02, n >= MIN_N)

    def mb(s):
        m = [x for x, t in s["mbias_r1_3p"] if t]
        t = [t for _, t in s["mbias_r1_3p"] if t]
        return sum(100.0 * x / y for x, y in zip(m, t)) / len(t) if t else float("nan")
    a, b = mb(res), mb(rest)
    add("R6 R1 3' M-bias CpG %", f"{a:.4f} vs rest {b:.4f}", "within 3", abs(a - b) <= 3.0)

    # Secondary, genome-wide.
    sc = {}
    for line in open(score):
        f = line.rstrip("\n").split("\t")
        sc[(f[0], f[1], f[2])] = f
    r_old, r_new = float(sc[("B-old", "common", "10")][4]), float(sc[("B-new", "common", "10")][4])
    add("S common-set delta r", f"{r_new - r_old:.6f}", ">= -0.001", r_new - r_old >= -0.001)
    po = report_counts(one(r"_PE_report\.txt$", old_dir))
    pn = report_counts(one(r"_PE_report\.txt$", new_dir))
    for ctx in ("CHG", "CHH"):
        add(f"S {ctx} %", f"{pn[ctx]:.4f} vs B-old {po[ctx]:.4f}", "<= B-old + 0.2",
            pn[ctx] <= po[ctx] + 0.2)
    mo, mn = metrics(os.path.join(old_dir, "metrics.tsv")), metrics(os.path.join(new_dir,
                                                                                 "metrics.tsv"))
    qo = 100.0 * int(mo["mapq0"]) / int(mo["records"])
    qn = 100.0 * int(mn["mapq0"]) / int(mn["records"])
    add("S MAPQ-0 %", f"{qn:.4f} vs B-old {qo:.4f}", "<= B-old + 2", qn <= qo + 2.0)
    do, dn = deconv_counts(os.path.join(old_dir, "deconv.txt")), deconv_counts(
        os.path.join(new_dir, "deconv.txt"))
    for i, name in enumerate(("precision", "recall")):
        add(f"S deconvolution {name} delta", f"{dn[i] - do[i]:.4f}", ">= -1", dn[i] - do[i] >= -1)

    if old_cons is not None:
        fams = rj["families"]
        tool, ours = new_cons["tool_duplex_families"], fams["families"]["duplex"]
        dev = abs(ours - tool) / tool if tool else float("inf")
        add("D0 helper duplex families vs tool", f"{ours} vs {tool} ({100 * dev:.4f}%)",
            "within 0.1%", dev <= 0.001)
        p = fams["partnered"]
        a, b = p["rescued"]["fraction"], p["rest"]["fraction"]
        add("D1 rescued partnered %", f"{100 * a:.4f} vs rest {100 * b:.4f}", ">= rest - 5",
            a is not None and b is not None and 100 * a >= 100 * b - 5.0)
        add("D2 consensus CpG %", f"{new_cons['cpg_pct']:.4f} vs B-old {old_cons['cpg_pct']:.4f}",
            "within 0.5", abs(new_cons["cpg_pct"] - old_cons["cpg_pct"]) <= 0.5)
        for ctx in ("chg", "chh"):
            a, b = new_cons[f"{ctx}_pct"], old_cons[f"{ctx}_pct"]
            add(f"D3 consensus {ctx.upper()} %", f"{a:.4f} vs B-old {b:.4f}", "<= B-old + 0.2",
                a <= b + 0.2)
    return [(sample,) + r for r in rows]


def cmd_verdict(args):
    rj = json.load(open(args.rescued))
    oc = json.load(open(args.old_consensus)) if args.old_consensus else None
    nc = json.load(open(args.new_consensus)) if args.new_consensus else None
    if (oc is None) != (nc is None):
        raise SystemExit("five_base_rescued: give both --old-consensus and --new-consensus")
    rows = criteria(args.sample, rj, args.old_dir, args.new_dir, args.score, oc, nc)
    with open(args.out, "w") as fh:
        fh.write("sample\tcriterion\tvalue\tthreshold\tstatus\n")
        for r in rows:
            fh.write("\t".join(map(str, r)) + "\n")
    for r in rows:
        print("\t".join(map(str, r)))
    return 0 if all(r[-1] == "PASS" for r in rows) else 1


# ---------------------------------------------------------------- selftest

def _sam(name, flag, pos, cigar, pnext, tlen, seq, xm, md, rx="CAGTAGT+CACCTAC"):
    tags = [f"XM:Z:{xm}", f"MD:Z:{md}"] + ([f"RX:Z:{rx}"] if rx else [])
    return "\t".join([name, str(flag), "1", str(pos), "40", cigar, "=", str(pnext), str(tlen),
                      seq, "I" * len(seq)] + tags) + "\n"


def _selftest():
    global CONTROL_FRAC, POOL_FRAC
    CONTROL_FRAC = POOL_FRAC = 1.0  # every rest pair is control and pool at this size
    d = tempfile.mkdtemp(prefix="fbr_selftest_")
    L = 10
    s = "A" * L

    # Geometry: OT dovetail (R2 starts 3 bp before R1), OB dovetail, contained, normal.
    ot1 = Mate(_sam("p", 99, 104, "10M", 101, 13, s, "Z.x......h", "0A9"))
    ot2 = Mate(_sam("p", 147, 101, "10M", 104, -13, s, "z.........", "10"))
    assert geometry(ot1, ot2) == ("dovetailed", 6), geometry(ot1, ot2)
    # R1 overhang = R1 bases at ref > 110 = read idx 7..9; R2 overhang = ref < 104 = idx 0..2.
    assert overhang_bases(ot1, ot2) == ({7, 8, 9}, {0, 1, 2})
    ob1 = Mate(_sam("q", 83, 201, "10M", 205, -14, s, "..........", "10"))
    ob2 = Mate(_sam("q", 163, 205, "10M", 201, 14, s, "..........", "10"))
    assert geometry(ob1, ob2) == ("dovetailed", 8), geometry(ob1, ob2)
    co1 = Mate(_sam("c", 99, 300, "10M", 300, 16, s, "..........", "10"))
    co2 = Mate(_sam("c", 147, 300, "16M", 300, -16, "A" * 16, "." * 16, "16"))
    assert geometry(co1, co2) == ("contained", 0)
    # F containing R but ending past it overhangs by the definition: F bases 308-309.
    in2 = Mate(_sam("c", 147, 302, "6M", 300, -10, s[:6], "......", "6"))
    assert geometry(co1, in2) == ("dovetailed", 2), geometry(co1, in2)
    no1 = Mate(_sam("n", 99, 400, "10M", 420, 30, s, "..........", "10"))
    no2 = Mate(_sam("n", 147, 420, "10M", 400, -30, s, "..........", "10"))
    assert geometry(no1, no2) == ("normal", 0)

    # Cycles, CIGAR walk, MD mismatches (called positions excluded by the tally).
    assert ot1.cycle(9) == 0 and ot2.cycle(0) == 0
    ind = Mate(_sam("i", 99, 10, "2S3M1I2M1D2M", 30, 20, s, "..........", "4A^G0C1"))
    assert list(ind.aligned()) == [(2, 10), (3, 11), (4, 12), (6, 13), (7, 14), (8, 16), (9, 17)]
    assert ind.mismatches() == {14, 16}, ind.mismatches()
    assert ind.span() == (10, 17)
    t = CycleTally()
    mm = Mate(_sam("m", 99, 1, "10M", 20, 29, s, "X.........", "0A1C7"))
    t.add(mm, lambda i: True)
    assert t.t["chg_t"] == {9: 1} and t.t["chg_m"] == {9: 1}
    assert t.t["mm"] == {7: 1} and t.t["unc"][7] == 1 and 9 not in t.t["unc"], t.t

    # Family key: swapped dual UMIs collapse; strand partner in the opposite molecule strand.
    fa = family_key(ot1, ot2)
    swapped = Mate(_sam("r", 83, 101, "10M", 104, -13, s, "..........", "10", "CACCTAC+CAGTAGT"))
    assert family_key(swapped, ot1) == fa, (family_key(swapped, ot1), fa)
    assert fa == "1\t100\t113\tCACCTAC+CAGTAGT"

    # End-to-end on hand-built BAMs: rescued (dovetailed OT), changed, rest, lost.
    sam_hdr = "@HD\tVN:1.6\tSO:unsorted\n@SQ\tSN:1\tLN:100000\n"
    fq = os.path.join(d, "r1.fq")
    names = ["dove", "chg", "keep", "lost", "partner", "keep2"]
    with open(fq, "w") as fh:
        for n in names:
            fh.write(f"@{n} 1:N:0:ACGT\n{s}\n+\n{'I' * L}\n")

    def pair(n, a, b, flags=(99, 147), rx="CAGTAGT+CACCTAC"):
        (p1, c1, x1, m1), (p2, c2, x2, m2) = a, b
        span = max(p1, p2) + L - min(p1, p2)
        t1 = span if flags[0] == 99 else -span
        return (_sam(n, flags[0], p1, c1, p2, t1, s, x1, m1, rx) +
                _sam(n, flags[1], p2, c2, p1, -t1, s, x2, m2, rx))
    keep = pair("keep", (1000, "10M", "Z.........", "10"), (1020, "10M", "z.........", "10"))
    keep2 = pair("keep2", (2000, "10M", "X.........", "0A9"), (2020, "10M", "..........", "10"))
    old_rows = [pair("chg", (500, "10M", "..........", "10"), (520, "10M", "..........", "10")),
                keep, pair("lost", (700, "10M", "..........", "10"),
                           (720, "10M", "..........", "10")), keep2]
    new_rows = [pair("dove", (104, "10M", "Z.x......h", "0A9"), (101, "10M", "z.........", "10")),
                pair("chg", (501, "10M", "..........", "10"), (520, "10M", "..........", "10")),
                keep,
                pair("partner", (101, "10M", "Z.........", "10"), (104, "10M", "..........", "10"),
                     flags=(83, 163), rx="CACCTAC+CAGTAGT"),
                keep2]

    def bam(name, rows):
        sam = os.path.join(d, name + ".sam")
        with open(sam, "w") as fh:
            fh.write(sam_hdr + "".join(rows))
        out = os.path.join(d, name + ".bam")
        subprocess.run(["samtools", "view", "-b", "-o", out, sam], check=True)
        return out
    drg = os.path.join(d, "drg.txt")
    with open(drg, "w") as fh:
        fh.write("chr1\t104\t+\t9\t1\tCG\tCGA\n")    # dove R1 Z at 104: score 0.9
        fh.write("chr1\t101\t+\t2\t8\tCG\tCGA\n")    # dove R2 z at 101: score 0.8
        fh.write("chr1\t1000\t+\t10\t0\tCG\tCGA\n")  # keep R1 Z: score 1.0
        fh.write("chr1\t1020\t+\t3\t3\tCG\tCGA\n")   # cov 6: not used
    out = os.path.join(d, "out")
    cmd_rescued(argparse.Namespace(fastq=fq, old=bam("old", old_rows), new=bam("new", new_rows),
                                   dragen=drg, out=out, seed=1146))
    rj = json.load(open(os.path.join(out, "rescued.json")))
    S = rj["sets"]
    assert (S["rescued"]["pairs"], S["changed"]["pairs"], S["rest"]["pairs"]) == (2, 1, 2), S
    assert rj["lost_pairs"] == 1 and rj["new_pairs"] == 5 and rj["old_pairs"] == 4
    assert S["rescued"]["shape"]["dovetailed"] == 2
    assert S["rescued"]["overhang"]["max"] == 6
    assert S["rescued"]["calls"]["Z"] == 2 and S["rescued"]["calls"]["X"] == 0
    # dove R1 is forward: its 3' cycle 0 is XM[-1] ('h'), cycle 9 is XM[0] ('Z').
    assert S["rescued"]["mbias_r1_3p"][9] == [1, 1], S["rescued"]["mbias_r1_3p"]
    o = rj["r4"]["overhang"]
    # dove R1 idx 9 ('h') is cycle 0; R1 idx 7..9 overhang; partner R1 (OB) idx 0..2.
    assert o["chh_t"] == {"0": 1} and o["chh_m"] == {}, o
    r5 = rj["r5"]
    assert r5["rescued"]["n"] == 2 and abs(r5["rescued"]["mean_score"] - 0.85) < 1e-9, r5
    # The sample is both rest pairs: keep R1 Z at 1000 scores 1.0; keep R2 is cov 6.
    assert r5["rest_sample"] == {"calls": 2, "n": 1, "mean_score": 1.0}, r5
    c = rj["r4"]["control"]
    # keep2 R1 'X' at idx 0 (cycle 9); its MD mismatch is at that called base, so no mm.
    assert c["chg_t"] == {"9": 1} and c["chg_m"] == {"9": 1} and c["mm"] == {}, c
    fams = rj["families"]
    # dove (OT) and partner (OB, swapped UMI, same span) form one duplex family.
    assert fams["families"] == {"total": 4, "duplex": 1, "simplex": 3}, fams
    assert fams["partnered"]["rescued"] == {"partnered": 2, "pairs": 2, "fraction": 1.0}

    def fails(fn, needle):
        try:
            fn()
        except SystemExit as e:
            assert needle in str(e), e
            return
        raise AssertionError(f"expected failure containing {needle!r}")
    # Cycle matching re-weights the control to the overhang's cycle mix; cycle 5 is unmatched.
    ov = {"n": {0: 2}, "d": {0: 10, 1: 10, 5: 4}}
    ct = {"n": {0: 10, 1: 30}, "d": {0: 100, 1: 100}}
    assert matched(ov, ct, "n", "d") == (10.0, 20.0, 20, 4), matched(ov, ct, "n", "d")
    swapped_order = bam("swap", [new_rows[2], new_rows[0]])
    fails(lambda: cmd_rescued(argparse.Namespace(fastq=fq, old=bam("old", old_rows),
                                                 new=swapped_order, dragen=drg, out=out,
                                                 seed=1146)), "not in FASTQ order")
    print("selftest OK")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd")
    r = sub.add_parser("rescued")
    r.add_argument("--fastq", required=True)
    r.add_argument("--old", required=True)
    r.add_argument("--new", required=True)
    r.add_argument("--dragen", required=True)
    r.add_argument("--out", required=True)
    r.add_argument("--seed", type=int, default=1146)
    c = sub.add_parser("consensus")
    c.add_argument("--bam", required=True)
    c.add_argument("--log", required=True)
    c.add_argument("--dragen", required=True)
    c.add_argument("--out", required=True)
    v = sub.add_parser("verdict")
    v.add_argument("--sample", required=True)
    v.add_argument("--rescued", required=True)
    v.add_argument("--old-dir", required=True)
    v.add_argument("--new-dir", required=True)
    v.add_argument("--score", required=True)
    v.add_argument("--old-consensus")
    v.add_argument("--new-consensus")
    v.add_argument("--out", required=True)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        _selftest()
    elif args.cmd == "rescued":
        cmd_rescued(args)
    elif args.cmd == "consensus":
        cmd_consensus(args)
    elif args.cmd == "verdict":
        return cmd_verdict(args)
    else:
        ap.print_help()
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
