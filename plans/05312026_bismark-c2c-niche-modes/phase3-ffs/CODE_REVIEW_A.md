# Phase 3 `--ffs` — Code Review A

**Date:** 2026-06-01
**Reviewer:** Code Reviewer A (independent; no shared state with Reviewer B)
**Target:** uncommitted working tree on branch `rust/c2c-v1x` (worktree `/Users/fkrueger/Github/Bismark-c2c`)
**Scope:** Phase 3 of the c2c v1.x epic — `--ffs` tetra/penta/hexamer context columns (7 → 10 cols), byte-identical to Perl v0.25.1.

---

## Top-line verdict: **APPROVE**

0 Critical, 0 High, 0 Medium. Two Low (informational/nits) below. The implementation is faithful, well-contained, and byte-identical to live Perl v0.25.1 across every mode and edge case I tested independently. The sanctioned deviation (a standalone `ffs_fields` helper rather than extending `extract`) is sound and keeps the non-ffs hot path untouched.

---

## Verification run (all green)

| Gate | Result |
|------|--------|
| `cargo test -p bismark-coverage2cytosine` | **168 passed, 0 failed** (97 lib + 18 P1 + 12 P2 + **9 P3** + 11 B + 7 C + 10 D + 4 sanity) |
| `cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings` | clean (exit 0) |
| `cargo fmt -p bismark-coverage2cytosine -- --check` | clean (exit 0) |

The 168-test claim and the no-regression claim in PLAN rev 2 are accurate.

---

## Live-Perl checks I ran (my OWN fixtures, not the committed goldens)

Fixtures built from scratch: a 3-chr genome `chr1=GCCGTGAAACACGGCTTT`, `chrC=CGTAAACCC` (uncovered), `chrN=ACNGTCAANNCGTTT` (N-bearing, both strands), plus degenerate `t1=CG`/`t2=C`/`t3=GC`/`t4=G` and a `b4=CGTA` boundary chr. Diffs are `perl ./coverage2cytosine` (repo-root v0.25.1) vs `target/debug/coverage2cytosine_rs`.

| Fixture / mode | Result |
|----------------|--------|
| `--CX --ffs` (3-chr) — interior, neg-wrap, chr-end empties, N-windows both strands, uncovered chr | **CX_report + summary byte-identical** |
| `--ffs` (CpG-only) | **byte-identical** |
| `--ffs --zero_based` | **byte-identical** |
| `--ffs --split_by_chromosome` (per-chr files) | **byte-identical (`diff -r`)** |
| `--ffs --gzip` (decompressed vs plain Perl golden) | **byte-identical** |
| `--ffs --merge_CpGs` merged cov vs Rust `--merge_CpGs` no-ffs vs Perl `--ffs --merge_CpGs` | **all three byte-identical** |
| `--CX` no-ffs Rust vs Perl (regression) | **byte-identical** |
| cols 1–7 of `--ffs` run vs no-ffs run (append-only) | **byte-identical** |
| cols 8–10 of `--ffs --zero_based` vs `--ffs` (context frozen) | **byte-identical** |
| `t*` 1-/2-base chromosomes, both strands | **byte-identical** (empty report, no crash) |
| `b4=CGTA`, C@i=0, hexa neg-wrap with want overrunning end → `substr(-2,6)="TA"` | **byte-identical** |

Representative `--CX --ffs` output (every edge case in one fixture; all matched Perl exactly):
```
chr1	2	+	1	1	CHG	CCG	CCGT	CCGTG	T          # fwd hexa neg-wrap substr(-1,6)="T"
chr1	3	+	2	2	CG	CGT	CGTG	CGTGA	GCCGTG     # help-example match
chr1	4	-	1	1	CG	CGG	CGGC		CACGGC     # rev penta empty (i<4); rev hexa i-3
chrN	2	+	0	0	CHG	CNG	CNGT	CNGTC	T          # N passed through (fwd)
chrN	12	-	0	0	CG	CGN	CGNN	CGNNT	AACGNN     # N passed through (rev, revcomp N→N)
chrC	1	+	0	0	CG	CGT	CGTA	CGTAA	CC         # fwd hexa neg-wrap substr(-2,6)="CC"
chrC	7	+	0	0	CHH	CCC			           # all-three-empty trailing \t\t\t
```

---

## From-scratch offset-table re-derivation (the prime directive)

I read all three Perl extraction blocks myself (`:262-330`, `:507-585`, `process_unprocessed_chromosomes:1421-1493`) and confirmed they are byte-for-byte identical offset arithmetic. The loop is `while ($seq =~ /([CG])/g)` with `$pos = pos($seq)`; for a single-char match at 0-based index `i`, Perl `pos()` returns the offset **after** the match → `$pos = i+1` (matches the Rust `let pos = (i + 1)`).

Substituting `$pos = i+1`:

| field | Perl | with `pos=i+1` | Rust `ffs_fields` | match |
|-------|------|----------------|-------------------|-------|
| fwd tetra | `substr(s,pos-1,4)`, guard `len≥pos-1+4` | `substr(s,i,4)`, `len≥i+4` | `perl_substr(s,i,4)` if `len≥i+4` | ✓ |
| fwd penta | `substr(s,pos-1,5)`, `len≥pos-1+5` | `substr(s,i,5)`, `len≥i+5` | `perl_substr(s,i,5)` if `len≥i+5` | ✓ |
| fwd hexa  | `substr(s,pos-3,6)`, `len≥pos-3+6` | `substr(s,i-2,6)`, `len≥i+4` | `perl_substr(s,i-2,6)` if `len≥i+4` (signed → neg-wrap at i=0,1) | ✓ |
| rev tetra | `revcomp(substr(s,pos-4,4))`, guard `pos-4≥0` | `substr(s,i-3,4)`, `i≥3` | `revcomp(perl_substr(s,i-3,4))` if `i≥3` | ✓ |
| rev penta | `revcomp(substr(s,pos-5,5))`, `pos-5≥0` | `substr(s,i-4,5)`, `i≥4` | `revcomp(perl_substr(s,i-4,5))` if `i≥4` | ✓ |
| rev hexa  | `revcomp(substr(s,pos-4,6))`, `pos-4≥0` | `substr(s,i-3,6)`, `i≥3` | `revcomp(perl_substr(s,i-3,6))` if `i≥3` | ✓ |

The two subtleties the plan flags are both correct in the code:
- **Forward hexa is the SIGNED offset `i-2`** with the *numeric* guard `len≥i+4` (NOT "perl_substr returned empty"). At `i=0,1` the offset is negative while the guard passes → Perl wraps from the string end. The Rust gates on `len >= i + 4` and lets `perl_substr` do the wrap. Live-verified at `chr1 i=1 → "T"`, `chrC i=0 → "CC"`, and the hardest boundary `b4 i=0 (len 4) → "TA"` (offset `-2` saturates to start 2, want 6 truncates at end) — all byte-identical to Perl.
- **Reverse hexa uses offset `i-3` (= `pos-4`), NOT `i-2`/`pos-3`**, guarded `i≥3`. Code matches (`perl_substr(seq, i as isize - 3, 6)`). Live-verified `chr1 i=4 → "CACGGC"`.

`perl_substr`'s negative-offset model (`saturating_sub(|offset|)`, truncate at end, empty if `start≥len`) is the existing v1.0 helper and reproduces Perl's negative-`substr` semantics exactly at the overrun boundary. `revcomp` (`tr/ACTG/TGAC/`, all else passthrough) passes `N` through — confirmed live on both strands.

---

## Findings by area

### Logic — clean
- `emit_position` calls `ffs_fields(seq, i, strand)` with the `strand` byte returned by `extract` (`b'+'` for C, `b'-'` for G) — the exact discriminator `ffs_fields` branches on. `ffs_fields` is only reached after guard 1 (`tri.len() < 3` skip) and guard 2 (last-base skip), but it carries its own independent `len`/`i≥N` guards, so it is correct regardless of which positions survive. No coupling bug.
- Append happens after `tri` and before `\n`; cols 1–7 are physically untouched. Empty fields render as nothing-between-tabs (`\t\t`), and the all-empty chr-end line correctly ends `…\t\t\t\n`. Both live-verified.
- Threaded through exactly one call site (`chromosome_report_bytes` passes `config.ffs`), which serves all three Perl blocks via the single kernel — the "dual-driver back-port" trap is structurally avoided (one Rust path, three Perl blocks). Confirmed the three Perl blocks are identical, so there is nothing to drift.

### `merge.rs` — confirmed NO change needed
`parse_report_row` requires `f.len() >= 6` and indexes only `f[0..=5]`; a 10-col ffs line splits to 10 fields and the trailing 3 are silently dropped (mirrors Perl's 6-element list assignment in `combine_CpGs_to_single_CG_entity`). Live-verified: `--ffs --merge_CpGs` merged cov == no-ffs merged cov == Perl. No regression to the Phase A mutex set (`--ffs --merge_CpGs` is not rejected, and must not be).

### CLI — clean
- `--ffs` rejection arm deleted; `ffs: bool` added to `ResolvedConfig` + constructor; help text updated. `UnsupportedFlag` variant retained (still used by `--drach`? — no: `--drach` is now supported too; the variant is now unused at the rejection site but the comment explains it is kept for the error-display contract / future deferral — acceptable, see Low-1).
- The SHARED `rejects_v1x_flags` test was correctly **replaced** by `ffs_resolves_and_composes` (positive resolve + composes with `--CX`/`--merge_CpGs`). The plan (§3.7) said "narrow the loop to `[("--drach","drach")]`" — but since `--drach` is *also* supported now (Phase 2), there is no remaining flag to reject, so removing the test entirely is the correct outcome (the plan text predates realising `--drach` is no longer rejected either). `tests/sanity.rs::unsupported_v1x_flag_is_rejected` correctly removed for the same reason; `missing_output_fails_with_clear_message` still covers the fail-clearly bar. **Not a defect** — see Low-2.
- The in-test `ResolvedConfig` literals in `gpc.rs`, `drach.rs`, and `report.rs::nome_cov_path_uses_raw_base` all got `ffs: false` (required for compilation) — done. The crate compiles and all tests pass, confirming no literal was missed.

### Efficiency — clean
`ffs_fields` is called only when `ffs` is true (gated `if ffs` in `emit_position`). The default/non-ffs hot path is byte-identical and computes nothing extra (regression diff confirmed). Per emitted ffs position: 3 short `perl_substr` slices + up to 3 `revcomp` allocations of ≤6 bytes — O(1), negligible. Mirrors the existing `tri`/`upstream` cost; no new heap structures.

### Structure — clean
The standalone `ffs_fields(seq, i, strand) -> (Vec<u8>, Vec<u8>, Vec<u8>)` is well-documented (the doc comment calls out the signed-offset / numeric-guard subtlety and the N-passthrough), unit-tested directly (V1–V4 + N-window), and leaves `extract`'s shipped semantics untouched. This is cleaner than threading a 6-field `Extracted` struct through `extract` and is exactly the spirit of the plan's §4 implementer-choice clause. Sound and well-contained.

### Edge cases independently re-verified
Forward-hexa neg-wrap at i=0/i=1 and the len-4 overrun boundary (V2); chr-end empty windows incl. all-three-empty trailing `\t\t\t` (V3/V16); reverse empty-penta at i=3 (V4); uncovered-chromosome 10-col `0 0` lines (V13); N-windows verbatim both strands (V15); 1-/2-base degenerate chromosomes (no crash); merge invariance (V6); context-summary invariance (V5); zero-based context-column freeze (V10). All byte-identical to live Perl.

---

## Recommendations (Low — non-blocking, optional)

- **Low-1 (informational):** With `--ffs` supported, `BismarkC2cError::UnsupportedFlag` now has no live construction site in `validate()`. The retained comment justifies keeping it for the error-display contract / future deferral, which is reasonable. If `cargo clippy` ever flags it as dead in a future toolchain, a `#[allow(dead_code)]` or a doc note would suffice — currently clean, no action needed.
- **Low-2 (doc hygiene):** PLAN §3.7 / §5 task 5 still instruct to "narrow the `rejects_v1x_flags` loop to `[("--drach","drach")]`", but the implementation correctly *removed* the test (because `--drach` is also no longer rejected). The rev-2 implementation notes already record the deviation ("removed the obsolete `rejects_v1x_flags` test"), so this is only a stale instruction in the body of the plan, not a code issue. No code change.

---

## Byte-identity claims I independently re-verified (summary)

1. The full §3.2 offset table (all six fields, both strands) — **re-derived from scratch** from all three Perl blocks and diffed field-by-field against the Rust; matches incl. fwd-hexa signed `i-2` neg-wrap and rev-hexa `i-3`.
2. Forward-hexa negative-wrap (i=0/i=1 + the len-4 overrun boundary) emits the wrapped short slice, NOT clamped-empty — live Perl.
3. N-windows emitted verbatim on both strands (the `--help` "Ns ignored" is stale) — live Perl.
4. Append-only: cols 1–7 byte-unchanged vs a no-ffs run; non-ffs Rust == non-ffs Perl (no regression).
5. `--ffs --merge_CpGs` merged cov == no-ffs merged cov == Perl (merge discards extra cols; no `merge.rs` change).
6. Context summary, `--zero_based` (context frozen), `--split`, `--gzip` orthogonality — all byte-identical to Perl.

**Verdict: APPROVE — ship it.**
