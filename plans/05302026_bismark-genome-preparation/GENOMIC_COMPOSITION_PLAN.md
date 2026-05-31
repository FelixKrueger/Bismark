# PLAN — `--genomic_composition` (genomeprep follow-up #919)

**Status:** REVISED rev 1 (2026-05-31) — manual review ✅, **dual plan-review folded in** (`GENOMIC_COMPOSITION_PLAN_REVIEW_A.md`/`_B.md`). **Awaiting implementation trigger. Do not implement** until an explicit trigger.
**Rev 1 (review fold-in):** (a) **first line of each file is unconditionally a header** (error if not `>`; NOT counted as sequence) — mirror `extract_chromosome_name`; (b) the freq pass does its **own duplicate-chromosome-name check and errors BEFORE writing the table** (Perl dies in the freq step before any output — "rely on the conversion's dup-check" was wrong: it would leave an orphan table file); (c) `s/\r//` = remove the **first `\r` anywhere** (not just trailing); (d) `prev` (di-carry) reset **per file** (at file start + each header); (e) reuse `convert::open_fasta` (make it `pub(crate)`); (f) **byte sort, NOT `fasta_name_cmp`** (that case-folds); (g) **array-indexed counters** (`[u64;256]` mono + flat `[u64;65536]` di) — no per-base allocation on GB genomes; (h) decisions resolved: write non-fatal, empty genome → 0-byte file.
**Branch:** `rust/genomeprep-genomic-composition` (off `rust/iron-chancellor`). **Issue:** #919 (deferred from epic #912). **Companion:** the genomeprep `SPEC.md` §2.7 / §5.3 (same dir).
**Acceptance gate:** `genomic_nucleotide_frequencies.txt` **byte-identical** to Perl Bismark v0.25.1.

---

## 1. What it does (Perl contract — `bismark_genome_preparation:518–570, 665–751`)

When `--genomic_composition` is given, *before* the bisulfite conversion, read the genome and write a **mono- + di-nucleotide frequency table** to `<genome>/genomic_nucleotide_frequencies.txt`. Currently the Rust port **accepts-and-ignores** the flag (with a note); this implements it.

**The frequency path is NOT the conversion path** (load-bearing): `read_genome_into_memory` does **`uc` only** — it does **NOT** apply the conversion's `[^ATCGN]→N` substitution. So the counter sees **raw uppercased bytes**:
- **Mono:** for every byte of the (uppercased) sequence, `freqs{byte}++` **unless the byte is `N`**. → IUPAC ambiguity codes (`R`,`Y`,`S`,`W`,`K`,`M`,`B`,`D`,`H`,`V`) and even stray non-ACGTN bytes (e.g. a space) become their own keys; only literal `N` is skipped.
- **Di:** for each position `i` in `0..len-1` with `i+2 <= len`, `di = seq[i..i+2]`; `freqs{di}++` **unless `di` contains `N`** (`index($di,'N') < 0`). Di-mers are counted on the **concatenated** chromosome sequence, so they **span line boundaries but NOT chromosome boundaries** (each chromosome is counted separately).
- Counts are summed across all chromosomes into one hash (order-independent — addition commutes; Perl iterates `keys %chromosomes` in hash order, which is fine).

**Output:** `"$key\t$count\n"` per key, **sorted by key** (Perl `sort` = byte-wise `cmp`), written to `${genome_folder}genomic_nucleotide_frequencies.txt` (the **genome folder**, not `Bisulfite_Genome/`). **Non-fatal:** if the file can't be opened, Perl `warn`s and skips the table (does not die).

**Other faithful details:**
- Same FASTA discovery as the conversion (extension precedence `.fa`→`.fa.gz`→`.fasta`→`.fasta.gz`, first non-empty group; reuse `discovery::find_fasta_files`).
- **Skip the legacy hardcoded `Mus_musculus.NCBIM37.fa`** filename (line 694) — that file is excluded from frequency counting (but NOT from the conversion).
- Sequence lines: Perl `chomp` (strip trailing `\n`) then `s/\r//` (remove the **first** `\r`), then `uc`. For normal/CRLF genomes this = strip the line terminator; an interior/multiple-`\r` line is pathological (Perl removes only the first `\r`) — document, don't over-engineer.
- gzip input read in-process (`MultiGzDecoder`), as the conversion does.
- `#74` (a Perl cwd bug) is **fixed in v0.25.1 and irrelevant to Rust** (absolute paths, no `chdir`) — nothing to reproduce.

---

## 2. Implementation outline

**New module `src/composition.rs`:**
- **Counters (array-indexed — no per-base allocation, rev-1):** `let mut mono = [0u64; 256]; let mut di = vec![0u64; 256*256];` (di flat-indexed `p*256 + u`). At the end, emit in **byte-lexical key order** by iterating: for `b in 0..=255`: if `mono[b] > 0` emit the 1-byte key `[b]`; **then** for `c in 0..=255`: if `di[b*256+c] > 0` emit the 2-byte key `[b,c]`. This yields exactly Perl's `sort` order (mono `A` before its di `AA`,`AC`,… because for each leading byte we emit the mono then its di block; and across leading bytes ascending) — **plain byte order, NOT `fasta_name_cmp`** (which case-folds). *(Equivalent to a `BTreeMap<Vec<u8>>` but allocation-free on the 3 Gbp hot path.)*
- `pub fn write_genomic_composition(files: &[PathBuf], genome_folder: &Path, logger: &Logger) -> Result<(), GenomePrepError>`:
  - `let mut seen: HashSet<Vec<u8>>` for the dup-name check (see below).
  - For each file: **skip if `file_name() == "Mus_musculus.NCBIM37.fa"`** (byte match). `reuse convert::open_fasta` (make it `pub(crate)`) — gz-aware; do NOT duplicate the gz detection.
    - `prev: Option<u8> = None` (declared **per file** — di never spans files or chromosomes).
    - **First line = header, unconditionally** (mirror `convert_split`): `read_until(b'\n')`; if 0 bytes → empty file → `NotFasta` (Perl: undef first line → die). Else `extract_chromosome_name(&line, file)?` (errors if first byte ≠ `>`); **dup-name check:** `if !seen.insert(name.to_vec()) { return Err(DuplicateChromosome(...)) }` — **BEFORE any table is written** (Perl `die`s here, leaving no file). The header line is **NOT counted**.
    - Then loop `read_until(b'\n')`:
      - If the line starts with `>` (in-file header): `extract_chromosome_name` + dup-check (as above); `prev = None`; do NOT count.
      - Else (sequence line): strip the trailing `\n`, then remove the **first `\r` anywhere** in the line (Perl `chomp` then `s/\r//` — note: first `\r`, not all). For each remaining byte `b`: `u = b.to_ascii_uppercase()`; **mono:** `if u != b'N' { mono[u as usize] += 1 }`; **di:** `if let Some(p) = prev { if p != b'N' && u != b'N' { di[p as usize*256 + u as usize] += 1 } }`; `prev = Some(u)`.
  - **Write the table** (only now — after all files counted, so a dup-name/`NotFasta` error above leaves NO file): build the output to `genome_folder.join("genomic_nucleotide_frequencies.txt")` by iterating the counters in the byte-lexical order above, each line = key bytes + `b"\t"` + decimal count + `b"\n"`. **Non-fatal on open/write/close error:** log a warning and return `Ok(())` (match Perl's warn-and-skip; §5.1).

**Wiring (`pipeline.rs`):** after `create_tree` (Step I) and **before** `convert_split` (Step II) — matching Perl's order — `if config.genomic_composition { composition::write_genomic_composition(&files, &config.genome_folder, &logger)?; }`. The dup-name/`NotFasta` errors here therefore fire **before** the conversion runs and before any table is written (matches Perl). Remove the current accept-and-ignore note (`pipeline.rs` + the `cli.rs` doc). `map_into`/conversion untouched (separate read path; **no `[^ATCGN]→N` mapping** here).

---

## 3. Edge cases

- **N-skipping:** mono skips only `N`; di skips any 2-mer containing `N`. Ambiguity codes (R/Y/…) are **counted** (NOT mapped to N — unlike conversion).
- **Di across line boundaries (within a chromosome):** the cross-line carry (`prev`) is essential; reset at each header.
- **Di NOT across chromosomes:** `prev = None` at every `>` header.
- **Last base of a chromosome / genome:** no trailing di (no next byte) — handled (di only fires when there's a `prev`).
- **Empty / N-only genome:** empty `freqs` → an empty table file (Perl writes an empty file — confirm).
- **`Mus_musculus.NCBIM37.fa`** present → excluded from counting.
- **gzip input** (`.fa.gz`/`.fasta.gz`): counted the same (MultiGzDecoder).
- **Stray bytes** (space/tab inside a sequence line): counted as their own keys (faithful — no N-mapping in this path).
- **Sort:** byte-lexical (`BTreeMap<Vec<u8>>` iteration = Perl `sort` for byte keys). `A` < `AA` < `AC` … < `AT` < `C` … (prefix sorts first).

---

## 4. Tests

- **Unit (`composition.rs`):** a known short genome (temp file) → exact expected table bytes. Cover: ACGT-only (mono A/C/G/T + the di present); a sequence with `N` (mono N skipped, di-with-N skipped, di across the N split correctly); an **ambiguity code** (`R` counted as mono + in di); **di across a line boundary** (multi-line record → cross-line di counted); **di NOT across chromosomes** (two records in one file → no di spanning them); **di NOT across files** (`prev` reset per file); **blank line mid-chromosome** preserves `prev` (di spans the blank line — Perl concatenates non-empty + empty lines, but a blank line contributes no bytes, so `prev` carries across it); the exact **sort order** (mono-before-its-di, e.g. `A` < `AA` < `AC`; ambiguity-code keys interleaved by byte). **Error paths (rev-1):** **first line not `>`** → `NotFasta` (+ no table); **bare `>` first line** → empty name, fine, first line not counted; **duplicate chromosome name** → `DuplicateChromosome` AND **the table file is NOT created**; **`s/\r//` first-`\r`-only** (`A\r\rC` → counts `A`,`\r`,`C` i.e. the SECOND `\r` survives); a **final line with no trailing `\n`** counted fully.
- **Integration / Perl-oracle:** add a `--genomic_composition` case to `tests/integration.rs` `oracle_compare` (or a dedicated test) comparing `genomic_nucleotide_frequencies.txt` byte-for-byte vs the real Perl script on a small synthetic genome (auto-skip if `perl` absent). Add `genomic_nucleotide_frequencies.txt` to the `tests/byte_identity_real_data.rs` `#[ignore]` gate's compared outputs (real E. coli on oxy).
- **CLI:** `--genomic_composition` no longer emits the "deferred/ignored" note; the file is produced.

---

## 5. Decisions (RESOLVED via dual plan-review)
1. **Write-failure handling → NON-FATAL** (warn + skip the table, continue), matching Perl. Applies to open **and** write/close errors.
2. **Duplicate chromosome name → the freq pass does its OWN check and errors BEFORE writing the table** (both reviewers; reverses the rev-0 recommendation). Perl runs the freq step before conversion and `die`s on a dup name in `read_genome_into_memory` with **no table written**; "rely on the conversion's dup-check" would write the full table then die later → an orphan file Perl never creates. (Cheap: a `HashSet` populated at each header.)
3. **Empty / N-only genome → a 0-byte file IS created** (both reviewers confirmed from source). Match (iterate empty counters → write nothing → empty file).
4. **(rev-1) First line of each file is unconditionally a header** — error (`NotFasta`) if its first byte isn't `>`, and it is **never counted as sequence** (Perl reads `<CHR_IN>` as the header with no `/^>/` test). Mirror `extract_chromosome_name` / `convert_split`.

---

## 6. Out of scope
- No change to the conversion, indexer, or `--combined_genome` paths.
- The frequency table is only produced under `--genomic_composition` (default off, unchanged).
