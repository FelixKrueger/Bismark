# Plan Coverage Report

**Mode:** B (code vs. plan — the design PLAN.md is the spec)
**Plan(s):** `phase3-single-instance-align-parse/PLAN.md`
**Code:** `rust/bismark-aligner/src/align.rs` + `pub mod align;` in `src/lib.rs`
**Date:** 2026-06-01
**Verdict:** COMPLETE

## Summary

- Total items: 33 (5 behavior steps · 11 signature items · 5 outline steps · 15 validations + the §13 deviation, de-duplicated where they overlap)
- DONE: 33
- PARTIAL: 0
- MISSING: 0
- DEVIATED: 1 (documented + verified real — counted under DONE)

Tests: `cargo test -p bismark-aligner` → **49 lib + 15 integration (cli) + 0 doc — all PASS**; the
`align::` module contributes **13 tests, all green**.

## Coverage ledger

### §3 Behavior

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 1 | 3.1 Spawn one instance — args `<options split_whitespace> <orient> -x <index> -U <input>`, `stdout(piped)`, `stderr(inherit)` | §3.1 / §5.3 | DONE | `spawn()` L154-164: `options.split_whitespace()` loop, then `orient.flag()`, `-x`, `-U`; `Stdio::piped()`/`Stdio::inherit()`. Arg order mirrors Perl. |
| 2 | 3.2 Skip SAM header — discard every `^@` line; first non-`@` = first record; `None` at EOF | §3.2 / §5.3 | DONE | L178-190: `read_line` loop, `if line.starts_with('@') { continue }`; `n==0 → break None`; else parse + store. |
| 3 | 3.3 Parse core fields by index (0,1,2,3,4,5,9,10) | §3.3 / §5.1 | DONE | `parse()` L110-122 maps f[0]→qname … f[9]→seq, f[10]→qual. RNAME kept raw (L113). |
| 4 | 3.3 Tag scan from field 11+ in field order; `AS:i:`→score, `XS:i:`/`ZS:i:`→second_best (last-wins), `MD:Z:`→md_tag; disjoint prefixes | §3.3 | DONE | L97-108: iterate `&f[11..]`, `strip_prefix` chain; XS/ZS share branch via `.or_else`, overwrite = last-in-order wins. |
| 5 | 3.3 Numeric policy: `AS/XS/ZS` parse to `i64` (accept negatives); unparseable tag → `None` | §3.3 | DONE | `v.parse::<i64>().ok()` (L99, L104) — negatives accepted, parse failure yields `None`. |
| 6 | 3.3 `< 11` tab fields → parse error | §3.3 | DONE | L77-82: `if f.len() < 11 { return Err(Validation(...)) }`. |
| 7 | 3.3 `raw_line` = the CHOMPED line (no trailing `\n`/`\r`) | §3.3 | DONE | L75 `trim_end_matches(['\n','\r'])`; `raw_line: trimmed.to_string()` L122. |
| 8 | 3.3 `is_unmapped()` = `flag == 4` (SE) | §3.3 / §4 | DONE | L127-129 `self.flag == 4`. |
| 9 | 3.4 `advance()` reads next line, parses, sets `current` (or `None` at EOF) | §3.4 / §5.4 | DONE | L207-216: `read_line` into reused `line_buf`; `n==0 → None` else `Some(parse)`. |
| 10 | 3.4 `current()` peeks without consuming | §3.4 / §4 | DONE | L202-204 `self.current.as_ref()`. |
| 11 | 3.5 `finish()` drains stdout BEFORE `wait()` (no full-pipe deadlock) | §3.5 / §5.4 | DONE | L223 `io::copy(&mut self.reader, &mut sink())` then L224 `child.wait()`; sets `finished=true`. |
| 12 | 3.5 Non-zero exit → error (intentional fail-closed) | §3.5 | DONE | L226-232: `status.success()` → Ok else `Err(Validation("exited unsuccessfully"))`. |
| 13 | 3.5 `Drop` does `kill()` THEN `wait()` | §3.5 / §5.4 | DONE | L236-244: `if !self.finished { kill(); wait(); }`. |
| 14 | 3.5 stderr inherited (no stderr-pipe to drain) | §3.5 | DONE | `Stdio::inherit()` L164; only stdout piped + drained. |

### §4 Signatures

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 15 | `SamRecord` struct: all 12 fields w/ correct types | §4 | DONE | L40-67 — qname/flag(u16)/rname/pos(u32)/mapq(u8)/cigar/seq/qual, `alignment_score:Option<i64>`, `second_best:Option<i64>`, `md_tag:Option<String>`, `raw_line:String`. Exact match. |
| 16 | `SamRecord::parse(&str) -> Result<SamRecord>` | §4 | DONE | L74. |
| 17 | `SamRecord::is_unmapped(&self) -> bool` | §4 | DONE | L127. |
| 18 | `AlignerStream` struct (Child + BufReader<ChildStdout> + current) | §4 | DONE | L133-139 (also `line_buf` for reuse, `finished` guard flag). |
| 19 | `spawn(bowtie2,&Path; options:&str; orient; index:&Path; input:&Path) -> Result<Self>` | §4 | DONE | L147-153 — signature matches exactly. |
| 20 | `current(&self) -> Option<&SamRecord>` | §4 | DONE | L202. |
| 21 | `advance(&mut self) -> Result<()>` | §4 | DONE | L207. |
| 22 | `finish(self) -> Result<()>` | §4 | DONE | L222 (`mut self`; consumes — matches `self`-by-value). |
| 23 | `enum Orientation { Norc, Nofw }` → `--norc`/`--nofw` | §4 / §5.2 | DONE | L20-36; `flag()` maps Norc→`--norc`, Nofw→`--nofw`. |

### §5 Implementation outline

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 24 | Step 1: SamRecord + parse + is_unmapped, unit-tested first | §5.1 | DONE | Covered by items 3-8,16,17; 9 parser unit tests. |
| 25 | Step 2: Orientation enum | §5.2 | DONE | Item 23. |
| 26 | Step 3: spawn (args, piped/inherited, BufReader, header-skip, store first) | §5.3 | DONE | Items 1,2. |
| 27 | Step 4: current/advance/finish + Drop kill-guard | §5.4 | DONE | Items 9-14. |
| 28 | Step 5: tests — parser units + fake-bowtie2 SAM emitter for end-to-end | §5.5 | DONE | 9 parser + 4 fake-bowtie2 integration tests (hermetic `#!/bin/sh` emitter, L320-329). |

### §9 Validation table

| # | Verify | Test | Status |
|---|--------|------|--------|
| V1 | parse core fields | `parse_core_fields` | DONE |
| V2 | tag scan AS/XS/MD (+ ZS) | `parse_tags` + `parse_negative_as_and_hisat2_zs` | DONE |
| V3 | unmapped flag==4, AS/MD may be None | `unmapped_record` | DONE |
| V4 | missing AS/MD on mapped → None (no die) | `unique_alignment_has_no_second_best` (no XS→None) + `unmapped_record` (AS/MD None) | DONE |
| V5 | header skip + first record | `stream_skips_header_then_walks_records_to_eof` | DONE |
| V6 | advance to EOF | `stream_skips_header_then_walks_records_to_eof` | DONE |
| V7 | finish reaps + exit status (0 Ok / 1 Err) | `stream_skips_...` (exit 0 Ok) + `finish_errors_on_nonzero_exit` (exit 1 Err) | DONE |
| V8 | spawn failure on bad path | `spawn_fails_on_bad_path` | DONE |
| V9 | empty/all-header stream | `all_header_stream_has_no_records` | DONE |
| V10 | realistic line — RNEXT/PNEXT/TLEN (`*`/`0`/`0`) before SEQ/QUAL | `parse_core_fields` (MAPPED const has `*\t0\t0` at f6-8; asserts SEQ=f9, QUAL=f10) | DONE |
| V11 | both XS and ZS → last-in-order wins | `both_xs_and_zs_last_wins` | DONE |
| V12 | unique, no XS/ZS → second_best None | `unique_alignment_has_no_second_best` | DONE |
| V13 | short line (<11) → parse error | `short_line_errors` | DONE |
| V14 | CRLF row + raw_line trim | `crlf_trimmed_and_raw_line_clean` | DONE |
| V15 | early-stop / partial read (no deadlock/zombie) | `early_stop_does_not_deadlock_or_zombie` (~5000 records > 64K pipe buffer; reads 1 then finish) | DONE |

### §13 Documented deviation

| # | Item | Source | Status | Notes |
|---|------|--------|--------|-------|
| 29 | Module named `align.rs` beside existing `aligner.rs` | §13 | DEVIATED (documented + REAL) | Both files confirmed present on disk: `src/align.rs` (14343 B) and `src/aligner.rs` (4380 B). `lib.rs` L19-20 declares `pub mod align;` then `pub mod aligner;`. The §13 note documents the close-but-distinct names (align = the alignment *stream*; aligner = binary *detection*); module-level doc comment (align.rs L1-11) disambiguates. Deviation matches the approved plan (PLAN §2/§5 already specify `align.rs`), so it is not a divergence from spec — it is the documented naming choice. |

## Specifically-required guard checks (from the audit brief)

| Guard | Present? | Location |
|-------|----------|----------|
| `^@` header-skip | YES | align.rs L186 `line.starts_with('@')` |
| `flag == 4` unmapped | YES | align.rs L128 |
| `< 11`-field error | YES | align.rs L77-82 |
| Field-order tag scan w/ last-wins (XS/ZS) | YES | align.rs L97-108 (`.or_else` shared branch, overwrite) |
| `raw_line` chomped | YES | align.rs L75 + L122 (`trim_end_matches(['\n','\r'])`) |
| `finish()` drains-before-wait | YES | align.rs L223 (`io::copy` to sink) before L224 (`wait`) |
| `Drop` kill+wait | YES | align.rs L236-244 |
| `>64K` early-stop test | YES | `early_stop_does_not_deadlock_or_zombie` — ~5000 records exceed the 64K stdout pipe buffer |

## Gaps (detail)

None.

## Test verification (Mode B)

| Test name | File | Status |
|-----------|------|--------|
| parse_core_fields | src/align.rs | PASS |
| parse_tags | src/align.rs | PASS |
| parse_negative_as_and_hisat2_zs | src/align.rs | PASS |
| both_xs_and_zs_last_wins | src/align.rs | PASS |
| unique_alignment_has_no_second_best | src/align.rs | PASS |
| unmapped_record | src/align.rs | PASS |
| short_line_errors | src/align.rs | PASS |
| crlf_trimmed_and_raw_line_clean | src/align.rs | PASS |
| stream_skips_header_then_walks_records_to_eof | src/align.rs | PASS |
| all_header_stream_has_no_records | src/align.rs | PASS |
| finish_errors_on_nonzero_exit | src/align.rs | PASS |
| spawn_fails_on_bad_path | src/align.rs | PASS |
| early_stop_does_not_deadlock_or_zombie | src/align.rs | PASS |

Full crate run: `49 passed; 0 failed` (lib) + `15 passed; 0 failed` (cli integration). No regressions
in Phase-1/2 modules (config, convert, discovery, options, aligner, cli), confirming the §7 promise that
`align` is **not wired into `run()`** and binary behavior is unchanged.

## Notes on minor count discrepancy (non-gap)

PLAN §13 / §10 claim "49 unit + 15 integration tests" for the *phase*. The actual lib total is 49 (which
includes Phase-1/2 modules), of which `align::` contributes 13; the "15 integration" refers to the 15 cli.rs
integration tests (Phase-1/2). The phase's own §9 calls for exactly 15 validations, all of which are covered
by the 13 align tests (V4 and V7 are each satisfied by two existing tests; V5/V6 share one stream test).
Every §9 row maps to a real, passing test — no validation is unmet. This is a wording imprecision in the
notes, not a coverage gap.

## Verdict

**COMPLETE.** All 5 §3 behavior steps, all 11 §4 signature items, all 5 §5 outline steps, and all 15 §9
validations map to real code and a real, passing test. Every guard called out in the audit brief is present
(`^@` skip, `flag==4`, `<11`-field error, field-order/last-wins tag scan, chomped `raw_line`,
drain-before-wait `finish()`, `Drop` kill+wait, and the >64K early-stop test). The §13 deviation (`align.rs`
alongside `aligner.rs`) is real (both files on disk, both declared in `lib.rs`) and documented. The module is
correctly **not wired into `run()`** (Phase 4's job), so Phase-1/2 tests remain green.
