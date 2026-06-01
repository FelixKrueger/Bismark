# PLAN — `bismark-report` (phased implementation of the Rust `bismark2report` port)

**Companion to:** `SPEC.md` (rev 1, 2026-06-01) + `SPEC_REVIEW_A.md` / `SPEC_REVIEW_B.md` (dual plan-review folded into the SPEC).
**Branch / worktree:** `rust/bismark2report` @ `~/Github/Bismark-report` (off `rust/iron-chancellor` @ `7dbcee3`).
**Status:** DRAFT rev 1 (2026-06-01) — **dual plan-review findings folded in** (`PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md`). Awaiting the explicit implementation trigger. Do **not** implement yet.
**Rev 1 changes (dual plan-review folded in):** (a) `--version`/`--man`/`--help` wiring corrected to mirror genomeprep/dedup — `#[command(disable_version_flag = true)]` + manual `version: bool` (prints `version_string()`) + `man: bool` (`print_long_help()`), handled in `main`; NOT clap auto-version, and `--help` cannot be aliased (A2/A3). (b) The **8 PE/SE `*_text` label strings** are byte-load-bearing — lift verbatim (Perl 218/222/236/241/247/252/258/263) + fixture (B1). (c) **Zero NEW dependency trees**: drop `glob` (use `read_dir`); deterministic timestamp = pure-std UTC integer math; default local time via the **already-locked `libc`** `localtime_r`; `chrono`/`time` avoided (A1/A6/§8/§10). (d) **Asset embedding is a NEW pattern** (no sibling precedent — the rev-0 "mirror genomeprep" claim was false): `include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../plotly/…"))` + a drift test asserting embedded == repo `plotly/` bytes (A5). (e) **Report bodies read as BYTES** (byte-oriented parse/substitute) so non-UTF8 filenames round-trip like Perl (`read_to_string` would panic); template/assets stay `&str` (A1/§8). (f) **Companion resolution order** = dedup→nucleotide→splitting→mbias, nucleotide uses `defined` vs others' truthiness — replicated for faithfulness/error-parity (HTML bytes identical regardless) (D1). (g) New fixtures: splitting/dedup/nuc gate-failure, alignment-gate-passes-but-context-fields-absent → empty fills, percent-`N/A` table-vs-graph, exact #711 nucleotide golden (E1/§9).
**Mode:** TDD-leaning — the **Perl `bismark2report v0.25.1` script is the primary test oracle** (mirror genomeprep/methcons); pure parsers are unit-tested before wiring.

---

## 1. Goal

Produce `bismark2report_rs`, a single self-contained binary that reads a Bismark alignment report (+ up to 4 optional companion reports) and emits an HTML report **byte-for-byte identical** to the current Perl `bismark2report v0.25.1`, modulo the one `localtime` timestamp line (normalized in the gate; SPEC §7). No BAM I/O, no `bismark-io` dependency — same shape as `bismark-genome-preparation`.

## 2. Context — placement & references

- **New crate:** `rust/bismark-report/` (added to `rust/Cargo.toml` `members`). Crate `bismark-report`, bin `bismark2report_rs` (SPEC §3 — `bismark-report` is convention-correct, no rename).
- **Closest sibling to mirror:** `rust/bismark-genome-preparation/` (standalone, no BAM, `cli.rs`/`error.rs`/`lib.rs`/`logging.rs`/`main.rs` + domain modules). Logging mirrors `bismark-extractor/src/logging.rs`. CLI/`--version` mirror `bismark-dedup`.
- **Perl source of truth:** `/Users/fkrueger/Github/Bismark/bismark2report` (1316 lines). **Assets:** `/Users/fkrueger/Github/Bismark/plotly/{plotly_template.tpl, plot.ly, bismark.logo, bioinf.logo}`.
- **No companion phase dependencies** (standalone plan, not an epic).

### 2.1 Proposed module layout
```
rust/bismark-report/
├── Cargo.toml                 # crate bismark-report; [[bin]] bismark2report_rs
├── CHANGELOG.md               # (Phase F)
├── README.md                  # (Phase F)
├── src/
│   ├── main.rs                # entry; maps errors → exit codes (§6.1)
│   ├── lib.rs                 # run() orchestration; version_string(); re-exports
│   ├── cli.rs                 # clap derive (incl. hidden --__test_timestamp)
│   ├── error.rs               # thiserror enum
│   ├── logging.rs             # verbose-gated STDERR (mirror extractor Logger)
│   ├── assets.rs              # include_str! the 4 assets + normalize() helper
│   ├── timestamp.rs           # local default | --__test_timestamp UTC override; Perl sprintf
│   ├── discovery.rs           # auto-detect globs, basename, companion resolution, multi-report loop slots
│   ├── template.rs            # doc assembly: asset inject, section collapse/excise, value subst, write
│   └── reports/
│       ├── mod.rs             # shared helpers (tab-split (undef,val); %-strip; whole-doc subst)
│       ├── alignment.rs       # mandatory parser + fill
│       ├── dedup.rs
│       ├── splitting.rs
│       ├── mbias.rs
│       └── nucleotide.rs
└── tests/
    ├── fixtures/              # crafted PE/SE report sets + special-case inputs + committed goldens
    ├── cli.rs                 # arg parsing + exit codes
    ├── assets.rs              # normalize() byte-equivalence + {{-free assertion
    ├── template.rs            # section logic + M-bias matrix + end-to-end fills
    ├── parsers.rs             # per-parser unit tests (or co-located #[cfg(test)])
    └── perl_vs_rust.rs        # Perl-oracle gate (auto-skip if perl absent) + real-data #[ignore]
```

---

## 3. Behavior (authoritative summary — see SPEC §2 for the full contract)

The orchestration order is **load-bearing** (SPEC §2.3) and must be reproduced exactly:
1. Read `plotly_template.tpl` → `doc` (normalized §2.6).
2. Inject `plot.ly` (greedy/dotall splice between the two `{{plotly_goes_here}}` markers; error if absent).
3. Inject `bismark.logo`, then 4. `bioinf.logo` (same splice).
5. Fill `{{date}}`/`{{time}}` (timestamp).
6. Alignment parser (mandatory) — fill or, on `defined`-gate failure, leave placeholders.
7. Dedup: present → collapse markers + fill; absent → excise block.
8. Splitting: same.
9. M-bias: present → collapse R1 markers + fill; SE → excise R2 block, PE → collapse R2 markers; absent → excise both R1+R2 blocks. **Script-block data placeholders are never inside these spans — they survive when unfilled (SPEC §2.7d/§5.4).**
10. Nucleotide: present → collapse markers + fill; absent → excise block.
11. Write `doc` verbatim (`doc` ends in `\n`; no trailing manipulation).

Per the multi-report loop: one HTML per alignment report; companion slots resolved per SPEC §2.2 (explicit / `none` / auto-detect), with the line-1256 explicit-var reset between reports.

---

## 4. Implementation outline (phased)

### Phase A — Crate scaffold + CLI + asset embedding + timestamp (infra MVP)

- **A1. Scaffold** (mirror `bismark-genome-preparation` layout): create `rust/bismark-report/Cargo.toml` (deps: `clap` derive pinned to the workspace version, `anyhow`, `thiserror`, and `libc` **at the version already in `Cargo.lock`** for `localtime_r`; **no** `flate2`/`noodles`/`bismark-io`/`glob`/`chrono`/`time` — see A6). Add `bismark-report` to `rust/Cargo.toml` `members`. Stub `main.rs`/`lib.rs`. **Report bodies are read as bytes** (not `read_to_string`) so non-UTF8 filenames round-trip like Perl (§8); the embedded template/assets are ASCII `&str`.
- **A2. `cli.rs`** — clap derive struct with the exact Perl flag spellings: `--alignment_report`, `--dedup_report`, `--splitting_report`, `--mbias_report`, `--nucleotide_report` (all `Option<String>`), `--dir`, `-o/--output`, `--verbose`, and the **hidden** `--__test_timestamp <i64>` (`#[arg(hide = true)]`). **Version/help wiring mirrors genomeprep/dedup (rev 1, verified):** `#[command(disable_version_flag = true)]` + a manual `pub version: bool` and a manual `pub man: bool` (doc: "print full help, alias of `--help`"). clap's built-in `-h/--help` → exit 0; `main` handles `version`/`man` **before** `run()` (print `version_string()` / `Cli::command().print_long_help()`, then exit 0). Do **not** use clap auto-version, and do **not** try to alias `--help` (clap can't). Exit-0 outcomes per SPEC §6.1; Perl's exit-1-on-help intentionally not reproduced.
- **A3. `lib.rs::version_string()`** via `env!("CARGO_PKG_VERSION")` (dedup precedent) — printed by `main` when `cli.version` is set (the `disable_version_flag` manual path from A2). The Bismark `v0.25.1` constant lives only in banner text, never in HTML bytes (not gated).
- **A4. `logging.rs`** — mirror `bismark-extractor` `Logger`; `--verbose` gates extra STDERR. (Diagnostic text is NOT byte-gated; `sleep` pauses dropped — SPEC §4.)
- **A5. `assets.rs`** — **embed the 4 assets (NEW pattern — rev 1: no workspace crate does this; the rev-0 "mirror genomeprep" claim was wrong).** Use `include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../plotly/<asset>"))` (manifest-relative → cwd-independent, no 3 MB duplication into the crate) **plus a drift test** asserting each embedded asset's bytes equal the live `plotly/<asset>` file (so the embed can't silently go stale; copy-into-crate is the fallback if repo-layout coupling is undesirable). Implement `normalize(raw: &str) -> String` faithfully reproducing `read_report_template` (SPEC §2.6/§8.2): split on `\n`, drop a single trailing empty element iff the input ended in `\n`, `replace('\r', "")` per piece, rejoin with `\n`, append a final `\n`; **empty input → `""`** (guard). Expose the 4 normalized assets.
- **A6. `timestamp.rs`** — `fn timestamp(test_epoch: Option<i64>) -> (date, time)`. **Zero NEW dependency trees (rev 1):** the deterministic path (`Some(epoch)`) converts the epoch → **UTC** civil time with **pure-std integer math** (days-from-civil; Unix time has no leap seconds). The default path (`None`) shows **local** time via the **already-locked `libc`** `localtime_r` (declare `libc` as a direct dep at the `Cargo.lock` version — not a new tree). Format with Perl's exact `%04d-%02d-%02d` / `%02d:%02d:%02d`. **Do not add `chrono`/`time`.** (The default-path value is normalized away in the gate, so a pure-std UTC fallback would also be acceptable — but `libc` keeps it faithfully local at no dep cost.)
- **A7. Phase-A tests** (`tests/cli.rs`, `tests/assets.rs`):
  - CLI parse of every flag; mutually-exclusive/`-o`-with-many deferred to Phase D.
  - `--version`/`--help`/`--man` → exit 0.
  - `normalize()` byte-equivalence vs a fresh Perl `read_report_template` intermediate for `plotly_template.tpl` (**Spike A**); plus unit cases: mid-line `\r`, no-trailing-newline, CRLF, **empty input → `""`**.
  - Assert no asset contains a live `{{` token and no asset contains `\r` (SPEC §8.13) — so literal value-substitution is safe.

### Phase B — Report parsers (pure functions, exhaustively unit-tested)

Each parser: `fn parse(text: &str) -> Captured` (pure; no I/O) + `fn fill(doc: String, c: &Captured) -> String`. Shared helpers in `reports/mod.rs`: tab-split keeping the 2nd field (`(undef,$v)=split /\t/`), `%`-strip, the whole-doc sequential substitution (`doc.replace("{{name}}", v)` in **Perl's order** — SPEC §8; cross-call re-substitution must match, so order is fixed).

- **B1. `alignment.rs`** (SPEC §2.7a): PE/SE detection by exact line text, which sets both the value AND the byte-load-bearing `*_text` label. **Lift the 8 label strings VERBATIM (rev 1 — they are in the gate):** total = `Sequence pairs analysed in total` (PE, Perl 218) / `Sequences analysed in total` (SE, 222); unique = `Paired-end alignments with a unique best hit` (236) / `Single-end alignments with a unique best hit` (241); no-aln = `Pairs without alignments under any condition` (247) / `Sequences without alignments under any condition` (252); multiple = `Pairs that did not map uniquely` (258) / `Sequences that did not map uniquely` (263). Context methylation (incl. the `Total C to T conversions` alternate for unmethylated, and `C methylated in Unknown context (CN or CHN):`); strand origin (PE vs SE patterns); `{{filename}}`/`{{bismark_version}}` from `Bismark report for: X (version: Y)`. **Fill gate = `is_some()` on `unique,no_aln,multiple,no_genomic,total_seqs`** (NOT truthiness; `0` passes). **Gate-passing ≠ all fields present (rev 1):** `total_C_count`/`meth_*`/`unmeth_*` are filled *inside* the gated block but are **not** in the 5-field gate, so an absent one fills the **empty string** (Perl undef-in-`s///`) — reproduce (fixture E1.9). Unknown-context `<tr>` inject snippets reproduced **byte-for-byte** (5/32 spaces; 4sp+4tab; 4sp+3tab — lift from Perl 433–448). Plotly strings: `{{alignment_stats_plotly}}`, `{{strand_alignment_plotly}}`, `{{cytosine_methylation_plotly}}` (N/A→`0` in graph only; table shows `N/A`).
- **B2. `dedup.rs`** (SPEC §2.7b): captures with `\s.*`-trim on dups/diff_pos; **leftover fallback = `total - dups`** (signed `i64`, reproduce negatives if any); fill gate `is_some()` on all 4 (`0` passes); `{{duplication_stats_plotly}}` = `leftover,dups`.
- **B3. `splitting.rs`** (SPEC §2.7c): like alignment's context block but `*_splitting` placeholders; note the phrasing differences (only `Total C to T conversions…`; `C methylated in Unknown context:` w/o `(CN or CHN)`); gate `is_some()` on 6 fields; its own Unknown-context snippets.
- **B4. `mbias.rs`** (SPEC §2.7d): accumulate per `(read_identity, context)` the `perc_x/perc_y/coverage_x/coverage_y` vecs; `state` = `paired` iff an `R2` header seen (else `single`); fill `{{mbias1_*}}` always (empty join allowed), `{{mbias2_*}}` **only if any R2 data rows** (`%mbias_2` analog). Return `(state, doc)`. The dead `{{bm_mbias_2}}` → `false` substitution is a no-op (template has no such token) — reproduce as no-op.
- **B5. `nucleotide.rs`** (SPEC §2.7e): per-line `\r`-strip + tab-split 6 cols; **line-0 header validation** (col 3 == `percent sample`, col 5 == `percent genomic`, else error); iterate the **fixed 20-key order** `A,T,C,G,AC,CA,TC,CT,CC,CG,GC,GG,AG,GA,TG,GT,TT,TA,AT,AA`; fill `{{nuc_<K>_*}}` verbatim; **missing key → percentages `0`, counts/coverage empty string** (Perl undef-in-replacement); plot arrays with the **distinct separators** (` , ` for x, `','` for y); the log2 ratio is **computed-but-not-emitted** → do NOT output any float. Gate = `looksOK`.
- **B6. Phase-B unit tests** (`tests/parsers.rs` or co-located): per parser — branch coverage; **gate passes when a field is `0`** (regression guard); dedup fallback; splitting phrasing variants; nucleotide fixed-order + header-validation error + missing-key (`0`/empty); mbias state + empty-R2.

### Phase C — Template assembly + section logic + orchestration

- **C1. `template.rs::inject_asset(doc, marker, asset)`** — greedy/dotall splice: find first index of `marker` and the end index of its **last** occurrence; replace the whole span with `asset`. Error if `marker` not found (mirror Perl `die`). Apply for plot.ly, bismark.logo, bioinf.logo in order.
- **C2. Section collapse/excise** — `collapse(doc, marker)` = remove **all** occurrences (`doc.replace(marker, "")`); `excise(doc, marker)` = first-index … last-occurrence-end splice (greedy/dotall). Each marker occurs exactly 2×.
- **C3. M-bias wiring** — deletion driven by `state` (SE→excise R2 block; PE→collapse R2 markers; absent→excise both); fill driven by R2-data-present. **Do not touch the script-block placeholders** — they survive when unfilled (assert in tests).
- **C4. `lib.rs::build_report(...)`** — the 11-step orchestration (§3) for one alignment report; `timestamp()` fill at step 5.
- **C5. `write_out_report`** — write `doc` bytes verbatim (ends in `\n`).
- **C6. Phase-C tests** (`tests/template.rs`): collapse vs excise on a synthetic doc; M-bias matrix (absent → 24 placeholders survive; SE → 12 `{{mbias2_*}}` survive + R2 `<div>` gone; PE → all filled); a full crafted-PE end-to-end fill compared to a committed golden generated with `--__test_timestamp`.

### Phase D — Discovery / auto-detection / multi-report loop / output naming

- **D1. `discovery.rs`** — alignment detection: if `--alignment_report` given use it; else `read_dir(cwd)` for names ending `E_report.txt`, **lexically sorted** (C-locale bytes). `basename` via `^(.+)_(P|S)E_report.txt$`. **Companion resolution in Perl's order (rev 1): dedup → nucleotide → splitting → mbias** (Perl 1141/1170/1201/1228 — this fixes which `die` fires first when several companions each have >1 match; the resolved files and thus the **HTML bytes are identical regardless of order**, but reproduce for error-parity). For each: explicit (`none`→absent), else `read_dir` matching `starts_with(basename) && ends_with(suffix)` for `deduplication_report.txt` / `nucleotide_stats.txt` / `splitting_report.txt` / `M-bias.txt`; **>1 match → error**, 0 → absent, 1 → use. **Replicate the per-flag condition asymmetry:** nucleotide uses `defined $nucleotide_coverage_report` (line 1171) while the other three use truthiness (`if ($x)`) — matters only for an explicit empty-string arg. Reproduce the **line-1256 reset**: an explicit companion flag applies to report #1 only; reports #2+ fall back to auto-detect.
- **D2. Output naming** — strip dir (`s/^.*\///`), strip `.txt`, append `.html`; or `-o` value **verbatim** (no `.html`); prefix `--dir` (trailing `/` unless empty). `-o` with **>1** alignment report → error.
- **D3. `lib.rs::run()`** — build the per-report slots, loop, call `build_report`, write each. No-alignment-report-found → emit the Perl hint message + **nonzero** exit (SPEC §6.1).
- **D4. Phase-D tests** (`tests/cli.rs`): tmpdir glob detection (PE + SE); basename; companion >1 → error; `none` skip; multi-report explicit-reset; naming with `--dir`/`-o`; `-o` with 2 reports → error; no-report → nonzero exit.

### Phase E — Byte-identity gate: Perl oracle + committed goldens + the 7 required fixtures

- **E1. Fixtures** (`tests/fixtures/`) — craft minimal **valid** PE and SE report sets (alignment + dedup + splitting + M-bias + nucleotide) plus the special cases: (3) alignment gate-failure (missing `no_genomic`), (4) `0`-through-gate (`no_genomic: 0`/`dups: 0`), (5) dedup leftover-fallback (no leftover line), (6) amplicon missing-nuc-key (#711 — **pin exact bytes**: `0` for percentages, empty string for counts/coverage), (7) two-alignment-reports-in-one-dir + explicit `--dedup_report`, **(8 rev 1) splitting & dedup & nucleotide gate-FAILURE** (placeholders survive, SPEC §5.4), **(9 rev 1) alignment gate-passes-but-context-fields-absent** (→ `{{total_C_count}}`/`{{meth_*}}` fill empty), **(10 rev 1) percent-`N/A`** (table shows `N/A`, graph shows `0`).
- **E2. Perl-oracle harness** (`tests/perl_vs_rust.rs`) — run `perl bismark2report …` and `bismark2report_rs …` on the same fixtures; **normalize the one timestamp line** in both (anchor on `Data processed at HH:MM:SS on YYYY-MM-DD`, replace with a fixed token, **assert exactly one match per file**); byte-diff the rest. **Auto-skip if `perl` is absent** (methcons pattern).
- **E3. Committed goldens** — generate the canonical outputs with `--__test_timestamp <fixed epoch>` (UTC) and commit; tests regenerate + byte-compare.
- **E4. SPEC §7 fixture assertions** (1) M-bias-absent → 24 survive; (2) M-bias-SE → 12 `{{mbias2_*}}` survive; (3)–(7) as above.

### Phase F — Real-data validation + docs + PR

- **F1. Real-data byte-identity** (`tests/perl_vs_rust.rs`, `#[ignore]`, oxy) — run Perl + Rust on real Bismark report sets from the benchmark datasets; normalize timestamp; diff. Verify oxy env on arrival (report paths, `perl`, `~/.cargo/bin`).
- **F2. Docs** — `README.md`, `CHANGELOG.md`, and update the top-level Rust-rewrite status table/per-tool list.
- **F3. PR** — base `rust/iron-chancellor`; epic/sub-issues per sibling convention; merge into iron-chancellor **only** on an explicit "merge for me".

---

## 5. Signatures (proposed; confirm during implementation)
```rust
// assets.rs
pub fn normalize(raw: &str) -> String;        // faithful read_report_template; empty → ""
pub const TEMPLATE: &str; pub const PLOTLY: &str; pub const BISMARK_LOGO: &str; pub const BIOINF_LOGO: &str;

// timestamp.rs
pub fn timestamp(test_epoch: Option<i64>) -> (String /*date*/, String /*time*/);

// template.rs
pub fn inject_asset(doc: String, marker: &str, asset: &str) -> Result<String, Error>; // greedy first→last
pub fn collapse(doc: String, marker: &str) -> String;   // remove all occurrences
pub fn excise(doc: String, marker: &str) -> String;     // first→last splice

// reports/*.rs  (per parser)
pub fn parse(text: &str) -> Result<Captured, Error>;    // nucleotide may error on bad header
pub fn fill(doc: String, c: &Captured) -> String;       // mbias returns (State, String)

// lib.rs
pub fn run(cli: &Cli) -> Result<(), Error>;
fn build_report(aln, dedup, split, mbias, nuc, test_epoch) -> Result<String, Error>;
```

---

## 6. Efficiency
Non-hotspot tool (one report → one HTML, run interactively). Reports are small text files → read fully into `String` (no streaming needed). The 3 MB `plot.ly` is embedded once (binary +~3 MB) and the per-report `doc` is a single owned `String` mutated in place. `str::replace` allocations are fine at this scale. No parallelism, no `mimalloc`.

## 7. Integration
- Reads: the 1–5 input report text files; embedded assets (compile-time).
- Writes: one `<report>.html` per alignment report under `--dir`.
- Order relative to other tools: post-extraction QC; consumes outputs of the aligner, deduplicate, extractor (splitting/M-bias), and bam2nuc. No effect on other crates beyond the `rust/Cargo.toml` `members` line.

## 8. Assumptions
- **Report bodies are read as BYTES (rev 1), not `&str`** — `{{filename}}` derives from a user-supplied FASTQ path that could be non-UTF8, and Perl round-trips arbitrary bytes (Rust `read_to_string` would panic). Parse line-by-line and substitute byte-oriented; a stray `\r` is handled per parser (nucleotide strips it; others rely on `chomp`-equivalent handling). The template + 4 assets are ASCII and stay `&str`; the assembled `doc` + injected values are byte-capable.
- The four assets are non-empty and `{{`-free and `\r`-free (verified in A7); the literal value-substitution + greedy splice are therefore byte-safe.
- Exit codes: clap's built-in `--help` → 0; manual `--man`/`--version` (A2) print + exit 0; arg errors → clap's 2; runtime errors → 1 (via `anyhow`/`main`). None byte-gated.
- **No NEW dependency trees (rev 1):** `glob`/`chrono`/`time` are NOT added — globbing uses `read_dir`, timestamps use pure-std UTC math (deterministic path) + the already-locked `libc` `localtime_r` (default path). Only `clap`/`anyhow`/`thiserror`/`libc` (all already in `Cargo.lock`) are used.
- The Perl `bismark2report v0.25.1` at repo root is the oracle; the committed `plotly/bismark_bt2_PE_report.html` is **stale (v0.19.1)** and is **not** used (SPEC §8.1).
- Glob sort order is low-stakes here (independent files; >1 companion → error) but reproduced lexically for the alignment loop.

## 9. Validation (key failure points)
1. **Asset normalization byte-equivalence** (A7 / Spike A) — Rust `normalize()` == Perl `read_report_template` intermediate for `plotly_template.tpl`. *Expected:* byte-identical.
2. **Fill-gate `0` regression** (B6) — a report with `no_genomic: 0` / `dups: 0` still fills. *Expected:* placeholders filled, not surviving.
3. **M-bias surviving placeholders** (C6/E4) — absent → 24 literal `{{mbias*}}`; SE → 12 `{{mbias2_*}}`. *Expected:* matches Perl exactly.
4. **Section excise greedy semantics** (C6) — first→last splice removes exactly the block. *Expected:* no marker residue, content gone.
5. **Unknown-context snippet bytes** (B6/E2) — Bowtie2 (Unknown-present) fixture. *Expected:* byte-identical tabs/spaces.
6. **Nucleotide missing-key + header-validation** (B6/E4) — amplicon fixture → `0`/empty; bad header → error.
7. **Perl-oracle byte-identity** (E2) — PE & SE full matrix, timestamp-normalized. *Expected:* identical.
8. **Exit codes** (A7/D4) — help/version 0; no-report nonzero; `-o`+2-reports error.
9. **Real-data gate** (F1, oxy) — real reports, timestamp-normalized. *Expected:* identical.
10. **Gate-passes-but-context-absent** (B1/E1.9) — alignment with the 5 gate fields present but no `Total number of C` line → `{{total_C_count}}`/`{{meth_*}}` fill **empty** (not `N/A`, not placeholder). *Expected:* matches Perl.
11. **Splitting/dedup/nuc gate-FAILURE** (E1.8) — placeholders survive (SPEC §5.4). *Expected:* matches Perl.
12. **Percent `N/A` table-vs-graph** (E1.10) — table cell `N/A`, graph value `0`. *Expected:* matches Perl.
13. **Asset drift** (A5) — embedded bytes == repo `plotly/` bytes. *Expected:* identical (guards stale embed).

## 10. Questions or ambiguities
- **Decided (rev 1):** timestamps use **zero new deps** — pure-std UTC math (deterministic) + already-locked `libc` `localtime_r` (default); `glob`/`chrono`/`time` not added (A6/A1). Asset embedding uses `include_str!` via `CARGO_MANIFEST_DIR` + a drift test (A5). Version/man wiring mirrors genomeprep (A2/A3). Report bodies read as bytes (§8).
- **No Critical ambiguities remain** — crate name, exit codes, timestamp hook, asset shipping, deps, byte-vs-str, and scope are all resolved (SPEC rev 1 + this PLAN rev 1).

## 11. Self-Review
- **Efficiency:** confirmed non-hotspot; full-read + single-String mutation is appropriate; no streaming/parallelism needed.
- **Logic:** the 11-step orchestration order (§3) matches SPEC §2.3; sequential whole-doc substitution preserves Perl's cross-call re-substitution semantics (order fixed per parser).
- **Edge cases:** empty asset (normalize guard), `0`-through-gate, gate-failure surviving placeholders, M-bias absent/SE script-block survival, dedup negative leftover, nucleotide missing-key (`0`/empty) + bad-header error, `-o` with >1 report, no-report nonzero exit, multi-report companion reset — all have explicit steps + fixtures.
- **Integration:** only touches `rust/Cargo.toml` `members`; downstream-neutral.
- **Remaining risks:** (a) the stale committed reference HTML must never be used as oracle (guarded by F1/E2 using the live Perl); (b) `localtime_r` via `libc` is `unsafe` glue — contain it in `timestamp.rs` with a test (the default-path value is non-gated anyway); (c) the `CARGO_MANIFEST_DIR`-relative embed couples to repo layout — the A5 drift test (validation §13) catches staleness, and copy-into-crate is the fallback.

---

## 12. Estimated sequencing
A (infra) → B (parsers, the bulk of unit tests) → C (assembly/sections/M-bias) → D (discovery/loop/naming) → E (Perl-oracle gate + fixtures + goldens) → F (oxy real-data + docs + PR). A–E are local; F needs oxy. Each phase ends green (unit + integration) before the next.

## 13. Next steps (workflow)
1. ✅ **Dual plan-review of this PLAN COMPLETE** (`PLAN_REVIEW_A.md`/`PLAN_REVIEW_B.md`); findings folded into this rev 1.
2. Implement **only** on the explicit trigger (`implement` / `/code-implementation`).
3. Post-implementation: dual `/code-reviewer` + `/plan-manager` coverage audit → real-data gate on oxy → docs/PR → merge on explicit "merge for me".

---

## 14. Implementation notes (2026-06-01)

Implemented Phases A–E + docs (F2) in `rust/bismark-report/` on branch `rust/bismark2report`. **Not committed/pushed; no PR** (awaiting review + explicit instruction).

**Modules** (as planned, §2.1): `cli.rs`, `error.rs`, `logging.rs`, `assets.rs`, `timestamp.rs`, `template.rs`, `discovery.rs`, `reports/{mod,alignment,dedup,splitting,mbias,nucleotide}.rs`, `lib.rs`, `main.rs`.

**Verification (all local, green):**
- `cargo build/test -p bismark-report`: **48 tests pass** — 36 unit (parsers/template/assets/timestamp/discovery), 8 CLI (`assert_cmd`), **4 Perl-oracle byte-identity** (`tests/perl_vs_rust.rs`).
- `cargo clippy -p bismark-report --all-targets -- -D warnings`: clean. `cargo fmt -- --check`: clean.
- **Byte-identity vs LIVE Perl `bismark2report` v0.25.1** (timestamp-normalized) confirmed on 4 fixtures: `wgbs_pe` (all 5 companions, PE M-bias, 3,152,702 B), `wgbs_se` (R1-only M-bias → R2 excised + `{{mbias2_*}}` survive, 3,147,357 B), `nondir_pe` (Unknown-context inject, 3,149,534 B), `minimal_pe` (no companions → all sections excised + 24 `{{mbias*}}` survive).

**Deviations from PLAN (documented):**
- **`anyhow` dropped** — the `thiserror` `ReportError` enum + `main` ExitCode mapping cover all error handling; no `anyhow::Result` was needed (keeps deps minimal, matches the "no new tree" intent). Deps: `clap`, `thiserror`, `libc` (+ dev `assert_cmd`/`predicates`/`tempfile`).
- **Committed full 3 MB goldens omitted** — the Perl-oracle test (deterministic via the same Perl) + the unit tests + the asset-drift test cover regression without bloating the repo with multiple ~3 MB HTML files. Edge behaviors (0-through-gate, gate-failure survival, dedup fallback, amplicon #711 missing-key, M-bias SE/PE) are pinned as fast hermetic **unit tests** on `parse`/`fill` rather than full-binary goldens.

**Open / remaining:**
- **F1 real-data gate on `oxy`** (full Bismark report sets, `#[ignore]`-style) — not run here (needs oxy + real reports).
- **F3 PR** — not opened; merge into `rust/iron-chancellor` only on explicit "merge for me".
- **F2 top-level Rust-rewrite status-table** update (repo-root docs) — deferred to the PR.

---

## 15. Post-implementation review + fixes (2026-06-01)

Dual `/code-reviewer` (A + B) + `/plan-manager` ran in fresh contexts (Reviewer B crashed once on an API socket error; re-run completed). **plan-manager verdict: COMPLETE** (53 DONE / 0 PARTIAL / 0 MISSING / 3 documented deviations / F1+F3 open-by-design). All findings folded:

| # | Sev | Finding | Fix |
|---|-----|---------|-----|
| 1 | **High** (B) | `Bismark report for:` version parse required `)` as the line's last byte → **CRLF reports** drop filename/version (empty `{{filename}}`/`{{bismark_version}}`) → byte divergence | `parse_report_for` now uses the **last `)` within `after`** (matches Perl's greedy, non-end-anchored regex). New CRLF Perl-oracle test (`crlf_alignment_byte_identical`) + 2 unit tests. |
| 2 | Medium (A) | Alignment-report glob used Rust byte `sort()`; Perl `File::Glob` folds case → diverges for mixed-case names, byte-relevant via the line-1256 first-report reset + explicit companion | Sort by `(ascii_lowercase, raw bytes)` (`glob_order_key`); unit test pins Perl order `a2_, a_, B_, C_`; doc comment narrowed. |
| 3 | Low (A) | `-o ""` → Rust used the empty name (write fails); Perl derives the name (truthiness) | `out_name` uses truthiness (`Some(o) if !o.is_empty()`); the `>1`-guard keeps `is_some` (Perl `defined`). |
| 4 | Low (B) | `assets.rs` doc referenced a non-existent `tests/assets.rs` | Doc now points to the inline `embedded_assets_match_repo_plotly_files` test. |

**Non-blocking note (plan-manager):** the nucleotide `looksOK` gate is intentionally omitted — it is provably always-true in Perl (the `obs`/`exp` hashref slots autovivify together on every stored key), so always-fill is byte-identical. Faithful, documented here.

**Post-fix gates:** `cargo test -p bismark-report` = **52 passed / 0 failed** (39 unit + 8 CLI + 5 Perl-oracle byte-identity incl. CRLF); `clippy --all-targets -D warnings` clean; `fmt --check` clean.

---

## 16. Real-data gate (F1) — PASSED 2026-06-01

Validated against **real Bismark v0.25.1 reports** from the profiling runs at `~/Desktop/TrimG_Bismark_test/` (real human WGBS PE; alignment + dedup + splitting + M-bias, auto-detected). Ran the live Perl `bismark2report` and `bismark2report_rs --__test_timestamp 0` in the same dir, normalized the timestamp line, byte-compared:

| Dataset | HTML | Result |
|---|---|---|
| Real **10M** PE (`SRR24827378_10M`) | 3,156,840 B | **byte-identical** ✅ |
| Real **full 55.7M** PE (`SRR24827378_GSM7445366`) | 3,157,378 B | **byte-identical** ✅ |

**Note on venue:** run locally rather than on `oxy`. `bismark2report` consumes only small *report* text files (sub-second regardless of dataset size) and its output is platform-independent (assets are identical bytes; timestamp normalized), so the value of the real-data gate is the *realism of the report formats*, fully achieved here with real Bismark output. **Not covered by real data** (no such reports in the local set): **SE** (no real SE run available) and **`--nucleotide_coverage`** (not enabled in these runs) — both covered by the synthetic Perl-oracle (`wgbs_se`) and nucleotide unit tests. A literal `oxy` run and/or real SE+nuc reports can be added if desired.

---

## 17. Second-round review (A2/B2) + fix (2026-06-01)

A workflow stop-hook prompted a re-review; since 4 fixes had landed *after* the first review, dual `/code-reviewer` A2/B2 ran focused on that diff (`CODE_REVIEW_A2.md`/`CODE_REVIEW_B2.md`). **Both confirmed all 4 first-round fixes sound, no regressions** (A2 verified glob order under 3 locales; B2 falsified end-to-end across 11+ scenarios incl. mixed-case multi-report + explicit companion).

**One new Low finding (B2), fixed:** `-o 0` — Perl string truthiness treats `"0"` as **false** (like `""`), so Perl derives the `.html` name; the rev-15 fix used `!o.is_empty()`, which accepted `"0"` and would write a file literally named `0`. Fixed with a shared `discovery::perl_truthy()` helper (`false` for `""` and `"0"`) applied at **both** truthiness sites — the `-o` name choice (`lib.rs`) AND the dedup/splitting/mbias companion checks (`discovery.rs`); nucleotide stays on `defined`/`is_some`. (Same bug class in two drivers → fixed together, per [[feedback_dual_driver_back_port]].) Added 2 regression tests (`perl_truthy_matches_perl_falsy_values` unit + `output_zero_is_perl_falsy_and_derives_name` CLI).

**Post-fix gates:** `cargo test -p bismark-report` = **54 passed / 0 failed** (40 unit + 9 CLI + 5 Perl-oracle); `clippy -D warnings` clean; `fmt --check` clean. No byte-identity-affecting issues remain open.
