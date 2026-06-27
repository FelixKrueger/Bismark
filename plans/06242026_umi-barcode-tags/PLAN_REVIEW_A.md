# Plan Review A — `--add_barcode` / `--add_umi` cell-barcode & UMI SAM tags

**Reviewer:** A (independent)
**Plan:** `/Users/fkrueger/Github/Bismark-umi/plans/06242026_umi-barcode-tags/PLAN.md`
**Date:** 2026-06-24
**Verdict:** **APPROVE WITH CHANGES.** The plan is technically sound, the integration points are real and correctly located, and the design (one insertion point per builder, `RunConfig`-clone propagation to multicore) is well-chosen. Every code claim I checked was accurate. The gaps are in the *parse contract* and *validation coverage*, not the wiring — and one of them (the `/1` / whitespace-underscore interaction) could silently produce wrong tags on real SeekGene PE data.

---

## Verification against the real code

I verified the plan's integration claims line-by-line against the worktree (`HEAD` on `rust/umi-barcode`). All confirmed:

| Plan claim | Code reality | Status |
|---|---|---|
| SE builder `single_end_sam_output` ~L341, tags NM/MD/XM/XR/XG ~L427–440, `id` in scope | `output.rs:341`; tag block `output.rs:427–440`; `id` at L414 | ✅ exact |
| PE builder `paired_end_sam_output` ~L453 | `output.rs:453` | ✅ exact |
| Shared inner `build_pe_mate` ~L584, tags ~L652–665, called once per mate with same `id` | `output.rs:584`; tags `652–665`; called L536 & L556 both passing `id` | ✅ exact |
| Existing tag order is NM/MD/XM/XR/XG | `output.rs:427–440` and `652–665` | ✅ exact |
| SE call site `lib.rs:1022`, passes `config.phred64` | `lib.rs:1022`, passes `config.phred64` | ✅ exact |
| PE call site `lib.rs:3061`, passes `config.phred64`, `dovetail` | `lib.rs:3061` | ✅ exact |
| `config: &RunConfig` in scope at both call sites | Yes — both are inside `route_se_decision` (`lib.rs:986`, `config` L996) and `route_pe_decision` (`lib.rs:3008`, `config` L3024) | ✅ |
| `RunConfig` siblings `ambiguous`/`ambig_bam`/`combined_index` | `config.rs:215/219/243`; mapping at `config.rs:429–447` (`phred64`/`ambiguous`/`ambig_bam`/`combined_index` from `cli.*`) | ✅ (struct fields are not at the exact L215/219 the plan cites for the *bools* — those line numbers point at the doc comments, but the fields and mapping are present and the pattern matches) |
| `cli.rs` clap-derive `#[arg(long = "...")]` underscored style | `cli.rs` e.g. `#[arg(long = "non_directional")]` L97, `#[arg(long = "combined_index")]` L113 | ✅ exact |
| `--multicore` reuses lib.rs process fns + clones `RunConfig` | `parallel.rs` calls `crate::process_se_chunk`/`process_pe_chunk` (L427, L487); `cfg = config.clone()` at L644 & L792 | ✅ |
| Nothing in the parallel/merge path strips or reorders tags | `merge_bams` (`parallel.rs:523`) uses `record_bufs` + `write_raw_record` (L531–535) — verbatim passthrough; `merge_aux_gz` is FastQ only | ✅ |
| `Data` tag-map type for the helper | Re-exported at `noodles_sam::alignment::record_buf::Data` (`noodles-sam-0.85.0/.../record_buf.rs:16`); `Data::insert(&mut self, tag: Tag, value: Value)` (`.../record_buf/data.rs:222`) | ✅ — helper signature `data: &mut Data` is feasible with **one new import** |
| `Tag`/`Value`/`BString` already imported in `output.rs` | `output.rs:14` (`BString`), L20 (`Tag`), L22 (`Value`) | ✅ |
| Adding a trailing param does not break un-enumerated call sites | The **only** production callers are `lib.rs:1022` and `lib.rs:3061`, both via `route_se_decision`/`route_pe_decision` (the single funnel for direct, combined-index, and multicore-chunk paths). Test callers: `output.rs` L991, L1014, L1064, L1113, L1161, L1197, L1259 (SE ×7) and L1427, L1587 (PE ×2). `combined.rs` references are comments only. | ✅ — **9 test call sites** to update, not "every" vaguely; enumerated here |

**Net:** the plan's "clean slate / single insertion point / multicore is free" architecture is fully borne out by the code. No hidden call site, no tag-stripping merge path, no missing import beyond `Data`.

---

## Logic review

### 1. The `splitn(3, '_')` ↔ Perl `split(/_/, $id, 3)` equivalence — mostly correct, one subtle gap

The plan's edge-case table is **correct for the rows it lists**. Rust `splitn(3, '_')` and Perl `split(/_/, $id, 3)` agree on:
- `BC_UMI_rest_a_b` → `["BC","UMI","rest_a_b"]` ✅
- `nounderscore` → `["nounderscore"]`; `it.next()` for UMI → `None` → `unwrap_or("")` → empty → no UR ✅
- `_UMI_rest` → `["","UMI","rest"]`; barcode empty → no CB ✅ (Rust `splitn` yields a leading empty field, same as Perl)
- `BC_` → `["BC",""]`; UMI empty → no UR ✅
- `BC_UMI` → `["BC","UMI"]` ✅

**However**, there is one divergence the plan does not call out, and it matters for the *guard semantics* (not the listed rows):

- Perl `split(/_/, $id, 3)` **strips trailing empty fields** only when the limit is omitted or 0; with an explicit limit of 3 it does **not** strip trailing empties, so this is actually consistent with Rust here. ✅ (I confirmed this is fine — but the plan asserts equivalence without stating *why* it holds despite Perl's trailing-empty-stripping reputation. Worth a one-line note so a future maintainer doesn't "fix" it.)
- The fork's actual guard is `append only if defined & non-empty`. Rust's `unwrap_or("")` + `!is_empty()` reproduces "defined & non-empty" faithfully for fields 0 and 1. ✅

This row is **correct but under-explained** — see Important item I-1.

### 2. PE tag equality — confirmed correct, but the `/1` provenance is unverified

The plan claims both mates get identical CB/UR because they share the QNAME. **Confirmed in code**: `paired_end_sam_output` passes the *same* `id` to both `build_pe_mate` calls (`output.rs:537`, L557), and the PE call site passes `identifier` (`lib.rs:3062`), which is R1's `fix_id`-normalised, `@`/`>`-stripped header (`lib.rs:2934`). So both mates are tagged from R1's name → identical tags. ✅

**But** — and this is the one place the plan's parse could silently produce a *wrong* tag — I traced how `identifier` is built and found two interactions the plan ignores:

1. **`/1` suffix is NOT stripped from the merge `identifier`.** The `/1`/`/2` stripping in the crate (e.g. `write_raw_pe_ambig_lines`, `seq_id`) applies to *SAM-stream* qnames, **not** to the FastQ-header-derived `identifier` used for the BAM output name. If a SeekGene R1 header is `BC_UMI_readname/1`, the merge `identifier` is `BC_UMI_readname/1`, the BAM QNAME is `BC_UMI_readname/1`, and `splitn(3,'_')` → `["BC","UMI","readname/1"]`. CB/UR are still correct (the `/1` lands in the ignored remainder), **but the BAM QNAME itself would carry `/1`** — which is a *pre-existing* aligner behavior, not introduced here. This is fine for the tags but worth confirming the *real* SeekGene names don't put the `/1` somewhere that breaks the contract (e.g. `BC_UMI/1_readname`, which would corrupt UR). See Critical C-1.

2. **`fix_id` collapses whitespace runs to a single `_`** (`convert.rs:82–96`, default non-`--icpc` path). So a header like `BC_UMI_readname extra description` becomes `BC_UMI_readname_extra_description` *before* the split. The barcode/UMI prefix is unaffected (it's before any whitespace), so CB/UR stay correct — **but** if the SeekGene prep ever emits a space *inside* the first three fields, those spaces become `_` and shift the field boundaries. Low probability for well-formed SeekGene names, but the plan's edge-case table is silent on whitespace→underscore, which is the single biggest "silent wrong tag" risk because it is invisible in the QNAME-as-typed. See Critical C-1.

### 3. Early-exit / no-op path — correct

`if !opts.enabled() { return; }` before any split → zero added cost and no record-layout change on the default path. Confirmed the helper is called *after* the XG insert and *before* `from_noodles_record`, so default output is byte-identical. ✅ This protects the existing Perl-oracle / worker-invariance gates.

### 4. Out-of-scope boundaries — confirmed in code

- `--ambig_bam`: raw lines via `write_raw_sam_line_to_bam` / `write_raw_pe_ambig_lines` (`lib.rs:1041`, `lib.rs:3082`; `output.rs:674`) — these never call the builders, so no tags leak. ✅
- `--unmapped` / `--ambiguous`: FastQ aux via `write_se_aux_record` / PE equivalents (`lib.rs:1050`, L3088) — not BAM, no tags. ✅
- The builders are reached **only** via `Decision::UniqueBest` / `DecisionPaired::UniqueBest` (`lib.rs:1004`, L3030). The scope boundary the plan claims is exactly the code boundary. ✅

---

## Assumptions

| Assumption (plan) | Assessment |
|---|---|
| Read names follow `BARCODE_UMI_<rest>` | **Stated but unverified against a real SeekGene FastQ.** This is the load-bearing assumption and the plan has no fixture proving the real name shape. See C-1. |
| PE mates share the QNAME; both tagged | **Confirmed in code.** ✅ |
| "Non-empty field" guard matches fork | **Confirmed** for fields 0/1 via `unwrap_or("") + !is_empty()`. ✅ |
| `CB`/`UR` are `Z` tags | Correct per 10x convention. ✅ |
| Underscore flag spellings | Confirmed matches `cli.rs` convention. ✅ |
| Tag order is functionally irrelevant | **Correct** for `samtools view` / `umi_tools` / pysam lookups (all key by tag name). ✅ The plan's decision to drop byte-identity is justified. |

**Missing assumption:** the plan does not state whether **both** mates *should* carry the UMI for `umi_tools dedup --paired`, or only R1. See I-2 — this is a correctness question for the downstream consumer, not just a style choice, and the plan asserts "Required for `umi_tools --paired`" without a citation.

---

## Efficiency

No concerns. O(len(QNAME)) split, ≤2 small allocations only when a flag is on, guarded behind the early return. `BarcodeUmiTags` is `Copy` and threaded by value — negligible. Default path is provably unchanged. The plan's efficiency section is accurate and the self-review is honest. ✅

---

## Validation sufficiency

This is the **weakest part of the plan** given there is *no byte-identity oracle* for this feature. The proposed tests (helper unit tests, flag matrix, malformed-name table, PE-both-mates, one fixture, regression suite) are a good start but have gaps:

**Gaps:**

1. **No real-SeekGene-name fixture.** Every test uses synthetic names (`BC_UMI_rest`). The single biggest risk — that the *real* QNAME shape differs from `BARCODE_UMI_<rest>` — is untested. At minimum, capture one real SeekGene read header (even pasted into a comment) and assert the parse against it. (C-1)

2. **Whitespace-in-name not tested.** Given `fix_id` collapses whitespace to `_` *before* the split, a test with a space in the header (`BC UMI rest` → `BC_UMI_rest` → correct tags; but `BC_UMI rest_x` → `BC_UMI_rest_x`) would document the interaction and catch a regression if `fix_id` ever changes. (I-1)

3. **The "neither flag → byte-identical to pre-change golden" assertion (Validation §2) needs a concrete golden.** The plan says "byte-identical to the pre-change golden" but does not name *which* existing test golden. The existing tests assert on `r.inner()` field-by-field, not raw BAM bytes. Clarify whether the no-flag assertion is "the existing assertions still pass" (mechanical, compiler+test-driven — sufficient) vs. a new raw-bytes golden (overkill). I recommend the former, stated explicitly. (I-3)

4. **`--multicore` not in the validation list at all.** The plan *argues* multicore is free (correctly), but proposes no test that runs the fixture with `--parallel 2` and asserts the tags survive the `merge_bams` round-trip. Since the whole point is downstream-correct tags and `merge_bams` is a separate code path (`record_bufs` → `write_raw_record`), one cheap integration assertion (`--add_barcode --add_umi --parallel 2`, then `samtools view`, confirm CB/UR present on every aligned record) would close the only untested production path. (I-4)

5. **No assertion that tags are absent on `--ambig_bam` in the *fixture*.** Validation §5 says "and none on `--ambig_bam`" — good, but this needs an input that actually *produces* an ambiguous alignment, which a tiny hand-built fixture may not. Either construct such a read or downgrade this to a unit-level assertion that the ambig path doesn't call the helper. (Optional O-1)

**What's sufficient:** the helper unit tests + flag matrix + malformed table genuinely cover the parse logic, and the compiler + existing test suite cover the "no-flag byte-identity" regression. The mechanical call-site churn is compiler-caught. Those parts are fine.

---

## Alternatives worth considering

1. **Parse once, thread the parsed pair — not the flags.** The plan threads `BarcodeUmiTags{add_barcode, add_umi}` (the flags) into the builders and re-parses the QNAME inside each builder. For PE this means the QNAME is split **twice** (once per `build_pe_mate` call) for the same insert. Negligible cost, but an alternative is to parse `(barcode, umi)` *once* at the `route_*_decision` level and pass `Option<&str>` pairs down. Trade-off: the plan's approach keeps the parse co-located with the insert (clearer, one helper), at the cost of a redundant PE split. **Recommendation: keep the plan's approach** — clarity wins, the double-split is free. Noting it only because the plan's self-review claims "no duplicated logic" while the PE path does run the parse twice. (Cosmetic.)

2. **`splitn(3, '_')` vs. a single `split_once('_')` chain.** Equivalent for fields 0/1 (the only fields used). `splitn(3,...)` is fine and most faithful to the Perl. No change needed.

3. **Validating the QNAME shape vs. silent skip.** The plan defers the "warn on malformed" question (open Q). Given there is no oracle and the failure mode is *silent missing tags*, a **`--add_umi` set but field 1 empty on the first N reads → one STDERR notice** (Bismark's never-silent convention, used elsewhere in this crate, e.g. the multicore memory warning at `parallel.rs:580`) would be cheap insurance against a user pointing the flags at non-SeekGene data and getting an empty-tag BAM with no signal. I'd promote this from "open" to "do it" — see I-5.

---

## Action items

### Critical
- **C-1 — Pin the real SeekGene QNAME shape before implementing.** The entire feature rests on `BARCODE_UMI_<rest>` *and* on the barcode/UMI sitting in fields 0/1 *after* `fix_id` (whitespace→`_`) and *with* any `/1`/`/2` still attached (it is **not** stripped from the merge `identifier`; confirmed `lib.rs:2934`). Obtain one real SeekGene FastQ R1 header (or BAM QNAME) and confirm: (a) no whitespace inside the first three fields, (b) `/1` (if present) lands in the remainder not in field 0/1, (c) the barcode is field 0 and UMI is field 1 (not reversed). Add it as a fixture/unit assertion. Without this, the plan could ship a silently-wrong-tag aligner that passes every synthetic test. *(Owner: confirm with the SeekGene write-up / a sample file.)*

### Important
- **I-1 — Document the `fix_id` whitespace→`_` interaction** in the Behavior section and add a unit test with a space in the header. The parse runs on the *post-`fix_id`* name, so a space anywhere in the first three logical fields shifts boundaries. (`convert.rs:82–96`.)
- **I-2 — State and justify "tag both mates" for `umi_tools dedup --paired`.** The plan asserts it's "required" without citation. Confirm `umi_tools` expects the UMI tag on *both* mates (it does for `--paired`, but the plan should say so, ideally with the umi_tools doc reference, since this is a correctness claim with no oracle).
- **I-3 — Make the no-flag regression assertion concrete.** Clarify it means "all existing `output.rs` builder tests pass unchanged once they pass `BarcodeUmiTags::default()`" (mechanical, sufficient) — not a new raw-BAM-bytes golden. Name it so the implementer doesn't over-build.
- **I-4 — Add a `--multicore`/`--parallel 2` integration assertion.** `merge_bams` (`parallel.rs:523`) is a distinct passthrough path (`record_bufs`→`write_raw_record`); a one-line fixture run with `--add_barcode --add_umi --parallel 2` + `samtools view` confirming CB/UR on every aligned record closes the only untested production path. The plan argues this is free but tests none of it.
- **I-5 — Promote the "warn on malformed name" open question to a decision.** Given no oracle and a silent-missing-tag failure mode, emit one STDERR never-silent notice (the crate's convention, cf. `parallel.rs:580`) when a flag is set but its field is empty on early reads. Cheap, and it surfaces "wrong data, wrong flags" immediately.

### Optional
- **O-1 — Either build an input that genuinely produces an ambiguous alignment for the "no tags on `--ambig_bam`" fixture assertion, or downgrade it** to a code-level assertion that the ambig path doesn't reach the helper (it doesn't — `write_raw_sam_line_to_bam` is separate). As written, a tiny fixture may never exercise the ambiguous branch, making §5's negative assertion vacuous.
- **O-2 — Note in Behavior that the PE QNAME parse runs twice** (once per `build_pe_mate`); harmless, but the self-review's "no duplicated logic" is slightly inaccurate. Cosmetic.
- **O-3 — `splitn(3, '_')` trailing-empty equivalence:** add a one-line comment that Perl `split(/_/, $id, 3)` *with an explicit limit* does NOT strip trailing empties (so `BC_` → `["BC",""]` in both languages), pre-empting a future "fix."

---

## Summary

The wiring is correct and verified — single insertion point per builder, `config` in scope at both call sites, multicore propagation via `RunConfig` clone, no tag-stripping merge path, `Data`/`Tag`/`Value`/`BString` all available (one `Data` import to add), and the 9 test call sites enumerated. The two resolved decisions (downstream-correct gate; aligner-only scope) are respected and well-justified by the code (consumers key by tag name; `--ambig_bam`/FastQ paths provably bypass the builders).

The risk is **not** in the integration — it is in the **parse contract assumption** (C-1: the real SeekGene name shape, the un-stripped `/1`, and the `fix_id` whitespace→`_` collapse all feed the same `splitn`) and in **validation coverage** for the untested production paths (I-4 multicore round-trip) and the silent-failure mode (I-5 never-silent notice). Address C-1 and I-4/I-5 and this is a safe, low-churn change.
