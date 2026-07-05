# Phase D PLAN — `--merge_CpGs` (+ `--discordance_filter`)

**Epic:** `05292026_bismark-coverage2cytosine/EPIC.md`, Phase D — `--merge_CpGs` (+ `--discordance_filter`)
**Design contract:** `../SPEC.md` (rev 3) — §9 (merge_CpGs).
**Status:** rev 2 — implemented + green; dual code-review (APPROVE×2, zero Critical/Important) + plan-manager (COMPLETE) folded. See Implementation notes.

## Implementation notes (Phase D — 2026-05-30)

**Implemented + byte-identical to Perl v0.25.1.** New `merge.rs` (`run_merge`, `parse_report_row`, `round6`/`pct6`, the 2-row `while let` pair loop + chr-start resync, sanity asserts, discordance, pool, stream-write); `report.rs` promoted `ReportWriter`/`report_path`/`report_name` to `pub(crate)` + `merged_cov_name`/`discordant_cov_name`/`merged_cov_path`/`discordant_cov_path`; `error.rs` +`MergeCpgSanityViolation`; `lib::run` post-pass. **97 tests pass** (64 unit + 11 phase-B + 7 phase-C + 10 phase-D + 5 sanity) after the rev-2 fold; fmt + clippy `-D warnings` clean; workspace builds; siblings untouched.

**Golden validation** (`tests/golden_phase_d.rs` + `tests/data/phase_d/`, from repo Perl v0.25.1): merged cov (`50.495050`), `--gzip` (decompressed), `--zero_based` half-open (`chr1 1 3`), **discordance gross** (Δ80→discordant), **discordance BOUNDARY** (`1/1` vs `11/9`, N=5 → MERGED not diverted — the rounding trap), **resync slide** (two consecutive `CGT` lone-orphan scaffolds → recovers on the real pair), **EOF-mid-resync** (trailing orphans → exit 1 + partial merged file == Perl's pre-die output). All match live Perl.

**Iteration log:**
- #1: clippy `neg_cmp_op_on_partial_ord` (test `!((a-b).abs() > 5.0)` → `<= 5.0`) + `manual loop→while let` (the 2-row read loop). Both cosmetic.
- Empirically confirmed during golden-gen: my first "resync" fixture (trailing orphans) ALSO hit the EOF-die; fixed it to put the lone orphans BEFORE a covered real-pair scaffold (cov order sA,sB,sC) so the slide recovers — and kept a separate EOF fixture for V13.
- #2 (post-review fold, rev 2): closed the dual code-review + plan-manager **advisory** gaps (zero Critical/Important were found). Added dedicated tests **V2** (merged/discordant filename derivation, `report.rs`), **V6** (both-measured gate pools the unmeasured-partner pair, not divert), **V8a** (same-chr chr-start resync `else` branch — a `CG…`-start chromosome with a same-chr real pair forces Perl `:1875`), **V10** (each `sanity_check` desync arm → typed error, no panic), and the **V14** multi-chromosome merged golden (chr1→chr2 transition in the pair loop). Appended a Phase-D block to `tests/data/phase_b/generate_goldens.sh` so every phase_d golden is regenerable from repo Perl v0.25.1 — the regen reproduced all 8 pre-existing goldens byte-for-byte. Fixed the stale IMPL `≥7`-fields note (code uses `<6`; Perl binds 6 vars). **No behavioural change** — all 5 new tests passed first run against the existing implementation. 92→97 tests.

**No design deviations.** All §3.1–§3.7 + the 2 rev-1 Criticals (rounded discordance, EOF-die no-panic/partial-file/no-cleanup) + V1–V14 implemented.

## 1. Goal

Add the `--merge_CpGs` post-pass: after the genome-wide CpG report is written (Phase B/C), **re-read it** and pool each CpG dinucleotide's top (`+`) and bottom (`-`) strand evidence into a single entry, written to a `*.merged_CpG_evidence.cov[.gz]`. With `--discordance_filter N`, route strand-discordant CpGs (Δ% > N) to a separate `*.discordant_CpG_evidence.cov[.gz]` instead of merging them. **Byte-identical to Perl v0.25.1.**

## 2. Context

- **Where:** a new `merge.rs` in `rust/bismark-coverage2cytosine/src/`, invoked from `lib::run` **after** `report::run_report` when `config.merge_cpgs`. Reuses: `report::ReportWriter` (the merged/discordant cov writers; gz iff `--gzip`); `cov::open_cov`-style gz-aware reading to re-open the CpG report; `report`'s filename helpers (extended).
- **Depends on:** Phase B (the report it re-reads) + Phase C (`ReportWriter`, gz). **Phase A already enforces the mutexes** (`--merge_CpGs` rejects `--CX`, `--split_by_chromosome`, `--coverage_threshold`; `--discordance_filter` requires `--merge_CpGs` + value `1..=100`) — so merge **always** runs on the single, non-split, CpG-context report (plain or gz). No re-validation here.
- **Perl ground truth:** `combine_CpGs_to_single_CG_entity` (`:1753-1958`).
- **Internal contract (new):** Phase D's input is **Phase B/C's output** — the CpG report's exact line bytes (`chr\tpos\tstrand\tm\tu\tcontext\ttri\n`) are now an internal contract, not just an external one.

### Empirically observed (local Perl v0.25.1 — ground truth)
- **Merged cov** (`-o merge`, cov chr1 pos2=403/400 `+`, pos3=5/0 `-`): file `merge.CpG_report.merged_CpG_evidence.cov`, one line `chr1\t2\t3\t50.495050\t408\t400` (pooled m=408, u=400, `%.6f` of 408/808·100).
- **`--gzip`:** `merge.CpG_report.merged_CpG_evidence.cov.gz` (report `merge.CpG_report.txt.gz`); summary plain.
- **`--discordance_filter 20`** (pos2=9/1 → 90%, pos3=1/9 → 10%, |Δ|=80>20): merged file **empty**; `merge.CpG_report.discordant_CpG_evidence.cov` has `chr1\t2\t2\t90.000000\t9\t1` + `chr1\t3\t3\t10.000000\t1\t9`.
- **Filenames** derive from the **report** filename `$global_cyt_report` (NOT the stem): strip trailing `.gz` then `.txt`, append `.merged_CpG_evidence.cov` / `.discordant_CpG_evidence.cov` (+ `.gz` if `--gzip`). → `{output_stem}.CpG_report.merged_CpG_evidence.cov[.gz]`, `…discordant…`.

## 3. Behavior

### 3.1 When it runs
`lib::run`: `report::run_report(config, &genome)?;` then `if config.merge_cpgs { merge::run_merge(config)?; }`. (No genome needed — merge only re-reads the report.)

### 3.2 Re-read the CpG report
Open the just-written report file (the path `report::report_path(config, None)` — plain or `.gz`), gz-aware (mirror `cov::open_cov`'s `.gz` → `MultiGzDecoder`). Parse each line as **bytes** into a `ReportRow { chr: Vec<u8>, pos: u32, strand: u8, m: u32, u: u32, context: Vec<u8> }` (the trinucleotide field is ignored). The report is genome-ordered; every CpG appears as `(p,+)` then `(p+1,-)` consecutively, **except** a chromosome-start CpG (see §3.3).

### 3.3 Pairing + chromosome-start resync (Perl `:1794-1897` — the historical bug source #98/#229)
Mirror the Perl `while (1)` loop reading **two rows per iteration** (`line1`, `line2`); terminate when fewer than two rows remain (`last unless ($line1 and $line2)`).

**Chromosome-start resync** — a CpG at genome position 1 has a `+` row but **no `-` partner** (its `-` base, the G at `i=1`, was dropped by the `len<3` guard). Detect `pos1 < 2` (1-based) / `pos1 < 1` (`--zero_based`) and resync (Perl `:1843-1883`, default branch):
- **If `chr1 != chr2`** (line1 is a lone chr-start CpG on a short scaffold; line2 is the next chr's first row): slide forward (`line1 = line2; line2 = next_row()`) **until `chr1 == chr2`** (handles *consecutive* short single-CpG scaffolds, Perl `:1852-1865`). Then **if still `pos1 < 2`**, advance once more (`line1 = line2; line2 = next_row()`, Perl `:1866-1873`).
- **Else (`chr1 == chr2` but `pos1 < 2`)**: advance once (`line1 = line2; line2 = next_row()`, Perl `:1875-1881`) — skips the orphan `+` and re-pairs from the next row.

(The `--zero_based` branch `:1809-1842` is identical with the `pos1 < 1` threshold.)

### 3.4 Sanity asserts (Perl `:1886-1897` — `die` → typed error)
After resync, assert (else `BismarkC2cError::MergeCpgSanityViolation { detail }`): `context1 == "CG"`, `context2 == "CG"`, `strand1 == '+'`, `strand2 == '-'`, `pos2 == pos1 + 1`, `chr1 == chr2`.

**These CAN fire at EOF-mid-resync (rev 1 C1, reviewer A — corrects the rev-0 "never fire" claim).** A genome ending in ≥2 trailing lone chr-start CpG scaffolds (e.g. two `CGT` scaffolds, each a single `+`-only orphan) drives the resync's read-ahead to EOF; Perl then hits an `undef` row and **`die`s (exit 255)** at the `context2 eq 'CG'` assert — with the merged lines written *before* the die **left on disk**. Required Rust behavior (verified vs live Perl by both reviewers):
- `next_row()` returns `Option<ReportRow>`; a `None` reached mid-resync (or a final unpaired row) flows into these asserts and yields `MergeCpgSanityViolation` — **return the error, never `panic!`/`unwrap`**.
- **Do NOT clean up the partially-written merged cov on this error** (Perl leaves the partial file; c2c has no partial-cleanup helper anyway — see §5). Exit code 1 vs Perl's 255 is **exempt** (STDERR/exit not byte-gated); the on-disk file bytes must match, which §3.6's streaming write guarantees.

### 3.5 Discordance routing (Perl `:1902-1932`) — only with `--discordance_filter N`
Only when **both strands measured** (`m1+u1 > 0` AND `m2+u2 > 0`; if either is 0, fall through to normal pooling — discordance is unjudgeable).

**Rounded comparison (rev 1 C1, binding — both reviewers verified vs live Perl).** Perl computes `$top = sprintf("%.6f", m1/(m1+u1)*100)` and `$bottom = sprintf("%.6f", …)` and then compares `abs($top - $bottom) > $disco` — Perl **numifies the `%.6f` strings**, so the comparison is on the **6-dp-rounded** values, strictly `>`, against the **integer** `N`. The Rust MUST round first: `let top = round6(m1, u1); let bottom = round6(m2, u2); if (top - bottom).abs() > N as f64 { … }`, where `round6(m,u)` = parse `format!("{:.6}", m as f64/(m+u) as f64*100.0)` back to `f64` (or equivalent 6-dp round). A **raw-f64** compare diverges: verified boundary `1/1`(50%) vs `11/9`(55.000…007%), `N=5` → raw-f64 `|Δ| = 5.0000000000000071 > 5` ⇒ discordant, but **Perl rounds → `|50.000000 − 55.000000| = 5.0`, not `> 5` ⇒ merged** (live Perl: merged `chr1 2 3 54.545455 12 10`, discordant empty). This is a **byte-divergence in BOTH output files**, not a nicety. Tested by the boundary golden (V12).

If `abs(round6(top) - round6(bottom)) > N`: write **both** rows to the discordant cov and **`continue`** (skip merging this pair):
- 1-based: `chr1\t{pos1}\t{pos1}\t{top:.6}\t{m1}\t{u1}` + `chr2\t{pos2}\t{pos2}\t{bottom:.6}\t{m2}\t{u2}`.
- `--zero_based`: `chr1\t{pos1}\t{pos1+1}\t…` (half-open).

### 3.6 Pool + emit (Perl `:1934-1952`)
**Stream the merged lines** — write each pooled line to the merged-cov `ReportWriter` as it is produced (Perl writes incrementally), so that if a later pair trips the EOF-mid-resync sanity error (§3.4), the partial merged file on disk holds exactly the lines written before it — matching Perl's partial-file-then-die. Do NOT buffer all merged lines and write at the end.

`pooled_m = m1+m2`, `pooled_u = u1+u2`. **Skip if `pooled_m + pooled_u == 0`** (uncovered CpGs dropped). `pct = format!("{:.6}", pooled_m as f64/(pooled_m+pooled_u) as f64*100.0)`. Emit to the merged cov:
- 1-based: `chr1\t{pos1}\t{pos2}\t{pct}\t{pooled_m}\t{pooled_u}`.
- `--zero_based`: `chr1\t{pos1}\t{pos2+1}\t{pct}\t{pooled_m}\t{pooled_u}` (half-open).

### 3.7 Filenames (Perl `:1766-1790`)
From the **report filename** (basename of `report_path(config, None)`): strip trailing `.gz`, then `.txt`; append `.merged_CpG_evidence.cov` (and `.gz` if `--gzip`). Discordant: same base + `.discordant_CpG_evidence.cov[.gz]`. The merged file is always opened (may end empty if all pairs discordant/zero); the discordant file is opened only with `--discordance_filter`.

## 4. Signatures
```rust
// merge.rs
pub fn run_merge(config: &ResolvedConfig) -> Result<(), BismarkC2cError>;

struct ReportRow { chr: Vec<u8>, pos: u32, strand: u8, m: u32, u: u32, context: Vec<u8> }
fn parse_report_row(line: &[u8], line_no: usize) -> Result<Option<ReportRow>, BismarkC2cError>;

/// Round a percentage to 6 dp the way Perl's sprintf-then-compare does
/// (§3.5): parse `format!("{:.6}", m as f64/(m+u) as f64*100.0)` back to f64.
fn round6(m: u32, u: u32) -> f64;

// report.rs — VISIBILITY PROMOTION (rev 1 I1, reviewers A+B): `ReportWriter`
//   (+ `create`/`write_all`/`finish`), `report_path`, and `report_name` are
//   currently PRIVATE; promote the ones merge.rs needs to `pub(crate)`. Add
//   merged_cov_path(config) / discordant_cov_path(config): from the
//   report_path(config, None) basename strip `.gz` then `.txt`, append the
//   suffix (+ `.gz` if --gzip).
// error.rs: + MergeCpgSanityViolation { detail: String }
// NOTE: c2c has NO partial-output-cleanup helper — do not invent one; the
//   EOF-mid-resync error (§3.4) intentionally leaves the partial merged file.
```

## 5. Implementation outline (TDD-friendly)
1. **`parse_report_row`** (bytes → `ReportRow`); unit tests incl. a `+`/`-` row, field parsing.
2. **Filename derivation** `merged_cov_path`/`discordant_cov_path` (strip `.gz`/`.txt` from the report basename + suffix + gz); unit tests incl. the `.CpG_report.merged_CpG_evidence.cov[.gz]` shape.
3. **`MergeCpgSanityViolation`** error variant.
4. **`run_merge` core** (no discordance): gz-aware re-read; the `while`-pair loop + chr-start resync (§3.3); sanity asserts; pool + skip-zero + `%.6f`; emit via `ReportWriter`. Unit/golden test the merged line (`chr1 2 3 50.495050 408 400`).
5. **Discordance** branch (§3.5): both-measured gate; `abs(top-bottom) > N` → discordant file + skip; else pool. Golden test the empty-merged + discordant pair.
6. **`--zero_based`** half-open coords for both merged + discordant.
7. **Wire** `lib::run` (after `run_report`, gated on `merge_cpgs`).
8. **Goldens + tests** (§9): extend `generate_goldens.sh` / add a `phase_d` data dir from repo Perl v0.25.1.

## 6. Efficiency
- **Stream** the report (a 2-row sliding window + read-ahead), do **not** buffer all rows — a human CpG report is ~tens of millions of lines; the genome is already in RAM, so a full-row Vec would roughly double memory. **rev 1 (reviewer B-I3):** the resync read-ahead is **bounded only by EOF**, not "a few rows" — on an all-short-scaffold genome it can consume the rest of the file in one resync. That's fine (memory stays O(1) with a streaming `next_row()` over a gz-aware `BufRead`); **do NOT cap the read-ahead**. O(report lines).

## 7. Integration
- **Reads:** the CpG report file written by `run_report` (this run). **Writes:** `*.merged_CpG_evidence.cov[.gz]` (always) + `*.discordant_CpG_evidence.cov[.gz]` (with `--discordance_filter`). Context summary + the report itself are untouched.
- **Internal contract:** depends on the exact report-line bytes from Phase B (`chr\tpos\tstrand\tm\tu\tcontext\ttri\n`). A Phase B format change would break Phase D's parse — covered by the sanity asserts + goldens.
- **Downstream:** none in-scope (extractor inline switch unaffected — it drives `--merge_CpGs` via subprocess argv today).

## 8. Assumptions
**From epic (shared):** byte-identity to Perl v0.25.1 (STDERR exempt); `%.6f` percentages; merged/discordant cov are coverage files (`chr start end pct m u`); gzip compared after decompression. **Phase-D specific:**
1. Phase A guarantees CpG-context, non-split, no-threshold report (the merge precondition) — no re-check.
2. The report is genome-ordered with consecutive `+`/`-` CpG pairs except chr-start (§3.3).
3. `%.6f` (Rust `{:.6}`) matches Perl `sprintf "%.6f"` (same round-half-even on `f64` as the `%.2f` case verified in Phase B — re-confirm on the golden).
4. Stream (don't buffer all rows) — §6.
5. Discordance compares the **`%.6f`-formatted** percentages numerically (Perl computes `sprintf` strings then `abs($a-$b)` on them — Perl numifies the strings; tiny rounding-boundary risk — Q1).

## 9. Validation
| # | Verify | How | Expected |
|---|--------|-----|----------|
| V1 | `parse_report_row` | unit: a `+` and `-` row | exact fields |
| V2 | merged-cov filename (report-derived) | unit: `-o merge` → `merge.CpG_report.merged_CpG_evidence.cov`; `--gzip` → `…cov.gz`; discordant variant | exact strings |
| V3 | **merged cov golden** | run `--merge_CpGs` on the phase_b fixture; compare `*.merged_CpG_evidence.cov` to Perl golden | byte-identical (`chr1 2 3 50.495050 408 400`) |
| V4 | `--merge_CpGs --gzip` | decompress merged `.gz` → == plain merged golden; report still `.gz`; summary plain | byte-identical |
| V5 | **discordance golden** | `--merge_CpGs --discordance_filter 20` on a both-measured-discordant fixture | merged empty; discordant == Perl golden (both rows) |
| V6 | discordance both-measured gate | a pair with one strand 0,0 + large Δ on the other | NOT discordant → pooled normally |
| V7 | `--zero_based` half-open | `--merge_CpGs --zero_based` | merged `pos1 pos2+1`; discordant `pos pos+1` vs Perl golden |
| V8a | **chr-start resync — same-chr branch** | genome starting with `CG…` (chr-start CpG, no `-` partner) covered; `--merge_CpGs` | merged cov matches Perl golden |
| V8b | **chr-start resync — consecutive-short-scaffold SLIDE** (rev 1 I1/I4) | a chromosome + **two consecutive ≥3-bp `CGT` lone-orphan scaffolds** (each a single `+`-only report row → triggers the `chr1≠chr2` slide, the #98/#229 path; a 2-bp `CG` scaffold would emit NO row and miss it) | merged cov matches Perl golden (no desync) |
| V9 | uncovered CpG skipped | a CpG pair both 0,0 | absent from merged cov (skip-zero) |
| V10 | sanity assert fires on a corrupt report | (unit) feed a desynced row pair | `MergeCpgSanityViolation` (no panic) |
| V11 | regression: Phases A–C unaffected | full suite | green |
| V12 | **discordance rounding boundary** (rev 1 C1) | `1/1`(50%) vs `11/9`(55.000…007%), `--discordance_filter 5` → rounded Δ = 5.0, NOT `>5` | **merged** (`chr1 2 3 54.545455 12 10`), discordant **empty** — vs Perl golden (a raw-f64 impl would wrongly discordant-route) |
| V13 | **EOF-mid-resync** (rev 1 C1) | genome ending in two trailing ≥3-bp `CGT` lone-orphan scaffolds → resync hits EOF | `MergeCpgSanityViolation` (exit 1, no panic); the partial merged file matches the lines Perl wrote before its `die` |
| V14 | multi-pair / multi-chromosome merged golden | a genome with several CpGs across 2 chromosomes, mixed coverage | merged cov byte-identical to Perl |

Goldens from repo Perl v0.25.1 (local). V8b/V13 fixtures verified against live Perl by both plan reviewers.

## 10. Questions or ambiguities
| Priority | Question | Resolution |
|----------|----------|------------|
| **Resolved (C1, binding)** | Discordance: raw `f64` or `%.6f`-rounded comparison? | **`%.6f`-rounded**, strict `>`, vs integer `N` (§3.5) — both reviewers proved a raw-f64 compare byte-diverges at the boundary (V12). No longer optional. |
| Resolved | Could the report file be `.gz` while merge re-reads it? | Yes if `--gzip` — re-read gz-aware (`MultiGzDecoder`). Covered (V4). |

No **Critical** ambiguities remain — §9 merge behavior is observed; the resync + the two rev-1 Criticals (rounded-discordance, EOF-mid-resync) are pinned and golden-tested.

## 11. Self-Review
- **Logic:** the 2-row sliding window + resync mirrors Perl `:1794-1897` exactly, incl. the consecutive-short-scaffold case (read-until-chr-match). Sanity asserts guard desync. Discordance `continue`s (skips merge) per Perl. Skip-zero matches Perl `:1939`.
- **Edge cases:** chr-start CpG (V8), consecutive short scaffolds (resync), uncovered CpG (V9), both-measured gate (V6), zero_based half-open (V7), gz report (V4), corrupt report (V10).
- **Efficiency:** streaming (no full-row buffer) — §6.
- **Integration:** report-line bytes are an internal contract; sanity asserts + goldens catch drift. `ReportWriter` reused for gz.
- **Risks:** (a) the resync is the highest-risk port — mitigated by V8a/V8b (incl. the consecutive-short-scaffold slide) + V13 (EOF-mid-resync) against live Perl; (b) discordance rounding — now binding (§3.5) + boundary golden V12; (c) `%.6f` parity (Phase B verified for `%.2f`; re-confirm on V3).

**Folded from dual plan-review (rev 1, 2026-05-30 — both APPROVE-WITH-CHANGES; both verified vs live Perl; core algorithm confirmed faithful):**
- **C1a (B): discordance compares `%.6f`-ROUNDED percentages**, strict `>`, vs integer `N` — a raw-f64 compare byte-diverges at the boundary. Now binding (§3.5) + `round6` helper (§5) + boundary golden V12.
- **C1b (A): EOF-mid-resync makes Perl `die`** with a partial merged file. `next_row()→Option`; mid-resync `None` → `MergeCpgSanityViolation` (no panic); **stream-write** merged lines (§3.6) so the partial matches; **no cleanup** on error (§3.4/§5). + V13.
- **I1 (A+B): visibility** — `ReportWriter`/`report_path`/`report_name` are private → `pub(crate)` promotion task (§5); c2c has no partial-cleanup helper (noted).
- **I2 (A+B): V8 fix** — a 2-bp `CG` scaffold emits no report row → can't trigger the slide; use ≥3-bp `CGT` lone-orphan scaffolds (V8b).
- **I3 (B): §6 efficiency** — resync read-ahead is bounded by EOF (not "a few rows"); don't cap it.

## Revision history
- **rev 0** (2026-05-29): initial Phase D plan from EPIC + SPEC rev 3 + Perl `combine_CpGs` (`:1753-1958`, resync read line-by-line) + Phase-B/C writer/reader reuse + empirically-observed merged/discordant/gzip behavior + report-derived filenames.
- **rev 1** (2026-05-30): dual plan-review folded (both APPROVE-WITH-CHANGES; both ran live Perl). 2 Criticals (rounded-discordance comparison; EOF-mid-resync die + no-panic/no-cleanup/stream-write) + Important (pub(crate) visibility, V8 ≥3-bp scaffold fix, EOF-bounded resync) + Q1 resolved + V12–V14.
- **rev 2** (2026-05-30): dual code-review (both **APPROVE**, zero Critical/Important; both byte-diffed the binary vs live Perl on 36+ adversarial fixtures incl. boundary rounding both directions, 1/2/3-orphan resync, EOF-die-with-partial, multi-chr, and a 22k-value `%.6f`↔`{:.6}` parity sweep) + plan-manager (**COMPLETE**, 42 items). All findings advisory; folded the test/provenance gaps — V2/V6/V8a/V10/V14 dedicated tests + the `generate_goldens.sh` phase_d block + the stale `≥7`-fields doc note. 97 tests.
