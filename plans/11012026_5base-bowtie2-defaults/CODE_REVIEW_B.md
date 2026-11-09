# Code Review B — 5-Base Bowtie 2 defaults (`feat/5base-bowtie2-defaults`)

Reviewer B, fresh context. Net change `git diff d1764d0..HEAD` (HEAD `7295720`), final state only.

## Summary

The builder change is small and correct: `--illumina_5base --bowtie2` resolves an effective
`maxins = cli.maxins.or(800)`, rejects `-I > maxins` (strict `>`, Bowtie 2 only) inside
`config::resolve` before any output or aligner spawn, and emits `--maxins` exactly once.
HISAT2, minimap2/rammap and the bisulfite builder are untouched. `--dovetail` is never pushed
on the 5-Base path — and, worth noting, it was never pushed at the base `d1764d0` either
(`common_aligner_options` has no dovetail logic; the pre-existing
`five_base_omits_bisulfite_policy_and_runtime_threads` test already forbade it). The net
dovetail change is therefore tests + docs only, which the CHANGELOG correctly omits.

Tests are strong: each regression named in the brief (dovetail returning, 800 dropped, `-I`
compared with `cli.maxins` instead of the effective cap, `>=` instead of `>`, HISAT2 gaining
the default or the check) fails at least one unit test, and the real-bowtie2 integration tests
exercise the actual 500/800/1200 boundaries and dovetail dropping.

The Python helper and bash harness are careful and fail loudly. I found **no Critical or High
issues**. Findings are Medium/Low: one doc over-statement about the engine comparison, a
precision nit on the `--no_dovetail` TLEN sentence, a missing PR link in the CHANGELOG entry,
a stale-cache risk in `own_cov10`, and some minor helper-statistics points.

Verification run (this Mac, unsandboxed because the worktree is outside the sandbox root):

| Command | Result |
|---|---|
| `cargo test -p bismark --lib aligner::` | 614 passed, 0 failed |
| `cargo test -p bismark --test aligner_five_base_bowtie2` | 3 passed (bowtie2 2.5.x on PATH, so not skipped) |
| `cargo test -p bismark --test aligner_cli five_base` | 15 passed |
| `cargo fmt -p bismark -- --check` | clean |
| `python3 -I validation/five_base_rescued.py --selftest` | selftest OK |
| `/opt/homebrew/bin/bash validation/five_base_engine_bench.sh selftest` | 52 passed, 0 failed |

## Issues by area

### Logic

**(a) Builder — combination matrix (all verified by reading `options.rs:261-305` and the tests):**

| Input (5-Base Bowtie 2) | Emitted | OK |
|---|---|---|
| none | `... --maxins 800` | yes |
| `-X 500` / `-X 1200` | `--maxins 500` / `1200` | yes |
| `-I 600` / `-I 800` | `--minins N --maxins 800` | yes (boundary `==` allowed) |
| `-I 801` / `-I 900` | error naming 800 | yes |
| `-I 900 -X 1000` | `--minins 900 --maxins 1000` | yes |
| `-I 600 -X 500` | error naming 500 | yes |
| `--local` | `-q --local --score-min G,20,8 --maxins 800` | yes |
| `--dovetail` / `--no_dovetail` / both / `--old_flag` | no `--dovetail` token | yes |
| HISAT2 any | no default `--maxins`, `-I` passes through | yes (by design) |
| minimap2 | clean slate unchanged | yes |

Other callers: `config.rs:1028` is the only caller of `build_five_base_aligner_options`;
`five_base_aligner_options` (`mod.rs:1891`) just appends `-p N --reorder`, so the report line,
the run summary (`config.rs:1812`) and the argv all carry the new string consistently. Nothing
parses `aligner_options` for `--maxins` (grep: only the bisulfite builder, `align.rs:1747` and
`report.rs` test literals). The `-I` check runs inside `config::resolve`, before output
creation; `five_base_1134_invalid_tuning_fails_before_output_creation` asserts that.

Error wording `-I/--minins (900) is larger than the maximum insert size (800); raise -X/--maxins`
matches the neighbouring 5-Base messages (lower-case, colon/semicolon, no `!`), and names the
effective cap even when the user never typed `-X`, which is the helpful case.

**L1 (Low) — the "dovetail" half of the source comment describes code that is not there.**
`options.rs:282`: `// Bowtie 2 defaults to an 800 bp insert cap; --dovetail stays off (its overhangs are not genomic).`
There is no dovetail handling in this function; "stays off" reads as narrative of the
intermediate `72bbf7b` state, and "(its overhangs are not genomic)" is justification. Under the
house style this belongs in the commit message. See "would fix directly".

**L2 (Low) — HISAT2 keeps silent zero-concordance on `-I > -X`.** By intent, and the docs
scope the error to Bowtie 2, so this is not a defect. HISAT2 has the same failure mode
(`-I 900` with its 500 default gives no concordant pairs), so it could be extended later.

### Errors / docs accuracy

**M1 (Medium) — "The comparison covers two 5-Base WGS gDNA libraries" over-states the engine comparison.**
`illumina-5-base.md` ("Choosing an engine"): the table, and the minimap2/HISAT2 rows, come from
NA12878 only. HG002 was run only for Bowtie 2 `-X 500` against the new default
(`bench_results/HG002/screen/` has B, B-old, B-new; no A or C). A reader will take "the
comparison" to mean the engine table. Suggested wording: "The engine comparison covers one
5-Base WGS gDNA library (NA12878); the Bowtie 2 default was also confirmed on HG002.
Short-fragment libraries such as cfDNA were not tested." I checked the HG002 figures
themselves: 90.5% → 94.5% (`metrics.tsv`) and 542,072 → 704,973 own-set cov≥10 CpGs (+30.05%,
`score.tsv`). Both are correct.

**L3 (Low) — `--no_dovetail` sentence is slightly broader than the reachable case.**
Docs: "`--no_dovetail` only flips the TLEN sign of OB pairs whose mates start at the same
position." From `output.rs:569-597`, the `dovetail` flag matters only in two branches:
- `start1 <= start2 && end2 >= end1 && flag_1 == 83` (OB, R1 leftmost or tied);
- `start2 < start1 && end1 >= end2 && flag_1 == 99` (OT, R2 strictly leftmost). For a proper
  pair this geometry is a dovetail, which Bowtie 2 (no `--dovetail`) marks non-concordant and
  `five_base_emit_pe_record` drops on `0x2`. So it is unreachable.

In the OB branch, `start1 < start2` is a dovetail (dropped) unless R2 is contained, which goes
to the unaffected containment branch. With `start1 == start2`, `end2 > end1` means the forward
mate extends past the reverse mate's end. Bowtie 2's pair classifier calls that a dovetail too,
so it is also dropped. The reachable case is therefore **OB pairs whose mates cover the same
reference span**, a subset of "start at the same position". The sentence is not wrong in its
effect, since it only says which pairs can change. It is imprecise, and I verified it for
Bowtie 2 only: HISAT2 is assumed to share Bowtie 2's default no-dovetail policy, and the
sentence sits in a paragraph that covers both engines. Optional rewording: "...of OB pairs
whose mates align to the same span."

**L4 (Low) — CHANGELOG entry lacks the issue/PR link every neighbour carries.**
`CHANGELOG.md:18` follows the bold-sentence format and is one sentence, so it passes the
one-sentence rule. Every other entry in the section carries `([#NNN](url))`; this one has none
(there is no issue). Add the PR number when the PR is opened. The repo-relative docs link
matches the existing `--allow_discordant` entry style (line 41).

### Test strength (b)

Each regression in the brief would be caught:

- **`--dovetail` re-emitted:** caught by `five_base_omits_bisulfite_policy_and_runtime_threads`
  (unit), the token-exact strings in `five_base_bowtie2_defaults_to_maxins_800`, the
  `count("--dovetail") == 0` checks in `five_base_bowtie2_maxins_default_reaches_argv` and in
  `five_base_1134_tuning_reaches_each_aligner_and_report`, and end-to-end by
  `five_base_pe_bowtie2_drops_dovetailed_pairs`. In that test both dovetail pairs are real
  dovetails under Bowtie 2's rule (OT: reverse R2 starts 20 bp before forward R1; OB: reverse
  R1 starts 20 bp before forward R2), so they would become concordant (`0x2`) and survive
  `five_base_emit_pe_record`, making `records.len() == 6`.
- **800 default dropped:** caught by about eight exact-string unit assertions, the argv test,
  `five_base_1134_default_argv_is_exact`, the report substring in
  `five_base_bowtie2_unconverted_index_end_to_end`, and the real-bowtie2 600 bp fragment
  (`Some(600)` at default).
- **`-I` checked against `cli.maxins`:** `-I 900` / `-I 801` alone (no `-X`) would stop erroring.
  Caught.
- **`>=` instead of `>`:** `-I 800` expects success. Caught.
- **HISAT2 gaining the default or the check:** `five_base(Aligner::Hisat2, &["-I","900"])` and
  the HISAT2 argv cases (`None` maxins). Caught.

**L5 (Low) — the real-bowtie2 tests skip silently when bowtie2 is absent.** This is the
pre-existing `have_bowtie2()` pattern, and memory notes a real-bowtie2 CI gate exists. Confirm
that CI job runs `aligner_five_base_bowtie2` so the two new tests are not vacuous there.

**L6 (Low) — unrelated churn in `options.rs:728`.** The trailing comma after `"--reorder"` was
removed inside the `assert!` array (left over from `72bbf7b`/`d27b11e`). `rustfmt` accepts it
because it is inside a macro, but the diff touches a line the change does not need. See "would
fix directly".

### Python helper (c) — `validation/five_base_rescued.py`

Verified correct:
- **FASTQ-order join:** `fastq_names` truncates at whitespace and then strips `/1`, the same
  order as `five_base_fastq_id` (`fix_id(..., true)` then `strip_mate_suffix`). An
  unmatched or out-of-order BAM record leaves the cursor stuck and raises at the end ("not in
  FASTQ order"); the selftest covers the swapped case.
- **CIGAR/MD:** `aligned()` consumes `M=X` on both sides, `IS` on the read only and `DN` on the
  reference only; `H` and `P` consume neither, which is correct. `span()` counts `MDN=X`. MD
  parsing advances the reference over deletions and ignores insertions, which is correct
  because MD has reference coordinates. The selftest covers `2S3M1I2M1D2M` with `4A^G0C1`.
- **Cycle orientation:** a reverse-strand record's SEQ/XM are reverse-complemented, so index
  0 is the last sequenced base. `cycle()` and the M-bias `xm[::-1]` for forward R1 are both
  right.
- **Strand of CpG calls:** `r1.reverse` decides for both mates, which is correct for
  directional OT/OB. The OB call sits at the + coordinate of the G, which matches DRAGEN's `-`
  strand C position.
- **Matched-site agreement:** per-site mean of `p` (methylated) or `1-p` (unmethylated),
  admitted minus pool, over DRAGEN cov≥10 sites present in both. The definition is
  self-consistent and the selftest checks one hand-computed site.
- **Verdict boundaries:** inclusive at each threshold, `NaN` → FAIL, and `INSUFFICIENT` gated by
  `MIN_N`. All of this has selftest coverage.

**L7 (Low) — `quantile` is "rank floor(n·f)+1", not nearest-rank.** For n=10 it returns the 2nd
value as p10 and the 10th as p90; nearest-rank gives the 1st and 9th. It only feeds INFO rows
(`|TLEN| p10/p50/p90`), so no verdict changes. A one-line docstring stating the definition
would do.

**L8 (Low) — overlapping mates double-count CpG/CHG/CHH calls.** `SetStats.add` and
`cpg_calls` count both mates' calls, so a site covered by both mates counts twice. Admitted
pairs (500-800 bp) overlap less than "rest" pairs, which shifts call weighting between the
sets a little. It is unlikely to move A3/A4/A6 by anything near their margins (HG002 A6 =
−0.0004 against a −0.02 bar), but A3/A4 are not overlap-deduplicated the way the extractor's
`--no_overlap` is. Worth one sentence in RESULTS if anyone asks.

**L9 (Low) — memory at ~100M pairs.** Python `set`s of up to tens of millions of `int`s
(`shared = {c>>1 ...} & {c>>1 ...}`) take a few GB. That is fine on oxy, where it demonstrably
completed (committed `rescued.json` for HG002 and NA12878), but it would not fit a laptop.
Building one set and filtering the other on the fly would roughly halve the peak. Not needed
now.

**L10 (Low) — the default (non-`--all-bases`) mode is now vestigial.** It tallies only overhang
bases. Under the final design admitted pairs have no dovetails (HG002: `dovetailed/contained
0/5`), so that mode always yields INSUFFICIENT for A5, and the harness always passes
`--all-bases`. Consider making all-bases the only behaviour, or the default.

**L11 (Low) — a truncated `samtools view` line raises `IndexError`, not a named error.** A
`samtools view` killed mid-stream can leave a partial last line, and `Mate.__init__` then
fails with a bare traceback. The run still fails, which is acceptable.

### Bash harness (d) — `validation/five_base_engine_bench.sh`

Verified:
- **`count_tok`:** `grep -c` prints `0` and exits 1; `|| true` keeps the `0`.
- **`first_line`:** `sed -n 1p` drains its input, so there is no SIGPIPE.
- **`trim_galore ... | grep -m1`:** can SIGPIPE, but it sits inside a `printf` argument, so
  `set -e` and `pipefail` do not fire, and the value is then checked against `want`.
- **`check_pg_line`:** pads CL with spaces, so a trailing `--dovetail` is still caught by
  `" $t "`, and `--dovetail` does not match `--no_dovetail`.
- **`mbias=$(ls ...) || die`:** an empty glob fails `ls`, so it dies explicitly.
- **`msg=$( (cmd_cell ...) ) && bad++`:** safe under `set -e`.

**M2 (Medium) — `own_cov10` caches its score file by existence only.** `if [[ ! -s $score ]]`
reuses `$ROOT/runs/$s/$tag/repro_score_$arm.tsv` even after the cell is re-run (`cmd_cell`
rewrites `$d` but never removes this file). A re-run cell would then be compared using the old
cell's own-set row. Either key the cache on the cell's `DONE` content, as `cmd_cell` does with
`$key`, or delete it in `cmd_cell`.

**L12 (Low) — errexit is not inherited inside `$(...)`.** `own=$(own_cov10 ...)` runs in a
command substitution, and bash leaves `set -e` off there by default (no `inherit_errexit`).
A failing `ls` or `python3` inside `own_cov10` therefore does not stop the function; it falls
through to the final `awk`. That `awk` exits 2 on a missing file, which aborts the parent, and
an empty result is caught by `compare_cells`' `-n` guards. The run still fails, but the error
message points at awk, not at the real cause. `shopt -s inherit_errexit` at the top would make
it precise.

**L13 (Low) — `own_cov10` uses `ls *CpG_report.txt*` without `| head -1`.** `cmd_cell` uses
`| head -1` for the same glob. If two files ever match, `--arm "$arm=$cpg"` would carry an
embedded newline. Make the two consistent.

**L14 (Low) — portability.** The harness uses `md5sum`, `zcat -f`, `readlink -f`, `local -A`
and `\t` in sed. These are fine on oxy (GNU, bash ≥ 4) and the selftest passes on this Mac
with Homebrew bash and `/usr/bin/sed`, but the real subcommands are Linux-only. A one-line
header note would do.

### Structure / style

- `SmallGenome` in the integration test removes the duplicated genome+index setup cleanly.
- The `mod.rs` `five_base_stub` change only edits a hand-written fixture string; harmless, and
  consistent with the new default.
- `rust/README.md` milestone line matches the existing journal style.
- New Python and bash comments are one line and state current facts. The exception is the
  `options.rs:282` comment (L1).

## Fixes I would apply directly (not applied — caller to apply after approval)

1. `rust/bismark/src/aligner/options.rs:282`: shorten to the current fact.
   ```rust
   // Bowtie 2 defaults to an 800 bp insert cap.
   ```
2. `rust/bismark/src/aligner/options.rs:728`: restore the trailing comma (`"--reorder",`) to
   drop the unrelated hunk.
3. `docs/src/content/docs/rust/illumina-5-base.md`: replace "The comparison covers two 5-Base
   WGS gDNA libraries; short-fragment libraries such as cfDNA were not tested." with "The
   engine comparison covers one 5-Base WGS gDNA library; the Bowtie 2 default was also
   confirmed on HG002. Short-fragment libraries such as cfDNA were not tested." (M1)

## Recommendations by priority

- **Critical:** none.
- **High:** none.
- **Medium:**
  - M1: correct the "two libraries" engine-comparison claim (fix 3).
  - M2: invalidate the `own_cov10` score cache when a cell is re-run.
- **Low:**
  - L1/L6: fixes 1 and 2.
  - L4: add the PR link to the CHANGELOG entry once the PR exists.
  - L3: narrow the `--no_dovetail` sentence to "mates align to the same span".
  - L5: confirm CI runs `aligner_five_base_bowtie2` with bowtie2 present.
  - L7–L11: Python helper polish (quantile docstring, overlap note, one-sided set, retire the
    overhang mode, named error on truncated SAM).
  - L12–L14: harness robustness (`inherit_errexit`, consistent `head -1`, Linux-only note).
  - L2: optionally extend the `-I > -X` guard to 5-Base HISAT2 in a later change.
