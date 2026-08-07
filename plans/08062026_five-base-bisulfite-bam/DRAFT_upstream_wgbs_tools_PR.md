> **Draft — not submitted.** Proposed PR to [`nloyfer/wgbs_tools`](https://github.com/nloyfer/wgbs_tools).
>
> ✅ **Validation checks 1 and 4 now PASS** — measured 2026-08-07, see `EXPERIMENT_patter_swap.md`.
> Unpatched `patter` calls 0.0 % methylation on a fully-CpG-methylated 5-Base dataset; patched
> calls 100.0 %, matching Bismark's own `XM` exactly. **Both strands verified independently**
> (OT 0→100 %, OB 0→100 %), which rules out a one-sided patch.
>
> ⚠️ Still outstanding before submission: **check 2** (default path byte-identical on EM-seq
> data — the "we broke nothing" evidence a maintainer will want most) and **check 3** (paired
> 5-Base/EM-seq through UXM, which needs real data). Also still true: the code change below is a
> **sketch** — the CLI plumbing (`--five_base` through `patter`'s argument parsing and
> `bam2pat.py`'s pass-through) has not been read yet and must be before this is a real patch.
>
> Note the reporter's field attempt at the same patch produced *no* change, which is very likely
> a silent build failure on his side (`setup.py`'s `raise` is commented out) rather than evidence
> against the patch. Do not cite his result either way in the PR.

---

## Title

`patter: optional inverted-polarity mode for 5-Base / TAPS-style libraries`

## Body (this is the whole visible description)

Adds an optional `--five_base` flag that swaps `patter`'s methylated/unmethylated base constants, so libraries in which 5mC reads as `T` — Illumina 5-Base, and TAPS-style chemistries generally — produce correct calls instead of systematically inverted ones. Default behaviour is unchanged: without the flag the constants are exactly as before.

<details>
<summary>Background and reasoning (AI-assisted — bin this section if it isn't useful)</summary>

**The problem.** `patter` derives each call from the read character at a CpG position, comparing it against `ReadOrient::ref_chr` (methylated) and `ReadOrient::unmeth_seq_chr` (unmethylated):

```cpp
// patter.h
ReadOrient OT{'C', 'T', 0, 0};
ReadOrient OB{'G', 'A', 1, 1};
```

Bisulfite and EM-seq convert *un*methylated C to T, so a `C` means methylated. Illumina 5-Base is the chemical inverse — the enzyme converts **5mC → T** and leaves unmethylated C as C — so on 5-Base data every call comes out backwards. Not noisy or degraded: inverted.

**Why a two-constant swap is sufficient.** `ref_chr` and `unmeth_seq_chr` are read in exactly two places in the codebase, `patter.cpp:153` and `:159`, both inside the call branch of `compareSeqToRef`. Swapping them therefore inverts the call polarity completely and affects nothing else.

It also survives the CpG guard, which is the part that could plausibly have broken:

```cpp
// is_cpg(), patter.cpp
ro.shift == 0 : (seq[j] == 'C' || seq[j] == 'T') && seq[j+1] == 'G'
ro.shift == 1 : (seq[j] == 'G' || seq[j] == 'A') && seq[j-1] == 'C'
```

Both branches accept either base, so they are polarity-agnostic. The context bases they require — `seq[j+1] == 'G'` for OT, `seq[j-1] == 'C'` for OB — are guanines on the read's own strand under 5-Base chemistry and so are never converted; they still read `G` and `C` respectively.

**Sketch of the change.** A flag-selected pair of constants, rather than editing the defaults:

```cpp
ReadOrient OT = five_base ? ReadOrient{'T', 'C', 0, 0} : ReadOrient{'C', 'T', 0, 0};
ReadOrient OB = five_base ? ReadOrient{'A', 'G', 1, 1} : ReadOrient{'G', 'A', 1, 1};
```

plus `--five_base` threaded through `patter`'s argument parsing and passed by `bam2pat.py`.

**Considered and rejected: the `MM`/`ML` path.** `patter` already auto-detects standard SAM base-modification tags (`patter.cpp:334-338`) and routes them to a modification-aware path, which would have been a cleaner answer — no polarity question at all. But `patter.cpp:341-343` throws `"Unrecognized bam format: paired end and nanopore"`, and Illumina 5-Base is a paired-end library. Lifting that restriction is a much larger change and is not proposed here.

**Note on scope.** This does not attempt to auto-detect 5-Base data. Explicit opt-in seemed safer than sniffing, since a wrong guess silently inverts every call.

</details>

---

## Validation required before submission

| # | Check | Expected |
|---|---|---|
| 1 | Patched `patter` on a real 5-Base BAM vs Bismark's own cytosine report for the same BAM | Strong positive per-CpG correlation. **Unpatched** should give the mirror image — that contrast is the actual evidence |
| 2 | Unpatched `patter` on an EM-seq BAM, before and after the patch is added but not enabled | Byte-identical `.pat` output — proves the default path is untouched |
| 3 | Paired 5-Base and EM-seq libraries from the same sample through UXM | Deconvolution results converge rather than oppose (this is the reporter's actual use case) |
| 4 | OT and OB reads separately (`--top_only` / `--bottom_only`) | Both strands correct; catches a swap applied to only one `ReadOrient` |

## Sequencing note

Bismark's own `--five_base_bisulfite_bam` converter (#1095) is **not** a prerequisite and does not overlap: this patch fixes 5-Base data at the consumer, needing no Bismark at all, and works on DRAGEN BAMs. The converter serves Bismark users who would rather not run a patched third-party binary. Either alone solves the reporter's problem; both is better.
