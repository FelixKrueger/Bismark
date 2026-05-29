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
| B | Core genome-wide report | `phase-b-core-report/` | ✅ Complete |
| C | `--gzip` + `--split_by_chromosome` | `phase-c-gzip-split/` | 📋 Planned |
| D | `--merge_CpGs` (+ `--discordance`) | `phase-d-merge-cpgs/` | 📋 Planned |
| E | Real-data byte-identity gate (colossal) | `phase-e-byte-identity-gate/` | 📋 Planned |

## History

- **2026-05-29** — SPEC rev 0→1 drafted + manual-review questions resolved; phases A–E confirmed by Felix; EPIC.md + this PROGRESS.md created; worktree `../Bismark-c2c` set up + isolation verified. Phase A `PLAN.md` written.
- **2026-05-29** — Dual plan-review of Phase A complete (`PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md`). Both APPROVE-WITH-CHANGES; both confirmed Deviation D1 (HashMap over IndexMap) sound. 2 Critical (context-conditional output-stem strip; output_dir='' vs parent_dir=getcwd() defaults) + several Important.
- **2026-05-29** — Review findings folded into Phase A `PLAN.md` rev 1 (C1, C2, glob-tier semantics, `--CX` clap surface, `MalformedFastaHeader`, noodles facts resolved, Genome no-public-iterator invariant, +8 test rows). SPEC synced to rev 2 (§6/§10.4/§11).
- **2026-05-29** — `IMPL.md` (TDD task list, 30-item coverage checklist → 10 tasks) written; **Phase A implemented**: crate `bismark-coverage2cytosine` (lib+bin), 40 tests pass, clippy clean, workspace builds, siblings untouched.
- **2026-05-29** — Dual code-review (`CODE_REVIEW_A/B.md`, both APPROVE, no Critical/High) + plan-manager (`COVERAGE.md`, **verdict COMPLETE**). Folded Medium/Low fixes (dotfile glob B-1, error conflation, header-divergence doc+test, unused-dep removal, .fasta-tier test); **43 tests pass**, clippy clean. SPEC→rev 3. **✅ Phase A COMPLETE.**
- **2026-05-29** — Phase A committed (`1f382e7` code, `4fea2be` docs, `c3876d2` issue-link), pushed; epic **#891** filed + on board (In Progress / 2-Next / L); **PR [#892](https://github.com/FelixKrueger/Bismark/pull/892)** opened vs `rust/iron-chancellor`.
- **2026-05-29** — Phase B `PLAN.md` written (rev 0) + dual plan-review (`PLAN_REVIEW_A/B.md`, both APPROVE-WITH-CHANGES; single-kernel + coordinate arithmetic + `%.2f` parity verified correct by both). Folded → **rev 1**: 1 Critical (C1 non-contiguous re-flush, plan-clarity) + Important (fresh-buffer seeding, CRLF/malformed parse policy, dup/blank-line, names_sorted≡%processed doc) + 9 test rows (V16–V24).
- **2026-05-29** — `IMPL.md` (TDD, 26-item checklist → 9 tasks) + **Phase B implemented**: `src/{cov,report,summary}.rs` + run wiring. 67 tests pass (incl. byte-identity goldens for {CpG, --CX, --zero_based, --threshold} vs Perl v0.25.1, generated locally); clippy clean.
- **2026-05-29** — Dual code-review (`CODE_REVIEW_A/B.md`, both **APPROVE**, no Critical/High; both cross-checked binary vs live Perl → byte-identical) + plan-manager (`COVERAGE.md`, INCOMPLETE = test-coverage gaps only). Closed all gaps (V10/V21/V22/V23 discriminating tests + B-M1 raw-byte compare): **71 tests pass**, clippy clean. **✅ Phase B COMPLETE.** Next: commit + push onto PR #892; then Phase C (--gzip + --split_by_chromosome).
