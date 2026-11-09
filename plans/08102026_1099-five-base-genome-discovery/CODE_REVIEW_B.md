# CODE REVIEW B — #1099: `--illumina_5base` must not require the converted CT/GA indexes

**Reviewer:** B (independent, fresh context)
**Target:** uncommitted working tree on branch `1099-five-base-genome-discovery`
**Scope reviewed:** `git diff` over `rust/bismark/src/aligner/{discovery,config,mod}.rs`,
`rust/bismark/tests/{aligner_cli,aligner_five_base_groundtruth}.rs`, `CHANGELOG.md`,
`docs/src/content/docs/rust/illumina-5-base.md`, plus `plans/08102026_1099-five-base-genome-discovery/`
(PLAN.md rev 1, PLAN_REVIEW_A.md, PLAN_REVIEW_B.md).
**Excluded by instruction:** `SESSION_HANDOFF.md` (unrelated edit travelling from `dev`).

> **OVERRIDE NOTED.** The `code-reviewer` skill normally says to fix unambiguous low-risk problems
> directly. I applied **nothing**. Three agents are reviewing the same uncommitted working tree
> concurrently, so any edit to a tracked file would race. The only file I wrote is this report.
> Everything below is a recommendation.

---

## Verdict

**APPROVE WITH CHANGES.** The diagnosis is right, the design is the right one, and the
load-bearing claim — that no 5-Base path reads the converted indexes — holds under an
independent audit I ran myself (§"Independently verified" below). The faithful bisulfite path is
untouched and its summary output is byte-identical, so `perl-oracle` cannot regress.

One **High** finding is a mechanical editing slip that should be fixed before the commit lands
(`discover_genome`, a `pub` function, lost its doc comment to the extracted helper). The rest are
two documentation-accuracy corrections, a summary line that is wrong for the common multi-FASTA
genome, and polish. Nothing here changes the fix's behaviour or blocks the approach.

On the question the lead asked most sharply — **did repointing the tests lose coverage?** Yes, but
less than the plan's §10 says it retained, and the loss is low-risk. Details in M5.

---

## Issues by area

### Logic

**H1 (High) — `discover_genome`'s doc comment was absorbed by the extracted helper.**
`rust/bismark/src/aligner/discovery.rs:134-140`:

```rust
/// Discover the genome folder, validate the bisulfite indexes for `aligner`
/// (Bowtie 2 `.bt2` or HISAT2 `.ht2`), and inventory the raw FASTA file(s).
/// The genome folder as an absolute path (Perl `chdir`+`getcwd`).
///
/// Both failure modes must stay `GenomeFolder`; a plain `?` on `canonicalize` compiles
/// and downgrades it to `AlignerError::Io`.
fn absolute_genome_dir(genome_arg: &Path) -> Result<PathBuf> {
```

The first two lines describe `discover_genome`, not `absolute_genome_dir`. The extraction inserted
the new signature *below* the old doc block instead of above it, so:

- `discover_genome` (line 149) — a **`pub`** item, and the only public entry point in the module —
  now has **no doc comment at all**, a visible regression in `cargo doc`.
- `absolute_genome_dir`, a 7-line private helper, carries a 5-line doc block that opens by
  describing a different function.

**Recommendation:** move the `/// Discover the genome folder, validate the bisulfite indexes …`
pair back onto `discover_genome` and leave `absolute_genome_dir` with its own one-liner. See L5 for
the house-style trim I would apply to what remains.

**M1 (Medium) — `GenomeIndexes::combined_index_basename`'s doc is now false, and it is the field
the doc pass missed.** Plan Task 1.3 said to document *why* the unprobed fields are safe. Three
fields got that treatment (`ct_index_basename`, `ga_index_basename`, `large_index`). The fourth did
not, and its **pre-existing** doc (`discovery.rs:87-93`) now states something the diff falsified:

> `Some` iff a complete set (small `.bt2` OR large `.bt2l`) is present; probed best-effort for
> **every** run (the cost is a directory stat).

`discover_genome_fasta_only` probes nothing and hard-codes `None`, so "every run" is no longer
true. This matters more than the fields that *were* documented, for two reasons:

1. `combined_index_basename` has ~20 readers in `mod.rs` (3159, 3259, 3370, 3558, 3669, 3979,
   4094, 4215, 4326, 5202, 5309, 5439, 5657, 5782, 6209, 6342, 6440, 6561), against
   `large_index`'s **one**. A reader auditing safety will look here first and find a false premise.
2. Its guarantor is **different** from the CT/GA one. Those are safe because `pipeline()`
   short-circuits `five_base` before any reader. `combined_index_basename` is safe because all four
   combined flags are rejected for 5-Base at `config.rs:835-845` (`combined_index`,
   `combined_index_sequential`, `combined_index_single_pass`, `combined_index_parallel` — I checked
   the guard covers all four). The `config.rs:865` comment records the *presence-guard* half of
   this, but the field itself says nothing.

**Recommendation:** add one line to the field doc naming the flag rejection as the guarantor, and
fix "every run" → "every bisulfite run".

**M2 (Medium) — the new field docs cite line numbers, and one is already wrong — invalidated by
this very diff.** `discovery.rs:78-79`:

```rust
/// Unvalidated after `discover_genome_fasta_only`: `pipeline()` routes `five_base` to
/// `run_pe_five_base` (`mod.rs:943`) before any reader stats it (`parallel.rs:597`).
```

`mod.rs:943` is the `unreachable!("5-Base single-end is rejected at config::resolve()")` inside the
`SingleEnd` arm — not the short-circuit. The guard is `if config.five_base {` at **mod.rs:940** and
the return is at **mod.rs:949**. The plan wrote `mod.rs:943-953` against the pre-diff file; the D4
hunk in this same commit removes three lines and adds one at `mod.rs:540`, shifting everything below
by −2. So the citation was stale the moment it was written, and will drift again on the next edit to
`mod.rs`. (`parallel.rs:597` is correct today — it is the `estimate_index_bytes(&cfg.genome.ct_index_basename)`
call in `emit_memory_warning`.)

**Recommendation:** cite the **function names**, which are stable, not line numbers:
"`pipeline()` returns `run_pe_five_base` for `five_base` before the fork dispatch, so the nearest
reader (`emit_memory_warning`) is unreachable." Same for the `parallel.rs:597` reference.

**M3 (Medium) — `summary()`'s new `reference:` line is wrong for a multi-FASTA genome.**
`config.rs:1610-1616` renders `self.genome.fastas.first()`. But the default minimap2 route does not
pass the first FASTA to minimap2 when there is more than one — `five_base_reference_fasta`
(`mod.rs:1584-1606`) concatenates **all** of them into `<output_dir>/.bismark_5base_concat_ref.fa`
and passes that:

```rust
let fastas = &config.genome.fastas;
if fastas.len() == 1 {
    return Ok((fastas[0].clone(), None));
}
let tmp = config.output.output_dir.join(".bismark_5base_concat_ref.fa");
```

A per-chromosome genome folder is the common GRCh38 layout, so this is not an exotic case: the
summary would name `chr1.fa` as "the reference" for a run that aligns against a 25-file
concatenation. The `FASTA(s): N file(s)` line immediately below does disclose N, which is what
keeps this Medium rather than High — a careful reader can spot the contradiction.

The concat path cannot be named at summary time (the summary prints at `mod.rs:276`, the concat
happens inside `run_pe_five_base`), so the honest options are:

- **Preferred:** drop `reference:` entirely and print only `5-Base index:`. `genome:` (the folder)
  and `FASTA(s): N file(s) (Fa)` already carry the reference identity, so the line is redundant even
  when it is correct, and dropping it shrinks the diff while still satisfying D2 (the fabricated
  `CT index:`/`GA index:` lines stop being printed either way).
- Or render `reference:      {N} FASTA file(s) under {genome_dir}`.

Related, trivially: `.first()...unwrap_or_else(|| "-".to_string())` is dead code —
`discover_fastas` returns `Err(NoFasta)` on an empty group, so `fastas` is non-empty by
construction whenever a `RunConfig` exists.

### Errors

**M4 (Medium) — the D4 change (`mod.rs:543`) has zero test coverage.** It replaces a hand-rolled
`canonicalize` + `discover_fastas` with the new sibling, which changes the error variant on that
path (`AlignerError::Validation("consensus: --genome …: {e}")` → `GenomeFolder` / `NoFasta`) and
adds a **new** rejection (`!is_dir`, previously an `Io` from `read_dir`).

The only test in the suite that passes `--five_base_consensus_from_bam` is
`aligner_five_base_bisulfite.rs:358` `rejects_both_standalone_bam_modes_together`, which is a
mutual-exclusion rejection — it returns long before genome resolution. So no test reaches
`run_five_base_consensus_standalone`'s genome line at all, before or after.

The change is an improvement (it gains the `is_dir` check and deletes a third copy of the prologue,
which is exactly what I5 asked for) and the plan documented the stderr-text loss. I am not asking
for a test as a merge gate — the path was uncovered before this diff too. Flagging it because the
diff's only *behaviour* change with no coverage anywhere is worth knowing about, and because the
error-text change is user-visible.

**M5 (Medium) — Task 5 (the desync aligner name) is also untested, but the CHANGELOG advertises
it.** `mod.rs:1905-1910` now formats `config.aligner.name()`. I confirmed `name()`
(`config.rs:57-65`) returns `"Bowtie 2"` / `"HISAT2"` / `"minimap2"`, so the message reads well and
matches the report's own "Bismark was run with Bowtie 2 against" wording. But nothing asserts that a
`--bowtie2` 5-Base desync says "Bowtie 2", and the CHANGELOG makes a specific claim about it.

**Recommendation (cheap, optional):** a variant of `make_fake_bowtie2_five_base_pe` that emits one
record per pair instead of two trips the `(Some(a), Some(b))` else-arm directly; assert the stderr
names Bowtie 2. Roughly 20 lines against a CHANGELOG sentence a user may quote back.

### Structure

**M6 (Medium) — the prepared-genome coverage claim in plan §10 is overstated; only one site
survives, and the bowtie2 route has none.** Plan §10 says:

> **`make_genome_mmi` deliberately retained** at the other three 5-Base sites (`aligner_cli.rs`
> ~`:6228`, `:6347`) so 5-Base-on-a-*prepared*-genome stays covered — the gap A and B both warned
> about.

I traced each retained call site against the guard order in `resolve`:

| Site | Helper | Reaches discovery (`config.rs:862`)? | Why |
|---|---|---|---|
| `five_base_umi_dedup_drops_duplicates` (`:6353`) | `make_genome_mmi` | **Yes** — real 5-Base run, succeeds end-to-end | the one surviving gate |
| `five_base_rejects_non_directional` (`:6234`) | `make_genome_mmi` | **No** | rejected at `config.rs:816-822`, before `:862` |
| `five_base_bowtie2_requires_index` (`:6327`) | `make_genome` | **No** | rejected inside `resolve_aligner` (`config.rs:1278`) — the first statement of `resolve` |
| `five_base_umi_len_requires_illumina_5base` (`:6407`) | `make_genome` | n/a | not a 5-Base run (no `--illumina_5base`) |

So prepared-genome 5-Base coverage rests on **exactly one** test, on the **minimap2 route only**.
`five_base_bowtie2_unconverted_index_end_to_end` was the only bowtie2 5-Base success test and it was
repointed, so the bowtie2 5-Base route now has no prepared-genome coverage at all. In the other two
"retained" tests the index files are inert scaffolding — and were inert before this diff too, since
the guard order is unchanged.

**How much does this matter?** Little, and I want to be explicit about why rather than just
assigning a priority. With a prepared genome, `discover_genome_fasta_only` differs from
`discover_genome` in exactly two fields — `large_index` (would be `true` for a `.bt2l` genome) and
`combined_index_basename` (would be `Some`) — and my independent audit below shows neither is
readable on any 5-Base path once `summary()` is branched. The FASTA inventory is byte-identical
because both entry points call the same unchanged `discover_fastas`. So there is no behaviour to
regress. I also checked the one silent-wrongness scenario worth worrying about — `discover_fastas`
recursing into `Bisulfite_Genome/` and picking up a `*.CT_conversion.fa` as the reference — and it
is not reachable: `discover_fastas` uses a single non-recursive `read_dir(genome_dir)`
(`discovery.rs:243`), and `make_genome_mmi` writes no FASTA under `Bisulfite_Genome/` anyway, so
even the retained prepared-genome test would not catch such a regression.

**Recommendation:** correct the §10 claim rather than adding tests — a future reader will trust
"three sites" and act on it. If one test is added, the cheapest is a second `--bowtie2` 5-Base
end-to-end against `make_genome`, which restores the engine-route symmetry the plan intended in
§5d.9. I would not add more than that.

**M7 (Medium) — no `rust/README.md` Milestones entry.** Memory records `rust/README.md` as the
canonical status journal, and PLAN_REVIEW_A's I7 cited the #1095 precedent as "that page plus
`rust/README.md`". The plan's Task 6 kept only the CHANGELOG + docs half, and the implementation
followed the plan. But the convention covers bug fixes, not just feature merges — the three most
recent comparable aligner fixes each added a dated Milestones bullet:

- `16f65f6` (#1092, `--rammap` preset) → `- **2026-08-02** — …`
- `11efbab` (#1081, minimap2/rammap MAPQ)
- `ddc7633` (#1083, Bowtie 2 `--local` MAPQ)

and the head of the list is currently `2026-08-07` (#1095). The per-tool table row need not change
for a bug fix, but the Milestones line is the established pattern, and the existing entries all
reference their plan directory (here `plans/08102026_1099-five-base-genome-discovery/`).

**Recommendation:** add one dated Milestones bullet in the house voice. The CHANGELOG paragraph can
be condensed into it directly.

### Efficiency

Nothing to report, and I do not think there is anything to look for. `discover_genome_fasta_only`
runs once per invocation and does one `canonicalize` plus the same up-to-four `read_dir` probes
`discover_genome` already did. `summary()` allocates one extra `String` per run.
`discover_genome_fasta_only(genome_arg)?.fastas` at `mod.rs:543` builds two `PathBuf`s it discards —
immaterial, and the alternative (exposing a second entry point) would be worse.

---

## Independently verified (I re-derived these rather than trusting the plan)

These are the claims the fix rests on. I checked each against the code myself; all hold.

**The reader audit is exhaustive.** Crate-wide grep for the four fields, excluding `discovery.rs`
and the test stub:

- `ct_index_basename` / `ga_index_basename` — five readers: `config.rs:1627-1628` (`summary()`, now
  branched away for 5-Base), `mod.rs:1208-1209` (SE bisulfite spawn), `mod.rs:4631-4632` (PE
  bisulfite spawn), `mod.rs:1342/1347` (in-process rammap index load), `parallel.rs:597`
  (`emit_memory_warning`). Exactly the four the plan named, plus the summary it fixes. The three
  alignment-path readers sit after `pipeline()`'s `return run_pe_five_base` (`mod.rs:940-949`),
  which I read: `let n = config.multicore;` is at 936, the `five_base` short-circuit at 940-950, and
  the layout `match` that reaches any `n > 1` dispatch at 951 — so the structural guarantee is real
  and `--multicore N` cannot route 5-Base through `parallel::run_pe_multicore`. The rammap readers
  are unreachable because `--rammap` + `--illumina_5base` is rejected at `config.rs:1267-1272`.
- `large_index` — **one** non-test reader, `config.rs:1629`, inside `summary()`'s else-branch. So
  `large_index: false` becomes provably unobservable for 5-Base once D2 lands, which is a stronger
  statement than the plan makes.
- `combined_index_basename` — see M1; safe, but via a different guarantor than the plan documents.

**The CHANGELOG's quoted error text is exact.** `AlignerError::FaultyIndex`'s Display
(`error.rs:43-46`) renders "the {aligner} index of the {converted}->converted genome seems to be
faulty or non-existant ('{missing}'). Please run the bismark_genome_preparation before running
Bismark", which with `aligner="Bowtie 2"`, `converted="C->T"`, `missing="BS_CT.1.bt2l"` matches the
CHANGELOG quote character for character — including the double arrow and the "non-existant"
spelling. The `BS_CT.1.bt2l` detail is subtle and correct: a genome with no `Bisulfite_Genome/`
fails the small probe, falls into the large arm, and the error names the **large** file.

**The `BS_CT.mmi` claim holds.** `genome_prep/cli.rs:145-160` permits at most one of
`--bowtie2` / `--hisat2` / `--minimap2` and defaults to Bowtie 2; `genome_prep/indexer.rs:117-125`
emits `-d {basename}.mmi` only under `Aligner::Minimap2`. So `bismark_genome_preparation` writes
`BS_CT.mmi` only under `--minimap2` (or its `--mm2` alias, which the CHANGELOG does not mention —
immaterial), and a genome prepared the ordinary way really was refused on the default 5-Base route.
This is the more important half of the bug and the CHANGELOG is right to lead with it.

**"Nothing changes for any bisulfite run" holds, including the summary bytes.** The else-branch text
is byte-identical to the old inline lines: in both the old and new literals the `\`-at-end-of-line
continuations strip the newline *and* the source indentation, so the nested `format!` emits
`"CT index:       …\nGA index:       …\nlarge index:    …\n"` with no leading whitespace, and the
outer `{index_lines}\` splices it at column 0 followed directly by `FASTA(s):`. The 16-column label
padding is preserved by the new labels too (`"reference:      "` and `"5-Base index:   "` are both
16 chars). Nothing in `src` or `tests` asserts the summary text (grep for `CT index:` finds only the
format string itself), so there is no snapshot to update either way.

**`"(none: minimap2 reads the FASTA)"` cannot lie.** `five_base_index == None` implies minimap2:
rammap is rejected (`config.rs:1267`), and `--bowtie2`/`--hisat2` cannot resolve without
`--five_base_index` (`config.rs:1277-1286`).

**The prologue extraction preserves `GenomeFolder` on both arms**, and
`genome_folder_errors_survive_the_shared_prologue` pins both branches for **both** entry points —
which is what I3 asked for, and it closes a variant that had zero coverage crate-wide. The `Io`
downgrade this guards against is a genuine trap; the sabotage record shows it caught.

**No dir-creation regression from dropping `create_dir_all`.** Removing the dummy-index writes also
removed the `create_dir_all` that implicitly created the genome dir's parent chain, so
`fs::write(dir.join("genome.fa"))` now needs `dir` to exist. I checked all nine repointed call
sites: `aligner_cli.rs:6171`/`:6273` and `aligner_five_base_groundtruth.rs:159`, `281`, `439`,
`580`, `810`, `923`, `987` all pass a `TempDir::new().unwrap()` path, which exists. No latent panic.

**Scope is defensible.** Every hunk traces to plan §10 under Felix's `implement`: D2 (summary), D3
(repointing), D4 (consensus prologue), Task 5 (desync name), Task 6 (CHANGELOG + docs). Task 5 and
D4 are strictly *different* defects from #1099, but the plan labels Task 5 a ride-along that both
plan reviewers asked for, and D4 was I5. I would not ask for either to be split out — D4 in
particular is what makes the "one prologue, not three" rationale true. The CHANGELOG discloses Task
5 explicitly, which is the right call.

**The docs paragraph is accurate** and consistent with the consensus path after D4 (which also needs
only the FASTA). Nothing in it overpromises.

**On CHANGELOG length:** it is a single very long paragraph, but that is this file's established
voice — the neighbouring #1095, #1092 and #1079 entries are the same shape and longer. The
house one-line comment rule governs source comments, not the CHANGELOG. No finding.

---

## Recommendations

Priority order. Only H1 is a must-fix before the commit.

### High

1. **H1 — Restore `discover_genome`'s doc comment.** Move the `/// Discover the genome folder,
   validate the bisulfite indexes for `aligner` …` pair from `discovery.rs:134-135` back above
   `discover_genome` at line 149, leaving `absolute_genome_dir` with only its own doc. A `pub`
   function in the module's only public entry point should not ship undocumented, and the current
   block reads as if `absolute_genome_dir` validates indexes.

### Medium

2. **M1 — Fix `combined_index_basename`'s doc** (`discovery.rs:87-93`): "probed best-effort for
   every run" → "every bisulfite run", plus one line naming `config.rs:835-845`'s flag rejection as
   the guarantor for the `None` that `discover_genome_fasta_only` hard-codes. This is the field with
   ~20 readers and it is the one the Task 1.3 doc pass skipped.
3. **M2 — Replace the line-number citations with function names** in the `ct_index_basename` doc
   (`discovery.rs:78-79`). `mod.rs:943` already points at the wrong statement, invalidated by this
   diff's own `mod.rs` hunk.
4. **M3 — Drop the `reference:` line from the 5-Base summary** (`config.rs:1610-1616`), or render it
   as a file **count**. As written it names `fastas[0]` for a run that concatenates all N FASTAs
   (`five_base_reference_fasta`, `mod.rs:1584-1606`) — wrong for the common per-chromosome genome.
   Dropping it is my preference: `genome:` + `FASTA(s): N file(s)` already carry the information, D2
   is satisfied by *not printing* the fabricated basenames, and the diff shrinks.
5. **M6 — Correct plan §10's prepared-genome claim.** Only `five_base_umi_dedup_drops_duplicates`
   reaches discovery; the other two "retained" sites are rejected before it (`config.rs:816-822`
   and `resolve_aligner` at `config.rs:1278`). Optionally add one `--bowtie2` 5-Base end-to-end
   against `make_genome` to restore engine-route symmetry — low value, since nothing on the 5-Base
   path can read the two fields that differ, but it is ~30 lines if wanted.
6. **M7 — Add the `rust/README.md` Milestones bullet.** Established for comparable aligner bug
   fixes (`16f65f6`, `11efbab`, `ddc7633`); the plan's Task 6 dropped this half of I7.
7. **M5 — Optionally cover Task 5**, since the CHANGELOG makes a specific claim about it: a fake
   bowtie2 emitting one record per pair trips the desync arm and lets you assert the message names
   Bowtie 2.
8. **M4 — No action required**, but be aware the D4 change is entirely uncovered: the only
   `--five_base_consensus_from_bam` test returns at the mutual-exclusion guard.

### Low

9. **CHANGELOG wording:** "in place of the two converted-index basenames" — the code replaces
   **three** lines (`CT index:`, `GA index:`, `large index:`). One word.
10. **`make_genome_fasta_only` duplicates `make_genome`'s FASTA write** verbatim
    (`aligner_cli.rs:38` vs `:44`). Have `make_genome` call `make_genome_fasta_only(dir)` as its
    last statement, so the FASTA contents cannot drift between the two.
11. **`discover_fastas` can drop to private.** D4 removed its last caller outside `discovery.rs`
    (it is now used only at `:196` and `:224`), so the `pub(crate)` at `:241` is over-broad. No lint
    fires, which is why it is Low.
12. **`run_five_base_consensus_standalone`'s error prefixes are now inconsistent.** Its other errors
    all read `consensus: …` (`mod.rs:550`, `:557`); the genome error is the only one without it.
    Acceptable as-is — `GenomeFolder`'s text names the path and asks the right question, and
    `--genome` is unambiguous — but worth a deliberate decision rather than a side effect.
13. **Comment-style trims** (house rule: default one line, two max, state the fact, never justify
    the change in source):
    - `discovery.rs:138-139` — "a plain `?` on `canonicalize` compiles and downgrades it to
      `AlignerError::Io`" pre-empts an objection and describes an implementation that isn't there.
      The invariant clause alone carries the constraint: *"Both failure modes must stay
      `GenomeFolder`."* The reasoning belongs in the commit message. (The same sentence in the
      **test** doc at `discovery.rs:336-337` is fine — a test doc explaining what it guards is
      exactly where that belongs.)
    - `config.rs:624-626` — `discover_genome_for_run`'s doc is three lines; two is the maximum.
    - `config.rs:1606-1607` — "instead of two basenames that need not exist" justifies the change.
      *"A 5-Base run probes no converted index; report the reference it aligns against."*

Everything else in the new comments reads correctly to me — current-state, no historical narrative,
and the in-test markers (`// #1099: no BS_*.mmi — 5-Base must not need one`) state the current fact
rather than the state being fixed.
