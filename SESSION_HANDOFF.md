# Session Handoff — 2026-08-07

**Next session's job:** **implement [#1095](https://github.com/FelixKrueger/Bismark/issues/1095). There are no gates left.** The plan is at rev 2, dual-reviewed plus a targeted section review, and the fixture that was blocking it now exists and is oracle-validated. It needs only the explicit implementation trigger.

Read `plans/08062026_five-base-bisulfite-bam/PLAN.md` — §0's revision table first, then §3 (behaviour) and §9 (validation). Do **not** re-derive the design facts; §5 below and PLAN §3.1 record them, and several were verified three times.

`dev` is at **`18ff591`**, clean, pushed. **No source changed this session** — the only thing under `rust/` is test data (`tests/data/five_base_bisulfite/`).

---

## 1. What we accomplished

Eight commits from `fab3e27`; 16 files, +2101.

| Commit | What |
|---|---|
| `ffa1176` | Handoff (previous session's work) |
| `5bf8b55` | Handoff correction — #1095 unblocked; `patter` alternative recorded |
| `bfdf207` | Plan **rev 0** + confirmed inputs + 2 drafts |
| `5859d59` | Plan **rev 1** + `PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md` |
| `d0a86c5` | Plan **rev 2** + `PLAN_REVIEW_36.md` |
| `6cff75b` | Handoff regeneration |
| `0e4c705` | **Soft-clip + indel fixture**, oracle-validated — the last gate on implementation |
| `18ff591` | **`patter` swap experiment** — the swap is confirmed on both strands |

**GitHub:** three replies on [#787](https://github.com/FelixKrueger/Bismark/issues/787) (now 9 comments); [#1095](https://github.com/FelixKrueger/Bismark/issues/1095) opened, cross-linked, board fields set (`Todo`/`aligner`/`1 - Now`/`M`, all read-back verified); #1081 and #1092 closed with notes that the fix is on `dev` but unreleased. Housekeeping: 6 stray 0-byte `*_PE_report.txt` deleted; the `reference_rust_rewrite_board` memory corrected.

**Two things went from reasoned to measured, and both are the session's real output:**

1. **`NM` counts soft-clipped bases** — `ot_softclip` in the new fixture gives `NM=20 == 11 mismatches + 0 ins + 9 softclip + 0 del`. Rev 1 of the plan asserted the opposite.
2. **The `patter` two-constant swap works** — unpatched 0.0 % vs patched 100.0 % methylation against a 100 % ground truth, **both strands independently**. So the reporter's negative field result is a local problem, not a refutation.

---

## 2. What's still pending

| Item | State |
|---|---|
| **#1095 implementation** | **Ready. No gates.** Needs the explicit "implement" trigger |
| @Danielsm8's two answers | (a) Are his pre/post-patch `.pat` files byte-identical? (b) What does DRAGEN put in `SEQ`? Asked in [`5216492876`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5216492876). **Neither blocks #1095** — the converter reads `XM` |
| Upstream `nloyfer/wgbs_tools` PR | Drafted, **unsent**. Validation checks 1 and 4 **pass** (G17). Outstanding: **check 2** — EM-seq byte-identical with the patch present but not enabled, which is now the cheap one and the evidence a maintainer will care about most; **check 3** needs real paired data; plus reading `patter`'s CLI plumbing |
| Release cut | `## Unreleased` has four aligner entries (#1079, #1080, #1081, #1092). Version literals still `3.1.0`. Magnitude: **minor → 3.2.0** |
| PR #1091 (rapidgzip) | Open, not ours, untouched |

---

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| **Standalone BAM converter, not `.pat` emission** | The reporter's goal is that paired 5-Base and EM-seq arms stay comparable. A converted BAM enters at the **front** of his path so both arms pass the same filters; `.pat` injects 5-Base **past** every filter. Secondary: `.pat` is keyed on wgbs_tools' own per-genome CpG index, which throws on any mismatch |
| **A flag on the aligner, not a subcommand** | Mirrors `--five_base_consensus_from_bam` (`mod.rs:167` → `:515`); avoids a permanent subcommand + classic-name alias for a niche shim |
| **Convert all cytosine contexts incl. `U`/`u`** | `patter` gates on a CpG mask so it is free; restricting to CpG means *adding* a filter; `U`/`u` is load-bearing, not tidiness (G4) |
| **Reply before planning** | His original issue was about DRAGEN data and the converter reads `XM`, so his answer decided whether we built this or a far larger genome-driven tool. He will re-run through Bismark |
| **§3.6 rev 2: key on the reconstructed reference, not `QUAL` + `@PG`** | Rev 1 reached outside the record for information it already contained, and got the units wrong doing it (G11). The replacement is provably exactly the leak set and provably empty without masking (G5), and deleted a CLI flag, a header parser, a conflict rule, a fail-loud fallback and one "remaining risk". **Rev 2 has no tunable behaviour at all** |
| **Reuse `output::make_mismatch_string` + `hemming_dist`** | Both `pub(crate)` in a sibling module; matching `rebuild_md_with_deletions` byte-for-byte otherwise is real work for no gain (G6) |
| **Test the `patter` swap per strand, not just in aggregate** | A one-sided patch would still have shown a plausible ~50 % aggregate flip. Splitting by `bam2pat`'s own FLAG selectors gave 0→100 % on each side, which is what made the result conclusive (G17) |
| **Closed #1081/#1092 manually** | Felix's call, against the earlier advice to let the release merge close them. Each carries a note that the fix is unreleased |

---

## 4. Files modified

**No source. The only `rust/` change is test data.**

| Path | |
|---|---|
| `plans/08062026_five-base-bisulfite-bam/` | `PLAN.md` (rev 2), `PROGRESS.md`, 3 review reports, `EXPERIMENT_patter_swap.md`, 2 reply drafts (both posted), the upstream PR draft (unsent) |
| `rust/bismark/tests/data/five_base_bisulfite/` | BAM + `pUC19.fa` + reads + generator + report + README (20K) |
| `SESSION_HANDOFF.md` | This document (third regeneration this session) |
| 6 × `*_PE_report.txt` | **Deleted** — untracked 0-byte files |
| `~/.claude/projects/.../memory/reference_rust_rewrite_board.md` | Corrected token-scope claim |

**Outside the repo:** `gh` scopes now include **`project`** (Felix ran `gh auth refresh -s project --hostname github.com` in a real terminal); 3 comments on #787; #1095 created + 4 board fields; #1081/#1092 closed.

---

## 5. Gotchas and constraints

### #1095 design facts — verified, do not re-derive

**G1 — the three load-bearing properties.** Confirmed independently by two reviewers across all four SE indices and all eight PE mate/index combinations, **and now empirically on the new fixture**:
1. `len(XM) == len(SEQ)` (parity check in `io::record::from_noodles_record`, relied on at `io/record.rs:301`); `XM` reversed in lockstep with `SEQ` — SE `output.rs:443-450` + `:463-467`, **PE `output.rs:671-675` + `:694-698`** (PE is the path that matters).
2. `XG:Z:CT` ⇒ ref base `C`, pair `(C,T)`; `XG:Z:GA` ⇒ `G`, `(G,A)`. ⚠️ **Emergent, not direct** — `methylation_call` branches on **`XR`** (`methylation.rs:575`), so this is invisible at any single line. Four-row derivation in PLAN §3.1.2.
3. `I`/`S` pad the genomic window with `b'X'` without advancing `pos` (`methylation.rs:174-181` SE, `:349-354` PE) ⇒ `XM == '.'` there, structurally.

**G2 — 🔑 do NOT use `iter_aligned()`.** Returns **5'-oriented** positions (`io/record.rs:245-249`) and skips `I`/`S` (`:295-296`); writing at `read_pos_5p` silently reverses every OB read's edits. Warning at `extractor/call.rs:182-183`. **Put a comment in the code** or a refactor will "simplify" into the bug. The fixture's `ot_softclip` (`9S81M`) / `ob_softclip` (`81M9S`) pair is the guard.

**G3 — `NM` includes insertions AND soft-clipped bases.** `hemming_dist` (`output.rs:150-157`) zips against `ref_seq`, which carries `b'X'` at every `I`/`S`; doc comment at `:141-144` says *"intentionally counted"*. Then `ext.indels` (deletions only) is added. **Proven on the fixture.**

**G4 — `U`/`u` must be in the rewrite set.** `push_ct_context` maps an `X` context base to `U`/`u` (`methylation.rs:641-649`), so a cytosine adjacent to a clip or insertion is unknown-context.

**G5 — 🔑 the §3.6 masking rule.** `--five_base_baseq` masks to `N` in the *call* sequence only (`mod.rs:1295-1307`; **the PE call site is `mod.rs:1695-1697`**, not the SE helper whose only caller passes `0`), leaving the raw 5-Base base in `SEQ` where `patter` scores it **inverted**. Fix: mask `b'N'` iff `XM[i] == '.'` **and** `ref_seq[i] == ref_base` **and** `SEQ[i] ∈ {meth, unmeth}`. Provably empty without masking, because at a genomic `C` the CT branch emits a letter for `C` **or** `T` with no further guard (`methylation.rs:587-596`).

**G6 — reuse `output::make_mismatch_string` + `hemming_dist`** with a per-record round-trip proof (reconstruct `ref_seq`, assert it reproduces `NM_old`/`MD_old`, then emit).

**G7 — `reconstruct_ref` is doubly load-bearing** — it feeds `NM`/`MD` *and* identifies G5's masking set. The round-trip proof is the guard.

**G8 — the fixture exists: `rust/bismark/tests/data/five_base_bisulfite/`.** 8 SE records over pUC19, both `XG` values, leading **and** trailing soft clips, both indel kinds. **Byte-identical Rust vs live Perl v0.25.1** including MAPQ. Read its `README.md` — it documents the four invariants it confirms and the exact regeneration commands. `pUC19.fa` ships alongside so a test can build its reference independently of `MD`.

**G9 — `test_files/*_from_TrimGalore.bam` are unaligned uBAMs** (0 `@SQ`, no `XM`). Rev 0 named them as gate fixtures. Other aligned options: `tests/data/dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` (42 indel records), `tests/data/dedup/nondir_pe_1030.bam` (**the only four-strand fixture**; also under `tests/data/extractor/`), `tests/data/filter_nonconversion/{se_default,pe_default,se_unmapped}/`.

**G10 — `crate::io::BamReader` silently drops unmapped reads** (`io/read.rs:268-274`, documented at `:7-9`), and `from_noodles_record` requires `XR` and enforces parity at the reader. For a pass-through converter use `noodles_bam::io::Reader` + `BamWriter::write_raw_record` (`io/write.rs:86`).

**G11 — a BAM stores 0-based phred scores, not ASCII.** `single_end_sam_output` subtracts the offset when writing (`output.rs:437-440`) while `mask_low_quality` compares ASCII, because it runs at align time on FastQ bytes. Reusing an align-time quality comparison on BAM data is off by 33 or 64.

**G12 — the appended `@PG` needs a distinct ID** (`ID:bismark-five-base-bisulfite`, `PP:Bismark`). A second `ID:Bismark` violates SAM's unique-ID rule. Bismark BAMs already carry several `@PG` lines (up to six observed) and `samtools merge` renames collisions with an 8-hex suffix.

**G13 — a counter one scope too wide fails loud on correct data.** Rev 1 put the flip counter at loop level, so masks would have incremented `flipped` and tripped the fail-loud on every correctly-masked run. `flipped` is a letter-position statistic, `masked` a gap-position one; disjoint.

**G14 — `MM`/`ML` tags are ruled out non-obviously.** `patter` auto-detects them (`patter.cpp:334-338`) but `:341-343` throws `"Unrecognized bam format: paired end and nanopore"`, and 5-Base is PE-only.

**G15 — `bam2pat` requires coordinate-sorted AND indexed input.** `is_bam_sorted` (`bam2pat.py:222-240`) **skips the BAM entirely** on a non-`coordinate` `@HD`, and `generate_sam_header` writes `SO:unsorted`. The closing `samtools sort` + `index` note is **mandatory**. Chromosome naming must match `wgbstools init_genome`; a total mismatch fails early in `set_regions`, a **partial** one silently drops contigs.

### wgbs_tools / experiment mechanics (G17's method — reusable)

**G16 — 🔑 `wgbs_tools`' `setup.py` does NOT fail on a compile error.** The `raise RuntimeError` is **commented out** (`setup.py:33`): a broken module prints a red `FAIL`, the loop continues through ~15 others, the process exits 0, and **the previously-built binary stays in place**. This is the leading explanation for the reporter's "no difference" result. Use `python3 setup.py -t <target> -v` to build one target so nothing hides the error, and `cmp` the binaries to confirm a rebuild took.

**G17 — the `patter` swap is CONFIRMED.** `plans/.../EXPERIMENT_patter_swap.md`. Swapping `ReadOrient OT{'C','T',0,0}` → `{'T','C',0,0}` and `OB{'G','A',1,1}` → `{'A','G',1,1}` (`patter.h:59-60`) took unpatched 0.0 % → patched 100.0 % methylation against a 100 % ground truth, **each strand independently** (OT 68 calls, OB 84). Identical line and unknown counts between runs, so polarity changes and nothing else.

**G18 — how to run `patter` standalone**, without `wgbstools init_genome`:
- Build the CpG dict yourself: `chrom\t<1-based locus>\t<index>` per forward CpG C, `bgzip`, `tabix -s 1 -b 2 -e 2`. `load_genome_ref` only does `tabix <ref> <region> | cut -f2-3`.
- PE needs `match_maker` (also a `setup.py` target): `samtools view -q 10 -F 1796 -f 3 <bam> | match_maker | patter <dict> <region> --min_cpg 1 --clip 0`.
- `.pat` alphabet: `METH='C'`, `UNMETH='T'` (`patter_utils.h:65-66`). Usage is `patter CPG_DICT REGION [opts]`, SAM on **stdin**, no header.
- Strand selectors (`bam2pat`'s own): OT = FLAG {99,147}, OB = {83,163}.

**G19 — `--illumina_5base` requires a minimap2 index to exist** even though it aligns against the *unconverted* genome: build with `bismark_genome_preparation --minimap2 <dir>` (creates `BS_CT.mmi`/`BS_GA.mmi`). The error message names `BS_CT.mmi` and is misleading about why.

**G20 — 🔑 mate ≠ strand when generating PE reads.** R1 and R2 are two ends of **one** converted molecule, so R2 = reverse complement of the converted strand R1 came from. OT and OB are two **different** molecules. Building R2 as the opposite genomic strand converted independently produces reads Bismark correctly refuses to call (`XM = '.'`), which looks like a Bismark bug and is not. This cost one wrong experiment.

**G21 — `bismark --output_dir <dir>` requires the directory to exist**; it fails with `failed to open BAM ... No such file or directory`.

### Tooling

**G22 — do not take agent reviewers at face value.** Reviewer B's CIGAR census claimed all fixtures were pure-`M`; they contain 42 indel records. B also called the forward test "sound" when it was vacuous. Both were settled by one command. Conversely B found the one real design hole neither the author nor A saw. **Check contested claims against source; do not pick a reviewer.**

**G23 — three `gh` reporting traps.** `gh api` never paginates by default and a truncated page is indistinguishable from an absent record (this produced a **false negative** on a cross-reference). `gh project item-list --format json` omits keys for unset fields. A stale single-select option ID is a **silent no-match** — `item-edit` exits 0 and changes nothing, so **verify by read-back**.

**G24 — `gh project` needs the `project` scope, and granting it needs a real TTY.** `gh auth refresh` runs a device-code flow; **backgrounding it fails** with `context deadline exceeded`. `read:project` is read-only.

**G25 — quote `gh api` URLs containing `?`** — zsh glob-expands it and aborts before `gh` runs.

**G26 — `$TMPDIR` differs between sandboxed and `dangerouslyDisableSandbox` calls.** Use the session scratchpad path explicitly. And `gh`, `git fetch/push`, `curl` need `dangerouslyDisableSandbox: true`.

**G27 — a `Bash` call containing `rm -rf` was denied** by the permission layer. Split destructive steps out, or avoid recursive deletes in compound commands.

**G28 — 0-byte `*_PE_report.txt` means an aborted run, not an empty result** — the report sink opens before alignment starts (`mod.rs:1400-1409`).

**G29 — merging to `dev` does not close linked issues** (only the default branch does). #1081/#1092 were closed manually; the guidance still applies to anything merged from here.

### Carried forward — matters the moment implementation starts

**G30 — 🔑 the feature-build gate.** `.github/workflows/rust_ci.yml:15` sets `RUSTFLAGS: "-D warnings"` at **workflow** scope, so a warning in `#[cfg(feature = "rammap-inprocess")]` code fails CI while both obvious local gates stay green:
```
RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings
```

**G31 — `cargo fmt --check` is its own CI job.** Run `cargo fmt -p bismark -- --check` before pushing.

**G32 — `gh pr merge --delete-branch` aborts its LOCAL step if the tree is dirty**, printing `failed to run git`, which reads like the merge failed. Check `git status` first; after any such error check `gh pr view --json state` before retrying.

**G33 — squash-merge hides merge status.** Verify with `git diff <commit> origin/dev` (expect 0 lines) + `gh pr list --head <branch> --state merged`, then `-D`. Never infer from ancestry.
