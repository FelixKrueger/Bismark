# Code Review B — Phase 1: `bismark-aligner` (CLI + options + discovery + detection)

**Reviewer:** B (independent, fresh context)
**Date:** 2026-06-01
**Scope:** `rust/bismark-aligner/src/{cli,discovery,aligner,options,config,error,lib,main}.rs`, `tests/cli.rs`, `Cargo.toml`
**Gate context:** acceptance is byte-identical *decompressed* SAM vs Perl Bismark v0.25.1 / Bowtie 2 2.5.5. Phase 1 performs **no** alignment, so nothing here is on the byte gate *yet* — but `aligner_options`, the FASTA `@SQ` order, and `command_line` (the `@PG` `CL:`) are all consumed by later byte-gated phases, so faithfulness to the Perl source is judged as load-bearing.

## Summary

Solid, faithful Phase-1 implementation. Build is clean, `cargo clippy -p bismark-aligner --all-targets -- -D warnings` passes with no warnings, and all **15 unit + 9 integration tests pass** (verified locally with the sandbox disabled). The byte-identity-critical pieces are correct against the Perl source:

- **`aligner_options` push order** in `options.rs` matches Perl `bismark` 7811–8142 exactly: `-q`/`-f` → `--phred33/64` → `-N` → `-L` → `-D` → `-R` → `--score-min` → `--rdg` → `--rfg` → `-p`/`--reorder` → `--ignore-quals` → (PE: `--no-mixed`/`--no-discordant`/`--dovetail`) → `--minins` → `--maxins`/`--maxins 500` → `--quiet`. `--ignore-quals` is correctly *not* last. Default SE = `-q --score-min L,0,-0.2 --ignore-quals` (Phase-0 verified). The ordering test (`seed_flags_precede_score_min_and_quiet_is_last`) locks the rev-1 correction.
- **Index discovery** (`discovery.rs`) mirrors 7646–7708: CT-then-GA, small (`.bt2`) first with large (`.bt2l`) fallback, and faithfully replicates Perl's behaviour of *not* dieing on incomplete small index (falls through to large) while dieing on the first missing *large* file (Perl's `die` at 7686/7697 makes the subsequent `=0` dead code — Rust's early-return matches).
- **Detection** (`aligner.rs`) mirrors 7060–7092 + 7444–7462: `--path_to_bowtie2` must be a directory, `bowtie2` appended; else PATH; runs `--version`, non-zero → die; pin-warn on `!= 2.5.5`.
- **Resolution** (`config.rs`) mirrors the Perl precedence and the mutual-exclusivity dies; `--genome`-vs-positional matches `shift @ARGV` (7609); PE/SE/format precedence and the deferral errors (hisat2/minimap2/sam/cram/local) are enforced.

The findings below are almost all **niche edge-case divergences** (off-gate, off-spine) plus a few quality nits. **No Critical issues.** The one I'd most want addressed before later phases lean on this is the *silent acceptance of several parsed-but-deferred flags* (High), because the plan itself (§11) called that out as the standing risk.

---

## Issues by area

### Logic / faithfulness

**[High] Several parsed-but-deferred flags are silently accepted and ignored (no deferral error).**
`config.rs` only emits "deferred"/"unsupported" errors for `--hisat2`, `--minimap2`, `--sam`, `--cram`, `--local`. But `--multicore`/`--parallel`, `--unmapped`, `--ambiguous`, `--ambig_bam`, `--nucleotide_coverage`, `--rg_tag`/`--rg_id`/`--rg_sample`, `--slam`, `--non_bs_mm`, `--skip`/`-s`, `--upto`/`-u`, `--prefix`, `--gzip`, `--most_valid_alignments` are parsed and then **never referenced** in any resolution path (confirmed by grep — no hits in `config.rs`/`options.rs`/`lib.rs`/`main.rs`). The binary prints a happy "resolved configuration" and exits 0 as if these were honoured. This is exactly the "binary never silently half-supports a later-phase mode" risk the plan flagged in §11. For Phase 1 (no alignment) it's harmless *today*, but it sets a trap: e.g. `--multicore 4` looks accepted but does nothing, and Perl (8244–46) at least validates `$multicore > 0`.
*Recommendation:* either (a) add an explicit "parsed but not yet wired (Phase N)" error for the flags that materially change output (`--multicore`, `--unmapped`, `--ambiguous`, `--nucleotide_coverage`, `--rg_tag`, `--slam`, `--non_bs_mm`, `--gzip`), or (b) surface them in the summary as "(parsed, not yet wired)" so the no-op is visible. Pure-passthrough/no-effect flags (`--dovetail`, `--most_valid_alignments`) can stay silent. At minimum, port Perl's `--multicore > 0` validation. (Perl: 8244–8249.)

**[Low] FASTA extension matching is case-insensitive in Rust but Perl's `<*.fa>` glob is case-sensitive on the pattern.**
`discovery.rs:154` matches with `name.to_ascii_lowercase().ends_with(suffix)`, so a file named `Genome.FA` or `chr1.Fasta` is matched. Perl's `read_genome_into_memory` (5031–46) uses `<*.fa>` / `<*.fa.gz>` / `<*.fasta>` / `<*.fasta.gz>`, which on a case-sensitive FS matches only the lowercase extension — `Genome.FA` would *not* be globbed by Perl. This is a divergence in *which files are discovered* (and therefore the `@SQ` set), distinct from the documented case-insensitive *sort* concern. It only bites a genome dir holding upper/mixed-case FASTA extensions (rare), so Low — but flag it for the Linux/oxy `@SQ` adjudication: the safe-faithful choice is to match the extension **case-sensitively** (`name.ends_with(suffix)`) and keep only the *sort* case-insensitive. (Perl: 5031–5046.)

**[Low] Aligner detection runs *after* genome discovery; Perl detects the aligner first.**
`config.rs::resolve` orders `discover_genome` (line 141) **before** `detect_bowtie2` (line 142). Perl calls `ensure_the_aligner_is_working` at 7507 — *before* the genome/index discovery at 7604+. So when both a bad genome dir and a missing/broken `bowtie2` are present, Perl emits the aligner-detection die first; Rust emits the genome-folder error first. Off-gate (stderr only, no alignment), but a user-visible error-precedence divergence. Low. (Perl: 7507 vs 7604.)

**[Low] `--score_min`/`--rdg`/`--rfg` empty-string edge diverges from Perl's truthiness gate.**
Perl gates these with `if ($score_min)` / `if ($rdg)` / `if ($rfg)` — an **empty string is falsy**, so `--score_min ""` / `--rdg ""` fall back to the defaults silently. In Rust, `cli.score_min = Some("")` → `valid_score_min_l("")` is false → `Validation` error; `--rdg ""` → `parse_int_pair("")` → None → error. Niche (nobody passes an empty option value), Low. (Perl: 7895, 7960, 7976.)

**[Low] Version regex is more permissive than Perl's.**
`aligner.rs::parse_bowtie2_version` finds the first line containing `"version"` and takes the next token. Perl's regex is `bowtie.*\s+version\s+(\d+\.\d+\.\d+)` (7078) — it additionally requires the literal `bowtie` earlier on the line and whitespace before `version`. For the real Bowtie 2 banner (`.../bowtie2-align-s version 2.5.5`) both extract `2.5.5` identically. Divergence only on pathological banners, and the parsed version feeds reports (not the BAM byte gate). Low.

### Errors / edge cases

**[Medium] Integer option widths are narrower than Perl's `=i` (signed), changing the accept/reject boundary.**
`cli.rs` declares `seedlen`/`-D`/`-R`/`-I`/`-X`/`multicore`/`mm2_maximum_length` as `u32` and `skip`/`upto` as `u64`. Perl's `GetOptions` uses `=i` (signed, effectively arbitrary-precision-ish via Perl scalar). Consequences: (1) a negative value like `-L -5` is a clap parse error (exit 2) in Rust but Perl accepts it and would push `-L -5`; (2) `--rdg 99999999999` (all digits, > `u32::MAX`) — Perl accepts and pushes the string, Rust's `parse_int_pair` `.parse::<u32>()` fails → error. These are nonsensical inputs that Bowtie 2 would itself reject, so impact is low, but the *accept/reject boundary* differs from the oracle. If strict argv-fidelity for `@PG` ever matters, these would need wider/looser types. Recommend at least a comment documenting the deliberate narrowing. (Perl: 7326–7336, GetOptions `=i`.)

**[Low] `--bam` flag from Perl is not declared.**
Perl declares `'bam' => \$bam` (7354). BAM is the v1 default so semantically a no-op, but `bismark_rs --bam <genome> <read>` is a clap parse error (exit 2) whereas Perl accepts it. Trivial surface gap; add a no-op `--bam` flag for invocation compatibility. (Perl: 7354.)

**[Low] `Cargo.toml` references a missing `README.md`.**
`readme = "README.md"` (Cargo.toml:10) but no `README.md` exists in the crate dir. Harmless for `build`/`test`/`clippy`; only warns on `cargo package`/`publish`. Add the file or drop the key.

### Efficiency

No concerns. The phase is a handful of `stat`s + one subprocess; `discover_fastas` collects the dir listing once and probes four suffixes — fine. (As §6 of the plan states, nothing is hot.)

### Structure / style

**[Low] `discover_fastas` clones `PathBuf`s out of borrowed tuples.**
`discovery.rs:152–166` collects `Vec<&(String, PathBuf)>` then `.map(|(_, p)| p.clone())`. Minor; could `into_iter()` over an owned, filtered `Vec` to avoid the clone, but the cost is negligible and the borrow form keeps the sort readable. Optional.

**[Low] `resolve_bowtie2_path` PATH branch swallows the `which` error.**
`aligner.rs:41`: `which::which("bowtie2").or_else(|_| Ok(PathBuf::from("bowtie2")))` deliberately falls back to the bare name so the failure surfaces at `--version` exec time (matching Perl, which just uses the literal `'bowtie2'`). This is intentional and documented in the comment — good. Noting it only so the next reader doesn't "fix" it: the fallback is load-bearing for Perl parity.

**[Low] `RunConfig::summary` prints `{:?}` for several paths.**
`config.rs:360,371,373,374` use `{:?}` (debug, quoted/escaped) for `fasta_kind`, `output_dir`, `basename`. Summary is explicitly not byte-gated (stderr), so cosmetic only.

---

## Test quality

Tests assert **real behaviour**, not tautologies/mocks:

- `options.rs` unit tests assert exact option strings incl. the rev-1 ordering correction (`seed_flags_precede_score_min_and_quiet_is_last`), the `--score_min` substitution, the PE tail + `--maxins 500`, `-f`, `--local` rejection, phred-without-`-q`, bad `-n`, and rdg/rfg validation + gap-penalty side effects. Strong.
- `discovery.rs` tests cover `.fa` priority + case-insensitive sort (`a.fa` before `B.fa`, `.fa.gz` ignored), `.fasta` fallback, incomplete index → `FaultyIndex`, no-FASTA → `NoFasta`. Good.
- `aligner.rs` tests cover the standard version line + the triple validator. Good.
- `tests/cli.rs` exercises the real binary end-to-end with a hermetic fake `bowtie2` (`echo "...version 2.5.5"`): version banner, no-genome, hisat2/minimap2 deferral, missing input, happy-path summary (asserts the options string + `single-end` + `Bowtie 2 2.5.5`), missing index, SAM deferral, and genome-as-positional + pbat. Solid integration coverage.

**Test gaps (recommend adding, none blocking):**
1. **No `large_index` (`.bt2l`) discovery test.** All fixtures build the small `.bt2` index; the `large_index = true` path (discovery.rs:112–124) and `RunConfig.large_index` flag are untested. Add a fixture with only `.bt2l` files. (Maps to plan §3.6.)
2. **No paired-end integration test.** `resolve_layout`'s PE branch (mate-count mismatch, `-1`==`-2`, `-2` without `-1`, `--single_end`+`-1` conflict, per-mate existence checks) is only exercised in unit-less form — there are no tests at all for the PE dies. Add unit tests on `resolve_layout` or CLI tests with `-1`/`-2`. This is the single biggest coverage hole given PE is a v1-targeted layout. (Plan §3.5, validation row #13 names `-1`==`-2` but no test implements it.)
3. **No `--basename` output-name test** despite validation row #11 promising `foo.bam` (not `foo_bismark_bt2.bam`). Output-name derivation isn't implemented in Phase 1 (deferred to Phase 5/6 per §13), so the assertion can't be made yet — but the validation table lists it as a Phase-1 check. Either implement the basename→name rule or move row #11 to Phase 5 in the plan. **Plan/impl mismatch worth flagging to the planner.**
4. **No `--phred64`-with-`-q` accepted-path test** (only the `-f` rejection is tested) and **no `-p`/`--reorder` ordering test** (the `-p 4` → `-p 4 --reorder` push and the `< 2` die).
5. **No real-bowtie2 2.5.5 version + pin-warn test** (validation row #8) — expected to run on CI/oxy, not local; fine to defer but note it's unmet locally.

---

## Recommendations (prioritized)

| Priority | Issue | Action |
|---|---|---|
| **Critical** | — | none |
| **High** | Silent acceptance of parsed-but-deferred flags (`--multicore`, `--unmapped`, `--ambiguous`, `--nucleotide_coverage`, `--rg_tag`, `--slam`, `--non_bs_mm`, `--gzip`) | Add explicit "not yet wired (Phase N)" errors for the output-affecting ones, or surface in summary; port Perl's `--multicore > 0` validation. Addresses plan §11's standing risk. |
| **High (test)** | No paired-end layout tests | Add unit/CLI tests for the PE dies (mate mismatch, `-1`==`-2`, `-2` w/o `-1`, `--single_end`+`-1`). |
| **Medium** | Integer option widths narrower than Perl `=i` | Document the deliberate narrowing; widen if strict `@PG` argv-fidelity becomes a gate. |
| **Medium (test)** | No `.bt2l` large-index discovery test | Add a large-index fixture. |
| **Low** | FASTA extension match is case-insensitive vs Perl's case-sensitive glob | Match extension case-sensitively, keep sort case-insensitive; adjudicate on Linux/oxy. |
| **Low** | Detection ordered after discovery (Perl detects first) | Reorder `detect_bowtie2` before `discover_genome` in `resolve` for error-precedence parity. |
| **Low** | `--score_min ""`/`--rdg ""` empty-string edge diverges | Treat empty option value as "use default" to match Perl truthiness, or accept the niche divergence. |
| **Low** | Version regex more permissive than Perl | Optionally require `bowtie` on the line + whitespace before `version`. Off-gate. |
| **Low** | `--bam` flag not declared (Perl 7354) | Add a no-op `--bam` flag for invocation compatibility. |
| **Low** | `Cargo.toml` `readme = "README.md"` missing file | Add README or drop the key. |
| **Low (test)** | Missing `--phred64`+`-q` accepted path, `-p`/`--reorder` ordering, `--basename` (or move to Phase 5) | Add the missing option-string tests; reconcile validation row #11 with the deferred output-naming. |

**Verification performed:** `cargo test -p bismark-aligner` → 15 unit + 9 integration + 0 doc, all pass. `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → clean. (Both run with the sandbox disabled per the worktree note.)
