#!/bin/sh
# Canonical-name `bismark` shim for the Rust-suite container (nf-core/methylseq drop-in).
#
# methylseq's 7 Bismark modules ALL capture the suite version from the `bismark`
# binary (verified against methylseq master), via two parser shapes:
#   Parser-1 (align, topic: versions): `bismark --version | grep Version | sed -e 's/Bismark Version: v//' | xargs`
#   Parser-2 (the other 6 modules):    `echo $(bismark -v 2>&1) | sed 's/^.*Bismark Version: v//; s/Copyright.*$//'`
#
# To keep the captured value BYTE-IDENTICAL to the Perl v0.25.1 oracle (so the
# modules' versions.yml — and their nf-test snapshots — are unchanged), we
# reproduce the Perl `bismark -v` banner shape: a `Bismark Version: v0.25.1`
# line + a `Copyright`-prefixed line, then honest Rust-suite provenance on
# line(s) AFTER `Copyright` containing NO "Version" token. Result:
#   Parser-1 → "0.25.1"   Parser-2 → "0.25.1 "   (exactly the Perl oracle).
#
# The `Bismark Version: v0.25.1` token is the FIXED byte-identity oracle version
# (= the suite's behavioural target + methylseq's pinned container tag), NOT the
# Rust suite version; only the provenance line carries the suite version
# (__SUITE_VERSION__, injected from $BISMARK_SUITE_VERSION at image build).
#
# Every NON-version invocation execs the unchanged `bismark_rs` binary (argv,
# stdin/stdout/stderr, exit code all preserved). methylseq always passes the
# version flag as the sole argument, so matching `$1` is sufficient.
case "$1" in
  --version|-v)
    printf 'Bismark Version: v0.25.1\nCopyright 2010-25 Felix Krueger, Altos Bioinformatics\nhttps://github.com/FelixKrueger/Bismark\n(Bismark Rust suite __SUITE_VERSION__ — byte-identical reimplementation of Bismark v0.25.1)\n'
    exit 0 ;;
esac
exec bismark_rs "$@"
