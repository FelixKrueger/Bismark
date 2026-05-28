#!/bin/sh
# Fake bismark2bedGraph that fails. Emits an error message to stderr and
# exits non-zero.
echo "fake b2bg ERROR: deliberate failure for test" >&2
exit 7
