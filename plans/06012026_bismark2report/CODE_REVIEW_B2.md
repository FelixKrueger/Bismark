# Code Review B2 — bismark-report (second-round, focused on the 4 review fixes)

**Reviewer:** Code Reviewer B2 (fresh context, recommend-only)
**Date:** 2026-06-01
**Scope:** Verify the 4 fixes from the first dual review (PLAN §15) resolve their findings without regression. Gate = HTML byte-identical to live Perl `bismark2report` v0.25.1 modulo the timestamp line.
**Tree state:** Fixes are in the working tree (crate not yet committed); reviewed the live files under `rust/bismark-report/src/`.

## Summary

**All 4 fixes are sound.** Each was re-derived against the Perl source of truth and then falsified with throwaway fixtures run through BOTH the live Perl `bismark2report` and the freshly-built `bismark2report_rs`, with the `Data processed at … </p>` timestamp line normalized and `cmp`'d. Every targeted scenario is byte-identical, including the original High-severity CRLF case, the load-bearing mixed-case multi-report + explicit `--dedup_report` path, and a battery of 7 degenerate `Bismark report for:` lines (no panics, all identical). Crate gates are green: `cargo test -p bismark-report` = 52 passed / 0 failed (39 unit + 8 CLI + 5 Perl-oracle incl. `crlf_alignment_byte_identical`); clippy `--all-targets` clean; `fmt --check` clean.

**One NEW, narrow byte-divergence found (Low):** `-o 0` (output name literally the string `"0"`). Perl truthiness treats `"0"` as falsy and falls back to the derived `.html` name; the Rust guard `Some(o) if !o.is_empty()` treats `"0"` as a valid name and writes a file literally named `0`. Confirmed end-to-end. This is a corner of the very "truthiness" claim Fix 3 makes, and is not the `-o ""` case that Fix 3 fixed (that one is correct). Real-world impact is negligible (a user naming their report file exactly `0`), and PLAN §15 already flags F3 as "open-by-design," but the gap is currently undocumented at this specific value and the lib.rs comment overstates the match ("Perl chooses the name with truthiness").

## Per-fix verdict

### Fix 1 — HIGH — `reports/alignment.rs::parse_report_for` (last-`)` within `after`) — RESOLVED ✓

Re-derived Perl `/^Bismark report for: (.*) \(version: (.*)\)/` semantics directly with a Perl probe (chomp removes only `\n`, leaving a trailing `\r` on CRLF lines): the greedy first `(.*)` splits at the **last** ` (version: `, and the greedy second `(.*)` runs to the **last** `)`. The Rust `rfind(rest, " (version: ")` + `rfind(after, ")")` reproduces this exactly.

Verified vs live Perl on 11 inputs — all match the Rust slicing logic:
- normal LF (filename with spaces) → fname=`r1.fq.gz and r2.fq.gz`, ver=`v0.25.1`
- **CRLF** (the original bug) → fname=`s.fq.gz`, ver=`v0.25.1` (trailing `\r` ignored)
- paren inside version (`…v0.25.1)beta)`) → ver=`v0.25.1)beta`
- no closing paren → no match (both sides leave fields unset)
- multiple ` (version: ` → split at the last one
- trailing text after the final `)` → version stops at that `)`
- nested ` (version: ` in the version field, empty version, empty filename — all identical.

End-to-end byte-identity confirmed: `crlf_alignment_byte_identical` (in-tree) + my CRLF/paren/trailing fixtures all `cmp`-equal to Perl.

**Panic safety:** `rest[..vpos]`, `rest[vpos+sep.len()..]`, and `after[..rparen]` are all sliced at indices returned by `rfind` (always `≤ len`), so no out-of-bounds. The 7-case degenerate run (incl. `Bismark report for:` with nothing after, prefix without trailing space, no version token) produced exit 0 and byte-identical output every time.

### Fix 2 — MEDIUM — `discovery.rs` glob collation `(ascii_lowercase, raw bytes)` — RESOLVED ✓

Empirically confirmed live Perl `<*E_report.txt>` orders mixed-case + digit-vs-underscore names case-insensitively: `a2_, a_, aB_, B_, C_` (NOT raw-byte `B_, C_, a2_, a_`). A Perl `sort { lc($a) cmp lc($b) or $a cmp $b }` reproduces the glob order exactly (verified `SAME_AS_GLOB: YES`). The Rust key `as_encoded_bytes().to_ascii_lowercase()` with `.then_with(|| x.cmp(y))` raw-byte tiebreak matches this.

- `as_encoded_bytes()` is correct here: for ASCII filenames it equals the UTF-8 bytes; `to_ascii_lowercase()` folds only A–Z, matching Perl `lc` under the C locale.
- Tiebreak: verified Perl orders case-fold-equal names by raw bytes (`AB < Ab < aB < ab`); Rust's `PathBuf::cmp` gives the same raw-byte order. This tiebreak only fires on a case-sensitive FS and is byte-neutral for independent reports; it is a sound determinism guarantee.

**Load-bearing path proven, not vacuous:** built a dir with `B_, a2_, a_, C_ _PE_report.txt` + an explicit `--dedup_report`, ran both tools. All four HTMLs byte-identical, and the dedup section landed ONLY on `a2_PE_report` (the case-insensitive first), in BOTH Perl and Rust — exactly the line-1256 first-report reset. The pre-fix raw-byte sort would have attached it to `B_` in Rust → divergence; the fix eliminates it.

### Fix 3 — LOW — `lib.rs` `-o ""` truthiness — RESOLVED for `""`, NEW narrow gap at `"0"` ⚠

- `-o ""`: Perl `if ($manual_output_file)` is falsy on `""` → derives the name. The `>1 report` guard uses `defined` → `is_some`. Rust matches: name uses `Some(o) if !o.is_empty()`, guard uses `cli.output.is_some()`. Confirmed end-to-end: both write the derived `sampleD_PE_report.html` and are byte-identical. ✓
- **`-o 0` (NEW, Low):** Perl truthiness is also falsy on the string `"0"` (verified: `[0] => falsy`), so Perl derives the name. Rust `!o.is_empty()` treats `"0"` as truthy → writes a file named `0`. Confirmed end-to-end: Perl wrote `sampleD_PE_report.html`; Rust additionally wrote a file literally named `0`. The HTML *content* is identical, but the **filename diverges**.

### Fix 4 — LOW — `assets.rs:6` doc — RESOLVED ✓

Doc comment now references the inline `embedded_assets_match_repo_plotly_files` test (assets.rs lines 6 and 121); that test exists and passes. Comment-only, zero byte impact.

## NEW issues

1. **Low — `-o 0` filename divergence (byte-affecting on that exact input).**
   - **Trigger:** `bismark2report_rs --alignment_report X_PE_report.txt -o 0`
   - **Perl:** writes the derived `X_PE_report.html` (`"0"` is falsy).
   - **Rust:** writes a file named `0`.
   - **File:** `rust/bismark-report/src/lib.rs:78-81`. The comment "Perl chooses the name with truthiness" is slightly inaccurate — `!o.is_empty()` is *emptiness*, not full Perl truthiness (which also rejects `"0"`).
   - **Real-world severity:** negligible (requires `-o` to be exactly `0`). PLAN §15 already lists F3 as "open-by-design," so this may be an accepted edge.

2. **(Not a regression — pre-existing, noted for completeness, no action implied)** The same string-truthiness asymmetry would also surface for any other Perl-falsy-but-nonempty value, but the only such strings are `"0"` and `""`; `""` is handled. No other values are Perl-falsy.

## Recommendations by priority

- **Low / optional:** If exact Perl parity at `-o 0` is desired, change the name guard in `lib.rs` to Perl truthiness, e.g. treat `Some(o)` as a real name only when `!o.is_empty() && o != "0"` (Perl `if ($str)` is false for `""` and `"0"` only — `"0.0"`, `"00"`, `" "` are all truthy). Alternatively, leave as-is and add a one-line note to PLAN §15 that the F3 "truthiness" fix intentionally covers only `""` (not `"0"`), so the claim and behavior agree.
- **Low / doc only:** Soften the `lib.rs:78` comment from "with truthiness" to "non-empty (the only Perl-falsy report names are `\"\"`, handled here, and `\"0\"`, not handled)" so it doesn't overstate parity.
- **None blocking.** Fixes 1, 2, 4 are fully correct and verified byte-identical; Fix 3 is correct for its stated `-o ""` target.

## Gates run (this review)
- `cargo build -p bismark-report --bin bismark2report_rs` — OK
- `cargo test -p bismark-report` — 52 passed / 0 failed
- `cargo clippy -p bismark-report --all-targets` — clean
- `cargo fmt -p bismark-report -- --check` — clean
- Live Perl vs Rust byte-identity (timestamp-normalized `cmp`): 3 baseline scenarios (wgbs_pe / wgbs_se / minimal_pe) + FIX1a/b/c + FIX3a + glob 4-report scenario + 7 degenerate report-for lines — **all IDENTICAL**; FIX3b (`-o 0`) — **filename DIVERGES** (the one new finding).
