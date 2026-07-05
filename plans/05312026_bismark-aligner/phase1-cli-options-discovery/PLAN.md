# PLAN — Phase 1: CLI + option parsing + genome/index discovery + aligner detection

> **Epic:** `05312026_bismark-aligner/EPIC.md`, Phase 1 — *CLI + options + discovery*
> Depends on: **Phase 0** (`phase0-determinism-spike/SPIKE_determinism.md`, ✅ premise holds). No alignment in this phase.

## 1. Goal

Stand up the `bismark-aligner` crate skeleton (binary `bismark_rs`) and implement everything that happens
**before** the first read is aligned: parse the command line, resolve run mode, validate + discover the
bisulfite genome indexes and raw FASTA, detect/verify the external aligner, and assemble the exact
`aligner_options` string. The phase deliverable is a binary that parses → validates → discovers → detects
→ prints a resolved-config summary → exits 0, with no alignment yet. This produces the `RunConfig` value
that every later phase consumes.

## 2. Context

- **New crate:** `rust/bismark-aligner` (add to `rust/Cargo.toml` `members`). Binary `bismark_rs`.
  Edition 2024, rust 1.89, GPL-3.0-only; mimalloc global allocator (output-neutral).
- **Perl source of truth:** `bismark` `sub process_command_line` (lines 7247–8451), `sub
  ensure_the_aligner_is_working` (7060–7092). Cite these for exact replication during implementation.
- **Does NOT use `bismark-io`/noodles yet** (no BAM I/O in Phase 1). Deps: `clap` (derive), `anyhow` +
  `thiserror`, `which` (aligner discovery), `mimalloc`.
- **Input contract (from `bismark_genome_preparation`):** `<genome>/Bisulfite_Genome/CT_conversion/BS_CT.*`
  + `GA_conversion/BS_GA.*` index files, and the raw genome FASTA(s) in `<genome>/`.
- **Sibling pattern:** mirror the module layout of `bismark-genome-preparation`
  (`cli`/`discovery`/`error`/`lib`/`main` + domain modules) and its Perl-oracle integration-test style.

## 3. Behavior (numbered)

**3.1 Capture argv.** Match Perl exactly: `$command_line = join(" ", @ARGV)` captured at program start
(line 32) — **program name excluded**, and crucially **before** any quality-flag rewrite (the
`--solexa1.3-quals`→`--phred64-quals` rewrite, 36–40). The `@PG` line is then `CL:"bismark $command_line"`
(emitted Phase 5, 8480). Store this pre-rewrite, space-joined argv string verbatim in `RunConfig`.

**3.2 Parse options (clap).** Declare the full Bismark option surface (the 60 options in the `GetOptions`
block, 7317–7382) so help text and argv fidelity are complete. **Wire** only the v1-spine subset (below);
options exclusive to later phases are parsed and stored but may be rejected at the point of use.

**3.3 Resolve aligner** (7414–7514). Default = Bowtie2. `--hisat2`/`--minimap2` → **error**: "HISAT2/
minimap2 are deferred to a v1.x follow-up; use Perl Bismark or `--bowtie2`." Replicate the mutual-
exclusivity `die`s (can't combine `--bowtie2`+`--hisat2`, etc.) functionally.

**3.4 Resolve library type.** directional (default) / `--non_directional` / `--pbat` (mutually exclusive).
Store the mode; v1 wires *directional* downstream, but parse all three now.

**3.5 Resolve read layout + format.** `-1` & `-2` → paired-end; `--se/--single_end` or positional singles
→ single-end. `-q/--fastq` (default) / `-f/--fasta`. Store layout; no alignment runs, so PE vs SE both
simply resolve here. Match Perl's filename handling (8020–8120):
- `--single_end`: normalize separators `:`→`,` (8080); positional singles: `join(",",@ARGV)` then
  whitespace→`,` (8083–87).
- **Validate every input file exists** — die "Supplied filename '…' does not exist" if not (8094–8119).
- Conflicts/sanity: `--single_end` + `-1/-2` → die (8022); `-1`/`-2` mate counts must match (8029);
  `-1` file == `-2` file → die (8037); `-2` without `-1` → die (8067).

**3.6 Genome folder + index discovery** (7620–7800):
- Make `genome_folder` absolute (canonicalize; ensure trailing separator semantics match Perl's
  `chdir`+`getcwd`). Error clearly if it doesn't exist / isn't a dir.
- `CT_dir = <abs>/Bisulfite_Genome/CT_conversion/`, `GA_dir = <abs>/Bisulfite_Genome/GA_conversion/`.
- Bowtie2 small-index presence check: `BS_CT.{1,2,3,4,rev.1,rev.2}.bt2` (+ `BS_GA.*`). If any missing,
  check the **large** index (`.bt2l`) before failing — emit the Perl message ("…seems to be faulty or
  non-existant… run bismark_genome_preparation"). Record whether small or large (`bt2_large_index`).
- `CT_index_basename = <CT_dir>BS_CT`, `GA_index_basename = <GA_dir>BS_GA` (7796–97).
- Locate the raw genome FASTA(s) — **priority fallback, NOT a union** (`read_genome_into_memory`,
  5031–50): glob `*.fa`; **only if empty** glob `*.fa.gz`; else `*.fasta`; else `*.fasta.gz`. The chosen
  category's **glob order is byte-significant** — it sets the `@SQ` header order in the BAM (the epic's
  documented macOS glob-case-fold landmine: Perl's bundled `File::Glob` sorts case-insensitively on all
  platforms; adjudicate on Linux/oxy, never macOS). Record the resolved paths + category now (don't load;
  loading + `@SQ` ordering is Phase 5).

**3.7 Aligner detection** (`ensure_the_aligner_is_working`, 7060–7092; path setup 7480-ish):
- If `--path_to_bowtie2 <dir>` given: require a directory, append `bowtie2`. Else assume `bowtie2` in
  PATH (use `which`).
- Run `bowtie2 --version`; on non-zero exit → die with the Perl-equivalent message. Parse the version
  triple from `bowtie.*version (\d+\.\d+\.\d+)`. Store it (used in reports later).
- **Version pin:** record the version; if `!= 2.5.5`, emit a *warning* (not an error) noting byte-identity
  is only guaranteed against the pinned 2.5.5. (CI pins 2.5.5; users aren't hard-blocked.)

**3.8 Assemble `aligner_options`** — **byte-identity-critical; EXACT push order, verified against Perl
7838–8142** (rev 1: corrected per dual plan-review — the order below is authoritative):
1. `-q` (fastq) / `-f` (fasta) — format flag (pushed in the format section, before 7838).
2. `--phred33` (7845) / `--phred64` (7853) — **only if** set; **each requires `-q`** (fastq) or Perl
   dies (7842/7850). (mutually exclusive, 7838.)
3. `-N <0|1>` — if `-n/--seedmms` defined; die unless value is 0 or 1 (7862–68).
4. `-L <int>` — if `-l/--seedlen` defined (7872).
5. `-D <int>` — if `-D` defined (Bowtie2-only; die otherwise, 7880–83).
6. `-R <int>` — if `-R` defined (Bowtie2-only, 7886–89).
7. `--score-min L,i,s` — **always**; default end-to-end `L,0,-0.2`. If `--score_min` given, validate
   **shape-only** `^L,(.+),(.+)$` (content is permissive — do NOT over-validate numerics) and substitute
   (7895–7954). **`--local` (G-form) is REJECTED in v1** — off the byte-identity spine (not half-wired).
8. `--rdg i,j` — only if `--rdg` given (validate `^\d+,\d+$`, 7960–68).
9. `--rfg i,j` — only if `--rfg` given (7976–84). [The 5,3 defaults are internal scalars for MAPQ, NOT
   added to the string.]
10. `-p <n>` then `--reorder` — only if Bowtie2 `-p`/`--parallel ≥ 2` (7993–99). Default: absent →
    single-threaded, deterministic (Phase-0 premise). [`--multicore` file-level is a *different* option →
    Phase 9.]
11. `--ignore-quals` — **always** (8012). **NOT last** — the PE/insert/quiet flags below follow it.
12. PE only (Phase 7): `--no-mixed`, `--no-discordant`, then `--dovetail` unless `--no_dovetail`
    (8044–56).
13. `--minins <n>` — if `-I` given (PE-only; die if SE, 8123–25).
14. `--maxins <n>` — if `-X` given, else `--maxins 500` for PE (SE → nothing) (8129–37).
15. `--quiet` — if `--quiet` set (8140–41).
- Join with single spaces. The **default SE** result = `-q --score-min L,0,-0.2 --ignore-quals`
  (Phase-0-verified) — but that bare case does NOT exercise ordering, so add an explicit order test with
  several flags set (e.g. `-n 1 -L 20 --quiet`) → `-q --phred33? -N 1 -L 20 --score-min L,0,-0.2
  --ignore-quals --quiet`.

**3.9 Output-target resolution.** BAM is the default (`--sam`/`--cram` override → v1 supports BAM; SAM/CRAM
parse but error "not yet supported" — see §10-Q2). Resolve `-o/--output_dir`, `-B/--basename`, `--prefix`,
`--gzip`. **Output-name rule:** if `--basename` is given it **fully overrides** → `<basename>.bam` (no
`_bismark_bt2` suffix, 1421); otherwise the derived name appends `_bismark_bt2.bam` to the read basename.
`--output_dir` and `--temp_dir` each default to the **empty string `''`** (parent/CWD-relative), set
**independently** (8201/8231).

**3.10 Build `RunConfig`** holding all resolved + derived state and print a readable summary, then exit 0.

### Edge cases
- Genome folder missing / no `Bisulfite_Genome/` / partial index → distinct, Perl-aligned errors.
- Both `-1/-2` and `--se` given → error. Neither + no positional → usage error.
- `--hisat2`/`--minimap2`/(SAM/CRAM if we defer) → explicit "deferred" errors, not silent acceptance.
- `--score_min` with wrong functional form → die with the Perl message.
- `bowtie2` absent from PATH and no `--path_to_bowtie2` → the detection error.
- Empty argv / `--help` / `--version` → help/version text (functionally equivalent; not byte-gated).

## 4. Signature (key types)

```rust
pub struct RunConfig {
    pub argv: Vec<String>,            // verbatim, for @PG CL (Phase 5)
    pub aligner: Aligner,             // Bowtie2 (only wired); Hisat2/Minimap2 -> error
    pub library: LibraryType,         // Directional | NonDirectional | Pbat
    pub layout: ReadLayout,           // SingleEnd { reads } | PairedEnd { r1, r2 }
    pub format: ReadFormat,           // FastQ | FastA
    pub genome_dir: PathBuf,          // absolute, trailing-sep semantics matched
    pub ct_index_basename: PathBuf,   // <CT_dir>BS_CT
    pub ga_index_basename: PathBuf,   // <GA_dir>BS_GA
    pub large_index: bool,            // .bt2l vs .bt2
    pub genome_fastas: Vec<PathBuf>,  // raw FASTA(s) for Phase 5
    pub aligner_path: PathBuf,        // resolved bowtie2 binary
    pub aligner_version: String,      // parsed x.y.z
    pub aligner_options: String,      // exact, ordered string
    pub gap_penalties: GapPenalties,  // rdg/rfg internal scalars (5,3 default) for MAPQ
    pub output: OutputTarget,         // dir, basename, prefix, bam/sam/cram, gzip, temp_dir
    // ... skip/upto, phred, ins/maxins, unmapped/ambiguous/ambig_bam, rg_*, etc.
}
pub fn parse_and_resolve(args: impl IntoIterator<Item = String>) -> Result<RunConfig, BismarkError>;
```

## 5. Implementation outline

1. Add `bismark-aligner` to `rust/Cargo.toml` members; create `Cargo.toml` (deps above) + `src/{main,lib}.rs`.
2. `error.rs`: `BismarkError` (thiserror) with variants for genome/index/aligner/option errors.
3. `cli.rs`: clap derive struct covering all 60 options (canonical names + the documented aliases, incl.
   `--genome` ⇒ genome folder); raw-argv capture; the mode-resolution + mutual-exclusivity logic (3.3–3.5).
4. `discovery.rs`: genome canonicalization, `CT_dir`/`GA_dir`, small/large index presence checks, basenames,
   raw-FASTA discovery (3.6).
5. `aligner.rs`: bowtie2 path resolution (`which`/`--path_to_bowtie2`) + `--version` exec + parse +
   pin-warn (3.7), mirroring `ensure_the_aligner_is_working`.
6. `options.rs`: ordered `aligner_options` assembly + validators for `--score_min`/`--rdg`/`--rfg` (3.8).
7. `config.rs`: `RunConfig` + `OutputTarget` derivation (3.9), the dry-run summary printer.
8. `lib.rs`: `parse_and_resolve()` orchestrating 3.1–3.10; `main.rs`: call it, print summary, exit.
9. Tests (see §9), incl. a Perl-oracle option-string comparison where feasible.

## 6. Efficiency

Trivial — argument parsing + a handful of `stat`s + one `bowtie2 --version` subprocess. No genome load
(deferred to Phase 5). No hot paths.

## 7. Integration

- **Produces** `RunConfig`, the single typed input to Phases 2–10 (read conversion reads `format`/`layout`/
  `library`; alignment reads `aligner_path`/`aligner_options`/`*_index_basename`; output uses `OutputTarget`;
  `@PG` uses `argv` + `aligner_version`).
- **Reads** the filesystem (genome dir, index files, FASTA paths) and execs `bowtie2 --version`. Writes
  nothing in this phase.
- Order: first stage of the pipeline; nothing precedes it except Phase 0's (validated) premise.

## 8. Assumptions

**From epic (shared):** Perl v0.25.1 oracle; Bowtie2 **2.5.5** pinned; gate = byte-identical *decompressed*
SAM content (noodles ≠ samtools BGZF); output fully Bismark-generated; `@PG` reconstructed from argv (the
samtools-pipe `@PG` policy is still pending — does not affect Phase 1); inputs = genome-prep's `BS_CT`/
`BS_GA` + raw FASTA; byte-identity adjudicated on Linux CI/oxy not macOS; crate `bismark-aligner`/binary
`bismark_rs`; do not name external *bisulfite* aligners in committed artifacts.

**Phase-specific:**
- The full option *surface* is parsed; only the **v1 spine** (Bowtie2 + FastQ + directional + SE) is wired
  downstream. HISAT2/minimap2 → hard error (v1.x). PE/non-dir/pbat/FastA are parsed + stored, wired in
  their later phases.
- Default Bowtie2 `aligner_options` == `-q --score-min L,0,-0.2 --ignore-quals` (Phase-0 verified) — this is
  a fixed assertion target.
- Help/`--version` text is *functionally* equivalent, **not** byte-gated (only the BAM is gated).
- Perl `GetOptions` auto-abbreviation (e.g. `--geno`) is **not** fully replicated; we alias only documented
  short/long forms (incl. `--genome`). *(Open — see §10.)*
- Output default = BAM. `--output_dir` and `--temp_dir` each default to the empty string `''`
  (parent/CWD-relative), set independently (verified, 8201/8231).

## 9. Validation

| # | Verify | How | Expected |
|---|--------|-----|----------|
| 1 | Default options string | unit test on `options.rs` with no overrides | `-q --score-min L,0,-0.2 --ignore-quals` |
| 2 | `--score_min L,0,-0.4` override | unit test | `--score-min L,0,-0.4` substituted in place |
| 3 | Index basename derivation | integration test on a fixture genome dir | `<abs>/Bisulfite_Genome/CT_conversion/BS_CT` (+ GA) |
| 4 | Missing/partial index | fixture lacking `BS_CT.3.bt2` | Perl-aligned "faulty or non-existant" error |
| 5 | bowtie2 missing | `--path_to_bowtie2` to empty dir | detection error mirroring Perl |
| 6 | `--hisat2`/`--minimap2` | CLI invocation | explicit "deferred to v1.x" error, exit ≠ 0 |
| 7 | argv captured verbatim | unit test | `RunConfig.argv` equals input args in order |
| 8 | bowtie2 2.5.5 version parse + pin-warn | run against real bowtie2 2.5.5 (oxy/CI) | version `2.5.5`, no warn; a different version warns |
| 9 | **options ORDER with multiple flags** | unit test `-n 1 -L 20 --quiet` | `-q -N 1 -L 20 --score-min L,0,-0.2 --ignore-quals --quiet` (seed flags before score-min; ignore-quals not last) |
| 10 | `--phred33`/`--phred64` without `-q` (with `-f`) | CLI invocation | die "Phred quality values work only when -q is specified" |
| 11 | `--basename foo` output name | unit test | `foo.bam` (NOT `foo_bismark_bt2.bam`) |
| 12 | FASTA priority-fallback + order | fixture dir with `.fa` + `.fa.gz` | only `.fa` chosen; case-insensitive glob order (verify on Linux) |
| 13 | missing input file / `-1`==`-2` | CLI invocation | Perl-aligned "does not exist" / same-file die |

## 10. Questions or ambiguities

- **(Open)** Perl `GetOptions` abbreviation matching — replicate generally, or alias only documented forms?
  *Assumption taken:* alias documented forms only (incl. `--genome`); revisit if a real invocation needs more.
- **(Open)** Do we wire `--sam`/`--cram` output in v1, or BAM-only (defer SAM/CRAM)? *Assumption:* BAM-only
  for v1; SAM/CRAM parse but error "not yet supported" until a later phase. (Confirm — low risk.)
- **(RESOLVED, dual review)** `--temp_dir` and `--output_dir` both default to the **empty string `''`**
  (parent/CWD-relative), set **independently** of each other (8201/8231) — not "the output dir."
- **(RESOLVED, dual review)** `--local` (Bowtie2 local mode, score-min `G`-form) is **rejected in v1** —
  off the byte-identity spine; do not half-wire it.

## 11. Self-Review

- **Efficiency:** nothing hot; one subprocess. ✓
- **Logic:** mode resolution mirrors Perl's precedence (aligner → library → layout → format → discovery →
  detection → options → output). `aligner_options` order matches 7838–8142 (corrected rev 1). ✓
- **Edge cases:** missing genome/index, conflicting modes, bad `--score_min`, absent aligner, empty argv,
  large (`.bt2l`) index — all covered in §3 edge cases + §9. ✓
- **Integration:** `RunConfig` is the clean seam to Phases 2–10; argv stored for `@PG`; FASTA paths recorded
  (not loaded) so Phase 5 can `read_genome_into_memory`. ✓
- **Adjusted during review:** scoped option *wiring* to the v1 spine while parsing the *full* surface (keeps
  the phase bounded yet `@PG`/help-faithful); made the version pin a warning, not a hard fail (don't block
  users on non-2.5.5 while CI pins it); explicitly excluded help/version text from the byte gate.
- **Remaining risks:** the v1-wired vs parsed-but-deferred boundary must be enforced consistently so the
  binary never silently half-supports a later-phase mode (mitigated by the explicit "deferred" errors).

## 12. Revision History

- **rev 1 (2026-06-01)** — folded in dual plan-review (`PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md`), all
  source-verified against `bismark` 7838–8142 / 5031–50 / 1421:
  - **§3.8 `aligner_options` order corrected** (was Critical in both reviews): seed flags `-N/-L/-D/-R`
    come **before** `--score-min`; `--ignore-quals` is **not last** (PE flags / `--minins` / `--maxins
    500` / `--quiet` follow); `--phred33/--phred64` added (require `-q`). Added an ordering test (§9 #9).
  - **§3.1** argv capture pinned to Perl semantics (file-scope `join(" ",@ARGV)`, program-name excluded,
    pre-quality-rewrite).
  - **§3.6** FASTA discovery corrected to priority-fallback (`.fa`→`.fa.gz`→`.fasta`→`.fasta.gz`) with the
    byte-significant `@SQ` glob-order / macOS-fold caveat.
  - **§3.5** added PE/SE input-file existence validation + `--se` separator normalization + mate/conflict dies.
  - **§3.9** `--basename` fully overrides (`<basename>.bam`, no `_bismark_bt2`); `--output_dir`/`--temp_dir`
    default `''` independently.
  - **§10** closed the `--temp_dir` question (default `''`); `--local` now rejected in v1 (was half-wired).
  - **§9** added validations #9–#13 (ordering, phred-without-q, basename, FASTA fallback, missing input).
- **rev 0 (2026-05-31)** — initial plan.

## 13. Implementation Notes (2026-06-01)

**Status: IMPLEMENTED & verified (15 unit + 9 integration tests green; clippy `-D warnings` clean; fmt clean).**

Crate `rust/bismark-aligner` (bin `bismark_rs`, v1.0.0-alpha.1), added to the workspace. Modules:
- `cli.rs` — full clap surface (all ~60 GetOptions flags; v1-spine wired, deferred flags parsed).
- `discovery.rs` — genome canonicalization, small/large index presence, FASTA priority-fallback
  (case-insensitive order). 4 unit tests.
- `aligner.rs` — bowtie2 path resolution + `--version` exec + triple parse + 2.5.5 pin-warn. 2 unit tests.
- `options.rs` — the corrected 15-position `aligner_options` assembly + validators. 9 unit tests
  (incl. the ordering test that locks `-N`/`-L` before `--score-min` and `--quiet` after `--ignore-quals`).
- `config.rs` — domain enums, `RunConfig`, `resolve()` orchestration, `summary()`.
- `error.rs` / `lib.rs` / `main.rs` (exit 0/1, clap=2; argv captured before parse for `@PG`).
- `tests/cli.rs` — 9 integration tests using a fake `bowtie2` (reports `version 2.5.5`) for hermetic
  happy-path coverage (no real Bowtie 2 needed).

**Deviations from the plan (documented):**
- **mimalloc omitted** in Phase 1 (the plan §2 listed it). Rationale: Phase 1 has no hot path (matches the
  sibling `bismark-genome-preparation`, which also omits it); add it in the threading/perf phase (Phase 9)
  where it matters. Output-neutral either way.
- **Flag clarity:** Bismark input flags `-n`/`-l` map to Bowtie 2 output flags `-N`/`-L` (and `-D`/`-R`
  pass through). The CLI uses the Bismark spellings; the options string emits the Bowtie 2 spellings.

**Carried-forward open items (later phases, not Phase 1 blockers):** the samtools-pipe `@PG` policy
(Phase 5 gate); Perl `GetOptions` auto-abbreviation not replicated (documented assumption); `--prefix`
exact output-name interaction with `--basename` to be confirmed against Perl when output naming is wired
(Phase 5/6).

### Post-review fix pass (2026-06-01)

Dual code-review (`CODE_REVIEW_A.md` / `CODE_REVIEW_B.md`) + plan-manager (`COVERAGE.md`, Verdict
**COMPLETE**) ran after implementation. plan-manager found full coverage; the code reviewers found items
the plan under-specified. Felix authorised the recommended fixes — all applied + tested:

- **FASTA discovery now mirrors `bismark-genome-preparation::discovery` exactly** (the cross-port `@SQ`
  contract): extension **match is case-SENSITIVE on raw bytes** (`name.ends_with(b".fa")`, `.gz` siblings
  excluded), filtering on `path.is_file()` (**follows symlinks**), and matching on `as_encoded_bytes()`
  (**non-UTF-8 safe** — fixes the `to_str()` drop). Sort stays case-insensitive (`fasta_name_cmp`). Added
  a prominent lockstep comment + a "promote to a shared crate" follow-up note.
- **`--pbat` constraints** (Perl 8155–56): `--pbat`+`--gzip` and `--pbat`+`-f` now die; the
  `--non_directional`+`--pbat` message matches Perl 8149.
- **`--multicore`/`--parallel` value validation** (Perl 8244): `< 1` dies.
- **Deferred-flag de-silencing:** provided-but-unwired flags (`--skip`, `--upto`, `--unmapped`,
  `--ambiguous`, `--ambig_bam`, `--nucleotide_coverage`, `--rg_tag`, `--slam`, `--non_bs_mm`,
  `--multicore`, `--gzip`, `--prefix`, `--basename`, `--old_flag`, `--sam-no-hd`) now emit a "recognised
  but not yet active" notice instead of being silently ignored.
- **Created the missing `README.md`** (`Cargo.toml` referenced it).
- **Tests added:** PE-layout dies (mate-count mismatch, `-1`==`-2`, SE/PE conflict, `-2`-without-`-1`),
  `--multicore 0`, deferred-flag notice, FASTA case-sensitivity, symlink-follow, byte-matcher non-UTF-8
  (+ a Linux-only real-fs non-UTF-8 test). **Totals: 18 unit + 15 integration (34 on Linux CI).**

**Deliberately deferred (low priority, not in the approved fix set):** integer-width parity with Perl's
signed `=i` (clap cleanly rejects nonsensical negatives — not a byte-identity risk for valid input);
detect-before-discover error-precedence reorder (only changes which error fires when *both* are broken;
no effect on successful-run byte-identity). Both noted for revisit if they ever matter.
