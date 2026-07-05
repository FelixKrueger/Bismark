# Phase 2 (`--drach`/`--m6A`) — Code Review B

**Reviewer:** Code Reviewer B (independent; fresh context, no shared state with Reviewer A)
**Date:** 2026-05-31
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` (branch `rust/c2c-v1x`, uncommitted working tree)
**Scope:** the just-completed `--drach`/`--m6A` DRACH-motif m6A filtering port (Perl `coverage2cytosine` v0.25.1 `generate_DRACH_report:1075-1383`).

---

## Top-line verdict: **APPROVE** — 0 Critical, 0 High.

The implementation is **byte-identical to Perl v0.25.1** on every fixture I built from scratch (16 distinct fixtures across 2 genomes, 14 mode runs against live Perl). All 155 crate tests pass, `clippy --all-targets -D warnings` is clean, `cargo fmt --check` is clean. No panic on any edge motif. The DRACH filter arithmetic, both-strand position anchors, standalone early-exit, flag interactions, filename derivation, ordering, empty-cov, and gzip parity all match Perl exactly. I found **no correctness defect**. The only findings are one Low (a shared cov-line byte-builder duplicated a 4th time — house-style, acceptable) and a few Informational notes.

---

## Tooling gate (re-run, not trusted from the plan)

| Gate | Result |
|------|--------|
| `cargo test -p bismark-coverage2cytosine` | **155 green** (92 lib + 18 P1 + 12 **P2** + 11 B + 7 C + 10 D + 5 sanity), 0 failed |
| `cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings` | clean (no warnings) |
| `cargo fmt -p bismark-coverage2cytosine -- --check` | clean (exit 0) |

---

## Live-Perl byte-identity checks I ran (my own from-scratch fixtures)

Every run diffed `target/debug/coverage2cytosine_rs` vs `perl ./coverage2cytosine` (v0.25.1) on fixtures I generated myself (NOT the committed goldens). All Perl `--drach` runs incur the 20 s `generate_DRACH_report` sleep (STDERR banner — exempt); I batched them in the background.

### Round 1 — genome `chrTOP/chrSTART/chrEND/chrEND2/chrBOT4/chrMIX/chrLOWER`, 8 modes

| Fixture / mode | Result |
|---|---|
| **default** (top + bottom interior, `chrTOP`) | **byte-identical** (report + cov) |
| **`--drach --CX`** (standalone early-exit; CX ignored) | byte-identical — no normal `.CX_report.txt` written; DRACH files == default |
| **`--drach --zero_based`** | byte-identical to default — zero_based ignored (always 1-based) |
| **`--drach --coverage_threshold 5`** | byte-identical |
| **`--drach --merge_CpGs`** (accepted, ignored) | byte-identical — merge silently dropped (early exit) |
| **empty cov** | **two 0-byte files** in both (Perl emits the `uninitialized value $chromosomes{""}` STDERR warning — exempt; exit 0; no panic) |
| **suffixed `-o foo.CpG_report.txt`** | byte-identical — `foo.CpG_report.txt_DRACH_report.txt` (**no suffix strip**) |
| **`--split_by_chromosome`** (7 chromosomes) | all 14 files byte-identical; `.chrchrTOP_` etc. (`.chr`+name doubling) confirmed; per-chr files emitted even for chromosomes with **no** DRACH hits (chrBOT4/chrMIX) — matches Perl |

### Round 2 — genome `chrMIX/chrN/chr2/chr1`, discriminating cases

| Fixture / mode | Result |
|---|---|
| **dup cov position** (`chrMIX 15` written twice: `2,2` then `7,3`) | **last-write-wins** (`7 3` emitted) — byte-identical |
| **non-sorted single-file ordering** (`chr2` lines before `chr1`) | covered = **cov-appearance order** (chr2 block before chr1 block); within chr all `+` then all `-` — byte-identical |
| **`--gzip`** | decompressed Rust == decompressed Perl AND == Rust plain — byte-identical |
| **`--coverage_threshold 3`** (discriminating: `chr2@7` cov=1 dropped) | byte-identical |
| **`--split_by_chromosome` on empty cov** | **no files** produced in either (no chromosome → no writer) |
| **non-ACGT emitting** (`chrN=NAACATTTGAACNTTT`, cov@4 & @12) | **byte-identical**: `chrN 4 + 3 1 NAACA CAT` (D=`N`≠C **passes**) + `chrN 12 + 5 5 GAACN CNT` (H=`N`≠G **passes**, non-ACGT in tri column too) — confirms the literal `!= b'C'`/`!= b'G'` byte-tests and that non-ACGT bytes flow through unchanged |

### Round 3 — bottom-strand position cross-check (the plan's headline)

| Fixture / mode | Result |
|---|---|
| **interior bottom DRACH** (`chrBOTI=TTTAACGTTAGTTT`, GT→C@11, `drach=AAACT`, `tri=CTA`) `--drach` | byte-identical: `chrBOTI 11 - 6 2 AAACT CTA` |
| **same fixture `--CX`** (independent cytosine report) | the `--CX` report **independently** places a `-`-strand C at **position 11** with trinucleotide **`CTA`** (CHH) — confirms `pos-1` is the genuine BS-seq cytosine coordinate (validates §3.6 derivation + Felix's Q1 resolution) |
| **cov chr absent from genome** (`chrGHOST` in cov, not in FASTA) | emits nothing for chrGHOST, no panic, in-genome `chrBOTI` unaffected — byte-identical (mirrors Perl's empty `while`-walk over an undef `$chromosomes{chrGHOST}`) |

### Round 4 — top-strand filter-arm near-misses (skip parity)

Genome `chrD0=CCACA…` (pos-0 `C`), `chrR=GTACA…` (pos-1 `T`∉{A,G}), `chrH=GAACG…` (pos-4 `G`), `chrPASS=GAACA…`; all cov@4.
- **byte-identical**: only `chrPASS 4 + 5 5 GAACA CAT` emitted; all three near-misses skip in BOTH Perl and Rust (Rust does not emit where Perl skips).

### Edge cases exercised within the above (no separate run needed)
- **Top-strand `pos<4` wrap EMITS** (`chrSTART=ACAAA` cov@2 → `chrA 2 + 9 1 AA CAA`, `drach=substr(-2,5)="AA"`) — round-1 default, byte-identical, no panic.
- **Bottom-strand `pos<4` dropped** (`chrBOT4` GT@idx0 → bottom `tri="AC"` len 2 → len-guard-skipped) — round-1, no emit, no panic.
- **Bottom truncated-5-mer EMITS** (`chrEND=AAAGTA` cov@4 → `chrEND 4 - 5 0 TACT CTT`; `chrEND2=AAAGTC` → `… GACT CTT`) — round-1, byte-identical (pos-5-missing → pass).
- **`--m6A` alias end-to-end**: produces DRACH files identical to `--drach` (not just a parse — actually triggers the path).

---

## Findings by area

### Logic / arithmetic — **all correct**

- **DRACH filter** (`is_drach_motif`, `drach.rs:242-247`): `five_mer.first().is_none_or(|&b| b != b'C')` (D), `matches!(get(1), Some(&b) if b==A||b==G)` (R), `get(4).is_none_or(|&b| b != b'G')` (H). This is an *exact* model of Perl's `substr($drach,N,1) ne/eq` on a `substr` that returns `''` past the end:
  - pos-0 missing → `is_none_or` true → **pass** (Perl `'' ne 'C'`).
  - pos-1 missing → `matches!` false → **fail** (Perl `'' eq 'A'||'' eq 'G'` false).
  - pos-4 missing → `is_none_or` true → **pass** (Perl `'' ne 'G'`).
  Verified live on `NAACA`/`GAACN`/`AA`(2-byte wrap)/`TACT`(4-byte trunc). **Correct.**
- **`perl_substr` offset bound (B-Important-1) — independently re-derived and confirmed unreachable.** I read `report.rs:99-111`: the helper clamps negative `start` via `saturating_sub` and keeps `want` — the documented divergence from Perl is **only at `offset < -len`** (Perl shrinks `want` by the overshoot). DRACH's most-negative offsets are: top `drach` = `pos-4` with `pos=i+2≥2` → `≥ -2`; bottom `tri` = `pos-4` → `≥ -2`. Any chromosome with an `AC`/`GT` has `len ≥ 2`, so every DRACH offset is `≥ -2 ≥ -len`. **The helper's `offset<-len` bug is provably unreachable from DRACH.** No `perl_substr` change needed (correctly left out of scope).
- **Top strand** (`drach_top`): `pos=i+2`, both `tri` (offset `pos-1`) and `drach` (offset `pos-4`) via `perl_substr` (the A-F1 Critical mandate) → the `pos<4` wrap emits (live-verified `ACAAA`→`AA CAA`). cov lookup at `pos`. Threshold `meth+nonmeth >= threshold`. `i += 1` advance (non-self-overlapping `AC` → identical match set to Perl `/(AC)/g`). **Correct.**
- **Bottom strand** (`drach_bottom`): `pos=i+2`, `tri=revcomp(perl_substr(seq, pos-4, 3))`, `drach=revcomp(perl_substr(seq, pos-3, 5))`, cov lookup + report at `key=pos-1`. Matches Perl `:1306-1379` (the `tr/ACTG/TGAC/` + `reverse` = `revcomp`, with non-ACGT pass-through). Position `pos-1` cross-validated against `--CX`. **Correct.**
- **Threshold auto-set** (`cli.rs:213`): `None if nome || self.drach => 1` — placed *after* the explicit-0 rejection (so `--coverage_threshold 0` still errors `ThresholdNotPositive`), explicit value survives. Matches Perl `:2188-2194`. **Correct** (unit-tested + live-verified default→1, explicit-5-survives, explicit-0-rejected).
- **General mutex preservation (A-F2):** I confirmed `cli.rs` did NOT add a `--drach` short-circuit; the un-rejection only removes the `--drach` arm from `UnsupportedFlag`. `--drach --merge_CpGs --coverage_threshold 5` still errors `MergeCpgsWithThreshold` (unit `drach_has_no_dedicated_mutex_but_general_mutexes_still_fire`). **Correct.**
- **Early-exit** (`lib.rs:62-64`): `if config.drach { return drach::run_drach(config, &genome); }` placed after genome load + the `Stored sequence information…` eprintln, before `report::run_report`. Mirrors Perl `:38-42`. **Correct.**
- **Empty-cov final-flush guard (A-F3/B-2):** `run_drach_single` opens both writers *before* the loop; the final flush is `if let Some(prev) = cur_chr.take()` — a zero-line cov never sets `cur_chr`, so the phantom `""`-chromosome walk is correctly skipped (no `genome.get("")`, no `unwrap` of a never-set chr). Two 0-byte files. Live-verified == Perl. **Correct.**

### Structure / naming / duplication

- **[Low] 4th copy of the cov-line byte-builder.** `push_drach_cov` (`drach.rs:251-264`) is byte-for-byte identical to `gpc.rs:push_gpc_cov` (and the merge/report-NOMe equivalents) — same 6-field `chr\tstart\tend\tpct\tmeth\tnonmeth\n`. `push_drach_report` differs from `push_gpc_report` only in fields 6–7 (`drach_5mer`/`tri` `&[u8]` vs `context`/`tri`). **Judgment:** this duplication is *acceptable house style* for this crate — each module is a self-contained Perl-sub port kept deliberately independent so a byte-divergence in one cannot silently propagate via a shared helper, and the builders are trivial (no logic). A shared `push_cov_line(out, chr, start, end, pct, m, u)` in `report.rs` would remove ~14 duplicated lines × 4 sites with zero behavioral risk and is worth doing as a **future, non-blocking** crate-wide tidy (not Phase 2). I do **not** recommend changing it in this PR — it would touch 4 modules for a cosmetic win and risk the byte-identity goldens for no functional gain.
- Module structure cleanly mirrors `gpc.rs` (the designated structural twin): `run_*` → single/split drivers → `*_chromosome_bytes` → `drach_top`/`drach_bottom` → filename helpers. Naming is clear and consistent. Doc comments are thorough and accurate (I cross-checked the `ACAAA`→`AA CAA` example in the module header against live Perl — correct).
- `#[allow(clippy::too_many_arguments)]` on `push_drach_report` (8 args) — same pattern as `gpc.rs`, acceptable.

### Tests

- 8 new `drach.rs` unit tests + 4 new `cli.rs` tests + 12 `golden_phase2.rs` byte-identity tests (V1–V16 mapped 1:1 to named test fns). The `generate_goldens.sh` is self-documenting with full provenance and the 20 s-sleep warning. The unit-test kernel anchors (`top_strand_chromosome_start_wrap_emits`, `bottom_strand_truncated_5mer_emits_at_pos_minus_1`, `is_drach_motif_short_slices_no_panic`) match the Perl values I independently reproduced.
- The 4 modified test files (`gpc.rs`, `report.rs` test cfgs gained `drach: false`; `sanity.rs` probe flipped `--drach`→`--ffs`) are correct, minimal, and don't regress (all green).

---

## Informational notes (not findings)

1. **`gpc.rs` is in the diff** but the only change is one line (`drach: false`) in its test `cfg()` helper — a mechanical `ResolvedConfig` field addition, not a behavioral change. Same for `report.rs`. Expected.
2. **Strict cov parsing vs Perl's lenient `split`.** DRACH reuses the shared `cov::parse_cov_line` (strict `u32`, CRLF-strip, blank-skip → `MalformedCovLine`), whereas Perl's DRACH `split /\t/` would coerce/accept garbage. This is the **pre-existing crate-wide Phase-B policy** (documented in `cov.rs`, B-I1/2/3), shared by the main report and gpc — not a Phase-2 regression, and cannot occur on real `bismark2bedGraph` output. Out of scope; noted for completeness.
3. **`is_drach_motif` uses `Option::is_none_or`** (stabilized Rust 1.82) — clippy/build is happy on this toolchain; no MSRV concern flagged in the crate.
4. **`String::from_utf8_lossy` on chromosome names** in `drach_base` (filename infix) — consistent with how the rest of the crate derives split filenames; non-UTF8 chr names are not a realistic concern and Perl would have its own behavior. Matches the established pattern.

---

## Byte-identity claims re-verified (summary)

- ✅ DRACH filter `D!=C / R∈{A,G} / H!=G` incl. non-ACGT pos-1 pass, pos-2 fail (via match), pos-5 pass, and the literal byte-tests (live `NAACA`/`GAACN`/filter-arm genome).
- ✅ Truncated <5-mer "pos-5 missing → pass AND emit" (bottom `AAAGTA`→`TACT CTT`).
- ✅ Both strands: top `AC`@pos, bottom `GT`→`pos-1`; `--drach` vs `--CX` agree on bottom position **11** + trinucleotide **CTA**.
- ✅ Top `pos<4` wrap EMITS (`ACAAA`→`AA CAA`), no panic; bottom `pos<4` dropped (tri len<3), no panic.
- ✅ Standalone early-exit; `--drach --CX`/`--merge_CpGs` accepted+ignored; `--drach --merge_CpGs --coverage_threshold 5` STILL errors (general mutex preserved).
- ✅ Default threshold→1, explicit 5 survives, explicit 0 rejected; `--zero_based` ignored.
- ✅ Raw-`-o` no-strip (suffixed `-o`), `.chrchr1` doubling, no header.
- ✅ Top-then-bottom ordering; covered-only; cov-appearance order (single-file non-sorted 2-chr); last-write-wins on dup positions; empty cov → two 0-byte files (exit 0); split-empty → no files; cov chr absent from genome → no emit, no panic.
- ✅ `%.6f` pct parity (cov col-4 ignored, recomputed); `perl_substr` offset never `< -len` (B-Important-1 unreachable).
- ✅ `--gzip` decompresses to plain golden; `--m6A` alias end-to-end.

**No divergence, off-by-one, panic, or no-mutex regression found.**
