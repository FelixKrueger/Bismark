# Phase D Code Review — `--merge_CpGs` (+ `--discordance_filter`) — Reviewer B

**Verdict: APPROVE** (zero Critical, zero Important; 1 Minor, 4 Nits — all doc/test-coverage/theoretical, none byte-diverging).

Phase D (`merge.rs` + `report.rs` cov-path helpers + `lib.rs` wiring + `error.rs`
variant + `golden_phase_d.rs` + `tests/data/phase_d/`) is, in my independent
judgement, **byte-identical to Perl `coverage2cytosine` v0.25.1** for the
`--merge_CpGs` post-pass across every adversarial input I built and ran. I did
NOT just reason about it — I diffed Rust vs **live Perl** (the v0.25.1 script at
the worktree root) on 14 hand-built fixtures, including all four "where prior
Criticals lived" classes from the brief (discordance rounding boundary,
EOF-mid-resync partial-file, chr-start multi-scaffold resync, filename
derivation). All matched byte-for-byte.

92 tests pass (62 unit + 11 phase-B + 7 phase-C + 7 phase-D + 5 sanity);
`clippy --all-targets -D warnings` clean; workspace builds; `git status` shows
only the c2c crate + plans (siblings untouched). Claims verified.

---

## What I ran (Perl vs Rust byte-diffs, all in `$TMPDIR`)

Every case below ran BOTH `./coverage2cytosine` (Perl v0.25.1) and
`target/debug/coverage2cytosine_rs` with `--merge_CpGs`, then `cmp`'d the
merged/discordant cov outputs (gz decompressed first, per SPEC §15).

| # | Adversarial input | Result |
|---|-------------------|--------|
| 1 | Bundled `merge/` (phase_b genome+cov) | **MATCH** (`chr1 2 3 50.495050 408 400`) |
| 2 | `--gzip` merge (decompressed) | **MATCH** |
| 3 | `--zero_based` merge half-open | **MATCH** (`chr1 1 3`) |
| 4 | Discordance **gross** Δ80, N=20 | **MATCH** (merged empty; discordant 2 rows) |
| 5 | Discordance **boundary** `11/9`(55%) vs `27/23`(54%), N=1 → rounded Δ=1.0 **not** >1 | **MATCH** (merged, not diverted) — the rounding trap, *independent* of the committed V12 fixture |
| 6 | Discordance **reverse boundary** `2/1`(66.666667) vs `37/23`(61.666667), N=5 → rounded subtraction reintroduces f64 error → `5.000000000000007 > 5` | **MATCH** (BOTH divert) — confirms `round6` parse-back matches Perl's numify-then-subtract exactly |
| 7 | Both-measured **gate**: one strand `0,0` + Δ100, N=1 | **MATCH** (pooled, not diverted) |
| 8 | **3 consecutive** lone-orphan `CGT` scaffolds → real pair | **MATCH** (slide consumes all 3 + extra advance) |
| 9 | **Interspersed** orphan/real-pair scaffolds (sA orphan, sB pair, sC orphan, sD pair) | **MATCH** |
| 10 | Slide lands on a **non-start** real pair (no extra advance) | **MATCH** |
| 11 | `--zero_based` resync (3 orphans) | **MATCH** (`sD 5 7`) |
| 12 | **Odd-row** trailing single orphan (clean `last`, no die) | **MATCH** |
| 13 | **EOF-mid-resync** slide exhausts file → Perl die(255) / Rust err(1) | **MATCH** (partial merged == Perl's pre-die output) |
| 14 | **Multi-chromosome** multi-CpG (chr1→chr2 transition, skip-zero pairs) | **MATCH** |
| 15 | Discordance **N=100** (Δ=100 not >100 → merged) and N=1 (diverted) | **MATCH** |
| 16 | `%.6f` formatting parity Perl `sprintf` vs Rust `{:.6}` over 22,650 `m/u` pairs | **identical, 0 divergence** |

The two historically-fragile rev-1 Criticals are both correctly handled and I
confirmed them against live Perl from scratch:
- **Rounded discordance (§3.5):** `round6(m,u)` = `format!("{:.6}").parse::<f64>()`
  reproduces Perl's numify-of-`sprintf("%.6f")` *including* the subtraction error
  it reintroduces (case #6 above — Perl `5.000000000000007 > 5` ⇒ divert, and
  Rust diverts too). A raw-f64 compare would diverge in BOTH directions; the impl
  rounds first. Correct.
- **EOF-mid-resync (§3.4):** `next_row()→Option`; a `None` reached mid-resync
  flows into the `let (Some,Some) = … else { return Err(MergeCpgSanityViolation) }`
  — no `panic!`/`unwrap` on the EOF path. Merged lines are **stream-written**
  (`write_cov_line` per pair, single `ReportWriter` opened up-front), so the
  on-disk partial matches Perl's pre-die file (cases #13). No cleanup on error
  (c2c has no cleanup helper; correct per §5).

---

## Findings

### Critical
None.

### Important
None.

### Minor

**M1 — V14 (multi-chromosome merged golden) has no committed fixture.**
`merge.rs` / `golden_phase_d.rs` — PLAN §9 V14 and IMPL T5 both list a `multi/`
multi-chromosome fixture, and the IMPL Task-4 generate-block enumerates a
`multi/` dir, but `tests/data/phase_d/` ships no `multi.*` fixture and
`golden_phase_d.rs` has no multi-chromosome test (7 tests, not 8). The behaviour
*is* correct — I built a 2-chromosome multi-CpG case (`chr1`→`chr2` with
skip-zero pairs) and it was byte-identical to live Perl (run #14) — but the
committed golden matrix does not pin the chromosome-transition path inside the
merge loop. The bundled fixtures are all single-chromosome except `resync`/`eof`
(which use scaffold transitions but only one *covered* real pair). Recommend
adding the multi-chromosome golden the plan promised, since chr transitions in
the pair loop are exactly where a desync regression would hide.
*Suggested fix:* add `multi.cov` + `multi.merged.golden` from Perl v0.25.1 and a
`merge_multi_chromosome_matches_golden` test (I have a working fixture in
`$TMPDIR/c2c_rev_b/multi*` if useful).

### Nits

**N1 — `parse_report_row` field-count threshold doc mismatch.**
`merge.rs:64` uses `if f.len() < 6` (need ≥6), but IMPL T3 says "need ≥7 fields".
The code is **correct** (Perl unpacks 6 vars; `context1` = field index 5; the
trinucleotide at index 6 is unused for merge), so `<6` faithfully matches Perl —
a 6-field line still yields a valid context. Only the IMPL note is stale. No
action needed beyond optionally fixing the IMPL text.

**N2 — strand field compares only the first byte.**
`merge.rs:70-72` stores `strand = *f[2].first()…` (single byte), then
`sanity_check` tests `r1.strand != b'+'`. Perl tests `$strand1 eq '+'` on the
whole field, so a corrupt strand like `++` would `die` in Perl but PASS in Rust.
This cannot occur on a real report (strand is always exactly `+`/`-`), so it is
a benign divergence on corrupt-input only. Same class as the pre-existing
accepted `MalformedCovLine` divergence. Leave as-is or note in SPEC §4's
divergence list.

**N3 — `next_row` skips blank lines; Perl does not.**
`merge.rs:96` loops past blank lines (`parse_report_row` → `None`), whereas
Perl's `<IN>` would feed a blank line straight into the asserts and `die`. The
report writer (`report.rs:219`) never emits blank lines and the file ends with a
single `\n` (no trailing blank record — confirmed `read_until` returns 0 at EOF,
giving `None`, matching Perl's undef). So this only differs on corrupt input,
never on a real report. Benign; consistent with the cov-parser's blank-skip.

**N4 — `u32` sums can overflow on absurd (non-real) input.**
`merge.rs:156-157,174-175,216` compute `r.m + r.u`, `pooled_m = r1.m + r2.m`,
`r1.pos + 1`, `r2.pos + 1` on `u32`. Perl is arbitrary-precision. On a real
report these are tiny (coverage counts, chromosome positions ≪ `u32::MAX`), and
`parse_u32` already caps each field at `u32::MAX`, so a sum could only overflow
on a hand-crafted report with two near-`u32::MAX` fields — impossible on real
data. Debug builds would panic (overflow check), release would wrap. This is the
same posture as the SPEC's resolved `u32` decision (§15) and the Phase-B cov
parser; no change required, but a `checked_add` would make the failure explicit
rather than a release-mode wrap if you want belt-and-braces.

---

## Areas I specifically verified clean (the brief's focus list)

- **Discordance rounding (§3.5):** built independent boundary fixtures in BOTH
  directions (raw-says-divert/rounded-says-merge, and rounded-subtraction-error
  cases) — Rust matches Perl exactly (runs #5, #6, #15). `round6` is right.
- **EOF-mid-resync (§3.4):** slide-exhausts-file → Rust `MergeCpgSanityViolation`
  (exit 1, no panic); partial merged file byte-identical to Perl's pre-die output
  (runs #13, plus bundled `eof/`). Stream-write confirmed.
- **Chr-start resync (§3.3):** same-chr advance, `chr1≠chr2` slide-until-match
  over 1/2/3 consecutive short scaffolds, the post-slide extra advance, slide
  landing on a non-start pair, and the `--zero_based` variant — all MATCH
  (runs #8–#12). This is the #98/#229 path; faithful.
- **Filename derivation (§3.7):** `cov_evidence_name` strips `.gz` then `.txt`
  (each at most once) from the **report** basename + suffix (+`.gz`); produces
  `m.CpG_report.merged_CpG_evidence.cov[.gz]` / `…discordant…`, exactly matching
  Perl's filenames on disk in every run. Unit-tested + verified on-disk.
- **Coordinates & pooling:** `pos1/pos2` (1-based) vs `pos1/pos2+1` (`--zero_based`
  half-open) for merged; `pos/pos` vs `pos/pos+1` for discordant; `m1+m2`/`u1+u2`;
  skip-zero (`pooled_m+pooled_u==0 → continue`); `%.6f`. All MATCH (runs #1,#3,#4,#7).
- **Both-measured gate:** either strand `0,0` ⇒ fall through to pooling (run #7).
- **Streaming/memory:** single `read_until` window via `next_row`, no full-row
  buffer; resync read-ahead uncapped (bounded only by EOF, as §6 requires).
- **`lib::run` ordering:** `run_report(...)?` calls `report_w.finish()` and drops
  the writer (closing the file) before returning; merge re-opens the closed file
  via `report_path(config, None)` + gz-aware `cov::open_cov`. Merge runs strictly
  after the report is fully written and closed. Correct.
- **gz empty-file edge:** an all-diverted run yields an empty merged `.gz` —
  `GzEncoder::finish()` on a zero-write encoder produces a valid empty-gzip
  stream that decompresses to 0 bytes, matching Perl's `gzip -c` (raw bytes
  differ only in the mtime header, per the SPEC's decompress-compare policy).

---

## Bottom line

The implementation faithfully reproduces the most error-prone Perl routine in the
crate (`combine_CpGs_to_single_CG_entity`), including its three historical
foot-guns (rounded discordance, EOF-mid-resync die-with-partial-file, and the
chr-start multi-scaffold resync). I could not produce a single byte-diverging
input. The findings are all doc/test-coverage/theoretical. **APPROVE.** The one
thing I'd actually do before tag is add the multi-chromosome golden the plan
already promised (M1) — not because it's broken (it isn't), but because it's the
one resync-adjacent path the committed matrix doesn't pin.
