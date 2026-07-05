# PLAN REVIEW A — Phase 5 (Genomic-seq + XM/XR/XG + SAM/BAM, SE directional)

**Reviewer:** A (independent, fresh context)
**Plan:** `phase5-genomic-seq-xm-sam-output/PLAN.md` (rev 0, 2026-06-01)
**Verdict:** Strong, well-source-grounded plan. The hard parts (the three counters, the two edge
guards, the dual complement helpers, the `make_mismatch_string` verbatim port, the P1 `@PG` policy)
are all correctly identified and correctly placed against the Perl. I found **no Critical logic
errors** that would silently produce wrong output if the plan is implemented as written. I did find
**two Important gaps** (a missing dependency / under-specified `@SQ`-order reuse, and an
under-specified `@PG`-field-ordering byte-identity risk in noodles) and several Optional
clarifications. Details below, all cross-checked against the Perl at the cited line numbers.

---

## 1. Logic review

I verified the plan's control flow against `bismark` v0.25.1 line by line.

### Confirmed correct

- **Call-site span (3120–3147).** `unique_best++` at 3121 (Phase 4), `extract_…` at 3124, the
  length guard at 3127–3131 (`!= length($sequence)+2` → warn + `genomic_sequence_could_not_be_extracted_count++`
  + `return 0` = skip), `calc_mapq` recompute at 3134, `methylation_call` at 3144,
  `print_bisulfite_mapping_result_single_end` at 3147. The plan's §3.2(b)–(e) ordering matches.
  ✔
- **The three counters in three places.** Confirmed: `unique_best_alignment_count` at 3121 (Phase
  4); the per-strand `CT_CT/CT_GA/GA_CT/GA_GA` at 4402/4411/4426/4441 — and they are reached **only
  after** both edge guards (4317 and 4390) have NOT fired (the guards `return` before line 4400);
  `genomic_sequence_could_not_be_extracted_count` at 3129 after the 3127 length guard. The plan's
  §3.2(a) "per-strand counter … reached only when neither edge guard fired" is exactly right, and
  §11 self-review restates it correctly. ✔ This was the headline Phase-4 rev-1 correction and the
  plan honours it.
- **The +2 padding and its two edge guards.** Index ∈ {1,3} prepends `substr(chr,pos-2,2)` guarded
  by `($pos-2) >= 0` (4317–4322); index ∈ {0,2} appends `substr(chr,pos,2)` guarded by
  `length(chr) >= pos+2` (4390–4395). On guard failure Perl stores the **partial**
  `unmodified_genomic_sequence` and returns — which then fails the 3127 length test. Plan §3.2(a)
  captures both guards and the early return. ✔
- **CIGAR walk ops.** `M` append + `pos+=len` (+MD-seq iff deletion); `I`/`S` → `'X' x len`, no
  pos change; `D` → `pos+=len`, `indels+=len`, MD-seq append iff deletion; `N` → `pos+=len` only;
  `H/P/X/=` or other → die (4379–4384). Plan §3.2(a) matches, including the **`indels` only counts
  `D`** subtlety (insertions/soft-clips deliberately do NOT add to `indels` — Perl 4347/4360
  comments; the padding `X` fails `hemming_dist` instead). ✔
- **Two complement helpers are genuinely different.** `reverse_complement` (5161): `tr/CATG/GTAC/`
  **then** `reverse`. `revcomp` (9228): `reverse` **then** `tr/ACTGactg/TGACTGAC/`. They differ on
  lower-case and on order; the plan flags "port each verbatim." For upper-case `ACGT` they coincide;
  `N` is left unchanged by both (not in either `tr` set). Genome is uppercased at load (5103), so
  in the SE-directional spine the inputs to `reverse_complement` are upper-case `ACGTN` + the
  padding base from genome → safe either way, but porting verbatim is the right call. ✔
- **Minus-strand reorientation (8577–8584).** `revcomp(actual_seq)`, `revcomp(ref_seq)`,
  `reverse($qual)`, and **only if CIGAR has `D`** `revcomp(genomic_seq_for_md_tag)`. XM reversed at
  8602–8607. Plan §3.3 matches. ✔
- **FLAG table (8521–8546).** `+/CT/CT→0`, `+/GA/GA→16`, `-/CT/GA→16`, `-/GA/CT→0`, else die. For
  SE-directional only index 0 (`+/CT/CT→0`) and index 1 (`-/CT/GA→16`) occur. Plan §3.3 matches. ✔
- **ref-seq trim (8570–8575).** `read_conversion eq 'CT'` → drop **last** 2 (`substr 0,len-2`);
  else drop **first** 2 (`substr 2,len-2`). Plan §3.3 matches. ✔
- **NM = hemming_dist + indels (8588–8592).** `hemming_dist` (9235) counts base-by-base inequality
  over `actual_seq` vs `ref_seq` (the **trimmed, possibly-revcomp'd** sequences), then `+= indels`.
  Padding `X` bases mismatch and are intentionally counted in `hemming_dist` (then partially netted
  by the comment at 4346 — but for NM Perl genuinely counts them). Plan §3.3 captures this. ✔
- **Default tag set + order (8706).** `NM:i, MD:Z, XM:Z, XR:Z, XG:Z`, no `XA`. `XA` is in the
  `$non_bs_mm` branch only (8694/8697); `RG:Z` is `$rg_tag`-only. Plan §3.3 + §8 match. ✔
- **Genome load once (273–277).** Guarded `unless (%chromosomes)`. Plan §3.1 + §5.5 "load once
  before the file loop." ✔
- **Default output = BAM.** Traced 7544 (`$bam=1` "Output format is BAM (default)") → 7580–7584
  (samtools found in PATH → `$bam=1`) → 1604 (`$bam==1` → `samtools view -bSh`). Plan §2 "Default
  output = BAM" and the output-name derivation (1562–1607: strip `(\.fastq\.gz|\.fq\.gz|\.fastq|\.fq)$`,
  `_bismark_bt2.sam` → `s/sam$/bam/`, `--basename`→`${basename}.bam`, `--prefix`→`$prefix.$name`)
  are correct. ✔ (Note: the `$bam=0 unless defined` at 1582 is a fallback only reached when the
  7544/7576/7584 path never set it — in the default samtools-present path it does. The plan need
  not handle the no-samtools `.sam.gz` (bam==2) or uncompressed (bam==0) fallbacks since those are
  out-of-default; this is implicitly covered by §8 "default output mode (BAM)".)

### Gaps / under-specifications (logic)

- **L-1 (Important): the index∈{1,3} prepend does NOT touch `genomic_seq_for_MD_tag`.** Perl 4322
  appends the two prepended bases to `$non_bisulfite_sequence` **only** — `$genomic_seq_for_MD_tag`
  is left alone (it only ever receives bases inside the `M`/`D` arms, gated on `$contains_deletion`).
  Plan §3.2(a) writes "prepend `chr[pos-2 .. pos]`" with no qualifier, so a literal reading is
  correct, but the contrast with the `M`-arm (which *does* append to MD-seq when `contains_deletion`)
  is exactly the kind of thing an implementer mirrors by reflex. **Add one sentence**: "the +2
  prepend/append bases are added to `unmodified_genomic_sequence` only, never to
  `genomic_seq_for_md_tag`." This matters for the deletion + minus-strand MD path, which is the
  single most error-prone code in the port.

- **L-2 (Optional): `methylation_call` length `warn` (4822) is non-fatal.** Perl `warn`s (does not
  die) when `scalar @seq != scalar @genomic - 2`. After the 3127 length guard this can only happen
  for indel reads where the read length and genomic length legitimately differ by something other
  than 2 — but Perl proceeds regardless (the loop runs `0..$#seq`, indexing `@genomic[$index+1/+2]`,
  which can read past the end → Perl returns `undef`/empty string for out-of-range, treated as
  non-G/non-C). The plan says "compare base-by-base"; an implementer must **not** assert/panic on a
  length mismatch and must tolerate genomic-context lookups at `index+1`/`index+2` running off the
  end (Perl yields `''`, i.e. "not G, not N/X" → falls through to CHH/`H`/`h`, or `.`). Worth one
  line in §3.2(c): "context-base reads past end-of-genomic-window behave as Perl's out-of-range
  `substr`/array access (empty, i.e. neither G nor N/X)." This is a genuine silent-divergence trap
  for short indel reads at a sequence end.

- **L-3 (Optional): `methylation_call` accumulates `total_meCHH/meCHG` too.** Plan §3.2(c)/§4 list
  8 counters but the field names in §4 only spell out `cpg/chg/chh/c_unknown` for me + unme — which
  is the correct 8. Perl 5006–5013 confirms exactly these 8 (`total_meCHH/meCHG/meCpG/meC_unknown`
  + the 4 `total_unmethylated_*`). ✔ on the count; just confirm the CHG/CHH ones aren't dropped
  (the §4 struct does list `total_me_chg/total_me_chh` — good).

- **L-4 (Optional): `bowtie_sequence` carried but unused.** Plan §2 correctly states it's not an
  output field (SEQ + methylation call use the original uc read). Confirmed: Perl uses `$actual_seq`
  (the original read, passed in) everywhere in `single_end_SAM_output`; `bowtie_sequence` only
  appears in dead `# print` debug lines (4396, 4456). The `BestAlignment.bowtie_sequence` field can
  stay (Phase 4 already populates it) but Phase 5 should never read it. Fine as-is; no action.

---

## 2. Assumptions

- **A-1 (Important): `@SQ` order — "reuse the genome-prep discovery sort" is not wired.** The plan
  §3.1 + §5.2 say to use "the same case-insensitive sort + matching as
  `bismark-genome-preparation::discovery`," but **`bismark-aligner/Cargo.toml` does not depend on
  `bismark-genome-preparation`**, and §5.1's dep list does not add it. So "reuse" today means either
  (a) add `bismark-genome-preparation` as a path dependency (its `discovery` module is `pub`, and
  `find_fasta_files` + `fasta_name_cmp` + `extract_chromosome_name` are all `pub fn`), or (b)
  re-implement the same logic in `genome.rs`. **The plan must pick one and say so**, because this is
  the byte-identity-critical `@SQ` ordering.
  - I verified the equivalence the plan relies on: Perl `read_genome_into_memory` line 5031 uses
    `<*.fa>` (csh_glob, case-folding on Linux+macOS) — the **same** glob `bismark-genome-preparation`
    reproduces in `fasta_name_cmp` (case-insensitive lower-fold + raw-byte tiebreak, "verified on
    Linux CI"). The extension fallback chain (`.fa`→`.fa.gz`→`.fasta`→`.fasta.gz`, first non-empty
    wins) matches `EXT_GROUPS` exactly. So sharing the genome-prep code is correct **and** is the
    safest way to keep the two ports' `@SQ` orderings from drifting. I'd recommend (a) explicitly.
  - **A subtle non-equivalence to call out:** genome-prep's `extract_chromosome_name`
    (discovery.rs:97) returns the **leading-empty-field** semantics of Perl `split /\s+/` — a bare
    `>` or leading-space header yields `""` and is **not** an error at extraction time. But the
    aligner's Perl (`read_genome_into_memory` 5069/5098) **dies** when the name is empty
    ("Chromosome names must not be empty…"). So if Phase 5 reuses genome-prep's
    `extract_chromosome_name`, it must add the **empty-name → die** check on top (the genome-prep
    crate does NOT die — it's the *aligner's* loader that does). Plan §3.1 step 2 mentions "Empty
    name … → die (5070)" so the intent is there; just make sure the reused helper's non-dying
    behaviour is wrapped with the aligner's die. Validation #2 tests this.

- **A-2 (Important): noodles `@PG` field ordering for `CL:`.** Plan §8 says "noodles preserves
  `Data` insertion order and `samtools view -h` renders integer tags as `:i:` — verify (§9 #11)."
  That covers the **record** tags. But the **header `@PG` line** is a separate serialization path:
  Bismark writes `@PG\tID:Bismark\tVN:v0.25.1\tCL:"bismark <argv>"`. In noodles a `Map<Program>`
  serializes `ID` first, then typed fields, then `other_fields` in insertion order. `VN` is a typed
  Program field; `CL` lives in `other_fields` (see `bismark-io` read.rs:1137–1143, which sets `CL`
  via `program::tag::COMMAND_LINE` into `other_fields_mut`). To get byte-identical
  `ID:Bismark\tVN:v0.25.1\tCL:"…"` you must set **VN via the typed version field** (so it lands
  before `other_fields`) and `CL` via `other_fields`. If `VN` is instead stuffed into `other_fields`
  after `CL`, the order flips and the gate fails on the (un-normalized) Bismark `@PG` line. The plan
  treats §0/P1 as filtering only the **samtools** `@PG`; the **Bismark** `@PG` is still gated
  byte-for-byte (§0 "gate Bismark's own `@PG` after dropping only the samtools line"). So this
  ordering is load-bearing. **Add an explicit sub-point to validation #12**: assert the exact
  `@PG\tID:Bismark\tVN:v0.25.1\tCL:"bismark <argv>"` byte string out of `samtools view -H`, and
  decide the VN-typed-vs-other_fields placement in §5.4. (Also: confirm the embedded double-quotes
  in `CL:"…"` round-trip through noodles unescaped — SAM has no quoting rules, so they should pass
  verbatim, but it's worth a #12 assertion.)

- **A-3 (validated): `best.mapq` reuse is safe.** Perl recomputes `mapq` at 3134 *after* the 3127
  length guard, but the inputs (`length($sequence)`, `alignment_score`, `alignment_score_second_best`)
  are all fixed at merge time and identical to what Phase 4 used. Reads that fail the guard are never
  written, so the pre-computed value is unused for them. Plan §2 + §8 are correct. ✔

- **A-4 (validated): default tags = `NM MD XM XR XG`, no `XA`.** Confirmed at 8706; `XA` is
  `$non_bs_mm`-only (8694/8697). ✔

- **A-5 (validated): QUAL ASCII↔phred.** Perl writes the ASCII QUAL string straight into the SAM
  text column; `single_end_SAM_output` does not numerically convert (only `--phred64` pre-converts
  the ASCII at 4191–4193). For BAM via noodles you must store phred scores (ASCII−33) in
  `quality_scores_mut()`; `samtools view -h` re-renders ASCII+33. The minus-strand `reverse` of the
  qual string happens before/independent of that numeric conversion (order is irrelevant to the byte
  result, as the plan notes). Plan §3.3 + edge-cases + §8 are correct. ✔ One caveat to verify in
  #11: a phred score of 0 = ASCII `!` (33) round-trips; and noodles must accept the full 0–93 range
  without clamping. The `synth_record_buf` test in `write.rs` uses `vec![30u8; 5]` so the phred-byte
  path is exercised, but not the ASCII→phred→ASCII *value preservation* across the real range — #11
  should diff actual QUAL bytes.

- **A-6 (validated): `--phred64` ported but inert.** Plan §3.3 ports `convert_phred64_quals_to_phred33`
  (4191/4218); v1 default phred33 means it's a no-op. Confirmed Perl applies it in
  `print_bisulfite_mapping_result_single_end` at 4191 **before** `single_end_SAM_output`, so the
  reversal at 8583 sees already-phred33 ASCII. Plan §3.2/§3.3 ordering (phred64-convert then
  assemble) matches. ✔

---

## 3. Efficiency

No concerns. Linear in reads × read-length for the per-base methylation call and the MD walk; the
genome is held once as `Vec<u8>` per chromosome (same footprint as Perl's `%chromosomes`); `refid`
is a small `HashMap`. The plan's §6 "pre-size to read_len+2" is reasonable. Two minor notes:

- **E-1 (Optional):** the per-read `format!`/`String` building for MD/XM and the `Vec<u8>`
  genomic window are unavoidable; pre-sizing helps but isn't gate-relevant. The dominant cost is
  Bowtie 2's anyway (§6 correct).
- **E-2 (Optional):** `refid` is typed `HashMap<&str, usize>` in the §4 signature for
  `single_end_sam_output`. Borrowing `&str` keys tied to `Genome.sq_order`/`chromosomes` lifetimes
  is fine but will entangle lifetimes through the call; an owned `HashMap<String, usize>` (built once)
  or storing `reference_sequence_id` resolution inside the record assembly is simpler. Inconsequential
  to bytes; flag only as ergonomics.

---

## 4. Validation sufficiency

The 14-row matrix is well-targeted. It covers each index path, both edge guards, the length guard,
each CIGAR op, all four context-call classes (Z/z X/x H/h U/u .), the MD builder's hard cases, the
plus/minus assembly, the header, and two integration gates (#11 round-trip, #13 hermetic e2e) plus
the #14 Linux byte-identity gate.

**The load-bearing one is #11** (noodles→BAM→`samtools view -h` fidelity), correctly identified.
I'd strengthen it and #9/#12:

- **V-1 (Important): #11/#12 must pin the `@PG` line bytes**, per A-2 — including the `VN:v0.25.1`
  position relative to `CL:` and the literal embedded quotes. As written, #12 says "Bismark `@PG
  CL:"bismark <argv>"` exact" but doesn't call out the field *order* risk, which is where noodles
  can silently diverge.

- **V-2 (Important): #9 (`make_mismatch_string`) needs a multi-deletion case.** The Perl deletion
  path (9325–9591) has a whole sub-state-machine for **>1 deletion in one read**
  (`$md_index_already_processed`, the `this_deletion_processed` short-circuit, the `@md`
  reconstitution at 9581, the "last element was a digit" tail at 9526–9578). The plan §9 #9 lists
  "1-deletion (`^`)" but not **2+ deletions**, which is precisely the code most likely to be
  mis-ported. Add a `…5M2D10M3D…`-style two-deletion MD case (and ideally a deletion-at-end and a
  deletion-adjacent-to-mismatch case). This is the single highest silent-wrong-bytes risk in the
  phase and one golden 1-deletion test will not exercise the reconstitution branch.

- **V-3 (Optional): add an N-in-genome (`U`/`u`) read that also has the context base be the
  *padding* `X`.** #8 covers N/X context generally; but the specific case where the +2 padding base
  (or an insertion `X`) is the downstream/second-downstream context — i.e. a C at the **last** read
  position whose CpG context comes from the appended padding — exercises the `U`/`u` path via `X`
  (4844/4856). Worth one targeted case so the padding-as-context path is pinned.

- **V-4 (Optional): #6 (length guard) "record not written"** — assert the writer received **zero**
  records for an all-edge-read input (e.g. via a counting writer double), not just that the counter
  incremented. The "skip" is the behaviour that must hold.

- **V-5 (Optional): the multi-deletion `genomic_seq_for_md_tag` + minus strand.** When a `-`-strand
  read (index 1) has a deletion, Perl revcomps `genomic_seq_for_md_tag` (4419) **and again** at 8581.
  Wait — 4419 revcomps it inside extraction (index 1), then 8581 revcomps it **again** (because
  `strand eq '-'` and CIGAR has D). That's a **double** revcomp of the MD-seq for minus-strand
  deletion reads. I confirmed both sites fire for index 1 + deletion: 4418–4420 (in extraction) and
  8580–8582 (in output). **This is a real, easily-missed interaction** — the implementer must apply
  *both* revcomps, not collapse them. Plan §3.2(a) mentions the extraction revcomp and §3.3 mentions
  the output revcomp, but never flags that for a minus-strand deletion read they **compose** (net
  effect: MD-seq ends up in its original extraction orientation, since revcomp∘revcomp = identity on
  the relevant alphabet). Whether the net is identity depends on the helper used — and both 4419 and
  8581 use **`reverse_complement`** vs **`revcomp`** respectively (4419 = `reverse_complement` 5161;
  8581 = `revcomp` 9228). On upper-case `ACGTN` they're equal, so the net is identity — but only
  because the genome is uppercased. **Add a §9 case: minus-strand read (index 1) WITH a deletion**,
  asserting the final MD tag, since this is the only path that hits the double-revcomp + the MD
  deletion reconstitution together. Currently no validation row combines index-1 + deletion.

Overall #11/#13/#14 give good silent-wrong-result coverage **provided** V-2 and V-5 are added; the
deletion machinery is the place a "green unit suite + passing tiny e2e" could still miss bytes,
because a tiny hand-built WGBS gate may contain no multi-deletion or minus-strand-deletion reads.

---

## 5. Alternatives

- **Write BAM via noodles (plan) vs emit SAM text directly.** The plan's choice (BamWriter, with
  #11 de-risking the round-trip and a "emit SAM text" contingency in §10) is the right one — it's
  the project's standing noodles decision, default Bismark output is BAM, and the gate is on
  **decompressed** content anyway so the BGZF encoder difference is already normalized out. The
  contingency is correctly flagged as a risk, not the plan. One refinement: since the gate runs
  `samtools view -h` regardless, the **SAM-text-direct** path would actually be *simpler* to make
  byte-identical (no noodles tag-serialization questions at all — you control every byte). It's
  worth keeping as a genuinely-cheap fallback, not just a "contingency," if #11 surfaces any
  `:i:`/tag-order/QUAL surprise. No change required, but I'd phrase §10's third bullet as "low-cost
  fallback" rather than "risk."

- **`Conversion` enum vs `&str`.** Plan's enum→`"CT"`/`"GA"` is cleaner and inconsequential to
  bytes. Agree. ✔

- **`reference_sequence_id` resolution.** Minor: rather than threading a `refid: &HashMap<&str,usize>`
  into `single_end_sam_output` (§4), the record assembly could take the resolved `tid: usize`
  directly (resolved by the driver from `sq_order`), keeping `output.rs` free of the genome
  lifetime. Ergonomic only.

---

## 6. Action items (prioritized)

### Critical
*(none — no defect that silently produces wrong output if the plan is implemented as written and
the deletion machinery is ported verbatim.)*

### Important
1. **A-1 / §5.1, §3.1, §5.2 — wire the `@SQ`-order source.** Decide and state: add
   `bismark-genome-preparation` as a path dependency and reuse `discovery::find_fasta_files` /
   `fasta_name_cmp` / `extract_chromosome_name`, **or** re-implement them in `genome.rs`. Either
   way, the aligner loader must add the **empty-name→die** check (Perl 5069/5098) on top of
   genome-prep's non-dying `extract_chromosome_name`. This is the byte-identity-critical `@SQ`
   ordering and it is currently unwired.
2. **A-2 / V-1 — pin the Bismark `@PG` line bytes and field order.** Set `VN` via noodles' typed
   Program version field and `CL` via `other_fields` (insertion order), and extend validation #12
   to assert the exact `@PG\tID:Bismark\tVN:v0.25.1\tCL:"bismark <argv>"` out of `samtools view -H`
   (incl. the embedded quotes). The samtools `@PG` is normalized out (P1); the **Bismark** `@PG` is
   still gated, so its field order is load-bearing.
3. **V-2 — add a multi-deletion (2+ `D`) `make_mismatch_string` case to #9.** The Perl deletion
   sub-state-machine (9325–9591) only fully runs with >1 deletion; a single 1-deletion golden does
   not exercise the `@md` reconstitution / `md_index_already_processed` branch — the highest
   silent-wrong-bytes risk in the phase.
4. **V-5 — add a minus-strand (index 1) + deletion case.** This is the only path that composes the
   **double revcomp** of `genomic_seq_for_md_tag` (extraction 4419 `reverse_complement` + output
   8581 `revcomp`) with the MD deletion reconstitution. No current validation row combines index-1
   with a deletion; the double-revcomp is easy to drop or duplicate.
5. **L-1 — state the prepend/append MD-seq omission.** Add one sentence to §3.2(a): the +2
   prepend (4322) / append (4395) bases go into `unmodified_genomic_sequence` **only**, never into
   `genomic_seq_for_md_tag`. Prevents a reflexive mirror of the `M`-arm's MD-seq append.

### Optional
6. **L-2 — note `methylation_call`'s non-fatal length `warn` (4822) + out-of-range context reads.**
   Don't panic on `@seq != @genomic-2`; context lookups past the window end behave as Perl's empty
   `substr` (neither G nor N/X). Matters for short indel reads at a sequence end.
7. **V-3 — add a `U`/`u` case where the context base is the padding `X`** (C at the last read
   position; CpG context from the appended +2 padding).
8. **V-4 — assert zero records written for an all-edge-read input** (counting writer double), not
   just the counter bump.
9. **§10 wording — reframe the SAM-text-direct path as a low-cost fallback**, since the gate runs on
   `samtools view -h` and a text emitter sidesteps every noodles tag-serialization question.
10. **E-2 / §4 ergonomics — consider passing a resolved `tid: usize`** into `single_end_sam_output`
    rather than `&HashMap<&str,usize>`, to keep `output.rs` free of the genome's lifetime.
11. **§0 / P1 consistency check — confirm the gate filters the samtools `@PG` from BOTH sides.**
    Plan §0 + §9 #14 say "(both filtered per §0)" — good; just make the filter a single shared
    helper used by #11, #13, and #14 so the three gates can't drift on what they strip.

---

## 7. Notes on what I verified directly in the Perl

- `check_results_single_end` 3110–3151: directional reject (3112), `unique_best++` (3121),
  `extract_…` (3124), length guard (3127–3131), `calc_mapq` (3134), `methylation_call` (3144),
  print (3147).
- `extract_corresponding_genomic_sequence_single_end` 4273–4467: pos−1 (4300), CIGAR split
  (4303–4306), pbat modifier (4308–4312), index{1,3} prepend + 4317 guard (4314–4323), CIGAR walk
  (4327–4385) incl. illegal-op die (4379–4384), index{0,2} append + 4390 guard (4387–4397),
  per-strand counters + strand/conv assignment + index-1/2 `reverse_complement` (4399–4448),
  end_position/indels store (4464–4466). **Confirmed the +2 bases are NOT added to MD-seq.**
- `methylation_call` 4800–5018: CT branch (4832–4912), GA branch (4913–4998), non-fatal length
  warn (4822), 8-counter accumulation (5006–5013).
- `read_genome_into_memory` 5022–5147: `<*.fa>`→`.fa.gz`→`.fasta`→`.fasta.gz` fallback (5031–5046),
  empty die (5048), per-file gunzip (5056), header → `extract_chromosome_name` + empty-name die
  (5064–5071), chomp+`\r`-strip+`uc`+concat (5074–5104), duplicate-name die (5080/5109),
  empty-seq **warn** (5085/5114), `++$SQ_count; $SQ_order{$SQ_count}=name` (5091–5092 / 5118–5122),
  CRAM-ref reconstitution out-of-scope (5128–5143, and note it iterates `keys %chromosomes` =
  hash order, not sq_order — irrelevant since CRAM is skipped).
- `extract_chromosome_name` 5149–5159; `reverse_complement` 5161–5166; `revcomp` 9228–9233;
  `hemming_dist` 9235–9244.
- `generate_SAM_header` 8452–8484: `@HD VN:1.0 SO:unsorted` (8454), `@SQ` in `sort {$a<=>$b} keys
  %SQ_order` (8466–8473) = numeric SQ_count order = insertion order, `@PG ID:Bismark
  VN:$bismark_version CL:"bismark $command_line"` (8480). `--sam_no_hd` (1732, skips header).
- `single_end_SAM_output` 8489–8711: FLAG (8521–8546), ref trim (8570–8575), minus revcomp+qual
  reverse+MD-seq revcomp (8577–8584), NM (8588–8592), MD (8596), XM (8601–8607), XR/XG (8611–8615),
  default column join (8706).
- `make_mismatch_string` 9252–9595: match-run/mismatch builder with `X`-padding skip (9295),
  leading/adjacent-mismatch `0`-padding (9302–9312), and the full **multi-deletion** `^<bases>`
  reconstitution state-machine (9325–9591).
- Output naming + `$bam` default: 1548–1616, 7544–7593 (default BAM via samtools-in-PATH).
- `bismark-io`: `BamWriter`/`finish()` (`#[must_use]`, BGZF EOF) write.rs:38–88; `BismarkRecord::
  from_noodles_record` re-validates XR/XG present + `XM.len()==seq.len()` record.rs:116–141;
  `@PG` `CL` via `program::tag::COMMAND_LINE` into `other_fields` read.rs:1137–1143; noodles pins
  `noodles-sam=0.85.0`/`noodles-bam=0.89.0`/`noodles-core=0.20.0`.
- `bismark-genome-preparation::discovery`: `find_fasta_files` + `fasta_name_cmp` (case-insensitive,
  Linux-CI-verified) + `extract_chromosome_name` (leading-empty-field, non-dying) discovery.rs:40–110.
