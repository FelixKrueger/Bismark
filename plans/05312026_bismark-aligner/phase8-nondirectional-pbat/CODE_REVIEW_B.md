# CODE REVIEW B — Phase 8: Non-directional + pbat (SE + PE, FastQ)

**Reviewer:** Code Reviewer B (independent, fresh context)
**Scope:** working-tree diff vs commit `ca6af0a` (Phase 7) in `rust/bismark-aligner/`
**Files:** `src/convert.rs`, `src/lib.rs`, `src/methylation.rs` (tests), `src/output.rs` (tests), `tests/cli.rs`
**Verdict: APPROVE** — every wiring cell verified against Perl v0.25.1; both load-bearing XM strings independently re-derived and confirmed; build/clippy/fmt clean; directional regression guard green. Only Low-severity nits.

---

## Summary

Phase 8 is, as the plan claimed, almost entirely **wiring**: conversion variants + per-mode driver instance plans + a 2-arm dispatch collapse. The pre-built machinery (SE `+2` modifier, `directional`-gated merge reject, FLAG/XR/XG, report library/rejected lines) is genuinely untouched. I verified every disputed cell against the Perl source rather than trusting §12, and **every one matches**. The test design closes the reviewer-flagged false-pass trap correctly: the GA-emitting fakes map only the `*_G_to_A*` reads on a chosen index, and each test asserts `unique best alignments: 1` plus a written record and byte-exact FLAG/SEQ/XR/XG/XM, so an all-unmapped silent pass is impossible.

- **Tests:** 183 lib + 28 integration = **211**, all pass.
- **clippy** `-p bismark-aligner --all-targets -- -D warnings`: clean.
- **cargo fmt** `--check`: clean (exit 0).

---

## Verification by review priority

### 1. Per-mode instance plans vs Perl `@fhs` — ALL CELLS CORRECT

**SE** (`se_instance_plan`). Orientation comes from the slot **name** (Perl 6873: `CTreadCTgenome`|`GAreadGAgenome` → `--norc`, else `--nofw`), not the index. Cross-checked names from the `@fhs` templates (7153–7242) + input assignment (519–546):

| Mode | Slot | `@fhs` name | index | orient | reads file | Rust tuple | OK |
|---|---|---|---|---|---|---|---|
| dir | 0 | CTreadCTgenome | CT | --norc | C→T (0) | `(Norc,Ct,0)` | ✓ |
| dir | 1 | CTreadGAgenome | GA | --nofw | C→T (0) | `(Nofw,Ga,0)` | ✓ |
| pbat | 0 | GAreadCTgenome | CT | --nofw | G→A (0) | `(Nofw,Ct,0)` | ✓ |
| pbat | 1 | GAreadGAgenome | GA | --norc | G→A (0) | `(Norc,Ga,0)` | ✓ |
| nondir | 0 | CTreadCTgenome | CT | --norc | C→T (0) | `(Norc,Ct,0)` | ✓ |
| nondir | 1 | CTreadGAgenome | GA | --nofw | C→T (0) | `(Nofw,Ga,0)` | ✓ |
| nondir | 2 | GAreadCTgenome | CT | --nofw | G→A (1) | `(Nofw,Ct,1)` | ✓ |
| nondir | 3 | GAreadGAgenome | GA | --norc | G→A (1) | `(Norc,Ga,1)` | ✓ |

The **SE pbat orientation flip** (s0 `--nofw`, s1 `--norc`) is confirmed: pbat's slot names are `GAread*`, so `GAreadGAgenome` (s1) is the only `--norc`, inverting directional. ✓

**PE** (`pe_instance_plan`). PE slot names are fixed at 295–298 (mode-independent); orientation rule 6466 (`CTread1GAread2CTgenome`|`GAread1CTread2GAgenome` → `--norc`). Per-mate conv kind from input assignment 394–452 (`inputfile_1`=R1 kind, `inputfile_2`=R2 kind):

| Mode | Slot | name | idx | orient | (k1,k2) | Rust tuple | OK |
|---|---|---|---|---|---|---|---|
| dir | 0 | CTread1GAread2CTgenome | CT | norc | (Ct,Ga) | `(0,Norc,ICt,Ct,Ga)` | ✓ |
| dir | 3 | CTread1GAread2GAgenome | GA | nofw | (Ct,Ga) | `(3,Nofw,IGa,Ct,Ga)` | ✓ |
| pbat | 1 | GAread1CTread2GAgenome | GA | norc | (Ga,Ct) | `(1,Norc,IGa,Ga,Ct)` | ✓ |
| pbat | 2 | GAread1CTread2CTgenome | CT | nofw | (Ga,Ct) | `(2,Nofw,ICt,Ga,Ct)` | ✓ |
| nondir | 0/1/2/3 | (all four) | CT/GA/CT/GA | norc/norc/nofw/nofw | (Ct,Ga)/(Ga,Ct)/(Ga,Ct)/(Ct,Ga) | matches | ✓ |

All 4 PE non-dir slot kinds and orientations match Perl 444–451 + the name rule. Merge slot index is the *literal* slot (`merge.rs:484` iterates `SCAN_ORDER=[0,3,1,2]` and keys `best.index` on the actual slot, not the scan position), so `best.index` equals the Perl `@fhs` index in every mode. ✓

### 2. pbat SE `+2` — 2-element Vec, NOT a padded 4-vec — CONFIRMED
`run_se` builds `streams` by `push`-ing only the `se_instance_plan` entries (2 for pbat) into a plain `Vec`; the SE merge uses `streams.iter_mut().enumerate()` (`merge.rs:157`), so pbat's physical slots 0/1 are real indices 0/1, and `extract_..._single_end(.., pbat=true, ..)` adds `pbat_mod=2` → eff 2/3 (CTOT/CTOB). Not a `Vec<Option<_>>`, no `None` padding. ✓ (`pbat` derived at `lib.rs:200` from `LibraryType::Pbat`.)

### 3. PE conversion dedup + slot→file mapping — CORRECT
`run_pe` collects unique `(mate,kind)` into `needed` (first-seen order), converts each once via `bisulfite_convert_fastq_pe_kind`, then `pe_lookup`s per slot. Counts: directional/pbat = 2 files, non-dir = 4 — matches Perl (directional/pbat make 2 converted files shared by both instances; non-dir makes 4). pbat inverts via explicit `ConvKind` (R1=Ga, R2=Ct) — no silent reuse of the directional read#→kind map (rev1 B I-1 satisfied: `bisulfite_convert_fastq_pe` now delegates to `_kind`). No `+2` modifier on the PE path. ✓

### 4. Directional byte-freeze — CONFIRMED
The directional SE+PE integration tests (`mapped_read_writes_bam_record_end_to_end`, `pe_mapped_writes_two_bam_records_end_to_end`, `unmapped_routing_and_report_end_to_end`, `ambiguous_and_ambig_bam_end_to_end`, `pe_unmapped_routing_to_1_and_2_files`) all pass through the generalized `run_se`/`run_pe`. The directional conversion path is byte-frozen by delegation (the `_kind` core is the same `convert_fastq_impl` with the same `(kind, suffix, file_base)`).

### 5. Load-bearing test risk — INDEPENDENTLY RE-DERIVED, CORRECT
The fakes map ONLY `*_G_to_A*` reads on a chosen index (`*BS_CT*` → CTOT, `*BS_GA*` → CTOB); each test asserts `unique best alignments:   1` + a written BAM record, so a non-dir/pbat test **cannot** false-pass on all-unmapped. I re-derived both XM strings from scratch (genome `TTGCGTACTT`, read `GCGTAC`, pos 3, 6M):

- **CTOT (eff 2, '-', GA/CT, FLAG 0):** M=`GCGTAC` + 3' append `TT` = `GCGTACTT`, revcomp → `AAGTACGC`. `methylation_call(GCGTAC, AAGTACGC, Ga)` (compare seq[i] vs genomic[i+2], context upstream): i0 G==G meC, up=A,A → `H`; i1 `.`; i2 `.`; i3 `.`; i4 A vs G converted, up=C → `z`; i5 `.` ⇒ forward `H...z.`, reversed on '-' → **`.z...H`**. SEQ = revcomp(read) = **`GTACGC`**. Matches both the unit test (`output.rs`) and the integration test (`tests/cli.rs`). ✓
- **CTOB (eff 3, '+', GA/GA, FLAG 16):** 5' prepend `TT` + M `GCGTAC` = `TTGCGTAC`, no revcomp. `methylation_call(GCGTAC, TTGCGTAC, Ga)`: i0 G==G meC, up=T,T → `H`; i1 `.`; i2 G==G meC, up=C → `Z`; i3–5 `.` ⇒ **`H.Z...`**, not reversed ('+'). SEQ = **`GCGTAC`** (original). Matches both tests. ✓

The integration tests also confirm the **original** read (not the converted `ACATAC` the fake emits) is used for SEQ/XM — a genuine end-to-end exercise of the GA `methylation_call` branch and the CTOT/CTOB SE FLAG arms, first run in Phase 8. PE index-1/2 records (FLAG 163/83 and 147/99, XR_1=GA/XR_2=CT, XG GA/CT) are asserted via the real PE engine. ✓

### 6. Per-mode temp cleanup — CORRECT + ASSERTED
SE deletes every `converted[*]` (1 for dir/pbat, 2 for non-dir); PE deletes every `converted[*].1` (2 for dir/pbat, 4 for non-dir). Best-effort `let _ = remove_file`, matching Perl's warn-never-die. Byte-invisible, but the integration tests assert the exact files are gone (`pbat_se_*`, `nondir_se_four_instances_*`, `pbat_pe_ga_index_*`, `nondir_pe_four_slots_*`). ✓

### 7. Structure / clippy / fmt — clean
Plan-as-data instance plans at single construction sites; `IndexChoice`/`conv_label`/`pe_id_suffix`/`file_base_for` are small and well-named. No duplication. clippy `-D warnings` and fmt both clean.

---

## Issues by area

### Logic / Correctness
None. Every Perl-truth cell (SE/PE × 3 modes × {slot, index, orient, file, conv kind}) verified; both XM derivations independent and exact; merge index semantics correct in all modes; reject gating inert for non-dir/pbat.

### Errors / Edge cases
- **Low — `pe_lookup` panics (`.expect`) instead of returning `Err`.** The dedup loop guarantees the invariant holds, so it is structurally unreachable; an internal-invariant panic is defensible, but a `Result` would be more idiomatic and consistent with the rest of the driver's error handling. Not a behavior bug.

### Efficiency / Style
- **Low — `Vec::with_capacity(2)` in `run_se` is undersized for non-dir (pushes 4).** Pure hint; the Vec grows correctly. Slightly misleading; `with_capacity(se_instance_plan(config.library).len())` or just `Vec::new()` would read truer. No functional impact.
- **Low (informational) — conversion STDERR banner changed** from a single combined PE line to one line per converted file (§12 deviation). STDERR is not byte-gated and no test asserts the old combined PE text (grep confirms); the directional SE "Created C->T converted" substring is preserved. Safe.

### Tests
No gaps found for the byte-identity contract at this scale. The fakes correctly cannot false-pass on all-unmapped. (As the plan notes, the headline byte-identity proof is still the oxy gate on `10M_SE`/`10M_PE` with `--non_directional`/`--pbat` — out of scope for this code review.)

---

## Fixes applied
None. All findings are Low-severity, non-behavioral nits where the current code is correct; per the skill's fix-vs-recommend guidance these are recommendations, not unambiguous defects warranting an in-place edit.

---

## Recommendations (priority)
1. **Low** — return a `Result` from `pe_lookup` (or `unwrap` with an `unreachable!`-style message) instead of `.expect`, for consistency with the driver's `Result` flow.
2. **Low** — drop or right-size the `Vec::with_capacity(2)` hint in `run_se`.
3. **Low (informational)** — note the per-file conversion banner change in the PR description so a future PE banner-text consumer isn't surprised.

---

## Verdict: **APPROVE**

The wiring is byte-faithful to Perl v0.25.1 across every SE/PE × mode cell; the load-bearing CTOT/CTOB/PE-index-1/2 paths are exercised for the first time with byte-exact assertions that I independently re-derived; the directional regression guard is green; build, clippy `-D warnings`, and fmt are clean. Remaining items are cosmetic Low-severity nits. Cleared for the oxy byte-identity gate.

**File:** `/Users/fkrueger/Github/Bismark-aligner/plans/05312026_bismark-aligner/phase8-nondirectional-pbat/CODE_REVIEW_B.md`
