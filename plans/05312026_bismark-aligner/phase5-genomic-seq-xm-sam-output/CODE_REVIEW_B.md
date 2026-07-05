# Code Review B — Phase 5 (genomic-seq extraction + XM/XR/XG call + SAM/BAM output, SE directional)

**Reviewer:** B (independent, parallel dual review)
**Scope:** `rust/bismark-aligner` Phase-5 changes — `genome.rs`, `methylation.rs`,
`output.rs`, `lib.rs` (driver), `config.rs`, `merge.rs` (Counters), `tests/cli.rs`.
**Oracle:** Perl `bismark` v0.25.1 (`/Users/fkrueger/Github/Bismark-aligner/bismark`).
**Verdict:** **APPROVE.** No Critical or High findings. The port is a faithful,
byte-accurate transcription of the Perl. The highest-risk surface
(`make_mismatch_string` + the multi-deletion re-indexing) was **independently
verified byte-identical against the live Perl across 27 cases** (see §"Deletion
verification"). All Medium/Low items below are documentation/robustness notes, not
correctness defects on the gated (length-guard-passing) path.

---

## Summary

Phase 5 produces the first real Bismark BAM. I traced every named sub against the
Perl line numbers in the brief and re-derived the load-bearing arithmetic:

- **`make_mismatch_string` / `rebuild_md_with_deletions`** — verbatim, **proven
  byte-identical** to Perl 9252–9595 across clean, single-mismatch, leading/adjacent
  mismatch, single/double/triple deletion, deletion-with-mismatch (before/between/
  after), multi-base deletions, leading deletion, and the trailing-token path.
- **Per-strand counters** (`ct_ct`/…) are bumped in extraction **only past both edge
  guards** (Perl 4317/4390); `unique_best` stays at merge (Perl 3121);
  `could_not_extract` is bumped in the driver after the **length** guard (Perl 3127/3129).
  Placement matches the Perl exactly — an edge read counts in `unique_best`, in **no**
  strand bucket, and in `could_not_extract`, and is not written.
- **The two complement helpers** (`reverse_complement` 5161 vs `revcomp` 9228) are
  **both** verified against live Perl on upper/lower/`N` inputs; they differ on
  lower-case exactly as Perl does. The minus-strand+deletion double-revcomp (4419 +
  8581) composes correctly (net identity on the upper-cased genome).
- **FLAG / +2 trim / NM = hemming + `D`-only indels / tag order `NM MD XM XR XG` /
  XM-reverse / QUAL ASCII→phred** — all match.
- **Header** — `@HD VN:1.0 SO:unsorted`, `@SQ` from `sq_order`, Bismark `@PG` with
  `VN` before `CL` and literal embedded quotes — pinned by a literal byte-diff test
  that passes.

108 unit + 16 integration tests pass; clippy `-D warnings` and `cargo fmt --check`
are clean (re-confirmed locally).

---

## Deletion verification (the highest-risk port)

I vendored the exact Rust `parse_cigar` + `make_mismatch_string` +
`rebuild_md_with_deletions` into a standalone binary and the exact Perl
`make_mismatch_string` into a Perl harness, then diffed their output over **27
cases**. **All 27 matched byte-for-byte.** Representative results (`actual|ref|cigar|md_seq → MD`):

```
ACGTAC|ACGTAC|2M1D2M1D2M|ACTGTAAC          → MD:Z:2^T2^A2      (≥2 deletions)
ACTT|ACGT|2M1D2M|ACNGT                     → MD:Z:2^N0G1       (deletion + mismatch)
ACGTACGTACGTACGT|ACGTACGTACGTACGA|5M1D5M1D6M|… → MD:Z:5^G5^G5A0 (2 del + trailing-token mismatch)
ACGTACGTACGT|ACGTACGTACGA|3M1D3M1D3M1D3M|…  → MD:Z:3^T3^G3^C2A0 (3 del + final-token mismatch)
TCGTACGT|ACGTACGT|4M1D4M|ACGTAACGT         → MD:Z:0A3^A4       (leading mismatch + del)
G|G|1D1M|TG                                → MD:Z:0^T1         (leading deletion)
ACGT|ACGT|2M3D2M|ACTTTGT                   → MD:Z:2^TTT2       (multi-base deletion)
```

This exercises the in-loop digit arm, the in-loop "wasn't-the-last-deletion"
re-indexing (`current_md_index = cmi + len(delstr) − len(op)`,
`md_index_already_processed = …−1`), and the **trailing arm** (Perl 9526–9578, the
`md_index_already_processed = current_md_index` **without** the `−1`, Rust output.rs:308–311).
Both arms produce Perl-identical output. I consider the deletion machinery
correct and do not recommend further change.

The in-loop guard difference is benign: Perl's `if ($op =~ /\d+/ …)` (contains a
digit) vs Rust's `!op_str.is_empty() && op_str.bytes().all(is_ascii_digit)` (all
digits). For every **reachable** value of `$op` (pure-digit string, single non-digit
mismatch base, or empty), the two predicates agree — the final `$op` is never a
mixed digit/non-digit string.

---

## Issues by area

### Logic — none (Critical/High)

All control flow matches the Perl. Specifically confirmed:

- **Edge-guard early return doesn't bump strand counters** (`methylation.rs:155–161`,
  `202–207` vs Perl 4317/4390). ✓
- **`indels` accrues for `D` only** (`methylation.rs:182–188`; `I`/`S`/`N` do not —
  Perl 4346/4360/4376). ✓  NM-with-indels is therefore `hemming + D_count`.
- **+2 prepend/append goes to `unmodified_genomic_sequence` only, never
  `genomic_seq_for_md_tag`** (`methylation.rs:159–160`, `206` vs Perl 4322/4395). ✓
- **`methylation_call` runs on the FULL window before the ref-seq trim/revcomp**
  (`lib.rs:301–306` then `single_end_sam_output` does the trim). ✓
- **Out-of-range context look-ups use sentinel `0`** (`methylation.rs:254`), which is
  neither `G`/`C`/`N`/`X` — identical to Perl's `undef`/empty-`substr` fall-through to
  CHH/`.`. Note this only ever fires for edge-truncated windows, which the length guard
  drops; for full windows `genomic[i+1]`/`[i+2]` are always in range. ✓
- **QNAME = `chomp → fix_id → strip @`** (`lib.rs:269–271`) matches Perl SE-FastQ loop
  (2420–2421 `chomp`+`fix_IDs`, 2442 `s/^\@//`). I verified `fix_IDs` **is** applied
  in the main read loop (not only the unmapped writer paths). ✓
- **Output-name derivation** (`lib.rs:183–209`) matches Perl 1562–1607 (suffix strip →
  `--prefix.` → `_bismark_bt2.bam` → `--basename` override → `output_dir` join). ✓
- **Counter placement**: `unique_best` at merge (`merge.rs:279`, Perl 3121), per-strand
  in extraction past guards (`methylation.rs:210–216`, Perl 4402–4441),
  `could_not_extract` in driver after length guard (`lib.rs:293–300`, Perl 3127–3130). ✓

### Errors / robustness — Low

1. **(Low) Read IDs round-tripped through `String::from_utf8_lossy`** (`lib.rs:271`).
   Perl treats QNAMEs as raw bytes; the Rust driver lossily converts the ID (and the
   sequence, `lib.rs:274`) to `String`. For ASCII FastQ this is inert, but a QNAME with
   an invalid UTF-8 byte would be silently replaced with U+FFFD, diverging from Perl.
   Real Illumina IDs are ASCII, so this won't affect the gate; flagging for awareness.
   (Same pattern likely predates Phase 5 in the Phase-2/4 driver.)

2. **(Low) Defensive `unwrap_or(0)` in `make_mismatch_string` Part 1** (`output.rs:134`).
   If `ref_seq.len() < actual.len()`, the sentinel `0` would be pushed as a NUL char.
   This is **unreachable** on the gated path (length guard ⇒ `actual.len() ==
   ref_seq.len()` after the +2 trim, including `X`-padded insertions/soft-clips), so it
   is purely defensive. No action needed; documenting that the invariant is what makes
   it safe.

3. **(Low) QUAL `wrapping_sub(offset)`** (`output.rs:376`). A quality byte below the
   offset (e.g. a literal `*` in a FastA-derived placeholder QUAL) would wrap to a huge
   phred score. The plan scopes FastA to Phase 9 and real FastQ qualities are ≥ `!`(33);
   safe for v1. Consider a debug-assert in a later phase if FastA lands.

### Structure / efficiency — Low

4. **(Low) CIGAR parsed three times per record.** `parse_cigar` runs in extraction,
   again in `rebuild_md_with_deletions` (only for `D` reads), and again in
   `cigar_to_ops` (`output.rs:441`). Linear and tiny; not worth threading the parsed
   runs through the seam for v1. Noting only.

5. **(Low) `cigar_to_ops` fallback `_ => Kind::Match`** (`output.rs:451`). Unreachable
   (extraction already validated the ops), but a silent `Match` fallback on an unknown
   op would mis-encode rather than fail. Since it's genuinely unreachable, leaving it is
   fine; a debug-unreachable would be marginally safer.

6. **(Low) `header_text` test helper duplicates the `bismark-io` writer path.** The
   `header_hd_sq_pg_exact_bytes` test serialises via `noodles_sam::io::Writer`, whereas
   production writes via `bismark-io::BamWriter`. They share the same noodles
   serialiser, so the byte assertion is valid, but the *true* gate (`samtools view -H`)
   is the Linux #18 run (correctly PENDING). Not a defect.

---

## Cross-checks against the Perl that PASSED (no action)

- `reverse_complement` (5161) and `revcomp` (9228) tr-maps + reverse order: verified on
  `ACGT/AAAA/ACGTN/acgt/ACGTacgtN/NNNN/CCGG/actgACTG` — Rust matches Perl exactly,
  including lower-case divergence (`reverse_complement("acgt")="tgca"` vs
  `revcomp("acgt")="ACGT"`).
- FLAG table (`output.rs:349–362`) == Perl 8521–8546 for all four (strand,XR,XG) combos;
  illegal combos error.
- `ref_seq` trim: CT drops last 2 (`output.rs:367`), GA drops first 2 (`output.rs:368`)
  == Perl 8570–8575.
- Minus-strand: revcomp SEQ + revcomp ref + (if `D`) revcomp md_seq + reverse QUAL
  (`output.rs:379–386`) == Perl 8577–8584; XM reversed (`output.rs:399–403`) == 8601–8607.
- `@SQ` order is consumed from `genome.sq_order` (single source = Phase-1 `fastas`);
  `genome.rs` does not re-glob; `sq_order` = encounter order across files + within
  multi-FASTA (record order, NOT sorted) — matches Perl `%SQ_order`.
- Genome loader: gunzip `.gz`, header→`extract_chromosome_name` (first whitespace token,
  `""` on leading space → loader dies), `chomp`+single-`\r`-strip (no `/g`), `uc`,
  dup-name die, empty-seq warn, empty-name die in loader. == Perl 5022–5159.
- `phred64` ASCII−64 (→ noodles phred score → `samtools` renders +33) == Perl 4191
  `convert_phred64_quals_to_phred33` (−64 then +33).
- `BISMARK_VERSION = "v0.25.1"` == Perl `$bismark_version` (line 28).
- `from_noodles_record` re-validates XR/XG presence + `XM.len()==SEQ.len()` — a free
  guard; holds for all valid reads.

---

## Recommendations (prioritized)

- **Critical:** none.
- **High:** none.
- **Medium:** none.
- **Low (optional, defer-friendly):**
  - (L1) Consider keeping QNAMEs as raw bytes through the driver rather than
    `from_utf8_lossy`, to be byte-exact on pathological non-ASCII IDs (also applies to
    the Phase-2/4 driver — out of Phase-5 scope, note for a later sweep).
  - (L4/L5) Optionally `debug_assert!`/`unreachable!` the `cigar_to_ops` op fallback and
    document the `make_mismatch_string` Part-1 length invariant inline.
  - Add (if cheap) one unit MD case combining **≥2 deletions with a mismatch between
    them** to the in-repo test set — the standalone harness proves it works, but the
    committed suite (`md_two_deletions`, `md_deletion_with_mismatch`) doesn't cover that
    exact combination. Strengthens the regression net for plan §9 #9.

The **only** remaining gate is the intentionally-PENDING Linux/oxy byte-identity run
(§9 #18), which is out of scope for this review.
