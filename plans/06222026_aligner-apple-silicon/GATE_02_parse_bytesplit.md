# GATE 02 — byte-level SAM field split in `SamRecord::parse`

**Change:** `SamRecord::parse` split the bowtie2 output line with
`str::split('\t')` + `trim_end_matches([..])`, whose `CharSearcher` (per-char
UTF-8 decode) was the hottest single parse frame in the baseline profile (~330
self-samples, GATE_00). Replaced with a byte scan over the already-validated
`&str`, slicing at the ASCII tab/newline boundaries. Signature unchanged
(`&str` in, `String` fields out), so **zero ripple** into merge/mapq/output.

**Byte-identity:** identical field bytes (the line is valid UTF-8, slices land on
ASCII boundaries, no re-validation). Verified by:
- 419 `bismark-aligner` unit tests (incl. the `SamRecord::parse` cases),
- a 246,856-record subset oracle (mimalloc+parse vs pre-change baseline),
- the **full 12,558,088-record** run below vs the golden.

## Wall-time (RRBS-10M, GRCm39, M4 Max, single run)

| Regime | mimalloc only | mimalloc + parse | delta |
|---|---|---|---|
| `--multicore 4` | 1603 s | **1579.7 s** | **−1.4 %** |
| `-p 6` | — | (neutral) | bowtie2-bound, no Rust headroom |

Full-run byte-identity at `--multicore 4`: **12,558,088 records == golden** ✓.

## Function-level benchmark (criterion, noise-free)

`cargo bench -p bismark-aligner --bench parse_bench` on a representative PE line:

| field split | time | speedup |
|---|---|---|
| `char_searcher` (pre-epic `str::split('\t')`) | 277.6 ns | — |
| `byte_scan` (current) | 136.2 ns | **2.04×** |

Full `SamRecord::parse` (current): 315.7 ns (the split was ~half of the old
parse, so the full parse is ~1.45× faster).

## Note

The function-level win is clean (**2.0× on the split**), but **end-to-end it is
within run-to-run noise** (GATE_03): the aligner is ~90 % bowtie2-bound under
`-p` (GATE_00), so a 140 ns/record parse saving is a fraction of a fraction of
wall-time. It is byte-identical and strictly faster, so it stays; no end-to-end
`-p` speedup is claimed.
