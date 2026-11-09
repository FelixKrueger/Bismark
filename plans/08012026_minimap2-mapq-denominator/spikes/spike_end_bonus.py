#!/usr/bin/env python3
"""Spike: does minimap2's `-x sr` end_bonus land in the reported AS:i:?

If it does, `perfect = 2 x read_length` is not an upper bound on AS, and the
MAPQ saturation that #1081 exists to remove comes straight back for short reads.

Two independent halves:
  A. SOURCE  - mechanical assertions against upstream minimap2 (and rammap) source.
  B. EMPIRICAL - run the real minimap2 binary with Bismark's exact option string
     and read the AS it emits, including cases constructed so that end_bonus
     decides the alignment.

Usage:  python3 spike_end_bonus.py [--mm2-src DIR]
Requires: minimap2 on PATH (any version; the version used is reported).
"""

from __future__ import annotations

import argparse
import math
import random
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

# Bismark's minimap2 option string, verbatim (options.rs:288), minus `-x <preset>`
# which is varied. `-t 2` is the Perl-faithful default.
BISMARK_OPTS = ["-a", "--MD", "--secondary=no", "-t", "2", "-K", "250K"]

# Presets Bismark can select (options.rs:256-283). Nothing else is reachable:
# `--mm2_*` are selectors only and there is no free-form option passthrough.
REACHABLE_PRESETS = ["map-ont", "map-pb", "sr"]

PASS, FAIL = "PASS", "FAIL"
results: list[tuple[str, str, str]] = []


def check(name: str, ok: bool, detail: str = "") -> bool:
    results.append((PASS if ok else FAIL, name, detail))
    print(f"  [{PASS if ok else FAIL}] {name}" + (f"  -- {detail}" if detail else ""))
    return ok


# ---------------------------------------------------------------------------
# Part A - source facts
# ---------------------------------------------------------------------------


def part_a(src: Path) -> None:
    print("\n=== A. SOURCE FACTS (minimap2) ===")
    opts = (src / "options.c").read_text()
    align = (src / "align.c").read_text()
    fmt = (src / "format.c").read_text()
    ksw = (src / "ksw2_extz2_sse.c").read_text()

    # A1 defaults
    check("A1 default match score a = 2", bool(re.search(r"opt->a = 2\b", opts)))
    check(
        "A1 default end_bonus = -1 (disabled, not 0)",
        bool(re.search(r"opt->end_bonus = -1", opts)),
    )
    check("A1 default sc_ambi = 1 (N scores <= 0)", bool(re.search(r"opt->sc_ambi = 1", opts)))

    # A2 per-preset overrides, for the three reachable presets only
    preset_bodies = split_presets(opts)
    for preset in REACHABLE_PRESETS:
        body = preset_bodies.get(preset, "")
        a_override = re.findall(r"mo->a = (\d+)", body)
        eb_override = re.findall(r"mo->end_bonus = (-?\d+)", body)
        a_val = int(a_override[0]) if a_override else 2  # inherits the default
        eb_val = int(eb_override[0]) if eb_override else -1
        check(
            f"A2 preset {preset}: a = {a_val}",
            a_val == 2,
            f"end_bonus = {eb_val}" + ("  <-- nonzero" if eb_val > 0 else ""),
        )

    # A3 which field AS:i: prints
    as_line = [l for l in fmt.splitlines() if "AS:i:" in l]
    check(
        "A3 AS:i: prints r->p->dp_score (ms:i: prints dp_max0)",
        len(as_line) == 1 and "dp_score" in as_line[0] and "dp_max0" in as_line[0],
        as_line[0].strip() if as_line else "not found",
    )

    # A4 every write to dp_score
    writes = re.findall(r"dp_score\s*(?:\+=|=)\s*([^;]+);", align)
    check(
        "A4 dp_score is only ever fed ez->max / ez->score - never + end_bonus",
        writes and all(w.strip() in {"ez->max", "ez->score"} for w in writes),
        f"{len(writes)} writes: {sorted(set(w.strip() for w in writes))}",
    )

    # A5 end_bonus reaches ksw only as a call argument, from the two extensions
    eb_calls = [
        l.strip()
        for l in align.splitlines()
        if "opt->end_bonus" in l and "mm_align_pair" in l
    ]
    check(
        "A5 opt->end_bonus is passed only by the left/right EXTENSIONS (gap fill passes -1)",
        len(eb_calls) == 2 and all("EXTZ_ONLY" in l for l in eb_calls),
        f"{len(eb_calls)} call sites, both KSW_EZ_EXTZ_ONLY",
    )

    # A6 inside ksw2, end_bonus only picks the traceback start
    eb_uses = [l.strip() for l in ksw.splitlines() if "end_bonus" in l and "(" not in l.split("end_bonus")[0][-1:]]
    decision = [l for l in ksw.splitlines() if re.search(r"mqe \+ end_bonus > \(int\)ez->max", l)]
    body_uses = [
        l.strip()
        for l in ksw.splitlines()
        if "end_bonus" in l and "void ksw_extz2" not in l and "int end_bonus" not in l
    ]
    check(
        "A6 in ksw2, end_bonus appears ONLY in the traceback-start decision",
        len(decision) == 1 and len(body_uses) == 1,
        (decision[0].strip() if decision else "decision line not found"),
    )

    # A7 the one mechanism that could inflate a DP score above a*len
    check(
        "A7 junc_bonus (the only additive bonus in a DP kernel) is splice-only => unreachable",
        all("ksw_exts2_sse" in l for l in align.splitlines() if "opt->junc_bonus" in l),
        "Bismark never selects a splice preset",
    )


def split_presets(opts: str) -> dict[str, str]:
    """Map each preset name to the body of its `else if` branch in mm_set_opt."""
    start = opts.index("int mm_set_opt(")
    body = opts[start:]
    parts = re.split(r"\}\s*else if \(", body)
    out: dict[str, str] = {}
    for part in parts:
        names = re.findall(r'strn?cmp\(preset, "([^"]+)"', part)
        for n in names:
            out[n] = part
    # `map-ont` shares the default branch, which sets nothing ("this is the same
    # as the default"), so an empty body is the correct answer for it.
    return out


def part_a_rammap(rammap_src: Path | None) -> None:
    print("\n=== A'. SOURCE FACTS (rammap - the pure-Rust minimap2 reimplementation) ===")
    if rammap_src is None or not rammap_src.exists():
        check("A'0 rammap-core checkout found", False, "skipped - not vendored locally")
        return
    api = (rammap_src / "api.rs").read_text()
    extend = (rammap_src / "align" / "extend.rs").read_text()
    common = (rammap_src / "align" / "dp" / "common.rs").read_text()

    check(
        "A'1 rammap sr preset: match_score = 2 AND end_bonus = 10 (mirrors minimap2)",
        bool(re.search(r"match_score = 2;", api)) and bool(re.search(r"end_bonus = 10;", api)),
    )
    writes = re.findall(r"dp_score \+= ([a-z_.]+);", extend)
    check(
        "A'2 rammap dp_score is only ever fed ez.max / ez.score",
        writes and all(w in {"ez.max", "ez.score"} for w in writes),
        f"{len(writes)} writes: {sorted(set(writes))}",
    )
    check(
        "A'3 rammap uses end_bonus only in the traceback-start decision",
        bool(re.search(r"max_query_end_score \+ end_bonus > result\.max", common)),
    )


# ---------------------------------------------------------------------------
# Part B - empirical
# ---------------------------------------------------------------------------


def random_ref(n: int, seed: int = 1081) -> str:
    rng = random.Random(seed)
    return "".join(rng.choice("ACGT") for _ in range(n))


def mutate(seq: str, positions: list[int]) -> str:
    s = list(seq)
    for p in positions:
        s[p] = {"A": "C", "C": "A", "G": "T", "T": "G"}[s[p]]
    return "".join(s)


def run_minimap2(ref_fa: Path, reads_fq: Path, preset: str) -> dict[str, tuple[int, str, int]]:
    """Return {qname: (AS, CIGAR, mm2_own_MAPQ)} for primary alignments."""
    cmd = ["minimap2", *BISMARK_OPTS, "-x", preset, str(ref_fa), str(reads_fq)]
    out = subprocess.run(cmd, capture_output=True, text=True, check=True)
    hits: dict[str, tuple[int, str, int]] = {}
    for line in out.stdout.splitlines():
        if line.startswith("@"):
            continue
        f = line.split("\t")
        flag = int(f[1])
        if flag & 0x900 or flag & 0x4:  # secondary/supplementary/unmapped
            continue
        as_i = next((int(t[5:]) for t in f[11:] if t.startswith("AS:i:")), None)
        if as_i is None:
            continue
        hits[f[0]] = (as_i, f[5], int(f[4]))
    return hits


def part_b() -> None:
    print("\n=== B. EMPIRICAL (real minimap2, Bismark's exact option string) ===")
    if shutil.which("minimap2") is None:
        check("B0 minimap2 on PATH", False, "skipped - no binary")
        return
    ver = subprocess.run(["minimap2", "--version"], capture_output=True, text=True).stdout.strip()
    print(f"  minimap2 version: {ver}")

    ref = random_ref(30_000)
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td)
        ref_fa = tmp / "ref.fa"
        ref_fa.write_text(">chrS\n" + "\n".join(ref[i : i + 60] for i in range(0, len(ref), 60)) + "\n")

        # Read set. Offset 5000 keeps every read interior to the reference so
        # both flanks are extendable (the only place end_bonus is passed).
        cases: list[tuple[str, int, str]] = []
        for length in (50, 100, 150, 250, 1000):
            base = ref[5000 : 5000 + length]
            cases.append((f"perfect_{length}", length, base))
            # Terminal mismatch: the local DP max sits BEFORE the last base, so
            # reaching the query end is chosen only because of end_bonus.
            cases.append((f"lastbase_mm_{length}", length, mutate(base, [length - 1])))
            cases.append((f"bothends_mm_{length}", length, mutate(base, [0, length - 1])))
            # Interior mismatches - end_bonus irrelevant, pure a/b arithmetic.
            cases.append((f"interior_1mm_{length}", length, mutate(base, [length // 2])))
            cases.append((f"interior_3mm_{length}", length, mutate(base, [length // 4, length // 2, 3 * length // 4])))

        reads_fq = tmp / "reads.fq"
        reads_fq.write_text(
            "".join(f"@{n}\n{s}\n+\n{'I' * len(s)}\n" for n, _, s in cases)
        )

        print(f"\n  {'preset':<9} {'case':<22} {'len':>5} {'AS':>6} {'2*len':>6} {'AS/2len':>8} {'CIGAR':<18} mm2_MAPQ")
        violations = []
        observed: dict[str, list[tuple[int, int]]] = {p: [] for p in REACHABLE_PRESETS}
        for preset in REACHABLE_PRESETS:
            hits = run_minimap2(ref_fa, reads_fq, preset)
            for name, length, _ in cases:
                if name not in hits:
                    print(f"  {preset:<9} {name:<22} {length:>5} {'-':>6} {'(unmapped)':>15}")
                    continue
                as_i, cigar, mapq = hits[name]
                perfect = 2 * length
                ratio = as_i / perfect
                flag = "  <== AS > 2*len" if as_i > perfect else ""
                if as_i > perfect:
                    violations.append((preset, name, as_i, perfect))
                observed[preset].append((length, as_i))
                print(
                    f"  {preset:<9} {name:<22} {length:>5} {as_i:>6} {perfect:>6} {ratio:>8.3f} {cigar[:18]:<18} {mapq}{flag}"
                )

        print()
        check(
            "B1 AS <= 2*len in EVERY observed cell, for every reachable preset",
            not violations,
            f"{sum(len(v) for v in observed.values())} alignments, 0 violations"
            if not violations
            else f"violations: {violations}",
        )
        sr_perfect = [(l, a) for l, a in observed["sr"] if a == 2 * l]
        check(
            "B2 a perfect read under -x sr scores EXACTLY 2*len (end_bonus does not leak)",
            len(sr_perfect) >= 3,
            f"{len(sr_perfect)} perfect-score cells under sr; +10/+20 would show here",
        )

    # ---- what the fix would produce -------------------------------------
    # Q5 (the tripwire): #1088 derives the ladder from `match_bonus > 0`, so a
    # nonzero bonus picks the LOCAL ladder. Both are priced out here because that
    # choice, not the denominator, is what sets the FLOOR - and the floor is the
    # consequential end (#1080's code review, HIGH-1).
    print("\n=== C. MAPQ under perfect = 2*len - BOTH ladders, no second best ===")
    print(
        f"  {'len':>6} {'AS':>7} {'%perf':>7} {'bestOver':>9} {'diff':>8} {'ratio':>7}"
        f" {'old':>5} {'new/local':>10} {'new/e2e':>8}"
    )
    for length in (50, 100, 150, 250, 1000):
        for frac in (1.0, 0.8, 0.6, 0.4, 0.2):
            as_best = int(round(2 * length * frac))
            print(f"  {length:>6} {as_best:>7} {frac * 100:>6.0f}% {mapq_row(length, as_best)}")

    print("\n  Rung-by-rung, no second best (what a UNIQUE read gets):")
    print(f"    {'ratio':>7} {'local':>7} {'e2e':>7}")
    for ratio in (1.0, 0.85, 0.75, 0.65, 0.55, 0.45, 0.35, 0.25, 0.1):
        print(f"    {ratio:>7.2f} {ladder_local(ratio, 1.0):>7} {ladder_end_to_end(ratio, 1.0):>7}")
    print("    floor:  local = 22, end-to-end = 0  <== 0 is 'discard' to samtools -q 1")

    # A second best IS routine here: Bismark compares across the 2 (directional
    # SE) strand instances and feeds the runner-up as `as_second`, even though
    # minimap2 itself runs `--secondary=no`.
    print("\n=== D. WITH a cross-instance second best (len 100, perfect = 200) ===")
    print(f"  {'AS':>5} {'AS2':>5} {'old':>5} {'new/local':>10} {'new/e2e':>8}")
    for as_best, as_second in ((200, 190), (200, 100), (200, 20), (120, 110), (120, 40)):
        scm = sc_min(100)
        bo = as_best - scm
        print(
            f"  {as_best:>5} {as_second:>5} "
            f"{full_ladder(bo, abs(scm), as_best, as_second, local=False):>5} "
            f"{full_ladder(bo, max(1.0, 200 - scm), as_best, as_second, local=True):>10} "
            f"{full_ladder(bo, max(1.0, 200 - scm), as_best, as_second, local=False):>8}"
        )


def sc_min(length: int, intercept: float = 0.0, slope: float = -0.2) -> float:
    """Bismark's scMin for minimap2: the LINEAR form, from a --score_min that
    minimap2 itself is never given (options.rs:233 throws the base string away)."""
    return intercept + slope * length


def mapq_row(length: int, as_best: int) -> str:
    scm = sc_min(length)
    best_over = as_best - scm
    old_diff = abs(scm)
    new_diff = max(1.0, 2.0 * length - scm)
    old = ladder_end_to_end(best_over, old_diff)
    new_local = ladder_local(best_over, new_diff)
    new_e2e = ladder_end_to_end(best_over, new_diff)
    return (
        f"{best_over:>9.1f} {new_diff:>8.1f} {best_over / new_diff:>7.3f}"
        f" {old:>5} {new_local:>10} {new_e2e:>8}"
    )


def ladder_end_to_end(best_over: float, diff: float) -> int:
    """Perl calc_mapq, no-second-best branch, end-to-end ladder (bismark:3947-54)."""
    for thresh, q in ((0.8, 42), (0.7, 40), (0.6, 24), (0.5, 23), (0.4, 8), (0.3, 3)):
        if best_over >= diff * thresh:
            return q
    return 0


def ladder_local(best_over: float, diff: float) -> int:
    """Perl calc_mapq, no-second-best branch, --local ladder (bismark:4082-89)."""
    for thresh, q in ((0.8, 44), (0.7, 42), (0.6, 41), (0.5, 36), (0.4, 28), (0.3, 24)):
        if best_over >= diff * thresh:
            return q
    return 22


def full_ladder(best_over: float, diff: float, as_best: int, as_second: int, local: bool) -> int:
    """The second-best branch of each ladder (mapq.rs:68-134 / 163-219)."""
    best_diff = abs(abs(as_best) - abs(as_second))
    if local:
        for k, q in ((0.9, 40), (0.8, 39), (0.7, 38), (0.6, 37)):
            if best_diff >= diff * k:
                return q
        for k, eq, hi, lo in ((0.5, 35, 25, 20), (0.4, 34, 21, 19), (0.3, 33, 18, 16), (0.2, 32, 17, 12), (0.1, 31, 14, 9)):
            if best_diff >= diff * k:
                return eq if best_over == diff else (hi if best_over >= diff * 0.5 else lo)
        if best_diff > 0.0:
            return 11 if best_over >= diff * 0.5 else 2
        return 1 if best_over >= diff * 0.5 else 0
    for k, eq, ne in ((0.9, 39, 33), (0.8, 38, 27), (0.7, 37, 26), (0.6, 36, 22)):
        if best_diff >= diff * k:
            return eq if best_over == diff else ne
    for k, eq, t1, v1, t2, v2, lo in (
        (0.5, 35, 0.84, 25, 0.68, 16, 5),
        (0.4, 34, 0.84, 21, 0.68, 14, 4),
        (0.3, 32, 0.88, 18, 0.67, 15, 3),
        (0.2, 31, 0.88, 17, 0.67, 11, 0),
        (0.1, 30, 0.88, 12, 0.67, 7, 0),
    ):
        if best_diff >= diff * k:
            if best_over == diff:
                return eq
            if best_over >= diff * t1:
                return v1
            return v2 if best_over >= diff * t2 else lo
    if best_diff > 0.0:
        return 6 if best_over >= diff * 0.67 else 2
    return 1 if best_over >= diff * 0.67 else 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--mm2-src", type=Path, required=True, help="dir with fetched minimap2 *.c")
    ap.add_argument("--rammap-src", type=Path, default=None, help="rammap-core/src dir")
    args = ap.parse_args()

    part_a(args.mm2_src)
    part_a_rammap(args.rammap_src)
    part_b()

    print("\n=== SUMMARY ===")
    failed = [r for r in results if r[0] == FAIL]
    for status, name, detail in results:
        if status == FAIL:
            print(f"  FAIL {name}  {detail}")
    print(f"  {len(results) - len(failed)}/{len(results)} checks passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
