# Session Handoff — 2026-08-05

**Next session's job:** write the plan for [#1095](https://github.com/FelixKrueger/Bismark/issues/1095). **It is UNBLOCKED** — @Danielsm8 answered on 2026-08-03 ([comment `5167178704`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5167178704)): his existing BAMs are from DRAGEN but he is **happy to re-run through Bismark**, so the XM-driven converter specced in #1095 is the right build and no genome-driven variant is needed. He uses `bam2pat` defaults plus `--clip`, hg38 via `wgbstools init_genome hg38`, paired-end, and does not need variant deconvolution at his 10X coverage.

> ⚠️ An earlier revision of this document claimed the plan was "blocked on his answer" for three days after he had already answered. #1095's body has been corrected too. Check the issue thread before trusting a "blocked" claim in any handoff.

`dev` is at **`fab3e27`** — unchanged. **No commits, no source edits, zero diff this session.** All output was GitHub-side.

---

## 1. What we accomplished

No code. The session's product is a decision, a posted reply, and a specced tracking issue.

| Artefact | State |
|---|---|
| Reply to @Danielsm8 | **Posted** — [#787 comment `5164331510`](https://github.com/FelixKrueger/Bismark/issues/787#issuecomment-5164331510). Felix's own text, verbatim |
| Tracking issue | **[#1095](https://github.com/FelixKrueger/Bismark/issues/1095) OPEN** — full diagnosis, 3 verified properties, 3 implementation traps, consumer requirements, 3 decisions, caveats, blocked-on |
| Board (Projects V2 #1) | #1095 added, item `PVTI_lAHOAFmTrc4BYkHjzg1Ye4o`; `Status=Todo`, `Component=aligner`, `Phase=1 - Now`, `Size=M` — all verified by read-back |
| Housekeeping | 6 stray 0-byte `*_PE_report.txt` deleted from repo root (untracked) |
| Memory | `reference_rust_rewrite_board.md` corrected — its "default `gh` token can write the board" claim was **false** |

**The substantive win:** the previous handoff's §5 diagnosis was verified against wgbs_tools' actual source (not just re-read), and the design came out **simpler than §5 assumed** — genome-free, CIGAR-free, and `NM`/`MD` exactly recomputable rather than going stale. See §5 G1–G4.

---

## 2. What's still pending

| Item | State |
|---|---|
| **#1095 plan** | **Unblocked, not yet written.** Design confirmed correct by the reporter's answer; write it, then manual review before any implementation |
| **Reply to @Danielsm8** | Owed. He answered on 2026-08-03 and has had no response since. Offer him the `patter` two-constant workaround (G17) — it works on the DRAGEN BAMs he already has |
| **Upstream `nloyfer/wgbs_tools` PR** | Not started. A `--five_base`/inverted-polarity flag on `patter` (G17). Their tool, so needs their buy-in + real-data validation |
| **#1081 / #1092** | **CLOSED** 2026-08-06 at Felix's instruction, each with a note that the fix is on `dev` but not in a tagged release. Supersedes the old "leave them open" guidance in G10 |
| Release cut | `## Unreleased` in `CHANGELOG.md` has **four** aligner entries (#1079, #1080, #1081, #1092). All three version literals still `3.1.0`. Magnitude: **minor → 3.2.0** |
| Real-data rammap concordance | Optional. `RAMMAP_PRESET=sr` on the env-gated crosscheck with oxy data |
| PR #1091 (rapidgzip) | Open, not ours, untouched |
| Cosmetic | The #787 comment body contains exactly **one** `"` — the quote opened before *"You're right"* never closes. Left as-is deliberately (Felix's text); one-click edit if wanted |

---

## 3. Key decisions (with rationale)

| Decision | Rationale |
|---|---|
| **Standalone BAM converter, NOT direct `.pat` emission** | The decisive reason is scientific, not technical: Mike's goal is that paired 5-Base and EM-seq arms stay comparable. A converted BAM enters at the **front** of his existing path so both arms pass the same filters; `.pat` injects 5-Base **past** every filter, defeating the comparison. Secondary: `.pat` is keyed on wgbs_tools' own per-genome CpG index (`locus2CpGIndex`, `patter.cpp:79-91`), which throws `std::logic_error("Reference Error. Unknown CpG locus")` on any mismatch — we'd own a coordinate system we don't control, plus reimplement their pairing/`clip_size`/`min_cpg`/filters/pat-collapse |
| **A flag on the aligner (`--five_base_bisulfite_bam <BAM>`), not a new subcommand** | Mirrors `--five_base_consensus_from_bam` exactly (`aligner/mod.rs:167` → `:515`). Keeps all `--five_base_*` surface in one place and avoids committing to a permanent new subcommand **plus** classic-name alias in `cli.rs:54-72` for a niche interop shim |
| **Convert ALL cytosine contexts, incl. `U`/`u`** | `patter` gates on a CpG mask so all-contexts costs it nothing; restricting to CpG means *adding* a filter; a half-converted file mixes two polarities silently for every other reader. `U`/`u` specifically is **load-bearing**, not tidiness — see G3 |
| **Reply to Mike BEFORE planning** | Not to confirm the diagnosis (that was confirmable from source, and was confirmed). The real reason: his original issue was about **DRAGEN**-processed data. The converter reads `XM`, so it needs a `bismark --illumina_5base` BAM. If he needs DRAGEN BAMs there is no `XM` to re-encode and it becomes a different, larger tool |
| **Posted Felix's text verbatim**, including the unclosed quote | Authorship. Silently "fixing" punctuation in correspondence to a third party alters his words; flagged it instead |
| **`Component=aligner`** despite #1092/#1088/#1083/#1081 all being untagged | Those four look like tagging drift, not a decision — Component is set on 120/145 items and exists to stop the table going flat |
| **`Phase`/`Size` initially left unset, then set on request** | 118/145 and 119/145 respectively are unset, and all four comparable recent aligner issues are unset, so populating them would have invented a convention. Felix overrode → `1 - Now` / `M`, which is right for a different reason: it keeps the item visible in `-status:Done` views, and `M` reflects the surrounding spike→plan→dual-review→implement→dual-review→coverage cycle rather than the small logic |

---

## 4. Files modified

**No source files. No commits.** `git diff fab3e27..HEAD` is empty.

| File | Change |
|---|---|
| `SESSION_HANDOFF.md` | Overwritten with this document (previous version was 2026-08-03; its "next session's job" is now actioned) |
| `_PE_report.txt`, `1_`, `a_`, `a2_`, `B_`, `C_PE_report.txt` | **Deleted** — 6 untracked 0-byte files at repo root |
| `~/.claude/projects/-Users-fkrueger-Github-Bismark/memory/reference_rust_rewrite_board.md` | Corrected: added a ⚠️ token-scope block; the old "board writes worked with the default `gh` token (PROJECT_TOKEN NOT needed)" note was stale and cost 3 wasted calls |

**Outside the repo:**

| Change | Detail |
|---|---|
| `gh` auth scopes | Felix ran `gh auth refresh -s project --hostname github.com` in a real terminal. Scopes now `admin:public_key, gist, project, read:org, repo, workflow` — `project` is **new** and required for any board work |
| GitHub | 1 comment on #787; issue #1095 created; 4 board field writes |

Scratchpad (session-local, will be wiped): `787_reply_draft.md`, `787_comment_final.md`, `tracking_issue.md`, plus fetched `patter.cpp`, `patter.h`, `bam2pat.py`.

---

## 5. Gotchas and constraints

### The #1095 design facts — verified, do not re-derive

**G1 — the three properties the converter rests on.** All verified in-tree this session:
1. **`len(XM) == len(SEQ)`** is an enforced invariant (parity check in `io::record::from_noodles_record`, relied on at `io/record.rs:301`), and `XM` is reversed in lockstep with `SEQ` for `-`-strand records (`aligner/output.rs:463-467`). So `XM[i]` ↔ `SEQ[i]` positionally **in BAM space**.
2. **The reference base is knowable from `XG` alone.** Across all four strand indices (`aligner/methylation.rs:131-135`), `methylation_call` emits a letter only where the genomic base is `C` (CT branch) or `G` (GA branch), and `single_end_sam_output` revcomps `SEQ` and `ref_seq` **together** for `-` strand. Hence `XG:Z:CT` ⇒ reference base `C` at every letter position; `XG:Z:GA` ⇒ `G`. This is exactly `patter`'s `ref_chr` (`patter.h:59-60`: `OT{'C','T',0,0}`, `OB{'G','A',1,1}`).
3. **Soft clips and insertions are structurally `.`** — `I`/`S` pad the genomic window with `b'X'` **without advancing `pos`** (`aligner/methylation.rs:174-181` SE, `:349-352` PE `walk_mate`; Perl 4346/4360). `X` matches no read base (reads are upper-cased `ACGTN`), so `methylation_call` falls to the `else` arm → `.`. **The window is therefore never mis-framed**, and a positional zip provably cannot touch a clipped or inserted base.

**G2 — 🔑 DO NOT use `iter_aligned()` in the converter.** G1.3 removes any need for reference positions, and `iter_aligned()` returns **5'-oriented** positions — writing into `SEQ` at `read_pos_5p` silently reverses every OB read's edits. The existing warning is at `extractor/call.rs:183`. It also exposes no BAM-space index, so reaching for it leads straight to either a wrong write or a pointless new accessor.

**G3 — `U`/`u` must be in the rewrite set.** `push_ct_context` maps an `X` in the *context* slots (i+1, i+2) to `U`/`u`, so a cytosine immediately before a soft clip or insertion is classified unknown-context, not CpG/CHG/CHH. Skip `U`/`u` and exactly those cytosines stay in 5-Base polarity while their neighbours flip.

**G4 — `NM`/`MD` do NOT go stale.** The 2026-08-03 handoff listed this as unavoidable; it is not. Given G1.2, `ΔNM = (new != ref) − (old != ref)` is exact, and `MD` + original `SEQ` reconstruct the reference window so `MD` can be re-derived exactly — still **no genome**. Bismark always writes both (`aligner/output.rs:485-487`); fail loud if `MD` is absent rather than guess.

**G5 — `five_base_emit_record` is not the production per-read path.** Its only caller (`aligner/mod.rs:2298`) is the **duplex-consensus** path, which synthesises `{len}M` CIGARs (`:2289`) and therefore never sees soft clips. Production per-read 5-Base is **PE** (`run_pe_five_base`); verify soft-clip behaviour against `walk_mate`, not the SE emit.

**G6 — consumer requirements (from `bam2pat.py`).** Coordinate-sorted **and** indexed (`is_bam_sorted`), `-q 10` (`MAPQ = 10`), `-F 1796` (`FLAGS_FILTER`), `-f 3` for PE. FLAG vocabulary `{99,147}`/`{83,163}` (PE) or `{0}`/`{16}` (SE); `is_bottom` (`patter_utils.cpp:163-168`) agrees with Bismark's `XG` on every one. **`samtools sort` + `index` is the only gap.**

**G7 — the converter is lossy at cytosines.** It replaces observed bases with called ones. 5-Base cannot separate a genuine `C>T` from 5mC without the opposite strand, so real variants get written as methylation — precisely the sites `--five_base_deconvolution` exists to find. Harmless for UXM (fragment-level CpG patterns); **the converted file must never be used for variant calling, and never be the primary BAM.**

**G17 — 🔑 `patter` can be made natively 5-Base-correct by swapping two constants.** `ref_chr`/`unmeth_seq_chr` are read in **exactly two places** (`patter.cpp:153`, `:159`) and nowhere else, so `OT{'C','T',0,0}` → `OT{'T','C',0,0}` and `OB{'G','A',1,1}` → `OB{'A','G',1,1}` inverts the call polarity completely. It survives `is_cpg()`, which accepts `C|T` (OT) / `G|A` (OB) and whose required context bases (`seq[j+1]=='G'`, `seq[j-1]=='C'`) are guanines on the read's own strand in 5-Base chemistry and therefore never converted. **Works on any 5-Base BAM including DRAGEN's** — no re-alignment, no Bismark. Candidate upstream PR; does not replace #1095, which serves Bismark users without a patched third-party binary.

**G18 — `MM`/`ML` modification tags are ruled out, and it's not obvious why.** `patter` already auto-detects standard SAM `MM`/`ML` (`patter.cpp:334-338`) and has a modification-aware path — which would have avoided falsifying `SEQ` at all. But `patter.cpp:341-343` throws `"Unrecognized bam format: paired end and nanopore"`, and 5-Base is PE-only. Do not re-propose this without checking whether upstream has lifted that restriction.

**G19 — chromosome naming must match wgbstools.** The reporter's genome is `wgbstools init_genome hg38`, so the Bismark genome must use the same naming (`chr1`-style). `locus2CpGIndex` throws `std::logic_error` on an unknown locus — loud, but only at the end of a long run.

**G20 — `patter` is not entirely `XM`-blind.** Its *call* path never reads `XM`, but `is_pass_ds_test` (`patter_utils.cpp:357-362`) parses `XM:Z:` for the opt-in `--ds_test` filter. Does not affect the diagnosis; just don't repeat "never reads XM" as an absolute.

### Tooling / environment

**G8 — `gh project` needs the `project` scope, and granting it needs a REAL TTY.** The default token did not have it (`gh project field-list` → *"missing required scopes [read:project]"*). `gh auth refresh` runs a device-code flow: **backgrounding it fails** with `context deadline exceeded` after printing the code, because the browser round-trip never completes. It must be run in a normal terminal window. `read:project` is read-only — you need `project` to *set* fields.

**G9 — three `gh` reporting traps hit this session:**
- **`gh api` never paginates by default.** A truncated page is indistinguishable from an absent record. Querying `/timeline` on #787 (~77 events) without `--paginate` produced a **false negative** on the #1095 cross-reference. Any "X is missing" assertion needs `--paginate`.
- **`gh project item-list --format json` omits keys for unset fields.** `it.get('component','')` returns `''` for every item, which reads as "field broken" when it means "unset on these items". Dump one item's raw keys to tell them apart.
- **A stale single-select option ID is a silent no-match**, not an error — `item-edit` exits 0 and changes nothing. **Always verify by read-back.** (The 64-day-old cached Status/Component IDs happened to still be valid; that was luck.)

**G10 — merging to `dev` does not close linked issues.** #1081 and #1092 remain OPEN despite `Closes #…`; GitHub only auto-closes on merge into the **default** branch (`master`). They close on the next `dev`→`master` release merge. Do not close manually if a release is near.

**G11 — `$TMPDIR` differs between sandboxed and `dangerouslyDisableSandbox` Bash calls.** A file `curl`'d into `$TMPDIR` in an unsandboxed call was unreachable via `$TMPDIR` from the next sandboxed call. Use the session scratchpad path explicitly for anything that must survive across both.

**G12 — `gh`, `git fetch/push`, `curl` need `dangerouslyDisableSandbox: true`.** A blocked fetch does not fail the next checkout — it silently uses a stale ref.

**G13 — 0-byte `*_PE_report.txt` means an aborted run, not an empty result.** The report sink is opened via `derive_output_path` **before** alignment starts (`aligner/mod.rs:1400-1409`), so a crashed run leaves an empty report. File presence is not a "did it finish?" signal; content is.

### Carried forward — will matter if implementation starts

**G14 — 🔑 the feature-build gate.** `.github/workflows/rust_ci.yml:15` sets `RUSTFLAGS: "-D warnings"` at **workflow** scope, so a warning in `#[cfg(feature = "rammap-inprocess")]` code fails CI while both obvious local gates stay green. For anything touching feature-gated code:
```
RUSTFLAGS="-D warnings" cargo clippy -p bismark --all-targets --features rammap-inprocess -- -D warnings
```

**G15 — `cargo fmt --check` is its own CI job.** Run `cargo fmt -p bismark -- --check` before pushing.

**G16 — `gh pr merge --delete-branch` aborts its LOCAL step if the tree is dirty**, printing `failed to run git`, which reads like the merge failed. Check `git status` is clean *before* merging; after any such error check `gh pr view --json state` before retrying.
