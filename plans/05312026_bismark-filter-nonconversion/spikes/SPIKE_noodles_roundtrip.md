# SPIKE — noodles `RecordBuf` BAM round-trip fidelity

**Date:** 2026-05-31
**Context:** `filter_non_conversion` Rust port (last per-read data tool). Part of the
Bismark Rust rewrite (`rust/iron-chancellor`). Branch/worktree:
`rust/filter-non-conversion` @ `~/Github/Bismark-filternonconv` (off `63d589c`).

## 1. Question, success criteria, strategy

**Question.** `filter_non_conversion` is a *verbatim pass-through* filter: it reads a
Bismark BAM, decides keep/remove per read from the XM tag, and writes each record
**unchanged** to one of two output BAMs. The Perl does this through a
`samtools view -h | perl | samtools view -bS -` pipe, so its output records are
exactly samtools' BAM→SAM→BAM re-encoding of the input. The Rust port will instead
read raw noodles `RecordBuf`s and write them back via noodles' BAM writer (the
`bam2nuc` C-1 "tag-agnostic raw record" approach — NOT `bismark-io::BismarkRecord`,
which drops unmapped reads and requires XR/XG). **Does a noodles `RecordBuf` round-trip
produce a `samtools view` record body byte-identical to (a) the original and (b) the
Perl-pipe reference?** If not, byte-identity of the kept/removed BAMs is impossible
without a SAM-text passthrough fallback.

**Success criteria.** For a real Bismark PE BAM, `samtools view` (record body, header
excluded) of `noodles_roundtrip(input)` is byte-identical to both `samtools view input`
and `samtools view <samtools-pipe(input)>`, for every record, with matching record count.

**Scope boundary (throwaway).** No filtering logic, no report, no performance, no
unmapped-read handling (the fixture is all-mapped). Single small fixture only — scale
is the job of the real-data gate. Header `@PG` divergence is known and out of scope
(the gate compares the body, per the resolved decision).

## 2. Script path and how to run

- Runnable artifact (cargo example, uses `bismark-io`'s pinned noodles deps):
  `rust/bismark-io/examples/fnc_roundtrip_spike.rs` (in the worktree; **throwaway**, untracked).
- Copy for the record: `plans/05312026_bismark-filter-nonconversion/spikes/fnc_roundtrip_spike.rs`.

```sh
cd ~/Github/Bismark-filternonconv/rust
cargo run -q -p bismark-io --example fnc_roundtrip_spike -- <in.bam> <out.bam>
# then, body-only comparison (header excluded):
samtools view <in.bam>  > a.sam
samtools view <out.bam> > b.sam
cmp a.sam b.sam
```

Fixture used: `rust/bismark-io/test_files/tiny_pe_bismark.bam` (203 records, real Bismark
PE alignment, GRCh-style Ecoli test genome; tags `NM:i, MD:Z, XM:Z, XR:Z, XG:Z`).

## 3. Results

**Iteration 1** (only iteration needed):

```
noodles round-trip wrote 203 records
record counts: orig=203 noodles=203 perl=203
=== noodles body vs original body ===   IDENTICAL
=== noodles body vs perl-pipe body ===  IDENTICAL
```

Meets all success criteria on the first run.

Supporting facts established alongside the spike:
- `samtools view` appends a `@PG` line on **every** invocation → the Perl pipe adds two
  extra `@PG` lines per run; noodles adds none. Headers can never match → **body-only
  comparison is mandatory** (resolved decision).
- samtools' own BAM→SAM→BAM round-trip is body-byte-identical (the Perl reference is faithful).
- XM call-char inventory on the fixture: `.`, `H`, `X`, `Z`, `h`, `x`, `z` (no `u`/`U`,
  which can occur in real data — cover in synthetic fixtures).

## 4. Findings summary

The noodles `RecordBuf`→BAM→`samtools view` body is **byte-identical** to both the
original and the Perl `samtools view -h | … | samtools view -bS -` reference. Tag order
(`NM, MD, XM, XR, XG`) and values survive the round-trip. The decoded-body comparison is
additionally **immune to BAM integer-width re-encoding** (SAM text renders all integer
tags as `i` regardless of the underlying BAM subtype) and to BGZF block-boundary
differences (we never diff raw `.gz`). So the verbatim-passthrough design is sound.

## 5. Reference snippets for implementation

Read path (raw, tag-agnostic, includes unmapped — unlike `bismark-io`'s reader):

```rust
use std::fs::File; use std::io::BufReader;
use noodles_bam as bam;
use noodles_sam::alignment::io::Write as _; // brings write_alignment_record into scope

let mut reader = bam::io::Reader::new(BufReader::new(File::open(input)?));
let header = reader.read_header()?;                       // noodles_sam::Header
let mut writer = bam::io::Writer::new(File::create(output)?); // wraps BGZF internally
writer.write_header(&header)?;
for result in reader.record_bufs(&header) {              // yields io::Result<RecordBuf>
    let record = result?;
    // ... decide keep/remove from record.data() XM tag ...
    writer.write_alignment_record(&header, &record)?;    // RecordBuf: impl Record
}
writer.try_finish()?;                                    // BGZF EOF marker (inherent method)
```

Notes for the real crate:
- `record_bufs(&header)` yields **all** records incl. unmapped (FLAG&0x4) — matches the
  Perl `samtools view -h` line stream. Do NOT route through `bismark-io`'s reader.
- Two output writers (kept + removed) get the **same** header written verbatim.
- XM tag access: `record.data().get(&Tag::from(*b"XM"))` → `Some(Value::String(s))`;
  **absent XM is legal** (SE keeps the read; PE errors). See `bismark-io::tags::xm` shape.
- `--parallel` BGZF threading is available via `MultithreadedReader/Writer` if wanted later
  (deferred for v1.0 per scoping).

## 6. Recommendation

**Proceed** with the verbatim-passthrough design on raw noodles `RecordBuf` (read via
`noodles_bam::io::Reader::record_bufs`, write via `noodles_bam::io::Writer`). Byte-identity
of the kept/removed BAM bodies is achievable without any SAM-text fallback. Lock the gate
to **body-only** comparison (`samtools view`, not `-H`).

## 7. Limitations

- One small fixture (203 records, all mapped, no `u`/`U` calls, no CRAM). The real-data
  byte-identity gate (colossal/oxy, 10M SE + PE) remains the production-scale proof.
- Did not exercise **unmapped** reads through the round-trip (fixture has none). Synthetic
  CI fixtures must include an unmapped record to prove it is passed through verbatim and
  routed to OUT (Perl keeps unmapped reads — XM absent → SE keeps; for PE an unmapped mate
  would need an XM or it dies — cover this in the SPEC's edge cases).
- Did not test a non-samtools-normalized "raw" Bowtie2 BAM; but the body comparison is
  against the Perl pipe (itself samtools-normalized), and noodles matched it exactly, so
  normalization parity is the relevant property and it holds.
