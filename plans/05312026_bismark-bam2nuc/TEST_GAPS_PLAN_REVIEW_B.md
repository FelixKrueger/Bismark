# Plan Review B — bam2nuc test-gap closure (4 robustness cells)

**Reviewer:** B (independent, fresh context)
**Plan under review:** `plans/05312026_bismark-bam2nuc/TEST_GAPS_PLAN.md`
**Date:** 2026-05-31
**Verdict:** **Implement after a minor revision** (one factual arithmetic error + two prose/snippet mismatches; no design blockers).

All claims below were checked against source under `/Users/fkrueger/Github/Bismark-bam2nuc/`. Citations are `file:line`.

---

## Summary of evidence verification

Every load-bearing factual claim in the plan was independently confirmed against the code:

| Plan claim | Verified? | Evidence |
|---|---|---|
| `count_reads_in_file` order: header → `build_chr_name_table` → `detect_paired_from_header` → count | YES | `count.rs:148-153` |
| `SePeUndetermined` raised **before** any output file is created | YES | error returns at `count.rs:152` (inside `count_reads_in_file`, called at `lib.rs:75`); the output file is `File::create`d only at `lib.rs:84`, strictly after |
| `all_indel` writes a header-only partial file (counting succeeds, later ZeroDivision) | YES | partial assertion at `golden.rs:204-209`; ZeroDivision is a *report-time* error, after the file is opened at `lib.rs:84` |
| `NonAsciiChromosomeName` Display = `"non-ASCII chromosome name in BAM header: {name:?}"` | YES | `error.rs:86` |
| `SePeUndetermined` Display contains `"single-end vs paired-end"` | YES | `error.rs:104` (`"failed to determine single-end vs paired-end from the BAM @PG header"`) |
| `--version`/`-V` prints to STDOUT, exits 0, BEFORE validation | YES | `cli.rs:30` (`disable_version_flag=true`), `cli.rs:63` (`short='V', long="version"`), `main.rs:27-30` (`println!` + `ExitCode::SUCCESS`, before `run`/`validate`) |
| `version_string()` contains `"bam2nuc_rs "` + OS | YES | `lib.rs:96-103` → `"bam2nuc_rs {ver} ({os}/{arch})"` |
| dev-deps `assert_cmd`/`predicates`/`tempfile`/`bstr`/`noodles-core` already present; no Cargo.toml change | YES | `Cargo.toml:50-57`; `noodles-sam =0.85.0` is a regular dep at `:44` |
| `run_stats("…","se_sorted.bam")` → `se_sorted.nucleotide_stats.txt` | YES | `golden.rs:81-82` derives name from `file_stem()` |
| non-Bismark `@PG` (`ID:bowtie2`) → `detect_paired_from_header` = `None` → `SePeUndetermined` | YES | `read.rs:665` requires both `starts_with("@PG")` AND `contains("ID:Bismark")`; falls through to `None` at `:676` |
| Bismark SE `@PG` (no `-1/-2`) would return `Some(false)`, NOT error | YES | `read.rs:674` returns `Some(has_1 && has_2)` → the fixture must lack `ID:Bismark` (it does — uses `ID:bowtie2`) |
| dedup `header_with_chrs` API pattern for non-ASCII `BString` header | YES | `pipeline.rs:1103-1112`; `BString::from(b"…".to_vec())` is a workspace-wide idiom (e.g. `bismark-extractor/tests/pe_phase_c.rs:752`) |
| Coordinate-sorted BAM is accepted (no sort check) | YES | `count.rs:148` uses raw `noodles_bam::io::Reader`, not `bismark-io::BamReader`; the `check_not_coordinate_sorted` guard at `read.rs:620` is never on this path (documented at `count.rs:141-143`) |
| Tallies are order-independent | YES | `freqs.rs:51-58` — counting is pure `+= 1` into fixed arrays; addition commutes |

The plan is **factually accurate on every architectural point I checked**, including the two that the brief flagged as risky (the Cell 3 no-file assertion and the SE-`@PG`-would-not-error trap). The only outright factual error is an arithmetic miscount of the resulting golden-test total (see Logic L-1).

---

## Logic review

**L-1 (Important — factual error). The V6 / step-5 test-count arithmetic is wrong.**
The plan adds **four** functions to `tests/golden.rs` (Cell 1 = two: `version_flag_long…` + `version_flag_short…`; Cell 3 = one; Cell 4 = one) and one to `count.rs`. The current golden count is **13** (`grep -c '#[test]' tests/golden.rs` → 13; confirmed against `golden.rs`). So the new totals are **72 unit / 17 golden / 2 sanity**, not the plan's stated **72 / 15 / 2** (plan lines 227, 270). Unit (71→72) and sanity (2→2) are right; the golden figure is a miscount (it looks like only Cells 3+4 were added to the base, omitting Cell 1's two `version_flag_*` fns). Fix the expected count in step 5 and V6, or the implementer will "verify" against a wrong target and may think something failed.

**L-2 (OK, verified). Cell 3's "no stats file" assertion is correct, not risky.**
The brief asked whether this is safe given `all_indel` asserts a header-only partial DOES exist. They are reconcilable: `SePeUndetermined` is raised at `count.rs:152` *inside* `count_reads_in_file`, which `lib.rs:75` calls *before* `File::create` at `lib.rs:84`. The `all_indel` partial exists because counting *succeeds* there and the error (`ZeroDivision`) is a later report-time failure after the file is opened. The plan's Cell 3 impl-time check (plan lines 84-90) is therefore belt-and-suspenders that will confirm what the code already guarantees. The fallback ("weaken to header-only/empty") will not trigger. **Recommend simplifying:** drop the conditional hedge and state plainly that `count.rs:152` precedes `lib.rs:84`, so the no-file assertion is unconditional. (Optional — the hedge is harmless, just unnecessary.)

**L-3 (Important — prose/snippet mismatch in Cell 2).** The Behavior section (plan lines 76-77) promises a *mixed* `["chr1","chr\xff"]` header exercising the first-offender short-circuit, and says "Assert the error `name` is the offending one." The actual code snippet (plan lines 197-221) does **neither**: it builds two **separate single-entry** headers and asserts only `matches!(…NonAsciiChromosomeName{..})` — it never inspects the `name` field, and never builds a mixed header, so "fires on first offender" is not actually tested. This is a discrepancy between stated intent and the implementation snippet, not a code bug. Either (a) drop the over-claim from the prose, or (b) make the snippet match: add a third `bad_mixed` header inserting `chr1` then `chr\xff` and assert `NonAsciiChromosomeName { name }` where `name == "chr\u{fffd}"` (note: `String::from_utf8_lossy` at `error.rs:51` turns `0xFF` into U+FFFD, so the asserted name is `"chr\u{fffd}"`, **not** `"chr\xff"` — easy to get wrong). I lean toward (a): the single-entry bad header already proves the guard fires; the first-offender behavior is a `for`-loop early `return` that is obvious from `count.rs:47-53` and low-value to pin.

**L-4 (OK). Ordering dependency in the generate script is correctly specified.** `se_sorted.bam = samtools sort se.bam` (plan line 139) must follow `se.bam`'s creation (`generate_goldens.sh:118`), and the golden harvest must follow the `se` harvest (`generate_goldens.sh:179-181`). The plan calls both out (plan lines 135, 141-145, 242). `run_perl` (`generate_goldens.sh:157-168`) is the correct helper to reuse for the new golden.

**L-5 (Minor). The new fixtures land in the `git status` "new files" set, but `no_bismark_pg.bam` is a *behavioural* fixture with no golden** — consistent with `all_indel.bam` (built at `generate_goldens.sh:152`, no golden harvested). The plan's V5 expectation ("only *new* `se_sorted_stats.golden`; 8 existing untouched") is correct for the goldens dir, but the implementer should also expect **two new BAMs** in `tests/data/` (the plan does say this at lines 111, 151-152). No conflict, just make sure the `git status` check in step 2 (plan line 150) whitelists all three new paths.

---

## Assumptions

**A-1 (Validated). "sorted stats == unsorted stats byte-for-byte" is guaranteed, not merely likely.**
Two independent reasons, both verified: (1) bam2nuc never inspects sort order on this code path (`count.rs:141-143`, raw reader, no `check_not_coordinate_sorted`); (2) the only state that flows to output is the integer tallies in `NucCounts`, mutated solely by commutative `+= 1` (`freqs.rs:51-58`). Record reordering cannot change any emitted byte. The plan's Assumption 2 (plan lines 251-253) is sound. The `assert_eq!(stats, se_stats.golden)` arm in Cell 4 (plan line 192) is therefore a **meaningful, non-tautological** invariant: it would fail if (a) SE/PE detection diverged between sorted/unsorted (it doesn't — both carry `ID:Bismark`, no `-1/-2`, so both → `Some(false)` per `read.rs:674`), or (b) some order-sensitivity crept in. Good guard.

**A-2 (Hidden assumption, low risk). SE/PE detection is robust to samtools' appended `@PG`.**
The plan (line 95) asserts `samtools sort` appends its `@PG` *after* Bismark's, so `ID:Bismark` is still found. This is correct *because* `detect_paired_from_header` (`read.rs:662-675`) returns on the **first** `@PG` line containing `ID:Bismark` regardless of any later `@PG`. Even if samtools prepended its own `@PG`, the bowtie/samtools line lacks `ID:Bismark` and is skipped (`read.rs:665`). So the SE classification is robust to `@PG` ordering — stronger than the plan's "appends after" justification. No risk.

**A-3 (Open risk, correctly flagged). Golden reproducibility / `rm -rf "$GOLD"` churn.**
`generate_goldens.sh:37` does `rm -rf "$GOLD"` then regenerates all 8 goldens from the dev box's Perl 5.34 + samtools 1.21. If the dev box's toolchain differs byte-wise from the original mint, the 8 existing goldens churn. The plan mitigates correctly (V5 / step 2: verify-unchanged + STOP) and offers the manual-mint fallback (plan lines 282-288). See Alternatives for my preference.

**A-4 (Validated). `BString` key type + `noodles-sam =0.85.0` API.** `bismark-io` pins the same `=0.85.0` (`bismark-io/Cargo.toml:20`) and bam2nuc pins `=0.85.0` (`Cargo.toml:44`), so a single noodles-sam copy is in the lockfile — `Header`/`Map<ReferenceSequence>`/`reference_sequences_mut()` type-check across crates. Cell 2's snippet is API-correct.

---

## Efficiency

Negligible and accurately described (plan lines 232-235). Four tiny tests, two ~400-byte BAM fixtures (cf. existing `se.bam` at 444 B), one ~0.5 KB golden, no prod/dep change. CI stays hermetic (no Perl/samtools at test time — `golden.rs:1-5`). Nothing to flag.

---

## Validation sufficiency

The V1–V6 matrix plus the "red checks" cover the four high-risk failure modes adequately:

- **V1 / Cell 1** — catches a regression in the clap `disable_version_flag` wiring or the `main.rs:27` short-circuit (the *only* untested path; `sanity.rs:7` only tests `version_string()` directly). **Sufficient.** Minor: `-V` cell omits the OS assertion that `--version` has — harmless asymmetry, the long form covers it.
- **V2 / Cell 2** — the positive-control arm (ASCII → `Ok(vec![b"chr1"])`) does prove the guard isn't always-erroring; the red check (flip `is_ascii()`) is the right discipline. **Sufficient**, modulo L-3 (the prose over-claims first-offender + name assertion that the snippet doesn't do).
- **V3 / Cell 3** — asserts exit code 1 **and** the `"single-end vs paired-end"` message **and** the no-file invariant. Triangulated; the message substring is real (`error.rs:104`). **Sufficient.** The "red check" (a fixture *with* `ID:Bismark` would succeed) is the correct sanity hook — and crucially the fixture genuinely lacks `ID:Bismark` (uses `ID:bowtie2`), which I verified is necessary at `read.rs:674`.
- **V4 / Cell 4** — byte-identity vs Perl oracle **plus** the order-independence invariant. **Not a tautology**: the `assert_bytes_eq(stats, golden("se_sorted_stats.golden"))` arm compares Rust output to the *Perl* mint (different producer), and the `assert_eq!(stats, golden("se_stats.golden"))` arm cross-checks against the *unsorted* SE golden. Both arms are meaningful (see A-1). **Sufficient.**
- **V5** — the churn guard. **Sufficient** given the STOP discipline.
- **V6** — whole-crate green + clippy + fmt. **Sufficient**, but the expected counts are wrong (L-1).

**One gap worth noting (Optional):** No cell asserts that the *cache* file is unaffected by Cell 3's failure (it isn't relevant — `get_genomic_frequencies` runs at `lib.rs:72` *before* `count_reads_in_file` at `lib.rs:75`, so on the `no_bismark_pg.bam` run the genome cache **will** be written to the temp genome dir before the error). This is fine — `copy_genome` uses a throwaway TempDir — but if anyone later asserts "no side effects" for Cell 3, note that the cache write is a real, expected side effect that precedes the SePeUndetermined error. Not a defect; just don't over-claim "no files written" in the test comment (the plan's comment at line 183 correctly scopes it to "any report write").

---

## Alternatives

**ALT-1 (Worth adopting for the golden, mild). Mint only the new golden; don't full-re-run.**
The plan's default is the full `generate_goldens.sh` re-run with a verify-unchanged gate, with manual-mint as a fallback (plan lines 282-288). I'd **flip the default**: the `rm -rf "$GOLD"` (`generate_goldens.sh:37`) is the single biggest risk in this otherwise trivial change — a toolchain drift produces churn unrelated to the work and forces a reconcile detour. Minting *only* `se_sorted_stats.golden` (run Perl on `se_sorted.bam` directly, copy the one file) leaves the 8 committed goldens provably untouched and still produces the two new BAM fixtures via the same `samtools` commands run standalone. The full re-run's only benefit is "re-proving provenance," which V5 already has to verify anyway. **However** — the script edits (adding the two fixture blocks + the harvest line) are still valuable as committed provenance for the *next* full regen, so: keep the script edits, but at impl time mint the new golden via a targeted run rather than the destructive full script. Reviewer-preference call; flagging because the plan itself lists this as an open question (plan lines 282-288).

**ALT-2 (Optional, mild). `--version` e2e placement.**
The plan puts Cell 1 in `tests/golden.rs` (plan line 153). That file's module doc (`golden.rs:1-5`) is explicitly about *byte-identity goldens + behavioral cells*, and it already hosts non-golden behavioural cells (`sam_input_is_rejected` at `golden.rs:213`, etc.), so `--version` is not wildly out of place. But it has no genome/golden involvement at all, so a case exists for `tests/sanity.rs` (which already owns the `version_string()` unit test, `sanity.rs:7`) — except `sanity.rs` currently does **not** depend on `assert_cmd` and is unit-only by design (plan line 38 notes this). Adding `assert_cmd` spawning to `sanity.rs` would muddy its "no binary spawn" character. A new `tests/cli.rs` is the cleanest home but adds a file for two tiny tests. **Net:** `golden.rs` is acceptable; if the implementer wants tidiness, `tests/cli.rs` is marginally better. Not worth blocking.

**ALT-3 (Optional). Build the Cell 3 BAM inline (noodles writer) vs committing a fixture.**
The plan commits `no_bismark_pg.bam` (plan line 111). An inline noodles `bam::io::Writer` build (as some sibling crates do) would avoid a committed binary and remove this fixture from the `generate_goldens.sh` dependency. But: (a) every other BAM fixture in this crate is committed + script-minted (`se.bam`, `pe.bam`, `all_indel.bam` — `generate_goldens.sh:118,130,152`), so a committed fixture is the established convention; (b) an inline build would diverge the SE/PE-detection fixture's `@PG` representation from the samtools-produced ones. **Keep the committed fixture** — consistency wins, and the byte content of *this* fixture isn't gated (it's behavioural). The plan's choice is correct.

**ALT-4 (Optional). Cell 2 could be a `tests/`-level test instead of a `count.rs` unit test.**
`build_chr_name_table` is `pub` (`count.rs:45`), so it's reachable from an integration test too. But the existing `count.rs #[cfg(test)]` module already unit-tests sibling helpers with synthetic headers/records (e.g. the `rec(...)` builder at `count.rs:304`), and the dedup precedent is also an in-module test (`pipeline.rs:1087`). In-module is the right call (cheaper, co-located, matches convention). The plan's choice is correct.

---

## Action items (prioritized)

### Critical
*(none — no design blockers; the production code is verified correct and unchanged)*

### Important
1. **Fix the golden test-count arithmetic (L-1).** Step 5 (plan line 227) and V6 (plan line 270) say "15 golden"; the correct figure is **17** (13 existing + 4 new). Update both, or the verify step checks against a wrong target.
2. **Reconcile Cell 2 prose vs snippet (L-3).** Either drop the "mixed header / first-offender / assert error `name`" claims from the Behavior section (plan lines 76-77), or extend the snippet to actually build a mixed `["chr1","chr\xff"]` header and assert the name — remembering the lossy-UTF-8 conversion makes the asserted name `"chr\u{fffd}"`, not `"chr\xff"` (`error.rs:51`). I recommend dropping the over-claim; the single-entry bad header is sufficient.

### Optional
3. **Flip the golden-mint default to "mint only the new golden" (ALT-1)** to eliminate the `rm -rf "$GOLD"` churn risk entirely while keeping the script edits as provenance. (Plan already lists this as an open question; this is just my recommended resolution.)
4. **Simplify Cell 3's no-file hedge (L-2).** The conditional "if `run()` pre-opens the file, weaken the assertion" cannot trigger: `count.rs:152` strictly precedes `lib.rs:84`. State it as a fact; drop the fallback branch.
5. **Add the OS-substring assertion to the `-V` short cell too** for symmetry with `--version` (trivial; plan line 65/166).
6. **Whitelist all three new paths in the step-2 `git status` check (L-5)** — `no_bismark_pg.bam`, `se_sorted.bam`, `goldens/se_sorted_stats.golden` — so the implementer doesn't mistake the two new BAMs for unexpected churn.

---

## Verdict

**Revise first (minor), then implement.** The plan is technically sound: I independently verified every architectural claim — including the two the brief flagged as risky (Cell 3's no-file assertion is *correct and unconditional*; the SE-`@PG`-would-not-error trap is *correctly avoided* by using `ID:bowtie2`). No production code change, no Cargo.toml change, no byte-identity risk. The only hard defect is a golden-count miscount (Important #1); the Cell 2 prose/snippet mismatch (Important #2) and the optional items are polish. Address the two Important items and the plan is ready to implement.
