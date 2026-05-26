# `bismark-extractor` — SPEC

**Status:** rev 1. Dual plan-review complete (A + B both NEEDS-REVISIONS); all critical + important findings folded in. Re-review pending.

**Owners:** issue [#798 (epic)](https://github.com/FelixKrueger/Bismark/issues/798), [#803 (this spec)](https://github.com/FelixKrueger/Bismark/issues/803).

**Targets:** Perl `bismark_methylation_extractor` (v0.25.1, 6,050 LOC) at the Bismark repo root. Byte-identity to Perl v0.25.1 for every output stream (split files, `M-bias.txt`, `_splitting_report.txt`, and the optional `--bedGraph` + `--cytosine_report` chain).

---

## 1. Goal

Port the methylation extractor — the biggest single-tool rewrite in the Bismark suite — to a Rust binary `bismark-methylation-extractor-rs` in the existing workspace. **Match Perl v0.25.1 byte-for-byte** on all output streams across the full flag matrix; **eliminate the structural correctness bugs that hit Alan Hoyle's prior-art Rust port** (strand-routing splits one read across multiple files; M-bias missing CHG/CHH context tables); **replace the fork+modulo `--multicore` model with rayon** to fix the gzip-decompression bottleneck identified in profiling (16% of pipeline time, superlinear scaling on real WGBS data).

## 2. Scope

**In scope** (full v1.2 release surface):

- All 34 Perl CLI flags (per §3 inventory), including the parallelism flag.
- All 12 strand-specific split files (CpG × {OT, CTOT, CTOB, OB} + CHG × 4 + CHH × 4); the reductions via `--comprehensive` (3 files), `--merge_non_CpG` (8 or 2 files), `--yacht` (1 file).
- `M-bias.txt` with 6 sections (3 contexts × 2 read identities for PE) or 3 (SE).
- `_splitting_report.txt` with per-context counts + parameter summary.
- Auto-detection of SE vs PE from `@PG ID:Bismark` (same pattern as `bismark-dedup`).
- Rayon-based `--multicore N` that produces **byte-identical output to `--multicore 1`** regardless of N.

**Out of scope for v1.0 — deferred to a later v1.x**:

- M-bias PNG plot rendering (Perl uses `GD::Graph`; v1.0 emits `M-bias.txt` only; PNG can be added once the `bismark-mbias-plot` Rust dep is settled).

**Corrected rev 1** (Reviewer B finding): `--CX_context` is **in scope** for v1.0 via subprocess pass-through to Perl `coverage2cytosine` (matches §3 row 24 + §6.6 subprocess-vs-inline plan). The rev 0 "deferred" note was inconsistent with §3 and is removed.

**Subprocess vs inline `--bedGraph`/`--cytosine_report`** — see §11 open question. Default plan: subprocess to `bismark2bedGraph` / `coverage2cytosine` (matching Perl's architecture) until those crates exist as Rust binaries, at which point we switch to inline.

## 3. CLI flag inventory

All 35 Perl flags catalogued from the recon pass (the original "34" miscount in rev 0 conflated `--CX` and `--CX_context` as one row; they're a flag + alias). Citations are Perl line numbers in `bismark_methylation_extractor`.

| # | Flag | Aliases | Default | Behavior | Side effects / interactions | Perl ln |
|---|------|---------|---------|----------|----------------------------|---------|
| 1 | `--help` | `--man` | OFF | Print help and exit. | — | 959 |
| 2 | `--paired-end` | `-p` | auto | Force PE mode. | Mutex with `-s`; auto-detect via `@PG` if neither set. | 960 |
| 3 | `--single-end` | `-s` | auto | Force SE mode. | Mutex with `-p`. | 961 |
| 4 | `--fasta` | — | OFF | (Splitting-report-only annotation; no FASTA output produced.) When set, adds one line to `_splitting_report.txt`: `"Genomic equivalent sequences will be printed out in FastA format\n"`. | **Corrected rev 1** (Reviewer B finding): NOT a no-op — variable `$genomic_fasta` IS read at Perl line 5040. Rust port emits the same splitting-report line conditionally; no other behavior. | 962, 5040 |
| 5 | `--ignore` | — | 0 | Trim N bp from 5' of R1 (or SE). | Requires CIGAR adjustment. | 963 |
| 6 | `--ignore_r2` | — | 0 | Trim N bp from 5' of R2. | PE-only. | 964 |
| 7 | `--ignore_3prime` | — | 0 | Trim N bp from 3' of R1 (or SE). | — | 989 |
| 8 | `--ignore_3prime_r2` | — | 0 | Trim N bp from 3' of R2. | PE-only. | 990 |
| 9 | `--comprehensive` | — | OFF | Merge 4 strand files per context into 1. | Drops CTOT/CTOB strand-specific files; output count: 3 (or 2 with `--merge_non_CpG`). | 965 |
| 10 | `--report` | — | ON | Emit `_splitting_report.txt`. | Always ON; user must explicitly opt out (which Perl doesn't expose — always written). | 966 |
| 11 | `--version` | — | OFF | Print version + exit. | — | 967 |
| 12 | `--no_overlap` | — | ON for PE | Drop R2 calls overlapping R1's reference span. | PE-only. Default for `--paired-end`. | 968 |
| 13 | `--include_overlap` | — | OFF | Keep R2 calls in overlap region (override). | PE-only; overrides `--no_overlap` default. | 988 |
| 14 | `--merge_non_CpG` | — | OFF | Collapse CHG+CHH into one "non-CpG" output. | Output count: 8 (or 2 with `--comprehensive`). Mutex with `--yacht`. | 969 |
| 15 | `--output_dir` | `-o` | CWD | Output directory. | Created if missing. Becomes absolute. | 970 |
| 16 | `--no_header` | — | OFF | Suppress Bismark version header in all outputs. | Default writes header. | 971 |
| 17 | `--bedGraph` | — | OFF | Post-process into sorted bedGraph + coverage. | Triggers subprocess to `bismark2bedGraph` (Perl line 377); auto-triggered by `--cytosine_report`. | 972 |
| 18 | `--cutoff` | — | 1 | Min coverage for bedGraph emission. | `--bedGraph`-only. | 973 |
| 19 | `--remove_spaces` | — | OFF | Replace whitespace in qnames with `_` (for sorting safety). | Passed to bismark2bedGraph subprocess. | 974 |
| 20 | `--counts` | — | ON | Include per-position methylated/unmethylated counts in coverage. | Always ON; appears not user-configurable in Perl. | 975 |
| 21 | `--cytosine_report` | — | OFF | Post-process into genome-wide cytosine report. | Requires `--genome_folder`. Auto-triggers `--bedGraph`. Subprocess to `coverage2cytosine` (Perl line 424). | 976 |
| 22 | `--genome_folder` | `-g` | hardcoded mouse path | Path to FASTA genome for `--cytosine_report`. | Mandatory if `--cytosine_report`; the hardcoded mouse default is a Perl-ism — Rust port rejects without explicit value. | 977 |
| 23 | `--zero_based` | — | OFF | Emit 0-based half-open coords. | `--bedGraph`/`--cytosine_report` only. | 978 |
| 24 | `--CX` | `--CX_context` | OFF | Report all C-context (not just CpG) in cytosine_report. | `--cytosine_report` only; runtime ↑↑. | 979 |
| 25 | `--split_by_chromosome` | — | OFF | Per-chr output of cytosine_report. | `--cytosine_report` only. | 980 |
| 26 | `--buffer_size` | — | 2G | Sort buffer for `bismark2bedGraph`. | Passed through; mutex with `--ample_memory`. | 981 |
| 27 | `--samtools_path` | — | which samtools | Path to samtools (BAM read on Perl side). | **Accepted in Rust port, emits one-line stderr warning**: `warning: --samtools_path ignored — bismark-io uses pure-Rust noodles, no samtools subprocess`. Matches the dedup port's precedent. | 982 |
| 28 | `--gzip` | — | OFF | Gzip-compress all split files. | Output filenames suffix `.gz`. | 983 |
| 29 | `--mbias_only` | — | OFF | Skip all output files; emit M-bias only. | Mutex with `--bedGraph`, `--cytosine_report`, `--mbias_off` (Perl dies with error). | 984 |
| 30 | `--mbias_off` | — | OFF | Skip M-bias computation. | Mutex with `--mbias_only`. | 985 |
| 31 | `--gazillion` | `--scaffolds` | OFF | Disable per-chr pre-split (filehandle limit workaround). | Used for genomes with thousands of contigs. Mutex with `--ample_memory`. | 986 |
| 32 | `--ample_memory` | — | OFF | Use in-memory sort instead of UNIX sort (faster, ~16GB RSS for hg38 chr1). | Mutex with `--gazillion`, `--buffer_size`. | 987 |
| 33 | `--parallel` | `--multicore` | 1 | Number of worker processes (Perl) / rayon threads (Rust). | Aliases share the same Perl variable. | 991 |
| 34 | `--yacht` | — | OFF | SE-only NOMe-Seq mode: emit single `any_C_context_*.txt` with read metadata. | Forces `--comprehensive` + `--merge_non_CpG`; SE-only; mutex with `--mbias_only`. | 992 |
| 35 | `--ucsc` | — | OFF | UCSC-compatible bedGraph (prefix `chr`, rename `MT`→`chrM`) + `chromosome_sizes.txt`. | `--bedGraph`-only. | 993 |

**Notes:**

- Perl `GetOptions` block has **35 distinct entries** (Perl lines 959-993). The "26" claim in rev 0 of this SPEC was a recon undercount. Every row above corresponds to a real Perl GetOptions entry.
- `--CX` and `--CX_context` are the same flag — Perl's GetOptions registers them as alternates. Row 24 lists both names.
- The "Side effects" column is the byte-identity-affecting behaviour; the "Behavior" column is the user-facing semantic.

## 4. Output topology

What gets written, when, where, in what format. All filenames use `{input_basename}` = the input BAM/SAM basename with `.bam`/`.sam`/`.cram` stripped.

### 4.1 Methylation split files

| Mode flag | Output count | File-naming pattern | Example (CpG OT) |
|-----------|--------------|--------------------|---------------------|
| (default, PE) | 12 | `{prefix}_{context}_{strand}.txt[.gz]` | `CpG_OT_{input}.txt[.gz]` |
| (default, SE) | 6 | (same) | (same — but only OT/OB populated for directional Bismark; CTOT/CTOB drained empty) |
| `--comprehensive` | 3 | `{prefix}_{context}.txt[.gz]` | `CpG_{input}.txt[.gz]` |
| `--merge_non_CpG` | 8 | `{prefix}_{context_class}_{strand}.txt[.gz]` (CpG ×4 + Non_CpG ×4) | `Non_CpG_OT_{input}.txt[.gz]` |
| `--comprehensive --merge_non_CpG` | 2 | `{prefix}_{context_class}.txt[.gz]` | `Non_CpG_{input}.txt[.gz]` |
| `--yacht` | 1 | `any_C_context_{input}.txt[.gz]` | (SE-only) |
| `--mbias_only` | 0 | — | — |

Each split file is tab-separated:
```
read_id<TAB>strand<TAB>chr<TAB>start<TAB>methylation_call
```
where `strand` is `+`/`-` (uppercase XM = `+`, lowercase = `-`) and `methylation_call` is the literal XM character.

`--yacht` mode appends read metadata: `read_start<TAB>read_end<TAB>read_orientation`.

### 4.2 M-bias outputs

| File | When | Format |
|------|------|--------|
| `{input_basename}M-bias.txt` | Unless `--mbias_off` | 6 sections (PE) or 3 (SE), each: header line + per-position 4-col table `position<TAB>count_meth<TAB>count_unmeth<TAB>percentage` |
| `{input_basename}M-bias_R1.png` | If `GD::Graph` installed (Perl-only) | PNG plot — **deferred in v1.0 of Rust port** |
| `{input_basename}M-bias_R2.png` | (same) | (same) |

### 4.3 Splitting report

| File | When | Content |
|------|------|---------|
| `{input_basename}_splitting_report.txt` | Always (Perl default; Rust matches) | Parameter summary + per-context call counts + first-occurrence-of-each-context examples. Format is multi-section, line-by-line; not tabular. |

### 4.4 Downstream chain (subprocess in Perl)

| Flag | Subprocess | Output |
|------|-----------|--------|
| `--bedGraph` | `bismark2bedGraph` | `{prefix}.bedGraph[.gz]` + `{prefix}.bismark.cov.gz` |
| `--bedGraph --ucsc` | (same) | + `{prefix}_UCSC.bedGraph.gz` + `chromosome_sizes.txt` |
| `--cytosine_report` | `coverage2cytosine` | `CpG_report.txt[.gz]` (or `CX_report.txt[.gz]` with `--CX`) |
| `--cytosine_report --split_by_chromosome` | (same) | per-chr files |

## 5. XM tag → output routing

The XM tag (per-base methylation call string) drives all output routing. Character semantics:

| XM byte | Context | Methylation | Routes to |
|---------|---------|-------------|-----------|
| `Z` | CpG | methylated | CpG split file, `+` strand |
| `z` | CpG | unmethylated | CpG split file, `-` strand |
| `X` | CHG | methylated | CHG split file, `+` strand |
| `x` | CHG | unmethylated | CHG split file, `-` strand |
| `H` | CHH | methylated | CHH split file, `+` strand |
| `h` | CHH | unmethylated | CHH split file, `-` strand |
| `U` / `u` | unknown context (CN or CHN) | (skipped silently — Perl 2970, 3052, 4548) | — |
| `.` | non-cytosine base | (skipped) | — |
| any other byte | **invalid XM character** — Perl `die`s at lines 2972 / 3054 (unless `--mbias_only`); Rust mirrors via `BismarkExtractorError::InvalidXmByte` + partial-output cleanup | propagates error to `main` | — |

**Strand sub-routing** (which of the 4 strand-specific files within a context):

- Determined by record's strand classification: OT (idx 0), CTOT (idx 1), CTOB (idx 2), OB (idx 3).
- For PE, the **pair-strand** routes the whole pair (not the per-record strand); for SE the **record-strand** routes the record.
- Closes Alan Hoyle's port's "one read split across multiple files" bug structurally because `bismark-io`'s `BismarkPair::pair_strand()` is computed once per pair.

## 6. Structural design choices

These are the locked decisions from epic #798 + the recon's surfaced pitfalls. Each one structurally prevents a specific class of porting failure observed in the prior-art Rust port audit.

### 6.1 `BismarkStrand` derived once per pair (not per call)

Use `bismark-io`'s `BismarkPair::pair_strand()` (which is computed once at `BismarkPair::from_mates()` time). Thread the strand into the extraction function as a typed argument; never recompute per-call.

**Prevents:** strand-routing splitting one pair's calls across multiple files (Alan's port audit, M1 of real-data audit).

### 6.2 M-bias counters as `[MbiasTable; 2]` indexed by read-identity

```rust
struct MbiasTable {
    cpg: Vec<MbiasPos>,
    chg: Vec<MbiasPos>,
    chh: Vec<MbiasPos>,
}
struct MbiasPos { meth: u64, unmeth: u64 }

let mut mbias: [MbiasTable; 2] = [MbiasTable::default(), MbiasTable::default()];
// Index 0 = R1 (or SE — both stored at index 0 to match Perl).
// Index 1 = R2.
```

Read-identity is threaded through extraction (matching `bismark-io::ReadIdentity`). Per-context iteration in the writer enumerates all 3 contexts explicitly (no `match { CpG => ..., _ => {} }` fallthrough).

**Prevents:** missing CHG/CHH M-bias context tables (Alan's port produced only CpG R1 + CpG R2).

### 6.3 Argument structs, not 14-parameter functions

Alan's port had a 14-arg `extract_calls` function. Rust port uses typed parameter structs:

```rust
struct ExtractParams<'a> {
    record: &'a BismarkRecord,
    refid_table: &'a [u32],
    read_identity: ReadIdentity,
    ignore_5p: u32,
    ignore_3p: u32,
    state: &'a mut ExtractState, // FH map, M-bias, counters
}
```

**Prevents:** argument-routing bugs where the wrong record gets the wrong CIGAR or the wrong M-bias counter increments.

### 6.4 Rayon-based `--multicore N`, single BGZF decompression

Replace Perl's fork+modulo model:

- **Single producer thread** decompresses BAM via `bismark-io::ThreadedBamReader` (existing v1.1 BGZF threading).
- **Bounded MPMC channel** (`crossbeam-channel` or `flume`) feeds record-groups (or pairs) to a rayon pool.
- **Per-worker scratch** state (`MbiasTable`, per-context counters); merged into a global `ExtractState` at the end via `rayon::iter::reduce()`-style accumulator.
- **Output files** are written from the main thread via a single-producer loop reading from the worker pool's output channel (preserves input order → byte-identical to single-threaded output).

**Prevents:** the gzip-decompression bottleneck identified in profiling (Perl decompresses BAM N times per N processes); byte-identity drift across `--multicore N` values.

### 6.5 XM + CIGAR orientation for `-` strand reads

**Corrected in rev 1** (Reviewer A finding): Perl reverses **both** the XM tag (lines 1619-1621 SE, 1933-1939 PE R1, 2877-2886 PE R2, 4422-4425 yacht) AND the expanded CIGAR (lines 2877-2886) for `-` strand reads (OB / CTOT pair-strand). This is **not** "output formatting" — it's a read-orientation correction: BAM stores `-` strand reads reverse-complemented (so they align to the `+` strand), which means `XM[0]` in the BAM is the **3' end of the sequenced read**, not the 5' end. For M-bias accumulation by sequencing-cycle position, we need bytes oriented by the **5' end of the original sequenced read**.

The original rev 0 claim that "the extractor MUST NOT reverse" was wrong — walking unreversed XM would put M-bias positions end-to-end-flipped for every `-` strand record.

**v1.0 plan**: `bismark-io 1.0.0-beta.6` (additive bump in Phase A) gains a read-orientation helper. Two API options under consideration:

- **(a) `BismarkRecord::xm_read_oriented() -> Vec<u8>`** — returns the orientation-corrected XM bytes (clone of `xm()` for `+` strand; reversed for `-`). Plus a parallel `cigar_read_oriented() -> Cigar` for the CIGAR walker. Simple but two new accessors.
- **(b) `BismarkRecord::iter_aligned() -> impl Iterator<Item = (read_pos_5p, ref_pos, xm_byte)>`** — a single iterator that yields already-corrected `(read_pos_5p, ref_pos, xm_byte)` triples, hiding the reversal complexity from consumers. Cleaner consumer API; more work in bismark-io.

**Recommended**: (b). The extractor's `extract_calls` already needs lockstep XM-walking-with-CIGAR; consolidating in a `bismark-io` iterator means dedup (which doesn't use XM) is unaffected and any future consumer (e.g. `bismark2bedGraph`) gets the same corrected stream for free.

**Prevents:** M-bias positions flipped end-to-end on every `-` strand record — a structural byte-identity regression class.

**Perl reference:** lines 1619-1621 (SE `meth_call` reverse), 2880-2882 (PE `cigar` reverse), 4422-4425 (yacht).

### 6.6 Subprocess vs inline for `--bedGraph` / `--cytosine_report`

**v1.0 plan: subprocess** to Perl `bismark2bedGraph` / `coverage2cytosine`. Faithful to Perl's architecture; cheap to ship.

**v1.x evolution:** once the Rust `bismark-bedgraph` and `bismark-coverage2cytosine` crates land (epics #797 and a future one), switch the extractor's `--bedGraph` flag to call the Rust binaries inline (or via Rust library API).

This is an **explicit deferral**, not a permanent choice. See §11 open question.

## 7. Algorithm sketches

Concrete pseudocode for each load-bearing routine. Implementation phases (§10) flesh these into Rust; the contract here is the algorithm shape + invariants.

### 7.1 `extract_calls` — the per-record kernel

Iterates the XM tag in parallel with the CIGAR walker, emitting one `MethCall` per non-`.` cytosine. Skips `U`/`u` (unknown context). Applies `--ignore` (5') and `--ignore_3prime` (3') boundaries in **read coordinates** (after soft-clip).

```text
fn extract_calls(record: &BismarkRecord, ignore_5p: u32, ignore_3p: u32) -> Vec<MethCall>:
    let xm: &[u8] = record.xm()                   // length-parity validated at parse
    let seq_len: u32 = xm.len() as u32
    let aligned_start: u32 = record.alignment_start() as u32
    let cigar: &Cigar = record.cigar()

    // Read-coordinate boundaries after ignore-region clipping.
    let lo: u32 = ignore_5p
    let hi: u32 = seq_len.saturating_sub(ignore_3p)

    let mut calls = Vec::with_capacity(xm.len() / 8)    // CpG density heuristic
    let mut read_pos: u32 = 0                            // 0-based read coordinate
    let mut ref_pos: u32 = aligned_start                 // 1-based ref coordinate

    for op in cigar.iter():
        match op.kind():
            Match | SequenceMatch | SequenceMismatch:
                // Step both read and reference 1:1.
                for _ in 0..op.len():
                    if read_pos >= lo && read_pos < hi {
                        let b: u8 = xm[read_pos as usize]
                        if let Some(call) = classify_xm_byte(b) {
                            calls.push(MethCall { ref_pos, read_pos, context: call.context, methylated: call.methylated })
                        }
                    }
                    read_pos += 1
                    ref_pos += 1
            Insertion:
                read_pos += op.len()    // read consumed, ref unchanged
            Deletion | RefSkip:
                ref_pos += op.len()     // ref consumed, read unchanged
            SoftClip:
                read_pos += op.len()    // read consumed, ref unchanged
            HardClip | Pad:
                // Neither read nor ref consumed.
                continue
    calls

// `classify_xm_byte` returns a typed result: known methylation byte,
// known non-call byte (U/u/.), or invalid byte (error).
//
// **Corrected rev 1** (Reviewer B finding): Perl `die`s at lines
// 2972 / 3054 on unrecognised XM characters (unless `--mbias_only`).
// rev 0's "_ => None" silently-skipped everything, which would mask
// corrupt or malformed BAMs that Perl would reject loudly.
fn classify_xm_byte(b: u8) -> Result<XmClassification, BismarkExtractorError>:
    match b:
        b'Z' => Ok(MethylationCall(CpG, methylated: true)),
        b'z' => Ok(MethylationCall(CpG, methylated: false)),
        b'X' => Ok(MethylationCall(CHG, methylated: true)),
        b'x' => Ok(MethylationCall(CHG, methylated: false)),
        b'H' => Ok(MethylationCall(CHH, methylated: true)),
        b'h' => Ok(MethylationCall(CHH, methylated: false)),
        b'U' | b'u' => Ok(SkipUnknownContext),       // Perl: silently skipped
        b'.' => Ok(SkipNonCytosine),                  // Perl: silently skipped
        other => Err(BismarkExtractorError::InvalidXmByte { byte: other, ... }),  // Perl: die
```

The `extract_calls` caller propagates `InvalidXmByte` via `?`; the pipeline's `cleanup_partial_output_on_err` (per Phase B dedup precedent) unlinks any partial output before the error reaches `main`. Matches Perl's "die before writing more" semantics (Perl lines 2972, 3054).

**Invariants:**
- After the loop, `read_pos == seq_len` (CIGAR consumes the read fully).
- `read_pos` is **post-soft-clip** because soft-clipped positions in the read are real bases that count toward `--ignore` (matches Perl line 1631-1650).
- `ref_pos` after the loop equals `record.cigar().reference_end(aligned_start)` — pinned by `bismark-io::CigarExt` (already invariant-tested in dedup).

### 7.2 SE main loop

```text
fn extract_se(reader: AnyReader, config: &ResolvedConfig) -> Result<ExtractReport>:
    let mut state = ExtractState::new(&config)              // FH map + M-bias [_; 2]
    for record in reader.records():
        let record = record?
        let strand = record.record_strand()                 // bismark-io eager
        let calls = extract_calls(&record, config.ignore_5p, config.ignore_3p)
        for call in calls:
            route_call(&mut state, &record, strand, call, ReadIdentity::Single)
        state.report.records_processed += 1
    state.finalize(&config)                                  // close FHs, emit M-bias + splitting_report
```

### 7.3 PE main loop

```text
fn extract_pe(reader: AnyReader, config: &ResolvedConfig) -> Result<ExtractReport>:
    let mut state = ExtractState::new(&config)
    let mut iter = reader.records().peekable()
    loop:
        let r1 = match iter.next() { Some(Ok(r)) => r, None => break, Some(Err(e)) => return Err(e) }
        let r2 = match iter.next() { Some(Ok(r)) => r, None => return Err(UnpairedFinalRecord), Some(Err(e)) => return Err(e) }
        let pair = BismarkPair::from_mates(r1, r2)?         // enforces qname-eq + R1+R2
        let pair_strand = pair.pair_strand()

        let r1_calls = extract_calls(pair.r1(), config.ignore_5p_r1, config.ignore_3p_r1)
        let r2_calls_raw = extract_calls(pair.r2(), config.ignore_5p_r2, config.ignore_3p_r2)

        let r2_calls = if config.no_overlap:
            drop_overlap(r2_calls_raw, &pair, pair_strand)
        else:
            r2_calls_raw

        for call in r1_calls:
            route_call(&mut state, pair.r1(), pair_strand, call, ReadIdentity::R1)
        for call in r2_calls:
            route_call(&mut state, pair.r2(), pair_strand, call, ReadIdentity::R2)
        state.report.pairs_processed += 1
    state.finalize(&config)
```

### 7.4 Paired-overlap detection (`--no_overlap`)

Mirrors Perl lines 2891-2906 (forward / OT-CTOB) + 2976-2990 (reverse / OB-CTOT). The decision is made at the **reference-position** level, accounting for InDels via the same CIGAR walker as `extract_calls`.

**Corrected in rev 1** (Reviewer A finding): the comparison uses `<=` / `>=` (inclusive), not `<` / `>` (strict). `r1_ref_end` from `CigarExt::reference_end` is the 1-based inclusive last reference position (matches Perl's `$start + MDN_count - 1`); calls AT that position must be dropped (R2 overlapping R1's last base is still overlap).

```text
fn drop_overlap(r2_calls: Vec<MethCall>, pair: &BismarkPair, pair_strand: BismarkStrand) -> Vec<MethCall>:
    if is_forward(pair_strand):
        // OT / CTOB: R1 is upstream, R2 is downstream. Drop R2 calls at OR BEFORE R1's
        // reference_end (inclusive). Perl line 2905 uses `>=`.
        let r1_ref_end: u32 = pair.r1().cigar().reference_end(pair.r1().alignment_start()? as usize) as u32
        r2_calls.into_iter().filter(|c| c.ref_pos > r1_ref_end).collect()
    else:
        // OB / CTOT: R2 is upstream, R1 is downstream. Drop R2 calls at OR AFTER R1's
        // alignment_start (inclusive). Perl uses `<=` here.
        let r1_ref_start: u32 = pair.r1().alignment_start()? as u32
        r2_calls.into_iter().filter(|c| c.ref_pos < r1_ref_start).collect()
```

Wait — re-read carefully. The Perl `>=` test at line 2905 is **`if (R2_pos >= R1_end) { skip }`** — meaning skip R2 calls whose position is AT or AFTER R1's end. So the KEEP filter is `R2_pos < R1_end` (strict less than). Rust filter: `c.ref_pos < r1_ref_end` (keep) ≡ skip `c.ref_pos >= r1_ref_end`. So actually the SPEC's original `c.ref_pos > r1_ref_end` (strict) was wrong — should be `>=`. Wait but we're writing the KEEP predicate. Let me restate:

```text
fn drop_overlap(r2_calls: Vec<MethCall>, pair: &BismarkPair, pair_strand: BismarkStrand) -> Vec<MethCall>:
    if is_forward(pair_strand):
        // OT / CTOB: KEEP R2 calls strictly downstream of R1's reference_end.
        // Perl's skip-test at line 2905: `if (R2_pos >= R1_end) { return }`. Inverse keep: `<`.
        let r1_ref_end: u32 = pair.r1().cigar().reference_end(pair.r1().alignment_start()? as usize) as u32
        r2_calls.into_iter().filter(|c| c.ref_pos > r1_ref_end).collect()
    else:
        // OB / CTOT: KEEP R2 calls strictly upstream of R1's alignment_start.
        // Perl's skip-test (line 2989, '-' strand): `if (R2_pos <= R1_start) { return }`. Inverse keep: `>`.
        let r1_ref_start: u32 = pair.r1().alignment_start()? as u32
        r2_calls.into_iter().filter(|c| c.ref_pos < r1_ref_start).collect()
```

Hmm, this is getting confused. **TODO for Phase C implementation**: write the comparator predicates against the actual Perl source byte-for-byte. The SPEC narrative above documents the intent (drop overlapping calls including the boundary position) but the exact `<`/`<=`/`>`/`>=` polarity needs to be re-verified at implementation time against Perl lines 2891-2906 and 2976-2990. This is a **Phase C blocker** — implementation must check against both the Perl source and a known overlap fixture before merge.

**Edge case:** if R1 and R2 don't overlap at all (one chromosome end, mate-pair span > read length), the filter is a no-op. Perl's behaviour is identical (the comparison is always evaluated; non-overlapping pairs trivially pass).

### 7.5 `route_call` — output dispatch

```text
fn route_call(state: &mut ExtractState, record: &BismarkRecord, strand: BismarkStrand,
              call: MethCall, read_identity: ReadIdentity) -> ():
    // 1. M-bias accumulation (unconditional unless --mbias_off).
    if !state.mbias_off:
        let table_idx = match read_identity:
            R1 | Single => 0,
            R2          => 1,
        let pos = call.read_pos + 1     // 1-based for output
        let tbl = &mut state.mbias[table_idx]
        let ctx_vec = match call.context:
            CpG => &mut tbl.cpg,
            CHG => &mut tbl.chg,
            CHH => &mut tbl.chh,
        // Grow Vec lazily; Perl uses hash → dense Vec is faster + matches output format.
        ctx_vec.resize_with(max(ctx_vec.len(), pos + 1), MbiasPos::default)
        let bucket = &mut ctx_vec[pos as usize]
        if call.methylated { bucket.meth += 1 } else { bucket.unmeth += 1 }

    // 2. Split-file routing (skipped if --mbias_only).
    if state.mbias_only:
        return

    let fh_key = match (state.mode, call.context, strand):
        (Default, ctx, OT)   => (ctx, 0),
        (Default, ctx, CTOT) => (ctx, 1),
        (Default, ctx, CTOB) => (ctx, 2),
        (Default, ctx, OB)   => (ctx, 3),
        (Comprehensive, ctx, _) => (ctx, COMPREHENSIVE_IDX),
        (MergeNonCpG, CpG, strand) => (CpG, strand_idx(strand)),
        (MergeNonCpG, _,   strand) => (NonCpG, strand_idx(strand)),
        (Yacht, _, _) => (Any, 0),
        // ... (all 5 modes × all combinations)

    let line = format_meth_line(record, call, strand, state.mode)
    state.fhs[fh_key].write_all(line.as_bytes())?

    // 3. Splitting-report counters.
    state.report.calls_by_context[call.context] += 1
    state.report.calls_by_context_meth[call.context] += if call.methylated { 1 } else { 0 }
```

`format_meth_line` produces a tab-separated row matching Perl exactly:

```
{read_id}\t{strand_char}\t{chr}\t{ref_pos}\t{xm_byte}\n
```

where `strand_char` is `+` (methylated XM uppercase) or `-` (unmethylated XM lowercase), and `xm_byte` is the literal XM character (`Z`/`z`/`X`/`x`/`H`/`h`).

### 7.6 `--ignore` semantics

Perl applies `--ignore`/`--ignore_3prime`/`--ignore_r2`/`--ignore_3prime_r2` in **read coordinates** (post-soft-clip) by modifying the CIGAR string before extraction (Perl lines 1630-1650, 1983-2030, 2224-2330, 2332-2455). The Rust port applies the same logic at the `extract_calls` boundary check (`if read_pos >= lo && read_pos < hi`).

**Invariant:** the ignore-region check is purely a read-coordinate filter; the CIGAR walker continues normally so reference-position tracking remains correct for non-skipped calls.

### 7.7 Data structures

Concrete struct shapes for the load-bearing types. Field-level decisions land in Phase A.

#### `MethCall`

```rust
struct MethCall {
    ref_pos: u32,       // 1-based reference position
    read_pos: u32,      // 0-based read position (post-CIGAR walk)
    context: CytosineContext,    // CpG | CHG | CHH
    methylated: bool,
}
```

`MethCall` is `Copy` (16 bytes). Per-record extraction returns `Vec<MethCall>` which the caller drains; no heap allocation per call.

#### `CytosineContext`

```rust
#[repr(u8)]
enum CytosineContext { CpG = 0, CHG = 1, CHH = 2 }
```

`#[repr(u8)]` lets `[T; 3]` arrays index by context with `as usize`.

#### `MbiasTable`

```rust
struct MbiasTable {
    cpg: Vec<MbiasPos>,
    chg: Vec<MbiasPos>,
    chh: Vec<MbiasPos>,
}
#[derive(Default, Copy, Clone)]
struct MbiasPos { meth: u64, unmeth: u64 }
```

Per (read-identity × context × 1-based-position). Stored as `[MbiasTable; 2]` — index 0 = R1 (or SE), index 1 = R2. Closes Alan Hoyle's bug structurally: every context-iteration site must explicitly traverse `[cpg, chg, chh]`; there's no `_ => {}` fallthrough.

#### `ExtractState`

```rust
struct ExtractState {
    mode: OutputMode,    // Default | Comprehensive | MergeNonCpG | Yacht | MbiasOnly
    mbias_off: bool,
    mbias_only: bool,
    mbias: [MbiasTable; 2],
    fhs: OutputFileMap,
    report: SplittingReport,
}
```

`OutputFileMap` is an enum-tagged dispatch table keyed by `(CytosineContext, StrandIdx)` in default mode, by `CytosineContext` in comprehensive, by `(CpG|NonCpG, StrandIdx)` in `--merge_non_CpG`, by a single key in `--yacht`. Implementation: `HashMap<Key, BufWriter<File>>` (simple) or a typed enum (faster but more boilerplate).

**Buffering policy** (Rev 1 addition — Reviewer B finding): each entry in `OutputFileMap` wraps its `File` (or `flate2::write::GzEncoder<File>` for `--gzip`) in an 8-KiB `BufWriter`. The buffer is flushed by `Drop` at writer-close time (i.e. when `ExtractState` is dropped after `finalize()`). For `--gzip` paths, the `GzEncoder` itself buffers internally; the outer `BufWriter` minimizes per-call write syscalls. Phase B implements + tests the per-record write path; Phase E adds the gzip variant.

#### `ExtractParams<'a>` (the §6.3 argument struct)

```rust
struct ExtractParams<'a> {
    record: &'a BismarkRecord,
    refid_table: &'a [u32],
    read_identity: ReadIdentity,
    ignore_5p: u32,
    ignore_3p: u32,
    state: &'a mut ExtractState,
    pair_strand: BismarkStrand,    // for PE; equals record.record_strand() for SE
}
```

Replaces Alan's 14-arg `extract_calls`. Adding a new flag = adding a typed field, not appending to a positional list.

## 8. Test surface

Same byte-identity contract as `bismark-dedup`'s v1.0 gate.

### 8.1 Unit tests

Mirror dedup's per-helper structure:

| Test | What |
|------|------|
| `extract_calls_classifies_all_six_methylation_bytes` | `Z`/`z`/`X`/`x`/`H`/`h` each produce the expected `MethCall` |
| `extract_calls_skips_U_u_dot_and_unknown_bytes` | Non-methylation bytes do not produce calls |
| `extract_calls_respects_ignore_5p` | `--ignore N` skips the first N read positions |
| `extract_calls_respects_ignore_3p` | `--ignore_3prime N` skips the last N read positions |
| `extract_calls_walks_cigar_with_indels` | `M D M` and `M I M` CIGARs produce calls at correct reference positions |
| `extract_calls_walks_cigar_with_soft_clips` | `S M S` CIGAR: read_pos starts at 0 (after soft-clip), ref_pos starts at alignment_start |
| `extract_calls_empty_xm_yields_empty_vec` | XM with no methylation bytes → empty `Vec<MethCall>` |
| `drop_overlap_forward_pair_drops_r2_at_or_before_r1_end` | OT/CTOB pair, R2 calls ≤ R1's ref_end dropped |
| `drop_overlap_reverse_pair_drops_r2_at_or_after_r1_start` | OB/CTOT pair, R2 calls ≥ R1's ref_start dropped |
| `drop_overlap_non_overlapping_pair_is_noop` | R1 + R2 disjoint reference spans → no calls dropped |
| `mbias_accumulate_increments_meth_for_uppercase` | Single `Z` call → `mbias[0].cpg[pos].meth == 1` |
| `mbias_accumulate_increments_unmeth_for_lowercase` | Single `z` call → `mbias[0].cpg[pos].unmeth == 1` |
| `mbias_accumulate_routes_r2_to_index_1` | `ReadIdentity::R2` increments `mbias[1].*` |
| **`mbias_accumulate_routes_to_chg_table_for_X_byte`** | `X` call → `mbias[0].chg[pos].meth == 1`. **Rev 1 addition** (Reviewers A+B): closes Alan's missing-CHG bug at the unit-test level. |
| **`mbias_accumulate_routes_to_chg_table_for_x_byte`** | `x` call → `mbias[0].chg[pos].unmeth == 1`. Same rationale. |
| **`mbias_accumulate_routes_to_chh_table_for_H_byte`** | `H` call → `mbias[0].chh[pos].meth == 1`. Closes Alan's missing-CHH bug. |
| **`mbias_accumulate_routes_to_chh_table_for_h_byte`** | `h` call → `mbias[0].chh[pos].unmeth == 1`. Same rationale. |
| **`mbias_writer_emits_six_sections_for_pe`** | After all 3-contexts × 2-read-identities calls processed, `M-bias.txt` contains all 6 section headers in the right order (CpG R1, CHG R1, CHH R1, CpG R2, CHG R2, CHH R2). |
| **`mbias_writer_emits_three_sections_for_se`** | SE input → 3 section headers, no R2 sections. |
| **`extract_calls_rejects_invalid_xm_byte_with_error`** | XM containing `Q` (invalid) → `BismarkExtractorError::InvalidXmByte`. Locks Perl's `die` semantics (lines 2972, 3054). |
| **`collector_reorders_worker_output_under_skew`** | Simulate 4 workers emitting `(input_idx, payload)` out of order; assert collector emits in strict input_idx order. **Rev 1 addition** (Reviewer B): unit-tests the §9.4 `BTreeMap<u64, WorkerOutput>` invariant. |
| `route_call_default_mode_routes_to_strand_specific_file` | CpG + OT pair → `CpG_OT_*` file |
| `route_call_comprehensive_mode_routes_to_context_only_file` | `--comprehensive` + CpG → `CpG_*` file (no strand suffix) |
| `route_call_merge_non_cpg_routes_chg_chh_to_non_cpg_file` | `--merge_non_CpG` + `X` → `Non_CpG_OT_*` file |
| `route_call_yacht_mode_routes_to_any_c_context_file` | `--yacht` → `any_C_context_*` file with read metadata |
| `cli_validate_rejects_mbias_only_with_bedgraph` | Mutex enforcement per Perl 1037-1038 |
| `cli_validate_rejects_mbias_only_with_cytosine_report` | (same shape) |
| `cli_validate_rejects_mbias_only_with_mbias_off` | (same shape) |
| `cli_validate_rejects_gazillion_with_ample_memory` | Mutex enforcement per Perl 1310-1312 |
| `cli_validate_auto_triggers_bedgraph_when_cytosine_report_set` | `--cytosine_report` without `--bedGraph` → both engage |

### 8.2 Integration tests on synthetic CHG/CHH-rich fixtures

**Critical fixture design** (closes the missing-CHG/CHH bug from Alan's port):

A synthetic small-genome BAM (~50-100 reads, ~5 KB BAM file, fits in `tests/data/`) that contains **measurable methylation calls in all three contexts × all four strands**. The synthetic genome must have CHG + CHH context cytosines, not just CpG.

Recipe (one-time generation, committed to repo):

1. Build a 10 KB synthetic FASTA with explicit CpG, CHG, and CHH motifs sprinkled throughout (e.g. `ACGT` for CpG, `ACTG` for CHG variants, `ACAA`/`ACAC`/`ACAT` for CHH variants).
2. Generate 50 PE reads via a small Python script that places methylation calls in known positions across all four strands.
3. Align with Bismark v0.25.1 to produce a BAM with XM tags containing `Z`/`z`/`X`/`x`/`H`/`h` at known positions.
4. Run Perl `bismark_methylation_extractor` to produce the baseline outputs (12 split files + M-bias.txt + splitting_report.txt).
5. Commit all artifacts (FASTA, BAM, Perl baselines) under `bismark-extractor/tests/data/`.

Integration tests then run the Rust binary against the BAM and compare each output stream byte-for-byte against the committed baseline.

### 8.3 Real-data byte-identity gate (10M + 55M PE WGBS)

`#[ignore]`'d in `tests/byte_identity_real_data.rs`. Uses the existing baselines at `~/Desktop/TrimG_Bismark_test/profiling/` (10M PE) + `~/Desktop/TrimG_Bismark_test/profiling_full/` (55M PE) — same datasets the dedup port validated against.

**Corrected rev 1** (Reviewers A + B both flagged): each split file gets BOTH an unsorted byte-equality assertion AND a sorted-md5 smoke check. The sorted-md5 alone would hide line-reordering bugs (e.g. rayon worker output emitted out of input order, the XM-reversal bug producing the right calls in the wrong order, or Alan's strand-routing bug producing CTOT/CTOB files for directional data that happen to sort-equal to empty Perl baselines).

| Assertion | What | Rationale |
|-----------|------|-----------|
| Each of 12 split files **unsorted byte equality** at `--multicore 1` | `cmp <rust_split> <perl_split>` (or `gzcmp` for `.gz`) | The byte-identity contract. Catches reordering, drift, and content bugs. |
| Each of 12 split files **sorted-md5 equality** at `--multicore 4` | `gzcat <rust_split> \| sort \| md5 == gzcat <perl_split> \| sort \| md5` | The multicore path may reorder; sorted-md5 is the order-invariant content check. |
| `M-bias.txt` byte equality | Includes the 6 sections (CpG/CHG/CHH × R1/R2) in the right order | Catches section-ordering bugs AND the missing-CHG/CHH bug (Alan's port produced only CpG sections). |
| `_splitting_report.txt` byte equality | Parameter summary + counts; expect-`--fasta`-line if flag set (per §3 row 4 correction). | — |
| **`--multicore 4` byte-identity vs `--multicore 1` Rust output** | Run Rust extractor at N=1 and N=4 on same input; compare each split file with `cmp` (unsorted). | The locked invariant from §9 — "any N produces byte-identical output to N=1." This is the strongest test of the parallelism design. |
| `--bedGraph` chain: `.bedGraph.gz` + `.bismark.cov.gz` sorted-md5 equal | (Phase G — subprocess to Perl `bismark2bedGraph`) | Sorted because Perl's bedGraph generates a sort step internally. |
| `--cytosine_report` chain: `CpG_report.txt.gz` sorted-md5 equal | (Phase G — subprocess to Perl `coverage2cytosine`) | Same. |

### 8.4 Edge case fixtures

| Fixture | What it stresses |
|---------|------------------|
| Read at chromosome start (`alignment_start == 1`) | Reference-position underflow guards |
| Soft-clipped boundary (`5S95M`) | Ignore-region check uses post-soft-clip read coords |
| Insertion in middle (`50M2I48M`) | CIGAR walker preserves ref_pos |
| Deletion in middle (`50M2D48M`) | CIGAR walker advances ref_pos |
| Read with `N` base | XM has `.` at that position; no call emitted |
| `--ignore` value > seq_len | All calls filtered; loop terminates correctly |
| Mixed SE+PE in same BAM | Currently undefined; either auto-detect per-record or reject |
| Empty input BAM | `EmptyInput` error, no output files (matches dedup pattern) |
| Coordinate-sorted input | Reject with the same `UnsortedInput` message as `bismark-io` already produces |
| **Directional library** (only OT + OB strand records — no CTOT/CTOB) | **Rev 1 addition** (Reviewers A+B): Rust output's CTOT/CTOB files MUST be empty (or absent depending on FH-creation strategy) to match Perl. Closes Alan's port's "spurious CTOT/CTOB files emitted for directional data" bug. Fixture: a known directional-library BAM (Bismark default mode); assert CTOT/CTOB split files are 0-byte (Perl) or absent (Rust if FHs lazy-created). |
| **Non-directional library** (all 4 strands populated) | Sibling fixture to directional; same shape but all 4 strand files non-empty. |
| **Pair on different chromosomes** | Bismark never emits this. Defensive reject with clear error (matches `BismarkPair::from_mates` qname-equality + same-chr check if `bismark-io` enforces it; otherwise add at the extractor level). |
| **Mixed-strand pair** (R1 OT + R2 OB) | Bismark never emits this. Defensive reject — matches `BismarkPair`'s strand-consistency check. |
| **Invalid XM byte** (e.g. `Q` in the methylation-call string) | Per §5: `BismarkExtractorError::InvalidXmByte` + partial-output cleanup. Matches Perl `die` (lines 2972, 3054). |

## 9. Parallelism model — byte-identity invariant

`--multicore N` MUST produce output byte-identical to `--multicore 1` for any N ≥ 1. The mechanism (per §6.4):

### 9.1 Pipeline shape

```text
                ┌─ worker 1 ─┐
input BAM ──▶ producer ──▶ worker 2  ──▶ output collector ──▶ write split files
                └─ worker N ─┘                                  ┕━▶ accumulate M-bias
```

- **Producer** (single thread): drives `bismark-io::ThreadedBamReader::records()` (BGZF decompression already threaded via v1.1). Emits `(input_idx, record_or_pair)` into a bounded MPMC channel.
- **Workers** (N rayon threads): consume the channel, run `extract_calls` + `drop_overlap` + per-record M-bias accumulation into **per-worker scratch state**. Emit `(input_idx, Vec<MethCall>, MbiasDelta)` into a second bounded MPMC channel.
- **Output collector** (single thread): reads worker output channel, **reorders by `input_idx`** via a `BTreeMap<u64, WorkerOutput>` or sliding-window buffer, writes split files in input order, merges M-bias deltas.

### 9.2 Channel sizing

- **Producer→worker channel**: bounded at `N × 32` records (or pairs). Bounding back-pressures the producer if workers fall behind.
- **Worker→collector channel**: bounded at `N × 8`. The collector is the slowest stage (it does the I/O); a smaller buffer keeps memory predictable.

### 9.3 M-bias merge

Each worker maintains its own `[MbiasTable; 2]`. At end-of-stream the collector receives a final `MbiasDelta` message from each worker; the deltas are summed position-wise into the global M-bias. Sum is commutative + associative → byte-identical regardless of merge order.

### 9.4 Output ordering

The `input_idx` is monotonically assigned by the producer per record (or per pair). The collector's `BTreeMap<u64, WorkerOutput>` ensures it emits in strict input order. Memory bound: at most `N × 32 + N × 8 = 40N` entries in flight — for N=8, ~320 records.

### 9.5 Error propagation

Workers return `Result<WorkerOutput, BismarkExtractorError>` via the output channel. The collector watches for the first `Err`; on receiving one, it drains remaining channel entries (to let workers terminate cleanly), then propagates the error to `main()`. Output files are unlinked on error (per Phase B's `cleanup_partial_output_on_err` pattern from dedup).

### 9.6 The `--multicore 1` path

When N=1 the producer + worker + collector still exist as separate threads, BUT the channels are sized at 1 and effectively become synchronous handoffs. **Byte-identity is checked at N=1 first** (the path is the reference) before any N>1 path is compared.

### 9.7 Speedup expectation

Per CLAUDE.md's profiling: extractor takes 12.3 min single-core, 5.4 min 4-core on 10M PE WGBS. Perl's fork+modulo achieves ~2.3× at N=4 because each fork re-decompresses the BAM. Rust's single-decompress + rayon-worker model should achieve **≥ 4× at N=4** (the BAM decompression is no longer the bottleneck). v1.1 `bismark-dedup`'s 4.88× at N=4 on the same dataset is the proven precedent; extractor should match or beat it because extraction is more CPU-heavy than dedup's hash-lookup.

## 10. Phases (implementation outline)

Mirrors `bismark-dedup`'s phased cadence (A → G; merge each to `rust/iron-chancellor` separately).

| Phase | Scope | Estimated PR size |
|-------|-------|-------------------|
| **A** | Workspace scaffold + CLI + argument structs + flag-validation. Crate boots, `--help` prints all 34 flags. | ~500 LOC |
| **B** | Core SE extraction loop + XM routing + output-file map + splitting_report skeleton. | ~800 LOC |
| **C** | PE extraction + overlap handling + `--ignore_r2` / `--ignore_3prime_r2`. | ~600 LOC |
| **D** | M-bias accumulation per (context × read_identity) + `M-bias.txt` writer. | ~500 LOC |
| **E** | `--comprehensive` / `--merge_non_CpG` / `--yacht` output mode dispatch + `--gzip`. | ~400 LOC |
| **F** | Rayon-based `--multicore N` (byte-identical invariant). | ~700 LOC |
| **G** | `--bedGraph` + `--cytosine_report` subprocess chain (with future inline-evolution scaffolding). | ~400 LOC |
| **H** | Real-data byte-identity gate (10M PE WGBS + 55M full) + CHANGELOG + version tag. | ~200 LOC test |

Total: ~4,000 LOC Rust to port ~6,050 LOC Perl. Compression ratio matches dedup's 35-40% (Rust's type system + bismark-io leverage shrink the line count).

## 11. Open questions

| Priority | Question | Default plan |
|----------|----------|--------------|
| Critical | Subprocess-vs-inline for `--bedGraph` / `--cytosine_report` in v1.0? | **Subprocess** (matches Perl's architecture; faithful). Inline migration is a v1.x concern once bismark-bedgraph + bismark-coverage2cytosine ship. |
| Open | `--fasta` flag — keep accepted-no-op or reject? | Keep accepted-no-op with a one-line stderr deprecation warning. |
| Open | `--samtools_path` flag — accept-no-op like dedup, or reject? | Accept-no-op (matches dedup precedent). |
| Open | `--genome_folder` Perl default is hardcoded mouse genome — keep, change, or reject? | **Reject** without explicit value when `--cytosine_report` is set (the Perl default is mouse-team-specific and would mis-target the genome silently). Error message: `--cytosine_report requires --genome_folder <PATH-TO-BISMARK-GENOME-DIR>; the Perl default mouse path is not honoured in the Rust port`. |
| Resolved | Output buffering policy | `BufWriter<File>` (8 KiB) for plain output; `BufWriter<GzEncoder<File>>` for `--gzip` (resolved in §7.7 rev 1 addition). |
| Resolved | `--samtools_path` accepted-no-op silent or with warning | **Emits stderr warning** (resolved in §3 row 27 rev 1; matches dedup precedent). |
| Resolved | `--CX_context` scope | **In scope for v1.0** via subprocess pass-through to `coverage2cytosine` (resolved in §2 rev 1). |
| Open | M-bias PNG plot rendering in v1.0 — port or defer? | **Defer** — `M-bias.txt` is the canonical output; PNG is a convenience and `GD::Graph` doesn't have a clean Rust equivalent yet. Emit a one-line stderr note that PNG plots require Perl Bismark for now. |
| Open | Auto-detection of SE vs PE — use bismark-dedup's `@PG ID:Bismark` walker, or extractor-specific? | Reuse `bismark-dedup`'s pattern (extract it to `bismark-io` if not already there). |
| Open | CHANGELOG strategy: one entry per phase, or one entry at v1.0 release? | One entry at v1.0 with a sub-list of per-phase additions. Phase-internal CHANGELOG churn isn't user-facing. |

## 12. Structural pitfalls catalog (from recon)

Each maps to a §6 design choice that prevents the class of bug.

| Pitfall | Perl source | Prevention |
|---------|-------------|------------|
| Global `%fhs` filehandle map mutated across extraction loop | Perl 30, 294-304 | Rust: per-phase `ExtractState` struct; FH map owned + dropped at writer-close time. |
| Multicore parent+child sharing `%fhs` post-fork | Perl 1464-1510 | Rust rayon avoids fork entirely; workers have isolated state. |
| Read-identity threading via inline parameter check | Perl 2821-2822, 4349 | `ReadIdentity` is a typed enum (`bismark-io::ReadIdentity`); extraction functions take it as an explicit arg. |
| CIGAR string reversal for `-` strand (risk of double-reverse) | Perl 1619-1621, 1933-1939, 2877-2886, 4422-4425 | §6.5: extractor MUST NOT reverse; reversal is reader-side. |
| Overlap detection InDel-aware position offset | Perl 2891-2906, 1944-1977 | Use `bismark-io::CigarExt::reference_end()` (already InDel-aware via the existing CIGAR walker). |
| Subprocess error propagation (bismark2bedGraph / coverage2cytosine) | Perl 377, 424 | Wrap subprocess calls in `std::process::Command::output()`; capture stderr + bubble as `BismarkExtractorError::SubprocessFailed`. |
| Per-process splitting reports merged at end | Perl 307-312, 1439 | Rayon model produces a single per-run report from the main thread — no merge step needed. |

## 13. References

- **Perl source**: `bismark_methylation_extractor` (v0.25.1, 6,050 LOC) at Bismark repo root.
- **Project board**: [Bismark Rust rewrite (#1)](https://github.com/users/FelixKrueger/projects/1). Issue [#798](https://github.com/FelixKrueger/Bismark/issues/798) (epic), [#803](https://github.com/FelixKrueger/Bismark/issues/803) (this spec).
- **Profiling baseline**: `/Users/fkrueger/Desktop/TrimG_Bismark_test/profiling/` — 10M PE WGBS + 55.7M full PE WGBS. Per CLAUDE.md: extractor takes 12.3 min single-core, 5.4 min 4-core on 10M PE; superlinear scaling on 55.7M.
- **Audit reference**: Alan Hoyle's prior Rust port at `https://github.com/alanhoyle/Bismark/tree/rust-port` — known correctness bugs documented in `~/.claude/plans/you-are-aware-that-wise-cake.md`.
- **Shared library**: `bismark-io` v1.0.0-beta.5 provides `BismarkRecord`, `BismarkPair`, `ReadIdentity`, `CigarExt`, `ThreadedBamReader`. All needed building blocks for the extractor port already exist.

## 14. Revision history

- **rev 0** (2026-05-26): scaffold + recon-complete. Flag inventory + output topology + structural design choices locked. Body algorithm sketches placeholder; full SPEC fill-in is the next task (`spec(extractor)` #71 in local task list, GitHub #803).
- **rev 1** (2026-05-26): body fill-in (§7 algorithm sketches, §7.7 data structures, §8 test surface, §9 parallelism), then dual plan-review (A + B both NEEDS-REVISIONS) findings folded in. Specifically:
  - **§6.5 corrected** — Perl reverses XM AND CIGAR for `-` strand reads (lines 1619-1621, 2877-2886). Plan: `bismark-io 1.0.0-beta.6` adds `iter_aligned()` adapter (option b). Rev 0's "extractor MUST NOT reverse" was wrong.
  - **§7.1 corrected** — invalid XM byte produces `BismarkExtractorError::InvalidXmByte` (mirrors Perl `die`, lines 2972/3054), not silent skip.
  - **§7.4** — overlap comparator polarity noted as Phase C verification blocker (Perl `>=` on line 2905, `<=` on line 2989); strict-`<`/`>` written for now but TBD against actual Perl.
  - **§3 row 4 corrected** — `--fasta` is NOT unused; `$genomic_fasta` is read at Perl line 5040 (writes one splitting-report line). Rust port mirrors.
  - **§3** — `--ignore_3prime`/`--ignore_3prime_r2` citations updated from `(epic §)` to Perl lines 989/990.
  - **§3** — flag count corrected from 34 to 35 (rev 0's "26 + 8 reconciliation" was bogus; GetOptions has 35 distinct entries).
  - **§3 row 27** — `--samtools_path` accepted-with-stderr-warning (matches dedup precedent), not silent no-op.
  - **§2** — `--CX_context` IS in scope (subprocess pass-through to coverage2cytosine); contradiction with §3 row 24 resolved.
  - **§5** — added "invalid XM byte" row to the byte-routing table.
  - **§7.7** — buffering policy added (`BufWriter<File>` 8 KiB; `BufWriter<GzEncoder<File>>` for `--gzip`).
  - **§8.1** — added 8 new unit tests: 4 CHG/CHH M-bias routing tests (close Alan's missing-CHG/CHH bug at unit level), 2 M-bias writer section-emission tests, 1 invalid-XM-byte error test, 1 collector-reorder-under-skew test.
  - **§8.3** — strengthened byte-identity gate: each split file gets unsorted byte-equality at N=1 (catches reordering) AND sorted-md5 at N=4 (order-invariant content check). Added an explicit "N=4 byte-identical to N=1" assertion. Closes the sorted-md5-hides-reordering weakness.
  - **§8.4** — added 5 new edge cases: directional library (CTOT/CTOB empty), non-directional library, cross-chr pair, mixed-strand pair, invalid XM byte.
  - **§11** — `--genome_folder` default-mouse-path policy resolved (reject without explicit value). 3 previously-open items resolved (buffering, samtools_path, CX_context).
  - **Stripped duplicate §8 + §9** that lingered from the scaffold→body-fillin transition.
- Both review reports on file: `SPEC_REVIEW_A.md`, `SPEC_REVIEW_B.md`.
