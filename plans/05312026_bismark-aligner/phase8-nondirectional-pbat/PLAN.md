# PLAN — Phase 8: Non-directional + pbat (SE + PE, FastQ) 🎯

> **Epic:** `05312026_bismark-aligner/EPIC.md`, Phase 8 — *Non-directional + pbat* 🎯 (byte-identity gate, all library types)
> Depends on: **Phase 5** (SE genomic-seq + the `pbat` +2 index modifier), **Phase 6** (SE report + aux), **Phase 7**
> (PE spine + the `library_line(library, paired)` report header + the generic slot-indexed PE merge). Directional SE+PE
> already byte-identical (Phases 1–7, gated). 🎯 **gate: byte-identical across ALL library types.**

## 1. Goal

Add the **non-directional** and **pbat** library types for both SE and PE (FastQ), byte-identical to Perl
`bismark` v0.25.1 + Bowtie 2 2.5.5. The headline is that **most of the machinery already exists** — Phase 8 is
concentrated in two places: the **read-conversion variants** and the **driver's per-mode instance plan** (which
`@fhs` slots spawn, which converted file each reads). Everything else (the merge's gated wrong-strand rejection,
the SE genomic-extraction `+2` pbat index modifier, the 3-way report header + the `directional`-gated rejected
line, the CLI/config `LibraryType` + conflict validation) is **already built and verified** — Phase 8 *wires* it.

The Perl mental model (verified, see §2): three modes via two booleans `$directional` / `$pbat`
(non-directional = both false). pbat changes **only which instances spawn** (+ the SE-only `+2` index modifier);
it does **not** alter any FLAG/strand-assignment logic. The merge code path is identical in all modes.

## 2. Context

### Already in place (verify-only — NO change expected)
| Component | Status | Evidence |
|---|---|---|
| CLI `--non_directional`/`--pbat` | ✅ `cli.rs:79–83` | parsed |
| `LibraryType {Directional, NonDirectional, Pbat}` + conflict dies (non_dir⊕pbat; pbat⊕gzip; pbat⊕fasta) | ✅ `config.rs:27–33, 275–303` | matches Perl 8146–8166 |
| SE genomic extraction `pbat` param + `pbat_mod = +2`, `eff = index + pbat_mod` | ✅ `methylation.rs:108/120–121` | Perl 4308–4311 |
| Merge wrong-strand rejection **gated on `directional`** (SE idx 2\|3, PE idx 1\|2) | ✅ `merge.rs` (passed `directional`) | Perl 3112–3118 / 3851–3857 |
| Report 3-way library line (`library_line(library, paired)`) + `directional`-gated rejected-count line | ✅ `report.rs` (Phase-7 fix) | Perl 1711–1719/1940–1948 + 2046–2049/2221–2224 |
| Merge iterates N streams (2 or 4); strand counters keyed on index/eff | ✅ Phases 4–7 | — |

### New work (the bulk)
- **`convert.rs`** — add the SE G→A + non-dir(both) outputs and the PE pbat(R1 G→A / R2 C→T) + non-dir(both-per-mate) outputs.
- **`lib.rs`** — generalize `run_se_directional` → `run_se` and `run_pe_directional` → `run_pe`, each driven by a
  **per-mode instance plan** (which converted file(s) to make, which streams to spawn with which index/orient/file,
  the `pbat` flag for SE). Add the dispatch arms for `(SE/PE, NonDirectional/Pbat, FastQ)`.

### Perl source of truth
- SE conversion `biTransformFastQFiles` 5489–5651 (pbat 5523–5539 = G→A only; non-dir opens CTOT+GTOA 5550–5573).
- PE conversion `biTransformFastQFiles_paired_end` 5810–6025 (pbat 5854–5876 R1 G→A/R2 C→T; non-dir 5901–5912 both-per-mate).
- SE `@fhs` template `reset_counters_and_fhs` 7153–7242 (dir/pbat = 2 slots; non-dir = 4) + input assignment 519–546.
- PE slot/input assignment 394–452 + launcher 6432–6523 (skip-unpopulated 6456–6463); SE launcher 6849–6911.
- SE `pbat_index_modifier` `extract_corresponding_genomic_sequence_single_end` 4308–4311 (PE has NONE — 4471+ keys on raw index).
- Reject gates 3112–3118 / 3851–3857; report 1711–1719/1940–1948 + 2046–2049/2221–2224; CLI 8146–8166.

## 3. Behavior (numbered)

### 3.1 Read conversion (`convert.rs`)
Reuse the shared `convert_fastq_impl(kind, id_suffix, file_base)` (Phase 7). Add entry points:
- **SE pbat** — G→A only (Perl 5523–5539): one `_G_to_A.fastq` file, no ID suffix. `convert_fastq_impl(Ga, b"", "_G_to_A")`.
- **SE non-directional** — C→T **and** G→A (Perl 5550–5573): two files (`_C_to_T` + `_G_to_A`), no ID suffix.
- **PE pbat** (Perl 5854–5876): R1 → **G→A** (`/1/1`), R2 → **C→T** (`/2/2`). (The mirror of directional's R1 C→T / R2 G→A.)
  🔴 **rev1 (B I-1):** the existing `bisulfite_convert_fastq_pe(read_number)` HARDCODES R1→C→T / R2→G→A by
  read-number — pbat inverts that. Add an explicit library-aware selector (a `(library, read_number) → ConvKind`
  map, or a `pbat: bool` param) rather than overloading the directional fn; specify the exact ConvKind per
  (mode, mate) so the implementer doesn't silently keep the directional mapping. See §4.
- **PE non-directional** (Perl 5901–5912): **each mate → BOTH** C→T and G→A (`/1/1` for R1's two files, `/2/2` for R2's) → 4 temp files.
- Edge cases: empty input, gzip (note **pbat⊕`--gzip` is rejected at config**), CRLF, skip/upto — all inherited from the shared core (already tested).

### 3.2 SE driver (`lib.rs`) — generalize `run_se_directional` → `run_se`
Per-mode **instance plan** (file(s) to convert → streams `(index, orientation, index-basename, reads-file)`), then
the existing `drive_merge(streams, …, pbat, …)`. The merge index = stream slot (`enumerate`); SE extraction adds
`pbat_mod` (+2) iff pbat → so pbat's physical slots 0/1 map to logical 2/3 (CTOT/CTOB). 🔴 streams MUST be supplied
in slot order so `enumerate` yields the Perl `@fhs` index.

| Mode | convert | streams (slot: index-basename / orient / reads) | `pbat` | reject |
|---|---|---|---|---|
| **directional** (existing) | C→T | s0: CT/`--norc`/C→T · s1: GA/`--nofw`/C→T | false | (gated off) |
| **pbat** | G→A | s0: CT/`--nofw`/G→A · s1: GA/`--norc`/G→A | **true** | off |
| **non-directional** | C→T + G→A | s0: CT/`--norc`/C→T · s1: GA/`--nofw`/C→T · s2: CT/`--nofw`/G→A · s3: GA/`--norc`/G→A | false | off (all kept) |

(Per Perl 519–546 + the 6873 `--norc`/`--nofw` name rule + the SE `@fhs` templates 7153–7242.)

### 3.3 PE driver (`lib.rs`) — generalize `run_pe_directional` → `run_pe`
PE uses the slot-indexed `&mut [Option<PairedAlignerStream>]` (length 4); populate the right slots per mode, feed
each its `-1`/`-2` converted files, then `drive_merge_pe`. PE extraction has **no** pbat modifier (keys on raw index).
Per-slot index: s0=CT, s1=GA, s2=CT, s3=GA; orient: s0,s1=`--norc`, s2,s3=`--nofw` (Perl 6466–6471 name rule).

| Mode | convert (files) | populated slots: `-1` / `-2` | reject |
|---|---|---|---|
| **directional** (existing) | C→T_1, G→A_2 | s0: C→T_1/G→A_2 · s3: C→T_1/G→A_2 | (gated off) |
| **pbat** | G→A_1, C→T_2 | s1: G→A_1/C→T_2 · s2: G→A_1/C→T_2 | off |
| **non-directional** | C→T_1, G→A_1, C→T_2, G→A_2 | s0: C→T_1/G→A_2 · s1: G→A_1/C→T_2 · s2: G→A_1/C→T_2 · s3: C→T_1/G→A_2 | off (all kept) |

(Per Perl 394–452.) Note the `Vec<Option<_>>` must place each stream at its **Bismark slot index** (0/1/2/3), `None` elsewhere.

### 3.4 Dispatch (`lib.rs` `pipeline()`)
Add arms: `(SingleEnd, NonDirectional|Pbat, FastQ) → run_se`; `(PairedEnd, NonDirectional|Pbat, FastQ) → run_pe`.
(Directional arms fold into the generalized `run_se`/`run_pe`.) FastA + all-modes threading remain Phase 9.

### 3.5 Report / temp cleanup
- Report: **no new code** — `write_report_header` already emits the pbat/non-dir library line (per `library_line`),
  and `print_final_analysis_report_{single,paired}_ends` already gate the rejected-count line on `directional`
  (pass `directional=false` for non-dir/pbat → line omitted, matching Perl). The 4 strand-count lines always print
  (unused strands stay 0; pbat SE lands counts in CTOT/CTOB via the `+2` modifier). **Verify** the exact bytes per mode.
- Temp cleanup: delete the per-mode temp set (SE pbat = G→A; SE non-dir = C→T+G→A; PE pbat = G→A_1+C→T_2; PE non-dir = 4 files), best-effort (Perl 1974–1999 / 2154–2181).

### Edge cases
- pbat SE: the `+2` index modifier maps physical slots 0/1 → logical CTOT(2)/CTOB(3); strand counters land in `ga_ct`/`ga_ga` (verify).
- non-dir: NO rejection — a read whose best is on a complementary strand is kept (directional would reject it).
- pbat⊕`--gzip` and pbat⊕FastA: already rejected at config (Perl 8155–8156) — no driver concern.
- non_dir⊕pbat: already a config die (Perl 8148–8153).
- A read mapping equally well to an original AND a complementary strand in non-dir → the existing cross-instance-tie ambiguity (already handled).

## 4. Signatures (proposed)
```rust
// convert.rs — pbat/non-dir entry points (reuse convert_fastq_impl).
// 🔴 rev1 (B I-1): the PE conversion must be LIBRARY-AWARE, not read-number-hardcoded.
//   Per-mate ConvKind by mode (Perl 5854–5912): directional R1=CT/R2=GA; pbat R1=GA/R2=CT;
//   non-dir R1=both/R2=both. The /1/1 //2/2 ID suffix is per-mate regardless of mode.
pub fn bisulfite_convert_fastq_se_ga(input, temp_dir, opts) -> Result<ConvertedReads>;     // pbat SE: G→A
// non-dir SE = call _se (C→T) + _se_ga (G→A). PE: add an explicit per-(library,read_number) ConvKind selector
// (a `pbat: bool`/`library` arg on the PE converter, or a free `pe_conv_kinds(library, read_number) -> &[ConvKind]`)
// so pbat's R1→G→A / R2→C→T inversion and non-dir's both-per-mate are explicit, NOT a silent reuse of directional.

// lib.rs — generalized drivers (directional folds in).
fn run_se(config: &RunConfig, reads: &[String]) -> Result<()>;
fn run_pe(config: &RunConfig, mates1: &[String], mates2: &[String]) -> Result<()>;
// internal per-mode instance plan: Vec<(usize /*slot*/, Orientation, &Path /*index*/, ...reads)>.
```

## 5. Implementation outline (TDD)
1. **`convert.rs`**: add the G→A SE entry + the non-dir/pbat PE entries (all via `convert_fastq_impl`). Unit-test
   the byte output + filenames (G→A SE; pbat PE R1 G→A `/1/1` + R2 C→T `/2/2`; non-dir per-mate both).
2. **`lib.rs` SE**: generalize `run_se_directional` → `run_se` with the §3.2 instance plan; directional path
   must stay byte-identical (the gated SE test suite is the regression guard). pbat passes `pbat=true` to drive_merge.
3. **`lib.rs` PE**: generalize `run_pe_directional` → `run_pe` with the §3.3 slot plan. directional path byte-frozen.
4. **Dispatch**: the 2 new arms (§3.4); shrink the `_ =>` "later phase" message to FastA/threading only.
5. **Temp cleanup** — 🔴 **rev1 (A): an EXPLICIT per-mode task** (it is byte-invisible, so NO gate/diff catches an
   omission): SE pbat = G→A; SE non-dir = C→T + G→A; PE pbat = G→A_1 + C→T_2; PE non-dir = all 4 (Perl 1974–1999 / 2154–2181). Best-effort.
6. **Tests** (§7) — conversion unit tests; SE/PE non-dir + pbat integration tests. 🔴 **rev1 (both reviewers,
   load-bearing): the existing `tests/cli.rs` fake bowtie2 only emits a mapped hit on the `*BS_CT*` index** — so a
   non-dir/pbat/CTOT/CTOB test would **silently pass on all-unmapped**. New fakes MUST emit mapped hits on the
   `*BS_GA*` index too (and for pbat, on the complementary strands), and the tests must **byte-assert the resulting
   SAM/XM** on the first-live paths (the `methylation_call` GA branch, the CTOT/CTOB SE FLAG arms, the PE index-1/2
   records) — these are exercised for the FIRST time in Phase 8 and the directional-data oxy gate lands ~0 reads on
   them, so the integration tests (not the gate) are the load-bearing proof. Plus report-bytes-per-mode + the oxy gate.

## 6. Efficiency
Non-dir doubles the instances (4 vs 2) + the converted files (2 vs 1 per mate) — inherent to the mode (matches
Perl). No new genome passes. pbat is the same cost as directional (2 instances). mimalloc already global.

## 7. Validation
| # | Verify | How | Expected |
|---|--------|-----|----------|
| 1 | SE G→A / non-dir / pbat PE / non-dir PE conversion bytes + filenames | unit | per Perl branches (G→A; pbat R1 G→A `/1/1`, R2 C→T `/2/2`; non-dir both-per-mate) |
| 2 | SE non-dir spawns 4 instances at slots 0–3 (CT/norc, GA/nofw, CT/nofw, GA/norc) reading C→T/C→T/G→A/G→A | integration — 🔴 **fake bt2 MUST emit mapped hits on `*BS_GA*` too** (not just `*BS_CT*`) else false-pass on all-unmapped; **byte-assert the GA-branch SAM/XM** | 4 streams; no rejection; strand counts across all 4; SAM/XM byte-correct |
| 3 | SE pbat: 2 instances reading G→A, `pbat=true` → eff index 2/3 → CTOT/CTOB | integration | counts in `ga_ct`/`ga_ga`; FLAG/XR/XG per CTOT/CTOB |
| 4 | PE non-dir: 4 slots populated with the right `-1`/`-2`; all kept | integration | 4 instances; report 4 strand rows |
| 5 | PE pbat: slots 1,2 (G→A_1/C→T_2); no modifier | integration | strands CTOB/CTOT |
| 6 | non-dir: NO wrong-strand rejection (a complementary-strand best is written) | unit/integration | rejected-count absent; read written |
| 7 | Report per mode: pbat/non-dir library line; rejected-count line **omitted** for non-dir/pbat | unit (bytes) | matches Perl 1711–1719/1940–1948; no rejected line |
| 8 | 🔴 **rev1 (A)** Directional SE+PE unchanged through the `run_se`/`run_pe` generalization | existing gated suite + the PR-#930 directional oxy gate | byte-frozen (the load-bearing regression guard — run it BEFORE wiring non-dir/pbat) |
| 9 | 🎯 **oxy gate**: `10M_SE` `--non_directional` + `--pbat`; `10M_PE` `--non_directional` + `--pbat`, identical argv | a **Phase-8 gate script** using the Phase-7 harness pattern (identical argv into the SAME `--output_dir`, Perl moved aside; samtools-`@PG` + wall-clock filtered) — NB `pe_gate.sh` was an ephemeral Phase-7 local script, not in-repo | byte-identical to Perl v0.25.1 (BAM + report + aux), all 4 mode×layout cells |

## 8. Assumptions
**From epic:** Perl v0.25.1 + Bowtie 2 2.5.5 oracle; byte-identity on decompressed content; adjudicate on
Linux/oxy; identical argv. Strand-instance table fixed (SPEC §1). **Phase-specific:** FastQ only (FastA = Phase 9);
single-core (multicore = Phase 9); the CLI/config/merge/SE-pbat-modifier/report are already built (verify-only);
the gate reuses the existing `10M_SE`/`10M_PE` datasets with the mode flags (a true non-dir/pbat library would
exercise more strands but is not required for the byte-identity contract — same reads, same mode, both tools).

## 9. Questions or ambiguities
- **(Open Q1 — RESOLVED, Felix 2026-06-02: generalize in place)** `run_se_directional`/`run_pe_directional` →
  `run_se`/`run_pe` with a per-mode instance plan (far less duplication; the gated directional path + the green
  PR-#930 CI are the regression guard). The directional output must stay byte-identical through the generalization.
- **(Open Q2 — RESOLVED by source)** pbat SE reuses the 2-slot array + the `+2` index modifier (already built,
  Phase 5); pbat PE physically populates slots 1,2 (no modifier). Confirmed (Perl 4308–4311 SE-only).
- **(Open Q3 — RESOLVED, Felix 2026-06-02: reuse with mode flags)** Gate on `10M_SE`/`10M_PE` with
  `--non_directional`/`--pbat` (4 mode×layout cells) for the byte-identity contract (same reads, same mode, both
  tools). GATE_OXY will note that a native non-dir/pbat library would exercise more complementary-strand alignments
  (deeper, optional — not required for byte-identity).
- **(Open Q4)** pbat conversion entry-point shape (one fn per (mode,mate) vs a `Conversions` helper). *Assumption:*
  small explicit entry points reusing `convert_fastq_impl` (simplest; matches the SE/PE directional style).

## 10. Self-Review
- **Logic:** the 3-mode model (two booleans; pbat = which-instances + SE `+2` modifier; merge identical) traced to
  Perl (agent-surveyed 5489–6025 + 7096–7244 + 519–546/394–452 + 4308–4311 + 3112–3118/3851–3857). The reject is
  already gated; the report already 3-way. ✓
- **Edge cases:** pbat index-modifier → CTOT/CTOB; non-dir no-reject; the config-level pbat⊕gzip/fasta + non_dir⊕pbat
  dies (already enforced); cross-instance ambiguity in non-dir. ✓
- **Integration:** reuses `convert_fastq_impl`, the generic merge (N streams + gated reject), SE `pbat_mod`, the
  report library-line + gated rejected line, the Phase-7 PE slot-indexed merge + `PeSinks`. New = conversion
  variants + the driver instance plans + 2 dispatch arms. SE/PE **directional** paths byte-frozen (regression guard). ✓
- **Risks:** (1) the SE non-dir 4-slot ordering + which file each reads (table §3.2 — easy to mis-wire); (2) the PE
  slot→file mapping per mode (§3.3); (3) keeping directional byte-identical through the generalization (the existing
  gated tests + the oxy directional gate are the guard); (4) pbat SE counters landing in CTOT/CTOB via the modifier.
  All unit/integration-pinnable before the oxy gate.

## 11. Revision History
- **rev 1 (2026-06-02)** — folded dual plan-review (`PLAN_REVIEW_A.md` 0C/4I/5O, `PLAN_REVIEW_B.md` 0C/3I/4O; both
  APPROVE-WITH-FINDINGS, **no contradictions, no Criticals**; both independently verified every slot/index/orient/file
  cell of §3.2/§3.3 and all six "already-built" claims — incl. the pbat-SE `+2` modifier end-to-end). Folded findings
  (all about coverage/precision, not wiring):
  - 🔴 **(both, load-bearing) test fakes:** the existing `tests/cli.rs` fake bt2 only emits on `*BS_CT*` → non-dir/pbat
    tests would silently pass on all-unmapped. New fakes MUST emit on `*BS_GA*` + the complementary strands, and
    byte-assert the SAM/XM on the first-live paths (GA `methylation_call`, CTOT/CTOB SE FLAGs, PE index-1/2). The
    integration tests — NOT the directional-data oxy gate — are the load-bearing proof (§5 step 6, §7 #2).
  - **(B I-1) PE conversion must be library-aware** — don't overload the read-number-hardcoded directional fn; explicit
    per-(library, read_number) ConvKind (§3.1, §4).
  - **(A) per-mode temp cleanup = an explicit task** (byte-invisible → no gate catches an omission) (§5 step 5).
  - **(A) directional regression bar** — run the gated suite + the PR-#930 directional oxy gate BEFORE wiring
    non-dir/pbat through the `run_se`/`run_pe` generalization (§7 #8).
  - **(nit, both) `pe_gate.sh`** was an ephemeral Phase-7 local script, not in-repo → §7 #9 now specifies a Phase-8
    gate script using the harness pattern.
  - Open Q1 (generalize in place) + Q3 (reuse `10M_SE`/`10M_PE` with mode flags) RESOLVED by Felix.
  Optionals (A/B): byte-level assertions per first-live arm, "Now running N instances" stderr parity — folded into §7.
- **rev 0 (2026-06-02)** — initial plan, after orienting on the Perl non-dir/pbat branches (agent-surveyed) + the
  existing Rust (CLI/config/merge/SE-pbat-modifier/report already built). Surfaces that Phase 8 is mostly *wiring*
  (conversion variants + per-mode driver instance plans); recommends generalizing the directional drivers in place.
  Awaiting manual review → (after approval) dual plan-review → implement trigger.

## 12. Implementation Notes (2026-06-02)

**Status: COMPLETE + GATED.** All unit + integration tests green (211); clippy `-D warnings` + `cargo fmt --check` clean.
Dual `/code-reviewer` → both **APPROVE** (`CODE_REVIEW_A.md`/`_B.md`; both re-derived the GA-branch XM bytes from Perl).
`/plan-manager` → **COMPLETE** (`COVERAGE.md`, all 7 items DONE). **oxy byte-identity gate ✅ PASS** (`GATE_OXY.md`):
all 4 cells (SE/PE × non-dir/pbat) byte-identical to Perl v0.25.1 + Bowtie 2 2.5.5 at **10k AND 1M** (pe_nondir =
1,703,244 records). NOT committed (commit/PR on explicit ask).

### What was built (all in `rust/bismark-aligner/`)
- **`convert.rs`** — added `bisulfite_convert_fastq_se_ga` (SE pbat / non-dir G→A) and a **library-aware** PE core
  `bisulfite_convert_fastq_pe_kind(.., read_number, kind: ConvKind)` (rev1 B I-1). The directional
  `bisulfite_convert_fastq_pe` now **delegates** to it with the fixed read#→kind map (R1=Ct/R2=Ga) — byte-frozen
  (the existing directional PE convert tests stayed green). Helpers `file_base_for(kind)` (`_C_to_T`/`_G_to_A`) +
  `pe_id_suffix(read_number)` (`/1/1`,`/2/2`). `ConvKind` stays `pub(crate)`.
- **`lib.rs`** — `run_se_directional`→**`run_se`** and `run_pe_directional`→**`run_pe`**, both driven by a per-mode
  instance plan built at a **single construction site** (reviewer A §5 / B Opt-4): `se_instance_plan(library)` returns
  `Vec<(Orientation, IndexChoice, file_idx)>` in Bismark slot order (so `enumerate` index == Perl `@fhs` index);
  `pe_instance_plan(library)` returns `Vec<(slot, Orientation, IndexChoice, kind_m1, kind_m2)>` placed into the
  length-4 `Vec<Option<_>>`. SE conversions via `convert_se_files`; PE converts each distinct `(mate, kind)` **once**
  (dedup — Perl makes 2 files for directional/pbat, 4 for non-dir) via `pe_lookup`. New enum `IndexChoice {Ct, Ga}`,
  banner helper `conv_label`. Dispatch (`pipeline`) now matches `(layout, format)` only → SE/PE FastQ for **all 3
  libraries**; the `_ =>` message shrunk to FastA/threading. **Per-mode temp cleanup** (rev1 A): both drivers now
  delete EVERY converted temp (1/2 for SE directional/non-dir, 2/4 for PE) — byte-invisible, asserted by tests.
- The SE-pbat `+2` modifier, the `directional`-gated reject, the FLAG/XR/XG, and the report are **unchanged** (already
  built + verified); Phase 8 only *wires* them — confirmed by the plan thesis holding under implementation.

### Tests (the load-bearing risk — rev1 §5 step 6, §7 #2–7)
- **convert.rs (4):** SE G→A bytes/name; PE pbat R1→G→A `/1/1` + R2→C→T `/2/2`; non-dir both-per-mate.
- **methylation.rs (4):** `extract` with **pbat=true** → eff 2 (ga_ct, '-', append+revcomp) and eff 3 (ga_ga, '+',
  prepend) — *no prior test passed pbat=true*; the **GA `methylation_call` branch** byte XM + context counters
  (methylated + converted) — *no prior GA-branch call test*.
- **output.rs (3):** `single_end_sam_output` CTOT (eff2 → FLAG 0, XR GA, XG CT, SEQ revcomp'd, XM reversed) + CTOB
  (eff3 → FLAG 16, XR GA, XG GA, SEQ/XM as-is), each via the **real** extraction+call; PE XR/XG for index 1/2.
- **tests/cli.rs (8):** **GA-emitting fakes** (`make_fake_bowtie2_ga_reads_{ct,ga}_index` SE, `..._pe_{ga,ct}_index`
  PE) that map only the G→A-converted reads on the chosen index — so the new strands actually MAP (each test asserts
  `unique best alignments: 1` + a written record, so it CANNOT false-pass on all-unmapped, the exact reviewer trap).
  pbat SE CTOT/CTOB; non-dir SE CTOT (index 2, no-rejection) / CTOB (index 3); pbat PE index 1/2; non-dir PE index 1
  (4-slot, no-rejection). Byte-assert FLAG/POS/SEQ/XR/XG/XM. Per-mode temp-cleanup assertions (1/2/2/4 files gone).
- **Directional regression guard (rev1 §7 #8):** all pre-existing SE+PE directional unit + integration tests stayed
  green through the `run_se`/`run_pe` generalization (byte-frozen).

### Totals: 183 lib + 28 integration = **211 tests** (was 193). Build clean; clippy `-D warnings` clean; fmt clean.

### Deviations from the plan
- None material. §4's "explicit per-(library, read_number) ConvKind" realised as `bisulfite_convert_fastq_pe_kind`
  (the directional fn delegates) rather than a free `pe_conv_kinds(..) -> &[ConvKind]` — same intent (no silent reuse
  of the directional read#→kind map), simpler call sites; the driver dedups conversions itself.
- Conversion STDERR banner is now one line **per converted file** (was a single combined PE line). Not byte-gated;
  no test asserts it. The directional SE "Created C->T converted" substring is preserved (existing test relies on it).

### Iteration log
- **#1** convert.rs variants + delegation refactor → 27 convert tests green (incl. directional regression).
- **#2** SE `run_se` generalization (instance plan as data) → build + 21 integ green; pbat SE now runs end-to-end.
- **#3** PE `run_pe` generalization (dedup conversions + slot plan) → full suite green; PE directional byte-frozen.
- **#4** unit tests (pbat eff2/3, GA branch, CTOT/CTOB SAM, PE idx1/2 XR/XG) → 183 lib green; hand-derived XM
  (`H.Z...` / `.z...H`) + SEQ (`GCGTAC`/`GTACGC`) confirmed by the real engine.
- **#5** integration GA-emitting fakes + byte assertions → 28 integ green (7 new).
- **#6** `cargo fmt --check` flagged long-line wraps (the separate CI gate — clippy-clean ≠ fmt-clean); applied
  `cargo fmt`, re-ran: 211 tests + fmt clean.
