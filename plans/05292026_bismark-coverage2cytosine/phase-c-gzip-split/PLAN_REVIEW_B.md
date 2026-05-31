# Phase C PLAN — Review B

**Reviewer:** Plan Reviewer B (independent, fresh context).
**Target:** `phase-c-gzip-split/PLAN.md` rev 0 — `--gzip` + `--split_by_chromosome` for the Rust `coverage2cytosine` port.
**Contract:** byte-identical to Perl `coverage2cytosine` v0.25.1 (gzip after decompression).
**Method:** read SPEC rev 3, EPIC, the Perl ground truth (`handle_filehandles:89-165`, split call sites `:200,216-219,457-466`, `process_unprocessed_chromosomes:1388-1565`, `print_context_summary:63-78`, `%processed` init `:1712/1734`), the shipped Phase B `src/{report,cov,summary}.rs` + `tests/golden_phase_b.rs`, and **ran the repo Perl v0.25.1 live** against deliberately-crafted fixtures (unsorted/non-contiguous cov, fully-covered genome, `--split`, `--split --gzip`, `--gzip`). Phase B baseline rebuilt + tested green (11 + 5 tests).

---

## Verdict

**APPROVE WITH CHANGES.** The gzip half is sound and fully verified (decompress == plain; summary stays plain; empty-gzip stream is real). The split half has **one Critical model error**: the plan's stated rule for *per-chr file lifetime* and *which chromosome holds the summary* is an oversimplification that is **byte-wrong on a non-contiguous (re-appearing) chromosome** — a case Phase B explicitly supports and tests (`non_contiguous_chromosome_re_emits`). The plan must adopt the precise Perl rule ("each chr-transition reopens+truncates the per-chr file; the summary lands in the chr of the *last* `handle_filehandles` call") before implementation. All other claims I checked are empirically correct.

---

## What I verified against live Perl (all confirmed)

Fixture genome (FASTA order): `chr1, chr2, scaf_short(2bp "CG"), chr3uncov`. Cov covers a subset.

| Plan claim | Live-Perl result | Verdict |
|---|---|---|
| `--gzip` report decompresses to the plain report | `gunzip -c gz.CpG_report.txt.gz` == `plain.CpG_report.txt`, **identical** | ✅ correct (V3) |
| `--gzip` summary is plain ASCII, == plain summary | `file` → "ASCII text"; `diff` identical | ✅ correct (V4) |
| `--split --gzip` per-chr report decompresses to plain split per-chr | all 4 chrs: `gunzip` == plain split, **identical** | ✅ correct (V9) |
| zero-emitting chr in `--split --gzip` → **valid empty-gzip stream** (not 0-byte) | `scaf_short` report = **20 bytes**, `1f8b…` magic, `gunzip` → 0 bytes | ✅ confirmed — §10 Q1 **resolved**: must `GzEncoder::finish()` with no writes |
| every genome chr gets a report file (covered + uncovered, incl. zero-emitting) | all 4 report files present; `scaf_short` = 0-byte plain | ✅ correct (V6) |
| every genome chr gets a summary file; only the last is non-empty | 3× 0-byte + 1× full (1310 B) summary | ✅ correct (V8) |
| covered-but-**zero-emitting** chr still gets a (0-byte) report file | `scaf_short` in cov → 0-byte report present | ✅ correct |
| split last-chr summary CONTENT == non-split summary content | `diff plain.summary psplit.<lastchr>.summary` → **identical** (1310 B) | ✅ correct (§4b) — whole-genome accumulation unchanged |
| fully-covered genome (no uncovered pass): summary → **last covered in cov-appearance order** | cov order `chrB,chrA` → summary in `chrA` (1310 B), `chrB` 0-byte; note `chrA` sorts *before* `chrB` — so it is cov-order, not bytewise | ✅ §10 Q2 **resolved**: last covered (cov order), NOT bytewise-last |
| `.chr` infix is literal & **before** the report suffix (`split.chrchr1.CpG_report.txt`) | confirmed | ✅ correct — note this **contradicts SPEC §5 table** which shows `…CpG_report.txt.chr<NAME>` (suffix after chr). The PLAN is right; the SPEC table row is misleading. |

The gzip header carries an **mtime** (bytes 4–7 of every stream, e.g. `dfdb 196a`), so raw gzip bytes are timestamp-dependent — the plan's decompress-then-compare contract (§3.1, V3/V9) is not just convenient, it is **necessary**. Good.

---

## Logic review

### CRITICAL — the per-chr file is **reopened+truncated** on every chr-transition; the plan's append/first-appearance model is byte-wrong on a re-appearing chromosome

Perl split-mode file lifetime (`generate_genome_wide_cytosine_report:457-466` + `handle_filehandles:99-150`):

```perl
if ($split_by_chromosome and $current_out_chr ne $last_chr){
    close CYT or warn $!;
    $current_out_chr = handle_filehandles($last_chr);   # opens '>' => TRUNCATE
}
```

`handle_filehandles` opens `CYT` with `'>'` (truncate, `:146`) and `CONTEXTSUMMARY` with `'>'` (truncate, `:117`) **unconditionally**, every time it is called. It is called for: (a) the first covered chr (`:217`), (b) **every** covered transition where the chr field changes (`:457-465`), and (c) every uncovered chr (`process_unprocessed_chromosomes:1396`).

**Consequence the plan misses:** Phase B *explicitly supports* a non-contiguous chromosome (cov `chrA … chrB … chrA`), flushing+re-emitting on each transition — see `tests/golden_phase_b.rs:110` `non_contiguous_chromosome_re_emits`, which asserts chrA appears **twice (≥4 lines)** in the single shared non-split file (both flushes append). In **split** mode the same input does NOT append: the `chr1→chr2→chr1` re-appearance **closes and reopens (truncates)** `…chrchrA.CpG_report.txt`, so the file ends up holding **only the last contiguous segment** of chrA. I verified this live:

- cov `chr2,chr1,chr1,chr2` (chr2 re-appears) → `split.chrchr2.CpG_report.txt` contains only the **second** chr2 segment; the first chr2 coverage (pos 9) is **gone** (shows `0 0`).
- Fully-covered cov `chrA,chrB,chrA` (no uncovered) → `re.chrchrA.CpG_report.txt`: pos 2 (first segment, covered `5/0`) shows `0 0`; only pos-4 segment (`2/0`) survives. File was truncated and rewritten.

The plan's §3.2 ("Each covered chromosome (as streamed, cov-appearance order) … opens its own writer") and Assumption-2 ("the **last-processed** chr's gets the full summary … Last-processed = last in {covered cov-order …, then uncovered sorted}") are **wrong for the re-appearance case**:

1. If the implementation **caches per-chr writers keyed by name** and appends on re-appearance, the re-appearing chr's file is **2× too long** (both segments) → byte-different from Perl. The implementation MUST `File::create` (truncate) a **fresh** writer on **every** transition, including a re-appearing chr — never reuse a cached writer for a name seen before.
2. "Last-processed = last covered in *first-appearance* cov-order, then uncovered sorted" is wrong: with cov `chrA,chrB,chrA` and no uncovered chrs, the **last `handle_filehandles` call** is for **chrA** (the re-appearance), so chrA holds the summary — even though chrB is the last *first-appearance*. I verified: summary landed in **chrA**, not chrB.

The correct, faithful rule: **the per-chr report+summary files are opened fresh (truncating) at each chr-transition (every covered transition + every uncovered chr); the full whole-genome summary is written once at the end to the chr of the _last_ `handle_filehandles` call** = (the last covered *segment's* chr — i.e. the chr that was current at EOF, which equals the last first-appearing chr only when no re-appearance occurs) **unless** uncovered chrs exist, in which case it is the **bytewise-last uncovered** chr.

**Fix:** rewrite §3.2 / Assumption-2 / §5-step-4 to (a) state the truncating reopen on every transition incl. re-appearance, and (b) define "last summary chr" as the chr of the last `handle_filehandles` call, not "last first-appearance". Add a split-mode regression mirroring the Phase B `non_contiguous_chromosome_re_emits` test (V12 below) — without it, byte-identity silently breaks on the one input class Phase B went out of its way to support.

> Realism note: SPEC §4 says real `bismark2bedGraph` cov is sorted by chr-then-pos, so re-appearance "shouldn't" happen on production input. But Phase B deliberately implements + tests it as faithful-Perl behavior, the Phase-E colossal gate uses arbitrary Perl-cov input, and a wrong model here is a latent byte-identity landmine. Match Perl exactly.

### IMPORTANT — `run_report`'s current covered-chr flush is *inline at each transition*; the split branch must interleave writer-open with that same loop, not post-process a chr list

Phase B `run_report` (`src/report.rs:231-296`) flushes covered chromosomes **inline** as the cov stream is consumed (`:243-259`), into the single shared `report_w`. There is no materialized "covered-chromosome list" to iterate afterwards. The plan §5-step-4/5 describes "for each chromosome processed (covered as streamed; uncovered sorted), call `flush_chromosome_to_own_file(name, …)`" — which reads as if a list is iterated post-hoc. The implementation must instead, **inside the existing streaming loop**, at each transition: close the previous per-chr writer (`finish()`), open a fresh truncating per-chr writer for the new chr, and create/truncate that chr's empty summary file (recording its path as `last_summary_path`). The plan should make explicit that the split branch **reuses the Phase-B transition point** (`report.rs:243`) rather than a separate pass — otherwise an implementer may diverge the flush logic and reintroduce the dual-driver back-port trap the SPEC §7.2 warns about.

### IMPORTANT — summary-file truncation must happen *per chr at open time*, with content written *once at the end* — order matters

Perl truncates `CONTEXTSUMMARY` for **every** chr at `handle_filehandles:117` (so all but the last end up 0-byte), and writes the actual summary **once**, after `generate_genome_wide_cytosine_report` returns, via `print_context_summary` (`:49`, `:63-78`) to whatever `CONTEXTSUMMARY` FH is currently open (= the last chr's). The plan §3.2 captures the net effect ("N empty + 1 full") but should pin the **mechanism + order**: (1) at each chr open, `File::create` its summary path (truncate to empty) and remember the path; (2) after the entire walk (covered + uncovered) completes, write the full `ContextSummary` to the **last-remembered** path. Writing the summary mid-walk, or to a path chosen by sorting rather than by "last opened", will diverge on the re-appearance and the fully-covered cases above.

### Minor — suffix-strip vs `.chr` infix order (no-op, but document)

In `handle_filehandles` the `.chr<NAME>` infix is appended (`:101`) **before** the `.CpG_report.txt$`/`.CX_report.txt$` strip (`:108/111`), so in split mode the strip never fires (the string now ends in the chr name). The Rust must apply `.chr` infix to the **already-cleaned Phase-A stem** and append the report suffix; it must NOT re-run suffix stripping after inserting the infix. Phase A already cleaned the stem, so this is naturally correct — just state it so an implementer doesn't "helpfully" re-strip.

---

## Assumptions review

- **Assumption 1 (`.chr` literal infix, `from_utf8_lossy`):** sound; real names are ASCII; the double-`chr` (`chrchr1`) is real (verified). OK. Edge: a chr name containing a path separator (`/`) would make `File::create` target a subdir — not possible on a Bowtie2 genome; not worth guarding, but a one-line note is cheap.
- **Assumption 2 (last-processed chr summary):** **wrong as stated** — see Critical. Must be restated as "chr of the last `handle_filehandles` call".
- **Assumption 3 (zero-emitting chr still gets a file; empty-gzip stream):** **confirmed correct** (20-byte empty-gzip; 0-byte plain). Promote §10 Q1 from "Open/non-blocking" to "Resolved: must emit a finished empty-gzip stream".
- **Assumption 4 (gz default compression; only decompressed bytes contractual):** correct and necessary (mtime in header).
- **Assumption 5 (`--gzip`/`--split` orthogonal, independent of `--CX`/`--zero_based`/`--threshold`):** correct — these live entirely in the Phase-B kernel which Phase C does not touch. Note one interaction: with `--threshold > 0` there is **no uncovered pass** (Perl `:714`), so in `--split --coverage_threshold N` the summary lands in the **last covered** chr and uncovered chrs get **no files at all**. The plan does not mention `--split + --threshold`; add a sentence + a test (V13) so an implementer doesn't assume uncovered files always exist.

---

## Efficiency

Fine and not the gate (SPEC §10.7). One FH open/close per chr-transition (bounded by contig count; on a re-appearing chr it's one extra open — matches Perl). gz is streaming over the per-chr byte buffer. No concern. The per-chr `Vec<u8>` buffer + `GzEncoder` is identical cost to Phase B's per-chr buffer; negligible.

---

## Validation sufficiency

The V-table is strong on the gzip half and on the empty-file set. Gaps, all tied to the Critical/Important findings:

- **MISSING V12 — split-mode re-appearance (the byte-identity landmine).** Add: cov `chrA,chrB,chrA` (non-contiguous) in `--split` → assert `…chrchrA.CpG_report.txt` contains **only the last segment** (truncated, NOT both blocks), and the summary lands in **chrA** (the last `handle_filehandles` call), not chrB. This is the split-mode mirror of the existing Phase-B `non_contiguous_chromosome_re_emits` test; its absence is the single biggest validation hole.
- **MISSING V13 — `--split --coverage_threshold N`.** Assert **no** uncovered-chr files are produced (Perl skips the uncovered pass when threshold>0) and the summary lands in the last *covered* chr. Confirms Assumption-5's interaction.
- **MISSING — `--split` fully-covered (no uncovered) summary placement.** V8 uses a fixture with uncovered chrs; add a fully-covered case asserting the summary lands in the **last covered (cov-order)** chr (I verified `chrA` for cov `chrB,chrA`). This is exactly §10 Q2 — fold it in as a test, not just a resolved question.
- **V6 file-SET equality — make it bidirectional.** "exact set … for every genome chr" should assert **no spurious files** (e.g. a stray whole-genome `{stem}.CpG_report.txt` from accidentally also running the non-split path) AND **no missing files**. The plan says "exact set" — keep it literally set-equal (Rust output dir listing == Perl output dir listing, both reports and summaries), since a missing or extra file is the most likely split-mode regression.
- **V8 — assert summary CONTENT, not just non-empty.** The plan checks "non-empty (full 64 rows)". Also `diff` the last-chr summary against the **non-split** `default.summary.golden` (the §4b claim — I verified they're byte-identical, 1310 B). A subtle accumulation bug (e.g. resetting the summary per chr) would still produce a "full 64-row" file with wrong counts; the content diff catches it.
- **V4 "assert NOT gzipped":** good — keep the `file`/magic-byte check (or assert the bytes are valid UTF-8 and start with `upstream\t`), since a regression that wraps the summary in gzip would still "decompress to the right thing" and pass a naive content check.
- **V11 regression:** correct and load-bearing — the whole Phase C value rests on Phase B bytes being untouched. Keep all four (default/cx/zero/thr) goldens. (I re-ran them: green.)

Recommend the goldens be generated by **extending `generate_goldens.sh`** with `split`, `gz`, `split+gzip` modes (mirroring the existing `for mode in …` loop), and for split, capturing the **full output-dir file listing** as a golden (sorted `ls`) so V6's set-equality is a checked-in artifact, not ad-hoc.

---

## Alternatives considered

- **`enum ReportWriter { Plain, Gz }` + explicit `finish()` (plan's choice):** sound. The explicit `finish()` (not Drop) is the right call — `GzEncoder` flushes its trailer on `finish()`; relying on `Drop` swallows the error and can't return it. The nesting `GzEncoder<BufWriter<File>>` (plan §4) is correct: buffer the **compressed** output going to disk. (The SPEC §10.5 text "`BufWriter<GzEncoder<File>>`" — buffer feeding the encoder — is the less-good ordering; the PLAN's `GzEncoder<BufWriter<File>>` is better and is what should ship. Flag the SPEC/PLAN wording mismatch so it isn't "corrected" the wrong way during review.) For byte-correctness, ordering doesn't change the *decompressed* bytes (both are lossless); it only affects syscall batching. Either passes V3/V9. Keep the PLAN's `GzEncoder<BufWriter<File>>`.
- **Reusing Phase B's `Box<dyn Write>` seam vs the new `enum`:** the enum is justified — `finish()` consuming `self` cannot be expressed through `&mut dyn Write`, and gz needs an explicit consuming finish. But note `flush_chromosome` currently takes `w: &mut dyn Write` (`report.rs:188`); the split path needs `emit_position` to write into a `Vec<u8>` then hand the buffer to the `ReportWriter` (plan §5-step-4 already says this) — keep `emit_position`/`flush_chromosome`'s buffer-building intact and only swap the final sink. Good.
- **Caching writers per chr (a `HashMap<chr, Writer>`):** explicitly REJECT (would break the truncate-on-reappearance semantics — see Critical). One writer at a time, reopened per transition, matches Perl.

---

## Action items

### Critical
1. **Fix the per-chr file lifetime + summary-chr rule for re-appearing chromosomes.** Rewrite §3.2, Assumption-2, and §5-step-4 to state: each chr-transition (every covered transition per Perl `:457-465`, every uncovered chr per `:1396`) **opens a fresh truncating per-chr report writer (`File::create`) AND truncates that chr's summary file** — never reuse/append a writer for a chr seen before. The full summary is written once at end to the chr of the **last `handle_filehandles` call**, NOT "last first-appearance cov-order". (Perl `handle_filehandles:117,146`, `:457-466`, `process_unprocessed_chromosomes:1393-1396,1562-1564`; Phase B `tests/golden_phase_b.rs:110`.)
2. **Add split-mode re-appearance regression (V12).** cov `chrA,chrB,chrA` in `--split`: assert the re-appearing chr's report holds only the last segment (truncated) and the summary lands in that re-appearing chr — the split mirror of Phase B's `non_contiguous_chromosome_re_emits`. (Cite Perl `:457-466`.)

### Important
3. **Promote §10 Q1 to Resolved:** zero-emitting chr in `--split --gzip` → a **finished empty-gzip stream** (verified 20 bytes, `1f8b…`, decompresses to 0 bytes). The Rust must `GzEncoder::finish()` with no prior writes; a 0-byte file would NOT match Perl. (Perl `:139-150` pipes through `gzip -c`.) Pin with a test asserting the file is non-zero, valid gzip, decompresses empty.
4. **Promote §10 Q2 to Resolved + test:** fully-covered (no uncovered) `--split` → summary lands in **last covered (cov-appearance) chr** (verified `chrA` for cov `chrB,chrA`). Add as a fixture/test, not just a resolved note.
5. **Make V6 file-SET equality bidirectional + checked-in:** assert the Rust output-dir listing == Perl's exactly (no spurious whole-genome report, no missing per-chr file), and capture the sorted `ls` as a golden via `generate_goldens.sh`.
6. **V8 assert summary CONTENT == non-split golden** (not just "non-empty 64 rows") — the §4b claim, verified byte-identical (1310 B). Catches a per-chr summary-reset accumulation bug.
7. **Add `--split --coverage_threshold N` (V13):** no uncovered files; summary in last covered chr. The plan omits this interaction entirely.

### Optional
8. **Note the suffix-strip-vs-`.chr`-infix order** (Perl `:101` infix precedes `:108/111` strip → strip is a no-op in split): apply `.chr` to the cleaned stem, do not re-strip. (Document so it isn't re-added.)
9. **Flag the SPEC mismatches** the PLAN silently corrects, so reviewers don't "fix" them backwards: SPEC §5 table shows `…CpG_report.txt.chr<NAME>` (suffix-after-chr) — the PLAN's `.chr`-before-suffix is the correct Perl behavior; SPEC §10.5 shows `BufWriter<GzEncoder<File>>` — the PLAN's `GzEncoder<BufWriter<File>>` is the better/correct nesting. Add a one-line "deviation from SPEC wording, intentional" note in §8.
10. **One-liner** that the gz header mtime is why decompress-then-compare is mandatory (not merely convenient) — strengthens the §3.1 rationale.

---

## Bottom line

The gzip path and the empty-file/summary-quirk model are correct and now fully empirically grounded — no surprises there. The one thing that will silently break byte-identity is the **split-mode treatment of a re-appearing chromosome**: the plan's "open one writer per chromosome in first-appearance order, summary to the last one" is an oversimplification of Perl's "truncate-reopen on every transition, summary to the last `handle_filehandles` call". Adopt the precise rule + add the V12 regression and this phase is ready. Phase B baseline re-verified green.
