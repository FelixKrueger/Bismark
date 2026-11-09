# Code Review B — #1104 simplex 5-Base consensus output

**Branch:** `1104-simplex-consensus` (base `dev` @ `53744d5`)
**Commits:** `50f1eef` (step 0: deterministic duplex emission order), `a80aa3a` (the feature)
**Reviewer:** B (independent, fresh context)
**Date:** 2026-08-15

## Verdict: APPROVE

No correctness defect reachable from any static input. One hardening gap in the new
fail-loud machinery (M1 — **since fixed by Reviewer A**, and I concur with their fix),
one vacuous test gate (M2 — **fixed by me, sabotage-verified**), a handful of Low
items — none blocks merge.

> **Note on tree state.** This review was written against the two committed commits;
> while it was in progress Reviewer A applied an uncommitted fix to
> `SimplexLedger::finish` (residual = `expected.len() - done.len()`) plus a unit test
> `ledger_reports_a_family_that_never_arrived`, and a new integration test
> `histogram_buckets_deeper_families_and_clamps_at_five`. All verification figures
> below are re-run against the current tree, which includes those changes and mine.

Default (`duplex`) byte-identity holds by construction: the tag call is gated on
`mx.is_some()`, the default path passes `None` everywhere, and every default-mode string
literal is byte-identical to the pre-diff text (verified literal-by-literal, including
the `\`-continuation reflows in the summary line and the standalone `Note:`).

## Verification (re-run, not trusted; each asserted on its own exit code)

| Command | Result |
|---|---|
| `cargo fmt -p bismark -- --check` | exit 0 — clean (incl. my test edit) |
| `cargo clippy -p bismark --all-targets` | exit 0 — clean (default features) |
| `cargo test -p bismark --lib` | **1503 passed, 0 failed** (1502 + Reviewer A's new test) |
| `cargo test -p bismark --test aligner_five_base_simplex` | **10 passed, 0 failed** |
| `cargo test -p bismark --test aligner_five_base_groundtruth` | 7 passed, 0 failed (unchanged) |
| `cargo test -p bismark --test aligner_five_base_bisulfite` | 11 passed, 0 failed (unchanged) |
| `cargo test -p bismark` (whole crate) | **exit 0 — 2215 passed, 0 failed, 20 ignored, 82 suites** |

Two process notes, since they nearly produced false confidence:

- A transient `FAILED` in the simplex suite (0.12 s, 9 passed / 1 failed) was a **shared-tree
  build race**, not a defect: these tests exec `target/debug/bismark` as a subprocess, and a
  concurrent reviewer's `cargo` invocation was relinking that binary. Re-running gave 10/10.
  Three agents share this checkout; treat any single anomalous integration failure here as
  suspect until re-run.
- My first full-suite attempt reported exit 0 while having run only ~40 lines: the pipeline
  ended in `| head -40`, so `head` closed the pipe, `grep` took SIGPIPE, and the **pipeline's
  exit status was `head`'s**. A truncated run that looks green is worse than a red one — the
  documented `head` pitfall, hit live.

## Adversarial checks performed (per the caller's list)

1. **`emit_family` behaviour preservation (duplex path).** Verified line-against-line
   vs the pre-diff inline body: the three skip guards (`sq_order`, `genome.get`,
   `end <= start`) map to `Ok(false)` and the caller's `skipped += 1` exactly as the
   old `continue`s did; `wrote_any` accounting identical; the FLAG `0x10` branch keeps
   the same revcomp-seq/reverse-qual handoff into `five_base_emit_record`; qname
   `format!("{qname_prefix}:{chrom}:{start}-{end}:{umi_str}")` with `"dpx"` reproduces
   the old literal. `set_mx` is called only inside `if let Some(m) = mx`, and the
   default mode passes `mx = None` (duplex loop: `spx_path.is_some().then_some(2)` —
   `None` when `EmitPaths::Duplex`). Byte-identity claim stands.
2. **`SimplexLedger`.** Three error directions reachable and tested (unknown key,
   done-set re-arrival, in-flight residual at `finish`). No double-emit is possible:
   completion removes the key from `arrived` and inserts into `done` atomically in the
   same call; only the completion path returns `Some(fam)`. `arrive_with`'s
   mutate-before-check is sound: a duplicated input (same BAM listed twice) inflates
   PASS 1's expected count identically, so the passes stay consistent; a genuine
   post-completion arrival hits the `done` check *before* the mutator runs. One gap:
   the **zero-arrival direction** (finding M1 below).
3. **Own-strand rule.** `fam.ob.is_empty()` is a correct discriminator: a family with
   members in both vectors would have had `ot > 0 && ob > 0` in PASS 1 (same `key_of`,
   same records) and be in `paired`, never in the ledger. Both-empty is impossible —
   the ledger yields a family only after ≥1 arrival, and every arrival pushes into one
   vector. The only evasion is a mid-run input mutation that flips a record's strand
   while keeping the family's total count — the same theoretical class the ledger's
   errors guard, and not worth tracking per-strand expected counts for (noted, L6).
4. **PASS 2 storage rule.** Default mode is record-for-record identical to the old
   code: `is_paired_fam && dpx_path.is_none()` never fires (dpx present),
   `!is_paired_fam && spx_path.is_none()` reproduces the old `!paired.contains →
   continue`. Both skips fire *before* the CIGAR walk, so simplex mode does no wasted
   covered-map work for paired families. Counters: each ledger completion calls
   `emit_family` exactly once and increments exactly one of `emitted_spx`/`skipped_spx`,
   so `emitted + skipped == expected_len` on any clean run.
5. **Histogram.** `size_hist[(n as usize).min(5) - 1]` cannot see `n == 0`: a `counts`
   entry is created by `or_default()` followed by an unconditional increment (PASS 1,
   mod.rs:2549-2554), so `ot + ob >= 1`. Buckets 1/2/3/4/≥5 over read counts; built in
   the PASS-1 sweep, so it covers all simplex families including later-skipped ones.
   The invariant lives two loops away from the subtraction (L7).
6. **Determinism.** `CKey` is exactly `{ref_id, start, end, umi_hash}` (mod.rs:2457-2462)
   — the sort key IS the full key, so the order is total and two *distinct* families
   cannot collide on all four (they would be the same key and one family).
   `sort_unstable` is safe with a total order over unique keys. Two different UMIs
   colliding in FNV-64 merge into one family — pre-existing duplex behaviour, not new.
7. **`set_mx`.** Mirrors `set_rx` (same `data_mut().insert` pattern), lowercase `mx` =
   SAM local-use space, `Value::from(i32)`. Workspace grep: no other `mx`/`MX` tag
   anywhere; extractor consumers parse XR/XG/XM only. Inert downstream.
8. **Validation placement.** All dispatch routes covered: (a) the bisulfite standalone
   is rejected at mod.rs:178 *before* its dispatch (and before the consensus dispatch,
   with the mutual-exclusion check above both); (b) `--five_base_consensus_from_bam`
   legitimately accepts the flag by construction; (c) every other route reaches
   `resolve()` and its guard (config.rs:769). No further pre-`resolve()` dispatches
   exist in `run()` (checked lines 165-197). SE input cannot reach the consensus with
   the flag silently ignored: `--illumina_5base` is PE-only at resolve, and the
   standalone PE-probes (mod.rs:563-579).
9. **Test quality.** The 9 integration tests assert real behaviour: exact XM strings
   for both simplex orientations, exact record counts, exact histogram substrings,
   exact error messages, file presence/absence per mode, and a two-run byte gate for
   step 0. Two gates were sabotage-verified by the implementer (sort removal;
   both-flags emission) — I verified the assertions they hit are the ones that would
   fail. Gates that were NOT sabotage-verified and are weaker than they look: M2
   (genuinely vacuous), L4, L5.

## Findings

### Medium

**M1. `SimplexLedger::finish()` misses the zero-arrival direction. — RESOLVED by
Reviewer A; I concur.**
Reviewer A applied exactly this change independently (`residual = expected.len() -
done.len()`) plus the missing unit test. **I agree with the fix and confirm it is
sound:** `done ⊆ expected` is an invariant — a key enters `done` only on the path where
`expected.get(&key)` already succeeded, and `expected` is never drained — so the
subtraction cannot underflow. Two reviewers reaching the same patch independently is
the strongest signal available that the gap was real. Original finding preserved below
for the record.
`five_base_duplex.rs` — `finish()` errors only when `arrived` (in-flight, partially
filled families) is non-empty. A family present in `expected` whose records *all*
vanish between PASS 1 and PASS 2 never creates an `arrived` entry, so `finish()`
returns `Ok(())` and the family is silently never emitted; the only trace is a
report line whose `emitted + skipped` no longer equals the printed family total —
which nothing machine-checks. The plan's own contract (§3.8: "Residual families at
EOF (arrived < expected): hard error"; §8.5: "every violation direction now fails
loud") includes `arrived == 0`. Unreachable without mid-run input mutation — the same
precondition as the three directions that *were* implemented, which is exactly why
this one should not be the odd one out. Suggested fix (all existing unit tests keep
passing — `done ⊆ expected`, so no underflow; the residual test's expected 2 families
have `done.len() == 0` → still `Residual(2)`):

```rust
pub fn finish(self) -> Result<(), LedgerError> {
    // done ⊆ expected; anything expected and not done is incomplete,
    // including families that never arrived at all.
    let missing = self.expected.len() - self.done.len();
    if missing == 0 { Ok(()) } else { Err(LedgerError::Residual(missing)) }
}
```

plus one unit test (`SimplexLedger::new([(7, 2)]) → finish() == Err(Residual(1))`).
(Reviewer A's landed patch is this patch.)

**M2. Duplex emission in `both` mode was not gated by any test. — FIXED by me,
sabotage-verified.**
`aligner_five_base_simplex.rs:330-335` — the `mx:i:2` check in
`both_mode_emits_one_own_strand_record_per_simplex_family` is
`dpx.lines().filter(|l| !l.starts_with('@')).all(|l| l.contains("mx:i:2"))`, and
`.all()` on an **empty** iterator is `true`. No test asserts the duplex BAM's record
count in `both` mode (the `single_fragment...` test asserts it is header-only, which a
broken run also satisfies).

**Sabotage-verified, both directions.** I changed the PASS-2 storage rule from
`is_paired_fam && dpx_path.is_none()` to `is_paired_fam && spx_path.is_some()` — which
drops every duplex family in `both` mode while leaving the default path untouched:

- **Before my fix:** the whole committed suite still passed. Default mode is unaffected,
  every simplex assertion holds, and the `mx:i:2` `.all()` ran over an empty iterator.
  A regression that silently empties the duplex BAM in `both` mode was invisible.
- **After my fix:** the run fails at the new assertion (`left: 0, right: 2`), printing
  the header-only duplex BAM. Exactly one assertion failed — mine — which is the direct
  measurement that nothing else in the suite covered this.

`mod.rs` was restored from a scratchpad `command cp -f` backup (never `git checkout --`)
and confirmed byte-identical to HEAD; the suite is back to 10/10 and `cargo fmt` is clean.

Applied fix, in `both_mode_emits_one_own_strand_record_per_simplex_family`:

```rust
// The count first: `.all()` holds vacuously on a header-only BAM, so without this
// a `both` run that dropped every duplex family would still pass the tag check.
let dpx_recs: Vec<&str> = dpx.lines().filter(|l| !l.starts_with('@')).collect();
assert_eq!(
    dpx_recs.len(),
    2,
    "the window-0 duplex family emits both records in `both` mode; got:\n{dpx}"
);
assert!(
    dpx_recs.iter().all(|l| l.contains("mx:i:2")),
    "every duplex record is tagged mx:i:2 in a non-default mode; got:\n{dpx}"
);
```

### Low

**L1. Plan V7's in-run half of the mode matrix is untested.** Every integration test
drives `--five_base_consensus_from_bam`; the in-run branch (mod.rs:1850-1881 — the
`_pe.5base_simplex.bam` suffix derivation and its `EmitPaths` match) is exercised by
no test in any mode but default (`aligner_five_base_groundtruth.rs` runs duplex only).
The shared `run_five_base_consensus` core keeps the risk small, but a wrong suffix
literal in the in-run derive would ship unnoticed. A minimap2-gated `both`-mode
variant of the existing groundtruth consensus test would close it.

**L2. Plan V7's cross-check against the duplex TXT `singletons` figure is not
implemented.** The count identity is asserted within the simplex report line only
(`simplex_mode_writes_only_the_simplex_bam`); no test runs `--five_base_duplex`'s TXT
pass alongside and compares. Plan-coverage item, not a code defect.

**L3. `simplex` mode reports nothing about the withheld duplex families.** Plan §3.5
says duplex families are "counted ... (their counts for the report come from PASS 1)",
but in `Simplex` mode neither report line mentions `paired.len()`, so a user cannot
see how much duplex signal they opted out of. One figure in the simplex line would do.

**L4. The 10th test's XM assertion is tautological on the record it checks.**
`simplex_never_calls_a_strand_it_did_not_sequence` asserts `xm[12] == b'.'` on the
emitted FLAG-0 record — a CT-call record can never call at a genomic `G`, so that
assertion passes under any implementation. The operative gates in the test are
`recs.len() == 1` and the FLAG-0 check (a sabotaged both-flags emission fails the
count first, never reaching the XM loop). Fine as documentation; not the contamination
proof the §11b note describes.

**L5. `default_mode_output_is_unchanged_and_untagged` is an invariant proxy, not a
byte gate.** It asserts absence (no simplex BAM, no `mx:i:`, no `spx:`, duplex-only
stderr) rather than comparing against a step-0 baseline binary — V1b's binary-vs-binary
comparison was necessarily a one-off. A wording drift after `"5-Base duplex consensus
(PE):"` would pass it; content drift is caught by the unchanged groundtruth suite.
Acceptable; stating it so nobody mistakes it for the V1b gate.

**L6. Per-strand expected counts are not tracked.** A mid-run mutation that flips one
record's strand while preserving the family's total evades all four error directions
and emits the wrong-strand record. Same theoretical class as M1's precondition;
tracking `(ot, ob)` expected pairs is not worth the memory. Recorded as accepted risk.

**L7. Histogram underflow guard is remote.** `(n as usize).min(5) - 1` panics on
`n == 0`; the `n >= 1` invariant is established two loops away in PASS 1. A
`debug_assert!(n >= 1)` beside the subtraction would pin it against future PASS-1
refactors. Cosmetic. (Reviewer A's `histogram_buckets_deeper_families_and_clamps_at_five`
now covers the upper clamp — the `- 1` underflow direction remains unreachable-by-invariant
rather than asserted.)

**L9. Broken docs anchor. — FIXED by me.**
`docs/src/content/docs/rust/illumina-5-base.md:120` linked
`[`--five_base_deconvolution`](#flags)`, but that page has no `Flags` heading (its
headings are Stability contract / Running it / Advanced modes / Simplex molecules /
Interop / Validation) and `#flags` appears nowhere in it on `dev` — the diff introduced
a dead link, in the caution admonition that is the whole point of the simplex caveat.
Repointed to `#advanced-modes`, where `--five_base_deconvolution` is actually
documented (line 74). One-word change, no other docs text touched.

**L8. §11b tallies were off by one.** As committed, the new integration suite was 9 tests
(the note says "10/10") and `five_base_duplex.rs` gained 8 unit tests (not 9) — verified by
`grep -c '#\[test\]'`. The named "10th test" does exist; only the counts were wrong. With
both reviewers' additions the suite is now genuinely 10, so §11b should be refreshed rather
than corrected: **10 integration tests, 9 ledger/consensus unit tests, 1503 lib tests.**

## Also verified (no findings)

- **Step 0 is minimal and honest:** 5 lines in mod.rs + test + one CHANGELOG sentence;
  the sort key equals the full `CKey`.
- **Writers up front** — header-only BAMs on empty input, per mode; tested
  (`single_fragment...` asserts the header-only duplex BAM in `both` mode).
- **`expect()` on `spx_writer`** at mod.rs:2800-2802 is invariant-sound: the ledger is
  non-empty only when `spx_path.is_some()`, which is exactly when `spx_writer` exists.
- **CLI/clap:** `ValueEnum` kebab-case gives `duplex|simplex|both`; `default_value =
  "duplex"` round-trips; `as_str()` matches the CLI spellings.
- **CHANGELOG/docs/README** match the repo's established long-form style; the docs
  page carries the caveat, the `--five_base_deconvolution` pairing, the mx-vs-XM
  disambiguation, and the read-counts-not-fragments note, as planned.
- **`config.rs` guard fires before genome discovery** — the guard test asserts the
  exact message and passes with a nonexistent genome/read set.

## Fixes applied by this reviewer

1. **M2** — `rust/bismark/tests/aligner_five_base_simplex.rs`: added the duplex
   record-count assertion to `both_mode_emits_one_own_strand_record_per_simplex_family`,
   ahead of the vacuity-prone `mx:i:2` `.all()`. Test-only, additive, sabotage-verified
   in both directions (see M2). Suite 10/10, `cargo fmt` clean.
2. **L9** — `docs/src/content/docs/rust/illumina-5-base.md:120`: repointed the dead
   `#flags` anchor to `#advanced-modes`.

Nothing in `src/` was changed by me. M1's production fix was landed by Reviewer A and
I concur with it.

## Recommendations by priority

| Priority | Item | Status |
|---|---|---|
| Medium | M2 — duplex record count in the `both`-mode test | **fixed by me** |
| Medium | M1 — zero-arrival hole in `SimplexLedger::finish()` | **fixed by Reviewer A; concurred** |
| Low | L9 — dead `#flags` docs anchor | **fixed by me** |
| Low | L1 — one minimap2-gated in-run `both`-mode test | open |
| Low | L3 — surface `paired.len()` in the `simplex`-mode report line | open |
| Low | L8 — refresh §11b's test tallies (10 / 9 / 1503) | open |
| Low | L2, L4, L5, L6, L7 — as described; none blocks | open |

**Nothing here blocks merge.** The two Medium items are both closed in the working tree;
Reviewer A's `finish()` fix and my test assertion are **uncommitted** and need folding
into the branch before it merges.

## Full-suite confirmation — GREEN

`cargo test -p bismark` (whole crate) exceeds the 10-minute foreground cap on this
contended shared checkout, so it was run **detached and unpiped** (no `head`, so the
exit status is cargo's own, unlike the truncated first attempt described above):

```
FULL_EXIT=0
82 suites reporting "test result: ok"
2215 passed; 0 failed; 20 ignored
no "test result: FAILED" and no error[…]/error: lines
```

Verified by counting the log's own result lines rather than trusting the exit code alone.

The run was launched **after** the sabotage was reverted and `mod.rs` confirmed identical
to HEAD, so it covers the current tree: Reviewer A's `finish()` fix + my `both`-mode
assertion + unmodified production code. Component figures within it:

- **1503/1503 lib unit tests** — includes every `SimplexLedger` test and both reviewers'
  additions.
- **10/10 `aligner_five_base_simplex`** (the new feature suite), **7/7
  `aligner_five_base_groundtruth`**, **11/11 `aligner_five_base_bisulfite`**.
- **clippy `--all-targets`** and **fmt** clean (re-checked after my test edit).

Nothing outstanding on the verification side. The only pre-merge action is mechanical:
Reviewer A's `finish()` fix and my two edits are **uncommitted** and need folding into
the branch.
