# `bismark-extractor` — SPEC

**Status:** scaffold / recon-complete. Flag inventory + structural-pitfall catalog locked from Perl source recon. Body fill-in + dual plan-review still pending.

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
- `--CX_context` cytosine-report (low-priority; Perl supports via the coverage2cytosine subprocess).

**Subprocess vs inline `--bedGraph`/`--cytosine_report`** — see §11 open question. Default plan: subprocess to `bismark2bedGraph` / `coverage2cytosine` (matching Perl's architecture) until those crates exist as Rust binaries, at which point we switch to inline.

## 3. CLI flag inventory

All 34 Perl flags catalogued from the recon pass. Citations are Perl line numbers in `bismark_methylation_extractor`.

| # | Flag | Aliases | Default | Behavior | Side effects / interactions | Perl ln |
|---|------|---------|---------|----------|----------------------------|---------|
| 1 | `--help` | `--man` | OFF | Print help and exit. | — | 959 |
| 2 | `--paired-end` | `-p` | auto | Force PE mode. | Mutex with `-s`; auto-detect via `@PG` if neither set. | 960 |
| 3 | `--single-end` | `-s` | auto | Force SE mode. | Mutex with `-p`. | 961 |
| 4 | `--fasta` | — | OFF | (Legacy; accepted but unused — variable never read in 6050 LOC). | Document as accepted-no-op; emit a one-line stderr deprecation warning. | 962 |
| 5 | `--ignore` | — | 0 | Trim N bp from 5' of R1 (or SE). | Requires CIGAR adjustment. | 963 |
| 6 | `--ignore_r2` | — | 0 | Trim N bp from 5' of R2. | PE-only. | 964 |
| 7 | `--ignore_3prime` | — | 0 | Trim N bp from 3' of R1 (or SE). | — | (epic §) |
| 8 | `--ignore_3prime_r2` | — | 0 | Trim N bp from 3' of R2. | PE-only. | (epic §) |
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
| 27 | `--samtools_path` | — | which samtools | Path to samtools (BAM read on Perl side). | **Accepted-no-op in Rust port** — bismark-io is pure-Rust noodles, no subprocess. | 982 |
| 28 | `--gzip` | — | OFF | Gzip-compress all split files. | Output filenames suffix `.gz`. | 983 |
| 29 | `--mbias_only` | — | OFF | Skip all output files; emit M-bias only. | Mutex with `--bedGraph`, `--cytosine_report`, `--mbias_off` (Perl dies with error). | 984 |
| 30 | `--mbias_off` | — | OFF | Skip M-bias computation. | Mutex with `--mbias_only`. | 985 |
| 31 | `--gazillion` | `--scaffolds` | OFF | Disable per-chr pre-split (filehandle limit workaround). | Used for genomes with thousands of contigs. Mutex with `--ample_memory`. | 986 |
| 32 | `--ample_memory` | — | OFF | Use in-memory sort instead of UNIX sort (faster, ~16GB RSS for hg38 chr1). | Mutex with `--gazillion`, `--buffer_size`. | 987 |
| 33 | `--parallel` | `--multicore` | 1 | Number of worker processes (Perl) / rayon threads (Rust). | Aliases share the same Perl variable. | 991 |
| 34 | `--yacht` | — | OFF | SE-only NOMe-Seq mode: emit single `any_C_context_*.txt` with read metadata. | Forces `--comprehensive` + `--merge_non_CpG`; SE-only; mutex with `--mbias_only`. | 992 |
| 35 | `--ucsc` | — | OFF | UCSC-compatible bedGraph (prefix `chr`, rename `MT`→`chrM`) + `chromosome_sizes.txt`. | `--bedGraph`-only. | 993 |

**Notes:**

- Perl `GetOptions` lists 26 entries; the additional 8 in this table come from auxiliary flags surfaced by reading the help text + the flag-dispatch logic (e.g. `--cutoff`, `--counts`, `--genome_folder` are passed through subprocesses).
- Perl's `--CX` is a real flag (alias for `--CX_context`, Perl line 979); the original epic #798 listed it under "mode flags" but it actually belongs to the cytosine-report subgroup.

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

### 6.5 CIGAR reversal is the reader's responsibility

`bismark-io`'s `BismarkRecord::cigar()` returns the noodles `Cigar` exactly as parsed from the BAM. The extractor MUST NOT reverse the CIGAR — Perl reverses it for output formatting (Perl lines 1619-1621, 2877-2886) but that's an output-side detail. The XM tag is already stored as-aligned by Bismark itself.

**Prevents:** the double-reversal class of bugs (read once on input, reversed on output, becomes wrong if a reviewer "fixes" the input by reversing it too).

### 6.6 Subprocess vs inline for `--bedGraph` / `--cytosine_report`

**v1.0 plan: subprocess** to Perl `bismark2bedGraph` / `coverage2cytosine`. Faithful to Perl's architecture; cheap to ship.

**v1.x evolution:** once the Rust `bismark-bedgraph` and `bismark-coverage2cytosine` crates land (epics #797 and a future one), switch the extractor's `--bedGraph` flag to call the Rust binaries inline (or via Rust library API).

This is an **explicit deferral**, not a permanent choice. See §11 open question.

## 7. Algorithm sketches

Detailed in the Phase A/B implementation plans; placeholders here.

### 7.1 SE main loop

```text
for record in reader.records():
    let strand = record.record_strand()           // bismark-io classifies eagerly
    let calls = extract_calls(record, ignore_5p, ignore_3p)
    for call in calls:
        route_to_output_file(call, strand)
        mbias_accumulate(call, read_identity = SE)
emit M-bias, splitting_report
```

### 7.2 PE main loop

```text
for pair in reader.pairs():                       // BismarkPair::from_mates enforces qname-eq + R1+R2
    let pair_strand = pair.pair_strand()
    let r1_calls = extract_calls(pair.r1(), ignore_5p_r1, ignore_3p_r1)
    let r2_calls = extract_calls(pair.r2(), ignore_5p_r2, ignore_3p_r2)
    if --no_overlap:
        r2_calls = r2_calls.filter(|c| c.ref_pos > r1_calls.last().ref_pos)
    for call in r1_calls:
        route_to_output_file(call, pair_strand)
        mbias_accumulate(call, ReadIdentity::R1)
    for call in r2_calls:
        route_to_output_file(call, pair_strand)
        mbias_accumulate(call, ReadIdentity::R2)
```

### 7.3 Paired-overlap detection

Mirrors Perl lines 2891-2906 + 2976-2990. Use `bismark-io::CigarExt::reference_end()` to compute R1's last reference position; drop R2 calls whose reference position ≤ R1's end (forward strand) or ≥ R1's start (reverse strand).

## 8. Test surface

Same byte-identity contract as `bismark-dedup`'s v1.0 gate.

### 8.1 Unit tests

- **XM routing**: every (XM byte × strand) → expected output file mapping.
- **CIGAR-aware ignore**: `--ignore N` correctly skips the first N read positions across `M`/`I`/`D`/`S` CIGAR ops.
- **Overlap detection**: synthetic pairs with known overlap → `--no_overlap` drops the right calls.
- **M-bias accumulation**: per (context × read-identity × position) increments match expected.
- **`--comprehensive` / `--merge_non_CpG` / `--yacht`** flag-matrix output-file counts.

### 8.2 Integration tests (10K CI fixtures)

- Synthetic small-genome BAMs with **measurable CHG and CHH signal** (not just CpG-rich). Closes Alan's missing-CHG/CHH bug at the CI level.
- Byte-identity vs Perl on each split file + `M-bias.txt` + `_splitting_report.txt`.

### 8.3 Real-data byte-identity gate (10M PE WGBS + 55M full sample)

`#[ignore]`'d in `tests/byte_identity_real_data.rs`. Compares Rust output to the existing Perl baseline at `~/Desktop/TrimG_Bismark_test/profiling/` (already on disk per session memory). Five separate assertions:

1. Each of the 12 split files (sorted-md5 equality).
2. `M-bias.txt` (byte equality, 6 sections).
3. `_splitting_report.txt` (byte equality).
4. Optional `--bedGraph` chain: `.bedGraph.gz` + `.bismark.cov.gz` (sorted-md5).
5. Optional `--cytosine_report` chain: `CpG_report.txt.gz` (sorted-md5).

## 9. Parallelism model — byte-identity invariant

`--multicore N` MUST produce output byte-identical to `--multicore 1` for any N ≥ 1.

**Mechanism** (per §6.4):

1. Main thread reads input BAM via `ThreadedBamReader` (BGZF decompression already threaded — leverages v1.1 work).
2. Records (or pairs) flow into a bounded MPMC channel.
3. Rayon worker pool consumes the channel, computes per-record `Vec<MethCall>` + per-record M-bias deltas.
4. Worker results flow into an output channel, **tagged with input-order index**.
5. A single output-collector thread reorders by index and writes split files + accumulates M-bias.

The output channel + reordering step is the byte-identity guard — output appears in input order regardless of which worker finished first.

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
| Open | `--genome_folder` Perl default is hardcoded mouse genome — keep, change, or reject? | Reject without explicit value when `--cytosine_report` is set (the Perl default is mouse-team-specific and would be misleading for general users). |
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
