# PLAN_REVIEW_B — Phase 3: Single-instance align + SAM parse (lockstep stream primitive)

**Reviewer:** B (independent, fresh context)
**Plan reviewed:** `phase3-single-instance-align-parse/PLAN.md`
**Grounding read:** Perl `bismark` 6849–6912, 2722–2796, 7124–7243; `EPIC.md`; `SPEC.md`; Phase 1–2 `src/{config,convert,aligner}.rs`.
**Verdict:** Sound and faithful in the main. **No Critical blockers.** Several Important faithfulness/lifecycle items should be nailed down in the plan text before implementation, chiefly (1) the exact contents of `raw_line` (Perl stores the **chomped** line, and `--ambig_bam` re-emits it), (2) a hardening of the validation matrix against a silent-wrong-parse on extra tab fields, and (3) explicit child-process deadlock/zombie semantics. Details below.

---

## 1. Logic review

The spawn → header-skip → store-first → peek/advance → finish/reap shape is a correct, minimal mirror of Perl 6871–6911. Walking the steps against the source:

- **§3.1 spawn / arg order.** Plan builds `<aligner_options split on whitespace> --norc -x <ct_index> -U <converted_temp>` and uses `stdout(piped())`, `stderr(inherit())`. This matches Perl 6882 (`$path_to_bowtie $bt2_options -x $fh->{bisulfiteIndex} -U $temp_dir$fh->{inputfile}`) and 6872–6878 (append `--norc` for `CTreadCTgenome`/`GAreadGAgenome`, else `--nofw`). `stderr` inherited matches Perl's pipe-to-terminal of the Bowtie 2 summary, and the EPIC confirms that summary text is **not** gated. **Faithful.** ✓

- **§3.2 header skip / store-first.** Discard `^@` lines, store first non-`@` as current, `None` at EOF — exact mirror of Perl 6884–6910. The comment "Bowtie 2 emits a line per read even when unmapped" is correct and load-bearing for the lockstep model. ✓

- **§3.3 field indices.** `[0,1,2,3,4,5,9,10]` for qname/flag/rname/pos/mapq/cigar/seq/qual matches Perl 2737 exactly. RNAME kept raw (suffix de-conversion deferred to Phase 4/5) — correct; Perl does `s/_(CT|GA)_converted$//` at 2763 in the *consumer*, not the parser. ✓

- **§3.3 unmapped.** `is_unmapped() = flag == 4` matches Perl 2739 (SE). The plan correctly flags this is SE-only (PE differs, Phase 7) and that the `flag==4` "store next, move on" advance logic (Perl 2739–2758) lives in the **Phase 4 consumer**, not in Phase 3. The primitive providing only peek+advance is the right seam. ✓

- **§3.5 finish/Drop.** `finish()` waits + checks exit status; `Drop` kills if not finished. Reasonable; see §4 Important items for the gaps in *how* this is specified.

**Logic gaps found:** none that break correctness of the primitive. The two substantive faithfulness nuances (raw_line contents; tag-scan collapse) are in §2.

---

## 2. Assumptions / faithfulness

### 2.1 `raw_line` semantics vs Perl `last_line` — **IMPORTANT, under-specified**

Plan §3.3 says: *"keep the **raw line** verbatim (Perl keeps `last_line` for later re-emit / `--ambig_bam`)."* The word *verbatim* is ambiguous and, if taken literally (line **with** its trailing `\n`/`\r`), will be **wrong**.

Perl 6897–6901: `chomp;` runs **before** `$fh->{last_line} = $_;`. So `last_line` is the line **with the terminator already stripped**. That stored value is exactly what `--ambig_bam` re-emits at 2807–2808 / 2823–2824 (`$first_ambig_alignment = $fhs[$index]->{last_line}; ... s/_(CT|GA)_converted//`). Since the eventual byte-identity gate (EPIC §5) is on SAM record content and `--ambig_bam` is in v1 scope (EPIC §2 "incl. `--ambig_bam`"), `raw_line` **must hold the chomped (terminator-free) line**, or Phase 6's `--ambig_bam` re-emit will carry a stray terminator / mis-format.

**Action:** State explicitly that `raw_line` = the line with the trailing terminator trimmed (same trim used before the QUAL split), i.e. byte-equal to Perl's chomped `last_line`. Add a unit assertion (`raw_line` of a record ending `\n` contains no `\n`). This also makes §3.6's "CRLF trim" and `raw_line` consistent (today they could diverge: trim only for the QUAL split but keep `raw_line` with `\r`).

### 2.2 Tag-scan ordering collapse — faithful, but worth a one-line justification

Perl 2775–2795 is a per-field `if (AS:i:) / elsif (ZS:i:) / elsif (MD:Z:) / else { if bowtie2 { if (XS:i:) / elsif (ZS:i:) } }`. The plan collapses this to "`AS:i:`→score; `XS:i:`/`ZS:i:`→second_best; `MD:Z:`→md_tag". This collapse **is behaviorally identical** because the four prefixes (`AS:i:`, `XS:i:`, `ZS:i:`, `MD:Z:`) are mutually exclusive on a single field — no field can match two, so the if/elsif precedence never fires a tie. Note the Perl quirk that `ZS:i:` appears in **both** the top-level `elsif` and the bowtie2 `else` branch (the latter is dead for bowtie2 since the top-level catches it first); the collapse correctly drops the dead branch. **No change needed**, but add a half-sentence noting "prefixes are disjoint so if/elsif order is immaterial" so a future reviewer doesn't 'fix' it back.

One faithfulness sharpening: the plan should specify the match is a **prefix/`starts_with`** semantics with `.*`-style capture of the remainder (Perl `AS:i:(.*)` captures greedily to end-of-field), and that `AS`/`XS`/`ZS` capture is parsed as **`i64`** (Bowtie 2 scores ≤ 0; the Self-Review §11 already notes i64 — good). `MD:Z:(.*)` is kept as `String`. Make sure the regex/`strip_prefix` keeps the *entire* remainder of the field (MD strings contain digits and letters; AS could in principle be multi-digit negative). The §9 row 2 test uses `MD:Z:50` — add an MD case with embedded letters (e.g. `MD:Z:7A42`) to prove the capture isn't numeric-only.

### 2.3 `Orientation::{Norc, Nofw}` and the strand table

`reset_counters_and_fhs` (7124–7243) confirms the SE-directional `@fhs` = `[CTreadCTgenome (BS_CT), CTreadGAgenome (BS_GA)]`, and 6873–6877 maps `CTreadCTgenome`/`GAreadGAgenome`→`--norc`, the cross pair→`--nofw`. The plan demonstrates `CTreadCTgenome` (`--norc`, CT index). The `Orientation` enum + `index`/`input` params make the primitive reusable for Phase 4's 2–4 instances. **Faithful.** One nit: the plan's signature names the enum value `Norc`/`Nofw` but the strand-instance *names* (`CTreadCTgenome`…) and the strand label (OT/CTOB…) are what Phase 4 needs for reporting. Phase 3 doesn't need them — fine to defer — but make sure the `spawn` signature doesn't bake in `Orientation` as the *only* per-instance identity such that Phase 4 has to thread a parallel "which instance is this" enum awkwardly. Minor; flag for Phase 4 design, not a Phase 3 blocker.

### 2.4 `-U` path: full path vs Perl's `$temp_dir$inputfile`

Perl passes `-U $temp_dir$fh->{inputfile}` (temp-dir prefix + relative name). Phase 2's `ConvertedReads.path` is already the full `temp_dir + name` concatenation (convert.rs 157–158), so passing `cr.path` as `input: &Path` reproduces the same string Bowtie 2 sees. The only divergence is the stderr summary text ("reading in sequences from …"), which is not gated. **Faithful / no action**, but the plan should pass `ConvertedReads.path` (not reconstruct), to avoid drift from Phase 2's temp-dir-empty CWD-relative handling.

### 2.5 `aligner_options` whitespace split

Plan §3.1 splits `aligner_options` "on whitespace". Perl never tokenizes — it interpolates the whole string into a shell command (6882 `open(... "|")` is a shell pipe). Rust uses `Command` (no shell), so it **must** tokenize. `aligner_options` is built by `options::build_aligner_options` (config.rs 170) under Bismark's control (default `-q --score-min L,0,-0.2 --ignore-quals` per EPIC §5) — single-space-joined, no quoted args, no embedded spaces in a single token → `split_whitespace()` is safe and produces the identical argv Bowtie 2 would receive from the shell. **Faithful for the controlled option set.** Worth a one-line assumption: "`aligner_options` is Bismark-generated and contains no shell-quoted/space-bearing tokens, so `split_whitespace` is exact." (If a future option ever embeds a quoted path this breaks, but that's out of the v1 controlled set.)

---

## 3. Efficiency

§6 is appropriate: one subprocess, linear streaming `BufReader`, parse-on-advance, reuse a line buffer. Not a hot path (Bowtie 2's index load dominates). Two notes:

- **Buffer reuse vs `raw_line` ownership.** The plan says "reuse a line buffer across `advance()`" *and* `SamRecord.raw_line: String` (owned). These are compatible only if `advance()` reads into the reused buffer and then **clones** the trimmed slice into the owned `raw_line` (and into the owned `qname`/etc. via `to_string()`). That's correct and cheap, but the plan should say so, lest an implementer try to hand out a borrow of the reused buffer and fight the borrow checker. Minor wording.
- **No premature optimization** — agreed. ✓

---

## 4. Child-process lifecycle (zombie / deadlock)

This is the riskiest area for a subprocess-wrapping primitive. The plan's mitigations (`finish()` reap + `Drop` kill-guard) are directionally right but **under-specified**; pin these down:

### 4.1 stderr inherited → **no deadlock from stderr** (good), but document the reasoning — **IMPORTANT**

Because the plan inherits stderr (not piped), there is **no second pipe to drain**, so the classic "child blocks writing stderr while parent only drains stdout" deadlock cannot occur. This is a *correct and deliberate* consequence of matching Perl (which only pipes stdout). The plan should state this explicitly as the reason it's deadlock-safe — otherwise a future change to `stderr(piped())` (e.g. to capture the summary) would silently reintroduce a deadlock. **Add a sentence.**

### 4.2 `Drop` without draining stdout — **IMPORTANT (zombie/SIGPIPE on early drop)**

If the stream is dropped early (e.g. Phase 4 abandons an instance after a `die`-equivalent), `Drop` must: (a) `kill()` the child, **then** (b) `wait()` it to reap (kill alone leaves a zombie until wait). The plan says "`Drop` guard kills the child" but does **not** say it also reaps. State: `Drop` = `kill()` + `wait()` (ignore errors). Also: dropping `ChildStdout` closes the read end → the child may get SIGPIPE on its next write; that's fine (we're killing it anyway) but means `kill()` may race a self-exit — `wait()` after `kill()` handles both. **Specify kill-then-wait in Drop.**

### 4.3 `finish()` after partial consumption — **IMPORTANT**

Perl reads stdout to EOF in lockstep, so the pipe is fully drained before the process exits. The Rust `finish()` is called by Phase 4 — but **when**? If `finish()` is called while stdout still has buffered/unread data, `wait()` could block if the child is still trying to write into a full pipe. For Phase 3's tests this won't bite (fakes emit a few lines), but the contract matters for Phase 4. Recommend the plan state the invariant: **`finish()` is only valid after the stream has been advanced to EOF (`current() == None`)**, or, more robustly, `finish()` drains any remaining stdout before `wait()`. The latter is safer and cheap. **Add the invariant or the drain.** Without it, a real run where Phase 4 stops early on one instance could hang in `wait()`.

### 4.4 Exit-status check semantics

Perl does not check Bowtie 2's exit status at all (the `open(... "|")` pipe close return is unchecked here). The plan **adds** a non-zero-exit → error check, which is *stricter* than Perl. This is a reasonable hardening (catches a crashed aligner instead of silently producing a truncated BAM), and it cannot make a successful run diverge (exit 0 path unchanged). **Keep it, but note it as an intentional deviation from Perl** (Perl is fail-open here) in §8/§11 so it's a conscious decision, not an accident. One caveat: a child killed by SIGPIPE (if Phase 4 ever closes stdout early then calls finish) exits non-zero → would error spuriously. Tie this back to 4.3's invariant.

---

## 5. Validation sufficiency (§9)

The 9-row matrix + the fake-`bowtie2` harness is a good shape and reuses the proven Phase-1 detection-test pattern (hermetic, no real Bowtie 2/index). It covers the headline failure modes: header skip, advance-to-EOF, empty/all-header, unmapped, missing AS/MD, spawn failure, exit status. **But there are gaps that could let a silent-wrong-parse through:**

### 5.1 **Gap — no negative/multi-field robustness test for field-index drift.** (IMPORTANT)
Row 1 tests "a canned mapped line" but the index extraction (`[9]`=SEQ, `[10]`=QUAL, tags from `[11..]`) is the single most likely place a typo silently mis-parses (off-by-one would still "parse" and produce a plausible-looking wrong SEQ/QUAL). Add a test with a **fully realistic Bowtie 2 SAM line** including the *optional fields between field 5 and 9* — i.e. RNEXT(6), PNEXT(7), TLEN(8) populated with non-trivial values (e.g. `=`, a number, `0`) — to prove the parser skips 6/7/8 and lands SEQ/QUAL on the right columns. The current §9 wording doesn't guarantee the canned line has 6/7/8 filled.

### 5.2 **Gap — tag at a non-canonical position / extra trailing tags.** (IMPORTANT)
Real Bowtie 2 lines carry tags like `AS:i:`, `XN:i:`, `XM:i:`, `XO:i:`, `XG:i:`, `NM:i:`, `MD:Z:`, `YT:Z:` in varying order, plus `XS:i:` only when a second alignment exists. Row 2 tests a single tidy combination. Add a test where `MD:Z:` and `AS:i:` are **separated by several unrelated tags** and `MD:Z:` appears **last**, to prove the `11..` scan walks all fields (not just field 11) and that unrelated tags (`XM:i:`, `NM:i:`) are correctly ignored — a `starts_with`/regex bug that matched `XS:i:` against `XM:i:` would be caught here. Also add a line **with `AS:i:` but no `XS:i:`** (the common unique-alignment case) to confirm `second_best == None`.

### 5.3 **Gap — POS/FLAG/MAPQ numeric-parse failure path.** (OPTIONAL→IMPORTANT)
`flag: u16`, `pos: u32`, `mapq: u8` are numeric parses. What happens on a malformed/empty numeric field? The plan's edge cases cover missing *tags* but not a malformed *core* numeric. Bowtie 2 won't emit these, but the parser's `Result` contract should be deliberate: either `parse()` returns `Err` (preferred — fail loud, consistent with the project's "fail explicitly" principle) or it must be documented why a default is safe. Add one negative unit (`pos = "notanumber"` → `Err`). This guards against a silent `unwrap_or(0)` slipping in.

### 5.4 **Gap — CRLF/terminator test is described but not in the matrix.** (IMPORTANT)
§3.6 and §2.1 both hinge on terminator trimming, and `raw_line` correctness depends on it, but §9 has **no row** asserting it. Add a row: fake emits a line ending `\r\n`; assert `qual` has no trailing `\r` **and** `raw_line` has no trailing `\r`/`\n`. This is the test that locks in the §2.1 fix.

### 5.5 **Gap — exit-status test interaction with EOF.** (OPTIONAL)
Row 7 tests `finish()` on exit 0 / exit 1. Make the exit-1 fake **also emit valid records first** then exit 1, to prove `finish()` errors even after a clean-looking stream (catches a "we only check status if no records" bug). And per §4.3, add/þnote the "finish after full advance-to-EOF" ordering in the test so the harness exercises the intended call sequence.

### 5.6 Adequacy verdict
With 5.1, 5.2, 5.4 added, the fake-`bowtie2` approach is **adequate to catch silent-wrong-parse** for the v1 spine. The fake approach's one inherent blind spot — that it can't prove the *real* Bowtie 2 emits the column/tag layout we assume — is acceptable here because that's exactly what the Phase-0 spike already validated on real output and what Phase 5/10's byte-identity gate will re-confirm end-to-end. No need to add a real-Bowtie 2 test in Phase 3.

---

## 6. Decisions (Open Qs) — assessment

- **Q1 (raw `split('\t')` vs noodles-sam): endorse raw split.** It is the faithful choice for this phase: it mirrors Perl's `split(/\t/)` exactly, keeps the chomped `raw_line` for `--ambig_bam` re-emit trivially, and avoids pulling noodles' SAM *reader* into a path that only needs 8 fields + 3 tags. noodles enters in Phase 5 for the BAM *write* (EPIC §5). **No rework risk** — the `SamRecord` struct is an internal type; Phase 5 building noodles `RecordBuf`s from it (or from the chosen alignment) is independent of whether Phase 3 used noodles to parse. Low risk. ✓

- **Q2 (wire into `run()` now vs keep unwired): endorse unwired primitive.** Wiring a throwaway single-instance drain into `run()` now would (a) need to be torn out in Phase 4 when the real 2-instance pipeline lands, and (b) perturb the Phase-1/2 binary behavior + tests for no gate value (Phase 3 has no byte-identity gate per EPIC §3). Keeping `run()` at its Phase-2 behavior and shipping `AlignerStream` as a library primitive is the lower-rework, lower-risk path and matches the EPIC's phase boundary ("build the lockstep primitive (one stream)"). ✓ One caveat: ensure the new `align` module is at least **referenced/compiled** (e.g. `pub mod align;` in lib.rs and the tests in the crate) so it doesn't bitrot as dead code — the plan implies this via the test suite, but state it.

---

## 7. Alternatives considered

- **Plain `Iterator<Item=Result<SamRecord>>` instead of `current()/advance()`.** The plan chose peek+advance, which is **correct for Phase 4's N-way lockstep merge**: the merge must compare the *current head* of all 2–4 streams against a target read-ID and selectively advance only the matching stream(s) (Perl 2730–2758). A bare `Iterator` consumes on `next()` and has no peek; you'd have to wrap it in `Peekable` anyway — and `Peekable::peek` returns `Option<&Result<_>>` which is awkward to thread through the merge with `?`. The explicit `current()->Option<&SamRecord>` + `advance()->Result<()>` split (errors surface on advance, peek is infallible) is the cleaner seam. **Endorse the chosen design.** It also matches Perl's mental model (`last_line`/`last_seq_id` = "current", getline = "advance") almost 1:1, which aids faithfulness auditing. ✓
- **One alternative worth a sentence:** expose `current_seq_id()` (or have the consumer read `current().qname`) — Perl tracks `last_seq_id` separately (6900) purely as a cache. Phase 4 will compare `current().qname` against the target ID; that's fine, no need for a separate field. No change.

---

## 8. Action items (prioritized)

### Critical
- *(none)* — no blocker to starting implementation once the Important items are folded into the plan text.

### Important
1. **Specify `raw_line` = chomped (terminator-free) line**, byte-equal to Perl's `last_line` (6898–6901), since `--ambig_bam` re-emits it (2807–2808). Use the same trim as the QUAL split; add a unit asserting `raw_line` has no trailing `\n`/`\r`. (§2.1)
2. **Add a CRLF/terminator row to §9** asserting both `qual` and `raw_line` are terminator-free. (§5.4)
3. **Add a realistic-layout parse test** with RNEXT/PNEXT/TLEN (fields 6–8) populated, to lock SEQ/QUAL on indices 9/10. (§5.1)
4. **Add a tag-scan robustness test**: many tags, `MD:Z:` last, unrelated tags (`XM:i:`,`NM:i:`,`YT:Z:`) present-and-ignored, and a unique-alignment line with `AS:i:` but **no** `XS:i:` → `second_best == None`. Add an MD value with letters (`MD:Z:7A42`). (§5.2)
5. **Pin child-process semantics in the plan:** `Drop` = `kill()` **then** `wait()` (reap, not just kill); `finish()` valid only after advance-to-EOF *or* drains remaining stdout before `wait()` (avoid a Phase-4 early-stop hang). State that inherited stderr is *why* there's no stdout/stderr deadlock. (§4.2–4.4)
6. **Note the intentional deviation**: Phase 3 checks Bowtie 2 exit status; Perl is fail-open. Record in §8/§11 as a conscious hardening, with the SIGPIPE caveat tied to the finish-after-EOF invariant. (§4.4)

### Optional
7. Add a half-sentence justifying the tag-scan collapse (prefixes disjoint → if/elsif order immaterial; bowtie2 `else`-branch `XS:i:` folded in, dead `ZS:i:` sub-branch dropped). (§2.2)
8. Add a negative core-numeric unit (`pos`/`flag` malformed → `Err`, fail-loud). (§5.3)
9. State `advance()` reads into a reused buffer and clones the trimmed slice into the owned `SamRecord` fields (clarify buffer-reuse vs owned-`String` coexistence). (§3)
10. State `aligner_options` is Bismark-generated with no space-bearing/quoted tokens, so `split_whitespace` reproduces the shell argv exactly. (§2.5)
11. Make the exit-1 fake also emit valid records before exiting non-zero (proves status check fires post-stream). Reference `align` module from lib.rs so it isn't dead code. (§5.5, §6)

---

## 9. Summary

The Phase 3 plan is a faithful, well-scoped mirror of Perl 6871–6911 / 2737–2796: spawn args, header-skip, store-first, field indices `[0,1,2,3,4,5,9,10]`, the AS/XS/ZS/MD tag scan, and `flag==4` SE-unmapped all check out against the source, and the `current()/advance()/finish()` design is the right seam for Phase 4's lockstep N-way merge (better than a bare Iterator). Both Open-Q decisions (raw `split('\t')`; unwired primitive) are sound and low-rework. No Critical blockers. The items to fix before implementing are (a) `raw_line` must be the **chomped** line — Perl stores `last_line` post-`chomp` and `--ambig_bam` re-emits it; (b) tighten the §9 matrix against silent-wrong-parse (realistic RNEXT/PNEXT/TLEN layout, multi-tag/`MD`-last scan, CRLF row); and (c) pin the child-process contract (Drop = kill+wait; finish-after-EOF-or-drain; inherited-stderr is the deadlock-safety reason; exit-status check is an intentional fail-closed deviation from Perl).
