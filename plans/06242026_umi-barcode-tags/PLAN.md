# PLAN — `--add_barcode` / `--add_umi`: cell-barcode & UMI SAM tags (Rust aligner)

**Status:** drafted 2026-06-24; **dual plan-review APPROVED** (A + B), findings folded in (rev 1); **parse contract verified against real `altos-labs/SeekSoulMethyl` source** (rev 2, 2026-06-24); awaiting implementation trigger.
**Branch / worktree:** `rust/umi-barcode` @ `/Users/fkrueger/Github/Bismark-umi` (based off `origin/rust/iron-chancellor` @ `61446e7`)
**Source of requirement:** SeekGene modified-Bismark write-up (`bismark_umi_barcode_changes`, fork of Perl Bismark v0.25.1)
**Reviews:** `PLAN_REVIEW_A.md` (APPROVE w/ changes), `PLAN_REVIEW_B.md` (APPROVE w/ minor fixes)

---

## Goal

Add two opt-in boolean flags to the Rust Bismark aligner (`bismark_rs`) that copy a **cell barcode** and a **UMI** out of each read's QNAME into 10x-Genomics-convention SAM/BAM tags on the primary aligned output:

| Flag | Tag written | Convention | Downstream consumer |
|------|-------------|------------|---------------------|
| `--add_barcode` | `CB:Z:<barcode>` | error-corrected cell barcode | `step3_split_bams.py` (per-cell BAM splitting) |
| `--add_umi`     | `UR:Z:<umi>`     | raw UMI | `umi_tools dedup --extract-umi-method tag --umi-tag UR --paired` |

Mirrors the SeekGene Perl fork's `build_barcode_umi_tags` feature but implemented **cleanly**: the acceptance bar is *downstream-correct tags* (consumers look tags up by **name**, so column order is functionally irrelevant), **not** byte-identity with the fork. The fork's latent SE `$XA_tag` bug is intentionally **not** replicated (and the Rust SE builder never had an XA tag to mishandle).

**Resolved decisions (user, 2026-06-24):**
- **Fidelity gate:** correct downstream tags (unit tests + a small BAM fixture), *not* byte-identity vs the SeekGene fork.
- **Scope:** aligner crate only (`bismark_rs`); no suite/container surface in this change.
- **Malformed QNAME (flag set but field empty):** skip the tag (no crash) **and** emit **one never-silent STDERR summary line per run** (Bismark never-silent convention). [Rev 1 — was "silent skip"; user chose the notice.]
- **Contract validation:** **verified against the real `altos-labs/SeekSoulMethyl` pipeline source** (rev 2). The real name the modified Bismark sees is **4 underscore-fields** — `<barcode>_<umi>_<alt>_<original-illumina-name>` (`seeksoultools/utils/barcode.py:393`), e.g. `AACGTGAT_TTGCAA_1N3T_VL00347:237:AAJCLHTM5:1:1101:32054:1000`. Barcode is field 0 and UMI is field 1 in both the 4-field and the 3-field (`barcode_umi_name`) layouts — confirmed by SeekSoul's own re-parser `addtag.py:63` (`bc, umi_raw, alt, readname = query_name.split("_", 3)`). `_alt` is a barcode-correction signature (`1N3T`/`M`/empty); the original name uses **colons, not underscores**, so it never leaks into fields 0/1. The plan's `splitn(3,'_')` extracts `(barcode, umi)` correctly for every variant. [Rev 2]

---

## Context

- **Crate:** `rust/bismark-aligner` (binary `bismark_rs`).
- **CLI surface:** `rust/bismark-aligner/src/cli.rs` — clap derive (`#[derive(Parser)]`). Flag spellings keep underscores (`--non_directional`, `--combined_index`), so the new flags are `--add_barcode` / `--add_umi`.
- **Runtime config:** `rust/bismark-aligner/src/config.rs` — `RunConfig` *struct definition* holds the wired flags as plain bools (siblings: `ambiguous` L215, `ambig_bam` L219, `combined_index` L243). The **`Cli → RunConfig` mapping** is the `Ok(RunConfig { … })` block at **`config.rs:412`**, with sibling assignments `ambiguous: cli.ambiguous` / `ambig_bam: cli.ambig_bam` at **L429–447**. [Rev 1 — corrected from the earlier "~L201–220", which is the struct def, not the mapping.]
- **SAM record assembly:** `rust/bismark-aligner/src/output.rs`
  - **SE:** `single_end_sam_output` (L341); tag block `NM/MD/XM/XR/XG` at **L427–440**; `id` (QNAME) in scope at L414.
  - **PE:** `paired_end_sam_output` (L453) builds both mates via a **single shared inner builder** `build_pe_mate` (L584); its tag block is at **L652–665**. `build_pe_mate` is invoked once per mate (`output.rs:537` rec1, `output.rs:557` rec2) with the **same `id`**, so one insertion point tags **both R1 and R2**.
- **Call sites (the only two production builder callers; all SE/PE/combined-index/multicore paths funnel here):**
  - SE: `lib.rs:1022` inside `route_se_decision` (`lib.rs:986`; `config: &RunConfig` at L997; `counters: &mut Counters` in scope — `methylation_call` just above takes it).
  - PE: `lib.rs:3061` inside `route_pe_decision` (`lib.rs:3008`; `config: &RunConfig` at L3024; `counters` in scope).
  - Reached **only** on `Decision::UniqueBest` / `DecisionPaired::UniqueBest`.
- **`--multicore` (`parallel.rs`):** reuses the same `lib.rs` process functions (`process_se_chunk`/`process_pe_chunk`, L427/L487) and clones `RunConfig` (`let mut cfg = config.clone();` L644/L792). New bool fields propagate to workers with no parallel-specific code. `merge_bams` (L523) reads each part with `record_bufs` and `write_raw_record` **verbatim** (L531–535) — optional tags survive in order. Per-worker `Counters` are merged via `Counters::merge`.
- **PE QNAME provenance (verified):** the `identifier` passed to `paired_end_sam_output` is built from the **R1 FastQ header** directly (`lib.rs:2934`: `fix_id` + `@`/`>`-strip), *not* the merge key. Bismark's internal `/1/1` suffix (`convert.rs:197`) is added only to reads sent to Bowtie 2 and stripped during merge (`align.rs:504`, `strip_suffix("/1")`) — it **never reaches the builder QNAME**. So the name reaching the split is the original read name.
- **No pre-existing barcode/UMI code** anywhere in the crate — clean slate. `Tag`/`Value`/`BString` are already imported in `output.rs` (L14/L20/L22); **`Data` is NOT** — one import to add (see outline).
- **Real pipeline context (verified `altos-labs/SeekSoulMethyl`, internal copy of `seekgene/SeekSoulMethyl`):** `modules/step2.nf` runs Bismark **paired-end** twice per sample — a directional "forward" pass and a `--pbat` "reverse" pass — both with `--parallel 8 --add_barcode --add_umi`. Barcode/UMI are written into the read name *upstream* by `seeksoultools/utils/barcode.py` (10x-style chemistry: barcode+UMI extracted from R1's sequence); the genomic mates are what Bismark aligns. Downstream, `bin/utils/step2_umi_tools_dedup.py` runs `umi_tools dedup --extract-umi-method tag --umi-tag UR --paired`, and `bin/utils/addtag.py` re-parses the name post-alignment to add `CB`/`CR`/`UB`/`UR` (so Bismark's `UR` is what umi_tools dedups on). All of this matches the plan: Bismark must emit `CB`=field0, `UR`=field1 on the PE records of both library types.

---

## Behavior

**Read-name contract:** `<barcode>_<umi>[_<alt>]_<original-name>` (real SeekSoul format, verified `barcode.py:393`/`addtag.py:63`) — parsed identically to the fork's `split(/_/, $id, 3)`: barcode = field 0, UMI = field 1, everything else lumped into the ignored remainder. **Note:** the parse runs on the **post-`fix_id`** QNAME, i.e. after whitespace runs have been collapsed to `_` (`convert.rs:76`). Harmless here — the real barcode/UMI/alt fields are alphanumeric and the original name is colon-delimited (no underscores, no spaces in the first three fields).

1. **Early exit:** if neither `--add_barcode` nor `--add_umi` is set → no-op; output byte-identical to the current aligner.
2. **Parse:** `id.splitn(3, '_')` → field 0 = barcode, field 1 = UMI, field 2 = remainder (untouched; preserves further underscores).
3. **CB:** if `--add_barcode` AND field 0 non-empty → insert `CB:Z:<field0>`.
4. **UR:** if `--add_umi` AND field 1 present AND non-empty → insert `UR:Z:<field1>`.
5. **Never-silent notice (Rev 1):** when a flag is set but its field is empty, count the occurrence; emit **one** STDERR summary line per run if the count > 0 (e.g. `WARNING: --add_umi set but N read(s) had no UMI field (QNAME not BARCODE_UMI_…) — UR tag omitted for those reads`). One line per flag, once per run — not per read.
6. **Placement:** tags appended **after `XG`** → `NM, MD, XM, XR, XG, [CB], [UR]`. Order is not functionally significant (consumers key by name); `Data::insert` appends a new tag, so the order is deterministic.
7. **PE:** both mates carry identical, non-empty `CB`/`UR` (same QNAME → same barcode/UMI). Required for `umi_tools dedup --paired` (reads the UMI tag; does not error if both mates carry it) and per-cell `CB` splitting (both mates must carry the barcode so neither is orphaned) — matches the 10x/CellRanger convention of CB/UB on every record of a pair.

**Edge cases (fail-soft; "append only if defined & non-empty", matching the fork — verified Rust `splitn`/Perl `split` byte-identical on these incl. the empty-middle and trailing-empty rows):**

| QNAME | `--add_barcode` | `--add_umi` |
|-------|-----------------|-------------|
| `AACGTGAT_TTGCAA_1N3T_VL00347:237:…:1000` (**real 4-field**) | `CB:Z:AACGTGAT` | `UR:Z:TTGCAA` (`1N3T_VL00347…` ignored) |
| `BC_UMI__VL00347:237:…:1000` (empty `_alt`) | `CB:Z:BC` | `UR:Z:UMI` (`_VL00347…` ignored) |
| `BC_UMI_rest_a_b` | `CB:Z:BC` | `UR:Z:UMI` (remainder `rest_a_b` ignored) |
| `nounderscore` | `CB:Z:nounderscore` | no field 1 → **no UR** (+notice) |
| `_UMI_rest` (leading `_`) | field 0 empty → **no CB** (+notice) | `UR:Z:UMI` |
| `BC_` (trailing `_`) | `CB:Z:BC` | field 1 empty → **no UR** (+notice) |
| `BC_UMI` (no remainder) | `CB:Z:BC` | `UR:Z:UMI` |
| `BC__rest` (empty middle field) | `CB:Z:BC` | field 1 empty → **no UR** (+notice) |

**Out of scope (no tags emitted) — verified the builders are never reached on these paths:**
- `--ambig_bam` — raw Bowtie2 lines via `write_raw_sam_line_to_bam` / `write_raw_pe_ambig_lines` (`output.rs:674`).
- `--unmapped` / `--ambiguous` — FastQ aux (`write_se_aux_record` etc.), not BAM.

---

## Signature

New, in `output.rs`:

```rust
use noodles_sam::alignment::record_buf::data::Data; // NEW import (Tag/Value/BString already present)

/// 10x-convention cell-barcode/UMI tag toggles parsed from the QNAME (`BARCODE_UMI_<rest>`).
/// Mirrors SeekGene's `build_barcode_umi_tags`. `Copy`; cheap to thread through builders.
#[derive(Clone, Copy, Default, Debug)]
pub struct BarcodeUmiTags {
    pub add_barcode: bool, // --add_barcode → CB:Z:
    pub add_umi: bool,     // --add_umi     → UR:Z:
}

impl BarcodeUmiTags {
    #[inline]
    pub fn enabled(self) -> bool { self.add_barcode || self.add_umi }
}

/// Split a QNAME into (barcode, umi) per `BARCODE_UMI_<rest>` (split on `_`, max 3).
/// Either field may be empty. SHARED by the tag-inserter (below) and the call-site
/// missing-field counter, so the two never diverge.
pub fn parse_barcode_umi(id: &str) -> (&str, &str) {
    let mut it = id.splitn(3, '_');
    (it.next().unwrap_or(""), it.next().unwrap_or(""))
}

/// Append `CB:Z:`/`UR:Z:` to `data` based on `opts`. No-op if neither flag is set;
/// inserts a tag only when its parsed field is non-empty.
fn append_barcode_umi_tags(data: &mut Data, id: &str, opts: BarcodeUmiTags) {
    if !opts.enabled() { return; }
    let (barcode, umi) = parse_barcode_umi(id);
    if opts.add_barcode && !barcode.is_empty() {
        data.insert(Tag::from(*b"CB"), Value::String(BString::from(barcode)));
    }
    if opts.add_umi && !umi.is_empty() {
        data.insert(Tag::from(*b"UR"), Value::String(BString::from(umi)));
    }
}
```

Builder signature changes (append `opts: BarcodeUmiTags` as a new trailing param — builders stay otherwise pure record constructors; counting lives in the orchestration layer):
- `single_end_sam_output(id, original_seq, qual, best, ext, methylation_call, refid, phred64, opts)`
- `paired_end_sam_output(id, seq_1, seq_2, qual_1, qual_2, best, ext, methcall_1, methcall_2, refid, phred64, dovetail, opts)` → forwards `opts` into both `build_pe_mate(...)` calls.
- `build_pe_mate(..., phred64, opts)` (already `#[allow(clippy::too_many_arguments)]`).

`Counters` (in `report.rs`/wherever the struct lives): add `pub add_barcode_missing: u64` and `pub add_umi_missing: u64`; include both in `Counters::merge`.

---

## Implementation outline

1. **`cli.rs`** — add two fields in the output/tag group:
   ```rust
   /// Write the CB:Z: tag (error-corrected cell barcode parsed from QNAME `BARCODE_UMI_...`).
   #[arg(long = "add_barcode")]
   pub add_barcode: bool,
   /// Write the UR:Z: tag (raw UMI parsed from QNAME `BARCODE_UMI_...`).
   #[arg(long = "add_umi")]
   pub add_umi: bool,
   ```
2. **`config.rs`** — add `pub add_barcode: bool` / `pub add_umi: bool` to `RunConfig`; in the `Ok(RunConfig { … })` mapping at **L412** (next to `ambiguous`/`ambig_bam`, L429–447) set `add_barcode: cli.add_barcode, add_umi: cli.add_umi`. Add:
   ```rust
   impl RunConfig {
       pub fn barcode_umi_tags(&self) -> BarcodeUmiTags {
           BarcodeUmiTags { add_barcode: self.add_barcode, add_umi: self.add_umi }
       }
   }
   ```
3. **`output.rs`** — add the `Data` import, `BarcodeUmiTags`, `parse_barcode_umi`, and `append_barcode_umi_tags` (see Signature). `BarcodeUmiTags`/`parse_barcode_umi` are `pub` (used by `config.rs`/`lib.rs`); the inserter stays private.
4. **`output.rs` (SE)** — add `opts` to `single_end_sam_output`; call `append_barcode_umi_tags(rec.data_mut(), id, opts)` immediately after the `XG` insert (~L440), before `from_noodles_record`.
5. **`output.rs` (PE)** — add `opts` to `paired_end_sam_output`; forward into both `build_pe_mate` calls. Add `opts` to `build_pe_mate`; call `append_barcode_umi_tags(rec.data_mut(), id, opts)` after the `XG` insert (~L665). **Single point covers both mates.**
6. **`Counters`** — add `add_barcode_missing` / `add_umi_missing` `u64` fields + extend `Counters::merge`.
7. **`lib.rs` (SE call site, L1022)** — `let tags = config.barcode_umi_tags();` then, before/after the builder call:
   ```rust
   if tags.enabled() {
       let (b, u) = output::parse_barcode_umi(identifier);
       if tags.add_barcode && b.is_empty() { counters.add_barcode_missing += 1; }
       if tags.add_umi && u.is_empty() { counters.add_umi_missing += 1; }
   }
   let record = single_end_sam_output(identifier, …, config.phred64, tags)?;
   ```
8. **`lib.rs` (PE call site, L3061)** — same `tags`/counter check on `identifier` (counted once per pair, not per mate), then pass `tags` to `paired_end_sam_output`.
9. **Run-end notice** — after the main alignment loop completes and per-worker `Counters` are merged (alongside the final report emission), if `add_barcode_missing > 0` / `add_umi_missing > 0`, `eprintln!` one summary line each. Gate on the respective flag being set (so a count of 0 stays silent; the flag-off path never increments anyway).
10. **Unit-test call sites** — update every existing call of the two builders in `output.rs` tests (SE ×7: L991/L1014/L1064/L1113/L1161/L1197/L1259; PE ×2: L1427/L1587 — grep to reconfirm) to pass `BarcodeUmiTags::default()`. Keeps current field-by-field assertions intact (no-flag path unchanged).

---

## Efficiency

O(len(QNAME)) split, ≤2 small `BString` allocations only when a flag is on, guarded behind the early return / `enabled()` check. Default (flagless) path: one bool check, no split, no allocation, record layout unchanged → existing Perl-oracle / worker-invariance gates untouched. The QNAME is split at most twice per read (once at the call site for counting, once in the builder for the insert) via the shared `parse_barcode_umi`; both are allocation-free `&str` walks — negligible vs. alignment/IO. `Data::insert`'s new-tag append is an O(≤7) scan — negligible.

---

## Integration

- **Writes:** up to two optional `Z`-type tags per `UniqueBest` record; up to two STDERR summary lines per run. **Reads:** QNAME only.
- **Order vs. other tags:** appended last; does not perturb `NM/MD/XM/XR/XG` bytes.
- **Downstream:** `CB` → `step3_split_bams.py`; `UR` → `umi_tools dedup --umi-tag UR --paired`.
- **`--multicore`:** `RunConfig` clone carries the bools; `Counters::merge` aggregates the missing-field counts; the notice fires once on the merged total. Worker-invariance preserved (tags are a pure function of each read's QNAME).
- **Default behavior (no flags):** byte-identical to the current aligner.

---

## Assumptions

- Read names follow `<barcode>_<umi>[_<alt>]_<original-name>` (real SeekSoul format, verified in `seeksoultools/utils/barcode.py:393` and `addtag.py:63`); barcode = field 0, UMI = field 1 in both the 4-field and 3-field layouts. *Configurable:* opt-in via the flags; **off by default**.
- Parsing happens on the **post-`fix_id`** QNAME (whitespace already collapsed to `_`); fields 0/1 are assumed whitespace-free (true for alphanumeric barcodes/UMIs).
- PE mates share the QNAME (verified: single `identifier` → both mates); both tagged identically.
- "Append only if defined & non-empty" matches the fork (verified `splitn`/`split` equivalence).
- `CB`/`UR` are `Z` (string) tags; `--add_umi` emits **`UR`** (raw), not `UB` (corrected) — per the write-up. [Resolved — default kept.]
- Flag spellings use underscores (`--add_barcode`, `--add_umi`).

---

## Validation

1. **Unit (parse/insert):** `BC_UMI_rest_a_b` both flags → `CB:Z:BC`, `UR:Z:UMI`; assert remainder `rest_a_b` not consumed.
2. **Unit (flag matrix):** `--add_barcode` only → CB present, UR absent; `--add_umi` only → vice versa; **neither** → no CB/UR and existing field-by-field builder assertions unchanged (this *is* the no-flag regression — mechanical, not a new raw-BAM golden).
3. **Unit (malformed table, incl. `BC__rest` empty-middle and `_UMI_rest` leading-empty):** exactly the skip behavior in the edge-case table; assert the corresponding `add_*_missing` counter increments.
4. **Unit (PE):** both R1 and R2 records carry **equal AND non-empty** `CB`/`UR` (presence-only would miss an empty/garbage-barcode bug).
5. **Unit/integration (notice):** flag set + a malformed read → `add_*_missing` > 0 and exactly one STDERR line emitted per run (not per read).
6. **Integration (fixture, single-core):** tiny **PE** FastQ using **realistic SeekSoul names** — `AACGTGAT_TTGCAA_1N3T_VL00347:237:AAJCLHTM5:1:1101:32054:1000` (and an empty-`_alt` `BC_UMI__name` variant) on both mates → `bismark_rs -1 … -2 … --add_barcode --add_umi`; `samtools view` shows `CB:Z:AACGTGAT`/`UR:Z:TTGCAA` on aligned records (alt/original-name correctly excluded). Exercise `--pbat` too (step2.nf uses it).
7. **Integration (`--ambig_bam` clean):** run with `--add_barcode --add_umi --ambig_bam` on an input that yields an ambiguous alignment; assert **zero** `CB:`/`UR:` in the `.ambig.bam` (explicit, not vacuous).
8. **Integration (`--multicore`):** same fixture with `--parallel 2`; assert CB/UR present on every aligned record of the merged BAM and record count matches the single-core run (proves `merge_bams` preserves tags + order).
9. **Regression:** full aligner suite green; `cargo fmt -p bismark-aligner -- --check` and `clippy -p bismark-aligner -- -D warnings` clean. Mind the separate `cargo fmt --check` CI job and `clippy::doc_lazy_continuation` (no `///` line may start with `+`/`-`/`*`).

---

## Questions or ambiguities

- **(Resolved)** Malformed QNAME → skip tag **+ one STDERR notice per run** (user, 2026-06-24).
- **(Resolved)** Contract validated via synthetic tests on the documented `BARCODE_UMI_<rest>` shape; no real-data fixture (user, 2026-06-24).
- **(Resolved)** `--add_umi` emits `UR` (raw), not `UB`.
- **(Resolved)** Gate = downstream-correct tags, not fork byte-identity; scope = aligner crate only (user, 2026-06-24).

---

## Self-Review

- **Efficiency:** early-return keeps the default path cost-free; ≤2 allocs only when flags on; shared `parse_barcode_umi` avoids divergence between counter and inserter. ✓
- **Logic:** one insertion point per builder; PE both-mates covered once via shared `build_pe_mate`; counting in the orchestration layer where `Counters` live. The QNAME is parsed twice per read (call-site counter + builder insert) — deliberate, negligible, and keeps builders pure. ✓
- **Design choice (recorded so it isn't "simplified" later):** tags are assembled **inside** the builders alongside `NM..XG`, not by mutating the record post-construction at the call site. Post-construction mutation was rejected because it would (a) bypass the `BismarkRecord` wrapper / need a new `inner_mut()` accessor, (b) split tag-assembly across two files, and (c) reintroduce per-mate duplication that the shared `build_pe_mate` eliminates.
- **Edge cases:** no/leading/trailing/empty-middle underscore enumerated and unit-tested; `--ambig_bam`/`--unmapped`/`--ambiguous` proven out of scope in code. ✓
- **Integration:** `--multicore` inherits via `RunConfig` clone + `Counters::merge`; no-flag byte-identity preserved. ✓
- **Review adjudication:** Reviewer A flagged the un-stripped `/1` as a Critical silent-wrong-tag risk; Reviewer B (and a direct code read of `lib.rs:2934` + `align.rs:504` + `convert.rs:197`) showed the builder QNAME comes from the R1 FastQ header and Bismark's `/1/1` never reaches it — even a trailing `/1` lands in the ignored remainder. Downgraded to "proceed on documented contract." ✓
- **Real-data verification (rev 2):** the name shape is no longer just "per the write-up" — it is confirmed against the live `altos-labs/SeekSoulMethyl` source (`barcode.py:393` writer + `addtag.py:63` re-parser + `step2.nf` invocation). The supplied S3 FastQ (`S1_G1_methylation_clean_R*`) turned out to be the **pre-barcode-extraction fastp stage** (standard Illumina names), so it could not directly exhibit the contract — but the pipeline source is authoritative and the real format (`barcode_umi_alt_origname`) is now baked into the fixtures. ⚠️ If `--add_*` were ever pointed at *pre-extraction* reads (no `_`), `--add_barcode` would emit a garbage whole-name `CB` **without** tripping the never-silent notice (field 0 is non-empty) — a real, if out-of-contract, footgun. Acceptable for the intended pipeline; noted.
- **Remaining risk:** the only churn is the mechanical call-site updates (compiler-caught) and the new `Counters` fields/merge.

---

## Implementation Notes (2026-06-24)

**Status:** IMPLEMENTED on `rust/umi-barcode`. Local verification all green (see below). Awaiting dual `code-reviewer` + `plan-manager` (verify phase).

**Files changed (aligner crate only, as scoped):**
- `cli.rs` — `--add_barcode` / `--add_umi` clap fields.
- `config.rs` — `RunConfig.add_barcode`/`.add_umi` + `Cli→RunConfig` mapping + `RunConfig::barcode_umi_tags()` accessor.
- `output.rs` — `Data` import; `BarcodeUmiTags` (pub) + `BarcodeUmiTags::enabled()`; `parse_barcode_umi` (pub) + `append_barcode_umi_tags` (private); `opts` param threaded through `single_end_sam_output`, `paired_end_sam_output`, and the shared `build_pe_mate`, with the helper called after the `XG` insert in the SE builder and in `build_pe_mate` (one point → both mates).
- `merge.rs` — `Counters.add_barcode_missing`/`.add_umi_missing` + `Counters::merge` extension.
- `lib.rs` — both `UniqueBest` call sites (`route_se_decision`, `route_pe_decision`) parse via the shared `parse_barcode_umi` to bump the missing-field counters and pass `config.barcode_umi_tags()` to the builder; `push_barcode_umi_notice` helper appended inside `counters_summary`/`counters_summary_pe`.
- Tests: 6 new unit tests — `parse_barcode_umi_splits_max_3_fields`, `se_both_flags_write_cb_and_ur_from_real_name`, `se_flag_matrix_barcode_only_umi_only_neither`, `se_empty_fields_skip_their_tag`, `pe_both_mates_carry_equal_nonempty_cb_ur` (output.rs), `barcode_umi_notice_emitted_when_fields_missing` (lib.rs). Updated the 9 existing builder call sites in `output.rs` tests to pass `BarcodeUmiTags::default()`.

**Deviations from the plan:**
1. **Notice emission site (plan step 9).** The plan said "after the main loop, alongside the final report emission." There are ~10 such sites (single-core SE/PE + `--multicore` + combined-index variants). Implemented instead as `push_barcode_umi_notice` appended inside the two end-of-run STDERR summary formatters `counters_summary`/`counters_summary_pe`, which every driver path (incl. the multicore merge at `parallel.rs:756/921`) funnels through. Single point, gated on `count > 0` (no config needed since the count is only incremented when the flag is set). The byte-gated report *file* is untouched. Cleaner than the plan and strictly covers all paths.
2. **Integration fixtures (plan Validation §6–8: end-to-end alignment with a genome + Bowtie 2, single-core / `--ambig_bam`-clean / `--multicore`).** NOT added as crate-level tests — they need a prepared genome + Bowtie 2 + running the binary, which is the real-data/oxy gate's job (consistent with how prior aligner phases gate end-to-end). The unit tests cover the parse + tag-write + notice logic deterministically; the multicore path is covered structurally (RunConfig clone + Counters::merge are unit-tested, and the tags are a pure QNAME function). **Recommend an oxy real-data smoke** (`bismark_rs --add_barcode --add_umi` PE, directional + `--pbat`, then `samtools view`) before merge — flagged for the verify phase.

**Local verification (worktree `Bismark-umi`, 2026-06-24):**
- `cargo test -p bismark-aligner --lib` → **426 passed, 0 failed** (incl. the 6 new tests).
- `cargo test -p bismark-aligner -- --test-threads=2` → all integration suites green (worker-invariance 100, methylseq 3, etc.), 0 failed.
- `cargo fmt -p bismark-aligner -- --check` → clean.
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → clean.
- `bismark_rs --help` → both `--add_barcode` / `--add_umi` present and documented.
