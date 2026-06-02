# PLAN — Phase 5: Genomic-seq extraction + `XM`/`XR`/`XG` call + SAM/BAM output (SE directional)

> **Epic:** `05312026_bismark-aligner/EPIC.md`, Phase 5 — *Genomic-seq + XM/XR/XG + SAM/BAM (SE dir)* 🎯
> Depends on: **Phase 1** (`RunConfig`: genome FASTA paths, output dir/basename/prefix, `command_line`),
> **Phase 2** (`fix_id`, the original-read re-reader), **Phase 4** (`Decision::UniqueBest(BestAlignment)` +
> `Counters` — the per-read seam this phase consumes). **This is the FIRST byte-identity gate.**

---

## 0. Decision RESOLVED 2026-06-01 (Felix): **P1 — normalize the samtools `@PG` out of the gate**

**The samtools-pipe `@PG` policy (SPEC §3 refinement B / Phase-0 spike).** Perl writes its SAM through
`samtools view -bSh - > out.bam` (1607). The stored BAM header therefore carries **two** `@PG` lines:

1. Bismark's own, written by `generate_SAM_header` (8480):
   `@PG	ID:Bismark	VN:v0.25.1	CL:"bismark <command_line>"`
2. The line **samtools injects on the SAM→BAM pipe**, e.g.
   `@PG	ID:samtools	PP:Bismark	VN:1.23.1	CL:/abs/path/to/samtools view -bSh -`
   — which embeds an **environment-specific absolute samtools path** + samtools version.

The Rust port writes BAM with **noodles** (`bismark-io::BamWriter`), which does **not** inject a samtools
`@PG`. So `samtools view -H rust.bam` shows one `@PG`; `samtools view -H perl.bam` shows two. We must pick:

- **(P1) Normalize-out of the gate** *(plan's working assumption)* — strip all `@PG` lines (or just
  `@PG ID:samtools …`) from **both** sides before the header diff; gate the records + `@HD`/`@SQ`/`@RG`
  byte-for-byte and gate Bismark's own `@PG` after dropping only the samtools line. Robust to path/version
  drift; matches "the samtools line embeds an env-specific path we can't reproduce portably".
- **(P2) Reproduce best-effort** — fabricate a synthetic `@PG ID:samtools PP:Bismark VN:<detected> CL:<path>
  view -bSh -` line in the noodles header, with the samtools path/version taken from the same detection
  Phase 1 already does. Achieves a *fully* byte-identical `samtools view -H`, but bakes a brittle, env-specific
  string into our header forever.

**RESOLVED → P1.** Felix chose **P1** (2026-06-01): normalize the samtools `@PG` line out of the gate;
reproduce everything else exactly, including Bismark's own `@PG CL:"bismark <argv>"` line. The gate
(§9 #14) filters `@PG ID:samtools …` from **both** sides before the header diff; the noodles header carries
only Bismark's `@PG`. P2 (fabricate a synthetic samtools `@PG`) is **dropped** — no synthetic-samtools-`@PG`
builder is added.

---

## 1. Goal

Produce the **actual Bismark BAM** for the single-end directional spine for the first time, and make its
**decompressed content byte-identical** to Perl Bismark v0.25.1 (driving Bowtie 2 2.5.5). For every
`Decision::UniqueBest` from the Phase-4 merge, extract the matching genomic window, make the per-base
`XM` methylation call (+ `XR`/`XG` tags), assemble the full SAM record (FLAG, `NM`/`MD`, revcomp for the
minus strand), and write it via `bismark-io::BamWriter`. This is the port of everything Perl does between
`check_results_single_end:3123` and `:3147`, plus the genome loader, the SAM-header generator, and the
SAM-output helpers.

**Gate:** `samtools view -h` of the Rust BAM == `samtools view -h` of the Perl BAM (records byte-identical;
header byte-identical modulo the samtools `@PG` per §0) on a **small SE-directional WGBS dataset**, run on
**Linux** (oxy idle-gate or Linux CI), per the epic's "byte-identity adjudicated on Linux, never macOS"
rule. The full-scale oxy gate is Phase 10.

## 2. Context

### Placement — new modules in `rust/bismark-aligner/src/`

| Module | Ports (Perl) | Responsibility |
|---|---|---|
| `genome.rs` | `read_genome_into_memory` 5022–5147, `extract_chromosome_name` 5149 | Load the raw FASTA(s) into an in-memory `Genome { chromosomes, sq_order }`. **Defines the `@SQ` order** (byte-identity-critical). |
| `methylation.rs` | `extract_corresponding_genomic_sequence_single_end` 4273–4467, `methylation_call` 4800–5018, `reverse_complement` 5161 | Genomic-window extraction (CIGAR walk, edge guards, per-strand counters) + the `XM` call (CpG/CHG/CHH/unknown). |
| `output.rs` | `generate_SAM_header` 8452–8484, `single_end_SAM_output` 8489–8711, `make_mismatch_string` 9252–9595, `hemming_dist` 9235, `revcomp` 9228 | Build the noodles `Header` + per-read `RecordBuf` (FLAG, ref-seq trim, NM/MD/XM/XR/XG, minus-strand revcomp). |

The **driver** (`lib.rs`, extending Phase 4's `run_se_directional`/`drive_merge`) loads the genome once,
opens the writer, and for each `UniqueBest` runs extract → length-guard → call → assemble → write.

### Dependencies (new this phase)

- **`bismark-io = "=1.0.0-beta.8"`** (path `../bismark-io`) — reuse `BamWriter`/`SamWriter`/`open_writer`
  + `BismarkRecord`. `BamWriter::write_record(&BismarkRecord)` is the BAM write path; `finish()` writes the
  BGZF EOF marker (must be called — `#[must_use]`).
- **`noodles-sam = "=0.85.0"`, `noodles-core = "=0.20.0"`, `bstr = "=1.10.0"`** — to build `RecordBuf`
  (name/flags/refid/pos/mapq/cigar/sequence/quality_scores/data) and the `Header`. Pins **must match
  bismark-io's transitive choice** (same versions `bismark-dedup` declares) or the workspace won't link.

### The seam from Phase 4

`merge::Decision::UniqueBest(BestAlignment)` already carries everything extraction needs:
`{ chromosome (de-converted), position (1-based), index (0/1), alignment_score, alignment_score_second_best,
md_tag, cigar, bowtie_sequence, mapq }`. **`mapq` is already computed** (Phase 4, verbatim `calc_mapq`) —
Phase 5 reuses it directly (Perl recomputes it at 3134 *after* the length guard, but the value is identical
since its inputs are read-length + `AS` + 2nd-best, all fixed at merge time; reads that fail the guard are
never written, so the pre-computed value is simply unused for them). **`bowtie_sequence` (the converted read
Bowtie 2 reported) is NOT used for output** — both the methylation call (3144) and the SAM `SEQ` field (4196)
use the **original** uc read, which the Phase-4 driver already re-reads in lockstep.

### Perl control-flow anchors

- Genome loaded once, before the per-file loop, guarded `unless (%chromosomes)` (273–277).
- Mapped reads are written to `OUT` **inside** `check_results_single_end` (3147). Unmapped/ambiguous are
  **not** written to the BAM — they go to separate `--un`/`--ambiguous` files (2451–2465, **Phase 6**). So
  the Phase-5 BAM contains **only** `UniqueBest` reads that pass the length guard.
- Default output = **BAM** (`$bam=1`, 7544) via `samtools view -bSh` (1607); output name = input minus
  `(\.fastq\.gz|\.fq\.gz|\.fastq|\.fq)$` + `_bismark_bt2.sam` → `s/sam$/bam/` (1562–1607), under
  `$output_dir`, overridable by `--basename` (`${basename}.bam`) / `--prefix` (`$prefix.$name`).

## 3. Behavior (numbered) — mirrors Perl 3123–3147 + the helpers

### 3.1 Genome load (`genome::read_genome_into_memory`, 5022–5147) — once, at pipeline start

1. **🔴 Consume Phase 1's already-ordered FASTA list — do NOT re-glob** *(rev 1, both reviewers)*. Phase 1's
   `discovery::discover_fastas` already produced `config.genome.fastas: Vec<PathBuf>` "in byte-significant
   order (sets `@SQ` order, Phase 5)" via `FastaKind::PROBE_ORDER` (`.fa`→`.fa.gz`→`.fasta`→`.fasta.gz`,
   first non-empty group wins) + `fasta_name_cmp` (case-insensitive sort, case-sensitive `.gz`-disjoint
   match, `is_file()` symlink-follow). The loader **takes that ordered slice** and iterates it — so the
   byte-identity-critical `@SQ` order has **exactly one source of truth** (`discover_fastas`), and `sq_order`
   falls out of the iteration order for free. (Re-globbing the folder would duplicate the single most
   load-bearing surface and risk silent divergence — rejected.) `genome.fastas` empty is impossible here
   (`discover_fastas` already errored `NoFasta`).
2. Per file (gunzip if `.gz`, 5056): first line must be a FASTA header → `extract_chromosome_name` =
   `s/^>//` then **first whitespace-split token** (5149–5159; die if no `>`). **`extract_chromosome_name`
   returns `""` for a leading-space header** (`> chr1` → `split /\s+/` → `("", "chr1")`, Perl returns `""`) —
   it does **not** die. The **empty-name → die** check (5069/5098) lives in the **loader (caller)**, not in
   `extract_chromosome_name` (matches the genome-prep helper's non-dying contract; the aligner's loader adds
   the die on top).
3. Read sequence lines: `chomp` + strip `\r` (5075–5076), **uppercase** (`uc`, 5103), concatenate. A `^>`
   line starts a new chromosome: store the previous (die if the name already exists, 5080/5109; warn — not
   die — on empty sequence, 5085/5114), `++$SQ_count`, `$SQ_order{$SQ_count}=name`.
4. Result: `Genome { chromosomes: HashMap<String, Vec<u8>>, sq_order: Vec<String> }` (insertion order).
   `--cram`-reference reconstitution (5129–5143) is **out of v1 scope** (skip).

### 3.2 Per `UniqueBest` (driver, replacing the Phase-4 "tally only" body)

For each `Decision::UniqueBest(best)` with original uc read `sequence` + `quality`:

**(a) Extract genomic sequence** (`methylation::extract_corresponding_genomic_sequence_single_end`,
4273–4467):
- `pos = best.position - 1` (1-based → 0-based, 4300).
- `contains_deletion = cigar.contains('D')` (4280).
- Parse CIGAR into `(len, op)` runs (4303–4306; die on length/op count mismatch).
- `pbat_index_modifier = 0` for SE-directional (`+2` only for `--pbat`, Phase 8 — 4310).
- **If `index(+mod) ∈ {1,3}`** (read needs 2 genomic bases *prepended*): **guard** `pos-2 >= 0`, else store
  the partial sequence and **return early** (4317–4321); otherwise prepend `chr[pos-2 .. pos]` to
  `unmodified_genomic_sequence` **only** (4322). **🔴 The +2 prepend/append bases are NEVER added to
  `genomic_seq_for_md_tag`** *(rev 1, Reviewer A)* — that second sequence receives bases only inside the
  `M`/`D` arms and only when `contains_deletion`. Do not reflexively mirror the `M`-arm's MD-seq append here.
- **CIGAR walk** (4327–4385): `M` → append `chr[pos .. pos+len]`, `pos += len` (+ append to
  `genomic_seq_for_md_tag` iff `contains_deletion`); `I`/`S` → append `'X' * len` (no `pos` change, **no
  `indels`**); `D` → `pos += len`, `indels += len` (+ genomic to MD-seq iff `contains_deletion`); `N` →
  `pos += len` (**no `indels`**); anything not `M/I/D/S/N` → die (4379–4384). **🔴 `indels` accrues for `D`
  ops ONLY** *(rev 1, Reviewer B)* — `I`/`S`/`N` deliberately do not bump it (4346/4360/4376); this is
  load-bearing because `NM:i = hemming_dist + indels` (8590), so a wrong `indels` silently corrupts every
  indel read's `NM` tag. (Insertion/soft-clip `X` padding fails the base-by-base `hemming_dist` instead.)
- **If `index(+mod) ∈ {0,2}`** (read needs 2 bases *appended*): **guard** `len(chr) >= pos+2`, else store
  partial + **return early** (4390–4394); otherwise append `chr[pos .. pos+2]` to
  `unmodified_genomic_sequence` only (4395).
- **Per-strand counter + strand/conversion assignment** (4400–4448), keyed on `index(+mod)` — *this is the
  Phase-4-deferred work, and it is reached only when neither edge guard fired*:
  - `0` → `CT_CT_count++`, strand `+`, read_conv `CT`, genome_conv `CT`.
  - `1` → `CT_GA_count++`, strand `-`, `CT`/`GA`; **`reverse_complement` the genomic seq** (+ MD-seq if del).
  - `2` → `GA_CT_count++`, strand `-`, `GA`/`CT`; reverse_complement (Phase 8 path; inert SE-dir).
  - `3` → `GA_GA_count++`, strand `+`, `GA`/`GA` (Phase 8 path; inert SE-dir).
  - else → die "Too many … result filehandles".
- Store `unmodified_genomic_sequence`, `genomic_seq_for_md_tag`, `end_position = pos`, `indels` (4450–4466).

**(b) Length guard** (Perl `check_results_single_end:3127`): if
`len(unmodified_genomic_sequence) != len(sequence) + 2` → warn "Chromosomal sequence could not be extracted…",
`genomic_sequence_could_not_be_extracted_count++` (3129), and **skip this read** (do not call / do not
write). This is exactly the case an edge guard produced a short sequence. **🔴 The 3127 *length* check is
THE gate, not the `GenomicExtraction.extracted` bool** *(rev 1, Reviewer B)* — Perl signals failure purely
by the sequence length, and a non-edge read could in principle also produce a wrong length (e.g. a CIGAR
anomaly). Branch on the length; treat `extracted` as documentation-only (or drop it).

**(c) Methylation call** (`methylation::methylation_call`, 4800–5018): compare the **original uc read** to
the genomic window base-by-base. **Call order: `methylation_call` runs BEFORE the ref-seq trim/revcomp**
(Perl 3144 then 8570/8577) — it is fed the FULL `unmodified_genomic_sequence` (read_len+2, already
revcomp'd in extraction for index 1), not the trimmed/revcomp'd `ref_seq` that `single_end_sam_output`
later builds. For the SE-directional spine `read_conversion` is always `CT` (indexes 0/1) → the **CT
branch** (4832–4912); the GA branch (4913–4998) is ported for Phase 8 but inert here. Build the `XM` string
with `Z/z` (CpG), `X/x` (CHG), `H/h` (CHH), `U/u` (unknown, when the context base is `N` **or** `X`),
`.` (non-C / non-informative). **The 4822 length `warn` is NON-fatal** *(rev 1, Reviewer A)* — do not
panic on `len(seq) != len(genomic)-2`; and context look-ups at `index+1`/`index+2` that run **past the end**
of the genomic window must behave as Perl's out-of-range `substr`/array access (empty = neither `G` nor
`N`/`X`, so they fall through to CHH/`.`), not panic — a real trap for short indel reads at a sequence end.
**Accumulate the 8 methylation counters** (`total_meCpG_count`, `total_unmethylated_CpG_count`, … 5006–5013)
— needed by the Phase-6 report; incremented here because this is where the call happens (avoids a Phase-6
back-edit). `--slam` (3140) is **out of v1 scope** → **hard-reject** in `resolve()` (§3.4).

**(d) SAM record assembly** (`output::single_end_sam_output`, 8489–8711): see §3.3.

**(e) Write** the assembled `BismarkRecord` to the open `BamWriter` (Perl `print OUT …` 8706).

### 3.3 SAM record fields (`single_end_SAM_output`, 8489–8711)

- **FLAG** from `(strand, read_conv, genome_conv)` (8521–8546): `+/CT/CT → 0`; `+/GA/GA → 16`; `-/CT/GA →
  16`; `-/GA/CT → 0` (any other combo → die). For SE-directional: index 0 → 0, index 1 → 16.
- **ref_seq trim of the +2 padding** (8570–8575): `read_conv eq 'CT'` → drop the **last** 2 bases
  (`substr 0, len-2`); else drop the **first** 2 (`substr 2, len-2`).
- **Minus-strand reorientation** (8577–8584): if `strand eq '-'` → `actual_seq = revcomp(actual_seq)`,
  `ref_seq = revcomp(ref_seq)`, `qual = reverse(qual)`, and (only if CIGAR has `D`) revcomp
  `genomic_seq_for_md_tag`. **`revcomp` = 9228** (`reverse` then `tr/ACTGactg/TGACTGAC/`) — *distinct from*
  `reverse_complement` (5161, `tr/CATG/GTAC/` then `reverse`) used in extraction; both are complement+reverse
  for upper-case `ACGTN`, but **port each verbatim** (they differ on lower-case/`N` handling).
- **🔴 Minus-strand + deletion = a DOUBLE revcomp of `genomic_seq_for_md_tag`** *(rev 1, Reviewer A)*: for an
  index-1 (`-`-strand) read **with a deletion**, the MD-seq is revcomp'd once in extraction (4419,
  `reverse_complement`) and **again** here (8581, `revcomp`). Both must be applied (net = identity on
  upper-case `ACGTN`, but only because the genome is upper-cased — do not collapse or drop either). This is
  the only path that composes the double revcomp with the MD deletion reconstitution; validate it (§9 #16).
- **`NM:i`** = `hemming_dist(actual_seq, ref_seq)` (9235, count of base-by-base inequalities; `X` padding
  bases mismatch and are intentionally counted as part of Perl's value) **+ `indels`** (8588–8592).
- **`MD:Z`** = `make_mismatch_string(actual_seq, ref_seq, cigar, genomic_seq_for_md_tag)` (9252–9595) — the
  match-run/mismatch-base builder, with the full **deletion** re-indexing path (`^<bases>`) and `X`-padding
  skip for insertions/soft-clips. **Port verbatim** (byte-identity-critical; complex).
- **`XM:Z`** = methylation call, **reversed** if `strand eq '-'` (8602–8607).
- **`XR:Z`** = read_conversion; **`XG:Z`** = genome_conversion (8611–8615).
- **Default tag set + order** (no `--non_bs_mm`, no `--rg_tag`, 8706): `NM:i, MD:Z, XM:Z, XR:Z, XG:Z` — **no
  `XA` tag in the default path** (XA is `--non_bs_mm`-only, 8679/8694; out of v1 scope). `--rg_tag`/`RG:Z`
  also out of v1 scope.
- **Columns** (8690): `QNAME=id`, FLAG, `RNAME=chr` (de-converted), `POS=start` (1-based), MAPQ (reused from
  Phase 4), `CIGAR` (Bowtie 2's, verbatim), `RNEXT=*`, `PNEXT=0`, `TLEN=0`, `SEQ=actual_seq`, `QUAL=qual`.
- **`--phred64`** → `convert_phred64_quals_to_phred33` (4191) before output; v1 default is phred33 (inert),
  but port the conversion since Phase 1 accepts `--phred64`.

### 3.4 Header (`generate_SAM_header`, 8452–8484)

`@HD\tVN:1.0\tSO:unsorted` (8454); then `@SQ\tSN:<name>\tLN:<len>` for each chromosome **in `sq_order`**
(8466–8469); then `@PG\tID:Bismark\tVN:v0.25.1\tCL:"bismark <command_line>"` (8480). The samtools `@PG` line
→ **§0 policy P1** (normalized out of the gate). `@RG` (8476) is `--rg_tag`-only (out of v1 scope).
**🔴 noodles header-serialization specifics to pin** *(rev 1, both reviewers)*:
- **Construct the `@HD` map with VN = `Version::new(1, 0)` AND `SO = unsorted`** — verify noodles serializes
  `VN:1.0` (not normalized to `VN:1`) and emits `SO:unsorted` (the `bismark-io` writer test sets neither in
  this exact form). Pin the literal bytes in §9 #12.
- **Bismark `@PG` field ORDER**: set `VN` via noodles' **typed `Map<Program>` version field** (serializes
  before `other_fields`) and `CL` via **`other_fields`** (`program::tag::COMMAND_LINE`, as `bismark-io`
  read.rs does) so the line is exactly `ID:Bismark\tVN:v0.25.1\tCL:"bismark <argv>"`. If `VN` is stuffed into
  `other_fields` after `CL`, the order flips and the (still-gated) Bismark `@PG` fails the diff. The embedded
  double-quotes in `CL:"…"` pass through verbatim (SAM has no quoting) — assert in #12.
- `--sam_no_hd` (skip header, 1732) is **out of v1 scope** → hard-reject (`@SQ`-less BAM is invalid anyway).

### Edge cases

- **Chromosome-edge read** → short genomic seq → length guard (3127) skips it; counted in `unique_best`
  (Phase 4) + `genomic_sequence_could_not_be_extracted` but **in no strand bucket** and **not written**.
- **CIGAR with `I`/`D`/`S`/`N`** → exercised by the genomic walk + `make_mismatch_string`; deletions drive
  the `genomic_seq_for_md_tag` second sequence + the MD `^` path.
- **`N`/`X` in the genomic context base** → `U`/`u` (unknown-context) methylation call.
- **Empty / single-chromosome / multi-FASTA genome** → `sq_order` must still be exactly the glob+within-file
  order (the gate's `@SQ` block).
- **QUAL** ASCII→phred conversion for BAM: subtract 33 from each ASCII byte to get the phred score noodles
  stores; `samtools view -h` renders it back as ASCII+33. The minus-strand `reverse` happens on the quality
  **before** conversion (order is irrelevant to the byte result, but keep it explicit).
- **MAPQ** range from `calc_mapq` is 0–42 — well clear of 255 ("missing"); store as-is.

## 4. Signatures (proposed)

```rust
// genome.rs
pub struct Genome {
    pub chromosomes: HashMap<String, Vec<u8>>, // upper-case ASCII, byte-indexed
    pub sq_order: Vec<String>,                  // @SQ order = iteration order of `fastas` + within-file
}
/// Consumes Phase 1's already-ordered FASTA list (`config.genome.fastas`) — does
/// NOT re-glob. `sq_order` is the encounter order across `fastas` (+ within multi-FASTA).
pub fn read_genome_into_memory(fastas: &[PathBuf]) -> Result<Genome>;
/// Returns the first whitespace-token after `>` ("" for a leading-space header —
/// the empty-name DIE lives in read_genome_into_memory, the caller). `Err` only if no `>`.
fn extract_chromosome_name(fasta_header: &str) -> Result<&str>;

// methylation.rs
#[derive(Clone, Copy, PartialEq, Eq)] pub enum Conversion { Ct, Ga } // -> "CT"/"GA" for XR/XG
pub struct GenomicExtraction {
    pub alignment_strand: u8,              // b'+' / b'-'
    pub read_conversion: Conversion,
    pub genome_conversion: Conversion,
    pub unmodified_genomic_sequence: Vec<u8>,
    pub genomic_seq_for_md_tag: Vec<u8>,
    pub end_position: u32,
    pub indels: u32,
    pub extracted: bool,                   // DOC-ONLY (edge guard fired); the driver gates on the
                                           // 3127 LENGTH check, not this bool (rev 1, Reviewer B)
}
/// Mirrors extract_corresponding_genomic_sequence_single_end (4273). Bumps the
/// per-strand counter ONLY when no edge guard fired (4400-4445, behind 4317/4390).
pub fn extract_corresponding_genomic_sequence_single_end(
    best: &BestAlignment, genome: &Genome, pbat: bool, counters: &mut Counters,
) -> Result<GenomicExtraction>;
/// Mirrors methylation_call (4800). Accumulates the 8 me/unme counters.
pub fn methylation_call(
    read: &[u8], genomic: &[u8], read_conversion: Conversion, counters: &mut Counters,
) -> Result<Vec<u8>>;
fn reverse_complement(seq: &[u8]) -> Vec<u8>; // 5161: tr/CATG/GTAC/ then reverse

// output.rs
pub fn generate_sam_header(genome: &Genome, command_line: &str) -> Header; // 8452
/// Mirrors single_end_SAM_output (8489): builds the full record. Returns None
/// for the length-guard-failed read (handled by caller via the 3127 guard).
pub fn single_end_sam_output(
    id: &str, original_seq: &[u8], qual: &[u8],
    best: &BestAlignment, ext: &GenomicExtraction, methylation_call: &[u8],
    refid: &HashMap<&str, usize>, phred64: bool,
) -> Result<BismarkRecord>;
fn make_mismatch_string(actual: &[u8], ref_seq: &[u8], cigar: &str, md_seq: &[u8]) -> String; // 9252
fn hemming_dist(a: &[u8], b: &[u8]) -> usize;  // 9235
fn revcomp(seq: &[u8]) -> Vec<u8>;             // 9228: reverse then tr/ACTGactg/TGACTGAC/
```

### `RunConfig` prereq (additive — rev 1, Reviewer B)

`RunConfig` has **no `phred64` field** today (it lives only on `Cli`/`aligner_options`). Add `pub phred64:
bool`, populated in `resolve()` from `cli.phred64` (the same additive pattern Phase 4 used for
`score_min_*`). Without it, `single_end_sam_output`'s `phred64` argument has no source. (Solexa quality
conversion, 4234, is **not** called at the SE print site — only `phred64` is, 4191 — so no `solexa` field is
needed.)

### `Counters` extension (additive — Phase 4 fields unchanged)

```rust
// per-strand (4402/4411/4426/4441) — only bumped behind the 4317/4390 edge guards
pub ct_ct_count: u64, pub ct_ga_count: u64, pub ga_ct_count: u64, pub ga_ga_count: u64,
// 3129
pub genomic_sequence_could_not_be_extracted_count: u64,
// 5006-5013 (incremented in methylation_call; REPORTED in Phase 6)
pub total_me_cpg: u64,  pub total_me_chg: u64,  pub total_me_chh: u64,  pub total_me_c_unknown: u64,
pub total_unme_cpg: u64, pub total_unme_chg: u64, pub total_unme_chh: u64, pub total_unme_c_unknown: u64,
```

## 5. Implementation outline

1. **Deps + config prereq:** add `bismark-io`, `noodles-sam`, `noodles-core`, `bstr` to
   `bismark-aligner/Cargo.toml` at the workspace-consistent pins (§2); confirm `cargo build -p
   bismark-aligner` links. **Add `pub phred64: bool` to `RunConfig`**, populated in `resolve()` from
   `cli.phred64` (additive, mirrors Phase 4's `score_min_*`). **Hard-reject the out-of-v1-scope
   output-affecting flags** in `resolve()` (alongside the existing `--sam`/`--cram` rejects):
   `--slam`, `--non_bs_mm`, `--rg_tag`, `--sam_no_hd` → `AlignerError::Unsupported` (fail-loud — they alter
   the record/tag set/header and are not gated). **Update `deferred_flags`**: drop `--rg_tag`/`--slam`/
   `--non_bs_mm`/`--sam-no-hd` (now rejected) and `--basename` (now honored, step 5); keep `--unmapped`/
   `--ambiguous`/`--ambig_bam`/`--nucleotide_coverage`/`--multicore`/`--old_flag` (Phase 6/9).
2. **`genome.rs`:** `read_genome_into_memory(fastas: &[PathBuf])` — **consume `config.genome.fastas`
   (already ordered by Phase 1), do NOT re-glob**; per-file parse (gunzip, header→`extract_chromosome_name`,
   `\r`-strip+chomp+`uc`+concat, dup-name die, empty-seq warn, empty-name die in the loader); build
   `sq_order` from the encounter order. Unit-test multi-FASTA + multi-file ordering, `.gz`, empty-name die,
   duplicate-name die, empty-sequence warn.
3. **`methylation.rs`:** `reverse_complement`; `extract_corresponding_genomic_sequence_single_end` (CIGAR
   walk + both edge guards + per-strand counters behind the guards + revcomp for index 1/3);
   `methylation_call` (CT + GA branches; counter accumulation). Byte-index the genome via `Vec<u8>` slices
   (guards guarantee in-range). Unit-test each index, each CIGAR op, edge guards (no counter bump), and the
   CpG/CHG/CHH/unknown call paths.
4. **`output.rs`:** `hemming_dist`, `revcomp`, `make_mismatch_string` (verbatim, incl. the deletion `^`
   path), then `single_end_sam_output` (FLAG, ref-seq trim, minus-strand revcomp + qual reverse, NM/MD/XM/
   XR/XG, QUAL ASCII→phred for the `RecordBuf`, wrap via `BismarkRecord::from_noodles_record` — which
   re-validates XR/XG/XM presence + `XM.len()==seq.len()`, a free correctness guard). Then
   `generate_sam_header` (noodles `Header`: `@HD` VN:1.0/SO:unsorted, `@SQ` from `sq_order`, Bismark `@PG`;
   samtools `@PG` per §0).
5. **Driver (`lib.rs`):** in `run_se_directional` — load the genome **once** before the file loop
   (`read_genome_into_memory(&config.genome.fastas)`); build the noodles header + `refid` map (chr→tid in
   `sq_order` index); construct the output path (1562–1607 rules: strip fastq/fq[.gz] suffix,
   `_bismark_bt2.bam`, `--basename`/`--prefix`/`output_dir`); open `BamWriter::from_path` (default BAM mode);
   after the loop call `writer.finish()`. In `drive_merge` — replace the "tally only" body: on `UniqueBest`,
   run extract → **length guard (3127, gate on length not the `extracted` bool)** → `methylation_call`
   (BEFORE trim/revcomp) → `single_end_sam_output` → `write_record`. Reuse `best.mapq`. Pass the
   **`@`-stripped `fix_id` identifier** (already computed in `drive_merge`) as QNAME, not the raw header.
   Tally the new counters.
6. **Counters + summary:** extend `Counters` (§4); update `counters_summary` to drop the "no BAM yet" caption
   and report the per-strand / could-not-extract counts (the full Bismark report layout is Phase 6).
7. **Deferred-flags notice:** shrink it — Phase 5 wires output, so drop `--bam`/output-mode flags it now
   honors and any flag it explicitly rejects (`--slam`, `--non_bs_mm`, `--rg_tag` → reject or assert-unset
   for v1; document which). A stale notice lies (Phase-2 lesson).
8. **Tests** (§9) — unit per module + a hermetic end-to-end integration (tiny genome FASTA + fake `bowtie2`
   emitting known SAM → assert the written BAM's `samtools view -h` against a hand-built expected) + the
   scripted real-Bowtie 2 byte-identity gate.

## 6. Efficiency

Linear in reads × read-length (the base-by-base call + MD walk). Genome held once in memory as `Vec<u8>`
per chromosome (same footprint as Perl's `%chromosomes`). `refid` lookup is a small `HashMap`. No
per-read allocation beyond the genomic window + the call/MD strings; pre-size them to `read_len + 2`. The
alignment CPU remains Bowtie 2's — no optimization target here (the goal is byte-identity, not speed).

## 7. Integration

- **Consumes:** `Decision::UniqueBest(BestAlignment)` + the original re-read read/qual (Phase 4 driver) +
  `RunConfig` (genome FASTA dir, output dir/basename/prefix, `command_line`, `phred64`).
- **Produces:** the Bismark BAM (the first real output) + the extended `Counters`.
- **Feeds Phase 6:** the strand + methylation counters → the alignment/splitting reports;
  `Ambiguous`/`NoAlignment` routing to `--ambiguous`/`--un` files + `--ambig_bam` (this phase leaves those
  untouched). **Feeds Phase 7/8:** `extract_*`/`methylation_call`/`single_end_sam_output` are written
  index-general (the GA branch + index 2/3 paths are ported now, inert on SE-directional) so PE / non-dir /
  pbat reuse them.
- **Downstream tools:** the emitted BAM must be consumable by the already-ported `bismark-extractor`/
  `-dedup` — guaranteed by byte-identity (and the `BismarkRecord` XR/XG/XM validation at construction).

## 8. Assumptions

**From epic (shared):** Perl v0.25.1 oracle + Bowtie 2 2.5.5 + samtools 1.23.1; output **fully
Bismark-generated** (only POS/CIGAR/which-alignment are aligner-derived); gate = byte-identical
**decompressed** SAM content (`samtools view` records + `-H` header), not raw `.bam` bytes (noodles BGZF ≠
samtools); **byte-identity adjudicated on Linux/oxy, never macOS**; BAM I/O via noodles (`bismark-io`);
`@SQ` order is jointly defined with `bismark-genome-preparation` discovery.

**Phase-specific:**
- **SE directional, default output mode (BAM), phred33, no `--non_bs_mm`/`--rg_tag`/`--slam`/`--sam_no_hd`/
  `--cram`.** Those output-affecting options are out of v1 scope → **hard-rejected** in `resolve()` (fail-loud,
  not asserted-unset — so they cannot silently produce wrong bytes). `--phred64` conversion is ported but
  inert by default (new `RunConfig.phred64` field — see the §4 prereq).
- **`@SQ` order comes from Phase 1's `config.genome.fastas`** (the one source of truth) — the loader consumes
  that ordered list, it does not re-glob (rev 1).
- `bowtie_sequence` (converted read) is **not** an output field; `SEQ`/methylation call use the **original**
  uc read. `best.mapq` (Phase 4) is reused, not recomputed.
- Genome stored as `Vec<u8>` (upper-case ASCII); the edge guards (4317/4390) guarantee in-range slicing.
- QUAL is a real FastQ quality string (FastQ-only this phase); `qual_byte - 33` for the noodles `RecordBuf`
  cannot underflow (a `*`/empty QUAL would only arise from FastA input = Phase 9).
- The two complement helpers (`reverse_complement` 5161 vs `revcomp` 9228) are **both** ported verbatim.
- `make_mismatch_string`'s deletion path is ported verbatim (real WGBS has indels; not a "common-case-only"
  port).
- Default tag set is exactly `NM MD XM XR XG` in that order; noodles preserves `Data` insertion order and
  `samtools view -h` renders integer tags as `:i:` — **verify** (§9 #11), it underpins the whole gate.

## 9. Validation

| # | Verify | How | Expected |
|---|--------|-----|----------|
| 1 | Genome load + `@SQ` order | unit: multi-file + multi-FASTA dir (mixed `.fa`/`.fa.gz`) | `sq_order` == glob+within-file order; `LN` == sequence length; matches genome-prep discovery order |
| 2 | `extract_chromosome_name` | unit | `>chr1 desc` → `chr1`; `> chr1` (leading space) → die; no `>` → die |
| 3 | Genomic extract, index 0 (OT, append +2) | unit, canned `BestAlignment` + tiny genome | `+`/`CT`/`CT`; seq == read_len+2; `CT_CT_count=1` |
| 4 | Genomic extract, index 1 (OB, prepend +2 then revcomp) | unit | `-`/`CT`/`GA`; reverse-complemented; `CT_GA_count=1` |
| 5 | Chromosome-edge guard (both 4317 & 4390) | unit: alignment within 2 bp of an end | short seq; **no** strand counter bump; `extracted=false` |
| 6 | Length guard (3127) | driver-level unit on the #5 read | `genomic_sequence_could_not_be_extracted_count=1`; record **not** written |
| 7 | CIGAR walk: `M`/`I`/`D`/`S`/`N` | unit per op + illegal op dies | genomic seq + `indels` + `genomic_seq_for_md_tag` correct |
| 8 | `methylation_call` CT context calls | unit: constructed read/genomic for each of Z/z X/x H/h U/u . | exact `XM` string + counter bumps |
| 9 | `make_mismatch_string` | unit: clean match, single mismatch, leading/adjacent mismatch (`0`-padding), 1-deletion (`^`), **≥2-deletion** (re-indexing branch 9402/9481/9526), **deletion-in-final-MD-token** (trailing arm 9526–9578), deletion-adjacent-to-mismatch, insertion/soft-clip (`X` skip) | exact `MD:Z` string per Perl |
| 10 | `single_end_sam_output` plus/minus strand | unit: index 0 → FLAG 0, no revcomp; index 1 → FLAG 16, `SEQ`/`QUAL`/`XM` reversed | exact fields + tag order `NM MD XM XR XG` |
| 11 | **noodles→BAM→`samtools view -h` round-trip fidelity** | integration: write 1 record, `samtools view -h`, **literal byte diff** vs hand-built SAM line | **`NM` renders `:i:`** (not `:Z:`/`:f:`); tag order literally `NM MD XM XR XG`; **QUAL = exact original ASCII**; SEQ exact; MAPQ exact |
| 12 | Header (**literal byte diff**) | unit/integration: `samtools view -H` vs hand-written expected | `@HD\tVN:1.0\tSO:unsorted` (catch `VN:1.0`→`VN:1` + missing `SO`); every `@SQ\tSN:..\tLN:..` in `sq_order` (no stray subfields); `@PG\tID:Bismark\tVN:v0.25.1\tCL:"bismark <argv>"` exact incl. embedded quotes + field order |
| 13 | **NM with indels** (`D`-only) | unit: CIGAR e.g. `50M2D3I47M` | `NM:i = hemming_dist + indels`, `indels` counts the 2 `D` **not** the 3 `I` (the `I` `X`-padding counts via `hemming_dist`) |
| 14 | **Length guard skips the read** | driver unit on an all-edge-read input via a counting writer double | `genomic_sequence_could_not_be_extracted_count` bumped AND **zero records written** |
| 15 | **`U`/`u` via padding `X` as context** | unit: C at the last read position whose CpG context is the appended +2 padding / an insertion `X` (4844/4856) | `U`/`u` call |
| 16 | **Minus-strand (index 1) + deletion** | unit: `-`-strand read with a `D` | exact `MD:Z` (double-revcomp of `genomic_seq_for_md_tag`: 4419 + 8581 both applied) + `NM` |
| 17 | **Hermetic end-to-end** | fake `bowtie2` + tiny genome → run pipeline → `samtools view -h rust.bam` | == hand-built expected SAM (records + header per §0); QNAME = `@`-stripped `fix_id` |
| 18 | **🎯 Byte-identity gate (Linux)** | small SE-dir WGBS, real Bowtie 2 2.5.5 + Perl v0.25.1; `diff <(samtools view -h rust.bam) <(samtools view -h perl.bam)` (both filtered per §0, via one shared filter helper used by #11/#17/#18) | empty diff |

## 10. Questions or ambiguities

- **(RESOLVED 2026-06-01 → P1)** **samtools-pipe `@PG` policy** (§0): Felix chose **P1 (normalize-out)**.
  The gate filters `@PG ID:samtools …` from both sides; the noodles header carries only Bismark's `@PG`.
  P2 dropped.
- **(Open)** **Where the small byte-identity gate runs.** The epic says "local" but also "adjudicate on
  Linux". *Assumption:* run the small SE-dir WGBS gate on **oxy** (idle-gated) or **Linux CI**, not macOS;
  macOS gets unit + hermetic-integration tests only. Confirm.
- **(Open)** **Write via `bismark-io::BamWriter` (BAM, default) vs a direct SAM-text writer.** *Assumption:*
  BamWriter (the project's noodles standard; default Bismark output is BAM). Validation #11 de-risks the
  round-trip; if a noodles encoding can't be made to match `samtools view -h`, the contingency is to emit
  the SAM line text directly — flagged as a risk, not the plan.
- **(Open)** **`Conversion` enum vs `&str`.** *Assumption:* a small enum mapped to `"CT"`/`"GA"` at the tag
  boundary (cleaner than threading strings). Inconsequential to output bytes.

## 11. Self-Review

- **Efficiency:** linear; genome loaded once as `Vec<u8>`; windows/strings pre-sized; no per-read genome
  re-read. ✓
- **Logic:** the three counters are placed exactly where Perl puts them — `unique_best` at 3121 (Phase 4,
  untouched), per-strand at 4402–4441 **behind** the 4317/4390 edge guards (so edge reads land in none), and
  `could_not_be_extracted` at 3129 after the 3127 length guard. The decision/print split from Phase 4 is
  preserved (merge stays pure; the driver prints). ✓
- **Edge cases:** chromosome edges, indel/soft-clip CIGARs, `N`/`X` context (`U`/`u`), phred64, minus-strand
  revcomp + qual reverse + XM reverse, empty/multi-FASTA `@SQ` order, MAPQ range. ✓
- **Integration:** index-general `extract`/`call`/`output` (GA + index 2/3 inert now) so Phase 7/8 reuse
  them; new `Counters` fields are additive; `BismarkRecord` construction re-validates XR/XG/XM. ✓
- **Risks:** (a) **`make_mismatch_string` deletion path** is intricate — verbatim port + targeted MD tests
  (#9). (b) **noodles→`samtools view -h` round-trip** (tag type/order, QUAL phred, header serialization) is
  the load-bearing gate assumption — pinned by #11/#12 before the real-data gate. (c) **`@SQ` glob order**
  must match genome-prep on the gate platform (not macOS) — #1 + the Linux gate. (d) the **samtools `@PG`**
  decision (§0/§10) — assumed P1; one localized change if P2.

## 12. Revision History

- **rev 1 (2026-06-01)** — folded in dual plan-review (`PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md`; both verified
  the core logic against Perl v0.25.1 — **no Critical logic defects**). All findings accepted (no
  contradictions). Changes:
  - 🔴 **Genome loader consumes Phase 1's `config.genome.fastas`, not a re-glob** (both reviewers; B Critical)
    — one source of truth for the `@SQ` order. (§3.1, §4, §5, §8.)
  - 🔴 **`phred64` config prereq** — `RunConfig` had no such field; added (additive, like `score_min_*`).
    (B Critical; §4, §5.)
  - 🔴 **Hard-reject `--slam`/`--non_bs_mm`/`--rg_tag`/`--sam_no_hd`** in `resolve()` (fail-loud, not defer);
    `--basename` dropped from `deferred_flags` (now honored). (Both; §3.2c, §3.4, §5, §8.)
  - **+2 prepend/append bases go to `unmodified_genomic_sequence` only, never `genomic_seq_for_md_tag`**
    (A). **`indels` accrues for `D` only** (B). (§3.2a.)
  - **3127 LENGTH check is the gate, not the `extracted` bool** (B). **`methylation_call` runs before the
    ref-seq trim/revcomp**; non-fatal 4822 warn; out-of-range context reads behave as Perl empty `substr`
    (A). (§3.2b/c, §4.)
  - **Minus-strand + deletion = double revcomp of `genomic_seq_for_md_tag`** (4419 + 8581) — both applied
    (A). (§3.3, §9 #16.)
  - **noodles `@PG` field order** (VN typed field, CL `other_fields`) + **`@HD VN:1.0`/`SO:unsorted`**
    serialization pinned; #11/#12 are now **literal byte diffs**. (Both; §3.4, §9.)
  - **Validation additions:** multi-deletion + deletion-in-final-token MD cases (#9), NM-with-indels (#13),
    length-guard-writes-nothing (#14), `U`/`u`-via-`X` (#15), minus-strand+deletion (#16), QNAME=`@`-stripped
    `fix_id` (#17), shared gate filter helper (#18). (§9.)
- **rev 0 (2026-06-01)** — initial plan. Manual review: Felix resolved the §0 `@PG` decision → **P1
  (normalize-out)**; §0/§10/§9 updated to reflect it. Then dual plan-review (folded into rev 1).

## 13. Implementation Notes (2026-06-01)

**Status: IMPLEMENTED — 108 unit + 16 integration tests green; clippy `-D warnings` + `cargo fmt --check`
clean.** (Full byte-identity gate on Linux/oxy = §9 #18, PENDING — see below.)

### What was built
- **`genome.rs`** — `read_genome_into_memory(fastas: &[PathBuf])` consuming Phase 1's
  `config.genome.fastas` (no re-glob; single `@SQ`-order source of truth) + `extract_chromosome_name`
  (empty-name die in the loader; `chomp` + first-`\r` strip via `chomp_cr`; `uc`; dup-name die; empty-seq
  warn). Returns `Genome { chromosomes: HashMap<String,Vec<u8>>, sq_order: Vec<String> }`.
- **`methylation.rs`** — `Conversion` enum; `reverse_complement` (5161); `parse_cigar` (pub(crate));
  `extract_corresponding_genomic_sequence_single_end` (CIGAR walk M/I/D/S/N, both edge guards, per-strand
  counters behind the guards, revcomp for index 1/2, `indels` = `D`-only, +2 NOT in MD-seq);
  `methylation_call` (CT + GA branches; `U`/`u` via `N` **or** `X`; out-of-range context via sentinel; 8
  counters accumulated).
- **`output.rs`** — `generate_sam_header` (noodles `Header`: `@HD VN:1.0 SO:unsorted`, `@SQ` in `sq_order`,
  `@PG ID:Bismark VN:v0.25.1 CL:"bismark <argv>"` — VN before CL via insertion order; samtools `@PG`
  normalised out per P1); `single_end_sam_output` (FLAG, +2 trim, minus-strand revcomp + qual-reverse,
  NM = hemming + indels, MD via `make_mismatch_string`, XM reversed for `-`, tags `NM MD XM XR XG` in order,
  QUAL ASCII→phred via `q - offset`); `make_mismatch_string` + `rebuild_md_with_deletions` (verbatim
  deletion `^` re-indexing); `hemming_dist`; `revcomp` (9228); `build_refid`; `write_record`.
- **`config.rs`** — additive `RunConfig.phred64`; `reject_unsupported_output_flags` hard-rejects
  `--slam`/`--non_bs_mm`/`--rg_tag`/`--sam-no-hd`; `deferred_flags` shrunk (dropped those + `--basename`).
- **`merge.rs`** — `Counters` extended (additive): `ct_ct`/`ct_ga`/`ga_ct`/`ga_ga`,
  `genomic_sequence_could_not_be_extracted_count`, the 8 methylation tallies.
- **`lib.rs`** — `run_se_directional` loads the genome once, builds the header + `refid`, opens a per-file
  `BamWriter`, drives the merge, `finish()`es. `drive_merge` now: on `UniqueBest` → extract → **3127 length
  guard** (gate on length; skip + count `could_not_extract`) → `methylation_call` → `single_end_sam_output`
  → `write_record`; QNAME = the `@`-stripped `fix_id`; reuses `best.mapq`. `output_bam_path` builds
  `…_bismark_bt2.bam` (suffix strip + `--prefix`/`--basename`/`--output_dir`). `counters_summary` updated.
- **Deps:** `bismark-io =1.0.0-beta.8`, `noodles-sam =0.85.0`, `noodles-core =0.20.0`, `bstr =1.10.0`.

### Deviations / decisions (documented)
- **noodles `Map<Program>` is a unit struct** — no typed `VN` field; both `VN` and `CL` go in `other_fields`
  (insertion order → `VN` before `CL`). Reviewer-A's "typed version field" doesn't apply in 0.85; the
  resulting byte order is identical regardless. Confirmed against the noodles header writer
  (`write_program` = `ID` then `other_fields` in order).
- **`@PG CL:` value carries literal surrounding double-quotes** (`CL:"bismark …"`, Perl 8480) — a real bug
  caught by the header byte-diff unit test and fixed.
- **Empty-chromosome `LN:0`** can't be represented by noodles' `NonZeroUsize` length; mapped to `1`
  (documented; pathological — excluded from real test genomes).
- **Tests**: the Phase-4 `tests/cli.rs` `happy_path` + `deferred_flag` now set `--output_dir` (they complete
  a full SE run and would otherwise write a BAM into the repo CWD); summary assertion updated
  (`Phase 4 merge summary` → `Mapping summary`). Added a mapped-read **hermetic e2e** (`make_fake_bowtie2_mapped`
  → read BAM back via `bismark-io`, assert FLAG/POS/MAPQ/SEQ/QUAL/MD/XM/XR/XG) and a **noodles round-trip**
  unit test (tag order + values + QUAL phred). Header byte-diff unit test pins `@HD`/`@SQ`/`@PG` exactly.

### PENDING — §9 #18 (the byte-identity gate, Linux/oxy)
The cargo suite is fully hermetic (no samtools/Bowtie 2). The **real-data byte-identity gate** — `diff
<(samtools view -h rust.bam) <(samtools view -h perl.bam)` on a small SE-directional WGBS dataset with Perl
Bismark v0.25.1 + Bowtie 2 2.5.5 + samtools 1.23.1 — must run on **oxy/Linux** (per the "adjudicate on Linux,
never macOS" rule). That run is the Phase-5 gate's final sign-off (the full-scale gate is Phase 10).

### Post-implementation verification (2026-06-01)
Dual `code-reviewer` (`CODE_REVIEW_A.md`/`CODE_REVIEW_B.md`) + `plan-manager` (`COVERAGE.md`), fresh contexts:
- **Both code reviews: APPROVE** — no Critical/High. **Reviewer B vendored both the Perl `make_mismatch_string`
  and the Rust port into a differential harness and diffed 27 deletion cases (single/double/triple, mismatch
  before/between/after, leading, multi-base, trailing-token re-index) → all byte-identical.** Reviewer A
  hand-traced the same machinery + the in-loop `−1` vs trailing-arm no-`−1` distinction → matches.
- **plan-manager: COMPLETE** — every §3/§5/§9 item implementable in cargo is DONE; 124→**128** tests pass.

**Convergent findings folded in (test-only + one defensive fix; no behavior change):**
- **M1 (Review A):** `make_mismatch_string` Part 1 now matches Perl's past-the-end `substr` (empty, not a NUL
  byte) via an explicit `None` arm (unreachable given the length guard, but the latent divergence is gone).
- **#16 test added** (the plan's rev-1 🔴 must-validate): `minus_strand_index1_deletion_double_revcomp` —
  index-1 + deletion through the real extraction → `MD:Z:3^G3`, `NM:i:1`, FLAG 16, SEQ `CCGGTT`
  (hand-derived; the MD-builder is the Perl-differential-verified one).
- **#13 tests added:** `extract_combined_deletion_insertion_indels_counts_d_only` (indels = `D` only) +
  `nm_includes_indels_d_only` (NM = hemming + indels).
- **#14 test added:** `chromosome_edge_read_counted_but_not_written` (mapped read off the chr edge →
  `could_not_extract` + header-only BAM, read back via `bismark-io`).

**Final: 111 unit + 17 integration tests; clippy `-D warnings` + `cargo fmt --check` clean; workspace builds.**

### §9 #18 real-data byte-identity gate (oxy/Linux) — ✅ PASSED 2026-06-01
`bismark_rs` (built on oxy from the uncommitted worktree) vs Perl Bismark v0.25.1 + Bowtie 2 2.5.5 +
samtools 1.23.1, on real GRCh38 WGBS SE-directional reads (`~/bismark_benchmarks/10M_SE`), identical argv,
`diff <(samtools view -h …| grep -v '@PG.*ID:samtools')` both sides:
- **10k reads → 8,402 records, BYTE-IDENTICAL** (incl. 1 chromosome-edge `could-not-extract` read, counted-
  not-written, matching Perl).
- **1M reads → 848,124 records, BYTE-IDENTICAL** (exercised thousands of indel/deletion/soft-clip reads +
  the header `@HD`/`@SQ`/Bismark `@PG`).

Details + reproduction: [`GATE_OXY.md`](./GATE_OXY.md). **Phase 5 is fully verified** (the full-scale SE+PE+
RRBS gate is Phase 10).
