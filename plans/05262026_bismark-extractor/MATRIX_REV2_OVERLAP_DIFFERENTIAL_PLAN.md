# Plan — PE matrix rev 2: overlap differential `≥5%` → `strictly > D`

## Context

The post-#880 colossal release-verification (Task #23) ran the full Phase H PE
matrix on `rust/iron-chancellor` HEAD `6782021` (out dir
`colossal:~/phase_h_pe_release_v879fix/`, 2026-05-28). Results:

- **All 10 byte-identity cells PASS** (Rust ≡ Perl, raw-byte for reports +
  sorted-equivalent for data files).
- **Cross-N invariance PASS** for all 5 cells (Rust N=1 ≡ Rust N=4).
- **M-bias baseline (D,N=1) = 11,443 B** ✓ (Phase C.1 polarity guard).
- **#879 fix verified**: the `r1r2_3p` cell (`--ignore_3prime 5
  --ignore_3prime_r2 5`) — which failed 8/8 pre-#880 — is now byte-identical
  to Perl at both N=1 and N=4.
- **Exit code 1**, caused **solely** by the `overlap`-cell mixed-metric
  differential heuristic.

The differential (PE plan rev 1 A-O3) asserts that `--include_overlap` must
raise the M-bias count-sum **> D + 5%**. On `SRR24827378_10M` the real bump is
**+2.28%** (overlap=192,423,276 vs D=188,123,599) — below the 5% floor, so the
gate trips.

**This is not a regression.** The `overlap` cell's `M-bias.txt` is
byte-identical between Perl and Rust (verified by `cmp` → `IDENTICAL`), so
192,423,276 is *Perl v0.25.1's* count-sum too. The heuristic would fail even
comparing Perl-against-Perl. The `5%` floor encodes a fixed assumption about
R1/R2 mate-overlap that this library (100% properly-paired, but longer inserts
⇒ less mate overlap) does not satisfy.

`--include_overlap` accumulates calls at **bases where R1 and R2 mates
physically overlap**. The count increase therefore scales with the
overlapping-base fraction — an insert-size-vs-read-length property that varies
per library — **not** with read count or methylation rate. The only invariant
that always holds is **monotonic**: count-sum is *strictly greater* than D
whenever any mate overlap exists. The matrix already gates `properly-paired
fraction ≥ 80%` in pre-flight, which guarantees overlap is present and the
strict-`>` test is meaningful. The `+5%` magnitude was an over-specification.

Intended outcome: recalibrate the assertion to the correct invariant
(`strictly > D`), so the release gate reflects byte-identity + monotonic
semantics rather than a dataset-specific magnitude. This mirrors the
RELEASE_CHECKLIST "baseline drift" escalation (rev 1 A-I4): when the matrix
asserts a value wrong for the *actual data*, fix the assertion via a rev-2 PR —
do not bypass the gate.

## Scope

Two files (no Rust source, no test changes):
1. `scripts/phase_h_pe_matrix.sh` — the differential assertion + wording.
2. **`rust/bismark-extractor/SPEC.md` (REQUIRED, rev 1 — both plan-reviewers
   Critical).** SPEC §8.3 **line 766 normatively pins the magnitude**:
   `overlap: M-bias data count-sum … > D's same metric by ≥ 5%`. The SPEC must
   be edited in the SAME PR to read `strictly > D`, or the harness would
   contradict its own spec — the exact anti-pattern this change removes. (My
   original "no SPEC change required" claim was factually wrong.)

## Behavior change

**Before:** `overlap` differential PASS iff `count-sum > D_COUNTS * 1.05`.
**After:** `overlap` differential PASS iff `count-sum > D_COUNTS` (strictly).

The three `<D` row-count assertions (r1_5p, r2_5p, r1r2_3p) are unchanged —
they already encode the correct invariant (`--ignore N` removes M-bias
positions ⇒ strictly fewer rows) and all passed (375, 375, 360 < 390).

## Implementation outline

`scripts/phase_h_pe_matrix.sh`:

1. **Assertion (lines ~501–511)** — replace the `+5%` threshold block. Note the
   **`-le` (fail-on-equality) boundary is retained deliberately** and now
   documented (rev 1 — Reviewer A Important): properly-paired (FLAG 0x2) does
   NOT imply mate overlap, so a long-insert library *could* legally produce
   `count-sum == D` (zero overlapping bases). We keep `-le` because zero net
   overlap on a WGBS library means `--include_overlap` was a no-op — a genuine
   regression worth failing on, not a false positive.
   ```bash
   # overlap count-sum strictly > D (rev 2: dropped the +5% floor; the
   # magnitude of the --include_overlap bump scales with mate-overlap-base
   # fraction (insert size vs read length), a per-library property, not a
   # fixed constant. The invariant that always holds is monotonic increase.
   # 80%-properly-paired pre-flight gate ensures overlap is meaningful.
   # `-le` fails on count-sum == D intentionally: zero net overlap = no-op
   # --include_overlap = regression on a WGBS library.)
   if [[ -n "$OVERLAP_COUNTS" && -n "$D_COUNTS" && "$D_COUNTS" -gt 0 ]]; then
     if [[ "$OVERLAP_COUNTS" -le "$D_COUNTS" ]]; then
       PASS_FLAG=0
       ROW_COUNT_DETAIL="$ROW_COUNT_DETAIL [FAIL: differential overlap count-sum=$OVERLAP_COUNTS not > D=$D_COUNTS]"
     fi
   else
     PASS_FLAG=0
     ROW_COUNT_DETAIL="$ROW_COUNT_DETAIL [FAIL: overlap count-sum=$OVERLAP_COUNTS or D count-sum=$D_COUNTS unreadable]"
   fi
   ```
   (Drops the `OVERLAP_THRESHOLD` computation entirely — grep-confirmed local to
   lines 503/506, no other references.)

2. **Wording updates** (no logic) — **FIVE sites, not four** (rev 1 — Reviewer A
   caught the missed 5th):
   - **Inline comment line ~501**: `# overlap count-sum > D + 5% (rev 1 A-O3)` →
     new rev-2 comment block above.  ← *was missing from rev 0 list*
   - Header comment line ~36: `> D + 5%` → `strictly > D`.
   - Comment line ~414: same.
   - speedup_table.md emitter line ~699: `strictly > D by ≥5%` → `strictly > D`
     (confirm line 700 still reads cleanly after the edit).
   - verdict REASON line ~773: `count-sum>D+5% for overlap` → `count-sum>D for overlap`.

3. **SPEC §8.3 line 766** (`rust/bismark-extractor/SPEC.md`): `> D's same metric
   by ≥ 5%` → `strictly > D's same metric` (REQUIRED — see Scope).

4. Add a `# rev 2 (2026-05-29):` provenance note near the assertion pointing at
   this plan + the colossal evidence dir, consistent with the file's existing
   `rev 1 A-O3` annotation style.

5. **Post-edit grep sweep** (rev 1 — both reviewers): after editing, run
   `grep -nE '5%|105|1\.05' scripts/phase_h_pe_matrix.sh rust/bismark-extractor/SPEC.md`
   to confirm no straggler references to the old magnitude remain.

## Assumptions

- The `overlap` count-sum is extracted correctly today (it is — the value
  192,423,276 was read and reported); only the comparison constant changes.
- `strictly > D` will hold on SRR24827378: 192,423,276 > 188,123,599 ✓ → PASS.
- No other call site references `OVERLAP_THRESHOLD` (to grep-confirm during
  implementation; it appears local to this block).

## Verification

**Stage 0 — executable syntax + boundary check (rev 1 — both reviewers
Critical/Important):** Stage A below is a manual arithmetic re-derivation; it
does NOT execute the edited code, so it cannot catch a `-le`/`-lt` slip or a
syntax error under `set -euo pipefail` that flips fail-closed → fail-open.
Before relying on either real-data stage:
1. `bash -n scripts/phase_h_pe_matrix.sh` — syntax check.
2. A mocked/synthetic FAIL-direction check on the **`== D` boundary** (the case
   no real-data stage exercises): feed `OVERLAP_COUNTS == D_COUNTS` and assert
   `PASS_FLAG` goes to 0; feed `OVERLAP_COUNTS = D_COUNTS + 1` and assert it
   stays 1. (Extract the block into a throwaway harness or set the vars and
   source the assertion.)
3. The post-edit grep sweep (implementation step 5).

After the edit (then two-stage; stage A is instant, stage B is the canonical record):

**Stage A — logic pre-check against preserved outputs (instant, no re-run):**
The byte-identity outputs in `~/phase_h_pe_release_v879fix/cell_p1_*/` are
deterministic and already on disk. Re-evaluate just the differential math:
`192423276 > 188123599` ⇒ overlap differential now PASSes; combined with the
already-recorded 10/10 byte PASS + cross-N PASS + perf-miss, the driver's exit
logic yields **exit 3** (byte-identity PASS, perf target missed — shippable for
v1.0 per RELEASE_CHECKLIST §3.3 / PHASE_H_PE_PLAN §1).

**Stage B — canonical fresh full re-run (the release-gate record):**
```bash
# on colossal, in tmux, bioinf env + cargo on PATH
bash scripts/phase_h_pe_matrix.sh \
  /weka/projects/bioinf/Data/Felix/bismark_benchmarks/10M_PE/SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplicated.bam \
  --out ~/phase_h_pe_release_v879fix_rev2/   # fresh dir
```
Expect **exit 3**, `matrix_verdict.txt` differential line shows
`overlap count-sum=… > D=…` (no `[FAIL …]`), all else PASS as before. ~2–2.5 h.

**Resolved (rev 1 — both reviewers):** Stage A alone is NOT an acceptable v1.0
gate record — it reconstructs the verdict by hand rather than executing the
edited driver. **Recommendation: Stage 0 + Stage B (fresh full re-run) is the
canonical record**; Stage A is retained only as a fast pre-flight sanity check
on the arithmetic. Stage B honors the checklist's literal "re-run on a fresh
`--out` dir" and is the artifact attached to #798. (Pending Felix's call on
whether to spend the ~2.5 h — but the reviewers' default is yes.)

## Implementation notes (2026-05-29)

Branch `matrix-rev2-overlap-differential` off `rust/iron-chancellor` (`6782021`).

Six edits applied, all per rev-1 plan:
1. `scripts/phase_h_pe_matrix.sh` assertion (501–507): `> D+5%` → strictly `> D`;
   `OVERLAP_THRESHOLD` removed entirely; documented `-le` fail-on-equality.
2–5. Wording at lines 36, 414, 699, 773 updated to "strictly > D".
6. `rust/bismark-extractor/SPEC.md:766`: `≥ 5%` → `strictly > D` + rev-2 rationale.

Verification:
- **Grep sweep**: `OVERLAP_THRESHOLD` → 0 matches (fully removed). Remaining `5%`
  hits are all historical rationale in comments, not live logic.
- **`bash -n`**: SYNTAX OK.
- **Stage 0 boundary test** (synthetic, exercises the edited comparison):
  6/6 assertions pass — real-data PASS, `==D` FAIL, `D+1` PASS, `D-1` FAIL,
  empty/`D==0` fail-closed. This covers the `==D` boundary no real-data stage hits.
- **Stage B** (fresh full re-run, Felix's choice): launched on colossal, out dir
  `~/phase_h_pe_release_v879fix_rev2/`; expect exit 3 (perf-miss, shippable).

No deviations from the rev-1 plan.

## Out of scope (tracked separately)

- **Perf miss** (Rust N=4 = 0.58× scaling): known #876 Finding #4; exit-3 class;
  file/append `perf(extractor):` under #798 — not part of this PR.
- **BAM MD5 reconciliation**: matrix recorded `4a44918c…`; handoff referenced
  `9ebec4c9…`. M-bias baseline 11,443 B matched ⇒ correct fixture; update the
  stale reference where it lives (separate housekeeping).
