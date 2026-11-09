# CODE REVIEW B — #1100 `--five_base_index` resolve-time validation (+ `--output_dir` auto-create)

**Reviewer:** B (independent; no shared state with Reviewer A or the coverage agent)
**Target:** branch `1100-five-base-index-validation`, **uncommitted** working tree over `dev` @ `688d919`
**Files:** `rust/bismark/src/aligner/{discovery,config,mod,cli}.rs`, `rust/bismark/tests/aligner_cli.rs`, `CHANGELOG.md`, `docs/src/content/docs/rust/illumina-5-base.md`
**Read:** `PLAN.md` (rev 2, incl. §11), `PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`, commit `1a4f8fa` (#1099)

> **⚠️ Fix-vs-recommend OVERRIDE acknowledged.** The skill's "fix low-risk problems directly" rule was
> overridden by the caller: three agents review the same uncommitted tree concurrently, so any edit
> would race. **I changed no tracked file.** Everything below is a recommendation; the only file I
> wrote is this report. All measurements were taken in `/tmp/claude-501/revB_1100_35539` against
> `rust/target/debug/bismark` (mtime 11:33, newer than both changed sources) and the installed
> `bowtie2 2.5.5`.

---

## Verdict

**Design is sound and the implementation matches the plan closely.** The placement is exactly as
specified, the `OsString`-concatenation decision is right (and, as it turns out, a *better* mirror of
real bowtie2 than the plan realised), and I could not fault the ordering guarantees. Four of the
plan's five manual §6 checks were unverified by the caller; I ran all of them against a **real**
`bowtie2-build` index and the real aligner, and they pass end-to-end.

**One Critical finding.** §2/§3 model the `BOWTIE2_INDEXES` fallback as living in the bowtie2
**wrapper** and mirror the wrapper's prefix-glob gate. That model is incomplete: `bowtie2-align-s`
performs its **own, independent** `BOWTIE2_INDEXES` fallback, gated only on whether
`<basename>.1.bt2` can be opened. Because `any_index_file_at` mirrors the wrapper's wider gate
instead of the binary's narrower one, **there is a measured configuration that real bowtie2 runs
successfully (exit 0) and this change now hard-rejects (exit 1)** — the exact false-rejection class
D1's decision was premised on being absent, and §7 records as "No false-reject case found". The
trigger is narrow and the fix is ~5 lines, but the premise behind D1 no longer holds as stated, so
Felix should see this before merge.

Second substantive finding: the plan's required `HISAT2_INDEXES` test twin (§5 item 12) was
**dropped without appearing in §11's deviation log**, leaving the `HISAT2_INDEXES` string literal with
zero coverage of any kind — the precise seam §5 item 12 existed to close.

Nothing I found threatens `perl-oracle` byte-identity or the byte-frozen bisulfite paths.

---

## Critical

### C1. Measured false rejection: the env fallback also lives in the aligner *binary*, whose gate is narrower

`discovery.rs:166-172` (`any_index_file_at`) blocks the env arm when **any** expected index file
exists at the basename, mirroring the wrapper's `glob($idx_basename . "*.bt2{,l}")`. The wrapper
(`/opt/homebrew/bin/bowtie2:379-398`) does behave exactly as §2 says — I re-read it and confirm the
prefix glob, the `unless (@idx_filenames)` gate, and `File::Spec->catfile` with a **single** directory
(no colon-list, so §3 is right to treat it as one path).

But the wrapper is not the layer that decides. `bowtie2-align-s` contains its own
`adjustEbwtBase()`-style resolution — its string table holds, in order,
`Trying ` / `  didn't work` / `BOWTIE2_INDEXES` / `  worked` / `Could not locate a Bowtie index
corresponding to basename "`. It probes `<base>.1.<ext>`, and **only if that open fails** joins
`getenv("BOWTIE2_INDEXES") + "/" + base`. (Note: `"/"` concatenation — the Rust code at
`discovery.rs:213-215` is therefore a *closer* mirror of the deciding layer than of `catfile`. That
part is a strength, not a defect.)

Consequence: when the file present at the basename is **not** `.1.bt2`, the wrapper's glob matches
(blocking *its* fallback) while the binary's `.1.bt2` probe still fails (engaging *its* fallback), and
the run works.

**Measured, three runs, same cwd containing only a zero-byte `puc.2.bt2`, complete real index at `$S/g`:**

| Run | Command | Exit |
|---|---|---|
| Real bowtie2, `BOWTIE2_INDEXES` set | `bowtie2 -x puc -1 … -2 … -S /dev/null` | **0** (full alignment summary, 5000 pairs processed) |
| Control — same, `BOWTIE2_INDEXES` unset | `env -u BOWTIE2_INDEXES bowtie2 -x puc …` | 1 (`bowtie2-align exited with value 1`) |
| **This change** | `BOWTIE2_INDEXES=$S/g bismark --illumina_5base --bowtie2 --five_base_index puc …` | **1** |

The Rust error is:

```
error: --five_base_index puc is not a complete Bowtie 2 index. Probed: puc.1.bt2, puc.1.bt2l. Build a NORMAL (unconverted) index once with bowtie2-build.
```

The control proves the env var is what rescued bowtie2, and the wrapper's own echoed command line
(`bowtie2-align-s --wrapper basic-0 -x puc …`) proves the basename reached the binary **unrewritten** —
so the resolution happened below the wrapper. `five_base_build_argv` (`mod.rs:1551-1571`) passes
`config.five_base_index` verbatim, and there is no `env_clear`/`env_remove` anywhere in
`aligner/`, so the child sees the same variable `resolve()` read.

**How reachable is this?** It needs a file matching `<basename>.{2,3,4,rev.1,rev.2}.bt2` (but *not*
`.1.bt2`) in the resolved-from directory, while `BOWTIE2_INDEXES` points at the real index. Realistic
instances: a killed/interrupted `bowtie2-build` in the working directory; a scratch directory holding
one salvaged index file; someone who deleted `puc.1.bt2` specifically to force the env index. Narrow —
but it is a **hard error with no escape hatch** (the plan declined `--force` on purpose, `§3 Rejected
alternatives`), so an affected user has no way through except renaming files they may not own.

**Recommended fix** — mirror the deciding layer instead of the wrapper. Replace `any_index_file_at`:

```rust
/// `true` if `<basename>.1.{bt2,bt2l}` exists — the file the aligner binary opens before it falls
/// back to its index environment variable.
fn first_index_file_at(aligner: Aligner, basename: &OsStr) -> bool {
    [false, true].into_iter().any(|large| {
        index_files_for(aligner, basename, large)
            .first()
            .is_some_and(|p| p.is_file())
    })
}
```

`index_suffixes` yields `1,2,3,4,rev.1,rev.2` for bowtie2 and `1..=8` for hisat2, so `.first()` is
`.1.<ext>` in both arms. That ordering is now load-bearing and deserves one clause on
`index_suffixes`' doc comment (house rule: put the invariant on the thing it constrains).

**This fix keeps the new test suite green.** `partial_index_at_the_basename_blocks_the_env_fallback`
(`aligner_cli.rs:6359`) writes exactly `puc.1.bt2`, so the narrower gate still fires and the test
still fails on `if true`. I verified the semantics against real bowtie2 for that case too: with
`puc.1.bt2` present the binary opens it, never reaches its env fallback, and dies on `puc.2.bt2` — so
rejecting is correct there and the test is pinning real behaviour. The gate change strictly *reduces*
divergence; it does not trade one false rejection for another.

**Also update the artefacts** if this is applied: §7's "No false-reject case found. Every divergence
runs in the over-accepting direction" and D1's "so the probe is not stricter than the aligner" are
both falsified as written, and D1's justification should record the two-layer resolution.

---

## High

### H1. The `HISAT2_INDEXES` test twin was dropped, undocumented — the literal has zero coverage

`grep -rn "HISAT2_INDEXES" bismark/tests/ bismark/src/` returns exactly two hits: the string literal
at `discovery.rs:178` and the help text at `cli.rs:121`. **No test of any kind exercises it.**

Plan §5 item 12 required it explicitly — *"Plus the `HISAT2_INDEXES` twin"* — and stated the reason:
*"a misspelled or swapped variable name, `env::var` instead of `var_os`, or a hard-coded `None` at the
call site all ship a hard error on a working setup with every unit test green."* §11's deviation log
records only **one** deviation (the vacuous unit test 7). This drop is therefore undocumented, which
matters more than the missing test itself: §11 is the record Felix and the coverage audit read.

Note the unit tests **cannot** close this: they inject `index_env` directly, so they exercise the
fallback mechanism while never touching the name-to-variable mapping. Only reading the real
environment does. Two cheap options, either sufficient:

1. A one-line unit test on the mapping — `assert_eq!(index_env_name(Aligner::Hisat2), Some("HISAT2_INDEXES"))`
   (plus the bowtie2 arm). Catches a typo, needs no binary.
2. An integration twin of `five_base_index_resolved_via_env_is_accepted` using `.env("HISAT2_INDEXES", …)`
   and 8 stub `.ht2` files. Note `hisat2` is **not** installed here or in CI, so this can only assert
   the check does *not* fire (stderr lacks `--five_base_index`) before `detect_aligner` rejects the
   missing binary — which is exactly what the bowtie2 twin's negative form asserts anyway.

The bowtie2 arm of `index_env_name` **is** covered (`aligner_cli.rs:6336`), so this is a one-arm gap.

### H2. Three doc surfaces contradict the plan's own hisat2 finding

`CHANGELOG.md`: *"exactly as the bowtie2 and hisat2 wrappers do — a retry under `$BOWTIE2_INDEXES` /
`$HISAT2_INDEXES`"*. `illumina-5-base.md:53-56`: *"As with bowtie2 and hisat2 themselves…"*.
`cli.rs:120-121`: *"as the aligner itself does"*.

Plan §2 concludes the opposite for hisat2: *"`HISAT2_INDEXES` — … Upstream `hisat2:382-391` globs
`"$ENV{HISAT2_INDEXES}/$idx_basename" . "ht2{,l}"` — no `*`, no `.` — so it cannot match a real
`puc.1.ht2`. The variable is effectively broken upstream."* Both statements cannot be true. As shipped,
a hisat2 user who reads the docs, sets `HISAT2_INDEXES`, and passes a bare basename would clear our
check and then have the aligner fail to find the index — reintroducing the late failure #1100 exists
to remove, for that configuration.

C1 makes me doubt the plan's conclusion rather than the docs: hisat2 is a bowtie2 derivative, and if
`hisat2-align-s` inherits `adjustEbwtBase()` then `HISAT2_INDEXES` works fine via the binary and the
wrapper glob is irrelevant. **I could not test this — `hisat2` is not installed on this box.** Either
way one artefact needs correcting, and the cheap resolution is the check the plan already parked for
oxy (§6 "Open for oxy"), now with a second question attached:

- does `hisat2-align-s` contain `HISAT2_INDEXES` in its string table (`strings … | grep`)?
- does hisat2 require all 8 `.ht2` files?

Until answered, I'd soften the three surfaces to name bowtie2 as measured and hisat2 as symmetric
("the same lookup is applied for `$HISAT2_INDEXES`"), rather than asserting hisat2 behaves this way.

---

## Medium

### M1. `Probed:` lists first-missing files, not probed paths — and the CHANGELOG repeats the overstatement

`discovery.rs:201-222` pushes **one** entry per arm (`first_missing_at` returns the first absent
file), so the list has at most 2 entries (4 with the env arm), while up to 24 paths are actually
stat-ed. The label `Probed:` and the CHANGELOG's *"listing every path probed"* both promise more than
is delivered. Measured example — a 5-of-6 index reports `Probed: puc.rev.2.bt2, puc.1.bt2l`, which
reads as "we only looked at these two".

`Missing:` would be accurate and is a one-word change; the CHANGELOG clause wants to become
something like "naming the flag and the first missing file in each index size".

### M2. When the env var is set but deliberately skipped, the error says nothing about it

`env_note` (`discovery.rs:229-232`) fires only for `(Some(var), None)` — i.e. only when the variable is
**unset**. In the C1 run, `BOWTIE2_INDEXES` was set and correct, the gate skipped it, and the error
was silent about the variable's existence. That is the single hardest error in this change to diagnose:
the user can see their index under `$BOWTIE2_INDEXES` and is told it is "not a complete Bowtie 2
index". Worth a third arm — when the variable is set and the gate suppressed the fallback, say so
("`$BOWTIE2_INDEXES` was not consulted: files matching `puc.*.bt2` exist at the basename"). If C1's
fix is applied this becomes rarer but not impossible.

### M3. No test pins "a rejected index creates no output tree" — the assertion §5 item 11b asked for

Plan §5 11b: *"Also assert a rejected index creates **nothing**, which holds because `resolve_output`
runs after `detect_aligner`."* Neither new test does this:

- `five_base_index_missing_fails_early` passes an **already-created** `TempDir` as `--output_dir`, so
  it asserts the dir is *empty*, not that it was never *created*.
- `output_dir_is_created_with_a_notice` only covers the success direction.

The behaviour is correct today — I measured it: `-o $S/never/a/b` with a bad index exits 1 and
`$S/never` does not exist. But nothing would catch someone moving `resolve_output` earlier in
`resolve()`, which would start leaving stray trees on every rejected run. One line inside the existing
`five_base_index_missing_fails_early` closes it: point `--output_dir` at a nested non-existent path and
`assert!(!parent.exists())`.

---

## Low

### L1. Comment style — the house rule (one line default, two max, current state, no justification)

Assessed every added comment. The call-site comment is the model to copy; four others argue with
rejected alternatives or pre-empt objections, which the rule assigns to the commit message.

| Site | Issue |
|---|---|
| `config.rs:874-875` call site | ✅ Two lines, states an ordering invariant on the thing it constrains. Keep as-is. |
| `config.rs:1569-1571` `resolve_output` | **Three lines** (over the two-line max). Sentence 3 (`create_dir_all("")` is `Ok`") describes a call the code never makes — the `!is_empty()` guard on the next line short-circuits it. Also embeds `legacy_perl/bismark:8176-8200`, a line number that will rot. Suggest one line: `// Perl creates a missing --output_dir and says so; unlike Perl's single-level mkdir, missing parents are created too.` |
| `discovery.rs:141-142` `index_files_for` | *"Appending rather than splitting … cannot fail on `..` or `/`"* — argues against the rejected `(dir, stem)` split. Plan/commit material. |
| `discovery.rs:163-165` `any_index_file_at` | *"A stray file the wrapper's wider glob matches and this does not merely widens the fallback, which is the harmless direction."* — textbook pre-emption of "but doesn't this diverge?", which the rule names explicitly. (Its *claim* is correct and I verified it — see V4 — but it belongs in the commit message.) |
| `discovery.rs:190-192` `validate_unconverted_index` | The `index_env`-is-a-parameter sentence justifies a design choice. Defensible as caller-facing contract ("you must inject this"), but should be one clause. |
| `discovery.rs:211-212` inline | *"Concatenation, not `Path::join`"* — names the rejected alternative, but states a real constraint on the line below it. Acceptable. |
| Test doc comments | Several run 2-3 lines and reference rejected designs (`pathological_basenames_error_rather_than_panic`: *"a split-based probe would panic…"*). Tests get some latitude and these genuinely explain *why the case matters*; flagging for consistency only, not asking for a rewrite. |

### L2. Perl's notice carries a trailing slash; ours does not

Perl (`legacy_perl/bismark:8177-8195`) appends `/` to `$output_dir` **before** the `warn`, so it prints
`Created output directory /path/to/out/!`. Ours prints `Created output directory /path/to/out!`
(measured). The `\n\n` blank line matches. Purely cosmetic, but the CHANGELOG says the notice is
restored *"including the notice, `Created output directory <dir>!`"*, so either match the slash or
don't claim verbatim parity.

Ordering also differs: Perl emits it during command-line processing (before the aligner probe); ours
lands after `Bowtie 2 seems to be working fine` because `resolve_output` runs after `detect_aligner`.
That is the deliberate D3/§11 trade-off (nothing written before a rejected index) and I would not
change it — just noting it is a real, visible divergence from Perl's transcript order.

### L3. `mod.rs` consensus path — error names a flag the user may not have passed, and is untested

`mod.rs:578-584`: `out_dir` defaults to `PathBuf::from(".")`, so a mkdir failure with no
`--output_dir` reports `consensus: --output_dir .: …`. Prefer wording that survives the default.

Reachability and correctness both check out: `--five_base_consensus_from_bam` dispatches at
`mod.rs:178-179`, **before** `resolve()`, so it never passes through `resolve_output` — the second
choke point is genuinely needed, not redundant. `create_dir_all(".")` on an existing dir is `Ok`, so
the default path is unaffected, and the `consensus: ` prefix matches its siblings at `mod.rs:549`/`558`.
The `.ok()` → `map_err` swap is a strict improvement (it previously swallowed a real mkdir failure and
surfaced it later as a confusing `failed to open BAM`).

The new error arm is **untested** — the only test touching this flag is
`aligner_five_base_bisulfite.rs:358`, which exercises the success path. Testing a mkdir failure needs
an unwritable parent (`0o555`), which is `#[cfg(unix)]`-only and arguably not worth it for a
one-line `map_err`. Recording it as a known gap rather than asking for the test.

### L4. Two near-duplicate stub-index helpers

`stub_index` (`discovery.rs:472-479`, unit tests) and `make_stub_bowtie2_index`
(`aligner_cli.rs:47-55`, integration) do the same job. They sit in different compilation units, so
sharing would need a `pub(crate)` test-only helper or a `tests/common/` module — not worth it for six
lines. Noting only because the integration copy hard-codes the six bowtie2 suffixes rather than
deriving them, so a future arity change touches two places. A comment on one pointing at the other
would be enough.

### L5. `pathological_basenames_error_rather_than_panic` is cwd-dependent

`".."` and `"puc."` are relative, so the test probes `...1.bt2` and `puc..1.bt2` **relative to the
test process's cwd** (`rust/bismark`). It passes because those files don't exist — but it would flip to
a false green if such a file ever appeared, and it silently tests a different thing than a reader
expects. Harmless in practice; a `TempDir`-rooted variant alongside the relative one would be
strictly better.

### L6. `BOWTIE2_INDEXES=""` probes an unintended absolute path

`index_env_for` returns `Some("")` for an empty-but-set variable, so the env arm builds `"" + "/" +
base` → `/puc.1.bt2`. Both bowtie2 and this code then fail, so the outcome matches; only the error
text is odd (it names a root path nobody asked for). The wrapper gates on `exists $ENV{…}`, so an
empty value does enter its arm too — the divergence is confined to the message. Not worth code, but
if M1/M2 are touched, skipping an empty `index_env` is free.

---

## Efficiency

Nothing to report. `validate_unconverted_index` runs **once per run** and costs at most 4 small `Vec`
allocations and 24 `is_file()` stats — immeasurable next to a bowtie2 index load. `index_files_for`
re-allocating per arm is the readable choice and I would not optimise it. The `resolve_output` change
adds one `is_dir()` stat per run.

---

## Answers to the specific questions posed

**V1. `--output_dir` blast radius — does it touch the byte-frozen paths or `perl-oracle`? No.**
`resolve_output` has exactly **one** caller (`config.rs:913`, inside `resolve()`), confirmed by grep;
the `bam2nuc`/`coverage2cytosine` `resolve_output_dir` functions are unrelated namesakes. So the
change is scoped to *aligner* runs. The `perl-oracle` job (`rust_ci.yml:164-240`) runs 13 named
oracles — genome-prep, extractor, dedup, methylation-consistency, `prepare` — and **none of them
invokes the aligner**, so neither the new directory nor the stderr notice can enter that comparison.
More generally the notice goes to **stderr** (`eprintln!`), while the oracles diff output *files*; and
Perl emits the same notice on `warn` (also stderr), so even a hypothetical aligner oracle would see
parity rather than a new line. No existing test asserts an exact full-stderr equality for an aligner
run (the only stderr captures, `aligner_cli.rs:3175-3188`, use `contains`-style predicates).

**V2. Does it fire when it should not? No.** Guarded on `!output_dir.as_os_str().is_empty()`, so the
default (`PathBuf::new()`, flag absent) never reaches `create_dir_all` — and the notice only prints
when `!is_dir()`, i.e. genuinely created. An existing dir is silent; an existing *file* at that path
fails loudly with `--output_dir <p>: File exists`; a symlink-to-dir is accepted (`is_dir()` follows).

**V3. Position relative to `detect_aligner` — correct, and a rejected index leaves nothing behind.**
Measured, not just read: `resolve()` calls `discover_genome_for_run` at `:861`, the new #1100 check at
`:874-882`, `detect_aligner` at `:890`, `resolve_output` at `:913`. I confirmed `resolve_output` is the
**last fallible step** in `resolve()` (no `?` or `return Err` between `:913` and the function's end at
`:995`), so within `resolve()` the directory is created only on success. End-to-end: a bad index with
`-o $S/never/a/b` exits 1 and `$S/never` is never created. Caveat worth knowing: failures *after*
`resolve()` returns (missing input FastQ, aligner death) still leave an empty directory behind — same
as Perl, and disclosed by the notice.

**V4. The `.ok()` → `map_err` unification — correct, reachable, not tested.** See L3.

**V5. Did the two repaired fixtures weaken what they assert? No — the #1099 guard is structurally intact.**
`five_base_resolve_does_not_require_the_converted_index` cannot be made vacuous by this change,
because `discover_genome_for_run` (`:861`) runs **before** the new check (`:874`): the bowtie2 arm
reaches discovery unconditionally. If `discover_genome_for_run` were ever made unconditional again it
would return `FaultyIndex` at `:861` and the `!matches!(e, FaultyIndex { .. })` assertion would fire —
so the regression guard still bites. The stub files land at the genome *root* (`normal_idx.*.bt2`),
not under `Bisulfite_Genome/CT_conversion/`, so they cannot accidentally satisfy the bisulfite check;
and they don't end in `.fa`/`.fasta`, so they don't perturb the byte-significant FASTA inventory that
sets `@SQ` order. The added `!e.to_string().contains("--five_base_index")` assertion is what makes the
new stub load-bearing (without it the `FaithfulIndex` assertion would pass even on a #1100 rejection,
since the new error is `Validation`). Same reasoning for
`five_base_bowtie2_unconverted_index_end_to_end`: it keeps its `make_genome_fasta_only` base, so the
"#1099: no CT/GA `.bt2`" property it guards is preserved, and its polarity assertions are untouched.

One note: plan §5 item 13 (`five_base_minimap2_route_is_not_index_checked`) is satisfied only
**vacuously** — the loop's first arm passes no `--five_base_index`, so `if let Some(index)` never
fires. That is fine, because `resolve_aligner` (`config.rs:1303-1309`) rejects `--five_base_index` for
the default minimap2 route outright, making the combination unreachable. Worth one line in COVERAGE so
it isn't mistaken for real coverage.

**V6. CHANGELOG accuracy.** Verified claim by claim; two defects (M1, H2) plus one cosmetic (L2).
Everything else holds:
- *"died at `failed to open BAM … No such file or directory`"* ✅ — `open_sinks` (`mod.rs:2801`) is on
  the general alignment path, so the old failure mode is described correctly for bisulfite runs too,
  not just 5-Base.
- *"a truncated BAM plus a report were left behind"* ✅ consistent with the pre-change behaviour and
  both plan reviewers' reproductions.
- *"both index sizes (a mammalian bowtie2 index is `.bt2l`, hisat2's `.ht2l`)"* ✅.
- *"including when that basename contains a path separator"* ✅ — I verified this end-to-end with the
  real aligner: `--five_base_index ./puc` with `BOWTIE2_INDEXES` set completes, exit 0. This is §2's
  central claim and it holds.
- *"a **stub or wrapper** aligner … will now be rejected"* ✅ honest and correctly scoped — and it is
  the disclosure that C1 sits adjacent to, since C1 adds a *second*, undisclosed rejection class.
- *"`--temp_dir` was already auto-created"* ✅ `convert.rs:128` (`temp_dir_prefix`), which also
  short-circuits on empty. Note it emits **no** notice, unlike the new `--output_dir` path and unlike
  Perl's `Using temp directory:` — a small internal asymmetry, out of scope.
- *"matching the rest of the suite"* (nested parents) ✅ `mod.rs:578` and `convert.rs:128` both use
  `create_dir_all`.
- *"for every run, not just 5-Base"* ✅ for aligner runs; since the bullet sits under
  `### bismark (aligner)` the scope reads correctly, though a reader might over-extend it to the
  extractor, which has its own unrelated `--output_dir` handling.

**V7. Out-of-scope content.** Only the `--output_dir` work, which is D2/D3-approved scope, plus the
`mod.rs` `.ok()` unification the plan folds into Task 3. Nothing else. Structurally I'd still prefer
`--output_dir` as its **own commit**: it changes every aligner run, while #1100 changes only
5-Base-with-an-engine-flag runs, and a bisect over the byte-frozen paths benefits from that split. A
recommendation, not an objection — the two are cleanly separated in the diff already.

**V8. Verifications I ran (the plan's §6 manual block, four of five previously unverified).** All against
the real `bowtie2 2.5.5` and a real `bowtie2-build` index of `test_files/pUC19.fa.gz`:

| Check | Result |
|---|---|
| Real `bowtie2-build` file set | exactly the 6 expected `.bt2` files — no arity surprise, no false rejection from extra files |
| §6.1 repro: `--five_base_index /no/such/idx` | exit **1**, one clear error, no `desync`, no `panicked`, **nothing written** to `-o` |
| §6.2 positive control: real index | exit **0**, BAM + PE report written — no false rejection |
| §6.3 env fallback: `BOWTIE2_INDEXES=$G`, bare `puc` | exit **0**, real run completes |
| §6.4 bare basename from cwd, env unset | exit **0**, real run completes |
| §2 row 3: `./puc` with env set | exit **0** — the separator does not disable the fallback ✅ |
| Stray prefix file (`puc_backup.1.bt2`) + env index | exit **0** — we over-accept, aligner succeeds anyway; the `any_index_file_at` doc comment's "harmless direction" claim is **verified** |
| **Partial index missing `.1.bt2` + env index** | **bowtie2 exit 0, bismark exit 1 → C1** |
| Rejected index + nested non-existent `-o` | exit 1, no directory tree created (M3's untested invariant) |
| `--output_dir` notice, nested path | `Created output directory …!` + blank line, dir created |

I did **not** re-run `cargo fmt --check`, the full suite, clippy, the test counts, or the four
sabotage checks — the caller reported those verified and I have no reason to doubt them. Let-chains
(`config.rs:878`, `discovery.rs:209`) are fine: edition 2024, `rust-version = "1.89"`, CI toolchain
1.89, and there is existing precedent at `genome_prep/indexer.rs:75`, `io/read.rs:673`,
`coverage2cytosine/report.rs:269`.

---

## Recommendations, prioritised

| # | Priority | Action |
|---|---|---|
| C1 | **Critical** | Narrow the env-fallback gate from "any expected index file" to `<basename>.1.{bt2,bt2l}` (`first_index_file_at`), mirroring `bowtie2-align-s` rather than the wrapper. Add an invariant clause to `index_suffixes` about `.1` being first. Correct §7's "no false-reject case found" and D1's premise. Existing tests stay green. |
| H1 | **High** | Restore coverage for the `HISAT2_INDEXES` arm (a one-line `index_env_name` assertion suffices), and record the drop in §11's deviation log either way. |
| H2 | **High** | Reconcile CHANGELOG / `illumina-5-base.md` / `cli.rs` help with §2's hisat2 conclusion. Resolve on oxy: does `hisat2-align-s` contain `HISAT2_INDEXES`, and does hisat2 need all 8 files? |
| M1 | Medium | Relabel `Probed:` → `Missing:`; soften the CHANGELOG's "listing every path probed". |
| M2 | Medium | Add an error arm for "env var set but the gate suppressed the fallback". |
| M3 | Medium | Assert a rejected index creates no output tree (nested non-existent `-o` in `five_base_index_missing_fails_early`). |
| L1 | Low | Trim `resolve_output`'s comment to ≤2 lines and drop the `legacy_perl` line numbers; move the four justification comments in `discovery.rs` to the commit message. |
| L2 | Low | Match Perl's trailing slash in the notice, or drop the verbatim-parity claim. |
| L3 | Low | Reword the consensus mkdir error so it doesn't name `--output_dir` when unset. |
| L4–L6 | Low | Cross-reference the duplicated stub helpers; add a `TempDir`-rooted pathological-basename case; skip an empty `index_env`. |

**Merge stance:** hold for C1 (a ~5-line change plus artefact corrections) and a decision on H1/H2.
Everything else is polish that can ride along or follow. The core design — placement, concatenation,
injection, `Validation` variant, fail-loud mkdir — is right and I would not change any of it.
