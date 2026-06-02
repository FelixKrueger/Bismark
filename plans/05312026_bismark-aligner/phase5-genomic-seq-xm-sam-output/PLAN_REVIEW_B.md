# PLAN_REVIEW_B — Phase 5 (Genomic-seq + XM/XR/XG + SAM/BAM, SE directional)

**Reviewer:** B (independent, fresh context)
**Plan reviewed:** `phase5-genomic-seq-xm-sam-output/PLAN.md` (rev 0, 2026-06-01)
**Verified against:** Perl `bismark` v0.25.1 (`./bismark`), Phase-4 `merge.rs`/`lib.rs`/`config.rs`/`discovery.rs`,
`bismark-io` `record.rs`/`write.rs`/`tags.rs`, EPIC.md, SPEC.md.

**Overall:** Strong, source-faithful plan. The Perl line references I spot-checked (3120-3151, 4273-4467,
4800-5018, 5022-5166, 8452-8711, 9228-9595) are all accurate, and the hard-won corrections folded back from
Phase 4 (the three counters at three lines, edge-guard gating, decision/print split) are correctly carried.
The §0 P1 decision is handled consistently. I found **one design issue that should change** (re-globbing the
genome vs consuming Phase 1's already-ordered FASTA list — the single byte-identity-critical surface), a
**config gap** (`phred64` is not on `RunConfig`), a couple of **header-serialization risks** that #11/#12 must
pin explicitly, and several smaller faithfulness notes. None are blockers; all are addressable in rev 1.

---

## 1. Logic review

### 1.1 The decision/print split, the three counters, the length guard — CORRECT

I traced the per-read body 3120-3151 against the plan:

- **`unique_best_alignment_count++` at 3121** — Phase 4, already in `merge.rs:242`. Plan leaves it (§11). ✓
- **`extract_corresponding_genomic_sequence_single_end` at 3124** — plan §3.2(a). ✓
- **Length guard at 3127**: `length(unmodified_genomic_sequence) != length(sequence)+2` → warn +
  `genomic_sequence_could_not_be_extracted_count++` (3129) + skip. Plan §3.2(b) and #6 are exact. ✓
- **MAPQ at 3133-3136** recomputed *after* the guard. Plan reuses `best.mapq` from Phase 4 and argues
  (§2) the value is identical because the inputs (read-len + `AS` + 2nd-best) are fixed at merge time and
  guard-failing reads are never written. I **verified** this: `calc_mapq(length($sequence), undef, AS,
  AS_2nd)` — none of those inputs change between 3134 and merge time, and `merge.rs:243` passes exactly
  `sequence.len()`, `None`, `best.alignment_score`, `second_for_mapq`. Reuse is safe. ✓
- **`methylation_call` at 3144** is fed `unmodified_genomic_sequence` (the FULL read+2 sequence, already
  revcomp'd in extraction for index 1) — NOT the trimmed/revcomp'd `ref_seq` that `single_end_SAM_output`
  later builds (8570/8577). Plan §3.2(c) gets this right (CT branch, full padded seq). Implementation must
  preserve the **ordering**: call `methylation_call` BEFORE the ref-seq trim/revcomp, exactly as Perl does
  (3144 then 8570/8577). The signatures in §4 keep these as separate functions, so the ordering is in the
  driver's hands — worth an explicit note in the implementation outline. ✓ (minor)

### 1.2 Per-strand counters behind the edge guards — CORRECT and subtle

Verified 4400-4445 are reached only after BOTH early-return guards (4317 for index 1/3, 4390 for index 0/2)
did NOT fire. The plan's repeated emphasis (§3.2(a), §4 doc-comment, §11, #5) that the strand counter bump is
"only when no edge guard fired" matches Perl exactly: a chromosome-edge read returns at 4318-4320 / 4391-4393
*before* reaching the `CT_CT_count++` etc., so it lands in `unique_best` but in no strand bucket. This is the
exact trap that bit Phase 4; the plan handles it. ✓

One **precision note**: the §4 `GenomicExtraction.extracted: bool` field is the plan's mechanism for the
guard. But Perl does NOT signal "extracted=false" — it signals the failure purely by the **length** of
`unmodified_genomic_sequence` (the 3127 guard re-measures length). The plan's `extracted` flag is a *Rust
convenience*; the driver MUST still apply the 3127 **length** check (`len != read+2`), not just
`!extracted` — because a non-edge read could in principle also produce a wrong-length sequence (e.g. a CIGAR
bug). The plan's §3.2(b) does say "length guard," and #6 tests it, but §4's `extracted` flag could tempt an
implementer to branch on the bool instead of the length. Recommend: keep the length check as the gate (the
faithful port), treat `extracted` as documentation-only or drop it. (Important-ish; see action items.)

### 1.3 CIGAR walk — CORRECT, with one omission to call out

The walk (4327-4385) is faithfully summarized: `M` appends + advances pos (+MD-seq iff deletion); `I`/`S`
append `'X'*len` no pos change; `D` advances pos + `indels += len` (+MD-seq); `N` advances pos only; illegal
ops die. Two faithfulness details the plan should pin so the verbatim port doesn't drift:

- **`indels` accrues for `D` only, NOT for `I`/`S`/`N`** (4346-4347 explicitly comment that insertions add
  nothing; 4376-4377 same for `N`). The plan's §3.2(a) bullet says "`I`/`S` → … (no `pos` change)" and "`D`
  → `pos += len`, `indels += len`" — correct, but does not state that `I`/`S`/`N` deliberately do NOT bump
  `indels`. This matters because `NM:i = hemming_dist + indels` (8590) — getting `indels` wrong silently
  corrupts every indel read's NM tag. Make it explicit. (Important.)
- **The illegal-op die uses `$cigar =~ tr/[HPX=]//`** (4379) — a transliteration *count* over the whole
  CIGAR string, not a per-op check, and it (Perl bug-or-feature) includes `[`, `]` in the character class.
  The plan §3.2(a) says "`H`/`P`/`X`/`=` or anything else → die." A faithful Rust port should die on any op
  that is not one of `M/I/D/S/N`; the exact Perl `tr` character class (`[HPX=`]`) is cosmetic since the
  `else` branch (4382) catches everything else anyway. Fine to port as "anything not MIDSN → die," but note
  it. (Optional.)

### 1.4 The two complement helpers — CORRECT and important

Verified they genuinely differ:
- `reverse_complement` (5161): `tr/CATG/GTAC/` THEN `reverse`. Leaves `N`, lower-case, and any other byte
  **unchanged**; only upper-case CATG are complemented.
- `revcomp` (9228): `reverse` THEN `tr/ACTGactg/TGACTGAC/`. Complements BOTH cases; leaves `N`/`X`/other
  unchanged.

For pure upper-case `ACGTN` input both yield the same result, but the genome is upper-cased (5103) and the
extraction can inject `'X'` padding — `'X'` is untouched by both, so they still agree on the data they see in
SE-directional. The plan's instruction to "port each verbatim" (§3.3, §8) is the right call and avoids a
latent divergence if these ever meet lower-case or unusual bytes (PE/non-dir phases). ✓

**Edge note (Optional):** `revcomp` is `my $seq = shift or die` — Perl-falsy, so it dies on an **empty
string** OR the literal string `"0"`. A faithful Rust `revcomp` would not reproduce the `"0"` die unless
coded; in practice no revcomp'd ACGTNX string equals `"0"`, so this is inert. Worth a one-line "we do not
replicate the Perl-falsy `"0"` die (unreachable for sequence data)" in the port notes.

### 1.5 The +2 padding and its trim — CORRECT

Verified 8570-8575: `read_conversion eq 'CT'` → drop the **last** 2 bases (`substr 0, len-2`); else drop the
**first** 2 (`substr 2, len-2`). For SE-directional `read_conversion` is always `CT` (index 0 and index 1
both = CT), so the trim is always "drop last 2." Plan §3.3 covers both branches (the `else` is Phase-8 inert).
✓ The interaction with the minus-strand revcomp (8577) happens AFTER the trim, in the right order. ✓

### 1.6 FLAG — CORRECT

8521-8546 verified. `+`/CT/CT→0; `+`/GA/GA→16; `-`/CT/GA→16; `-`/GA/CT→0; any other combo dies. SE-directional:
index 0 (`+`/CT/CT) → 0; index 1 (`-`/CT/GA) → 16. Plan §3.3 exact. ✓

### 1.7 `make_mismatch_string` deletion path — CORRECT to flag as the #1 verbatim-port risk

I read 9252-9595 in full. The match/mismatch run-builder (9283-9319) is straightforward, but the deletion
re-indexing (9325-9591) is genuinely intricate and **stateful in a way that resists idiomatic Rust**:

- It builds `@md = split //, $new_MD` (per-character array of the MD string), then **re-splits `@md` from a
  freshly rebuilt `$new_MD` after each deletion** (9581) and uses `$md_index_already_processed` /
  `$current_md_index` to skip already-rewritten prefix characters on the next deletion. A naive
  `chars().enumerate()` Rust port will be wrong for **multi-deletion** reads because the indices shift when
  `${pos_before_deletion}^${deleted_bases}` (multi-char) replaces a single `$op` token (9485, 9559).
- There are **two** copies of the split-the-matching-run logic: the in-loop arm (9441-9502) and the
  trailing "last element was a digit" arm (9526-9578) for when the deletion falls in the final MD token.
  Both must be ported.
- `$verbose` gates only `warn` statements (never mutates state) — the Rust port can drop all of it.

The plan correctly says "port verbatim" and dedicates #9 to it. **Strengthen #9**: the listed cases (clean,
single mismatch, leading/adjacent `0`-padding, 1-deletion `^`, insertion/soft-clip `X` skip) do NOT include a
**multi-deletion** read (≥2 `D` ops in one CIGAR), which is exactly where the `md_index_already_processed`
re-indexing fires (9402-9405, 9481-9501). Without a 2-deletion test, the hardest branch ships untested. Also
add a **deletion-in-the-final-MD-token** case (exercises the 9526-9578 trailing arm) and a
**deletion-adjacent-to-a-mismatch** case. (Important — see action items.)

### 1.8 Genome loader — CORRECT, but see Assumptions §2.1 for the re-glob design issue

5022-5147 verified: glob fallback chain, gunzip, first-line FASTA-header requirement, `extract_chromosome_name`
(`s/^>//` + first whitespace token), empty-name die (5069/5098), duplicate-name die (5080/5109),
empty-sequence WARN-not-die (5085/5114), `uc` (5103), `\r` strip + chomp (5065-5066, 5075-5076),
`SQ_order{++count}=name` insertion order. The plan §3.1 captures all of these. The `--cram` reconstitution
(5129-5143) is correctly scoped out. ✓

**One faithfulness nuance on `extract_chromosome_name`:** the plan's §4 signature
`fn extract_chromosome_name(fasta_header: &str) -> Result<&str>` returns the name and (per §3.1 step 2) "die
if no `>`". But the **empty-name die** (5069/5098) lives in the *caller* (`read_genome_into_memory`), not in
`extract_chromosome_name` — Perl's `extract_chromosome_name` happily returns `''` for `> chr1` (leading
space: `s/^>//` → ` chr1`, `split /\s+/` → `('', 'chr1')`, returns `''`). So `extract_chromosome_name` must
return `Ok("")` for the leading-space case, and the **caller** must die on empty. Test #2 expects "`> chr1`
(leading space) → die" — that die must be asserted at the **loader** level, not inside
`extract_chromosome_name`. The plan's prose is slightly ambiguous here; make sure the die-on-empty is in the
loader. (Important-ish.)

---

## 2. Assumptions

### 2.1 🔴 `@SQ` order: re-globbing duplicates Phase 1's already-ordered FASTA list (DESIGN ISSUE)

This is my most important finding. **Phase 1 already discovered and ordered the FASTA files**:
`config.genome.fastas: Vec<PathBuf>` is documented in `discovery.rs:81` as *"Raw FASTA file(s), in
byte-significant order (sets `@SQ` order, Phase 5)"* — and `discovery.rs` header (lines 9-17) explicitly says
this ordering "sets the BAM `@SQ` order in Phase 5" and is "a deliberate mirror of
`bismark-genome-preparation::discovery`."

The Phase-5 plan's §3.1 / §4 instead defines `read_genome_into_memory(genome_folder: &Path)` that **re-globs
the folder from scratch** with its own Perl-faithful `<*.fa>`→`<*.fa.gz>`→… fallback chain and its own sort.
This:
1. **Duplicates** the discovery/ordering logic in a *second* place, against the project's own stated intent.
2. Creates a **byte-identity divergence risk**: if the loader's re-glob and `discover_fastas` ever disagree
   (sort, case-fold, symlink-follow via `is_file()`, the `.fa` vs `.fa.gz` disjointness), the `@SQ` header
   silently breaks — and `@SQ` order is the single most load-bearing header property at this gate.

**Recommendation:** the genome loader should **consume the already-ordered list**, e.g.
`read_genome_into_memory(fastas: &[PathBuf]) -> Result<Genome>` (or take `&GenomeIndexes`), iterating
`config.genome.fastas` in order and only doing the per-file parse (header, uc, concat, dup/empty checks).
The `@SQ` order then has exactly ONE source of truth (Phase 1's `discover_fastas`), and `sq_order` falls out
of the iteration order for free. This also makes the genome-prep-parity contract a single point to validate
on Linux (#1), not two. (Important → arguably Critical for the gate; see action items.)

If there's a reason to re-glob (e.g. the loader is meant to be reusable independently of `RunConfig`), the
plan must at minimum state that the loader's glob is asserted byte-for-byte equal to `discover_fastas`'s
output and add a test that runs both over the same multi-file dir and asserts identical ordering. But
consuming the existing list is strictly safer.

### 2.2 🔴 `phred64` is NOT on `RunConfig` (CONFIG GAP)

The plan's §3.3, §4 (`single_end_sam_output(..., phred64: bool)`), and §8 ("`--phred64` conversion is ported
but inert by default") assume the driver can read `phred64` off the config. But I checked `config.rs` —
`RunConfig` has **no `phred64` field**. The flag exists on `Cli` (`cli.rs:99-100`, `--phred64-quals`) and is
threaded into `aligner_options` (`options.rs:41-43`), but it is **not surfaced on `RunConfig`**. So the
driver cannot pass `phred64` to `single_end_sam_output` without a config change.

This is the same "additive `RunConfig` extension" pattern Phase 4 used for `score_min_intercept/slope`. The
plan should add a step-0 config prereq: add `pub phred64: bool` (and, if faithful, `solexa`/`solexa1.3` — see
below) to `RunConfig`, populated in `resolve()`. As written, §5 step 5 reaches for a field that doesn't
exist. (Important.)

**Sub-note (solexa):** Perl `print_bisulfite_mapping_result_single_end` (4191) only branches on `$phred64`,
calling `convert_phred64_quals_to_phred33`. There is a *separate* `convert_solexa_quals_to_phred33` (4234)
but it is NOT called at this print site — so the plan is right to port only the phred64 conversion. Good
(no action), but worth a one-line note that solexa is intentionally not in this path.

### 2.3 `bowtie_sequence` unused for output — CORRECT

Verified: `single_end_SAM_output` builds `SEQ` from `$actual_seq` (the original read, arg 2 = `$sequence`),
and `methylation_call` (3144) compares `$sequence` (original) to the genomic window. `bowtie_sequence` (the
converted read Bowtie 2 reported) is never an output field. The plan's §2/§8 assertion holds; the
`BestAlignment.bowtie_sequence` field carried from Phase 4 is dead weight for Phase 5 (harmless). ✓

### 2.4 Default tag set `NM MD XM XR XG`, no `XA` — CORRECT

Verified 8700-8709: the default (`!non_bs_mm`, `!rg_tag`) print is exactly
`NM_tag, MD_tag, XM_tag, XR_tag, XG_tag` in that order. `XA` (8679/8694) and `RG` (8702) are
`--non_bs_mm`/`--rg_tag`-only. The plan §3.3 / §8 are exact. **Note**: `--non_bs_mm` and `--rg_tag` are
already in `deferred_flags` (`config.rs:294-296`) but NOT yet *rejected* — the plan §3.2(c)/§7 says reject or
assert-unset for v1. Recommend a hard reject in `resolve()` (like `--sam`/`--cram` already are) so they
cannot silently change the tag set/order. The `--slam` reject (3140 path) likewise. (Important — fail-loud,
not fail-silent.)

### 2.5 QUAL ASCII↔phred for BAM — CORRECT, but pin the encoding contract

The plan §3.3/§ edge-cases says "subtract 33 from each ASCII byte." This is right for noodles' `RecordBuf`
`QualityScores`, which stores **raw phred scores** (0-93), and `samtools view -h` re-renders them as ASCII+33.
The `write.rs` test (`synth_record_buf`, line 398-399) confirms: `QualityScores::from(vec![30u8; 5])` (raw
phred 30), which `samtools view` would render as `?` (30+33=63). So the driver must build
`QualityScores` from `qual_byte - 33` for each ASCII byte of the (already phred33) quality string. The
minus-strand `reverse` of the quality (8583) happens on the ASCII string before the subtract; order is
irrelevant to the result. Plan is correct; #11 must assert the round-trip renders the **exact original ASCII**
QUAL. ✓

**Edge:** a `*` QUAL (no quality, FASTA input) — out of scope (FastQ only in Phase 5), but if a read's qual
is `*`/empty the `qual_byte - 33` would underflow. FastQ guarantees a real qual string; note it.

### 2.6 `@HD VN:1.0` and `SO:unsorted` header serialization — UNDER-VALIDATED (RISK)

This is the part of #11/#12 I'm least confident the plan has de-risked. The `bismark-io` writer builds the
noodles `Header` and `write.rs`'s own test uses `Version::new(1, 6)` — but Bismark needs **`@HD\tVN:1.0\tSO:unsorted`**
(8454). Two concrete risks the plan must pin, not assume:

1. **Does noodles serialize `VN:1.0` or normalize it to `VN:1`?** noodles' `header::record::value::map::header::Version`
   is a `(major, minor)` pair; if `Version::new(1, 0)` serializes as `VN:1.0` we're fine, but if it drops the
   trailing `.0` (→ `VN:1`) the `@HD` line diverges. This must be **checked empirically** in #12, not assumed.
2. **`SO:unsorted`** must be set on the `@HD` map. The `synth_header` in `write.rs` does NOT set `SO` at all
   (it only sets the version) — so the existing writer test does not exercise `SO:unsorted`. The plan must
   construct the `@HD` map with the sort-order subfield = `unsorted` and #12 must assert the literal
   `@HD\tVN:1.0\tSO:unsorted` bytes.
3. **`@SQ` line field order**: noodles emits `@SQ\tSN:..\tLN:..`. Perl emits `SN` then `LN` (8469). Verify no
   extra subfields (e.g. noodles adding `M5`/`UR`) sneak in. The plan only stores name+len, so this should be
   clean, but #12 should diff the whole `@SQ` block.

Recommend #12 explicitly diff the **literal header bytes** (`samtools view -H`) against a hand-written
expected string, char-for-char, for `@HD`, every `@SQ`, and the Bismark `@PG` — not just "looks right."
(Important.)

### 2.7 `@PG CL:"bismark <command_line>"` — CONSISTENT with §0 P1

8480 verified: `@PG\tID:Bismark\tVN:$bismark_version\tCL:"bismark $command_line"`. The plan reproduces
Bismark's own `@PG` (with `VN:v0.25.1` spoofed via `BISMARK_VERSION` in `lib.rs:46`, and `command_line` from
`RunConfig.command_line`) and normalizes only the samtools `@PG` out of the gate (§0 P1). This is consistent
throughout (§0, §3.4, #14, §10). One thing to confirm in #12: the `CL:` value uses **double quotes** around
`bismark <argv>` (8480) — the Rust port must emit the literal `CL:"bismark ..."` with the quotes, and the
argv reconstruction must match Perl's `$command_line` (program name excluded, per `lib.rs:60` doc). The
Phase-1 `command_line` capture should already match; #14's real-data diff is the ultimate check, but a
hand-built #12 assertion on the `@PG` line (quotes included) de-risks earlier. ✓ (with the note)

---

## 3. Efficiency

- **Genome as `Vec<u8>` per chromosome** — same footprint as Perl's `%chromosomes` strings; correct and
  necessary. Held once (load before the file loop, §5 step 5). For a human genome this is ~3 GB resident,
  matching Perl. No issue. ✓
- **Per-read work** is linear in read-length (the call + MD walk + hemming). Pre-sizing the window/strings to
  `read_len + 2` (§6) is a sound micro-opt; not load-bearing. ✓
- **`refid` HashMap** (`&str → tid`) is small (one entry per chromosome) and looked up once per read. The §4
  signature uses `refid: &HashMap<&str, usize>` — fine, but note the lifetime: the keys borrow from
  `genome.sq_order`/`chromosomes`, which outlive the per-read loop, so this is OK. Alternatively store the
  tid on the de-converted chromosome at merge time, but the HashMap is simplest. ✓
- The plan correctly notes alignment CPU is Bowtie 2's and not a target here. The only allocation worth
  watching is `make_mismatch_string` rebuilding strings per deletion — but indel reads are a minority and the
  Perl does the same, so byte-identity > micro-opt here. ✓

No efficiency concerns.

---

## 4. Validation sufficiency

The 14 validations are well-targeted. Gaps / strengthening:

- **#9 (`make_mismatch_string`)** — **add a multi-deletion (≥2 `D`) case** and a **deletion-in-final-MD-token**
  case (§1.7). This is the highest-risk verbatim port; the current cases miss the `md_index_already_processed`
  re-indexing branch entirely. Without it the hardest code path ships green-but-untested. (Important.)
- **#11 (round-trip fidelity)** — strengthen to assert, on a real `samtools view -h` of a 1-record BAM:
  (a) `NM:i:<n>` renders as `:i:` not `:Z:`/`:f:` (noodles `Value::UInt8`/`Int32` round-trips to `:i:`);
  (b) tag **order** is literally `NM MD XM XR XG` (noodles `Data` is insertion-ordered — confirm the writer
  preserves it through BAM encode→decode); (c) QUAL renders the **exact original ASCII**; (d) SEQ exact;
  (e) MAPQ integer exact. The plan lists these; make them explicit asserts. (Important — load-bearing.)
- **#12 (header)** — make it a **literal byte diff** of `@HD`/`@SQ`/`@PG` vs a hand-written expected, to catch
  the `VN:1.0`-vs-`VN:1` and `SO:unsorted` risks (§2.6). (Important.)
- **Missing: NM with indels.** No validation checks `NM:i = hemming_dist + indels` for a read with a
  deletion AND an insertion (where `indels` counts only `D`). Add a case asserting the exact NM for a
  CIGAR like `50M2D3I47M` (NM must include the 2 from `D` but NOT the 3 from `I` — the `I` bases are `X`
  padding that hemming_dist counts as mismatches instead). This is a silent-wrong-result trap (§1.3).
  (Important.)
- **Missing: minus-strand MD/NM end-to-end.** #10 checks SEQ/QUAL/XM reversal for index 1, but does not
  assert the **MD:Z** and **NM:i** for a minus-strand read with a mismatch (where `ref_seq` and `actual_seq`
  are both revcomp'd before `make_mismatch_string`/`hemming_dist`). A minus-strand read with one mismatch
  near an end is a good combined case. (Optional → Important.)
- **#13/#14 platform** — §10 open-question correctly flags Linux/oxy for the gate; macOS gets unit +
  hermetic only. Consistent with the epic. The hermetic #13 (fake bowtie2 + tiny genome) is the right
  pre-Linux smoke. ✓
- **`U`/`u` unknown-context** (#8) — good that it's called out; ensure the test covers the context base being
  both `N` and `X` (4844/4856 treat `X` like `N`), since `X` is the insertion/soft-clip padding that can
  legitimately appear as the look-ahead base. (Optional.)

Overall the validation catches the major failure modes once #9/#11/#12 are strengthened and the NM-with-indels
gap is closed.

---

## 5. Alternatives

- **Write BAM via noodles vs emit SAM text directly** (plan §10 open Q): I agree with the plan's choice —
  `BamWriter` is the project standard, default Bismark output is BAM, and the downstream Rust tools consume
  BAM. #11 is the right de-risking. The SAM-text contingency is correctly flagged as a fallback, not the
  plan. The only residual risk is header serialization (§2.6), which is a noodles-config detail, not a reason
  to abandon noodles. Keep BAM. ✓
- **`Conversion` enum vs `&str`** (plan §10): the enum is cleaner and output-neutral (mapped to `"CT"`/`"GA"`
  at the tag boundary). Agree, inconsequential. ✓
- **Genome loader consuming `config.genome.fastas` vs re-globbing** (my §2.1): consuming the existing list is
  the better alternative and removes a whole class of `@SQ`-order risk. **This is the one alternative I'd
  push the plan to adopt.** (See action items.)
- **`extracted: bool` flag vs length check** (my §1.2): prefer the faithful length check (3127) as the gate;
  the bool is a documentation convenience that risks diverging from Perl if treated as the decision. Minor.

---

## 6. Action items (prioritized)

### Critical (address before implementation)

1. **Consume Phase 1's `config.genome.fastas` for the `@SQ` order instead of re-globbing** (§2.1). Change
   `read_genome_into_memory` to take the already-ordered FASTA list (`&[PathBuf]` or `&GenomeIndexes`) so the
   `@SQ`/byte-identity-critical ordering has exactly one source of truth (`discover_fastas`), matching
   `discovery.rs`'s own stated contract. If re-globbing is kept for any reason, add a test asserting the
   loader's order == `discover_fastas`'s order over the same multi-file dir. This is the single most
   load-bearing surface of the gate.

2. **Add `phred64` (and the config plumbing) to `RunConfig`** (§2.2). The plan's signatures/outline reference
   `config.phred64`, but `RunConfig` has no such field today. Add an additive config prereq (step 0, mirroring
   Phase 4's `score_min_*`), populated in `resolve()`. Without it §5 step 5 can't compile.

### Important

3. **Strengthen #9 with a multi-deletion (≥2 `D`) test + a deletion-in-final-MD-token test** (§1.7). The
   `md_index_already_processed` re-indexing (9402-9405, 9481-9501, 9526-9578) is the hardest branch and is
   currently untested.

4. **Add an NM-with-indels validation** asserting `NM:i = hemming_dist + indels` where `indels` counts only
   `D` ops (NOT `I`/`S`/`N`), e.g. a `…M…D…I…M` CIGAR (§1.3, §4 missing-validation). Silent-wrong-result trap.

5. **Make #11 explicit**: assert `NM` renders `:i:` (not `:Z:`/`:f:`), tag order is literally `NM MD XM XR XG`
   through a BAM encode→decode, QUAL renders exact original ASCII, SEQ/MAPQ exact (§4).

6. **Make #12 a literal byte diff** of `@HD\tVN:1.0\tSO:unsorted`, every `@SQ\tSN:..\tLN:..`, and
   `@PG\tID:Bismark\tVN:v0.25.1\tCL:"bismark …"` vs a hand-built expected — to catch noodles `VN:1.0`-vs-`VN:1`
   normalization, a missing `SO:unsorted`, or stray `@SQ` subfields (§2.6).

7. **Hard-reject (not just defer) the output-affecting flags out of v1 scope**: `--slam`, `--non_bs_mm`,
   `--rg_tag` should fail loudly in `resolve()` (like `--sam`/`--cram` already do), not silently no-op, so
   they cannot change the tag set/order (§2.4, plan §7). Update `deferred_flags` accordingly and drop
   `--basename` from it (Phase 5 honors it).

8. **Keep the 3127 length check as the gate** (not the `extracted` bool) and locate the **empty-name die**
   in the loader, not in `extract_chromosome_name` (which must return `Ok("")` for `> chr1`) (§1.2, §1.8).

9. **Pin the `methylation_call`-before-trim/revcomp ordering** and the "`indels` accrues for `D` only"
   invariant in the implementation outline so the verbatim port doesn't drift (§1.1, §1.3).

### Optional

10. Add a minus-strand MD:Z/NM:i combined case (mismatch near an end) to #10 (§4).
11. Cover both `N` and `X` as the unknown-context look-ahead base in #8 (§4).
12. Note in the port docs that the Perl `revcomp` `"0"`-is-falsy die is intentionally not replicated
    (unreachable for sequence data) (§1.4), and that solexa quality conversion is intentionally not in this
    print path (§2.2).
13. Confirm in #14 that QNAME is the `@`-stripped `fix_id` identifier (the driver already computes this in
    `lib.rs:191`), not the raw header — the implementation should pass `identifier`, not the raw line.

---

*Reviewer B — independent review; no source/plan files were modified.*
