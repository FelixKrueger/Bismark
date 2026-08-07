# Session Handoff — 2026-08-07

**Next session's job:** land [#1095](https://github.com/FelixKrueger/Bismark/issues/1095). **It is implemented, dual-code-reviewed, and pushed to `dev`** — what remains is process, not engineering:

1. **Open the PR / merge to `master`.** #1095 is still OPEN and the work is unreleased. `dev`→`master` is the release signal.
2. **Cut 3.2.0.** `## Unreleased` now carries **five** aligner entries and all three version literals still read `3.1.0`. Minor bump (behaviour changes in default paths).
3. **The upstream `nloyfer/wgbs_tools` PR** (`DRAFT_upstream_wgbs_tools_PR.md`) now has *both* a synthetic control and real-sample confirmation. Only validation **check 2** remains — EM-seq `.pat` byte-identical with the patch present but not enabled — which is doable locally with the machinery in `EXPERIMENT_patter_swap.md`.

`dev` is at **`7be9dde`**, clean, pushed, in sync.

---

## 1. What we accomplished

Eleven commits from `fab3e27`. **#1095 went the whole pipeline in one session:** diagnosis → plan (rev 0→2) → dual plan review + targeted section review → fixture → implementation → dual code review + coverage audit → review fixes.

| Commit | What |
|---|---|
| `ffa1176`, `5bf8b55`, `6cff75b`, `5694cf9` | Handoffs (this is the 5th) |
| `bfdf207`, `5859d59`, `d0a86c5` | Plan rev 0 → 1 → 2, with `PLAN_REVIEW_A/B/36.md` |
| `0e4c705` | Soft-clip + indel fixture, oracle-validated |
| `18ff591` | `patter` two-constant swap experiment |
| **`6a3ea0d`** | **Implementation** — `--five_base_bisulfite_bam` |
| **`7be9dde`** | **Dual code review + coverage audit, and the fixes** |

**The feature is proven, not just tested.** With **stock, unmodified** `patter` on a fully CpG-methylated 5-Base control:

| Route | METH / total | Methylation |
|---|---|---|
| stock `patter` + raw 5-Base BAM | 0 / 152 | **0.0 %** |
| patched `patter` + raw 5-Base BAM | 152 / 152 | 100.0 % |
| **stock `patter` + converted BAM** | **152 / 152** | **100.0 %** |

Ground truth 100 %; Bismark's own `XM` agrees (`Z=168, z=0`). Flip rate exactly `1.000000` on 5-Base input, `0.000000` on the bisulfite fixture with a byte-identical SAM body.

**@Danielsm8 confirmed it on real data** ([`5219193146`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5219193146)): after the `patter` patch + UCSC hg38, *"when I do this and run it through uxm, it works!!!!!!!!!!!!!"*, with a paired-EM-Seq figure. Four replies posted on #787; the last ([`5220352348`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5220352348)) gives him the contig-intersection diagnostic and tells him he can drop the patched `wgbs_tools` once he uses the converter.

---

## 2. What's still pending

| Item | State |
|---|---|
| **#1095 PR / merge + 3.2.0 cut** | The only thing between this and shipped |
| Upstream `wgbs_tools` PR | Drafted, unsent. Checks 1 + 4 pass (synthetic) and check 3 is effectively satisfied by the reporter's figure. **Check 2 outstanding** + reading `patter`'s CLI plumbing |
| @Danielsm8 | His ~80 % vs usual ~98 % marker recovery is very likely the **partial contig intersection** (G15); diagnostic sent, awaiting his result. Not a blocker |
| **Known #1095 gaps — documented, not closed** | See §5 G9. None is in behaviour |
| PR #1091 (rapidgzip) | Open, not ours, untouched |

---

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| **Converter, not direct `.pat` emission** | The reporter's goal is comparable 5-Base and EM-seq arms. A converted BAM enters at the **front** of his path so both arms pass the same filters; `.pat` injects past every filter. Also avoids owning wgbs_tools' per-genome CpG index |
| **A flag on the aligner, not a subcommand** | Mirrors `--five_base_consensus_from_bam`; no new permanent subcommand + classic alias for a niche shim |
| **§3.6 masking keys on the reconstructed reference, not `QUAL` + `@PG`** | Rev 1 reached outside the record for data it already held, and got the units wrong doing it. The replacement is provably exactly the leak set and provably empty without masking — so the idempotence gate covers it. Deleted a CLI flag, a header parser, a conflict rule and a fail-loud fallback. **The feature has no tunable behaviour at all** |
| **Reuse `output::make_mismatch_string` + `hemming_dist`** | Matching `rebuild_md_with_deletions` byte-for-byte otherwise is real work for no gain. Validated by an independent oracle (G6) |
| **Refused runs remove their output** (review H1) | `BamWriter`'s `Drop` writes the BGZF EOF marker, so a leftover passed `samtools quickcheck` while four places claimed refusal. Contract now matches the text |
| **`samtools_available()` panics under `$CI`** (review H2/H3) | A silent skip left the load-bearing gate green in CI while asserting nothing. Mirrors the existing minimap2 guard |
| **Documented the unmapped-pass-through test gap rather than faking it** | No tracked fixture has unmapped records **and** `MD`. Behaviour was verified in review by crafting such a record; a broken or misleading test would be worse than a recorded gap |
| **Closed #1081/#1092 manually** | Felix's call, against earlier advice to let the release merge do it |

---

## 4. Files modified

| Path | |
|---|---|
| `rust/bismark/src/aligner/five_base_bisulfite.rs` | **New** — the per-record algorithm + 25 unit tests |
| `rust/bismark/src/aligner/{mod.rs, cli.rs}` | The flag, mutual exclusion, driver, H1 cleanup |
| `rust/bismark/tests/aligner_five_base_bisulfite.rs` | **New** — 7 integration gates |
| `rust/bismark/tests/data/five_base_bisulfite/` | **New** — oracle-validated soft-clip fixture (20K) |
| `.github/workflows/rust_ci.yml` | samtools added to the `test` job |
| `CHANGELOG.md`, `rust/README.md`, `docs/…/rust/illumina-5-base.md` | Entry, Milestones line, "Interop" section |
| `plans/08062026_five-base-bisulfite-bam/` | PLAN (rev 2 + §10b notes), PROGRESS, 3 plan reviews, 2 code reviews, COVERAGE, the `patter` experiment, 3 reply drafts, the upstream PR draft |
| `SESSION_HANDOFF.md` | This document |

**Outside the repo:** `gh` scopes now include `project`; 4 comments on #787; #1095 created + board fields; #1081/#1092 closed.

---

## 5. Gotchas and constraints

### #1095 — the design facts (verified repeatedly; do not re-derive)

**G1 — the three load-bearing properties.** Confirmed by four independent reviews *and* empirically on the fixture: (1) `len(XM) == len(SEQ)` with `XM[i]` ↔ `SEQ[i]` in BAM space, `XM` reversed in lockstep with `SEQ` (SE `output.rs:443-450`/`:463-467`, **PE `:671-675`/`:694-698`**); (2) `XG:Z:CT` ⇒ ref base `C`, pair `(C,T)`, `GA` ⇒ `G`, `(G,A)` — ⚠️ **emergent**, since `methylation_call` branches on **`XR`** (`methylation.rs:575`); (3) `I`/`S` pad the genomic window with `b'X'` without advancing the reference cursor, so `XM` is structurally `'.'` at every clipped/inserted position.

**G2 — 🔑 never use `iter_aligned()` here.** 5'-oriented positions + skips `I`/`S`; writing at `read_pos_5p` silently reverses every OB read's edits. The fixture's `9S81M`/`81M9S` asymmetric pair is the guard.

**G3 — `NM` includes insertions AND soft-clipped bases** (`hemming_dist` counts the `b'X'` padding — its doc says *"intentionally counted"*), plus deleted bases. Proven on the fixture: `ot_softclip` `20 = 11 + 0 + 9 + 0`. An earlier plan revision stated this backwards.

**G4 — the §3.6 masking rule.** Mask `b'N'` iff `XM[i] == '.'` **and** `ref_seq[i] == ref_base` **and** `SEQ[i] ∈ {meth, unmeth}`. Provably empty without masking, because at a genomic `C` the CT branch emits a letter for `C` **or** `T` with no further guard (`methylation.rs:587-596`). The PE call site of the leak is `mod.rs:1695-1697`, **not** the SE helper (whose only caller passes `baseq = 0`).

**G5 — `reconstruct_ref` is triply load-bearing:** it feeds `NM`, `MD` **and** the masking set. Guarded by the per-record round-trip proof (reconstruct → assert it reproduces `NM_old`/`MD_old` → only then emit).

**G6 — the deletion path is oracle-validated.** `rebuild_md_with_deletions` (verbatim Perl port, own comments read *"Perl dies — unreachable"*) agrees with an **independent** from-genome oracle on `3S5M2D4M1I4M1D3M` → `5^GA4G1A1^T0G1A0`, `NM 11`. Committed as `oracle_two_deletions_soft_clip_and_insertion`.

**G7 — refused runs must leave no output.** `BamWriter`'s `Drop` writes the BGZF EOF marker (`io/write.rs:36-40`), so any leftover passes `samtools quickcheck` and reads as a finished conversion. The fallible section is wrapped so every error path (including mid-stream per-record failure) removes the output *and* the report. **Do not unwrap that structure.**

**G8 — the gates must not be able to skip.** `samtools_available()` panics when `$CI` is set, and CI installs samtools. Before this, three gates — including the load-bearing idempotence gate — reported green in CI while asserting nothing.

**G9 — remaining #1095 gaps, all in validation:**
- **Unmapped/secondary pass-through has no committed test.** Needs a BAM with unmapped records **and** `MD`; no tracked fixture has both (`filter_nonconversion/se_unmapped` has no `MD`; the Trim Galore uBAMs are untracked and tagless). Behaviour verified in review. Closing it means adding an unmapped record to the `five_base_bisulfite` fixture, which invalidates the record/call counts other tests assert.
- **§9.8 `XG` ⟺ FLAG assertion** over `nondir_pe_1030.bam` — verified by hand and in `PLAN_REVIEW_B` §1.3, and invisible to the idempotence gate. Cheap; worth adding.
- **Committed tests are single-end and cover 2 of 4 strand indices** on a PE-only feature. Reviewer B confirmed byte-identity over `nondir_pe_1030.bam` (all four) and `synth_barcode_10k…_pe.bam` (12974 records) — so adding those gates codifies a verified property.
- **A lower-case `MD` base** silently misses a mask (`parse_md` accepts any ASCII letter; `ref_base` is upper-case). Only reachable via a non-Bismark BAM (`samtools calmd`).
- Fixture letter census is `Z`/`x`/`h` only; `z`/`X`/`H`/`U`/`u` are covered by unit tests, not by the fixture.

**G10 — `bam2pat` requires coordinate-sorted AND indexed input** (`is_bam_sorted` **skips the BAM entirely** on a non-`coordinate` `@HD`, and Bismark writes `SO:unsorted`), so the sort/index note is mandatory. Chromosome naming must match `wgbstools init_genome`; a total mismatch fails early in `set_regions`, a **partial** one silently drops contigs — the leading suspect for the reporter's 80 %.

**G11 — noodles owns the `@PG` chain** and re-links a later program's `PP` regardless of what we set, so the output's chain stays internally consistent but misstates running order. Metadata only. Don't "fix" it by dropping the `@PG`.

**G12 — `MM`/`ML` tags are ruled out non-obviously:** `patter` auto-detects them but throws `"Unrecognized bam format: paired end and nanopore"`, and 5-Base is PE-only.

### wgbs_tools / experiment mechanics

**G13 — 🔑 `wgbs_tools`' `setup.py` does NOT fail on a compile error** — the `raise` is commented out (`setup.py:33`). A broken module prints a red `FAIL`, the loop continues, the process exits 0, and the **old binary stays**. This was the reporter's whole problem. Use `python3 setup.py -t <target> -v` and `cmp` the binaries.

**G14 — the `patter` swap is CONFIRMED** (`EXPERIMENT_patter_swap.md`): `OT{'C','T',0,0}`→`{'T','C',0,0}`, `OB{'G','A',1,1}`→`{'A','G',1,1}` takes 0.0 %→100.0 % against a 100 % ground truth, **each strand independently** (OT 68 calls, OB 84). Now also confirmed on the reporter's real data.

**G15 — running `patter` standalone**, no `wgbstools init_genome` needed: build the CpG dict as `chrom\t<1-based locus>\t<index>`, `bgzip`, `tabix -s 1 -b 2 -e 2`; PE needs `match_maker`; `samtools view -q 10 -F 1796 -f 3 <bam> | match_maker | patter <dict> <region> --min_cpg 1 --clip 0`; `.pat` alphabet is `METH='C'`, `UNMETH='T'`; strand selectors are OT `{99,147}` / OB `{83,163}`.

**G16 — `--illumina_5base` needs a minimap2 index to exist** even though it aligns to the *unconverted* genome: `bismark_genome_preparation --minimap2 <dir>`. The error names `BS_CT.mmi` and is misleading about why.

**G17 — 🔑 mate ≠ strand when generating PE reads.** R1/R2 are two ends of **one** converted molecule (R2 = revcomp of the strand R1 came from); OT/OB are two **different** molecules. Getting this wrong produces reads Bismark correctly refuses to call (`XM = '.'`), which looks like a Bismark bug. Cost one wrong experiment.

**G18 — `bismark --output_dir <dir>` requires the directory to exist.**

### Process and tooling

**G19 — 🔑 the recurring failure mode this session: vacuous verification.** Six instances, every one caught only by *running* something: a plan gate naming unaligned uBAMs as its fixtures; a forward test asserting via `extract_calls`, which the converter cannot affect; an `NM` invariant stated backwards; an assertion using `!exists() || err.contains(..)`, which short-circuits past the property it names; three CI gates that skip silently; and a shell check printing "H1 FIXED" while `exit=127` meant the binary never ran. **A check whose failure you have never observed is not yet a check.**

**G20 — do not take agent reviewers at face value.** A reviewer's CIGAR census was wrong about indel coverage; another called a vacuous test "sound"; a code reviewer's oracle expected `NM 13` where the correct answer for the re-derived input was `11`. Conversely, reviewers found the two things nobody else did (the `--five_base_baseq` leak; H1). **Verify contested claims against source; never pick a reviewer.**

**G21 — three `gh` reporting traps.** `gh api` never paginates by default and a truncated page is indistinguishable from an absent record (this produced a false negative on a cross-reference). `gh project item-list --format json` omits keys for unset fields. A stale single-select option ID is a **silent no-match** — verify by read-back.

**G22 — `gh project` needs the `project` scope and an interactive TTY to grant it** (`gh auth refresh` backgrounded fails with `context deadline exceeded`). Quote `gh api` URLs containing `?` — zsh globs it. `$TMPDIR` differs between sandboxed and `dangerouslyDisableSandbox` calls. `gh`/`git fetch,push`/`curl` need the sandbox flag. A `Bash` call containing `rm -rf` was denied by the permission layer.

**G23 — `git add -A plans/` over-stages.** Dozens of unrelated pre-existing untracked plan files from other features live under `plans/`; stage this feature's paths explicitly.

**G24 — merging to `dev` does not close linked issues** (only the default branch does).

### Carried forward

**G25 — 🔑 the feature-build gate.** `.github/workflows/rust_ci.yml:15` sets `RUSTFLAGS: "-D warnings"` at **workflow** scope, so a warning in `#[cfg(feature = "rammap-inprocess")]` code fails CI while both obvious local gates stay green:
```
RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings
```

**G26 — `cargo fmt --check` is its own CI job**, and `cargo fmt` will reformat error attributes and closure params. Run `cargo fmt -p bismark -- --check` before pushing.

**G27 — `gh pr merge --delete-branch` aborts its LOCAL step if the tree is dirty**, printing `failed to run git`, which reads like the merge failed. Check `git status` first; then `gh pr view --json state` before retrying.

**G28 — squash-merge hides merge status.** Verify with `git diff <commit> origin/dev` (expect 0 lines) + `gh pr list --head <branch> --state merged`. Never infer from ancestry.
