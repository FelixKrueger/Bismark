# PLAN — `bismark-genome-preparation` (phased implementation)

**Status:** REVISED rev 1 (2026-05-30) — manual review ✅, **dual `plan-reviewer` findings folded in** (`PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md`). **Awaiting implementation trigger. Do not implement** until an explicit trigger (`implement` / `/code-implementation`).
**Rev 1 changes (review fold-in):** corrected chromosome-name extraction (bare `>` is **not** an error; leading-whitespace → **empty** name; split that keeps the leading empty field, **not** `split_whitespace()`; only first-byte-not-`>` errors) — A3 + the inverted test; combined-genome byte-oracle built **from the converted stream**, mode-independent (D1); **sort by `file_name()` bytes** (A3); **`--path_to_aligner` validated early in Step I**, no `which`-fallback when explicit (A2/A4/A7); **always emit `--threads N`** + adopt `BISMARK_BIN → which → current_exe` discovery (A7); **Perl is the primary test oracle from Phase A** (A9); zero-sequence-record + empty-file + CR-only + non-ASCII + mixed-case-glob fixtures pulled into Phase A (A3/A5).
**Companion:** `SPEC.md` (same dir, rev 2). **Workflow:** plan → manual review → dual `plan-reviewer` → implement on trigger → dual `code-reviewer` + `plan-manager` coverage audit → real-data gate → docs → PR.

This is a **design plan** (phased, module-level). Exact per-file RED-GREEN-REFACTOR task lists are produced by `implementation-planner` **after** the trigger (mirrors the methcons convention). **Public-artifact rule:** no external-tool names anywhere in this plan / code / docs / issues (design rationale lives in internal memory).

---

## 0. Coordination / prerequisites

- **Epic:** **[#912](https://github.com/FelixKrueger/Bismark/issues/912) `epic(genomeprep): port bismark_genome_preparation to Rust`** (created 2026-05-30; labels `enhancement`/`rust-rewrite`/`epic`; body archived at `EPIC.md`). Real linked sub-issues: **#905** spec · **#906** Phase A · **#907** Phase B · **#908** Phase C · **#909** Phase D · **#910** test (byte-identity gate) · **#911** docs. The implementing PR will `Closes` them; epic → closed(completed)+board Done on merge. Two forward-looking follow-up issues still to file: *rammap-at-aligner* and *`--genomic_composition`*. *(Board hygiene: the auto-add workflow adds `rust-rewrite` issues to Project #1; the 7 sub-issue items get `gh project item-delete`d to keep them link-only — epic #912 is the board card.)*
- **Worktree** `~/Github/Bismark-genomeprep` on `rust/genome-preparation` (off `rust/iron-chancellor` @ `3715703`) is set up. All work happens here. Never `git checkout`/`switch` in the shared `~/Github/Bismark`.
- **Workspace member to add:** `rust/Cargo.toml` `members = [..., "bismark-genome-preparation"]` (Phase A).
- **Prior art:** alanhoyle's `bismark-genome-prep` (`~/Github/alanhoyle-bismark-rustport`) — structural reference only; **do not copy** its three byte-divergences (CRLF stripping, trailing-newline addition, slam suffix) — SPEC §3.1.
- **Progress tracking:** a `PROGRESS.md` / board card will be created with the epic (mirrors the methcons convention of EPIC.md + PLAN.md rather than a standalone PROGRESS.md).

---

## Plan coverage checklist (every SPEC item → phase/task)

| # | SPEC item | SPEC § | Covered by |
|---|---|---|---|
| 1 | Mandatory genome-folder arg; absolutize; missing → error | §2.1, §6.1 | A2 |
| 2 | FASTA extension precedence (`.fa`→`.fa.gz`→`.fasta`→`.fasta.gz`, first non-empty wins) | §2.1 | A3 |
| 3 | Lexical glob ordering (MFA order + indexer file_list) | §2.1, §8.1 | A3 |
| 4 | gzip input via `MultiGzDecoder` | §3, §8.11 | A5 |
| 5 | `Bisulfite_Genome/{CT,GA}_conversion/` tree + overwrite-warn | §2.4 | A4 |
| 6 | MFA outputs `genome_mfa.{CT,GA}_conversion.fa` | §2.5, §5.2 | A5 |
| 7 | Header rewrite `>{chr}_{CT,GA}_converted\n` (LF; description dropped); **name extraction = exact Perl: bare `>`→empty, leading-ws→empty, split keeps leading empty field (not `split_whitespace`)** | §2.5, §8.7 | A3, A5 |
| 8 | Byte transform `uc → [^ATCGN\n\r]→N → tr/C/T/ (G/A)`, line-ending preserved | §2.5, §5.2, §8.14 | A5 |
| 9 | CRLF preserved in seq lines; whitespace→N; final-no-newline preserved | §8.3/§8.4/§8.6 | A5, C2 |
| 10 | Duplicate chromosome name → error (across all files) | §2.5, §6.1 | A3 |
| 11 | bowtie2-build indexer (default) + concurrency | §2.6, §4.5 | A7 |
| 12 | Indexer discovery `BISMARK_BIN→which→current_exe`; `--path_to_aligner` validated **early (Step I)**, no `which`-fallback when explicit | §2.6, §4.7 | A4, A7 |
| 13 | `--parallel` (≥2); **always emit `--threads N` (N=1 default, Perl-faithful)**; `--large-index` passthrough | §2.2, §2.6 | A2, A7 |
| 14 | `--single_fasta` per-chromosome outputs | §2.5, §5.2 | B1 |
| 15 | `--hisat2` indexer | §2.2, §2.6 | B2 |
| 16 | `--minimap2`/`--mm2` (`-k 20`, `.mmi`) + exclusions | §2.2, §2.6, §6.1 | B3 |
| 17 | Aligner mutual-exclusion + minimap2-incompat validation | §2.2, §6.1 | A2 (rules), B3 (mm2) |
| 18 | `--slam` (T→C/A→G) with **fixed** `_CT_/_GA_` headers + deprecation warning | §2.5, §6.3, §8.13 | C1 |
| 19 | `--genomic_composition` accepted-and-ignored (note; no silent gap) | §9 | C3 |
| 20 | `--verbose` diagnostics; STDERR not byte-matched; version banner constant | §4.2, §4.9, §6.2 | A6, C3 |
| 21 | `--help`/`--man`/`--version` (clap; not byte-gated) | §4.3 | A2 |
| 22 | First byte **not** `>` → error; **bare `>` is NOT an error**; empty dir → error | §8.9, §6.1 | A3, C2 |
| 23 | `--combined_genome` extension (combined FASTA + combined index; opt-in) | §10 | D1, D2 |
| 24 | Combined output independent of `--single_fasta` (**oracle built from converted stream**); composes w/ slam; auto-large-index | §10.3, §10.4 | D1, D2 |
| 25 | Byte-identity gate (CT/GA FASTA) + secondary index-build check | §7 | A9, E1 |
| 26 | Real-data byte-identity on oxy | §7 | E1, E2 |
| 27 | **(rev-1)** Zero-sequence record + 0-byte file + CR-only line endings + non-ASCII→N fixtures | §8.9, §8.15, §8.16 | A5, A9 |
| 28 | **(rev-1)** Glob sort on `file_name()` bytes (+ mixed-case/digit fixture) | §8.1 | A3 |
| 29 | **(rev-1)** Perl script = primary test oracle from Phase A (auto-skip if absent) | §7 | A9 |

*(Every row has a phase/task. If implementation reveals an uncovered item, add a task before closing the phase.)*

---

## Module layout (mirrors dedup/methcons conventions)

```
rust/bismark-genome-preparation/
├── Cargo.toml                 # crate bismark-genome-preparation; [[bin]] bismark_genome_preparation_rs
└── src/
    ├── main.rs                # ExitCode (0 ok / 1 our error / clap 2); --version short-circuit; calls run()
    ├── lib.rs                 # module decls + version_string() (CARGO_PKG_VERSION) + pub fn run(cli)
    ├── error.rs               # thiserror enum (I/O, no-FASTA, dup-chr, bad-header, indexer-fail, validation)
    ├── cli.rs                 # clap-derive Cli + validate() -> ResolvedConfig
    ├── logging.rs             # Logger (mirror extractor); --verbose gating
    ├── discovery.rs           # FASTA glob + extension precedence + lexical sort; extract_chromosome_name; uniqueness
    ├── convert.rs             # core byte transform + per-file FASTA streaming (MFA + single_fasta); header rewrite
    ├── folders.rs             # Bisulfite_Genome/{CT,GA}_conversion/ tree (+ overwrite warn)
    ├── indexer.rs             # which-discovery + --path_to_aligner; command build (bt2/hisat2/mm2); concurrent CT/GA run
    ├── combined.rs            # --combined_genome: concat CT+GA → combined FASTA + build combined index
    └── pipeline.rs            # run(): Step I folders → Step II convert → Step III index → [opt combined]
```

---

## Phase A — Crate scaffold + CLI + core CT/GA conversion + MFA + bowtie2 (MVP)

**Goal:** a working default-mode tool — read a genome dir (`.fa`/`.gz` glob, MFA mode), write byte-identical C→T and G→A `genome_mfa.*` files under `Bisulfite_Genome/{CT,GA}_conversion/`, and launch concurrent `bowtie2-build`. This delivers the **entire byte-identity gate for the common case**; later phases add modes/indexers/edge polish.

### A1. Scaffold
- `Cargo.toml`: package `bismark-genome-preparation`; `[[bin]] name = "bismark_genome_preparation_rs"`. Deps: `clap` (=4.5.30, derive), `flate2` (=1.1.9), `which` (=7.0.3), `thiserror` (=2.0), `anyhow` (=1.0). dev-deps: `assert_cmd`, `predicates`, `tempfile`. **No `bismark-io`** (no BAM). Add member to `rust/Cargo.toml`.
- `lib.rs` — module decls + `version_string()` (CARGO_PKG_VERSION; dedup pattern) + `pub fn run(cli) -> Result<(), GenomePrepError>`.
- `main.rs` — `ExitCode` (dedup pattern): `--version` short-circuit; `Ok→SUCCESS`, our `Err→from(1)`, clap→2.
- `error.rs` — `thiserror` enum: `Io`, `NoFasta`, `DuplicateChromosome(String)`, `BadHeader(PathBuf)`, `IndexerLaunch{tool,source}`, `IndexerFailed{tool}`, `Validation(String)`.

### A2. CLI (`cli.rs`) — SPEC §6
- clap-derive `Cli`: positional `genome_folder: PathBuf`; flags with **underscore** long names (`--single_fasta`, `--path_to_aligner`, `--large-index`, `--genomic_composition`, `--combined_genome`); `--minimap2` with `alias="mm2"`; `disable_version_flag = true` (manual `--version`); `--man` aliases `--help`.
- `validate() -> ResolvedConfig { genome_folder(absolutized), aligner: Aligner{Bowtie2|Hisat2|Minimap2}, path_to_aligner, parallel: Option<u32>, single_fasta, slam, large_index, genomic_composition, combined_genome, verbose }`.
- **Validation (SPEC §6.1):** aligner mutual-exclusion (>1 selected → error); minimap2 + (`single_fasta`|`slam`|`large-index`) → error; `--parallel` given but `<2` → error; absolutize genome folder (error if it doesn't exist); empty-dir / no-FASTA error is raised in A3 (needs the glob).
- **(rev-1) `--path_to_aligner` is validated EARLY — in Step I (A4), before any conversion** (Perl L589). A bad explicit path must fail before FASTA is written. When given, the indexer binary is resolved as exactly `{path}/{binary}` with **no `which`-fallback** (SPEC §2.6/§4.7).
- Unit tests: each conflict → error; default = bowtie2; `--mm2` alias; `--parallel 1` → error; underscore long-names parse.

### A3. Discovery (`discovery.rs`) — **pure, exhaustively unit-tested**
- `find_fasta_files(dir) -> Result<Vec<PathBuf>>`: try `.fa` (excluding `.fa.gz`), else `.fa.gz`, else `.fasta` (excluding `.fasta.gz`), else `.fasta.gz`; **first non-empty group wins**; **sort on the `file_name()` bytes (C-locale / bytewise), NOT the full `PathBuf`** (rev-1, SPEC §8.1); empty → `NoFasta` error.
- `extract_chromosome_name(header: &str) -> Result<&str>` — **EXACT Perl semantics (rev-1, both reviewers; SPEC §2.5/§8.7):**
  - Error **only** if the first byte is **not** `>`.
  - A **bare `>`** → name `""` (NOT an error) → Perl writes `>_CT_converted`.
  - Strip the leading `>`, then take the first field of a split on Perl's `\s` set **that keeps a leading empty field** — i.e. `s.split(|c| matches!(c, ' '|'\t'|'\n'|'\r'|'\x0c')).next().unwrap_or("")`. **Do NOT use `str::split_whitespace()`** (it skips leading whitespace → diverges on `>  chr1`). So `>  chr1 desc` → `""`, `>chr1 desc` → `chr1`.
- Uniqueness is enforced in `convert.rs` via a `HashSet` (spans all files); helper `check_unique(&mut HashSet, name) -> Result<()>`.
- **Unit tests:** extension precedence (mixed dir picks `.fa` only; `.fa.gz`-only dir; `.fasta` fallback); **lexical order `chr1, chr10, chr11, chr2` (NOT numeric)** + a **mixed-case + digit** fixture (rev-1) — the load-bearing glob-order tests (SPEC §8.1); name extraction (`>chr1 desc`→`chr1`; CRLF `>chr1\r`→`chr1`; **`>` bare → `""` (NOT error)**; **`>  chr1` → `""` (leading-whitespace empty name)**; a first line **not** starting with `>` → error); duplicate detection. *(All oracled against the Perl script where present — A9.)*

### A4. Folders + early aligner-path validation (`folders.rs` / Step I)
- `create_tree(genome_folder) -> Result<(ct_dir, ga_dir)>`: create `Bisulfite_Genome/`; if it exists, **warn + overwrite** (no error; SPEC §2.4); create `CT_conversion/` + `GA_conversion/` (guarded).
- **(rev-1) Validate `--path_to_aligner` HERE (Step I), before conversion** — resolve `{path}/{binary}` and confirm it exists/is executable; error out now (Perl L589) so a bad path never leaves a converted-but-unindexed genome.
- Unit test: fresh dir creates tree; pre-existing `Bisulfite_Genome/` → warns, still returns the dirs; bad `--path_to_aligner` → error **before** any conversion output appears.

### A5. Conversion core (`convert.rs`) — **the byte-identity heart; pure transform exhaustively unit-tested**
- `fn transform_seq_line(raw: &[u8], target: Target) -> Vec<u8>` where `Target ∈ {Ct, Ga}` and slam-ness is carried in `Target` (or a `Mode`): for each byte `b`: `u = b.to_ascii_uppercase(); keep if u ∈ {A,T,C,G,N,\r,\n} else N; then C→T (Ct) / G→A (Ga)` — operate on **raw bytes including the terminator** so CRLF/`\r` and final-no-newline survive. (SPEC §5.2, §8.14 — **NOT** trim-and-re-emit.)
- `fn converted_header(chr: &str, target: Target) -> Vec<u8>` → `>{chr}_CT_converted\n` / `>{chr}_GA_converted\n` — **fixed suffix even in slam** (SPEC §2.5, §8.13).
- `open_fasta(path) -> Box<dyn BufRead>`: `.gz` → `MultiGzDecoder` (SPEC §8.11), else plain.
- `convert_all(files, ct_writer, ga_writer, mode, single_fasta, ...)`: stream each file line-by-line via `read_until(b'\n')`; first line → header (extract name, uniqueness, write converted headers); subsequent `>`-lines → new headers; else → `transform_seq_line` to both writers. MFA = two persistent writers; `--single_fasta` = per-chr writers (Phase B1 toggles writer construction).
- **Unit tests (pure transform):** `acgt`→uc; ambiguity `RYSWKMBDHV`→N; **non-ASCII / high byte → N** (rev-1, confirmed agrees with Perl); lowercase→upper; **C→T** vs **G→A** outputs; **CRLF line → CRLF preserved**; **final line w/o `\n` → no `\n`**; **interior/trailing whitespace → N**; empty line `\n` passthrough; a known multi-line record → exact expected CT and GA bytes.
- **(rev-1) Record/file-shape fixtures pulled into Phase A:** a **zero-sequence record** (header→header, and header at EOF — emits just the converted header, no sequence; SPEC §8.9); a **0-byte FASTA file**; a **CR-only (old-Mac) file** (whole file read as one line via `read_until(b'\n')` → header only — SPEC §8.15). Oracle each against the Perl script (A9).

### A6. Logging (`logging.rs`) — mirror extractor `Logger`
- `Logger { verbose }` with `note()`/`info()` to STDERR. Step banners (`Step I/II/III`), the `Bisulfite Genome Indexer version v0.25.1 (last modified: 19 May 2022)` line, conversion totals. Not byte-gated (SPEC §4.2).

### A7. Indexer (`indexer.rs`) — bowtie2 path + concurrency
- `resolve_indexer(aligner, path_to_aligner) -> Result<String>`: binary = `bowtie2-build` (A). **(rev-1) Discovery tier = `BISMARK_BIN → which → current_exe`** (adopt the extractor's `subprocess.rs:215` convention for workspace consistency). **When `--path_to_aligner` is given, use exactly `{path}/{binary}` and do NOT `which`-fallback** (validated already in A4/Step I).
- `build_command(dir, basename, aligner, parallel, large_index) -> Command`: bt2 → `bowtie2-build --threads N [--large-index] -f <comma-joined *.fa> BS_CT|BS_GA` (re-glob `*.fa` in the dir; SPEC §2.6). **(rev-1) ALWAYS pass `--threads N`** (N=`parallel.unwrap_or(1)`) — Perl-faithful (it always emits the flag, default 1).
- `run_both(ct_dir, ga_dir, ...)`: spawn CT in a thread + GA on the main thread (mirror Perl fork; SPEC §4.5); join; propagate first failure (`IndexerFailed`).
- Unit tests: command-string construction (**`--threads 1` present by default**; `--large-index` when set); discovery-tier resolution + explicit-path no-fallback; discovery failure → clear error. (Actual index build is exercised in A9/E only when the tool is on PATH.)

### A8. Pipeline (`pipeline.rs`)
- `run(config)`: Step I `create_tree` → Step II open MFA writers + `convert_all` → Step III `run_both` (bowtie2). `--combined_genome` hook is a no-op until Phase D. Logging threaded throughout.

### A9. Phase-A tests (integration, `tests/`)
- **(rev-1) Perl is the PRIMARY oracle** (mirror methcons `perl_vs_rust_*`; auto-skip if `perl` absent): run the actual `bismark_genome_preparation` and `bismark_genome_preparation_rs` on the **same synthetic input** and `diff` the CT/GA FASTA byte-for-byte. This (not hand fixtures) is what catches name-extraction divergences (bare `>`, leading whitespace). Run the oracle over the A3/A5 edge fixtures too (bare-`>`, leading-whitespace header, zero-sequence record, 0-byte file, CR-only, non-ASCII, mixed-case glob order).
- Secondary hand-checked fixtures for the subtle edges (CRLF preserved, final-no-newline) where eyeballing the expected bytes adds confidence.
- Assert: directory tree exists; headers/line-wrapping preserved; if `bowtie2-build` is on PATH, indices build (secondary; skip otherwise).
- CLI validation integration tests (no-FASTA dir → error; conflicting aligners → error; **bad `--path_to_aligner` errors before any output is written**).

**Phase A acceptance:** default-mode CT/GA MFA byte-identical to Perl on synthetic input; all unit tests green; `cargo fmt`/`clippy -D warnings` clean.

---

## Phase B — Output modes + remaining indexers

**Goal:** `--single_fasta` per-chromosome outputs; `--hisat2` and `--minimap2` indexer wiring + their validation.

### B1. `--single_fasta` (`convert.rs` + `pipeline.rs`)
- Per-chromosome writer construction (`<chr>.CT_conversion.fa` / `.GA_conversion.fa`) replacing the two persistent MFA writers; conversion logic unchanged. Tests: per-chr files byte-identical; the **set of files** matches; output independent of indexer.

### B2. `--hisat2` (`indexer.rs`)
- Binary `hisat2-build`; same `-f <files> BS_CT|BS_GA` command shape + `--threads`/`--large-index`. Tests: command construction; (functional build gated on tool availability).

### B3. `--minimap2` (`indexer.rs` + `cli.rs`)
- Binary `minimap2`; command `minimap2 -k 20 [-t N] -d BS_CT.mmi|BS_GA.mmi <files>` (SPEC §2.6). Enforce minimap2 exclusions (already in A2 validation; assert here). Tests: command construction (`-k 20`, `-t`, `.mmi`); exclusion errors (`--minimap2 --single_fasta` etc.).

**Phase B acceptance:** all three indexers produce correct command lines; `--single_fasta` byte-identical; minimap2 exclusions enforced.

---

## Phase C — `--slam` (deprecated), edge cases, accept-and-ignore flags

**Goal:** the deprecated slam mode, the full edge-case sweep, and the accepted-but-ignored `--genomic_composition` + `--verbose`.

### C1. `--slam` (`convert.rs` + `cli.rs`)
- Slam transform: `tr/T/C/` (CT file) and `tr/A/G/` (GA file) instead of C→T/G→A; **headers stay `_CT_converted`/`_GA_converted`** (SPEC §8.13 — the alanhoyle divergence we must not copy). Emit a STDERR **deprecation** warning (SPEC §6.3) + Perl's experimental warning (no `sleep`). Tests: slam CT/GA bytes correct; **assert header suffix is `_CT_`/`_GA_`, NOT `_TC_`/`_AG_`** (pinning the byte-trap).

### C2. Edge-case sweep (integration fixtures)
- CRLF-input genome → headers LF, seq lines keep `\r` (SPEC §8.3). Final line without `\n` → preserved. Interior whitespace → N. Empty/blank lines passthrough. First line not `>` → error. Empty dir / no FASTA → error. Duplicate chromosome name across two input files → error. gzipped `.fa.gz` input → byte-identical to plain. *(Several already unit-tested in A5/A3; this consolidates end-to-end fixtures.)*

### C3. Accept-and-ignore + diagnostics
- `--genomic_composition`: accepted, **ignored with a one-line STDERR note** ("deferred; not produced in this version") — explicit, not a silent gap (SPEC §9). `--verbose`: extra diagnostics via `Logger`. `--version`/`--help`/`--man`: clap, not byte-gated. Tests: `--genomic_composition` runs without producing the file + emits the note; `--version` prints the banner.

**Phase C acceptance:** slam byte-identical with correct headers; every SPEC §8 edge case covered by a fixture; `--genomic_composition` cleanly deferred.

---

## Phase D — `--combined_genome` extension (Bismark-Rust)

**Goal:** the additive, opt-in combined reference + combined index (SPEC §10). **Not byte-gated vs Perl** (no counterpart); structurally validated.

### D1. Combined FASTA (`combined.rs`)
- After Steps II/III, if `config.combined_genome`: create `Bisulfite_Genome/Combined/`; write `genome_mfa.combined.fa` = **CT block ++ GA block** (all `*_CT_converted` then all `*_GA_converted`, glob order). Always a single MFA, **independent of `--single_fasta`**.
- **(rev-1) Byte source = the converted sequence stream, NOT the MFA files** (both reviewers): build the combined output by running the conversion into the combined writer (all CT records, then all GA records). The MFA files **don't exist in `--single_fasta` mode**, so the oracle must not depend on them. Composes with `--slam` (slam-converted seqs; headers still `_CT_`/`_GA_`).
- Test (SPEC §10.4, mode-independent): build an **expected** combined buffer from the same converted stream and assert `genome_mfa.combined.fa` is byte-equal. **MFA mode** additionally checks `== CT MFA ++ GA MFA`; **`--single_fasta` mode** checks against the assembled expected buffer (no MFA files). Include a `--slam` run.

### D2. Combined index (`combined.rs` + `indexer.rs`)
- Build one combined index with the selected indexer (`BS_combined.*` / `BS_combined.mmi`) over `genome_mfa.combined.fa`, reusing `indexer.rs`. Note: `bowtie2-build` auto-promotes to large index for mammalian-size combined refs (no need to force `--large-index`; SPEC §10.3). Concurrency: additional job after/with the standard two.
- Test: combined index builds (gated on tool availability); without `--combined_genome`, the `Combined/` dir is absent (additive — current behavior untouched).

**Phase D acceptance:** combined FASTA == CT++GA bytes; combined index builds; flag is purely additive (off ⇒ identical to Phase A–C output).

---

## Phase E — Real-data byte-identity validation (oxy) + docs/polish

**Goal:** prove byte-identity on a real genome and finish docs/CI hooks.

### E1. Byte-identity harness (`tests/byte_identity_real_data.rs`, `#[ignore]`)
- Env-var-overridable genome dir; skip gracefully if absent. Run Perl `bismark_genome_preparation` and `bismark_genome_preparation_rs` on **copies** of the same genome dir; **`diff` the CT and GA converted FASTA byte-for-byte** (the gate). Secondary: assert each index **builds** (don't diff index bytes). Cover MFA + `--single_fasta` (+ a `--slam` smoke).

### E2. oxy run procedure (during implementation; ask before destructive ops)
- `dcli ssh oxy` (needs `dangerouslyDisableSandbox:true` on macOS — Keychain). **Verify oxy's env on arrival** (distinct host): genome data path, mamba env, `~/.cargo/bin`, `bowtie2-build`/`hisat2-build` availability. Prepend env `bin` to PATH (no `mamba activate`). Timing via bash `time`/`$SECONDS` (not `/usr/bin/env time`). Long index builds in detached tmux + a `~/*.status` marker. Fresh work dir; gate byte-identity on the (fast) conversion, validate the (slow) index build separately.

### E3. Docs & polish
- Crate `README.md` + `CHANGELOG.md` (Keep a Changelog); check/refresh `docs/bismark/genome_preparation.md`. rustdoc on public items. `cargo fmt` / `clippy -D warnings` / full `cargo test`. Flag the CI matrix epic (#796) for a genomeprep diff-vs-Perl job.

**Phase E acceptance:** real-data CT/GA FASTA byte-identical (MFA + single_fasta) for a real genome; indices build; docs done; clean fmt/clippy/test.

---

## Resolved decisions (carried from SPEC §9 + rev-1 review fold-in; no open decisions)
1. **All three indexers external subprocesses** (bowtie2/hisat2/minimap2); rammap deferred to the aligner layer.
2. **`--slam` in v1.0, deprecated**, with fixed `_CT_`/`_GA_` headers (the load-bearing byte-trap).
3. **Indexer builds run concurrently** (mirror Perl fork).
4. **`--genomic_composition` deferred** — accepted-and-ignored with an explicit note.
5. **`--combined_genome` (rev 2):** additive, opt-in; combined FASTA + combined index; alignment-correctness validation deferred to the aligner rewrite.
6. **(rev-1) Chromosome-name extraction = exact Perl semantics:** bare `>` and leading-whitespace headers yield an **empty** name (not an error, not the next token); only a first byte ≠ `>` errors; split keeps the leading empty field (not `split_whitespace()`). (A3 — both reviewers, Critical.)
7. **(rev-1) Indexer threads = always `--threads N` (N=1 default), Perl-faithful**; discovery tier `BISMARK_BIN → which → current_exe`; **no `which`-fallback when `--path_to_aligner` is explicit**, validated early in Step I. (A4/A7.)
8. **(rev-1) Combined-genome byte-oracle built from the converted stream**, mode-independent (works in `--single_fasta`). (D1.)
9. **(rev-1) Perl script is the primary test oracle from Phase A** (hand fixtures secondary). (A9.)

**Residual risks to watch during implementation:** glob-sort parity vs Perl on real genome dirs (A3/E1 — chr1/chr10/chr2 ordering, now sorted on `file_name()` bytes); line-ending fidelity on CRLF + final-no-newline + CR-only (A5/C2); large-genome streaming (never slurp the conversion path); zero-sequence/empty-file shapes (A5).

## Estimated sequencing
A (MVP: core conversion + bowtie2 + MFA — the whole gate for the common case) → B (single_fasta + hisat2/minimap2) → C (slam + edge sweep + accept-ignore) → D (combined-genome extension) → E (real-data gate + docs). Phases share `convert.rs`/`indexer.rs`/`pipeline.rs`, so they are **sequential, not parallel streams** (mirrors methcons — splitting would conflict on shared core modules).

## Notes on skill division
This phased PLAN is the `plan-writer`-level design. Per the methcons convention, the granular per-file RED-GREEN-REFACTOR task list (TDD) is generated by `implementation-planner` **after** the implementation trigger, scoped phase-by-phase.

---

## Implementation notes (2026-05-30)

**Status: Phases A, B, C, D COMPLETE and verified (local, with the Perl script as oracle). Phase E (real-genome run on oxy + docs) PENDING.**

Crate `bismark-genome-preparation` (bin `bismark_genome_preparation_rs`, v1.0.0-alpha.1) added to the workspace. Modules: `error`, `cli`, `logging`, `discovery`, `convert`, `folders`, `indexer`, `combined`, `pipeline`, `lib`, `main`. As anticipated (and as methcons did), the small algorithm meant Phases A–D landed together in one coherent crate rather than as separate code drops; the phase-scoped behaviors are all present and individually tested.

**What was built:**
- **Discovery** (`discovery.rs`): extension precedence (`.fa`→`.fa.gz`→`.fasta`→`.fasta.gz`, first non-empty group wins), sort on **`file_name()` bytes**; `extract_chromosome_name` with **exact Perl semantics** (bare `>` → empty name not error; leading-whitespace → empty name; only first-byte-not-`>` errors; split keeps leading empty field — not `split_whitespace()`).
- **Conversion** (`convert.rs`): raw-byte transform incl. terminator (`uc → [^ATCGN\n\r]→N → tr`), CRLF/final-no-newline preserved; MFA + `--single_fasta`; `--slam` (T→C/A→G) with **fixed `_CT_`/`_GA_` headers**; gzip input via `MultiGzDecoder`; cross-file chromosome uniqueness; `write_combined` (CT block ++ GA block, built from the converted stream — mode-independent).
- **Folders** (`folders.rs`): `Bisulfite_Genome/{CT,GA}_conversion/` tree + overwrite-warn.
- **Indexer** (`indexer.rs`): all three (`bowtie2-build`/`hisat2-build`/`minimap2 -d -k 20`); discovery tier `BISMARK_BIN→which→current_exe`; `--path_to_aligner` no-fallback; **always emits `--threads N`** (N=1 default); concurrent CT/GA builds (thread + main).
- **CLI** (`cli.rs`): all flags with Perl underscore spellings; aligner mutual-exclusion + minimap2 exclusions; `--parallel ≥2`; `--genomic_composition` accepted-and-ignored with a note; `--man` aliases `--help`; manual `--version`.
- **Combined** (`combined.rs`) + **pipeline** (`pipeline.rs`): Step I (discover + early path validation + folders) → II (convert) → III (concurrent index) → IV (opt combined).

**Verification (all green, local):**
- `cargo test -p bismark-genome-preparation` — **27 lib unit tests + 5 integration tests** pass; `clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean; `cargo check --workspace` clean (no sibling breakage).
- **Byte-identity proven against the actual Perl script**: `perl_vs_rust_byte_identical_mfa` and `perl_vs_rust_byte_identical_single_fasta` run the real `bismark_genome_preparation` (auto-skip if `perl` absent) on a representative genome (dropped header description, lowercase, IUPAC ambiguity `RYSWKMBDHV`, multi-record, **multi-file glob order** `chr1/chr10/chr2`, **final-no-newline**) and assert the CT/GA FASTA are **byte-identical** — in both MFA and `--single_fasta` modes.
- Unit tests pin the load-bearing details: the name-extraction edges (bare `>`, leading whitespace), CRLF preservation, final-no-newline, interior-whitespace→N, non-ASCII→N, slam direction + fixed header suffix, glob lexical order, extension precedence, duplicate-name error, empty-file error, and the indexer command strings (`--threads 1` default, `--large-index`, minimap2 `-k 20`/`.mmi`).
- Binary end-to-end exercised with a **fake `bowtie2-build`** (via `BISMARK_BIN`) so the full pipeline (incl. Step III + `--combined_genome`) runs without a real indexer.

**Decisions realised as planned:** raw-byte transform (not trim-and-re-emit); exact Perl name extraction; sort on `file_name()` bytes; `--slam` deprecated + fixed `_CT_`/`_GA_` suffix; always `--threads N`; `BISMARK_BIN→which→current_exe` discovery with no-fallback on explicit path; early path validation; concurrent indexer; `--combined_genome` additive/opt-in built from the converted stream; `--genomic_composition` accepted-and-ignored.

**Deviations from the plan:** none material. Phases A–D implemented together (justified above). `convert::Counts` returns conversion totals (used for the STDOUT totals line — not byte-gated). Crate version started at `1.0.0-alpha.1`.

**Iteration log:**
- `#1` — first compile + lib tests: 27/27 passed on the first build.
- `#2` — full suite incl. Perl oracle: 5/5 integration passed (real Perl byte-match, MFA + single_fasta).
- `#3` — `clippy -D warnings`: clean (no findings). `cargo fmt`: applied cosmetic line-wraps in `tests/integration.rs`; re-ran tests → still 32/32 green.

**Post-audit fixes (2026-05-30) — dual code-review + plan-manager (`CODE_REVIEW_{A,B}.md`, `COVERAGE.md`):** both reviewers 0 Critical; plan-manager verdict COMPLETE for the A–D gate. Applied (user-approved scope: H1 + agreed tests + M1):
- **H1 (High, byte-identity):** Perl's `File::Glob` sorts **case-insensitively**, not bytewise — my SPEC §8.1 premise was wrong. `discovery::fasta_name_cmp` now sorts `(ascii-lowercased, raw bytes)`; `indexer.rs` re-glob aligned to the same comparator. **Verified against real Perl** by the new `perl_vs_rust_mixed_case_glob_order` oracle test (mixed-case filenames now byte-identical). SPEC §8.1 corrected.
- **M1 (Medium):** non-UTF-8 `.fa` file names were silently dropped (`to_str()` filter); `in_group` now matches on `as_encoded_bytes()`, and the glob filter keeps non-UTF-8 names (`glob_includes_non_utf8_name` test, skips on UTF-8-enforcing filesystems like APFS).
- **Agreed test gaps closed:** `cli.rs` (8 validation unit tests), `folders.rs` (2 unit tests), `bad_path_to_aligner_fails_before_conversion` (verifies early-validation: no FASTA written), and Perl-oracle tests for **edge inputs** (CRLF / zero-sequence / CR-only / final-no-newline / ambiguity), **slam**, and **gzip `.fa.gz`** input.
- **Deferred (per user):** the Low findings (`IndexerFailed` spawn-vs-exit detail, `*.fa`-directory handling, `canonicalize` symlink note) and the M3 combined 3×-I/O efficiency item.
- **Result:** **39 lib + 10 integration = 49 tests pass**; `clippy -D warnings` + `cargo fmt --check` clean; `cargo check --workspace` clean.
- Iteration `#4` — `glob_includes_non_utf8_name` failed on macOS APFS (rejects invalid-UTF-8 names — environment limit, not a code bug); made it skip-on-unsupported-FS so it still verifies inclusion on ext4/CI.

**Phase E (pending — needs oxy):** real-genome CT/GA byte-identity at scale (the synthetic + Perl-oracle tests prove the *logic*; the `tests/byte_identity_real_data.rs` `#[ignore]` harness + the oxy run per E1/E2 remain). Also pending: crate `README.md` + `CHANGELOG.md` + mkdocs page (#911), the `--genomic_composition` follow-up, board sub-issue item-delete cleanup once the auto-add workflow fires, and committing the branch + opening the PR (`Closes #905–#911`).
