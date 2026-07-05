# PLAN_REVIEW_A — Phase 9a: FastA input (SE + PE, all library types)

**Reviewer:** A (independent, fresh context)
**Plan:** `plans/05312026_bismark-aligner/phase9a-fasta/PLAN.md` (rev 0)
**Verdict:** **APPROVE-WITH-FINDINGS**

The plan is well-traced, correctly scoped, and its core thesis ("FastA = FastQ minus the quality line, plus `.fa` suffix, `-f`, and a synthesized Phred-40 QUAL; all strand/merge/FLAG/XM/report machinery reused unchanged") is **verified against the Perl and the existing Rust** — it holds. The QUAL story is correct, the aux story is correct, the format-agnostic-reuse claim is correct. There are, however, **two factual misstatements** that should be corrected before implementation (one a real correctness item, one a stale "open" item that is already done), plus several smaller clarifications. None of them changes the scope or the goal.

I verified every Perl line the plan cites and read the relevant Rust (`options.rs`, `config.rs`, `convert.rs`, `lib.rs` drivers, `aux_out.rs`, `output.rs`).

---

## 1. Logic review

### What checks out (verified, not taken on faith)

- **QUAL synthesis (the headline risk).** Perl `check_results_single_end` 2707–2708 `unless ($quality_value){ $quality_value = 'I'x(length$sequence); }` — confirmed. The **PE equivalent exists** at `check_results_paired_end` **3274–3280** (`$quality_value_1`/`_2` each defaulted to `'I'x(length)`) — the plan's claim "Same default in the PE" is **correct** (line ref 2707 in §3.3 is SE-only; the PE defaults are at 3274/3279 — minor citation gap, content right). Feeding uniform `b"I"` through the Rust SAM writer is correct: `output.rs:382-383` does `q - offset` with `offset=33` (default) → `0x49 - 33 = 40` = Phred 40. The minus-strand `scores.reverse()` (`output.rs:392`, `:616`) is a genuine no-op on a uniform string. **The plan's QUAL design is sound and lands at the right layer** (driver/merge, not the SAM writer) — confirmed the SAM writer takes a raw ASCII `qual: &[u8]` and applies the offset itself, so synthesizing `'I'×len` upstream is exactly right.

- **Format-agnostic reuse.** Verified the SE FastA bowtie2 sub (`bismark` 6729) issues a **byte-identical** bowtie2 invocation to the FastQ one (`-U $temp_dir$fh->{inputfile}`, same `--norc`/`--nofw`, same `$aligner_options`). The only deltas are the `.fa` inputfile name and `-f` in `aligner_options`. The Rust `align.rs`/merge/`output.rs`/report code does not branch on format. **Reuse claim verified.**

- **Report is format-agnostic.** The report header (`bismark` 1642 / 1007 / 1010) is `Bismark report for: $sequence_file (version: …)` — **no format-specific wording**. The "Input … in FastA format" strings at 338/393/497/518 are **stderr `warn`s**, not in the report file. §3.5's worry ("the report header may state the input format") is **unfounded** — pure verify-only, no FastA report wording. Good to downgrade this risk.

- **Aux extension `.fa.gz`.** Verified: the final unmapped/ambiguous output is **always gzipped** (`merge_individual_*` 1293 `open(…,"| gzip -c -")`; suffix `_unmapped_reads.fa.gz` / `_ambiguous_reads.fa.gz` at 1332/1340/1351 etc.). Rust `aux_filename(…, fasta, …)` already selects `fa`/`fq` (`aux_out.rs:48`) and appends `.gz`. §3.4's claim is **correct, verify-only.**

- **2-line conversion + truncated-tail drop + `/1/1`,`/2/2`.** Verified against `biTransformFastAFiles` 5245–5290 (SE) and `biTransformFastAFiles_paired_end` 5388–5455 (PE): 2-line read, `last unless ($header and $sequence)`, `chomp`+`fix_IDs`+`$header.="\n"`, `uc$sequence`, per-mate `s/$/\/1\/1/`,`/2/2`, library logic (dir C→T; pbat G→A; non-dir both), suffix `_C_to_T.fa`/`_G_to_A.fa`(+`.gz` SE only). The plan mirrors all of this correctly. **PE-FastA gzip is rejected with a warn-and-continue-uncompressed** (5311–5314), distinct from SE — the plan does not call this out (see Important #3).

### What is WRONG (real correctness item)

- **🔴 CRITICAL — the FastA record sanity check is PER-RECORD, not record-1-only, and must NOT be omitted.** §3.1 step 6 says *"record-1 sanity: record 1's header must start with `>` … Confirm Perl actually does a FastA record-1 sanity; if not, OMIT it."* This mischaracterizes Perl in **both** directions:
  - Perl FastA dies **on every record**, unconditionally: `bismark` **5271** (SE) `die "… FastA format at sequence $count …" unless ($header =~ /^>.*/);` and **5414** (PE) `… unless ($header =~ /^>/);`. There is **no** `if ($count == 1)` guard.
  - By contrast, Perl FastQ **is** record-1-only: 5612/5769/5938/6155 all wrap the die in `if ($count == 1){ … }`. The existing Rust mirrors *that* (FastQ) at `convert.rs:345` (`count == 1 && …`).
  - **Consequence:** the FastA conversion must check `header.starts_with(b">")` on **every** record (placed after `uc$sequence` + tab-detect, matching Perl order 5263→5271), not copy the FastQ record-1-only pattern. A malformed record 2 must **die** under FastA (it passes verbatim under FastQ — see the FastQ test `convert.rs:530`). If the implementer follows §3.1 step 6 literally (record-1-only, or omit), the die behavior will diverge from Perl. **Fix the plan's step 6 to specify per-record `^>`.** Note also SE uses `/^>.*/` and PE uses `/^>/` (functionally equivalent — `starts_with(b">")` covers both).

### What is STALE (already implemented — not "new work", not "open")

- **🟠 IMPORTANT — `options.rs` `-f` is already DONE and tested; remove it from "the bulk" and from the open-items list.** §2 lists `options.rs` under "New work (the bulk)"; §3.2/§5 step 2/§6 step 6/§10 flag *"confirm the exact `-f` token + position … (Open, high)"*. In fact:
  - `options.rs:25-28` already pushes `-f` for `ReadFormat::FastA` as the **first** option (same slot as `-q`), matching Perl 7808–7811 (`push @aligner_options, '-f'` — first push, before phred/`-N`).
  - The `-f --phred33/--phred64` **die** is already implemented (`require_fastq`, `options.rs:183-188`) and matches Perl 7848–7852 (`if ($phred64){ unless ($fastq){ die "Phred quality values work only when -q (FASTQ) is specified" } }`).
  - There are already passing tests: `fasta_uses_dash_f` (`options.rs:293-296`, asserts `"-f --score-min … --ignore-quals"`) and `phred33_with_fasta_dies` (`options.rs:313-314`).
  - **So the §10 open item "exact `-f` token + position" and "`-f --phred64` rejected?" are both RESOLVED in code today.** The plan should reclassify `options.rs` as verify-only and the `-f`/phred open items as **closed** (Perl-pinned: `-f` leads; `--phred33`/`--phred64` with FastA dies). This matters because it misdirects implementation effort and inflates the risk register; the actual new work in `options.rs` is **zero**.

---

## 2. Assumptions

- **"`aux_filename` already FastA-aware"** — true (`aux_out.rs:40-48`). ✓
- **"`-f ⊕ -q` dies; `--pbat ⊕ -f` dies"** — true (`config.rs:294-300`, `:341-344`), **but the Rust die *messages* differ from Perl's** (Rust: "Please specify either -q/--fastq OR -f/--fasta, not both." vs Perl 7804 "Only one sequence filetype can be specified (fastA or fastQ)"; and the `--pbat`+`-f` message differs from Perl too). These are **stderr strings, not part of the SAM/report byte gate**, so they do not threaten byte-identity. Flag as a known, accepted divergence (the gate is SAM/BAM/report/aux content, not stderr). Not blocking. ✓ assumption holds for the gate.
- **"FastA QUAL = `'I'×len`"** — confirmed (SE 2707-2708, PE 3274-3280). ✓
- **"library logic identical to FastQ"** — confirmed (SE 5273-5288; PE 5429-5454). ✓
- **"strand/merge/FLAG/XM/report format-agnostic"** — confirmed (aligner sub 6729 byte-identical invocation; report header 1642). ✓
- **Unstated but true:** the SE re-read (`process_single_end_fastA_…` 2336-2342) chomps `$sequence` and `$identifier` *before* count/skip/upto and does **not** re-append `\n` to the id (unlike FastQ 2422), then `s/^>//` at 2359. The Rust driver already chomps + strips `@`; for FastA it just strips `>` instead. The aux write uses the **non-uppercased** chomped `$sequence` (2370/2375) — and the Rust FastQ aux already uses `seq_orig = chomp_newline(&seq)` (non-uc), so `write_fasta_record` must feed that same non-uc seq. The plan's `>id\nseq` is correct **provided** `seq` is the non-uc chomped original. Worth stating explicitly (see Optional #2).

---

## 3. Efficiency

No concerns. FastA records are half the lines; conversion + re-read remain O(reads); no extra genome passes or instances. The shared-record-shape-core refactor (§4/§5/§10) is the right efficiency-and-correctness call — it keeps the FastQ path byte-frozen by construction rather than by copy-paste drift. mimalloc already global. The plan correctly defers `--multicore`/`subset_input_file_FastA` to 9b.

---

## 4. Validation sufficiency

The validation table (§9) is strong and targets the right risks (QUAL=`IIIIII`, `-f` token, conversion bytes, FastQ regression freeze, oxy gate on FastA-converted real subsets). Gaps to close:

1. **(maps to Critical #1)** Add a **negative conversion test**: a FastA input whose record 2 header lacks `>` must **die** (per-record sanity, Perl 5271/5414). And a record-1-lacks-`>` die. Without this, the per-record-vs-record-1 bug ships silently (the happy path never exercises it).
2. **No test pins the `s/^>//` id-strip in the re-read.** Add an assertion that the BAM QNAME has **no** leading `>` (and, by symmetry, no leading `@`) — i.e., a FastA id `>read1` lands as `read1` in column 1. Cheap, guards the `strip_prefix(b">")` swap.
3. **Aux content for FastA must assert the 2-line shape AND the non-uc sequence** (`>id\n<original-case seq>\n`), not just the `.fa.gz` filename (row 6 only mentions the 2-line shape + name). If the implementer accidentally writes the uc seq, the gate would catch it on real data, but a unit assertion is cheaper.
4. **`-f --phred64` / `-f --phred33` die** — already covered by the existing `options.rs` test for phred33; **add the phred64 case** for parity (Perl dies on both). Minor.
5. **Empty FastA input → header-only BAM** is listed (§Edge cases) but not in the table; add a row (one-liner, mirrors the FastQ empty-input behavior).
6. The **oxy gate (row 8)** is the real backstop and is well-specified (seqtk/awk FastQ→FastA, `-f`, 3 libraries × SE/PE, identical argv, byte-identical BAM+report+aux). Note in the plan that the FastA conversion **must preserve the same read order** as the FastQ subset so the BAM is positionally comparable (it will, since it's a 1:1 line transform). Sufficient.

With items 1–3 added, validation is sufficient to catch a wrong QUAL, a wrong `-f` placement (already locked by existing tests), a mis-mirrored conversion (incl. the per-record sanity), and a FastQ regression.

---

## 5. Alternatives

- **Shared `RecordShape`-parameterized core vs separate FastA functions (§10 open).** The plan's leaning (shared core) is the **right** call and I'd elevate it from "decide at implementation" to a recommendation: the existing `convert_fastq_impl` already takes `(kind, id_suffix, file_base)`; adding a `shape: RecordShape { FastQ, FastA }` that controls (a) line arity (4 vs 2), (b) extension (`.fastq`/`.fa`), (c) header prefix sanity (`@` record-1-only vs `>` per-record), and (d) whether a `+`/qual line is read/written, keeps **one** path and makes the FastQ-byte-freeze structural. The same `RecordShape` can drive the `drive_merge` re-read arity + the QUAL-synthesis branch. This minimizes the surface where FastQ could regress.
- **QUAL synthesis location** — the plan already picks the correct layer (driver/merge, mirroring Perl `check_results_*`). The alternative (synthesize inside `single_end_sam_output`) is correctly rejected in §11; endorse.
- No alternative to the 9a/9b split is warranted — the EPIC (lines 67-69, 86-87) sanctions it and the threading concern is genuinely orthogonal to the per-record format branch.

---

## 6. Action items (prioritized)

### Critical (fix before implementation)
- **C1.** Correct §3.1 step 6: the FastA record sanity check (`header starts with '>'`) runs **per-record / on every record** (Perl 5271 SE `/^>.*/`, 5414 PE `/^>/`), **not** record-1-only, and must **not** be omitted. This is the FastQ-vs-FastA asymmetry (FastQ sanity is `if($count==1)`; FastA is unconditional). Add the negative tests (Validation #1 — malformed record-2 must die under FastA).

### Important
- **I1.** Reclassify `options.rs` as **verify-only / already-done**, and mark the §10 open items "exact `-f` token+position" and "`-f --phred64` rejected?" as **RESOLVED**: `-f` is already pushed first (`options.rs:25-28`, Perl 7808-7811) and `-f`+`--phred33/64` already dies (`require_fastq`, `options.rs:183-188`, Perl 7848-7852), with passing tests `fasta_uses_dash_f` and `phred33_with_fasta_dies`. Remove it from "the bulk" (§2) and from §6 step 6.
- **I2.** Note the **PE-FastA-specific gzip behavior**: `biTransformFastAFiles_paired_end` 5311-5314 **warns and continues uncompressed** if `--gzip` is set (PE FastA temps are never gzipped), and the PE suffix is always `_C_to_T.fa`/`_G_to_A.fa` (no `.gz`, 5343-5344) — unlike SE FastA which honors `--gzip` (5198-5205). The shared core's gzip handling must respect this PE-only override.
- **I3.** Add Validation rows for the **id-strip** (`>read1` → QNAME `read1`), the **FastA aux non-uc 2-line content**, and **empty FastA input** (Validation #2, #3, #5).

### Optional
- **O1.** Fix the minor citation in §3.3: the PE QUAL default is at `check_results_paired_end` **3274-3280**, not 2707 (2707 is SE-only). Content is right; reference should point PE implementers to the correct spot.
- **O2.** State explicitly that `write_fasta_record` must feed the **non-uppercased chomped** original `seq` (matching Perl 2370/2375 and the existing FastQ aux `seq_orig`), not `seq_uc`.
- **O3.** Note that the Rust `-f⊕-q` and `--pbat⊕-f` **die messages differ from Perl** but are stderr-only (outside the byte gate) — accepted divergence, document it so a future reviewer doesn't re-flag.
- **O4.** Endorse the shared-`RecordShape` core in §10 as the chosen approach (not merely a candidate), since it makes the FastQ byte-freeze structural.

---

## 7. Summary

The plan's central design (QUAL `'I'×len` synthesized at the driver; `.fa`/`-f`; reuse of all format-agnostic machinery) is **correct and verified end-to-end against Perl v0.25.1 and the existing Rust**. Two factual fixes are needed: the **per-record FastA sanity die** (Critical — a genuine behavior divergence if implemented as written) and the **stale `options.rs`/`-f` "open" framing** (Important — already done and tested; correct the plan so effort and risk are not misdirected). With those corrected and three validation rows added, this is ready to implement.

**Verdict: APPROVE-WITH-FINDINGS.**
