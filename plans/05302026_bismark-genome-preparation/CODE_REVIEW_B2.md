# Code Review B2 — `bismark-genome-preparation` post-audit delta

**Reviewer:** B (independent, fresh context)
**Scope:** FOCUSED review of the post-audit delta only (M1/H1 fixes + new tests), NOT the whole crate.
**Commit:** `0505985` (single squashed commit; delta = changed surface listed below).
**Verdict:** PASS — no Critical or High issues. The H1 case-fold fix is correct and its oracle test genuinely exercises it. A few Low-priority notes only.

| Severity | Count |
|----------|-------|
| Critical | 0 |
| High     | 0 |
| Medium   | 0 |
| Low      | 4 |

Suite re-run locally (sandbox-disabled, cargo writes `target/`): **39 lib + 10 integration = 49 passed, 0 failed, 0 ignored, no warnings**. `perl` and `gzip` are present on this host, so all Perl-oracle tests RAN (did not skip).

---

## Area 1 — `fasta_name_cmp` correctness + Perl-collation fidelity (discovery.rs:34-38)

**Verdict: correct.** This was the focus of the review and it holds up under empirical Perl testing.

### Total order
`a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()).then_with(|| a.cmp(b))` is the lexicographic composition of two total orders → a total order (antisymmetric, transitive, total, and never `Equal` for distinct byte strings because the raw-byte tiebreak disambiguates case-only collisions). Correct for `sort_by`.

### Does it reproduce Perl's `<*.fa>` collation? — verified empirically
I ran real Perl (`v5.34.1`, darwin) `<*.fa>` on the adversarial set `{aa, ab, Ba, ZZ}`:

- **Perl:** `aa, ab, Ba, ZZ`
- **Pure bytewise (`sort {$a cmp $b}`):** `Ba, ZZ, aa, ab`

Perl is unambiguously **case-insensitive**, matching `fasta_name_cmp`'s `(lowercase, raw)` key. I also confirmed the collation is **locale-independent** (identical under default locale, `LC_ALL=C`, and `LC_ALL=C LANG=C LC_COLLATE=C`), so the comment at discovery.rs:26-33 ("locale-independent") is accurate and the H1 fix is not a locale-dependent gamble.

### The `Ab` vs `aB` tiebreak (reviewer's specific concern)
Could NOT be exercised on this host: APFS is case-insensitive and collapses `Ab.fa`/`aB.fa`/`AB.fa`/`ab.fa` into a single dirent. This is a filesystem limitation, not a code defect — and on a case-sensitive FS the `(lowercase, raw)` tiebreak gives a deterministic, total order regardless of what Perl does for that exotic case (the byte-identity contract is pinned by the oracle test, see Area 3). **No issue.**

### Is the `perl_vs_rust_mixed_case_glob_order` oracle WEAK? — NO, it is strong
This was the sharpest question in the brief. I verified the oracle's filenames (`chr1.fa, Chr10.fa, CHR2.fa, Scaffold_a.fa, scaffold_b.fa`) sort **differently** under the two candidate implementations:

- **Perl / case-insensitive (Rust):** `chr1, Chr10, CHR2, Scaffold_a, scaffold_b`
- **Pure bytewise:** `CHR2, Chr10, Scaffold_a, chr1, scaffold_b`

A non-case-folding regression would produce a different MFA concatenation order and the byte-diff would FAIL. The per-file headers/sequences are all distinct (`s_chr1/AAAA`, `s_chr10/CCCC`, `s_chr2/GGGG`, `s_sa/TTTT`, `s_sb/ACGT`), so the order is observable in output bytes. **The test is not weak — it genuinely pins the case-fold.**

---

## Area 2 — `in_group` on bytes (discovery.rs:16-24)

**Verdict: correct, no off-by-one / suffix-overlap bug.**

- `.fa` group: `ends_with(b".fa") && !ends_with(b".fa.gz")`. A `.fa.gz` name ends in `.gz`, so `ends_with(b".fa")` is already `false` — the `!ends_with(b".fa.gz")` guard is **defensively redundant but harmless** (never the deciding factor). Same for `.fasta` vs `.fasta.gz`.
- `.fasta` files do not leak into `.fa` (last 3 bytes of `...x.fasta` are `sta`, not `.fa`).
- Matching on `n.as_encoded_bytes()` (stable since Rust 1.75) is the correct M1 fix: a non-UTF-8 `.fa` name is matched on bytes rather than dropped by a `to_str()` filter. The `#[cfg(unix)]` test `glob_includes_non_utf8_name` correctly self-skips on case-/UTF-8-restrictive filesystems and only asserts the positive guarantee otherwise.
- Precedence `.fa → .fa.gz → .fasta → .fasta.gz` matches Perl source lines 610-626 (verified). First non-empty group wins; groups are disjoint.

---

## Area 3 — Test soundness

**Verdict: tests assert real behavior and fail loudly.**

- **`bad_path_to_aligner_fails_before_conversion` (integration.rs:259-282)** — genuinely proves *ordering*, not just "it failed." pipeline.rs:45-48 calls `resolve_explicit` in Step I, BEFORE `create_tree` (line 49) and Step II conversion (line 53). The test asserts `.failure()` AND `!CT_conversion.fa.exists()`. If validation regressed to run after conversion, the CT MFA would exist on a failing run and the `!...exists()` assert would fail loudly. Sound. (Minor: it does not assert the specific error variant — acceptable, the no-output assert is the load-bearing proof.)
- **`perl_vs_rust_gzip_input` in-place gzip** — verified empirically: `gzip g.fa` removes `g.fa` and leaves only `g.fa.gz`. So the `.fa` group is empty and the `.fa.gz` fallback group is exercised exactly as the comment (line 401) claims. Both Perl (`gunzip -c`) and Rust (`MultiGzDecoder`) decode the same input. Sound.
- **`perl_vs_rust_edge_inputs_mfa`** — exercises CRLF, zero-sequence record at EOF, header→header, final-no-newline, CR-only file, lowercase+ambiguity in one diff against real Perl. Strong.
- **cli.rs unit tests (8)** — cover default aligner, `--mm2` alias, conflicting aligners, minimap2 exclusions (loops over all three), `--parallel < 2`, default/explicit threads, missing genome folder, underscore long-flag parsing. All assert error variants or resolved values; no tautologies. Good.
- **folders.rs unit tests (2)** — create-subdirs + pre-existing-dir-is-overwrite-not-error. Real assertions on `is_dir()` / `ends_with(...)`. Good.

---

## Area 4 — `oracle_compare` helper (integration.rs:287-331)

**Verdict: correct; cannot pass-by-skipping unexpectedly when `perl` is present.**

- Separate `perl_genome` / `rust_genome` dirs under one tmp root — no cross-contamination.
- Perl: fake `bowtie2-build` found via PATH-prepend (`{fakebin}:{PATH}`). Rust: fake indexer via `BISMARK_BIN`. Both wirings are isolated and exercise the real discovery tiers.
- Skip path is ONLY `!have_perl()`. When perl is present (this host), it always runs both and asserts byte-equality. A wrong-path output would make `fs::read(rel).unwrap()` **panic** (loud), not silently pass. No silent-skip risk.
- `extra_args` are applied to BOTH perl and rust before the positional dir — consistent.
- `have_cmd` is used ONLY to gate the gzip oracle on `gzip` availability; it does NOT gate any indexer (the fake indexer covers Step III), so there is no "pass because a real aligner is missing" hole.

---

## Area 5 — `indexer.rs` re-glob sort (indexer.rs:103-112)

**Verdict: no regression.** The `*.fa` re-glob now sorts via `fasta_name_cmp` (consistent with discovery). It collects via `to_string_lossy()` (line 106), which is lossy for non-UTF-8 names — but the files being globbed here are the **converter's own output** in the CT/GA dirs (`genome_mfa.CT_conversion.fa` for MFA, or `<chrname>.CT_conversion.fa` for `--single_fasta`), which are ASCII/derived names, not arbitrary user names. The lossy path is therefore unreachable for non-UTF-8 in practice and does not affect the byte-identity contract. Acceptable for the delta.

---

## Recommendations

### Low
1. **(discovery.rs:18,20)** The `!name.ends_with(b".fa.gz")` / `!name.ends_with(b".fasta.gz")` guards in `in_group` are dead conditions (a `.gz` name never ends with `.fa`/`.fasta`). Harmless, but a one-line comment noting they're defensive-redundant would prevent a future reader from assuming they're load-bearing. Not required.
2. **(integration.rs `Ab`/`aB` tiebreak)** Consider a `#[cfg(unix)]` oracle test that writes case-only-distinct names guarded by a "skip if the FS folds case" check (the same pattern already used in `glob_includes_non_utf8_name`). On case-sensitive CI (ext4) this would pin the raw-byte tiebreak against Perl; on APFS it self-skips. Optional hardening — current tests already prove the case-*fold*, just not the case-only *tiebreak*.
3. **(README.md:22-24)** `--genomic_composition` (accepted-and-ignored) is omitted from the "Key options" list while `--combined_genome` is documented. Minor doc completeness; it is a deferred no-op so low impact.
4. **(indexer.rs:106)** Optional: a one-line comment that the lossy `to_string_lossy()` is safe here because the globbed names are converter-produced (not user FASTA names) would forestall a "why lossy after the M1 byte fix?" question in review.

---

## Things explicitly checked and found OK
- `fasta_name_cmp` is a valid total order and matches real Perl glob collation (empirically, locale-independent).
- `perl_vs_rust_mixed_case_glob_order` differs from a bytewise sort → genuinely exercises the fix (NOT weak).
- `in_group` byte matching: no off-by-one, no suffix overlap, groups disjoint, M1 non-UTF-8 handling correct.
- `bad_path_to_aligner_fails_before_conversion` proves ordering (no FASTA on failure), backed by pipeline.rs:45-53.
- gzip oracle's in-place gzip leaves only `.fa.gz` (verified empirically).
- `oracle_compare` dir isolation, fake-indexer wiring, and no silent-skip-when-perl-present.
- Full suite green (49 tests), no compiler warnings.
