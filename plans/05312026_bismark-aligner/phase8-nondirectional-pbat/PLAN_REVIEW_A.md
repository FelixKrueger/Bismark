# PLAN_REVIEW_A — Phase 8: Non-directional + pbat (SE + PE, FastQ)

**Reviewer:** A (independent, fresh context)
**Plan:** `plans/05312026_bismark-aligner/phase8-nondirectional-pbat/PLAN.md` (rev 0)
**Verdict:** APPROVE-WITH-FINDINGS — 0 Critical, 4 Important, 5 Optional

I verified every load-bearing claim against the Perl source (`bismark` v0.25.1) and
the existing Rust (`rust/bismark-aligner/src/`). The plan's central thesis — that
Phase 8 is *wiring* (conversion variants + the per-mode driver instance plan), with
the merge/extraction/FLAG/report machinery already mode-agnostic — **holds up under
source verification**. The instance tables (§3.2, §3.3) are correct in every slot/
index/orientation/file cell. The findings below are about coverage gaps and one
genuinely-first-time-exercised code path, not about wrong wiring.

---

## 1. Logic review (verified against source)

### 1.1 The two per-mode instance tables — VERIFIED CORRECT

**SE (§3.2)** — cross-checked against Perl `@fhs` templates (`reset_counters_and_fhs`
7124–7244), the SE input-assignment (519–546), and the `--norc`/`--nofw` name rule
(`single_end_align_…_bowtie2` 6871–6878):

- The orientation in Perl is decided by `$fh->{name}` (rule 6873: name
  `CTreadCTgenome` **or** `GAreadGAgenome` → `--norc`, else `--nofw`), NOT by slot.
  The plan's table got every cell right by tracing the per-mode `@fhs` *names*:
  - **directional** (7153–7167): s0 `CTreadCTgenome`/CT/`--norc`, s1 `CTreadGAgenome`/GA/`--nofw`, both read C→T (544/526). ✓
  - **pbat** (7199–7214): s0 `GAreadCTgenome`/CT/**`--nofw`**, s1 `GAreadGAgenome`/GA/**`--norc`**, both read G→A (535). ✓ — note the orientation *flips* vs directional; the plan captures this.
  - **non-dir** (7216–7242): s0 CT/`--norc`/C→T, s1 GA/`--nofw`/C→T, s2 CT/`--nofw`/G→A, s3 GA/`--norc`/G→A (544–545 assign s0,s1=C→T; s2,s3=G→A). ✓

**PE (§3.3)** — cross-checked against the PE name assignment (295–298), the PE input
assignment (405–451), and the PE name rule (6466–6471):

- PE `@fhs` names (295–298): s0 `CTread1GAread2CTgenome`, s1 `GAread1CTread2GAgenome`,
  s2 `GAread1CTread2CTgenome`, s3 `CTread1GAread2GAgenome`. Rule 6466: name
  `CTread1GAread2CTgenome` **or** `GAread1CTread2GAgenome` → `--norc` ⇒ s0,s1=`--norc`;
  s2,s3=`--nofw`. The plan says "s0,s1=`--norc`, s2,s3=`--nofw`". ✓
- Per-slot index basename in the PE templates (7126–7151 / 7170–7214 / 7216–7242):
  order is CT, GA, CT, GA ⇒ s0=CT, s1=GA, s2=CT, s3=GA. ✓
- Per-mode `-1`/`-2` (Perl 405–451):
  - **directional**: s0 = C→T_1/G→A_2, s3 = C→T_1/G→A_2 (s1,s2 undef). ✓
  - **pbat**: s1 = G→A_1/C→T_2, s2 = G→A_1/C→T_2 (s0,s3 undef). ✓
  - **non-dir**: s0 C→T_1/G→A_2, s1 G→A_1/C→T_2, s2 G→A_1/C→T_2, s3 C→T_1/G→A_2. ✓ (matches the plan cell-for-cell)

### 1.2 Conversion variants (§3.1) — VERIFIED CORRECT

- **SE pbat** = G→A only, no ID suffix (Perl 5523–5539 + 5618–5622; returns
  `$G_to_A_infile`, 5643). ✓
- **SE non-dir** = C→T **and** G→A, no suffix (5540–5573 + 5623–5632). ✓
- **PE pbat** = R1 G→A `/1/1`, R2 C→T `/2/2` (5854–5876 opens; 5945–5960 tags by
  `$read_number` regardless of mode; 5965–5973 transforms). ✓ — the `/1/1`/`/2/2` tag
  is mode-independent; only the `tr` direction flips. The plan's note "the mirror of
  directional's R1 C→T / R2 G→A" is exactly right.
- **PE non-dir** = each mate → BOTH C→T and G→A → 4 temp files (5901–5912 +
  5986–5990). ✓

The existing `convert_fastq_impl(input, temp_dir, opts, kind, id_suffix, file_base)`
(`convert.rs:198`) already parameterises kind/suffix/stem, so the new entries are
thin wrappers (the plan's §4 shape is sound). `convert_seq_g_to_a` already exists
(`convert.rs:149`). ✓

### 1.3 "Already built / verify-only" claims — INDEPENDENTLY CONFIRMED

| Claim | Verified at | Confirmed |
|---|---|---|
| (a) SE reject gated on `directional` (idx 2/3) | `merge.rs:311` `if directional && (best.index == 2 \|\| 3)` vs Perl 3112–3118 | ✓ — non-dir/pbat (directional=false) keep all spawned strands |
| (a) PE reject gated on `directional` (idx 1/2) | `merge.rs:661` `if directional && (best.index == 1 \|\| 2)` vs Perl 3851–3856 | ✓ |
| (b) SE `pbat_mod` (+2) maps slots 0/1 → eff 2/3 | `methylation.rs:120–121` `eff = best.index + pbat_mod` vs Perl 4308–4311; strand/conv table 131–141; counters 210–216 | ✓ — eff 2→`ga_ct_count`, eff 3→`ga_ga_count`; CTOT/CTOB |
| (c) report library line (SE vs PE pbat wording) | `report.rs:83–98` vs Perl SE 1715 ("(OT and OB) strands") / PE 1944 ("(OT, OB)") | ✓ — byte-exact, both forms present |
| (c) rejected-count line gated on `directional` | `report.rs:151` (SE) / `report.rs:223` (PE) `if directional` vs Perl 2046–2049 / 2221–2224 | ✓ — omitted for non-dir/pbat |
| (d) CLI/config conflict dies | `config.rs:276–300` (non_dir⊕pbat, pbat⊕gzip, pbat⊕fasta) vs Perl 8148–8156 | ✓ — messages byte-match |
| FLAG tables cover all 4 strands | SE `output.rs:356–369` (all 4 (strand,conv,conv) arms) / PE `output.rs:469–480` (idx 0/1/2/3) vs Perl 8521–8546 / 8825–8868 | ✓ — no library branch; pure index function |
| merge iterates N streams; counters keyed on index | `merge.rs` SE `enumerate`, PE `SCAN_ORDER [0,3,1,2]` | ✓ |

### 1.4 Completeness — no missed mode-specific behavior found, with ONE caveat

I grepped `output.rs`, `align.rs`, `methylation.rs`, `aux_out.rs` for any
`directional`/`pbat`/`non_directional`/`library` branch. The ONLY library-coupled
logic outside the driver is:
- the merge reject (already gated, §1.3a),
- the report library line + rejected line (already gated, §1.3c),
- the SE `pbat_mod` (already wired, §1.3b),
- the conflict dies (already wired, §1.3d).

**No** mode branch exists in the @SQ header, the genomic extraction strand dispatch
(it is purely index/eff-keyed), the SAM FLAG/XR/XG, the TLEN/dovetail logic, or the
aux FastQ routing. The plan's claim that pbat "changes only which instances spawn (+
SE +2 modifier)" is **borne out** by the source.

**CAVEAT (→ Important #1):** the `methylation_call` **GA branch** (`methylation.rs:586–603`,
Perl 4913–4998) is, by the code's own comment, "ported for Phase 8, inert here." For
SE-directional it is never reached (read_conversion is always Ct). pbat SE (eff 2/3)
and non-dir (s2/s3) are the **first** code paths that route `Conversion::Ga` into the
methylation call. The same is true of the SE FLAG arms `(b'-', Ga, Ct)` / `(b'+', Ga,
Ga)` and the PE `best.index == 1 || 2` arms — all present but exercised for the FIRST
time in Phase 8. This is not a wiring gap, but it means the byte-identity risk in
Phase 8 is concentrated in code that has never met real Perl output. See Validation.

---

## 2. Assumptions

- **§8 "gate reuses 10M_SE/10M_PE with the mode flags":** SOUND for the byte-identity
  *contract* (same reads + same mode through both tools must agree byte-for-byte).
  But a directional library fed `--non_directional`/`--pbat` will land **few or zero**
  reads on the complementary strands (s2/s3 for non-dir; the G→A-only instances for
  pbat will mostly fail to map a directional library). So the gate may pass while the
  GA methylation branch + the CTOT/CTOB FLAG arms see **near-zero** coverage — exactly
  the first-time-exercised paths (§1.4 caveat). Open Q3 acknowledges this ("a native
  non-dir/pbat library would exercise more strands") but defers it as "optional." I
  disagree on the risk weighting — see Important #2.
- **Open Q4 (conversion entry-point shape):** the "small explicit entry points"
  assumption is fine and matches the existing SE/PE directional style. No concern.
- **Open Q1 (generalize in place):** reasonable; the directional path is frozen by the
  existing gated tests + PR-#930 CI. The risk is mechanical (see Important #3).
- **Implicit assumption the plan does NOT state:** temp-file cleanup must enumerate the
  per-mode file *set*. The current `run_se_directional` deletes exactly one file
  (`lib.rs:225`), and `run_pe_directional` deletes exactly two (`lib.rs:623–624`).
  Non-dir SE = 2 files, PE pbat = 2 files, PE non-dir = 4 files. The plan mentions this
  in §3.5/§5.5 but it is easy to under-deliver. See Optional #1.

---

## 3. Efficiency

§6 is accurate: non-dir doubles instances (4 vs 2) and converted files; pbat is the
same cost as directional. No new genome passes. mimalloc is already global. Two notes:

- The genome is loaded **once per `run_se`/`run_pe`** (outside the read-file loop,
  `lib.rs:133/523`) — non-dir does not re-load it. Good; no regression.
- For non-dir SE, the 4 instances all read from 2 converted files (s0,s1←C→T;
  s2,s3←G→A). The plan correctly does NOT propose 4 separate converted files for SE
  (Perl makes 2). For PE non-dir, 4 files (C→T_1, G→A_1, C→T_2, G→A_2). No waste.

---

## 4. Validation sufficiency (§7)

Adequate in structure (conversion units, fake-bt2 integration per mode, report-byte
tests, the oxy gate). The fake-bt2 harness in `tests/cli.rs` already branches on the
`-x` index basename (`*BS_CT*`, line 77–80) and reads the `-U`/`-1`/`-2` file, so it
CAN faithfully drive a 4-instance non-dir run and a 2-instance pbat run with a hit on
the complementary strand. Gaps:

- **GA-branch methylation coverage (→ Important #1+#2).** §7 row 3 ("pbat … counts in
  ga_ct/ga_ga; FLAG/XR/XG per CTOT/CTOB") is the right idea, but the plan should make
  it a *byte-level* assertion of the emitted SAM record (FLAG, XR:Z:GA, XG:Z, the GA
  `XM` string) for a **synthetic read engineered to map on the complementary strand** —
  not merely "counts land in ga_ct." A pure counter check would pass even if the GA
  `XM` call or the minus-strand reorientation were subtly wrong.
- **PE pbat / non-dir SAM-record byte tests (→ Important #2).** §7 rows 4–5 assert
  "4 instances" / "strands CTOB/CTOT" but not the actual two-mate SAM records for
  index 1 and index 2 (the PE FLAG pairs `(163,83)` / `(147,99)`, the index-1/2 +2
  ref-trim swap at `output.rs:483–491`, and the dovetail sub-cases at 507/520). These
  are first-exercised in Phase 8 and deserve an integration byte assertion.
- **§7 row 9 names `pe_gate.sh`** — there is no such script in the repo (the Phase-7
  GATE_OXY.md references it as the oxy-side harness; the in-repo gate scripts are
  `scripts/byteid_run.sh` / `oxy_idle_gate.sh`). Not a logic flaw, but the plan should
  point at the real harness so the implementer doesn't hunt for a missing file. See
  Optional #2.
- **Regression guard for the generalization (§7 row 8)** = "existing suite, byte-frozen."
  The SE/PE directional integration + report-byte tests in `tests/cli.rs` + `report.rs`
  exist and are the right guard; the plan should name them explicitly so the implementer
  runs them as the acceptance bar for the refactor BEFORE adding new arms.

---

## 5. Alternatives considered

- **A driver-level `InstancePlan` struct** (a `Vec<(slot, Orientation, &Path index, files)>`
  built by a `match config.library`) is exactly what §4 sketches and is the right shape —
  it keeps the convert→spawn→merge→cleanup loop identical across modes and confines the
  mode logic to one table. Recommend the plan commit to this (rather than three parallel
  branches inside `run_se`), because it makes the directional path a *data* row, not a
  forked code path, which is the strongest defence of byte-identity (Important #3).
- **Deferring pbat to a separate phase from non-dir** — rejected, correctly: they share
  the same machinery and both flip the same booleans; splitting would duplicate the
  driver refactor.

---

## 6. Action items

### Critical
*(none — the wiring tables and the verify-only claims are all source-confirmed.)*

### Important
1. **Call out that the GA methylation branch + CTOT/CTOB FLAG arms are FIRST exercised
   in Phase 8.** Add a `methylation_call` unit test that feeds a `Conversion::Ga` read
   against a known genomic window and asserts the exact `XM` string + the 8 context
   counters (not just "counts land in ga_ct"). The branch is ported but has never met
   real input; a counter-only check (§7 row 3) would not catch a GA `XM`/reorientation
   bug. (`methylation.rs:586–603`, `output.rs:358–360`.)
2. **Add SAM-record byte assertions for the complementary strands**, SE (eff 2/3) and
   PE (index 1/2), via the fake-bt2 harness with a synthetic read engineered to map on
   the complementary strand — assert FLAG (`0`/`16` SE; `163,83`/`147,99` PE), `XR`/`XG`,
   and the index-1/2 +2-trim swap (`output.rs:483–491`) + dovetail sub-cases
   (`output.rs:507/520`). The 10M_SE/10M_PE×mode gate may land near-zero reads on these
   strands (a directional library mostly will not map G→A-only), so the gate alone is
   insufficient to exercise them (Open Q3's "optional" deeper run under-weights this).
   Either engineer a small synthetic complementary-strand dataset for the gate, or rely
   on these targeted integration tests as the primary coverage — but say which.
3. **Pin the directional regression bar explicitly.** Before adding the new arms, the
   `run_se_directional→run_se` / `run_pe_directional→run_pe` refactor must leave the
   named directional tests (`tests/cli.rs` mapped/edge/unmapped/ambiguous + PE; `report.rs`
   directional byte tests) green with zero diff. List them in the plan as the
   acceptance gate for step §5.2/§5.3, and prefer the `InstancePlan`-as-data shape so
   directional becomes one table row (Alternative §5).
4. **Specify per-mode temp-file cleanup explicitly as a checklist**, not a parenthetical
   (§3.5). Current code deletes 1 file (SE, `lib.rs:225`) / 2 files (PE, `lib.rs:623–624`).
   Phase 8 must delete: SE pbat=1 (G→A); SE non-dir=2 (C→T,G→A); PE pbat=2 (G→A_1,C→T_2);
   PE non-dir=4. Best-effort (Perl 1974–1999 / 2154–2181). An under-delivered cleanup
   leaves temp files but is byte-invisible, so no test/gate will catch it — make it a
   named task with an asserted post-condition (or at least a code-review checklist item).

### Optional
1. Add a (cheap) integration assertion that the expected temp files are GONE after a
   non-dir/pbat run, to close the byte-invisible cleanup gap from Important #4.
2. Fix the §7 row-9 reference: name the real gate harness (`scripts/byteid_run.sh` /
   `oxy_idle_gate.sh`) instead of `pe_gate.sh`, or note it is the oxy-side script from
   Phase-7 GATE_OXY.md.
3. §3.5 says "the 4 strand-count lines always print (unused strands stay 0)." Confirmed
   correct (`report.rs:147–149` SE / `report.rs:219–221` PE always print all four; the
   PE join order is 0,2,1,3 per Perl 2218). Worth a one-line report-byte test per mode
   (pbat SE: ga_ct/ga_ga populated, ct_ct/ct_ga = 0) to pin the per-mode byte shape, as
   §7 row 7 intends — make it concrete.
4. The plan's §3.1 edge note "pbat⊕`--gzip` is rejected at config" is correct
   (`config.rs:287`), but the shared `convert_fastq_impl` still has a gzip path; the new
   SE-G→A / PE-pbat entries should pass `opts` through unchanged (gzip will simply be
   false because config rejected it) — no special-casing needed. A one-line note avoids
   an implementer adding a redundant guard.
5. Consider a unit test for the new SE-G→A conversion entry asserting the byte output +
   the `_G_to_A.fastq` filename (mirrors the existing `pe_read2_g_to_a_*` test), so the
   SE pbat conversion is pinned independently of the integration path (§7 row 1 covers
   this — just make the SE-G→A case explicit, since the existing G→A test is PE-only).

---

## 7. Bottom line

The plan is accurate where it matters most: the two instance tables are correct in
every cell against the Perl, and the "already built" machinery (gated reject, SE +2
modifier, 3-way report, conflict dies, mode-agnostic FLAG/XR/XG) is genuinely in place
and mode-independent. The real risk is **coverage, not correctness of the plan**: a
family of code paths (GA methylation branch, CTOT/CTOB SE FLAGs, PE index-1/2 records)
becomes live for the first time in Phase 8, and the proposed validation leans on a gate
that may not exercise them on a directional library. Tighten the validation (Important
#1–#2), pin the directional regression bar (#3), and make temp cleanup a named task
(#4), and this is ready to implement.
