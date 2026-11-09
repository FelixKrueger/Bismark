# Plan Coverage Report

**Mode:** B (code vs plan; no separate IMPL.md — spec is PLAN.md rev 1)
**Plan(s):** `plans/11052026_1152-docker-wrapper-version/PLAN.md` (rev 1)
**Date:** 2026-11-05
**Verdict:** COMPLETE

## Summary

- Total items: 17
- DONE: 16
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 1 (documented, with reason)

Audited against the uncommitted diff in `/Users/fkrueger/Github/Bismark-1152` (base `da431b7`): `CHANGELOG.md` (+1), `Dockerfile` (+12/-7), `docker/bismark-canonical-wrapper.sh` (+5/-9). No other tracked files changed.

## Coverage ledger

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | Wrapper version read from `/usr/local/bin/bismark.bin --version`, token after `(Bismark Rust suite) v` up to next space | Behavior 1 / Outline 1 | DONE | `banner=$(/usr/local/bin/bismark.bin --version \|\| true)` + `sed -n 's/^bismark (Bismark Rust suite) v\([^ ]*\) .*/\1/p'`; absolute path; placed after the `bismark.bin` install step. |
| 2 | Shape check `^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$`, fails on empty/`unknown`/unsafe chars; message includes banner | Behavior 2 | DONE | `grep -Eqx` with the exact regex; error echoes `'$banner'` to stderr, `exit 1`. |
| 3 | Output check: first line of `/usr/local/bin/bismark -v` must equal `Bismark Version: v<token>` | Behavior 3 | DONE | `[ "$(/usr/local/bin/bismark -v \| head -n1)" = "Bismark Version: v$v" ] \|\| { …; exit 1; }` after chmod. |
| 4 | Remove runtime-stage `ARG BISMARK_SUITE_VERSION`; builder `ARG`/`ENV` stays | Behavior 4 | DONE | Only remaining occurrences are builder lines 17–18 (`ARG …=unknown`, `ENV …`). |
| 5a | Plain build prints `v3.1.0`, parsers give `3.1.0`, equals `bismark.bin --version` | Behavior 5 | DONE | Re-confirmed on local `bismark:1152`: wrapper `v3.1.0`, P1 `3.1.0`, P2 `3.1.0`, binary `v3.1.0`. |
| 5b | `--build-arg` build prints `v<v>` | Behavior 5 | DONE | Re-confirmed on `bismark:1152-beta`: `v3.2.0-beta.1`. |
| 5c | Prerelease: wrapper and binary both `v3.2.0-beta.1` | Behavior 5 | DONE | Same image; binary banner `v3.2.0-beta.1`. |
| 6 | Unchanged: wrapper runtime (`case`, `exec bismark.bin "$@"`), other layers, release workflows | Behavior 6 | DONE | Wrapper diff is comment-only; Dockerfile diff confined to the wrapper block and its comment; no `.github/` changes. |
| 7 | Dockerfile `RUN` block matches outline step 1 | Outline 1 | DONE | Byte-for-byte match with the outlined block (modulo line continuation indentation). |
| 8 | Dockerfile block comment: rewrite aligner-shape sentence; add one line that version is read from `bismark.bin --version` | Outline 2 | DONE | Now cites `bismark (Bismark Rust suite) v<suite> (…)`; added `# The wrapper's version is read from \`bismark.bin --version\`, so the two always agree.` |
| 9 | Wrapper header: lines 10–11 suite banner; drop 3.0.0-cut narrative; `__SUITE_VERSION__` filled from `bismark.bin --version` | Outline 2 | DONE | All three applied; GA/OD-7/3.0.0 narrative removed; example updated to 3.1.0. |
| 10 | CHANGELOG: one sentence under `### All tools`, links #1152 | Outline 3 / Q1 | DONE | Single sentence, first entry under `### All tools` in Unreleased, links issue #1152. |
| 11 | No Rust, test or workflow changes | Outline 4 | DONE | `git diff --stat` shows only the three files. |
| 12 | Q2: no runtime-stage override; wrapper always uses binary's version | Q2 | DONE | No runtime ARG/ENV remains. |
| 13 | Validation 1 (plain build) and 2 (`--build-arg 3.2.0-beta.1`) | Validation 1–2 | DEVIATED (documented) | Images built from a copy of the Dockerfile with only the `micromamba install` step removed, due to laptop TLS interception; reason recorded in "Implementation notes → Deviation". Results PASS and re-confirmed by this audit. |
| 14 | Validation 3: `bismark --help` exits 0 with composed help | Validation 3 | DONE | Re-confirmed exit 0 on both images. |
| 15 | Validation 4: guard arms (a) shape, (b) `vunknown`, (c) missing placeholder, plus unmodified body exits 0, in-image GNU sed; recorded in notes | Validation 4 | DONE | Taken from notes: (a)(b)(c) exit 1 with correct messages, plus extra (d) `v3.1.0&x` and valid prerelease stub; real binary exits 0. |
| 16 | Validation 5: release.yml `p1`/`p2` on check-2 image, `EXPECT=3.2.0-beta.1` | Validation 5 | DONE | Notes: both match. This audit's parser runs give P1 `3.2.0-beta.1`, P2 `3.2.0-beta.1 ` (trailing space inherent to Parser-2's `echo`/sed; unchanged from pre-existing behaviour). |

## Gaps (detail)

None.

### Note on item 13 (documented deviation)

**Expected:** `docker build .` of the real `Dockerfile`.
**Found:** validation images built from a copy minus the `micromamba install` layer; the reason (TLS interception of `conda.anaconda.org` inside Docker on this laptop) and the scope (all other lines byte-identical, including the new `RUN`, `ENV PATH`, base image and builder stage) are written in PLAN.md "Implementation notes → Deviation".
**Residual:** the full Dockerfile (and amd64) is exercised only by the release workflow on the next cut, as the plan's Self-Review already states.

## Test verification (Mode B only)

| Test | Where | Status |
|------|-------|--------|
| `bismark -v` on `bismark:1152` = `Bismark Version: v3.1.0` | local image (this audit) | PASS |
| Parser-1 / Parser-2 on `bismark:1152` = `3.1.0` | local image (this audit) | PASS |
| `bismark.bin --version` on `bismark:1152` = `… v3.1.0 (linux/aarch64 — built …)` | local image (this audit) | PASS |
| `bismark -v` / parsers / binary on `bismark:1152-beta` = `3.2.0-beta.1` | local image (this audit) | PASS |
| `bismark --help` exit 0 on both images | local image (this audit) | PASS |
| Guard arms (a)–(d), prerelease stub, real binary | PLAN.md notes (harness) | PASS (from notes) |
| release.yml `p1`/`p2` with `EXPECT=3.2.0-beta.1` | PLAN.md notes | PASS (from notes) |

No Rust/unit tests were in scope (Outline 4).

## Verdict

COMPLETE. Every Behavior (1–6), Implementation outline step (1–4), Validation (1–5) and resolved question (Q1, Q2) is implemented. The one deviation (validation images built without the micromamba layer) is documented in PLAN.md with its reason. Commit and PR are not yet expected.
