"""Simulate a directional Illumina 5-Base PE library: methylated C reads as T on the original strand."""
import gzip, random, sys

out, npairs = sys.argv[1], int(sys.argv[2])
rng = random.Random(42)
comp = str.maketrans("ACGT", "TGCA")
rc = lambda s: s.translate(comp)[::-1]

contigs = {}
for name, length in [("chrA", 3_000_000), ("chrB", 1_500_000)]:
    g = [rng.choice("ACGT") for _ in range(length)]
    for s in range(20_000, length - 5_000, 30_000):  # 1.5 kb CpG islands, a CG every 6 bp
        for i in range(s, s + 1500, 6):
            g[i], g[i + 1] = "C", "G"
    contigs[name] = "".join(g)
with open(f"{out}/genome/genome.fa", "w") as f:
    for name, seq in contigs.items():
        f.write(f">{name}\n")
        for i in range(0, len(seq), 60):
            f.write(seq[i:i + 60] + "\n")

names = list(contigs)
def mutate(seq):
    return "".join(rng.choice("ACGT".replace(b, "")) if rng.random() < 0.005 else b for b in seq)

L = 150
with gzip.open(f"{out}/R1.fq.gz", "wt") as r1, gzip.open(f"{out}/R2.fq.gz", "wt") as r2:
    for n in range(npairs):
        if rng.random() < 0.02:  # junk pair
            a = "".join(rng.choice("ACGT") for _ in range(L)); b = "".join(rng.choice("ACGT") for _ in range(L))
        else:
            c = contigs[rng.choice(names)]
            ins = max(L, int(rng.gauss(330, 90)))
            st = rng.randrange(0, len(c) - ins)
            top = list(c[st:st + ins])
            ot = rng.random() < 0.5
            meth = rng.random() < 0.6
            for i in range(len(top)):
                p = st + i
                if ot and c[p] == "C" and c[p + 1:p + 2] == "G" and meth and rng.random() < 0.9:
                    top[i] = "T"
                if not ot and c[p] == "G" and p > 0 and c[p - 1] == "C" and meth and rng.random() < 0.9:
                    top[i] = "A"
                if c[p] == "C" and c[p + 1:p + 2] != "G" and ot and rng.random() < 0.01:
                    top[i] = "T"
            top = "".join(top)
            fwd, rev = top[:L], rc(top)[:L]
            a, b = (fwd, rev) if ot else (rev, fwd)
            a, b = mutate(a), mutate(b)
        q = "I" * L
        r1.write(f"@p{n}/1\n{a}\n+\n{q}\n")
        r2.write(f"@p{n}/2\n{b}\n+\n{q}\n")
