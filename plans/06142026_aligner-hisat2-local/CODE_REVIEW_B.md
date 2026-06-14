# Code Review B — HISAT2 `--local` alignment (test-quality + edge-case lens)

**Reviewer:** B (test non-vacuity + edge cases, complementing Reviewer A's correctness focus)
**Branch:** `rust/aligner-hisat2-local` (worktree `/Users/fkrueger/Github/Bismark-hisat2local`), uncommitted vs `origin/rust/iron-chancellor`
**Scope:** `rust/{README.md, bismark-aligner/src/{cli,config,options,mapq}.rs, bismark-aligner/tests/cli.rs}`
**Verdict:** **APPROVE-WITH-NITS**

The change is faithful to Perl v0.25.1 `--hisat2 --local`, the new tests are non-vacuous and genuinely exercise the load-bearing paths, and every IMPL-review delta item (A1/A2/A3/A4+B1/B5) is addressed. I found **no Critical or High issues**. One Medium (a Perl-divergent error message on an invalid-input path — not gate-breaking), and a few Low nits. Scoped tests re-run green locally (14 lib + 1 e2e).

---

## Verification performed (independent)

**MAPQ test arithmetic — re-derived from scratch in Python against the Perl ladder (`bismark:4082-4178`):** all six `local_hisat2_default_params_mapq` assertions are correct:
- `(50,None,0,None)`→44, `(50,None,-1,None)`→22, `(150,None,0,None)`→44 (no-second-best, sub-unity diff 0.7824).
- `(50,None,0,Some(-1))`→40 (flat top bucket), `(50,None,-1,Some(-1))`→0.
- **The critical PE summed-ln `==diff` leaf** `(150,Some(150),0,Some(-1))`→34: `diff = 0.4·ln(150) = 2.004254`, `diff·0.5 = 1.002127`, so `best_diff=1 < diff·0.5` falls into the **0.4 bucket** (not 0.5) and `best_over==diff` → 34. **Confirmed correct and genuinely `ln()`-dependent** — naive integer-`scMin` arithmetic would not reproduce it.

**Soft-clip e2e non-vacuity — traced the full path:** read `ACGTAC` (6 bp), CIGAR `2S4M`, index 0 (CT_CT, `+`) at pos 1 → window = 2×`X` (the `S`, `methylation.rs:174` `S`-as-`I`) + 4 genomic (`M`) + 2 (3' append) = **8 bytes** = `seq_uc.len()+2` → **passes the `read_len+2` driver guard** (`lib.rs:787`), record is written, and the `2S4M` CIGAR round-trips into the BAM. The test's `has_softclip` assertion is **load-bearing** (a regression that dropped `--no-softclip` handling or mishandled `S` would fail it), and the report's `!contains("--no-softclip")` / `!contains("--local")` asserts directly contrast V7 (`hisat2_se_mapped_names_and_report`, which DOES echo `--no-softclip`).

**Tests re-run (sandbox disabled — cargo lock needs it):** `mapq::*`, `options::tests::score_min*`, `options::tests::hisat2_local*`, `config::tests::resolve_local*` → 14 passed; `tests/cli.rs::hisat2_local_softclip_roundtrip_and_options` → 1 passed.

---

## Findings

### Medium

**M1 — HISAT2-`--local` G-form rejection emits the *end-to-end* error message, diverging from Perl (invalid-input path; NOT gate-breaking).**
`build_aligner_options` collapses HISAT2-local and end-to-end into one `else` branch (`options.rs:99-115`). A HISAT2-local user passing `--score_min G,20,8` is rejected by `valid_score_min_l` with **"In end-to-end mode (default) the option '--score_min <func>' needs to be in the format <L,value,value>..."**. Perl v0.25.1 has a *distinct* message for this case:
- Perl `:7909` (HISAT2 `--local`, G given): `"In HISAT2 --local mode, the option '--score_min <func>' needs to be in the format <L,value,value>"`
- Perl `:7918` (end-to-end): `"In end-to-end mode (default) ... <L,value,value> . Please consult ..."`

So the user is told "end-to-end mode" while in `--hisat2 --local` mode. This is a faithfulness gap, but only on a `die`/STDERR path — the Bismark byte-identity gate compares the BAM + report of *valid* runs, never stderr of a rejected run, so it does **not** break the oxy gate. The `score_min_params` G-form reject for HISAT2 (`options.rs:355`) is tested with `is_err()` only (`score_min_params_aligner_and_mode_defaults`), so the message divergence is also **untested**.
*Recommendation:* either (a) accept as a documented, out-of-gate deviation (cheapest, and arguably fine since it's an error path), or (b) branch the `else` arm to emit the HISAT2-`--local` message when `cli.local && aligner == Hisat2`, and add a `.to_string().contains("HISAT2 --local mode")` assert. Low risk either way; flagging because the prompt named this exact edge case ("HISAT2-local must reject a G-form") and the *rejection* is faithful but the *message* is not.

### Low

**L1 — The `local_hisat2_default_params_mapq` "1-ULP wobble would flip it" comment overclaims fragility.**
The comment at `mapq.rs:405-406` says a "1-ULP `ln()` wobble pushing `diff·0.5` below 1.0 would flip this to 35." Measured: `diff·0.5 = 1.0021270588`, i.e. **~0.00213 above 1.0 ≈ 9.6e12 ULPs of headroom** — a single ULP cannot flip it. The assertion value (34) is correct and the test IS genuinely `ln()`-dependent (it needs the real `ln(150)` to land in the 0.4 bucket), so this is purely a misleading comment. Risk: at oxy-gate time it could spawn a false "platform-`ln()`-parity will break this leaf" worry. Reword to "this leaf is `ln()`-valued (not integer-reproducible); the margin above the 0.5 boundary is ~0.002 — comfortably above any cross-platform `ln()` drift" or similar.

**L2 — The SE arm of IMPL-delta A4+B1 ("SE second-best `best_over==diff` interior leaf") is physically unsatisfiable at `(0,-0.2)`; the implementer correctly substituted a PE case.**
Verified: with a single SE read and `(0,-0.2)`, the smallest non-zero `best_diff` (=1) only escapes the flat-top buckets (≥0.6·diff) once `diff > 1.667`, i.e. `readLen > 4167 bp` — unreachable for real reads. The interior `==diff` leaf (34/35/...) is reachable only by **doubling diff via PE summed-ln** (150+150 → diff≈2.0), which is exactly what the test does. This is a **sound deviation** from the literal A4+B1 wording, not a defect — but it is undocumented. Worth a one-line note in the test (or PROGRESS) that the SE interior leaf is unreachable in this param regime, so a future reader doesn't "restore" a vacuous SE sweep.

**L3 — Report substring asserts (`!report.contains("--local")` / `!contains("--no-softclip")`) scan the whole report, not the `aligner_options` line.**
`tests/cli.rs:2050-2051`. Safe today (those substrings appear only in the option string), but slightly fragile if future report text ever mentions `--local`/`--no-softclip` elsewhere. The positive assert at `:2049` already pins the exact option line, so the negatives are belt-and-suspenders; consider asserting the negatives against the extracted option line for robustness. Non-blocking.

**L4 — `methylseq_conformance.rs:185` docstring is now partially stale** ("HISAT2/minimap2-local + ... unsupported"). It is a comment, not an assertion (the test only exercises Bowtie 2 `--local`), so nothing fails — but it now contradicts the shipped behavior for HISAT2. Trivial doc refresh.

---

## Edge-case coverage assessment (prompt item 2)

| Edge case | Covered? | Notes |
|-----------|----------|-------|
| `--hisat2 --local --score_min G,…` rejected | Partial | Rejection IS tested (`is_err()`); message divergence is M1, untested |
| `--hisat2 --local --score_min L,…` override accepted | ✅ | `score_min_params` + `hisat2_local_option_string` (`L,0,-0.6`) |
| `--hisat2 --local --multicore N` compose | Unit-decomposed | `--score-min` (step 7) and `-p N --reorder` (step 10) are independent, softclip-tail keys on `cli.local` independently; no single composed-string test, but oxy gate has the explicit cell. Low risk. |
| `--hisat2 --local` PE | Unit only | `hisat2_local_option_string` PE arm (option string). **No PE e2e soft-clip round-trip** despite IMPL Task 5 saying "SE + PE" — deferred to oxy gate (see C1). |
| `--hisat2 --local` non-dir / pbat | Not tested locally | Deferred to oxy gate matrix. Strand/conversion routing is shared with end-to-end (`methylation.rs` index dispatch), untouched by this change, so low risk. |
| soft-clip vs `read_len+2` guard | ✅ | Traced: `2S4M` window = 8 = `seq.len()+2`, passes (verified above). |
| `--local --combined_index` rejected | ✅ (unchanged) | `config.rs:307-313`, preserved. |
| minimap2 `--local` rejected ("by design") | ✅ | `resolve_local_aligner_scope` asserts `.contains("--minimap2")` && `.contains("by design")`. |

### Coverage note (not a defect — deferred by design)

**C1 — No PE / non-dir / pbat / multicore `--local` *e2e* test exists; the only new e2e test is SE-directional.**
IMPL Task 5 reads "SE + PE" and Task 10 (oxy) is "SE + PE × {dir, non-dir, pbat} + a `--multicore` cell". The local test infra (`make_fake_hisat2_local_softclip`) is SE-only. This is consistent with how the rest of the aligner suite defers full PE/non-dir/pbat byte-identity to the oxy gate (the unit/e2e layer pins the *option string* and a representative SE round-trip; the oxy gate proves byte-identity across the matrix). I would **not block** on it — but the IMPL's "SE + PE" wording for Task 5 is not literally met locally, and the PE soft-clip round-trip (PE `read_len+2` per-mate guard with an `S` op) is genuinely a *different* code path (`extract_corresponding_genomic_sequence_paired_end`, `lib.rs:2814/2822`) than the SE one that was tested. Recommend either a PE fake-HISAT2-local e2e (cheap, mirrors the SE one) **or** an explicit note that PE soft-clip is gated only at oxy. The oxy gate's non-vacuity asserts (B2+B3: `S`-count > 0 + same-dataset cross-check) remain the real proof and must not be skipped.

---

## IMPL-review delta cross-check (prompt item 3)

| Item | Status |
|------|--------|
| A1 — reject test shape ("NOT the local-reject error", accept Ok OR non-local Err for HISAT2) | ✅ `resolve_local_aligner_scope` uses `if let Err` + negative asserts; verified the fall-through is `"No genome folder specified!"` (`config.rs:787`) |
| A2 — minimap2 message asserted (`contains("by design")`) | ✅ |
| A3 — `config.rs:291-294` reject-block comment flipped | ✅ |
| A4+B1 — `ln()`-sensitive MAPQ buckets (not vacuous `(20,8)`-only) | ✅ via PE summed-ln `==diff` leaf (SE arm unsatisfiable → L2) |
| B5 — byte-frozen option-string tests kept green | ✅ `hisat2_local_option_string` re-asserts Bowtie2-local `--local --score-min G,20,8`; V7/V8 end-to-end still echo `--no-softclip --omit-sec-seq` |

---

## Oxy-gate risk assessment (prompt item 4)

- **@PG / report version line:** the report echoes the SAME `aligner_options` the aligner received (verified via the e2e test) — the L-form + `--omit-sec-seq` + no-`--local`/no-`--no-softclip` delta is faithful to Perl `:7913/7947/8311`. No new @PG surface.
- **MAPQ:** `score_min_params(cli, aligner)` (the load-bearing edit, `options.rs:352`) uses `rsplit_once` (greedy last-comma, Perl-equivalent) and `calc_mapq` is unchanged/sign-agnostic. The `ln()` bit-safety was established by the Phase-0 spike; the PE summed-ln leaf has ~0.002 boundary headroom (L1), so cross-platform `ln()` drift will not flip it.
- **Soft-clip non-vacuity:** the local run produces soft-clips (proven in the e2e); the oxy gate's blocking B2+B3 asserts (`S`-count > 0 AND same-dataset `--hisat2` end-to-end ≈ 0 AND BAMs differ) remain mandatory — do not pass a vacuous (S=0) gate.
- **Ordering:** option assembly is linear; `--local`/multicore/softclip-tail are independent. No ordering risk introduced.

No oxy-gate-breaking issue found in the code under review. The remaining risk is purely **gate execution discipline** (run the non-vacuity asserts; cover PE/non-dir/pbat at the gate per C1).

---

## Summary

- **Verdict: APPROVE-WITH-NITS.** No Critical/High. Faithful, well-tested, IMPL-delta-complete.
- **Act on first:** M1 (Perl-divergent G-form error message — decide accept-as-deviation or fix+test) and L1 (overclaiming ULP comment).
- **Before/at oxy gate:** C1 (PE/non-dir/pbat soft-clip is gate-only; consider a cheap PE e2e) + enforce the B2+B3 non-vacuity asserts.

**File:** `/Users/fkrueger/Github/Bismark-hisat2local/plans/06142026_aligner-hisat2-local/CODE_REVIEW_B.md`
