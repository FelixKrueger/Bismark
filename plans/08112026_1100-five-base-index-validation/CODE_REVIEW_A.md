# CODE REVIEW A — #1100: validate `--five_base_index` at resolve time

**Reviewer:** A (independent, fresh context) · **Date:** 2026-08-11
**Target:** branch `1100-five-base-index-validation`, **uncommitted** working tree over `dev` @ `688d919`
**Files:** `rust/bismark/src/aligner/{discovery,config,mod,cli}.rs`, `rust/bismark/tests/aligner_cli.rs`,
`CHANGELOG.md`, `docs/src/content/docs/rust/illumina-5-base.md`

> **Override noted, as instructed.** The skill's "fix low-risk problems directly" rule was
> **suspended** for this review: three agents are reviewing the same uncommitted tree concurrently, so
> every finding below is a *recommendation*. The only file I wrote is this report. No tracked file was
> touched; experiments ran in a unique scratch dir (`$TMPDIR/cr_A_1100_<pid>_<epoch>`) using the real
> `bowtie2` 2.5.5 at `/opt/homebrew/bin` and the debug binary at `rust/target/debug/bismark`.

---

## Verdict

**Approve.** No Critical and no High issues. The probe is correct: I hunted the false rejection the
plan accepts as its main risk and **did not find one** in 20 adversarial basename forms measured
against real bowtie2. Three Medium items are about *what the error and the docs say*, not about what
the code decides; one of them (M3) is a pre-existing silent-wrong-answer gap that this change is the
natural place to close, offered as a follow-up rather than a blocker.

The two claims the design rests on both survive independent re-derivation, and one of them I can
strengthen beyond what the plan claims (see "Confirmations" below).

---

## Confirmations — what I measured, and what it settles

### 1. The six-file requirement is not over-strict (the D1 premise)

The plan cites "missing `.2`/`.rev.1`/`.rev.2` SIGSEGVs". I deleted **each of the six in turn** from a
real pUC19 index and ran single-end alignment:

| deleted | bowtie2 2.5.5 outcome |
|---|---|
| `.1.bt2` | `Could not locate a Bowtie index corresponding to basename` |
| `.2.bt2` | `Could not open index file …` → **died with signal 11 (SEGV)** |
| `.3.bt2` | `Could not open reference-string index file … for reading` |
| `.4.bt2` | `Could not open reference-string index file …` |
| `.rev.1.bt2` | **SEGV** |
| `.rev.2.bt2` | **SEGV** |

All six are individually required, so the full-set rule cannot false-reject, and it converts three
SEGVs into a clean message. **I also closed the one gap the plan's measurement left open:**
`bowtie2-build --noref` legitimately produces a **four-file** index (`.1`, `.2`, `.rev.1`, `.rev.2` —
documented as sufficient for unpaired alignment), which would have been a real false rejection. It is
not: bowtie2 2.5.5 rejects a `--noref` index even for single-end (`Could not open reference-string
index file noref/puc.3.bt2`). The `-r/--noref` mode is dead for this aligner version.

### 2. `OsString` concatenation vs `File::Spec->catfile` — equivalent

Read the installed wrapper (`/opt/homebrew/bin/bowtie2:379-398`) and enumerated `catfile` against the
Rust `dir + "/" + base` over 5 directory forms × 4 basename forms. `catfile` **canonicalises** where
the Rust concatenation does not (`catfile("/opt","./puc")` = `/opt/puc` vs `/opt/./puc`;
`catfile("/opt/","puc")` = `/opt/puc` vs `/opt//puc`), but every pair names the same file, so
`is_file()` cannot disagree. The sharp cases agree exactly:

- absolute basename: `catfile("/opt","/abs/puc")` = `/opt//abs/puc`; Rust `/opt//abs/puc`. **Identical** — this is the case `Path::join` would have broken, and §3 is right to call it out.
- empty variable: bowtie2 gates on `exists $ENV{...}`, not truthiness, so `BOWTIE2_INDEXES=""` still triggers the fallback with `catfile("","puc")` = `/puc`. Rust `var_os` → `Some("")` → `/puc`. **Identical**, and measured: both reject.

For **hisat2** the match is even tighter — upstream joins with a bare interpolation,
`"$ENV{'HISAT2_INDEXES'}/$idx_basename"`, which *is* literally the Rust concatenation.

### 3. The wrapper's env gate — divergence is one-directional, by construction and by measurement

`any_index_file_at` tests the 12 exact expected names; the wrapper globs the wider
`$basename . "*.bt2{,l}"`. Every name in our set also matches the wider glob, so "we found something
and the wrapper found nothing" is unreachable — the gate cannot false-reject. The reverse (wrapper
finds a prefix-colliding stray, we don't) merely widens our fallback. Measured agreement on the
gate's own case (`stray1+env-complete`).

### 4. `HISAT2_INDEXES` is broken upstream, exactly as §2 claims

Fetched `DaehwanKimLab/hisat2:hisat2` — line 385 is
`glob("$ENV{'HISAT2_INDEXES'}/$idx_basename" . "ht2{,l}")`: **no `*`, no `.`**, so it looks for a file
literally named `<dir>/<basename>ht2` and can never match a real `puc.1.ht2`. The plan's reading is
verbatim correct, and so is its conclusion that symmetric treatment can only over-accept. See **M2**
for the documentation consequence.

### 5. The HISAT2 arity of 8 is not an open question

The plan leaves "does hisat2 require all 8 `.ht2`?" open for oxy. It is answerable **from the oracle**:
`legacy_perl/bismark:7739/7750/7769/7779` enumerate `BS_{CT,GA}.{1..8}.{ht2,ht2l}` explicitly. The
probe reuses Bismark's own long-shipped constant, so this is not new risk — the oxy trip is optional
confirmation, not a gate.

### 6. The invariant the probe rests on, verified independently

No `set_current_dir` and no `Command::current_dir` anywhere in `rust/bismark/src/` except
`genome_prep/indexer.rs:115` (unrelated), and no `env_clear`/`env_remove` on any spawn. So a relative
basename resolves identically at probe time and exec time, and the child inherits the same
`BOWTIE2_INDEXES` that `resolve()` read.

### 7. Ordering and the "nothing written" contract

Check at `config.rs:874-882`, between `discover_genome_for_run` (`:861`) and `detect_aligner` (`:890`)
— exactly where §3 requires. Measured: the #1100 repro emits **one** error and the output directory is
**not created at all**. `resolve_output` (`:913`) is the **last fallible step** in `resolve()` (which
ends at `:995`), so no error inside `resolve()` can leave a freshly-created empty directory behind.
The standalone BAM modes return at `mod.rs:179/184` **before** `resolve()`, so a stale
`--five_base_index` on a `--five_base_bisulfite_bam` command line is *not* newly rejected — correct,
since nothing aligns.

### 8. Restore integrity (`discovery.rs` was wiped and reconstructed — iteration log #2)

**Clean.** Six new items (`index_files_for`, `first_missing_at`, `any_index_file_at`,
`index_env_name`, `index_env_for`, `validate_unconverted_index`), the `stub_index` test helper, and 10
new tests. No duplicated function, no missing function, **no orphaned doc comment or attribute**, no
dead code, no leftover sabotage. `first_missing` and `first_missing_at` coexist deliberately (§3's
accepted two-line duplication). Compiles and `aligner::discovery` runs **35 passed / 0 failed**.

The `aligner_cli.rs` attribute hazard from iteration log #1 is also clear:
`five_base_bowtie2_unconverted_index_end_to_end` retains its doc comment **and** its
`#[cfg(unix)] #[test]`, and each of the five new tests carries its own.

`cargo fmt -p bismark -- --check` → **exit 0**, asserted on the command's own status (iteration log
#4's lesson applied, not just recorded).

*Note, not a defect:* the plan's "`aligner::discovery` 25 → 36" is the **Linux** count.
`non_utf8_filename_not_dropped` is `#[cfg(target_os = "linux")]`, so macOS sees 35. The PLAN figure
reproduces only on CI.

### 9. The false-rejection sweep — 20 forms, zero false rejections

Each row compares real `bowtie2 -x <base>` (does it align?) against the debug binary's verdict (does
`is not a complete` fire?):

| form | bowtie2 | probe | |
|---|---|---|---|
| relative complete · absolute complete · `..`-relative (`full/../full/puc`) · double slash | ACCEPT | ACCEPT | agree |
| bare + `$BOWTIE2_INDEXES` · **`./puc` + env** (§2's key finding) · absolute + env set · env with trailing slash | ACCEPT | ACCEPT | agree |
| large-only (`.bt2l`) index | ACCEPT | ACCEPT | agree |
| whitespace in the index path | ACCEPT | ACCEPT | agree |
| full filename as basename (`puc.1.bt2`) · trailing dot · directory as basename | reject | reject | agree |
| env set but **empty** · `nope/puc` + env · colon-separated env list | reject | reject | agree |
| **partial (1 of 6) at basename + complete under env** (the gate) | reject | reject | agree |
| glob metacharacters (`pu[c]`), with and without env | reject | reject | agree |

**No `*** FALSE REJECTION ***` row.** The risk §7 bounds by measurement is, as far as I can drive it,
genuinely bounded.

---

## Issues by area

### Logic

**M1 (Medium) — the gate-blocked env fallback produces an error that misdirects the user.**
`env_note` fires only when the variable is **unset**, which is the case that needs it *least*. The
case that needs it most is silent. Measured, from a cwd holding one stray `puc.1.bt2` with
`BOWTIE2_INDEXES` pointing at a complete index:

```
error: --five_base_index puc is not a complete Bowtie 2 index. Probed: puc.2.bt2, puc.1.bt2l.
Build a NORMAL (unconverted) index once with bowtie2-build.
```

Nothing says `$BOWTIE2_INDEXES` is set, that it was **deliberately** not consulted, or which file
suppressed it — and the closing advice is wrong: the user *has* a complete index, in the directory
they configured. This is precisely the configuration bowtie2 turns into a SEGV, so it is a case users
reach, and the new check's whole value proposition is explaining it better than the aligner does.

*Recommend* a second note for the gated branch, e.g. when `index_env.is_some()` and
`any_index_file_at` was true: `($BOWTIE2_INDEXES is set but was not used: <file> matches the basename,
and the aligner only falls back when nothing does)`. The information is already in hand at that point
— `any_index_file_at` would need to return the matching path instead of a `bool`.

**M3 (Medium, pre-existing — offered as follow-up) — a bisulfite-CONVERTED index is silently
accepted.** Measured: `--five_base_index <genome>/Bisulfite_Genome/CT_conversion/BS_CT` passes the
check with no complaint. The run then aligns raw 5-Base reads against a C→T-converted genome and
produces systematically wrong methylation calls **with no error at any stage** — the one
silent-wrong-answer path in this area, and strictly worse than the truncated BAM #1100 set out to fix.

The check already knows what it wants (its own message says "Build a NORMAL (unconverted) index") and
the call site already holds `genome.ct_index_basename` / `ga_index_basename` from `:861`, so a guard is
a few lines. I am **not** calling this a blocker: it predates the branch and closing it is scope
growth. But this change is the natural home for it, and it is worth a decision rather than silence.

### Errors / diagnostics

**L1 (Low) — "every path probed" is an overclaim.** `probed` collects the **first missing file per
arm** — 2 entries normally, 4 when the env arm engages — out of up to 24 stats. The message itself
("Probed: …") is defensible, since each listed path *was* probed; the CHANGELOG's "listing every path
probed" and PLAN §3.4's "every path probed" are not. *Recommend* rewording the CHANGELOG to e.g.
"naming the flag and the paths it looked for".

**L2 (Low) — probed paths print relative.** `Probed: puc.2.bt2` gives a reader no idea *where* it
looked, and since the probed list is the whole diagnostic, that undercuts it. bowtie2 is no better,
but we are trying to be. *Recommend* absolutising the probed paths (or naming the cwd once).

**L6 (Low) — `mod.rs`'s newly fail-loud `create_dir_all` has no test.** `run_five_base_consensus_standalone`
went from `.ok()` (silent) to `map_err → Validation`. Strictly better and correctly *not* covered by
`resolve_output` (this path returns at `mod.rs:179`, before `resolve()`, so the duplication is
genuine), but nothing asserts it. Low because the failure mode it replaces was worse.

### Structure

**L3 (Low) — Task 1's planned `--large-index` note is missing from the code.** The claim is *true* —
I verified `five_base_build_argv` (`mod.rs:1565-1571`) emits only `-x`/`-1`/`-2`, with no passthrough
that could inject `--large-index` — but the sentence Task 1 asked for is not in `discovery.rs`, so a
maintainer reading `validate_unconverted_index` has no in-place answer to "why do two arms cover every
case?". One line, e.g. `/// Two arms suffice: this route never emits --large-index.`

**L4 (Low) — `index_files_for` leans on an unwritten `index_suffixes` contract.** It calls
`index_suffixes(aligner, "", large)` to harvest *bare suffixes*, which works only because the format
string is `{stem}.{s}.{ext}`. `index_suffixes`' own doc comment describes it as producing file names
for a stem and says nothing about the empty-stem case, so a future refactor (a `Path::join`, a prefix,
a `format!` reorder) breaks `index_files_for` silently. Per CLAUDE.md's "put an invariant on the thing
it constrains", the line belongs on `index_suffixes`, not on the caller.

### Tests

**L5 (Low, for the coverage audit rather than a change) — the two positive integration tests cannot
fail if the check is deleted.** `five_base_index_resolved_via_env_is_accepted` and
`five_base_index_resolved_from_cwd_is_accepted` assert `!stderr.contains("--five_base_index")`, which
is vacuously true with no check at all. Their real content is `.assert().success()`, and the sabotage
record shows the call-site seam *is* pinned (passing `None` for `index_env` went red), so coverage is
adequate in aggregate — but the assertion itself is one-directional and reads stronger than it is.

Hermeticity of the new tests is otherwise good: `five_base_index_resolved_from_cwd_is_accepted` does
`.env_remove("BOWTIE2_INDEXES")`, and the tests that omit it pass absolute basenames where an ambient
variable cannot change the verdict. All five set `--temp_dir` where they proceed far enough to convert,
so none leaks conversion artefacts into the tree.

### Efficiency

Nothing to raise. The probe costs ≤ 24 `stat` calls and ≤ 8 small `Vec` allocations, once per run,
on a path that is about to spawn an aligner. `any_index_file_at` re-derives lists that
`first_missing_at` already built, which is the right trade for keeping each function's meaning
single-purpose.

### Documentation

**M2 (Medium) — three places promise `$HISAT2_INDEXES` works; upstream it cannot.** Confirmed from
hisat2's source (see Confirmation 4). The claims:

- `CHANGELOG.md`: "exactly as the bowtie2 and hisat2 wrappers do — a retry under `$BOWTIE2_INDEXES` / `$HISAT2_INDEXES`"
- `illumina-5-base.md`: "As with bowtie2 and hisat2 themselves, a basename that is not found is also looked up under …"
- `cli.rs:120-121`: "as the aligner itself does — also resolved via `$BOWTIE2_INDEXES` / `$HISAT2_INDEXES`"

A user who reads any of these, sets `HISAT2_INDEXES`, and passes a bare basename will clear **our**
check and then fail inside hisat2. The *code* decision (stay symmetric, over-accept, be correct the
day upstream fixes the glob) is right and I would not change it. The *claim* is what needs scoping —
e.g. keep the unqualified sentence for bowtie2 and note that Bismark honours `$HISAT2_INDEXES` even
though hisat2's own lookup is currently broken.

**L7 (informational) — the notice is byte-identical to Perl's, with two cosmetic divergences.**
Perl: `warn "Created output directory $output_dir!\n\n"` (`legacy_perl/bismark:8191`). Rust:
`eprintln!("Created output directory {}!\n", …)` — same stream (stderr), same trailing blank line.
Two things Perl does that Rust does not: it appends a trailing `/` to the path first (so Perl prints
`… directory out/!`), and it also emits `Output will be written into the directory: …` in *both*
branches. Neither is byte-gated and neither is worth changing; recording it so the parity claim is not
overread.

Everything else in the docs checks out: the CHANGELOG bullets landed under `## Unreleased` →
`### bismark (aligner)`; the docs paragraph sits naturally in the prose ahead of the example block;
the quoted legacy failure (`failed to open BAM … No such file or directory`) matches the real write
path at `mod.rs:2801`/`4795`; and `--output_dir` pointing at an existing *file* gives a clean
flag-named error (`--output_dir /…/afile: File exists (os error 17)`), which is also what Perl's `$!`
would render.

**L8 (informational) — the check is necessary, not sufficient; the CHANGELOG reads slightly stronger.**
Four measured forms pass our probe and can still fail inside the aligner: zero-byte index files (by
design — it is what the fixtures rely on), prefix-colliding strays that block bowtie2's wider glob,
glob metacharacters in the basename, and whitespace in the index path. All four fall through to
today's late error, so every divergence really is in the benign direction, exactly as §7 claims — but
"checked at start-up" invites the reading that acceptance guarantees the aligner will agree. No change
needed; worth knowing.

---

## Recommendations, prioritised

| # | Priority | Recommendation |
|---|---|---|
| **M1** | **Medium** | Add a note for the **gate-blocked** env fallback — the one failure this change can produce whose message actively misdirects. Needs `any_index_file_at` to return the matching path rather than a `bool`. |
| **M2** | **Medium** | Scope the `$HISAT2_INDEXES` claim in `CHANGELOG.md`, `illumina-5-base.md` and `cli.rs`. Keep the code symmetric; stop promising a lookup hisat2 cannot perform. |
| **M3** | **Medium** (follow-up) | Decide on rejecting a **bisulfite-converted** basename for `--five_base_index`. Currently accepted silently, and the consequence is wrong calls rather than a failed run. Pre-existing; a guard is a few lines at the existing call site. |
| **L1** | Low | Reword the CHANGELOG's "listing every path probed" — the message lists the first missing file per arm. |
| **L2** | Low | Absolutise the probed paths in the error (or name the cwd once). |
| **L3** | Low | Add Task 1's missing one-line note that this route never emits `--large-index`, so two arms suffice. |
| **L4** | Low | Put the empty-stem invariant on `index_suffixes`, which `index_files_for` silently depends on. |
| **L5** | Low | Note in the coverage audit that the two positive integration tests are one-directional; the seam is pinned by sabotage, not by them. |
| **L6** | Low | Consider a test for `mod.rs`'s newly fail-loud `create_dir_all`. |
| **L7/L8** | Info | No action. Recorded so the Perl-parity and "checked at start-up" claims are not overread. |

## Fixes applied

**None** — per the override at the top of this report. All findings above are recommendations; the
only file written was this report.
