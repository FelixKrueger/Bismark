#!/bin/sh
# Fetch the DRAGEN reference outputs needed by concordance.py for a 5-Base sample
# from the BaseSpace "Illumina 5-Base DNA" demo project (id 11420409).
#
# Downloads ONLY the small reference files (not the multi-TB BAM/FASTQ):
#   *.CX_report.txt.gz        per-CpG methylation report  (-> concordance.py methyl --dragen)
#   *.methyl_metrics.csv      global methylation metrics  (sanity)
#   *.hard-filtered.vcf.gz    germline SNV VCF            (-> concordance.py deconv --vcf)
#
# Requires an authenticated `bs` CLI (BaseSpaceCLI). Usage:
#   ./fetch_dragen.sh <dragen_dataset_id> <out_dir>
#
# HG001 = NA12878, HG002 = NA24385 (same Genome-in-a-Bottle individuals). The two
# samples to validate (DRAGEN-complete dataset IDs in this project):
#   NA12878 (HG001): Sample8 100ng = ds.a4a909aaec7c41478b90a1ad5c94c452
#                    (the metrics sample used in VALIDATION_REAL_DATA.md; other lanes
#                     Sample1-36 are listed by: bs list dataset --project-id 11420409)
#   HG002 (NA24385): Sample40 50ng = ds.75c97a1bad7345659341a2a1c4beb306
#                    (NA24385 lanes: Sample37-42)
#
# The matching RAW READS are SEPARATE datasets (DataSetType illumina.fastq.v1.8); list
# them with `bs list dataset --project-id 11420409 | grep fastq` (Sample8 FASTQ =
# ds.b6834497fec442568113475fc76fcce1, Sample40 FASTQ = ds.bf318453a0c64f98b61589998b46e879) and download with
# `bs download dataset --id <fastq_ds> -o <reads_dir>` for the alignment input.
set -eu

DS="${1:?usage: fetch_dragen.sh <dragen_dataset_id> <out_dir>}"
OUT="${2:?usage: fetch_dragen.sh <dragen_dataset_id> <out_dir>}"
mkdir -p "$OUT"

for ext in CX_report.txt.gz methyl_metrics.csv hard-filtered.vcf.gz; do
  echo ">> downloading *.$ext from $DS"
  bs download dataset --id "$DS" -o "$OUT" --extension "$ext"
done

echo "Done. Reference files in $OUT:"
ls -1 "$OUT" | grep -E 'CX_report\.txt\.gz$|methyl_metrics\.csv$|hard-filtered\.vcf\.gz$' || true
echo
echo "Next: extract our cytosine report from the 5-Base BAM, then run e.g."
echo "  python3 concordance.py methyl  --ours <ours_pe.CX_report.txt> --dragen $OUT/*.CX_report.txt.gz"
echo "  python3 concordance.py deconv  --variants <ours_pe.5base_deconvolution.txt> --vcf $OUT/*.hard-filtered.vcf.gz"
