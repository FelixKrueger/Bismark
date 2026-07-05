# Third-party notices — Bismark Rust suite

The Bismark Rust suite is licensed **GPL-3.0-only** (see `LICENSE` / the repo's
`license.txt`). This file records third-party software it bundles or depends on.

## Bundled external tools (the "batteries-included" Docker image only)
The container image ships these unmodified upstream binaries at **pinned**
versions (the suite's byte-identity to Perl Bismark is validated only against
these exact versions). They are *invoked as subprocesses*, not linked. Each is
distributed under its own license; the prebuilt-binary tarballs do **not** bundle
them (the user supplies them on `PATH`).

| Tool | Pinned version | License | Upstream |
|------|----------------|---------|----------|
| Bowtie 2 | 2.5.5 | GPL-3.0 | https://github.com/BenLangmead/bowtie2 |
| HISAT2 | 2.2.2 | GPL-3.0 | https://github.com/DaehwanKimLab/hisat2 |
| minimap2 | 2.31-r1302 | MIT | https://github.com/lh3/minimap2 |
| samtools / htslib | 1.23.1 | MIT/Expat | https://github.com/samtools/samtools |

Each tool's full license text is available in its upstream repository and (in the
Docker image) under its install prefix.

## Rust dependencies
Built from crates.io. The dependency tree is predominantly **MIT** and/or
**Apache-2.0** licensed (e.g. the `noodles-*` BAM/SAM/CRAM family, `clap`,
`flate2`, `gzp`, `thiserror`, `rayon`). The authoritative, version-exact list is
`Cargo.lock`; per-crate licenses are resolvable via `cargo metadata` /
`cargo-about`. No dependency imposes terms incompatible with GPL-3.0-only
distribution of the suite.

> A machine-generated, exhaustive dependency-license manifest (via `cargo-about`)
> will accompany the GA release; this notice covers the beta track.
