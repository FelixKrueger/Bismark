## Summary

Port the Perl `methylation_consistency` (~556 LOC, v0.25.1) to a Rust binary in the cargo workspace. Reads a Bismark alignment BAM and splits its reads into **three** BAMs by **read-level** methylation consistency — consistently methylated (`>= --upper_threshold`, default 90%), consistently unmethylated (`<= --lower_threshold`, default 10%), and mixed — plus a `_consistency_report.txt`. Only the `XM:Z:` tag is consulted (counts `Z`/`z`, or `H`/`h` with `--chh`).

The **simplest** of the post-alignment ports: the algorithm is "count two byte classes in the XM string, classify by a rounded percentage, route the record(s) to one of three BAMs." All difficulty is in byte-identity to Perl's formatting + edge cases, not the algorithm. Closest sibling: `epic(dedup)` (#792) — same read→classify→write-BAM(s)+report shape.

## Change

New crate `bismark-methylation-consistency`, binary `methylation_consistency_rs`. CLI matches Perl:

- `-s`/`--single_end`, `-p`/`--paired_end` — default: auto-detect from the Bismark `@PG` line. **No Bismark `@PG` ⇒ SE** (Perl falls through; *not* an error).
- `--chh` — experimental CHH context: count `H`/`h`, `_CHH` filename infix, "Too few CHHs" report label.
- `--lower_threshold` (0–49, default 10), `--upper_threshold` (51–100, default 90).
- `-m`/`--min-count` (≥0 integer, default 5).
- `--samtools_path` — accepted for compatibility; unused (noodles does all I/O).
- `--quiet` — new; suppress STDERR diagnostics (extractor precedent).
- Output filenames match Perl exactly: `{root}{_CHH?}_all_meth.bam`, `_all_unmeth.bam`, `_mixed_meth.bam`, `_consistency_report.txt` (strip a single trailing `.bam` from the input to form `{root}`).

## Implementation notes

- Reuse `bismark-io` (`open_reader` / `BamWriter` / `tags::xm` / `detect_paired_from_header`) and mirror `bismark-dedup`'s crate layout (`cli`/`pipeline`/`report`/`filename`/`error`) + its byte-identity test methodology (compare records read back via `open_reader` + the report `.txt` verbatim — **not** raw BGZF bytes, since the Perl pipes through `samtools view -b` whose compression differs).
- Single-threaded streaming for v1.0 (no `mimalloc` / threaded BGZF — perf follow-up).
- SPEC + phased plan: `plans/05292026_bismark-methylation-consistency/{SPEC,PLAN}.md` (branch `rust/methylation-consistency`).

## Design pitfalls to avoid

- **Round-then-compare**: Perl rounds the percentage to `%.1f` **before** the threshold comparison — a 10.04% read → unmethylated, 10.05% → mixed. Format → parse → compare; never compare the raw fraction.
- **Auto-detect → SE fallback**: a missing Bismark `@PG` line means SE (Perl leaves `$paired` undefined and falls through), **not** an error — opposite of dedup's policy.
- **SE is not sort-checked; PE is** (`@HD SO:coordinate` guard). `bismark-io`'s `open_reader` always rejects coordinate-sorted input, so the SE path needs a no-sort-check entry point (small `bismark-io` addition).
- **Report bytes**: 49-hyphen separator + exact label spacing copied verbatim from the Perl source; `%.2f` percentages; literal `N/A` when the grand total is 0.
- **PE counts pairs, not records** (one increment per pair though 2 BAM records are written); `--min-count 0` zero-call reads are skipped (counted in no bucket).
- **No `@PG` / version injection** — the input header is copied through verbatim.

## Sub-issues

- [ ] `spec(methcons)`: SPEC + phased plan + Perl flag inventory + byte-identity contract  ← drafted, in review
- [ ] `impl(methcons)`: crate scaffold + CLI + SE classification / route / report (Phase A)
- [ ] `impl(methcons)`: paired-end pairing + per-pair counting + PE coordinate-sort guard (Phase B)
- [ ] `impl(methcons)`: `--chh` context + edge cases (empty / truncated / multi-file / min-count=0 / N/A report) (Phase C)
- [ ] `test(methcons)`: byte-identity diff vs Perl on 10M SE / PE + `--chh` (report.txt + 3 BAMs at record level)
- [ ] `test(methcons)`: flag-combination matrix + classification boundary unit tests
- [ ] `spike(methcons)`: `%.1f`/`%.2f` Perl-vs-Rust formatting parity; empty-bucket BAM behavior
- [ ] `docs(methcons)`: README + rustdoc + `--help` text + CHANGELOG entry

## References

- Perl source: [`methylation_consistency`](https://github.com/FelixKrueger/Bismark/blob/master/methylation_consistency) (556 LOC, v0.25.1)
- SPEC + plan: `plans/05292026_bismark-methylation-consistency/{SPEC,PLAN}.md` (branch `rust/methylation-consistency`, worktree `~/Github/Bismark-methcons`)
- Blocked by: `epic(infra: noodles-io)` (#794, Done) — uses shared `bismark-io` BAM I/O
- Sibling: `epic(dedup)` (#792) — same tool shape (read → classify → write BAM(s) + report)
