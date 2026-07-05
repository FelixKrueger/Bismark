# Code Review B — Phase 4 (N-way lockstep merge + scoring + MAPQ)

**Reviewer:** B (independent, fresh context — audit only, no code modified)
**Date:** 2026-06-01
**Scope:** `mapq.rs`, `merge.rs`, `align.rs` (`SamStream`), `lib.rs` (driver), `config.rs`/`options.rs` (`score_min_*`), `tests/cli.rs`
**Gate:** byte-identical *decompressed* SAM content vs Perl Bismark v0.25.1 → faithfulness is paramount.
**Grounding:** Perl `bismark` `calc_mapq` (3923–4180), `check_results_single_end` (2702–3151), driver `process_single_end_fastQ_file…` (2413–2466), `single_end_align_fragments…bowtie2` (6849–6912), `reset_counters_and_fhs` (7113–7243).

## Verdict

**APPROVE — no Critical/High issues found.** The MAPQ ladder is transcribed leaf-for-leaf correct (every integer, every threshold, including the 0.84/0.68 ⟷ 0.88/0.67 shift). The merge logic faithfully reproduces the overwrite / best_AS / amb_same_thread / chr:pos-dedup / 3075 second-best / unique-best sort+boot / directional rejection / flag-4 advance+die / discard-until-id-change behavior. The driver wires the lockstep key, skip/upto, and child-process lifecycle correctly with no deadlock or zombie. Phase-5 per-strand counters are correctly absent. `cargo test -p bismark-aligner` = **67 unit + 15 CLI pass**; `cargo clippy --all-targets -- -D warnings` = **clean**.

Findings below are all Medium/Low (test-coverage hardening + faithful-replica documentation), none blocking.

---

## 1. `calc_mapq` transcription — VERIFIED CORRECT (focus #1)

Compared `mapq.rs` 22–115 against Perl 3932–4076, **line by line**:

- `scMin = intercept + slope·readLen` (mapq 22) = Perl 3932 (end-to-end; `--local` rejected, so `log` branch is dead and correctly omitted). PE read-2 addend (24) = Perl 3934–36.
- `diff = scMin.abs()` (26) = Perl 3938; `bestOver = AS_best − scMin` (27) = Perl 3939.
- **No-second-best ladder** (31–45): `0.8→42, 0.7→40, 0.6→24, 0.5→23, 0.4→8, 0.3→3, else→0` = Perl 3948–54 **exactly**.
- **With-second-best** `bestDiff = (|AS_best| − |AS_sec|).abs()` (49) = Perl 3957.
- Top buckets (50–57): `0.9→39/33, 0.8→38/27, 0.7→37/26, 0.6→36/22` = Perl 3958–89.
- **Inner threshold shift verified** (the flagged silent-divergence risk):
  - bucket 0.5 (58–67): `==diff→35, ≥0.84→25, ≥0.68→16, else→5` = Perl 3990–4002 ✓
  - bucket 0.4 (68–77): `34 / ≥0.84→21 / ≥0.68→14 / 4` = Perl 4004–4016 ✓
  - bucket 0.3 (78–87): `32 / ≥0.88→18 / ≥0.67→15 / 3` = Perl 4018–4030 ✓
  - bucket 0.2 (88–97): `31 / ≥0.88→17 / ≥0.67→11 / 0` = Perl 4032–4044 ✓
  - bucket 0.1 (98–107): `30 / ≥0.88→12 / ≥0.67→7 / 0` = Perl 4046–4058 ✓
  - `bestDiff > 0` (108–109): `≥0.67→6 / 2` = Perl 4060–66 ✓
  - final else (110–113): `≥0.67→1 / 0` = Perl 4068–74 ✓

  The 0.5/0.4 buckets use **0.84/0.68**, the 0.3/0.2/0.1 buckets use **0.88/0.67** — transcribed exactly. No wrong integer anywhere.
- The `==diff` vs `≥0.84/≥0.68` split is correct in every bucket; the no-second-best vs with-second-best split (the `let Some(sec) = … else` at 29) matches Perl's `if (!defined $AS_secBest) … else …`.
- Exact `f64` `==`/`>=` (not epsilon): intentional and documented (mapq 5–7, `#[allow(clippy::float_cmp)]` 12). Reviewer A's bit-identical-`f64` finding is plausible; the comparisons mirror Perl's `$bestOver == $diff` literally, which is the correct choice for byte parity. **(See Medium-1: this premise should be pinned by the real-data gate.)**

## 2. Merge faithfulness (`merge.rs` vs 2702–3151) — VERIFIED CORRECT (focus #2)

- **overwrite / best_AS / amb_same_thread** (144–167): the Perl "moved-down" ordering (2828 `$best_AS_so_far = …` placed *after* the 2818–2820 `amb_same_thread = 0` reset) is reproduced — Rust sets `best_as_so_far` at 156 after the `amb_same_thread = false` reset at 154. The downstream `best_as_so_far == Some(alignment_score)` check (165) therefore reads the just-updated value, exactly as Perl 2847 reads the updated `$best_AS_so_far`. `>=` keeps equally-good alignments (151), strict `>` resets amb (153). First-alignment branch (147–149) matches Perl 2802–04 (no amb touch). ✓
- **chr:pos dedup** (`insert_alignment` 276): key = `"{chromosome}:{pos}"` (de-converted chr) = Perl 2877 `join(":",$chromosome,$position)`; `HashMap::insert` overwrites the same key = Perl's same-location overwrite (2877–2894 comment). ✓ Unit test `same_location_in_both_instances_dedups` exercises this.
- **3075 second-best conditional** (224–227): `best.second_best if > runner_up else runner_up` = Perl 3075–80 exactly. The single-entry case (210–213) copies the stored `second_best` = Perl 3033–43. ✓
- **unique-best sort + tie-boot** (214–228): descending sort by AS (`Reverse`), `entries[0].AS == entries[1].AS → Ambiguous` (216) = Perl 3060–63; else best = entries[0], runner-up = entries[1].AS = Perl 3051–82 (examines only the first two). Determinism holds despite non-deterministic `HashMap::into_values()` order: only `entries[0]` (strictly the unique max) and the *AS value* of `entries[1]` are consumed; when 3+ entries exist with `entries[1].AS == entries[2].AS`, the value used is identical regardless of order, and `best` is always the strict max. **No HashMap-order divergence.** ✓
- **branch boundaries** (210/214/229): `==1`, `2..=4`, else die = Perl 3033/3048/3086. `len==0` is unreachable (guarded by `alignments.is_empty()` → NoAlignment at 203). ✓
- **directional index-2/3 rejection** (237–240) = Perl 3112–18 (inert on SE-directional where only indices 0/1 exist; covered by a 4-instance unit test). ✓
- **flag==4 advance + die** (104–112): advance once, die if next qname == identifier = Perl 2739–58; `continue` to next instance; EOF (current None) handled = Perl 2752–57. ✓ Unit test `flag4_then_same_id_dies`.
- **discard-until-qname-changes** (192–194): hoisted out of all three Perl branches (2858/2897/2936) — behavior-preserving since the loop body is identical in each. Terminates at EOF (`current()` None) and at qname change. No infinite loop. ✓
- **de-conversion + AS/MD die** (115–139): strip `_CT_converted`/`_GA_converted`, else die (2763–68); `AS`/`MD` mandatory on a mapped record (128–139) = Perl 2838. ✓ Unit tests `missing_converted_suffix_errors`; the lenient parse (align.rs) defers presence-enforcement to the merge, matching Perl die at 2838.
- **Strand counters absent (Phase 5):** `Counters` (53–66) holds only `sequences_count`, `unique_best_alignment_count`, `unsuitable_sequence_count`, `no_single_alignment_found`, `alignments_rejected_count`. The per-strand `CT_CT_count`/`CT_GA_count`/… (Perl 7113–7120) and `seen`/`wrong_strand` are **correctly NOT** incremented in Phase 4. ✓ Confirmed against `reset_counters_and_fhs`.

## 3. Lockstep key (focus #3) — VERIFIED CORRECT

Driver `drive_merge` (lib.rs 189–193): `identifier = fix_id(chomp(header))` then strip leading `@` = Perl 2420–21 (`chomp`+`fix_IDs`) + 2442 (`s/^\@//`). `sequence = uc(chomp(seq))` (193) = Perl 2444 `uc$sequence`. Crucially, the converted FastQ header that Phase 2 writes (convert.rs 197–198, `fix_id(chomp(id))`+`\n`, `@` preserved) is what Bowtie 2 emits as QNAME (minus `@`), so the driver-derived key matches the stream QNAME **by construction** — both derive from the same `fix_id(chomp())`. skip/upto/icpc come from the same `config.read_processing` that `ConvertOptions::from_config` copied into Phase 2, so the driver and the converted file process an identical record set (verified: convert and driver both `count += 1` per raw record, then `count <= skip` continue / `count > upto` break — identical operators; the convert-only max-length guard is inert because `--mm2_maximum_length` is hard-rejected at resolve() on the Bowtie 2 spine). **Lockstep integrity holds for skip/upto.** ✓

## 4. Driver / child-process (focus #4) — VERIFIED CORRECT

- **2 streams on the C→T file** (lib.rs 112–127): index 0 = `Norc`+`ct_index_basename` (CTreadCTgenome), index 1 = `Nofw`+`ga_index_basename` (CTreadGAgenome). Verified against Perl 6873–77 (`CTreadCTgenome|GAreadGAgenome → --norc`, else `--nofw`), index assignment 7155–67, and `inputfile` 500 (`$fhs[0]=$fhs[1]=$C_to_T_infile` — **both** instances read the C→T file). ✓
- **finish() drain+wait** (align.rs 241–252): drains stdout to sink *then* `wait()` — avoids deadlocking a child blocked on a full pipe during `--upto` early-stop. Validated by `early_stop_does_not_deadlock_or_zombie` (5000 records > 64 KiB pipe). ✓
- **No zombie on error path:** if `drive_merge` returns `Err`, the `for s in streams { s.finish()? }` is skipped and the streams are dropped; `Drop` (255–262) does `kill()` *then* `wait()`. ✓
- **streams-in-lockstep assumption:** a stream whose current qname ≠ identifier (or EOF) is skipped via `is_none_or` (merge 97 `continue`) = Perl 2730/2735 silent skip. `current().unwrap()` at 100 is safe (guarded). ✓

## 5. Tests (focus #5)

`merge` units assert real behavior across: unique-best (one mapped + one unmapped), best-across-instances-by-AS (+ runner-up second-best), cross-instance tie → Ambiguous, same-thread amb (AS==XS) → boot, same-location dedup, both-unmapped → NoAlignment, missing-suffix error, flag4+same-id die, directional index-2 rejection (4-instance). `mapq` units cover the full no-second-best ladder, top with-second-best buckets (39/38/35), not-at-diff (1/26/33), non-integer scMin, and a user score_min slope. `align` units cover the live fake-bowtie2 stream (header skip, EOF, nonzero exit, early-stop drain). `cli.rs` fake bowtie2 now reads `-U` and emits one flag-4 record per converted read, exercising the lockstep + flag-4 path end-to-end (asserts "Phase 4 merge summary" + "no alignment found:"). **All assert real outcomes, not smoke.**

---

## Issues & Recommendations

### Medium

- **M-1 (test gap — the exact branches the prompt flagged as the silent-divergence risk are UNTESTED).** No `mapq` unit hits any inner-threshold leaf: the 25/16/5 (bucket-0.5), 21/14/4 (0.4), 18/15/3 (0.3), 17/11 (0.2), 12/7 (0.1), or 6/2 leaves. I verified these are all byte-correct by hand, but they are precisely where a future edit (e.g., a fat-fingered 0.84→0.88) would slip through green tests. **Recommend** adding ~6 targeted asserts that land in each inner bucket — e.g. with `diff=10`: a case where `bestDiff∈[5,6)` and `bestOver∈[6.8,8.4)` → 16, and one with `bestOver∈[8.8,…)` in a 0.3-bucket → 18 — so every distinct integer in the ladder is asserted at least once. Low effort, high regression value; the ladder is the byte-identity crux.

- **M-2 (no test for `AS_secBest` present but worse than runner-up — the 3075 `>` branch).** `best_across_instances_by_score` exercises the `else` arm of 3075 (uses runner-up AS), but no unit covers the arm where `best.second_best > runner_up` so the *stored* second-best is kept. **Recommend** one merge unit: two instances, best instance carries `XS:i:` strictly greater than the other instance's AS, and assert `alignment_score_second_best` equals the XS (not the runner-up). Closes the 3075 conditional both ways.

### Low

- **L-1 (faithful-replica documentation — `finish()` fail-closed on non-zero Bowtie 2 exit).** align.rs 241–252 returns `Err` on a non-zero child exit; Perl closes the pipe fail-open (no status check). This is an *intentional, documented* hardening (align.rs 238–240), and on the byte-identity gate a non-zero Bowtie 2 exit means a failed run anyway — but it is a behavioral deviation from Perl. Already noted in code; no action needed beyond keeping the deviation list current for the Phase-5/real-data gate sign-off.

- **L-2 (latent lockstep coupling for future phases).** Driver/convert lockstep depends on the convert-side max-length guard staying inert (it drops records the driver would keep). Safe today (`--mm2_maximum_length` hard-rejected at resolve on the Bowtie 2 spine). **Recommend** a one-line comment in `drive_merge` (or a debug-assert that `maximum_length_cutoff.is_none()`) so the PE/minimap2 phases don't silently desync the driver from the converted file.

- **L-3 (CRLF in QNAME — pre-existing, not a Phase-4 regression).** `chomp_newline` keeps a trailing `\r`, so a CRLF FastQ yields a header `@…\r` in the converted file and an identifier `…\r` in the driver. Driver and converted file are mutually consistent (same `fix_id(chomp())`), but whether Bowtie 2 strips the `\r` from QNAME determines parity — the same exposure Perl has. Flag for the real-data gate to include a CRLF input; no Phase-4 code change.

- **L-4 (`bestDiff` integer subtraction).** mapq 49 computes `(as_best.abs() − sec.abs()).abs() as i64→f64`; AS magnitudes are tiny so no overflow, and the result equals Perl's float `abs(abs−abs)`. Fine as-is; noted only for completeness.

## Cross-check note for the caller
Reviewer A independently verified the bit-identical-`f64` premise behind the exact float comparisons (mapq 5–7). I concur the `==`/`>=` choice is correct for byte parity; I additionally recommend the inner-bucket asserts (M-1) so that premise is *exercised*, not just asserted in prose.

**Report path:** `/Users/fkrueger/Github/Bismark-aligner/plans/05312026_bismark-aligner/phase4-nway-merge-scoring/CODE_REVIEW_B.md`
