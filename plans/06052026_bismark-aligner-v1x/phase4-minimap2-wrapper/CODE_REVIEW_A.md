# CODE_REVIEW_A — Phase 4: minimap2 single-end wrapper

**Reviewer:** A (independent, fresh context). **Date:** 2026-06-05.
**Target:** uncommitted diff on `rust/aligner-mm2` (off `iron-chancellor` `49a1518`), crate `rust/bismark-aligner` (`bismark_rs`). Files: `src/{config,aligner,options,discovery,align,lib}.rs` + `tests/cli.rs`.
**Oracle:** Perl `bismark` v0.25.1 (worktree copy). **Lens:** faithfulness to the byte-identity oracle (decompressed SAM + report + aux), not idiomatic preference.

## Verdict

**APPROVE.**

Every load-bearing claim was re-derived from the Perl source and holds. The implementation is a clean, `kind`-gated wrapper: the merge/MAPQ/XM core is reused untouched, the clean-slate options + positional `.mmi` spawn are faithful, and the two now-frozen backends (Bowtie 2 / HISAT2) are byte-frozen with explicit regression guards. The PE-minimap2 reject is a clean fail-loud reached before any alignment. I found **0 Critical** and **0 Important** issues; only 3 Optional polish items (none byte-relevant, none blocking the V9 oxy gate).

---

## Re-derived load-bearing claims (verified against the Perl source)

### 1. s2-ignored / merge-MAPQ no-op — VERIFIED (Perl 2772-2796)
I read the SE parse loop directly. The tag scan is, in order: `AS:i:` (2777) → `ZS:i:` (2780, unconditional) → `MD:Z:` (2783) → `else { if ($bowtie2) { XS:i: (2788) / ZS:i: (2791) } }`. **There is no `s2:i:` branch anywhere.** minimap2 emits the lowercase `s2:i:` second-best chaining score, which never matches the case-sensitive `/ZS:i:/`, and the `$bowtie2` branch is dead for `--minimap2` → `$second_best` stays undef. The Rust `SamRecord::parse` (align.rs) strips only `AS:i:`/`XS:i:`/`ZS:i:`/`MD:Z:` — it does **not** capture `s2:i:`. **No `s2` parse branch was added.** The new test `minimap2_s2_tag_is_ignored` feeds a real minimap2 tag set (`AS:i:20 … s1:i:18 s2:i:14 … MD:Z:10`) and asserts `second_best == None` + `alignment_score == Some(20)` (positive AS, no sign assumption) + `md_tag == Some("10")`. The 🔴 comment at the tag-scan loop documents the trap (the Phase-3 spike's "read s2" instruction was WRONG). `calc_mapq` is therefore fed `(AS, None, len, (0,-0.2))` exactly as Perl — byte-identical. **Confirmed: the regression that would silently break MAPQ is NOT present, and is guarded.**

### 2. Clean-slate option string — VERIFIED (Perl 8359-8413)
I read the assembly block. `@aligner_options=()` (8359), then `push '-a'` (8362), `'--MD'` (8365), `'--secondary=no'` (8368), `'-t 2'` (8372, **a single string element**), `'-x sr'`/`'-x map-pb'`/`'-x map-ont'` (8387/8396/8403, **single string elements**), `'-K 250K'` (8413). Joined with spaces → `-a --MD --secondary=no -t 2 -x map-ont -K 250K`. The Rust `minimap2_options` returns `format!("-a --MD --secondary=no -t 2 -x {preset} -K 250K")` — **byte-identical**. Push order, preset selection (`--mm2_short_reads`→`sr`, `--mm2_pacbio`→`map-pb`, default OR `--mm2_nanopore`→`map-ont` via the shared `else`), and the 3 conflict dies (short⊕nanopore 8376, short⊕pacbio 8379, pacbio⊕nanopore 8392) all match, including the per-block die ordering. NOT `-ax sr`. V2/V3 tests pin all of it (default string, each preset, each conflict, the clean-slate wipe of Bowtie 2 flags).

**Build-then-wipe — VERIFIED (Perl 8287-8325 runs before 8359):** The HISAT2 tail block (8287) runs first; for `--minimap2` (`$hisat2=0`) the `else` (8318-8324) dies on `--no-spliced-alignment` / `--known-splicesite-infile`, and the `-N`-range / `--score_min`-shape validation in the Bowtie 2 base also runs before the clean-slate. The Rust replicates this: `build_aligner_options` builds + validates the Bowtie 2 base, calls `apply_aligner_specific_options(.., Minimap2)` (which takes the non-Hisat2 branch → dies on splice flags), and only then substitutes `minimap2_options`. So `--minimap2 -n 2` and `--minimap2 --no-spliced-alignment` both die exactly as Perl. Tests `minimap2_still_validates_bowtie2_base` + the splice-die paths cover it.

### 3. Positional `.mmi` spawn shape — VERIFIED (Perl 7022/7025)
`$mmi = $fh->{bisulfiteIndex}.".mmi"` (7022); `$minimap_commandline = "$path_to_minimap2 $mm2_options $mmi $temp_dir$fh->{inputfile}"` (7025). No `-x basename`, no `-U`, no `--norc`/`--nofw` (7011-7016 commented). The Rust `build_se_argv` Minimap2 arm does exactly: tokenize `options`, then push `index.as_os_str() + ".mmi"` (literal byte append) + `input` — **no orient flag, no `-x`/`-U`**. The Bowtie 2 / HISAT2 arm is byte-frozen (`<opts> <orient> -x <index> -U <input>`) and pinned by `se_argv_bowtie2_shape_frozen` + `se_argv_hisat2_same_shape_as_bowtie2`. `se_argv_minimap2_positional_mmi` asserts the exact argv vector AND negatively asserts no `--norc`/`--nofw`/`-U`/bare-basename; `se_argv_minimap2_orientation_independent` proves the strand flag is never emitted. The `-x map-ont` *preset* is correctly distinguished from the Bowtie 2 `-x <index>` shape (it lives inside the options string, not the index slot). **Bowtie 2 / HISAT2 argv unchanged through the refactored builder.**

### 4. Version parse — VERIFIED (Perl 7081-7084)
For `minimap2` the `elsif` body is empty (comments only) → `$aligner_version` stays the chomped raw `--version` output (bare `2.31-r1302`). The Rust `parse_minimap2_version` takes the first non-empty trimmed line; `detect_aligner` dispatches by `kind`. The version is warn-only (Perl 7089) and never reaches the gated BAM/report, so any parse difference is byte-harmless. `parses_bare_minimap2_version` also asserts the Bowtie 2 banner parser returns `None` on the bare number (so the minimap2-specific parse is genuinely required). Match.

### 5. SE convert needs NO change — VERIFIED (Perl 5584-5631)
The SE FastQ transform chomps + `fix_IDs` the identifier and re-appends `\n` (5584-5586), then writes the converted body with the **unmodified** identifier — **no `/1`, no `/1/1`, no suffix at all**. The `/1`/`/2` (mm2) vs `/1/1`/`/2/2` (others) distinction is PE-converter-only (deferred with PE). The Rust SE converter is untouched (`id_suffix=b""`). The integration test `minimap2_se_mapped_names_and_report` exercises the bare-ID lockstep end-to-end (one read maps). Match.

### 6. max-length range/default + count interaction — VERIFIED (Perl 8344-8356, 5598-5604, analysis loop 2433)
`resolve_mm2_max_length` folds both Perl blocks: outside minimap2 mode each `--mm2_*` flag dies (the `unless($mm2)` block 8329-8341, with the Perl-matching `(--pacbio)`/`(--nanopore)` wording); in minimap2 mode `--mm2_maximum_length` is range-checked `100..=100_000` (8346-8351) and defaults to `10000` when absent (8354). Tests cover `<100`/`>100000` die, the `100`/`100000` boundaries, absent→10000, and non-minimap2→None. **Count interaction (I-3):** the cutoff drops the read from the converted temp (`convert.rs` 332, `next`), but `drive_merge` (lib.rs 636) iterates the **original** read file and bumps `sequences_count` before any aligner-stream lookup — so a >cutoff read is still "analysed + no-alignment", exactly as Perl's analysis loop (2433) counts off the original input. The integration test `minimap2_max_length_drop_counts_as_no_alignment` pins 2 analysed / 1 unique / 50% / 1 no-alignment + exactly 1 BAM record. The cutoff length comparison is faithful too: Perl never chomps `$sequence` before `length$sequence` (only `$identifier` is chomped), so the newline counts; the Rust `seq.len()` (read via `read_until(b'\n')`, newline retained) matches, so a read whose base-count equals the cutoff is dropped on both sides (off-by-one is consistent, not a divergence).

### 7. PE-minimap2 reject — VERIFIED clean + reachable-first
The reject lives in `config::resolve` after `resolve_layout` and **before** `discover_genome`/`detect_aligner`/any spawn — so a `--minimap2 -1 -2` run fails at config-resolution time with `AlignerError::Unsupported` and a clear message, well before any garbage alignment. `minimap2_paired_end_is_rejected` asserts exit 1 + the "paired-end … minimap2 … not supported" text. This is faithful + safe: the Perl PE minimap2 path is unfinished WIP (`# TODO`+`sleep(1)`×2/pair, 6697-6708) and the PE report writer mislabels minimap2 as "HISAT2" (1845-1850) — no trustworthy oracle, so a hard reject is the correct choice (mirrors the HISAT2-multicore reject precedent and the SE report writer 1724-1725 which *does* have the correct `$mm2` branch).

### 8. Regression surface (two now-frozen backends) — guarded
The SE report writer (1721-1729) confirmed: `elsif ($mm2)` → "Bismark was run with **minimap2** against …" (lowercase) — the Rust `name()` returns `"minimap2"` lowercase, byte-significant, matches. Discovery: `index_suffixes(Minimap2)` = single `.mmi`; the `large` flag is a documented no-op (both probes hit the same `.mmi`), so a missing `.mmi` yields a minimap2 `FaultyIndex` (not a spurious large-index success) and `large_index` stays false — `missing_mmi_errors_with_minimap2_wording` + `discovers_complete_mmi_index` + `bt2_index_rejected_in_minimap2_mode` cover it. The `lib.rs` SE spawn call site passes `&config.detected_aligner.path` (the resolved minimap2 binary) — the variable is legacy-named `bt2` but the value is correct. `bowtie2_hisat2_strings_byte_frozen_alongside_minimap2` + the two `se_argv_*_shape_frozen` tests pin the frozen backends through the refactor.

---

## Findings

### Critical
*(none)*

### Important
*(none)*

### Optional (polish — not byte-relevant, not blocking the gate)

- **O-1 — stale "deferred" CLI doc comments.** `cli.rs` 55/220/223/226/229 still describe `--minimap2` / `--mm2_*` as "deferred to a v1.x follow-up". They are now active. Cosmetic (doc strings, not `--help`-gated byte output). Refresh when convenient.
- **O-2 — misleading local variable name `bt2`.** `lib.rs:259` binds `let bt2 = &config.detected_aligner.path;` and passes it to `AlignerStream::spawn` for all three aligners. The value is correct (the detected binary, minimap2 included); only the name is a Bowtie-2-era leftover. Rename to `aligner_bin`/`bin` for clarity. Zero behavioral impact.
- **O-3 — V9 harness reminder (carry-over from the plan, not the code).** The plan's V9 already calls for asserting zero secondary/supplementary records on every instance and run-to-run determinism at 1M. The unit/integration layer can't prove the one-primary-per-read lockstep invariant for the non-dir/pbat 4-instance both-strand population (only the directional spike exercised it). Ensure the oxy gate harness keeps those assertions — they are the only residual exposure once the static review is clean. (This is a gate-step note, not a code defect.)

---

## Could the gate pass while wrong? / could a regression hide?

- The s2-footgun is closed (no `s2` branch + a real-`s2:i:` parse test + a 🔴 code comment) — the highest-value silent-MAPQ-break risk is guarded.
- The two frozen backends are pinned by argv + option-string byte-freeze tests, and the per-aligner shape is a pure, separately-tested `build_se_argv` (the Bowtie 2 / HISAT2 path is physically the same code as before, just relocated). Low regression risk.
- The max-length default (10000) is inert on real bisulfite reads, but the count-interaction is unit/integration-tested with a forced-short cutoff, so it cannot hide behind real-data inputs at the gate.
- I did **not** rebuild/re-run the suite (target lock contention with sibling agents, per the caller's guidance; the suite was reported green at 300 tests + clippy `-D warnings` + fmt clean). All claims above were verified by reading the diff + the Perl oracle directly; the test code is in the diff and was reviewed.

---

**File:** `plans/06052026_bismark-aligner-v1x/phase4-minimap2-wrapper/CODE_REVIEW_A.md`
