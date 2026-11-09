# Plan Review B — Move the legacy Perl toolchain to `legacy_perl/`

**Plan:** `/Users/fkrueger/Github/Bismark/plans/08082026_legacy-perl-move/PLAN.md` (revision 1)
**Reviewer:** B (independent, fresh context)
**Date:** 2026-08-08
**Repo state:** branch `dev`, verified against working tree

**Verdict: REQUEST CHANGES.**

The plan's *concept* is sound and its Perl-side reasoning is verified-correct: moving the set
atomically really does require zero Perl `$RealBin` edits, and `license.txt` really must stay at
root. But the consumer inventory is incomplete by **nine files**, and — more seriously — **every
mechanism the plan relies on to catch such a miss is itself vacuous**. The straggler grep cannot
match the idiom the missed files use, the `perl-oracle` job does not cover three of the eight paths
the plan itself lists, and the `cargo test` filter in V1/V2 selects 13 tests while the plan believes
it selects all of them. Executed as written, this plan can land with 9 broken files and a fully
green CI.

---

## 1. Logic review

### 1.1 What I verified as correct

Rather than take §5 on faith I resolved every claim against source. The following are **exact**:

| Plan claim | Verified |
|---|---|
| 8 Rust test path literals at the stated file:line | All 8 exact — `dedup_integration_dedup.rs:2214`, `extractor_nondir_swapped_flags_1030.rs:263`, `genome_prep_integration.rs:34`, `report_perl_vs_rust.rs:64`, `summary_perl_oracle.rs:22`, `:23`, `summary_template_drift.rs:12`, `methylation_consistency_integration.rs:466` |
| Drift guards at `report/assets.rs:125`, `summary/assets.rs:110`, comments at `:123`/`:108` | Exact |
| Perl cross-script couplings | Exact: `bismark_methylation_extractor:377` (`$RealBin/bismark2bedGraph`), `:424` (`$RealBin/coverage2cytosine`), `bismark:941` (`$RealBin/bam2nuc`), `bismark2report:1029` and `bismark2summary:140` (`$RealBin/plotly/$template`) |
| `use lib "$RealBin/../lib"` is a harmless no-op dir | Confirmed — `bismark:9`, `bismark2report:6`, `bismark2summary:6`; no `lib/` exists before or after |
| `release.yml` loops are Rust multicall symlinks, not Perl | Confirmed — `release.yml:131-136` is `ln -s bismark "staging/.../${b}"`; `:165` likewise |
| `license.txt` must stay at root | Confirmed — `release.yml:137` `cp license.txt "staging/${ARCHIVE}/LICENSE"` |
| `rust_ci.yml:164` is the `perl-oracle` job | Exact |
| `report_perl_vs_rust.rs` `repo_root()` has one other call site | Confirmed — definition `:23`, sole use `:64`. The plan's pre-edit grep guard is satisfiable |
| `test_data.fastq` referenced only as docs example text | Confirmed — only `docs/src/content/docs/installation.md:99,105`, no path reference anywhere in code/CI. **Open-2's default is safe** |
| `validation/` and `docker/` are clean | Confirmed — `validation/VALIDATION_REAL_DATA.md:381` mentions `deduplicate_bismark` in prose only; `docker/` holds one wrapper script with no Perl refs |
| `plans/.gitignore` carries `archived/` | Confirmed (`archived/`, `__pycache__/`) |
| Assumption 3: `ci_tests.yml` triggers need no change | Confirmed — `ci_tests.yml:5` is `on: [push, pull_request]` with **no** `paths:` filter. The plan's "check the workflow header" resolves to "nothing to do" |

Additional non-consumers I checked that the plan does not mention, and which are genuinely clean:
`rust/justfile:64-67` and `:82-85` (loops over `./target/release/$b` — Rust binaries);
`rust/bismark/Cargo.toml` (no `include`/`exclude` touching `plotly`); `docs.yml`,
`ga-candidate-image.yml`, `link_closing_pr.yml` (no Perl refs); `docs/` (no `./<script>`
invocation examples, no repo-root location claims).

### 1.2 C1 — Five golden-generation scripts are missing from the inventory

§5 lists **three** golden/dev scripts. There are **eight** shell files that reference a moved Perl
script by computed repo-relative path. The five the plan misses:

| File:line | Reference |
|---|---|
| `rust/bismark/tests/data/nome_filtering/phase_b/generate_goldens.sh:9` | `NOME="$(cd "$(dirname "$0")/../../../../.." && pwd)/NOMe_filtering"` |
| `rust/bismark/tests/data/coverage2cytosine/phase_b/generate_goldens.sh:5` | `C2C="$(cd "$(dirname "$0")/../../../../.." && pwd)/coverage2cytosine"` |
| `rust/bismark/tests/data/coverage2cytosine/phase1/generate_goldens.sh:13` | `C2C="$(cd "$HERE/../../../../.." && pwd)/coverage2cytosine"` |
| `rust/bismark/tests/data/coverage2cytosine/phase2_drach/generate_goldens.sh:15` | same idiom |
| `rust/bismark/tests/data/coverage2cytosine/phase3_ffs/generate_goldens.sh:12` | same idiom |

### 1.3 C2 — `scripts/` is NOT grep-clean

§2 and §11 both assert `scripts/` is a verified non-consumer ("grep-clean"). It is not. Four
byte-identity **gate drivers** — the tooling used to prove Rust equals Perl — default to a
root-relative Perl path:

| File:line | Reference |
|---|---|
| `scripts/c2c_byte_identity_matrix.sh:120` | `[[ -n "$PERL_C2C" ]] \|\| PERL_C2C="$REPO_ROOT/coverage2cytosine"` (usage text `:29` says "repo-root `./coverage2cytosine`") |
| `scripts/phase_h_smoke.sh:134` | `PERL_BIN="${PERL_BIN:-$REPO_ROOT/bismark_methylation_extractor}"` (usage text `:57`) |
| `scripts/phase_h_se_matrix.sh:116` | `PERL_BIN="${PERL_BIN:-$REPO_ROOT/bismark_methylation_extractor}"` |
| `scripts/phase_h_pe_matrix.sh:163` | `PERL_BIN="${PERL_BIN:-$REPO_ROOT/bismark_methylation_extractor}"` |

Unlike the golden scripts (§1.5), these resolve **correctly today**: all four define
`REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"`
(`c2c_byte_identity_matrix.sh:80`, `phase_h_smoke.sh:133`, `phase_h_se_matrix.sh:87`,
`phase_h_pe_matrix.sh:99`), which is the real repo root. So this is a genuine
working-to-broken regression, not the preservation of an existing bug.

Mitigating: all are env-overridable and fail loud when the default misses —
`c2c_byte_identity_matrix.sh:121` (`[[ -r "$PERL_C2C" ]] || usage_err`),
`phase_h_se_matrix.sh:117` and `phase_h_pe_matrix.sh:164` (`[[ ! -x "$PERL_BIN" ]] → exit 2`).
Nothing silently produces a wrong result. But these are the scripts a future
byte-identity investigation reaches for first, and they will break on the next use.

### 1.4 C3 — The straggler sweep cannot detect C1 or C2

§5's sweep, restated verbatim:

```
grep -rnE '\.\./\.\./(bismark|deduplicate|coverage2|bam2nuc|filter_non|NOMe|methylation_c|plotly)' \
  rust --include='*.rs' --include='*.sh' | grep -v legacy_perl | grep -v target
```

Two independent false negatives:

1. **The regex requires the script name to directly follow `../../`.** The five missed golden
   scripts write `$(cd "$HERE/../../../../.." && pwd)/coverage2cytosine` — `&& pwd)/` sits between
   the `../..` run and the name. No match. I confirmed this by running the plan's own regex: it
   returns exactly the 10 lines §5 already lists, and none of the five missed files.
2. **The sweep never looks at `scripts/`.** It is scoped to `rust`.

So V5 ("Zero non-`legacy_perl` hits") is satisfiable while nine files are broken. The sweep is
currently a false-negative generator dressed as a net — which is precisely the failure mode §11's
vacuous-verification guard is written to prevent.

### 1.5 I3 — All six data-dir golden scripts are *already* broken; the plan's edits preserve that

I resolved the up-level arithmetic empirically:

- `rust/bismark/tests/data/bam2nuc/generate_goldens.sh:26` — `$SCRIPT_DIR/../../../..` (4 up) →
  `/Users/fkrueger/Github/Bismark/rust`. So `:27 PERL_BAM2NUC="$REPO_ROOT/bam2nuc"` points at
  `rust/bam2nuc`, which **does not exist**.
- The five missed scripts use 5 up from one level deeper → also `/Users/fkrueger/Github/Bismark/rust`,
  i.e. `rust/coverage2cytosine` and `rust/NOMe_filtering` — neither exists.

These are stale relics of the multicall consolidation into `rust/bismark/` (one directory level
disappeared). Consequence for the plan: its `bam2nuc` edit produces `rust/legacy_perl/bam2nuc` —
**still broken**. This is not a regression, but §5 presents the edit as preserving a working path.
Either fix the up-count while touching the line (add one `..`) or state explicitly that these
scripts are already dead and the edit is cosmetic. Do not leave the reader believing they work.

### 1.6 I2 — The `ci_tests.yml` edit regex misses one invocation, and the count is wrong

§5 says: "All **43** script invocations `./<name>` → `./legacy_perl/<name>` (every line matching
`^\s+\./(bismark|deduplicate|coverage2|bam2nuc|filter_non|NOMe|methylation_consistency)`)".

There are **44**. The anchored regex yields 43 because `ci_tests.yml:37` is
`        run: ./bismark --help` — the invocation is inline after `run:`, so `^\s+\./` cannot match
it. Applying the plan's stated regex literally leaves the "Bismark help message" step calling a
nonexistent `./bismark`.

This one is self-correcting: the V5 straggler grep for `ci_tests.yml` is **unanchored**
(`grep -nE '\./(bismark|...)' ... | grep -v legacy_perl`) and would catch line 37. But "43" is
stated as a verification target, and a reviewer checking "did all 43 get changed?" would sign off on
a broken workflow. Fix the count to 44 and drop the `^\s+` anchor from the edit recipe.

For completeness: `ci_tests.yml` is 132 lines; the only `NOMe`-adjacent reference is
`:99 ./coverage2cytosine ... --NOMe-seq` (a flag, not the script), and there is no
`NOMe_filtering`, `methylation_consistency`, `plotly`, or `test_data.fastq` invocation in it.

### 1.7 I6 — `copy_bismark_files_for_release.pl` DOES need a change; "zero Perl code changes" is false

§2 states that moving the set together "requires zero Perl code changes". That holds for the 12
executables' `$RealBin` couplings (verified §1.1) but **not** for the packager the plan also moves.

`copy_bismark_files_for_release.pl:18` derives its base from its own location:

```perl
my ($volume, $dist_dir, $this_script) = File::Spec->splitpath(__FILE__);
```

and then at `:27` copies `catfile($dist_dir, $file)` for every entry of `@files` (`:20`), which
includes three files the plan deliberately **keeps at root**: `CHANGELOG.md`, `license.txt`, and
`Bismark_alignment_modes.pdf`. After the move `$dist_dir` becomes `legacy_perl/`, so the very first
iteration looks for `legacy_perl/CHANGELOG.md`, fails, and hits
`cp(...) or die "Copy failed: $!"` at `:45`. Today that same loop succeeds.

The plan's edge-case analysis (§11) considers only `$RealBin` shell-outs and plotly resolution — it
does not consider `__FILE__`-relative reads of *root-resident siblings*, which is a distinct
mechanism.

Severity is tempered by the packager already being partly dead: its `@docs` stage (`:37`) wants
`Docs/make_docs.pl` and `Docs/Bismark_User_Guide.html`, and neither `make_docs.pl` nor
`Bismark_User_Guide.html` exists anywhere in the repo, so stage 3 already dies. It is also
superseded by `release.yml` for the Rust suite. So "accept and document" is defensible — but the
plan must stop claiming zero Perl changes, and should state which stage now dies and why that is
acceptable.

(Incidental: `[ -d Docs ]` returns true on this case-insensitive macOS filesystem because it
resolves to `docs/`. Assumption 6 is still fine — no case-only rename is involved — but note the
repo does have live `Docs`/`docs` case aliasing, reflected in both `.gitattributes` and
`.dockerignore` carrying each spelling.)

### 1.8 I4 — `rust/README.md:102` is a root-location claim outside the prose inventory

§5's prose list covers `README.md`, `CHANGELOG.md`, `docs/`, `CONTRIBUTING.md`, and `CLAUDE.md`. It
omits `rust/README.md:102`:

> "…the Perl scripts (`bismark`, `deduplicate_bismark`, `bismark_methylation_extractor`, …) **live
> at the repository root** and need **Perl** + a **Bowtie 2 / HISAT2** backend + **samtools** on
> `PATH`."

Judgement call rather than a clear error: the sentence is scoped to "download the v0.25.1 release
(or `git checkout v0.25.1`)", and at tag `v0.25.1` the scripts *do* live at the root — so it stays
technically true. It still needs a conscious decision, particularly because `rust/README.md` is the
canonical status journal that gets touched on every module merge.

Verified as needing nothing: `CONTRIBUTING.md` makes no path claim (it says "this repository's
default branch", `:7`); `docs/src/content/docs/installation.md:78` routes Perl users to "check out
the corresponding legacy tag" with no root claim. So §3.7's premise that docs pages "claim the
scripts live at the repo root" is largely unfounded — the grep will come back near-empty, which is
fine, but the plan should not imply matches are expected.

---

## 2. Assumptions

### 2.1 Stated assumptions — audit

| # | Assumption | Verdict |
|---|---|---|
| 1 | Name is `legacy_perl/` | Fixed by user; not relitigated |
| 2 | `test_data.fastq` moves | **Valid** — only `installation.md:99,105`, prose only |
| 3 | `ci_tests.yml` triggers unchanged | **Valid** — `:5` `on: [push, pull_request]`, no `paths:` |
| 4 | `license.txt` stays at root | **Valid** — `release.yml:137` |
| 5 | `test_files/` stays at root | **Valid** — referenced by `ci_tests.yml` args and Rust tests |
| 6 | `git mv` safe on case-insensitive FS, preserves mode | **Valid** for the move; see §1.7 note on live `Docs`/`docs` aliasing |
| 7 | `plans/.gitignore` carries `archived/` | **Valid** — confirmed |
| 8 | Local Mac has perl/gzip/samtools for the oracle battery | Plausible; **but see §3.2 — the battery as written wouldn't prove much even if it runs** |

### 2.2 Unstated assumptions the plan should surface

- **A1 (load-bearing, false):** *the plan's grep idiom covers all path forms.* It covers exactly one
  (`../../<name>`). The repo uses at least three others: `$(cd ... && pwd)/<name>` (6 files),
  `$REPO_ROOT/<name>` (4 files in `scripts/`, 1 in `tests/data/bam2nuc`), and one hardcoded absolute
  path (§I5). This single assumption is the root cause of C1, C2, and C3.
- **A2 (false):** *`perl-oracle` covers every oracle path.* See §3.1.
- **A3 (false):** *`cargo test -- perl_oracle perl_vs_rust drift` selects all affected tests.* See §3.2.
- **A4:** *`rust_ci.yml` will run on this PR.* True but non-obvious and worth stating — `rust_ci.yml:5`
  and `:9` gate on `paths: ["rust/**", ".github/workflows/rust_ci.yml"]`. Commit 2 touches `rust/**`,
  so it triggers. A hypothetical root-only variant of this change would **silently skip all Rust CI**.
- **A5:** *`.gitattributes` Linguist coverage survives.* It does, but by accident — see §5 O1.
- **A6:** *nothing consumes the moved files by absolute path.* Nearly true; one exception (§I5).

### 2.3 I5 — A machine-specific absolute path the plan describes too casually

`rust/bismark/tests/data/filter_nonconversion/generate_goldens.sh:28`:

```bash
PERL_FNC="${PERL_FNC:-/Users/fkrueger/Github/Bismark/filter_non_conversion}"
```

§5 describes this as "default path gains `legacy_perl/`", which is the right edit, but does not flag
that the default is a hardcoded path to one developer's home directory — it works only on Felix's
Mac and is invisible to any repo-relative sweep. This is the only such hardcode in tracked files
(the other hits are `.claude/settings.local.json`, untracked). While touching the line, replace it
with a computed repo root so the next relocation is caught by ordinary greps.

---

## 3. Validation sufficiency

This is the weakest part of the plan, and the reason for the REQUEST CHANGES verdict. The plan
correctly identifies vacuous verification as the risk (§11's G19/H1 lineage) and then builds three
gates that are themselves vacuous.

### 3.1 C4 — `perl-oracle` is not the tripwire §7 claims

§2 and §7 assert the `perl-oracle` job's fail-loud count would catch "any missed oracle path".
I traced all 13 asserted test names (`rust_ci.yml:206` `EXPECTED=13`, list at `:208-220`) to their
files:

| File | Tests in the 13 | Honors `BISMARK_REQUIRE_PERL` |
|---|---|---|
| `genome_prep_integration.rs` | 8 | Yes (`:52`, `:59`) |
| `methylation_consistency_integration.rs` | 3 | Yes (`:487`, `:494`) |
| `dedup_integration_dedup.rs` | 1 | Yes (`:2232`, `:2237`) |
| `extractor_nondir_swapped_flags_1030.rs` | 1 | Yes (`:281`, `:286`) |

Those four files are also **the only four** in the entire Rust tree that read
`BISMARK_REQUIRE_PERL`. Now the three path literals with **no** coverage:

- **`summary_perl_oracle.rs:22` and `:23`** — `perl_script()` (`:20-34`) returns `None` when either
  `../../bismark2summary` or `../../plotly/plot.ly` is missing. Twelve tests then do
  `let Some(script) = perl_script() else { return; }`. Only **one** of the twelve
  (`oracle_wgbs_two_sample`, `:102`) prints a `skipping:` line; the other eleven
  (`:113, :150, :175, :229, :290, :323, :363, …`) return **completely silently**. No
  `BISMARK_REQUIRE_PERL` check. Not among the 13.
- **`summary_template_drift.rs:12`** — `if !src_path.exists() { eprintln!("skipping: …"); return; }`.
  No `BISMARK_REQUIRE_PERL` check. Not among the 13.

So if the implementer fixes five of the eight literals and misses the summary pair, **CI is
completely green** — 12 summary oracles and the template-drift guard pass without comparing
anything. `rust_ci.yml:225`'s `grep -q '^skipping:'` guard cannot help: it inspects only the output
of the `--exact`-filtered 13-test run.

For balance, two consumer classes *are* genuinely protected, though via the main `test` job rather
than `perl-oracle` as the plan states:

- `report_perl_vs_rust.rs:64` fails loud — a missing script makes `perl` exit nonzero and
  `assert!(perl_status.status.success())` (`:71-72`) trips. Its `perl_available()` gate (`:15-21`)
  only probes `perl --version`, not the script.
- Both plotly drift guards fail loud — `report/assets.rs:130` and `summary/assets.rs:115` do
  `std::fs::read_to_string(base.join(name)).unwrap()`, which panics on a missing directory.

### 3.2 C5 — V1/V2's test filter selects 13 tests, not the battery the plan believes

V1 and V2 both run:

```
cd rust && BISMARK_REQUIRE_PERL=1 cargo test -p bismark --no-fail-fast -- \
  perl_oracle perl_vs_rust drift
```

libtest filters on the **test function path**, not the integration-test binary name. Checking the
actual function names:

- `perl_oracle` — matches **nothing**. `summary_perl_oracle.rs` is a *file* name; its tests are
  `oracle_wgbs_two_sample`, `oracle_all_rrbs_raw_mode`, `oracle_single_rrbs_section_asymmetry`,
  `oracle_plot_excluded_sample`, `oracle_mixed_types_die_writes_txt_not_html`,
  `oracle_mixed_case_glob_row_order`, `oracle_nontrivial_g15_tail`,
  `oracle_plot_excluded_in_middle`, `oracle_single_wgbs`, `oracle_all_excluded_zero_plotted`,
  `oracle_explicit_argv_order`, `oracle_basename_zero_truthiness`.
- `drift` — matches **nothing relevant**. The only function repo-wide containing "drift" is
  `rust/bismark/src/dedup/dedup.rs:384 alan_drift_regression_distinct_ends_distinct_positions`,
  entirely unrelated. The plotly guards are `embedded_assets_match_repo_plotly_files` /
  `vendored_assets_match_repo_plotly_files`, and the template guard is
  `embedded_template_matches_perl_heredoc` — none contain "drift".
- `perl_vs_rust` — matches exactly the 13 CI oracles. `report_perl_vs_rust.rs`'s tests are named
  `pe_full_companions_byte_identical`, `se_r1_only_mbias_byte_identical`,
  `nondirectional_unknown_context_byte_identical`, `minimal_alignment_only_byte_identical`,
  `crlf_alignment_byte_identical` — no match.

**Net effect:** V1's sabotage observation and V2's post-fix confirmation exercise 13 tests covering
4 of the 10 path literals. They never touch the report oracle (5 tests), the summary oracle (12),
the template-drift guard (1), or either plotly drift guard (2). V1's central promise — "observe each
gate fail before trusting its green" — is unmet for the majority of the change, and V2's "none skip
under `BISMARK_REQUIRE_PERL=1`" is unachievable because the three unprotected files never read that
variable.

### 3.3 I1 — V3 names a test that does not exist

V3 and the §5 sabotage step both name `embedded_assets_match_repo_plotly_files` "in both modules" /
"(report + summary modules)". Actual names differ:

- `rust/bismark/src/report/assets.rs:119` — `embedded_assets_match_repo_plotly_files`
- `rust/bismark/src/summary/assets.rs:104` — `vendored_assets_match_repo_plotly_files`

Filtering on the plan's name runs one guard and silently omits the other — with no error, because a
filter that matches nothing is not a failure. Same class of bug as C5.

### 3.4 The single highest-value fix

Rather than patching each hole individually, add one **unconditional, tooling-free existence
assertion** — it cannot skip, needs no `perl`, runs in the main `test` job, and converts the whole
layout into an enforced invariant:

```rust
// rust/bismark/tests/legacy_perl_layout.rs
#[test]
fn legacy_perl_toolchain_is_present() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../legacy_perl");
    for f in [
        "bam2nuc", "bismark", "bismark2bedGraph", "bismark2report", "bismark2summary",
        "bismark_genome_preparation", "bismark_methylation_extractor", "coverage2cytosine",
        "deduplicate_bismark", "filter_non_conversion", "methylation_consistency",
        "NOMe_filtering", "plotly/plot.ly", "plotly/plotly_template.tpl",
        "plotly/bismark.logo", "plotly/bioinf.logo",
    ] {
        assert!(dir.join(f).exists(), "legacy_perl/{f} missing — layout assumption broken");
    }
}
```

This makes the silent-skip hole in `summary_perl_oracle.rs` / `summary_template_drift.rs`
non-fatal *for this change*, and it is ~15 lines. Everything else in this section is a refinement.

### 3.5 Other validation gaps

- **No gate on the five missed golden scripts or the four `scripts/` drivers.** Nothing in CI runs
  them. A one-line `bash -n` plus a grep assertion that no `scripts/**` or `tests/data/**` shell file
  contains a non-`legacy_perl` reference to a moved name is the cheap version.
- **V7 is under-specified.** `legacy_perl/bismark --version` and `--help` do not exercise a single
  `$RealBin` shell-out — the couplings at `bismark_methylation_extractor:377,424`, `bismark:941`,
  `bismark2report:1029`, `bismark2summary:140` are the actual risk. V4 delegates this to CI, which is
  reasonable, but V7 should not be presented as a smoke test of the couplings. A local
  `perl legacy_perl/bismark2report --verbose` in a dir with a report fixture would prove plotly
  resolution in seconds.
- **The `.dockerignore` residual risk is stated but not cheaply retired.** `docker build --no-cache`
  reaching the `cargo build` layer would settle it locally without a release dry-run.

---

## 4. Efficiency analysis

Mechanical, no runtime paths touched; §6's core claim is right. Two corrections:

- **The Docker-context figure is off by ~7×.** §6 says "~3 MB of plotly assets + scripts". Actual
  `plotly/` is **10 files, ~14 MB** (`plot.ly` 3.0 MB, `bismark_summary_single_cells.html` 5.4 MB,
  `bismark_bt2_PE_report.html` 2.9 MB, `bismark_summary_WGBS.html` 2.9 MB,
  `bismark_summary_RRBS.html` 3.1 MB, plus logos/template/PNG/txt) — and none of it is currently
  excluded (`.dockerignore` excludes `**/*.md`, not `*.html`). Add `test_data.fastq` (2.1 MB) and
  ~1.1 MB of Perl scripts: the context shrinks by roughly **20 MB**, a better win than claimed.
- **§3.1 says `plotly/` has "4 asset files".** It has 10; four are the assets the code reads
  (`plot.ly`, `plotly_template.tpl`, `bismark.logo`, `bioinf.logo`), six are example reports/images.
  Harmless — the whole directory moves — but the inventory should be right, and the six examples are
  most of the 14 MB.

No complexity, memory, or scalability concerns. One-commit `git mv` is the correct choice for rename
detection and `git log --follow`.

---

## 5. Alternatives

- **A. Centralize the Rust-side path (recommended).** Ten Rust literals plus 8 shell files plus 4
  `scripts/` drivers is 22 sites encoding the same fact. `rust/bismark/tests/common/mod.rs` already
  exists (and `summary_perl_oracle.rs:16` already does `mod common;`); adding
  `pub fn legacy_perl(name: &str) -> PathBuf` there collapses the test-side literals to one
  definition, so the next relocation is a one-line change and any future miss is a compile error
  rather than a silent skip. Trade-off: files not already declaring `mod common;` need one added
  line, and `src/**` drift guards can't use a `tests/` module — a small `const LEGACY_PERL_DIR` in
  the crate covers those two.
- **B. Root compatibility symlinks** (`bismark -> legacy_perl/bismark`, …) to preserve
  `blob/master/<script>` deep links and clone-root-on-PATH users. **Reject** — it reintroduces
  exactly the root clutter that motivates the change, and GitHub does not follow symlinks for blob
  URLs, so it wouldn't even fix the deep links. The plan's "accept and document" is the right call.
- **C. Fold the already-broken golden scripts into scope.** Six data-dir scripts are dead (§1.5).
  Options: fix the up-count (~6 one-char edits, restores real capability), or delete them as
  superseded. Either beats mechanically appending `legacy_perl/` to a path that resolves nowhere. My
  preference: fix the arithmetic — they are the reproducibility record for the checked-in goldens.
- **D. Harden the three unprotected oracle files.** Give `summary_perl_oracle.rs` and
  `summary_template_drift.rs` the same `BISMARK_REQUIRE_PERL` treatment the other four have, and
  raise `rust_ci.yml`'s `EXPECTED` to cover them. This is the *thorough* fix for C4 but is arguably
  a separate hardening change — it alters what CI asserts, not where files live. Reasonable to defer
  behind §3.4's existence test, which is in-scope and sufficient for this move.
- **E. Two commits vs one.** As planned (move, then fix) is right: it keeps rename detection clean
  and makes the sabotage observation possible. Keep it — just note that Commit 1 alone touches no
  `rust/**`, so pushing it in isolation would not trigger `rust_ci.yml` (§A4).

---

## 6. Action items

### Critical — do not implement until addressed

1. **C1** Add the five missed golden scripts to §5: `nome_filtering/phase_b/generate_goldens.sh:9`,
   `coverage2cytosine/{phase_b:5, phase1:13, phase2_drach:15, phase3_ffs:12}/generate_goldens.sh`.
2. **C2** Retract the "`scripts/` grep-clean" claim in §2 and §11; add
   `scripts/c2c_byte_identity_matrix.sh:120` (+ usage `:29`), `scripts/phase_h_smoke.sh:134`
   (+ usage `:57`), `scripts/phase_h_se_matrix.sh:116`, `scripts/phase_h_pe_matrix.sh:163`.
3. **C3** Replace the straggler sweep with one that can actually fail. Cover the whole repo (not just
   `rust`), drop the `../../`-adjacency requirement, and exclude only `target/`, `plans/`, `.git/`.
   Something like: `grep -rnE '(bam2nuc|bismark2bedGraph|bismark2report|bismark2summary|bismark_genome_preparation|bismark_methylation_extractor|coverage2cytosine|deduplicate_bismark|filter_non_conversion|methylation_consistency|NOMe_filtering)' --include='*.rs' --include='*.sh' --include='*.yml' .` piped through a filter for `legacy_perl`, then triage the (larger) hit list by hand. Verify the new sweep flags all 9 files listed above **before** fixing them — that is the sweep's own sabotage test.
4. **C4** Correct §2/§7: `perl-oracle` covers 13 tests from 4 files only
   (`rust_ci.yml:206-220`). `summary_perl_oracle.rs:22,23` and `summary_template_drift.rs:12` have
   **no** fail-loud gate and do **not** read `BISMARK_REQUIRE_PERL`; 11 of the 12 summary oracles
   return without even printing `skipping:`. State that `report_perl_vs_rust.rs` and both plotly
   guards are protected via the main `test` job (not `perl-oracle`).
5. **C5** Fix V1/V2's filter. `perl_oracle` and `drift` match nothing; `perl_vs_rust` matches only
   the 13. Use explicit test names, or run the affected test binaries wholesale
   (`--test summary_perl_oracle --test summary_template_drift --test report_perl_vs_rust …`), and
   add `--lib` for the two `src/` drift guards. Re-scope V2's "none skip" claim to the four files
   that can honor it.
6. **Add the §3.4 existence test** (`legacy_perl_layout.rs`) as an explicit §5 task and V-row. It is
   the only proposed gate that cannot skip, and it closes C4/C5's blast radius.

### Important

7. **I1** Fix V3/§5: the summary guard is `vendored_assets_match_repo_plotly_files`
   (`summary/assets.rs:104`), not `embedded_assets_match_repo_plotly_files`.
8. **I2** `ci_tests.yml` has **44** invocations, not 43; drop the `^\s+` anchor so
   `:37 run: ./bismark --help` is included.
9. **I3** State that all six data-dir golden scripts already resolve to a nonexistent
   `rust/<script>`, and decide: fix the up-count, or mark the edits cosmetic.
10. **I6** Retract "zero Perl code changes". `copy_bismark_files_for_release.pl:18,20,27` reads
    `CHANGELOG.md`, `license.txt`, and `Bismark_alignment_modes.pdf` from its **own** directory via
    `__FILE__`; after the move stage 1 dies at `:45`. Choose: patch those three to `$dist_dir/..`,
    leave the packager at root, or accept-and-document (noting stage 3 already dies on the missing
    `Docs/make_docs.pl`).
11. **I4** Decide on `rust/README.md:102` ("live at the repository root") — arguably still true in
    its `v0.25.1` context, but it must be a decision, not an omission.
12. **I5** While editing `filter_nonconversion/generate_goldens.sh:28`, replace the hardcoded
    `/Users/fkrueger/Github/Bismark/...` default with a computed repo root.
13. Strengthen **V7** to exercise a `$RealBin` coupling (e.g. `perl legacy_perl/bismark2report` over
    a report fixture, proving `:1029` plotly resolution) rather than `--version`/`--help`.

### Optional

14. **O1** `.gitattributes:10` `plotly/**` is slash-anchored at root and goes dead after the move.
    Linguist coverage survives via `**/plotly/**` (`:11`) and `*.ly` (`:9`), so behavior is
    unchanged — remove the dead line for hygiene. Not currently mentioned in the plan.
15. **O2** `.prettierignore:2` `plotly/` uses gitignore semantics (no internal slash → matches at any
    depth), so it still covers `legacy_perl/plotly/`. No prettier job exists in any workflow, so this
    is dev-convenience only. Worth one confirming line in §3's "explicitly unchanged".
16. **O3** Fix §3.1 ("4 asset files" → 10) and §6 ("~3 MB" → ~20 MB).
17. **O4** `.claude/settings.local.json:7-27` holds ~10 Bash permission entries with absolute root
    Perl paths; they stop matching after the move (extra permission prompts). Untracked, so outside
    the PR — mention as a post-merge nuisance.
18. Consider **Alternative A** (centralized `legacy_perl()` helper) to make the next relocation a
    one-line change and a compile error rather than a silent skip.
19. **A4** Record in §8 that `rust_ci.yml:5,9` gate on `paths: ["rust/**", …]`, so Rust CI runs here
    only because Commit 2 touches `rust/**`.

---

## 7. What is genuinely strong in this plan

Worth stating so the rewrite does not lose it: the Perl-side analysis is excellent and I could not
fault it. The `$RealBin` atomic-move insight, the `license.txt`-stays-at-root catch traced to
`release.yml:137`, the `release.yml`-loops-are-Rust-symlinks disambiguation, the `use lib` no-op
observation, the `repo_root()` altitude guard, and the ci_tests-outputs-land-in-CWD reasoning are all
verified-correct and each is the kind of thing a mechanical find-and-replace would have broken. The
sabotage-first discipline is the right instinct.

The failure is narrower than the finding count suggests: the plan trusted **one grep idiom** to
enumerate consumers and **one CI job** to catch misses, and neither is as broad as assumed. Fix the
enumeration method and the gates, and the rest of the plan stands.
