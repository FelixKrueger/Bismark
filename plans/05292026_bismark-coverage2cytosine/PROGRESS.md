# PROGRESS — `bismark-coverage2cytosine`

**Type:** Epic (5 phases) · **Branch/worktree:** `rust/coverage2cytosine` @ `../Bismark-c2c`
**Epic:** [`EPIC.md`](./EPIC.md) · **Design contract:** [`SPEC.md`](./SPEC.md)
**Last updated:** 2026-05-29

## Status legend
📋 Planned → 📝 Planning → 🚧 Implementing → ✅ Complete · ⏸️ Deferred · ❌ Excluded

## Phase status

| # | Phase | Plan | Status |
|---|-------|------|--------|
| A | Scaffold + CLI + genome reader | `phase-a-scaffold-cli-genome/` | ✅ Complete |
| B | Core genome-wide report | `phase-b-core-report/` | 📋 Planned |
| C | `--gzip` + `--split_by_chromosome` | `phase-c-gzip-split/` | 📋 Planned |
| D | `--merge_CpGs` (+ `--discordance`) | `phase-d-merge-cpgs/` | 📋 Planned |
| E | Real-data byte-identity gate (colossal) | `phase-e-byte-identity-gate/` | 📋 Planned |

## History

- **2026-05-29** — SPEC rev 0→1 drafted + manual-review questions resolved; phases A–E confirmed by Felix; EPIC.md + this PROGRESS.md created; worktree `../Bismark-c2c` set up + isolation verified. Phase A `PLAN.md` written.
- **2026-05-29** — Dual plan-review of Phase A complete (`PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md`). Both APPROVE-WITH-CHANGES; both confirmed Deviation D1 (HashMap over IndexMap) sound. 2 Critical (context-conditional output-stem strip; output_dir='' vs parent_dir=getcwd() defaults) + several Important.
- **2026-05-29** — Review findings folded into Phase A `PLAN.md` rev 1 (C1, C2, glob-tier semantics, `--CX` clap surface, `MalformedFastaHeader`, noodles facts resolved, Genome no-public-iterator invariant, +8 test rows). SPEC synced to rev 2 (§6/§10.4/§11).
- **2026-05-29** — `IMPL.md` (TDD task list, 30-item coverage checklist → 10 tasks) written; **Phase A implemented**: crate `bismark-coverage2cytosine` (lib+bin), 40 tests pass, clippy clean, workspace builds, siblings untouched.
- **2026-05-29** — Dual code-review (`CODE_REVIEW_A/B.md`, both APPROVE, no Critical/High) + plan-manager (`COVERAGE.md`, **verdict COMPLETE**). Folded Medium/Low fixes (dotfile glob B-1, error conflation, header-divergence doc+test, unused-dep removal, .fasta-tier test); **43 tests pass**, clippy clean. SPEC→rev 3. **✅ Phase A COMPLETE.** Next: Phase B plan (core genome-wide report).
