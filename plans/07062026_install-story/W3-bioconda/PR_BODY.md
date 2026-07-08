Updates **bismark** from Perl v0.25.1 → the **Rust suite v3.0.0**.

Bismark v3.0.0 is a ground-up Rust rewrite of the full post-alignment suite plus the
aligner wrapper, shipped 2026-07-07 (crates.io `bismark` 3.0.0; GH tag
`bismark-rust-v3.0.0`). Its faithful default path is **byte-identical** to Perl Bismark
v0.25.1. It is a single multicall binary — `bismark <subcommand>` or the classic tool
names (`deduplicate_bismark`, `bismark_methylation_extractor`, …) as `argv[0]` aliases.

### What changes in this recipe
- **Source:** GitHub tarball of the Rust tag `bismark-rust-v3.0.0` (the Perl tags are `v*`).
- **Build:** compiled with `{{ compiler('rust') }}` (was `noarch: generic`). One `bismark`
  binary + 11 classic-name symlinks (mirrors the upstream Docker/release build; not 12 fat
  binaries). Follows the Trim Galore v2.3.0 Rust recipe pattern.
- **Run deps trimmed to the external aligners only:** `bowtie2 =2.5.5`, `hisat2`, `minimap2`.
  Dropped **perl** (this is the Rust suite), **samtools** (pure-Rust `noodles` BAM/SAM/CRAM
  I/O — the suite never shells out to samtools), and **coreutils** (own I/O + pure-Rust gzip).
- **Platforms:** `linux-64`, `linux-aarch64`, `osx-arm64` (skips Intel Mac) — the three the
  upstream release builds on stable Rust; all three run deps are available on all three.

`bismark=0.25.1` (the Perl version) remains installable for anyone pinning the old behavior.

---

- [x] This PR adds a new recipe / updates an existing recipe.
- [x] AGPL, GPL v3 or other suitable license (GPL-3.0-only).
- [x] Recipe maintainers listed (`FelixKrueger`, `ewels`).
- [x] Build (`build.sh`) + tests (`test.commands`) added; `bismark --version` → 3.0.0 and
      all 12 tool names resolve.
