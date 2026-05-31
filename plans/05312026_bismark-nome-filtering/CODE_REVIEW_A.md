# Code Review A — `bismark-nome-filtering` Phase A (scaffold + genome reader)

**Reviewer:** A (independent, fresh context). **Mode:** RECOMMEND-ONLY — no source modified.
**Date:** 2026-05-31. **Branch/worktree:** `rust/nome-filtering` @ `~/Github/Bismark-nome`.

**Scope reviewed:** `rust/bismark-io/src/genome.rs` (NEW), `rust/bismark-io/{Cargo.toml,src/lib.rs}`,
`rust/bismark-nome-filtering/**` (`Cargo.toml`, `src/{lib,main,error,cli,filename,substr}.rs`,
`tests/cli_phase_a.rs`), `rust/Cargo.toml`. Against SPEC rev 1, IMPL_phase-A, and Perl `NOMe_filtering` v0.25.1.

---

## Summary

**Verdict: Phase A is correct and faithful. No Critical or High issues.** The scaffold is honest
about its scope (validate + dir + path-resolve + infile-exists + genome load, no output file), the
genome promotion is a near-verbatim copy of the c2c twin with exactly the three documented changes
(tier-parameterization, module-local `GenomeError`, doc note), `BismarkIoError` is genuinely
untouched, the no-version-bump promotion is real, and every §9/§4/§P15/§P16 byte-identity hazard the
SPEC names is handled. I independently confirmed:

- `cargo test -p bismark-io genome` → 13/13 green; `cargo test -p bismark-nome-filtering` → 26 unit + 6 integration + 1 doctest green.
- `cargo clippy -p bismark-io -p bismark-nome-filtering --all-targets -- -D warnings` → clean.
- `cargo build --workspace` → builds; `bismark-io/Cargo.toml` still `version = "1.0.0-beta.8"` (P7 satisfied).
- `--version` → `NOMe_filtering_rs 0.1.0-beta.1 (macos/aarch64)`; `--help` exits 0.
- `BismarkIoError` (`bismark-io/src/error.rs`) has **no** genome variants — promotion is additive (P16 satisfied).

Findings below are all Medium/Low — mostly Phase-B-readiness flags and one documentation gap about a
genuine Perl quirk the port (correctly, per SPEC) diverges from. Nothing blocks Phase A.

---

## Issues by area

### Logic / correctness

**A-L1 (Medium, Phase-B readiness — documentation gap). Perl's infile `-e` check runs at the ORIGINAL CWD, not under `--dir`; the port checks under `--dir`. This divergence is real but undocumented.**
Perl `process_commandline:452-455` does `-e $coverage_infile` on the **bare filename at the original
CWD** — *before* any `chdir` (the genome `chdir` is `:519`, the output-dir `chdir` is `:58-61`, both
later). The real input open happens at `:66-70` relative to `--dir`. So Perl checks existence in one
place and opens in another. The Rust port (`lib.rs:58` `cfg.input_path.exists()` where
`input_path = dir.join(infile)`, `cli.rs:117`) checks existence **under `--dir`** — which is what the
*open* uses, and is the sane behavior. This is the right call and matches SPEC §4's "resolve input =
`dir.join(infile)`", but the SPEC §4 path-contract note only discusses the *open* locations
(`:58-77`); it never mentions that Perl's `-e` (`:453`) resolves at the original CWD. Consequently:
- If a caller runs from a CWD where the bare infile does *not* exist but it *does* exist under `--dir`,
  Perl dies at `:453` ("File did not exist…") while the Rust port proceeds. The extractor always invokes
  with the file living under `--dir`, and typically with CWD == something where the relative name may or
  may not resolve — so this is an error-path-only divergence, not a happy-path output divergence.
- **Recommendation:** Add one sentence to SPEC §4 (and the `cli.rs`/`lib.rs` doc comment) noting that the
  port deliberately resolves the `-e` check under `--dir` (unlike Perl's original-CWD `-e`), because the
  port has no real `chdir`. This is the *correct* unification, but it should be an explicit, recorded
  deviation rather than an implicit one. Error messages aren't byte-gated (§2), so there is no
  byte-identity risk on the gated output — this is purely a "make the deviation explicit" item.

**A-L2 (Low, ordering — no observable effect in Phase A, flag for Phase B/C). `validate()` checks `merge_CpGs+CX` before `MissingGenomeFolder` before `InfileNotFound`; Perl's die order is the reverse.**
Perl order of dies in `process_commandline`: (1) no-args → help+exit `:444`, (2) **infile `-e`** `:453`,
(3) **genome mandatory** `:488-494`, (4) **merge+CX** `:497-500`. The Rust `validate()` (`cli.rs:103-110`)
checks **merge+CX first, then genome, then infile**. Because none of these messages are byte-gated (§2,
STDERR only) and each path still exits non-zero, this never affects the gated artifact. But if two
error conditions co-occur (e.g. `--merge_CpGs --CX` *and* a missing genome), Perl reports the genome
error first whereas the port reports merge+CX. **Recommendation:** harmless for byte-identity; if you
want exact `stderr` parity for a future user-facing parity test, reorder to infile → genome → merge+CX.
Otherwise leave as-is and note it. (Not a defect — flagging for completeness.)

**A-L3 (Low, verified-correct — no action). `perl_substr` `isize` cast of `s.len()`.**
`substr.rs:20` does `let l = s.len() as isize`. On 64-bit this is safe for any genome (the reader's
`check_chr_len` caps sequences at `u32::MAX` ≈ 4.29e9, far below `isize::MAX` ≈ 9.2e18). On a 32-bit
target a `u32::MAX`-length chromosome (~4GB) would exceed `isize::MAX` (~2.1GB) and the cast would wrap
negative — but 32-bit isn't a supported target and the helper is only ever fed genome-window slices
(`ext_seq`, ≤ read-length + 4, tiny) in Phase B, never a whole chromosome. **No action**; mentioning so
the next reviewer doesn't re-flag it. The early-return guards (`start < 0 || start > l`) plus
`saturating_add(len).min(s.len())` make every `&s[start..end]` index in-bounds — confirmed no panic
path, matching SPEC §9 / P1 (the `start == L → &s[L..L]` empty-slice case is unit-tested at
`substr.rs:57`).

### Efficiency

**A-E1 (Low — no action). `discover_fasta_files` reads the whole dir once, then filters per tier.**
`genome.rs:171-195` collects all entries once and re-scans the in-memory `Vec` per tier — correct and
cheap (genome dirs hold a handful of files). Identical to the c2c twin. The per-tier `tier_files.sort()`
is `O(k log k)` on a tiny set; the comment correctly notes order is irrelevant to output (no public
insertion-order accessor). No change.

**A-E2 (Low — no action). `cli.rs:121-129` `let _ = (…inert tuple…)`.**
Binding the inert flags to `_` to document intent is idiomatic and zero-cost. Fine.

### Error handling

**A-Err1 (Low, intent-confirmed — no action). `EmptyInput` declared but never raised in Phase A.**
`error.rs:30-34` declares `EmptyInput` with the exact Perl message (`:174`). It is correctly NOT raised
anywhere in Phase A — the SPEC coverage table (IMPL row 11 + the note under it) and D4 both document
that it is raised in **Phase B** *after* the header write. This is the documented intent, not a dropped
requirement. The message text matches Perl `:174` verbatim including the "(e.g. was the input file
empty?)" parenthetical. Good.

**A-Err2 (Low — no action, but note for Phase B). `InfileNotFound` doubles as "no positional infile".**
`cli.rs:110` maps a `None` infile to `InfileNotFound`, and the doc comment (`error.rs:23-25`) plus the
IMPL note (T6) acknowledge this conflates Perl's two distinct paths: Perl prints help + `exit(0)` on
*no args* (`:444-450`) vs `die` on *non-existent file* (`:453-455`). Phase A's mapping is acceptable
because neither stderr/help text nor exit-code-for-no-args is byte-gated (§2), and both yield non-zero.
**Recommendation (Phase B/C):** if a user-facing parity test ever checks the no-args case, route
`infile.is_none()` to a help-and-exit-0 path in `main` before `run` (the IMPL T6 note already flags this
as optional polish). Not required for Phase A.

**A-Err3 (Low — no action). `MissingGenomeFolder` / `InfileNotFound` / `MergeCpgsWithCx` messages match Perl.**
`MissingGenomeFolder` (`error.rs:19`) == Perl `:494`. `InfileNotFound` (`error.rs:24`) == Perl `:454`.
`MergeCpgsWithCx` (`error.rs:38-41`) == Perl `:499`. All faithful (not byte-gated, but nice).

### Structure / idioms

**A-S1 (Low — no action). `genome.rs` is a faithful promotion of the c2c twin.**
Diffed `bismark-io/src/genome.rs` against `bismark-coverage2cytosine/src/genome.rs`. The only deltas are
exactly the three the IMPL/SPEC prescribe: (1) `load(folder, tiers: &[&str])` replaces the hardcoded
`const FASTA_TIERS:[&str;4]` + `load(folder)`; `discover_fasta_files(dir, tiers)` iterates the supplied
tiers (first-non-empty wins, dotfile-exclude unchanged); (2) the error type is a module-local
`GenomeError` (NOT `BismarkC2cError`/`BismarkIoError`) with the same five variants; (3) the module doc
updates the glob-priority note to "tier-parameterized." Uppercase-on-load (`genome.rs:249`), Mus skip
(`:118`), CRLF strip (validated by `mus_skipped_among_others_and_crlf_stripped`, noodles auto-strips the
header `\r`), first-token name (`record.name()`, `:244`), dup-name error (`:122-126`), the `u32` guard
(`check_chr_len`, `:257`), the no-public-insertion-iterator invariant (only `names_sorted()`, `:150`),
and the gz-capability (kept verbatim so it's a true general promotion) are all preserved. The
bare-`>`-header → `MalformedFastaHeader` divergence is inherited and pinned by a test (`:356`). The
`flate2` dep was added to `bismark-io/Cargo.toml:34` additively with `version` unchanged. **Excellent
fidelity.**

**A-S2 (Low — no action). Tier list `&[".fa", ".fasta"]` correct; `.fa.gz` footgun (P14) preserved.**
`lib.rs:64` passes the two PLAIN tiers — the deliberate Perl-faithful divergence from c2c's four tiers.
The footgun is pinned by `fa_gz_invisible_with_two_plain_tiers` (`genome.rs:304`), which I confirmed
green. The promoted reader remains gz-*capable* (the `.fa.gz`-tier test `:399` passes) — crippling was
correctly avoided; the footgun lives in NOMe's *tier list*, not the reader.

**A-S3 (Low — no action). `main.rs` / `lib.rs` Phase-A boundary is honest.**
`run()` (`lib.rs:48-74`) does exactly validate → create-dir → resolve-paths (in `validate`) →
infile-exists → genome-load → stderr line, and **writes no output file** — the Phase-B work is a clearly
labelled comment (`:70-73`). This is a legitimately-scoped milestone, NOT a stub-with-hardcoded-return
that the implementation rules forbid: `run` exercises real CLI validation, real path resolution, and a
real genome load that can fail. `main.rs` maps `Ok → 0`, `Err → 1`, clap-parse → 2 (clap default). The
`--version` short-circuit (`main.rs:24`) precedes `run`, matching Perl `:429`. `version_string()` format
(`lib.rs:30-37`) matches the SPEC (`NOMe_filtering_rs <semver> (<os>/<arch>)`).

**A-S4 (Low — no action). `filename.rs` single-strip is correct; does NOT reuse dedup's loop (P15).**
`derive_manowar_name` (`filename.rs:31-42`) strips exactly one `.gz` then one `.txt` via two independent
`strip_suffix` calls, then appends `.manOwar.txt` + `.gz` — a faithful transcription of Perl
`:466-468` + the force-`.gz` at `:74-76`. It correctly does NOT loop like dedup's
`derive_output_stem` (which iterates `[".gz",".sam",".bam",".txt"]` and *also* strips the leading
directory). The `x.gz.gz`→`x.gz.manOwar.txt.gz` and `x.txt.txt`→`x.txt.manOwar.txt.gz` single-strip
cases are unit-tested (`:69,75`) and green. No leading-directory strip — correct per SPEC §4.

**A-S5 (Low, Phase-B readiness). `run()` structure will accommodate the D4 header-before-loop ordering — confirm the writer is opened before the read loop in Phase B.**
The current `run()` resolves `cfg.output_path` (`cli.rs:118`) but does not open the writer (correct for
Phase A — no output file). For Phase B byte-identity (D4), the writer MUST be opened and the header line
written **before** the read loop, so the empty-input path leaves the header-only `.gz`. The present
`run()` signature returning `Result<(), BismarkNomeError>` and the resolved `output_path` give Phase B
everything it needs; nothing in the Phase-A structure blocks the header-first ordering. **Recommendation:**
when wiring Phase B, ensure `EmptyInput` is returned *after* the header `write_all` + a flush/finish that
guarantees the 61-byte artifact lands on disk (the `GzEncoder` must be `.finish()`-ed on the error path,
not dropped mid-buffer). Flagging now so it isn't missed — no Phase-A change.

**A-S6 (Low — no action). `clap` surface matches §4.** `disable_version_flag = true` (`cli.rs:32`) +
the explicit `-V/--version` bool (`cli.rs:77`) reproduce the custom version string path. Inert flags
(`--zero_based`, `--CX`/`--CX_context` alias, `--GC`/`--GC_context` alias, `--gzip`, `--nome-seq`,
`--merge_CpGs`, `--parent_dir`) are all present and accepted. `--help` exits 0 (verified). Note Perl's
`print_helpfile` exits **1** (`:658`), while clap's `--help` exits **0** — not byte-gated (§2), but a
micro-divergence worth a one-line note if exact help exit-code parity is ever wanted (Low).

### Test quality

**A-T1 (Low — no action). Tests assert real behavior, not tautologies.** The `genome.rs` tests cover
first-token name + uppercase + multi-FASTA join, fa-beats-fasta no-union, the `.fa.gz` footgun,
fasta-tier fallback, Mus-only-empty + Mus-skip-among-others + CRLF, cross-file dup-name, bare-header
divergence, no-FASTA, dotfile-exclude, bytewise `names_sorted`, gz round-trip, and the `u32` guard
helper directly. `substr.rs` has the full §9/§12 adversarial matrix incl. `start==L`→empty/no-panic and
`offset==-L`→start 0. `filename.rs` has the P15 single-strip cases. `cli.rs` covers the die,
mandatory-genome, inert acceptance, the `--dir` path contract (input + output under `--dir`), no-dir→`.`,
`--version`-without-infile, and the `--CX_context` alias. `cli_phase_a.rs` integration covers
`--version`, `--help`, both dies, valid-load-exits-0, and nonexistent-infile. **Good coverage for a
scaffold.** Two small gaps (nice-to-have, not blocking):
- No test pins that `run()` writes **no** output file in Phase A (the SPEC/IMPL emphasize "no output").
  A one-liner asserting `!output_path.exists()` after the valid invocation would lock the Phase-A
  boundary so a Phase-B regression can't silently start emitting early. **Recommendation (Low):** add it.
- No test for the explicit `--dir ""` (empty string) case. I verified manually that
  `PathBuf::from("").join("sample.txt") == "sample.txt"` and `"".as_os_str().is_empty() == true`, so the
  empty-`--dir` path (the extractor's documented `--dir ''`) correctly resolves to CWD-relative bare
  filenames and skips `create_dir_all` (`lib.rs:52`). Behavior is correct; a test would pin it. **(Low.)**

---

## Recommendations (prioritized)

**Critical:** none.

**High:** none.

**Medium:**
1. **A-L1** — Record the deliberate deviation that the port resolves the infile-exists (`-e`) check under
   `--dir` (`lib.rs:58` / `cli.rs:117`), whereas Perl's `-e` (`:453`) resolves at the original CWD before
   any chdir. Add one sentence to SPEC §4 and the `cli.rs`/`lib.rs` doc. Error-path only, not byte-gated,
   but should be an *explicit* recorded deviation. (Doc-only; no code change in Phase A.)

**Low:**
2. **A-S5** — Phase-B note: open the writer and write the header *before* the read loop and `.finish()`
   the `GzEncoder` on the `EmptyInput` error path so the header-only `.gz` (D4) actually lands. No
   Phase-A change.
3. **A-T1a** — Add a Phase-A test asserting the valid invocation leaves **no** output file
   (`!output_path.exists()`), locking the scaffold boundary.
4. **A-T1b** — Add a test for `--dir ""` (empty-string) resolving to CWD-relative bare filenames (verified
   correct manually; just pin it).
5. **A-L2 / A-Err2 / A-S6** — Optional stderr/exit-code parity polish (die ordering; no-args → help+exit-0;
   `--help` exit 1 vs clap's 0). All STDERR/help, none byte-gated; leave as-is unless a future parity test
   wants exact stderr.

**No action (verified correct, listed to spare the next reviewer a re-flag):** A-L3 (`isize` cast safe
under the `u32` genome guard), A-E1/E2, A-Err1/Err3, A-S1/S2/S3/S4, A-T1 coverage.

---

## Confirmations against the review checklist

- ✅ `genome.rs` faithful to the c2c twin; tier-parameterization (`load(folder, &[&str])`, first-non-empty,
  no union, dotfile-exclude) correct; module-local `GenomeError` (NOT `BismarkIoError` variants).
- ✅ `BismarkIoError` (`bismark-io/src/error.rs`) **untouched** — no genome variants; P16 satisfied.
- ✅ Uppercase, Mus skip, CRLF strip, first-token name, dup-name, `u32` guard, no public insertion-order
  iterator — all preserved.
- ✅ `flate2` added to `bismark-io` with **no** version bump (`version = "1.0.0-beta.8"` confirmed);
  `cargo build --workspace` resolves all `=beta.8` sibling pins (P7 satisfied).
- ✅ `perl_substr` reproduces §9: negative-from-end, `|offset|>L`→empty, `start==L`→empty/no-panic,
  over-length→truncate; no slice-index panic path; casts sound for genome-bounded inputs.
- ✅ `derive_manowar_name` matches Perl `:464-468`+`:74-76`: single-strip `.gz` then `.txt`, append +
  force `.gz`, no leading-directory strip; `x.gz.gz`/`x.txt.txt` single-strip pinned.
- ✅ `cli.rs validate`: merge+CX die; mandatory-genome; `--dir` path contract (`dir.join(infile)` /
  `dir.join(derived)`, no real chdir); inert-flag acceptance. No partial-move/borrow issues (`self`
  consumed by value, fields moved out cleanly).
- ✅ `error.rs`: `#[from]` wiring for `GenomeError` + `std::io::Error`; `EmptyInput` declared-not-raised is
  documented Phase-B intent (D4), not a dropped requirement.
- ✅ `run()`/`main.rs`: Phase-A scope honest (no output file); `version_string()` format + `ExitCode`
  mapping correct; no forbidden stub-with-hardcoded-return.
- ✅ `#![forbid(unsafe_code)]` + `#![warn(missing_docs)]` on both crates; clippy `-D warnings` clean;
  tests assert real behavior.
- ✅ No Phase-B byte-identity hazard introduced by the Phase-A `run()` structure (header-before-loop D4
  ordering accommodatable — see A-S5).
