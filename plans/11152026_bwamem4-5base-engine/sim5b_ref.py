"""Simulate directional Illumina 5-Base PE reads from a real FASTA: methylated C reads as T on the
original strand. Usage: sim5b_ref.py <in.fa[.gz]> <outdir> <npairs> [nowrite]. Writes <outdir>/genome/<name>.fa
(uppercased) and <outdir>/R1.fq.gz, R2.fq.gz. Fragments containing N are skipped."""
import gzip, random, sys

src, out, npairs = sys.argv[1], sys.argv[2], int(sys.argv[3])
rng = random.Random(42)
comp = str.maketrans("ACGT", "TGCA")
rc = lambda s: s.translate(comp)[::-1]

write_genome = len(sys.argv) < 5 or sys.argv[4] != "nowrite"
contigs, name, parts = {}, None, []
opener = gzip.open if src.endswith(".gz") else open
with opener(src, "rt") as f:
    for line in f:
        if line.startswith(">"):
            if name:
                contigs[name] = "".join(parts).upper()
            name, parts = line[1:].split()[0], []
        else:
            parts.append(line.strip())
contigs[name] = "".join(parts).upper()
if write_genome:
    with open(f"{out}/genome/{name}.fa", "w") as f:
        for nm, seq in contigs.items():
            f.write(f">{nm}\n")
            for i in range(0, len(seq), 60):
                f.write(seq[i:i + 60] + "\n")
names = [n for n in contigs if len(contigs[n]) > 1000]
weights = [len(contigs[n]) for n in names]

L = 150
def mutate(seq):
    return "".join(rng.choice("ACGT".replace(b, "")) if rng.random() < 0.005 else b for b in seq)

n = 0
with gzip.open(f"{out}/R1.fq.gz", "wt") as r1, gzip.open(f"{out}/R2.fq.gz", "wt") as r2:
    while n < npairs:
        c = contigs[rng.choices(names, weights)[0]]
        ins = max(L, int(rng.gauss(330, 90)))
        if len(c) < ins + 2:
            continue
        st = rng.randrange(0, len(c) - ins - 1)
        frag = c[st:st + ins + 1]
        if "N" in frag:
            continue
        ot = rng.random() < 0.5
        meth = rng.random() < 0.7  # most human CpGs are methylated
        top = list(frag[:ins])
        for i in range(ins):
            if meth and frag[i:i + 2] == "CG" and ot and rng.random() < 0.9:
                top[i] = "T"
            if meth and i > 0 and frag[i - 1:i + 1] == "CG" and not ot and rng.random() < 0.9:
                top[i] = "A"
        top = "".join(top)
        fwd, rev = top[:L], rc(top)[:L]
        a, b = (fwd, rev) if ot else (rev, fwd)
        q = "I" * L
        r1.write(f"@c{n}/1\n{mutate(a)}\n+\n{q}\n")
        r2.write(f"@c{n}/2\n{mutate(b)}\n+\n{q}\n")
        n += 1
