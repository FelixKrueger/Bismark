# PLAN_REVIEW_A — `bismark-genome-preparation` (Reviewer A)

**Reviewer:** A (independent, fresh context)
**Date:** 2026-05-30
**Target:** `plans/05302026_bismark-genome-preparation/PLAN.md` (rev 0)
**Companion:** `SPEC.md` (rev 2)
**Perl source of truth:** `bismark_genome_preparation` (848 lines)
**Prior art audited:** alanhoyle `bismark-genome-prep/src/main.rs`
**Verdict:** Plan is **sound and unusually well-grounded** in the Perl source. The phasing is correct, the byte-identity heart (A5) is specified correctly, and the three alanhoyle divergences are correctly identified and explicitly forbidden. I found **3 Critical** issues (one a genuine byte-identity correctness gap, two missing-error-parity gaps), **6 Important**, and several Optional items.

---

## 1. Logic review

### 1.1 What the plan gets right (verified against Perl)

- **Raw-line byte transform (A5 / SPEC §5.2):** Correct. Operating on raw bytes *including* the terminator via `read_until(b'\n')`, with `\r` and `\n` in the keep-set and never re-terminating, faithfully reproduces Perl lines 459–463 (`uc` → `s/[^ATCGN\n\r]/N/g` → `tr`). This is the single most important decision and the plan nails it, explicitly rejecting alanhoyle's `trim_end_matches` + re-emit `\n` (verified at alanhoyle main.rs:303 and :345–348, which is divergence #1). **Confirmed correct.**
- **Slam header suffix (C1 / SPEC §8.13):** Verified against Perl lines 427–429 and 454–455: the header is `print CT_CONVERT ">",$chromosome_name,"_CT_converted\n";` with the literal `### TODO: Change this for GrandSlam` comment that was *never acted on*. The slam branches (lines 468–489) only alter `tr`, never the header. alanhoyle main.rs:251–252 emits `_TC_converted`/`_AG_converted` — confirmed divergence #2. Plan C1 correctly pins `_CT_`/`_GA_` with an assertion test. **Confirmed correct.**
- **Extension precedence (A3 / SPEC §2.1):** Matches Perl lines 610–626: `.fa` → `.fa.gz` → `.fasta` → `.fasta.gz`, first non-empty group wins. The "`.fa` excluding `.fa.gz`" exclusion in `find_fasta_files` is correct because Perl's `<*.fa>` glob does NOT match `*.fa.gz` (the `.gz` suffix means `*.fa` won't match). **Confirmed correct.**
- **Uniqueness across all files via HashSet (A3/A5):** Matches Perl `%chromosomes` (lines 409–414). Correct that it spans all input files.
- **`extract_chromosome_name` (A3):** strip `>` (else error), first whitespace token. Matches Perl 572–582.
- **Indexer command shapes (A7/B2/B3):** `bt2/hisat2: <bin> [--threads N] [--large-index] -f <files> BS_CT`; `mm2: <bin> -k 20 [-t N] -d BS_CT.mmi <files>`. Matches Perl 270–275 / 295–299. **Confirmed correct.**
- **Concurrency model:** CT in spawned thread + GA on main thread, join, propagate first failure. Faithful to Perl's fork (lines 241–306) for the *outputs* (FASTA + indices are concurrency-independent). Good.
- **`--genomic_composition` accepted-and-ignored with an explicit note (C3 / SPEC §9):** Correctly avoids alanhoyle's silent no-op (divergence #3). Good.

### 1.2 CRITICAL — Glob ordering: Perl `<*.fa>` re-glob in the indexer dir uses the *converted* filenames, and the plan's `*.fa` re-glob is right, but the MFA-vs-single_fasta ordering interaction with the indexer file_list is under-verified

Two separate globs feed two separate orderings, and the plan conflates them:

1. **Input glob** (Perl 610) over the genome dir → defines MFA concatenation order AND the duplicate-detection order. Plan A3 covers this with `find_fasta_files` + `sort()`.
2. **Re-glob inside `CT_conversion/`** (Perl 266: `my @fasta_files = <*.fa>;`) → defines the indexer `file_list`. Plan A7 says "re-glob `*.fa` in the dir". **This is correct for MFA mode** (one file `genome_mfa.CT_conversion.fa`), but in **`--single_fasta` mode** the indexer `file_list` is the lexical sort of all `<chr>.CT_conversion.fa` files — which is a *different* lexical ordering than the input `.fa` files (the names now carry the `.CT_conversion.fa` suffix and have lost any directory-path component). The plan never states that the indexer re-glob in single_fasta mode must reproduce Perl's `<*.fa>` lexical order in the conversion dir. Since indexer input ordering is **not byte-gated** (only the FASTA is), this is *probably* harmless to the gate — **but** if a future check ever diffs the index or if file_list order ever matters to a downstream consumer, this is unverified. **Action:** PLAN A7/B-tests should explicitly state that the indexer file_list is produced by a lexical sort of the conversion-dir `*.fa` glob (matching Perl `<*.fa>`), and that this is deliberately NOT the input glob order in single_fasta mode. *(Severity is Critical-as-documentation-gap; the byte gate itself is safe.)*

### 1.3 CRITICAL — Missing-genome-folder error must be a *die-equivalent*, but the plan absolutizes in `cli.rs` via "error if it doesn't exist" — Perl's order differs and produces a different failure surface

Perl flow (lines 88–107): `shift @ARGV` → if folder given, `chdir` into it (die if it doesn't exist) and `getcwd()` to absolutize; **else** die "Please specify a genome folder". The plan (A2) folds absolutization into `validate()` with "error if it doesn't exist." Two concerns:

- **`--path_to_aligner` is validated by `chdir` in Step I (Perl 589–604), BEFORE the genome glob.** The plan validates `--path_to_aligner` in A7 (indexer) — i.e. *after* conversion. Perl validates it in Step I (`create_bisulfite_genome_folders`, line 589) *before* the FASTA glob and *before* any conversion. So Perl **fails fast** on a bad aligner path before writing any output; the Rust plan would do the whole conversion (writing files) and only then discover the aligner path is bad. This is a **behavioral divergence**: on a bad `--path_to_aligner`, Perl writes nothing useful and dies early; Rust writes the full converted genome then errors. Not a byte-identity issue per se (the gate is the FASTA, which would be correct), but it changes failure semantics and leaves partial output. **Action:** Move `--path_to_aligner` existence/resolution validation to Step I (folders phase / start of pipeline), matching Perl 589–604, so it fails before conversion.

### 1.4 CRITICAL — Empty-sequence / header-immediately-followed-by-header chromosome is not in the byte-identity test matrix

Perl handles a record whose header is immediately followed by another header (zero sequence lines) gracefully: it writes the converted header, then the inner `while` loop sees the next `>` and writes the next header — so a zero-length chromosome produces just `>chrX_CT_converted\n` with no sequence bytes. The plan's edge sweep (C2) lists CRLF, final-no-newline, whitespace→N, empty/blank lines, first-line-not-`>`, empty dir, duplicate name, gzip — but **not** an empty-sequence chromosome (header-then-header, or header-as-last-line with no sequence, or a completely empty file after the header). SPEC §8.9 mentions "empty sequence lines (`\n`) pass through" but not the **zero-sequence-record** case, and the PLAN coverage checklist has no row for it. The streaming `convert_all` must emit the header and then correctly handle the immediate `>` or EOF. **Action:** add a fixture: a multi-record file where record 2 has a header and *no* sequence line, and a single-record file that is just `>chr1\n` (header only, EOF). Assert the CT/GA bytes match Perl exactly (header line, then nothing).

### 1.5 IMPORTANT — First-line-is-`>`-but-empty-name (bare `>` or `>` + only whitespace) behavior under Perl vs the plan

Perl `extract_chromosome_name` (572–582): `$fasta_header =~ s/^>//` succeeds for a bare `>` (strips it, leaving `""`), then `split /\s+/, ""` returns an **empty list**, so `($chromosome_name) = split(...)` makes `$chromosome_name` **undef**. The converted header becomes `>_CT_converted\n` (undef stringifies to empty with a warning). The plan A3 test says "bare `>` ... → error". **That contradicts Perl**: a bare `>` does NOT die in Perl — only a line that doesn't *start* with `>` dies (the `else` branch at 579). So `>` alone yields an empty chromosome name and an `>_CT_converted` header, not an error. **Action:** Verify the exact Perl behavior for bare `>` and `>   ` (whitespace-only) and align the plan: these are NOT errors in Perl. The plan's "bare `>` → error" unit test would *introduce* a divergence. (The error case is *first byte is not `>`*, per Perl 575/579.)

### 1.6 IMPORTANT — CRLF on the *first/header* line: Perl chomps `\n` only, not `\r`; the `\r` survives into `extract_chromosome_name`

Perl line 403 `chomp $first_line` removes only the trailing `\n` (CRLF → leaves `>chr1\r`). Then `extract_chromosome_name` does `split /\s+/` — and Perl's `\s` **includes `\r`** — so `>chr1\r` → strip `>` → `split /\s+/, "chr1\r"`. Here the `\r` is a *trailing* whitespace char; `split /\s+/` on `"chr1\r"` yields `("chr1")` (trailing whitespace produces no extra field for the first token, and `\r` after `chr1` is treated as the delimiter run). So the name is `chr1` — the `\r` is dropped. The plan A3 test "CRLF header `>chr1\r` → `chr1`" matches this. **BUT:** note this relies on Rust's `extract_chromosome_name` splitting on Rust's whitespace definition. Rust `split_whitespace()` treats `\r` as whitespace (it is Unicode White_Space), so `>chr1\r` → `chr1`. **Confirmed consistent — but only because both languages treat `\r` as whitespace.** The plan should make the `\r`-as-whitespace dependency explicit in the test rationale, and add a test for an *interior* header like `>chr1\rdesc` (CR not followed by LF — old-Mac style mid-header) to confirm the token boundary. Minor but worth pinning given the whole port hinges on header parity.

### 1.7 IMPORTANT — `read_until(b'\n')` and a `\r`-only (old-Mac) line ending: the WHOLE FILE becomes one "line"

SPEC §8.14 and the review brief call out CR-only (old-Mac, `\r` with no `\n`) line endings. With `read_until(b'\n')`, a CR-only file has **no `\n` bytes at all**, so the entire file is read as a single record-spanning blob: first `read_until` returns the entire file (header + all sequence joined by `\r`), the header extraction runs on the whole thing, and the "sequence" loop never iterates. What does Perl do? Perl `<IN>` reads on `$/` = `\n` by default, so Perl *also* slurps the whole CR-only file as one line — `chomp` removes nothing (no trailing `\n`), `extract_chromosome_name` splits on `\s+` (which includes `\r`) taking the first token, and then **there is no second `<IN>` line**, so no sequence is ever written. So Perl and Rust **agree** (both produce just the header, no sequence) — *provided* the Rust header extraction splits on `\r` identically. This is a genuinely subtle equivalence. **Action:** the plan should add a CR-only fixture to C2 and document that the agreed behavior is "header only, no sequence" (not a crash, not a full conversion) — currently CR-only is mentioned in the brief but **absent from the plan's edge matrix**.

### 1.8 IMPORTANT — `--parallel` value plumbing: Perl defaults `$parallel = 1` and only adds `--threads`/`-t` when `$parallel` is truthy; with default 1 it STILL passes `--threads 1`

Re-read Perl 109–115 and 251–259: when `--parallel` is *not* given, `$parallel = 1` (line 114). Then in `launch_indexer` line 251 `if ($parallel)` is **true** (1 is truthy), so `$multicore = "--threads 1"` (or `-t 1` for mm2). **Perl always passes `--threads N` to the indexer, with N=1 by default.** The plan (A2/A7) models `parallel: Option<u32>` and says "`--parallel` (≥2) → `--threads N`" — implying that *without* `--parallel`, no `--threads` flag is emitted. **That is a command-line divergence from Perl** (Perl emits `--threads 1`). Index bytes aren't gated, so this won't fail the gate, but the indexer command string differs, and the plan's A7 unit test ("args/flags for `--threads`") could lock in the wrong behavior. **Action:** decide explicitly — either (a) match Perl and always emit `--threads <parallel|1>`, or (b) document the omission of `--threads 1` as an accepted divergence (§4-style). The plan currently does neither. *(Note: validation still correctly rejects `--parallel 1` as an explicit user value per Perl 110, because `$parallel > 1` is checked only inside `if (defined $parallel)`.)*

### 1.9 IMPORTANT — Sequential-fallback command differs from the forked command (no `--threads` in the fallback)

Perl's fork-failed fallback (lines 309–356) builds the command **without** `$multicore` (compare line 274 `$path_to_aligner $multicore $large_index ...` vs line 325 `$path_to_aligner $large_index ...` — no `$multicore`). So in the sequential path Perl drops the threads flag entirely. The plan's concurrency model (A7 `run_both`) is "spawn CT in a thread + GA on main thread" and always joins — it does not model a "fork failed → sequential, drop threads" fallback. In Rust, thread spawning effectively never fails the way `fork()` can, so the fallback is moot for *outputs*. This is fine, but the plan should note that the Rust port has **no sequential-fallback path** (and therefore no `--threads`-dropping edge), as a documented divergence. Low risk; flagged for completeness.

### 1.10 IMPORTANT — Combined-genome composition with `--single_fasta`: where does the combined FASTA get its bytes?

PLAN D1 says combined FASTA = "CT block ++ GA block" and SPEC §10.1 says it is byte-equal to concatenating `genome_mfa.CT_conversion.fa` ++ `genome_mfa.GA_conversion.fa`. **But in `--single_fasta` mode those MFA files do not exist** — the standard outputs are per-chromosome `<chr>.CT_conversion.fa`. The plan acknowledges combined is "always a single MFA, independent of `--single_fasta`" and offers "stream-concatenate the produced CT then GA content **(or re-run `convert_all` into the combined writer)**." Streaming the per-chr files in single_fasta mode requires concatenating them **in the same glob order** the MFA would have used, and the §10.4 acceptance test ("combined == CT MFA ++ GA MFA bytes") **cannot be run in single_fasta mode because the MFA files don't exist**. **Action:** PLAN D1 should specify the single_fasta combined path concretely — almost certainly the cleanest is to always run a third MFA-style pass (re-run `convert_all` with a single combined writer) regardless of `--single_fasta`, and the D1 test should construct the expected bytes from an *MFA-mode* reference run, not from the on-disk single_fasta files. As written, the test is ambiguous/unrunnable in the single_fasta case.

---

## 2. Assumptions

### 2.1 Validated / correct
- "Rust `Vec<PathBuf>::sort()` == Perl `<*.fa>` lexical order for ASCII filenames." **Largely correct** but with caveats below (2.2).
- "noodles-fasta would break wrapping" — correct; raw streaming is the right call.
- "MultiGzDecoder needed for multi-member" — correct and matches the SPEC's stated genome `.gz` reality.
- "Header always LF even for CRLF input" — verified (Perl writes the literal `"\n"`, lines 427/454).

### 2.2 IMPORTANT — Glob-sort parity assumption is not as airtight as the plan implies
The plan (A3, §8.1) asserts `Vec<PathBuf>::sort()` reproduces Perl `<*.fa>`. Subtleties the plan does NOT address:
- **`PathBuf` sort sorts on the full `OsStr` path**, not the bare filename. Since `find_fasta_files` returns `dir.join(name)` paths, the *common directory prefix* is identical for all entries, so the comparison effectively reduces to filename order — **OK**, but the plan should sort by `file_name()` explicitly (or document that the shared-prefix invariant makes full-path sort equivalent) so a future refactor that mixes subdirectories can't silently break it.
- **Locale:** Perl `File::Glob` default sort is **bytewise (C locale), NOT locale-aware** — good, this matches Rust's bytewise `Ord` on `OsStr`. The plan should *state* this (the brief explicitly asks about locale) rather than leave it implicit; otherwise a reviewer can't confirm parity.
- **Case sensitivity:** bytewise means uppercase (`A`–`Z`, 0x41–0x5A) sorts **before** lowercase (`a`–`z`, 0x61–0x7A) and before digits? No — digits (0x30–0x39) sort before uppercase before lowercase. A dir with `Chr1.fa` and `chr1.fa` orders `Chr1.fa` first in both. The plan's only ordering test is `chr1, chr10, chr11, chr2` (all lowercase). **Action:** add a mixed-case + digit fixture (`chr1.fa`, `Chr2.fa`, `chrM.fa`, `chr10.fa`) to the A3 test to actually exercise the bytewise edge, and add an explicit assertion that the sort is bytewise/C-locale.
- **`.gz` siblings in the same group:** if a dir has `chr1.fa.gz` and `chr2.fa.gz` (the `.fa.gz` group), ordering is again bytewise on the full name including `.gz` — fine, but untested. Low risk.

### 2.3 IMPORTANT — `which`-based discovery vs Perl's PATH-only assumption
Perl does NOT use `which`; it just builds the command string `bowtie2-build ...` and lets the shell/`exec` resolve it (or uses `--path_to_aligner` as a literal prefix). The plan's A7 adds `which::which` discovery (extractor precedent). This is *better* than Perl, but introduces a **new early-failure mode**: if `bowtie2-build` is not on PATH, the Rust port could fail at discovery time, whereas Perl fails only when `system()` runs (after conversion). Combined with §1.3, the plan should decide **when** indexer discovery happens. If discovery is at Step III (after conversion), behavior roughly matches Perl. If the plan front-loads discovery, it changes failure timing. The plan is silent on the *timing* of `which` discovery. **Action:** specify that indexer discovery occurs at Step III launch (not before conversion), to preserve "FASTA is written even if the indexer is missing" — OR document the divergence. Note the extractor precedent also supports a `BISMARK_BIN` env override and `current_exe()` fallback (subprocess.rs:200–232); the plan should state whether genomeprep adopts the same 3-tier discovery or only `which` + `--path_to_aligner`.

### 2.4 IMPORTANT — Binary name `bismark_genome_preparation_rs` vs the alanhoyle/clap `name`
SPEC §3 marks the binary name "(Confirm in review)". The dedup/methcons precedent is `<perl_name>_rs`, so `bismark_genome_preparation_rs` is consistent. **Confirmed reasonable — adopt it.** One nit: alanhoyle's clap `#[command(name = "bismark_genome_preparation")]` (no `_rs`) would make `--help`/usage print the un-suffixed name; the plan should set clap `name` to match the actual installed binary name for clean usage text (not gated, but tidy).

### 2.5 Hardcoded version string
SPEC §4.9 / PLAN A6: the `v0.25.1` + `19 May 2022` banner is diagnostic-only and not in FASTA. **Correct** — verified the banner (Perl 628) is a `warn` to STDERR and carries no bytes into any output file. The plan's use of `version_string()` (CARGO_PKG_VERSION) for `--version` and a separate hardcoded Bismark-version constant for the Step I banner mirrors methcons (`version_string` uses crate version, NOT `0.25.1`). Consistent.

---

## 3. Efficiency analysis

- **Streaming conversion (never slurp):** A5/§8.12 correctly mandates line-streaming for the conversion path. Good — human genome is ~3 GB; slurping would be a memory bug. `read_until(b'\n')` reuse of a single `Vec<u8>` buffer (cleared per line) keeps allocation flat. The plan should explicitly say to **reuse one buffer** (clear, don't reallocate) — alanhoyle reuses a `String` (main.rs:300 `line.clear()`), which is the right pattern; the plan should mirror it for the `Vec<u8>` raw-byte buffer.
- **Two-output write:** each sequence line is transformed twice (CT + GA). Minor — could compute the N-mapped/upcased base once then branch C→T vs G→A into two output buffers in a single pass. The plan implies two `transform_seq_line` calls (one per target). A single-pass two-output transform would halve the per-byte work. **Optional** micro-opt; not worth complicating the byte-identity-critical code unless profiling shows it matters. Flag as Optional.
- **Combined-genome (Phase D):** if implemented as "re-run convert_all into a third writer," that's a third full streaming pass over all inputs (acceptable, streaming). If implemented as "concatenate the already-written MFA files," that's a cheap byte copy (better) — but only valid in MFA mode (see §1.10). **Recommend** the byte-copy-of-MFA approach when MFA outputs exist, and a dedicated combined streaming pass in single_fasta mode.
- **Concurrency:** thread for CT + main for GA is fine; the index builds dominate wall-time and are external. No concern.

No scalability red flags. The plan correctly keeps the only slurping path (`--genomic_composition`) out of scope.

---

## 4. Validation sufficiency

### 4.1 Strong points
- The pure-transform unit tests in A5 (CRLF preserved, final-no-newline, whitespace→N, ambiguity→N, C→T vs G→A) are the right battery and target the highest-risk code.
- The "vs actual Perl output when `perl` available, auto-skip otherwise" approach in A9 is excellent — it makes the byte gate real on every CI run that has Perl, not just a hand-written fixture (which could encode the same bug).
- The slam header assertion (C1) directly pins the alanhoyle divergence #2.
- The real-data harness (E1) with `diff` on CT/GA and `#[ignore]` is appropriately scoped.

### 4.2 Gaps (mapped to the Critical/Important items above)
- **Empty-sequence chromosome** (§1.4) — no test. **Critical gap.**
- **Bare `>` header** (§1.5) — the planned test asserts *error*, which likely contradicts Perl. **Must verify against Perl before writing the test**, else the test locks in a divergence.
- **CR-only (old-Mac) line ending** (§1.7) — in the brief, absent from the plan's matrix.
- **Mixed-case / digit glob ordering** (§2.2) — only the all-lowercase `chr1/chr10/chr2` case is tested; the bytewise case-boundary is untested.
- **Indexer command string for default `--parallel` (=1)** (§1.8) — the A7 test must assert the *exact* Perl-equivalent command, and the plan hasn't decided whether `--threads 1` is emitted.
- **`--path_to_aligner` failure timing** (§1.3) — no test that a bad aligner path fails *before* conversion (or a documented decision that it fails after).
- **Multi-byte / non-ASCII bytes in a sequence line:** the brief asks about this. `b.to_ascii_uppercase()` leaves non-ASCII bytes unchanged, then `s/[^ATCGN\n\r]/N/` maps them to `N`. Perl `uc` on a non-ASCII byte under `use bytes`-less semantics is subtle, but since the regex maps anything not in `[ATCGN\n\r]` to `N` *regardless* of the uc result, the outcome is `N` in both. **Likely fine**, but add one fixture line with a high byte (e.g. `0xFF` or a UTF-8 multibyte) to confirm both produce `N`. **Important** (cheap, removes a real uncertainty).
- **Combined == MFA++MFA in single_fasta mode** (§1.10) — the stated D1 test is unrunnable in single_fasta mode; needs a concrete expected-bytes source.

### 4.3 Missing assertion: directory/file SET equality in single_fasta and combined
SPEC §7.3 requires "the set of files matches" in single_fasta mode. PLAN B1 says "the set of files matches" — good. But there's no test that **no extra files** are produced (e.g. a stray writer for an empty chromosome, or a leftover MFA file). And for combined (D2): "without `--combined_genome`, the `Combined/` dir is absent" — good, that's the additive guarantee. Keep both; they're load-bearing for the "additive" claim.

---

## 5. Alternatives & trade-offs

1. **Single-pass dual-output transform** (vs two `transform_seq_line` calls): halves per-byte work. Trade-off: slightly more intricate code in the byte-identity-critical path. **Recommend deferring** (Optional) — correctness first; the indexer build dominates wall time anyway.
2. **Combined FASTA via byte-copy of MFA files** (when MFA exists) vs re-streaming inputs: byte-copy is strictly cheaper and *guarantees* the §10.4 "combined == CT MFA ++ GA MFA bytes" property by construction (no second transform that could drift). **Recommend** byte-copy in MFA mode; reserve a re-stream only for single_fasta mode.
3. **Validate `--path_to_aligner` + resolve indexer at Step I** (Perl order) vs deferring to Step III: Perl validates at Step I (line 589) but *resolves the binary name* at Step III (line 210). The faithful split is: **validate the directory exists at Step I; resolve+run the binary at Step III.** The plan should adopt this split rather than doing everything in A7. (See §1.3.)
4. **`std::fs::read_dir` + manual extension filter** (alanhoyle's approach, main.rs:198–221) vs a `glob` crate: read_dir is dependency-free and the plan implicitly uses it. Fine — but the plan must sort explicitly (read_dir order is OS-arbitrary, NOT sorted), which the plan does say (`sort()`). Just make sure A3 sorts by `file_name()` (see §2.2).
5. **Atomic/temp-then-rename output writes** vs direct truncate-and-write: Perl truncates in place (`open '>'`). The plan follows Perl (overwrite). Keeping Perl's non-atomic behavior is correct for byte-parity; no change needed, but worth a one-line note that a failed mid-run conversion leaves partial files (same as Perl).

---

## 6. Coverage audit (SPEC → PLAN)

The PLAN's own coverage table (rows 1–26) is thorough and I could map each SPEC behavior to a task. Items the table **under-specifies or omits**:

- **Empty-sequence chromosome** (Perl 431–456 handles header-then-header / header-at-EOF) — **no row**. Add.
- **Indexer file_list ordering in single_fasta mode** (Perl 266 re-glob) — row 11 covers "bowtie2-build indexer + concurrency" but not the single_fasta file_list ordering. Clarify.
- **`--path_to_aligner` validation timing** (Perl 589–604, Step I) — row 12 covers discovery but not the *Step-I timing*. Clarify.
- **Default `--threads 1` emission** (Perl 114 + 251) — row 13 covers `--parallel ≥2 → --threads N` but not the N=1 default behavior. Decide + document.
- **Sequential-fallback command shape** (Perl 309–356) — intentionally dropped (no fork-fail path in Rust); should be an explicit documented divergence (it currently isn't in §4 of the SPEC).
- **Bare-`>` / empty-name header** (Perl 575–577 → undef name) — row 22 says "first line not `>` → error" but conflates it with the bare-`>` case, which is NOT an error in Perl. Split these.

Everything else in the SPEC (gzip input, MultiGzDecoder, MFA/single_fasta, slam, combined, accept-and-ignore genomic_composition, verbose, version/help/man, overwrite-warn) is covered.

---

## 7. Phasing / sequencing

- **Phase A as the full byte-identity gate for the common case:** **Correct.** Default mode = MFA + bowtie2, and the converted FASTA is identical regardless of indexer/mode (SPEC §9). A delivers exactly the gated artifact for the common path. Good call.
- **Sequential, not parallel, phases** (shared `convert.rs`/`indexer.rs`/`pipeline.rs`): **Sound** — same rationale as methcons; parallel streams would conflict on the core modules.
- **Phase boundaries:** A (MVP) → B (modes+indexers) → C (slam+edges+ignore) → D (combined) → E (real-data+docs). Reasonable. **One re-ordering suggestion:** the **bare-`>` / empty-name** and **CR-only** decisions affect the *core* `extract_chromosome_name` and `convert_all` in A3/A5 — they should be resolved and tested **in Phase A**, not deferred to the C2 edge sweep, because getting them wrong means the Phase-A "byte-identity gate" claim is false. Pull those specific edge cases forward into A3/A5.
- **Task sizing:** A5 is the largest/riskiest task and is appropriately isolated as "the byte-identity heart." A7 (indexer) bundles discovery + command-build + concurrency + path_to_aligner; given §1.3/§2.3, consider splitting the *discovery+validation* (Step I timing) from *command-build+run* (Step III). Minor.

---

## 8. Action items (prioritized)

### Critical (must resolve before/at implementation — affect correctness or byte-parity)
1. **Empty-sequence chromosome edge case** (§1.4, §4.2): add a coverage row + fixture for header-then-header and header-at-EOF (zero sequence lines). Verify Rust `convert_all` emits just the converted header. Pull into **Phase A** tests.
2. **Bare-`>` / empty-name header is NOT an error in Perl** (§1.5): the planned A3 "bare `>` → error" test contradicts Perl (lines 575–577 yield an empty name + `>_CT_converted` header). Verify against Perl and align — only "first byte is not `>`" is an error. Fix the plan before writing the test, else it locks in a divergence.
3. **`--path_to_aligner` validation timing** (§1.3): Perl validates the aligner path in Step I (line 589) *before* conversion; the plan validates in A7 (after conversion), so a bad path leaves a fully-converted-but-unindexed genome and a late error. Move directory validation to Step I (resolve+run still at Step III), or document the divergence explicitly.

### Important (resolve during implementation; document decisions)
4. **Default `--threads 1` emission** (§1.8, §6): Perl always emits `--threads <parallel|1>` (default 1). Decide whether the Rust indexer command matches (emit `--threads 1`) or omits it as a documented divergence; fix the A7 command-string test accordingly.
5. **CR-only (old-Mac) line ending** (§1.7): add to the C2 edge matrix; document the agreed behavior (header only, no sequence — both Perl and `read_until`-based Rust agree because the whole file is one "line").
6. **Glob-sort parity rigor** (§2.2): sort by `file_name()` explicitly; add a mixed-case + digit ordering fixture (`chr1.fa`, `Chr2.fa`, `chr10.fa`, `chrM.fa`); assert bytewise/C-locale ordering; state that Perl `File::Glob` default is bytewise (not locale-aware).
7. **Combined-genome in `--single_fasta` mode** (§1.10): D1 must specify the concrete byte source (recommend: byte-copy the MFA files when they exist; otherwise a dedicated combined streaming pass) and the D1 test must derive expected bytes from an MFA reference run, not from on-disk single_fasta files (those MFA files don't exist in single_fasta mode).
8. **Indexer discovery timing + tier** (§2.3): specify that `which`/`--path_to_aligner` discovery happens at Step III launch (preserving "FASTA written even if indexer missing"), and whether genomeprep adopts the extractor's 3-tier discovery (`BISMARK_BIN` → `which` → `current_exe()`) or a reduced set.
9. **Non-ASCII / high-byte sequence char** (§4.2): add one fixture line with a high byte (e.g. `0xFF`) confirming both Perl and Rust map it to `N`.

### Optional (nice-to-have / documentation tidiness)
10. Single-pass dual-output transform micro-opt (§3, §5.1) — defer; correctness first.
11. Note the *no sequential-fallback path* divergence (§1.9) in SPEC §4.
12. Reuse a single `Vec<u8>` line buffer in the streaming loop (clear, don't reallocate) — state explicitly in A5 (§3).
13. Set clap `name = "bismark_genome_preparation_rs"` so usage text matches the installed binary (§2.4).
14. One-line note that, like Perl, a failed mid-run conversion leaves partial truncated files (non-atomic writes) (§5.5).
15. Add a "no extra files produced" assertion to the single_fasta + combined set-equality tests (§4.3).

---

## 9. Summary

The plan is **high quality and faithful to the Perl source** on every load-bearing byte-identity point I could check (raw-byte transform, slam header suffix, extension precedence, header rewrite, LF-header/CRLF-sequence asymmetry). It correctly internalizes all three alanhoyle divergences as things to avoid. The Critical items are not flaws in the core conversion algorithm but in the **edge-case test matrix and failure-timing semantics**: an untested empty-sequence chromosome, a planned test that would lock in a bare-`>` divergence, and `--path_to_aligner` validation happening after conversion instead of before. The Important items are mostly "decide and document" (default `--threads 1`, glob-sort rigor, combined-in-single_fasta, discovery timing, CR-only). None of these block starting Phase A; items 1–2 should be pulled into Phase A specifically, since they undermine the "Phase A is the whole gate" claim if left to the C2 sweep.

**Counts: Critical = 3, Important = 6, Optional = 6.**
