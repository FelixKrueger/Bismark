> **POSTED 2026-08-06** as
> [`#787` comment `5203512462`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5203512462),
> verbatim as below. Reply to @Danielsm8's
> [answer](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5167178704) of 2026-08-03.
>
> Awaiting his result from the `patter` two-constant patch — that is real-data evidence for or
> against the inversion hypothesis, and it gates the upstream PR (see
> `DRAFT_upstream_wgbs_tools_PR.md` §Validation, check 1).

Hi Mike,

That's exactly what I needed, thank you — and your answer to (1) makes this much easier.

Since you're happy to re-run through Bismark, the converter is a small, self-contained job: it reads the `XM` tag Bismark already writes and re-encodes `SEQ` into bisulfite convention, so nothing has to be re-derived from the genome. It's planned and tracked in #1095.

But there's something you can try **today**, on the DRAGEN BAMs you already have, without waiting for us.

`patter` decides methylated-vs-unmethylated from two constants per strand, in `src/pipeline_wgbs/patter.h`:

```cpp
ReadOrient OT{'C', 'T', 0, 0};
ReadOrient OB{'G', 'A', 1, 1};
```

The first character is the base it treats as methylated, the second as unmethylated. For 5-Base those are simply the wrong way round, so swapping them:

```cpp
ReadOrient OT{'T', 'C', 0, 0};
ReadOrient OB{'A', 'G', 1, 1};
```

should give correct calls. I checked that those two fields are each read in exactly one place (`patter.cpp:153` and `:159`), so the change is contained — and the CpG-detection test around it accepts both `C`/`T` and `G`/`A`, so it keeps working either way. Rebuild wgbs_tools and run your usual `bam2pat` → UXM.

Two caveats. I haven't validated this on real 5-Base data, so please treat the first run as a sanity check rather than a result — if the inversion is what's wrong, your 5-Base and EM-seq deconvolutions should start converging instead of opposing each other. And it's a local patch, so I'd rather offer it to nloyfer as a proper opt-in flag than have you maintain a fork.

One thing for when you do re-run through Bismark: the genome you align against needs the same chromosome naming as your `wgbstools init_genome hg38` — so `chr1`, not `1`. `patter` throws on an unrecognised CpG locus, so a mismatch fails loudly, but only at the end of the run, which is a slow way to find out.

Best,
Felix
