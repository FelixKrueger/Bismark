#!/bin/sh
set -eu
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
docker build -q -t bismark-ab -f "$REPO/plans/09112026_stream-converted/spikes/Dockerfile.ab" \
  "$REPO/plans/09112026_stream-converted/spikes" >/dev/null
exec docker run --rm -t \
  -v "$REPO":/repo \
  -v bismark-cargo-registry:/usr/local/cargo/registry \
  -v bismark-cargo-git:/usr/local/cargo/git \
  -v bismark-target-189:/repo/rust/target \
  -v bismark-bench-work:/work \
  -w /repo bismark-ab \
  bash /repo/plans/09112026_stream-converted/spikes/bench-ab.sh
