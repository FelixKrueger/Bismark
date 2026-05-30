# Spike results — methylation_consistency port

## Spike 1 — `%.1f` / `%.2f` formatting parity (Perl `sprintf` vs Rust `{:.N}`)

**Status: VALIDATED** (by Reviewer B's compiled Rust probe, 2026-05-29; to be formalized as committed unit tests).

- **Finding:** Rust `format!("{:.1}", x)` / `{:.2}` and Perl `sprintf("%.1f"/"%.2f", x)` are **decision-identical** for the classification and report. Both round-half-to-even on the *same* IEEE-754 `f64`.
- **The SPEC's earlier rationale was wrong and is corrected:** "exact-halfway ties are essentially unreachable" is FALSE — `meth/total*100` hits exact representable ties for power-of-two totals, e.g. `1/16 → 6.25`, `1/8 → 12.5`, `1801/2000 → 90.05`. Rust and Perl agree on *all* of these — **but only because the f64 is computed in identical operation order.**
- **Action (folded into PLAN A3):** pin the expression as `meth as f64 / total as f64 * 100.0` (this exact op-order), and add tie-boundary unit tests incl. `90.05 → "90.0" → all_meth` (≥90 boundary via rounding), `10.05 → "10.1" → mixed`, `10.04 → "10.0" → all_unmeth`, plus `6.25`, `12.5`, `87.5`. The worked example in SPEC §2.5 (`10.05`) is fine as a *decision* illustration but is not a clean representable value — keep it as illustrative only.

## Spike 2 — empty-bucket BAM behavior

**Status: DONE** (ran Perl `methylation_consistency` locally, 2026-05-29). Fully reproducible from the experiment below; the throwaway BAM/SAM scratch is not committed.

**Experiment:** built a 3-read SE BAM, all reads fully methylated (`XM:Z:ZZZZZZZZZZ`, 10 CpG calls ≥ min-count 5 → 100% → `all_meth`), so `all_unmeth` and `mixed` buckets receive zero records. Ran `perl methylation_consistency -s input.bam` (samtools 1.21, perl 5).

**Findings:**
1. **Empty buckets → 0-byte files that EXIST but are UNREADABLE.** `input_all_unmeth.bam` and `input_mixed_meth.bam` were each **0 bytes**. `samtools view` on them errors: `[main_samview] fail to read the header from "..."`, exit 1. (Perl opens all three `| samtools view -b -S - > file` pipes eagerly via shell `>`, so the files are created; empty stdin makes samtools write nothing → 0 bytes, no header, no BGZF EOF.)
2. **Perl itself errors on the empty buckets but exits 0.** STDERR shows `[main_samview] fail to read the header from "-".` (the empty-bucket samtools subprocess fails) and `Can't close unmeth file: ... line 347` / `Can't close mixed file: ... line 348` (Perl's `close` reporting the subprocess failure as a warning). Perl exit code = 0 regardless. So the 0-byte empty bucket is a samtools-failure **wart**.
3. **DECISION (user, 2026-05-29): emit *valid empty BAMs***, not 0-byte files. The Rust port writes a proper header + BGZF EOF for empty buckets (readable, downstream-usable). This diverges from Perl's 0-byte output **for empty buckets only**; all meaningful output (records + report.txt) stays byte-identical. Implementation: **eager-open all three `BamWriter`s** at start with the (verbatim) input header → populated buckets get header+records, empty buckets get a valid empty BAM. The Phase-D harness compares empty buckets at the **record level** (both = zero records), not raw bytes.

**BONUS finding — samtools provenance `@PG` lines (affects the BAM byte-identity contract):**
- The populated `input_all_meth.bam` header carried **extra `@PG ID:samtools.N` provenance lines** beyond the original `@PG ID:Bismark`: one for Perl's `samtools view -H <input>` (header extraction) and one for `samtools view -b -S -` (BAM write). samtools auto-appends a `@PG` per invocation by default.
- The Rust port (noodles, header written **verbatim** — confirmed identical to `bismark-dedup`'s `reader.header().clone()` → `open_writer` convention) adds **no** such lines.
- **Consequence:** output BAM **headers cannot be byte-identical.** The byte-identity contract for the BAMs therefore compares: **all alignment records** (ordered, all fixed fields + tags-as-set) + **`@HD` + `@SQ`** + the **`@PG ID:Bismark`** line; it **excludes** the samtools `@PG ID:samtools*` provenance lines (which the Rust rewrite intentionally eliminates). This is exactly why `bismark-dedup`'s byte-identity test compares only a qname set — it was sidestepping this same header divergence. (Reviewer A's "compare all header lines" / Reviewer B's "compare parsed header content" would both over-constrain here.)

**BONUS finding — report templates byte-validated.** The real Perl `input_consistency_report.txt` matched SPEC §5.1 exactly: 49-hyphen separator; `Total single-end records` + 5 spaces + `-` + TAB; label spacing 4/2/1/3; `{:.2}`-style percentages (`100.00%`, `0.00%`); **no leading `\n`** and **no trailing blank line** (confirms Reviewer A's "no leading newline" gotcha vs dedup's report).
