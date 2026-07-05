#!/bin/sh
# Canonical-name `bismark` version-probe wrapper for the Rust-suite container
# (nf-core/methylseq drop-in).
#
# methylseq's 7 Bismark modules ALL capture the suite version from the `bismark`
# binary (verified against methylseq master), via two parser shapes:
#   Parser-1 (align, topic: versions): `bismark --version | grep Version | sed -e 's/Bismark Version: v//' | xargs`
#   Parser-2 (the other 6 modules):    `echo $(bismark -v 2>&1) | sed 's/^.*Bismark Version: v//; s/Copyright.*$//'`
#
# The REAL aligner binary's own `--version` is `Bismark Aligner (Rust port)
# Version: <v>` — NOT the `Bismark Version: v<v>` shape methylseq greps for. So
# we keep a thin wrapper that reproduces the Perl banner SHAPE (a `Bismark
# Version: v<v>` line + a `Copyright`-prefixed line) but emits the TRUE suite
# version. GA: honest provenance — this DROPS the beta-era impersonation of
# v0.25.1 and reports the real suite version, so every run records `bismark
# 2.0.0`. Result for suite version 2.0.0:
#   Parser-1 → "2.0.0"   Parser-2 → "2.0.0"
#
# __SUITE_VERSION__ is injected from $BISMARK_SUITE_VERSION at image build (=
# rust/VERSION). methylseq re-baselines its nf-test version snapshots on this GA
# bump (one-time, normal version-bump maintenance — see the GA epic OD-7).
#
# Every NON-version invocation execs the unchanged real aligner, installed in
# the image as `bismark.bin` (argv, stdin/stdout/stderr, exit code all
# preserved). methylseq always passes the version flag as the sole argument, so
# matching `$1` is sufficient.
case "$1" in
  --version|-v)
    printf 'Bismark Version: v__SUITE_VERSION__\nCopyright 2010-25 Felix Krueger, Altos Bioinformatics\nhttps://github.com/FelixKrueger/Bismark\n'
    exit 0 ;;
esac
exec bismark.bin "$@"
