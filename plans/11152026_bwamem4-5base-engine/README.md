# bwa-mem4 as the 5-Base engine: evidence

Raw data and scripts behind the PR that replaces the `--bwamem3` engine with bwa-mem4 4.5.0
(`--bwamem4`). Measured on an Apple M4 Max (16 cores, 128 GiB), 15 Nov 2026.

## Tools

- bwa-mem3 0.16.0: bioconda, `micromamba create -p $S/mm3env -c conda-forge -c bioconda bwa-mem3=0.16.0`.
- bwa-mem4 4.5.0: the `v4.5.0` tag (b4197fc), built at the baseline target, like its release
  binaries: `RUSTFLAGS= cargo build --release --target-dir $S/bwa4-tag`.
- `$S/bismark-dev`: Bismark at dev ab7d686 (`--bwamem3`). `$S/bismark-new`: this branch (`--bwamem4`).
  `$S/devbin/bismark_methylation_extractor`: the dev extractor (the extractor is unchanged).

## Data

- `sim5b.py <outdir> <npairs>`: a random 4.5 Mb genome with CpG islands, plus directional 5-Base pairs.
- `sim5b_ref.py <fasta[.gz]> <outdir> <npairs> [nowrite]`: 5-Base pairs drawn from a real FASTA.
  Methylated C reads as T on the original strand, with 0.5% substitutions; fragments with N are skipped.
- chr21: UCSC hg38 `chr21.fa.gz`, 300,000 pairs. GRCh38: Ensembl primary assembly (194 contigs),
  1,000,000 pairs. Both indexes are built with `bwa-mem3 index --meth`, outside the `--genome` folder.

## Scripts

- `bench_engine.sh <index> <R1> <R2> <reps> <threads...>`: both engines alone, with Bismark's exact
  `mem --meth=5base …` argv, interleaved. One TSV row per run, with the SAM-body md5 and the load average.
- `bench_bismark.sh <run dir> <index> <reps> <threads>`: whole Bismark runs, interleaved, with the
  BAM-records md5.
- `summarise.py <tsv>`: medians and ratios.
- `rerun_all.sh`: everything in the PR, end to end (identity checks, chr21, GRCh38).

## Files

| File | What |
|---|---|
| `bench_grch38.tsv`, `bench_bismark_grch38.tsv` | GRCh38, engine alone (8 and 16 threads) and whole Bismark run (`-p 8`) |
| `bench_chr21.tsv`, `bench_bismark_chr21.tsv` | chr21, the same |
| `index_chr21.txt` | `index --meth` on chr21, 3 runs of each |
| `grch38_bwamem3_index.log` | `bwa-mem3 index --meth --max-memory 139G` on GRCh38 |
