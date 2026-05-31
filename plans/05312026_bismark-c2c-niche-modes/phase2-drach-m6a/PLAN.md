# Phase 2 PLAN — `--drach` / `--m6A` (DRACH-motif m6A filtering)

**Epic:** `05312026_bismark-c2c-niche-modes/EPIC.md`, Phase 2 — DRACH m6A (`--drach`/`--m6A`)
**Design contract:** the v1.0 [`../../05292026_bismark-coverage2cytosine/SPEC.md`](../../05292026_bismark-coverage2cytosine/SPEC.md) (rev 3) — §6 (genome reader), §7.1 (coordinate arithmetic), §7.5 (chromosome ordering), §5/§10.5 (output topology, gzip/split writers), §3/§10.2 (CLI/validate), §10.6 (errors).
**Perl ground truth:** `coverage2cytosine` v0.25.1 — `generate_DRACH_report` (`:1075-1201`), `drach_filtering_top_strand` (`:1207-1289`), `drach_filtering_bottom_strand` (`:1291-1383`), the main-flow hook (`:38-42`), and `process_commandline` (`:2188-2194` threshold auto-set; `:2028` flag spec).
**Status:** rev 3 (2026-05-31) — **IMPLEMENTED** (155 crate tests green, fmt+clippy clean, all 12 modes Rust≡Perl v0.25.1 byte-identical; see "Implementation notes" below). Awaiting dual code-review + plan-manager. rev 2 — **dual plan-review folded** (A: APPROVE-WITH-CHANGES 1 Crit/4 Imp; B: APPROVE-WITH-CHANGES 0 Crit/3 Imp; both reproduced every claim on live Perl). **Q1 RESOLVED by Felix:** BS-seq is cytosine-specific, so the bottom-strand C anchor (`pos-1`) is the intended position — DRACH ports **byte-identical to Perl v0.25.1 on both strands**. rev 2 hardened the chromosome-start handling (the **top** strand also negative-wraps at `pos<4` and emits — both strands now mandate `perl_substr`), the `is_drach_motif` short-slice safety, the general-mutex preservation, the empty-cov guard, and the validation matrix (V15/V16 + reworded V7/V10). **Ready for the implement trigger** (see revision history for the full fold). No logic defect found.

---

## 1. Goal

Add `--drach` / `--m6A`: a **standalone early-exit mode** that scans the genome for DRACH 5-mers (the m6A sequence context `D-R-A-C-H`, where `D∈{A,G,T}`, `R∈{A,G}`, `H∈{A,C,T}`), looks up each motif's measured C in the coverage map, and writes a DRACH-filtered cytosine report + coverage file — then **exits without producing the normal cytosine report**. Flip `--drach`/`--m6A` from the v1.0 CLI-**rejected** state to **supported**. **Byte-identical to Perl v0.25.1** (both strands).

**On the Perl `// TODO` (`:1369`, *"Need to determine the position of the C involved correctly!"*):** the bottom strand reports the measured C at `$pos-1`. My derivation (§3.6) — cross-validated against live Perl and the main `--CX` report — shows `$pos-1` is the **correct genomic 1-based coordinate of the bottom-strand C**. **Felix confirmed (2026-05-31): because BS-seq is cytosine-specific, anchoring on the C (not the m6A `A`) is the intended behavior — `pos-1` is correct.** The `// TODO` is a resolved/vestigial doubt; the Rust reproduces `pos-1` and is byte-identical to Perl. (See §3.6 + §10 Q1.)

## 2. Context

- **Where the code lives:** a new module `drach.rs` in `rust/bismark-coverage2cytosine/src/`, invoked from `lib::run` as an **early-exit branch** (mirror Perl `:38-42`): if `config.drach`, run `drach::run_drach(config, &genome)?` and **return `Ok(())`** *before* `report::run_report` / context summary / merge. Reuses Phase A/B/C infrastructure:
  - `genome::Genome` (§6 reader: uppercased, `Mus` skip, four-suffix glob, dup-name error) — the same in-RAM map the main report walks.
  - The Phase-B per-chromosome **cov buffering** (`CovMap = HashMap<u32,(u32,u32)>`, flush-on-`chr`-transition, insertion-ordered covered-chromosome list — §7.5). DRACH is **covered-chromosomes-only** (Perl never runs an uncovered pass in `generate_DRACH_report` — §3.5), so no `names_sorted()` uncovered pass.
  - The Phase-C **`ReportWriter`** (plain or `GzEncoder` for `--gzip`) and the per-chromosome truncating-writer reopen for `--split_by_chromosome`.
  - `BismarkC2cError`, `ResolvedConfig`, `validate()`.
- **Depends on:** v1.0 merged (EPIC §3 Precondition) — Phases A (genome reader + CLI + errors), B (cov parse + per-chr buffering + the `tr/ACTG/TGAC/` complement helper + `pos = i+1` convention), C (gzip + split writers). **Independent of Phase 1 (`--gc`/`--nome`) and Phase 3 (`--ffs`).**
- **Reuse, don't re-derive:** the §7.1 coordinate arithmetic, the `tr/ACTG/TGAC/` 4-byte identity-elsewhere complement (SPEC P7), and the gz/split writer plumbing are already shipped in Phase B/C. DRACH adds a *new motif walk + 5-mer filter*, not new infrastructure.

## 3. Behavior

### 3.0 Standalone early-exit + flag interactions (Perl `:38-42`, `:2188-2194`) — empirically confirmed

1. **Standalone early-exit.** With `--drach`, Perl reads the genome, runs `generate_DRACH_report`, and `exit 0` — it **never** writes the normal `.CpG_report.txt`/`.CX_report.txt`, the `.cytosine_context_summary.txt`, or runs `--merge_CpGs`. The Rust `lib::run` branches to `drach::run_drach` and returns before any of those.
2. **`--drach` silently ignores normal-report flags.** Combining `--drach` with `--CX` or `--merge_CpGs` does **NOT die** — they are accepted and then ignored (early exit). Verified. **So `validate()` must NOT add a mutex** between `--drach` and `--CX`/`--merge_CpGs`; it only flips `--drach`/`--m6A` from the v1.0 `UnsupportedFlag` rejection to accepted. ⚠️ **rev 2 (A-F2): un-rejecting `--drach` must NOT bypass or reorder the pre-existing GENERAL mutexes.** Perl runs them in `process_commandline` *before* the `:38` early-exit, so they still fire with `--drach`: live-Perl-verified that `--drach --CX --merge_CpGs` **dies** (CX×merge, `:2140`) and `--drach --merge_CpGs --coverage_threshold 5` **dies** (threshold×merge, `:2176`). The current Rust `validate()` already enforces these and Phase 2 keeps them (correct by inheritance) — but do **not** add an early `if self.drach { … }` short-circuit inside `validate()` that would skip the CX×merge / threshold×merge / nome×merge / disco-requires-merge checks. Pin with a unit asserting `--drach --merge_CpGs --coverage_threshold 5` still errors (V1).
3. **`--drach` ignores `--zero_based`.** The DRACH subs never reference `$zero`; positions are **always 1-based**. Verified (`--drach --zero_based` == `--drach`). The Rust must NOT apply the zero-based `pos -= 1` in the DRACH path.
4. **`--drach` auto-sets `threshold = 1`** unless the user passed `--coverage_threshold > 0` (Perl `:2188-2194`). So an uncovered DRACH motif (lookup miss → `0`) is skipped (`>= threshold`, threshold ≥1). This auto-set happens in `validate()`/resolution, after the generic threshold resolution (explicit `--coverage_threshold 5` survives; default `0` → `1`). **`--drach` honors `--gzip` and `--split_by_chromosome`.**

### 3.1 Output topology (Perl `:1114-1131`)

`{raw-o}` = the **raw `-o` value, NOT suffix-stripped** (⚠️ divergence from the main report: the DRACH `filehandles_func` uses `$cytosine_out` verbatim — no `.CpG_report.txt`/`.CX_report.txt` strip; the `:1105` "Assume cleaned" comment is inaccurate — verified by grepping every use).

| File | When | gzip? | Format |
|------|------|-------|--------|
| `{raw-o}_DRACH_report.txt[.gz]` | always (default, single file) | `--gzip` | 7-col DRACH report (§3.4) |
| `{raw-o}_DRACH.cov[.gz]` | always | `--gzip` | 6-col cov (§3.4) |
| `{raw-o}.chr<NAME>_DRACH_report.txt[.gz]` | `--split_by_chromosome` | `--gzip` | per-chr report |
| `{raw-o}.chr<NAME>_DRACH.cov[.gz]` | `--split_by_chromosome` | `--gzip` | per-chr cov |

**No header line** in either file (verified — first line is data). The `.chr<NAME>` infix is appended to `{raw-o}` *before* `_DRACH_report.txt` (Perl `:1115`), so a chromosome literally named `chr1` yields `{raw-o}.chrchr1_DRACH_report.txt` (same `.chr`+name pattern as the main split report). The `output_dir` prefix applies as in Phase C.

### 3.2 Top-strand walk (`drach_filtering_top_strand`, Perl `:1207-1289`) — BYTE-IDENTICAL

For each chromosome (covered order, §3.5), scan the uppercased sequence for every `AC` (Perl `/(AC)/g`; Rust `seq[i]==b'A' && seq[i+1]==b'C'`, `i` from 0):

1. `pos = i + 2` (1-based; two-char `AC` match → `pos()` is `i+2`). **The measured C is the `C` of `AC`, at 1-based `pos`** — Perl reports `$pos` (`:1285-1286`), which IS the C's 1-based coordinate. (Verified: fixture `GAACA`, AC at idx2-3, `pos=4` = the C; the `--CX` report independently puts a `+`-C at pos 4 — they agree.) The `AC`/`GT` scan advances by `+1` (`i` from 0); these distinct-byte 2-mers cannot self-overlap, so an `i += 1` scan yields the identical match set to Perl's `/(AC)/g` `pos()`-advance (rev 2, A-F4/B-Opt1 — do **not** copy `gpc.rs`'s `j += 2`).
2. `tri_nt_top = perl_substr(seq, pos-1, 3)` (C + next 2; `tri[0]==C`). (Perl `:1224`.) ⚠️ **Use `perl_substr` (isize offset), NOT a naive `seq[pos-1..pos+2]` slice** — see step 3's chromosome-start case.
3. `drach_top = perl_substr(seq, pos-4, 5)` (5-mer, C at offset 3). (Perl `:1225`.) ⚠️ **rev 2 (A-F1, CRITICAL): the TOP strand also has a chromosome-start negative-`substr` wrap, and unlike the bottom strand it EMITS.** For an `AC` at `pos = 2` or `3`, the offset `pos-4` is **negative** (`-2`/`-1`); Perl's `substr` wraps it from the string **end**, while `tri_nt_top` (offset `pos-1 ≥ 1`) stays positive and can be full length ≥ 3 → the position clears the `len<3` guard and **emits a real line** whose `drach_5mer` column is the wrapped fragment. **Live-Perl-verified:** `ACAAA` cov@2 → `chrA 2 + 9 1 AA CAA` (`drach=substr("ACAAA",-2,5)="AA"`). So `drach_top` (and `tri_nt_top`) **must** be extracted via the Phase-B `perl_substr` helper (`report.rs`, isize offset, models Perl's from-end wrap) — a naive `seq[pos-4..]` would `usize`-underflow → **panic**, or if clamped to 0 would yield the wrong prefix → byte-divergence. (Identical mandate to the bottom strand, §3.3 step 6.) Pinned by the new top-strand `pos<4` golden (§9 V15).
4. **DRACH filter** (Perl `:1246-1273`) on the **forward** 5-mer `drach_top`: keep iff `drach[0] != 'C'` (pos1 = `D`), **and** `drach[1] ∈ {A,G}` (pos2 = `R`), **and** `drach[4] != 'G'` (pos5 = `H`, excludes only G — note a non-ACGT pos-5 byte *passes*; reproduce the literal `!= 'G'` byte-test). Else skip. Positions 3 (`A`) + 4 (`C`) are guaranteed by the `AC` match. ⚠️ The 5-mer can be **short** (the chromosome-start wrap of step 3, or a chromosome-end truncation): `is_drach_motif` must use `.get(0)/.get(1)/.get(4)` with Perl-`substr`-empty semantics — **pos-0 missing → "" ne 'C' → passes; pos-1 missing → "" ∉ {A,G} → fails; pos-4 missing → "" ne 'G' → passes** — never index `byte[0]`/`byte[1]` directly (would panic on a <2-byte slice from the wrap). (rev 2, A-F8.)
5. **Guard** `tri_nt.len() < 3` → skip (Perl `:1275`). Perl applies the DRACH filter (4) **before** this guard (5); the relative order is **not byte-observable** (both are side-effect-free skips with no counter/partial-write before the `next`), so keeping the Perl order is a defensive default, not a byte-identity requirement (rev 2, B-Opt2). A near-end `AC` whose `drach_top` runs off the end is **short** (<5); Perl's `substr($drach,4,1)` of a <5 string returns `''`, and `'' ne 'G'` is **true** → pos-5 test *passes* on a truncated 5-mer (handled by step 4's `.get(4)` semantics).
6. **Coverage lookup** at key `pos` (the C's 1-based coordinate): `(meth, nonmeth) = chr_map.get(pos).unwrap_or((0,0))` (Perl `:1278-1281`).
7. **Threshold + emit** (Perl `:1283-1287`): if `meth + nonmeth >= threshold`:
   - `pct = format!("{:.6}", meth/(meth+nonmeth)*100)`.
   - **DRACHCOV:** `chr \t pos \t pos \t pct \t meth \t nonmeth \n`.
   - **DRACH:** `chr \t pos \t '+' \t meth \t nonmeth \t drach_top \t tri_nt_top \n`.

### 3.3 Bottom-strand walk (`drach_filtering_bottom_strand`, Perl `:1291-1383`) — BYTE-IDENTICAL

Scan every `GT` (Perl `/(GT)/g`; Rust `seq[i]==b'G' && seq[i+1]==b'T'`). The bottom-strand DRACH is the reverse-complement of a top-strand window:

1. `pos = i + 2` (1-based). The `G` of `GT` is at 0-based `i = pos-2` = **1-based `pos-1`**.
2. `tri_nt_bottom = substr(seq, pos-4, 3)` then `tr/ACTG/TGAC/` + `reverse` (Perl `:1306,1311-1312`) = `revcomp(seq[pos-4 .. pos-1])`; `tri[0]` = `complement(G of GT)` = the bottom-strand **C**.
3. `drach_bottom = substr(seq, pos-3, 5)` then `tr/ACTG/TGAC/` + `reverse` (Perl `:1307-1309`) = `revcomp(seq[pos-3 .. pos+1])`; `drach_bottom[3]` (the DRACH C at offset 3) = the same bottom-strand C. (Self-consistent: `tri[0] == drach[3] == the C`.)
4. **DRACH filter** — identical structure to the top strand (Perl `:1335-1363`), on the **transformed** `drach_bottom`: `drach[0] != 'C'`, `drach[1] ∈ {A,G}`, `drach[4] != 'G'`; else skip. Same truncation/`tr` semantics as §3.2 (complement leaves non-ACGT unchanged — SPEC P7).
5. **Guard** `tri_nt.len() < 3` → skip (Perl `:1366`).
6. **Coverage lookup + reported position — at `pos-1`** (Perl `:1371-1379`). The bottom-strand C occupies the same genomic base as the `G` of the `GT` match, whose 1-based coordinate is `pos-1` (§3.6). **Felix confirmed this is the intended cytosine anchor for BS-seq.** So look up `(meth, nonmeth) = chr_map.get(pos-1).unwrap_or((0,0))` and report `pos-1`. ⚠️ Chromosome-start edge: when `pos < 4`, Perl's `substr(seq, pos-4, …)` uses a **negative offset** that wraps from the string end (SPEC P3); the Rust must reproduce the Perl negative-substr wrap (reuse the Phase-B `perl_substr` helper) — pin in a test (V7). **rev 2 (B-Important-1) — `perl_substr` offset bound:** the most-negative offset DRACH ever passes is `pos-4` with `pos = i+2 ≥ 2`, i.e. `≥ -2`; and any chromosome containing an `AC`/`GT` has length `≥ 2`, so the offset is always `≥ -len`. The shipped `perl_substr` matches Perl exactly for `offset ≥ -len` (verified `substr("GT",-2,3)="GT"`, `substr("GT",-1,5)="T"`); its **known divergence at `offset < -len`** (Perl shrinks `want` by the overshoot — `substr("ACGT",-5,3)="AC"` — whereas the Rust helper clamps `start` to 0 and keeps `want`, returning `"ACG"`; its `report.rs` unit test asserts the Rust value) is therefore **unreachable from DRACH**. No `perl_substr` change is needed for this phase; the latent helper bug is tracked separately (out of Phase-2 scope).
7. **Threshold + emit** (Perl `:1376-1379`): if `meth + nonmeth >= threshold`:
   - `pct = format!("{:.6}", meth/(meth+nonmeth)*100)`.
   - **DRACHCOV:** `chr \t pos-1 \t pos-1 \t pct \t meth \t nonmeth \n`.
   - **DRACH:** `chr \t pos-1 \t '-' \t meth \t nonmeth \t drach_bottom \t tri_nt_bottom \n`.

### 3.4 Output line formats (both strands)

- **`_DRACH_report.txt`:** `chr \t pos \t strand \t meth \t nonmeth \t drach_5mer \t tri_nt \n` (7 cols; `strand` = `+`/`-`).
- **`_DRACH.cov`:** `chr \t pos \t pos \t pct \t meth \t nonmeth \n` (6 cols; both position columns equal; `pct` = `%.6f` recomputed from `meth`/`nonmeth` — the cov-file's own column-4 percentage is **ignored**, verified).

### 3.5 Chromosome processing order + per-chromosome flush (Perl `:1141-1199`)

1. Read cov lines into `%chr` (1-based `start` → `(meth, nonmeth)`), flushing on each `chr`-field transition.
2. On transition / at EOF, call `drach_filtering_top_strand(last_chr, %chr)` **then** `drach_filtering_bottom_strand(last_chr, %chr)` — **top strand fully, then bottom strand fully** (Perl `:1166-1167,1192-1193`). So within a chromosome **all `+` lines precede all `-` lines** (a byte-identity fact).
3. **No uncovered-chromosome pass** — the DRACH path iterates the cov file, not the genome; a chromosome absent from the cov emits nothing. Covered order = cov-file appearance order (insertion-ordered, SPEC P1 — **never `BTreeMap`**).
4. **Last-write-wins** on duplicate cov positions; CRLF/blank-line handling per the Phase-B cov parse policy.
5. **Empty cov input:** the DRACH path lacks the main report's `die`; it produces **empty DRACH output files** (open them, write nothing). Pin in V8 (confirm vs Perl). ⚠️ **rev 2 (A-F3 / B-Important-2) — final-flush guard.** In single-file mode the two output writers must be created **before** the read loop (so a zero-line cov still yields two 0-byte files, exit 0 — matching Perl, which opens the filehandles in `filehandles_func` up front). Perl then runs its *final* `drach_filtering_*($last_chr, %chr)` with `$last_chr` **undef** (warning `uninitialized value $chromosomes{""}`, STDERR-exempt) and walks an empty sequence → no output. The Rust driver uses the Phase-1 `cur_chr: Option<Vec<u8>>` skeleton, where the final flush is `if let Some(prev) = cur_chr.take() { … }` — so it **correctly omits** the phantom `""`-chromosome flush and must **not** `unwrap()` a never-set `last_chr` or call `genome.get("")`. Net output is byte-identical (two empty files); the guard just prevents a panic on the zero-line path. (In `--split_by_chromosome` mode, like the core report, an empty cov produces **no** per-chr files — there is no chromosome to open a writer for.)

### 3.6 The bottom-strand C position (`pos-1`) — derivation + resolution

**Setup.** A `GT` match at 0-based `(i, i+1)`; `pos = i+2`. The measured bottom-strand cytosine is the DRACH offset-3 C, which (§3.3) maps to the **`G` of the `GT`** at 0-based `i = pos-2`, i.e. **1-based `pos-1`.**

**Three cross-checks (live Perl v0.25.1, fixture `top="AAATGTTCAAAGTACGTACGT"`):**
1. **Geometry:** the bottom-strand C *is* a forward-strand `G` (the G of GT); its 1-based coordinate is `pos-1`.
2. **Main-report convention:** in `generate_genome_wide_cytosine_report`, a bottom-strand C (`$1 eq 'G'`) is keyed/reported at the G's 1-based coordinate; for the DRACH G of GT that is `pos-1`.
3. **Empirical agreement:** `--drach` and `--CX` on the same fixture put the bottom hit at the **same position (5) + same trinucleotide (CAT)**; other GT matches agree at 12/16/20.

**Resolution (Felix, 2026-05-31).** All three say `pos-1` is the correct C coordinate — exactly what Perl computes. **Felix confirmed: BS-seq is cytosine-specific, so the intended anchor is the C (not the m6A `A`), and `pos-1` is correct.** The `// TODO` was a vestigial doubt. The bottom strand is therefore a **plain byte-identical port** of Perl `pos-1` — no divergence, no separate reference. (The only residual edge to pin is the chromosome-start `pos<4` negative-substr wrap, §3.3 step 6 / V10 — a faithful reproduction of Perl, not a fix.)

## 4. Signatures

```rust
// drach.rs
/// Standalone DRACH/m6A early-exit mode (Perl generate_DRACH_report).
/// Reads the cov, buffers per chromosome, scans the genome for DRACH 5-mers on
/// both strands, writes {raw-o}_DRACH_report.txt[.gz] + {raw-o}_DRACH.cov[.gz]
/// (per-chr with --split_by_chromosome). Covered chromosomes only; always
/// 1-based; honors --gzip / --split_by_chromosome; ignores --zero_based / --CX
/// / --merge_CpGs. Byte-identical to Perl v0.25.1.
pub fn run_drach(config: &ResolvedConfig, genome: &Genome) -> Result<(), BismarkC2cError>;

/// Top strand: scan `seq` for `AC`, DRACH-filter, look up cov at the C (`pos`),
/// emit `+` lines. Byte-identical to Perl. ⚠️ Extract BOTH `tri_nt_top` and
/// `drach_top` via `perl_substr` (isize offset) — a chromosome-start `AC`
/// (`pos<4`) gives a negative `drach` offset that Perl WRAPS from the string
/// end and still EMITS (rev 2, A-F1; e.g. `ACAAA` cov@2 → `chrA 2 + 9 1 AA CAA`).
fn drach_top(/* chr, seq, cov map, threshold, writers */);

/// Bottom strand: scan `seq` for `GT`, DRACH-filter the revcomp'd 5-mer, look
/// up cov at the bottom-strand C (`pos-1`, the BS-seq cytosine anchor — confirmed
/// correct), emit `-` lines. Byte-identical to Perl (incl. the pos<4 negative-
/// substr wrap, SPEC P3).
fn drach_bottom(/* … */);

/// True iff the 5-mer is DRACH. Use `.get(0)/.get(1)/.get(4)` with Perl-substr
/// "missing byte == empty string" semantics — NEVER index `five_mer[0]` (the
/// chromosome-start wrap + chromosome-end truncation can hand this a <2-byte
/// slice; direct indexing would panic). Rules (rev 2, A-F8):
///   pos-0 (`D`): missing → "" ne 'C' → PASS;  present → `!= b'C'`
///   pos-1 (`R`): missing → "" ∉ {A,G} → FAIL; present → `== b'A' || == b'G'`
///   pos-4 (`H`): missing → "" ne 'G' → PASS;  present → `!= b'G'`
/// (pos-2 `A` / pos-3 `C` are guaranteed by the `AC`/`GT` match.) Shared by
/// both strands. Verified: non-ACGT pos-1 passes (`NAACA`), pos-2 fails (`GNACA`),
/// pos-5 passes (`GAACN`); a 2-byte `AA` passes (the top-strand wrap).
fn is_drach_motif(five_mer: &[u8]) -> bool;
```

`ReportWriter` / the gz+split reopen logic are reused from Phase C (already `pub(crate)` after Phase D); add `drach_report_path` / `drach_cov_path` (build `{raw-o}[.chr<NAME>]_DRACH_report.txt[.gz]` etc. from the **raw `-o`**, NOT the stripped stem).

## 5. Implementation outline (TDD-friendly)

1. **CLI un-rejection (`cli.rs`/`validate()`):** remove `--drach`/`--m6A` from the `UnsupportedFlag` set; add `drach: bool`; flip the `--help` "(v1.x, rejected)" label. **No `--drach` × `{--CX,--merge_CpGs}` mutex** (Perl ignores — §3.0). Implement the **threshold auto-set**: after generic threshold resolution, `if config.drach && config.threshold == 0 { config.threshold = 1; }`. Unit-test: accepted; default threshold→1; explicit 5 survives; `--drach --zero_based` resolves (path ignores zero_based).
2. **`is_drach_motif`** + unit tests (positives; each filter arm negative; truncated <5-mer pos-5 "missing → not G → pass"; non-ACGT bytes).
3. **Filename helpers** `drach_report_path`/`drach_cov_path` (raw-`-o`, `_DRACH_report.txt`/`_DRACH.cov`, `+.gz`, `.chr<NAME>` infix); unit tests incl. `.chrchr1` doubling + a **suffixed `-o`** (no strip).
4. **`drach_top`** (byte-identical): `AC` scan + `pos=i+2`, `tri_nt`/`drach` extraction, filter, `len<3` guard (after filter), cov lookup at `pos`, threshold, emit `+`. Golden vs Perl.
5. **`drach_bottom`** (byte-identical): `GT` scan + `pos=i+2`, extraction + `tr/ACTG/TGAC/`+reverse (reuse Phase-B complement + `perl_substr` for the negative-offset wrap), filter, `len<3` guard, cov lookup + report at `pos-1`, threshold, emit `-`. Golden vs Perl (incl. the chromosome-start `pos<4` edge, V10).
6. **`run_drach` driver:** per-chr cov buffering (reuse Phase-B parse), flush-on-transition + final flush, `drach_top` then `drach_bottom` per chromosome (§3.5 order), covered-chromosomes-only, insertion order. Writers per §3.1 (single file default; per-chr reopen for split); gz iff `--gzip`. Empty-cov → empty files.
7. **Wire `lib::run`:** `if config.drach { return drach::run_drach(config, &genome); }` **before** `report::run_report` (after genome load), mirroring Perl `:38-42`.
8. **Goldens + tests** (§9): a tiny multi-FASTA fixture with top-strand DRACH, bottom-strand DRACH, a chromosome-edge motif (both the truncated-end and the `pos<4`-start cases), a non-DRACH near-miss per filter arm, an uncovered motif (threshold-skip), and a 2-chromosome cov for ordering. Generate goldens from repo Perl v0.25.1.

## 6. Efficiency

- O(genome length) per chromosome per strand (two linear scans of the in-RAM uppercased sequence) + O(cov lines) parse. Same single-threaded, whole-genome-in-RAM posture as the main report (SPEC §10.7). No extra genome copy.
- Cov buffer is one chromosome at a time, flushed on transition. Writers stream line-by-line; gz via `GzEncoder`.

## 7. Integration

- **Reads:** the genome FASTA (shared `Genome`) + the positional `*.bismark.cov[.gz]`. **Writes:** `{raw-o}_DRACH_report.txt[.gz]` + `{raw-o}_DRACH.cov[.gz]` (per-chr with split). **Does NOT write** the normal report / summary / merged cov (early exit).
- **Order:** the `--drach` branch runs **instead of** the normal pipeline (early return). No downstream c2c step consumes DRACH output.
- **Internal contract:** reuses the Phase-B cov parse + complement + `perl_substr` + `pos=i+1` and the Phase-C writers. Phase 4 of this epic adds a DRACH cell to the oxy gate. The extractor inline switch does not drive `--drach`.

## 8. Assumptions

**From epic (shared — EPIC §6):**
1. Byte-identity to Perl v0.25.1 for new output streams (STDERR exempt) — **both strands** (no exception after Q1's resolution).
2. Reuse v1.0 infrastructure (`genome.rs`, cov parse, `ReportWriter` plain/gz + split, `ResolvedConfig`/`validate()`, `BismarkC2cError`, the `--gzip`/`--split_by_chromosome`/`-o`/`--dir`/`--parent_dir` machinery). Flip `--drach`/`--m6A` rejected→supported.
3. Built on the merged v1.0 (EPIC §3).
4. Local Perl-v0.25.1 goldens on tiny fixtures (this plan) + the oxy real-data gate (epic Phase 4). Worktree isolation.
5. Niche-flag interactions mirror Perl `process_commandline`.

**Phase-2 specific (verified against live Perl unless noted):**
6. `--drach` is **standalone early-exit** — no normal report / summary / merge; **does not die** when combined with `--CX`/`--merge_CpGs` (ignores them).
7. `--drach` **ignores `--zero_based`** (always 1-based).
8. `--drach` **auto-sets `threshold = 1`** unless `--coverage_threshold > 0`.
9. DRACH filenames use the **raw `-o`** (no `.CpG_report.txt`/`.CX_report.txt` strip).
10. DRACH report has **no header**; cov `pct` is **recomputed** `%.6f` (cov col-4 ignored).
11. **Covered-chromosomes-only** (no uncovered pass); within a chromosome **all `+` then all `-`**; covered order = cov-file appearance order.
12. **Both strands byte-identical to Perl** — the bottom-strand C anchor `pos-1` is the intended BS-seq cytosine position (Felix, 2026-05-31; §3.6).
13. Empty cov → empty DRACH files (no `die`) — to be confirmed in V8.
14. The DRACH filter only excludes position-5 `== 'G'`; a truncated <5-mer treats the missing position-5 as "not G" → passes (Perl `substr` semantics). Byte-level.
15. Chromosome-start `pos < 4` on the bottom strand → Perl negative-`substr` wrap (SPEC P3); reproduce faithfully via `perl_substr` (V10).

## 9. Validation

**Both strands validate by byte-identity vs Perl v0.25.1** (tiny fixtures; `generate_goldens.sh` extended with a `phase2_drach` block, regenerable from repo Perl). (The rev-0 "bottom-strand corrected reference" is dropped — Q1 resolved to `pos-1`-is-correct.)

| # | Verify | How | Expected |
|---|--------|-----|----------|
| V1 | CLI un-rejection + no spurious mutex + general mutexes preserved | unit: `--drach`, `--drach --CX`, `--drach --merge_CpGs`, `--m6A` all accepted; **AND `--drach --merge_CpGs --coverage_threshold 5` still errors** (general threshold×merge mutex not bypassed, rev 2 A-F2) | accepted; no `UnsupportedFlag`; the general-mutex combo errors |
| V2 | threshold auto-set | unit: `--drach`→1; `--drach --coverage_threshold 5`→5; order matches Perl | exact |
| V3 | `is_drach_motif` (`.get`-based, short-slice safe) | unit: positives + each filter-arm negative + **non-ACGT at pos-1 (passes) / pos-2 (fails) / pos-5 (passes)** + **0/1/2-byte slices** (pos-0 missing→pass, pos-1 missing→fail, pos-4 missing→pass; a 2-byte `AA`→pass) — must not panic on <2 bytes (rev 2 A-F8/B-3) | exact booleans, no panic |
| V4 | DRACH filenames | unit: `-o samp`→`samp_DRACH_report.txt`/`.cov`; `--gzip`; `--split` chr `chr1`→`samp.chrchr1_DRACH_report.txt`; **suffixed `-o` not stripped** | exact strings |
| V5 | **top-strand report + cov golden** | `--drach` on the fixture; diff to Perl v0.25.1 | byte-identical |
| V6 | **bottom-strand report + cov golden** | `--drach`; diff the `-` lines (incl. position) to Perl | byte-identical (`pos-1`) |
| V7 | **chromosome-start BOTTOM motif (`pos<4`)** | a `GT` near the chromosome start (e.g. `GTACGTACGT` cov@1) | **no panic; emits NOTHING** — the rc-wrapped bottom `tri` is always len<3 → len-guard-skipped (rev 2 A-F5; "byte-identical" = empty==empty, exercises the negative-substr-wrap without a crash) |
| V8 | empty cov input | `--drach` on an empty `.cov` | empty `_DRACH_report.txt` + `_DRACH.cov` (no `die`, exit 0); no panic on the never-set-`last_chr` final flush (rev 2 A-F3/B-2); confirm vs Perl |
| V9 | uncovered motif threshold-skip | a DRACH motif with no cov entry (threshold 1) | absent from both files |
| V10 | chromosome-end motif (truncated 5-mer, **bottom EMITS**) | a `GT` near the chromosome end (e.g. `AAAGTC`/`AAAGTA` cov@4) | byte-identical: the 4-byte `drach` **passes** (pos-5 missing) and tri len≥3 → **emits** `… 4 - GACT/TACT CTT` (pins the truncated-5-mer pass-AND-emit path, not just a skip) |
| V11 | `--gzip` | `--drach --gzip` → decompress → == plain goldens | byte-identical (decompressed) |
| V12 | `--split_by_chromosome` ordering | 2-chromosome cov | per-chr files; within each, all `+` then all `-`; covered order = cov appearance |
| V13 | `--zero_based` ignored | `--drach --zero_based` == `--drach` | identical (1-based) |
| V14 | regression: v1.0 + Phases 1/3 unaffected | full suite | green |
| V15 | **chromosome-start TOP motif (`pos<4`) EMITS** (rev 2 A-F1, the Critical) | `--drach` on `chrA=ACAAA` cov@2 | byte-identical golden `chrA 2 + 9 1 AA CAA` — the wrapped `drach=substr(-2,5)="AA"`; proves the top strand uses `perl_substr` (a naive slice would panic/diverge) |
| V16 | **single-file 2-chromosome ordering** (rev 2 B-3) | a single-file (non-split) cov with chromosomes in non-sorted appearance order (e.g. chr2 lines before chr1) | covered order = cov-appearance order (chr2 block before chr1 block); within each chr all `+` then all `-` — distinct from V12 (split-only) |

Goldens from repo Perl v0.25.1 (local; a per-fixture `tests/data/phase2_drach/generate_goldens.sh` modeled on `tests/data/phase1/generate_goldens.sh` — one generator per phase dir, NOT a shared script). The structural twin for `drach.rs` is **`gpc.rs`** (a covered-only both-strand motif walk with per-chr `HashMap` buffering, flush-on-transition, top@`pos`/bottom@`pos-1`, single/split/gz writers) — the closest existing model; mirror its shape.

## 10. Questions or ambiguities

| Priority | Question | Status |
|----------|----------|--------|
| **Resolved (Q1)** | What is the correct bottom-strand C position? | **`pos-1`** — Felix confirmed (2026-05-31) BS-seq is cytosine-specific, so the C (not the m6A `A`) is the intended anchor; Perl's `pos-1` is correct. My derivation (§3.6: geometry + main-report convention + live `--CX` agreement) independently shows `pos-1` is the C's 1-based coordinate. Bottom strand is a **plain byte-identical port**; the `// TODO` is vestigial. |
| Open (Q2) | Empty cov → empty files vs `die`? | §3.5.5 proposes empty files (no `die` in the DRACH path); pinned by V8. |
| Resolved (Q3) | Does the cov-file column-4 percentage matter? | No — Perl recomputes `%.6f` from `meth`/`nonmeth` (verified). |
| Resolved (Q4) | Position-5 filter is `!= 'G'` only; non-ACGT pos-5 passes? | Yes — reproduce the literal byte-level `!= b'G'` (Assumption 14; V3). |

## 11. Self-Review

- **Logic:** the two linear motif scans + per-position DRACH filter + cov lookup + threshold mirror Perl `:1207-1383` exactly on **both** strands (the bottom strand reports at `pos-1`, the confirmed-correct C anchor). Standalone early-exit + threshold-auto-set + flag-ignore are verified against live Perl. Per-chromosome flush order (top-then-bottom, covered-only, cov-appearance) is a byte-identity fact.
- **Edge cases:** chromosome-end truncated 5-mer (V10 + the "pos-5 missing → pass" substr semantics); chromosome-start `pos<4` negative-substr wrap on the bottom strand (V7, SPEC P3); uncovered motif threshold-skip (V9); empty cov (V8/Q2); non-ACGT motif bytes (V3); `.chrchr1` filename doubling + suffixed-`-o` no-strip (V4).
- **Efficiency:** two O(genome) scans + O(cov) parse, single-threaded, no extra genome copy.
- **Integration:** early-exit branch in `lib::run`; reuses Phase-B parse/complement/`perl_substr` + Phase-C gz/split writers; no downstream consumer. Adjusted §3.0 after empirically confirming `--drach` does NOT die on `--CX`/`--merge_CpGs` (no mutex added — a trap avoided).
- **Remaining risk:** low — the previously-headline Q1 (bottom-strand position) is resolved (byte-identical to Perl `pos-1`). Secondary: the empty-cov behavior (Q2) is proposed, golden-confirmed by V8 before reliance.

## Implementation notes (rev 3 — implemented 2026-05-31)

**Status: IMPLEMENTED.** Steps 1–8 complete; **155 crate tests green** (92 lib incl. 4 new `--drach` CLI + 8 new `drach.rs` kernel tests; 12 Phase-2 goldens; 18 Phase-1 + 11 B + 7 C + 10 D + 5 sanity — **no regression**); `cargo fmt --check` + `clippy --all-targets -D warnings` clean. **Not yet committed; not yet dual-code-reviewed/plan-managed** (awaiting Felix's go-ahead).

**What was built (per §5):**
- **`cli.rs`:** dropped the `--drach` arm of the v1.x rejection (kept `--ffs`); added `drach: bool` to `ResolvedConfig`; the threshold auto-set is `None if nome || self.drach => 1` (explicit value survives; explicit 0 still rejected). NO `--drach` mutex; the general merge mutexes are untouched (V1 unit asserts `--drach --merge_CpGs --coverage_threshold 5` still errors). Narrowed `rejects_v1x_flags` to `--ffs`.
- **`drach.rs` (new, ~430 ln):** `is_drach_motif` (`.get`-based / short-slice-safe); `drach_top` (`AC` scan, **both `tri_nt` and the 5-mer via `perl_substr`** so the chromosome-start `pos<4` wrap emits — A-F1); `drach_bottom` (`GT` scan, `revcomp`'d 5-mer, report/look-up at `pos-1`); `run_drach` single + split drivers (per-chr buffer, top-then-bottom, covered-only; single opens writers up front → empty cov yields 2 empty files; split = fresh truncating per-chr writers); `drach_report_path`/`drach_cov_path` (raw-`-o` base).
- **`lib.rs`:** early-exit `if config.drach { return drach::run_drach(config, &genome); }` after genome load, before `report::run_report` (Perl `:38-42`).
- **Tests/goldens:** `tests/data/phase2_drach/` (5 fixtures, 9 per-mode Perl golden dirs) + `generate_goldens.sh` (full provenance; note the Perl `sleep(20)` per run — STDERR/timing, exempt). `tests/golden_phase2.rs` (12 dir-vs-Perl byte-identity tests, V1–V16). Updated `sanity.rs` (the v1.x-rejection probe: `--drach` → `--ffs`).
- An out-of-tree scratch sweep verified **all 12 modes** Rust≡Perl v0.25.1 byte-for-byte (top, bottom, gzip, zero-ignored, threshold, `--CX` early-exit, the top-strand `pos<4` wrap, bottom truncation, raw-suffix, split, single-file ordering, empty) before the goldens were committed.

**No deviations from rev 2** — every folded review item was implemented as specified (the top-strand `perl_substr` mandate + golden V15, the `.get`-based `is_drach_motif`, the general-mutex preservation, the empty-cov guard, the bottom-emit truncation V10, the single-file ordering V16). The bottom-strand `pos-1` anchor is a plain byte-identical port (Q1 resolved).

## Revision history
- **rev 0** (2026-05-31): initial Phase 2 plan from EPIC + v1.0 SPEC + Phase-D template + full read of the Perl DRACH path + empirical probing of live Perl. Drafted with the bottom-strand C position as a headline Critical question (the `// TODO`), top-strand-byte-identical vs bottom-strand-corrected split, pending Felix's decision.
- **rev 1** (2026-05-31): **Q1 resolved by Felix** — BS-seq is cytosine-specific, so the C (`pos-1`) is the intended bottom-strand anchor (not the m6A `A`); Perl's `pos-1` is correct. Collapsed the divergence: **both strands now byte-identical to Perl v0.25.1**; dropped the corrected-hand-reference validation (V6/V7 → byte-identity goldens); the chromosome-start `pos<4` negative-substr wrap (V7) is now framed as faithful reproduction (SPEC P3), not a fix. Status → ready for manual review.
- **rev 2** (2026-05-31): **dual plan-review folded** (`PLAN_REVIEW_A.md` APPROVE-WITH-CHANGES — 1 Critical, 4 Important; `PLAN_REVIEW_B.md` APPROVE-WITH-CHANGES — 0 Critical, 3 Important; both independently re-ran live Perl v0.25.1 on 14+/20 from-scratch fixtures and reproduced every behavioral claim byte-for-byte). Folded: **A-F1 (Critical)** — the TOP strand also negative-`substr`-wraps at `pos<4` and **emits** (`ACAAA`→`chrA 2 + 9 1 AA CAA`); both `tri_nt_top` and `drach_top` must use `perl_substr`, not naive slicing (§3.2 steps 2-3, §4 `drach_top`, new golden V15). **A-F8** — `is_drach_motif` must be `.get`-based / short-slice-safe (§3.2 step 4, §4, V3). **A-F2** — the general mutexes (CX×merge, threshold×merge) still fire under `--drach`; don't bypass them (§3.0.2, V1). **A-F3/B-2** — empty-cov final-flush guard: writers up front, `Option`-skeleton skips the phantom `""`-chr flush, no panic (§3.5.5, V8). **A-F5** — V7 reworded to "no panic; emits nothing" (bottom `pos<4` is always len-guard-skipped). **A-F6** — top-strand `pos<4` emit golden (V15). **A-F7** — V10 now pins the bottom truncated-5-mer **pass-AND-emit**. **B-1** — documented the `perl_substr` offset bound (DRACH never goes below `-len`, so the helper's `offset<-len` divergence is unreachable; §3.3 step 6). **B-3** — added V16 (single-file 2-chr ordering) + the non-ACGT pos-1/pos-2 unit (V3). **A-F4/B-Opt1** — `AC`/`GT` use an `i += 1` scan (non-self-overlapping; not `gpc.rs`'s `j += 2`; §3.2 step 1). **B-Opt2** — softened the filter-before-`len<3` "byte-identity fact" wording (the order is not byte-observable; §3.2 step 5). Pointed the implementer at `gpc.rs` as the structural twin + the per-phase `generate_goldens.sh` convention (§9). **No logic defect was found in the plan's stated arithmetic** — all changes are completeness/test-coverage hardening. Status → **ready for the implement trigger.**
