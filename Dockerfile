# Bismark Rust suite — batteries-included image.
#
# Ships all 12 suite binaries PLUS the external tools the pipeline shells out to,
# at the PINNED versions the suite is byte-identity-validated against (a stock
# `apt-get install bowtie2` would ship a non-matching build — see
# rust/THIRD-PARTY-NOTICES.md). Pinned tool versions come from bioconda via
# micromamba (using bioconda to INSTALL deps here is unrelated to the separate,
# deferred decision not to PUBLISH Bismark to bioconda until GA).

# ── Build stage ──────────────────────────────────────────────
FROM rust:1.89-bookworm AS builder
WORKDIR /build

# The suite version is injected (the release workflow passes rust/VERSION);
# build.rs reads $BISMARK_SUITE_VERSION so every binary's --version is correct
# even without .git in the build context.
ARG BISMARK_SUITE_VERSION=unknown
ENV BISMARK_SUITE_VERSION=${BISMARK_SUITE_VERSION}

# Copy the whole repo (context = repo root) and build the workspace --locked.
# (A dep-only cache layer is omitted: the 14-crate workspace makes the dummy-src
# trick brittle; a clean --locked build is correct and CI caches via the registry.)
COPY . .
RUN cargo build --release --locked --manifest-path rust/Cargo.toml

# ── Runtime stage ────────────────────────────────────────────
# micromamba base so the pinned aligners + samtools install cleanly from bioconda.
FROM mambaorg/micromamba:1.5.8-bookworm-slim

LABEL org.opencontainers.image.source="https://github.com/FelixKrueger/Bismark"
LABEL org.opencontainers.image.description="Bismark Rust suite (beta) — bisulfite aligner + methylation tools, byte-identical to Perl v0.25.1"
LABEL org.opencontainers.image.licenses="GPL-3.0-only"

# The micromamba base ends on `USER mambauser` (UID 1000); installing into the
# base env + copying binaries into root-owned /usr/local/* needs root.
USER root

# Pinned external tools (byte-identity requires these exact versions).
RUN micromamba install -y -n base -c bioconda -c conda-forge \
      bowtie2=2.5.5 \
      hisat2=2.2.2 \
      minimap2=2.31 \
      samtools=1.23.1 \
    && micromamba clean --all --yes
ENV PATH=/opt/conda/bin:$PATH

# The 12 suite binaries (uniform `_rs` names during the beta/Perl-coexistence track).
COPY --from=builder \
    /build/rust/target/release/bismark_rs \
    /build/rust/target/release/deduplicate_bismark_rs \
    /build/rust/target/release/bismark_methylation_extractor_rs \
    /build/rust/target/release/bismark2bedGraph_rs \
    /build/rust/target/release/coverage2cytosine_rs \
    /build/rust/target/release/bismark_genome_preparation_rs \
    /build/rust/target/release/bam2nuc_rs \
    /build/rust/target/release/NOMe_filtering_rs \
    /build/rust/target/release/filter_non_conversion_rs \
    /build/rust/target/release/methylation_consistency_rs \
    /build/rust/target/release/bismark2report_rs \
    /build/rust/target/release/bismark2summary_rs \
    /usr/local/bin/

# License + third-party notices.
COPY license.txt /usr/local/share/bismark/LICENSE
COPY rust/THIRD-PARTY-NOTICES.md /usr/local/share/bismark/THIRD-PARTY-NOTICES.md
