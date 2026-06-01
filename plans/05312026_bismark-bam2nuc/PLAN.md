# PLAN — `bismark-bam2nuc` (Rust port of Perl `bam2nuc`)

**Status:** rev 1 — dual plan-review folded in (both reviewers APPROVE; reports `PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md`). Awaiting the explicit implementation trigger (do NOT implement until then).
**Date:** 2026-05-31
**Source design:** `SPEC.md` (rev 1, all design Qs resolved). Read it first.
**Mode:** TDD (RED → GREEN → REFACTOR per task; byte-identity goldens are the acceptance driver).
**Worktree/branch:** `rust/bam2nuc` @ `~/Github/Bismark-bam2nuc`. **Build green baseline confirmed** (`cargo check --workspace`).

### rev-1 review fold-in (2026-05-31)
Both reviewers independently re-derived every byte-identity-critical claim against the live Perl interpreter (+ live Rust-vs-Perl for `%.Nf` rounding) and confirmed them ALL correct. Changes folded in:
- **C-1 (Critical, both):** add `noodles-bam`/`noodles-bgzf` as **runtime** deps (Task A1). Resolves **OI-1**: the raw-`RecordBuf` path is **direct `noodles_bam::io::Reader::record_bufs`** — `bismark-io` offers no tag-agnostic passthrough.
- **C-2 (Critical, B):** define the `alignment_start() == None` policy (Task C3/C5). `noodles Position` is `NonZero`, so Perl's POS=0 negative-offset `substr` is unreachable; the `None` case must NOT silently skip (keeps the SE flag-`die` faithful).
- **C-3 (Critical, both):** the **empty-count-field** is a HARD, pinned golden cell (Task D2/E2) — it is unreachable on the oxy gate (real data fills every cell), so the local golden is its sole guard.
- **I (Important):** allocation-free `NucCounts` via `[u64;256]` mono + boxed packed-`u16` di array (Task B1/B2; also makes `count==0 ⇔ absent`, cleanly matching Perl's undef→empty-field); case-sensitive `derive_output_name` (`.BAM`→Err, Task D1); substr POS-strictly-past-end test (C3); IUPAC-on-revcomp test vector (C4); non-canonical-PE-flag + all-InDel-skipped(ZeroDivision exit) + synthetic-×1000 cache-reuse + differing-two-file golden cells (E2); SE/PE-detection divergence cell or documented raw-Bismark-only (F1).
- **O (Optional folded):** delete c2c's stale "no-iterator" invariant comment when adding `seqs()` (A3); document `*`-SEQ divergence + `--samtools_path` not validated + partial-file-on-`ZeroDivision` parity (§ Deviations); empty-genome 0-byte-cache + content-BAM-named-`.sam` cells (E2); single-threaded-BGZF throughput note (C5).

One-line goal: produce a `bam2nuc_rs` binary whose `*.nucleotide_stats.txt` and `genomic_nucleotide_frequencies.txt` are **byte-identical to Perl `bam2nuc` v0.25.1**.

---

## Test infrastructure

- **Framework:** Rust `cargo test -p bismark-bam2nuc` (unit `#[cfg(test)]` modules + `tests/*.rs` integration goldens). No Python.
- **Local Perl oracle (confirmed available):** `/usr/bin/perl ./bam2nuc` (v0.25.1), `samtools 1.21` at `/opt/homebrew/bin/samtools`. Goldens generated locally via `tests/data/generate_goldens.sh` (mirrors c2c's `generate_goldens.sh`). Pin `LC_ALL=C` in the script.
- **Synthetic fixtures (built by the golden script, committed):**
  - Tiny genome dir(s): a 1–2 chromosome `.fa` with hand-chosen ACGTN content (+ one IUPAC base in a dedicated cell to exercise the count-everything cache rows). Lengths chosen so reads can run off the chromosome end (substr-saturation cell).
  - Tiny BAMs: hand-written SAM → `samtools view -bS`. SE BAM (flags 0/16, `@PG ID:Bismark` SE command line) and PE BAM (flags 99/147/83/163, `@PG` with `-1 … -2 …`). Plus adversarial cells: an InDel-CIGAR read (skipped), a read near a chromosome end (truncated span), a read on a chr absent from the genome (contributes nothing), a read whose span is all-N.
  - Rust unit tests build in-memory SAM via `noodles_sam`/`Cursor` (like `bismark-io` tests) and BGZF `.bam` via `noodles_bgzf::io::Writer` (like c2c `genome.rs` V12b) where a real file is needed.
- **Dev-deps (mirror c2c/extractor):** `assert_cmd`, `predicates`, `tempfile`, `noodles-bgzf` (BAM fixture writer), `noodles-sam` (record/header construction), `bstr`, `flate2` (decode `.fa.gz` fixtures + any gz goldens).
- **No new test data must be requested from the user** — everything is synthesizable locally with the confirmed Perl+samtools oracle.

---

## Plan coverage checklist

Every SPEC behaviour maps to ≥1 task. (Empty Task cell ⇒ incomplete — none allowed.)

| # | Plan item (SPEC §) | Task(s) |
|---|---|---|
| 1 | Crate `bismark-bam2nuc` + workspace member + binary `bam2nuc_rs` + thin `main.rs` ExitCode 0/1/2 (§2) | A1, E1 |
| 2 | `--version` one-liner `version_string()` (§3, D2) | A1 |
| 3 | CLI parse: `--dir`, `-g/--genome_folder`(mandatory), `--parent_dir`, `--samtools_path`(no-op), `--genomic_composition_only`, `--version`, positional inputs (§3) | A2 |
| 4 | Path resolution: `output_dir` default `""`+trailing-slash; `parent_dir` default `getcwd()`+slash; `genome_folder` mandatory+slash (§3) | A2 |
| 5 | Arg-count rule: no inputs AND not `--genomic_composition_only` → help/error (§3) | A2 |
| 6 | `--samtools_path` accepted-but-ignored no-op (Q4, §12 D1) | A2 |
| 7 | Genome reader: 4-suffix glob priority, Mus skip, uppercase, `\r` strip, dup-name error, first-token name, gz support (§5) | A3 |
| 8 | Genome `seqs()` all-sequences accessor (order-agnostic) for genomic composition (§5) | A3 |
| 9 | `process_sequence` mono counting (skip `N`, count IUPAC) (§6.3) | B1 |
| 10 | `process_sequence` di counting (overlapping windows, skip N-window, count IUPAC) (§6.3) | B1 |
| 11 | Genomic composition over all chromosomes (+strand, no revcomp) (§6.3) | B3 |
| 12 | Cache WRITE: bytewise-sorted `word\tcount\n`, count-everything (incl. IUPAC) (§6.2) | B3 |
| 13 | Cache write precedence: genome_folder → output_dir → skip-with-warn (§6.1) | B3 |
| 14 | Cache READ + reuse-if-present; existence checked ONLY against genome_folder (§6.1) | B4 |
| 15 | chr-name table from header reference sequences (§7.2) | C1 |
| 16 | CIGAR `[IDSN]` skip = op kinds {Insertion,Deletion,SoftClip,Skip}; H/P/=/X kept (§7.3) | C2 |
| 17 | Genomic span extraction `substr(chr,POS-1,len(SEQ))` with saturation (missing chr/end/oob → clamp, never panic) (§7.4) | C3 |
| 18 | SE flag correction: 0→fwd, 16→revcomp, else→hard error (§7.5) | C4 |
| 19 | PE flag correction: 99/147→fwd, else→revcomp, NEVER die (latent `or 163` bug) (§7.5, D3) | C4 |
| 20 | reverse-complement = `reverse` + `tr/GATC/CTAG/` (N/others untouched) (§7.5) | C4 |
| 21 | SE/PE detection via `detect_paired_from_header`; `None`→error (§7.1, Q5) | C5 |
| 22 | Per-read driver: open `without_sort_check`, raw `RecordBuf`, field extract, skip InDel, span, correct, accumulate sample counts (§7.2, §11, Q1) | C5 |
| 23 | Unmapped-read policy: keep SE die-on-unexpected-flag faithful (do NOT silently drop) (§11) | C4, C5 |
| 24 | Output filename: basename + `s/(bam|cram)$/nucleotide_stats.txt/`; non-bam/cram → error (no-dot-anchor quirk) (§9, D4) | D1 |
| 25 | Stats header line exact (§8.1) | D2 |
| 26 | Mono rows A,C,G,T then 16 di rows fixed order (§8.1) | D2 |
| 27 | Separate mono vs di totals; `%.2f` pct, `%.3f` coverage (§8.2) | D2 |
| 28 | Missing sample count → empty field, 0 in math (§8.3) | D2 |
| 29 | Division-by-zero → hard error matching Perl (Q6, §8.3) | D2 |
| 30 | `%.2f`/`%.3f` round-half-to-even parity vs C printf (§8.3, §10) | D2, F1 |
| 31 | Top-level flow incl. `--genomic_composition_only` compute+write+exit; per-file loop resets sample counts; reuse cache after file #1 (§4) | E1 |
| 32 | Input format gate: accept `.bam`; reject `.sam`(Q2) + `.cram`(Q3) with clear errors (§1, §9) | E1 |
| 33 | mimalloc global allocator (Q8, §2) | A1 |
| 34 | Local Perl-oracle goldens + `generate_goldens.sh` + integration byte-compare (§14) | E2 |
| 35 | oxy real-data byte-identity gate: SE + PE + `--genomic_composition_only` + cache-reuse; `LC_ALL=C` (Q9, §10, §14) | F1 |
| 36 | Docs: README + CHANGELOG + de-jargoned `--help` (sibling convention) | F2 |
| 37 | Multiple input files in argv order, one stats file each (§4) | C5, E1 |
| 38 | `noodles-bam`/`noodles-bgzf` runtime deps; direct `record_bufs` read path (C-1/OI-1) | A1, C5 |
| 39 | `alignment_start() == None` policy (no silent skip; SE stray-flag errors faithfully); POS=0 negative-offset unreachable (NonZero) (C-2) | C3, C5 |
| 40 | Allocation-free `NucCounts` (`[u64;256]` + boxed packed-`u16` di); `count==0 ⇔ absent` (I-2) | B1, B2, D2 |
| 41 | Case-sensitive output-name strip (`.BAM`→Err) reconciled with E1 gate (I-1) | D1 |
| 42 | SE/PE-detection divergence (ID:Bismark-scoped @PG) covered or documented (I-3) | C5, F1 |
| 43 | `--samtools_path` not validated (D1a); `*`-SEQ divergence (D7) documented | A2, F2 |

---

## Phase A — Scaffold + CLI + genome reader

> Closest cribs: c2c `Cargo.toml`, `main.rs`, `lib.rs`, `cli.rs`, `error.rs`, `genome.rs`; extractor `Cargo.toml` (mimalloc).

### Task A1 — Crate scaffold + error enum skeleton + mimalloc + version
**Files:** `rust/Cargo.toml` (add member), `rust/bismark-bam2nuc/Cargo.toml` (new), `src/lib.rs`, `src/main.rs`, `src/error.rs` (new).

- RED: `tests/sanity.rs` — assert `version_string()` starts with `"bam2nuc_rs "` and contains the OS; assert `BismarkBam2nucError` `Display` round-trips for one variant. `cargo test -p bismark-bam2nuc` fails to compile (crate doesn't exist).
- GREEN:
  - Add `"bismark-bam2nuc"` to `rust/Cargo.toml` `members`.
  - `Cargo.toml`: name `bismark-bam2nuc`, version `1.0.0-alpha.1`, `[[bin]] name="bam2nuc_rs"`, deps `bismark-io`(path,`=1.0.0-beta.8`), `clap`(derive,`=4.5.30`), `thiserror`(`=2.0.0`), `noodles-fasta`(`=0.61.0`), `flate2`(`=1.1.9`), `noodles-sam`(`=0.85.0`), **`noodles-bam`(`=0.89.0`)**, **`noodles-bgzf`(`=0.47.0`)**, `mimalloc`(`=0.1.52`,default-features=false); dev-deps `assert_cmd`/`predicates`/`tempfile`/`bstr` (`noodles-bgzf` is now a regular dep). **C-1:** `noodles-bam`/`noodles-bgzf` are RUNTIME deps — the Phase-C reader is `noodles_bam::io::Reader::record_bufs` (a `noodles_bam::io::Reader<noodles_bgzf::io::Reader<R>>`); `bismark-io` does NOT re-export noodles-bam and its only BAM API forces XR/XG/XM + drops unmapped, so it cannot supply the tag-agnostic path. Pins match the single transitive versions in the workspace `Cargo.lock` (verified: one `noodles-sam 0.85.0`, `noodles-bam 0.89.0`, `noodles-bgzf 0.47.0`) — do NOT bump, or a second copy enters the lock and `record_bufs(&header)` won't type-check across noodles-sam versions.
  - `main.rs`: `#[global_allocator] static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;`, `fn main() -> ExitCode` mirroring c2c (parse → `--version` → `run` → map err to exit 1).
  - `lib.rs`: module decls (stub the not-yet-written ones behind later tasks), `version_string()` (format `"bam2nuc_rs {ver} ({os}/{arch})"`), `pub fn run(config:&ResolvedConfig)->Result<(),BismarkBam2nucError>` stub returning `Ok(())` (filled E1).
  - `error.rs`: `BismarkBam2nucError` enum (thiserror), initial variants: `Io(#[from] io::Error)`, plus placeholders added per phase. Messages echo Perl wording where it exists.
- VERIFY: `cargo test -p bismark-bam2nuc` green; `cargo build -p bismark-bam2nuc` produces `bam2nuc_rs`.

### Task A2 — CLI parser + validation/resolution (`cli.rs`)
**Files:** `src/cli.rs` (new), `src/error.rs` (+variants), `src/lib.rs` (export).

- RED: `cli.rs` `#[cfg(test)]` tests:
  - `clap_definition_is_valid` (`Cli::command().debug_assert()`).
  - parses positional inputs (`Vec<PathBuf>`), `-g/--genome_folder`, `--dir`, `--parent_dir`, `--samtools_path`, `--genomic_composition_only`, `-V/--version`.
  - `validate()`: missing `-g` → `MissingGenomeFolder`; no inputs AND not `--genomic_composition_only` → `MissingInput`(help-equivalent); `--samtools_path` value is ignored (no error, not stored as behaviour); `genome_folder` trailing-slash normalised; `output_dir` defaults `""`, `--dir x` → absolute + trailing `/`; `parent_dir` defaults `current_dir()`.
- GREEN: `Cli` (clap derive) + `ResolvedConfig { inputs:Vec<PathBuf>, output_dir:String, parent_dir:PathBuf, genome_folder:PathBuf, genomic_composition_only:bool }`. `validate()` mirrors Perl `process_commandline` order. Reuse c2c's `resolve_output_dir` helper verbatim. Add error variants `MissingGenomeFolder`, `MissingInput`.
- VERIFY: `cargo test -p bismark-bam2nuc cli`.

> Note: `--samtools_path` is parsed (so the `bismark` pipeline's invocation doesn't error) but dropped during resolution — it is NOT in `ResolvedConfig`.

### Task A3 — Genome reader (`genome.rs`)
**Files:** `src/genome.rs` (new), `src/error.rs` (+variants), `src/lib.rs` (export).

- RED: port c2c `genome.rs`'s test battery (multifasta first-token+uppercase, glob priority, Mus skip, no-fasta error, gz plain + BGZF, duplicate-name error, CRLF strip, empty-seq kept, bare-header error) **plus** a new test: `seqs()` yields all `(name,seq)` pairs (count + content), order-agnostic.
- GREEN: copy c2c `genome.rs` into this crate, renaming the error type to `BismarkBam2nucError` with equivalent variants (`NoGenomeFasta`, `DuplicateChromosomeName`, `MalformedFastaHeader`, `ChromosomeTooLong`). **Add** `pub fn seqs(&self) -> impl Iterator<Item=(&[u8],&[u8])>` (or `values()`) — bam2nuc needs to iterate ALL sequences for genomic composition; ordering is commutative (di-windows never cross chromosome boundaries) so no insertion-order invariant is needed. **O-4:** when copying, DELETE c2c genome.rs's module-level "exposes NO public insertion-order iterator … Do NOT add an `iter()`/`keys()` passthrough" invariant comment — adding `seqs()` directly contradicts it (that invariant existed only to protect c2c's covered-vs-uncovered OUTPUT ordering, which bam2nuc has no analogue of). Keep `get(name)` for span extraction.
- VERIFY: `cargo test -p bismark-bam2nuc genome`.

---

## Phase B — `process_sequence` counter + genomic-composition cache (`freqs.rs`)

> The heart of the byte-identity contract for the cache file.

### Task B1 — `process_sequence` mono/di counter
**Files:** `src/freqs.rs` (new), `src/lib.rs` (export).

- RED: `freqs.rs` tests:
  - `"ACGT"` → mono A,C,G,T each 1; di AC,CG,GT each 1 (3 windows).
  - N handling: `"ANG"` → mono A,G (no N); di: none (AN has N, NG has N).
  - IUPAC counted: `"ARG"` → mono A,R,G; di AR,RG.
  - overlapping: `"AAAA"` → mono A=4; di AA=3.
  - empty / 1-base: `""` → nothing; `"A"` → mono A=1, no di.
- GREEN: `pub fn process_sequence(seq:&[u8], counts:&mut NucCounts)`:
  ```rust
  for i in 0..seq.len() {
      let m = seq[i];
      if m != b'N' { counts.bump_mono(m); }
      if i + 2 <= seq.len() {
          let (a, b) = (seq[i], seq[i+1]);
          if a != b'N' && b != b'N' { counts.bump_di(a, b); }
      }
  }
  ```
  **Counts everything except N-containing words** (no ACGT pre-filter — IUPAC bytes are counted).

  **`NucCounts` = ALLOCATION-FREE array representation** (Important I-2/E-1, both reviewers — a `HashMap<Vec<u8>,u64>` would allocate a key per `bump`, ~6 B allocations on the genome pass + ~11 B over 55 M reads):
  ```rust
  pub struct NucCounts {
      mono: [u64; 256],            // indexed by byte
      di:   Box<[u64; 65536]>,     // indexed by (a as usize) << 8 | b as usize ; 512 KiB, heap
  }
  impl NucCounts {
      fn bump_mono(&mut self, b: u8)        { self.mono[b as usize] += 1; }
      fn bump_di(&mut self, a: u8, b: u8)   { self.di[(a as usize) << 8 | b as usize] += 1; }
      pub fn mono(&self, b: u8) -> u64      { self.mono[b as usize] }
      pub fn di(&self, a: u8, b: u8) -> u64 { self.di[(a as usize) << 8 | b as usize] }
  }
  ```
  **Key invariant (subsumes the §8.3 empty-field logic):** counts only ever increment from 0, so **`count == 0` ⇔ "never seen" ⇔ Perl's `undef`**. The report (D2) prints an empty field when `count == 0` and uses `0` in the arithmetic — exactly Perl's undef→`""`-in-column / undef→0-in-math behaviour, with no `Option`/`contains_key` bookkeeping. Box the di array (512 KiB) to avoid a large stack frame; two live `NucCounts` (sample + genomic) ≈ 1 MiB — fine.
- VERIFY: `cargo test -p bismark-bam2nuc freqs::tests::`.

### Task B2 — `NucCounts` cache serialization helper
**Files:** `src/freqs.rs`.

- RED: tests — `to_cache_string()` produces bytewise-sorted `word\tcount\n` lines; verify exact 20-line ordering for an ACGTN-only count set (`A,AA,AC,AG,AT,C,CA,…,T,TA,TC,TG,TT`); verify IUPAC placement precisely (**O-4/B**): a single-char `R` (byte 0x52) sorts after the `G*` di-group and after `N`, before `T`; `AR` lands **between `AG` and `AT`** (A=0x41,G=0x47,R=0x52,T=0x54 → AG<AR<AT). `count==0` cells are omitted (never-seen words are not written — matches Perl writing only `keys %genomic_freqs`).
- GREEN: `to_cache_string(&self)->String`: collect `(word_bytes, count)` for every populated cell — mono cells `mono[b]>0` → 1-byte word `[b]`; di cells `di[a<<8|b]>0` → 2-byte word `[a,b]` — into a `Vec<(Vec<u8>,u64)>`, `sort_unstable_by(|x,y| x.0.cmp(&y.0))` (bytewise, matching Perl `sort keys` under `LC_ALL=C`), then join `format!("{}\t{}\n", str::from_utf8(&word).unwrap(), n)`. (Word bytes are ASCII → utf8 always valid.) The collect+sort runs once at cache-write time over ≤ a few hundred entries — negligible; the hot path (`bump_*`) stays allocation-free.
- VERIFY: `cargo test`.

### Task B3 — Genomic composition + cache write with precedence
**Files:** `src/freqs.rs`, `src/error.rs` (none expected — write failure falls back, doesn't error), `src/lib.rs`.

- RED: tests with `tempfile` dirs:
  - compute over a 2-chr genome → counts match a hand-computed expectation; cache file in genome_folder is byte-identical to expected string.
  - precedence: read-only genome dir (`set_permissions` 0o555) → falls back to output_dir; both unwritable → no file, no error (warn only). *(Gate the perms test on unix; skip on platforms where 0o555 dirs are still writable by owner — document.)*
  - +strand only (no revcomp) — verify a known asymmetric sequence gives strand-specific di counts.
- GREEN: `pub fn compute_genomic(genome:&Genome) -> NucCounts` (iterate `genome.seqs()`, `process_sequence` each). `pub fn write_cache(counts:&NucCounts, genome_folder:&Path, output_dir:&str) -> ()` implementing Perl precedence (try `${genome_folder}genomic_nucleotide_frequencies.txt`, else `${output_dir}…`, else warn-and-continue). Cache filename constant `genomic_nucleotide_frequencies.txt`.
- VERIFY: `cargo test`.

### Task B4 — Cache read + reuse-if-present (`get_genomic_frequencies`)
**Files:** `src/freqs.rs`, `src/error.rs`.

- RED: tests:
  - write a cache via B3, then `get_genomic_frequencies` reads it back into a `NucCounts` byte-equivalently (round-trip).
  - reuse precedence: cache present in genome_folder → returns parsed counts WITHOUT recomputing (assert by planting a cache whose counts differ from what the genome would produce, and checking the planted values win).
  - existence check is ONLY against genome_folder (a cache sitting in output_dir does NOT prevent recompute).
- GREEN: `pub fn get_genomic_frequencies(genome:&Genome, genome_folder:&Path, output_dir:&str) -> Result<NucCounts,BismarkBam2nucError>`:
  ```
  if genome_folder/genomic_nucleotide_frequencies.txt exists:
      read lines, split('\t') → (word, count) into NucCounts   // count parsed as u64
      return
  else:
      counts = compute_genomic(genome); write_cache(&counts, …); return counts
  ```
  Cache-line parse: `let (w,c) = line.split_once('\t')`; tolerate trailing newline; populate the array by word length — `w.len()==1` → `mono[w[0]] = c`; `w.len()==2` → `di[w[0]<<8|w[1]] = c`; other lengths → `MalformedCacheLine{line_no}` (accepted divergence — cannot occur on a Perl-written cache). All-zero cells stay absent (never-seen), preserving `count==0 ⇔ absent`.
- VERIFY: `cargo test`.

---

## Phase C — Per-read counting (`count.rs`)

> The novel core. Operates on raw `noodles RecordBuf` (Q1). Closest cribs: extractor `header.rs`, `bismark-io` record/cigar.

### Task C1 — chr-name table from header
**Files:** `src/count.rs` (new), `src/error.rs` (+`NonAsciiChromosomeName`), `src/lib.rs`.

- RED: test — build a `noodles_sam::Header` with `@SQ SN:chr1`,`SN:chr2`; `build_chr_name_table` returns `["chr1","chr2"]` (as `Vec<Vec<u8>>`); non-ASCII name → error.
- GREEN: mirror extractor `header::build_chr_name_table` but return `Vec<Vec<u8>>` (bam2nuc indexes the genome by byte name). Index by `reference_sequence_id`.
- VERIFY: `cargo test`.

### Task C2 — CIGAR `[IDSN]` skip predicate
**Files:** `src/count.rs`.

- RED: tests — `5M`→keep; `5M2I3M`→skip; `5M2D`→skip; `2S6M`→skip; `5M2N5M`→skip; `50M2H`→keep; `*`(empty)→keep.
- GREEN: `fn cigar_has_indel(cigar:&noodles…Cigar)->bool` — true if any op kind ∈ {Insertion, Deletion, SoftClip, Skip}. Use noodles op-kind matching.
- VERIFY: `cargo test`.

### Task C3 — Genomic span extraction with substr saturation
**Files:** `src/count.rs`.

- RED: tests against a fixture chr `"ACGTACGT"` (len 8):
  - POS=1,len=4 → `"ACGT"`.
  - POS=6,len=4 (runs off end) → `"CGT"` (truncated, 3 bytes).
  - POS=9,len=2 (start exactly at end) → `""`.
  - **POS=20,len=2 (start STRICTLY past end) → `""`** (I-1/V-2: Perl `substr` returns `undef` here, behaviourally == empty in `process_sequence` — both reviewers verified live; this test guards the clamp against a future "un-clamp → panic" refactor).
  - chr absent → `""`.
- GREEN: `fn extract_span(genome:&Genome, chr:&[u8], pos1:usize, read_len:usize)->Vec<u8>`:
  ```rust
  let Some(seq) = genome.get(chr) else { return Vec::new(); };
  let n = seq.len();
  let p = pos1.saturating_sub(1);          // POS is 1-based; mapped reads pos≥1
  let start = p.min(n);
  let end = (p + read_len).min(n);
  seq[start..end].to_vec()
  ```
  **C-2 / `alignment_start() == None` policy (decided here, applied in C5):** noodles `Position` is `NonZero<usize>` (verified, noodles-core), so any record that HAS an `alignment_start` has POS ≥ 1 → `saturating_sub(1) ≥ 0`, never negative. Perl's POS=0 negative-offset `substr($chr,-1,…)` (which returns the chromosome's LAST char) is therefore **unreachable** from a noodles record — add a test COMMENT recording this so a future reader doesn't "fix" the clamp to handle it. The driver (C5) must still decide the `None` case: do **NOT** silently skip — pass it through so the SE flag-check (`correct_se`) errors faithfully on a stray-flag/unmapped read (a `None`-start record is unmapped → flag ∉ {0,16} → `InvalidSeFlag`, matching Perl's `die`). For the span itself, a `None` start is treated as the missing-chr case → empty span.
- VERIFY: `cargo test`.

### Task C4 — SE/PE flag correction (+ revcomp) — incl. latent PE bug
**Files:** `src/count.rs`.

- RED: tests:
  - revcomp: `revcomp(b"ACGTN")` → `b"NACGT"` (reverse then `tr/GATC/CTAG/`; N untouched). Verify on `b"GATC"`→`b"GATC"`(rev=CTAG, tr→GATC) and a clearly asymmetric case `b"AACG"`→ rev `GCAA` → tr `CGTT` → `b"CGTT"`.
  - **IUPAC survives revcomp untouched (I-6/B):** `revcomp(b"ARGY")` → reverse `YGRA` → `tr/GATC/CTAG/` (only G/A/T/C mapped: G→C, R/Y unmapped, A→T) → **`b"YCRT"`**. Pin these exact bytes (proves IUPAC bytes are not corrupted on a flag-16/83 reverse read).
  - SE: flag 0 → returns span as-is; flag 16 → revcomp; flag 4 (or any other) → `Err(InvalidSeFlag{flag})`.
  - PE: flag 99 → as-is; flag 147 → as-is; flag 83 → revcomp; flag 163 → revcomp; **flag 0 (PE) → revcomp** (the `or 163` bug: anything ≠99/147 revcomps, NEVER errors); flag 256 (PE) → revcomp.
- GREEN:
  ```rust
  fn revcomp(seq:&[u8]) -> Vec<u8> {
      seq.iter().rev().map(|&b| match b { b'G'=>b'C', b'A'=>b'T', b'T'=>b'A', b'C'=>b'G', x=>x }).collect()
  }
  // SE: reachable die
  fn correct_se(span:Vec<u8>, flag:u16) -> Result<Vec<u8>,BismarkBam2nucError> {
      match flag { 0=>Ok(span), 16=>Ok(revcomp(&span)), f=>Err(BismarkBam2nucError::InvalidSeFlag{flag:f}) }
  }
  // PE: replicate `elsif ($flag == 83 or 163)` → always-true → no die
  fn correct_pe(span:Vec<u8>, flag:u16) -> Vec<u8> {
      if flag == 99 || flag == 147 { span } else { revcomp(&span) }
  }
  ```
  Add error variant `InvalidSeFlag{flag:u16}` (message echoes Perl `"failed to detect valid Bismark FLAG tag: <flag>"`).
- VERIFY: `cargo test`.

### Task C5 — Per-read driver loop (one input file → sample `NucCounts`)
**Files:** `src/count.rs`, `src/error.rs` (+`SePeUndetermined`), `src/lib.rs`.

- RED: integration-style tests using in-memory SAM (`noodles_sam::io::Reader` over `Cursor`) + a tiny genome:
  - SE BAM, 2 reads (flag 0 + flag 16) on `chr1` → sample counts equal hand-computed (genomic spans, with revcomp on the flag-16 read).
  - PE BAM, reads flags 99/147/83/163 → counts match (83/163 revcomp'd).
  - InDel read skipped (count unaffected; skipped++).
  - read on missing chr contributes nothing — use a **PE flag ≠ 99/147 (e.g. 83)** so this also exercises `correct_pe` revcomp-of-empty (I-5/B #5).
  - header with no Bismark `@PG` → `SePeUndetermined` error.
  - **stray-flag fixture (C-2 / OI-1):** an SE read with an unexpected flag (e.g. 4) reaches `correct_se` → `Err(InvalidSeFlag{4})` (proves the raw reader does NOT silently drop it, keeping the SE `die` faithful).
  - leading non-Bismark `@PG` (e.g. `samtools`) + trailing Bismark PE `@PG` → Rust detects **PE**; comment notes Perl `test_file` would say SE (accepted divergence, O-2/A).
- GREEN: `pub fn count_reads_in_file(path:&Path) -> Result<(NucCounts, ReadStats), BismarkBam2nucError>`:
  - Open with **`noodles_bam::io::Reader::new(BufReader<File>)`** directly (C-1/OI-1 resolved: NOT `bismark-io::BamReader`, which forces XR/XG/XM + drops unmapped; NOT a passthrough — none exists). `read_header()` then iterate `record_bufs(&header)` — this yields raw `RecordBuf`s with **no** XR/XG/XM validation, **no** unmapped filter, and **no** coordinate-sort check (all three confirmed by both reviewers against noodles-bam 0.89.0). The `.bam` accept gate (rejecting `.sam`/`.cram`) lives in E1 via `bismark_io::AlignmentKind::from_path` (content sniff); the file is opened twice (sniff + read) — fine, sibling precedent.
  - Build chr-name table (C1) from header. Detect SE/PE via `bismark_io::detect_paired_from_header` (`None`→`SePeUndetermined`). **Divergence (I-3, accepted):** this matches the first `@PG` with `ID:Bismark`; Perl `test_file` matches the first `@PG` of any ID. Equivalent on raw single-`@PG` Bismark BAMs; differs only on `samtools sort/view`-reprocessed BAMs (which prepend a `samtools` `@PG`). F1 covers this (see Phase F).
  - For each record: read `flags` (`u16::from(rec.flags())`), `reference_sequence_id` → chr name via the C1 table (**a `None`/`-1` id → treat as absent chr → empty span**, C-2), `alignment_start()` (`Option<Position>`; `Some`→1-based `usize`, **`None`→ pass POS such that the span is empty AND the flag still flows to `correct_se`/`correct_pe`** so a stray SE flag errors faithfully), `cigar`, `sequence().len()` (= Perl `length$sequence`; assume a real SEQ on Bismark BAMs — `*`-SEQ divergence documented in § Deviations). If `cigar_has_indel` → `stats.skipped+=1; continue`. `span = extract_span(...)`. `corrected = if pe { correct_pe(span,flag) } else { correct_se(span,flag)? }`. `process_sequence(&corrected, &mut counts)`.
  - Unmapped policy (§11): do not pre-filter; raw `record_bufs` yields every record, so SE `correct_se` raises `InvalidSeFlag` on a stray flag (faithful to Perl's `die`). Confirmed by the stray-flag fixture above.
  - **Throughput (O-5/B, note only):** single-threaded BGZF decode via plain `noodles_bam::io::Reader` is acceptable for v1.0 — bam2nuc's per-read work is light (no XM parse / M-bias) and it's a QC step off the alignment hot path. The extractor's 2-thread BGZF decoder (#884) is a possible future optimization, out of scope here.
- VERIFY: `cargo test -p bismark-bam2nuc count`.

---

## Phase D — Report writer + output filename

### Task D1 — Output filename derivation (`output_name.rs`)
**Files:** `src/output_name.rs` (new), `src/error.rs` (+`NotBamOrCram`), `src/lib.rs`.

- RED: tests — `sample.bam`→`sample.nucleotide_stats.txt`; `/path/to/x.bam`→`x.nucleotide_stats.txt` (basename); `y.cram`→`y.nucleotide_stats.txt`; `z.sam`→`Err(NotBamOrCram)`; no-dot quirk `foosubbam`→`foosubnucleotide_stats.txt`; `a.bam.bam`→`a.bam.nucleotide_stats.txt` (only the trailing `bam` stripped); **case-sensitive: `weird.BAM`→`Err(NotBamOrCram)`** (I-1/B: Perl's regex `(bam|cram)$` is case-sensitive — verified `.BAM`→die). Document each quirk in a test comment.
- GREEN: `pub fn derive_output_name(infile:&Path)->Result<String,BismarkBam2nucError>`: take basename (`file_name`), if it ends with the EXACT lowercase token `"bam"` or `"cram"` → strip that trailing token and append `"nucleotide_stats.txt"`; else `Err(NotBamOrCram)`. Replicate the no-dot anchor (strip trailing `bam`/`cram`, not `.bam`/`.cram`) AND the case-sensitivity (do NOT lowercase before matching). **Reconcile with E1 (I-1):** E1's `AlignmentKind::from_path` content-sniff may *accept* a `.BAM`-named real BAM, but `derive_output_name` then returns `Err(NotBamOrCram)` — net outcome (no stats file) matches Perl, which reads the `.BAM` via plain-open garbage then dies at name derivation. Note this benign two-path-to-error in a comment.
- VERIFY: `cargo test`.

### Task D2 — Stats report writer (`calculate_averages`) (`report.rs`)
**Files:** `src/report.rs` (new), `src/error.rs` (+`ZeroDivision`), `src/lib.rs`.

- RED: tests:
  - exact byte output for a hand-built `(sample, genomic)` count pair: header line + 4 mono + 16 di rows, tabs + trailing `\n`, `%.2f`/`%.3f` values matching hand-computed.
  - missing sample di-word → that row's `count sample` field is **empty** (`AA\t\t0.00\t<g>\t<gp>\t0.000`).
  - `%.2f` rounding parity: assert specific values that exercise round-half-to-even (e.g. a count ratio giving `…X5` at the 3rd dp) — cross-checked against Perl `sprintf` output captured in a comment.
  - zero mono total (empty sample) → `Err(ZeroDivision)`; zero genomic count for a word → `Err(ZeroDivision)`.
- GREEN: `pub fn write_stats(out:&mut impl Write, sample:&NucCounts, genomic:&NucCounts) -> Result<(),BismarkBam2nucError>`:
  - write header `(di-)nucleotide\tcount sample\tpercent sample\tcount genomic\tpercent genomic\tcoverage\n`.
  - mono group `[b"A",b"C",b"G",b"T"]`; di group the fixed 16 in SPEC order.
  - per group: `total_sample = Σ sample.mono(w)/di(w)`; `total_genomic` likewise; if either total `==0` → `Err(ZeroDivision)`.
  - per word: `cs_n = sample.mono(b)` (or `di(a,b)`). **Empty-field (C-3, the count==0⇔absent invariant from B1):** the printed `count sample` field is `if cs_n==0 { String::new() } else { cs_n.to_string() }` (empty when never-seen — Perl's undef→`""`); the math always uses `cs_n` (0 when absent — Perl's undef→0). `pct = format!("{:.2}", 100.0*cs_n as f64/total_sample as f64)`. genomic count `cg_n`; if `cg_n==0` (coverage denominator) → `Err(ZeroDivision)`. `cov = format!("{:.3}", cs_n as f64/cg_n as f64)`. Write the 6-field row. **O-1:** on `ZeroDivision` the Rust port may have already written the header + some rows (parity with Perl dying mid-`calculate_averages`); it does NOT clean up the partial stats file — document, matching c2c.
- VERIFY: `cargo test -p bismark-bam2nuc report`.

> §8.3 rounding note: Rust `{:.2}`/`{:.3}` use round-half-to-even (matches glibc printf). The D2 parity test pins it locally; **F1 confirms it on the oxy target platform** (the platform-can't-adjudicate lesson from genome-prep).

---

## Phase E — CLI wiring + `run()` + local Perl-oracle goldens

### Task E1 — `run()` top-level flow + input format gate + `main` wiring
**Files:** `src/lib.rs`, `src/main.rs`, `src/error.rs` (+`SamNotSupported`,`CramNotSupported`).

- RED: `assert_cmd` integration tests (`tests/cli_e2e.rs`):
  - `--genomic_composition_only -g <tiny_genome>` → writes `genomic_nucleotide_frequencies.txt`, exit 0, no stats file.
  - normal run on tiny SE `.bam` → writes `<sample>.nucleotide_stats.txt` to `--dir`.
  - `.sam` input → exit 1, stderr mentions BAM/CRAM. `.cram` input → exit 1, stderr mentions "not supported (v1.x)".
  - two `.bam` inputs → two stats files; cache written once, reused for the second.
- GREEN: `run(config)`:
  ```
  genome = Genome::load(&config.genome_folder)?
  if config.genomic_composition_only {
      get_genomic_frequencies(&genome, &config.genome_folder, &config.output_dir)?;  // computes+writes (or reuses)
      return Ok(());
  }
  for infile in &config.inputs {
      // input format gate
      match AlignmentKind::from_path(infile)? { Bam => {}, Sam => return Err(SamNotSupported), Cram => return Err(CramNotSupported) }
      let genomic = get_genomic_frequencies(&genome, &config.genome_folder, &config.output_dir)?;  // reuse after file #1
      let (sample, _stats) = count_reads_in_file(infile)?;
      let name = derive_output_name(infile)?;
      let path = format!("{}{}", config.output_dir, name);
      let mut out = BufWriter::new(File::create(&path)?);
      write_stats(&mut out, &sample, &genomic)?;
  }
  Ok(())
  ```
  Add error variants `SamNotSupported` (msg: BAM/CRAM only; clearer than Perl's), `CramNotSupported` (msg: deferred to v1.x). main.rs already wired in A1.
- VERIFY: `cargo test -p bismark-bam2nuc`.

> Ordering subtlety to honour: Perl derives the output name (and can `die NotBamOrCram`) AFTER counting. The Rust gate rejects `.sam`/`.cram` BEFORE counting (cheaper, same observable outcome since a `.bam` always derives cleanly). Document this as a benign reordering.

### Task E2 — Local goldens: `generate_goldens.sh` + committed fixtures + byte-compare tests
**Files:** `tests/data/generate_goldens.sh` (new), `tests/data/**` (fixtures + goldens, committed), `tests/golden.rs` (new).

- RED: `tests/golden.rs` reads each committed golden and asserts the Rust binary's output is byte-identical. Cells:
  1. `--genomic_composition_only` → cache golden (ACGTN genome). **Run against a CLEAN genome_folder (no pre-existing cache)** so it exercises the compute+write path (B/#6).
  2. **IUPAC-genome cache golden** (the SOLE guard for the count-everything-not-just-ACGT rule — V-4/B). Position the IUPAC base so it yields BOTH a mono row (`R`) AND di rows (e.g. genome `…ACRGT…` → mono `R`, di `CR` and `RG`).
  3. SE `.bam` → stats golden.
  4. PE `.bam` → stats golden (canonical 99/147/83/163).
  5. **Non-canonical-PE-flag cell (V-1/A):** a PE `.bam` with a flag ∉ {99,147,83,163} (e.g. a secondary/supplementary read) run through BOTH Perl and Rust — proves the `or 163` latent bug end-to-end (it revcomps, never dies), not just at the unit level.
  6. **Empty-count-field cell (C-3, MANDATORY — both reviewers):** a tiny SE `.bam` whose sample is genuinely missing ≥1 of the 16 di-words (easy: a 1-read 4-bp span has only 3 di-words). Assert the EXACT bytes including the empty `\t\t` field (e.g. a literal `AA\t\t0.00\t…` line). This logic is **unreachable on the oxy gate**, so this local golden is its only guard against printing `0` instead of `""`.
  7. **All-InDel-skipped cell (I-4/B):** a `.bam` where every read has an InDel CIGAR → sample mono total 0 → Rust exits 1 (`ZeroDivision`) and Perl dies — assert the exit code (end-to-end, not just the D2 unit test).
  8. cache-reuse: pre-plant a cache with **obviously-synthetic values (e.g. ×1000)** that could not arise from the tiny genome, run SE, assert the stats `count genomic` column shows the planted values (a recompute-instead-of-reuse bug visibly fails — I-5/B).
  9. **Two-input cell (I-5/B):** two `.bam` inputs with **different** sample content → two stats files; cache written once (file 1) + reused (file 2). Assert file 2's `count sample` ≠ file 1's (a `%freqs`-not-reset bug fails).
  10. InDel-skip cell (a single read skipped among others; counts reflect it).
  11. chr-end truncation cell.
  12. **Empty-genome `--genomic_composition_only` cell (O-3/A):** a Mus-only genome dir → a 0-byte `genomic_nucleotide_frequencies.txt` (byte-correct by construction).
  13. **Content-BAM-named-`.sam` cell (O-6/A):** assert both Perl and Rust error (no stats file) — Rust via `derive_output_name`/gate, Perl via name-derivation `die`.
- GREEN: write `generate_goldens.sh` (mirrors c2c): builds the tiny genome(s), writes SAM → `samtools view -bS` → `.bam` (SE `@PG ID:Bismark` SE CL; PE `@PG` with `-1 … -2 …`), runs local Perl `./bam2nuc --genome_folder … --samtools_path /opt/homebrew/bin/samtools …` and `--genomic_composition_only`, copies outputs to `tests/data/*.golden`. Commit fixtures + goldens. Run the script once; tests then run hermetically (no Perl/samtools needed in CI — they compare against committed goldens).
- VERIFY: `cargo test -p bismark-bam2nuc --test golden`.

> Provenance: every golden must be regenerable by `generate_goldens.sh` (re-running reproduces byte-for-byte). Record the samtools + Perl versions in a header comment.

---

## Phase F — oxy real-data byte-identity gate + docs

### Task F1 — Real-data gate harness (oxy)
**Files:** `tests/byte_identity_real_data.rs` (`#[ignore]`), `scripts/bam2nuc_byte_identity.sh` (new).

- Harness (fail-CLOSED, mirrors c2c/bedgraph scripts): on oxy, with `LC_ALL=C` (a CHECKED invariant — I-2, both reviewers: Perl `sort keys` cache order is byte-correct only under `LC_ALL=C`), Perl v0.25.1 from `~/micromamba/envs/bismark-test/bin` (PATH-prepend), genome `~/bismark_benchmarks/genome`, BAMs under `~/bismark_benchmarks/`. Cells: ≥1 SE BAM, ≥1 PE BAM, `--genomic_composition_only`, cache-reuse, **+ one `samtools sort`/`view`-reprocessed BAM** (I-3/B: prepends a non-Bismark `@PG`, exercising the `detect_paired_from_header` ID:Bismark-scoped vs Perl first-`@PG` divergence — OR, if that BAM diverges, document the gate uses raw Bismark output only and record the expected SE/PE call for both tools). For each: run Perl + Rust into **distinct out-dirs**, `diff` both `*.nucleotide_stats.txt` and `genomic_nucleotide_frequencies.txt` byte-for-byte; gated exit codes; cross-cell differentials (SE vs PE counts differ). **Confirms the `%.2f`/`%.3f` rounding contract on the Linux target platform** (same-platform Rust-vs-Perl diff: any libc rounding shift affects both identically, so byte-identity holds unless Rust-core and oxy-libc disagree on a tie — both reviewers found no such case on macOS).
- Don't disturb a running extractor `fulldata_bench` tmux; use the idle gate. Build in an isolated worktree on oxy (rustup toolchain); leave the main checkout on iron-chancellor for parallel sessions.
- VERIFY (manual, on oxy): `bash scripts/bam2nuc_byte_identity.sh` → all cells PASS (exit 0).

### Task F2 — Docs
**Files:** `rust/bismark-bam2nuc/README.md`, `CHANGELOG.md`, de-jargoned `--help` text.

- Per sibling convention (Rust ports are NOT in the public mkdocs nav). README: what it does (genomic-seq composition QC), usage, byte-identity note, deviations (D1–D6). CHANGELOG: `1.0.0-alpha.1` initial port. `--help`: plain-language option descriptions.
- VERIFY: `cargo build` (README/CHANGELOG are docs; `--help` snapshot optional).

---

## Final verification

```bash
cd ~/Github/Bismark-bam2nuc/rust
cargo fmt -p bismark-bam2nuc
cargo clippy -p bismark-bam2nuc --all-targets -- -D warnings
cargo test -p bismark-bam2nuc            # all unit + golden integration green
cargo test --workspace                   # no regressions in siblings
# then, on oxy (manual gate):
LC_ALL=C bash scripts/bam2nuc_byte_identity.sh
```

## Commit plan

One commit per phase on `rust/bam2nuc` (base `rust/iron-chancellor`), each dual-code-reviewed + plan-manager-verified before the next:
- A: `feat(bam2nuc): crate scaffold + CLI + genome reader`
- B: `feat(bam2nuc): process_sequence counter + genomic composition cache`
- C: `feat(bam2nuc): per-read genomic-sequence counting`
- D: `feat(bam2nuc): nucleotide_stats report + output filename`
- E: `feat(bam2nuc): run() wiring + local byte-identity goldens`
- F: `docs(bam2nuc): README/CHANGELOG + oxy real-data gate harness`

Final PR base = `rust/iron-chancellor` (NOT master). Title references the epic (TBD — create a `bam2nuc` epic issue on the "Bismark Rust rewrite" board, sibling to #797/#798/#891).

---

## Parallel-stream note

Single sequential stream (NOT split into `IMPL_stream-*`): phases share `error.rs` (grows each phase) and `lib.rs` (wiring), and the byte-identity gate requires sequential build-up — per the skill's "do NOT split when tasks share a source file." Within a phase, tasks are ordered validation → core → edge → integration.

## Open items — RESOLVED by dual plan review (2026-05-31)

- **OI-1 (C5): RESOLVED → direct `noodles_bam::io::Reader::record_bufs`.** Both reviewers confirmed `bismark-io` has no tag-agnostic passthrough (its BAM API forces XR/XG/XM + drops unmapped), and that raw `record_bufs` does NOT filter unmapped / sort-check — so the SE faithful-`die` requirement is met. Requires the C-1 dep additions.
- **OI-2 (D2): RESOLVED → hard requirement.** Both reviewers verified live that Perl emits an EMPTY count field (not `0`) for an absent word. Promoted to the mandatory E2 cell #6 (literal `\t\t`), since it's unreachable on the oxy gate. The array `count==0 ⇔ absent` invariant (B1) implements it cleanly.
- **OI-3 (F1): still to confirm operationally (not a plan blocker).** The exact oxy SE/PE BAM paths under `~/bismark_benchmarks/` and the reuse-vs-regenerate cache choice are resolved at gate-run time (Phase F is a manual, post-implementation step). Plan-level guidance: run cell #1 against a clean genome dir (compute+write) and a separate cell with a pre-planted Perl cache (reuse) to cover both precedence branches.

---

## Implementation notes (2026-05-31)

**All phases A–F implemented + green** on branch `rust/bam2nuc`. `cargo test -p bismark-bam2nuc` = **70 unit + 12 golden + 2 sanity** pass, **1 `#[ignore]` real-data smoke** (skipped), 0 doctest failures; `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt` clean; `cargo check --workspace` green (additive new member, no sibling changes). Modules: `cli` `error` `genome` `freqs` `count` `output_name` `report` + `lib`/`main`.

**Byte-identity CONFIRMED LOCALLY vs Perl v0.25.1** across 12 golden cells generated by the real Perl `bam2nuc` + samtools (`tests/data/generate_goldens.sh`): cache (ACGTN / IUPAC R-rows / empty-genome 0-byte), SE stats + computed cache, PE stats (canonical flags), **non-canonical PE flag (proves the `or 163` revcomp bug)**, cache-reuse (planted ×1000 genomic column), two-file (per-file `%freqs` reset + cache reuse), all-InDel→`ZeroDivision` exit 1 (Perl dies 255), SAM/CRAM reject, missing-genome.

**Deviations from PLAN rev 1 (documented):**
- **D-impl-1:** cache serializer is `NucCounts::cache_bytes() -> Vec<u8>` (raw, byte-exact) rather than the PLAN's `to_cache_string()` — handles a hypothetical non-ASCII genome byte without panicking; tests assert on `String::from_utf8` of the bytes.
- **D-impl-2:** added an `error::BamIo(#[from] bismark_io::BismarkIoError)` variant (not in the PLAN's enum list) — required for the `AlignmentKind::from_path` format-sniff gate in `run()`.
- **D-impl-3 (load-bearing test discovery):** the golden GENOME fixture **must contain all 16 di-words** (chr1 = de Bruijn B(4,2) `AACAGATCCGCTGGTTA`), because Perl `bam2nuc` ITSELF dies "Illegal division by zero" computing `coverage = freqs/genomic_freqs` for any di-word absent from the genome. Found while generating goldens; the Rust port matches Perl (errors on the same degenerate input).
- **D-impl-4:** the `report.rs` module-doc example uses a ` ```text ` fence with literal `<TAB>` markers (no real tab chars, no 4-space indent) to satisfy clippy `tabs_in_doc_comments` AND avoid rustdoc compiling it as a doctest.
- `count_records` is factored out of `count_reads_in_file` so the per-record loop is unit-tested with synthetic `RecordBuf`s; the thin BAM-open wrapper is exercised end-to-end by the golden tests.

**PENDING (manual, post-review):** the **oxy real-data byte-identity gate RUN**. Harness is built + committed (`scripts/bam2nuc_byte_identity.sh` fail-closed driver + `tests/byte_identity_real_data.rs` `#[ignore]` smoke); the actual oxy execution (`dcli ssh oxy`, build via rustup, run the script over real hg38 + real Bismark SE/PE BAMs) is the final confirmation and is not yet done. Confirms OI-3 + the `%.Nf` rounding contract on the Linux target.

**Dual code-review + plan-manager DONE (2026-05-31):** plan-manager **COVERAGE = COMPLETE** (0 unresolved; 43/43 checklist rows + 18 tasks DONE; suite re-run green). Both code-reviewers **APPROVE, no Critical/High**; reviewer B brute-forced `%.2f`/`%.3f` (43,100 ratio cases, 0 Rust-vs-Perl mismatches). The one shared finding (A1 = B's M-1): `extract_span` with a `ref_id=Some` + `POS=0`/`None`-alignment_start record returned the chromosome FRONT (comment wrongly said "empty"); byte-neutral (unreachable on real Bismark BAMs). Reports: `COVERAGE.md`, `CODE_REVIEW_A.md`, `CODE_REVIEW_B.md`.

**Post-review FIX #1 applied (Felix-approved 2026-05-31):** `extract_span` now returns an empty span for `pos1 == 0` (+ corrected comment + regression test `count_records_none_alignment_start_contributes_nothing` + an `extract_span` POS=0 assertion). Chosen over replicating Perl's `substr(-1)` last-base accident (confirmed via `perl -e`: `substr("ACGTACGT",-1,4)=="T"`); documented as a robustness divergence. Re-verified: 71 unit + 12 golden + 2 sanity green, clippy `-D warnings` clean, byte-identity goldens unchanged.

**Deferred (Felix's call, NOT applied):** A2/D8 (document the non-ASCII `@SQ` hard-error divergence); test polish L-2 (strengthen CRAM-reject assertion) + T-1 (content-BAM-named-`.sam` golden cell); Low items A3 (`--1`/`--2` acceptance), L-3 (per-read allocation micro-opt).

**OXY REAL-DATA GATE: PASSED (2026-05-31).** Source tar-piped to oxy `/tmp/bam2nuc-gate` (no commit/push), release binary built (15.6s). Ran `scripts/bam2nuc_byte_identity.sh` over the **full hg38 genome** + real **10M SE** (`directional_10M_R1_val_1_bismark_bt2.bam`) + **10M PE** (`..._pe.bam`) Bismark BAMs, Perl v0.25.1 + samtools from the `bismark-test` micromamba env. **3/3 core cells byte-identical** (`genome_comp` cache 255 B; `se` stats 851 B + cache; `pe` stats 852 B + cache). **Confirms the `%.Nf` rounding contract on the Linux target.** **`sorted` cell also run + PASSED** (851 B, byte-identical): a `samtools sort`-reprocessed SE BAM (`SO:coordinate`) — confirmed the `@PG` chain has `ID:Bismark` FIRST with samtools' `@PG` appended after (`PP:Bismark`), so SE/PE detection does NOT diverge, and the raw `record_bufs` reader accepts coordinate-sorted input. (Pre-seeded the already-proven genomic cache so both tools reuse it, isolating the sorted-BAM read.) OI-3 resolved (BAM paths above; genome had no pre-existing cache → both tools computed fresh). Post-fold-in/fix-#1/doc-polish test counts: **71 unit + 13 golden + 2 sanity**.

**STILL NOT committed** — awaiting Felix's direction on commit / PR (base `rust/iron-chancellor`). oxy `/tmp/bam2nuc-gate` (source + gate.log) left in place; the 3 GB genome copy was auto-purged by the script's trap.
