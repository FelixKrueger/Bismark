# PLAN_REVIEW_A — Phase 3: Single-instance align + SAM parse (lockstep stream primitive)

- **Reviewer:** A (independent, fresh context)
- **Plan reviewed:** `phase3-single-instance-align-parse/PLAN.md`
- **Grounding:** Perl `bismark` v0.25.1 (`single_end_align_fragments_to_bisulfite_genome_fastQ_bowtie2` 6849–6912; `check_results_single_end` 2722–2796; `reset_counters_and_fhs` 7124–7243), EPIC.md, SPEC.md, Phase-1/2 source (`config.rs`, `convert.rs`, `discovery.rs`, `aligner.rs`, `tests/cli.rs`).
- **Verdict:** Solid, faithful, well-scoped primitive. The peek/advance seam is right for Phase 4. **Two Important faithfulness gaps** in the tag-scan and one **Important deadlock risk** in the child-process lifecycle that the plan under-specifies. No Critical blockers, but the tag-scan precedence and the stdout-pipe-full-on-error path must be nailed before implementation, or Phase 4 inherits a silent-wrong-parse / hang.

---

## 1. Logic review

### What is correct (verified against Perl)

- **Spawn arg order (§3.1).** `aligner_options` (whitespace-split) → `--norc` → `-x <index>` → `-U <reads>` exactly mirrors Perl 6872–6882 (`open ($fh->{fh},"$path_to_bowtie $bt2_options -x $fh->{bisulfiteIndex} -U $temp_dir$fh->{inputfile} |")`). The `--norc`/`--nofw` selection (6873–6878) is captured by the `Orientation` enum. ✓
- **Header skip (§3.2).** Perl 6884–6894 reads lines, `last unless /^\@/`, then stores the first non-`@` line. The plan replicates this and correctly notes Bowtie 2 emits one line per read even when unmapped (Perl comment 6896). ✓
- **Store-first-record / EOF→None (§3.2).** Perl 6897–6910: if a first line exists, store it; else `last_seq_id`/`last_line` = undef. Plan's `current: Option<SamRecord>` set to `None` at EOF is the faithful analogue. ✓
- **Field indices (§3.3).** Perl 2737 destructures `(split /\t/)[0,1,2,3,4,5,9,10]` → QNAME, FLAG, RNAME, POS, MAPQ, CIGAR, SEQ, QUAL. Plan's index list matches exactly. ✓
- **RNAME kept raw (§3.3).** Perl de-converts (`s/_(CT|GA)_converted$//`) only at 2763, inside `check_results` (= Phase 4). Deferring de-conversion to Phase 4/5 and keeping RNAME verbatim in Phase 3 is correct and explicitly faithful. ✓
- **`flag == 4` = SE-unmapped (§3.3 / edge cases).** Perl 2739 `if ($flag == 4)`. The plan correctly puts the *consumer* "store next, move on" logic (Perl 2740–2758) in Phase 4 and keeps Phase 3 to a pure `is_unmapped()` predicate. ✓
- **`raw_line` retained.** Perl stores `last_line` verbatim for re-emit / `--ambig_bam` (2807, 2823). Plan keeps `raw_line: String`. ✓
- **No-die-on-missing-tags (§3.3 edge cases).** Perl's `die` for missing AS/MD is at 2838, *inside* the scoring loop (Phase 4), not the parser. Plan correctly has Phase 3 parse-not-die. ✓
- **`Orientation` enum vs strand table.** `reset_counters_and_fhs` 7124–7243 + 6873–6878: `CTreadCTgenome`/`GAreadGAgenome` → `--norc`; `CTreadGAgenome`/`GAreadCTgenome` → `--nofw`. The 2-value enum is sufficient because the *index* and *input* are passed as separate params; the enum only carries the flag. Faithful. ✓

### Faithfulness gaps (the parser)

**(A) Tag-scan precedence is mis-summarised — `ZS:i:` outranks `XS:i:`, and only ONE tag wins per field.** *(Important)*

The plan §3.3 says "`XS:i:`/`ZS:i:` → `second_best`" as if they are interchangeable. Perl 2777–2795 is an ordered `if/elsif` chain evaluated **per field**:

```perl
if    (/AS:i:(.*)/) { $alignment_score = $1 }
elsif (/ZS:i:(.*)/) { $second_best     = $1 }   # ZS wins here, top-level
elsif (/MD:Z:(.*)/) { $MD_tag          = $1 }
else { if ($bowtie2) {
         if    (/XS:i:(.*)/) { $second_best = $1 }
         elsif (/ZS:i:(.*)/) { $second_best = $1 }  # dead: ZS already caught above
} }
```

Consequences a naïve Rust port can get wrong:
1. **Within a single field, the chain is exclusive** — a field matching `AS:i:` is never tested for the others. A Rust `for tag in fields[11..] { if starts_with("AS:i:")… else if starts_with("ZS:i:")… }` reproduces this; a flat sequence of independent `if`s on the same field does not (and would be wrong if any field ever matched two patterns — it won't for well-formed SAM, but match the structure for safety/clarity).
2. **`ZS:i:` is evaluated before `XS:i:`.** For Bowtie 2 output this never collides (Bowtie 2 emits `XS:i:`, not `ZS:i:`), so it is inert on the v1 spine — but the plan's §9 row 2 *deliberately tests a HISAT2 `ZS:i:` line*, so the parser must apply the `ZS`-before-`XS` precedence or that test encodes the wrong contract. The inner `else`-branch `ZS:i:` (2791) is **dead** in Perl (already caught at 2780); the Rust port should NOT add a second independent `ZS` check that could shadow a prior `XS` in the same field.

**Action:** §3.3 / §4 `parse` should specify the exact ordered chain (AS, then ZS, then MD, then XS) with at-most-one-match-per-field semantics, and §9 row 2 should assert that a line carrying *both* `XS:i:` and `ZS:i:` resolves `second_best` from `ZS` (documents the precedence). This is the single most likely silent-wrong-parse for the eventual non-Bowtie2 path and is cheap to lock down now.

**(B) The tag match is an unanchored substring with a greedy `(.*)` capture — not a strict `KEY:i:<int>` prefix parse.** *(Important)*

Perl `/AS:i:(.*)/` matches the pattern **anywhere** in the field and captures everything to end-of-field (after `chomp`, so no trailing `\n`). The plan §3.3/§4 phrases it as `AS:i:<int>` and §11 says "parse as `i64`". Two divergences to pin:

1. **Capture is `String`, not `int`, in Perl.** `$alignment_score`/`$second_best` are captured as strings and only later used numerically (Perl auto-coerces at the `>=`/`==` comparisons in 2813/2844). For Bowtie 2 these are always clean signed integers, so parsing to `i64` in Phase 3 is *behaviourally* equivalent **and** desirable (Phase 4 needs the numeric value). But the plan must decide: if `i64::from_str` ever fails on a captured tag value, does Phase 3 error or store `None`? Perl never fails (it would silently coerce a non-numeric to 0 with a warning). Recommend: store `Option<i64>` and, on a malformed numeric, store `None` (parse-not-die, matching the §3.3 "no die" stance) rather than erroring the whole stream. Document this explicitly — otherwise it's an undefined behaviour the two reviewers/implementer could each resolve differently.
2. **`md_tag` keeps the value only (not the `MD:Z:` prefix).** Perl captures `$1` = everything after `MD:Z:`. Plan's example "`MD:Z:50` → `md_tag="50"`" (§9 row 2) is correct (value only). Just confirm `parse` strips the `MD:Z:` prefix and stores the bare value, consistent with how Phase 5's MD reconstruction (Perl 9276/9345) re-adds the prefix.

Neither (A) nor (B) breaks the v1 Bowtie2 spine, but both are exactly the kind of "looks fine on the happy path, wrong on an edge" that the byte-identity gate exists to catch — and they are nearly free to specify precisely now.

### Smaller logic notes

- **FLAG type (`u16`).** Fine for SAM (max 4095 with all bits). ✓
- **POS `u32`, MAPQ `u8`.** SAM POS is ≤ 2^31−1 (samtools), fits u32; MAPQ 0–255 fits u8. ✓ But the plan does not say what happens if `FLAG`/`POS`/`MAPQ` fail to parse (e.g. a malformed/truncated line). Perl `split` + numeric-context never errors. Recommend: `parse` returns `Err` on a structurally broken core line (too few fields) — that is a genuine "the aligner produced garbage" signal — but be explicit. See §5 below.

---

## 2. Assumptions

- **"Raw `split('\t')` vs noodles-sam" (Open Q1).** *Sound.* noodles-sam would (a) impose its own field validation that could reject a line Perl's `split` happily processes, and (b) not naturally preserve the verbatim `raw_line` Bismark re-emits. Raw split is the faithful choice; noodles enters in Phase 5 for the BAM *write* (per SPEC §7 / EPIC §5). Confirm and close Q1 as "raw split".
- **"Not wired into `run()`" (Open Q2).** *Sound, and the better choice.* Wiring a single-instance drain into `run()` now would (a) require a real Bowtie 2 + index to exercise `run()` end-to-end (the Phase-1/2 CLI tests use only a fake `bowtie2` that echoes a version string — see `tests/cli.rs:42`), and (b) produce a half-pipeline that emits nothing useful, churning the binary's observable behaviour twice (Phase 3 then Phase 4). Keeping it a library primitive preserves the green Phase-1/2 binary tests. Confirm Q2 as "unwired primitive".
- **Arg-order faithfulness (§8).** Correctly flagged as cosmetic (Bowtie 2 is order-independent) but replicated for parity. ✓
- **`stderr` inherited (§8).** Matches Perl (the `| getline` pipe captures only stdout; Bowtie 2's summary goes to the terminal). ✓ One nuance for Phase 4/9: when N instances all inherit stderr, their summaries interleave — Perl has the same behaviour (4 pipes, shared terminal), so this is faithful, not a defect. Worth a one-line note that the interleaving is expected.
- **Determinism (§2 / EPIC).** Single-thread-per-instance, no `-p`/`--reorder`. The plan relies on this for the lockstep model; Phase-0 confirmed it. ✓ Note: the plan does NOT add `-p 1` or assert single-threadedness — it relies on `aligner_options` *not* containing `-p`. That is correct for the v1 default options (`-q --score-min L,0,-0.2 --ignore-quals`, EPIC §5), but a user passing `-p 4` would silently break the lockstep order. This is a Phase 4 concern (the merge is where order matters), but flag it: Phase 4 or option-validation must reject/strip user `-p` > 1 (or add `--reorder`). Out of scope for Phase 3, but the assumption should be recorded as "options are assumed `-p`-free; enforced later."

---

## 3. Efficiency

Appropriate and not over-engineered. §6 is right: this is not a hot path (Bowtie 2's index load + alignment dominates). Streaming `BufReader` line read + parse-on-advance + buffer reuse is the correct shape.

- **Buffer reuse caveat.** §6/§11 say "reuse a line buffer across `advance()`". If using `BufRead::read_line(&mut String)`, the implementer must `buf.clear()` each call (read_line *appends*). And because `SamRecord` owns `String` fields (`qname`, `raw_line`, …), each `advance()` allocates regardless of buffer reuse — the reused buffer only saves the *read* allocation, not the record allocation. That is fine (record allocation is unavoidable while `current()` must outlive the next read), but the "buffer reuse" claim is minor; don't let it imply zero-alloc. No change needed, just don't over-promise.
- **`String` vs `&str` for `SEQ`/`QUAL`.** These can be long (read length). Owning them is necessary because the buffer is reused. Fine.

No efficiency blockers.

---

## 4. Child-process lifecycle (the real risk area)

**(C) stdout-pipe-full deadlock on the error / early-drop path.** *(Important)*

With `stdout(piped())` and a synchronous `BufReader` read loop, the parent only drains stdout when it calls `advance()`. Two scenarios the plan does not fully address:

1. **`finish()` called before EOF (consumer stops early).** Phase 4's lockstep can legitimately stop draining one instance while others still run (e.g. one stream hits EOF, or — in later phases — an early-out). If `finish()` just calls `child.wait()` while the child still has unread data buffered in the OS pipe, and Bowtie 2 is still trying to *write* more than the pipe buffer (64 KiB on Linux) holds, the child blocks on write and `wait()` blocks forever → **deadlock**. The plan's `finish()` = "wait + check exit status" must either (a) drain stdout to EOF before waiting, or (b) drop/close the stdout handle first (causing the child's next write to fail with EPIPE, so it exits). The `Drop` kill-guard covers the *dropped-without-finish* case but `finish()` itself needs the drain-or-close discipline. **Specify this in §3.5.**
2. **Non-zero exit detection vs unread stdout.** If the child exits non-zero *after* the parent already saw EOF (normal), `wait()` returns the status cleanly. But Bowtie 2 typically writes its error to **stderr** (inherited) and may still have flushed partial SAM to stdout. `finish()` checking only the exit status is correct for the gate; just ensure the drain happens so the status is observed rather than a hang. The plan says "non-zero → error" — confirm the error surfaces the exit code (and ideally a hint that stderr has the detail), matching Perl's `die "Can't open pipe to bowtie: $!"` spirit (6882) and the non-zero-exit-on-close behaviour.

For Phase 3's single-instance demo this rarely bites (the test drains to EOF), but the *primitive* is built here and reused by Phase 4 where early-stop is real. Better to bake the drain-or-close into `finish()`/`Drop` now than to debug a Phase-4 hang.

**(D) `Drop` kill-guard — reap after kill.** *(Optional but recommended)* `Child::kill()` sends SIGKILL but does **not** reap; the guard should `kill().ok(); wait().ok();` (or rely on the fact that `Child`'s own `Drop` does *not* wait → zombie until parent exits). For a long-running Phase-4/9 process spawning many instances across chunks, an un-reaped killed child is a transient zombie. Cheap to do right: kill then wait in the guard. The plan says "kills the child" — add "and reaps (`wait`)".

**(E) `finish(self)` + `Drop` double-handling.** *(Optional)* `finish(self)` consumes the struct, so `Drop` won't also run on the same value — good, no double-kill. But if `AlignerStream` holds the `Child` directly, `finish` must take ownership of the `Child` out of `self` (e.g. via `ManuallyDrop` or by structuring fields so the `Drop` guard is a no-op once `finish` ran). Easiest: make the kill-guard a small inner type and have `finish` `mem::forget`/disarm it. Worth one sentence in §4 so the implementer doesn't fight the borrow checker or accidentally kill-then-wait twice.

These are the classic subprocess pitfalls; the plan *names* the right mitigations (`finish` + `Drop`) but does not specify the **drain-before-wait** invariant, which is the one that actually causes hangs.

---

## 5. Validation sufficiency (§9)

The fake-`bowtie2`-script approach is **the right pattern** (it's exactly what `tests/cli.rs:42` already does, and it makes the tests hermetic — no real Bowtie 2 / index). The 9 cases cover the core happy + edge paths well. Gaps, by priority:

**Important — add these (they target silent-wrong-parse / hang, which is the whole point of the gate):**

- **5.1 Tag precedence (`ZS` before `XS`).** Per finding (A): a record carrying *both* `XS:i:-30` and `ZS:i:-20` must resolve `second_best = -20` (ZS wins). §9 row 2 tests them on *separate* lines; add a single line with both to lock the precedence. Without it, a flat-`if` mis-port passes row 2 but is wrong.
- **5.2 Negative AS / multi-tag real Bowtie 2 line.** §11 flags negative scores but §9 has no explicit negative-AS assertion beyond row 2's `-12`. Add a realistic full Bowtie 2 SAM line (with `NM:i:`, `YS:i:`, `YT:Z:` and other tags *between/around* the ones we want) to prove the scan finds AS/XS/MD regardless of position and ignores unknown tags. This is the realistic input and is currently only implicitly covered.
- **5.3 `finish()` after partial read (drain/close).** Per finding (C): a fake that emits, say, 100 KiB of SAM (more than one pipe buffer) where the test calls `current()` once then `finish()` *without* advancing to EOF — must NOT hang and must return cleanly. This is the test that would have caught the deadlock. The current row 7 ("finish reaps + exit status") drains fully first, so it can't catch the pipe-full case.
- **5.4 Malformed/short line.** A non-`@` line with < 11 (or < 5) fields: assert `parse` behaves as specified (error vs lenient) — and that the *specification* exists (see finding §1 smaller-notes). Right now §3.3 is silent on a truncated core line; §9 doesn't test it. Pick a behaviour and assert it.

**Optional — nice to have:**

- **5.5 CRLF / trailing-`\r` trim.** §3.3 edge-cases mention CRLF handling ("trim the line terminator before splitting QUAL"). Add a fake line ending `\r\n` and assert QUAL has no trailing `\r`. (Bowtie 2 won't emit CRLF, but the trim is in the code, so test the code that exists.) Note: `BufRead::read_line` strips `\n` but **keeps** `\r` — so this trim is load-bearing and untested by rows 1–9.
- **5.6 Whitespace-split of `aligner_options`.** §3.1 splits options on whitespace into argv. A test that the default `-q --score-min L,0,-0.2 --ignore-quals` splits to the right tokens (and that an empty/extra-space options string doesn't produce empty argv entries that Bowtie 2 would reject). Perl's `$bt2_options` is a single shell string in a `qx`-style pipe (the shell re-splits); Rust must split it itself, and naive `split(' ')` on a double-space yields an empty arg. Use `split_whitespace()`. Worth a one-line test + a §3.1 note specifying `split_whitespace()` (not `split(' ')`).
- **5.7 Arg-vector assertion.** Since the spawn arg order is a stated faithfulness contract (§8), a test that captures the argv the fake was invoked with (e.g. the fake writes `"$@"` to a file the test reads) and asserts `--norc -x <idx> -U <reads>` order. Cheap insurance that the contract doesn't silently regress.

**Coverage of the stated edge cases (§3 edge cases):** empty/all-header (row 9 ✓), unmapped (row 3 ✓), missing AS/MD (row 4 ✓), spawn failure (row 8 ✓), non-zero exit (row 7 ✓), line-terminator trim (NOT tested → 5.5). So one stated edge case (CRLF trim) is specified but unvalidated.

---

## 6. Alternatives considered

- **noodles-sam parsing (Open Q1).** Rejected correctly — see §2. Raw split is more faithful and preserves `raw_line`.
- **Iterator vs peek/advance.** The plan's `current()`/`advance()` (peek without consume) is the **right** choice over `impl Iterator<Item=Result<SamRecord>>`. The N-way lockstep merge (Perl 2722–2796) needs to *inspect* each stream's current `last_seq_id` against the driving `identifier` and advance only the matching stream(s) — that is a peek-then-conditionally-advance pattern, which `Iterator::next()` (consume-on-look) cannot express without an external `Peekable`-style holding slot. Building the holding slot into the type is cleaner and mirrors Perl's `last_line`/`last_seq_id` stored state exactly. **Sound; no rework risk for Phase 4.**
- **Streaming reads via stdin vs temp file (SPEC 1A-i).** Out of scope here (Phase 2 already writes the temp file and passes `-U`); the plan correctly consumes the temp path. ✓
- **`-p 1`/`--reorder` defensively.** Not added; relies on default options being `-p`-free. Acceptable for Phase 3; should be enforced in option validation by Phase 4 (see §2). An alternative — have `spawn` *assert* the options contain no `-p`>1 — is arguably worth it but belongs in the option layer, not the stream primitive.

---

## 7. Action items (prioritized)

### Critical
*(none — no blocker; the spine is faithful and well-scoped)*

### Important
1. **(A) Specify the exact tag-scan chain** in §3.3/§4: ordered `AS → ZS → MD → (else, bowtie2) XS`, at-most-one-match-per-field, `ZS` outranks `XS`. Add §9 test 5.1 (a line with both `XS` and `ZS` → `second_best` from `ZS`). *(Perl 2777–2795; plan §3.3, §9 row 2.)*
2. **(B) Decide + document the numeric-parse policy** for `AS`/`XS`/`ZS` captures: store `Option<i64>`, lenient `None` on malformed (parse-not-die), `md_tag` = bare value (no `MD:Z:` prefix). State it in §3.3/§4. *(Perl captures `$1` as string; plan §11 says "parse as i64" but is silent on failure.)*
3. **(C) Specify `finish()`/`Drop` drain-or-close-before-wait** in §3.5 to prevent a stdout-pipe-full deadlock when the consumer stops before EOF (real in Phase 4). Add §9 test 5.3 (emit > one pipe buffer, `finish()` after one `current()`, assert no hang). *(Std subprocess pitfall; plan §3.5, §11.)*
4. **Define `parse` behaviour on a structurally short/garbage line** (< required fields) — error vs lenient — and test it (5.4). *(Plan §3.3 silent.)*

### Optional
5. **(D)** `Drop` guard: `kill()` **then** `wait()` (reap), and disarm the guard in `finish()` so it doesn't double-handle. *(Plan §3.5/§4.)*
6. **Use `split_whitespace()`** (not `split(' ')`) for `aligner_options`; note it in §3.1; add test 5.6.
7. **Add the realistic-multi-tag + negative-AS line test** (5.2) and the **argv-order assertion** (5.7) and the **CRLF/`\r`-trim** test (5.5) — all cheap insurance against silent regressions of stated contracts.
8. Record the **"options assumed `-p`-free; enforced in option layer by Phase 4"** assumption in §8, and the one-line note that inherited-stderr summaries interleave across instances (expected, faithful).

---

## 8. Bottom line

The plan is faithful to Perl on every field index, the spawn invocation, the header-skip/store-first model, and the SE `flag==4` unmapped rule, and the peek/advance design is the correct seam for Phase 4's lockstep merge (no rework risk). Close Open Q1 (raw split) and Q2 (unwired primitive) as proposed. Before implementing, tighten three things: (1) the tag-scan **precedence/exclusivity** (so the HISAT2 `ZS` test encodes the real contract and the future non-Bowtie2 path doesn't silently mis-pick `second_best`); (2) the **numeric-parse + malformed-line policy** (lenient vs error, stated, not implicit); and (3) the **drain-before-wait** invariant on `finish()`/`Drop` (the one thing that turns into a hard-to-debug Phase-4 hang). Add the four targeted tests (both-tags precedence, partial-read finish, realistic multi-tag line, short line). None of these is a redesign — they are precision edits to a fundamentally sound plan.
