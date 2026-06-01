# Spike — Perl `glob` sort order for `bismark2summary` BAM discovery

**Date:** 2026-06-01
**Question:** SPEC rev-0 §2.3/§4.8/§8.6 specified the BAM-discovery glob sort as **bytewise** (`Vec::sort()` / `LC_ALL=C`). Reviewer A (Critical) said Perl `glob` uses **case-folding collation**, not bytewise; Reviewer B (Important) assumed bytewise == Perl for ASCII names. **Direct contradiction**, and byte-identity-critical: row order in *both* the `.txt` and the `.html` (categories, all 13 y-arrays, the `num_samples`/x-values count) is the discovery order. This spike settles it empirically on both byte-identity platforms.

## Method
Created Bismark-style BAM filenames in a temp dir, then compared `perl -e 'print "$_\n" for <*bismark_bt2.bam>'` against bytewise sort and the candidate Rust key `(ascii_lowercased, original_bytes)`, under multiple locales, on **macOS** (local, Perl v5.34.1) and **oxy/Linux** (Perl v5.38.2, kernel 6.12 amzn2023).

## Results

### Mixed-case, distinguishing position (`apple`, `Mango`, `zebra`, `Banana`, `cherry`, `Delta`, `a`, `b`, `z`)
**Perl glob — identical on macOS and Linux, invariant across default / `LC_ALL=C` / `en_US.UTF-8`:**
```
a, apple, b, Banana, cherry, Delta, Mango, z, zebra
```
- **Case-fold-primary**: `Banana` after `b_`, `Mango` after `cherry`/`Delta` (folded `m` > `d`), not uppercase-first.
- **Raw bytes secondary within a folded group**: `a_…` before `apple_…` (`_`=0x5F < `p`); `b_…` before `Banana…` (`_`=0x5F < `a`).

**Bytewise / codepoint sort (== Rust `str::sort()`, == `LC_ALL=C sort`):**
```
Banana, Delta, Mango, a, apple, b, cherry, z, zebra   ← DIFFERENT (uppercase first)
```

**Candidate Rust key `sort_by_key(|n| (n.to_ascii_lowercase(), n.clone()))`:** **matches Perl exactly.**

### Case-only differences (`Apple`, `aPPle`, `apple`, `BETA`, `beta`, `Zeta`, `zeta`) — Linux only (macOS FS is case-insensitive, can't hold them)
**Perl glob:**
```
Apple, aPPle, apple, BETA, beta, Zeta, zeta
```
**`(ascii_lowercased, original_bytes)` key:** **matches Perl exactly** (tie within folded `apple`: raw bytes `A`=0x41 < `aP`=0x61,0x50 < `ap`=0x61,0x70).

## Conclusion (RESOLVED — Reviewer A correct, Reviewer B's bytewise assumption wrong)
- Perl `glob`/`<*…>` sorts **case-fold-primary, raw-ASCII-bytes-secondary**, and is **locale-invariant and platform-invariant** (macOS Perl 5.34.1 ≡ Linux Perl 5.38.2). My initial "Linux uses bundled `strcmp` → bytewise" hypothesis was **refuted** by the oxy run.
- **Rust implementation:** collect each glob's matches, then `sort_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()).then_with(|| a.as_bytes().cmp(b.as_bytes())))` (equivalently `sort_by_key((to_ascii_lowercase, bytes))`). A single comparator matches Perl on every platform/locale tested — **no platform-conditional logic needed**. Do **NOT** use a plain bytewise `.sort()` and do **NOT** use the `glob` crate's default (also bytewise).
- Scope: applies only to the **auto-glob** path (per-glob sort, then concatenated in the fixed four-glob order). The explicit-`@ARGV` path uses argv order verbatim — unaffected.
- **Fixture required:** a mixed-case multi-sample auto-glob directory (e.g. `apple_…`, `Mango_…`, `zebra_…`) whose Perl-vs-Rust row order differs under a bytewise sort. Without it the gate cannot catch a bytewise regression.

## Caveat
ASCII-only reasoning. Bismark BAM names are ASCII in practice; `to_ascii_lowercase` folds only `A–Z`. Non-ASCII filenames were not tested (and Bismark never produces them) — documented divergence boundary, not a v1.0 concern.
