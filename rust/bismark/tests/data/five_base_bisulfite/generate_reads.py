import sys

g = "".join(l.strip() for l in open(sys.argv[1]) if not l.startswith(">")).upper()
COMP = str.maketrans("ACGTN", "TGCAN")


def rc(s):
    return s.translate(COMP)[::-1]


def bisulfite(seq):
    """Convert C->T except at CpG (kept = methylated). CpG is palindromic, so applying
    this to a revcomp'd (bottom-strand) sequence is correct for an OB read."""
    return "".join(
        ("C" if (i + 1 < len(seq) and seq[i + 1] == "G") else "T") if b == "C" else b
        for i, b in enumerate(seq)
    )


FOREIGN = "GTCAGTCAGT"  # 10 bp absent from pUC19 -> soft clip
reads = []

# ---- OT (forward) records: XG:Z:CT, encoding pair (C,T) --------------------
reads.append(("ot_plain", bisulfite(g[100:180])))
reads.append(("ot_softclip", FOREIGN + bisulfite(g[300:380])))
reads.append(("ot_ins", bisulfite(g[600:640]) + "A" + bisulfite(g[640:680])))
reads.append(("ot_del", bisulfite(g[900:940]) + bisulfite(g[942:982])))

# ---- OB (reverse) records: XG:Z:GA, encoding pair (G,A) --------------------
# Built from the bottom strand, so Bismark emits FLAG 16 and revcomps SEQ.
reads.append(("ob_plain", bisulfite(rc(g[1200:1280]))))
# The foreign prefix sits at the READ's 5' end; after the output revcomp it lands at
# the RIGHT end of SEQ -> a TRAILING soft clip. Asymmetric on purpose: this is the
# record that catches a read_pos_5p-indexed write (PLAN G2 / §9.3).
reads.append(("ob_softclip", FOREIGN + bisulfite(rc(g[1400:1480]))))
reads.append(("ob_ins", bisulfite(rc(g[1600:1640])) + "A" + bisulfite(rc(g[1560:1600]))))
reads.append(("ob_del", bisulfite(rc(g[1900:1940]) + rc(g[1856:1896]))))

with open(sys.argv[2], "w") as fh:
    for name, seq in reads:
        fh.write(f"@{name}\n{seq}\n+\n{'I' * len(seq)}\n")

print("\n".join(f"{n:12s} len={len(s)}" for n, s in reads))
