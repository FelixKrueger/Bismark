#!/usr/bin/env python3
"""Admitted-pair and verdict checks for the 5-Base Bowtie 2 --maxins 800 default.

Pure stdlib, like five_base_compare.py; BAMs are read through `samtools view`.
Pre-registration: plans/11012026_5base-bowtie2-defaults/PREREGISTRATION.md (rev 2).

  rescued   --fastq <R1> --old <old arm _pe.bam> --new <new arm _pe.bam>
            --dragen <CpG-only CX> --out <dir> [--all-bases] [--seed 1146]
            Joins both raw BAMs in FASTQ order into admitted / changed / rest
            pairs and writes <dir>/rescued.json: per-set geometry, |TLEN|,
            methylation, R1 3' M-bias, sampled per-base tallies against a
            cycle-matched control, and matched-site DRAGEN agreement.
  verdict   --sample S --rescued <json> --old-arm A --new-arm B --old-dir <cell>
            --new-dir <cell> --score <score.tsv> --out <tsv>
            Applies the pre-registered A1-A7 and secondary checks; exits 1
            unless every one passes.
  --selftest
"""

import argparse
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
# Seeded draws (one per new-arm pair) select the per-base samples and the agreement pool.
CONTROL_FRAC = 0.01
ADMIT_FRAC = 0.10
POOL_FRAC = 0.05
MBIAS_3P = range(0, 10)
MBIAS_MID = range(60, 70)
REST_LONG = (450, 500)
MIN_N = 10_000
SETS = ("admitted", "changed", "rest", "rest_long")


# ---------------------------------------------------------------- SAM parsing

class Mate:
    __slots__ = ("name", "flag", "rname", "pos", "cigar", "tlen", "seqlen", "xm", "md", "_span")

    def __init__(self, line):
        f = line.rstrip("\n").split("\t")
        self.name = f[0]
        self.flag = int(f[1])
        self.rname = f[2]
        self.pos = int(f[3])
        self.cigar = f[5]
        self.tlen = int(f[8])
        self.seqlen = len(f[9])
        self.xm = self.md = None
        for t in f[11:]:
            tag = t[:2]
            if tag == "XM":
                self.xm = t[5:]
            elif tag == "MD":
                self.md = t[5:]
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


def quantile(hist, f):
    """Quantile f of a {value: count} histogram."""
    total = sum(hist.values())
    if not total:
        return None
    target, seen = int(total * f), 0
    for v in sorted(hist):
        seen += hist[v]
        if seen > target:
            return v
    return max(hist)


class SetStats:
    def __init__(self):
        self.pairs = 0
        self.shape = {"dovetailed": 0, "contained": 0, "normal": 0}
        self.overhang = {}
        self.tlen = {}
        self.calls = dict.fromkeys("ZzXxHh", 0)
        # [methylated, total] R1 CpG calls over the 3' and mid-read cycle windows
        self.mbias = {"3p": [0, 0], "mid": [0, 0]}

    def add(self, r1, r2):
        self.pairs += 1
        shape, over = geometry(r1, r2)
        self.shape[shape] += 1
        if shape == "dovetailed":
            self.overhang[over] = self.overhang.get(over, 0) + 1
        t = abs(r1.tlen)
        self.tlen[t] = self.tlen.get(t, 0) + 1
        for m in (r1, r2):
            for ch in "ZzXxHh":
                self.calls[ch] += m.xm.count(ch)
        xm = r1.xm if r1.reverse else r1.xm[::-1]  # 3'-first
        for name, window in (("3p", MBIAS_3P), ("mid", MBIAS_MID)):
            if len(xm) <= window[-1]:
                continue
            seg = xm[window[0]:window[-1] + 1]
            m, u = seg.count("Z"), seg.count("z")
            self.mbias[name][0] += m
            self.mbias[name][1] += m + u

    def to_json(self):
        def dist(h):
            return {"n": sum(h.values()), "p10": quantile(h, 0.1), "p50": quantile(h, 0.5),
                    "p90": quantile(h, 0.9), "max": max(h) if h else None}
        in_cap = sum(n for t, n in self.tlen.items() if 500 < t <= 800)
        return {"pairs": self.pairs, "shape": self.shape, "overhang": dist(self.overhang),
                "tlen": dist(self.tlen),
                "tlen_500_800_fraction": in_cap / self.pairs if self.pairs else None,
                "calls": self.calls, "mbias_r1": self.mbias}


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


def same_alignment(a, b):
    return all(x.pos == y.pos and x.cigar == y.cigar for x, y in zip(a, b))


# ---------------------------------------------------------------- rescued

def cmd_rescued(args):
    os.makedirs(args.out, exist_ok=True)
    stats = {s: SetStats() for s in SETS}
    adm_t, ctrl_t = CycleTally(), CycleTally()
    adm_calls, pool_calls = array("q"), array("q")
    rng = random.Random(args.seed)
    lost = new_pairs = old_pairs = fastq_pairs = adm_sampled = ctrl_sampled = 0
    old_it, new_it = sam_pairs(args.old), sam_pairs(args.new)
    old, new = next(old_it, None), next(new_it, None)
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
        draw = rng.random()
        if o is None:
            label = "admitted"
        elif not same_alignment(o, n):
            label = "changed"
        else:
            label = "rest"
        stats[label].add(r1, r2)
        if label == "admitted":
            cpg_calls(r1, r2, adm_calls)
            if args.all_bases:
                if draw < ADMIT_FRAC:
                    adm_sampled += 1
                    for m in n:
                        adm_t.add(m, lambda i: True)
            else:
                for m, ob in zip(n, overhang_bases(r1, r2)):
                    if ob:
                        adm_t.add(m, ob.__contains__)
        elif label == "rest":
            if REST_LONG[0] <= abs(r1.tlen) <= REST_LONG[1]:
                stats["rest_long"].add(r1, r2)
            if draw < CONTROL_FRAC:
                ctrl_sampled += 1
                for m, ob in zip(n, overhang_bases(r1, r2)):
                    ctrl_t.add(m, lambda i, ob=ob: i not in ob)
            if draw < POOL_FRAC:
                cpg_calls(r1, r2, pool_calls)
    if old is not None or new is not None:
        left = (old or new)[0].name
        raise SystemExit(f"five_base_rescued: BAM record {left} is not in the FASTQ, or the BAMs "
                         "are not in FASTQ order")
    agreement = matched_site_agreement(args.dragen, adm_calls, pool_calls)
    out = {"seed": args.seed, "all_bases": args.all_bases, "control_frac": CONTROL_FRAC,
           "admit_frac": ADMIT_FRAC, "pool_frac": POOL_FRAC,
           "fastq_pairs": fastq_pairs, "old_pairs": old_pairs, "new_pairs": new_pairs,
           "lost_pairs": lost, "admitted_sampled": adm_sampled, "control_sampled": ctrl_sampled,
           "sets": {s: stats[s].to_json() for s in SETS},
           "per_base": {"admitted": adm_t.to_json(), "control": ctrl_t.to_json()},
           "agreement": agreement}
    with open(os.path.join(args.out, "rescued.json"), "w") as fh:
        json.dump(out, fh, indent=1)
    print(f"admitted {stats['admitted'].pairs} / {new_pairs} pairs; "
          f"changed {stats['changed'].pairs}; lost {lost}")


def matched_site_agreement(dragen, adm_calls, pool_calls):
    """Mean over shared DRAGEN cov >= 10 sites of (admitted - pool) per-site mean call score."""
    shared = {c >> 1 for c in adm_calls} & {c >> 1 for c in pool_calls}
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
                if k in shared:
                    p[k] = m / (m + u)

    def per_site(calls):
        acc = {}
        for c in calls:
            v = p.get(c >> 1)
            if v is not None:
                a = acc.setdefault(c >> 1, [0.0, 0])
                a[0] += v if c & 1 else 1.0 - v
                a[1] += 1
        return acc
    a, b = per_site(adm_calls), per_site(pool_calls)
    diffs = [a[k][0] / a[k][1] - b[k][0] / b[k][1] for k in a if k in b]
    return {"admitted_calls": len(adm_calls), "pool_calls": len(pool_calls),
            "sites": len(diffs),
            "mean_diff": sum(diffs) / len(diffs) if diffs else None,
            "admitted_mean": sum(v[0] / v[1] for v in a.values()) / len(a) if a else None,
            "pool_mean": sum(v[0] / v[1] for v in b.values()) / len(b) if b else None}


# ---------------------------------------------------------------- verdict

def report_counts(path):
    """Methylation % per context from a Bismark PE report's methylated / unmethylated counts."""
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
    """Sample rate and the control rate re-weighted to the sample's cycle mix."""
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


def mbias_shift(s):
    """R1 CpG % over 3' cycles 0-9 minus cycles 60-69."""
    (m3, t3), (mm, tm) = s["mbias_r1"]["3p"], s["mbias_r1"]["mid"]
    if not t3 or not tm:
        return float("nan")
    return 100.0 * m3 / t3 - 100.0 * mm / tm


def status(ok, n_ok=True):
    return "PASS" if ok and n_ok else ("INSUFFICIENT" if not n_ok else "FAIL")


def admitted_criteria(rj):
    """[(criterion, value, threshold, status)] for A1-A7, plus report-only rows."""
    rows = []
    S = rj["sets"]
    adm, rest, long_ = S["admitted"], S["rest"], S["rest_long"]
    frac = 100.0 * adm["pairs"] / rj["new_pairs"] if rj["new_pairs"] else float("nan")
    rows.append(("A1 admitted % of new pairs", f"{frac:.4f}", "1-15", status(1.0 <= frac <= 15.0)))
    f = adm["tlen_500_800_fraction"]
    rows.append(("A2 admitted with 500 < |TLEN| <= 800 %",
                 "nan" if f is None else f"{100 * f:.4f}", ">= 95",
                 status(f is not None and f >= 0.95)))
    for ctx, (m, u) in (("CHG", "Xx"), ("CHH", "Hh")):
        a, b = pct_of(adm["calls"], m, u), pct_of(rest["calls"], m, u)
        rows.append((f"A3 admitted {ctx} %", f"{a:.4f} vs rest {b:.4f}", "<= rest + 0.5",
                     status(a <= b + 0.5)))
    a, b = pct_of(adm["calls"], "Z", "z"), pct_of(rest["calls"], "Z", "z")
    rows.append(("A4 admitted CpG %", f"{a:.4f} vs rest {b:.4f}", "within 5",
                 status(abs(a - b) <= 5.0)))
    rows.append(("A4 report: rest 450-500 bp CpG %", f"{pct_of(long_['calls'], 'Z', 'z'):.4f}",
                 "report only", "INFO"))
    pb = {k: {kk: {int(x): n for x, n in vv.items()} for kk, vv in v.items()}
          for k, v in rj["per_base"].items()}
    o, c = pb["admitted"], pb["control"]
    ov, cv, n, drop = matched(o, c, "mm", "unc")
    rows.append(("A5 admitted mismatch %", f"{ov:.4f} vs control {cv:.4f} (n {n}, unmatched {drop})",
                 "<= 2x control", status(ov <= 2 * cv, n >= MIN_N)))
    for ctx, num, den in (("CHG", "chg_m", "chg_t"), ("CHH", "chh_m", "chh_t")):
        ov, cv, n, drop = matched(o, c, num, den)
        rows.append((f"A5 admitted {ctx} %",
                     f"{ov:.4f} vs control {cv:.4f} (n {n}, unmatched {drop})",
                     "<= control + 1.0", status(ov <= cv + 1.0, n >= MIN_N)))
    ag = rj["agreement"]
    d = ag["mean_diff"]
    rows.append(("A6 matched-site DRAGEN agreement, admitted - pool",
                 f"{d} (sites {ag['sites']})", ">= -0.02",
                 status(d is not None and d >= -0.02, ag["sites"] >= MIN_N)))
    a, b = mbias_shift(adm), mbias_shift(rest)
    rows.append(("A7 R1 3' M-bias shift (cycles 0-9 minus 60-69)", f"{a:.4f} vs rest {b:.4f}",
                 "within 3", status(abs(a - b) <= 3.0)))
    rows.append(("report: admitted |TLEN| p10/p50/p90",
                 f"{adm['tlen']['p10']}/{adm['tlen']['p50']}/{adm['tlen']['p90']}", "report only",
                 "INFO"))
    rows.append(("report: admitted dovetailed/contained",
                 f"{adm['shape']['dovetailed']}/{adm['shape']['contained']}", "report only", "INFO"))
    rows.append(("report: changed / lost pairs", f"{S['changed']['pairs']} / {rj['lost_pairs']}",
                 "report only", "INFO"))
    return rows


def secondary_criteria(old_arm, new_arm, old_dir, new_dir, score):
    """Genome-wide checks of the new arm against the old arm."""
    rows = []
    sc = {}
    for line in open(score):
        f = line.rstrip("\n").split("\t")
        sc[(f[0], f[1], f[2])] = f
    for arm in (old_arm, new_arm):
        if (arm, "common", "10") not in sc:
            raise SystemExit(f"five_base_rescued: no common-set row for {arm} in {score}")
    # The common set depends on every arm scored, so the file must hold exactly these two.
    arms = {k[0] for k in sc if k[1] == "common"}
    if arms != {old_arm, new_arm}:
        raise SystemExit(f"five_base_rescued: {score} scores {sorted(arms)}, expected "
                         f"{old_arm} and {new_arm} only")
    r_old, r_new = float(sc[(old_arm, "common", "10")][4]), float(sc[(new_arm, "common", "10")][4])
    dr = round(r_new - r_old, 6)  # score.tsv carries r to 6 decimals
    rows.append(("S common-set delta r", f"{dr:.6f}", ">= -0.001", status(dr >= -0.001)))
    po = report_counts(one(r"_PE_report\.txt$", old_dir))
    pn = report_counts(one(r"_PE_report\.txt$", new_dir))
    for ctx in ("CHG", "CHH"):
        rows.append((f"S {ctx} %", f"{pn[ctx]:.4f} vs {old_arm} {po[ctx]:.4f}",
                     f"<= {old_arm} + 0.2", status(pn[ctx] <= po[ctx] + 0.2)))
    mo = metrics(os.path.join(old_dir, "metrics.tsv"))
    mn = metrics(os.path.join(new_dir, "metrics.tsv"))
    qo = 100.0 * int(mo["mapq0"]) / int(mo["records"])
    qn = 100.0 * int(mn["mapq0"]) / int(mn["records"])
    rows.append(("S MAPQ-0 %", f"{qn:.4f} vs {old_arm} {qo:.4f}", f"<= {old_arm} + 2",
                 status(qn <= qo + 2.0)))
    do = deconv_counts(os.path.join(old_dir, "deconv.txt"))
    dn = deconv_counts(os.path.join(new_dir, "deconv.txt"))
    for i, name in enumerate(("precision", "recall")):
        rows.append((f"S deconvolution {name} delta", f"{dn[i] - do[i]:.4f}", ">= -1",
                     status(dn[i] - do[i] >= -1)))
    for key in ("own", "gain"):
        row = sc.get((new_arm, key, "10"))
        if row:
            rows.append((f"report: {new_arm} {key}-set cov>=10 n / r", f"{row[3]} / {row[4]}",
                         "report only", "INFO"))
    rows.append(("report: map_eff", f"{mn['map_eff']} vs {old_arm} {mo['map_eff']}",
                 "report only", "INFO"))
    return rows


def cmd_verdict(args):
    rj = json.load(open(args.rescued))
    rows = admitted_criteria(rj) + secondary_criteria(args.old_arm, args.new_arm, args.old_dir,
                                                      args.new_dir, args.score)
    rows = [(args.sample,) + r for r in rows]
    with open(args.out, "w") as fh:
        fh.write("sample\tcriterion\tvalue\tthreshold\tstatus\n")
        for r in rows:
            fh.write("\t".join(map(str, r)) + "\n")
    for r in rows:
        print("\t".join(map(str, r)))
    return 0 if all(r[-1] in ("PASS", "INFO") for r in rows) else 1


# ---------------------------------------------------------------- selftest

def _sam(name, flag, pos, cigar, pnext, tlen, seq, xm, md):
    return "\t".join([name, str(flag), "1", str(pos), "40", cigar, "=", str(pnext), str(tlen),
                      seq, "I" * len(seq), f"XM:Z:{xm}", f"MD:Z:{md}"]) + "\n"


def _fails(fn, needle):
    try:
        fn()
    except SystemExit as e:
        assert needle in str(e), e
        return
    raise AssertionError(f"expected failure containing {needle!r}")


def _selftest_units():
    L, s = 10, "A" * 10
    ot1 = Mate(_sam("p", 99, 104, "10M", 101, 13, s, "Z.x......h", "0A9"))
    ot2 = Mate(_sam("p", 147, 101, "10M", 104, -13, s, "z.........", "10"))
    assert geometry(ot1, ot2) == ("dovetailed", 6), geometry(ot1, ot2)
    assert overhang_bases(ot1, ot2) == ({7, 8, 9}, {0, 1, 2})
    ob1 = Mate(_sam("q", 83, 201, "10M", 205, -14, s, "..........", "10"))
    ob2 = Mate(_sam("q", 163, 205, "10M", 201, 14, s, "..........", "10"))
    assert geometry(ob1, ob2) == ("dovetailed", 8), geometry(ob1, ob2)
    # CpG calls of an OB pair (R1 reverse) key on the minus strand at their reference position.
    obz = Mate(_sam("q", 83, 201, "10M", 205, -14, s, ".Z........", "10"))
    calls = array("q")
    cpg_calls(obz, ob2, calls)
    assert list(calls) == [site_key("1", 202, "-") << 1 | 1], list(calls)
    calls = array("q")
    cpg_calls(ot1, ot2, calls)  # R1 forward: plus strand; 'Z' at ref 104, 'z' at ref 101
    assert sorted(calls) == sorted([site_key("1", 101, "+") << 1, site_key("1", 104, "+") << 1 | 1])
    co1 = Mate(_sam("c", 99, 300, "10M", 300, 16, s, "..........", "10"))
    co2 = Mate(_sam("c", 147, 300, "16M", 300, -16, "A" * 16, "." * 16, "16"))
    assert geometry(co1, co2) == ("contained", 0)
    # F containing R but ending past it overhangs by the definition: F bases 308-309.
    in2 = Mate(_sam("c", 147, 302, "6M", 300, -10, s[:6], "......", "6"))
    assert geometry(co1, in2) == ("dovetailed", 2), geometry(co1, in2)
    no1 = Mate(_sam("n", 99, 400, "10M", 420, 30, s, "..........", "10"))
    no2 = Mate(_sam("n", 147, 420, "10M", 400, -30, s, "..........", "10"))
    assert geometry(no1, no2) == ("normal", 0)
    assert ot1.cycle(9) == 0 and ot2.cycle(0) == 0
    ind = Mate(_sam("i", 99, 10, "2S3M1I2M1D2M", 30, 20, s, "..........", "4A^G0C1"))
    assert list(ind.aligned()) == [(2, 10), (3, 11), (4, 12), (6, 13), (7, 14), (8, 16), (9, 17)]
    assert ind.mismatches() == {14, 16}, ind.mismatches()
    assert ind.span() == (10, 17)
    t = CycleTally()
    t.add(Mate(_sam("m", 99, 1, "10M", 20, 29, s, "X.........", "0A1C7")), lambda i: True)
    assert t.t["chg_t"] == {9: 1} and t.t["chg_m"] == {9: 1}
    assert t.t["mm"] == {7: 1} and t.t["unc"][7] == 1 and 9 not in t.t["unc"], t.t
    # Cycle matching re-weights the control to the sample's cycle mix; cycle 5 is unmatched.
    ov = {"n": {0: 2}, "d": {0: 10, 1: 10, 5: 4}}
    ct = {"n": {0: 10, 1: 30}, "d": {0: 100, 1: 100}}
    assert matched(ov, ct, "n", "d") == (10.0, 20.0, 20, 4), matched(ov, ct, "n", "d")
    assert quantile({1: 1, 5: 8, 9: 1}, 0.1) == 5 and quantile({1: 1, 5: 8, 9: 1}, 0.0) == 1
    assert quantile({}, 0.5) is None
    # M-bias windows: forward R1 reads its 3' end from the right; a 70 bp read fills both.
    st = SetStats()
    xm = "z" * 10 + "." * 50 + "Z" * 10
    st.add(Mate(_sam("b", 99, 1, "70M", 100, 170, "A" * 70, xm, "70")),
           Mate(_sam("b", 147, 100, "70M", 1, -170, "A" * 70, "." * 70, "70")))
    assert st.mbias == {"3p": [10, 10], "mid": [0, 10]}, st.mbias
    assert abs(mbias_shift(st.to_json()) - 100.0) < 1e-9
    assert st.to_json()["tlen_500_800_fraction"] == 0.0 and st.to_json()["tlen"]["p50"] == 170


def _rj(**over):
    """A synthetic rescued.json that passes every admitted criterion."""
    calls = {"Z": 500, "z": 500, "X": 10, "x": 990, "H": 10, "h": 990}
    mb = {"3p": [50, 100], "mid": [50, 100]}
    base = {"calls": calls, "mbias_r1": mb, "tlen_500_800_fraction": 0.0,
            "tlen": {"p10": 1, "p50": 2, "p90": 3}, "shape": {"dovetailed": 0, "contained": 0},
            "pairs": 100}
    adm = dict(base, tlen_500_800_fraction=0.97, pairs=5)
    tally = {"unc": {"0": 20000}, "mm": {"0": 100}, "chg_t": {"0": 20000}, "chg_m": {"0": 200},
             "chh_t": {"0": 20000}, "chh_m": {"0": 200}}
    rj = {"new_pairs": 100, "lost_pairs": 0,
          "sets": {"admitted": adm, "rest": base, "rest_long": base, "changed": base},
          "per_base": {"admitted": tally, "control": tally},
          "agreement": {"mean_diff": 0.0, "sites": 20000}}
    for path, v in over.items():
        node = rj
        keys = path.split("__")
        for k in keys[:-1]:
            node = node[k]
        node[keys[-1]] = v
    return rj


def _selftest_criteria():
    def st(rj, prefix):
        return [r[3] for r in admitted_criteria(rj) if r[0].startswith(prefix)]
    assert all(r[3] in ("PASS", "INFO") for r in admitted_criteria(_rj())), admitted_criteria(_rj())
    # Boundaries: each criterion passes at its edge and fails just beyond it.
    assert st(_rj(sets__admitted__pairs=15), "A1") == ["PASS"]
    assert st(_rj(sets__admitted__pairs=16), "A1") == ["FAIL"]
    assert st(_rj(sets__admitted__pairs=0), "A1") == ["FAIL"]
    assert st(_rj(sets__admitted__tlen_500_800_fraction=0.95), "A2") == ["PASS"]
    assert st(_rj(sets__admitted__tlen_500_800_fraction=0.949), "A2") == ["FAIL"]
    edge = {"Z": 500, "z": 500, "X": 15, "x": 985, "H": 15, "h": 985}  # 1.5 vs 1.0
    assert st(_rj(sets__admitted__calls=edge), "A3") == ["PASS", "PASS"]
    hi = {"Z": 500, "z": 500, "X": 16, "x": 984, "H": 10, "h": 990}  # CHG 1.6 vs 1.0
    assert st(_rj(sets__admitted__calls=hi), "A3") == ["FAIL", "PASS"]
    ok = {"Z": 550, "z": 450, "X": 10, "x": 990, "H": 10, "h": 990}  # CpG 55 vs 50
    assert st(_rj(sets__admitted__calls=ok), "A4 admitted") == ["PASS"]
    bad = {"Z": 551, "z": 449, "X": 10, "x": 990, "H": 10, "h": 990}
    assert st(_rj(sets__admitted__calls=bad), "A4 admitted") == ["FAIL"]
    mm = dict(_rj()["per_base"]["admitted"], mm={"0": 201})
    assert st(_rj(per_base__admitted=mm), "A5 admitted mismatch") == ["FAIL"]
    mm = dict(_rj()["per_base"]["admitted"], mm={"0": 200})
    assert st(_rj(per_base__admitted=mm), "A5 admitted mismatch") == ["PASS"]
    for ctx in ("chg", "chh"):  # control 1.0%: 2.0% passes, 2.005% fails
        ok_t = dict(_rj()["per_base"]["admitted"], **{f"{ctx}_m": {"0": 400}})
        bad_t = dict(_rj()["per_base"]["admitted"], **{f"{ctx}_m": {"0": 401}})
        name = f"A5 admitted {ctx.upper()}"
        assert st(_rj(per_base__admitted=ok_t), name) == ["PASS"], ctx
        assert st(_rj(per_base__admitted=bad_t), name) == ["FAIL"], ctx
    small = {k: {"0": v["0"] // 4} for k, v in _rj()["per_base"]["admitted"].items()}
    assert st(_rj(per_base__admitted=small), "A5") == ["INSUFFICIENT"] * 3
    assert st(_rj(agreement={"mean_diff": -0.02, "sites": 20000}), "A6") == ["PASS"]
    assert st(_rj(agreement={"mean_diff": -0.021, "sites": 20000}), "A6") == ["FAIL"]
    assert st(_rj(agreement={"mean_diff": 0.0, "sites": 9999}), "A6") == ["INSUFFICIENT"]
    assert st(_rj(agreement={"mean_diff": None, "sites": 0}), "A6") == ["INSUFFICIENT"]
    assert st(_rj(sets__admitted__mbias_r1={"3p": [53, 100], "mid": [50, 100]}), "A7") == ["PASS"]
    assert st(_rj(sets__admitted__mbias_r1={"3p": [54, 100], "mid": [50, 100]}), "A7") == ["FAIL"]

    # Secondary checks read the cells by arm name.
    d = tempfile.mkdtemp(prefix="fbr_sec_")

    def cell(name, chg_m, mapq0, prec):
        p = os.path.join(d, name)
        os.makedirs(p)
        with open(os.path.join(p, "x_PE_report.txt"), "w") as fh:
            for ctx, m in (("CpG", 500), ("CHG", chg_m), ("CHH", 10)):
                fh.write(f"Total methylated C's in {ctx} context:\t{m}\n")
                fh.write(f"Total unmethylated C's in {ctx} context:\t{1000 - m}\n")
        with open(os.path.join(p, "metrics.tsv"), "w") as fh:
            fh.write("mapq0\trecords\tmap_eff\n" f"{mapq0}\t1000\t90.0\n")
        with open(os.path.join(p, "deconv.txt"), "w") as fh:
            fh.write(f"  precision (any DRAGEN C>T/G>A):  {prec}/100 = x%\n")
            fh.write("  recall:                          50/100 = 50.0%\n")
        return p
    old, new = cell("B", 10, 10, 90), cell("B-new", 12, 30, 89)
    score = os.path.join(d, "score.tsv")
    with open(score, "w") as fh:
        fh.write("arm\tset\tcov\tn\tr\n")
        fh.write("B\tcommon\t10\t5\t0.960\nB-new\tcommon\t10\t5\t0.959\n")
        fh.write("B-new\town\t10\t9\t0.97\n")
    rows = secondary_criteria("B", "B-new", old, new, score)
    got = {r[0]: r[3] for r in rows}
    assert got["S common-set delta r"] == "PASS"
    assert got["S CHG %"] == "PASS" and got["S MAPQ-0 %"] == "PASS"
    assert got["S deconvolution precision delta"] == "PASS"
    assert got["report: B-new own-set cov>=10 n / r"] == "INFO"
    new2 = cell("B-new2", 13, 31, 88)
    score2 = os.path.join(d, "score2.tsv")
    with open(score2, "w") as fh:
        fh.write("arm\tset\tcov\tn\tr\n")
        fh.write("B\tcommon\t10\t5\t0.960\nB-new2\tcommon\t10\t5\t0.9579\n")
    got = {r[0]: r[3] for r in secondary_criteria("B", "B-new2", old, new2, score2)}
    assert got["S common-set delta r"] == "FAIL" and got["S CHG %"] == "FAIL"
    assert got["S MAPQ-0 %"] == "FAIL" and got["S deconvolution precision delta"] == "FAIL"
    _fails(lambda: secondary_criteria("B-old", "B-new", old, new, score), "no common-set row")
    # A third scored arm changes the common set, so the check refuses the file.
    with open(score2, "a") as fh:
        fh.write("B-new\tcommon\t10\t5\t0.959\n")
    _fails(lambda: secondary_criteria("B", "B-new2", old, new2, score2), "only")


def _selftest_end_to_end():
    global CONTROL_FRAC, ADMIT_FRAC, POOL_FRAC
    CONTROL_FRAC = ADMIT_FRAC = POOL_FRAC = 1.0  # every pair is sampled at this size
    d = tempfile.mkdtemp(prefix="fbr_selftest_")
    L, s = 10, "A" * 10
    fq = os.path.join(d, "r1.fq")
    names = ["long", "chg", "keep", "lost", "long2", "keep2"]
    with open(fq, "w") as fh:
        for n in names:
            fh.write(f"@{n} 1:N:0:ACGT\n{s}\n+\n{'I' * L}\n")

    def pair(n, a, b, flags=(99, 147)):
        (p1, x1, m1), (p2, x2, m2) = a, b
        span = max(p1, p2) + L - min(p1, p2)
        t1 = span if flags[0] == 99 else -span
        return (_sam(n, flags[0], p1, "10M", p2, t1, s, x1, m1) +
                _sam(n, flags[1], p2, "10M", p1, -t1, s, x2, m2))
    keep = pair("keep", (1000, "Z.........", "10"), (1020, "z.........", "10"))
    keep2 = pair("keep2", (2000, "X.........", "0A9"), (2020, "..........", "10"))
    old_rows = [pair("chg", (500, "..........", "10"), (520, "..........", "10")), keep,
                pair("lost", (700, "..........", "10"), (720, "..........", "10")), keep2]
    # Two admitted pairs with 600 bp fragments; one carries a mismatch at an uncalled base.
    new_rows = [pair("long", (101, "Z.........", "3A6"), (691, "z.........", "10")),
                pair("chg", (501, "..........", "10"), (520, "..........", "10")), keep,
                pair("long2", (3591, "..........", "10"), (3001, "..........", "10"),
                     flags=(83, 163)), keep2]

    def bam(name, rows):
        sam = os.path.join(d, name + ".sam")
        with open(sam, "w") as fh:
            fh.write("@HD\tVN:1.6\tSO:unsorted\n@SQ\tSN:1\tLN:100000\n" + "".join(rows))
        out = os.path.join(d, name + ".bam")
        subprocess.run(["samtools", "view", "-b", "-o", out, sam], check=True)
        return out
    drg = os.path.join(d, "drg.txt")
    with open(drg, "w") as fh:
        fh.write("chr1\t101\t+\t9\t1\tCG\tCGA\n")    # long R1 Z: admitted score 0.9
        fh.write("chr1\t1000\t+\t10\t0\tCG\tCGA\n")  # keep R1 Z: rest only, not shared
        fh.write("chr1\t691\t+\t2\t8\tCG\tCGA\n")    # long R2 z: admitted only, not shared
    out = os.path.join(d, "out")
    old_bam, new_bam = bam("old", old_rows), bam("new", new_rows)
    cmd_rescued(argparse.Namespace(fastq=fq, old=old_bam, new=new_bam, dragen=drg, out=out,
                                   seed=1146, all_bases=True))
    rj = json.load(open(os.path.join(out, "rescued.json")))
    S = rj["sets"]
    assert (S["admitted"]["pairs"], S["changed"]["pairs"], S["rest"]["pairs"]) == (2, 1, 2), S
    assert rj["lost_pairs"] == 1 and rj["new_pairs"] == 5 and rj["old_pairs"] == 4
    assert rj["admitted_sampled"] == 2 and rj["control_sampled"] == 2
    assert S["admitted"]["tlen"]["p50"] == 600 and S["admitted"]["tlen_500_800_fraction"] == 1.0
    assert S["admitted"]["shape"]["normal"] == 2
    # --all-bases tallies every base of the admitted pairs: the mismatch at ref 104 is uncalled.
    pa = rj["per_base"]["admitted"]
    assert sum(pa["unc"].values()) == 4 * L - 2 and pa["mm"] == {"6": 1}, pa
    # No admitted site is shared with the rest pool, so the matched-site check has no sites.
    ag = rj["agreement"]
    assert (ag["sites"], ag["mean_diff"], ag["admitted_calls"], ag["pool_calls"]) == \
        (0, None, 2, 2), ag
    assert S["rest_long"]["pairs"] == 0

    # Matched sites: admitted and pool calls at one shared site.
    with open(drg, "a") as fh:
        fh.write("chr1\t5000\t+\t8\t2\tCG\tCGA\n")
    k = site_key("1", 5000, "+")
    ag = matched_site_agreement(drg, array("q", [k << 1 | 1, k << 1]),
                                array("q", [k << 1 | 1]))
    assert ag["sites"] == 1 and abs(ag["mean_diff"] - (0.5 - 0.8)) < 1e-9, ag

    _fails(lambda: cmd_rescued(argparse.Namespace(fastq=fq, old=old_bam,
                                                  new=bam("swap", [new_rows[2], new_rows[0]]),
                                                  dragen=drg, out=out, seed=1146,
                                                  all_bases=True)), "not in FASTQ order")


def _selftest():
    _selftest_units()
    _selftest_criteria()
    _selftest_end_to_end()
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
    r.add_argument("--all-bases", action="store_true",
                   help="tally every base of a seeded sample of admitted pairs, not overhangs")
    r.add_argument("--seed", type=int, default=1146)
    v = sub.add_parser("verdict")
    v.add_argument("--sample", required=True)
    v.add_argument("--rescued", required=True)
    v.add_argument("--old-arm", required=True)
    v.add_argument("--new-arm", required=True)
    v.add_argument("--old-dir", required=True)
    v.add_argument("--new-dir", required=True)
    v.add_argument("--score", required=True)
    v.add_argument("--out", required=True)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        _selftest()
    elif args.cmd == "rescued":
        cmd_rescued(args)
    elif args.cmd == "verdict":
        return cmd_verdict(args)
    else:
        ap.print_help()
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
