# PROGRESS — `bismark-coverage2cytosine` v1.x (specialty methylation modes)

**Type:** Epic (4 phases) · **Crate:** `bismark-coverage2cytosine` · **Branch:** v1.x off `rust/iron-chancellor` (after PR #892 merges)
**Epic:** [`EPIC.md`](./EPIC.md) · **Predecessor:** [`../05292026_bismark-coverage2cytosine/`](../05292026_bismark-coverage2cytosine/) (v1.0, shipped + tagged `…v1.0.0-beta.1`)
**Last updated:** 2026-05-31

## Status legend
📋 Planned → 📝 Planning → 🚧 Implementing → ✅ Complete · ⏸️ Deferred · ❌ Excluded

## Phase status

| # | Phase | Plan | Status |
|---|-------|------|--------|
| 1 | GpC report + NOMe-Seq (`--gc` / `--nome-seq`) | `phase1-gpc-report-nome-seq/` | 📋 Planned |
| 2 | DRACH m6A (`--drach` / `--m6A`) | `phase2-drach-m6a/` | 📋 Planned |
| 3 | FFS context columns (`--ffs`) | `phase3-ffs/` | 📋 Planned |
| 4 | Real-data byte-identity gate | `phase4-byte-identity-gate/` | 📋 Planned |

## History

- **2026-05-31** — Epic skeleton created (`EPIC.md` + 4 phase subdirs). Scopes the four specialty modes deferred from v1.0 (Felix, 2026-05-29). Decisions (Felix, 2026-05-31): `--gc` + `--nome-seq` are **one** phase (nome-seq sets `--gc`); **DRACH included** (the Perl `// TODO` is read as a vestigial leftover — its phase plan confirms this first). **Precondition:** v1.0 PR #892 merges to `rust/iron-chancellor` before this epic's branch forks. Phases 1–3 mutually independent; #4 (oxy real-data gate, extends `c2c_byte_identity_matrix.sh`) gates the v1.x tag. Per-phase plans not yet written.
