# CODE_REVIEW_B — Phase 4: minimap2 single-end wrapper (byte-identity)

**Reviewer B** · 2026-06-05 · independent / fresh context / adversarial.
**Target:** uncommitted diff on `rust/aligner-mm2` (off `iron-chancellor` `49a1518`),
crate `rust/bismark-aligner` (`bismark_rs`). Files: `config.rs`, `aligner.rs`,
`options.rs`, `discovery.rs`, `align.rs`, `lib.rs`, `tests/cli.rs`.
**Oracle:** Perl `bismark` v0.25.1 (worktree copy) + minimap2 2.31-r1302.
**Gate:** decompressed-SAM + report byte-identical to Perl. SE only (PE rejected).

---

## Verdict: **APPROVE** — no Critical, no Important findings.

The implementation is a faithful, well-scoped SE minimap2 wrapper. Every
load-bearing claim in the plan I re-derived **directly from the Perl source**
(not from the plan) and confirmed in the Rust diff. The merge/MAPQ/XM core is
genuinely reused unchanged; the wrapper deltas (clean-slate options, positional
`.mmi`, single-`.mmi` discovery, bare version parse, PE reject, max-length) are
each byte-correct against the oracle. The two highest-risk traps the plan-reviews
flagged — the `s2:i:` mis-capture and the SE `/1` suffix — are both handled
correctly **and** guarded by tests. 300 tests green (253 lib + 47 integ),
clippy `-D warnings` clean, `cargo fmt --check` clean (all re-run by me).

Three Optional notes below (all deferred-to-oxy-gate items already in the plan,
not code defects).

---

## Oracle claims I re-derived myself (not trusted from the plan)

| Claim | Perl cite (verified) | Rust (verified) | Result |
|---|---|---|---|
| Option order + literal | 8362→8413 push order `-a`,`--MD`,`--secondary=no`,`-t 2`,`-x <p>`,`-K 250K` | `options.rs:243` `format!("-a --MD --secondary=no -t 2 -x {preset} -K 250K")` | **MATCH** |
| Preset routing + conflict-die nesting | 8374-8408 (`short{nano,pb die}`/`elsif pacbio{nano die}`/`else map-ont`); default sets `$mm2_nanopore=1`→`map-ont` | `options.rs:228-242` identical nesting; `--mm2_nanopore` alone → `map-ont` | **MATCH** |
| Positional `.mmi`, no strand flag, no `-x`/`-U` | 7022 `$mmi=$bisulfiteIndex.".mmi"`; 7025 `$opts $mmi $reads`; 7011-7016 `--norc`/`--nofw` commented | `align.rs:198-203` positional `index+".mmi"`, no orient, no `-x`/`-U` | **MATCH** |
| `.mmi` literal byte append (dot-safe basename) | `$bisulfiteIndex.".mmi"` (string concat) | `OsString::push(".mmi")` on the bare `…/BS_CT` basename | **MATCH** |
| No `s2:i:` second-best branch | SE parse 2772-2796: `AS`,`ZS`(uncond),`MD`,`XS`/`ZS`(gated `if $bowtie2`); `s2` lowercase never matches `/ZS:i:/` | `align.rs:106-117` strips only `AS:i:`/`XS:i:`/`ZS:i:`/`MD:Z:` → `second_best=None` | **MATCH** |
| Version parse no-op | 7081-7084 minimap2 `elsif` does nothing → bare chomped `2.31-r1302`; warn-only (7089) | `aligner.rs:158-164` first non-empty trimmed line | **MATCH** (warn-only, can't break gate) |
| SE report writer has `$mm2` branch | 1724-1725 `elsif($mm2)` "Bismark was run with minimap2 … options: $aligner_options" | `report.rs:68-69` `aligner.name()`="minimap2" + `aligner_options` verbatim | **MATCH** |
| SE convert appends NO suffix | `biTransformFastQFiles` 5489-5651: id is chomp→fix→`\n`, no `s/$/\/1/` | `convert.rs` SE `id_suffix=b""` (unchanged) | **MATCH** |
| `/1` (mm2) vs `/1/1` is PE-only | 5945-5959 inside `if($read_number==1)` PE converter | `convert.rs:197-200` `pe_id_suffix` only; PE-mm2 unreachable (rejected) | **MATCH** |
| Max-length `>` (not `>=`), default 10000, `100..=100000` | 5600 `length$sequence > $cutoff`; 8346/8349 `<100`/`>100000` die; 8354 default 10000 | `convert.rs:333` `seq.len() > cutoff`; `config.rs:resolve_mm2_max_length` `!(100..=100_000)` + `None→Some(10000)` | **MATCH** |
| Dropped read still analysed + no-align | analysis loop 2413-2444 reads the **original** file, `++sequences_count`, `check_results_single_end`→no match | `lib.rs` drive-merge counts original reads; test `…drop_counts_as_no_alignment` (2 analysed/1 unique/1 no-align/50%) | **MATCH** |
| `-N`/splice dies fire before the wipe | `-N` die 7867; splice dies 8319-8324 — both before `@aligner_options=()` 8359 | `options.rs:192-198` build-then-substitute; `apply_aligner_specific_options` dies for non-HISAT2 | **MATCH** |

---

## Adversarial angles — findings

**1. `s2:i:` parse branch (the spike's WRONG instruction).** NOT present.
`align.rs:106-117` captures only `AS:i:`/`XS:i:`/`ZS:i:`/`MD:Z:`. minimap2's
`s2:i:` is lowercase and is neither stripped nor matched. Test
`minimap2_s2_tag_is_ignored` feeds a real minimap2 tag set incl. `AS:i:20` +
`s2:i:14` and asserts `second_best==None` (and positive AS captured — no
Bowtie2 sign assumption). A 🔴 code comment at `align.rs:99-104` documents the
deliberate omission. **No MAPQ-divergence path.** Resolved correctly.

**2. Option string byte-for-byte.** Exactly `-a --MD --secondary=no -t 2 -x
map-ont -K 250K` — order, spacing, `map-ont` (not `sr`), and it flows VERBATIM
into the SE report "run with" line (`report.rs` uses `aligner_options`
unchanged). `split_whitespace()` tokenizes `-t 2`/`-x map-ont`/`-K 250K` into
separate argv items, matching Perl's shell-split of the option string.
Asserted by `minimap2_default_option_string` + the integration report check.

**3. SE read-id suffix.** SE appends nothing — `convert.rs` SE `id_suffix=b""`
is unchanged (byte-frozen by non-modification). Re-derived from Perl 5489-5651
(no `s/$/\/1/` in the SE transform). No `/1` desync.

**4. Positional `.mmi` + dropped strand flag.** `build_se_argv` Minimap2 arm:
no `orient.flag()`, no `-x`, no `-U`; index passed as `index + ".mmi"`
positionally. The `.mmi` is appended to the END of the basename, so a basename
or path containing a dot is handled correctly (literal concat). Tests
`se_argv_minimap2_positional_mmi` + `…_orientation_independent` pin it. NB: the
`-x map-ont` *preset* inside the options is distinct from the Bowtie2 `-x
<index>` shape — verified the minimap2 path never emits `-x <index>`.

**5. Build-then-wipe validations.** `apply_aligner_specific_options` runs first
for minimap2 (dies on `--no-spliced-alignment`/`--known-splicesite-infile` via
the non-HISAT2 branch), and the `-N` range die fires in the base assembly
(Perl 7867, before the 8359 wipe). Tests `minimap2_still_validates_bowtie2_base`
(`-n 2` dies) + `minimap2_clean_slate_discards_bowtie2_flags`. Mirrors Perl order.

**6. Max-length boundaries + count interaction.** `100..=100_000` inclusive
(matches Perl `<100`/`>100000` die → 100 and 100000 valid). Default 10000 when
absent in minimap2 mode; `None` for non-minimap2 (guard inert). Convert-side
comparison is strict `>` (length INCLUDES the `\n` since `read_until` retains
it — matching Perl's un-chomped `length$sequence`, same off-by-one on both
sides). The dropped read is still counted as analysed + no-alignment, proven
end-to-end by `minimap2_max_length_drop_counts_as_no_alignment`. No off-by-one.

**7. Bowtie 2 / HISAT2 regression.** All deltas are `kind`-gated. The minimap2
option substitution is a guarded early return; Bowtie2/HISAT2 strings are
re-pinned by `bowtie2_hisat2_strings_byte_frozen_alongside_minimap2` and the
argv shape by `se_argv_bowtie2_shape_frozen` / `se_argv_hisat2_same_shape_as_bowtie2`.
`index_suffixes`/detection/multicore-temp-names all dispatch on `aligner`/`token()`
with no Bowtie2 hardcoding. Full 300-test suite green (no regressions).

**8. PE-minimap2 reject.** Reachable + loud: `config.rs:245-252` rejects after
`resolve_layout` (before `discover_genome`), `AlignerError::Unsupported` → exit
1 with "paired-end … minimap2 … not supported". Test
`minimap2_paired_end_is_rejected` confirms. The PE converter's `/1/1` suffix is
therefore dead-for-minimap2 (never reached) — correct, no silent wrong output.

---

## Optional (deferred-to-gate, not code defects)

- **O-1 (V9 oxy gate, already planned):** `map-ont` default determinism at 1M
  (`-t 2`, `-K 250K` multi-minibatch) and the non-dir/pbat 4-instance both-strand
  population are validated only at the oxy gate. Code is correct; the numeric
  byte-identity is the gate's job. The plan's V9 already demands a
  zero-secondary/zero-supplementary assertion across all instances — keep it.
- **O-2 (`--multicore --minimap2`):** allowed (no reject, unlike HISAT2),
  consistent with the plan's OQ-4d lean ("expect worker-invariance"). The
  multicore temp/output names use `token()`→`mm2` correctly. Worker-invariance is
  UNVALIDATED until the V9 multicore cell — if it diverges, add a fail-loud
  reject like HISAT2. No code issue today.
- **O-3 (version trim vs Perl chomp):** `parse_minimap2_version` trims
  leading/trailing whitespace where Perl only `chomp`s. Warn-only field, never
  in the gated BAM/report — byte-harmless. No change needed.

---

## Build / lint / test (re-run by me, sandbox-disabled)

- `cargo test -p bismark-aligner --lib` → **253 passed; 0 failed.**
- `cargo test -p bismark-aligner --tests` → **47 passed; 0 failed.**
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → clean.
- `cargo fmt -p bismark-aligner -- --check` → clean.

---

## Summary

Critical: **0** · Important: **0** · Optional: **3** (all gate-deferred, planned).

The Phase-4 SE minimap2 wrapper is byte-faithful to the Perl oracle on every
path I could re-derive, the two known byte-identity traps (`s2:i:`, SE `/1`) are
correctly avoided and test-guarded, and the Bowtie 2 / HISAT2 backends are
provably frozen. Recommend proceeding to the V9 oxy byte-identity gate (carrying
the planned zero-secondary/supplementary + 1M-determinism assertions, and a
`--multicore` SE cell with a fail-loud fallback if it diverges).

**File:** `plans/06052026_bismark-aligner-v1x/phase4-minimap2-wrapper/CODE_REVIEW_B.md`
