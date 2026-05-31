# Code Review A2 — `bismark-genome-preparation` post-audit delta

**Reviewer:** A (independent, parallel) · **Date:** 2026-05-31
**Scope:** FOCUSED review of the post-first-review delta only — the M1/H1 fixes + new tests on
`src/discovery.rs`, `src/indexer.rs`, `src/cli.rs`, `src/folders.rs`, `tests/integration.rs`, `README.md`.
**Commit:** `0505985` (HEAD).
**Suite:** green locally — 39 lib + 10 integration on macOS (re-run: all pass).

---

## Summary / verdict

The two delta fixes are individually well-implemented:

- **M1 (bytes-based `in_group`)** — correct, no panic risk, no behavior regression. Approve.
- **`fasta_name_cmp` is a valid total `Ord`** — correct as a comparator (reflexive/antisymmetric/transitive,
  total via the raw-bytes tiebreak). Approve as a function.
- The new **unit tests are real assertions** (not vacuous) and the **oracle tests do compare bytes and
  fail loudly**. The non-UTF-8 skip logic is defensible. Approve the test mechanics.

**However**, there is **one Critical finding**: the *premise* behind the H1 fix is platform-circular. The
rev-3 SPEC and the prior review concluded "Perl `<*.fa>` glob sort is case-insensitive **and
locale-independent**," and `fasta_name_cmp` was calibrated to that. That conclusion was reached **entirely
on macOS**, where Perl's `<...>` delegates to **BSD libc `glob(3)`**, which case-folds. On **Linux/glibc**
(the stated production target — colossal), Perl's `<*.fa>` sorts **bytewise** (`strcmp`, C locale) and does
**NOT** case-fold. So on Linux, `fasta_name_cmp` produces a **different MFA concatenation order than Perl**,
breaking the byte-identity gate for any genome dir with mixed-case FASTA names. The
`perl_vs_rust_mixed_case_glob_order` oracle test cannot catch this on macOS (both sides fold) and would
only fail on Linux.

| Severity | Count |
|----------|-------|
| Critical | 1 |
| High     | 0 |
| Medium   | 1 |
| Low      | 2 |

---

## Critical

### C1 — `fasta_name_cmp` matches Perl's glob on **macOS only**; on Linux it diverges (byte-identity gate)

**Files:** `src/discovery.rs:26-38` (`fasta_name_cmp` + its doc-comment), `src/discovery.rs:34-38`,
oracle test `tests/integration.rs:359-373` (`perl_vs_rust_mixed_case_glob_order`),
SPEC §8.1 / §"Glob sort order parity" (the rev-3 "CORRECTED" claim).

**The claim under review.** The code comment (`discovery.rs:26-33`) and the rev-3 SPEC assert Perl's glob
sort is "case-insensitive **and locale-independent** (verified empirically in code review)," and pin
`fasta_name_cmp = (ascii-lowercased, raw bytes)` against it.

**What I verified (root cause).** The case-folding the prior review observed is **not** a property of Perl
or of `File::Glob` — it is a property of the **C library `glob(3)`** that `File::Glob` delegates to, and
that differs by platform:

- `File::Glob.pm` (`/System/Library/Perl/5.34/.../File/Glob.pm:68-71`) sets `GLOB_NOCASE` **only** when
  `$^O =~ /^(?:MSWin32|VMS|os2|dos|riscos)$/`. On **darwin and linux, `GLOB_NOCASE` is NOT set.** So Perl
  itself is not requesting case-insensitivity on either macOS or Linux.
- On **macOS**, the underlying **BSD `glob(3)` collates case-insensitively anyway** (man 3 glob: "sorted in
  ascending **collation** order"; Darwin's collation case-folds). Demonstrated:
  - `readdir` raw order: `aa, ZZ, Ba, ab`
  - `bsd_glob("*.fa", 0)` (POSIX flags) → **`Ba, ZZ, aa, ab`** (bytewise)
  - `bsd_glob("*.fa", GLOB_CSH)` (= what `<*.fa>` uses; `GLOB_CSH & GLOB_NOCASE == 0`) → **`aa, ab, Ba, ZZ`**
    (case-folded — i.e. the *libc CSH path* folds on Darwin, independent of the NOCASE flag)
  - Perl `<*.fa>` → `aa, ab, Ba, ZZ` (matches `fasta_name_cmp`)
- On **Linux/glibc**, `glob(3)` sorts via `strcmp`/`strcoll`; under the C/POSIX locale this is **bytewise**
  and does **NOT** case-fold. Expected Perl `<*.fa>` order there is `CHR2, Chr10, Scaffold_a, chr1,
  scaffold_b` (digits < upper < lower), **not** the macOS/`fasta_name_cmp` order
  `chr1, Chr10, CHR2, Scaffold_a, scaffold_b`.

(The prior review's "confirmed under `LC_ALL=C`/`LC_COLLATE=C`" observation is consistent with this: on
**macOS**, the BSD CSH-glob folding is not driven by `LC_*`, so the locale knob doesn't change it. That is a
statement about Darwin's libc, not about Perl or about Linux. The "locale-independent" wording conflated two
different axes — the LC_* locale vs. the BSD-vs-glibc `glob(3)` implementation.)

**Impact.** The glob order fixes (a) the MFA concatenation order in `genome_mfa.{CT,GA}_conversion.fa`
(byte-identity-gated) and (b) the comma-joined indexer `file_list` (not gated). For any genome dir whose
FASTA filenames mix case (capitalised scaffolds, `chrUn_*` next to `GRCh38_*`, etc.), the **Rust MFA bytes
will differ from Linux-Perl**. Concretely, with `{chr1.fa, Chr10.fa, CHR2.fa, Scaffold_a.fa,
scaffold_b.fa}`:
- Rust / macOS-Perl: `chr1, Chr10, CHR2, Scaffold_a, scaffold_b`
- Linux-Perl (glibc, C locale): `CHR2, Chr10, Scaffold_a, chr1, scaffold_b`

Note the H1 fix actually *inverted* the divergence: the original bytewise sort matched Linux-Perl (and
diverged from macOS-Perl); `fasta_name_cmp` now matches macOS-Perl and diverges from Linux-Perl. Whichever
single comparator is chosen, it cannot be byte-identical to Perl on **both** platforms, because Perl itself
is not byte-identical to itself across the two libc `glob` implementations.

**Why the oracle test does not catch it.** `perl_vs_rust_mixed_case_glob_order` compares Rust vs. *the
local* Perl. On macOS both fold → pass (this is exactly why the suite is green). It would fail on Linux —
but the gate is being declared on macOS. The test is *sound as a detector on a case-sensitive FS*; it is
**not** sound as evidence that the chosen comparator equals Perl, because it was only ever run where the two
agree.

**Recommendation (decide; do not silently keep macOS-calibration):**
1. **Pin the production platform.** The byte-identity gate target is Linux (colossal). If so, the faithful
   comparator on Linux is **bytewise** (the *original* pre-H1 behavior), not case-folded — i.e. the H1 "fix"
   regresses Linux parity. Re-derive `fasta_name_cmp` against **Linux Perl** (or just `name.as_bytes()`
   bytewise for the C locale), and run `perl_vs_rust_mixed_case_glob_order` **on a case-sensitive FS / on
   Linux** before declaring the gate met. (The harness can't decide this on macOS.)
2. **Correct the comment + SPEC.** Replace "case-insensitive and locale-independent" with the accurate
   statement: Perl `<*.fa>` ordering is the platform libc `glob(3)` collation — **bytewise on glibc/Linux,
   case-folding on BSD/macOS** — and document which platform the byte-identity contract targets. The current
   comment (`discovery.rs:26-33`) and SPEC §8.1 assert a false invariant.
3. If cross-platform byte-identity is genuinely required, the only robust answer is to **not rely on libc
   glob collation at all**: choose one canonical order (recommend bytewise, the POSIX/Linux default), apply
   it on both Rust and Perl sides, and accept that Rust will diverge from *macOS* Perl (documented). The
   current code does the opposite (matches macOS Perl, diverges from Linux Perl), which is the worse default
   for a Linux production target.

---

## Medium

### M2 — `indexer.rs` re-glob uses `to_string_lossy()` (M1 not applied here) + inherits C1's order

**File:** `src/indexer.rs:103-112`.

The discovery-side M1 fix (match/sort on `as_encoded_bytes`) was **not** mirrored in the indexer re-glob,
which still does `e.file_name().to_string_lossy().into_owned()` then filters `s.ends_with(".fa")` and sorts
via `fasta_name_cmp(a.as_bytes(), ...)`. Two notes:
- A non-UTF-8 converted-FASTA name would be **lossy-mangled** here (replacement chars) before being handed
  to the indexer, unlike discovery. In practice the converted file names this crate writes are deterministic
  ASCII (`genome_mfa.CT_conversion.fa`, or `<chr>.CT_conversion.fa` in `--single_fasta`, where `<chr>` comes
  from a header and *could* be non-ASCII). Low real-world risk, but it is an inconsistency with the stated
  M1 intent and the doc-comment "Same case-insensitive ordering as the discovery glob."
- This path also carries the **C1 order divergence** for the `file_list`, but `file_list` is the indexer
  input order (**not** byte-identity-gated), so it only matters cosmetically / for index reproducibility.

**Recommendation:** for consistency, glob/sort on bytes here too (collect `OsString`, sort by
`as_encoded_bytes()`, build the comma list from the same bytes). Severity Medium only because names are
effectively always ASCII. Whatever C1 resolves to, apply the *same* comparator here.

---

## Low

### L1 — `glob_includes_non_utf8_name` can silently pass without testing the guarantee

**File:** `tests/integration.rs` is fine; this is `src/discovery.rs:229-250`.

The test has **two** early-`return` skip paths: (a) the FS rejects the non-UTF-8 write, (b) the name isn't
retrievable (`files.is_empty()`). On a case-insensitive/UTF-8-restricting FS (APFS, the dev default), the
write at `discovery.rs:239` fails and the test returns **before asserting anything** — it becomes a no-op
that always "passes." The only real assertion (`files.len() == 1`, line 249) runs solely on a permissive
case-sensitive FS (ext4/CI). This is acceptable *as designed* (the comment is honest), but it means the M1
guarantee is **unverified on the dev machine** and the green local suite does not exercise it. Confidence in
M1 therefore rests on CI running on ext4. Recommend either (a) gate the test on a known-permissive temp FS,
or (b) add a **pure-unit** test that calls `in_group(b"chr\xff.fa", ".fa")` directly (no filesystem) so the
byte-matching is asserted unconditionally on every platform. The unit-level guarantee is the part actually
fixed by M1; the FS round-trip is environmental.

### L2 — `glob_mixed_case_is_case_insensitive` unit test encodes the (C1-disputed) macOS order as ground truth

**File:** `src/discovery.rs:212-227`.

The unit test asserts `["aa.fa", "ab.fa", "Ba.fa", "ZZ.fa"]`. That is the **correct expectation for
`fasta_name_cmp` as written**, so the test is valid for what it tests (the comparator), and it is a real
assertion (not vacuous). But it bakes the macOS-Perl order in as "the" answer; if C1 is resolved toward
bytewise/Linux parity, this fixture's expected value flips to `["Ba.fa", "ZZ.fa", "aa.fa", "ab.fa"]`. Flag
for revisiting alongside C1; no change needed if C1 confirms macOS as the target.

---

## Items checked and found correct (delta)

- **`fasta_name_cmp` totality/`Ord` validity** (`discovery.rs:34-38`): `lc.cmp(lc).then_with(|| a.cmp(b))`
  is reflexive, antisymmetric, transitive, and total — a sound comparator for `sort_by`. No equal-but-unequal
  trap (raw-bytes tiebreak makes distinct names strictly ordered). Correct.
- **Bytes-based `in_group`** (`discovery.rs:16-24`): the ASCII extension suffixes are byte-identical to their
  `&str` form, so `name.ends_with(b".fa") && !name.ends_with(b".fa.gz")` reproduces the old `&str` matcher
  exactly — no missed/extra matches. No non-UTF-8 name is dropped (the M1 goal). Correct.
- **`as_encoded_bytes()` panic risk** (`discovery.rs:58,64,65`): none — `OsStr::as_encoded_bytes` is total
  and non-panicking (stable ≥1.74). The `unwrap_or(b"")` sort-key fallback is dead-but-safe (`read_dir`
  never yields `.`/`..`, so `file_name()` is always `Some`). Correct.
- **`cli.rs` tests** (`cli.rs:213-303`): 8 real assertions — default aligner, `--mm2` alias, conflicting
  aligners, minimap2 exclusions (loops over all three), `--parallel < 2`, default/explicit threads, missing
  folder, underscore long flags. All assert concrete `Result`/field values. Not vacuous.
- **`folders.rs` tests** (`folders.rs:41-63`): 2 real assertions (subdir creation + structure; pre-existing
  dir is overwrite-not-error). Sound.
- **`bad_path_to_aligner_fails_before_conversion`** (`integration.rs:259-282`): asserts both `.failure()`
  **and** that the CT MFA does not exist — a genuine "fails before Step II writes anything" guarantee, not
  just an exit-code check. Good.
- **Oracle byte-comparison mechanics** (`oracle_compare`, `integration.rs:287-331`; and the inline oracles):
  read both files with `fs::read` (raw bytes) and `assert_eq!` the `Vec<u8>` with a diff message. They fail
  loudly on mismatch and auto-skip only when `perl` (or `gzip`) is genuinely absent — the skip is logged to
  stderr, not silent-pass. Sound. (The C1 caveat is about *what* they compare against — local Perl — not
  about the comparison being byte-level.)
- **`perl_vs_rust_edge_inputs_mfa`** (CRLF / zero-seq / header→header / CR-only / final-no-newline) and
  **`perl_vs_rust_slam`** and **`perl_vs_rust_gzip_input`**: all exercise real divergence-prone paths and
  compare bytes. Good coverage of the conversion-stream edges; these are unaffected by C1 (single-file or
  all-lowercase fixtures).

---

## Bottom line

The delta's **mechanics** are solid: M1 is correctly implemented and panic-free, `fasta_name_cmp` is a valid
`Ord`, and every new test makes real byte-level assertions. The blocker is **C1**: the H1 "fix" and the
rev-3 SPEC are calibrated to **macOS Perl glob** and rest on a "locale-independent case-insensitive"
invariant that is **false on Linux** (the glob collation is a per-platform libc property —
bytewise on glibc, case-folding on BSD — not a Perl/locale property). For a Linux byte-identity gate the new
comparator *regresses* parity for mixed-case genome dirs, and the green oracle test on macOS is not evidence
to the contrary. Resolve C1 by pinning the target platform, correcting the comment/SPEC, and running the
mixed-case oracle on a case-sensitive FS before declaring the gate met. Everything else in the delta can be
merged as-is.
