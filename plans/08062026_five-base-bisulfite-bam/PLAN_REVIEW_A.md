# PLAN_REVIEW_A — `--five_base_bisulfite_bam`

**Reviewer A** · fresh context · target `plans/08062026_five-base-bisulfite-bam/PLAN.md` · repo at `dev` `bfdf207`

**Verdict: the design is sound; the validation strategy has two unsound legs and one unimplementable behaviour.**

I verified all three load-bearing properties in §3.1 against source, plus the idempotence argument across all four SE strand indices and all four PE index/mate combinations. They hold. What does not hold: the plan's characterisation of Bismark's `NM` (C2), the fixtures named for its primary gate (C1), the unmapped-passthrough behaviour given the reader it picks (C3), and §9.2's method, which is vacuous as written (I8).

---

## 1. Claims that check out

Stated briefly, per instruction — no padding.

**§3.1.1 `len(XM) == len(SEQ)`, lockstep reversal — VERIFIED.**
Parity enforced at `rust/bismark/src/io/record.rs:122-130` (`XmSeqLengthMismatch`), relied on at `:301`. `SEQ` and `XM` are reversed together for `-` strand: SE `rust/bismark/src/aligner/output.rs:443-450` + `:463-467`; PE `output.rs:671-675` + `:694-698`. The plan cites only the SE lines — PE is identical and, since 5-Base is paired-end only (`cli.rs:99-100`), PE is the path that actually matters. Add the PE citation.

**§3.1.2 `XG` fixes the reference base and the `(meth, unmeth)` pair — VERIFIED, but the plan's derivation is too short.**
The subtlety the plan glosses: `methylation_call` branches on **`XR`**, not `XG` (`methylation.rs:575`, `:583`). The XG relation is emergent, from three facts composed:

| | index→(strand, XR, XG) | branch's ref base | output revcomp | ⇒ BAM ref base |
|---|---|---|---|---|
| SE 0 | `+`, CT, CT | `genomic[i]=='C'` | no | `C` |
| SE 1 | `-`, CT, GA | `genomic[i]=='C'` | yes | `G` |
| SE 2 | `-`, GA, CT | `genomic[i+2]=='G'` | yes | `C` |
| SE 3 | `+`, GA, GA | `genomic[i+2]=='G'` | no | `G` |

(`methylation.rs:131-141`, `:584-620`, `output.rs:443-450`.) I walked the PE table too (`methylation.rs:421-433`, `output.rs:546-556`, `:671-675`) — all eight mate/index combinations agree, including the index-keyed rather than XR-keyed `+2` trim. The `(meth, unmeth)` pair follows the same way: CT-branch meth read base `C` → `G` after revcomp, GA-branch meth `G` → `C`. So the table in §3.1.2 is right for all four indices, SE and PE.

Confirmed on real fixture data — `rust/bismark/tests/data/dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` record 1: `XG:Z:GA`, `XM[50]=='Z'`, `SEQ[50]=='G'`, `MD:Z:...3G7C1` places a match at offset 50 ⇒ ref `G`. And `XM[49]=='.'` where `SEQ[49]=='N'` over ref `C` — the `.`-on-mismatch path.

**Action:** write the derivation into the plan. `methylation.rs:131-135` alone does not show it, and the first maintainer to read `:575` will conclude the branch key is `XR` and that §3.1.2 is wrong.

**§3.1.3 `I`/`S` are structurally `.` — VERIFIED.**
`I`/`S` pad with `b'X'` without advancing `pos`: SE `methylation.rs:174-181`, PE `walk_mate` `:349-354`. `X` matches no read base and is not `C`/`G`, so both branches fall to `.` (`:597-599`, `:615-617`). For the GA branch the `i+2` offset lines up because the `+2` context sits at the **front** of the window for every GA-branch record — index 3 by prepend, index 2 by append-then-revcomp — so window index `i+2` ↔ read position `i`, and the padding lands there. `reverse_complement` (`:60-71`) leaves `X` unchanged, so the padding survives both revcomps.

**§9.1's idempotence argument itself — SOUND.**
Walked all four SE indices and all eight PE mate/index combinations. With `five_base=false` a letter is emitted only where the read base already equals `meth` (upper) or `unmeth` (lower) in read space; the joint revcomp carries that into BAM space. The two cases the task flagged as possible breakers do not break it:
- **Mismatches at cytosine positions**: a genomic `C` under a read `G`/`N` yields `.` (`methylation.rs:597-599`) — never rewritten. Only `C`/`T` (CT) or `G`/`A` (GA) reach a letter.
- **Indels**: `D` has no read position; `I`/`S` are `.` per §3.1.3.

So the gate is legitimate as a *concept*. It is unsound as *specified* — see C1, C3, I3.

**§3.3.2 cross-check as a hard error — RIGHT CALL, cannot fire on genuine data.**
At a letter position the read base is `meth` or `unmeth`. If it is `meth` (== `ref_base`), `MD` must record a match ⇒ reconstructed ref == read base == `ref_base`. If it is `unmeth`, `MD` must record a mismatch carrying the ref base ⇒ reconstructed ref == `ref_base`. The check can therefore only fire when `MD_old` disagrees with `SEQ_old`, which is precisely the condition you want to hear about. Keep it as a hard error.

**§2's refusal to use `iter_aligned()` — CORRECT.**
`record.rs:274-322` returns `read_pos_5p`, reverses order and re-indexes for OB/CTOT (`:310-319`), and additionally *skips* `I`/`S` (`:295-296`) so it cannot even address every `SEQ` byte. The standing warning is at `extractor/call.rs:182-183` as cited. The replacement assertion is genuinely stronger than a CIGAR gate — but note it is an assertion about the *input*, not a guard on the *write*, so it only works because §3.2's loop is unconditional over `0..seq.len()`. It is. Consistent.

**§3.4 `U`/`u` inclusion — CORRECT and correctly motivated.** `push_ct_context` maps an `X` context base to `U`/`u` at `methylation.rs:641-643`; `push_ga_context` at `:673-675`.

**Line citations — spot-checked ~20, all resolve to what the plan says.** Including `output.rs:485-487` (MD always written), `mod.rs:167` (dispatch), `mod.rs:515` / `:536` (consensus precedent), `cli.rs:183`, `ubam.rs:214/239`, `io/tags.rs:21-63`.

---

## 2. Critical findings

### C1. §9.1 names Trim Galore **uBAMs** as the bisulfite fixtures. They are unaligned and carry no `XM`.

`test_files/BS-seq_10K_se_trimmed_from_TrimGalore.bam` header:

```
@HD	VN:1.6
@PG	ID:trim_galore	PN:trim_galore	VN:2.2.0	CL:... --output-format ubam ...
```

No `@SQ` lines, no alignments, no `XM`/`XR`/`XG`. Both `test_files/*_from_TrimGalore.bam` files are Trim Galore uBAM outputs — Phase-1 input fixtures for `--single_end`/`-1/-2` transcoding, not Bismark output. The plan's primary gate cannot run on them, and this is the one test everything else leans on (§11.3 says so explicitly).

Real aligned bisulfite fixtures do exist and are better than what the plan asked for:

- `rust/bismark/tests/data/dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` — PE, `bismark_bt2`, and a CIGAR census over the first 5000 records gives **5023 M, 12 I, 11 D**, plus mismatch-dense `MD` strings. Exercises the deletion re-indexing path and the insertion path in one file.
- `rust/bismark/tests/data/filter_nonconversion/{se_default,pe_default}/` — SE and PE.
- `rust/bismark/tests/data/dedup/nondir_pe_1030.bam` — **non-directional PE**, i.e. the CTOT/CTOB indices. §9.1 claims the gate validates "the whole encoding table" across all four strand indices; a directional fixture only contains two of them. This file is what makes that claim true. Use it.

**Action:** replace the fixture list. Assert the CIGAR census in the test setup so a future fixture swap cannot silently drop indel coverage.

### C2. Bismark's `NM` does **not** exclude insertions. It includes them — and it also counts soft-clipped bases.

`hemming_dist` (`output.rs:150-157`) is `actual.len() - matches` over a positional zip of read against `ref_seq`, and `ref_seq` carries `b'X'` at every `I`/`S` position (`methylation.rs:174-181`). `X` never equals a read base, so **every inserted and every soft-clipped base contributes +1**. `ext.indels` (D only) is added on top (`output.rs:453` SE, `:690` PE). The actual identity is:

```
NM = (mismatches at M) + (#inserted bases) + (#soft-clipped bases) + (#deleted bases)
```

Standard SAM `NM` is mismatches + insertions + deletions, soft clips excluded. So the real Bismark quirk is **soft-clip inflation**, and insertions are counted normally. The plan asserts the opposite in three places: §3.3's ⚠️ block ("`indels` counts deletions only … a Perl-faithful quirk that excludes insertions"), §8.5 ("Bismark's `NM` excludes insertions"), and §11's self-review item 1.

The **formula in §3.3.4 is still correct** — it only touches `M` positions, and the plan's own one-line justification for that ("the delta is safe because XM letters only ever occur at M positions") is the right reasoning. The problem is that the *stated invariant* is false and load-bearing:

- §9.5's oracle is specified as "mirroring `output.rs:452-460`" while §3.3's prose describes a different `NM`. An implementer who builds the oracle from the prose gets a disagreement on every record containing `I` or `S`, and the plan has pre-emptively told them not to "fix" `NM` — so the likely resolution is to change the converter, introducing exactly the divergence the plan was trying to prevent.
- This is not an edge case for this feature. **5-Base BAMs routinely contain soft clips.** minimap2 `-x sr` soft-clips, and `cli.rs:123-131` documents that `--five_base_umi_len` *relies* on the aligner soft-clipping the UMI prefix ("A non-soft-clipping aligner would mis-frame the call"). Soft-clip-inflated `NM` is the norm for the exact input this feature targets.

**Action:** (a) correct §3.3 / §8.5 / §11.1 to state the real identity; (b) keep the delta and keep the "letters only at M" justification; (c) §9.5's fixture list must add a **soft-clip record** and an **insertion record** — "Include a deletion-containing record" is the least important of the three.

### C3. The reader the plan reuses silently drops unmapped records, so §3.5's pass-through is unimplementable as designed.

`BamReader::records()` is `filter_map(filter_unmapped_then_classify)` and drops `FLAG & 0x4` (`io/read.rs:268-274`, `:628-645`; documented at `:7`; asserted by the test at `:1100-1128`: *"unmapped read (FLAG & 0x4) must be silently dropped, not surfaced"*). `from_path_without_sort_check` — the plan's §2 "Existing code to reuse" — returns exactly that reader, and `BamReader`'s `inner` is private with no raw-records accessor.

Consequences:

1. §3.5's "Unmapped (`0x4`) → Passed through **unchanged**, counted" cannot be done. The driver would **delete** them. The plan's stated reason for passing them through ("copying faithfully keeps the output a true sibling of the input") is the correct instinct and the chosen API defeats it.
2. §9.1's gate silently degrades to "identical except unmapped reads are gone" on any input containing them. Bismark's own aligner BAMs don't, so the gate would pass — and then break on a user's BAM.
3. `from_noodles_record` (`record.rs:116-141`) additionally **requires `XR`** and enforces XM/SEQ parity *at the reader*. So §3.5's "Secondary / supplementary → pass through unchanged" hard-errors instead: a secondary record without tags never reaches the converter. §3.5's fail-loud row for `len(XM) != len(SEQ)` also fires at the reader as `BismarkIoError::XmSeqLengthMismatch`, with no file or QNAME context — contradicting §3.5's "Name the file and QNAME".

**Action:** read with `noodles_bam::io::Reader` directly inside the new module (the header comes off it the same way), classify per record yourself, and write pass-through records with the **already-existing** `BamWriter::write_raw_record` (`io/write.rs:86`), which was added for exactly this ("records that are deliberately *not* Bismark-shaped"). Adding a raw-records accessor to bismark-io is the other option but it is a published-crate API change and drags in the exact-pin cascade — call that out in the plan rather than discovering it mid-implementation.

### C4. There is no public API to mutate `SEQ`/`NM`/`MD` on a `BismarkRecord`.

`record.rs:179-230` exposes `inner()` (immutable), `xm()`, `alignment_start()`, `cigar()`, `umi()`, `set_umi()`, `set_rx()`. No `inner_mut()`. §5's driver loop ("per record classify → pass-through or re-encode → write") has no way to apply a `Reencoded` to a record.

Workable route: clone `inner()` into a `RecordBuf`, set `sequence_mut()`, `data_mut().insert(NM/MD)`, then write. Worth pinning down because the obvious alternative (add `inner_mut()` to bismark-io) is an API change to a published crate.

Good news while you are there: `Data::insert` **replaces in place and preserves field order** (noodles-sam 0.85.0 `src/alignment/record_buf/data.rs:222-232` — `get_index_of` then `mem::replace`, `push` only when absent). So overwriting `NM`/`MD` cannot reorder the tag block, and §9.1's SAM-text comparison is safe on that axis.

---

## 3. Important findings

### I1. Do not write a new `emit_md`. Reuse `output::make_mismatch_string`.

This is the largest risk reduction available and it costs nothing. `make_mismatch_string` and `hemming_dist` are `pub(crate)` in `crate::aligner::output`; the new module lives in `crate::aligner`, so both are directly callable. The reuse works because both of the emitter's inputs are reconstructible in BAM space:

- **`ref_seq`** is one byte per read position: genomic base at `M`, `b'X'` at `I`/`S`, deleted bases excluded. That is exactly what §5's `reconstruct_ref` already produces.
- **`md_sequence`** (only consulted when the CIGAR has `D`) is ref-bases-at-`M` + `X`-at-`I`/`S` + deleted-bases-at-`D`, in **genome-forward order** — because extraction revcomps it for the `-` indices (`methylation.rs:220-223`, `:536-546`) and the output layer revcomps it *again* (`output.rs:446-448`, `:683-688`), and two revcomps compose to identity on `ACGTX`. Fully derivable from CIGAR + `MD_old`.

That makes §9.1's idempotence hold **by construction** instead of by a hand-written emitter happening to reproduce `rebuild_md_with_deletions` (`output.rs:228-390`) — a verbatim Perl port whose own comments label branches *"Perl dies — unreachable"* and *"a non-digit here should never happen"*. Matching that byte-for-byte on deletion records is real work for no gain.

Answering the task's question directly: **reconstruction from `(SEQ_old, CIGAR, MD_old)` is always sufficient** for every reference base anyone needs — all `M` positions (match ⇒ read base, mismatch ⇒ the letter) and all `D` positions (`^XYZ`). It is *not* sufficient for `I`/`S` positions, and nothing needs it to be: `MD` is blind to them by design.

The run-length boundary trap a fresh emitter would hit: `make_mismatch_string:195` handles `X` as `Some(&b'X') => { /* ignored */ }` — it neither increments the run nor emits a mismatch. So `3M2I4M` with everything matching yields `MD:Z:7`, not `3` then `4`. Standard, but a naive emitter that resets runs at insertions gets it wrong, and the resulting BAM would carry an `MD` inconsistent with every other Bismark BAM.

### I2. Add the cheap invariant the plan is missing: at every XM-letter position, `seq_old[i] ∈ {meth, unmeth}`.

This is a real property of both branches — CT emits a letter only when the read base is `C` or `T` (`methylation.rs:585-599`), GA only `G` or `A` (`:605-618`) — and it survives the joint revcomp. It needs no `MD`, no reference reconstruction, is O(1) per position, and it catches the same failure class as §3.3.2 (mis-zipped `XM`, inverted XG→pair table, the `read_pos_5p` bug) *without depending on `MD_old` being well-formed*. Keep §3.3.2 as well — they fail on different corruptions.

It also collapses the `NM` delta: at a letter position, `new != ref_base ⟺ XM is lowercase`, and `old != ref_base ⟺ old == unmeth`. So **`NM` needs no reference reconstruction at all** — only `MD` does. Saying that in the plan cleanly separates the two risk surfaces and makes O1 below a real decision rather than an all-or-nothing.

### I3. §3.5 / §9.1's "byte-identical BAM" is not achievable. Say "identical SAM text".

BGZF block boundaries depend on the writer, and aux `NM` is re-encoded from `Value::from(nm as i32)` (`output.rs:485`, `:715`) — a third-party BAM storing `NM` as `C`/`c`/`s` comes back as `i`. Same SAM text, different bytes. §9.1's "How" line already says "compare decompressed SAM bodies", so this is the plan contradicting itself — but §3.5's "Output is **byte-identical**" and §9.1's bolded headline are the sentences someone will implement with `cmp`. Fix the wording in both.

### I4. §5.3's `--illumina_5base` requirement and §9.1's gate are in direct tension.

The gate is "run **the converter** on a standard bisulfite Bismark BAM"; §5.3 makes the CLI refuse exactly that. Either the gate becomes a unit test on `reencode()` — weaker than advertised, since it stops exercising the driver, the tag round-trip, and the reader/writer, which is where C3 and C4 live — or the integration test passes `--illumina_5base` on bisulfite input, which is fine but makes the guard advisory.

Recommendation: keep the guard, have the integration gate pass `--illumina_5base` with a comment saying why. The guard's job is stopping an *unwitting* user; the flag is the assertion. But the plan must state which, because as written §9.1 and §5.3 cannot both be satisfied.

### I5. §3.5's CIGAR table omits `N`, `=`/`X`, and `P`.

`reconstruct_ref` and the MD emitter both have to handle `N` (skip): it consumes reference, no read bases, no `MD` run (`methylation.rs:189-191`, `output.rs:254`). `=`/`X`/`P` cannot appear in a Bismark BAM — `parse_cigar` rejects everything outside `M I D S N` (`methylation.rs:192-197`) — but the new module reads a BAM *as given*, so it must reject them explicitly. Note `cigar_to_ops` (`output.rs:751-767`) maps unknown ops to `Kind::Match` at `:762`; that silent fall-through has precedent in this codebase and should not be copied into a converter whose whole premise is positional exactness.

### I6. §9.5's oracle is under-specified and its fixture list has the priorities backwards.

"small synthetic genome + `make_mismatch_string`/`hemming_dist` as the oracle" is ambiguous about whether the oracle *is* those functions or merely mirrors them. If the implementation calls them (per I1), the test must build the reference by an **independent route** — from the synthetic genome and the alignment, not from `MD_old` — or it degenerates into asserting a function equals itself.

Fixtures to add, in priority order: soft-clip record (C2 — the common 5-Base case), insertion record, mismatch immediately adjacent to a deletion (the `rebuild_md_with_deletions` re-indexing path, `output.rs:303-334`), multi-deletion record (the `md_index_already_processed` path, `:327-331`).

### I7. §9.2's "How" does not test its "Verify".

§9.2 verifies "every XM-letter position holds `meth`/`unmeth` per `XG`, and `XM` itself is unchanged". Its method is "re-run `extract_calls` on the converted BAM and assert the calls are identical to the original's". But `extract_calls` reads **`XM`**, which the converter never touches — the plan says so itself in §7. **The assertion passes no matter what the converter does to `SEQ`**, including nothing at all, and including an inverted `meth`/`unmeth` pair.

This is the plan's only hermetic forward test of the encoding, and it is vacuous. Combined with the fact that §9.7 (the only test of the *sign*, which is the entire point of the feature) needs real data and cannot run in CI, **the sign is currently untested in CI**.

**Action:** make §9.2 assert on `SEQ` directly — for each XM-letter position, `seq_new[i] == meth` iff `XM[i]` is upper-case, per the `XG` table — and add one direction-pinning assertion: on 5-Base input, at least one position must go `T → C` (XG=CT) or `A → G` (XG=GA). A converter with the pair swapped produces perfectly self-consistent output and passes everything else in §9.

### I8. The 5-Base consensus BAM is a legitimate input and the plan should say so.

For the reporter's duplex workflow the most likely input is `five_base_consensus.bam`. Those records are emitted through `five_base_emit_record` → `single_end_sam_output` (`mod.rs:2298-2311`), so `XM`/`MD`/`NM`/`XG` are internally consistent and the converter applies cleanly — `{L}M` CIGARs, FLAG 0 or 0x10, SE-shaped. One line in §7 confirming it, because it is the first thing a user will try.

Same paragraph, one more line worth having: `--five_base_baseq` masking is harmless. `mask_low_quality` returns `N` (`mod.rs:1295-1307`) and the mask applies to the *call* sequence only while `single_end_sam_output` receives the unmasked read (`mod.rs:1354-1365`), so a masked position gets `.` in `XM` and the converter leaves the base alone. "`SEQ` and the call string were built from different sequences" is exactly the kind of thing that reads as a bug six months from now.

Note also that `run_five_base_consensus_standalone` **generates a fresh header** from the genome (`mod.rs:533`) rather than copying the input's. The new path copies the input header (correct — it has no genome), so it diverges from the precedent it is modelled on. Deliberate and right; worth one line so it doesn't get "fixed".

---

## 4. Efficiency

Nothing wrong. Streaming, O(read_len) per record, no genome — correct, and the plan is right that the consensus path's `read_genome_into_memory` (`mod.rs:531`) is several GB and multiple seconds avoided.

Two small inconsistencies:

- §4's `Reencoded { seq: Vec<u8>, md: String, … }` allocates two heap objects per record, which contradicts §6's "reusable across records if it ever matters". Either take `&mut` out-params or drop the sentence.
- I1's reuse of `make_mismatch_string` adds a `String` allocation per record plus the `strip_prefix("MD:Z:")` dance. Irrelevant next to BGZF, and the plan's own conclusion ("effectively I/O-bound; the win is parallel BGZF, not the maths") is correct.

---

## 5. Alternatives worth considering

### O1. Don't emit `MD`/`NM` at all.

`patter` reads only `SEQ` at CpG positions (§2), and **nothing in this repository reads `MD` or `NM`** — `tags::md` and `tags::nm` (`io/tags.rs:39-63`) have zero call sites outside their own unit tests. So §3.3 — which §11.1 names as the plan's biggest bug risk — buys nothing for the stated goal.

Options: (a) drop both tags, add `samtools calmd` to the closing `Note:`; (b) keep the delta-computed `NM` (trivial per I2) and drop `MD`.

Counter-argument, which the plan should state rather than leave implicit: a BAM carrying an `MD` that disagrees with its `SEQ` is worse than one carrying no `MD`, and generic consumers (IGV, `samtools stats`, variant callers) do read it. Given I1 makes exact `MD` nearly free, I'd keep it — but the plan should record that it weighed dropping it, because otherwise it is accepting its single largest risk without having priced the alternative.

### O2. On Open-1 (coordinate sort) — the plan's call is right.

Read-order output plus a loud `Note:` matches the `ubam.rs:214/239` precedent and keeps this a pure streaming pass. Worth adding: the read-order output is **mate-adjacent**, which is what a subsequent `samtools sort` needs anyway, so nothing is lost.

### O3. On Open-4 (upstream `patter` patch) — one supporting data point.

For **directional** libraries — which 5-Base is (`cli.rs:99-100`) — `patter`'s FLAG-derived OT/OB agrees with Bismark's `XG` at every index: SE `FLAG & 0x10 ⟺ XG:Z:GA` (`output.rs:413-426`), and PE index 0 → `(99,147)` with `XG:CT` vs index 3 → `(83,163)` with `XG:GA` (`output.rs:528-539`). So the XG-based encoding this plan performs and the FLAG-based orientation `patter` infers cannot disagree. That is a real (if partial) pre-check on the *sign*, obtainable without real data, and it supports pursuing both tracks.

---

## 6. Action items

### Critical

1. **C1** — Replace §9.1's fixture list. `test_files/*_from_TrimGalore.bam` are Trim Galore **uBAMs** (unaligned, no `XM`); the gate cannot run on them. Use `rust/bismark/tests/data/dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` (PE, 12 I + 11 D + dense mismatches in the first 5000 records), `.../dedup/nondir_pe_1030.bam` (non-directional PE — the only fixture that makes the "all four strand indices" claim true), and `.../filter_nonconversion/{se_default,pe_default}/`.
2. **C2** — Correct §3.3, §8.5 and §11.1: Bismark's `NM` **includes** insertions and **also counts soft-clipped bases** (`hemming_dist` `output.rs:150-157` × `X`-padding `methylation.rs:174-181`). Keep the delta and keep the "letters only at `M`" justification. Add soft-clip and insertion records to §9.5 — soft clips are the *norm* in 5-Base BAMs (`cli.rs:123-131`).
3. **C3** — §3.5's unmapped/secondary pass-through is unimplementable with `BamReader` (`read.rs:268-274` drops `FLAG & 0x4`; `from_noodles_record` also requires `XR`). Read via `noodles_bam::io::Reader` directly and write pass-throughs with the existing `BamWriter::write_raw_record` (`io/write.rs:86`).
4. **I7** — §9.2's method is vacuous (`extract_calls` reads `XM`, which the converter never modifies), so the encoding *and its sign* are currently untested in CI. Assert on `SEQ` directly and add a direction-pinning assertion (`T → C` for `XG:CT`).

### Important

5. **I1** — Reuse `output::make_mismatch_string` + `output::hemming_dist` (both `pub(crate)`, same module tree) instead of writing `emit_md`. Both inputs are reconstructible in BAM space; `md_sequence` is genome-forward because the two revcomps cancel. Makes §9.1 hold by construction and removes the `rebuild_md_with_deletions` matching problem entirely.
6. **C4** — Decide and document how `Reencoded` reaches the record: `BismarkRecord` has no `inner_mut()`. Clone `inner()` → mutate → write. (`Data::insert` preserves field order, so tag ordering is safe.)
7. **I2** — Add the `seq_old[i] ∈ {meth, unmeth}` assertion at every letter position. Independent of `MD`, and it makes `NM` need no reference reconstruction at all.
8. **I3** — "byte-identical BAM" → "identical SAM text" in §3.5 and §9.1's headline.
9. **I4** — Resolve the §5.3-vs-§9.1 conflict explicitly (recommend: keep the guard, pass `--illumina_5base` in the gate with a comment).
10. **§3.1.2** — Write the four-row derivation into the plan. `methylation_call` branches on `XR` (`methylation.rs:575`), so "`XG` alone fixes the reference base" is not visible at any single line and will read as an error.
11. **I5** — Add `N` to §3.5's CIGAR table; reject `=`/`X`/`P` explicitly rather than falling through the way `cigar_to_ops:762` does.
12. **I6** — Make §9.5's oracle build its reference independently of `MD_old`; add the mismatch-adjacent-to-deletion and multi-deletion cases.
13. **I8** — One line each in §7: the 5-Base consensus BAM is a valid input; `--five_base_baseq` masking is harmless; the input header is copied (unlike the consensus precedent, which synthesises one).

### Optional

14. **O1** — Record the "drop `MD`/`NM`" alternative and why it was rejected. Nothing in-tree reads either tag.
15. **O3** — Add the directional FLAG↔`XG` agreement to Open-4 as a hermetic partial pre-check on the sign.
16. **O2 / §6** — Drop §6's "reusable across records" sentence or change `Reencoded` to write into caller buffers.
17. Refresh the header: it names `5bf8b55` as the dev tip; `dev` is at `bfdf207` (the commit that added this plan).

---

## 7. Summary

The core insight — that `XM` already carries conventional polarity and `XG` alone determines the `(ref, meth, unmeth)` triple, so the re-encode is a positional zip needing no genome — is correct, and I verified it independently against all four SE indices and all eight PE mate/index combinations. §3.1's three properties hold. The idempotence *argument* holds, including for indels and for mismatches at cytosine positions.

What needs fixing before implementation is concentrated in validation and I/O plumbing, not in the algorithm: the primary gate names files that cannot serve as its input (C1), the forward test asserts something the converter cannot affect so the sign is untested (I7), the stated `NM` invariant is false in a way that will mislead the `NM`/`MD` oracle (C2), and the reader chosen for reuse cannot deliver the pass-through the plan promises (C3). All four are fixable without touching §3's design.

One recommendation is worth more than the rest: **reuse `make_mismatch_string`** (I1). The plan correctly identifies MD re-emission as its biggest risk and then proposes to write it fresh, when the byte-exact implementation is `pub(crate)` in a sibling module and its inputs are reconstructible.
