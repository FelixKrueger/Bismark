# CODE_REVIEW_A — Phase 3: single-instance align + SAM parse (lockstep stream primitive)

- **Reviewer:** A (independent, fresh context; audit-only — no code modified)
- **Code reviewed:** `rust/bismark-aligner/src/align.rs` (new, the whole phase) + the `pub mod align;` line in `rust/bismark-aligner/src/lib.rs:19`.
- **Grounding:** Perl `bismark` v0.25.1 `single_end_align_fragments_to_bisulfite_genome_fastQ_bowtie2` (6849–6912), `check_results_single_end` field/tag extraction (2737, 2773–2796, 2838) + `flag==4` (2739); PLAN.md rev 1 (§3, §3.5, §9, §13) + PLAN_REVIEW_A/B.
- **Build:** `cargo test -p bismark-aligner` → **49 unit + 15 cli + 0 doc PASS** (13 of the unit tests are in `align.rs`). `cargo clippy -p bismark-aligner --all-targets -- -D warnings` → **clean**.

## Verdict

**APPROVE.** The primitive is a faithful, well-scoped mirror of the Perl spawn → header-skip → store-first → peek/advance model. Every field index, the tag scan, `flag==4`, raw arg order, the chomped `raw_line`, and the child-lifecycle contract check out against the source and against the rev-1 plan. **No Critical or High issues.** All findings below are Medium/Low — the byte-identity-relevant ones (Medium) are either documented deviations that cannot fire on real data or test-coverage gaps for stated contracts; none changes output on the v1 Bowtie 2 spine. The load-bearing child-process risk (drain-before-wait / kill-then-wait) is implemented correctly and is backed by a genuinely meaningful test.

---

## 1. Byte-identity faithfulness

All verified **correct** against Perl:

- **Spawn arg order** (`align.rs:154–164`): `options.split_whitespace()` → `orient.flag()` → `-x <index>` → `-U <input>` exactly mirrors Perl 6882 (`$path_to_bowtie $bt2_options -x $fh->{bisulfiteIndex} -U $temp_dir$fh->{inputfile}`), with `--norc`/`--nofw` from 6873–6878. `split_whitespace()` (not `split(' ')`) is the right choice — collapses runs of spaces so the option string `-q --score-min L,0,-0.2 --ignore-quals` tokenizes to the same argv the shell would hand Bowtie 2 (PLAN_REVIEW_A §5.6 / B item 10). ✓
- **Header skip + store-first** (`align.rs:178–190`): discard `line.starts_with('@')`, first non-`@` line becomes `current`, EOF (`n==0`) → `None`. Exact mirror of Perl 6884–6910. ✓
- **Field indices** (`align.rs:110–123`): `0` qname, `1` flag, `2` rname, `3` pos, `4` mapq, `5` cigar, `9` seq, `10` qual — identical to Perl 2737 `(split /\t/)[0,1,2,3,4,5,9,10]`. ✓
- **RNAME kept raw** (`align.rs:113`, doc 44–46): suffix `_CT_converted`/`_GA_converted` retained; Perl de-converts only at 2763 inside the consumer (= Phase 4). ✓
- **Tag scan** (`align.rs:96–108`): per-field, mutually-exclusive `if / else if` over `f[11..]` in field order. The Perl chain (2777–2795) is `AS / elsif ZS / elsif MD / else{ if XS / elsif ZS }`; the Rust order is `AS / (XS|ZS) / MD`. **These are behaviorally identical** because the four prefixes are disjoint on a single tab-token — no field can match two, so the if/elsif precedence never resolves a tie, and the Perl `else`-branch dead `ZS` sub-branch (2791, already caught at 2780) is correctly dropped. `second_best` last-XS/ZS-wins matches Perl re-assigning `$second_best` as fields advance. ✓ (See Medium-1 for the one residual nuance.)
- **`flag==4` SE-unmapped** (`align.rs:127–129`): `self.flag == 4`, matching Perl 2739. The "store next, move on" advance logic (Perl 2740–2758) is correctly deferred to the Phase 4 consumer — Phase 3 exposes only the predicate. SE-only; PE is Phase 7, doc'd. ✓
- **`raw_line` = chomped line** (`align.rs:75, 122`): `trim_end_matches(['\n','\r'])` then stored. Perl stores `last_line` *after* `chomp` (6898) and `--ambig_bam` re-emits it (2807–08). This was the rev-1 fold of PLAN_REVIEW_B §2.1 and is implemented. ✓ (One sub-`chomp` nuance → Low-1.)
- **Numeric core fields** parse to `u16`/`u32`/`u8` and **error** on a malformed FLAG/POS/MAPQ (`align.rs:83–91`) — the deliberate fail-loud choice both plan reviewers asked for (PLAN_REVIEW_A §1 smaller-notes, B §5.3). ✓

No faithfulness **divergence** that affects output on the v1 Bowtie 2 path. The two byte-relevant nuances below are documented and unreachable on real data.

## 2. Child-process contract (the load-bearing risk) — correct

- **`finish()` drains before wait** (`align.rs:222–224`): `std::io::copy(&mut self.reader, &mut sink())` empties the OS pipe **before** `self.child.wait()`. This is exactly the fix for the full-stdout-pipe deadlock (Bowtie 2 blocked on `write()` into a full pipe while the parent blocks in `wait()`) flagged by PLAN_REVIEW_A §4(C) / B §4.3. ✓
- **`Drop` reaps** (`align.rs:236–243`): on the not-`finished` path it does `kill()` **then** `wait()` — kill alone leaves a zombie until the parent exits (PLAN_REVIEW_A §4(D) / B §4.2). ✓
- **No double-wait** (`align.rs:225, 239`): `finish(mut self)` sets `finished = true` after a successful `wait()`, so the subsequent `Drop` is a no-op. On the *error* path of `finish()` (`?` on copy or wait) `finished` stays `false` and `Drop` re-issues `kill()`/`wait()` — both are `let _`-swallowed and idempotent against an already-reaped/exited child (returns `ECHILD`/already-exited, harmless). No panic, no zombie, no hang. ✓
- **stderr inherited** (`align.rs:164`): only stdout is piped, so there is no second pipe to deadlock on — the deliberate reason the contract is deadlock-safe (PLAN_REVIEW_B §4.1), and it matches Perl piping only stdout. ✓
- **Early-stop test is meaningful** (`align.rs:395–407`): the fake emits 5000 records (~250 KB, well over macOS's 16–64 KiB pipe buffer); the test consumes only `current()` (read once during `spawn`) and calls `finish()`. Without the `std::io::copy` drain the child would still be blocked writing into a full pipe and `wait()` would hang forever — so this test genuinely exercises the drain path, not a tautology. ✓

No remaining hang or zombie path found.

## 3. Rust correctness / quality

- **`read_line` UTF-8 assumption** (`align.rs:182, 209`): `BufRead::read_line(&mut String)` returns `Err(InvalidData)` on non-UTF-8 bytes. SAM from Bowtie 2 is ASCII/printable (QNAME/SEQ/QUAL/tags), so this is safe in practice and an invalid-UTF-8 stream surfaces as a clean I/O error rather than UB. Acceptable; see Low-2 for a note.
- **peek/advance API** is the correct seam for Phase 4's N-way lockstep (peek-then-conditionally-advance; a bare `Iterator` would force a `Peekable` holding slot). `current() -> Option<&SamRecord>` (infallible peek) + `advance() -> Result<()>` (errors surface on read/parse) is clean and mirrors Perl's `last_line`/`getline` 1:1. ✓
- **Buffer reuse** (`align.rs:208–209`): `advance()` does `self.line_buf.clear()` before `read_line` (which appends) — correct; the owned `SamRecord` `String` fields are necessarily cloned out of the reused buffer, which is fine. Minor: `spawn()` uses a separate local `line` for the header-skip/first-record read rather than `self.line_buf` (a one-time allocation) — not a defect.
- **Error handling**: `spawn` failure → `Validation` with the binary path (matches the error module's Perl-mirroring style); non-zero exit → `Validation` with the status — the **intentional fail-closed** deviation from Perl's fail-open pipe close, documented at `align.rs:219–221`. ✓
- **Idiom/clippy**: clean under `-D warnings`. `Orientation` is a tidy `Copy` enum; doc comments cite Perl line numbers throughout.

## 4. Test quality

The 13 align tests assert **real behavior**, not tautologies, and the fake-`bowtie2` harness (`align.rs:320–344`) is hermetic (no real Bowtie 2/index). Coverage vs PLAN §9:

| §9 row | Covered by | Real assertion? |
|---|---|---|
| 1 core fields | `parse_core_fields` | ✓ asserts SEQ=idx9, QUAL=idx10 explicitly |
| 2 tag scan (+ZS) | `parse_tags`, `parse_negative_as_and_hisat2_zs` | ✓ incl. negative AS/ZS |
| 3 unmapped | `unmapped_record` | ✓ `is_unmapped()` + AS/MD None |
| 4 missing AS/MD | `unmapped_record` (AS/MD None) | partial (see Medium-2) |
| 5 header skip | `stream_skips_header_then_walks_records_to_eof` | ✓ |
| 6 advance→EOF | same | ✓ |
| 7 finish exit status | `finish_errors_on_nonzero_exit` | ✓ Err on exit 1 |
| 8 spawn failure | `spawn_fails_on_bad_path` | ✓ |
| 9 empty/all-header | `all_header_stream_has_no_records` | ✓ |
| 11 both XS+ZS last-wins | `both_xs_and_zs_last_wins` | ✓ |
| 12 unique no second-best | `unique_alignment_has_no_second_best` | ✓ |
| 13 short line | `short_line_errors` | ✓ |
| 14 CRLF + raw_line | `crlf_trimmed_and_raw_line_clean` | ✓ |
| 15 early-stop | `early_stop_does_not_deadlock_or_zombie` | ✓ meaningful (>64K) |

**Gaps** (all Medium/Low — see issues): §9 row **10** (realistic line with RNEXT/PNEXT/TLEN 6–8 populated to prove SEQ/QUAL land on 9/10) is NOT a standalone test — `MAPPED` does have `*\t0\t0` at fields 6–8 so it's implicitly covered, but there is no test with *non-trivial* fields 6–8 (`=`, a real PNEXT, a real TLEN). A mapped line with `AS:i:` but **no** `XS:i:` plus other ignored tags (`XM:i:`/`NM:i:`) — PLAN_REVIEW_B §5.2 — is only partly covered. No argv-order assertion (PLAN_REVIEW_A §5.7, Optional). No malformed-core-numeric unit (`pos="x"` → Err; PLAN_REVIEW_B §5.3).

---

## Issues by area & recommendations (prioritized)

### Critical
*(none)*

### High
*(none)*

### Medium

**M1 — `MD:Z:` value capture is anchored prefix vs Perl's unanchored regex; equivalent on real data, worth a one-test guard.** *(faithfulness, byte-relevant only off-path)*
`align.rs:105` uses `strip_prefix("MD:Z:")` (anchored at field start) where Perl uses `/MD:Z:(.*)/` (unanchored). For well-formed SAM the tag key is always at field start, so the captured value is identical; `(.*)` and `strip_prefix` both keep the entire field remainder. The current MD test asserts only `MD:Z:10` (digits). Recommend an MD value with **embedded letters** (e.g. `MD:Z:7A42`) — PLAN_REVIEW_B §2.2/item 4 — to prove the capture isn't accidentally numeric-only and that a real MD string round-trips. Cheap insurance; not a behavior change. *(Perl 2783; `align.rs:105`.)*

**M2 — No test for a *mapped* record missing `AS`/`MD` (PLAN §9 row 4 / edge case).** *(test gap)*
Row 4 ("missing AS/MD on a *mapped* record → `None`, no die — Phase 4 enforces") is only exercised via the `flag==4` `unmapped_record` test, where AS/MD are legitimately absent. The faithfulness contract — that a `flag==0` line lacking `AS:i:`/`MD:Z:` parses to `None` **without** erroring (Phase 3 parse-not-die; Perl dies at 2838 in the *consumer*) — is not directly asserted. Add a unit: a mapped (`flag 0`) line with ≥11 fields but no AS/MD → `parse` Ok, `alignment_score==None`, `md_tag==None`. *(Perl 2838; PLAN §9 row 4 / §3 edge case 3.)*

**M3 — No realistic-layout test with non-trivial RNEXT/PNEXT/TLEN (PLAN §9 row 10) and no multi-tag/ignored-tag scan test.** *(test gap, silent off-by-one risk)*
The index extraction (`[9]`=SEQ, `[10]`=QUAL, tags from `[11..]`) is the most likely place an off-by-one would *still parse* and silently produce a plausible-but-wrong SEQ/QUAL. `MAPPED` uses `*\t0\t0` for fields 6–8, which is realistic but uniform. Add a test with **non-trivial** fields 6–8 (e.g. `=`, a PNEXT integer, a TLEN integer) and several **unrelated tags** interleaved around the wanted ones (`XN:i:`, `XM:i:`, `NM:i:`, `YT:Z:`) with `MD:Z:` appearing **last**, to prove the `11..` scan walks all fields and that a `starts_with` bug can't match `XS:i:` against `XM:i:`. (PLAN_REVIEW_A §5.1–5.2 / B §5.1–5.2.) This is the single most valuable test to add.

### Low

**L1 — `trim_end_matches(['\n','\r'])` strips `\r`, but Perl `chomp` does not.** *(documented deviation, unreachable on real data)*
`align.rs:75` strips both `\n` and `\r`; Perl `chomp` (6898) removes only the trailing `$/` (= `\n`), so on a `\r\n` line Perl's `last_line` would **retain** the `\r`. If `--ambig_bam` ever re-emitted a CRLF record, Perl would carry the stray `\r` and Rust would not — a one-byte divergence. This **cannot fire on real data** (Bowtie 2 writes Unix line endings; the converted temp FastQ is Rust-generated), and the Rust behavior is arguably more correct, but it is technically not byte-identical to Perl `chomp` under hypothetical CRLF. Already documented in the plan as a deliberate trim choice; flagging so it's a conscious record, not an accident. No change recommended for the gate. *(Perl 6898; `align.rs:75`, test `crlf_trimmed_and_raw_line_clean` 311–316.)*

**L2 — `read_line` assumes UTF-8 on Bowtie 2 stdout.** *(robustness note)*
`align.rs:182, 209` use `BufRead::read_line(&mut String)`, which errors on non-UTF-8 bytes. SAM is ASCII so this is safe; the failure mode (clean `io::Error`) is acceptable. No action — noting for completeness since the focus list called it out.

**L3 — Lenient `i64` parse of `AS`/`XS`/`ZS` diverges from Perl on non-numeric tag values.** *(documented, unreachable)*
`align.rs:99, 102` do `v.parse::<i64>().ok()` → `None` on a non-numeric score; Perl captures the raw string (never undef from a present tag) and would *not* die at 2838, later coercing to 0 in numeric comparison. So on a malformed `AS:i:foo`, Rust→`None`→Phase 4 dies; Perl→keeps the bad string→coerces. This only occurs on malformed input Bowtie 2 never emits, and the plan documents the lenient policy (`align.rs:71–73`). No change recommended. *(Perl 2777/2838; `align.rs:96–108`.)*

**L4 — No argv-order assertion test.** *(optional insurance)*
The spawn arg order (`<options> --norc -x <idx> -U <reads>`) is a stated faithfulness contract but only enforced by reading the code. PLAN_REVIEW_A §5.7 suggested a fake that writes `"$@"` to a file the test reads. Bowtie 2 is order-independent so this is purely regression insurance — Optional, mentioned only because the focus list asked about arg order.

---

## Bottom line (top findings)

1. **Faithful and correct.** Every Perl contract — field indices `[0,1,2,3,4,5,9,10]`, the AS/XS/ZS/MD tag scan (disjoint-prefix collapse is behaviorally identical to Perl's if/elsif), `flag==4` SE-unmapped, raw RNAME, chomped `raw_line`, raw `<options> --norc -x -U` arg order — checks out. No Critical/High.
2. **Child-process contract is right.** `finish()` drains stdout *before* `wait()` (no full-pipe deadlock), `Drop` does `kill()` **then** `wait()` (no zombie), the `finished` flag disarms the double-wait, and the 5000-record (>64K) early-stop test genuinely exercises the drain — not a tautology.
3. **Tests assert real behavior**; 49 unit + 15 cli pass, clippy `-D warnings` clean.
4. **Recommended additions (Medium):** a mapped-record-missing-AS/MD unit (M2), and a realistic line with non-trivial RNEXT/PNEXT/TLEN + interleaved ignored tags with `MD:Z:` last (M3 — guards the highest-risk silent off-by-one). M1 (MD value with letters) is cheap.
5. **Two documented deviations** (L1 `\r` strip vs `chomp`; L3 lenient i64) are unreachable on real Bowtie 2 data and are conscious choices — no gate impact.

**Report file:** `/Users/fkrueger/Github/Bismark-aligner/plans/05312026_bismark-aligner/phase3-single-instance-align-parse/CODE_REVIEW_A.md`
