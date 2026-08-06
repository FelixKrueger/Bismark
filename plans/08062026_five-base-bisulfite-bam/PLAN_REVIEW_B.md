# PLAN_REVIEW_B — `--five_base_bisulfite_bam`

**Reviewer B** · independent review of `plans/08062026_five-base-bisulfite-bam/PLAN.md` · repo at `dev` `bfdf207`.

Method: every load-bearing claim was checked against `rust/bismark/src/` and against the real consumer source fetched from `nloyfer/wgbs_tools@master` (`patter.cpp`, `patter.h`, `patter_utils.cpp`, `bam2pat.py`). Nothing below is taken from the plan's own quotations.

**Verdict: the core design is correct and the central claim survives scrutiny — but two findings are Critical, and the single strongest validation in the plan cannot run as written.**

---

## 0. What checks out (stated briefly, not padded)

| Plan claim | Status |
|---|---|
| §3.1.1 `len(XM) == len(SEQ)`, `XM[i]` ↔ `SEQ[i]` in BAM space | ✅ Parity check `io/record.rs:122-129`; `XM` reversed in lockstep with the `SEQ`/`ref_seq` revcomp at `aligner/output.rs:462-467` and `:445-451` |
| §3.1.2 `XG` alone fixes the reference base (`CT`→`C`, `GA`→`G`) | ✅ Verified for all four strand indices **and** for PE. See §1.3 below for the derivation — the plan is right, but for a subtler reason than it states |
| §3.1.3 `I`/`S` are structurally `.` in `XM` | ✅ `X` padding at `methylation.rs:173-181` (SE) and `:349-352` (PE `walk_mate`); `methylation_call` (`:572`) emits letters only where the genomic base is `C`/`G`, so `X` → `.` |
| §3.4 `U`/`u` arise next to clips/insertions and must be included | ✅ Exactly `methylation.rs:641` and `:647` (`downstream == b'N' \|\| b'X'` → unknown context). The reasoning is correct |
| §2's `iter_aligned()` warning | ✅ `io/record.rs:245-249` documents the 5′ remap; the standing warning is `extractor/call.rs:185-186`. Keeping it out is right, and the "add a comment saying so" instruction is worth keeping |
| §3.5 hard clips absent | ✅ True, and for a better reason than given: minimap2 `-x sr` *does* emit `H` on supplementary alignments, but `five_base_next_primary` filters `0x100`/`0x800` before any record is emitted |
| §6 no genome load | ✅ Correct and worth the flag — the consensus path's `read_genome_into_memory` (`mod.rs:531`) is genuinely avoidable here |
| §7 dispatch short-circuits before `pipeline()` | ✅ `mod.rs:165-168` is the right shape to copy |
| Open-1: `bam2pat` needs coordinate-sorted + indexed | ✅ Stronger than the plan says: `is_bam_sorted` (`bam2pat.py:222-240`) **skips the BAM entirely** on a non-`coordinate` `@HD`, and `generate_sam_header` writes `SO:unsorted` (`output.rs:109-111`), so the copied header will carry it. The `Note:` is therefore mandatory, not advisory |
| `-f 3` / `-q 10` are harmless | ✅ All four Bismark PE flags carry `0x2`, and `five_base_emit_pe_record` refuses non-proper pairs up front (`mod.rs:1663-1668`). MAPQ is `rec1.mapq.min(rec2.mapq)` (`mod.rs:1686`) so both mates share one value — `-q 10` can never split a pair and leave `match_maker` a singleton |
| `clean_CIGAR` deletes `I`/`S` before comparison | ✅ `patter_utils.cpp:240-241`. The plan's decision to leave those bases untouched is exactly right — they never reach `is_cpg` |

---

## 1. Logic review

### 1.1 🔴 CRITICAL — `--five_base_baseq` leaves 5-Base-polarity bases in `SEQ`, producing silently **inverted** `patter` calls

This is the one failure mode that reproduces the exact bug the plan exists to eliminate.

`mask_low_quality` (`aligner/mod.rs:1295-1307`) masks low-quality bases to `N` **in the call sequence only**. The record is then built from the *unmasked* read:

```rust
// mod.rs:1352-1360
// Base-quality masking applies to the CALL ONLY; the BAM SEQ keeps the original read.
let call_seq = mask_low_quality(seq_uc, qual_bytes, baseq, ...);
let methcall = methylation_call(&call_seq, ..., true, counters);
let record = single_end_sam_output(identifier, seq_uc, qual_bytes, ...);
//                                              ^^^^^^^ unmasked
```

`cli.rs:134-135` states it as a feature: *"The BAM SEQ is unchanged (only the methylation call is masked)."* The existing test proves the shape (`mod.rs:6717-6731`): genome `AACGAATTTT`, read `TGAA` at POS 3, qual `!III`, `--five_base_baseq 20` → `XM == "...."` while `SEQ` stays `TGAA`.

Now run the plan's §3.2 over that record. `XM[0] == '.'` → **untouched** → `SEQ[0]` stays `T`. Hand it to `patter` (OT, `shift == 0`):

- `is_cpg(seq, 0, OT)` (`patter.cpp:98-99`): `seq[0] == 'T'` ✓ and `seq[1] == 'G'` ✓ → **true**.
- `s == ro.unmeth_seq_chr` (`'T'`) → `cur_status = UNMETH` (`patter.cpp:153-157`).

The truth under 5-Base chemistry is `T` = **methylated**. The call is inverted, silently, with no counter and no warning.

I enumerated the complete set of positions where `XM` is `.` yet `patter` can still score a CpG, to bound the leak:

| `XM == '.'` because | Reaches `patter`? |
|---|---|
| **base-quality masked** (`--five_base_baseq > 0`) | **YES — the leak.** `SEQ` ∈ {C,T}/{G,A}, `is_cpg` passes, call inverted |
| soft clip / insertion (`g == 'X'`) | No — `clean_CIGAR` (`patter_utils.cpp:240-241`) deletes those bases |
| genuine mismatch, read base ∉ {C,T} (SNP/error) | No — `is_cpg` requires `seq[j]` ∈ {C,T} (OT) / {G,A} (OB) |
| deletion (`D`) | No SEQ position exists; `clean_CIGAR` substitutes `N` |
| chromosome-edge guard | Record never written (`mod.rs:1347-1350`) |

So masking is the *only* leak — but it is a real one, it is opt-in for exactly the quality-conscious user who would also run `--five_base_deconvolution`, and every masked cytosine in a CpG context becomes a wrong-signed call.

**Fix — pick one:**

1. **Detect and refuse.** Bismark writes the verbatim argv into `@PG CL:` (`output.rs:118-123`). The converter reads the input header anyway; parse the `CL:` for `--five_base_baseq <n>` with `n > 0` and refuse with a message naming the flag. Cheap, precise, no per-record cost.
2. **Mask the leak.** At an `XM == '.'` position whose `SEQ[i]` ∈ {`meth`, `unmeth`}, write `N`. `is_cpg` then fails → `UNKNOWN`, which is the *correct* verdict for a no-call. Costs an `NM`/`MD` update at those positions (`N` vs ref `C` is a mismatch) — mechanical if I2 below is adopted.
3. **At minimum**, count those positions and fail loud if the count is non-zero, rather than finishing quietly.

Option 1 is the smallest correct change; option 2 is the most robust and composes with the rest of the design.

### 1.2 🔴 CRITICAL — the fixtures named for the §9.1 idempotence gate are **unaligned uBAMs**

§9.1 is the plan's own "strongest check available" and specifies:

> **How:** existing bisulfite test BAMs (SE and PE, incl. `test_files/*_from_TrimGalore.bam`)

Those two files are `trim_galore --output-format ubam` outputs. Verified:

```
$ samtools view test_files/BS-seq_10K_paired_val_from_TrimGalore.bam | head -1
SRR24827378.1  77  *  0  255  *  *  0  0  AATT...  ????...
```

FLAG 77/141/4, RNAME `*`, and **no** `XM`/`XR`/`XG`/`MD`/`NM`. The converter fails loud on record 1 (or drops every record as unmapped — see I3). The load-bearing gate, as written, cannot run.

Aligned bisulfite fixtures that *do* exist and are suitable:

- `rust/bismark/tests/data/dedup/synth_bclconvert_10k_R1_val_1_bismark_bt2_pe.bam` — real `bismark_bt2` PE output
- `rust/bismark/test_files/tiny_pe_bismark.bam`, `rust/bismark/reads_bismark_bt2.bam`
- `rust/bismark/tests/data/extractor/nondir_pe_1030.bam` — **non-directional PE, all four `XG`/FLAG combinations**; the ideal fixture for I4
- `rust/bismark/tests/data/filter_nonconversion/se_unmapped/in.bam` — contains unmapped records, for the §3.5 pass-through row

**But the gate is still weaker than the plan claims.** Every one of those fixtures is pure-`M`:

```
$ samtools view .../synth_bclconvert_10k_..._pe.bam | awk '{print $6}' | sort | uniq -c
4364 51M   4347 48M   3391 49M   574 47M   ...   (nothing but nM)
```

So §9.1 exercises **zero** deletion, insertion or soft-clip MD paths. The claim *"Any error in §3.1.2 or §3.3 shows up as a diff"* is overclaimed: the deletion branch — `rebuild_md_with_deletions` (`output.rs:228`, ~170 lines of Perl-faithful re-indexing) — escapes the gate entirely, and the plan itself names MD re-emission as its likeliest bug (§11.1).

That gap is not academic. **Soft clips are routine in real 5-Base BAMs**: the default 5-Base engine is minimap2 `-x sr` (`mod.rs:1173-1180`), and `--five_base_umi_len` *depends* on the aligner soft-clipping the UMI prefix (`cli.rs:125-128`). With `--five_base_umi_len 8`, essentially every read carries an `8S` prefix. The dominant real-world CIGAR shape would have synthetic-only coverage.

**Fix:** retarget §9.1 to the aligned fixtures above (add `nondir_pe_1030.bam` explicitly — it is the only four-strand fixture in the repo), and commit one aligned fixture carrying at least one `D`, one `I` and a leading `S` **with real `XM`/`MD`/`NM`**, generated by the live Perl oracle so the MD is authentic. `tests/data/bam2nuc/all_indel.bam` has the CIGARs (`3M1I4M`, `4M2D4M`) but **no aux tags at all**, so it is not reusable as-is.

### 1.3 The `XG`-vs-FLAG question: the equivalence holds — but the plan never noticed it exists

`patter` infers strand from the SAM FLAG, never from `XG` (`patter_utils.cpp:163-168`):

```cpp
bool is_bottom(int samflag, bool is_paired_end) {
    if (is_paired_end) { return (((samflag & 0x53) == 83) || ((samflag & 0xA3) == 163)); };
    return ((samflag & 0x10) == 16);
}
```

The plan's encoding is driven entirely by `XG`. It never mentions the FLAG. I checked whether they can disagree:

**SE** (`output.rs:413-419`) — the flag is a *pure function of `genome_conversion`*:

| (strand, XR, XG) | strand name | FLAG | `is_bottom` | XG says |
|---|---|---|---|---|
| `(+, Ct, Ct)` | OT | 0 | top | CT → top ✓ |
| `(-, Ga, Ct)` | CTOT | 0 | top | CT → top ✓ |
| `(+, Ga, Ga)` | CTOB | 16 | bottom | GA → bottom ✓ |
| `(-, Ct, Ga)` | OB | 16 | bottom | GA → bottom ✓ |

So `FLAG & 0x10 ⟺ XG == "GA"`. They cannot disagree.

**PE** (`output.rs:526-537`) — index → `(flag_1, flag_2)`: `0→(99,147)`, `1→(163,83)`, `2→(147,99)`, `3→(83,163)`. `is_bottom` is false for 99 and 147, true for 83 and 163. Indices 0/2 carry `XG:Z:CT`, indices 1/3 carry `XG:Z:GA`. **All four agree** — and they agree *because of* the index-1/2 R1↔R2 first/second-in-pair swap (the `#1030` quirk, commented at `output.rs:526-527`). Without that swap a non-directional CTOT read would be read as bottom, and every unmethylated CpG on it would silently become `UNKNOWN` while every methylated one became `METH` — a heavy methylation bias.

Confirmed empirically on the only four-strand fixture in the repo:

```
$ samtools view rust/bismark/tests/data/extractor/nondir_pe_1030.bam | ...
   4  99  XG:Z:CT  XR:Z:CT      6  83  XG:Z:GA  XR:Z:CT
   4 147  XG:Z:CT  XR:Z:GA      6 163  XG:Z:GA  XR:Z:GA
```

Also checked the two 5-Base-specific shapes:

- **5-Base PE** only ever selects index 0 or 3 (`let index = if rec1.flag & 0x10 != 0 { 3 } else { 0 }`, `mod.rs:1669`) → flags `{99,147}` / `{83,163}`, `XG` CT/GA. Agrees.
- **5-Base consensus** BAM goes through `five_base_emit_record` → `single_end_sam_output` (`mod.rs:2298-2310`), so its records carry SE-style FLAG 0/16 and `flag & 1 == 0`. `bam2pat`'s `is_pair_end` reads only the first record's `flag & 1` (`bam2pat.py:262-267`), so it treats the consensus BAM as single-end — which is correct, one record per family. And the consensus records **do** carry `NM`/`MD`/`XM`/`XR`/`XG`, so the plan's fail-loud-on-missing-`MD` rule will not reject the file a 5-Base user most wants to convert. Worth stating in the docs, since it is not obvious.

**So the plan's conclusion is safe.** But the equivalence is load-bearing, non-obvious, and **invisible to §9.1** (the output is byte-identical whether or not you got this right). Action: state it in the plan and add a unit test asserting `XG == "CT" ⟺ FLAG ∉ {16, 83, 163}` over the `nondir_pe_1030.bam` fixture, so a future change to the flag table breaks a test rather than the science.

### 1.4 §2 and §3.5 contradict each other on unmapped records

§2 says read via `crate::io::BamReader::from_path_without_sort_check`. §3.5 says unmapped records are *"passed through **unchanged**, counted."*

Those are incompatible. `io/read.rs:7-9`:

> **Silently filters unmapped reads** (SAM FLAG & 0x4). […] access to unmapped reads should use the underlying noodles reader

`filter_unmapped_then_classify` (`read.rs:631-645`) returns `None` for `flags & 0x4`. Follow §2 and the records vanish before the loop sees them; the §3.5 counter can never fire, and the "true sibling of the input" rationale is false.

Two further consequences of routing through `BismarkRecord`:

- `from_noodles_record` also requires **`XR`** and validates the `XR`/`XG` *combination* (`record.rs:112-116`). §3.5's tag matrix and §9.6's fail-loud list both omit `XR`.
- It gives the `len(XM) == len(SEQ)` check for free (`record.rs:122-129`) — so §3.5's row there is already satisfied.

**Resolution:** either read raw noodles and write with `BamWriter::write_raw_record` (`io/write.rs:87`, added for precisely this — non-Bismark-shaped records), or drop the pass-through claim. In practice Bismark writes unmapped reads to FASTQ, not into the BAM, so a Bismark BAM has none; saying *that* is more honest than the sibling argument. Pick one and make §2 and §3.5 agree.

### 1.5 §5.7's chromosome-naming rationale points at the wrong failure mode

The plan says a naming mismatch surfaces as `locus2CpGIndex` throwing `std::logic_error`, *"loudly, but only at the end of a long run."* Both halves are wrong.

`locus2CpGIndex` (`patter.cpp:81-93`) is reached only when `conv[start_locus + i]` is true (`patter.cpp:144-175`), and `conv`/`dict` are built from the same `tabix` region in `load_genome_ref` (`patter.cpp:28-41`). So the locus is always present in `dict` and the throw is **unreachable** on this path. The upper bound is guarded at `patter.cpp:141` (reads past the last CpG are skipped, `continue`), and `conv[]` cannot be over-indexed.

What actually happens: `set_regions` (`bam2pat.py:49-80`) intersects `samtools idxstats` names against the wgbstools genome's, and on an **empty** intersection raises immediately, before a single read is parsed:

> `Failed retrieving valid chromosome names. Perhaps you are using a wrong genome reference. Try running: wgbstools set_default_ref -ls`

Loud and *early* — the opposite of the plan's claim. The genuinely silent hazard is a **partial** intersection: extra Bismark contigs (scaffolds, alt/decoy, `chrM` vs `chrMT`) are dropped without comment and the run "succeeds" with missing data.

**Other reference hazards the plan should name** (the team lead asked whether naming is the only one — it is not):

1. `wgbstools`' CpG dictionary is built from **its own** hg38 FASTA. `patter` never compares against a reference — it reads the read. So a patch/build skew between the wgbstools genome and the Bismark genome is silent, surfacing only as extra `UNKNOWN`s via `is_cpg`.
2. `is_cpg` needs the **neighbouring** context base in `SEQ` (`seq[j+1] == 'G'` for OT, `seq[j-1] == 'C'` for OB — `patter.cpp:96-103`). That base has `XM == '.'` and is therefore untouched by the plan. It is correct by chemistry — a genomic `G` on a top-strand read, and a genomic `C` on a forward-represented bottom-strand read, are unconverted in both bisulfite and 5-Base — so **the plan does satisfy `is_cpg` on both strands**. Worth stating explicitly in the plan, because it is the one place where an untouched base is load-bearing.
3. `--clip` is applied to the **cleaned** (reference-space) sequence, after `clean_CIGAR` has removed soft clips and inserted `N` for deletions (`patter.cpp:169`). It is not a read-cycle clip. Since the reporter uses `--clip` (§8.8), one line in the docs note is warranted.

---

## 2. Assumptions

**Correct as stated:** 1, 2, 3, 6 (see §0). Assumption 4 (`NM`/`MD` on every mapped record) holds — `single_end_sam_output`/`paired_end_sam_output` insert both unconditionally (`output.rs:485-489`).

**Assumption 5 is wrong.**

> *"Bismark's `NM` excludes insertions — preserved via delta arithmetic (§3.3)."*

`nm = hemming_dist(&actual_seq, &ref_seq) + ext.indels` (`output.rs:454`). `ref_seq` carries `b'X'` at every `I` **and** `S` position (`methylation.rs:173-181`, `:349-352`), and `hemming_dist` does **not** skip `X`. Its own doc comment says so (`output.rs:141-144`):

> *positions past `ref_seq`'s end count as differences; `X` padding bases mismatch — **intentionally counted**, then `indels` is added by the caller*

So Bismark's `NM` = real mismatches + (# inserted bases) + (**# soft-clipped bases**) + (# deleted bases). Insertions **are** counted — via `hemming_dist`, not via `indels`. And soft clips are counted too, which is the genuinely non-standard part and a large term for `--five_base_umi_len 8`.

The plan's *arithmetic* survives (the delta only touches `M` positions with a real ref base, so the `X` terms cancel), but the rationale is inverted, and §9.5's instruction to assert *"including Bismark's insertions-excluded `NM` quirk"* would send the implementer to write an oracle that contradicts the code. Fix the prose in §3.3 and §8.5, and restate §9.5's expectation as "insertions *and soft clips* counted as mismatches".

**Unstated assumption worth promoting (and it is a gift).** `methylation_call` emits a letter in exactly two branches (`methylation.rs:583-600` CT, `:604-620` GA): `base == g` where `g ∈ {C,G}`, or `g == 'C' && base == 'T'` / `g == 'G' && base == 'A'`. Therefore:

> **At every `XM`-letter position, `SEQ[i]` ∈ {`meth`, `unmeth`}.**

No `MD`, no genome, no reconstruction. That is a strictly cheaper and strictly earlier check than §3.3.2's ref-base cross-check, and it catches the same bug class (wrong `XG` mapping, off-by-one zip, missed reversal). It holds even with base-quality masking, since masked positions carry `.`.

Its corollary is more valuable still. With `five_base = true`, `push_ct_context(..., !five_base)` on a matched `C` gives **lower**case and `push_ct_context(..., five_base)` on a read `T` gives **UPPER**case. Working that through all four strand indices (including the revcomp for `-`):

- **5-Base input:** *every* letter position flips (`Z` ⇒ SEQ was `T` ⇒ becomes `C`; `z` ⇒ SEQ was `C` ⇒ becomes `T`).
- **Bisulfite input:** *no* letter position flips.

So `flipped / (# XM letters)` is exactly `1.000` for 5-Base and `0.000` for bisulfite, per record. That is a hard runtime discriminator on the **file**, where the plan's only guard (`--illumina_5base`) is a statement about the user's **intent**. It catches "wrong BAM handed to the converter" and any mixed/half-converted file, and it is free. See O6 for the reporting change.

---

## 3. Efficiency

The plan's analysis is right and I have nothing to add on complexity: streaming, `O(read_len)`, two `Vec<u8>` per record, no genome, I/O-bound on BGZF. Two small notes:

- Routing through `BismarkRecord` costs only the tag lookups + the length check; the expensive `iter_aligned()` CIGAR walk is not on this path (`record.rs:292-310`). Good.
- If I3 is resolved toward the raw noodles reader, the free `len(XM) == len(SEQ)` check is lost and must be re-added explicitly. §3.5 already lists it; just don't let it fall through the crack.

**`NM`/`MD` churn is total, not marginal** — worth knowing for capacity and for reading test output. Under bisulfite convention nearly every unmethylated cytosine is an `MD` mismatch; under 5-Base only 5mC positions are. So the converter rewrites `MD` and raises `NM` on essentially *every* record. Downstream this is harmless (nothing in the `bam2pat` path filters on `NM`; MAPQ is copied) — but it means any `MD`-dialect error will be pervasive rather than rare, which strengthens the case for I2 below.

---

## 4. Alternatives

### A1 (recommended) — don't re-implement `MD`; **reuse Bismark's own generator**, and use it as a per-record proof

Both `make_mismatch_string` (`output.rs:180-215`) and `hemming_dist` (`output.rs:150-157`) are `pub(crate)`. A new `aligner/five_base_bisulfite.rs` sits in the same crate and can call them directly.

```
1. reconstruct ref_seq from (seq_old, CIGAR, MD_old):  X at I/S, real bases at M, ^ bases at D
2. PROVE it:   hemming_dist(seq_old, ref_seq) + Σ(D lengths)              == NM_old
               make_mismatch_string(seq_old, ref_seq, cigar, md_seq)      == "MD:Z:" + MD_old
3. emit:       NM_new = hemming_dist(seq_new, ref_seq) + Σ(D lengths)
               MD_new = make_mismatch_string(seq_new, ref_seq, cigar, md_seq)   (strip "MD:Z:")
```

Why this is better than §3.3 as written:

- The output `MD` is in Bismark's exact dialect **by construction**, not by test. `rebuild_md_with_deletions` (`output.rs:228`) is a Perl-faithful re-indexing routine; an independent re-emit has to match it byte-for-byte on the deletion path, and §9.1 cannot reach that path at all (see 1.2).
- Step 2 makes the delta-vs-recompute debate moot and settles the assumption-5 confusion by construction — you are using the very function that defines the quirk.
- Step 2 is a **per-record proof** that the reference reconstruction is right, on 5-Base data, including deletions — strictly stronger than §3.3.2's cross-check (which only pins the ref base at letter positions) and it covers exactly what the idempotence gate cannot.
- `md_seq` is reconstructable: it is genomic bases over `M`+`D` runs with `X` at `I`/`S` (`methylation.rs:341-360`), and `MD_old`'s `^XYZ` supplies the deleted bases. It is only needed when the CIGAR contains `D`.

This eliminates the plan's own #1 remaining risk (§11.1). I'd make it the implementation, and keep §9.5's from-genome oracle as an independent second opinion.

### A2 — skip the BAM entirely and emit `.pat` directly

Bismark already holds everything `patter` computes (`XM`, reference position, CpG context). Writing wgbs_tools `.pat` directly would skip the sort/index step, the `MD`/`NM` arithmetic, and the misleading-`SEQ` hazard (§11.4) outright.

Cost: reimplementing `locus2CpGIndex` against wgbstools' `tabix`-indexed CpG dictionary, plus `merge_PE`, `strip_pat`, `clean_CIGAR`, `--clip` and the MAPQ/flag filters — and then re-validating all of it against `patter` for byte-identity. It also forfeits everything `bam2pat` gives you for free (`--top_strand`, `--mbias`, `--blacklist`/`--whitelist`, `--min_cpg`, and any future `patter` behaviour).

I think the plan's BAM route is the right call — but the plan should say *why* in one sentence, because "produce a file `patter` reads correctly" invites the reader to ask why we're not producing `patter`'s output.

### A3 — Open-4's upstream `patter` patch is smaller than it sounds

The plan's "yes, in parallel" is right. One concrete note for whoever writes it: flipping `ReadOrient`'s `ref_chr`/`unmeth_seq_chr` (`patter.h:59-60`) is **not sufficient** — `is_cpg` (`patter.cpp:96-103`) hardcodes the `C`/`T` and `G`/`A` alternatives independently:

```cpp
return (j < seq.size() - 1) && ((seq[j] == 'C') || (seq[j] == 'T')) && (seq[j + 1] == 'G');
```

Under 5-Base polarity those two literals still happen to be the right *set* (`{C,T}`), so `is_cpg` needs no change for 5-Base specifically — but a patch that only edits the two structs and claims generality would be wrong for any future chemistry with a different alphabet. Worth a line in the upstream PR so the maintainer sees it was considered.

---

## 5. Validation sufficiency

| §  | Assessment |
|---|---|
| 9.1 | **Cannot run as written** (1.2). Retarget the fixtures; add `nondir_pe_1030.bam`; add an indel/soft-clip fixture. Downgrade the "any error shows up as a diff" claim — with pure-`M` fixtures it is not true |
| 9.2 | Sound, and the `XM`-preserving design is what makes it possible. Add: assert `flipped == (# XM letters)` (see §2) |
| 9.3 | Good and correctly motivated. The "construct it so a `read_pos_5p`-indexed write produces a *detectably different* string" instruction is the right level of paranoia |
| 9.4 | Right cases, but synthetic-only — and soft clips are the *common* real shape for 5-Base (1.2). Pair it with a real fixture |
| 9.5 | Right idea, wrong expectation ("insertions-excluded" — see §2). Under A1 this becomes a much stronger per-record self-check |
| 9.6 | Add `XR` to the missing-tag matrix (1.4). Asserting on message content, not `is_err()`, is the right standard |
| 9.7 | Correctly scoped, and "the real job is proving the sign is right" is the right framing. Add a **`--five_base_baseq 0` vs `> 0`** arm — under C1 the masked run is where the sign is still wrong, and a whole-genome correlation would dilute it below notice |

**The largest gap:** nothing in §9 catches C1 or I4, and both are silent-wrong-answer modes. Add (a) a unit test that a masked (`XM == '.'`, `SEQ ∈ {meth,unmeth}`) position is either rejected or `N`-masked, and (b) the `XG` ⟺ FLAG assertion over `nondir_pe_1030.bam`.

---

## 6. Action items

### Critical

1. **C1 — handle `--five_base_baseq` masking.** `XM == '.'` positions whose `SEQ` base is in {`meth`, `unmeth`} carry raw 5-Base polarity into a file that claims bisulfite convention, and `patter` scores them **inverted**. Refuse the input when the `@PG CL:` shows a non-zero `--five_base_baseq`, or mask those positions to `N`. Add the unit test. (§1.1)
2. **C2 — fix the §9.1 fixtures.** `test_files/*_from_TrimGalore.bam` are unaligned uBAMs with no `XM`/`XG`/`MD`/`NM`. Retarget to the aligned fixtures listed in §1.2, add `nondir_pe_1030.bam`, and commit one aligned fixture with a `D`, an `I` and a leading `S` carrying real `MD`/`NM` — otherwise the deletion `MD` path and the routine soft-clip case are never gated. Drop the "any error shows up as a diff" claim. (§1.2)

### Important

3. **I1 — correct assumption 5.** Bismark's `NM` counts insertions **and soft clips** as mismatches (`output.rs:141-144`, `:454`); it is `indels` that is deletions-only. Fix §3.3's warning box, §8.5 and §9.5's expectation. The delta arithmetic itself is fine. (§2)
4. **I2 — implement `MD`/`NM` by reusing `make_mismatch_string` + `hemming_dist`, with the `seq_old` round-trip as a per-record proof.** Removes the plan's own #1 risk and makes byte-identity structural. (§4/A1)
5. **I3 — resolve the §2/§3.5 contradiction on unmapped records.** `BamReader::records()` silently drops `FLAG & 0x4` (`read.rs:7-9`, `:631-645`), so §3.5's pass-through is unimplementable as specified. Use raw noodles + `write_raw_record`, or drop the claim. Add `XR` to the required-tag matrix. (§1.4)
6. **I4 — document and test the `XG` ⟺ FLAG equivalence.** `patter` uses the FLAG, the plan uses `XG`; they agree for every Bismark record, PE agreement depending on the `#1030` first/second-in-pair swap. Load-bearing, non-obvious, and invisible to §9.1. Assert it. (§1.3)
7. **I5 — assert `SEQ[i] ∈ {meth, unmeth}` at every letter position, and use the flip rate as the 5-Base/bisulfite discriminator.** Free, stronger than the `MD` cross-check, and the only check that validates the **file** rather than the user's intent. (§2)
8. **I6 — rewrite §5.7's chromosome note.** `locus2CpGIndex` cannot throw on this path; the real guard is `set_regions` failing early and loudly (`bam2pat.py:71-78`). The silent hazard is a *partial* name intersection. Add the wgbstools-vs-Bismark reference-skew and `--clip`-is-reference-space notes. (§1.5)

### Optional

9. **O1 — dispatch order.** §5.2 places the new guard "beside `:167`", but `mod.rs:167` already `return`s for `five_base_consensus_from_bam`. Put the mutual-exclusion check *before* both dispatches or it never fires.
10. **O2 — assert the output carries no `MM:Z:`.** `detect_nanopore` (`bam2pat.py:243-259`, `:279-282`) silently switches to `--nanopore` mode — reading `MM`/`ML` instead of `SEQ`, with `-q 0 -F 3844` — if the header has `\tPL:ONT` or any of the first 200 records has `\tMM:Z:`/`\tMm:Z:`. Bismark writes neither (no `MM`/`ML` tag anywhere in `rust/bismark/src`), and `\tXM:Z:` does not match `\tMM:Z:`, so this is safe today. But the converter copies aux data verbatim, so one assertion is cheap insurance against a future tag-preserving input path silently defeating the entire feature.
11. **O3 — note that `--ds_test` still works.** `is_pass_ds_test` (`patter_utils.cpp:350-398`) reads `XM:Z:` directly, so `bam2pat --ds_test` behaves correctly on the converted BAM. A pleasant consequence of not touching `XM`; worth one docs line.
12. **O4 — say that `MD`/`NM` change on essentially every record**, so reviewers reading a diff don't mistake pervasive churn for a bug. (§3)
13. **O5 — Open-3 is a fair deferral.** `five_base_deconv.rs` writes only a report and never touches `XM`, so there is no existing masking to reuse; a real `C>T` het at a CpG already reads as methylated in Bismark's own `XM` and cytosine report. The converter propagates a pre-existing limitation rather than adding one — though it additionally erases the variant from `SEQ`. At UXM_deconv's marker-block resolution this is noise. Keep §11.4's "never the primary BAM" as the operative mitigation.
14. **O6 — report the flip *rate*, not the per-record mean.** Given I5, `flipped / (# letters)` must be exactly `1.000` (5-Base) or `0.000` (bisulfite). Anything between is a bug or a mixed file. Far more diagnostic than "flipped-per-record mean", and it costs one extra counter.

---

## 7. Summary

The plan's central claim — that `XM` + `XG` alone are sufficient to re-encode `SEQ` into bisulfite convention, with no genome and no re-alignment — **is correct**, and I verified it independently for all four strand indices in both SE and PE, including the revcomp bookkeeping. The `XG`-driven encoding also happens to agree with `patter`'s FLAG-driven strand inference on every Bismark record, so the design works on the real consumer. The scope calls I was asked to assess are right: converting all eight letters including `U`/`u` is correct and free; rejecting hard clips is correct (and unreachable, since supplementaries are filtered upstream); read-order output with a `Note:` is acceptable and in fact mandatory, because `bam2pat` refuses an unsorted BAM outright.

Two things must change before implementation. `--five_base_baseq` masking leaves raw 5-Base bases in `SEQ` at `XM == '.'` positions, and `patter` reads them **inverted** — the precise bug this feature exists to fix, surviving in the output. And the fixtures named for the idempotence gate are unaligned uBAMs, so the plan's strongest validation cannot run; the substitutes that do exist are all pure-`M`, leaving the deletion `MD` path and the routine soft-clip case ungated. Beyond that, `NM`'s treatment of insertions is stated backwards, §2 and §3.5 disagree about unmapped records, and the `MD` work should reuse Bismark's own generator rather than re-deriving its dialect.
