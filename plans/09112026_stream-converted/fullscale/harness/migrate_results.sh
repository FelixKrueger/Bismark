#!/bin/sh
# The sampler gained an anon_peak_bytes column mid-run (memory.current counts
# page cache, which flattered the file arm by gigabytes). Rows written before
# that have 11 fields; rows after have 12. Pad the old ones so the file has one
# schema. Idempotent.
set -u
f=${1:?}
[ -f "$f" ] || exit 0
n=$(head -1 "$f" | awk -F'\t' '{print NF}')
[ "$n" -eq 12 ] && { echo "migrate: $f already 12 columns"; exit 0; }
awk -F'\t' 'BEGIN{OFS="\t"}
  NR==1 { $0="shape\tarm\trep\twall_s\tpeak_temp_kb\tpeak_conv_kb\tmem_peak_bytes\tanon_peak_bytes\tcpu_usec\tpids_peak\tout_kb\trc"; print; next }
  NF==11 { print $1,$2,$3,$4,$5,$6,$7,"NA",$8,$9,$10,$11; next }
  { print }' "$f" > "$f.mig" && mv "$f.mig" "$f"
echo "migrate: $f padded to 12 columns"
