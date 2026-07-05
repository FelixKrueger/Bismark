# `bismark-bedgraph` — SPEC

**Status:** rev 1.1. Dual plan-review folded in (rev 1); **implemented + verified byte-identical to Perl v0.25.1** across the flag matrix (rev 1.1, 2026-05-29). rev 1.1 also corrects the §2.1B ordering *direction* (the OB-owned chr sorts FIRST, not last) after running the Perl as ground truth. See Revision History §11.

**Owners:** epic [#797](https://github.com/FelixKrueger/Bismark/issues/797), spec sub-issue [#802](https://github.com/FelixKrueger/Bismark/issues/802).

**Target:** Perl `bismark2bedGraph` (v0.25.1, 893 LOC) at the Bismark repo root. **Decompressed-content byte-identity to Perl v0.25.1** for both output streams (`.bedGraph.gz`, `.bismark.cov.gz`, plus the optional `.zero.cov` and `_UCSC.bedGraph.gz`).

**Scope boundary:** `bismark2bedGraph` ONLY. `coverage2cytosine` (the CpG/CX report producer downstream in the same chain) is explicitly OUT of scope — a separate future crate. This crate produces `.bedGraph.gz` + `.bismark.cov.gz`; nothing reads the genome FASTA.

---

## 1. Goal

Port `bismark2bedGraph` — the bedGraph + coverage producer — to a Rust binary `bismark2bedGraph_rs` in the existing `rust/` cargo workspace. It consumes the per-context methylation-call files emitted by the methylation extractor (`CpG_OT_*`, `CpG_OB_*`, … and the CHG/CHH equivalents) and emits a sorted, gzip-compressed bedGraph and a coverage file.

**Why now:** This unblocks **sub-gate 2** of the extractor epic (#798). Today the Rust extractor's `--bedGraph` path shells out to Perl `bismark2bedGraph` (Phase G, `rust/bismark-extractor/src/subprocess.rs`), so a Rust-vs-Perl byte-identity comparison of the bedGraph/cov streams is tautological (one producer). A genuinely-independent Rust producer makes that comparison a real correctness signal — and, as #797 notes, "byte-identity here pins down the extractor's strand-classification correctness as well."

This crate does **not** modify `rust/bismark-extractor` (a parallel session owns perf work there). Switching the extractor to call this binary inline is a future follow-up in #798's domain, explicitly out of scope here (§9).

### 1.1 Resolved scoping decisions (Felix, 2026-05-29)

| # | Decision | Consequence |
|---|----------|-------------|
| D1 | **`.gz` byte-identity = decompressed-content identity.** Use pure-Rust `flate2`; the gate compares `zcat`-decompressed bytes, not raw compressed bytes. | No subprocess dependency on system `gzip`. Matches the extractor's convention for compressed data files. Raw `.gz` bytes will differ from Perl (zlib ≠ GNU gzip deflate); this is expected and acceptable. |
| D2 | **`--gazillion`/`--scaffolds` = accepted-but-ignored no-op alias.** | In-memory aggregation has no open-filehandle limit, so the scaffold workaround is unnecessary. Byte-identity is guaranteed for **default mode only**. Perl's scaffold mode uses `sort -V` (version sort) → different chr ordering; we deliberately do NOT replicate that. Documented divergence. |
| D3 | **In-memory aggregation; defer external spill.** v1 aggregates all calls into an in-memory map; `--buffer_size` and `--ample_memory` are accepted-but-ignored. | Simplest, fastest, byte-identical. Handles human/mouse CpG comfortably (~1.5–2 GB): per-cytosine + both-strand, that's ~56M positions genome-wide — ~2× the ~28M CpG *dinucleotide* sites, since each strand's C is a distinct coordinate (~38M covered in a typical WGBS sample; measured CpG_OT 19.2M + CpG_OB ~19.2M). **⚠️ RAM caveat (both reviewers, I3):** a full human/mouse `--CX` WGBS run is ~0.6–1.1 B covered positions → **~30–50 GB** in a standard `HashMap` — and this is **strictly more memory-hungry than Perl's default**, which spills to per-chr temp files and never holds the genome in RAM. v1 must (a) document this ceiling, (b) prefer a compact map (e.g. `rustc-hash` + per-chr `Vec<(pos,m,u)>` or interned chr ids) to lower the constant, and (c) **fail cleanly** (clear OOM-context error, not a panic/abort) rather than thrash. External merge-sort spill is the documented future fix (§9). |

---

## 2. The Perl algorithm (what we must reproduce)

Read `bismark2bedGraph` top-to-bottom. The data flow:

1. **Select input files** (`:73-112`). For default (CpG-only) mode, keep only input files whose **basename** matches `^CpG` (`:96`); under `--CX`, keep all input files (`:91-93`). If no files survive the CpG filter → die with the "specify `--CX`" message (`:110-112`).
2. **Open outputs** (`:114-134`). bedGraph via `gzip -c >` (`:114`), write `track type=bedGraph\n` header unconditionally (`:116`). Coverage via `gzip -c >` (`:123`). Optional zero-cov as a **plain (un-gzipped)** file (`:132`).
3. **Pre-split into per-chromosome temp files** (`:160-288`). For each input file, for each call line, split on `\t` → `(chr, pos)` at fields `[2,3]` (`:256`). Open/reuse a temp filehandle **keyed on `$chr` alone** in a hash `%temp_fhs` that **persists across the input-file loop** (`:274`). The temp filename is `{infile_basename}.chr{chr}.methXtractor.temp` (`:276`), where `{chr}` has `|`→`_` and `/`→`_` applied (`:271-272`) **for the filename/key only** — the line content written keeps the original chr (`:283`).
4. **Sort temp files lexicographically** by filename (`:316` `foreach my $in (sort @temp_files)`) and process each: pipe through UNIX `sort -k4,4n` (numeric on position, `:449`), aggregate consecutive equal-position lines, emit one output row per distinct position (`:453-485`, `generate_output` `:590-618`).
5. **Optional `--ucsc` post-pass** (`:514-552`): re-read the written bedGraph, prefix chr names with `chr`, rename `MT`→`chrM`, write `{bedGraph}_UCSC.bedGraph.gz`.

### 2.1 The two byte-identity-critical mechanisms (read carefully)

**(A) Cross-strand merge via shared `%temp_fhs`.** Because `%temp_fhs` is keyed on `$chr` only and persists across the outer input-file loop (`:137`, `:274`), the *second* input file that encounters an already-seen chromosome **appends into the first file's temp file**. So all strands' calls for a given chr land in one temp file owned by whichever input **first saw** that chr. The cross-strand position merge is an emergent property of this filehandle reuse, not an explicit dictionary keyed on position. **Our in-memory aggregation reproduces the *result* (one merged stream per chr, counts summed per position) directly — see §4.**

**(B) Chromosome output order = `sort @temp_files`.** This is a **lexicographic sort of the temp *filename* strings** (`:316`), NOT insertion order and NOT natural-numeric order. For the common single-prefix case this is ASCII order of `chr{name}`:

```
chr1, chr10, chr11, …, chr19, chr2, chr20, …, chrM, chrMT, chrX, chrY
```

Verified empirically: `printf 'CpG_OB.chr1.tmp\nCpG_OB.chr10.tmp\nCpG_OB.chr2.tmp\nCpG_OB.chrX.tmp\nCpG_OB.chrMT.tmp\nCpG_OT.chr1.tmp\n' | sort` →
`chr1, chr10, chr2, chrMT, chrX` (all `CpG_OB`), then `CpG_OT.chr1`.

> ⚠️ The epic #797 body says "match Perl input-order (first chromosome seen wins) … use IndexMap (insertion-order)". **This is incorrect** and would produce byte-DIFFERENT output. The authoritative ordering is the lexicographic temp-filename sort. This SPEC supersedes that note; flagged in §8.

**Ordering is TWO distinct steps — do not conflate them** (corrected rev 1, both reviewers, C1):

*Step 1 — ownership (process order).* `bismark2bedGraph` does **NOT sort its input files**: `@sorting_files = @ARGV` (`:680`) → `@bedfiles` is built preserving that order (`:74-103`) → the main loop iterates `@bedfiles` in **argv order** (`:160`). The **first input file in argv order** to emit a call for a given chromosome **owns** that chr; its basename becomes the temp-filename prefix for that chr (`:276`). Later files' calls for an already-owned chr append into the owner's stream (the §2.1A merge). Ownership is never reassigned.

*Step 2 — output order.* Chromosomes are emitted in **bytewise-ascending order of the synthetic temp-filename string** (`:316` `sort @temp_files`):
```
{owner_basename}.chr{transformed_chr}.methXtractor.temp
```
where `owner_basename` is the Step-1 owner's basename and `transformed_chr` has `|`→`_`, `/`→`_` (`:271-272`). **Output chr name = original (untransformed).** Within a chromosome, positions ascending numeric.

> ⚠️ **The rev-0 claim that "the extractor passes files lexicographically, `CpG_OB` before `CpG_OT`" was WRONG** (unverified assumption). The Perl methylation extractor pushes files in fixed order **`OT, CTOT, CTOB, OB`** (no sort), and `bismark2bedGraph` honors whatever argv order it's given. The order is **caller-determined**; this crate must faithfully reproduce "first file in argv order owns the chr," whatever that order is.
>
> **Concrete byte-divergence this controls** — VERIFIED against Perl ground truth (rev 1.1): with argv order `OT,…,OB`, `CpG_OT` owns every chr present on the OT strand. A chromosome present **only** on OB (e.g. a contig/spike-in with no OT calls) is owned by `CpG_OB`, so its key is `CpG_OB_….chr{X}…`. Because `"CpG_OB" < "CpG_OT"` bytewise (`B` < `T`), that key sorts **BEFORE** all `CpG_OT_….chr…` keys → the OB-only chromosome appears at the **front** of the output, not in natural position and not at the end. (Running Perl on `OT={1,2}, OB={2,MT}` emits chromosome order **`MT, 1, 2`** — MT first.) ⚠️ The rev-1 text and Reviewer A both stated "appears at the end" — that direction was **wrong**; the owner whose basename is alphabetically smaller sorts first. Happy-path tests (chrs on all strands → all OT-owned → plain ASCII chr-name order) never expose this; the chromosome-absent-from-the-first-file test (Phase 3 + Phase 6, anchored to the verified `MT,1,2`) is mandatory.

**Crate requirement:** preserve argv order through file selection — **never glob/`read_dir`-reorder or sort the input file list** (Phase 2). **Gate requirement:** the byte-identity harness must pass the **identical file list in identical order** to both Perl and Rust (Phase 6).

### 2.2 Methylation-call validation (`:558-588`)

Each call line is `id<TAB>strand<TAB>chr<TAB>pos<TAB>call`. Validation decides CpG-vs-nonCpG by the call letter (`:564` — `/^z/i`):

- **CpG** (`call` ∈ {`Z`,`z`}): valid iff (`+`,`Z`) or (`-`,`z`).
- **non-CpG** (`call` ∈ {`X`,`x`,`H`,`h`}): valid iff (`+`, one of `Z`/`X`/`H`) or (`-`, one of `z`/`x`/`h`).

Two distinct failure modes (corrected rev 1, Reviewer A) — match Perl exactly (Felix 2026-05-29):
- **Missing field** (strand or call `undef`, i.e. a malformed/short line) → Perl `croak`s and **dies** (`:560`,`:562`). Rust: return a hard error → exit 1.
- **Present-but-inconsistent** (both fields present, wrong combination, e.g. `+z`) → warn to stderr and **skip the line** (`:369-372`, `:467-470`) — do not count it.

In practice the extractor always emits well-formed 5-field lines, so the croak path never fires; we replicate it for exact fidelity.

Counting (`:374-381`, `:471-477`): `strand == '+'` → methylated; else → unmethylated. (Validation guarantees `+`↔uppercase, `-`↔lowercase.)

### 2.3 Methylation percentage formatting — `%.15g` (THE hard part)

Perl computes `meth% = (meth / total) * 100` (`:399`, `:601`) and prints it with **default NV stringification**, which is C `printf("%.15g", x)`. The rounding line (`:602`) is **commented out** — there is no 2-dp rounding. Verified empirically:

| meth/total | Perl output |
|---|---|
| 1/2 | `50` |
| 1/1 | `100` |
| 1/3 | `33.3333333333333` |
| 2/3 | `66.6666666666667` |
| 1/7 | `14.2857142857143` |
| 5/6 | `83.3333333333333` |
| 1/1000000 | `0.0001` |
| 1/10000000 | `1e-05` ← **switches to scientific at exp ≤ −5** |
| 2/300000 | `0.000666666666666667` |

Rust has **no built-in `%g`**. A faithful `%.15g` reimplementation is a **hard requirement**, isolated in its own module with a property test against Perl (§6, Phase D). Reviewer B **empirically validated a pure-Rust approach** — `format!("{:.14e}", x)` post-processed per C `%g` rules — matching C `printf("%.15g")` across **2,003,000 fractions + all scientific-notation boundary cases with 0 mismatches**. That's the primary implementation; a libc `snprintf("%.15g")` shim is the documented fallback if a platform `%g` discrepancy (macOS↔Linux, Reviewer A) ever surfaces. The **same** formatted string is written to both bedGraph and coverage (Perl reuses the one `$meth_percentage` variable), so format once per position.

---

## 3. CLI flag inventory

All flags from the Perl `GetOptions` block (`bismark2bedGraph:637-651`). Citations are Perl line numbers.

| # | Flag | Aliases | Default | Behavior | Byte-identity / interactions | Perl ln |
|---|------|---------|---------|----------|------------------------------|---------|
| 1 | `--help` | `--man` | OFF | Print help, exit 1. | — | 637 |
| 2 | `--dir` | — | `''` (CWD) | Output directory. | Created if missing; resolved to absolute; trailing `/` enforced. | 638 |
| 3 | `-o`/`--output` | — | **required** | bedGraph output filename. | No path separators allowed (die `:729`). `.gz` appended if absent (`:733-735`). | 639 |
| 4 | `--no_header` | — | OFF | Treat input first line as a version header to skip / not. | When OFF, consume + skip the **first line of each input file** (`:182`,`:244`) regardless of content (a data-loss footgun — gate ON/OFF in Phase 6). Plus: skip any line matching `Bismark` **anywhere** in the stream. ⚠️ Perl uses TWO regexes — `/^Bismark /` (with space) in the pre-split loop (`:231`,`:249`) but `/^Bismark/` (**no space**) on the actual output-driving loop (`:454`, `:343`). Our single-pass operative rule is the **no-space** `starts_with("Bismark")`. | 640 |
| 5 | `--cutoff` | — | 1 | Min total coverage to emit a position. | Must be `> 0` (die `:748-751`). Positions with `0 < total < cutoff` are dropped. | 641 |
| 6 | `--remove_spaces` | — | OFF | (Perl: replace whitespace in id with `_` for sort safety.) | **No effect on our output** — aggregation ignores the id field. Accepted; the Perl `.spaces_removed.txt` intermediate is NOT produced. Documented divergence (intermediate artifact only; outputs identical). | 642 |
| 7 | `--counts` | — | ON | (Perl: include counts in coverage.) | **No-op in Perl too** — `$counts` defaults to 1 and is never read in output logic. Coverage always carries counts. Accepted, ignored. | 643 |
| 8 | `--CX` | `--CX_context` | OFF | Use ALL input files (every cytosine context), not just `^CpG`. | Changes input-file selection (§2 step 1). | 644 |
| 9 | `--buffer_size` | — | `2G` | (Perl: UNIX sort `-S` buffer.) | **Accepted-but-ignored** (D3, in-memory). Format still validated for CLI parity (`\d+%` or `\d+[KMGT]`, die `:766`). Mutex with `--ample_memory` (die `:762-764`) preserved. | 645 |
| 10 | `--version` | — | OFF | Print version block, exit 0. | Must reproduce Perl version text (`:665-677`). | 646 |
| 11 | `--gazillion` | `--scaffolds` | OFF | (Perl: skip per-chr pre-split, sort everything with `-k3,3V`.) | **Accepted-but-ignored no-op** (D2). Byte-identity NOT guaranteed in this mode. Mutex with `--ample_memory` (die `:782-786`) preserved. | 647 |
| 12 | `--ample_memory` | — | OFF | (Perl: in-RAM array sort.) | **Accepted-but-ignored** (D3 — we're always in-memory). Mutex with `--buffer_size` and `--gazillion` preserved. | 648 |
| 13 | `--zero_based` | — | OFF | Emit an additional 0-based half-open coverage file. | Plain (un-gzipped) `.zero.cov`; quirky filename (§4.3). | 649 |
| 14 | `--ucsc` | — | OFF | Emit an additional UCSC-compatible bedGraph. | `chr` prefix + `MT`→`chrM`; `{out}_UCSC.bedGraph.gz` (§4.4). | 650 |

Positional args (`@ARGV`, `:680`): one or more methylation-extractor call files (`.txt` or `.txt.gz`). Empty → print help, exit (`:683-689`).

---

## 4. Output topology & formats

`{out}` = the normalized bedGraph filename (always ends `.gz`). All outputs written into the resolved `--dir`.

### 4.1 bedGraph (`{out}`, gzipped)
```
track type=bedGraph
{chr}\t{pos-1}\t{pos}\t{meth%}
```
Header line always written (`:116`). Data: 0-based start, 1-based end (`:406`,`:607`). One line per emitted position. `{meth%}` per §2.3.

### 4.2 Coverage (`.bismark.cov.gz`, gzipped)
Filename: `{out}` with trailing `bedGraph.gz`→`bismark.cov.gz`, else `.bismark.cov.gz` appended (`:118-121`).
⚠️ **Non-`bedGraph` output names (Reviewer B):** `-o sample` first normalizes to `sample.gz` (`.gz` append, no `bedGraph` token), then the coverage regex `s/bedGraph\.gz$/…/` **fails** → fallback appends → `sample.gz.bismark.cov.gz`. The Phase 1 filename test table must cover both `-o foo.bedGraph` (→ `foo.bismark.cov.gz`) and `-o sample` (→ `sample.gz.bismark.cov.gz`).
```
{chr}\t{pos}\t{pos}\t{meth%}\t{meth_count}\t{unmeth_count}
```
1-based start = end (`:409`,`:610`). No header line.

### 4.3 Zero-based coverage (`--zero_based`, **plain text, NOT gzipped**)
Filename derivation (`:126-133`): start from `{out}` (which ends `.gz`); try `s/bedGraph$/bismark.zero.cov/` → **fails** (string ends `.gz`); fall to `s/$/.bismark.zero.cov/` → **`{out}.bismark.zero.cov`** (e.g. `foo.bedGraph.gz.bismark.zero.cov`). ⚠️ This is a latent Perl filename quirk; **replicate exactly**, do not "fix".
```
{chr}\t{pos-1}\t{pos}\t{meth%}\t{meth_count}\t{unmeth_count}
```

### 4.4 UCSC bedGraph (`--ucsc`, gzipped)
Filename: `{out}` with `.gz` stripped, then `_UCSC.bedGraph.gz` appended (`:524-526`) → `foo.bedGraph_UCSC.bedGraph.gz`. Re-emit the bedGraph (header line first, `:533-534`) with: `MT`→`chrM` (`:537-539`); any chr not already starting `chr` → `chr{chr}` (`:542-545`).

---

## 5. Module layout (house style — matches bismark-extractor)

```
rust/bismark-bedgraph/
  Cargo.toml            # workspace-inherited metadata; [[bin]] name = "bismark2bedGraph_rs"
  SPEC.md               # this file, copied in at Phase A
  src/
    main.rs             # CLI dispatch; --version/--help; exit codes 0/1/2
    lib.rs              # public API + module exports; version_string()
    cli.rs              # clap-derive Cli + validate() -> ResolvedConfig (mutex/precondition checks)
    error.rs            # thiserror BismarkBedgraphError
    filename.rs         # bedGraph/coverage/zero/ucsc name derivation (the trailing-ext quirks)
    input.rs            # per-context file selection + line parsing + header/^Bismark skip
    validate.rs         # validate_methylation_call (CpG vs nonCpG)
    aggregate.rs        # (chr,pos)->(meth,unmeth) map + chr ownership + temp-filename ordering key
    fmt_g.rs            # faithful %.15g formatter (+ its own tests)
    output.rs           # bedGraph/coverage/zero writers (flate2), cutoff filter, emission loop
    ucsc.rs             # --ucsc post-pass
  tests/
    *.rs                # synthetic-line unit tests, smoke tests (assert_cmd), byte-identity gate (#[ignore])
    fixtures/           # small committed CpG_OT/CpG_OB fixtures + expected decompressed outputs
```

**Cargo conventions:** inherit `edition`/`rust-version`/`license`/etc. via `.workspace = true`; exact-pin all deps (`flate2 = "=1.1.9"`, `clap = "=4.5.30"` (derive), `thiserror = "=2.0.0"`). Add `"bismark-bedgraph"` to `rust/Cargo.toml` `members`. Binary takes the `_rs` suffix during coexistence (drop after v1.0). `disable_version_flag = true` + custom `--version` in `main.rs`.

---

## 6. Implementation phases (see EPIC.md + per-phase PLANs)

| Phase | Deliverable | Byte-identity locked |
|-------|-------------|----------------------|
| A | Crate skeleton, workspace wiring, CLI + flag inventory, filename derivation (pure fns) | filename quirks (unit) |
| B | Input file selection, line parsing, header/`^Bismark` skip, call validation | parse + validate (unit) |
| C | In-memory aggregation + chr ownership + temp-filename ordering key | chr ordering + cross-file merge (unit) |
| D | `%.15g` formatter + bedGraph/coverage/zero writers + cutoff | number format (property) + line layout (golden) |
| E | `--ucsc` post-pass + accepted-but-ignored flags wired end-to-end | UCSC transform (unit) |
| F | Byte-identity gate vs Perl v0.25.1 on colossal + flag matrix + RELEASE_CHECKLIST | full-stream decompressed identity (real data) |

---

## 7. Edge cases (first-class)

- **No CpG files + default mode** → die with Perl's exact message (`:111`).
- **Empty input file / all-header** → valid; produces just the `track type=bedGraph` header + empty coverage (cf. real bug reports #595 empty cov, #774 cov structure).
- **Position with `0 < total < cutoff`** → dropped (no row).
- **Invalid call line** → warn + skip; not counted.
- **Chr name with `|` or `/`** → output keeps original; ordering key uses transformed (`_`).
- **chr10 vs chr2** → ASCII order (chr10 before chr2) — the headline ordering test.
- **Same position on `+` and `-` across input files** → counts summed into one row (cross-strand merge).
- **Gzipped vs plain `.txt` input** → both accepted (`:201-206`, `:167-172`); detect by `.gz` suffix.
- **`--cutoff` non-positive** → die (`:748-751`).
- **`--buffer_size` + `--ample_memory`** (or `--gazillion` + `--ample_memory`) → die for CLI parity even though both ignored.
- **meth% scientific-notation boundary** (e.g. 1/10000000 → `1e-05`) → covered by the `%.15g` property test (must **deliberately seed high-total cases** — random small-denominator pairs never reach the `e`-notation branch; Reviewer B).
- **Malformed/short input line** (missing strand or call field) → **die** (Perl croak, §2.2); **present-but-inconsistent** → warn+skip.
- **Chromosome absent from the first argv file** (e.g. MT only on `CpG_OB` under `OT,…,OB` order) → owned by `CpG_OB` → because `"CpG_OB" < "CpG_OT"` its key sorts **before** all OT-owned chrs → appears at the **front** (verified Perl emits `MT, 1, 2`). §2.1B. **The make-or-break ordering case** — explicit test in Phase 3 + Phase 6.
- **`--no_header` ON vs OFF** → OFF unconditionally drops the first line of each file (data-loss footgun); both states gated in Phase 6.
- **CRLF input** → Perl `chomp` strips `\n` only, leaving a trailing `\r` on the **last** field (the call letter), which then fails the exact-equality validation → warn+skip every line. We **match this**: strip `\n` only (not `\r`), exact-byte field comparison. Extractor output is LF-only, so this is a degenerate-input parity note, not a normal path.
- **Very large `--CX` aggregate** → may exceed RAM (§1.1 D3); must fail with a clear error, not panic/abort/thrash.

---

## 8. Open questions / flagged divergences

| # | Item | Disposition |
|---|------|-------------|
| Q1 | Epic #797 "IndexMap insertion order" guidance contradicts the actual two-step ordering (§2.1B). | **Resolved (rev 1, both reviewers):** ownership = first file in **argv order** (no input-file sort); output order = bytewise sort of `{owner}.chr{chr}…temp` strings. NOT insertion order, NOT lexicographic *file* order. Will correct the #797 body in the spec PR. |
| Q2 | `--gazillion` scaffold-mode chr ordering (`sort -V`) deliberately not replicated (D2). | Documented divergence; byte-identity gate covers default mode only. |
| Q3 | `--remove_spaces` intermediate `.spaces_removed.txt` not produced (no effect on outputs). | **Resolved (Felix 2026-05-29):** accepted — documented divergence; outputs identical. |
| Q4 | `--version`/`--help` exact text — replicate Perl byte-for-byte or modernize? | **Resolved (Felix 2026-05-29):** **lightly modernize** both. The byte-identity contract covers only the *data* streams (bedGraph/cov/zero/UCSC); stderr banners, `--version`, and `--help` text are NOT byte-gated and may be modernized to match the Rust suite's house style. |

---

## 9. Out of scope (v1) / future

- **`coverage2cytosine`** — separate crate, not started.
- **Wiring the extractor to call this binary inline** (replacing `subprocess.rs`'s Perl call) — future task in #798's domain; do NOT touch `rust/bismark-extractor` here.
- **External merge-sort spill for `--buffer_size`** (>RAM `--CX` datasets) — documented future capability (D3).
- **Faithful `--gazillion` version-sort** (D2) — only if a real scaffold-genome byte-identity need arises.
- **Raw-`.gz`-byte identity** (D1) — not pursued; decompressed-content identity is the contract.

---

## 10. Verification (full detail in Phase F PLAN)

Real-data byte-identity on **colossal** (see memory `reference_colossal_access`): `dcli ssh colossal`, env `bioinf` (Perl Bismark v0.25.1 + `bismark2bedGraph`), data under `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/`. **Use a distinct out-dir** from any other running session (incl. the parallel `coverage2cytosine` session). Pin `LC_ALL=C` for Perl's UNIX-sort collation parity.

**Harness invariants (rev 1):**
- **Identical argv order** to Perl and Rust — same file list, same order (§2.1B C1). Never glob/sort the file list in the harness; pass an explicit ordered list.
- **Baseline runs in default mode** — assert the comparison is taken **without** `--ample_memory`/`--gazillion`/`--buffer_size` (the extractor *can* forward these; the byte-identity baseline must not use them — D2/D3).
- **Never `diff` raw `.gz`** — always decompress first (D1); bake this into the script so a contributor can't accidentally byte-compare compressed files.

Gate (decompressed content, D1): for each cell of {CpG-only, `--CX`} × {`--cutoff` 1, 5, 10} × {`--no_header` ON, OFF}, plus `--zero_based` and `--ucsc` variants:
```
zcat rust/{out}            | <normalize> > rust.bedGraph
zcat perl/{out}            | <normalize> > perl.bedGraph
diff rust.bedGraph perl.bedGraph            # must be empty
# same for .bismark.cov.gz, and plain diff for .zero.cov / zcat for _UCSC
```
Inputs are Perl-extractor-produced `CpG_*`/`CHG_*`/`CHH_*` files (independent of our producer). A small synthetic fixture set is committed for CI; the full-genome run is `#[ignore]` + env-var-gated like `bismark-dedup`'s `byte_identity_real_data.rs`.

---

## 11. Revision history

**rev 1.1 (2026-05-29)** — implementation + ground-truth verification:
- Crate `rust/bismark-bedgraph` implemented (Phases 1–6); 70+ unit/doctest + 8 hermetic CI byte-identity cells pass; live Perl-vs-Rust comparison **byte-identical (decompressed)** across default / `--cutoff` / `--CX` / `--zero_based` / `--ucsc` / `--no_header`.
- **Ordering DIRECTION corrected (§2.1B, §7):** running the Perl as ground truth (`OT={1,2}, OB={2,MT}` → emits `MT, 1, 2`) showed the OB-owned chromosome sorts **FIRST**, because `"CpG_OB" < "CpG_OT"`. The rev-1 text and Reviewer A's "appears at the end" were the wrong direction. The aggregator code was already correct; the SPEC text + a test expectation were fixed.
- **Deviation (I3):** the planned "clean OOM error" is not implemented — Rust's allocator aborts on exhaustion and can't be `Result`-caught without `try_reserve` plumbing; the `OutOfMemory` error variant was removed and the RAM ceiling is documented instead (honest non-guarantee).

**rev 1 (2026-05-29)** — dual plan-review (reports `PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md`); all Critical + Important findings folded in:
- **C1 (CRITICAL, both reviewers):** chromosome ordering corrected. `bismark2bedGraph` does NOT sort input files; ownership = first file in **argv order**; output order = bytewise sort of temp-filename strings. The rev-0 "lexicographic, `CpG_OB` first" claim was a wrong unverified assumption. Crate must preserve argv order; gate must pin identical argv. New mandatory test: chromosome-absent-from-first-file. (§2.1B, §7, §10.)
- **I1 (both):** `^Bismark` skip — Perl uses `/^Bismark /` (space) pre-split but `/^Bismark/` (no space) on the output path (`:454`); operative rule is no-space `starts_with("Bismark")`. (§3 row 4.)
- **I3 (both):** `--CX` RAM ceiling ~30–50 GB, strictly worse than Perl default; document ceiling, compact map, clean-fail. (§1.1 D3, §7.)
- **Reviewer A:** validation `croak`-on-missing-field vs warn+skip-on-inconsistent split — match Perl exactly (Felix). (§2.2.)
- **Reviewer B:** `-o sample` filename fallback `sample.gz.bismark.cov.gz` added to test surface (§4.2); `--no_header` ON/OFF added to the gate matrix (§10); CRLF parity resolved (§7); `%.15g` property test must seed high-total cases (§7); pure-Rust formatter empirically validated (0/2,003,000 mismatches) as primary, libc shim as fallback (§2.3).
- Both reviewers independently **verified correct**: the full `%.15g` table, all filename quirks, the validation truth table, cutoff semantics, the always-on `track type=bedGraph` header, and bytewise/locale-independent ordering (`Ord<[u8]>` + `LC_ALL=C` align).

**rev 0 (2026-05-29)** — initial draft; three scoping decisions resolved with Felix (D1/D2/D3, §1.1).
