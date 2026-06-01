# Plan Review A — bam2nuc test-gap closure (4 robustness cells)

**Reviewer:** A (independent, fresh context)
**Plan reviewed:** `plans/05312026_bismark-bam2nuc/TEST_GAPS_PLAN.md`
**Date:** 2026-05-31
**Verdict:** **Sound to implement** after one small documentation fix (the test-count claim is wrong) and two recommended assertion-hardening tweaks. No blocking logic defect; every code-level claim I checked is accurate.

I verified the plan against the live source rather than taking it at face value. Findings cite `file:line`.

---

## 1. Logic review

### Cell 1 — `--version` / `-V` e2e — CORRECT
- `main.rs:27-30` short-circuits on `cli.version` with `println!("{}", version_string())` (STDOUT) then `return ExitCode::SUCCESS` (exit 0). The plan's "exit 0 + stdout contains `bam2nuc_rs ` + OS" (plan §Cell 1, lines 63-65) is faithful.
- `version_string()` (`lib.rs:96-103`) = `"bam2nuc_rs {ver} ({os}/{arch})"`, so STDOUT contains both `"bam2nuc_rs "` and `std::env::consts::OS` (e.g. `macos` inside `(macos/aarch64)`). The `contains(OS)` substring assertion holds.
- `disable_version_flag = true` (`cli.rs:30`) + custom `#[arg(short='V', long="version")]` (`cli.rs:63-64`) is the real wiring; `clap_definition_is_valid` (`cli.rs:147`) already proves no clap conflict. The gap is genuine: `sanity.rs:7` only tests `version_string()` directly, never the spawn path (plan lines 38-39 — accurate).

### Cell 2 — non-ASCII `@SQ` unit test — CORRECT
- `build_chr_name_table` (`count.rs:45-57`) iterates `header.reference_sequences()`, takes `name.as_ref()` as `&[u8]` (`count.rs:48`), and returns `NonAsciiChromosomeName { name: lossy }` on the first non-ASCII name (`count.rs:49-53`). The plan's "fires on first offender" claim (plan lines 76-77, 213) matches the early `return Err`.
- The `Header`-building pattern in the plan (plan lines 197-221) matches the proven dedup helper `header_with_chrs` (`bismark-dedup/src/pipeline.rs:1103-1112`): same `reference_sequences_mut().insert(BString::from(..), Map::<ReferenceSequence>::new(NonZeroUsize…))`, same `use` paths (`noodles_sam::header::record::value::Map` + `::map::ReferenceSequence`). dedup uses `BString::from(&str)`; the plan uses `BString::from(Vec<u8>)` — required because `b"chr\xff"` is not valid UTF-8 and cannot be a `&str`. Both `From` impls exist in `bstr =1.10.0`. Compiles.
- `Header` is in scope in `count.rs` (imported `count.rs:19`), so the test fn only needs the three local `use`s the plan adds. Correct.

### Cell 3 — `SePeUndetermined` e2e — CORRECT, including the file-output assertion
This is the assertion the brief flagged for scrutiny. I traced both call sites:
- `count_reads_in_file` (`count.rs:144-154`) runs `build_chr_name_table` (line 150) **before** `detect_paired_from_header(..).ok_or(SePeUndetermined)?` (lines 151-152). For Cell 3's fixture (ASCII `chr1`), the non-ASCII guard passes and the `SePeUndetermined` branch is what fires — the two error branches do not collide.
- `detect_paired_from_header` (`bismark-io/src/read.rs:649-677`) serialises the header to SAM text and only matches a line that BOTH `starts_with("@PG")` AND `contains("ID:Bismark")` (`read.rs:665`). `@PG ID:bowtie2` satisfies the first but not the second → `continue` → loop exhausts → returns `None` (`read.rs:676`). Confirmed the fixture triggers the gap.
- The error wording (`error.rs:104`) is `"failed to determine single-end vs paired-end from the BAM @PG header"`; the plan's `predicates::str::contains("single-end vs paired-end")` (plan line 182) is a real substring. (Exit 1: `main.rs:36` maps any `BismarkBam2nucError` to `ExitCode::from(1)`.)
- **No-stats-file assertion is correct.** In `run()` (`lib.rs:60-87`), `count::count_reads_in_file` is called at `lib.rs:75`; the output file is only created at `lib.rs:84` (`File::create(&out_path)`), after `derive_output_name` (`lib.rs:81`). The `SePeUndetermined` error returns from line 75, so `no_bismark_pg.nucleotide_stats.txt` is never opened. The plan's contrast with `all_indel` (where counting *succeeds* and the header is written before a later `ZeroDivision`) is exactly right — and the existing `all_indel_sample_zerodivision_exits_one` cell (`golden.rs:187-210`) confirms the partial-header behaviour. The plan's impl-time "verify run() doesn't pre-open" check is already satisfiable by inspection; the resolution is "assertion stands as written."

### Cell 4 — coordinate-sorted BAM golden — CORRECT, strong cross-check
- `run_stats` derives the output name from the BAM stem (`golden.rs:81-82`: `Path::new(bam).file_stem()`), so `"se_sorted.bam"` → reads `out/se_sorted.nucleotide_stats.txt`. Matches `derive_output_name("se_sorted.bam")` → strip trailing `bam` token → `"se_sorted.nucleotide_stats.txt"` (`output_name.rs:33-35`, confirmed by `bam_basic` test). Rust output name and Perl golden-harvest name agree.
- Order-independence is real: `write_stats` (`report.rs:61-95`) emits a fixed `HEADER` line + 4 mono + 16 di rows in canonical order from accumulated counts — **no** read-order or header-derived (`@PG`/`@SQ`) content. So `samtools sort` (which only reorders records + appends its own `@PG`) cannot change a single output byte. The invariant `se_sorted_stats.golden == se_stats.golden` (plan lines 98-100, 192) is provably valid.
- The `assert_eq!(stats, golden("se_stats.golden"))` arm (plan line 192) is a genuine cross-check — it compares the Rust output over the *sorted* BAM against the *unsorted* golden minted from `se.bam` in a separate Perl run. This is NOT a file-compared-to-itself tautology; it is the strongest of the four cells. Good.
- The extra `@PG` from `samtools sort` lands *after* Bismark's `@PG` (plan line 94), so `detect_paired_from_header` still finds `ID:Bismark` first/at-all and returns SE. Confirmed by the `read.rs:662-676` loop (any matching line returns; non-matching `@PG` is skipped).

### `generate_goldens.sh` edits — ORDERING CORRECT
- `se.bam` is built at `generate_goldens.sh:118`; the plan inserts `samtools sort -o se_sorted.bam se.bam` "after `se.bam`" (plan lines 135-140). The SAM-build section (lines 98-152) is distinct from the harvest section (154-190); placing the `sort` anywhere after line 118 in the build section is valid. Recommend placing it right after the `se.bam` block (`:118`) for locality.
- The golden harvest `run_perl se_sorted … ; cp .../se_sorted.nucleotide_stats.txt …` (plan lines 143-144) belongs after the `se` harvest at `:179-181`. `run_perl` returns the run dir on STDOUT (`generate_goldens.sh:167`) and routes diagnostics to STDERR (`:164`), so `run_dir="$(run_perl …)"` captures only the path — the plan's snippet is consistent with the helper's contract.

---

## 2. Assumptions

| # | Plan assumption | Verdict |
|---|---|---|
| 1 | Dev box reproduces the 8 existing goldens byte-for-byte (Perl 5.34 + samtools 1.21) | **Open risk, correctly flagged.** `rm -rf "$GOLD"` (`generate_goldens.sh:37`) regenerates ALL goldens on every run. There are exactly 8 committed goldens (verified by `ls tests/data/goldens/`), matching V5. The mitigation (step 2 / V5: verify-unchanged + STOP) is the right guard. See Action item I-2 for a stronger fallback. |
| 2 | `samtools sort` only reorders + appends `@PG`; sorted stats == unsorted | **Fixed/true** — bam2nuc never inspects sort order (raw `record_bufs`, `count.rs:153`) and emits no header-derived bytes (`report.rs`). |
| 3 | `BString` is the `reference_sequences_mut()` key type in `noodles-sam =0.85.0` | **Confirmed** via `pipeline.rs:1106-1107` (same pin: `Cargo.toml:44`). |
| 4 | `SePeUndetermined` is surfaced before any output file is created | **Confirmed** by inspection (`lib.rs:75` vs `:84`); the plan's impl-time hedge resolves to "assertion stands." |
| 5 | These are regression tests — green on first run | Consistent with "code already exists and is correct." |

**No-Cargo.toml-change assumption (plan line 47):** verified. `Cargo.toml:50-57` already declares `assert_cmd =2.0.16`, `predicates =3.1.2`, `tempfile =3.10.1`, `bstr =1.10.0`, `noodles-core =0.20.0`; `noodles-sam =0.85.0` is a regular dep (`:44`). Holds.

---

## 3. Efficiency

Negligible, as the plan states (4 tiny tests, 2 small BAMs, 1 ~0.5 KB golden, no prod/dep change, CI stays hermetic). Nothing to add.

---

## 4. Validation sufficiency

The V1–V6 table + "red checks" are well-targeted. Specific assessment of "could a cell pass without exercising its branch?":

- **Cell 2** — guarded against tautology: the positive-control ASCII arm (plan line 211) proves the guard is not always-erroring; the negative arm proves it fires. Both arms in one fn. Good. (The red-check "flip `is_ascii()`" is sound; the positive control already covers it permanently.)
- **Cell 3** — the no-file assertion + the specific stderr substring + exit code 1 together pin the branch. The only way this passes spuriously is if the fixture's `@PG` accidentally contained `ID:Bismark`; the red-check (plan line 276) calls this out. **Strengthen** by also asserting `detect_paired_from_header` semantics indirectly — see I-1.
- **Cell 4** — the dual assertion (vs its own Perl golden AND vs the unsorted golden) is the gold standard; cannot pass as a file-compared-to-itself.
- **Cell 1** — `success()` + two `stdout(contains(..))` predicates. Adequate. One soft gap: it does not assert STDOUT vs STDERR routing beyond `contains` (a regression that printed the version to STDERR would still satisfy `success()` but FAIL `.stdout(contains(..))`, since `assert_cmd`'s `.stdout(..)` checks the STDOUT stream specifically). So routing IS implicitly covered. Good.

**One concrete inaccuracy in the validation/expected-count claims (see Action item C-1):** the plan repeatedly states the post-change golden-test count is **15** (plan lines 226, 270 "counts 72/15/2/1-ignored"). I counted the existing `tests/golden.rs` at **13** `#[test]`s (matches the plan's "was 13") and the plan adds **4** new golden cells (its own Signatures list, lines 104-108: two version fns + one SePe fn + one sorted fn). 13 + 4 = **17**, not 15. The unit count is right (71 src tests verified across modules → +1 Cell 2 = 72) and sanity is right (2). The "15" is an internal arithmetic slip (Cell 1 contributes TWO tests, not one). This must be fixed or the V6 / step-5 gate will "fail" against a wrong expected number.

**Untracked test file:** there is a 4th test binary `tests/byte_identity_real_data.rs` (1 `#[ignore]` test) that the plan's count summary never names. The "1 ignored" is acknowledged, so this is cosmetic — but the count line would read more clearly as `72 unit / 17 golden / 2 sanity / 1 ignored (real-data)`.

---

## 5. Alternatives

1. **Cell 3 fixture — non-Bismark `@PG` vs omit `@PG` entirely.** The plan uses `@PG ID:bowtie2`. An alternative is a header with *no* `@PG` at all. Both yield `None` from `detect_paired_from_header` (the loop simply finds no matching line). The non-Bismark-`@PG` choice is **better** — it mirrors a realistic "aligned by something other than Bismark" scenario and proves the detector discriminates on `ID:Bismark` content (not mere `@PG` presence). Keep as planned. (If you want belt-and-suspenders, a second fixture with zero `@PG` is possible but redundant — not worth the extra committed binary.)

2. **Golden-mint strategy — full re-run vs manual single-golden mint.** The plan defaults to a full `generate_goldens.sh` re-run (which `rm -rf`'s all goldens) with a verify-unchanged guard. Given the churn risk is real and the only NEW golden is `se_sorted_stats.golden`, the **manual single-mint fallback** (run Perl on `se_sorted.bam` into a scratch dir, copy just that one golden) is lower-risk and I'd recommend making it the *primary* path for the golden, while still adding the two fixture-build lines to `generate_goldens.sh` for provenance. See I-2. Either is acceptable; the plan already documents both.

3. **Cell 2 placement.** Unit test in `count.rs #[cfg(test)]` is correct (it tests an internal `pub fn` with synthetic headers, matching the existing `count_records_*` driver tests at `count.rs:336-436`). No better placement.

4. **Optional extra coverage (not required):** Cell 3 could additionally assert that the genome cache *was* written (since `get_genomic_frequencies` at `lib.rs:72-73` runs before the failing `count_reads_in_file`). This documents that the failure is post-cache-write — but it's a side effect into the *genome* tempdir, not the out dir, and adds noise. Skip unless a reviewer wants it.

---

## Action items

### Critical
- **C-1 — Fix the golden test-count claim.** Change every "15 golden" / "72/15/2" to **"17 golden" / "72/17/2/1-ignored"** (plan lines 226 and 270). Cell 1 adds TWO `#[test]`s, so `13 + 4 = 17`. Verified: `tests/golden.rs` currently has 13 `#[test]`s; `src/*` has 71 unit tests; `tests/sanity.rs` has 2; `tests/byte_identity_real_data.rs` has the 1 ignored. If left at 15, the step-5 verification gate compares against a wrong target. (Documentation-only; no code impact.)

### Important
- **I-1 — Make Cell 3 immune to a Bismark-`@PG` slip.** The plan's red-check (line 276) says "double-check the fixture's `@PG` truly lacks `ID:Bismark`." Bake this into the test rather than leaving it as a manual mental check: e.g. nothing in the *output* assertions would catch a fixture that accidentally said `ID:Bismark` (it would then succeed and the cell would just be a dead pass that never reaches the error). The exit-1 + stderr-substring assertions DO catch that (a Bismark `@PG` would make it exit 0 with no such stderr), so the cell is self-defending *as written* — but only because of the negative assertions. Keep all three negative assertions (`.failure().code(1).stderr(contains(...)))`); do not weaken to just `.failure()`. (No change needed if implemented exactly as in the plan snippet, lines 175-184 — just don't drop the `.code(1)`/`stderr` arms.)
- **I-2 — Prefer the manual single-golden mint to avoid `rm -rf "$GOLD"` churn.** `generate_goldens.sh:37` deletes and rebuilds all 8 goldens; a Perl/samtools version drift on the dev box would silently rewrite the committed goldens. Recommend: (a) add the two fixture-build lines + the harvest line to `generate_goldens.sh` for provenance (so a future clean re-mint is correct), BUT (b) for *this* change, mint only `se_sorted_stats.golden` by running Perl on `se_sorted.bam` into a scratch dir and copying that single file — leaving the 8 committed goldens byte-untouched. This sidesteps the V5 churn risk entirely instead of merely detecting it. The plan lists this as a fallback (lines 282-288); promote it to the default for the golden bytes.

### Optional
- **O-1 — Name the 4th test binary in the count line.** Add `tests/byte_identity_real_data.rs (1 ignored, real-data)` to the count summary for clarity (purely cosmetic).
- **O-2 — Cell 1 short-flag could also assert OS** for symmetry with the long-flag test (the plan only asserts `bam2nuc_rs ` on `-V`, lines 164-167). Trivial; both flags hit the same `main.rs:28` line, so not necessary.
- **O-3 — Locality of the `samtools sort` line.** Place it immediately after the `se.bam` `make_bam` at `generate_goldens.sh:118` (rather than vaguely "after the pe/all_indel blocks") so the dependency on `se.bam` is visually obvious. Minor.

---

## Bottom line

Every load-bearing technical claim in the plan checks out against the source: the `count_reads_in_file` ordering (`build_chr_name_table` before `SePeUndetermined`), the `run()` "no stats file before count returns" guarantee, the error wordings and stderr substrings, the `-V`/`--version` STDOUT+exit-0 wiring, the dev-deps being already present, the `run_stats` stem→output-name mapping, the `detect_paired_from_header` `None`-on-`ID:bowtie2` behaviour, and the `BString` insert API. The order-independence invariant for Cell 4 is provably correct (no header-derived output bytes).

The **only** must-fix is the **arithmetic slip on the golden test count (15 → 17, C-1)**, which would otherwise trip the post-implementation verification gate. Two `Important` items (I-1 keep Cell 3's full negative assertions; I-2 prefer the manual single-golden mint) harden it but are not blockers. **Implement after fixing C-1.**
