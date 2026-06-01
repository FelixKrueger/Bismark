# PLAN_REVIEW_A — Phase 2: Read conversion (C→T, FastQ SE directional)

- **Reviewer:** A (independent, fresh context)
- **Date:** 2026-06-01
- **Target:** `phase2-read-conversion/PLAN.md`
- **Grounded against:** Perl `bismark` v0.25.1 `biTransformFastQFiles` (5489–5651), `fix_IDs` (6235–6246),
  the `$maximum_length_cutoff`/`$icpc` GetOptions wiring (7308/7313/7375/7378, 8333–8356, 8430–8431,
  9861), Phase-1 `src/{cli,config}.rs` (as implemented), EPIC.md, SPEC.md.

## Verdict

The plan's **per-record transform is faithful to the Perl** in almost every detail I could check
line-by-line — `chomp`-then-re-append, `uc` THEN `tr/C/T/`, verbatim id2/qual, `count++` before
skip/upto, record-1 sanity, the `last unless (all four)` truncated-tail drop, and the `--icpc` vs default
`fix_IDs` behaviour all match. There are, however, **two Critical issues** that must be resolved before
implementation: (1) a **cross-phase contradiction** — Phase 1 implemented `--icpc` as a *deferred HISAT2
flag*, but Phase 2 depends on it being the live FastQ-ID-truncation toggle; and (2) the **golden oracle in
validation #5 does not exist** as described (the Phase-0 spike neither produced nor committed
`subset.fq.gz_C_to_T.fastq`). Both are fixable with small plan edits, but as written the plan would send an
implementer down a wrong path on `--icpc` and to a missing file for the byte-identity gate.

A handful of **Important** items concern in-loop ordering precision (the `--icpc` warning, tab-detection,
and length-cutoff happen at specific points in the Perl loop and the plan's numbered order diverges
slightly), and validation gaps (no explicit test for the lowercase **and** CRLF interaction with `tr`, no
multi-member gzip test, no record-2+-malformed-is-NOT-checked assertion).

---

## 1. Byte-identity faithfulness (line-by-line vs Perl 5489–5651 / 6235–6246)

I walked the Perl loop and matched each step to the plan §3.3:

| Perl | Action | Plan | Match? |
|---|---|---|---|
| 5577–5582 | read 4 lines, `last unless` all four | §3.3 preamble + step "stop when any missing" | ✅ exact (truncated tail dropped) |
| 5584 | `chomp $identifier` (only `\n`) | §3.3.2 "strip a single trailing `\n` only; `\r` kept" | ✅ exact |
| 5585 | `fix_IDs($identifier)` | §3.3.2 + §"fix_IDs" | ✅ exact |
| 5586 | `$identifier .= "\n"` | §3.3.2 "re-append `\n`" | ✅ exact |
| 5588 | `++$count` | §3.3.1 "count += 1 **before** skip/upto" | ✅ exact |
| 5590–5592 | `if($skip){ next unless $count>$skip }` | §3.3.3 | ✅ exact (incl. falsy-0 in §"edge cases") |
| 5593–5595 | `if($upto){ last if $count>$upto }` | §3.3.3 | ✅ exact |
| 5597 | `$sequence = uc$sequence` | §3.3.4 "ASCII-uppercase whole seq line" | ✅ exact (newline preserved — seq NOT chomped) |
| 5598–5604 | `$maximum_length_cutoff` → `next` if too long | §3.3 (mentioned in §8 as inert) | ⚠️ see Important #3 — **omitted from the in-loop numbered steps** |
| 5607–5609 | `index($id,"\t")!=-1` → `$seqID_contains_tabs++` | §3.3.6 | ⚠️ ordering — see Important #2 |
| 5612–5616 | record-1 only: `$id !~ /^\@/ or $id2 !~ /^\+/` → die | §3.3.5 | ✅ logic exact; ⚠️ ordering — Perl sanity is AFTER tab-detect |
| 5624–5626 | `tr/C/T/` then `print join('',id,seq,id2,qual)` | §3.3.4 + §3.3.7 | ✅ exact (id2/qual verbatim) |

**Temp-file naming (§3.1 vs Perl 5491–5546):** correct. `$filename` is the basename with any
leading dir stripped via `m/(.*\/)(.*)$/` (extensions kept); `--prefix p` → `p.$filename`; then
`s/$/_C_to_T.fastq/` (or `.fastq.gz`). Full path `$temp_dir$filename`. Plan §3.1 matches, including the
"temp_dir already carries its separator / is empty for CWD" note (Perl concatenates `${temp_dir}${name}`
with no inserted separator — see 5548/5554 — so `temp_dir` MUST end in `/` when non-empty; Phase 1 stores
it as a `PathBuf` defaulting to `''`). **Implementation note for the implementer:** because Perl does raw
string concat (`$temp_dir$C_to_T_infile`), the Rust side must reproduce that concatenation semantics, NOT
`Path::join` (which would normalize and could differ on a non-slash-terminated temp_dir). The plan should
state this explicitly (see Important #5).

**`uc` then `tr/C/T/` lowercase net effect (§3.3.4):** the plan's table `a→A,c→T,g→G,t→T,n→N` is correct.
`uc('acgtn')` = `ACGTN`, then `tr/C/T/` only touches uppercase `C` → so lowercase `c` becomes `T` (via the
uc step), exactly as the plan states. ✅

**`fix_IDs` (§"fix_IDs" vs 6235–6246):** exact. Default `s/[ \t]+/_/g` (collapse run of spaces/tabs to a
single `_`); `--icpc` `s/[ \t].*$//g` (truncate at first space/tab). Operates on the chomped id with the
leading `@` retained. The plan's validation #1/#2 cases are correct (`@R 1:N`→`@R_1:N`, `@R\t1`→`@R_1`,
`--icpc`→`@R`).

**Verdict on faithfulness:** the conversion math is right. The issues below are about a cross-phase flag
contradiction, in-loop *ordering*, and validation, not the transform itself.

## 2. Original read intentionally NOT stored — correct?

**Yes, correct per the Perl architecture.** Phase 2 (`biTransformFastQFiles`) writes only the converted
temp file; the *unconverted* read sequence/quality are re-read later. Confirmed in the Perl: the
methylation-call path re-reads the original FastQ and re-applies `fix_IDs` independently at 2343/2421
(SE) and 2520/2620 (PE) — i.e. Bismark deliberately re-reads originals in lockstep rather than carrying
them through conversion. So §1/§7's "original re-read in lockstep (Phase 3+)" is architecturally faithful,
and Phase 2's sole deliverable being the converted temp path is the right seam. **One caveat for the
hand-off:** the re-read at 2343 etc. *also* runs `fix_IDs`, so the read-ID written into the temp file and
the read-ID the later loop matches on are produced by the *same* `fix_IDs`. The plan should note that the
Phase-2 `fix_id` helper is the **same** code Phase 3+ will reuse for the original re-read, so the two never
drift (it already says the helpers are "built reusable", which covers this — just make the lockstep-ID
consequence explicit so a later phase doesn't reimplement `fix_IDs` divergently). Low priority.

## 3. gzip handling

`flate2::read::MultiGzDecoder` for input is the right call and matches the sibling
`bismark-genome-preparation` precedent. Perl uses `gunzip -c $file |` (5500–5502), which is multi-member
tolerant, so `MultiGzDecoder` (not plain `GzDecoder`, which stops after the first member) is the correct
choice for decompressed-byte identity. **The plan says `MultiGzDecoder` in §2 — good.** The `--gzip` temp
output gated on *decompressed* content only is sound: the temp file is internal/transient and deleted, and
Perl pipes through `| gzip -c -` (5551) whose exact bytes (compression level, OS byte, mtime) we would
never reproduce with flate2 anyway, so gating raw bytes there would be a false gate. **No byte-identity
trap** as long as: (a) the *plain* `.fastq` path is the primary gate (it is, §8), and (b) the implementer
does not accidentally gate the `.gz` temp's raw bytes. One thing to add to validation: a test that
decompresses the `--gzip` temp output and `cmp`s the **decompressed** bytes against the plain run (the plan
has #6 "gzip input → identical to plain" but NOT the inverse "gzip *output* decompresses to the plain
content"). Minor — see Important #4.

## 4. `RunConfig` seam extension (`ReadProcessing`)

**Additive and sound in principle, but the plan under-specifies it and one field is mis-sourced.**

Verified against the implemented Phase 1: the read-processing options *do* currently live on `Cli`
(`cli.rs`): `skip: Option<u64>` (90), `upto: Option<u64>` (93), `gzip: bool` (177), `prefix:
Option<String>` (165), `icpc: bool` (217), `maximum_length_cutoff: Option<u32>` (223). They are **not**
on `RunConfig` — `config.rs` only carries `gzip`/`prefix` indirectly via `OutputTarget`, and does NOT
carry `skip`/`upto`/`icpc`/`maximum_length_cutoff` at all. So the plan's premise (§8, §10) that these
must be threaded into `RunConfig` is correct, and an additive `ReadProcessing` sub-struct is a clean,
non-disruptive extension (Phase 1's tests construct `RunConfig` only via `resolve()`, so adding a field
populated inside `resolve()` won't break them — verified there are no struct-literal `RunConfig{...}`
constructions in Phase-1 tests outside `resolve`).

**BUT note `gzip` and `prefix` already exist on `OutputTarget`.** The plan must decide whether
`ReadProcessing` *duplicates* `gzip`/`prefix` or whether Phase 2 reads them from the existing
`output: OutputTarget`. Duplicating them invites drift (two sources of truth for `--gzip`). Recommend:
`ReadProcessing` carries only the *new* fields (`skip`, `upto`, `icpc`, `maximum_length_cutoff`) and the
converter reads `gzip`/`prefix`/`temp_dir` from `output`. The plan should state this to avoid a
double-source seam. (Important #5.)

## 5. Edge cases & validation sufficiency (§9)

The edge-case *enumeration* in §3 is strong (gz, empty, truncated tail, CRLF, lowercase, `--gzip` temp,
skip≥total, falsy-0). The *validation table* has gaps:

- **#5 golden is the critical gap (see Critical #2)** — the named oracle file does not exist.
- **No test for the CRLF + lowercase interaction together**, nor for `tr/C/T/` leaving a trailing `\r`
  untouched on the seq line (a `\r` before `\n` survives `uc` and `tr` — should be asserted, since a
  naive `lines()`-based Rust impl that strips `\r` would silently diverge). #4 covers CRLF on id/seq but
  doesn't assert the seq line's `\r` survives the `C→T` transform specifically.
- **No multi-member gzip test** — the whole reason for `MultiGzDecoder` is multi-member `.gz`; #6 only
  tests a single-member `.fq.gz`. Add a concatenated-member input.
- **Record-2+ sanity is silently absent (correctly!) but untested** — Perl checks `/^\@/`,`/^\+/` *only*
  on record 1 (5612 `if($count==1)`). A malformed record 5 must NOT die. #8 tests record-1 malformed →
  die, but there's no test that a malformed-but-present record N>1 passes through verbatim (this is a
  silent-wrong-output risk if an implementer over-validates).
- **`--icpc` has no integration test** (only `fix_id` unit #2) — given the cross-phase confusion
  (Critical #1), an end-to-end test that `--icpc` actually truncates in the *written temp file* is
  warranted.
- **`upto=0`/`skip=0` falsy semantics untested** — §3 calls it out but #7 only tests `--skip 2 --upto 5`.
  Add `--upto 0` (must NOT stop at record 1) and `--skip 0` (must NOT skip) cases; these are the exact
  falsy-scalar traps where a Rust `Option<u64>` with `Some(0)` could behave differently from Perl's `if
  ($upto)`.
- **count semantics across skip:** §3.3.1 says count is the running record number incl. skipped. Worth an
  assertion that with `--skip 2` on 10 reads, the loop still counts 1..10 and `upto` is measured against
  the *unskipped* count (Perl 5588 increments before both checks). #7 says "count semantics match" but
  doesn't pin the expected count value — make it explicit.

The synthetic-golden approach (validation #5, RESOLVED in §10) is the **right** approach *if* the golden is
actually generated by Perl v0.25.1 and committed — but as written it points at a non-existent spike
artifact (Critical #2). A tiny hand-built FastQ (a few records exercising lowercase, CRLF, spaces+tabs in
ID, a comment after a space) run through `perl bismark` (or a minimal harness calling
`biTransformFastQFiles`) and committed as the oracle would be adequate and hermetic.

## 6. Efficiency

§6 is appropriate — linear, buffered, not a hot path relative to alignment. One concrete note: the
signature returns `Vec<u8>` from `fix_id`/`convert_seq_line` per record, which allocates twice per read.
For a port that's "not a hot path" this is fine, but since these helpers WILL be reused by the eventual
threaded/large-scale paths (Phase 9, PE), an in-place `&mut Vec<u8>` / reusable-buffer variant would be
strictly better and costs little now. Optional.

---

## Action items

### Critical (resolve before implementation)
1. **`--icpc` cross-phase contradiction.** Phase-1 `cli.rs:216–218` documents `--icpc` as *"HISAT2
   `--ignore-quals` variant (deferred)"* — but the Perl `$icpc` (7375, 6238, 8430–8431, help 9861) is the
   **FastQ read-ID truncation toggle** that `fix_IDs` keys on, and Phase 2 depends on it being live. The
   plan must (a) note this Phase-1 mislabel as a documented seam correction, (b) confirm `--icpc` is wired
   into `ReadProcessing` as an *active* (not deferred) flag for Phase 2, and (c) ensure Phase 1's
   "deferred flags" notice (`config.rs:240–263`) does NOT list `--icpc` (it currently doesn't — but it
   also isn't carried into `RunConfig`, so it's effectively dropped today). Without this, an implementer
   following Phase 1's comment will treat `--icpc` as inert and silently produce wrong temp files when
   `--icpc` is supplied.
2. **Validation #5 golden does not exist.** The plan says "reuse the Phase-0 spike's
   `subset.fq.gz_C_to_T.fastq`", but the spike (`phase0-determinism-spike/`) committed only `.md`/`.sh`,
   ran Bismark with `--temp_dir` (which deletes the converted temp after alignment), and built its own
   `ct.fq` via `awk` (line 73) — *not* `biTransformFastQFiles` output. There is no committed
   `_C_to_T.fastq` anywhere in the worktree. Replace #5 with: generate a small deterministic golden by
   running **Perl bismark v0.25.1** (or a minimal harness) on a hand-crafted FastQ that exercises
   lowercase / CRLF / space+tab IDs / a post-space comment, and **commit it** as the oracle. Mark it
   hermetic + regenerable.

### Important
3. **`maximum_length_cutoff` belongs in the in-loop numbered steps.** Perl applies it at 5598–5604
   (`next` if `length$sequence > cutoff`) *between* `uc` and the tab/sanity/write steps. §3.3 omits it
   from the numbered loop (only §8 mentions it as "inert for Bowtie 2"). It IS inert on the v1 spine
   (only defined for `--minimap2`, 8345–8356), so it never fires — but the plan should add it as an
   explicit (guarded, never-taken on v1) loop step so the loop structure matches Perl exactly and the
   later mm2 phase has the hook. Also note: in Perl it's `Option`-like via `defined`, and when set it's
   floored to 10000 default — but again, mm2-only.
4. **In-loop ordering precision.** Perl order is: `uc` → length-cutoff → **tab-detect** → **record-1
   sanity** → `tr/C/T/`+write. The plan §3.3 lists sanity (step 5) *before* tab-detect (step 6). Neither
   affects written bytes (tab-detect sets a warning flag; sanity only fires on record 1 and either dies or
   passes), so this is **not** a byte-identity risk — but for a faithfulness-paramount port, reorder the
   numbered steps to match Perl (tab-detect at 5607 precedes sanity at 5612) to avoid an implementer
   "fixing" the order later and a reviewer flagging a phantom divergence. Document that the order is
   byte-neutral.
5. **Tighten the `ReadProcessing` seam + temp-path concat.** (a) State that `ReadProcessing` carries only
   the *new* fields (`skip`/`upto`/`icpc`/`maximum_length_cutoff`) and that `gzip`/`prefix`/`temp_dir` are
   read from the existing `OutputTarget` (avoid two sources of truth for `--gzip`/`--prefix`). (b) State
   that the temp path is built by **raw string concatenation** (`format!("{temp_dir}{name}")`-style),
   mirroring Perl's `${temp_dir}${infile}`, NOT `Path::join`, because a non-`/`-terminated `temp_dir`
   would otherwise normalize differently.
6. **Add the missing validations** from §5 above: multi-member gzip; `--gzip` output decompresses to the
   plain content; record-N>1 malformed passes through verbatim (no over-validation); `--icpc` end-to-end;
   `--upto 0`/`--skip 0` falsy semantics; pinned `count` value across `--skip`. These directly target the
   silent-wrong-output modes a byte-identity gate must catch.

### Optional
7. In-place/reusable-buffer variant of `fix_id`/`convert_seq_line` to avoid 2 allocs/record, since these
   helpers are explicitly reused by later (threaded/PE) phases.
8. Make explicit in §7 that the Phase-2 `fix_id` is the **same** helper Phase 3+ uses for the original-read
   re-read (Perl 2343/2421/2520/2620), so the in-temp read-ID and the lockstep match-ID can never drift.
9. The plan calls the truncated-tail behaviour "Perl's `last unless`" — precise; consider a one-line test
   that a 3-line trailing fragment after N complete records yields exactly N records (boundary clarity).

---

## Assumptions surfaced / validated

- **Validated:** read-processing options currently live on `Cli`, not `RunConfig`; additive
  `ReadProcessing` is non-disruptive to Phase-1 tests (no external `RunConfig{...}` literals).
- **Validated:** the Perl conversion math (`chomp`/`fix_IDs`/`uc`→`tr/C/T/`/verbatim id2+qual/`count++`
  before skip+upto/record-1-only sanity/`last unless` tail) — all match the plan.
- **Validated:** `--gzip` decompressed-only gate is correct (Perl `| gzip -c -`; flate2 bytes never match
  → gating raw bytes would be a false gate).
- **Flagged assumption:** the plan assumes `--icpc` is a live toggle — TRUE in Perl, but Phase-1 code
  comments call it deferred (Critical #1). Must be reconciled.
- **Flagged assumption:** the plan assumes a reusable Phase-0 golden exists — FALSE (Critical #2).
