# PLAN_REVIEW_B — Sync methylseq's vendored `bismark/*` modules onto the merged `-p` threading model

**Reviewer:** B (independent)
**Plan reviewed:** `plans/08172026_methylseq-bismark-align-sync/PLAN.md`
**Date:** 2026-08-17
**Verdict:** **REQUEST CHANGES**

The mechanics are sound and unusually well-researched — import-by-sha instead of `nf-core modules update`,
the explicit `git rm` of the patch (correctly noting `git checkout <sha> -- <path>` never deletes), the
`cmp`-against-clean-checkout faithfulness proof, the confined-diff rule, and the positive control on
`rastair/mbiasparser`'s surviving `patch` key. The combined-index no-op claim — the plan's central
technical argument — I verified independently and it holds exactly.

But the plan's **central prediction about snapshot movement is falsified by upstream's own green CI at
the same thread count**, and the gate that authorises overwriting snapshots (V5) is consequently pointed
the wrong way. Combined with a file-scoped `--update-snapshot` in step 8 that would sweep three
must-not-move Bowtie 2 scenarios, there is a live path for a genuine Bowtie 2 regression to be absorbed
as a routine re-baseline. That is the failure mode the plan is otherwise carefully designed to prevent,
so it needs fixing before implementation.

---

## 1. Verification log — what I checked independently

Everything below was verified live against `nf-core/methylseq@9855ef94` and `nf-core/modules@c565f1b2`
via the REST API, and against the local Bismark tree at `/Users/fkrueger/Github/Bismark`.

### 1.1 Confirmed correct

| Plan claim | Result |
|---|---|
| methylseq `dev` HEAD `9855ef94` (2026-08-11, template merge 4.0.3) | ✓ `9855ef9444cfb0111517b6f5f56bb4a0cd78ed7a`, "Merge pull request #620 from nf-core/nf-core-template-merge-4.0.3" |
| #12385 merged as `1976cfb7f315a29658046728e071d9f60f3e1fe9` | ✓ `merged: true`, `merged_at: 2026-07-29` |
| #12581 merged as `c565f1b27c06a1e8b726e2a646d4c822ae3b514a` | ✓ `merged: true`, `merged_at: 2026-08-15` |
| All 7 `bismark/*` modules pinned at `7d264419ee98edd5a0886d3782fb87553628272b` | ✓ all seven, `branch: master` |
| `c565f1b2` is latest upstream for all 7 paths | ✓ per-path commit listing |
| `align` gained `1976cfb7` **and** `c565f1b2`; other six gained `c565f1b2` only | ✓ |
| Only `bismark/align` carries a `patch` key; `rastair/mbiasparser` carries the other | ✓ (42 nf-core modules total, exactly 2 patched) |
| Assumption 7: other six differ only in `meta.yml` | ✓ **verified at blob level** — for all six, `main.nf`, `environment.yml`, `tests/main.nf.test`, `tests/main.nf.test.snap` and both `.conda-lock` blobs are byte-identical across the two shas; only `meta.yml` changed |
| `align`: `main.nf` `ff7af322`→`848dbab1`, `meta.yml` `1bd9fcd5`→`82bb0986`, `tests/main.nf.test` `b2fe71b6`→`17787b38`, `tests/main.nf.test.snap` `b416113450` unchanged, `environment.yml` `dc444907` unchanged | ✓ all six blob shas exact |
| Assumption 8: `bismark-align.diff` touches only `main.nf` | ✓ the diff's own header lists `environment.yml`, `meta.yml`, both test files, the module test config and both `.conda-lock` files as unchanged |
| §2.3: `bismark_align.config` emits no `--multicore`/`--parallel`/`-p`/`--minimap2`/`--rammap` | ✓ read the whole `ext.args` closure |
| `BISMARK_ALIGN` is `process_high` = 12 cpus / 72 GB; methylseq overrides only `time` | ✓ `conf/base.config:44-48` (label) and `:75-77` (`withName: BISMARK_ALIGN { time = { 8.d * task.attempt } }` — nothing else) |
| `resourceLimits = [cpus: 4, …]` in both `conf/test.config` and `tests/nextflow.config` | ✓ both, and both also set `memory: '15.GB'` — which matters, see §1.3 |
| `nf-test.config` ignores `modules/nf-core/**/tests/*` | ✓ (also `subworkflows/nf-core/**/tests/*`) |
| §2.4: `.nftignore` does not reach `getReadsMD5()` | ✓ **the plan's subtlest claim, and it is right.** In `bismark_hisat_variants.nf.test`: `stable_path = getAllFilesFromDir(outputDir, ignoreFile: 'tests/.nftignore')` but `bam_files = getAllFilesFromDir(outputDir, include: ['**/*.bam'])` — no `ignoreFile`. The reads MD5 genuinely escapes `.nftignore` |
| Scenario counts | ✓ `bismark_variants` 14, `combined_index` 4 (incl. `bismark_hisat_combined_index`), `bismark_hisat_variants` 3, `index_downloads` contains `bismark_hisat_with_hisat2_index`, `targeted_sequencing_variants` 5 (2 bismark) |
| Subworkflow passes no resource args | ✓ `fastq_align_dedup_bismark/main.nf:36-40` — three inputs, none resource-related; `BISMARK_SUMMARY` consumes `bam.name` only |
| `options.rs:158-169` — `-p` always ships with `--reorder` | ✓ verbatim, and **no aligner gate** in the block, so HISAT2 gets `--reorder` too. (`options.rs:247`'s "`--reorder` is Bowtie-2-only" is inside `minimap2_options`, describing a clean slate that discards it — not a contradiction) |
| `mod.rs:154-161` — the HISAT2 thread-dependence notice | ✓ quote is verbatim: "deterministic and byte-identical to Perl `--hisat2 -p {n}`, but the result depends on the thread count (it is NOT identical to single-core HISAT2)" |
| `config.rs` cited for `-p`/`--multicore` behaviour | ✓ `config.rs:801-809` is the only `-p`-related validation: `--hisat2` with both `--multicore N` and `-p M` is rejected as ambiguous. Upstream's `!args.contains('--multicore')` guard means both are never emitted together |

### 1.2 The combined-index no-op claim — verified, and it survives a sharp edge the plan did not name

The claim is correct, but for a reason worth recording because it is fragile.

methylseq's config emits **both** flags in non-directional combined mode:

```groovy
use_combined ? ' --combined_index' : '',
( use_combined && ( params.single_cell || params.non_directional || params.zymo ) ) ? ' --combined_index_sequential' : '',
```

The bare `--combined_index` is **always** present when combined mode is on. That matters because the
patch's condition is a negative-lookahead regex, `/--combined_index(?!_)/`, which requires the bare
flag — had methylseq emitted only `--combined_index_sequential`, the patch would **not** have matched
(falling through to the legacy `--multicore` branch) while upstream's plain
`args.contains('--combined_index')` substring test **would** match. The two would then disagree and the
no-op claim would be false. They agree only because the bare flag is emitted unconditionally.

Given that, the equivalence is exact:

- patch: matches → `task.cpus >= 2` → `-p ${task.cpus}`
- upstream: `contains('--combined_index')` → true; `--non_directional` present but
  `--combined_index_parallel` absent → `n_instances = 1` → `pthreads = cpus` → emitted when `>= 2`

Same value, same floor, for every combination methylseq can produce. §2.2's conclusion and V6 stand.

I also confirmed the patch's stated rationale is real: `config.rs:1250-1256` rejects
`--combined_index` with `--multicore`/`--parallel` outright, so the classic/combined split is genuinely
required rather than an optimisation.

### 1.3 The `-p` matrix (§3) — every cell recomputed, all correct

The old-model arithmetic depends on `memory` as well as `cpus`, which the plan does not spell out but
gets right. With `resourceLimits` at cpus 4 / 15 GB:

- directional: `ccore = 4/3 = 1`; `mcore = 15GB/13GB = 1`; `min = 1`, not `> 1` → **no `--multicore`** ✓
- non-directional: `ccore = 4/5 = 0`; `mcore = 15GB/18GB = 0` → **no flag** ✓

At production 12 cpus / 72 GB:

- directional: `ccore = 12/3 = 4`; `mcore = 72/13 = 5`; `min = 4` → **`--multicore 4`** ✓
- non-directional: `ccore = 12/5 = 2`; `mcore = 72/18 = 4`; `min = 2` → **`--multicore 2`** ✓

New model: directional `n=2` → `-p 2` @4, `-p 6` @12 ✓; non-directional `n=4` → no flag @4 (1 < 2),
`-p 3` @12 ✓; combined `n=1` → `-p 4` @4, `-p 12` @12 ✓; hisat identical to directional ✓.

Every cell in §3 is right.

### 1.4 No hard-failure risk from `-p` in any methylseq-reachable combination

I checked for the failure mode where the pipeline breaks outright: Rust bismark has **no** validation
rejecting `-p` alongside `--non_directional`, `--pbat`, `--combined_index_sequential`, `--local` or
`--unmapped`. The only `-p` guards are `p < 2` (`options.rs:163-167`) — never emitted, upstream floors
at 2 — and the HISAT2 `--multicore`+`-p` ambiguity die. So the change cannot produce an immediate
crash. Good news the plan could state, since it is the cheapest thing a reviewer will worry about.

---

## 2. Logic review — findings

### 2.1 CRITICAL — "upstream's own tests run at cpus=2" is false, and it inverts the plan's central prediction

This is the finding that changes the plan.

`nf-core/modules`' test config, `tests/config/nf-test.config` @ `c565f1b2`, contains:

```groovy
process {
    cpus   = 4
    memory = '15.GB'
    time   = '2.h'
}
```

A hard `cpus = 4` set, overriding the `process_high` label. **Upstream module tests run at cpus = 4 —
the same effective value as methylseq CI**, not cpus = 2.

The plan asserts the opposite in two places, and builds on it in both:

- §2.1, explaining why `tests/main.nf.test.snap` is unchanged: *"upstream's own tests run at cpus=2,
  where `-p` is omitted"*.
- §10, the closing argument: *"upstream's own module tests run at cpus=2, where `-p` is omitted and
  nothing moved, whereas methylseq runs at cpus=4, where `-p 2` is emitted and HISAT2 snapshots do
  move."*

What actually happened upstream:

1. #12385 added four new tests, all titled "4 cpus", which assert on the **report text** rather than
   snapshotting — e.g. `assert file(process.out.report[0][1]).text.contains('-p 2 --reorder')`. That is
   part of why the snap file did not change.
2. The **five pre-existing tests** — `bowtie2` SE/PE, `hisat2` SE/PE, `minimap2` SE — **do** snapshot
   `bam(process.out.bam[0][1]).getReadsMD5()`. At cpus 4 / 15 GB the old model emitted no flag
   (`min(4/3, 15/13) = 1`, not `> 1`); the new model emits `-p 2 --reorder` for the four
   bowtie2/hisat2 tests. So those four tests changed CLI from no-flag to `-p 2`.
3. `tests/main.nf.test.snap` blob sha is **identical** at both refs: `b416113450`. The stored reads
   MD5s are real values, including `hisat2 | single-end` = `936c0d5ce713130113e99e09a5b53afd` and
   `hisat2 | paired-end` = `fb9d284aec4b2c727af3c5ffd77c6381`.
4. #12385's CI was green on head `a9759e0f`: **58 success, 3 skipped, 0 NULL, 0 failure** across 61
   check runs, including all 16 nf-test shards on each of docker, singularity and conda. The 3 skipped
   are the two GPU matrices and `nf-core lint subworkflows` — none of which would have run
   `bismark/align`. So the unchanged snapshot was genuinely re-validated, not merely left alone.

**Therefore upstream CI is direct empirical evidence that HISAT2's reads MD5 did *not* move going from
no-`-p` to `-p 2 --reorder`.** At the very thread count methylseq CI will use. The `hisat2 | paired-end`
value discriminates (`fb9d284a…` ≠ bowtie2 PE `458c5350…`), so this is not an artefact of a degenerate
test set — though note `hisat2 | single-end` and `bowtie2 | single-end` share `936c0d5c…`, so the SE
test carries less signal.

Consequences for the plan:

- §3 consequence 3 ("**HISAT2 rows change output** … its reads MD5 is expected to change") is
  contradicted by the closest available measurement.
- §3.1's "moves" rows, and §1's headline "**exactly two snapshot files move**", are probably wrong.
  The likely outcome is that **zero** snapshot files move.
- §10's "Not blocking, recorded for honesty" note already hedges ("the reads MD5 might not move at
  all"), and §10's closing paragraph says the HISAT2 re-baseline "*does* bite harder in methylseq than
  upstream". That framing should be retired: it does not bite harder, because upstream ran at the same
  cpus and nothing moved.
- **V5 is pointed the wrong way.** It reads: "run the 2 HISAT2-bearing files, **inspect the failure**,
  then `--update-snapshot`". If nothing fails, V5 has no defined outcome and an implementer may run
  `--update-snapshot` anyway on the plan's authority, destroying the signal. If something *does* fail,
  upstream's result says it warrants investigation, not a re-baseline. V5 is the one gate that
  authorises overwriting recorded truth, and it currently presumes the overwrite is expected.

**Required change:** flip the default expectation to *no snapshot moves anywhere*, for Bowtie 2 and
HISAT2 alike, citing upstream's unchanged `b416113450` at cpus 4 as the basis. Make any movement — in
either family — a defect signal to be investigated first, with re-baselining a decision taken after
diagnosis rather than a scheduled step. §3.1's table becomes a single "must not move" column plus a
named exception path.

Note this does not weaken the plan; it strengthens it. The change becomes "no snapshot anywhere should
move, and here is upstream's green run proving it at the same thread count" — a much stronger claim
than "two files re-baseline by design", and a much easier PR to review.

### 2.2 IMPORTANT — file-scoped `--update-snapshot` would silently absorb Bowtie 2 movement

`tests/index_downloads.nf.test.snap` holds **five** scenarios in one file:

```
- Params: default with bowtie2-index
- Params: bismark with run_preseq with bowtie2-index
- Params: bismark with run_methurator with bowtie2-index
- Params: bwameth with bwameth-index
- Params: bismark_hisat with hisat2-index
```

Step 8 says regenerate "**scoped to the two HISAT2 test files**", and §3.1 scopes the expected movement
to the "`bismark_hisat with hisat2-index` scenario only". But `--update-snapshot` is applied per *test
run*, not per scenario: running that file rewrites entries for every scenario it executes. Three
bismark-Bowtie 2 scenarios and one bwameth scenario sit in the same file, all of them "must not move".

So the instruction as written creates exactly the vacuous-gate shape the plan elsewhere guards against:
a real Bowtie 2 regression in `default_with_bowtie2_index` would be absorbed into the re-baseline
without anyone seeing a failure.

**Required change:** before any `--update-snapshot` on `index_downloads.nf.test`, run it *without* the
flag and confirm that **only** the `bismark_hisat with hisat2-index` scenario fails. Then, after
regeneration, assert that the four non-hisat scenario blocks are byte-identical to the committed
version (a JSON-key-level diff, not an eyeball). `bismark_hisat_variants.nf.test.snap` needs no such
care — all three of its scenarios are HISAT2.

### 2.3 IMPORTANT — §2.4's stated reason is false: the bismark report *does* record the command line

§2.4 concludes: *"Nothing captures the bismark command line, so a CLI change with identical output
moves nothing."*

Upstream's four new tests disprove the premise directly — they assert on the report's text:

```groovy
{ assert file(process.out.report[0][1]).text.contains('-p 2 --reorder') }
```

The bismark alignment report embeds the aligner invocation. The plan's *conclusion* survives, but only
via `.nftignore`:

- the report is published to `${params.aligner}/alignments/logs/*.txt`, caught by
  `*/alignments/logs/*.txt`;
- `BISMARK_REPORT` / `BISMARK_SUMMARY` consume it and emit into `reports/` and `summary/`, caught by
  `*/{reports,summary}/*.{html,txt}`.

That is a materially different and weaker guarantee than "nothing captures it": it is a dependency on
two glob rules in a file a template merge can touch. And it leaves one thing genuinely unchecked — the
MultiQC outputs that **are** content-hashed. `bismark_alignment.txt`, `bismark_strand_alignment.txt`,
`bismark_mbias_*.txt` and `bismark-methylation-dp.txt` all appear in `stable_path` and are derived from
the reports. They should carry only parsed numeric fields, but the plan should say so explicitly rather
than resting on a claim I have shown to be false.

**Required change:** re-derive §2.4's conclusion from the two `.nftignore` globs, name them, and add a
one-line check that no un-ignored MultiQC file echoes the bismark CLI. Assumption 10 (template merges
can reformat configs) should extend to `.nftignore`, since that is now a named dependency.

### 2.4 IMPORTANT — §3.1 and V5 under-enumerate what a HISAT2 change would move

If HISAT2 output ever *did* move, the blast radius is far wider than §3.1's "reads MD5 + un-ignored
bismark multiqc stats". The hisat snapshot's `stable_path` list has **67 hashed entries**, including
every methylation call:

`*.bedGraph.gz`, `*.M-bias.txt`, `CHG_OT/CHG_OB/CHH_OT/CHH_OB/CpG_OT/CpG_OB_*.txt.gz`,
`*.bismark.cov.gz`, `*_splitting_report.txt` — roughly 40 hashes per scenario — plus the four MultiQC
bismark files above.

V5's expected outcome ("only reads-MD5 values and un-ignored bismark multiqc stats change") would
therefore be **violated by a correct run**, and an implementer following it literally would read a
legitimate downstream propagation as a defect. Fix the enumeration, or (better, given §2.1) drop the
"expected to move" framing entirely and keep only the `stable_name`-lists-identical assertion, which is
well chosen and remains valid either way.

### 2.5 IMPORTANT (factual) — the delta table omits `tests/nextflow.config`, which #12385 *did* change

§2.1's file table has a row for `tests/nextflow_pthreads.config` (absent/absent) and **no row for the
file that actually changed**. Blob shas for `modules/nf-core/bismark/align/tests/nextflow.config`:
`3313dfa884` @ `7d264419` → `6bdc178d47` @ `c565f1b2`. The change adds a `genomeprep_args` fallback so
the new tests can pass alignment-only flags to `BISMARK_ALIGN` without breaking genome prep:

```groovy
ext.args = params.containsKey('genomeprep_args') ? params.genomeprep_args : params.bismark_args
```

`gh api repos/nf-core/modules/commits/1976cfb7…` confirms #12385 touched exactly three files:
`main.nf` (+54/−30), `tests/main.nf.test` (+130/−0), `tests/nextflow.config` (+4/−1).

§11's self-review says: *"The corrected finding: the module's test config is `tests/nextflow.config`,
and it predates #12385."* That is the second wrong answer to the question the self-review was correcting
— the first error (existence check misreported as comparison) was caught, but the replacement conclusion
is also wrong. The file exists *and* was changed by the very PR under discussion.

No functional impact on methylseq: module tests are not executed there, `git checkout <sha> --
modules/nf-core/bismark/` brings the file along automatically, and V2's `cmp` would catch any
divergence. But a table presented as "verified live" should be right, and the same self-review paragraph
is what a reviewer will trust when deciding not to re-derive the table themselves.

Also cosmetic: the table lists 6 rows for `align`, but the module has 8 files — the two `.conda-lock`
blobs are unchanged and unlisted (they are mentioned in step 3).

### 2.6 IMPORTANT (factual) — an open methylseq PR *does* touch `modules.json`

§2.1 records "open methylseq PRs touching `modules.json`: **none** (5 open PRs, oldest activity 2025-08,
none in this area)". There are indeed 5 open PRs, but **#598** ("refactor: migrate to nf-core-utils
plugin", 24 files, base `dev`, updated 2026-04-10) changes `modules.json`.

Its hunk removes `utils_nextflow_pipeline`, `utils_nfcore_pipeline` and `utils_nfschema_plugin` from the
**subworkflows** section — a different region of the file from `modules/nf-core/bismark/*`, so a textual
merge should auto-resolve and the practical risk is low.

But three separate statements rest on the false version: §2.1's table row, §6's "**Ordering:**
independent of everything else open in methylseq. Land order does not matter", and §10's "no open PR
touches `modules.json`, so there is no reason to wait." Correct the fact and keep the conclusion with
its real justification: one open PR touches `modules.json`, in a non-overlapping section, so no
coordination is needed.

---

## 3. Assumptions

### 3.1 Stated assumptions — audit

| # | Assumption | Status |
|---|---|---|
| 1 | CI runs `BISMARK_ALIGN` at `task.cpus = 4` | **Inference, not observation** — see §3.2 below. Correctly predicts today's no-`--multicore` behaviour; V3 gives it teeth |
| 2 | Production default `process_high` = 12 cpus | ✓ verified, and "overrides only `time`" is exactly right |
| 3 | Config emits no thread or minimap-family flag | ✓ verified against the full closure |
| 4 | Bowtie 2 `-p N --reorder` byte-identical to single-core and to old `--multicore` | Not verifiable from here (Q6 Phase-0 lives in `plans/`, which I am not to browse) — but **independently corroborated** by upstream's unchanged `bowtie2` SE/PE reads MD5 at `-p 2` |
| 5 | HISAT2 deterministic per N, not equal across N | ✓ Bismark's own notice, `mod.rs:154-161`. But see §2.1 — at `-p 2` on sarscov2 it did not differ from no-`-p`, so "not equal across N" does not imply "will differ here" |
| 6 | Vendored module tests not executed by methylseq CI | ✓ `ignore = ['modules/nf-core/**/tests/*', …]` |
| 7 | Six non-`align` modules differ only in `meta.yml` | ✓ verified at blob level |
| 8 | `bismark-align.diff` touches only `main.nf` | ✓ |
| 9 | Upstream may advance — hence step 2 | ✓ sound, and step 2 is the right mitigation |
| 10 | Template merge could reformat configs | ✓ sound; **extend it to `.nftignore`**, which §2.3 shows is now a named dependency |

### 3.2 Unstated assumption worth surfacing: `resourceLimits` reduces `task.cpus`

The entire §3 matrix hangs on `resourceLimits = [cpus: 4]` capping `process_high`'s 12 down to a
`task.cpus` of 4 *as seen inside the script block*. That is documented Nextflow behaviour, and it is
consistent with methylseq's current no-`--multicore` CI behaviour — but note the asymmetry with
upstream, which sets `cpus = 4` outright and therefore proves nothing about `resourceLimits`.

This is not a defect, and V3 would catch it loudly (they would see `-p 6` where the plan predicts
`-p 2`). But it should be stated as an inference rather than left implicit, and it is a reason to treat
V3 as mandatory rather than best-effort.

### 3.3 Unstated assumption: which library types the Q6 Phase-0 benchmark covered

§5 quotes "wall −38…−46 %, peak PSS −44…−82 %" and §7 assumption 4 asserts three-way Bowtie 2
byte-identity, but neither says **which library types** were measured. This matters because of §4.3
below: the production non-directional cell (`-p 3` × 4 instances) is the most-changed configuration in
the whole matrix and is exercised by no gate in the plan. If Phase-0 was directional WGBS only, the
non-directional claim is an extrapolation and should say so.

---

## 4. Validation sufficiency

The gate set is well-constructed in shape — V1's positive control on `rastair/mbiasparser`, V2's
`cmp`-against-clean-checkout as the proof the patch is gone rather than half-applied, V7's confined
diff, and the closing note that "an unrun test reads as a pass" are all exactly right instincts. Three
gaps.

### 4.1 V5 authorises the wrong action (Critical) — see §2.1

### 4.2 V3 has the vacuous-pass shape the plan warns about elsewhere

V3 greps `.nf-test/**/.command.sh` for bismark invocations and expects "`-p 2` directional, `-p 4`
combined, no `-p` non-directional, **no `--multicore` anywhere**".

Two problems:

1. **The negative can pass vacuously.** If the glob matches nothing — work dir cleaned, `NFT_WORKDIR`
   pointed elsewhere, a scenario that never ran — "no `--multicore` anywhere" is trivially satisfied and
   reads as success. V3 must **first** assert a positive with a counted expectation (`-p 2` found in
   exactly N invocations, N = the number of directional scenarios executed) before asserting any
   absence. The plan already knows this failure mode: §11 records catching an existence check
   misreported as a comparison, and the V3–V6 note warns that an unrun test reads as a pass. V3 just
   does not apply the lesson to itself.
2. **A better gate already exists upstream.** Since the report text contains `-p 2 --reorder` (§2.3),
   methylseq can assert the CLI *in-test* on a published output, exactly as #12385 does, instead of
   grepping Nextflow internals post-hoc. That converts V3 from a manual step into a CI-enforced one.
   Worth doing even if only for the bismark and bismark_hisat default scenarios.

### 4.3 The production non-directional `-p 3` path is exercised by nothing

Tracing coverage for every row of §3:

| §3 row | CI coverage at cpus 4 | Production `-p` value | Exercised anywhere? |
|---|---|---|---|
| directional (bowtie2) | 14 `bismark_variants` + `default` + 3 `index_downloads` | `-p 6` | CLI yes (at `-p 2`); `-p 6` no |
| pbat | **no classic pbat scenario** (only `bismark_combined_index_pbat`) | `-p 6` | no |
| non-directional / zymo / single_cell | **no classic non-directional scenario**; and at cpus 4 `pthreads = 1` so no `-p` is emitted even if there were one | **`-p 3`, 4 instances** | **no** |
| hisat directional | 3 + 1 scenarios | `-p 6` | CLI yes (at `-p 2`) |
| combined index | 4 scenarios | `-p 12` | yes at `-p 4` |

I checked the 14 `bismark_variants` scenario names: none is non-directional or pbat. Non-directional
appears only under `combined_index` (which routes to `n_instances = 1`). Upstream's non-directional test
deliberately asserts the *absence* of `-p`. So the cell that changes most — `--non_directional` going
from one forked `--multicore 2` run to four concurrent instances at `-p 3` each — is verified by no gate
in this plan and by no test in either repo.

This is not a crash risk (§1.4: nothing rejects `-p` with `--non_directional`). It is a silent-wrong-
output risk, and it is the one place where the plan's "Bowtie 2 output is unchanged" headline is
extrapolated rather than measured at the relevant configuration.

**Recommendation:** state explicitly which library types Q6 Phase-0 covered. If non-directional was not
among them, add a one-off manual concordance run (`--non_directional`, 12 cpus, old `--multicore 2` vs
new `-p 3`, records compared both in file order and under `LC_ALL=C sort`) before the PR, or scope the
PR's claim to the library types actually measured.

### 4.4 Smaller gate notes

- **V8** — "`nf-core pipelines lint` … a wrong sha or a leftover patch entry fails loudly there" is
  right, and worth noting *why* it works for a de-patched module: lint applies any recorded patch before
  comparing, so removing both the file and the key while writing pristine upstream files is
  self-consistent. Doing it by hand rather than via `nf-core modules patch --remove` is fine precisely
  because step 3 already wrote pristine files — V2 is what proves it.
- **V9** — "enumerate check-runs and assert zero NULL conclusions" is exactly the right instinct. Worth
  adding the other half: also assert the **expected count** of nf-test check-runs, and treat `skipped`
  as distinct from `success`. On #12385 the tally was 58 success / 3 skipped / 0 NULL, and confirming
  the 3 skipped were GPU + lint-subworkflows (not a bismark shard) was necessary to trust the result.
  A bare "no failures" would not have distinguished those cases.

---

## 5. Efficiency

Nothing here is hot-path code and §5's framing is correct: N forked chunks each re-loading a ~3.5 GB
index become one instance per strand threaded internally, so index loads stop scaling with the
parallelism knob.

Two observations:

- **The memory-guard removal argument is sound and well made.** `ccore = min(cpus/3, memory/13GB)`
  existed because each fork carried its own index; `-p` threads share one, so peak is set by the index
  loads. §11's counterfactual ("had `-p` scaled memory, dropping the guard would have been a
  regression") is the right way to state it.
- **Total thread budget is respected in every cell**, which the plan does not say but should, as it is
  the obvious reviewer question: directional `2 × -p 6 = 12`; non-directional `4 × -p 3 = 12`; combined
  sequential `1 × -p 12 = 12`. One line in §5 pre-empts a round of PR discussion.

No efficiency concerns with the plan itself.

---

## 6. Alternatives

1. **Sync only `align`, not all seven** (§10's own open question). The plan assumes all seven and I
   agree, with a sharper reason than the one given: the other six change only `meta.yml`, so a
   seven-module repin costs nothing in review surface while an `align`-only repin leaves six modules
   pinned to a sha that is no longer the newest for their paths — which is exactly the state that made
   *this* sync necessary. Keep all seven.

2. **Assert the CLI in-test rather than grepping `.command.sh`** (see §4.2). Upstream's
   `report.text.contains('-p 2 --reorder')` pattern is available to methylseq's pipeline tests and turns
   V3 into a permanent regression gate. This is the highest-value addition available and it costs a
   couple of lines.

3. **Fetch the target sha directly instead of `--depth=50`.** `c565f1b2` is 21 commits behind
   `nf-core/modules@master` today; that repo moves fast enough that depth 50 may not reach it by the
   time this is implemented, and `git checkout <sha> -- <path>` then dies with "fatal: reference is not
   a tree". It fails loud, so this is a nuisance rather than a risk, but
   `git fetch nf-core-modules --depth=1 <full-40-char-sha>` is exact, cheap and immune to drift.

4. **Quote upstream's real `main.nf` guard in §2.2.** The plan's excerpt begins at
   `def n_instances = 2` and omits the enclosing condition:

   ```groovy
   else if (!args.contains('--multicore') && !args.contains('--parallel') && !args.contains('-p ') && task.cpus) {
   ```

   The omission is unhelpful in a way worth fixing because the omitted guard **strengthens** the plan's
   case: it is what preserves the patch's `!(args =~ /(?:^|\s)-p\s/)` behaviour and prevents a duplicate
   `-p` when a user supplies their own. A reader who checks the plan against the real file finds the
   quote incomplete at exactly the point where the equivalence argument is made. (Upstream's
   `contains('-p ')` is a looser substring test than the patch's anchored regex; I checked every flag
   methylseq can emit — `--pbat`, `--prefix`, `--score_min L,0,-N`, `--known-splicesite-infile …`,
   `--combined_index_sequential` — and none contains `-p `, so the looseness is harmless here. One line
   noting that closes the question.)

5. **Consider making the PR's claim "no snapshot moves"** (follows from §2.1). It is a stronger,
   cleaner claim than "two files re-baseline by design", it is backed by upstream's green CI at the same
   cpus, and it removes the reviewer's hardest question ("how do I know that re-baseline was
   legitimate?"). If a HISAT2 snapshot *does* move, that becomes a finding to investigate and report —
   which is the correct handling either way.

---

## 7. Action items

### Critical

1. **Correct the cpus premise and invert V5.** Upstream module tests run at `cpus = 4`
   (`nf-core/modules tests/config/nf-test.config`: `process { cpus = 4; memory = '15.GB' }`), not 2. The
   pre-existing `hisat2` SE/PE tests snapshot `getReadsMD5()`, went from no-flag to `-p 2 --reorder`
   under #12385, and their MD5s are **unchanged** in a snap file whose blob sha is identical at both refs
   (`b416113450`), validated by 58-success/3-skipped/0-NULL CI on docker + singularity + conda. Rewrite
   §2.1's note, §3 consequence 3, §3.1, §10's closing paragraph and §1's "exactly two snapshot files
   move" accordingly. Default expectation becomes **no snapshot moves**; any movement in either aligner
   family is a defect signal investigated before any re-baseline.

2. **Make the snapshot regeneration scenario-safe.** `index_downloads.nf.test.snap` holds three
   must-not-move bismark-Bowtie 2 scenarios plus a bwameth one alongside the single hisat scenario, and
   `--update-snapshot` is per-run, not per-scenario. Run it without the flag first and confirm only
   `bismark_hisat with hisat2-index` fails; after any regeneration, assert the other four scenario
   blocks are byte-identical.

### Important

3. **Re-derive §2.4.** The bismark report *does* record the CLI — upstream asserts on exactly that
   (`report.text.contains('-p 2 --reorder')`). Replace "nothing captures the bismark command line" with
   the two `.nftignore` globs that actually provide the guarantee
   (`*/alignments/logs/*.txt`, `*/{reports,summary}/*.{html,txt}`), and add a check that no
   content-hashed MultiQC file (`bismark_alignment.txt`, `bismark_strand_alignment.txt`,
   `bismark_mbias_*.txt`, `bismark-methylation-dp.txt`) echoes the CLI. Extend assumption 10 to cover
   `.nftignore`.

4. **Fix §3.1/V5's enumeration of what would move.** `stable_path` has 67 hashed entries per hisat
   scenario, including all methylation calls (bedGraph, M-bias, six context files, `.cov.gz`, splitting
   reports). "Reads MD5 + un-ignored bismark multiqc stats" would be violated by a correct run. Keep the
   `stable_name`-lists-identical assertion — it is the right guard and survives either outcome.

5. **Correct §2.1's delta table.** Add `tests/nextflow.config` (`3313dfa884` → `6bdc178d47`, changed by
   #12385, adds the `genomeprep_args` fallback); drop the phantom `tests/nextflow_pthreads.config` row;
   fix §11's "it predates #12385". #12385 touched exactly three files: `main.nf`,
   `tests/main.nf.test`, `tests/nextflow.config`.

6. **Correct the open-PR claim.** #598 ("refactor: migrate to nf-core-utils plugin") *does* touch
   `modules.json`, in the subworkflows section — no overlap with the bismark entries, so the "no
   coordination needed" conclusion stands, but §2.1, §6 and §10 all currently cite a false fact.

7. **Close the non-directional gap.** State which library types Q6 Phase-0 covered. The production
   `--non_directional` cell (`-p 3` × 4 instances) is exercised by no gate in this plan and no test in
   either repo — methylseq has no classic non-directional pipeline scenario, and at cpus 4 no `-p` is
   emitted anyway. Either add a manual old-vs-new concordance run for it or scope the PR's byte-identity
   claim to the measured library types.

8. **Give V3 a positive assertion before its negative.** "No `--multicore` anywhere" passes vacuously on
   an empty glob. Assert a counted positive (`-p 2` in exactly N invocations, N = executed directional
   scenarios) first. Better: adopt upstream's in-test `report.text.contains('-p 2 --reorder')`
   assertion.

### Optional

9. Quote upstream's full `else if (!args.contains('--multicore') && !args.contains('--parallel') &&
   !args.contains('-p ') && task.cpus)` guard in §2.2 — it strengthens the equivalence argument the
   section is making. Note that `contains('-p ')` is a looser test than the patch's regex, harmless for
   every flag methylseq emits.
10. Replace `git fetch --depth=50 master` with `git fetch nf-core-modules --depth=1 <full-sha>`;
    `c565f1b2` is already 21 commits behind master and will drift further.
11. Record in §2.2 *why* the combined-index equivalence holds so narrowly: methylseq emits the bare
    `--combined_index` unconditionally, which is the only reason the patch's `(?!_)` lookahead and
    upstream's plain substring test agree. Had only `--combined_index_sequential` been emitted, they
    would diverge. That is a real trap for anyone editing `bismark_align.config` later, and it belongs
    next to the claim.
12. Add one line to §5 noting the total thread budget is respected in every cell
    (`2 × 6 = 4 × 3 = 1 × 12 = 12` at `process_high`) — it pre-empts the obvious PR question.
13. Extend V9 to assert the expected *count* of nf-test check-runs and to treat `skipped` as distinct
    from `success`, not just to check for NULLs.
14. Add the two unchanged `.conda-lock` blobs to §2.1's table for completeness (8 files, not 6).

---

## 8. Verdict

**REQUEST CHANGES.**

The plan's mechanics are ready to implement essentially as written, and its hardest technical claim —
that dropping `bismark-align.diff` is a true no-op on the combined-index path — I verified
independently and it is exactly right, including the `>= 2` floor. The §3 arithmetic is correct in every
cell, including the memory term in the old model that the plan does not show but gets right. The
`.nftignore`-versus-`getReadsMD5()` analysis is subtle and correct.

What blocks it is that the plan's central risk narrative is built on a false premise about upstream's
test cpus, and the correction runs in the plan's favour: upstream already ran this change at cpus 4 with
snapshotted HISAT2 reads MD5s and nothing moved. Because the plan expects movement, its one
snapshot-overwriting gate is written to expect a failure and re-baseline it, and step 8's file-scoped
`--update-snapshot` would sweep three must-not-move Bowtie 2 scenarios along with it. Those two
together are a path for a real regression to land as a routine re-baseline — the exact outcome §3.1's
"defect signal, not a re-baseline" rule exists to prevent.

Fix items 1–2, correct the factual errors in 3–6, and close the non-directional coverage statement in 7,
and this is a strong plan for a low-risk, well-evidenced change.
