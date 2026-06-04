# Phase 10 — Full-scale real-data gate on oxy

> **Epic:** `05312026_bismark-aligner/EPIC.md`, Phase 10 — *Real-data gate on oxy* (the **LAST** phase of the faithful aligner port).

- **Created:** 2026-06-03 · **rev 1** (dual-plan-review findings folded in — see Revision History).
- **Branch / worktree:** `rust/aligner-v1` @ `~/Github/Bismark-aligner` (crate `rust/bismark-aligner`, binary `bismark_rs`). Push the Phase-10 PR to a **fresh** branch (force-push is deny-ruled).
- **Oracle / pins:** Perl Bismark **v0.25.1** · Bowtie 2 **2.5.5** · samtools **1.23.1** (oxy `~/micromamba/envs/bismark-test`).
- **Status:** Planning. **No gate has been run.** Execution is gated on Felix's explicit `implement` trigger.

---

## 1. Goal

Confirm, at **realistic full scale on real sequencing data**, that `bismark_rs` reproduces Perl Bismark v0.25.1 + Bowtie 2 2.5.5. The per-phase gates already proved byte-identity at 10k/1M on subsets; Phase 10's *new* information is **full chromosome diversity + a second genome build + real-data edge-case coverage + scale** (every GRCh38/GRCm39 contig incl. alt/`Un`/`random`/`M`, the `_(CT|GA)_converted` de-conversion on rare scaffolds, rare CIGARs, the genomic-seq-extraction-failure path, throughput/memory at 10⁸ reads). When this passes, the faithful aligner epic is **complete** and the row in `rust/README.md` flips to "10 of 10 ✅".

**Decisions resolved with Felix 2026-06-03 (the four critical kickoff questions):**

1. **Oracle / acceptance = HYBRID.** Strict byte-identity on a large feasible subset (10M) **plus** full-scale content-identity (sort/hash-normalized) vs the Perl `--parallel`-layout BAMs. Strict *ordering* stays proven by the 9b gate (N=1,000,003, coprime) + the 10M strict run.
2. **Cells = realistic only.** WGBS **SE-directional** + WGBS **PE-directional** (human `full_size`) + mouse **RRBS PE-directional**. Non-directional/pbat stay covered by their 1M per-phase + 9b gates (§9 O5 records the explicit risk-acceptance).
3. **RRBS input = align raw reads directly.** Feed raw `_R1/_R2.fastq.gz` to both Perl and Rust; alignment/byte-identity logic is identical to trimmed reads.
4. **Perf = yes, framed honestly.** Capture wall-clock + peak RSS for Rust `--parallel P` vs Perl `--multicore/--parallel P` at matched core count. Report the scaling win only; wall-clock is **Bowtie 2-dominated (~74%, unchanged by the port)** so the Rust win is wrapper + `--multicore` scaling, **Amdahl-bounded**; never a per-core Rust-vs-Perl figure (`feedback_extractor_parallel_cpu_messaging`).

---

## 2. Context

### Placement / dependencies
- **Depends on Phase 9b** (order-preserving `--multicore`/`--parallel`, merged as `22f3224`; flake-fix `15a34f1`). The aligner is feature-complete: SE+PE, FastQ+FastA, directional/non-directional/pbat, byte-identical **and** worker-count-invariant.
- This phase **writes no production Rust code.** It is a validation/gate phase: a re-base, an oxy build, two gate harness scripts, a run procedure, and a results doc (`GATE_OXY.md`). The only repo artifacts are under `plans/05312026_bismark-aligner/phase10-realdata-gate-oxy/` plus a `rust/README.md` status-row bump on the merge PR. (Kickoff prompt lives at the worktree root `PHASE10_KICKOFF_PROMPT.md`, not in this phase dir.)
- **Re-base prerequisite:** `rust/aligner-v1` has diverged twice (9b squash + flake-fix squash) → re-base onto `origin/rust/iron-chancellor` (`15a34f1`) **before** building on oxy, so the binary under test is exactly the merged code. `git reset --hard` AND force-push are **both deny-ruled** → use the 9b dance: `git stash` (if dirty) → `git rebase --onto origin/rust/iron-chancellor <old-aligner-v1-head>` → `git stash pop`; push the PR to a **fresh** branch. **Verify the result with a tree-diff, not `--version`** (§4 step 0, V1).

### The central tension (why the oracle is split)
The Rust port's `--parallel N` reproduces Perl **single-core** output (contiguous chunks + in-chunk-order merge — a *stronger* invariant than Perl's own fork+modulo `--multicore` layout). The pre-existing full BAMs on oxy were generated with Perl **`--parallel 4`** (confirmed via `@PG CL:`), whose fork+modulo striping **reorders** the records. Therefore:
- A **strict byte-identical** full-scale diff would require a fresh Perl **single-core** 84M run — single-threaded Bowtie 2 on 84M reads is plausibly **tens of hours**, high-risk on an ephemeral K8s pod. → done on the **10M subset** instead.
- At **full scale** we compare **content** (the multiset of record lines), which is order-independent and therefore matches Perl `--parallel P` directly.

**Why content-equality is *valid* (the load-bearing mechanism, A1).** Bowtie 2 seeds its multimap tie-break from the **read sequence + name, not the read's ordinal position or chunk assignment** (SPEC §3; 9b GATE_OXY proved per-read-content seeding). So each read produces the *same* alignment regardless of which chunk/stripe it lands in — which is exactly why 9b's contiguous-chunk model is byte-identical to single-core. The same argument applies to Perl's fork+modulo stripes: each child runs single-threaded Bowtie 2 on its stripe, and each read's alignment depends only on its content. Hence Perl `--multicore` emits the *same multiset* of record lines as Perl single-core, only reordered. **rev 1 no longer leaves this on faith: §4 step 4 measures it directly at 10M (Perl `--multicore P` vs Perl single-core) before the 84M gate relies on it.**

### oxy environment (verified 2026-06-03)
- `dcli ssh oxy` (`dockyard-oxy-0`), **128 cores**. Env `~/micromamba/envs/bismark-test` (Bismark v0.25.1 + Bowtie 2 2.5.5 + samtools 1.23.1). cargo 1.96 at `~/.cargo/bin`. All `dcli`/`gh`/`git` need `dangerouslyDisableSandbox: true`.
- **Disk:** `/var/tmp` (= the 1.0 T `overlay` root, **678 G free**) is node-local **ephemeral** (a pod recycle wipes it + kills detached jobs). `/home` = 99 G, **86 G free**, persists. → run + write big output to `/var/tmp`; keep durable artifacts (scripts, `GATE_OXY.md`, logs, the built binary) on `/home` and capture results off-box.
- **Datasets** `~/bismark_benchmarks/`:
  | dir | content | reads | genome |
  |---|---|---|---|
  | `genome/` | human GRCh38 + bisulfite index | — | GRCh38 |
  | `full_size/` SE | `..._SE_trimmed_full_size.fq.gz` (2.87 G) | **83,985,631** (per its `_SE_report.txt`; **re-measure at run time**) | GRCh38 |
  | `full_size/` PE | `..._R1_val_1.fq.gz` (2.28 G) + `..._R2_val_2.fq.gz` (2.85 G) | tens of M (measure) | GRCh38 |
  | `full_size/` Perl BAMs | SE `..._bismark_bt2.bam` (5.5 G) + PE `..._bismark_bt2_pe.bam` (11.3 G) | — | **made with Perl `--parallel 4`** |
  | `RRBS_PE/` | mouse, raw `_R1/_R2.fastq.gz` (S3 symlinks) | measure | **GRCm39** (own `genome/` + built index) |
  | `RRBS_PE/...bismark_bt2_pe.bam` | old reference, **May 2024** | — | **wrong provenance → regenerate, never reuse** |
  | `10M_SE`, `10M_PE` | the per-phase subset reads | 10,000,000 | GRCh38 |
- **Precedent harness:** `phase9b-threading/phase9b_worker_invariance_gate.sh` — runs the three-way comparison (Perl single-core vs Rust `--parallel 1` vs Rust `--parallel P`), with the `@PG`-block filter, wall-clock-filtered reports, decompressed-content (BAM + aux) comparison. **The 10M strict gate adapts this harness.** Note: 9b compares *all* BAMs (incl. `--ambig_bam`) via `samtools view` (harness lines 51/79) — the raw-`noodles_bam`-reader is a property of the **production merge code** (`merge_bams`), **not** the harness (rev-1 correction; see §3.3).

---

## 3. Behavior — the gate definition

### 3.1 Two complementary gates

**Gate A — subset strict byte-identity + direct assumption check (10M).** For each realistic cell, on the existing `10M_SE`/`10M_PE` datasets (and a 10M RRBS subset):
- **A-strict:** `bismark_rs --parallel 1` **==** Perl `bismark` **single-core** — strict byte-identical **decompressed** SAM content (`samtools view -h`, `@PG` block filtered) + alignment/splitting reports (wall-clock line filtered) + `--unmapped`/`--ambiguous`/`--ambig_bam` aux (decompressed). In-order → streaming `cmp` (§3.4).
- **A-worker (bonus, near-free):** `bismark_rs --parallel P` **==** `bismark_rs --parallel 1` — re-confirms worker-invariance at 10× the 9b scale. **Run at the same `P` used for Gate B** so the literal full-scale merge configuration is exercised at 10M.
- **A-assumption (bonus, the rev-1 addition — the highest-value check):** **Perl `--multicore P`** content **==** Perl **single-core** content (sort/hash-normalized, §3.4). This *directly measures* the load-bearing A1 assumption (Perl multicore is multiset-invariant) at 10M, converting it from an inference into a measured fact before the 84M content gate depends on it.

**Gate B — full-scale content-identity + perf (84M SE / full PE / full RRBS).** For each realistic cell:
- Run **Perl `--parallel P`** (timed) and **`bismark_rs --parallel P`** (timed) with **identical argv** (minus the orchestration flags that legitimately differ — §3.3) on the full dataset, writing to `/var/tmp`.
- **B0 — same-input guard:** assert both runs point at the **identical `--genome` path** (same `BS_CT`/`BS_GA` index + FASTA) — a divergence must be attributable to the aligner, not the index. (V0.)
- **B1 — report identity (cheap, runs first):** the alignment report is an order-independent count summary → Perl and Rust reports must be **byte-identical modulo the wall-clock line**. *B1 is necessary but NOT sufficient* (two different alignment outcomes can share identical counts) — B2 is the content authority. A single mismatched count = drift; fail fast before the expensive scan.
- **B1.5 — count reconciliation (hard gate, rev-1 addition):** `LC_ALL=C` `samtools view -c` on Perl == Rust == report-implied count, **AND** `wc -l` equality of the two `samtools view` (header-stripped) streams, **before** any hashing. Catches a drop/duplicate/truncation that a coincidentally-insensitive B1 could miss.
- **B2 — content identity (rigorous):** the **multiset of full record lines** must be equal. Per-`RNAME`-sharded `LC_ALL=C sort` → `md5sum` (§3.4) — sharding shrinks each sort, gives FAIL locality, and the per-chromosome md5 vector is the comparand. Header (`@HD`/`@SQ`, `@PG` filtered) compared separately and must be byte-identical.
- **B2.5 — distinct-`RNAME` set equality (rev-1 addition):** `cut -f3 | LC_ALL=C sort -u` on each side must match — *directly* gates Phase 10's headline chromosome/scaffold-diversity claim (incl. the `_(CT|GA)_converted` de-conversion on rare contigs), which B2 otherwise only implies.
- **B3 — aux identity:** `--unmapped`/`--ambiguous` are FastQ → **record-ize each 4-line record to one line (`paste - - - -`) BEFORE `LC_ALL=C sort`+`md5sum`** (sorting raw FastQ lines breaks record grouping — rev-1 critical fix); `--ambig_bam` compared via `samtools view` records, sort/hash-normalized (tagless raw records are valid SAM lines to samtools).
- **B4 — perf:** `/usr/bin/time -v` wall-clock + peak RSS for both Perl `--parallel P` and Rust `--parallel P` at matched `P`, reading **`/var/tmp`-staged inputs** (no S3-mount jitter). Report the scaling win per §1.4. A **recycle mid-perf-cell invalidates that cell's timing → re-run clean from scratch, not resumed** (idempotency protects correctness, not wall-clock).

**Oracle provenance for Gate B:** primary = a **freshly regenerated** Perl `--parallel P` run in the pinned env (this is the B4 perf run too — one run serves both perf and a same-env content oracle). The pre-existing `--parallel 4` BAMs serve as a **layout-invariance corroboration + provenance smoke** (V9): comparing **fresh-Perl-`--parallel P` vs old-Perl-`--parallel 4`** (Perl/Perl) directly observes two Perl worker counts producing the same multiset (corroborating A1) *and* retires the unknown-Bowtie 2-version of the old BAM. **V9 adds no independent-correctness signal** — both oracles are the same Perl version (faithful-port circularity is *by design*; the Perl run *is* the oracle). For **RRBS**, always regenerate (the May-2024 BAM has wrong provenance; never reuse).

### 3.2 Cells

| Cell | Dataset | Genome | Library | Gate A | Gate B |
|---|---|---|---|---|---|
| `se_dir` | `full_size` SE 84M (A: `10M_SE`) | GRCh38 | directional SE | ✅ strict + worker + assumption | ✅ content + perf |
| `pe_dir` | `full_size` PE (A: `10M_PE`) | GRCh38 | directional PE | ✅ strict + worker + assumption | ✅ content + perf |
| `rrbs_pe_dir` | `RRBS_PE` raw (A: subset or full) | GRCm39 | directional PE | ✅ (strict-full preferred if size allows) | ✅ content + perf (if hybrid) |

RRBS read count is unknown → **measure it first**. The Gate A harness is **parameterized on input-path + optional subset-N** so the same script serves a 10M head-subset *or* a full-size strict run. RRBS is the **designated strict-full candidate** (a strict byte-identical full run on a *second genome* is the strongest single new datapoint Phase 10 can produce); keep the threshold flexible and **confirm with Felix** rather than auto-defaulting to hybrid (§9 O1).

### 3.3 What legitimately differs (filter, don't fail on) — and what to assert
- **`@PG` block** — the Bismark `@PG CL:"bismark <argv>"` records the per-run argv (incl. `--parallel`, `-o`, `--temp_dir`); the samtools-pipe `@PG` line embeds an abs path. Filter the **whole `@PG` block** (9b lesson). *Consequence:* the old BAM's provenance `@PG` (v0.25.1 + samtools 1.23.1) is then asserted out-of-band (read once manually), not gated — V9 covers the rest.
- **Header completeness (rev-1):** after the `@PG` filter, **enumerate in `GATE_OXY.md` exactly which header lines remain** and assert byte-identity on them. Explicitly verify: (1) `@HD SO:`/`GO:` sort-order tags agree (Bismark output is unsorted); (2) `@SQ` order is **genome-derived (deterministic), not path/temp-derived** (the Phase-1 glob-order lesson — macOS/Linux flip-flop; adjudicated on Linux/oxy); (3) any `@CO` lines are not argv/path-bearing. A legitimately-varying header line must surface as a *finding*, not silently pass or fail.
- **Wall-clock line** in reports (`Bismark completed in …`). Filter it.
- **gz/BGZF framing** — Perl writes BAM via `samtools view -bSh -` and aux via external `gzip`; Rust via noodles/`GzEncoder`. The decompressed *content* is the invariant, never raw bytes. (Phase-0 + 9b lesson.)
- **`--ambig_bam` (rev-1 correction):** compared via **`samtools view`** (as the 9b harness actually does — lines 51/79), **not** a raw `noodles_bam` reader. The raw-noodles-reader caveat belongs to the *production merge path* (`merge_bams`, where `bismark_io::BamReader` rejects tagless XR/XG/XM-less records) — already shipped and proven; it is **not** a harness detail.
- **Record order at full scale** — Gate B is order-normalized by design (Perl `--parallel P` is reordered). Strict order is Gate A's job.

### 3.4 Comparison machinery at 10⁸ scale (must not buffer; must be canonical)
`diff <(samtools view …) <(…)` buffers both streams — at 84M records (~21 GB SAM text SE, ~2× PE) this exhausts memory. Refinements over the 9b harness:
- **Pin `LC_ALL=C`** on **every** `sort`, `cmp`, `md5sum`, `grep` in both harnesses. The "two independent sorts are equal iff multisets equal" guarantee requires an *identical, deterministic total order*; an unpinned UTF-8 locale risks a non-total/unstable collation and (if the locale ever differs between the two timed runs, e.g. across a recycle) a false FAIL. `LC_ALL=C` = byte-wise total order, fastest, reproducible.
- **Gate A (10M, in-order):** expected-identical streams → **streaming `cmp`** (O(1) memory) on the `@PG`-filtered `samtools view -h` output. On mismatch, **diagnosis recipe:** `cmp` reports the first differing byte offset → map to a line number → `sed -n 'START,ENDp'` a bounded window on **both decompressed, `@PG`-filtered** streams (never re-`diff` the full stream — that re-introduces the buffering hazard).
- **Gate B (84M, order-normalized):** per-`RNAME`-sharded `LC_ALL=C sort -S 25% --parallel=<N> -T /var/tmp` → `md5sum`, compared as a per-chromosome md5 vector (locality on FAIL) plus a combined md5. The B1.5 count + `wc -l` guard runs first. Header compared separately with `LC_ALL=C cmp`. **Run the two within-cell sorts (Perl side, Rust side) sequentially** (or `-S 25%`) so two `-S 50%` reservations can't OOM. **Disk headroom (PE worst case):** 2 fresh BAMs (~11 G ×2) + old cross-check BAM (~11 G) + sort temp (~1–1.5× of ~42 GB decompressed) ≈ ~130 G peak ≪ 678 G `/var/tmp`. ✔
  - *Optional fallback if `/var/tmp`/RAM is tight:* a **commutative order-independent hash** (Σ per-line 128-bit hash, paired with the B1.5 count assertion) proves the same multiset equality in O(1) memory with no sort — keep the sort as the on-FAIL diagnostic.
  - *On FAIL:* the per-`RNAME` md5 vector localizes the divergence to specific chromosome(s); then `comm -3 <(LC_ALL=C sort a) <(LC_ALL=C sort b) | head` on that shard shows the differing lines directly.

### 3.5 Robust execution on an ephemeral pod
- Run long alignments **detached**: `setsid nohup <cmd> < /dev/null > log 2>&1 &`; **poll frequently**; capture artifacts off-box (`dcli ssh oxy 'cat …' > local`) as each cell finishes — a recycle wipes `/var/tmp` and kills detached jobs (lost a Phase-4 matrix this way).
- Transfer each gate script to oxy as **its own step** (a backgrounded `cat>f && bash f &` reads `/dev/null` → 0-byte script — the documented gotcha).
- Keep scripts + `GATE_OXY.md` + logs + the built binary on `/home` (durable); run output on `/var/tmp` (capacity, ephemeral).
- **Stage S3-symlinked reads locally first** (`cp` the RRBS `_R1/_R2` + the timed-cell WGBS inputs off the S3 FUSE mount to `/var/tmp`) so a long alignment isn't bottlenecked/interrupted by the mount — **required for all timed B4 cells** (so mount jitter doesn't pollute wall-clock asymmetrically), not just nice-to-have.
- **A recycle mid-cell:** re-run that cell only (the harness is idempotent per cell). **For a perf cell, re-run clean** — a resumed/partial run invalidates the wall-clock/RSS number.

---

## 4. Implementation outline (the run procedure)

> All steps execute **on oxy** unless noted, via `dcli ssh oxy '…'` with `dangerouslyDisableSandbox: true`. **None of this runs until Felix's `implement` trigger.**

0. **Re-base (local, prerequisite) + verify by tree-diff.** In `~/Github/Bismark-aligner`: `git fetch origin`; `git stash` (if dirty) → `git rebase --onto origin/rust/iron-chancellor <old-aligner-v1-head>` → `git stash pop`. **Verify the crate is pure iron-chancellor: `git diff origin/rust/iron-chancellor -- rust/bismark-aligner` must be EMPTY** (a tree-hash compare — `--version` can't detect a stray replayed commit). Never `git checkout` in the shared `~/Github/Bismark`.
1. **Build on oxy.** `tar czf - --exclude=target . | dcli ssh oxy 'rm -rf /var/tmp/aligner_p10 && mkdir -p /var/tmp/aligner_p10 && tar xzf - -C /var/tmp/aligner_p10'`; then `cd /var/tmp/aligner_p10 && ~/.cargo/bin/cargo build --release -p bismark-aligner` → binary `/var/tmp/aligner_p10/target/release/bismark_rs`. **Copy the binary to `/home`** (recycle insurance). Capture `bismark_rs --version` + the iron-chancellor commit for the repro tuple.
2. **Measure inputs.** Read counts for full **SE, PE, and RRBS** (`zcat … | wc -l`/4 — don't trust the stored `_SE_report.txt` blindly); decide RRBS strict-full vs hybrid (§3.2, confirm with Felix). Stage RRBS + the timed WGBS inputs to `/var/tmp`. Record read counts/sizes (+ md5 if cheap) for the repro tuple.
3. **Write `phase10_subset_strict_gate.sh`** — adapt `phase9b_worker_invariance_gate.sh`: realistic cells only (`se_dir`, `pe_dir`, `rrbs_pe_dir`); **parameterized on input-path + optional subset-N** (serves 10M-subset *or* full-size RRBS); read the full 10M datasets directly; `LC_ALL=C` everywhere; swap the big-cell `diff` for streaming `cmp` + the §3.4 diagnosis recipe; add the **A-assumption leg** (Perl `--multicore P` vs Perl single-core, sort/hash-normalized); run **A-worker at the Gate-B `P`**; keep `--unmapped --ambiguous --ambig_bam`.
4. **Run Gate A (detached, polled).** Per cell: A-strict (Rust `--p1` == Perl single-core), A-worker (Rust `--pP` == `--p1`), A-assumption (Perl `--mP` == Perl single-core). All must PASS; capture exit codes + record counts + the assumption-check result to `GATE_OXY.md`. **A-assumption PASS is the gate that unlocks trusting Gate B.**
5. **Write `phase10_fullscale_content_gate.sh`** — NEW: per cell — B0 same-`--genome` guard → run Perl `--parallel P` (timed) + Rust `--parallel P` (timed) → B1 report-identity (`LC_ALL=C cmp` of wall-clock-filtered reports) → B1.5 count reconciliation (`view -c` ×2 + report-implied + `wc -l`) → B2 per-`RNAME`-sharded `LC_ALL=C sort`→`md5sum` (+ header `cmp`) → B2.5 distinct-`RNAME` set equality → B3 aux (`paste`-record-ized FastQ + `samtools view` ambig, sort/hash) → B4 perf (`/usr/bin/time -v`, `/var/tmp`-staged inputs). Also run the **Perl-fresh-`--parallel P` vs Perl-old-`--parallel 4`** cross-check (V9) for WGBS SE+PE.
6. **Run Gate B (detached, polled, off-box capture per cell).** SE first, then PE, then RRBS. Capture wall/RSS + per-`RNAME` md5 vector + report diffs + the repro tuple as each finishes. A recycled perf cell → re-run clean.
7. **Author `GATE_OXY.md`** — per-cell results (Gate A: strict PASS + counts + assumption-check; Gate B: report-identical Y/N, count-reconciled Y/N, content per-`RNAME` md5 match Y/N, RNAME-set match Y/N, aux match Y/N, header-lines-enumerated + identical, Rust vs Perl wall + RSS at matched P). Record the **full reproduction tuple** (binary version + build commit, Bowtie 2/samtools/Bismark `--version` verbatim, dataset read-counts/sizes/md5s, each run's exact argv). State the honest Bowtie 2-dominated/Amdahl perf framing. Mark the epic gate ✅/❌.
8. **On PASS:** bump `rust/README.md` aligner row → "Phase 10 of 10 ✅" + a dated Milestones line (`project_rust_status_journal`); update `EPIC.md` Phase 10 status + `PROGRESS.md`. Commit to a **fresh** branch, open the PR. **Squash-merge + push only on Felix's explicit ask.**
9. **On any FAIL:** stop, save the per-`RNAME`-localized diff window + logs off-box, do **not** auto-fix — report the gap and wait for instructions (`~/.claude/CLAUDE.md`).

---

## 5. Efficiency

- **Don't buffer 21–42 GB SAM streams.** Gate A uses streaming `cmp`; Gate B uses per-`RNAME`-sharded `LC_ALL=C sort -S 25% -T /var/tmp` then `md5sum`, sorts **sequential within a cell** (no concurrent `-S 50%` OOM). B1 report-identity + B1.5 count guard are the O(report)/O(1) fast gates that fail before any full scan.
- **Reuse the existing `--parallel 4` BAMs** as the V9 layout-invariance/provenance cross-check (no extra full Perl run there); regenerate only the one timed Perl `--parallel P` run (doubles as perf + fresh oracle) and RRBS.
- **Parallelism budget:** 128 cores. Both Perl `--multicore P` and Rust `--parallel P` spawn the **identical 2P-instance topology** (directional: P chunks × 2 single-threaded Bowtie 2 instances) → pick `P` so `2P ≲ 128` (e.g. P=16 → 32 threads). State the symmetry so the perf number is unimpeachable. Run cells **sequentially**.
- **Optional O(1)-memory path:** a commutative multiset hash replaces the sort entirely (§3.4) if `/var/tmp`/RAM is ever tight — same guarantee given the B1.5 count assertion.
- **`--ambig_bam` raw records** have no XR/XG/XM, but the *harness* reads written `.bam` via `samtools view` (no tag validation), so this is a non-issue for comparison; the raw-`noodles_bam` reader is a production-merge property only.

## 6. Integration

- **Reads:** the genome-prep `BS_CT`/`BS_GA` indexes + raw FASTA (GRCh38 + GRCm39 — **both sides MUST consume the identical `--genome` path**, V0), the `full_size`/`10M_*`/`RRBS_PE` FastQ, and the pre-existing Perl `--parallel 4` BAMs (V9 cross-check).
- **Writes:** only `/var/tmp` run output (ephemeral) + `plans/.../phase10-realdata-gate-oxy/{phase10_subset_strict_gate.sh, phase10_fullscale_content_gate.sh, GATE_OXY.md, logs}` + the `rust/README.md` row bump (durable, ride the PR).
- **Downstream:** the validated full-scale Bismark BAM is the input contract for the already-ported `bismark-extractor`/`bismark-dedup`/etc. — confirming it byte/content-matches Perl at full scale closes the loop on the whole post-alignment chain.
- **Order relative to other work:** final phase of the faithful epic; unblocks the v2 alternative-models epic (the byte-identical baseline becomes the concordance oracle).

## 7. Assumptions

**From the epic (shared, apply here):**
- Oracle = Perl Bismark **v0.25.1**; Bowtie 2 pinned **2.5.5**; samtools **1.23.1** (oxy `bismark-test` env). Part of the gate.
- BAM/SAM I/O via **noodles**; output is fully Bismark-generated (only POS/CIGAR/which-alignment-wins is Bowtie 2-derived).
- Gate is **byte-identical decompressed SAM content**, not raw `.bam` bytes (noodles' BGZF ≠ samtools').
- Byte-identity is **adjudicated on Linux (oxy)**, never macOS dev (the `@SQ` glob-order lesson).
- Determinism: single Bowtie 2 thread per instance → per-read alignment independent → order preserved & worker-count-invariant (proven 9b).
- Public-artifact constraint: don't name external *bisulfite* aligners in committed docs/code (Bowtie 2 is fine — a declared dependency).

**Phase-10-specific:**
- **A1 (load-bearing):** Perl `--multicore/--parallel P` produces the same multiset of record lines as single-core, only reordered. *Mechanism:* Bowtie 2 seeds tie-breaks from read content, not file/chunk position (§2). **rev 1 measures this directly at 10M (Gate A A-assumption) before the 84M gate relies on it**; further corroborated by B1 report identity, the 9b in-order proof, and V9 (Perl `--parallel 4` vs fresh `--parallel P`).
- **A2:** the pre-existing `full_size` Perl BAMs are v0.25.1 + samtools 1.23.1 (`@PG` confirms) + Bowtie 2 2.5.5 (version not in `@PG`; **retired by the V9 Perl-fresh-vs-old content match** — if a known-2.5.5 fresh Perl run equals the old BAM, the old BAM used a byte-compatible aligner).
- RRBS aligned from **raw** reads on both sides — alignment logic identical; untrimmed reads push *more* reads into the unmapped/clipped/genomic-seq-failure paths, which **increases** coverage of exactly the edge paths the gate targets (a positive, not a risk).
- `full_size` SE = 83,985,631 reads (re-measured at run time); full PE + RRBS counts measured at run time.
- Both Perl and Rust consume the **identical `--genome` index** per cell (V0) — a divergence is the aligner's, not the index's.
- `/var/tmp` survives a single gate run (no recycle mid-run) — flagged, not guaranteed; mitigated by detach + per-cell off-box capture + idempotent re-run (correctness) + clean re-run for perf cells.

## 8. Validation (the gate IS the validation; key failure points)

| # | What to verify | How | Expected |
|---|---|---|---|
| V0 | Both sides use the same index | assert identical `--genome` path per cell (RRBS index present) | identical inputs |
| V1 | Re-based binary == merged code | **`git diff origin/rust/iron-chancellor -- rust/bismark-aligner` empty** (tree-diff, not `--version`) | empty diff |
| V2 | Gate A A-strict, SE-dir 10M | 9b harness, `LC_ALL=C` streaming `cmp` of `@PG`-filtered SAM | Rust `--p1` == Perl single-core, byte-identical; report + aux identical |
| V3 | Gate A A-strict, PE-dir 10M | as V2, incl. `--ambig_bam` via `samtools view` | byte-identical |
| V4 | Gate A A-worker 10M (at Gate-B `P`) | Rust `--pP` vs `--p1` | identical (10× the 9b bar, full-scale `P`) |
| **V5** | **Gate A A-assumption 10M (rev-1)** | **Perl `--multicore P` vs Perl single-core, sort/hash** | **identical multiset — directly validates A1** |
| V6 | Gate B B1 report identity, all cells | `LC_ALL=C cmp` wall-clock-filtered reports (fresh Perl `--pP`) | byte-identical (necessary, not sufficient) |
| V7 | Gate B B1.5 count reconciliation | `view -c` Perl==Rust==report-implied + `wc -l` equality | all equal, pre-hash |
| V8 | Gate B B2 content identity, SE 84M | per-`RNAME` `LC_ALL=C sort`→`md5sum`, header `cmp` | md5 vector match; header identical (`@PG` filtered) |
| V9 | Gate B B2 content identity, PE full | as V8 | md5 vector match |
| V10 | Gate B B2.5 distinct-`RNAME` set | `cut -f3 \| LC_ALL=C sort -u` both sides | set equal (gates scaffold diversity incl. GRCm39 alt/Un/random/M) |
| V11 | Gate B B3 aux identity | FastQ **record-ized** (`paste - - - -`) then sort/md5; ambig via `samtools view` | content match |
| V12 | Header completeness | enumerate surviving lines; `@HD SO:`/`GO:`, `@SQ` order genome-derived, `@CO` | identical; any varying line = finding |
| V13 | Perl layout-invariance + provenance (was V9) | fresh Perl `--pP` vs old Perl `--p4`, sort/hash | match — corroborates A1, retires A2 (not an independent-correctness signal) |
| V14 | Content identity, RRBS (mouse) | as V8 (or strict-full if size allows) | match — GRCm39 + sparse CCGG |
| V15 | Perf, framed honestly | `/usr/bin/time -v` wall + RSS, matched P, `/var/tmp` inputs | recorded; Bowtie 2-dominated/Amdahl caveat stated; no per-core Rust-vs-Perl claim |

*Genomic-seq-extraction failures (SE oracle shows 36): the **count** is covered by B1; the **same-reads-discarded** property is covered by B2's main-BAM multiset (discarded reads are absent on both sides) + V7's count guard. No separate independent probe needed — it is a B1-derived consistency check.*

## 9. Questions or ambiguities

**Resolved (Felix, 2026-06-03):** oracle/acceptance = hybrid; cells = realistic only; RRBS = raw reads; perf = yes/honest. (§1.)

**Open (revisit if it bites):**
- **O1 (Open — wants a Felix nudge):** RRBS strict-full vs hybrid. *Preferred:* strict-full if RRBS's measured size makes a single-core Perl run feasible (a strict byte-identical full run on GRCm39 is the strongest single new datapoint). Keep the threshold flexible; **confirm at measure time** rather than auto-defaulting to hybrid at 20M.
- **O2 (Open):** worker count `P` for Gate B / perf. *Assumption:* P=16 (2P=32 ≤ 128) for clean comparable timing; both sides same `P`; Gate A A-worker runs at this same `P`.
- **O3 (Open):** recycle handling — correctness cells re-run idempotently; **perf cells re-run clean** (timing not resumable).
- **O4 (Open):** S3→`/var/tmp` staging is a **requirement** for timed B4 cells (resolved from rev-0's "assumption: yes").
- **O5 (PARTIALLY CLOSED 2026-06-04, Felix-directed):** a **`pbat_pe` cell is now gated at full scale** via Felix's R1↔R2-swap trick — feeding R2 as `-1` and R1 as `-2` with `--pbat` makes the directional PE data align as genuine pbat (CTOT/CTOB), so the 4-instance pbat path is exercised at 10⁸ scale (Gate A 10M strict+worker+assumption + Gate B full content+perf, same machinery as the other cells; no pre-existing pbat oracle → V13 skipped for this cell). **Non-directional** remains *not* gated at full scale (the concatenation-construction option was deferred) — accepted residual: it traverses the same genome/scaffolds as the directional + pbat full-scale cells (no new scaffold coverage forgone), and 4-instance non-dir worker-invariance + byte-identity were proven at coprime 1M (9b) + 10k (Phase 8); the only residual is a non-dir-only *scale-dependent* bug, low-risk (per-read logic is scale-free).
- **O6 (Open — named residual):** the **one ordering risk Gate B cannot see** — a *reordering-only* bug (no record loss) that triggers only at the full-scale chunk size (~5.25M reads/chunk at P=16/84M), larger than any chunk ordering-tested at 10M/1M. Mitigated by running Gate A A-worker at the Gate-B `P`; named here rather than implying full ordering coverage. (A *record-loss* capacity bug at huge chunk would still be caught by B1.5+B2.)

## 10. Self-Review

- **Efficiency:** caught the 9b harness's `diff`-buffering hazard at 84M → streaming `cmp` (Gate A), per-`RNAME`-sharded `LC_ALL=C sort`→`md5sum` with sequential within-cell sorts (Gate B); front-loaded B1 report-identity + B1.5 count guard so drift fails before any 21 GB scan; added a commutative-hash O(1)-memory fallback. Disk-headroom arithmetic shown (~130 G ≪ 678 G).
- **Logic:** the order-vs-content split is internally consistent — strict *ordering* is algorithmic, proven at 1M (9b) + re-proven at 10M (Gate A) at the full-scale `P`; full scale tests *content* + edge cases (order-independent). The load-bearing A1 assumption is now *measured directly* at 10M (V5) instead of only inferred, and its mechanism (Bowtie 2 per-read-content seeding) is spelled out. V9→V13 reframed as layout-invariance/provenance corroboration (honestly *not* an independent-correctness signal).
- **Edge cases:** FastQ aux record-ization (the rev-1 critical harness fix — line-sorting FastQ would mask a divergence); `--ambig_bam` tagless records via `samtools view` (misattribution corrected); distinct-`RNAME` set equality + `@SQ` byte-identity as the *direct* scaffold-diversity gates (incl. GRCm39 alt/Un/random/M + de-conversion); count reconciliation + `wc -l` to block silent drop/dup; `LC_ALL=C` for deterministic total order; genomic-seq-extraction failures (count via B1, identity via B2); ephemeral-pod recycle (detach + per-cell capture + idempotent re-run; clean re-run for perf); same-index guard (V0); tree-diff binary verification (V1).
- **Integration:** validated BAM is the input contract for downstream Rust tools; no production code changes → zero regression surface beyond the gate.
- **Remaining risks (named, not hidden):** (a) a strict full-scale byte-identity run is deliberately avoided (single-core Perl 84M ≈ tens of hours; separate run if ever wanted) — RRBS strict-full (O1) partially covers it on a second genome; (b) the reordering-only-at-huge-chunk ordering residual (O6); (c) non-dir/pbat untested at full scale (O5, accepted); (d) faithful-port circularity is by design — V13 corroborates layout-invariance/provenance, not independent correctness.

---

## Revision History
- **rev 2 (2026-06-04, during execution):** (a) **`pbat_pe` full-scale cell added** (Felix-directed, partially closes O5) via the R1↔R2-swap trick. (b) **Resource envelope corrected:** oxy advertises the node's 128c/991G but Felix's cgroup is **32c/256G** → gates run at **P=8** (16 Bowtie 2 processes, headroom) not P=16, and Gate B's content sort uses an **absolute `-S 16G`** (a `%` would size against the 991G node RAM and risk an OOM-kill on the 40G PE sort). Correctness is worker-count-invariant, so P does not affect the byte/content verdict. (c) Gate A (already PASSED at P=16 before the correction) stands — its result is P-invariant; only its perf row is at P=16/10M.
- **rev 1 (2026-06-03):** Dual plan-review (A+B) findings folded in. **Critical:** `LC_ALL=C` everywhere (§3.4); three-way count reconciliation + `wc -l` before hashing (B1.5, V7); FastQ-aux record-ization via `paste - - - -` before sort (B3, V11 — a real latent harness bug); `--ambig_bam` misattribution corrected to `samtools view` (§3.3); tree-diff binary verification replacing `--version` (§4 step 0, V1). **Important:** direct Perl-`--multicore`-vs-single-core multiset check at 10M (Gate A A-assumption, V5 — converts the load-bearing A1 from inference to measured fact); header-line enumeration + `@SQ`/`@HD SO:`/`@CO` checks (V12); distinct-`RNAME` set equality (V10); V0 same-`--genome` guard; reframed V9→V13 as layout-invariance/provenance corroboration; run Gate A A-worker at the Gate-B `P`; named the O6 ordering residual; recycle invalidates perf timing (clean re-run); sequential within-cell sorts + symmetric 2P topology + disk-headroom arithmetic; S3→`/var/tmp` staging promoted to a requirement; SE read-count re-measured at run time; Gate A harness parameterized on input-path + subset-N (serves strict-full RRBS). **Optional folded in:** per-`RNAME` md5 sharding + `comm -3` for FAIL locality; commutative-hash O(1) fallback; reproducibility tuple in `GATE_OXY.md`; explicit non-dir/pbat full-scale risk-acceptance (O5); Bowtie 2-dominated/Amdahl perf caveat.
- **rev 0 (2026-06-03):** Initial plan. Four critical kickoff questions resolved with Felix up front. Oxy datasets + Perl-`--parallel-4`-BAM provenance verified by inspection.
- **rev 1 (2026-06-03):** Dual plan-review (A+B) findings folded in. **Critical:** `LC_ALL=C` everywhere (§3.4); three-way count reconciliation + `wc -l` before hashing (B1.5, V7); FastQ-aux record-ization via `paste - - - -` before sort (B3, V11 — a real latent harness bug); `--ambig_bam` misattribution corrected to `samtools view` (§3.3); tree-diff binary verification replacing `--version` (§4 step 0, V1). **Important:** direct Perl-`--multicore`-vs-single-core multiset check at 10M (Gate A A-assumption, V5 — converts the load-bearing A1 from inference to measured fact); header-line enumeration + `@SQ`/`@HD SO:`/`@CO` checks (V12); distinct-`RNAME` set equality (V10); V0 same-`--genome` guard; reframed V9→V13 as layout-invariance/provenance corroboration; run Gate A A-worker at the Gate-B `P`; named the O6 ordering residual; recycle invalidates perf timing (clean re-run); sequential within-cell sorts + symmetric 2P topology + disk-headroom arithmetic; S3→`/var/tmp` staging promoted to a requirement; SE read-count re-measured at run time; Gate A harness parameterized on input-path + subset-N (serves strict-full RRBS). **Optional folded in:** per-`RNAME` md5 sharding + `comm -3` for FAIL locality; commutative-hash O(1) fallback; reproducibility tuple in `GATE_OXY.md`; explicit non-dir/pbat full-scale risk-acceptance (O5); Bowtie 2-dominated/Amdahl perf caveat.
