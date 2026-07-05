# PLAN — Phase 3: Single-instance alignment + SAM parse (lockstep stream primitive)

> **Epic:** `05312026_bismark-aligner/EPIC.md`, Phase 3 — *Single-instance align + SAM parse*
> Depends on: **Phase 1** (`RunConfig`: `detected_aligner.path`, `aligner_options`, `ct/ga_index_basename`)
> and **Phase 2** (the `_C_to_T.fastq` temp file). No scoring, no merge, no output yet.

## 1. Goal

Build the **lockstep stream primitive**: spawn **one** Bowtie 2 subprocess against the converted temp
FastQ, consume its SAM stdout, skip the header, and expose a *"current record + advance"* interface that
Phase 4 will drive across the 2–4 instances. Parse each SAM line into the fields Phase 4's scoring needs
(8 core fields + `AS:i:`/`XS:i:`/`ZS:i:`/`MD:Z:` tags). **No** best-alignment scoring, strand merge, `XM`
call, or BAM output — those are Phases 4/5. This phase is delivered as a tested library primitive; it is
**not yet wired into `run()`** (Phase 4 wires the multi-instance pipeline).

## 2. Context

- **New module** `rust/bismark-aligner/src/align.rs`. Uses `std::process::Command` (subprocess-wrap
  Bowtie 2 — the SPEC fork-1 decision). **No `bismark-io`/noodles yet** — we parse Bowtie 2's *text* SAM
  stdout line-by-line (the eventual BAM *write* uses noodles in Phase 5).
- **Perl source of truth:** `single_end_align_fragments_to_bisulfite_genome_fastQ_bowtie2` (6849–6912) for
  the spawn + header-skip + store-first-line model, and `check_results_single_end` (2722–2796) for the
  field/tag extraction (`split /\t/`; indices `0,1,2,3,4,5,9,10`; tags scanned from field 11+).
- **Determinism (Phase-0 spike):** single-threaded per instance (no `-p`), no reorder flags → Bowtie 2
  emits records in input order; the lockstep model relies on this.

## 3. Behavior (numbered) — mirrors Perl 6871–6911 / 2722–2796

**3.1 Spawn one instance.** For the v1 spine demonstrate `CTreadCTgenome` (CT index, `--norc`). Build the
arg vector the Perl way (6872–6882): `aligner_options` split on whitespace, then `--norc`, then `-x
<ct_index_basename>`, then `-U <converted_temp_path>`. `Command::new(<bowtie2 path>)` with `stdout(piped())`
and `stderr(inherit())` (Bowtie 2's alignment summary goes to the terminal, as in Perl; it is not gated).

**3.2 Skip the SAM header.** Read lines from the child's stdout (buffered); discard every line beginning
with `@` (Perl 6884–6894). The first non-`@` line is the first alignment record (Bowtie 2 emits a line per
read even when unmapped). Store it as the *current* record; at EOF store `None`.

**3.3 Parse a SAM line** (`SamRecord`) — `split('\t')`:
- core fields by index: `0` QNAME, `1` FLAG (u16), `2` RNAME (kept raw, incl. `_CT_converted`/
  `_GA_converted` suffix — de-conversion is Phase 4/5), `3` POS (u32), `4` MAPQ (u8), `5` CIGAR, `9` SEQ,
  `10` QUAL.
- optional tags scanned from field `11..` in **field order** (Perl 2775–2795): `AS:i:<int>` →
  `alignment_score`; `XS:i:`/`ZS:i:` → `second_best`; `MD:Z:<str>` → `md_tag`. The four prefixes are
  **disjoint** (one tag per field), so Perl's `if/elsif` chain collapses to independent matches; `second_best`
  is **overwritten on each `XS`/`ZS` match (last-in-field-order wins)** — matches Perl, which sets it both
  at 2780 (`ZS`) and 2788 (`XS`) as fields are scanned. (Bowtie 2 emits `XS:i:`, HISAT2 `ZS:i:` — a line
  won't carry both; the last-wins rule only matters for robustness.)
- **Numeric / malformed policy:** `AS`/`XS`/`ZS` values parse to `i64` (Bowtie 2 scores are ≤ 0 → must
  accept negatives); an unparseable tag value → leave the field `None` (lenient; Phase 4 enforces presence
  of `AS`/`MD` on a mapped record, Perl `die` 2838). A structurally short line (`< 11` tab fields) → a
  **parse error** (Bowtie 2 always emits ≥ 11 fields, even for `flag==4`).
- **`raw_line` = the CHOMPED line** (trailing `\n`/`\r` stripped). Perl stores `last_line` *after* `chomp`
  (6898) and `--ambig_bam` (in v1 scope) re-emits that stored value (2807–08) — keeping the terminator
  would inject a stray `\n` into Phase 6's re-emit.
- `is_unmapped()` = `flag == 4` (Perl 2739 — SE; PE differs, Phase 7).

**3.4 Advance.** `advance()` reads the next stdout line, parses it, and replaces *current* (or sets `None`
at EOF). `current()` peeks without consuming. (Phase 4's `flag == 4` "store next, move on" logic lives in
the consumer; Phase 3 only provides peek+advance.)

**3.5 Finish / reap (pinned per dual review).** `finish()` reaps the child and checks exit status. **It is
valid only at EOF or after draining stdout** — calling `wait()` while the child still has buffered stdout
and we've stopped reading would deadlock the child on a full pipe (a real risk in Phase 4's early-stop).
So `finish()` either (a) is called after `current()` has reached `None` (EOF), or (b) drains+discards
remaining stdout, then `wait()`s. The **`Drop` guard does `kill()` THEN `wait()`** (kill alone leaves a
zombie). `stderr` is **inherited** (not piped) — that is *why* there is no stderr-pipe deadlock; only
stdout is piped and is always drained. The non-zero-exit → error is an **intentional fail-closed**
deviation from Perl (which closes the pipe fail-open); document it.

### Edge cases
- **Empty stream / all-header** → first record is `None`; `finish()` still reaps cleanly.
- **Unmapped record** (`flag == 4`) → parsed like any line; `is_unmapped()` true; no `AS`/`MD` required.
- **Missing `AS:i:`/`MD:Z:` on a mapped record** → parser leaves them `None`; Phase 4 enforces presence
  (Perl `die` at 2838) — Phase 3 does not die (it only parses).
- **Aligner exits non-zero / fails to spawn** → error from `spawn()`/`finish()`.
- **Early stop (partial read)** — drop/finish a stream before EOF (real in Phase 4): must not deadlock or
  zombie (drain-or-kill+wait, per §3.5).
- **CRLF in SAM** (won't happen from Bowtie 2, but) `split('\t')` + trailing `\n`/`\r` handling: trim the
  line terminator before splitting QUAL (the trimmed line is also what `raw_line` stores).

## 4. Signature (proposed)

```rust
pub struct SamRecord {
    pub qname: String, pub flag: u16, pub rname: String, pub pos: u32, pub mapq: u8,
    pub cigar: String, pub seq: String, pub qual: String,
    pub alignment_score: Option<i64>, pub second_best: Option<i64>, pub md_tag: Option<String>,
    pub raw_line: String,
}
impl SamRecord {
    pub fn parse(line: &str) -> Result<SamRecord>;   // split('\t'); tag scan from field 11
    pub fn is_unmapped(&self) -> bool;               // flag == 4 (SE)
}

pub struct AlignerStream { /* Child + BufReader<ChildStdout> + current: Option<SamRecord> */ }
impl AlignerStream {
    pub fn spawn(bowtie2: &Path, options: &str, orient: Orientation, index: &Path, input: &Path) -> Result<Self>;
    pub fn current(&self) -> Option<&SamRecord>;
    pub fn advance(&mut self) -> Result<()>;
    pub fn finish(self) -> Result<()>;               // wait + check exit status
}
pub enum Orientation { Norc, Nofw }                  // --norc / --nofw per the strand-instance table
```

## 5. Implementation outline

1. `align.rs`: `SamRecord` + `parse` (split, field indices, tag scan) + `is_unmapped`. Unit-test first.
2. `Orientation` enum (`--norc`/`--nofw`).
3. `AlignerStream::spawn`: build args (options split + orient + `-x` + `-U`), `Command` with piped stdout /
   inherited stderr, wrap stdout in `BufReader`, skip `@` headers, read+store the first record.
4. `current` / `advance` (read next line, parse, set/clear current) / `finish` (wait + status) + `Drop` kill-guard.
5. Tests (see §9) — parser units + a fake-`bowtie2` SAM emitter for the end-to-end stream.

## 6. Efficiency

One subprocess + linear streaming read; reuse a line buffer across `advance()`; parse only on advance.
Not a hot path at this phase (the genome/index load is Bowtie 2's cost). No premature optimization.

## 7. Integration

- **Consumes** `RunConfig` (aligner path, options, index basenames) + Phase 2's converted temp path.
- **Produces** the `AlignerStream` primitive that **Phase 4** drives across the 2 (SE-directional) / 4
  instances for the read-ID-lockstep best-alignment merge. Phase 4 also adds the `flag == 4` advance, the
  `RNAME` de-conversion, and the scoring (`best_AS_so_far`, ambiguity).
- **Not wired into `run()` in this phase** — `run()` keeps its Phase-2 behavior (resolve → convert →
  summary). Wiring the multi-instance pipeline is Phase 4 (keeps Phase-1/2 tests + binary behavior stable).

## 8. Assumptions

**From epic (shared):** Perl v0.25.1 oracle + Bowtie 2 2.5.5 pinned; output is fully Bismark-generated
(Bowtie 2 SAM is parsed, not passed through); single-thread-per-instance determinism; byte-identity gate is
on the eventual BAM (Phase 5), not on this primitive.

**Phase-specific:**
- **Raw `split('\t')` parsing** (matches Perl exactly + the lockstep peek/advance model), not noodles-sam.
  noodles enters in Phase 5 for the BAM *write*. *(Open Q1.)*
- The arg **order** mirrors Perl (`<options> --norc -x <idx> -U <reads>`); Bowtie 2 parses flags
  order-independently, but we replicate Perl's order for faithfulness.
- `stderr` inherited (Bowtie 2's summary → terminal, as in Perl; not gated).
- Single instance (`CTreadCTgenome`) demonstrated here; the 2–4-instance fan-out is Phase 4 (the primitive
  is built reusable via `Orientation` + index/input params).

## 9. Validation

| # | Verify | How | Expected |
|---|--------|-----|----------|
| 1 | `SamRecord::parse` core fields | unit on a canned mapped line | qname/flag/rname/pos/mapq/cigar/seq/qual correct |
| 2 | tag scan | unit lines with `AS:i:-12`, `XS:i:-20`, `MD:Z:50` (+ HISAT2 `ZS:i:`) | `alignment_score=-12`, `second_best=-20`, `md_tag="50"` |
| 3 | unmapped | unit on a `flag==4` line | `is_unmapped()` true; `AS`/`MD` may be `None` |
| 4 | missing AS/MD on mapped | unit | parses to `None` (no die — Phase 4 enforces) |
| 5 | header skip + first record | integration: fake `bowtie2` emits `@HD`/`@SQ` + 3 records | first `current()` is record 1 |
| 6 | advance to EOF | integration | `advance()` walks records 2,3 then `current()` → `None` |
| 7 | finish reaps + exit status | integration: fake exits 0 / exits 1 | `finish()` Ok / Err respectively |
| 8 | spawn failure | bad binary path | `spawn()` errors |
| 9 | empty/all-header stream | fake emits only headers | first `current()` is `None`; `finish()` Ok |
| 10 | realistic full line | unit: RNEXT/PNEXT/TLEN populated (`*`/`0`/`0`) before SEQ/QUAL | SEQ/QUAL come from indices 9/10, not earlier fields |
| 11 | both `XS:i:` and `ZS:i:` present | unit | `second_best` = the last-in-field-order value (overwrite rule) |
| 12 | unique alignment, no `XS`/`ZS` | unit | `second_best == None` |
| 13 | short line (`< 11` fields) | unit | parse error |
| 14 | CRLF row + `raw_line` | unit: a `\r\n`-terminated SAM line | QUAL has no trailing `\r`; `raw_line` has no `\n`/`\r` |
| 15 | early-stop / partial read | integration: advance once on a many-record fake, then `finish()` | no deadlock, no zombie (drain-or-kill+wait); completes |

(Tests use a fake `bowtie2` script that emits canned SAM regardless of args — hermetic, no real Bowtie 2 /
index needed; same pattern as the Phase-1 detection tests.)

## 10. Questions or ambiguities

- **(Open Q1)** Parse via raw `split('\t')` vs `noodles-sam`. *Assumption:* raw split (faithful + lean;
  noodles for the Phase-5 BAM write). Confirm.
- **(Open Q2)** Wire into `run()` now (spawn+drain a single instance, report a count) vs keep as an unwired
  primitive until Phase 4. *Assumption:* unwired primitive (keeps binary behavior + existing tests stable;
  Phase 4 wires the real multi-instance pipeline). Confirm.

## 11. Self-Review

- **Efficiency:** one subprocess, streaming, buffer reuse. ✓
- **Logic:** spawn/header-skip/store-first/advance mirror Perl 6884–6911; field indices + tag scan mirror
  2773–2795; `flag==4` SE-unmapped per 2739. ✓
- **Edge cases:** empty/all-header, unmapped, missing tags, spawn/exit failure, line-terminator trim. ✓
- **Integration:** clean `AlignerStream` seam for Phase 4's N-way merge; `raw_line` retained for `--ambig_bam`
  / re-emit; not wired into `run()` so Phase-1/2 behavior is untouched. ✓
- **Risks:** child-process lifecycle (zombies/deadlock) — mitigated by the pinned §3.5 contract
  (drain-or-EOF before `wait`; `Drop` = kill+wait). The `is_unmapped()==flag==4` is SE-only; flagged for
  Phase 7 (PE). Tag-scan handles negative `AS:i:` (Bowtie 2 scores ≤ 0) — parse as `i64`.

## 12. Revision History

- **rev 1 (2026-06-01)** — folded in dual plan-review (`PLAN_REVIEW_A.md`/`PLAN_REVIEW_B.md`; both APPROVE,
  no Critical, design endorsed). Source-verified precision edits:
  - **`raw_line` = the CHOMPED line** (Perl stores `last_line` post-`chomp` 6898; `--ambig_bam` re-emits it
    2807–08) — B.
  - **Child lifecycle pinned** (§3.5): `finish()` drain-or-EOF before `wait` (avoids full-stdout-pipe
    deadlock on Phase-4 early-stop); `Drop` = `kill()`+`wait()`; stderr inherited; non-zero-exit = intentional
    fail-closed — A+B.
  - **Tag scan** specified as field-order, last-`XS`/`ZS`-wins, disjoint prefixes; numeric → `i64` (accept
    negatives), malformed → `None`, `<11` fields → parse error — A+B.
  - **§9 validations added** (#10–#15): realistic RNEXT/PNEXT/TLEN line, both-tags precedence, no-`XS`
    unique (`second_best==None`), short-line error, CRLF/`raw_line` trim, early-stop/partial-read.
- **rev 0 (2026-06-01)** — initial plan.

## 13. Implementation Notes (2026-06-01)

**Status: IMPLEMENTED & verified — 49 unit + 15 integration tests green; clippy `-D warnings` + fmt clean.**

- New module `src/align.rs`: `Orientation` (`--norc`/`--nofw`), `SamRecord` (`parse` + `is_unmapped`),
  `AlignerStream` (`spawn`/`current`/`advance`/`finish` + `Drop`). Registered as `pub mod align` in
  `lib.rs`; **not wired into `run()`** (Phase 4 wires the multi-instance pipeline), so binary behavior +
  Phase-1/2 tests are unchanged.
- All rev-1 review points implemented: `raw_line` is the chomped line; tag scan is field-order /
  last-`XS`/`ZS`-wins / `i64` (negatives) / `<11`-fields→error; `finish()` drains stdout before `wait()`;
  `Drop` does `kill()`+`wait()`; stderr inherited; non-zero exit → error (fail-closed).
- **Tests:** 13 in `align.rs` — parser units (core fields, AS/XS/MD, HISAT2 `ZS`, both-XS-ZS last-wins,
  unique no-second-best, unmapped, short-line error, CRLF/`raw_line` trim) + fake-`bowtie2` integration
  (header-skip→walk→EOF, all-header, non-zero-exit error, bad-path spawn error, and the **early-stop /
  ~5000-record partial-read** test that would hang if `finish()` didn't drain).
- **Deviation (documented):** module is `align.rs` alongside the existing `aligner.rs` (Phase-1 detection);
  the names are close but distinct (align = the alignment *stream*; aligner = binary *detection*) — module
  docs disambiguate. Considered renaming; kept `align.rs` to match the approved plan.
- **Carried forward:** Phase 4 drives 2–4 `AlignerStream`s in read-ID lockstep (the N-way merge + scoring +
  `flag==4` advance + RNAME de-conversion); `is_unmapped()==flag==4` is SE-only (PE = Phase 7).

### Post-review (2026-06-01)

Dual code-review (both **APPROVE**, no Critical/High) + plan-manager (**COMPLETE**, 0 gaps). Felix authorised
the 4 recommended test additions (pure test-strengthening; no production-code change):
1. `mapped_record_missing_as_md_parses_to_none` — a *mapped* read with no `AS`/`MD` parses to `None`
   (lenient; Phase 4 enforces) — both reviewers' shared gap.
2. `realistic_line_with_mate_fields_and_trailing_md` — non-trivial RNEXT/PNEXT/TLEN + ignored tags
   (`YT:Z:`/`NM:i:`) + `MD:Z:` last → guards the SEQ/QUAL index split.
3. `md_tag_with_mismatch_letters` — `MD:Z:5A4`.
4. `malformed_numeric_fields_error` — bad FLAG/POS/MAPQ → error.

Two Low items left as documented/unreachable on real Bowtie 2 output (the `\r`-strip-vs-`chomp` nuance and
the `read_line` UTF-8 assumption). **Final totals: 53 unit + 15 integration tests; clippy `-D warnings` + fmt clean.**
