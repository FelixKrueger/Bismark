# Session Handoff — 2026-08-06

**Next session's job:** implementation of [#1095](https://github.com/FelixKrueger/Bismark/issues/1095) is planned, dual-reviewed and ready — **but two gates come first, and neither is the implementation trigger:**

1. **Generate a soft-clip fixture.** No fixture in this repo has a single `S` CIGAR op (verified: 0 of 12974 and 0 of 20), yet `8S`-prefixed reads are the dominant real 5-Base shape and exactly where the corrected `NM` bites. Needs a live minimap2/Perl oracle run — see G8.
2. **Check [#787](https://github.com/FelixKrueger/Bismark/issues/787) for @Danielsm8's result** from the `patter` two-constant patch. That is real-data evidence on the *sign*, and it gates the upstream PR (not the converter).

`dev` is at **`d0a86c5`**, clean, pushed. **No source changed this session** — nothing in `rust/` or the Perl scripts. `git diff fab3e27..HEAD` is plan files only.

---

## 1. What we accomplished

Five commits from `fab3e27`, all documentation and planning.

| Commit | What |
|---|---|
| `ffa1176` | Session handoff (previous session's work) |
| `5bf8b55` | Handoff correction — #1095 unblocked; `patter` two-constant alternative recorded |
| `bfdf207` | Plan **rev 0** + confirmed inputs + 2 drafts |
| `5859d59` | Plan **rev 1** + `PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md` |
| `d0a86c5` | Plan **rev 2** + `PLAN_REVIEW_36.md` |

**GitHub side:**

| Artefact | State |
|---|---|
| [#787 comment `5164331510`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5164331510) | Diagnosis + three questions. Felix's own text, verbatim |
| [#787 comment `5203512462`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5203512462) | The `patter` workaround he can run **today on his existing DRAGEN BAMs** |
| [#1095](https://github.com/FelixKrueger/Bismark/issues/1095) | OPEN. Full diagnosis, confirmed inputs, the `patter` alternative, `MM`/`ML` ruled out |
| Board (Projects V2 #1) | #1095 item `PVTI_lAHOAFmTrc4BYkHjzg1Ye4o` — `Todo` / `aligner` / `1 - Now` / `M`, all read-back verified |
| #1081, #1092 | **CLOSED** at Felix's instruction, each noting the fix is on `dev` but unreleased |

**Housekeeping:** 6 stray 0-byte `*_PE_report.txt` deleted from repo root; `reference_rust_rewrite_board.md` memory corrected (its "default `gh` token can write the board" claim was false and cost 3 calls).

**The plan itself** — `plans/08062026_five-base-bisulfite-bam/` — went rev 0 → 1 → 2 through a full dual review plus one targeted section review. **The algorithm never changed.** Both full reviewers independently verified it across all four SE strand indices and all eight PE mate/index combinations. Everything that needed fixing was scaffolding: fixtures, tag arithmetic, the reader, the forward test, and finally §3.6's mechanism.

---

## 2. What's still pending

| Item | State |
|---|---|
| **Soft-clip fixture** | **Blocks implementation.** Does not exist; must be generated (G8) |
| **@Danielsm8's `patter` result** | Awaited on #787. Gates the upstream PR only, not the converter |
| **Implementation of #1095** | Planned, reviewed, not started. Needs the explicit "implement" trigger |
| Upstream `nloyfer/wgbs_tools` PR | Drafted (`DRAFT_upstream_wgbs_tools_PR.md`), **not submitted**. Blocked on the result above + reading `patter`'s CLI plumbing |
| Release cut | `## Unreleased` has **four** aligner entries (#1079, #1080, #1081, #1092). All three version literals still `3.1.0`. Magnitude: **minor → 3.2.0** |
| Optional | One more targeted look at §3.6 before implementing — it is on its third attempt and the first two failed the same way (G7) |
| PR #1091 (rapidgzip) | Open, not ours, untouched |

---

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| **Standalone BAM converter, not direct `.pat` emission** | The decisive reason is scientific: the reporter's goal is that paired 5-Base and EM-seq arms stay comparable. A converted BAM enters at the **front** of his existing path so both arms pass the same filters; `.pat` injects 5-Base **past** every filter. Secondary: `.pat` is keyed on wgbs_tools' own per-genome CpG index (`locus2CpGIndex`), which throws on any mismatch |
| **A flag on the aligner, not a new subcommand** | Mirrors `--five_base_consensus_from_bam` (`mod.rs:167` → `:515`). Avoids a permanent new subcommand **plus** classic-name alias in `cli.rs:54-72` for a niche interop shim |
| **Convert all cytosine contexts incl. `U`/`u`** | `patter` gates on a CpG mask so it costs nothing; restricting to CpG means *adding* a filter; a half-converted file mixes two polarities silently. `U`/`u` is load-bearing, not tidiness — see G4 |
| **Reply before planning** | Not to confirm the diagnosis (confirmable from source, and confirmed). His original issue was about **DRAGEN** data, and the converter reads `XM` — so the answer decided whether we built this or a much larger genome-driven tool. He answered: happy to re-run through Bismark |
| **§3.6 rev 2: key on the reconstructed reference, not on `QUAL` + `@PG`** | Rev 1 reached *outside* the record — a header, a threshold, an encoding offset — for information the record already contained, and got the units wrong doing it. The replacement is provably exactly the leak set and provably empty without masking (G5), and it deleted a CLI flag, a header parser, a conflict rule, a fail-loud fallback, two test rows and one "remaining risk" |
| **Dropped the `--five_base_bisulfite_baseq` flag entirely** | It had no role left once §3.6 needed no threshold. The `masked` counter plus a `Note:` surface what happened more informatively than a value the user must remember. Rev 2 has **no tunable behaviour at all** |
| **Reuse `output::make_mismatch_string` + `hemming_dist`** rather than writing an `MD` emitter | Both `pub(crate)` in a sibling module; matching `rebuild_md_with_deletions` byte-for-byte otherwise is real work for no gain. Makes idempotence structural (G6) |
| **Closed #1081/#1092 manually** | Felix's call, against the previous handoff's advice to let the release merge close them. Each carries a comment saying the fix is on `dev` but not in a tagged release |
| **`Component=aligner`** despite four recent sibling issues being untagged | Those look like tagging drift; Component is set on 120/145 items and exists to stop the table going flat |

---

## 4. Files modified

**No source files. No `rust/` or Perl changes.** `git diff fab3e27..HEAD` = 8 plan/doc files, +1658.

| File | Change |
|---|---|
| `SESSION_HANDOFF.md` | This document (overwrote the 2026-08-03 version, then corrected in `5bf8b55`, now regenerated) |
| `plans/08062026_five-base-bisulfite-bam/PLAN.md` | 533 lines — rev 2 |
| `.../PROGRESS.md` | 47 lines |
| `.../PLAN_REVIEW_A.md`, `_B.md`, `_36.md` | 242 + 320 + 272 lines |
| `.../DRAFT_reply_787.md` | Posted; header records the comment URL |
| `.../DRAFT_upstream_wgbs_tools_PR.md` | **Unsubmitted**, carries its own blockers |
| `_PE_report.txt`, `1_`, `a_`, `a2_`, `B_`, `C_` | **Deleted** — 6 untracked 0-byte files |
| `~/.claude/projects/.../memory/reference_rust_rewrite_board.md` | Corrected token-scope claim |

**Outside the repo:**

| Change | Detail |
|---|---|
| `gh` auth scopes | Felix ran `gh auth refresh -s project --hostname github.com` in a real terminal. Scopes now include **`project`** (new, required for any board work) |
| GitHub | 2 comments on #787; #1095 created + 4 board fields; #1081/#1092 closed with comments |

---

## 5. Gotchas and constraints

### The #1095 design facts — verified, do not re-derive

**G1 — the three load-bearing properties.** Independently confirmed by two reviewers across all four SE indices and all eight PE mate/index combinations:
1. `len(XM) == len(SEQ)` (parity check in `io::record::from_noodles_record`, relied on at `io/record.rs:301`), and `XM` is reversed in lockstep with `SEQ` — SE `output.rs:443-450` + `:463-467`, **PE `output.rs:671-675` + `:694-698`** (PE is the path that matters; 5-Base is PE-only).
2. `XG:Z:CT` ⇒ reference base `C`, pair `(C,T)`; `XG:Z:GA` ⇒ `G`, `(G,A)`. ⚠️ **Emergent, not direct** — `methylation_call` branches on **`XR`** (`methylation.rs:575`), so this is invisible at any single line and reads as an error. The four-row derivation is in PLAN §3.1.2.
3. `I`/`S` pad the genomic window with `b'X'` without advancing `pos` (`methylation.rs:174-181` SE, `:349-354` PE), and `X` matches no read base ⇒ `XM == '.'` there, structurally. The window is never mis-framed.

**G2 — 🔑 do NOT use `iter_aligned()`.** It returns **5'-oriented** positions (`io/record.rs:245-249`) and skips `I`/`S` (`:295-296`), so it cannot address every `SEQ` byte, and writing at `read_pos_5p` silently reverses every OB read's edits. Standing warning at `extractor/call.rs:182-183`. G1.3 removes any reason to want reference positions. **Put a comment in the code**, or a refactor will "simplify" into the bug.

**G3 — `NM` includes insertions AND soft-clipped bases.** `hemming_dist` (`output.rs:150-157`) zips against `ref_seq`, which carries `b'X'` at every `I`/`S`; its doc comment says *"`X` padding bases mismatch — intentionally counted"*. Then `ext.indels` (deletions only) is added. The identity is `mismatches at M + inserted + soft-clipped + deleted`. Rev 1 of the plan stated this backwards; soft-clip inflation is a **large** term under `--five_base_umi_len 8`.

**G4 — `U`/`u` must be in the rewrite set.** `push_ct_context` maps an `X` in the *context* slots to `U`/`u` (`methylation.rs:641-649`), so a cytosine adjacent to a clip or insertion is unknown-context. Skip it and exactly those cytosines stay in 5-Base polarity while neighbours flip.

**G5 — 🔑 the §3.6 masking rule, and why it is provable.** `--five_base_baseq` masks to `N` in the *call* sequence only, leaving the raw 5-Base base in `SEQ` (`mod.rs:1295-1307`; **the PE call site is `mod.rs:1695-1697`**, not the SE helper whose only caller passes `0`). Those positions carry `XM == '.'`, and `patter` scores them **inverted**. The fix:

> mask `b'N'` iff `XM[i] == '.'` **and** `ref_seq[i] == ref_base` **and** `SEQ[i] ∈ {meth, unmeth}`

It is *provably empty when no masking was applied*: at a genomic `C` the CT branch emits a letter for `base == 'C'` **or** `'T'` with no further guard (`methylation.rs:587-596`), so `XM == '.'` there proves the call sequence differed from `SEQ`. GA symmetric. Consequence: the idempotence gate covers this path, because `masked` must be 0 on any bisulfite fixture.

**G6 — reuse `output::make_mismatch_string` + `hemming_dist`** (both `pub(crate)`, same module tree) with a per-record round-trip proof: reconstruct `ref_seq`, assert it reproduces `NM_old`/`MD_old`, *then* emit. Do not write a fresh `MD` emitter — `rebuild_md_with_deletions` (`output.rs:228-390`) is a verbatim Perl port whose own comments read *"Perl dies — unreachable"*.

**G7 — `reconstruct_ref` is doubly load-bearing.** It feeds `NM`/`MD` **and** identifies §3.6's masking set. A bug there silently changes which bases get masked. The round-trip proof (G6) is the guard.

**G8 — no soft-clip fixture exists.** 0 `S` ops in `dedup/synth_barcode_10k_..._pe.bam` (12974 records) and `dedup/nondir_pe_1030.bam` (20). Bowtie 2 end-to-end never clips, so no `bismark_bt2` fixture ever will — but minimap2 `-x sr` is the default 5-Base engine and `--five_base_umi_len` *depends* on clipping the UMI prefix. **Generate one from a live oracle** with a leading `S`, an `I` and a `D`, carrying authentic `MD`/`NM`.

**G9 — `test_files/*_from_TrimGalore.bam` are unaligned uBAMs.** 0 `@SQ` lines, no `XM`/`XR`/`XG`/`MD`/`NM`. Rev 0 of the plan named them as fixtures for its primary gate. Aligned fixtures that do work: `tests/data/dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` (42 indel records), `tests/data/dedup/nondir_pe_1030.bam` (**the only four-strand fixture**; also at `tests/data/extractor/`), `tests/data/filter_nonconversion/{se_default,pe_default,se_unmapped}/`.

**G10 — `crate::io::BamReader` silently drops unmapped reads.** `records()` is `filter_map(filter_unmapped_then_classify)` and drops `FLAG & 0x4` (`io/read.rs:268-274`, `:631-645`, documented at `:7-9`); `from_noodles_record` also requires `XR` and enforces `XM`/`SEQ` parity at the reader. For a pass-through converter use `noodles_bam::io::Reader` directly + the existing `BamWriter::write_raw_record` (`io/write.rs:86`).

**G11 — a BAM stores 0-based phred scores, not ASCII.** `single_end_sam_output` subtracts the offset when writing (`output.rs:437-440`); `mask_low_quality` compares ASCII because it runs at align time on FastQ bytes. Reusing an align-time quality comparison on BAM data is off by 33 or 64 — which in rev 1 would have masked every no-call position in the file.

**G12 — the appended `@PG` needs a distinct ID.** `ID:bismark-five-base-bisulfite`, `PP:Bismark`. A second `ID:Bismark` violates SAM's unique-ID rule. Note Bismark BAMs already carry several `@PG` lines (up to six observed), and `samtools merge` renames colliding IDs with an 8-hex suffix.

**G13 — a counter placed one scope too wide can fail loud on correct data.** Rev 1 put the flip counter at loop level, so §3.6's masks would have incremented `flipped`, pushing the rate above 1 and tripping the fail-loud on **every correctly-masked run**. `flipped` is a letter-position statistic, `masked` a gap-position statistic; disjoint by construction.

**G14 — `MM`/`ML` modification tags are ruled out, non-obviously.** `patter` auto-detects them (`patter.cpp:334-338`) and has a modification-aware path — which would avoid falsifying `SEQ` entirely — but `patter.cpp:341-343` throws `"Unrecognized bam format: paired end and nanopore"`, and 5-Base is PE-only. Do not re-propose without checking whether upstream lifted it.

**G15 — chromosome naming, accurately.** A mismatch does **not** surface as `locus2CpGIndex` throwing — unreachable, since `conv`/`dict` come from the same `tabix` region. `set_regions` (`bam2pat.py:49-80`) intersects `samtools idxstats` names against the wgbstools genome's and raises **immediately** on an empty intersection. The silent hazard is a **partial** intersection quietly dropping contigs (scaffolds, alt/decoy, `chrM` vs `chrMT`).

**G16 — `bam2pat` requires coordinate-sorted AND indexed input.** `is_bam_sorted` (`bam2pat.py:222-240`) **skips the BAM entirely** on a non-`coordinate` `@HD`, and `generate_sam_header` writes `SO:unsorted` (`output.rs:109-111`). So the closing `samtools sort` + `index` note is **mandatory**, not advisory.

**G17 — the `patter` two-constant swap** (offered to the reporter). `ref_chr`/`unmeth_seq_chr` are read in exactly two places (`patter.cpp:153`, `:159`), so `OT{'C','T',0,0}` → `{'T','C',0,0}` and `OB{'G','A',1,1}` → `{'A','G',1,1}` inverts the call polarity completely. It survives `is_cpg`, which accepts `C|T` / `G|A` and whose required context bases are guanines on the read's own strand and so never converted. **Works on any 5-Base BAM including DRAGEN's.** `patter` is not entirely `XM`-blind either — `is_pass_ds_test` (`patter_utils.cpp:350-398`) parses `XM:Z:` for `--ds_test`.

### Process and tooling

**G18 — do not take agent reviewers at face value.** Reviewer B's CIGAR census claimed all fixtures were pure-`M`; they contain 42 indel records. Reviewer B also called the forward test "sound" when it was vacuous. Both were resolved by running one command. Conversely B found the one real design hole neither I nor A saw. **Check contested claims against source rather than picking a reviewer.**

**G19 — three `gh` reporting traps.**
- **`gh api` never paginates by default**, and a truncated page is indistinguishable from an absent record. Querying #787's `/timeline` (~77 events) without `--paginate` produced a **false negative** on a cross-reference that was present.
- **`gh project item-list --format json` omits keys for unset fields** — `it.get('component','')` returns `''` for every item, which reads as "field broken" rather than "unset here".
- **A stale single-select option ID is a silent no-match**, not an error: `item-edit` exits 0 and changes nothing. **Always verify by read-back.**

**G20 — `gh project` needs the `project` scope, and granting it needs a real TTY.** `gh auth refresh` runs a device-code flow; **backgrounding it fails** with `context deadline exceeded` after printing the code. Run it in a normal terminal. `read:project` is read-only — you need `project` to *set* fields.

**G21 — quote `gh api` URLs containing `?`.** zsh glob-expands it and aborts before `gh` runs (`no matches found`), which looks like an API failure.

**G22 — `$TMPDIR` differs between sandboxed and `dangerouslyDisableSandbox` Bash calls.** A file `curl`'d into `$TMPDIR` unsandboxed was unreachable via `$TMPDIR` from the next sandboxed call. Use the session scratchpad path explicitly.

**G23 — `gh`, `git fetch/push`, `curl` need `dangerouslyDisableSandbox: true`.** A blocked fetch does not fail the next checkout — it silently uses a stale ref.

**G24 — 0-byte `*_PE_report.txt` means an aborted run, not an empty result.** The report sink opens before alignment starts (`mod.rs:1400-1409`). File presence is not a "did it finish?" signal; content is.

**G25 — merging to `dev` does not close linked issues** (only the default branch does). #1081/#1092 were closed manually this session, so the previous handoff's "leave them open" guidance no longer applies to those two — but it still applies to anything merged to `dev` from here.

### Carried forward — will matter the moment implementation starts

**G26 — 🔑 the feature-build gate.** `.github/workflows/rust_ci.yml:15` sets `RUSTFLAGS: "-D warnings"` at **workflow** scope, so a warning in `#[cfg(feature = "rammap-inprocess")]` code fails CI while both obvious local gates stay green:
```
RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings
```

**G27 — `cargo fmt --check` is its own CI job.** Run `cargo fmt -p bismark -- --check` before pushing.

**G28 — `gh pr merge --delete-branch` aborts its LOCAL step if the tree is dirty**, printing `failed to run git`, which reads like the merge failed. Check `git status` is clean *before* merging; after any such error check `gh pr view --json state` before retrying.

**G29 — squash-merge hides merge status.** `git branch -d` says "not fully merged". Verify with `git diff <your-commit> origin/dev` (expect 0 lines) plus `gh pr list --head <branch> --state merged`, then `-D`. Never infer from ancestry.
