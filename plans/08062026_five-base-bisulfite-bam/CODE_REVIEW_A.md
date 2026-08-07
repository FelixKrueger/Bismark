# CODE_REVIEW_A — `6a3ea0d` · `--five_base_bisulfite_bam` (#1095)

**Reviewer A** · fresh context · target: commit `6a3ea0d` on `dev`
**Focus:** the algorithm in `rust/bismark/src/aligner/five_base_bisulfite.rs` and its correctness.
**Method:** read the real source (`methylation.rs`, `output.rs`, `io/`, `rust_ci.yml`) rather than the plan or the comments; then ran ~40 adversarial inputs through `reencode` from a throwaway integration test, built the independent from-genome `MD`/`NM` oracle that PLAN §10b lists as *not done*, and exercised the driver on five real fixtures plus two synthesised inputs. All probe files removed; the tree is clean apart from one comment-only fix (below).

---

## 1. Summary

**The algorithm is correct.** I tried hard to break `reconstruct_ref` / `parse_md` / the masking rule and did not manage it on any input a Bismark BAM can contain. Four claims the plan makes and I verified independently rather than taking on trust:

1. **The §3.1.2 `XG` → (ref base, meth, unmeth) table is right at all four strand indices.** I re-derived it from `methylation.rs:129-141` (index → strand/XR/XG), `:604-620` (the GA branch tests `genomic[i+2]`), the extraction's asymmetric ±2 padding (`:154-160` prepend for eff 1/3, `:199-204` append for eff 0/2), the extraction revcomp (`:216-222`, eff 1/2) and the output revcomp (`output.rs:449-454`). The prepend/append asymmetry is what makes `genomic[i+2]` land on the same locus for both GA indices, and both revcomps cancel — so `ref_seq`, `SEQ` and `md_seq` are all genome-forward in BAM space for every index. **Empirically confirmed**: `tests/data/dedup/nondir_pe_1030.bam` (20 records, all four `XR`/`XG` combinations, non-directional PE) passes through the converter with a **byte-identical SAM body**.
2. **The `NM` identity is exact.** `bismark_nm` (`five_base_bisulfite.rs:364-366`) is `hemming_dist(seq, ref_seq) + deleted`, matching `output.rs:456` (SE) and `output.rs:690` (PE `build_pe_mate`) character for character, including `hemming_dist`'s counting of `b'X'` padding at `I`/`S` (`output.rs:147-157`) and `indels` being deletions-only (`methylation.rs:182-190`). Confirmed on the fixture: `ot_softclip` `NM:i:20 = 11 mismatches + 9 soft-clipped`.
3. **The `run_left > 0` guard before a `D` (`:306-314`) is correct and rejects no valid `MD`.** A deletion always terminates an `MD` match run with a digit before `^`, so `run_left` is necessarily 0 at a `D`. I confirmed both adjacent cases work: mismatch immediately *before* a deletion (`MD "3T0^GA4"`) and immediately *after* (`MD "4^GA0A3"`), plus two deletions in one record (`MD "3^AC3^T3"`).
4. **The masking rule is exactly the leak set.** Verified property 2 at source: on the CT branch a genomic `C` emits a letter for a call base of `C` **or** `T` with no further guard (`methylation.rs:592-599`), GA symmetric (`:610-617`). So `XM == '.'` ∧ `ref == ref_base` ∧ `SEQ ∈ {meth, unmeth}` proves `call_seq != SEQ`, i.e. `mask_low_quality` fired. Nothing else can reach the set. Confirmed end-to-end by synthesising a masked input (blanked one `Z` in the fixture's `XM`, which leaves `NM`/`MD` valid so the round-trip proof still passes): **exactly one base became `N`, `NM` 10→11, `MD` `1C9C24…` → `1C4C4C24…`, and the other seven records were byte-identical.** Surgical.

**Beyond the plan — I built §9.5's missing oracle and it passes.** Two cases, comparing the emitted `MD`/`NM` against a reference supplied explicitly (never reconstructed from `MD`):

| case | input `MD` / `NM` | emitted | oracle | verdict |
|---|---|---|---|---|
| `8M2D8M`, fully-methylated 5-Base read, mismatch abutting the deletion | `1C1C1C2^GA0C4C1C0` / 8 | `8^GA8` / 2 | `8^GA8` / 2 | ✅ |
| `3S5M2D4M1I4M1D3M`, two deletions + leading clip + insertion | `1C1C1^GA0C1C0C0G0C0A0C0^T0G0C0A0` / 19 | `5^GA3C0G1A1^T0G0C0A0` / 13 | same | ✅ |

That is the highest-risk code in the change (`rebuild_md_with_deletions`, the verbatim Perl port whose own comments read *"Perl dies — unreachable"*) running on a much denser `MD` than the one it round-tripped, and it agrees exactly. **These two cases should be committed as unit tests** — they are the strongest evidence in the change and cost nothing now that they are known to pass.

**The defects I found are all in the driver and the test surface, not the maths.** Two are High. Nothing is Critical.

**Prior reviews:** `PLAN_REVIEW_36`'s F1 (positional conjunct), F4/T2 (flip counter inside the letter arms — honoured at `:467-470` vs `:476-479`; `letters` and `masked` are genuinely disjoint) and F10/T6 (distinct `@PG` ID — honoured and tested) are all correctly implemented. R13 (reject `=`/`X`/`P`/`H`) is honoured: `parse_cigar` accepts any non-digit byte, and `reconstruct_ref:334-339` rejects it — I confirmed `4=`, `2=2X`, `4M0P` and `2H4M` all produce `UnsupportedCigarOp`. I re-raise none of their settled points.

---

## 2. Issues

### HIGH-1 · Every error path after the writer opens leaves a valid-looking, incomplete `.bisulfite.bam` on disk

`mod.rs:716` opens the writer; the D3 guard is at `:812`, the flip-rate guard at `:830` — **both after `writer.finish()` at `:804-806`** — and a per-record `reencode` failure propagates out of the loop with the writer merely dropped. `Drop` still writes the BGZF EOF marker (`io/write.rs:36-40`), so the leftover file *passes `samtools quickcheck`*.

Measured, not inferred:

```
$ bismark --illumina_5base --five_base_bisulfite_bam BS-seq_10K_se_trimmed_from_TrimGalore.bam --output_dir $T
error: … contained 9996 record(s) but none could be re-encoded …
exit=1
-rw-r--r--  218526  BS-seq_10K_se_trimmed_from_TrimGalore.bisulfite.bam   ← verbatim copy of the input
```

```
$ bismark … --five_base_bisulfite_bam calmd.bam …        # mid-stream per-record abort
error: record ot_softclip: the reconstructed reference failed its round-trip proof …
-rw-r--r--  572  calmd.bisulfite.bam    ← 1 of 8 records; samtools quickcheck passes
```

Three reasons this matters more than a stray temp file:

- **The D3 guard's entire purpose is defeated.** §10b D3 says it exists because otherwise *"the run 'succeeded' while emitting a copy of the input — the user would believe they had converted something."* The run now fails, but **the file the user would believe in is still there**, still named `.bisulfite.bam`. A pipeline globbing `*.bisulfite.bam` gets it.
- **The flip-rate message asserts something untrue.** `mod.rs:831-837`: *"refusing rather than writing a file that is half one convention and half the other."* The file is fully written and finalised before the check runs.
- The mid-stream case yields a **silently truncated** BAM that looks healthy.

`RoundTripFailed`'s own message — *"nothing was written"* — states the intended semantics. Make it true.

**Recommendation (I did not apply this: `mod.rs` is being edited concurrently by another agent, and there are two defensible fixes).** Preferred: write to `<out>.bisulfite.bam.tmp` and `fs::rename` after all three guards pass — that also makes the whole per-input transform atomic. Minimal alternative: a `let _ = std::fs::remove_file(&out_path);` before each of the three error returns, plus wrapping the record loop so a per-record error takes the same path. Note `BamWriter::from_path` already establishes the cleanup convention (`io/write.rs:49-55` removes the file on header-write failure).

### HIGH-2 · The committed test surface is single-end and covers only 2 of the 4 strand indices — on a paired-end-only feature

Every test in `tests/aligner_five_base_bisulfite.rs` uses one fixture, `softclip_indel_se.bam`: 8 **single-end** records, all `XR:Z:CT`, i.e. strand indices 0 and 1 only. Indices 2 and 3 (`XR:Z:GA` — non-directional / PBAT) are never exercised by any test. Meanwhile 5-Base itself is **paired-end only** (`cli.rs:99-100`, quoted by the plan at §3.1.1 and §3.6.6), so the shape real users feed this tool has no coverage at all.

PLAN §9.1 named the fix and called it load-bearing: `nondir_pe_1030.bam` is *"the only fixture covering all four strand indices. **Without it the 'whole encoding table' claim is false**, since a directional fixture holds only two."* It was dropped, and unlike §9.5/§9.8 it does **not** appear in §10b's "Not done — deliberately out of scope" list. That is the one gap the plan itself flagged in advance.

I ran the two PE fixtures the plan named. Both pass:

| fixture | records | result |
|---|---|---|
| `tests/data/dedup/nondir_pe_1030.bam` | 20 PE, all 4 `XR`/`XG` combos | 20/20 re-encoded, flip 0.000000, masked 0, **SAM body byte-identical** |
| `tests/data/dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` | 12974 PE, 42 with `I`/`D` | 12974/12974, flip 0.000000, masked 0, **SAM body byte-identical** |

**Recommendation:** parameterise `bisulfite_input_round_trips_to_identical_sam_text` over all three fixtures. Zero risk — I have run it. `nondir_pe_1030.bam` also delivers §9.8's `XG` ⟺ FLAG assertion almost for free, since it is the only fixture with all four FLAG/`XG` combinations.

*(Two side notes on §9.1's fixture list, which is still inaccurate in the same way rev 1's R2 fixed: `filter_nonconversion/{se_default,pe_default}/in.bam` and `se_unmapped/in.bam` **have no `MD` tag** and are rejected outright — I checked. So the §3.7 unmapped-pass-through **success** path, where some records convert and some copy through, has no fixture and no test; the only pass-through coverage is the uBAM case, which is all-unmapped and hits the D3 guard instead. Appending one unmapped record to the pUC19 fixture would close it.)*

### HIGH-3 · The load-bearing idempotence gate passes vacuously in CI

`aligner_five_base_bisulfite.rs:42-48` + `:77-80`: three tests — including the gate the file's own header calls *"the load-bearing one"* — do `if !samtools_available() { eprintln!("skipping…"); return; }`.

The main `cargo test` job in `.github/workflows/rust_ci.yml:18-46` installs **minimap2 only**. samtools is installed solely in the separate `perl-oracle` job (`:178-193`). So in CI today `bisulfite_input_round_trips_to_identical_sam_text`, `xm_is_left_untouched` and `appends_a_pg_with_a_distinct_id` **all take the skip branch and report green without asserting anything**.

The repo already solved this, for this same feature family, at the request of a prior review — `tests/aligner_five_base_groundtruth.rs:55-71`:

```rust
// Fail loud in CI: these real-aligner gates must not pass vacuously (#787 review).
if !present && std::env::var_os("CI").is_some() {
    panic!("minimap2 not found but $CI is set: … refusing to no-op.");
}
```

**Recommendation:** copy that guard into `samtools_available()`, and add samtools to the `test` job. Better still: `bisulfite_input_reports_a_zero_flip_rate` already covers the whole driver over the deletion and soft-clip records **without** samtools, which shows the comparison can be done in-process with noodles and the external dependency dropped entirely.

### MEDIUM-1 · A lower-case `MD` reference base silently produces wrong `NM`/`MD` **and a missed mask**

`parse_md:208-210` accepts `is_ascii_alphabetic()` and stores the byte verbatim, so `ref_seq` can hold `b'c'`. Every downstream comparison is case-sensitive: `hemming_dist` (`output.rs:150-157`), `make_mismatch_string` (`:196-200`), and the masking conjunct `reference.ref_seq[i] == ref_base` (`five_base_bisulfite.rs:476`). The round-trip proof cannot catch it — the recomputed `MD` reproduces the same lower-case byte, and `hemming_dist` counts a mismatch either way.

Measured:

| input | got | truth |
|---|---|---|
| `reencode(b"ATGT", b".Z..", Ct, "4M", "1c2", 1)` | `nm=1, md="1c2"` | `nm=0, md="4"` — flipping `T→C` makes it a **match** |
| `reencode(b"ACGT", b"....", Ct, "4M", "1c2", 1)` | **`masked=0`** | `masked=1` (uppercase `"1C2"` masks; see the unit test at `:747`) |

The second row is the one that matters: it is the §3.6 inversion leak surviving into the output, silently — the exact failure class the feature exists to remove.

**Reachability:** Bismark's own genome is uppercased (`aligner/genome.rs:124`), so a straight Bismark BAM is safe. But `samtools calmd` emits the reference base as it appears in the FASTA, and soft-masked assemblies (UCSC `hg38.fa`) are the norm — any pipeline step that regenerates `MD` against one produces lower-case letters at every repeat-region mismatch. The converter accepts third-party BAMs (it requires only `XM`/`XR`/`XG`/`MD`/`NM`), so this is reachable.

**Recommendation** (behaviour change, so not applied): reject rather than normalise, in keeping with the module's stated philosophy. In `parse_md`, after accepting an alphabetic byte:

```rust
if b[i].is_ascii_lowercase() {
    return Err(bad(format!(
        "lower-case reference base '{}' at offset {i}: the MD tag was written against a \
         soft-masked reference. Regenerate it against an upper-case FASTA.",
        b[i] as char
    )));
}
```

Normalising instead (uppercase + compare the round-trip case-insensitively) also works but silently changes the emitted `MD`'s case relative to the input, which weakens §9.1's SAM-text comparison for such inputs. Either way, add the two probe rows above as unit tests.

### MEDIUM-2 · `parse_md` accepts any ASCII letter as a reference base

Same site (`:201`, `:208`). `MD "1Q2"` yields `ref_seq[1] == b'Q'`, round-trips cleanly, and emits `md="1Q2"` with `nm=1`. Harmless in isolation but it is the same missing validation as MEDIUM-1 and shares the fix: restrict to `ACGTN` (uppercase). Reference `N` must stay allowed.

### MEDIUM-3 · A `samtools calmd`-regenerated Bismark BAM is rejected on every soft-clipped record, and the message points at the wrong cause

`calmd` recomputes `NM` to the *standard* definition, which excludes soft-clipped bases; Bismark's includes them (§3.4). Measured on the fixture: `ot_softclip` `NM:i:20 → 11`, `ob_softclip` `25 → 16` (`MD` unchanged in all 8 records). Feeding that back:

```
error: bisulfite: record ot_softclip: the reconstructed reference failed its round-trip proof
       (recomputed NM 20 != recorded NM 11). The output would be untrustworthy, so nothing was written.
```

Refusing is arguably right — the record's tags genuinely disagree with the converter's model. Two problems with *how*:

- **The message misdirects.** It blames the reconstruction, which is fine; the actual cause is a well-known and completely benign difference in `NM` convention. A user will go looking for a bug in Bismark.
- **This will be hit.** PLAN §9.1: `--five_base_umi_len` *depends* on soft-clipping the UMI prefix, which "makes `nS`-prefixed reads the dominant real 5-Base shape". So the *typical* real input is the one that trips on any `NM` regeneration. And the run aborts on the **first** offending record, discarding the rest of the file (with HIGH-1's partial output left behind).

**Recommendation:** when `nm_check != nm_old` **and** the CIGAR contains `S` **and** `nm_check - nm_old` equals the total soft-clipped length, say so:

> `recomputed NM {nm_check} != recorded NM {nm_old}; the difference is exactly the {n} soft-clipped bases. Bismark counts soft clips in NM and `samtools calmd` does not — this BAM's NM was regenerated by other tooling. Re-run the aligner, or drop NM/MD before converting.`

Cheap, and it turns a dead end into a diagnosis.

### MEDIUM-4 · `verdict` claims "output is unchanged" when `masked > 0`

`mod.rs:840-845` keys the verdict on `flipped` alone. On the synthesised masked input:

```
no-call cytosines masked to N	1
verdict	already bisulfite convention (input was NOT 5-Base) — output is unchanged
Note: 1 no-call cytosine(s) were masked to N. …
```

The verdict and the `Note:` two lines later contradict each other, and the verdict is the false one — a base *was* changed, and `NM`/`MD` with it. For a feature whose whole design principle is "never silent, never misleading", the summary line should not be the misleading part. Fix: make the `flipped == 0` arm read `"…— no calls re-encoded"`, or append `" (but {masked} base(s) masked to N)"` when `masked > 0`.

### LOW-1 · The round-trip proof is weaker than §3.4 claims — one concrete hole

The proof is `make_mismatch_string ∘ reconstruct_ref == id`, plus one genuinely independent constraint from `NM`. What it therefore does **not** pin is the *identity* of a mismatch's reference base, because that byte is read out of `MD_old` and written straight back:

```
reencode(b"ACGT", b"....", Ct, "4M", "1C2", 1)  -> ERR RoundTripFailed (recomputed NM 0 != 1)   ✅ caught
reencode(b"ACGT", b"....", Ct, "4M", "1G2", 1)  -> OK  md="1G2" masked=0                        ← unverifiable
```

If the truth were `1C2`, that position should have been masked. A stale or corrupt `MD`/`NM` pair from the same tool passes and silently changes the masking decision. This is an accepted consequence of §3.4/Open-5 ("no genome"), and it is what §9.5's independent oracle addressed for the *emit* direction — I built it and it passes (§1), so the residual exposure is only the *input* direction. **No code change recommended;** §3.4's "PROVE the reconstruction" and §11's "stronger guard than rev 1 had anywhere" just overstate it, and remaining-risk 3 ("`reconstruct_ref` is doubly load-bearing") deserves this sentence rather than the reassurance it currently carries.

### LOW-2 · Test gaps (all cheap, all now known-passing)

The 21 unit tests are genuinely good — they assert exact strings and exact counters, not shapes, and I could not construct a plausible bug that survives them. Specifically: dropping the positional conjunct breaks `masks_only_no_call_reference_cytosines` (`:747`, offset 3 is a `T` at reference `T`); swapping meth/unmeth breaks `five_base_input_flips_every_letter` (`:624`) and `ga_strand_uses_the_g_a_pair` (`:650`); a loop-level flip counter breaks `:754`; `ref_seq` not being `X` at `I` breaks `gaps_are_never_written_insertion` (`:718`). The three test-data errors from §10b's iteration log are correctly fixed — I re-derived all three by hand.

Missing, in rough value order:

| # | gap | note |
|---|---|---|
| a | **No `reencode` test with a `D` in the CIGAR.** `:560` covers `reconstruct_ref` only. The path is covered by the fixture, but only via the driver | My two oracle cases (§1) fill this and are stronger than anything else available |
| b | **The `MD` half of the round-trip proof is untested.** `:820` asserts `what.contains("NM")` only | `reencode(b"ACGT", b"....", Ct, "4M", "8", 0)` → `RoundTripFailed` on `MD` |
| c | `MalformedMd` "MD ran out of aligned bases" untested | `("8M", "1C0C4")` — an `MD` implying 7 aligned bases |
| d | §9.7.5's explicit disjointness assertion absent | `gaps_are_never_written_soft_clip` (`:695`) *is* such a record (letters 1, masked 1) — it just never asserts `flipped`/`letters` |
| e | §9.7.4's OB-symmetric neighbour case (reference `C` at `j-1` on a `GA` record) absent | the OT direction is covered implicitly at `:747` |
| f | §9.3 asks for the asymmetric trap on a **reverse-strand** record; `asymmetric_calls_edit_the_correct_positions` (`:671`) uses `XgStrand::Ct` | the reversal guard is still genuine in both `:650` and `:671` — I checked that a 5'-indexed write yields a detectably different string in each — so this is naming, not coverage |

### LOW-3 · Driver niggles

- **Same-stem inputs from different directories silently overwrite each other.** `mod.rs:706-713` derives the output name from `file_stem()` only, so `--five_base_bisulfite_bam a/x.bam b/x.bam` writes `x.bisulfite.bam` twice, the second clobbering the first, with the report following it. §10 Open-2 considered collisions but only for a fixed output name. Cheap guard: track emitted paths per run and refuse a repeat.
- **The suggested sort command produces `<name>.bam.sorted.bam`** (`mod.rs:879`, `{0}` is the full output path including `.bam`). PLAN §5.8 wrote `<out>.sorted.bam`. Functional, ugly, and it is the command users will paste.
- **The appended `@PG` has no `PN:`.** Optional in SAM, but every other `@PG` in the file has one, so `samtools` output looks inconsistent.
- `cigar_to_string` (`mod.rs:604-624`) is correct and total over noodles' `Kind`; mapping `H`/`P`/`=`/`X` to their letters so `reconstruct_ref` can name them in `UnsupportedCigarOp` is the right call. Verified.

---

## 3. Things I checked that are fine

Worth recording so a later reader does not redo them.

- **`md_seq` orientation.** The new module builds it genome-forward (M+D bases, `X` at `I`/`S`, nothing at `N`), matching `methylation.rs:168-190` exactly, including the `contains_deletion` gate being irrelevant (`make_mismatch_string` only reads `md_sequence` when the CIGAR has `D`). The extraction revcomp (`methylation.rs:216-222`) and the output revcomp (`output.rs:449-454` SE, `:684-688` PE) cancel because `methylation::reverse_complement` and `output::revcomp` agree on `{A,C,G,T,X,N}` and each is an involution there. Confirmed empirically by `ob_del` (`XG:GA`, `35M4D45M`) round-tripping byte-identically.
- **`md_seq` indexing vs `del_pos`.** `rebuild_md_with_deletions` advances `del_pos` over `M`, `I`/`S` and `D` but not `N` (`output.rs:247-256`); the new module's `md_seq` layout matches that cumulative index. The `N`+`D`-together mis-indexing is a pre-existing aligner quirk faithfully mirrored, not introduced here.
- **No indexing panic is reachable.** `ref_seq.len() == read_pos`, and `:343-351` rejects `read_pos != seq.len()`, so `reference.ref_seq[i]` at `:448` and `:476` is always in bounds.
- **`Match(0)` tokens dropped by `parse_md:195`** is safe: a zero run contributes nothing, and `make_mismatch_string` always re-emits the trailing count. `MD "4C0"`, `"1C0C5"`, `"1C0C0C4"` all round-trip.
- **`MD` longer/shorter than the CIGAR implies** is always caught — short by `MalformedMd`, long by the `MD` half of the round-trip proof. Checked `("4M","8")`, `("4M","4C3")`, `("4M2S","6")`, `("8M","4^AC4")`, `("4M2D4M","4^A4")`, and a `Match` run left pending at end-of-record.
- **Overflow.** `MD "99999999999999999999999"` gives a clean `MalformedMd`, no panic.
- **Idempotence on its own output** (§5.6's claim) holds, and for the reason stated: letters are no longer `.`, masked bases are `N ∉ {meth, unmeth}`, `MD` still describes the true reference so the round-trip still passes, and re-inserting `@PG ID:bismark-five-base-bisulfite` into an `IndexMap` replaces rather than duplicates.
- **The file-level flip-rate guard is equivalent to the per-record invariant.** No record can have `flipped > letters`, so `Σflipped == Σletters` ⟺ every record flipped fully, and `Σflipped == 0` ⟺ none did. The aggregate is sound, not a weakening.
- **`write_raw_record`** (`io/write.rs:86-89`) is exactly the right tool — it bypasses `BismarkRecord` validation, which is what the pass-through needs.
- **Header vs records.** Records are decoded against `in_header` while `header` (with the extra `@PG`) is written; reference-sequence IDs index `@SQ`, which is untouched. Safe.
- **`XM` is genuinely never modified** — asserted by `xm_is_left_untouched`, and I confirmed byte-identical `XM` on all five fixtures I ran.
- D2's `@PG`-chain note is accurate: noodles does re-link `PP` on serialisation. Metadata only.

---

## 4. Fixes applied

One, comment-only, in `rust/bismark/src/aligner/five_base_bisulfite.rs`:

- Removed a stale two-line duplicate doc comment above `asymmetric_calls_edit_the_correct_positions` (was `:668-671`). The first sentence described "a reverse-strand-shaped record", which the test is not (`XgStrand::Ct`) — a leftover from §10b iteration `#2`'s rewrite, immediately followed by the correct sentence.

Verified after the edit: `cargo fmt -p bismark -- --check` clean; `cargo test -p bismark --lib five_base_bisulfite` **21 passed**; `cargo test -p bismark --test aligner_five_base_bisulfite` **7 passed**.

I deliberately did **not** touch `mod.rs`: another agent is editing it concurrently (I observed uncommitted error-message improvements to `create_dir_all`, `read_header` and the report write appear mid-review) and every driver finding above has more than one defensible resolution.

---

## 5. Priorities

| # | Priority | Item |
|---|---|---|
| HIGH-1 | **High** | Error paths leave a valid-looking incomplete `.bisulfite.bam`; the flip-rate message claims the opposite. Temp-file + rename, or `remove_file` on all three returns |
| HIGH-2 | **High** | Add `nondir_pe_1030.bam` + `synth_barcode_10k_…_pe.bam` to the idempotence gate. PE-only feature, SE-only tests, 2 of 4 strand indices. Both verified passing |
| HIGH-3 | **High** | The gate skips silently without samtools; CI's `test` job has no samtools. Copy the `$CI` panic from `aligner_five_base_groundtruth.rs:55-71`, or drop samtools and compare in-process |
| MED-1 | Medium | Reject lower-case `MD` bases — currently wrong `NM`/`MD` **and a missed mask**, silently |
| MED-2 | Medium | Restrict `MD` bases to `ACGTN` |
| MED-3 | Medium | Name the soft-clip `NM`-convention difference in the `RoundTripFailed` message; `calmd` output is rejected on the dominant real 5-Base shape |
| MED-4 | Medium | `verdict` must not say "output is unchanged" when `masked > 0` |
| LOW-1 | Low | Soften §3.4/§11's claims about the round-trip proof; record the substituted-`MD`-base hole |
| LOW-2 | Low | Commit my two oracle cases as unit tests (§9.5); add the `MD`-half round-trip, `MalformedMd`, and disjointness assertions |
| LOW-3 | Low | Same-stem output collision; `.bam.sorted.bam`; missing `PN:` |

**Verdict: APPROVE with follow-ups.** The algorithm — which is the part that could have been quietly wrong for years — is sound, and the end-to-end `patter` result in §10b is real evidence rather than a shape assertion. HIGH-1 and HIGH-3 should be fixed before this is relied on; HIGH-2 is a five-line change against fixtures I have already run.
