# IMPL_REVIEW_A — HISAT2 `--local` (Reviewer A)

**Target:** `plans/06142026_aligner-hisat2-local/IMPL.md` (TDD task list) cross-checked against `PLAN.md` rev 2 + live source in `/Users/fkrueger/Github/Bismark-hisat2mc/rust/bismark-aligner/`.
**Verdict:** **APPROVE-WITH-CHANGES.** The seam map, the Critical (`score_min_params` MAPQ fix), the option-delta edits, and the no-op plumbing analysis are all source-accurate. **One Critical implementability bug:** the Task 1 RED step asserts `resolve(--hisat2 --local)` is `Ok`, but on the fixture-free path that call cannot be `Ok` — once the reject is lifted it falls through to the "No genome folder specified" validation and returns a *different* `Err`. The RED step as written is unachievable; the assertion must be "errors with something OTHER than the local-reject" (exactly what the prompt's Verify #4 anticipated).

---

## 1. Logic

### Task 2 (🔴 the Critical) — correct and exactly right
- Source confirms `score_min_params` (`options.rs:347-352`) keys on `cli.local` *alone* and hardcodes G-form `(20.0, 8.0)`. For `--hisat2 --local` this returns the wrong constants AND its `prefix = "G,"` would reject a valid user L-form. The proposed signature `score_min_params(cli, aligner)` with `if cli.local && aligner == Bowtie2 { ("G,", (20.0,8.0)) } else { ("L,", (0.0,-0.2)) }` produces the correct three outcomes: Bowtie2-local→G `(20,8)`; HISAT2-local→L `(0,−0.2)`; end-to-end (any aligner)→L `(0,−0.2)`. **Exactly right.**
- **Call site has `aligner` in scope.** The single production caller is `config.rs:360` (`options::score_min_params(cli)?`), inside `resolve`, and `aligner` is a live binding there (it is `match`-ed at `config.rs:347`). Threading `aligner` in compiles.
- **No other callers.** `grep score_min_params` across the crate returns only `options.rs:347` (def), `:516-523` (test), and `config.rs:360` (caller). The signature change breaks nothing else. ✔
- `diff = sc_min.abs()` (`mapq.rs:42`) confirms `calc_mapq`/`calc_mapq_local` are sign-agnostic; for `(0,−0.2)` @ readLen 50, `sc_min = −0.2·ln(50) ≈ −0.782`, `diff ≈ 0.782` (the sub-unity regime rev-2 flagged). **No `calc_mapq` production change needed — IMPL is right.**

### Task 3 (option delta) — correct + complete; the `else`-interaction worry is unfounded
- Local block (`options.rs:82-103`) runs for **all** aligners inside the shared `build_aligner_options`. When `cli.local` is true the end-to-end score-min `else` (`:104-119`) is **not** taken, so there is **no double-push** of `--score-min`. The prompt's Verify-#3 concern ("does the `else` interact wrongly?") is answered NO: `else` only runs when `!cli.local`. Routing HISAT2-local through a new HISAT2 arm of the `:82` block (push L-form, do not push `--local`) yields exactly one `--score-min L,0,-0.2`. ✔
- HISAT2 tail is in `apply_aligner_specific_options(base, cli, aligner)` (`options.rs:288`), which receives `cli`, so `cli.local` is in scope at `:328` for the `--no-softclip` drop. ✔
- Target string verified against the frozen HISAT2 end-to-end literals: end-to-end SE = `-q --score-min L,0,-0.2 --ignore-quals --no-softclip --omit-sec-seq` (`:561`); end-to-end PE = `… --no-mixed --no-discordant --maxins 500 --no-softclip --omit-sec-seq` (`:613`). HISAT2-local = the same minus `--no-softclip` and with **no** `--local`. The two edits produce exactly that. ✔
- **`valid_score_min_l` is the right validator for HISAT2-local.** PLAN table (lines 73, 107-108) + Perl `7907-7913` confirm HISAT2 local AND end-to-end both want **L-form**; only Bowtie 2-local wants G-form. ✔
- Byte-frozen regressions hold: Bowtie 2-local still hits the Bowtie 2 arm (`--local --score-min G,20,8`); HISAT2 end-to-end still hits the `else` + the `--no-softclip --omit-sec-seq` tail. The `bowtie2_pe_string_byte_frozen_with_aligner_param` / `hisat2_se_option_string` tests guard these. ✔

### Task 1 (reject lift) — GREEN correct, RED unachievable as written (see §3, Critical)
- `config.rs:295` gate is `if cli.local { if aligner != Bowtie2 { Unsupported } … }`. Changing the inner condition to `aligner == Aligner::Minimap2` is the right one-conjunct lift (Bowtie 2 + HISAT2 fall through; minimap2 still rejected). The `--minimap2 --local` reject fires at `:295`, **before** genome discovery (`:336`), so the minimap2 half of the test stays fixture-free and `Err`-with-"local by design". ✔
- **But the HISAT2 half cannot assert `Ok`** — see §3.

### No-op plumbing (Verify #6) — verified
- `score_min_local = cli.local` (`config.rs:361`), aligner-independent, feeds six `calc_mapq` call sites in `lib.rs` (729/1258/1640/2744/3339/3789) plus merge/combined. Since the only thing wrong for HISAT2-local was the `(intercept,slope)` (fixed in Task 2) and `calc_mapq`'s `local` branch is aligner-agnostic, **no `score_min_local` plumbing edit is needed.** IMPL correctly lists this as a no-op. ✔
- **No `Cli::validate()` exists** (`grep fn validate` in `cli.rs` = none) — nothing to amend there. ✔
- **No deferred-flags notice** is keyed on `--local`+HISAT2. ✔
- **PE second-mate handling:** `calc_mapq` already adds the `ln(read2_len)` term under `local` (`mapq.rs:35-37`); soft-clip-as-`I` is aligner-agnostic (`methylation.rs:174`, `b'I' | b'S'`). PE reuses with zero new code — matches PLAN. ✔
- Assumption 6 (hard-clip orthogonal) confirmed: the CIGAR walk has no `H` arm; `H` hits the catch-all `_` and fails loud (`methylation.rs:192`). ✔

---

## 2. Coverage (rev-2 PLAN → IMPL)

Every rev-2 item maps to a task; the "Plan coverage checklist" is **honest**:
- Reject lift + minimap2 msg → Task 1 ✔ · Amend reject test → Task 1 ✔
- `score_min_params(cli,aligner)` G/L (🔴) → Task 2 ✔ · Update its test → Task 2 ✔
- options local block (HISAT2 L, no `--local`) → Task 3 ✔ · drop `--no-softclip` → Task 3 ✔
- Mandatory `(0,−0.2)` Perl-cross-checked MAPQ test → Task 4 ✔
- e2e soft-clip round-trip SE+PE → Task 5 ✔ · Docs flips → Task 6 ✔
- oxy gate + soft-clip non-vacuity (blocking) + `--multicore` cell + Q4 determinism → Final ✔
- `--non_bs_mm` no-op / hard-clip orthogonal → documented no-ops ✔
- Remove `debug_assert_eq!(aligner, Bowtie2)` (rev-2 "discrete step", `options.rs:83`): subsumed into Task 3's GREEN ("replace `debug_assert_eq!` with a match"). Present, not dropped. ✔

**Coverage gap (Important):** rev-2 explicitly lists `cli.rs:169` `--local` help and `config.rs:178` `score_min_local` doc as surfaces to *flip*. Task 6 covers `cli.rs:169` and `config.rs:178`, but the IMPL **misses `config.rs:291-294`** (the reject-block doc comment "HISAT2-`--local` is experimental … not supported") — Task 1's prose says "Update the `:178`/`:292-294` doc comments" but the seam table row for docs only lists `README.md:61-62, cli.rs:169, config.rs:178`. The `:291-294` comment will read false after the lift. Minor, but it is a stale-surface the rev-2 "flip not just add" mandate targets.

**Coverage gap (Optional):** `tests/methylseq_conformance.rs:185` carries a comment "(HISAT2/minimap2-local … are still rejected …)". The test body (`methylseq_align_local_now_accepted`) only exercises **Bowtie 2** `--local` (`:198`), so it will **not break** — but its comment becomes partially false (HISAT2-local no longer rejected). Not in any task list. Cosmetic.

---

## 3. Implementability

### 🔴 CRITICAL — Task 1 RED step is unachievable as written
IMPL Task 1 RED + coverage-checklist item #2 say: *"assert `resolve(--hisat2 --local …)` is **Ok** (fixture-free path: the reject fires pre-I/O…)."* This is wrong. Trace of `resolve` (`config.rs:256`):
1. `:295` local reject — once lifted for HISAT2, **falls through**.
2. `:312` `resolve_genome_and_positional` (`:776`) — with `cli_from(&["--local","--hisat2"])`, `cli.genome = None` and `cli.positional = []`, so `it.next()` is `None` → returns **`Err(Validation("No genome folder specified! …"))`** (`:781-787`). This is *before* any disk I/O.

So `resolve(--hisat2 --local)` is **never `Ok`** on the fixture-free path — it trades the local-reject `Err` for the no-genome `Err`. The reject *does* fire pre-I/O (true), but lifting it just exposes the next pre-I/O validation. **The RED assertion must be:** the result is **NOT** an `Err` whose message contains the local-reject text (i.e. `match resolve(...) { Err(e) => assert!(!e.to_string().contains("--local is only supported with Bowtie 2")), Ok(_) => {} }`), exactly as the prompt's Verify #4 anticipated. As written, the RED step would fail to compile/pass and the implementer would be stuck. **Fix the RED step in Task 1 + coverage item #2 before implementing.** (Minimap2 half is fine — it short-circuits at `:295` with the new "local by design" message.)

> Note: this does not change the GREEN code — only the test assertion. The e2e `Ok`-path is genuinely exercised in Task 5 (with a real fake-genome fixture), so end-to-end success is still covered; only the *unit* RED claim is wrong.

### Ordering / independent testability
- **Tasks 2 and 3 must land together to compile-and-pass, but each is independently *authored*.** Task 2 changes `score_min_params`'s signature; its only non-test caller is `config.rs:360`, which Task 2 itself updates — so Task 2 compiles standalone. Task 3 touches the `options.rs:82` block + the `:328` tail, independent of the `score_min_params` signature. They don't share a symbol, so either order compiles. The IMPL's order (1→2→3→4→5→6) is sound. **No blocking dependency**, but flag: Task 2's RED ("won't compile — old signature") and Task 3's RED (options-string asserts) are separate failing states; run them as written, sequentially. ✔
- Task 1's GREEN (reject lift) is a prerequisite for Task 5's e2e (`--hisat2 --local` must resolve past the reject). IMPL orders Task 1 first. ✔

---

## 4. Validation sufficiency

Strong overall. Specific checks:
- **Task 2 test** adds the three required cases incl. the HISAT2-local `(0,−0.2)` parse + G-form-rejected-for-HISAT2 / L-form-rejected-for-Bowtie2-local. Sufficient to lock the branch. ✔
- **Task 4** correctly transcribes the Perl ladder rather than self-consistency, and targets the `diff ≈ 0.78` regime the existing `(20,8)` test (`mapq.rs:370-381`) never hits. The existing local tests use `(20,8)` only (verified `mapq.rs:376`) — so this is genuinely new coverage. ✔ **One sharpening:** Task 4 should also assert at least one PE case (`calc_mapq(len1, Some(len2), …, (0,−0.2), local=true)`) so the *two-mate* `ln` sum (`mapq.rs:35-37`) is cross-checked at sub-unity diff, not just SE — rev-2 mandated SE *and* PE for the gate but the unit test as scoped is SE-only.
- **Final oxy gate** keeps the blocking soft-clip non-vacuity (`$6 ~ /S/ … > 0`) and Q4 determinism prereq — the two failure-modes that would let the gate pass vacuously. Good. The `--multicore` cell proves local + #986 compose. ✔
- **Regression coverage** (Bowtie 2-local #981, HISAT2 end-to-end, single+multicore HISAT2, minimap2) is named in Final step 1; the byte-frozen options tests + `methylseq_align_local_now_accepted` back it. ✔

**Gap (Important):** No unit test asserts the **`--minimap2 --local` reject message** now contains "local by design". Task 1 RED says assert minimap2 is `Err` (it already is), but the *message change* (Q3's explicit requirement) is only verified by the existing substring? — the current test (`:1066`) just asserts `.is_err()`, not the new text. The RED step should assert `.contains("local by design")` so the Q3-mandated message is actually pinned, else the message could regress silently.

---

## 5. Action items

### Critical (fix before implementing)
- **A1 — Task 1 RED is unachievable.** `resolve(--hisat2 --local)` cannot be `Ok` fixture-free; after the lift it returns the "No genome folder specified" `Err`. Rewrite the RED assertion (and coverage-checklist item #2) to "the result is not the local-reject `Err`" (Ok OR a non-local-reject Err), per prompt Verify #4. Code (GREEN) is unaffected.

### Important
- **A2 — pin the minimap2 "local by design" message.** Task 1's amended test should assert the minimap2 `Err` *message* `.contains("local by design")`, not just `.is_err()` (Q3 is an implementation requirement, not just rationale).
- **A3 — add `config.rs:291-294` to the docs-flip set.** The reject-block comment ("HISAT2-`--local` is experimental … not supported") goes stale after the lift; Task 1 prose mentions `:292-294` but the seam table omits it. Make it an explicit Task 6 (or Task 1) edit.
- **A4 — Task 4 should include a PE `(0,−0.2)` MAPQ case.** rev-2 mandates SE+PE; the unit ladder as scoped is SE-only — add one two-mate `ln`-sum assertion at sub-unity diff.

### Optional
- **A5 —** refresh the stale comment at `tests/methylseq_conformance.rs:185` ("HISAT2/minimap2-local … still rejected") to "minimap2-local + combined-index … still rejected". Test body unaffected (Bowtie 2 only), purely cosmetic.

---

**Bottom line:** The IMPL is implementable and the Critical (`score_min_params`) is exactly right, with all byte-freeze regressions accounted for. Land A1 (rewrite the Task 1 RED assertion) before starting; fold A2–A4 in the same pass. A5 is cosmetic.
