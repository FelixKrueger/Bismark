# Progress — `bismark-nome-filtering`

**Feature:** Rust port of standalone Perl `NOMe_filtering` v0.25.1 (byte-identical).
**Branch / worktree:** `rust/nome-filtering` @ `../Bismark-nome` (off `origin/rust/iron-chancellor` @ `2b05ec8`).
**Status:** ✅ Byte-identity COMPLETE (Phases A+B+C all GREEN) — v1.0 tag pending

> Standalone tool — **distinct** from `coverage2cytosine --nome-seq` (the in-c2c flag on `rust/c2c-v1x`).

## Pipeline

| Step | Artifact | State |
|------|----------|-------|
| Plan / SPEC | `SPEC.md` | ✅ rev 1 (dual-review folded) |
| Manual review (Felix) | — | ✅ done |
| Dual plan-review | `PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md` | ✅ done — both APPROVE; §8/§9 arithmetic verified vs live Perl (zero correctness defects); all findings folded into rev 1 |
| Implementation plan | `IMPL_phase-A.md` | ✅ Phase A (12-row coverage → 6 TDD tasks) |
| Implement (Phase A) | crate `rust/bismark-nome-filtering` + `bismark_io::genome` | ✅ done — workspace builds, 33 tests pass, clippy clean |
| Dual code-review + plan-manager (Phase A) | `CODE_REVIEW_A/B.md`, `COVERAGE.md` | ✅ done — both reviewers APPROVE (no Critical/High); plan-manager **COMPLETE** (18/18 DONE) |

## Phases (see SPEC §11)

| Phase | Scope | State |
|-------|-------|-------|
| A | scaffold + clap CLI + promoted `bismark_io::genome` + errors + filename + `perl_substr` | ✅ **COMPLETE** (33 tests green; clippy clean; dual code-review APPROVE; plan-manager 18/18 DONE) |
| B | core per-read lookup + always-gzipped `.manOwar.txt.gz` output | ✅ **COMPLETE — BYTE-IDENTICAL to Perl v0.25.1** (63 crate tests + direct `cmp`; clippy clean). Dual code-review APPROVE (no Critical/High; both regenerated goldens from live Perl + ran differential tests); plan-manager COMPLETE (15/15 + 9/9). Code-review hardening **APPLIED**: committed reverse-strand-counting + multi-chr-ordering goldens, `GzEncoder<BufWriter<File>>`, accepted-divergences documented (SPEC rev 2). 65 crate tests green. |
| C | real-data byte-identity gate (**oxy**) → v1.0 tag | ✅ **GREEN (2026-06-01)** — full 10M SE (10.29 GB `--yacht`) Perl≡Rust **byte-identical** (8,494,374 lines, md5 `7bdf7d5d…`; C2 gz-input also PASS); Rust ~3.4× Perl; oxy crate tests green. See `PHASE_C_RESULT.md`. Tag pending Felix's go. |

## Key decisions (SPEC §3)

- **D1** genome reader → promoted to `bismark-io` (additive, no version bump).
- **D2** inert flags → accept-and-ignore (Perl-faithful); reproduce only `--merge_CpGs`+`--CX` die.
- **D3** reverse-read-at-chr-start edge → faithfully replicate via `perl_substr`.
- **D4** empty / all-`^Bismark` input → **replicate** Perl (write header, then error → header-only `.gz` + non-zero exit).
- **D5** genome error ownership → **module-local `GenomeError`** in `bismark_io::genome` (keeps public `BismarkIoError` untouched).

## History

- **2026-05-31** — Kickoff. Worktree created; SPEC.md rev 0 + PROGRESS.md written; memory `project_nome_filtering_port` recorded. Paused for Felix's manual review.
- **2026-05-31** — Manual review done. Dual plan-review launched (two parallel `plan-reviewer` Agents, fresh contexts); both APPROVE, both verified §8/§9 arithmetic vs live Perl (zero correctness defects). Felix resolved the one open decision (empty-input → **replicate**). All findings folded into **SPEC rev 1** (D4, D5, `--dir` path contract, same-position last-wins, single-strip filename, byte-scan, `perl_substr` `start==L`, `.fa.gz` footgun, expanded §12/§13 P11–P17). **Ready for the explicit "implement" trigger.**
- **2026-05-31** — **Committed Phases A+B to `rust/nome-filtering`** (2 commits: `6ab03f7` feat — the crate + bismark-io genome promotion + Perl-golden fixtures incl. the binary `.gz`; `1d05cf4` docs — SPEC/IMPL/reviews/coverage). Working tree clean; HEAD verified via `git show --stat` (the post-commit README hook's bedgraph diff is the known spurious parallel-session artifact — ignored). **Phase C planned** — `IMPL_phase-C.md` (real-data byte-identity gate on **oxy** [Felix directive 2026-05-31, the c2c Phase-E host]: build via rustup, generate a real `--yacht` input, `nome_gate.sh` Perl-vs-Rust `cmp`, RELEASE checklist → `bismark-nome-filtering-v1.0.0-beta.1` tag).
- **2026-05-31** — Follow-up `docs(nome-filtering): Phase C plan + progress` commit; Phase C host set to **oxy** (not colossal) per Felix; SPEC bumped to rev 3.
- **2026-06-01** — **Phase C executed on oxy → GREEN.** Pushed `rust/nome-filtering`; built release binary on oxy (rustup, cargo 1.96); generated real `--yacht` (Perl extractor `-s --yacht` on the 10M SE BAM → 10.29 GB). Full byte-identity gate: Perl≡Rust **byte-identical** (8,494,374 lines, md5 `7bdf7d5d…`); C2 gz-input PASS (md5 == plain). Rust ~3.4× Perl (1:28 vs 5:01), ~3.1 GB RSS. oxy crate tests green. Recorded in `PHASE_C_RESULT.md`. **Remaining: CHANGELOG/README + commit driver/result + tag `bismark-nome-filtering-v1.0.0-beta.1` (pending Felix's go).**
- **2026-05-31** — **Phase B code-review hardening applied** (Felix go): added committed Perl-golden fixtures for a reverse-strand-*counting* read (`rev.golden` → `1 1 0 0`) + multi-chromosome emission order (`multichr.golden`, chr2-before-chr1); switched output to `GzEncoder<BufWriter<File>>`; documented the accepted divergences (malformed-line skip + grouping side-effect, coord reformat, multi-char field, non-UTF-8→exit1, exit 1 vs 255, unreachable warn-skip) in SPEC rev 2 (§2 + D4 byte-count fix). Final gate: 65 crate tests + 210 bismark-io tests green, clippy `-D warnings` clean.
- **2026-05-31** — **Phase B implemented + verified BYTE-IDENTICAL.** `IMPL_phase-B.md` (15-row coverage → 9 TDD tasks); `code-implementation` shipped `src/nome.rs` (revcomp, classify, cytosine_lookup, process_read, per_read_filtering, write_report) + `run()` restructure (header-before-loop D4) + Perl-golden fixtures (`tests/data/phase_b/` via `generate_goldens.sh`). 63 crate tests green (7 decompress-then-compare goldens covering ACG/TCG accept, GCG reject, GpC-CHG/CHH, the edge asymmetry, D4 empty-input, gz-input, CRLF, unknown-chr, N-context); direct Perl-vs-Rust `cmp` = BYTE-IDENTICAL; clippy `-D warnings` clean. Pre-flagged SPEC correction: the `else→warn-skip` context branch is unreachable (tri[0] is always C). Under dual code-review + plan-manager. Nothing committed (working-tree only).
- **2026-05-31** — **"implement" trigger given. Phase A implemented + verified.** `implementation-planner` → `IMPL_phase-A.md` (12-row coverage → 6 TDD tasks); `code-implementation` shipped: `bismark_io::genome` promotion (additive `flate2`, no version bump, module-local `GenomeError`) + new `bismark-nome-filtering` crate (CLI/validate, `filename`, `perl_substr`, errors, Phase-A `run()`). `cargo build --workspace` PASS, 33 tests green, clippy `-D warnings` clean. Dual `code-reviewer` (fresh contexts, recommend-only): both **APPROVE**, no Critical/High; `plan-manager` Mode A+B: **COMPLETE 18/18**. Non-blocking Medium/Low items pre-folded to Phase B. Nothing committed (working-tree only).
