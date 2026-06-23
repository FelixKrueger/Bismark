# Experimental paired-end minimap2

**Date:** 2026-06-23 · **Branch:** `rust/minimap2-pe` (off `origin/rust/iron-chancellor`)

Follow-up to the v1.x epic (`plans/06052026_bismark-aligner-v1x/`), Phase 4, which
**hard-rejected** PE minimap2 for lack of a trustworthy Perl oracle. This wires it anyway,
as an explicitly **experimental, never-silent, NOT-byte-identical** path. Per the
maintainer's choice: mirror Perl's actual mechanics (positional two-file invocation),
enable directly via `--minimap2 -1/-2`.

## Why there is no oracle (unchanged from Phase 4)

The Perl `paired_end_align_fragments_to_bisulfite_genome_fastQ_minimap2` (`bismark`
6623-6723) is unfinished WIP: a `# TODO: Need to check this.`, two `sleep(1)` calls per
read pair, and read-1 detection via `s/\/1$//`. Its PE report writer (1845-1850) has no
`$mm2` branch, mislabeling minimap2 PE as "HISAT2". So byte-identity to Perl is out of
scope; the path is gated behind a never-silent notice.

## Empirical findings (minimap2 2.31, installed via brew; matches the pinned 2.31-r1302)

Running `minimap2 <opts> <index>.mmi <input1> <input2>` (Perl's invocation):

1. **Interleaved pairing works.** minimap2 reads the two files in lockstep and emits the
   mates **interleaved** (read1, read2, read1, read2, …) in input order — so the existing
   consecutive-line PE pair reading (`PairedAlignerStream`) applies unchanged.
2. **No paired-end flags.** Even with `-ax sr` and a proper FR pair, minimap2 emits
   **SE-style flags** (FLAG 0 / 16 mapped, 4 unmapped; RNEXT `*`; TLEN 0; no 0x1 PAIRED
   bit). minimap2 aligns the two files as **independent single-end reads**. Fine for
   Bismark: it re-derives all mate fields (FLAG 99/147 by index, RNEXT/PNEXT/TLEN) itself.
3. **QNAME suffix not clipped.** minimap2 keeps the converter's full `/1/1`,`/2/2`
   (Perl's `s/\/1$//` would strip only one → a stray `/1`, the `# TODO` bug).

## Implementation (all changes gated / byte-neutral for Bowtie 2 / HISAT2)

- `config.rs` — reject only **rammap** PE (SE-only); minimap2 PE proceeds.
- `lib.rs` — `minimap2_paired_experimental_notice()`, emitted in `run()` when the resolved
  layout is PE minimap2 (SE minimap2 stays silent).
- `align.rs` — `build_pe_argv` minimap2 arm: `<opts> <index>.mmi <input1> <input2>`
  (positional, no `-x`/`-1`/`-2`/orient). `SamPair::from_lines` reads the read-1 marker via
  a shared `strip_read1_marker` (`/1/1` then `/1`) — provably byte-neutral for the frozen
  backends (their tail is a single `/1`; the conv tag `__CT`/`__GA` precedes it).
- `merge.rs` — enforce **PE concordance** for minimap2 (gated on `Aligner::Minimap2`) and
  skip non-concordant "pairs" as no-PE-alignment. A concordant pair is: both mates mapped
  (neither FLAG 4), same chromosome, **FR orientation** (mates on opposite strands, what
  Bowtie 2's default `--fr`+`--no-discordant` guarantees), and **fragment length within
  `[--minins, --maxins]`** when those are set. Bowtie 2 / HISAT2 are concordant by
  construction (the aligner enforces `--fr`/`--no-mixed`/`--maxins`), so the branch never
  fires for them. `--minins`/`--maxins` are now carried in `RunConfig` (`config.rs`) and
  threaded to `check_results_paired_end`; fragment length uses a `ref_span` (CIGAR
  reference-consuming ops) helper.
- `output.rs` — `--ambig_bam` raw PE tag strip tolerates the `/1/1` form.
- Report label is the correct `minimap2` (deliberate divergence from Perl's broken HISAT2
  mislabel).

The faithful Bowtie 2 / HISAT2 PE merge / scoring / MAPQ / `XM`-`XR`-`XG` / BAM output is
reused unchanged.

## Concordance (step 1, 2026-06-23)

minimap2 aligns the two mate files as independent single-end reads with NO concordance
enforcement of its own, so Bismark enforces it: both mates mapped, same chromosome, **FR
orientation**, fragment within **`[--minins, --maxins]`**. Fragment bounds default to
**UNBOUNDED** (long-read-oriented — a short-read insert cap like Bowtie 2's 500 would
wrongly drop valid long-read pairs); pass `--maxins` to cap the insert. This closes the
"same-chromosome but discordant/huge-TLEN" gap from the first cut.

## Remaining for a non-experimental ("concordance-gated") status (step 2)

Still needs a measured concordance gate on real long-read bisulfite PE data vs the trusted
PE backends (Bowtie 2 / HISAT2 PE on short-read WGBS for the short-read case), with a
documented tolerance, determinism, and worker-invariance — mirroring how `--rammap` /
`--combined_index` were gated. Until then the never-silent EXPERIMENTAL notice stays. Exact
byte-identity to Perl is permanently out of scope (no oracle).

## Tests

- lib: `SamPair::from_lines` double-suffix + single-suffix byte-neutrality; `build_pe_argv`
  positional shape; merge skips half-mapped / cross-chromosome, maps concordant.
- integration (`tests/cli.rs`): `make_fake_minimap2_pe` (SE-style flags, interleaved,
  un-clipped `/1/1`) + `minimap2_pe_mapped_names_and_report` (notice on stderr, two BAM
  records FLAG 99/147 + mate fields, `minimap2` report label + option string). The old
  `minimap2_paired_end_is_rejected` is replaced by `..._is_accepted_not_rejected`.

## Real-data sanity (Perl NOT an oracle)

On a small real bisulfite PE dataset + a real `.mmi`: confirm mate-field consistency
(RNEXT `=`, reciprocal PNEXT, TLEN sign), cross-check XM/XR/XG vs the Rust **SE** minimap2
result for the same reads aligned singly, and confirm no `Either the first or the second id
need to be read 1` errors (the direct test of the QNAME handling).
