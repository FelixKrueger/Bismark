# Full-dataset byte-identity results (archived from oxy `/var/tmp`, 2026-05-31)

Rust `bismark-methylation-extractor-rs` (`iron-chancellor @ a7aaf61`) vs Perl Bismark
v0.25.1, gzip mode. Two checks per dataset: **Rust-vs-Perl parity** (`--multicore 12`;
byte-identity is multicore-invariant) and **Rust-vs-Rust worker-invariance** across the
`--parallel` sweep. Comparison: deterministic files (`_splitting_report.txt`, `M-bias.txt`)
strict `cmp`; per-context data files `gunzip|sort|md5` (order-free); expected Perl-only
M-bias `*.png` delta excluded. Proves **parity with Perl**, not absolute correctness.

| Dataset | Size | Parity (Rust vs Perl) | File match | Worker-invariance | Verdict |
|---|---|---|---|---|---|
| WGBS-PE | 64.6M read pairs | PASS (gzip) | 8/8 (2 raw + 6 sorted-equiv) | `--parallel {1,2,4,8,16}` all match | **BYTEID PASS** |
| WGBS-SE | 63.6M reads | PASS (gzip) | 8/8 (2 raw + 6 sorted-equiv) | `--parallel {1,2,4,8,16}` all match | **BYTEID PASS** |
| RRBS-PE | 30.6M read pairs | PASS (gzip) | 8/8 (2 raw + 6 sorted-equiv) | `--parallel {1,16}` match | **BYTEID PASS** |

**All three full datasets are byte-identical to Perl Bismark v0.25.1** (parity) and produce
identical output at every worker count (deterministic `batch_seq` reorder). Combined with the
perf result (gzip ~4.8× vs Perl `--multicore 12` at full scale), the extractor's
correctness + performance campaign is complete.

Raw verbatim status files lived at `oxy:/var/tmp/fulldata_bench/byteid/byteid_<ds>.status`
(deleted in the post-campaign cleanup; verdicts captured above). Perf data: `perf_sweep_results.csv`.
