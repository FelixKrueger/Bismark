# Bismark Rust Rewrite Candidates

## Summary

The Bismark Perl codebase contains several CPU-bound hot paths that would benefit significantly from Rust rewrites. The highest-impact targets are the methylation call extraction loop (per-base character classification with CIGAR-aware position tracking), hash-based duplicate detection, and genome-wide cytosine iteration. These three components dominate runtime for typical WGS bisulfite sequencing datasets.

Scripts focused on report generation (`bismark2report`, `bismark2summary`) and genome preparation (`bismark_genome_preparation`, `bin/bismark_genome_conversion.pl`) are NOT candidates -- they are I/O-bound, run once per experiment, or already replaced by native Nextflow/awk.

## Priority Matrix

| Component | Script | Lines | Impact | Complexity | Priority |
|-----------|--------|-------|--------|------------|----------|
| Methylation call extraction | `bismark_methylation_extractor` | 2813-4226 | High | Medium | P0 |
| CIGAR string parsing | `bismark_methylation_extractor` | 4228-4345 | High | Low | P0 |
| SAM/BAM line parsing + tag extraction | `bismark_methylation_extractor`, `deduplicate_bismark` | multiple | High | Low | P0 |
| Hash-based duplicate detection | `deduplicate_bismark` | 220-473, 574-832 | High | Low | P1 |
| Genome loading + cytosine iteration | `coverage2cytosine` | 168-700 | High | Medium | P1 |
| bedGraph aggregation + sorting | `bismark2bedGraph` | 316-506 | Medium | Low | P2 |
| Alignment engine (methylation calling) | `bismark` | 2289-6200 | Already replaced | N/A | Skip |
| HTML report generation | `bismark2report` | 1-1300 | Low | Low | Skip |
| Summary statistics | `bismark2summary` | 1-1722 | Low | Low | Skip |
| Genome preparation | `bismark_genome_preparation` | 1-848 | Already replaced | N/A | Skip |
| FASTA C->T/G->A conversion | `bin/bismark_genome_conversion.pl` | 1-167 | Being replaced (awk) | N/A | Skip |

## Detailed Analysis

### P0: Methylation Call Extraction (innermost hot loop)

- **Location:** `bismark_methylation_extractor` lines 2813-4226 (`print_individual_C_methylation_states_paired_end_files` and `print_individual_C_methylation_states_single_end`)
- **Current implementation:** For every read, the methylation call string (XM:Z: tag) is `split(//,...)` into individual characters. Each character is classified via an `if/elsif` chain (Z/z = CpG meth/unmeth, X/x = CHG, H/h = CHH, `.` = skip, U/u = unknown). For each non-dot character, the script: (1) checks/applies CIGAR offset, (2) computes genomic position, (3) increments M-bias counters in nested hash (`$mbias_1{context}->{position}->{meth/un}`), (4) prints a tab-joined line to the appropriate filehandle. For paired-end reads with `--no_overlap`, an early-return boundary check runs on every base.
- **Why Rust:** This is the single hottest loop in all of Bismark. Every base of every read passes through it. The character classification is a perfect match for a lookup table or SIMD byte-shuffle. The nested hash updates (`$mbias_1{CHG}->{$index+1}->{meth}++`) are extremely expensive in Perl (hash lookup per base per read) but trivial with a flat array in Rust. The `join("\t",...)` + print per cytosine is also slow; Rust could buffer writes.
- **Estimated speedup:** 10-50x. The core loop is entirely CPU-bound character classification + hash updates + I/O formatting.
- **Rust approach:**
  - Use `rust-htslib` or `noodles` for BAM reading (skip samtools pipe overhead entirely)
  - Byte-level lookup table (256-entry array) for methylation character classification -- single array index vs. 8-way `if/elsif`
  - Flat `Vec<u32>` arrays for M-bias counters indexed by `[context][position]` instead of nested hash-of-hash
  - `memchr` crate for fast scanning of methylation call strings (skip `.` characters in bulk)
  - Buffered `BufWriter` with pre-allocated format strings for output
  - Overlap detection as simple integer comparison (already is, but Perl overhead per-base is large)
- **Interface:** Standalone CLI: `bismark-meth-extract [--paired|--single] [--no_overlap] [--comprehensive] [--merge_non_CpG] --output_dir <dir> <input.bam>`. Reads BAM directly (no samtools pipe). Writes same output format as Perl version. Called from Nextflow process.

### P0: CIGAR String Parsing

- **Location:** `bismark_methylation_extractor` lines 4228-4345 (`check_cigar_string`), plus CIGAR expansion at lines 2849-2875, and duplicate CIGAR parsing in `deduplicate_bismark` lines 302-348, 360-454, 655-810
- **Current implementation:** CIGAR strings are parsed by splitting on `\D+` and `\d+` to extract lengths and operations. These are then expanded into a "composite CIGAR" array where each element is a single operation character (e.g., `76M2D4M` becomes 82 single-character entries). The `check_cigar_string` subroutine is called for EVERY base of EVERY read that has InDels, walking the expanded array with index arithmetic. The check for `$cigar =~ /^\d+M$/` (simple match, no indels) provides a fast path that the author notes "speeds up the extraction process by up to 60%".
- **Why Rust:** The CIGAR expansion creates a temporary Perl array of length equal to the read+deletions. For a 150bp read with deletions, this is 150+ array pushes per read. The `check_cigar_string` function does 5-way string comparison (`eq 'M'`, `eq 'I'`, etc.) per base. Rust could: (a) avoid expansion entirely by walking CIGAR operations directly with a state machine, (b) use byte comparison instead of string comparison, (c) inline the entire operation.
- **Estimated speedup:** 5-20x for reads with InDels. Reads without InDels already take the fast path.
- **Rust approach:**
  - Zero-allocation CIGAR walker: iterate operations directly from the CIGAR string without expanding to per-base array
  - `match` on u8 bytes (`b'M'`, `b'I'`, `b'D'`, `b'S'`, `b'N'`) -- branchless or jump-table compiled
  - Combine with methylation extraction into a single pass (no separate `check_cigar_string` call)
  - Use `rust-htslib` `CigarStringView` which already provides efficient CIGAR iteration
- **Interface:** Bundled into the methylation extraction tool above (not a separate binary).

### P0: SAM/BAM Line Parsing and Tag Extraction

- **Location:** `bismark_methylation_extractor` lines 1574-1595, 1867-1907; `deduplicate_bismark` lines 258-277, 603-629
- **Current implementation:** Each SAM line is `split(/\t/)` to extract fields, then a regex `while(/(XM|XR|XG):Z:([^\t]+)/g)` scans the entire line for Bismark-specific tags. The `$value =~ s/\r//` and `chomp` are applied to each extracted value. For paired-end files, this happens twice per read pair (once for each mate).
- **Why Rust:** Perl's `split` creates a new array of strings on every line. The regex scan for tags traverses the full line even though the tags are typically at the end. Rust could:
  - Read BAM directly (binary format, no text parsing at all)
  - For SAM: use `memchr` to find tab positions without allocating, extract only needed fields by index
  - Find XM/XR/XG tags with a single-pass scan using `memchr` for `:Z:` pattern
- **Estimated speedup:** 3-10x for SAM parsing alone. If reading BAM directly via `rust-htslib`, the entire samtools-pipe overhead is eliminated (fork, text conversion, pipe I/O), which adds another 2-5x.
- **Rust approach:**
  - `rust-htslib` for native BAM reading (preferred)
  - Fallback SAM parser using `memchr` for field splitting
  - Aux tag extraction via `rust-htslib` `Record::aux()` method (O(1) for known tags)
- **Interface:** Bundled into the methylation extraction and deduplication tools.

### P1: Hash-Based Duplicate Detection

- **Location:** `deduplicate_bismark` lines 220-473 (default mode), lines 574-832 (barcode/RRBS mode)
- **Current implementation:** For every read/pair, a composite key is constructed from `(strand, chromosome, start [, end] [, barcode])` joined with `:`. This key is looked up in `%unique_seqs` hash. If found, the read is a duplicate (removed); if not, the read is written to output and the key is inserted. A second hash `%positions` tracks duplicate positions for reporting. For WGS datasets (e.g., 30x human), this hash holds tens of millions of entries. Perl hashes have ~100 bytes overhead per key-value pair.
- **Why Rust:** Memory and speed. A Perl hash with 50M entries at ~100 bytes/entry = ~5GB RAM. A Rust `FxHashSet<u64>` with hashed composite keys = ~800MB. The per-lookup cost drops from Perl's string hashing + comparison + allocation to Rust's inline FxHash + integer comparison. The CIGAR parsing for end-position calculation (duplicated ~6 times in the script!) also benefits.
- **Estimated speedup:** 3-10x speed, 5-8x memory reduction.
- **Rust approach:**
  - `rust-htslib` for BAM reading/writing
  - `FxHashSet<u64>` or `FxHashSet<(u32, u32, u32, u8)>` for dedup keys (chr_id, start, end, strand)
  - CIGAR end-position calculation reused from the shared CIGAR module
  - Streaming: read record, compute key, check set, write or skip
  - Memory-mapped BAM reading for large files
- **Interface:** `bismark-dedup [--single|--paired] [--barcode] [--bam] -o <output> <input.bam>`. Direct BAM-to-BAM, no samtools pipe.

### P1: Genome Loading and Cytosine Iteration

- **Location:** `coverage2cytosine` lines 168-700 (`generate_genome_wide_cytosine_report`), lines 1578-1669 (`read_genome_into_memory`)
- **Current implementation:** The entire reference genome is loaded into memory as Perl strings (`$chromosomes{$chr} = $sequence`). For human, this is ~3GB of sequence stored as Perl scalars (~6GB actual RAM due to Perl string overhead). Then a regex `while ($chromosomes{$last_chr} =~ /([CG])/g)` iterates through EVERY position, extracting trinucleotide context via `substr()`, doing reverse-complement via `reverse` + `tr/ACTG/TGAC/`, checking against the coverage hash, determining CG/CHG/CHH context via regex, and writing output. For a human genome in CX mode, this processes ~1.2 billion cytosine positions (both strands).
- **Why Rust:** The regex-based position iteration (`/([CG])/g`) is extremely slow for scanning 3 billion characters. Rust could use SIMD (`memchr` or custom AVX2/NEON) to find C/G positions in bulk. The `substr` + `reverse` + `tr///` for trinucleotide context on reverse strand is 3 separate Perl operations that could be a single lookup table in Rust. The coverage hash lookup (`exists $chr{$last_chr}->{$pos}`) could be a simple array index if positions are stored in a sorted vector.
- **Estimated speedup:** 10-30x. The genome scan is purely CPU-bound, and SIMD character scanning provides 16-32x speedup over byte-by-byte regex matching.
- **Rust approach:**
  - `2bit` or packed representation for genome (4x less memory than ASCII)
  - `memchr` crate with SIMD for scanning C/G positions
  - Pre-computed trinucleotide context lookup table (no `substr` + `reverse` + `tr`)
  - Sorted `Vec<(u32, u16, u16)>` for coverage data (position, meth_count, unmeth_count) with binary search
  - Parallel chromosome processing with `rayon`
- **Interface:** `bismark-cov2cyt --genome <genome_dir> [--CX] [--merge_CpG] [--nome-seq] [--gzip] -o <output> <input.cov.gz>`. Direct replacement for `coverage2cytosine`.

### P2: bedGraph Aggregation and Sorting

- **Location:** `bismark2bedGraph` lines 316-506
- **Current implementation:** Two modes: (1) Unix `sort` piped through shell (slow, disk-based), (2) `--ample_memory` mode using two enormous Perl arrays sized to the largest chromosome (~250M entries for human chr1, ~16GB for two arrays). In the sort-based mode, each line is parsed, and position-based aggregation happens in a streaming fashion. In the array mode, methylated/unmethylated counts are accumulated per position, then iterated.
- **Why Rust:** The sort-based mode shells out to Unix `sort` which is already well-optimized. The array-based mode is memory-wasteful (allocating 250M-entry arrays for sparse data). Rust could use a `HashMap<u32, (u16, u16)>` per chromosome (position -> counts) which would use only as much memory as there are covered positions. For the sort-based path, Rust's in-memory sort would avoid the temp-file overhead.
- **Estimated speedup:** 2-5x. This is partially I/O-bound (reading/writing coverage files), but the aggregation and sorting benefit from Rust's data structures.
- **Rust approach:**
  - `FxHashMap<u32, (u16, u16)>` per chromosome for position-based aggregation
  - In-memory sort of positions after aggregation (typically <10M positions per chromosome even for CX)
  - Buffered gzip output via `flate2` crate
  - Could be combined with the methylation extractor as a post-processing step
- **Interface:** Already being replaced by Python. If Rust is desired: `bismark-bedgraph [--CX] [--cutoff N] -o <output> <meth_extractor_output_files>`.

## Not Recommended for Rust

### bismark (main alignment engine) -- Already Replaced
The alignment engine (9,999 lines) has been replaced by native Nextflow alignment processes that call Bowtie2/HISAT2/minimap2 directly. The Perl wrapper's main value was in managing the bisulfite conversion logic (C->T and G->A genome handling, four-strand alignment), which is now handled by Nextflow subworkflows. The actual alignment is delegated to the aligner.

### bismark_genome_preparation -- Already Replaced
Replaced by native Nextflow genome preparation processes. The script just generates in-silico converted genomes and calls the aligner's indexing command.

### bin/bismark_genome_conversion.pl -- Being Replaced by awk
Simple FASTA C->T/G->A transliteration. The `tr/C/T/` and `tr/G/A/` operations are I/O-bound (limited by FASTA read/write speed). Awk or even `sed` can do this at line speed.

### bismark2report -- I/O Bound, Small Data
Reads a few summary files, does regex substitutions on an HTML template, and writes a single HTML file. Total data processed is kilobytes. No hot loops. Not worth optimizing.

### bismark2summary -- I/O Bound, Small Data
Similar to `bismark2report` but aggregates across samples. Reads report files (kilobytes each), builds an HTML summary with embedded Plotly charts. The `while(<BISMARK_REPORT>)` loops process at most hundreds of lines per sample. Not a bottleneck.

## Recommended Implementation Order

1. **Single Rust binary: `bismark-meth-extract`** (P0 components combined)
   - Replaces `bismark_methylation_extractor` entirely
   - Includes native BAM reading, CIGAR parsing, methylation call extraction, M-bias calculation
   - This is the highest-impact target: the methylation extractor is the longest-running step in most pipelines
   - Suggested crates: `rust-htslib`, `memchr`, `flate2`, `rayon` (for `--multicore`)

2. **`bismark-dedup`** (P1)
   - Replaces `deduplicate_bismark`
   - Shares CIGAR parsing code with `bismark-meth-extract`
   - Can be a separate binary or a subcommand of a unified `bismark-rs` tool

3. **`bismark-cov2cyt`** (P1)
   - Replaces `coverage2cytosine`
   - Independent of the other tools (operates on coverage files + genome FASTA)
   - High impact for genome-wide CX reports (hours -> minutes)

4. **bedGraph generation** (P2, optional)
   - Could be folded into `bismark-meth-extract` as a `--bedGraph` flag
   - Or left as Python replacement if that's already working well

## Shared Rust Infrastructure

All three binaries should share a common library crate with:
- CIGAR string parser (zero-allocation walker)
- SAM/BAM tag extraction helpers
- Bismark strand identification (XR:Z: + XG:Z: -> OT/CTOT/CTOB/OB)
- Methylation character classification lookup table
- Genome sequence loading and context extraction
