# Plan Review A — R2: worker-side gzip output (#884)

**Plan:** `PERF_R2_WORKER_OUTPUT_PLAN.md` (rescoped to the `--gzip` path only)
**Reviewer:** A (fresh context, code-grounded)
**Date:** 2026-05-29

Verdict: **Sound core idea, well-scoped by the Phase-0 spike, but the plan has
two concrete Critical gaps that will cause silent or test-visible breakage if
not addressed: (1) the version-header placement under multi-member gzip is
unspecified, and (2) three in-repo tests + the test helper decode with
single-member `GzDecoder`, which truncates multi-member output.** The downstream
Perl consumption is *safe* (it reads via `gunzip -c`, which is multi-member-
correct) — but that safety must be asserted, not assumed.

---

## 1. Logic review

### 1.1 Multi-member gzip correctness — holds, with one unaddressed detail (header)

The core claim is correct: a `flate2::write::GzEncoder` finalized per batch emits
a complete gzip member (header + DEFLATE + CRC32/ISIZE footer) when it is
`finish()`ed or dropped, and the byte-concatenation of independently-finalized
members is a valid RFC-1952 multi-member gzip stream. The collector's job reduces
to `write_all(member_bytes)` per key in `batch_seq` order — a raw append, no
re-compression. This is the same construction `bgzip`/BGZF uses. **Confirmed
sound.**

However the plan does **not** specify where the per-file **version header line**
goes. Today `OutputFileMap::new` (output.rs:126-128) writes `SPLIT_FILE_HEADER`
(`"Bismark methylation extractor version v0.25.1\n"`) into each writer at
eager-open — i.e. it becomes the leading bytes of gzip member 0 (or plain bytes
for `.txt`). Under R2 the worker compresses per-key *call* buffers into members;
the collector appends them. The plan's design section never says:

- Does the collector still open the file and write the header (as an
  uncompressed prelude or as its own gzip member 0) before appending worker
  members? If the collector writes the raw header bytes of `SPLIT_FILE_HEADER`
  directly into a `.gz` file followed by worker-produced gzip members, the file
  is **not** a valid gzip stream (raw text prefix + gzip members → `gunzip`
  errors / `flate2` errors on the leading non-magic bytes).
- Or does the collector emit a tiny header-only gzip member first, then the
  workers' members? That works but must be stated.

This is **load-bearing for byte/content-identity** and for the empty-sweep (a
"header-only" file is what the sweep deletes — see §1.2). It must be pinned in
the plan with the exact mechanism. **Action item C1.**

Relatedly: the plan says `write_call` becomes "format-then-append" and keeps the
single-threaded path working. But the single-threaded `.txt` path is explicitly
out of scope (rescope), and `--gzip` single-threaded (N=1) still flows through
the parallel pipeline (`extract_se_parallel` is used for all N≥1 — see
parallel.rs:186-200, `run_pipeline` with `n_workers = parallel.max(1)`). So
there is **no** surviving "legacy single-threaded gzip writer" — every `--gzip`
run is the worker/collector path. The plan should drop the implication that a
`write_call`-based single-threaded gzip path remains as a tested fallback; it
does not (the only `OutputFileMap::write_call` gzip caller left would be the
direct unit tests in output_modes_phase_e.rs, see §4).

### 1.2 Empty-sweep interaction — needs the header decision resolved first

`finalize_with_empty_sweep` (output.rs:283-342) classifies a file as empty iff
`records_written == 0`, then `drop(writer)` to seal the gzip trailer and
`remove_file`. The doc invariant (output.rs:71-75) is explicit: *"`records_written`
is bumped iff a call row is written ... Any future writer that adds non-call
non-header bytes ... MUST also bump this counter."*

Under R2 the collector no longer calls `write_call` (which is where the counter
bumps today, output.rs:229), so **the plan must define how `records_written` /
the "did any batch write bytes" signal is maintained collector-side**. The plan
says "a file is 'empty' if no batch wrote bytes to it" — good intent, but:

- A `.gz` file that received the header member but **zero call members** must
  still be classified empty and deleted (matches Perl's `was empty -> deleted`
  and today's behaviour where a header-only file is swept). If the collector
  writes a header member unconditionally at open, then "no bytes written" can't
  mean "file size 0" — it must track "any *call* member appended". The counter
  semantics must be ported, not just the file. **Action item C1 (same root).**
- Empty member edge: if a worker emits a batch where a given key got zero calls,
  it should emit **no member** for that key (not an empty-DEFLATE member). An
  empty gzip member is technically valid but wasteful and risks an off-by-one in
  the "did anything write" accounting. The plan should state workers omit
  zero-length keys from `per_key_bytes`. **Action item I1.**

### 1.3 Ordering — correct as designed, matches the existing invariant

The plan requires per-key member concatenation in `batch_seq` order. parallel.rs
already reorders via `BTreeMap<u64, …>` draining `next_emit_seq` in order
(parallel.rs:999-1033), and the module's byte-identity invariant §1 (parallel.rs:36-41)
documents `(batch_seq, within_idx)` as isomorphic to the legacy `input_idx`. R2
keeps the same reorder buffer; it only changes the per-batch payload from
`Vec<WorkerOutputItem>` to per-key byte buffers. As long as the collector appends
each key's member while draining in `next_emit_seq` order, ordering is preserved.
**Confirmed.** The one subtlety: a batch produces ≤12 members (one per key);
the collector must append all of a batch's members before advancing
`next_emit_seq`, which the existing drain loop structure already enforces.

### 1.4 Downstream consumption (the flagged highest-risk interaction) — SAFE, but assert it

Traced end-to-end:

- `state.rs::finalize` → `run_phase_g_chain` is fed `finalization.kept`
  (the kept `.gz` split-file absolute paths) (state.rs:181-191).
- `subprocess.rs::build_bismark2bedgraph_argv` appends those kept paths verbatim
  as the positional tail (subprocess.rs:322-324).
- Perl `bismark2bedGraph` reads each positional infile. **Every** read path of a
  `.gz` infile uses the *system* `gunzip -c`:
  - `:168` `open (READ,"gunzip -c $infile |")` (the `--remove_spaces` pre-pass)
  - `:202` `open (IN,"gunzip -c $infile |")` (main read)
  - `:434` / `:446` `open $ifh, "gunzip -c $in | sort … |"` (the sort path)

  GNU/BSD `gunzip -c` (and `zcat`) **fully concatenate-decompress all members**
  of a multi-member gzip stream — this is RFC-1952 standard and is exactly what
  BGZF relies on. So multi-member input does **not** break `bismark2bedGraph`.
  **The Phase G chain is safe under R2.** Note: Perl never uses
  `IO::Uncompress::Gunzip` here (which would need `MultiStream => 1`), so the
  known Perl single-member gotcha does **not** apply.

  Caveat to verify on colossal: the header skipping. Perl strips the header via
  `next if ($line =~ /^Bismark/)` (`:454`) on the *decompressed line stream*, and
  the `--remove_spaces` path reads exactly one header line with `$_ = <READ>`
  (`:182`). Both operate on the post-`gunzip` concatenated text, so as long as
  the multi-member stream decompresses to `header\n` + call-lines (header only
  once, at the front), Perl behaves identically. This is precisely why the header
  placement (C1) matters: if R2 accidentally emits the header **per member**
  (one per batch), the decompressed text gets N header lines interspersed —
  `:454` filters lines starting `Bismark` so the *main* path is robust, but the
  `--remove_spaces` path (`:182`) consumes exactly one line as the header and
  would mis-handle the rest. **Action item C2: add a downstream multi-member
  test** (and confirm header-once).

### 1.5 `--gzip` is the only place a new `MultiGzDecoder` matters

Because the rescope leaves `.txt` untouched and raw-byte-identical, and the
collector still writes M-bias.txt / splitting_report.txt itself (strict-byte,
unchanged), the *only* behavioural change is the `.gz` data files becoming
multi-member. Good, tight blast radius.

---

## 2. Assumptions

| # | Assumption (stated or implied) | Validity |
|---|--------------------------------|----------|
| A1 | Concatenated per-batch gzip members = valid multi-member gzip | **True** (RFC-1952). |
| A2 | Downstream `bismark2bedGraph` handles multi-member `.gz` | **True** — confirmed it uses system `gunzip -c` at all 4 read sites. Must be *tested*, not just assumed (C2). |
| A3 | "The smoke already accepts sorted-equivalence for `.gz`" | **True** — `phase_h_smoke.sh:271-282` does `zcat \| sort \| md5sum` for `*.gz`; no raw-`.gz` cmp. |
| A4 | No in-repo test asserts raw `.gz` byte-identity | **Partly false / dangerous.** `parallel_phase_f.rs:741` and two phase_e tests assert **exact decoded-content equality** (not sorted) using **single-member `GzDecoder`**. These break under multi-member output. See C3. |
| A5 | A single-threaded `write_call` gzip path survives as a tested fallback | **False** — all N≥1 `--gzip` runs go through `run_pipeline`. The only remaining `write_call`-gzip callers are the phase_e *unit* tests. |
| A6 | The version header handling carries over unchanged | **Unstated / unresolved** — the biggest logic gap (C1). |
| A7 | Memory of per-batch per-key buffers × workers is boundable | Plausible; needs a number. BATCH_SIZE=4096 records × ≤12 keys × compressed bytes × (n_workers × channel-depth `n*4`). Compression shrinks it; should be fine but state the bound (I2). |

---

## 3. Efficiency analysis

- **Expected win is real and large.** The spike measured ~52 s flat single-thread
  `GzEncoder` cost; moving it per-worker parallelizes the dominant term. The
  ~52 s/N projection (N=4 → ~13 s compression, `.gz` total ~30 s) is the right
  order of magnitude. The collector's residual work becomes memcpy + `write_all`
  of already-compressed bytes (small relative to compression).
- **Channel payload size.** Switching from `Vec<RoutedCall>` (which carries
  `Arc<[u8]>` qname clones + small structs) to per-key **compressed** byte
  buffers is generally *smaller* on the wire (gzip shrinks ~3-5×), and removes the
  collector's per-call `write_call` formatting/routing. Net efficiency positive.
  The plan's note to "reuse buffers if profiling shows alloc churn" is the right
  posture — don't pre-optimize.
- **Compression-level consistency.** Workers must use the **same**
  `Compression::default()` as today's `open_writer` (output.rs:393) so the
  *content* is identical and (incidentally) compression ratio/CPU matches the
  spike. Not a correctness issue for sorted-equivalence, but state it (I3).
- **One real efficiency caveat:** per-batch members add gzip header/footer
  overhead (~18 bytes/member) and reset the DEFLATE dictionary every 4096
  records, slightly reducing compression ratio vs one big member. Negligible at
  these sizes, but it means `.gz` file *sizes* will differ from N=1-single-member
  — which is fine because the contract is decoded-sorted-equivalence, not size.
  Worth a one-line note so no one "fixes" a size delta later.

---

## 4. Validation sufficiency — INSUFFICIENT as written; needs additions

The plan's verification list (spike gate, `cargo test`, clippy/fmt, Phase H
matrix, perf re-measure) is necessary but has gaps:

1. **(Critical) Existing tests will break and the plan doesn't flag them.**
   - `tests/parallel_phase_f.rs:741 parallel_gzip_n4_decompresses_identical_to_legacy_plain`
     runs the **real** `--gzip --parallel 4` pipeline and decodes with
     `decompress_gz` → `flate2::read::GzDecoder` (line 349), which reads **only
     the first member**. Under R2 (4 workers, multi-batch) this truncates to the
     first batch's calls and the `assert_eq!(decoded, plain)` fails.
   - The helper `decompress_gz` (line 346-351) must switch to
     `flate2::read::MultiGzDecoder`.
   - `tests/output_modes_phase_e.rs:345` and
     `tests/output_modes_phase_e_smoke.rs:183` also use `GzDecoder`, but those
     drive `OutputFileMap` directly (single member) so they *may* still pass —
     however they should be migrated to `MultiGzDecoder` defensively, or the plan
     must explicitly state they remain single-member.
   - **This is the single most important addition: the plan must enumerate these
     three call sites and prescribe the `MultiGzDecoder` switch.** (C3)

2. **(Critical) No downstream multi-member test.** The plan explicitly puts the
   Phase G chain "out of scope," but R2 *changes the bytes that Phase G consumes*.
   Add at least one test (or a Phase-H cell) that runs `--gzip --parallel 4
   --bedGraph` end-to-end through the real `bismark2bedGraph` and asserts the
   resulting `.bedGraph.gz` / `.bismark.cov.gz` is sorted-equivalent to the N=1
   (or Perl) result. Without it, the highest-risk interaction is unverified by
   automation. (C2)

3. **(Important) `.gz` N=1 ≡ N=4 sorted-equivalence test.** The plan says "add a
   `--gzip` N=1≡N=4 sorted-equivalence test if not present." It is **not**
   present — the only N-cross gzip test is the N=4-vs-legacy-plain one (which
   itself needs the MultiGzDecoder fix). Add an explicit `--gzip` N=1 vs N=4 test
   that decodes both with `MultiGzDecoder`, sorts, and compares. (I4)

4. **(Important) Header-once assertion.** Add an assertion that the decoded
   multi-member `.gz` contains the version header **exactly once at the front**
   (guards the C1 header-placement decision and the `bismark2bedGraph:182`
   `--remove_spaces` path). (I5)

5. **(Important) Empty-`.gz` sweep test.** Add/confirm a test that a key which
   receives zero calls under `--gzip --parallel N` is swept (deleted), and that
   a key receiving only the header (no call members) is also swept — matching the
   `records_written == 0` Perl behaviour. (I6)

6. **(Optional) Many-small-batches stress.** Force `BATCH_SIZE` small (or use an
   input >> BATCH_SIZE) so the `.gz` file has many members, then decode+sort.
   The 8199-record boundary test referenced in the plan should be extended to the
   `--gzip` path. (O1)

---

## 5. Alternatives worth noting

- **A1 — Keep compression in the collector but on a dedicated compressor thread
  pool (one thread per output key, ≤12).** Each key's writer is single-owner, so
  12 compressor threads could each own a `GzEncoder` and the collector just fans
  pre-formatted (uncompressed) bytes to them. This avoids multi-member output
  entirely (each file stays single-member → no test/header/downstream churn) and
  still parallelizes compression up to 12×. Trade-off: 12 is a hard parallelism
  cap (vs N workers), and it adds a second fan-out stage. Given the spike shows
  ~52 s dominated by compression and typical N≤8, a 12-way single-member design
  could match R2's win **with zero byte-contract change** (raw-`.gz` identity
  preserved, no MultiGzDecoder migration, no downstream risk). **Worth a
  paragraph of consideration before committing to multi-member** — it may be the
  lower-risk path to the same speedup. (Flag for user.)

- **A2 — Worker formats (uncompressed) per-key buffers; collector compresses.**
  Parallelizes only formatting, not compression — the spike says formatting/raw
  write is ~6 s and compression is ~52 s, so this recovers little. Reject
  (consistent with the plan).

- **A3 — `pigz`-style: single member, parallel DEFLATE blocks with a shared
  dictionary.** Highest fidelity (true single-member, best ratio) but far more
  complex; not justified. Reject.

---

## 6. Action items

### Critical
- **C1 — Specify version-header placement under multi-member gzip.** Define
  exactly how `SPLIT_FILE_HEADER` is emitted (e.g. collector writes a header-only
  gzip member 0 at open, or the first worker member carries it) and how the
  empty-sweep's `records_written`/"any-call-bytes" signal is maintained now that
  the collector no longer calls `write_call`. A raw-text header prefix in front
  of gzip members would produce an invalid `.gz`. (output.rs:126-128, :229,
  :283-342)
- **C2 — Add a downstream multi-member Phase G test.** Run `--gzip --parallel 4
  --bedGraph` (and `--cytosine_report` if cheap) end-to-end through the real Perl
  `bismark2bedGraph`; assert `.bedGraph.gz`/`.bismark.cov.gz` sorted-equivalent to
  N=1/Perl. The chain is `out of scope` for *implementation* but its *input bytes
  change*, so it is in scope for *validation*. (subprocess.rs:322-324; Perl
  bismark2bedGraph:168/202/434/446)
- **C3 — Enumerate and fix the single-member `GzDecoder` test sites.** The plan
  must call out `tests/parallel_phase_f.rs:346-351` (`decompress_gz`), and the
  `GzDecoder` uses at `tests/output_modes_phase_e.rs:345` and
  `tests/output_modes_phase_e_smoke.rs:183`, and prescribe migrating the
  pipeline-level decode (at minimum `decompress_gz`) to
  `flate2::read::MultiGzDecoder`. Without this, the existing gzip byte-identity
  test fails (truncated to member 0) the moment R2 lands.

### Important
- **I1 — Workers omit zero-length keys** from `per_key_bytes` (no empty gzip
  members); state this so the empty-sweep accounting stays correct.
- **I2 — State the in-flight memory bound** (≈ n_workers × channel-depth ×
  per-batch compressed bytes across ≤12 keys) and confirm it's acceptable.
- **I3 — Pin `Compression::default()`** in the worker to match today's
  `open_writer` so content/CPU match the spike.
- **I4 — Add explicit `--gzip` N=1 ≡ N=4 sorted-equivalence test** (decode both
  with `MultiGzDecoder`). The plan says "if not present" — it is not present.
- **I5 — Assert header appears exactly once** at the front of the decoded
  multi-member stream (guards C1 + the Perl `--remove_spaces` `:182` path).
- **I6 — Empty-`.gz` sweep test** under `--gzip --parallel N` (zero-call key and
  header-only key both deleted).

### Optional
- **O1 — Many-members stress test:** extend the 8199-record boundary test to
  `--gzip` so the `.gz` file has many members; decode+sort+compare.
- **O2 — Evaluate the 12-way single-member compressor-thread alternative (A1)**
  before committing to multi-member; it may deliver the same speedup with zero
  byte-contract change and no downstream/test churn. (Flag for user decision.)
- **O3 — One-line note** that `.gz` *file sizes* will differ from N=1 single-
  member (per-member overhead + dictionary resets) and that this is expected
  under the decoded-sorted-equivalence contract — so no one "fixes" it later.

---

## Summary

The R2 rescope to gzip-only is well-justified by the spike, and the multi-member
construction is fundamentally correct. The **highest-risk interaction (Phase G
downstream) is actually safe** because Perl `bismark2bedGraph` reads every `.gz`
infile via system `gunzip -c`, which is multi-member-correct — but the plan must
*test* this, not assume it. Two Critical gaps need closing before implementation:
(C1) the version-header placement / empty-sweep accounting under multi-member is
unspecified and could yield invalid `.gz` files, and (C3) three in-repo tests
decode with single-member `GzDecoder` (notably the real-pipeline
`parallel_gzip_n4_*` test) and will break/truncate under multi-member output —
the plan does not mention them. Recommend resolving C1-C3 and adding the
multi-member downstream + N=1≡N=4 gzip tests before the byte-identity rewrite.
