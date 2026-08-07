> **POSTED 2026-08-07** as
> [`#787` comment `5216492876`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5216492876),
> verbatim as below. Reply to @Danielsm8's
> [result](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5210728023) of 2026-08-07.
> Evidence: `EXPERIMENT_patter_swap.md`.
>
> Awaiting two answers from him: whether his pre/post-patch `.pat` files are byte-identical (which
> would confirm a silent build failure), and what DRAGEN actually puts in `SEQ`. Neither blocks
> #1095 — the converter reads `XM`.

Hi Mike,

Thanks for trying it, and don't worry — the result is useful even though it looks like a dead end.

I've now tested the same patch here in a controlled setting, and **it does work**. On a synthetic
5-Base dataset where every CpG is methylated, unpatched `patter` calls 0% methylation and the
patched build calls 100%, matching Bismark's own calls exactly. Same on each strand separately.
So the idea is sound and something else is going on at your end.

Two candidates, and you can tell them apart quickly.

**1. The build may not have taken.** This is my first suspicion, because `wgbs_tools`' `setup.py`
does *not* stop on a compilation error — the line that would raise is commented out. It prints a
red `FAIL` for the broken module and carries on through the other ~15, then exits successfully.
So a small syntax slip in `patter.h` gives you a wall of output with one `FAIL` buried in it, a
zero exit code, and **the previous binary still in place** — which would look exactly like "no
difference", including no difference across references.

The quickest check needs no rebuild: **are your before- and after-patch `.pat` files
byte-identical?** If they are, the patch never made it into the binary. If you'd rather rebuild,
`python setup.py -t patter -v` compiles just that one target so nothing can hide the error.

For reference, the change I tested was exactly:

```cpp
// src/pipeline_wgbs/patter.h
ReadOrient OT{'T', 'C', 0, 0};   // was {'C','T',0,0}
ReadOrient OB{'A', 'G', 1, 1};   // was {'G','A',1,1}
```

**2. DRAGEN's BAM may not store the raw read.** This is the more interesting possibility and it is
my mistake for not flagging it earlier. The whole approach assumes `SEQ` contains the read as
sequenced, which is true of Bismark's output. If DRAGEN instead writes a reference-normalised
`SEQ`, or carries the methylation in a tag rather than in the bases, then `patter` is reading
something that isn't the 5-Base signal at all — and no polarity change can fix that. It would
also explain why swapping references made no difference, and possibly your 50% marker capture.

One way to check, on any DRAGEN read: pull a record and compare its bases at a few CpGs against
the reference. If the Cs look untouched — no `T`s where you'd expect converted positions — then
`SEQ` has been normalised and that is the answer.

**On your question: yes, that is exactly the plan.** Re-run through Bismark to produce the BAMs,
then put those through the converter. It sidesteps this entire question, because the converter
reads the `XM` tag Bismark writes rather than trying to interpret anyone's `SEQ`. The converter is
planned, reviewed and ready to build (tracked in #1095) — I'd rather not have you spend more time
on the `patter` patch in the meantime.

Good news on the atlas side: because the converter leaves `XM` untouched, `bam2pat --ds_test`
keeps working on its output too.

One thing for when you re-run: the genome you align against needs the same chromosome naming as
your `wgbstools init_genome hg38` — so `chr1`, not `1`. A complete mismatch fails immediately and
loudly, but a *partial* one silently drops the contigs that don't match and the run appears to
succeed with missing data, which may be worth ruling out for your 50% observation independently
of everything above.

Best,
Felix
