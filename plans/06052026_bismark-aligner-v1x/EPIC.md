# EPIC — `bismark` aligner v1.x: HISAT2 + minimap2 wrappers

- **Created:** 2026-06-05
- **Branch / worktree:** `rust/aligner-v1x` @ `~/Github/Bismark-aligner` (off `origin/rust/iron-chancellor` @ `fc38191`).
- **SPEC:** [`SPEC.md`](./SPEC.md) (rev 0, approved 2026-06-05 — forks resolved: HISAT2 first, spike-first byte-identity; OQ1–5 leans accepted).
- **Crate / binary:** the existing `rust/bismark-aligner` (`bismark_rs`) — **additive**, no new crate.
- **Oracle / pins:** Perl Bismark **v0.25.1** · **HISAT2 2.2.2** · **minimap2 2.31-r1302** · samtools 1.23.1 (oxy `bismark-test` env).

---

## 1. Goal

Add **HISAT2** and **minimap2** as alternative alignment backends to the Rust `bismark` aligner, each **byte-identical to Perl Bismark v0.25.1 driving the same pinned aligner**. Reuses the faithful Bowtie2 port's merge/scoring/XM/SAM-output/report core; the new work is the per-aligner **wrapper** + (for minimap2) a **merge-path adaptation** for its no-strand-restriction both-strand alignment. Byte-identity per aligner is **gated by a determinism spike first** (concordance fallback only if a spike shows non-determinism).

## 2. Scope

**IN (v1.x):**
- **HISAT2** backend (pinned 2.2.2): wrapper + byte-identity gate.
- **minimap2** backend (pinned 2.31-r1302): wrapper + merge adaptation + byte-identity gate (contingent on the Phase-3 spike).
- Reads: SE + PE, FastQ (+ FastA where the per-phase plan confirms parity). **Directional first** (OQ2); non-directional/pbat added per the faithful-port precedent once the spikes show no surprises.
- Generalize `aligner.rs` to a multi-backend abstraction; per-aligner option assembly; wire the already-parsed `--hisat2`/`--minimap2`/`--path_to_*`/splice/preset CLI surface.

**OUT:**
- New crate (additive to `bismark-aligner`). genome-prep changes (all 3 bisulfite indexes already exist on oxy).
- The v2 alternative-engines epic (rammap / combined-index, concordance-gated) — separate (`project_aligner_v2_alternative_models`).
- HISAT2/minimap2 in the perl-oracle CI gate (OQ5 lean: oxy-only; CI pin a follow-up).

## 3. Phase breakdown (execution order + dependencies)

Spike-first per aligner. 🎯 marks a byte-identity gate.

- **Phase 1 — HISAT2 determinism spike** (oxy). Run Perl `bismark --hisat2` + HISAT2 2.2.2 twice on a small real subset → alignment records byte-identical run-to-run? Settle reorder/option flags + capture the exact HISAT2 `aligner_options` Bismark assembles. Gates the HISAT2 byte-identity premise. (`spike` skill.)
- **Phase 2a — HISAT2 wrapper core (detection + options + discovery + naming + SE gate).** Generalize `aligner.rs` → multi-backend (pin 2.2.2); per-aligner option assembly = **Bowtie2 base + `--no-softclip --omit-sec-seq` appended LAST** (after the PE tail + `--quiet`, Perl 8314), **`--dovetail` suppressed for HISAT2** (Perl gates it `if($bowtie2)`), + splice-flag handling/fail-loud; `.ht2` discovery as an **8-suffix arity** change (`{1..8}.ht2`, no `rev.*`); naming token threaded through lib.rs **+ `parallel.rs`** (or `--multicore`+`--hisat2` fail-loud-rejected) + `ReportHeader` aligner field/report wording. 🎯 **SE** byte-identity gate (directional → non-dir/pbat SE) at 10k+1M on oxy. **Bowtie2 byte-frozen** (regression-guard, append-to-finished-string keeps it structural). Depends on #1. *(Dual review of the combined Phase 2 surface: `phase2a-hisat2-core/PLAN_REVIEW_{A,B}.md`.)*
- **Phase 2b — HISAT2 paired-end + remaining gate.** The **PE read-1 `ZS` asymmetry fix** — read-1 second-best parsed **XS-only** for HISAT2 PE (Perl 3372-3382 has *no* `ZS` branch → `second_best_1` always undef → backfilled to `AS`; the Rust uniform parser over-captures `ZS` → wrong MAPQ → non-identical BAM). The dual review's load-bearing find; needs a post-parse `r1.second_best=None` mask + a dedicated PE-HISAT2 multi-mapper unit test. 🎯 **PE + non-dir/pbat + FastA** byte-identity gate at 10k+1M on oxy. Depends on #2a.
- **Phase 3 — minimap2 determinism + selection spike** (oxy). The highest-risk phase (OQ3): is minimap2 2.31 deterministic run-to-run, and what does its **both-strand (no `--norc`/`--nofw`)** alignment do to the unique-vs-ambiguous / best-alignment arithmetic vs the Bowtie2/HISAT2 model? Characterizes the merge adaptation + decides byte-identity-vs-concordance reachability before committing Phase 4. (`spike` skill.)
- **Phase 4 — minimap2 wrapper + merge adaptation + byte-identity gate.** Positional `.mmi` invocation; `-ax sr` option assembly; `/1 /2` retention in convert/ID; the no-strand-restriction merge/selection path; naming + report wording. 🎯 byte-identity gate at 10k + 1M on oxy. Bowtie2 + HISAT2 byte-frozen.
- **Phase 5 — combined v1.x full-scale real-data gate + PR.** HISAT2 + minimap2 full-scale on oxy (human WGBS SE+PE; RRBS/mouse optional, OQ4), `rust/README.md` journal + status-row bump, fresh-branch PR → iron-chancellor (squash-merge on explicit ask).

## 4. Sub-plan table

| # | Phase | Plan file | Depends on |
|---|-------|-----------|------------|
| 1 | HISAT2 determinism spike ✅ **premise HOLDS** | `phase1-hisat2-determinism-spike/spikes/SPIKE_hisat2_determinism.md` | — |
| 2a | HISAT2 core (detect+options+discovery+naming+SE gate) 🎯 | `phase2a-hisat2-core/PLAN.md` | #1 |
| 2b | HISAT2 PE (read-1 ZS fix) + PE/non-dir/pbat/FastA gate 🎯 | _(to be written)_ | #2a |
| 3 | minimap2 determinism + selection spike 🎯-premise | `phase3-minimap2-determinism-selection-spike/SPIKE_*.md` | #2b |
| 4 | minimap2 wrapper + merge adaptation + gate 🎯 | _(to be written)_ | #3 |
| 5 | Combined full-scale gate + PR 🎯 | _(to be written)_ | #2b, #4 |

Sub-plans are written separately via `plan-writer` (spikes via the `spike` skill). When a plan is written, update its row from `_(to be written)_` to the actual filename.

## 5. Shared assumptions (apply across all phases)

- **Oracle = Perl Bismark v0.25.1**; **HISAT2 2.2.2**, **minimap2 2.31-r1302** pinned (oxy `bismark-test` env). samtools 1.23.1.
- All bisulfite indexes (`.bt2`/`.ht2`/`.mmi`) **already exist** on oxy `~/bismark_benchmarks/genome/Bisulfite_Genome/` — no genome-prep prerequisite for the gate.
- **Bowtie2 paths stay byte-frozen** — every HISAT2/minimap2 phase regression-guards against the existing Bowtie2 gate.
- BAM/SAM I/O via `noodles`; output fully Bismark-generated; gate on **decompressed** SAM content, `@PG` policy as the Bowtie2 port (Bismark line from argv, samtools line normalised); adjudicated on Linux/oxy.
- Byte-identity is the target; **a determinism spike gates it per aligner** — concordance fallback is a documented, Felix-decision-only path.
- HISAT2/minimap2 are general aligners + Bismark's declared dependencies → naming them in committed artifacts is fine.
- Per-phase workflow: plan-writer → manual review → dual plan-review → (trigger) `/code-implementation` → dual `/code-reviewer` + `/plan-manager` → oxy gate. Re-base `rust/aligner-v1x` before each PR (force-push denied → fresh branches); squash-merge on explicit ask.

## 6. Integration points

- **Generalization seam:** `aligner.rs` (Bowtie2-only → multi-backend), `options.rs` (per-aligner option assembly), `cli.rs` (the deferred `--hisat2`/`--minimap2` surface), `discovery.rs` (`.ht2`/`.mmi` index extensions). The merge/scoring/XM/output/report core is reused; minimap2 additionally adapts the merge/selection path.
- **Upstream:** consumes genome-prep's `BS_CT`/`BS_GA` `.ht2` + `.mmi` indexes (already built). **Downstream:** the emitted Bismark BAM stays consumable by the ported Rust tools (byte-identical by construction).
- **Aligner-agnostic core** (Phase 4 of the faithful port) was built precisely to make these wrappers additive.

## 7. Follow-ups (out of this epic)
- **v2 epic:** rammap (pure-Rust minimap2) + combined-index single/dual alignment — concordance-gated, spike-first (`project_aligner_v2_alternative_models`).
- HISAT2/minimap2 in the perl-oracle CI gate (OQ5 follow-up).
