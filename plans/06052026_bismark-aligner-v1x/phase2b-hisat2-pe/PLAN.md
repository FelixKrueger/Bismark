# PLAN — Phase 2b: HISAT2 paired-end (read-1 `ZS` asymmetry) + PE/non-dir/pbat/FastA gate

> **Epic:** `06052026_bismark-aligner-v1x/EPIC.md`, Phase 2b. **Depends on:** Phase 2a (HISAT2 SE core — DONE + gated; PR #949) and the faithful-port Phase 7/8/9a PE machinery (merged).

- **Created:** 2026-06-05 · **rev 1** (focused dual plan-review fixes folded; see Revision History).
- **Branch / worktree:** `rust/aligner-v1x` @ `~/Github/Bismark-aligner`, crate `rust/bismark-aligner` (`bismark_rs`). Builds on the 2a commit (`376a6d9`).
- **Oracle / pin:** Perl Bismark **v0.25.1** + **HISAT2 2.2.2** (oxy `bismark-test`), samtools 1.23.1.

---

## 1. Goal
Make `--hisat2` **paired-end byte-identical** to Perl v0.25.1 + HISAT2 2.2.2 by reproducing the **read-1 `ZS` asymmetry** (the load-bearing find of the dual plan-review, B-L1), then gate PE end-to-end across **directional → non-directional → pbat → FastA PE** at 10k + 1M on oxy. The PE alignment/merge/XM/output/report machinery is already built (Phase 7/8/9a, aligner-agnostic) and the PE HISAT2 option string is already pinned (2a, no `--dovetail`); the only **new logic** is the second-best mask. **SE + Bowtie 2 stay byte-frozen.**

## 2. Context — what exists vs what's new
- **Already correct, reused unchanged** (verified by 2a + the faithful port):
  - **Convert** (`convert.rs`): PE R1 C→T / R2 forward G→A + the `/1 /2` tag handling (Phase 7); pbat inverts, non-dir both-per-mate (Phase 8); FastA PE (Phase 9a). Aligner-agnostic.
  - **Spawn** (`align.rs` `PairedAlignerStream::spawn`, L342): `-1/-2` piped, binary-path-parameterized — Perl drives HISAT2 identically (`-1 … -2 …`). The `SamPair`/`PairedSamStream` peek-two model is unchanged.
  - **Merge/score/MAPQ** (`merge.rs check_results_paired_end`, `mapq.rs`): the PE selection, FLAG/TLEN/strand tables, `calc_mapq` (already `read2_len: Option`-ready) — all aligner-agnostic.
  - **Methylation / output / report / aux** (`methylation.rs`, `output.rs`, `report.rs`, `aux_out.rs`): PE genomic-seq extraction (per-mate, no fragment span), `paired_end_sam_output`, the PE report header (line-order differs from SE — Phase 7) + the `aligner` field branch added in 2a ("run with HISAT2").
  - **Options** (`options.rs`): the PE HISAT2 string `-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --maxins 500 --no-softclip --omit-sec-seq` (**no `--dovetail`**) is built + unit-pinned in 2a (V3).
  - **Discovery/detection/naming**: `.ht2` discovery, `detect_aligner(Hisat2)`, the `_bismark_hisat2_pe*` token (lib.rs PE sites) — all done in 2a.
  - **`--multicore` + `--hisat2` is hard-rejected** (2a gate finding) → the PE gate is **single-core only**; no PE multicore cell.
- **NEW (the only production logic in 2b):** the **read-1 `ZS` mask** for HISAT2 PE.

### The bug being fixed (source-cited)
- **Perl PE read-1 loop** (`bismark` 3372–3382): `if AS:i: / elsif XS:i: / elsif MD:Z:` — **no `ZS:i:` branch**. HISAT2 read-1 emits `ZS:i:` (not `XS:i:`) → `$second_best_1` is **always undef** → backfilled to `$alignment_score_1` (3465–3469).
- **Perl PE read-2 loop** (3386–3403): `else { if($bowtie2){XS:i:} else{ZS:i:} }` — read-2 **does** capture `ZS` for HISAT2.
- **Rust** (`align.rs` `SamRecord::parse`, L101–104) captures `XS:i:` **or** `ZS:i:` uniformly for **every** record. `merge.rs check_results_paired_end` (L598) reads `r1.second_best` + `r2.second_best`, backfills the missing one to its own AS (L599–602), and sums (`sum_second = s1 + s2`, L605) → `calc_mapq`.
- **Divergence:** a PE-HISAT2 pair whose **mate-1** carries `ZS:i:` (a multi-mapping read-1): Perl `sb1 = undef → as1`; Rust `sb1 = ZS_1` (with `ZS_1 ≤ as1`) → different `sum_second` → different MAPQ → non-byte-identical BAM. (Read-2 with `ZS` is already correct on both sides.)

## 3. Behavior
1. **The mask:** in the PE merge, for HISAT2 only, **read-1's second-best is ignored** (treated as if no `XS`/`ZS` tag) — read-2 keeps its `XS`-or-`ZS` second-best. This makes the Rust PE path reproduce Perl's read-1-has-no-ZS-branch behavior. Concretely, at `merge.rs:598` the `sb1` source becomes `None` when `aligner == Hisat2`, so the existing backfill (`sb1 = sb1.or(Some(as1))`) sets `sb1 = as1` exactly as Perl does, and `sb2` is unchanged.
   - **Per-instance:** the mask applies to **every** instance's read-1 in the per-slot loop (the merge scans instances 0,3,1,2), not just the first — `r1` is re-read per instance, so masking at the `let sb1 = …` site covers all.
   - **SE is NOT masked:** `check_results_single_end` is untouched — Perl's SE loop (2780) captures `ZS` for any aligner, and 2a's SE gate proved SE correct. Only the PE function gets the flag.
   - **Bowtie 2 is NOT masked:** for `aligner == Bowtie2` the behavior is byte-identical to today (read-1 keeps `XS`).
2. **Wiring (CORRECTED, dual review):** `check_results_paired_end` gains an `aligner: Aligner` parameter, threaded from the **single** production call site — `drive_merge_pe` (`lib.rs:1231`) — which **both** the single-core and `--multicore` paths reach via `process_pe_chunk`. **`parallel.rs` does NOT call `check_results_paired_end`, so no `parallel.rs` edit is needed** (rev 0 wrongly said "two call sites"). `config.aligner` is in scope there. The `run_pe`/PE test helper(s) must thread the new param too.
3. **Report:** the PE HISAT2 report reads "Bismark was run with HISAT2 …" (2a's `ReportHeader.aligner` branch, used by both SE and PE; the PE line-order is Phase 7's). The echoed `aligner_options` is the PE HISAT2 string (no `--dovetail`).
4. **Gate (single-core, 10k + 1M):** PE directional → PE non-directional → PE pbat → FastA PE (directional + non-dir; **`--pbat ⊕ -f` dies**, Perl 8156, so no FastA-pbat) → **a single-core `--ambig_bam` PE cell (directional, 1M)**. Compare decompressed SAM (`@PG` filtered) + `_PE_report.txt` (wall-clock filtered) + `--unmapped`/`--ambiguous`/`--ambig_bam` aux. **No `--multicore` PE cell** (rejected for HISAT2).
   - **🔴 Why the `--ambig_bam` PE cell (Critical, review B):** `--ambig_bam` is the **only** PE path that re-emits the **raw aligner SAM record** (`output.rs` `build_raw_record`/`write_raw_pe_ambig_lines`, ~755–797 — FLAG/RNEXT/PNEXT/TLEN verbatim, the Bismark reconstruction bypassed). HISAT2's raw PE mate-link fields need not match Bowtie 2's, it is supported single-core (rejected only with `--multicore`, 2a), and it was **never byte-gated for HISAT2** (2a had no ambig cell). This is the highest-risk untested HISAT2 PE path → gate it.

## 4. Signature
```rust
// merge.rs — add the aligner (or a bool) so the PE function can mask read-1's ZS.
pub fn check_results_paired_end<S: PairedSamStream>(
    id: &str,
    /* …existing… */,
    aligner: Aligner,            // NEW (or `mask_read1_second_best: bool`)
    /* …directional, intercept, slope, want_ambig, counters… */
) -> Result<DecisionPaired>;
//   ... at the second-best site (L598) ...
//   let sb1_src = if aligner == Aligner::Hisat2 { None } else { r1.second_best };
//   let (mut sb1, mut sb2) = (sb1_src, r2.second_best);
```
*(Decision: pass `Aligner` for clarity + future minimap2; a `bool` is the minimal alternative. Either keeps the change a one-line gate at a single site.)*

## 5. Implementation outline (TDD)
1. **Lock the baseline** (full suite green at HEAD `376a6d9`; note SE/Bowtie 2 gate md5s are already green).
2. `merge.rs`: add the `aligner: Aligner` param to `check_results_paired_end`; at L598 source `sb1` from `None` iff `Hisat2`. **NB the test discriminator is the `aligner` ARGUMENT, not the tag string** — `align.rs SamRecord::parse` (L100-104) unifies `XS:i:`/`ZS:i:` into the one `second_best` field, so a record with `ZS:i:-6` parses identically to one with `XS:i:-6`; a `mapped_pair_zs` helper is cosmetic (the existing `mapped`-with-`Some(v)` works — just call the function with `Aligner::Hisat2` vs `Aligner::Bowtie2`). **Unit tests first (the 4 mate-tag cases):**
   - **(A) mate-1 second-best + mate-2 second-best, HISAT2** (r1 `Some(-6)`, r2 `Some(-6)`): assert `sum_of_alignment_scores_second_best` = `as1 + zs2 = 0 + (-6) = -6` (read-1 **ignored**, backfilled to `as1=0`), **NOT** `-12`.
   - **(B) mate-1 second-best, Bowtie 2** (regression): read-1's value IS used — unchanged from today.
   - **(C) mate-1 second-best ONLY, mate-2 none, HISAT2** — **the subtlest case (review I-1/B):** masking `sb1→None` makes the `sb1.is_some() || sb2.is_some()` gate (L599) **false** → the merge takes the **no-second-best** branch (Perl 3593), flipping `calc_mapq` from the with-second-best ladder to the no-second-best ladder (a byte-visible MAPQ change, e.g. cap 39→42). Without the mask, today's Rust would wrongly take the second-best branch with `Some(zs1 + as2)`. Assert the no-second-best `DecisionPaired` (and the resulting MAPQ).
   - **(D) mate-1 none, mate-2 second-best, HISAT2**: `sb1 = as1` (backfill), `sb2 = zs2` — read-2 keeps its ZS; same as Perl.
3. `lib.rs`: thread `config.aligner` into the **single** `check_results_paired_end` call in `drive_merge_pe` (L1231) — both single-core and `--multicore` reach it via `process_pe_chunk` (no `parallel.rs` edit). **Update the `run_pe`/PE merge test helper(s) to pass the new `aligner` param.**
4. **Verify-only** (no new code, assert in tests/gate): PE convert (R1 C→T / R2 G→A), PE option string (no dovetail, 2a-pinned), `_bismark_hisat2_pe*` naming, spliced-`N` PE extraction. **Add a PE+HISAT2 report-header unit test** (review I-2: only SE-HISAT2 + PE-Bowtie 2 exist today; the PE line-order + "run with HISAT2" branch is orthogonal but a 3-line test freezes it).
5. **HISAT2-aware PE fake** (named `hisat2`, banner `hisat2-align-s version 2.2.2`, via `--path_to_hisat2`): emit a PE pair where **mate-1 has `ZS:i:`** and assert the BAM record's MAPQ matches the read-1-ZS-ignored expectation (the gate can't reliably hit a mate-1-ZS read at 10k). Integration: PE directional → non-dir/pbat → FastA PE.
6. **🎯 PE oxy byte-identity gate** — `bismark_rs --hisat2` vs Perl `bismark --hisat2` + HISAT2 2.2.2, identical argv, decompressed SAM (`@PG` filtered) + `_PE_report.txt` (wall-clock filtered) + `--unmapped`/`--ambiguous`, **10k + 1M**, PE {directional, non-dir, pbat} + FastA PE {directional, non-dir}. Re-run the SE + Bowtie 2 gates (regression). Harness = the 2a `phase2a_hisat2_se_gate.sh` extended with PE cells (or a sibling `phase2b_hisat2_pe_gate.sh`).

## 6. Efficiency
A single conditional at one merge site; zero hot-path impact. Everything else is reuse.

## 7. Integration
Reads `.ht2` indexes; writes `_bismark_hisat2_pe*` PE BAM/report/aux. SE + Bowtie 2 branches byte-frozen. Consumes the 2a HISAT2 core. No `--multicore` PE for HISAT2 (rejected).

## 8. Assumptions
- **From epic:** oracle Perl v0.25.1 + HISAT2 2.2.2; decompressed-SAM gate; `@PG` aligner-independent; HISAT2 deterministic single-core (spike + 2a gate); indexes on oxy; Bowtie 2 byte-frozen.
- HISAT2 PE raw stream always carries `AS:i:`/`MD:Z:` on both mates (merge dies otherwise, Perl 3405) — confirm in the fake/gate.
- The PE strand-instance model (2 instances directional, 4 non-dir/pbat) + `--norc`/`--nofw` is identical for HISAT2 (Perl 6371–6376; 2a confirmed for SE; PE is the same table).
- `calc_mapq` PE form is aligner-agnostic + valid (2a confirmed); the only PE MAPQ risk is the read-1 second-best input — exactly what this plan fixes.
- `--multicore` + `--hisat2` is rejected (2a) → the PE gate is single-core; PE worker-invariance is not a HISAT2 concern.
- **(review M-1) The read-1 mask drops `second_best` UNCONDITIONALLY for HISAT2** — broader than Perl's read-1 `elsif XS:i:` capture, but exact for HISAT2 2.2.2: HISAT2 emits `ZS:i:` (secondary score) and `XS:A:` (spliced strand), **never `XS:i:`**, so read-1's captured second-best is always the `ZS` value and Perl's read-1 `XS:i:` branch never fires. The two would diverge only for a hypothetical HISAT2 that emits `XS:i:` on read-1; the 1M oxy gate (V8) is the backstop.

## 9. Validation
| # | Verify | How | Expect |
|---|---|---|---|
| V1 | SE + Bowtie 2 byte-frozen | full suite + SE oxy gate + Bowtie 2 PE-dovetail cell | unchanged |
| V2 | **(A) PE-HISAT2 mate-1 `ZS` + mate-2 `ZS`** | unit (`aligner=Hisat2`) | `sum_second = as1 + zs2` (read-1 ZS dropped), NOT `zs1+zs2` |
| V3 | **(B) PE-Bowtie 2 mate-1 second-best kept** | unit (regression, `aligner=Bowtie2`) | unchanged from today |
| V4 | **(C) PE-HISAT2 mate-1 `ZS` ONLY, mate-2 none** | unit | gate flips to **no-second-best** branch → MAPQ ladder switch (e.g. cap 39→42), NOT `Some(zs1+as2)` |
| V4b | **(D) PE-HISAT2 mate-1 none, mate-2 `ZS`** | unit | `sb1=as1` backfill, `sb2=zs2` |
| V5 | PE option string | unit (2a-pinned) | no `--dovetail`; softclip last |
| V6 | naming/report PE | integration + **PE+HISAT2 report-header unit test** | `_bismark_hisat2_pe*` + "run with HISAT2" (PE line order) |
| V7 | spliced-`N` PE extraction | fake + oxy | XM/genomic-seq byte-equal per mate |
| V8 | 🎯 PE oxy gate | Perl `--hisat2` vs Rust, 10k+1M, PE dir/non-dir/pbat + FastA PE dir/non-dir **+ a single-core `--ambig_bam` PE cell (dir, 1M)** | byte-identical |

## 10. Questions / ambiguities
- **OQ-2b-1 (Open):** pass `Aligner` vs a `bool` to `check_results_paired_end`? *Assumption:* `Aligner` (clarity + minimap2-ready). Non-critical (either is a one-site gate).
- **OQ-2b-2 (RESOLVED):** mask at `merge.rs:598` (merge-entry) — **both 2b reviewers confirmed an exhaustive grep shows L598 is the ONLY PE-path read of `r1/r2.second_best`**, so it is the single sufficient chokepoint (no separate ambiguity/MAPQ/`amb_same_thread` site escapes it). Per-mate parse flag rejected (larger blast radius).
- **OQ-2b-3 (Open):** does the 10k PE gate reliably contain a mate-1-`ZS` read? Likely at 1M, not guaranteed at 10k → the **V2 unit test is the reliable catch**, not the gate (mirrors 2a's reasoning). The 1M PE gate is the at-scale confirmation.

## 11. Self-Review
- **Logic:** the fix is one conditional at the exact divergence site (`merge.rs:598`), source-cited to the Perl read-1/read-2 asymmetry; it cannot affect SE (different function) or Bowtie 2 (`aligner != Hisat2`). The backfill already in place (`.or(Some(as1))`) does the rest, matching Perl's undef→AS path.
- **Edge cases:** mate-1-ZS (the bug), mate-1-no-tag, both-no-tag, Bowtie 2 regression, spliced-N PE, pbat (no FastA-pbat), non-dir 4-instance — all in V2–V8.
- **Validation:** V2 is a hard unit assertion on the MAPQ *input* (the byte-visible consequence flows through the verbatim `calc_mapq`); the gate (V8) is the at-scale proof. V1 guards the frozen SE/Bowtie 2 paths.
- **Risks:** low — the PE machinery is proven (Phase 7/8/9a) and the HISAT2 deltas are proven (2a). The single new conditional is well-localized. The only residual is whether HISAT2 PE has other latent asymmetries beyond read-1 `ZS`; the dual review found only this one, and the PE gate would surface any other.

## Implementation Notes (2026-06-05)

**Status:** implemented on `rust/aligner-v1x` (on top of the 2a commit `376a6d9`); **277 tests green** (233 lib + 44 integration), `clippy --all-targets -D warnings` + `cargo fmt --check` clean. The 🎯 PE oxy gate (V8) is the next step.

### What was built
- **`merge.rs`** — `check_results_paired_end` gained an `aligner: Aligner` param; at the second-best site, `r1_second_best = if aligner == Aligner::Hisat2 { None } else { r1.second_best }` (the existing `.or(Some(as1))` backfill then sets `sb1=as1`, reproducing Perl's undef→AS). +4 mate-tag unit tests (A −6, B −12, C None/demoted + Bowtie 2 contrast −6, D −6) via a new `run_pe_aln(..., aligner)` helper; `run_pe` delegates with `Aligner::Bowtie2`. The discriminator is the `aligner` ARG (parser unifies XS/ZS), so `mapped_pair` with `Some(v)` stands in for HISAT2's `ZS` (no `mapped_pair_zs` helper needed — A's clarification).
- **`lib.rs`** — threaded `config.aligner` into the **single** `check_results_paired_end` call in `drive_merge_pe` (both single-core and `--multicore` reach it via `process_pe_chunk`; **no `parallel.rs` edit** — the rev-1 correction).
- **`report.rs`** — +1 test `pe_header_hisat2_run_with_line` (PE+HISAT2 header, "run with HISAT2", PE line-order, no `--dovetail`).
- **`tests/cli.rs`** — `make_fake_hisat2_pe` (mate-1 carries `ZS:i:` → exercises the mask) + `hisat2_pe_mapped_names_and_report` (asserts `_bismark_hisat2_pe.bam`, 2 records, "run with HISAT2", PE option string no-dovetail).

### Iteration log
- **#1** mask + signature + wiring + 6 tests; merge tests green first try (A/B/C/D all as predicted).
- **#2** `cargo fmt` rewrapped the hand-written multi-arg calls (cosmetic).
- **#3** clippy `cloned_ref_to_slice_refs`: Case C's `&[pair.clone()]` → `std::slice::from_ref(&pair)`.
- **#4** dual code-review + plan-manager (all APPROVE/COMPLETE). Folded: M-1 (§8 unconditional-mask note + code comment — HISAT2 emits `ZS:i:`/`XS:A:`, never `XS:i:`), L-1 (PE integration MAPQ assertion, pinned 38), L-2 (deleted stray test-output files), emoji removed.
- **#5 🔴 PE oxy gate found a SEPARATE bug (the `ZS` mask was correct).** `pe_dir`/`pe_nondir` failed on a 12-line TLEN-sign diff for **same-POS fully-overlapping FR pairs with read-1 reverse (FLAG 83)** (e.g. read .1175, 60M/60M at chr7:81287727). Root cause: Perl's `$dovetail` *variable* (8047) is `!no_dovetail` for ALL aligners; the `if($bowtie2)` only gates the option PUSH. Rust derived the PE TLEN `dovetail` from `aligner_options.contains("--dovetail")` → `false` for HISAT2 (flag suppressed in 2a) → flipped TLEN. **Fix:** `RunConfig.dovetail = !cli.no_dovetail`; `lib.rs` uses `config.dovetail` (Bowtie 2 no-op). Regression guard: `output.rs::pe_tlen_tree` index-3 same-POS cases. Neither the plan nor the dual review caught it (TLEN was assumed aligner-agnostic); the gate did.

### 🎯 V8 PE oxy byte-identity gate — DONE + PASSED (2026-06-05)
Full record in `GATE_OXY.md` + harness `phase2b_hisat2_pe_gate.sh`. `bismark_rs --hisat2` byte-identical to Perl v0.25.1 + HISAT2 2.2.2, single-core, real GRCh38 PE, decompressed SAM (`@PG`-filtered) + `_PE_report.txt` (wall-clock-filtered) + decompressed aux:
- **10k — all 6 cells:** pe_dir 16,032 / pe_nondir 16,034 / pe_pbat 24 / pe_fasta_dir 16,034 / pe_fasta_nondir 16,036; pe_ambig_dir main 16,032 + ambig 1,780 + 4 aux.
- **1M:** pe_dir **1,620,342** / pe_nondir **1,620,302**; pe_ambig_dir main 1,620,342 + **ambig 173,420** (the raw-aligner-passthrough path, review B's Critical) + 4 aux — all byte-identical.

### Audit results
- Dual `/code-reviewer` (A + B) **APPROVE** (0 Critical/High); `/plan-manager` **COMPLETE**. Reports: `CODE_REVIEW_{A,B}.md`, `COVERAGE.md`. The TLEN `dovetail` bug was outside the reviewed surface (the gate caught it).

### NOT done here (next phases)
- minimap2 (Phases 3–4) + the combined v1.x full-scale gate (Phase 5).

## Revision History
- **rev 1 (2026-06-05):** focused dual plan-review (`PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md`, both APPROVE). Folded: **wiring corrected** — ONE call site `drive_merge_pe` (lib.rs:1231), no `parallel.rs` edit (both reviewers); **Case-C unit test added** (mate-1-`ZS`-only flips the gate to the no-second-best MAPQ ladder, byte-visible — A I-1/B); **`--ambig_bam` PE gate cell added** (B **Critical** — the only raw-aligner-passthrough PE path, never gated for HISAT2); discriminator clarified as the `aligner` arg not the tag (parser unifies XS/ZS — A); `run_pe` test-helper threading noted (B); FastA-pbat die citation 8155→**8156** (B). Both reviewers independently confirmed the `merge.rs:598` mask is the single sufficient chokepoint and found NO PE-HISAT2 divergence beyond the read-1 `ZS` family.
- **rev 0 (2026-06-05):** initial 2b plan (split from the combined Phase 2; the SE core shipped as 2a/PR #949). The read-1 `ZS` asymmetry (dual review B-L1) is the sole new logic; the rest is reuse + the PE gate. Multicore PE excluded (HISAT2 multicore rejected in 2a).
