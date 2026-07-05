# Phase 2 PLAN_REVIEW_A — `--drach` / `--m6A` (DRACH-motif m6A filtering)

**Reviewer:** Plan Reviewer A (independent; no shared state with Reviewer B).
**Plan reviewed:** `phase2-drach-m6a/PLAN.md` (rev 1, 2026-05-31).
**Method:** full read of the plan + the Perl DRACH path (`coverage2cytosine` v0.25.1 `:38-42`, `:1075-1383`, `:2028`, `:2174-2194`), the shipped Rust crate (`cli.rs`, `cov.rs`, `report.rs`, `gpc.rs`, `lib.rs`), **plus 14 live-Perl fixture runs** built from first principles in an isolated tmp worktree to verify the byte-identity claims (not taken on faith).

---

## Top-line verdict: **APPROVE-WITH-CHANGES**

The plan is high quality and the byte-identity claims I could test all held up against live Perl — **except one real gap**: the plan frames the chromosome-start negative-`substr` wrap as a **bottom-strand-only** concern (§3.3/§3.6/V7/V10/Assumption 15), but I proved with live Perl that the **TOP strand also has a negative-`substr` wrap in the `drach_top` 5-mer extraction at `pos<4`, and it EMITS a line** (`ACAAA` → `chrA 2 + 9 1 AA CAA`). The Rust top-strand `drach` extraction therefore MUST use `perl_substr` (not naive slicing, which would panic on `usize` underflow or, if clamped, produce the wrong prefix). This is the one Critical fix; the rest are Important framing/clarity items.

No Critical *logic* error in the parts the plan does cover — the AC/GT arithmetic, the filter, the `pos`/`pos-1` anchors, the top-then-bottom ordering, the threshold auto-set, the `--zero_based`-ignored / no-drach-mutex / raw-`-o` / `.chrchr1` claims are all **verified byte-identical**.

---

## Live-Perl checks I ran (fixture → result)

| # | Claim tested | Fixture | Live-Perl result | Verdict |
|---|--------------|---------|------------------|---------|
| L1 | Top/bottom arithmetic + filter | `chr1=AAATGTTCAAAGTACGTACGT`, cov@{5,12,15,16,19,20} | only `chr1 5 - 3 3 GAACA CAT` emitted; my hand-trace of every AC/GT match reproduced it exactly | ✅ §3.2/§3.3 correct |
| L2 | Threshold auto-set to 1 | same | STDERR "setting coverage threshold to 1"; threshold-1 behaviour | ✅ §3.0.4 |
| L3 | Explicit `--coverage_threshold 5` survives | `GGAACTGGGAAATGTTCAAA` cov@{5(8),14(2)} | pos14 (cov 2) skipped, pos5 (cov 8) emitted; "5 (user defined)" | ✅ §3.2.7 |
| L4 | `--drach --coverage_threshold 0` | same | **DIES** (EXIT 255, "must be a positive integer") — generic check fires before auto-set | ✅ current Rust `validate()` already rejects `Some(0)` |
| L5 | `--zero_based` ignored | `test.cov` | `--drach --zero_based` report+cov **byte-identical** to `--drach` | ✅ §3.0.3 |
| L6 | No `--drach`×`--CX` mutex | `--drach --CX` | EXIT 0; only DRACH output; identical to plain `--drach` | ✅ §3.0.2 |
| L7 | No `--drach`×`--merge_CpGs` mutex | `--drach --merge_CpGs` | EXIT 0; only DRACH output | ✅ §3.0.2 |
| L8 | **General mutexes still fire with `--drach`** | `--drach --CX --merge_CpGs`; `--drach --merge_CpGs --coverage_threshold 5` | **BOTH DIE** (EXIT 255) — the CX×merge and threshold×merge mutexes run in `process_commandline` before the early exit | ⚠️ plan silent on this (Important — see F2) |
| L9 | Raw-`-o` (no strip) filenames | `-o sample` | `sample_DRACH_report.txt` / `sample_DRACH.cov` | ✅ §3.1 (and confirmed the strip is on a local copy `$cytosine_report_file`, never on `$cytosine_out`) |
| L10 | `.chrchr1` doubling + gzip + split | 2-chr genome (`chr1`,`contig2`), split+gzip | `sample.chrchr1_DRACH_report.txt.gz`, `sample.chrcontig2_…` | ✅ §3.1 |
| L11 | Top-then-bottom ordering within a chr | `GGAACTGGGAAATGTTCAAA` (top@5 + bottom@14) | `+` line (pos5) printed **before** `-` line (pos14); cov same order | ✅ §3.5.2 |
| L12 | Uncovered motif threshold-skip + cov pct recomputed | pos5 uncovered, pos14 cov | only pos14 emitted; pct `75.000000` = 6/8 recomputed (col-4 ignored) | ✅ §3.0.4 / §3.4 / Q3 |
| L13 | Empty cov → empty files | `: > empty.cov` | EXIT 0, **both files 0 bytes**; STDERR warns "uninitialized $last_chr" (exempt) | ✅ §3.5.5/Q2 — but see F3 (no-last_chr-to-flush guard) |
| L14 | Bottom-strand truncated-5-mer EMITS | `chrT=AAAGTC` cov@4 | `chrT 4 - 5 0 GACT CTT` — drach='GACT' (4-byte truncated), p5 missing → passes, emits at `pos-1=4` | ✅ confirms the truncated-pass path is **live & reachable** on the bottom strand |
| **L15** | **TOP-strand `pos<4` negative-`substr` wrap EMITS** | `chrA=ACAAA` cov@2 | **`chrA 2 + 9 1 AA CAA`** — `drach_top=substr(-2,5)` wraps from the string end → `'AA'`, passes filter, **emits** | ❌ **plan omits the top-strand wrap (CRITICAL — F1)** |
| L16 | Bottom-strand `pos<4` wrap never emits | `GTACGTACGT` cov@{1,5,9} | **empty** output — every `pos<4` bottom motif has `tri` len<3 after rc-wrap → len-guard skips | ✅ §3.3.6/V7 (but V7's expected = "empty/no-panic", not a byte-identical *line* — see F5) |

(Brute-force enumeration over all ACGT sequences of len 4–10 confirmed: bottom-strand `pos<4` matches **always** have `tri` len<3 → never emit; top-strand `pos<4` matches with a full `tri` **do** emit — 24 distinct cases in len 5–7.)

---

## Findings by area

### Logic

**F1 — CRITICAL: the top strand also has a `pos<4` negative-`substr` wrap, and it emits — the plan only handles the bottom strand.**
The plan's §3.2 describes `drach_top = substr(seq, pos-4, 5)` as "`seq[pos-4 .. pos+1]`" — a positive-offset slice. But for an `AC` near the chromosome start (`pos=2` or `pos=3`), the offset `pos-4` is **negative** (`-2`/`-1`), and Perl's `substr` wraps it from the end of the string. The top-strand `tri_nt = substr(pos-1, 3)` stays **positive** (so it can be full length ≥3 and clear the len-guard), so unlike the bottom strand the top-strand `pos<4` case **does emit a real line** whose reported `drach_5mer` column is the wrapped fragment. Verified on live Perl: `ACAAA` (cov@2) → `chrA 2 + 9 1 AA CAA` (the `AA` is `substr("ACAAA", -2, 5)`).
- **Risk:** if the implementer codes the top `drach` as `&seq[pos-4 .. pos+1]`, the `pos-4` index underflows `usize` → **panic**; if "fixed" by clamping to 0 it yields `seq[0..]` (e.g. `ACA`) ≠ Perl's end-wrapped `AA` → **byte-identity divergence** in both the report `drach` column and the keep/skip decision.
- **Action:** §3.2 step 2/3 must extract BOTH `tri_nt_top` and `drach_top` via the Phase-B `perl_substr` helper (offset as `isize`), exactly as §3.3 step 6 already mandates for the bottom strand. Add an explicit top-strand `pos<4` golden (e.g. `ACAAA`) — see F6/V-new. (`report::perl_substr(seq, offset: isize, want: usize)` at `report.rs:99` is the right tool and already has a negative-wrap test.)

**F2 — IMPORTANT: the plan says "no mutex," but must also state the pre-existing GENERAL mutexes still fire under `--drach`.**
§3.0.2 correctly says not to add a `--drach`-specific mutex (verified: `--drach --CX` and `--drach --merge_CpGs` each exit 0). **However**, Perl's `process_commandline` runs its general mutexes *before* the `:38` early-exit, so they still die with `--drach`: I verified `--drach --CX --merge_CpGs` **dies** (CX×merge, Perl `:2140`) and `--drach --merge_CpGs --coverage_threshold 5` **dies** (threshold×merge, `:2176`). The current Rust `validate()` (cli.rs:169–198) already enforces these and the plan keeps them — so this is *correct by inheritance* — but the plan should **explicitly state that un-rejecting `--drach` must NOT bypass / reorder the existing CX×merge, threshold×merge, nome×merge, and disco-requires-merge checks**, lest a future refactor (e.g. an early `if config.drach { … }` short-circuit inside `validate()`) accidentally skip them. Add a one-line note + a unit assertion that `--drach --merge_CpGs --coverage_threshold 5` still errors.

**F3 — IMPORTANT: empty-cov path needs an explicit "no last_chr to flush" guard.**
§3.5.5/Q2 is **confirmed** (empty cov → two 0-byte files, EXIT 0). But in Perl this works only because `$last_chr` is `undef` and `$chromosomes{""}` is an empty no-match. The Rust driver (§5.6 "flush-on-transition + final flush") must guard the **final flush** against the zero-line case (no `last_chr` was ever set) — otherwise it could `unwrap`/index a non-existent chromosome or emit a phantom flush. The plan should add: "if no cov line was read, open+truncate both output files and write nothing (no genome walk)." V8 covers the observable output but not this internal guard.

### Assumptions

**F4 — OPTIONAL: clarify why the simple `i += 1` AC/GT scan is byte-identical to Perl `/g`.**
§3.2/§3.3 use `seq[i]==b'A' && seq[i+1]==b'C'` with `i` from 0. This is **safe** because `AC` and `GT` are non-self-overlapping 2-mers (the 2nd pattern byte ≠ the 1st), so a `+=1` scan yields the identical match set to Perl's `pos()`-advancing `/(AC)/g` (verified: `ACAC` → pos {2,4}). Worth one sentence so the implementer doesn't copy gpc.rs's `j += 2` "non-overlapping" trick and wonder if it matters here (it doesn't — but stating it avoids a needless divergence-hunt).

**F5 — IMPORTANT: V7's expected result is "empty output + no panic," not a byte-identical emitted line.**
I proved (brute force, all ACGT len 4–10) that a bottom-strand `pos<4` motif **always** has `tri` len<3 after the rc-wrap and is therefore len-guard-skipped — it never emits. So V7 ("chromosome-start bottom motif `pos<4` → byte-identical to Perl, negative-substr wrap") validates that the Rust (a) does **not panic** on the negative offset and (b) emits **nothing**, matching Perl. The plan wording ("byte-identical to Perl") is technically true (empty == empty) but invites the implementer to expect a non-empty golden line. Reword V7 to "no panic; emits nothing (Perl skips via len-guard)." The genuinely emitting wrap case is the **top strand** (F1) — that is where a non-empty golden is needed.

### Efficiency
No issues. Two O(genome) linear scans + O(cov) parse, single-threaded, whole-genome-in-RAM — identical posture to the shipped report/gpc walks. The `gpc.rs` sibling (a second covered-only both-strand motif walk with `HashMap<u32,(u32,u32)>` per-chr buffering, flush-on-transition, top-at-`pos`/bottom-at-`pos-1`, single+split+gz writers) is an almost line-for-line template for `drach.rs` — the "reuse Phase A/B/C" claim is real and the closest model is **`gpc.rs`** (the plan cites Phase B/C/D but should point the implementer at `gpc.rs` as the structural twin).

### Validation sufficiency
The V1–V14 matrix catches most of the high-risk modes (no-mutex L6/L7, threshold L3/L4, raw-`-o` L9, `.chrchr1` L10, ordering L11, uncovered-skip L12, empty L13, bottom truncation L14, `--zero_based` L5, gzip). **Gaps:**
- **F6 — IMPORTANT (new test):** add a **top-strand `pos<4` negative-wrap golden** (e.g. `ACAAA` cov@2 → `chrA 2 + 9 1 AA CAA`). This is the only case that exercises the F1 emitting wrap; without it the highest-risk divergence is untested. V10 ("chromosome-end truncated 5-mer") covers the *end*; V7 covers the *start bottom* (which can't emit) — neither covers the *start top* (which can).
- **F7 — OPTIONAL:** add a bottom-strand truncated-5-mer **emit** golden (the L14 `AAAGTC` case) explicitly under V10. The plan's V10 says "an `AC`/`GT` near the chromosome end" but the *load-bearing* end case is the bottom-strand truncated-pass-AND-emit (L14), which differs from the top end case (which can never emit because a full top `tri` forces a full top `drach`). Pin the bottom-emit one.
- **F8 — OPTIONAL:** the `is_drach_motif` unit (V3) should test **each** position with Perl-`substr`-empty semantics, not just position 5: a 0-byte 5-mer (p1 missing → "" ne C passes, but p2 missing → fails), a 1-byte (`G` → p2 missing → fails), a 2-byte (`GA` → p1 ok, p2 ok, p5 missing → **passes**). The plan's signature comment indexes `byte[0]`/`byte[1]` directly, which would **panic** on a <2-byte slice — the impl must `.get(0)/.get(1)/.get(4)` with None→("" semantics): pos0 None→pass, pos1 None→fail, pos4 None→pass. (End-truncation never produces <4-byte top / <3-byte bottom drach, so <2-byte slices arise only from the F1 start-wrap — but the helper is shared, so it must be correct for all lengths.)

### Alternatives
None material — a faithful Perl port is the right call and the gpc.rs substrate makes it low-risk. One note: the plan could lift the per-chr-buffering + flush + writer scaffold from `gpc.rs` into a tiny shared helper to avoid a third copy (report/gpc/drach), but that is a refactor beyond this phase's byte-identity scope; copying gpc.rs's shape is acceptable and lower-risk for now.

---

## Action items

**Critical**
1. **(F1)** Extract BOTH `tri_nt_top` and `drach_top` via `perl_substr` (isize offset, negative-wrap), not naive slicing — the top strand has a real `pos<4` negative-`substr` wrap that **emits** (`ACAAA` → `chrA 2 + 9 1 AA CAA`). Update §3.2 steps 2–3 + the §4 `drach_top` doc to mirror the bottom-strand `perl_substr` mandate.

**Important**
2. **(F6)** Add a top-strand `pos<4` negative-wrap **golden** (e.g. `ACAAA` cov@2) to §9 — the only test that exercises the F1 emitting wrap.
3. **(F2)** State explicitly in §3.0/§5.1 that un-rejecting `--drach` must NOT bypass the pre-existing general mutexes (CX×merge, threshold×merge, nome×merge, disco-requires-merge); add a unit asserting `--drach --merge_CpGs --coverage_threshold 5` still errors.
4. **(F3)** Add a "no cov line read → open+truncate both files, write nothing, no walk" guard to the §5.6 driver (the empty-cov path that Perl handles via undef `$last_chr`).
5. **(F5)** Reword V7's expected result to "no panic; emits nothing" (bottom `pos<4` is always len-guard-skipped).

**Optional**
6. **(F8)** Broaden V3 / `is_drach_motif` to test 0/1/2-byte slices with `.get()`-based Perl-substr semantics (pos0 None→pass, pos1 None→fail, pos4 None→pass); avoid `byte[0]` indexing that panics on short slices.
7. **(F7)** Pin the bottom-strand truncated-5-mer **emit** case (`AAAGTC` cov@4 → `chrT 4 - 5 0 GACT CTT`) explicitly under V10.
8. **(F4)** One sentence noting AC/GT are non-self-overlapping so an `i += 1` scan == Perl `/g` (no `j += 2` needed).
9. Point the implementer at **`gpc.rs`** as the structural twin (covered-only both-strand motif walk, per-chr buffer/flush, top@`pos`/bottom@`pos-1`, single/split/gz writers) — it is the closest model, more so than the report/merge phases the plan currently cites.

---

## Confirmed-correct (no action)
Q1/`pos-1` bottom anchor (geometry + main-report convention + live `--CX` agreement all hold; Felix's decision stands — faithful Perl port); the filter `p1!=C ∧ p2∈{A,G} ∧ p5!=G`; `pos=i+2` on both strands; `tr/ACTG/TGAC/`+reverse via `revcomp` (report.rs:115, leaves N unchanged); top@`pos`, bottom@`pos-1` reported+looked-up; 7-col report / 6-col cov with `%.6f` recomputed pct (col-4 ignored); **no header line**; raw-`-o` no-strip; `.chrchr1` doubling; top-then-bottom per-chr ordering; covered-only / cov-appearance order (insertion-ordered, never BTreeMap); threshold auto-set (default→1, explicit survives, `0` dies); `--zero_based` ignored; honors `--gzip`/`--split_by_chromosome`; the `cov.rs` parse + `ReportWriter` (pub(crate)) + `perl_substr` + `revcomp` reuse claims are all real in the shipped crate.
