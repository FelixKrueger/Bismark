# SPEC — `bismark-bam2nuc` (Rust port of Perl `bam2nuc`)

**Status:** rev 1 — design questions Q1–Q9 RESOLVED (Felix, 2026-05-31). Awaiting go-ahead to draft the PLAN (do NOT plan/implement yet).
**Date:** 2026-05-31

**Resolved decisions (Felix, 2026-05-31):** Q1 = **raw `noodles::RecordBuf`** (tag-agnostic, faithful) · Q2 = **reject `.sam`** (replicate Perl, clearer error) · Q3 = **defer CRAM to v1.x** (reject in v1.0) · Q6 = **hard error on division-by-zero** (match Perl's crash). Lower-stakes defaults confirmed: Q4 `--samtools_path` = accepted-but-ignored no-op · Q5 `ID:Bismark`-scoped `@PG` match = equivalent · Q7 binary = `bam2nuc_rs` · Q8 = mimalloc global allocator · Q9 = oxy real-data gate (SE + PE + `--genomic_composition_only`).
**Branch / worktree:** `rust/bam2nuc` @ `~/Github/Bismark-bam2nuc` (off `origin/rust/iron-chancellor`, HEAD `2c8f15f`).
**Perl source:** `./bam2nuc` v0.25.1, ~610 lines (read in full).
**Sibling crates cribbed:** `bismark-coverage2cytosine` (genome reader, CLI/error/`run()` scaffolding, sprintf byte-identity), `bismark-extractor` (BAM read via `bismark-io`, chr-name table, mimalloc), `bismark-io` (BAM/SAM/CRAM reader, `detect_paired_from_header`).

---

## 1. Purpose & scope

`bam2nuc` computes **mono- and di-nucleotide coverage** of a Bismark alignment as a QC metric: for every read it tallies the **genomic** sequence at the read's mapped span (NOT the read's own bases), then compares the read-derived composition against the whole-genome composition. The output (`*.nucleotide_stats.txt`) is picked up and plotted by `bismark2report`.

It is invoked by the main `bismark` pipeline via `--nucleotide_coverage`.

**v1.0 scope (RESOLVED):**
- Normal mode: **BAM** → `<sample>.nucleotide_stats.txt`. `.sam` and `.cram` inputs → **rejected with a clear error** (Q2 reject SAM; Q3 defer CRAM to v1.x).
- `--genomic_composition_only` mode: compute + write genome composition, then exit.
- Genome-composition cache (`genomic_nucleotide_frequencies.txt`) read/reuse + write with the same precedence as Perl.
- Single-end and paired-end (auto-detected from `@PG`), exactly per Perl's flag logic.
- Records read via **raw `noodles::RecordBuf`** (Q1) — tag-agnostic; we manage unmapped-read handling ourselves (see §7.2/§11).

**The acceptance gate is byte-identity** of BOTH output files vs Perl v0.25.1 (§10).

---

## 2. Binary & crate

- New crate `rust/bismark-bam2nuc` added to the `rust/` workspace `members`.
- Binary name **`bam2nuc_rs`** (sibling convention: `coverage2cytosine_rs`, `bismark2bedGraph_rs`, `bismark_genome_preparation_rs`). **Confirm in §13 Q7.**
- Library + thin `main.rs` returning `ExitCode` (exit `0` success / `1` any `BismarkBam2nucError` / `2` clap parse error), per c2c.
- Deps (mirror siblings, exact `=` pins from the workspace lock):
  - `bismark-io` (path dep) — BAM/SAM/CRAM reading + `detect_paired_from_header`.
  - `clap` (derive), `thiserror`.
  - `noodles-fasta` + `flate2` for the genome reader (mirror c2c `genome.rs` pins).
  - `noodles-sam` for `Header` reference-sequence-name table.
  - `mimalloc` global allocator (sibling precedent #884/#915 — free throughput on the per-read loop, byte-identity-neutral). **Confirm in §13 Q8.**

---

## 3. CLI surface

Perl `process_commandline` (`bam2nuc:346-465`) options:

| Perl option | Type | Rust mapping | Notes |
|---|---|---|---|
| `--help` | flag | clap help | not byte-gated |
| `--dir=s` | string | `--dir <PATH>` → `output_dir` | output dir; default `""` (cwd-relative prefix), trailing-slash normalised |
| `-g`, `--genome_folder=s` | string | `-g`/`--genome_folder <PATH>` | **mandatory**, dies if absent; trailing-slash normalised |
| `--parent_dir=s` | string | `--parent_dir <PATH>` | default `getcwd()`; trailing-slash normalised |
| `--samtools_path=s` | string | `--samtools_path <PATH>` | **accepted-but-ignored no-op** (we use pure-Rust `bismark-io`, no samtools subprocess). See §12 D1. **Confirm §13 Q4.** |
| `--genomic_composition_only` | flag | `--genomic_composition_only` | compute+write genome composition then exit. NOTE: the internal var is `$genome_freq_only`; the **CLI flag name is `--genomic_composition_only`** (the task brief's `--genome_freq_only` is not the actual flag). |
| `--version` | flag | `-V`/`--version` | sibling one-liner `version_string()`, NOT Perl's banner (D2) |
| *(positional)* | `@ARGV` | `[INPUT...]` | one or more input alignment files; each produces its own stats file |

There is **no `-o`/`--output`** — the output filename is **derived** from the input filename (§9).

**Argument-count rules (Perl `:393-401`):** if no input files AND not `--genomic_composition_only` → print help + exit. `--genomic_composition_only` does not require input files.

---

## 4. Modes & top-level flow

Mirrors Perl `bam2nuc:36-61`:

```
parse + resolve CLI
read_genome_into_memory(parent_dir/genome_folder)   # §5
if --genomic_composition_only:
    get_genomic_frequencies()                        # §6 (compute+write OR reuse), then exit 0
else:
    for each input file in argv order:               # §7
        get_genomic_frequencies()                    # reuse cache if present (written by file #1)
        detect SE/PE from @PG                         # §7.1
        per-read loop → tally read mono/di            # §7.2-7.4
        write <sample>.nucleotide_stats.txt           # §8-9
```

`%freqs` (read composition) is **reset per input file**; `%genomic_freqs` is recomputed/reloaded per input file but is deterministic (cache reused after file #1).

---

## 5. Genome reading

**Reuse the c2c `genome.rs` pattern verbatim** (it is a port of the identical Perl `read_genome_into_memory` + `extract_chromosome_name`; `bam2nuc:474-575` is line-for-line equivalent to `coverage2cytosine`'s). Write a fresh `genome.rs` in this crate (no cross-crate dep on c2c).

Behaviours reproduced:
- **Four-suffix glob priority** `*.fa` → `*.fa.gz` → `*.fasta` → `*.fasta.gz`; first tier with ≥1 file wins; no FASTA anywhere → error (Perl `:495-497`).
- **Skip `Mus_musculus.NCBIM37.fa`** (Perl `:502`).
- **Uppercase on load**; strip `\r` (CRLF safety).
- Chromosome name = first whitespace token after `>`.
- **Duplicate name → error** (Perl `:526-529`, `:548-551`).
- Plain `.fa`/`.fasta` + gzipped via `MultiGzDecoder` (handles plain gzip + BGZF-framed).

**Difference from c2c's genome usage:** bam2nuc needs (a) `get(chr) -> Option<&[u8]>` to extract read spans, and (b) iteration over **all** chromosome sequences (any order) to compute genomic composition. Genomic counting is commutative, so a `HashMap` + an internal `values()`/`iter()` accessor is fine (no ordering invariant — unlike c2c, which deliberately hid insertion order). Add a `seqs()` or `iter()` accessor here.

**Documented divergence (same as c2c):** a bare `>` / nameless header → Rust errors (`MalformedFastaHeader`); Perl stores an empty-name chromosome. Cannot occur on a Bowtie2-built Bismark genome.

---

## 6. Genomic composition + cache (`genomic_nucleotide_frequencies.txt`)

Perl `get_genomic_frequencies` (`:160-216`). **The cache file is part of the byte-identity gate (§10).**

### 6.1 Precedence (exactly Perl)
1. **If `${genome_folder}genomic_nucleotide_frequencies.txt` exists** → read it line-by-line (`element\tfreq`), populate `genomic_freqs`. **Do NOT recompute.** (This means: in the gate, if Perl already wrote the cache, the Rust run reuses it byte-for-byte; counts are then identical by construction.)
2. **Else** → compute composition over all chromosomes (`process_sequence` on each full + strand sequence; §6.3), then **attempt to write** the cache:
   - First try `${genome_folder}genomic_nucleotide_frequencies.txt`.
   - On failure (e.g. read-only genome dir) → try `${output_dir}genomic_nucleotide_frequencies.txt`.
   - On failure of both → skip writing (warn only); continue.
   - **Existence check is only ever against `genome_folder`** — so if the cache landed in `output_dir`, a subsequent input file recomputes (Perl quirk; replicate, harmless to byte-identity since counts are identical).

### 6.2 Cache file format (byte-identity)
```
foreach my $f (sort keys %genomic_freqs) { print "$f\t$genomic_freqs{$f}\n"; }
```
- **Sorted by Perl string sort = bytewise** on the word bytes. For a pure-ACGTN genome this is exactly 20 lines in this order:
  `A, AA, AC, AG, AT, C, CA, CC, CG, CT, G, GA, GC, GG, GT, T, TA, TC, TG, TT`
  (1-char word sorts before its 2-char extensions because a prefix sorts first.)
- Each line: `<word>\t<count>\n`. Trailing newline on the last line. No header.
- **Count ALL words, not just ACGT** (§6.3) — a genome containing IUPAC ambiguity codes (R/Y/…) would add extra sorted lines (e.g. `R`, `AR`, `RA`). To match Perl byte-for-byte the counter must NOT pre-restrict to ACGT; the ACGT restriction applies only to the **stats** output (§8), never to the cache.

### 6.3 `process_sequence` (the counter) — Perl `:318-343`
For a sequence `seq` of length `L`:
- **Mono:** for each index `i ∈ [0, L)`: `m = seq[i]`; `if m != 'N' { freqs[m] += 1 }`.
- **Di:** for each `i` where `i+2 <= L`: `d = seq[i..i+2]`; `if !d.contains('N') { freqs[d] += 1 }` (overlapping windows; `L-1` windows total).
- N-handling: any mono `== 'N'` skipped; any di containing `N` skipped. Non-ACGTN bytes (IUPAC) are **counted** (they are not `N`).
- The genome pass runs `process_sequence` on each **+ strand** chromosome sequence as-is (no reverse-complement). Di windows never cross chromosome boundaries (each chr processed separately).

---

## 7. Per-read counting

### 7.1 SE/PE detection — Perl `test_file` (`:255-273`)
Perl scans `samtools view -H` for the **first `@PG`** line and checks its command line for `\s-1\s+` AND `\s+-2\s` → PE, else SE; if no `@PG` at all → both false → `die "Failed to figure out SE or PE"`.

**Rust:** use `bismark_io::detect_paired_from_header(header)` → `Some(true)`=PE, `Some(false)`=SE, `None`=error out. **Minor divergence to confirm (§13 Q5):** the Rust helper matches the first `@PG` with `ID:Bismark` (Perl matches the first `@PG` regardless of ID). Equivalent for real Bismark BAMs (single Bismark `@PG`).

### 7.2 Reading records
- Open via a path-dispatching reader (`bismark-io`). Use the **`without_sort_check` constructors** — bam2nuc counts each read independently (no pairing), so coordinate-sorted input is fine and Perl never checks sort order.
- For each record, extract: **FLAG**, **chr name** (via a `reference_sequence_id → name` table built from the header, mirroring `extractor::header::build_chr_name_table`), **alignment_start** (1-based POS), **CIGAR**, **read length** (= `len(SEQ)`).
- **Open design fork (§13 Q1):** whether to go through `bismark-io::BismarkRecord` (requires `XR/XG/XM`, validates `XM.len()==SEQ.len()`, silently drops unmapped reads) or read the raw `noodles` `RecordBuf` directly (tag-agnostic, faithful to Perl's field-only extraction). bam2nuc uses none of those tags; raw `RecordBuf` is the more faithful path. See §11 for the divergences each choice implies.

### 7.3 CIGAR `[IDSN]` exclusion — Perl `:126-130`
`if ($cigar =~ /[IDSN]/) { ++$skipped; next; }`
- Skip the read if its CIGAR contains **any** Insertion (`I`), Deletion (`D`), SoftClip (`S`), or RefSkip (`N`) op.
- Structured-op equivalent (noodles): skip if any op kind ∈ {`Insertion`, `Deletion`, `SoftClip`, `Skip`}.
- **Not** skipped: `M`, `=`, `X`, `H` (hard clip), `P` (padding). Bismark/Bowtie2 emits only `M` (+ I/D/S), so `=`/`X`/`H`/`P` are non-Bismark edge cases; the structured check matches the regex exactly regardless.

### 7.4 Genomic span extraction — Perl `:133`
`extracted = substr(chromosomes{chr}, start-1, len(SEQ))`
- 0-based start = `POS - 1`; length = `len(SEQ)` (the read's sequence length, NOT the CIGAR reference span — equal here because no I/D/S/N).
- **Perl `substr` saturation semantics MUST be replicated (no panic):**
  - chr **absent** from genome → Perl `substr(undef,…)` → empty string → read contributes nothing (Perl also emits an "uninitialized value" warning; we just contribute nothing).
  - `start-1 >= chr_len` → empty.
  - `start-1 + len > chr_len` → truncated slice `chr[start-1 ..]` (fewer than `len` bytes).
  - Rust: `let s = &chr[min(p, n) .. min(p+len, n)]` where `p = start-1`, `n = chr_len`.

### 7.5 Strand correction — Perl `calc_single_end` / `calc_paired_end` (`:219-252`)
Applied to the **extracted genomic span** before counting:

**SE (`calc_single_end`):**
- `flag == 0` → forward (count as-is).
- `flag == 16` → reverse-complement: `reverse(seq)` then `tr/GATC/CTAG/` (G→C,A→T,T→A,C→G; N and others unchanged).
- **else → error/exit** (Perl `die "failed to detect valid Bismark FLAG tag: $flag"`). Reachable code.

**PE (`calc_paired_end`):**
- `flag == 99 || flag == 147` → forward.
- **else → reverse-complement.** ⚠️ **LATENT PERL BUG TO REPLICATE (§12 D3):** Perl's `elsif ($flag == 83 or 163)` always evaluates true (because `163` is a truthy constant), so the `else{die}` is **dead code** and *every* non-99/147 flag is reverse-complemented — not just 83/163. We replicate: PE forward iff `flag ∈ {99,147}`, else revcomp, **never error**.

Then `process_sequence(corrected_span)` accumulates into `%freqs` (§6.3) — the sample composition.

---

## 8. Output stats file (`*.nucleotide_stats.txt`)

Perl `calculate_averages` (`:276-315`). Written to `${output_dir}<derived_name>` (§9).

### 8.1 Exact format
- **Header line** (Perl `:284`, tab-separated, trailing `\n`):
  `(di-)nucleotide\tcount sample\tpercent sample\tcount genomic\tpercent genomic\tcoverage`
- **Mono rows** in the fixed order `A, C, G, T`, then **di rows** in the fixed order
  `AA, AC, AG, AT, CA, CC, CG, CT, GA, GC, GG, GT, TA, TC, TG, TT`.
- Per row: `<word>\t<count_sample>\t<pct_sample>\t<count_genomic>\t<pct_genomic>\t<coverage>\n`.

### 8.2 The math (separate totals for mono vs di)
- `mono_total_sample = Σ freqs[w] for w in {A,C,G,T}`; `mono_total_genomic = Σ genomic_freqs[w]`.
- `di_total_sample = Σ freqs[w] for the 16 di-words`; `di_total_genomic` likewise.
- `pct_sample = sprintf("%.2f", 100 * freqs[w] / total_sample)` (mono uses mono_total, di uses di_total).
- `pct_genomic = sprintf("%.2f", 100 * genomic_freqs[w] / total_genomic)`.
- `coverage = sprintf("%.3f", freqs[w] / genomic_freqs[w])`.

### 8.3 Byte-identity subtleties (CRITICAL — surface on tiny fixtures)
- **`%.2f` / `%.3f` rounding:** Rust `format!("{:.2}", x)` uses round-half-to-even, matching glibc `printf`. Property-test vs Perl/C across many values (no custom `%g` machinery needed — unlike bedGraph). **Verify on the target platform via the real-data gate.**
- **Missing sample count → empty field, 0 in math:** if `freqs[w]` was never incremented (a word absent from a tiny sample), Perl interpolates `$freqs{$word}` as the **empty string** in the count column, while the numeric divisions coerce undef → 0 (so `pct=0.00`, `coverage=0.000`). Replicate: store counts in a map; print `""` for an absent key but use `0` in the arithmetic. (On real data every word appears, so this only bites tiny goldens — but the local fixtures need it.)
- **Division by zero → Perl dies:** if a `*_total == 0` (empty/all-skipped sample) or `genomic_freqs[w] == 0` (degenerate genome), Perl throws "Illegal division by zero" and exits. Decide whether the Rust port mirrors this as a hard error or guards it (§13 Q6). Never occurs on real data.

---

## 9. Output filename derivation — Perl `:147-152`

```
out = basename(infile);                       # s/.*\///
die unless out =~ s/(bam|cram)$/nucleotide_stats.txt/;   # NB: no '.' anchor
write to "${output_dir}${out}";
```
- `sample.bam` → `sample.nucleotide_stats.txt`; `sample.cram` → `sample.nucleotide_stats.txt`.
- **Latent quirk (§12 D4):** the regex anchors on `(bam|cram)$` with **no preceding dot**, so a name ending in `…bam`/`…cram` without a dot would also match (e.g. `foosubbam` → `foosubnucleotide_stats.txt`); and a name not ending in `bam`/`cram` (notably **`.sam`**) makes the substitution fail → **Perl dies** "File needs to be in BAM or CRAM format". So Perl can *read* a `.sam` (plain open) but cannot *name* its output → effectively rejects SAM. Decide SAM handling in §13 Q2.

---

## 10. Byte-identity invariants (THE GATE)

Both files must match Perl v0.25.1 byte-for-byte:
1. **`*.nucleotide_stats.txt`** — header + 4 mono + 16 di rows; `%.2f`/`%.3f` formatting; mono-then-di fixed order; empty-count-field behaviour; tab/newline layout.
2. **`genomic_nucleotide_frequencies.txt`** — bytewise-sorted `word\tcount\n`; count-everything (incl. IUPAC); reused byte-for-byte when already present.

Determinism levers that make this hold:
- Counting is commutative ⇒ chromosome/read order is irrelevant to counts.
- The cache-reuse precedence guarantees the genomic column is identical between a Perl-then-Rust comparison.
- `%.Nf` rounding parity must be confirmed on the **target platform** (oxy) via the real-data gate — local macOS goldens are necessary but not sufficient for the rounding contract (lesson from the genome-prep glob-order saga: the dev platform cannot adjudicate a platform-specific contract).

---

## 11. Input-record design fork (the central open question — §13 Q1)

| Aspect | Via `bismark-io::BismarkRecord` | Via raw `noodles::RecordBuf` |
|---|---|---|
| `XR/XG/XM` required | **Yes** — errors if absent | No (faithful to Perl) |
| `XM.len()==SEQ.len()` check | Yes | No |
| Unmapped reads | **silently dropped** (FLAG&0x4) before counting | we choose (Perl would die on SE / revcomp garbage on PE) |
| Format detection / CRAM ref / threaded BGZF | reused for free | must wire ourselves |
| Faithful to Perl's "any aligned BAM" intent | partial | full |
| Sibling-consistency | high | lower |

**RESOLVED (Q1) → raw `RecordBuf`.** Faithful + tag-agnostic, still using `bismark-io`'s magic-byte format detection where it doesn't impose the tag contract. Open detail for the PLAN: pick the lowest-friction way to iterate `noodles` `RecordBuf`s without `BismarkRecord`'s validation — either a thin local reader over `noodles_bam`/`noodles_sam`, or (if `bismark-io` exposes a raw-record path) reuse it. **Unmapped-read policy:** Perl would `die` on an SE unmapped read (flag ∉ {0,16}) and revcomp-garbage a PE one; since Bismark BAMs contain no unmapped reads, either filtering them or letting the SE flag-check error is acceptable — the PLAN should pick the option that keeps the SE `die`-on-unexpected-flag semantics intact (do NOT silently drop, so the SE flag contract stays faithful). Confirm in code review against a fixture with a stray flag.

---

## 12. Documented deviations from Perl

- **D1 — no samtools subprocess.** Pure-Rust reading (`noodles-bam` directly + `bismark-io` format-sniff). `--samtools_path` accepted-but-ignored no-op (drop-in compat with the `bismark` pipeline). **D1a (plan-review O-5):** the Rust port does NOT validate the `--samtools_path` value — Perl `die`s on a non-existent path (`:436-453`); the Rust port ignores it. Byte-neutral (never reaches the gated output files).
- **D2 — `--version`** uses the sibling one-liner, not Perl's multi-line banner (not byte-gated).
- **D3 — PE flag bug replicated:** any PE flag ≠ 99/147 is reverse-complemented; no `die`. (Perl's `or 163` bug.)
- **D4 — output-name regex quirk replicated:** `(bam|cram)$` with no dot anchor AND **case-sensitive** (plan-review I-1: `.BAM` → error, like Perl); non-bam/cram name (incl. `.sam`) → error.
- **D5 — STDERR progress messages** are not byte-reproduced (only output files are gated); follow sibling `eprintln!` conventions.
- **D6 — bare/nameless FASTA header** → error (vs Perl's empty-name chromosome). Inherited from the c2c genome reader.
- **D7 — `*`-SEQ read (plan-review O-2):** a record with `SEQ == "*"` yields noodles `sequence().len() == 0` (→ empty span, 0 counts), whereas Perl's `length("*") == 1` would count 1 genomic base. **Unreachable on real Bismark BAMs** (which always carry a real SEQ with a linear CIGAR); documented for completeness.
- **D8 — non-ASCII `@SQ SN:` chromosome name (code-review A2):** `build_chr_name_table` hard-errors (`NonAsciiChromosomeName`) on a non-ASCII reference-sequence name; Perl does no such check (it uses the raw bytes as a hash key). Byte-neutral and **unreachable on ASCII Bismark genomes**; inherited from `bismark-extractor::header` (Bismark's downstream tools can't round-trip non-ASCII names safely).
- **D9 — `POS=0` / no-alignment-start record (code-review A1/M-1):** a record with a valid chromosome but no alignment start (`POS=0`) contributes nothing (empty span); Perl's `substr($seq, 0-1, len)` would index from the chromosome's END (returning the last base) via the negative offset. We deliberately do NOT replicate that accident. **Unreachable on real Bismark BAMs** (no positionless-but-mapped records).

---

## 13. Resolved decisions (Felix, 2026-05-31)

- **Q1 — Record source: RESOLVED → raw `noodles::RecordBuf`.** Tag-agnostic, faithful to Perl's field-only extraction. We do NOT route through `bismark-io::BismarkRecord` (which would require XR/XG/XM + silently drop unmapped). We still reuse `bismark-io`'s magic-byte format detection / reader open where it doesn't impose the tag contract. Unmapped-read handling: see §7.2/§11.
- **Q2 — SAM input: RESOLVED → reject `.sam`.** Replicate Perl (output-name derivation handles only trailing `bam`/`cram`); a `.sam` (or any non-bam/cram name) → hard error with a message clearer than Perl's. See §9.
- **Q3 — CRAM scope: RESOLVED → defer to v1.x.** v1.0 rejects `.cram` with a clear "not yet supported in the Rust port; use Perl bam2nuc / a future v1.x" error. No concatenated-reference indexing in v1.0.
- **Q4 — `--samtools_path`: RESOLVED → accepted-but-ignored no-op** (drop-in compat with the `bismark` pipeline). See §3 / §12 D1.
- **Q5 — SE/PE `@PG` match: RESOLVED → equivalent.** Use `bismark_io::detect_paired_from_header` (matches first `@PG` with `ID:Bismark`) as equivalent to Perl's first-`@PG` match.
- **Q6 — Division-by-zero: RESOLVED → hard error matching Perl.** Zero mono/di total (empty / all-skipped sample) or zero genomic count → `BismarkBam2nucError`, exit 1. Never occurs on real data. See §8.3.
- **Q7 — Binary name: RESOLVED → `bam2nuc_rs`** (`*_rs` sibling convention).
- **Q8 — mimalloc: RESOLVED → include** global allocator in v1.0 (byte-neutral per-read-loop throughput; sibling precedent #884/#915).
- **Q9 — Real-data gate: RESOLVED → oxy.** `dcli ssh oxy`; Perl v0.25.1 in `~/micromamba/envs/bismark-test/bin` (PATH-prepend, not `mamba activate`); genome `~/bismark_benchmarks/genome`; BAMs under `~/bismark_benchmarks/`. Cells: ≥1 SE BAM, ≥1 PE BAM, plus a `--genomic_composition_only` cell and a cache-reuse cell. Don't disturb the extractor `fulldata_bench` tmux if running — use the idle gate. Pin `LC_ALL=C`.

---

## 14. Test strategy (sketch — detail in PLAN)

- **Unit:** genome reader (reuse c2c's test battery), `process_sequence` mono/di + N-handling, CIGAR `[IDSN]` skip, substr saturation (chr-end / missing-chr / out-of-range), SE/PE flag correction incl. the PE `or 163` bug, `%.2f`/`%.3f` rounding (property test vs known C values), missing-count empty-field, cache read/write/precedence.
- **Golden (local, Perl-oracle):** the Perl `./bam2nuc` is self-contained and runs on macOS (like `./coverage2cytosine` did) ⇒ generate goldens locally with a `generate_goldens.sh`. Tiny synthetic genome + tiny SAM/BAM fixtures covering: SE fwd/rev, PE all four flags, InDel-skip, chr-end truncation, missing chr, cache reuse, `--genomic_composition_only`, IUPAC-in-genome cache lines.
- **Real-data byte-identity gate (oxy, Q9):** full genome + real Bismark SE & PE BAMs; diff both output files vs Perl v0.25.1; `--genomic_composition_only` cell; cache-reuse cell. Pin `LC_ALL=C`.

---

## 15. Module plan (sketch — detail in PLAN)

```
src/
  cli.rs       # clap Cli + validate() -> ResolvedConfig (Perl process_commandline)
  error.rs     # BismarkBam2nucError (thiserror; Perl-echoing messages)
  genome.rs    # Genome reader (port of c2c genome.rs + a seqs() accessor)
  freqs.rs     # process_sequence counter + genomic-composition cache (read/write/precedence)
  count.rs     # per-read loop: field extract, CIGAR skip, substr, SE/PE correction
  report.rs    # calculate_averages -> *.nucleotide_stats.txt (formatting, row order)
  output_name.rs # filename derivation (s/(bam|cram)$/.../)
  lib.rs       # run(): genome -> (genomic_composition_only? exit) -> per-file loop
  main.rs      # thin ExitCode wrapper + --version
```

Phasing (proposed): A scaffold/CLI/genome → B `process_sequence` + cache → C per-read counting → D report writer + filename → E CLI wiring + local goldens → F oxy real-data gate. Each phase: dual plan-review (done once up front) + dual code-review + plan-manager, per `~/.claude/CLAUDE.md`.
