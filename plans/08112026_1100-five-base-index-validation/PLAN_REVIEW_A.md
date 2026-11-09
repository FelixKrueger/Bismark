# PLAN_REVIEW_A — #1100 `--five_base_index` resolve-time validation

**Reviewer:** A (independent; no shared state with Reviewer B)
**Target:** `plans/08112026_1100-five-base-index-validation/PLAN.md` rev 1
**Date:** 2026-08-11
**Verdict:** **APPROVE WITH CHANGES.** The plan's central technical claim (§2) is correct — I re-derived
it from scratch and reproduced every row of its table, including the load-bearing `./puc` row. The
design is sound and the hard-error decision (D1) is better supported than the plan itself argues,
because bowtie2 genuinely requires all six index files (measured below), so the probe is not stricter
than the aligner. But there are **two blocking gaps**: the plan breaks a currently-green test and does
not say so, and Task 2's placement instruction is loose enough to silently void #1099's regression
guard.

Everything below was verified against source or measured. Scratch dir:
`$TMPDIR/rev1100A_73898_1786431654` (unique per session, per N7).

---

## 0. Verification log — what I checked, and what it returned

| # | Plan claim | Method | Result |
|---|---|---|---|
| V1 | bowtie2 joins `$BOWTIE2_INDEXES` with the basename **as supplied**, so `./puc` is rescued | Read `/opt/homebrew/bin/bowtie2:379-398`; built a real pUC19 index with `bowtie2-build`; ran all 6 table rows from an unrelated cwd | **CONFIRMED, all six rows exactly.** `-x ./puc` + env → rc 0. The separator heuristic really would have rejected a working setup |
| V2 | The separator heuristic (the issue's own suggestion) is wrong | Follows from V1 | **CONFIRMED.** §2 is the plan's strongest section |
| V3 | A single small-index probe would reject a large index | `bowtie2-build --large-index`, then plain `-x` | **CONFIRMED** — rc 0 against a `.bt2l`-only index. The two-arm probe is required |
| V4 | *Unstated by the plan:* does bowtie2 actually need all 6 files? | Deleted each of the 6 in turn, re-ran | **All 6 required.** Missing `.1` → "Could not locate a Bowtie index"; missing `.2`/`.rev.1`/`.rev.2` → **SIGSEGV**; `.3`/`.4` → "Could not open reference-string index file". This materially strengthens D1 |
| V5 | Check placed before `detect_aligner` is assertable in CI | `awk 'NR>=637&&NR<=880'` over `resolve()`, grepped for any `Command`/exec | **CONFIRMED** — `detect_aligner` (`config.rs:880`) is the first and only exec in `resolve()` |
| V6 | The guards guarantee `five_base_index.is_some()` when aligner ∈ {Bowtie2, Hisat2} and `illumina_5base` | Read `resolve_aligner` `config.rs:1262-1300` | **CONFIRMED.** `illumina_5base` + no engine flag → `Minimap2`; + `--bowtie2`/`--hisat2` → hard error if index is `None`. The unwrap is sound |
| V7 | §1's failure shape (buried error, desync, truncated BAM left behind) | Ran the repro with `rust/target/debug/bismark` | **CONFIRMED** — exit 1, `(ERR): "/no/such/idx" does not exist…` followed by `error: Bowtie 2 produced fewer PE records than read pairs (desync)`, and a 306-byte `test_R1_bismark_bt2_pe.bam` + a report left in `-o` |
| V8 | Task 3's `--output_dir` hole | Ran a **working** 5-Base run into a nonexistent `-o` | **CONFIRMED** — dies at `error: failed to open BAM "…": I/O error: No such file or directory` after printing the whole summary. (Message is not quite "bare" — it names the path and the operation; the real defect is *lateness*, not wording) |
| V9 | *Unstated:* is `--temp_dir` the same hole? | Same run with a nonexistent `--temp_dir` | **No** — exit 0, already auto-created. Task 3's output-dir-only scope is correct; no asymmetry to fix |
| V10 | `HISAT2_INDEXES` — the plan defers this to oxy | Read the upstream hisat2 wrapper source (`DaehwanKimLab/hisat2:377-398`) | **Answered from source; no oxy trip needed.** See F7 — the upstream glob is malformed and the fallback cannot match a real index |
| V11 | Rust `Path::join` reproduces Perl `File::Spec->catfile` | Ran both | **NO** — see F3 |
| V12 | `create_dir_all` on `resolve_output`'s empty-`PathBuf` default is safe | Ran `std::fs::create_dir_all("")` | **Safe** — returns `Ok(())`. Task 3 in `resolve_output` will not break the ~30 unit tests that call `resolve()` with no `--output_dir` |
| V13 | Perl's own behaviour for `--output_dir` | Read `legacy_perl/bismark:8176-8200` | Perl **creates it and warns** `Created output directory $output_dir!`. Task 3 is *restoring* Perl behaviour, not inventing it — but Perl uses non-recursive `mkdir` (see F8) |

Line-number drift in the plan (all harmless, but fix on the way past): `resolve_output` is at
`config.rs:1549` (plan says 1533); the sibling `create_dir_all`s are at `mod.rs:579` and `mod.rs:648`
(plan says 582/651); `first_missing`/`index_suffixes` are at `discovery.rs:131`/`:111`.

---

## 1. Logic review

### F1 — CRITICAL: the plan breaks a currently-green test and never mentions it

`rust/bismark/tests/aligner_cli.rs:6271` `five_base_bowtie2_unconverted_index_end_to_end` does:

```rust
.arg("--five_base_index")
.arg(genome.path().join("normal_idx")) // basename; the fake bowtie2 ignores it
```

No index files exist at that basename — the test pairs it with a fake `bowtie2` shell script
(`make_fake_bowtie2_five_base_pe`) and asserts `.success()`. I ran it: **currently passes.**

```
test five_base_bowtie2_requires_index ... ok
test five_base_bowtie2_unconverted_index_end_to_end ... ok
test result: ok. 2 passed
```

Task 2 turns it red. It is the **only** end-to-end coverage of the bowtie2 5-Base route, so this is not
a test that can be deleted. The fix is one line — write six empty `normal_idx.*.bt2` stubs, which pass
`is_file()` — but it must be a named task, because an implementer who hits a red test they were not
warned about is as likely to weaken the check as to fix the fixture.

I grepped every test file: `--five_base_index` appears in `aligner_cli.rs` only, and only this one call
site passes a path. So it is exactly one collision, not a class.

**This also surfaces a false-rejection class the risk table does not name:** a *stub or wrapper*
`bowtie2` reached via `--path_to_bowtie2` that does not need a real index. The repo's own test suite is
an instance. Users do this too (conda shims, cluster wrappers, `bowtie2` scripts that stage an index at
run time). D1 blocks all of them with no escape hatch. That is a legitimate consequence of Felix's
decision, but it belongs in §7 and in the CHANGELOG, not discovered by a user.

### F2 — CRITICAL: Task 2's placement is under-specified, and one legal reading voids #1099's guard

Task 2 says "after `aligner` and `genome_arg` are known and **before** `detect_aligner`". Measured, that
is anywhere in lines **805–879**: `genome_arg` is resolved at `config.rs:804`,
`discover_genome_for_run` runs at `:861`, `detect_aligner` at `:880`. So the instruction permits
placement *before* `:861` — and that has a specific bad consequence.

`config.rs:2306` `five_base_resolve_does_not_require_the_converted_index` (the #1099 regression guard)
loops over `vec!["--bowtie2", "--five_base_index", "/nonexistent/idx"]` and asserts only

```rust
assert!(!matches!(e, AlignerError::FaultyIndex { .. }), …)
```

If the new check runs **before** `:861`, that arm dies at the new `Validation` error, the assertion
passes vacuously, and `discover_genome_for_run` — the thing #1099 fixed — is never reached on the
bowtie2 arm again. Green forever, testing nothing. This is precisely the `gh run list --commit`
short-SHA failure shape from the repo's own memory: a guard that passes because it never ran.

**Fix:** pin the position explicitly — *between `discover_genome_for_run` (`:861`) and `detect_aligner`
(`:880`)* — and say why in the plan (genome errors win; the #1099 guard keeps its reach). Genome-first
is also the better user-facing order: a missing genome is the more fundamental input error.

Secondary: after the pin, harden that #1099 test's bowtie2 arm to use a *real* stub index dir so it
keeps exercising both gates rather than depending on which one fires first.

### F3 — IMPORTANT: `Path::join` is not `File::Spec->catfile`; as written the env arm is wrong for an absolute basename

The plan specifies the env candidate as `<env>/<basename as given>`, which the natural Rust reading
implements as `env_dir.join(basename)`. Measured, the two disagree:

```
Perl  catfile("/opt/idx", "/no/such/idx") = /opt/idx//no/such/idx
Rust  join   ("/opt/idx", "/no/such/idx") = /no/such/idx          # absolute RHS replaces
Perl  catfile("",         "puc")         = /puc
Rust  join   ("",         "puc")         = puc
```

Two consequences:

1. **The plan's own example error text is unreachable.** It shows
   `(also tried $BOWTIE2_INDEXES: /opt/idx/no/such/idx.1.bt2)`. `Path::join` yields
   `/no/such/idx.1.bt2` — i.e. the message prints the *same path twice* — and catfile yields the
   double-slash form. Since §7's stated mitigation is "the error lists every path probed, so a wrong
   rejection is self-diagnosing", a duplicated line directly weakens the only mitigation D1 has.
2. **A genuine (if exotic) false rejection.** `-x /idx/puc` with `BOWTIE2_INDEXES=/opt` and the real
   index at `/opt/idx/puc` **works under bowtie2** (catfile → `/opt//idx/puc`) and would be rejected.

**Fix (one line, and simpler than `join`):** build the env candidate by `OsString` concatenation —
`env` + `/` + `basename` — not `Path::join`. That reproduces catfile for absolute basenames *and* for
the empty-var case, and removes the need to special-case anything.

### F4 — IMPORTANT: prefer appending to the whole basename over splitting it into `(dir, stem)`

`first_missing(aligner, dir: &Path, stem: &str, large)` requires the caller to split
`--five_base_index` into a parent and a UTF-8 stem. Measured Rust behaviour at the boundaries:

```
"puc"      parent=Some("")   file_name=Some("puc")     # ok, join("") is a no-op
"./puc"    parent=Some(".")  file_name=Some("puc")     # ok
"d/puc/"   parent=Some("d")  file_name=Some("puc")     # ok
"d//puc"   parent=Some("d")  file_name=Some("puc")     # ok — and bowtie2 accepts D//puc (measured rc 0)
"puc."     parent=Some("")   file_name=Some("puc.")    # ok
".."       parent=Some("")   file_name=None            # <-- None
"/"        parent=None       file_name=None            # <-- None
```

So `--five_base_index ..` or `/` yields `file_name() == None`. An `.unwrap()`/`.expect()` there is a
**panic on a hard-error path** — a crash instead of the clean message the whole change exists to
produce. Separately, `stem: &str` means a non-UTF-8 basename needs a `to_str()` that can return `None`;
clap hands you a `PathBuf`, so this is reachable.

**Fix:** don't split. bowtie2 itself never does — it appends (`$basename . ".1.bt2"`). Change Task 1 to
expose the *suffix list* (or add `pub(crate) fn missing_index_file(aligner, basename: &Path, large) ->
Option<PathBuf>` that pushes onto an `OsString`). That removes the `None` panic, the UTF-8 constraint,
and F3's `join` divergence in one stroke, and it is strictly closer to what the aligner does.

### F5 — IMPORTANT: the probe does not mirror the wrapper's *ordering* gate

The wrapper tries the env dir only when `glob("<basename>*.bt2{,l}")` is **empty**. So a *partial* index
at the given basename **blocks** the env fallback. Measured:

```
BOWTIE2_INDEXES=$D bowtie2 -x part2/puc …   # part2/ holds only puc.1.bt2
  => rc=1  "Could not open index file part2/puc.2.bt2"  +  SIGSEGV
```

The plan's order ("all-6 at the basename fails → try env") would find the env index and return `Ok`,
after which bowtie2 **segfaults**. Not a regression (that is today's behaviour), and it is the safe
direction under D1 — but §2 asserts "the probe must mirror the wrapper", and here it does not.

**Fix:** either mirror it (skip the env arm when *any* `<basename>*.bt2{,l}` exists) or state the
deliberate deviation. I lean **mirror it**: it is one `read_dir`-free `glob`-equivalent check, it makes
§2's claim literally true, and it keeps the error message honest about what was tried.

### F6 — IMPORTANT: §5 cannot cover "bare basename, no env, index in cwd", and shouldn't try

Test 5/6 cover the env arm. The remaining positive form — `-x puc` with cwd *being* the index dir, no
env var — is not unit-testable: `Path::new("").join("puc.1.bt2")` is cwd-relative, and changing cwd in
a Rust test is the same shared-process hazard §3 rightly avoids for `set_var`. §6's manual block covers
the env variant but not this one.

**Fix:** add one manual line to §6 (`cd "$G" && bismark … --five_base_index puc …`), and note in §5 why
it is not a unit test. Cheap, and it closes the last uncovered *positive* form.

### F7 — IMPORTANT: the oxy `HISAT2_INDEXES` blocker is resolvable from source, and §6's contingency is backwards

Upstream `hisat2:382-391`:

```perl
my @idx_filenames = glob($idx_basename . "*.ht2{,l}");
unless(@idx_filenames) {
    if(exists $ENV{"HISAT2_INDEXES"}) {
        @idx_filenames = glob("$ENV{'HISAT2_INDEXES'}/$idx_basename" . "ht2{,l}");
    }
```

Note the env glob: **no `*` and no `.`** before `ht2`. The pattern is `<env>/<basename>ht2{,l}`, which
for a real index (`puc.1.ht2`) can never match. `HISAT2_INDEXES` is effectively **broken upstream** for
ordinary index names. It also uses plain `/` interpolation rather than `catfile`.

This makes §6's contingency the wrong way round. §6 says: *"If it does not [behave like
`BOWTIE2_INDEXES`], drop the hisat2 half of the fallback."* But:

- **Keep** the symmetric fallback → in the "broken upstream" world the probe merely *over-accepts*
  (falls through to today's buried error: benign), and in a future world where upstream fixes the glob
  it is already correct.
- **Drop** it → only safe in the broken world, and becomes a false rejection the day upstream fixes it.

**Keeping is strictly safer in both worlds.** Recommend: keep it, add a one-line comment naming the
upstream glob quirk, and **retire the oxy trip as a merge blocker** — it is answered.

Replace it with the oxy question that is *actually* still open: **does hisat2 require all 8 `.ht2`
files?** I measured this for bowtie2 (all 6, V4) but hisat2 is not installed here. `discover_genome`
already ships the 8-file assumption for bisulfite runs, so it is not new risk — but under a hard error
it is worth the five minutes on a box that has hisat2.

### F8 — IMPORTANT: Task 3 should match Perl, which already does this — including the notice

`legacy_perl/bismark:8176-8200` does not just create the directory, it announces it:

```perl
mkdir $output_dir or die "Unable to create directory $output_dir $!\n";
warn "Created output directory $output_dir!\n\n";
```

Three things follow that the plan should state rather than leave to the implementer:

1. **Emit the notice.** It is Perl's behaviour, it is free, and it directly answers §7's last risk row
   ("creating a directory the user did not intend") — the user is *told*. Right now that row's
   mitigation is "every sibling entry point already does it", which is an argument from consistency,
   not from the user's point of view.
2. **`create_dir_all` vs Perl's non-recursive `mkdir`** is a real deviation: Perl dies on
   `-o a/b/c` when `a` is absent; `create_dir_all` succeeds. Forgiving direction, and matching
   `mod.rs:579`/`:648`/`convert.rs:129`, but document it as a chosen deviation.
3. **Pick one location.** The plan offers "`resolve_output` … *or* the align path's first write". A plan
   should decide. Recommend `resolve_output`: single choke point, mirrors Perl's timing inside
   `process_command_line`, and V12 confirms the empty-`PathBuf` default is safe there. Note the
   consequence: `resolve_output` runs *after* `detect_aligner`, so a bad index still creates nothing —
   which is what test 8 asserts.

While there: the three existing sites disagree with each other. `mod.rs:579` uses
`std::fs::create_dir_all(&out_dir).ok()` — **silently swallowing a real mkdir failure**, against the
repo's never-silent principle — while `mod.rs:648` maps it to a `Validation` error. Adding a fourth
variant makes it worse. Fold a one-line "unify on the erroring form" into Task 3.

### F9 — OPTIONAL: index forms I checked that are **not** false-rejection risks

Recording these so the next reviewer doesn't re-derive them. All measured against the real index:

| Form | bowtie2 | Plan's probe | Verdict |
|---|---|---|---|
| Symlinked index **files** | rc 0 | `is_file()` follows symlinks → accept | ✅ agree |
| Symlinked index **directory** | rc 0 | accept | ✅ agree |
| `D//puc` (doubled separator) | rc 0 | `parent()="D"`, `file_name()="puc"` → accept | ✅ agree |
| `D` (directory, no slash) | rejected | reject | ✅ agree |
| `D/` (directory, trailing slash) | wrapper glob passes, **align fails** | reject | ✅ plan is *better* — earlier, clearer |
| `D/puc.` (trailing dot) | wrapper glob passes, **align fails** | reject | ✅ plan is better |
| `D/puc.1` (suffix included) | align fails | reject | ✅ plan is better |
| Partial index (5 of 6) | align fails (SIGSEGV) | reject | ✅ plan is better |
| Mixed: all `.bt2l` + a stray `.1.bt2` | picks **small**, fails | large arm → accept | ⚠️ over-accepts; benign (falls through to today's error) |
| Mixed: all `.bt2` + a stray `.1.bt2l` | rc 0 | small arm → accept | ✅ agree |
| Dangling symlink | glob matches, align fails | reject | ✅ plan is better |
| Case-differing basename (`D/PUC`) | **rejected** (Perl glob is case-sensitive) | `is_file()` on APFS → **accept** | ⚠️ macOS-only over-accept; benign, but means a macOS test can't pin case behaviour |

Net: **every divergence I found runs in the over-accepting direction** (fall through to today's buried
error) **except the ones F3 and F4 introduce**. That is the right shape for a hard error, and it is a
stronger argument for D1 than §7 currently makes — worth putting in the plan.

### F10 — OPTIONAL: `--large-index` is not reachable, so the two-arm probe is complete

The wrapper's small/large decision keys on `.1.<ext>` only, and `--large-index` forces large. Bismark
never emits `--large-index` for the 5-Base route (`five_base_build_argv`, `mod.rs:1546-1577`, emits
`-x` + `-1`/`-2` and nothing else), so the plan's two-arm probe covers every reachable case. Worth one
sentence so a future reader doesn't wonder.

---

## 2. Assumptions

**Stated and validated**

- *bowtie2 honours `BOWTIE2_INDEXES` for separator-bearing basenames* — validated (V1), and it is the
  plan's load-bearing claim. It holds.
- *The existing guards make `five_base_index` non-`None` on the bowtie2/hisat2 5-Base path* —
  validated (V6). The unwrap is sound. (Style: `if let Some(idx)` reads better than `.expect()` on a
  validation path, though `mod.rs:1559` sets an `.expect()` precedent.)
- *`resolve`-level tests cannot assert `Ok`* — validated (V5): `detect_aligner` is the only exec, and
  CI installs minimap2 + samtools only (`.github/workflows/rust_ci.yml:35-45`).
- *`HISAT2_INDEXES` is unverified* — now verified from source (F7); the caveat can be retired.

**Implicit, and load-bearing**

- **A1 — that no existing test passes a nonexistent `--five_base_index`.** False (F1).
- **A2 — that `resolve()` reading the env var is the right layer.** Mostly yes: the parameter-injection
  design *does* genuinely defeat the parallelism hazard, because unit tests never touch the process
  environment and integration tests set it per-child via `assert_cmd`'s `.env()`. One nit: `resolve()`
  then owns the aligner→var-name mapping (`BOWTIE2_INDEXES` / `HISAT2_INDEXES`), which is index
  knowledge living outside the module that owns index knowledge. A three-line
  `fn index_env_for(aligner) -> Option<OsString>` in `discovery` keeps it in one testable place and
  makes the minimap2/rammap → `None` case explicit.
- **A3 — that "the probe mirrors the wrapper" is literally true.** Not quite (F3, F5).
- **A4 — that `first_missing`'s `(dir, stem)` shape fits an arbitrary user basename.** It fits the
  common forms but has two hard edges (F4).
- **A5 — that empty stub files are an acceptable index fixture.** True and worth stating: `is_file()`
  is true for a 0-byte file, so unit fixtures (and F1's fix) need no real `bowtie2-build`. This is what
  keeps §5a fast and CI-safe.
- **A6 — that a stub/wrapper aligner is not a supported configuration.** Adopted implicitly by D1, and
  the repo's own suite depends on it (F1). Make it explicit.

---

## 3. Efficiency

No concerns; the change is stat-bound and trivial.

- Worst case: bowtie2 6 small + 6 large + 12 more for the env arm = **≤24 `stat` calls**; hisat2
  ≤32. Against a run that then spends minutes-to-hours in the aligner, unmeasurable.
- `first_missing` uses `.find()`, so it short-circuits on the first miss — the common failure case
  (nothing there) costs **one** stat per arm, not six.
- `index_suffixes` allocates a `Vec<String>` per call (≤4 calls). Irrelevant here, and pre-existing.
  If F4's append-based refactor lands, a `&'static [&'static str]` suffix table drops the allocation
  entirely as a side effect — nice, not necessary.
- Task 3 adds one `mkdir` syscall to `resolve_output`, and `create_dir_all("")` short-circuits before
  any syscall for the default case (V12).
- No scalability dimension: the cost is independent of genome size, read count and `--multicore`.

---

## 4. Alternatives

The plan's four rejected alternatives are all correctly rejected, and I verified the reasoning for
two of them:

- *"Validate inside `resolve_aligner`"* — correctly rejected. Confirmed: `illumina_5base_engine_selection`
  (`config.rs:2302`) calls `resolve_aligner` directly with the literal basename `"idx"`; a presence
  check there would break it. The plan's instinct was right and the test proves it.
- *"Reuse `discover_genome`"* — correct, it hard-codes `Bisulfite_Genome/CT_conversion/BS_CT`.

Alternatives the plan did **not** consider, in order of value:

1. **Put the helper in `discovery.rs`, not `config.rs`** *(recommended)*. §3 puts it in `config.rs`,
   which forces two helpers to `pub(crate)` and creates §7's third risk row. Moving it to `discovery`
   — the module that already owns per-aligner index-suffix arity and the small→large retry — keeps
   both helpers **private**, deletes that risk row, and gives the env-var mapping a natural home
   (A2). `config::resolve` then calls one `pub(crate)` entry point. Strictly less surface area.
2. **Append to the basename instead of splitting it** *(recommended — F4)*. Closer to what bowtie2
   does, and it kills three edge cases at once.
3. **A `--force`-style escape hatch** for stub/wrapper aligners *(recommended against, but decide
   deliberately)*. D1 forecloses it, and I agree for the default path — but F1 shows the repo itself
   needs the configuration D1 blocks. If Felix would rather not ship a flag, the answer is fixture
   files in the test, which is the F1 fix anyway. Worth one line in §7 recording that the option was
   seen and declined.
4. **Probe only the first file per arm** (`.1.bt2`, mirroring the wrapper's own small/large decision)
   and let the aligner catch the rest. Cheaper and impossible to false-reject on a partial index —
   but V4 shows a partial index **segfaults**, so catching it early is worth more than the marginal
   safety. Keep the all-files probe; the plan is right.
5. **Warn once and continue on the env-arm-only match**, hard-error only when neither arm resolves.
   A middle setting between D1 and warn-only, targeted exactly at the residual risk. Mentioning it for
   completeness; D1 settled this and the added state is not worth it.

---

## 5. Action items

### Critical — fix before implementation

- **C1 (F1)** Add a task: fix `aligner_cli.rs:6271` `five_base_bowtie2_unconverted_index_end_to_end`,
  which passes a nonexistent `--five_base_index` and currently **passes**. Write six empty
  `normal_idx.{1,2,3,4,rev.1,rev.2}.bt2` stubs. Frame it as the plan's strongest false-rejection guard:
  it is the only end-to-end exercise of a *real* basename through the new check.
- **C2 (F2)** Pin Task 2's call site to **between `config.rs:861` (`discover_genome_for_run`) and
  `:880` (`detect_aligner`)**, not the 805–879 range the current wording allows. State the reason:
  earlier placement makes #1099's regression guard (`config.rs:2306`) vacuously green on its bowtie2
  arm, and genome errors should win.

### Important — fix in rev 2

- **I1 (F3)** Specify the env candidate as `OsString` **concatenation** (`env` + `/` + basename), not
  `Path::join`. Fix §3's example error text, which no implementation can currently produce.
- **I2 (F4)** Change Task 1 to expose the suffix list / a `missing_index_file(aligner, basename, large)`
  that appends to the whole basename. Removes the `file_name() == None` panic (`--five_base_index ..`),
  the UTF-8 constraint, and I1's divergence together.
- **I3 (F5)** Either mirror the wrapper's gate (skip the env arm when any `<basename>*.bt2{,l}` exists)
  or record the deviation. As written, §2's "must mirror the wrapper" is not literally true.
- **I4 (F7)** Keep the hisat2 env fallback (it can only over-accept — safe in both worlds), note the
  malformed upstream glob, and **retire the oxy `HISAT2_INDEXES` merge blocker**. Replace it with the
  question that is still open: does hisat2 require all 8 `.ht2` files?
- **I5 (F8)** Task 3: choose `resolve_output`; emit Perl's `Created output directory …` notice; record
  `create_dir_all` vs Perl's non-recursive `mkdir` as a deliberate deviation; fold in unifying
  `mod.rs:579`'s silent `.ok()`.
- **I6 (F1)** Add to §7 and to Task 4's CHANGELOG text: a **stub/wrapper `bowtie2` via
  `--path_to_bowtie2` that does not need a real index will now fail**. User-visible behaviour change,
  and the repo's own suite is an instance.
- **I7 (F6)** Add the "bare basename resolved from cwd, no env" case to §6's manual block, and note in
  §5 why it cannot be a unit test (cwd mutation = the same shared-process hazard as `set_var`).

### Optional — nice to have

- **O1** Add unit tests for: a **symlinked** index (positive — pins that the implementation uses
  `is_file()`, not `symlink_metadata`), an **absolute** basename (positive), a `../`-relative basename
  (positive — common in pipelines), and **pathological basenames that must error rather than panic**
  (`..`, `/`, a trailing dot). The first three are the false-rejection guards D1 most needs; the last
  is I2's regression test.
- **O2** Strengthen test 8: also assert stderr does **not** contain `panicked` and that the report file
  is absent too, not just the `.bam`. "Nothing is written" is the actual contract.
- **O3** Put V4 (all 6 bowtie2 files are genuinely required — missing `.2`/`.rev.*` **segfaults**) into
  §2 or §7. It is the best available argument that the probe is not stricter than the aligner, and it
  makes D1 look like the considered choice it is.
- **O4** Put the F9 table (or its conclusion — *every* divergence found runs in the over-accepting
  direction except the two introduced by I1/I2) into §7. It converts "residual risk, accepted" into a
  bounded, evidenced claim.
- **O5** Add `cargo clippy -p bismark --all-targets --features binseq-input -- -D warnings` to §6;
  `rust_ci.yml:135` runs it and the plan's list omits it.
- **O6** Note that `--temp_dir` is **already** auto-created (V9), so Task 3's output-dir-only scope is
  deliberate rather than an oversight.
- **O7** Fix the line-number drift: `resolve_output` 1533→**1549**; `mod.rs` 582/651→**579/648**.
- **O8** Soften §1/Task 3's "a bare `No such file or directory`" — measured, the message is
  `error: failed to open BAM "<path>": I/O error: No such file or directory`. The defect is that it
  comes *after* the full config summary, not that it is uninformative.
- **O9** State A5 explicitly (empty stub files satisfy the probe), since it is what makes both §5a and
  C1's fix cheap.

---

## 6. Summary

The plan is well-researched and its riskiest claim survives independent re-derivation intact — §2 is
correct, the `./puc` rescue is real, and the separator heuristic really would have shipped a
false rejection. Measurement also handed the plan a better argument than it makes for itself: bowtie2
requires **all six** index files (missing `.2` or `.rev.*` segfaults), so the probe is not stricter than
the aligner, and every divergence I could find runs in the over-accepting direction. Under D1 that is
the right shape.

The two blockers are both about the plan's completeness rather than its design: it will break a green
test it does not know about (**C1**), and its placement instruction is loose enough to silently void
#1099's regression guard (**C2**). The `Path::join`/`catfile` mismatch (**I1**) and the
`(dir, stem)` split (**I2**) are the only places the change would introduce false rejections of its
own, and both are one-line fixes — best taken together with the "append, don't split" refactor. One
piece of good news: the `HISAT2_INDEXES` question is answerable from upstream source, so §6's oxy
merge blocker can be retired (**I4**).
