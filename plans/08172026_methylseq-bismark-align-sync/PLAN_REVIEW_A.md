# PLAN_REVIEW_A — Sync methylseq's vendored `bismark/*` onto the merged `-p` threading model

**Reviewer:** A (independent)
**Plan:** `plans/08172026_methylseq-bismark-align-sync/PLAN.md`
**Date:** 2026-08-17
**Verdict:** **REQUEST CHANGES**

The plan's substance is right and most of its concrete claims verify exactly. I am requesting changes
because **two validation gates can pass vacuously** (V9 via a skipped shard matrix, V3 via rows that
no test exercises) and **two factual premises are wrong** (a changed file missing from the §2.1 delta
table, and the "upstream tests run at cpus=2" claim). None of these break the approach; all are
fixable in the plan text before implementation.

---

## 0. What I verified independently, and what held

Everything below was checked against live GitHub (`gh api` REST) or the local repo, not taken from
the plan.

| Plan claim | Result |
|---|---|
| methylseq `dev` HEAD `9855ef94`, 2026-08-11, template merge 4.0.3 | ✅ `9855ef9444cfb0111517b6f5f56bb4a0cd78ed7a`, exact |
| all 7 `bismark/*` pinned at `7d264419ee98edd5a0886d3782fb87553628272b` | ✅ exact, all seven |
| `bismark/align` has a registered `"patch"` key; `rastair/mbiasparser` is the only other patched module | ✅ exact (42 modules total, exactly 2 with `patch`) — V1's positive control is valid |
| upstream latest for all 7 paths = `c565f1b27c06a1e8b726e2a646d4c822ae3b514a` | ✅ exact, all seven |
| `align` gained `1976cfb7f315` (#12385) **and** `c565f1b2` (#12581); other 6 gained `c565f1b2` only | ✅ exact |
| #12581 is `meta.yml`-only for all 7 | ✅ commit touches exactly 7 `meta.yml` files |
| #12385 merged | ✅ `merged=true`, merge commit `1976cfb7f315` |
| blob shas: `main.nf` `ff7af322`→`848dbab1`; `meta.yml` `1bd9fcd5`→`82bb0986`; `tests/main.nf.test` `b2fe71b6`→`17787b38`; `main.nf.test.snap` `b416113450` both; `environment.yml` `dc444907` both | ✅ all five exact |
| `tests/nextflow_pthreads.config` never existed at either sha | ✅ correct |
| `bismark-align.diff` touches only `main.nf` | ✅ the diff explicitly lists all 8 other component files as unchanged |
| patch ≡ upstream on the combined path for methylseq-emittable args | ✅ verified by reading both (see §1.1) |
| `conf/modules/bismark_align.config` emits no `--multicore`/`--parallel`/`-p`/`--minimap2`/`--rammap` | ✅ read in full; nothing matches, including no accidental `-p ` substring |
| `process_high` = 12 cpus / 72 GB; methylseq overrides only `time` for `BISMARK_ALIGN` (`8.d`) | ✅ exact |
| `resourceLimits = [cpus: 4, memory: '15.GB', time: '1.h']` in both `conf/test.config` and `tests/nextflow.config` | ✅ exact — and independently corroborated: CI runners are `4cpu-linux-x64` |
| `nf-test.config` ignores `modules/nf-core/**/tests/*` → vendored module tests never run | ✅ exact, and doubly so: the CI matrix also passes `--filter pipeline` |
| `getReadsMD5()` is not subject to `.nftignore` | ✅ **verified at the assertion**: `bam_files = getAllFilesFromDir(outputDir, include: ['**/*.bam'])` is a separate call with no `ignoreFile:` |
| nothing captures the bismark command line | ✅ `*/alignments/logs/*.txt` is in `.nftignore`, and `stable_name` is a name list |
| some `multiqc_data/*.txt` are un-ignored | ✅ `.nftignore` names 5 specific ones; `multiqc_bismark_*` are not among them |
| §3.1 scenario counts: hisat_variants 3, bismark_variants 14, index_downloads has `bismark_hisat with hisat2-index` | ✅ 3, 14, and yes |
| `targeted_sequencing_variants` bismark rows are bowtie2-only (no hisat) | ✅ both bismark scenarios use `aligner: "bismark"` |
| no open methylseq PR touches `modules.json`; oldest activity 2025-08 | ✅ 5 open PRs (#598, #593, #560, #379, #295), none in this area; oldest 2025-08-10 |
| no in-flight release | ✅ latest release 4.2.0 (2025-12-12); `dev` is `4.3.0dev` |
| §3 arithmetic, every row | ✅ recomputed from both `main.nf` versions — see §1.2 |
| `nf-core pipelines lint` cross-checks vendored files, runs in CI | ✅ `linting.yml` runs it on `pull_request`, plus `--release` |
| `options.rs:158-169` says `-p` always ships with `--reorder` | ✅ block at 158, `opts.push("-p {p}")` + `opts.push("--reorder")` at 168-169 |
| `mod.rs:154-161` HISAT2 thread-dependence notice, quoted verbatim | ✅ exact, including "it is NOT identical to single-core HISAT2" |
| `config.rs` performs the HISAT2 `--multicore`→`-p` remap | ✅ `hisat2_multicore_threads` (557-561), wired at 798 and 1030 |
| benchmark figures wall −38…−46 %, PSS −44…−82 % | consistent with the recorded Q6 Phase-0 result; not independently re-measurable here |

That is an unusually high hit rate. The corrections below are narrow.

### 0.1 One bonus verification the plan didn't claim but needed

`.conda-lock/` tree blob is `b1732a24e2` at **both** shas, and the two lock filenames
(`linux_amd64-bd-9557d6ab108a83e4_1.txt`, `linux_arm64-bd-f83bc6617fa3cadd_1.txt`) are identical at
both refs and match what methylseq has vendored. This matters — see Important #7.

---

## 1. Logic review

### 1.1 The combined-index no-op claim is sound (and I tightened its scope)

I read both sides. The patch:

```groovy
if(args =~ /--combined_index(?!_)/){
    if(task.cpus && (task.cpus as int) >= 2 && !(args =~ /(?:^|\s)-p\s/)){ args += " -p ${task.cpus}" }
}
else if(!args.contains('--multicore') && task.cpus){ …old ccore… }
```

Upstream `@c565f1b2` (the plan quotes the inner block correctly but omits the enclosing guard):

```groovy
else if (!args.contains('--multicore') && !args.contains('--parallel') && !args.contains('-p ') && task.cpus) {
    def n_instances = 2
    if (args.contains('--combined_index')) {
        n_instances = args.contains('--non_directional') && args.contains('--combined_index_parallel') ? 2 : 1
    }
    else if (args.contains('--non_directional')) { n_instances = 4 }
    def pthreads = ((task.cpus as int) / n_instances) as int
    if (pthreads >= 2) { args += " -p ${pthreads}" }
}
```

The equivalence holds for every methylseq-emittable argument set. I checked the case that would have
broken it: methylseq emits **both** `--combined_index` **and** `--combined_index_sequential` for
non-directional combined runs (`bismark_align.config` lines 15-16 are two separate list entries). Had
it emitted only `--combined_index_sequential`, the patch's `(?!_)` lookahead would **not** have fired
while upstream's substring `contains('--combined_index')` **would**, and the combined non-directional
row would have silently flipped from an old `--multicore` value to `-p cpus` — with the additional
consequence that `--combined_index` + `--multicore` is a hard error in bismark
(`rust/bismark/src/aligner/config.rs:1250-1252`), so at 12 cpus that row would fail loudly today.
It does not, because both flags are emitted. **The plan's conclusion is right; it just doesn't say
that the `(?!_)`-vs-substring asymmetry is load-free only because both flags travel together.** Worth
one line in §7 as a latent-divergence note (Optional #13).

### 1.2 The §3 matrix arithmetic is correct in every cell

Recomputed from the two `main.nf` versions plus the verified resource facts (`process_high` 12 cpus /
72 GB; CI capped to 4 cpus / 15 GB):

- **Today**, directional @12: `ccore = min(12/3, 72GB/13GB) = min(4,5) = 4` → `--multicore 4` ✓
- **Today**, non-directional @12: `cpu_per_multicore=5`, `mem_per_multicore=18GB` →
  `min(12/5, 72/18) = min(2,4) = 2` → `--multicore 2` ✓
- **Today** @4 (CI): directional `min(1, 1) = 1`, not `> 1` → no flag ✓ (note the memory limit
  15 GB/13 GB = 1 pins it to 1 independently of cpus — belt and braces for the plan's claim)
- **After**: directional @4 → `4/2 = 2` → `-p 2` ✓; non-directional @4 → `4/4 = 1 < 2` → no flag ✓;
  directional @12 → `-p 6` ✓; non-directional @12 → `-p 3` ✓; combined (n=1) → `-p 4` / `-p 12` ✓
- **HISAT2** @12 today: `--multicore 4`, remapped by bismark to `-p 4 --reorder` ✓

### 1.3 Gap: §3's consequence 3 understates the HISAT2 change — production shifts too

§3 frames HISAT2 movement as a CI artefact ("In CI, HISAT2 goes from no threading flag to `-p 2`").
But at `process_high` the production HISAT2 path goes from `-p 4` (via the `--multicore 4` remap) to
`-p 6`. By the plan's own cited notice, that is a **different result for real `--aligner
bismark_hisat` users**, not just a CI re-baseline. The plan's §10 defence ("HISAT2 output is already
cpus-dependent today") is a good reason not to reopen the decision, but it is not a reason to leave
the effect out of the changelog. See Important #10.

### 1.4 Vacuous-pass hunt — two live channels found

**Channel A (Critical #1): `confirm-pass` is green with zero tests executed.**

`.github/workflows/nf-test.yml` → `get-shards` runs
`nf-test test --dry-run --changed-since HEAD^ --filter pipeline --tag cpu`, and sets
`total_shards=0` when it sees `Nothing to do` or cannot parse `Executed N tests`. The matrix job is
gated `if: needs.get-shards.outputs.total_shards > 0 && …`. Then:

```yaml
confirm-pass:
  needs: [nf-test]
  if: always()
  steps:
    - if: ${{ contains(needs.*.result, 'failure') }}   → exit 1
    - if: ${{ contains(needs.*.result, 'cancelled') }} → exit 1
    - if: ${{ contains(needs.*.result, 'success') }}   → exit 0
```

With `nf-test` **skipped**, `needs.*.result == ['skipped']`: no step fires, nothing exits non-zero,
and `confirm-pass` reports **success**. V9 as written — "green; enumerate check-runs and assert zero
NULL conclusions" — passes on this. So does a human glance at the checks list. The fix is a positive
assertion, not a negative one (details in Critical #1).

A second, milder edge on the same workflow: `continue-on-error: ${{ matrix.NXF_VER ==
'latest-everything' }}` means `latest-everything` failures never fail CI and never appear in
`needs.*.result`; they surface only as a PR comment fragment. For a PR whose *whole point* is a
re-baselined snapshot, a `latest-everything` failure is exactly the signal you want (it would mean the
new hash is Nextflow-version-sensitive), and V9's "green" will not show it.

**Channel B (Critical #2): V3 asserts on rows no methylseq test can produce.**

I grepped all nine pipeline test files for `pbat|non_directional|zymo|single_cell|local_alignment`.
The **only** hits are inside `combined_index.nf.test`. There is **no methylseq pipeline test that
exercises the classic (non-combined) `--pbat` or `--non_directional` path at all.** So V3's
expectation list —

> `-p 2` directional, `-p 4` combined, no `-p` non-directional, **no `--multicore` anywhere**

— has an unobservable third clause. An implementer greps for `-p` in a non-directional run's
`.command.sh`, finds no such run, gets an empty result, and records "no `-p` — as expected". That is
precisely the empty-search-reads-as-success failure mode. The fourth clause ("no `--multicore`
anywhere") is fine and is the strongest single assertion in the whole plan — keep it, and make it a
repo-wide `grep -c` with an explicit expected count of 0 **plus** a non-zero count of files searched.

### 1.5 Gap: `index_downloads.nf.test.snap` holds four scenarios that must not move

The plan's §3.1 correctly says only the `bismark_hisat with hisat2-index` scenario moves within that
file. But the file has **five** scenarios: `default_with_bowtie2_index`,
`bismark_run_preseq_with_bowtie2_index`, `bismark_run_methurator_with_bowtie2_index`,
`bwameth_with_bwameth_index`, and `bismark_hisat_with_hisat2_index`. `nf-test --update-snapshot` is
per-file, so it re-baselines all five. V5's assertion ("only reads-MD5 values and un-ignored bismark
multiqc stats change") is file-scoped and would be satisfied by a diff that also silently moved a
bowtie2 scenario's reads MD5 — the very thing §3.1 calls a defect signal. V5 needs per-scenario
confinement (Important #5).

### 1.6 Two small counting slips

- `combined_index.nf.test` has **5** tests, not 4: the four scenarios plus a loud-fail negative
  ("combined_index with a pre-built index lacking the combined reference fails loudly"). V6 should
  name it, because it is a genuinely valuable guard here: it proves the `-p` rewrite didn't convert a
  loud combined-index failure into a silent fall-through to the classic path.
- §5's memory-guard formula `ccore = min(cpus/3, memory/13GB)` covers only the directional case; the
  old code switches to `cpus/5` and `18GB` under `--non_directional`. §3's table gets this right, so
  it's a §5 wording slip.

### 1.7 Steps 1-11 are otherwise internally consistent

Step ordering is sound. Step 4's observation that `git checkout <sha> -- <path>` adds and overwrites
but never deletes is correct and is the right reason for the explicit `git rm`. Step 7's `cmp`-against-
clean-checkout is the correct proof that the patch is gone rather than half-applied, and it is stronger
than trusting `nf-core pipelines lint`. Step 10's "do not run `nextflow lint -format`" is the right
carry-over.

---

## 2. Assumptions

### 2.1 Stated assumptions 1-8: seven hold, one is wrong

Assumptions 1, 2, 3, 4 (as a recorded prior result), 5, 6, 8 all verified — see §0.

**Assumption 7 is wrong as written.** "The six non-`align` bismark modules differ from the pinned sha
only in `meta.yml`" is true. But the companion claim in §2.1 and §11 — that `align`'s delta is
`main.nf` + `meta.yml` + `tests/main.nf.test` — omits `tests/nextflow.config`:

```
tests/nextflow.config   3313dfa884  →  6bdc178d47      CHANGED (by #12385)
```

`1976cfb7f315` touched three files, not two: `main.nf`, `tests/main.nf.test`, **and
`tests/nextflow.config`** (it added a `params.containsKey('genomeprep_args')` fallback so the new
tests can give genome-prep a different arg string). §11 states the opposite outright: *"the module's
test config is `tests/nextflow.config`, and it predates #12385."*

The irony is instructive. §11's self-review caught a false-`SAME` for a file that exists at neither
ref — and then missed the file that exists at both and genuinely changed. Both failures have the same
root cause: a **hand-enumerated file list**. Derive the delta from the commits instead (Important #3).

Practical impact is small (the file lives under `tests/`, which methylseq's nf-test ignores, and step
3's directory-wide checkout imports it correctly either way) — but §2.1 is the table a reviewer will
read to decide what changed, and it is wrong.

### 2.2 An unstated assumption that is false: "upstream's own tests run at cpus=2"

§2.1 and §10 both rest on this. `nf-core/modules` `tests/config/nf-test.config` sets:

```groovy
process { cpus = 4; memory = '15.GB'; time = '2.h' }
```

**Upstream module tests run at cpus = 4 — the same as methylseq CI.** So the plan's closing argument
in §10 ("it *does* bite harder in methylseq than upstream: upstream's own module tests run at cpus=2,
where `-p` is omitted and nothing moved, whereas methylseq runs at cpus=4") is wrong on both halves.

The real reason upstream's `main.nf.test.snap` is byte-identical at both shas:

1. The four `-p` tests #12385 added are **assertion-only** — their `then` blocks contain
   `assert process.success` plus a `file(process.out.report[0][1]).text.contains(…)`, and no
   `snapshot(...).match()`. The snap has exactly five keys, all pre-existing:
   `bowtie2 single-end`, `bowtie2 paired-end`, `hisat2 single-end`, `hisat2 paired-end`,
   `minimap2 single-end`.
2. The five pre-existing snapshot tests **did** change CLI at cpus=4 (from no flag — old model:
   `min(4/3, 15GB/13GB) = 1`, not `> 1` — to `-p 2 --reorder`), and their snapshots still didn't
   move, because what they capture is a *boolean*
   (`report.readLines().contains("Number of alignments with a unique best hit from the different
   alignments:\t5047")`), plus `process.out.unmapped` and versions. **No reads MD5.**

So: same thread count, different snapshot sensitivity. That is the correct framing, and it should
replace §10's paragraph.

**This correction is good news for the plan's HISAT2 prediction and should be harvested.** Upstream's
`hisat2 | single-end` and `hisat2 | paired-end` snapshot tests went from no threading flag to
`-p 2 --reorder` on the sarscov2 test genome, and the unique-best-hit count stayed at exactly 5047 in
both. That is real evidence — count-level rather than MD5-level, so not proof — that HISAT2's output
on a genome this small is insensitive to the 1→2 thread step. §10's "recorded for honesty" paragraph
currently offers no evidence at all; this is strictly better than nothing and points the same way.

### 2.3 An unstated assumption worth surfacing: `--changed-since HEAD^` selects the bismark tests

Both the shard-count step and the test run use `--changed-since HEAD^` (on a `pull_request` merge
commit, `HEAD^` is the base tip, so this is the whole-PR diff). The plan's §9 note is right to be
cautious. Two mechanics are worth writing down:

- `nf-test.config`'s `triggers` list (which forces a full run) contains `nextflow.config`,
  `conf/test.config`, `tests/.nftignore`, `tests/nextflow.config`, `bin/*` … but **not** `modules.json`
  and **not** `modules/**`. So this PR does not hit the full-run escape hatch and depends entirely on
  nf-test's dependency-graph resolution from `modules/nf-core/bismark/align/main.nf` up through
  `subworkflows/nf-core/fastq_align_dedup_bismark` to `../main.nf`.
- Every pipeline test shares `script "../main.nf"`, so if resolution works at all it should select
  effectively every `cpu`-tagged pipeline test (including bwameth ones). If it does not work, it
  selects only the two `.snap`-adjacent files — or nothing. The plan should assert which happened
  rather than infer it.

### 2.4 Configurable assumptions 9-10 are the right two, but incomplete

Step 2's re-verification list ("what the target sha is and which modules changed behaviourally vs
docs-only") is missing a third question that has a `conf/` blast radius — see Important #7.

---

## 3. Efficiency analysis

Nothing here is hot-path, and the plan says so. The claimed mechanism is correct and I can confirm
the code-level half of it: one instance per strand with internal threads replaces N forked chunks each
re-loading the index, and `-p` never ships without `--reorder`
(`rust/bismark/src/aligner/options.rs:158-169`), which is what makes the Bowtie 2 output
thread-invariant.

§5's reasoning that dropping the memory guard is safe *because* the new knob does not scale memory is
the right shape of argument — it names the condition under which it would have been a regression. Two
notes:

- The formula should mention the non-directional variant (§1.6).
- The new model has **no memory guard at all**, so on a machine with plenty of cores and little RAM
  (`process_high` = 12 cpus but a site config capping memory), `-p 6` will still be emitted where the
  old model would have throttled to `--multicore 1`. That is fine — one index load is the floor
  either way, and it is *strictly less* memory than the old model at the same cpus — but it means the
  new model's peak is set by a quantity nothing in the config observes. Worth one line, because it is
  the question a reviewer will ask.
- Cost of the validation itself is the real efficiency item: the plan mandates re-running four
  bismark-touching test files without update plus two with, on a box with docker and network test
  data. §10 flags this. No optimisation suggested — it's irreducible.

---

## 4. Validation sufficiency

V1, V2, V6, V7, V8 are well-designed. V1's positive control (assert `rastair/mbiasparser` still has
its `patch` key) is exactly the right guard against a JSON edit that deletes too much, and I confirmed
the control exists. V2 is the strongest gate in the plan. V8 is CI-enforced (`linting.yml` runs
`nf-core pipelines lint` and `--release` on every PR), so it cannot be skipped.

The gaps, in order of severity:

1. **V9 can pass with zero tests run** (§1.4 Channel A).
2. **V3 has an unobservable clause** (§1.4 Channel B).
3. **V5 is file-scoped where it needs to be scenario-scoped** (§1.5).
4. **V3's locus is fragile.** `.nf-test/**/.command.sh` depends on work dirs surviving the run
   (CI sets `NFT_WORKDIR: "~"`, and the action's cleanup step does `rm -rf /home/ubuntu/tests/`), and
   a `grep` that matches nothing is indistinguishable from a `grep` that found no files. The bismark
   **report** carries the aligner command line — that is exactly what upstream's own module tests
   assert on — and methylseq publishes it to `${outdir}/${aligner}/alignments/logs/*.txt`. It is a
   published output, always present, and `.nftignore`d for hashing so reading it costs nothing. Assert
   there, with a file count (see Important #9).
5. **No cross-engine gate.** CI runs `profile: [conda, docker, singularity] × NXF_VER: [25.04.0,
   latest-everything]`. A HISAT2 reads MD5 regenerated on oxy with docker must also hold under conda
   and singularity. Today's green snapshots prove the three engines agree *now*, so the risk is
   modest, but the plan should name PR CI as the gate and pre-commit to reading a conda-only failure
   as version skew, not as a re-baseline trigger.
6. **No orphan check after the import** (Important #7).

One thing the plan gets right that is easy to get wrong: V4's framing ("A failure is a defect signal
— investigate, do not re-baseline") and §3.1's "must not move" rows. That is the correct polarity and
it is stated three times. Keep it.

---

## 5. Alternatives

1. **Sync only `bismark/align`** (§10's second open question). The plan's reasoning for all-seven —
   uniform sha, other six are docs-only, keeps lint quiet — is sound and I'd keep it. One extra point
   in its favour the plan doesn't make: `nf-test.yml` has `paths-ignore: ["**/meta.yml", "**/*.md",
   "docs/**", …]`, so the six docs-only modules contribute nothing that could trigger or suppress CI.
   The choice is cosmetic; all-seven is the better default.
2. **Keep the patch and only repin.** Rejected correctly — the patch's `else if` is what suppresses
   the `-p` model on the classic path, so keeping it would import the change and disable it. Worth
   stating that explicitly in §2.2 as the reason the retirement is not optional: it is not a cleanup,
   it is the enabling step.
3. **Pin `-p` in `conf/modules/bismark_align.config` instead of taking upstream's model.** Would keep
   methylseq's diff to one config file and avoid a vendored-module sync. Rejected implicitly, and
   rightly: it re-introduces divergence from upstream (the thing #12385 exists to remove) and
   upstream's `!args.contains('-p ')` guard means a config-emitted `-p` would silently suppress the
   module's own arithmetic — a footgun for the next person.
4. **Land the sync and the HISAT2 re-baseline as two PRs.** Not considered. It is genuinely
   attractive here: PR 1 syncs and proves the Bowtie 2 rows don't move (all "must not move" gates
   green, no `--update-snapshot` anywhere), PR 2 does nothing but re-baseline the two HISAT2 files.
   The reviewer of PR 2 then sees a diff that is *only* snapshot churn, with the CLI change already
   merged and green. Against it: PR 1's CI would be red on the two HISAT2 files, so it can't merge as
   a unit — which kills the idea. Worth one line in §6 recording that it was considered and why the
   red-CI-in-between makes it impossible, because a reviewer will ask.
5. **Wait for the `-p` → `--threads` long-name switch** (mashehu's #12385 ask). §6 excludes it for the
   right reason (the shipped `bismark:3.1.0` container has no `--threads`). Keep the exclusion and
   keep the sentence — it pre-empts the obvious review question.

---

## 6. Action items

### Critical

1. **Close the vacuous-green channel in V9.** Replace "green; enumerate check-runs and assert zero
   NULL conclusions" with positive assertions:
   - from the `get-shards` job log, assert `total_shards > 0` and record the dry-run's
     `Executed N tests` value;
   - assert the `nf-test` matrix job **ran** (not `skipped`) — `confirm-pass` reports success when
     `nf-test` is skipped, because none of its three conditional steps fires on `'skipped'`;
   - name the bismark scenarios you require to appear as `ok` in the TAP output / step summary
     (at minimum: the 14 `bismark_variants`, both `default`, all 5 `combined_index`, all 3
     `bismark_hisat_variants`, `bismark_hisat with hisat2-index`);
   - note that `latest-everything` shards are `continue-on-error`, so a failure there shows up only
     as a PR comment fragment — and that for a re-baselining PR that fragment must be read.

2. **Fix V3's unobservable clause.** No methylseq pipeline test exercises the classic `--pbat` or
   classic `--non_directional` path (only `combined_index.nf.test` uses those params). Split V3 into
   what is observable — directional bowtie2 `-p 2`, hisat2 `-p 2`, combined `-p 4`, and a repo-wide
   `--multicore` count of exactly 0 **with a non-zero count of files searched** — and record the
   non-directional/pbat classic rows as an accepted coverage gap, citing upstream's module-level
   `bowtie2 | non_directional | low-cpu omits -p (4 cpus / 4 instances < 2)` test as the evidence for
   that row instead.

### Important

3. **Correct the §2.1 delta table and §11.** `tests/nextflow.config` changed
   (`3313dfa884` → `6bdc178d47`, by #12385, which touched three files not two). §11's *"it predates
   #12385"* is false. Drop the `tests/nextflow_pthreads.config` row (a file that exists at neither
   ref) and derive the delta mechanically:
   `gh api repos/nf-core/modules/commits/<sha> --jq '.files[].filename'`, unioned over the range —
   a hand-enumerated list is what produced both errors.

4. **Correct the cpus=2 premise in §2.1 and §10.** `nf-core/modules` `tests/config/nf-test.config`
   sets `process { cpus = 4; memory = '15.GB' }` — upstream ran at the *same* cpus as methylseq. Replace
   §10's "bites harder in methylseq" paragraph with the actual explanation: the four new `-p` tests are
   assertion-only (the snap has exactly 5 keys, all pre-existing) and the pre-existing snapshots capture
   a report boolean + `unmapped` + versions, not a reads MD5. Then harvest the upside for §10's HISAT2
   prediction: upstream's `hisat2` SE and PE tests **did** transition to `-p 2 --reorder` at cpus=4 and
   kept the same unique-best-hit count (5047) on sarscov2 — count-level, not MD5-level, evidence, but
   evidence, where §10 currently has none.

5. **Make V5 scenario-scoped.** `index_downloads.nf.test.snap` holds 5 scenarios and
   `--update-snapshot` is per-file. Require: run the file once *without* update and confirm the
   failure list names only `bismark_hisat with hisat2-index`; after update, assert the four other
   scenario blocks (`default_with_bowtie2_index`, `bismark_run_preseq_with_bowtie2_index`,
   `bismark_run_methurator_with_bowtie2_index`, `bwameth_with_bwameth_index`) are byte-identical.

6. **Add a cross-engine clause.** The re-baselined HISAT2 reads MD5 must hold under conda,
   docker *and* singularity (the CI matrix), while §10 plans a docker-only run on oxy. Name PR CI as
   the gate and pre-commit to treating a conda-only or singularity-only failure as engine/version
   skew to investigate, not as a second re-baseline.

7. **Add a `conf/` coupling check to step 2 and a new validation row.**
   `conf/containers_conda_lock_files_{amd64,arm64}.config` hard-code the vendored lock-file paths by
   exact name (`modules/nf-core/bismark/align/.conda-lock/linux_amd64-bd-9557d6ab108a83e4_1.txt`,
   `…linux_arm64-bd-f83bc6617fa3cadd_1.txt`), and `conf/containers_{docker,singularity_*}_*.config`
   hard-code container digests. This is a no-op for `7d264419 → c565f1b2` — I verified the
   `.conda-lock` tree blob is `b1732a24e2` at both shas and both filenames are unchanged — which is
   the *only* reason step 6's "nothing under `conf/`" is correct. Add to step 2: "did any
   `.conda-lock` filename or container digest change between the shas?", and add a validation row:
   after the import, assert every path referenced by `conf/containers_conda_lock_files_*.config`
   exists, and that no orphaned `.conda-lock/*` file survived (`git checkout -- <path>` does not
   delete). I could find no `includeConfig` referencing these files, so they appear to be `-c`
   opt-in — meaning a stale reference would **not** be caught by CI and would bite only users.

8. **Make step 3's fetch sha-addressed.** `git fetch nf-core-modules --depth=50 master` is
   traffic-dependent: master is currently 21 commits ahead of `c565f1b2`, so it works today, but a
   week's drift breaks it. Use `git fetch nf-core-modules <target-sha> --depth=1` — GitHub supports
   fetch-by-sha and it is independent of upstream volume. (Fails loudly either way, so this is
   friction, not risk.)

9. **Move V3's locus to the published bismark report.** `${outdir}/${aligner}/alignments/logs/*.txt`
   contains the aligner command line — exactly what upstream's module tests assert on
   (`file(process.out.report[0][1]).text.contains('-p 2 --reorder')`). It survives work-dir cleanup
   (the CI action `rm -rf`s the nf-test tree), and pairing the grep with a report-file count makes
   "no match" distinguishable from "no files".

10. **Have the CHANGELOG sentence name the HISAT2 output shift.** Production `--aligner
    bismark_hisat` goes from `-p 4` (via the `--multicore 4` remap) to `-p 6` at `process_high`, so
    results change for real users, not just in CI. One sentence carries it, and per CLAUDE.md a
    changelog is read by someone deciding whether an upgrade affects them.

### Optional

11. §5's memory-guard formula should note the non-directional variant (`cpus/5`, `mem/18GB`), which
    is what produced the `--multicore 2` cell §3 already has right.
12. Consider trimming the existing unreleased 4.3.0dev combined-index entry, which says combined
    index is "parallelised with Bowtie 2/HISAT2 `-p` instead of `--multicore`" — after this change
    that is true of every mode, so the clause now reads as a combined-index-specific feature when it
    is no longer one. Same unreleased section, one clause.
13. Add a §7 latent-divergence note: the patch matched `--combined_index(?!_)` (bare only) while
    upstream matches `contains('--combined_index')` (also `_sequential`). They agree only because
    `bismark_align.config` emits both flags together; a future config that emitted only
    `--combined_index_sequential` would diverge — and, since bismark rejects
    `--combined_index` + `--multicore`, would fail loudly at 12 cpus.
14. V6 should name the fifth test in `combined_index.nf.test` (the "fails loudly" negative). It is a
    useful guard: it proves the rewrite didn't turn a loud combined-index failure into a silent
    fall-through to the classic path.
15. Step 9 should name the changelog convention concretely — emoji-prefixed bullet under
    `### Pipeline Updates`, `([#NNN](url))` suffix — since "matching the file's existing format" is
    ambiguous in a file whose existing entries are multi-sentence.
16. Record in §6 that a two-PR split (sync first, re-baseline second) was considered and is
    impossible because PR 1 would have to merge with two red test files.

---

## 7. Verdict

**REQUEST CHANGES.**

The approach is correct, the diagnosis of the patch as *enabling* rather than *cleanup* is the key
insight, and the combined-index no-op claim survives adversarial checking — including the specific
`(?!_)`-vs-substring case that would have broken it. Blob shas, module shas, resource arithmetic,
`.nftignore`-vs-`getReadsMD5()` exposure, snapshot classification, and every local Bismark source
citation all verified exactly.

Two things must change before implementation, both about gates rather than design: **V9 currently
accepts a CI run in which nothing executed**, and **V3 asserts on a scenario methylseq cannot
produce**. Two factual premises must also be corrected — the `tests/nextflow.config` omission from
the §2.1 delta table, and the cpus=2 claim about upstream's own tests (which, corrected, actually
*strengthens* the plan's weakest prediction). With items 1-10 folded in, this is ready to implement.
