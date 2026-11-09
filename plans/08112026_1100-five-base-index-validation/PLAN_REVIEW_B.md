# PLAN_REVIEW_B — #1100 `--five_base_index` resolve-time validation

**Reviewer:** B (independent; no shared state with Reviewer A)
**Target:** `plans/08112026_1100-five-base-index-validation/PLAN.md` rev 1
**Base:** `dev` @ `688d919` (follow-up to #1099 → `1a4f8fa`)
**Method:** every claim in the plan checked against source; §2's resolution table re-measured against real `bowtie2` 2.5.5; the #1100 repro and both `--output_dir` failure modes reproduced with `rust/target/debug/bismark`; Perl `legacy_perl/bismark`'s own `-o` semantics measured. `hisat2` is absent, so every HISAT2 statement below is reasoning, not measurement, and is labelled as such.

**Verdict:** the plan's core design is sound and its central empirical claim holds — I attacked it hard and could not break it. But **Task 2 as written turns an existing green end-to-end test red**, and **§5's test list cannot detect the one implementation slip that would actually ship a false rejection**. Both are cheap to fix. Task 3 is a real bug fix but carries an unstated Perl divergence.

---

## 1. Logic review

### 1.1 ✅ §2's resolution table reproduces exactly — 6 for 6

Built a real pUC19 index and ran every row from a third directory:

| `-x` argument | `BOWTIE2_INDEXES` | Plan says | Measured (rc) |
|---|---|---|---|
| `puc` | `$D` | OK | **OK (0)** |
| `puc` | unset | fails | fails (255) |
| `./puc` | `$D` | **OK** | **OK (0)** |
| `$D/puc` (absolute) | — | OK | OK (0) |
| `/definitely/not/here/puc` | `$D` | fails | fails (2) |
| `nope/puc` | `$D` | fails | fails (2) |

The `./puc` row — the one that refutes the issue's own separator heuristic — is real. **§2's central finding stands, and the plan is right to reject the separator-based check.** The wrapper source confirms the mechanism (`bowtie2` `Extract_IndexName_From`: `File::Spec->catfile($ENV{BOWTIE2_INDEXES}, $idx_basename)`), and confirms the plan's probe *order* (cwd first, env only `unless (@idx_filenames)`).

### 1.2 🔴 CRITICAL — Task 2 breaks `five_base_bowtie2_unconverted_index_end_to_end`

`rust/bismark/tests/aligner_cli.rs:6271` is an existing, green, `.success()`-asserting end-to-end test that does exactly what #1100 is about to forbid:

```rust
.arg("--five_base_index")
.arg(genome.path().join("normal_idx")) // basename; the fake bowtie2 ignores it
```

`normal_idx` appears **once** in the whole repo (that line). No `normal_idx.*` file is ever created — the fixture is `make_genome_fasta_only`, which writes only `genome.fa` (`aligner_cli.rs:43-45`). The test works today because a fake `bowtie2` shell script ignores `-x` entirely. Under Task 2 it fails at `resolve()` before the fake binary is ever consulted.

The plan does not mention this test. §5c's sabotage list would not surface it either, because it only sabotages *new* assertions.

The fix is small but must be in the plan, or an implementer meeting a red test on the byte-frozen-adjacent 5-Base path may reach for weakening the check instead: create six zero-byte files next to the basename. I verified this satisfies the probe — `first_missing` gates on `dir.join(f).is_file()` (`discovery.rs:131-135`), nothing more — and that real `bowtie2` *rejects* a zero-byte set (`readU: Undefined error: 0`), which is irrelevant here because the test's `bowtie2` is fake. This is the same "the files can be empty" property the #1099 commit message already leans on.

### 1.3 🔴 CRITICAL — the placement wording lets the #1099 regression test be hollowed out

§3 says "Placed before `aligner::detect_aligner` (`config.rs:880`)"; Task 2 says "after `aligner` and `genome_arg` are known". `genome_arg` is resolved at `config.rs:804`, so the plan permits anywhere in `804..880` — and `discover_genome_for_run` sits at **`config.rs:861`**, inside that window.

If the check lands before 861, the second loop arm of `five_base_resolve_does_not_require_the_converted_index` (`config.rs:1789-1810`) —

```rust
vec!["--bowtie2", "--five_base_index", "/nonexistent/idx"],
```

— stops reaching genome discovery at all. It still **passes**, because its assertion is the negative `!matches!(e, AlignerError::FaultyIndex { .. })` and the new error is `Validation`. The #1099 guard silently degrades to a tautology on the arm that matters most (the bowtie2 arm is the one with unvalidated CT/GA basenames).

Two things needed: pin the placement to **strictly between 861 and 880**, and give that test's bowtie2 arm a *valid* index (six empty `.bt2` files, as in 1.2) so it keeps exercising `discover_genome_fasta_only`.

### 1.4 🔴 CRITICAL — nothing in §5 pins the environment-variable name, or that `resolve()` reads it at all

This is the gap the brief asked about, and it is the one that ships a false rejection.

Task 2: "`resolve()` reads `BOWTIE2_INDEXES` / `HISAT2_INDEXES` per the resolved aligner and passes it in." §3's whole point is that `index_env` is **injected**, so tests 1-7 all supply the value themselves. Test 8 is a negative case with no environment at all.

Consequence: every one of these implementation slips leaves all eight tests green while making a *working* `BOWTIE2_INDEXES` setup a hard error —

- misspelling either variable name;
- swapping them (reading `HISAT2_INDEXES` on a Bowtie 2 run);
- using `env::var` instead of `env::var_os` and dropping a non-UTF-8 value;
- passing `None` unconditionally at the call site.

The injection design is right (it is what makes the fallback testable without racy `set_var`), but it moves the untested seam to the call site and §5 does not cover the seam.

**Closable at CI, no aligner needed** — exploit the same property test 8 relies on. The check precedes `detect_aligner`, so a run whose index resolves *through the environment* gets past the check and then dies at `detect_aligner` on a CI box with no bowtie2. Assert the **absence** of the message (the #1099 pattern already used at `config.rs:1804`):

```
BOWTIE2_INDEXES=<dir with 6 zero-byte puc.*.bt2>
bismark --illumina_5base --bowtie2 --five_base_index puc --genome <fasta-only> -1 r1 -2 r2
→ stderr must NOT contain "--five_base_index"
```

An integration test sets the variable in a child process, so it is not subject to the `set_var` hazard §3 correctly avoids in unit tests. Add the `HISAT2_INDEXES` twin, and add "pass `None` for `index_env` at the call site" to §5c's sabotage list.

### 1.5 ✅ The guard cannot mis-fire — verified on every path

Enumerated every route into `resolve()`:

- **`--five_base_consensus_from_bam`** — `aligner/mod.rs:178-180` returns before `resolve()` at `:186`. Cannot reach the check. ✅
- **`--five_base_bisulfite_bam`** — `mod.rs:183-185`, same. ✅ (The mutual-exclusion check at `:168` also precedes both, so neither can be skipped.)
- **`--minimap2` 5-Base** — `resolve_aligner` returns `Aligner::Minimap2` (`config.rs:1300`) and `--five_base_index` is rejected outright at `:1293-1299`. The `matches!(aligner, Bowtie2 | Hisat2)` half of the guard excludes it regardless. ✅
- **`--rammap`** — rejected with `--illumina_5base` at `config.rs:1267-1273`. ✅
- **Non-5-Base runs** — `--five_base_index` without `--illumina_5base` dies at `config.rs:1302-1306`, so `illumina_5base` alone is a sufficient guard. ✅
- **`run_config_stub`** (`config.rs:1662`) — constructs a `RunConfig` directly, never calls `resolve`. Unaffected. ✅
- **Other `resolve()` callers** — 15 unit tests in `config.rs` plus `tests/aligner_methylseq_conformance.rs:230`. All non-5-Base except the #1099 pair in 1.3; the conformance test is `--hisat2` without `--illumina_5base`. ✅

So the scope is correct as specified. But see 1.6 — *correct as specified* is not *pinned by a test*.

### 1.6 🟡 No test pins that the check does **not** fire on the minimap2 5-Base route

§5 has seven positive-helper tests and one negative integration test. Nothing asserts the minimap2 5-Base path stays clean. The existing #1099 test's first loop arm (`extra = Vec::new()`) does exercise it, but its assertion is `!matches!(e, FaultyIndex)` — a mis-scoped guard raising `Validation` sails straight through.

One clause closes it: in that same loop, also assert the error text does not contain `--five_base_index` for the no-engine-flag arm.

### 1.7 🟡 §2's "replicates bowtie2's own resolution" overstates what the probe mirrors

The wrapper's presence rule is a **prefix glob for ≥1 file**, not a full-set check:

```perl
my @idx_filenames = glob($idx_basename . "*.bt2{,l}");
```

The full-set requirement belongs to `bowtie2-align-s`, one layer down. Measured consequences:

| Case | bowtie2 verdict | Plan's probe | Same outcome? |
|---|---|---|---|
| 5 of 6 `.bt2` | wrapper accepts, **`bowtie2-align` SEGVs** | reject | ✅ (strictly better) |
| complete `.bt2l` | OK | accept (arm 2) | ✅ **— arm 2 is load-bearing** |
| `-x $D/pu` (prefix collision) | wrapper accepts, align rejects | reject | ✅ |
| `-x $D/puc.1.bt2` | reject | reject | ✅ |
| 6 zero-byte `.bt2` | reject | **accept** | ⚠️ false accept = status quo |

D1's justification survives, but should be phrased precisely: the probe mirrors **the wrapper's env fallback** combined with **the aligner binary's full-set requirement**. Its one measured divergence is a false *accept* (zero-byte files), which is harmless — it restores today's fall-through. I found **no false-reject case in this sweep.** That is worth recording in §7 as measurement rather than leaving the risk stated but untested.

### 1.8 ✅ §1's failure shape reproduces exactly

```
(ERR): "/no/such/idx" does not exist or is not a Bowtie 2 index
error: Bowtie 2 produced fewer PE records than read pairs (desync)
```
left behind a **293-byte truncated BAM** and a **394-byte report**, after printing the full resolved-config summary. #1099's engine-name fix is visible ("Bowtie 2", not "minimap2"). All accurate.

---

## 2. Task 3 — blast radius

### 2.1 ✅ It fixes a real, general bug (not 5-Base-specific)

Measured, prepared pUC19 genome, faithful bisulfite SE run, missing `-o`:

```
error: failed to open BAM ".../outC2_missing/test_R1_bismark_bt2.bam": I/O error: No such file or directory (os error 2)
```

Same on the 5-Base path. So the fix is correctly scoped to *every* run, and framing it as a 5-Base issue is the only thing that is off. (Minor: the plan calls today's message "a bare `No such file or directory`" — it does name the path, so "bare" overstates it.)

### 2.2 🟡 IMPORTANT — `create_dir_all` diverges from Perl on nested paths, and the plan's stated justification is the weaker one

Measured against `legacy_perl/bismark` (option handling at `legacy_perl/bismark:8178-8192` — `chdir` first, `mkdir` on failure, `die` on `mkdir` failure):

| `-o` | Perl v0.25.1 | Plan's `create_dir_all` |
|---|---|---|
| single level, missing | **creates it, run completes** | creates it ✅ same |
| nested, missing parents | **dies:** `Unable to create directory .../pdeep/a/b/ No such file or directory` | **creates the whole tree** ⚠️ diverges |

So Task 3 **restores Perl parity** for the common case — a much stronger argument than the plan's "every sibling entry point already does it", and it should replace it in §7. But it also **exceeds** Perl on the nested case, which is exactly the brief's "could it mask a user typo that today fails loudly": `-o /Users/felix/Desktp/results` currently fails on both Perl and Rust and would begin silently materialising a tree.

The sibling precedent does not transfer cleanly, either: `extractor/output.rs:198` documents itself as matching Perl `make_path` (recursive) — the *extractor's* Perl is recursive; the *aligner's* Perl is single-level `mkdir`.

No byte-identity exposure (creating a directory changes no output bytes, and `perl-oracle` pre-creates its dirs), and I found no test asserting the current failure. So this is a decision to make explicitly, not a blocker:

- **(a) keep `create_dir_all`** — friendlier, matches the rest of the suite; requires a CHANGELOG line under **All tools / bismark (aligner)** saying it applies to *every* run and that nested paths are now created where Perl died. Recommended.
- **(b) `std::fs::create_dir`** — Perl-exact, nested typos still fail loud. Defensible under the repo's faithfulness culture, and one line either way.

Either way, three specifics the plan should nail down:

1. **Use the fail-loud sibling.** The plan cites "`mod.rs:582` / `:651`", which straddles two *different* behaviours: `mod.rs:579` is `create_dir_all(&out_dir).ok()` — **silently swallows the error** — while `mod.rs:648` maps it into a `Validation` naming `--output_dir`. Copy `:648`; `.ok()` violates never-silent.
2. **`create_dir_all("")` is `Ok(())`** — verified empirically, and load-bearing, because `resolve_output` defaults `output_dir` to `PathBuf::new()` (`config.rs:1561`). Worth one line in the plan so nobody "fixes" it with an unnecessary emptiness guard, and so the assumption is recorded if it ever changes.
3. **Placement.** `resolve_output` is called at `config.rs:903`, *after* `detect_aligner` (`:880`) — so a run that fails aligner detection or the new index check creates no stray directory, and test 8's "no `.bam` in the output dir" assertion holds either way. That is better than Perl (which mkdirs in `process_command_line`, before genome validation) and worth stating as intentional. It also means Task 3 and Task 2 compose correctly: a rejected index leaves nothing at all behind.

One more beneficiary the plan does not mention: `five_base_reference_fasta` (`mod.rs:1589-1593`) writes `.bismark_5base_concat_ref.fa` into `output_dir` via `File::create`, so a multi-FASTA 5-Base genome hits the same missing-directory failure earlier than BAM-open. Task 3 covers it; worth a line so the fix is not later narrowed to the BAM writer.

---

## 3. Assumptions

### 3.1 🟡 IMPORTANT — the `HISAT2_INDEXES` risk is inverted

The plan: "verify on oxy before merge. If it does not [behave symmetrically], drop the hisat2 half of the fallback rather than guessing." Under D1 that instruction points the wrong way.

- **Include the fallback, HISAT2 lacks it** → the Rust check is *more permissive* than hisat2. Worst case is a false **accept**: the run proceeds and hisat2 issues its own error. That is exactly today's behaviour — no regression.
- **Omit the fallback, HISAT2 has it** → a user with a working `HISAT2_INDEXES` setup gets a **hard error on a working pipeline**. That is the #1099 failure mode, repeated, and the one thing D1 makes expensive.

Symmetric treatment is the strictly safer default *because* it is unverified. Recommend: keep it symmetric, drop "before merge" from the gate, and record the asymmetry so a later oxy result is read correctly — a negative finding would be a reason to leave the code alone, not to change it. (Reasoning only; `hisat2` is not installed here.)

### 3.2 ✅ Verified-clean assumptions worth recording in the plan

Both are load-bearing for the whole probe and neither is currently stated:

- **The aligner never changes directory.** No `set_current_dir` anywhere in `rust/bismark/src/aligner/`, and no `Command::current_dir` on any aligner spawn (the only `current_dir` in the crate is `genome_prep/indexer.rs:115`). So a relative `--five_base_index` resolves against the same cwd at probe time and at exec time — without this the probe could disagree with the child. Note that Perl *does* `chdir` into `output_dir` (`legacy_perl/bismark:8183`), so the Rust port's cwd model differs; that is pre-existing and out of scope, but it means the probe must be reasoned about against the Rust model, not Perl's.
- **Bismark never touches `BOWTIE2_INDEXES`/`HISAT2_INDEXES`** — no `env_remove`/`.env()` on any spawn, so the child inherits exactly what `resolve()` read. The read-once-and-inject design is sound.

### 3.3 🟡 The error variant is load-bearing for an existing test

`AlignerError::Validation` is the right choice, but the plan does not say *why it cannot be `FaultyIndex`*: `config.rs:1805` asserts `!matches!(e, AlignerError::FaultyIndex { .. })` for precisely this input. Reusing `FaultyIndex` — superficially the better-named variant, and it already carries `{aligner, converted, missing}` — flips that test red for a reason that looks unrelated. Put the constraint in the plan and the commit message. (Not in a source comment: that would be justifying the change in source.)

### 3.4 Line-number drift (implementers navigate by these)

| Plan | Actual |
|---|---|
| `resolve_output (config.rs:1533)` | **1549** |
| `discovery.rs:154-177` (small→large probe) | **157-181** |
| `config.rs:1278-1287` (index-required guard) | 1277-1286 |
| `mod.rs:582` / `:651` | `create_dir_all` at **579** (`.ok()`) and **648** (`map_err`) — see 2.2 |
| `config.rs:880`, `:1293-1298`, `:1302-1306`, `convert.rs:129` | ✅ correct |

---

## 4. Efficiency

Nothing to fix. Worst case is `2 locations × 2 arms × 8 suffixes = 32` `stat` calls, once per run, against a run measured in minutes to hours. `index_suffixes` allocating a `Vec<String>` per arm is irrelevant at this call frequency and matching it to `discover_genome`'s existing shape is worth more than avoiding the allocation.

One ordering point, which the plan already gets right and should keep: the retry must be **per-location two-arm** (basename small → basename large → env small → env large), not per-arm two-location. bowtie2's single glob `*.bt2{,l}` tests small and large *together* at one location before moving on, so location must be the outer loop. §3's wording ("the basename as given, then … `<env>/…`" + "each candidate is checked with the two-arm probe") expresses this correctly — do not let it get reordered during implementation.

---

## 5. Alternatives

### 5.1 Recommended: one new `pub(crate)` item, not two, and probe by concatenation

The plan promotes both `first_missing` and `index_suffixes` to `pub(crate)`, then duplicates the two-arm retry at the new call site. There is a smaller and safer factoring.

`first_missing(aligner, dir: &Path, stem: &str, large: bool)` takes a **directory plus a `&str` stem**, but `--five_base_index` is a single `Option<PathBuf>` (`cli.rs:121`). Using it forces the call site to split with `parent()`/`file_name()` and then `to_str()` — which introduces a `None` case for a non-UTF-8 basename that the plan does not address, in a module whose own header stresses matching on raw bytes so non-UTF-8 names are never silently dropped.

Both problems vanish by mirroring what bowtie2 actually does — **string concatenation**, `glob($idx_basename . "*.bt2{,l}")`, no path decomposition at all:

```rust
/// First expected index file for `basename` that is absent, or `None` if the set is complete.
/// The caller owns the small→large retry.
pub(crate) fn missing_index_file(aligner: Aligner, basename: &Path, large: bool) -> Option<String>
```

built by pushing each suffix onto `basename.as_os_str().to_owned()` (`OsString::push`). That keeps `index_suffixes` private, adds one item instead of two, needs no UTF-8, and is a provable mirror of the wrapper rather than an approximation of it. It also handles a trailing separator the way bowtie2 does (`$D/` → `$D/.1.bt2`) where the `parent()`/`file_name()` split would silently probe `$D.1.bt2` instead — I measured that bowtie2 rejects `-x $D/` either way, so this is cleanliness rather than a live bug, but it is one less thing to reason about.

I would **not** factor the small→large retry itself into a shared helper. `discover_genome` (`discovery.rs:157-181`) needs the retry *coordinated across CT and GA* and needs to know which arm won (`large_index`), so a per-basename helper cannot serve both call sites without growing a return type that fits neither. Two lines of duplication is the right price.

### 5.2 Rejected alternatives — all four hold up

§3's four rejections are each correct, and 1.7 strengthens the separator one (measured) while 3.3 gives the `resolve_aligner` one a second reason beyond the stated ones (its tests assert on selection *and* the #1099 test constrains the variant).

One the plan does not list, worth a sentence for the record: **probe by prefix glob (`<basename>*.bt2*`), exactly the wrapper's rule.** More permissive, so a partial index would pass and reach the SEGV I measured — which is why the plan's stricter full-set rule is the better choice. Recording the rejection shows the stricter rule was chosen, not defaulted into.

---

## 6. Documentation surface — incomplete

Task 4 covers `CHANGELOG.md` only. Two gaps:

- **`docs/src/content/docs/rust/illumina-5-base.md`** — #1099 edited this file, and the `--five_base_index` example sits at line 55-60 with no mention that the index is now checked up front or that `BOWTIE2_INDEXES`/`HISAT2_INDEXES` is honoured. One sentence after that code block.
- **`--five_base_index`'s own help text** (`cli.rs:115-121`) — currently silent on the environment variable. Yes, add a clause: under a hard error, a user whose index resolves through the environment needs to know before they hit it. The error message listing every probed path is good self-diagnosis *after* the fact; the help text is what prevents the ticket. One clause ("also resolved via `$BOWTIE2_INDEXES`/`$HISAT2_INDEXES`, as the aligner itself does") is enough — this is a doc comment, not a code comment, so the one-line rule does not bind it, but keep it short anyway.
- **CHANGELOG scope** — per 2.2, the `--output_dir` change is an all-runs behaviour change and needs its own bullet, not a clause inside the 5-Base entry.

---

## 7. Validation sufficiency — summary

| Failure mode | Covered by §5? |
|---|---|
| Large-index arm dropped | ✅ test 2 (and I confirmed `.bt2l` genuinely requires it) |
| Env fallback dropped | ✅ tests 5, 6 — **at the helper only** |
| Env variable **misnamed / swapped / not read** | 🔴 **no** — see 1.4 |
| Check placed after `detect_aligner` | ✅ test 8 + sabotage |
| Check placed *before* `discover_genome_for_run` | 🔴 **no** — see 1.3 |
| Guard fires on the minimap2 5-Base route | 🟡 **no** — see 1.6 |
| hisat2 **large** (`.ht2l`) arm | 🟡 **no** — test 3 is small-index only |
| Per-aligner suffix arity | ✅ test 3 |
| Partial index | ✅ test 7 |
| Existing `.success()` 5-Base bowtie2 test | 🔴 **turns red** — see 1.2 |
| Output dir created on the faithful path | 🟡 no test proposed at all |

---

## 8. Action items

### Critical — fix before implementation

1. **Add the fixture repair for `five_base_bowtie2_unconverted_index_end_to_end`** (`tests/aligner_cli.rs:6271`) to Task 2: create `normal_idx.{1,2,3,4,rev.1,rev.2}.bt2` as zero-byte files. Without this the plan ships a red test on the 5-Base path. (1.2)
2. **Pin the call-site placement to strictly between `config.rs:861` and `:880`** — after `discover_genome_for_run`, before `detect_aligner` — and give the bowtie2 arm of `five_base_resolve_does_not_require_the_converted_index` (`config.rs:1789`) a valid index fixture, so the #1099 guard keeps exercising discovery instead of passing vacuously. (1.3)
3. **Add an integration test that pins the environment-variable read**, one per aligner: set `BOWTIE2_INDEXES`/`HISAT2_INDEXES` to a directory of zero-byte index files, pass the bare basename, assert stderr does **not** contain `--five_base_index`. CI-safe (the run dies later at `detect_aligner`). Add "call site passes `None` for `index_env`" to §5c. Without this, a misspelled or swapped variable name ships a hard error on a working setup with all eight listed tests green. (1.4)

### Important — resolve in the plan

4. **Restate Task 3's justification and own its divergence.** Perl *does* create a single-level `-o` (`legacy_perl/bismark:8178-8192`) and *dies* on a nested one — measured both. Lead with Perl parity, state the nested-path deviation explicitly, pick (a) `create_dir_all` + a CHANGELOG line or (b) Perl-exact `create_dir`, copy the **fail-loud** sibling at `mod.rs:648` (not `.ok()` at `:579`), and record that `create_dir_all("")` is `Ok(())` because `output_dir` defaults to empty. (2.2)
5. **Flip the `HISAT2_INDEXES` instruction.** Keep the fallback symmetric *because* it is unverified: including it risks a false accept (status quo), omitting it risks a false reject (the #1099 failure mode). Drop "verify before merge" as a gate; keep the oxy check as a follow-up whose negative result means *leave the code alone*. (3.1)
6. **Add the two missing test cases:** hisat2 **large** (`.ht2l`) acceptance, and one clause asserting the guard does not fire on the minimap2 5-Base route. (1.6, §7)
7. **Extend Task 4 to `docs/src/content/docs/rust/illumina-5-base.md` and the `--five_base_index` help text** (`cli.rs:115-121`), and give the `--output_dir` change its own CHANGELOG bullet scoped to all runs. (§6)
8. **Record in §3 that `AlignerError::Validation` is mandatory, not stylistic** — `FaultyIndex` flips `config.rs:1805` red. Plan and commit message, not a source comment. (3.3)

### Optional — improvements

9. **Add one `pub(crate)` item instead of two**, probing by concatenating suffixes onto the basename `OsString` rather than splitting `parent()`/`file_name()` — it mirrors the wrapper's actual `glob($basename . "...")` semantics, keeps `index_suffixes` private, and removes an unaddressed non-UTF-8 `to_str()` failure case. Keep the two-arm retry duplicated; `discover_genome` needs it coordinated across CT/GA and needs to know which arm won, so a shared helper fits neither caller. (5.1)
10. **Record the two verified-clean invariants in §3** — no `set_current_dir`/`Command::current_dir` in the aligner (so a relative basename resolves identically at probe and exec time), and no env scrubbing on aligner spawns. Both are load-bearing and neither is currently stated. (3.2)
11. **Fold the adversarial sweep into §7** in place of the bare "residual" row: partial (SEGV), complete `.bt2l` (OK), prefix collision, full-filename basename, trailing separator, zero-byte set — no false-reject case found; the one divergence is a false *accept*. §7 asks reviewers to attack D1; this is the attack and its result, and it is worth keeping. (1.7)
12. **Fix §2's "replicates bowtie2's own resolution"** to "mirrors the wrapper's env fallback plus the aligner binary's full-set requirement" — the wrapper's own rule is a ≥1-file prefix glob. (1.7)
13. **Correct the line references** in §3/Task 3 (`resolve_output` 1533→1549, `discovery.rs` 154-177→157-181, `mod.rs:582/651`→`579`/`648`). (3.4)
14. **Mention `five_base_reference_fasta`** (`mod.rs:1589`) as a second beneficiary of Task 3, so the fix is not later narrowed to the BAM writer. (2.2)
15. **§6's manual block invokes `bismark` from `PATH`** while the debug build lives at `rust/target/debug/bismark`. Name the binary explicitly, or a stale installed 3.1.0 will silently "confirm" the old behaviour.
