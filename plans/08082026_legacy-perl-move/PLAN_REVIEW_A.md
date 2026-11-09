# Plan Review A — `legacy_perl/` move (PLAN.md rev 1)

**Reviewer:** A (independent) · **Date:** 2026-08-08
**Target:** `/Users/fkrueger/Github/Bismark/plans/08082026_legacy-perl-move/PLAN.md` (revision 1)
**Repo state:** branch `dev`, verified against working tree at review time.

## Verdict

**REQUEST CHANGES.** The plan's *direction* is sound and its Perl-internal analysis is
accurate — I verified all 10 claimed Rust file:line references and all 5 claimed Perl
`$RealBin` couplings exist exactly as described, and the `release.yml` / `Dockerfile` /
`license.txt` non-consumer reasoning is correct. But three defects would let this change
land broken-and-green:

1. **9 consumers are missing from the inventory** — 5 golden-generation scripts under
   `rust/bismark/tests/data/` and 4 harnesses under `scripts/`. §11 explicitly certifies
   `scripts/` as "grep-clean"; it is not.
2. **The §7 "perl-oracle tripwire" claim is false for 4 of the 8 changed Rust test paths**,
   and two of those skip *silently*. Missing three specific edits produces a fully green CI
   across both workflows.
3. **The V1/V2 `cargo test` filter strings match almost none of the tests they name**, and
   the V5 straggler grep is structurally blind to every shell-script site. The validation
   section reads as rigorous but, as written, would not catch defect 2.

None of this is architectural — it is inventory and validation-net repair. With the Critical
items folded in, the plan is implementation-ready.

---

## 1. Logic review

### 1.1 What checks out (verified, not taken on faith)

Every file:line reference in §5's Rust table is exact:

| Claimed | Verified content |
|---|---|
| `rust/bismark/tests/dedup_integration_dedup.rs:2214` | `let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../deduplicate_bismark");` |
| `rust/bismark/tests/extractor_nondir_swapped_flags_1030.rs:263` | `…join("../../bismark_methylation_extractor");` |
| `rust/bismark/tests/genome_prep_integration.rs:34` | `.join("../../bismark_genome_preparation")` |
| `rust/bismark/tests/report_perl_vs_rust.rs:64` | `let perl_script = repo_root().join("bismark2report");` |
| `rust/bismark/tests/summary_perl_oracle.rs:22` | `…join("../../bismark2summary");` |
| `rust/bismark/tests/summary_perl_oracle.rs:23` | `…join("../../plotly/plot.ly");` |
| `rust/bismark/tests/summary_template_drift.rs:12` | `let src_path = …join("../../bismark2summary");` |
| `rust/bismark/tests/methylation_consistency_integration.rs:466` | `…join("../../methylation_consistency");` |
| `rust/bismark/src/report/assets.rs:125` | `let base = …join("../../plotly");` |
| `rust/bismark/src/summary/assets.rs:110` | `let base = …join("../../plotly");` |

The Perl-internal claims in §2 are also exact: `bismark:941` `system ("$RealBin/bam2nuc @args")`;
`bismark_methylation_extractor:377` → `$RealBin/bismark2bedGraph`, `:424` → `$RealBin/coverage2cytosine`;
`bismark2report:1029` and `bismark2summary:140` → `open (DOC,"$RealBin/plotly/$template")`.
The structural argument — move the set atomically and `$RealBin` relativity is preserved by
construction, so zero Perl edits — is correct for these 5 couplings.

Verified non-consumers, as claimed:
- `release.yml:131-136` copies the built Rust multicall binary (`cp "${BINDIR}/bismark"` at
  `:130`) and `ln -s bismark` for the 11 classic names into `staging/` — Rust symlinks, not
  Perl. `:165-172` smoke-tests the extracted tarball. Confirmed unaffected.
- `release.yml:137` `cp license.txt "staging/${ARCHIVE}/LICENSE"` runs from repo root →
  §8.4's "license.txt stays at root" is correctly derived and load-bearing.
- `Dockerfile` COPYs are `:29 COPY . .`, `:71` rust target binary, `:92` `docker/` wrapper,
  `:100` `license.txt`, `:101` `rust/THIRD-PARTY-NOTICES.md`. Nothing from the move set is
  build-consumed → the `.dockerignore` addition is safe.
- `rust_ci.yml` contains **zero** references to any moved filename (grepped) — paths live
  inside tests, as claimed.
- `docs.yml`, `ga-candidate-image.yml`, `link_closing_pr.yml`: zero hits.
- `validation/` contains only prose/command-example mentions (`VALIDATION_REAL_DATA.md:54,
  104, 106, 277, 295, 381`), no root-relative paths.
- §11's `use lib "$RealBin/../lib"` claim is true: neither `/Users/fkrueger/Github/lib`
  (before) nor `/Users/fkrueger/Github/Bismark/lib` (after) exists → no-op both ways.
  Sites: `bismark:9`, `bismark2report:6`, `bismark2summary:6`,
  `bismark_methylation_extractor:9`.
- §5's guard "grep `report_perl_vs_rust.rs` for other `repo_root()` call sites" is
  satisfiable: exactly one call site (`:64`) plus the definition (`:23`). Keep the guard.
- `ci_tests.yml:5` is `on: [push, pull_request]` — **no `paths:` filter**, so §5's "check the
  workflow header" open item resolves to nothing to do.

### 1.2 CRITICAL — 9 consumers missing from the inventory

§11 certifies: *"Verified non-consumers: … `scripts/`, `validation/`, `docker/` (grep-clean)."*
`scripts/` is **not** grep-clean, and §5's golden-script list is 3 of 8.

**Five golden-generation scripts under `rust/bismark/tests/data/` — not in §5, and *not*
env-overridable** (they hard-derive the path from the script's own location, so there is no
escape hatch when the derivation goes wrong):

- `rust/bismark/tests/data/coverage2cytosine/phase1/generate_goldens.sh:13`
  `C2C="$(cd "$HERE/../../../../.." && pwd)/coverage2cytosine"`
- `rust/bismark/tests/data/coverage2cytosine/phase_b/generate_goldens.sh:5`
  `C2C="$(cd "$(dirname "$0")/../../../../.." && pwd)/coverage2cytosine"`
- `rust/bismark/tests/data/coverage2cytosine/phase2_drach/generate_goldens.sh:15` (same idiom)
- `rust/bismark/tests/data/coverage2cytosine/phase3_ffs/generate_goldens.sh:12` (same idiom)
- `rust/bismark/tests/data/nome_filtering/phase_b/generate_goldens.sh:9`
  `NOME="$(cd "$(dirname "$0")/../../../../.." && pwd)/NOMe_filtering"`

**Four harnesses under `scripts/`** — these *do* have overrides, so they degrade to a clear
"not found" error rather than silent wrongness, but their documented defaults become wrong:

- `scripts/c2c_byte_identity_matrix.sh:120` `[[ -n "$PERL_C2C" ]] || PERL_C2C="$REPO_ROOT/coverage2cytosine"`
  (`REPO_ROOT` derived at `:80`; the usage text at `:29` also says "repo-root `./coverage2cytosine`")
- `scripts/phase_h_smoke.sh:134` `PERL_BIN="${PERL_BIN:-$REPO_ROOT/bismark_methylation_extractor}"`
  (`REPO_ROOT` at `:133`; usage text at `:57`)
- `scripts/phase_h_se_matrix.sh:116` (same idiom; `REPO_ROOT` at `:87`)
- `scripts/phase_h_pe_matrix.sh:163` (same idiom; `REPO_ROOT` at `:99`)

These are dev/benchmark tooling, not CI gates, so nothing goes red — which is exactly why
they need to be in the inventory rather than discovered months later mid-benchmark.

### 1.3 CRITICAL — the §7 tripwire does not cover the paths most likely to fail silently

§7 asserts: *"The `perl-oracle` job's fail-loud count assertions are the tripwire that would
catch any missed oracle path (a missed path makes a skip-guarded test skip → count short →
job red)."*

The `perl-oracle` job (`rust_ci.yml:206-221`) runs `EXPECTED=13` named tests with
`--exact`. I mapped all 13 to their source files:

| Source file | Oracle tests in the 13 |
|---|---|
| `genome_prep_integration.rs` | 8 |
| `methylation_consistency_integration.rs` | 3 |
| `dedup_integration_dedup.rs` | 1 |
| `extractor_nondir_swapped_flags_1030.rs` | 1 |

That is **4 of the 8 changed path sites**. The other four are outside the job entirely:
`report_perl_vs_rust.rs:64`, `summary_perl_oracle.rs:22`, `summary_perl_oracle.rs:23`,
`summary_template_drift.rs:12`.

Worse, `BISMARK_REQUIRE_PERL` is honoured in exactly those same 4 files and nowhere else
(`dedup_integration_dedup.rs:2232`, `methylation_consistency_integration.rs:487`,
`extractor_nondir_swapped_flags_1030.rs:281`, `genome_prep_integration.rs:52`). So V2's
"none skip under `BISMARK_REQUIRE_PERL=1`" is **unachievable** for the summary tests — they
do not read the variable.

Behaviour of the four uncovered sites if their edit is missed:

- **`summary_perl_oracle.rs:22-23` — SILENT SKIP.** `perl_script()` (`:20-34`) returns `None`
  when either `../../bismark2summary` or `../../plotly/plot.ly` is absent; all **12**
  `oracle_*` tests then print a notice and **pass**. The file's own module doc (`:11-13`)
  states this: *"Auto-skips (prints a notice, passes) when `perl`, the Perl source, or the
  `plotly/` assets are unavailable."*
- **`summary_template_drift.rs:12` — SILENT SKIP.**
  `if !src_path.exists() { eprintln!("skipping: Perl source unavailable at …"); return; }`
  (`:13-19`) → passes.
- `report_perl_vs_rust.rs:64` — **fails loudly.** It gates only on `perl_available()`
  (`:15-20`), never on script existence, so a bad path makes `perl` exit non-zero and
  `assert!(perl_status.status.success(), "perl bismark2report failed for `{label}`")`
  (`:70-73`) fires. Real gate.
- The two `src` drift guards — **fail loudly.** Both do
  `std::fs::read_to_string(base.join(name)).unwrap()` (`report/assets.rs:132`,
  `summary/assets.rs:114`) → panic on a missing dir. Real gates.

**Net residual risk as the plan stands:** omit the three edits in `summary_perl_oracle.rs`
(×2) and `summary_template_drift.rs` (×1) and **every job in both workflows is green** — the
`test` job (`rust_ci.yml:48-49`, `cargo test --workspace`) runs them and they skip;
`perl-oracle` never runs them; the skip-detector at `rust_ci.yml:222-224`
(`grep -q '^skipping:'`) only inspects the output of the 13 `--exact` tests. Thirteen
byte-identity oracle tests would be silently switched off. That is precisely the failure
mode issue #796 was created to close.

### 1.4 IMPORTANT — `copy_bismark_files_for_release.pl` breaks; "zero Perl code changes" is overstated

§2 and §11 generalise the `$RealBin` argument to the whole move set. It does not hold for the
packager. `copy_bismark_files_for_release.pl:18` resolves everything from its own directory:

```perl
my ($volume, $dist_dir, $this_script) = File::Spec->splitpath(__FILE__);
```

`:20` then copies, from `$dist_dir`: `CHANGELOG.md`, `license.txt`,
`Bismark_alignment_modes.pdf` — **all three stay at root** — and `:24` copies
`Docs/make_docs.pl`, `Docs/README.md`, `Docs/Bismark_User_Guide.html`. After the move,
`$dist_dir` is `legacy_perl/`, so `cp` fails and `:44` `die "Copy failed: $!"` fires on the
**first** entry (`CHANGELOG.md` is element 0).

Mitigating: the script is **already** non-functional today. `make_docs.pl` and
`Bismark_User_Guide.html` do not exist anywhere in the tree (`find` returns nothing), and
`Docs/` resolves only via case-insensitive APFS to `docs/` — so it already dies at the
`@docs` stage on any run. The move makes it fail earlier, not newly broken.

The problem is the plan being *silent* about it while asserting the opposite. Pick one and
say so: (a) note it is already dead and moves as a historical artifact; (b) leave it at root;
or (c) fix the three root-file references. Any is defensible; the unqualified "zero Perl code
changes" is not.

### 1.5 IMPORTANT — §5's `ci_tests.yml` instruction misses one invocation

§5 says *"All 43 script invocations … (every line matching
`^\s+\./(bismark|deduplicate|coverage2|bam2nuc|filter_non|NOMe|methylation_consistency)`)"*.

The anchored pattern does match exactly 43 lines — but there are **44** invocations. The miss
is `ci_tests.yml:37`:

```yaml
        run: ./bismark --help
```

An inline `run:` scalar, so `./bismark` is not at line-start-after-whitespace. A mechanical
`sed` driven by §5's pattern skips it. Self-healing in practice (the step would fail with
"No such file or directory", and V5's *unanchored* straggler grep does catch it), but the
instruction is wrong as written and should say 44.

For completeness: `ci_tests.yml:95, 101, 116` also contain the script names, but only as
`- name:` step labels — cosmetic, no change required.

### 1.6 Minor — inventory detail errors

- **§3.1 says `plotly/` is "4 asset files"; it is 10** (`git ls-files plotly/ | wc -l` = 10):
  `Bismark_Alignment_Stats_Summary.png`, `bioinf.logo`, `bismark.logo`,
  `bismark_bt2_PE_report.html`, `bismark_summary_RRBS.html`, `bismark_summary_WGBS.html`,
  `bismark_summary_single_cells.html`, `bismark_summary_single_cells.txt`, `plot.ly`,
  `plotly_template.tpl`. "4" is the number the report drift guard compares, not the directory
  contents. Harmless to the `git mv` (whole dir moves) but the behaviour spec should be right.
- **Two cosmetic env-hint references not in §5**:
  `rust/bismark/tests/filter_nonconversion_byte_identity_real_data.rs:17`
  (`FNC_PERL=~/Github/Bismark/filter_non_conversion`) and `nome_gate.sh:8` (§5 lists only
  `:19`). Same class as the one the plan already accepted as cosmetic.
- `.claude/settings.local.json:7-27` holds absolute paths to the root scripts. Untracked and
  local; permission-allowlist entries only, so stale entries just mean a re-prompt. Worth a
  one-line mention beside the local `CLAUDE.md` item in §5.

---

## 2. Assumptions

### Validated
- §8.1 naming — user decision, not relitigated.
- §8.2 `test_data.fastq` moves safely. Confirmed: the only references are prose in
  `docs/src/content/docs/installation.md:99, 105`. `docs/src/content/docs/quick-reference.md`
  discusses `test_dataset.fastq` — a *different*, non-existent filename. No code or CI path
  reference anywhere (`ci_tests.yml` uses `test_files/`, never `test_data.fastq`).
- §8.4 `license.txt` at root — correct, and load-bearing via `release.yml:137`.
- §8.5 `test_files/` at root — correct; `ci_tests.yml` passes `./test_files/` throughout
  (e.g. `:98, 104, 119`) and those are CWD-relative, unchanged by the move.
- §8.6 `git mv` preserves mode — the 12 scripts and
  `copy_bismark_files_for_release.pl` are all `-rwxr-xr-x`; no case-only renames involved.

### Unstated assumptions that should be written down

- **A1 (important): linguist survives the move.** The plan's entire *purpose* is a Rust-first
  landing page, and that depends on `.gitattributes`. `\.gitattributes:10`
  `plotly/**  linguist-vendored` contains a mid-pattern slash → **root-anchored** → goes dead
  after the move. `:11` `**/plotly/**  linguist-vendored` is unanchored → still matches
  `legacy_perl/plotly/**`, so `linguist-vendored` **survives** and the language bar is safe.
  The plan gets the right outcome by luck. Add an explicit note, otherwise a future tidy-up
  that deletes the "redundant" line 11 silently re-adds 17 MB of vendored HTML/LilyPond to
  the language bar. (`*.ly` at `:9` and `*.html` at `:12` are extension-based and unaffected.)
- **A2 (minor): `.prettierignore` needs no edit.** `.prettierignore:2` is `plotly/` —
  gitignore semantics, trailing separator only, so unanchored and it still matches
  `legacy_perl/plotly/`. State this so an implementer neither worries nor "fixes" it.
- **A3:** the Perl scripts are currently **inside** the Docker build context
  (`.dockerignore` excludes `*.fastq.gz` but not `*.fastq`, and has no entry for the scripts
  or `plotly/`). Worth saying, because it is what makes the `.dockerignore` addition a real
  win rather than a no-op.

---

## 3. Efficiency

Mechanical change; §6's "CI runtime unchanged, no runtime code paths touched" is right.

One factual correction: **§6's "~3 MB of plotly assets + scripts" is off by roughly 7×.**
Measured (`du -ch`) the move set is **20 MB**: `plotly/` 17 MB, `test_data.fastq` 2.0 MB, the
13 Perl files 1.0 MB. So the Docker context shrinks by ~20 MB, not ~3 MB — the change is
*better* than advertised. Fix the figure so the CHANGELOG line is accurate.

No complexity, memory, or scalability concerns: no runtime code path is touched, and
`git mv` of 20 MB is trivial.

---

## 4. Validation sufficiency

The validation table is the weakest part of the plan. It looks thorough and is not.

### V1/V2 — CRITICAL: the filter strings match almost nothing they name

The command in the sabotage step and V1/V2 is:

```bash
cd rust && BISMARK_REQUIRE_PERL=1 cargo test -p bismark --no-fail-fast -- \
  perl_oracle perl_vs_rust drift
```

`cargo test` filters on **test names**, not file names. Integration-test file names are never
part of the filter string. Actual test names:

| Filter | Intended target | Actual test names | Matches |
|---|---|---|---|
| `perl_oracle` | `summary_perl_oracle.rs` | `oracle_wgbs_two_sample`, `oracle_all_rrbs_raw_mode`, … (12 × `oracle_*`, at `:100, 111, 148, 173, 227, 284, 318, 358, 395, 407, 451, 505`) | **0** |
| `drift` | `summary_template_drift.rs` + both asset guards | `embedded_template_matches_perl_heredoc` (`:11`), `embedded_assets_match_repo_plotly_files` (`report/assets.rs:119`), `vendored_assets_match_repo_plotly_files` (`summary/assets.rs:104`) | **0** |
| `perl_vs_rust` | incl. `report_perl_vs_rust.rs` | `pe_full_companions_byte_identical` (`:129`), `se_r1_only_mbias_byte_identical` (`:135`), `nondirectional_unknown_context_byte_identical` (`:142`), `minimal_alignment_only_byte_identical` (`:148`), `crlf_alignment_byte_identical` (`:155`) | **0 in that file** |

`perl_vs_rust` *does* match the genome-prep / methcons / dedup / extractor test names (those
are literally `perl_vs_rust_*`). So the command as written exercises **exactly the 4 files
that are already fail-loud and already covered by CI**, and provides **zero** evidence about
the 3 fragile sites plus both plotly guards — the only places where a missed edit is
dangerous. V1's sabotage would look convincing (things fail!) while never touching the risk.

Suggested replacement, name-verified against the tree:

```bash
cd rust && BISMARK_REQUIRE_PERL=1 cargo test -p bismark --no-fail-fast -- \
  perl_vs_rust oracle_ byte_identical match_repo_plotly_files matches_perl_heredoc \
  2>&1 | tee /tmp/sabotage.log
grep -c '^test .* \.\.\. ok$' /tmp/sabotage.log
grep -n 'skipping' /tmp/sabotage.log   # every hit is an untrustworthy green — enumerate it
```

### V3 — IMPORTANT: wrong test name for the summary guard

§5 and V3 both call the pair `embedded_assets_match_repo_plotly_files` ("in both modules").
Only the report one has that name (`rust/bismark/src/report/assets.rs:119`). The summary one
is **`vendored_assets_match_repo_plotly_files`** (`rust/bismark/src/summary/assets.rs:104`).
Filtering on the named string runs one guard and reports V3 green having verified half of
what it claims. V3's parenthetical "(report + summary modules)" shows the intent is right —
just fix the name.

### V4 — overstates its coverage

V4 claims `ci_tests.yml` exercises "extractor→bedGraph/c2c, bismark→bam2nuc, report/summary→plotly".
Verified:

- **report/summary→plotly: yes.** `ci_tests.yml:60` `./bismark2report` and `:61`
  `./bismark2summary` both hit `open (DOC,"$RealBin/plotly/$template")`.
- **extractor→bismark2bedGraph: plausibly yes** via `--bed` (`:59, 68, 84, 85, 92, 93`),
  which Getopt::Long resolves as an unambiguous abbreviation of `--bedGraph`.
- **bismark→bam2nuc: NO.** `--nucleotide_coverage` appears **nowhere** in `ci_tests.yml`
  (grep: zero hits), so `bismark:941` is never reached.
- **extractor→coverage2cytosine: NO.** `--cytosine_report` appears **nowhere** in
  `ci_tests.yml` (zero hits), so `bismark_methylation_extractor:424` is never reached.

This is low-consequence — the couplings cannot break, because all scripts move together and
`$RealBin` relativity is preserved by construction — but V4 is offered as *evidence* for
something it does not test. State the structural argument as the assurance and scope V4 to
what it actually covers, so nobody later banks on a check that isn't there.

### V5 — CRITICAL: the straggler grep cannot see the shell-script class

The first sweep is:

```bash
grep -rnE '\.\./\.\./(bismark|deduplicate|coverage2|bam2nuc|filter_non|NOMe|methylation_c|plotly)' \
  rust --include='*.rs' --include='*.sh' | grep -v legacy_perl | grep -v target
```

I ran it against the current tree: **13 hits, all `.rs`, zero `.sh`.** The pattern requires a
literal `../../` immediately before the script name, so it cannot see:

- `$REPO_ROOT/bam2nuc` (`bam2nuc/generate_goldens.sh:27`) — a site §5 *does* list
- `/Users/fkrueger/Github/Bismark/filter_non_conversion`
  (`filter_nonconversion/generate_goldens.sh:28`) — listed
- `path/to/Bismark/NOMe_filtering` (`nome_gate.sh:19`) — listed
- `$(cd … && pwd)/coverage2cytosine` × 4 and `$(cd … && pwd)/NOMe_filtering` — **the five
  sites §5 missed**
- `$REPO_ROOT/…` × 4 under `scripts/` — **also missed** (and `scripts/` isn't even in the
  grep's search path)

It also cannot see `repo_root().join("bismark2report")` (`report_perl_vs_rust.rs:64`), since
there is no `../../` there either. So V5 verifies 13 of the 23 real sites and is blind to
precisely the 9 the inventory dropped.

I tried to construct a single "zero hits" replacement grep. A sufficiently inclusive pattern
(`[/"](<names>|plotly/)` over `rust scripts`) returns **361 lines** — far too noisy to gate
on. Recommendation: **stop treating the sweep as a zero-hit grep.** Replace V5 with

1. a *positive* checklist asserting each of the 23 enumerated sites was edited (§1.1 + §1.2
   of this review give the full list); plus
2. three narrow, purpose-built greps for the idioms that actually occur —
   `\.\./\.\./(<names>|plotly)` (13 `.rs` sites), `(\$REPO_ROOT|&& pwd\)|Bismark)/(<names>)`
   over `rust scripts --include='*.sh'` (9 `.sh` sites), and `repo_root\(\)\.join\("` (1 site).

### Gap not covered by any V-row

Nothing asserts that the 12 `summary_perl_oracle.rs` tests and
`embedded_template_matches_perl_heredoc` **actually ran**. That is the single highest-value
addition. Cheapest sufficient fix, in scope: add these 13 names to the `perl-oracle` job's
`--exact` list and bump `EXPECTED=13` → `EXPECTED=26`. That both closes the hole permanently
and makes the tripwire claim in §7 true. It does mean teaching those two files to honour
`BISMARK_REQUIRE_PERL` (or asserting on the `skipping:` grep already at `rust_ci.yml:222-224`,
which would then cover them since they'd be inside the filtered run — note
`summary_template_drift.rs:15` prints `skipping:` at line start, so the existing detector
would catch it verbatim; `summary_perl_oracle.rs`'s notice text should be checked for the
same prefix).

If that is judged out of scope for a move commit, the minimum is an explicit V-row: *after
Commit 2, run the full battery and assert `grep -c '^test .* ok$'` equals a recorded
pre-move baseline* — so 13 tests quietly switching off shows up as a number.

---

## 5. Alternatives

- **A1 — a shared fail-loud helper.** Put a single
  `fn legacy_perl_script(name: &str) -> PathBuf` in `rust/bismark/tests/common/` that panics
  when the script is absent, and route all 8 test sites through it. Converts "silent skip on
  a moved script" from a recurring hazard into a structural impossibility, and the next
  relocation becomes a one-line change. Trade-off: a larger diff than a pure move, and it
  changes local-dev ergonomics for Perl-less machines (which is what the skips are for) — so
  it likely wants the `BISMARK_REQUIRE_PERL` escape hatch preserved. Given this is the second
  time skip-guards have been the weak link (§2 cites the #796 lineage), I'd argue it is worth
  a follow-up plan even if not this one.
- **A2 — root symlinks for compatibility** (`bismark -> legacy_perl/bismark`). Would preserve
  `blob/master/<script>` deep links and clone-root-on-PATH users. **Recommend against**: it
  defeats the entire stated goal (the root listing stays cluttered) and complicates
  `.gitattributes`. Recording it only so §10 shows it was considered and rejected.
- **A3 — split the workflow edit into its own commit.** Three commits (move / code+prose /
  workflow) instead of two makes a red `ci_tests.yml` trivially bisectable. Marginal value;
  take it or leave it.
- **A4 — add env overrides to the 5 c2c/NOMe golden scripts while you're in there.** They are
  the only Perl-dependent scripts with no override (unlike `PERL_FNC`, `PERL_BAM2NUC`,
  `PERL_C2C`, `PERL_BIN`, `PERL_NOME`). A `${PERL_C2C:-<derived>}` wrapper costs one line
  each and makes the next relocation a no-op for them. Optional, and arguably scope creep.

---

## 6. Action items

### Critical (fix before implementation)

1. **Add the 9 missing consumers to §5.** Five non-overridable golden scripts:
   `coverage2cytosine/phase1/generate_goldens.sh:13`, `…/phase_b/…:5`,
   `…/phase2_drach/…:15`, `…/phase3_ffs/…:12`, `nome_filtering/phase_b/generate_goldens.sh:9`.
   Four `scripts/` harnesses: `c2c_byte_identity_matrix.sh:120` (+ usage text `:29`),
   `phase_h_smoke.sh:134` (+ `:57`), `phase_h_se_matrix.sh:116`, `phase_h_pe_matrix.sh:163`.
   Delete the "`scripts/` … grep-clean" certification from §11.
2. **Close the silent-skip hole.** `summary_perl_oracle.rs` (12 tests) and
   `summary_template_drift.rs` (1) skip-and-pass when the script is missing, are absent from
   `perl-oracle`'s `EXPECTED=13`, and do not read `BISMARK_REQUIRE_PERL`. Either add all 13
   names to `rust_ci.yml:206-221` and bump `EXPECTED` to 26, or add a V-row asserting the
   post-fix ok-count matches a recorded pre-move baseline. Then correct §7 — as written its
   tripwire claim is false for 4 of 8 sites.
3. **Fix the V1/V2 filter strings.** `perl_oracle`, `drift`, and (for
   `report_perl_vs_rust.rs`) `perl_vs_rust` match **zero** tests. Use
   `perl_vs_rust oracle_ byte_identical match_repo_plotly_files matches_perl_heredoc`, and
   have V1 record the ok-count and every `skipping` line rather than eyeballing failures.
4. **Replace V5.** The `../../`-anchored grep sees 13 of 23 sites and zero `.sh` files — it is
   blind to exactly the class the inventory dropped. Use a positive checklist of all 23 sites
   plus the three narrow greps in §4 above.

### Important

5. **Resolve `copy_bismark_files_for_release.pl`** (`:18`, `:20`, `:24`) and drop the blanket
   "zero Perl code changes" claim. It copies `CHANGELOG.md` / `license.txt` /
   `Bismark_alignment_modes.pdf` from its own directory and would `die` on the first one.
   Note that it is already dead at the `@docs` stage today, then pick: accept-as-artifact,
   leave at root, or fix the three refs.
6. **Fix the summary guard's name** in §5 and V3: `vendored_assets_match_repo_plotly_files`,
   not `embedded_…` (`rust/bismark/src/summary/assets.rs:104`).
7. **§5 `ci_tests.yml` is 44 invocations, not 43** — the anchored pattern misses `:37`
   (`run: ./bismark --help`). Also record that there is no `paths:` filter (`:5` is
   `on: [push, pull_request]`), closing that open item.
8. **Scope V4 honestly.** `bismark→bam2nuc` and `extractor→coverage2cytosine` are **not**
   exercised (`--nucleotide_coverage` and `--cytosine_report` are absent from
   `ci_tests.yml`). Cite the structural `$RealBin` argument as the assurance instead.

### Optional

9. Record in §3/§8 that `.gitattributes:11` (`**/plotly/**`, unanchored) is what keeps
   `linguist-vendored` working after the move, and that `:10` (`plotly/**`, root-anchored)
   goes dead — so the language bar, i.e. the plan's whole point, is protected by line 11.
10. Note that `.prettierignore:2` (`plotly/`) needs no edit — unanchored under gitignore
    semantics.
11. `plotly/` is 10 tracked files, not 4 (§3.1). Fix §6's "~3 MB" → **~20 MB** (plotly 17 MB,
    `test_data.fastq` 2.0 MB, scripts 1.0 MB) so the CHANGELOG figure is right.
12. Two cosmetic env-hints not listed: `filter_nonconversion_byte_identity_real_data.rs:17`
    and `nome_gate.sh:8`. Mention `.claude/settings.local.json:7-27` alongside the local
    `CLAUDE.md` item.
13. Consider A1 (shared fail-loud test helper) as a follow-up plan.
