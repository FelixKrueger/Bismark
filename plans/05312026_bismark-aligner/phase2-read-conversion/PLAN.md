# PLAN — Phase 2: Read conversion (C→T, FastQ SE directional)

> **Epic:** `05312026_bismark-aligner/EPIC.md`, Phase 2 — *Read conversion*
> Depends on: **Phase 1** (`phase1-cli-options-discovery/PLAN.md`, ✅ — provides the `RunConfig`).

## 1. Goal

Produce the bisulfite-converted temporary FastQ file that the Bowtie 2 instance reads, **byte-identical**
to Perl `bismark`'s `biTransformFastQFiles` output, for the v1 spine (**FastQ, single-end, directional**).
For directional SE this is exactly one file: `<temp_dir>/<prefix.>?<basename>_C_to_T.fastq`. The original
(unconverted) read is **not** stored here — it is re-read in lockstep during the later methylation-call
loop (Phase 3+), so Phase 2's sole deliverable is the converted temp file.

## 2. Context

- **New module** `rust/bismark-aligner/src/convert.rs`. Consumes the Phase-1 `RunConfig`
  (`layout: SingleEnd { reads }`, `format: FastQ`, `library: Directional`, `output.temp_dir`) plus the
  read-processing options (`prefix`, `skip`, `upto`, `gzip`, `icpc`) — see §8 about threading these into
  the seam.
- **Perl source of truth:** `biTransformFastQFiles` (5489–5651) and `fix_IDs` (6235–6246).
- **gzip input** via `flate2::read::MultiGzDecoder` (pure-Rust; mirrors `bismark-genome-preparation`'s
  multi-member-safe `.gz` reading — Perl uses `gunzip -c |`, but decompressed bytes are identical).
- **No `bismark-io`/noodles** yet (FastQ text in, FastQ text out).

## 3. Behavior (numbered) — mirrors Perl 5489–5651

**3.1 Temp-file name.** `basename` = input file name with any directory stripped (extensions are **kept**:
`subset.fq.gz` → `subset.fq.gz`). With `--prefix p` → `p.<basename>`. Append `_C_to_T.fastq` (or
`_C_to_T.fastq.gz` if `--gzip`). Full path = **raw string concatenation** `<temp_dir><name>` (Perl
`${temp_dir}${infile}`, 5548/5554) — **NOT `Path::join`** (which would normalize a non-`/`-terminated
`temp_dir` differently). **temp_dir normalization:** Perl makes `$temp_dir` absolute + trailing-`/` when
set (8211–31); Phase 1 currently stores the raw value (empty = CWD). Phase 2 must normalize it the Perl
way (absolute + trailing `/` when non-empty) before the concat so `--temp_dir foo` → `foo/<name>`, not
`foo<name>`. (Default empty temp_dir → CWD-relative `<name>`, which already matches Perl.)

**3.2 Open input.** If the path ends in `.gz`, decompress (MultiGzDecoder); else read raw. Buffer.

**3.3 Per-record loop** (read 4 lines: `id`, `seq`, `id2`, `qual`; stop when any is missing — i.e. a clean
EOF on a 4-line boundary; a truncated final record is dropped, matching Perl's `last unless (all four)`).
**Step order matches Perl 5577–5634 exactly** (rev-1: corrected per dual review — byte-neutral but faithful):
  1. `count += 1` **before** the skip/upto checks (count = running record number incl. skipped). (5588)
  2. **ID:** strip a single trailing `\n` only (Perl `chomp`; a `\r` is **kept** → CRLF preserved), apply
     `fix_IDs`, then re-append `\n`. (5584–86)
  3. **skip/upto:** if `skip` set, `continue` while `count <= skip`; if `upto` set, `break` once
     `count > upto`. (5590–95) **⚠ Consequence:** because the record-1 sanity check (step 6) sits
     *after* this, a non-zero `--skip` makes `count==1` `continue` here → **the FastQ sanity check is
     bypassed when skipping** (Perl quirk — replicate it; do NOT move sanity before skip).
  4. **Uppercase:** ASCII-uppercase the whole seq line (trailing newline preserved). (5597)
  5. **Max-length guard** (`maximum_length_cutoff`): if set and `len(seq) > cutoff`, skip the record.
     (5598–5604) **mm2-only** — only ever `defined` under `--minimap2`, so **inert on the v1 Bowtie 2
     spine**; include the guarded (never-taken) step so the loop structure matches Perl + the later mm2
     phase has the hook.
  6. **tab-in-id detection:** if the id contains a tab, set a flag (later warning; not byte-affecting).
     (5607–09) — Perl does this *before* the sanity check.
  7. **FastQ sanity — only when `count == 1`** (so it is bypassed if record 1 was skipped, per step 3):
     the (fixed) `id` must start with `@` and `id2` with `+`, else die "Input file doesn't seem to be in
     FastQ format…". (5612–16)
  8. **Convert + write:** replace `C`→`T` on the uppercased seq (`tr/C/T/`, 5624–25; net incl. lowercase:
     `a→A,c→T,g→G,t→T,n→N,…`), then write `id + seq_C_to_T + id2 + qual`. `id2` and `qual` are written
     **verbatim** (original bytes incl. line endings); only `id` was re-terminated and `seq` transformed.
     (5626)

**3.4 Close + return** the temp-file path (relative name + the absolute path for the aligner `-U`).

**`fix_IDs` (6235):** default → replace every run of spaces/tabs `[ \t]+` with a single `_`; `--icpc` →
truncate the id at the first space/tab (`s/[ \t].*$//`). Operates on the chomped id (leading `@` kept).

### Edge cases
- **gzip input** → identical decompressed bytes → identical conversion.
- **Empty input** → 0 records → empty temp file, no error.
- **Truncated final record** (not a multiple of 4 lines) → dropped (Perl `last unless` all four present).
- **CRLF** line endings → preserved (chomp removes only `\n`; `uc`/`C→T` preserve `\r`; verbatim id2/qual).
- **lowercase bases** → uppercased before `C→T` (so `c`→`T`).
- **`--gzip` temp file** → its *compressed* bytes are NOT gated (internal, transient, deleted after use);
  only the **decompressed** content must match Perl. Plain `.fastq` is the primary byte-identity target.
- **skip ≥ total** → empty output; **upto = 0** → Perl treats `$upto` falsy (0 disables) — replicate
  (only apply `upto` when > 0; `skip` likewise).

## 4. Signature (proposed)

```rust
/// Write the C→T-converted FastQ temp file for one SE directional input.
/// Returns (relative_name, absolute_path) of the created `_C_to_T.fastq`.
pub fn bisulfite_convert_fastq_se(
    input: &Path,
    temp_dir: &Path,
    opts: &ConvertOptions, // prefix, skip, upto, gzip, icpc, maximum_length_cutoff
) -> Result<ConvertedReads>;

pub struct ConvertedReads { pub name: String, pub path: PathBuf, pub count: u64 }
```
The per-record transform (`fix_IDs`, uc+`C→T`) lives in small private helpers so non-directional (adds
G→A), pbat (G→A only), and PE (two files) phases reuse it.

## 5. Implementation outline

0. **Prereqs (rev-1):** add `flate2` (pinned, matching genome-prep) to the crate `Cargo.toml`; correct the
   Phase-1 `cli.rs` `--icpc` doc comment (it is the `fix_IDs` ID-truncation toggle, issue #236 — not a
   HISAT2/deferred flag); add an additive `ReadProcessing` sub-struct to `RunConfig` (only `skip`/`upto`/
   `icpc`/`maximum_length_cutoff`), populated in `resolve()`; normalize `temp_dir` (absolute + trailing
   `/` when set) in `resolve()`.
1. `convert.rs`: `ConvertOptions` (+ a `From<&RunConfig>`/builder), `ConvertedReads`.
2. `fix_id(id: &[u8], icpc: bool) -> Vec<u8>` — byte-level `[ \t]+`→`_` (or truncate); unit-tested vs Perl cases.
3. `convert_seq_line(seq: &[u8]) -> Vec<u8>` — ASCII-uppercase then `C`→`T`, newline preserved.
4. `bisulfite_convert_fastq_se` — open (gz/plain) → buffered 4-line loop → skip/upto → sanity → write.
5. Wire into `lib::run` for the SE-directional path (still no alignment): after `resolve`, convert each
   SE read file, report the temp path(s). Keep the Phase-1 summary.
6. Tests: unit (fix_id, convert_seq_line, CRLF, lowercase) + integration (golden temp file vs Perl).

## 6. Efficiency

Linear in input bytes; buffered I/O; per-record allocations kept small (reuse buffers). gzip via flate2.
Not a hot path relative to alignment; no premature optimization.

## 7. Integration

- **Reads:** the SE input FastQ (plain/gz). **Writes:** `<temp_dir>/…_C_to_T.fastq`.
- **Produces** the converted temp path consumed by Phase 3 (single-instance alignment) via Bowtie 2 `-U`.
- The original read is re-read later (methylation-call loop) — Phase 2 deliberately does not retain it.

## 8. Assumptions

**From epic (shared):** Perl v0.25.1 oracle; gate = byte-identical content (here: the plain converted temp
file, byte-for-byte; gzipped temp = decompressed-content equivalence); byte-identity adjudicated on Linux
CI/oxy; crate `bismark-aligner`.

**Phase-specific:**
- v1 wires **directional SE FastQ** only. Non-directional (adds the G→A file), pbat (G→A only), PE (two
  files), and FastA are later phases — the per-record helpers are built reusable for them.
- **`ReadProcessing` seam (rev-1, dual review):** add an additive `ReadProcessing` sub-struct carrying
  **only the new fields** `skip`, `upto`, `icpc`, `maximum_length_cutoff`. **`gzip`/`prefix`/`temp_dir`
  already live on `OutputTarget`** — read them from there (do **not** duplicate → avoids two sources of
  truth). Populated inside `resolve()`, so Phase-1 tests (which build `RunConfig` only via `resolve`) are
  undisturbed.
- **`--icpc` correction (rev-1, dual review — both reviewers):** Phase 1's `cli.rs` mislabels `--icpc` as
  a *"HISAT2 `--ignore-quals` variant (deferred)"*. It is **not** that — in Perl `$icpc` is a plain
  boolean whose **only** effect is in `fix_IDs` (6238, issue #236): truncate the read ID at the first
  space/tab instead of underscoring. Phase 2 wires `--icpc` as a **live** flag in `ReadProcessing`, and
  **corrects the Phase-1 `cli.rs` doc comment** (rides this phase's PR; the flag already parses as a bool,
  so this is a doc + wiring fix, not a parse change). Confirm `--icpc` is NOT in Phase 1's deferred-flags
  notice (it isn't).
- **Dependency:** add **`flate2`** (pinned, matching `bismark-genome-preparation`) to the crate
  `Cargo.toml` — Phase 1 didn't need it; Phase 2 does, for `.gz` input (`MultiGzDecoder`) and the
  optional `--gzip` temp output.
- `skip`/`upto` are applied here (they shape the converted output); `0` disables (`upto`) / no-skip
  (`skip`), matching Perl's falsy-scalar semantics.
- `maximum_length_cutoff` is mm2-only (parsed but inert for Bowtie 2); replicate the guard but it won't
  fire on the v1 spine.

## 9. Validation

| # | Verify | How | Expected |
|---|--------|-----|----------|
| 1 | `fix_id` default | unit: `@R 1:N` , `@R\t1` | `@R_1:N`, `@R_1` |
| 2 | `fix_id --icpc` | unit: `@R 1:N` | `@R` |
| 3 | `convert_seq_line` | unit: `ACGTacgtN\n` | `ATGTATGTN\n` (uc then C→T) |
| 4 | CRLF preserved | unit: id/seq with `\r\n` | `\r\n` retained in all four lines |
| 5 | **Golden temp file** | integration: `cmp` the converted temp vs a **committed synthetic golden generated by Perl v0.25.1** (`biTransformFastQFiles`) on a hand-built FastQ exercising lowercase / CRLF / space+tab IDs / a post-space comment. *(The golden does NOT exist yet — generate + commit it during impl; the Phase-0 spike's `awk`-built `ct.fq` is NOT a valid oracle, no `uc`/`fix_IDs`.)* | byte-identical |
| 6 | gzip input (single + **multi-member**) | integration: `.fq.gz` (incl. concatenated members) in → plain temp out | identical to plain-input run (proves `MultiGzDecoder`) |
| 7 | `--gzip` temp output | integration: decompress the `.gz` temp, `cmp` vs the plain run | decompressed content identical (raw `.gz` bytes NOT gated) |
| 8 | skip/upto + count | unit/integration: `--skip 2 --upto 5` on 10 reads | records 3–5 written; count runs 1..N over **unskipped** numbering |
| 9 | falsy `0` semantics | integration: `--skip 0` / `--upto 0` | no skip / no early stop (Perl `if($skip)`/`if($upto)` falsy) |
| 10 | `--skip` bypasses sanity | integration: `--skip 1` on input whose record 1 is malformed | no die (sanity is `count==1`, after the skip-`next`) |
| 11 | `--icpc` end-to-end | integration: ID `@R 1:N` with `--icpc` | temp ID is `@R` (truncated), not `@R_1:N` |
| 12 | malformed record 1 vs N>1 | integration | record-1 non-`@` → die; a malformed record 5 passes **verbatim** (no over-validation) |
| 13 | malformed FastQ (record 1) | integration: non-`@` first line | die "doesn't seem to be in FastQ format" |
| 14 | empty / truncated-tail input | integration: empty file; 3-line trailing fragment after N records | empty temp (exit 0); exactly N records |

## 10. Questions or ambiguities

- **(RESOLVED, Felix 2026-06-01)** Read-processing options are threaded into `RunConfig` via an additive
  `ReadProcessing` sub-struct (keeps the seam typed; additive so Phase 1 tests are undisturbed).
- **(RESOLVED, Felix 2026-06-01)** Validation #5 uses a tiny **synthetic golden generated by Perl
  v0.25.1** (deterministic, hermetic, committed); the oxy real-data check stays for Phase 10.

## 11. Self-Review

- **Efficiency:** linear, buffered; reusable per-record helpers. ✓
- **Logic:** loop/skip/upto/sanity ordering matches Perl 5576–5634 (count++ before skip/upto; sanity on
  record 1; verbatim id2/qual). ✓
- **Edge cases:** gz, empty, truncated tail, CRLF, lowercase, `--gzip` temp (decompressed-gate), skip≥total
  — all in §3/§9. ✓
- **Integration:** clean hand-off (temp path) to Phase 3; original-read re-read deferred by design. ✓
- **Risks:** the `RunConfig` extension is a (documented) cross-phase seam change — keep it additive so it
  doesn't disturb Phase 1's tests. The `--gzip` temp-file gate is decompressed-content, not raw bytes
  (noted, consistent with the project gate definition).

## 12. Revision History

- **rev 1 (2026-06-01)** — folded in dual plan-review (`PLAN_REVIEW_A.md` / `PLAN_REVIEW_B.md`), all
  source-verified:
  - **§8 `--icpc` correction** (both reviewers): it's the `fix_IDs` ID-truncation toggle (Perl 6238, issue
    #236), not HISAT2/deferred — wire it live + fix the Phase-1 `cli.rs` doc comment.
  - **§8 `ReadProcessing` seam tightened:** carry only new fields; read `gzip`/`prefix`/`temp_dir` from
    `OutputTarget` (no double source of truth). Added the missing **`flate2`** dependency.
  - **§3.1 temp path:** raw `${temp_dir}${name}` concat (not `Path::join`) + Perl `temp_dir`
    normalization (absolute + trailing `/`).
  - **§3.3 loop reordered to Perl's exact order** (uc → max-len → tab-detect → record-1 sanity →
    `tr/C/T/`+write); documented that a non-zero `--skip` **bypasses** the sanity check (Perl quirk); added
    `maximum_length_cutoff` as an inert-on-v1 step.
  - **§9 validations:** fixed #5 (golden does NOT exist — generate + commit a synthetic Perl v0.25.1
    golden), added multi-member gzip, `--gzip`-decompresses-to-plain, falsy `0`, `--skip`-bypasses-sanity,
    `--icpc` e2e, record-1-vs-N>1 malformed, truncated-tail.
  - **§5** added a prereqs step (flate2, icpc doc fix, ReadProcessing, temp_dir normalization).
- **rev 0 (2026-06-01)** — initial plan.

## 13. Implementation Notes (2026-06-01)

**Status: IMPLEMENTED & verified — 34 unit + 15 integration tests green; clippy `-D warnings` + fmt clean.**

- New module `src/convert.rs`: `fix_id` (byte-level ws→`_` / `--icpc` truncate), `convert_seq_c_to_t`
  (`uc` then `C→T`), `chomp_newline` (strip `\n` only), `temp_dir_prefix` (Perl normalization),
  `bisulfite_convert_fastq_se` (the 4-line loop in Perl's exact order), `ConvertOptions`/`ConvertedReads`.
  Wired into `lib::run` for the v1 spine (SE + directional + FastQ); other modes print a "later phase" note.
- **Prereqs done:** `flate2` added to `Cargo.toml`; Phase-1 `cli.rs` `--icpc` doc comment corrected (it's
  the `fix_IDs` ID-truncation toggle, not HISAT2); `RunConfig` gained an additive `ReadProcessing`
  sub-struct (`skip`/`upto`/`icpc`/`maximum_length_cutoff`) populated in `resolve()` — Phase-1 tests
  undisturbed (they build `RunConfig` only via `resolve`).
- **Deviations (documented):**
  - **temp_dir normalization placed in `convert::temp_dir_prefix`, not `resolve()`** (plan §5 step 0 said
    `resolve()`). Rationale: normalization here `create_dir_all`s + canonicalizes the temp dir, a
    filesystem side-effect that doesn't belong in pure config resolution; the convert layer is where the
    dir is actually written. Behaviourally identical (absolute + trailing separator; empty → CWD).
  - **Validation #5 golden is spec-derived** (hand-computed from the line-by-line-verified Perl transform,
    committed as `convert.rs` `GOLDEN_IN`/`GOLDEN_OUT` constants), exercising space/tab IDs + lowercase +
    non-bare `+`-line. The **authoritative Perl-generated end-to-end** byte-identity check is the Phase-10
    oxy gate (full `bismark` run), since `biTransformFastQFiles` isn't callable standalone.
- **Tests cover the §9 table:** golden, multi-member gzip, gzip-output-decompresses-to-plain, skip/upto +
  count, falsy-`0`, `--skip`-bypasses-sanity, `--icpc` e2e, record-1-vs-N>1 malformed, truncated tail,
  empty input, `--prefix` naming. Binary-level happy-path/deferred tests updated to pass `--temp_dir`
  (output contained) + assert the converted file is produced.
- **Carried forward:** non-directional (adds G→A), pbat (G→A), PE (two files), FastA — later phases reuse
  `fix_id`/`convert_seq_c_to_t`; the same `fix_id` is reused by the Phase 3+ original-read re-read so IDs
  never drift.

### Post-review fix pass (2026-06-01)

Dual code-review (`CODE_REVIEW_A.md` / `CODE_REVIEW_B.md`, both **APPROVE**, no Critical) + plan-manager
(`COVERAGE.md`, **COMPLETE**). Felix authorised **all** recommended fixes — applied + tested:
- **`deferred_flags` no longer lists `--skip`/`--upto`/`--gzip`/`--prefix`** (now active in Phase 2; the
  notice was stale Phase-1 state — both reviewers + plan-manager flagged it).
- **`--mm2_maximum_length` without `--minimap2` now errors** in `resolve()` (Perl 8333) — prevents the
  convert-side length guard from silently dropping records on the Bowtie 2 spine (it's now unreachable
  there; the guard stays for the mm2 phase).
- **`--prefix` trailing-dot trim** (`s/\.+$//`, Perl 8238) applied in `resolve_output`.
- **Removed the redundant double-uppercase** on the write path (`convert_seq_c_to_t` already uppercases).
- **`seqid_tab_count` plumbed into `ConvertedReads`** for the Phase-6 report (documented as effectively
  always 0 — `fix_id` removes tabs before the check; a faithful replica of Perl's dead detection).
- **Pinned `cr.count` in the `upto` test**; added **file-level CRLF** + **no-final-newline-verbatim** tests.
- **Final totals: 36 unit + 15 integration tests; clippy `-D warnings` + fmt clean.**
- *Deferred-then-applied per Felix:* `--prefix` dot-trim and `seqid_tab_count` (I'd suggested deferring
  these to later phases; applied now on request — both are forward-compatible / content-neutral for Phase 2's
  gated artifact).
