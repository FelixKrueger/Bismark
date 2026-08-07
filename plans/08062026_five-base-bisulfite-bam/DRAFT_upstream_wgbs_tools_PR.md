> **Draft — not submitted. Technically ready; submission awaits Felix's go-ahead.**
> Proposed PR to [`nloyfer/wgbs_tools`](https://github.com/nloyfer/wgbs_tools) (base: `master` @ `6f24bed`).
>
> ✅ **All four validation checks pass** (2026-08-07/08):
> - **1 & 4** — measured on the unconditional swap (`EXPERIMENT_patter_swap.md`: 0.0 %→100.0 %
>   against 100 % ground truth, each strand independently), and carried to this flag-gated patch by
>   **observed byte-identity**: `--five_base` output ≡ swapped-constant-binary output on both the
>   synthetic fixture and real chr21 WGBS data (`EXPERIMENT_patter_flag_gated.md`).
> - **2** — default path untouched: with the patch present but not enabled, `.pat` output is
>   **byte-identical** at three levels — `patter` standalone on synthetic PE bisulfite (mixed
>   methylation), `patter` standalone on real WGBS chr21 (59,250 pat lines, 0 mismatches), and the
>   full `wgbstools bam2pat` CLI (patched vs stock `bam2pat.py`, decompressed `.pat.gz`).
> - **3** — effectively satisfied: the reporter confirmed the corrected polarity end-to-end through
>   UXM on his real paired data ([#787 comment `5219193146`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5219193146)).
>
> The CLI plumbing has been read (`main.cpp` `InputParser`; `bam2pat.py` `proc_chr`/`start_threads`
> positional tuple/argparse) and the sketch below is replaced by the **validated diff**.
>
> Note the reporter's field attempt at the same patch produced *no* change, which is very likely
> a silent build failure on his side (`setup.py`'s `raise` is commented out) rather than evidence
> against the patch. Do not cite his result either way in the PR.

---

## Title

`patter: optional inverted-polarity mode (--five_base) for 5-Base / TAPS-style libraries`

## Body (this is the whole visible description)

Adds an optional `--five_base` flag to `bam2pat`/`patter` that swaps the methylated/unmethylated
call characters, so libraries in which 5mC reads as `T` — Illumina 5-Base, and TAPS-style
chemistries generally — produce correct calls instead of systematically inverted ones. Default
behaviour is unchanged: without the flag, `.pat` output is byte-identical to current `master`
(verified on real WGBS data and a synthetic fixture; details folded below).

<details>
<summary>Background, validation and reasoning (AI-assisted — bin this section if it isn't useful)</summary>

**The problem.** `patter` derives each call from the read character at a CpG position, comparing it
against `ReadOrient::ref_chr` (methylated) and `ReadOrient::unmeth_seq_chr` (unmethylated):

```cpp
// patter.h
ReadOrient OT{'C', 'T', 0, 0};
ReadOrient OB{'G', 'A', 1, 1};
```

Bisulfite and EM-seq convert *un*methylated C to T, so a `C` means methylated. Illumina 5-Base is
the chemical inverse — the enzyme converts **5mC → T** and leaves unmethylated C as C — so on
5-Base data every call comes out backwards. Not noisy or degraded: inverted.

**Why swapping the two constants is sufficient.** `ref_chr` and `unmeth_seq_chr` are read in
exactly two places, `patter.cpp:153` and `:159`, both inside the call branch of `compareSeqToRef`.
Swapping them inverts the call polarity completely and affects nothing else. The CpG guard is
polarity-agnostic — both `is_cpg()` branches accept either base, and the context bases they
require (`seq[j+1] == 'G'` for OT, `seq[j-1] == 'C'` for OB) are guanines on the read's own strand
under 5-Base chemistry, hence never converted. (`nr_bad_conv` is declared but never incremented in
`patter.cpp`, so there is no conversion-QC filter to consider.)

**The change.** Two files. In `main.cpp`, the flag overrides the public members after construction
— no `patter.h` or constructor change, so the default path is untouched by inspection:

```cpp
patter p(argv[1], argv[2], mbias_path, min_cpg, clip, is_np, np_thresh, is_long, is_ds_test, cpc_call, combine_mods);
if (input.cmdOptionExists("--five_base")) {
    // Inverted-polarity chemistry (Illumina 5-Base, TAPS): 5mC reads as T,
    // unmethylated C stays C. Swap the meth/unmeth call characters.
    p.OT = ReadOrient{'T', 'C', 0, 0};
    p.OB = ReadOrient{'A', 'G', 1, 1};
}
```

plus `[--five_base]` in the usage string. In `bam2pat.py`: an argparse `store_true` option,
`proc_chr(..., five_base=False)`, `if five_base: patter_cmd += ' --five_base '`, and
`self.args.five_base` appended to the `proc_chr` parameter tuple.

**Validation.**
- *Correctness on 5-Base data:* on a fully CpG-methylated synthetic 5-Base dataset (ground truth
  100 %, aligner's own calls agree), stock `patter` reports 0.0 % methylation; with `--five_base`,
  100.0 % — each strand verified independently (`--top_only`/`--bottom_only` selectors), ruling
  out a one-sided swap. Confirmed end-to-end through UXM on real paired 5-Base/EM-seq data by the
  user who hit the problem ([Bismark #787](https://github.com/FelixKrueger/Bismark/issues/787)).
- *Default path untouched:* with the patch applied but the flag absent, `.pat` output is
  **byte-identical** to stock — for `patter` standalone on real WGBS data (SRR24827378, chr21,
  59,250 pat lines) and on a mixed-methylation synthetic PE fixture, and for the full
  `wgbstools bam2pat` run (decompressed `.pat.gz`).
- *The flag does exactly one thing:* with `--five_base` on bisulfite input, every output line is
  the exact C↔T mirror of the stock line (0 mismatches over 59,250 real-data lines); positions,
  CpG indices and counts unchanged.

**Considered and rejected: the `MM`/`ML` path.** `patter` auto-detects standard base-modification
tags (`patter.cpp:334-338`), which would avoid the polarity question entirely, but
`patter.cpp:341-343` throws `"Unrecognized bam format: paired end and nanopore"`, and Illumina
5-Base is a paired-end library. Lifting that restriction is a much larger change than this flag.

**Scope.** No auto-detection — a wrong guess silently inverts every call, so explicit opt-in
seemed safer. `add_cpg_counts.h:61-62` duplicates the same `ReadOrient` constants; `bam2pat` does
not use that tool and this PR does not touch it — happy to extend the flag there if wanted.

</details>

---

## Validation record (internal — not part of the PR)

| # | Check | Result |
|---|---|---|
| 1 | Patched `patter` on 5-Base BAM vs aligner ground truth; unpatched gives the mirror image | ✅ 0.0 % vs 100.0 % vs 100 % truth (`EXPERIMENT_patter_swap.md`); carries to the flag by byte-identity (`EXPERIMENT_patter_flag_gated.md`) |
| 2 | Unpatched vs patched-but-not-enabled on EM-seq-chemistry BAM | ✅ byte-identical `.pat` at 3 levels: standalone synthetic, standalone real chr21 (0/59,250), full `bam2pat` CLI |
| 3 | Paired 5-Base and EM-seq libraries through UXM converge | ✅ effectively — reporter's real-data confirmation, #787 comment `5219193146` |
| 4 | OT and OB strands separately | ✅ both strands 0 %→100 % independently (`EXPERIMENT_patter_swap.md`) |

## Submission mechanics (when Felix says go)

- Fork `nloyfer/wgbs_tools`, branch e.g. `five-base-polarity`, `git apply five_base_wgbs_tools.patch`
  (the validated diff, saved next to this file; reverse-apply-checked against the tested tree),
  commit with the reasoning in the commit message, PR with the Title/Body above.
- Check `git diff` before pushing — only the two files, no formatter drift (global rule).

## Sequencing note

Bismark's own `--five_base_bisulfite_bam` converter (#1095) is **not** a prerequisite and does not
overlap: this patch fixes 5-Base data at the consumer, needing no Bismark at all, and works on
DRAGEN BAMs. The converter serves Bismark users who would rather not run a patched third-party
binary. Either alone solves the reporter's problem; both is better.
