# CODE REVIEW A — Phase 1: CLI + options + discovery (`bismark-aligner`)

- **Reviewer:** A (independent, fresh context)
- **Date:** 2026-06-01
- **Scope:** `rust/bismark-aligner/src/{cli,discovery,aligner,options,config,error,lib,main}.rs`,
  `tests/cli.rs`, `Cargo.toml`. Grounded against Perl `bismark` v0.25.1
  (`process_command_line` 7247–8451, `read_genome_into_memory` FASTA glob 5031–50) and the rev-1 PLAN.
- **Mode:** AUDIT — no code modified (dual reviewers run in parallel; fixes recommended, not applied).
- **Build/test:** `cargo test -p bismark-aligner` → **24 pass** (15 unit + 9 integration). `cargo clippy
  -p bismark-aligner --all-targets -- -D warnings` → **clean**. `cargo fmt --check` → **clean**. Workspace
  member registered in `rust/Cargo.toml`.

## Summary

Solid, faithful Phase-1 skeleton. The **byte-identity-critical `aligner_options` push order
(`options.rs`) is correct** against Perl 7811–8142 — format → phred → `-N`/`-L`/`-D`/`-R` → `--score-min`
→ `--rdg`/`--rfg` → `-p`/`--reorder` → `--ignore-quals` → PE flags → `--minins`/`--maxins`(/`500`) →
`--quiet`. The seed-flags-before-`--score-min` and `--ignore-quals`-not-last invariants (the rev-1 plan
fix) are both honored and locked by a dedicated ordering test. The small/large `.bt2`/`.bt2l` index
fallback, the FASTA priority-fallback, the deferral errors (HISAT2/minimap2/SAM/CRAM), and the v1-spine
validations all match Perl's intent. Error/exit-code handling is clean (1 for `AlignerError`, 2 for clap).

The **one substantive finding** is that `discovery.rs` diverges from its **adjudicated sibling**
`bismark-genome-preparation` on FASTA discovery in three ways (extension case-sensitivity, non-UTF-8
names, symlink following). Both ports implement the same Perl `<*.fa>` glob and **must agree** because
they jointly define the `@SQ`/index ordering contract — the aligner sets `@SQ` order (Phase 5), genome-prep
set the index/MFA order. The genome-prep version was settled on Linux CI; the aligner re-implemented it
more loosely. The rest are Medium/Low faithfulness nits and test gaps that are not Phase-1 blockers.

---

## Issues by area

### Logic / faithfulness

**[High] H1 — FASTA extension match is case-INSENSITIVE; the adjudicated sibling (and Perl `<*.fa>`) is
case-SENSITIVE.**
`discovery.rs:154` filters with `name.to_ascii_lowercase().ends_with(suffix)`, so `chr1.FA`,
`genome.Fasta`, etc. are matched as `.fa`/`.fasta`. Perl's `<*.fa>` glob (`File::Glob` csh_glob) matches
the *pattern* **case-sensitively** on Linux (`GLOB_NOCASE` is not set there); only the *sort* folds case.
The sibling `bismark-genome-preparation/src/discovery.rs:18` — verified on Linux CI — matches the
extension case-sensitively: `name.ends_with(b".fa") && !name.ends_with(b".fa.gz")`. Its mixed-case test
(`discovery.rs:218`) uses only lowercase `.fa` extensions; the fold is for the **stem**, not the
extension.
- **Impact:** a genome dir containing only `chr1.FA` → Perl/genome-prep find no `.fa` (fall through, then
  die "no FASTA"); the aligner accepts it. Worse, a mixed dir where uppercase-extension files exist would
  yield a **different FASTA set → different `@SQ` order/content** than the index genome-prep built →
  Phase-5 gate failure. Even if rare, the two ports diverging on the shared discovery contract is a latent
  correctness bug.
- **Fix:** match the extension on **raw bytes, case-sensitively**, exactly as genome-prep's `in_group`
  (`name.ends_with(b".fa") && !name.ends_with(b".fa.gz")`, etc.). Keep the case-insensitive *sort*. Add a
  test asserting `chr1.FA` is NOT matched. Best shared via a tiny helper or by copying genome-prep's
  `in_group` verbatim. Adjudicate on Linux/oxy, never macOS (per the epic's glob-fold landmine).

**[Medium] M1 — non-UTF-8 FASTA filenames are silently dropped.**
`discovery.rs:145` uses `e.file_name().to_str().map(...)` — a non-UTF-8 name yields `None` and is filtered
out. genome-prep explicitly fixed this in *its* code review (M1 there): it matches on
`OsStr::as_encoded_bytes()` so a non-UTF-8 `.fa` is not dropped (`discovery.rs:64`, test
`glob_includes_non_utf8_name` at :237). The aligner reintroduced the bug the sibling already closed.
- **Impact:** a genome FASTA with a non-UTF-8 name (uncommon but real on some filesystems) is invisible to
  the aligner but visible to genome-prep/Perl → `@SQ` mismatch.
- **Fix:** filter/sort on `as_encoded_bytes()` like genome-prep (folds into the H1 fix).

**[Medium] M2 — symlinked FASTA files are excluded (DirEntry::file_type does not follow symlinks).**
`discovery.rs:144` uses `e.file_type().map(|t| t.is_file())`. `DirEntry::file_type()` does **not** traverse
symlinks (a symlink reports `is_symlink()`, so `is_file()` is false). Perl `<*.fa>` returns symlinked
names (it doesn't even `-f`-filter), and genome-prep uses `p.is_file()` (`discovery.rs:62`) which **stats
through** symlinks. Genome dirs that symlink the reference FASTA (common) would be handled by Perl/genome-prep
but dropped by the aligner.
- **Fix:** use `genome_dir.join(name).is_file()` (follows symlinks) for the file-vs-dir test, matching
  genome-prep. (Note: Perl globs directories too, but `read_genome_into_memory` then tries to `open` them;
  filtering to files-or-symlinks-to-files is the safer faithful choice and matches genome-prep.)

**[Medium] M3 — `--pbat` skips Perl's two pbat-specific dies (`--pbat` + `--gzip`, `--pbat` + `-f`).**
Perl 8155–8156 dies on `--pbat --gzip` and on `--pbat -f/--fasta`. `config.rs::resolve_library`
(191–204) only checks `--non_directional` vs `--pbat` mutual exclusion. So `bismark_rs --pbat --gzip …`
and `bismark_rs --pbat -f …` **resolve successfully** in Rust but die in Perl. Phase 1 deliberately
*resolves* pbat (parsed + stored), so accepting an invalid pbat combination is a silent half-support —
exactly the "v1-wired vs deferred boundary must be enforced consistently" risk the plan §11 flags.
- **Fix:** add the two dies in `resolve_library` (or a dedicated pbat-validation step): error on
  `cli.pbat && cli.gzip` and `cli.pbat && cli.fasta`, mirroring the Perl messages.

**[Low] L1 — `--multicore`, `--basename`+`--multicore`, `--sam`+`--multicore`, `--rg_sample`-without-`--rg_id`
validations not replicated.**
Perl validates `--multicore > 0` (die on 0/negative; 8245), `--basename` + `--multicore > 1` (8260),
`--sam` + `--multicore` (8252), and `--rg_sample` requires `--rg_id` (8271). The Rust stores these but
never validates. They are genuinely deferred (multicore = Phase 9, RG = output phase), so Low — but flag
them so the boundary is tracked and not forgotten. `--multicore 0` in particular is accepted today.

**[Low] L2 — aligner detection runs AFTER genome discovery (Perl runs it first); error precedence differs.**
Perl resolves aligner + path + `ensure_the_aligner_is_working` at 7414–7514 **before** genome discovery
(7604+) and SAM/CRAM/samtools (7517–7593). The Rust `config.rs::resolve` (133–145) runs
`discover_genome` *before* `detect_bowtie2`, and `resolve_output` (SAM/CRAM error) *after* both. With both
a bad genome and a missing bowtie2, Perl reports the bowtie2 error first; Rust reports the genome error
first. STDERR-only / both exit 1 → not byte-gated, Low. Optional: reorder to detect the aligner before
discovery for diagnostic parity.

**[Low] L3 — bowtie2 version regex is looser than Perl's.**
Perl 7078: `bowtie.*\s+version\s+(\d+\.\d+\.\d+)` requires the literal substring `bowtie` *before*
`version` and whitespace-delimited `version`. `aligner.rs::parse_bowtie2_version` (91–93) only needs a
line `contains("version")` then takes the next whitespace token, so it would accept `foo version 2.5.5`
(no "bowtie") and could be fooled by a path containing "version". Real bowtie2 always says
`bowtie2-align-s version x.y.z`, so impact is negligible; tighten to require `bowtie` if strict parity is
wanted.

**[Low] L4 — `--path_to_bowtie2` error text copies the minimap2 wording, not the bowtie2 wording.**
`aligner.rs:34` says `"...is invalid (it MUST be a directory)!"` which matches Perl's **minimap2** message
(7491). The bowtie2 message (7456) is `"...is invalid (not a directory)!"`. Error text is not byte-gated;
cosmetic. Use the bowtie2 phrasing for migrating users.

**[Low] L5 — no-genome path: Rust exits 1, Perl prints help and exits 0.**
Perl 7612–7616 `print_helpfile(); exit;` (exit **0**) when no genome. Rust returns a usage `Validation`
error → exit 1. Usage path, not byte-gated; exit-1-on-bad-usage is arguably better UX, but it differs from
Perl. Low.

### Efficiency
Nothing hot (a few `stat`s + one `bowtie2 --version`). Allocation in `discover_fastas` (collect all
entries, then re-scan per extension category) is fine for a genome dir. No concerns.

### Errors / robustness
- Exit-code mapping is correct (1 for `AlignerError`, 2 for clap via derive; `--version` handled manually
  and exits 0). `command_line` is captured from `args().get(1..)` **before** `Cli::parse_from`, correctly
  excluding the program name and matching the `@PG CL:` contract (lib.rs / main.rs).
- `discover_genome` uses `canonicalize` (existence + absolute path, faithful to Perl `chdir`+`getcwd`).
- The Perl large-index loop has a latent dead-code `die`-inside-loop (7686) that makes the large check
  "die on first missing"; the Rust `first_missing` short-circuit reproduces the *effective* behavior
  faithfully. (Perl's per-missing-file `warn`s on the small index are STDERR diagnostics, not replicated —
  acceptable, not byte-gated.)

### Structure / style
- Clean module split mirroring the sibling genome-prep layout; good doc comments citing Perl line ranges;
  `GapPenalties` semantics (`--rdg`→deletion, `--rfg`→insertion, 5/3 defaults) match Perl 7962–7988 exactly.
- `--genome` is the canonical clap name with `--genome_folder` as alias; Perl only has `--genome_folder`.
  This is an intentional, plan-documented (§13, line 153) addition; both spellings work. Fine.
- **`--bam` flag is missing** from the CLI surface. Perl GetOptions has `'bam'` (7354). `bismark_rs --bam …`
  is an unknown-flag clap error (exit 2) though BAM is the default. Trivial surface gap; add a no-op
  `--bam` bool for argv/`@PG` fidelity. (Low.)

### Tests
- Unit + integration tests assert **real behavior** (exact option strings, real index `stat`s, a real
  fake-`bowtie2` exec, real STDERR config dump) — no mocks/tautologies. Good.
- **[Medium] T1 — PE resolution is untested end-to-end.** `config.rs::resolve_layout` (239–296) has four
  PE/SE validation branches — `--single_end`+`-1` conflict (242), mate-count mismatch (254), `-1`==`-2`
  same-file die (260), `-2`-without-`-1` (271) — with **zero** test coverage. Plan §9 #13 explicitly lists
  "`-1`==`-2`" as a required validation; only the missing-input-file case is tested
  (`missing_input_file_errors`). Add integration tests for `-1`/`-2` mismatch, same-file, and SE/PE conflict.
- **[Low] T2 — no test exercises the H1 case** (uppercase/mixed-case extension). The existing FASTA test
  proves the case-fold *sort* but not the extension *match* — which is exactly why the H1 divergence slipped
  through. Add `chr1.FA`-not-matched once H1 is fixed.
- **[Low] T3 — large-index (`.bt2l`) fallback is untested.** `discover_genome`'s large-index path
  (105–126) has no test (small index always present in fixtures). Add a fixture with only `.bt2l` files.
- §9 #8 (real bowtie2 2.5.5 version-parse + pin-warn) is correctly deferred to oxy/CI (hermetic fake used
  locally) — acceptable.

---

## Recommendations (prioritized)

1. **[High] H1** — make FASTA extension matching case-SENSITIVE on raw bytes, identical to
   `bismark-genome-preparation::discovery::in_group`; keep the case-insensitive sort. The two ports MUST
   agree on the `@SQ`/index ordering contract. Add a `chr1.FA`-not-matched test. Adjudicate on Linux.
2. **[Medium] M1 + M2** — fold non-UTF-8 (match on `as_encoded_bytes`) and symlink-following (`join().is_file()`)
   into the H1 rewrite so `discover_fastas` is byte-for-byte aligned with the sibling.
3. **[Medium] M3** — add the `--pbat`+`--gzip` and `--pbat`+`-f` dies (Perl 8155–56) to `resolve_library`
   so pbat does not silently half-resolve invalid combinations.
4. **[Medium] T1** — add PE-resolution integration tests (mate mismatch, `-1`==`-2`, SE/PE conflict);
   close plan §9 #13.
5. **[Low] L1–L5, L2-reorder, T2, T3, `--bam`** — track the deferred validations (multicore/basename/RG),
   tighten the version regex, fix the `--path_to_bowtie2` message wording, add the `.bt2l` test, add the
   missing `--bam` no-op flag. None block Phase 1.

**Verdict:** Phase 1 is functionally complete and the byte-identity-critical `aligner_options` assembly is
faithful. Recommend addressing **H1 (and the M1/M2/M3 cluster)** before Phase 5's `@SQ` gate, since the
FASTA-discovery divergence from the adjudicated sibling is the only finding that can break the byte gate.
