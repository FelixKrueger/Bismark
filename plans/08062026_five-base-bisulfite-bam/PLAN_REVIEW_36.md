# PLAN_REVIEW_36 — targeted review of PLAN.md §3.6 (`--five_base_baseq` masking leak)

**Scope:** `plans/08062026_five-base-bisulfite-bam/PLAN.md` rev 1, **§3.6 only**, plus its dependencies §3.2, §3.3, §3.4, §4, §5.6, §9.1, §9.7. Everything else in the plan was already double-reviewed (`PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`) and is out of scope here.

**Method:** every claim checked against `rust/bismark/src/` at `dev`, against `noodles-bam 0.89.0` in the local registry, against real fixture headers via `samtools`, and against the real consumer source fetched from `nloyfer/wgbs_tools@master` (`patter.cpp`, `patter_utils.cpp`). Two claims were verified by running commands (`samtools merge` @PG collision behaviour; fixture @PG census).

---

## Verdict

**§3.6 identifies a real bug and picks the right remedy shape — mask to `N` — but the mechanism it uses to *find* the positions is wrong in one detail that is data-destroying, and more fragile than it needs to be in three others.**

- The leak is real and I reproduced the reasoning end-to-end against upstream `patter`. The plan's diagnosis is correct.
- `N` is the correct filler: verified `is_cpg` (`patter.cpp:96-103`) and that `patter` itself pads deletions with `'N'` (`patter_utils.cpp:237-239`), so `N` reliably yields `UNKNOWN`.
- The author's rejection of B's proposal is **correct in substance** (the arithmetic is a little overstated — see F5).
- **C1 is the finding that matters:** the `QUAL` comparison is in the wrong units. Implemented literally, §3.6.3 masks essentially *every* no-call position in the file — worse than the rule the plan rejected — and §9.7 as written cannot catch it.
- **F1 is the finding worth acting on:** the converter already reconstructs the reference base at every position (§3.4 step 1), and `{XM == '.'} ∩ {ref == ref_base} ∩ {SEQ ∈ {meth, unmeth}}` is *provably exactly* the leak set, *provably empty* when no masking was applied. That removes the new CLI flag, the `@PG` parser, §3.6.4, §3.6.5, two rows of §9.6, and §11's "remaining risk 3" — and it makes §9.1 the regression test for the masking path.

---

## 0. What checks out (briefly)

| §3.6 claim | Status |
|---|---|
| `mask_low_quality` masks the call only; `SEQ` keeps the raw base | ✅ `mod.rs:1295-1307`; SE `mod.rs:1354` + `:1362-1366` passes `seq_uc` (unmasked) to `single_end_sam_output`; `cli.rs:134-135` states it as a feature |
| A masked base always yields `XM == '.'` | ✅ `N` fails both arms of the CT branch (`methylation.rs:587`, `:594`) and both of the GA branch (`:607`, `:613`) → `'.'`. So `{masked} ⊆ {XM == '.'}` **exactly**, no exceptions |
| `is_cpg` passes on a leaked `T`, and `T == unmeth_seq_chr` ⇒ UNMETH | ✅ Verified upstream: `patter.cpp:98-99` `(seq[j]=='C' \|\| seq[j]=='T') && seq[j+1]=='G'`; `patter.cpp:153-157` sets UNMETH on `unmeth_seq_chr`. Under 5-Base `T` = methylated ⇒ inverted, silently |
| `patter` never reads QUAL for calls | ✅ No `qual` reference anywhere in `patter.cpp` outside a doc comment. So the mask *must* be encoded into `SEQ` — the premise of §3.6 is sound |
| Masking is the **only** route to a leak | ✅ Independently re-derived. At a genomic `C`, `XM == '.'` implies the call-sequence base ∉ {C,T} (`methylation.rs:587-599`); with no masking the call base *is* `SEQ[i]`, so `SEQ[i]` ∉ {C,T} and `is_cpg` fails. `I`/`S` deleted by `clean_CIGAR` (`patter_utils.cpp:239-241`), `D` has no `SEQ` position, edge-guard records never written (`mod.rs:1348-1351`) |
| `N` at a `.` position is a new mismatch ⇒ `NM`/`MD` after masking | ✅ `hemming_dist` (`output.rs:150-157`) counts it; `make_mismatch_string` (`output.rs:196-200`) emits the ref base at a mismatch, so an `N` read base is handled like any other mismatch. §3.4 operating on the final `seq_new` is the right ordering |
| Select on `ID:Bismark`, not the last `@PG` | ✅ Directionally right and verified: `dedup/synth_barcode_10k_..._pe.bam` carries **six** `@PG` lines (`Bismark`, `samtools`, `samtools.1`, `.2`, `.3`). Taking the last would read a `samtools view` command line. See F6/F7 for what the rule still misses |
| Consensus BAM applies no masking | ✅ `mod.rs:2306` passes literal `0`. See F8 for the part the plan gets right only by accident |
| `QUAL[i]` ↔ `XM[i]` ↔ `SEQ[i]` coherent in BAM space (unstated but load-bearing) | ✅ Not asserted anywhere in the plan, but true: `scores.reverse()` sits in the same `if strand == b'-'` block as the `SEQ` revcomp and the `XM` reversal — SE `output.rs:443-450` + `:463-467`, PE `output.rs:671-675` + `:694-698` |

---

## 1. Critical

### C1 — 🔴 The `QUAL` comparison is in the wrong units, and the `phred64` parameter must not exist

§3.6.3: *"at every position with `XM == '.'` **and** `QUAL[i] < offset + n`, write `b'N'`"*, and §4's signature takes `phred64: bool` to choose the offset.

That is `mask_low_quality`'s comparison (`mod.rs:1299-1303`, `min = offset + baseq`) — correct **there**, because it receives raw FastQ ASCII: `mod.rs:1586-1587` builds `qual1_bytes` straight off the FastQ line via `chomp_newline`.

The converter does not read FastQ. It reads a BAM, and the BAM stores **offset-free Phred scores**:

```rust
// aligner/output.rs:437-441
// QUAL → phred SCORES for the BAM (ASCII − offset). phred64 input (Perl 4191)
// uses offset 64; default phred33 uses 33. `samtools view -h` re-renders ASCII+33.
let offset: u8 = if phred64 { 64 } else { 33 };
let mut scores: Vec<u8> = qual.iter().map(|&q| q.wrapping_sub(offset)).collect();
```

Same at `output.rs:668-669` for PE. On read-back, `noodles_bam::record::QualityScores::as_bytes` (`noodles-bam-0.89.0/src/record/quality_scores.rs:15-17`) returns the raw BAM field, i.e. those same scores — and this repo already documents it: `aligner/ubam.rs:127` — `let quals = rec.quality_scores().as_ref(); // raw phred (0-based), empty if absent`.

**Consequences of the plan as written:**

- The correct condition is `qual[i] < n`. Nothing else.
- `phred64` is not merely redundant, it is absorbed at write time: `ascii − 64 < n ⟺ score < n` and `ascii − 33 < n ⟺ score < n`. The BAM-space threshold is `n` for **both** input encodings. Passing `phred64` into `reencode()` invites `offset + n` with `offset == 64`, which masks every position (real scores top out near 41–60).
- Even with `offset == 33` and a sane `n == 20`, the literal rule masks every `.` position whose score is below 53 — i.e. **every no-call position in the file** (~80% of bases at 5-Base cytosine density). That is strictly worse than the rule §3.6 rejected: it destroys the read, inflates `NM` by ~0.8·read_len, and rewrites `MD` into near-total mismatch.
- It fails **only** when `n > 0` (§3.6.3 short-circuits at `n == 0`), which is exactly the case CI cannot reach: §9.1 runs on bisulfite BAMs that resolve `n = 0`, and §9.9 is manual. Combined with C2, this bug ships.

**Action:** §3.6.3 → `QUAL[i] < n` where `QUAL` is the BAM's stored score. Delete `phred64` from §4's signature. Add one sentence to §3.6 recording *why* (`output.rs:439-440` normalises at write time), or a future reader will "restore" the offset.

### C2 — 🔴 §9.7 has no negative control, so it passes under C1

§9.7 asserts only the positive direction: a low-quality `.` position whose `SEQ` is in {meth, unmeth} *must* become `N`. An implementation that masks **every** `.` position passes that assertion. So does one that masks the whole read.

The masking path is reachable in CI only through hand-built records (§9.1's fixtures all resolve to `n = 0`; §9.9 is manual and needs real data), which makes §9.7 the *sole* automated guard on a silent data-destruction mode.

**Action — §9.7 must assert the boundary, not just the hit:**

1. `masked == 1` **exactly** on a record built with one sub-threshold `.` position (not `>= 1`).
2. A **high-quality** `.` position in the same record is byte-identical to the input (this is the assertion C1 fails).
3. A position at `QUAL == n` exactly is **not** masked (`mask_low_quality` uses `q < min`, so `n` itself is kept — `mod.rs:1303`).
4. No **letter** position is ever masked (`masked` and `flipped` are disjoint — see F4).
5. One realistic-scale case: a 100 bp record with two sub-threshold bases must yield `masked == 2`, `NM_new == NM_old + 2` — a whole-read mask fails this loudly.

---

## 2. Important

### F1 — 🟠 There is a simpler rule that is *exactly* correct and needs neither `QUAL` nor `@PG`

This answers the caller's question 7, and I think it should replace §3.6.1–§3.6.5.

§3.4 step 1 **already reconstructs `ref_seq` for every record**, one byte per read position, from `(SEQ_old, CIGAR, MD_old)` — and §3.4 step 2 proves it byte-exact before anything is emitted. `ref_seq` is in BAM space, and §3.1.2 fixes the reference base at a scoreable cytosine: `C` for `XG:CT`, `G` for `XG:GA`. So the converter already knows, for free, whether a given position *is* a cytosine on the read's strand.

**Proposed rule:**

> mask `b'N'` iff `XM[i] == '.'` **and** `ref_seq[i] == ref_base` **and** `SEQ[i] ∈ {meth, unmeth}`

Three properties, all derivable from code already read for this review:

1. **It is exactly the leak set.** `patter` scores position `j` only when the reference position is a dictionary CpG (`patter.cpp:141`, `if (conv[start_locus + i])`) *and* `is_cpg` passes, which requires `seq[j] ∈ {C,T}` (OT) / `{G,A}` (OB) — `patter.cpp:98-101`. A leak therefore requires `SEQ[i] ∈ {meth, unmeth}` at a reference cytosine with no Bismark call. Nothing outside the set can leak; nothing inside it is ever scored correctly (under 5-Base chemistry `patter`'s verdict there is inverted by construction).
2. **It is provably empty when no masking was applied.** At a genomic `C` with `SEQ[i] ∈ {C,T}`, `methylation_call`'s CT branch emits a letter unconditionally (`methylation.rs:587-596`) — so `XM == '.'` at such a position is *only* possible if the call sequence differed from `SEQ`, which today happens exactly when `mask_low_quality` fired. GA branch symmetric (`:607-614`). No threshold to know, nothing to detect.
3. **It costs nothing.** No genome, no new flag, no header parsing, no `QUAL`. `MD` is already mandatory (§3.7) and already validated per record (§3.4 step 2). At `I`/`S` positions `ref_seq[i] == b'X' != ref_base`, so clipped/inserted bases are structurally excluded — which is what F2 and F3 below both want.

**What it buys:**

- C1 disappears (no `QUAL`, no offset, no `phred64`).
- §3.6.1, §3.6.2, §3.6.4, §3.6.5 disappear; so do two rows of §9.6 and §11's remaining-risk 3 ("§3.6's `@PG` parsing is the one fragile mechanism in the plan").
- F6/F7/F8 (merge, reheader, consensus) all disappear — a header the converter never reads cannot mislead it.
- **§9.1 becomes the regression test.** Property 2 says the masked count on any non-masked input is 0, so the idempotence gate on bisulfite fixtures now *does* cover the masking code path — previously it could not (C2).
- It also covers any *future* divergence between the call sequence and `SEQ` (a second masking option, a consensus variant mask, a trimming pass) without a code change, because it keys on the invariant rather than on one flag's name.

**What it costs — stated fairly:** the fix becomes silent. Mitigate the same way B's option 3 suggested: report `masked` in `<stem>.bisulfite_report.txt` and print a one-line `Note:` when it is non-zero (*"N no-call cytosines masked to N; this input appears to have been produced with --five_base_baseq"*). That is strictly more informative than the current design, which prints the threshold it guessed rather than the positions it actually found. If a belt-and-braces flag is still wanted, keep `--five_base_bisulfite_baseq` **only** as an assertion (`0` ⇒ require `masked == 0`), never as the identifying mechanism.

I am not claiming §3.6's approach produces wrong *calls* once C1 is fixed — with `qual[i] < n` and the right `n` it masks a superset of the leak, which is safe for `patter`. F2 and F3 are about what that superset costs; F1 is about the machinery it needs to guess `n` at all.

### F2 — 🟠 The `QUAL`-only set suppresses genuine, high-quality CpG calls

`is_cpg` reads the read's **neighbour** base, not the reference (verified upstream, `patter.cpp:96-103`):

```cpp
if (ro.shift == 0) return (j < seq.size()-1) && ((seq[j]=='C') || (seq[j]=='T')) && (seq[j+1]=='G');
else               return (j > 0)            && ((seq[j]=='G') || (seq[j]=='A')) && (seq[j-1]=='C');
```

So on an OT record, `N`-masking a low-quality `G` at `j+1` destroys the call at `j` — even when `j` itself is high quality and correctly encoded. That `G` sits at a reference `G`, so its `XM` is `'.'` (`methylation.rs:591-592`), and if its score is below `n` the plan masks it. Symmetric on OB: masking a low-quality `C` at `j-1` kills the call at `j` (`XM == '.'` there too, since an `XG:GA` record's letters live at reference `G`).

Magnitude is modest — of the order of the intended masking itself — and the direction is conservative (lost observations, not wrong ones). But it is avoidable at zero cost: `G ∉ {C,T}` and `C ∉ {G,A}`, so adding the `SEQ[i] ∈ {meth, unmeth}` conjunct (F1's third clause, i.e. B's condition intersected with the plan's) spares both neighbours exactly.

### F3 — 🟠 The `QUAL`-only set corrupts the inline UMI in the soft-clipped prefix, silently

With `--five_base_umi_len 8` the UMI prefix is *in* `SEQ` and soft-clipped (`cli.rs:125-128`), and `mask_low_quality` masks the whole read including that prefix — so those positions are `.` with low `QUAL`, and §3.6.3 masks them to `N`.

That is invisible to every check in the plan: `ref_seq` carries `b'X'` at `S` positions, `X != N` and `X != <base>` both mismatch, so `NM` is unchanged (`output.rs:150-157`), and `make_mismatch_string` skips `Some(&b'X')` entirely (`output.rs:195`) so `MD` is unchanged. The only visible effect is that the UMI bases are gone — and Bismark's own `--five_base_umi_len` dedup re-derives the UMI from exactly that prefix.

Either conjunct fixes it (F1's `ref_seq[i] == ref_base` excludes `X`; F2's `SEQ[i] ∈ {meth, unmeth}` excludes most of it). Worth an explicit line in §3.6: **never write into a soft-clipped or inserted position.** §3.3 invariant 3 currently only forbids `XM` *letters* there; masking would be the first thing in the design that writes into a gap.

### F4 — 🟠 §3.2's loop counts masks as flips, so §5.7 fails loud on every real masked run

§3.2's pseudocode puts the flip counter at loop level:

```
    b'.'  =>  ()                     # but see §3.6
    ...
if seq_new[i] != seq[i] { flipped += 1 }
```

§3.6.3 says *"Count these separately from `flipped`"* and §4 documents `flipped/letters ∈ {0.0, 1.0}` — but the natural implementation folds masking into the `'.'` arm of that same loop, and then every mask increments `flipped`. §5.7 and §3.5 fail loud when the rate is *"neither 0 nor 1"*, so a correctly-masked 5-Base file reports `flipped/letters > 1` and **aborts**. A false failure that looks like a data problem, on exactly the quality-conscious input the feature was written for.

**Action:** move the comparison inside the two letter arms (`flipped` is a letter-position statistic, `masked` a gap-position statistic; they are disjoint by construction), and say so in §3.2 rather than only in §3.6.3.

### F5 — 🟠 §3.6.4 makes the escape hatch inoperative

§3.6.4: *"If auto-detect finds a value and the user passed a **different** one ⇒ fail loud."* §9.6 confirms this is intended (*"baseq flag disagreeing with the header"*).

Then the flag can only ever *confirm* the header — it can never override it. That is precisely backwards for the two cases where auto-detect is wrong: the consensus BAM whose synthesised header carries an inert `--five_base_baseq` (F8), and a merged BAM where the wrong `@PG` won (F6). The user knows the right answer, passes it, and gets a hard error with no way forward.

**Action:** an explicit `--five_base_bisulfite_baseq` should **win**, with a `Note:` when it differs from the header (*"header says 20, using 0 as given"*). Keep fail-loud for the genuinely unresolvable case only. Note that F1 removes this decision entirely.

Aside, on the caller's question 3: the plan's *"roughly half of every read"* for B's rule is an overestimate. `P(XM == '.') ≈ 1 − C_content ≈ 0.79` for hg38, and `P(read ∈ {C,T} | not a called cytosine) ≈ T/(A+G+T) ≈ 0.37`, so B's rule hits ≈ **30%** of a 5-Base read and ≈ **40%** of a bisulfite read (T-enriched). The conclusion is unaffected — it destroys the file and shatters §9.1 — but if the figure stays in the plan as the recorded reason for rejecting a reviewer's proposal, it should be right. The *real* reason to reject B's rule as stated is not the count: it is that `{SEQ ∈ {meth, unmeth}}` alone does not imply the position is a reference cytosine, which is what makes it over-broad. Intersected with either the plan's `QUAL` test or F1's `ref_seq` test it becomes exactly right — the two proposals compose rather than compete, and §0's "One reviewer recommendation was rejected" overstates the disagreement.

### F6 — 🟠 A merged BAM carries two Bismark `@PG` lines and the rule silently picks one

§3.6.2 selects on `ID:Bismark`. Verified empirically what `samtools merge` does with the collision:

```
$ samtools merge -f -o m.bam a.s.bam b.s.bam   # both Bismark BAMs
@PG  ID:Bismark               VN:v0.25.1  CL:"bismark --genome /g reads.fq.gz"
@PG  ID:Bismark-35BF4663      VN:v0.25.1  CL:"bismark --genome /g reads.fq.gz"
```

The second is renamed with an 8-hex suffix. An exact `ID:Bismark` test matches the first and ignores the second; if the two runs used different `--five_base_baseq`, the resolved threshold depends on merge order, silently.

**Action (if `@PG` parsing survives):** match `ID:Bismark` as a **prefix** (covers `Bismark-<hex>` from `merge` and `Bismark.<n>` from other paths), collect the value from *every* Bismark `@PG`, and fail loud on disagreement. That is the case fail-loud is actually for.

### F7 — 🟠 Reuse `detect_paired_from_header`; don't write a fourth `@PG` parser

§5.4 introduces `parse_baseq_from_pg()` from scratch, and §2's "Existing code to reuse" does not mention that this repo already has the exact same parser, hardened:

`crate::io::read::detect_paired_from_header` (`io/read.rs:710-741`) serialises the header to SAM text (with a comment explaining why: the text format is the stable contract, not noodles' `Programs` type), filters `@PG` lines containing `ID:Bismark` (a **substring** test — it already handles F6's renamed line), and uses `arg_present` (`io/read.rs:751+`) for whitespace-delimited token matching. It resolves **last-Bismark-`@PG`-wins** (`:736-738`, deliberate, matching Perl).

Two things follow:

- §3.6.2's warning — *"Select on `ID:Bismark`, **not** the last `@PG` — ... 'last @PG wins' is a trap this repo has already hit"* — conflates two rules. The trap was taking the last `@PG` of *any* ID; "last **Bismark** `@PG` wins" is the existing, intentional behaviour. As written the plan reads as an instruction to contradict `io/read.rs:736-738`. Reword: filter to Bismark `@PG` lines first, *then* decide first / last / all-must-agree (F6 argues all-must-agree).
- The parser must accept **both** `--five_base_baseq 20` and `--five_base_baseq=20`; clap accepts both and `command_line` is a verbatim `argv.join(" ")` (`mod.rs:118-119`). `arg_present`-style token matching alone misses the `=` form. Also worth noting for the implementer: `--five_base_bisulfite_baseq` does not contain `--five_base_baseq` as a substring, so the converter's own flag cannot self-match — but only token matching makes that robust rather than lucky.

### F8 — 🟠 The consensus BAM: no leak exists, and auto-detect gets the right answer by luck — or the wrong one

The caller's question 5. Three separate facts:

1. **The leak does not exist in a consensus BAM.** `mod.rs:2306` passes literal `0` for baseq, so no `mask_low_quality` call ever fires on that path. The consensus' *other* `N` bases (uncovered, tie, or C>T-variant positions — `five_base_duplex.rs:332`, `:338`, `:358`, `:361`) are already `N` in `SEQ`, so `is_cpg` fails and `patter` returns `UNKNOWN`. `revcomp` preserves `N` (`output.rs:170`), so the reverse record is fine too. Correct as-is; §7's claim that the consensus BAM is a valid input holds.
2. **Auto-detect reaches `0` by luck.** `run_five_base_consensus_standalone` **synthesises** a fresh header — `generate_sam_header(&genome, command_line)` at `mod.rs:533`, with `command_line` being the *consensus* invocation's argv. The original per-read run's `@PG` (the one that would name `--five_base_baseq 20`) is discarded. So the auto-detect answer for a consensus BAM has nothing to do with the run that produced the underlying calls; it is right only because the consensus applies no masking of its own.
3. **And it can reach the wrong answer.** `--five_base_baseq` is *accepted* on the consensus path: `run()` short-circuits at `mod.rs:167-169` **before** `resolve()`, and `run_five_base_consensus_standalone` validates only `--illumina_5base` and `--genome` (`mod.rs:517-526`). So `bismark --five_base_consensus_from_bam x.bam --illumina_5base --genome g --five_base_baseq 20` is legal, inert, and lands verbatim in the synthesised `@PG CL:`. The converter would then read `20` off a file with no masking and mask its no-call positions — and per F5 the user cannot override it.

**Action:** §7's consensus paragraph needs one sentence saying the consensus header describes the *consensus* command, not the alignment that produced the calls. F1 makes the whole question moot (`masked == 0` provably, by property 2).

### F9 — 🟠 §3.6 cites only the SE call site, for a paired-end-only feature

§3.6 cites `mod.rs:1295-1307` and `:1352-1360`. `mod.rs:1352-1360` is inside `five_base_emit_record`, the **SE** helper — whose only production caller is the consensus path (`mod.rs:2298`), the one path that passes `baseq = 0`. 5-Base is paired-end only (`cli.rs:99-100`).

The load-bearing call site is `mod.rs:1695-1697` in `five_base_emit_pe_record`:

```rust
let off = if phred64 { 64 } else { 33 };
let call1 = mask_low_quality(seq1_uc, qual1, baseq, off);
let call2 = mask_low_quality(seq2_uc, qual2, baseq, off);
...
let (out1, out2) = paired_end_sam_output(identifier, seq1_uc, seq2_uc, qual1, qual2, ...);
//                                                   ^^^^^^^^ ^^^^^^^^ unmasked
```

Same leak, both mates, and it is the only one a real user hits. Add `mod.rs:1695-1697` + `output.rs:645-708` (`build_pe_mate`) to §3.6's citations. This is accuracy, not logic — but an implementer told to verify "is `SEQ` really unmasked?" will check the cited SE line, find the consensus caller passing `0`, and could reasonably conclude the leak is unreachable.

### F10 — 🟠 The resolved threshold is not recorded anywhere in the output

§5.6 resolves the threshold; §5.7 lists counters but not the threshold. Under auto-detect, nothing in the output BAM or the report says which value was used, so the conversion is not reproducible from its own artefacts — and the output BAM is itself a "regenerated header" case for anything downstream.

**Action:** put the resolved threshold and the `masked` count in `<stem>.bisulfite_report.txt`, and give the appended `@PG` a **distinct ID** (`ID:bismark-five-base-bisulfite`, `PP:Bismark`) — appending a second `ID:Bismark` would violate SAM's unique-ID rule and confuse this very auto-detect. Worth one line in §5.6 that the output *keeps* the input's `@PG ID:Bismark`, so re-running the converter on its own output auto-detects the same threshold; that is harmless (already-`N` stays `N`, letters are no longer `.`), i.e. accidentally idempotent, but it should be a stated property rather than a coincidence.

---

## 3. Optional

- **O1 — absent `QUAL`.** A BAM with `QUAL == *` stores `0xFF` per base, and noodles returns those bytes verbatim (`record/quality_scores.rs:15-17`, no missing-value collapse — `is_empty()` is only true for an empty `SEQ`). Under §3.6.3 nothing would be masked (`255 < n` is never true), which is *safe*, but it means the masked set is unidentifiable. If `@PG` parsing survives, a resolved `n > 0` on a record with no `QUAL` is a contradiction worth failing loud on. (Unreachable via Bismark itself, which always writes `QUAL`. F1 doesn't care.)
- **O2 — `N` is `patter`'s own idiom.** `clean_CIGAR` pads `D`/`N` CIGAR ops with `'N'` (`patter_utils.cpp:237-239`), so `N` is exactly how `patter` represents "no information". Worth one line in §3.6 — it makes the choice of filler a verified property of the consumer rather than an inference.
- **O3 — `masked` scope.** §4's `masked: u32` is per record; §5.7 reports a file total. Fine, but state that `masked` is *not* part of the flip-rate denominator anywhere it is mentioned (§3.5, §4, §5.7) — F4 shows that ambiguity is one careless line from a false abort.
- **O4 — non-CpG cytosines.** Both the plan's rule and F1's mask *all* no-call cytosines, including CHG/CHH, which `patter` never reads. Harmless (~0.2–0.4% of bases at `n = 20`) and it keeps the rule simple; not worth narrowing to CpG context, but worth one line so a later reader does not think it is a bug.
- **O5 — §9.7's `is_cpg` assertion.** *"Assert that `is_cpg` would then fail"* — the test cannot call `is_cpg`. What it can assert is the emitted byte, which is what the section then says. Reword to avoid implying a C++ dependency in a Rust test.

---

## 4. Answers to the seven questions asked

| # | Question | Answer |
|---|---|---|
| 1 | Is the `QUAL` comparison correct? | **No — C1.** `mask_low_quality` receives FastQ ASCII (`mod.rs:1586-1587`); a BAM stores offset-free scores (`output.rs:439-440`, `ubam.rs:127`, noodles `quality_scores.rs:15-17`). Correct test is `qual[i] < n`; `phred64` must be deleted from §4 |
| 2 | Is the masked set correctly identified? | **Exactly, then over-broadly.** `{XM == '.'} ∩ {score < n}` *is* Bismark's masked set precisely (every masked base yields `'.'`, `methylation.rs:587-616`) — no under-capture. But it is a strict superset of the *leak*, and the extra positions cost real calls (**F2**) and the inline UMI (**F3**). Minimal set = add `SEQ[i] ∈ {meth, unmeth}` |
| 3 | Is B's proposal rightly rejected? | **Yes in substance, with the wrong arithmetic and the wrong reason.** ≈30% of a 5-Base read / ≈40% of a bisulfite read, not "roughly half"; and the defect is that `SEQ ∈ {meth, unmeth}` does not imply a reference cytosine. Intersected with a positional test it is *exactly* right — the two proposals compose (**F5**, **F1**) |
| 4 | Is `@PG` auto-detection sound? | **Fragile.** Right to filter on `ID:Bismark` (fixtures carry up to six `@PG`), but: misses the renamed sibling in a merged BAM (**F6**, verified), reads a synthesised header for a consensus BAM (**F8**), reinvents `detect_paired_from_header` and misstates its rule (**F7**), and its fail-loud is strict where risk is low while §3.6.4 blocks the override where auto-detect is actually wrong (**F5**). §3.6.5's harshness is defensible; its *asymmetry* is not |
| 5 | The consensus BAM specifically | **No leak exists there** (`mod.rs:2306` passes `0`; other no-calls are already `N`). Auto-detect returns `0` **by luck** — the header is synthesised from the consensus command (`mod.rs:533`) — and returns the **wrong** value if the user passes the inert `--five_base_baseq` on that command line, which is accepted (`mod.rs:167-169` precedes `resolve()`). **F8** |
| 6 | Interaction with `NM`/`MD` and the gates | **Ordering is right.** `N` is a normal mismatch for both `hemming_dist` (`output.rs:150-157`) and `make_mismatch_string` (`output.rs:196-200`), and §3.4 operating on the final `seq_new` handles it. §9.1 is unaffected (bisulfite input resolves `n = 0`) *provided* §3.6.5 means "no Bismark `@PG`" and not "no baseq token" — the plan does not say which, and the two readings give opposite results for the primary gate (**F11 below**). §3.5's invariant **is** at risk, via §3.2's loop, not via the maths (**F4**) |
| 7 | Anything simpler and equally correct? | **Yes — F1.** Key on the reconstructed `ref_seq` §3.4 already computes: `{XM == '.'} ∩ {ref_seq[i] == ref_base} ∩ {SEQ[i] ∈ {meth, unmeth}}`. Provably exactly the leak set, provably empty without masking, no flag, no `QUAL`, no header parsing |

### F11 — 🟠 (from question 6) §3.6.2 vs §3.6.5: "no baseq token" and "no Bismark `@PG`" are not the same case

§3.6.2 parses *"the `@PG` line with `ID:Bismark` for `--five_base_baseq <n>`"*. §3.6.5 fails loud when *"auto-detect cannot find a Bismark `@PG`"*. The common case — a Bismark `@PG` **is** present but carries no `--five_base_baseq` — is covered by neither sentence.

It has to resolve to `n = 0`: the flag's `default_value_t = 0` (`cli.rs:136`) means absence genuinely *is* "no masking". Under that reading §9.1 passes on every fixture (verified: `filter_nonconversion/se_default/in.bam` → `CL:"bismark --genome /g reads.fq.gz"`, `dedup/nondir_pe_1030.bam` → `CL:"bismark ... --non_directional ..."`, neither with a baseq token). Under the other reading the plan's own primary gate fails loud on every fixture in §9.1. One sentence fixes it — but the fact that a plan this careful is ambiguous on the trigger condition of its own fail-loud is itself an argument for F1, which has no trigger condition.

---

## 5. Action items

**Critical — do not implement §3.6 without these**

1. **C1** — §3.6.3: `QUAL[i] < n` (BAM scores are offset-free); delete `phred64` from §4's signature and say why in one line.
2. **C2** — §9.7: add the negative controls (`masked` exact; high-quality `.` untouched; `QUAL == n` kept; letters never masked; `NM_new == NM_old + masked`).

**Important**

3. **F1** — replace the `QUAL` + `@PG` mechanism with the `ref_seq`-based rule. Subsumes C1, F2, F3, F5, F6, F7, F8, F11 and §11's remaining-risk 3. If it is adopted, keep the `masked` counter and the `Note:`, and downgrade `--five_base_bisulfite_baseq` to an optional assertion or drop it.
4. **F4** — move the flip counter into §3.2's letter arms; state that `flipped` and `masked` are disjoint (otherwise §5.7 aborts every real masked run).
5. **F2 / F3** — if §3.6's `QUAL` route is kept, add both conjuncts (`SEQ[i] ∈ {meth, unmeth}`, and never write into `I`/`S`).
6. **F5** — an explicit flag must override the header with a `Note:`, not conflict-error.
7. **F6 / F7** — prefix-match `ID:Bismark*`, require all Bismark `@PG` values to agree, accept `--flag=value`, and reuse `detect_paired_from_header`/`arg_present` (`io/read.rs:710-755`) rather than a new parser; reword §3.6.2's "last `@PG`" warning so it does not read as contradicting `io/read.rs:736-738`.
8. **F8 / F9** — §7: the consensus header describes the consensus command, not the alignment. §3.6: cite the PE call site `mod.rs:1695-1697`.
9. **F10** — record the resolved threshold + `masked` in the report; give the output's appended `@PG` a distinct ID.
10. **F11** — say explicitly that a Bismark `@PG` without the flag resolves to `0`.
11. **F5 aside** — correct the ≈30–40% figure in §0/§3.6 and reframe "rejected" as "composed with a positional test".

**Optional** — O1–O5 above.

---

## 6. One line on the rest of §3.6

The diagnosis, the choice of `N`, the "masking is the only leak" enumeration, the `NM`/`MD`-after-masking ordering, and the decision to distrust the last `@PG` all check out against source. The section's problem is not its reasoning about `patter` — that part is now verified from upstream — it is that it reaches outside the record for information (a header, a threshold, an encoding offset) that the record already contains.
