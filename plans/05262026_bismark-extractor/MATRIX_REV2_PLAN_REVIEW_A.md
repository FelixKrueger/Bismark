# Plan Review A — PE matrix rev 2: overlap differential `≥5%` → `strictly > D`

**Reviewer:** A (independent, fresh context)
**Plan:** `plans/05262026_bismark-extractor/MATRIX_REV2_OVERLAP_DIFFERENTIAL_PLAN.md`
**Target:** `scripts/phase_h_pe_matrix.sh`
**Date:** 2026-05-29

---

## Verdict (summary)

The core recalibration — dropping the `+5%` floor to `strictly > D` — is **logically
correct** for the stated invariant and is the right fix for the misfire. However, the plan
contains **one factual error that is Critical**: it claims "no SPEC change required," but
SPEC §8.3 line 766 **does** pin the `≥ 5%` magnitude in normative prose. The PR must edit
SPEC §8.3 too, or the SPEC and the gate will disagree — exactly the "assertion wrong for the
actual data" failure the plan is trying to eliminate, just relocated to the spec. There is
also one **Important** wording-site the plan missed (the emitter prose at line 700, which the
plan references only at 699), and the equality-boundary question (`-le`) deserves an explicit
ruling rather than the silent inheritance it currently has.

---

## 1. Logic review

### 1.1 Is `strictly > D` the correct invariant? — Yes.

The reasoning is sound. `--include_overlap` overrides the default `--no_overlap`, so R2
calls in the mate-overlap region that were previously dropped are now retained and
accumulated into M-bias positions (which are read-relative, not reference-relative, so no
new rows appear). Therefore count-sum strictly increases iff any overlap base exists. The
`+5%` was an over-specified magnitude derived from one library's geometry; replacing it with
the monotonic invariant is correct. The three `<D` row-count assertions are correctly left
untouched — they already encode the right monotonic invariant and all passed.

### 1.2 Does dropping `OVERLAP_THRESHOLD` introduce an edge case? — One subtle inconsistency.

The proposed block (plan lines 74–82) keeps the structure of the current code (lines
502–511): an outer guard `[[ -n "$OVERLAP_COUNTS" && -n "$D_COUNTS" && "$D_COUNTS" -gt 0 ]]`
with an `else` that fail-closes on unreadable values. This is preserved correctly and the
`D_COUNTS -gt 0` guard prevents the divide-by-zero that the old `* 105 / 100` never actually
risked anyway. Good.

**Subtle inconsistency worth flagging:** the three row-count asserts at lines 489/493/497
each carry a trailing `2>/dev/null` on the `[[ ... ]]` test itself:

```bash
if [[ -n "$R1_5P_ROWS" && -n "$D_ROWS" && "$R1_5P_ROWS" -ge "$D_ROWS" ]] 2>/dev/null; then
```

The proposed overlap block (plan line 75) does **not** carry the `2>/dev/null`:

```bash
if [[ "$OVERLAP_COUNTS" -le "$D_COUNTS" ]]; then
```

The current overlap block (line 504) also omits it, so the plan is faithful to the existing
style — but note the asymmetry. The `2>/dev/null` on the row-count tests exists because
`[[ "X" -ge "Y" ]]` with a non-integer operand throws a "bad math expression" / "invalid
arithmetic operator" diagnostic to stderr (the `count_mbias_rows`/`sum_mbias_counts` awk
helpers can in principle emit an empty string if `$f` is missing, though the `-n` guard
catches that first). In the overlap block the outer `[[ -n "$OVERLAP_COUNTS" && -n
"$D_COUNTS" && "$D_COUNTS" -gt 0 ]]` guard already proves both operands are non-empty
integers **before** the inner `-le`, so the inner test cannot throw and the `2>/dev/null`
would be redundant. The logic is safe as written. **Optional**: for symmetry and defense in
depth, mirror the `2>/dev/null` so a future refactor that weakens the outer guard doesn't
silently fail-open. Low priority — the outer guard is robust today.

One thing the plan gets exactly right: it preserves the `-le` (fail on `<=`) sense, which is
the strict-`>` PASS condition expressed as its negation. The fail message text is updated to
"not > D" — consistent.

### 1.3 Wording-string consistency — one missed site.

The plan lists four wording sites (plan lines 88–91):
- Header comment ~36 — **confirmed**: line 36 reads `> D + 5%`. ✓ (current text is at line 36)
- Comment ~414 — **confirmed**: line 414 reads `count-sum > D + 5%`. ✓
- speedup_table.md emitter ~699 — **partially correct**: line 699 reads `strictly > D by ≥5%`,
  but the **immediately following line 700** is a continuation that also encodes the 5%
  semantics implicitly (`--include_overlap accumulates counts at existing positions; rows
  unchanged`). Line 700 itself does not say "5%", so it may not need editing — but the plan
  should explicitly confirm 699 is the only line in that `echo` block carrying the magnitude.
- verdict REASON ~773 — **confirmed**: line 773 reads `count-sum>D+5% for overlap`. ✓

**Additional missed site (Important):** the inline comment at **line 501**
(`# overlap count-sum > D + 5% (rev 1 A-O3)`) directly above the assertion block still says
`> D + 5%`. The plan's outline §1 replaces the *assertion block* (lines 502–511) and adds a
new comment, but does not call out line 501 explicitly. If the implementer replaces only
502–511, line 501 is orphaned with stale `+5%` text. The plan's proposed new comment (lines
68–73 of the plan) would presumably subsume it, but this should be made explicit so the
implementer deletes/replaces line 501 rather than leaving a contradictory one-liner above the
new block. **Recommend the plan enumerate line 501 in the wording-updates list.**

So: comments at lines **36, 414, 501**, emitter **699**, verdict **773** — five sites, not
four. The plan lists four and folds 501 implicitly. Make it explicit.

---

## 2. Assumptions

### 2.1 "Count bump scales with overlap-base fraction, not read count." — Valid.

Correct. M-bias accumulation adds `methylated + unmethylated` counts at read-relative
positions. `--no_overlap` removes R2 calls whose reference coordinate falls within R1's
reference span; `--include_overlap` keeps them. The number of *additional* calls retained is
a function of how many R2 bases overlap R1, i.e. `read_length − (insert_size − read_length)`
clamped to `[0, read_length]` per pair — purely an insert-size-vs-read-length geometry. Read
count scales both D and overlap count-sums proportionally, so it cancels in the ratio. The
empirical +2.28% on a longer-insert library vs a hypothetical ~5%+ on a short-insert library
is exactly consistent with this model. **Assumption validated.**

### 2.2 Is the 80%-properly-paired gate sufficient to make `strictly > D` meaningful? — Mostly, with a caveat.

This is the weakest link in the plan, and the plan itself flags it as an open question in the
review prompt. The honest answer:

- **Properly-paired (FLAG 0x2) does NOT imply mate overlap.** A pair can be properly paired
  (correct orientation, both mapped, insert within the aligner's expected distribution) yet
  have **zero overlapping bases** if `insert_size > R1_len + R2_len` (a gap between mates).
  The plan's own §7.4 SPEC discussion describes exactly this geometry: read `.9` of the 10M
  PE BAM is a disjoint FR pair with a 7 bp gap, where `drop_overlap` keeps **all** R2 calls.
  For such a pair, `--no_overlap` and `--include_overlap` produce **identical** output.

- Therefore a **pathological-but-legal** library where ~all properly-paired reads have inserts
  longer than `R1_len + R2_len` would produce `overlap count-sum == D` (zero added calls),
  and the new `-le` test would **FAIL** (exit 1) even though Rust ≡ Perl byte-identically.
  This is the same class of false-FAIL the plan is fixing, just at the opposite boundary.

- **How likely is this in practice?** For canonical WGBS PE libraries (the gate's stated
  target), inserts are typically 150–400 bp with 100–150 bp reads, so substantial overlap is
  the norm and `> D` holds comfortably (as the +2.28% case shows — that is already a
  *long-insert* library and it still cleared `> D`). The 80% gate filters out exome/mate-pair
  panels. So for the intended use the strict-`>` is meaningful. But the gate does **not**
  *guarantee* overlap > 0; it only makes it overwhelmingly likely.

**Is `-le` (fail on equality) the right boundary?** This is a genuine design call the plan
should make explicitly rather than inherit silently:

- **Argument for `-le` (current plan):** count-sum == D means literally zero overlap bases
  across the entire library, which for a ≥80%-properly-paired WGBS library is biologically
  implausible and more likely indicates a bug (e.g., `--include_overlap` silently not taking
  effect, or `drop_overlap` running when it shouldn't). Failing on equality catches a real
  "include_overlap is a no-op" regression. This is the fail-closed-friendly choice and is
  **defensible**.
- **Argument for `-lt` (fail only on strict decrease):** equality is not a *regression* in
  the byte-identity sense — if Perl also produces count-sum == D on that library, the cells
  are still byte-identical and the release should not be blocked. `-lt` would treat equality
  as PASS and only fire when overlap count-sum is *less* than D (which would be a true
  polarity/logic inversion — the C.1 bug class this cell guards against).

**My recommendation:** keep `-le` (fail on equality) **but** state the rationale in the plan
and in the new code comment: equality is treated as a differential FAIL because for a
≥80%-properly-paired WGBS library zero net overlap indicates `--include_overlap` is a no-op
(a regression), and the pre-flight gate plus the WGBS-target scoping make a legitimate
zero-overlap library out of scope. Cross-reference the existing pre-flight escape hatch
(lines 155–156 already mention adding `--skip-overlap-differential` for non-canonical BAMs).
This converts a silent inheritance into a documented decision. **Important.**

### 2.3 "No other call site references OVERLAP_THRESHOLD." — Confirmed.

Grep of the script shows `OVERLAP_THRESHOLD` appears only at lines 503 and 506 (the
computation and its single use), both inside the block being replaced. Dropping it is safe.
The plan correctly lists this as a grep-confirm-during-implementation item; I confirm it now.

### 2.4 "No SPEC change required." — FALSE. This is the Critical finding.

The plan states (lines 50–52): *"No Rust source, no test changes, no SPEC change required
(SPEC §8.3 does not pin the 5% magnitude; if it does on re-read, a one-line SPEC note will be
added in the same PR — to verify during implementation)."*

**SPEC §8.3 DOES pin the 5% magnitude, in normative prose.** Line 766:

> - `overlap`: M-bias data **count-sum** (`sum(methylated + unmethylated)` across all R2 data
>   rows) > D's same metric **by ≥ 5%**.

This is the spec defining the differential contract. If the gate changes to `strictly > D`
but the SPEC keeps `≥ 5%`, the implementation diverges from its own normative spec — the
precise anti-pattern the plan's Context section invokes ("when the matrix asserts a value
wrong for the actual data, fix the assertion"). The fix must edit line 766 to read
`> D's same metric (strictly; the +5% magnitude was over-specified — the bump scales with
mate-overlap-base fraction, a per-library property)` or equivalent.

The plan's own escape clause ("if it does on re-read, a one-line SPEC note will be added")
means the plan is **not wrong about the remedy**, only about the premise. But because the
plan's Scope section asserts "no SPEC change required" as a stated fact, an implementer
following it literally could skip the SPEC edit and ship a self-contradicting spec. **Promote
the SPEC §8.3 line-766 edit from a conditional footnote to an explicit, mandatory task in the
implementation outline.** This is **Critical** for plan correctness even though the code edit
is trivial.

(Note: line 766 is the only normative `5%` in the spec; the surrounding bullets at 764–768
describe the metric and fail-closed semantics, which remain valid. A single-line edit
suffices.)

---

## 3. Efficiency

N/A for runtime. Dropping the `OVERLAP_THRESHOLD=$(( D_COUNTS * 105 / 100 ))` arithmetic
removes one integer multiply/divide — immeasurable. No effect on the 2–2.5 h matrix runtime,
which is dominated by the 10 smoke subprocess invocations. No concern.

---

## 4. Validation sufficiency

### 4.1 Stage A (instant re-evaluation against preserved outputs) — logically sound *as a math check*, NOT a full gate substitute.

Stage A re-evaluates `192423276 > 188123599` against outputs already on disk and reasons that
since the byte-identity cells cannot change, only the verdict-math constant changed, so the
new exit code must be 3. **As a logic check this is sound** — it correctly proves the
recalibration produces the intended PASS for this dataset.

**What Stage A misses that Stage B catches:**

1. **The edited code never executes.** Stage A is a manual arithmetic re-derivation by the
   reviewer/implementer; it does **not** run the modified `phase_h_pe_matrix.sh`. A typo in
   the new block (e.g., `-ge` instead of `-le`, a transposed variable, a broken `if/else`
   that flips fail-closed to fail-open, a stray character that makes the `[[ ]]` test always
   true) would pass Stage A's mental math but be caught only by Stage B actually running the
   script. **This is the single biggest gap.** For a fail-closed gate the most dangerous
   regression is a fail-*open* one (gate says PASS when it should FAIL), and Stage A cannot
   detect it because it doesn't exercise the code path.
2. **The wording-string edits are unverified.** Stage A does not regenerate `speedup_table.md`
   or `matrix_verdict.txt`, so a missed/garbled `5%` string (see §1.3) would ship.
3. **`set -e` / pipefail interactions.** The new block runs under `set -euo pipefail`. Stage A
   cannot confirm the edited block doesn't introduce an unguarded non-zero exit that aborts
   the script before the verdict is written.

**Mitigation (Important):** Stage A should be upgraded from "manual arithmetic" to "run the
modified differential block against the preserved M-bias.txt files." Concretely, the
implementer can run the script's `count_mbias_rows`/`sum_mbias_counts`/differential logic
against `~/phase_h_pe_release_v879fix/cell_p1_*/rust/*M-bias.txt` — either by pointing a fresh
matrix invocation at the preserved BAM with the smoke results short-circuited, or (simpler) by
extracting the differential block into a throwaway harness fed the preserved files. Even
better: add a **shellcheck** pass and a tiny **unit test** that feeds two synthetic M-bias
files (one with count-sum == D, one with count-sum == D+1) and asserts FAIL/PASS respectively
— this directly guards the `-le` boundary and runs in milliseconds with zero colossal time.

### 4.2 Stage B (fresh 2.5 h re-run) — necessary and correctly specified.

Stage B is the canonical record and exercises the real code end-to-end. The plan's command is
correct (fresh `--out` dir, honors the empty-dir pre-flight). Expected exit 3 is right given
the known perf miss. **The plan's open question — "is Stage A an acceptable substitute for the
v1.0 gate record?" — should be answered: NO for the release record, YES as a fast pre-check.**
The RELEASE_CHECKLIST language ("re-run on a fresh `--out` dir") is normative for the gate
artifact; Stage A is a developer convenience to fail fast before spending 2.5 h. Run both:
Stage A (upgraded to actually execute the block) first to catch typos in seconds, Stage B to
produce the tagged evidence.

### 4.3 Missing validation: equality-boundary test.

Neither stage exercises the `count-sum == D` boundary (§2.2), because the real dataset is at
+2.28% (comfortably `> D`). The `-le` vs `-lt` decision is therefore **never tested on real or
synthetic data** by the proposed validation. If the team adopts the synthetic-unit-test
suggestion in §4.1, add the `== D` case explicitly. Otherwise the boundary behavior is
asserted only by code reading. **Important** given that the boundary is the one place the new
invariant could mis-fire.

---

## 5. Alternatives

| Alternative | Trade-off vs plan's `strictly > D` |
|---|---|
| **Small epsilon floor** (e.g. `> D + 0.1%` or `> D + 1000` counts) | Guards against a near-zero-overlap library producing a spuriously-tight PASS, but reintroduces a magic magnitude — the exact over-specification the plan is removing. The +2.28% case shows even a long-insert WGBS library clears `> D` by millions of counts, so any epsilon below ~1% is indistinguishable from strict-`>` in practice and above ~1% risks the same misfire. **Not worth it** — strict-`>` is cleaner. |
| **`--skip-overlap-differential` flag** | Already foreshadowed in the pre-flight error text (lines 155–156). Useful as an *escape hatch* for genuinely-disjoint libraries (the §2.2 pathological case), orthogonal to this recalibration. **Recommend tracking as the follow-up the code already advertises**, but it is NOT a substitute for fixing the threshold — without the strict-`>` fix the default WGBS path still misfires. Could be bundled, but increases scope; defer is fine. |
| **Make threshold configurable** (`--overlap-min-bump PCT`, default 0) | Most flexible, but adds a CLI surface and a default-0 knob that nobody will tune; YAGNI for a release-gate harness. The per-library variability argues *against* a fixed configurable number and *for* the parameter-free monotonic invariant. **Reject.** |
| **Compare overlap cell against itself across N (already done) and rely solely on Perl-vs-Rust byte-cmp** (drop the differential entirely) | The differential exists as a *semantic* guard that `--include_overlap` actually took effect (catches a flag-parsing regression where the flag is silently ignored and the cell accidentally byte-matches D's no_overlap output... which would NOT byte-match the overlap Perl output, so byte-cmp already catches it). Arguably the Perl-vs-Rust byte-cmp on the overlap cell **already** guarantees correctness, making the differential partially redundant. **However**, the differential adds a human-readable "overlap is doing something" signal in the verdict and guards the polarity direction (C.1). Keeping it as `strictly > D` is the right minimal change; dropping it entirely is a larger scope decision out of band for this PR. |

The plan's choice (parameter-free `strictly > D`) is the best of these for a release gate.

---

## 6. Action items (prioritized)

### Critical
1. **Add a mandatory SPEC §8.3 edit task.** Line 766 normatively states `> D's same metric by
   ≥ 5%`. The plan's "no SPEC change required" premise is factually wrong. Promote the SPEC
   edit from a conditional footnote to an explicit implementation task: change line 766 to
   `strictly > D` with the per-library-magnitude rationale. Without this the spec and gate
   contradict each other. (Plan lines 50–52.)

### Important
2. **Enumerate line 501 in the wording-update list.** The inline comment `# overlap count-sum
   > D + 5% (rev 1 A-O3)` directly above the assertion still says `+5%`; the plan's outline
   replaces only 502–511 and must explicitly delete/replace 501 to avoid a contradictory
   orphaned comment. (Plan §1, lines 67–84.)
3. **Make the `-le` (fail-on-equality) boundary an explicit, documented decision**, not a
   silent inheritance. State in the plan and the new code comment that `count-sum == D` is
   treated as FAIL because zero net overlap on a ≥80%-properly-paired WGBS library indicates
   `--include_overlap` is a no-op (a regression), and that genuinely-disjoint libraries are
   out of scope (use the advertised `--skip-overlap-differential` escape hatch). (Plan §2.2 /
   review §2.2.)
4. **Upgrade Stage A from manual arithmetic to actually executing the edited block** against
   the preserved M-bias files (or add a millisecond synthetic-input unit test of the
   differential function). Manual math cannot catch a typo that flips fail-closed to
   fail-open — the worst regression class for this gate. Add a `count-sum == D` synthetic case
   to test the boundary that no real-data stage exercises. Answer the plan's open question:
   Stage A is a fast pre-check, NOT a substitute for the Stage B gate artifact. (Plan §4.1 /
   review §4.)

### Optional
5. **Mirror the `2>/dev/null` on the new `[[ -le ]]` test** for symmetry with the three
   row-count asserts (lines 489/493/497) and defense against a future weakening of the outer
   guard. Safe to omit today because the outer `-n && -gt 0` guard proves integer operands.
   (Review §1.2.)
6. **Confirm line 700 (emitter continuation) carries no `5%` semantics** before deciding it
   needs no edit; the plan references only 699. (Review §1.3.)
7. **Consider bundling or explicitly deferring `--skip-overlap-differential`** — it is already
   advertised in the pre-flight error text (lines 155–156) and is the proper handling for the
   §2.2 zero-overlap pathological library. Not required for this PR; note the tracking issue.

---

## Most important findings (top 3)

1. **Critical — the "no SPEC change required" premise is wrong.** SPEC §8.3 line 766
   normatively pins `≥ 5%`; the PR must edit it or the spec contradicts the gate.
2. **Important — `strictly > D` is correct, but the `-le` equality boundary is an undocumented
   design call.** The 80%-properly-paired gate makes overlap likely, not guaranteed; a
   long-insert library can legally hit `count-sum == D`. Keep `-le` but document the rationale.
3. **Important — Stage A doesn't execute the edited code.** It's a manual math re-derivation
   that cannot catch a typo flipping fail-closed to fail-open. Add a synthetic-input unit test
   (including the `== D` boundary) and treat Stage A as a pre-check, not the gate record.
