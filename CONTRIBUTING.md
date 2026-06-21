# Contributing to Bismark

Thank you for your interest in contributing to Bismark.

## Bismark (Perl) is in maintenance freeze

The Perl version of Bismark (`v0.25.x`, this repository's default branch) is in **maintenance freeze**.
It now receives **critical correctness and security fixes only** — no new features and no performance
changes.

Bismark is being reimplemented in Rust. The
**[Bismark Rust suite](https://felixkrueger.github.io/Bismark/rust/overview/)** reimplements every tool
and is byte-identical to Perl `v0.25.1` on the faithful default path (the opt-in combined-index and
`rammap` modes are concordance-gated rather than byte-identical). It is faster, lower-memory,
worker-invariant, and actively developed, and it adds capabilities the Perl architecture cannot reach.
It is currently in beta. At the Rust general release the Perl code will be archived as tagged legacy,
following the model of [Salmon's `cpp` branch](https://github.com/COMBINE-lab/salmon).

This is a freeze, not a judgement on the quality of contributions. It lets development effort go where
it now compounds.

## What is still welcome on the Perl version

- **Critical bug fixes** — a correctness regression or a crash.
- **Security fixes.**

For either, please **open an issue first** describing the problem, so we can confirm it belongs on the
frozen branch before you invest time in a pull request.

## What should go to the Rust suite instead

Everything else: **new features, performance work, new aligners or modes, and refactors.** The Rust
suite is developed on the **`rust/iron-chancellor`** branch. Good starting points:

- [Rust rewrite: scope and motivation](https://felixkrueger.github.io/Bismark/rust/overview/)
- [Benchmarks](https://felixkrueger.github.io/Bismark/rust/benchmarks/)

## A note on performance contributions

Performance contributions are genuinely appreciated. The reason they now belong in the Rust suite
rather than the Perl code is **end-to-end**: the Rust tools are faster *and* lower-memory *and*
worker-invariant, and they keep their gains as cores are added, where the Perl wrapper's single-threaded
design saturates. That combination is not reachable by a Perl patch, however well its hot loops are
optimised (see the [benchmarks](https://felixkrueger.github.io/Bismark/rust/benchmarks/)). Performance
changes to the Perl version will not be merged during the freeze; we would much rather see the same
effort directed at the Rust suite, where it carries forward instead of into a frozen codebase.

## Getting in touch

Please [open an issue](https://github.com/FelixKrueger/Bismark/issues) — the pinned freeze announcement
has more detail and is a good place to ask where a contribution best fits. Thank you again for
contributing to Bismark.
