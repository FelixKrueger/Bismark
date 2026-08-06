# PLAN — `--five_base_bisulfite_bam`: re-encode 5-Base `XM` calls into bisulfite-convention `SEQ`

**Issue:** [#1095](https://github.com/FelixKrueger/Bismark/issues/1095) · **Origin:** [#787](https://github.com/FelixKrueger/Bismark/issues/787) (@Danielsm8) · **Branch:** to be cut from `dev` (`5bf8b55`)

**Status:** awaiting manual review. Not reviewed by agents, not implemented.

---

## 1. Goal

Add an opt-in aligner flag `--five_base_bisulfite_bam <BAM>` (repeatable) that reads existing Bismark **5-Base** BAM(s) and writes, per input, a *second* BAM whose `SEQ` has been re-encoded into **bisulfite convention** from the `XM` tag Bismark already produced — with `NM` and `MD` updated exactly.

The outcome is a drop-in input for `wgbs_tools bam2pat` → `UXM_deconv` (cell-of-origin deconvolution), which today produces **systematically inverted** calls on 5-Base data. No re-alignment; the byte-frozen aligner is untouched.

**Non-goal:** this file is never the primary BAM. Its `SEQ` no longer matches the sequencer.

---

## 2. Context

### Why it is needed

Bismark's 5-Base methylation is *already* conventional — the polarity inversion happens when `XM` is written (`aligner/methylation.rs:560-614`), so extractor/coverage/bedGraph/cytosine reports are all correct. But `patter` (the C++ engine behind `bam2pat`) **never reads `XM` for its calls**: it takes the read character in `SEQ` at CpG positions and compares it against two per-strand constants (`patter.h:59-60`):

```cpp
ReadOrient OT{'C', 'T', 0, 0};   // {ref_chr = methylated, unmeth_seq_chr = unmethylated, shift, mbias_ind}
ReadOrient OB{'G', 'A', 1, 1};
```

5-Base `SEQ` keeps the original read (`aligner/mod.rs:1352-1353`), and 5-Base chemistry is bisulfite's inverse (5mC→T; unmethylated C stays C). Hence inverted output.

### Placement

| File | Change |
|---|---|
| `rust/bismark/src/aligner/cli.rs` | New field after `five_base_consensus_from_bam` (~`:183`) |
| `rust/bismark/src/aligner/mod.rs` | Early-dispatch guard beside `:167`; new `run_five_base_bisulfite_standalone()` modelled on `run_five_base_consensus_standalone()` (`:515`) |
| **`rust/bismark/src/aligner/five_base_bisulfite.rs`** *(new)* | The pure per-record re-encode + MD/NM maths. No I/O, so fully unit-testable |
| `rust/bismark/tests/aligner_five_base_bisulfite.rs` *(new)* | Integration + the idempotence gate (§9.1) |
| `docs/src/content/docs/rust/illumina-5-base.md` | New subsection under "Advanced modes" |
| `CHANGELOG.md`, `rust/README.md` | Entry + Milestones line |

### Existing code to reuse

- **Read:** `crate::io::BamReader::from_path_without_sort_check` — precedent `mod.rs:536`.
- **Write:** the `write_record(&mut writer, &record)` pattern — precedent `mod.rs:2309`.
- **Tags:** `io::tags::{xm, xg, md, nm}` all exist (`io/tags.rs:21-63`). `xm`/`xg` return `Err(MissingTag)`; `md`/`nm` return `Ok(None)`.
- **Dispatch:** the `--five_base_consensus_from_bam` short-circuit is the exact shape to copy.

### Deliberately NOT reused

**`BismarkRecord::iter_aligned()` must not be used here.** It returns **5'-oriented** positions (`io/record.rs:245-249`), so writing into `SEQ` at `read_pos_5p` silently reverses every OB read's edits — see the standing warning at `extractor/call.rs:183`. §3 needs no reference positions at all, so there is no reason to reach for it. **The implementer should add a comment saying so**, or a later refactor will "simplify" into the bug.

---

## 3. Behavior

### 3.1 The three properties this rests on (all verified in-tree)

1. **`len(XM) == len(SEQ)`** — enforced by the parity check in `io::record::from_noodles_record` and relied on at `io/record.rs:301`. `XM` is reversed in lockstep with `SEQ` for `-`-strand records (`aligner/output.rs:463-467`). So `XM[i]` ↔ `SEQ[i]` positionally **in BAM space**.
2. **`XG` alone fixes the reference base.** Across all four strand indices (`aligner/methylation.rs:131-135`) `methylation_call` emits a letter only where the genomic base is `C` (CT branch) or `G` (GA branch), and `single_end_sam_output` revcomps `SEQ` and `ref_seq` **together** for `-` strand. Therefore:

   | `XG:Z:` | reference base at every XM-letter position | methylated | unmethylated |
   |---|---|---|---|
   | `CT` | `C` | `C` | `T` |
   | `GA` | `G` | `G` | `A` |

   These are exactly `patter`'s `ref_chr` / `unmeth_seq_chr`.
3. **Soft clips and insertions are structurally `.`** — `I`/`S` pad the genomic window with `b'X'` without advancing `pos` (`aligner/methylation.rs:174-181` SE, `:349-352` PE `walk_mate`). `X` matches no read base (reads are upper-cased `ACGTN`), so `methylation_call` falls through to `.`. The window is never mis-framed, and a positional zip **provably cannot touch a clipped or inserted base**.

### 3.2 Per-record algorithm

```
meth, unmeth, ref_base  =  XG == "CT" ? ('C','T','C') : ('G','A','G')

seq_new = seq.clone()
flipped = 0
for i in 0 .. seq.len():                      # len(xm) == len(seq), asserted
    match xm[i]:
        b'Z' | b'X' | b'H' | b'U'  =>  seq_new[i] = meth
        b'z' | b'x' | b'h' | b'u'  =>  seq_new[i] = unmeth
        b'.'                       =>  ()                     # untouched
        other                      =>  return Err(InvalidXmByte)
    if seq_new[i] != seq[i] { flipped += 1 }
```

All eight letters are rewritten, **including `U`/`u`** — see §3.4.

### 3.3 `NM` and `MD` — recomputed exactly, no genome

1. **Reconstruct the reference** over the aligned span from `(seq_old, CIGAR, MD_old)` using the standard `MD` grammar: integer = that many matches (ref == read), letter = mismatch with the ref base given, `^XYZ` = deleted ref bases.
2. **Cross-check (free correctness gate):** at every XM-letter position the reconstructed ref base **must** equal `ref_base` from §3.1.2. Mismatch ⇒ hard error naming the QNAME and offset. This validates the plan's central claim on **every record at runtime** for O(1) per position. Do not downgrade it to a `debug_assert`.
3. **`MD_new`** = re-emit from `(ref, seq_new, CIGAR)`.
4. **`NM_new` = `NM_old` + Σ over rewritten positions of** `(new != ref_base) as i64 - (old != ref_base) as i64`.

> ⚠️ **Compute `NM` as a delta — do not recompute from scratch.** Bismark's `NM` is `hemming_dist + indels` where `indels` counts **deletions only** (`aligner/output.rs:453`, `methylation.rs:187` *"D only"*) — a Perl-faithful quirk that excludes insertions. A from-scratch recompute would silently "fix" it and diverge from every other Bismark BAM. The delta is safe because XM letters only ever occur at `M` positions (§3.1.3).

### 3.4 Why `U`/`u` must be included

`push_ct_context` maps an `X` in the *context* slots (i+1, i+2) to `U`/`u` (`methylation.rs:641-649`), so a cytosine immediately before a soft clip or insertion is classified unknown-context rather than CpG/CHG/CHH. Excluding `U`/`u` would leave exactly those cytosines in 5-Base polarity while their neighbours flip — a silent, half-converted file. `patter` ignores non-CpG anyway, so including them is free.

### 3.5 Edge cases

| Case | Handling |
|---|---|
| Unmapped (`0x4`) | Passed through **unchanged**, counted. `bam2pat` drops them via `-F 1796` anyway; copying faithfully keeps the output a true sibling of the input |
| `XM` or `XG` absent | **Fail loud** — not a Bismark BAM. Name the file and QNAME |
| `MD` absent | **Fail loud.** Bismark always writes it (`output.rs:485-487`); without it `MD_new` is unknowable |
| `NM` absent | Treat as "cannot update" ⇒ fail loud, same reasoning |
| `len(XM) != len(SEQ)` | **Fail loud** — a structural invariant violation |
| `XM` byte outside `ZzXxHhUu.` | **Fail loud**, naming QNAME + offset |
| Hard clip (`H`) in CIGAR | **Reject.** `SEQ` would be shorter than the read and the `MD`/ref reconstruction assumptions break. Bismark does not emit `H` |
| Secondary / supplementary (`0x100`/`0x800`) | Pass through unchanged + **one** aggregated warning at the end. Bismark does not emit them |
| Deletion (`D`) | No `SEQ` position exists ⇒ nothing to rewrite. Ref reconstruction consumes `^XYZ` |
| Insertion / soft clip | `XM` is `.` (§3.1.3) ⇒ untouched. Assert this rather than assume: if a letter appears at an `I`/`S` position, fail loud |
| Empty BAM (header only) | Write a header-only output + a `Note:`; exit 0 |
| Input is a **bisulfite** (non-5-Base) BAM | Output is **byte-identical** — see §9.1. Guarded by requiring `--illumina_5base` |

---

## 4. Signature

```rust
/// One record's bisulfite-convention re-encode. Pure: no I/O, no genome, no CIGAR
/// walk beyond the reference reconstruction needed for `MD`.
pub struct Reencoded {
    pub seq: Vec<u8>,
    pub nm: i64,
    pub md: String,
    /// Positions whose base actually changed (0 for a bisulfite input).
    pub flipped: u32,
}

/// Re-encode `seq` so that each called cytosine carries the base bisulfite
/// chemistry would have produced: `C`/`G` when `XM` says methylated, `T`/`A` when
/// unmethylated. Which pair applies is fixed by `xg` (see PLAN §3.1.2).
///
/// `XM` is NOT modified — it is already correct and stays the source of truth.
///
/// # Errors
/// `InvalidXmByte`, `LengthMismatch`, `RefBaseConflict` (the §3.3.2 cross-check),
/// `CallInGap` (a letter at an `I`/`S` position), `MalformedMd`.
pub fn reencode(
    seq: &[u8],
    xm: &[u8],
    xg: XgStrand,
    cigar: &Cigar,
    md_old: &str,
    nm_old: i64,
    qname: &str,          // error messages only
) -> Result<Reencoded, FiveBaseBisulfiteError>;
```

---

## 5. Implementation outline

1. **`cli.rs`** — add `#[arg(long = "five_base_bisulfite_bam", value_name = "BAM")] pub five_base_bisulfite_bam: Vec<PathBuf>`, doc-commented in the `[#787]` style of its neighbours. Keep the `///` lines free of leading `+ `/`- `/`* ` (clippy `doc_lazy_continuation`).
2. **`mod.rs` dispatch** — beside `:167`, add `if !cli.five_base_bisulfite_bam.is_empty() { return run_five_base_bisulfite_standalone(&cli, &command_line); }`. Mutually exclusive with `--five_base_consensus_from_bam`: fail loud if both.
3. **Validation in `run_five_base_bisulfite_standalone`** — require `--illumina_5base` (mirrors the consensus flag; guards against silently rewriting a bisulfite BAM). Explicitly **do not** require `--genome`: this path needs no reference.
4. **New module `five_base_bisulfite.rs`** — `XgStrand` enum, `FiveBaseBisulfiteError`, `reencode()`, plus:
   - `reconstruct_ref(seq, cigar, md) -> Vec<u8>` (ref bases over aligned positions),
   - `emit_md(ref, seq, cigar) -> String`.
   Both are small, pure, and independently unit-testable — write them first.
5. **Driver loop** — for each input BAM: open with `from_path_without_sort_check`; copy the header and append a `@PG` line recording the invocation; per record classify → pass-through or re-encode → write. Output `<output_dir>/<input-stem>.bisulfite.bam` (1:1, unlike the consensus path's N:1 fixed name).
6. **Counters + report** — records read / re-encoded / passed through / bases flipped, and flipped-per-record mean. Print to stderr and to `<stem>.bisulfite_report.txt`.
7. **Closing `Note:`** — print the exact next commands, following the `ubam.rs:214/239` precedent of instructing rather than shelling out:
   ```
   samtools sort -o <out>.sorted.bam <out> && samtools index <out>.sorted.bam
   wgbstools bam2pat --genome hg38 <out>.sorted.bam
   ```
   Also warn that the Bismark genome's chromosome naming must match `wgbstools init_genome` (`chr1`-style), because `locus2CpGIndex` throws `std::logic_error` on an unknown locus — loudly, but only at the end of a long run.
8. **Tests** — §9.
9. **Docs + CHANGELOG + `rust/README.md`** row and Milestones line.
10. **Pre-push gates** — `cargo fmt -p bismark -- --check`; `cargo clippy -p bismark --all-targets`; and if any feature-gated code is touched, `RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings`.

---

## 6. Efficiency

- **Streaming, O(1) records in memory.** One `Vec<u8>` per record for `seq_new` plus one for the reconstructed reference — both `read_len`, reusable across records if it ever matters.
- **O(read_len) per record**, one pass for the re-encode and one for the ref reconstruction / `MD` emit.
- **No genome load.** The consensus path calls `read_genome_into_memory` (`mod.rs:531`); this path must not — that is several GB and a multi-second startup avoided for hg38.
- Effectively I/O-bound: BGZF decompress + recompress dominates. If it ever needs to be faster, the win is parallel BGZF, not the maths.

---

## 7. Integration

- **Reads:** one or more Bismark 5-Base BAMs (read-order, as the aligner emits them).
- **Writes:** `<stem>.bisulfite.bam` per input + `<stem>.bisulfite_report.txt`.
- **Order:** strictly post-alignment. Dispatch short-circuits before `pipeline()`, so no alignment code path is entered and byte-identity of the aligner is structurally untouched.
- **Downstream:** user runs `samtools sort` + `index`, then `bam2pat` → `UXM_deconv`.
- **`XM` is not modified**, so the converted BAM remains fully usable by Bismark's own extractor — a useful consistency property and the basis of §9.2.
- **Output is read-order, not coordinate-sorted** (see §10, Open-1).

---

## 8. Assumptions

**Fixed (verified in code, not configurable):**

1. `len(XM) == len(SEQ)`; `XM[i]` ↔ `SEQ[i]` in BAM space (§3.1.1).
2. `XG:Z:CT` ⇒ ref base `C`, pair `(C,T)`; `XG:Z:GA` ⇒ ref base `G`, pair `(G,A)` (§3.1.2).
3. `XM` letters occur only at `M`-consuming positions; `I`/`S` are always `.` (§3.1.3).
4. Bismark writes `NM` and `MD` on every mapped record.
5. Bismark's `NM` excludes insertions — preserved via delta arithmetic (§3.3).
6. `XM` vocabulary is exactly `Z z X x H h U u .`.

**From the reporter** ([#787](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5167178704)):

7. He will re-run through `bismark --illumina_5base`, so `XM` is present. (Had he insisted on DRAGEN BAMs, this plan would not apply.)
8. `bam2pat` defaults plus `--clip`; hg38 via `wgbstools init_genome hg38`; paired-end.
9. Variant/methylation deconvolution not needed at 10X ⇒ §10 Open-3 is safely deferred.

**Configurable:** input BAM list, `--output_dir`. Nothing about the encoding itself.

---

## 9. Validation

### 9.1 🔑 The idempotence gate — the strongest check available

**Run the converter on a standard *bisulfite* Bismark BAM. The output must be byte-identical to the input.**

This holds by construction: `methylation_call` emits `Z` exactly when the read base equals a genomic `C`, and `z` exactly when it is `T` at a genomic `C` — so for bisulfite data `SEQ` *already* carries `meth`/`unmeth` at every letter position, and `NM`/`MD` are unchanged because no base moves.

It validates, in one shot and with **existing fixtures and no 5-Base data**: the whole encoding table, the `XG`→pair mapping, the positional zip, the OB-strand orientation, and the `NM`/`MD` arithmetic. Any error in §3.1.2 or §3.3 shows up as a diff.

- **How:** existing bisulfite test BAMs (SE and PE, incl. `test_files/*_from_TrimGalore.bam`); compare decompressed SAM bodies, ignoring the added `@PG`.
- **Expected:** zero differing records; `flipped == 0` for every record.

### 9.2 5-Base forward check

- **Verify:** on a 5-Base BAM, every XM-letter position holds `meth`/`unmeth` per `XG`, and `XM` itself is unchanged.
- **How:** re-run `extract_calls` on the converted BAM and assert the calls are identical to the original's.
- **Expected:** identical call sets; `flipped > 0`.

### 9.3 The OB-strand orientation trap

- **Verify:** a `-`-strand record with a deliberately **asymmetric** `XM` (e.g. one letter near each end, different case) flips the correct end.
- **How:** hand-built record; assert exact expected `SEQ`. Construct it so a `read_pos_5p`-indexed write produces a *detectably different* string, not a coincidentally equal one.
- **Expected:** exact match. **This test is the guard against the `iter_aligned()` bug described in §2.**

### 9.4 Gaps

- **Verify:** soft-clipped and inserted positions are untouched, and a letter at such a position fails loud.
- **How:** synthetic records `{n}S{m}M`, `{a}M{b}I{c}M`; plus a fault-injected record with a letter at an `S` position.
- **Expected:** clipped/inserted bases identical; injected fault errors with `CallInGap`.

### 9.5 `NM`/`MD` exactness

- **Verify:** for records where flipping turns a mismatch into a match (and vice versa), `NM_new`/`MD_new` equal an independent from-genome recomputation.
- **How:** small synthetic genome + `make_mismatch_string`/`hemming_dist` as the oracle, mirroring `output.rs:452-460`. Include a deletion-containing record.
- **Expected:** exact agreement, **including** Bismark's insertions-excluded `NM` quirk.

### 9.6 Fail-loud matrix

Missing `XM` / `XG` / `MD` / `NM`, length mismatch, bad `XM` byte, hard clip, both `--five_base_*_from_bam` flags together, missing `--illumina_5base`. Each must produce a specific, actionable message — assert on message content, not just `is_err()`.

### 9.7 End-to-end concordance (manual, needs real data)

- **Verify:** convert → sort → index → `bam2pat` → compare per-CpG methylation against Bismark's own cytosine report for the same BAM.
- **Expected:** high correlation (concordance-gated, **not** byte-identical — `patter` applies its own `--clip`, MAPQ ≥ 10 and `-F 1796` filters). A *negative* or near-zero correlation is the signature of the inversion bug still being present, so this test's real job is proving the sign is right.

---

## 10. Questions or ambiguities

**No critical questions remain** — the reporter's answer removed the only one (XM-driven vs genome-driven).

| # | Priority | Question | Assumption taken |
|---|---|---|---|
| Open-1 | Open | Emit coordinate-sorted + indexed output, since `bam2pat` requires it? | **Read-order + a loud `Note:` with the exact commands.** Matches `ubam.rs:214/239` (which tells users to run `samtools collate`) and keeps this a pure streaming pass with no samtools subprocess in a new path. Easy to revisit |
| Open-2 | Open | Output naming: `<stem>.bisulfite.bam` vs the consensus path's fixed `five_base_consensus.bam`? | **`<stem>.bisulfite.bam`** — this transform is 1:1, so a fixed name would collide across inputs |
| Open-3 | Open | Offer masking of `--five_base_deconvolution` variant sites to `N`? | **Deferred.** Not needed at the reporter's 10X (§8.9). Worth revisiting if anyone runs this at 100X, since real `C>T` variants are encoded as methylation |
| Open-4 | Open | Also pursue the upstream `patter` two-constant patch? | **Yes, in parallel** — tracked separately. It needs no Bismark at all and works on DRAGEN BAMs, but depends on a third party merging. It does not replace this plan |

---

## 11. Self-Review

**Efficiency.** Streaming, O(read_len) per record, no genome. The only allocation growth would come from re-allocating two buffers per record; noted as a trivial optimisation, deliberately not pre-optimised.

**Logic — two errors found and fixed while drafting:**

1. **First draft recomputed `NM` from scratch.** That would have silently repaired Bismark's insertions-excluded `NM` quirk (`output.rs:453`) and made the output diverge from every other Bismark BAM in a way no test here would catch. Changed to delta arithmetic (§3.3.4) with the reasoning recorded inline, since this is exactly the kind of "cleanup" a reviewer might otherwise request.
2. **First draft gated the rewrite on a CIGAR walk** to avoid touching clipped bases. Property §3.1.3 makes that unnecessary — and the gate would have pulled in `iter_aligned()` and its `read_pos_5p` trap. Removed; replaced with an *assertion* that letters never appear at `I`/`S` positions (§3.5), which is strictly stronger: it turns an assumption into a runtime check.

**Edge cases.** Covered in §3.5: unmapped, missing tags, length mismatch, bad bytes, hard clips, secondary/supplementary, deletions, insertions, soft clips, empty BAM, bisulfite input. The `MD`-reconstruction cross-check (§3.3.2) additionally converts the plan's central claim from an assumption into a per-record runtime assertion.

**Integration.** Dispatch short-circuits before any alignment code, so aligner byte-identity is structurally preserved rather than merely tested. `XM` is never modified, which keeps the converted BAM readable by Bismark's own extractor and is what makes §9.2 possible.

**Remaining risks.**

1. **`MD` re-emission is the only non-trivial algorithm here** and the likeliest source of subtle bugs (run-length boundaries around deletions especially). §9.5 tests it against an independent from-genome oracle rather than against itself.
2. **§9.7 needs real 5-Base data** and cannot run in CI. Everything else is hermetic.
3. **The idempotence gate (§9.1) is load-bearing.** If it is ever weakened to "mostly identical", the design's central claim stops being tested at all.
4. **The output is inherently misleading if mishandled** — `SEQ` disagrees with the sequencer. Mitigations are naming, a distinct report file, the docs, and never making it the primary BAM; none of these prevent a user from feeding it to a variant caller.
