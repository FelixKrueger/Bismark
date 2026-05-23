# `bismark-io` — design doc

Status: draft for review. Once approved, this lands at `rust/bismark-io/DESIGN.md` as the deliverable of [#799](https://github.com/FelixKrueger/Bismark/issues/799), part of [#794](https://github.com/FelixKrueger/Bismark/issues/794).

## Goals

`bismark-io` is the **shared library crate** for Bismark Rust rewrite's BAM/SAM/CRAM I/O. It wraps [`noodles`](https://github.com/zaeleus/noodles) to expose Bismark-aware record types — strand-classified, tag-decoded, soft-clip-aware. Every per-binary crate (dedup, extractor, bedgraph, …) consumes it.

Design priorities, in order:

1. **Structural correctness over ergonomic shortcuts.** Strand is a property of a read (or pair), not of a position-on-a-read. Make the latter impossible to express.
2. **Zero external runtime deps.** No `samtools` subprocess, no `htslib` C link.
3. **Byte-equal output to Perl Bismark v0.25.1** is a CI gate. The library exposes APIs that let callers produce that — including matching Perl's output conventions where they differ from natural Rust defaults.
4. **Testable without I/O.** Pure functions (CIGAR span, strand derivation, tag decoding) live behind interfaces that take byte slices, not `File`s.

Non-goals:

- High-level pipeline orchestration. `bismark-io` is a library; orchestration lives in the binary crates.
- A complete typed Bismark record. We expose what Bismark binaries need; we do not aim to model every SAM optional field.

## Workspace location

The Rust rewrite lives in `rust/` at the top of the Bismark repo. Tree:

```
Bismark/
├── bismark                            # Perl (unchanged)
├── bismark_methylation_extractor      # Perl (unchanged)
├── … (other Perl scripts)
├── rust/
│   ├── Cargo.toml                     # workspace manifest
│   ├── Cargo.lock
│   ├── bismark-io/                    # this crate
│   │   ├── Cargo.toml
│   │   ├── DESIGN.md                  # this file
│   │   └── src/
│   ├── bismark-dedup/                 # binary crate (Phase 1)
│   ├── bismark-bedgraph/              # binary crate (Phase 1)
│   ├── bismark-extractor/             # binary crate (Phase 1)
│   └── … (Phase 2/3 binary crates added incrementally)
└── …
```

Rust binaries take an **`_rs` suffix** during the coexistence period (v0.26 → v1.0) so users can install Rust + Perl side-by-side without PATH conflicts:

| Perl                              | Rust binary name (during coexistence) |
|-----------------------------------|---------------------------------------|
| `deduplicate_bismark`             | `deduplicate_bismark_rs`              |
| `bismark_methylation_extractor`   | `bismark_methylation_extractor_rs`    |
| `bismark2bedGraph`                | `bismark2bedGraph_rs`                 |
| `coverage2cytosine`               | `coverage2cytosine_rs`                |

After v1.0 of the Rust port, the `_rs` suffix is dropped and the Rust binaries become the default `deduplicate_bismark` etc. The Perl scripts move to a `legacy/` directory at that point.

## Crate layout (Phase 1)

```
rust/bismark-io/src/
├── lib.rs                # public exports + crate-level rustdoc
├── error.rs              # BismarkIoError (thiserror)
├── strand.rs             # BismarkStrand enum + From<XR/XG> derivation
├── record.rs             # BismarkRecord wrapper over noodles::sam::Record
├── tags.rs               # XM/XR/XG/MD/NM accessors
├── cigar.rs              # Bismark-flavoured CIGAR helpers
├── read.rs               # BamReader / SamReader
└── write.rs              # BamWriter / SamWriter
```

Aspirational shared-crate split (Phase 2 or later, NOT in scope for #799):

- `bismark-core` — domain types shared across binaries (CytosineContext, ReadIdentity, splitting-report shapes)
- `bismark-report` — shared report-writing helpers (M-bias tables, splitting reports)
- `bismark-cli` — Perl-flag-parity CLI conventions, alias mappings

For Phase 1, all of these stay inside `bismark-io` or inside the per-binary crates. We refactor outwards only when sharing actually pays.

## Decisions on the four open questions

### Q1: Strand classification API

**Decision: eager classification at parse time, but with an explicit distinction between *per-record* and *per-pair* strand.**

Bismark encodes the strand across TWO SAM optional tags rather than a single field:

| Tag      | Meaning                                                              | Values       |
|----------|----------------------------------------------------------------------|--------------|
| `XR:Z:`  | Read conversion (which way this record was sequenced rel. to alignment) | `CT` or `GA` |
| `XG:Z:`  | Genome conversion (which converted reference this record aligned to)    | `CT` or `GA` |

The four-way classification is derived from the 2×2 combination:

| XR | XG | Strand |
|----|----|--------|
| CT | CT | OT (Original Top)         |
| GA | CT | CTOT (Complementary to OT)|
| CT | GA | OB (Original Bottom)      |
| GA | GA | CTOB (Complementary to OB)|

**Critical subtlety for paired-end data**: within a single PE pair from a directional library, R1 and R2 have **different per-record XR/XG** combinations. For an OT pair, R1 has `XR=CT, XG=CT` (per-record strand = OT) but R2 has `XR=GA, XG=CT` (per-record strand = CTOT). This is because R2 is sequenced from the complementary direction — same library molecule, different sequencing direction. Verified in real data: `samtools view ...deduplicated.bam | head -2` shows exactly this on the audit dataset.

This is precisely where the prior-art port's strand-routing bug lived. If output files are chosen by per-record strand, R1's calls go to `CpG_OT_*` and R2's calls go to `CpG_CTOT_*` — splitting one pair's calls across two files. **Perl correctly uses R1's strand to classify the whole pair, routing both R1 and R2 calls to the same OT file.**

`bismark-io` makes this distinction explicit at the type level:

```rust
pub enum BismarkStrand { OT, CTOT, OB, CTOB }

impl BismarkStrand {
    pub fn from_xr_xg(xr: &[u8], xg: &[u8]) -> Result<Self, BismarkIoError>;
}

pub struct BismarkRecord {
    inner: noodles::sam::Record,
    record_strand: BismarkStrand,    // derived from THIS record's XR/XG, at parse time
    // … other decoded tags
}

impl BismarkRecord {
    /// Strand derived from this record's own XR/XG.
    /// For R2 of a directional OT pair this returns CTOT — NOT the pair strand.
    /// Output routing for paired-end data should use BismarkPair::pair_strand() instead.
    pub fn record_strand(&self) -> BismarkStrand;
    pub fn read_identity(&self) -> ReadIdentity;   // R1 / R2 / SE
}

pub struct BismarkPair {
    r1: BismarkRecord,
    r2: BismarkRecord,
    pair_strand: BismarkStrand,    // derived from R1, governs output routing for both mates
}

impl BismarkPair {
    /// Library-level strand of this pair. Output routing for both R1 AND R2
    /// methylation calls uses this — not record_strand() per mate.
    pub fn pair_strand(&self) -> BismarkStrand;
    pub fn r1(&self) -> &BismarkRecord;
    pub fn r2(&self) -> &BismarkRecord;
}
```

For single-end data, `record_strand() == pair_strand()` trivially; binaries that handle SE-only paths can use `BismarkRecord` directly without going through `BismarkPair`.

For directional libraries (the common case), only OT and OB pair-strands occur. CTOT/CTOB pair-strands only appear in non-directional library prep.

**Rationale**:
1. Per-record strand is cheap to compute (one tag-pair lookup), stored at parse time.
2. Pair-strand is the routing decision and is decided by R1 — exposing it as a separate typed field on `BismarkPair` makes it impossible for a caller to accidentally route by per-record strand.
3. The cost is negligible compared to BAM decompression; the structural correctness benefit is removing an entire class of bugs from every downstream binary.

### Q2: CIGAR helper API

**Decision: both — expose `noodles::sam::record::Cigar` for raw access, add Bismark-specific helpers on top.**

```rust
pub trait CigarExt {
    /// Reference-span (M+D+N+=+X ops). Used by dedup PE key derivation and
    /// reverse-strand end-of-alignment calc. Do NOT use pos.saturating_sub(1).
    fn reference_span(&self) -> u32;

    /// Yield (read_position, ref_position, op) triples in alignment order.
    /// Handles soft-clips at edges, indels, ref-skips.
    fn aligned_positions(&self) -> impl Iterator<Item = (usize, u64, CigarOp)>;

    /// Read-portion length (M+I+S+=+X). Used to validate XM/XR/seq length parity.
    fn read_span(&self) -> u32;
}
```

The helpers live in `cigar.rs` and are implemented as an extension trait on `noodles::sam::record::Cigar`. Callers can still use the underlying noodles type directly if they need rare operations.

### Q3: CRAM support

**Decision: CRAM read AND write supported in v1.0, both via `noodles-cram`. No `samtools` subprocess.**

Perl Bismark's aligner already writes CRAM optionally — but it does so by piping BAM through `samtools view -h -C -T <ref>` (see `bismark:1448`, `:1601`, `:1803`). That means Perl Bismark CRAM users currently have a hidden samtools dependency. Our Rust port using `noodles-cram` for both read AND write is **strictly stronger** than Perl's current CRAM guarantee — it eliminates the samtools dependency for CRAM users.

```rust
pub enum AlignmentKind { Bam, Sam, Cram }

pub fn open_reader(path: &Path, cram_ref: Option<&Path>)
    -> Result<Box<dyn BismarkRecordReader>, BismarkIoError>;

pub fn open_writer(path: &Path, header: &noodles::sam::Header, cram_ref: Option<&Path>)
    -> Result<Box<dyn BismarkRecordWriter>, BismarkIoError>;
```

CRAM is a reference-based compression format — both reader and writer need the reference FASTA used during alignment. The API takes an optional `cram_ref: &Path`:

- For CRAM **read**: required (unless an embedded reference is present, which Bismark CRAM doesn't use).
- For CRAM **write**: required.
- For BAM / SAM: ignored.

`bismark-io` also exposes a helper to reconstitute a multi-FASTA reference from a Bismark genome directory, matching Perl Bismark's behaviour at `bismark:5131` (`Bismark_genome_CRAM_reference.mfa`):

```rust
pub fn reconstitute_cram_reference_from_bismark_genome(
    bismark_genome_dir: &Path,
    output: &Path,
) -> Result<(), BismarkIoError>;
```

This keeps the user-facing experience of `--cram_ref` (binaries) compatible with Perl Bismark's behaviour while doing the I/O natively in Rust.

Phase 1 binaries that use CRAM write:

- `deduplicate_bismark_rs` — when input is CRAM, output is CRAM (matches Perl behaviour: dedup preserves input format).

Phase 3 binaries that use CRAM write:

- `bismark_rs` (aligner) — `--cram` + `--cram_ref` flags as Perl, but routed through `noodles-cram` natively.

Closes user-facing request: [#788](https://github.com/FelixKrueger/Bismark/issues/788) (CRAM support, both read and write).

### Q4: Error handling

**Decision: `thiserror` typed errors in the lib; binaries wrap in `anyhow::Error` for top-level reporting.**

```rust
#[derive(Debug, thiserror::Error)]
pub enum BismarkIoError {
    #[error("malformed BAM/SAM record at offset {offset}: {reason}")]
    MalformedRecord { offset: u64, reason: String },

    #[error("missing required Bismark tag: {tag}")]
    MissingTag { tag: &'static str },

    #[error("invalid XR/XG combination: XR={xr:?}, XG={xg:?}")]
    InvalidStrandTags { xr: Vec<u8>, xg: Vec<u8> },

    #[error("unsupported file kind for path: {0}")]
    UnsupportedKind(PathBuf),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("noodles BAM error: {0}")]
    Bam(#[from] noodles::bam::io::reader::Error),

    // … additional variants as the surface area grows
}
```

Binary crates depend on `bismark-io` for these typed errors plus `anyhow` for chaining context at orchestration level (e.g. "while processing chunk N of file X").

## Type sketches

Top-level API surface (sketch, will firm up during impl):

```rust
// Reading
pub struct BamReader<R: BufRead> { … }
impl<R: BufRead> BamReader<R> {
    pub fn from_path(path: &Path) -> Result<BamReader<…>, BismarkIoError>;
    pub fn header(&self) -> &noodles::sam::Header;
    pub fn records(&mut self) -> impl Iterator<Item = Result<BismarkRecord, BismarkIoError>>;
}

// Writing
pub struct BamWriter<W: Write> { … }
impl<W: Write> BamWriter<W> {
    pub fn new(writer: W, header: &noodles::sam::Header) -> Result<Self, BismarkIoError>;
    pub fn write(&mut self, rec: &BismarkRecord) -> Result<(), BismarkIoError>;
    pub fn finish(self) -> Result<(), BismarkIoError>;
}

// Records
impl BismarkRecord {
    pub fn strand(&self) -> BismarkStrand;
    pub fn xm(&self) -> &[u8];
    pub fn read_identity(&self) -> ReadIdentity;       // R1 / R2 / SE
    pub fn reference_span(&self) -> u32;
    pub fn inner(&self) -> &noodles::sam::Record;       // escape hatch
}
```

## Testing strategy

- **Unit tests** for `BismarkStrand::from_xr_xg`, CIGAR helpers, tag accessors. Pure functions, no I/O.
- **Property tests** (`proptest`): round-trip `BismarkRecord` through write-then-read on synthetic records. Strand-derivation idempotency across XR/XG permutations.
- **Integration tests**: read the existing `test_files/test_R1_bismark_bt2_pe.bam` (Perl Bismark fixture); assert per-record strand classification matches expected.
- **No samtools dependency in tests.** `bismark-io` is fully testable on pure Rust.

## Future extensions (out of scope for this PR)

- Alignment-side (`bismark` aligner) record types — different shape; defer to Phase 3.
- BGZF parallel decompression — for now, `noodles` defaults; revisit if extractor profiling shows decompression is the bottleneck.
- `bismark-core` / `bismark-report` / `bismark-cli` crate factoring — done when ≥2 binary crates would benefit from the same shared code.

## What this PR delivers

1. `rust/Cargo.toml` — workspace manifest with `bismark-io` as the first member.
2. `rust/bismark-io/Cargo.toml` — crate manifest, dependencies on `noodles-bam`, `noodles-sam`, `noodles-cram`, `noodles-fasta` (for the reference-reconstitution helper), `thiserror`. Edition 2024.
3. `rust/bismark-io/src/lib.rs` — empty stub with module declarations (no implementations).
4. `rust/bismark-io/DESIGN.md` — this file.
5. `rust/README.md` — top-level pointer explaining workspace layout, `_rs` suffix convention, and where to start.

No implementation. Implementation lives in the next sub-issue under [#794](https://github.com/FelixKrueger/Bismark/issues/794) (`impl(noodles-io): BAM/SAM reader`).

## Open to feedback

If you disagree with any of the four decisions above, or want a different crate split for Phase 1, this is the place to push back — every decision here propagates to ~10 binary crates, so resolving disagreements now is far cheaper than later.
