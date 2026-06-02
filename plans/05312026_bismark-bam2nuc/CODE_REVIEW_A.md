# Code Review A — `rust/bismark-bam2nuc` (binary `bam2nuc_rs`)

**Reviewer:** Code Reviewer A (independent; recommend-only — no source mutated)
**Date:** 2026-05-31
**Scope:** Port of Perl `bam2nuc` v0.25.1. Acceptance gate = byte-identity of `*.nucleotide_stats.txt` and `genomic_nucleotide_frequencies.txt` vs Perl v0.25.1.
**Build/test:** `cargo test -p bismark-bam2nuc` → **all green** (12 golden + 2 sanity + ~30 unit + 1 ignored real-data smoke). `cargo clippy -p bismark-bam2nuc --all-targets -- -D warnings` → **clean**. (Sandbox blocked `target/` writes; re-ran with sandbox disabled.)

---

## Summary

This is a careful, well-documented port. The byte-identity-critical machinery — the `process_sequence` counter, the bytewise-sorted cache serializer, the separate-mono/di totals, the `%.2f`/`%.3f` formatting, the empty-count-field invariant, the cache-reuse precedence, the SE die / PE-`or 163` strand logic, and the case-sensitive no-dot-anchor output-name strip — all match the Perl source as I traced it line-by-line. The golden suite exercises the fragile cells (empty field `\t\t`, IUPAC cache lines, planted-cache reuse, non-canonical PE flag, all-InDel ZeroDivision). I verified noodles-fasta's `Definition` parser takes the first-whitespace token as the name (matching Perl `extract_chromosome_name`), so genome keys line up with BAM `@SQ SN:` names.

I found **one genuine latent divergence** (the `POS == 0` span extraction, with a misleading code comment), **one undocumented behavioral divergence** (non-ASCII chr-name hard error), and a handful of Low items. None are reachable on real Bismark BAMs, so none threaten the oxy gate — but the `POS==0` case is a real correctness/faithfulness bug and the comment is wrong, so I rate it High among the findings even though its trigger is adversarial.

Highest-confidence action items: **A1** (fix the `extract_span` POS==0 divergence + comment), then **A2** (document the non-ASCII chr-name divergence as D8).

---

## Issues by area

### 1. Logic / byte-identity

#### A1 — `extract_span` diverges from Perl `substr` for `POS == 0` (and its doc comment is wrong) — **HIGH**
`count.rs:76-85` + comment `count.rs:176-178`.

`count_records` maps a missing `alignment_start` to `pos1 = 0` (`count.rs:178`: `record.alignment_start().map_or(0, usize::from)`), with the comment claiming "None (unmapped) → 0 so the span is empty". That claim is **false** when the record still carries a valid `reference_sequence_id` pointing at a present chromosome:

```
p     = pos1.saturating_sub(1) = 0
start = 0.min(n)               = 0
end   = 0.saturating_add(read_len).min(n) = read_len.min(n)
seq[0..read_len]   // NON-empty span from the START of the chromosome
```

Perl (`bam2nuc:133`) computes `substr($chromosomes{$chr}, $start - 1, length$sequence)` with `$start = 0`, i.e. `substr($seq, -1, len)`. A **negative** Perl offset counts from the end, so Perl returns just the **last 1 character** of the chromosome — NOT `seq[0..read_len]`. So for a `ref_id=Some` + `POS=0` record:
- Perl counts 1 genomic base (the chromosome's last base).
- Rust counts up to `read_len` bases from the chromosome's front.

This is a true divergence. It is **unreachable on real Bismark BAMs** (Bismark never writes unmapped/POS=0 records, and a mapped read always has POS ≥ 1), and for an SE stray-flag record the `correct_se` error fires anyway — but a PE record with `ref_id=Some` + `POS=0` (flag ∉ {99,147}) would silently mis-count via the revcomp path. The goldens don't exercise it.

Recommendation: return an empty span when `pos1 == 0` (the only faithful-and-safe choice; Perl's `substr(-1)` "last base" behaviour is itself nonsensical here), and fix the misleading comment. E.g. early-return `Vec::new()` if `pos1 == 0`, or guard `count_records` to skip the span+correction entirely when `alignment_start().is_none()` while still routing the flag through `correct_se` to preserve the SE die contract. Byte-neutral on the gate; removes the latent PE mis-count.

#### A2 — Non-ASCII chromosome-name hard error is an UNDOCUMENTED divergence — **MEDIUM**
`count.rs:45-57` (`build_chr_name_table`) returns `NonAsciiChromosomeName` for any `@SQ SN:` with non-ASCII bytes. Perl `bam2nuc` performs **no such check** — it uses the chr name verbatim as a `%chromosomes` hash key (`bam2nuc:119,133`) and would happily count a non-ASCII chr present in both BAM and genome. This is a behavioral divergence not listed in the documented deviations (D1–D7 / D-impl-1..4). It is inherited from `bismark-extractor::header` and is unreachable on Bowtie2-built Bismark genomes (ASCII names), but it should be recorded as a deviation (suggest **D8**) for the same completeness reason D6/D7 were. Byte-neutral on the gate.

#### A3 — SE/PE detection: documented Q5 divergence is slightly wider than stated — **LOW**
`count.rs:140-141` uses `bismark_io::detect_paired_from_header`. Beyond the documented "ID:Bismark vs first-`@PG`" point, the helper (`bismark-io/src/read.rs:672-673`) also treats `--1`/`--2` as present, whereas Perl's `/\s-1\s+/` regex does **not** match `--1` (the char before `-1` in `--1` is `-`, not `\s`). Only matters for the double-dash invocation form, which Bismark never emits. Subsumed by Q5 ("equivalent for real Bismark BAMs"); noting for completeness. No action needed.

#### A4 — Cache reader is stricter than Perl on malformed lines — **LOW (already documented)**
`freqs.rs:199-219` errors (`MalformedCacheLine`) on a line that isn't `<word>\t<count>` or whose word length ∉ {1,2}, where Perl silently stores it. Already documented (error.rs:71-81 "accepted divergence; cannot occur on a Perl-written cache"). A Perl-written cache only ever has 1/2-byte words and `word\tcount` lines, so the gate (Perl-then-Rust) is safe. One micro-note: a CRLF cache would have Rust's `lines()` strip `\r` while Perl `chomp` leaves it on the freq token — both still coerce to the same integer, so byte-identity of the *downstream stats* holds. No action.

### 2. Efficiency

#### A5 — 1–2 `Vec<u8>` allocations per read in the hot loop — **LOW**
`extract_span` (`count.rs:84`) allocates a `Vec` per kept read; reverse-strand reads allocate a second `Vec` in `revcomp` (`count.rs:89-100`). On a ~55 M-read file that's tens of millions of small allocations. mimalloc mitigates this (and the SPEC justifies mimalloc precisely for this), and `process_sequence` could in principle count in place / from a reused scratch buffer (revcomp into a thread-local `Vec`, count without materialising the forward span). Correctness is fine; flagging only as a future throughput lever. Byte-neutral.

#### A6 — `cache_bytes` scans the full 256 + 65536 counter space — **LOW (non-issue)**
`freqs.rs:79-90` iterates all 65 792 slots on every call. Called once per genome compute (and once per cache round-trip test); negligible. No action.

### 3. Errors / panics / security

- **No reachable panics in non-test code.** `output_name.rs:29` uses `unwrap_or_default()`; `report.rs:134` `expect` is on the guaranteed-ASCII `MONO`/`DI` constants; `freqs.rs:213-214` index fixed-size arrays only after a `wb.len()` match; the `NucCounts` mono/di indexing is bounded by construction (`u8`→256, `(u8<<8|u8)`→65536). Confirmed via `grep` over `src/`.
- **Partial-file-on-error depends on `BufWriter` Drop-flush.** `lib.rs:84-86` wraps the output in an 8 KiB `BufWriter`; when `write_stats` returns `ZeroDivision` mid-routine, `out` is dropped without an explicit `flush`, so the already-written header/rows reach disk only via `BufWriter`'s best-effort `Drop` flush. The full stats file is <1 KiB (well under the buffer), so the entire partial lands, matching Perl's autoflush (`$|++`) partial-file behaviour — and `all_indel_sample_zerodivision_exits_one` confirms the header is present. This is **byte-neutral** (the partial file is never byte-gated; the gate only covers successful real-data runs). Noting the implicit dependency only. **LOW.**
- **Format-gate (content sniff) vs output-name (extension) can classify differently** for adversarially-misnamed files (e.g. `x.bam` containing SAM text → rejected by content; `x.dat` containing BAM magic → accepted then `NotBamOrCram`). Both reject, at different points; byte-neutral; unreachable on real pipeline output. **LOW.**
- No secrets, no `unsafe` (`#![forbid(unsafe_code)]`), no command/path injection (no subprocess; `--samtools_path` is dropped).

### 4. Structure / idiom

- Module split (cli/error/genome/freqs/count/report/output_name/lib/main) is clean and matches SPEC §15. Doc comments cite the exact Perl line ranges, which made verification straightforward.
- `NucCounts` `count == 0 ⇔ absent` invariant is a tidy, allocation-free realisation of Perl's `undef`→empty-field / undef→0-in-math semantics; the report and cache both lean on it consistently. Good.
- The PLAN text's "packed-`u16` di array" wording (PLAN:80) refers to the *index* packing, not the count type; the impl correctly uses `u64` counts (a `u16` count would overflow on real genome di-counts in the billions). No discrepancy — just confirming the impl choice is the safe one.
- D-impl-1..4 deviations all match the code (`cache_bytes()->Vec<u8>`, the `BamIo` error variant, the de-Bruijn all-16-di-words golden genome, the `text`-fenced report doc). D1–D7 verified against the code:
  - **D1/D1a:** `--samtools_path` parsed (`cli.rs:53-55`) and dropped (`cli.rs:102`), never validated. ✓
  - **D2:** `version_string()` one-liner (`lib.rs:96-103`). ✓
  - **D3:** `correct_pe` 99/147→fwd else revcomp, never errors (`count.rs:120-126`); golden `pe_noncanonical` (flag 65) proves it. ✓
  - **D4:** `derive_output_name` strips trailing lowercase `bam`/`cram`, no dot anchor, case-sensitive (`output_name.rs:33-38`); tests cover `foosubbam`, `a.bam.bam`, `weird.BAM`→Err. ✓
  - **D5:** stderr-only progress (`eprintln!`). ✓
  - **D6:** bare `>` header → `MalformedFastaHeader` (verified noodles `Definition::FromStr` → `MissingName` → `InvalidData`). ✓
  - **D7:** `*`-SEQ → `sequence().len()==0` → empty span; unreachable. ✓

---

## Recommendations (priority-ordered)

| # | Priority | File:line | Action |
|---|---|---|---|
| A1 | **High** | `count.rs:76-85`, comment `count.rs:176-178` | Return an empty span when `pos1 == 0` (and fix the comment that wrongly claims POS-0 already yields an empty span). Perl `substr($seq, -1, len)` returns the chromosome's last base, not `seq[0..read_len]`; the current code mis-counts a `ref_id=Some`+`POS=0` PE record via the revcomp path. Unreachable on real Bismark BAMs but a faithful-to-Perl correctness gap + an incorrect comment. Byte-neutral on the gate. |
| A2 | **Medium** | `count.rs:45-57` | Document the non-ASCII chr-name hard error as a deviation (suggest **D8**) — Perl performs no such check (`bam2nuc:119,133` uses the name verbatim as a hash key). Inherited from `bismark-extractor::header`; unreachable on ASCII Bismark genomes; byte-neutral. Doc-only. |
| A3 | Low | `count.rs:140-141` (helper `read.rs:672`) | Note (already ~covered by Q5) that the SE/PE helper also accepts `--1`/`--2`, which Perl's `/\s-1\s+/` does not. No code change; Bismark never emits the double-dash form. |
| A4 | Low | `freqs.rs:199-219` | Already documented as an accepted divergence (stricter cache parse). No change. |
| A5 | Low | `count.rs:84,89-100` | Future throughput: avoid the per-read span/revcomp `Vec` allocations (count from a reused scratch buffer). Byte-neutral; mimalloc already mitigates. |
| A6 | Low | `lib.rs:84-86` | Optional: note (or assert in a test) the reliance on `BufWriter` Drop-flush for the partial-file-on-ZeroDivision parity, so a future change to buffer size / explicit-flush ordering doesn't silently regress it. Byte-neutral. |

**No Critical findings.** The byte-identity contract for real Bismark SE/PE BAMs and the genomic cache is faithfully reproduced; the gate-relevant paths match Perl. The single High item (A1) is a latent, real-data-unreachable divergence with a wrong comment; everything else is Low/doc.

---

### Verification notes for the caller

- Re-ran the full suite + clippy with the sandbox disabled (the only way `target/` was writable here); both clean.
- I verified the noodles-fasta `Definition` first-token-name parsing (`~/.cargo/.../noodles-fasta-0.61.0/src/record/definition.rs:107-130`) and the `detect_paired_from_header` semantics (`rust/bismark-io/src/read.rs:649-696`) directly against source, since those underpin the genome-key↔BAM-chr-name match and the SE/PE gate.
- I could **not** execute Perl in this environment (harness blocked `/usr/bin/perl` even with the sandbox disabled), so the Perl `substr(-1, len)` semantics in A1 are stated from the language spec rather than a live run — please confirm with `perl -e 'print substr("ACGTACGT",-1,4)'` (expected: `T`) if you want a belt-and-braces check before acting on A1.
