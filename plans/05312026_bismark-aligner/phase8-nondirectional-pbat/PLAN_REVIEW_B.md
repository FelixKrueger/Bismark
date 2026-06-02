# PLAN REVIEW B — Phase 8: Non-directional + pbat (SE + PE, FastQ)

**Reviewer:** B (independent, fresh context)
**Plan:** `plans/05312026_bismark-aligner/phase8-nondirectional-pbat/PLAN.md` (rev 0, 2026-06-02)
**Verdict:** APPROVE-WITH-FINDINGS — **0 Critical, 3 Important, 4 Optional**

I traced every slot/file/orientation/index claim against the Perl source and the
already-built Rust. The plan's central thesis — *Phase 8 is wiring; the merge,
the SE `+2` modifier, the FLAG tables, the report library/strand lines, and the
config conflicts are all already correct* — is **verified true**. The pbat-SE
index-modifier path (the focus area flagged as the most likely Critical) is
correct end-to-end. Findings are about plan accuracy and test scaffolding, not
about wrong slot/strand wiring.

---

## 1. Logic review (every slot/file/index claim verified)

### 1.1 pbat SE index-modifier — VERIFIED CORRECT (no Critical here)
This was the highest-risk area. Trace:
- pbat-SE `@fhs` is a **2-slot** array (Perl 7199–7214): slot 0 = `GAreadCTgenome`
  (bisulfiteIndex `$CT_index_basename`), slot 1 = `GAreadGAgenome`
  (bisulfiteIndex `$GA_index_basename`).
- Both slots read `$G_to_A_infile` (Perl 535).
- Orientation rule (Perl 6873): `CTreadCTgenome|GAreadGAgenome → --norc`, else
  `--nofw`. ⇒ slot 0 (`GAreadCTgenome`) = `--nofw`; slot 1 (`GAreadGAgenome`) =
  `--norc`.
- ⇒ pbat SE physical slots: **s0 = CT-index / `--nofw` / G→A reads**, **s1 =
  GA-index / `--norc` / G→A reads**. This is **exactly** the plan §3.2 pbat row.
- The merge stores `index = 0,1` (enumerate slot). Extraction adds `pbat_mod=+2`
  (`methylation.rs:120–121`) ⇒ `eff = 2,3`.
- `eff=2` → `(b'-', Ga, Ct)` → counter `ga_ct_count`; `eff=3` → `(b'+', Ga, Ga)`
  → counter `ga_ga_count` (`methylation.rs:131–141, 210–214`). These are the
  CTOT/CTOB buckets — matching the plan §3.2/§3.5 claim "counts land in
  `ga_ct`/`ga_ga`".
- The SE FLAG is derived from `(strand, read_conv, genome_conv)`, NOT the raw
  index (`output.rs:356–360`): `eff=2 → (-,GA,CT) → FLAG 0`; `eff=3 → (+,GA,GA)
  → FLAG 16`. Byte-identical to Perl 8534–8538 / 8526–8527.

**Conclusion:** pbat SE feeds the G→A read file against the correct CT/GA
indexes, lands in CTOT/CTOB, and emits the Perl FLAGs. ✔ No Critical.

### 1.2 SE non-dir 4-slot wiring — VERIFIED CORRECT
Non-dir SE `@fhs` (Perl 7216–7242): s0 `CTreadCTgenome`/CT-idx, s1
`CTreadGAgenome`/GA-idx, s2 `GAreadCTgenome`/CT-idx, s3 `GAreadGAgenome`/GA-idx.
Input files (Perl 544–545): s0,s1 ← C→T; s2,s3 ← G→A. Orientation (6873): s0
`--norc`, s1 `--nofw`, s2 `--nofw`, s3 `--norc`. **Plan §3.2 non-dir row matches
all four slots exactly.** ✔ No swap.

### 1.3 PE non-dir / pbat slot→file mapping — VERIFIED CORRECT
- Non-dir (Perl 444–451): s0 `-1`C→T₁/`-2`G→A₂, s1 `-1`G→A₁/`-2`C→T₂, s2
  `-1`G→A₁/`-2`C→T₂, s3 `-1`C→T₁/`-2`G→A₂. **= plan §3.3 non-dir row.** ✔
- pbat (Perl 425–432): s0/s3 = undef; s1 `-1`G→A₁/`-2`C→T₂, s2 `-1`G→A₁/`-2`C→T₂.
  **= plan §3.3 pbat row.** ✔
- Per-slot index (PE `@fhs` template 7126–7151 / 7170–7197 / 7216–7242, all
  identical): s0=CT, s1=GA, s2=CT, s3=GA. **= plan.** ✔
- Per-slot orientation: PE names are overwritten at Perl 295–298 to
  `CTread1GAread2CTgenome`(s0), `GAread1CTread2GAgenome`(s1),
  `GAread1CTread2CTgenome`(s2), `CTread1GAread2GAgenome`(s3); rule 6466 ⇒
  s0,s1 `--norc`, s2,s3 `--nofw`. **= plan §3.3.** ✔
- PE FLAG constants (`output.rs:469–473`) index 0→(99,147), 1→(163,83),
  2→(147,99), 3→(83,163) match Perl 8825–8868 `!old_flag` default. The merge
  stores index = Bismark slot, the reject is `directional && index∈{1,2}`
  (`merge.rs:661`) — inert for non-dir/pbat. ✔

### 1.4 Conversion (§3.1) — VERIFIED CORRECT
- SE pbat = G→A only, no suffix (Perl 5523–5539 + the 1-element return 5643). ✔
- SE non-dir = C→T + G→A, no suffix (Perl 5550–5573). ✔
- PE pbat = R1 G→A `/1/1`, R2 C→T `/2/2` (Perl 5854–5876 + suffix 5945–5960). ✔
- PE non-dir = both per mate (Perl 5901–5912). ✔
- The existing `convert_fastq_impl(kind, id_suffix, file_base)` already
  parameterizes all of this; the only gap is **inverted R1/R2 mapping** for pbat
  PE (existing `bisulfite_convert_fastq_pe` hardcodes R1=Ct/R2=Ga,
  `convert.rs:185–192`). The plan acknowledges this (§4 comment). ✔ (see Imp-1)

### 1.5 Report / report strand rows — VERIFIED CORRECT
- Library lines: SE pbat (Perl 1715, doubled "strands"/"OT and OB"), PE pbat
  (1944, "(OT, OB)"), non-dir (1718/1947) — `report.rs:83–98` matches each
  byte-for-byte. ✔
- The SE strand rows (Perl 2044) and PE rows (2219) print all four buckets
  unconditionally in every mode; pbat-SE's `ga_ct`/`ga_ga` therefore land in the
  GA/CT + GA/GA rows correctly (`report.rs:147–148`, no code change needed). ✔
- Rejected-count line gated on `directional` (Perl 2046 SE / 2223 PE);
  `report.rs:151/223` already gate. The driver passes `directional = matches!(…
  Directional)` (`lib.rs:139/377/526/782`) ⇒ omitted for non-dir/pbat. ✔

### 1.6 Directional path stays byte-identical — LOW RISK
The Open-Q1 decision (generalize `run_se_directional → run_se` in place) is sound
*because* the directional instance plan (§3.2/§3.3 first rows) is itself one arm
of the same table the directional code already implements (`lib.rs:157–172`
SE; `557–574` PE). The existing gated SE/PE suites + the phase-10 directional
oxy gate are the regression guard. No logic change is needed for directional —
only a refactor that must reproduce the same `(orient, index_basename, file)`
tuples. ✔

---

## 2. Assumptions

- **A-ok:** "the merge code path is identical in all modes" — confirmed: the
  merge never branches on library type; only `directional` gates the reject, and
  the strand/FLAG is computed downstream from the (effective) index.
- **A-ok:** pbat PE "no modifier" — PE extraction keys on the raw stored index
  (`methylation.rs:410, 421–425`); Perl 4471+ has no pbat modifier. The pbat PE
  strands come from physically populating slots 1,2 (not from a `+2`).
- **A-watch (Open-Q3):** the gate reuses `10M_SE`/`10M_PE` with mode flags. This
  is correct for the *byte-identity contract* (same reads, same mode, both
  tools), but see Imp-3 — these libraries are directional, so the complementary
  slots (GA-read instances) will produce **near-zero** alignments. A byte-clean
  gate on near-empty CTOT/CTOB does NOT exercise the new strand/FLAG paths with
  real volume; the unit/integration tests (§7 #2–5) are the real guard for those.
  The plan notes this (Open-Q3, GATE_OXY note) — acceptable, but the integration
  tests must therefore be strong (Imp-2).
- **A-implicit:** the "Now running N instances" / "Input file is …" stderr
  banners (Perl 6855–6868 SE, 6438–6452 PE) are warn-only and NOT gated (the gate
  filters stderr). The plan does not mention them; that's fine — confirmed
  non-load-bearing.

---

## 3. Efficiency

§6 is accurate: non-dir = 4 instances + 2 converted files/mate (inherent, matches
Perl); pbat = 2 instances (same as directional); no extra genome passes. No
concern. One note: the SE driver currently builds `Vec<AlignerStream>` and the PE
driver `Vec<Option<PairedAlignerStream>>`. The generalized `run_se` must keep the
**positional `enumerate` index = Perl `@fhs` index** invariant (§3.2 🔴) — for
pbat SE that means a **2-element** Vec at slots [0,1] (NOT [2,3]); the `+2` lives
only in extraction. Mis-sizing this (e.g. building a 4-vec with None at 0,1) would
break the enumerate→index mapping. Flagging as Opt-1 (the plan is correct as
written; this is an implementation trap to call out).

---

## 4. Validation sufficiency

§7 covers the right cases (conversion bytes, 4-slot SE non-dir, pbat-SE CTOT/CTOB,
PE non-dir/pbat slots, no-reject, report-per-mode, directional regression, oxy
gate). Gaps:

- **Imp-2:** the existing `tests/cli.rs` fakes emit a mapped hit **only on the
  `*BS_CT*` index** (`cli.rs:78–80`, and the PE fake reads the `-1` CT file,
  `cli.rs:560–566`). Tests §7 #2/#3/#4/#5 require the complementary slots (GA
  index / pbat G→A reads) to actually MAP. The plan says "via fake bowtie2" but
  does NOT state that **new fake variants emitting on `*BS_GA*` (and a pbat-PE
  fake reading the G→A `-1` file)** must be written. Without them, #3 ("counts in
  ga_ct/ga_ga") and #2/#4 cannot be exercised — the test would silently pass on
  all-unmapped. Make this explicit in §5/§7.
- **Imp-3 (gate realism):** see §2 A-watch — add a line to §7 #9 / GATE_OXY that
  the directional 10M libraries will populate CTOT/CTOB at ~0, so the
  integration tests (#2–5), not the gate, are the proof that the new strand
  arithmetic + FLAGs are right. (The plan half-says this; make it a stated
  acceptance condition so it isn't lost.)
- **Opt-2:** no test asserts the **temp-cleanup set per mode** (§3.5 / §5 step 5)
  — e.g. pbat SE deletes the G→A temp (not a C→T that was never made). A unit/
  integration assertion that the right temp file(s) are removed (and no stray
  remains) would catch a copy-paste of the directional cleanup. Optional.
- **Opt-3:** no explicit test for **pbat-SE FLAG bytes** (eff=2 → FLAG 0, eff=3 →
  FLAG 16). §7 #3 says "FLAG/XR/XG per CTOT/CTOB" but doesn't pin the values; pin
  them (FLAG 0 / XR GA / XG CT for CTOT; FLAG 16 / XR GA / XG GA for CTOB).

---

## 5. Alternatives

The in-place generalization (Open-Q1) is the right call over duplicating drivers
(less drift, the gated directional path guards it). No better alternative. One
structural suggestion (Opt-4): model the per-mode SE plan as a small
`Vec<(Orientation, &Path index, &Path reads)>` built by a `match library` and
let `drive_merge` keep enumerate-indexing — this makes the "slot order ==
enumerate index" invariant a single, auditable construction site (mitigates
Opt-1). The §4 signature sketch already gestures at this.

---

## 6. Action items

### Critical
*(none)*

### Important
- **Imp-1 — pbat PE conversion entry shape.** The existing
  `bisulfite_convert_fastq_pe` hardcodes R1→C→T / R2→G→A (`convert.rs:185–192`).
  pbat PE needs R1→G→A `/1/1` `_G_to_A` and R2→C→T `/2/2` `_C_to_T` (Perl
  5854–5876). Specify the exact new entry (e.g. a `read_number × library`
  dispatch or a dedicated `_pbat` fn) so the implementer doesn't reuse the
  directional fn and silently produce the wrong files. (Plan flags it in §4 but
  leaves the shape "Open-Q4 assumption".)
- **Imp-2 — new fake-bowtie2 variants for the complementary slots.** §7 #2–5
  cannot exercise CTOT/CTOB/pbat unless a `*BS_GA*`-emitting SE fake and a
  pbat/non-dir PE fake (mapping on the G→A `-1`) are added. State this explicitly
  in §5 step 6 and §7.
- **Imp-3 — gate realism caveat as an acceptance condition.** The directional
  10M reuse means the new strands are ~empty in the oxy gate; record that the
  integration tests are the load-bearing proof for the new strand/FLAG paths.

### Optional
- **Opt-1 — pbat-SE Vec sizing trap.** Call out that pbat SE supplies a 2-element
  stream Vec at enumerate-slots [0,1]; the `+2` is extraction-only. Do NOT build a
  4-vec with None at 0,1.
- **Opt-2 — assert per-mode temp cleanup** (pbat-SE deletes only the G→A temp,
  etc.).
- **Opt-3 — pin pbat-SE FLAG/XR/XG byte values** in §7 #3 (FLAG 0/GA/CT and
  16/GA/GA).
- **Opt-4 — single-construction-site instance plan** (`match library → Vec<(orient,
  index, reads)>`) to make the slot-order invariant auditable.
- **Nit — "extend `pe_gate.sh`" (§7 #9):** no `pe_gate.sh` exists in the repo
  (only `scripts/oxy_idle_gate.sh`); the oxy gate lives in phase 10. Reword to the
  actual phase-10 gate harness.

---

## 7. Final verdict

**APPROVE-WITH-FINDINGS** — 0 Critical, 3 Important, 4 Optional.

The plan's correctness claims hold up under a line-by-line Perl/Rust trace: every
SE and PE slot→(index, orientation, file) mapping, the pbat-SE `+2` modifier path,
the FLAG tables, the report library/strand lines, and the config conflicts are
already built and verified correct, so Phase 8 genuinely is wiring. The Important
items are about *plan completeness* (the inverted pbat-PE conversion entry, the
need for GA-emitting fakes to actually test the new strands, and a gate-realism
caveat), not about wrong wiring — address them before implementation so the new
strand/FLAG paths are proven by tests, since the directional-library oxy gate
won't exercise them at volume.
