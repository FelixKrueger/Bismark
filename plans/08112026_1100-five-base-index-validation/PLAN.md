# PLAN — #1100: validate `--five_base_index` at resolve time

**Issue:** [#1100](https://github.com/FelixKrueger/Bismark/issues/1100) · split out of [#1099](https://github.com/FelixKrueger/Bismark/issues/1099) (merged `1a4f8fa`)
**Rev:** 2 — dual plan review folded (`PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`). §0 holds the one decision left.
**Branch to create:** `1100-five-base-index-validation` off `dev` (G23; `dev` is at `688d919`)

> Both reviewers: **design sound, §2's central claim survives independent re-derivation**, two Criticals
> (both agreed, both about plan completeness rather than design). Rev-1 corrections in §10.

---

## 0. Decisions

| # | Decision | Outcome |
|---|---|---|
| **D1** | Hard error or warning? | **HARD ERROR** (Felix). Now better supported than rev 1 argued: Reviewer A measured that bowtie2 requires **all six** index files — missing `.2`/`.rev.1`/`.rev.2` **SIGSEGVs** — so the probe is not stricter than the aligner. Both reviewers swept for false-reject cases; **every divergence found runs in the over-accepting (benign) direction** once I1/I2 below are applied |
| **D2** | Include the `--output_dir` fix? | **IN** (Felix) — Task 3 |
| **D3** | Task 3 semantics — Perl creates a single-level `-o` (and warns) but **dies** on a nested one | **(a) `create_dir_all` + Perl's notice + CHANGELOG disclosure** (Felix, 2026-08-11). Restores Perl parity for the common case and matches `mod.rs:648`/`convert.rs:129`; **deliberately exceeds Perl on nested paths**, so `-o a/b/c` with `a` absent now succeeds where Perl died. The notice is what keeps that honest — the user is told what was created |

---

## 1. Current behaviour

`--five_base_index` is accepted unchecked; `five_base_build_argv` passes it as `-x` and the run proceeds until the aligner rejects it. Reproduced by both reviewers:

```
(ERR): "/no/such/idx" does not exist or is not a Bowtie 2 index
error: Bowtie 2 produced fewer PE records than read pairs (desync)
```

leaving a **~300-byte truncated BAM** and a report in `--output_dir`, after printing the full resolved-config summary. The defect is *lateness*, not wording — the message does name the path. #1099 made this reachable: a missing bisulfite index no longer stops these users first.

Only bowtie2/hisat2 are in scope — `--five_base_index` is rejected outright for minimap2 (`config.rs:1293-1299`), which reads the FASTA directly.

## 2. 🔑 The resolution rule, measured twice

#1100 and #1099's review proposed "check only when the basename contains a path separator". **That is false**, and both reviewers reproduced all six rows independently:

| `-x` argument | `BOWTIE2_INDEXES` | Result |
|---|---|---|
| `puc` | set | OK |
| `puc` | unset | fails |
| **`./puc`** | set | **OK — the separator does not disable the fallback** |
| absolute `$D/puc` | — | OK |
| `/definitely/not/here/puc` | set | fails |
| `nope/puc` | set | fails |

Mechanism, from the wrapper (`bowtie2:379-398`): `glob($idx_basename . "*.bt2{,l}")` first; **only if that is empty**, retry `File::Spec->catfile($ENV{BOWTIE2_INDEXES}, $idx_basename)`.

Two precision corrections from review, both folded below:

- **The wrapper's own presence rule is a ≥1-file prefix glob**, not a full-set check — the all-files requirement lives one layer down in `bowtie2-align-s`. So the probe mirrors *the wrapper's env fallback* **plus** *the aligner binary's full-set requirement*, which is what makes it safe rather than strict (D1).
- **The env arm is gated on the first glob being empty.** A *partial* index at the given basename therefore **blocks** the fallback in bowtie2 (measured: partial → `Could not open index file …` + SIGSEGV). §3 mirrors that gate.

**`HISAT2_INDEXES` — resolved from source; no oxy trip needed.** Upstream `hisat2:382-391` globs `"$ENV{HISAT2_INDEXES}/$idx_basename" . "ht2{,l}"` — **no `*`, no `.`** — so it cannot match a real `puc.1.ht2`. The variable is effectively broken upstream. Keep our fallback symmetric anyway: it can then only **over-accept** (falling through to today's error, no regression), and it is already correct the day upstream fixes the glob. Omitting it would be a false *reject* the day that happens — the expensive direction under D1.

## 3. Design

One `pub(crate)` entry point, **in `discovery.rs`** — the module that already owns per-aligner suffix arity and the small→large retry. This keeps `first_missing`/`index_suffixes` **private** (rev 1 would have promoted both) and gives the env-var mapping a home next to the index knowledge it belongs to.

```rust
// discovery.rs
pub(crate) fn validate_unconverted_index(
    aligner: Aligner,
    basename: &Path,
    index_env: Option<&OsStr>,   // injected; never read from the environment here
) -> Result<()>

pub(crate) fn index_env_for(aligner: Aligner) -> Option<OsString>   // BOWTIE2_INDEXES / HISAT2_INDEXES / None
```

**Probe, mirroring the wrapper exactly:**

1. If **any** `<basename>*.{bt2,bt2l}` (or `.ht2/.ht2l`) file exists → the env arm is skipped (the wrapper's gate).
2. At the basename: full-set small arm, else full-set large arm. Complete → `Ok`.
3. Else, only if step 1 found nothing **and** the env var is set: the same two arms at `<env> + "/" + <basename>`.
4. Else → `AlignerError::Validation` naming the flag, the aligner and **every path probed**.

**Build candidate paths by `OsString` concatenation, never `Path::join` or `parent()`/`file_name()`.** Three reasons, all measured:

- `Path::join("/opt/idx", "/no/such/idx")` **replaces** with the absolute RHS, whereas Perl's `catfile` concatenates (`/opt/idx//no/such/idx`). So `join` makes rev 1's own example error text unreachable (it would print the same path twice) **and** creates a real false rejection: `-x /idx/puc` with `BOWTIE2_INDEXES=/opt` and the index at `/opt/idx/puc` works under bowtie2.
- `--five_base_index ..` and `/` give `file_name() == None`, so a split-based probe panics on the very hard-error path this change exists to make clean.
- A non-UTF-8 basename would need `to_str()`, which can fail — in a module whose header stresses matching on raw bytes so such names are never silently dropped.

Concatenation is also what bowtie2 itself does (`$basename . ".1.bt2"`), so it is a provable mirror rather than an approximation.

**`AlignerError::Validation` is mandatory, not stylistic:** `config.rs:1805` asserts `!matches!(e, FaultyIndex { .. })` for exactly this input, so reusing the better-named `FaultyIndex` variant would flip #1099's guard red for an unrelated-looking reason.

**Call site: strictly between `discover_genome_for_run` (`config.rs:861`) and `detect_aligner` (`:880`).** Rev 1 said "after `genome_arg`, before `detect_aligner`", which permits 805–879 — and anything before 861 makes #1099's guard vacuous (§5e). Genome errors should also win: a missing genome is the more fundamental input error. `resolve()` calls `index_env_for(aligner)` and injects the result.

Injection (not `env::var` inside the helper) is what makes the fallback testable: Rust tests share a process and run in parallel, so `set_var` is racy and `unsafe` in this edition. Integration tests set it per-child via `.env()`, which is safe.

**Invariants this rests on, both verified:** no `set_current_dir` or `Command::current_dir` anywhere in `rust/bismark/src/aligner/` (so a relative basename resolves identically at probe time and exec time — note Perl *does* `chdir` into `output_dir`, so this is the Rust model, not Perl's); and no env scrubbing on any aligner spawn, so the child inherits exactly what `resolve()` read.

### Rejected alternatives

- **Separator-based check** — refuted by §2.
- **`Path::join` for the env candidate** — refuted above.
- **Splitting into `(dir, stem)` to reuse `first_missing`** — panics on `..`/`/`, needs UTF-8.
- **Reuse `discover_genome`** — hard-codes `Bisulfite_Genome/CT_conversion/BS_CT`.
- **Validate inside `resolve_aligner`** — `illumina_5base_engine_selection` (`config.rs:2302`) calls it directly with the literal basename `"idx"`; a presence check there breaks it.
- **Probe only `.1.bt2` per arm** (the wrapper's own rule) — a partial index then reaches a SIGSEGV; catching it early is worth more.
- **Factor the small→large retry into a shared helper** — `discover_genome` needs it coordinated across CT *and* GA and needs to know which arm won (`large_index`); a per-basename helper fits neither caller. Two lines of duplication is the right price.
- **Warn instead of erroring** — settled by D1.
- **A `--force`-style escape hatch** for stub aligners — declined; the answer is fixture files (Task 2b).

## 4. Tasks

### Task 1 — `discovery.rs`: the probe
`validate_unconverted_index` + `index_env_for` as §3. `first_missing`/`index_suffixes` stay **private**. One sentence noting `--large-index` is unreachable for this route (`five_base_build_argv` emits only `-x`/`-1`/`-2`), so the two arms cover every case.

### Task 2 — `config.rs`: the call site
Call from `resolve()` **between `:861` and `:880`**, guarded by `cli.illumina_5base && matches!(aligner, Bowtie2 | Hisat2)`. The existing guards make `five_base_index` non-`None` there (`config.rs:1277-1286`) — prefer `if let Some(idx)` over `.expect()` on a validation path.

### Task 2b — repair two existing tests (**not optional**; both reviewers flagged this as Critical)
1. **`five_base_bowtie2_unconverted_index_end_to_end`** (`tests/aligner_cli.rs:6271`) passes `--five_base_index <genome>/normal_idx` with **no such files** and currently passes, because its fake bowtie2 ignores `-x`. Task 2 turns it red. Write six zero-byte `normal_idx.{1,2,3,4,rev.1,rev.2}.bt2` files — `first_missing` gates on `is_file()` only. It is the sole end-to-end exercise of the bowtie2 5-Base route, so it must be fixed, not deleted.
2. **`five_base_resolve_does_not_require_the_converted_index`** (`config.rs:2306`) — give its bowtie2 arm a valid stub index too, so it keeps reaching `discover_genome_for_run` rather than stopping at the new check.

### Task 3 — `--output_dir` auto-created (D2 in, D3 = `create_dir_all` + notice)
In **`resolve_output`** (`config.rs:1549`) — one choke point, and it runs *after* `detect_aligner`, so a rejected index leaves nothing behind. Copy the **fail-loud** sibling at `mod.rs:648` (`map_err` → `Validation` naming `--output_dir`), **not** `mod.rs:579`'s `create_dir_all(&out_dir).ok()`, which silently swallows a real mkdir failure; fold a one-line unification of that site in. Emit Perl's notice (`Created output directory <dir>!`) — it is Perl's behaviour, free, and it answers the "created something the user didn't intend" risk from the user's side. `create_dir_all("")` is `Ok(())`, which matters because `output_dir` defaults to `PathBuf::new()`.

Beneficiaries beyond the BAM writer: `five_base_reference_fasta` (`mod.rs:1589`) writes the multi-FASTA concat there too. `--temp_dir` is **already** auto-created (`convert.rs:129`), so the output-dir-only scope is deliberate.

### Task 4 — CHANGELOG + docs
- `CHANGELOG.md` → `## Unreleased` → `### bismark (aligner)`: the new early failure; that `BOWTIE2_INDEXES`/`HISAT2_INDEXES` are honoured; **and the user-visible consequence of D1** — a *stub or wrapper* `bowtie2` reached via `--path_to_bowtie2` that does not need a real index will now be rejected (conda shims, cluster wrappers; the repo's own suite was an instance). Give `--output_dir` its **own bullet**, scoped to **all runs** (not just 5-Base): it is now created if absent, with a notice, restoring Perl's single-level behaviour — and, deliberately unlike Perl, nested parents are created too rather than the run dying.
- `docs/src/content/docs/rust/illumina-5-base.md` — one sentence after the `--five_base_index` example.
- `--five_base_index`'s help text (`cli.rs:115-121`) — one clause noting the env-var fallback, so a user learns it before hitting the error rather than from it.

## 5. Tests

`resolve`-level tests **cannot assert `Ok`** (`detect_aligner` at `:880` is `resolve()`'s only exec; CI installs minimap2 + samtools only). The check precedes it, so negatives are CI-assertable; positives go through the helper.

### 5a. Unit — `validate_unconverted_index` (no binary needed)
1. `complete_small_index_accepted` — 6 `.bt2` → `Ok`.
2. **`complete_large_index_accepted`** — 6 `.bt2l` → `Ok`. Fails if the large arm is dropped.
3. `hisat2_arity` — 8 `.ht2` → `Ok`; 6 → `Err`. **Plus 8 `.ht2l` → `Ok`** (the hisat2 large arm, missing from rev 1).
4. `missing_index_rejected` — empty dir → `Err`, message contains `--five_base_index`.
5. **`env_fallback_accepted`** — bare `puc` absent at cwd, present under the injected dir → `Ok`.
6. **`env_fallback_applies_to_a_separator_basename`** — `./puc` → `Ok`. The §2 finding; red under a separator heuristic.
7. **`env_fallback_is_skipped_when_the_basename_partially_matches`** — 1 of 6 at the basename, complete set under the env dir → **`Err`**. Mirrors the wrapper's gate.
8. `partial_index_rejected` — 5 of 6 → `Err` naming the missing file.
9. **`pathological_basenames_error_not_panic`** — `..`, `/`, trailing dot. The I2 regression test.
10. **False-rejection guards** (the class D1 makes expensive): symlinked index files, symlinked index *directory*, an absolute basename, a `../`-relative basename → all `Ok`.

### 5b. Integration — `tests/aligner_cli.rs`
11. `five_base_index_missing_fails_early` — nonexistent basename over a FASTA-only genome: non-zero exit, stderr contains `--five_base_index`, **must NOT contain `desync`** or `panicked`, and **no `.bam` and no report** in the output dir. Needs no aligner (the check precedes `detect_aligner`).
11b. **`output_dir_is_created_with_a_notice`** — a run with a nonexistent `-o` (nested, to pin D3's deviation) succeeds and stderr contains `Created output directory`. The notice is the only signal a typo produces, so it needs its own assertion. Also assert a rejected index creates **nothing**, which holds because `resolve_output` runs after `detect_aligner`.
12. **`five_base_index_resolved_via_env_is_accepted`** — 🔑 *the gap neither rev-1 test covered.* Set `BOWTIE2_INDEXES` (via `.env()`, so no `set_var` hazard) to a dir of zero-byte index files, pass the bare basename, assert stderr does **NOT** contain `--five_base_index`. Plus the `HISAT2_INDEXES` twin. Without this, a misspelled or swapped variable name, `env::var` instead of `var_os`, or a hard-coded `None` at the call site all ship a hard error on a working setup with every unit test green.
13. **`five_base_minimap2_route_is_not_index_checked`** — the default 5-Base route must not raise `--five_base_index`. (Or one added clause on the #1099 test's first loop arm.)

### 5c. Sabotage — each new assertion observed failing once
Drop the large arm → 2, 3 red. Drop the env fallback → 5, 6, 12 red. Drop the wrapper's gate → 7 red. **Pass `None` for `index_env` at the call site → 12 red.** Move the check before `:861` → the #1099 guard stops reaching discovery (assert it, don't just reason about it). Move it after `detect_aligner` → 11 red on a box without bowtie2.

## 6. Verification

```bash
cargo test -p bismark --lib aligner::{config,discovery}
cargo test -p bismark --test aligner_cli five_base
cargo test -p bismark
cargo fmt -p bismark -- --check
cargo clippy -p bismark --all-targets -- -D warnings
cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings
cargo clippy -p bismark --all-targets --features binseq-input -- -D warnings   # rust_ci.yml:135
```

Manual — **invoke `rust/target/debug/bismark` explicitly**, never `bismark` from `PATH` (a stale installed 3.1.0 would silently "confirm" the old behaviour). Unique scratch dir per N7:

```bash
B=rust/target/debug/bismark
G=$TMPDIR/g1100_$$; mkdir -p "$G" "$TMPDIR/o1100_$$"
gzip -dc test_files/pUC19.fa.gz > "$G/pUC19.fa"

# 1. the #1100 repro: one loud error, and NOTHING written
$B --illumina_5base --bowtie2 --five_base_index /no/such/idx --genome "$G" \
   -1 test_files/test_R1.fastq.gz -2 test_files/test_R2.fastq.gz -o "$TMPDIR/o1100_$$"

# 2. positive control — a REAL index must still run
bowtie2-build -q "$G/pUC19.fa" "$G/puc"
$B --illumina_5base --bowtie2 --five_base_index "$G/puc" --genome "$G" …

# 3. env fallback (the case a naive check breaks)
cd "$TMPDIR" && BOWTIE2_INDEXES="$G" $B --illumina_5base --bowtie2 --five_base_index puc …

# 4. bare basename resolved from CWD, no env var — not unit-testable (cwd mutation is the
#    same shared-process hazard as set_var), so it must be checked here
cd "$G" && $B --illumina_5base --bowtie2 --five_base_index puc …
```

**Open for oxy** (no longer a merge blocker — §2 answered the env question from source): does hisat2 require all **8** `.ht2` files? `discover_genome` already ships that assumption for bisulfite runs, so it is not new risk, but it is five minutes on a box with hisat2.

## 7. Risks

| Risk | Status |
|---|---|
| **A false rejection breaks a working pipeline** — the #1099 failure mode repeated | **Bounded by measurement, not just accepted.** Both reviewers swept: partial index (bowtie2 SEGVs — we reject earlier), complete `.bt2l` (accept), prefix collision, full-filename basename, trailing separator/dot, directory-as-basename, symlinked files and dirs, doubled separator, dangling symlink, zero-byte set, case-differing basename on APFS. **No false-reject case found.** Every divergence runs in the over-accepting direction — i.e. falls through to today's behaviour. The two that *would* have introduced false rejections were `Path::join` and the `(dir, stem)` split, both removed in §3 |
| The probe is stricter than the aligner | **No** — bowtie2 requires all six files; missing `.2`/`.rev.*` **SIGSEGVs** (measured) |
| **Stub/wrapper aligners now fail** | Real, and a direct consequence of D1. Disclosed in the CHANGELOG (Task 4); the repo's own suite was an instance and is fixed in Task 2b |
| Task 3 creating a tree the user did not intend — e.g. `-o /Users/felix/Desktp/out` now materialises silently where Perl died | **Accepted** (D3 = a). Mitigated by emitting Perl's `Created output directory …` notice, so a typo is visible in the run's own output rather than discovered later. A test should assert the notice fires, since it is now the only signal |
| `HISAT2_INDEXES` unverified upstream | Answered from source (§2): broken upstream, and symmetric treatment can only over-accept |

## 8. Out of scope

`discover_genome`'s bisulfite check; `--five_base_index` for minimap2 (already rejected); Perl's `chdir`-into-`output_dir` cwd model; 3.2.0, #1095, `rust/README.md`.

## 11. Implementation notes (branch `1100-five-base-index-validation`)

| Change | Where |
|---|---|
| `validate_unconverted_index` + `index_env_for` + `index_env_name`; candidates built by appending suffixes to the basename `OsString` | `discovery.rs` (`first_missing`/`index_suffixes` stayed **private**, as §3 intended) |
| Call site, guarded on `illumina_5base && (Bowtie2\|Hisat2)`, **between `discover_genome_for_run` and `detect_aligner`** | `config.rs` |
| `--output_dir` created with Perl's `Created output directory <dir>!` notice, fail-loud | `config.rs::resolve_output` |
| `mod.rs`'s silent `create_dir_all(&out_dir).ok()` unified onto the erroring form | `mod.rs` |
| Two predicted fixtures repaired + `make_stub_bowtie2_index` helper | `tests/aligner_cli.rs`, `config.rs` |
| Env-var clause on `--five_base_index`; docs paragraph; two CHANGELOG bullets | `cli.rs`, `illumina-5-base.md`, `CHANGELOG.md` |

**Deviation from §5 (documented).** Planned unit test 7
(`env_fallback_is_skipped_when_the_basename_partially_matches`) **was deleted, not kept**: as written
it used an *absolute* basename, so the env-join could never resolve and it passed whether or not the
gate existed — vacuous. The gate is only observable with a **relative** basename, which needs cwd
control: unsafe in-process (the same shared-process hazard as `set_var`), safe in a child. It is now
`partial_index_at_the_basename_blocks_the_env_fallback` in `tests/aligner_cli.rs`, using
`.current_dir()`, and it **is** sabotage-verified. The same move also closed the one positive form
§5 could not reach — `five_base_index_resolved_from_cwd_is_accepted` (Reviewer A's F6), which had no
coverage at all.

**Sabotage record** — every new assertion observed failing once:

| Sabotage | Observed |
|---|---|
| Probe the small arm only | **3 red** (`complete_large_index_accepted`, `hisat2_arity_and_large_arm`, +1) |
| Drop the env fallback | **2 red** (`env_fallback_accepted`, `env_fallback_applies_to_a_separator_basename`) |
| Remove the wrapper's gate (`if true`) | **1 red** (`partial_index_at_the_basename_blocks_the_env_fallback`) |
| Call site passes `None` for `index_env` | **1 red** (`five_base_index_resolved_via_env_is_accepted`) — the seam Reviewer B identified as the gap |

**Iteration log**

`#1` Anchored an `Edit` on `fn five_base_bowtie2_unconverted_index_end_to_end()` rather than on its
attributes, which orphaned its `#[cfg(unix)] #[test]` onto the first inserted test — **the identical
mistake as #1099's H1**, one day later. Anchor on the doc comment/attributes, not the signature.

`#2` 🔑 **Destroyed all `discovery.rs` work with `git checkout -- <file>`** while "reverting" a
sabotage. The changes were uncommitted, so there was nothing to revert *to*. Worse, sabotages B and C
then reported "0 failing" against a file that could no longer compile — a false green. Restored from
context; re-ran the sabotages with a file copy. **Never revert a sabotage with `git checkout` on
uncommitted work**, and note `cp` is aliased to `cp -i` here, so a restore prompts and hangs.

`#3` Sabotage C then caught the vacuous test 7 above — the sabotage discipline paying for itself by
finding a bad *test* rather than bad code.

`#4` Reported "FMT CLEAN" from `cargo fmt -- --check | head -20 && echo CLEAN`, which reports
**`head`'s** exit status. `fmt` had two real diffs and CI runs it as its own job. Assert on the
command's own exit code (`if cargo fmt …; then`), never through a pipe.

**Counts:** `aligner::discovery` 25 → 36; `aligner_cli` 116 → 121; full suite 80 suites, 0 failures;
`cargo fmt --check` clean (verified by exit code).

## 12. Phase-5 review fixes (applied)

Dual code review: **A Approve · B hold-for-C1** · plan-manager **INCOMPLETE** (45 items, 38 DONE, 5 PARTIAL, 1 MISSING, 1 documented deviation — none behavioural). Felix: *"fix them all, and reject converted indexes too."*

### 🔴 C1 — a real false rejection, verified independently before acting

Reviewer B measured a working configuration this change rejected; Reviewer A's 20-form sweep reported
**none**. I reproduced B's case myself rather than picking a reviewer: a directory holding only
`puc.2.bt2` with `BOWTIE2_INDEXES` pointing at a real index — **real bowtie2 exit 0, our check exit 1**,
and the env-unset control exit 1 proving the variable was what rescued bowtie2.

The reviews reconcile: A's "partial (1 of 6)" row had `.1.bt2` present, where bowtie2 legitimately dies.
**The env fallback lives at two layers** — the wrapper script (wide prefix glob) and the *binary*
(`adjustEbwtBase`, gated on `<base>.1.<ext>` alone). The binary decides. `any_index_file_at` mirrored
the wrapper, so any non-`.1` file suppressed a fallback the aligner still takes.

**Fixed** by `first_index_file_at`, gating on `<basename>.1.{bt2,bt2l}` only, with the `.1`-is-first
invariant recorded on `index_suffixes` (the thing it constrains). Verified both directions afterwards:
the C1 case is accepted, and a `.1.bt2`-only partial index is still rejected — matching bowtie2 in each.

**§2/§7/D1 corrected:** "no false-reject case found" was **false as written**, and D1's "not stricter
than the aligner" held only against the wrapper. §2's `HISAT2_INDEXES`-is-broken-upstream conclusion was
also wrong for the same reason — see below.

### 🔑 `$HISAT2_INDEXES` resolved from upstream source (the reviewers' one real disagreement)

A read hisat2's wrapper glob (`<env>/<basename>ht2{,l}` — no `*`, no `.`) and concluded the variable is
broken upstream; B doubted it. Settled by reading `DaehwanKimLab/hisat2`'s **`gfm.cpp`**:
`adjustEbwtBase` probes `<base>.1.<ext>`, and on failure joins `getenv("HISAT2_INDEXES") + "/" + base`.
So the variable **works**, via the binary; the wrapper is not the deciding layer. Consequences: the
docs/CHANGELOG/help claims are **correct as written** (A's M2 and B's H2 both dissolve), the C1 fix is
symmetric across both aligners **by source, not assumption**, and the `OsString` concatenation matches
what both binaries do. §6's oxy trip for this question is retired.

### Applied

| Fix | Raised by |
|---|---|
| C1 gate narrowed to `.1.<ext>`; `.1`-is-first invariant on `index_suffixes` | B C1 (verified by me) |
| **Converted-index rejection** — `reject_converted_five_base_index`, canonicalising so `..`/symlinks cannot slip past | A M3 + Felix |
| Error now says when the variable was **set but deliberately not consulted**, naming the suppressing file — previously it told users to build an index they already had | A M1, B M2 |
| `Probed:` → `Missing:`; CHANGELOG "every path probed" → "the files it looked for" | A L1, B M1 |
| Gap 1 — the `--large-index`-unreachable note | PM Gap 1, A L3 |
| Gap 2 — a rejected run now asserts `!nested.exists()`, so "nothing written" covers "never created" | PM Gap 2, B M3 |
| Gap 3 — `index_env_name_maps_each_aligner` + `hisat2_env_fallback_accepted` | PM Gap 3, B H1 |
| Gap 4 — the two unrun sabotages executed (below) | PM Gap 4 |
| `resolve_output` comment 3 lines → 2, `legacy_perl` line numbers dropped; three justification comments trimmed | B L1 |
| Consensus mkdir error no longer names `--output_dir` when unset | B L3 |
| CHANGELOG: Perl-notice verbatim-parity claim softened; two-layer resolution explained; converted-index rejection documented | B L2 |

### Sabotage record — all **six** §5c claims now verified

| Sabotage | Observed |
|---|---|
| Probe the small arm only | 3 red |
| Drop the env fallback | 2 red |
| Remove the fallback gate | 1 red (`partial_index_at_the_basename_blocks_the_env_fallback`) |
| Call site passes `None` for `index_env` | 1 red (`five_base_index_resolved_via_env_is_accepted`) |
| **Check moved after `detect_aligner`, bowtie2 off `PATH`** (the CI shape) | **1 red**, with a same-PATH control passing — so the property is pinned, not incidental |
| **#1099 re-broken (discovery unconditional)** | **2 red** (`faithful_resolve_still_requires_the_converted_index`, `discover_genome_for_run_skips_the_index_check_for_five_base`) — Task 2b's stub index did **not** weaken #1099's guard |

### Iteration log (this round)

`#5` Reported sabotage results from `env PATH=/usr/bin:/bin cargo test` — which removed **cargo** from
`PATH`, so both runs printed nothing and I read the silence as a result. Twice in one session, silence
mistaken for signal. Redone with `$HOME/.cargo/bin` kept and Homebrew dropped, **plus a control run**
to prove the difference came from the sabotage.

`#6` A second sabotage attempt died on a stale `cwd` (`cd rust` from a directory already inside
`rust`). Absolute paths throughout, or `env -C`, is the only reliable shape here.

**Counts after the fix round:** `aligner::discovery` 38, `aligner::config` 46, `aligner_cli` 122.

## 10. Rev-1 corrections (for the record)

1. Would have **broken a green test** (`five_base_bowtie2_unconverted_index_end_to_end`) with no task for it — both reviewers Critical.
2. Placement wording permitted 805–879, which would have made #1099's guard **vacuously green** on its bowtie2 arm — both reviewers Critical.
3. **No test pinned the env-var read**; the injection design moved the untested seam to the call site (B).
4. `Path::join` ≠ `catfile`: rev 1's example error text was unreachable and the env arm carried a real false rejection (A).
5. The `(dir, stem)` split **panics** on `--five_base_index ..` or `/` (A).
6. Missed the wrapper's **ordering gate** — a partial index blocks the env fallback (A).
7. The `HISAT2_INDEXES` contingency was **backwards**: keeping it symmetric is the safe direction, and the question is answerable from upstream source rather than needing oxy (both).
8. Cited `mod.rs:582` as the `create_dir_all` precedent; the real sites are `:579` (silent `.ok()` — the wrong one to copy) and `:648`.
9. Perl's `-o` behaviour was unstated: it creates single-level **and warns**, but **dies** on nested (both).
10. Line-number drift: `resolve_output` 1533→**1549**; `discovery.rs` 154-177→**157-181**; `mod.rs` 582/651→**579/648**.
11. Docs surface incomplete (help text, docs page, separate `--output_dir` bullet) and §6 omitted the `binseq-input` clippy job and invoked `bismark` from `PATH`.
