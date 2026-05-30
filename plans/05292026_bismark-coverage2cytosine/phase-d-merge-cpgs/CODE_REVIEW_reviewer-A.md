# Phase D Code Review — Reviewer A

**Target:** `--merge_CpGs` (+ `--discordance_filter`) post-pass in `bismark-coverage2cytosine`
**Scope reviewed:** `src/merge.rs` (new), `tests/golden_phase_d.rs` (new), `tests/data/phase_d/` (new), and the `git diff HEAD` deltas to `src/report.rs`, `src/lib.rs`, `src/error.rs`, `src/cli.rs`.
**Method:** read code in full; built `coverage2cytosine_rs` (debug); ran live Perl v0.25.1 (`./coverage2cytosine`, perl 5.34.1, macOS) against the Rust binary on **20+ self-constructed adversarial fixtures** in `$TMPDIR`, byte-diffing the merged/discordant cov files; ran the committed test suite (92 green) + clippy `-D warnings` (clean).

---

## VERDICT: APPROVE

The Phase D implementation is **byte-identical to Perl v0.25.1** on every adversarial case I constructed, including all four "where prior Criticals lived" focus areas (discordance rounding, EOF-mid-resync, chr-start resync, filename derivation). The algorithm is a faithful port; the error path returns a typed error and never panics; the streaming-write contract holds the partial file byte-identical to Perl's pre-die output. No Critical or Important defects found.

**Findings: 0 Critical · 0 Important · 2 Minor · 2 Nit.**

---

## What I verified against LIVE Perl (all byte-identical Rust≡Perl)

Every diff below was `diff -q` clean (or, for gzip, decompress-then-diff clean):

| Area | Fixtures (mine, beyond the committed goldens) | Result |
|------|-----------------------------------------------|--------|
| **Discordance rounding (focus #1)** | own `1/1`(50%) vs `11/9`(55.0000…007%), N=5 → **MERGED** not diverted; plus a 24-fraction `sprintf %.6f` vs Rust `format!("{:.6}")` sweep incl. round-half cases (`5/8`,`7/8`,`1/16`, large denominators) | identical |
| **EOF-mid-resync (focus #2)** | committed `eof` (2 trailing orphans); `eof2` (3 trailing orphans → slide runs out without ever matching); `eof3` (orphan-orphan-realpair then 2 uncovered orphans → real pair merged, then die); `empty` (first pair slides to EOF → 0-byte partial); `eof_z` (zero-based); `eofgz` (`--gzip`) | exit 255(Perl)/1(Rust); **partial merged file byte-identical** in every case incl. the 0-byte case |
| **Chr-start resync (focus #3)** | `caseA` (single orphan slide + extra-advance); `caseB` (same-chr single-advance); `caseB_z`; `resync_z` (zero-based slide); `sl` (slide breaks with pos1≥2 → no extra advance); `rd`/`rdisc` (resync slide + extra-advance landing on a discordant pair) | identical |
| **Filename derivation (focus #4)** | `-o foo`, `-o foo.CpG_report.txt` (the suffixed extractor path), `-o foo.gz`, `-o foo.txt`, `-o sample.bismark.cov`, all × `{--gzip, --discordance_filter}` | merged/discordant filenames identical to Perl |
| Coordinates | zero-based half-open (`pos2+1` merged, `pos+1` discordant) across `merge_zero`, `bigdz`, `resync_z` | identical |
| Pooling / skip-zero / both-measured gate | `gate` (one strand 0,0 + huge Δ → pooled not discordant); `big`/`bigdz` (7-CpG, 3-chromosome, soft-masked + N-context genome, mixed coverage) | identical |
| gz re-read | `gzread` (merge re-reads a `--gzip` `.CpG_report.txt.gz`) | decompressed-identical |

The chr-start resync — flagged as "the highest-risk port" — reproduces Perl's `:1843-1883` exactly, including the consecutive-short-scaffold slide (`chr1≠chr2` read-until-match), the extra advance when still `pos1<thr` after the slide, and the same-chr single advance. Both the default (`pos1<2`) and `--zero_based` (`pos1<1`) thresholds are correct (`thr = if zero {1} else {2}`).

---

## Notable correct-but-subtle behaviors (called out so they aren't "fixed" later)

1. **Partial gzip on the error path is valid and matches Perl after decompression.** On EOF-die with `--gzip`, `run_merge` returns the error *before* `merged_w.finish()`, yet the partial `.gz` still decompresses correctly. This is because `GzEncoder`'s `Drop` writes the gzip trailer when `merged_w` falls out of scope. I diffed the raw bytes (`xxd`): Perl and Rust differ only in the gzip **mtime/OS header bytes** (Perl stamps a timestamp; Rust zeros them) — exactly the impl-dependent container the SPEC §15/P10 excludes from the gate. Decompressed bytes are identical. **Correct.** (Do not "fix" by adding an explicit finish-on-error — that would change nothing observable and risks the partial-file contract.)
2. **`BufWriter` Drop flushes the plain partial on the error path** — verified the plain EOF-die partial is byte-identical to Perl. (Drop's flush silently ignores errors, but that only matters under disk-full, which Perl's `print` doesn't check either.)
3. **Slide-loop `is_none_or(|x| x.pos < thr)` (merge.rs:132)**: after the slide loop, `o1` is provably always `Some` (it is set from `o2.take()` where `o2` was `Some` each iteration), so the `None` arm is dead. Harmless defensive code; matches Perl's unconditional `if ($pos1 < 2)` re-check. Not a defect.
4. **`pct6` "caller guarantees m+u>0"** is upheld on every call site: discordant writes are behind the both-measured gate (`r1.m+r1.u>0 && r2.m+r2.u>0`); the merged write is behind `pooled_m+pooled_u==0 → continue`. No division-by-zero path.

---

## Minor

### M1 — `generate_goldens.sh` was not extended for Phase D (provenance gap)
**File:** `tests/data/phase_b/generate_goldens.sh` (unmodified) · IMPL Task 4.
The IMPL prescribes appending a `phase_d` golden-generation block to `phase_b/generate_goldens.sh` so the `merge/`, `disc_*`, `resync/`, `eof/`, `multi/` goldens are reproducible from a committed script. The script was **not** touched; the phase_d fixtures + goldens were generated ad-hoc. This is a **maintainability/reproducibility** gap, not a correctness one — I independently confirmed every committed golden matches live Perl v0.25.1, so the goldens themselves are genuine Perl output. **Suggested fix:** append the documented phase_d block (or add a `phase_d/generate.sh`) before the Phase E real-data gate, so the goldens can be regenerated when Perl is bumped.

### M2 — u32 pool/position arithmetic can wrap silently in release builds
**File:** `merge.rs:174-175` (`pooled_m = r1.m + r2.m`, `pooled_u = r1.u + r2.u`), `:165-166`/`:179` (`pos + 1`), `sanity_check:216` (`r1.pos + 1`).
`[profile.release]` in `rust/Cargo.toml` does **not** set `overflow-checks`, so these `u32` adds wrap (no panic) on overflow. This is **not reachable on real `bismark2bedGraph` output** — a single position would need >4.3 billion reads, and positions max ~2.5e8 (SPEC §15) — and the SPEC explicitly accepts `u32`. I flag it only because Perl would not wrap (it'd carry the value into a float), so a pathological hand-crafted cov *could* diverge. Consistent with the existing `cov.rs`/Phase-B `u32` decision; acceptable as-is. **Optional:** `checked_add` on the pool sums to surface an error instead of a silent wrap, matching the SPEC's "fail explicitly" posture. No change required for v1.0.

---

## Nit

### N1 — `parse_report_row` field-count guard is `< 6` while the report has 7 fields
**File:** `merge.rs:64`. The check requires ≥6 tab fields (it reads indices 0–5; the trinucleotide at index 6 is intentionally ignored). This is correct and mirrors `cov.rs::parse_cov_line`'s own `< 6`. A 6-field (trinucleotide-less) line would be accepted — but the report is Rust-written with 7 fields, so this never occurs. Purely a defensive-consistency note.

### N2 — duplicated `parse_u32` helper across `merge.rs` and `cov.rs`
**File:** `merge.rs:44-49` duplicates `cov.rs:67-72` verbatim. Trivial; not worth a shared-module churn for two call sites, but a candidate for a future `util` consolidation.

---

## Cross-checks (all pass)
- **Mutexes** (`--merge_CpGs` ⊥ `--CX`/`--split_by_chromosome`/`--coverage_threshold`; `--discordance_filter` requires merge + `1..=100`) are enforced in `cli.rs::validate` (Phase A) — so `merged_cov_path` hardcoding `cx=false` in `report_name(...)` is sound (CX can never co-occur). Verified by the existing `rejects_merge_with_*` unit tests.
- **No panics on the error path**: the only `unwrap`/`expect` in `run_merge` are `o2.as_ref().unwrap()` / `o1.as_ref().unwrap()` (both provably `Some` at those points) and `pct6().parse().expect()` (a `format!`-produced f64 always parses). The EOF/desync paths flow through `let (Some,Some) = … else { return Err(MergeCpgSanityViolation) }` and `sanity_check` → typed error. Confirmed no panic across all EOF fixtures (Rust exited 1 with a clean "sanity violation" message, never SIGABRT).
- **Streaming / O(1) memory**: `next_row()` reads one line at a time over a gz-aware `BufRead`; merged lines are stream-written via a single `ReportWriter`; no full-row `Vec`. The resync read-ahead is bounded only by EOF (not capped) — matches PLAN §6/I3.
- **Scope**: `git status` shows only the c2c crate (`src/{merge,report,lib,error}.rs`, `tests/golden_phase_d.rs`, `tests/data/phase_d/`) + the plans dir. `bismark-extractor` and `bismark-bedgraph` untouched; `tests/data/` only added-to, never modified.
- **Test suite**: 62 unit + 0 doctests + 11 phase-B + 7 phase-C + 7 phase-D + 5 sanity = **92 green**; clippy `-D warnings` clean.

---

*Reviewer A — recommend-only; no files other than this report were written. All Perl/Rust adversarial output was written to `$TMPDIR`, never into the repo tree.*
