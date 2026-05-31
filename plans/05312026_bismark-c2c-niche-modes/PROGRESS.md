# PROGRESS — `bismark-coverage2cytosine` v1.x (specialty methylation modes)

**Type:** Epic (4 phases) · **Crate:** `bismark-coverage2cytosine` · **Branch:** v1.x off `rust/iron-chancellor` (after PR #892 merges)
**Epic:** [`EPIC.md`](./EPIC.md) · **Predecessor:** [`../05292026_bismark-coverage2cytosine/`](../05292026_bismark-coverage2cytosine/) (v1.0, shipped + tagged `…v1.0.0-beta.1`)
**Last updated:** 2026-05-31

## Status legend
📋 Planned → 📝 Planning → 🚧 Implementing → ✅ Complete · ⏸️ Deferred · ❌ Excluded

## Phase status

| # | Phase | Plan | Status |
|---|-------|------|--------|
| 1 | GpC report + NOMe-Seq (`--gc` / `--nome-seq`) | `phase1-gpc-report-nome-seq/PLAN.md` | ✅ Verified (dual code-review APPROVE + plan-manager COMPLETE) — awaiting commit |
| 2 | DRACH m6A (`--drach` / `--m6A`) | `phase2-drach-m6a/PLAN.md` | 📝 Planning |
| 3 | FFS context columns (`--ffs`) | `phase3-ffs/PLAN.md` | 📝 Planning |
| 4 | Real-data byte-identity gate | `phase4-byte-identity-gate/` | 📋 Planned |

## History

- **2026-05-31** — Epic skeleton created (`EPIC.md` + 4 phase subdirs). Scopes the four specialty modes deferred from v1.0 (Felix, 2026-05-29). Decisions (Felix, 2026-05-31): `--gc` + `--nome-seq` are **one** phase (nome-seq sets `--gc`); **DRACH included** (the Perl `// TODO` is read as a vestigial leftover — its phase plan confirms this first). **Precondition:** v1.0 PR #892 merges to `rust/iron-chancellor` before this epic's branch forks. Phases 1–3 mutually independent; #4 (oxy real-data gate, extends `c2c_byte_identity_matrix.sh`) gates the v1.x tag. Per-phase plans not yet written.
- **2026-05-31** — Phases 1–3 plans drafted (one drafting agent per mode, each grounded in the Perl source + live-Perl probing). **Phase 1** (`phase1-gpc-report-nome-seq/PLAN.md`, 285 ln): GpC report runs in addition to the core report; GpC filenames from raw `-o`; GpC has no `--zero_based` branch; `--nome-seq` sets `--gc`+threshold≥1, dies on `--CX`/`--merge_CpGs`, skips uncovered chromosomes. **Phase 2** (`phase2-drach-m6a/PLAN.md` rev 1, standalone early-exit): DRACH `// TODO` investigated → **resolved (Felix): BS-seq is cytosine-specific, `pos-1` C anchor is correct → byte-identical both strands**, no divergence. **Phase 3** (`phase3-ffs/PLAN.md`, 235 ln): pure append-3-columns (7→10), the forward `hexa_nt` negative-substr wrap is the gotcha, no mutexes. All byte-identical to Perl v0.25.1. **Awaiting manual review → per-phase dual plan-review** (none implemented; v1.0-merge precondition met).
- **2026-05-31** — **Phase 1 dual plan-review** done (`phase1-gpc-report-nome-seq/PLAN_REVIEW_A.md` APPROVE-WITH-CHANGES + `PLAN_REVIEW_B.md` APPROVE; both independently re-ran live Perl v0.25.1 and confirmed **every** surprising claim — **0 Critical, 0 byte-identity divergences**). Folded → **PLAN rev 1**: non-contiguous-chr re-appearance (forbid writer/buffer caching), NOMe×split filename + 4 added goldens (V18–V21), threshold-precision wording, NOMe mutex-check order, `config`-immutability, summary-under-threshold correction. **Phase 1 ready for the implement trigger.** Phases 2 & 3 still await their own dual reviews.
- **2026-05-31** — **Phase 1 IMPLEMENTED** (`/code-implementation`, on `rust/c2c-v1x`). New `src/gpc.rs` (GpC walk + writers); `report.rs` threaded NOMe (ACG/TCG filter + `.NOMe.CpG.cov` companion + `nome_cov_path` + uncovered-pass gate + `pct6` promotion); `lib.rs` wires `gpc::run_gpc` after report→merge. Tests: **131 crate tests green** (80 lib, 18 Phase-1 goldens, 11 B + 7 C + 10 D + 5 sanity — no regression); `fmt --check` + `clippy --all-targets -D warnings` clean. Goldens at `tests/data/phase1/` (+ `generate_goldens.sh`, full provenance); all 16 modes verified Rust≡Perl v0.25.1 byte-for-byte. **One documented deviation** (PLAN rev 2 §Implementation notes): the NOMe core `.cov` filename uses the **raw `-o`** base, not the stem (matches live Perl; the plan's `{stem}.NOMe.CpG.cov` prose was only right for a plain `-o`).
- **2026-05-31** — **Phase 1 VERIFIED.** Dual code-review (`CODE_REVIEW_A.md` + `CODE_REVIEW_B.md`, both **APPROVE**, 0 Critical/High/Medium; each independently re-ran live Perl v0.25.1 on 30+ from-scratch adversarial fixtures and confirmed byte-identity + the raw-`-o` deviation correctness + no Phase-D regression from the `pct6` move). 3 Low nits folded (stale `lib.rs` Phase-C doc; `v5` test strengthened to assert Rust output; uncovered-pass comment precision). Plan-manager (`COVERAGE.md`) **COMPLETE** — 40 DONE / 0 PARTIAL / 0 MISSING / 1 DEVIATED (accepted); every §5 step, §3 behavior, and §9 V1–V21 row maps to a passing test. **Awaiting Felix's go-ahead to commit `rust/c2c-v1x`** (still must not merge until the full v1.x epic lands).
