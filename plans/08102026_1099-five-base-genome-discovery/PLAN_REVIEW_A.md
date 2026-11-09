# PLAN_REVIEW_A — #1099 `--illumina_5base` must not require the converted CT/GA indexes

**Reviewer:** A (independent; no shared state with Reviewer B)
**Plan reviewed:** `plans/08102026_1099-five-base-genome-discovery/PLAN.md` rev 0
**Method:** every factual claim in the plan checked against the source on `dev` (`6ecf8a7`), plus
empirical runs of the existing debug build `rust/target/debug/bismark` and `cargo test -p bismark
--lib aligner::discovery` (21/21 green baseline).

**Verdict: the diagnosis and the chosen design are correct; the test set is not shippable as
written.** Two of the six proposed tests would be red on the CI runners for a reason unrelated to the
bug, the strongest available regression test is already sitting in the repo unused, and one
user-visible consequence of the fix is mis-analysed in the plan (it is not a `Debug` impl). None of
this touches the core one-branch change, which I would merge as designed.

---

## 1. Logic review

### 1.1 What checks out

Verified independently, claim by claim:

| Plan claim | Verified |
|---|---|
| `config.rs:847` calls `discover_genome` unconditionally | ✅ exact line |
| `discover_genome` errors `FaultyIndex` unless the CT/GA set is complete (`discovery.rs:143-166`) | ✅ (plan cites 143-161; the block runs to 166) |
| `--bowtie2`/`--hisat2` 5-Base aligns against `--five_base_index` (`mod.rs:1558-1569`) | ✅ |
| default 5-Base passes the FASTA positionally (`mod.rs:1571-1577`) | ✅ |
| the `if !cli.illumina_5base` precedent sits one line above (`config.rs:837-839`) | ✅ |
| empty placeholder index files let a 5-Base run proceed | ✅ reproduced end-to-end (below) |
| `--genome` is still genuinely required | ✅ `run_pe_five_base` reads `config.genome.fastas` (`mod.rs:1736`, `1588`) and `genome_dir` for the report (`mod.rs:1739`) |
| the faithful paths are untouched, so the Perl-oracle gates are unaffected | ✅ `discover_genome` is not edited; the report line uses the genome *folder*, never an index basename (`report.rs:68`) |

Reproduction, both directions, on this machine:

```
$ bismark --illumina_5base --bowtie2 --five_base_index $TMPDIR/bogus --genome $TMPDIR/g \
          -1 test_files/test_R1.fastq.gz -2 test_files/test_R2.fastq.gz -o $TMPDIR/out
error: the Bowtie 2 index of the C->T->converted genome seems to be faulty or non-existant
('BS_CT.1.bt2l'). Please run the bismark_genome_preparation before running Bismark
```

then with twelve `touch`ed empty files it runs through discovery, loads `chr pUC19 (2686 bp)`, opens
the output BAM and only dies in bowtie2. The plan's "proof they are never read" is sound.

### 1.2 The ct/ga reader audit is incomplete (the fix rests on it)

The plan says the converted basenames are read "only on bisulfite paths (`mod.rs:1211-1212`,
`1345/1350`, `4633-4634`)". There is a **fourth** reader the plan misses:

```rust
// parallel.rs:597
let detail = match estimate_index_bytes(&cfg.genome.ct_index_basename) {
```

The conclusion survives — `pipeline()` short-circuits `config.five_base` to `run_pe_five_base`
(`mod.rs:943-953`) **before** the `n > 1` fork dispatch (`mod.rs:1031-1032`), so
`emit_memory_warning` is unreachable for 5-Base even with `--multicore N`. (Consistent with
`aligner_cli.rs:6163`, which passes `--multicore 4` to a 5-Base run today and still asserts a
single-instance option string.) But the whole fix rests on this audit, so it should be stated
completely, and the `--multicore` reasoning should be written down — it is the non-obvious half.

### 1.3 `summary()` is not a `Debug` impl, and this is user-visible

The plan's §1 says "`large_index` is read only by a `Debug` impl (`config.rs:1609`)" and §3 calls it
"display-only". Both are wrong. `config.rs:1609` is inside `RunConfig::summary()` (`config.rs:1568`,
doc-commented *"A human-readable resolved-config summary (STDERR; not byte-gated)"*), and it is
printed **unconditionally for every run** at `mod.rs:276`. Observed, on the placeholder-index
reproduction above:

```
genome:         /private/tmp/claude-501/revA/g
CT index:       /private/tmp/claude-501/revA/g/Bisulfite_Genome/CT_conversion/BS_CT
GA index:       /private/tmp/claude-501/revA/g/Bisulfite_Genome/GA_conversion/BS_GA
large index:    false
```

After the fix, a 5-Base user with a raw genome folder gets those three lines naming two paths that
**do not exist on their disk** — on the very run whose error message just told them to build that
index. That is the concrete form of the requirement issue #1099 itself states:

> the care needed is in what `GenomeIndexes` should carry on that path, since
> `ct_index_basename`/`ga_index_basename`/`large_index` are meaningless for 5-Base and **should not
> be silently fabricated**.

The plan fabricates them and defends it on the grounds that nothing observes them. Something does.
Fix: branch `summary()` on `self.five_base` (already on `RunConfig`, no new field) and print the
reference FASTA and the `--five_base_index` instead of CT/GA/large. Checked: no test anywhere asserts
on the summary text, and the persisted `*_report.txt` never carries an index basename, so the change
is contained.

### 1.4 The over-reach risk, honestly bounded

Asked directly: can the fix cause a *wrong result*? No, not today. If the branch over-reached and a
bisulfite run took the unconverted path, the run would fail loudly at bowtie2 spawn on a missing
`-x` index — availability, not silent miscalls. The one silent-wrong-calls scenario requires both a
*prepared* genome and a 5-Base run reaching a bisulfite spawn, and `pipeline()`'s unconditional
`return run_pe_five_base(...)` makes that unreachable (as does 5-Base's PE-only + no-rammap
guarding, which rules out the SE rammap reader at `mod.rs:1345/1350`).

That bound is worth writing into §7, because it is the justification for the light test set — and it
is also the honest cost of rejecting the `Option` refactor: the fabricated paths convert a *future*
mis-wiring from "loud panic" into "aligns against the converted genome with inverted polarity". The
field doc-comment the plan proposes is the only guard, and a doc comment does not fail CI.

### 1.5 The drift argument counts two callers; there are three

§3 justifies `absolute_genome_dir` "so the two entry points cannot drift". There is already a third
copy of exactly that prologue:

```rust
// mod.rs:543-546 — run_five_base_consensus_standalone
let genome_dir = std::fs::canonicalize(genome_arg).map_err(|e| {
    AlignerError::Validation(format!("consensus: --genome {}: {e}", genome_arg.display()))
})?;
let (fastas, _kind) = crate::aligner::discovery::discover_fastas(&genome_dir)?;
```

`--five_base_consensus_from_bam` bypasses `resolve` entirely (`mod.rs:178-179`) and hand-rolls
canonicalize + `discover_fastas` — with a different error type and **no `is_dir` check**. That is
`discover_genome_unconverted`'s second caller, waiting for it to exist. Either fold it in (the error
text is stderr diagnostics, not byte-gated) or state in the plan why the third copy stays. Leaving it
is the drift the plan's own rationale argues against.

Note also that this path is independent evidence for the design: a 5-Base entry point that only ever
needed the FASTA already exists and already does the FASTA-only thing.

---

## 2. Assumptions

### 2.1 Stated assumptions that hold

- **`discover_fastas` is reusable as-is** — ✅ `pub(crate)`, `discovery.rs:200`, no aligner argument.
- **`--combined_index` is already rejected for 5-Base, so `combined_index_basename: None` is safe** —
  ✅ and the *ordering* is what makes it safe: the 5-Base combined rejection (`config.rs:820-830`)
  precedes the combined-presence guard (`config.rs:850`). Fragile coupling 30 lines apart; worth one
  line of comment, not a redesign.
- **5-Base has no Perl oracle** — ✅ concordance-gated only; `perl-oracle` CI cannot regress here.

### 2.2 Stated assumption that is false

> §7: "The prologue extraction subtly changing `discover_genome`'s errors — **existing discovery
> tests cover them**"

They do not. `AlignerError::GenomeFolder` has **zero** test coverage anywhere in the crate — grepped
`rust/bismark/src` and `rust/bismark/tests`; the identifier appears only at its two construction
sites (`discovery.rs:134`, `:136`) and its `thiserror` definition. The 21 green discovery tests cover
`FaultyIndex`, `NoFasta`, fasta ordering and per-aligner arity — never the genome-folder prologue.

So the stated mitigation for the extraction risk does not exist. The plan's own test 3 covers only
the `!is_dir` branch of the **new** function. Extracting shared code is exactly when you pin both
branches for both entry points.

### 2.3 Unstated assumptions the plan needs to surface

1. **`resolve` execs the aligner binary.** `detect_aligner` (`config.rs:864`) runs `<aligner>
   --version` and returns `Err(AlignerNotWorking)` if absent — it sits *after* `discover_genome`
   (847). Tests 4/5 therefore assert `Ok` from a function that shells out. This is the source of
   Critical item C1 below. (`config.rs:1626` documents this exact hazard for `run_config_stub`:
   *"Exists because `resolve` cannot be used: it execs `<aligner> --version`"*.)
2. **`resolve` requires the read files to exist on disk.** `resolve_layout` → `check_exists`
   (`config.rs:1492-1493`, defined 1525) runs at line 791, *before* discovery. Tests 4/5/6 need real
   files (empty temp files suffice) or they never reach the code under test.
3. **`config.rs`'s test module is currently 100 % fixture-free** — no `tempfile`, no `test_files`,
   and not one of its ~19 `resolve` calls asserts `Ok`: every one either `unwrap_err()`s an early
   guard or uses the "this guard did not fire" shape (§4.1). Tests 4/5/6 would be the first
   on-disk-fixture tests there. Not a blocker (`tempfile` is already a dev-dep, used by
   the `discovery.rs` tests), but it is a change in the character of that module and the plan should
   say so.
4. **The CHANGELOG path.** There is no `rust/CHANGELOG.md`. Task 3 means `CHANGELOG.md` at the repo
   root, `## Unreleased` (line 4) → `### bismark (aligner)` (line 14).
5. **`first_missing` and `index_suffixes` are private** (`discovery.rs:123`, `:103`). Task 4's
   "reuse `discovery::first_missing`" needs a visibility bump.

---

## 3. Efficiency

Nothing to flag. `discover_genome_unconverted` *removes* 12–16 `is_file()` stats plus the combined-index
probe from the 5-Base path; `discover_fastas` is unchanged. Against a genome load and an alignment
this is unmeasurable in either direction. No memory or scalability implications: `GenomeIndexes` gains
no field and the struct is cloned once per run.

One pre-existing, out-of-scope observation: `discover_fastas` re-`read_dir`s the genome folder up to
four times (once per extension group, `discovery.rs:201-221`). Irrelevant at this scale; do not touch
it in a bug-fix branch — it is byte-order-significant code shared with genome-prep.

---

## 4. Validation sufficiency

This is where the plan needs the most work. The proposed set is six tests; as written, two cannot
pass in CI, one can pass vacuously, and the two best tests in the repo are not used.

### 4.1 Test 4 would be red in CI (Critical)

> 4. `five_base_bowtie2_resolves_without_converted_index` — `--illumina_5base --bowtie2
>    --five_base_index <anything>` over an unprepared genome → `Ok`.

Post-fix, `resolve` gets past discovery and then execs `bowtie2 --version`. `.github/workflows/rust_ci.yml`
installs **minimap2 + samtools only** — for all three `cargo test -p bismark` jobs (lines 35-44,
96-104, 136-144). No job installs bowtie2 or hisat2. Demonstrated with the current binary, PATH
stripped, on a genome whose index check already passes:

```
$ env PATH=/usr/bin:/bin bismark --illumina_5base --bowtie2 --five_base_index … --genome … -1 … -2 …
error: failed to execute Bowtie 2 properly (could not run 'bowtie2 --version'). …
```

So test 4 is green on Felix's Mac (bowtie2 on PATH) and red on every CI runner — the classic
green-locally trap, landing on a currently-green job. Test 5 (minimap2) happens to survive because
minimap2 *is* installed, but only by luck of that install list.

**Fix — use the idiom this file already has for exactly this problem, on 5-Base tests specifically.**
`five_base_duplex_guards` (`config.rs:2105-2122`) is the closest precedent, comment and all:

```rust
// PE + duplex is ALLOWED (the PE duplex report): no single-end rejection. resolve
// may still fail for unrelated reasons (no genome), but not on the SE guard.
if let Err(e) = resolve(&cli_from(&["--illumina_5base", "--five_base_duplex", "-1", …]), …) {
    assert!(!e.to_string().contains("single-end only"), "…: {e}");
}
```

Six of the module's ~19 `resolve` calls use that shape (`:1860`, `:1950`, `:2107`, `:2135`, `:2157`,
`:2185`); the rest are `unwrap_err()`. **Not one asserts `Ok`** — which is the module telling you that
`resolve` is not `Ok`-assertable in a unit test. Write test 4 the same way, with a typed match:

```rust
if let Err(e) = resolve(&cli_from(&[...]), "cmd".into()) {
    assert!(
        !matches!(e, AlignerError::FaultyIndex { .. }),
        "5-Base must not require the converted index; got: {e}"
    );
}
```

Typed `matches!`, not a substring. That fails before the fix (`FaultyIndex`), passes after it whether
the run ends `Ok` (bowtie2 present) or `Err(AlignerNotWorking)` (CI), and is aligner-install-agnostic.

### 4.2 Tests 4/5/6 can pass vacuously (Critical)

`check_exists` runs at line 791, before discovery. If the tests do not create the read files, all
three fail out at `InputFileMissing` — and test 6 (`unwrap_err()` → assert `FaultyIndex`) would
**pass for the wrong reason** if written as `is_err()`. Two requirements for the implementer: create
real (empty is fine) temp read files, and assert the typed variant everywhere. Same for test 6's
sabotage check — under sabotage it must fail *because the error is no longer `FaultyIndex`*, not
because the error changed shape for some other reason.

### 4.3 The strongest regression test already exists and is not used (Important)

Two existing 5-Base **end-to-end** tests already carry this bug's workaround, with comments that name
it:

```rust
// aligner_cli.rs:6167  (five_base_pe_end_to_end_inverts_polarity)
make_genome_mmi(genome.path()); // genome.fa = chr1 ACGTACGT + BS_*.mmi for discovery

// aligner_cli.rs:6269  (five_base_bowtie2_unconverted_index_end_to_end)
make_genome(genome.path()); // CT/GA .bt2 (for discovery) + genome.fa chr1 ACGTACGT
```

```rust
// aligner_five_base_groundtruth.rs:93-101  (write_genome; write_genome_multi is the same at :754)
/// Genome dir: raw `genome.fa` (the 5-Base path aligns against it) + dummy `BS_*.mmi`
/// so index discovery passes (the 5-Base path passes the FASTA directly, not the mmi).
```

Both `aligner_cli.rs` tests are **hermetic** — a fake `bowtie2`/`minimap2` shell shim supplied via
`--path_to_*`, so they need no real aligner and run on any runner — and both assert the full chain:
BAM records, the `XM` tag (`.Z...z`), the report's option string. `aligner_five_base_groundtruth.rs`
adds real-minimap2 synthetic-ground-truth runs over 7 call sites of the two helpers.

Post-fix, deleting the dummy-index writes from those helpers (a FASTA-only genome) turns 13 existing
call sites into a regression gate for #1099 that is end-to-end, CI-portable, covers **both** engine
routes, and *removes* scaffolding instead of adding tests. It dominates plan tests 4 and 5 on every
axis. This should be a task, not an afterthought.

### 4.4 The over-reach guard already exists too (Important)

The plan calls test 6 "the guard that the fix did not over-reach". A stronger one is already in the
tree: `aligner_cli.rs:423-445` `missing_index_errors` runs the real binary against a genome with one
`BS_CT.3.bt2` removed and asserts `.failure().code(1).stderr(contains("faulty or non-existant"))`.
Note it installs **no** fake bowtie2 — it passes on CI today precisely because discovery errors
before `detect_aligner`. Make the branch unconditional and it goes red (the error becomes
`AlignerNotWorking`, so the stderr predicate fails).

Test 6 is still worth adding — it is fast, typed, and unit-level — but the plan should name
`missing_index_errors` as the existing end-to-end guard, and the sabotage check should require **both**
to go red. That is a far more convincing "did not over-reach" demonstration than one new unit test.

### 4.5 Gaps

- **No test for the canonicalize-failure branch** of the extracted prologue (nonexistent path), for
  either entry point — and see §2.2: there is no coverage of `GenomeFolder` at all today.
- **No `.fa.gz`-only genome case.** `five_base_reference_fasta` (`mod.rs:1587-1591`) hands a single
  `.fa.gz` straight to minimap2 positionally. Fine (minimap2 reads gzip), untested for a genome with
  no index. Cheap to add to the sibling's tests via `fasta_kind == FaGz`.
- **Nothing asserts the summary output**, which is the one user-visible artifact the fix changes
  (§1.3). If C2 is taken, assert it: a 5-Base summary must not contain `CT index:`.

---

## 5. Alternatives

### 5.1 Additive sibling vs. the `Option<PathBuf>` refactor — the plan's call is right, its reasoning is not

**Keep the additive sibling; reject the `Option` refactor as scoped.** Agreed with the plan, for a
reason it does not give. The refactor's real cost is not "six call sites": it is that four of the five
readers (`mod.rs:1211-1212`, `1345`, `1350`, `4633-4634`) are on byte-frozen alignment paths, so the
refactor either sprinkles `expect()` panics there — a panic is a worse regression than a cosmetic
display line — or threads new `Result` plumbing through code whose whole value is that it has not
changed since it was proven byte-identical. That is a real risk taken to fix a display problem, on a
bug-fix branch, and the plan is right to decline it. (For accuracy: I count **five** reader
expressions, not six — three of them are `match` arms and one is `parallel.rs:597`.)

But the plan's stated reason — that the fabrication is invisible — is false (§1.3), and the issue
explicitly asks for the values not to be fabricated silently. **Take C2 instead**: branch `summary()`
on `self.five_base`. That is ~6 lines, no struct churn, no `unwrap`, and it removes the only place
the fabrication is observable, which satisfies the issue's requirement in substance. Combined with
the field doc-comment the plan already proposes, that is the right trade.

**Do not** add a `converted_indexes_validated: bool` provenance field: `RunConfig.five_base` already
answers the only question anyone asks, and a new `GenomeIndexes` field breaks `run_config_stub`
(`config.rs:1632`, deliberately non-`Default` so a new field breaks loudly) for no gain.

If you want a fraction of the `Option`'s safety without its cost, a `debug_assert!(!config.five_base)`
at the two bisulfite spawn readers buys the loud-failure property back in CI (tests run with debug
assertions; release builds compile it out, so byte-frozen output cannot be affected).
`run_pe_five_base` spawns its own `std::process::Command` (`mod.rs:1861`) and shares nothing with
`PairedAlignerStream::spawn`, so the assertion cannot fire on a legitimate 5-Base run. Optional.

### 5.2 Rejecting the `require_converted_index: bool` parameter — agreed, no notes

Correct call for the stated reason. Nothing to add.

### 5.3 Task 4 — the plan is under-selling it, and the issue asked for it

The plan flags Task 4 (fail early on a missing `--five_base_index`) as "separable — cut if the
reviewer prefers … a second defect, not this bug". Two corrections.

**First, the issue asks for it in scope:** *"Worth covering in the same change: a 5-Base run should
fail loudly and early on a missing `--five_base_index`."* Cutting it is Felix's call, not a
reviewer's; the plan should not treat reviewer silence as consent.

**Second, today's failure is worse than "late" — it is misattributed.** From the reproduction:

```
(ERR): "/tmp/claude-501/revA/bogus_idx" does not exist or is not a Bowtie 2 index
Exiting now ...
error: minimap2 produced fewer PE records than read pairs (desync)
```

That is a `--bowtie2` run, and Bismark's own final error names **minimap2** (`mod.rs:1912`,
hard-coded). A user sees a bowtie2 complaint buried above a minimap2 desync error and has to guess.
That strengthens the case for Task 4, and it exposes a second one-word wording bug worth a line in
the plan even if Task 4 is cut.

**If Task 4 is kept, two implementation hazards:**

1. `first_missing`/`index_suffixes` are private (`discovery.rs:123`, `:103`) → bump to `pub(crate)`.
2. **A bare `--five_base_index` basename can legitimately resolve via `BOWTIE2_INDEXES`** (confirmed:
   the string is present in `/opt/homebrew/bin/bowtie2-align-s`; HISAT2 has `HISAT2_INDEXES`). A hard
   presence check on the literal basename would falsely reject a working setup — a *new* spurious
   rejection introduced by a fix for a spurious rejection. This has never mattered for the bisulfite
   path (which always builds an absolute basename from the genome folder), so `--five_base_index` is
   the first place it is reachable. Mitigate by checking only when the basename contains a path
   separator, by honouring the env var, or by warning instead of erroring.

### 5.4 Naming (weak preference)

`discover_genome_unconverted` overloads "unconverted", which in 5-Base vocabulary describes the
*reference* rather than the *discovery mode* — `--five_base_index` points at an unconverted index and
this function has nothing to do with it. `discover_genome_fasta_only` says what it does. Defensible
either way; not worth a round trip.

---

## 6. Action items

### Critical

- **C1 — Test 4 cannot pass in CI.** `resolve` execs `bowtie2 --version` (`config.rs:864`, after
  discovery at 847) and `rust_ci.yml` installs minimap2 + samtools only. Replace the `→ Ok`
  assertion with the `if let Err(e) = resolve(…)` + `!matches!(e, AlignerError::FaultyIndex { .. })`
  idiom the module already uses for 5-Base guards (`config.rs:2105-2122`, `five_base_duplex_guards`).
  Apply the same shape to test 5 (it survives only because minimap2 happens to be installed).
- **C2 — `summary()` is not a `Debug` impl; fix the plan's analysis and the output.**
  `RunConfig::summary()` (`config.rs:1568`) is printed unconditionally at `mod.rs:276`, so after the
  fix every 5-Base run prints `CT index:` / `GA index:` / `large index:` for paths that do not exist.
  Branch on `self.five_base` (no new field; nothing asserts the summary text today). This is the
  issue's own "should not be silently fabricated" requirement, and it is the thing that makes
  rejecting the `Option` refactor defensible.
- **C3 — Tests 4/5/6 need on-disk read files and typed assertions.** `check_exists`
  (`config.rs:1492-1493`) runs before discovery, so without real read files all three exit at
  `InputFileMissing` — and test 6, the over-reach guard, would pass vacuously. Assert
  `matches!(err, AlignerError::FaultyIndex { .. })`, never `is_err()`. Note in the plan that these
  are the first fixture-carrying tests in `config.rs`'s module.

### Important

- **I1 — Add a task to repoint the existing 5-Base test helpers at a FASTA-only genome.**
  `aligner_cli.rs:6167` / `:6269` and `aligner_five_base_groundtruth.rs:95` / `:754` (13 call sites)
  all write dummy CT/GA index files solely to get past discovery, with comments saying so. Dropping
  those writes post-fix is an end-to-end, CI-portable, both-engines regression gate that deletes
  scaffolding instead of adding tests.
- **I2 — Name `aligner_cli.rs:425 missing_index_errors` as the existing over-reach guard**, and make
  the sabotage check require both it and test 6 to go red. It is end-to-end and it passes on CI today
  only because discovery errors before `detect_aligner`, which is exactly the property under test.
- **I3 — §7's mitigation "existing discovery tests cover them" is false.** `AlignerError::GenomeFolder`
  has zero coverage crate-wide. Add both prologue branches (canonicalize failure + `!is_dir`) for
  `discover_genome` as well as the sibling — the extraction is precisely when to pin them.
- **I4 — Complete the ct/ga reader audit:** `parallel.rs:597` is a fourth reader. Record why it is
  unreachable for 5-Base (`pipeline()` short-circuits `five_base` at `mod.rs:943-953` before the
  `n > 1` fork dispatch, so `--multicore N` never routes 5-Base through `parallel::run_pe_multicore`).
  The fix rests on this audit being exhaustive.
- **I5 — The drift argument counts two callers; there are three.**
  `run_five_base_consensus_standalone` (`mod.rs:543-546`) already hand-rolls canonicalize +
  `discover_fastas`, with a different error and no `is_dir` check. Either make it the sibling's second
  caller (accepting the stderr text change) or say why it stays.
- **I6 — Task 4 is requested by the issue** ("worth covering in the same change"), so cutting it needs
  Felix's sign-off. If kept: bump `first_missing`/`index_suffixes` visibility, and do not hard-reject
  a bare basename that `BOWTIE2_INDEXES`/`HISAT2_INDEXES` can resolve. Either way, record the
  misattributed error (`mod.rs:1912` says "minimap2" on a bowtie2 run).
- **I7 — Task 3 is under-specified and docs are missing.** Name `CHANGELOG.md` at the repo root
  (there is no `rust/CHANGELOG.md`), `## Unreleased` → `### bismark (aligner)` (line 14). Add one
  sentence to `docs/src/content/docs/rust/illumina-5-base.md` ("Running it", ~line 44) saying the
  genome folder needs only the FASTA — no `bismark_genome_preparation` — which is the sentence that
  would have prevented the report. Precedent: the #1095 commit (`6a3ea0d`) updated that page plus
  `rust/README.md` alongside the CHANGELOG.

### Optional

- **O1** — One line noting that `combined_index_basename: None` is safe only because the 5-Base
  `--combined_index` rejection (`config.rs:820-830`) precedes the presence guard (`:850`).
- **O2** — State the blast-radius bound in §7: over-reach costs availability, not silent miscalls
  (a bisulfite run on the unconverted path dies at bowtie2 spawn). It justifies the light test set.
- **O3** — `debug_assert!(!config.five_base)` at the two bisulfite spawn readers, to buy back the
  loud-failure property the `Option` refactor would have given. Compiles out in release, so byte-frozen
  output is unaffected; `run_pe_five_base` spawns its own `Command` and cannot trip it.
- **O4** — Add a `.fa.gz`-only genome case to the sibling's tests (`fasta_kind == FaGz`), the shape
  `five_base_reference_fasta` passes straight to minimap2.
- **O5** — Watch `clippy::doc_lazy_continuation` when appending to the two field doc-comments: a
  wrapped `///` line must not start with `- `/`+ `/`* ` under `-D warnings`.
- **O6** — Verification block: add the tests that actually cover this —
  `cargo test -p bismark --test aligner_cli five_base` and
  `cargo test -p bismark --test aligner_five_base_groundtruth`.
- **O7** — Naming: `discover_genome_fasta_only` reads more plainly than
  `discover_genome_unconverted`, which collides with 5-Base's "unconverted index" vocabulary.

---

## 7. Baseline established during review

- `cargo test -p bismark --lib aligner::discovery` → 21 passed, 0 failed (pre-change baseline).
- Bug reproduced and the placeholder-file workaround confirmed with `rust/target/debug/bismark`.
- `detect_aligner`'s post-discovery position and its `AlignerNotWorking` failure confirmed by running
  with `PATH` stripped of bowtie2.
- `rust_ci.yml` aligner installs confirmed: minimap2 + samtools, never bowtie2/hisat2, in all three
  `cargo test -p bismark` jobs.
