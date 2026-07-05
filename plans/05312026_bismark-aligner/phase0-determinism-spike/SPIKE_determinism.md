# SPIKE — Phase 0 determinism (Rust `bismark-aligner`)

- **Date:** 2026-06-01 · **Host:** oxy (`bismark-test` env) · **Verdict:** ✅ **byte-identity premise HOLDS** (with two gate-definition refinements).
- **Linked to:** [`../EPIC.md`](../EPIC.md) Phase 0 · [`../SPEC.md`](../SPEC.md) fork #2 (acceptance gate).

## 1. Question · success criteria · scope

**Question:** Is the Perl Bismark v0.25.1 → Bowtie2 2.5.5 pipeline deterministic enough that a faithful
Rust reimplementation can produce a byte-identical BAM (SE directional)?

**Success criteria:**
- **C1** — two independent Perl Bismark runs on the same ~10k-read SE input → byte-identical alignment
  **records** + identical header.
- **C2** — standalone Bowtie2 2.5.5, same invocation run twice → byte-identical records.
- **C3** — the `@PG` line is captured + reconstructable; the default invocation passes no output-reordering
  flags (`-p`/`--threads`/`--reorder`/`--non-deterministic`/`--seed`).

**Scope boundary (out):** no Rust code; SE directional only; not testing PE / non-directional / pbat /
scale / methylation-call correctness — only the determinism + version-pin + `@PG` premise. Throwaway.

## 2. Scripts · how to run

- `spike_determinism.sh` — iterations C1/C2/C3 (the core experiment).
- `spike_determinism_confirm.sh` — iteration #2 (raw-byte identity under identical invocation).

Run on oxy (detached, survives SSH drops):
```
dcli ssh oxy 'cat > /var/tmp/spike.sh' < spike_determinism.sh
dcli ssh oxy 'cd /var/tmp && nohup bash spike.sh > spike.out 2>&1 < /dev/null & echo $!'
# then poll /var/tmp/spike.out
```
Inputs: `~/bismark_benchmarks/10M_SE/directional_10M_R1_val_1.fq.gz` (subsampled to 10k reads),
`~/bismark_benchmarks/genome/` (human GRCh38 + prepared `Bisulfite_Genome`). Toolchain (verified):
Bismark **v0.25.1**, `bowtie2-align-s` **2.5.5**, samtools **1.23.1**.

## 3. Results

**Mapping sanity:** 10,000 reads → 84.0% mapping efficiency, 8,403 unique-best hits → the determinism
test is meaningful (reads genuinely align, not all-unmapped).

| Criterion | Result |
|---|---|
| **C1b — records identical run-to-run** | ✅ **8,402 records byte-identical** (`samtools view` diff empty) |
| **C1a — header identical run-to-run** | ⚠️ "differs" — but **only** the `@PG CL:` paths (`run1/tmp1` vs `run2/tmp2`); after stripping `@PG`, the 195 `@HD`/`@SQ` lines are **identical**, and after path-normalization the **full header is identical**. → a harness artifact (I gave the two runs different `-o`/`--temp_dir`), not nondeterminism. |
| **C2 — standalone Bowtie2 determinism** | ✅ **10,000 records identical** run-to-run |
| **C3 — invocation flags** | ✅ **no** `-p`/`--threads`/`--reorder`/`--non-deterministic`/`--seed`. Default aligner options = `-q --score-min L,0,-0.2 --ignore-quals`; per-instance `--norc` (CTreadCTgenome→`BS_CT`), `--nofw` (CTreadGAgenome→`BS_GA`). |

**Iteration #2 (raw-byte identity under identical invocation):** launched on oxy but the job vanished
with no output while a *sibling session's* `NOMe_filtering` job was running (load 1.8 → 5.3). Backed off
to avoid piling a second genome-loader on a shared box. **Inconclusive — deferred** (see §6/§7); not
load-bearing for the premise.

## 4. Findings summary

1. **Records are fully deterministic** — the hardest part (bisulfite best-alignment selection across the
   2 instances) reproduces byte-for-byte run-to-run. This is the core premise, and it holds.
2. **Bowtie2 2.5.5 is deterministic** for Bismark's invocation, single-threaded, default seed (no
   `--non-deterministic`). Output order = input order (no `-p`).
3. **Stored-BAM header `@PG` block is two lines** (verbatim from the spike):
   ```
   @PG  ID:Bismark    VN:v0.25.1  CL:"bismark --genome <g> <reads> -o <out> --temp_dir <tmp>"
   @PG  ID:samtools   PN:samtools  PP:Bismark  VN:1.23.1  CL:<abs-path>/samtools view -bSh -
   ```
   (A third `@PG ID:samtools.1 … CL:samtools view -H …` appears only when you *extract* the header with
   `samtools view -H`; it is **not** in the stored BAM.)
4. **🚩 Gate-definition refinement A — decompressed, not raw bytes.** The Rust port writes BAM via
   **noodles**; Perl Bismark writes it via **`samtools view -bSh -`**. Different deflate backends produce
   different BGZF *compressed bytes* from identical content, so **`cmp` on the `.bam` file is not a viable
   gate.** The gate must compare **decompressed** content: `samtools view` (records) + `samtools view -H`
   (header). (The run-to-run raw md5 already differed here — driven by the `@PG CL` path difference.)
5. **🚩 Gate-definition refinement B — `@PG` policy.** Byte-identical *header* requires reproducing both
   `@PG` lines, including the samtools line's absolute binary path (environment-specific) and the Bismark
   line's verbatim command line. Recommended policy: gate `@HD` + `@SQ` + the Bismark `@PG` (with `CL:`
   reconstructed from the Rust port's own argv) byte-for-byte, and treat the **samtools-pipe `@PG`** line
   as either (i) emitted best-effort with a configurable samtools version/path, or (ii) normalized out of
   the gate. Decision belongs to Felix (it slightly redefines "full byte-identical BAM").

## 5. Reference snippets (carry to implementation)

- **Per-instance Bowtie2 command (SE directional), as Bismark issues it:**
  ```
  bowtie2 -q --score-min L,0,-0.2 --ignore-quals --norc  -x <genome>/Bisulfite_Genome/CT_conversion/BS_CT  -U <tmp>/<reads>_C_to_T.fastq
  bowtie2 -q --score-min L,0,-0.2 --ignore-quals --nofw  -x <genome>/Bisulfite_Genome/GA_conversion/BS_GA  -U <tmp>/<reads>_C_to_T.fastq
  ```
  Both instances read the **same** C→T-converted temp FastQ (`<reads>_C_to_T.fastq`); the genome (not the
  read) differs, with `--norc`/`--nofw` restricting orientation. No `-p`, no `--reorder`, no `--seed`.
- **Default `aligner_options`** for Bowtie2 = `-q --score-min L,0,-0.2 --ignore-quals` (this is the string
  to reproduce; user `--score_min` etc. override it).
- **`@PG ID:Bismark` line** = `@PG\tID:Bismark\tVN:<version>\tCL:"bismark <command_line>"` — `CL:` is the
  verbatim argv joined by spaces, wrapped in literal double-quotes (Perl source line 8480).

## 6. Recommendation

**PROCEED** with the epic on the **2A full-byte-identity gate**, with the gate operationally defined as
**byte-identical decompressed SAM content** (`samtools view` + `samtools view -H`), the Bismark `@PG`
reconstructed from argv, and a Felix-blessed policy for the samtools-pipe `@PG` line (refinement B).
Phase 5's first gate is unblocked. Re-run iteration #2 opportunistically when oxy is idle to confirm
that, *under identical invocation*, the decompressed content (incl. both `@PG` lines) matches — but it is
not a blocker.

## 7. Limitations

- SE directional only; PE / non-directional / pbat determinism unverified (expected to hold — same
  mechanism — but re-confirm at each phase gate).
- 10k-read subset of one human WGBS sample; not full-scale (Phase 10 covers scale).
- Raw-BGZF/identical-invocation confirmation (iteration #2) deferred due to shared-box contention.
- Did **not** verify methylation-call (`XM`/`XR`/`XG`) correctness — out of scope; that is Phase 5's job.
- noodles-vs-samtools BGZF byte-equivalence was **not** tested and is **assumed infeasible** (refinement A);
  if raw-byte BAM identity is ever required, it needs its own spike on noodles' BGZF backend options.
