# PLAN_REVIEW_A — Phase 9b: order-preserving file-level threading (`--multicore`/`--parallel`)

**Reviewer:** A (independent, fresh context)
**Plan:** `plans/05312026_bismark-aligner/phase9b-threading/PLAN.md` (rev 0, 2026-06-03)
**Verdict:** **APPROVE with conditions.** The architecture is sound and the load-bearing claims hold up against the source. Worker-invariance is achievable for the stated reasons, the three invariants (contiguous partition + ordered single-writer merge + commutative counters) are correctly identified, and the central correctness assumption is genuinely the right one. No **Critical** logic defect. Several **Important** items, most about validation rigour and a couple of behavioral divergences (record-1 sanity, STDERR/Perl-argv in the gate) that the plan does not yet name. All are pinnable before the oxy gate.

I verified every "reuse-this-helper" / line-number claim against the actual source. The map is accurate (a couple of trivial drifts noted).

---

## Source verification (what I confirmed)

| Plan claim | Source | Verdict |
|---|---|---|
| `pipeline()` dispatches on `config.layout`, insertion point at `lib.rs:109–114` | `lib.rs:109–114` | ✅ exact |
| `run_se` `228–336`, `run_pe` `715–845` | `lib.rs:228`, `715` | ✅ exact |
| `drive_merge` applies skip/upto (`lib.rs`) | `lib.rs:485–525` | ✅ skip/upto + Perl-falsy `s>0`/`u>0` guards present |
| Converter (`convert_fastq_impl`) ALSO applies skip/upto | `convert.rs:316–327` | ✅ confirmed — same falsy-0 guards |
| `--multicore` in `deferred_flags`; `validate_multicore` ≥1 | `config.rs:335`, `308–317` | ✅ exact |
| Report fns consume only `Counters`; header takes file-names only | `report.rs:46`, `102–163` | ✅ no streaming/order state |
| `write_completion_line(elapsed_secs)` — caller-computed | `report.rs:323` | ✅ orchestrator-owned |
| `Counters` = all-`u64` monotone counts, `#[derive(Default,Clone,PartialEq,Eq)]` | `merge.rs:63–102` | ✅ sum is commutative/associative |
| Perl modulo stripe `($line_count-$offset)%$multicore==0`; offset-order BAM merge | `bismark:169`, `1457–1480` | ✅ confirmed → Perl mc-N ≠ single-core |
| `flate2 = "=1.1.9"` pinned workspace-wide; aux uses `Compression::default()` | `Cargo.toml:28`, `lib.rs:392–398` | ✅ same encoder both paths |
| Extractor: std-threads-not-rayon deadlock lesson | `bismark-extractor/src/parallel.rs` module docs | ✅ accurately summarised |
| 226 existing tests | actual count = **227** (`#[test]` across crate) | ⚠️ trivial drift |

---

## Logic review

### The core model is correct
Worker-invariance reduces to three invariants, all of which I confirmed are satisfied:

1. **Contiguous partition** — `split_contiguous` writes each effective read to exactly one chunk, in order. Concatenating chunks in order == the effective input. ✅
2. **In-order single-writer merge** — `merge_bams` copies per-chunk records in chunk order through one writer; `merge_aux_gz` re-emits per-chunk plain aux through one encoder in chunk order. Because each read aligns independently of its file-mates (§2.6, validated below), the per-chunk record sequence concatenated in chunk order == the single-core record sequence. ✅
3. **Commutative counter sum** — `Counters` is all monotone `u64` counts; the report is a pure function of the summed `Counters`. Element-wise `+` is order-independent. ✅

This is the same shape as the extractor's invariance proof, correctly adapted (chunk-level, not record-level — the per-instance Bowtie 2 subprocess is the indivisible unit). The "Insight" box (§2.5) explaining why the extractor's reorder buffer does *not* fit is accurate and the right call.

### §2.6 central assumption — SOUND, and the gate would catch a failure loudly
Bowtie 2 seeds its per-read RNG from read name+seq+qual+`--seed` (not file ordinal). I cannot disprove this and it is the standard Bowtie 2 contract; the Phase-0 determinism spike (EPIC line 49) already relied on run-to-run identity without `-p`/`--reorder`/`--seed`. If it were false, the `--parallel 4` vs `--parallel 1` BAM diff would be **non-empty** (a loud failure in the §9 #7 unit gate and the §9 #11 oxy gate), not silent. The risk framing in §11.1 is correct. **No change required** beyond ensuring the gate actually exercises a chunk boundary (see Important-1).

### Q2 (N==1 delegates to `process_chunk`) — LOWER risk than the plan frames it
I traced the concern. The report header/footer and `write_completion_line` are written by the **caller** (`run_se`/`run_pe` lines 290–321, 801–831), and they depend only on file names + the final `Counters` + a caller-computed elapsed value — none of which lives inside the proposed `process_*_chunk`. So a delegation that has `run_se` (N==1) write to the *final* paths and the caller write the report cannot perturb report bytes, temp-cleanup ordering, or the wall-clock line as long as the extraction body is moved verbatim. The **227-test suite + the single-core byte-frozen regression (§9 #8) is sufficient** *provided* one gap is closed: the existing tests must include an assertion over the **report body** and the **aux files** for at least one N==1 cell (not just BAM), or the refactor could silently reorder a `write!` inside the report path. Confirm `tests/cli.rs` already asserts report+aux on a single-core cell; if not, add it before the refactor (Important-3).

### skip/upto (§3.6) — CORRECT, and it deliberately diverges from a Perl quirk (worth stating)
The reasoning is airtight for the *single-core-equivalence* goal, but the plan under-documents *why* it is safe to diverge from Perl here. I read the Perl subset path:

- `subset_input_file_FastQ` (`bismark:153–179`) applies **`--upto`** via a **per-worker** `$seqs_processed == $upto` check, and applies **NO `--skip`** at all. `--skip` is then re-applied by the converter (`biTransformFastQFiles`, `bismark:5590–5591`) and again at the merge read (`bismark:2347`).
- Because `$seqs_processed` counts only the reads *this worker* kept, Perl's `--multicore N --upto U` keeps up to **N×U** reads total — a genuine Perl multicore quirk that already makes Perl mc-N ≠ single-core for `--upto`.

The plan applies skip **and** upto **once, globally, at the split**, then clears both in the per-chunk converter and driver. This yields exactly single-core semantics (the gate target) and avoids the N×U quirk. **This is correct.** Off-by-one check: the converter/driver use `count <= s` to skip and `count > u` to stop, with falsy-0 guards (`convert.rs:316–327`, `lib.rs:514–525`). The splitter must reproduce *that exact* boundary — "effective set = reads `(skip, upto]`" (§3.2 step 1) matches `count > skip && count <= upto`. ✅ **Action (Important-2):** add a unit test that pins the boundary with `--skip 1 --upto 3` over a 5-read input split N=2 (skip drops read 1; upto keeps reads 2,3; chunk 0 = read 2, chunk 1 = read 3), asserting the concatenated subsets == reads 2–3 *and* the per-chunk pipeline runs with skip=0/upto=None. Without this exact test the double-application risk is real because BOTH the converter and the driver still carry live skip/upto code that must be passed zeroed.

### Q4 aux raw-byte identity — SOUND, but the Perl-side comparison is mis-stated
- **Rust `--parallel N` vs `--parallel 1`:** identical. Both use the same pinned `flate2 1.1.9` `GzEncoder` at `Compression::default()`. The single-core path streams routed records incrementally into one encoder; the merge path streams the concatenated plain per-chunk bytes into one encoder. flate2/miniz_oxide deflate output is a pure function of the *byte sequence*, independent of `write_all` call boundaries (the encoder buffers internally and flushes deterministically on `finish()`). Since the concatenated plain per-chunk aux byte sequence == the single-core plain byte sequence (same reads, same order, same formatting via `write_se_aux_record`/`write_pe_aux`), the gz raw bytes are identical. ✅
- **⚠️ vs Perl (the §9 #11 oxy gate):** Perl writes aux via `| gzip -c` (an external gzip, different encoder). So `--parallel 1` aux raw bytes will **NOT** equal Perl's aux raw bytes — only the *decompressed* content matches. The plan's §9 #11 row says "aux" without distinguishing, and §3.5 frames the whole rationale around raw-byte identity. **The gate must compare aux raw-bytes only Rust-vs-Rust (`--parallel N` vs `--parallel 1`), and decompressed-content for Rust-vs-Perl.** This is an Important fix to the validation table, not a logic defect (the design is right; the gate spec is ambiguous). (Important-4)

### Q3 BAM merge — CORRECT for the decompressed-content gate
EPIC lines 101–102 confirm the gate is **decompressed** SAM content, not raw BAM bytes (noodles ≠ samtools encoder). So copying per-chunk records under one shared header via noodles, skipping per-chunk headers, reproduces single-core decompressed content exactly. Header hazard check: every per-chunk BAM is opened with the **same** `&header` (the shared `generate_sam_header` output — `@SQ` from the one genome load, `@PG` from the original argv), so `@SQ`/`@PG`/`SO`/`VN` are identical across chunks; writing one final header from that same shared object and skipping per-part headers is safe. No field hazard. ✅ The open Q3 note ("confirm whether `bismark-io` exposes a record copier or use `noodles_bam::io::Reader`") is fine to defer to implementation.

### PE lockstep split — adequate, with one strengthening
The per-chunk `R1 count == R2 count` assertion (§3.8, §9 #3) catches a *size* desync but not a *content* desync (a splitter bug that wrote R1 read 5 and R2 read 6 into the same chunk with equal counts). In practice the splitter reads both mate files with the same record arity and the same chunk boundaries derived from the **same effective count**, so identical boundaries ⇒ identical reads — but the count-only assertion does not *prove* it. **Mitigation already exists downstream:** `check_results_paired_end` keys the merge on the R1 identifier and the PE driver re-reads both originals in lockstep (`drive_merge_pe:1021–1049`), so a true mate desync would surface as id-mismatch / wrong pairing → a BAM diff in the gate. The count assertion is a cheap early tripwire; keep it, but the **real** proof is the §9 #7 PE worker-invariance byte-diff. Note this in §3.8 so a reviewer doesn't over-trust the count assertion. (Optional)

### Error propagation / orphaned subprocesses — STRONGER than the plan claims
`AlignerStream` and `PairedAlignerStream` both implement `Drop` that does `child.kill()` then `child.wait()` if not finished cleanly (`align.rs:255–261`, `461–466`). Combined with `std::thread::scope` (which joins *all* spawned threads before the scope returns, even on panic/early-return), a worker that errors mid-pipeline will drop its owned streams → its Bowtie 2 children are killed+reaped, and the scope joins the siblings before the orchestrator surfaces the first error. So the §3.8/§9 #10 guarantee is **already backstopped by existing Drop impls** — the plan should cite `align.rs:255/461` so the implementer doesn't reinvent kill-on-error. One real caveat to pin: `std::thread::scope` *propagates a panic* by re-panicking on join; the orchestrator must convert a worker `Err` into a returned `Result` (not a panic) so the "first error returned" wording in §3.8 holds, and decide whether a worker **panic** (vs `Err`) should abort loudly (acceptable) — the §9 #10 test should assert the *`Err`* path returns cleanly with no orphan, and ideally a second case that a panic doesn't leave orphans either. (Important-5)

---

## Validation sufficiency (the project's recurring false-pass trap)

This is where I have the most to add. The plan correctly names the trap (§11.2) but the §9 table has gaps that could let a false-pass through:

1. **§9 #7 is the real gate and MUST span a chunk boundary at every N.** Good that it specifies "count NOT divisible by N." But add the explicit requirement that the test feeds a count where **at least two chunks are non-empty AND a `UniqueBest`, an `Ambiguous`, and a `NoAlignment` read each fall on *both sides* of a chunk boundary** — otherwise a merge bug that only mis-handles, say, the aux routing of the first read of a non-first chunk could pass. The fake-bt2 harness can be seeded to force each decision class. (Important-1)
2. **The fake-bt2 caveat (§5 step 6) is load-bearing and under-specified.** "The fakes work per chunk unchanged" is true *only if* the fake aligns based on the **converted-read content**, not on a line-ordinal / `NR%4` pattern (the exact Phase-8 `*BS_CT*`-only and Phase-9a `NR%4`/`^@` fakes that previously false-passed, per MEMORY). Since each chunk has a *different* converted file with reads at *different* ordinals, a fake that keys on ordinal will produce DIFFERENT alignments per chunk and the test would either spuriously fail or (worse) spuriously pass if the fake is symmetric. **The plan must require the fake-bt2 to be content-addressed (decision determined by the read sequence/name), and a test must assert that the SAME read produces the SAME fake alignment regardless of which chunk/ordinal it lands in** — that single assertion is what actually closes the false-pass hole for the unit gate. (Important-1, same item)
3. **§9 #6 raw-vs-decompressed must be split Rust-vs-Rust (raw) and Rust-vs-Perl (decompressed)** — see Q4 above. (Important-4)
4. **§9 #11 oxy gate: guard the Perl argv.** The script must invoke **Perl WITHOUT `--multicore`** while invoking Rust with `--parallel 4`. If the harness reuses the phase9a pattern of passing the *same* `"${ARGS[@]}"` to both binaries, and `--parallel 4` ends up in `ARGS`, Perl would run mc-4 → reordered output → the gate diffs and *fails for the wrong reason* (or, if someone "fixes" it by also filtering order, silently passes). The plan should state: Rust gets `--parallel N`; Perl gets the identical argv **minus** `--parallel`. (Important-6)
5. **STDERR is explicitly not gated (§3.7)** — fine, but the gate scripts pipe stderr to logs and check exit codes only; confirm the harness does not accidentally diff a `.log`. (Optional)

---

## Behavioral divergences the plan does not yet name

1. **Record-1 FastQ sanity check fires per-chunk.** `convert_fastq_impl` does `if count == 1 && (!fixed_id.starts_with(@) || !id2.starts_with(+)) { return Err(... "doesn't seem to be in FastQ format at sequence 1") }` (`convert.rs:344–349`). In single-core this fires once on the whole file's read 1. In the chunked path each chunk's converter sees *its own* read 1, so the check fires N times (once per chunk's first read). For **valid** FastQ this is inert (byte-invisible — the check never trips). But: (a) the error **message** is hard-coded `"sequence {count}"` with `count==1`, so a malformed read that is read #5000 of the file but read #1 of chunk 2 would be reported as "sequence 1" — a *different* error message/position than single-core, and the error could be raised by a *different* chunk than the global read order implies. This only matters on malformed input (an error path, not the byte-gated happy path), but it IS a divergence from single-core error reporting. The plan should acknowledge it in §3.8 edge cases (the happy-path gate is unaffected; the error-message text is best-effort/STDERR-class). (Optional, bordering Important if error-path fidelity is in scope — it appears NOT to be, given the gate is happy-path.)
2. **`--gzip` per-chunk converted temps + the subset-file gz-ness.** §3.2 step 2 says subset files "may be written plain." Confirm the per-chunk converter is then invoked treating the plain subset as input regardless of the *original* input's gz-ness (it keys gz off the path suffix — `convert.rs:275`), so plain subset files must NOT carry a `.gz` suffix. The naming scheme `<basename>.temp.<chunk>[.<ext>]` (§3.2) must NOT append `.gz` for a gz original if the bytes are plain, or the converter will try MultiGzDecoder on plain bytes and fail. **Pin:** subset suffix tracks the *actual on-disk encoding* of the subset, not the original. (Important-7)

---

## Efficiency

No concerns. Genome loaded once and borrowed read-only (no `Arc` needed under `std::thread::scope` — correct, the borrow outlives the scope). The 2-pass count (Q1) decompresses gz once extra — negligible vs alignment, as stated. Up-to-4N concurrent Bowtie 2 index loads is inherent to the file-level model (matches Perl); the no-cap + memory-estimate warning (Q5) is a reasonable, byte-invisible choice. One nit: the memory estimate `n × instances × index-size` (§3.7) is an *upper* bound (all N chunks peak simultaneously, which is the realistic worst case) — fine to publish as "peak."

---

## Alternatives considered (all correctly rejected by the plan)
- Per-chunk gz aux + member-concat: decompressed-identical only, fails raw-byte Rust-vs-Rust — correctly rejected (Q4).
- Raw-BGZF-block BAM concat: would couple to noodles' block layout and is unnecessary given the decompressed-content gate — correctly rejected (Q3).
- rayon pool: the extractor's documented deadlock — correctly avoided in favour of `std::thread::scope` (§2.5).
- RAM-aware auto-cap: diverges from Perl + cross-platform RAM dep — correctly rejected (Q5).

No missing alternative worth surfacing.

---

## Action items

### Critical
*(none — no output-byte-changing defect; scope/model are sound and source-verified.)*

### Important
1. **Harden §9 #7 against false-pass.** Require the worker-invariance unit test to (a) feed a count where ≥2 chunks are non-empty AND each decision class (`UniqueBest`/`Ambiguous`/`NoAlignment`) appears on both sides of a chunk boundary, and (b) require the **fake-bt2 to be content-addressed** (alignment determined by read seq/name, not line ordinal / `NR%4`), with an explicit test that the same read yields the same fake alignment regardless of chunk/ordinal. This single content-addressing assertion is what closes the recurring false-pass hole.
2. **Pin the skip/upto boundary with a dedicated unit test** (e.g. `--skip 1 --upto 3` over 5 reads, N=2): assert the concatenated subsets == reads 2–3 AND that the per-chunk converter+driver are invoked with skip=0/upto=None (double-application is a live risk — both `convert.rs:316` and `lib.rs:514` still carry skip/upto code).
3. **Before the Q2 refactor, confirm `tests/cli.rs` asserts the report BODY and aux files (not just BAM) on a single-core cell;** if it does not, add it. The N==1-delegation safety argument relies on those bytes being regression-guarded.
4. **Split §9 #6 / §3.5 raw-vs-decompressed semantics:** aux **raw-byte** identity is a Rust-vs-Rust property (`--parallel N` vs `--parallel 1`); against the **Perl** oracle, aux can only be **decompressed-content** identical (Perl uses external `gzip`). State this in the validation table and the gate script.
5. **Spell out the `std::thread::scope` error contract:** worker `Err` must be converted to a returned `Result` (not a panic) so "first error returned" holds; cite the existing `Drop` kill+reap on `AlignerStream`/`PairedAlignerStream` (`align.rs:255/461`) as the orphan backstop; the §9 #10 test should assert no orphaned Bowtie 2 process on the `Err` path (and ideally on a panic path).
6. **Oxy gate (§9 #11): Perl must be invoked WITHOUT `--multicore`** while Rust runs `--parallel N`. Do not pass `--parallel`/`--multicore` verbatim to the Perl oracle (it would striped-reorder and fail the diff for the wrong reason). The phase9a gate passes identical `"${ARGS[@]}"` to both — the 9b script must strip the parallel flag from the Perl argv.
7. **Subset-file suffix must track actual on-disk encoding,** not the original input's gz-ness: if subset bytes are written plain, the temp name must NOT carry `.gz` (the converter keys gz off the path suffix, `convert.rs:275`), else MultiGzDecoder is applied to plain bytes.

### Optional
8. Note the **record-1 FastQ sanity check fires per-chunk** (`convert.rs:344`) → on malformed input the error message/position ("sequence 1") and the reporting chunk differ from single-core. Happy-path gate is unaffected; acknowledge in §3.8 as an error-path/STDERR-class divergence.
9. In §3.8, clarify that the PE `R1==R2 count` assertion is an early tripwire only; the actual mate-desync proof is the §9 #7 PE byte-diff (id-keyed merge would surface a content desync).
10. Trivial: "226 tests" → **227** (current actual count).
11. Confirm the gate harness does not diff STDERR `.log` files (STDERR is explicitly not gated, §3.7).

---

## Bottom line
The plan is **implementation-ready after the Important items are folded in** (most are validation-spec tightenings, not design changes). The three invariants are correct and source-verified, the central Bowtie 2 per-read-independence assumption is sound and would fail loudly (not silently) if wrong, and the deliberate divergence from Perl's striped multicore layout is the right call and matches the EPIC's worker-invariance gate. The dominant residual risk is a **false-passing test**, exactly as the plan itself flags — Important-1 and Important-2 are the items that actually retire that risk.
