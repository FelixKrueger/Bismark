# Code Review A — `--add_barcode` / `--add_umi` (Rust aligner)

**Reviewer:** A (independent; ran in parallel with Reviewer B, no coordination)
**Scope:** uncommitted impl on `rust/umi-barcode` @ `/Users/fkrueger/Github/Bismark-umi`, crate `rust/bismark-aligner`.
**Diff:** `git -C /Users/fkrueger/Github/Bismark-umi diff origin/rust/iron-chancellor -- rust/bismark-aligner`
**Spec:** `plans/06242026_umi-barcode-tags/PLAN.md`
**Gate (per task brief):** downstream-correct tags, NOT byte-identity vs the SeekGene fork.

## Verdict

**APPROVE.** The implementation is correct, faithful to the (verified) parse contract, and surgically scoped so the default (no-flag) path is provably unchanged. No Critical/High findings. Three Low observations + one Medium documentation/footgun note (already acknowledged in the plan, restated here for the verify phase). I did **not** modify any files (per the brief — two reviewers share the worktree).

## Verification re-run (this reviewer, sandbox-escalated for the sibling-worktree target dir)

- `cargo test -p bismark-aligner --lib` → **426 passed, 0 failed**, incl. all 6 new tests (`parse_barcode_umi_splits_max_3_fields`, `se_both_flags_write_cb_and_ur_from_real_name`, `se_flag_matrix_barcode_only_umi_only_neither`, `se_empty_fields_skip_their_tag`, `pe_both_mates_carry_equal_nonempty_cb_ur`, `barcode_umi_notice_emitted_when_fields_missing`).
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → clean.
- `cargo fmt -p bismark-aligner -- --check` → clean.

Matches the plan's "Local verification."

---

## Findings by area

### 1. Logic / correctness — PASS

- **`parse_barcode_umi` (`output.rs:60`)** — `id.splitn(3, '_')` then `(it.next().unwrap_or(""), it.next().unwrap_or(""))`. Correct: barcode = field 0, umi = field 1, remainder ignored. `splitn` with limit 3 keeps trailing empties (`"BC_"` → `("BC", "")`) — Rust `splitn` and Perl `split(/_/, $id, 3)` agree on every row of the edge-case table (verified the test `parse_barcode_umi_splits_max_3_fields` exercises empty-middle `BC__name`→`("BC","UMI")`... actually `BC_UMI__name`→`("BC","UMI")`, leading `_UMI_rest`→`("","UMI")`, trailing `BC_`→`("BC","")`, no-`_` `nounderscore`→`("nounderscore","")`, empty `""`→`("","")`). All match the spec table.
- **`append_barcode_umi_tags` (`output.rs:77`)** — early-returns on `!enabled()` (zero work, zero alloc on the default path), then inserts CB/UR **only when the parsed field is non-empty**. Correct "append only if defined & non-empty" semantics. Called once after the XG insert in **both** `single_end_sam_output` (`output.rs:494`) and `build_pe_mate` (`output.rs:724`) — the single PE insertion point covers both mates because `paired_end_sam_output` forwards the same `opts` into both `build_pe_mate` calls (`output.rs:610`, `output.rs:631`) with the same `id`. Verified.
- **Tag placement / `Data::insert` semantics** — confirmed against noodles-sam-0.85.0 `record_buf/data.rs:222`: `insert` is *append-on-new-key, replace-on-existing-key*, preserving push order for new keys (`self.0.push(field)`). Bismark builds each record fresh with only NM/MD/XM/XR/XG, so CB then UR are always new keys appended after XG, deterministic order `NM,MD,XM,XR,XG,[CB],[UR]`. No replacement risk; no ordering perturbation of the byte-gated tags.
- **Call-site missing-field counting (`lib.rs:1022` SE, `lib.rs:3072` PE)** — guarded by `barcode_umi.enabled()`, then per-flag (`add_barcode && bc.is_empty()`, `add_umi && umi.is_empty()`). Counted **once per read (SE) / once per pair (PE)** — the PE counter runs on `identifier` (not per-mate), exactly as specified. Reached only inside the `UniqueBest` / `DecisionPaired::UniqueBest` arm, so ambiguous/unmapped paths never count or tag. Both call sites use the shared `parse_barcode_umi`, so the counter and the inserter cannot diverge. Correct.
- **All driver paths funnel correctly** — I traced `route_se_decision` callers (`lib.rs:957` main SE, `:1493` generic-stream, `:1875` combined-index) and `route_pe_decision` callers (`:2984`, `:3585`, `:4035`); every SE/PE/combined-index/multicore path routes through these two shared functions, so the counting + tagging are reached on **all** of them with no per-driver duplication. `parallel.rs` reuses `process_*_chunk` which call the same routers; the new `RunConfig` bools propagate to workers via the existing `RunConfig` clone with no parallel-specific code.
- **PE QNAME provenance** — verified `identifier` is built from the R1 FastQ header (`lib.rs:2945`: `fix_id` + prefix-strip), **not** the Bowtie2 merge key, so Bismark's internal `/1/1` suffix never reaches the builder QNAME (the plan-review's `/1` Critical was correctly downgraded). The same `identifier` reaches both mates. The Reviewer-A-of-the-*plan* concern is moot in code.

### 2. Never-silent notice — PASS (verifies the brief's key ask)

- `push_barcode_umi_notice` (`lib.rs:~4085`) appends one STDERR WARNING line per flag-with-misses, gated on `count > 0` (and the count is only ever incremented when the flag is set, so no config gate is needed — clean). Appended inside `counters_summary` (`lib.rs:4132`) and `counters_summary_pe` (`lib.rs:4107`).
- **Does NOT touch the byte-gated report file.** Confirmed the separation: the report file is written via the `report` module (`report::write_report_header` / `print_final_analysis_report_*` at the `_SE_report.txt`/`_PE_report.txt` sites), while `counters_summary*` is consumed **only** via `eprintln!` (STDERR) at every call site (`lib.rs:679/1213/1313/1619/2030/2264/2711/3255/3362/3712/4276/4506` + `parallel.rs:756/921`). The notice cannot reach the byte-gated file. This is the cleaner deviation #1 in the plan, and it is correct.
- **Fires once per run on ALL paths incl. `--multicore`** — the parallel merge accumulates per-worker `Counters` into `total` (`Counters::merge` extended for both new fields, `merge.rs:171–172`) and calls `counters_summary*` on the merged `total` (`parallel.rs:756/921`). One notice per run on the merged count. Verified by the `barcode_umi_notice_emitted_when_fields_missing` test (SE summary: nothing→no WARNING; umi-only→exactly the UMI line; PE summary with both→both lines, with the counts surfaced).

### 3. Default-path / byte-identity preservation — PASS

- No-flag path: `enabled()` is `false` → `append_barcode_umi_tags` early-returns before any split/alloc; the call-site counter block is skipped entirely. Record layout is byte-for-byte the prior NM..XG. The 9 existing builder call sites in `output.rs` tests were updated to pass `BarcodeUmiTags::default()` and their field-by-field assertions are unchanged — this *is* the no-flag regression guard. Worker-invariance/Perl-oracle gates are untouched because the default emits identical bytes.

### 4. Efficiency — PASS

- Default path: one bool check, no split, no alloc. Flagged path: O(len QNAME) allocation-free `&str` split done at most twice per read (call-site counter + builder insert) via the shared helper, ≤2 small `BString` allocs only when a non-empty field is present. `Data::insert`'s new-key path is an O(≤7) linear scan over the existing tags — negligible. No concerns.

### 5. Structure / style — PASS

- Naming consistent with the crate (`BarcodeUmiTags`, `parse_barcode_umi`, `append_barcode_umi_tags`, `barcode_umi_tags()` accessor mirror the `ambiguous`/`ambig_bam` siblings). `BarcodeUmiTags` is `Copy`/`Default`/`Debug`, threaded by value — appropriate for a 2-bool struct. The `RunConfig::barcode_umi_tags()` accessor keeps the `output`→`config` coupling one-directional. Doc comments are accurate and reference the verified SeekSoul sources. `clippy::too_many_arguments` already allowed on `build_pe_mate`. Borrow/lifetime: `parse_barcode_umi` returns `&str` slices borrowed from the input `id` — fine, callers use them immediately. No issues.

---

## Recommendations (priority-ordered)

### Medium — documented footgun: pre-extraction reads emit a garbage whole-name `CB` without tripping the notice

If `--add_barcode` is ever pointed at reads that have **not** been through SeekSoul barcode-extraction (standard colon-delimited Illumina names, no `_`), `parse_barcode_umi` returns the whole name as field 0 (non-empty), so a junk `CB:Z:<whole-read-name>` is written and the never-silent notice does **not** fire (the missing-field counter only increments on an *empty* field). The plan's Self-Review already calls this out and accepts it as out-of-contract for the intended pipeline. **No code change recommended** — but flag it for the verify phase / release notes so a future user who mis-points the flag isn't silently mis-tagged. (The `--add_umi` side is self-protecting: a name with no `_` yields an empty UMI field → notice fires.) Within the settled "downstream-correct tags for the SeekSoul pipeline" gate, this is acceptable.

### Low — fixture/integration tests (plan Validation §6–8) intentionally deferred to the oxy gate

Deviation #2 in the plan: the end-to-end fixture (genome + Bowtie 2), `--ambig_bam`-clean, and `--multicore` integration tests were not added as crate tests (they need a prepared genome + binary run — the real-data gate's job). The unit tests cover parse + tag-write + notice deterministically, and the multicore path is covered structurally (RunConfig clone + `Counters::merge` unit-tested; tags are a pure QNAME function). This is consistent with how prior aligner phases gate end-to-end. **Recommend the oxy real-data smoke before merge** (PE directional + `--pbat`, then `samtools view` to confirm `CB`/`UR` on aligned records, zero CB/UR in any `.ambig.bam`, and a `--parallel 2` run matching single-core record counts). Flagged, not blocking the code review.

### Low — `--ambig_bam` out-of-scope is asserted only by the plan, not by a crate test

The plan (Validation §7) wants an explicit "zero `CB:`/`UR:` in `.ambig.bam`" assertion. In code this is guaranteed because the ambig path uses `write_raw_sam_line_to_bam` / `write_raw_pe_ambig_lines` and never reaches the builders (the `Decision::Ambiguous` arm at `lib.rs:1048` does not call `append_barcode_umi_tags`). That is sound by construction, but the explicit negative assertion lives only in the deferred oxy smoke. Fine to leave to the gate; noting for completeness.

### Low — notice wording is slightly imprecise on the trigger condition

The WARNING text says `QNAME not '<barcode>_<umi>_...'` for an *empty* field, but the trigger is specifically an **empty parsed field** (leading `_`, or fewer than the expected underscores), not "name doesn't match the pattern" in general (a no-`_` name like `nounderscore` trips the UMI notice but its barcode field is non-empty). Cosmetic; the count and the omitted-tag statement are accurate. No change required.

---

## Summary

The change does exactly what the brief and plan specify, with the two documented deviations being improvements (single-point notice in the STDERR summary funnel; integration deferred to the oxy gate). Parse semantics, both-mates PE coverage, once-per-read/pair counting, the never-silent notice (STDERR-only, once-per-run, all drivers incl. `--multicore`), and default-path byte-identity are all correct and (re-)verified green. The only standing item is the **Medium** pre-extraction-`CB` footgun, which is already acknowledged and acceptable within the settled gate. **Approve; recommend the oxy real-data smoke as the pre-merge gate.**
