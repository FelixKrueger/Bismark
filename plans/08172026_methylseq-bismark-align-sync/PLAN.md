# PLAN — Sync methylseq's vendored `bismark/*` modules onto the merged `-p` threading model

**Created:** 2026-08-17 · **Revised:** 2026-08-17 (rev-1, post dual plan review)
**Target repo:** `nf-core/methylseq` (branch `dev`) — *not* this repo. Plan lives here by the
convention of the alignment-mode track (see `plans/07182026_bismark-align-resource-model/`).
**Track:** nf-core/methylseq discussion issue [#615](https://github.com/nf-core/methylseq/issues/615),
Q6 (`--multicore` fork → intra-instance `-p` divisor).
**Status:** implemented and shipped as [nf-core/methylseq#622](https://github.com/nf-core/methylseq/pull/622) on Felix's trigger; CI complete. See §12.

---

## 0. Revision history

**rev-1 (2026-08-17)** — folds both reviewers' findings. The substantive change is that **rev-0's
central prediction was backwards**:

| rev-0 said | rev-1 says |
|---|---|
| Upstream module tests run at `cpus = 2`, so `-p` was never emitted there | Upstream sets `cpus = 4` outright. It **was** 2 at the sha methylseq pins, and was bumped to 4 on **2026-07-23** by [#12402](https://github.com/nf-core/modules/pull/12402) — six days before #12385 merged. The work was developed at 2 and merged at 4 |
| Exactly two snapshot files move; HISAT2 re-baselines by design | **No snapshot should move anywhere.** Upstream's HISAT2 tests snapshot a reads MD5, went from no-flag to `-p 2 --reorder` at cpus=4, and their MD5s are unchanged in a snap blob identical either side of #12385, validated by green CI on docker + singularity + conda |
| V5 expects a failure and re-baselines it | V5 is inverted: movement in **either** aligner family is a defect signal, diagnosed before any re-baseline is even considered |

Both reviewers independently caught the false premise. They contradicted each other on the mechanism —
Reviewer A said the upstream snapshots capture "a boolean + `unmapped` + versions, **not** a reads MD5"
and attributed a count of 5047 to HISAT2; Reviewer B said they capture `getReadsMD5()`. **B is right.**
Verified directly: the `hisat2 | single-end` entry's first element is
`"936c0d5ce713130113e99e09a5b53afd"`, and 5047 belongs to the **minimap2** test — the HISAT2 tests
assert 5009. A's misattribution understated its own finding, since MD5-level evidence is far stronger
than count-level.

Also corrected in rev-1: the §2.1 delta table (a changed file was missing), the §2.4 reasoning (its
conclusion held for the wrong reason), the open-PR claim, four vacuous-pass channels in the gates, and
a per-file `--update-snapshot` that would have swept four must-not-move scenarios.

---

## 1. Goal

Bring `nf-core/methylseq`'s seven vendored `modules/nf-core/bismark/*` modules from the sha they are
pinned at today (`7d264419`, the bismark 3.1.0 bump of 2026-07-14) up to current upstream
(`c565f1b27c06a1e8b726e2a646d4c822ae3b514a`, 2026-08-15), so that the `-p` threading model merged
upstream in [nf-core/modules#12385](https://github.com/nf-core/modules/pull/12385) actually reaches
the pipeline. In the process, **retire `bismark-align.diff`** — methylseq's local patch from
[#616](https://github.com/nf-core/methylseq/pull/616).

The patch retirement is **not cleanup, it is the enabling step**: the patch's `else if` is exactly what
suppresses the `-p` model on the classic path, so importing the new module without removing the patch
would ship the change and disable it.

Intended outcome: methylseq gains the measured Q6 win (wall −38…−46 %, peak PSS −44…−82 % on oxy,
Bowtie 2 records byte-identical); no vendored patch remains on `bismark/align`; and **no snapshot
anywhere moves**, with upstream's green CI at the same thread count as the basis for that expectation.

## 2. Context

### 2.1 What is where (verified live 2026-08-17; re-verified in review by two independent reviewers)

| Fact | Value |
|---|---|
| methylseq `dev` HEAD | `9855ef9444cfb0111517b6f5f56bb4a0cd78ed7a` (2026-08-11, template merge 4.0.3); version `4.3.0dev` |
| methylseq pins all 7 `bismark/*` modules at | `7d264419ee98edd5a0886d3782fb87553628272b` |
| upstream latest for all 7 `bismark/*` paths | `c565f1b27c06a1e8b726e2a646d4c822ae3b514a` |
| `bismark/align` gained in between | `1976cfb7f315a29658046728e071d9f60f3e1fe9` (#12385, `-p` model) **and** `c565f1b2` (#12581, meta.yml dead links) |
| the other 6 modules gained in between | `c565f1b2` **only** — verified at blob level: `main.nf`, `environment.yml`, both test files and both `.conda-lock` blobs are byte-identical across the range; only `meta.yml` differs |
| methylseq has a recorded patch | `modules/nf-core/bismark/align/bismark-align.diff`, registered as `"patch"` in `modules.json` |
| other patched module in the repo | `rastair/mbiasparser` — **out of scope**; it is also V1's positive control (42 nf-core modules, exactly 2 patched) |
| open methylseq PRs | 5. **#598 ("refactor: migrate to nf-core-utils plugin") does modify `modules.json`** (+0/−15, removing three `utils_*` entries from the **subworkflows** section — no overlap with `modules/nf-core/bismark/*`). The other 4 do not touch it |
| latest methylseq release | 4.2.0 (2025-12-12) — no in-flight release |

**`bismark/align` file-level delta — all 8 component files, each probed for presence at *both* refs:**

| File | `7d264419` | `c565f1b2` | Note |
|---|---|---|---|
| `main.nf` | `ff7af322` | `848dbab1` | #12385 (+54/−30) plus nf-core reformatting |
| `meta.yml` | `1bd9fcd5` | `82bb0986` | #12581 docs |
| `tests/main.nf.test` | `b2fe71b6` | `17787b38` | #12385 (+130/−0) — four new `-p` tests |
| **`tests/nextflow.config`** | **`3313dfa884`** | **`6bdc178d47`** | **#12385 (+4/−1)** — adds a `genomeprep_args` fallback so alignment-only flags don't reach genome prep |
| `tests/main.nf.test.snap` | `b416113450` | `b416113450` | unchanged — see §2.5, this is the key evidence |
| `environment.yml` | `dc444907` | `dc444907` | unchanged; container stays `bismark:3.1.0` |
| `.conda-lock/linux_amd64-bd-9557d6ab108a83e4_1.txt` | unchanged | unchanged | tree blob `b1732a24e2` at both refs |
| `.conda-lock/linux_arm64-bd-f83bc6617fa3cadd_1.txt` | unchanged | unchanged | same |

#12385 touched **exactly three files** — confirmed mechanically via
`gh api repos/nf-core/modules/commits/1976cfb7… --jq '.files[].filename'`, which is how this table
should be derived. rev-0 hand-enumerated it and lost `tests/nextflow.config`.

### 2.2 The patch that upstream supersedes

`bismark-align.diff` confines itself to `main.nf` (its own header lists all eight other component
files as unchanged). It inserts a combined-index branch ahead of the `--multicore` block and demotes
that block to `else if`:

```groovy
if(args =~ /--combined_index(?!_)/){
    if(task.cpus && (task.cpus as int) >= 2 && !(args =~ /(?:^|\s)-p\s/)){
        args += " -p ${task.cpus}"
    }
}
else if(!args.contains('--multicore') && task.cpus){   // was: if(...)
```

Upstream `main.nf` @ `c565f1b2`, quoted **with its enclosing guard** (rev-0 omitted it, at exactly the
point where the equivalence argument is made — and the guard strengthens that argument, since it is
what preserves the patch's "don't double up on a user-supplied `-p`" behaviour):

```groovy
else if (!args.contains('--multicore') && !args.contains('--parallel') && !args.contains('-p ') && task.cpus) {
    def n_instances = 2
    if (args.contains('--combined_index')) {
        n_instances = args.contains('--non_directional') && args.contains('--combined_index_parallel') ? 2 : 1
    }
    else if (args.contains('--non_directional')) {
        n_instances = 4
    }
    def pthreads = ((task.cpus as int) / n_instances) as int
    if (pthreads >= 2) { args += " -p ${pthreads}" }
}
```

For every argument combination methylseq can emit, `n_instances = 1` in combined-index mode, so
`pthreads == task.cpus` — identical to the patch, `>= 2` floor included.

> ⚠️ **The equivalence is narrower than it looks, and this is a trap for whoever edits
> `conf/modules/bismark_align.config` next.** The patch matches `--combined_index(?!_)` (the *bare*
> flag only); upstream matches `contains('--combined_index')` (which also matches
> `--combined_index_sequential`). They agree **only because that config emits the bare flag
> unconditionally** whenever combined mode is on. A future config emitting only
> `--combined_index_sequential` would make the patch fall through to `--multicore` while upstream still
> took the combined branch — and since bismark rejects `--combined_index` together with
> `--multicore`/`--parallel` (`rust/bismark/src/aligner/config.rs:1250-1256`), that divergence would
> fail loudly at 12 cpus rather than silently. Both reviewers found this independently.

Upstream's `contains('-p ')` is a looser test than the patch's anchored regex. Harmless here: no flag
methylseq emits (`--pbat`, `--prefix`, `--score_min L,0,-N`, `--known-splicesite-infile …`,
`--combined_index_sequential`, `--minins`/`--maxins`, `--local`, `--unmapped`) contains the substring
`-p `.

methylseq never emits `--combined_index_parallel`, so upstream's `_parallel` clause is inert here —
correctly, since a sequential combined run uses the whole cpu budget per pass.

### 2.3 How methylseq drives the module

- `conf/modules/bismark_align.config` builds `ext.args` from params: `--bowtie2` | `--hisat2`,
  `--pbat`, `--non_directional` (from `single_cell` | `non_directional` | `zymo`),
  `--combined_index` (+ `--combined_index_sequential` when non-directional), `--unmapped`,
  `--score_min`, `--local`, `--minins`/`--maxins`. **It never emits `--multicore`, `--parallel`,
  `-p`, `--minimap2` or `--rammap`** — so upstream's `isMinimapLike` branch is dead code in
  methylseq (left alone deliberately: divergence from upstream is worse than unreachable code), and
  the `-p` branch always applies.
- `BISMARK_ALIGN` carries `label 'process_high'` (12 cpus / 72 GB); methylseq overrides only `time`
  (`conf/base.config:75-77`).
- Both `conf/test.config` and `tests/nextflow.config` set `resourceLimits = [cpus: 4, memory: '15.GB',
  time: '1.h']`, so **CI runs at `task.cpus = 4`**. Note this is an *inference* from documented
  Nextflow behaviour (`resourceLimits` capping the label's 12 down to 4 as seen inside the script
  block), corroborated by methylseq's current no-`--multicore` CI behaviour but not directly observed.
  V3 is what gives it teeth — it would show `-p 6` instead of `-p 2` if the inference were wrong.
- `nf-test.config` ignores `modules/nf-core/**/tests/*`, so the four `-p` module tests that arrive
  with the sync are shipped as files but **never executed by methylseq CI**. (Its CI also passes
  `--filter pipeline`.)
- `subworkflows/nf-core/fastq_align_dedup_bismark` invokes `BISMARK_ALIGN` and passes no
  resource-related arguments, so it needs no change.

### 2.4 What the snapshots actually capture

Every bismark pipeline test snapshots four things: the versions YAML, a **name** list
(`stable_name`), a **content-hash** list (`stable_path`, built with `ignoreFile: 'tests/.nftignore'`),
and

```groovy
bam_files.collect{ file -> [ file.getName(), bam(file.toString()).getReadsMD5() ] }
```

where `bam_files = getAllFilesFromDir(outputDir, include: ['**/*.bam'])` — **a separate call with no
`ignoreFile:`**. So although `.nftignore` excludes the BAMs from content hashing, the reads MD5
escapes it and is snapshotted.

**Does anything snapshot the bismark command line?** rev-0 said "nothing captures it". That was wrong
in its reasoning even though the conclusion holds: **the bismark report does embed the aligner
invocation** — upstream's own new tests assert on exactly that
(`file(process.out.report[0][1]).text.contains('-p 2 --reorder')`). The real guarantee is two
`.nftignore` globs:

- `*/alignments/logs/*.txt` — the alignment report as published by `BISMARK_ALIGN`;
- `*/{reports,summary}/*.{html,txt}` — the `BISMARK_REPORT` / `BISMARK_SUMMARY` derivatives.

That is a weaker guarantee than "nothing captures it": it is a dependency on two glob rules in a file
a template merge can touch (hence assumption 10 below). And it leaves one thing to check explicitly:
**MultiQC's bismark data files are *not* ignored.** `.nftignore` names five specific
`multiqc_data/*.txt` files, and `multiqc_bismark_*` is not among them, so `bismark_alignment.txt`,
`bismark_strand_alignment.txt`, `bismark_mbias_*.txt` and `bismark-methylation-dp.txt` are
content-hashed. They should carry only parsed numeric fields — V10 asserts that rather than assuming it.

**Blast radius if output ever did change:** far wider than "reads MD5 plus a couple of MultiQC files".
For one HISAT2 scenario the snapshot holds 231 `stable_name` entries and **67 `stable_path` content
hashes**, including every methylation call — `*.bedGraph.gz`, `*.M-bias.txt`, the six
`CHG/CHH/CpG_{OT,OB}_*.txt.gz` context files, `*.bismark.cov.gz`, `*_splitting_report.txt`. rev-0's V5
expectation ("only reads-MD5 and un-ignored bismark multiqc stats change") would have been violated by
a *correct* run.

### 2.5 Upstream's own CI is the evidence that nothing should move

This is what rev-0 got backwards, and the correction is the plan's strongest asset.

`nf-core/modules` sets a hard `process { cpus = 4; memory = '15.GB'; time = '2.h' }` in
`tests/config/nf-test.config`. That value was **2 at `7d264419`** and was bumped to **4 on 2026-07-23**
by #12402 ("bump default CPU and memory to use the full runner resources") — six days before #12385
merged on 07-29. So:

1. Of the module's **9** tests, the **4 added by #12385 are assertion-only** (no `snapshot()` call);
   they assert on the report text. That is part of why the snap file did not grow.
2. The **5 pre-existing tests** — `bowtie2` SE/PE, `hisat2` SE/PE, `minimap2` SE — **do** snapshot
   `bam(...).getReadsMD5()`. At cpus=4 the old model emitted no flag (`min(4/3, 15GB/13GB) = 1`, not
   `> 1`); the new model emits `-p 2 --reorder` for the four bowtie2/hisat2 tests.
3. `tests/main.nf.test.snap` is byte-identical at both refs (blob `b416113450`), holding real MD5
   values — `hisat2 | single-end` = `936c0d5ce713130113e99e09a5b53afd`, `hisat2 | paired-end` =
   `fb9d284aec4b2c727af3c5ffd77c6381`.
4. #12385's final head `a9759e0f` was green: **58 success / 3 skipped / 0 failure / 0 NULL** across 61
   check runs, on docker, singularity and conda. The PR modified `bismark/align/main.nf`, so upstream's
   change detection certainly selected that module's tests. The unchanged snapshot was therefore
   **re-validated, not merely left alone**.

**Conclusion: HISAT2's reads MD5 did not move going from no-`-p` to `-p 2 --reorder`, at the exact
thread count methylseq CI uses.** The `hisat2 | paired-end` value discriminates (it differs from
bowtie2 PE), so this is not an artefact of a degenerate test set — though note `hisat2 | single-end`
and `bowtie2 | single-end` share the same MD5, so the SE test carries less signal.

This does **not** contradict Bismark's own thread-dependence notice
(`rust/bismark/src/aligner/mod.rs:154-161`, quoted in §3). "Not identical across N" is a statement
about what is *guaranteed*, not a prediction that any given N differs on any given input. On a genome
this small, the 1→2 step evidently does not.

## 3. Behaviour — the `-p` matrix this change produces

Integer division; `-p` emitted only when `pthreads >= 2`. The old model's value depends on **memory as
well as cpus** (`ccore = min(cpus/N, memory/M)`, with `N=3, M=13GB` directional and `N=5, M=18GB`
non-directional).

| methylseq scenario | relevant `ext.args` | `n` | today @ cpus 4 | after @ cpus 4 | today @ cpus 12 | after @ cpus 12 |
|---|---|---|---|---|---|---|
| bismark default / rrbs / em_seq / nomeseq / targeted / preseq / qualimap / skip_* | `--bowtie2` | 2 | *(no flag)* | `-p 2` | `--multicore 4` | `-p 6` |
| pbat | `--bowtie2 --pbat` | 2 | *(no flag)* | `-p 2` | `--multicore 4` | `-p 6` |
| non_directional / zymo / single_cell | `--bowtie2 --non_directional` | 4 | *(no flag)* | *(no flag)* | `--multicore 2` | `-p 3` |
| **bismark_hisat** default / rrbs / save_reference | `--hisat2` | 2 | *(no flag)* | **`-p 2`** | `--multicore 4` → bismark remaps to `-p 4 --reorder` | `-p 6` |
| combined_index directional / pbat / non-directional / hisat | `… --combined_index …` | 1 | `-p 4` *(patch)* | `-p 4` | `-p 12` *(patch)* | `-p 12` |

At cpus=4 the old model emits nothing in *every* row — the memory term pins it to 1 independently of
cpus (`15GB/13GB = 1`), so today's CI has no threading flag anywhere.

Four consequences:

1. **Combined-index rows do not change at all** — the claim that the patch is superseded rather than
   merely replaced. Gated by V6.
2. **Bowtie 2 rows change CLI but not output.** `-p N` always ships with `--reorder`
   (`rust/bismark/src/aligner/options.rs:158-169`, with no aligner gate, so HISAT2 gets it too), and
   the Q6 Phase-0 benchmark proved Bowtie 2 records byte-identical three ways (single-core oracle ==
   old `--multicore` == new `-p`), both file-order and `LC_ALL=C sort`, **directional and
   non-directional**. Independently corroborated by upstream's unchanged bowtie2 MD5s (§2.5).
3. **HISAT2 rows change CLI, and per §2.5 are not expected to change output either.** Bismark's notice
   (`mod.rs:154-161`) says the result *"depends on the thread count (it is NOT identical to single-core
   HISAT2)"` — a guarantee boundary, not a prediction. Upstream measured this exact transition at
   cpus=4 and the MD5 held.
4. **Nothing can crash.** Rust bismark has no validation rejecting `-p` alongside `--non_directional`,
   `--pbat`, `--combined_index_sequential`, `--local` or `--unmapped`. The only `-p` guards are the
   `p < 2` floor (never reached — upstream floors at 2) and the HISAT2 `--multicore`+`-p` ambiguity
   error (`config.rs:801-809`), which upstream's `!args.contains('--multicore')` guard makes
   unreachable. Cheapest reviewer worry, dispatched.

### 3.1 Expected snapshot movement: **none**

| Snapshot | Expectation |
|---|---|
| `tests/default.nf.test.snap` | must not move |
| `tests/bismark_variants.nf.test.snap` (14 scenarios) | must not move |
| `tests/bismark_hisat_variants.nf.test.snap` (3 scenarios) | must not move — see §2.5 |
| `tests/index_downloads.nf.test.snap` (5 scenarios, 1 of them hisat) | must not move |
| `tests/combined_index.nf.test.snap` (4 scenarios + 1 loud-fail negative) | must not move — CLI is unchanged here |
| `tests/targeted_sequencing_variants.nf.test.snap` (2 bismark scenarios) | must not move |
| `tests/bwameth_*`, `tests/bwamem_taps` | untouched — different aligner |

**Any movement, in either aligner family, is a defect signal.** Re-baselining is not a scheduled step
in this plan; it is a decision taken only after diagnosis, and it requires a written explanation of
*why* the new value is correct. The named exception path is §9's V5.

## 4. Implementation outline

1. **Clone upstream, not the fork.** `FelixKrueger/methylseq`'s own `dev` is chronically stale. Clone
   `nf-core/methylseq`, branch off `origin/dev`, e.g. `chore/bismark-modules-sync-12385`. Push to the
   fork, PR into upstream `dev`.
2. **Re-verify before touching anything.** Upstream may have moved since 2026-08-17. Derive the delta
   *mechanically* (`gh api repos/nf-core/modules/commits/<sha> --jq '.files[].filename'`, unioned over
   the range) rather than by hand — that is what produced rev-0's missing row. Three questions to
   answer and record in the PR body:
   - what is the target sha, and which modules changed **behaviourally** vs docs-only?
   - did any `.conda-lock` **filename** or container **digest** change? (see step 6)
   - is `tests/config/nf-test.config`'s `cpus` still 4? (it has moved once already)
3. **Import the seven modules by sha, avoiding `nf-core modules update`** — it cascades into unrelated
   modules. Fetch **by sha**, not by branch depth: `c565f1b2` is already 21 commits behind
   `nf-core/modules@master` and that repo moves fast enough that a depth guess will rot.
   ```bash
   git remote add nf-core-modules https://github.com/nf-core/modules.git
   git fetch nf-core-modules --depth=1 <full-40-char-target-sha>
   git checkout <full-40-char-target-sha> -- modules/nf-core/bismark/
   ```
   Path prefixes are identical in both repos, so this lands the files in place, including `tests/`,
   `meta.yml` and `.conda-lock/`.
4. **Delete the retired patch.** `git checkout <sha> -- <path>` overwrites and adds but never deletes,
   so `bismark-align.diff` survives step 3:
   ```bash
   git rm modules/nf-core/bismark/align/bismark-align.diff
   ```
   Then remove the `"patch"` key from the `bismark/align` entry in `modules.json`. (Equivalent to
   `nf-core modules patch --remove bismark/align`; by hand to keep the blast radius to bismark. This is
   self-consistent because step 3 wrote pristine upstream files — `nf-core pipelines lint` applies any
   recorded patch before comparing, so file and key must go together, and V2 is what proves they did.)
5. **Repin `modules.json`.** Full 40-char target sha for all seven `bismark/*` entries. Leave `branch`
   and `installed_by` alone; do not touch `rastair/mbiasparser` or any non-bismark entry.
6. **Check the `conf/` coupling.** `conf/containers_conda_lock_files_{amd64,arm64}.config` hard-code
   the vendored lock-file paths **by exact filename**, and `conf/containers_{docker,singularity_*}_*.config`
   hard-code container digests. This is a no-op for `7d264419 → c565f1b2` (lock-file tree blob
   `b1732a24e2` at both refs, both filenames unchanged) — which is the *only* reason step 7's "nothing
   under `conf/`" holds. If a future target sha changes a lock filename or digest, `conf/` must move
   too. No `includeConfig` references these files, so they appear to be `-c` opt-in: **a stale
   reference would not be caught by CI and would bite only users.** Gated by V11.
7. **Confine the diff.** `git diff --stat` must list only `modules/nf-core/bismark/**`,
   `modules.json`, and `CHANGELOG.md` (step 10). Nothing under `conf/`, nothing under `workflows/`.
8. **Prove the import is faithful.** `cmp` every imported file against a clean checkout of the target
   sha (V2). The vendored `main.nf` must equal upstream's byte-for-byte — the proof the patch is gone
   rather than half-applied.
9. **Run the tests; expect no snapshot to move.** Run the bismark-touching pipeline tests with docker,
   **without `--update-snapshot`**. Expected: all pass. If anything fails, stop and diagnose — do not
   regenerate. Only after a written diagnosis, and only for the specific failing scenario, consider a
   re-baseline under V5's scenario-safety rules.
10. **CHANGELOG**: one sentence, matching the file's existing format (emoji-prefixed bullet under
    `### Pipeline Updates`, `([#NNN](url))` suffix). It must name the **production** HISAT2 change:
    `--aligner bismark_hisat` goes from `-p 4` (via the old `--multicore 4` remap) to `-p 6` at
    `process_high`, so real users' results can shift even though the test-scale output does not. A
    changelog is read by someone deciding whether an upgrade affects them. No migration notes, no
    benchmark table.
11. **Lint**: `nf-core pipelines lint` (CI runs it on every PR via `linting.yml`, plus `--release`).
    **Do not run `nextflow lint -format`** — it breaks the nf-test classic parser.
12. **Optionally add the permanent gate** (recommended — see V4): adopt upstream's in-test assertion
    pattern in methylseq's own tests, so the emitted CLI is checked by CI forever rather than by a
    one-off manual grep.
13. **PR**: one short plain-language paragraph — what changed, why, and that no output changes at test
    scale, with upstream's green run at the same cpus as the evidence. Detail goes in the commit
    message; AI-assisted analysis in a folded `<details>` block.

## 5. Efficiency

Nothing here is hot-path; the efficiency claim is the pipeline's runtime. The model replaces N forked
bismark chunks (each re-loading a ~3.5 GB index) with one instance per strand threaded internally: one
index load per strand instead of N per strand. Measured on oxy 2026-07-18 (Bowtie 2, bismark 3.0.0,
verified applicable to 3.1.0 by tag-diff of `options.rs` + `parallel.rs`): wall −38…−46 %, peak PSS
−44…−82 %.

**The total thread budget is conserved in every cell** at `process_high`: directional `2 × -p 6 = 12`,
non-directional `4 × -p 3 = 12`, combined sequential `1 × -p 12 = 12`. Worth stating — it is the first
thing a reviewer will check.

The old memory guard (`ccore = min(cpus/3, memory/13GB)` directional, `min(cpus/5, memory/18GB)`
non-directional) has no analogue in the new branch and correctly disappears: peak memory is now set by
the index loads, which no longer scale with the parallelism knob. Had `-p` scaled memory, dropping the
guard would have been a regression. One consequence to state plainly: **the new model has no memory
guard at all**, so on a many-core, low-memory site config it emits `-p 6` where the old model throttled
to `--multicore 1`. That is still strictly less memory than the old model at the same cpus — one index
load is the floor either way — but the new peak is set by a quantity nothing in the config observes.

## 6. Integration

- **Reads:** `modules.json`, the seven vendored module directories, `tests/*.snap`.
- **Writes:** the same, plus `CHANGELOG.md`.
- **Downstream in-repo:** `subworkflows/nf-core/fastq_align_dedup_bismark` passes no resource args —
  no change needed.
- **Downstream out-of-repo:** none. A pipeline-local repin.
- **Ordering:** one open PR (#598) touches `modules.json`, but in the subworkflows section, with no
  overlap with the bismark entries — a textual merge auto-resolves, so no coordination is needed and
  land order does not matter.
- **CI triggering:** `nf-test.yml` has `paths-ignore: ["**/meta.yml", "**/*.md", "docs/**", …]`, so the
  six docs-only modules contribute nothing that could trigger or suppress CI. `nf-test.config`'s
  `triggers` list (which forces a full run) contains neither `modules.json` nor `modules/**`, so this
  PR does **not** hit the full-run escape hatch — test selection depends entirely on nf-test's
  dependency-graph resolution from the module up through the subworkflow to `../main.nf`. V3 asserts
  which tests actually ran rather than inferring it.
- **Considered and rejected — a two-PR split** (sync + prove nothing moves, then re-baseline
  separately): impossible, because if anything *did* need re-baselining, PR 1 would have to merge with
  red test files. Recorded because a reviewer will ask.
- **Not in scope:** the `-p` → `--threads` long-name switch mashehu asked for on #12385. The shipped
  `bismark:3.1.0` container has no `--threads`; that switch waits on a future bismark release and
  container bump.

## 7. Assumptions

Fixed (verified this session, most of them twice):

1. methylseq CI runs `BISMARK_ALIGN` at `task.cpus = 4` — an **inference** from `resourceLimits` in
   both `conf/test.config` and `tests/nextflow.config`, not a direct observation. V3 tests it.
2. Production default is `process_high` = 12 cpus / 72 GB; methylseq overrides only `time`.
3. `conf/modules/bismark_align.config` emits no thread flag and no minimap-family flag.
4. Bowtie 2 under `-p N --reorder` is byte-identical to single-core and to the old `--multicore`
   chunking — Q6 Phase-0, 3-way, both orderings, **directional and non-directional**. Corroborated
   independently by upstream's unchanged bowtie2 MD5s at cpus=4.
5. HISAT2 under `-p N` is deterministic per N and **not guaranteed** equal across N (Bismark's notice).
   Per §2.5, at the 1→2 step on sarscov2 it is in fact equal. HISAT2 was not part of the Phase-0
   benchmark; it rides the same mechanism.
6. Vendored module tests are not executed by methylseq CI (`nf-test.config` ignore globs + `--filter
   pipeline`).
7. The six non-`align` bismark modules differ from the pinned sha only in `meta.yml` (blob-level).
8. `bismark-align.diff` touches only `main.nf`.
9. Nothing in Rust bismark rejects `-p` in any methylseq-reachable combination.

Configurable / could change under us:

10. A template merge could reformat configs **or `.nftignore`** — the latter now matters, because §2.4's
    command-line guarantee rests on two of its globs. Re-check both if `dev` has moved.
11. Upstream `nf-core/modules` may advance before this lands — hence step 2. If `bismark/align` gains a
    *behavioural* commit after `c565f1b2`, re-derive §3 rather than trusting the table. In particular
    `tests/config/nf-test.config`'s `cpus` has already moved once (2 → 4 on 2026-07-23); §2.5's evidence
    is specific to cpus=4.

## 8. Signature

Not applicable — no new functions. The unit of change is `modules.json` + vendored files.

## 9. Validation

Every gate below states a **positive** assertion before any negative one, because a negative over an
empty set is the failure mode this project keeps hitting.

| # | What to verify | How | Expected |
|---|---|---|---|
| V1 | The patch is fully retired | `git status` shows `bismark-align.diff` deleted; a `python3` check over `modules.json` asserts **(a)** no `patch` key under `bismark/align` **and (b)** `rastair/mbiasparser` still has one, **and (c)** the count of patched modules is exactly 1 | all three hold |
| V2 | Import is faithful | clean-checkout the target sha to a temp dir; `cmp` all 8 files × 7 modules, asserting a **non-zero file count** first | 56 files compared, 0 differences |
| V3 | The right tests ran, and the CLI matches §3 | assert `total_shards > 0` from the `get-shards` job and record its `Executed N tests`; assert the `nf-test` matrix job's result is `success`, **not `skipped`**; then require these scenarios to appear as `ok`: 14 `bismark_variants`, `default`, all 5 `combined_index` tests, 3 `bismark_hisat_variants`, `bismark_hisat with hisat2-index`. Then per-scenario, from the published report `*/alignments/logs/*.txt` (**paired with a report-file count**, so "no match" is distinguishable from "no files"): `-p 2` for directional bowtie2 and hisat2, `-p 4` for combined, and `--multicore` absent everywhere | counts match; `-p` values per §3; `--multicore` count 0 over a non-zero file count |
| V4 | *(recommended)* The CLI gate becomes permanent | add upstream's in-test pattern to methylseq's own tests — `assert file(...report...).text.contains('-p 2 --reorder')` for the bismark and bismark_hisat default scenarios | CI-enforced from then on |
| V5 | **No snapshot moves** | run all six bismark-touching test files **without** `--update-snapshot` | **all pass.** Any failure is a defect signal: diagnose first, and write down why the new value is correct before even considering a re-baseline. If a re-baseline is agreed, it is scenario-safe: confirm the without-update run failed on **only** the intended scenario, then after `--update-snapshot` assert every other scenario block in that file is byte-identical (a JSON-key-level diff, not an eyeball) — `index_downloads.nf.test.snap` holds 3 bismark-Bowtie 2 scenarios and a bwameth one alongside its single hisat scenario, and `--update-snapshot` is per-run, not per-scenario |
| V6 | Combined-index is a true no-op | all 5 tests in `combined_index.nf.test` pass unchanged, including `bismark_hisat_combined_index` **and the loud-fail negative** ("pre-built index lacking the combined reference fails loudly") | PASS — the negative proves the rewrite did not turn a loud combined-index failure into a silent fall-through to the classic path |
| V7 | Diff is confined | `git diff --stat origin/dev` | only `modules/nf-core/bismark/**`, `modules.json`, `CHANGELOG.md` |
| V8 | Repo-level checks | `nf-core pipelines lint` (CI-enforced, cannot be skipped) | modules section clean |
| V9 | Full CI, non-vacuously | assert the **expected count** of nf-test check-runs; treat `skipped` as distinct from `success`; assert zero NULL conclusions. **Note `confirm-pass` reports success when the `nf-test` job is skipped** — its three steps fire only on `failure`/`cancelled`/`success`, and none matches `skipped`. Also read the `latest-everything` PR-comment fragment: those shards are `continue-on-error`, so their failures never appear in `needs.*.result` | expected count present, 0 NULL, 0 failure, `nf-test` job actually ran, `latest-everything` fragment clean |
| V10 | No un-ignored file echoes the CLI | grep the content-hashed MultiQC bismark files (`multiqc_bismark_alignment.txt`, `multiqc_bismark_strand_alignment.txt`, `multiqc_bismark_mbias_*.txt`, `multiqc_bismark-methylation-dp.txt`) for `-p ` / `--multicore` | no matches — they carry parsed numeric fields only |
| V11 | `conf/` coupling intact | assert every path referenced by `conf/containers_conda_lock_files_*.config` exists after the import, and that no orphaned `.conda-lock/*` file survived (`git checkout -- <path>` does not delete) | all referenced paths exist; no orphans |
| V12 | Cross-engine agreement | PR CI runs `profile: [conda, docker, singularity] × NXF_VER: [25.04.0, latest-everything]` while step 9 runs docker-only | all engines agree. A conda-only or singularity-only failure is engine/version skew to investigate, **not** a licence to re-baseline |

**Coverage gaps, stated rather than papered over:**

- **No methylseq pipeline test exercises the classic (non-combined) `--pbat` or `--non_directional`
  path.** Those params appear only in `combined_index.nf.test`. So the non-directional row of §3 cannot
  be observed in methylseq CI at all — and at cpus=4 it would emit no `-p` even if it could. The
  evidence for that row is upstream's module-level test `bowtie2 | non_directional | low-cpu omits -p
  (4 cpus / 4 instances < 2)`, plus Phase-0's non-directional byte-identity measurement.
- **The production non-directional cell (`-p 3` × 4 instances) is exercised by no automated test in
  either repo.** It is the most-changed configuration in the matrix. Its evidence is Phase-0, which did
  cover non-directional Bowtie 2 three-way. **HISAT2 non-directional at production cpus is measured by
  nothing** — if the PR claims byte-identity, scope that claim to Bowtie 2 and to the library types
  Phase-0 actually measured.

## 10. Questions and ambiguities

**Open (assumption taken, worth a decision at implementation time):**

- **Where to run the tests.** Needs a real docker run of the pipeline. Assumption: oxy, which has the
  precedent (`q6_modules`) and the engines; mind the K8s-pod recycle rules (only `/home` persists,
  capture results off-box). A laptop run is likely impractical. Note V12: whatever box is used, PR CI
  is the cross-engine authority.
- **Whether to sync all seven modules or only `align`.** Assumption: all seven. Both reviewers agreed,
  and each added a reason: the six docs-only modules are excluded from CI triggering by `paths-ignore`
  so they cost nothing in review surface, and an `align`-only repin would leave six modules pinned to a
  sha that is no longer newest for their paths — the very state that made this sync necessary.
- **Whether to add V4's permanent in-test CLI gate** in this PR or a follow-up. Assumption: this PR;
  it costs a couple of lines and converts the weakest gate into a CI-enforced one.
- **Timing.** No in-flight release; #598's `modules.json` hunk does not overlap. Assumption: open it
  when the work is done.

**No critical ambiguities.** The one decision that could have been critical — accepting a HISAT2
re-baseline — has been **dissolved rather than deferred**: per §2.5 no re-baseline is expected, so
there is nothing to accept. Should HISAT2 output move anyway, that is a finding to investigate and
report, not a scheduled step; the prior scope decision (Q6 widened from Bowtie 2 to Bowtie 2 + HISAT2
on 2026-07-18, on the grounds that HISAT2 is already cpus-dependent today via `--multicore = cpus/n`)
remains the backstop, and is not reopened here.

## 11. Self-review

### 11.1 rev-1 corrections (from dual review, each verified before folding)

- **The cpus=2 premise was false and it inverted the plan's central prediction.** Both reviewers caught
  it. Root cause: a memory of the #12385 work recorded "CI default is cpus=2 not 4", which was true when
  written and went stale when #12402 bumped it on 2026-07-23. The lesson is not "check the config" but
  **a recorded fact about someone else's repo has a shelf life** — step 2 now re-asks the cpus question
  explicitly, and the memory has been corrected with the date and the PR that moved it.
- **Reviewer A and B contradicted each other on the mechanism; B was right.** A claimed the upstream
  snapshots capture no reads MD5 and attributed count 5047 to HISAT2, concluding the evidence was
  count-level. Direct check: the `hisat2 | single-end` entry's first element is a reads MD5
  (`936c0d5c…`), and 5047 belongs to the **minimap2** test — HISAT2 asserts 5009. Taking A at face value
  would have kept a weaker argument than the facts support. Both reports were treated as claims to
  verify, not conclusions to adopt.
- **§2.1's delta table lost a changed file.** `tests/nextflow.config` changed (`3313dfa884` →
  `6bdc178d47`); #12385 touched three files, not two. rev-0's §11 said the file "predates #12385" —
  true and irrelevant, since it also *changed* in it. Both errors, rev-0's and its correction, came from
  **hand-enumerating a file list**; step 2 now derives it from the commit API.
- **§2.4's conclusion was right for the wrong reason.** The bismark report *does* record the CLI —
  upstream asserts on exactly that. The guarantee is two `.nftignore` globs, which is a weaker and more
  fragile claim than "nothing captures it", and it left the content-hashed MultiQC bismark files
  unchecked (now V10).
- **The blast radius was under-enumerated by an order of magnitude** — 67 content hashes per hisat
  scenario, including every methylation call, not "reads MD5 plus a couple of MultiQC files". rev-0's V5
  would have been violated by a correct run.
- **Four vacuous-pass channels closed:** `confirm-pass` reports success when the `nf-test` job is
  *skipped*; `latest-everything` failures never reach `needs.*.result`; V3's "no `--multicore` anywhere"
  was trivially true over an empty glob; and V3's original locus (`.nf-test/**/.command.sh`) does not
  survive the CI action's work-dir cleanup. V3/V9 now assert positives with counts first.
- **A per-file `--update-snapshot` would have swept four must-not-move scenarios** out of
  `index_downloads.nf.test.snap`. That was the one path by which a genuine Bowtie 2 regression could
  have landed as routine churn — the exact outcome §3.1's rule exists to prevent.
- **Factual fixes:** open PR #598 *does* touch `modules.json` (subworkflows section, no overlap — the
  conclusion survives on different grounds); `combined_index.nf.test` has 5 tests, not 4; the `align`
  module has 8 files, not 6; §5's memory formula needed its non-directional variant.
- **Additions both reviewers wanted:** the `.conda-lock`/container-digest coupling in `conf/` (V11,
  step 6), sha-addressed fetching, the combined-index narrowness trap, the conserved thread budget, the
  no-crash finding, and the full enclosing guard in §2.2's quote.

### 11.2 What rev-0 got right and rev-1 keeps

Both reviewers verified, independently and exactly: every module and blob sha; the seven-module pin;
the patch registration and its single-file scope; the `-p` arithmetic in **every** cell of §3, including
the memory term rev-0 did not show; `bismark_align.config` emitting no thread flag; the resource labels;
the `.nftignore`-versus-`getReadsMD5()` exposure (called "the plan's subtlest claim, and it is right");
the scenario counts; every citation into `rust/bismark/src/aligner/`; and the combined-index no-op
argument — including the `(?!_)`-versus-substring edge that would have broken it.

Both also endorsed the mechanics unchanged: import-by-sha over `nf-core modules update`, the explicit
`git rm` with its "checkout never deletes" reasoning, the `cmp`-against-clean-checkout faithfulness
proof, the confined diff, V1's positive control, and the "defect signal, not a re-baseline" polarity.

### 11.3 Remaining risks

1. **HISAT2 at production cpus (`-p 6`) is measured by nothing.** §2.5's evidence covers the 1→2 step on
   a small genome; the 4→6 step on real data is unmeasured. This is why step 10's changelog sentence
   names it.
2. **The production non-directional cell has no automated gate** in either repo (§9's coverage gaps).
3. Upstream can move before this lands, invalidating §2.1 and possibly §2.5's cpus basis — step 2.
4. Every gate needs a methylseq clone plus docker and network test data; none of it can run from this
   repo.
5. §2.4's command-line guarantee now rests on two named `.nftignore` globs, so a template merge that
   touches that file changes the analysis (assumption 10).

---

## 12. Implementation notes (2026-08-17)

Implemented in the existing clone at `~/Github/methylseq`, on branch
**`chore/bismark-modules-sync-12385`** based on `upstream/dev` = `9855ef9444cfb0111517b6f5f56bb4a0cd78ed7a`
(the fork's own `dev` was 32 commits behind, as the plan warned). Commit **`721ae33c`** —
15 files, +212/−92 — pushed to the fork and opened as
**[nf-core/methylseq#622](https://github.com/nf-core/methylseq/pull/622)** (base `dev`).

Scope confirmed against `compare/dev...721ae33c` rather than the PR object's counts: `ahead=1`,
`behind=0`, 15 files, +212/−92 — the two agree, and `modules.json` is 8/9 as intended.

**Step 2 re-verification, all clean:** target sha still latest for all seven paths; `#12385` touched
exactly three files and `#12581` exactly seven `meta.yml`s (derived from the commit API, as rev-1
requires); both `.conda-lock` filenames and the container digest `bismark:3.1.0--9557d6ab108a83e4`
identical across the range, so the `conf/` coupling is the predicted no-op; upstream's test `cpus` is
still 4.

### Gate results

| Gate | Result |
|---|---|
| V1 patch retired | ✅ all three sub-assertions, incl. the `rastair/mbiasparser` positive control and patched-count == 1 |
| V2 faithful import | ✅ **50** files compared, 0 differences, 0 missing, with a control proving `cmp` can fail |
| V4 permanent CLI gate | ✅ implemented in `tests/default.nf.test` + `tests/bismark_hisat_variants.nf.test` |
| V7 confined diff | ✅ only `modules/nf-core/bismark/**`, `modules.json`, `CHANGELOG.md`, 2 test files |
| V8 lint | ✅ `nf-core modules lint bismark/align`: **41 passed, 0 failed**, 3 pre-existing warnings |
| V9 full CI | ✅/⚠️ **complete: 100 check-runs, 95 pass, 5 fail, 0 pending, 0 NULL.** The vacuous-skip channel did not fire (15 shards × 3 profiles × 2 Nextflow versions materialised). `pre-commit` passed, so the local hook bypass cost nothing, and CI's `nf-core pipelines lint` **passed** (246 tests) — it uses the repo's pinned nf-core 4.0.3, not the local 3.5.2, and not `--release`. **No `latest-everything` failure fragment was posted**, so the `continue-on-error` blind spot is empty. The 5 failures are 2 root causes plus 3 aggregators — see below |
| V5 no snapshot moves | ⚠️ **held for alignment; one environment-specific exception.** `bismark_hisat with rrbs` passed under conda/25.04.0 with its reads MD5 intact, and every methylation output matched byte-for-byte everywhere. Two conda/25.04.0 shards failed on **HISAT2 index bytes only** — `BS_CT.1.ht2`, `BS_GA.1.ht2`, `BS_combined.1.ht2` — with `.2`–`.8`, the converted FASTAs and all splitting reports identical. Not attributable to this change: `bismark/genomepreparation`'s `main.nf` is byte-identical across the sha range, the `-p` model touches only `BISMARK_ALIGN`, and the same tests pass on docker/25.04.0, singularity/25.04.0 and all three `latest-everything` cells (the conda one verified at *step* level, since `continue-on-error` can mask a job conclusion). The "found" `BS_CT.1.ht2` hash is identical across both shards, so it is a consistent alternative value rather than run-to-run noise |
| V6 combined-index no-op | ✅ `combined_index.nf.test`'s loud-fail negative passed, and the combined-index scenarios pass on every engine except the conda/25.04.0 cell above, where only index bytes differ |
| V3 CLI matches §3 | ✅ confirmed locally on a real run: all four published alignment reports carried `-p 2 --reorder`, zero `--multicore`, over a non-zero file count |
| V10 MultiQC echo | ✅ **settled by inference from CI.** The un-ignored `multiqc_data/*.txt` bismark files are inside `stable_path`; the bowtie2 snapshots did not move even though the emitted CLI changed from no flag to `-p 2`, so those files provably do not echo the aligner command line |
| V11 `conf/` coupling | ✅ 14 referenced lock paths all exist, 0 orphans, digests match; both halves control-tested |
| V12 cross-engine | ⛔ needs PR CI (local run is docker-only, and on arm64 under amd64 emulation) |

### Deviations from the plan

1. **V2's expected count was wrong: 50, not 56.** The plan assumed 8 files per module uniformly.
   `align` alone has 8 — it is the only one with `tests/nextflow.config` — and the other six have 7.
   The counted assertion is what caught it; a bare "0 differences" would have hidden it.
2. **The prettier pre-commit hook was bypassed deliberately.** `prek`/pre-commit **re-stages** the
   hook's reflow, so the first commit silently carried a 178/62 `modules.json` instead of 8/9. Pristine
   `upstream/dev` is *already* prettier-dirty under the pinned prettier 3.8.3 (303 → 321 lines), so the
   reflow is pre-existing upstream drift and does not belong in this PR. Committed with `--no-verify`
   and recorded in the commit message.
3. **`nf-core pipelines lint` mutates the working tree.** It rewrote both `modules.json` and
   `ro-crate-metadata.json`; both reverted. Run lint *after* staging, and re-check `git diff` before
   committing.
4. **The recommended V4 gate needed a guard the plan did not anticipate.** Groovy's `every {}` returns
   **true on an empty list**, so `reports.every { it.text.contains('-p 2 --reorder') }` would pass when
   no report was found. Both assertions are preceded by `assert reports.size() > 0`.

### The conda `.1.ht2` mismatch, classified (re-run on Felix's instruction)

Re-ran the two failed conda shards (attempt 2 of run `32058839431`). Both failed again on the same two
tests, and the "found" hashes are **identical to attempt 1**:

| file | committed (expected) | conda produces |
|---|---|---|
| `BS_CT.1.ht2` | `78695d6728b6b218ec8aca2be312a531` | `9afaaa621c6c7c3efe66b4df917e86ea` |
| `BS_GA.1.ht2` | `f2fa3a9a…` | `6cbd0fd1…` |
| `BS_combined.1.ht2` | `2ae6ebf0…` | `20d6fa70…` |

**Verdict: deterministic under conda, not run-to-run instability, and pre-existing.** Evidence:

1. Two independent runs produced byte-identical results, so `hisat2-build` is reproducible *within* an
   engine.
2. The conda value `9afaaa62…` has **never been committed** to the repo (`git log -S` across all refs),
   while the expected `78695d67…` dates to `634119b5` (the bismark 3.0.0 modernisation) and survived
   the 3.1.0 bump plus two snapshot regenerations — i.e. the snapshots were generated on
   docker/singularity.
3. **#616 (merged 2026-07-24) ran 30 conda jobs with 0 failures**, so conda matched these hashes then.
   The drift therefore arrived between 2026-07-24 and 2026-08-17 — almost certainly a new bioconda
   `hisat2` build, since `bismark` itself still reports 3.1.0 in the conda run and only the `.1.ht2`
   files differ (`.2`–`.8`, the converted FASTAs and every methylation output match).
   *Residual gap:* "0 failures across 30 conda jobs" does not by itself prove those two scenarios were
   among the tests selected on that run.
4. This change cannot reach index generation: `bismark/genomepreparation`'s `main.nf` is byte-identical
   across the sha range, and the `-p` model applies only to `BISMARK_ALIGN`.

Consequence: any PR that selects these tests under conda will hit this until the snapshots are
regenerated under conda or the `hisat2` build is pinned. It is a methylseq snapshot-stability issue,
not a #622 defect.

⚠️ **Watcher-classifier bug worth remembering.** The re-run watcher reported *"DIFFERENT from attempt 1
— the index build is unstable run-to-run"*, which is the opposite of the truth. Its hash extraction had
returned **empty** (GitHub's job-log endpoint 404s for a few seconds after a job completes), and the
classifier treated "empty does not contain the attempt-1 hash" as "a different hash". A comparison
against an empty capture must fail loudly, never fall through to the else-branch. Same shape as N28,
committed *by the very script written to guard against it*.

### Environment findings (worth carrying forward)

- **A stale Seqera placeholder in the shell environment blocks every local Nextflow run.**
  `TOWER_WORKSPACE_ID=your-workspace-id-here` (and `TOWER_ACCESS_TOKEN=your-tower-token-here`) makes
  Nextflow die instantly with `ERROR ~ For input string: "your-workspace-id-here"`. Worked around with
  `env -u TOWER_WORKSPACE_ID -u TOWER_ACCESS_TOKEN -u TOWER_API_ENDPOINT`; the profile itself was left
  alone.
- **Local `nf-core` is 3.5.2 while `.nf-core.yml` asks for 4.0.3**, so local pipeline lint is not CI
  lint. All 13 `--release` failures are unrelated to this change: `4.3.0dev` not being a release
  version, template drift, three stale `nf_test_content` rules that contradict the actual
  `nf-test.config`, and **two merge-marker hits inside an untracked `plans/` leftover in that clone**.
- **This clone carries untracked `SESSION_HANDOFF.md` and `plans/`** — every `git add` used explicit
  paths, never `-A`.
- Docker here is colima on **aarch64**, so the amd64 container runs under emulation (verified working:
  it reports `bismark v3.1.0 … linux/x86_64`). A native arm64 image exists but needs the `arm64`+wave
  profile.
- Two shell traps re-confirmed: `for x in $var` does not word-split in this environment (it produced a
  false `MISS` in the first V11 run), and a guessed JSON key (`failed` instead of `tests_failed`)
  reported a false **0 failures** from the lint output.
