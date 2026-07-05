# PLAN — Phase 4: minimap2 wrapper (clean-slate options + positional `.mmi` + `/1` retention) + byte-identity gate

> **Epic:** `06052026_bismark-aligner-v1x/EPIC.md`, Phase 4. **Depends on:** Phase 3 spike (✅ premise HOLDS — `phase3-minimap2-determinism-selection-spike/spikes/SPIKE_minimap2_determinism_selection.md`) + the shipped HISAT2 backend (2a/2b, `49a1518`).

- **Created:** 2026-06-05 · **rev 1** (dual plan-review folded; **scoped to SE — PE-minimap2 is NOT byte-identity-reachable**, see §0. Revision History at the end).

## 0. 🔴 SCOPE — SE only (PE-minimap2 is not byte-identity-reachable)
The dual plan-review (`PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md`) confirmed: **SE minimap2 IS byte-identity-reachable** (the wrapper + the merge-no-op thesis are verified), but **PE minimap2 has no trustworthy Perl oracle** —
- **C-1:** Perl `paired_end_…_minimap2` (6697-6708) is unfinished WIP: an uncommented `# TODO: Need to check this.` + `warn …; sleep(1)` **twice per read pair** (the only uncommented sleeps in the script; ~2s/pair + a stderr flood).
- **C-2:** the Perl PE report writer (1845-1850) has **no `$mm2` branch** → it mislabels minimap2 PE runs as **"HISAT2"** (the SE writer 1722-1728 does have a `$mm2` branch).

You cannot byte-match a reference that is WIP and mislabels its own report. **Therefore Phase 4 = SE minimap2 only.** PE-minimap2 is **deferred out of the faithful v1.x byte-identity scope** (Felix decision 2026-06-05 — documented known gap; Bowtie2 + HISAT2 PE cover paired-end; re-opening would require fixing the upstream Perl path). See §10 OQ-4c. This does NOT affect Bowtie 2/HISAT2 PE (both shipped + byte-identical). **The SE convert needs NO change** (review finding — see §2/§5).
- **Branch / worktree:** a **fresh branch off `origin/rust/iron-chancellor` (`49a1518`)** @ `~/Github/Bismark-aligner` (the local `rust/aligner-v1x` diverged at the #949 squash — branch fresh; `reset --hard`/force-push are deny-ruled). Crate `rust/bismark-aligner` (`bismark_rs`).
- **Oracle / pin:** Perl Bismark **v0.25.1** + **minimap2 2.31-r1302** (oxy `bismark-test`), samtools 1.23.1.

## 1. Goal
Add **minimap2** as a byte-identical alignment backend, completing the v1.x aligner set. The Phase-3 spike proved determinism + input-order output hold and that the feared both-strand-selection divergence is **moot** under Bismark's real options (`--secondary=no` ⇒ one forward primary per read). So Phase 4 is a **pure wrapper** — **the merge/scoring/MAPQ/XM/methylation core is reused UNCHANGED** — plus several invocation deltas that differ more from Bowtie 2 than HISAT2 did. **SE byte-identical only** (directional → non-dir/pbat); **PE-minimap2 is out of scope** (§0 — the Perl PE oracle is WIP). **Bowtie 2 + HISAT2 stay byte-frozen.**

## 2. Context — reused vs new (the seam), source-cited

**Reused UNCHANGED (verified against the spike + Perl):**
- **`merge.rs` — NO change.** minimap2 emits `AS:i:` + `--MD` but NOT `ZS`/`XS:i:`; Bismark's parse loop (Perl 2772-2796) has **no `s2:i:` branch**, so the within-instance 2nd-best is **always undef → backfilled to AS** (Perl 3467) — exactly the no-2nd-best path the Rust parser already produces (`second_best=None`). **🔴 The Phase-3 spike report's Q4/§4/§5 are WRONG on this** (B I-4): they say "the merge must read `s2:i:` for minimap2" — the OPPOSITE of the oracle. Bismark IGNORES `s2`. Do NOT add an `s2:i:` parse branch (it would silently break MAPQ byte-identity). The spike report has been corrected with a `[CORRECTION]` note; V6 feeds a real `s2:i:` tag and asserts `second_best==None`. The cross-instance 2/4-instance selection + index→strand mapping is identical (the spike saw 0 reverse-strand on the CT instance, so the commented-out `--norc`/`--nofw` has ~no effect).
- **`mapq.rs` — NO change.** MAPQ is `calc_mapq(len, _, AS_best, AS_secBest)` called **unconditionally** (Perl 3134 SE / 3876 PE) using the global `$score_min_intercept`/`$score_min_slope` (default `0`/`-0.2` — minimap2 never sets `--score-min`). `calc_mapq` is the verbatim Bowtie2 ladder; fed minimap2's `AS` + `second_best=None`-backfill + `(0,-0.2)` + len it produces the same MAPQ as Perl (the spike's BAM is byte-deterministic incl. MAPQ). Rust `score_min_params` already returns `(0,-0.2)` when `--score_min` is absent.
- **`methylation.rs` / `output.rs` (XM/XR/XG, genomic-seq, SAM fields) — reused.** Format-agnostic. `config.dovetail` (2b) is already aligner-independent → the PE TLEN is correct for minimap2 with no change.
- **`report.rs`** — the `aligner.name()` "was run with …" branch (2a) covers minimap2 once the enum variant + name exist.

**NEW / generalized (the wrapper deltas — bigger than HISAT2's):**
- **`config.rs`** — `Aligner::Minimap2` + `token()="mm2"` + `name()="minimap2"`; `resolve_aligner` returns it (drop the deferred-error); pick `cli.path_to_minimap2`; preset-conflict dies (Perl 8375/8378/8391: short⊕nanopore, short⊕pacbio, pacbio⊕nanopore); `--mm2_short_reads`/`--mm2_pacbio`/`--mm2_nanopore`-without-`--minimap2` die (Perl 8330). `--mm2_maximum_length` is ALREADY validated minimap2-only (config.rs 173) — now it must be ACTIVE (the convert-side cutoff, default 10000, Perl 8354).
- **`options.rs`** — minimap2 is a **CLEAN-SLATE** assembly (Perl 8359 `@aligner_options = ()`), NOT the Bowtie2 base + delta (the HISAT2 model). Exactly, in order (Perl 8361-8413): `-a` → `--MD` → `--secondary=no` → `-t 2` → `-x <preset>` (`--mm2_short_reads`→`sr`; `--mm2_pacbio`→`map-pb`; **default OR `--mm2_nanopore`→`map-ont`** — Perl 8399-8408, the `else` branch serves both, B I-5) → `-K 250K`. The Bowtie2/HISAT2 path is untouched (gate the whole assembly on `kind`).
- **`aligner.rs`** — `detect_aligner(Minimap2)`; `PINNED_MINIMAP2_VERSION="2.31-r1302"`; **a minimap2-only version parse** (minimap2 prints only the version number, e.g. `2.31-r1302`, Perl 7081-7083 — the `split("version")` parser won't match).
- **`discovery.rs`** — minimap2 index = a **single `.mmi`** file per basename (`BS_CT.mmi`/`BS_GA.mmi`), NOT the 6/8-suffix set. `index_suffixes(Minimap2,…)` = `["mmi"]` (the basename + `.mmi`); no large/`.bt2l`/`.ht2l` variant.
- **`align.rs`** — minimap2 invocation is **positional + no strand flag**: `minimap2 <opts> <BS_*.mmi> <reads>` (SE) / `<opts> <mmi> <reads1> <reads2>` (PE) (Perl 7025/6669). vs Bowtie2/HISAT2 `<orient --norc/--nofw> -x <basename> -U/-1/-2`. So `spawn` needs a per-aligner invocation shape: minimap2 passes the **full `.mmi` path positionally**, drops the `orient.flag()` (`--norc`/`--nofw`), and drops `-x`/`-U`/`-1`/`-2`. The `SamRecord` parse is unchanged (AS+MD captured; no ZS/XS for minimap2 → `second_best=None`, correct).
- **`convert.rs`** — **SE: NO change** (review A/B I-1). The Perl SE transform (`biTransformFastQFiles` 5489-5651) appends **NO** read-id suffix (not `/1`, not `/1/1`); the Rust SE converter already matches (empty suffix). The **`/1` single-tag delta is PE-ONLY** (Perl 5945-5959, in the read-number branch) → it belongs with the deferred PE-minimap2 work (§0), NOT here. The only SE convert touch is **`--mm2_maximum_length`** (range-die `<100`/`>100000` + default `10000`, Perl 8350-8356; the convert-side drop already exists in Rust — the work is removing the `resolve` deferred-error gate, not "activating" the drop). Inert for short bisulfite reads but faithful.
- **`lib.rs`** — `_bismark_mm2*` naming token (via `Aligner::token()`); minimap2 dispatch.

## 3. Behavior
1. **Selection:** `--minimap2` → `Aligner::Minimap2` (drop deferred-error); preset-conflict + non-minimap2-preset dies; default Bowtie 2 unchanged.
2. **Detection:** `detect_aligner(Minimap2, cli.path_to_minimap2)` — resolve `minimap2`, `--version` → parse the bare version (`2.31-r1302`), warn if ≠ pinned.
3. **Options (clean-slate):** `-a --MD --secondary=no -t 2 -x map-ont -K 250K` (default); preset from `--mm2_short_reads`(`sr`)/`--mm2_pacbio`(`map-pb`)/default(`map-ont`). Pinned default string unit-asserted.
4. **Index discovery:** single `.mmi` per basename; `-x`/positional handled in spawn (the index path = `<basename>.mmi`).
5. **Invocation:** positional `.mmi` + reads, no orient flag (align.rs per-aligner spawn shape).
6. **Convert:** single `/1`/`/2` tag for minimap2; `--mm2_maximum_length` cutoff active (minimap2-only).
7. **Naming/report:** `<base>_bismark_mm2{,_pe}.bam` / `_{SE,PE}_report.txt`; "Bismark was run with minimap2 …".
8. **Merge/MAPQ/XM:** reused unchanged (§2). `second_best=None` for minimap2; `calc_mapq` with `(0,-0.2)`.
9. **`--multicore` + `--minimap2` (OQ-4d):** UNLIKE HISAT2, minimap2 alignment is per-read-independent (no batch-global splice discovery) → worker-invariance *should* hold. But `-t 2` (intra-process) + the `-K 250K` minibatching need a worker-invariance check before allowing it. *Lean:* gate a `--multicore` minimap2 cell (expect worker-invariant); fall back to a fail-loud reject if it isn't. (Contrast: HISAT2 was hard-rejected because splice discovery is batch-global.)
10. **Bowtie 2 + HISAT2 byte-frozen:** the clean-slate options + per-aligner spawn shape are gated on `kind` so the existing paths can't move; V1 re-runs the Bowtie2 + HISAT2 suites + gates.

## 4. Signature (sketch)
```rust
// config.rs
pub enum Aligner { Bowtie2, Hisat2, Minimap2 }   // token: mm2; name: minimap2
// aligner.rs
pub const PINNED_MINIMAP2_VERSION: &str = "2.31-r1302";
fn parse_minimap2_version(stdout: &str) -> Option<String>;   // bare "x.y-rNNNN"
// options.rs — clean-slate branch
//   if kind==Minimap2 { return minimap2_options(cli) }  // -a --MD --secondary=no -t 2 -x <preset> -K 250K
// discovery.rs
//   index_suffixes(Minimap2,…) => vec!["<stem>.mmi"]
// align.rs — per-aligner invocation
//   Minimap2: cmd.args(opts); cmd.arg(format!("{index}.mmi")); cmd.arg(reads...)   // positional, no orient/-x/-U
// convert.rs — single /1 (Minimap2) vs /1/1 (others); apply maximum_length_cutoff for Minimap2
```

## 5. Implementation outline (TDD)
1. **Lock Bowtie2 + HISAT2 baseline** (full suite + note the gate md5s).
2. `config.rs`: `Aligner::Minimap2` + token/name; `resolve_aligner`→Minimap2; preset dies; `--mm2_maximum_length` range-die + default 10000 (remove the deferred-error gate); dispatch detection with `path_to_minimap2`. Tests: resolve + conflict dies; non-minimap2-preset dies; **UPDATE the un-deferral tests** (V11): `resolve_aligner_minimap2_still_deferred` → now resolves; the `--mm2_maximum_length`-without-minimap2 deferred-error → flipped (it's now valid in minimap2 mode, still errors in non-minimap2 mode).
3. `aligner.rs`: `PINNED_MINIMAP2_VERSION`; `parse_minimap2_version` (+ a unit test on `2.31-r1302`); `detect_aligner(Minimap2)`.
4. `options.rs`: clean-slate `minimap2_options` (gate on kind); preset selection. Tests: default string `-a --MD --secondary=no -t 2 -x map-ont -K 250K`; `--mm2_short_reads`→`sr`; `--mm2_pacbio`→`map-pb`; Bowtie2/HISAT2 strings byte-unchanged.
5. `discovery.rs`: `.mmi` single-file suffix for Minimap2. Tests: `.mmi` discovery; missing-`.mmi` error.
6. `align.rs`: per-aligner spawn shape (positional `.mmi`, no orient/-x/-U). Tests: the minimap2 command line is `<opts> <mmi> <reads>` (no `--norc`/`-x`/`-U`); SamRecord parse of a minimap2 line (AS+MD, no ZS/XS → `second_best=None`).
7. `convert.rs`: **SE — verify NO change** (no suffix appended; already matches). Wire `--mm2_maximum_length` (range-die + default 10000 in `config.rs`; the drop already exists in convert). Tests: SE minimap2 → no suffix; a >cutoff read dropped (minimap2 only); Bowtie2/HISAT2 unchanged. (The `/1` single-tag is PE-only → deferred with PE, §0.)
8. Naming/report + dispatch in lib.rs. Tests: `_bismark_mm2*` name; "run with minimap2".
9. **minimap2-aware fakes** (named `minimap2`, banner `2.31-r1302`, via `--path_to_minimap2`): SE mapped (positional `.mmi` invocation; one forward primary; `s2:i:` present-but-ignored → `second_best=None`), assert FLAG/XM/MAPQ. Integration: SE directional → non-dir/pbat SE.
10. **🎯 oxy byte-identity gate** — `bismark_rs --minimap2` vs Perl `bismark --minimap2` + minimap2 2.31, identical argv, decompressed SAM (`@PG` filtered) + report (wall-clock filtered) + aux, **10k + 1M**, SE directional → non-dir/pbat SE (+ a `--multicore` SE cell per OQ-4d). PE → a later step (this phase or fold into Phase 5 — OQ-4e). Bowtie2 + HISAT2 gates re-run (V1).

## 6. Efficiency
Additive enum-dispatch + a clean-slate option branch + a positional-spawn branch + a single-`.mmi` suffix; zero hot-path impact. The merge/MAPQ/XM core is untouched.

## 7. Integration
Reads `.mmi` indexes (present on oxy, ~7.9 GB); writes `_bismark_mm2*`. Bowtie2 + HISAT2 byte-frozen. The emitted BAM stays consumable by the ported Rust tools.

## 8. Assumptions
- **From the Phase-3 spike:** determinism HOLDS (`-t 2`, byte-identical run-to-run); output in input order (lockstep OK); default options `-a --MD --secondary=no -t 2 -x map-ont -K 250K`; `s2:i:` ignored by Bismark (→ `second_best=None`); MAPQ via `calc_mapq` with default `(0,-0.2)`; `/1`/`/2` single-tag; both-strand effect moot under `--secondary=no`.
- **From epic:** decompressed-SAM gate; `@PG` aligner-independent; `.mmi` on oxy; the 2b dovetail trap (config.dovetail aligner-independent — already fixed).
- minimap2 raw stream carries `AS:i:` + `MD:Z:` on every aligned record (the merge dies otherwise) — confirm in the fake/gate.
- The 2/4-instance strand model + index→strand mapping holds for minimap2 (spike: 0 reverse on the CT instance; verify at the gate).

## 9. Validation
| # | Verify | How | Expect |
|---|---|---|---|
| V1 | Bowtie2 + HISAT2 byte-frozen | full suite + both oxy gates | unchanged |
| V2 | minimap2 default option string | unit (hard literal) | `-a --MD --secondary=no -t 2 -x map-ont -K 250K` |
| V3 | preset selection + dies | unit | `sr`/`map-pb`/`map-ont`; conflict + non-mm2 dies |
| V4 | `.mmi` discovery | unit | single-file resolve; missing-`.mmi` error |
| V5 | minimap2 spawn shape | unit | positional `.mmi`, no `--norc`/`-x`/`-U`/`-1`/`-2` |
| V6 | minimap2 SamRecord parse — **feed a real `s2:i:` tag** | unit | AS+MD captured; **`second_best==None` (s2 IGNORED)** — guards against the spike's wrong "read s2" instruction |
| V7 | SE convert + max-len | unit | SE appends **NO suffix** (not `/1`, not `/1/1`); `--mm2_maximum_length` range-die `<100`/`>100000` + default 10000; >cutoff read dropped (mm2 only) |
| V8 | naming/report (SE) | integration | `_bismark_mm2*` + "run with minimap2" (SE report writer 1722-1728 HAS the `$mm2` branch) |
| V9 | 🎯 SE oxy gate | Perl `--minimap2` vs Rust, 10k+1M, dir + non-dir/pbat SE (+ `--multicore` per OQ-4d) | byte-identical; **assert 0 secondary/supplementary on every instance** (review A — the lockstep one-primary-per-read invariant) |
| V11 | un-deferral tests updated | unit | `minimap2_still_deferred` + the `--mm2_maximum_length` deferred-error tests flipped to the enabled behavior |
| V10 | MAPQ parity on minimap2 AS scale | within V9 (BAM byte-identical ⇒ MAPQ matches) | holds |

## 10. Questions / ambiguities (Felix approved the leans 2026-06-05 — RESOLVED)
- **OQ-4a (RESOLVED → pin during impl):** minimap2 version-parse — `minimap2 --version` emits the bare `2.31-r1302`; the parse takes the first whole line, trims, compares to the pin. Confirm the exact format when wiring `detect_aligner(Minimap2)`.
- **OQ-4b (RESOLVED → default `map-ont` gated):** gate the **default `map-ont`** preset at the oxy gate; unit-test the `sr`/`map-pb` option strings (V3) but do NOT gate them (the both-strand population differs under `sr`; not worth the oxy hours unless a user needs it).
- **OQ-4c (RESOLVED 2026-06-05 — Felix: DEFER PE-minimap2 out of v1.x):** PE-minimap2 is **NOT byte-identity-reachable** (C-1 Perl PE WIP `# TODO`+`sleep(1)`×2/pair; C-2 PE report mislabels mm2 as "HISAT2"). **Decision: defer PE-minimap2 entirely** — ship SE minimap2 byte-identical; document PE-minimap2 as a known gap (the Perl oracle is unfinished; Bowtie2 + HISAT2 PE cover paired-end). **Phase 4 = SE minimap2.** Phase 5's combined full-scale gate covers Bowtie2 / HISAT2 (SE+PE) / **minimap2 SE**. (Re-opening PE-minimap2 would require fixing the upstream Perl path — out of the faithful v1.x.)
- **OQ-4d (RESOLVED → gate a `--multicore` cell expecting invariance):** add a `--multicore` SE gate cell; **expect worker-invariance** (minimap2 is per-read-independent, unlike HISAT2's batch-global splice discovery). Fall back to a fail-loud reject ONLY if the gate shows divergence (then document like the HISAT2 reject).
- **OQ-4e (RESOLVED → reproduce `-t 2` verbatim):** Bismark hardcodes `-t 2`; the Rust reproduces it verbatim in the assembled option string (not a thread count we choose). Confirm 1M (multi-minibatch) determinism at the gate.

## 11. Self-Review
- **Logic:** the spike de-risked the merge — Phase 4 touches the wrapper only (options/spawn/discovery/convert/detect/naming), with `merge.rs`/`mapq.rs`/`methylation.rs` provably reused (minimap2 → `second_best=None` → the existing no-2nd-best path; `calc_mapq` verbatim + matching inputs). Each delta is source-cited (Perl 8359-8413 options, 7025 invocation, 5945-5958 `/1`, 2772-2796 parse, 3134 MAPQ).
- **Edge cases:** preset conflicts, non-mm2 preset dies, missing `.mmi`, >max-len reads, `--multicore`, the `-t 2` determinism at scale — all in V2-V10/OQs.
- **Bowtie2 + HISAT2 frozen:** the clean-slate + spawn-shape changes are `kind`-gated; V1 re-runs both prior suites + gates. This is the biggest regression surface (two backends now frozen) — V1 must include a HISAT2 PE cell + a Bowtie2 PE-dovetail cell.
- **Risks:** LOW after the spike. The residual unknowns (OQ-4a/d) are bounded (a version-string read; a worker-invariance gate cell). The both-strand fear is retired. The clean-slate options + positional spawn are mechanical once `kind`-gated.

## 12. Implementation Notes (2026-06-05)

**Status:** IMPLEMENTED on branch `rust/aligner-mm2` (off `iron-chancellor` `49a1518`); local-green (300 tests: 253 lib + 47 integ; clippy `-D warnings` + `cargo fmt --check` clean). **NOT committed.** Awaiting dual `/code-reviewer` + `/plan-manager`, then the V9 oxy gate.

**Seams touched (all `kind`-gated; Bowtie 2 + HISAT2 byte-frozen):**
- `config.rs` — `Aligner::Minimap2` (+`token()="mm2"`, `name()="minimap2"` lowercase); `resolve_aligner`→Minimap2 (deferred-error dropped); detection-path arm → `cli.path_to_minimap2`; **PE-minimap2 reject** (`Unsupported`, after `resolve_layout`); new `resolve_mm2_max_length(cli, aligner)` folding BOTH Perl blocks — the `unless($mm2)` non-minimap2 flag dies (8329-8341) AND the `if($mm2)` range-die(`100..=100_000`)+default-10000 (8344-8356) — returns the resolved cutoff into `read_processing`.
- `aligner.rs` — `PINNED_MINIMAP2_VERSION="2.31-r1302"`; `parse_minimap2_version` (bare first non-empty line — Perl does no regex, 7081-7083); `detect_aligner` dispatches the parser by `kind`; `binary_name`/`pinned_version`/`path_flag` arms.
- `options.rs` — `minimap2_options(cli)` clean-slate `-a --MD --secondary=no -t 2 -x <preset> -K 250K` + preset selection (`sr`/`map-pb`/`map-ont`, default+`--mm2_nanopore`→`map-ont`) + the 3 conflict dies. **Build-then-wipe:** the Bowtie 2 base + `apply_aligner_specific_options` still run (so `-N 2`/splice-flag dies fire as in Perl), then the result is substituted — mirrors Perl's `@aligner_options=()` order.
- `align.rs` — extracted a **pure `build_se_argv(aligner, …)`** (unit-testable, physically separates the frozen Bowtie 2/HISAT2 shape from minimap2's); minimap2 = positional `<index>.mmi <input>` (literal byte append, no orient/`-x`/`-U`). `AlignerStream::spawn` gained the `aligner` param + aligner-neutral error strings. Added the 🔴 "`s2:i:` intentionally ignored" comment at the tag-scan loop.
- `discovery.rs` — `index_suffixes(Minimap2)` = single `<stem>.mmi`; `large` ignored (no `.mmil`; small/large fallback is a harmless no-op).
- `lib.rs` — SE spawn call site passes `config.aligner`. **Convert = NO change** (SE no-suffix already correct, byte-frozen by non-modification); naming token `_bismark_mm2` flows via `token()`; report "run with minimap2" via `name()`.

**Deviations / decisions vs the plan:**
- **PE-minimap2 → hard reject** (not merely "documented gap"): a paired-end `--minimap2` run now fails loudly (`config.rs`, test `minimap2_paired_end_is_rejected`). Faithful + safe — mirrors the HISAT2-multicore reject precedent and the SKILL's "fail explicitly" rule; otherwise PE would silently feed a Bowtie 2-shaped argv to minimap2.
- **`gap_penalties` confirmed vestigial** (grepped: never read by `calc_mapq` or anywhere outside `config/options`), so the minimap2 clean-slate returns default penalties with zero output impact — `--rdg`/`--rfg` are wiped exactly as Perl does.
- **I-3 (max-length count interaction) verified by construction + tested end-to-end:** the SE merge loop is driven by the *original* read file (`drive_merge` bumps `sequences_count` before the stream lookup), so a >cutoff read dropped from the temp is still "analysed + no-alignment". `minimap2_max_length_drop_counts_as_no_alignment` pins it (2 analysed / 1 unique / 1 no-align / 50.0%).
- **V5b regression guard added** (Reviewer A): `se_argv_bowtie2_shape_frozen` + `se_argv_hisat2_same_shape_as_bowtie2` pin the frozen argv through the refactored builder.

**V-gate → test map (local):** V2 `minimap2_default_option_string`; V3 `minimap2_preset_selection`/`minimap2_preset_conflicts_die`; V4 `minimap2_suffix_is_single_mmi`/`discovers_complete_mmi_index`/`missing_mmi_errors_with_minimap2_wording`/`bt2_index_rejected_in_minimap2_mode`; V5 `se_argv_minimap2_positional_mmi`/`se_argv_minimap2_orientation_independent`; V5b `se_argv_{bowtie2,hisat2}_*`; V6 `minimap2_s2_tag_is_ignored`; V7 `mm2_maximum_length_range_and_default` + `minimap2_max_length_drop_counts_as_no_alignment` (+ SE-no-suffix implicit via the integration mapping success); V8 `minimap2_se_mapped_names_and_report`; V11 `resolve_aligner_selects_minimap2`/`minimap2_is_accepted_not_deferred`/`mm2_flags_require_minimap2_mode`; PE-reject `minimap2_paired_end_is_rejected`. **V1** = the 277-test baseline still green (no regressions) + the byte-frozen argv/option guards.

**🎯 V9 + V10 — oxy SE byte-identity gate: ✅ PASSED 2026-06-05** (`GATE_OXY.md`; harness `phase4_minimap2_se_gate.sh`). `bismark_rs --minimap2` byte-identical to Perl v0.25.1 + minimap2 2.31-r1302 at **10k AND 1M**, all cells: se_dir (7,932 / 796,919 rec), se_nondir (7,940 / 797,799), se_pbat (51 / 6,858), report identical (wall-clock filtered) — V10 (MAPQ) holds by BAM identity. **Worker-invariance CONFIRMED (OQ-4d):** Rust `--parallel 8` == `--parallel 1` (10k+1M) — minimap2 is per-read-independent, multicore ALLOWED (unlike the HISAT2 reject). **`--secondary=no` → 0 SECONDARY** on every instance (the lockstep invariant, Reviewer A's V9 ask). **Scale finding (byte-harmless):** at 1M the GA/G→A instance emits 2 SUPPLEMENTARY (flag 2064, chimeric `SA:Z:`) records — `--secondary=no` does not suppress supplementary; both affected reads are absent from BOTH BAMs (identical) and se_nondir is byte-identical, so zero divergence. The harness's diagnostic was corrected to fail only on SECONDARY.

**Iteration log:**
- #1 Added the `Minimap2` enum variant + all match arms across config/aligner/discovery; crate compiled clean (no missing arms in production code).
- #2 Added behavior + unit tests; lib 233→253 green. Integration: 1 stale test (`minimap2_is_deferred`) failed as expected (minimap2 no longer deferred) → rewrote it as `minimap2_is_accepted_not_deferred` + added `minimap2_paired_end_is_rejected`.
- #3 Added minimap2 integration tests (positional-`.mmi` fake that can't false-pass on a wrong invocation shape; V8 + I-3). Two `PathBuf`→`&Path` compile errors at the BAM-reader calls → borrowed. All 300 green.
- #4 clippy clean; `cargo fmt` reflowed 4 files (long collect chain + array→inline); re-verified fmt + tests clean. Removed 2 stray `reads_bismark_bt2.*` files a pre-existing Bowtie 2 test regenerates.

## Revision History
- **rev 1 (2026-06-05):** dual plan-review (`PLAN_REVIEW_A.md` APPROVE-WITH-CHANGES + `PLAN_REVIEW_B.md` REQUEST-CHANGES; both verified the merge/MAPQ no-op thesis is CORRECT). Folded: **§0 — scoped to SE; PE-minimap2 deferred** (C-1 Perl PE WIP `# TODO`+`sleep(1)`×2/pair; C-2 PE report mislabels minimap2 as HISAT2 — both confirmed in the oracle); **SE convert needs NO change** (the `/1` is PE-only, A/B I-1 — was a byte-identity trap); **the Phase-3 spike's "merge must read s2" is WRONG** (B I-4 — added a code note + V6 asserts `second_best==None` on a real `s2:i:`; the spike report corrected); `--mm2_nanopore`→`map-ont` added (B I-5); V9 gains a **zero-secondary/supplementary** assertion (A); **un-deferral tests** flipped (V11); `--mm2_maximum_length` range-die + default 10000 (B I-2/I-3). OQ-4c reopened as a Felix scope decision.
- **rev 0 (2026-06-05):** initial Phase 4 plan, grounded in the Phase-3 spike + a Perl source trace. Key finding: the merge needs NO adaptation (s2 ignored → `second_best=None`; calc_mapq verbatim).
