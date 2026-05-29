# PLAN — `bismark-methylation-consistency` (phased implementation)

**Status:** REVISED rev 1 (2026-05-29) — manual review ✅, dual plan-review ✅ (findings folded in; see `PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md`), spikes resolved ✅ (see `spikes/RESULTS.md`). **Awaiting implementation trigger. Do not implement yet.**
**Companion:** `SPEC.md` (same dir). **Workflow:** plan → manual review → dual `plan-reviewer` → implement on trigger → dual `code-reviewer` + `plan-manager` coverage audit.

This is a **design plan**. Exact per-file task lists / TDD ordering are produced by `implementation-planner` after the implementation trigger.

---

## 0. Coordination / prerequisites

- **Tracking epic: [#890](https://github.com/FelixKrueger/Bismark/issues/890) `epic(methcons): port methylation_consistency to Rust`** — filed 2026-05-29, on the "Bismark Rust rewrite" board (Status=Todo, Phase=1-Now, Size=S), mirroring sibling epics (dedup #792, bedgraph #797, extractor #798). Sub-tasks tracked as a body checklist (matching the sibling convention; not broken into separate issues). Epic body archived at `EPIC.md` in this dir.
- Worktree `~/Github/Bismark-methcons` on `rust/methylation-consistency` (off `rust/iron-chancellor`) is set up. All work happens here. Never `git checkout`/`switch` in the shared `~/Github/Bismark`.
- Workspace member to add: `rust/Cargo.toml` `members = [..., "bismark-methylation-consistency"]` (Phase A).

---

## Phase A — Crate scaffold + CLI + SE end-to-end (MVP)

**Goal:** a working SE tool: read a Bismark BAM, classify each read, route to three **eager-opened** BAMs (empty buckets → valid empty BAMs, SPEC §5.2), write the report. This delivers the entire algorithm except PE pairing and CHH/edge polish.

### A1. Scaffold (mirror `bismark-dedup` layout)
- `rust/bismark-methylation-consistency/Cargo.toml` — package `bismark-methylation-consistency`, `[[bin]] name = "methylation_consistency_rs"`. Deps: `bismark-io = { path = "../bismark-io" }`, `clap` (=4.5.30, derive), `noodles-sam` (pin to match bismark-io), `thiserror`, `anyhow`. dev-deps: `assert_cmd`, `predicates`, `tempfile`, `bstr`, `noodles-core`. (No `mimalloc` in v1.0 — matches dedup.)
- Add member to `rust/Cargo.toml`.
- `src/lib.rs` — module decls + `pub fn version_string() -> String` (CARGO_PKG_VERSION, dedup pattern). `src/main.rs` — `ExitCode` handling (`0` ok, `1` on our error, clap → `2`), `--version` short-circuit, calls `run(cli)`.
- `src/error.rs` — `thiserror` enum wrapping `BismarkIoError`, I/O, mate-mismatch, threshold-validation, etc.

### A2. CLI (`src/cli.rs`)
- clap-derive `Cli` per SPEC §6: positional `files: Vec<PathBuf>`; flags with **underscore** long names; `disable_version_flag = true`; `-s`/`-p` `conflicts_with`; `min_count: u32` default 5 with `-m`/`--min-count`; `lower_threshold`/`upper_threshold` as `Option<i64>`.
- `validate()` → `ResolvedConfig { files, mode: ModeChoice {Force(SE|PE)|Auto}, chh, lower, upper, min_count, samtools_path, quiet }`. Apply defaults; range-check thresholds (emit Perl-matching error strings); error on zero files (Perl usage string).

### A3. Classification core (`src/classify.rs`) — **pure, exhaustively unit-tested**
- `struct Counts { meth: u32, unmeth: u32 }`; `count_xm(xm: &[u8], chh: bool) -> Counts` — single pass counting `Z`/`z` (or `H`/`h`).
- `enum Bucket { AllMeth, AllUnmeth, Mixed }` and `enum Routing { Discard, Skip, Route(Bucket) }`.
- `fn classify(c: Counts, min_count: u32, lower: i64, upper: i64) -> Routing`:
  1. `total = meth + unmeth`; `if total < min_count → Discard`.
  2. `if total == 0 → Skip` (only reachable when `min_count == 0`).
  3. `pct = format!("{:.1}", meth as f64 / total as f64 * 100.0).parse::<f64>().unwrap()` — **round then compare** (SPEC §2.5). **Pin this exact op-order** (`meth as f64 / total as f64 * 100.0`) — Spike 1 proved Perl/Rust parity holds *because* the f64 is computed identically.
  4. `if pct <= lower as f64 → AllUnmeth; else if pct >= upper as f64 → AllMeth; else Mixed`.
- **Unit tests:** boundary cases (`pct` exactly `lower`/`upper`; the 10.04→`"10.0"`→unmeth vs 10.05→`"10.1"`→mixed round-then-compare edge; all-meth, all-unmeth, mixed); **power-of-two tie cases that exercise round-half-to-even** (`1/16→6.25`, `1/8→12.5`, `7/8→87.5`, and crucially `1801/2000→90.05→"90.0"→all_meth`); `min_count` discard incl. `total==0` with `min_count=0`; CHH counting; empty XM; XM with `.`/`x`/`X`/`U`/`u`/`h`/`H` mixed (only the target context counts). *(This formalizes Spike 1 — already validated by Reviewer B.)*

### A4. Filenames (`src/filename.rs`)
- **⚠ Do NOT mirror `bismark-dedup/src/filename.rs` literally** (Reviewer B's #1 trap): dedup **basename-strips** (`s/.*\///`) and writes to CWD / `--output_dir`. methcons does **neither** — Perl keeps the **full input path** and writes outputs **adjacent to the input** (`$file_root = $file; $file_root =~ s/\.bam$//`, line 186). Use dedup only as a *structural* reference, not for the path logic.
- `fn output_root(input: &Path) -> String` — strip a **single trailing `.bam`** only (Perl `s/\.bam$//`); **keep the full directory prefix + the rest of the basename verbatim** (no basename strip, no extension chain).
- `fn bucket_path(root, chh, bucket) -> PathBuf` → `{root}{_CHH?}_all_meth.bam | _all_unmeth.bam | _mixed_meth.bam`.
- `fn report_path(root, chh) -> PathBuf` → `{root}{_CHH?}_consistency_report.txt`.
- Unit tests: nested dir input (`/a/b/c.bam` → `/a/b/c_all_meth.bam`, **same dir**); path with no `.bam`; `.bam` stripped only once (`x.bam.bam`→`x.bam`); a dotted basename (`s.sorted.bam`→`s.sorted`). **Explicitly assert the output directory == input directory.**

### A5. Report (`src/report.rs`)
- `struct Tally { all_meth, all_unmeth, mixed, discarded: u64 }` with `total()`.
- `fn render(tally, type_str: &str, lower, upper, min_count, chh) -> String` — **verbatim** templates from SPEC §5.1 (49-hyphen separator; exact internal spacing; `{:.2}` percentages; `N/A` when `total==0`). Copy the literal format strings out of the Perl source, not by hand.
- Same string drives both the file and (unless `--quiet`) STDERR.
- Unit tests: a known tally → exact expected string (incl. the `N/A`/`total==0` variant and the CHH label variant).

### A6. Logging (`src/logging.rs`) — mirror extractor `Logger`
- `Logger { quiet }` with `info(msg)` (STDERR unless quiet). Startup `Upper:`/`Lower:` banner; per-file `Now processing file:` line; the summary echo.

### A7. SE pipeline (`src/pipeline.rs`)
- `run(config) -> Result<()>`: for each input file → `process_file` dispatching SE/PE (PE wiring lands in Phase B).
- `process_file` (SE):
  1. **Open reader (BAM-only, no-sort-check):** `BamReader::without_sort_check(BufReader::new(File::open(path)?))` — Open Decision #1 **option (a)** (already-public ctor; **no `bismark-io` change**). SE is not sort-checked (Perl); `open_reader` is deliberately NOT used because it always rejects coordinate-sort.
  2. **Empty check (`bam_isEmpty`):** peek the first record; if none → skip the file (no outputs), log skip, continue to the next file.
  3. Header = `reader.header().clone()` — written **verbatim**, no `@PG` added (§4.9).
  4. **Eager-open all three `BamWriter`s** with the header (so empty buckets become valid empty BAMs — SPEC §5.2). Hold them + the tally in one struct.
  5. Stream records: `count_xm` → `classify` → tally++ → `write_record` to the chosen bucket (Discard/Skip write nothing).
     - **Missing-XM ⇒ graceful STOP** (§4.1): if `.records()` yields the reader's *missing-XM* `Err`, **stop the loop, keep the tally so far, and proceed to finalize** — do NOT propagate as fatal (Perl `last`, exit 0). Other reader `Err`s (truncation, malformed BGZF) remain fatal.
  6. **Finalize on ALL paths:** `finish()` each of the three writers (BGZF EOF) — including the graceful-stop path, and via a guard so an early error still finalizes (Reviewer B: an un-`finish()`ed writer leaves an EOF-less, undecodable BAM). Render + write the report; echo to STDERR (unless `--quiet`).

### A8. Phase-A tests (integration, `tests/`)
- Synthetic SE BAM (built via `bismark_io::BamWriter`) with reads engineered to land in each bucket + a discard + (min_count=0) a zero-call skip + **one bucket left empty**. Invoke the binary via `assert_cmd`; assert:
  - `*_consistency_report.txt` **byte-exact** (incl. no leading `\n`).
  - Each **populated** bucket BAM, read back via `bismark_io::open_reader`, has exactly the expected records **in order**, comparing fixed fields (qname, FLAG, RNAME, POS, MAPQ, CIGAR, RNEXT, PNEXT, TLEN, SEQ, QUAL) positionally **and tags as a set** (SPEC §7). This is *stronger* than dedup's qname-set test.
  - The **empty** bucket is a **valid empty BAM** (opens cleanly, yields **zero** records) — not 0-byte (§5.2).
  - Header round-trip: output `@HD`/`@SQ` + `@PG ID:Bismark` preserved; assert **no `@PG ID:samtools*`** is required (Rust adds none — §4.9).
  - **Output directory == input directory** (the A4 path-preservation guard).
  - Bucket counts == report.
- CLI validation tests (threshold ranges 0–49 / 51–100, `-s`+`-p` conflict, zero files → usage error).

**Phase A acceptance:** SE synthetic end-to-end byte-identical report + correct per-bucket records; all unit tests green; `cargo fmt`/`clippy` clean.

---

## Phase B — Paired-end support

**Goal:** PE detection + pairing + per-pair counting + the PE sort guard.

### B1. SE/PE resolution
- `resolve_mode(config, &header) -> Mode`: `Force` honored; else `detect_paired_from_header(&header)` → `Some(true)=PE`, `Some(false)=SE`, **`None`=SE** (SPEC §2.3 — *not* an error). Log the auto-detect outcome like Perl.

### B2. PE sort guard (PE only) — implement the *correct* guard
- After mode resolves to PE, inspect the (already-read) header's `@HD SO:` field; if `coordinate` → error. This is the guard Perl **intended**: its own `/^\@SO/` check (line 471) is **dead code** (§2.4), so this is an **intentional, output-equivalent fix** (decision 2026-05-29), not a faithful replica of the dead path. SE is **not** checked. Because the reader was opened no-sort-check (A7.1), this manual `@HD SO` check is the only sort gate. The Perl 100k-read adjacency pre-flight is dropped — the per-pair name `die` in B3 covers malformed adjacency.

### B3. PE pipeline (`process_file_pe`)
- Iterate records two-at-a-time: `r1 = next`, `r2 = next`.
  - If `r2` is `None` (odd trailing record) → **stop** (drop the dangling R1, uncounted; mirrors Perl's `$_ = <IN>` → undef → `last`). Do **not** raise dedup's `UnpairedFinalRecord` error.
  - **R2 missing-XM ⇒ graceful STOP** (§4.1): if reading R2 yields the reader's missing-XM `Err`, stop + finalize (R1's counts for this pair are discarded), exit 0 — same handling as A7.5.
  - **Mate check: exact qname equality** (`r1.inner().name() == r2.inner().name()`); else error ("READ IDs of R1 (…) and R2 (…) did not match …"). **Decision (both reviewers): manual exact-qname — NOT `BismarkPair::from_mates`** (which adds R1/R2 FLAG validation Perl lacks), and **no `/1`,`/2` suffix stripping** (the main-loop check is exact; only the *dropped* pre-flight stripped suffixes).
  - `counts = count_xm(r1.xm()) + count_xm(r2.xm())`; `classify`; on `Route(b)` write **both** r1 and r2 to bucket `b`; tally increments **once** (per pair).
- Report `type_str = "paired-end"`.

### B4. Phase-B tests
- Synthetic PE BAM (R1/R2 adjacent, FLAG 0x41/0x81) → each bucket holds R1+R2 pairs; report counts pairs; total = #pairs.
- PE auto-detect via a `@PG ID:Bismark` CL with `-1`/`-2`; SE via CL without; **no Bismark `@PG` ⇒ SE** test.
- Coordinate-sorted PE BAM → error; coordinate-sorted SE BAM → **no** error (processed).
- Mate-name-mismatch → error with the Perl-style message.
- Odd trailing R1 → dropped, not counted.

**Phase B acceptance:** PE synthetic byte-identical; SE/PE resolution table correct incl. `None→SE`; sort guard SE/PE-asymmetric.

---

## Phase C — CHH, edge cases & spikes

**Goal:** the experimental CHH path, the remaining pre-flight/edge behaviors, and the two verification spikes.

### C1. CHH context
- `--chh`: `count_xm(.., chh=true)` counts `H`/`h`; filenames gain `_CHH`; report label `Too few CHHs`; startup experimental warning (no `sleep`). Tests: CHH SE + PE synthetic; `_CHH` filenames; label.

### C2. Pre-flight & edge behaviors
- **Empty file** (`bam_isEmpty`): zero records → skip file, **no outputs**, log skip. Test with an empty/header-only BAM.
- **Missing-XM ⇒ graceful STOP (§4.1), NOT fatal:** a record (R1 or R2) lacking `XM:Z:` → stop *this file's* loop, **finalize its three BAMs + report with counts-so-far, exit 0**; in multi-file mode continue to the next file. **Test:** a BAM whose 2nd record has no XM → report tallies only record 1, and all three output BAMs are valid/decodable.
- **Truncation:** noodles surfaces truncated BGZF as an I/O error → map to a clear (fatal) error (text not byte-matched; SPEC §4). Best-effort test (truncated fixture).
- **Multiple input files:** loop independently; each gets its own outputs + report. Per-file disposition: empty → skip+continue; missing-XM → finalize-this-file+continue (graceful stop); **fatal** (truncation, mate-name mismatch, coordinate-sorted PE) → error out (nonzero exit).
- **`total == 0` report** → all `N/A` (reachable when the first record lacks XM and we stop, or empty-after-filter). Test the `N/A` rendering.
- **`min_count == 0`** zero-call skip path. Test.

### C3. Spike 1 — number-formatting parity — ✅ DONE (validated by Reviewer B, 2026-05-29)
- Result: Rust `{:.1}`/`{:.2}` is **decision-identical** to Perl `sprintf`, incl. power-of-two ties, given the pinned op-order. No code change beyond pinning the expression + adding the tie unit tests (**A3**). See `spikes/RESULTS.md`.

### C4. Spike 2 — empty-bucket BAM behavior — ✅ DONE (2026-05-29)
- Result: Perl emits **0-byte, unreadable** files for empty buckets; **decision = emit valid empty BAMs** (eager writers, **A7.4**). Bonus: discovered the samtools-`@PG` header divergence (**§4.9**). SPEC §5.2/§7/§8 updated. See `spikes/RESULTS.md`.

**Phase C acceptance:** CHH + all edge cases (empty file, missing-XM graceful stop, truncation, multi-file, `min_count==0`, `total==0`/`N/A`) covered by tests. (Both spikes already resolved — see C3/C4.)

---

## Phase D — Real-data byte-identity validation + polish

**Goal:** prove byte-identity on real Bismark BAMs (colossal) and finish docs/CI hooks.

### D1. Byte-identity harness (`tests/byte_identity_real_data.rs`, `#[ignore]`, dedup-style)
- Env-var-overridable data dir (default colossal `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/`). Skip gracefully if absent.
- For `10M_SE`, `10M_PE`, and a `--chh` run: invoke Perl `methylation_consistency` and `methylation_consistency_rs` with the **same path arg** (so any path-derived strings match); assert:
  - `*_consistency_report.txt` byte-equal.
  - Each **populated** bucket BAM record-level equal: read both back via `bismark_io::open_reader`; compare fixed fields (qname, flag, rname, pos, mapq, cigar, rnext, pnext, tlen, seq, qual) **in order** + **tags as a set**; compare header `@HD`/`@SQ` + `@PG ID:Bismark` only — **exclude `@PG ID:samtools*`** (Perl injects them, Rust doesn't — §4.9). Optional secondary cross-check: `samtools view` text-diff with samtools `@PG` lines filtered.
  - **Empty buckets:** assert both sides yield **zero records** (Perl 0-byte vs Rust valid-empty-BAM — §5.2); do not compare raw bytes.
  - Bucket counts == report.

### D2. Colossal run procedure (during implementation, ask before destructive ops)
- `dcli ssh colossal` (needs `dangerouslyDisableSandbox:true` on macOS — Keychain). Repo `~/Github/Bismark`; Rust `~/.cargo/bin`; activate bioinf by prepending `~/miniforge3/envs/bioinf/bin` to PATH (do **not** `mamba activate`). Fresh `--out`/work dir distinct from other sessions. Long jobs in detached tmux + poll a `~/*.status` marker. (methcons only needs the BAM — no genome.)

### D3. Polish
- `README.md` (crate) + rustdoc on public items (dedup/io style).
- `cargo fmt` / `clippy -D warnings` / full `cargo test`.
- Flag epic/CI wiring to the user (the CI matrix epic #796 may want a methcons diff-vs-Perl job).

**Phase D acceptance:** real-data report byte-identical and BAMs record-identical for SE, PE, CHH; docs done; clean clippy/fmt/test.

---

## Resolved decisions (was "Risks & open decisions"; all closed 2026-05-29 via dual plan-review + Spike 2)

1. **SE no-sort-check:** **RESOLVED → option (a)** — open BAM via the already-public `BamReader::without_sort_check`; apply `@HD SO:coordinate` rejection manually for PE (B2). **No `bismark-io` change.** *(Both reviewers; reverses the draft's option-(b) recommendation — `BamReader::without_sort_check` is already public.)*
2. **`BismarkRecord` strictness vs Perl's XM-only leniency:** **RESOLVED → keep `BismarkRecord`** (max reuse); document the strictness + the **missing-XM graceful-stop** handling (§4.1, A7.5/B3/C2).
3. **PE pairing:** **RESOLVED → manual exact-qname** (not `BismarkPair::from_mates`; no `/1`,`/2` stripping) (B3).
4. **`--samtools_path`:** **RESOLVED → accept-and-ignore** (noodles does I/O; no effect on output).
5. **Binary name:** **RESOLVED → `methylation_consistency_rs`** (dedup style).
6. **Empty-bucket file (Spike 2):** **RESOLVED → emit valid empty BAMs** via eager writers (§5.2, A7.4).
7. **(New) samtools `@PG` provenance:** **RESOLVED** — header written verbatim; byte-identity gate excludes `@PG ID:samtools*` (§4.9, §7). Vindicates dedup's qname-set test; corrects the draft's "compare all header lines" assumption.
8. **(New) Output directory:** **RESOLVED** — keep the full input path, write outputs **adjacent** to the input (A4); do **not** mirror dedup's basename-strip.
9. **(New) PE coordinate-sort guard:** **RESOLVED** — implement the *correct* `@HD SO:coordinate` guard (Perl's `/^\@SO/` is dead code); intentional output-equivalent fix (B2, §4.6).

**No open decisions remain.** Residual low-risk items to watch during implementation: header round-trip fidelity on real multi-`@SQ`/`@PG`/`@CO` headers (Phase D); `finish()`-on-all-paths writer finalization (A7.6).

## Estimated sequencing
A (largest — full algorithm, SE) → B (PE) → C (CHH + edge cases) → D (real-data gate + polish). **Both spikes are already resolved** (pre-implementation), so their outcomes are baked into A3 (formatting/ties) and A7/§5.2 (empty buckets) rather than being Phase-C tasks.

---

## Implementation notes (2026-05-29)

**Status: Phases A, B, C COMPLETE and verified. Phase D (large real-data gate on colossal) PENDING cluster access.**

Crate `bismark-methylation-consistency` (bin `methylation_consistency_rs`) added to the workspace. Modules: `error`, `classify`, `filename`, `report`, `logging`, `cli`, `pipeline`, `lib`, `main`. SE and PE share one unified `pipeline.rs` (the PE path is `stream_pe`, SE is `stream_se`); CHH and all edge behaviors are threaded through `ResolvedConfig`, so A/B/C landed together rather than as three separate code drops.

**Verification (all green):**
- `cargo test` — **48 lib unit tests + 16 integration tests** pass; release build (LTO) compiles; `clippy -D warnings` and `cargo fmt --check` clean.
- **Byte-identity proven against the real Perl script**: 3 automated `perl_vs_rust_*` integration tests run the actual `methylation_consistency` (auto-skip if `perl`/`samtools` absent) and assert the report is byte-identical and per-bucket records match, for **SE, PE, and CHH**. Plus a manual run on the Spike 2 fixture confirmed the report is byte-identical and records identical.
- Unit tests pin the load-bearing details: round-then-compare (`10.04→unmeth`, `10.05→mixed`), power-of-two ties, inclusive boundaries, `min_count==0` skip, the 49-hyphen separator, the verbatim report templates (incl. `N/A` and CHH-label variants), and the path-preservation (outputs adjacent to input) guard.

**Decisions realised as planned:** option (a) `BamReader::without_sort_check` (no `bismark-io` change); `BismarkRecord` kept (strict); manual exact-qname PE pairing; `--samtools_path` accept-and-ignore; binary `methylation_consistency_rs`; **empty buckets → valid empty BAMs** (eager-open all three writers); **PE `@HD SO:coordinate` guard** implemented (Perl's `/^@SO/` dead code); **missing-XM → graceful stop** (catch the reader's `MissingTag{"XM"}`, finalize partial output, exit 0); header written verbatim (no `@PG`); finalize-on-all-paths.

**Iteration log:**
- `#1` — `cargo test` (lib): 47/48 passed. The `lower_threshold_range` test fed `--lower_threshold -1` (space form), which **clap** treats as an unknown flag and rejects at parse (Perl's Getopt::Long would accept it and range-reject; both reject — CLI errors aren't byte-gated). Fixed the *test* to use the `=-1` form, which reaches the validate-layer range check. → 48/48.
- `#2` — `clippy -D warnings`: one `while_let_on_iterator` lint in `stream_se`. Changed `while let Some(item) = records.next()` → `for item in records.by_ref()`. Clean.
- `#3` — `cargo fmt`: wrapped long `assert_eq!` lines (mechanical). Added a CIGAR to synthetic test records so they're well-formed for the `samtools` round-trip in the Perl-vs-Rust tests.

**Deviation from the plan:** none material. The plan anticipated A/B/C as separate phases; they were implemented in one unified pipeline (the algorithm is small and the SE/PE/CHH paths share almost everything), which is simpler and fully covered by the phase-scoped tests.

**Phase D (deferred — needs colossal):** the synthetic + Perl-vs-Rust tests prove the byte-identity *logic*; the remaining step is the large `10M_SE` / `10M_PE` / `--chh` run on the cluster (`/weka/projects/bioinf/Data/Felix/bismark_benchmarks/`) per D1/D2. The `perl_vs_rust_*` harness in `tests/integration.rs` generalises to that data (point it at the real BAMs). Also pending: wiring a methcons diff-vs-Perl job into CI epic #796.

### Post-implementation audit (dual code-review + plan-manager, 2026-05-29)
Reports: `CODE_REVIEW_A.md`, `CODE_REVIEW_B.md`, `COVERAGE.md`. **plan-manager verdict: COMPLETE** (38 DONE / 2 PARTIAL / 0 MISSING / 1 cosmetic DEVIATED; D2 deferred). **Both code reviewers: no Critical/blocking issues** — faithful port. Findings addressed (iteration `#4`):
- **Finalizer (both reviewers, Medium):** `BucketWriters::finish` now attempts all three writers and returns the first error (was `?`-chaining, which skipped the rest on the first failure). Fixed `Result<(), BismarkIoError>` → `MethConsError` via `map_err`.
- **Strict-record divergence (Reviewer B, High):** behavior is intentional (rev-1 locked: keep `BismarkRecord`) but was untested and the SPEC wording was imprecise. SPEC §4.1 tightened to state the *fatal-abort* semantics exactly; added `malformed_record_missing_xr_is_fatal` test pinning it.
- **Coverage gaps (plan-manager PARTIAL):** added `multiple_input_files_each_get_own_outputs`, `pe_odd_trailing_r1_is_dropped_uncounted`, and `truncated_bam_is_fatal` tests. Integration tests now **20** (all green); lib unit tests 48.
- Low findings (u32 counters unreachable-overflow, 6-line SO-check duplication, usage-string rename, unmapped-only-input disposition) reviewed and accepted as-is / null on genuine Bismark data.

### Phase D — real-data byte-identity gate (colossal, 2026-05-29): ✅ PASSED
Ran Perl `methylation_consistency` vs the release Rust binary (built from this branch on `dockyard-colossal-0`) on the real 10M Bismark BAMs at `/weka/projects/bioinf/Data/Felix/bismark_benchmarks/`, each in an isolated work dir (input symlinked so outputs land in the work dir, never the shared data). Comparison: `_consistency_report.txt` byte-diff + per-bucket `samtools view` record-md5 (header omitted → samtools `@PG` provenance correctly excluded; empty buckets → empty record stream both sides).

| Case | Input | Records | Report | Buckets | Perl / Rust |
|---|---|---|---|---|---|
| SE | `directional_10M…bt2.bam` (621M) | 8,501,508 | IDENTICAL | all 3 record-md5 match | 16s / 6s |
| SE `--chh` | same | 8,501,508 | IDENTICAL | all 3 match (all_unmeth = 7.31M recs) | 40s / 40s |
| PE | `directional_10M…bt2_pe.bam` (1.3G) | 8,542,385 pairs | IDENTICAL | all 3 match | 32s / 15s |

The bucket md5s are **non-empty and match** Perl exactly → genuine record-level byte-identity at scale, confirming noodles preserves records faithfully (the reviewers' tag-order/round-trip concern is a non-issue on real data). Rust ~2–2.7× faster on SE/PE. **The byte-identity acceptance gate (SPEC §7) is met.** Still open before the epic is fully Done: PR #896 merge, and the `docs(methcons)` CHANGELOG/mkdocs entry + CI diff-vs-Perl job (#796).
