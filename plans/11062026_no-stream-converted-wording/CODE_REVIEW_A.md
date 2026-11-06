# Code review A: `--no_stream_converted` wording (2026-11-06)

Target: uncommitted diff in `/Users/fkrueger/Github/Bismark-docs-nsc` (branch `docs/no-stream-converted-wording`, base dev `f73c34c`):
- `docs/src/content/docs/options/alignment.md:190,194`
- `rust/bismark/src/aligner/cli.rs:372-378` (`--help` text)

## Summary

The claim holds on every successful alignment path. Every place that writes `_C_to_T` / `_G_to_A` files (or the combined-index equivalents) hands them back to the per-input driver, which deletes them after that input's pass. There is no option that skips the deletion. Two caveats: a run that fails or is killed leaves the files behind, because every delete comes after a `?`; and the Perl line range cited in NOTES.md points at the wrong cleanup. The wording is accurate, short and describes current behaviour only. One nearby phrase in the same docs entry ("keeps files") still suggests the files persist.

**Verdict: APPROVE, with one Medium and three Low suggestions.**

## Verification of the claim, path by path

Deletion sites (all `let _ = std::fs::remove_file(&cr.path)`, best-effort, silent):

| Path | Where the files are made | Where they are deleted |
|---|---|---|
| SE faithful (dir / non-dir / pbat; Bowtie 2, HISAT2, minimap2, rammap subprocess, rammap in-process over files) | `process_se_chunk` → `convert_se_files` (`mod.rs:1311`) | `mod.rs:1882`, after `sinks.finish()` |
| SE combined directional / pbat / non-dir 2-pass / non-dir sequential | `mod.rs:3978`, `4277`, `4702` | `mod.rs:3852`, `3952`, `4251`, `4675`; the sequential spill SAM is deleted separately at `mod.rs:4780` |
| SE combined single-pass (tagged) | `convert_se_tagged_interleaved` (`mod.rs:4937`) | `mod.rs:4909` |
| PE faithful (all libraries/aligners) | `process_pe_chunk` (`mod.rs:5256`) | `mod.rs:5557` |
| PE combined directional / pbat / non-dir / sequential / tagged | `mod.rs:6242-6250`, `6583-6586`, `7143-7146`, `7366` | `mod.rs:6103`, `6210`, `6552`, `7108` (+ spill `7236`), `7338` |
| `--multicore` / `--parallel` > 1 (SE, PE) | per chunk, via `process_se_chunk` / `process_pe_chunk` | `parallel.rs:442`, `511`, per chunk, together with the chunk subset files (`444`, `513-514`) |
| Streamed / in-process rammap | no files written (FIFOs, removed by `FifoSet::drop`, `stream.rs:91`) | n/a |
| `--illumina_5base` | `resolve_stream_converted` returns files-off with no reason (`config.rs:441`), but 5-Base converts nothing | n/a |

Timing: the files are deleted after the whole per-input pass (alignment, methylation calls, report, sinks finished), not as soon as the aligner exits. In a multi-file run, each file's set is gone before the next file starts. Under `--multicore` the "input" is the chunk subset, so deletion happens per chunk. "Once its input has been aligned" describes this well enough.

Perl: the converted-file deletion is in `print_final_analysis_report_single_end` (`legacy_perl/bismark:1975-1993`) and `..._paired_ends` (`:2155-2174`). Both run at the end of each input file, which matches the Rust behaviour. "As in Perl Bismark" is accurate.

## Issues

### Medium

**M1. "keeps files" in the same entry still implies the files persist** (`docs/src/content/docs/options/alignment.md:196`): "`--rammap_subprocess` runs the external binary and keeps files." Next to the new "temporary" sentence, a reader can take "keeps" to mean the files are retained. Suggested change: "…runs the external binary and uses files." (or "…and writes files"). The same verb appears in source comments at `config.rs:419` ("so it keeps files") and `config.rs:422` ("keeps files"). Those are not user-facing; fixing them is optional.

### Low

**L1. Failed or killed runs leave the files behind.** Every deletion site comes after a `?` (for example `mod.rs:1871-1881`, `parallel.rs:430-441`). An error in alignment, merging or output therefore returns before the cleanup, and no top-level handler sweeps `--temp_dir`. The only cleanup that runs regardless of success, at `mod.rs:318-320`, covers just the uBAM/BINSEQ transcode dirs. "Bismark has no option to keep them afterwards" is still true, but a user may find stray `_C_to_T` files after a crash. Line 200 already says this about FIFOs ("A run that is killed can leave them behind"). An optional parallel clause at line 190 would be: "…each one is deleted once its input file has been aligned (a run that fails or is killed can leave them behind)." This is a judgement call on brevity, not a correctness problem.

**L2. NOTES.md cites the wrong Perl lines.** `plans/.../NOTES.md` cites "Perl `bismark:795-800`". That block deletes the `--multicore` input chunks (`@temp_input` = `<file>.temp.<i>`, built at `:606-633`), not the converted `_C_to_T` / `_G_to_A` files. The correct references are `bismark:1975-1993` (SE) and `:2155-2174` (PE). This is a plan artifact only, but fix it if NOTES.md is quoted in the PR or commit message.

**L3. Naming nuance for combined single-pass.** `--combined_index_single_pass` writes `<file>_CT_GA_tagged.fastq` (`convert.rs:614`, `791`), not `_C_to_T` / `_G_to_A`. It is also temporary and deleted (`mod.rs:4909`, `7338`), so the claim holds; only the parenthetical filename example is incomplete. No change is needed.

### Not introduced by this change (FYI only)

- The fallback list in `cli.rs:376-378` names `--rammap_subprocess` but not rammap with FastA input or a build without the in-process backend. The wiki table (`wiki/usage/align-reads.md:105`) lists both. This predates the change and is outside its scope.
- `alignment.md:192`, "nothing is lost by not keeping them", is still fine: it compares streaming with writing the files, not with keeping them.

## Style

- `cli.rs`: "The files are temporary: each is deleted once its input has been aligned." This is one sentence, describes current state, and has no history or justification, so it fits the comment rules. The reflow of the following lines is unavoidable.
- `alignment.md:190`: "As in Perl Bismark," compares with Perl rather than narrating history. That is acceptable in user docs, where Perl users are the audience. It could be dropped for brevity without losing anything.
- `alignment.md:194`: removing the "inspect the converted reads" use case and adding "while a run is in progress; Bismark has no option to keep them afterwards" is accurate and short.

## Other user-facing text checked

- `CHANGELOG.md:19`: "`--no_stream_converted` restores the files". Accurate: it restores the temp files as they were in Perl/pre-#1120, which were also deleted. Leave it.
- `docs/.../rust/choosing-an-alignment-mode.md:38`: "get files back everywhere". Accurate in the same sense. Leave it.
- `wiki/usage/align-reads.md:98-110` and `wiki/reference/aligner-cli.md:87`: these only say "written to `--temp_dir` as files" and make no claim that the files persist or can be inspected. Leave them.
- `docs/.../options/alignment.md:186` (`--gzip`): "Temporary bisulfite conversion files". Consistent with the change.
- No remaining "inspect the converted reads" text anywhere in docs, wiki, README or source.

## Fixes applied

None (read-only review).

## Recommendations

| Priority | Item |
|---|---|
| Critical | none |
| High | none |
| Medium | M1: change "keeps files" to "uses files" in `alignment.md:196` |
| Low | L1: optionally note that a failed or killed run can leave the files, matching the FIFO sentence at line 200 |
| Low | L2: correct the Perl line reference in NOTES.md (`:1975-1993` / `:2155-2174`, not `:795-800`) |
| Low | L3: none needed; the tagged-file name differs but the deletion holds |
