# PLAN — `--five_base_bisulfite_bam`: re-encode 5-Base `XM` calls into bisulfite-convention `SEQ`

**Issue:** [#1095](https://github.com/FelixKrueger/Bismark/issues/1095) · **Origin:** [#787](https://github.com/FelixKrueger/Bismark/issues/787) (@Danielsm8) · **Branch:** to be cut from `dev` (`bfdf207`)

**Rev 2** — fixes a units bug in rev 1's §3.6. Awaiting manual review. Not implemented.

---

## 0. Revision history

### Rev 1 → rev 2

§3.6 was the only part of rev 1 that was new design rather than a correction, and it had a bug.

**§3.6's mechanism was replaced outright.** Rev 1 identified the masked set from `QUAL` against a threshold auto-detected out of the input's `@PG CL:`. That had a data-destroying units bug, and needed a CLI flag, a header parser, a conflict rule and a fail-loud fallback to work at all. It is replaced by a rule keyed on the reference base the converter **already reconstructs** — provably exactly the leak set, provably empty without masking, and needing none of that machinery.

| # | Change | Source |
|---|---|---|
| **T1** | **🔑 Mechanism replaced.** Mask iff `XM[i] == '.'` **and** `ref_seq[i] == ref_base` **and** `SEQ[i] ∈ {meth, unmeth}`. Deletes the new CLI flag, the `@PG` parser, rev 1's §3.6.4/§3.6.5, two `@PG`-related rows of §9.6, and rev 1's "remaining risk 3". Makes **§9.1 the regression test** for this path (property 2), which rev 1's mechanism could not be. §3.6 | `PLAN_REVIEW_36` F1 |
| T2 | **🔑 Flip counter moved into the letter arms.** At loop level it also counts masks, so `flipped/letters > 1` and §3.5's fail-loud aborts **every correctly-masked 5-Base run** — a false failure on the exact input the feature targets. `flipped` and `masked` are disjoint. §3.2, §9.7.5 | `PLAN_REVIEW_36` F4 |
| T3 | **`QUAL` units bug** — rev 1 masked where `QUAL[i] < offset + n`, but a BAM stores 0-based phred scores (`output.rs:437-440`), so that masked *every* `.` position. Found in self-review, confirmed independently as the review's C1. Moot under T1, which reads no `QUAL` at all | self-review + `PLAN_REVIEW_36` C1 |
| T4 | **§9.7 rewritten with negative controls.** Rev 1 asserted only that a maskable position gets masked — which an implementation that masks *everything* also satisfies. Now: exact counts, non-cytosine `.` untouched, gaps untouched, `is_cpg` neighbour preserved, disjointness, `NM` bookkeeping, and `masked == 0` on every bisulfite fixture. §9.7 | `PLAN_REVIEW_36` C2 |
| T5 | **PE call site cited.** Rev 1 pointed at the **SE** helper, whose only caller passes `baseq = 0`; the leak users actually hit is `mod.rs:1695-1697` in `five_base_emit_pe_record`. §3.6.6 | `PLAN_REVIEW_36` F9 |
| T6 | **Appended `@PG` gets a distinct ID** (`ID:bismark-five-base-bisulfite`, `PP:Bismark`). A second `ID:Bismark` violates SAM's unique-ID rule. §5.6 | `PLAN_REVIEW_36` F10 |
| T7 | **"B's proposal was rejected" reframed to "composed with".** B's conjunct is necessary and is clause 3 of T1's rule; what it lacked was a positional test. Rev 1's "roughly half of every read" corrected to ≈30% (5-Base) / ≈40% (bisulfite). §3.6 | `PLAN_REVIEW_36` F5 |
| T8 | **`N` is `patter`'s own idiom** — `clean_CIGAR` pads `D`/`N` ops with `'N'` (`patter_utils.cpp:237-239`), so the filler choice is a verified property of the consumer rather than an inference. Also: non-CpG cytosines are masked too, harmlessly. §3.6 | `PLAN_REVIEW_36` O2/O4 |
| T9 | **Consensus BAM restated.** No leak exists there (`mod.rs:2306` passes `0`; its other `N`s are already `N` in `SEQ`), and under T1 `masked == 0` follows from property 2 rather than from a coincidence in the synthesised header. §3.6.6 | self-review + `PLAN_REVIEW_36` F8 |

### Rev 0 → rev 1

The **algorithm survived both reviews intact**; every change below is in validation, I/O plumbing, or one genuine design hole.

| # | Change | Source |
|---|---|---|
| R1 | **New: `--five_base_baseq` masking leak closed.** Masked positions kept raw 5-Base bases in `SEQ`, which `patter` scores **inverted** — the exact bug this feature exists to remove. §3.6 | B-C1 |
| R2 | **Fixtures corrected.** Rev 0 named `test_files/*_from_TrimGalore.bam`; those are unaligned uBAMs (0 `@SQ`, no `XM`). §9.1 | A-C1, B-C2 |
| R3 | **`NM` prose corrected.** Bismark's `NM` *includes* insertions **and** soft-clipped bases. Rev 0 asserted the opposite in three places. Formula unchanged. §3.3 | A-C2, B-I1 |
| R4 | **`MD`/`NM` now reuse `output::make_mismatch_string` + `hemming_dist`**, with a per-record round-trip proof. Removes rev 0's self-declared largest risk. §3.4 | A-I1, B-A1 |
| R5 | **Reader changed to raw noodles + `write_raw_record`.** `BamReader::records()` drops `FLAG & 0x4`, so rev 0's unmapped pass-through was unimplementable. §2, §3.7 | A-C3, B-I3 |
| R6 | **§9.2 rewritten — it was vacuous.** It asserted via `extract_calls`, which reads `XM`, the one thing the converter never touches; it passed even with the meth/unmeth pair swapped. **The sign was untested in CI.** | A-I7 |
| R7 | **New: flip-rate invariant** — `flipped / (#XM letters)` is exactly `1.000` (5-Base) or `0.000` (bisulfite). A free, hermetic discriminator on the *file*. §3.5, §9.2 | B-I5/O6 |
| R8 | **New: `XG` ⟺ FLAG equivalence documented and tested.** `patter` infers strand from the FLAG, this plan encodes from `XG`. They agree for every Bismark record — load-bearing, non-obvious, invisible to §9.1. §3.8 | B-I4 |
| R9 | **`seq_old[i] ∈ {meth, unmeth}` assertion added** at every letter position. §3.3 | A-I2, B-I5 |
| R10 | "byte-identical BAM" → **"identical SAM text"**. Unachievable literally (BGZF boundaries, `NM` int-width re-encode). §9.1 | A-I3 |
| R11 | **§5.3-vs-§9.1 conflict resolved** — the gate passes `--illumina_5base` on bisulfite input, with a comment. §5, §9.1 | A-I4 |
| R12 | **§3.1.2 derivation written out.** `methylation_call` branches on **`XR`**, not `XG`; the `XG` relation is emergent and reads as an error otherwise. | A-§3.1.2, B-§1.3 |
| R13 | **CIGAR `N` handled; `=`/`X`/`P` rejected explicitly** rather than falling through as `cigar_to_ops:762` does. §3.7 | A-I5 |
| R14 | **Dispatch order fixed** — rev 0 put the mutual-exclusion check "beside `:167`", which already `return`s, so it would never fire. §5.2 | B-O1 |
| R15 | **Chromosome note rewritten.** `locus2CpGIndex` cannot throw on this path; the real guard is `set_regions` failing *early*, and the silent hazard is a **partial** name intersection. §5.7 | B-I6 |
| R16 | Record-mutation route documented (`BismarkRecord` has no `inner_mut()`). §5.5 | A-C4 |
| R17 | Assert output carries no `MM:Z:` — `detect_nanopore` would silently switch modes. §9.6 | B-O2 |
| R18 | Notes added: consensus BAM is valid input; header is copied (unlike the precedent, which synthesises); `MD`/`NM` churn is total; `--ds_test` still works. §7 | A-I8, B-O3/O4 |
| R19 | "Drop `MD`/`NM`" alternative recorded and priced. §10 | A-O1 |

**One reviewer recommendation was rejected — see §3.6.** B's option 2 ("at an `XM == '.'` position whose `SEQ[i]` ∈ {meth, unmeth}, write `N`") is broken as stated: it would `N`-mask every genomic non-cytosine whose read base happens to be `C` or `T` — roughly half of every read. §3.6 uses `QUAL` to identify the actually-masked set instead.

---

## 1. Goal

Add an opt-in aligner flag `--five_base_bisulfite_bam <BAM>` (repeatable) that reads existing Bismark **5-Base** BAM(s) and writes, per input, a *second* BAM whose `SEQ` has been re-encoded into **bisulfite convention** from the `XM` tag Bismark already produced — with `NM` and `MD` updated exactly.

The outcome is a drop-in input for `wgbs_tools bam2pat` → `UXM_deconv`, which today produces **systematically inverted** calls on 5-Base data. No re-alignment; the byte-frozen aligner is untouched.

**Non-goal:** this file is never the primary BAM. Its `SEQ` no longer matches the sequencer.

---

## 2. Context

### Why it is needed

Bismark's 5-Base methylation is *already* conventional — the inversion happens when `XM` is written (`aligner/methylation.rs:560-614`), so extractor/coverage/bedGraph/cytosine reports are all correct. But `patter` (the C++ engine behind `bam2pat`) **never reads `XM` for its calls**: it takes the read character in `SEQ` at CpG positions and compares it against two per-strand constants (`patter.h:59-60`):

```cpp
ReadOrient OT{'C', 'T', 0, 0};   // {ref_chr = methylated, unmeth_seq_chr = unmethylated, shift, mbias_ind}
ReadOrient OB{'G', 'A', 1, 1};
```

5-Base `SEQ` keeps the original read (`aligner/mod.rs:1352-1353`), and 5-Base chemistry is bisulfite's inverse (5mC→T; unmethylated C stays C). Hence inverted output.

(`patter`'s *call* path never reads `XM`, but `is_pass_ds_test` does parse `XM:Z:` for the opt-in `--ds_test` filter — `patter_utils.cpp:350-398`. Does not affect the diagnosis; see §7.)

### Placement

| File | Change |
|---|---|
| `rust/bismark/src/aligner/cli.rs` | Two new fields after `five_base_consensus_from_bam` (~`:183`) |
| `rust/bismark/src/aligner/mod.rs` | Mutual-exclusion check **before** the `:165-168` dispatch (R14); new `run_five_base_bisulfite_standalone()` |
| **`rust/bismark/src/aligner/five_base_bisulfite.rs`** *(new)* | Pure per-record re-encode + reference reconstruction. No I/O |
| `rust/bismark/tests/aligner_five_base_bisulfite.rs` *(new)* | Integration + the §9 gates |
| `rust/bismark/tests/data/five_base_bisulfite/` *(new)* | **A soft-clip fixture must be generated** — no existing fixture has any `S` (§9.1) |
| `docs/src/content/docs/rust/illumina-5-base.md` | New subsection under "Advanced modes" |
| `CHANGELOG.md`, `rust/README.md` | Entry + Milestones line |

### Existing code to reuse

- **`output::make_mismatch_string`** and **`output::hemming_dist`** — both `pub(crate)` in `crate::aligner::output`; the new module is in the same module tree so both are directly callable (R4). This is the single largest risk reduction in the plan.
- **Write:** `crate::io::BamWriter`, including **`write_raw_record`** (`io/write.rs:86`), which exists precisely for records that are deliberately not Bismark-shaped (R5).
- **Tags:** `io::tags::{xm, xr, xg, md, nm}` (`io/tags.rs:21-63`).
- **Dispatch:** the `--five_base_consensus_from_bam` short-circuit shape (`mod.rs:167` → `:515`).

### Deliberately NOT reused

**`crate::io::BamReader`.** Its `records()` is `filter_map(filter_unmapped_then_classify)` and **silently drops `FLAG & 0x4`** (`io/read.rs:268-274`, `:631-645`, documented at `:7-9`), so the unmapped pass-through in §3.7 is impossible through it. `from_noodles_record` additionally requires `XR` and enforces the `XM`/`SEQ` parity check at the *reader*, which would turn §3.7's specific, QNAME-bearing errors into a generic `BismarkIoError`. **Read with `noodles_bam::io::Reader` directly** and re-add the parity check explicitly (§3.7) — it is no longer free.

**`BismarkRecord::iter_aligned()`.** It returns **5'-oriented** positions (`io/record.rs:245-249`) and additionally *skips* `I`/`S` (`:295-296`), so it cannot even address every `SEQ` byte. Writing into `SEQ` at `read_pos_5p` silently reverses every OB read's edits — see the standing warning at `extractor/call.rs:182-183`. §3.2 needs no reference positions, so there is no reason to reach for it. **Add a comment in the code saying so**, or a later refactor will "simplify" into the bug.

---

## 3. Behavior

### 3.1 The three properties this rests on

All three were independently re-verified by both reviewers across all four SE strand indices **and** all eight PE mate/index combinations.

**3.1.1 `len(XM) == len(SEQ)`, and `XM[i]` ↔ `SEQ[i]` in BAM space.** Parity enforced at `io/record.rs:122-130`, relied on at `:301`. `XM` is reversed in lockstep with the `SEQ`/`ref_seq` revcomp: SE `aligner/output.rs:443-450` + `:463-467`; **PE `output.rs:671-675` + `:694-698`** — and PE is the path that matters, since 5-Base is paired-end only (`cli.rs:99-100`).

**3.1.2 `XG` alone fixes the reference base and the `(meth, unmeth)` pair.**

⚠️ **This is emergent, not direct.** `methylation_call` branches on **`XR`**, not `XG` (`methylation.rs:575`, `:583`). Anyone checking a single line will conclude this section is wrong. The relation composes three facts — the index→(strand, XR, XG) map (`methylation.rs:131-141`), which genomic base each branch tests (`:584-620`), and the joint `SEQ`/`ref_seq` revcomp for `-` strand (`output.rs:443-450`):

| index | (strand, XR, XG) | strand | branch tests | output revcomp | ⇒ BAM ref base | `(meth, unmeth)` |
|---|---|---|---|---|---|---|
| 0 | `+`, CT, CT | OT | `genomic[i] == 'C'` | no | `C` | `(C, T)` |
| 1 | `-`, CT, GA | OB | `genomic[i] == 'C'` | yes | `G` | `(G, A)` |
| 2 | `-`, GA, CT | CTOT | `genomic[i+2] == 'G'` | yes | `C` | `(C, T)` |
| 3 | `+`, GA, GA | CTOB | `genomic[i+2] == 'G'` | no | `G` | `(G, A)` |

PE agrees at all eight mate/index combinations (`methylation.rs:421-433`, `output.rs:546-556`, `:671-675`). So:

| `XG:Z:` | reference base at every XM-letter position | methylated | unmethylated |
|---|---|---|---|
| `CT` | `C` | `C` | `T` |
| `GA` | `G` | `G` | `A` |

These are exactly `patter`'s `ref_chr` / `unmeth_seq_chr`.

**3.1.3 Soft clips and insertions are structurally `.`.** `I`/`S` pad the genomic window with `b'X'` without advancing `pos` (`methylation.rs:174-181` SE, `:349-354` PE `walk_mate`). `X` matches no read base (reads are upper-cased `ACGTN`) and is not `C`/`G`, so both branches fall to `.` (`:597-599`, `:615-617`). `reverse_complement` leaves `X` unchanged (`:60-71`), so the padding survives both revcomps. The window is therefore **never mis-framed**, and a positional zip **provably cannot touch a clipped or inserted base**.

### 3.2 Per-record re-encode

```
meth, unmeth, ref_base  =  XG == "CT" ? ('C','T','C') : ('G','A','G')

seq_new = seq.clone()
flipped = 0 ; letters = 0 ; masked = 0
for i in 0 .. seq.len():                      # len(xm) == len(seq), asserted
    match xm[i]:
        b'Z' | b'X' | b'H' | b'U'  =>  { letters += 1; seq_new[i] = meth
                                         if meth   != seq[i] { flipped += 1 } }
        b'z' | b'x' | b'h' | b'u'  =>  { letters += 1; seq_new[i] = unmeth
                                         if unmeth != seq[i] { flipped += 1 } }
        b'.'                       =>  { if ref_seq[i] == ref_base            # §3.6
                                            && seq[i] in {meth, unmeth}
                                         { seq_new[i] = b'N'; masked += 1 } }
        other                      =>  return Err(InvalidXmByte)
```

⚠️ **The flip comparison must live inside the two letter arms, not at loop level (T2).** A loop-level `if seq_new[i] != seq[i] { flipped += 1 }` also counts every §3.6 mask, so `flipped/letters` exceeds 1 and §3.5's fail-loud fires on **every correctly-masked 5-Base file** — a false abort that reads like a data problem, on exactly the input this feature was written for. `flipped` is a letter-position statistic and `masked` a gap-position statistic; they are **disjoint by construction**, and `masked` is never part of the flip-rate denominator.

All eight letters are rewritten, **including `U`/`u`** — see §3.9.

### 3.3 Invariants asserted per record

1. **`len(XM) == len(SEQ)`** (R5: no longer free — assert explicitly).
2. **`seq_old[i] ∈ {meth, unmeth}` at every letter position** (R9). A real property of both branches: CT emits a letter only when the read base is `C` or `T` (`methylation.rs:585-599`), GA only `G` or `A` (`:605-618`), and it survives the joint revcomp. Needs no `MD`, no genome, O(1) per position. Catches a mis-zipped `XM`, an inverted `XG`→pair table, and the `read_pos_5p` bug — and it holds even under base-quality masking, since masked positions carry `.`.
3. **No `XM` letter at an `I`/`S` position** (§3.1.3 turned into a runtime check).
4. **The round-trip proof** — see §3.4 step 2.

### 3.4 `NM` and `MD` — reuse Bismark's own generator (R4)

Rev 0 proposed writing a fresh `MD` emitter and named it the plan's largest risk. Both reviewers independently pointed out the byte-exact implementation is already `pub(crate)` in a sibling module, and that matching `rebuild_md_with_deletions` (`output.rs:228-390`, a verbatim Perl port whose own comments read *"Perl dies — unreachable"*) is real work for no gain.

```
1. RECONSTRUCT ref_seq from (seq_old, CIGAR, MD_old), one byte per read position:
      M  -> genomic base   (MD digit ⇒ read base; MD letter ⇒ that letter)
      I/S-> b'X'           (MD is blind to these by design)
      D  -> excluded from ref_seq; bases come from MD's ^XYZ
   Also build md_sequence (only needed when the CIGAR has D): ref bases at M+D,
   b'X' at I/S, in GENOME-FORWARD order — the extraction revcomp
   (methylation.rs:220-223) and the output revcomp (output.rs:446-448) cancel.

2. PROVE the reconstruction, per record, before emitting anything:
      hemming_dist(seq_old, ref_seq) + Σ(D lengths)          == NM_old
      make_mismatch_string(seq_old, ref_seq, cigar, md_seq)  == "MD:Z:" + MD_old
   Any mismatch ⇒ hard error naming the QNAME. This is strictly stronger than
   rev 0's §3.3.2 cross-check (which only pinned the ref base at letter
   positions) and it covers the deletion path, which §9.1 cannot reach.

3. EMIT:
      NM_new = hemming_dist(seq_new, ref_seq) + Σ(D lengths)
      MD_new = make_mismatch_string(seq_new, ref_seq, cigar, md_seq)   # strip "MD:Z:"
```

Reconstruction from `(SEQ_old, CIGAR, MD_old)` is **always sufficient** for every reference base anyone needs: all `M` positions and all `D` positions. It is *not* sufficient for `I`/`S`, and nothing needs it to be.

⚠️ **Bismark's `NM` — the correct identity (R3).** Rev 0 claimed `NM` "excludes insertions". **False.** `hemming_dist` (`output.rs:150-157`) is `actual.len() - matches` over a positional zip against `ref_seq`, and `ref_seq` carries `b'X'` at every `I` **and** `S` position. `X` never equals a read base, so both are counted — the doc comment at `output.rs:141-144` says *"`X` padding bases mismatch — intentionally counted"*. Then `ext.indels` (deletions only) is added:

```
NM = (mismatches at M) + (#inserted bases) + (#soft-clipped bases) + (#deleted bases)
```

Soft-clip inflation is the genuinely non-standard part, and it is a **large** term under `--five_base_umi_len 8`. Step 2 above makes this moot by construction — you are calling the function that defines the quirk rather than restating it in prose.

### 3.5 The flip-rate invariant (R7)

With `five_base = true`, `push_ct_context(..., !five_base)` on a matched `C` gives **lower** case and `push_ct_context(..., five_base)` on a read `T` gives **UPPER** case. Working that through all four indices including the revcomp:

| Input | Every letter position |
|---|---|
| **5-Base** | flips (`Z` ⇒ `SEQ` was `T` ⇒ becomes `C`; `z` ⇒ was `C` ⇒ becomes `T`) |
| **bisulfite** | never flips |

So **`flipped / letters` is exactly `1.000` for 5-Base and `0.000` for bisulfite, per record.** Anything in between is a bug or a mixed file.

This is a hard runtime discriminator on the **file**, where `--illumina_5base` is only a statement about the user's **intent**. Report the rate (not a per-record mean) and fail loud on a value that is neither 0 nor 1.

### 3.6 🔑 The `--five_base_baseq` masking leak (R1)

**The hole.** `mask_low_quality` (`mod.rs:1295-1307`) masks low-quality bases to `N` **in the call sequence only**; the record is built from the *unmasked* read (`mod.rs:1352-1360`, stated as a feature at `cli.rs:134-135`). So a masked cytosine gets `XM == '.'`, §3.2 leaves `SEQ` untouched, and it keeps its raw 5-Base base. Then in `patter`: `is_cpg` passes (it accepts `seq[j] ∈ {C,T}` and only needs `seq[j+1] == 'G'`), and `T == unmeth_seq_chr` ⇒ **UNMETH** — when under 5-Base chemistry `T` means **methylated**.

That is the precise bug this feature exists to remove, surviving in the output, silently. B enumerated every other route to `XM == '.'` and confirmed masking is the **only** leak: soft clips and insertions are deleted by `clean_CIGAR` (`patter_utils.cpp:240-241`), genuine mismatches fail `is_cpg`, deletions have no `SEQ` position, and edge-guard records are never written.

**The fix — key on the reference base the converter already reconstructs (T1).**

§3.4 step 1 already rebuilds `ref_seq` for every record, one byte per read position, and step 2 proves it byte-exact before anything is emitted. §3.1.2 fixes the reference base at any scoreable cytosine. So the converter already knows — for free, per position — whether a position *is* a cytosine on the read's strand:

> **Mask `b'N'` iff `XM[i] == '.'` **and** `ref_seq[i] == ref_base` **and** `SEQ[i] ∈ {meth, unmeth}`.**

Three properties, each verified against source:

1. **It is exactly the leak set.** `patter` scores a position only when the reference locus is a dictionary CpG (`patter.cpp:141`) *and* `is_cpg` passes, which requires `seq[j] ∈ {C,T}` (OT) / `{G,A}` (OB) (`patter.cpp:96-103`). A leak therefore requires `SEQ[i] ∈ {meth, unmeth}` at a reference cytosine carrying no Bismark call. Nothing outside the set can leak, and nothing inside it is ever scored correctly.
2. **It is provably empty when no masking was applied.** At a genomic `C` the CT branch emits a letter when the call base is `C` (`methylation.rs:587-591`) or `T` (`:594-596`) — unconditionally, no further guard. So `XM == '.'` at a genomic `C` proves `call_seq[i] ∉ {C,T}`; and with no masking `call_seq == SEQ`, so `SEQ[i] ∉ {C,T}`. Contrapositive: the three conjuncts together **prove** masking fired. GA branch symmetric (`:607-614`). **There is no threshold to know and nothing to detect.**
3. **It costs nothing.** No genome, no `QUAL`, no new flag, no header parsing. `MD` is already mandatory (§3.7) and already validated per record. At `I`/`S` positions `ref_seq[i] == b'X' != ref_base`, so clipped and inserted bases are structurally excluded — the converter never writes into a gap.

`N` is the right filler, and it is `patter`'s own idiom: `clean_CIGAR` pads `D`/`N` CIGAR ops with `'N'` (`patter_utils.cpp:237-239`), so `N` is exactly how `patter` represents "no information", and `is_cpg` fails there.

**What this replaces.** Rev 1 identified the masked set via `QUAL` against a threshold auto-detected from the input's `@PG CL:`. That required a new CLI flag, a header parser, a conflict rule, and a fail-loud fallback — and it reached *outside* the record for information the record already contains. It also over-captured in two ways that the rule above avoids for free: it masked the `is_cpg` **neighbour** base (`seq[j+1]`/`seq[j-1]`), destroying good high-quality calls, and it masked the inline UMI inside the soft-clipped prefix under `--five_base_umi_len` — invisibly, since `ref_seq` carries `X` there so neither `NM` nor `MD` changes.

**On B's original proposal.** B suggested masking any `XM == '.'` position whose `SEQ[i]` ∈ {meth, unmeth}. Rev 1 recorded that as *rejected*; that overstated the disagreement. B's conjunct is the **third** clause above and is necessary — what it lacked was a positional test proving the position is a reference cytosine. The two proposals compose rather than compete. (Rev 1 also put the cost of B's rule alone at "roughly half of every read"; the real figure is ≈30% of a 5-Base read and ≈40% of a bisulfite one. The conclusion stands, but the number was wrong.)

**Ordering and interactions.** `N` at a `.` position is a new mismatch, so `NM`/`MD` must be computed **after** masking — §3.4 already does, operating on the final `seq_new`. Both are handled normally: `hemming_dist` counts it (`output.rs:150-157`) and `make_mismatch_string` emits the reference base at a mismatch (`output.rs:196-200`).

Masking does not disturb the other invariants. `N` is written only at `.` positions, so §3.3.2 and §3.5's flip rate — both letter-position statistics — are untouched; `masked` and `flipped` are **disjoint by construction** (§3.2). And by property 2, **§9.1 is now the regression test for this path**: `masked` must be exactly 0 on any non-masked input, which every bisulfite fixture is. Rev 1's mechanism could not be covered by §9.1 at all.

**Reporting.** The fix is silent by design, so surface it: report `masked` in `<stem>.bisulfite_report.txt` and print a one-line `Note:` when non-zero — *"N no-call cytosines masked to N; this input appears to have been produced with `--five_base_baseq`."* That is strictly more informative than rev 1's design, which announced a threshold it had guessed rather than the positions it actually found.

Non-CpG cytosines are masked too, which `patter` never reads. Harmless and it keeps the rule uniform; noted so a later reader does not mistake it for a bug.

### 3.6.6 Where the leak actually lives, and the consensus BAM

⚠️ **Rev 1 cited the wrong call site (T5).** It pointed at `mod.rs:1352-1360`, inside `five_base_emit_record` — the **SE** helper, whose only production caller is the consensus path, which passes `baseq = 0`. 5-Base is paired-end only (`cli.rs:99-100`), so the call site that real users hit is `five_base_emit_pe_record`:

```rust
// mod.rs:1695-1697
let off = if phred64 { 64 } else { 33 };
let call1 = mask_low_quality(seq1_uc, qual1, baseq, off);
let call2 = mask_low_quality(seq2_uc, qual2, baseq, off);
// ... paired_end_sam_output(identifier, seq1_uc, seq2_uc, ...)  ← unmasked
```

Same leak, both mates. Cite this and `output.rs:645-708` (`build_pe_mate`) — an implementer sent to verify "is `SEQ` really unmasked?" at the SE line finds the consensus caller passing `0` and could reasonably conclude the leak is unreachable.

**The consensus BAM carries no leak.** `run_five_base_consensus` passes literal `0` (`mod.rs:2306`), and the consensus `SEQ` is built from the collapsed consensus rather than from masked reads. Its *other* `N` bases — uncovered, tie, or C>T-variant positions (`five_base_duplex.rs:332`, `:338`, `:358`, `:361`) — are already `N` in `SEQ`, so `is_cpg` fails and `patter` returns `UNKNOWN`; `revcomp` preserves `N` (`output.rs:170`), so the reverse record is fine too. §7's claim that it is a valid input holds, and under the rule above `masked == 0` follows from property 2 rather than from luck.

*(Rev 1 relied on the consensus header resolving to threshold 0. It does — `run_five_base_consensus_standalone` synthesises a fresh header at `mod.rs:533` describing the consensus command — but only by coincidence, and it could resolve to the **wrong** value: `--five_base_baseq` is accepted on the consensus path, since `mod.rs:167-169` short-circuits before `resolve()`, so it lands verbatim in the synthesised `CL:` while being entirely inert. The rule above makes the whole question moot.)*

### 3.7 Reader, writer, and record classes (R5, R13)

Read with `noodles_bam::io::Reader`; copy the input header and append a `@PG` line. Per record:

| Class | Handling |
|---|---|
| Unmapped (`0x4`) | `write_raw_record` **unchanged**, counted |
| Secondary / supplementary (`0x100`/`0x800`) | `write_raw_record` unchanged + **one** aggregated warning. Bismark does not emit them (`five_base_next_primary` filters them upstream) |
| Mapped primary | Re-encode. Requires `XM`, `XR`, `XG`, `MD`, `NM` — any missing ⇒ **fail loud** naming file + QNAME |

CIGAR ops: `M`/`I`/`D`/`S` per §3.1.3, plus **`N`** (consumes reference, no read bases, no `MD` run — `methylation.rs:189-191`, `output.rs:254`). **Reject `=`, `X`, `P` and `H` explicitly** — do *not* copy `cigar_to_ops`' silent map-to-`Match` fall-through (`output.rs:762`) into a converter whose whole premise is positional exactness.

### 3.8 `XG` ⟺ FLAG equivalence (R8)

`patter` infers strand from the **SAM FLAG** (`is_bottom`, `patter_utils.cpp:163-168`), never from `XG`. This plan encodes from `XG`. They agree for every Bismark record:

- **SE** (`output.rs:413-419`): the FLAG is a pure function of `genome_conversion` ⇒ `FLAG & 0x10 ⟺ XG == "GA"`.
- **PE** (`output.rs:526-537`): index → `0→(99,147)`, `1→(163,83)`, `2→(147,99)`, `3→(83,163)`; `is_bottom` is false for 99/147, true for 83/163; indices 0/2 carry `XG:CT`, 1/3 carry `XG:GA`. All four agree — **and they agree *because of* the index-1/2 R1↔R2 first/second-in-pair swap** (the `#1030` quirk, `output.rs:526-527`). Without it a CTOT read would be read as bottom, biasing methylation heavily.

Load-bearing, non-obvious, and **invisible to §9.1** (the output is identical whether or not this holds). Hence the test in §9.8.

### 3.9 Why `U`/`u` must be included

`push_ct_context` maps an `X` in the context slots to `U`/`u` (`methylation.rs:641-649`; `push_ga_context` `:673-675`), so a cytosine immediately before a soft clip or insertion is classified unknown-context. Excluding `U`/`u` would leave exactly those cytosines in 5-Base polarity while their neighbours flip — a silent, half-converted file. `patter` ignores non-CpG anyway, so including them is free.

### 3.10 Other edge cases

| Case | Handling |
|---|---|
| `len(XM) != len(SEQ)` | Fail loud (explicit check — R5 removed the free one) |
| `XM` byte outside `ZzXxHhUu.` | Fail loud, naming QNAME + offset |
| Deletion (`D`) | No `SEQ` position ⇒ nothing to rewrite; `^XYZ` feeds the reconstruction |
| Empty BAM (header only) | Header-only output + a `Note:`; exit 0 |
| Bisulfite (non-5-Base) input | **Identical SAM text** out — §9.1. `flipped/letters == 0.000` |

---

## 4. Signature

```rust
pub struct Reencoded {
    pub seq: Vec<u8>,
    pub nm: i64,
    pub md: String,
    pub letters: u32,        // XM-letter positions seen
    pub flipped: u32,        // bases actually changed  (flipped/letters ∈ {0.0, 1.0})
    pub masked: u32,         // §3.6 N-masked positions
}

/// Re-encode `seq` so each called cytosine carries the base bisulfite chemistry would
/// have produced: `C`/`G` when `XM` says methylated, `T`/`A` when unmethylated. Which
/// pair applies is fixed by `xg` (PLAN §3.1.2).
///
/// `XM` is NOT modified — it is already correct and stays the source of truth.
///
/// No-call positions that sit at a reference cytosine and still carry a scoreable base
/// are masked to `N`, closing the inversion leak in PLAN §3.6. This needs no threshold,
/// no `QUAL`, and no knowledge of the quality encoding — the reconstructed reference
/// identifies the set exactly (§3.6, property 2). There is deliberately **no** `qual`,
/// `baseq` or `phred64` parameter.
///
/// # Errors
/// `InvalidXmByte`, `LengthMismatch`, `SeqNotInPair` (§3.3.2), `CallInGap` (§3.3.3),
/// `RoundTripFailed` (§3.4 step 2), `MalformedMd`, `UnsupportedCigarOp`.
pub fn reencode(
    seq: &[u8], xm: &[u8], xg: XgStrand,
    cigar: &Cigar, md_old: &str, nm_old: i64,
    qname: &str,          // error messages only
) -> Result<Reencoded, FiveBaseBisulfiteError>;
```

---

## 5. Implementation outline

1. **`cli.rs`** — add `five_base_bisulfite_bam: Vec<PathBuf>`, doc-commented in the `[#787]` style of its neighbours. Keep `///` continuation lines free of leading `+ `/`- `/`* ` (clippy `doc_lazy_continuation`). **One new flag only** — §3.6's rule needs no threshold, so rev 1's `--five_base_bisulfite_baseq` is gone (§10 Open-6).
2. **Dispatch (R14)** — put the **mutual-exclusion check before** the existing `mod.rs:165-168` block, which already `return`s; then the new guard. Ordering matters or the check never fires.
3. **Validation** — require `--illumina_5base`; explicitly **do not** require `--genome`.
4. **New module** — `XgStrand`, `FiveBaseBisulfiteError`, `reencode()`, `reconstruct_ref()`. Write `reconstruct_ref` first and test it against §3.4 step 2 in isolation: everything else depends on it, including §3.6's masking rule. **No `@PG` parser** — rev 1 needed one, §3.6's rule does not.
5. **Record mutation (R16)** — `BismarkRecord` has no `inner_mut()`; and this path uses raw noodles anyway. Clone the record into a `RecordBuf`, then `sequence_mut()` and `data_mut().insert(NM/MD)`. `Data::insert` **replaces in place and preserves field order** (noodles-sam `record_buf/data.rs:222-232`), so the tag block cannot be reordered — which is what makes §9.1's SAM-text comparison safe.
6. **Driver** — per input BAM: open, copy header, append a `@PG`, stream records, write `<output_dir>/<input-stem>.bisulfite.bam`.

   ⚠️ **The appended `@PG` needs a distinct ID (T6)** — `ID:bismark-five-base-bisulfite`, `PP:Bismark`. Appending a second `ID:Bismark` violates SAM's unique-`@PG`-ID rule. Note also that the output *retains* the input's `@PG ID:Bismark`, so the converter is safely idempotent on its own output (already-`N` stays `N`; letters are no longer `.`) — a stated property, not a coincidence.
7. **Counters + report** — records read / re-encoded / passed through, `letters`, `flipped`, **flip rate**, `masked`. Fail loud if the flip rate is neither 0 nor 1; print the §3.6 `Note:` when `masked > 0`. Write `<stem>.bisulfite_report.txt`.
8. **Closing `Note:`** — following the `ubam.rs:214/239` precedent of instructing rather than shelling out:
   ```
   samtools sort -o <out>.sorted.bam <out> && samtools index <out>.sorted.bam
   wgbstools bam2pat --genome hg38 <out>.sorted.bam
   ```
   ⚠️ **Mandatory, not advisory:** `is_bam_sorted` (`bam2pat.py:222-240`) **skips the BAM entirely** on a non-`coordinate` `@HD`, and `generate_sam_header` writes `SO:unsorted` (`output.rs:109-111`), which the copied header carries.
9. **Chromosome-naming note (R15)** — the accurate version. A name mismatch does **not** surface as `locus2CpGIndex` throwing: that is unreachable here, because `conv`/`dict` are built from the same `tabix` region (`patter.cpp:28-41`). `set_regions` (`bam2pat.py:49-80`) intersects `samtools idxstats` names against the wgbstools genome's and raises **immediately** on an empty intersection, before any read is parsed. The genuinely silent hazard is a **partial** intersection — extra Bismark contigs (scaffolds, alt/decoy, `chrM` vs `chrMT`) are dropped without comment and the run "succeeds" with missing data. Also warn that wgbstools' CpG dictionary comes from *its own* FASTA, so a patch/build skew is silent, and that `patter`'s `--clip` operates on the **cleaned, reference-space** sequence (`patter.cpp:169`), not read cycles.
10. **Tests** — §9.
11. **Docs + CHANGELOG + `rust/README.md`**.
12. **Pre-push gates** — `cargo fmt -p bismark -- --check`; `cargo clippy -p bismark --all-targets`; and if feature-gated code is touched, `RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings`.

---

## 6. Efficiency

- Streaming, O(1) records in memory; O(read_len) per record.
- **No genome load** — the consensus path's `read_genome_into_memory` (`mod.rs:531`) is several GB and multiple seconds avoided.
- Reusing `make_mismatch_string` costs one `String` per record plus a `strip_prefix("MD:Z:")`; irrelevant next to BGZF. Effectively I/O-bound — if it ever needs to be faster the win is parallel BGZF, not the maths.
- **`MD`/`NM` churn is total, not marginal** (R18). Under bisulfite convention nearly every unmethylated cytosine is an `MD` mismatch; under 5-Base only 5mC positions are. So essentially *every* record's `MD` changes and `NM` rises. Harmless downstream (nothing in the `bam2pat` path filters on `NM`), but it means any `MD`-dialect error is pervasive rather than rare — which is why §3.4 step 2 checks every record rather than sampling.

---

## 7. Integration

- **Reads:** Bismark 5-Base BAM(s), read-order. **The 5-Base consensus BAM (`five_base_consensus.bam`) is a valid input** and is the first thing a duplex user will try: its records go through `five_base_emit_record` → `single_end_sam_output` (`mod.rs:2298-2311`), so `XM`/`XR`/`XG`/`MD`/`NM` are all present and internally consistent, with `{L}M` CIGARs and SE-style FLAG 0/16. `bam2pat` reads only the first record's `flag & 1` (`bam2pat.py:262-267`) and so treats it as single-end — correct, one record per family.
- **Writes:** `<stem>.bisulfite.bam` + `<stem>.bisulfite_report.txt`.
- **Header is copied** from the input, unlike `run_five_base_consensus_standalone`, which synthesises one from the genome (`mod.rs:533`). Deliberate — this path has no genome. Worth the comment so it isn't "fixed".
- **Order:** strictly post-alignment; dispatch short-circuits before `pipeline()`, so aligner byte-identity is structurally untouched.
- **`XM` is never modified**, so the output stays readable by Bismark's own extractor — and `bam2pat --ds_test` keeps working, since `is_pass_ds_test` reads `XM:Z:` directly (`patter_utils.cpp:350-398`).
- **`--five_base_baseq` interaction:** see §3.6. This is the one place where the "`SEQ` and the call string were built from different sequences" design becomes a correctness issue rather than a curiosity.
- Output is read-order, not coordinate-sorted (§10 Open-1).

---

## 8. Assumptions

**Fixed (verified in code):**

1. `len(XM) == len(SEQ)`; `XM[i]` ↔ `SEQ[i]` in BAM space (§3.1.1).
2. `XG:Z:CT` ⇒ ref base `C`, pair `(C,T)`; `XG:Z:GA` ⇒ `G`, `(G,A)` (§3.1.2).
3. `XM` letters occur only at `M`-consuming positions; `I`/`S` are always `.` (§3.1.3).
4. Bismark writes `NM`, `MD`, `XM`, `XR`, `XG` on every mapped record (`output.rs:485-489`).
5. **`NM` = mismatches at `M` + inserted + soft-clipped + deleted bases** (§3.4). It is `ext.indels` that is deletions-only. *(Corrected in rev 1 — rev 0 stated this backwards.)*
6. `XM` vocabulary is exactly `Z z X x H h U u .`.
7. At every letter position `seq_old[i] ∈ {meth, unmeth}` (§3.3.2).
8. `XG` ⟺ FLAG for every Bismark record, PE agreement depending on the `#1030` swap (§3.8).

**From the reporter** ([#787](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5167178704)): will re-run through `bismark --illumina_5base`; `bam2pat` defaults plus `--clip`; hg38 via `wgbstools init_genome hg38`; paired-end; deconvolution not needed at 10X.

**Configurable:** input BAM list and `--output_dir`. **Nothing else** — the encoding, and now the §3.6 masking rule, are both fully determined by the record. Rev 1 needed a threshold flag; rev 2 has no tunable behaviour at all.

---

## 9. Validation

### 9.1 The idempotence gate — identical **SAM text** (R2, R10, R11)

**Run the converter on a standard *bisulfite* Bismark BAM. The output's SAM text must be identical to the input's** (modulo the added `@PG`). It holds by construction: for bisulfite data `SEQ` already carries `meth`/`unmeth` at every letter position, so nothing moves.

Not *byte*-identical — BGZF block boundaries are writer-dependent and `NM` is re-encoded as `i32` (`output.rs:485`), so a third-party `NM` stored as `C`/`c`/`s` returns as `i`. Compare decompressed SAM bodies.

The gate must pass `--illumina_5base` even on bisulfite input (R11): §5.3's guard exists to stop an *unwitting* user, and the flag is the test's assertion of intent. Comment it in the test.

**Fixtures — corrected (R2).** Rev 0 named `test_files/*_from_TrimGalore.bam`; those are `trim_galore --output-format ubam` outputs with **0 `@SQ` lines** and no `XM`/`XR`/`XG`/`MD`/`NM`. Verified. Use instead:

| Fixture | Why |
|---|---|
| `tests/data/dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` | Real `bismark_bt2` PE, 12974 records, **42 with `I`/`D`** — exercises the deletion re-indexing path |
| `tests/data/dedup/nondir_pe_1030.bam` | **Non-directional PE — the only fixture covering all four strand indices.** Without it the "whole encoding table" claim is false, since a directional fixture holds only two. (Also present at `tests/data/extractor/`; either path works) |
| `tests/data/filter_nonconversion/{se_default,pe_default}/` | SE + PE coverage |
| `tests/data/filter_nonconversion/se_unmapped/in.bam` | Contains unmapped records — gates §3.7's pass-through |

✅ **The soft-clip fixture now exists** — `tests/data/five_base_bisulfite/softclip_indel_se.bam` (see that directory's `README.md`). It was the last gate on implementation.

No pre-existing fixture had a single `S` op (verified: 0 of 12974 and 0 of 20). Bowtie 2 end-to-end never emits one, so no `bismark_bt2` fixture ever will — but minimap2 `-x sr` is the default 5-Base engine (`mod.rs:1173-1180`) and `--five_base_umi_len` *depends* on soft-clipping the UMI prefix (`cli.rs:125-128`), which makes `nS`-prefixed reads the dominant real 5-Base shape and exactly where the corrected `NM` (§3.4) bites.

8 SE records over pUC19 (2686 bp), Bowtie 2 2.5.5 `--local`, **byte-identical between the Rust aligner and live Perl `bismark` v0.25.1** — all fields including MAPQ, so `MD`/`NM`/`XM` are oracle-authentic:

| QNAME | FLAG | CIGAR | `XG` | Covers |
|---|---|---|---|---|
| `ot_plain` / `ob_plain` | 0 / 16 | `80M` | `CT` / `GA` | controls |
| **`ot_softclip`** | 0 | **`9S81M`** | `CT` | leading clip |
| **`ob_softclip`** | 16 | **`81M9S`** | `GA` | trailing clip |
| `ot_ins` / `ob_ins` | 0 / 16 | `40M1I40M` | `CT` / `GA` | insertion |
| `ot_del` / `ob_del` | 0 / 16 | `40M2D40M` / `35M4D45M` | `CT` / `GA` | deletion (`MD` `^` path) |

The two `*_softclip` records are a deliberately **asymmetric pair** — the foreign prefix sits at each read's 5′ end, and the output revcomp moves it to the opposite end of `SEQ` for the reverse record. That is §9.3's `read_pos_5p` guard available as real data rather than a hand-built record.

**Four of this plan's invariants were confirmed on it at generation time**, which is worth more than the fixture itself: `len(XM) == len(SEQ)` on clipped and inserted records; `NM == mismatches + inserted + soft-clipped + deleted` (e.g. `ot_softclip` `20 == 11 + 0 + 9 + 0` — the empirical proof of §3.4's corrected identity); `XM == '.'` at every `S`/`I` position; and `SEQ[i] ∈ {meth, unmeth}` at every letter position across both strands with clips and indels present (§3.3.2).

`pUC19.fa` ships alongside it so §9.5's oracle can build its reference **independently of `MD`**.

Assert the CIGAR census in test setup so a future fixture swap cannot silently drop coverage.

### 9.2 The forward check — rewritten, was vacuous (R6)

Rev 0 asserted via `extract_calls`, which reads **`XM`** — the one thing the converter never touches. It passed regardless of what happened to `SEQ`, **including with the meth/unmeth pair swapped**. Since §9.7 needs real data, that left **the sign untested in CI**.

Assert on `SEQ` directly:

1. For every XM-letter position: `seq_new[i] == meth` iff `XM[i]` is upper-case, per the `XG` table.
2. **Direction pin:** on 5-Base input at least one position must go `T → C` (`XG:CT`) or `A → G` (`XG:GA`).
3. **Flip rate** (R7): `flipped == letters` on 5-Base input; `flipped == 0` on bisulfite. Assert exactly, not approximately.
4. `XM` unchanged.

A converter with the pair swapped produces perfectly self-consistent output and passes everything else in §9. Checks 2 and 3 are what catch it.

### 9.3 The OB-strand orientation trap

A `-`-strand record with a deliberately **asymmetric** `XM` (one letter near each end, different case) must flip the correct end. Hand-built; assert exact expected `SEQ`; construct it so a `read_pos_5p`-indexed write yields a *detectably different* string rather than a coincidentally equal one. **This is the guard against the `iter_aligned()` bug in §2.**

### 9.4 Gaps

Soft-clipped and inserted positions untouched (`{n}S{m}M`, `{a}M{b}I{c}M`), and a fault-injected letter at an `S` position errors with `CallInGap`. Pair with the real soft-clip fixture from §9.1 — synthetic-only coverage is not enough for the common case.

### 9.5 `NM`/`MD` exactness

§3.4 step 2 already proves the reconstruction per record on every input. Add an independent from-genome oracle for the *emit* direction — and it **must build its reference independently of `MD_old`** (from a small synthetic genome and the alignment), or it degenerates into asserting a function equals itself.

Fixtures in priority order: **leading soft clip** (the common 5-Base shape), insertion, mismatch immediately adjacent to a deletion (`rebuild_md_with_deletions`' re-indexing path, `output.rs:303-334`), multi-deletion (`md_index_already_processed`, `:327-331`).

Expected: exact agreement, including insertions **and soft clips** counted as mismatches (§3.4).

### 9.6 Fail-loud matrix

Missing `XM` / `XR` / `XG` / `MD` / `NM`; length mismatch; bad `XM` byte; `H`/`=`/`X`/`P` in CIGAR; letter at an `I`/`S` position; round-trip failure (§3.4 step 2); flip rate strictly between 0 and 1; both `--five_base_*_from_bam`-family flags together; missing `--illumina_5base`. Assert on **message content**, not just `is_err()`.

(Rev 1 also needed rows for an unresolvable baseq threshold and for the flag disagreeing with the header. §3.6's rule has no trigger condition, so both are gone — as is the ambiguity rev 1 carried about which of those two cases its fail-loud actually covered.)

Plus (R17): assert the output contains **no `MM:Z:`**. `detect_nanopore` (`bam2pat.py:243-259`) silently switches to `--nanopore` mode — reading `MM`/`ML` instead of `SEQ`, with `-q 0 -F 3844` — if the header has `\tPL:ONT` or any of the first 200 records has `\tMM:Z:`. Bismark writes neither today, and `\tXM:Z:` does not match `\tMM:Z:`, so this is cheap insurance against a future tag-preserving input path silently defeating the whole feature.

### 9.7 The masking rule (R1, T1)

Every assertion here is on the **emitted byte**, not on `is_cpg` — a Rust test cannot call `patter`'s C++.

1. **Positive.** A `.` position at a reference cytosine whose `SEQ` base is in {meth, unmeth} becomes `N`, and `masked` counts it. Build one such position in a record and assert `masked == 1` **exactly** — not `>= 1`.
2. **🔑 Negative — the assertion rev 1 lacked.** In the *same* record, a `.` position at a reference **non**-cytosine whose `SEQ` base happens to be `C`/`T` must be **byte-identical to the input**. This is what fails if the positional conjunct is dropped, which is exactly the over-masking that both rev 1's `QUAL` rule and B's original proposal would have caused.
3. **Gaps are never written.** A soft-clipped and an inserted position, both with `SEQ ∈ {meth, unmeth}`, stay untouched — `ref_seq[i] == b'X' != ref_base`. Guards the inline-UMI corruption case under `--five_base_umi_len`.
4. **The `is_cpg` neighbour survives.** On an OT record, the `G` at `j+1` (reference `G`, so `XM == '.'`) must be unchanged, so the call at `j` stays scoreable. Symmetric for `C` at `j-1` on OB. This is the good-call destruction that rev 1's `QUAL` rule caused.
5. **Disjointness (T2).** No letter position is ever masked, and `masked` is never in the flip-rate denominator: assert `flipped <= letters` and `flipped/letters ∈ {0.0, 1.0}` on a record that *also* has masked positions. A loop-level flip counter fails this — and would otherwise abort every real masked run.
6. **`NM` bookkeeping.** A 100 bp record with two masked positions must give `NM_new == NM_old + 2`. A whole-read mask fails this loudly.
7. **Provable emptiness (property 2).** On every §9.1 bisulfite fixture, `masked == 0`. **This is what makes §9.1 the regression test for the masking path** — rev 1's threshold-based mechanism could not be reached by §9.1 at all, which is why its bug would have shipped.

Nothing in rev 0's §9 caught any of this, and rev 1's §9.7 tested only direction 1 — the direction in which over-masking is invisible.

### 9.8 `XG` ⟺ FLAG (R8)

Assert `XG == "CT" ⟺ FLAG ∉ {16, 83, 163}` over `nondir_pe_1030.bam` — the only fixture with all four combinations. The equivalence is load-bearing and invisible to §9.1, so a future change to the flag table must break a test rather than the science.

### 9.9 End-to-end concordance (manual, needs real data)

Convert → sort → index → `bam2pat` → compare per-CpG methylation against Bismark's own cytosine report. Concordance-gated, **not** byte-identical (`patter` applies its own `--clip`, MAPQ ≥ 10, `-F 1796`). A negative or near-zero correlation is the signature of the inversion surviving, so this test's real job is proving the **sign**.

Run **two arms: `--five_base_baseq 0` and `> 0`.** Under §3.6 the masked run is where the sign was still wrong, and a whole-genome correlation would dilute a partial inversion below notice.

---

## 10. Questions or ambiguities

**No critical questions remain.**

| # | Priority | Question | Assumption taken |
|---|---|---|---|
| Open-1 | Open | Coordinate-sort + index the output? | **Read-order + a `Note:`.** Matches `ubam.rs:214/239`, keeps this a pure streaming pass. The read-order output is mate-adjacent, which is what a later `samtools sort` wants anyway. Note the `Note:` is **mandatory** (§5.8) |
| Open-2 | Open | Output naming | `<stem>.bisulfite.bam` — 1:1 transform, so a fixed name would collide |
| Open-3 | Open | Mask `--five_base_deconvolution` variant sites to `N`? | **Deferred.** Not needed at 10X. Note `five_base_deconv.rs` writes only a report and never touches `XM`, so there is no existing masking to reuse; a real `C>T` het at a CpG *already* reads as methylated in Bismark's own `XM` and cytosine report, so the converter propagates a pre-existing limitation rather than adding one — though it additionally erases the variant from `SEQ` |
| Open-4 | Open | Also pursue the upstream `patter` patch? | **Yes, in parallel** (`DRAFT_upstream_wgbs_tools_PR.md`). One note for it: `is_cpg` hardcodes `{C,T}`/`{G,A}` independently of `ReadOrient` (`patter.cpp:96-103`); those happen to be the right sets for 5-Base, so no change is needed there — but a *generality* claim would be wrong |
| Open-5 | Open | Drop `MD`/`NM` instead of maintaining them? | **Rejected, but priced (R19).** Nothing in this repository reads either tag — `tags::md`/`tags::nm` have no call sites outside their own unit tests — and `patter` reads only `SEQ`. But a BAM carrying an `MD` that disagrees with its `SEQ` is worse than one carrying none, and generic consumers (IGV, `samtools stats`, variant callers) do read it. §3.4's reuse makes exact `MD` nearly free, so keep it. **Note this is now doubly load-bearing:** §3.6's masking rule depends on the reconstructed `ref_seq`, which comes from `MD`. Dropping `MD` would take the leak fix with it |
| Open-6 | Open | Keep a `--five_base_bisulfite_baseq` flag as an *assertion* (`0` ⇒ require `masked == 0`)? | **Dropped (T1).** It has no role in identifying the masked set any more, and `masked` plus the `Note:` already surface what happened — more informatively than a threshold the user has to remember. Cheap to add later if someone wants a scriptable "this file should have no masking" check; not worth a permanent public flag on speculation |

---

## 10b. Implementation notes (2026-08-07)

**Implemented and green.** `cargo fmt --check` clean, `cargo clippy --all-targets` clean, full `cargo test -p bismark` green (1469 lib + 116 aligner-cli + 7 new integration + all other suites, 0 failures).

| Piece | Where |
|---|---|
| Per-record algorithm + 21 unit tests | `rust/bismark/src/aligner/five_base_bisulfite.rs` |
| CLI flag, mutual exclusion, driver | `rust/bismark/src/aligner/{cli.rs, mod.rs}` |
| 7 integration gates | `rust/bismark/tests/aligner_five_base_bisulfite.rs` |
| Docs / CHANGELOG / Milestones | `docs/.../rust/illumina-5-base.md`, `CHANGELOG.md`, `rust/README.md` |

### End-to-end validation — the feature is proven, not just tested

Run against the real 5-Base BAM from `EXPERIMENT_patter_swap.md`, using **stock, unmodified** `patter`:

| Route | METH / total | Methylation |
|---|---|---|
| stock `patter` + raw 5-Base BAM | 0 / 152 | **0.0 %** ❌ |
| patched `patter` + raw 5-Base BAM | 152 / 152 | 100.0 % ✅ |
| **stock `patter` + converted BAM** | **152 / 152** | **100.0 %** ✅ |

Ground truth is 100 % (pUC19 fully CpG-methylated); Bismark's own `XM` agrees (`Z=168, z=0`). So the converter makes **unmodified** wgbs_tools correct, and agrees call-for-call with the independent `patter` patch. Flip rate on that input was exactly `1.000000` (872/872); on the bisulfite fixture exactly `0.000000` with the SAM body **byte-identical**.

### Deviations from the plan

| # | Deviation | Why |
|---|---|---|
| D1 | Typed `FiveBaseBisulfiteError` (thiserror) converted to `AlignerError::Validation` at the driver boundary, rather than the plan's flat signature | Gives the unit tests structural matching while keeping CLI behaviour consistent with the rest of the aligner |
| D2 | **noodles owns the `@PG` chain.** Our `@PG` gets a distinct ID as planned (T6), but noodles re-links the chain on serialisation, so an input's `@PG ID:samtools PP:Bismark` comes out as `PP:bismark-five-base-bisulfite` — the chain stays internally consistent but no longer reflects the true running order. Setting `PP` explicitly does not prevent it | Metadata only; nothing in Bismark or `bam2pat` walks `PP`. Recorded in a code comment, including "do not fix by dropping the `@PG`" — a re-encoded BAM must carry a record that it was |
| D3 | **New guard not in the plan:** an input with records read but **none re-encoded** now fails loud | Found by the uBAM test. Every uBAM record is unmapped, so all took the verbatim pass-through path and the run "succeeded" while emitting a copy of the input — the user would believe they had converted something. Exactly the silent-no-op class §3.6 exists to prevent |

### Iteration log

`#1` — Module compiled first try; one `unused_mut` on the reference-reconstruction closure. Fixed (CI runs `-D warnings`, so a warning is a failure).

`#2` — 18/21 unit tests passed; **all three failures were my test data, not the code.** `gaps_are_never_written` expected `CCACGTAC` but got `CCACGTAN` — offset 7 is a reference `C` with a scoreable base and no call, so §3.6 correctly masked it. `insertions_are_never_written` used `'C'` as an XM byte (not in the vocabulary) and correctly raised `InvalidXmByte`. `asymmetric_calls_edit_the_correct_positions` put a call at a `G`, correctly raising `SeqNotInPair`. Rewrote all three against hand-verified records; the masking cases now assert the *contrast* between a gap position and a genuine uncalled cytosine, which is what the positional conjunct buys.

`#3` — Driver compiled after fixing four import/type errors (`Tag` path, `RecordBuf` path, program-tag constants, `BismarkIoError` needing `map_err` — `AlignerError` has no `From`).

`#4` — `@PG` chain investigated and D2 recorded. First attempt set `PP` to the input's last program to force an append; noodles ignored it and re-linked anyway. Comment corrected to state the observed behaviour rather than the intended fix — a comment asserting something untrue is worse than none.

`#5` — 6/7 integration tests passed; the uBAM case exposed D3. Added the fail-loud guard and tightened the test to assert the message explains nothing was convertible.

`#6` — `cargo fmt` reformatted three files (error-attribute wrapping, closure params). Clippy and the full suite green.

### Not done — deliberately out of scope

- **§9.5's independent from-genome `NM`/`MD` oracle.** The per-record round-trip proof (§3.4 step 2) already validates the reconstruction on *every* record of every input, including the deletion path, and the committed fixture confirms the `NM` identity on real data. A second oracle would be worth adding if `MD` handling ever changes.
- **§9.8's `XG` ⟺ FLAG assertion** over `nondir_pe_1030.bam`. The equivalence was verified by hand and in `PLAN_REVIEW_B` §1.3; it is not yet a test. **Worth adding** — it is load-bearing and invisible to the idempotence gate.
- **§9.9 real-data concordance** needs Mike's data.

## 11. Self-Review

**What rev 1 changed and why.** Both reviewers verified the algorithm independently across all four SE indices and all eight PE mate/index combinations; §3.1's three properties and the idempotence *argument* hold. Every rev-1 change is in validation, I/O plumbing, or §3.6.

**The two errors in rev 0 that mattered most:**

1. **§9.1 named fixtures that cannot run it.** The primary gate — the one §11 called load-bearing — pointed at Trim Galore uBAMs. I took this from a memory note that literally contained the word "uBAM" and never ran `samtools view -H`. A fixture named in a validation section is a claim about a file, and claims about files are one command from being checked.
2. **The `NM` invariant was stated backwards.** I reasoned from `ext.indels` (genuinely deletions-only) and missed that insertions and soft clips are already counted *upstream* by `hemming_dist` via the `X` padding. Two mechanisms feed one number and I audited one. The formula survived; the prose would have sent an implementer to build an oracle that contradicts the code — while rev 0's own ⚠️ told them not to "fix" `NM`.

**The design hole neither I nor Reviewer A found:** §3.6. Reviewer B traced the `--five_base_baseq` masking path into `patter` and found the inversion surviving in the output. I rejected B's proposed remedy — it would have `N`-masked roughly half of every read — and used `QUAL` to identify the actually-masked set instead.

**Rev 2, first: rev 1's own remedy had the same defect it rejected.** Rev 1's §3.6 masked where `QUAL[i] < offset + n`, lifted straight from the align-time comparison in `mask_low_quality`. But a BAM stores 0-based phred scores, not ASCII (`output.rs:437-440`), so that would have masked *every* `XM == '.'` position — the exact "destroys the file" outcome I had just rejected B's proposal for, arrived at from the other direction. Reusing a comparison across a serialisation boundary silently changes its units.

**Rev 2, second: the whole mechanism was wrong, not just its arithmetic.** The targeted review's F1 observed that §3.4 step 1 *already* reconstructs the reference base at every position, and that `{XM == '.'} ∩ {ref == ref_base} ∩ {SEQ ∈ {meth, unmeth}}` is provably exactly the leak set and provably empty without masking (verified: at a genomic `C` the CT branch emits a letter for both `C` and `T` with no further guard, `methylation.rs:587-596`). Rev 1 had been reaching outside the record — to a header, a threshold, an encoding offset — for information the record already contained. Adopting it deleted a CLI flag, a header parser, a conflict rule, a fail-loud fallback, two test rows, and one "remaining risk".

**Three separate attempts at the same twelve lines, and the first two failed the same way.** B's rule over-masked because it had no positional test; rev 1's over-masked because of a units error. That is the argument for §9.7's negative controls (T4): rev 1's §9.7 asserted only that a maskable position gets masked, which an implementation that masks *everything* satisfies perfectly. The test suite could not distinguish the fix from the disaster.

**And one false-abort that no amount of arithmetic care would have caught (T2):** rev 1's §3.2 put the flip counter at loop level, so every mask would have incremented `flipped`, pushing the rate above 1 and tripping §3.5's fail-loud on every correctly-masked run.

**Where the reviewers disagreed**, resolved by checking myself: §9.2 was vacuous (A right, B too generous); the fixtures *do* contain indels (A right, B's CIGAR census wrong); `nondir_pe_1030.bam` exists at both cited paths (neither wrong). B's soft-clip gap survives its own bad census — zero `S` records anywhere — and that is now §9.1's required new fixture.

**Remaining risks.**

1. **The soft-clip fixture does not exist yet** and must be generated from a live oracle. Until it does, the dominant real-world 5-Base CIGAR shape is untested — and §3.4's corrected `NM` is precisely where it bites.
2. **§9.9 needs real 5-Base data** and cannot run in CI. Everything else is hermetic — and R6/R7 mean the *sign* is now covered hermetically, which it was not in rev 0.
3. **`reconstruct_ref` is now doubly load-bearing.** It was already the input to `NM`/`MD` (§3.4); under T1 it is also what identifies §3.6's masking set. A bug there is no longer just a wrong tag — it silently changes which bases get masked. Mitigated by §3.4 step 2 proving the reconstruction byte-exact **per record** before anything is emitted, which is a stronger guard than rev 1 had anywhere. (Rev 1's risk 3 was "`@PG` parsing is the one fragile mechanism in the plan"; T1 deleted that mechanism.)
4. **The output is inherently misleading if mishandled** — `SEQ` disagrees with the sequencer. Mitigations are naming, a distinct report, docs, and never making it the primary BAM; none stop a user feeding it to a variant caller.
