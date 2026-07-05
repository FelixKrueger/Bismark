# PLAN — Phase 2a: HISAT2 wrapper core (detection + options + discovery + naming + SE gate)

> **Epic:** `06052026_bismark-aligner-v1x/EPIC.md`, Phase 2a. **Depends on:** Phase 1 (spike ✅ premise HOLDS).
> **Split from the combined Phase 2** per dual plan-review (`PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md`, this dir): 2a = the SE-gated wrapper *core*; **2b** = the PE read-1 `ZS` asymmetry fix + PE/non-dir/pbat/FastA gate.

- **Created:** 2026-06-05 · **rev 1** (split + dual-review fixes folded; see Revision History).
- **Branch / worktree:** `rust/aligner-v1x` @ `~/Github/Bismark-aligner`, crate `rust/bismark-aligner` (`bismark_rs`).
- **Oracle / pin:** Perl Bismark **v0.25.1** + **HISAT2 2.2.2** (oxy `bismark-test`), samtools 1.23.1.

---

## 1. Goal
Make `--hisat2` a working, **SE byte-identical** HISAT2 backend: aligner detection + option assembly + index discovery + output/report naming, gated SE-only on oxy. The PE alignment path (which carries the `ZS` read-1 asymmetry) is **2b**. **Bowtie 2 stays byte-frozen.** `options.rs` is built fully correct here (SE **and** PE option strings, unit-pinned) since it's shared infra; only the PE *end-to-end gate* is deferred to 2b.

## 2. Context — seams (verified by dual review against the source)
- `config.rs` — `enum Aligner` (L20) has only `Bowtie2`; **add `Hisat2`**. `RunConfig.aligner` **field already exists** (L128). `resolve_aligner` (L251) returns `Err(deferred)` for `--hisat2` → return `Aligner::Hisat2`. `detect_bowtie2` call (L186) → dispatch on kind.
- `aligner.rs` — `detect_bowtie2`+`PINNED_BOWTIE2_VERSION` → `detect_aligner(kind, path)`; add `PINNED_HISAT2_VERSION="2.2.2"`. The `parse_bowtie2_version` `split("version")` parser **already** handles `hisat2-align-s version 2.2.2` (reuse verbatim).
- `options.rs` — `build_aligner_options` is order-critical (L22-180). **Do NOT thread a conditional into the push loop.** Build the Bowtie2 string unchanged, then **append `" --no-softclip --omit-sec-seq"` to the finished string iff `Hisat2`** (mirrors Perl's last-push, L8314 — keeps Bowtie2 structurally frozen). **Suppress `--dovetail` for HISAT2 PE** (L143-152 push it unconditionally; Perl gates `if($bowtie2)` 8051-8059). Splice flags → §3.4.
- `discovery.rs` — `bt2_suffixes` (L88-98) is **hardcoded to 6 Bowtie2 suffixes** `{1,2,3,4,rev.1,rev.2}`; consumed by `first_missing` (L102) + `discover_genome` (L122-143). HISAT2 = **8 suffixes** `{1..8}` (no `rev.*`) + `.ht2l` large fallback → make the suffix **list** per-aligner (not just the extension).
- `lib.rs` — `_bismark_bt2` naming literals at **L330/341/477/912/922/1014** (`derive_output_path` `default_suffix` arg). `--basename` path uses `basename_suffix` which carries **no** token (L529) → thread the token ONLY into `default_suffix`.
- `parallel.rs` — **7th seam (review A-Critical):** `_bismark_bt2` hardcoded at **L406/409/458/461/685/695/728/828/840/888** (`--multicore` path). Thread the token here too (so `--multicore`+`--hisat2` names correctly).
- `report.rs` — `ReportHeader` (L34) carries `aligner_options` but **not** the aligner kind; L64 hardcodes "Bismark was run with **Bowtie 2**…". Add an `aligner`/`aligner_name` field; branch the line (Perl HISAT2 = 1728/1849). The `aligner_options` echo (L64-67) surfaces option errors in the report too.
- `align.rs` — spawn (`AlignerStream::spawn` L166 / PE L354) takes the binary as `&Path` + emits `-x/-U` (`-1/-2`) — **already aligner-agnostic** (Perl drives HISAT2 identically, 6818/6380). `ZS`-or-`XS` parse present + tested (L100-104, tests L499/507). *(The PE read-1 ZS asymmetry is a merge-path concern → 2b, NOT here.)*
- `methylation.rs` — `N`-op extraction (L189/362) faithfully ports Perl 4372-4377 (verified, review B-L6) → "verify, not implement."
- `error.rs` — `FaultyIndex` (L42) + detector msg (L55) are Bowtie2-worded; **stderr, outside the gate** → fidelity-only.

## 3. Behavior
1. **Selection:** `--hisat2` → `Aligner::Hisat2` (drop the deferred-error); conflict errors (config L252-266) preserved; default Bowtie 2. minimap2 stays deferred-error (Phase 4).
2. **Detection:** `detect_aligner(Hisat2, cli.path_to_hisat2)` — resolve `hisat2` (dir-or-PATH), `--version`, parse `2.2.2`, warn if ≠ pinned.
3. **Options (CORRECTED per dual review):** HISAT2 = the Bowtie2 string **with `--no-softclip --omit-sec-seq` appended LAST** (after the PE tail + `--maxins` + `--quiet`). **`--dovetail` is NOT emitted for HISAT2** (PE). Pinned strings:
   - SE: `-q --score-min L,0,-0.2 --ignore-quals --no-softclip --omit-sec-seq`
   - PE: `-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --maxins 500 --no-softclip --omit-sec-seq` *(no `--dovetail`)*
4. **Splice flags (decision — review G2/L4):** `--no-spliced-alignment` / `--known-splicesite-infile <f>` (cli.rs L211/214, currently parsed-and-ignored). **Handle faithfully** (Perl 8289-8307: push before the softclip delta; file-exists check on the infile; **die** if both set, Perl 8290; **die** in non-HISAT2 mode, Perl 8319-8324 — closes a pre-existing Bowtie2 silent-no-op gap). *(Budget fallback: fail-loud-reject in both modes — never silent. Default: handle.)*
5. **Index discovery:** per-aligner suffix **list** — Bowtie2 `{1,2,3,4,rev.1,rev.2}.bt2`(+`.bt2l`); HISAT2 `{1..8}.ht2`(+`.ht2l`). `first_missing`/`discover_genome` consume the list; `-x <basename>` unchanged.
6. **Naming:** `aligner_token(kind)` (`bt2`/`hisat2`) threaded into `default_suffix` at the lib.rs (6) **and parallel.rs (10)** call sites; **NOT** into `basename_suffix`, `_unmapped_reads*`, or `_ambiguous_reads*` (no token in Perl — review L5). Emit `<base>_bismark_hisat2.bam` / `_SE_report.txt`; report line "Bismark was run with HISAT2 against …".
7. **`--ambig_bam` (review L5):** trace whether Perl emits a HISAT2 ambig BAM (temp names hardcoded `_bismark_bt2.ambig.bam` "# only for Bowtie 2", 656/661/715/720). If Bowtie2-only → **hard-reject `--ambig_bam` in HISAT2 mode**; else pin the exact name. Resolve before implementing the ambig naming.
8. **SE edge cases:** spliced `N`-CIGAR extraction byte-equal (broadened tests, V6); SE `ZS` multi-mapper 2nd-best parse + MAPQ (V5, both `ZS==AS` tie and `ZS<AS`); discard arithmetic (smoke, not a guard — V10); `.ht2l` large index; `--phred64`; `--non_directional`/`--pbat` **SE** (4-instance); FastA SE.
9. **Bowtie 2 byte-frozen:** append-to-finished-string + token-only-at-`default_suffix` make this structural; V1 re-runs the full suite + Bowtie2 oxy gate (incl. a **PE-dovetail** cell — the `--dovetail` kind-gating must keep `options.rs::paired_end_tail_and_default_maxins` green).

## 4. Signature
```rust
// config.rs
pub enum Aligner { Bowtie2, Hisat2 }                 // + Minimap2 later
// aligner.rs
pub const PINNED_HISAT2_VERSION: &str = "2.2.2";
pub fn detect_aligner(kind: Aligner, path_to: Option<&Path>) -> Result<DetectedAligner>;
// options.rs — Bowtie2 string unchanged; append delta on the finished string
pub fn build_aligner_options(cli:&Cli, kind:Aligner, fmt:ReadFormat, is_paired:bool) -> Result<(String,GapPenalties)>;
//   ... existing body ...; gate `--dovetail` push on kind==Bowtie2;
//   let s = opts.join(" "); if kind==Hisat2 { s = format!("{s} --no-softclip --omit-sec-seq") }
// discovery.rs
fn index_suffixes(kind: Aligner, large: bool) -> Vec<&'static str>;  // bt2:{1,2,3,4,rev.1,rev.2} / ht2:{1..8}
// naming
fn aligner_token(kind: Aligner) -> &'static str;     // Bowtie2=>"bt2", Hisat2=>"hisat2"
```

## 5. Implementation outline (TDD)
1. **Lock Bowtie 2 baseline** (full suite + note gate md5s).
2. `config.rs`: `Aligner::Hisat2`; `resolve_aligner`→Hisat2; populate `RunConfig.aligner`; dispatch detection. Tests: resolve+detect; conflicts preserved; minimap2 still deferred.
3. `aligner.rs`: `detect_aligner(kind)`; `PINNED_HISAT2_VERSION`; reuse the version parser. Test: `hisat2-align-s version 2.2.2` → `2.2.2`; mismatch warns.
4. `options.rs`: gate `--dovetail` on `Bowtie2`; append `--no-softclip --omit-sec-seq` to the finished string for Hisat2; splice-flag handling (§3.4). Tests: SE+PE HISAT2 pinned strings (§3.3) — **no `--dovetail` in PE**; both-splice-set die; non-HISAT2 splice die; **Bowtie2 strings byte-unchanged**.
5. `discovery.rs`: per-aligner suffix list (8 `.ht2`, `.ht2l` fallback). Tests: `.ht2` 8-file discovery; `.ht2l`; missing-index wording.
6. Naming: `aligner_token`; thread into lib.rs (6) + parallel.rs (10) `default_suffix` sites; `ReportHeader.aligner` + report line. Tests: HISAT2 SE name/report; `--basename` drops token; `_unmapped`/`_ambiguous`/(ambig per §3.7) untouched.
7. `--ambig_bam` HISAT2 decision (§3.7) implemented (reject or pin).
8. **HISAT2-aware fakes** (named `hisat2`, banner `hisat2-align-s version 2.2.2`, reached via `--path_to_hisat2`): SE `ZS` multi-mapper (both `ZS==AS`/`ZS<AS`) + **broadened spliced** (multi-`N`, `N`-adjacent-`I`/`D`, GA/OB-strand), each XM/MAPQ-asserted. Integration: SE directional → non-dir/pbat SE → FastA SE.
9. **🎯 SE oxy byte-identity gate** — `bismark_rs --hisat2` vs Perl `bismark --hisat2` + HISAT2 2.2.2, identical argv, decompressed SAM (`@PG` filtered) + report (wall-clock filtered) + `--unmapped`/`--ambiguous` aux, **10k + 1M**, SE directional → non-dir/pbat SE (a real gate cell) → FastA SE; + a `--multicore` SE cell (proves the parallel.rs token). Bowtie 2 gate re-run (V1).

## 6. Efficiency
Additive enum-dispatch + a trailing concat + a per-aligner suffix list; zero hot-path impact.

## 7. Integration
Reads `.ht2` indexes (present on oxy); writes `_bismark_hisat2*` SE BAM/report/aux. Bowtie 2 branch byte-frozen. 2b consumes this core for the PE path.

## 8. Assumptions
- From epic: oracle Perl v0.25.1 + HISAT2 2.2.2; decompressed-SAM gate; `@PG` aligner-independent (gate filter unaffected); HISAT2 deterministic (spike); indexes on oxy.
- `calc_mapq` is aligner-agnostic + valid for HISAT2 (review confirmed; the only MAPQ risk is the PE input second_best → **2b**).
- HISAT2 raw stream always carries `AS:i:`/`MD:Z:` (merge dies otherwise) — confirm in the fake/gate.
- 2/4-instance strand model + `--norc`/`--nofw` identical for HISAT2 (Perl 6371-6376; spike).

## 9. Validation
| # | Verify | How | Expect |
|---|---|---|---|
| V1 | Bowtie 2 byte-frozen | full suite (incl. PE-dovetail) + Bowtie2 oxy gate | unchanged |
| V2 | HISAT2 SE option string | unit | `…--ignore-quals --no-softclip --omit-sec-seq` |
| V3 | HISAT2 PE option string | unit (hard literal) | `…--no-mixed --no-discordant --maxins 500 --no-softclip --omit-sec-seq` (**no `--dovetail`**) |
| V4 | `.ht2`/`.ht2l` 8-suffix discovery | unit | basename resolved; large fallback; correct arity |
| V5 | SE `ZS` 2nd-best → MAPQ | fake (`ZS==AS` tie + `ZS<AS`) | merge decision + MAPQ byte-equal |
| V6 | spliced `N` extraction | fakes: multi-`N`, `N`+indel, GA/OB-strand + oxy 12-rec | XM/genomic-seq byte-equal |
| V7 | naming/report | integration | `_bismark_hisat2*` + "run with HISAT2" |
| V8 | splice flags | unit + a gate cell w/ `--no-spliced-alignment` | handled/echoed (or fail-loud); both-set & non-HISAT2 die |
| V9 | 🎯 SE oxy gate | Perl `--hisat2` vs Rust, 10k+1M, dir + non-dir/pbat SE + FastA + `--multicore` SE | byte-identical |
| V10 | discard arithmetic | report unique-best − discards == BAM recs | holds — **smoke check only** (not a correctness guard) |

## 10. Questions / ambiguities
- **OQ-2a (RESOLVED):** option order — `--no-softclip --omit-sec-seq` appended LAST; `--dovetail` suppressed for HISAT2. (Was wrong in rev 0; corrected per dual review.)
- **OQ-2d (Open):** `--ambig_bam`+HISAT2 — supported (pin name) or Bowtie2-only (hard-reject)? Resolve by tracing Perl 656/661/715/720 vs 1575/1586 before implementing §3.7. *Assumption:* hard-reject if the "only for Bowtie 2" comment reflects reality.
- **OQ-2e (Open):** thread the token through `parallel.rs` (preferred — mechanical) vs fail-loud-reject `--multicore`+`--hisat2` in 2a. *Assumption:* thread it (+ the `--multicore` SE gate cell).
- **OQ-2f (Open):** splice flags — handle faithfully (default) vs fail-loud-reject + defer wiring. *Assumption:* handle.

## 11. Self-Review
- **Logic:** every fix is located + source-cited; the append-to-finished-string + token-only-at-`default_suffix` disciplines make Bowtie 2 byte-frozen *structural*, not hope-based. The PE `ZS` hazard is correctly excised to 2b (this plan does not touch the merge second-best path).
- **Edge cases:** spliced-`N` broadened (multi-N/indel/GA-strand), `ZS` tie-vs-shift, `.ht2l`, `--phred64`, non-dir/pbat SE, FastA, `--multicore` SE, splice-flag dies — all in V5-V10.
- **Validation:** V10 demoted to a smoke check (identity, not guard); V3 a hard literal (no `--dovetail`); V1 includes a PE-dovetail cell so the kind-gating can't regress Bowtie 2 PE.
- **Risks:** OQ-2d/e/f are bounded (a Perl read + a scoping choice each, no new design). The genuine deferred risk (PE `ZS`) is in 2b by design.

## Implementation Notes (2026-06-05)

**Status:** implemented on `rust/aligner-v1x`; **271 tests green** (228 lib + 43 integration), `clippy --all-targets -D warnings` clean, `cargo fmt --check` clean. Local only — oxy SE gate (V9) is the next step.

### What was built (by seam)
- **config.rs** — `Aligner::Hisat2` variant + `Aligner::{token,name}()` methods; `resolve_aligner` returns `Hisat2` (minimap2 still `Unsupported`, conflicts preserved); `resolve()` threads `aligner` into `discover_genome`/`detect_aligner`/`build_aligner_options`, picks `path_to_hisat2` for the HISAT2 binary, and **hard-rejects `--ambig_bam`+`--multicore(>1)`+`--hisat2`** (OQ-2d); `summary()` aligner-aware. New `mod tests` (4: select/conflicts/minimap2-deferred).
- **aligner.rs** — `detect_bowtie2`→`detect_aligner(kind, path)`; `PINNED_HISAT2_VERSION="2.2.2"`; per-kind `binary_name`/`pinned_version`/`path_flag`; version parser reused verbatim. +2 tests (hisat2 banner parse, token/name/helpers).
- **options.rs** — `build_aligner_options(cli, aligner, fmt, is_paired)`; `--dovetail` (and its `--old_flag` conflict) gated on `Bowtie2`; new `apply_aligner_specific_options` appends the HISAT2 tail to the **finished** string in Perl order `[--no-spliced-alignment][--known-splicesite-infile <f>] --no-softclip --omit-sec-seq` (8286-8326), with the both-set / missing-file / non-HISAT2 dies. +8 HISAT2 tests; existing Bowtie2 tests now pass `Aligner::Bowtie2` (prove byte-frozen).
- **discovery.rs** — `bt2_suffixes`→`index_suffixes(aligner, stem, large)` returning a `Vec` (Bowtie2 6 `.bt2`, **HISAT2 8 `.ht2`**, no `rev.*`); `aligner` threaded through `first_missing`+`discover_genome`; `FaultyIndex` carries the aligner name. +5 tests (8-arity, complete/large discovery, HISAT2 wording, 6-files-not-complete, bt2-rejected-in-hisat2).
- **error.rs** — `FaultyIndex{aligner,…}` + `AlignerNotWorking{…,path_flag}` aligner-aware (STDERR/non-gated fidelity).
- **report.rs** — `ReportHeader.aligner` + the "Bismark was run with {HISAT2|Bowtie 2}" branch (Perl 1722/1728). +1 test (HISAT2 SE header).
- **lib.rs (6 sites) + parallel.rs (10 sites)** — `_bismark_bt2*` literals → `format!("_bismark_{tok}…")` via `config.aligner.token()`, threaded ONLY into `default_suffix`/temp names (never `basename_suffix`/`_unmapped`/`_ambiguous`); both `ReportHeader` constructions in each file get `aligner`.
- **merge.rs** — +2 tests (V5): `mapped_zs` helper; SE `ZS==AS`→ambiguous, `ZS<AS`→unique-best-with-ZS-second (the SE half of the ZS path; PE read-1 asymmetry is 2b).
- **methylation.rs** — +4 tests (V6): spliced-`N` skip, multi-`N`, `N`+`D` (indels count D only), GA/OB-strand spliced. Verified the `N`-op is "verify, not implement" (Perl 4376).
- **tests/cli.rs** — `make_genome_ht2` + `make_fake_hisat2_mapped` (`hisat2`, banner `hisat2-align-s version 2.2.2`, via `--path_to_hisat2`); V7 (naming+report), V8 (`--no-spliced-alignment` echo), OQ-2d reject, single-core ambig token. `hisat2_is_deferred`→`hisat2_is_accepted_not_deferred`.

### Open questions resolved
- **OQ-2d (`--ambig_bam`):** single-core HISAT2 → **supported**, `_bismark_hisat2.ambig.bam` (Perl 1583-1586 generic `$outfile` route); the token threading produces it by construction. `--multicore(>1)` + HISAT2 + `--ambig_bam` → **hard-reject** (Perl's multicore temp-name builder only populates `@temp_ambig_bam` for Bowtie 2 — 650-711 — so multicore-HISAT2 silently drops it; failing loudly is honest, and the SE gate cell uses `--unmapped/--ambiguous`, not `--ambig_bam`).
- **OQ-2e (`--multicore` naming) — REVISED by the V9 gate:** the token is still threaded through `parallel.rs` (all 10 sites; correct + exercised for Bowtie 2), **but `--multicore`/`--parallel N>1` + `--hisat2` is now HARD-REJECTED** in `config.rs`. The gate found HISAT2's splice-site discovery is input-batch-global, so the chunked output is not byte-identical to Perl (Perl itself is not worker-invariant for HISAT2) — see the V9 finding below. Felix-approved 2026-06-05.
- **OQ-2f (splice flags):** **handled faithfully** (Perl 8286-8326), not deferred.

### Deviations from the plan (documented)
1. **`--local` + `--hisat2`:** Perl's experimental HISAT2+`--local` path pushes `--omit-sec-seq` only (8310-8312). v1 rejects `--local` for **every** aligner (off the byte-identity spine; pre-existing), so this path is intentionally not reproduced — the default endToEnd tail (`--no-softclip --omit-sec-seq`) is the only HISAT2 path v1 supports. Documented in `apply_aligner_specific_options`.
2. **Splice-flag die ordering:** the non-HISAT2 / both-set / missing-file dies live in `build_aligner_options` (called after `discover_genome`+`detect_aligner` in `resolve`), whereas Perl raises them earlier in `process_command_line`. WHICH error fires first can differ for a malformed invocation, but all are STDERR (non-gated) and all fail loudly — no silent no-op, no gate impact.
3. **error.rs / `summary()` / `RunConfig` doc comments** made aligner-aware (the plan marked these "optional fidelity"); done for honesty — all STDERR/non-gated.

### Iteration log
- **#1** Coordinated signature refactor across config/aligner/options/discovery/error/report + naming in lib/parallel; compiled clean first try.
- **#2** `discovery::incomplete_ht2_index_errors_with_hisat2_wording` failed: asserted the missing file was `BS_CT.7.ht2`, but removing one *small* `.ht2` correctly triggers the small→large fallback (Perl 7646-7800), so the error names the first missing *large* `.ht2l`. Fixed the assertion to the real fallback contract + added `six_ht2_files_is_not_a_complete_hisat2_index`. Test-bug, not code-bug.
- **#3** `hisat2_is_deferred` integration test failed (expected): `--hisat2` is now wired. Replaced with `hisat2_is_accepted_not_deferred` (asserts it progresses past selection, NOT "deferred").
- **#4** `cargo fmt --check` flagged one over-long `assert!` in tests/cli.rs; `cargo fmt` rewrapped it (cosmetic; no string contents changed).

### 🎯 V9 SE oxy byte-identity gate — DONE + PASSED (2026-06-05)
Full record in `GATE_OXY.md` (this dir) + harness `phase2a_hisat2_se_gate.sh`. `bismark_rs --hisat2` built on oxy from this worktree vs Perl v0.25.1 + HISAT2 2.2.2 + samtools 1.23.1, real GRCh38, identical argv into the same `-o`:
- **Single-core byte-identical** (decompressed SAM `@PG`-filtered + report wall-clock-filtered): SE `se_dir` 8360 rec (10k) / **844,267 (1M)**; `se_nondir` 8362 (10k) / **843,765 (1M)**; `se_pbat` 49 (10k); FastA `se_fasta` 8358 (10k). Naming token = `hisat2` (basename match).
- **🔴 V9 FINDING — `--multicore` + `--hisat2` is NOT byte-identical (now hard-rejected):** HISAT2 discovers splice sites across the whole input read set, so chunking the reads changes the spliced alignments. Verified at 1M: Perl single-core 1310 spliced ≠ Perl `--multicore 8` 1219 (Perl itself is not worker-invariant); Rust `--parallel 8` (844256/1225) ≠ Perl `--multicore 8` (844305/1219) ≠ single-core (844267/1310). 115/207 differing diff lines were spliced. → **`config.rs` hard-rejects `--multicore`/`--parallel N>1` + `--hisat2`** (subsumes the prior `--ambig_bam`+multicore reject); single-core `--hisat2` is the byte-identical path. OQ-2e revised accordingly. This supersedes the plan's "`--multicore` SE gate cell proves the token by byte-identity" — the token is still threaded (Bowtie 2 uses it), but multicore+HISAT2 is rejected.

### NOT done here (by design)
- **PE alignment / the read-1 `ZS` asymmetry fix** → **2b** (this plan does not touch the merge second-best path; the PE option string is unit-pinned but not end-to-end-gated).

## Revision History
- **rev 1 (2026-06-05):** **Split** the combined Phase 2 → 2a (this, SE core) + 2b (PE `ZS` fix + PE gate), per dual plan-review. Folded: option order **corrected** (append LAST, not "before PE tail"); **`--dovetail` suppressed** for HISAT2 PE (B-L3); **splice-flag handling/dies** (G2/L4); discovery as a **suffix-arity** change (A-§1.2a); **`parallel.rs` 7th seam** token (A-§1.2b); `ReportHeader` aligner field + fake-binary naming (A-§1.5); **append-to-finished-string** + token-only-at-`default_suffix` safety lever (A-§3); V6 **broadened**, V10 **demoted**, V5 strengthened; `--ambig_bam`+HISAT2 trace (B-L5); error.rs fidelity note. The PE read-1 `ZS` asymmetry (B-L1) → **2b**.
- **rev 0 (2026-06-05):** initial combined Phase 2 plan (now split). Dual review: `PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md`.
