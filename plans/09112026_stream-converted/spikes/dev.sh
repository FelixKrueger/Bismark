#!/bin/sh
# Reproducibility-gate builds only: the repo pins rust 1.89 (Dockerfile,
# `just reproduce`), while this machine's local toolchain is newer. Day-to-day
# iteration should just use local cargo — this wrapper is for confirming a
# result holds on the pinned toolchain.
#   spikes/dev.sh build --locked -p bismark --bin bismark
set -eu
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
exec docker run --rm -t \
  -v "$REPO":/repo \
  -v bismark-cargo-registry:/usr/local/cargo/registry \
  -v bismark-cargo-git:/usr/local/cargo/git \
  -v bismark-target-189:/repo/rust/target \
  -w /repo rust:1.89-bookworm \
  cargo "$@" --manifest-path rust/Cargo.toml
