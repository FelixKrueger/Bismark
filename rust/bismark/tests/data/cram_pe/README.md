# `cram_pe` fixtures: paired-end CRAM written by samtools

Used by `io::read::tests::samtools_pe_cram_reads_back_as_bam_except_tlen`.

| File | What |
|---|---|
| `tlen_pe.bam` | 14 Bismark PE pairs (`bismark` v3.2.0, Bowtie 2) from E. coli K-12, all inside a 6 kb window |
| `tlen_pe.cram` | The same BAM as CRAM, written by samtools 1.25 |
| `ecoli_window.fa` / `.fai` | The 6 kb window, E. coli K-12 positions 50501-56500, as contig `ecoli_window` |

The pairs were picked so that the BAM TLENs include all three cases a CRAM reader meets:

- 8 pairs whose TLEN follows htslib's rule (leftmost record positive) with the first record leftmost;
- 4 pairs that follow the rule but whose first record is the rightmost one. samtools stores these
  as attached mates, and noodles-cram re-signs their TLEN with the first record positive, so they
  read back with the sign flipped;
- 2 pairs that do not follow the rule, which samtools stores detached with the TLEN as is.

## Provenance

Records come from a 5,000-pair test run. Pairs with both mates inside the window were kept,
`POS`/`PNEXT` were shifted by -50500 and `RNAME` set to `ecoli_window`. The header was cut
down to `@HD`, `@SQ` and a short `@PG ID:Bismark`. Then:

```bash
samtools faidx ecoli_window.fa
samtools view --no-PG -b -o tlen_pe.bam win.sam
samtools view --no-PG -C -T ecoli_window.fa -o tmp.cram tlen_pe.bam
samtools view -H --no-PG tmp.cram | sed 's/\tUR:[^\t]*//' > h.sam
samtools reheader --no-PG h.sam tmp.cram > tlen_pe.cram
```

`samtools view -T ecoli_window.fa tlen_pe.cram` gives the same columns 1-11 as the BAM.
