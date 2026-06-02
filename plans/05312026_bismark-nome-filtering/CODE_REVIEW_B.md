# Code Review B — `bismark-nome-filtering` Phase A (scaffold + CLI + promoted `bismark_io::genome`)

**Reviewer:** B (independent, fresh context). **Mode:** RECOMMEND-ONLY — no source files were modified.
**Target:** Phase A of the Rust port of Perl `NOMe_filtering` v0.25.1, held to byte-identity.
**Date:** 2026-05-31.

## Verdict

**APPROVE.** Phase A is a faithful, honestly-scoped scaffold. All claimed gates pass on this machine: `cargo build --workspace` resolves with every sibling still pinned at `bismark-io =1.0.0-beta.8` (no version bump leaked into `Cargo.lock`), `cargo test -p bismark-io genome` (13/13) and `cargo test -p bismark-nome-filtering` (26 unit + 6 integration + 1 doctest) are green, and a fresh `cargo clippy -p bismark-io -p bismark-nome-filtering --all-targets -- -D warnings` is clean. The genome promotion is a near-verbatim transcription of the c2c reader with exactly the two SPEC-mandated edits (tier-parameterization + module-local `GenomeError`), `BismarkIoError` is genuinely untouched, and `perl_substr`/`filename`/`cli` reproduce the Perl semantics the SPEC pins.

I found **no Critical or High defects**. The findings below are Medium/Low — mostly divergences that are either out-of-scope-by-design (help/STDERR/exit codes are not byte-gated per SPEC §2) or latent traps that should be pinned by a test or a comment before Phase B builds on top of this. The most useful catch for the team is **M1** (the `output_dir.join(infile)` absolute-path footgun, untested) and **M2** (a structural gap that could let Phase B violate the D4 header-before-loop ordering if `run()` is extended naively).

---

## Issues by area

### 1. `bismark_io::genome` promotion (`rust/bismark-io/src/genome.rs`)

**Faithful to the c2c twin.** A line-by-line diff against `rust/bismark-coverage2cytosine/src/genome.rs` confirms the body is identical except for the two intended changes:
- `discover_fasta_files(dir, tiers)` now takes `tiers: &[&str]` and iterates the supplied list instead of the `const FASTA_TIERS: [&str;4]` (genome.rs:170–200). First-non-empty-tier-wins, no union, dotfile-exclude (`!n.starts_with('.')`, :185) — all preserved verbatim. ✔
- Errors are a module-local `GenomeError` (:44–83) rather than `BismarkC2cError`. ✔
- Uppercase-on-load (:249), Mus skip inside the loop (:118), CRLF strip (delegated to noodles + verified by `mus_skipped_among_others_and_crlf_stripped`), first-token name (`record.name()`, :244), cross-file dup-name error (:122–126), `u32` guard (:257–265), no public insertion-order iterator (only `names_sorted`, :150) — all carried over. ✔

**`BismarkIoError` is untouched** (`rust/bismark-io/src/error.rs`): no new variants; its pre-existing `DuplicateChromosomeName` is the CRAM-reconstitution variant and is unrelated to `genome::GenomeError::DuplicateChromosomeName`. The "additive, no version bump" promotion (SPEC P7/P16) is satisfied — `Cargo.lock` shows `bismark-io` still `1.0.0-beta.8`. ✔

**`flate2` added without a version bump** is safe: `bismark-io`'s `Cargo.toml` already carried the `noodles-fasta`/`noodles-bgzf` deps the reader needs; adding `flate2 = "=1.1.9"` (matching c2c's pin) is purely additive, and the comment (:31–34) documents it. ✔

**`.fa.gz` footgun (P14)** is correctly preserved by the *tier list passed at the call site* (`run()` passes `&[".fa", ".fasta"]`, lib.rs:64), not by crippling the reader — which remains gz-capable and is exercised by `loads_plain_gzip_fa_gz_when_gz_tier_supplied`. The `fa_gz_invisible_with_two_plain_tiers` test pins the footgun. ✔ Good design: the reader is a true general promotion.

- **[Low] L1 — `read_dir` ordering relies on the in-loop sort, not the OS.** `discover_fasta_files` collects `read_dir` entries (non-deterministic OS order) then `tier_files.sort()` (:192) before returning. Because the genome `HashMap` has no public insertion-order accessor and dup-names hard-error, file order cannot reach output — so this is correct. Noting only because the c2c comment said "Order is irrelevant to output (D1)" and the promoted comment generalized it to "no public insertion-order accessor" (:190–191), which is the right justification. No action.

- **[Low] L2 — `MalformedFastaHeader` swallows a genuinely-empty file the same as a nameless header.** `read_one_fasta` maps a zero-record file to `MalformedFastaHeader` (:220–224), and noodles `InvalidData` → same variant (:236–239). This matches the c2c twin and the SPEC's documented bare-`>` divergence (P-note in §7), and `bare_or_nameless_header_errors` pins it. The c2c twin additionally had `empty_file_in_winning_tier_errors` and `headerless_file_errors` tests that were **not** carried into the promoted module's test set. Not a correctness gap (the code path is identical), but the promoted module's test coverage is slightly thinner than its source. Consider porting those two cases for parity. (Low.)

### 2. `substr.rs` `perl_substr` (`rust/bismark-nome-filtering/src/substr.rs`)

Reproduces Perl `substr` rvalue semantics exactly (SPEC §9). I re-derived each branch:
- `start = offset>=0 ? offset : L+offset` (:21). ✔
- `start < 0 || start > L` → `&[]` (:22–24): covers `|offset|>L` (negative underflow) **and** positive `offset>L`. ✔
- `start == L` → `&s[L..L]` = `&[]`, no panic (:25–27). The `end = start.saturating_add(len).min(s.len())` clamps `end` to `L`, and `start==L ⇒ end==L`, so `&s[L..L]` is the valid empty slice — **no index panic** (Rust permits `&s[len..len]`). ✔ This is the exact reverse-edge degenerate boundary (P1).
- over-length → `min(len, L-start)` via the `.min(s.len())` clamp (:26). ✔

**Cast soundness for pathological inputs:** `let l = s.len() as isize` (:20). On a 64-bit target a chromosome up to `u32::MAX` bytes (the genome guard ceiling) is ~4.3e9, far below `isize::MAX` (~9.2e18), so `s.len() as isize` cannot overflow to negative for any genome the reader will accept. `l + offset` for the negative-offset path: `offset` originates from `pos ± k` where `pos` is a 1-based index into `seq` (≤ chr length) — bounded, no overflow. **No soundness issue.** (Phase B must keep feeding it `isize` offsets that stay within these bounds; it will, because offsets derive from genomic coordinates ≤ chr length.) ✔

Tests are adversarial and complete vs the SPEC §12 list (negative-in-range, `|offset|>L`, `start==L`/no-panic, over-length, offset-past-end, zero-len, `offset==-L`→start 0). ✔ No issue.

### 3. `filename.rs` `derive_manowar_name` (`rust/bismark-nome-filtering/src/filename.rs`)

Matches Perl `:464-468` + `:74-76`: one `.gz` strip (`strip_suffix`, :33), then one `.txt` strip (:36), append `.manOwar.txt` (:39), force `.gz` (:40). The two-statement `strip_suffix` form is **single-strip-per-extension** (not dedup's loop) — `x.gz.gz`→`x.gz.manOwar.txt.gz`, `x.txt.txt`→`x.txt.manOwar.txt.gz` both pinned (:69–78). ✔ No directory strip (matches Perl). ✔ The doctest documents the chain. **No issue** — this is exactly right per P15.

- **[Low] L3 — order-sensitivity of the two strips is correct but worth a one-line note.** Perl strips `.gz` *then* `.txt` (in that source order). The Rust mirrors this (`.gz` first at :33, `.txt` at :36). A hypothetical `x.txt.gz` → strip `.gz` → `x.txt` → strip `.txt` → `x` → `x.manOwar.txt.gz`, which the `txt_gz` test confirms. Correct. Only flagging that reversing the two `if` blocks would silently diverge on `x.txt.gz` (would yield `x.txt.manOwar.txt.gz`); a `// order matters: .gz before .txt (Perl source order)` comment would harden it against a future "tidy-up". (Low.)

### 4. `cli.rs` `validate` (`rust/bismark-nome-filtering/src/cli.rs`)

- `--merge_CpGs` + `--CX` die: checked first (:103–105), correct error. ✔ The `cx_context_alias_parses` test confirms `--CX_context` (visible_alias) also triggers it. ✔
- Mandatory genome: `ok_or(MissingGenomeFolder)` (:107–109). ✔
- `--dir` contract: `input_path = output_dir.join(&infile)` (:117), `output_path = output_dir.join(derive_manowar_name(&infile_str))` (:118), no real chdir. ✔ Pinned by `dir_path_contract_resolves_input_and_output_under_dir`.
- Inert-flag acceptance: all parsed, `let _ = (...)` no-op (:121–129). ✔
- Partial-move soundness: `validate(self)` consumes `self`; `self.infile` and `self.genome_folder` are moved out via `ok_or` on the owned `Option`, and the inert tuple reads the remaining `Copy`/owned fields — no borrow-after-move. Compiles clean. ✔

- **[Medium] M1 — `output_dir.join(infile)` silently discards `--dir` when `infile` is an absolute path; this is untested and undocumented in code.** Rust's `Path::join` *replaces* the base when the argument is absolute (I verified: `PathBuf::from("/out").join("/abs/sample.txt") == "/abs/sample.txt"`). So an absolute positional infile makes `input_path` ignore `--dir` entirely, and `output_path` becomes `output_dir.join("/abs/sample.manOwar.txt.gz")` = `/abs/sample.manOwar.txt.gz` (the `derive_manowar_name` keeps the full absolute string since it does no directory strip). The net effect (read and write both at the absolute location, `--dir` ignored) is *arguably* Perl-consistent — Perl `chdir $output_dir` then `open` of an absolute path also ignores the chdir — so this is probably not a behavior bug. **But** the SPEC §4 contract is written entirely around bare filenames, the filename.rs doc explicitly says "a path-qualified infile is untested," and there is no test or code comment capturing the absolute-path join semantics. Recommend either (a) a one-line comment at cli.rs:117 noting "join() drops --dir for an absolute infile — Perl-consistent (chdir is likewise ignored for absolute opens)", or (b) a unit test pinning `parse(&["-g","/g","--dir","/out","/abs/sample.txt"])` → `input_path == /abs/sample.txt`. Without one of these, a future refactor that "fixes" the absolute case (e.g. by force-prepending `--dir`) would silently diverge from Perl with no failing test. (Medium — documentation/test gap, not a live bug.)

- **[Low] L4 — `infile_str` is computed via `to_string_lossy().into_owned()` while `input_path` uses the raw `&infile`.** cli.rs:116–117 derive the *output* name from a lossy-UTF-8 rendering of the infile but resolve the *input* path from the raw `OsStr`. For any non-UTF-8 infile these two would describe different basenames. Real yacht filenames are ASCII, and Perl operates on bytes throughout (its `s///` is byte-wise), so this is a theoretical concern only. Pre-existing pattern (dedup does the same). No action needed; noting for completeness. (Low.)

- **[Low] L5 — `-V` short flag is a Rust-only addition.** Perl `NOMe_filtering` registers only `'version'` (long, line 411) — there is no `-V`. The Rust `Cli` adds `#[arg(short = 'V', long = "version")]` (cli.rs:77). Harmless (version output is not byte-gated, SPEC §2) and conventional for a Rust CLI, but it is a surface that Perl lacks. No action. (Low.)

### 5. `error.rs` (`rust/bismark-nome-filtering/src/error.rs`)

Variants and `#[from]` wiring are correct: `Genome(#[from] GenomeError)` with `#[error(transparent)]` (:14–16), `Io(#[from] std::io::Error)` (:45–46), plus the three string-message variants. ✔ The error messages for `MissingGenomeFolder` (:19) and `InfileNotFound` (:24) match the Perl die strings verbatim (`NOMe_filtering:494,454`). `MergeCpgsWithCx` (:38–42) matches `:499`. `EmptyInput` message (:30–34) matches `:174`. ✔

- **[Low] L6 — `EmptyInput` declared-not-raised is documented intent, not a dropped requirement.** The variant carries a doc comment stating it is "raised **after** the output header has been written... (Phase B wires the header-first ordering)" (:27–34), and the IMPL coverage-checklist note (row after #12) flags it as "declared here so the enum is complete... raised in Phase B." This is the right call — declaring the full enum now avoids a churny Phase-B edit, and the D4 ordering (header byte written *before* the error) is the Phase-B gate's job. **No action** — properly scoped, not a stub. (Low — informational.)

- **[Low] L7 — `InfileNotFound` doubles as "no positional infile supplied".** `validate` maps `self.infile == None` → `InfileNotFound` (cli.rs:110), and `run` also raises it when the resolved path doesn't exist (lib.rs:58–60). The doc comment acknowledges both uses (:22–25). Perl distinguishes these (no-ARGV → helpfile + `exit` 0; non-existent → `die` exit non-zero). Since help/exit-code is not byte-gated (SPEC §2) and the IMPL T6 notes call this an acceptable Phase-A simplification, no action. See also M3 below for the broader ordering divergence. (Low.)

### 6. `lib.rs` `run()` + `main.rs`

`run()` scope is honest: validate → create output dir (guarded against `""`/`.`, :52–54) → infile-exists check (:58–60) → genome load → stderr line (:65–68) → `Ok(())`, with an explicit "Phase B lands here" comment (:70–72). No output file is written, matching the Phase-A gate. ✔ `version_string()` format is `NOMe_filtering_rs <semver> (<os>/<arch>)` (:30–37) — verified at runtime: `NOMe_filtering_rs 0.1.0-beta.1 (macos/aarch64)`. ✔ `main.rs` maps `Ok`→`SUCCESS`, `Err`→`ExitCode::from(1)`, and clap parse errors→2 (clap default) — documented in the module doc (:6–10). ✔ No forbidden silent no-op: `run()` is a legitimately-scoped milestone, not a stub. ✔

- **[Medium] M2 — `run()`'s current shape does not yet host the D4 header-before-loop ordering, and the genome load sits *before* any writer is opened.** This is correct *for Phase A*, but it is the single most important thing for Phase B to get right (D4 / P11: the empty-input path must leave a header-only `.gz` on disk). As written, the natural Phase-B extension is to append the read loop after the genome load (after lib.rs:68). That ordering would be **wrong** for D4 unless the writer is opened and the header line flushed *before* the loop and *before* the `EmptyInput` decision. The `run()` body gives no structural cue (no placeholder for "open writer + write header here, then loop, then maybe-EmptyInput"). Recommend a Phase-A comment in `run()` explicitly reserving that ordering, e.g. at lib.rs:70: `// Phase B ordering (D4/P11): open GzEncoder(output_path) → write header line → stream+group reads → flush; if no data line was ever seen, the header-only .gz is already on disk, THEN return EmptyInput.` This costs nothing now and directly prevents the highest-risk Phase-B divergence the SPEC calls out. (Medium — forward-looking; not a Phase-A defect.)

- **[Medium] M3 — no-argument / no-infile invocation diverges from Perl's order, and validate's check order differs.** Runtime check: `NOMe_filtering_rs` with **no args** prints `error: Please specify a genome folder to proceed` and exits 1. Perl with no args (after passing help/version) hits `unless (@ARGV)` *first* (line 444), prints the helpfile, and `exit`s (code 0). So for the bare-invocation case the Rust port reports a *genome* error where Perl reports a *missing-file/help* state, and exits 1 vs 0. Additionally, `validate` checks merge+CX → genome → infile (cli.rs:103–110), whereas Perl checks no-ARGV/help → version → infile-exists → genome → merge+CX. All of these are STDERR/exit-code/help paths that SPEC §2 explicitly does **not** byte-gate, and the IMPL T6 note pre-acknowledges the simplification, so this is **not** a blocking defect. Flagging it so it is a conscious, documented choice rather than an accident — and because if a future real-data harness ever asserts on exit codes for the empty-invocation case, this will surface. Consider, optionally, handling `cli.infile.is_none()` in `main` before `run` to print the helpfile (per the IMPL's own "optional polish" note). (Medium — divergence, out-of-scope-by-design but undocumented as a *deliberate* deviation in the code.)

### 7. Rust idioms / structure / tests

- Naming is consistent with siblings (`Cli`/`validate`/`ResolvedConfig`/`BismarkNomeError`). `#![forbid(unsafe_code)]` + `#![warn(missing_docs)]` present on both `lib.rs` files. ✔
- `Genome::len`/`is_empty` pair present (no clippy `len_without_is_empty`). ✔
- Test quality is good: the genome module pins the two-plain-tier-no-union, `.fa.gz`-invisible footgun, gz-still-works, dotfile-exclude, Mus skip, CRLF, dup-name, bare-header divergence, `u32` guard. The CLI tests pin the die (+ alias), missing-genome, inert acceptance, the `--dir` contract, and version-without-infile. ✔
- **[Low] L8 — Phase-A integration test `help_exits_successfully` asserts `.success()` (exit 0).** Perl's `print_helpfile` ends with `exit 1` (line 658). clap's `--help` exits 0. Not byte-gated, conventional for clap, but the test *codifies* the divergence from Perl's exit-1. Acceptable; noting that if exit-code parity is ever desired it will need a clap override + test change. (Low.)
- **[Low] L9 — `--nome-seq` "non-negatable" is slightly stronger than Perl.** SPEC §4 and the cli.rs doc say `--nome-seq` is non-negatable. Perl registers `'nome-seq' => \$nome` as a plain bool, so `--nome-seq=0`/`--nonome-seq` would actually *disable* `$nome` in Perl (turning off NOMe filtering — a real behavior difference). The Rust flag is a plain `bool` and is inert (NOMe is unconditional). Since the NOMe path is the only behavior this tool has and the flag never reaches output in either implementation for the *default* (flag-present-or-absent) case, this only matters if a caller passes `--nome-seq=0` — which the SPEC implicitly treats as out of scope. No Phase-A action; worth a one-line SPEC footnote that Perl's `--nome-seq=0` (NOMe-off) is an unsupported/undocumented mode. (Low — SPEC nuance, not code.)

---

## Recommendations (prioritized)

| Priority | Ref | Recommendation | Locus |
|----------|-----|----------------|-------|
| **Critical** | — | None. | — |
| **High** | — | None. | — |
| **Medium** | M1 | Pin the absolute-infile `Path::join` semantics with a unit test **or** a one-line code comment (it silently drops `--dir`; Perl-consistent but untested/undocumented). | `cli.rs:117` |
| **Medium** | M2 | Add a Phase-A comment in `run()` reserving the D4/P11 ordering (open writer → write header → loop → maybe-`EmptyInput`) so the natural Phase-B extension doesn't append the loop *before* the header and break the header-only-`.gz` artifact. | `lib.rs:70` |
| **Medium** | M3 | Document (in code or a SPEC deviation note) that the no-arg/no-infile path deliberately diverges from Perl's helpfile-then-`exit`-0 (Rust reports genome/infile error, exit 1) and that validate's check order differs — both non-gated but should be a conscious choice. Optionally handle `infile.is_none()` in `main` (IMPL's own "optional polish"). | `cli.rs:103–110`, `main.rs` |
| **Low** | L2 | Port c2c's `empty_file_in_winning_tier_errors` + `headerless_file_errors` tests into the promoted `genome.rs` for parity (code path is identical; coverage is thinner). | `genome.rs` tests |
| **Low** | L3 | Add `// order matters: .gz before .txt (Perl source order)` to harden against a future reorder. | `filename.rs:33` |
| **Low** | L1,L4,L5,L6,L7,L8,L9 | Informational / no action: read_dir sort justification, lossy-vs-raw infile string, `-V` addition, `EmptyInput` declared-not-raised (correct), `InfileNotFound` dual-use, `--help` exit 0, `--nome-seq` negation nuance. | various |

---

## Verification performed

- Read SPEC rev 1, IMPL Phase A, full Perl source (660 LOC), the c2c genome twin + its `Cargo.toml`, and `bismark-io`'s `error.rs`/`lib.rs`/`Cargo.toml`.
- `git status` / `Cargo.lock` diff: only the new crate + member; `bismark-io` stays `=1.0.0-beta.8` (P7 ✔).
- `cargo build --workspace` ✔ · `cargo test -p bismark-io genome` 13/13 ✔ · `cargo test -p bismark-nome-filtering` 26 unit + 6 integration + 1 doctest ✔ · fresh `cargo clippy -p bismark-io -p bismark-nome-filtering --all-targets -- -D warnings` clean ✔.
- Runtime: `--version`/`-V` → `NOMe_filtering_rs 0.1.0-beta.1 (macos/aarch64)`, exit 0; no-arg → genome error, exit 1 (see M3).
- Confirmed Rust `Path::join` absolute-arg replacement semantics with a standalone `rustc` snippet (see M1).
