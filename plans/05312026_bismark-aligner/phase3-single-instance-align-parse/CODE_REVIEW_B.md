# CODE REVIEW B — Phase 3: single-instance align + SAM parse (`align.rs`)

**Reviewer:** B (independent, fresh context). Audit-only — no code modified (two reviewers run in parallel; must not race).
**Scope:** `rust/bismark-aligner/src/align.rs` (new), `src/lib.rs` (`pub mod align;`).
**Gate:** byte-identical decompressed SAM vs Perl Bismark v0.25.1 → faithfulness to Perl `bismark` is paramount.

## Verdict

**APPROVE.** The phase is faithful to the Perl source on every load-bearing point I checked (spawn arg order, header-skip + store-first, field indices `0,1,2,3,4,5,9,10`, tag scan with last-`XS`/`ZS`-wins, `flag==4`, raw-`RNAME`, chomped `raw_line`). The child-process contract — the one real risk — is implemented correctly: `finish()` drains stdout *before* `wait()`, `Drop` does `kill()`+`wait()`, and the early-stop test genuinely exercises a >64K-pipe-buffer backpressure path. No Critical or High issues. A few Medium/Low items below are quality/test-coverage notes, none block the phase (and most are explicitly Phase-4 deferrals already documented in the plan).

**Build status (verified, sandbox disabled — worktree outside writable paths):**
- `cargo test -p bismark-aligner` → **49 unit + 15 CLI integration pass** (13 of the units are in `align.rs`); 0 failed.
- `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → **clean**.
- `cargo fmt -p bismark-aligner -- --check` → **clean** (CI gates fmt separately).

## Faithfulness audit (Perl ↔ Rust)

| Concern | Perl | Rust | Verdict |
|---|---|---|---|
| Spawn arg order `<opts> --norc -x <idx> -U <reads>` | 6882 | `spawn` 154–164 (`options.split_whitespace()` → `orient.flag()` → `-x idx` → `-U input`) | **Match.** Order replicated even though Bowtie 2 is order-independent. |
| stderr inherited (summary → terminal, not gated) | `warn`/pipe-to-terminal | `.stderr(Stdio::inherit())` 164 | **Match.** Only stdout piped → no stderr-pipe deadlock. |
| `^@` header skip + store first non-`@` | 6884–6894 | `spawn` loop 180–190 (`line.starts_with('@')` → continue; first non-`@` → parse) | **Match.** |
| EOF before any record → `None` | 6906–6910 (undef) | `n==0` → `break None` 183 | **Match.** |
| Field indices `0,1,2,3,4,5,9,10` | 2737 | `parse` 110–123 (`f[0],f[1],f[2],f[3],f[4],f[5],f[9],f[10]`) | **Match.** SEQ/QUAL from 9/10, not earlier fields (proven by `parse_core_fields` w/ `*\t0\t0` at 6/7/8). |
| RNAME kept raw (suffix retained) | de-conversion at 2763 (Phase 4/5) | `rname: f[2]` raw 113 | **Match.** Correctly NOT de-converted here. |
| Tag scan field-order, AS/XS(ZS)/MD, last-wins | 2775–2795 | `parse` 96–108 | **Match — verified carefully** (see note below). |
| `flag == 4` SE-unmapped | 2739 | `is_unmapped` 127–129 | **Match.** SE-only; PE deferred to Phase 7 (documented). |
| `raw_line` = chomped line | `chomp` 6898; `--ambig_bam` re-emits 2807–08 | `trimmed.to_string()` after `trim_end_matches(['\n','\r'])` 75/122 | **Match.** Prevents stray `\n` on Phase-6 re-emit. |

### Tag-scan precedence — verified equivalent (not a bug)
Perl's `if(AS) / elsif(ZS) / elsif(MD) / else{ if bowtie2 { if(XS) elsif(ZS) } }` (2777–2795) looks structurally different from Rust's `if AS / else if (XS or ZS) / else if MD` (98–107). Because the four prefixes are **disjoint** (a SAM tag field is exactly one of `AS:i:`/`XS:i:`/`ZS:i:`/`MD:Z:`), at most one arm fires per field in *both* implementations, and `ZS`→`second_best` / `XS`→`second_best` collapse to the same assignment. The "last-`XS`/`ZS`-in-field-order wins" semantics are preserved (both overwrite). **Faithful.** (`both_xs_and_zs_last_wins` asserts this.)

## Issues by area

### Child-process contract (the load-bearing risk) — PASS
- `finish()` (222–233) does `std::io::copy(&mut self.reader, &mut sink())` **then** `child.wait()` — correct drain-before-wait ordering. With stderr inherited (not piped), stdout is the only pipe and it is always drained → no full-pipe deadlock on Phase-4 early-stop.
- `Drop` (236–244) does `kill()` **then** `wait()` only when `!finished` → no zombie, no double-reap on the happy path (`finish()` sets `finished=true` before returning Ok).
- `early_stop_does_not_deadlock_or_zombie` (397–407) emits 5000 records (well past the ~64K stdout pipe buffer), reads only `r0`, then `finish()`s. This is a **meaningful** backpressure test: a non-draining `wait()` would hang the child on `write()`. Verified it passes in <1s (drain works). Good.

### Rust correctness / quality
- **[Low] `read_line` imposes a UTF-8 requirement Perl does not.** `BufReader::read_line` (182, 209) returns `io::Error(InvalidData)` on non-UTF-8 stdout bytes; Perl `getline()` on the pipe is byte-mode and never decodes. Bowtie 2 SAM (QNAME/SEQ/QUAL/tags) is ASCII in practice, and the gate is on Bowtie 2's own output, so this is effectively unreachable — but it is a stricter I/O contract than Perl. If you ever want bit-for-bit robustness against exotic QNAMEs, `read_until(b'\n')` + `from_utf8_lossy`/byte-split would match Perl's tolerance. Not required for this phase.
- **[Low] Strict numeric parse vs Perl's lenient coercion.** `AS/XS/ZS` use `v.parse::<i64>().ok()` (99/104) and FLAG/POS/MAPQ use strict `parse` (83–91). Perl coerces strings numerically (lenient: `"4xyz"`→`4`). The plan §3.3 explicitly chose lenient-tag→`None` + Phase-4 presence enforcement, and Bowtie 2 always emits clean integers, so this is a **documented, acceptable** deviation. No action.
- **[Low] Double-`wait()` on the rare `finish()` error path.** `finish()` sets `finished=true` *after* `self.child.wait()?` (224–225). If `wait()` itself errors (not the non-zero-exit case, which sets `finished` first), `finished` stays `false` and `Drop` then `kill()`+`wait()`s an already-handled child. On the non-zero-*exit* path `finished` is already true, so no double-wait there. The genuine-`wait()`-error path is rare and the extra `kill`/`wait` are `let _`-ignored and harmless (ECHILD), but moving `self.finished = true;` to immediately after the `wait()?` (before the `status.success()` check) would make `Drop` a strict no-op once reaping was attempted. Cosmetic.
- **[OK] API / buffer reuse.** peek (`current`) / `advance` separation is clean; `line_buf` is reused across `advance()` (208–214) and cleared each call; `spawn` uses a local `line` for header skip (correct — the first record is stored as owned `current`, no aliasing). Idiomatic.
- **[OK] Error mapping.** `spawn` failure → `Validation` with binary path; missing stdout handle → `Validation`; `read_line` io errors flow through `?` as `AlignerError::Io`. Non-zero exit → `Validation` (intentional fail-closed; documented in §3.5).

### Test quality
The 13 align tests assert real behavior, not tautologies: `parse_core_fields` distinguishes SEQ@9/QUAL@10 from the `*/0/0` filler at 6/7/8; `crlf_trimmed_and_raw_line_clean` asserts no trailing `\r` on QUAL **and** `raw_line == MAPPED`; the stream tests use a real `#!/bin/sh` fake-bowtie2 over an actual subprocess+pipe (not a mock), so header-skip, advance-to-EOF, exit-status, and backpressure are exercised end-to-end. `unmapped_record` is at the exact 11-field boundary (verified field count = 11) — good boundary coverage.

**Gaps vs §9:**
- **[Medium] §9.4 not covered as written.** §9.4 = "missing AS/MD on a **mapped** record → parses to `None` (no die)". The closest test is `unmapped_record` (flag==4), which is §9.3, not §9.4. There is no test where `flag != 4` (a mapped line) lacks `AS:i:` or `MD:Z:` and the parser returns `Some(record)` with `alignment_score: None` / `md_tag: None`. This is the one validation row from the plan that isn't directly asserted. The code path is exercisable (the tag loop simply leaves them `None`), but the plan listed it explicitly. Recommend adding one unit: a `flag==0` line with only `XS:i:` (no AS, no MD) → `parse` Ok, `alignment_score==None`, `md_tag==None`. Low effort.
- **[Low] No test for malformed FLAG/POS/MAPQ.** `parse` (83–91) returns errors for unparseable FLAG/POS/MAPQ, but only the `<11`-fields error path (`short_line_errors`) is tested. A one-line negative test (e.g. `flag="x"` → `is_err()`) would lock in the §3.3 "unparseable FLAG/POS/MAPQ → parse error" contract. Optional.
- **[Low] `is_unmapped()==flag==4` exactness.** No test asserts a non-4 unmapped-ish flag (e.g. flag with bit 0x4 set among others, like flag==20) is treated as *mapped* under the SE rule. This is intentional per Perl 2739 (`==4`, not `& 4`), and `parse_core_fields` covers flag==0 → not unmapped, but an explicit `flag==20 → !is_unmapped()` test would document the SE quirk for the Phase-7 PE author. Optional.

## Recommendations (prioritized)

- **Critical:** none.
- **High:** none.
- **Medium:**
  1. Add a unit for **§9.4** — a *mapped* record (`flag==0`) missing `AS`/`MD` parses to `Some` with those fields `None` (no error). It's the only §9 row not directly asserted.
- **Low:**
  2. Add a malformed-FLAG (or POS/MAPQ) negative test to lock in the §3.3 parse-error contract.
  3. Add a `flag==20 → !is_unmapped()` test to document the SE `==4` (not `&4`) rule for the Phase-7 PE port.
  4. Move `self.finished = true;` in `finish()` to right after `self.child.wait()?` so `Drop` is a strict no-op once reaping was attempted (cosmetic; current behavior is harmless).
  5. (Note only) `read_line` requires UTF-8 stdout where Perl reads bytes — practically unreachable for Bowtie 2 SAM; revisit only if a future input demonstrates non-UTF-8 QNAMEs.

## Notes for downstream phases (not Phase-3 defects)
- `is_unmapped()` is SE-only (`flag==4`); Phase 7 PE needs the paired-flag logic. Already flagged in code (126) and plan §11.
- The `flag==4` "store next, move on, die-if-same-id" advance (Perl 2740–2758) lives in the Phase-4 consumer, not here — correct per §3.4.
- `raw_line` is retained specifically for `--ambig_bam` re-emit (Perl 2807–08); Phase 6 must apply the `_(CT|GA)_converted` strip (Perl 2808) on re-emit since `align.rs` deliberately keeps RNAME raw.

**Report path:** `/Users/fkrueger/Github/Bismark-aligner/plans/05312026_bismark-aligner/phase3-single-instance-align-parse/CODE_REVIEW_B.md`
