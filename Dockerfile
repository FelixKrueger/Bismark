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
# `--features bismark-aligner/rammap-inprocess` compiles bismark_rs's in-process
# rammap backend ON so the shipped `--rammap_inprocess` opt-in is functional
# (default `--rammap` stays subprocess). The feature is bismark-aligner-only, so
# the other 11 binaries are byte-unaffected; the rammap-core git dep is fetched at
# build (network available in the build stage). Changing this line invalidates the
# buildx layer cache once (expected — a one-time cold compile per arch).
COPY . .
RUN cargo build --release --locked --manifest-path rust/Cargo.toml --features bismark-aligner/rammap-inprocess

# ── Runtime stage ────────────────────────────────────────────
# micromamba base so the pinned aligners + samtools install cleanly from bioconda.
FROM mambaorg/micromamba:1.5.8-bookworm-slim

LABEL org.opencontainers.image.source="https://github.com/FelixKrueger/Bismark"
LABEL org.opencontainers.image.description="Bismark Rust suite (beta) — bisulfite aligner + methylation tools, byte-identical to Perl v0.25.1"
LABEL org.opencontainers.image.licenses="GPL-3.0-only"

# The micromamba base ends on `USER mambauser` (an unprivileged user); installing into the
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

# `procps` (provides `ps`) — REQUIRED by Nextflow/nf-core: the task wrapper shells
# out to `ps` to collect per-task CPU/RSS metrics on every process (even without
# `-with-trace`). The bioconda bismark image gets it transitively; this slim image
# does not, so a pipeline (e.g. nf-core/methylseq) fails without it. From Debian
# (apt) rather than conda — it is a base-OS tool. (Found by the methylseq proof run.)
RUN apt-get update \
    && apt-get install -y --no-install-recommends procps \
    && rm -rf /var/lib/apt/lists/*

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

# ── Canonical tool names (nf-core/methylseq drop-in) ─────────
# methylseq calls the Bismark tools by canonical name (no `_rs`) and captures the
# suite version from the `bismark` binary via `bismark -v`/`--version`. Exposing
# canonical names lets a methylseq container-swap PR use only a `withName` override
# (no module script edits):
#   - `bismark` is a version-probe WRAPPER — its `-v`/`--version` output is kept
#     byte-identical to the Perl v0.25.1 oracle so methylseq's versions.yml + the
#     bismark nf-test snapshots are unchanged (docker/bismark-canonical-wrapper.sh).
#   - the other 11 are plain symlinks (methylseq scrapes ONLY `bismark`, so their
#     `--version` stays the truthful Rust-suite banner).
ARG BISMARK_SUITE_VERSION=unknown
COPY docker/bismark-canonical-wrapper.sh /tmp/bismark-wrapper.sh
RUN set -eu; \
    sed "s|__SUITE_VERSION__|${BISMARK_SUITE_VERSION}|" /tmp/bismark-wrapper.sh \
        > /usr/local/bin/bismark; \
    chmod +x /usr/local/bin/bismark; \
    rm /tmp/bismark-wrapper.sh; \
    for t in deduplicate_bismark bismark_methylation_extractor bismark2bedGraph \
             coverage2cytosine bismark_genome_preparation bam2nuc NOMe_filtering \
             filter_non_conversion methylation_consistency bismark2report bismark2summary; do \
      ln -s "${t}_rs" "/usr/local/bin/${t}"; \
    done

# License + third-party notices.
COPY license.txt /usr/local/share/bismark/LICENSE
COPY rust/THIRD-PARTY-NOTICES.md /usr/local/share/bismark/THIRD-PARTY-NOTICES.md

# Drop back to the non-root base user (nf-core/Apptainer convention). The binaries
# in /usr/local/bin + the conda env in /opt/conda are world-executable, so the
# unprivileged base user (`mambauser`) runs them fine.
USER mambauser
