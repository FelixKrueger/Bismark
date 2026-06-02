# Code Review A — bam2nuc test-gap closure (4 robustness cells)

**Reviewer:** Reviewer A (independent, fresh context)
**Target:** PR #922 follow-up — 4 added regression/characterization tests + 2 BAM fixtures
+ 1 golden + script/docs edits. **No production code changed.**
**Plan:** `plans/05312026_bismark-bam2nuc/TEST_GAPS_PLAN.md`
**Date:** 2026-05-31

## Summary

Verdict: **SHIP AS-IS.** All four cells assert real, non-tautological behaviour over the
correct branch; both new BAM fixtures are correctly constructed; the Cell-3 no-file
assertion is provably correct against the `run()` control flow; `generate_goldens.sh` is
ordered and self-consistent; and the suite compiles, lints clean, and passes with exactly
the claimed counts. I found no Critical/High/Medium issues. One Low nit (cosmetic) is
recorded below. I applied no fixes (none warranted; production code is untouched and the
tests are sound).

Verification (run locally, sandbox disabled):
- `cargo test -p bismark-bam2nuc` → **72 unit + 17 golden + 2 sanity**, 1 ignored, 0 failed.
  Matches the plan/COVERAGE claim (was 71/13/2; golden +4).
- `cargo clippy -p bismark-bam2nuc --all-targets -- -D warnings` → clean.

## Issues by review area

### 1. Logic — do the tests assert real behaviour? (no tautologies)

**No tautologies.** Each cell bites:

- **Cell 4 `se_sorted_stats_byte_identical` (golden.rs:336-348)** — the focus question.
  The dual assert (`== se_sorted_stats.golden` AND `== se_stats.golden`) is a **meaningful
  cross-check, NOT circular.** Provenance confirms it:
  - `se_sorted_stats.golden` was minted by the **Perl oracle** running on `se_sorted.bam`
    (`generate_goldens.sh:199-201` `run_perl se_sorted ...`), independently of `se_stats.golden`
    (minted on `se.bam`). I confirmed `se_sorted.bam`'s record order is genuinely different
    from `se.bam` (r5 hoisted to position 2; r6 at chr2:10 moved last — coordinate order).
  - I `diff`'d the two committed goldens: **byte-identical.** So the test simultaneously proves
    (a) Rust == Perl on the sorted input, and (b) order-independence of the tally (sorted ==
    unsorted). The first assert is a real Perl-oracle byte gate; the second is a real invariant
    check. Neither compares a file to itself.

- **Cell 2 `build_chr_name_table_rejects_non_ascii_sq_name` (count.rs:303-331)** — exercises the
  real `count.rs:49 !bytes.is_ascii()` guard. The positive-control arm
  (`build_chr_name_table(&ok).unwrap() == vec![b"chr1".to_vec()]`) **does** prevent an
  always-erroring false pass: if the guard fired unconditionally, `.unwrap()` on the ASCII header
  would panic and fail the test. The negative arm (`chr\xff`) asserts the
  `NonAsciiChromosomeName` variant. Both arms required → genuine.

- **Cell 1 `version_flag_{long,short}` (golden.rs:291-310)** — real e2e coverage of the clap
  wiring + main-fn branch that the prior `version_string()` unit test did NOT touch. I confirmed
  `cli.rs:30 disable_version_flag = true` + `cli.rs:63 #[arg(short='V', long="version")]` + the
  `main.rs:27` short-circuit; spawning `--version`/`-V` is the only path that exercises them.
  Asserting both `"bam2nuc_rs "` and `std::env::consts::OS` is a substantive check on the printed
  string.

- **Cell 3 `non_bismark_pg_bam_is_se_pe_undetermined` (golden.rs:312-333)** — asserts exit code 1
  **and** the `"single-end vs paired-end"` message **and** absence of the stats file. Three
  independent observations; not a tautology.

### 2. Errors / edge cases — `no_bismark_pg.bam` fixture correctness

**Correct.** `samtools view -H no_bismark_pg.bam` shows:
```
@PG  ID:bowtie2   ...
@PG  ID:samtools  ...
@PG  ID:samtools.1 ...
```
There is **no `ID:Bismark`** anywhere. `detect_paired_from_header` (`bismark-io/src/read.rs:649`)
serialises the header and only returns `Some(..)` on a line that both `starts_with("@PG")` and
`contains("ID:Bismark")`; with none present it falls through to `None`
(read.rs:676). Subtle point I checked: the substring match is `"ID:Bismark"`, and
`"ID:bowtie2"`/`"ID:samtools"` do not contain it (and `bowtie2`/`samtools` don't contain
`Bismark`), so there is no accidental match. → `count.rs:152` maps `None` → `SePeUndetermined`.
The fixture is exactly what the cell needs.

### 3. Structure / control flow — Cell-3 `assert!(!...exists())` correctness

**Correct, and tighter than the contrast `all_indel` cell — as intended.** Tracing `run()`
(`lib.rs:44-90`): `count::count_reads_in_file(...)` is called at **lib.rs:75**, which calls
`detect_paired_from_header(...).ok_or(SePeUndetermined)?` at **count.rs:151-152** — i.e. the
error returns *before* control reaches `std::fs::File::create(&out_path)` at **lib.rs:84**.
Therefore no `no_bismark_pg.nucleotide_stats.txt` is ever created, and the unconditional
`assert!(!...exists())` is right.

Contrast holds: the `all_indel` cell (golden.rs:187-210) asserts a **header-only partial DOES
exist** because there counting *succeeds* (no SePe error) and the ZeroDivision occurs later in the
report-writing path, after the header line is already written. The two cells correctly assert
opposite file-existence outcomes for the two different failure points. The plan's "Open — Cell 3
file-output assertion" is fully resolved by the code path.

### 4. Logic — Cell 2 actually exercises the guard

Covered under area 1 above. The non-ASCII `BString::from(b"chr\xff".to_vec())` insert reaches
`count.rs:48-53`, where `name.as_ref()` yields the raw bytes and `!bytes.is_ascii()` is true → the
`NonAsciiChromosomeName` arm. The `matches!(.. { .. })` variant-only match is appropriate (the
`name` field is lossy-UTF-8, so `0xFF` would surface as `U+FFFD` — matching the variant avoids
coupling to that). Positive control prevents the always-error false pass. Confirmed real.

### 5. `generate_goldens.sh` edits — ordering & self-consistency

**Correct and self-consistent.**
- `se_sorted.bam` is built by `"$SAMTOOLS" sort -o .../se_sorted.bam .../se.bam` (line ~121)
  **immediately after** `make_bam "$WORK/se.sam" "$DATA/se.bam"` (line ~118) — `se.bam` exists
  first. ✓
- `make_bam`/`SAMTOOLS`/`run_perl` are all defined before use (`SAMTOOLS=` line 28, `make_bam ()`
  line 99, `run_perl ()` line 173). ✓
- `no_bismark_pg.bam` is a self-contained SAM heredoc → `make_bam`, added after the `all_indel`
  block; no ordering dependency. ✓
- Golden harvest `run_perl se_sorted "$DATA/genome_acgtn" "$DATA/se_sorted.bam"` →
  `cp .../se_sorted.nucleotide_stats.txt "$GOLD/se_sorted_stats.golden"` is placed right after the
  `se` harvest (lines ~196-201). ✓
- A full re-run (`rm -rf "$GOLD"` at the top, then rebuild) would regenerate all goldens including
  the new one, byte-consistently. The fixtures+goldens were minted by hand this round (plan step 2)
  to avoid churning the 8 existing goldens, but the script remains a faithful provenance record. ✓

### 6. Compile + pass

Reported counts above. All green, clippy clean. The new `bstr`/`noodles-sam` `use`s in Cell 2 are
scoped inside the test fn (count.rs:304-307), so they don't touch the module import block and don't
trip `unused_imports`.

### 7. Fixture inspection — `se_sorted.bam`

`samtools view -H se_sorted.bam` confirms `@HD ... SO:coordinate` (genuinely coordinate-sorted) and
that `@PG ID:Bismark` is **retained** ahead of the samtools `@PG ID:samtools/.1/.2` lines — exactly
the SE/PE-detection-survives-sort path the cell guards. Body is reordered relative to `se.bam`. ✓

## Fixes applied

None. The change is test-only, sound, and passing; there was nothing unambiguous-and-low-risk to
fix.

## Recommendations (prioritized)

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low (optional, cosmetic — do not block ship):**
  - L-1: The plan step 1 mentioned "update the header comment block's fixture inventory to mention
    the two new BAMs." The script's top comment block (lines 1-20) is a *prose provenance* note, not
    an enumerated inventory, and each fixture is documented by an inline comment at its `make_bam`/
    `sort` call (the two new ones included). So there is no enumerated list to fall out of date —
    treat the plan item as N/A rather than a gap. No action needed.
  - L-2: `version_flag_short` (golden.rs:304-310) asserts only `"bam2nuc_rs "`, not the OS string
    (the long-flag cell asserts both). This is deliberate de-duplication and fine; if one wanted
    perfect symmetry one could add `.stdout(contains(OS))` to the short cell too. Purely optional.

## Verdict

**Ship as-is.** All four cells exercise their intended branch with real, non-circular assertions;
the two fixtures are correct (`no_bismark_pg.bam` lacks `ID:Bismark`; `se_sorted.bam` is
coordinate-sorted yet retains Bismark `@PG`); the Cell-3 no-file assertion matches the `run()`
control flow; `generate_goldens.sh` is correctly ordered and reproducible; and `cargo test`
(72/17/2, 1 ignored) + `clippy -D warnings` are green. No production code changed. Only a cosmetic
Low nit, which does not block.
