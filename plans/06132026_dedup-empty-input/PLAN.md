# PLAN — `deduplicate_bismark_rs` graceful zero-alignment handling

**Slug:** `06132026_dedup-empty-input` · **Crate:** `rust/bismark-dedup` (bin `deduplicate_bismark_rs`)
**Branch / worktree:** `rust/dedup-empty-input` @ `~/Github/Bismark-dedup` (off `origin/rust/iron-chancellor` `f1bcf42`)
**Status:** PLAN — awaiting manual review → dual plan-review → explicit implement trigger. **No source edited yet.**

---

## Goal

Make the Rust `deduplicate_bismark` **not crash** when handed a BAM/SAM with **zero alignment
records** (a header-only file, which the Bismark aligner emits when nothing aligns). Instead of
erroring out (current beta.5: `error: input file is empty` → **exit 1**), it must **gracefully
emit a valid header-only deduplicated output + a zero-count `deduplication_report.txt`, and exit
0**, across **every** invocation path (SE/PE × single/`--multiple` × `--parallel` × UMI modes).

The motivating failure: an nf-core/methylseq 4.2.0 run on `ghcr.io/felixkrueger/bismark:2.0.0-beta.5`
died at `BISMARK_DEDUPLICATE` for a sample where nothing aligned (`deduplicate_bismark -s --bam
<header-only.bam>`). The non-zero exit fails the Nextflow process and aborts the whole pipeline.

### Intentional divergence from Perl — read this first

This is a **deliberate, documented divergence from Perl v0.25.1**, *not* a byte-identity fix. The
original task framing ("Perl handles this gracefully, exit 0") was empirically **incorrect** — see
the Oracle table below. Perl **also dies** on zero-alignment input. Felix's directive (2026-06-13):

> "from a Bismark perspective it is fine to die if there are no reads, it is just that it crashes
> the entire methylseq nf-core pipeline."

So the goal is **pipeline robustness**, achieved by being *more graceful than Perl* on this one
degenerate input — the same class of deliberate methylseq drop-in fix as beta.3's aligner `--bam`
no-op, c2c `--genome` alias, and beta.5's extractor `--CX --bedGraph`. Byte-identity to Perl on
**non-empty** input is unchanged and stays gated (perl-oracle CI + existing tests).

---

## Context

### Where the code lives
- `rust/bismark-dedup/src/pipeline.rs` — the 8 public entry points + their empty-input guards.
- `rust/bismark-dedup/src/main.rs` — `process_one` / `process_multiple` dispatch; already prints
  `Output file is:`, writes the report, and exits 0 on `Ok`. **No behavioral change needed here.**
- `rust/bismark-dedup/src/report.rs` — `DedupReport::format()` **already** renders the `count == 0`
  case (currently as `0 (N/A%)`, unit-tested by `format_uses_na_when_count_is_zero`). This path is
  currently *dead in practice* because the guard fires before any report is built. The fix
  **activates** it, and **rev 1 changes the empty rendering from `N/A%` → `0.00%`** (dual-review
  recommendation; byte-identity-safe — see Open Q-2 resolution).
- `rust/bismark-dedup/src/error.rs` — `BismarkDedupError::EmptyInput(PathBuf)` (variant kept; doc
  updated — see below).

### How the crash happens (root cause, verified against source)
1. methylseq calls `deduplicate_bismark -s --bam <bam>` → `main::process_one` (single file, no
   `--parallel`, no UMI) → `pipeline::run_single` (`main.rs:175`).
2. `run_single` opens the reader, then **peeks the first record** (`pipeline.rs:313-316`):
   ```rust
   let mut records = reader.records().peekable();
   if records.peek().is_none() {
       return Err(BismarkDedupError::EmptyInput(input.to_path_buf()));
   }
   ```
   A header-only BAM yields no records → `peek() == None` → `EmptyInput` → `main` prints
   `error: …` and returns `ExitCode::from(1)` (`main.rs:39-42`). The `Output file is:` line printed
   at `main.rs:135` *before* the call matches the methylseq crash log exactly.
3. **`bismark_io` silently filters unmapped (FLAG 0x4) reads** (`rust/bismark-io/src/read.rs:7,577,
   585`). So an *all-unmapped* BAM also presents zero records to the reader and trips the same guard
   — but per Felix, **Bismark never emits FLAG 4 reads**, so the real-world trigger is the
   header-only case, not all-unmapped.

### The 8 entry points and their guards (the full surface to fix)
All 8 `pub fn`s in `pipeline.rs` guard the zero-records case identically (peek-None / first-record-None):

| Entry point | zero-records guard line(s) | empty-file-**list** guard (KEEP) |
|---|---|---|
| `run_single` (302) | 314-316 | — |
| `run_multiple` (347) | 408-414 (`first_record` match) | 354-356 |
| `run_single_parallel` (476) | 488-489 | — |
| `run_multiple_parallel` (516) | 560-567 | 523-524 |
| `run_single_umi` (810) | 823-824 | — |
| `run_multiple_umi` (848) | 896-899 | 856-857 |
| `run_single_parallel_umi` (950) | 963-964 | — |
| `run_multiple_parallel_umi` (989) | 1036-1039 | 997-998 |

The `inputs.is_empty()` guards (354/523/856/997) are a **different** case — an empty **file list**,
already blocked upstream by `NoInputFiles` in `Cli::validate()`. They are dead/defensive and **stay
erroring**; only the **zero-records** guards become graceful.

### Empirical reproduction (done 2026-06-13; local samtools 1.21 + perl 5.34 + cargo 1.95)
Fixtures: `header_only_se.bam` (0 records), `header_only_pe.bam` (0 records, PE `@PG`),
`all_unmapped.bam` (2 FLAG-4 records, no XR/XG → 0 mapped). Constructed via `printf` SAM + `samtools
view -bh`. Reproduction lives at `$TMPDIR/dedup_repro/` (ephemeral); commands recorded below.

**Current Rust beta.5 behavior** (`target/debug/deduplicate_bismark_rs`):

| Input | Command | Result |
|---|---|---|
| SE header-only | `-s --bam header_only_se.bam` | `error: input file is empty` → **exit 1**, no files |
| PE header-only | `-p --bam header_only_pe.bam` | same → exit 1, no files |
| SE all-unmapped | `-s --bam all_unmapped.bam` | same (FLAG4 filtered → 0 records) → exit 1, no files |

**Perl v0.25.1dev oracle** (`perl deduplicate_bismark …`):

| Input | Result | Exit | Files left |
|---|---|---|---|
| SE/PE header-only | `### File appears to be empty… ###` (`bam_isEmpty`, before any output) | **255** | none |
| SE all-unmapped | `Failed to determine read and genome conversion` (`deduplicate_bismark:317`, no XR/XG) | **29** | header-only BAM + **0-byte** report |

→ **Perl is never graceful here.** The fix is an intentional improvement, confirmed by Felix.

### Cascade check (the "verify the chain" deliverable — Felix Q2)
Built `bismark_methylation_extractor_rs` and ran it on `header_only_se.bam` (plain `-s` extraction):
it **already handles it gracefully** — "Processed 0 lines", deletes the empty per-context files,
prints a zero-count Cytosine report, **exits 0**. So fixing dedup should *unblock* the methylseq
chain rather than move the crash one step downstream. (Caveat: the **full** methylseq extractor
invocation — `--bedGraph --CX --cytosine_report --genome_folder …` — was not exercised on a
header-only BAM; flagged for validation, see V7.)

### Conventions to follow
- Tests build BAM fixtures via `bismark_io::BamWriter` + noodles (`integration_dedup.rs::write_bam`,
  `synth_header`, `build_record`, `ot_pair`) — **no samtools dependency in tests** (keep it that way).
- Pre-push gate: `cargo test -p bismark-dedup`, `cargo clippy -p bismark-dedup --all-targets -- -D
  warnings`, **`cargo fmt -p bismark-dedup -- --check`** (fmt is a separate CI job). dedup does not
  need `--test-threads=2`.
- Build from `~/Github/Bismark-dedup/rust/` (the workspace root; the repo root has no Cargo.toml).
  cargo/git-write in this sibling worktree need `dangerouslyDisableSandbox` from the Bash tool.

---

## Behavior (target, after fix)

For **every** entry point, when the *zero-records* condition is reached:

1. **Do not error.** Proceed to open the output writer with the (cloned) input header.
2. The output is a **valid header-only deduplicated file** (BAM/SAM/CRAM matching input format) with
   a correct trailer (BGZF EOF for BAM). The header is the input file's header verbatim (`run_*`
   already clones `reader.header()`).
3. The dedup stream loop runs over the empty record iterator → a **no-op** (writes nothing). `finish()`
   flushes the trailer.
4. Return a `DedupReport` with `count = 0, removed = 0, n_positions = 0` → `main` writes the
   `*.deduplication_report.txt`, echoes it to stderr, and **exits 0**. **rev 1:** the empty-count
   percentages render as `0.00%` (not `N/A%`) — see Open Q-2. Concretely: `Total number duplicated
   alignments removed:\t0 (0.00%)` and `Total count of deduplicated leftover sequences: 0 (0.00% of
   total)`.
5. Emit one informational **stderr** line before proceeding (recommended, see Open Q-1), e.g.
   `Input contains no alignments — writing an empty deduplicated file and a zero-count report.`
   This signals the graceful path is intentional and aids debugging. Not required for methylseq
   correctness (it keys only on exit code + the output files).

### Edge cases (explicit)
- **Header-only single file (the real case):** SE and PE → header-only output + `count=0` report +
  exit 0.
- **All-unmapped single file (defensive only — Bismark never produces it):** `bismark_io` filters
  FLAG 4 → zero records → identical graceful output (`count=0`). Output BAM is header-only (unmapped
  records are *not* written — they were filtered on read). Documented divergence from Perl's
  die-at-317; acceptable per Felix.
- **`--multiple`, file1 empty, file2 non-empty:** **must NOT error.** Open writer with file1's
  header, stream file1 (empty), then stream file2 → output contains file2's deduplicated records;
  report `count` = records actually analysed across all files (e.g. 1 pair). The current
  `iter::once(first).chain(rest)` peek-stash exists *only* to avoid a leftover header-only BAM on
  the now-removed error path; with graceful handling the stash is unnecessary and the loop simply
  streams every reader in order.
- **`--multiple`, all files empty:** header-only output + `count=0` report + exit 0.
- **Empty file *list* (`inputs.is_empty()`):** unchanged — stays `EmptyInput` (unreachable in
  practice; `NoInputFiles` guards it upstream).
- **PE empty input:** `stream_pe` over an empty iterator never enters the loop → no
  `UnpairedFinalRecord`, no panic. Confirmed by reading `stream_pe` (`pipeline.rs:251-285`).

---

## Signature

**No public signature changes.** The 8 `pub fn run_*` keep their signatures and `Result<DedupReport,
BismarkDedupError>` return type. Internally they stop returning `Err(EmptyInput)` on the zero-records
path and instead fall through to the existing writer/stream/finish/report sequence.

`BismarkDedupError::EmptyInput` is **retained** (still used by the defensive `inputs.is_empty()`
guards) but its doc comment is corrected:
```rust
/// The `--multiple` input file LIST was empty (defensive; normally blocked
/// upstream by `NoInputFiles`). NOTE: a file with zero *alignment records*
/// is NOT an error — it is handled gracefully (header-only output + zero-count
/// report + exit 0) to keep nf-core/methylseq from crashing on no-alignment
/// samples. See plans/06132026_dedup-empty-input/PLAN.md.
#[error("input file is empty: {0}")]
EmptyInput(PathBuf),
```

---

## Implementation outline

> TDD-friendly ordering: invert the two existing tests + add new ones first (they will fail against
> current code), then make the production change, then re-run.

### A. Production change — `pipeline.rs` (the 8 entry points)

For each of the **single-file** entry points (`run_single`, `run_single_parallel`, `run_single_umi`,
`run_single_parallel_umi`): **delete** the peek-None `EmptyInput` guard. The `Peekable` was used
only for that guard — replace with a plain `reader.records()` iterator fed straight into
`stream_se`/`stream_pe`. The empty case then naturally produces header-only output + a `count=0`
report. Emit the informational stderr line (Open Q-1) right before opening the writer (or keep it in
`main` — see A.3).

For each of the **`--multiple`** entry points (`run_multiple`, `run_multiple_parallel`,
`run_multiple_umi`, `run_multiple_parallel_umi`): **remove only the `first_record` peek-stash + its
`None => Err(EmptyInput)` arm.** Open the writer with `headers[0]` first (the leftover-BAM concern
that motivated the stash is moot now that we *want* the output file), stream the first reader's
records directly with `refid_tables[0]`, and **keep the existing pop-first + subsequent-files loop
structure (`let i = i_zero_based + 1`, `refid_tables[i]`) UNCHANGED** — see A.3 for why the `+1` must
not be touched. Keep the `inputs.is_empty()` guard and the format/`@SQ` validation at the top.

1. **`run_single`** (302): remove lines 313-316 (`.peekable()` + the guard). Keep the
   `build_chr_intern`/`build_refid_table`/`open_writer`/`stream_*`/`finish`/`into_report` sequence
   unchanged; just feed it `reader.records()` directly.
2. **`run_single_parallel`** (476), **`run_single_umi`** (810), **`run_single_parallel_umi`** (950):
   same surgery at 488-489 / 823-824 / 963-964. Verify each one's surrounding reader/writer setup
   (the UMI variants use `UmiDedupState`/`UmiDedupKey`; the parallel ones use `ThreadedBamWriter`)
   still flows correctly with an empty iterator — all three just skip the loop body.
3. **`run_multiple`** (347): keep `inputs.is_empty()` (354), `len == 1 → run_single` (357), **and
   the cross-file format + `@SQ`-consistency validation (362-381) which already runs BEFORE any
   record peek — do NOT move it; it must still fire on all-empty input** (rev 1, B-#3). Then:
   **(a)** open the writer with `headers[0].clone()`; **(b)** stream the first reader's records
   directly — `stream_*(first_reader.records(), &refid_tables[0], …)` — replacing the `first_record`
   peek-stash block (404-416) and its `EmptyInput` arm (413) (empty first reader = no-op); **(c)
   leave the subsequent-files loop (431-441) UNCHANGED**, including `let i = i_zero_based + 1` and
   `refid_tables[i]`. ⚠️ **rev 1 (A-I1 + B-#4): the `+1` is correct ONLY because the first reader is
   still consumed separately via `readers_iter.next()`. Do NOT restructure to "iterate all readers
   from index 0" — that would require dropping the `+1`, and a mismatch silently corrupts chr-id
   translation on reordered-`@SQ` multi-file runs.** Minimal change = remove the peek-stash error
   path only; keep the pop-first + `skip(1)`/`+1` indexing intact.
4. **`run_multiple_parallel`** (516), **`run_multiple_umi`** (848), **`run_multiple_parallel_umi`**
   (989): mirror the `run_multiple` change at 560-567 / 896-899 / 1036-1039, preserving each one's
   `let i = i_zero_based + 1` loop. ⚠️ **rev 1 (A-I2): the UMI + parallel variants wrap streaming in
   a `final_result` and call `cleanup_partial_output_on_err(output, final_result)` (844/945/984/1084)
   to delete the partial output on a mid-stream error. KEEP that wrapper.** Removing the empty
   early-return is safe: the empty path now returns `Ok`, so cleanup is not triggered and the
   header-only output is correctly retained; genuine record-N failures still clean up as before.
5. **Doc/comment cleanup:** update `run_single`'s doc (296: "If `None`, return `EmptyInput`" → "empty
   input → header-only output + zero-count report"); the `run_multiple` rationale comment (388-403)
   and the `run_single_parallel`'s comment at 619; and `main.rs:295` (`// Empty input — let
   downstream EmptyInput fire.` → `// Empty input — downstream handles it gracefully (zero-count
   report).`).
6. **`error.rs`:** update the `EmptyInput` doc comment (see Signature). Keep the variant.
6b. **`report.rs` (rev 1, Open Q-2):** change the `count == 0` branch in `DedupReport::format()` from
   `("N/A", "N/A")` to `("0.00", "0.00")` so the empty report renders `0 (0.00%)` /
   `0 (0.00% of total)`. Do NOT compute `0/0` (yields `NaN`); hardcode the `"0.00"` strings for the
   `count == 0` arm. Update the unit test `format_uses_na_when_count_is_zero` → rename to
   `format_renders_zero_pct_when_count_is_zero` and assert the `0.00%` bytes. This is byte-identity-
   safe (Perl never emits a zero-count report — it dies — so there is no Perl oracle to match) and
   removes any risk of a downstream parser choking on the non-numeric `N/A`.

### B. Test changes — `rust/bismark-dedup/tests/integration_dedup.rs`

7. **Invert `empty_input_errors_before_any_output_file_is_created`** (562) →
   `empty_input_produces_header_only_output_and_zero_count_report`. Assert: `.success()`; the output
   BAM **exists** and is a readable header-only BAM (0 records — verify by opening with
   `bismark_io::open_reader` and asserting `records().next().is_none()`, header `@SQ` preserved); the
   report **exists** with content matching the `count=0` rendering
   (`Total number of alignments analysed in <path>:\t0`, `0 (0.00%)`, `0 different position(s)`,
   `0 (0.00% of total)`). Test the SE path too (the current test uses `--paired`).
8. **Invert `multiple_mode_empty_file1_leaves_no_output_files_behind`** (645) →
   `multiple_mode_empty_file1_still_processes_file2`. Same fixtures (file1 header-only, file2 = one
   `ot_pair`). Assert: `.success()`; output BAM exists and contains the file2 pair (2 records);
   report `count = 1` pair (`leftover = 1`).

### C. New regression tests (`integration_dedup.rs` and/or a focused file)

9. Add graceful-path coverage for the remaining single-file modes so the fix is proven across the
   surface (cheap, fixture-reuse): header-only via **`--parallel 2`** (BAM) and via **UMI**
   (`--barcode`/`--bclconvert` with a UMI-shaped qname header — header-only so no qname needed) →
   each `.success()` + header-only output + `count=0` report.
10. Add an **all-unmapped** defensive test: build a BAM with 1-2 FLAG-4 records (reuse
    `build_record` with `flag |= 0x4`), run `-s` → `.success()` + header-only output + `count=0`
    report (documents the FLAG-4-filtered-to-empty divergence from Perl).
11. Add a **`--multiple` all-files-empty** test → `.success()` + header-only output + `count=0`.
11b. **rev 1 (B-#3): reordered-`@SQ` empty-file1 test** — file1 header-only with `@SQ` order
    `[chr1, chr2]`; file2 (non-empty) with `@SQ` order `[chr2, chr1]` and a record on `chr2`. Run
    `--multiple` → `.success()`; assert file2's record lands under the correct chromosome (proves the
    `refid_tables[i]` indexing survives the empty-file1 refactor — the A-I1/B-#4 hazard).
11c. **rev 1 (B-#3): `--multiple` validation still fires on empty** — confirm the existing
    `multiple_mode_rejects_different_sq_name_sets_across_inputs` and the mixed-format test still
    `.failure()` even when the offending file is header-only (the format/`@SQ` checks run before any
    record peek, so emptiness must not bypass them). Add a header-only variant of one of these if not
    already covered.

### D. Conformance suite (Felix Q2 — "track the chain")

12. Add an **empty-input row** to `rust/bismark-dedup/tests/methylseq_conformance.rs`: a Tier-3-style
    case that actually *runs* the binary on a header-only BAM via `assert_cmd` and asserts
    `.success()` + output files exist (the existing rows are parse/validate-only and would not catch
    this class). Add a top-of-file note that the empty-input class is a methylseq drop-in concern,
    cross-referencing this plan and the cascade finding (extractor already graceful).

### E. Docs / provenance

13. Add a one-line entry to `rust/README.md` Milestones (and the dedup row note if applicable):
    "deduplicate_bismark: zero-alignment input now emits an empty deduplicated BAM + zero-count
    report + exit 0 (methylseq drop-in robustness; intentional divergence from Perl, which dies)."
14. Leave a short code comment at the (now-removed) guard sites pointing at this plan so future
    readers understand the intentional divergence.

### F. Release (post-merge, separate, on Felix's explicit go — NOT part of the implement step)

15. Cut **beta.6**: bump `rust/VERSION` `2.0.0-beta.5` → `2.0.0-beta.6` + the 3 mirror literals
    (justfile `suite_tag`, rust/README Installing + bump-note, docs/installation.md); PR to
    iron-chancellor; `gh workflow run release.yml --ref rust/iron-chancellor -f dry_run=true` then
    `=false` (the workflow OWNS the tag). Then bump the methylseq pin `:2.0.0-beta.5` →
    `:2.0.0-beta.6` (`FelixKrueger/methylseq@bismark-rust-profile` `nextflow.config:263`, the only
    version-literal spot). Watch the 3 known real-run-only publish-path bugs.

---

## Efficiency

- Removing the `peek()`/`first_record` stash **removes** a tiny amount of work; the streaming
  loop is unchanged. Complexity stays `O(records)` time, `O(distinct positions)` memory.
- Empty input is now `O(1)` work after header clone + writer open/finish — strictly cheaper than the
  error path was perceived to be, and produces a tiny (header-only) output file.

## Integration

- **Read/written:** reads the input BAM/SAM/CRAM header + (zero) records; writes a header-only
  `*.deduplicated.bam` (or `.sam`/`.sam.gz`/`.cram`) + a `*.deduplication_report.txt`. Same paths
  `derive_output_paths` already computes.
- **`main.rs` unchanged in behavior:** `process_one`/`process_multiple` already write the report and
  return `Ok` → exit 0. They need no change beyond the optional comment at 295.
- **Downstream (methylseq):** `BISMARK_DEDUPLICATE` now exits 0 + emits a valid (empty) BAM + report
  → Nextflow proceeds. The next module, `BISMARK_METHYLATIONEXTRACTOR` (Rust extractor), **already**
  handles a header-only BAM gracefully (verified, exit 0). Net: the no-alignment sample flows
  through instead of aborting the run.
- **Existing tests inverted:** the two tests in §B currently assert the *old* error behavior and
  WILL fail after the fix — they must be updated in the same change (not deleted).
- **perl-oracle CI gate:** unaffected — it compares non-empty real-data outputs; this change touches
  only the zero-records path. All non-empty dedup tests stay green.

## Assumptions

1. **Graceful is intentional** and supersedes byte-identity-to-Perl *on zero-alignment input only*
   (Felix-confirmed 2026-06-13). All non-empty behavior stays byte-identical.
2. **The real-world trigger is a header-only BAM** (0 records); Bismark never emits FLAG-4 reads
   (Felix-confirmed). All-unmapped is covered only as a defensive test.
3. The output **header is the input header verbatim** (already how `run_*` clone it). A header-only
   BAM with the original `@HD`/`@SQ`/`@PG` lines is a valid, downstream-readable file.
4. A `count=0` report rendering `0 (0.00%)` (rev 1) is acceptable methylseq-side. MultiQC's Bismark
   dedup module parses the integer COUNTS (not the parenthetical percent), so `0.00%` is safe; the
   switch from `N/A%` removes any non-numeric-token risk. Confirmed as a hard gate by V7b.
5. `--multiple` with a mix of empty + non-empty files should process the non-empty files (not error);
   counts reflect records actually analysed.
6. CRAM output of a header-only file works the same as BAM via `open_writer` (CRAM path is rarely
   used in methylseq; covered defensively if cheap, else noted).

## Validation

| # | Verify | How | Expected |
|---|---|---|---|
| V1 | SE header-only is graceful | Build header-only BAM; `deduplicate_bismark_rs -s --bam x.bam` | exit 0; `x.deduplicated.bam` exists, readable, 0 records, `@SQ` preserved; `x.deduplication_report.txt` shows `…analysed…:\t0` + `0 (0.00%)` |
| V2 | PE header-only is graceful | same with `-p` on PE-`@PG` header-only BAM | exit 0; header-only output + zero-count report; no `UnpairedFinalRecord` |
| V3 | `--multiple` empty file1 + non-empty file2 | 2-file `--multiple` run | exit 0; output has file2's records; report `count` = file2's analysed count |
| V4 | All single-file modes graceful | header-only via `--parallel 2` and via a UMI flag | each exit 0 + header-only output + zero report |
| V5 | All-unmapped defensive | FLAG-4-only BAM, `-s` | exit 0 + header-only output + `count=0` (documents divergence) |
| V6 | Non-empty path unchanged | `cargo test -p bismark-dedup` (full suite incl. byte-identity tests) | all green; non-empty reports/counts unchanged |
| V7a | **Cascade — full extractor on empty BAM** (HARD **merge** gate; rev 1) | Run the full methylseq extractor command (`--bedGraph --CX --cytosine_report --genome_folder <genome> -s`) on a header-only/dedup'd-empty BAM | extractor exits 0; emits empty/zero outputs without error (proves dedup→extract doesn't just move the crash downstream) |
| V7b | **Real methylseq + MultiQC end-to-end** (HARD **pin-bump** gate; rev 1) | Run nf-core/methylseq on `bismark:2.0.0-beta.6` with a no-alignment sample (or inject a header-only BAM); let MultiQC ingest the zero-count dedup report | pipeline completes; MultiQC parses the `0 (0.00%)` dedup report without error. If MultiQC errors, that blocks the pin bump (not the merge). |
| V8 | Lint/format gates | `cargo clippy -p bismark-dedup --all-targets -- -D warnings` + `cargo fmt -p bismark-dedup -- --check` | clean |
| V9 | Conformance row | the new empty-input row in `methylseq_conformance.rs` | runs binary on header-only BAM → success |
| V10 | **`--multiple` reordered-`@SQ` empty-file1** (rev 1, A-I1/B-#4) | empty file1 `[chr1,chr2]` + non-empty file2 `[chr2,chr1]` with a `chr2` record, `--multiple` | exit 0; file2's record maps to the correct chromosome (no refid-table off-by-one) |

> The Perl-oracle "byte-identity on empty input" from the original task brief is **dropped** as a
> validation: Perl dies (exit 255/29), so byte-identity would *contradict* the graceful goal. The
> oracle for the report format is the crate's own already-tested `count=0` rendering, not Perl.

## Questions or ambiguities

- **(Resolved — Critical)** Graceful vs. match-Perl-die, and scope (all paths vs methylseq-only):
  resolved via AskUserQuestion 2026-06-13 → **graceful, all paths**; **fix dedup now + verify/track
  the chain**. FLAG-4 not-the-cause confirmed by Felix.
- **(Open Q-1)** Exact informational stderr wording on the empty path, and whether to emit it from
  `pipeline` (per-entry) or once in `main`. Proposed: emit once in `main::process_one`/
  `process_multiple` after `run_*` returns a `count==0` report, to avoid duplicating across 8
  functions. *Assumption taken:* include a single concise line; final wording at implement time.
- **(Resolved — rev 1, Open Q-2)** `N/A%` vs `0.00%` for the empty report. **Resolved → render
  `0.00%`** (both plan reviewers' recommendation). Rationale: byte-identity-safe (Perl emits no
  zero-count report — it dies — so there is no oracle to match), and it eliminates any risk of a
  downstream parser tripping on the non-numeric `N/A` token. Implemented in step 6b; verified as a
  hard gate by V7b. **Felix can veto this back to `N/A%`** — it's the one pipeline-facing output
  change in this fix.
- **(Open Q-3)** Keep `EmptyInput` variant for the defensive `inputs.is_empty()` case, or replace
  those with `NoInputFiles`? *Assumption taken:* keep `EmptyInput` (minimal churn), update its doc.

## Self-Review

- **Efficiency:** change only removes work; no new allocations on the hot path. ✓
- **Logic:** the empty iterator flows through `stream_se`/`stream_pe` as a no-op; `finish()` writes a
  valid trailer; `into_report()` yields `count=0`. Verified `stream_pe` won't raise
  `UnpairedFinalRecord` on empty (loop never entered). ✓
- **Edge cases:** header-only (SE/PE), all-unmapped (FLAG-4-filtered), `--multiple` empty-file1,
  all-files-empty, empty-file-*list* (unchanged), UMI + parallel modes — all enumerated. ✓
- **Integration:** the **two existing tests assert the old error behavior** and must be inverted in
  the same change — explicitly listed (a silent break would be the most likely miss). perl-oracle CI
  + non-empty tests unaffected. ✓
- **Cascade:** verified the extractor is already graceful on header-only, so the dedup fix should
  actually unblock methylseq (the one un-verified link is the *full* extractor invocation — now a
  hard gate, V7a). ✓
- **`--multiple` indexing (rev 1):** the `refid_tables[i]` `+1` off-by-one hazard is avoided by NOT
  restructuring the loop — only the peek-stash error path is removed; the pop-first + `skip(1)`/`+1`
  indexing is preserved. Guarded by V10. ✓
- **Remaining risks:** (a) MultiQC ingestion of the zero-count report — de-risked by switching to
  `0.00%` (rev 1) + the hard V7b gate; (b) CRAM header-only output (low-traffic in methylseq;
  defensive coverage if cheap). Both low and validation-gated.

## Implementation Notes (2026-06-13, rev 1 implemented)

**Implemented on branch `rust/dedup-empty-input`.** All quality gates green:
`cargo test -p bismark-dedup` (86 lib + 39 integ + 2 conformance + 7 sanity + 1 doctest, **0
failed**; 9 real-data byte-identity tests `ignored` as usual — run on oxy), `cargo clippy -p
bismark-dedup --all-targets -- -D warnings` clean, `cargo fmt -p bismark-dedup -- --check` clean.

**Production changes (3 files):**
- `pipeline.rs` — removed the zero-records guard at all **8** entry points. Single-file variants now
  feed `reader.records()` / `records_with_umi(...)` straight into the stream (no `.peekable()`/peek).
  `--multiple` variants open the writer with `headers[0]` first, stream the first reader directly,
  and **keep the pop-first + `i = i_zero_based + 1` / `refid_tables[i]` loop unchanged** (no off-by-one).
  UMI/parallel `--multiple` variants keep their `(|| {...})()` closure + `cleanup_partial_output_on_err`
  wrapper intact (empty → `Ok`, output retained; genuine mid-stream errors still clean up). Doc
  comments updated; `EmptyInput` variant retained for the defensive `inputs.is_empty()` (empty file
  *list*) case.
- `report.rs` — `count == 0` renders `0.00%` (was `N/A%`); unit test renamed
  `format_uses_na_when_count_is_zero` → `format_renders_zero_pct_when_count_is_zero`.
- `main.rs` — both `process_one`/`process_multiple` emit one informational stderr line when
  `report.count() == 0`; the bclconvert empty-peek comment de-staled.

**Test changes:**
- `integration_dedup.rs` — inverted the two stale tests; added `empty_input_se_is_graceful`,
  `empty_input_pe_is_graceful`, `empty_input_parallel_is_graceful`, `empty_input_umi_is_graceful`,
  `all_unmapped_input_is_graceful`, `multiple_mode_empty_file1_still_processes_file2`,
  `multiple_mode_all_files_empty_is_graceful`, `multiple_mode_sq_mismatch_fires_when_file1_is_empty`
  (B-#3). Two shared helpers: `assert_header_only_output`, `assert_zero_count_report`.
- `methylseq_conformance.rs` — added Tier-3 `methylseq_deduplicate_empty_input_does_not_crash_pipeline`
  (`-s` and `-p`, runs the binary on a header-only BAM, asserts exit 0 + output files) + top-of-file note.

**Manual reproduction (rebuilt binary):** SE/PE header-only + all-unmapped → exit 0, valid
header-only BAM (0 records, `@SQ` preserved), report renders `0 (0.00%)`.

**DEVIATION from rev 1 (V10 / test 11b — reordered-`@SQ` empty-file1 off-by-one guard) — OMITTED.**
On analysis the test **cannot distinguish correct from off-by-one indexing** in the empty-file1
scenario: with file1 empty there are no cross-file dedup interactions, so file2's records only dedup
against each other, and *any* bijective `refid_table` (correct `[1]` or off-by-one `[0]`) maps file2's
own refids consistently → identical output. The off-by-one is only observable when **two non-empty
files with reordered `@SQ` dedup against each other** — a pre-existing `--multiple` path unrelated to
the empty-input fix (and one that surfaces a separate written-record-refid question out of scope
here). Since the implementation **structurally preserves** the `+1` pop-first indexing (verified by
`multiple_mode_empty_file1_still_processes_file2` + the B-#3 validation test + code comments), the
off-by-one cannot occur. Net: V10's intent (guard the indexing) is met structurally; the specific
adversarial test as specified is not constructible. Flagged for the code-review/plan-manager pass.

**Deferred to the methylseq/oxy environment (need a genome):** V7a (full extractor
`--bedGraph --CX --cytosine_report --genome_folder` on a header-only BAM — merge gate) and V7b (real
methylseq + MultiQC end-to-end — pin-bump gate). The **plain** extractor was verified graceful on a
header-only BAM locally (exit 0). One local Bash attempt at a V7a-partial (`--bedGraph --CX`, no
genome) was declined; re-run in the methylseq env before the pin bump.

## Revision History
- **rev 0 (2026-06-13):** initial plan. Root cause + Perl oracle + cascade established empirically
  (local samtools/perl/cargo). Two critical decisions pre-resolved with Felix.
- **rev 1 (2026-06-13):** folded dual plan-review (A + B, both APPROVE-WITH-CHANGES, 0 Critical / 4
  Important each). Changes: (1) empty report renders `0.00%` not `N/A%` (Open Q-2 resolved; step 6b +
  test rename); (2) `--multiple` refactor pinned to remove ONLY the peek-stash error path, preserving
  the `refid_tables[i]` `+1` indexing (A-I1/B-#4) + new V10 reordered-`@SQ` test; (3) preserve the
  `cleanup_partial_output_on_err` wrapper in UMI/parallel `--multiple` variants (A-I2); (4) `--multiple`
  format/`@SQ` validation must still fire on empty input + tests 11b/11c (B-#3); (5) V7 split into hard
  V7a (merge gate, full extractor on empty BAM) + V7b (pin-bump gate, real methylseq + MultiQC).
  Reviews: `PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`.
