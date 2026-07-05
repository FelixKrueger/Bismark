# SPEC — `bismark` aligner v1.x: HISAT2 + minimap2 wrappers

- **Created:** 2026-06-05 · **rev 0** (for manual review — forks pre-resolved with Felix where noted).
- **Branch / worktree:** `rust/aligner-v1x` @ `~/Github/Bismark-aligner` (off `origin/rust/iron-chancellor` @ `fc38191`, which carries the merged faithful Bowtie2 port through Phase 10 + the extractor inline-streaming #947).
- **Crate / binary:** the existing `rust/bismark-aligner` (`bismark_rs`) — **additive**, no new crate.
- **Oracle / pins:** Perl Bismark **v0.25.1** · **HISAT2 2.2.2** · **minimap2 2.31-r1302** · samtools 1.23.1 (oxy `bismark-test` env). (Bowtie2 2.5.5 is the already-shipped backend.)
- **Predecessor:** the faithful Bowtie2 epic `plans/05312026_bismark-aligner/` (SPEC + EPIC, Phases 0–10, all merged). This is the deferred **Phase J** of that epic, promoted to its own v1.x epic (Felix's 2026-05-31 call).

---

## 1. Goal

Add **HISAT2** and **minimap2** as alternative alignment backends to the Rust `bismark` aligner, each **byte-identical to Perl Bismark v0.25.1 driving the same pinned aligner**. The merge/scoring/XM/SAM-output/report core built in the faithful Bowtie2 port is reused; the new work is the per-aligner **wrapper** (binary resolution + version pin, option assembly, SAM-parse deltas, index handling, output naming, report wording) plus — for minimap2 — a **merge-path adaptation** (it does not restrict strand).

**Acceptance = byte-identical decompressed SAM content + reports + aux**, exactly as the Bowtie2 gate, but vs Perl + the respective pinned aligner — **contingent on a per-aligner determinism spike passing first** (§4).

---

## 2. Forks — RESOLVED with Felix (2026-06-05)

1. **Sequencing:** ✅ **HISAT2 first, then minimap2.** HISAT2 is closest to Bowtie2 (same instance model + `--norc`/`--nofw`, near-identical SAM modulo `ZS`-vs-`XS`); minimap2 is the more divergent second wrapper.
2. **Gate methodology:** ✅ **Spike-first byte-identity.** A per-aligner determinism spike (à la the Bowtie2 Phase 0) runs FIRST and *gates* the byte-identity premise; only if a spike shows non-determinism do we fall back to a concordance gate for that aligner. Byte-identity is the target for both.

**Standing (from the faithful port, unchanged):** byte-identity is on **decompressed** SAM content (`samtools view` + `-H`, `@PG` normalised), not raw BAM bytes; adjudicated on Linux/oxy; output is fully Bismark-generated (only POS/CIGAR/which-alignment-wins is aligner-derived); single-thread-per-instance (or reorder-controlled) for determinism.

---

## 3. What's reused vs new (the seam)

**Reused unchanged** (aligner-agnostic core, already shipped + gated for Bowtie2):
- `convert.rs` — read conversion (C→T / G→A) — *mostly* (minimap2 needs the `/1 /2` retention delta, §3.2).
- `merge.rs` / `mapq.rs` — lockstep merge, best-alignment scoring, `calc_mapq` — *directly* for HISAT2; *adapted* for minimap2 (§3.2).
- `methylation.rs` — genomic-seq extraction + `XM`/`XR`/`XG` call — incl. the **`N`-CIGAR spliced-read** path (already present, Perl 4372) that HISAT2 can exercise.
- `output.rs` / `report.rs` / `aux_out.rs` — SAM/BAM output, reports, `--unmapped`/`--ambiguous`/`--ambig_bam` — modulo per-aligner **naming + report wording** (§3.1/§3.2).
- `parallel.rs` — order-preserving `--multicore`/`--parallel` (worker-invariant) — aligner-agnostic.

**New / generalized:**
- `aligner.rs` — currently Bowtie2-only (`PINNED_BOWTIE2_VERSION`, `resolve_bowtie2_path`, version parse). **Generalize to a multi-backend abstraction**: an `Aligner` enum/trait carrying {binary name, path-resolution, version pin + parse, option assembly, index extension(s), invocation shape, SAM-parse quirks, output-name token, report wording}.
- `options.rs` — `aligner_options` assembly is Bowtie2-shaped (`-q --score-min L,0,-0.2 --ignore-quals` …). Add **per-aligner option assembly** (HISAT2 + minimap2 defaults from the Perl `process_command_line`).
- `cli.rs` — the `--hisat2`/`--minimap2`/`--path_to_*`/splice/preset surface is **already parsed** (deferred); wire it.
- `align.rs` — `SamRecord` **already** parses `XS:i:`-or-`ZS:i:` (Phase 3 anticipated HISAT2); verify + extend for minimap2 tags if needed.

### 3.1 HISAT2 deltas (thin wrapper)
- **Binary:** `hisat2` (resolve via `--path_to_hisat2` dir or PATH); pin **2.2.2** (`hisat2-align-s version 2.2.2`).
- **Index:** `BS_CT`/`BS_GA` `*.ht2` (present on oxy). Invocation: `hisat2 <opts> --norc|--nofw -x <BS_index> -U <reads>` piped — **same instance/strand model as Bowtie2** (`$hisat2_options = $aligner_options` + per-instance `--norc`/`--nofw`, Perl `single_end_align_fragments_..._hisat2`).
- **SAM parse:** 2nd-best score tag is **`ZS:i:`** not `XS:i:` (Perl 2791/3397) — already handled by the parser.
- **Spliced reads:** HISAT2 may emit `N` CIGAR ops (spliced) → genomic-seq extraction already handles `N` as a skipped region.
- **Options (✅ spike-confirmed, Phase 1 — corrects this section's pre-spike guess):** HISAT2 `aligner_options` = the **Bowtie2 base `-q --score-min L,0,-0.2 --ignore-quals`** PLUS exactly **`--no-softclip --omit-sec-seq`** (it DOES use `--score-min L,0,-0.2`; the earlier "NOT --score-min" / `--no-1mm-upfront`/`--no-spliced-alignment` guess was **wrong**). Per-instance `--norc`/`--nofw` added downstream; exact PE ordering Perl-verified in Phase 2.
- **Naming/report:** `_bismark_hisat2{,_pe}.bam` / `_SE`/`_PE_report.txt`; report line "Bismark was run with HISAT2 against …".
- **Best-alignment caveat:** Bismark's own help says HISAT2 best-alignment is "not exactly known" — *irrelevant to byte-identity* (we replicate Perl's path), but it means we cannot reason from first principles; the gate (spike + Perl-differential) is the authority.

### 3.2 minimap2 deltas (wrapper + merge adaptation)
- **Binary:** `minimap2`; pin **2.31-r1302**.
- **Index:** single `.mmi` (`BS_CT.mmi`/`BS_GA.mmi`, present on oxy). Invocation: `minimap2 <opts> <BS.mmi> <reads>` — **positional**, no `-x`/`-U` (Perl `single_end_align_fragments_..._minimap2`).
- **No strand restriction:** `--norc`/`--nofw` are **commented out** in the Perl minimap2 path → each instance aligns **both strands** → the **unique-vs-ambiguous / best-alignment arithmetic differs** from the Bowtie2/HISAT2 model. ⚠️ **This is the load-bearing minimap2 risk**: the Phase-4 merge/selection core assumes strand-restricted instances. minimap2 needs the merge path studied + adapted (and likely its own determinism/selection characterization in the spike).
- **`/1 /2` retention:** minimap2 does NOT strip the trailing `/1 /2`; Bismark *appends* `/1`/`/2` to the identifier (Perl 5947/5955) → converter/ID handling delta.
- **Preset/scoring:** `-ax sr` (short-read) default (Perl comments); minimap2 has its own scoring (no `--score-min L,0,-0.2`).
- **SAM-default output**, `_bismark_…` minimap2 naming; report line "Bismark was run with minimap2 against …".
- **Determinism:** minimap2 has a `--seed` + multithread output reordering → the spike must confirm single-thread (or `--seed`-pinned) run-to-run determinism + a stable output order. **This is the #1 minimap2 risk.**

---

## 4. Methodology — spike-first byte-identity (per aligner)

Mirrors the Bowtie2 Phase 0. For **each** aligner, before wrapper implementation:
- **Determinism spike** on a small real subset (oxy): run the pinned aligner twice with identical inputs → alignment records byte-identical run-to-run? Settle reorder/seed flags (HISAT2 `--no-...`? minimap2 `--seed`/single-thread). For **minimap2**, *additionally* characterize the both-strand selection (how many reads change unique↔ambiguous vs the Bowtie2 model) so the merge adaptation is grounded.
- **If deterministic → byte-identity gate** (Perl + pinned aligner, identical argv, decompressed SAM + reports + aux, `@PG` policy from the Bowtie2 port). **If not → concordance gate** for that aligner (documented, Felix-decision).

---

## 5. Proposed phasing (for the EPIC — `epic-writer` after SPEC approval)

- **Phase 0H — HISAT2 determinism spike** (oxy; gates the HISAT2 byte-identity premise + settles option/flag set).
- **Phase 1H — HISAT2 wrapper**: generalize `aligner.rs` → multi-backend; HISAT2 binary resolve + version pin + option assembly; wire `--hisat2`/`--path_to_hisat2`; `.ht2` index discovery; naming/report wording. 🎯 byte-identity gate (SE+PE, directional first, then non-dir/pbat) at 10k/1M on oxy. Bowtie2 paths byte-frozen.
- **Phase 2H — HISAT2 full-scale gate** (oxy real data) — or fold into a combined v1.x full-scale gate at the end.
- **Phase 0M — minimap2 determinism + selection spike** (oxy; the both-strand arithmetic + determinism — the highest-risk spike).
- **Phase 1M — minimap2 wrapper + merge adaptation**: positional `.mmi` invocation; `-ax sr` option assembly; `/1 /2` retention in convert/ID; the no-strand-restriction merge/selection path; naming/report. 🎯 byte-identity gate.
- **Phase 2M — minimap2 full-scale gate.**
- **Phase 3 — combined v1.x full-scale real-data gate on oxy** (HISAT2 + minimap2, SE+PE, library types as the spikes justify) + docs/journal bump + PR.

*(Phase count/splits are a starting point; `epic-writer` finalizes. Each phase runs the full plan→dual-review→implement→dual-code-review+plan-manager→oxy-gate pipeline, as the Bowtie2 phases did.)*

---

## 6. Shared assumptions
- All bisulfite indexes (Bowtie2 `.bt2`, HISAT2 `.ht2`, minimap2 `.mmi`) **already exist** on oxy `~/bismark_benchmarks/genome/Bisulfite_Genome/` — no genome-prep prerequisite for the gate. (Mouse GRCm39 likewise carries `.ht2`; verify `.mmi` if RRBS cells are gated.)
- Bowtie2 paths must stay **byte-frozen** through the generalization (regression-guard every HISAT2/minimap2 phase against the existing Bowtie2 gate).
- BAM/SAM I/O via `noodles`; output fully Bismark-generated; `@PG` policy (Bismark line reconstructed from argv, samtools line normalised) as in the Bowtie2 port.
- Public-artifact constraint: HISAT2/minimap2 are general aligners + Bismark's declared dependencies → naming them is fine.

## 7. Open questions — ✅ RESOLVED (Felix, 2026-06-05: approved all leans below)
- **OQ1:** Phasing granularity — per-aligner full-scale gate (Phase 2H/2M) vs one combined full-scale gate at the end (Phase 3)? *(Lean: per-aligner 10k/1M gate inline + ONE combined full-scale gate at the end, to minimise oxy hours.)*
- **OQ2:** Cells per aligner — SE+PE directional are must-haves; do HISAT2/minimap2 need non-dir/pbat gated, or directional-only first? *(Lean: directional SE+PE first; add non-dir/pbat per the faithful-port precedent if the spikes show no surprises.)*
- **OQ3:** minimap2 both-strand merge adaptation — is byte-identity to Perl's minimap2 path actually reachable, or does minimap2's selection have an irreducible non-determinism that forces concordance? **The Phase-0M spike answers this before we commit** — flag as the highest-risk unknown.
- **OQ4:** RRBS / mouse cells for v1.x, or human WGBS only? *(Lean: human WGBS for the wrappers; RRBS optional at the combined full-scale gate.)*
- **OQ5:** Version pinning into CI — add HISAT2 2.2.2 / minimap2 2.31 to the perl-oracle CI gate, or oxy-only? *(Lean: oxy-only for the wrappers; CI pin is a follow-up.)*

## 8. Conventions (match the faithful port)
- New work on `rust/aligner-v1x`; plan/review docs under **this** dir (`plans/06052026_bismark-aligner-v1x/`) so they ride the PRs; per-aligner sub-dirs as the EPIC defines.
- Workflow per phase: plan-writer → manual review → dual plan-review → (trigger) implement → dual code-review + plan-manager → oxy gate. Spikes via the `spike` skill.
- Re-base `rust/aligner-v1x` onto iron-chancellor before each PR (force-push denied → fresh branches); squash-merge on Felix's explicit ask.
