# SPEC — Rust port of `bismark` (the aligner / "big beast")

- **Status:** rev 1 — manual review PASSED (Felix approved 2026-05-31); all forks SETTLED (see §8). Next: phased EPIC via `epic-writer`.
- **Date:** 2026-05-31 (rev 0 drafted + approved + forks settled same day)
- **Branch / worktree:** `rust/aligner` @ `~/Github/Bismark-aligner` (off `origin/rust/iron-chancellor` @ `63d589c`).
- **Perl source of truth:** `./bismark`, v0.25.1, 10,027 lines, ~60 subroutines.
- **Part of:** the Bismark Rust rewrite (`rust/iron-chancellor`). This is the **largest** remaining port: ~74% of pipeline runtime, external-tool-dependent.

> **Reading guide.** This SPEC is deliberately scoped to the **five architectural forks** that must be settled *before* any planning. Each fork below has: (a) what the Perl code actually does, (b) the options, (c) a recommendation, (d) an **OPEN QUESTION** for Felix. §7 collects all open questions. Nothing here is a plan — the phased EPIC comes only after these forks are decided.

---

## 1. What `bismark` actually is (grounded in the source)

`bismark` is **not an aligner** — it is a **bisulfite-aware wrapper around an external general-purpose aligner** (Bowtie2 by default; HISAT2 / minimap2 optional). Its job, end to end:

1. **Read conversion** (`biTransformFastQFiles*`, lines 5489–6234). Writes temp FastQ files in which the reads are C→T transliterated (and, for non-directional, the G→A complement is produced too).
2. **Aligner invocation** (`*_align_fragments_to_bisulfite_genome_*`, 6253–7059). Opens **2 (directional-SE) or 4 (PE / non-directional / pbat)** subprocess pipes, each running one aligner instance against one converted index:
   ```perl
   open ($fh->{fh}, "$path_to_bowtie $bt2_options -x $fh->{bisulfiteIndex} -U $temp_dir$fh->{inputfile} |")
   ```
   Each instance gets a strand-restriction flag (`--norc` or `--nofw`) — see the table below. The aligner writes SAM to stdout; Bismark reads it line by line.
3. **Lockstep N-way merge + scoring** (`check_results_single_end` 2702, `check_results_paired_end` 3269 — the ~630-line core, `calc_mapq` 3923). Bismark holds all 2–4 SAM streams open simultaneously, advances them in read-ID lockstep, and selects the single best bisulfite alignment across instances by alignment score (`AS:i`), assigning strand origin (OT/OB/CTOT/CTOB).
4. **Genomic-seq extraction + methylation call** (`extract_corresponding_genomic_sequence_*` 4273/4471, `methylation_call` 4800). Pulls the matching genomic window from the in-memory genome and generates the per-base `XM` methylation-call string (+ `XR`/`XG` tags).
5. **SAM/BAM output** (`generate_SAM_header` 8452, `single_end_SAM_output` 8489, `paired_end_SAM_output` 8713). **Bismark fully regenerates every output record** — it does *not* pass Bowtie2's SAM through. The only Bowtie2-derived fields are POS / CIGAR / the chosen alignment; FLAG, MAPQ, tags, and chromosome-name de-conversion are all Bismark's.
6. **Orchestration / multicore / reports** (`multi_process_handling` 66, `process_command_line` 7247 — a 1,200-line option parser, `merge_individual_*` 960–1547, `print_final_analysis_report_*` 1964/2146).

### The strand-instance table (from `reset_counters_and_fhs`, 7124–7243)

| Instance name | Index basename | Orientation flag | Strand | SE-directional? |
|---|---|---|---|---|
| `CTreadCTgenome` | `BS_CT` | `--norc` | OT (con ori forward) | ✅ |
| `CTreadGAgenome` | `BS_GA` | `--nofw` | CTOB (con ori reverse) | ✅ |
| `GAreadCTgenome` | `BS_CT` | `--nofw` | CTOT (compl con forward) | — |
| `GAreadGAgenome` | `BS_GA` | `--norc` | OB (compl con reverse) | — |

- **Directional SE** → instances 1–2 only. **Directional PE / non-directional / pbat** → all 4 (directional *rejects* wrong-strand hits post-hoc, `alignments_rejected_count`). **pbat-SE** → instances 3–4 only.
- The `--norc`/`--nofw` restriction is mandatory: it is what stops every read multi-mapping against the bisulfite-converted index. (Same constraint flagged for the combined-index experiment.)

### Input contract (from `bismark_genome_preparation`, already ported)

The aligner consumes, per genome folder:
- `Bisulfite_Genome/CT_conversion/BS_CT.*` and `Bisulfite_Genome/GA_conversion/BS_GA.*` — the aligner index basenames (`$CT_index_basename` / `$GA_index_basename`).
- The **raw genome FASTA** — loaded into memory (`read_genome_into_memory`, 5022) for genomic-sequence extraction during the methylation call. So the aligner needs *both* the index *and* the unconverted reference.
- genome-prep also produces an opt-in **combined CT+GA index** (`--combined_genome`); its alignment-correctness validation was explicitly **deferred to this port** (see fork #4).

---

## 2. FORK #1 — Wrap the external aligner (subprocess) vs pure-Rust engine (rammap)

**What Perl does:** opens 2–4 OS pipes to an external aligner binary (`bowtie2` / `hisat2` / `minimap2`) and parses their SAM stdout. The alignment CPU is entirely the external tool's; Bismark's own cost is read conversion + SAM parsing + scoring + the fork-per-instance model.

**Options:**
- **(1A) Wrap Bowtie2 via subprocess**, byte-identical to Perl. Rust spawns `std::process::Command` per instance, reads piped stdout, parses SAM. The reference aligner stays external (pinned version).
- **(1B) Pure-Rust engine (rammap / minimap2 reimpl).** Replaces the aligner entirely. This changes *which* alignments are produced → cannot be byte-identical to today's Bowtie2-based Bismark, and rammap is a minimap2 reimpl, not a Bowtie2 reimpl, so it wouldn't match even the minimap2 path's output exactly.

**Recommendation: 1A for v1.** Byte-identity to Perl *requires* the same aligner choosing the same alignments. rammap / pure-Rust is a **v2+ experiment** (tracked separately, follow-up #918), explicitly out of the v1 byte-identity gate.

> **OPEN QUESTION 1:** Confirm v1 = subprocess-wrap Bowtie2; rammap/pure-Rust deferred to v2+. *(Recommended: yes.)*

**Sub-decision (1A-i): how to feed reads.** Perl writes a converted temp FastQ and passes `-U tempfile`. Rust could (a) match Perl exactly (write temp file, pass `-U`) or (b) stream converted reads to the child via stdin. **Recommend (a)** for byte-identity + determinism + parity with Perl's temp-file behaviour; revisit streaming as a v2 perf item.

---

## 3. FORK #2 — Acceptance gate: byte-identical BAM vs concordance *(THE key question)*

This is far harder here than for the post-alignment tools, because the output depends on the **exact external aligner version + invocation** in addition to Bismark's own logic.

**What's in our control vs not:**
- ✅ **In our control (fully Bismark-generated):** FLAG, MAPQ (`calc_mapq`), the `XM`/`XR`/`XG` tags, chromosome-name de-conversion (`s/_(CT|GA)_converted$//`), the best-alignment *selection* logic, SAM field formatting, the `@HD`/`@SQ` headers. The Rust port can match these byte-for-byte.
- ⚠️ **Aligner-dependent:** POS / CIGAR / `AS:i` / which alignment wins. Deterministic **only if** we pin the Bowtie2 version, run identical per-instance options (incl. `--norc`/`--nofw`), and run **single-threaded per instance** (or fixed seed + `--reorder`). Bowtie2's multimap tie-break is pseudo-random but seeded per read → reproducible with a pinned version.
- 🚩 **The `@PG` landmine (line 8480):** `@PG ID:Bismark VN:v0.25.1 CL:"bismark <command_line>"`. The `CL:` string is the verbatim invocation; the Rust binary is invoked differently, so it cannot match naturally.

**Gate options:**
- **(2A) Byte-identical BAM, header + records, against a pinned Bowtie2 version** — spoof `@PG VN:v0.25.1` and reconstruct the canonical `CL:"bismark …"` string (sibling ports successfully spoofed version strings). Strongest gate, matches the sibling-port standard. Risk: brittle to Bowtie2 version drift → mitigated by pinning the exact version that generated the Perl golden.
- **(2B) Byte-identical alignment *records*; header compared normalized** (exclude/normalize `@PG CL`). Sidesteps the command-line-spoofing debate; everything else identical.
- **(2C) Concordance gate** — mapping rate + per-read methylation-call agreement ≥ threshold. Weakest, hardest to trust; reserve for cases where byte-identity proves empirically impossible (uncontrollable aligner nondeterminism).

**Recommendation:** target **2A**, fall back to **2B** if the `@PG CL` spoofing is judged not worth it, and use **2C only** if a pre-implementation determinism spike (Phase 0) shows byte-identity is unreachable. **A Phase-0 spike must prove byte-identity is achievable on a tiny dataset before we commit the whole epic to a byte-identity gate.**

> **OPEN QUESTION 2a:** Which gate — 2A (full byte-identity, spoof `@PG`), 2B (records byte-identical, header normalized), or 2C (concordance)? *(Recommended: 2A, with a Phase-0 spike to de-risk.)*
>
> **OPEN QUESTION 2b:** Which **Bowtie2 version** do we pin as the byte-identity reference? (Needs to be the version used to generate the Perl golden on oxy. This pin becomes part of the gate and CI.)

---

## 4. FORK #3 — Scope of v1: aligner × library-type × SE/PE × input format

The Perl wrapper covers a 3×3×2×2 matrix. v1 cannot do all of it byte-identically at once.

| Axis | Values | v1 recommendation |
|---|---|---|
| **Aligner** | Bowtie2 (default), HISAT2, minimap2 | **Bowtie2 only**; HISAT2/minimap2 = later phases |
| **Library type** | directional (default), non-directional, pbat | **directional first**, then non-directional + pbat |
| **Reads** | single-end, paired-end | **SE first** (`check_results_single_end` ~450 LOC), then PE (`check_results_paired_end` ~630 LOC — the hardest single function) |
| **Input format** | FastQ (default), FastA; plain or gzipped | **FastQ first** (plain + gzip), FastA later |

**Recommended v1 spine:** `Bowtie2 + FastQ + directional + SE` → first byte-identity gate. Then expand one axis per phase.

> **OPEN QUESTION 3:** Confirm the v1 spine (Bowtie2 + FastQ + directional + SE) and the expansion order (PE → non-directional/pbat → FastA → HISAT2/minimap2). Any axis Felix wants pulled earlier/later?

---

## 5. FORK #4 — Perf/memory ideas that BREAK byte-identity (mark v2, keep out of the v1 gate)

These are real wins but must not contaminate the v1 byte-identity gate:

- **Combined CT+GA single-instance index** (one aligner run on the doubled reference with `--norc`/`--nofw`, instead of 2–4 instances). genome-prep already produces this index opt-in; **this port is where its alignment correctness gets validated.** It has *different ambiguity arithmetic* (each locus exists twice → inflated ambiguous/low-MAPQ fraction in C-poor regions) → **not byte-identical** → ship as an **alternative alignment mode** gated by *concordance*, never a silent replacement. **v2.**
- **Threading model.** The Perl `--multicore` model splits the input into chunks and forks the *whole* pipeline per chunk (each re-reading the genome and running its own 2–4 aligner instances) — the same fork+re-decompress waste the extractor's #884 fixed. **Key property: file-level chunking with single-threaded-per-instance aligners + an order-preserving merge can stay byte-identical** (per-read alignment is independent of other reads). So a Rust thread-pool over input chunks is **v1-compatible** *if* it preserves output order and per-read determinism — and must be proven worker-count-invariant (as the extractor was). In contrast, Bowtie2 `-p` multithreading *within* an instance reorders output → **v2** (needs `--reorder`, changes nothing for correctness but must be validated).
- **rammap / pure-Rust engine** — v2+ (fork #1).
- **mimalloc** global allocator (sibling ports use it) — output-neutral → fine in v1 as a perf-only change.

> **OPEN QUESTION 4:** Agree these are all v2/out-of-gate? Specifically: is **order-preserving file-level chunking** (Rust's natural multicore) in-scope for v1 with a worker-invariance gate, or do we ship single-threaded v1 first and add threading as its own phase? *(Recommended: ship a single-threaded byte-identical core first, then add order-preserving chunking as a late v1 phase with a worker-invariance gate — mirrors the extractor.)*

---

## 6. FORK #5 — Phasing (the heaviest of any port → EPIC)

Contingent on the answers above, the natural decomposition:

- **Phase 0 — Determinism spike.** Prove on a tiny dataset that a pinned Bowtie2 + replicated invocation yields byte-identical alignment records, and settle the `@PG` strategy. **Gates the entire byte-identity premise.** (`spike` skill.)
- **Phase A — CLI + option parsing + genome/index discovery + aligner detection** (`process_command_line` parity; no alignment yet).
- **Phase B — Read conversion** (FastQ SE directional C→T) → byte-identical converted temp files vs Perl.
- **Phase C — Single-instance align + SAM parse** (one Bowtie2 stream; store/advance lockstep primitive).
- **Phase D — Multi-instance lockstep merge + best-alignment scoring + strand assignment** (SE directional, 2 instances; `check_results_single_end` + `calc_mapq`).
- **Phase E — Genomic-seq extraction + XM/XR/XG call + MAPQ + SAM/BAM output** (SE directional) → **first byte-identity gate** (SE directional WGBS, local).
- **Phase F — Reports** (alignment report, splitting report, ambiguous/unmapped outputs, `--ambig_bam`).
- **Phase G — PE support** (`check_results_paired_end`, `paired_end_SAM_output`) → byte-identity gate (PE).
- **Phase H — Non-directional + pbat modes** (4-instance, wrong-strand rejection) → byte-identity gate.
- **Phase I — FastA input + order-preserving threading** (worker-invariance gate).
- **Phase J — HISAT2 + minimap2 aligners.**
- **Phase K — Real-data gate on oxy** (full WGBS SE + PE + mouse RRBS, byte-identical vs Perl v0.25.1 + pinned Bowtie2; `/var/tmp`, idle-gate, reusable `scripts/` harness).
- **v2 backlog:** combined-index alignment mode (concordance gate), Bowtie2 `-p`/`--reorder`, rammap engine, stdin-stream reads.

> **OPEN QUESTION 5:** Is this phase decomposition right, and is an **epic** (via `epic-writer`) the right container? Any phases to split/merge/reorder?

---

## 7. Conventions (match sibling ports)

- **New crate** `rust/bismark-aligner`; **binary `bismark_rs`** (parity with `bismark_methylation_extractor_rs`, `bismark_genome_preparation_rs`). *Confirm binary name.*
- **BAM/SAM I/O via `noodles`** (pure-Rust; no htslib, no samtools subprocess) — the standing BAM-I/O decision.
- Workspace member added to `rust/Cargo.toml`; `edition 2024`, `rust-version 1.89`, `GPL-3.0-only`.
- All plan/review docs live in **this worktree's** `plans/05312026_bismark-aligner/` so they ride the PRs.
- Workflow: this SPEC → manual review → (after approval) phased EPIC + dual plan-review → implement → dual code-review + plan-manager → real-data gate on oxy.
- **Public-artifact constraint:** do not name external *bisulfite* aligners in committed docs/code/issues. (Bowtie2/HISAT2/minimap2 are general aligners and Bismark's own declared dependencies — naming those is fine.) Present the combined-index approach as a Bismark-Rust design.

> **OPEN QUESTION 6:** Confirm crate `bismark-aligner` + binary `bismark_rs`.

---

## 8. Decisions — RESOLVED 2026-05-31 (Felix)

1. **Fork 1** ✅ v1 = **subprocess-wrap Bowtie2**; rammap / pure-Rust = v2+ (follow-up #918).
2. **Fork 2a** ✅ gate = **2A, full byte-identical BAM** — **operationally defined (Phase-0 spike, 2026-06-01) as byte-identical _decompressed_ SAM content** (`samtools view` records + `samtools view -H` header), **NOT** raw `.bam` bytes (the Rust port writes BAM via noodles, Perl via `samtools view -bSh -` → different BGZF encoders). Bismark `@PG` reconstructed from argv; the samtools-pipe `@PG` line gets a policy (refinement B — best-effort reproduce vs normalize-out, **pending Felix**). **Spike PASSED:** 8,402 records byte-identical run-to-run, Bowtie2 2.5.5 deterministic (C2), no reordering flags (C3). See [`phase0-determinism-spike/SPIKE_determinism.md`](./phase0-determinism-spike/SPIKE_determinism.md).
3. **Fork 2b** ✅ pin the Bowtie2 version **installed in oxy's `bismark-test` env** (the env that generates the Perl golden). **Detected 2026-05-31: Bowtie2 `2.5.5`** (`bowtie2-align-s version 2.5.5`, alongside samtools `1.23.1` + Bismark Perl `v0.25.1`). Bake **`bowtie2 2.5.5`** into the gate + CI; Phase 0 confirms the golden was generated with it.
4. **Fork 3** ✅ v1 spine = **Bowtie2 + FastQ + directional + SE** first; expansion order PE → non-directional/pbat → FastA → HISAT2/minimap2.
5. **Fork 4** ✅ combined-index alignment mode, Bowtie2 `-p` intra-instance threading, and rammap are all **v2** (out of the v1 byte-identity gate). **Order-preserving file-level chunking** ships as a **late-v1 phase** behind a worker-invariance gate; the byte-identical **single-threaded core comes first**.
6. **Fork 5** ✅ phasing = the Phase 0–K decomposition, with **Phase 0 = a determinism spike run first** (prove byte-identity reachable + settle `@PG` before committing the epic). Container = an **epic** via `epic-writer`.
7. **Conventions** ✅ crate `bismark-aligner`, binary `bismark_rs` (parity with sibling ports).

---

## 9. Out of scope for v1 (explicit)

- HISAT2 / minimap2 aligners (Phase J, but still v1-epic) and the combined-index alignment mode, Bowtie2 `-p` intra-instance threading, rammap engine, stdin-streamed reads (all **v2**).
- `--genomic_composition`-style niche options unless required for byte-identity (audit during Phase A).
- Any optimization that changes output bytes without an explicit alternative-mode flag + concordance gate.
