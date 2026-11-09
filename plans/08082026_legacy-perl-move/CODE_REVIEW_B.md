# Code Review B — `legacy_perl/` move (PR #1098, branch `legacy-perl-move`)

**Reviewer:** B (independent, fresh context)
**Date:** 2026-08-08
**Diff reviewed:** `git diff dev...legacy-perl-move` — `dea61f9` (pure `git mv`) + `19cb7db` (path fixes + layout gate)
**Spec:** `plans/08082026_legacy-perl-move/PLAN.md` rev 2

## Verdict

**APPROVE.**

The move is mechanically correct and the consumer inventory is complete. I re-derived the
inventory from scratch (independent repo-wide sweeps, not the plan's lists) and found **no
missed consumer**. I re-checked every path edit's arithmetic empirically — all seven shell
scripts and all ten Rust literals resolve to the intended target. I ran the oracle battery,
both plotly drift guards, and the new layout gate: all green, with zero `skipping` lines. The
implementation also fixes **one consumer the plan's inventory missed** (a 7th golden script).

Everything I found is polish: one user-facing doc sentence that states the location backwards
(Medium), and five Low items (a redundant/ragged `.gitattributes` line, two stale comment
phrases, a doubled `dirname` computation, a dropped word in help text, and a plan-notes count
that is now understated). None of them blocks the merge; all of them are safe to fold in as a
single follow-up commit on this branch if the caller wants.

---

## What I verified independently (evidence)

### Path arithmetic — all correct

Resolved every rewritten up-count to an absolute path on this machine. All seven land on
`/Users/fkrueger/Github/Bismark`:

| Script | Rewritten expression | Ups | Resolves to |
|---|---|---|---|
| `rust/bismark/tests/data/bam2nuc/generate_goldens.sh:26` | `$SCRIPT_DIR/../../../../..` | 5 | repo root ✓ |
| `…/coverage2cytosine/phase1/generate_goldens.sh:13` | `$HERE/../../../../../..` | 6 | repo root ✓ |
| `…/coverage2cytosine/phase2_drach/generate_goldens.sh:15` | same | 6 | repo root ✓ |
| `…/coverage2cytosine/phase3_ffs/generate_goldens.sh:12` | same | 6 | repo root ✓ |
| `…/coverage2cytosine/phase_b/generate_goldens.sh:5` | `$(dirname "$0")/../../../../../..` | 6 | repo root ✓ |
| `…/filter_nonconversion/generate_goldens.sh:28` | `$(dirname "$0")/../../../../..` | 5 | repo root ✓ |
| `…/nome_filtering/phase_b/generate_goldens.sh:9` | `$(dirname "$0")/../../../../../..` | 6 | repo root ✓ |

`CARGO_MANIFEST_DIR/../..` (`rust/bismark` → repo root) also confirmed, which is the basis for
all ten Rust literals.

The pre-existing breakage the plan describes is real and is repaired: before this branch these
scripts resolved one level short, at `…/Bismark/rust`.

### Consumer inventory — complete, plus one the plan missed

Independent sweep of every `(../)+`-style join across all of `rust/`:

```
rust/bismark/tests/summary_stale_oracle_tripwire.rs:15   ../../docs/images/…   (unmoved)
rust/bismark/tests/aligner_five_base_groundtruth.rs:729  ../../test_files      (unmoved)
rust/bismark/tests/report_perl_vs_rust.rs:25             ../..  (repo_root())
… all 11 remaining hits are legacy_perl/-prefixed
```

The three plan idiom greps return only `legacy_perl`-prefixed hits (`.rs` class: just the two
deliberately-kept historical comments at `report/assets.rs:7` and `summary/assets.rs:18`),
zero for the `.sh` class, and zero non-prefixed invocations in `ci_tests.yml`.

**The plan's inventory is one short.** §5(d) enumerates **6** golden scripts; the diff
correctly also fixes a **7th** —
`rust/bismark/tests/data/coverage2cytosine/phase3_ffs/generate_goldens.sh:12` — with the same
5→6 up-count repair plus `legacy_perl/`. Verified resolving. Good catch by the implementer,
but see Low-6 below: §12 doesn't record it, so the plan still says "6 files" / "23 code sites"
when the true figures are 7 / 24.

Also confirmed clean by direct inspection, not by the plan's word:
`rust_ci.yml` (**zero** moved filenames; its `perl-oracle` `--exact` list is test *names*),
`release.yml` (its 11 hits are the Rust multicall symlink-name loops at `:131-134`/`:165-168`
and the `--help` smoke at `:337-338`; `cp license.txt` at `:137` reads repo root, and
`license.txt` stays), `docs.yml`, `ga-candidate-image.yml`, `link_closing_pr.yml`,
`docker/bismark-canonical-wrapper.sh`, `validation/`, `RELEASE_CHECKLIST*.md`, `_config.yml`,
`CONTRIBUTING.md`. `rust/bismark-{io,extractor,bedgraph}/` are untracked leftovers
(`git ls-files` = 0 each) with zero references.

### `ci_tests.yml` — no step broken

44 invocations, all converted, zero stragglers (`grep -c legacy_perl/` = 44; the unanchored
non-prefixed grep returns nothing, including the inline `:37` `run:` the plan warned about).

The important question is whether relocating the *executable* changes where the tools look for
*inputs*. It does not:

- Every `$RealBin` use in the toolchain is cross-script exec or plotly (`bismark:941`,
  `bismark_methylation_extractor:377,424`, `bismark2report:1029`, `bismark2summary:140`) — all
  preserved by the atomic move.
- `bismark2report` and `bismark2summary` are the two invoked bare, with no `--dir`/no BAM
  arguments. Their discovery is a **CWD glob**, not `$RealBin`: `bismark2report:1114`
  `@alignment_reports = <*E_report.txt>;` and `bismark2summary:159,169,179,189`
  `@detected_files = <*bismark_bt2.bam>;` etc. (`bismark2summary:101` documents this: "the
  current working directory is scanned"). The job CWD is still the repo root, where the
  outputs land — so these steps see exactly what they saw before.
- Every other argument is `./test_files/…` or a bare output filename, both CWD-relative and
  untouched.

`use lib "$RealBin/../lib"` is a no-op on both sides — neither `/Users/fkrueger/Github/lib`
(pre-move) nor `/Users/fkrueger/Github/Bismark/lib` (post-move) exists.

### `.gitattributes` — linguist behaviour preserved

`git check-attr linguist-vendored` returns **set** for all six moved plotly assets, including
the three not covered by `*.ly`/`*.html` (`bismark.logo`, `bioinf.logo`,
`Bismark_Alignment_Stats_Summary.png`, `plotly_template.tpl`). The Perl scripts stay
`unspecified`, i.e. still counted as Perl — correct, since the goal is the file listing, not
the language bar.

I also confirmed in a throwaway repo that `**/plotly/**` **alone** already sets the attribute
on `legacy_perl/plotly/{bismark.logo,x.tpl,a.png}` — so line `:10` is genuinely redundant
belt-and-braces, exactly as the plan claimed (see Low-1).

### `.dockerignore` — the exclusion is safe

`Dockerfile` does `COPY . .` then
`cargo build --release --locked --manifest-path rust/Cargo.toml -p bismark --bin bismark …`.
Nothing in `legacy_perl/` is build-consumed:

- the only `include_*!` reaching outside its own dir is
  `summary_template_drift.rs:8 include_str!("../src/summary/summary_template.html")` — inside
  `rust/`;
- `rust/bismark/build.rs` has no `../..`/`legacy_perl`/`plotly` reference;
- both drift guards are `#[cfg(test)]` and read at *test* runtime, not compile time, so
  `cargo build` never touches the path.

### `git mv` fidelity

`git ls-files -s legacy_perl/` shows mode **100755** for all 13 Perl files (and 100644 for the
plotly assets/`test_data.fastq`), and the diff carries no mode lines — execute bits preserved
in the index, not just on disk. `git log --follow legacy_perl/bismark2report` traverses the
rename into pre-move history.

### Tests I ran

| Check | Result |
|---|---|
| `cargo test -p bismark --test legacy_perl_layout` | `1 passed` |
| `cargo test -p bismark --lib -- match_repo_plotly_files` (`BISMARK_REQUIRE_PERL=1`) | `2 passed` — `report::…::embedded_assets_match_repo_plotly_files` + `summary::…::vendored_assets_match_repo_plotly_files`, both against `legacy_perl/plotly/` |
| Full 6-filter battery (`perl_vs_rust oracle_ byte_identical match_repo_plotly_files matches_perl_heredoc legacy_perl_toolchain`, `--no-fail-fast`, `BISMARK_REQUIRE_PERL=1`) | **exit 0**, zero `skipping` lines |
| `summary_perl_oracle.rs` | `12 passed` — proves **both** literals at `:22,:23` resolve, and that the plan's "silent-green hazard" tests are now genuinely *running* |
| `summary_template_drift.rs` | `1 passed` (`:12` resolves) |
| `perl_vs_rust_nondirectional_pe_dedup` / `…_pe_extractor` | both ok (`dedup_integration_dedup.rs:2215`, `extractor_nondir_swapped_flags_1030.rs:263` resolve) |
| `methylation_consistency_integration.rs -- --exact perl_vs_rust_{se_three_way,pe_three_way,chh_se}` | `3 passed` (`:466` resolves) |
| `cargo fmt --all -- --check` | clean |

The two literals I did not re-run individually (`genome_prep_integration.rs:34`,
`report_perl_vs_rust.rs:64`) are both **fail-loud by construction** — `canonicalize().unwrap()`
and an unguarded `perl <script>` respectively — so the battery's exit 0 covers them.

### The layout gate asserts the right invariant

The union of the two drift guards' asset lists is exactly
`{plotly_template.tpl, plot.ly, bismark.logo, bioinf.logo}` (`report/assets.rs:128-131` reads
all four, `summary/assets.rs:113-115` reads three) — precisely the four the layout test names.
No code-read asset is uncovered. The 12 script names match `git ls-files legacy_perl/` exactly.
The gate is unconditional, needs no Perl, and `--exact` filtering in the `perl-oracle` job
means the new test binary contributes zero `… ok` lines there, so `EXPECTED=13` is unaffected.

---

## Issues

### Medium-1 — `rust/README.md:102` states the location backwards for its own instruction

```
Or run it from source: download the [v0.25.1 release](…) (or `git checkout v0.25.1`) — the
Perl scripts (`bismark`, `deduplicate_bismark`, `bismark_methylation_extractor`, …) live in
`legacy_perl/` (at the `v0.25.1` tag: the repository root) and need **Perl** + …
```

The sentence tells the reader to *download / check out `v0.25.1`*. In that tree the scripts are
at the **repository root** — so the main clause is false for the reader who follows the
sentence, and the true statement is demoted to a parenthetical. The plan (§3.9) asked for
wording "true on both the dev branch and at the `v0.25.1` tag", which this technically is, but
it leads with the branch case in a sentence whose subject is the tag case.

**Recommended replacement** for `rust/README.md:102`:

```markdown
Or run it from source: download the [v0.25.1 release](https://github.com/FelixKrueger/Bismark/releases/tag/v0.25.1) (or `git checkout v0.25.1`) — the Perl scripts (`bismark`, `deduplicate_bismark`, `bismark_methylation_extractor`, …) sit at the repository root there (on `master`/`dev` they live in `legacy_perl/`) and need **Perl** + a **Bowtie 2 / HISAT2** backend + **samtools** on `PATH`.
```

`README.md:13` and `README.md:60` do not have this problem — they describe the current tree and
route Perl users to the release separately, which is correct.

### Low-1 — `.gitattributes:10` is redundant and now breaks the block's alignment

```
 *.ly               linguist-vendored
 legacy_perl/plotly/** linguist-vendored
 **/plotly/**       linguist-vendored
 *.html             linguist-vendored
```

I verified empirically that `**/plotly/**` alone covers `legacy_perl/plotly/*`, so `:10`
contributes nothing (the plan knew this and kept it deliberately — that is a fine call). The
cost is cosmetic: the new pattern is 21 characters and no longer fits the file's 20-column
attribute alignment, leaving the block visibly ragged. Either fix works:

**(a) realign the block** (keeps belt-and-braces):

```
*.ly                  linguist-vendored
legacy_perl/plotly/** linguist-vendored
**/plotly/**          linguist-vendored
*.html                linguist-vendored
```

**(b) drop `:10`** — the anchored pattern was always subsumed by `:11`; deleting it removes a
line that has to be re-edited on every future relocation.

### Low-2 — drift-guard comments and assertion messages still call the source `plotly/`

`rust/bismark/src/report/assets.rs:122` and `rust/bismark/src/summary/assets.rs:107` both read
"must equal the CANONICAL repo `plotly/` files (Perl's source of truth)" — the sentence that
names the source of truth is the one left stale, while the *next* line names the new path.
More concretely, the failure messages send a reader to a path that no longer exists:

- `report/assets.rs:135` — `"embedded {name} drifted from plotly/{name}"`
- `summary/assets.rs:119` — `"vendored {name} drifted from plotly/{name}"`

Fixing the first mention lets the redundant second one go, which also shortens the comment
(project rule prefers one line stating the fact). For `report/assets.rs:120-125`:

```rust
        // Drift guard: the vendored `assets/` bytes embedded via `include_str!` must equal
        // the canonical `legacy_perl/plotly/` files (Perl's source of truth). Read at test
        // runtime, so `cargo package`'s verify-build is unaffected.
```

and in the assertions, `plotly/{name}` → `legacy_perl/plotly/{name}` in both files. The two
*historical* narrations at `report/assets.rs:7` and `summary/assets.rs:18` are correctly left
alone — they describe past `include_str!` attempts, not the current path.

### Low-3 — `filter_nonconversion/generate_goldens.sh:28` computes the script dir twice

```sh
PERL_FNC="${PERL_FNC:-$(cd "$(dirname "$0")/../../../../.." && pwd)/legacy_perl/filter_non_conversion}"
HERE="$(cd "$(dirname "$0")" && pwd)"
```

Behaviour is correct as written — I confirmed the up-count resolves, that `${VAR:-…}` evaluates
the subshell lazily so a `PERL_FNC` override still short-circuits it, and that a failed `cd`
propagates under the file's `set -euo pipefail`. But `HERE` on the very next line already does
the `dirname "$0"` work. Moving `HERE` above and reusing it leaves one place to get the
up-count wrong instead of two:

```sh
HERE="$(cd "$(dirname "$0")" && pwd)"
PERL_FNC="${PERL_FNC:-$(cd "$HERE/../../../../.." && pwd)/legacy_perl/filter_non_conversion}"
```

Replacing the hardcoded `/Users/fkrueger/…` default with a computed one is a clear improvement
either way.

### Low-4 — `scripts/c2c_byte_identity_matrix.sh:29` help text lost a word

`repo-root ./coverage2cytosine` became `repo ./legacy_perl/coverage2cytosine`. "repo" reading
as a bare noun modifier is awkward; `repo-root ./legacy_perl/coverage2cytosine` matches the
sibling scripts' phrasing.

(For contrast, `scripts/phase_h_smoke.sh:57`'s `./legacy_perl/bismark_methylation_extractor`
inherits a pre-existing inaccuracy — the real default is `$REPO_ROOT`-relative, not CWD-relative
— but that predates this branch and is not worth touching here.)

### Low-5 — the layout gate does not assert the execute bit

`ci_tests.yml` invokes `./legacy_perl/<name>` directly, so mode 755 is load-bearing for that
workflow; the oracles all invoke `perl <script>` and do not care. The gate asserts only
`.exists()`, so a tree that lost `+x` would stay green in `rust_ci.yml` and fail only in the
slower Perl-only workflow.

This is narrow — git records 100755 in the index and preserves it on checkout, and
`ci_tests.yml` has no `paths:` filter so it runs on every push. Optional hardening, and note it
applies to the 12 scripts but not the 4 plotly assets, so it means splitting the single array:

```rust
    for f in [/* 12 script names */] {
        let p = dir.join(f);
        assert!(p.exists(), "legacy_perl/{f} missing — layout assumption broken");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert!(mode & 0o111 != 0, "legacy_perl/{f} lost its execute bit");
        }
    }
    for f in ["plotly/plot.ly", "plotly/plotly_template.tpl", "plotly/bismark.logo", "plotly/bioinf.logo"] {
        assert!(dir.join(f).exists(), "legacy_perl/{f} missing — layout assumption broken");
    }
```

My recommendation is to **skip this** unless the caller wants it: it doubles the test's length
to guard a failure mode CI already catches loudly.

### Low-6 — PLAN §12 does not record the 7th golden script (documentation only)

The implementation correctly fixed
`rust/bismark/tests/data/coverage2cytosine/phase3_ffs/generate_goldens.sh:12`, which §5(d)'s
table of six does not list. §12's "Deviations from plan" has six entries and this is not among
them, so the plan's headline counts — "6 files" in §5(d), "23 code sites" in §2/§5, "**(d)**
6 files" in the V5 checklist — are understated by one. One line in §12 keeps the next
relocation's checklist starting from 7 / 24 rather than 6 / 23.

---

## Areas with nothing to report

- **Efficiency.** Nothing wasteful introduced. One extra trivial test binary; the removed
  20 MB of Docker build context is a real win. The single pure-rename commit keeps
  `--follow` working (verified).
- **Structure / naming.** `legacy_perl_layout.rs` and `legacy_perl_toolchain_is_present` say
  what they do. No duplication introduced. The two comment rewrites at
  `genome_prep_integration.rs:30-31` and `summary_perl_oracle.rs:21` incidentally fix *stale
  crate names* left over from the multicall consolidation (`rust/bismark-genome-preparation/`,
  `rust/bismark-summary/`) — a small bonus.
- **CHANGELOG / README.** Heading placement (`### Repository layout` first under
  `## Unreleased`) matches the file's existing convention. The relative link
  `[`legacy_perl/`](legacy_perl)` resolves on GitHub from both `CHANGELOG.md` and `README.md`,
  and the docs site does not ingest `CHANGELOG.md`, so there is no broken-link surface.
- **Docs sweep.** Confirmed near-empty as the plan predicted: the only markdown root-location
  claim anywhere in `docs/`, `CONTRIBUTING.md`, `README.md`, `rust/README.md`, `_config.yml`
  was `rust/README.md:102` (already edited; see Medium-1). `installation.md:99,105`'s
  `test_data.fastq` mentions describe a *downloadable* test dataset used with an absolute
  genome path — no in-repo path claim — so Assumption 2 holds and no edit is needed.
- **`.prettierignore:2`.** Correctly left alone: `plotly/` is a trailing-slash-only pattern
  and matches a `plotly` directory at any depth under gitignore semantics.
- **Informational, no action.** The `bismark` crate declares no `include`/`exclude`, so
  `tests/` ships in the published `.crate`, and the layout test is unconditional. This adds no
  new failure *class* (`cargo publish`'s verify step runs `cargo build`, not `--all-targets`,
  and the two drift guards already fail the same way outside the workspace). Worth knowing only
  because `.dockerignore` now prunes `legacy_perl/`: a `cargo test` step inside a Docker-context
  job would fail on the layout test and both drift guards. Nothing does that today —
  `release.yml:337-338` only smoke-tests `--help`.
