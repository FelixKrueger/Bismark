# Plan Review B — Phase 9a: FastA input (SE + PE, all library types)

**Reviewer:** Plan Reviewer B (independent, fresh context)
**Plan:** `plans/05312026_bismark-aligner/phase9a-fasta/PLAN.md` (rev 0)
**Date:** 2026-06-02
**Verdict:** **APPROVE WITH CHANGES** — the design is sound and traces correctly to Perl on the load-bearing points (format-agnostic core, `'I'×len` QUAL for SE *and* PE, conversion shape). But there is **one Critical validation gap** (the reused fake-bowtie2 harness hardcodes the 4-line FastQ record shape and will silently mis-process 2-line FastA input), plus several Important corrections where the plan describes already-done work as new, mis-states the FastA sanity-check placement, mis-reasons about `--phred64`, and proposes a risky blind `git reset --hard`. None of these change the goal/scope; all are fixable in a rev 1.

---

## 1. Logic review

### 1.1 Central reuse claim (format-agnostic core) — VERIFIED TRUE
I grepped `merge.rs`, `methylation.rs`, `output.rs`, `report.rs` for any `ReadFormat`/`.format`/`FastA`/`FastQ` functional reference. **Zero** — the only two hits are doc comments (`output.rs:338`, `merge.rs:291`). The FLAG/XM/XR/XG/extraction/MAPQ/merge/report machinery genuinely does not read the format. The format matters in exactly four places, matching the plan's enumeration:
- conversion (`convert.rs`),
- the driver re-read arity + QUAL synthesis (`lib.rs`),
- the aligner-options `-f` (`options.rs` — **already done**, see 1.4),
- the aux record shape (`aux_out.rs`).

So the plan's spine claim is correct and the scope of new work is right *in kind*, though overstated in `options.rs` (see Important I-1).

### 1.2 QUAL synthesis — VERIFIED, including PE
- **SE:** `check_results_single_end` 2707–2709: `unless ($quality_value){ $quality_value = 'I'x(length$sequence); }`. The SE FastA re-read calls it with **no** quality arg (2361), so `'I'×len` fires. ✓
- **PE:** `check_results_paired_end` 3271–3280 has the **same** default for **both** mates, keyed on each mate's own sequence length (`length$sequence_1`, `length$sequence_2`). The PE FastA re-read (2541) passes only `(uc$seq1, uc$seq2, $id1)` — no quality args. ✓ The plan's "Same default in the PE path" is correct.
- **Layer:** The plan correctly insists the synthesis lands in the **driver** (mirroring Perl `check_results_*`), feeding raw `'I'` bytes into `single_end_sam_output`/`paired_end_sam_output`, NOT pre-decoding scores. I traced `single_end_sam_output` (`output.rs:382`): `offset = if phred64 {64} else {33}; scores = qual.wrapping_sub(offset); … if minus strand { scores.reverse() }`. For `'I'` (73): 73−33 = 40 = Phred 40, and reversing a uniform array is a no-op. ✓ The mechanism is correct.

### 1.3 Conversion mirror — VERIFIED, with one placement error
SE `biTransformFastAFiles` (5169–5306) and PE `biTransformFastAFiles_paired_end` (5308–5487) match the FastQ structure exactly except: 2-line read (`$header=<IN>; $sequence=<IN>; last unless ($header and $sequence)`), suffix `_C_to_T.fa`/`_G_to_A.fa` (+`.gz` SE only — PE gzip is **not supported**, see I-4), `>`-prefix sanity, and PE `/1/1`,`/2/2` tags. Library logic (directional C→T; pbat G→A; non-dir both; PE per-mate kind) is identical. The Rust `convert_fastq_impl` is a clean template. **But** the FastA sanity check is **per-record, not record-1-only** — see Critical-adjacent finding C-2 / I-2 below.

### 1.4 Dispatch + driver branch — sound
`pipeline()` (`lib.rs:106`) currently routes `(SingleEnd, FastQ)`→`run_se`, `(PairedEnd, FastQ)`→`run_pe`, else the deferred `_ =>` arm. Adding the two FastA arms and shrinking `_ =>` to threading-only is correct. The driver re-read branch (4-line vs 2-line) is the right seam; `drive_merge`/`drive_merge_pe` already isolate the read loop.

---

## 2. Critical findings

### C-1 🔴 The reused fake-bowtie2 harness hardcodes the **4-line FastQ record shape** — it will silently mis-process 2-line FastA and produce a green-but-wrong integration test.
This is the highest-risk gap and directly contradicts the plan's §5 step 5 and §9 rows 3–5.

Every fake in `rust/bismark-aligner/tests/cli.rs` derives qnames from the converted file with `awk 'NR%4==1 { id=$1; sub(/^@/,"",id); … }'` (lines 51, 78, 79, and the edge/ambig/PE variants at 392/451/571/594). It reads the `-U`/`-1` **converted file** and assumes **4 lines per record**.

When the converted file is now a **2-line FastA** (`>id\nseq\n>id2\nseq2\n`):
- `NR%4==1` matches line 1 (`>id`) and line 5 (`>id3`) → it **skips every other read** and emits the wrong qnames.
- `sub(/^@/,"",id)` does **not** strip the FastA `>` prefix → qnames carry a leading `>` and won't lockstep-match the driver's `>`-stripped identifiers.

Result: the merge sees mismatched/absent qnames; the test could pass trivially (everything "unmapped") while exercising none of the FastA path. The plan's hedge ("they work if they key on the index/`_G_to_A` as before, but note the file is now `.fa`") is **incorrect**: the fakes do NOT merely key on the index — they parse the converted file by 4-line arithmetic.

**Required:** add FastA-aware fakes that use `NR%2==1` and `sub(/^>/,"",id)` (SE and PE), or parameterize the existing fakes on record arity. The Phase-8 GA/strand-emitting fakes for non-dir/pbat must each get a 2-line variant. Without this, validation rows 3–6 do not actually validate FastA.

---

## 3. Important findings

### I-1 🟠 `options.rs` is **already complete** for FastA — the plan describes done work as new and frames a resolved item as "open."
`build_aligner_options` (`options.rs:24–28`) already emits `-q` for FastQ / `-f` for FastA, **first** in the option string. The phred33/phred64 × FastA die is already wired via `require_fastq` (`options.rs:37–43, 183–189`), and there are already tests `fasta_uses_dash_f` (293) and `fasta_phred33_dies` (313). I confirmed against Perl 7811: `push @aligner_options, '-f'` sits in the SEQUENCE FILE FORMAT block (7801–7823) that runs **before** all other option assembly, so `-f` leads — exactly the Rust order.

Consequences:
- §2.4 "New work … `options.rs` — emit `-f`" and §5 step 2 should become **verify-only**.
- §10 open question "the exact `-f` token + position in `aligner_options`" is **resolved**: token = `-f`, position = first. Move it to the Resolved list. The plan should not carry this as an open risk into implementation.

### I-2 🟠 The FastA format sanity check is **per-record** in Perl, not record-1-only — the plan's §3.1 step 6 is wrong on both counts.
The plan says "record 1's `header` must start with `>` … Confirm Perl actually does a FastA record-1 sanity; if not, OMIT it." Both halves are wrong:
- Perl **does** the check (SE 5271, PE 5414): `die "Input file doesn't seem to be in FastA format at sequence $count" unless ($header =~ /^>.*/)` (SE) / `=~ /^>/` (PE).
- It runs on **every** record (no `if ($count == 1)` guard) — contrast the **FastQ** path, which *is* record-1-only (`if ($count == 1)` at 5612). It sits **after** skip/upto, so a skipped record's `next` bypasses it (same as FastQ).

The existing Rust `convert_fastq_impl` does record-1-only sanity (`if count == 1 …`, 344). A correct FastA mirror must check `^>` on **every non-skipped record** (and there is no `+`/id2 check). This is a behavioral difference (a malformed record N>1 must die for FastA but is written verbatim for FastQ), so a shared core must parameterize the sanity rule, not just the prefix byte.

### I-3 🟠 `--phred64`/`--phred33` with `-f` are **rejected** by Perl — the plan's "phred64 inert with synthesized `'I'`" edge case is moot and mis-reasoned.
Perl 7848–7852: `if ($phred64){ unless ($fastq){ die "Phred quality values work only when -q (FASTQ) is specified\n"; } }` (and the same for `--phred33`, 7840–7844). Since `-f` sets `$fasta` and leaves `$fastq` falsy, `-f --phred64`/`-f --phred33` **die before any alignment**. So the QUAL never reaches the SAM path under those flags; there is nothing to make "inert." (Aside: were it not rejected, `'I'` would NOT be inert under phred64 — 73−64 = 9 ≠ 73−33 = 40; the plan's claim that phred64 is "inert with synthesized `'I'`" is simply false. It happens to be irrelevant because the combination dies.)
The Rust already enforces this die (`options.rs` `require_fastq`), so §3 edge-case bullet "FastA + `--phred64`" should be rewritten as: *confirmed rejected by both Perl and the existing Rust; add/keep a test asserting the die*, not as a QUAL-handling concern.

### I-4 🟠 PE FastA **gzip is not supported** by Perl (warns + writes uncompressed) — the plan does not mention this.
`biTransformFastAFiles_paired_end` 5311–5314: `if ($gzip){ warn "GZIP compression of temporary files is not supported for paired-end FastA data. Continuing to write uncompressed files\n"; sleep(2); }`, and the PE FastA suffix is unconditionally `_C_to_T.fa`/`_G_to_A.fa` (5343–5344, **no** `.gz`). SE FastA *does* honor `--gzip` (5198–5205). The plan's §3.1 says "gzip (pbat⊕gzip already dies)" but does not capture the **PE-FastA-gzip → uncompressed-with-warning** behavior. The PE conversion must ignore `--gzip` (write plain `.fa`), and the SE/PE conversion cores therefore diverge on the gzip-suffix rule. Flag this so an implementer using one shared core does not wrongly gzip PE FastA temps.

### I-5 🟠 Report/BAM filename suffix strip is **FastQ-only** — a real byte trap the plan omits.
Perl report-name strip (1622) `s/(\.fastq\.gz|\.fq\.gz|\.fastq|\.fq)$//` does **not** strip `.fa`/`.fasta`. So `reads.fa` → report `reads.fa_bismark_bt2_SE_report.txt` and BAM `reads.fa_bismark_bt2.bam` (the `.fa` is retained). The Rust `strip_fastq_suffix` (`lib.rs:412`) is FastQ-only and thus **already correct** — but the plan never names this trap, and a "helpful" implementer might add `.fa`/`.fasta` to the strip list, silently breaking byte-identity. §3.5 should explicitly state: **do NOT extend `strip_fastq_suffix` for FastA**; the `.fa` extension stays in the BAM/report stems.

### I-6 🟠 The `git reset --hard origin/rust/iron-chancellor` instruction is risky as written.
The worktree HEAD is `81bd408` ("Merge remote-tracking branch 'origin/rust/iron-chancellor' into rust/aligner"), and Phase 8 (`d4568f3`) is present in the local history. The plan asserts Phases 1–8 were squash-merged into iron-chancellor as `c865320`, but the current branch carries a **merge commit** ahead of origin, not a clean reset point. A blind `git reset --hard origin/rust/iron-chancellor` could discard `81bd408` and risks re-surfacing or losing Phase-8 state if `c865320` ≠ what's locally merged. **Before any reset**, the plan should require verifying that `origin/rust/iron-chancellor` actually contains the Phase-8 changes (e.g. confirm `merge.rs`/`methylation.rs`/PE non-dir code are in `origin`), or simply build 9a on the current tip and let the PR diff be reviewed normally. Do not gate implementation on a destructive reset that hasn't been validated.

---

## 4. Optional findings

### O-1 Shared-core refactor: parameterize record arity *and* the sanity rule, not just the suffix byte.
The plan's §4 suggests a `RecordShape { FastQ, FastA }` param. Good — but per I-2 (per-record vs record-1-only sanity) and I-4 (PE gzip divergence), the shape param must also drive (a) the sanity-check cadence/predicate, (b) whether id2/qual lines exist, (c) the PE-gzip-suppression rule. If those don't fit cleanly into one core, a thin separate `convert_fasta_impl` (the plan's alternative) is *less* risk to the byte-frozen FastQ path — I'd lean that way given the FastQ paths are the regression guard. Either is acceptable; just don't let the refactor touch the FastQ branch's observable behavior.

### O-2 SE vs PE aux `>`-prefix provenance differs (byte-equivalent, worth a code comment).
SE FastA aux re-prepends `>` to the `>`-stripped id (`">$identifier\n"`, 2369), while PE FastA aux uses the **un-stripped** `$orig_identifier_*` saved before `s/^>//` (2549–2552). Net bytes are identical (both start with `>`), but the new `write_fasta_record` + driver wiring should document which id it receives so a future change doesn't desync SE/PE.

### O-3 §9 row 8 (oxy gate) overlaps Phase 10.
The EPIC puts the full-scale real-data oxy gate in **Phase 10**. A lighter FastA-converted-subset smoke is reasonable as a 9a sanity, but label it as such (not "the gate") to avoid implying 9a owns the full byte-identity gate.

### O-4 Record-count parity for PE FastA.
PE FastQ requires both files to have equal record counts (implicitly, via lockstep). PE FastA inherits this. A truncated/mismatched-length FastA pair should be handled exactly as FastQ (lockstep `last unless …`). Worth an explicit unit test since FastA's 2-line cadence makes off-by-one truncation easier to introduce.

---

## 5. Assumptions audit

| Plan assumption | Status |
|---|---|
| FastA QUAL = `'I'×len` (Phred 40), SE **and** PE | ✅ verified (2707–2709, 3271–3280) |
| Strand/merge/FLAG/XM/report code is format-agnostic | ✅ verified (zero functional format refs in merge/methylation/output/report) |
| Converted suffix `.fa`/`.fa.gz` | ⚠️ SE yes; **PE = `.fa` only, never `.gz`** (I-4) |
| Aligner flag `-f`, leads option string | ✅ verified (Perl 7811; Rust `options.rs` already emits it first) |
| `-f` token/position "open" | ❌ already resolved + implemented (I-1) |
| `options.rs` is new work | ❌ already complete (I-1) |
| FastA record-1 sanity "confirm/omit" | ❌ Perl checks `^>` on **every** record (I-2) |
| `-f --phred64` "inert" | ❌ rejected by Perl + Rust; reasoning is also wrong (I-3) |
| Aux filename `.fa.gz` already FastA-aware | ✅ verified (`aux_out.rs:48`, Perl 1663/1697 + `.gz`) |
| Report header may state input format | ❌ report body has no FastA/FastQ wording (1642 + 1711–1728 are format-agnostic) — non-issue |
| FastQ paths byte-frozen | ✅ guarded by existing suite, provided the shared-core refactor doesn't touch the FastQ branch (O-1) |

---

## 6. Validation sufficiency

Will the proposed validations catch the highest-risk failure modes?
- **Wrong QUAL** (e.g. synthesizing decoded scores, or `*` instead of `I`): caught by §9 rows 3–4 *only if* the fakes actually feed FastA reads through — blocked by **C-1**. After C-1 is fixed, the `QUAL = IIIIII` byte-assert is a good guard.
- **Wrong `-f`/regression on `-q`**: covered by the existing `options.rs` tests (already present) + §9 row 2; sufficient.
- **Mis-mirrored conversion** (suffix, `>` prefix, per-record sanity, PE gzip, `/1/1`/`/2/2`): §9 row 1 covers the happy path; **add** explicit tests for (a) per-record `^>` die at record N>1 (I-2), (b) PE-gzip→uncompressed (I-4), (c) truncated PE pair (O-4). As written, row 1 would miss I-2/I-4.
- **FastQ regression**: §9 row 7 (existing suite byte-frozen) is the right guard; keep it as a hard gate.
- **Filename trap**: no row guards against an implementer extending `strip_fastq_suffix` for `.fa` (I-5). Add a unit assertion that a `.fa` input yields a `reads.fa_…report.txt`/`reads.fa_…bam` stem.

Net: the validation **plan is insufficient as written** because (1) its integration harness can't actually exercise FastA (C-1), and (2) it has no assertions for the per-record sanity, PE-gzip, and filename-strip traps. All are addressable.

---

## 7. Efficiency

No concerns. FastA is half the lines of FastQ; conversion + re-read stay O(reads); no extra genome passes or instances; mimalloc already global. The shared-core refactor (O-1) is a code-organization choice, not a perf one.

---

## 8. Alternatives

- **9a/9b split:** sound and matches the EPIC (Phase 9 explicitly split; threading + worker-invariance gate are 9b). FastA is a per-record format branch, orthogonal to the per-file threading wrapper. ✓
- **Shared core vs duplicated FastA fns:** given the per-record-sanity (I-2) and PE-gzip (I-4) divergences, a thin separate `convert_fasta_impl` arguably protects the byte-frozen FastQ path better than threading a `RecordShape` enum through the FastQ core. Either works; the deciding factor is "never perturb the FastQ branch." I'd document that as the constraint and let the implementer pick.
- **Build-on-current-tip vs hard-reset:** prefer building on the current tip (I-6) unless the squash-merge equivalence is positively confirmed.

---

## 9. Action items

**Critical (must fix before implementation):**
- **C-1** Add FastA-aware fake bowtie2 scripts (`NR%2==1`, `sub(/^>/,"",id)`) for SE and PE — including 2-line variants of the Phase-8 non-dir/pbat strand fakes — or the integration tests (§9 rows 3–6) validate nothing for FastA.

**Important (fix in rev 1):**
- **I-1** Reclassify `options.rs` work + the `-f` token/position question as **verify-only / resolved** (`-f`, first position; phred×FastA die already wired + tested).
- **I-2** Correct §3.1 step 6: FastA `^>` sanity runs on **every** non-skipped record (Perl 5271/5414), not record-1-only; no `+`/id2 check. The shared core must parameterize the sanity cadence, not just the prefix byte. Add a test for a malformed record N>1 dying.
- **I-3** Rewrite the `--phred64` edge case: `-f --phred33`/`-f --phred64` are **rejected** by Perl (7840–7853) and already by Rust; assert the die. Drop the "inert `'I'`" reasoning (it's false and irrelevant).
- **I-4** Capture PE-FastA-**gzip** behavior: warn + write **uncompressed** `.fa` (Perl 5311–5314, 5343–5344). PE FastA must ignore `--gzip`; SE FastA honors it. Add a test.
- **I-5** State the filename trap: do **NOT** add `.fa`/`.fasta` to `strip_fastq_suffix`; the `.fa` stays in BAM/report stems (Perl 1622). Add a stem-name assertion.
- **I-6** Replace the blind `git reset --hard` with: verify `origin/rust/iron-chancellor` contains Phase 8, or build on the current tip; do not gate on a destructive reset.

**Optional:**
- **O-1** If using a shared core, thread the sanity rule + PE-gzip rule + record arity through it; otherwise prefer a separate `convert_fasta_impl` to protect the byte-frozen FastQ branch.
- **O-2** Document the SE-vs-PE `>`-prefix id provenance in `write_fasta_record`/driver.
- **O-3** Label §9 row 8 as a FastA smoke, not "the gate" (full oxy gate = Phase 10).
- **O-4** Add a truncated/mismatched-length PE FastA lockstep test.

---

## 10. Verdict

**APPROVE WITH CHANGES.** The architecture is correct and the load-bearing Perl traces (format-agnostic core, `'I'×len` QUAL for SE + PE, conversion mirror) check out. Address **C-1** (FastA-aware fakes — without it the integration tests are vacuous), fold in the Important corrections (especially I-1 done-work reclassification, I-2 per-record sanity, I-4 PE gzip, I-5/I-6 traps), and the plan is implementation-ready. None of the findings change the goal or scope.
