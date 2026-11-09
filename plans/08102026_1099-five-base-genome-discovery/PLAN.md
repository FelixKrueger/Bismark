# PLAN — #1099: `--illumina_5base` must not require the converted CT/GA indexes

**Issue:** [#1099](https://github.com/FelixKrueger/Bismark/issues/1099) · reported by @Danielsm8 in [#1095](https://github.com/FelixKrueger/Bismark/issues/1095#issuecomment-5244413281)
**Rev:** 1 — all agreed findings from `PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md` folded; §0 holds the four decisions that need Felix
**Branch to create:** `1099-five-base-genome-discovery` off `dev` (G23 — currently sitting on `dev`)

> Both reviewers: **diagnosis and design correct, ship as designed; the rev-0 test set was not
> shippable.** §5 is rewritten accordingly. Every rev-0 factual claim was independently verified
> except the four corrections recorded in §9.

---

## 0. Open decisions (need Felix)

| # | Decision | A | B | My lean |
|---|---|---|---|---|
| **D1** | **Task 4** (fail early on a missing `--five_base_index`) — keep or cut? | In scope: #1099 says "worth covering in the same change"; cutting is Felix's call, not a reviewer's | Cut it — second defect, widens blast radius | **Cut**, file separately. But it needs your word, not mine (§4 Task 4) |
| **D2** | Branch `RunConfig::summary()` on `five_base` so a 5-Base run stops printing `CT index:`/`GA index:` for paths that don't exist | **Critical** — this is the issue's "should not be silently fabricated", and it's what makes rejecting the `Option` refactor defensible | Optional (#11) | **Take it.** ~6 lines, nothing asserts the summary text, and it closes the one observable hole |
| **D3** | Repoint the **13 existing** 5-Base test-helper call sites at a FASTA-only genome (drop their dummy index writes) | **Important (I1)** — end-to-end, both engines, CI-portable, *deletes* scaffolding | not raised | **Take it** — it is the strongest #1099 gate available and costs less than the tests it replaces |
| **D4** | Fold `run_five_base_consensus_standalone` (`mod.rs:543-546`) in as the sibling's second caller | **Important (I5)** — a third copy of the prologue, with no `is_dir` check | not raised | **Take it**, or record why the third copy stays — the plan's own anti-drift rationale demands one or the other |

---

## 1. Context and root cause

`config::resolve` resolves the genome unconditionally:

```rust
let genome = discovery::discover_genome(aligner, &genome_arg)?;   // config.rs:847
```

`discover_genome` returns `AlignerError::FaultyIndex` unless the `BS_CT.*` / `BS_GA.*` set under
`<genome>/Bisulfite_Genome/` is complete (`discovery.rs:143-166`). Nothing on the 5-Base path opens
those files:

| 5-Base route | what it actually aligns against | code |
|---|---|---|
| `--bowtie2` / `--hisat2` | the user's **unconverted** index from `--five_base_index` | `mod.rs:1557-1569` |
| default (minimap2) | the genome **FASTA**, passed positionally | `mod.rs:1571-1577` |

**Complete reader audit of the converted basenames** (rev 0 missed the fourth; the whole fix rests on
this list being exhaustive):

| Reader | Reachable for 5-Base? |
|---|---|
| `mod.rs:1211-1212` (SE bisulfite spawn) | No |
| `mod.rs:1345` / `:1350` (in-process rammap index load) | No — `--rammap` is rejected for 5-Base (`config.rs:1251-1257`) |
| `mod.rs:4633-4634` (PE bisulfite spawn) | No |
| **`parallel.rs:597`** — `estimate_index_bytes(&cfg.genome.ct_index_basename)` in `emit_memory_warning` | No |

The guarantor for all four is one line: **`pipeline()` short-circuits `config.five_base` to
`run_pe_five_base` at `mod.rs:943-953`, *before* the `n > 1` fork dispatch (`mod.rs:1031-1032`)** — so
even `--multicore N` never routes 5-Base through `parallel::run_pe_multicore`. (`aligner_cli.rs:6163`
already passes `--multicore 4` to a 5-Base run and asserts a single-instance option string.)
`parallel.rs:597` also degrades gracefully (`read_dir(parent).ok()?` → `None`), but the structural
guard is the real answer.

`large_index` is read at `config.rs:1609` — **inside `RunConfig::summary()`** (`config.rs:1568`), *not*
a `Debug` impl as rev 0 claimed, and printed **unconditionally for every run** at `mod.rs:276`. See D2.

One line above the offending call the same guard already exists (`config.rs:837-839`):

```rust
if !cli.illumina_5base {
    reject_unsupported_paired_aligner(aligner, &layout)?;
}
```

Present in released **3.1.0** (same call, ~`config.rs:596` at tag `bismark-rust-v3.1.0`) and on `dev`.

**`--genome` stays required for 5-Base** — `five_base_reference_fasta` (`mod.rs:1587-1609`) needs
`config.genome.fastas` on *both* routes, and `run_pe_five_base` reads `fastas`/`genome_dir` for the
in-memory genome and the report. Only the *converted-index requirement* is wrong.

## 2. Behaviour change

| Run | Before | After |
|---|---|---|
| `--illumina_5base [--bowtie2 --five_base_index X]`, genome folder without `Bisulfite_Genome/` | `FaultyIndex` on `BS_CT.1.bt2l` | runs |
| **`--illumina_5base` (default minimap2) on a genome prepared the ordinary Bowtie 2 way** | **`FaultyIndex` on `BS_CT.mmi`** — genome prep only makes that under `--minimap2`, so a correctly-prepared user is *also* blocked | **runs** |
| `--illumina_5base`, genome folder with no FASTA | `FaultyIndex` (fires first) | `NoFasta` — still fails, correctly |
| **any non-5-Base run**, unprepared genome | `FaultyIndex` | **`FaultyIndex` — unchanged** |
| any prepared genome, any run | unchanged | unchanged (bar D2's summary lines) |

Row 2 is the larger and more common case and was invisible in rev 0. Byte-identity: the faithful paths
are untouched and the branch is gated on a flag no oracle run sets, so `perl-oracle` cannot regress.
5-Base has no Perl oracle (concordance-gated only).

## 3. Design

**Additive sibling**, `discover_genome` behaviourally untouched. Both reviewers endorse.

```rust
// discovery.rs
pub(crate) fn discover_genome_fasta_only(genome_arg: &Path) -> Result<GenomeIndexes>
```

- `pub(crate)`, matching `discover_fastas` (B O9) — one in-crate caller, no reason to be crate-public API.
- Named `_fasta_only`, not `_unconverted` (A O7): "unconverted" already means the *reference* in 5-Base
  vocabulary and this function has nothing to do with `--five_base_index`.
- Shared prologue `absolute_genome_dir(genome_arg) -> Result<PathBuf>` extracted from
  `discovery.rs:132-138`. **Both error cases must stay `AlignerError::GenomeFolder`** — see §7 for the
  `?`-downgrade trap.
- Reuses `discover_fastas` **as-is**: its sort order is the `@SQ` contract shared with
  `bismark-genome-preparation` (`discovery.rs:1-17`); a duplicate would be a byte-identity landmine.
- `large_index: false`, `combined_index_basename: None`. Not "display-only hand-waves" — *provably
  unobservable*: `--combined_index` is rejected for 5-Base at `config.rs:820-830`, **before** the
  presence guard at `:850` (O1: that ordering is the reason, worth a comment, it is fragile at 30 lines
  apart).

Call site, factored into a named function so the branch is unit-testable without an aligner binary
(B alternative B — this is what makes §5.1 work):

```rust
fn discover_genome_for_run(five_base: bool, aligner: Aligner, genome_arg: &Path) -> Result<GenomeIndexes> {
    if five_base { discover_genome_fasta_only(genome_arg) } else { discover_genome(aligner, genome_arg) }
}
```

### Rejected alternatives

- **`require_converted_index: bool` parameter on `discover_genome`** — churns the signature every
  faithful run depends on, plus its 21 tests, for no gain. Both reviewers agree.
- **`Option<PathBuf>` for the two basenames** — both reviewers say **no**, and rev 0's *reason* was
  wrong. It is not that the fabrication is invisible (it is visible — D2); it is that four of the five
  reader expressions sit on **byte-frozen** alignment paths, so the refactor either sprinkles
  `expect()` panics there (a worse regression than a cosmetic line) or threads new `Result` plumbing
  through code whose entire value is being unchanged since it was proven byte-identical. The invariant
  is already structural (`mod.rs:943`). **D2 satisfies the issue's "should not be silently fabricated"
  in substance for ~6 lines.**
- **A `converted_indexes_validated` provenance field** — `RunConfig.five_base` already answers it, and
  a new `GenomeIndexes` field breaks `run_config_stub` (`config.rs:1632`, deliberately non-`Default`).
- **Discover, then swallow `FaultyIndex` when `five_base`** — hides genuine faulty-index errors and
  leaves discovery order load-bearing.
- **A `GenomeRefs` enum on `RunConfig.genome`** (`Bisulfite(..)` vs `Unconverted { .. }`) — the truthful
  shape, both reviewers agree, **separate PR**.

## 4. Tasks

### Task 1 — `discovery.rs`: extract the prologue, add the sibling
1. Extract `discovery.rs:132-138` into `fn absolute_genome_dir(genome_arg: &Path) -> Result<PathBuf>`,
   preserving `AlignerError::GenomeFolder(genome_arg.to_path_buf())` on **both** the canonicalize
   failure and `!is_dir`. Call it from `discover_genome`.
2. Add `pub(crate) fn discover_genome_fasta_only`: `absolute_genome_dir` → `discover_fastas` →
   `GenomeIndexes { conventional CT/GA basenames, large_index: false, combined_index_basename: None }`.
3. Doc the two basename fields with **why** they are safe, not just that they are unchecked: name
   `mod.rs:943` as the guarantor and `parallel.rs:597` as the nearest reader (house style: the
   invariant goes on the thing it constrains). Watch `clippy::doc_lazy_continuation` — a wrapped `///`
   line must not begin `- `/`+ `/`* ` under `-D warnings` (A O5).

### Task 2 — `config.rs`: the branch
Add `discover_genome_for_run` (§3) and call it at `config.rs:847`. Comment the
`820-830`-before-`850` ordering (O1).

### Task 3 — D2 (if taken): `summary()`
Branch `RunConfig::summary()` (`config.rs:1568-1609`) on `self.five_base`: print the reference FASTA
and `--five_base_index` in place of `CT index:` / `GA index:` / `large index:`. No new field; no test
asserts the summary text; the persisted `*_report.txt` never carries an index basename.

### Task 4 — D1: fail early on a missing `--five_base_index`
**Cut unless Felix says keep** (#1099 asked for it, so this is his call). If kept, three hazards both
reviewers surfaced:
1. `first_missing` and `index_suffixes` are **private** (`discovery.rs:123`, `:103`) → `pub(crate)`.
2. `first_missing` takes `large: bool` with **no fallback**; the small→large retry lives in
   `discover_genome` (`:143-166`). A single small probe would falsely reject a valid `.bt2l`/`.ht2l`
   index — replicate the two-arm probe.
3. **A bare basename can legitimately resolve via `BOWTIE2_INDEXES` / `HISAT2_INDEXES`** (A confirmed
   the string in the local `bowtie2-align-s`). A hard presence check would be a *new* spurious
   rejection introduced by a fix for a spurious rejection. Check only when the basename contains a
   path separator, honour the env var, or warn rather than error.

### Task 5 — `mod.rs:1912`: the misattributed error (ride-along either way)
It hardcodes `"minimap2"` in the desync error on the **shared** 5-Base PE path, so a `--bowtie2` run
with a bad `--five_base_index` prints `error: minimap2 produced fewer PE records than read pairs
(desync)` after leaving a truncated BAM. Both reviewers flagged it; it is the *first* thing a #1099
user hits once discovery stops blocking. One token: `config.aligner.name()`.

### Task 6 — CHANGELOG + docs
- `CHANGELOG.md` at the **repo root** (there is no `rust/CHANGELOG.md`), `## Unreleased` → `### bismark
  (aligner)`. Cover both routes, that `--genome` is still required, that non-5-Base runs are unchanged.
  Credit @Danielsm8.
- One sentence in `Docs/src/content/docs/rust/illumina-5-base.md` ("Running it", ~`:44-56`): the genome
  folder needs only the FASTA, no `bismark_genome_preparation`. **The page already shows `--genome`
  with no prep step** — independent corroboration that the rejection is a bug — yet #1099's reporter
  still inferred the opposite. Precedent: the #1095 commit `6a3ea0d` touched this page + `rust/README.md`
  alongside the CHANGELOG.

## 5. Tests (rewritten — rev 0's set was not shippable)

**Why rev 0 was wrong:** `resolve` execs `<aligner> --version` via `detect_aligner` at
`config.rs:864`, *after* discovery — and `rust_ci.yml` installs **minimap2 + samtools only** in all
three `cargo test -p bismark` jobs (`:35-44`, `:96-104`, `:136-144`). Never bowtie2 or hisat2. So
rev-0 test 4 was green on Felix's Mac and **red on every runner**; test 5 survived only by luck of that
install list. `config.rs:1626-1630` already documents the hazard, and **not one** of the module's ~19
`resolve` calls asserts `Ok`.

Three further traps, all mandatory:
- `check_exists` (`config.rs:1492-1493`) runs at line 791, *before* discovery → every resolve-level
  test needs **real read files** (empty temp files suffice) or it exits at `InputFileMissing`.
- `--illumina_5base` without `-1` is rejected at `config.rs:782-788`.
- **Assert typed variants, never `is_err()`** — otherwise the over-reach guard passes vacuously.

### 5a. Unit — `discovery.rs`
1. `fasta_only_needs_no_bisulfite_index` — dir with `g.fa` only → `Ok`; `fastas.len()==1`,
   `fasta_kind==Fa`, `combined_index_basename.is_none()`, `!large_index`.
2. `fasta_only_still_requires_a_fasta` — empty dir → `NoFasta`.
3. `fasta_only_rejects_a_non_directory` / `_nonexistent_path` → `GenomeFolder`.
4. **`discover_genome` mirrors of 3** — nonexistent path *and* path-to-a-file → `GenomeFolder`.
   `AlignerError::GenomeFolder` has **zero coverage crate-wide** today (both reviewers grepped it), so
   rev 0's "existing discovery tests cover them" was false. See §7.
5. *(A O4)* `.fa.gz`-only genome → `fasta_kind == FaGz`; the shape `five_base_reference_fasta`
   (`mod.rs:1587-1591`) hands straight to minimap2.

### 5b. Unit — the branch, no binary needed
6. `discover_genome_for_run(five_base=true, …)` over a FASTA-only dir → `Ok`;
   `(five_base=false, …)` → `FaultyIndex`. This is why §3 factors the branch out.

### 5c. Resolve-level — using the module's own 5-Base idiom
7. Rewrite rev-0 tests 4/5 as the shape `five_base_duplex_guards` (`config.rs:2105-2122`) already uses
   — assert the *specific* error cannot be the one under test, so it is install-agnostic:
   ```rust
   if let Err(e) = resolve(&cli_from(&[...]), "cmd".into()) {
       assert!(!matches!(e, AlignerError::FaultyIndex { .. }),
               "5-Base must not require the converted index; got: {e}");
   }
   ```
   Fails before the fix (`FaultyIndex`), passes after whether the run ends `Ok` (bowtie2 present) or
   `Err(AlignerNotWorking)` (CI). Note these are the **first fixture-carrying tests** in that module.
8. `faithful_run_still_rejects_unprepared_genome` — no `--illumina_5base` → `matches!(…, FaultyIndex)`.

### 5d. Integration — the gates that prove #1099 at the user's level
9. Two hermetic tests in `tests/aligner_cli.rs`, modelled on
   `five_base_bowtie2_unconverted_index_end_to_end` (`:6265-6314`), which uses
   `make_fake_bowtie2_five_base_pe` + `--path_to_bowtie2` and needs **no real aligner**:
   `five_base_bowtie2_needs_no_bisulfite_index` and **`five_base_minimap2_needs_no_bisulfite_index`**
   (the important half — the `BS_CT.mmi` case, §2 row 2). Assert the `XM` tag, not just exit status.
   Add `make_genome_fasta_only` as the fifth of the `make_genome*` family (B #13).
10. **D3 (if taken): repoint the existing helpers.** `aligner_cli.rs:6167`, `:6269` and
    `aligner_five_base_groundtruth.rs:95`, `:754` — **13 call sites** — write dummy CT/GA index files
    *solely* to pass discovery, and say so in comments (`// CT/GA .bt2 (for discovery)`). Dropping those
    writes turns them into an end-to-end, both-engines, CI-portable #1099 gate that **deletes**
    scaffolding. Keep `make_genome` in use where a prepared genome is the point.

### 5e. Over-reach — double-covered, sabotage-verified
The existing **`missing_index_errors`** (`aligner_cli.rs:423-446`) runs the real binary against a
genome with `BS_CT.3.bt2` removed and asserts exit 1 + `"faulty or non-existant"`, with **no** fake
bowtie2 — it passes on CI today *precisely because* discovery errors before `detect_aligner`, which is
the property under test. **Sabotage check: make the branch unconditional and require BOTH test 8 and
`missing_index_errors` to go red.** Under sabotage test 8 must fail *because the variant is no longer
`FaultyIndex`* (it becomes `AlignerNotWorking`), not merely because something changed.

## 6. Verification

```bash
cargo test -p bismark --lib aligner::discovery      # baseline established by both reviewers: 21 passed
cargo test -p bismark --lib aligner::config
cargo test -p bismark --test aligner_cli five_base
cargo test -p bismark --test aligner_five_base_groundtruth
cargo test -p bismark                              # full crate
cargo fmt -p bismark -- --check                    # its own CI job
cargo clippy -p bismark --all-targets -- -D warnings
cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings   # G26
```

End-to-end (rev 0's recipe died on the un-created `-o` directory — §8 — with a message that reads like
the fix failing; B verified this):

```bash
G=$TMPDIR/g1099; mkdir -p "$G" "$TMPDIR/out"
gzip -dc test_files/pUC19.fa.gz > "$G/pUC19.fa"          # FASTA only, no Bisulfite_Genome/

# default (minimap2) — needs no index anywhere, so this must END IN CLEAN SUCCESS
bismark --illumina_5base --genome "$G" \
        -1 test_files/test_R1.fastq.gz -2 test_files/test_R2.fastq.gz -o "$TMPDIR/out"
# expect exit 0; out/test_R1_bismark_mm2_pe.bam + _PE_report.txt written;
# stderr MUST NOT contain "faulty or non-existant"

# negative control — same genome, no --illumina_5base
bismark --genome "$G" test_files/test_R1.fastq.gz -o "$TMPDIR/out"
# expect exit 1 + "the Bowtie 2 index of the C->T->converted genome seems to be faulty"
```

**Sharpest manual check (B):** with 14 empty placeholder index files the default run completes today;
post-fix, with the placeholders **deleted**, its output must be *identical*. That is the strongest
statement of "these files are never read".

## 7. Risks

| Risk | Mitigation |
|---|---|
| **The prologue extraction silently downgrades `GenomeFolder` → `Io`.** The natural `std::fs::canonicalize(genome_arg)?` **compiles** and swaps the clear error for a bare `I/O error: No such file or directory`, via `AlignerError`'s `Io(#[from])` (`error.rs:15-16`) — a user-visible regression on the **faithful** path | Test 5a.4 pins **both** branches for **both** entry points. Rev 0's stated mitigation did not exist |
| Fix over-reaches, faithful runs stop validating the index | Test 8 + existing `missing_index_errors`, both required red under sabotage (§5e) |
| **Blast-radius bound:** over-reach costs *availability*, not silent miscalls — a bisulfite run on the unconverted path dies at bowtie2 spawn on a missing `-x`. The silent-wrong-calls scenario needs a prepared genome *and* a 5-Base run reaching a bisulfite spawn, which `pipeline()`'s unconditional `return run_pe_five_base` makes unreachable | This bound is what justifies a light test set (A O2) |
| A future refactor moves the `mod.rs:943` short-circuit below the fork dispatch → `parallel.rs:597` starts stat-ing a path that does not exist | Task 1.3 names `mod.rs:943` as the guarantor. *(A O3, optional:* `debug_assert!(!config.five_base)` at the two bisulfite spawn readers buys the loud-failure property back in CI; compiles out in release so byte-frozen output is unaffected, and `run_pe_five_base` spawns its own `Command` so it cannot trip)* |

## 8. Out of scope

- **`-o` not auto-created.** Separate defect, and a real inconsistency rather than policy: the two
  standalone 5-Base entry points *do* `create_dir_all` (`mod.rs:582`, `:651`) and `temp_dir` is created
  (`convert.rs:129`) — only the align path's `--output_dir` is not. Noted in #1099.
- **`--five_base_bisulfite_bam` / the 3.2.0 release.**
- **The stray `>` in `C->T->converted`** — deferred as **cosmetic**. Rev 0's rationale was wrong twice
  and is corrected here: Perl writes `the C->T converted genome`, one arrow then a space
  (`legacy_perl/bismark:7655`/`7666`/`7686`/`7697`), so the double arrow is a **Rust template
  artefact** (`error.rs:43-46` renders `{converted}->converted` with `converted = "C->T"`), not a
  faithful mirror; and `error.rs:5-7` states outright that **no error text is part of the byte-identity
  gate**. A one-line template fix (`{converted} converted genome`) is available and safe if wanted.
- `discover_fastas` re-`read_dir`ing up to four times — pre-existing, byte-order-significant, shared
  with genome-prep. Do not touch on a bug-fix branch.

## 10. Implementation notes (branch `1099-five-base-genome-discovery`)

Felix answered the §0 decisions with a bare `implement`, i.e. take the recorded leans:
**D1 cut** — filed as [#1100](https://github.com/FelixKrueger/Bismark/issues/1100) carrying all three
implementation hazards the reviewers found (private `first_missing`, the missing two-arm probe, and
`BOWTIE2_INDEXES`/`HISAT2_INDEXES` resolution) plus the `-o`-not-created defect — **D2 taken**,
**D3 taken**, **D4 taken**, Task 5 taken.

| Change | Where |
|---|---|
| `absolute_genome_dir` extracted; `GenomeFolder` preserved on both arms | `discovery.rs` |
| `pub(crate) discover_genome_fasta_only` | `discovery.rs` |
| Field docs naming `mod.rs:943` as the guarantor and `parallel.rs:597` as the nearest reader | `discovery.rs` |
| `discover_genome_for_run` + branched call site + the `820-830`-before-`850` ordering comment | `config.rs` |
| **D2** — `summary()` prints `reference:` / `5-Base index:` instead of `CT index:` / `GA index:` / `large index:` on a 5-Base run | `config.rs` |
| **D4** — the consensus path's hand-rolled prologue replaced by the sibling (gains the `is_dir` check; loses its bespoke stderr text) | `mod.rs` |
| **Task 5** — desync error uses `config.aligner.name()` | `mod.rs` |
| CHANGELOG entry + a docs paragraph stating no genome-prep step is needed | `CHANGELOG.md`, `Docs/src/content/docs/rust/illumina-5-base.md` |

**Deviation from §5d.9 (documented, not silent).** Rev 1 called for *adding* two integration
tests. Implemented as A's I1 instead: the two existing engine-route end-to-end tests were
**repointed** at `make_genome_fasta_only` — `five_base_pe_end_to_end_inverts_polarity`
(minimap2, the `BS_CT.mmi` case) and `five_base_bowtie2_unconverted_index_end_to_end`
(bowtie2) — and the groundtruth helpers `write_genome` / `write_genome_multi` lost their dummy
`BS_*.mmi` writes, repointing 7 more call sites. Adding near-clones alongside repointed
originals would have been redundant.

⚠️ **Corrected after review** (all three phase-5 agents caught this; the rev-0 wording was wrong):
the repointed total is **9 test sites across 4 locations**, not "13 call sites". And
prepared-genome 5-Base coverage rests on **exactly one** test, not three: of the four retained
`make_genome*` 5-Base sites, only `five_base_umi_dedup_drops_duplicates` reaches discovery —
`five_base_rejects_non_directional` and `five_base_bowtie2_requires_index` are rejected by guard
order *before* it, and `five_base_umi_len_requires_illumina_5base` is not a 5-Base run at all. So
the **bowtie2 route now has no prepared-genome coverage**. Both reviewers judged one test adequate,
because with a prepared genome the two entry points differ in only `large_index` and
`combined_index_basename`, neither of which is readable on any 5-Base path once D2 lands. Recorded
rather than papered over; adding one `--bowtie2` 5-Base end-to-end against `make_genome` would
restore engine-route symmetry if wanted (~30 lines, low value).

**Sabotage record** (every new assertion observed failing once):

| Assertion | Sabotage | Observed |
|---|---|---|
| `genome_folder_errors_survive_the_shared_prologue` | `canonicalize(genome_arg)?` in the helper | **red** — caught the `Io` downgrade exactly |
| `faithful_resolve_still_requires_the_converted_index` | branch made unconditional | **red**, and non-vacuously (the variant stops being `FaultyIndex`) |
| existing `missing_index_errors` (`aligner_cli.rs:423`) | same | **red** — §5e's both-guards requirement met |
| `fasta_only_needs_no_bisulfite_index`, `discover_genome_for_run_*`, `five_base_resolve_*` | n/a | fail by construction pre-fix (the function did not exist / the branch rejected) |

**Iteration log**

`#1` First end-to-end run "passed" against `$TMPDIR/g1099` — the exact path in rev 1 §6, which
**Reviewer B had already populated with placeholder index files**. Both the positive and the
negative control were therefore vacuous (the control reached bowtie2 instead of erroring). Re-run
on a pristine `$TMPDIR/g1099_clean_$$`: 5-Base succeeds with no `faulty or non-existant`, the
summary shows `reference:` + `5-Base index: (none: minimap2 reads the FASTA)`, and the faithful
control still errors. A live instance of G19 — a shared scratch path made the check meaningless.

**Test counts:** `aligner::discovery` 21 → 25; `aligner::config` 42 → 45; `--test aligner_cli
five_base` 6 passed; `--test aligner_five_base_groundtruth` 7 passed.

## 11. Phase-5 review fixes (applied)

Dual code review: **A APPROVE with documentation fixes · B APPROVE WITH CHANGES** · plan-manager
**COMPLETE** (33 items, 32 DONE, 1 documented deviation, 0 gaps). No correctness defect was found by
any of the three; every finding was in comments, doc strings, one summary line, or this plan's own
claims. Reports: `CODE_REVIEW_A.md`, `CODE_REVIEW_B.md`, `COVERAGE.md`.

Applied (all three agents concurred on the first two):

| Fix | Where | Raised by |
|---|---|---|
| Reattached `discover_genome`'s doc comment — the extraction had orphaned it onto `absolute_genome_dir`, leaving the module's only `pub` entry point undocumented (`missing_docs` is off for this module, so CI could not catch it) | `discovery.rs` | A H1, B H1, PM O1 |
| Field-doc guarantor now anchored on **function names**, not line numbers — `mod.rs:943` was invalidated by this change's own D4 hunk and pointed at the `unreachable!` arm | `discovery.rs` | A M1, B M2, PM O2 |
| `--genome` help text notes `--illumina_5base` needs only the FASTA — the string a user actually reads, and the most plausible reason #1099's reporter believed prep was required | `cli.rs` | A M2 |
| Dropped the 5-Base `reference:` summary line: it named `fastas.first()` while `five_base_reference_fasta` **concatenates** a multi-FASTA genome, so it was wrong for a per-chromosome GRCh38 layout. `genome:` + `FASTA(s): N file(s)` already carry it, and D2 is satisfied by not printing the fabricated basenames | `config.rs` | A M3, B M3 |
| `combined_index_basename` doc: "every run" → "every bisulfite run", plus its own guarantor (the `--illumina_5base` scope guards reject all four combined flags) — the field with ~20 readers, missed by the Task 1.3 doc pass | `discovery.rs` | B M1 |
| `combined_index` comment anchored on the guard that actually rejects, not "above" (the line above names `reject_combined_index_unsupported`, which has no `five_base` check) | `config.rs` | A L2 |
| CHANGELOG: the summary change replaces **three** lines, not two | `CHANGELOG.md` | B L9 |
| House-style trims: dropped the `?`-downgrade trap narrative from `absolute_genome_dir` (it justifies the change in source; the test name and commit message carry it) and shortened `discover_genome_for_run`'s doc to two lines | `discovery.rs`, `config.rs` | A L3, B L13 |
| §10's "13 call sites" → 9 sites / 4 locations; the prepared-genome claim corrected from three tests to one | this file | A L4, B M6, PM O4 |

**Left for Felix — a decision, not an omission:** the `rust/README.md` Milestones bullet. B rates it
Medium (established for comparable aligner fixes `16f65f6`, `11efbab`, `ddc7633`); A rates it Low
("nothing in it is now wrong"); the plan-manager notes the README's own rule binds *module-merge PRs
into `master`*, which this `dev` bug-fix branch arguably is not.

**Declined, with reasons:** new tests for the D4 consensus path (B M4 — uncovered before this change
too, and B explicitly did not ask for it as a gate) and for Task 5's desync message (B M5, ~20
lines); narrowing `five_base_resolve_does_not_require_the_converted_index` to an allow-list of
variants (A L5 — the companion over-reach test catches a regression from the other direction).

**Post-fix verification:** `aligner::discovery` 25/25, `aligner::config` 45/45, `aligner_cli
five_base` 6/6, `cargo fmt --check` clean. 5-Base summary now prints `genome:` / `5-Base index:` /
`FASTA(s):` only.

## 9. Corrections to rev 0 (for the record)

1. `config.rs:1609` is `RunConfig::summary()` printed unconditionally at `mod.rs:276`, **not** a `Debug`
   impl — so the fabricated basenames *are* user-visible (→ D2).
2. `parallel.rs:597` is a **fourth** reader of `ct_index_basename`; the audit was incomplete.
3. §7's "existing discovery tests cover them" was **false** — `GenomeFolder` has zero coverage.
4. §8's byte-gate/Perl-mirror rationale for `C->T->converted` was **false on both counts**.
5. Rev-0 tests 4/5 could not pass in CI (`resolve` execs the aligner; no bowtie2 on the runners), and
   the rev-0 §6 recipe failed on the `-o` directory rather than where it predicted.
