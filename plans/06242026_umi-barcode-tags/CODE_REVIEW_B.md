# Code Review B — `--add_barcode` / `--add_umi` cell-barcode & UMI SAM tags

**Reviewer:** B (independent)
**Date:** 2026-06-24
**Worktree:** `/Users/fkrueger/Github/Bismark-umi` (branch `rust/umi-barcode`)
**Diff under review:** `git diff origin/rust/iron-chancellor -- rust/bismark-aligner`
**Spec:** `plans/06242026_umi-barcode-tags/PLAN.md`
**Files:** `src/cli.rs`, `src/config.rs`, `src/output.rs`, `src/merge.rs`, `src/lib.rs`

---

## Verdict

**APPROVE.** No Critical or High findings. The implementation is correct, worker-invariant,
provably no-op on the default path, and matches the spec including both documented deviations.
All my focus-angle concerns (thread-safety under `--multicore`, count/tag-site agreement,
double-parse divergence, edge cases, non-UTF8, no-flag byte-identity, test quality) check out.
The findings below are all Low/Medium polish items — none blocks merge.

Verification I re-ran locally (all green):
- `cargo test -p bismark-aligner --lib` → **426 passed, 0 failed** (incl. all 6 new tests, confirmed by name).
- `cargo fmt -p bismark-aligner -- --check` → exit 0.
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → exit 0.

---

## What I verified (and why it's correct)

### Thread-safety / worker-invariance under `--multicore` — CORRECT
- Per-worker `process_se_chunk`/`process_pe_chunk` (`lib.rs:375`/`2559`) mutate only their own
  `&mut Counters`; they emit **no** `counters_summary`. The two new counters
  (`merge.rs:135-136`) are summed in `Counters::merge` (`merge.rs:171-172`).
- The merge driver builds `total` by folding every worker's counters
  (`parallel.rs:729-732` SE, mirrored at the PE merge) and emits a **single**
  `counters_summary(read_file, &total)` (`parallel.rs:756`, PE `:921`).
- I enumerated every `eprintln!("{}", counters_summary…)` call site (grep): single-core SE
  (`lib.rs:679/1213/1313/1619/2030/2264`), single-core PE
  (`:2711/3255/3362/3712/4276/4506`), and the two multicore merges (`parallel.rs:756/921`).
  **Every** driver path funnels through `counters_summary`/`counters_summary_pe`, and the
  notice is appended *inside* those formatters (`lib.rs:4108/4133` via `push_barcode_umi_notice`
  `:4085`). So the notice fires **exactly once per run** on the merged total — it cannot
  double-fire (workers don't summarize) and cannot be dropped (no driver bypasses the
  summary). The missing-field count is an exact sum and is therefore worker-invariant.

### Count-site vs. tag-write-site agreement — EXACT COMPLEMENTS
- Count (call site, `lib.rs:1023-1030` SE / `:3073-3080` PE): increment iff `enabled()` AND
  `add_barcode && bc.is_empty()` (resp. umi).
- Write (`output.rs:81-87`): insert iff `opts.add_barcode && !barcode.is_empty()` (resp. umi).
- Both call the **same** `parse_barcode_umi(identifier)` on the **same** `identifier`.
  Field empty ⇒ counted, not written. Field non-empty ⇒ not counted, written. These are
  exact complements: **no read is both counted and tagged, and none is omitted-but-uncounted.**
- The count block sits in the `UniqueBest` arm with **no early return between it and the
  builder call**, and the SE/PE length-guard `return Ok(())` (`lib.rs:1008-1015` / `3044-3059`)
  fires *before* the count block — so a length-guard-skipped read produces no record and is
  correctly neither tagged nor counted. Verified.

### Double-parse of the QNAME — no divergence
- `parse_barcode_umi` (`output.rs:60`) is the single source of truth, called once at the call
  site (count) and once inside the builder (write). Both are allocation-free `&str` walks on the
  identical input string. They cannot diverge because they share the function and the input.

### Non-UTF8 QNAME — handled upstream, cannot panic
- `identifier: &str` is always built via `String::from_utf8_lossy(id_bytes).into_owned()`
  (`lib.rs:932`, and identically at `:1454/1771/2394/…` across every driver). Non-UTF8 bytes
  are already replaced with U+FFFD *before* the builder, consistent with how the existing
  QNAME is emitted into the BAM. So `splitn` and `BString::from(&str)` are always on valid
  UTF-8 — no panic risk, no new failure mode.

### No-flag path provably unchanged — byte-identical
- `append_barcode_umi_tags` early-returns at `output.rs:78` when `!enabled()`, so **nothing is
  pushed** to `Data`. `Data` in noodles-sam 0.85 is `Vec<(Tag, Value)>` and `insert` pushes a
  new key to the end (`record_buf/data.rs:222-232`) — CB/UR are always new keys, so when flags
  are on they append after `XG` and never reorder existing tags; when off, nothing happens.
  The existing tag block already relies on this same `insert` ordering. The default path adds
  one bool check (`enabled()`), no split, no alloc, no record-layout change → existing
  Perl-oracle / worker-invariance gates untouched.

### Out-of-scope paths — confirmed not reached
- `--ambig_bam` and `--unmapped`/`--ambiguous` route through `write_raw_*`/`write_*_aux_record`
  in the non-`UniqueBest` arms (`lib.rs:1048-1062`, PE `:3100+`), which never call the builders
  — so no CB/UR there, matching the spec.

### Combined-index single-pass ("tagged") path — barcode parse sees the CLEAN name
- This was my one real worry (the `__CT`/`__GA` qname tag). Confirmed safe: in
  `process_se_chunk_combined_nondir_tagged`, `identifier` is built from the **re-read input
  header** (`lib.rs:2391` `fix_id(chomp(id))`), NOT from the tagged Bowtie2 stream; the tag is
  stripped per-record (`:2424` `strip_conv_tag`) before routing. All driver variants build
  `identifier` the same way and funnel through `route_se_decision`/`route_pe_decision`, so
  barcode/UMI parsing is correct on **every** path (single-core, multicore, combined-index
  directional/non-dir model-(a)/single-pass, pbat).

### Test quality — real assertions, not tautologies
- `pe_both_mates_carry_equal_nonempty_cb_ur` (`output.rs:1213`) asserts each mate's CB/UR
  equals a specific **non-empty literal** (`"AACGTGAT"`/`"TTGCAA"`) — proves equal AND
  non-empty, the exact thing the focus called for. `pe_io` builds valid genomic windows so the
  builder is genuinely exercised (not a no-op skip).
- `se_empty_fields_skip_their_tag` covers `nounderscore` (umi empty → no UR), `_UMI_rest`
  (barcode empty → no CB), and the easy-to-get-wrong `BC__rest` empty-middle (umi empty → no UR).
- `barcode_umi_notice_emitted_when_fields_missing` asserts the count surfaces in the text
  ("3 read(s)"), that only the relevant warning appears, and that zero counts stay silent.

---

## Findings

### Low-1 — PE notice wording says "read(s)" but counts pairs
`push_barcode_umi_notice` (`lib.rs:4086-4104`) emits `"… {N} read(s) had an empty … field"`.
On the PE path the counter is bumped **once per pair** (`lib.rs:3073-3080`, by design — see the
plan's "counted once per pair, not per mate"), while the tag is omitted on **both** mates. So a
PE run reports N *pairs* using the word "read(s)", which slightly under-describes the number of
output records missing the tag (2N). The count itself is correct and worker-invariant; only the
noun is imprecise. The SE path is exact. *Recommendation (optional):* leave as-is (the count is a
useful, well-defined "reads/pairs with a malformed name") or reword to "read pair(s)" on the PE
formatter. Not worth a behavior change.

### Low-2 — `--add_barcode` on pre-extraction (no-underscore) reads is a silent garbage-CB footgun
Already documented in the plan's Self-Review (rev 2). If the flags are ever pointed at reads
*without* the `barcode_umi` prefix (standard Illumina names, no leading underscore), field 0 is
the **whole name** and non-empty, so `--add_barcode` writes a garbage whole-name `CB:Z:` and the
never-silent notice does **not** fire (the notice only triggers on an *empty* field). This is
out-of-contract for the intended SeekSoul pipeline and the plan accepts it. I confirmed the code
behaves exactly as documented (`se_empty_fields_skip_their_tag` proves `nounderscore` → whole-name
CB). *Recommendation:* none required for this scope; if a future hardening pass wants it, a
heuristic ("flag set but <N reads contained any `_`") could warn — but that's a separate decision,
not a defect here.

### Low-3 — `___` (all-underscores) QNAME not explicitly unit-tested
`parse_barcode_umi("___")` → `("", "")` (both empty → both counted, no tags), which is correct
and fully covered *behaviorally* by the empty-field logic the existing tests exercise. The
explicit table in the tests covers `""`, leading-empty, trailing-empty, and empty-middle, but not
the all-empty multi-underscore case. *Recommendation (optional):* one extra `assert_eq!` line in
`parse_barcode_umi_splits_max_3_fields`. Trivial; no correctness risk.

### Low-4 — Integration fixtures deferred to the oxy gate (documented deviation #2)
The plan's Validation §6-8 (end-to-end alignment with a genome + Bowtie 2, single-core /
`--ambig_bam`-clean / `--multicore`) were not added as crate-level tests; they require a prepared
genome + Bowtie 2 + the running binary. This matches how prior aligner phases gate end-to-end and
is explicitly flagged in the Implementation Notes as needing an **oxy real-data smoke before
merge**. I concur with that recommendation — the unit tests deterministically cover parse + write
+ notice + the structural multicore invariants (clone + merge), but the actual `merge_bams`
tag-preservation and a real PE + `--pbat` + `--multicore 2` run on a genome are only proven by the
gate. *Recommendation:* run the oxy smoke (PE directional + `--pbat`, then `samtools view` for
CB/UR) as part of the verify phase, as the author already flagged.

### Low-5 (style) — `BString::from(barcode)` where `barcode` is `&str`
`output.rs:83/86` use `BString::from(barcode)` (`barcode: &str`). Correct and clear; consistent
with the surrounding `BString::from(genome_conv.as_str())` calls. No action — noting only that
there's no cheaper path (the `Z` tag value must own its bytes).

---

## Notes on items explicitly out of scope (per the task brief, not relitigated)
- Gate = downstream-correct tags (not fork byte-identity) — respected; the no-flag path is the
  only byte-identity claim and it holds.
- SE `$XA_tag` fork bug intentionally not replicated — N/A (the Rust SE builder never had an XA
  tag); confirmed no XA handling anywhere near the new insert point.
- Scope = aligner crate only — confirmed; the diff touches only `rust/bismark-aligner`.

---

## Summary table

| # | Priority | Area | Item |
|---|----------|------|------|
| Low-1 | Low | Logic/wording | PE notice says "read(s)" but counts pairs (count is correct) |
| Low-2 | Low | Edge case | No-underscore reads → silent garbage `CB` (documented, accepted) |
| Low-3 | Low | Test | `___` all-underscores case not explicitly asserted (covered behaviorally) |
| Low-4 | Low | Validation | E2E fixtures deferred to oxy gate — run the smoke before merge |
| Low-5 | Low | Style | `BString::from(&str)` — fine, no action |

No Critical/High/Medium. **Approve for merge after the oxy real-data smoke (Low-4).**
