# `--no_stream_converted` wording fix (2026-11-06)

The docs and `--help` said `--no_stream_converted` lets you inspect the converted reads. The temporary `_C_to_T` / `_G_to_A` files are deleted once each input has been aligned (`aligner/mod.rs:1882` and the sibling cleanup sites; Perl `legacy_perl/bismark:1975-1993` SE, `2155-2174` PE), so that only works while a run is in progress.

Changed:
- `docs/src/content/docs/options/alignment.md` (the `--no_stream_converted` entry)
- `rust/bismark/src/aligner/cli.rs` (the `--no_stream_converted` doc comment, which is the `--help` text)

Left unchanged: CHANGELOG and wiki, which only say the reads are written to `--temp_dir` as files, which is accurate.

Felix asked for the docs fix on 2026-11-06 and said "implement" for the `cli.rs` change. No PLAN.md: wording-only. A dual code review was requested before the PR.

## Review fixes (after `CODE_REVIEW_A.md`, `CODE_REVIEW_B.md`; both approve)

- `alignment.md`: says a failed or killed run leaves the files in `--temp_dir`; "As in Perl Bismark" dropped (both reviewers).
- `alignment.md:196`: "keeps files" → "uses files" (both reviewers).
- Perl citation in these notes corrected (both reviewers).
- Not changed:
  - the `*_CT_GA_tagged*` filename of single-pass mode, which is temporary too;
  - silent failed deletes, which need a separate PR;
  - the `cli.rs` fallback list omitting rammap with FastA input, which is outside this change.
