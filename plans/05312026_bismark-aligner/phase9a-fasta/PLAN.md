# PLAN — Phase 9a: FastA input (SE + PE, all library types) 🎯

## 1. Goal

Add **FastA input** (`-f`/`--fasta`) to the Rust `bismark` aligner for both SE and PE,
across all three library types (directional / non-directional / pbat), **byte-identical**
to Perl `bismark` v0.25.1 + Bowtie 2 2.5.5. FastA differs from FastQ only in the *record
shape* (2-line `>id` / `seq`, no quality), the *converted-file suffix* (`.fa` not `.fastq`),
the *aligner format flag* (`-f` not `-q`), and a *synthesized QUAL* (Phred 40 = `'I'` ×
read-length). All strand/merge/FLAG/XM/report machinery is reused unchanged.

> **Epic:** `05312026_bismark-aligner/EPIC.md`, Phase 9 — *FastA input + order-preserving threading*, split into **9a (FastA, this plan)** and **9b (threading)** per Felix. Order-preserving `--multicore`/`--parallel` threading + the worker-invariance gate are **Phase 9b** and explicitly OUT of scope here.

## 2. Context

### Placement / dependencies
- **Depends on Phases 1–8** (SE + PE FastQ, all library types — squash-merged to `rust/iron-chancellor` as `c865320`). 🔴 **Before implementing 9a, re-base the `rust/aligner` worktree onto `origin/rust/iron-chancellor`** so 9a builds on the squashed base (else the next PR re-surfaces Phases 1–8). rev1 B I-6: do NOT blind `reset --hard` — the local tip is the freshen merge `81bd408` (ahead of origin, contains Phase 8 `d4568f3`). First `git fetch origin` and **verify content-equivalence** (`git diff origin/rust/iron-chancellor -- rust/` should be empty — Phase 8 is in `c865320`), THEN `git reset --hard origin/rust/iron-chancellor`. If the diff is non-empty, investigate before resetting.
- Worktree `~/Github/Bismark-aligner`, branch `rust/aligner`; crate `rust/bismark-aligner` (bin `bismark_rs`). cargo/git there need `dangerouslyDisableSandbox`.

### Already in place (verify-only)
| Component | Status | Evidence |
|---|---|---|
| `ReadFormat::FastA` resolves (`-f`/`--fasta`) | ✅ `config.rs:340–351` | `resolve_format` |
| `-f ⊕ -q` dies; `--pbat ⊕ -f` dies | ✅ `config.rs:294–300, 341–344` | Perl 8155 |
| `aux_filename(..., fasta: bool, ...)` already FastA-aware | ✅ `aux_out.rs` (Phase 6) | the `fasta` arg already flips the unmapped/ambiguous extension |
| Strand/merge/FLAG/XM/XR/XG/report machinery is **format-agnostic** | ✅ Phases 4–8 (both reviewers: `merge`/`methylation`/`output`/`report` have ZERO functional format refs) | none of it reads the format |
| **Bowtie 2 `-f` flag** for FastA (instead of `-q`) | ✅ **rev1**: `options.rs:25–28` already pushes `-f` **first** (Perl 7808–7811) — verify-only | test `fasta_uses_dash_f` |
| **`-f ⊕ --phred33/--phred64` dies** | ✅ **rev1**: `options.rs:183–188` `require_fastq` (Perl 7840–7853) — verify-only | test `phred33_with_fasta_dies` |
| `strip_fastq_suffix` is FastQ-only **by design** (Perl 1622 does NOT strip `.fa`) | ✅ already correct | 🔴 **do NOT extend it for FastA** (rev1 B I-5) |

### New work (the bulk)
- **`convert.rs`** — a 2-line FastA conversion core + FastA entry points mirroring the FastQ ones.
- **`lib.rs`** — format-branch the per-mode conversion + the driver re-read loops (4-line FastQ vs 2-line FastA) + synthesize the Phred-40 QUAL + the 2 dispatch arms.
- **`aux_out.rs`** — a 2-line FastA record writer for `--unmapped`/`--ambiguous`.
- (**`options.rs` is NOT new work** — the `-f` flag + the phred-die are already implemented + tested; rev1 A/B I-1.)

### Perl source of truth (`~/Github/Bismark-aligner/bismark`, v0.25.1)
- SE FastA conversion `biTransformFastAFiles` **5169–5307**; PE `biTransformFastAFiles_paired_end` **5308+**. 2-line read (`$header=<IN>; $sequence=<IN>; last unless ($header and $sequence)`), `chomp`+`fix_IDs`, suffix `_C_to_T.fa`/`_G_to_A.fa` (+`.gz`), library logic identical to FastQ (directional C→T; pbat G→A; non-dir both).
- SE FastA methylation-call re-read `process_single_end_fastA_file_for_methylation_call` **2317–2483**: 2-line read, strip leading `>` (`s/^>//`), `check_results_single_end(uc$sequence,$identifier)` — **no quality arg**. PE `process_fastA_files_for_paired_end_methylation_calls` **2484+**.
- 🎯 **FastA QUAL** `check_results_single_end` **2707–2709**: `unless ($quality_value){ $quality_value = 'I'x(length$sequence); }` → **Phred 40 (`'I'`) × read length**. PE has the SAME default **per mate** in `check_results_paired_end` **3271–3280** (rev1: both reviewers pinned the PE lines).
- 🔴 **FastA record sanity is PER-RECORD, not record-1-only** (rev1 A/B, corrects §3.1): Perl FastA dies on **every** non-skipped record whose header is not `^>` (SE **5271**, PE **5414**; no `if ($count==1)` guard) — UNLIKE FastQ which checks only record 1 (5612). A malformed record 2 must die under FastA but passes verbatim under FastQ.
- Format dispatch `$sequence_file_format eq 'FASTA'` at 337/496 (conversion), 746–779 (aligner launch → `-f`), 1737/1955 (report + which methylation-call fn).
- aux output for FastA: `process_single_end_fastA_…` writes `">$identifier\n$sequence\n"` (2-line) to AMBIG/UNMAPPED (2454–2466).

## 3. Behavior (numbered)

### 3.1 Read conversion — FastA (`convert.rs`)
A 2-line analog of `convert_fastq_impl`. Per record (Perl 5245–5290):
1. Read **2** lines: `header`, `sequence`. `break` if either is empty (truncated tail dropped — Perl `last unless ($header and $sequence)`).
2. `count += 1`.
3. ID: `chomp_newline` → `fix_id(icpc)` → (PE only) append `/1/1`|`/2/2` → re-append `\n`. **Header prefix is `>`** (not `@`).
4. skip/upto (Perl-falsy 0), same as FastQ.
5. max-length guard (mm2-only, inert) — same.
6. 🔴 **PER-RECORD `^>` sanity** (rev1 A/B — corrected): on **every** non-skipped record, the `header` must start with `>`, else die (Perl SE 5271 / PE 5414, NO record-1 guard). This DIFFERS from FastQ's record-1-only `@` check (`convert.rs:345`) — the FastA core must NOT copy the record-1-only pattern. There is no `id2`/`+` check (no `+` line).
7. Write: `header` (with suffix), then `convert_one(seq, kind)` + `\n`. **No `+`/qual lines.**
- Filenames: `<prefix.>?<basename>_C_to_T.fa[.gz]` / `_G_to_A.fa[.gz]` (suffix `_C_to_T.fa`, not `_C_to_T.fastq`).
- Library variants (identical pattern to FastQ): SE directional → C→T; SE pbat → G→A; SE non-dir → C→T + G→A; PE per-mate library-aware kind (`pe_kind`), `/1/1`,`/2/2` tags retained.
- 🔴 **gzip is SE-only for FastA** (rev1 B I-4): SE FastA honors `--gzip` (writes `.fa.gz`, Perl 5198–5205); **PE FastA does NOT** — Perl warns and writes uncompressed `.fa` (5311–5314, 5343–5344). The shared core must gate gzip off for PE FastA (warn-and-continue), unlike PE FastQ.
- Edge cases: empty input → empty output; CRLF (`chomp` strips `\n`, keeps `\r`); skip/upto. (Inherited from the shared core, like FastQ.)

### 3.2 Aligner format flag (`options.rs`) — VERIFY-ONLY (rev1 A/B I-1)
**Already implemented + tested** — no new work. `build_aligner_options` pushes **`-f`** FIRST for FastA (`options.rs:25–28`, Perl 7808–7811, before all other option assembly), and `require_fastq` (`options.rs:183–188`, Perl 7840–7853) makes `-f ⊕ --phred33/--phred64` die (tests `fasta_uses_dash_f`, `phred33_with_fasta_dies`). The `@PG CL:` embeds `aligner_options`, so the position-1 `-f` is the byte-faithful form. Verify it stays unchanged.

### 3.3 Driver re-read + QUAL (`lib.rs`)
The drivers re-read the **original** reads in lockstep with the merge. Branch on `config.format`:
- **FastQ** (existing): 4-line read; QUAL = the read's quality line (ASCII −33/−64).
- **FastA** (new): 2-line read (`id`, `seq`); strip leading `>` from the id (Perl 2317-loop `s/^>//`); **synthesize `qual = b"I".repeat(seq_uc.len())`** (Perl 2707–2708, Phred 40). Everything downstream (`single_end_sam_output` / `paired_end_sam_output`) is unchanged — `'I'` − 33 = Phred 40; minus-strand qual-reverse is a no-op (all bytes equal).
- The dispatch (`pipeline()`) routes `(SingleEnd, FastA)` → `run_se` and `(PairedEnd, FastA)` → `run_pe`; the per-mode conversion (`convert_se_files` / the PE conversion) and the re-read loop both branch on format. The `_ =>` deferred arm shrinks to **threading-only** (Phase 9b) — FastA is no longer deferred.

### 3.4 Aux output — FastA (`aux_out.rs`)
`--unmapped`/`--ambiguous` for FastA write **2-line** records `>id\nseq` (Perl 2454–2466), not the 4-line FastQ form. Add a `write_fasta_record` (or branch `write_fastq_record` on a `fasta` flag). The **filename** is already FastA-aware (`aux_filename(..., fasta, ...)`, Phase 6) — verify the extension (`.fa.gz` vs `.fq.gz`) matches Perl.

### 3.5 Report
`print_final_analysis_report_*` already format-agnostic for the counts; verify the **header wording** for FastA (Perl 1737/1955 select the FastA methylation-call fn; the report header may state the input format). Byte-check per the gate. No new counters.

### Edge cases
- FastA + pbat: pbat⊕gzip dies (config); FastA pbat = G→A conversion + the SE `+2` modifier (reused from Phase 8). FastA + non-dir = 4 instances (reused).
- A read with no quality → QUAL synthesized as `'I'×len` (never empty).
- Empty FastA input → empty converted file → header-only BAM (like FastQ).
- FastA + `--phred33`/`--phred64`: **rejected at config** (Perl 7840–7853; `require_fastq` already wired) — NOT inert. (rev1 corrects the earlier "inert" reasoning: `'I'`−64 ≠ `'I'`−33, but it's moot since the combo dies.)
- PE FastA + `--gzip`: warn + write uncompressed `.fa` (SE FastA gzips; PE does not — §3.1).

## 4. Signatures (proposed)
```rust
// convert.rs — 2-line FastA core (mirror of convert_fastq_impl) + entry points.
fn convert_fasta_impl(input, temp_dir, opts, kind: ConvKind, id_suffix: &[u8], file_base: &str) -> Result<ConvertedReads>;
pub fn bisulfite_convert_fasta_se(input, temp_dir, opts) -> Result<ConvertedReads>;       // C→T, "_C_to_T", ">", suffix ".fa"
pub fn bisulfite_convert_fasta_se_ga(input, temp_dir, opts) -> Result<ConvertedReads>;    // G→A
pub(crate) fn bisulfite_convert_fasta_pe_kind(input, temp_dir, opts, read_number, kind) -> Result<ConvertedReads>;
// The file extension (".fa"/".fastq") + record arity is the ONLY structural difference;
// consider a `RecordShape { FastQ, FastA }` param threaded through a shared core to avoid duplication.

// lib.rs — format-branch the conversion + the re-read; synthesize FastA QUAL.
// aux_out.rs — write_fasta_record(w, id, seq) → ">id\nseq\n".
```

## 5. Implementation outline (TDD)
1. **`convert.rs`**: add the FastA 2-line core + SE/SE-GA/PE-kind FastA entry points (`.fa` suffix, `>` prefix, **per-record `^>` sanity**, no qual, **PE gzip-off**). Prefer a shared core parameterized by a `RecordShape {FastQ, FastA}` over a copy of `convert_fastq_impl` (rev1 A/B: makes the FastQ byte-freeze structural). Unit-test byte output + filenames for all 3 libraries (SE + PE), SE gzip, CRLF, skip/upto, empty, **+ a negative test: malformed record-2 DIES under FastA** (vs passes under FastQ), **+ an id-strip assertion** (`>read1` → id `read1`).
2. **`options.rs`**: VERIFY-ONLY — `-f`/phred-die already implemented + tested (§3.2). Just re-run the existing tests; no new code.
3. **`lib.rs`**: format-branch `convert_se_files` + the PE conversion; format-branch the `drive_merge`/`drive_merge_pe` re-read loops (2-line + synthesized `'I'×len` QUAL); add the 2 dispatch arms; shrink the `_ =>` message to threading-only. **Do NOT extend `strip_fastq_suffix` for `.fa`** (Perl 1622 keeps it FastQ-only; already correct).
4. **`aux_out.rs`**: FastA 2-line record writer (`>id\nseq`); route `--unmapped`/`--ambiguous` through it when FastA. Assert the aux content is the non-uc original seq.
5. **Tests** (§9) — 🔴 **LOAD-BEARING (rev1 B C-1): the Phase-8 fake-bowtie2 fakes hardcode the 4-line FastQ shape** (`awk 'NR%4==1 … sub(/^@/,"",id)'`). Fed a 2-line FastA converted file they skip every other read and keep the `>` — so the FastA integration tests would PASS while validating nothing (a false-pass exactly like the Phase-8 `*BS_CT*`-only trap). **Write FastA-aware fakes** (`NR%2==1`, `sub(/^>/,"",id)`) for SE + PE + the Phase-8 strand variants (`*BS_GA*`/G→A-reads), and byte-assert FLAG/SEQ/**QUAL=`IIIIII`**/XM. **Directional + non-dir + pbat FastQ must stay byte-frozen** (the regression guard).
6. Pre-gate confirmations are now closed (rev1): `-f` (position 1, Perl 7811), phred-die (7840–7853), aux `.fa.gz` (SE always gzipped, Perl 1293), per-record sanity (5271/5414). Remaining: the FastA aux extension exact bytes + PE-gzip-off, both covered by tests + the gate.

## 6. Efficiency
FastA records are half the lines of FastQ; conversion + re-read are O(reads). No new genome passes, no extra instances. The shared-core refactor keeps one code path. mimalloc already global.

## 7. Integration
- Reads: the original FastA input (2-line); writes the `.fa`/`.fa.gz` converted temp(s); the BAM/report/aux are the existing writers (QUAL synthesized upstream). Temp cleanup reuses the per-mode loop (now deleting `.fa` files).
- Order relative to other phases: independent of 9b (threading wraps the per-file loop; FastA is a per-record format branch). Downstream Phase 10 gate will include FastA cells.

## 8. Assumptions
**From epic:** Perl v0.25.1 + Bowtie 2 2.5.5 oracle; byte-identity on decompressed SAM content (samtools `@PG` normalised); adjudicate on Linux/oxy; identical argv. **Phase-specific:** FastA QUAL = `'I'×len` (Phred 40, confirmed Perl 2707–2708); converted suffix `.fa`/`.fa.gz`; aligner flag `-f`; library logic identical to FastQ; the strand/merge/FLAG/XM/report code is format-agnostic (verify-only); FastQ paths byte-frozen.

## 9. Validation
| # | Verify | How | Expected |
|---|--------|-----|----------|
| 1 | FastA conversion bytes + `.fa` filenames, all 3 libraries SE+PE | unit | per Perl 5169/5308 (2-line, `>` prefix, C→T/G→A/both, `/1/1`,`/2/2` on PE) |
| 2 | `-f` in aligner_options for FastA; `-q` unchanged for FastQ | unit | exact token/position vs Perl |
| 3 | SE FastA end-to-end: BAM SEQ + **QUAL = `IIIIII`** (Phred 40) + FLAG/XM/XR/XG | integration (fake bt2) | byte-correct; QUAL all `I` |
| 4 | PE FastA end-to-end (both mates QUAL `I×len`) | integration | byte-correct |
| 5 | 🔴 **FastA-aware fakes** (rev1 B C-1): the Phase-8 fakes are `NR%4==1`/`sub(/^@/)` — they false-pass on a 2-line `.fa`. New fakes MUST be `NR%2==1`/`sub(/^>/)` (SE + PE + `*BS_GA*`/G→A strand variants) | integration | without them, rows 3–6 validate nothing |
| 6 | FastA non-dir + pbat (FastA-aware strand fakes from #5) | integration | CTOT/CTOB land; QUAL `I` |
| 7 | `--unmapped`/`--ambiguous` FastA = 2-line `>id\nseq` (non-uc orig), `.fa.gz` (SE) name | integration | matches Perl 2454–2466 |
| 8 | malformed **record-2 DIES** under FastA (per-record `^>`); id-strip (`>r1`→`r1`) | unit | FastA dies; FastQ (record-1-only) passes record-2 verbatim |
| 9 | **FastQ directional/non-dir/pbat byte-frozen** through the format branch | existing suite | zero diff (regression guard) |
| 10 | 🎯 **oxy gate**: a FastA-converted subset of `10M_SE`/`10M_PE` (seqtk/awk FastQ→FastA), `-f`, all 3 libraries × SE/PE, identical argv into same `-o` | Phase-8 harness pattern | byte-identical to Perl v0.25.1 (BAM + report + aux) |

## 10. Questions or ambiguities
- **(Resolved at rev1 by both reviewers)** `-f` token = position-1, already implemented (Perl 7811); `-f ⊕ --phred33/64` already dies (7840–7853); FastA record sanity is PER-RECORD `^>` (5271/5414); aux is `.fa.gz` SE-always-gzipped (1293); PE FastA gzip = warn+uncompressed (5311–5314); FastA QUAL = `'I'×len` SE 2707–2709 + PE 3271–3280. None change goal/scope.
- **(Open — decide at implementation)** Refactor shape: a shared core parameterized by `RecordShape {FastQ, FastA}` vs separate FastA functions. *Assumption:* shared core (both reviewers endorsed — keeps FastQ byte-frozen by construction). The shape difference = record arity (2 vs 4), suffix (`.fa` vs `.fastq`), prefix (`>` vs `@`), sanity scope (per-record vs record-1), and PE-gzip (off vs on).
- **(Resolved)** Threading = Phase 9b. Gate = FastA-converted subset of the existing datasets.

## 11. Self-Review
- **Logic:** FastA = FastQ minus the quality line + `.fa` suffix + `-f` + synthesized QUAL; every strand/merge/FLAG/XM/report path is format-agnostic (Phases 4–8). Traced to Perl (conversion 5169/5308, re-read 2317/2484, QUAL 2707–2708, dispatch 337/496/746–779/1737/1955). ✓
- **Edge cases:** empty input; truncated tail (`last unless header and seq`); CRLF; skip/upto; pbat (G→A + `+2`); non-dir (4 instances); minus-strand QUAL-reverse no-op; `-f --phred64`. ✓
- **Integration:** reuses `convert_one`/`fix_id`/`ConvKind`, the per-mode instance plans (Phase 8), `single_end_sam_output`/`paired_end_sam_output` (QUAL synthesized upstream), `aux_filename` (already FastA-aware). New = the 2-line core + the re-read branch + `-f` + the 2-line aux writer. FastQ byte-frozen. ✓
- **Risks (post-rev1):** (1) 🔴 the **FastA-aware fakes** — without them the integration tests false-pass on a 2-line `.fa` (B C-1); this is the dominant test risk; (2) keeping FastQ byte-frozen through the format branch (existing tests + the shared-`RecordShape` core guard it); (3) the QUAL synthesis must land at the merge/driver level (mirroring Perl `check_results_*`), not in `single_end_sam_output`, so the minus-strand reverse + phred offset behave identically; (4) the per-record `^>` sanity (NOT record-1-only) + PE-gzip-off — both byte/behaviour-pinnable by the §9 tests. The `-f` placement + phred-die are RESOLVED (already implemented, verify-only). All pinnable before the oxy gate.

## 12. Revision History
- **rev 1 (2026-06-02)** — folded dual plan-review (`PLAN_REVIEW_A.md` APPROVE-WITH-FINDINGS, `PLAN_REVIEW_B.md` APPROVE-WITH-CHANGES; no contradictions; both traced every load-bearing claim to source — format-agnostic core, the `'I'×len` QUAL for SE *and* PE, the conversion mirror all confirmed true). Folded findings (all factual corrections, no goal/scope change):
  - 🔴 **(B C-1, load-bearing) FastA-aware fakes** — the Phase-8 fake bowtie2 hardcodes the 4-line FastQ shape (`NR%4==1`/`sub(/^@/)`) → would false-pass on a 2-line `.fa`. New fakes MUST be `NR%2==1`/`sub(/^>/)` (SE + PE + strand variants), byte-asserting QUAL=`I…` (§5 step 5, §9 #5). Mirror of the Phase-8 `*BS_GA*` trap.
  - 🔴 **(A/B) per-record `^>` sanity** — FastA dies on EVERY record (Perl 5271/5414), NOT record-1-only like FastQ (5612). §3.1 step 6 corrected; +a negative test (§9 #8).
  - **(A/B I-1) `options.rs` `-f` is already done** + tested (`-f` position-1 Perl 7811; `-f⊕--phred` dies 7840–7853) → moved to verify-only; removed from "new work" and from the open questions.
  - **(B I-3/`--phred`)** `-f --phred33/64` is REJECTED (not "inert"); corrected the wrong `73−64` reasoning (moot — it dies).
  - **(B I-4) PE FastA gzip = warn+uncompressed**; SE FastA honors `--gzip`. Documented the SE/PE divergence (§3.1).
  - **(B I-5)** `strip_fastq_suffix` stays FastQ-only (Perl 1622) — flagged do-NOT-extend (§5 step 3).
  - **(B I-6)** softened the branch-reset to verify-equivalence-then-reset (§2).
  - PE QUAL default line pinned (3271–3280); both reviewers endorsed the shared-`RecordShape` core.
- **rev 0 (2026-06-02)** — initial plan, after orienting on the Perl FastA branches (conversion 5169/5308, re-read 2317/2484, the `'I'×len` QUAL at 2707–2708, dispatch 337/496/746–779/1737/1955) + the existing Rust (format already resolves; `aux_filename` already FastA-aware; strand/merge/FLAG/report format-agnostic). Scope = FastA only (threading split to 9b per Felix). Awaiting manual review → (after approval) dual plan-review → implement trigger.

## 13. Implementation Notes (2026-06-02)

**Status: COMPLETE + GATED.** 226 tests (194 lib + 32 integration); clippy `-D warnings` + `cargo fmt --check` clean.
Built on `rust/aligner` re-based onto `origin/rust/iron-chancellor` (`7f7d77d`). Dual `/code-reviewer` → both
**APPROVE** (`CODE_REVIEW_A.md`/`_B.md`; both re-derived the FastA-branch XM + confirmed `convert_fastq_impl`
byte-frozen). `/plan-manager` → **COMPLETE** (`COVERAGE.md`). **oxy FastA gate ✅ PASS** (`GATE_OXY.md`): all 4 cells
(SE/PE × {directional, non-dir}) byte-identical to Perl v0.25.1 + Bowtie 2 2.5.5 at **10k AND 1M** (pe_dir =
1,703,304 records; pbat excluded — `--pbat ⊕ -f` dies). NOT committed (commit/PR on explicit ask).

### What was built
- **`convert.rs`** — `convert_fasta_impl` (2-line core) + `bisulfite_convert_fasta_se` / `_se_ga` / `_pe_kind` (pub(crate)). `>` prefix, `.fa`/`.fa.gz` suffix, **per-record `^>` sanity** (Perl 5271), **no** max-length guard, no `+`/qual line. PE FastA forces gzip off (Perl 5311). +9 unit tests (golden C→T/G→A, PE pbat-kind, the record-2-dies negative test, record-1 malformed, SE gzip, PE gzip-off, empty/CRLF, skip/upto).
- **`lib.rs`** — `pipeline()` now matches layout only (FastQ + FastA both route to `run_se`/`run_pe`; the `_=>` deferral is gone — only `--multicore` remains deferred, via `deferred_flags`). Format-dispatch helpers `convert_se_ct`/`convert_se_ga`/`convert_pe_kind`; `convert_se_files` + the PE conversion branch on format; `drive_merge`/`drive_merge_pe` re-read 2 lines for FastA, strip `>`, and synthesize **`qual = b"I".repeat(seq_len)`** (Perl 2707/3271). New `write_se_aux_record` (FastA 2-line vs FastQ 4-line); `write_pe_aux` gained a `fasta` param. `strip_fastq_suffix` left FastQ-only.
- **`aux_out.rs`** — `write_fasta_record` (`>id\nseq`); `aux_filename`'s `fasta` flag already produced `.fa.gz`. +2 unit tests.
- **`options.rs`** — VERIFY-ONLY (the `-f` flag + the `-f⊕--phred` die were already implemented + tested, rev1 I-1).
- **`tests/cli.rs`** — FastA-aware fakes (`NR%2==1`, `sub(/^>/)`) for SE mapped / GA-index / unmapped / PE; 4 integration tests byte-asserting FLAG/SEQ/**QUAL=Phred 40**/XR/XG/XM for SE-directional, SE-non-dir CTOB (eff 3), PE-directional, and the 2-line FastA unmapped aux. FastQ (28 tests) byte-frozen.

### Deviations from the plan
1. **Separate `convert_fasta_impl` instead of a shared `RecordShape` core** (§4 preferred the shared core; rev1 A/B endorsed it). Rationale: the 2-vs-4-line read/write + per-record-vs-record-1 sanity + absent max-len guard diverge enough that a merged core is more branches than shared code; leaving `convert_fastq_impl` UNMODIFIED guarantees the FastQ byte-freeze. Shared logic is the existing helpers (`fix_id`/`convert_one`/`temp_dir_prefix`/`pe_id_suffix`/`file_base_for`). Same intent (FastQ frozen + helpers reused), lower risk.
2. **The `fasta_se_pbat_…` planned integration cell became `fasta_se_nondir_…`** — `--pbat ⊕ -f` DIES at config (Perl 8155; the plan's §3.1/§9 noted this die but the test draft missed it). Non-directional is the FastA complementary-strand path; it reaches the SAME eff-3 CTOB record via slot 3. (Caught by the test failing on the config die.)

### Iteration log
- **#1** convert.rs FastA core + 9 unit tests → 36 convert tests green (FastQ 27 frozen).
- **#2** lib.rs format-branch (dispatch/convert/re-read/QUAL/aux) + aux_out FastA writer → 194 lib green; 28 FastQ integration green (regression guard).
- **#3** FastA integration tests → 2 failures: (a) reads in the genome dir got globbed as genome FASTAs (`.fa` ≠ `.fq`) → moved reads to a separate TempDir; (b) `--pbat -f` dies → switched the strand cell to `--non_directional`. → 32 integration green.
- **#4** `cargo fmt` (the separate CI gate) + module-doc refresh → 226 tests, clippy + fmt clean.
