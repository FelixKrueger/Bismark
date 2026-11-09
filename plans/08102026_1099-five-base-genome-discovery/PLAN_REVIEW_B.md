# PLAN_REVIEW_B — #1099: `--illumina_5base` must not require the converted CT/GA indexes

**Reviewer:** B (independent; no shared state with Reviewer A)
**Target:** `/Users/fkrueger/Github/Bismark/plans/08102026_1099-five-base-genome-discovery/PLAN.md` (rev 0)
**Date:** 2026-08-10 · branch `dev` @ `6ecf8a7`
**Method:** every factual claim in the plan checked against source; bug and behaviour reproduced with
`rust/target/debug/bismark`; baseline `cargo test --lib aligner::discovery` run (21 passed).

**Verdict: the diagnosis and the chosen design are correct and I would ship them essentially as
written. The test plan is not shippable as specified — two of the six proposed tests cannot pass in
the Rust CI job, and the one automated gate that would actually prove #1099 fixed is missing.** Fix
§5/§6 and this is a clean one-branch fix.

---

## 1. Verification of the plan's factual claims

Everything I could check, I checked. The plan is unusually accurate about its own code references.

| Plan claim | Verdict | Evidence |
|---|---|---|
| `config::resolve` resolves the genome unconditionally at `config.rs:847` | **True** | `rust/bismark/src/aligner/config.rs:847` |
| `discover_genome` errors `FaultyIndex` unless the CT/GA set is complete (`discovery.rs:143-161`) | **True** | `discovery.rs:143-166` (small-then-large fallback) |
| bowtie2/hisat2 5-Base aligns against `--five_base_index` (`mod.rs:1558-1562`) | **True** | `mod.rs:1557-1569` |
| default (minimap2) 5-Base aligns the FASTA directly (`mod.rs:1571-1573`) | **True** | `mod.rs:1571-1577` |
| converted basenames read only on bisulfite paths (`mod.rs:1211-1212`, `1345/1350`, `4633-4634`) | **True** | plus one more reader the plan missed — see §2.1 |
| `large_index` read only by a `Debug`/display impl (`config.rs:1609`) | **True** | `RunConfig::summary()`; printed at `mod.rs:276` |
| the `if !cli.illumina_5base` precedent sits one line above, `config.rs:838` | **True** | `config.rs:837-839` |
| present in released 3.1.0 | **True** | `git show bismark-rust-v3.1.0:…/config.rs` — same call, ~line 596 (plan says 597; immaterial) |
| with 12 required index files present but **empty**, a 5-Base run completes | **True — reproduced** | 14 zero-byte files (`BS_{CT,GA}.{1,2,3,4,rev.1,rev.2}.bt2` + `.mmi`) → full run, report + BAM written |
| `--combined_index` already rejected for 5-Base | **True** | `config.rs:820-830`, *before* line 847 |

Both failure modes reproduced verbatim:

```
$ bismark --illumina_5base --bowtie2 --five_base_index … --genome <fasta-only dir> -1 … -2 …
error: the Bowtie 2 index of the C->T->converted genome seems to be faulty or non-existant ('BS_CT.1.bt2l').

$ bismark --illumina_5base --genome <fasta-only dir> -1 … -2 …
error: the minimap2 index of the C->T->converted genome seems to be faulty or non-existant ('BS_CT.mmi').
```

The second one is worth naming in the plan explicitly: on the **default** route the demanded file is
`BS_CT.mmi`, which `bismark_genome_preparation` only ever produces under `--minimap2`. So a user who
*did* run genome preparation the normal (Bowtie 2) way is still rejected on the default 5-Base engine.
That is a strictly larger blast radius than "genome folder without `Bisulfite_Genome/`" and belongs in
the §2 behaviour table as its own row — it is the row that shows the fix helps users who followed the
documentation.

---

## 2. Logic review

### 2.1 Root cause: complete. No other call site or subcommand reaches the rejection.

I enumerated every reader.

- `discover_genome` has **exactly one** non-test call site in the whole workspace: `config.rs:847`.
- The two standalone 5-Base entry points **return before `resolve`** — `mod.rs:178-180`
  (`--five_base_consensus_from_bam`) and `mod.rs:183-185` (`--five_base_bisulfite_bam`), with
  `resolve` only at `mod.rs:186`. The consensus path calls `discovery::discover_fastas` directly
  (`mod.rs:546`), i.e. it already does exactly what the plan is adding for the align path. They are
  immune, and the plan is right not to touch them.
- No other subcommand (extractor, bedgraph, c2c, bam2nuc, nome_filtering) goes through
  `aligner::discovery` at all; each has its own genome-folder handling.

One reader the plan's list omits: `parallel.rs:597`,
`estimate_index_bytes(&cfg.genome.ct_index_basename)` inside `emit_memory_warning`. It is
**unreachable** for 5-Base — `pipeline()` short-circuits on `config.five_base` at `mod.rs:943-953`,
*before* the `n > 1 → parallel::run_*_multicore` branch — and it degrades gracefully anyway
(`read_dir(parent).ok()?` → `None` → the warning just omits the GB estimate). No action needed, but
the plan's §1 sentence "the converted basenames are read only on bisulfite paths" should name it, or
a reader auditing the invariant will find it and lose confidence in the list.

### 2.2 Flag interactions: all safe, and each for a *checked* reason

| Flag with `--illumina_5base` | Interaction with skipping discovery | Why safe |
|---|---|---|
| `--combined_index` (+`_sequential`/`_single_pass`/`_parallel`) | none | rejected at `config.rs:820-830`, before line 847; so the `combined_index_basename.is_none()` guard at `config.rs:850` and every `mod.rs` reader inside a `if config.combined_index` branch are unreachable. `None` is correct. |
| `--rammap` | none | rejected in `resolve_aligner` (`config.rs:1251-1257`), which runs at `resolve`'s **first line** (624). The 5-Base aligner set is exactly {Bowtie2, Hisat2, Minimap2} — which is why the sibling needs no `aligner` parameter. |
| `--hisat2` | same as `--bowtie2` | `resolve_aligner` returns `Hisat2` at `config.rs:1271-1275`; `five_base_build_argv` treats it identically (`-x <five_base_index>`). |
| `--multicore` / `--parallel N` | none | 5-Base consumes N as intra-instance `-p`/`-t` (`five_base_aligner_options`, `mod.rs:1502-1526`); the fork path is never entered (§2.1). |
| `--five_base_consensus_from_bam` | none | bypasses `resolve` entirely (§2.1). |
| `--minimap2` | none | co-occurrence is explicitly allowed (`config.rs:1248`); resolves to `Minimap2` either way. |

So §3's `large_index: false` / `combined_index_basename: None` are not just "display-only" hand-waves
— they are provably unobservable. Worth stating that way in the plan, because "display-only" invites a
future reader to assume it's a shortcut.

### 2.3 The prologue extraction has an unguarded trap the plan's risk table dismisses

§7 row 2 says: *"Both error cases preserved verbatim; existing discovery tests cover them."* **The
second half is false.** `AlignerError::GenomeFolder` has **zero** test coverage — not in
`discovery.rs`'s 21 unit tests, not in `aligner_cli.rs`, nowhere in the repo (grepped
`GenomeFolder` + `"failed to access genome folder"` across `src/` and `tests/`).

That matters more than a missing test usually would, because the natural way to write the helper is
wrong and still compiles:

```rust
fn absolute_genome_dir(genome_arg: &Path) -> Result<PathBuf> {
    let dir = std::fs::canonicalize(genome_arg)?;   // ← silently becomes AlignerError::Io
    …
}
```

`AlignerError` has `Io(#[from] std::io::Error)` (`error.rs:15-16`), so `?` on `canonicalize` type-checks
and swaps a clear *"failed to access genome folder … (does it exist and is it a directory?)"* for a bare
*"I/O error: No such file or directory"* — a user-visible regression on the **faithful** path, with no
test anywhere to catch it. The plan's test 3 covers only the new sibling.

**Action: add a mirror test for `discover_genome` itself** (nonexistent path → `GenomeFolder`; path to
a regular file → `GenomeFolder`). Two lines each, and they are the only thing standing between the
extraction and that regression.

### 2.4 Scope boundary (§8): two of three calls are right, one has a false rationale

- **`-o` not auto-created — agree it is separate, but the plan's own §6 recipe trips over it.** Verified:
  `bismark --illumina_5base … -o <nonexistent>` fails with
  `error: failed to open BAM "…/test_R1_bismark_mm2_pe.bam": I/O error: No such file or directory`,
  *after* printing the full resolved-config summary and `>>> Writing 5-Base mapping results to … <<<`.
  §6 says "it will then fail on the bogus index — that is the aligner, not us". It will not; it will
  fail on the output directory, with a message that reads exactly like the fix not working. See §5.3.
  (Side evidence that this defect is a real inconsistency rather than a policy: the two standalone
  5-Base entry points *do* `create_dir_all(&out_dir)` — `mod.rs:582` and `mod.rs:651` — and `temp_dir`
  is created too, `convert.rs:129`. Only the align path's `--output_dir` isn't. Still a separate fix.)

- **`--five_base_bisulfite_bam` / the 3.2.0 release — correctly excluded.**

- **The stray `>` in `C->T->converted` — the *decision* is fine, the *stated reason is wrong twice over*,
  and that is worth correcting because it will be inherited.** §8 says the string "is a faithful mirror
  of the Perl wording and the Perl-oracle gates compare it". Neither holds:
  1. Perl says **`the C->T converted genome`** — one arrow, then a space (`legacy_perl/bismark:7655`,
     `7666`, `7686`, `7697`). The Rust template is `"the {aligner} index of the {converted}->converted
     genome"` with `converted = "C->T"` (`error.rs:43-46`), which renders `C->T->converted`. The double
     arrow is a Rust-side template artefact, not a mirror. (Perl's HISAT2 wording differs again —
     `"seems to be faulty ($file doesn't exist)"`, `legacy_perl/bismark:7743` — so the unified Rust
     template already deviates.)
  2. `error.rs:5-7` states outright: *"None of the error text is part of the byte-identity gate
     (diagnostics go to STDERR)."* Nothing compares it.

  Either fix the template (`{converted} converted genome`) or replace the rationale with the honest one
  ("cosmetic, unrelated to #1099, deferred"). Leaving the false claim in the plan is the harm.

### 2.5 Task 4 ("separable"): I'd cut it from this PR, but the plan under-sells *why* it matters and under-specifies the check

Cutting it is the right call for blast radius. But two corrections.

**(a) The consequence of the fix makes Task 4's target the very next thing a #1099 user hits, and the
failure is worse than "only fails when the aligner runs".** Reproduced with a nonexistent
`--five_base_index` on a `--bowtie2` 5-Base run:

```
(ERR): "/tmp/…/no_such_idx" does not exist or is not a Bowtie 2 index
Exiting now ...
error: minimap2 produced fewer PE records than read pairs (desync)
```

The final error names **minimap2 on a `--bowtie2` run** (`mod.rs:1912` hardcodes the engine in a message
on the shared 5-Base PE path), the child's nonzero exit is not surfaced as such, and a truncated BAM +
report are left in the output directory. Before this fix, that user got a clean, actionable
"run bismark_genome_preparation"; after it, they get this. If Task 4 is cut, the **hardcoded `"minimap2"`
at `mod.rs:1912` should still be replaced with `config.aligner.name()`** — a one-token change on a
non-byte-gated path that turns a misleading error into a correct one, and it is genuinely part of "make
the 5-Base entry path honest".

**(b) If Task 4 is kept, its sketch is under-specified in a way that produces false rejections.**
`discovery::first_missing` is a **private** `fn` (`discovery.rs:123`) — reuse needs `pub(crate)`, which
the plan doesn't mention. More importantly `first_missing` takes a `large: bool` and has no fallback of
its own; the small→large retry lives in `discover_genome` (`discovery.rs:143-166`). A user's plain
bowtie2 index of a large genome is `.bt2l`, and hisat2's is `.ht2l`, so a single
`first_missing(aligner, dir, stem, false)` would reject a perfectly good large index. Any Task 4 must
replicate the two-arm probe. Also note `Path::parent()` of a bare relative basename (`normal_idx`) is
`Some("")`; `read_dir("")` fails, `join` works — decide which you rely on.

### 2.6 Smaller logic points

- **Ordering change, benign:** today the index check precedes `discover_fastas`, so a genome with
  neither index nor FASTA reports `FaultyIndex`; after the fix the 5-Base path reports `NoFasta`. The
  plan's §2 row 2 predicts this correctly.
- **The resolved-config summary becomes misleading.** `RunConfig::summary()` (`config.rs:1596-1609`) is
  printed unconditionally (`mod.rs:276`) and prints `CT index:`, `GA index:`, `large index:`. Today
  those paths are guaranteed to exist; after the fix a 5-Base run prints two paths that do not exist
  and a `large index: false` with nothing behind it. I saw exactly that in every run above. Cosmetic,
  STDERR, not gated — but it is the one place the "unvalidated basenames" fiction becomes user-visible,
  so it deserves a line in the plan (suppress those three lines when `five_base`, or suffix
  `(not used: 5-Base aligns unconverted)`).

---

## 3. Assumptions

**Stated and validated:**
- "`--genome` stays required for 5-Base" — correct, and load-bearing: `five_base_reference_fasta`
  (`mod.rs:1587-1609`) needs `config.genome.fastas` on *both* engine routes (single FASTA passed
  through, multi-FASTA concatenated), and `run_pe_five_base` reads `config.genome.fastas` /
  `genome_dir` for the in-memory genome and the report.
- "the faithful bisulfite paths are untouched, so the Perl-oracle gates are unaffected" — correct;
  `discover_genome` is not edited and the branch is gated on a flag no oracle run sets.

**Unstated, and each one true but worth writing down:**
1. *No `GenomeIndexes` consumer other than `resolve` exists.* True — it is constructed only in
   `discover_genome`, `run_config_stub` (`config.rs:1659`, test-only) and the new sibling.
2. *The 5-Base short-circuit in `pipeline()` is what makes the unvalidated basenames unread.* This —
   not a doc comment on the fields — is the actual invariant. A future refactor that moved the
   `config.five_base` check at `mod.rs:943` below the multicore branch would silently start stat-ing
   `ct_index_basename` (`parallel.rs:597`). §3's field doc should name `mod.rs:943` as the guarantor;
   that is the "invariant on the thing it constrains" the repo style asks for.
3. *`--illumina_5base` never resolves to `Rammap`.* True (§2.2) and it is why the sibling can omit the
   `aligner` parameter. Say so, or the omission looks like an oversight.

**On the plan's explicit reviewer question — "push back if you'd rather take the `Option` refactor now":
no, don't take it.** The `Option` does not buy safety here, it relocates it: six `expect`/`unwrap` sites
on byte-frozen paths whose justification is *the same* control-flow argument as the doc comment, only now
spelled as a panic. The invariant is already enforced structurally by `mod.rs:943`. Spend the honesty
budget on documenting that (assumption 2) rather than on a signature refactor riding a bug fix. If the
honest shape is ever wanted, the right rung is not `Option<PathBuf>` but an enum on `RunConfig.genome`
(`Bisulfite(GenomeIndexes)` vs `Unconverted { genome_dir, fastas, fasta_kind }`) — a separate PR.

---

## 4. Efficiency

Not a concern, and slightly positive.

- The sibling skips 12–16 `is_file()` stats (Bowtie 2 small + large fallback) plus the `Combined/`
  probe. Unmeasurable next to reading the genome FASTA.
- No allocation, memory or scaling change. `discover_fastas` does one `read_dir` per extension group
  as before.
- No new I/O on the faithful path.
- The extraction adds one function call per run.

The only "efficiency" item worth a sentence is the one the plan already handles: the sibling must not
re-implement `discover_fastas`, because the FASTA sort order is the `@SQ` contract shared with
`bismark-genome-preparation` (`discovery.rs:1-17`). Reusing it as-is is exactly right, and a
duplicate would be a byte-identity landmine.

---

## 5. Validation sufficiency

This is where the plan needs work. As written it has one gate that cannot pass in CI, one that is
environment-fragile, and no automated gate for the actual bug at the level the user experiences it.

### 5.1 Tests 4 and 5 cannot be written as `resolve(...) → Ok`

`resolve` execs the aligner before it can return `Ok`: `aligner::detect_aligner` at `config.rs:864`
runs `Command::new(&path).arg("--version").output()` (`aligner.rs:103-110`). The codebase already
documents this, at `config.rs:1626-1630`:

> Exists because `resolve` cannot be used: it execs `<aligner> --version` (`aligner::detect_aligner`)…

Consistent with that, **every one of the ~19 `resolve(...)` calls in `config.rs`'s test module asserts
an `Err`** — all of them guards that fire before line 864. There is no precedent for an `Ok` and the
reason is structural.

Consequences, verified:

- **Test 4 (`--illumina_5base --bowtie2 --five_base_index X` → `Ok`) will fail in Rust CI.** The
  `cargo test` job installs **minimap2 + samtools only** (`.github/workflows/rust_ci.yml:35-45`; same
  for the `rammap-inprocess` and `binseq-input` jobs at :96 and :136). No bowtie2, no hisat2 —
  which is why every bowtie2 integration test uses a fake shell script. Reproduced the exact CI
  failure locally with a stripped PATH:
  ```
  $ env PATH=/usr/bin:/bin bismark --illumina_5base --bowtie2 --five_base_index … --genome … -1 … -2 …
  error: failed to execute Bowtie 2 properly (could not run 'bowtie2 --version'). …
  ```
- **Test 5 (default minimap2 → `Ok`) passes in CI but fails on any dev box without minimap2** — the
  inverse of the repo's convention, which is to *skip* locally and *panic* in CI when a real binary is
  missing (`aligner_five_base_bisulfite.rs:59-64`).
- Both also need real `-1`/`-2` files on disk: `resolve_layout` calls `check_exists` →
  `InputFileMissing` (`config.rs:1486-1494`, `1529`). And `--illumina_5base` without `-1` is rejected
  at `config.rs:782-788`, so `cli_from(&["--illumina_5base"])` alone cannot reach discovery at all.
- **Test 6 is fine**, for a reason the plan doesn't state: `discover_genome` (847) precedes
  `detect_aligner` (864), so `FaultyIndex` is reachable with no aligner installed. It is also
  non-vacuous — if the branch were made unconditional, the run would sail past discovery and die at
  `AlignerNotWorking` instead, so the assertion flips. The sabotage check the plan mandates will work.

**Fix: put the positive gates where the repo already puts them — `tests/aligner_cli.rs`, with fake
aligner binaries.** The pattern is sitting right next to the target:
`five_base_bowtie2_unconverted_index_end_to_end` (`aligner_cli.rs:6265-6314`) uses
`make_fake_bowtie2_five_base_pe` + `--path_to_bowtie2 <tempdir>` and needs no real aligner. Note its
own line 6267: `make_genome(genome.path()); // CT/GA .bt2 (for discovery)` — that comment *is* the bug,
written down. `make_genome` (`aligner_cli.rs:29-39`) creates `Bisulfite_Genome/` precisely to satisfy a
check the run does not need.

So the #1099 gate is a near-clone of that test with a FASTA-only genome dir, asserting `.success()`:

- `five_base_bowtie2_needs_no_bisulfite_index` — genome dir containing only `genome.fa`,
  `--illumina_5base --bowtie2 --five_base_index <basename> --path_to_bowtie2 <fake>` → success, and
  assert the same `XM` as the existing test so it is a real gate, not just an exit code.
- `five_base_minimap2_needs_no_bisulfite_index` — same with `make_fake_minimap2_five_base_pe`. **This
  one is the important half**: it is the case where the demanded file is `BS_CT.mmi`, i.e. a genome
  prepared the ordinary Bowtie 2 way is *also* rejected today (§1). No unit test covers that.

Both run in every CI job, on every platform, with no aligner installed. Keep the plan's tests 1–3
(they are good) and keep 6 as the unit-level sabotage target; drop 4 and 5, or rewrite them as the two
integration tests above.

### 5.2 The over-reach guard already exists at integration level — say so

`missing_index_errors` (`aligner_cli.rs:425-446`) runs a faithful SE alignment against a genome with
`BS_CT.3.bt2` removed and asserts exit 1 + `"faulty or non-existant"`, with no `--path_to_bowtie2`. If
the branch were dropped and unconverted discovery ran for everything, that test fails (the run would
reach `detect_aligner` and report `AlignerNotWorking`). The plan treats test 6 as the sole guard; noting
the existing one costs a line and tells the implementer the over-reach is double-covered.

### 5.3 §6's end-to-end recipe would be misread as a failure

As written it uses `-o /tmp/out` with no `mkdir`, and predicts *"it will then fail on the bogus index —
that is the aligner, not us"*. Verified: it fails on the **output directory** instead —
`error: failed to open BAM "…": I/O error: No such file or directory` — the §8 defect firing before
the aligner is ever reached. An implementer following the recipe sees a failure whose message says
nothing about indexes and cannot tell whether the fix worked.

Rewrite the recipe to (a) `mkdir -p` the output dir, and (b) state the **exact** expected signature for
each route, so "past discovery" is observable rather than inferred:

```bash
G=$TMPDIR/g1099; mkdir -p "$G" "$TMPDIR/out"
gzip -dc test_files/pUC19.fa.gz > "$G/pUC19.fa"      # FASTA only — no Bisulfite_Genome/

# default (minimap2) — must now RUN TO COMPLETION (minimap2 needs no index at all)
bismark --illumina_5base --genome "$G" \
        -1 test_files/test_R1.fastq.gz -2 test_files/test_R2.fastq.gz -o "$TMPDIR/out"
# expect: exit 0, out/test_R1_bismark_mm2_pe.bam + _PE_report.txt written.
# MUST NOT contain "faulty or non-existant".

# negative control — the same genome WITHOUT --illumina_5base
bismark --genome "$G" test_files/test_R1.fastq.gz -o "$TMPDIR/out"
# expect: exit 1, "the Bowtie 2 index of the C->T->converted genome seems to be faulty"
```

The default route is the better end-to-end demo than the plan's `--bowtie2` one, because it needs no
index anywhere and therefore ends in a *clean success* rather than an expected-but-confusing aligner
failure. (Confirmed locally: with 14 empty placeholder index files the same command completes and
writes both outputs — so post-fix, with the placeholders deleted, it must complete identically. That
comparison is itself a good manual check: **the run's output must be unchanged whether or not the
placeholder files exist**, which is the sharpest statement of "these files are never read".)

### 5.4 Highest-risk failure mode, and whether the plan catches it

| Failure mode | Caught? |
|---|---|
| Fix over-reaches; faithful runs stop validating the index | **Yes** — test 6 (sabotage-checked) + existing `missing_index_errors` |
| Fix doesn't actually fix #1099 for the default (minimap2) route | **No automated gate** — plan test 5 is the only one and it is `resolve`-level and CI-fragile → §5.1 |
| Prologue extraction turns `GenomeFolder` into `Io` | **No** — zero coverage of `GenomeFolder` anywhere → §2.3 |
| 5-Base later reads the unvalidated basenames | **Partially** — a doc comment. The real guard is `mod.rs:943`; naming it is worth more than the comment → §3 assumption 2 |
| Regression on a *prepared* genome (both routes) | **No** — every existing 5-Base test uses `make_genome`, i.e. a prepared genome, so the prepared case stays covered by the existing suite. Fine as-is; just don't delete `make_genome` from those tests while adding the new ones |

---

## 6. Alternatives

**A. The chosen sibling — endorse.** Additive, `discover_genome` untouched, no churn to 21 existing
tests, and shared prologue prevents drift. Correctly rejects the `bool` parameter.

**B. Factor the branch into a named function (recommended addition, not a replacement).** The plan puts
the branch inline at `config.rs:847`, which is why its tests have to drive all of `resolve` and
therefore need an aligner binary. A three-line named function is unit-testable with no binary at all:

```rust
fn discover_genome_for_run(five_base: bool, aligner: Aligner, genome_arg: &Path) -> Result<GenomeIndexes>
```

Then the branch itself gets direct, CI-independent coverage (`five_base=true` over a FASTA-only dir →
`Ok`; `false` → `FaultyIndex`), and the integration tests of §5.1 cover the user-visible contract. This
is the cheapest way to get both levels without fighting `detect_aligner`. Cost: one small function.

**C. `pub(crate)` rather than `pub` for the sibling.** `discovery` is `pub mod` (`mod.rs:35`) inside
`pub mod aligner`, so `pub fn discover_genome_unconverted` becomes crate-public API for a function with
exactly one in-crate caller. `discover_fastas` is deliberately `pub(crate)` (`discovery.rs:200`).
Match it.

**D. Rejected: discover, then fall back on `FaultyIndex` when `five_base`.** Tempting because it is one
`match`, but it swallows a genuine faulty-index error and leaves discovery order load-bearing. Don't.

**E. Rejected for now: the `GenomeRefs` enum on `RunConfig.genome`.** The truthful shape (§3), a
separate PR, and not worth coupling to a bug fix — same verdict as the plan reaches about `Option`, one
rung further up.

---

## 7. Action items

### Critical

1. **Rewrite tests 4 and 5.** They assert `resolve(...) → Ok`, which requires the aligner binary
   (`config.rs:864` → `aligner.rs:103`); the Rust CI `cargo test` job installs only minimap2 +
   samtools (`rust_ci.yml:35-45`), so **test 4 fails in CI** and test 5 fails on any box without
   minimap2. `config.rs:1626-1630` already records that `resolve` cannot be used this way, and all ~19
   existing `resolve` test calls assert `Err`. Replace with (a) the alternative-B named function for
   unit-level branch coverage, and/or (b) two integration tests in `tests/aligner_cli.rs` modelled on
   `five_base_bowtie2_unconverted_index_end_to_end` (`:6265`) using the fake-aligner + `--path_to_*`
   convention. **Include the minimap2 route** — that is the one whose missing file is `BS_CT.mmi`.
2. **Add `GenomeFolder` tests for `discover_genome` itself.** `AlignerError::GenomeFolder` has zero
   coverage repo-wide, so §7's "existing discovery tests cover them" is false, and the natural
   extraction (`canonicalize(genome_arg)?`) silently downgrades it to `AlignerError::Io` via
   `#[from]` and still compiles. Two assertions on the faithful function, not just on the sibling.
3. **Fix the §6 recipe.** `mkdir -p` the `-o` directory (otherwise it dies on the §8 out-of-scope
   defect with a message that reads like the fix failing) and state the exact expected signature per
   route. Prefer the default minimap2 route as the demo — it ends in a clean success.

### Important

4. **Correct §8's rationale for `C->T->converted`.** Perl writes `C->T converted` with one arrow
   (`legacy_perl/bismark:7655/7666/7686/7697`), so the double arrow is a Rust template artefact, not a
   mirror; and `error.rs:5-7` says error text is *not* byte-gated, so nothing compares it. Either fix
   the template or state the honest reason for deferring.
5. **Add the `BS_CT.mmi` row to the §2 behaviour table.** On the default engine, a genome prepared the
   ordinary Bowtie 2 way is *also* rejected today. That is the bigger, more common case and it is
   currently invisible in the plan.
6. **Document the real invariant, not just the field.** §3/Task 1.3's doc should name the 5-Base
   short-circuit at `mod.rs:943` as what guarantees the unvalidated basenames are never read (the
   nearest reader being `parallel.rs:597`). Put it on the fields, per house style, but say *why* they
   are safe rather than only that they are unchecked.
7. **If Task 4 is cut, still fix `mod.rs:1912`** — it hardcodes `"minimap2"` in the desync error on the
   shared 5-Base PE path, so a `--bowtie2` run with a bad `--five_base_index` reports
   `error: minimap2 produced fewer PE records than read pairs (desync)` after writing a truncated BAM.
   That is the *first* thing a #1099 user will hit once discovery stops blocking them. One token:
   `config.aligner.name()`.
8. **If Task 4 is kept, specify it properly:** `first_missing` is private (`discovery.rs:123`) and takes
   `large: bool` with no fallback of its own — a single small-index probe would falsely reject a valid
   `.bt2l`/`.ht2l` index. Replicate the two-arm probe from `discovery.rs:143-166`.

### Optional

9. `pub(crate)` not `pub` for `discover_genome_unconverted`, matching `discover_fastas`.
10. Note `parallel.rs:597` in §1's "read only on bisulfite paths" list (unreachable for 5-Base via
    `mod.rs:943`, and `None`-safe anyway) so the audit trail is complete.
11. Suppress or annotate the `CT index:` / `GA index:` / `large index:` lines of `RunConfig::summary()`
    (`config.rs:1596-1609`, printed at `mod.rs:276`) for a 5-Base run — post-fix they print paths that
    do not exist.
12. One line in `docs/src/content/docs/rust/illumina-5-base.md` (the "Running it" block, `:45-56`)
    saying the genome folder needs only the FASTA and no `bismark_genome_preparation` run. The docs
    already show `--genome` with no prep step — which corroborates that the rejection is a bug — but
    #1099's reporter still inferred the opposite, so making it explicit is cheap.
13. Add the reproduction dir shape to §5 as a shared test helper (`make_genome_fasta_only`) rather than
    inlining it in three tests; `aligner_cli.rs` already has `make_genome`/`make_genome_mmi`/
    `make_genome_ht2`/`make_genome_chr1` and this is the fifth of the family.
