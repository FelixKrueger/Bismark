# Code Review A: 5-Base Bowtie 2 `--maxins 800` default

Reviewer A, fresh context. Target: `git diff d1764d0..HEAD` on `feat/5base-bowtie2-defaults` (HEAD `7295720`), final state only. No files were edited.

## Summary

The builder change is small and correct. `build_five_base_aligner_options` resolves `maxins = cli.maxins.or(800 if Bowtie 2)`, rejects `-I > maxins` on Bowtie 2 only, and emits `--maxins` once. `--dovetail` is never pushed on the 5-Base path, so an explicit `--dovetail` does nothing. HISAT2 and minimap2 option strings are unchanged, and the bisulfite builder (`common_aligner_options` lines 70-87) is untouched. Tests cover the default, explicit overrides, the `-I` boundary (800 accepted, 801 rejected), the dovetail flags, `--local`, HISAT2 pass-through and minimap2. They also cover argv reaching the engine and a real-bowtie2 TLEN/dovetail gate.

The harness implements PREREGISTRATION rev 2 faithfully. A1–A7, the sampling (one `Random(1146)` draw per B-new pair in FASTQ order, whatever the set), the 10% / 1% / 5% admitted/control/pool fractions, `--all-bases`, cycle matching, the n ≥ 10,000 INSUFFICIENT gates, the repro and repro-compare lists, the B-old refusal and the B-new `@PG` checks all match the text. I found no Critical or High issues. There are two Medium harness robustness gaps that could, in principle, give a stale or wrong-scope PASS on a re-run. Neither affected the committed results.

Verification run locally:
- `cargo test -p bismark --lib aligner::`: 614 passed.
- `cargo test -p bismark --test aligner_five_base_bowtie2`: 3 passed (bowtie2 on PATH).
- `cargo test -p bismark --test aligner_cli five_base`: 15 passed.
- `cargo fmt -p bismark -- --check`: clean. `cargo clippy -p bismark --all-targets -- -D warnings`: clean.
- `python3 -I validation/five_base_rescued.py --selftest`: OK.
- `bash validation/five_base_engine_bench.sh selftest` (homebrew bash): 52 passed, 0 failed.

## Issues by area

### Logic

1. **(Medium) `own_cov10` cache survives a cell re-run.** `validation/five_base_engine_bench.sh:374-386` caches the own-set score at `$ROOT/runs/$s/$tag/repro_score_$arm.tsv`. That path is outside the cell directory `$d`, which `cmd_cell` wipes with `rm -rf "$d"` (line 214). If a cell is re-run, for example after a scratch wipe of the cell only or a rebuilt binary, `repro` and `repro-compare` reuse the old cell's n/r. The own-set check could then pass on stale data. This is a silent-wrong-result risk for the gate's own-set item. Fix: write the cache inside `$d`, or have `cmd_cell` remove `repro_score_$arm.tsv` (and the RESULTS copy) when it starts.
2. **(Medium) `verdict` does not check the scope of `score.tsv`.** `cmd_verdict` reads `$RESULTS/$s/$tag/score.tsv`, which any later `score` call for that sample/tag overwrites. `secondary_criteria` only checks that both arms have a `common/10` row. If `score HG002 screen B B B-new B-old` (or any extra arm) were run, the common set would be the intersection over every arm, not B ∩ B-new as rev 2 pre-registers ("`score HG002 screen B B B-new`"), and Δr would still be evaluated. The committed `score.tsv` has only B and B-new, so the result stands. Fix: in `secondary_criteria`, require that the set of arms in `score.tsv` is exactly `{old_arm, new_arm}`, and fail otherwise.
3. **(Low) "Admitted" does not check that the pair is proper.** Rev 2 defines admitted as "pairs proper in B-new and absent from B". `cmd_rescued` labels any B-new pair absent from B as admitted and never tests `0x2`. The 5-Base path appears to emit only concordant pairs, since the `-X 500` / 600 bp case in `five_base_pe_bowtie2_default_max_insert_is_800` yields 0 records, so this is an implicit assumption, not a bug. Fix: in `sam_pairs` or at labelling, fail loudly on a pair without `0x2`, which makes the assumption explicit.
4. **(Low) "Changed" compares only POS and CIGAR.** `same_alignment` ignores RNAME and strand. This matches the rev 2 wording exactly, so it is not a deviation. A pair that moved to another contig at the same POS and CIGAR would count as "rest". The chance is negligible; noted only for completeness.
5. **(Low) A5's "unmatched" count is in bases, not cycles.** Rev 2 says cycles with no control denominator "are dropped from both sides and their count is reported". `matched()` reports the dropped sample bases (`dropped += d`). This has no effect here (0 on HG002), but the verdict label could say "unmatched bases".
6. **(Low) Minus-strand site keys have no selftest.** `cpg_calls` keys OB calls as `(chrom, p, "-")` at the XM call's reference position, which is the plus-strand G. DRAGEN's minus-strand CX rows use the same convention, and the HG002 A6 result (1.67M shared sites, mean diff −0.0004) shows the keys join. Still, every selftest fixture is `+`. Fix: add one OB pair to `_selftest_end_to_end` with a `-` DRAGEN row.

### Efficiency

No concerns. `rescued` does one streaming pass over two `samtools view` pipes. Per-base work is limited to the sampled pairs, CpG calls are packed into `array('q')`, and the DRAGEN file is filtered down to shared keys before it is stored.

### Errors

7. **(Low) The M-bias capture assumes exactly one match.** `mbias=$(ls "$d"/*M-bias.txt)` at bench.sh:245 is quoted as a single argument to `cp`. If the extractor ever wrote two M-bias files, `cp` would fail on a filename containing a newline. The fail-loud direction is fine, but `one`-style handling (count == 1, else die) would give a clearer error.
8. Error paths otherwise fail loudly. These include an unpaired or misordered BAM, a BAM record missing from the FASTQ, missing XM/MD tags, empty metrics columns and empty deconvolution counts (`compare_cells` requires non-empty values), a missing common row, and incomplete report counts.

### Structure / house style

9. **(Low) The comment at `options.rs:282` mixes two facts.** Its dovetail half sits on a line that does nothing with dovetail. "--dovetail stays off" is also phrased relative to a change, and "(its overhangs are not genomic)" is justification, which belongs in the commit message per house style. Suggested wording: `// Bowtie 2 defaults to an 800 bp insert cap.` The `--dovetail` absence is already covered by the tests and docs.
10. **(Low) Unrelated formatting change in a test.** `options.rs` `five_base_omits_bisulfite_policy_and_runtime_threads` drops the trailing comma after `"--reorder"`. This is diff noise unrelated to the change; rustfmt accepts either form inside the macro. Restore it to keep the diff minimal.
11. **(Low) The A6 verdict label says "rest" for the pool.** The label is "A6 matched-site DRAGEN agreement, admitted - rest", but the comparison set is the 5% pool. Suggested label: "admitted - pool".
12. **(Low) The CHANGELOG entry has no issue/PR link.** The surrounding entries in `### bismark (aligner)` carry `([#NNN](…))`. The new entry is one sentence, as required, but has no reference. Add the PR number once it exists. The same applies to the `rust/README.md` milestone line, which cites only the plans dir.
13. Other source comments (`aligner_five_base_bowtie2.rs` OT/OB geometry line, the HISAT2 `-I` line, the harness `# B-new must reach its options through the defaults alone.`) are one line and state facts. No historical narrative found in committed source.

### Docs accuracy (checked against code and results)

- `illumina-5-base.md`: "`--no_dovetail` only flips the TLEN sign of OB pairs whose mates start at the same position". This is correct per `output.rs:572-597`. Without Bowtie 2 `--dovetail`, the only reachable dovetail-gated branch is `flag_1 == 83 && start1 == start2`.
- The table row "Bowtie 2 (default, 800 bp)" (94.5 / 0.9643 / 822,432) equals #1146 B-X, which `repro` reproduced exactly. HG002's "+30% CpGs at cov ≥ 10" is 704,973 / 542,072 = 1.300, which is correct.
- **(Low) The engine comparison now names two libraries, but the table covers one.** "The comparison covers two 5-Base WGS gDNA libraries" follows a table that is NA12878 only, and #1146's HG002 screen holds only arm A. Only the Bowtie 2 `-X 500` vs 800 comparison has HG002 data. Suggested wording: "The engine comparison covers one 5-Base WGS gDNA library (NA12878); the Bowtie 2 insert-cap result was repeated on a second (HG002)."
- `alignment.md`: the `-X` default note and the `--dovetail` note are accurate, and the link style matches the 20 other `/Bismark/...` links.

## Fixes I would apply directly (pending approval)

1. `options.rs:282`: change the comment to `// Bowtie 2 defaults to an 800 bp insert cap.`
2. `options.rs` test `five_base_omits_bisulfite_policy_and_runtime_threads`: restore the trailing comma after `"--reorder"`.
3. `five_base_rescued.py` `admitted_criteria`: change the A6 label from `admitted - rest` to `admitted - pool`. This is a label only; the committed verdict TSVs would keep the old label unless they are regenerated.
4. `illumina-5-base.md`: reword the "two libraries" sentence as above.

## Recommendations

| Priority | Item |
|---|---|
| Critical | None |
| High | None |
| Medium | #1: invalidate or relocate the `own_cov10` cache on cell re-run (bench.sh:374-386) |
| Medium | #2: `secondary_criteria` should require `score.tsv` to contain exactly the two arms |
| Low | #3: assert `0x2` on B-new pairs (makes "proper" explicit) |
| Low | #6: OB/minus-strand selftest case for A6 site keys |
| Low | #5, #7, #11: label/count wording, single-M-bias-file check |
| Low | #9, #10, #12: comment trim, stray comma, CHANGELOG/README PR link |
| Low | Docs "two libraries" wording |

The Medium items harden the harness for future re-runs. They do not cast doubt on the committed rev 2 results: `score.tsv` holds exactly B and B-new, and every cell ran once at `d27b11e`.
