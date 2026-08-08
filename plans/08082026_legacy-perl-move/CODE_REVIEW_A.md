# Code Review A — `legacy-perl-move` (PR #1098 → `dev`)

**Reviewer:** A (independent, fresh context)
**Date:** 2026-08-08
**Diff reviewed:** `git diff dev...legacy-perl-move` — `dea61f9` (pure move) + `19cb7db` (path fixes)
**Spec:** `plans/08082026_legacy-perl-move/PLAN.md` rev 2
**Toolchain on this box:** rustc 1.95.0 (CI pins 1.89)

## Verdict

**APPROVE** — with two Medium items worth folding in before merge (neither blocks correctness of
the move) and six Low/cosmetic items.

The change does what the plan says. Every path edit is correct, the consumer inventory is
complete (I re-derived it independently rather than ticking the plan's tables), the layout gate
asserts the right invariant on the right file set, and the workflow edit cannot break inter-step
references. I re-ran fmt, clippy, the layout test, and the full oracle battery: all green.

The one thing I could **not** reproduce is §12's V2 ok-count (I measure 72, §12 records 73) —
see M1. The substantive invariant behind V2 does hold; it is the *count* as evidence that
doesn't reproduce.

---

## What I verified independently

Not a restatement of the plan — these are checks I ran myself.

### The move itself

- All 24 moved paths are `R100` (byte-identical renames) under `git diff --name-status -M`.
  Nothing was edited in transit.
- Mode bits preserved in the index: 13 × `100755` (the 12 executables +
  `copy_bismark_files_for_release.pl`), 11 × `100644` (plotly assets + `test_data.fastq`).
- Root listing is now directories + top-level docs + the two Python helpers + `license.txt`
  — the stated goal is met.

### Path edits — arithmetic checked by execution, not by counting dots

I `cd`'d each golden script's directory and ran its literal up-chain. All resolve to
`/Users/fkrueger/Github/Bismark`:

| Script | ups | resolves to |
|---|---|---|
| `rust/bismark/tests/data/bam2nuc/generate_goldens.sh:26` | 5 | repo root ✓ |
| `.../coverage2cytosine/phase1/generate_goldens.sh:13` | 6 | repo root ✓ |
| `.../coverage2cytosine/phase2_drach/generate_goldens.sh:15` | 6 | repo root ✓ |
| `.../coverage2cytosine/phase3_ffs/generate_goldens.sh:12` | 6 | repo root ✓ |
| `.../coverage2cytosine/phase_b/generate_goldens.sh:5` | 6 | repo root ✓ |
| `.../filter_nonconversion/generate_goldens.sh:28` | 5 | repo root ✓ |
| `.../nome_filtering/phase_b/generate_goldens.sh:9` | 6 | repo root ✓ |

Note the implementation fixed **seven** scripts; the plan's table (d) listed only six. The extra
one (`phase3_ffs`) is a correct catch — see L5 on the record-keeping.

`filter_nonconversion/generate_goldens.sh:28` is also a genuine improvement beyond a path fix: it
replaces a hardcoded `/Users/fkrueger/...` absolute default with a computed root, while keeping
the `PERL_FNC` override. The lazy `${VAR:-$(...)}` form means the `cd` only runs when the override
is absent, and under `set -e` a failing `cd` aborts rather than silently yielding
`/legacy_perl/filter_non_conversion`.

### Consumer inventory — re-derived, not trusted

I swept for every idiom that can reach a moved file, with `command grep`:

- **Every** `join("../..` in `rust/**` (`*.rs`, excluding `target/`): 13 hits. Eleven are
  `legacy_perl/`-prefixed; the other two are `../../docs/images/bismark_summary_report.html`
  (`summary_stale_oracle_tripwire.rs:15`) and `../../test_files`
  (`aligner_five_base_groundtruth.rs:729`) — both correctly untouched, since `docs/` and
  `test_files/` stay at root.
- `repo_root()` exists once (`report_perl_vs_rust.rs:23`) and has exactly one call site (`:64`),
  which was updated. No other indirect helper joins a moved name.
- `scripts/` and `rust/**/*.sh`: the `$REPO_ROOT`/`$HERE`/`$SCRIPT_DIR`/`&& pwd)` prefix sweep
  returns **zero** non-`legacy_perl` hits. Two further harnesses carry a Perl-binary default —
  `scripts/overnight_driver.sh:37,38` and `scripts/bench_run.sh:45` — but they point at
  `$HOME/micromamba/envs/bismark-test/bin/...`, i.e. outside the repo, so they are move-immune and
  correctly left alone. Everything else matching a script name in `scripts/`+`validation/` is
  prose.
- No Rust **production** code resolves a Perl script on the filesystem. The
  `"bismark2bedGraph"` / `"coverage2cytosine"` / `"bam2nuc"` string literals in
  `src/extractor/downstream_filenames.rs`, `src/bam2nuc/cli.rs` etc. are argv[0] values and
  in-process tool labels, not lookups.
- `docs/`, `CONTRIBUTING.md`, `_config.yml`, `validation/`, `docker/`: zero root-location claims
  and zero path-like references to moved files. The `test_data.fastq` mentions at
  `docs/src/content/docs/installation.md:99,105` are a v0.7.8 historical example report over a
  *downloadable* dataset (line 88 says so explicitly) — the move does not invalidate them, so
  Assumption 2 holds and no docs edit was needed.
- `.gitignore` contains only `.cache` and `.DS_Store` — nothing shadows `legacy_perl/`.

**Conclusion: no consumer was missed.**

### The 12 Perl executables need no edits — confirmed at source

`command grep -nE 'RealBin|FindBin|__FILE__|use lib'` across all 13 Perl files gives exactly:
`bismark:941` → `$RealBin/bam2nuc`; `bismark_methylation_extractor:377,424` →
`$RealBin/bismark2bedGraph` / `$RealBin/coverage2cytosine`; `bismark2report:1029` and
`bismark2summary:140` → `$RealBin/plotly/$template`. All five targets moved together, so all five
still resolve. Zero edits required. ✓

`use lib "$RealBin/../lib"` (4 sites) changes referent from `<parent-of-repo>/lib` to
`<repo>/lib`. Neither exists (checked both), so it stays a no-op — and post-move it points
*inside* the repo, which is the more controlled of the two. Observation only, no action.

### The layout gate asserts the right file set

I traced which plotly assets the Perl code actually opens, rather than trusting the list.
`read_report_template()` is called with exactly `plotly_template.tpl`, `plot.ly`,
`bismark.logo`, `bioinf.logo` (`bismark2report:59,63,73,78`) and with three of those four
(`bismark2summary:127,128,129`). The test's four names are therefore **exactly** the code-read
set — complete, with nothing superfluous. Correctly omits `test_data.fastq` and
`copy_bismark_files_for_release.pl`, which no code consumes.

`CARGO_MANIFEST_DIR` for this crate is `rust/bismark`, so `../../legacy_perl` is the repo-root
directory. Test passes; it is unconditional and has no skip path.

Worth calling out as a real improvement: `genome_prep_integration.rs:36` does
`.canonicalize().unwrap()`, so a missing script there panics with an opaque `Os { code: 2 }`
rather than routing through `skip_or_panic`. The new layout gate now fires first with a named
diagnostic. That is exactly the value the gate was meant to add.

### `ci_tests.yml` cannot have broken an inter-step reference

- The job (`BismarkCI`) has **no** `working-directory` default, so every step runs at the
  checkout root and `./legacy_perl/<name>` resolves. ✓
- 44 invocations converted; `command grep -nE '\./(bam2nuc|bismark|bismark2|bismark_|coverage2|deduplicate|filter_non|NOMe|methylation_c)' … | grep -v legacy_perl`
  returns **zero**. Independently counted: `grep -c 'legacy_perl/'` = 44. ✓
- Only the *program* path gained a prefix. All output references stay bare
  (`test_R1_bismark_bt2_pe.bam`), `--genome ./test_files/` is untouched, and
  `bismark2bedGraph`'s `CpG_*` / `CHG_*` / `CHH_*` globs still expand in the job CWD where the
  extractor wrote them. `bismark2report` / `bismark2summary` discover inputs from CWD, not
  `$RealBin`. Nothing in the chain moved.

### `perl-oracle` job is not perturbed — the near-miss I went looking for

The new test is named `legacy_perl_toolchain_is_present`, which **contains the substring
`perl`**. If `rust_ci.yml`'s oracle job filtered on a substring, `EXPECTED=13` would now see 14
and the job would fail. It does not: `rust_ci.yml:207-222` passes `--exact` with 13 explicitly
spelled test names, so a non-listed name cannot match. `EXPECTED=13` stays valid, and the
`grep -q '^skipping:'` detector is unaffected. ✓

### `.gitattributes` preserves linguist behaviour

`plotly/**` contains a slash, so it was root-anchored and would have gone dead — the rewrite to
`legacy_perl/plotly/**` is the correct fix. Independently, `**/plotly/**` on the next line
matches at any depth (leading `**/` includes the top level), so vendoring was never at risk even
mid-transition. Assumption 10 confirmed. `*.ly`, `*.html`, `docs/**` unchanged and unaffected.

`.prettierignore` correctly **not** edited: `plotly/` has no internal slash, so under gitignore
semantics it matches a `plotly` directory at any depth, including `legacy_perl/plotly/`. (Moot in
CI anyway — there is no prettier job.)

### `.dockerignore` exclusion is safe

`Dockerfile:29` is `COPY . .` and the builder then builds `rust/`. The only other `COPY`s are
`rust/target/release/bismark`, `docker/bismark-canonical-wrapper.sh`, `license.txt`,
`rust/THIRD-PARTY-NOTICES.md` — none under `legacy_perl/`. The runtime image's Perl-*named*
binaries are Rust multicall symlinks created in `release.yml:131-134,165-168` from bare names, so
nothing in the image ever needed the Perl scripts. `release.yml` correctly needed no edit (it
packages `license.txt` from root, which stays). ✓

### The packager really was already dead

`copy_bismark_files_for_release.pl:24` wants `Docs/make_docs.pl` and `Bismark_User_Guide.html`;
neither exists in the tree (checked). So loop 3 already `die`s today. Post-move it dies earlier —
`@files` at `:20` includes root-resident `CHANGELOG.md`, `license.txt`,
`Bismark_alignment_modes.pdf`, which are not siblings of the script any more, so loop 1 fails.
Accepting it broken (Open-5) introduces no regression. ✓

### Gates re-run

| Gate | Command | Result |
|---|---|---|
| fmt | `cargo fmt --all -- --check` | clean (exit 0) |
| clippy | `cargo clippy -p bismark --all-targets -- -D warnings` | clean |
| Layout gate | `cargo test -p bismark --test legacy_perl_layout` | 1 passed |
| Full battery | `BISMARK_REQUIRE_PERL=1 cargo test -p bismark --no-fail-fast -- perl_vs_rust oracle_ byte_identical match_repo_plotly_files matches_perl_heredoc legacy_perl_toolchain` | **72 ok / 0 failed** |

Population present **by name** in the battery (this is the check that matters, not the total):
all 13 `perl_vs_rust_*`; all 5 report oracles (`pe_full_companions_byte_identical`,
`se_r1_only_mbias_byte_identical`, `nondirectional_unknown_context_byte_identical`,
`minimal_alignment_only_byte_identical`, `crlf_alignment_byte_identical`); all 12 `oracle_*`
summary oracles; `embedded_template_matches_perl_heredoc`;
`report::assets::tests::embedded_assets_match_repo_plotly_files`;
`summary::assets::tests::vendored_assets_match_repo_plotly_files`;
`legacy_perl_toolchain_is_present`. Zero libtest skips — the four `skipping` hits in the log are
Perl `bismark2report` chatter (`No deduplication report present, skipping...`), matching §12.

---

## Issues

### Logic

**M1 (Medium) — V2's recorded ok-count does not reproduce; the count is weak evidence anyway.**
§12 records `73 ok = N_baseline + 1` (baseline 72). My run of the identical 6-term battery on this
branch yields **72 ok / 0 failed**. I cannot check out `dev` to re-measure the baseline
(report-only, parallel reviewer active), so I cannot say whether the delta is +1 here.

Why this is not a correctness problem: I verified the entire Perl-dependent population *by test
name* (list above) — 34 oracles/guards, all present, all passing, zero libtest skips. Nothing
switched off.

Why it is still worth fixing: the V2 gate is a bare `grep -c`, and the filters `oracle_` and
`byte_identical` incidentally match ~38 non-Perl unit tests (`aligner::combined::tests::
mechanism_matches_oracle_*`, `parallel_*_byte_identical`, …). That population shifts with
toolchain, `cfg`, and feature flags, so the absolute number is not a stable invariant and a future
"count changed" alarm will be noise. Recommendation: restate V2 name-wise (assert the 34 expected
names are present and ok) or re-record V0/V2 back-to-back on one toolchain and note the toolchain
in §12. Either way, **document that 72 was observed on rustc 1.95.0** so the next reader isn't
chasing a phantom regression.

**M2 (Medium) — the layout gate checks existence but not the execute bit, which 44 CI steps
depend on.** `rust/bismark/tests/legacy_perl_layout.rs` asserts `.exists()` only. But
`ci_tests.yml` runs the scripts as `./legacy_perl/<name>` — mode `+x` is load-bearing for all 44
invocations. A change that keeps the files but drops the bit (a `core.fileMode=false` checkout, a
`cp` in some future packaging step, a Windows round-trip) leaves this gate green and surfaces only
as `Permission denied` deep in `ci_tests.yml`. The gate exists precisely to catch layout drift
before it reaches there, so this is the one property it should cover and doesn't.

Suggested replacement for the whole file:

```rust
// Layout gate for the byte-identity oracles: unconditional, no tooling needed, cannot skip.
#[test]
fn legacy_perl_toolchain_is_present() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../legacy_perl");
    // ci_tests.yml runs these as ./legacy_perl/<name>, so the execute bit is load-bearing.
    for f in [
        "bam2nuc",
        "bismark",
        "bismark2bedGraph",
        "bismark2report",
        "bismark2summary",
        "bismark_genome_preparation",
        "bismark_methylation_extractor",
        "coverage2cytosine",
        "deduplicate_bismark",
        "filter_non_conversion",
        "methylation_consistency",
        "NOMe_filtering",
    ] {
        let p = dir.join(f);
        assert!(p.exists(), "legacy_perl/{f} missing — layout assumption broken");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert!(mode & 0o111 != 0, "legacy_perl/{f} lost its execute bit (mode {mode:o})");
        }
    }
    for f in [
        "plotly/plot.ly",
        "plotly/plotly_template.tpl",
        "plotly/bismark.logo",
        "plotly/bioinf.logo",
    ] {
        assert!(dir.join(f).exists(), "legacy_perl/{f} missing — layout assumption broken");
    }
}
```

### Errors

Nothing. No broken step, no dead pattern, no unsafe exclusion. The four specific hazards the
brief asked about — workflow CWD, `.gitattributes` linguist behaviour, `.dockerignore` safety, and
whether any consumer was missed — all check out clean, with evidence above.

### Efficiency

Nothing wasteful introduced. One trivial existence test added; Docker context shrinks. The one
note (L3) is a redundant `.gitattributes` line the plan chose deliberately.

### Structure / naming / comments

**L1 (Low) — drift-guard assert messages name a path that no longer exists.** Both now compare
against `legacy_perl/plotly/` but report drift against `plotly/`:

- `rust/bismark/src/report/assets.rs:134` — `"embedded {name} drifted from plotly/{name}"`
  → `"embedded {name} drifted from legacy_perl/plotly/{name}"`
- `rust/bismark/src/summary/assets.rs:118` — `"vendored {name} drifted from plotly/{name}"`
  → `"vendored {name} drifted from legacy_perl/plotly/{name}"`

**L2 (Low) — `.gitattributes:10` breaks the file's hand-alignment.** Every other line pads the
attribute to column 20; the new line uses a single space. Either pad the block to a new column or
accept — cosmetic, but this file is visibly hand-aligned.

**L3 (Low) — `.gitattributes:10` is now strictly redundant with `:11`.** `**/plotly/**` already
matches `legacy_perl/plotly/**` at any depth. The plan calls this deliberate belt-and-braces
(Assumption 10) and I have no objection — but if it stays, it deserves the one-line reason inline,
since the next reader will otherwise delete one of the two.

**L4 (Low) — `rust/README.md:102` emphasises the layout the reader won't see.** The sentence
instructs "download the [v0.25.1 release] (or `git checkout v0.25.1`)", and a reader who does that
finds the scripts at the **repo root** — which the sentence demotes to a parenthetical after
asserting they "live in `legacy_perl/`". Inverting it makes the sentence true for the action it
describes:

> Or run it from source: download the [v0.25.1 release](…) (or `git checkout v0.25.1`) — the Perl
> scripts (`bismark`, `deduplicate_bismark`, `bismark_methylation_extractor`, …) sit at the root
> of that tag (on the current `dev` branch they live in `legacy_perl/`) and need **Perl** + a
> **Bowtie 2 / HISAT2** backend + **samtools** on `PATH`.

`README.md:60` does not have this problem — its link makes the referent explicit.

**L5 (Low) — §12's record understates the change and overstates the sampling.** The
implementation fixed seven golden scripts; the plan's table (d) listed six, and §12 deviation 6
says "all **four** sampled resolve". So `phase3_ffs` was fixed but never recorded as an addition,
and 3 of 7 were never spot-checked. All seven do resolve correctly (I verified each individually),
so this is bookkeeping, not a defect — but the deviation list should name the seventh file as a
plan gap the implementation closed.

**L6 (Low, informational) — the new test hard-fails outside a full repo checkout.** `bismark` is
published to crates.io with no `include`/`exclude` in `rust/bismark/Cargo.toml`, so `tests/` ships
in the tarball and a downstream `cargo test` would find no `../../legacy_perl`. This is a
**pre-existing class**, not new: both drift guards already `read_to_string("../../plotly/…").unwrap()`
and would panic identically on `dev`. The PR broadens the assumption rather than creating it, and
`cargo package`'s verify build does not run tests, so nothing in the release path is affected. If
it is ever worth closing, gate on a *workspace marker* (e.g. `../Cargo.toml` exists) — never on
`legacy_perl/` itself, which would destroy the unskippability that is the whole point of the gate.

---

## Recommendations, prioritised

**Critical:** none.

**High:** none.

**Medium**
1. **M2** — add the `#[cfg(unix)]` execute-bit assertion to `legacy_perl_layout.rs` (patch above).
   Cheap, and it closes the one property the gate's own consumer (`ci_tests.yml`) actually needs.
2. **M1** — record the toolchain against §12's ok-counts and note that 72 was independently
   observed on rustc 1.95.0; longer term, restate V2 as a name-presence assertion rather than a
   `grep -c`.

**Low**
3. **L1** — fix the two drift-guard assert messages (`report/assets.rs:134`,
   `summary/assets.rs:118`).
4. **L4** — invert the emphasis in `rust/README.md:102`.
5. **L5** — name `phase3_ffs/generate_goldens.sh` in §12 as a plan gap the implementation closed,
   and correct "all four sampled" to seven verified.
6. **L2 / L3** — `.gitattributes` alignment, and a one-line reason for keeping the redundant
   anchored pattern.
7. **L6** — no action; worth a line in §13 follow-ups so the constraint is written down.

None of the above changes behaviour. The move is sound and I would merge it with M2 folded in.
