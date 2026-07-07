# bismark-io test fixtures

## `tiny_pe_bismark.bam`

A 22 KB Bismark-Perl-generated BAM, used by the integration test
`tests/integration_fixture_bam.rs`.

### How it was generated

Pinned tool versions used at fixture-generation time (2026-05-23):

- **Bismark Perl v0.25.1**
- **bowtie2 2.4.5** (the aligner Bismark uses)
- **samtools 1.21** (used by Bismark for BAM I/O + the `head|samtools view` subsetting step)

Regenerating with substantially different versions of any of these may
produce a slightly different BAM (different `@PG` lines, possibly
different alignment ties), in which case the integration-test
assertions on exact record counts will need updating to match.

Against the existing repo-level test fixtures:

```bash
# 1. Decompress the genome and run Bismark genome preparation.
gunzip -c <repo_root>/test_files/NC_010473.fa.gz > /tmp/genome/NC_010473.fa
<repo_root>/bismark_genome_preparation --bowtie2 /tmp/genome/

# 2. Run Bismark alignment on the PE FASTQ test fixtures.
<repo_root>/bismark \
    --genome /tmp/genome/ \
    -1 <repo_root>/test_files/test_R1.fastq.gz \
    -2 <repo_root>/test_files/test_R2.fastq.gz

# 3. Subset the resulting BAM to the first 100 PE pairs (~200 alignment
#    records) to keep the committed fixture small.
samtools view -h /tmp/work/test_R1_bismark_bt2_pe.bam \
    | head -208 \
    | samtools view -bS - \
    > rust/bismark-io/test_files/tiny_pe_bismark.bam
```

(`head -208` = 8 header lines + 200 alignment lines.)

### What the fixture contains

- **Header**: `@HD VN:1.0 SO:unsorted`, single `@SQ` for `Ecoli_K12`, Bismark `@PG`.
- **203 alignment records** total: 102 R1 + 101 R2 (boundary effects from the head-208 cutoff: +2 extra R1, +1 extra R2 over the 100 complete pairs). All mapped.
- **Strand variety** (per-record `XR/XG` distribution):
  - 55 records `XR:Z:CT XG:Z:CT` → `BismarkStrand::OT` (R1 of OT-pairs)
  - 55 records `XR:Z:GA XG:Z:CT` → `BismarkStrand::CTOT` (R2 of OT-pairs)
  - 47 records `XR:Z:CT XG:Z:GA` → `BismarkStrand::OB` (R1 of OB-pairs)
  - 46 records `XR:Z:GA XG:Z:GA` → `BismarkStrand::CTOB` (R2 of OB-pairs)
- This is a **directional** library, so the pair-strands present are only `OT` and `OB`. The CTOT/CTOB per-record strands appear because R2 of a directional pair is sequenced from the complementary direction (see `DESIGN.md` §Q1).

### Regenerating after a Bismark Perl behaviour change

The fixture is pinned to Bismark Perl v0.25.1's output. If a future Bismark Perl release changes its BAM output (XM-tag conventions, header fields, etc.) the fixture must be regenerated using the same command sequence above and the integration test's expected values updated.
