#!/bin/sh
# Copy results out of scratch into the repo and commit. /tmp is a scratch volume;
# /workspace is what survives. Called after every phase.
set -u
REPO=/workspace/Bismark
DEST=$REPO/plans/09112026_stream-converted/fullscale
LABEL=${1:-progress}

mkdir -p "$DEST/results" "$DEST/plots" "$DEST/logs"
for d in run run2m run10m_extra; do
  [ -d "/tmp/fs/$d" ] || continue
  mkdir -p "$DEST/results/$d"
  # C1 byte-identity for any arm pair that has completed since last time. This
  # is the primary claim, so it runs on every snapshot rather than at the end.
  docker run --rm -v /tmp/fs:/fs -v bismark-target-189:/t -e WORK=/fs/$d \
    bismark-ab /fs/bin/compare.sh >> "/tmp/fs/$d/compare.log" 2>&1
  [ -f "/tmp/fs/$d/C1.tsv" ] && cp "/tmp/fs/$d/C1.tsv" "$DEST/results/$d/"
  [ -f "/tmp/fs/$d/compare.log" ] && cp "/tmp/fs/$d/compare.log" "$DEST/results/$d/"
  cp /tmp/fs/$d/report_diff_*.txt "$DEST/results/$d/" 2>/dev/null
  [ -f "/tmp/fs/$d/results.tsv" ] && cp "/tmp/fs/$d/results.tsv" "$DEST/results/$d/"
  if [ -d "/tmp/fs/$d/series" ]; then
    mkdir -p "$DEST/results/$d/series"; cp /tmp/fs/$d/series/*.tsv "$DEST/results/$d/series/" 2>/dev/null
  fi
  # Regenerate plots for this dir, then copy them in.
  /tmp/fs/mamba/envs/plot/bin/python /tmp/fs/bin/plot.py "/tmp/fs/$d" >/dev/null 2>&1
  if [ -d "/tmp/fs/$d/plots" ]; then
    mkdir -p "$DEST/plots/$d"; cp /tmp/fs/$d/plots/*.png "$DEST/plots/$d/" 2>/dev/null
  fi
  # The bismark reports and stderr are small and worth keeping; BAMs are not.
  for o in /tmp/fs/$d/out/*/; do
    [ -d "$o" ] || continue
    t=$DEST/logs/$d/$(basename "$o"); mkdir -p "$t"
    cp "$o"/*_report.txt "$o"/stderr.log "$t/" 2>/dev/null
  done
done
[ -f /tmp/fs/edge.log ] && cp /tmp/fs/edge.log "$DEST/logs/"
[ -f /tmp/fs/schedule.log ] && cp /tmp/fs/schedule.log "$DEST/logs/"
[ -f /tmp/fs/genomeprep.log ] && cp /tmp/fs/genomeprep.log "$DEST/logs/"

cd "$REPO" || exit 1
git add -A plans/09112026_stream-converted/fullscale >/dev/null 2>&1
git diff --cached --quiet && { echo "snapshot: nothing new"; exit 0; }
git commit -q -m "validation: $LABEL (#1120 full-scale)

Automated snapshot from the running validation. See
plans/09112026_stream-converted/fullscale/RESULTS.md.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
echo "snapshot committed: $LABEL -> $(git log --oneline -1)"

# Push straight away: the point of snapshotting is that a lost session costs at
# most the phase in flight, and a commit that never left the box does not buy
# that. Failure here is not fatal — the commit stands and the next phase retries.
if git push -q fork HEAD:rust/stream-converted 2>&1 | tail -3; then
  echo "snapshot pushed: $(git log --oneline -1 --format=%h)"
else
  echo "snapshot push FAILED (commit is local; will retry next phase)"
fi
