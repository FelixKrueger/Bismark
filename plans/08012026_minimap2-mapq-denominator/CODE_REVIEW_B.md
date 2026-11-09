# CODE REVIEW B — #1081 minimap2/rammap MAPQ denominator

**Reviewer:** B (independent; no coordination with Reviewer A)
**Target:** branch `plan/mapq-minimap2-denominator`, uncommitted working tree on `dev` `172da96`
**Plan:** `plans/08012026_minimap2-mapq-denominator/PLAN.md` (rev 2) + `SPIKE.md`
**Date:** 2026-08-01

---

## Verdict

**Ship after the HIGH-2 wording correction.** The production change is correct, minimal and
well gated: I re-derived every value independently and found no arithmetic or wiring defect,
and I could not construct a seventh silent-fault vector in the production diff — the surface is
genuinely one `match` arm feeding one construction site, and the construction matrix + four BAM
cells + the reference sweep cover it from both ends.

Two problems that must not ship as-is:

1. **HIGH-1 — the new rammap BAM test breaks CI** (`rammap-inprocess` job). Reproduced, root-caused,
   **fixed in place** (one flag + comment), re-verified under both feature configurations.
2. **HIGH-2 — the user-facing "floor is 22 / `-q 20` is still close to a no-op" claim is false**, in the
   CHANGELOG *and* the docs page. It is the same defect class as #1080's HIGH-1 (the floor is what a
   `-q` filter sees), one branch over — and it contradicts the "11" the same CHANGELOG bullet quotes
   two sentences earlier. Not fixed by me: the sentence is mandated verbatim by PLAN §7 step 9, so the
   correction is the author's call, not a reviewer's. Proposed wording below.

Everything the plan claims is unaffected, I verified independently and it holds (§"Verified" below).

---

## HIGH-1 — the new rammap BAM test fails in the `rammap-inprocess` CI job **[FIXED]**

`rust/bismark/tests/aligner_cli.rs` — `rammap_mapq_uses_the_perfect_score_denominator_end_to_end`
invoked `--rammap --path_to_rammap <fake>` **without `--rammap_subprocess`**. Since 3.1.0, `--rammap`
**defaults to the in-process backend** on a `--features rammap-inprocess` build, which loads the real
`.mmi`; `make_genome_mmi` (`:2649-2657`) writes a 1-byte `x` placeholder. Reproduced:

```
$ cargo test -p bismark --features rammap-inprocess --test aligner_cli rammap_mapq_uses
error: failed to load rammap index …/BS_CT.mmi: Empty index file
code=1
test rammap_mapq_uses_the_perfect_score_denominator_end_to_end ... FAILED
```

`.github/workflows/rust_ci.yml:101` runs exactly `cargo test -p bismark --features rammap-inprocess`,
so this is a red CI on the PR, not a hypothetical. The trap is **already documented in the same file**
for the sibling test (`aligner_cli.rs:3061-3067`, `rammap_se_mapped_names_report_and_notice`:
*"`--rammap` now DEFAULTS to the in-process backend on a `--features rammap-inprocess` build, so we
pin the subprocess path explicitly … backend-stable on BOTH builds"*), and the precedent was not
followed. The `--path_to_rammap` docs (`alignment.md:349`) also describe it as
*"used by `--rammap_subprocess`"*, so the flagless form was doubly off-pattern.

**Fix applied** (`aligner_cli.rs`, in the new test only):

```rust
        .arg("--rammap")
        // The score model is resolved from `Aligner::Rammap`, so either backend proves the
        // arm — but `--rammap` DEFAULTS to in-process on a `--features rammap-inprocess`
        // build, which would load the fake 1-byte `.mmi` and fail. Pin the subprocess.
        .arg("--rammap_subprocess")
```

**Re-verified:** feature ON `ok`, feature OFF `ok`, `cargo fmt -p bismark -- --check` clean.

**V7's intent survives the fix.** The `ScoreModel` is built at the single production site
`config.rs:840` from `(score_min_params, cli.local, aligner)` — before and independent of any backend
selection — so a subprocess run still proves the arm covers `Aligner::Rammap`, and fault injection (v)
(`drop | Aligner::Rammap`) still lands on this cell and only this cell.

---

## HIGH-2 — "the floor is 22, so `-q 20` is still close to a no-op" is false (CHANGELOG + docs)

The floor of **22** belongs to the *no-second-best* branch only. With a cross-instance runner-up the
local ladder's sub-rungs go far lower, and this change makes several of them newly reachable.
Enumerated over `len = 100`, `AS ∈ 1..200`, `AS₂ < AS` (plus the no-runner-up case), against the two
ladders exactly as transcribed in `mapq.rs`:

| | reachable MAPQ values |
|---|---|
| pre-#1081 | 6, 12, 17, 18, 21, 22, 25, 26, 27, 33, 42 |
| **post-#1081** | **2, 9, 11, 12, 14, 16, 17, 18, 19, 21**, 22, 24, 25, 28, 31–42, 44 |

Ten post-fix values are below 22 and nine are below 20. Concretely: `AS 80, AS₂ 79` at 100 bp
returns **2** (was 6) — reachable because `best_over < 0.5·diff` only became possible once `diff`
grew. Over that grid, cells landing under MAPQ 20 go from **1372/19900 to 13507/19900**. Grid density
is not read density, so I make no claim about what fraction of a real BAM moves — but the direction is
unambiguous, and it is the opposite of what the release notes say.

Two false statements, both user-facing:

- `docs/src/content/docs/options/alignment.md:314` — *"MAPQ therefore ranges from 22 (a read scoring
  well below its best possible score) to 44."* Simply false; 2–21 are reachable.
- `CHANGELOG.md:14` — *"**the floor is 22, not 0**, so a `-q 20`-style filter is still close to a
  no-op — this fixes MAPQ's *discrimination*, not its usefulness as a hard filter."* False, and
  **self-contradicting**: the same bullet quotes the near-tie dropping to **11** two sentences
  earlier. A methylseq user who reads "still close to a no-op" and then loses reads at `-q 20` has
  been actively misled — and the good news (filterability *is* partly restored for the runner-up
  branch) is being denied rather than claimed.

**Root cause is in the plan, not the implementation.** PLAN §3.4 says *"The local ladder's floor is
22"* unscoped, and §7 step 9 mandates the sentence; the implementation shipped it faithfully. That is
why I did not edit it — but it must be corrected before release. Suggested minimal wording:

> …**a uniquely-aligned read cannot fall below 22** (the end-to-end ladder would have floored it at 0),
> so for those reads a `-q 20`-style filter stays close to a no-op. Reads whose runner-up comes from
> another strand instance are scored on the ladder's sub-rungs and **can now fall as low as 2**, so
> `-q`-filtered read counts do change for that subset.

and for the docs page: *"MAPQ is 22–44 for a uniquely-aligned read, and can be lower (down to 2) when
a second strand instance produced a competing score."*

Related, and I would **not** rename them, only add one scoping clause each:

- `mapq.rs` test `minimap_like_floor_is_twentytwo_not_zero` — true for the branch it tests; its doc
  should say *"the floor of the no-second-best branch"*.
- `config.rs:205-206` (`local_ladder()` doc) — *"its floor is 0 rather than 22"* is a correct
  ladder-vs-ladder comparison for the unique case; add *"(unique reads; the runner-up sub-rungs go
  lower in both ladders)"*.

---

## MEDIUM-1 — "MAPQ was 42 for **every** uniquely-aligned minimap2 read" is categorically false

Pre-fix, reads with a cross-instance runner-up scored 6/12/17/18/21/22/25/26/27/33 — and in Bismark's
own accounting those reads **are** uniquely-aligned (`merge.rs:366` increments
`unique_best_alignment_count` on exactly that path). The claim appears in `CHANGELOG.md:14` (bolded,
as the headline), `rust/README.md:181`, and in substance in the new docs block. The plan foresaw this
phrasing risk (§7 step 9: *"phrased as that condition, **not** as 'reads that map to both strands'"*)
and the fix landed for the *runner-up* sentence but not for the headline.

Fix: *"MAPQ was 42 for every uniquely-aligned read with no competing alignment from another strand
instance — the large majority."*

## MEDIUM-2 — `rust/README.md:250` still makes the corrected claims, uncorrected

The trailing `✅` summary block of `## Milestones` (undated, present tense) reads:

> ✅ **`bismark` aligner v1.x COMPLETE — HISAT2 (SE+PE) + minimap2 (SE)** … **Both backends
> byte-identical to Perl v0.25.1** driving the same pinned aligner; the Phase-5 combined 10M gate
> confirmed all 13 cells …

Both statements are the ones the PR *did* correct 89 lines earlier in the row at `:161` (which now says
*"minimap2 SE **was** byte-identical … up to and including 3.1.0; since #1081 it deliberately is not"*
and tags the 13-cell gate *"a **pre-#1081** record"*). V13's gate (`grep -c "byte-identical to Perl
v0.25.1 + minimap2" rust/README.md`) cannot match this phrasing, so the gate could not have caught it —
the same lesson as PLAN §11 risk 7 ("this list is a starting point, not an inventory"). Line `:230` is a
**dated** 2026-06-06 log entry and is defensible as history, though a `(pre-#1081)` tag would match the
plan's own instruction for the 13-cell record.

## MEDIUM-3 — a third stale doc claim the sweep missed

`docs/src/content/docs/usage/alignment.md:65` lists the BAM fields as
*"`MAPQ` (calculated for Bowtie 2 and HISAT2)"*. Bismark computes MAPQ for minimap2/rammap too — it
always did, the value was just degenerate — so this is pre-existing, but it is exactly the drive-by
class the plan chose to fix at `options.rs:81`, and it is now the only place in the docs that denies
minimap2 has a computed MAPQ.

## LOW-1 — `config.rs` `normalize()` points at documentation that does not exist there

The new paragraph (`config.rs:247-250`) ends *"documented in the `--score_min` help"*. The CLI help
(`cli.rs:285`) is one line: *"Min-score function, e.g. `L,0,-0.2` (Bowtie 2 --score-min)."* — it says
nothing about minimap2. The behaviour **is** documented, in `options::score_min_params`' rustdoc
(`options.rs:375-377`) and in `alignment.md:316`. Either retarget the cross-reference or add the note
to the clap help (the latter is the better fix — `--score_min` silently moving minimap2 MAPQ without
moving an alignment is a footgun a `--help` reader should see).

## LOW-2 — the V15 gate's option string is not "verbatim", and the reordering is silent

`tests/aligner_minimap2_as_bound.rs:31-32` documents `BISMARK_OPTS` as *"Bismark's minimap2 option
string, verbatim, minus the varied `-x <preset>`"*, then builds
`-a --MD --secondary=no -t 2 -K 250K -x <preset>` — i.e. it appends `-x` **last**, whereas Bismark emits
`… -t 2 -x map-ont -K 250K` with `-x` in the middle (`report` echo, `aligner_cli.rs:3101`). With
minimap2 the order matters in principle: a preset overrides options that precede it, and `-x sr`
carries `-2K50m`, so in the test's order the preset silently wins the `-K` the gate thinks it passed.

I measured it and it does **not** affect what the gate asserts — the `AS:i:` values for all 16 fixture
reads under `-x sr` are byte-identical in both orders (`-K` is the mini-batch size, not a scoring
knob). So this is not a correctness problem, but the doc claim is false and the next person to add a
preset-sensitive assertion here would be misled. One-line fix: emit `-x <preset>` before `-K 250K` and
the string really is verbatim.

## LOW-3 — comment volume vs the project's "one line, state the fact" rule

New comment lines are **32%** of added lines; the surrounding modules run 16–25%
(`mapq.rs` 24%, `config.rs` 25%, `merge.rs` 16%, `options.rs` 22%). Longest new blocks are 12 lines.
Most of the volume is plan-mandated (the hand-derivations in the unit tests, the three load-bearing
fixture constraints in `make_fake_minimap2_two_instance`) and I would keep it — a reader editing those
fixtures needs it. Two carry *evidence and history* rather than facts, and belong in the commit message:

- `config.rs:172` (`end_to_end()` doc) — *"The name is kept for its ~47 call sites"*. A count that goes
  stale on the next refactor; the sentence works without it.
- `config.rs:198-209` (`local_ladder()` doc) — 12 lines re-arguing the rejected alternative
  (second predicate, floor 0 vs 22, `-q 1`). Three lines plus the `#1081` link carries the same weight;
  the argument itself lives in PLAN §3.4, which is where a reader should find it.

---

## The seventh fault: I could not find one in the production diff

I went looking for a fault that ships green through the whole suite (the highest-value finding
available) and came back with the CI break above and the prose defects, not a silent behavioural one.
What I ruled out, and why the suite catches each:

| Candidate fault | Caught by |
|---|---|
| minimap arm gated `if !local` | construction matrix loops `local ∈ {false,true}` |
| arm widened to `Bowtie2 \| Minimap2 \| Rammap` | construction matrix (Bowtie 2 e2e) + V11 |
| `local_ladder()` → `>= 0.0` | construction matrix + `end_to_end_matches_the_pre_fix_formula` |
| `diff` clamp `max(1.0)` → `abs(...)` | V14's `assert_eq!(diff, 1.0)` for `L,10,-0.2`, `len ≤ 4` |
| `perfect()` drops mate 2 | `local_bowtie2_matches_independent_reference`'s PE cells (`mapq.rs:529-542`) — the reference sums independently |
| arm uses `BOWTIE2_LOCAL_MATCH_BONUS` (equal value) | fault injection (iii) proves the production arm reads `MINIMAP2_MATCH_BONUS` |
| `minimap_like()` built with the wrong form | construction matrix `== from_emitted(…, Linear, …)`, and BAM cell (b) 41 vs 36 |
| a fifth aligner inheriting `_ => 0.0` | construction matrix classifies all four explicitly |

The one structural reason the surface is this small: there is exactly **one** production `ScoreModel`
construction site (`config.rs:840`) — I grepped for every `ScoreModel::` in `src/` and all other
occurrences are in `mod tests`. So there is no second path that could carry a stale model.

**The plan's V4 trap #2 is closed better than the plan proposed.** PLAN §7 step 5 asked for either an
`assert!(match_bonus > 0.0)` inside the reference or ladder derivation inside it, because a caller
passing `0.0` would compare an end-to-end model against the local ladder. The implementation did
neither and instead removed the parameter: `positive_bonus_reference` takes no `match_bonus` at all
(`mapq.rs:451-483`), so no caller *can* pass 0.0. That is a structural close, not a comment — the
deviation is an improvement and is worth recording as one (it is not in §12's deviation list).

---

## The V15 real-aligner gate: sound, non-vacuous, and I could not make it flaky

I reproduced the test's data generation (the fixed LCG, offset 5000, the four mutation shapes) and ran
its exact option string against minimap2 2.31-r1302 for all three presets:

| preset | primary hits | perfect cells | `AS > 2·len` | perfect reads with `AS == 2·len` |
|---|---|---|---|---|
| `map-ont` | 16/16 | 4 | 0 | 4/4 |
| `map-pb` | 16/16 | 4 | 0 | 4/4 |
| `sr` | 16/16 | 4 | 0 | 4/4 |

- **Non-vacuous:** 12 perfect cells vs the `perfect_cells >= 6` guard, and `assert!(!hits.is_empty())`
  per preset catches a preset that silently drops everything.
- **Deterministic:** fixed LCG reference, no sampling, pure DP scores; `-t 2` affects output order only
  and nothing here depends on order.
- **Cannot panic on odd SAM:** unmapped/secondary/supplementary are filtered by `flag & 0x904` before
  `f[11..]`, and slicing a `Vec` at `index == len` is legal, so a tag-less mapped line yields an empty
  tag slice rather than a panic. Hard-clipped SEQ (which would under-state `read_len`) only occurs on
  supplementary records, which are filtered.
- **Version robustness:** the equality half depends only on minimap2's `a = 2`, which is precisely the
  premise the gate exists to police — a distro build whose reachable presets changed the match score
  (e.g. a `map-pb` aliased onto `map-hifi`'s `-A1`) would fail loudly, which is a true positive, not
  flakiness. I found no input in the fixture set where a version difference could change the *perfect*
  score without invalidating `MINIMAP2_MATCH_BONUS`.
- **CI actually runs it, and the `$CI` panic cannot misfire:** all three jobs that run these tests
  install minimap2 first (`rust_ci.yml:35-42`, `:93-100`, `:131-138`), and the `perl-oracle` job runs
  `cargo test -p bismark -- --exact <13 names>`, so `minimap2_reports_at_most_two_per_base` never
  executes there and `have_minimap2()`'s panic is unreachable in that job. Its `eprintln!("skipping: …")`
  also cannot trip that job's `grep -q '^skipping:'` guard for the same reason.

One honest limitation, already stated in the code: `config.rs`'s
`minimap_like_perfect_score_bounds_observed_alignment_scores` is arithmetic over a hardcoded table and
cannot fail unless someone edits the table — its doc says exactly that and points at V15. Correct
framing; no change needed.

---

## Verified independently (plan claims I did not take on trust)

**Arithmetic.** I re-implemented both ladders from `mapq.rs:54-227` in Python and re-derived, from
scratch: all 10 cells of PLAN §3.3 table 1 (new **and** old), all 10 of table 2, the four BAM cells
(a) 44 / (b) 41 / (c) 22 and (d) **11** (old 27), the four `--score_min` rows
(44/44/41/28/22 · 44/44/42/41/28 · 44/44/44/44/42 · 44/44/44/44/44), and the `L,10,-0.2` clamp cells
(`diff == 1`, `best_over < 0`, MAPQ 22 for `len ≤ 4`, clamp inert from `len 5`). **Every value in the
implementation matches**, including the pre-fix controls. Cell (b)'s five fault-discrimination values
also reproduce exactly: correct 41 · Log form 36 · bonus 0.0 → 42 · bonus 1.0 → 44 · end-to-end ladder
→ 24.

**Reachability / scope.**

- `calc_mapq` has **four** call sites: `merge.rs:367` (the only reachable one), `merge.rs:740` (PE —
  rejected for both aligners at `config.rs:577`, with its own guard test at `:1747`), `combined.rs:339`
  and `:711` (combined — rejected at `config.rs:1088`). `--local` rejected at `config.rs:721`.
- `merge.rs:367` passes `sequence.len()`, the **FastQ** read length, so clipping cannot reach it.
- **5-Base is unaffected** — traced the path rather than trusting the grep: `run_pe_five_base` →
  `five_base_align_and_call_pe` → `five_base_emit_pe_record` never touches `drive_merge*` or
  `config.score_model` (the six `score_model` uses in `mod.rs` are all inside `drive_merge`,
  `drive_merge_combined`, `drive_merge_pe`, `drive_merge_combined_pe`, `select_and_route_se_nondir`,
  `select_and_route_pe_nondir`). MAPQ is `rec.mapq` / `rec1.mapq.min(rec2.mapq)`, and
  `--five_base_min_mapq` filters that same passed-through column (`mod.rs:2060`).
- **`--ambig_bam` is unaffected** — `output.rs:819` parses MAPQ from field 4 of the raw aligner line and
  `:833` writes it verbatim. The plan's consequence (a `--minimap2 --ambig_bam` run emits two different
  MAPQ scales in two files) is real and correctly stated.
- **No downstream consumer** — zero `mapq`/`mapping_quality` references in
  `src/{extractor,dedup,bedgraph,coverage2cytosine,filter_nonconversion,bam2nuc,report,summary,meta,io}`.
- **`--multicore` is worker-invariant** — the model is resolved once from the CLI into `RunConfig`
  before any fork, and MAPQ is a pure per-read function of `(len, AS, AS₂, model)`.
- `--unmapped`/`--ambiguous` write FastQ/FastA; no MAPQ column exists there.

**Gates, re-run by me after my fix:**

| Gate | Result |
|---|---|
| `cargo fmt -p bismark -- --check` | clean |
| `cargo clippy -p bismark --all-targets` | **0 warnings, 0 errors** |
| `cargo test -p bismark --lib` | **1445 passed / 0 failed** |
| `cargo test -p bismark --test aligner_cli` | **115 passed / 0 failed** (incl. all four new BAM cells and the two HISAT2-local ladder gates V11 names) |
| `cargo test -p bismark --test aligner_minimap2_as_bound` | **1 passed** against real minimap2 2.31-r1302 |
| `--features rammap-inprocess`, the new rammap cell | passes after the HIGH-1 fix (failed before) |

PLAN V13's stale-comment grep returns **0**. The `CHANGELOG.md` placeholder bullet is gone, exactly one
#1081 bullet exists, and the neighbouring #1079 bullet is correctly amended to *"Bowtie 2 and HISAT2
end-to-end paths"*.

**A note on the test run, not a finding.** A full `cargo test -p bismark --features rammap-inprocess`
in the shared `rust/target` showed one unrelated failure,
`rammap_fasta_default_fires_note_subprocess_suppresses`. It passes in isolation, and the failure
signature (banner said *"subprocess rammap binary"*, FastQ-only note absent — both driven by the
`#[cfg(feature = "rammap-inprocess")]` block at `mod.rs:241`) means the `target/debug/bismark` the test
shelled out to had been rebuilt **feature-off** mid-run. Several review agents are running in this
working tree concurrently and share `rust/target`; that is the cause. Not attributable to this branch.

---

## Fixes applied by me

| File | Change | Verification |
|---|---|---|
| `rust/bismark/tests/aligner_cli.rs` (`rammap_mapq_uses_the_perfect_score_denominator_end_to_end`) | added `--rammap_subprocess` + a 3-line comment | `cargo test -p bismark --features rammap-inprocess --test aligner_cli rammap_mapq_uses` → **ok**; same without the feature → **ok**; `cargo fmt -p bismark -- --check` clean |

Nothing else touched. `PLAN.md` / `SPIKE.md` / `PROGRESS.md` untouched.

---

## Recommended actions, in order

1. **HIGH-2** — correct the floor/`-q 20` claim in `CHANGELOG.md:14` and
   `docs/.../options/alignment.md:314`; scope the two code comments. *(Requires an author decision
   because PLAN §7 step 9 mandates the current sentence; PLAN §3.4's unscoped "floor is 22" should be
   corrected in the same pass so the next reader does not re-derive it.)*
2. **MEDIUM-1** — qualify "42 for every uniquely-aligned read" in `CHANGELOG.md:14` and `README.md:181`.
3. **MEDIUM-2** — restate `rust/README.md:250` the way `:161` was restated; optionally tag `:230`.
4. **MEDIUM-3** — `docs/.../usage/alignment.md:65` field list.
5. **LOW-1** — retarget (or, better, satisfy) the `--score_min` help cross-reference.
6. **LOW-2** — put `-x <preset>` before `-K 250K` in `BISMARK_OPTS` so "verbatim" is true.
7. **LOW-3** — trim the two evidence-carrying comments.
8. Record the `positive_bonus_reference` parameter removal in §12's deviation list — it is the
   structural close of a trap the plan only asked to be commented.
