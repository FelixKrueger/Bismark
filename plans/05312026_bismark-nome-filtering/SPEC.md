# `bismark-nome-filtering` — SPEC

**Status:** rev 3 (**Phases A & B IMPLEMENTED + COMMITTED** to `rust/nome-filtering`; dual code-review APPROVE [no Critical/High] + plan-manager COMPLETE; output **byte-identical to Perl v0.25.1**, verified by Perl-generated decompress-then-compare goldens + a direct `cmp` cross-check; clippy `-D warnings` clean). Grounded against Perl `NOMe_filtering` v0.25.1 (660 LOC) + the `bismark-io` / `bismark-dedup` / `bismark-coverage2cytosine` Rust patterns. **Phase C (real-data byte-identity gate on oxy — Felix directive 2026-05-31) is the remaining v1.0 step.**

**Target:** Perl `NOMe_filtering` (v0.25.1) at the Bismark repo root. **Byte-identical to Perl v0.25.1** on the (single) output file *and* on the on-disk output-file state of the empty-input error path (§D4).

**Branch / worktree:** `rust/nome-filtering` in an isolated git worktree at `../Bismark-nome` (off `origin/rust/iron-chancellor` @ `2b05ec8`). New crate `bismark-nome-filtering` in the existing `rust/` workspace.

> ⚠️ **Scope clarity.** This is the **standalone** `NOMe_filtering` tool (its own 660-line Perl script at the repo root). It is **NOT** `coverage2cytosine --nome-seq` (an in-c2c flag being ported separately on `rust/c2c-v1x`). Separate crate, separate branch, separate byte-identity target. **Does NOT touch** `rust/bismark-coverage2cytosine` or any sibling worktree (parallel sessions own those).

---

## 1. Context — why this change

Bismark's post-alignment tools are being rewritten Perl→Rust (epic on `rust/iron-chancellor`) for speed and maintainability, each port held to **byte-identity** against Perl v0.25.1 and validated on real data before any v1.0 tag. `NOMe_filtering` is the per-read NOMe-Seq classifier that consumes the methylation extractor's `--yacht` output and emits a per-read CG/GC methylation tally. It is small (one core sub) but arithmetic-dense, with several reachable Perl quirks that must be reproduced exactly. Porting it (a) completes another tool in the suite, (b) lets the future Rust pipeline run NOMe-Seq end-to-end without shelling to Perl, and (c) promotes the shared "Bismark genome into memory" reader into `bismark-io` so c2c-style ports stop re-deriving it.

## 2. Scope

**In scope (v1.0 — byte-identical to Perl v0.25.1):**
- Streaming the `--yacht` 8-field input (gz-aware; `^Bismark` header skip; consecutive-ReadID grouping).
- The genome reader (two plain suffixes, uppercase, Mus skip, dup-name, first-token name) — **promoted into `bismark-io`**.
- The core per-read `cytosine_lookup`: suitability guard, ±2 bp extended-sequence extraction, fwd-C/rev-G trinucleotide + upstream-context extraction, CG/CHG/CHH classification, NOMe ACG/TCG (CpG) and GpC (non-CpG) filters, per-read meth/unmeth tally.
- The always-gzipped output: `.manOwar.txt.gz` name derivation, the 8-column header, exact column format, and the **empty-input header-then-error on-disk artifact** (§D4).
- The live CLI surface + Perl-faithful acceptance of inert flags + the reachable die + the `--dir` input/output path contract (§4).
- Local Perl-v0.25.1 goldens on synthetic fixtures, then a **real-data byte-identity gate**.

**Out of scope / not byte-identity-gated:**
- **STDERR.** The `warn` progress chatter is informational; not gated (dedup/c2c precedent). The two `sleep(2)` calls are dropped (intentional deviation; no output effect). _Note: the empty-input on-disk file IS gated (§D4) — it is an output artifact, not STDERR._
- Performance targets (advisory only for v1.0; Perl is single-threaded + whole-genome-in-RAM, the Rust port matches that model).
- Migrating **c2c** onto the promoted genome module (separate session owns that crate — noted as a follow-up, not done here).

**Accepted divergences (out-of-distribution / non-gated channels — rev 2, from Phase-B code review):** these cannot occur on real `bismark_methylation_extractor --yacht` output, or live on channels (STDERR / exit code) that are not byte-identity-gated:
- **Malformed yacht line** (`<8` TAB fields or a non-numeric `pos`/`start`/`end`): the Rust port **skips** it (A4). Side-effect (code-review A): a malformed line interspersed *inside* a same-ReadID run does NOT trigger a flush in Rust, whereas Perl's lenient `split` yields a differing implied id and **does** flush — so Perl may emit two lines where Rust emits one. Out-of-distribution.
- **Coordinate reformatting:** Rust parses `pos/start/end` as `u32` and re-emits canonical decimal; Perl echoes the raw field. Diverges only on non-canonical numerics (leading zeros, trailing space, signs).
- **Multi-char `state`/`call` field:** Rust keys on the first byte; Perl compares the whole field. Only the first byte is ever `+`/`-` (col 2) or `z/Z/x/X/h/H` (col 5) on real data.
- **Non-UTF-8 input byte:** Rust `lines()` hard-errors (exit 1); Perl preserves the raw byte (exit 0). Real yacht IDs are ASCII. (This also makes the byte-faithful `id`/`chr` output write moot for inputs reachable via `lines()`, but the write is kept for defensiveness.)
- **Error exit code:** Rust exits `1` on any error; Perl `die` exits `255`. Exit codes are not gated (STDERR/exit-status are informational).
- **`else→warn-skip` context branch is unreachable:** for a real uppercased genome `tri[0]` is always `C` (forward `tri` starts at the scanned C; reverse `tri` is the revcomp of a window ending in the scanned G → `complement(G)==C`), so `classify` never returns `None`. The branch is retained (matching Perl's structure) but is dead code; the planned "VS-N warn-skip" test was dropped as impossible. N-context is instead covered by the `ncontext` golden (`CNG`→CHG / `CNN`→CHH classification).

## 3. Resolved decisions

| # | Decision | Choice | Consequence |
|---|----------|--------|-------------|
| D1 | Genome reader structure | **Promote to `bismark-io`** | New `bismark_io::genome` module, tier-parameterized `load(folder, &tiers)`. **Additive, NO version bump** (bismark-io is `1.0.0-beta.8`; all 7 siblings pin `=1.0.0-beta.8` — bumping breaks their pins). c2c untouched. Verified at kickoff: no `genome` module exists yet (only `cram_ref.rs`). |
| D2 | Inert/vestigial flags | **Accept-and-ignore (Perl-faithful)** | `--zero_based`/`--CX`/`--GC`/`--gzip`/`--nome-seq`/`--merge_CpGs` parsed, no output effect. Reproduce only the `--merge_CpGs`+`--CX` die. |
| D3 | Reverse-read-at-chr-start edge | **Faithfully replicate** | A `perl_substr` helper reproduces negative-offset-from-end + over-length truncation, so these reads emit the same all-zero line Perl does. |
| D4 | Empty / all-`^Bismark` input | **Replicate Perl exactly** (Felix, 2026-05-31; rev 1) | Open the writer, **write the header line, THEN raise `EmptyInput`** (non-zero exit). Perl leaves a header-only `.gz` on disk whose **decompressed content is exactly the 57-byte header line** (`:74-78` before the read loop, `:173-175` die). The Rust port leaves the **same** on-disk artifact. NOT a clean pre-output error. (Review B-L1; verified by `vs_empty` golden.) |
| D5 | Genome error ownership | **Module-local `GenomeError`** (rev 1) | `bismark_io::genome` exposes its own `GenomeError` (NoGenomeFasta / DuplicateChromosomeName / MalformedFastaHeader / ChromosomeTooLong / Io); the NOMe crate maps it into `BismarkNomeError`. Keeps the public `BismarkIoError` enum **untouched**, so "additive, no version bump" cannot break a sibling's non-exhaustive `match`. (Review A-I5 / B-A3.) |

## 4. CLI flag inventory (Perl `process_commandline:394-514`)

**Live (affect behavior):**
| Flag | Default | Behavior | Perl ln |
|------|---------|----------|---------|
| positional `<infile>` | (required) | yacht input file; resolved **relative to `--dir`** (see note); must exist else die (no file→help+exit; non-existent→die). | 444-455 |
| `-g`/`--genome_folder` | (required) | FASTA genome dir; **mandatory** (die without it). | 407, 488-495 |
| `--dir` | `''` (CWD) | output directory **AND input directory** — Perl `chdir`s into it, then opens BOTH the input and the output by bare filename relative to it. | 406, 58-77 |
| `--parent_dir` | `getcwd()` | **Effectively inert in the Rust port** — Perl uses it only to `chdir` back after reading the genome (`:457-462,589`). Rust reads the genome by explicit/absolute path without changing CWD, so `--parent_dir` has no observable output effect. Accept-and-ignore. (Review A-I6.) | 410 |
| `--version` | — | print version + exit. | 411, 429-441 |
| `--help`/`--man` | — | print help + exit(1). | 405, 424-427 |

**⚠️ `--dir` path contract (Review A-C2, B-L4 — the highest-value CLI subtlety).** Perl `chdir $output_dir` at the top of `per_read_filtering` (`:58-61`), then opens the **input** by bare filename (`:66-70`) and the **output** by bare filename (`:77`) — *both relative to `--dir`*. The methylation extractor invokes NOMe with a bare-filename input + `--dir`. The Rust port must replicate by resolving **input = `output_dir.join(infile)`** for the read and **output = `output_dir.join(derived_name)`** for the write — **without** a real process `chdir`. If the Rust port resolves the input relative to the original CWD, it reads/writes the wrong location with no failing golden (see §12 VS-dir).

**Inert (parsed, no output effect — accept-and-ignore per D2):**
`--zero_based`, `-CX`/`--CX_context`, `--GC`/`--GC_context`, `--gzip` (output is *always* gzipped), `--nome-seq` (`$nome` defaults to 1 and is non-negatable → NOMe filtering is *unconditional*; there is no `--no-nome-seq`), `--merge_CpGs` (alone, never referenced in the processing path). Verified at review: `--CX` sets `$CX_context`/skips `$CpG_only` but neither is consumed downstream; `--GC` auto-sets `$gc_context` (never consumed); none reach output (Review A-§2.7).

**Reachable die:** `--merge_CpGs` **+** `-CX` → die "Merging … only supported if CpG-context is selected only (lose the option --CX)" (`:498-500`). The companion `--split_by_chromosome` check (`:501-503`) is **unreachable** (`$split_by_chromosome` has no GetOptions entry → always false) — document, do not implement a path for it.

**Output-naming derivation (`:464-468`):** `out = infile`; strip **one** trailing `.gz`; strip **one** trailing `.txt`; append `.manOwar.txt`; then force `.gz` at write time (`:74-76`). e.g. `x.txt.gz`→`x.manOwar.txt.gz`; `x.gz`→`x.manOwar.txt.gz`; `x.txt`→`x.manOwar.txt.gz`; `x.gz.gz`→`x.gz.manOwar.txt.gz`; `x.txt.txt`→`x.txt.manOwar.txt.gz`; `x`→`x.manOwar.txt.gz`. ⚠️ **Each extension is stripped at most once, independently** — do NOT reuse `bismark-dedup`'s multi-strip `strip_suffix` loop, which would strip both `.gz`s (Review A-I4).

## 5. Input format (`--yacht`)

8 tab-separated fields, **no header** in real yacht files (the `Bismark …` banner is STDERR-only), but the Perl defensively skips any `^Bismark` line (`:91`) — reproduce. gz input via filename `gz$` suffix (`:66`) → decompress (`MultiGzDecoder`).

```
<seq-ID>  <state +/->  <chr>  <pos>  <call z/Z/x/X/h/H>  <read start>  <read end>  <read orientation +/->
```
Parsed as `($id,$state,$chr,$pos,$context,$start,$end,$strand)` (`:93`). Column 2 (`$state`) is the **+/-** that drives the meth/unmeth tally; column 5 (`$context`) is the **call letter** (case carries the same meth signal but the tally keys on column 2). Column 8 (`$strand`) is parsed but **never used** in the processing path. For reverse reads the yacht emitter writes `read start` = rightmost coord, `read end` = leftmost (so `start > end`).

**Per-read map key space (Review A-§2.1):** the per-read map is keyed by the **yacht column-4 genomic position** (an absolute 1-based chromosome coordinate), NOT a read-relative index. `cytosine_lookup` looks it up via the derived `g = pos + offset - 1` (§8).

**Same-position rule (Review B-L2):** two yacht lines at the **same** column-4 position within one read → **last line wins** (overwrite both `state` and `call`). Implement with an **unconditional insert in input order** (`map.insert(...)`), NOT `entry().or_insert()` (which would silently flip to first-wins).

**Per-read grouping (`:89-168`, streaming):** consecutive same-ReadID lines accumulate into the `pos → {state, call}` map; the read's `start/end/chr` are taken from its **first** line. On ReadID change, flush the previous read then re-init for the new one. The **last** read is flushed after the loop (`:177-219`). Non-consecutive blocks of the same ReadID are treated as TWO separate reads (grouping is by *consecutive* id).

**Flush-vs-seed separation (Review B-L3):** the in-loop `else` branch does TWO things — (1) flush the previous read, (2) seed the new read's first line. The **shared flush routine (§8) must do ONLY the flush**; seeding the new read belongs to the loop body *after* the flush returns. The EOF flush calls the same routine and does NOT seed (there is no next read). Conflating the two would diverge at EOF.

**Empty / all-`^Bismark` input (D4):** if no data line was ever read (`last_read` never defined), Perl — having *already* opened the writer and printed the header (`:74-78`) — hits the die at `:173-175`. The Rust port replicates: header written, then `EmptyInput` error + non-zero exit, leaving the header-only `.gz` on disk.

## 6. Output

- Single file, **always gzipped**, named per §4 (`.manOwar.txt.gz`), written into `--dir` (or CWD) at `output_dir.join(derived_name)`.
- Header line (`:78`), written **before** the read loop (so it survives the empty-input error path, D4): `ReadID\tChr\tStart\tEnd\tmeth_CG\tunmeth_CG\tmeth_GC\tunmeth_GC\n`. _Columns 7/8 are labelled `meth_GC`/`unmeth_GC` in the header but the underlying counters are `meth_nonCG`/`unmeth_nonCG` — do not rename._
- One data line per **suitable** read (`:389`): `id\tchr\toffset\tend\tmeth_CG\tunmeth_CG\tmeth_nonCG\tunmeth_nonCG\n`, counts as bare integers (`u32` Display; no float/`%`-format). `offset`/`end` are always **ascending** (min/max of start/end) because reverse reads call `cytosine_lookup(...,last_end,last_start,...)` (`:155,217`).
- Output lines are emitted in **input read order** — compared **un-sorted** (Review A-I9). No sort step is applied to the NOMe output anywhere (a sort would diverge).
- gzip byte-identity is asserted **after decompression** (the gzip container is impl-dependent; `flate2` `GzEncoder` + `Compression::default()`, like c2c).

## 7. Genome reader — promoted to `bismark-io` (§D1)

New module `bismark_io::genome` (distinct from `cram_ref`), mirroring Perl `read_genome_into_memory:516-590` + `extract_chromosome_name:592-602`:
- **`load(folder: &Path, tiers: &[&str]) -> Result<Genome, GenomeError>`** — tier-parameterized glob priority (first non-empty tier wins; no union; exclude dotfiles). **NOMe passes `[".fa", ".fasta"]` (two PLAIN suffixes, no `.gz`)** — a deliberate divergence from c2c's four-tier `.fa/.fa.gz/.fasta/.fasta.gz`. ⚠️ **Footgun (Review A/B-A4):** a `.fa.gz`-only genome (common, and accepted by c2c) is **invisible** to NOMe and triggers the "does not contain any sequence files" error — Perl-faithful, but the c2c session may be surprised. See pitfall P14.
- Skip `Mus_musculus.NCBIM37.fa` (`:535`). Uppercase on load (`:571`). Strip `\r` (`:541,549`). Chromosome name = first whitespace token after `>` (`:592-602`). **Duplicate name → error** (`:553-556, 575-578`).
- **Errors are a module-local `GenomeError` (§D5)**, NOT new `BismarkIoError` variants — keeps bismark-io's public enum untouched so the no-version-bump promotion can't break a sibling's non-exhaustive match.
- **Documented divergence inherited from c2c (Review A-O4/B-A5):** a bare/nameless `>` header → the noodles-based reader **errors** (`MalformedFastaHeader`), whereas Perl stores an empty-name chromosome. Cannot occur on a Bowtie2-built Bismark genome; pinned by a test, not worked around.
- Store `HashMap<Vec<u8>, Vec<u8>>` (name→uppercased bytes); no public insertion-order iterator (order never reaches output — output order is input read order). Reuse c2c's `u32` chr-length guard.
- **Additive, no version bump** (§3 D1). Phase A verifies the workspace still builds with all siblings pinned at `beta.8` (`cargo build --workspace`). **Unknown-chromosome reads** (`$last_chr` not in the map): Perl `length($chromosomes{$last_chr})` on an absent key yields undef→0 in the numeric guard, so the guard's second clause fails and the read is **silently skipped** (no line). In Rust, `genome.get(name)` returns `None` → treat length as 0 → guard fails. (Review A-O2 wording fix.)

## 8. Core algorithm — `cytosine_lookup` + the flush path (THE byte-identity crux)

Ported from `per_read_filtering:48-230` (the in-loop flush `:116-168` and the identical EOF flush `:177-219`) + `cytosine_lookup:242-391`. Use **one shared flush routine** for both flush sites (avoid the Perl duplication and its divergence risk — cf. the dedup "dual-driver back-port" memory). Per §5, the shared routine does the flush ONLY (no seeding).

> **Both reviewers verified every claim in this section against live Perl v0.25.1 — no correctness defects.** The notes below are the exact transcriptions plus the structural cautions they flagged.

**Per read (after grouping):**
1. `length = (end>=start) ? end-start+1 : start-end+1` (`:117-122`).
2. **Suitability guard (`:132`)** — note it tests `$last_start` for *both* strands: `suitable = (last_start - 2 > 1) && (chr_len >= last_start - 2 + length + 4)`. On an unknown chr, `chr_len`=0 → not suitable → skip (no line). The `>=` (not `>`) boundary is exact — pin it (§12 VS-guard).
3. If suitable, extract (using `perl_substr`, §9):
   - forward (`end>=start`): `seq = substr(chr, start-1, length)`, `ext_seq = substr(chr, start-3, length+4)`; call `lookup(id, chr, seq, start, end, ext_seq, read)`.
   - reverse: `seq = substr(chr, end-1, length)`, `ext_seq = substr(chr, end-3, length+4)`; call `lookup(id, chr, seq, end, start, ext_seq, read)` (offset=end, the smaller coord). **This genomic-window extraction is the SOLE caller that may pass a negative offset to `perl_substr`** (Review B-A2).
   - **Reverse with `end ∈ {1,2}`:** `end-3` is negative → `perl_substr` reads from the chromosome **end** → degenerate `ext_seq` (1–2 bytes) → every `tri_nt` fails the `len<3` guard → an **all-zero** line (`id chr end start 0 0 0 0`). Forward reads with `start ≤ 3` instead fail the guard → **no line**. Reproduce this asymmetry exactly (both outcomes are named goldens — §12 VS-edge).

**`cytosine_lookup(id, chr, seq, offset, end, ext_seq, read)` (`:242-391`):** walk `seq` as a **plain byte scan (NOT the `regex` crate** — Review B-A-alt2); for each byte at index `i` that is `b'C'` or `b'G'`, set `pos = i + 1` (Perl `pos()` semantics):
- `C` (strand `+`): `tri_nt = perl_substr(ext_seq, pos+1, 3)`; `upstream = perl_substr(ext_seq, pos, 3)`.
- `G` (strand `-`): `tri_nt = perl_substr(ext_seq, pos-1, 3)` then **reverse + complement** (`tr/ACTG/TGAC/` = A↔T, C↔G; identity on all other bytes incl. `N`); `upstream = perl_substr(ext_seq, pos, 3)` then reverse + complement.
- `if tri_nt.len() < 3 { continue }` (`:287`).
- **Context** on `tri_nt` (byte-regex, `:291-303`): `^CG`→`CG`; `^C.G$`→`CHG`; `^C..$`→`CHH`; **else** STDERR warn + skip. A revcomp `tri_nt` that does NOT start with `C` (e.g. an `N` adjacent to a G call) reaches this warn-skip branch (§12 VS-N).
- **Coverage:** genomic position `g = pos + offset - 1`; proceed only if `read` contains key `g` (`:305`).
- **NOMe filter + tally** (keyed on the stored `state` = column-2 `+/-`):
  - `CG`: only if stored call ∈ {`z`,`Z`} **and** `upstream ∈ {ACG, TCG}` → `+`⇒`meth_CG++`, `-`⇒`unmeth_CG++` (`:312-336`). A failing upstream executes `next` (`:329`).
  - `CHG`: only if stored call ∈ {`x`,`X`} **and** `upstream` starts `GC` → `+`⇒`meth_nonCG++`, `-`⇒`unmeth_nonCG++` (`:337-356`).
  - `CHH`: only if stored call ∈ {`h`,`H`} **and** `upstream` starts `GC` → `+`/`-`⇒ `meth/unmeth_nonCG` (`:357-376`).
  - **Structural caution (Review A-I3):** the CG branch does an explicit `next` on upstream-fail, while CHG/CHH just fall through (no `next`). Net behavior is identical (base not tallied, loop continues), but do **not** "simplify" by adding a spurious early-out — mirror the structure.
  - A stored-call/context mismatch is silently disregarded (`:333,353,373`); an `else` context is an unreachable `die` (`:377`); a `state` that is neither `+`/`-` is an unreachable `die`.
- Emit the read's line (§6) with the four accumulated counts.

## 9. `perl_substr` helper (§D3) — exact Perl `substr` rvalue semantics

`fn perl_substr(s: &[u8], offset: isize, len: usize) -> &[u8]` (prefer a borrowed sub-slice; the revcomp path allocates its 3 bytes separately):
- `L = s.len()`. `start = offset >= 0 ? offset : L + offset`.
- If `start < 0` **or** `start > L` → Perl warns + returns undef → treat as **empty** (`b""`), so downstream `len<3` skips (matches Perl `length(undef)==0`).
- **`start == L` → empty slice `b""`** (Perl returns `""`, defined, not undef) — ensure the Rust slice yields `&[]`, **no out-of-range panic** (Review A-I1 / B-A1). This is the exact boundary the reverse-edge degenerate path lands on.
- Else take `min(len, L - start)` bytes from `start`.
- (Perl's negative-`LEN` form is not used by this tool; assert/ignore.)
Covered by adversarial unit tests (negative offset in-range → tail bytes; `|offset|>L` → empty; `start==L` → empty; over-length → truncate).

## 10. Rust structural design

- **Crate** `rust/bismark-nome-filtering` (lib+bin), added to `rust/Cargo.toml` `members` (becomes the 8th). Bin `NOMe_filtering_rs`. `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]`. Mirrors `bismark-dedup`'s `lib.rs`/`main.rs` split.
- **CLI:** clap-derive `Cli` → `validate() -> Result<ResolvedConfig, _>` (the dedup/c2c pattern): live flags resolved, inert flags accepted, the `--merge_CpGs`+`-CX` die, mandatory-genome + infile-exists checks, the `--dir`-relative input/output path resolution (§4, join-not-chdir). `disable_version_flag = true` + `version_string()` = `NOMe_filtering_rs <semver> (<os>/<arch>)` via `env!("CARGO_PKG_VERSION")`. Crate version starts `0.1.0-beta.1`.
- **Errors:** `thiserror` `BismarkNomeError` — `Io(#[from])`, `MissingGenomeFolder`, `InfileNotFound`, `EmptyInput`, `MergeCpgsWithCx`, `Genome(#[from] bismark_io::genome::GenomeError)` (§D5). `EmptyInput` is raised **after** the header is written (D4).
- **Modules:** `cli.rs`, `error.rs`, `filename.rs` (the `.manOwar` derivation — **single-strip-per-extension**, not dedup's loop), `nome.rs` (grouping + flush + `cytosine_lookup` + `perl_substr`), `lib.rs` (`run(&ResolvedConfig)` + `version_string()`), `main.rs` (`parse → version? → run → ExitCode`).
- **`bismark_io::genome`** (new, additive, no version bump): tier-parameterized `load`, module-local `GenomeError`, no public insertion-order iterator. Phase-A `cargo build --workspace` confirms the `=beta.8` pins still resolve.
- **I/O:** `flate2::read::MultiGzDecoder` for gz input; `GzEncoder<BufWriter<File>>` + `Compression::default()` for the always-gzipped output. Per-read map: `HashMap<u32,(u8 state, u8 call)>` (or `FxHashMap`); unconditional insert (last-wins, §5). Counts `u32`. The C/G walk is a byte scan, not `regex`.

## 11. Phases

| Phase | Scope | Gate |
|-------|-------|------|
| **A** — scaffold + genome | Worktree (done); crate (lib+bin) added to workspace members; clap `Cli`/`validate` (all §4 rules incl. the one die, inert acceptance, `--parent_dir` inert, `--dir` path contract); `--help`/`--version`; **promote `bismark_io::genome`** (tier-parameterized, additive/no-bump, module-local `GenomeError`) + consume with `[".fa",".fasta"]`; `BismarkNomeError`; `filename.rs` (single-strip). Workspace builds with all siblings still pinned `beta.8`; genome loads. | unit tests green (incl. filename `x.gz.gz`/`x.txt.txt`, glob `.fa`-beats-`.fasta`, dup-name, bare-header divergence); `--help`/`--version`; clippy clean |
| **B** — core + output | The §8 heart: yacht parse + `^Bismark` skip + gz input; consecutive-ReadID grouping + flush (shared routine, flush-only; seed in loop body); same-position last-wins; suitability guard; `perl_substr` (§9); fwd/rev `tri_nt`+`upstream`; revcomp; CG/CHG/CHH; NOMe ACG/TCG + GpC filters; per-read tally; **header-before-loop** + always-gzipped `.manOwar.txt.gz` output; **empty-input replicate** (header-only `.gz` + non-zero exit, D4). | **Synthetic Perl-v0.25.1 goldens byte-identical** (decompressed compare) across the §12 matrix |
| **C** — real-data gate | Driver script + RELEASE checklist; run Rust vs Perl v0.25.1 on real `--yacht` output (Perl extractor `--yacht`, single-end) + a real genome; assert byte-identity (decompressed, emission order, un-sorted). Gates the `bismark-nome-filtering-v1.0` tag. | raw-byte-identity on real data |

## 12. Test surface

**Unit (synthetic):** `perl_substr` (negative-in-range, `|offset|>L`→empty, **`start==L`→empty/no-panic**, over-length truncate); revcomp `tr/ACTG/TGAC/` (ACGT mapped, `N`/other pass through); context classification incl. `N`-containing (`CNG`→CHG, `CNN`→CHH) + unclassifiable; `pos`→index mapping; fwd-C/rev-G `tri_nt`+`upstream` at interior/chr-start/chr-end; NOMe filters (ACG/TCG accept, GCG/CCG reject for CG; GpC gate for CHG/CHH); tally by column-2 `+/-`; **same-position last-wins** (hard assert: `+Z` then `-Z` @same pos → unmeth); filename derivation incl. **`x.gz.gz`→`x.gz.manOwar.txt.gz`** and **`x.txt.txt`→`x.txt.manOwar.txt.gz`** (single-strip); CLI validation (the die, mandatory-genome, missing/nonexistent infile); empty-input → header written, then `EmptyInput`.

**Integration goldens (committed, Perl v0.25.1):** a tiny multi-FASTA genome (CpG at start, CpG at end, a short scaffold, an `N` run) + a hand-built `--yacht` input (`.txt` and `.txt.gz`) covering:
- **VS-edge (named, independent assertions):** a forward read with `start ≤ 3` (expect **no line**) AND a reverse read with `end ∈ {1,2}` (expect the **all-zero line** `id chr end start 0 0 0 0`).
- **VS-dir:** invoke the binary the way the extractor does — bare-filename input living *inside* `--dir` — and confirm the port reads it from there and writes the output there. (Guards the §4 path contract.)
- **VS-empty:** empty input AND all-`^Bismark` input → assert the on-disk artifact is the header-only `.gz` (decompresses to exactly the header line) and the exit code is non-zero (D4).
- **VS-N:** a read overlapping the `N` run that reaches BOTH `CNG`/`CNN` classification AND the revcomp-`tri_nt`-not-starting-with-`C` warn-skip branch.
- **VS-guard:** reads sized to hit `chr_len == last_start-2+length+4` exactly (suitable) and one less (not).
- **VS-pad:** a CpG as the literal last base of a forward read (upper-pad boundary).
- **VS-crlf:** a CRLF yacht input (pin the Rust trim choice; harmless since the tally keys on col-2, but confirm cols 2-7 aren't mangled).
- Plus: ACG/TCG-pass and GCG-reject CpGs; GpC hits in CHG/CHH; the `^Bismark` header line; unknown-chromosome read (skip); non-consecutive same-ReadID (two reads); the gz round-trip.

Diff the decompressed output against the committed golden (raw `assert_eq!` on bytes, the c2c `golden_phase_*` harness via `assert_cmd` + `MultiGzDecoder`). Goldens generated once from the repo's macOS-runnable Perl `./NOMe_filtering` via a checked-in `generate_goldens.sh`.

**Real-data gate (Phase C):** mirror c2c's Phase-E / RELEASE_CHECKLIST pattern on **oxy** (Felix directive 2026-05-31; oxy is the c2c Phase-E host — NOMe output is per-read and small, so oxy's ~99 GB home cap is a non-issue and a plain `cmp` works, no stream-compare needed; re-verify oxy access/env/paths first session). Generate real `--yacht` output with Perl `bismark_methylation_extractor --yacht` (single-end) on the benchmark data; run Perl `NOMe_filtering` and Rust `NOMe_filtering_rs` against it + the benchmark genome; `cmp` the decompressed outputs **in emission order (no sort)**; `LC_ALL=C` only if some *upstream* step needs deterministic ordering (the NOMe output itself is never sorted); distinct out-dir; purge-on-pass.

## 13. Pitfalls catalog

| # | Pitfall | Perl src | Prevention |
|---|---------|----------|------------|
| P1 | Reverse `end∈{1,2}` negative `substr` → all-zero line; forward `start≤3` → no line | `:132-156` | `perl_substr` (§9) + both **named** edge goldens (§12 VS-edge) |
| P2 | Guard uses `last_start` for both strands (not min) | `:132` | Port the guard verbatim; do **not** "fix" to use the smaller coord |
| P3 | `tr/ACTG/TGAC/` complementing `N`/other bytes | `:276,281` | 4-byte map (A↔T, C↔G), identity elsewhere |
| P4 | `pos()` off-by-one | `:262` | `pos = i+1`; substr offsets `pos±k` per §8 |
| P5 | Tally by column-5 case instead of column-2 `+/-` | `:317-320` | Key tally strictly on stored `state` (col 2) |
| P6 | Wrong glob suffixes (using c2c's 4 incl. `.gz`) | `:522-527` | NOMe = two **plain** tiers `[".fa",".fasta"]` |
| P7 | Bumping `bismark-io` version → breaks sibling `=beta.8` pins | n/a (new) | Additive module + module-local `GenomeError`, **no version bump**; Phase-A `cargo build --workspace` check |
| P8 | gzip-byte compare (container-dependent) | `:77` | Decompress-then-compare in goldens + gate |
| P9 | Output Start/End not ascending for reverse | `:155,217,389` | `offset/end = min/max(start,end)` |
| P10 | Re-emitting non-consecutive same-ReadID as one read | `:105-168` | Group by **consecutive** ReadID; flush on change + EOF |
| P11 | Empty input → clean error, no file (would diverge) | `:74-78,173-175` | **D4 replicate:** write header before read loop, THEN `EmptyInput` + non-zero exit (VS-empty golden) |
| P12 | Resolving the INPUT relative to original CWD, not `--dir` | `:58-77` | `output_dir.join(infile)` read + `output_dir.join(derived)` write, no real `chdir` (VS-dir golden) |
| P13 | `entry().or_insert()` flips same-position to first-wins | `:107-108,166-167` | Unconditional `insert` in input order (last-wins); hard-assert test |
| P14 | `.fa.gz`-only genome invisible → "no FASTA files" error | `:522-529` | Documented footgun (§7); Perl-faithful by design |
| P15 | Reusing dedup's multi-strip filename loop strips both `.gz` | `:464-468` | Single-strip-per-extension; `x.gz.gz`/`x.txt.txt` unit tests |
| P16 | Widening public `BismarkIoError` → sibling non-exhaustive `match` breaks | n/a (new) | Module-local `GenomeError` (§D5), not `BismarkIoError` variants |
| P17 | Shared flush routine also seeds next read → EOF flush diverges | `:160-167,177-219` | Flush routine flushes ONLY; seeding in the loop body (§5) |

## 14. Open questions (remaining)

| Priority | Question | Default |
|----------|----------|---------|
| Resolved | Empty-input behavior | **Replicate** Perl (header-then-error, header-only `.gz`) — Felix, 2026-05-31 (D4). |
| Resolved | Genome error ownership | **Module-local `GenomeError`** (D5). |
| Low | Crate semver start | `0.1.0-beta.1` (sibling cadence). |
| Resolved | Real-data gate host | **oxy** (Felix, 2026-05-31) — the c2c Phase-E host; NOMe's small per-read output makes oxy's ~99 GB home cap irrelevant (plain `cmp`, no stream-compare). |
| Open | Optional later c2c migration onto `bismark_io::genome` | Out of scope here; coordinate with the c2c session. |

## 15. References

- **Perl source:** `./NOMe_filtering` (v0.25.1, 660 LOC) at the Bismark repo root (macOS-runnable).
- **SPEC house style + byte-identity discipline:** `plans/05292026_bismark-coverage2cytosine/SPEC.md`.
- **Rust scaffold:** `rust/bismark-dedup/src/{lib,cli,main,error,filename}.rs` (lib+bin + clap `Cli`→`validate()` + `thiserror`).
- **Genome reader to promote:** `rust/bismark-coverage2cytosine/src/genome.rs` (the four-tier twin; NOMe needs the two-plain-tier variant + module-local error).
- **gzip / golden harness:** c2c `src/report.rs` (`GzEncoder`/`Compression::default()`), `src/cov.rs`/`genome.rs` (`MultiGzDecoder`), `tests/golden_phase_*.rs` (`assert_cmd` + decompress-then-`assert_eq!`).
- **Input format:** `--yacht` in Perl `bismark_methylation_extractor` (single-end; `any_C_context` file; no header) — also implemented in `rust/bismark-extractor`.
- **Reviews:** `PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md` (both APPROVE; arithmetic verified vs live Perl; folded into this rev 1).
- **Memory:** `project_nome_filtering_port`, `project_coverage2cytosine_port`, `reference_colossal_access`, `feedback_sandbox_credentialed_tools`, `feedback_dual_driver_back_port`.

## 16. Revision history

- **rev 0** (2026-05-31): initial draft, written at planning kickoff. Source surveyed in full (660 LOC); sibling scaffolds + c2c SPEC surveyed. Three kickoff decisions resolved with Felix (D1/D2/D3). Worktree `../Bismark-nome` (`rust/nome-filtering`) created off `origin/rust/iron-chancellor` @ `2b05ec8`; `bismark-io` confirmed `1.0.0-beta.8` with no pre-existing `genome` module.
- **rev 3** (2026-05-31): Phases A+B **committed** to `rust/nome-filtering` (`6ab03f7` feat + `1d05cf4` docs). **Phase C host retargeted colossal → oxy** (Felix directive) — oxy is the c2c Phase-E host and NOMe's small per-read output sidesteps oxy's disk cap; `IMPL_phase-C.md` written (driver `nome_gate.sh` + RELEASE checklist). No code change.
- **rev 2** (2026-05-31): **Phases A & B implemented, dual-code-reviewed (APPROVE, no Critical/High), plan-manager COMPLETE.** Phase A: `bismark_io::genome` promotion (additive `flate2`, no version bump, module-local `GenomeError`) + crate scaffold/CLI/`filename`/`perl_substr`/errors. Phase B: `src/nome.rs` (revcomp, classify, cytosine_lookup, process_read, per_read_filtering, write_report) + `run()` restructure (header-before-loop D4); output **byte-identical to Perl** (Perl-golden fixtures under `tests/data/phase_b/` + direct `cmp`). Folded code-review hardening: committed reverse-strand-counting + multi-chromosome-ordering goldens; `GzEncoder<BufWriter<File>>`; the **Accepted-divergences** list above (malformed-line skip + grouping side-effect, coord reformatting, multi-char field, non-UTF-8 → exit 1, exit-code 1 vs 255, `else→warn-skip` unreachable). §3 D4 byte-count corrected (decompressed header = 57 bytes). Phase C (real-data gate) remains.
- **rev 1** (2026-05-31): **dual plan-review folded.** Both reviewers (A via `perl -e` micro-experiments, B by running `./NOMe_filtering` against synthetic fixtures) **independently verified the entire §8/§9 byte-identity arithmetic against live Perl v0.25.1 — zero correctness defects.** Folded: **D4** (empty-input replicate — Felix decision); **D5** (module-local `GenomeError`); the **`--dir` input/output path contract** (§4, A-C2); `--parent_dir` reclassified inert (A-I6); same-position last-wins + flush-vs-seed separation (B-L2/L3); single-strip filename + don't-reuse-dedup-loop (A-I4); byte-scan-not-regex (B-A-alt2); `perl_substr` `start==L`→empty/no-panic (A-I1/B-A1); `.fa.gz` footgun (P14), bare-header divergence inheritance, length(undef) wording fix; expanded §12 (VS-edge named, VS-dir, VS-empty, VS-N, VS-guard, VS-pad, VS-crlf) and §13 (P11–P17). Awaiting Felix's explicit "implement" trigger.
