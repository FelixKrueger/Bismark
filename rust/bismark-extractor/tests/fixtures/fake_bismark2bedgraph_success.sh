#!/bin/sh
# Fake bismark2bedGraph for Phase G RealRunner tests. Emits a single line
# to stderr (so the drain thread observes a normal subprocess), creates a
# dummy output file matching --output, and exits clean.
echo "fake b2bg: invoked with $#" >&2
# Find --output value (defensive against test passing it or not).
while [ $# -gt 0 ]; do
    if [ "$1" = "--output" ]; then
        touch "$2" 2>/dev/null || true
        break
    fi
    shift
done
exit 0
