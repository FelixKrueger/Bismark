# PLAN — bam2nuc test-gap closure (4 robustness cells)

**Feature:** `rust/bismark-bam2nuc` (epic #921, PR #922)
**Parent artifacts:** `SPEC.md` (rev 1), `PLAN.md` (rev 1), `COVERAGE.md` (verdict COMPLETE)
**Mode:** TDD (characterization/regression — code already exists; tests pin behaviour)
**Status:** awaiting manual review → dual plan-review → explicit implement trigger
**Date:** 2026-05-31

---

## Goal

Close the 4 optional test gaps recorded in the bam2nuc session handoff (§2). All four
exercise **robustness/edge branches that the byte-identity gate does not touch** — none
is a byte-identity risk. The aim is regression coverage on:

1. `--version` / `-V` **end-to-end** (binary spawn), not just `version_string()` unit.
2. `build_chr_name_table` **non-ASCII `@SQ`** error branch (`count.rs:49`, `NonAsciiChromosomeName`).
3. **`SePeUndetermined`** end-to-end — a BAM with no Bismark `@PG` (`count.rs:152`).
4. A **committed** coordinate-sorted-BAM byte-identity golden (only run live on oxy so far).

Outcome: 3 new behavioural/e2e tests + 1 unit test + 1 new byte-identity golden cell, all
hermetic in CI, with the two new BAM fixtures + the new golden minted reproducibly by
`tests/data/generate_goldens.sh`.

## Context — placement & references

- **Test conventions (already established):**
  - `tests/golden.rs` — drives the binary via `assert_cmd::Command::cargo_bin("bam2nuc_rs")`
    (helper `bin()`), copies a committed genome fixture into a `tempfile::TempDir`
    (`copy_genome`), and either byte-compares against `goldens/*.golden`
    (`assert_bytes_eq`) or asserts exit code + `stderr` for behavioural cells. The
    "Behavioral / exit-code cells" section already holds `sam_input_is_rejected`,
    `cram_input_is_rejected`, `all_indel_sample_zerodivision_exits_one`, etc.
  - `src/count.rs` `#[cfg(test)]` — pure-helper + synthetic-`RecordBuf` unit tests
    (`use noodles_sam::header::record::value::{Map, map::ReferenceSequence}` pattern
    cribbed from `bismark-dedup/src/pipeline.rs:1103 header_with_chrs`).
  - `tests/sanity.rs` — unit-level: calls `version_string()` directly (does **not** spawn
    the binary; no `assert_cmd`). This is exactly why gap #1 (the e2e path) is uncovered.
  - `tests/data/generate_goldens.sh` — single source of provenance for every BAM fixture
    and golden. Builds SAM→BAM via `samtools view -b`, runs the **real Perl `bam2nuc`
    v0.25.1** under `LC_ALL=C`, harvests goldens. `rm -rf "$GOLD"` + rebuild ⇒ a re-run
    regenerates *all* fixtures/goldens. Behavioural-only fixtures (e.g. `all_indel.bam`)
    are built here too but have **no** golden.
- **Dependencies:** all dev-deps needed are already present in `bismark-bam2nuc/Cargo.toml`
  — `assert_cmd`, `predicates`, `tempfile`, `bstr` (`=1.10.0`), `noodles-core`; plus the
  regular deps `noodles-sam`. **No Cargo.toml change required.**
- **Host requirement (impl-time only):** minting the gap #3/#4 fixtures + the gap #4 golden
  re-runs `generate_goldens.sh`, which needs Perl + `samtools` (dev box: Perl 5.34,
  samtools 1.21 at `/opt/homebrew/bin/samtools` — the exact toolchain that produced the
  current goldens). CI itself stays hermetic (no Perl/samtools).
- **Error wording references** (`src/error.rs`): `NonAsciiChromosomeName` →
  `"non-ASCII chromosome name in BAM header: {name:?}"`; `SePeUndetermined` →
  `"failed to determine single-end vs paired-end from the BAM @PG header"`.
- **SE/PE detection** (`bismark-io/src/read.rs:649 detect_paired_from_header`): serialises
  the header to SAM text and returns `None` unless some line both `starts_with("@PG")` and
  `contains("ID:Bismark")`. (bismark-io already unit-tests the `None` case; bam2nuc's gap
  is the *end-to-end* surfacing of that `None` as `SePeUndetermined`.)

## Behavior — the 4 cells

### Cell 1 — `--version` / `-V` e2e (behavioural)
- Spawn `bam2nuc_rs --version` ⇒ exit 0, `stdout` contains `"bam2nuc_rs "` **and**
  `std::env::consts::OS`.
- Spawn `bam2nuc_rs -V` ⇒ exit 0, `stdout` contains `"bam2nuc_rs "`.
- Rationale: `main.rs:27` short-circuits on `cli.version` before `run()`. Only the unit
  `version_string()` was tested; the clap wiring (`disable_version_flag=true` +
  `#[arg(short='V', long="version")]` at `cli.rs:63`) + the main-fn branch were not.

### Cell 2 — non-ASCII `@SQ` error branch (unit)
- Build a `noodles_sam::Header` with one reference sequence whose name holds a non-ASCII
  byte (`b"chr\xff"`), call `build_chr_name_table(&header)`, assert
  `Err(NonAsciiChromosomeName { .. })`.
- Add a positive control in the same test: an all-ASCII name returns `Ok` with the bytes,
  so the test proves the guard fires *only* on non-ASCII (not always). Two separate
  single-entry headers (one ASCII `Ok`, one non-ASCII `Err`) suffice — no mixed-header
  first-offender case is needed to exercise the branch.
- Note: `NonAsciiChromosomeName.name` is lossy-UTF-8, so a `0xFF` byte surfaces as `U+FFFD`
  (`"chr\u{fffd}"`), not the raw byte. The test matches the variant only (`{ .. }`), so this
  is moot here — flagged only in case a future assertion inspects the exact `name` string.

### Cell 3 — `SePeUndetermined` e2e (behavioural)
- New committed fixture `no_bismark_pg.bam`: `@HD` + `@SQ SN:chr1 LN:17` + a **non-Bismark**
  `@PG` (`ID:bowtie2`) + one mapped read. `detect_paired_from_header` ⇒ `None`.
- Spawn `bam2nuc_rs -g <genome_acgtn> --dir <out>/ no_bismark_pg.bam` ⇒ exit 1,
  `stderr` contains `"single-end vs paired-end"`.
- File-output assertion: the error is raised in `count_reads_in_file` *before* any report
  writing, so **no** `no_bismark_pg.nucleotide_stats.txt` should exist. **Impl-time check:**
  confirm `lib.rs run()` does not open/write the output file before
  `count_reads_in_file` returns (contrast the `all_indel` cell, where a header-only partial
  *is* written because counting succeeds and the error is a later `ZeroDivision`). If
  `run()` does pre-open the file, weaken the assertion to "header-only / empty" accordingly
  and document it.

### Cell 4 — coordinate-sorted BAM byte-identity golden
- New committed fixture `se_sorted.bam` = `samtools sort se.bam`. `samtools sort` appends
  its **own** `@PG` *after* Bismark's, so `detect_paired_from_header` still finds
  `ID:Bismark` ⇒ SE (the SE/PE-detection-divergence path the oxy `sorted` cell guarded).
- Mint golden `se_sorted_stats.golden` by running the **Perl oracle** on `se_sorted.bam`.
- Test: run Rust over `se_sorted.bam`, byte-compare against `se_sorted_stats.golden`.
- Invariant assertion: bam2nuc tallies are order-independent, so
  `se_sorted_stats.golden == se_stats.golden` byte-for-byte; the test asserts both
  (proves sorting changes nothing *and* that the Perl oracle agrees on the sorted input).

## Signatures (new test fns + fixtures)

- `tests/golden.rs`:
  - `fn version_flag_long_prints_version_and_exits_zero()`
  - `fn version_flag_short_prints_version_and_exits_zero()`
  - `fn non_bismark_pg_bam_is_se_pe_undetermined()`
  - `fn se_sorted_stats_byte_identical()`
- `src/count.rs` `#[cfg(test)]`:
  - `fn build_chr_name_table_rejects_non_ascii_sq_name()`
- New committed binaries: `tests/data/no_bismark_pg.bam`, `tests/data/se_sorted.bam`,
  `tests/data/goldens/se_sorted_stats.golden`.

## Implementation outline (ordered)

> TDD note: the production code for all four branches already exists and is correct, so
> each test is expected to pass on first run. The "red" discipline here is to confirm each
> test actually exercises its branch — see Validation §"red checks".

1. **`generate_goldens.sh` — add the two fixtures + one golden** (do this first; the
   golden.rs cells consume their outputs):
   - After the `pe_noncanonical`/`all_indel` fixture blocks, add a **non-Bismark-@PG** SAM
     and build it (no golden — behavioural):
     ```bash
     # Non-Bismark @PG → detect_paired_from_header returns None → SePeUndetermined.
     # (Perl test_file likewise dies "Failed to figure out SE or PE".) Behavioural cell.
     {
       printf '@HD\tVN:1.6\tSO:unsorted\n'
       printf '@SQ\tSN:chr1\tLN:17\n'
       printf '@PG\tID:bowtie2\tPN:bowtie2\tVN:2.5.0\tCL:bowtie2 -x g -U reads.fq\n'
       printf 'r1\t0\tchr1\t1\t40\t8M\t*\t0\t0\tACGTACGT\tIIIIIIII\n'
     } > "$WORK/no_bismark_pg.sam"
     make_bam "$WORK/no_bismark_pg.sam" "$DATA/no_bismark_pg.bam"
     ```
   - After `se.bam` is built (it must exist first), add the coordinate-sorted derivative:
     ```bash
     # Coordinate-sorted SE BAM: samtools appends its @PG AFTER Bismark's, so SE/PE
     # detection still sees ID:Bismark (SE). Tallies are order-independent ⇒ identical stats.
     "$SAMTOOLS" sort -o "$DATA/se_sorted.bam" "$DATA/se.bam"
     ```
   - In the "run Perl … harvest goldens" section, after the `se` harvest, add:
     ```bash
     run_dir="$(run_perl se_sorted "$DATA/genome_acgtn" "$DATA/se_sorted.bam")"
     cp "$run_dir/out/se_sorted.nucleotide_stats.txt" "$GOLD/se_sorted_stats.golden"
     ```
   - Update the header comment block's fixture inventory to mention the two new BAMs.
2. **Mint ONLY the new artifacts — do NOT run the full script's `rm -rf "$GOLD"`** (default,
   per dual plan-review). The step-1 edits keep `generate_goldens.sh` as the reproducible
   provenance record, but to avoid churning the 8 committed goldens we build only what's new,
   by hand, on the dev box (Perl 5.34 + samtools 1.21, `LC_ALL=C`):
   - Build `no_bismark_pg.bam` and `se_sorted.bam` (run just the new SAM-heredoc +
     `samtools view -b` / `samtools sort` commands — not the whole script).
   - Mint `se_sorted_stats.golden` only: run the real Perl `bam2nuc` on `se_sorted.bam` into
     a temp genome copy and copy out `se_sorted.nucleotide_stats.txt`.
   - **Verify** `git status` shows ONLY 3 new files (`no_bismark_pg.bam`, `se_sorted.bam`,
     `goldens/se_sorted_stats.golden`) — the 8 existing goldens are untouched by construction.
   - *Optional provenance check:* a full `bash generate_goldens.sh` re-run should reproduce
     all goldens byte-identically; if it instead modifies an existing golden, that signals a
     Perl/samtools drift — investigate, but never commit that churn.
3. **`tests/golden.rs` — add Cell 1, Cell 3, Cell 4** in the "Behavioral / exit-code cells"
   region (reusing `bin()`, `copy_genome`, `data_dir()`, `golden()`, `assert_bytes_eq`):
   ```rust
   #[test]
   fn version_flag_long_prints_version_and_exits_zero() {
       bin().arg("--version").assert().success()
           .stdout(predicates::str::contains("bam2nuc_rs "))
           .stdout(predicates::str::contains(std::env::consts::OS));
   }

   #[test]
   fn version_flag_short_prints_version_and_exits_zero() {
       bin().arg("-V").assert().success()
           .stdout(predicates::str::contains("bam2nuc_rs "));
   }

   #[test]
   fn non_bismark_pg_bam_is_se_pe_undetermined() {
       let tmp = copy_genome("genome_acgtn");
       let genome = tmp.path().join("genome");
       let out = tmp.path().join("out");
       std::fs::create_dir_all(&out).unwrap();
       bin()
           .arg("-g").arg(&genome)
           .arg("--dir").arg(format!("{}/", out.display()))
           .arg(data_dir().join("no_bismark_pg.bam"))
           .assert()
           .failure()
           .code(1)
           .stderr(predicates::str::contains("single-end vs paired-end"));
       // Error raised during counting setup, before any report write (see impl check).
       assert!(!out.join("no_bismark_pg.nucleotide_stats.txt").exists());
   }

   #[test]
   fn se_sorted_stats_byte_identical() {
       let (_t, stats) = run_stats("genome_acgtn", "se_sorted.bam");
       assert_bytes_eq(&stats, &golden("se_sorted_stats.golden"), "se_sorted");
       // Order-independent tally: sorted stats == unsorted SE golden, byte-for-byte.
       assert_eq!(stats, golden("se_stats.golden"), "sorted == unsorted SE stats");
   }
   ```
4. **`src/count.rs` `#[cfg(test)]` — add Cell 2** (build a `Header` like dedup's
   `header_with_chrs`, with a non-ASCII `BString` key):
   ```rust
   #[test]
   fn build_chr_name_table_rejects_non_ascii_sq_name() {
       use bstr::BString;
       use noodles_sam::header::record::value::Map;
       use noodles_sam::header::record::value::map::ReferenceSequence;
       use std::num::NonZeroUsize;

       let ln = NonZeroUsize::try_from(1000).unwrap();

       // positive control: all-ASCII names round-trip to bytes.
       let mut ok = Header::default();
       ok.reference_sequences_mut()
           .insert(BString::from(b"chr1".to_vec()), Map::<ReferenceSequence>::new(ln));
       assert_eq!(build_chr_name_table(&ok).unwrap(), vec![b"chr1".to_vec()]);

       // non-ASCII byte in a @SQ name → NonAsciiChromosomeName (fires on first offender).
       let mut bad = Header::default();
       bad.reference_sequences_mut()
           .insert(BString::from(b"chr\xff".to_vec()), Map::<ReferenceSequence>::new(ln));
       assert!(matches!(
           build_chr_name_table(&bad).unwrap_err(),
           BismarkBam2nucError::NonAsciiChromosomeName { .. }
       ));
   }
   ```
   (`Header` is already in scope in `count.rs`; add the `use`s inside the test fn to avoid
   touching the module's import block.)
5. **Run the suite** (sandbox disabled — `target/` writes):
   `cargo test -p bismark-bam2nuc` → expect 72 unit + 17 golden + 2 sanity (was 71/13/2;
   golden +4 = two `version_flag_*` + `non_bismark_pg_*` + `se_sorted_*`),
   1 ignored. `cargo clippy -p bismark-bam2nuc --all-targets -- -D warnings` clean;
   `cargo fmt`.
6. **Update `COVERAGE.md`** (and any test-count claims in `PLAN.md`) to reflect the closed
   gaps and the new counts. Optionally note in the PR that the 4 handoff gaps are closed.

## Efficiency

Negligible — 4 small tests + 2 tiny BAM fixtures (a handful of records) + 1 ~1 KB golden.
No production-code change, no new dependency, no hot-path impact. CI stays hermetic.

## Integration

- **Reads/writes:** new committed binaries under `tests/data/` (+ one golden); edits to
  `generate_goldens.sh`, `tests/golden.rs`, `src/count.rs`, `COVERAGE.md`. No `src` behaviour
  changes, no public API change, no version bump.
- **Ordering in `generate_goldens.sh`:** `se_sorted.bam` must be created *after* `se.bam`
  (it sorts it); its golden harvest goes in the run-Perl section after the `se` harvest.
- **Downstream:** none — additive tests only. PR #922's CI gate (`cargo test` + workspace
  `clippy -D warnings` + `fmt`) must stay green.

## Assumptions

1. **Dev box reproduces the original goldens byte-for-byte** (Perl 5.34 + samtools 1.21).
   If not, step 2's verify-unchanged check catches it before commit. (Open risk — see below.)
2. `samtools sort` preserves all per-record fields bam2nuc reads (POS, CIGAR, ref name,
   FLAG, SEQ) and only reorders + appends its `@PG`. → sorted stats == unsorted stats.
   (Fixed: bam2nuc never inspects sort order.)
3. `bstr::BString` is the key type of `header.reference_sequences_mut()` in
   `noodles-sam =0.85.0` (confirmed via dedup `pipeline.rs:1103` using the same pattern).
4. The `SePeUndetermined` error is surfaced before any output file is created (validated at
   impl-time in step 3 / Cell 3's impl check).
5. These are **regression** tests against already-correct code — expected green on first
   run (not red-green TDD).

## Validation

| # | What to verify | How | Expected |
|---|---|---|---|
| V1 | `--version`/`-V` print + exit 0 | `cargo test version_flag_*` | both pass; stdout has `bam2nuc_rs ` + OS |
| V2 | non-ASCII `@SQ` → error; ASCII → ok | `cargo test build_chr_name_table_rejects_non_ascii_sq_name` | pass (both arms) |
| V3 | non-Bismark `@PG` → exit 1 + msg + no stats file | `cargo test non_bismark_pg_bam_is_se_pe_undetermined` | pass |
| V4 | sorted stats byte-identical to Perl + to unsorted | `cargo test se_sorted_stats_byte_identical` | pass; `se_sorted_stats.golden == se_stats.golden` |
| V5 | existing goldens unchanged after minting the new artifacts | `git status` / `git diff --stat tests/data/` | only 3 *new* files added; 8 existing goldens untouched |
| V6 | whole crate still green | `cargo test -p bismark-bam2nuc` + `clippy -D warnings` + `fmt --check` | all green; counts 72/17/2/1-ignored |

**"Red" checks (confirm each test bites):**
- Cell 2: mentally/temporarily flip the `is_ascii()` guard → test must fail. (Or assert the
  positive-control arm to prove the guard isn't always-erroring.)
- Cell 3: a fixture *with* a Bismark `@PG` would succeed — so the assertion depends on the
  non-Bismark `@PG`. Double-check the fixture's `@PG` truly lacks `ID:Bismark`.
- Cell 4: the `assert_eq!(stats, se_stats.golden)` arm proves the cell isn't trivially
  comparing a file to itself minted from the same run.

## Questions or ambiguities

- **Resolved (dual plan-review) — golden reproducibility.** A full `generate_goldens.sh`
  re-run does `rm -rf "$GOLD"`, rebuilding *all* goldens and risking churn on the 8 committed
  ones if the dev box's Perl/samtools drift from the original mint. **Default is now to mint
  ONLY the new artifacts by hand** (step 2), leaving the existing goldens untouched by
  construction; the edited script stays the provenance record, and a full re-run is an
  optional reproducibility check — never the commit path.
- **Open — Cell 3 file-output assertion.** Whether `run()` pre-creates the output file is
  resolved at impl-time (step 3 impl check). No behaviour change either way; only the
  assertion's exact form depends on it.

_No Critical questions — none change the goal, scope, or the production code (which is
unchanged)._

## Self-Review

- **Efficiency:** trivial; additive tests, tiny fixtures, no prod/dep change. ✓
- **Logic:** ordering dependency (`se_sorted.bam` after `se.bam`) called out; golden harvest
  placement specified; test counts updated. ✓
- **Edge cases:** Cell 2 covers both arms + first-offender short-circuit; Cell 3 asserts
  exit code *and* message *and* no stray file; Cell 4 asserts byte-identity to Perl *and*
  the order-independence invariant. ✓
- **Integration:** confirmed all dev-deps already present (no Cargo.toml edit); CI stays
  hermetic; PR #922 gate unaffected; no version bump. ✓
- **Remaining risks:** (1) golden-reproducibility churn — now eliminated by minting only the
  new artifacts (revision 3); (2) the Cell-3 no-file assertion form — resolved (see notes).

## Implementation Notes (2026-05-31)

Implemented after dual plan-review + the 3 applied revisions. **All steps green; no
production-code change.**

**What was built:**
- `tests/data/generate_goldens.sh` — added `se_sorted.bam` (`samtools sort se.bam`, after
  `se.bam`), `no_bismark_pg.bam` (bowtie2-`@PG` SAM heredoc, after `all_indel`), and the
  `se_sorted_stats.golden` harvest (after the `se` harvest). Provenance preserved.
- **Minted by hand** (NOT a full script re-run): built the 2 new BAMs + ran the real Perl
  `bam2nuc` v0.25.1 (Perl 5.34, samtools 1.21, `LC_ALL=C`) on `se_sorted.bam` →
  `se_sorted_stats.golden`. `git status` confirmed **only 3 new files**; the 8 existing
  goldens are byte-unchanged (V5 ✓).
- `tests/golden.rs` — Cells 1/3/4: `version_flag_long_*`, `version_flag_short_*`,
  `non_bismark_pg_bam_is_se_pe_undetermined`, `se_sorted_stats_byte_identical`.
- `src/count.rs` `#[cfg(test)]` — Cell 2: `build_chr_name_table_rejects_non_ascii_sq_name`
  (ASCII positive control + `chr\xff` → `NonAsciiChromosomeName`).

**Verification (V1–V6 all ✓):** `cargo test -p bismark-bam2nuc` = **72 unit + 17 golden +
2 sanity**, 1 ignored, 0 failed. `clippy -p bismark-bam2nuc --all-targets -D warnings` clean.
`fmt` clean. The Cell-4 invariant `se_sorted_stats.golden == se_stats.golden` confirmed
byte-identical at mint time (Perl agrees) and re-asserted in the test.

**Cell-3 no-file assertion — resolved:** confirmed (and the dual reviewers verified) that
`SePeUndetermined` returns inside `count_reads_in_file` (`count.rs:152`) *before* `run()`
creates the output file (`lib.rs:84`), so the unconditional `assert!(!...exists())` is
correct. (Contrast `all_indel`, where counting succeeds and a header-only partial *is*
written before the later `ZeroDivision`.)

**Iteration log:**
- `#1`: All 4 cells passed on first `cargo test` run (72/17/2) — expected, since these are
  regression tests over existing correct code.
- `#2`: `cargo fmt --check` flagged the Cell-2 `.insert(...)` chained call; `rustfmt` prefers
  one-arg-per-line when the arg list exceeds the width budget. Ran `cargo fmt` to apply
  (cosmetic, also touched `golden.rs`); re-check clean. No behavioural change.

**Deviations from plan:** none material. Minor: rustfmt expanded the Cell-2 insert layout
(documented above).
