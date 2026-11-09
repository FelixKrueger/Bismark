#!/bin/bash
set -xeuo pipefail

# Pin the --version banner to the package version. build.rs otherwise resolves it
# from rust/VERSION (shipped in the tarball), but the GitHub archive has no .git, so
# the git short-hash degrades to "unknown"; setting the suite version explicitly (as
# the GA Dockerfile / release.yml do) guarantees `bismark --version` reports 3.0.0.
export BISMARK_SUITE_VERSION="${PKG_VERSION}"

# Build the ONE `bismark` multicall binary from the workspace crate under rust/.
# Byte-identical invocation to the GA Dockerfile / release.yml: features
# rammap-inprocess (the opt-in in-process rammap backend) + binseq-input (decode
# .vbq/.cbq) — both default-OFF, so they must be named explicitly. Dependencies
# (incl. the rammap-core git dep, pinned =1.1.1 @ rev 5ea62cd live on GitHub, and
# the binseq crates.io dep) are fetched at build — bioconda's compiled-recipe build
# env has network, and --locked builds exactly the shipped Cargo.lock resolution.
cargo build --release --locked \
    --manifest-path rust/Cargo.toml \
    -p bismark --bin bismark \
    --features bismark/rammap-inprocess,bismark/binseq-input

# conda-forge's rust compiler sets CARGO_BUILD_TARGET (e.g. x86_64-conda-linux-gnu),
# so the artifact lands under target/<triple>/release; fall back to the plain path
# (rustup layout, e.g. a local build).
BIN="rust/target/${CARGO_BUILD_TARGET:-}/release/bismark"
[ -f "$BIN" ] || BIN="rust/target/release/bismark"

install -d "${PREFIX}/bin"
install -m 0755 "$BIN" "${PREFIX}/bin/bismark"

# Ship the 11 classic tool names as relative symlinks to the one binary; it self-routes
# on argv[0] (bismark::cli::dispatch), so `deduplicate_bismark ...` runs the dedup path.
cd "${PREFIX}/bin"
for b in deduplicate_bismark bismark_methylation_extractor bismark2bedGraph \
         coverage2cytosine bismark_genome_preparation bam2nuc NOMe_filtering \
         filter_non_conversion methylation_consistency bismark2report bismark2summary ; do
    ln -s bismark "$b"
done
