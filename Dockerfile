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

# Copy the whole repo (context = repo root) and build the ONE `bismark` multicall
# binary --locked. `--features bismark/rammap-inprocess` compiles the in-process
# rammap backend ON (the shipped `--rammap_inprocess` opt-in; default `--rammap`
# stays subprocess), and `binseq-input` ON (so it decodes BINSEQ `.vbq`/`.cbq`; a
# default build rejects them fail-loud). Both features now live on the single
# `bismark` crate; the rammap-core git dep + the binseq crates.io dep are fetched at
# build (network available in the build stage). `--bin bismark` builds only the one
# binary — the 12 classic names are symlinks to it in the runtime stage. Changing
# this line invalidates the buildx layer cache once (a one-time cold compile/arch).
COPY . .
RUN cargo build --release --locked --manifest-path rust/Cargo.toml -p bismark --bin bismark --features bismark/rammap-inprocess,bismark/binseq-input

# ── Runtime stage ────────────────────────────────────────────
# micromamba base so the pinned aligners install cleanly from bioconda.
FROM mambaorg/micromamba:1.5.8-bookworm-slim

LABEL org.opencontainers.image.source="https://github.com/FelixKrueger/Bismark"
LABEL org.opencontainers.image.description="Bismark Rust suite — bisulfite aligner + methylation tools; faithful core byte-identical to Perl v0.25.1"
LABEL org.opencontainers.image.licenses="GPL-3.0-only"

# The micromamba base ends on `USER mambauser` (an unprivileged user); installing into the
# base env + copying binaries into root-owned /usr/local/* needs root.
USER root

# Pinned external tools (byte-identity requires these exact versions).
# No samtools: the Rust suite does its own BAM/SAM/CRAM I/O (pure-Rust noodles;
# --samtools_path is accepted-but-ignored), and nf-core/methylseq's Bismark modules
# were audited (2026-07-08, methylseq 4.2.0) to never invoke samtools in this image —
# its sort/index/flagstat/stats run in methylseq's separate samtools container.
RUN micromamba install -y -n base -c bioconda -c conda-forge \
      bowtie2=2.5.5 \
      hisat2=2.2.2 \
      minimap2=2.31 \
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

# Post-fold there is ONE multicall binary; install it as `bismark.bin` and symlink
# the 11 classic tool names to it (each self-routes on `argv[0]` via
# `bismark::cli::dispatch`, so `deduplicate_bismark …` runs the dedup path).
# `/usr/local/bin/bismark` is the version-probe WRAPPER (installed below) that
# execs `bismark.bin` — so the 11 symlinks point at `bismark.bin`, NOT the wrapper
# (a symlink to the wrapper would lose the classic `argv[0]` and run the aligner).
COPY --from=builder /build/rust/target/release/bismark /usr/local/bin/bismark.bin
RUN set -eu; cd /usr/local/bin; for b in \
      deduplicate_bismark bismark_methylation_extractor bismark2bedGraph \
      coverage2cytosine bismark_genome_preparation bam2nuc NOMe_filtering \
      filter_non_conversion methylation_consistency bismark2report bismark2summary; do \
      ln -s bismark.bin "$b"; \
    done

# ── Canonical `bismark` version-probe wrapper (nf-core/methylseq drop-in) ─────
# methylseq calls the Bismark tools by canonical name and captures the suite
# version from the `bismark` binary via `bismark -v`/`--version`. The 11 tools
# above already carry their canonical names; only `bismark` needs special
# handling, because methylseq scrapes its version banner in a shape the real
# aligner binary does NOT emit (`Bismark Aligner (Rust port) Version: …`):
#   - `/usr/local/bin/bismark` is the version-probe WRAPPER — its `-v`/`--version`
#     prints the methylseq-parseable `Bismark Version: v<suite>` banner carrying
#     the TRUE GA suite version; every other invocation execs the real aligner
#     (`bismark.bin`). See docker/bismark-canonical-wrapper.sh.
#   - the other 11 tools ARE the real binaries (methylseq scrapes ONLY `bismark`,
#     so their `--version` stays the truthful Rust-suite banner).
ARG BISMARK_SUITE_VERSION=unknown
COPY docker/bismark-canonical-wrapper.sh /tmp/bismark-wrapper.sh
RUN set -eu; \
    sed "s|__SUITE_VERSION__|${BISMARK_SUITE_VERSION}|" /tmp/bismark-wrapper.sh \
        > /usr/local/bin/bismark; \
    chmod +x /usr/local/bin/bismark; \
    rm /tmp/bismark-wrapper.sh

# License + third-party notices.
COPY license.txt /usr/local/share/bismark/LICENSE
COPY rust/THIRD-PARTY-NOTICES.md /usr/local/share/bismark/THIRD-PARTY-NOTICES.md

# Drop back to the non-root base user (nf-core/Apptainer convention). The binaries
# in /usr/local/bin + the conda env in /opt/conda are world-executable, so the
# unprivileged base user (`mambauser`) runs them fine.
USER mambauser
