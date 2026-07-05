# CODE_REVIEW_B — Phase 2a: HISAT2 wrapper core (SE byte-identical)

**Reviewer:** B (independent). **Date:** 2026-06-05.
**Target:** uncommitted diff on `rust/aligner-v1x` @ `~/Github/Bismark-aligner`, crate `rust/bismark-aligner`.
**Plan:** `plans/06052026_bismark-aligner-v1x/phase2a-hisat2-core/PLAN.md` (rev 1 + Implementation Notes).
**Oracle:** Perl `bismark` v0.25.1 (same worktree).

## Verdict

**APPROVE.** The implementation is faithful, well-scoped, and byte-identity-safe for the SE HISAT2 path. Every byte-visible change was cross-checked against the cited Perl lines and matches. Bowtie 2 is structurally byte-frozen (append-to-finished-string + token-only-at-`default_suffix`, both verified). The OQ resolutions (ambig-bam multicore reject, splice handling, parallel.rs token) are correct against the Perl source. **No Critical or High findings.** A handful of Low fidelity/test-rigor notes below; none block the SE oxy gate (V9), which remains the right next step.

- Tests: **228 lib + 43 integration = 271 green**; `clippy --all-targets -D warnings` clean (both re-run by me, confirmed).

---

## What I verified against the Perl oracle (all match)

| Area | Perl | Rust | Result |
|---|---|---|---|
| HISAT2 SE tail appended LAST (after `--quiet`) | push order 8012→8044→8125-8141→**8295-8314** | `apply_aligner_specific_options(opts.join(" "), …)` after `--quiet` | ✅ exact |
| `--no-mixed`/`--no-discordant` for BOTH aligners | 8044-8045 (`if($bowtie2)` commented out) | pushed unconditionally in `is_paired` | ✅ |
| `--dovetail` Bowtie2-only | 8051-8059 `if($bowtie2){ if($dovetail){…} }` | `if aligner==Bowtie2 && !cli.no_dovetail` | ✅ |
| `--old_flag`/`--dovetail` conflict Bowtie2-only | nested inside `if($bowtie2)` 8052-8054 | nested inside the `Bowtie2 &&` gate | ✅ (HISAT2 PE + `--old_flag` does NOT die — correct, the die is Bowtie2-gated) |
| splice both-set die | 8290 | `if cli.nosplice { if known_splices.is_some() { Err } }` | ✅ |
| splice missing-file die | 8304 | `if !infile.exists() { Err }` | ✅ |
| splice flags die in non-HISAT2 | 8319-8324 | `if aligner!=Hisat2 { if nosplice/known → Err }` | ✅ (closes the Perl Bowtie2 silent-no-op gap; honest) |
| splice flag order (before softclip) | 8295 then 8314 | `tail.push(nosplice); tail.push(known); tail.push("--no-softclip --omit-sec-seq")` | ✅ |
| index suffixes HISAT2 `{1..8}.ht2`, no rev, `.ht2l` large | 7739/7750 + 7769/7779 | `(1..=8).map(…)` with `ht2`/`ht2l` | ✅ exact arity + ext |
| index small→large fallback | 7759-7791 | `(None,None) => false; _ => check large` | ✅ |
| report "was run with HISAT2" SE | 1728 | `h.aligner.name()` substitution | ✅ |
| report "was run with HISAT2" PE | 1849 (single `\n`) | shared format string, `paired` branch unchanged | ✅ (untested unit, but shared path) |
| ZS/XS second-best parse | 2779 (`ZS` both) / 2786-2791 (`XS` Bowtie2-only) | `strip_prefix("XS:i:").or(strip_prefix("ZS:i:"))`, last wins | ✅ equivalent for real data (see L2) |
| SE ZS==AS → ambiguous | 2840-2953 same-thread amb | `if alignment_score==sb && best_as_so_far==Some(as) { amb_same_thread=true }` | ✅ V5 tie asserts `Decision::Ambiguous` |
| N-op extraction | 4372-4377 (advance pos, no genomic seq, no indels) | `b'N' => { pos += len; }` (pre-existing) | ✅ V6 asserts `indels==0` + seq |
| multicore HISAT2 drops ambig BAM | 676-684 (`else{ }` pushes output+report only) | hard-reject `--hisat2 + --ambig_bam + multicore>1` | ✅ honest fail-loud |
| `_bismark_hisat2.ambig.bam` single-core | 1583-1586 generic `$outfile` route | token threaded into `_bismark_{tok}.ambig.bam` | ✅ |
| empty Bowtie2 base string impossible | `-q`/`-f` first (7811/7816/7822) | `-q`/`-f` pushed first unconditionally → single-space join guaranteed | ✅ |

---

## Issues by area

### Logic / correctness
- **Bowtie 2 byte-frozen — confirmed structural.** `options.rs` builds the Bowtie 2 string identically and the HISAT2 delta is appended only on the finished string; the `--dovetail` push is now `aligner==Bowtie2 && !cli.no_dovetail`, so a Bowtie 2 run is bit-for-bit unchanged (the `bowtie2_pe_string_byte_frozen_with_aligner_param` + the unchanged-expected `paired_end_tail_and_default_maxins` tests both prove it). Token threading touches only `default_suffix`/temp names, never `basename_suffix`/`_unmapped`/`_ambiguous`. ✅
- **`apply_aligner_specific_options` ordering deviation — NON-GATED (confirmed).** It runs in `build_aligner_options`, called *after* `discover_genome`+`detect_aligner` in `resolve()`, so for a malformed invocation the genome/aligner error may fire before the splice die (Perl raises the splice die earlier in `process_command_line`). I traced `main.rs`: every `AlignerError` → `eprintln!("error: {e}")` + exit 1, no output files written. The byte-identity gate compares BAM/report/aux content, none of which exist on an error exit. **No gate scenario is masked**; both paths fail loudly. Deviation #2 is correctly documented. ✅
- **`--ambig_bam`+`--multicore`+`--hisat2` reject — correct + complete.** Verified Perl 676-684: the multicore SE HISAT2 `else` branch pushes only `@temp_output`/`@temp_reports`, never `@temp_ambig_bam` → Perl silently no-ops the ambiguous BAM in multicore HISAT2. Rust's multicore merge would emit one, so the hard-reject is the honest divergence. Gated on `cli.multicore.unwrap_or(1) > 1`; `--parallel` is a `visible_alias` of `--multicore` so both aliases are caught. Single-core `--ambig_bam`+HISAT2 IS supported (OQ-2d resolved correctly). ✅

### Discovery
- **`Vec` replacing the fixed `[String; 6]` does not change Bowtie 2 behavior.** Bowtie 2 still yields exactly `{1,2,3,4,rev.1,rev.2}.bt2`(+`.bt2l`); only the container type changed. `first_missing`/`discover_genome` consume the same list. ✅
- **`.ht2l` large fallback reachable + correct.** `falls_back_to_large_ht2l_index` covers the all-large case; `incomplete_ht2_index_errors_with_hisat2_wording` covers the small-incomplete→large-missing path (the iteration-log #2 fix is right: removing a small `.ht2` triggers the fallback, so the error correctly names the first missing `.ht2l`, not the removed `.ht2`). The 8-suffix arity is enforced (`six_ht2_files_is_not_a_complete_hisat2_index` + `bt2_index_rejected_in_hisat2_mode`). ✅

### Test rigor
- The HISAT2 fake (`make_fake_hisat2_mapped`) is **not** a false-pass: it keys on `*BS_CT*` in the `-x` arg (which `ct_index_basename` guarantees), maps on the CT instance and is UNMAPPED on GA → a genuine unique-best on the OT strand. The `--version` banner is the real HISAT2 shape (`hisat2-align-s version 2.2.2`) reached via `--path_to_hisat2`. ✅
- V5 (`merge.rs`) asserts the *consequence*: the tie case → `Decision::Ambiguous` + `unsuitable_sequence_count==1`; the shift case → `UniqueBest` with `alignment_score_second_best==Some(-6)`. SE-only — the PE read-1 ZS asymmetry is correctly NOT touched (2b). ✅
- V6 (`methylation.rs`) asserts genomic-seq bytes, `indels`, MD-seq, strand, and conversion counters — real byte-level checks, not "it parsed." ✅

---

## Recommendations (priority)

### Critical
None.

### High
None.

### Medium
None.

### Low (fidelity / robustness / doc — none gate-blocking; orchestrator's discretion)

- **L1 — splice-flag `warn` lines not emitted.** Perl emits `warn "Running HISAT2 without detecting spliced alignments\n"` at 8293 and 8300 when `--no-spliced-alignment`/`--known-splicesite-infile` are used. The Rust emits no such STDERR notice. STDERR/non-gated → fidelity-only; mention in the deviations list for completeness.

- **L2 — ZS/XS parse is aligner-blind (cannot bite real data, but worth a guard-comment).** Perl only consumes `XS:i:` `if ($bowtie2)` (2786); `ZS:i:` is consumed for both. The Rust `strip_prefix("XS:i:").or(strip_prefix("ZS:i:"))` (last-wins) would consume a stray `XS:i:` on a *HISAT2* record that Perl would discard. I worked the cases: for real HISAT2 2.2.2 output (ZS only, never XS) and real Bowtie 2 output (XS only, never ZS), Rust == Perl in every field order. The only divergence requires a HISAT2 record carrying an `XS:i:` tag, which does not occur in this pipeline (plan assumption: HISAT2 uses ZS). The existing comment at `align.rs:93-95` is accurate for real data; consider adding "(HISAT2 never emits XS; Bowtie 2 never emits ZS — the disjunction is safe by aligner output convention)" so a future reader doesn't mistake it for a faithful port of Perl's `$bowtie2`-gated branch.

- **L3 — `FaultyIndex` HISAT2 wording is not byte-identical to Perl.** Perl HISAT2 says `"The HISAT2 index of the C->T converted genome seems to be faulty ($file doesn't exist). Please run bismark_genome_preparation --hisat2 before running Bismark."` (7743/7791) — note the `--hisat2` in the remediation hint and the differing phrasing. The Rust `FaultyIndex` reuses the Bowtie 2-shaped message with just the aligner name swapped (drops `--hisat2`, says "non-existant"/"run the bismark_genome_preparation"). STDERR/non-gated → fidelity-only (the plan §2 flags this as fidelity-only). No action required for the gate.

- **L4 — V5 shift-test comment cites the wrong Perl arm.** `hisat2_se_zs_below_as_is_unique_best_with_zs_second` exercises the **single-entry** branch (`entries.len()==1` → `second_for_mapq = b.second_best`), not the multi-entry "best's-own arm" at `merge.rs:330-333` that the comment ("Perl 3075 best's-own arm") references. The assertion (`Some(-6)`) is correct; only the comment is slightly misleading. Cosmetic.

- **L5 — V5/V7 do not assert the final MAPQ byte.** The plan V5 says "MAPQ byte-equal" but the test asserts only the `second_best` feeding `calc_mapq` (`Some(-6)`), not the resulting `b.mapq`. Since `calc_mapq` is aligner-agnostic and already proven for the XS path, the MAPQ is identical by construction — but a one-line `assert_eq!(b.mapq, …)` would make the V5 claim literally true rather than transitively true. Optional strengthening; the real MAPQ proof is the oxy gate.

- **L6 — `--local`+`--hisat2` rejected, not reproduced (documented Deviation #1).** Perl's experimental HISAT2+`--local` pushes `--omit-sec-seq` only (8310-8312). v1 rejects `--local` for every aligner before reaching the tail. This is intentional (experimental Perl path, off the spine) and fails loudly — acceptable; already documented.

---

## Scope confirmation
- PE end-to-end (the read-1 `ZS` asymmetry) is correctly **NOT** touched here — `merge.rs` SE path and the unit-pinned PE option string are the only PE-adjacent changes; the merge second-best PE path is untouched, deferred to 2b. ✅
- The 🎯 SE oxy byte-identity gate (V9) is correctly listed as the remaining step before commit.

**Report path:** `plans/06052026_bismark-aligner-v1x/phase2a-hisat2-core/CODE_REVIEW_B.md`
