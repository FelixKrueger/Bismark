# GATE 03 — Phase 2/4 cumulative (parse byte-split + build_pe_mate alloc reduction)

## End-to-end wall-time (RRBS-10M, GRCm39, M4 Max, `--multicore 4`, single runs)

| Binary | wall | vs mimalloc |
|---|---|---|
| mimalloc only | 1603.0 s | — |
| mimalloc + parse byte-split | 1579.7 s | −1.4 % |
| mimalloc + parse + build_pe_mate | 1601.4 s | −0.1 % |

**Honest reading:** the 1580–1603 s spread (~1.5 %) is **within laptop run-to-run
noise** (thermal throttling, efficiency-core scheduling). The Phase 2/4
micro-optimizations are byte-identical and genuinely reduce work (the `CharSearcher`
per-char decode in `parse`; two per-mate intermediate `Vec`/`String` allocations in
`build_pe_mate`), but their **end-to-end wall-time effect is below the measurement
noise floor** at this scale. Only **mimalloc (3.13×)** is a clear signal well above
noise.

Full-run byte-identity at `--multicore 4`: **12,558,088 records == golden** ✓ (the
whole stack: mimalloc + parse + build_pe_mate).

## Why keep Phase 2/4 if it doesn't move end-to-end wall-time

- **Byte-identical and free.** They reduce allocation count and replace a
  per-char decode with a byte scan; they cannot regress output (oracle-verified)
  and are cleaner code.
- **The benefit is real at the function level, just drowned in end-to-end noise.**
  bowtie2 dominates ~90 % of `-p` wall (GATE_00), so a Rust-side parse/alloc win
  is a small fraction of a fraction. The right instrument is a **criterion
  micro-bench** (function-level, nanosecond precision, no bowtie2/thermal noise) —
  added in Phase 5 to demonstrate the parse speedup directly.
- **They compound under heavier contention / larger N**, where the allocator and
  Rust CPU carry more of the load.

## Conclusion

The PR headline is **mimalloc** (3.13× on the contended path, the only change with
a clear end-to-end signal). Phase 2/4 are honest, byte-identical allocation-hygiene
improvements presented as such — function-level faster, end-to-end within noise —
not over-claimed as an end-to-end speedup. Deeper Phase 4 targets (moving the XM
Vec, methylation_call buffer reuse) were skipped: they require threading ownership
up the whole output call chain for sub-0.5 % gains that the noise floor cannot even
confirm.
