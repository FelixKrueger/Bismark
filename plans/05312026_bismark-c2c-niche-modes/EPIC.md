# EPIC — `bismark-coverage2cytosine` v1.x (specialty methylation modes)

**Status:** rev 1 (2026-05-31) — Phases 1–3 plans drafted (all **byte-identical to Perl v0.25.1**; DRACH Q1 resolved → no divergence). Phase 4 (oxy gate) plan pending. Awaiting manual review → per-phase dual plan-review.
**Predecessor:** the v1.0 epic [`../05292026_bismark-coverage2cytosine/EPIC.md`](../05292026_bismark-coverage2cytosine/EPIC.md) — Phases A–E shipped + byte-identity-proven on oxy (full hg38); tagged **`bismark-coverage2cytosine-v1.0.0-beta.1`** (PR #892).
**Crate:** the existing `bismark-coverage2cytosine` in the `rust/` workspace.
**Branch:** a new v1.x branch off `rust/iron-chancellor` once PR #892 merges (see §3 Precondition).
**Design contract:** the v1.0 [`SPEC.md`](../05292026_bismark-coverage2cytosine/SPEC.md) §2 (deferred scope), §3 (flags 14–17 marked ⛔ v1.x), §15. Each phase drafts its own SPEC additions with its plan.

---

## 1. Goal

Extend the byte-identity-proven v1.0 crate with the four specialty methylation modes deferred from v1.0 (Felix, 2026-05-29), each **byte-identical to Perl `coverage2cytosine` v0.25.1** (STDERR exempt), reusing the v1.0 genome-walk / report-writer / cov-parse / CLI infrastructure. Each phase flips its flag(s) from the v1.0 CLI-**rejected** state (today they error with "not supported in the Rust port yet (v1.x); use Perl coverage2cytosine") to **supported**.

## 2. Scope

**In (v1.x):**
- `--gc` / `--gc_context` (GpC-context report) **+** `--nome-seq` (NOMe-Seq filtering) — combined (nome-seq sets `--gc`; Felix, 2026-05-31).
- `--drach` / `--m6A` (DRACH-motif m6A filtering).
- `--ffs` (tetra/penta/hexamer nucleotide-context columns).
- Extending the real-data byte-identity harness (`scripts/c2c_byte_identity_matrix.sh`) + `RELEASE_CHECKLIST_c2c.md` with cells for the new modes; gating the v1.x tag.

**Out:**
- Anything beyond these four modes; the v1.0 core (done); non-c2c tools.
- A perf pass (parallel genome walk) — still a separate candidate (v1.0 SPEC §10.7), not part of this epic.

## 3. Precondition

v1.0 must be **merged** to `rust/iron-chancellor` (PR #892, currently green A–E). This epic's branch forks from the merged result so the niche modes build on the shipped core.

## 4. Phase breakdown (execution order)

Each phase is independently byte-identity-testable against Perl v0.25.1 and ships with its own full cycle (plan → manual review → dual plan-review → implement → dual code-review + plan-manager), mirroring the v1.0 cadence.

- **Phase 1 — GpC report + NOMe-Seq (`--gc`/`--gc_context` + `--nome-seq`).** Port `generate_GC_context_report` (the GpC-context report stream) and `--nome-seq` (which sets `--gc` and adds the ACG/TCG CpG + GpC filtering). Combined because nome-seq is a thin filter on top of the GpC machinery (Felix, 2026-05-31). Un-reject both flags in `validate()` + `--help`. (Perl ln 2022 / 2025.)
- **Phase 2 — DRACH m6A (`--drach`/`--m6A`).** Port `generate_DRACH_report` (a **standalone early-exit mode**; DRACH-motif m6A filtering, ~300 LOC). The Perl `// TODO` (`:1369`, bottom-strand C position) was investigated (geometry + main-report convention + live `--CX` agreement all show `pos-1` is the C's coordinate) and **resolved (Felix, 2026-05-31): BS-seq is cytosine-specific, so the C anchor `pos-1` is intended — byte-identical to Perl on both strands**, no divergence. Plan: `phase2-drach-m6a/PLAN.md` rev 1. (Perl ln 2028.)
- **Phase 3 — FFS context columns (`--ffs`).** Add the tetra/penta/hexamer nucleotide-context columns to the cytosine-report line (a report-line format extension). (Perl ln 2023.)
- **Phase 4 — Real-data byte-identity gate.** Extend `c2c_byte_identity_matrix.sh` with `--gc` / `--nome-seq` / `--drach` / `--ffs` cells (+ the cross-cell differentials that prove each flag actually changes the output), run the full-genome matrix on oxy, and gate the `bismark-coverage2cytosine-v1.x` tag. Reuses the v1.0 harness pattern + the mandatory §0 fail-CLOSED self-tests.

## 5. Sub-plan table

| # | Phase | Plan file | Depends on |
|---|-------|-----------|------------|
| 1 | GpC report + NOMe-Seq | `phase1-gpc-report-nome-seq/PLAN.md` | v1.0 merged |
| 2 | DRACH m6A | `phase2-drach-m6a/PLAN.md` | v1.0 merged |
| 3 | FFS context columns | `phase3-ffs/PLAN.md` | v1.0 merged |
| 4 | Real-data byte-identity gate | `phase4-byte-identity-gate/PLAN.md` | #1, #2, #3 |

Phases 1–3 are **mutually independent** (different flags / code paths), so they can be planned and implemented in any order or in parallel; Phase 4 gates them all.

## 6. Shared assumptions

1. **Byte-identity to Perl v0.25.1** for every new output stream; STDERR exempt (same contract as v1.0).
2. **Reuse v1.0 infrastructure** — `genome.rs` reader, cov parse, `ReportWriter` (plain/gz), `ResolvedConfig`/`validate()`, `BismarkC2cError`, and the `--gzip` / `--zero_based` / `--split_by_chromosome` / `-o` / `--dir` / `--parent_dir` machinery. Each phase flips its flag from rejected → supported (update `validate()` + the `--help` "(v1.x, rejected)" labels).
3. **Built on the merged v1.0** (§3 Precondition).
4. **Testing model from v1.0:** local Perl-v0.25.1 goldens on tiny fixtures + the oxy real-data gate (Phase 4). Worktree isolation; never disrupt the shared checkout.
5. **Niche-flag interactions mirror Perl** `process_commandline`: `--nome-seq` sets `--gc`; honor any mutex/coupling of these flags with the core flags exactly as Perl does.

## 7. Integration points

- **v1.0 → all phases:** the shipped genome walk + report writer + CLI are the substrate; each phase adds a context-classification variant and/or an output stream, never a rewrite.
- **Phase 1 internal:** `--nome-seq` builds on the `--gc` GpC machinery (same phase).
- **Phases 1–3 → Phase 4:** Phase 4 runs the real binary across the new flags end-to-end vs Perl — the integration test, mirroring v1.0 Phase E.
- **Harness reuse:** Phase 4 **extends** `scripts/c2c_byte_identity_matrix.sh` (the v1.0 fail-CLOSED driver) rather than writing a new one — add cells, keep the gzip-integrity / empty-tolerant / differential / disk-discipline machinery.

## 8. References
- Predecessor epic + SPEC: `../05292026_bismark-coverage2cytosine/`. Memory: `project_coverage2cytosine_port`.
- Perl source: `coverage2cytosine` v0.25.1 at the repo root (`generate_GC_context_report`, `generate_DRACH_report`, the `--ffs`/`--nome-seq` paths).
- Harness: `scripts/c2c_byte_identity_matrix.sh`, `RELEASE_CHECKLIST_c2c.md`.
