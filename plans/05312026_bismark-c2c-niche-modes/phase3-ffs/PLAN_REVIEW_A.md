# Phase 3 PLAN review — Reviewer A

**Plan:** `plans/05312026_bismark-c2c-niche-modes/phase3-ffs/PLAN.md` (rev 0)
**Reviewer:** A (independent; no shared state with Reviewer B)
**Date:** 2026-05-31
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` (branch `rust/c2c-v1x`, baseline `cargo build` GREEN)

## Verdict: **APPROVE-WITH-CHANGES**

The byte-identity crux — the six tetra/penta/hexa substr offsets on both strands, including the forward-hexa negative-wrap and the reverse-hexa `i-3` asymmetry — is **correct and re-verified byte-identical against live Perl v0.25.1** from a from-scratch reimplementation of §3.2 (two independent fixtures, including an N-containing genome). The scope (CpG+CX, covered+uncovered), the `--merge_CpGs` no-op interaction, the summary invariance, and the `--zero_based` orthogonality all reproduced on live Perl. The design (append-3-columns to the single `extract`/`emit_position` kernel, no `merge.rs` change) is sound.

The changes I'm requesting are **not** logic flaws in the offset table — they are: (1) the prompt-flagged **stale line numbers / `emit_position` signature** (the plan was written against pre-Phase-1 `report.rs`; every cited line is now shifted and the `nome`/`cov_out` params already exist), which the implementer must re-confirm; (2) a **mis-described test-surface count** (`extract` has ONE caller, not two; the rejection test is a shared `--drach`/`--ffs` loop that must be *narrowed*, not removed); and (3) one **genuine validation gap** I uncovered — the `--help` text claims "sequences containing Ns are ignored," which is **FALSE in v0.25.1** (Perl emits N-windows verbatim); the plan never mentions N-handling, and a naive implementer could "helpfully" add the filtering the help describes and diverge.

---

## Live-Perl checks I ran (worktree `coverage2cytosine` v0.25.1)

Fixtures: `chr1=GCCGTGAAACACGGCTTT`, `chrM=AACGCCAAGGCC`, `chrC=CGTAAACCC` (chrC left **uncovered** to exercise the uncovered pass); separate `chrN=ACGNTGCGNAACG`.

| # | Check | Fixture / command | Result |
|---|-------|-------------------|--------|
| A | **§3.2 full offset table** == live Perl | `--CX --ffs`; my from-scratch §3.2 reimpl diffed field-by-field (meth/nonmeth zeroed) | **IDENTICAL** (all interior + edge + empty) |
| B | forward hexa **negative-wrap** `i=1` | chr1 pos2 | hexa = `T` (`substr(seq,-1,6)`); my reimpl matches Perl |
| C | forward hexa **negative-wrap** `i=0` | chrC pos1 | hexa = `CC` (`substr(seq,-2,6)`); matches |
| D | reverse **empty penta** `i=3` + tetra/hexa | chr1 pos4 `-` | tetra=`CGGC`, penta=`""`, hexa=`CACGGC`; matches |
| E | forward **empty penta** at chr-end | chr1 pos15 `+` | tetra=`CTTT`, penta=`""`, hexa=`GGCTTT`; matches |
| F | forward **all-three empty** at chr-end | chrC pos7 `+` | tetra/penta/hexa all `""` → line ends `...CCC\t\t\t\n`; matches |
| G | empty-field byte rendering | `cat -A` of D/F | `...CGGC\t\tCACGGC\n` (matches §3.3); all-empty = trailing `\t\t\t\n` |
| H | **CpG-only** `--ffs` (no `--CX`) | `--ffs` | 10-col, CG-only lines, incl. uncovered chrC. Scope (§3.1) confirmed |
| I | **uncovered chr** 10-col `0 0` lines (V13) | chrC genome-only, thr 0 | emits 10-col `0 0` lines (Perl `:1524`) |
| J | **summary invariance** (V5) | `--CX` vs `--CX --ffs` summaries | byte-IDENTICAL |
| K | **`--ffs --merge_CpGs`** allowed + merged-cov invariance (V6) | `--ffs --merge_CpGs` vs `--merge_CpGs`; diff merged cov | Perl accepts both; merged cov byte-IDENTICAL; ffs report is 10-col |
| L | **`--zero_based`** orthogonality (V10) | `--CX --ffs --zero_based` | pos −1; cols 7–10 byte-identical to 1-based |
| M | **N-windows** behaviour | `chrN` with embedded `N`s, `--CX --ffs` | Perl **emits** N-windows verbatim (`CGN`, `CGNT`, revcomp passes N→N); `--help` "Ns are ignored" is FALSE; my §3.2 reimpl matches Perl exactly |
| N | Rust crate baseline | `cargo build -p bismark-coverage2cytosine` | GREEN |

I also read all three Perl extraction blocks (`:262-341`, `:507-585`, `:1421-1493`), the three emission groups, `process_commandline:2018-2029` (flag spec; **no** `tetra` mutex anywhere), `combine_CpGs_to_single_CG_entity:1802` (6-element split discard), and the shipped Rust `report.rs` (`perl_substr`, `revcomp`, `extract`, `emit_position`, the call chain) + `merge.rs::parse_report_row` + `cli.rs`.

---

## Findings

### Logic

- **L1 (positive).** The §3.2 offset table is exactly right and the riskiest case — forward hexa where the guard `len≥i+4` *passes while the offset `i-2` is negative* (→ Perl negative-wrap, NOT empty, NOT clamped-to-0) — is correctly captured and reproduced (checks B/C). The reverse-hexa `i-3`/`want=6` with the `i≥3` (tetra) guard, distinct from `i-2`, is also correct (check D). `perl_substr` already models the wrap (`report.rs:99-111`); `revcomp` passes N through (`:113-126`). No clamping bug.
- **L2 (positive).** The forward-hexa and forward-tetra guards are *both* `len≥i+4` (Perl `:265`/`:279`: tetra `len≥pos-1+4`, hexa `len≥pos-3+6` → both `len≥i+4`). So they never diverge; a single position can emit a forward-window tetra **and** a negative-wrap hexa (chr1 i=1: tetra `CCGT`, hexa `T`) — confirmed.
- **L3 (positive).** No-`merge.rs`-change claim (§3.6) verified two ways: `parse_report_row` (`merge.rs:58-71`) requires `f.len()≥6` and reads only `f[0..6]` (it doesn't even read `tri` at f[6]), so a 10-col line is tolerated; and live Perl's merged cov is byte-identical with/without `--ffs` (check K). A trailing-all-empty line (`...CCC\t\t\t`) splits to 10 fields → still ≥6 → fine.

### Assumptions (stale refs — prompt-flagged; all confirmed stale)

- **A1 (Important — STALE signature).** §3.3/§4 present `emit_position(... threshold, ffs, accumulate_summary, summary, out)` with `ffs` immediately after `threshold`. The **live** signature (post-Phase-1, `report.rs:169-182`) is `(... threshold, nome, accumulate_summary, summary, out, cov_out)` — i.e. there is already a `nome: bool` after `threshold` and a `cov_out: &mut Vec<u8>` after `out`. The implementer must thread `ffs` **alongside** `nome` (e.g. after `nome`), and `chromosome_report_bytes` (now returns `(Vec<u8>,Vec<u8>)`) passes `config.ffs` at the single `emit_position` call (`report.rs:279-292`). Mechanical, but the plan's signature block will not compile as written — re-confirm against the live tree.
- **A2 (Important — STALE line numbers).** Every cited `report.rs`/`cli.rs` line is shifted by Phase 1. Verified live: `perl_substr` is `:99` (not `:91`); `revcomp` `:115` (not `:107`); `extract` `:145` (not `:137`); `emit_position` `:169` (not `:161`); `chromosome_report_bytes` `:264` (not `:226`); `run_single` `:316`; `run_split` `:406`; `flush_split_chromosome` `:471`. In `cli.rs`: the `--ffs` rejection is `:159-161` (not `:158-160`); `ResolvedConfig` ends `:142-143` (no field literally named after `discordance` is wrong — the last field IS `discordance: Option<u8>` at `:142`, so "add after discordance" is fine); constructor block `:234-252`; the doc-comment `:99-101`. The rejection **test** is `rejects_v1x_flags` at `cli.rs:320-332` (NOT `:303`).
- **A3 (Important — mis-described test surface).** §4/§5 task 4 say "`extract`'s … two interior call sites: `emit_position` + its unit-test harness `run_t`." Wrong on two counts: (a) `extract` has **exactly one** caller — `emit_position` (`report.rs:184`); `run_t`/`run_nome` call `emit_position`, not `extract`. So extending `extract`'s signature touches one line. (b) Phase 1 split the harness into `run_t` → `run_nome(...,false)` → `emit_position` (`report.rs:677-720`). Threading `ffs` through the kernel requires updating `run_nome` (the real driver, with the `nome` param at `:690`) and the `run_t`/`run_t0` wrappers — re-confirm these, not "`run_t`."
- **A4 (Important — rejection-test wording).** §3.7/§5 task 5 say "remove `("--ffs","ffs")` from the rejection test loop … replace with a positive assertion." The live test is a **shared** loop `for (flag,frag) in [("--drach","drach"),("--ffs","ffs")]` (`cli.rs:323`). The implementer must **narrow** it to `[("--drach","drach")]` (Phase 2 still rejects `--drach`) — do **not** delete the whole `rejects_v1x_flags` test. The plan's intent is right; the wording risks an over-deletion that would drop `--drach` rejection coverage.

### Validation sufficiency

- **V-gap-1 (Important — N-windows / `--help` mismatch).** Check M: the Perl `--help` (`:2291`) claims "sequences containing Ns are ignored," but v0.25.1 does **NOT** filter N-containing ffs windows — it emits them verbatim, and `tr/ACTG/TGAC/` leaves `N` unchanged on the reverse strand. The correct behavior is the plan's offset table (no N-filter), and the existing `revcomp` handles it. **Two action items:** (a) add a validation cell (a `chrN` golden with C/G whose tetra/penta/hexa windows span an `N`, both strands) so a future "helpful" N-filter would be caught; (b) add a one-line note to §3.2/§8 that the `--help` "Ns ignored" text is stale and must NOT be implemented. Without this, the matrix never exercises an N inside an ffs window.
- **V-gap-2 (Optional).** The matrix lacks an explicit **all-three-empty trailing-tab** assertion (check F: a line ending `...CCC\t\t\t\n`). V3's chrC `i=6` covers the *fields*, but an explicit byte-level golden of the trailing-tab line (and that `parse_report_row` still accepts it in the `--merge_CpGs` re-read) would lock the rendering. Cheap; fold into V8/V9.
- **V-gap-3 (Optional).** No cell pins a position where a forward window emits a real tetra **and** a negative-wrap hexa simultaneously (chr1 i=1: tetra `CCGT`, hexa `T`). V2 covers the wrap value; a combined assertion documents the "guard passes, offset negative" interaction explicitly.
- **Coverage is otherwise sufficient:** V2 (fwd-hexa wrap), V4 (rev empty-penta `i=3`), V3 (chr-end empties), V6 (merge invariance), V5 (summary), V13 (uncovered) all map to checks I reproduced on live Perl. The forward-hexa negative-wrap and reverse-hexa `i-3` — the two subtle offsets — are caught by V2 and V4.

### Efficiency

- No concerns. §6 is accurate: O(1) per cytosine (≤6-byte slices + ≤3 short `revcomp` allocs on `-` positions), the `ffs==false` path skips computation so the v1.0 hot path is untouched. Output grows ~15–25 B/line; the Phase-4 disk-headroom note is appropriate. `Vec<u8>` over `SmallVec` is the right byte-identity-first call.

### Alternatives

- The `Extracted` struct vs extended tuple (§4, §10 Open) is correctly left to the implementer; it doesn't affect output. A struct is clearly better for 6 fields. No other alternative worth pursuing — the append-to-the-single-kernel design is the right one and structurally avoids the dual-driver drift the plan calls out.

---

## Action items

### Critical
*(none — the offset-table crux is correct and live-verified; the plan is implementable as designed.)*

### Important
1. **A1/A2 — re-confirm the live `emit_position` signature and ALL line numbers before implementing.** The plan was written against pre-Phase-1 `report.rs`; `nome`/`cov_out` already exist and every cited line is shifted. Thread `ffs` alongside `nome`; pass `config.ffs` at the single `chromosome_report_bytes`→`emit_position` call (`report.rs:279`).
2. **A3 — `extract` has ONE caller; the harness is now `run_t`→`run_nome`→`emit_position`.** Update `run_nome` + the `run_t`/`run_t0` wrappers, not "`run_t`."
3. **A4 — NARROW the shared `rejects_v1x_flags` loop to `[("--drach","drach")]`; do not delete it** (Phase 2 still needs `--drach` rejection). Add the positive `--ffs`-resolves assertion separately (V7).
4. **V-gap-1 — add an N-window golden cell AND a note that the `--help` "Ns ignored" claim is stale/false** (v0.25.1 emits N-windows verbatim; do not implement N-filtering).

### Optional
5. **V-gap-2** — explicit byte-level golden of the all-three-empty trailing-tab line (`...\t\t\t\n`) + its tolerance in the `--merge_CpGs` re-read.
6. **V-gap-3** — a combined assertion at chr1 i=1 (real fwd tetra + negative-wrap hexa on the same line).
7. **Doc nits:** covered-pass emission `print` lines are `:399/414/433/441` (plan says `:398/413/432/441` — it cites the `if ($tetra)` guard line); last-chr `:641/656/675/683` and uncovered `:1524/1533/1545/1553` are exact. §5 task 6 "extend the existing `generate_goldens.sh`" — there is one generator **per phase dir** (`tests/data/phase_b/`, `phase1/`); follow that pattern with a new `tests/data/phase3_ffs/generate_goldens.sh`.

---

## Bottom line

The plan's hardest part — the six substr offsets and the forward-hexa negative-wrap — is **correct and independently re-derived byte-identical against live Perl v0.25.1** on two fixtures (including N-containing). The remaining work is mechanical hygiene (re-sync the stale Phase-1 line numbers / `emit_position` signature, narrow the shared rejection test) plus one real validation gap (the N-window behaviour vs the false `--help` claim). None block implementation; all should be folded into rev 1 before the implement trigger.
