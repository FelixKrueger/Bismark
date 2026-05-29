# `bismark-coverage2cytosine` — SPEC

**Status:** rev 2 (manual-review questions resolved; Phase-A dual plan-review synced). Grounded against Perl `coverage2cytosine` v0.25.1 (2,321 LOC) + the established `bismark-io` / `bismark-extractor` / `bismark-dedup` Rust patterns. Phases A–E confirmed; Phase A PLAN at rev 1 (dual-reviewed, APPROVE-WITH-CHANGES folded). NOT yet approved for implementation (awaiting Felix's trigger).

**Owners:** epic [#891](https://github.com/FelixKrueger/Bismark/issues/891). Part of Phase H sub-gate 2 of the Bismark Rust port (the cytosine-report producer downstream of `bismark2bedGraph`).

**Target:** Perl `coverage2cytosine` (v0.25.1) at the Bismark repo root. **Byte-identical to Perl v0.25.1** for every output stream in the v1.0 scope (see §2).

**Branch / worktree:** `rust/coverage2cytosine` in an isolated git worktree at `../Bismark-c2c` (off `origin/rust/iron-chancellor` @ 8a2a147). New crate `bismark-coverage2cytosine` in the existing `rust/` workspace. **Does NOT touch `rust/bismark-extractor`** (parallel session) or `rust/bismark-bedgraph` (#797, parallel session).

---

## 1. Goal

Port the genome-wide cytosine-report producer to a Rust binary `coverage2cytosine_rs` + a reusable library crate in the `rust/` workspace. It reads a Bismark coverage file (`*.bismark.cov[.gz]`) plus the genome FASTA and emits a per-cytosine report (CpG by default; all-context with `--CX`), the always-on cytosine-context summary, and — with `--merge_CpGs` — a pooled-strand coverage file.

**Match Perl v0.25.1 byte-for-byte** on all in-scope output streams. The crate is built so the `bismark-extractor` can later call it **inline** (replacing today's Perl subprocess in `bismark-extractor/src/subprocess.rs`), closing Phase H sub-gate 2 of the extractor's byte-identity gate.

This is the second of the two crates that unblock the extractor's sub-gate 2: `bismark-bedgraph` (#797) produces the `.bismark.cov.gz`; this crate consumes it.

## 2. Scope

**In scope (v1.0)** — confirmed with Felix 2026-05-29 ("Core + merge_CpGs"):

- Genome-wide cytosine report: **CpG-only (default)** and **`--CX` all-context**.
- The always-written **`*.cytosine_context_summary.txt`** (since Perl 2020-04-28; byte-identity includes it).
- `--zero_based` (0-based coordinate variant).
- `--split_by_chromosome` (one report file per chromosome).
- `--coverage_threshold N` (minimum coverage to report a position).
- `--gzip` (gzip-compressed report + cov outputs).
- `--genome_folder` (mandatory; the Perl hardcoded-mouse-default is **rejected** in the Rust port — see §15).
- `-o/--output`, `--dir`, `--parent_dir` (output naming + placement, matching Perl exactly).
- **`--merge_CpGs`** post-pass → `*.merged_CpG_evidence.cov[.gz]`, and its companion **`--discordance N`** filter → `*.discordant_CpG_evidence.cov[.gz]`.
- A **library API** consumable by `bismark-extractor` for the future inline switch (the wiring into the extractor is out of scope here — parallel session owns that crate).

**Deferred to v1.x** (separate post-1.0 epic phases, per Felix 2026-05-29 "Phase in later"):

- `--gc` / `--gc_context` (GpC-context report; the `generate_GC_context_report` path).
- `--nome-seq` (NOMe-Seq ACG/TCG CpG filtering + GpC filtering; sets `--gc`).
- `--drach` / `--m6A` (DRACH-motif m6A filtering; `generate_DRACH_report`, ~300 LOC, carries a Perl `// TODO`).
- `--ffs` (tetra/penta/hexamer nucleotide context columns).

These flags are **rejected at the CLI** in v1.0 with a clear "not yet supported in the Rust port; use Perl `coverage2cytosine`" message (NOT silently ignored — silent acceptance would produce wrong output).

**Out of scope (byte-identity not asserted), all versions:**

- **STDERR byte-identity.** The `warn`/`on_screen_summary` progress chatter is informational; matching it byte-for-byte is not a goal (consistent with the dedup + extractor precedent). STDERR content is free to diverge; only file outputs are gated.
- M-bias/PNG (not a c2c concern).

## 3. CLI flag inventory

All flags from Perl `process_commandline` (`coverage2cytosine:2011-2029`). Citations are Perl line numbers.

| # | Flag | Alias | Default | v1.0 | Behavior | Perl ln |
|---|------|-------|---------|------|----------|---------|
| 1 | `-o`/`--output` | — | (required) | ✅ | Output basename (mandatory; Perl dies without it). | 2018, 2077 |
| 2 | `--dir` | — | CWD (`''`) | ✅ | Output directory; created if missing; made absolute. | 2013, 2084 |
| 3 | `-g`/`--genome_folder` | — | (required) | ✅ | Path to FASTA genome dir (mandatory). Rust rejects without explicit value (no mouse default). | 2014, 2119 |
| 4 | `--zero_based` | — | OFF | ✅ | 0-based coordinates throughout (`pos -= 1`). | 2015 |
| 5 | `-CX`/`--CX_context` | `CX` | OFF | ✅ | Report every cytosine context, not just CpG. | 2016 |
| 6 | `--split_by_chromosome` | — | OFF | ✅ | One output file per chromosome (`.chr<NAME>` infix). | 2017 |
| 7 | `--parent_dir` | — | CWD | ✅ | Base dir to resolve relative paths against. | 2019 |
| 8 | `--version` | — | OFF | ✅ | Print version + exit (Rust emits TG-style provenance). | 2020 |
| 9 | `--merge_CpGs` | — | OFF | ✅ | Post-pass: pool top/bottom CpG → single dinucleotide cov. Mutex with `--CX`, `--split_by_chromosome`, `--coverage_threshold`. | 2021 |
| 10 | `--discordance_filter N` | — | OFF | ✅ | (merge_CpGs only) Route discordant CpGs (Δ% > N) to a separate file. Requires `--merge_CpGs`; 1≤N≤100. | 2026 |
| 11 | `--coverage_threshold N` | `--threshold` | 0 | ✅ | Min coverage to report a position. >0 ⇒ uncovered chromosomes/positions skipped. Mutex with `--merge_CpGs`. | 2027 |
| 12 | `--gzip` | — | OFF | ✅ | gzip report + cov outputs (NOT the context summary). | 2024 |
| 13 | `--help`/`--man` | `man` | — | ✅ | Print help + exit. | 2012 |
| 14 | `--GC`/`--GC_context` | `GC` | OFF | ⛔ v1.x | GpC-context report. Rejected in v1.0. | 2022 |
| 15 | `--nome-seq` | — | OFF | ⛔ v1.x | NOMe-Seq filtering (sets `--gc`). Rejected in v1.0. | 2025 |
| 16 | `--drach`/`--m6A` | `m6A` | OFF | ⛔ v1.x | DRACH m6A filtering. Rejected in v1.0. | 2028 |
| 17 | `--ffs` | — | OFF | ⛔ v1.x | tetra/penta/hexamer context columns. Rejected in v1.0. | 2023 |

**Validation rules** (mirror Perl `process_commandline:2138-2194`):

- `--merge_CpGs` + `--CX` → die (Perl 2140): "Merging … only supported … CpG-context only (lose the option --CX)".
- `--merge_CpGs` + `--split_by_chromosome` → die (Perl 2143).
- `--merge_CpGs` + `--coverage_threshold` → die (Perl 2176).
- `--discordance_filter` without `--merge_CpGs` → die (Perl 2165); value must be `0 < N ≤ 100` (Perl 2168).
- `--coverage_threshold` value must be `> 0` when set (Perl 2178); unset ⇒ default `0`.
- Missing `-o` → die (Perl 2078); missing `--genome_folder` → die (Perl 2134); no positional cov infile → help + exit (Perl 2059).
- **`--CX` default coupling**: when `--CX` is absent, Perl sets `$CpG_only = 1` (Perl 2112-2115). Reproduce.

The Rust `ResolvedConfig::validate()` enforces these (mirrors `bismark-dedup::cli::Cli::validate`). v1.x flags (`--gc`/`--nome-seq`/`--drach`/`--ffs`) are **rejected** here with a "not supported in Rust port" error rather than accepted.

## 4. Input format

The positional `*.bismark.cov[.gz]` coverage file (Perl-generated by `bismark2bedGraph`), tab-separated, **1-based** coordinates throughout:

```
<chr>  <start>  <end>  <methylation%>  <count_methylated>  <count_unmethylated>
```

Perl parses `($chr,$start,$end,undef,$meth,$nonmeth) = split /\t/` (`:209`) — **column 4 (the percentage) is discarded**; `$end` is read but unused in the report path. Lines are buffered per-chromosome into a `pos → (meth, nonmeth)` map. `start` is the lookup key.

- `.gz` detection is by literal filename suffix `gz$` (`:186`); `.gz` ⇒ decompress (`flate2`), else read plain.
- The coverage file is assumed **sorted by chromosome then position** (it is, by `bismark2bedGraph` construction). The report walks the *genome*, not the cov file, so within-chromosome cov order does not affect report-line order — but **chromosome appearance order in the cov file DOES** drive covered-chromosome output order (§7.5).
- **Parse policy (rev 3, Phase-B review):** lines are processed per-chromosome, flushed on each `chr`-transition (driven solely by the transition — a **non-contiguous** chr re-flushes + re-emits, matching Perl `:227`); duplicate positions are last-write-wins (Perl `%chr` overwrite `:224`); a trailing `\r` is stripped (CRLF) and blank lines skipped; `start`/`meth`/`nonmeth` are strict `u32` (a non-numeric field → `MalformedCovLine` error — an **accepted divergence** from Perl's lenient `"abc"`→0 coercion, which cannot occur on real `bismark2bedGraph` output). See `phase-b-core-report/PLAN.md` §3.1.

## 5. Output topology

`{stem}` = the `-o` value with any trailing `.CpG_report.txt` / `.CX_report.txt` stripped (Perl `handle_filehandles:107-112`).

| File | When | gzip? | Format |
|------|------|-------|--------|
| `{stem}.CpG_report.txt[.gz]` | default (CpG-only) | `--gzip` | 7-col cytosine report (§6) |
| `{stem}.CX_report.txt[.gz]` | `--CX` | `--gzip` | 7-col cytosine report (all contexts) |
| `{raw-o}.chr<NAME>.CpG_report.txt[.gz]` etc. | `--split_by_chromosome` | `--gzip` | per-chromosome report. **rev 3 (Phase-C verified):** the `.chr<NAME>` infix is appended to the **raw `-o`** *before* the suffix strip (Perl `:99-101`), which then no-ops — so a **suffixed** `-o` (the extractor path) **doubles** the suffix: `foo.CpG_report.txt` → `foo.CpG_report.txt.chrchr1.CpG_report.txt`. The per-chr context-summary files are all empty except the **last-processed** chromosome's (full). See `phase-c-gzip-split/PLAN.md`. |
| `{stem}.cytosine_context_summary.txt` | **always** | **never** | 6-col context summary (§8) |
| `{stem}.merged_CpG_evidence.cov[.gz]` | `--merge_CpGs` | `--gzip` | 6-col pooled-strand cov (§9) |
| `{stem}.discordant_CpG_evidence.cov[.gz]` | `--merge_CpGs --discordance_filter` | `--gzip` | 6-col discordant cov (§9) |

**Cytosine report line** (`:408` etc.), tab-separated + trailing `\n`:

```
<chr>  <position>  <strand>  <count_methylated>  <count_unmethylated>  <context>  <trinucleotide>
```

- `position`: 1-based by default; `position - 1` with `--zero_based`.
- `strand`: `+` (genome C) / `-` (genome G / bottom-strand C).
- `context`: `CG` | `CHG` | `CHH`.
- `trinucleotide`: the 5'→3' trinucleotide (revcomp for `-` strand).

## 6. Genome reading

Mirrors Perl `read_genome_into_memory:1648-1739` + `extract_chromosome_name:1741-1751`. Reuses the **`bismark-io::cram_ref`** noodles-fasta pattern as the structural reference, but with c2c-specific quirks (a new module — see §10 for the bismark-io-vs-crate-local decision):

1. **Glob priority** (`:1654-1669`): `*.fa` → fallback `*.fa.gz` → `*.fasta` → `*.fasta.gz`. **First non-empty glob wins** (do NOT union them). _Note: Perl globs only these four; `.fna`/`.ffn` that `cram_ref.rs` accepts are NOT in c2c's set — match Perl._ **rev 2 (code-review B-1):** exclude leading-dot files (Perl's `<*.fa>` glob never matches dotfiles) — a partial-download `.GRCh38.fa.gz` must not be ingested as a chromosome Perl would never see.
2. **Skip `Mus_musculus.NCBIM37.fa`** (`:1678`) — a Perl-ism (the tophat whole-mouse file). Reproduce the skip.
3. **Chromosome name** = first whitespace-delimited token after `>` (`:1745`). **Resolved (rev 1, Phase-A dual review read noodles-fasta 0.61.0 source):** noodles `record.name()` returns up-to-first-ASCII-whitespace — matching Perl's `split /\s+/` token 0 for normal `>chrN[ desc]` headers — so use it directly (no manual split). noodles also auto-strips trailing `\r` (CRLF) from header + sequence. **Accepted divergence (rev 2, code-review A-M1):** for a *nameless* header (bare `>` or whitespace immediately after `>`) Perl stores an empty-name chromosome with no error, whereas noodles raises `MissingName` (InvalidData) → the Rust port errors with `MalformedFastaHeader`. This cannot occur on a Bowtie2-built Bismark genome (clean `>chrN` headers); pinned by a test rather than worked around.
4. **Uppercase the sequence** (Perl `uc`, `:1720`). ⚠️ **Critical, and a divergence from `cram_ref.rs` which does NOT uppercase.** Soft-masked (lowercase) genome bases must be uppercased or the `/[CG]/` walk + context regexes silently miss them. Strip `\r` (`:1690, 1698`).
5. **Multi-FASTA** supported; **duplicate chromosome name → error** (`:1702-1705`, matches `cram_ref.rs` `DuplicateChromosomeName`).
6. Store as a plain `HashMap<Vec<u8>, Vec<u8>>` `name → uppercased sequence bytes` (rev 1: order-irrelevant — see §10.4/§11). The Perl `%processed` covered/uncovered tracking is **Phase-B** logic (covered = cov-file appearance; uncovered = `Genome::names_sorted()`), not a field on the genome map.

**Names are byte-strings (`Vec<u8>`)** not `String`, per the `cram_ref.rs` rationale (FASTA/SAM names may be non-UTF-8). The whole genome is held in memory (Perl does the same); for hg38 ≈ 3 GB — acceptable and matches Perl's footprint. See §11 for the chromosome-ordering structure.

## 7. Core algorithm — the genome-wide report (THE byte-identity crux)

Ported from `generate_genome_wide_cytosine_report:168-745` + `process_unprocessed_chromosomes:1388-1565`.

### 7.1 Coordinate arithmetic (the crux of the crux)

Perl walks `while ($seq =~ /([CG])/g)` and reads `$pos = pos($seq)`. Perl's `pos()` returns the offset **just past** the matched char, so a base at **0-based index `i`** yields **`pos = i + 1`**, which Perl then treats as the **1-based coordinate**. The Rust port walks the sequence bytes; for each byte at index `i` that is `b'C'` or `b'G'`, set `pos = i + 1` (1-based) and reproduce these `substr` translations (`substr(seq, off, len)` = `seq[off .. off+len]`, 0-based `off`):

**Genome `C` (forward, strand `+`):**
- `tri_nt = substr(seq, pos-1, 3)` = `seq[i .. i+3]` (the C + next 2 bases).
- `upstream = substr(seq, pos-2, 3)` = `seq[i-1 .. i+2]`. ⚠️ When `i == 0`, Perl `substr(seq, -1, 3)` wraps to **count from the string end** (returns the trailing 1 char). Must replicate this Perl negative-offset wrap (only feeds the context summary's `ubase`; see §8 + pitfall P3).

**Genome `G` (reverse / bottom-strand C, strand `-`):**
- If `pos-3 < 0` (i.e. `i < 2`): `tri_nt = substr(seq, 0, pos)` = `seq[0 .. i+1]` (1 or 2 bytes ⇒ filtered by the length<3 guard). (`:294-296`)
- Else: `tri_nt = substr(seq, pos-3, 3)` = `seq[i-2 .. i+1]`, then **reverse + complement** via `tr/ACTG/TGAC/` (`:301-302`).
- `upstream = substr(seq, pos-2, 3)` = `seq[i-1 .. i+2]`, reverse + complement (`:335-337`).

**Reported position:** `pos` (1-based) or `pos - 1` (`--zero_based`). `pos - 1 == i` — i.e. zero-based coord equals the 0-based array index, as expected.

**Complement map** `tr/ACTG/TGAC/`: `A↔T`, `C↔G`. ⚠️ It does **NOT** translate `N` or any other byte — they pass through unchanged. Reproduce exactly (a 4-byte lookup that leaves all other bytes identity).

### 7.2 Per-position guards (order matters for STDERR, not for output)

Applied per matched C/G (`:343-377`, and the structurally-identical last-chr block `:597-621` + uncovered-chr block `:1495-1517`):

1. `tri_nt.len() < 3` ⇒ skip (trinucleotide could not be extracted — chromosome edge).
2. `(seq.len() - pos) == 0` ⇒ skip (the very last genome base; its bottom-strand partner would need the following base). (`:347-349`)
3. Coverage lookup: `meth, nonmeth = chr_map.get(pos).unwrap_or((0, 0))` (stored key is 1-based `start`).
4. `meth + nonmeth < threshold` ⇒ skip. Default `threshold == 0` ⇒ never skips (uncovered positions emit `0 0`).
5. Context classification (regex, §7.3). Unclassifiable ⇒ STDERR warn + skip.
6. `context_reporting(tri_nt, upstream, meth, nonmeth)` — accumulate context summary (§8). **Runs for all contexts, before the CpG-only output filter.**
7. Emit (CpG-only: only if context==CG; `--CX`: all contexts).

_The covered-chr block and the last-chr block apply guards 1-2 vs 3-4 in a different textual order, but the net set of emitted positions is identical (threshold default 0 makes the ordering moot; with a user threshold the `>=` test is on the looked-up counts either way). The Rust port uses one shared per-position routine to avoid the Perl duplication (and the divergence risk it carries — cf. the dedup "dual-driver back-port" memory)._

### 7.3 Context classification

On the 5'→3' `tri_nt` (`:365-377`):

- `^CG` ⇒ `CG`
- `^C.G$` (C, any byte, G; len 3) ⇒ `CHG`
- `^C..$` (C + any 2; len 3) ⇒ `CHH`
- else ⇒ STDERR warn "context could not be determined" + skip.

`.` matches any byte incl. `N` (e.g. `CNG`→CHG, `CNN`→CHH, `CGN`→CG). The first byte is always `C` for a real forward-C or a revcomp'd G, so the `else` is a rare defensive path (non-ACGTN bytes). Reproduce the regex semantics exactly (byte-level, not Unicode).

### 7.4 Output filtering (CpG-only vs --CX)

- **Default (`CpG_only`)**: emit a line only when `context == CG` (`:386`).
- **`--CX`**: emit every classified position (CG/CHG/CHH) (`:429`).

### 7.5 Chromosome processing order (byte-identity sensitive)

1. **Covered chromosomes first, in coverage-file appearance order.** Perl buffers cov lines until the `chr` field changes, then processes the just-finished chromosome (`:206-468`); the last chromosome is flushed after EOF (`:476-690`). So covered-chromosome report order = order of first appearance in the cov file. ⚠️ Use an **insertion-ordered structure** — NOT a `BTreeMap` (which would canonical-sort and break byte-identity). This is the exact pitfall #797 calls out for `bismark2bedGraph`.
2. **Uncovered chromosomes next, in `sort keys %processed` order** (Perl `:722` — alphabetical/bytewise sort of chromosome names). Only when `threshold == 0` (`:714-717`: a positive threshold skips uncovered chromosomes entirely; the v1.x `--nome` also skips them). Each emitted via the all-zero-coverage walk.

### 7.6 Empty-input guard

If no `$last_chr` was ever defined (empty cov file / wrong path), Perl dies (`:472-474`): "No last chromosome was defined …". Reproduce as a typed error. (With `threshold==0`, an empty cov file still emits the full all-zero genome report for every chromosome in `sort` order via the uncovered-chromosome pass — verify Perl's exact behavior here: `$last_chr` undefined ⇒ die BEFORE the uncovered pass. So empty cov ⇒ die, not a zero-genome report. Pin in a test.)

## 8. Cytosine context summary

`reset_context_summary:1961-1975` + `context_reporting:1977-1988` + `print_context_summary:63-78`.

- **Init**: all 16 `C{A,C,G,T}{A,C,G,T}` trinucleotides × 4 upstream bases `{A,C,G,T}` = **64 rows**, counts zeroed.
- **Accumulate** (`context_reporting`): `ubase = upstream[0]`. Only if `tri_nt` and `ubase` are **pure ACTG** (`unless (tri_nt =~ /[^ACTG]/ or ubase =~ /[^ACTG]/)`): add `meth`→`m`, `nonmeth`→`u` at `summary[tri_nt][ubase]`. (N-containing contexts contribute nothing.)
- **Output** (`print_context_summary`), tab-separated + `\n`:
  - Header: `upstream\tC-context\tfull context\tcount methylated\tcount unmethylated\tpercent methylation`.
  - Rows sorted by `(tri_nt, ubase)` (Perl `sort keys` bytewise): `ubase  tri_nt  {ubase}{tri_nt}  m  u  perc`.
  - `perc = sprintf("%.2f", m/(m+u)*100)` when `m+u>0`, else literal `N/A` (`:69-74`).
- Written for **both** CpG-only and `--CX` runs (summary always reflects all contexts). Always uncompressed.

## 9. `--merge_CpGs` post-pass

`combine_CpGs_to_single_CG_entity:1753-1958`. Runs AFTER the report is written; **re-reads the just-written CpG report** (`$global_cyt_report`, possibly gzipped) and pools strand-pairs.

- Read lines in pairs: line1 (expected `+`), line2 (expected `-`).
- **Chromosome-start special case** (`:1809-1883`): a CpG at genome position 1 has a `+` entry but its `-` partner (the G at pos 2, `i==1`) was dropped by the len<3 guard (§7.1). Perl detects `pos1 < 2` (1-based) / `pos1 < 1` (`--zero_based`) and reads ahead to resync pairs, including across short scaffolds. Port this resync faithfully (it is the historical source of bugs #98/#229 — see §13).
- **Sanity asserts** (`:1886-1897`, Perl `die`): `context1==CG`, `context2==CG`, `strand1=='+'`, `strand2=='-'`, `pos2==pos1+1`, `chr1==chr2`. Reproduce as typed errors.
- **`--discordance_filter N`** (`:1902-1932`): only when **both** strands measured (`m1+u1>0` AND `m2+u2>0`); if `abs(top% − bottom%) > N` (each `sprintf "%.6f"`), write both rows to `*.discordant_CpG_evidence.cov` and **skip** merging this pair. Coordinates: 1-based `chr,pos,pos` or `--zero_based` half-open `chr,pos,pos+1`.
- **Pool** (`:1934-1952`): `pooled_m=m1+m2`, `pooled_u=u1+u2`; **skip if `pooled_m+pooled_u == 0`**; `pooled_pct = sprintf("%.6f", pooled_m/(pooled_m+pooled_u)*100)`. Write `chr1  pos1  pos2  pooled_pct  pooled_m  pooled_u` (1-based) or `chr1  pos1  pos2+1  …` (`--zero_based`, half-open).

## 10. Structural design choices (Rust)

Mirroring the locked decisions from `bismark-io`/`bismark-extractor`/`bismark-dedup`:

### 10.1 Crate shape — lib + bin
`lib.rs` exposes the library API (so the extractor can later call it inline, per §1); `main.rs` is the thin binary `coverage2cytosine_rs`. Mirrors `bismark-dedup`'s `lib.rs`/`main.rs` split. `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]`.

### 10.2 CLI = clap-derive `Cli` → `ResolvedConfig::validate()`
Exactly the `bismark-dedup::cli` pattern: a `#[derive(Parser)] Cli`, a `validate()` that resolves + rejects illegal flag combos (§3) into a `ResolvedConfig`, `disable_version_flag = true` + custom `version_string()`.

### 10.3 Genome reader — crate-local module on noodles-fasta
A new `genome.rs` in this crate, built on `noodles-fasta` (like `cram_ref.rs`) but with the §6 quirks (uppercase, Mus skip, four-suffix glob priority, insertion-tracked map, `processed` flags). **Open question (§15):** promote to `bismark-io` as a shared `genome::load_bismark_genome()` or keep crate-local. Default: crate-local for v1.0 (it is c2c-shaped: in-memory whole-genome map with per-chr `processed` state); promote later if `bismark-bedgraph` needs the same.

### 10.4 Chromosome ordering — covered vs uncovered (rev 1, clarified post Phase-A review)
Covered-chromosome output order = cov-file appearance order (§7.5). The **insertion-ordered structure applies to the Phase-B "covered-chromosome appearance list"** (e.g. a `Vec<Vec<u8>>` built as cov-file chromosomes are first seen, or `indexmap::IndexMap`) — **NOT** to the genome sequence map, which is a plain `HashMap` whose order never reaches output (see §11). **Never `BTreeMap`** for the covered list. The uncovered set is emitted in a separately bytewise-sorted pass (`Genome::names_sorted()`). (Same byte-identity trap as #797, but scoped to the covered list.)

### 10.5 Output writers — `BufWriter`, optionally gzip
`BufWriter<File>` for plain; **`GzEncoder<BufWriter<File>>`** for `--gzip` (rev 3 Phase-C correction — the encoder wraps a buffered file writer; `flate2` is a regular dep). The context summary is **never** gzipped. For `--split_by_chromosome`, a fresh **truncating** writer is opened per chromosome on every transition (Perl `handle_filehandles` reopen at `:457-466` — a re-appearing chr's file is truncated, keeping only the last segment); filename `.chr<NAME>` infix per §5 (raw-`-o`, suffix-doubling). gzip byte-identity is asserted **after decompression** (the gzip container is impl-dependent).

### 10.6 Typed errors via `thiserror`
`BismarkC2cError` enum (mirrors `bismark-dedup::error`): `MissingOutput`, `MissingGenomeFolder`, `NoGenomeFasta`, `DuplicateChromosomeName`, `EmptyCoverageInput`, `MergeCpGSanityViolation { … }`, `UnsupportedFlag { flag }` (the v1.x rejections), `Io(#[from])`. Partial outputs cleaned up on error (the dedup `cleanup_partial_output_on_err` pattern).

### 10.7 Performance posture
Byte-identity is the v1.0 gate; perf is **advisory** (matches the extractor's stance). The Perl is single-threaded and holds the whole genome in RAM; the Rust port matches that model for v1.0. Profiling (CLAUDE.md) bundles c2c into the "bedGraph + cyt_report 57 min, 5-8× est." line; a perf pass (parallelize the per-chromosome genome walk — embarrassingly parallel, with an ordered collector for byte-identity, à la the extractor's §9) is a candidate v1.x phase, not a v1.0 requirement.

## 11. Data structures (sketch)

```rust
/// Whole-genome sequence map. NOTE (rev 1, dual-review of Phase A): a plain
/// HashMap is sufficient — the genome map's iteration order NEVER reaches
/// output (covered chromosomes emit in cov-file appearance order [Phase B];
/// uncovered in bytewise-sorted order). The insertion-order/IndexMap
/// requirement (P1) applies to the Phase-B *covered-chromosome appearance
/// list*, NOT to this map. `Genome` exposes no public insertion-order
/// iterator (only `names_sorted()`), which keeps that guarantee airtight.
struct Genome {
    /// name → uppercased sequence bytes; private; no order-dependent accessor.
    chromosomes: HashMap<Vec<u8>, Vec<u8>>,
}

/// Per-chromosome coverage buffer: 1-based pos → (meth, nonmeth).
type CovMap = HashMap<u32, (u32, u32)>;   // or FxHashMap (rustc-hash, dedup precedent)

#[derive(Clone, Copy)]
enum Context { Cg, Chg, Chh }

/// 64-cell context summary: [trinucleotide C** ][upstream ACGT] → (m, u).
struct ContextSummary { /* fixed 16×4 grid */ }

struct ResolvedConfig {
    cov_infile: PathBuf,
    output_stem: String,
    output_dir: PathBuf,
    parent_dir: PathBuf,
    genome_folder: PathBuf,
    cpg_only: bool,        // !cx_context
    cx_context: bool,
    zero_based: bool,
    split_by_chromosome: bool,
    threshold: u32,        // 0 = report all
    gzip: bool,
    merge_cpgs: bool,
    discordance: Option<u8>,
}
```

## 12. Test surface

### 12.1 Unit tests (in-crate, synthetic)
- **Coordinate arithmetic**: forward-C and reverse-G `tri_nt`/`upstream` extraction at interior, chr-start (`i=0,1`), chr-end (`i=len-1`) positions — assert exact bytes incl. the `i=0` upstream negative-wrap (P3) and the last-base exclusion (guard 2).
- **Complement** `tr/ACTG/TGAC/`: ACGT mapped, `N` + other bytes pass through.
- **Context classification**: CG/CHG/CHH + N-containing (`CNG`→CHG etc.) + unclassifiable `else`.
- **Context summary**: pure-ACTG gating (N contexts contribute 0), `%.2f` vs `N/A`, 64-row sorted order, header bytes.
- **CpG-only vs --CX** emission filter.
- **Coverage lookup**: covered ⇒ counts; uncovered ⇒ `0 0`; threshold skip.
- **Genome reader**: glob priority (`.fa` wins over `.fa.gz`), Mus skip, uppercase, multi-FASTA, dup-name error, first-token name.
- **Chromosome ordering**: covered = cov-appearance order; uncovered = sorted; mixed.
- **merge_CpGs**: pooling math, `%.6f`, skip-zero-coverage, chr-start resync, sanity-violation errors; discordance routing + both-measured gate; zero_based half-open coords.
- **Filename derivation**: `-o foo` → `foo.CpG_report.txt`; `-o foo.CpG_report.txt` → dedup-strip → `foo.CpG_report.txt`; `--CX`, `--gzip`, `--split_by_chromosome` `.chr<NAME>` infixes; context-summary name.
- **Validation**: every §3 mutex/range rule; v1.x flag rejection.
- **Empty cov input** ⇒ `EmptyCoverageInput` error (§7.6).

### 12.2 Integration tests (small fixtures, `#[ignore]`-free)
A tiny synthetic genome (a few hundred bp, multi-FASTA, with CpG-at-start, CpG-at-end, N runs, a short scaffold) + a hand-built `.bismark.cov` and `.cov.gz`. Run the binary; diff against a checked-in Perl-v0.25.1 golden for: CpG report, CX report, context summary, `--zero_based`, `--split_by_chromosome`, `--gzip` (decompressed compare), `--merge_CpGs` (+`--discordance`). Goldens generated once from Perl v0.25.1 and committed (cf. epic #795 fixtures).

### 12.3 Real-data byte-identity gate (colossal) — the release gate
Per `reference_colossal_access`. On colossal (`bioinf` env, Perl v0.25.1), against a **Perl-`bismark2bedGraph`-generated** `.bismark.cov.gz` (NOT a Rust-bedgraph one — coordinate via #797 if/when that crate lands; until then Perl cov input keeps the two producers genuinely independent, satisfying the §13 sub-gate-2 "two independent producers" rule):

- Genome: `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/genome/` (verify exact subpath first session).
- Input cov: derived from the 10M PE dataset via Perl `bismark2bedGraph`.
- **Distinct out-dir from other sessions** (Felix directive) — e.g. `~/c2c_byte_identity_<ts>/`.
- Assert **raw-byte-identity** Rust≡Perl on: `.CpG_report.txt[.gz]`, `.CX_report.txt[.gz]`, `.cytosine_context_summary.txt`, `.merged_CpG_evidence.cov[.gz]`, `.discordant_CpG_evidence.cov[.gz]`. (Reports are genome-ordered + deterministic ⇒ raw bytes, not sorted-md5; gzip compared after decompression to avoid mtime/OS gzip-header noise — pin `flate2` output or decompress-then-compare.)
- `LC_ALL=C` for any sort-dependent step.
- Run the matrix: {default CpG} × {`--CX`} × {`--zero_based`} × {`--gzip`} × {`--merge_CpGs`(+`--discordance`)} × {`--split_by_chromosome`} (a representative subset, not full cross-product).

`★` This gate is real validation precisely because the two pipelines do NOT share a c2c producer — Perl-c2c vs Rust-c2c, on a common Perl-generated cov input. (Contrast the extractor's Phase G subprocess tautology, per `project_phase_h_byte_identity_ordering`.)

## 13. How this fits Phase H sub-gate 2

Per `project_phase_h_byte_identity_ordering`: the extractor's sub-gate 2 (`*.bismark.cov.gz`, `CpG_report.txt[.gz]`/`CX_report.txt[.gz]`) was blocked because Phase G routes BOTH pipelines through the SAME Perl `coverage2cytosine` (one producer ⇒ tautological compare). This crate is the **independent Rust producer** for the `*_report.txt` half. Once it + `bismark-bedgraph` (#797) land and the extractor switches to calling them inline, sub-gate 2 becomes a genuine two-producer byte-identity comparison. **The inline switch in `bismark-extractor` is out of scope here** (parallel session owns that crate); this crate just makes itself callable.

## 14. Phases (proposed — pending your confirmation before EPIC.md is written)

Mirrors the `bismark-dedup`/`bismark-extractor` phased cadence (A→…; each merges to `rust/coverage2cytosine`, then the whole branch merges to `rust/iron-chancellor`).

| Phase | Scope | Depends |
|-------|-------|---------|
| **A** | Workspace scaffold + crate (`lib`+`bin`) + clap `Cli`/`ResolvedConfig::validate` (all §3 rules incl. v1.x rejections) + `genome.rs` reader (§6) + error enum. Crate boots; `--help`/`--version`; genome loads. | — |
| **B** | **Core report** (the §7 heart): cov parsing, per-chr buffering, genome C/G walk + exact coordinate arithmetic, context classification, CpG-only vs `--CX`, `--zero_based`, `--coverage_threshold`, covered+uncovered chromosome ordering, **`.cytosine_context_summary.txt`** (§8). Plain output. | A |
| **C** | `--gzip` (report+cov) + `--split_by_chromosome` (per-chr writers + `.chr<NAME>` filename derivation). | B |
| **D** | **`--merge_CpGs`** (+`--discordance_filter`): the §9 post-pass incl. chr-start resync + sanity asserts + discordant routing. | B (C if gzip merge) |
| **E** | **Real-data byte-identity gate** on colossal (§12.3): driver script + matrix + RELEASE checklist. Gates the `bismark-coverage2cytosine-v1.0` tag. | B, C, D |

**Deferred (v1.x, separate epic / phases):** `--gc`/`--nome-seq` (one phase), `--drach`/`--m6A` (one phase), `--ffs` (folds into the report phases). A v1.x perf phase (parallel genome walk) is also a candidate (§10.7).

## 15. Open questions

| Priority | Question | Default |
|----------|----------|---------|
| Resolved | v1.0 flag scope | **Core + `--merge_CpGs`**; niche modes deferred (Felix, 2026-05-29). |
| Resolved | Niche modes (`--gc`/`--nome`/`--drach`/`--ffs`) | **Phased to v1.x**; rejected at CLI in v1.0 (Felix, 2026-05-29). |
| Resolved | `--genome_folder` Perl hardcoded-mouse default | **Reject** without explicit value (Felix, 2026-05-29 — matches extractor SPEC §11; the mouse default mis-targets silently). |
| Resolved | Promote genome reader to `bismark-io`? | **Crate-local** for v1.0 (Felix, 2026-05-29 "ok for now"); promote later if `bismark-bedgraph` needs it. |
| Resolved | Coverage-pos integer width | **`u32`** pos + `u32` counts (Felix, 2026-05-29). hg38 max chr ≈ 2.5e8 < `u32::MAX`. A Phase-A `debug_assert`/checked guard rejects any chromosome > `u32::MAX` (T2T/polyploid edge) with a clear error rather than silently wrapping; revisit `u64` only if a real fixture trips it. |
| Resolved | gzip byte-identity vs Perl | **Compare after decompression** (Felix, 2026-05-29 "OK"). Perl pipes through system `gzip`; the gzip container is NOT asserted byte-identical (version/flag-dependent), but the decompressed report/cov bytes ARE. |
| Resolved | STDERR scope | **Not** byte-identity-gated (dedup/extractor precedent; no objection at manual review 2026-05-29). |
| Open | `bismark2bedGraph` coordination | #797 is on branch `rust/bismark-bedgraph` (parallel session). v1.0 tests use **Perl-generated** cov input regardless; coordinate the inline hand-off later. (External dependency, not a local decision.) |

## 16. Structural pitfalls catalog

| # | Pitfall | Perl source | Prevention |
|---|---------|-------------|------------|
| P1 | `BTreeMap` canonical-sorts covered chromosomes ⇒ byte-NOT-identical | implicit (Perl hash-of-buffers + cov order) | Insertion-ordered map for covered set; sorted pass for uncovered set (§7.5, §10.4). |
| P2 | Not uppercasing the genome ⇒ soft-masked CpGs silently dropped | `:1720` `uc` | Uppercase on load (§6.4); explicit divergence from `cram_ref.rs`. |
| P3 | Perl `substr(seq,-1,3)` negative-offset wrap for `upstream` at `i=0` | `:288, 335` | Replicate from-end wrap (only affects context-summary `ubase`); unit-tested (§12.1). |
| P4 | `pos()` off-by-one (treating the match index as the coord) | `:256, 263` | `pos = i+1`; report `pos`/`pos-1`; substr uses `pos-1`/`pos-3` (§7.1). |
| P5 | Last-genome-base bottom-strand C double-count/edge | `:347-349` | Explicit `(len-pos)==0` skip guard (§7.2 guard 2). |
| P6 | merge_CpGs chr-start desync (missing `-` partner) ⇒ wrong pooling / die | `:1809-1883` (bugs #98/#229) | Faithful resync port + sanity asserts (§9); unit + integration tests. |
| P7 | `tr/ACTG/TGAC/` mistakenly complementing `N` | `:302` | 4-byte identity-elsewhere complement (§7.1). |
| P8 | Glob union instead of first-non-empty-wins; including `.fna`/`.ffn` | `:1654-1669` | Four-suffix ordered glob, first non-empty wins (§6.1). |
| P9 | Silently accepting v1.x flags ⇒ wrong/empty output | n/a (new) | Reject `--gc`/`--nome`/`--drach`/`--ffs` at CLI (§3). |
| P10 | Comparing gzip bytes (gzip-version-dependent) | `:140-142` (`| gzip -c`) | Decompress-then-compare in the byte-identity gate (§12.3, §15). |

## 17. References

- **Perl source**: `coverage2cytosine` (v0.25.1, 2,321 LOC) at the Bismark repo root.
- **Rust patterns**: `bismark-io/src/cram_ref.rs` (noodles-fasta genome reconstitution), `bismark-extractor/SPEC.md` (SPEC house style + byte-identity discipline), `bismark-extractor/src/subprocess.rs` (the current Perl-c2c subprocess contract this crate will eventually replace), `bismark-dedup/src/{lib,cli,main,error,filename}.rs` (lib+bin CLI scaffold).
- **Epics**: #797 `bismark2bedGraph` (upstream sibling, parallel session). This crate's epic + SPEC issue to be filed.
- **Memory**: `project_phase_h_byte_identity_ordering` (sub-gate 2 two-producer rule), `reference_colossal_access` (real-data test machine/paths/env), `project_rust_rewrite`.

## 18. Revision history

- **rev 0** (2026-05-29): initial draft. Grounded against Perl v0.25.1 + the bismark-io/extractor/dedup patterns. v1.0 scope (Core + `--merge_CpGs`) and niche-mode deferral confirmed with Felix. Phase breakdown (A–E) proposed, pending confirmation before `EPIC.md` is written. Awaiting manual review.
- **rev 1** (2026-05-29): manual-review pass 1. §15 open questions resolved by Felix — reject `--genome_folder` without value; genome reader crate-local for now; `u32` pos/counts (+ overflow guard); gzip compared post-decompression; STDERR not gated. Worktree isolation verified (3 distinct-branch worktrees; c2c fully isolated). Phase breakdown still pending explicit confirmation before `EPIC.md`.
- **rev 2** (2026-05-29): synced from Phase-A dual plan-review (both APPROVE-WITH-CHANGES). §6/§10.4/§11: genome map is a plain `HashMap` (order-irrelevant; insertion-order requirement scoped to the Phase-B covered-chromosome list, not the genome map); `Genome` exposes no public insertion-order iterator. noodles `record.name()` (up-to-whitespace) + auto-`\r`-strip confirmed against noodles-fasta 0.61.0 source. Phases A–E confirmed by Felix; EPIC.md written. Phase-A-specific folds (context-conditional output-stem strip; `output_dir=""` vs `parent_dir=getcwd()`; glob-tier semantics; `--CX` clap surface; `MalformedFastaHeader`) live in `phase-a-scaffold-cli-genome/PLAN.md` rev 1.
- **rev 3** (2026-05-29): **Phase A implemented, dual-code-reviewed (both APPROVE), plan-manager verdict COMPLETE.** §6.1 dotfile-exclusion + §6.3 nameless-header accepted-divergence added from code-review (B-1, A-M1). Crate `bismark-coverage2cytosine` (lib+bin) shipped: 43 tests pass, clippy clean. Phase B (core genome-wide report) is next.
