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

## Note

Single-run delta (median-of-3 deferred to the PR). Modest, as the profile
predicted: the CharSearcher was ~0.5 % of `-p` wall, ~1.4 % of the `--multicore`
wall (where the Rust side carries 4 concurrent pipelines). It compounds with
mimalloc on the contended path and is byte-identical, so it stays. `-p` is
bowtie2-bound, so no realistic-usage speedup is claimed.
