# Phase A PLAN — Scaffold + CLI + genome reader

**Epic:** `05292026_bismark-coverage2cytosine/EPIC.md`, Phase A — Scaffold + CLI + genome reader
**Design contract:** `../SPEC.md` (rev 1) — §3 (CLI), §6 (genome reading), §10 (structural choices), §11 (data structures).
**Status:** rev 0 — awaiting dual plan-review, then implementation trigger.

## 1. Goal

Stand up the `bismark-coverage2cytosine` crate as a workspace member (`lib` + `bin coverage2cytosine_rs`), with:

- a clap-derived `Cli` and a `ResolvedConfig::validate()` that enforces **every** Perl flag-validation rule (SPEC §3) **and rejects the v1.x flags** (`--gc`/`--nome-seq`/`--drach`/`--ffs`) rather than silently accepting them;
- a `genome.rs` whole-genome FASTA reader reproducing Perl `read_genome_into_memory`'s quirks (uppercase, `Mus_musculus.NCBIM37.fa` skip, four-suffix glob priority, first-token chromosome name, duplicate-name error, gz support, `u32` length-overflow guard);
- a `BismarkC2cError` `thiserror` enum.

**Acceptance:** crate compiles + `cargo clippy`/`fmt` clean; `coverage2cytosine_rs --help` lists the v1.0 flags; `--version` prints a TG-style provenance string; the genome reader loads a multi-FASTA (plain + `.gz`) into memory with correct names/lengths; all validation + genome-reader unit tests pass. **No report algorithm yet** (Phase B).

## 2. Context

- **Where:** new directory `rust/bismark-coverage2cytosine/` in the worktree `/Users/fkrueger/Github/Bismark-c2c`, added to `rust/Cargo.toml` `members`. Branch `rust/coverage2cytosine`.
- **Patterns to mirror** (read these, match house style):
  - `rust/bismark-dedup/src/{lib,main,cli,error}.rs` — the `lib`+`bin` split, `Cli`→`validate()→ResolvedConfig`, `disable_version_flag` + custom `version_string()`, priority-ordered rejections, `thiserror` enum with `#[from] BismarkIoError` / struct variants.
  - `rust/bismark-io/src/cram_ref.rs` — noodles-fasta reading, `Vec<u8>` chromosome names, duplicate-name detection, FASTA-suffix matching. **Divergences to apply** (SPEC §6): uppercase the sequence; `Mus_musculus.NCBIM37.fa` skip; four-suffix *priority* glob (not union); plain-gzip via `flate2`.
- **Dependencies (this phase):** A→ none. B/C/D/E depend on A's `ResolvedConfig`, `Genome`, `BismarkC2cError` (epic sub-plan table).
- **Crates to add** (pin to the workspace's existing transitive versions — verified in `Cargo.lock`):
  - `clap = { version = "=4.5.30", features = ["derive"] }`
  - `thiserror = "=2.0.0"`
  - `noodles-fasta = "=0.61.0"`, `noodles-core = "=0.20.0"`
  - `flate2 = "=1.1.9"` (plain-gzip `.fa.gz`/`.fasta.gz` — Perl uses `gunzip -c`; noodles `build_from_path` only handles BGZF, so we decompress with `flate2::read::MultiGzDecoder` and feed a noodles `fasta::Reader`).
  - dev: `assert_cmd = "=2.0.16"`, `predicates = "=3.1.2"`, `tempfile = "=3.10.1"`, `bstr = "=1.10.0"`.
  - **NOT** in Phase A: `indexmap` (Phase B — covered-chromosome order), `rustc-hash` (Phase B — cov map), `mimalloc` (a v1.x perf phase). Don't pre-add.

## 3. Behavior

### 3.1 CLI surface (clap-derive `Cli`)

Flags (SPEC §3; long names + Perl aliases). All optional except the positional cov infile:

| clap field | flag(s) | type | default |
|------------|---------|------|---------|
| `cov_infile` | positional (1, required) | `PathBuf` | — |
| `output` | `-o`/`--output` | `Option<String>` | — (required at validate) |
| `dir` | `--dir` | `Option<PathBuf>` | `None` ⇒ `output_dir = ""` (empty path **prefix**, Perl :2108-2110) |
| `genome_folder` | `-g`/`--genome_folder` | `Option<PathBuf>` | — (required at validate) |
| `parent_dir` | `--parent_dir` | `Option<PathBuf>` | `None` ⇒ `getcwd()` (Perl :2070-2071) |
| `zero_based` | `--zero_based` | `bool` | false |
| `cx_context` | `--CX_context` + `visible_alias = "CX"` | `bool` | false |
| `split_by_chromosome` | `--split_by_chromosome` | `bool` | false |
| `merge_cpgs` | `--merge_CpGs` | `bool` | false |
| `discordance` | `--discordance_filter` | `Option<u8>` | `None` |
| `threshold` | `--coverage_threshold` (alias `--threshold`) | `Option<u32>` | `None` (⇒ 0) |
| `gzip` | `--gzip` | `bool` | false |
| `version` | `-V`/`--version` | `bool` | false (clap auto-version disabled) |
| **v1.x rejects** | `--gc`/`--GC_context`, `--nome-seq`, `--drach`/`--m6A`, `--ffs` | `bool` | false (each rejected at validate) |

`#[command(disable_version_flag = true)]`; `version` handled in `main` (prints `version_string()`). The four v1.x flags ARE declared (so they parse) but `validate()` rejects them with `UnsupportedFlag`.

**`--CX` flag surface (rev 1 fix, reviewers A+B):** do NOT model `-CX` as a clap short — clap would parse `-CX` as bundled single-char shorts `-C -X`. Use `#[arg(long = "CX_context", visible_alias = "CX")]`, which accepts both `--CX_context` and `--CX` (Perl's `GetOptions "CX|CX_context"`). There is no single-dash `-CX` in Perl either (GetOptions treats `CX` as a long token), so no parity is lost.

### 3.2 `Cli::validate()` → `ResolvedConfig` (rejections in priority order)

Mirror Perl `process_commandline` (citations in SPEC §3):

1. `version` true → caller (`main`) short-circuits before `validate` (like dedup).
2. Any v1.x flag set → `UnsupportedFlag { flag }` (e.g. `"--nome-seq is not supported in the Rust port (v1.x); use Perl coverage2cytosine"`). Check `--gc`, `--nome-seq`, `--drach`, `--ffs` in that order.
3. `output` is `None` → `MissingOutput` (Perl :2078).
4. `genome_folder` is `None` → `MissingGenomeFolder` (Perl :2134). The Perl hardcoded-mouse default is **not** honoured (SPEC §15).
5. `merge_cpgs && cx_context` → `MergeCpgsWithCx` (Perl :2140).
6. `merge_cpgs && split_by_chromosome` → `MergeCpgsWithSplit` (Perl :2143).
7. `merge_cpgs && threshold.is_some()` → `MergeCpgsWithThreshold` (Perl :2176).
8. `discordance.is_some() && !merge_cpgs` → `DiscordanceWithoutMerge` (Perl :2165).
9. `discordance` value not in `1..=100` → `DiscordanceOutOfRange { value }` (Perl :2168).
10. `threshold == Some(0)` → `ThresholdNotPositive` (Perl :2178 — explicit 0 is rejected; *absence* ⇒ 0 default which means "report all"). _Note: `Some(0)` (user typed `--coverage_threshold 0`) is an error; `None` resolves to `threshold = 0` meaning report-all._
11. Resolve (rev 1 — two reviewer-Critical fixes folded):
    - `cpg_only = !cx_context` (Perl :2112-2115); `threshold_resolved = threshold.unwrap_or(0)`.
    - **`output_stem` strip is CONTEXT-CONDITIONAL** (C1, reviewers A+B vs Perl `handle_filehandles:107-112`): strip **exactly one** suffix — `.CX_report.txt` **iff `cx_context`**, else `.CpG_report.txt`. Do **NOT** strip both. Consequence (byte-identity trap): `-o foo.CX_report.txt` in *default* (CpG) mode keeps the full stem `foo.CX_report.txt` (only the `.CpG_report.txt` suffix is eligible to strip, which doesn't match), so the report becomes `foo.CX_report.txt.CpG_report.txt` — exactly as Perl produces it. Encode the cx-selected strip; assert both cross cases in V7.
    - **`output_dir` and `parent_dir` have DIFFERENT defaults** (C2, reviewer B vs Perl :2070-2071, :2108-2110): `parent_dir` defaults to **`getcwd()`** (absolute CWD); `output_dir` defaults to the **empty string `""`** (a path *prefix*, NOT the CWD path) — Perl writes outputs to `"${output_dir}${filename}"`, so an empty prefix means "relative to CWD" without baking an absolute path into any constructed path. When `--dir` IS given, Perl makes it absolute + ensures a trailing `/` (`:2084-2106`); reproduce (resolve to absolute, push trailing separator). Keep these two fields distinct in `ResolvedConfig` (`output_dir: String`-prefix vs `parent_dir: PathBuf`).

`main` exit codes (match dedup): 0 success; 1 any `BismarkC2cError`; 2 clap parse error.

### 3.3 Genome reader (`genome.rs`)

`Genome::load(genome_folder) -> Result<Genome, BismarkC2cError>` reproducing Perl `read_genome_into_memory:1648-1739` + `extract_chromosome_name:1741-1751`:

1. **Glob priority** (SPEC §6.1; rev 1 tier-semantics clarified, reviewers A+B vs Perl :1654-1673): collect top-level files matching `*.fa`; if **zero filenames matched**, try `*.fa.gz`; then `*.fasta`; then `*.fasta.gz`. **The first tier with ≥1 matching FILENAME wins** — do NOT union tiers, and **the win is decided by filename match, NOT by whether the files yield usable sequence**. Only if **all four** globs matched zero filenames → `NoGenomeFasta` (Perl dies at :1671-1673). Do **not** descend subdirectories (skips `Bisulfite_Genome/`). Within a tier, file order is irrelevant to output (§6 / D1).
2. **Skip** any file literally named `Mus_musculus.NCBIM37.fa` (Perl :1678) — the skip happens **inside** the per-file loop, AFTER the tier was chosen. Consequence (rev 1, reviewer A): a winning tier containing *only* `Mus_musculus.NCBIM37.fa` produces an **empty genome with NO error** (the glob was non-empty, so `NoGenomeFasta` does not fire, and there's no fall-through to the next tier). Reproduce exactly; test it.
2a. **Malformed / empty file in the winning tier → error** (rev 1, reviewer B I2/I4 vs Perl `extract_chromosome_name:1741-1751`): Perl reads the first line as a FASTA header and `die`s if it isn't (`/^>/` fails) — an empty file (undef first line) or a headerless file both die. noodles silently yields zero records for these, so we must **detect a non-empty file that produced zero valid records / a first non-`>` byte and raise `MalformedFastaHeader { file }`** to match Perl. (A truly empty winning tier is handled by step 1; this is about a *present* file that is empty/garbage.)
3. For each file: open (if name ends `.gz` → `flate2::read::MultiGzDecoder`, else plain `File`), wrap in `BufReader`, feed a `noodles_fasta::io::Reader`.
4. For each FASTA record: **chromosome name** = first whitespace-delimited token of the definition (Perl `split /\s+/`). **Resolved (rev 1, reviewers A+B read noodles-fasta 0.61.0 source):** noodles `Definition::name()` already returns the bytes up to the first ASCII whitespace — exactly Perl's token 0 — so use it directly; **the manual-split fallback is unnecessary** (dropped). Keep the name as `Vec<u8>`. _Note (reviewer B): noodles parses the header through a `String` internally, so the `Vec<u8>`-name "non-UTF-8 fidelity" rationale is weaker than first stated; we keep `Vec<u8>` for cheap byte-compare + consistency with `cram_ref.rs`, not for non-UTF-8 support._
5. **Sequence**: concatenate, **uppercase** each base (`b.to_ascii_uppercase()` — load-bearing per pitfall P2). **Resolved (rev 1):** noodles already strips trailing `\r` (CRLF) from header + sequence lines, so **no manual `\r` strip is needed** — but add a CRLF fixture *test* (V-row) to lock that noodles behavior, since byte-identity depends on it. Empty-sequence records: keep (Perl warns but stores) — store an empty `Vec<u8>`.
6. **Duplicate name** across records/files → `DuplicateChromosomeName { name }` (Perl :1702-1705).
7. **`u32` overflow guard** (SPEC §15): if any sequence length `> u32::MAX`, return `ChromosomeTooLong { name, len }` (positions are `u32`). Practically never hits (hg38 max ≈ 2.5e8) but fails loud, not silent.
8. Store `name → uppercased seq` in a `HashMap<Vec<u8>, Vec<u8>>`. **Genome map order is byte-identity-irrelevant** (covered = cov-file order, set in Phase B; uncovered = bytewise-sorted via `names_sorted()`), so a `HashMap` is sufficient (clarifies SPEC §11's `IndexMap` sketch — see §11 Deviations).

`Genome` API (consumed by Phase B):

```rust
pub struct Genome { chromosomes: std::collections::HashMap<Vec<u8>, Vec<u8>> }
impl Genome {
    pub fn load(genome_folder: &std::path::Path) -> Result<Self, BismarkC2cError>;
    pub fn get(&self, name: &[u8]) -> Option<&[u8]>;     // sequence bytes
    pub fn contains(&self, name: &[u8]) -> bool;
    pub fn names_sorted(&self) -> Vec<&[u8]>;             // bytewise sort, for uncovered pass
    pub fn len(&self) -> usize;                           // chromosome count
    pub fn is_empty(&self) -> bool;
}
```

**INVARIANT (rev 1, reviewer A — keeps Deviation D1 airtight through Phases B–D):** `Genome` exposes **no public insertion-order-dependent iterator** — the only name-iterating accessor is `names_sorted()` (bytewise-sorted). Any future "iterate chromosomes" need must go through a sorted or explicitly-cov-ordered accessor, never the raw `HashMap` order. This is what guarantees genome-map order can never leak into output. Enforce by keeping `chromosomes` private and NOT adding an `iter()`/`keys()` passthrough.

## 4. Signatures

```rust
// cli.rs
#[derive(clap::Parser, Debug)]
#[command(name = "coverage2cytosine_rs", about = "...", disable_version_flag = true)]
pub struct Cli { /* §3.1 fields */ }
impl Cli { pub fn validate(self) -> Result<ResolvedConfig, BismarkC2cError>; }

#[derive(Debug, Clone)]
pub struct ResolvedConfig { /* SPEC §11 fields, resolved */ }

// error.rs
#[derive(Debug, thiserror::Error)]
pub enum BismarkC2cError { /* §5 variants */ }

// genome.rs — §3.3
// lib.rs
pub fn version_string() -> String;   // "coverage2cytosine_rs <semver> (<os>/<arch>)"
```

## 5. `BismarkC2cError` variants (Phase A subset)

`Io(#[from] std::io::Error)`; `MissingOutput`; `MissingGenomeFolder`; `UnsupportedFlag { flag: &'static str }`; `MergeCpgsWithCx`; `MergeCpgsWithSplit`; `MergeCpgsWithThreshold`; `DiscordanceWithoutMerge`; `DiscordanceOutOfRange { value: u8 }`; `ThresholdNotPositive`; `NoGenomeFasta { dir: PathBuf }`; `DuplicateChromosomeName { name: String }`; `ChromosomeTooLong { name: String, len: usize }`; **`MalformedFastaHeader { file: PathBuf }`** (rev 1 — a present file in the winning glob tier with no valid FASTA header / zero records; mirrors Perl `extract_chromosome_name` `die`). (Phase B adds `EmptyCoverageInput`; Phase D adds `MergeCpgSanityViolation`.) Error strings echo Perl's `die` wording where it exists.

## 6. Efficiency

- Genome load is O(total genome bytes); whole genome held in RAM (matches Perl; hg38 ≈ 3 GB). Uppercasing is a single in-place pass per record.
- `HashMap<Vec<u8>, Vec<u8>>` lookup is O(1) per chromosome (Phase B does one lookup per covered chromosome).
- `names_sorted()` is O(K log K) in chromosome count K (tiny).
- No premature parallelism (byte-identity gate first; perf is a v1.x phase per SPEC §10.7).

## 7. Integration

- **Reads:** the genome FASTA dir only (Phase A). The cov infile is opened in Phase B.
- **Writes:** nothing in Phase A (no report yet).
- **Exposes** (lib API): `Cli`, `ResolvedConfig`, `Genome`, `BismarkC2cError`, `version_string()` — consumed by Phase B–E and (eventually) by `bismark-extractor`'s inline switch (out of scope here).
- **Downstream:** `ResolvedConfig`'s resolved fields (`cpg_only`, `threshold`, `output_stem`, `zero_based`, …) are the contract Phase B codes against; the `Genome` is the report walk's input.

## 8. Assumptions

**From epic (shared):**
1. Byte-identity to Perl v0.25.1 binds all in-scope output; STDERR exempt.
2. Genome uppercased on load; held wholly in RAM.
3. `u32` positions/counts with an overflow guard.
4. `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]`; clap-derive `Cli`→`validate()`; `thiserror`; conventions match dedup/extractor.
5. All work in `../Bismark-c2c`; never touch `rust/bismark-extractor` or `rust/bismark-bedgraph`.

**Phase-A specific:**
6. noodles `fasta::Reader` over a `MultiGzDecoder`/`File` `BufReader` reproduces Perl's record splitting; chromosome name = first whitespace token. **Resolved (rev 1):** noodles `record.name()` already returns up-to-first-whitespace and already strips `\r` — confirmed against noodles-fasta 0.61.0 source by both reviewers; no manual split or `\r` strip needed.
7. Genome map order is byte-identity-irrelevant (justified §3.3 step 8); `HashMap` chosen over `IndexMap` accordingly.
8. The four v1.x flags are declared in clap but rejected at `validate` (not `hide`-only) so users get a clear message, not a silent no-op.
9. `.fa.gz` are plain gzip (Perl `gunzip -c`); `MultiGzDecoder` handles plain + multi-member gzip (and BGZF, which is gzip-framed).

## 9. Validation

| # | Verify | How | Expected |
|---|--------|-----|----------|
| V1 | Crate builds + lints clean | `cargo build/clippy/fmt -p bismark-coverage2cytosine` in worktree | no warnings/errors |
| V2 | `--help` lists v1.0 flags; `--version` provenance | `assert_cmd` run | help contains `--merge_CpGs`, `--CX_context`, etc.; version matches `coverage2cytosine_rs \d+\.\d+` |
| V3 | clap definition valid | `Cli::command().debug_assert()` | passes |
| V4 | Each validation rule fires | unit tests per §3.2 rule (missing -o, missing -g, merge+CX, merge+split, merge+threshold, discordance-without-merge, discordance range, threshold==0) | correct `BismarkC2cError` variant each |
| V5 | v1.x flags rejected | `--gc`/`--nome-seq`/`--drach`/`--ffs` each | `UnsupportedFlag { flag }` |
| V6 | `cpg_only` coupling | no `--CX` ⇒ `cpg_only==true`; `--CX` ⇒ false | correct |
| V7 | `output_stem` strip is **context-conditional** (C1) | `-o foo.CpG_report.txt` (default) → stem `foo`; `-o foo.CX_report.txt` (default/CpG) → stem **`foo.CX_report.txt`** (NOT stripped); `-o foo.CX_report.txt --CX` → stem `foo`; `-o foo.CpG_report.txt --CX` → stem `foo.CpG_report.txt` (NOT stripped); `-o foo` → `foo` | exact stems per Perl :107-112 |
| V7b | `output_dir` vs `parent_dir` defaults (C2) | no `--dir`, no `--parent_dir` | `output_dir == ""` (empty prefix); `parent_dir == getcwd()`. With `--dir d` → absolute + trailing `/` |
| V8 | Genome glob priority | dir with both `chr.fa` and `chr.fa.gz` | only `.fa` tier read (gz ignored) |
| V9 | Mus skip | dir with `Mus_musculus.NCBIM37.fa` + `chr1.fa` | only `chr1` loaded |
| V9b | Mus-only winning tier ⇒ empty genome, **no error** | dir with ONLY `Mus_musculus.NCBIM37.fa` (a `.fa`) | `Genome` loads empty; **no `NoGenomeFasta`**, no fall-through to other tiers |
| V10 | Uppercase | FASTA with lowercase `acgt` | stored as `ACGT` |
| V11 | Multi-FASTA + first-token name + **trailing-description drop** | `>chr1 some description\nACGT\n>chr2\n...` | names exactly `chr1`,`chr2` (description dropped) |
| V11b | Cross-file duplicate name → error | two separate `.fa` files both declaring `chr1` | `DuplicateChromosomeName { name: "chr1" }` |
| V11c | Malformed / empty file in winning tier → error | a present `bad.fa` that is empty OR has no `>` header | `MalformedFastaHeader { file }` |
| V12 | `.fa.gz` plain-gzip load | gzip (not bgzf) a FASTA | loads identically to plain |
| V12b | `.fa.gz` BGZF load | BGZF-framed FASTA (cram_ref.rs test style) | loads identically (MultiGzDecoder handles gzip-framed BGZF) |
| V13 | `names_sorted` order | chromosomes `chr10,chr2,chrX` | bytewise sort (`chr10`<`chr2`) |
| V14 | Empty genome dir (all four globs empty) | no FASTA at all | `NoGenomeFasta` |
| V15 | `--CX` alias parses = `--CX_context` | `--CX` and `--CX_context` | both ⇒ `cx_context==true`; `-CX` is NOT accepted as a short |
| V16 | CRLF sequence handled | FASTA with `\r\n` line endings | `\r` not present in stored sequence (locks noodles auto-strip) |
| V17 | Empty-sequence record kept | `>chrEmpty\n>chr1\nACGT\n` | `chrEmpty` stored with empty seq; `chr1` intact |

## 10. Questions or ambiguities

| Priority | Question | Assumption taken |
|----------|----------|------------------|
| **Resolved** | Does noodles `fasta::Record::name()` return up-to-whitespace, or the full definition line? | **Up-to-first-ASCII-whitespace** — both reviewers read noodles-fasta 0.61.0 source; matches Perl `split /\s+/` token 0. Manual-split fallback dropped. |
| **Resolved** | `.fa.gz` framing in real Bismark genomes (plain gzip vs bgzf)? | `flate2::MultiGzDecoder` handles plain gzip AND gzip-framed BGZF; noodles `build_from_path` is BGZF-only so `MultiGzDecoder` is genuinely required in Phase A. Tested by V12 (plain) + V12b (BGZF). |
| Open | Bytewise vs locale sort for `names_sorted` | **Bytewise** (`Vec<u8>` `Ord`) — matches Perl default `sort` under `LC_ALL=C` (the byte-identity test env, SPEC §12.3). |

No **Critical** ambiguities remain — scope, flags, and genome semantics are pinned by the SPEC + Felix's confirmed answers + the rev-1 reviewer folds (C1, C2).

## 11. Self-Review

**Checked:**
- **Logic:** validation order mirrors Perl `process_commandline`; the `threshold Some(0)` vs `None` distinction is handled (only explicit 0 is an error). `cpg_only = !cx` coupling reproduced.
- **Edge cases:** empty genome dir (V14), Mus skip (V9), dup names (V11), lowercase (V10), `.gz` (V12), empty-sequence records (stored, per Perl), multi-FASTA (V11), `u32` overflow guard (§3.3.7).
- **Efficiency:** single uppercase pass; O(1) lookups; no premature parallelism.
- **Integration:** lib API (`ResolvedConfig`/`Genome`/error) is the exact seam Phase B needs; matches dedup's `validate→ResolvedConfig` contract.

**Adjusted from SPEC during planning:**
- **Deviation D1 (genome map structure):** SPEC §11 sketched `IndexMap` for `Genome`; this plan uses `HashMap` because the genome map's order never reaches output (covered = cov-file order [Phase B]; uncovered = bytewise-sorted). `IndexMap` is deferred to Phase B for the *covered-chromosome appearance list*. This corrects a SPEC over-statement and trims a Phase-A dep. (Will note in SPEC at next rev.)
- **Dep precision:** `flate2` is needed in Phase A (plain-gzip `.fa.gz`), earlier than the SPEC's "Phase C gzip" framing implied — because the genome reader, not just output, can be gzipped.

**Folded from dual plan-review (rev 1, 2026-05-29 — both reviewers APPROVE-WITH-CHANGES):**
- **C1 (Critical, A+B): context-conditional `output_stem` strip** — strip exactly one suffix gated on `--CX`, not both. Fixed §3.2 step 11 + V7. Byte-identity trap.
- **C2 (Critical, B): split `output_dir` (default `""`) vs `parent_dir` (default `getcwd()`)** — distinct Perl defaults; fixed §3.2 step 11 + §3.1 table + V7b.
- **Glob tier semantics (A+B)**: first tier with ≥1 *filename* wins (by name, not by usable content); Mus-only tier ⇒ empty genome with no error; present-but-malformed/empty file ⇒ `MalformedFastaHeader`. Fixed §3.3 steps 1/2/2a + §5 + V9b/V11c.
- **`--CX` clap surface (A+B)**: `long="CX_context"` + `visible_alias="CX"`, never `-CX`. Fixed §3.1 + V15.
- **noodles facts resolved (A+B)**: `record.name()` up-to-whitespace (fallback dropped); noodles auto-strips `\r` (manual strip dropped, CRLF test added). Fixed §3.3 steps 4/5 + §10.
- **`Genome` no-public-iterator invariant (A)** added (§3.3) — keeps D1 airtight downstream.
- **Test coverage (A+B)**: added V7b, V9b, V11/V11b/V11c, V12b, V15, V16, V17.

**Remaining risks:**
- Detecting a present-but-empty/garbage FASTA in the winning tier (V11c) requires checking noodles yields ≥1 record per non-empty file — verify the exact noodles signal (zero records vs parse error) at impl time.
- `output_dir`-as-string-prefix vs `PathBuf` join must be exercised end-to-end in Phase B/C (where files are actually written); Phase A only resolves the value.
- Optional (deferred, non-blocking): `discordance` parse-vs-validate exit-code split (clap parse error = exit 2 vs `DiscordanceOutOfRange` = exit 1) — documented, not changed.

## Implementation notes (Phase A — 2026-05-29)

**Status: implemented + green.** Crate `bismark-coverage2cytosine` (`lib` + `bin coverage2cytosine_rs`) created in the `../Bismark-c2c` worktree. Files: `Cargo.toml`, `src/{lib,error,cli,genome,main}.rs`, `tests/sanity.rs`; added to workspace `members`.

**Results:** `cargo build` ✓; `cargo clippy --all-targets -- -D warnings` ✓ clean; `cargo test -p bismark-coverage2cytosine` → **40 passed / 0 failed** (35 unit + 5 integration); `cargo fmt` applied; full workspace `cargo build` ✓ (siblings untouched — git shows only `rust/Cargo.toml`, `rust/Cargo.lock`, new `rust/bismark-coverage2cytosine/`).

**Iteration log:**
- #1: First `cargo test` failed — `Genome::unwrap_err()` in tests needs `Genome: Debug`. Added `#[derive(Debug)]` to `Genome`. Resolved.
- #2: `clippy -D warnings` flagged `type_complexity` on `Vec<(Vec<u8>, Vec<u8>)>` return. Added `type FastaRecords = Vec<(Vec<u8>, Vec<u8>)>` alias; updated `read_one_fasta`/`collect_records` signatures. Clean.

**Empirically confirmed (had been read-only reviewer claims):** noodles-fasta 0.61.0 strips `\r` (CRLF test green); `flate2::MultiGzDecoder` decodes both plain gzip and BGZF (both gz tests green); `record.name()` is up-to-first-whitespace (multi-FASTA test green). The plan's dropped manual `\r`-strip + name-split fallback are confirmed unnecessary.

**No design deviations beyond the two mechanical fixes above.** All §3 rules, §5 errors, §3.3 genome quirks, and V1–V17 are implemented. `u32` guard tested via the `check_chr_len` helper (a >4 GiB fixture is infeasible).

**Post-code-review fixes (rev 2, 2026-05-29 — dual code-review + plan-manager).** plan-manager verdict **COMPLETE** (30/30 checklist, 10/10 tasks, 17/17 V-rows → passing tests). Both code reviews APPROVE (no Critical/High). Folded the agreed Medium/Low findings:
- **Dotfile glob (B-1, byte-divergence potential):** `discover_fasta_files` now excludes leading-dot files (`!n.starts_with('.')`) — Perl's `<*.fa>` never matches dotfiles. + test `dotfiles_are_not_matched_by_glob`.
- **Error conflation (A-M2 = B-B4):** `collect_records` maps only `ErrorKind::InvalidData` → `MalformedFastaHeader`; other noodles errors (truncated gzip etc.) → `Io`. Confirmed: nameless-header → InvalidData (the new `bare_or_nameless_header_errors` test passes), so headerless/empty cases still route to `MalformedFastaHeader`.
- **Header divergence (A-M1):** accepted + documented (SPEC §6.3) + pinned by `bare_or_nameless_header_errors`. Cannot occur on a Bowtie2-built genome.
- **Unused deps removed:** `noodles-core` (dep) + `bstr` (dev-dep).
- **Added `.fasta`-tier-selection test** (`fasta_tier_chosen_when_no_fa`).
Result after fixes: **43 tests pass** (38 unit + 5 integration), clippy clean, workspace builds, siblings untouched.

## Revision history
- **rev 0** (2026-05-29): initial Phase A plan from EPIC + SPEC rev 1. Awaiting dual plan-review.
- **rev 1** (2026-05-29): dual plan-review folded (A+B both APPROVE-WITH-CHANGES; D1 confirmed sound by both). 2 Critical (C1 context-conditional stem strip; C2 output_dir/parent_dir defaults) + Important (glob tier semantics, `--CX` surface, noodles facts, Genome invariant) + 8 new test rows. SPEC §11/§15 synced.
- **rev 2** (2026-05-29): implemented + dual code-review + plan-manager (COMPLETE). Folded code-review Medium/Low fixes (dotfile glob, error conflation, header-divergence doc+test, unused-dep removal, .fasta-tier test). 43 tests pass.
