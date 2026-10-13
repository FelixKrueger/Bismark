---
type: usage
title: Install Bismark
description: The four ways to get the bismark binary, and the external aligners each one needs.
tags: [install, bioconda, cargo, container, nf-core]
page_id: install
subject_status: current
updated: 2026-10-12
sources:
  - repo: Bismark
    path: Dockerfile
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/Cargo.toml
    commit: 65b183a
  - repo: Bismark
    path: .github/workflows/release.yml
    commit: 65b183a
  - repo: Bismark
    path: rust/bismark/src/aligner/aligner.rs
    commit: 65b183a
  - repo: Bismark
    path: docker/bismark-canonical-wrapper.sh
    commit: 65b183a
---

# Install Bismark

## Summary

Bismark is one binary, `bismark`, that holds all 12 tools. The developer says people get it from Bioconda, crates.io, the GHCR container, or through nf-core/methylseq. The aligner step also needs an external aligner on `PATH`: Bowtie 2 by default, or HISAT2 or minimap2.

## Ways to get it

| Channel | Command | Notes |
|---|---|---|
| Bioconda | `mamba install -c bioconda -c conda-forge bismark` | The developer gave this channel. The recipe is not in this repo. |
| crates.io | `cargo install bismark` | One crate, `bismark` (`rust/bismark/Cargo.toml`). The `rammap-inprocess` and `binseq-input` features are off by default; add `--features rammap-inprocess,binseq-input` to get them. |
| Container | `docker pull ghcr.io/felixkrueger/bismark:latest` | Tags `:latest` and `:<version>` (`release.yml`). Bundles Bowtie 2 2.5.5, HISAT2 2.2.2 and minimap2 2.31, and builds with both optional features (`Dockerfile`). |
| Prebuilt binaries | GitHub release assets | Built for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` and `aarch64-apple-darwin` (`release.yml`). |
| nf-core/methylseq | Pipeline modules | The developer gave this channel. The container's `bismark --version` prints the `Bismark Version: v<version>` line that methylseq parses (`docker/bismark-canonical-wrapper.sh`). |

## Steps

1. Install the binary with one of the channels above.
2. Check it runs: `bismark --version`. `bismark --help` lists the subcommands.
3. Put an aligner on `PATH`, or pass its folder with `--path_to_bowtie2`, `--path_to_hisat2` or `--path_to_minimap2`. The aligner step checks that the aligner runs before it starts (`detect_aligner` in `aligner/aligner.rs`). If it does not, see [troubleshooting.md](../troubleshooting.md).
4. Prepare the genome once: [usage/prepare-a-genome.md](prepare-a-genome.md).

## Classic tool names

In the container each classic name (`deduplicate_bismark`, `bismark_methylation_extractor`, `bismark2bedGraph`, `coverage2cytosine` and the rest) is a symlink to the same binary (`Dockerfile`). The binary picks the tool from the name it was called by. `bismark <subcommand>` works everywhere.

BAM, SAM and CRAM reading and writing is built into the binary. `bismark dedup` and `bismark extract` accept `--samtools_path` and ignore it.

## Links

- [overview.md](../overview.md): the subcommand table.
- [usage/align-reads.md](align-reads.md): choosing an aligner.

## Open questions

- Whether a `cargo install` without the optional features should be the documented default is not in the code.
