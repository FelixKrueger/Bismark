# Phase B PLAN — Review B

**Reviewer:** Plan Reviewer B (independent, fresh context).
**Target:** `phase-b-core-report/PLAN.md` (rev 0).
**Ground truth read directly:** Perl `coverage2cytosine` v0.25.1 — `generate_genome_wide_cytosine_report:168-745`, `process_unprocessed_chromosomes:1388-1565`, `context_reporting:1977-1988`, `reset_context_summary:1961-1975`, `print_context_summary:63-78`, `handle_filehandles:89-165`; Phase-A `cli.rs`/`genome.rs`/`error.rs`.

**Verdict: APPROVE WITH CHANGES.** The coordinate arithmetic, single-kernel equivalence, ordering, and summary semantics are faithful to the Perl. But there are **real byte-identity divergence risks in coverage-file parsing** (CRLF, trailing/blank line, malformed lines, non-numeric fields) that the plan currently underspecifies relative to Perl's exact `split /\t/` + numeric-coercion behavior, plus a genuine **non-contiguous-chromosome streaming divergence** the plan's stream-flush does not reproduce. None block the *kernel*; all are parse/stream-layer fixes that must be pinned before implementation to keep the byte-identity contract honest.

---

## 1. Logic review

### 1.1 Single-kernel equivalence (§3.2, §3.3) — VERIFIED CORRECT
I independently traced all three Perl blocks:

- **Covered-chr** (`:343` len<3 → `:347` last-base → `:355` lookup → `:361` threshold → `:365-377` classify → `:381` `context_reporting` → `:384-447` emit).
- **Last-chr** (`:588` lookup → `:594` threshold → `:597` len<3 → `:600` last-base → `:609-621` classify → `:624` `context_reporting` → `:628-689` emit).
- **Uncovered** (`:1495` len<3 → `:1500` last-base → `:1505-1517` classify → `:1519-1559` emit; **no `context_reporting`, no lookup, meth/nonmeth always 0**).

All five gates are pure `next` skip-guards; the emit + summary actions execute iff *every* guard passes. Reordering pure skip-guards **cannot change the surviving set**, so the emit set is identical across covered/last blocks. The classify-warn (`:375`/`:619`) sits *after* both len<3 and threshold in **both** blocks, so the STDERR-warn set is also identical (and STDERR is not gated anyway). The plan's claim that one kernel reproduces all three is **sound**. The accumulate-summary flag (true for covered/last via `:381`/`:624`, false for uncovered — Perl's uncovered block has no `context_reporting`) is faithfully described in §3.5. ✅ No action needed on the kernel logic itself.

One nuance worth a code comment (not a plan change): in the last-chr block a `len<3` position *does* reach the threshold test first (`:594` before `:597`), but because classify is still gated behind `len<3` (`:597` < `:609`), no warn and no emit ever results — consistent with the covered block. The kernel's guard order (covered-block order) is the safe canonical choice.

### 1.2 Coverage-file parsing (§3.1 step 2) — UNDERSPECIFIED, byte-identity risk
The plan says "split on `\t`, fields 0/1/4/5". Perl is `my ($chr,$start,$end,undef,$meth,$nonmeth) = (split /\t/);` (`:209`) after `chomp` (`:207`). Several Perl-specific behaviors are not pinned:

1. **Trailing newline / blank final line.** Perl `while (<IN>)` + `chomp` then `split /\t/` on an **empty string** yields an empty list ⇒ `$chr` is `undef`. `$chr eq $last_chr` (`:223`) compares `undef eq <str>` → false → falls to the `else` (`:227`) block, treating the blank line as a **chromosome transition to `undef`** — it flushes `$last_chr`, then sets `$last_chr = undef` (`:453`) and stores `$chr{undef}->{undef}`. On the *next* real line (if any) or at EOF this is messy but in practice the cov file has a trailing `\n` only on the *last data line* (no trailing blank line) — `while(<IN>)` does not yield a phantom final empty record for a file ending in `\n`. **The realistic risk is a file with a literal blank line in the middle or a trailing blank line.** The plan must state its behavior explicitly: does a blank/short line get skipped, error, or mimic Perl's undef-transition? Byte-identity means *matching Perl*, and Perl here does something pathological (flush-on-undef). **Recommend:** the realistic Bismark cov never has blank lines; pin a test that an empty trailing line (after the final `\n`) produces **no** extra flush (matches `while(<IN>)` not yielding a final empty record), and explicitly decide+test the mid-file-blank-line case (error is acceptable since Perl's behavior there is itself buggy and never exercised on real data — but the plan must *say* so).

2. **CRLF in the cov file.** Perl `chomp` removes only the trailing `$/` (`\n`), leaving a trailing `\r` on `$nonmeth` → `$nonmeth` then used in `$meth + $nonmeth >= $threshold` (`:361`): Perl numeric coercion of `"123\r"` is **123** (leading-numeric coercion, trailing junk ignored, with a non-fatal warning under `use warnings`). So a CRLF cov file in Perl still parses `nonmeth` correctly. The plan parses fields as **bytes** and converts the last field to `u32` via (presumably) `str::parse`/`atoi`. `"123\r".parse::<u32>()` **errors** in Rust. **This is a concrete divergence**: a CRLF cov that Perl tolerates would make the Rust port error (or, if it trims, must trim `\r` exactly as Perl's numeric coercion would tolerate). **Recommend:** strip a single trailing `\r` from the line before field-splitting (mirrors genome.rs which already strips `\r`), OR use leading-numeric coercion for the count fields. Pin a CRLF-cov test.

3. **Non-numeric / leading-numeric meth/nonmeth.** Perl coerces `$meth`, `$nonmeth` numerically: `"12abc"` → 12, `"abc"` → 0, `""`/undef → 0 (all with warnings, none fatal). The plan's `u32` parse will **error** on any of these. On a well-formed Bismark cov this never happens, but the plan asserts byte-identity and should state the policy: a strict parse that errors on malformed input is a *defensible accepted divergence* (Perl would silently mis-count), but it must be **documented as a deviation** (like the Phase-A `MalformedFastaHeader` divergence) and tested, not left implicit.

4. **`start` (field 1) parse.** Same coercion concern — Perl uses `$start` as a hash key (`:224`), so `"100\r"` (CRLF) becomes the **string** key `"100\r"` in Perl's `%chr`, but the lookup at `:355` uses `$pos` (an integer) → `exists $chr{...}->{$pos}` compares integer `100` against string key `"100\r"`; Perl hash keys are strings, so `$chr{...}{100}` (the lookup) and `$chr{...}{"100\r"}` (the store) are **different keys** ⇒ the CRLF position would silently **fail to find its coverage** and emit `0 0`. This is a subtle Perl bug, but it means: under CRLF, Perl emits `0 0` for covered positions; a Rust port that strips `\r` before parsing `start` would emit the *real* counts → **divergence**. This is academic (real cov is LF), but it shows the parse layer's `\r` handling has byte-identity consequences in *both* directions. **Recommend:** treat CRLF cov as out-of-contract and document it; do not silently "fix" it. Pin the decision.

5. **Too-few-fields line.** Perl `split /\t/` on a line with <6 tab fields leaves `$meth`/`$nonmeth` as `undef` → 0; `$start` may be undef. The plan does not state behavior. **Recommend:** decide (error vs Perl-style undef→0) and test V-row it.

6. **`$end` and field 3 (`undef`).** The plan correctly discards both. Perl's `undef` placeholder for field 3 is purely positional; no issue. ✅

### 1.3 Streaming flush — non-contiguous chromosome blocks (§3.1 step 3) — DIVERGENCE
The prompt's concern is real and the plan does **not** address it. Perl flushes `%chr` on every `$chr ne $last_chr` transition (`:227`), then **resets `%chr = ()`** (`:450`) and sets `$last_chr = $chr` (`:453`). If the cov has `chrA … chrB … chrA` (non-contiguous), Perl:
- flushes chrA (block 1), resets, processes chrB, flushes chrB, resets, **starts chrA again** as a fresh `$last_chr`,
- at EOF flushes chrA a **second time** — walking the *entire chrA genome sequence again* but with only the *second* chrA block's coverage in `%chr`.

So Perl emits **chrA's full genome twice** (once per block), the second time with only the later positions covered. Additionally `$processed{chrA}` is just set to 1 (idempotent), so chrA is not later re-emitted as "uncovered". **Net Perl behavior: a chromosome appearing in N non-contiguous blocks is emitted N times in the report.**

The plan's design uses a `seen: HashSet<Vec<u8>>` and "flush the just-finished chromosome … record it in `seen` … clear the buffer". If the plan's flush logic flushes purely on `chr != last_chr` (matching Perl) **without consulting `seen` to suppress**, it would reproduce Perl's double-emit — *good for byte-identity*. But if the implementer naïvely uses `seen` to *merge* or *skip* a re-seen chromosome (a "correct" dedup), it would **diverge** from Perl. The plan's wording ("record it in a `seen`") suggests `seen` is for the *uncovered* pass (correct — `seen` ⇄ Perl `%processed`), not for suppressing re-flush. **This is ambiguous and dangerous.** The Bismark cov is sorted by `bismark2bedGraph` construction so non-contiguous chr never occurs in practice (SPEC §4), but the plan asserts byte-identity and must **state explicitly**: flush is driven solely by the `chr != last_chr` transition (Perl `:227`), `seen` is used **only** to compute the uncovered set, and a non-contiguous re-appearance therefore re-emits (matching Perl). **Recommend:** pin this in §3.1 and add a V-row (even if marked "pathological / out-of-real-contract").

A second-order subtlety — **VERIFIED RESOLVED.** The uncovered pass iterates `sort keys %processed` (`:722`). I read `read_genome_into_memory:1648-1739` and confirmed Perl seeds **`$processed{$chromosome_name} = 0` for every genome chromosome at load time** (`:1712` and `:1734`). So `sort keys %processed` **is** the full genome keyset, and the plan's uncovered set = `names_sorted() − seen` is **byte-identical** to Perl's `keys %processed where !processed`. The plan's genome-driven uncovered pass is correct — no over-emit, no under-emit. (SPEC §6.6's "`%processed` tracking is Phase-B logic, not a field on the genome map" is a Rust *structural* choice — Phase B reconstructs the genome keyset via `names_sorted()` rather than carrying a `processed` flag on the map — and it lands on the identical keyset. ✅) **No longer a Critical item;** retained here only to document the verification.

### 1.4 `perl_substr` negative-wrap (§3.3, Assumption 2) — CORRECT but verify the wrap target
Forward-C at `i=0`: Perl `substr($seq, -1, 3)`. Perl `substr` with negative offset counts from the end: offset `-1` = last char, length 3 but only 1 char remains ⇒ returns the **trailing 1 byte** of the chromosome. The plan states this (`upstream = trailing 1 byte`). `ubase = upstream[0]` = the genome's **last base**. This only feeds the context summary (P3). ✅ The plan's `perl_substr` signature (negative offset, end-truncate, empty if out of range) matches. **One missed case to test:** Perl `substr($seq, -1, 3)` on a **1-byte chromosome** (`seq="C"`, `i=0`): offset -1 = index 0, returns `"C"`; `ubase='C'`. And forward-C at `i=0` on an empty-after... edge — covered by V1/V4 if they include a len-1 chromosome. **Recommend:** V4 explicitly include a 1-bp and 2-bp chromosome for the `i=0` wrap + len<3 skip interplay.

### 1.5 `revcomp` (§3.3, V2) — CORRECT
`tr/ACTG/TGAC/` = A↔T, C↔G, all else (incl. N, lowercase — but genome is uppercased) **identity**. Plan + V2 correct. Note: because the genome is uppercased on load (Phase-A genome.rs:202), lowercase never reaches revcomp; but a stray non-ACGT byte (e.g. `R`, `Y` IUPAC codes, or `.`) passes through unchanged and then hits classify, where `^C..$` matches it as CHH (or the `else` warn if it's the first base). This matches Perl exactly. ✅

### 1.6 Report-line format (§3.4) — CORRECT, one thing to pin
Perl `:408`: `join("\t", $last_chr, $pos, $strand, $meth, $nonmeth, $context, $tri_nt), "\n"`. Plan's column order `<chr>\t<pos>\t<strand>\t<meth>\t<nonmeth>\t<context>\t<tri_nt>\n` **matches** (note: SPEC §6 line at top shows the same order). No trailing tab; single `\n`; `chr` and `tri_nt` as raw bytes; `pos`/`meth`/`nonmeth` as decimal. `strand` `+`/`-`, `context` `CG`/`CHG`/`CHH`. ✅ **Pin:** the **`tetra` columns are NEVER emitted** in Phase B (the `if ($tetra)` branches at `:399`/`:414` etc. are the `--ffs` path, rejected at CLI in Phase A). The plan's kernel must hard-omit them — confirmed by §3.4 showing the 7-col form only. Good, but add a one-line note that the `$tetra` branches are dead in v1.0 so no implementer reaches for penta/hexa columns.

### 1.7 Filename derivation (§3.6, outline step 6) — CORRECT, verify the prefix concat
Phase-A `ResolvedConfig.output_dir` is a **string prefix** (`""` or absolute-with-trailing-`/`), and `output_stem` already has the context suffix stripped (cli.rs:188-198). Perl `handle_filehandles`:
- report: `"${output_dir}${cytosine_report_file}"` where `$cytosine_report_file = $stem . '.CpG_report.txt'` (`:134`) or `.CX_report.txt` (`:130`).
- summary: `"${output_dir}${context_summary_file}"` where `$context_summary_file = $stem . '.cytosine_context_summary.txt'` (`:115-116`).

Plan's `{output_dir}{output_stem}.CpG_report.txt` = string-concat of prefix + stem + suffix. **Matches Perl exactly** (Perl also string-concats `${output_dir}$file`). No double-dot risk: stem has no trailing dot (it's the `-o` value minus a `.CpG_report.txt`/`.CX_report.txt` suffix), and the literal `.CpG_report.txt` carries its own leading dot. **One edge:** if `-o foo.` (trailing dot, no recognized suffix), stem stays `foo.` → file `foo..CpG_report.txt` — but Perl does the identical concat, so byte-identical. ✅ No double-suffix risk because Phase A strips exactly one context-appropriate suffix and the plan re-appends exactly one.

### 1.8 Empty-input guard (§3.1 step 4) — CORRECT ordering
Perl: `unless (defined $last_chr){ die … }` at `:472-474` — **before** the uncovered pass (`:706-728`). So empty cov ⇒ die, even at threshold 0 (the all-zero-genome report is **not** produced). Plan reproduces this (`EmptyCoverageInput` before the uncovered pass). ✅ Matches Perl. V12 covers it.

### 1.9 Context summary `%.2f` (§3.6, §10 open) — LOW RISK, mitigation sufficient
Rust `format!("{:.2}", x)` and Perl `sprintf "%.2f"` (→ C `printf`) both operate on IEEE-754 `f64` and both use **round-half-to-even** on glibc and macOS libc. The summary domain is `m/(m+u)*100` ∈ [0,100]. The risk is a value landing *exactly* on a half-ULP boundary at the 2nd decimal — vanishingly rare and identical-rounded on both platforms anyway. The plan's mitigation (V14 unit + V15 golden) is the correct way to *prove* it rather than argue it. **Non-blocking.** If a golden ever diverges, an explicit `(x*100).round_ties_even()`-style helper is the fix, but I do not expect to need it. The same reasoning extends to the `%.6f` in Phase D (out of scope here). ✅ Mitigation sufficient.

---

## 2. Assumptions

| # | Assumption (plan §8) | Assessment |
|---|----------------------|------------|
| 1 | `pos = i+1`; substr arithmetic single source of truth | ✅ Matches Perl `pos()` semantics (`:256`/`:263`). |
| 2 | only `upstream` uses negative-wrap; `tri_nt` never negative | ✅ Reverse-G `i<2` branch (`:294`) uses `substr(seq,0,pos)` (non-negative). Correct. |
| 3 | one kernel ≡ all three Perl blocks | ✅ Independently verified (§1.1). |
| 4 | cov chr absent from genome ⇒ emit nothing | ✅ Perl `while(undef =~ /[CG]/g)` is zero iterations. But see §1.3: **does Perl still mark it `$processed{chr}=1`?** Yes (`:240` runs before the genome-walk `while`). So an absent-from-genome cov chr is marked processed and the empty walk emits nothing — plan §3.2 matches. ✅ |
| 5 | empty cov ⇒ error before uncovered pass | ✅ (§1.8). |
| 6 | `f64` `%.2f` ≡ Perl | ✅ low risk, golden-verified (§1.9). |
| — | **(unstated)** uncovered set = `names_sorted() − seen` ≡ Perl `sort keys %processed` | ⚠️ **Depends on whether Perl seeds `%processed` from the genome reader.** Must verify (§1.3, CRITICAL). |
| — | **(unstated)** cov count fields are always clean ASCII decimals (no `\r`, no junk) | ⚠️ Implicit; CRLF/malformed handling undocumented (§1.2). |
| — | **(unstated)** non-contiguous chr re-emits (matches Perl) vs dedups | ⚠️ Ambiguous; must pin (§1.3). |

---

## 3. Efficiency
- O(genome) walk, O(1) per-position `HashMap` lookup, one chromosome's cov buffer resident — matches Perl, appropriate. ✅
- `FxHashMap` (SPEC §11) for the per-chr cov buffer is a reasonable micro-opt; not required for byte-identity. The genome buffer lookup key is `u32`; `FxHashMap<u32,(u32,u32)>` is fine.
- **One concern:** the genome is held whole + each chromosome's cov buffer; for hg38 the genome is ~3 GB (Phase A). No new memory pressure in Phase B beyond a single chr's cov map. ✅
- `BufWriter` 8 KiB default is fine; consider a larger buffer (64 KiB) for the report which is large, but that is a perf-only tweak (§10.7 advisory). Not a plan change.
- The `open_report_writer` seam for Phase C is good forward design. ✅

---

## 4. Validation sufficiency (V1–V15)

**Strong coverage:** coordinate arithmetic (V1/V4/V5), revcomp (V2), classify incl. N (V3), last-base (V6), threshold (V7), CpG/CX (V8), zero_based (V9), covered-order (V10), uncovered-order+threshold-gate (V11), empty-cov (V12), absent-chr (V13), summary (V14), byte-identity golden (V15).

**Gaps to close (the prompt's #7):**
- **G1 (CRLF cov):** no test that a `\r\n` cov parses identically (or the documented divergence). **Add.** (§1.2.2/§1.2.4)
- **G2 (malformed cov line):** too-few-fields / non-numeric meth — no V-row for the decided behavior (error vs Perl-undef→0). **Add.** (§1.2.5)
- **G3 (duplicate position within a chr):** Perl `%chr` is last-write-wins (`:224-225` overwrites). The plan's `HashMap::insert` is also last-write-wins — **but no test pins it.** Add a V-row: two cov lines for the same `(chr,start)` with different counts ⇒ the **second** wins (matches Perl hash overwrite). **Add.**
- **G4 (trailing/blank line):** no test that a trailing `\n` (or a blank final line) does not spawn a phantom flush. **Add.** (§1.2.1)
- **G5 (non-contiguous chr):** no test that `chrA…chrB…chrA` re-emits chrA twice (matching Perl). **Add**, even if marked pathological/out-of-real-contract. (§1.3)
- **G6 (report-line exact bytes):** V15 is a whole-file golden, which covers this transitively, but a **focused unit test asserting the exact 7-field tab layout + single trailing `\n` + no trailing tab** for one known position would catch a column-order/separator regression faster than a 1000-line golden diff. **Add** a small exact-bytes unit test (the prompt explicitly flags this).
- **G7 (mixed covered+uncovered golden):** V15 should explicitly include a genome where **some** chromosomes are covered and **others** are not, in one run at threshold 0, to exercise the covered-then-uncovered ordering boundary in a single golden (not just separate cases). **Strengthen V15.**
- **G8 (1-bp / 2-bp chromosome):** the `i=0` upstream wrap + len<3 skip interplay on a degenerate scaffold. **Add to V4/V5.** (§1.4)
- **G9 (uncovered `%processed` keyset):** a test that confirms the uncovered pass emits **exactly** the genome chromosomes not in the cov (resolving §1.3's `%processed`-seeding question against a Perl golden). This is partly V11+V15, but the *Perl-seeding semantics* must be pinned by a golden on a multi-chr genome with a covered + an uncovered chromosome.

**V14 detail:** good that it includes `%.2f` vs `N/A`, pure-ACTG gating, 64-row sort, and the `i=0` wrap ubase. Confirm the row **sort key** is `(tri_nt, ubase)` *bytewise* matching Perl's nested `sort keys` (outer `%context_summary` keyed by tri_nt, inner by ubase) — Perl sorts tri_nt first, then ubase (`:66-67`). The plan §3.6 says "sorted by `(tri_nt, ubase)` bytewise" — ✅ matches. (All 16 tri_nt are `C__` and all 4 ubase are ACGT, so bytewise == lexical here.)

---

## 5. Alternatives considered
- **Collect-then-sort vs stream-flush:** the plan correctly rejects `BTreeMap`/collect (P1 byte-identity). Streaming is the right call and matches Perl's flush-on-transition. No better alternative for byte-identity.
- **`String::parse::<u32>` vs leading-numeric coercion** for count fields: a Perl-faithful leading-numeric coercion (parse the leading digit run, ignore trailing) would *match Perl's tolerance* of CRLF/junk; a strict parse is cleaner but a documented divergence. **Recommend the strict parse + documented divergence + trailing-`\r` strip** (simplest, and real cov is always clean) — but the decision must be explicit, not implicit.
- **Single kernel vs three transcribed blocks:** plan's single kernel is correct and avoids the dual-driver back-port trap (memory). Endorsed.

---

## 6. Action items

### Critical (resolve before implementation trigger)
- **C1 — Pin non-contiguous-chromosome flush semantics (§1.3, §4-G5).** State explicitly in §3.1 that flush is driven *solely* by the `chr != last_chr` transition (Perl `:227`); `seen` is used **only** for the uncovered set, **never** to suppress a re-flush; a chromosome reappearing in N non-contiguous blocks therefore emits N times (matching Perl, which sets `%processed` idempotently but re-walks the genome per block). Add the G5 test. Without this pin, an implementer's "sensible" dedup-on-`seen` would silently diverge from Perl. (The Bismark cov is sorted in practice, so this never fires on real data — but the plan asserts byte-identity unconditionally.)

  _(Note: the `%processed`-seeding question I flagged is **resolved-correct** — Perl seeds `$processed{chr}=0` for every genome chromosome at `:1712`/`:1734`, so the plan's `names_sorted() − seen` uncovered set is byte-identical. See §1.3. Not a Critical item.)_

### Important (fold into the plan before/at implementation)
- **I1 — Coverage-line `\r` / malformed-field policy (§1.2).** Decide and document: (a) strip a single trailing `\r` before splitting (recommended), (b) strict `u32` parse on count/start fields with a typed error on non-numeric as a **documented accepted divergence** from Perl's silent numeric coercion. Add CRLF (G1) + malformed-line (G2) tests.
- **I2 — Duplicate-position last-write-wins test (§4-G3).** Add a V-row pinning that a repeated `(chr,start)` keeps the **second** line's counts (matches Perl `%chr` overwrite at `:224-225`). The `HashMap::insert` already does this; the test guards against a future "dedup-detect" change.
- **I3 — Trailing/blank-line behavior (§1.2.1, §4-G4).** Pin + test that a file ending in `\n` produces no phantom flush; decide the mid-file-blank-line case (error acceptable; document it).
- **I4 — Exact-report-line-bytes unit test (§4-G6).** Add a focused test asserting the 7-field tab layout, single `\n`, no trailing tab, raw-byte chr/tri_nt — independent of the V15 whole-file golden.
- **I5 — Strengthen V15 to a mixed covered+uncovered golden in one run (§4-G7)** plus a 1-bp/2-bp degenerate scaffold (§4-G8) to exercise the `i=0` wrap + len<3 boundary.

### Optional
- **O1 — Note `$tetra` branches are dead in v1.0 (§1.6)** so no implementer reaches for penta/hexa columns; the kernel emits the 7-col form only.
- **O2 — Larger report `BufWriter` (64 KiB)** — perf-only, advisory (§10.7); not a byte-identity concern.
- **O3 — stderr note for cov-chr-absent-from-genome (plan §10 open):** harmless (STDERR not gated); fine to emit one line or omit.

---

## 7. Bottom line
The **kernel, coordinate arithmetic, ordering, summary, filename derivation, and empty-input guard are faithful to Perl v0.25.1** — I verified each against the source. The single-kernel equivalence claim is correct. The byte-identity exposure is **entirely at the parse/stream layer**: the `%processed`-seeding question (C1) and the non-contiguous-flush semantics (C2) are genuine divergence risks the plan does not yet pin, and the CRLF/malformed/duplicate/blank-line parse cases (I1–I3) are under-tested. None require redesign; all are "pin the Perl behavior + add a test" fixes. Resolve C1/C2 and fold I1–I5, and this plan is implementation-ready.

**Report file:** `/Users/fkrueger/Github/Bismark-c2c/plans/05292026_bismark-coverage2cytosine/phase-b-core-report/PLAN_REVIEW_B.md`
