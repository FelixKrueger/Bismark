# Phase 1 PLAN — GpC report (`--gc`/`--gc_context`) + NOMe-Seq (`--nome-seq`)

**Epic:** `05312026_bismark-c2c-niche-modes/EPIC.md`, Phase 1 — GpC report + NOMe-Seq
**Design contract:** the v1.0 `../../05292026_bismark-coverage2cytosine/SPEC.md` (rev 3) — §6 (genome reader), §7 (coordinate arithmetic / context classification), §5 (output topology), §8 (context summary), §10 (structural choices). This phase **reuses** that infrastructure and adds one new output stream + one new walk model + the NOMe filters.
**Perl ground truth:** `coverage2cytosine` v0.25.1 (repo root) — `generate_GC_context_report:751-1073`, the core-report NOMe hooks in `generate_genome_wide_cytosine_report:387-394,402-406,417-421,460-462,630-637,644-648,659-663,692-693,701-703,708-713,740-742`, `handle_filehandles:119-150`, the main flow `:38-84`, and `process_commandline:2147-2161`.
**Status:** rev 0 (2026-05-31) — initial draft from EPIC + v1.0 SPEC + Perl + **live-Perl-v0.25.1 fixture confirmation** (see §3 "Empirically observed"). Awaiting manual review.

---

## 1. Goal

Flip `--gc`/`--gc_context` and `--nome-seq` from **CLI-rejected** (the v1.0 `UnsupportedFlag` stubs) to **supported**, and add:

1. **The GpC-context report** (`generate_GC_context_report`) — a second genome walk over every `GC` dinucleotide that emits a GpC-context per-cytosine report `{raw-o}.GpC_report.txt[.gz]` **and** a companion coverage file `{raw-o}.GpC.cov[.gz]`. `--gc` runs this **in addition** to the normal core report + context summary.
2. **NOMe-Seq mode** (`--nome-seq`), which (a) implies `--gc`, (b) bumps the coverage threshold to 1, (c) restricts the **core** CpG report to CpGs whose upstream-trinucleotide context is `ACG`/`TCG`, writes it to `{stem}.NOMe.CpG_report.txt[.gz]` with a **companion** `{stem}.NOMe.CpG.cov[.gz]`, and **skips uncovered chromosomes** entirely, and (d) restricts the **GpC** report to *non-CG* GpC sites (drops the CG-context GpC entries) and writes it to `{raw-o}.NOMe.GpC_report.txt[.gz]` + `{raw-o}.NOMe.GpC.cov[.gz]`.

**Byte-identical to Perl v0.25.1** for every new/changed output stream (STDERR exempt — same contract as v1.0).

## 2. Context

- **Where the code lives:** a new module `gpc.rs` in `rust/bismark-coverage2cytosine/src/` (the GpC walk + writers), plus targeted changes to `cli.rs` (un-reject the two flags + resolve their couplings into `ResolvedConfig`), `report.rs` (NOMe core-report changes: ACG/TCG filter, `.NOMe.*` filenames, the NOMe `.cov` companion, the uncovered-chromosome skip), `lib.rs` (call `gpc::run_gpc` after the core report + summary when `gc_context`), and `error.rs` (no new variant expected — see §10).
- **Reused v1.0 infrastructure (per epic shared assumption 2):**
  - `genome::Genome` (`get`, `names_sorted`, `contains`) — the same uppercased in-memory genome (`§6`). The GpC walk reads the **same `Genome::get(name)` slice**.
  - `cov::open_cov` / `cov::parse_cov_line` — the GpC report re-reads the **same coverage file** (Perl re-opens `$coverage_infile`), per-chromosome buffered exactly like the core report (`§4`/`§7.5`).
  - `report::ReportWriter` (`create`/`write_all`/`finish`; plain or `GzEncoder`) — used for both the GpC report and the GpC `.cov`, and for the NOMe core `.cov` companion.
  - `report::{perl_substr, revcomp, classify_context, Context}` — the GpC walk's trinucleotide context classification is the **same** `^CG`/`^C.G$`/`^C..$` logic (`§7.3`); `revcomp` and `perl_substr` are the same primitives.
  - `ResolvedConfig.{output_raw, output_stem, output_dir, gzip, zero_based, split_by_chromosome, threshold, cpg_only}` — note the GpC filenames derive from **`output_raw`** (the raw `-o`, NOT the stripped stem — confirmed §3), while the core `.NOMe.*` filenames derive from **`output_stem`** (the dedup-stripped base, like the normal core report).
- **Phase dependencies (per the epic sub-plan table):** depends on v1.0 (Phases A–E) merged. Phases 2 (DRACH) and 3 (FFS) are mutually independent of this phase. **The FFS interaction:** Perl's GpC `print CYT` lines have **no** `$tetra` branch (the GpC report never emits FFS columns), but the **core** report's NOMe `print CYT` shares the file with the `$tetra` branch (Perl `:399/:414` vs `:404/:419`). v1.0 has no FFS; this phase must not assume FFS columns. The NOMe core-report changes here live **inside** the same `if ($CpG_only){ if (/^CG/){ ... } }` block that Phase 3 will extend — coordinate so Phase 3's FFS columns and this phase's NOMe `.cov` companion compose (Perl already nests them: `if($tetra){...}else{ if($nome){...}else{...} }`). **Not a blocker** (different flags), but flagged for the implementer (§10, Q-open).

## 3. Behavior

### Empirically observed (local Perl v0.25.1 — ground truth, this session)

Fixtures run against the repo Perl `./coverage2cytosine` (self-contained, v0.25.1). All filenames/format/ordering below are **observed**, not inferred.

- **`-o sample --gc`** (genome `AGCAGCGCATGCGGCATTAGCTAGC`, cov at 2,3,6,7,8):
  - Emits **both** `sample.CpG_report.txt` (the normal core report, incl. uncovered all-zero CpGs at 12,13) **and** `sample.GpC_report.txt` + `sample.GpC.cov` + `sample.cytosine_context_summary.txt`.
  - `sample.GpC_report.txt`:
    ```
    chr1	6	+	10	0	CG	CGC
    chr1	7	-	0	5	CG	CGC
    chr1	8	+	3	1	CHH	CAT
    ```
  - `sample.GpC.cov` (the `.cov` is `chr  pos  pos  %.6f-pct  meth  nonmeth`, **start==end**, 1-based):
    ```
    chr1	6	6	100.000000	10	0
    chr1	7	7	0.000000	0	5
    chr1	8	8	75.000000	3	1
    ```
  - **Ordering within a `GC`: bottom strand (`-`) printed BEFORE top strand (`+`)** (Perl `:917-939`: the bottom-strand block precedes the top-strand block).
- **`-o sample --nome-seq`** (same fixture): files are `sample.NOMe.CpG_report.txt`, `sample.NOMe.CpG.cov`, `sample.NOMe.GpC_report.txt`, `sample.NOMe.GpC.cov`, `sample.cytosine_context_summary.txt` (**summary has NO `.NOMe` infix**). NOMe core report **empty** here (no covered ACG/TCG CpG); NOMe GpC report keeps only the `CHH` site (CG-context GpC dropped):
    ```
    chr1	8	+	3	1	CHH	CAT
    ```
- **NOMe ACG/TCG filter is on the *upstream* trinucleotide** (genome `TTACGTTAGCATCGTT`, cov at 4,5,13): NOMe core report keeps pos 4 (`+`, upstream `ACG`), pos 5 (`-`, upstream revcomp = `ACG`), pos 13 (`+`, upstream `TCG`); a covered CpG whose upstream is neither ACG nor TCG is dropped.
- **`--gc --gzip`** → `sample.GpC_report.txt.gz` + `sample.GpC.cov.gz`; summary plain. Decompressed GpC bytes identical to the plain run.
- **`--gc --split_by_chromosome`** → `sample.chrchr1.GpC_report.txt` + `sample.chrchr1.GpC.cov` + `sample.chrchr1.cytosine_context_summary.txt` (the `.chr` infix appended to **raw `-o`**, same doubling rule as the core report).
- **`-o sample.CpG_report.txt --gc`** → GpC files are `sample.CpG_report.txt.GpC_report.txt` + `sample.CpG_report.txt.GpC.cov` — i.e. the GpC name uses the **raw, un-stripped `-o`** (Perl `$cytosine_out` is the raw option value; the "cleaned" comment at `:787` is misleading — `handle_filehandles` strips a *local* copy only). The core CpG report dedup-strips to `sample.CpG_report.txt`.
- **GpC chromosome-edge guards** (genome `GCAGCTTAGC`, cov at 1,2,4,5): the first `GC` (pos 2) is **entirely dropped** (its bottom-strand trinucleotide `substr(seq,pos-4,3)` is <3 bp); the last `GC` (idx 8-9) is dropped (top trinucleotide <3 bp). Only the interior `GC` at pos 4/5 emits (`4 - CTG`, then `5 + CTT`).

### 3.1 CLI resolution (`cli.rs` — Perl `process_commandline:2147-2161`, `:2188`)

Un-reject `--gc`/`--nome-seq`; keep `--drach`/`--ffs` rejected (Phases 2/3). Add to `ResolvedConfig`: `gc_context: bool`, `nome: bool`. Resolution order (mirror Perl exactly — the order matters because NOMe mutates `gc_context` and `threshold`):

1. **v1.x rejection** now only fires for `--drach` and `--ffs` (drop the `--gc`/`--nome-seq` arms).
2. After the existing `cov_infile`/`output`/`genome_folder`/merge-mutex checks, add the **NOMe block** (Perl `:2147-2161`), evaluated **before** the threshold-default block:
   - `--nome-seq` + `--CX` → **die** `NomeWithCx` (Perl `:2148`: "NOMe-Seq filtering only works for CpG context, please drop the '--CX' option").
   - `--nome-seq` + `--merge_CpGs` → **die** `NomeWithMerge` (Perl `:2149`).
   - `--nome-seq` ⇒ set `gc_context = true` (unless already set) (Perl `:2150-2153`).
   - `--nome-seq` ⇒ if `--coverage_threshold` was **not** given, set `threshold = 1`; if it **was** given, keep the user value (Perl `:2154-2160`). ⚠️ This means with `--nome-seq` the existing `threshold == Some(0)` → `ThresholdNotPositive` check still fires (an explicit `--coverage_threshold 0` is illegal regardless), and an explicit positive threshold with `--nome-seq` is honoured.
3. The existing threshold block: when `threshold` unset, default `0`. **NOTE the NOMe `=1` default is applied in step 2 before this**, so NOMe never falls to the `0` default. Reproduce by resolving `threshold` after the NOMe block (the cleanest faithful order: parse `--coverage_threshold` → if nome and unset, `1`; else existing logic).
4. **`--gc` does NOT bump the threshold at CLI time.** Perl bumps `$threshold` to 1 **inside** `generate_GC_context_report` (`:758-761`), *after* the core report has already run at the original threshold. So for `--gc` *without* `--nome-seq`, the **core report runs at threshold 0** (full uncovered genome) and **only the GpC walk uses an effective threshold of `max(threshold,1)`**. ⚠️ Reproduce this split: do **not** raise `ResolvedConfig.threshold`; instead the GpC walk computes its own `gpc_threshold = if config.threshold == 0 { 1 } else { config.threshold }`. (With `--nome-seq`, `config.threshold` is already ≥1 from step 2, so `gpc_threshold == config.threshold`.)

Mutex summary (Perl-faithful): `--nome-seq` ✗ `--CX`; `--nome-seq` ✗ `--merge_CpGs`; `--gc` alone has no new mutexes (it composes with `--CX`, `--zero_based`, `--split_by_chromosome`, `--gzip`, `--coverage_threshold`). `--merge_CpGs` ✗ `--CX`/`--split`/`--threshold` (unchanged); since `--nome-seq` ✗ `--merge_CpGs`, the merge path never combines with NOMe.

### 3.2 Core-report NOMe changes (`report.rs` — Perl core hooks)

When `config.nome` (only ever true with `cpg_only == true`, since NOMe ✗ `--CX`):

1. **ACG/TCG upstream filter** (Perl `:387-394`, `:630-637`): inside `emit_position`, after `classify_context` returns `Cg` and **before** emitting, if `nome` is set, keep the position **only if** the upstream trinucleotide equals `ACG` or `TCG` (the same `upstream` 3-mer already computed by `extract`, revcomp'd for `-` strand). Otherwise skip (no emit, no `.cov` write — but the context-summary accumulation at guard 6 already happened, which is correct: Perl's `context_reporting` at `:381`/`:624` runs *before* the NOMe `next`). ⚠️ The filter applies to **CG-context positions only** (it's nested inside `if ($tri_nt =~ /^CG/)`); CHG/CHH are never emitted in CpG-only mode anyway.
2. **`.NOMe.CpG.cov` companion** (Perl `:402-406`, `:417-421`, `:644-648`, `:659-663`): when `nome` and a CpG passes the ACG/TCG filter, in **addition** to the report line, write a coverage line to a separate `CYTCOV` writer: `chr  out_pos  out_pos  %.6f-pct  meth  nonmeth` where `pct = meth/(meth+nonmeth)*100` to 6 dp and `out_pos` honours `--zero_based` (`pos-1`). ⚠️ `meth+nonmeth` is guaranteed `> 0` here because NOMe threshold ≥ 1 (an uncovered `0,0` position is dropped by guard 3 before reaching this — **no division by zero**). The non-NOMe core report writes **no** `.cov` (Perl only opens `CYTCOV` when `$nome` — `handle_filehandles:141-148`).
3. **Filenames** (Perl `handle_filehandles:119-127`): with `nome`, the core report is `{stem}.NOMe.CpG_report.txt[.gz]` and its cov companion is `{stem}.NOMe.CpG.cov[.gz]`. The **context-summary** name is the **non-NOMe** base `{stem}.cytosine_context_summary.txt` (the `.NOMe` is appended to `cytosine_report_file`/`cytosine_coverage_file` *after* the summary name was taken from `cytosine_report_file` at `:115-116`). NOMe is incompatible with `--CX`, so only the `.CpG_*` (never `.CX_*`) branch applies.
4. **Uncovered-chromosome skip** (Perl `:708-713`): with `nome`, the uncovered-chromosome pass is **entirely skipped** — NOMe reports covered positions only (regardless of threshold). In the Rust `run_single`/`run_split`, gate the uncovered pass on `config.threshold == 0 && !config.nome` (today it is `config.threshold == 0`; NOMe always has threshold ≥1 so the `threshold==0` guard *already* skips it — but make the `!nome` explicit for clarity and to defend against a future `--nome-seq --coverage_threshold` path where the user threshold could be... still ≥1, so moot; nevertheless mirror Perl's distinct `if($nome)` branch).

### 3.3 The GpC walk (`gpc.rs` — Perl `generate_GC_context_report:751-1073`)

A second genome walk, structurally parallel to the core report but over `GC` dinucleotides. **Effective threshold** = `gpc_threshold` (§3.1 step 4): `max(config.threshold, 1)`.

**Per-chromosome processing** (same streaming model as the core report): re-open the coverage file (`cov::open_cov(&config.cov_infile)`), buffer cov lines per-chromosome into `pos → (meth, nonmeth)` (`cov::parse_cov_line`), flush each chromosome on `chr`-transition by walking `Genome::get(chr)`. **Covered chromosomes only** — the GpC report has **no uncovered-chromosome pass** (Perl `generate_GC_context_report` only processes chromosomes seen in the cov file; `$processed{$last_chr}=1` is set but no `sort keys %processed` loop exists in this function). A cov chromosome absent from the genome yields no bytes (empty `while` walk).

**The `GC` walk + coordinate arithmetic** (Perl `:848-940` covered-chr block, identical `:966-1060` last-chr block — collapse to ONE shared kernel, per the v1.0 SPEC §7.2 anti-dual-driver rule). For each 0-based index `j` where `seq[j]==b'G' && seq[j+1]==b'C'` (a `GC` dinucleotide), Perl's `pos($seq)` after matching `/(GC)/g` is the offset **past** the `GC` = `j+2`. Let `pos = j + 2` (so `pos` is 1-based and points one past the C). Then:

- **Top strand C** (the `C` at 0-based `j+1`, 1-based `pos`): strand `+`.
  - `tri_nt_top = perl_substr(seq, pos-1, 3)` = `seq[j+1 .. j+4]` (the C + next 2 bases). (Perl `:862`.)
- **Bottom strand C** (the `G` at 0-based `j`, reported 1-based as `pos-1`): strand `-`.
  - `tri_nt_bottom = revcomp(perl_substr(seq, pos-4, 3))` = `revcomp(seq[j-2 .. j+1])`. ⚠️ `pos-4 = j-2`; when `j < 2`, `pos-4` is negative → `perl_substr` negative-wrap (use the existing `perl_substr` — it already models Perl's from-end wrap), producing a short slice that fails the `<3` guard (matches observed: first-`GC`-at-chr-start drops). (Perl `:866-868`.)
- **Guards (both must pass, else `next` skips the WHOLE dinucleotide — both strands)** (Perl `:871-872`, `:989-990`):
  1. `tri_nt_top.len() < 3` → skip the dinucleotide.
  2. `tri_nt_bottom.len() < 3` → skip the dinucleotide.
- **Context classification**: classify `tri_nt_top` and `tri_nt_bottom` with the **same** `classify_context` (`^CG`/`^C.G$`/`^C..$`). If **either** is unclassifiable → Perl `warn` + `next` (skip the whole dinucleotide; STDERR exempt). (Perl `:887-914`.)
- **Coverage lookup**: top counts from `buffer.get(pos)`; bottom counts from `buffer.get(pos-1)` (both 1-based keys; uncovered → `(0,0)`). (Perl `:875-884`.)
- **Emit order: BOTTOM strand first, then TOP** (Perl prints the bottom block `:917-927` before the top block `:929-939`).
  - **Bottom**: if `meth_bottom + nonmeth_bottom >= gpc_threshold`:
    - `pct = meth_bottom/(meth_bottom+nonmeth_bottom)*100` to 6 dp.
    - **NOMe filter** (Perl `:919-922`): if `config.nome && context_bottom == Cg` → **skip** (drop CG-context GpC). Else emit:
      - GpC `.cov`: `chr  (pos-1)  (pos-1)  pct  meth_bottom  nonmeth_bottom` (NB: **the GpC `.cov` is NOT `--zero_based`-adjusted** — Perl uses `$pos-1` literally with no `$zero` branch; confirmed there is no `if($zero)` anywhere in `generate_GC_context_report`).
      - GpC report: `chr  (pos-1)  -  meth_bottom  nonmeth_bottom  context_bottom  tri_nt_bottom`.
  - **Top**: if `meth_top + nonmeth_top >= gpc_threshold`:
    - `pct = meth_top/(meth_top+nonmeth_top)*100` to 6 dp.
    - **NOMe filter** (Perl `:931-934`): if `config.nome && context_top == Cg` → **skip**. Else emit:
      - GpC `.cov`: `chr  pos  pos  pct  meth_top  nonmeth_top`.
      - GpC report: `chr  pos  +  meth_top  nonmeth_top  context_top  tri_nt_top`.

⚠️ **No `--zero_based` in the GpC report or `.cov`** — Perl's GpC function has no `$zero` handling. The reported coordinates are always the 1-based `pos` / `pos-1`. (Verify in a `--gc --zero_based` golden — Q-resolved expectation: identical to `--gc` without `--zero_based`.)

⚠️ **GpC threshold is `>= gpc_threshold` with `gpc_threshold ≥ 1`** ⇒ uncovered (`0,0`) positions are **always dropped** from the GpC report — unlike the core report's default (threshold 0) which emits all-zero positions. This is why the observed GpC report has far fewer lines than the core report.

### 3.4 GpC filenames (`gpc.rs` — Perl `:789-813`)

The GpC `filehandles_func` builds names from the **raw `-o`** (`config.output_raw`), NOT the stripped stem:

- `partial = output_raw` (raw, no strip).
- `--split_by_chromosome` (and a chromosome is being processed): `partial = format!("{output_raw}.chr{name}")` (Perl `s/$/.chr${my_chr}/` — appends, same raw-`-o` doubling as the core report's split path).
- `--nome-seq`: `partial = format!("{partial}.NOMe")` (Perl `:797`).
- GpC report: `partial + ".GpC_report.txt"` (+ `.gz` if gzip).
- GpC cov: `partial + ".GpC.cov"` (+ `.gz` if gzip).
- Both prefixed with `config.output_dir` (the `"${output_dir}${file}"` concat — reuse the `report_path`-style `{dir}{name}` join).

Observed shapes (§3): `-o sample --gc` → `sample.GpC_report.txt` / `sample.GpC.cov`; `--gzip` → `+.gz`; `--split` → `sample.chrchr1.GpC_report.txt`; `--nome-seq` → `sample.NOMe.GpC_report.txt` / `sample.NOMe.GpC.cov`; `-o sample.CpG_report.txt --gc` → `sample.CpG_report.txt.GpC_report.txt`.

### 3.5 `--split_by_chromosome` writer lifecycle (GpC) (Perl `:815, :833, :949-958, :1043, :1055, :1069-1071`)

- Non-split (default): open one GpC report writer + one GpC cov writer for the whole genome up front (`filehandles_func->()` at `:815`).
- Split: on the **first** cov chromosome, open the per-chr writers (`filehandles_func->($chr)` at `:833`); on each subsequent `chr`-transition, **close the old writers and open new ones** for the new chromosome (`:949-958`). ⚠️ **Perl latent quirk** (`:951-954`): in split mode Perl closes `GC` always but closes `GCCOV` **only when `$nome`** at the transition — so in **non-NOMe split mode the GpC cov filehandle is NOT explicitly closed between chromosomes** and is only reopened (truncating) by the next `filehandles_func` call. Net effect on **file bytes**: identical to closing then reopening (the OS flushes on reopen/truncate; each chr's `.GpC.cov` ends with that chr's lines). The Rust port should use the same **fresh truncating writer per chromosome** pattern as `flush_split_chromosome` (`report.rs:400-415`) for both the GpC report and GpC cov — this yields byte-identical per-chr files without reproducing Perl's asymmetric-close bug (the bug has no output consequence; confirm with a multi-chr split golden in §9). **Do not** carry a stale writer across chromosomes.
- The GpC walk has **no context-summary file** (the summary is written once by the core report, not by `generate_GC_context_report`).

### 3.6 Edge cases

1. **`GC` at chromosome start** (`j=0` or `j=1`): bottom `tri_nt` (`seq[j-2..j+1]`) is <3 bp via `perl_substr` negative-wrap → the `len<3` guard drops the whole dinucleotide. (Observed: pos-2 GC dropped.)
2. **`GC` at chromosome end** (`j+1 == len-1`, i.e. C is the last base): top `tri_nt = seq[j+1..j+4]` is 1 bp → `len<3` → drop. (Observed.)
3. **Overlapping `GCGC`**: Perl `/(GC)/g` is **non-overlapping** (after matching a `GC` at `j`, the next match starts at `j+2`). The Rust walk must also be **non-overlapping** — step the scan index past the matched `GC` (`j += 2` on a match, `j += 1` otherwise), NOT a simple `for j in 0..len-1` that would double-count `GCGC` → two matches at `j=0` and `j=2` (which IS what non-overlapping gives) but would wrongly also match at `j=1` if `seq[1..3]=="GC"`. ⚠️ Confirm the exact Perl `pos()` advancement: after `/(GC)/g` matches at offset `j`, `pos` is `j+2` and the next search resumes at `j+2`, so `GCGC` (idx 0-3) yields matches at `j=0` and `j=2` — **non-overlapping, but consecutive `GC`s are both found**. Implement as a `while j+1 < len { if seq[j]==G && seq[j+1]==C { …; j += 2 } else { j += 1 } }` loop. (Pin with a `GCGC` unit test.)
4. **Empty cov file**: Perl's GpC function has no "no last chromosome" die (that die is in the core report). With an empty cov, the core report already errors `EmptyCoverageInput` *before* the GpC walk runs (the GpC walk is only reached after the core report returns Ok). So an empty cov can never reach `gpc.rs`. (No new guard needed; document.)
5. **`config.nome` with a non-CG GpC site that is uncovered**: dropped by the `>= gpc_threshold` (≥1) guard before the NOMe filter — fine.
6. **Multi-FASTA / cov-chromosome-not-in-genome**: `Genome::get` returns `None` → no bytes for that chromosome (matches Perl's empty `while ($chromosomes{$chr} =~ /(GC)/g)` over an undef → no iterations; Perl would `warn` "uninitialized" to STDERR — exempt).

## 4. Signatures

```rust
// cli.rs — ResolvedConfig gains:
pub gc_context: bool,   // --gc / --gc_context (also set by --nome-seq)
pub nome: bool,         // --nome-seq

// cli.rs validate(): drop the --gc / --nome-seq UnsupportedFlag arms; add:
//   if self.nome_seq && self.cx_context  -> Err(NomeWithCx)
//   if self.nome_seq && self.merge_cpgs  -> Err(NomeWithMerge)
//   let gc_context = self.gc || self.nome_seq;
//   threshold: if self.nome_seq && self.threshold.is_none() { 1 }
//              else { existing resolution (Some(0) already rejected above) }

// gpc.rs
/// Generate the GpC-context report + cov (Perl generate_GC_context_report).
/// Re-reads the coverage file; walks every GC dinucleotide of each covered
/// chromosome's genome sequence. Effective threshold = max(config.threshold, 1).
pub fn run_gpc(config: &ResolvedConfig, genome: &Genome) -> Result<(), BismarkC2cError>;

// gpc.rs internal: the shared per-dinucleotide kernel (mirrors emit_position).
// Appends report bytes to `report_out` and cov bytes to `cov_out`.
#[allow(clippy::too_many_arguments)]
fn emit_gpc_dinucleotide(
    name: &[u8], seq: &[u8], j: usize,
    buffer: &HashMap<u32, (u32, u32)>,
    nome: bool, gpc_threshold: u32,
    report_out: &mut Vec<u8>, cov_out: &mut Vec<u8>,
);

// gpc.rs filename helpers (raw-`-o` based; reuse output_dir join):
fn gpc_report_path(config, chr: Option<&[u8]>) -> PathBuf;  // {dir}{raw}[.chr{c}][.NOMe].GpC_report.txt[.gz]
fn gpc_cov_path(config, chr: Option<&[u8]>) -> PathBuf;      // {dir}{raw}[.chr{c}][.NOMe].GpC.cov[.gz]

// report.rs — NOMe-aware core report (additions, not a rewrite):
//   emit_position(...) gains a `nome: bool` arg (ACG/TCG upstream filter) and a
//   `cov_out: Option<&mut Vec<u8>>` (the NOMe .cov companion). When nome &&
//   context==Cg && upstream ∉ {ACG,TCG} -> return (skip). When nome and emitted,
//   also push the .cov line to cov_out.
//   report_name/report_path gain NOMe filename variants ({stem}.NOMe.CpG_report.txt
//   + {stem}.NOMe.CpG.cov); summary_name stays the non-NOMe base.
//   The uncovered pass: gate on `config.threshold == 0 && !config.nome`.

// error.rs — two new variants:
#[error("NOMe-Seq filtering only works for CpG context (drop the --CX option)")]
NomeWithCx,
#[error("NOMe-Seq filtering does not work with --merge_CpGs (some positions are filtered out)")]
NomeWithMerge,
```

## 5. Implementation outline (TDD-friendly)

1. **`cli.rs`**: drop the `--gc`/`--nome-seq` rejection arms; add `NomeWithCx`/`NomeWithMerge` checks; resolve `gc_context = gc || nome_seq`; resolve the NOMe threshold default (1 when nome & unset); add `gc_context`/`nome` to `ResolvedConfig`. Unit-test: `--nome-seq` sets `gc_context` + threshold 1; `--nome-seq --CX` → `NomeWithCx`; `--nome-seq --merge_CpGs` → `NomeWithMerge`; `--nome-seq --coverage_threshold 0` → still `ThresholdNotPositive`; `--nome-seq --coverage_threshold 5` → threshold 5; `--gc` alone → `gc_context` true, threshold 0; `--drach`/`--ffs` still rejected.
2. **`gpc.rs` primitives + kernel**: the `GC`-walk index loop (non-overlapping, §3.6.3) + `emit_gpc_dinucleotide` (the bottom-then-top emit with the two `len<3` + classify-both guards + NOMe CG-skip). Unit-test against the live-Perl anchors (§3 fixtures): the `AGCAGC…` GpC report bytes, the `GCAGCTTAGC` edge-drops, a `GCGC` overlap, the NOMe CG-skip.
3. **`gpc.rs` filenames**: `gpc_report_path`/`gpc_cov_path` (raw-`-o` + `.chr` + `.NOMe` + suffix + gz). Unit-test every observed shape (§3.4).
4. **`gpc.rs` driver** `run_gpc`: per-chr streaming (single-file vs split, mirroring `report.rs:run_single`/`run_split` minus the summary + minus the uncovered pass), two `ReportWriter`s (report + cov), `gpc_threshold = max(config.threshold,1)`.
5. **`report.rs` NOMe core changes**: thread `nome` + the `.cov` companion through `emit_position`/`chromosome_report_bytes`/`run_single`/`run_split`; add the `.NOMe.*` filenames; gate the uncovered pass on `!config.nome`. Unit/golden-test the NOMe core report + `.NOMe.CpG.cov` (the `TTACGTTAGCATCGTT` fixture) + the empty-NOMe-core case.
6. **`lib.rs`**: after `report::run_report` (and the always-on summary it writes), add `if config.gc_context { gpc::run_gpc(config, &genome)?; }`. (Merge stays gated on `merge_cpgs`; NOMe ✗ merge so they never co-occur.)
7. **Goldens + integration tests** (§9): a new `tests/data/phase1/` dir + a `tests/golden_phase1.rs`, with a `generate_goldens.sh` block (mirroring `phase_d`) that regenerates every golden from the repo Perl v0.25.1.

## 6. Efficiency

- The GpC walk is a **second full genome pass** (Perl does the same — it re-reads the cov + re-walks the genome). O(genome length + cov lines). The genome is already in RAM (`Genome` is passed by reference — no reload). Per-chromosome cov buffering is O(covered positions per chr), freed on transition. No full-row buffering. Matches the v1.0 single-threaded posture (SPEC §10.7); a parallel walk is out of scope (epic §2).
- The cov file is read **twice** total when `--gc` (once for the core report, once for the GpC report) — exactly as Perl does. Acceptable; matches byte-identity baseline.

## 7. Integration

- **Reads:** the genome (`Genome`, shared, in-RAM) + the coverage file (re-opened for the GpC pass). **Writes (new):** `{raw}.GpC_report.txt[.gz]` + `{raw}.GpC.cov[.gz]` (always, with `--gc`); with `--nome-seq` also the core `{stem}.NOMe.CpG_report.txt[.gz]` + `{stem}.NOMe.CpG.cov[.gz]` (instead of the plain `{stem}.CpG_report.txt`) and the GpC `.NOMe.*` variants.
- **Order relative to other steps:** Perl runs the core report (`:44`) → context summary (`:49`) → `--merge_CpGs` (`:58`) → GpC report (`:82`). ⚠️ The GpC report runs **after** the merge post-pass. Since `--nome-seq` ✗ `--merge_CpGs` and `--gc` alone may co-occur with `--merge_CpGs`, the Rust order in `lib::run` must be: `run_report` → `if merge_cpgs { run_merge }` → `if gc_context { run_gpc }`. (Currently `lib::run` is `run_report` → `if merge_cpgs { run_merge }`; append the `gc_context` arm last.)
- **Downstream:** none in-scope. The extractor inline-switch is unaffected (it drives these flags via subprocess argv if at all).
- **Internal contract:** the GpC report does NOT re-read the core report (unlike merge) — it re-reads the **cov file** + the genome, so it is independent of the core report's line format.

## 8. Assumptions

**From epic (shared):**
1. Byte-identity to Perl v0.25.1 for every new/changed output stream (STDERR exempt).
2. Reuse v1.0 infrastructure (`genome.rs`, `cov.rs`, `ReportWriter`, `ResolvedConfig`/`validate`, the `--gzip`/`--zero_based`/`--split_by_chromosome`/`-o`/`--dir`/`--parent_dir` machinery); flip the two flags rejected→supported.
3. Built on merged v1.0.
4. Testing model: local Perl-v0.25.1 goldens on tiny fixtures + the oxy real-data gate (Phase 4).
5. Niche-flag interactions mirror Perl `process_commandline`: `--nome-seq` sets `--gc`, sets threshold 1, ✗ `--CX`, ✗ `--merge_CpGs`.

**Phase-1 specific:**
1. The GpC report/cov filenames derive from the **raw `-o`** (`output_raw`), NOT the stripped stem — confirmed by live Perl (§3).
2. The GpC report/cov **never** apply `--zero_based` (no `$zero` branch in `generate_GC_context_report`) — confirmed by reading the Perl; pin with a golden.
3. `gpc_threshold = max(config.threshold, 1)`; the GpC walk drops uncovered (`0,0`) positions; the GpC report has **no uncovered-chromosome pass** (covered chromosomes only).
4. The NOMe core `.cov` percentage and the GpC `.cov` percentage are `%.6f` (Rust `{:.6}`) of `meth/(meth+nonmeth)*100` — same as the merge `%.6f` parity already verified in Phase D; re-confirm on a golden.
5. The context summary is written **once** by the core report, with the **non-NOMe** base name, and reflects all contexts of the core walk (unchanged by `--gc`/NOMe).
6. `%.2f`/`%.6f` Rust↔Perl formatting parity holds (established in Phases B/D).

## 9. Validation

Goldens from the repo Perl v0.25.1 (local, this session's fixtures). The `generate_goldens.sh` phase1 block regenerates all of them.

| # | Verify | How | Expected |
|---|--------|-----|----------|
| V1 | CLI: `--nome-seq` resolution | unit: `--nome-seq` → `gc_context` true, `nome` true, threshold 1; `--gc` alone → `gc_context` true, threshold 0 | exact `ResolvedConfig` |
| V2 | CLI: NOMe mutexes | unit: `--nome-seq --CX` → `NomeWithCx`; `--nome-seq --merge_CpGs` → `NomeWithMerge`; `--nome-seq --coverage_threshold 0` → `ThresholdNotPositive`; `--nome-seq --coverage_threshold 5` → threshold 5 | typed errors / value |
| V3 | CLI: `--drach`/`--ffs` still rejected | unit | `UnsupportedFlag` |
| V4 | **GpC report golden (`--gc`)** | run on the `AGCAGCGCATGCGGCATTAGCTAGC` fixture; diff `*.GpC_report.txt` + `*.GpC.cov` vs Perl golden | byte-identical (bottom-before-top; CG-context GpCs present) |
| V5 | `--gc` emits the core report too | same run; `*.CpG_report.txt` present + identical to a plain (no-`--gc`) core run | byte-identical (incl. uncovered all-zero CpGs — core threshold 0 unaffected by `--gc`) |
| V6 | **GpC chromosome-edge guards** | `GCAGCTTAGC` fixture | first/last `GC` dropped; only interior `GC` emits (`4 - CTG`, `5 + CTT`) vs Perl golden |
| V7 | **GpC `GCGC` non-overlapping walk** | a `…GCGC…` fixture (covered) | both consecutive `GC`s found, none double-counted, vs Perl golden |
| V8 | `--gc --gzip` | decompress `*.GpC_report.txt.gz` + `*.GpC.cov.gz` → == plain goldens; summary plain | byte-identical |
| V9 | `--gc --split_by_chromosome` (multi-chr) | 2-chromosome fixture; per-chr `*.chr<N>.GpC_report.txt`/`.GpC.cov` | byte-identical to Perl per-chr goldens (the `.chrchr1` doubling + the §3.5 writer lifecycle) |
| V10 | **`--gc --zero_based` == `--gc`** | run both; diff the GpC report + cov | identical (GpC has no zero_based branch) |
| V11 | **NOMe core report golden** | `TTACGTTAGCATCGTT` fixture, `--nome-seq` | `*.NOMe.CpG_report.txt` keeps only ACG/TCG-upstream CpGs (pos 4,5,13) + `*.NOMe.CpG.cov` companion, vs Perl golden |
| V12 | **NOMe drops non-ACG/TCG CpGs** | `AGCAGCGCATGCGGCATTAGCTAGC` fixture, `--nome-seq` | `*.NOMe.CpG_report.txt` empty; `*.NOMe.GpC_report.txt` keeps only the CHH GpC (CG GpCs dropped), vs Perl golden |
| V13 | NOMe summary filename | any `--nome-seq` run | `{stem}.cytosine_context_summary.txt` (NO `.NOMe` infix), content == the all-context summary |
| V14 | NOMe skips uncovered chromosomes | a 2-chr genome, cov only on chr1, `--nome-seq` | core NOMe report has chr1 lines only (no chr2 all-zero); GpC report chr1 only |
| V15 | raw-`-o` GpC filename | `-o foo.CpG_report.txt --gc` | `foo.CpG_report.txt.GpC_report.txt` + `foo.CpG_report.txt.GpC.cov`; core report `foo.CpG_report.txt` (single-stripped) |
| V16 | NOMe `.cov` no division-by-zero | the `0,0`-at-uncovered case is dropped by threshold≥1 before the `.cov` write | no panic; absent from `.cov` |
| V17 | regression: v1.0 (Phases A–E) unaffected | full suite | green; a plain run (no `--gc`/`--nome-seq`) writes NO `.cov` / `.GpC.*` / `.NOMe.*` files |

## 10. Questions or ambiguities

| Priority | Question | Resolution / assumption |
|----------|----------|-------------------------|
| Resolved | Does `--gc` also emit the normal core report + summary? | **Yes** — Perl `:44`/`:49` run unconditionally; GpC is *additional* (`:82`). Confirmed by fixture (§3). |
| Resolved | GpC filename base — raw `-o` or stripped stem? | **Raw `-o`** (`output_raw`). `$cytosine_out` is the raw option value; the "cleaned" comment is misleading. Confirmed (`-o foo.CpG_report.txt` fixture). |
| Resolved | Does the GpC report honour `--zero_based`? | **No** — `generate_GC_context_report` has no `$zero` branch; coords are always 1-based `pos`/`pos-1`. Pinned by V10. |
| Resolved | Where does the `--gc` threshold bump (0→1) apply? | **Inside `generate_GC_context_report` only** (`:758-761`), after the core report ran. Core report (with `--gc` but no NOMe) stays at threshold 0; the GpC walk uses `max(threshold,1)`. The NOMe path sets threshold 1 at CLI time, before the core report. Pinned by V5. |
| Resolved | GpC strand emit order? | **Bottom (`-`) before top (`+`)** (Perl `:917-939`). Confirmed by fixture. |
| Resolved | NOMe ACG/TCG filter — on the C's own context or its upstream trinucleotide? | The **upstream trinucleotide** (`upstream_context`, Perl `:388/:631`), `ACG` or `TCG`, for both strands (`-` strand upstream is revcomp'd). Confirmed (pos 5 `-` kept via revcomp upstream `ACG`). |
| Resolved | NOMe uncovered chromosomes? | **Skipped entirely** (Perl `:708-713`) for the core report; the GpC report is covered-only regardless. Pinned by V14. |
| Resolved | NOMe `.cov` companion for the core report? | **Yes** — `{stem}.NOMe.CpG.cov[.gz]`, written only in NOMe mode (Perl opens `CYTCOV` only when `$nome`). Confirmed by fixture. |
| Open (non-critical) | Phase 3 (FFS) composition with the NOMe core `.cov` | Perl nests `if($tetra){…}else{ if($nome){…}else{…} }` — FFS columns and the NOMe `.cov` are mutually-exclusive *branches* in Perl (a NOMe run never emits FFS columns; an FFS run never emits the NOMe `.cov`). v1.0 has no FFS, so this phase emits the NOMe `.cov` unconditionally inside the CpG branch. When Phase 3 lands, keep them as sibling branches. **No action this phase; documented for Phase 3.** Assumption: `--ffs` + `--nome-seq` together is a real but untested Perl combo — flag to Felix if Phase 3 needs it. |
| Open (non-critical) | Does Perl's split-mode non-NOMe `GCCOV`-not-closed quirk (`:951-954`) ever change output bytes? | Reasoned **no** (reopen/truncate flushes); the Rust fresh-writer-per-chr (§3.5) sidesteps it. Pinned by the V9 multi-chr split golden — if V9 ever diverges, revisit. |

**No Critical ambiguities remain** — every GpC/NOMe behavior above is either read from the Perl with a line-ref or confirmed against live Perl v0.25.1 on a fixture this session. The plan does not invent behavior.

## 11. Self-Review

- **Efficiency:** GpC is a second O(genome+cov) pass (Perl-faithful); genome shared by reference (no reload); per-chr cov buffer freed on transition; no full-row buffering. (§6.)
- **Logic:** the `GC` walk mirrors Perl's `/(GC)/g` `pos=j+2` exactly; one shared kernel for the covered-chr + last-chr blocks (avoiding the dual-driver trap, per the dedup memory). Bottom-before-top order, the two `len<3` guards skipping the whole dinucleotide, both-strand context classification, and the NOMe CG-skip all line-ref'd. Core NOMe changes are surgical additions to `emit_position` + filename helpers, not a rewrite.
- **Edge cases:** GC-at-start (§3.6.1), GC-at-end (§3.6.2), `GCGC` overlap (§3.6.3, the highest-risk arithmetic — pinned V7), empty cov (can't reach gpc.rs — §3.6.4), cov-chr-not-in-genome (§3.6.6), NOMe div-by-zero (impossible by threshold≥1 — V16), NOMe summary filename (V13), raw-`-o` doubling (V15).
- **Integration:** `lib::run` order `report → merge → gpc` (GpC last, per Perl `:82`); plain runs emit no new files (V17 regression). The GpC report re-reads the cov + genome (independent of the core report's bytes — no internal-format contract, unlike merge).
- **Risks:** (a) the `GC` `pos=j+2` arithmetic + non-overlapping walk is the highest-risk port — mitigated by V4/V6/V7 against live Perl; (b) the GpC-has-no-`--zero_based` claim — pinned by V10 (a surprising asymmetry vs the core report, so explicitly tested); (c) `%.6f` parity — re-confirmed on V4/V11 goldens (already proven for merge in Phase D); (d) the split-mode writer lifecycle quirk — sidestepped + V9-pinned.

## Revision history
- **rev 0** (2026-05-31): initial Phase 1 plan from the v1.x EPIC + v1.0 SPEC rev 3 + Perl `generate_GC_context_report:751-1073` / core-report NOMe hooks / `process_commandline:2147-2161` + **live-Perl-v0.25.1 fixture confirmation** of every GpC/NOMe filename, format, ordering, and edge guard. Awaiting manual review.
