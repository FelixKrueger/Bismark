# Changelog — `bismark-nome-filtering`

All notable changes to the standalone NOMe-Seq filter port are documented here.

## v1.0.0-beta.1 — 2026-06-01

Initial release. Rust port of the **standalone** Perl `NOMe_filtering` (v0.25.1) —
a per-read NOMe-Seq classifier. (Not to be confused with `coverage2cytosine
--nome-seq`, a separate in-c2c flag.)

### Byte-identity
- **Byte-identical to Perl `NOMe_filtering` v0.25.1.** Validated by synthetic
  Perl-generated decompress-then-compare goldens (CG ACG/TCG accept, GCG reject,
  GpC CHG/CHH, the reverse-`end∈{1,2}` all-zero / forward-`start≤3` no-line edge
  asymmetry, the empty-input header-only `.gz`, gz-input, CRLF, unknown-chr,
  N-context, reverse-strand counting, multi-chromosome emission order) **and** a
  full real-data gate on oxy: the 10M SE dataset (10.29 GB `--yacht` input,
  hg38) → 8,494,374 output lines, decompressed-byte-identical (md5
  `7bdf7d5d9735246d10aa657c27695ce4`).

### Features
- Reads the methylation extractor's `--yacht` 8-field per-call output (gz-aware;
  skips `^Bismark` header lines; groups consecutive same-ReadID calls).
- Per-read NOMe filter: CpG only in A-CG / T-CG context; GpC (non-CG) only when
  preceded by G. Tallies `meth_CG / unmeth_CG / meth_GC / unmeth_GC` per read.
- Always-gzipped per-read report `<stem>.manOwar.txt.gz`.
- Genome reader promoted to a shared, tier-parameterized `bismark_io::genome`
  module (additive to `bismark-io`, no version bump). NOMe uses the two **plain**
  suffixes `.fa` → `.fasta` (no `.gz`) — matching Perl; a `.fa.gz`-only genome
  folder is intentionally not found.

### Performance
- ~3.4× faster than Perl single-threaded (1:28 vs 5:01 on the 10M SE input);
  near-identical memory (~3.1 GB RSS, the genome held in RAM, matching Perl).

### Accepted divergences (out-of-distribution / non-gated)
- Malformed yacht lines (`<8` fields / non-numeric coords) are skipped; non-UTF-8
  input errors; error exit code is `1` (Perl `die` → 255). None occur on real
  `--yacht` output; STDERR/exit codes are not byte-identity-gated. See the SPEC
  "Accepted divergences" section.
