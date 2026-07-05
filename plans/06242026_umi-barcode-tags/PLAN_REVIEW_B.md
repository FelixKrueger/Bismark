# PLAN_REVIEW_B — `--add_barcode` / `--add_umi` (Rust aligner CB/UR tags)

**Reviewer:** B (independent)
**Plan reviewed:** `/Users/fkrueger/Github/Bismark-umi/plans/06242026_umi-barcode-tags/PLAN.md`
**Worktree:** `/Users/fkrueger/Github/Bismark-umi` @ `rust/umi-barcode` (base `origin/rust/iron-chancellor` @ `61446e7`)
**Verdict:** **APPROVE with minor fixes.** The design is sound, the integration claims hold against the real code, and the highest-risk correctness claim (Perl/Rust split equivalence) is empirically confirmed. No Critical issues. Three Important items and several Optional refinements below.

---

## What I verified against the code (not on faith)

Every concrete integration claim in the plan was checked in the worktree. Summary of confirmations:

| Plan claim | Verified | Evidence |
|---|---|---|
| SE builder `single_end_sam_output` exists; tag block `NM/MD/XM/XR/XG` ends ~L440; `id` is the QNAME | ✅ | `output.rs:341` fn; tags inserted L427–440; `id: &str` at L342, `*rec.name_mut() = …BString::from(id…)` L414 |
| PE builder `paired_end_sam_output` (L453) → shared inner `build_pe_mate` (L584); tag block ends ~L665 | ✅ | `output.rs:453`, `:584`; tags L652–665 |
| `build_pe_mate` called once per mate with the **same `id`** → one insert tags both R1+R2 | ✅ | `output.rs:537` (rec1) and `:557` (rec2) both pass `id` as the first arg |
| Existing tag order is NM/MD/XM/XR/XG | ✅ | both builders insert in exactly that sequence |
| `Data` is the noodles tag-map mutated via `rec.data_mut()`; `insert` **appends** when the tag is new | ✅ | noodles-sam **0.85.0** `record_buf/data.rs`: `pub struct Data(Vec<(Tag, Value)>)`; `insert` does `self.0.push(field)` when `get_index_of` is `None`. CB/UR are never pre-present → guaranteed appended after XG. Order claim holds. |
| `Tag` / `Value` / `BString` already imported in `output.rs` | ✅ | `output.rs:14` (`bstr::BString`), `:20` (`…field::Tag`), `:22` (`record_buf::data::field::Value`) |
| `Data` itself is **not** currently imported | ⚠️ confirmed | only `Value` is pulled from `record_buf::data::field`; the helper's `data: &mut Data` param needs `use noodles_sam::alignment::record_buf::data::Data;` added. Plan flags this in step 8 + the footnote — good, but see Important #1. |
| SE call site `lib.rs:1022`; PE call site `lib.rs:3061`; both have `config: &RunConfig` in scope; both pass `config.phred64` | ✅ | L1022 is inside `route_se_decision` (`lib.rs:986`, sig has `config: &RunConfig` at L997). L3061 is inside `route_pe_decision` (`lib.rs:3008`, `config: &RunConfig` at L3024). |
| These are the **only** two production builder call sites | ✅ | grep for `single_end_sam_output(` / `paired_end_sam_output(` across `src/` + `tests/`: exactly 2 production calls (both in lib.rs); `combined.rs` only *mentions* them in doc comments, no call. Every combined-index variant funnels through `route_se_decision`/`route_pe_decision`. |
| `RunConfig` siblings `ambiguous` (L215), `ambig_bam` (L219), `combined_index` (L243) are plain bools | ✅ | `config.rs` struct |
| `Cli → RunConfig` mapping sets `ambiguous: cli.ambiguous` / `ambig_bam: cli.ambig_bam` | ✅ but **location wrong in plan** | mapping is `Ok(RunConfig { … })` at `config.rs:412`, with assignments at L429–447 — **not** "L201–220" as the Context section states (that range is the struct *definition*). See Important #2. |
| clap-derive flag style `#[arg(long = "…")]` | ✅ | `cli.rs:113` `#[arg(long = "combined_index")]` etc. — plan's `#[arg(long = "add_barcode")]` matches exactly. |
| `--multicore` reuses lib.rs process fns + clones `RunConfig`; nothing strips/reorders tags in merge | ✅ | `parallel.rs:644`/`:792` `let mut cfg = config.clone();`; workers call `crate::process_se_chunk`/`process_pe_chunk` (L427/L487). `merge_bams` (L523) reads each part with `record_bufs` and `write_raw_record` **verbatim** (L531–535) — CB/UR survive in order. "Multicore is free" holds completely. |

**Bottom line on integration:** every load-bearing line number and structural claim is correct (modulo the one mislocated reference noted in Important #2). The plan's mental model of the code is accurate.

---

## Logic review

### The split-parse equivalence (the one claim that could silently produce wrong output) — VERIFIED

The fork uses Perl `split(/_/, $id, 3)`; the plan uses Rust `id.splitn(3, '_')`. I ran both on a 10-input edge set (including the 5 plan rows plus `BC__rest`, `__`, `_`, empty). **Byte-for-byte identical** on all 10:

```
input            Rust splitn(3,'_')                    Perl split(/_/,$id,3)
BC_UMI_rest_a_b  b=BC  u=UMI r=rest_a_b   -> CB UR     (identical)
nounderscore     b=…   u=""  r=""         -> CB NO     (identical)
_UMI_rest        b=""  u=UMI r=rest       -> NO UR     (identical)
BC_              b=BC  u=""  r=""         -> CB NO     (identical)
BC_UMI           b=BC  u=UMI r=""         -> CB UR     (identical)
BC__rest         b=BC  u=""  r=rest       -> CB NO     (identical)   <-- empty UMI field, NOT skipped-collapsed
BC_UMI_          b=BC  u=UMI r=""         -> CB UR     (identical)   <-- trailing empty field PRESERVED under limit
__ / _ / ""      -> NO NO                              (identical)
```

The subtle Perl behaviour the plan implicitly relies on is correct: Perl normally strips **trailing** empty fields, but a positive `LIMIT` (3) **disables** that stripping — so `BC_UMI_` keeps `u="UMI"` and `BC__rest` keeps an empty field-1. Rust `splitn` behaves the same. **The plan's 5-row edge-case table is a correct subset; no row is wrong.** This was the single highest-risk item and it checks out.

### PE QNAME / both-mates tagging — correct, and the `/1`-`/2` worry is a non-issue

- At the PE call site the `identifier` passed to `paired_end_sam_output` (lib.rs:3061) is `id1_fixed` with the `@`/`>` prefix stripped (`lib.rs:2931–2936`), derived from the **R1 FastQ header** via `convert::fix_id`. The same `identifier` flows into both `build_pe_mate` calls, so **both mates necessarily get identical CB/UR**. ✅
- The `/1`/`/2` concern is moot: those suffixes are **never in the FastQ QNAME**. Bismark *adds* `/1/1`,`/2/2` internally during conversion (`convert.rs:197 pe_id_suffix`), Bowtie 2 strips the outer pair, and the merge strips the remaining `/1` (`align.rs:502–520`). The name reaching the builder is the original read name. So for a SeekGene read `BARCODE_UMI_<rest>`, the parsed `identifier` is exactly that. ✅
- **Tagging both mates is the right call** for the stated consumers. `umi_tools dedup --paired` reads the UMI tag from R1 but does not error if R2 also carries it; per-cell `CB`-based splitting needs the barcode on *both* mates so neither is orphaned. This matches 10x/CellRanger convention (CB/UB on every record of a pair). I see no correctness argument for R1-only here. ✅ (One caveat: see Important #3 on the umi_tools tag choice.)

### Scope-boundary (where tags must NOT leak) — confirmed in code

- `--ambig_bam`: written via `write_raw_sam_line_to_bam` / `write_raw_pe_ambig_lines` (`output.rs:674`), which copy raw Bowtie2 lines and never touch the two builders → **no CB/UR**, as the plan states. ✅
- `--unmapped` / `--ambiguous`: FastQ aux paths (`write_se_aux_record`, lib.rs:1050/1057) — not BAM, no tag surface. ✅
- The `Decision::Ambiguous` / `DecisionPaired::Ambiguous` arms (lib.rs:1037, 3078) never call the builders. ✅

So the tags are emitted **only** on `Decision::UniqueBest` records, exactly the intended surface.

---

## Assumptions — audit

| Assumption (plan) | Assessment |
|---|---|
| Read names follow `BARCODE_UMI_<rest>`; feature opt-in/off by default | Valid. Early-return on `!enabled()` guarantees default byte-identity. |
| PE mates share the QNAME | **Verified true in code** (single `identifier` → both mates). |
| "Append only if defined & non-empty" matches the fork | Matches the Perl behaviour I reproduced. |
| CB/UR are `Z` string tags | Correct per 10x convention. |
| `fix_id` whitespace→`_` does not corrupt fields 0/1 | **New consideration the plan does not state.** `fix_id` (`convert.rs:76`) collapses each run of spaces/tabs to a single `_` *before* the QNAME reaches the builder. If a SeekGene name ever contained whitespace inside the barcode/UMI region it would be re-split — but barcodes/UMIs are alphanumeric and the `BARCODE_UMI_` prefix is added upstream, so in practice fields 0/1 are whitespace-free. Worth a one-line note in Assumptions that parsing happens **post-`fix_id`** (Optional #2). Not a bug. |

No hidden assumption rises to a blocker.

---

## Efficiency

Accurate as written. Default path: one `opts.enabled()` bool check, branch-not-taken, **before** any split → zero added allocation/work, record layout unchanged. Flagged path: one `splitn` walk (O(len QNAME), no allocation — `splitn` yields `&str` slices) plus at most two `BString::from(&str)` allocations. `Data::insert`'s `get_index_of` is an O(n) linear scan over the ≤7 existing tags — negligible. No scalability concern; this is per-record constant work dwarfed by alignment/IO. ✅

---

## Validation sufficiency

The proposed unit + fixture matrix is **mostly sufficient** given there is no byte-identity oracle. It covers: the canonical parse, the flag matrix (incl. the neither-flag byte-identity regression), the malformed-name skips, the PE both-mates equality, and a binary-level fixture check via `samtools view`. Gaps worth closing:

1. **`__rest`-style "present-but-empty middle field" is not in the unit list.** The plan's table omits `BC__rest` (empty UMI between two underscores → CB yes, UR **no**). This is exactly the case most likely to be implemented wrong (a naive `split('_')` without the empty-check, or a `> 2 fields` guard, would emit `UR:Z:`). Add it explicitly. (Important #3 below — it's both a test gap and the place a regression would hide.)
2. **No assertion that `--ambig_bam` records carry NO CB/UR.** The plan's fixture step *mentions* checking ambig_bam has none, but it is phrased loosely ("and none on `--ambig_bam`"). Make this an explicit, asserted step in the integration test (run with `--add_barcode --add_umi --ambig_bam`, assert the `.ambig.bam` has zero `CB:`/`UR:`). This is the one place a scope leak could occur and the only assertion that proves the boundary.
3. **No `--multicore N` validation.** "Multicore is free" is verified by me at the code level, but there is no test exercising `--add_barcode --add_umi --multicore 2` to prove the merge preserves tags + ordering. Cheap to add to the fixture step (same input, `--parallel 2`, assert CB/UR present on the merged BAM and the file count/record count matches the single-core run). Recommended given worker-invariance is a load-bearing project gate.
4. **PE both-mates: assert the values are equal AND non-empty**, not just "both carry CB/UR". A bug that tagged both mates with an empty/garbage barcode would pass a presence-only check.

None of these are Critical (the design is right), but #1 and #2 close the two places silent-wrong-output could survive the current test list.

---

## Alternatives considered

**Threading a `Copy BarcodeUmiTags` struct through the builder signatures (plan's choice) vs. alternatives:**

- **(Chosen) `opts: BarcodeUmiTags` trailing param on both builders + `build_pe_mate`.** Pros: single insertion point per builder; PE both-mates covered once; compiler enforces all call sites updated; `Copy`/`Default` keeps churn mechanical; the no-flag default is provably byte-identical. Cons: touches 2 production + 9 test call sites (all in `output.rs`/`lib.rs`, mechanical). **This is the right trade-off** — it keeps the parse logic colocated with tag assembly where `Tag`/`Value`/`BString` already live.

- **(Rejected) Mutate the record post-construction at the lib.rs call sites** (`append_barcode_umi_tags(record.inner_mut().data_mut(), identifier, opts)` after the builder returns). Pros: zero builder-signature churn; no test-call-site updates. Cons: (a) `BismarkRecord` wraps the noodles record and re-validates on construction — appending after `from_noodles_record` bypasses that wrapper and may need a new `inner_mut()` accessor; (b) splits the tag-assembly logic across two files (builder writes NM..XG, caller writes CB/UR), which is exactly the kind of split the faithful port has avoided; (c) for PE you'd call it twice (rec1, rec2) at the call site, re-introducing the duplication the shared `build_pe_mate` eliminates. **The plan's choice is cleaner.** Worth one sentence in the plan recording *why* post-construction mutation was not chosen, so a future reader doesn't "simplify" toward it.

- **(Rejected) A broader `OutputOptions`/builder-options struct** folding `phred64`/`dovetail`/`opts` together. Pro: caps the `too_many_arguments` growth. Con: out of scope — a refactor of the existing faithful signatures, churns the byte-identity-frozen call sites for no functional gain. Defer.

No alternative beats the chosen approach; the only ask is to *document* the rejection of post-construction mutation.

---

## Action items (prioritized)

### Critical
*(none)*

### Important
1. **Pin the `Data` import + insert semantics now, not "during implementation."** The helper signature `fn append_barcode_umi_tags(data: &mut Data, …)` requires `use noodles_sam::alignment::record_buf::data::Data;` added to `output.rs` (currently absent). I confirmed the concrete type is `noodles_sam::alignment::record_buf::data::Data` (noodles-sam 0.85.0) and that `.insert(tag, value)` appends for a new tag. The plan leaves this as a step-8 "confirm during implementation" — it's already confirmed; bake the exact `use` line into the plan so the implementer doesn't re-derive it. (`output.rs:11–36` import block.)
2. **Fix the mislocated mapping reference in the Context section.** Plan L30 says "`Cli → RunConfig` mapping lives near L201–220" — that range is the *struct definition*. The actual `Ok(RunConfig { … })` mapping is at `config.rs:412`, with the sibling assignments `ambiguous: cli.ambiguous` (L431) / `ambig_bam: cli.ambig_bam` (L432). Implementation step 2 is correct ("near the ambiguous/ambig_bam lines"), so this is a doc inconsistency that could send the implementer to the wrong line — correct it to ~L412/L429–447.
3. **Add the empty-middle-field test row + an explicit ambig_bam-clean assertion.** (a) Add `BC__rest` to the unit matrix asserting **CB yes / UR no** — this is the case a naive parse gets wrong and it is missing from the plan's table. (b) Promote the fixture's "none on `--ambig_bam`" remark to an asserted step (run with `--ambig_bam`, assert zero `CB:`/`UR:` in the `.ambig.bam`). These close the two silent-wrong-output gaps. *(Also fold in the `--multicore 2` and PE value-equality checks from "Validation sufficiency" #3/#4 if cheap.)*

### Optional
4. **Record why post-construction mutation was rejected** (one line in Alternatives/Self-Review) so the design intent — keep CB/UR assembly inside the builders alongside NM..XG — survives future "simplification."
5. **Note in Assumptions that parsing is post-`fix_id`** (whitespace already collapsed to `_`). Harmless for alphanumeric barcodes/UMIs, but stating it pre-empts confusion about a name like `BC_UMI_read 1` becoming `BC_UMI_read_1`.
6. **Resolve the two Open questions before implementing** (they're already low-risk defaults): silent-skip on malformed (matches fork) and `UR` (raw) not `UB`. Recommend keeping both defaults; just flip them from "Open" to "Resolved — default kept" so they don't linger.
7. **`enabled()` is `&self` on a `Copy` type** — trivially fine; consider `self` by value for a `Copy` struct to read more idiomatically, but not worth a churn. clippy won't complain either way.

---

## Conformance with the resolved decisions

Both user-resolved decisions are respected and I did not relitigate them:
- **Gate = downstream-correct tags, not fork byte-identity.** The validation section is built around tag *presence/value by name* + a `samtools view` fixture, exactly right. The one residual risk under this gate is silent-wrong tag *content/placement*, which Important #3 addresses.
- **Scope = aligner crate only.** Verified no suite/container surface is touched; all changes are confined to `cli.rs`/`config.rs`/`output.rs`/`lib.rs` + crate-local tests. The fork's SE `$XA_tag` bug is correctly *not* replicated (the Rust SE builder never had an XA tag to mis-handle).

**Recommendation: APPROVE.** Address Important #1–#3 (small, all confined to the plan text + test list), then proceed to implementation. The architecture, integration points, and the make-or-break split equivalence are all verified correct.
