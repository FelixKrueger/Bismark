//! `bismark-io` — Bismark-aware BAM/SAM/CRAM I/O on top of [`noodles`].
//!
//! This crate is the shared library for Bismark's Rust rewrite, exposing
//! Bismark-flavoured wrappers around [`noodles`] record types — strand-classified,
//! tag-decoded, CIGAR-aware.
//!
//! See `DESIGN.md` (in the crate root) for the design contract. Implementation
//! lands incrementally via sub-issues of the parent epic.
//!
//! [`noodles`]: https://github.com/zaeleus/noodles
