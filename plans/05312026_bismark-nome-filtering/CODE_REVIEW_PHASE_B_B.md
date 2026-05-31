# Code Review — Phase B (Reviewer B)

**Target:** Rust port of Perl `NOMe_filtering` v0.25.1, Phase B (per-read NOMe filter + always-gzipped `.manOwar.txt.gz`).
**Reviewer:** B (independent, fresh context, recommend-only — no files modified).
**Date:** 2026-05-31.

**Files reviewed:**
- `rust/bismark-nome-filtering/src/nome.rs` (NEW — the full pipeline)
- `rust/bismark-nome-filtering/src/lib.rs` (`run()` restructure + `pub mod nome`)
- `rust/bismark-nome-filtering/src/filename.rs`, `src/substr.rs` (re-read for arithmetic)
- `rust/bismark-nome-filtering/tests/golden_phase_b.rs`, `tests/cli_phase_a.rs`, `tests/data/phase_b/**`
- Perl `NOMe_filtering` `per_read_filtering:48-230` + `cytosine_lookup:242-391` (in full)
- `bismark-io/src/genome.rs` (`get`/`len`/`load` API)

---

## Summary

**Verdict: the port is byte-identical to Perl v0.25.1 on all real-data-plausible inputs. No Critical or High issues. Approve.**

I re-derived the §8/§9 extraction arithmetic from the Perl independently and confirmed every `isize` offset, the suitability guard, the fwd/rev `tri`/`upstream` derivation, the `g = pos+offset-1` genomic key, and the NOMe filter logic match the Perl exactly. I went beyond the committed fixtures and ran **head-to-head differential tests** (Rust binary vs repo Perl, decompress-then-`cmp`) on scenarios the success-only goldens don't cover:

- **Byte-identical**: VS-pad (CpG as the read's last base), a *suitable* reverse read that genuinely counts G-strand calls (`meth_CG=1,unmeth_CG=1` — not just the all-zero edge), multi-chromosome emission ordering, same-position last-wins, non-consecutive same-ReadID (two reads), gz-input parity, CRLF input, unknown-chr (exit 0, header-only), empty-input (exit non-zero, header-only `.gz`), blank-line-between-reads.

- I **confirmed the implementer's claim** that the `classify → None` (`else { warn; next }`) branch is **unreachable** when `tri.len()==3`: the scanned byte (C or G) is always the first base of `tri` — forward `tri[0]=ext[pos+1]=the C`; reverse `tri` = revcomp of a 3-mer whose last base is the scanned `G`, so `tri[0]=complement(G)=C`. Verified with adversarial `N`-flanked Perl micro-experiments. The `eprintln!` is dead-but-harmless (STDERR not gated; mirrors Perl's identically-dead branch). Keep it — it documents the Perl structure.

The only findings are **Low/Medium**: a handful of *malformed-input* divergences (multi-char or empty `state`/`call`; trailing-space / leading-zero / negative coords) that arise from the `.first()`-byte and `u32::parse` design choices. **None can occur on real `--yacht` output** and all fall under the SPEC's A4 "accepted divergence" decision — but two of them are *not literally* enumerated in A4 and the `state`-divergence corresponds to a Perl **fatal die**, so they are worth a one-line code comment / A4 amendment.

Tests: `cargo test -p bismark-nome-filtering` → **49 unit + 6 cli + 7 golden + 1 doctest, all green**. `cargo build --workspace` clean; `cargo test -p bismark-io` → 191/10/6/3 green (P7 no-sibling-breakage confirmed). `cargo clippy -p bismark-nome-filtering -p bismark-io --all-targets -- -D warnings` clean.

---

## Verification of the brief's focus areas

### 1. `perl_substr` extraction arithmetic (`process_read` + `cytosine_lookup`) — CORRECT
Independently traced the `main` fixture (chr1 `TTACGTTTCGTTGCGTTTGCAGTTTGCATTACGTTTTTTTTT`, forward read start=4 end=32) through Perl `substr` and the byte scan: seq=`CGTTTCGTTGCGTTTGCAGTTTGCATTAC`, ext=`TACGTTTCGTTGCGTTTGCAGTTTGCATTACGT`. Walking C/G with `pos=i+1`, `tri`/`upstream` per §8, and `g=pos+offset-1` reproduces exactly the covered positions (g=4 ACG-accept, g=9 TCG-accept, g=14 GCG-**reject**, g=20 CHG-GpC, g=27 CHH-GpC, g=32 ACG-accept) → `meth_CG=1,unmeth_CG=2,meth_GC=1,unmeth_GC=1` = `main.golden`. The Rust offsets `(pos+1)`, `pos`, `(pos-1)` as `isize` and the `start-1`/`start-3`/`end-1`/`end-3` window extraction match the Perl line-for-line.
- **Reverse `end∈{1,2}` path**: traced — `end-3` negative → `perl_substr` reads from the chromosome end → degenerate ext → all `tri.len()<3` → all-zero line. No panic (the `perl_substr` `start>L`/`start==L`→`&[]` guards hold; unit-tested in `substr.rs`). Matches `edge.golden` against fresh Perl.
- **Forward `start≤3`**: guard `(start-2>1)` false → `process_read` returns `Ok(())` → no line. Matches Perl (`edge.golden` has no `fwd_low` line).

### 2. Suitability guard `(start-2>1) && (chr_len >= start-2+length+4)` — EXACT match to Perl `:132`
- `>=` boundary verified by unit test `guard_ge_boundary_suitable_and_one_less_not` AND by reasoning: `chr_len as i64 >= ...` (not `>`) is the faithful `length($chromosomes{...}) >= (...)`.
- `start < 2` underflow handled by `start as i64 - 2` (e.g. `0-2=-2 > 1` false). Faithful to Perl numeric `$last_start - 2` going negative.
- **Uses `start` (col-6) for both strands** — confirmed correct (P2). For a reverse read col-6 is the *larger* coord (P2 of the pair); the guard intentionally tests the larger coord even though extraction uses the smaller (`end`). I verified a reverse read where the guard correctly **fails** because the *larger* coord pushes the window past chr_len (my `revreal` probe), matching Perl. Unknown-chr → `chr_len=0` → guard fails → skip (verified end-to-end, exit 0, header-only).

### 3. Grouping / flush — CORRECT
- Consecutive-ReadID match via `lid.as_slice() == id`. Same-position **last-wins** via unconditional `read.insert` (NOT `or_insert`) — `same_position_within_read_last_wins` test + my `lastwins` differential both confirm `+Z` then `-z` → unmeth.
- `process_read` flushes ONLY; seeding (`read.clear()` + `insert` + `last=Some(...)`) is in the loop body (P17). EOF flush at the `match last` tail does not seed. Non-consecutive same-ReadID → two reads (`non_consecutive_same_id_is_two_reads` + my `noncons` differential → 3 lines, byte-identical to Perl).
- `EmptyInput` raised iff `last` is `None` at EOF.

### 4. D4 empty-input — CORRECT
`write_report` writes `HEADER` before opening the input / running the loop, and calls `enc.finish()` on **both** the Ok and Err paths before propagating `result`. Verified end-to-end: empty input → header-only `.gz` on disk (decompresses to exactly `HEADER`) + non-zero exit. A non-empty input whose only read is on an unknown chr exits **0** with header-only output (NOT `EmptyInput`) — confirmed against Perl (both exit 0, byte-identical). The two paths are correctly distinguished.

### 5. The match-guard NOMe filter — CORRECT
Reproduces Perl exactly: CG ⇒ `{z,Z}` + `upstream ∈ {ACG,TCG}`; CHG ⇒ `{x,X}` + `upstream` starts `GC`; CHH ⇒ `{h,H}` + `GC`; tally by col-2 `state` (`+`→meth, `-`→unmeth, else no-op). The CG explicit-`next` vs CHG/CHH fall-through is behaviourally folded into the match guards (A-I3) — net behaviour identical (no count, loop continues). `is_gpc = upstream.len()>=2 && upstream[0..2]==b"GC"` correctly guards short upstream; the `upstream == b"ACG"/b"TCG"` exact-equality is the right short-upstream test for CG (a 2-byte upstream can never `==` a 3-byte literal → reject, matching Perl `eq`). Context/call mismatch → `_ => {}` no count (`cpg_context_but_call_is_chh_letter_disregarded` test). The `_` state arm silently drops a non-`+/-` state — Perl would `die "This should never happen"` there, but that is unreachable on real data and STDERR/exit aren't gated.

### 6. A4 parse policy — FAITHFUL on real data (see Low/Medium for the OOD divergences)
`<8` fields and non-numeric `pos/start/end` → skip. `^Bismark` skip via `line.starts_with("Bismark")` faithful to Perl `/^Bismark/` (both anchored-prefix). `f[1].first()` / `f[4].first()` single-byte extraction is correct for the canonical single-char `state`/`call`. The `.unwrap_or(b'?')` sentinel for an empty field is benign: `b'?'` never matches `+/-` or any call letter → no count, which matches Perl `eq` on an empty string.

### 7. Output + idioms — CORRECT
`w.write_all(id)` / `write_all(chr)` keep id/chr byte-faithful (no `from_utf8_lossy`); integer columns via `writeln!`. `offset/end` always ascending (`process_read` passes `(start,end)` fwd, `(end,start)` rev). `#![forbid(unsafe_code)]` present; `nome.rs` has zero `unsafe`. Casts sound on 64-bit (`u32` < `isize::MAX`; `g=pos+offset-1` cannot underflow since `pos≥1`). Clippy clean (`#[allow(clippy::too_many_arguments)]` justified by the faithful 8-param port).

### 8. Test sufficiency — goldens are REAL; gaps are Low
`generate_goldens.sh` runs the repo Perl v0.25.1 and commits the decompressed output. I regenerated `main`/`edge`/`ncontext` from fresh Perl → identical to the committed `.golden`s. Gaps the success-only fixtures miss are listed under Low-2.

---

## Issues by area

### Parsing / malformed-input divergences (out-of-distribution; A4 territory)

I ran the Rust binary and repo Perl head-to-head on non-canonical inputs. **None can occur on real `--yacht` output** (col-2 is a single `+/-`, col-5 a single call letter, coords are canonical decimal), so none affect the Phase-C real-data gate. But they are silent divergences the goldens won't catch, and two are not literally covered by A4's wording ("`<8` fields / non-numeric coords"):

| Input | Perl | Rust | Note |
|-------|------|------|------|
| `state = "+x"` | `eq '+'` false → falls to the unreachable `die` → **header-only file** | `.first()='+'` → counts meth | Perl *dies*; Rust over-counts |
| `call = "Zz"` | `eq 'Z'` false → CG branch disregarded → no count | `.first()='Z'` → counts | Rust over-counts |
| `end = "32 "` (trailing space) | numifies to 32, suitable; prints End column as the **literal string** `32 ` | `"32 ".parse::<u32>()` fails → A4 skip → no line | both diverge in opposite directions |
| `start/pos = "04"` (leading zero) | prints Start column **literally** `04` | parses → reprints `4` | reveals Perl prints the *original string*, Rust round-trips through `u32` |
| `pos/start = "-4"` (negative) | numifies; off-genome → no line | `parse::<u32>` fails → skip → no line | both → no line (converge by accident) |
| `state = ""` (empty) | `eq '+'` false → `die` → header-only | `b'?'` → no count → emits zero line | diverge |

Cross-cutting observation (the root of the leading-zero/trailing-space cases): **Perl prints the Start/End columns as the raw yacht strings (`$offset`,`$end` are the original `$last_start`/`$last_end` strings), whereas Rust prints the re-formatted `u32`.** On canonical decimal coords these are identical; on any non-canonical numeric representation they diverge. This is benign for real data but is the single most "surprising" structural difference and deserves a one-line note in code or the SPEC.

### Exit code (intentional, documented divergence)
Perl's empty-input `die` exits **255**; Rust `main.rs` maps `BismarkNomeError` → `ExitCode::from(1)`. SPEC §2 gates only "non-zero exit" + the on-disk artifact (not the exact code/STDERR), so this is fine. The golden test hard-asserts `.code(1)` — that pins the *Rust* contract, not Perl-parity, which is consistent. Worth a one-line acknowledgement that 1≠255 is deliberate.

---

## Recommendations (prioritized — RECOMMEND ONLY)

### Critical
None.

### High
None.

### Medium
- **M1 — Amend A4 (or add a code comment) to cover the `.first()` truncation.** The SPEC's A4 decision enumerates only "`<8` fields / non-numeric coords → skip." The implementer additionally chose `state`/`call` = *first byte only*, which silently diverges from Perl on multi-char fields (Perl uses whole-string `eq`, and on a multi-char `state` Perl *dies*). This cannot occur on real data, but the decision is undocumented. Recommend: add a one-line comment near `let state_b = f[1].as_bytes().first()...` ("real yacht state/call are single bytes; multi-char is OOD and accepted divergence — Perl would die/disregard") and/or extend A4 in the SPEC. Pin with a unit test asserting the *documented* Rust behaviour (no Perl-parity claim).

### Low
- **L1 — Document the "raw-string vs re-formatted u32" Start/End divergence.** Add a comment where the columns are written (or in §6) noting Perl prints the original coordinate strings while Rust prints the parsed `u32`; identical on canonical decimal, divergent on leading-zeros/embedded-space. Purely defensive documentation.
- **L2 — Optional fixture additions for silent-divergence coverage.** The committed goldens are all *success* cases plus the edge/empty paths. Consider adding (against fresh Perl goldens) at least one of: (a) a **suitable reverse read that genuinely counts G-strand calls** (the current `edge` reverse case is only the all-zero degenerate path — a real reverse-counting read is the highest-value missing scenario; I verified it byte-identical manually), and (b) a **multi-chromosome, multi-read** fixture to lock emission ordering. Both currently pass against Perl but are unguarded by a committed golden.
- **L3 — Note the exit-code 1≠255 divergence** as deliberate (SPEC §2) in a comment near `main.rs`'s `ExitCode::from(1)` so a future reader doesn't "fix" it toward Perl's 255.
- **L4 — (informational) The `eprintln!` warn-skip in `cytosine_lookup` is unreachable** when `tri.len()==3` (proven above). It is correct to keep it (mirrors Perl, STDERR not gated). No action; flagging only so a future "dead code" cleanup doesn't remove the documentation-of-Perl-structure.

---

## Confirmation of the implementer's specific claim
**"The `else→warn-skip` context branch is unreachable (tri[0] always C)" — CONFIRMED.** Forward `tri` begins at `ext[pos+1]`, which is the scanned `C`. Reverse `tri` is the revcomp of a 3-mer whose final base is the scanned `G`; revcomp moves that base to position 0 and complements `G→C`. Adversarial `N`-flanked windows (`NNG`,`NAG` → revcomp `CNN`,`CTN`) still start with `C`. The only way to reach `None` is `tri.len()!=3`, which the prior `if tri.len()<3 {continue}` already excludes. Dead-but-faithful.

---

### Report path
`/Users/fkrueger/Github/Bismark-nome/plans/05312026_bismark-nome-filtering/CODE_REVIEW_PHASE_B_B.md`
