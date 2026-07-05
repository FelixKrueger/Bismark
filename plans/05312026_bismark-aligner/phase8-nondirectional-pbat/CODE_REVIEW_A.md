# CODE REVIEW A — Phase 8: Non-directional + pbat (SE + PE, FastQ)

**Reviewer:** Code Reviewer A (independent, fresh context)
**Scope:** working-tree diff vs `ca6af0a` (Phase 7) — `rust/bismark-aligner/{src/convert.rs, src/lib.rs, src/methylation.rs, src/output.rs, tests/cli.rs}`
**Worktree:** `/Users/fkrueger/Github/Bismark-aligner` (branch `rust/aligner`)
**Verdict:** **APPROVE** (1 Low-priority doc-drift nit; no functional findings)

---

## Summary

Phase 8 adds the non-directional and pbat library types for SE and PE FastQ. The plan thesis — that this is *mostly wiring* over machinery already built and verified (merge reject gate, SE `+2` pbat modifier, FLAG/XR/XG, 3-way report) — **holds under implementation**. The change is concentrated exactly where the plan said: conversion variants (`convert.rs`), per-mode instance plans (`lib.rs`), and 2 dispatch arms, plus the load-bearing GA-strand tests.

I independently cross-checked **every** slot/index/orientation/file cell of §3.2 (SE) and §3.3 (PE) against the Perl v0.25.1 source, re-derived the SE `+2` modifier extraction and the GA `methylation_call` branch by hand, and ran the full suite + clippy + fmt. **Everything is byte-faithful to Perl on the load-bearing paths**, the conversion dedup is correct, the directional path is byte-frozen, the load-bearing GA-strand fakes genuinely map (no all-unmapped false-pass), and the per-mode temp cleanup is complete and test-asserted.

### Build / verify (all green)
- `cargo test -p bismark-aligner` → **183 lib + 28 integration = 211 tests, 0 failed** (matches §12).
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → clean.
- `cargo fmt -p bismark-aligner -- --check` → clean (exit 0).

---

## Verification against the Perl source of truth

### Priority 1 — Per-mode instance plans (every cell vs Perl)

**SE** (`se_instance_plan`, `lib.rs:152`). Perl `@fhs` SE templates (7199–7244) + input assignment (519–546) + the `--norc`/`--nofw` name rule (6873: `CTreadCTgenome`|`GAreadGAgenome` → `--norc`, else `--nofw`):

| Mode | Perl @fhs (slot order) | orient | index | reads-file | Rust plan | Match |
|---|---|---|---|---|---|---|
| directional | CTreadCTgenome, CTreadGAgenome | norc, nofw | CT, GA | C→T, C→T | `[(Norc,Ct,0),(Nofw,Ga,0)]` | ✅ |
| pbat | GAreadCTgenome, GAreadGAgenome | nofw, norc | CT, GA | G→A, G→A | `[(Nofw,Ct,0),(Norc,Ga,0)]` | ✅ |
| non-dir | CTreadCTgenome, CTreadGAgenome, GAreadCTgenome, GAreadGAgenome | norc, nofw, nofw, norc | CT, GA, CT, GA | C→T, C→T, G→A, G→A | `[(Norc,Ct,0),(Nofw,Ga,0),(Nofw,Ct,1),(Norc,Ga,1)]` | ✅ |

The `enumerate` index in `run_se` equals the Perl `@fhs` index (streams pushed in slot order; `Vec::with_capacity(2)` does not constrain length — pbat=2, non-dir=4). ✅

**PE** (`pe_instance_plan`, `lib.rs:584`). Perl PE input assignment (394–452) + the unconditional name reassignment (295–298) + the PE name rule (6466: `CTread1GAread2CTgenome`|`GAread1CTread2GAgenome` → `--norc`):

I confirmed via `grep` that the PE `@fhs` names are reassigned at **295–298 unconditionally for all PE modes** (s0=`CTread1GAread2CTgenome`, s1=`GAread1CTread2GAgenome`, s2=`GAread1CTread2CTgenome`, s3=`CTread1GAread2GAgenome`) — so s0,s1 → `--norc`, s2,s3 → `--nofw`; per-slot genome index = CT,GA,CT,GA.

| Mode | Perl slots: -1/-2 | Rust plan `(slot,orient,idx,k1,k2)` | Match |
|---|---|---|---|
| directional | s0 C→T_1/G→A_2, s3 C→T_1/G→A_2 | `(0,Norc,Ct,Ct,Ga),(3,Nofw,Ga,Ct,Ga)` | ✅ |
| pbat | s1 G→A_1/C→T_2, s2 G→A_1/C→T_2 | `(1,Norc,Ga,Ga,Ct),(2,Nofw,Ct,Ga,Ct)` | ✅ |
| non-dir | s0 C→T_1/G→A_2, s1 G→A_1/C→T_2, s2 G→A_1/C→T_2, s3 C→T_1/G→A_2 | `(0,Norc,Ct,Ct,Ga),(1,Norc,Ga,Ga,Ct),(2,Nofw,Ct,Ga,Ct),(3,Nofw,Ga,Ct,Ga)` | ✅ |

Each stream is placed at its Bismark slot in the length-4 `Vec<Option<_>>` (initialised `[None;4]`). ✅ The merge scan order (0,3,1,2) is unchanged.

### Priority 2 — pbat SE `+2` routing

`run_se` derives `pbat = matches!(config.library, Pbat)` (`lib.rs:200`) and passes it to `drive_merge` (`lib.rs:270`), which forwards to `extract_corresponding_genomic_sequence_single_end(.., pbat, ..)`. The pbat stream Vec is **2 elements** (the plan returns 2 tuples), so physical slots 0/1, lifted to eff 2/3 by the extraction `+2` modifier (Perl 4308–4311) — **NOT** a 4-vec with `None` at 0/1 (the Opt-1 trap is avoided). ✅ I re-derived the `+2` extraction from Perl 4315–4322 (index 1/3: prepend `substr($chr,$pos-2,2)`) and 4388 (index 0/2: append + later revcomp); both unit tests (`extract_pbat_se_index0_eff2_ga_ct`, `extract_pbat_se_index1_eff3_ga_ga`) match Perl byte-for-byte.

### Priority 3 — PE conversion dedup + pbat inversion

The `needed`/`converted` dedup (`lib.rs:660–680`) collects distinct `(mate, kind)` from the plan: directional → 2 files (C→T_1, G→A_2), pbat → 2 (G→A_1, C→T_2), non-dir → 4 (all). Verified by trace; matches Perl 405–451. The library-aware core `bisulfite_convert_fastq_pe_kind(.., read_number, kind)` takes the substitution explicitly while the ID suffix (`pe_id_suffix`) comes from `read_number` — exactly matching Perl 5945–5990, where in non-dir BOTH the C→T and G→A R1 files carry the SAME `/1/1` (suffix applied to `$identifier` once, before both writes). The directional `bisulfite_convert_fastq_pe` **delegates** with the fixed R1=Ct/R2=Ga map. ✅ (rev1 B I-1 satisfied — no silent reuse of the directional read#→kind map.)

### Priority 4 — Directional byte-freeze

The directional SE/PE instance-plan rows reproduce the pre-Phase-7/8 wiring exactly; all pre-existing directional unit + integration tests stayed green through the `run_se`/`run_pe` generalization (the 211-test run includes them: e.g. `mapped_read_writes_bam_record_end_to_end`, `pe_mapped_writes_two_bam_records_end_to_end`, `unmapped_routing_and_report_end_to_end`). The directional SE banner substring "Created C->T converted" is preserved (asserted at `tests/cli.rs:172`). ✅

### Priority 5 — Load-bearing test fakes + hand-derived XM

The new fakes (`make_fake_bowtie2_ga_reads_{ct,ga}_index`, `make_fake_bowtie2_pe_{ga,ct}_index*`) emit a **mapped** record (FLAG 0/16 SE, 99/147 PE) only when the index matches AND the reads file is the `*_G_to_A*` one — i.e. they genuinely map on the `BS_GA`/G→A-reads strands. Each test asserts `"unique best alignments:   1"` in stderr (impossible on all-unmapped — the count would be 0) **and** reads back the written BAM record to byte-assert FLAG/POS/SEQ/XR/XG/XM. The exact all-unmapped false-pass trap both plan reviewers flagged is defeated on both counts.

I independently re-derived the GA `methylation_call` branch (Perl 4916–4998: compare `seq[i]` vs `genomic[i+2]`, look upstream at `genomic[i+1]` then `genomic[i]`):
- `methylation_call_ga_branch_contexts`: read `GCGTAC`, genomic `TTGCGTAC` → `H.Z...` (me_chh=1, me_cpg=1). ✅
- CTOB (eff3, FLAG 16, strand '+', no reorientation): XM `H.Z...`. ✅
- CTOT (eff2, FLAG 0, strand '-', SEQ revcomp `GTACGC`, XM reversed): forward call `H...z.` → reversed `.z...H`. ✅
All hand-asserted values verified against Perl and confirmed by the real engine in `output.rs`/`cli.rs` end-to-end.

### Priority 6 — Per-mode temp cleanup

SE (`lib.rs:289`) and PE (`lib.rs:760`) both iterate `&converted` and `remove_file` every temp: SE pbat=1 / non-dir=2; PE pbat=2 / non-dir=4. Each integration test asserts the exact temp set is gone (`pbat_se_*` 1 file; `nondir_se_*` 2; `pbat_pe_*` 2; `nondir_pe_*` 4). Byte-invisible but test-covered. ✅

### Priority 7 — Correctness / errors / structure

- **Reject gate uses RAW index, gated on `directional`** (Perl 3112–3118 / 3851–3857; Rust `merge.rs:311`, `merge.rs:661`). The `+2` modifier is applied only in extraction (after the reject decision), so there is no ordering hazard. For pbat/non-dir `directional=false` → no reject; the `nondir_se_four_instances_ctot_no_rejection` and `nondir_pe_four_slots_index1_no_rejection` tests confirm a record on index 2/1 is KEPT. ✅
- **Report gating** (verify-only, unchanged): rejected-count line gated on `directional` (`report.rs:151,223`); covered by `non_directional_omits_rejected_line` / `pe_non_directional_omits_rejected_line`. Library header line covered. ✅
- `pe_lookup`'s `.expect(...)` is an invariant assertion (`needed` is built *from* the plan, so every planned `(mate,kind)` is guaranteed present) — cannot fire at runtime. Acceptable.
- No temp-filename collisions: stems `_C_to_T`/`_G_to_A` keep all 4 non-dir PE files distinct.
- Naming/structure clean; single construction site for each plan (reviewer-A §5 / B Opt-4 honored); clippy/fmt clean.

---

## Issues by area

### Logic — none
### Efficiency — none (non-dir's 4 instances / 4 temps are inherent to the mode, matching Perl; conversions deduped)
### Errors — none
### Structure — 1 Low (doc drift, below)

---

## Fixes applied
None (no unambiguous low-risk defect found).

---

## Recommendations

- **[Low] Stale module-level doc comment** (`lib.rs:15–21`). The crate doc still reads *"Implemented so far (single-end directional spine) … The SE-directional pipeline runs end to end. PE / non-directional / pbat / FastA / threading land in later phases."* As of Phase 7 (PE) and Phase 8 (non-dir + pbat) this is inaccurate — only FastA + multicore/threading remain. The diff already updated the runtime dispatch message (`lib.rs:114`) to the correct reality, so this header is the lone straggler. Doc-only, no functional impact; was already stale after Phase 7. Suggest refreshing to match the dispatch message. (Not blocking.)

- **[Informational] Multicore not gated in dispatch.** `pipeline` matches on `(layout, format)` only, so `--multicore N` would silently route to `run_se`/`run_pe` (which are single-core) rather than the "later phase" arm. This is consistent with the documented Phase-8 scope (single-core assumed; multicore = Phase 9) and no test exercises it, so it is not a Phase-8 defect — just flagging for the Phase-9 author so the multicore arm is added deliberately.

---

## Verdict

**APPROVE.** The implementation is byte-faithful to Perl v0.25.1 on every load-bearing cell I verified (SE/PE instance plans across all 3 modes, the pbat `+2` routing, the GA methylation branch, the conversion dedup + pbat inversion + per-mate ID suffix), the directional path is byte-frozen, the load-bearing GA-strand tests genuinely map and byte-assert FLAG/SEQ/XR/XG/XM (no all-unmapped false-pass), and the per-mode temp cleanup is complete and asserted. 211 tests green, clippy `-D warnings` clean, fmt clean. The only finding is a Low-priority stale module doc comment. Clear to proceed to the dual code-review comparison and the oxy byte-identity gate.
