# Plan — extractor console progress/diagnostics logging

## Context

The Rust `bismark-methylation-extractor-rs` is nearly silent: today it prints
only the per-file `contains data -> kept` / `was empty -> deleted` cleanup
summary (via `eprintln!` in `output.rs:319/322`). The Perl
`bismark_methylation_extractor` prints a rich diagnostic log (banner, detected
mapping mode, parameter summary, a `Processed lines: N` progress counter every
500k, and a final per-context methylation summary).

Felix's observation (2026-05-29, running the SE 10M BAM): the silence is
"arguably quite verbose [in Perl], but it helps tremendously for helping users
with debugging." On a long run, no output is indistinguishable from a hang; and
silent auto-detection of SE/PE is exactly the kind of wrong-guess that bites
users. **Goal:** give the Rust port Perl-equivalent observability.

**This is NOT on the beta.1 byte-identity critical path** and can land post-tag:
the Phase H matrix compares *output files* (`cmp` on reports/M-bias, sorted-md5
on data files) and only `tail -3`s the console for display. stdout/stderr is
outside the byte-identity invariant — verified against `phase_h_smoke.sh` /
`phase_h_pe_matrix.sh`. So adding console logging cannot regress the gate.

## Investigation findings (informs the design)

- **Perl streams everything diagnostic to STDERR** via `warn` (232 calls; **0**
  `print STDOUT`, **0** `print STDERR`, 2 trivial bare prints). Data → files via
  `print REPORT`. Confirmed lines: param summary (`bismark_methylation_extractor:54`),
  mode detection (1172/1177), progress (1553/1850), kept/deleted (607/615),
  final report `warn`'d (2562) **and** written to file (2510).
- **Rust already uses the right channel**: the kept/deleted lines go to stderr
  via `eprintln!` (`output.rs`). No `log`/`tracing`/`indicatif` dependency — raw
  `eprintln!` is the existing idiom; we keep it (no new dep).
- **Header lines: split by record type, don't blanket-drop** (Felix correction,
  2026-05-29). Perl `warn`s every header line indiscriminately. But:
  - `@SQ` (190+ contig dictionary) — genuine noise; suppress by default.
  - `@PG` — **valuable provenance** (Bismark version + exact CL that produced the
    BAM, samtools chain). Emit by default.
  - `@HD` — format version + sort order; mildly useful, emit by default.
  Because the Rust port reads the header **structurally via noodles**, it can
  emit just `@PG`/`@HD` and skip the `@SQ` flood — cleaner than Perl's
  all-or-nothing line dump. (`@SQ` can be exposed under `--verbose` for the rare
  case someone wants the full dictionary on screen.)
- The **final methylation numbers already exist** in the Rust port (they're
  computed for the byte-identical `_splitting_report.txt`). Echoing them to
  stderr is a near-free mirror, not a new computation.

## Behavior

Decision: **all new output → stderr** (matches Perl `warn` + existing Rust
`eprintln!`; keeps stdout clean for any future piping). `--version` stays on
**stdout** (`main.rs:37`, correct and unchanged).

Default **on**; add `-q`/`--quiet` to suppress the *informational* log
(banner, params, mode, header provenance, progress, final summary,
kept/deleted). `--quiet` must NOT suppress genuine warnings/errors. **Audit of
existing `eprintln!` sites** (rev 1 — both reviewers): classify each as info
(gate) vs warning/error (never gate):
- `output.rs:319/322` kept/deleted → info (gate).
- `output.rs:317` AND `output.rs:369` — **two** `failed to remove` warnings →
  never gate (genuine warnings).
- `main.rs:44` `error:` → never gate.
- `subprocess.rs:411` `[bismark-extractor] spawning:` (Phase G) → always-on info
  today; **decision**: gate under `--quiet` for consistency (it's informational).
- `subprocess.rs:519` (cytosine w/ no kept files) → warning, never gate.
Add `--verbose` ONLY to opt the `@SQ` reference dictionary back in.

Log lines to add (stderr), Perl-equivalent wording where it aids familiarity:
1. **Banner**: `*** Bismark methylation extractor (Rust) version <crate ver> ***`
2. **Mode detection**: `Treating file(s) as {single|paired}-end data (auto-detected from @PG)` or `(forced via -s/-p)`. (No Perl `sleep(1)` — that's cosmetic; omit.)
3. **Parameter summary**: cores/`--parallel`, output dir, ignore_{5p,3p,r2,3p_r2}, overlap mode, comprehensive/merge flags, gzip.
4. **Header provenance**: emit `@HD` + each `@PG` record (Bismark version + CL,
   samtools chain) **as verbatim SAM-text lines** (preserves `@PG` order +
   exact `CL:` text). Suppress `@SQ` unless `--verbose`. Via a serialize-header-
   to-text + line-filter helper (NOT field-walking the noodles `Map<Program>`,
   which can reorder tags — rev 1).
5. **Progress**: `Processed lines: N` every 500_000, ticked off the existing
   `call_strings_processed` counter (+1/SE record, +2/PE pair) so it matches
   Perl byte-for-byte (rev 1 — not a per-pair tick).
6. **Final summary**: total methylation call strings, total C's analysed, per-context methylated/unmethylated counts + percentages (mirror of splitting_report).

## Implementation outline

> **rev 1 (2026-05-29) — both plan-reviewers, grounded against the code.**
> My original "dual-dispatch" claim was WRONG: `main.rs` dispatches EVERY
> `--parallel` value (incl. N=1) to `parallel.rs::extract_{se,pe}_parallel`.
> `route.rs` is a per-*call* router (no read loop); `pipeline.rs::extract_{se,pe}`
> are **test-only** byte-identity oracles (`lib.rs`). There is a SINGLE live read
> site — `parallel.rs::producer_loop` — which is single-threaded, so no atomic is
> needed. [[feedback_dual_driver_back_port]] does NOT apply here.

1. **CLI (`cli.rs`)**: add `-q`/`--quiet` (short+long) and `--verbose`
   (**long-only** — `-v` is free but reserve it; `-V`/`--version` already taken,
   clap auto-version disabled). Thread into `Config`. Define a **testable**
   `Logger { quiet, verbose }` (write to an `impl Write` / return strings, not a
   hardcoded `eprintln!`) so the gate is unit-testable. `-q` + `--verbose`
   together: `--quiet` wins (silence everything informational, incl. `@SQ`).
2. **Single emission chokepoint = `run_pipeline` (`parallel.rs:177`)**, right
   after `build_chr_name_table` — header + `is_paired` are both in hand there for
   SE and PE. Emit banner + mode + params + header provenance once, before the
   producer starts. (NOT `pipeline.rs`.)
   - **Header provenance**: REUSE the proven idiom from
     `bismark-io::detect_paired_from_header` (`read.rs:649`) — it serializes the
     noodles header to SAM text and walks lines. Filter those lines to `@HD` +
     `@PG` and print **as-is** (byte-faithful, preserves `@PG` ordering
     Bismark→samtools→samtools.1, sidesteps noodles `Map<Program>` tag
     reordering). Skip `@SQ` unless `--verbose`. Expose a small helper in
     `bismark-io` (e.g. `header_provenance_lines(&Header, include_sq) -> Vec<String>`).
3. **Progress counter** — `Processed lines: N` every 500_000, in
   `parallel.rs::producer_loop` (the one live site). Plain **local** counter (no
   atomic — single producer thread). **Reuse Perl's exact semantics**: the port
   already tracks `call_strings_processed` (+1 per SE record, +2 per PE pair);
   tick the progress off THAT so the number matches Perl byte-for-byte on the
   same BAM. Do NOT use a per-pair tick (would show half Perl's count on PE).
4. **Final summary** — emit where the splitting-report stats are finalized
   (the counts feeding `_splitting_report.txt`), reusing those values. No
   recomputation. Mirror Perl's wording (call strings, C's analysed, per-context
   counts + %).
5. **kept/deleted lines** (`output.rs:319/322`) — already stderr; route through
   the same `Logger` quiet-gate.
6. **Banner** uses `env!("CARGO_PKG_VERSION")` (→ `1.0.0-beta.1`), NOT the
   Perl-locked `BISMARK_VERSION` constant (which is pinned to v0.25.1 for
   output-file headers and must stay that way).

## Assumptions (rev 1 — verified by reviewers)

- **VERIFIED**: single live read site is `parallel.rs::producer_loop`; header +
  `is_paired` available at `run_pipeline` (`parallel.rs:177`). `route.rs` /
  `pipeline.rs` are not runtime read loops.
- **VERIFIED**: `call_strings_processed` already exists with +1/SE, +2/PE
  semantics — reuse it for the progress tick (matches Perl exactly).
- **VERIFIED**: clap `-V`/`--version` taken (auto-version off); `-q`/`--quiet`/
  `--verbose` free.
- Header serialize-to-SAM-text path exists (`detect_paired_from_header` uses it);
  a `bismark-io` provenance helper will reuse it. Confirm the noodles version's
  header `to_string`/writer round-trips `@PG CL:` verbatim.
- No new crate; `Logger` helper over `eprintln!`, made unit-testable.

## Edge cases

- `--quiet` suppresses info only; the two `failed to remove` warnings
  (`output.rs:317`,`:369`), `subprocess.rs:519`, and `main.rs:44` errors still print.
- `--parallel N` progress: single-threaded producer → plain local counter, no
  interleaving (the atomic concern was based on the wrong-location assumption).
- `--mbias_only` / `--yacht` / `--comprehensive`: final summary + params still
  print sensibly (guard against referencing files that mode didn't produce).
- Empty input / 0 reads: progress prints nothing; final summary shows zeros, not a panic.
- `--version`: stdout only, no banner duplication.
- `-q` + `--verbose`: `--quiet` wins.

## Verification

1. **Automated tests** (rev 1 — both reviewers; don't rely on manual eyeballing):
   - `Logger` unit tests: info gated by `--quiet`, warnings/errors NOT gated.
   - Progress cadence: a synthetic N-record run emits the counter at the right
     boundaries with Perl-matching counts (+1/SE, +2/PE).
   - `header_provenance_lines` test: `@HD`+`@PG` emitted in order, `@SQ` excluded
     unless `--verbose`, `@PG CL:` text verbatim.
2. **On colossal**, run SE + PE 10M; eyeball stderr against Perl (banner, mode,
   params, `@PG` provenance, progress cadence, final %s). Confirm stdout empty
   (`1>/dev/null` keeps the log; `2>/dev/null` silences it). `-q` → no info but a
   forced bad-path error still prints.
3. **Byte-identity unaffected**: re-run `phase_h_smoke.sh` on one SE + one PE
   cell **including an AutoDetect cell** (guards the header-read path) — expect
   PASS (console changes don't touch output files).
4. `cargo test -p bismark-extractor` + `cargo clippy` clean under `-D warnings`.
5. Progress counter present at the single live site (`parallel.rs::producer_loop`);
   confirm it fires at both `--parallel 1` and `--parallel 4` (both route through
   that producer).

## Implementation notes (2026-05-29)

Branch `feat-extractor-console-logging` off `rust/iron-chancellor` (`0b4c0de`).
All rev-1 corrections applied. Status: **101 lib tests (3 new) + all integration
binaries pass; clippy clean under `-D warnings`**; `--help` shows `-q/--quiet`
and `--verbose`.

Files:
- **`src/logging.rs` (new)** — `Logger { quiet, verbose }` (Copy) with gated
  `info`/`note`/`banner`/`parameters`/`header_provenance`/`progress`/`final_summary`;
  pure builders `header_provenance_lines` / `filter_header_text` /
  `parameters_text` / `final_summary_text`. 3 unit tests (quiet gate, summary
  shape+%, `@SQ`-drop/`@HD`+`@PG`-keep).
- `src/cli.rs` — `-q/--quiet` + `--verbose` (long-only) on `Cli`; `quiet`/`verbose`
  on `ResolvedConfig` + mapping.
- `src/parallel.rs` — `run_pipeline` emits banner/params/provenance once (before
  the reader moves to the producer); `producer_loop` gains a `logger` param + a
  `tick` counting **each record read** (+1 SE, +2 PE) → `Processed lines: N` every
  500k.
- `src/state.rs` — `finalize` builds the logger, passes it to the sweep, and
  emits the final summary after `write_splitting_report`.
- `src/output.rs` — `finalize_with_empty_sweep(logger)` gates kept/deleted +
  trailing blanks; the `failed to remove` warning stays ungated.
- `src/subprocess.rs` — `RealRunner { quiet }` gates the `spawning:` line.
- Test `ResolvedConfig` literals updated (output.rs, subprocess.rs, phase_g.rs);
  `phase_g_realrunner.rs` updated for the `RealRunner` field.

### Deviations from rev-1 plan (documented)
1. **Provenance helper lives in `bismark-extractor` (`logging.rs`), NOT
   `bismark-io`.** The extractor already depends on `noodles-sam =0.85.0`
   directly, so a local impl avoids bumping `bismark-io` (hard-`=`-pinned by both
   extractor AND dedup) and the dedup pin cascade. Same serialize-to-SAM-text
   idiom as `detect_paired_from_header`; zero behavioural difference.
2. **Progress counter counts records at the read site (producer)** rather than
   reusing the worker-side `call_strings_processed`. The producer reads in order
   (one `records_iter.next()` = one SAM line), giving Perl's exact `line_count`
   (+1 SE, +2 PE) without cross-thread aggregation — the more faithful + simpler
   site. Same resulting N as `call_strings_processed`.
3. `(*n).is_multiple_of(500_000)` per clippy (stable since 1.87 ≤ MSRV 1.89).

### Pending
- Visual eyeball on colossal (plan Stage 2) — no on-disk BAM fixture locally.
- `phase_h_smoke.sh` byte-identity regression (SE + PE + AutoDetect cell).

## Out of scope

- bedGraph/cytosine subprocess (Phase G) already stream their own Perl output.
- Structured/JSON logging, log levels beyond quiet, a `log` crate — defer.
- M-bias plot drawing (Perl's GD::Graph note) — not applicable.
