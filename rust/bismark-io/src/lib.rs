//! `bismark-io` — Bismark-aware BAM/SAM/CRAM I/O on top of [`noodles`].
//!
//! This crate is the shared library for Bismark's Rust rewrite, exposing
//! Bismark-flavoured wrappers around [`noodles`] record types — strand-
//! classified, tag-decoded, CIGAR-aware.
//!
//! See `DESIGN.md` (in the crate root) for the design contract and
//! `~/.claude/plans/05232026_bismark-io-v1/PLAN.md` for the implementation
//! plan. The crate is sync-only for v1.0; pure-Rust with no `samtools`
//! subprocess, no `htslib` C link, no `unsafe` blocks.
//!
//! [`noodles`]: https://github.com/zaeleus/noodles

#![forbid(unsafe_code)]

pub mod cigar;
pub mod cram_ref;
pub mod error;
pub mod pair;
pub mod read;
pub mod record;
pub mod strand;
pub mod tags;

pub use cigar::{AlignedPosition, AlignedPositions, CigarExt};
pub use cram_ref::reconstitute_cram_reference_from_bismark_genome;
pub use error::BismarkIoError;
pub use pair::BismarkPair;
pub use read::{AlignmentKind, AnyReader, BamReader, CramReader, SamReader, open_reader};
pub use record::{BismarkRecord, ReadIdentity};
pub use strand::BismarkStrand;
