## Summary

Port the Perl `bismark_genome_preparation` (~848 LOC, v0.25.1) to a Rust binary in the cargo workspace. Reads a genome directory of FASTA file(s) and writes two in-silico bisulfite-converted references — a **C→T-converted** (top-strand) copy and a **G→A-converted** (bottom-strand) copy — under `<genome>/Bisulfite_Genome/{CT,GA}_conversion/`, then runs an external indexer (`bowtie2-build` default / `hisat2-build` / `minimap2 -d`) on each.

A **different shape** from the post-alignment ports (dedup/extractor/methcons/bedgraph/c2c, all BAM tools): **FASTA in → converted FASTA out → external indexer subprocess**. **No BAM I/O** — does *not* use `bismark-io`. The algorithm is trivial (`uc` → map non-`ATCGN` to `N` → C→T / G→A transliteration); all difficulty is **byte-identity of the FASTA layout** — line wrapping, header rewriting, file/chromosome ordering, line endings.

## Change

New crate `bismark-genome-preparation`, binary `bismark_genome_preparation_rs`. CLI matches Perl:

- Positional genome folder; `--bowtie2` (default) / `--hisat2` / `--minimap2`|`--mm2`; `--path_to_aligner`, `--parallel`, `--single_fasta`, `--large-index`, `--verbose`/`--help`/`--man`/`--version`.
- `--slam` — **deprecated** (slated for removal); T→C / A→G transitions; headers stay `_CT_converted`/`_GA_converted`.
- `--genomic_composition` — **deferred** in v1.0 (accepted-and-ignored with a one-line note).
- `--combined_genome` — **new Bismark-Rust extension** (opt-in, additive): also builds a single combined CT+GA reference FASTA + combined index for the future aligner. Not byte-gated.
- Outputs match Perl exactly: `Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa` + GA counterpart (MFA default), or per-chromosome `<chr>.{CT,GA}_conversion.fa` with `--single_fasta`.

## Implementation notes

- Standalone crate (`clap` + `flate2` + `which` + `anyhow`/`thiserror`); **raw line-streaming** (not noodles-fasta) to preserve byte-exact wrapping; gzip input via `MultiGzDecoder`; indexer via `std::process::Command` with `BISMARK_BIN → which → current_exe` discovery; **concurrent CT/GA index builds** (mirror Perl's `fork`).
- **Acceptance gate = byte-identical CT/GA converted FASTA vs Perl v0.25.1**; the index build is a *secondary* check. The **Perl script is the primary test oracle** (synthetic, auto-skip if absent) from Phase A onward.
- SPEC + phased plan + dual plan-review: `plans/05302026_bismark-genome-preparation/{SPEC,PLAN,PLAN_REVIEW_A,PLAN_REVIEW_B}.md` (branch `rust/genome-preparation`, worktree `~/Github/Bismark-genomeprep`).

## Design pitfalls to avoid

- **Chromosome-name extraction = exact Perl:** a **bare `>` is NOT an error** (empty name → `>_CT_converted`); a **leading-whitespace** header → **empty** name (Perl `split /\s+/` keeps the leading empty field — use a split that does NOT skip it, **not** `split_whitespace()`); only a first byte ≠ `>` errors.
- **Raw-byte line transform preserving the terminator:** CRLF stays CRLF; a final line without a newline keeps none; interior whitespace → `N`. Do **NOT** trim-and-re-emit `\n`.
- **SLAM headers stay `_CT_converted`/`_GA_converted`** even in slam mode (Perl never changed them — a `### TODO` that was never acted on).
- **Glob ordering = lexical on `file_name()` bytes** (`chr1, chr10, chr2` — not numeric); fixes MFA concatenation order + indexer `file_list`.
- **Extension precedence:** `.fa` → `.fa.gz` → `.fasta` → `.fasta.gz`, **first non-empty group wins** (never mixed).
- **Always emit `--threads N`** (N=1 default, Perl-faithful); **validate `--path_to_aligner` early (Step I)**, before conversion, with no `which`-fallback when an explicit path is given.
- **`--combined_genome` is additive/opt-in**, not byte-gated (no Perl counterpart); its byte-oracle is built from the converted stream (mode-independent); alignment-correctness validation is **deferred to the aligner rewrite**.

## Sub-issues

Tracked as real linked sub-issues (see the Sub-issues panel):

- #905 — spec (SPEC + plan + dual plan-review + byte-identity contract)
- #906 — Phase A: scaffold + CLI + core CT/GA conversion + MFA + bowtie2 (MVP)
- #907 — Phase B: `--single_fasta` + hisat2/minimap2 indexers
- #908 — Phase C: `--slam` (deprecated) + edge cases + accept-and-ignore
- #909 — Phase D: `--combined_genome` extension (combined ref + index)
- #910 — byte-identity gate (synthetic Perl-oracle + real genome on oxy)
- #911 — docs: README + CHANGELOG + mkdocs page

## References

- Perl source: [`bismark_genome_preparation`](https://github.com/FelixKrueger/Bismark/blob/master/bismark_genome_preparation) (848 LOC, v0.25.1)
- SPEC + plan: `plans/05302026_bismark-genome-preparation/{SPEC,PLAN}.md` (branch `rust/genome-preparation`, worktree `~/Github/Bismark-genomeprep`)
- Related future initiative (NOT this epic): combined-genome alignment + a Rust aligner engine (the `--combined_genome` output is produced here so that work can consume it directly).
