# Code review B: `--no_stream_converted` wording (2026-11-06)

Target: uncommitted diff in `/Users/fkrueger/Github/Bismark-docs-nsc` (branch `docs/no-stream-converted-wording`, base dev `f73c34c`):
`docs/src/content/docs/options/alignment.md` and `rust/bismark/src/aligner/cli.rs` (`--help` text).

## Summary

The core claim holds on a successful run. Every path that writes converted reads to disk deletes them, and no CLI option keeps them. Deletion runs at the end of each input file (or pair, or `--multicore` chunk), after the alignment, the methylation calls, the report and `sinks.finish()`. That is the same point as Perl (`print_final_analysis_report_*`, which `unlink`s the files once "all sequences ... have been analysed"). "Once its input has been aligned" is loose but fair.

The new text does not cover three things:

1. **Failed runs.** Every cleanup loop sits after a chain of `?`. Any error before it returns early, so a failed or aborted run (and any panic or kill) leaves the converted files in `--temp_dir`. Perl does the same (`die` skips the unlink). It matters to the disk-constrained users this docs entry is aimed at.
2. **Silent deletion.** Rust deletes with `let _ = std::fs::remove_file(..)` and says nothing if it fails. Perl warns on failure (`Could not delete temporary file ...`) and also reports success. "As in Perl" is accurate about *when* the files go, not about the messages.
3. **Neighbouring text.** The unchanged `--rammap_subprocess runs the external binary and keeps files` (alignment.md:196) now reads as if those files survive the run, which contradicts the new sentence.

No other user-facing text (CHANGELOG, wiki, `choosing-an-alignment-mode.md`, READMEs) says the files persist or that they can be inspected afterwards. The NOTES.md decision to leave those alone is correct.

## Verification: converted-file create and delete sites

All sites are in `rust/bismark/src/aligner/`. "Deleted" means deleted on success; every path keeps its files on an error return.

| Path | Created | Deleted |
|---|---|---|
| SE per-strand (Bowtie 2 / HISAT2 / minimap2 / rammap subprocess; rammap in-process with `--no_stream_converted`), dir/non-dir/pbat | `mod.rs:1311` `convert_se_files` (inside `process_se_chunk`) | `mod.rs:1881-1883` in `run_se`, after report + `sinks.finish()` (1 file for dir/pbat, 2 for non-dir) |
| SE combined directional / non-dir 2-pass / pbat / HISAT2 variants | `mod.rs:3978, 4277, 4702` (+ siblings) | `mod.rs:3851, 3951, 4250, 4674` |
| SE combined sequential spill `*.ct_pass.sam.gz` | `mod.rs:4725-4728` | `mod.rs:4780` (in the chunk fn), then the converted files in the caller's loop |
| SE combined single pass: `*_CT_GA_tagged*` (not `_C_to_T`/`_G_to_A`) | `mod.rs:4937` (`convert.rs:614`) | `mod.rs:4908` |
| PE per-strand, dir/non-dir/pbat | `mod.rs:5256` | `mod.rs:5556-5558` (2 or 4 files) |
| PE combined dir / pbat | `mod.rs:6242, 6250` (+ sibling) | `mod.rs:6102, 6209` |
| PE combined non-dir 2-pass / sequential (+ spill) | `mod.rs:6583-6586, 7143-7146`; spill `7176` | `mod.rs:6551, 7107`; spill `7236` |
| PE combined single pass (two `_CT_GA_tagged` files) | `mod.rs:7366` | `mod.rs:7337` |
| `--multicore` SE / PE chunks | the same `process_*_chunk` fns | `parallel.rs:441-444` / `510-514`, per chunk, with the `<file>.temp.<i>` subsets |
| `--illumina_5base` | nothing written (`config.rs:441-447`) | n/a |
| Streamed (FIFO) runs | FIFOs only | `stream.rs:91-96` (`FifoSet` Drop) |

Perl: `legacy_perl/bismark:1975-1993` (SE) and `2155-2174` (PE) delete the converted files. **`bismark:795-800`, cited in NOTES.md, deletes the `--multicore` split *input* chunks (`@temp_input`, built at 606-633), not the `_C_to_T`/`_G_to_A` files.** NOTES.md is not shipped, but the citation is wrong.

## Issues

### Medium

- **M1. A failed run keeps the files** (`mod.rs:1881` and every cleanup site above sit after `?`; also `parallel.rs:441`). Neither text says so. Suggested addition in alignment.md: "...deleted once its input file has been aligned (a failed run leaves them in `--temp_dir`)." The `--help` text can leave it out.
- **M2. alignment.md:196 (unchanged line) contradicts the new claim.** "`--rammap_subprocess` runs the external binary and keeps files" now reads as "keeps them after the run". Change it to "...and uses files" or "...writes files".

### Low

- **L1. "As in Perl Bismark" (alignment.md:190).** This is a cross-implementation comparison, not history, so it does not break the style rule. It is only fully true for timing: Perl warns when a deletion fails and Rust does not (M1/L3). Consider dropping the clause. The sentence reads fine without it, and the cli.rs text already omits it.
- **L2. File names.** Both texts name `_C_to_T` / `_G_to_A`. The `--combined_index_single_pass` fallback writes `*_CT_GA_tagged*` files instead, and sequential also writes a `.ct_pass.sam.gz` spill. All of them are deleted the same way, so the claim holds, but the names are not exhaustive. This predates the diff, so I would leave it.
- **L3. Deletion failures are silent** (`let _ = std::fs::remove_file`, e.g. `mod.rs:1882`). This is a code observation, outside this wording PR. A stale file gives no warning, unlike Perl. Possible follow-up: warn on any error other than `NotFound`, as `mod.rs:958-967` already does for 5-Base partial outputs.
- **L4. cli.rs diff noise.** Rewrapping the last three lines of the doc comment grows the diff from 3 lines to 6. That is harmless: rustfmt does not wrap comments, and the longest line is 89 characters, under 100. Keeping the original wrap would shrink the diff if you want it minimal.
- **L5. alignment.md:194, "look at the converted reads while a run is in progress".** Accurate: each file exists from its conversion until that input finishes. With `--multicore`, each chunk's files live only for that chunk. Fine as written.
- **L6. Out of scope, but on the same line.** The fallback list at alignment.md:198 and in cli.rs omits the rammap FastA case, where the in-process backend falls back to the subprocess and writes files. It prints its own `Note:` (`mod.rs:306-312`). The wiki covers it. Optional.

### Critical / High

None.

## Style

- cli.rs: three short sentences, present tense, no history. It matches the repo's comment style.
- alignment.md: the wording is current-state and short. "Bismark has no option to keep them afterwards" is true: there is no keep or retain flag in `cli.rs`.

## Recommendations

1. (Medium) Add the failed-run caveat to alignment.md:190 (M1).
2. (Medium) Change "keeps files" to "uses files" at alignment.md:196 (M2).
3. (Low) Consider dropping "As in Perl Bismark," (L1).
4. (Low) Fix the Perl citation in NOTES.md: 1975-1993 and 2155-2174, not 795-800.
5. (Follow-up, separate PR) Warn when a converted temp file cannot be deleted (L3).
