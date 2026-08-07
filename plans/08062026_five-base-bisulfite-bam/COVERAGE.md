# Plan Coverage Report

**Mode:** B (code vs. plan — the design plan's §5 outline + §9 validation list used as the task ledger)
**Plan(s):** `plans/08062026_five-base-bisulfite-bam/PLAN.md` (rev 2)
**Implementation:** commit `6a3ea0d` (+ fixture commit `0e4c705`), branch `dev`
**Date:** 2026-08-07
**Verdict:** **INCOMPLETE — 22 items unresolved** (3 of them already self-reported in §10b; **19 are not**)

---

## Summary

- Total items: **100**
- DONE: **72**
- PARTIAL: **15**
- MISSING: **10**
- DEVIATED (documented): **3**

The **algorithm and the driver are complete**. Every behaviour clause in §3 exists in the code, in the form the plan specified, including all four §3.3 invariants, the §3.4 round-trip proof, the §3.5 flip-rate fail-loud, and all three conjuncts of §3.6's masking rule with the T2 counter placement. `cargo fmt --check` and `cargo clippy --all-targets` are clean (verified, not taken on report), and all 28 tests pass.

**Every open item is in validation, not in behaviour.** §10b's self-report is accurate about the three things it admits to, but it is **not complete**: it does not mention that §9.1's fixture list was reduced from five fixture groups to one, that §9.1's required CIGAR census assertion was not written, that four of §9.6's rows have no test, that two of §9.7's seven sub-assertions are absent, that `U`/`u` (§3.9, which has a whole plan section arguing for it) is exercised nowhere, or that two of §5.9's three chromosome-naming warnings were not written.

---

## Coverage ledger

### §5 Implementation outline

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | `cli.rs`: `five_base_bisulfite_bam: Vec<PathBuf>`, doc-commented, clippy-safe `///` continuations, one new flag only | §5.1 | DONE | `cli.rs:184-195`. No `--five_base_bisulfite_baseq`, per T1 |
| 2 | Mutual-exclusion check **before** the `:165-168` dispatch, then the new guard | §5.2, R14 | DONE | `mod.rs:166-181`; check precedes both `return`ing dispatches |
| 3 | Require `--illumina_5base`; explicitly do **not** require `--genome` | §5.3 | DONE | `mod.rs:634-640`; no genome load anywhere on the path |
| 4 | New module: `XgStrand`, `FiveBaseBisulfiteError`, `reencode()`, `reconstruct_ref()`; `reconstruct_ref` written first and tested in isolation; **no `@PG` parser** | §5.4 | DONE | `five_base_bisulfite.rs`; 6 dedicated `reconstruct_ref` unit tests ahead of the re-encode tests |
| 5 | Clone to `RecordBuf`, `sequence_mut()`, `data_mut().insert(NM/MD)` (in-place, order-preserving) | §5.5, R16 | DONE | `mod.rs:836-844`, with the order-preservation comment |
| 6 | Driver: per input — open, copy header, append `@PG` with a **distinct ID** (`ID:bismark-five-base-bisulfite`, `PP:Bismark`), stream, write `<output_dir>/<stem>.bisulfite.bam`; **state** the idempotent-on-own-output property | §5.6, T6 | PARTIAL | Everything present and tested except the plan's "stated property, not a coincidence": nothing in code or docs says the output retains `@PG ID:Bismark` and so the converter is idempotent on its own output |
| 7 | Counters + report: read / re-encoded / passed through, `letters`, `flipped`, flip rate, `masked`; fail loud off {0,1}; §3.6 `Note:`; `<stem>.bisulfite_report.txt` | §5.7 | DONE | `mod.rs:846-880`; all six counters in the report |
| 8 | Closing `Note:` with `samtools sort`/`index` + `bam2pat`, framed as **mandatory** | §5.8 | DONE | `mod.rs:861-870`; states `bam2pat` refuses non-`SO:coordinate` |
| 9 | Chromosome-naming note: (a) naming must match `init_genome`, total vs **partial** intersection; (b) wgbstools' CpG dictionary comes from **its own FASTA**, so a patch/build skew is silent; (c) `patter --clip` operates on the **cleaned, reference-space** sequence, not read cycles | §5.9, R15 | PARTIAL | (a) present in both the runtime note (`mod.rs:866-870`) and the docs. **(b) and (c) appear nowhere** — not in the note, not in `illumina-5-base.md`, not in the CHANGELOG |
| 10 | Tests — §9 | §5.10 | PARTIAL | See §9 rows below |
| 11 | Docs + `CHANGELOG.md` + `rust/README.md` Milestones | §5.11 | DONE | New "Interop" subsection under Advanced modes; CHANGELOG entry; dated Milestones line |
| 12 | Pre-push gates: `cargo fmt -p bismark -- --check`; `cargo clippy -p bismark --all-targets` | §5.12 | DONE | **Re-run in this audit: both clean.** The feature-gated `rammap-inprocess` variant is not applicable — no feature-gated code touched |

### §3 Behaviour

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 13 | `len(XM) == len(SEQ)`, `XM[i]` ↔ `SEQ[i]` in BAM space | §3.1.1 | DONE | Enforced at `reencode()` entry; module doc records the lockstep-revcomp derivation |
| 14 | `XG` alone fixes ref base and the `(meth, unmeth)` pair: `CT`→`C`,`(C,T)`; `GA`→`G`,`(G,A)` | §3.1.2 | DONE | `XgStrand` + `xg_fixes_the_pair_and_the_reference_base`; doc records that it is **emergent** from `XR` (R12) |
| 15 | Soft clips and insertions are structurally `.`; a positional zip cannot touch them | §3.1.3 | DONE | `ref_seq` carries `b'X'`; two `gaps_are_never_written_*` tests |
| 16 | Per-record loop exactly as specified; unknown `XM` byte ⇒ `Err(InvalidXmByte)` | §3.2 | DONE | `five_base_bisulfite.rs:430-482` |
| 17 | **Flip counter inside the two letter arms**, never at loop level | §3.2, T2 | DONE | `:468-470`, inside `Some(base)`. `masks_only_no_call_reference_cytosines` asserts `flipped == 0` on a masked record, so the loop-level regression is caught |
| 18 | All eight letters rewritten, **including `U`/`u`** | §3.2, §3.9 | PARTIAL | Code includes `b'U'`/`b'u'` (`:432-433`). **Nothing exercises them**: no unit test uses `U`/`u`, and the committed fixture's letter census is `Z=37, x=38, h=75` — no `U`, `u`, `X`, `H`, or `z` anywhere. §3.9 exists specifically because dropping `U`/`u` yields a silently half-converted file; that removal would break no test |
| 19 | Assert `len(XM) == len(SEQ)` explicitly (R5 removed the free reader check) | §3.3.1 | DONE | + `rejects_an_xm_seq_length_mismatch` |
| 20 | Assert `seq_old[i] ∈ {meth, unmeth}` at **every** letter position | §3.3.2, R9 | DONE | `SeqNotInPair`, + `rejects_a_letter_whose_base_is_outside_the_pair`; also confirmed on the fixture at generation time |
| 21 | Assert no `XM` letter at an `I`/`S` position | §3.3.3 | DONE | `CallInGap` via `ref_seq[i] == b'X'`, + `rejects_a_call_in_a_gap` |
| 22 | The round-trip proof as a per-record invariant | §3.3.4, §3.4 | DONE | `:404-421`, before anything is emitted |
| 23 | Step 1: reconstruct `ref_seq` (M→genomic, I/S→`X`, D excluded) and `md_seq` (M+D, `X` at I/S, genome-forward) | §3.4 | DONE | `reconstruct_ref`; 6 unit tests incl. the `md_seq`-spans-M+D case |
| 24 | Step 2: `hemming_dist + Σ(D)` == `NM_old` **and** `make_mismatch_string` == `MD_old`, else hard error naming the QNAME | §3.4 | DONE | Both checks; `RoundTripFailed` names the record and says nothing was written |
| 25 | Step 3: emit `NM_new`/`MD_new` computed on the **final** `seq_new` | §3.4, §3.6 | DONE | `:485-490`, after masking |
| 26 | Preserve Bismark's `NM` identity (mismatches + inserted + **soft-clipped** + deleted), do not silently "fix" it | §3.4, R3 | DONE | Reuses `hemming_dist`/`make_mismatch_string`; `bismark_nm`'s doc comment states the identity correctly |
| 27 | Flip rate as a **file** rate (not a per-record mean); fail loud on a value that is neither 0 nor 1 | §3.5, R7 | DONE | `mod.rs:848-860`; reported to 6 dp |
| 28 | Mask `b'N'` iff `XM[i]=='.'` **and** `ref_seq[i]==ref_base` **and** `SEQ[i] ∈ {meth,unmeth}` | §3.6, T1 | DONE | `:474-479`, all three conjuncts, no `QUAL`/`baseq`/`phred64` parameter anywhere (T3 moot) |
| 29 | `NM`/`MD` computed **after** masking | §3.6 | DONE | See #25 |
| 30 | Report `masked`; print a one-line `Note:` naming `--five_base_baseq` when non-zero | §3.6 | DONE | Report row + `mod.rs:854-860` |
| 31 | Note that non-CpG cytosines are masked too, harmlessly, so a later reader does not read it as a bug | §3.6, T8 | PARTIAL | Stated in the plan; **not** in the module/function docs, which describe the rule without mentioning that it deliberately over-covers non-CpG cytosines |
| 32 | Leak lives in `five_base_emit_pe_record`, not the SE helper; consensus BAM carries no leak and `masked == 0` there follows from property 2 | §3.6.6, T5, T9 | DONE | Plan-text requirements; no code claim contradicts them |
| 33 | Unmapped (`0x4`): `write_raw_record` unchanged, counted | §3.7, R5 | PARTIAL | Code correct (`mod.rs:757-777`) and raw `noodles_bam::io::Reader` used precisely so they survive. **No test exercises a mixed mapped+unmapped BAM** — `filter_nonconversion/se_unmapped/in.bam`, named by §9.1 for exactly this, is unused; the uBAM test hits the D3 guard instead |
| 34 | Secondary/supplementary: pass through + **one** aggregated warning | §3.7 | PARTIAL | `warned_secondary` latch implemented correctly; untested |
| 35 | Mapped primary requires `XM`,`XR`,`XG`,`MD`,`NM` — any missing ⇒ fail loud naming **file + QNAME** | §3.7 | DONE | `ctx()` closure carries `bam.display()` + `qname`; `XR` required though unused, with a comment saying why |
| 36 | CIGAR: handle `M`/`I`/`D`/`S`/`N`; **reject `=`, `X`, `P`, `H` explicitly** rather than the `cigar_to_ops` fall-through | §3.7, R13 | DONE | `reconstruct_ref`'s `other =>` arm returns `UnsupportedCigarOp`; `parse_cigar` passes any op byte through, so all four reach it. `N` is a no-op run as specified |
| 37 | `XG` ⟺ FLAG equivalence documented | §3.8, R8 | DONE | Plan §3.8 with the SE and all four PE cases and the `#1030` swap. (The **test** is item #71) |
| 38 | §3.10 table: length mismatch, bad `XM` byte, deletion, empty BAM (header-only + `Note:`, exit 0), bisulfite ⇒ identical SAM text | §3.10 | PARTIAL | Four rows DONE. **Empty BAM**: behaviour is correct (the D3 guard is gated on `n_read > 0`, header-only output written, exit 0) but there is no empty-input-specific `Note:` — the report's verdict line reads "no methylation calls" — and no test |

### §9 Validation

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 39 | Idempotence gate: bisulfite BAM in ⇒ **identical SAM text** out, modulo the added `@PG`; decompressed bodies compared | §9.1, R10 | DONE | `bisulfite_input_round_trips_to_identical_sam_text`; passes |
| 40 | Fixtures: `synth_barcode_10k…_pe.bam` (12974 rec, 42 with `I`/`D`), **`nondir_pe_1030.bam` (all four strand indices)**, `filter_nonconversion/{se,pe}_default/`, `se_unmapped/in.bam`, plus the new soft-clip fixture | §9.1, R2 | **MISSING** | **Only `five_base_bisulfite/softclip_indel_se.bam` is used.** All four other fixtures exist on disk and none is referenced by any test. Consequences: no PE record ever passes through the converter; **no non-directional input, so only 2 of the 4 strand indices are exercised** — the plan states plainly that without it "the 'whole encoding table' claim is false"; no unmapped pass-through; and the 12974-record real-BAM scale run is absent. Not mentioned in §10b |
| 41 | Assert the **CIGAR census** in test setup so a future fixture swap cannot silently drop coverage | §9.1 | **MISSING** | The census is written in `fixture()`'s doc comment and in the fixture `README.md`; nothing asserts it. A fixture swap would silently void §9.4 and §9.5's shapes. Not mentioned in §10b |
| 42 | Forward check 1: at every letter position `seq_new[i] == meth` iff `XM[i]` is upper-case, per the `XG` table | §9.2, R6 | DONE | Asserted on `SEQ` (not via `extract_calls`) as exact byte strings across CT and GA, upper and lower; the fixture round trip covers `x`/`h` on both strands |
| 43 | Forward check 2 — **direction pin**: at least one `T→C` (`CT`) or `A→G` (`GA`) on 5-Base input | §9.2, R6 | DONE | `five_base_input_flips_every_letter` (T→C), `ga_strand_uses_the_g_a_pair` (A→G) |
| 44 | Forward check 3 — flip rate asserted **exactly**: `flipped == letters` on 5-Base, `flipped == 0` on bisulfite | §9.2, R7 | DONE | Unit `assert_eq!(out.flipped, out.letters)`; integration `flip rate\t0.000000` |
| 45 | Forward check 4: `XM` unchanged | §9.2 | DONE | `xm_is_left_untouched` compares all 8 tags |
| 46 | OB-strand orientation trap: a `-`-strand record with an **asymmetric, mixed-case** `XM`, exact expected `SEQ`, built so a `read_pos_5p` write differs detectably | §9.3 | PARTIAL | Split across two records, neither matching the spec: `ga_strand_uses_the_g_a_pair` is GA with exact-`SEQ` assertion but **symmetric and single-case**; `asymmetric_calls_edit_the_correct_positions` is mixed-case but `XgStrand::Ct` (forward), where a `read_pos_5p` write is indistinguishable — and its doc comment mislabels it "a reverse-strand-shaped record". A GA record with mixed-case letters at both ends does not exist. The fixture's `ot_softclip`/`ob_softclip` pair covers the bug on real data via the gate |
| 47 | Gaps: `{n}S{m}M` and `{a}M{b}I{c}M` untouched; fault-injected letter at an `S` position ⇒ `CallInGap` | §9.4 | DONE | `gaps_are_never_written_soft_clip`, `gaps_are_never_written_insertion`, `rejects_a_call_in_a_gap` (+ the real fixture, as §9.4 required) |
| 48 | Independent **from-genome** `NM`/`MD` oracle, built without `MD_old`; priority shapes: leading soft clip, insertion, mismatch abutting a deletion, multi-deletion | §9.5 | DEVIATED | **Admitted in §10b** with a reason (the per-record round-trip proof runs on every record of every input, incl. the deletion path; the fixture confirms the `NM` identity on real data). `pUC19.fa` was committed for it and is unused |
| 49 | Fail-loud: missing `XM` | §9.6 | **MISSING** | No test reaches the tag-missing errors. The uBAM's records are all unmapped, so they take the pass-through and the **D3** guard fires — hence the test's `contains("none could be re-encoded") \|\| contains("XM")` disjunction, which passes on the first branch. Not mentioned in §10b |
| 50 | Fail-loud: missing `XR` | §9.6 | **MISSING** | As #49 |
| 51 | Fail-loud: missing `XG` | §9.6 | **MISSING** | As #49. `BadXg` (a *wrong* value) is also untested |
| 52 | Fail-loud: missing `MD` | §9.6 | **MISSING** | As #49 |
| 53 | Fail-loud: missing `NM` | §9.6 | **MISSING** | As #49 |
| 54 | Fail-loud: `len(XM) != len(SEQ)` | §9.6 | DONE | `rejects_an_xm_seq_length_mismatch`, asserts the QNAME is in the message |
| 55 | Fail-loud: bad `XM` byte, naming QNAME + offset | §9.6 | DONE | `rejects_an_unknown_xm_byte` |
| 56 | Fail-loud: `H` in CIGAR | §9.6 | DONE | `ref_rejects_unsupported_cigar_ops` |
| 57 | Fail-loud: `=`, `X`, `P` in CIGAR | §9.6 | **MISSING** | Only `H` is tested. Same code arm, but the plan lists all four and this is the arm that replaces `cigar_to_ops`' silent fall-through. Not mentioned in §10b |
| 58 | Fail-loud: letter at an `I`/`S` position | §9.6 | PARTIAL | `S` tested; `I` not (identical code path) |
| 59 | Fail-loud: round-trip failure (§3.4 step 2) | §9.6 | PARTIAL | `rejects_a_record_whose_tags_fail_the_round_trip` covers the **`NM`** branch; the `MD` branch has no test |
| 60 | Fail-loud: flip rate strictly between 0 and 1 | §9.6 | **MISSING** | The guard exists (`mod.rs:850-860`) and is the only defence against a half-converted file; nothing tests it. Not mentioned in §10b |
| 61 | Fail-loud: both `--five_base_*_from_bam`-family flags together | §9.6 | DONE | `rejects_both_standalone_bam_modes_together`, asserts "mutually exclusive" |
| 62 | Fail-loud: missing `--illumina_5base` | §9.6 | DONE | `requires_illumina_5base`, asserts the message names the flag |
| 63 | Assert the output contains **no `MM:Z:`** (`detect_nanopore` would silently switch modes) | §9.6, R17 | **MISSING** | No test, no assertion, no comment anywhere. Not mentioned in §10b |
| 64 | Masking 1 — **positive**: a `.` at a reference cytosine with a scoreable base becomes `N`; `masked == 1` **exactly** | §9.7, T4 | DONE | `masks_only_no_call_reference_cytosines` |
| 65 | Masking 2 — **negative**: in the *same* record, a `.` at a reference **non**-cytosine whose base is `C`/`T` stays byte-identical | §9.7, T4 | DONE | Same test: offset 3 carries `T` at reference `T`; the exact-`SEQ` assertion `ANGT` fails if the positional conjunct is dropped |
| 66 | Masking 3 — gaps never written (soft-clipped and inserted positions with scoreable bases) | §9.7 | DONE | Both `gaps_are_never_written_*` tests assert the contrast against a genuine uncalled reference C in the same record |
| 67 | Masking 4 — the `is_cpg` neighbour survives: `G` at `j+1` on OT, **and symmetrically `C` at `j-1` on OB** | §9.7, T4 | PARTIAL | OT half is asserted byte-exactly (`ANGT` keeps the `G`). **The OB half does not exist** — no GA/`XgStrand::Ga` record anywhere in the suite has a masked position, so the symmetric case is untested in either direction. Not mentioned in §10b |
| 68 | Masking 5 — **disjointness**: assert `flipped <= letters` and `flipped/letters ∈ {0,1}` on a record that **also** has masked positions | §9.7, T2 | PARTIAL | The defect T2 targets is caught (`masks_only_no_call_reference_cytosines` asserts `flipped == 0` where a loop-level counter would give 1), but that record has `letters == 0`, so no rate is formed. `gaps_are_never_written_soft_clip` *is* a mixed record (`letters=1, masked=1`) and asserts neither `flipped` nor `letters`. The specified assertion is absent |
| 69 | Masking 6 — `NM` bookkeeping: a 100 bp record with **two** masked positions gives `NM_new == NM_old + 2` | §9.7 | PARTIAL | Asserted in the weaker `n = 1` form on a 4 bp record (`nm == 1` from `nm_old = 0`, `md == "1C2"`). The two-mask/100 bp form is absent |
| 70 | Masking 7 — provable emptiness: `masked == 0` on **every** §9.1 bisulfite fixture | §9.7, T1 | PARTIAL | Asserted on the one fixture that is used, from the report (`no-call cytosines masked to N\t0`), plus a unit test. "Every §9.1 fixture" is bounded by item #40 |
| 71 | `XG == "CT"` ⟺ `FLAG ∉ {16,83,163}` over `nondir_pe_1030.bam` | §9.8, R8 | **MISSING** | **Admitted in §10b** ("Worth adding"). Confirmed absent |
| 72 | End-to-end concordance: convert → sort → index → `bam2pat` vs Bismark's cytosine report, two arms (`--five_base_baseq 0` and `> 0`) | §9.9 | DEVIATED | **Admitted in §10b** (needs real data). §10b reports a *stronger* substitute for the sign — stock `patter` 0.0 % → 100.0 % on a fully-methylated pUC19 5-Base control, agreeing with the independent `patter` patch. The **masked (`> 0`) arm** is not covered by that substitute |

### Rev-1 changes (R1-R19)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 73 | R1 masking leak closed | DONE | Superseded in mechanism by T1; the leak is closed |
| 74 | R2 fixtures corrected (uBAMs cannot run the gate) | DONE | The uBAM is used only as a negative fixture, which is correct |
| 75 | R3 `NM` prose corrected (includes insertions **and** soft clips) | DONE | `bismark_nm`'s doc comment states it correctly and says not to "fix" it |
| 76 | R4 reuse `make_mismatch_string` + `hemming_dist`, per-record round-trip proof | DONE | Both called; no new `MD` emitter written |
| 77 | R5 raw noodles reader + `write_raw_record`; parity check re-added explicitly | DONE | Driver comment states why `BamReader` is not used |
| 78 | R6 §9.2 asserts on `SEQ`, not via `extract_calls` | DONE | Items #42-45 |
| 79 | R7 flip-rate invariant | DONE | Items #27, #44 |
| 80 | R8 `XG` ⟺ FLAG **documented and tested** | PARTIAL | Documented; the test is item #71 (MISSING, admitted) |
| 81 | R9 `seq_old ∈ {meth,unmeth}` assertion | DONE | Item #20 |
| 82 | R10 "identical SAM text", not byte-identical | DONE | The gate compares `samtools view` bodies, with the reason in a comment |
| 83 | R11 gate passes `--illumina_5base` on bisulfite input, **with a comment** | PARTIAL | `run_converter` passes it, so the gate works; there is no comment at the call site explaining why a bisulfite fixture is run under a 5-Base flag. The rationale does exist in `cli.rs` and in the validation error text |
| 84 | R12 §3.1.2 derivation written out (`XR` not `XG`) | DONE | In the plan and in the module doc |
| 85 | R13 CIGAR `N` handled; `=`/`X`/`P` rejected explicitly | DONE | Item #36 (the *test* is #57) |
| 86 | R14 dispatch order fixed | DONE | Item #2, + a test |
| 87 | R15 chromosome note rewritten (partial intersection is the silent hazard) | PARTIAL | Item #9 — the partial-intersection point landed; the CpG-dictionary and `--clip` points did not |
| 88 | R16 record-mutation route documented | DONE | Item #5 |
| 89 | R17 assert no `MM:Z:` | **MISSING** | Item #63 |
| 90 | R18 notes: consensus BAM is valid input; header **copied deliberately** (unlike the precedent, which synthesises); `MD`/`NM` churn is total; `--ds_test` still works | PARTIAL | Plan text complete; docs state the `--ds_test` point. The code comment says only "Copy the input header and append our own `@PG`" — it does not record that copying is deliberate and must not be "fixed" into synthesising, which is what R18 asked for |
| 91 | R19 "drop `MD`/`NM`" alternative recorded and priced | DONE | §10 Open-5, incl. that `MD` is now doubly load-bearing under T1 |

### Rev-2 changes (T1-T9)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 92 | T1 mechanism replaced with the three-conjunct reference-base rule; no CLI flag, no `@PG` parser | DONE | Item #28. `reencode`'s signature has no `qual`/`baseq`/`phred64`; grep confirms no `@PG CL:` parsing on the path |
| 93 | T2 flip counter in the letter arms; `flipped` and `masked` disjoint | DONE | Item #17 (the specified §9.7.5 assertion is #68, PARTIAL) |
| 94 | T3 `QUAL` units bug moot — read no `QUAL` at all | DONE | No `QUAL` access anywhere in the module or the driver |
| 95 | T4 §9.7 rewritten with negative controls | PARTIAL | 1, 2, 3 and 7 present; 4 half-present; 5 and 6 in weaker forms. See #64-70 |
| 96 | T5 PE call site cited (`mod.rs:1695-1697`) | DONE | Plan-text |
| 97 | T6 appended `@PG` gets a distinct ID | DONE | `ID:bismark-five-base-bisulfite`, + `appends_a_pg_with_a_distinct_id`, which also asserts no duplicate IDs |
| 98 | T7 B's proposal reframed as "composed with" | DONE | Plan-text |
| 99 | T8 `N` is `patter`'s own idiom; non-CpG cytosines masked too, harmlessly | PARTIAL | Plan-text DONE; item #31 — neither point is in the code, where the second one is what a later reader would question |
| 100 | T9 consensus BAM restated (no leak; `masked == 0` from property 2) | DONE | Plan-text |

---

## Gaps in detail

### Item 40: §9.1's fixture list reduced from five groups to one — **not admitted in §10b**

**Expected:** the gate runs over `dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` (12974 records, 42 with `I`/`D`), `dedup/nondir_pe_1030.bam` (**the only fixture with all four strand indices**), `filter_nonconversion/{se_default,pe_default}/`, `filter_nonconversion/se_unmapped/in.bam` (unmapped pass-through), **and** the new `five_base_bisulfite/softclip_indel_se.bam`.

**Found:** only `softclip_indel_se.bam` — 8 SE records. All four other fixtures exist on disk (verified) and are referenced by no test in `aligner_five_base_bisulfite.rs`.

**Gap:** four things the plan required are consequently unexercised end-to-end:
- **No paired-end record ever passes through the converter.** 5-Base is paired-end only (`cli.rs:99-100`), so the production shape is untested at the driver level.
- **Only 2 of 4 strand indices.** The fixture is directional SE (`XG:CT` FLAG 0, `XG:GA` FLAG 16). CTOT/CTOB never appear, so the §3.1.2 encoding table is half-tested — the case the plan called out by name.
- **Unmapped pass-through (§3.7) is untested on a mixed BAM.** The only test that touches the pass-through path is the uBAM one, where *every* record is unmapped and the run correctly fails via D3.
- **No scale run.** 12974 records with 42 `I`/`D` records vs. 8 records.

### Item 41: §9.1's CIGAR census assertion — **not admitted in §10b**

**Expected:** "Assert the CIGAR census in test setup so a future fixture swap cannot silently drop coverage."
**Found:** the census is prose in `fixture()`'s doc comment and in the fixture `README.md`.
**Gap:** no assertion. Replacing the fixture with one lacking `S`/`I`/`D` would leave every test green while voiding items #47 and #66.

### Items 49-53: none of §9.6's five missing-tag rows is reached — **not admitted in §10b**

**Expected:** a fail-loud test per required tag (`XM`, `XR`, `XG`, `MD`, `NM`), asserting on **message content**.
**Found:** `rejects_input_without_bismark_tags` feeds a uBAM whose every record is unmapped. Those take the verbatim pass-through, so no tag lookup happens; the error raised is the **D3** "none could be re-encoded" guard. The test's assertion is `err.contains("none could be re-encoded") || err.contains("XM")` and passes on the first disjunct.
**Gap:** the `ctx()` error path (`mod.rs:797-830`) — including its file+QNAME message contract — is never executed by any test. The plan's own framing applies: these are the errors a user meets when they point the flag at a non-Bismark aligned BAM.

### Item 57: `=`, `X`, `P` CIGAR rejection untested — **not admitted in §10b**

**Expected:** §9.6 lists `H`/`=`/`X`/`P`.
**Found:** `ref_rejects_unsupported_cigar_ops` tests `2H4M` only.
**Gap:** three of four. They share one code arm, so the risk is low — but R13's whole point is that this arm replaces `cigar_to_ops`' silent map-to-`Match`, and a regression that restored the fall-through for `=`/`X` would pass.

### Item 60: the flip-rate fail-loud is untested — **not admitted in §10b**

**Expected:** §9.6 row "flip rate strictly between 0 and 1", asserting on message content.
**Found:** the guard at `mod.rs:850-860` and its message exist; nothing triggers them.
**Gap:** the only defence against writing a half-converted file has no test. Reaching it needs an input with a mixed flip rate — a two-record BAM (one 5-Base, one bisulfite) would do it.

### Item 63 / R17: no `MM:Z:` assertion — **not admitted in §10b**

**Expected:** assert the output contains no `MM:Z:`, because `detect_nanopore` (`bam2pat.py:243-259`) silently switches to `--nanopore` mode — reading `MM`/`ML` instead of `SEQ`, with `-q 0 -F 3844` — on `\tPL:ONT` in the header or `\tMM:Z:` in the first 200 records.
**Found:** nothing. No test, no assertion, no comment.
**Gap:** cheap insurance the plan explicitly asked for against a future tag-preserving input path silently defeating the whole feature.

### Item 67: §9.7.4's OB half is absent — **not admitted in §10b**

**Expected:** the `is_cpg` neighbour survives on **both** strands — `G` at `j+1` for OT, `C` at `j-1` for OB.
**Found:** OT only, and asserted incidentally via the exact-`SEQ` string `ANGT`.
**Gap:** **no `XgStrand::Ga` record anywhere in the suite has a masked position.** The entire masking rule (§3.6/T1, the change rev 2 was written for) is tested on the CT strand only. `ref_base()` is strand-derived, so a GA-side error in the rule would not break a test.

### Items 68, 69: §9.7.5 and §9.7.6 in weaker forms

**68 expected:** on a record that *also* has masked positions, assert `flipped <= letters` and `flipped/letters ∈ {0.0, 1.0}`.
**68 found:** the T2 regression is caught by `flipped == 0` on a `letters == 0` record. `gaps_are_never_written_soft_clip` is the mixed record and asserts neither counter.
**69 expected:** 100 bp record, **two** masked positions, `NM_new == NM_old + 2` ("a whole-read mask fails this loudly").
**69 found:** the `n = 1` form on a 4 bp record.
**Gap:** both are one added assertion each on records that already exist.

### Item 18 / §3.9: `U`/`u` is exercised nowhere — **not admitted in §10b**

**Expected:** §3.9 is a dedicated plan section arguing that excluding `U`/`u` "would leave exactly those cytosines in 5-Base polarity while their neighbours flip — a silent, half-converted file."
**Found:** the code includes both arms. Fixture letter census (measured in this audit): `Z=37, x=38, h=75, .=512` — **no `U`, `u`, `X`, `H` or `z`**. No unit test uses `U`/`u`.
**Gap:** deleting `b'U'`/`b'u'` from the match would turn them into `InvalidXmByte` and break no test. A `U`/`u` unit test is two lines; note that a `U` arises next to a soft clip or insertion, so the shape is already in the fixture's provenance.

### Item 9 / §5.9: two of three chromosome-note warnings missing — **not admitted in §10b**

**Expected:** in addition to the naming/partial-intersection point, warn that (b) wgbstools' CpG dictionary comes from *its own* FASTA, so a patch/build skew is silent, and (c) `patter`'s `--clip` operates on the **cleaned, reference-space** sequence (`patter.cpp:169`), not read cycles.
**Found:** (a) only, in both the runtime note and `illumina-5-base.md`.
**Gap:** (c) is the sharper of the two for this reporter, who is running `bam2pat` **with `--clip`** (§8, "from the reporter").

### Items 6, 31, 83, 90: four "state it so it isn't "fixed"" requirements not in the code

The plan repeatedly asks for a specific fact to be recorded at the code site. Four were not:
- **#6** the output retains `@PG ID:Bismark`, so the converter is idempotent on its own output — "a stated property, not a coincidence".
- **#31 / T8** non-CpG cytosines are masked too, harmlessly — "noted so a later reader does not mistake it for a bug".
- **#83 / R11** why the gate passes `--illumina_5base` on bisulfite input.
- **#90 / R18** the header is copied **deliberately**, unlike `run_five_base_consensus_standalone`, which synthesises — "worth the comment so it isn't 'fixed'".

D2's comment shows the pattern was followed where it was applied; these four sites were missed. Each is one line.

### Item 46 / §9.3: the OB orientation trap is split and mislabelled

**Expected:** one hand-built `-`-strand record, asymmetric `XM` with one letter near each end **in different cases**, exact expected `SEQ`, built so a `read_pos_5p` write differs detectably.
**Found:** two tests, neither matching. `ga_strand_uses_the_g_a_pair` is GA with an exact-`SEQ` assertion that *would* catch a reversed write (letters at 2 and 6 → a 5'-indexed write lands at 5 and 1, giving a different string) but is single-case and symmetric. `asymmetric_calls_edit_the_correct_positions` is mixed-case but `XgStrand::Ct`, where 5'-indexing is indistinguishable — and its doc comment calls it "a reverse-strand-shaped record", which it is not, and carries two stacked doc paragraphs saying the same thing.
**Gap:** the guard exists in substance (across two tests plus the fixture's asymmetric clip pair through the gate) but not in the single form specified, and the misleading comment would send a future reader to the wrong test.

### Documentation inconsistency: `PROGRESS.md` contradicts `PLAN.md` §10b

`PROGRESS.md:51` still lists as a tracked risk: "`MD` re-emission is the only non-trivial algorithm — **tested against an independent from-genome oracle (PLAN §9.5)**, not against itself." §10b correctly records that oracle as *not done*. `PROGRESS.md:52` also still says "byte-identical BAM out", which R10 replaced with identical SAM text, and `:53` cites "PLAN §9.7" for the end-to-end concordance risk, which is §9.9. `PROGRESS.md` also duplicates its "Code review"/"PR → dev" rows.

---

## Test verification

| Test | File | Status |
|---|---|---|
| `md_tokenises` | `src/aligner/five_base_bisulfite.rs` | PASS |
| `ref_reconstructs_all_matches` | same | PASS |
| `ref_reconstructs_mismatches_from_md_letters` | same | PASS |
| `ref_pads_soft_clips_and_insertions_with_x` | same | PASS |
| `ref_takes_deleted_bases_from_md_and_excludes_them_from_ref_seq` | same | PASS |
| `ref_rejects_a_cigar_seq_length_disagreement` | same | PASS |
| `ref_rejects_unsupported_cigar_ops` | same | PASS |
| `xg_fixes_the_pair_and_the_reference_base` | same | PASS |
| `bisulfite_input_is_unchanged` | same | PASS |
| `five_base_input_flips_every_letter` | same | PASS |
| `ga_strand_uses_the_g_a_pair` | same | PASS |
| `asymmetric_calls_edit_the_correct_positions` | same | PASS |
| `gaps_are_never_written_soft_clip` | same | PASS |
| `gaps_are_never_written_insertion` | same | PASS |
| `masks_only_no_call_reference_cytosines` | same | PASS |
| `no_masking_when_every_cytosine_is_called` | same | PASS |
| `rejects_an_xm_seq_length_mismatch` | same | PASS |
| `rejects_an_unknown_xm_byte` | same | PASS |
| `rejects_a_call_in_a_gap` | same | PASS |
| `rejects_a_letter_whose_base_is_outside_the_pair` | same | PASS |
| `rejects_a_record_whose_tags_fail_the_round_trip` | same | PASS |
| `bisulfite_input_round_trips_to_identical_sam_text` | `tests/aligner_five_base_bisulfite.rs` | PASS |
| `bisulfite_input_reports_a_zero_flip_rate` | same | PASS |
| `xm_is_left_untouched` | same | PASS |
| `appends_a_pg_with_a_distinct_id` | same | PASS |
| `requires_illumina_5base` | same | PASS |
| `rejects_both_standalone_bam_modes_together` | same | PASS |
| `rejects_input_without_bismark_tags` | same | PASS (but see items 49-53: it exercises the D3 guard, not the tag-missing path) |

`cargo test -p bismark --lib five_base_bisulfite` → **21 passed, 0 failed**.
`cargo test -p bismark --test aligner_five_base_bisulfite` → **7 passed, 0 failed**.
`cargo fmt -p bismark -- --check` → clean. `cargo clippy -p bismark --all-targets` → clean.

### Tests the plan requires that do not exist

| Required test | Source | Status |
|---|---|---|
| Gate over `dedup/nondir_pe_1030.bam` (all four strand indices) | §9.1 | MISSING |
| Gate over `dedup/synth_barcode_10k…_pe.bam` (12974 rec, 42 `I`/`D`) | §9.1 | MISSING |
| Gate over `filter_nonconversion/{se_default,pe_default}/` | §9.1 | MISSING |
| Unmapped pass-through over `filter_nonconversion/se_unmapped/in.bam` | §9.1, §3.7 | MISSING |
| CIGAR-census assertion in test setup | §9.1 | MISSING |
| Independent from-genome `NM`/`MD` oracle (4 shapes) | §9.5 | MISSING (admitted, §10b) |
| Missing `XM` / `XR` / `XG` / `MD` / `NM` (5 tests, message content) | §9.6 | MISSING |
| `=` / `X` / `P` in CIGAR | §9.6 | MISSING |
| Letter at an `I` position | §9.6 | MISSING |
| Round-trip failure on the **`MD`** branch | §9.6 | MISSING |
| Flip rate strictly between 0 and 1 | §9.6 | MISSING |
| Output contains no `MM:Z:` | §9.6, R17 | MISSING |
| `is_cpg` neighbour survives on **OB** (`C` at `j-1`) | §9.7.4 | MISSING |
| Disjointness assertion on a record with letters **and** masks | §9.7.5 | MISSING |
| `NM_new == NM_old + 2`, 100 bp, two masks | §9.7.6 | MISSING (weaker `n=1` form exists) |
| `XG` ⟺ FLAG over `nondir_pe_1030.bam` | §9.8, R8 | MISSING (admitted, §10b) |
| `U`/`u` letters rewritten | §3.2, §3.9 | MISSING |
| Secondary/supplementary pass-through + one aggregated warning | §3.7 | MISSING |
| Empty (header-only) BAM ⇒ header-only output, exit 0 | §3.10 | MISSING |
| End-to-end `bam2pat` concordance, `--five_base_baseq > 0` arm | §9.9 | MISSING (admitted, §10b; the `= 0` arm has a stronger substitute) |

---

## Verdict

**INCOMPLETE — 22 items unresolved.** The feature's behaviour is complete and correct against the plan; the shortfall is entirely in §9's validation coverage, and **§10b's self-report understates it**.

§10b's three admissions (§9.5's oracle, §9.8's test, §9.9's real data) are accurate and, for §9.5 and §9.9, reasoned. The following were **not** disclosed:

**Blocking-grade (a plan requirement with no coverage at all):**
1. **§9.1's fixture list was cut from five groups to one** (item 40). No PE record, and **only 2 of 4 strand indices** — the plan says in terms that without `nondir_pe_1030.bam` the "whole encoding table" claim is false. All four unused fixtures exist on disk.
2. **§9.1's CIGAR-census assertion** was not written (item 41). A fixture swap silently voids §9.4 and §9.6's shapes.
3. **§9.6's five missing-tag rows are unreachable** (items 49-53): the uBAM test hits D3's guard, so the `ctx()` error path never runs. The test's `||` disjunction hides this.
4. **§9.6's flip-rate fail-loud has no test** (item 60) — the sole guard against writing a half-converted file.
5. **R17 / §9.6's `no MM:Z:` assertion is absent entirely** (item 63).
6. **§9.7.4's OB half is absent** (item 67), and with it *all* GA-strand masking coverage — the rule rev 2 was written to replace is tested on the CT strand only.
7. **`U`/`u` (§3.2, §3.9) is exercised by nothing** (item 18): no unit test, and the fixture's census contains no `U`/`u`/`X`/`H`/`z`. Deleting those two match arms would break no test.
8. **§5.9's CpG-dictionary and `patter --clip` warnings were not written** (item 9) — the `--clip` one matters for this reporter, who runs `bam2pat --clip`.

**Weakened rather than missing:**
9. §9.6's `=`/`X`/`P` (item 57) and `I`-position letter (58) and `MD`-branch round-trip (59) rows.
10. §9.7.5 and §9.7.6 present in weaker forms (items 68, 69).
11. §9.3's OB orientation trap split across two tests, one of them mislabelled as reverse-strand when it is `XgStrand::Ct` (item 46).
12. Four "state it in the code so it isn't 'fixed'" requirements (items 6, 31, 83, 90).
13. §3.7's secondary/supplementary path and §3.10's empty-BAM path are implemented and untested (items 34, 38).

**Also:** `PROGRESS.md:51` still asserts the §9.5 from-genome oracle **was** used, contradicting §10b; `:52` still says "byte-identical BAM" (R10 replaced that); `:53` cites §9.7 for what is §9.9; and its pipeline table has duplicated rows.

Per the workflow: these are gaps, not fixes. No code was changed by this audit.
