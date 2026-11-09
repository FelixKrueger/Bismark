# CODE REVIEW A — #1099: `--illumina_5base` must not require the converted CT/GA indexes

**Reviewer:** A · **Date:** 2026-08-10 · **Branch:** `1099-five-base-genome-discovery` (uncommitted working tree)
**Reviewed:** `git diff` over `rust/bismark/src/aligner/{discovery,config,mod}.rs`,
`rust/bismark/tests/{aligner_cli,aligner_five_base_groundtruth}.rs`, `CHANGELOG.md`,
`docs/src/content/docs/rust/illumina-5-base.md`, plus `PLAN.md` (rev 1) and both plan reviews.
`SESSION_HANDOFF.md` ignored as instructed.

> **Override applied (as instructed):** three agents are reviewing the same uncommitted tree
> concurrently, so I applied **no fixes to any tracked file**. Everything is a recommendation; the
> only file I wrote is this report. Where I wanted to test a hypothesis I ran the existing debug
> binary against throwaway fixtures instead of editing source.

---

## Verdict

**APPROVE, with documentation fixes.** The fix is correct, minimal, and correctly scoped. Every
claim I was asked to doubt holds — and the two that mattered most (the exhaustive reader audit and
the `--combined_index` ordering) I re-derived from source and confirmed empirically rather than
taking them from the plan. All four defects I found are in **comments, doc strings and one summary
line**; none affects the faithful bisulfite path, and none can produce a wrong methylation call.

Suites re-run on this tree: `--lib aligner::` **537 passed / 0 failed**; `--test aligner_cli
five_base` **6 passed**; `--test aligner_five_base_groundtruth` **7 passed**.

| # | Area | Priority | One-liner |
|---|---|---|---|
| H1 | Structure / docs | **Medium-High** | `discover_genome`'s doc comment was orphaned onto `absolute_genome_dir`; the public fn is now undocumented |
| M1 | Logic (anti-drift) | **Medium** | The `mod.rs:943` guarantor citation is stale by exactly this commit's own line shift |
| M2 | Docs | **Medium** | `--genome`'s `--help` still says "prepared with bismark_genome_preparation" |
| M3 | Logic (D2) | **Medium** | `reference:` names one FASTA of N; the real reference on a multi-FASTA genome is the concat temp file |
| L1–L7 | mixed | Low | dead fallback, unanchored comment, house-style comment, inert test scaffolding, a hollowable assertion, `rust/README.md`, docs nit |

---

## Verification of the specific claims I was asked to judge

### 1. `absolute_genome_dir` — is `GenomeFolder` preserved on both arms, for both entry points? **Yes.**

`discovery.rs:140-147` returns `AlignerError::GenomeFolder(genome_arg.to_path_buf())` on the
`canonicalize` failure (via `map_err`, not `?`) **and** on `!is_dir`. Both entry points call it as
their first statement — `discover_genome` at `:150`, `discover_genome_fasta_only` at `:215` — so all
four combinations are covered, and the new
`genome_folder_errors_survive_the_shared_prologue` test pins exactly those four.

Confirmed at the binary level too (`--five_base_consensus_from_bam`, which is the second caller):

```
--genome <a file>        → error: failed to access genome folder "…/pUC19.fa" (does it exist and is it a directory?)
--genome <nonexistent>   → error: failed to access genome folder "…/nope" (does it exist and is it a directory?)
--genome <dir, no FASTA> → error: the specified genome folder "…/empty" does not contain any sequence files in FastA format …
```

No `Io` downgrade on any path. The risk §7 called out is genuinely closed.

### 2. Is the field-doc invariant actually true — no reader of the unvalidated basenames? **Yes, and it is stronger than the plan claims.**

I re-ran the audit from scratch (`grep -rn 'ct_index_basename\|ga_index_basename\|large_index' src/`,
then resolved each hit's enclosing function and its callers) rather than trusting §1's table:

| Reader | Enclosing function | Reachable when `five_base`? |
|---|---|---|
| `mod.rs:1208-1209` | `process_se_chunk` | No — SE only; 5-Base SE is `unreachable!` at `mod.rs:944` |
| `mod.rs:1342`, `:1347` | `build_se_inprocess_streams` | No — SE only, and `--rammap` is rejected for 5-Base |
| `mod.rs:4631-4632` | `process_pe_chunk` | No — `run_pe_five_base` (`mod.rs:1731`) calls neither `process_pe_chunk` nor anything in `parallel` (grepped its whole body: zero hits) |
| `parallel.rs:597` | `emit_memory_warning`, called **only** from `parallel.rs:660` / `:812` inside `run_se_multicore` / `run_pe_multicore` | No — those are reached only from `pipeline()`'s `n > 1` arms (`mod.rs:988`, `:1028`), which sit **below** the `five_base` return |
| `config.rs:1627-1629` | `RunConfig::summary()` | **Was the one live reader.** D2 branches it away |
| `large_index` | `config.rs:1629` only | same |

So the guarantor is confirmed: `pipeline()` returns `run_pe_five_base` before both the SE/PE dispatch
and the `--multicore` fan-out, which is why `aligner_cli.rs`'s `--multicore 4` 5-Base test passes
against a FASTA-only genome. Worth recording explicitly: **after D2 the fabricated basenames are not
merely unread on the alignment path, they are unobservable** — there is no longer any expression
anywhere that can reach them under `five_base`. That is a materially better answer to the issue's
"should not be silently fabricated" than "the readers are unreachable", and it is the reason
rejecting the `Option<PathBuf>` refactor is defensible.

`combined_index_basename: None` is also safe, but **not for the reason the new comment implies** —
see L2. I tested the sharpest case (`--bowtie2 --five_base_index`, which clears every guard inside
`reject_combined_index_unsupported`):

```
--illumina_5base --bowtie2 --five_base_index X --combined_index → error: --illumina_5base is not supported with --combined_index (a separate bisulfite alignment model).
--illumina_5base --combined_index                               → same
```

Both die at the `if cli.illumina_5base` scope block (`config.rs:835-845`), well before the presence
guard at `:866`. Claim holds.

### 3. `RunConfig::summary()` — correct output, no format-string bug, `\n\` indentation as intended? **Yes on all three.**

The `\<newline>` continuation strips the newline *and* the following line's leading whitespace, so
the source indentation is purely cosmetic on both arms and `{index_lines}\` correctly lets
`index_lines`' own trailing `\n` supply the break. The non-5-Base arm reproduces the previous three
lines verbatim — same order, same padding, same `\n` placement — so no faithful run's stderr changes.
Label padding is consistent with the 16-column gutter (`reference:` 10+6, `5-Base index:` 13+3,
matching `CT index:` 9+7). Live output:

```
genome:         /private/tmp/.../g
reference:      /private/tmp/.../g/pUC19.fa
5-Base index:   (none: minimap2 reads the FASTA)
FASTA(s):       1 file(s) (Fa)
```

and on the bowtie2 route `5-Base index:   …/normal_idx`. One real defect in this branch — M3, the
multi-FASTA case.

### 4. `mod.rs` D4 — does anything depend on the old `Validation` text or on the absence of `is_dir`? **No, and the change is a net improvement.**

`grep -rn 'consensus: --genome' src/ tests/` → zero hits, so nothing asserts the retired string.
Losing the `consensus:` prefix costs a little context (the user no longer learns *which* mode
complained), but `--genome` is unambiguous and the replacement message is self-describing. The gained
`is_dir` check is a genuine fix, not just consistency: `--genome <file>` previously fell through to
`discover_fastas` → `read_dir` on a file → a bare `AlignerError::Io`, and now produces the clear
`GenomeFolder` text (verified above). The third copy of the prologue is gone, which is what D4 asked
for.

Task 5 also verified live — the bowtie2 route now says `Bowtie 2 produced fewer PE records than read
pairs (desync)`, no longer hard-coding minimap2.

### 5. Did repointing weaken `five_base_pe_end_to_end_inverts_polarity` / `five_base_bowtie2_unconverted_index_end_to_end`? **No — strictly stronger.**

`make_genome_mmi` (`aligner_cli.rs:2662`) and `make_genome` (`:38`) write **the identical FASTA**
`>chr1\nACGTACGT\n` that `make_genome_fasta_only` writes, so the only behavioural delta is the absence
of the dummy index files. Every assertion survives, unmodified:

- minimap2 test: `"Bismark was run with minimap2 against"`, the exact option string
  `-a --MD --secondary=no -t 4 -x sr -K 250K`, `Sequence pairs analysed in total:\t1\n`,
  `Mapping efficiency:\t100.0% \n` (trailing space intact), 2 BAM records, R1 located by FLAG `0x40`,
  `XM == b".Z...z"`.
- bowtie2 test: `"Bismark was run with Bowtie 2 against"`, `-q --score-min L,0,-0.6`, the negative
  `!contains("--norc")`, 2 records, `XM == b".Z...z"`.

Both now additionally prove #1099 end to end on their respective engine routes, which is what §5d.9
wanted from two *new* tests — the documented deviation is the better trade. Same for the 7 groundtruth
call sites: `write_genome`/`write_genome_multi` lost only the `Bisulfite_Genome/**/BS_*.mmi` writes,
no `Bisulfite_Genome` reference remains in that file, and its 7 tests pass.

---

## Issues

### H1 (Medium-High) — `discover_genome`'s doc comment was orphaned onto `absolute_genome_dir`

`discovery.rs:134-139` is one contiguous `///` block, because the new helper was inserted *between*
the old doc comment and the item it documented. `absolute_genome_dir` therefore reads:

```rust
/// Discover the genome folder, validate the bisulfite indexes for `aligner`
/// (Bowtie 2 `.bt2` or HISAT2 `.ht2`), and inventory the raw FASTA file(s).
/// The genome folder as an absolute path (Perl `chdir`+`getcwd`).
///
/// Both failure modes must stay `GenomeFolder`; …
fn absolute_genome_dir(genome_arg: &Path) -> Result<PathBuf> {
```

and `pub fn discover_genome` (`:149`) now has **no doc comment at all**. The first two lines describe
a different function — one that validates indexes and inventories FASTAs, neither of which
`absolute_genome_dir` does.

This is a public-API regression, not just cosmetics: `discovery` is `pub mod` (`mod.rs:35`) inside
`pub mod aligner` (`lib.rs:14`), so `bismark::aligner::discovery::discover_genome` is externally
reachable and its rustdoc entry is now empty. It escapes CI because `missing_docs` is deliberately
**not** set for the aligner module — `lib.rs:8-12` records that the aligner/genome_prep/report crates
never had it — so neither `clippy -D warnings` nor the build can catch it.

**Recommendation:** move the two-line `/// Discover the genome folder, …` paragraph back to
immediately above `pub fn discover_genome`, leaving `absolute_genome_dir` with only its own
description.

### M1 (Medium) — the `mod.rs:943` anti-drift citation is already stale, by this commit's own shift

The field doc at `discovery.rs:78-79` claims:

> `pipeline()` routes `five_base` to `run_pe_five_base` (`mod.rs:943`) before any reader stats it
> (`parallel.rs:597`).

At `HEAD`, `mod.rs:943` was `if config.five_base {` — correct when the plan was written. The D4 edit
removed 3 net lines above it, so the guarantor now sits at **`mod.rs:940`**, and line 943 is the
`ReadLayout::SingleEnd { .. } =>` arm — the `unreachable!` branch, i.e. the opposite of the routing
statement the doc is asserting. The citation was invalidated by the very commit that introduced it.

This matters more than a normal stale line number, because this comment is the *entire* anti-drift
mechanism protecting two unvalidated fields — §7's mitigation for "a future refactor moves the
short-circuit below the fork dispatch" is a reader finding that line. `parallel.rs:597` happens to
still be right today, and will rot the same way.

**Recommendation:** anchor on names, which don't shift — e.g. "`pipeline()`'s `five_base`
short-circuit returns `run_pe_five_base` before the `--multicore` fan-out, so no reader
(`parallel::emit_memory_warning`, the SE/PE chunk spawners) stats it." Keep it to the one line the
house style allows.

### M2 (Medium) — `--genome`'s own `--help` still says "prepared with bismark_genome_preparation"

`cli.rs:30`:

```rust
/// Genome folder (prepared with bismark_genome_preparation). May also be
/// given as the first positional argument.
```

The docs page got the corrective paragraph, but this is the string a 5-Base user actually reads at
the terminal, and it states the exact belief #1099 is about. The plan's §4 Task 6 argued the docs page
"already shows `--genome` with no prep step" was independent corroboration that the rejection was a
bug — yet the reporter inferred the opposite anyway. This help text is the most plausible reason why,
and it survives the fix untouched.

**Recommendation:** one clause, e.g. `Genome folder (prepared with bismark_genome_preparation;
--illumina_5base needs only the FASTA)`. Same spirit and cost as the docs sentence already added.

### M3 (Medium) — `reference:` is wrong on a multi-FASTA genome

Verified against a genome holding two `.fa` files:

```
genome:         /private/tmp/.../gmulti
reference:      /private/tmp/.../gmulti/a_puc19.fa
5-Base index:   (none: minimap2 reads the FASTA)
FASTA(s):       2 file(s) (Fa)
```

minimap2 does **not** receive `a_puc19.fa`. `five_base_reference_fasta` (`mod.rs:1584-1606`) passes a
single-FASTA genome through directly but concatenates a multi-FASTA genome into
`<output_dir>/.bismark_5base_concat_ref.fa` and passes *that*. So `reference:` names one of N files
and the actual reference goes unmentioned. `FASTA(s): 2 file(s)` on the next line is the only hint,
and a reader who trusts a label called `reference:` will not go looking for it.

D2 exists precisely so `summary()` stops asserting a path that isn't the one in play; this is the same
defect class in miniature, introduced by the fix for it. Not a correctness bug — nothing reads the
summary — but it undercuts the change's own rationale.

**Recommendation** (pick one, all ~2 lines):
- `reference:` = `genome_dir` plus a count when `fastas.len() > 1`, e.g.
  `…/gmulti (2 FASTA files, concatenated)`;
- print all of `fastas` joined, matching how `reads:` already handles multiple inputs; or
- drop `reference:` entirely — `genome:` and `FASTA(s):` are already truthful and, in the
  single-FASTA case, `reference:` adds only the file name.

### L1 (Low) — the `"-"` fallback in the `reference:` line is unreachable

`config.rs:1616`. `discover_fastas` returns `NoFasta` for a genome with no FASTA, so any `RunConfig`
that reaches `summary()` has at least one; the only empty-`fastas` construction is `run_config_stub`
(`config.rs:1702`), which sets `five_base: false` and never calls `summary()`. Harmless, but a `-` in
a summary reads as an achievable state. If M3 is taken this disappears anyway.

### L2 (Low) — `config.rs:865`'s "rejected above" points the reader at the wrong guard

The comment is **correct** (I verified it empirically above) but unanchored, and the line directly
above it names `reject_combined_index_unsupported` — which contains **no** `illumina_5base` /
`five_base` check whatsoever. A reader checking the claim goes there first, finds nothing, and has to
work backwards. The real guard is the `if cli.illumina_5base` scope block at `config.rs:835-845`.
§3's own reasoning ("it is fragile at 30 lines apart") is the argument for naming it.

**Recommendation:** "…rejected by the `--illumina_5base` scope guards above, so its unprobed `None`
is safe." Name the guard, not a line number (see M1).

### L3 (Low) — `absolute_genome_dir`'s second doc paragraph is the "justify the change in the source" pattern

```rust
/// Both failure modes must stay `GenomeFolder`; a plain `?` on `canonicalize` compiles
/// and downgrades it to `AlignerError::Io`.
```

The first clause is a legitimate invariant sitting on the thing it constrains. The second is the trap
narrative — an argument aimed at a reviewer wondering "but wouldn't `?` be simpler?", which the house
style puts in the commit message. It is also already stated twice more: in the test name
`genome_folder_errors_survive_the_shared_prologue` and verbatim in that test's own comment
(`discovery.rs:336-337`). **Recommendation:** keep the first clause, drop the second; the commit
message and the test carry it.

### L4 (Low) — two of the three retained `make_genome*` 5-Base sites are inert

`five_base_rejects_non_directional` (`:6234`, `make_genome_mmi`) fails at `config.rs:818` and
`five_base_bowtie2_requires_index` (`:6327`, `make_genome`) fails inside `resolve_aligner`
(`config.rs:1278`) — both **before** discovery at `config.rs:862`, so neither ever stats an index
file. §10's "`make_genome_mmi` deliberately retained … so 5-Base-on-a-*prepared*-genome stays
covered" therefore rests on exactly one test, `five_base_umi_dedup_drops_duplicates` (`:6353`), which
does run end to end.

One test is adequate cover for "a prepared genome still works", so this is not a gap — but it is worth
knowing it is one test rather than three, and the two inert sites are candidates for the same
scaffolding cleanup the rest of the change performs.

### L5 (Low) — `five_base_resolve_does_not_require_the_converted_index` can be hollowed out silently

It asserts only `!matches!(e, FaultyIndex { .. })`. If a future guard ever returns before
`config.rs:862`, the test passes without exercising discovery at all — vacuously green, no signal.
The install-agnostic shape is right (and matches the module's `five_base_duplex_guards` idiom); the
gap is that nothing pins *where* the error came from. §10's sabotage record establishes non-vacuity
today, not tomorrow.

**Recommendation:** narrow to an allow-list of expected variants (`AlignerNotWorking | …`) rather than
a single exclusion, or assert `Ok` on the arm where the aligner is known present. Low, because the
companion over-reach test (`faithful_resolve_still_requires_the_converted_index`) would still catch a
genuine regression from the other direction.

### L6 (Low) — `rust/README.md` untouched

`rust/README.md` is the canonical status journal, and the plan's own cited precedent (#1095 →
`6a3ea0d`) touched it alongside the docs page and the CHANGELOG. Nothing in it is now *wrong* — I
checked the aligner row and the 5-Base Milestones entries, and neither claims 5-Base needs prepared
indexes — so a bug fix arguably doesn't warrant a line. Flagging it so the omission is a decision.

### L7 (Low) — the docs paragraph can be misread on the bowtie2/hisat2 route

"There is no `bismark_genome_preparation` step for 5-Base" is accurate, but a bowtie2-route user
still needs a plain index built with `bowtie2-build` for `--five_base_index`. The sentence immediately
above says so, so this is a nit; half a clause would close it.

---

## Efficiency

Nothing to raise. `discover_genome_fasta_only` does one `canonicalize`, two `join` chains and one
`discover_fastas`, and **skips** the 12–16 `is_file` stats the bisulfite path performs — it is
strictly cheaper than what it replaces. `discover_fastas` is reused verbatim, so the `@SQ`-ordering
contract with `bismark-genome-preparation` is untouched (correct — a duplicate would have been a
byte-identity landmine) and its known up-to-four `read_dir` scans are correctly left alone on a
bug-fix branch. The two `PathBuf`s built and discarded on the consensus path (which takes only
`.fastas`) are three allocations per process — not worth a change.

## Errors / security

None. No new `unwrap`/`expect`/`panic!` on a reachable path; no new I/O; no user input reaching a
command line; the two new `PathBuf`s are `join`s of a canonicalized directory with literals. The
blast-radius bound §7 states holds: an over-reach would cost availability (a bisulfite run dying at
aligner spawn on a missing `-x`), never a silent miscall, and `pipeline()`'s unconditional return
makes the silent-miscall path unreachable.

## Byte-identity

Safe. The branch is gated on `cli.illumina_5base`, which no `perl-oracle` cell sets; the faithful
`discover_genome` is behaviourally unchanged (only its prologue was extracted, and I verified both of
its error arms still produce `GenomeFolder`); and the non-5-Base arm of `summary()` emits the same
three lines in the same order — and `summary()` is stderr, not gated, in any case.

## CHANGELOG and docs accuracy

Both check out. The quoted error text matches the binary exactly (`the Bowtie 2 index of the
C->T->converted genome seems to be faulty or non-existant ('BS_CT.1.bt2l')` — reproduced verbatim in
my negative-control run, double arrow included). The `BS_CT.mmi` claim is correct:
`index_suffixes(Minimap2, "BS_CT", _)` is `["BS_CT.mmi"]` (`discovery.rs:122`), and
`discovery.rs:104-107` confirms genome-prep writes that only under `--minimap2`, so §2's row 2 — the
larger case — is real. The entry's length matches its neighbours in that file. The docs paragraph is
accurate modulo L7.

## Informational (out of scope, pre-existing — not part of this change)

In my run, `summary()` reported `aligner_options: … -t 2 …` while the minimap2 process actually
received `-t 10`. The summary's option string and the child's argv disagree on `-t`. I did not chase
it and it is unrelated to this diff; I note it only because it sits inside the same `summary()`
truthfulness question D2 raised, and because M3's fix will put someone in this function.

---

## Recommendations, in the order I would apply them

1. **H1** — reattach `discover_genome`'s doc comment (`discovery.rs:134-139`). Public-API doc
   regression; nothing in CI will catch it.
2. **M1** — replace the `mod.rs:943` / `parallel.rs:597` line numbers with function names
   (`discovery.rs:78-79`). It is already wrong.
3. **M2** — one clause on `--genome`'s help text (`cli.rs:30`). Highest user-visible value per
   character in this whole change.
4. **M3** — make the 5-Base `reference:` line truthful for a multi-FASTA genome (`config.rs:1610-1621`).
5. **L2, L3** — anchor the `config.rs:865` comment on the guard's name; trim `absolute_genome_dir`'s
   second doc paragraph.
6. **L4, L5, L6, L7** — optional; L6 is a decision to record rather than a change to make.

None of these block the merge on correctness grounds. H1 through M3 are what I would want fixed
before the commit lands, since all four are one- or two-line edits in files the commit already
touches.
