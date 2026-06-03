# Code Review — Phase B (`bismark-nome-filtering`) — Reviewer A

**Scope:** Phase B core of the Rust port of Perl `NOMe_filtering` v0.25.1, held to byte-identity.
**Files reviewed:** `src/nome.rs`, `src/lib.rs`, `src/substr.rs`, `src/filename.rs`, `src/cli.rs`, `src/error.rs`, `src/main.rs`, `bismark-io/src/genome.rs`, `tests/golden_phase_b.rs`, `tests/data/phase_b/**`.
**Method:** Read the Perl (`per_read_filtering:48-230`, `cytosine_lookup:242-391`, `process_commandline`, `read_genome_into_memory`) in full; re-derived the §8/§9 arithmetic; ran the full test suite (`cargo test -p bismark-nome-filtering`: 6 cli_phase_a + 7 golden_phase_b + 14 nome unit + 1 doctest, all green), clippy (`-D warnings`, clean), workspace build (clean), and `bismark-io` genome tests (13 green). Regenerated all four goldens from live Perl v0.25.1 (`/usr/bin/perl` 5.34.1) — **byte-identical** to the committed ones. Hand-traced `main` and `ncontext` against the Perl algorithm (both match the golden exactly). Independently validated the reverse-G strand CpG path against live Perl (not covered by any golden).

**Recommend-only.** No source modified.

---

## Summary

**The port is correct and the byte-identity crux is faithfully reproduced.** Every claim the orchestrator asked me to re-derive checks out:

- `cytosine_lookup` `pos=i+1` / fwd-C `tri=ext[pos+1..]`,`up=ext[pos..]` / rev-G `tri=revcomp(ext[pos-1..])`,`up=revcomp(ext[pos..])` / `len<3` skip / `g=pos+offset-1` / the three context match-guards: **all verified against the Perl and against live-Perl traces.** The match-guard refactor (vs Perl's nested-if + CG `next`) is behaviorally identical — no arm ever mis-routes, no "context-matches-but-call-mismatches" case ever counts.
- `process_read`: length, the `start`-for-both-strands `i64` guard (verified for start∈{0,1,2,3,4} against Perl), fwd/rev `perl_substr` extraction, the reverse `end∈{1,2}` degenerate all-zero path, unknown-chr → empty slice → skip: **all correct.**
- `per_read_filtering`: `^Bismark` skip, consecutive grouping, first-line start/end/chr, same-position last-wins (unconditional `insert`), flush-on-change + EOF flush, flush-only `process_read` with seeding in the loop body, `EmptyInput` only when no read seen: **all correct.**
- `write_report`/`run`: header-before-loop, `enc.finish()` on the `EmptyInput` path (header-only `.gz` lands), `GzEncoder`+`Compression::default()`, gz-aware `MultiGzDecoder` input: **all correct.** Empty-input artifact decompresses to exactly the 57-byte header (matches live Perl).
- Output format, header bytes, ascending offset/end, byte-faithful `id`/`chr` writes: **all correct.**
- The SPEC's claim that the `else→warn-skip` classification branch is **unreachable** from the live pipeline is **CONFIRMED** (proof below).

**The findings below are all on adversarial/malformed input that cannot occur in real `--yacht` output** (no blank lines, no malformed records, ASCII-only IDs). They do **not** threaten the Phase C real-data byte-identity gate. They are nonetheless genuine, observable Perl-vs-Rust divergences within the "accepted divergence" scope the SPEC/IMPL carved out (§5, IMPL A4) — and the specific *mechanisms* are undocumented, so I surface them for an explicit decision.

---

## Issues by area

### Logic / byte-identity

#### High — Malformed/blank interspersed line breaks Perl's read grouping (skip-vs-flush divergence)
The IMPL A4 decision skips a line with `<8` fields or non-numeric coords (`nome.rs:268, 274`). But Perl does **not** skip a short line — Perl `split /\t/` on a blank/short line still sets `$id` (to `undef` or to the line's first token), and that value participates in the `$id eq $last_read` grouping test (`NOMe_filtering:105`). When the skipped line's implied ID differs from the surrounding read's ID, **Perl treats it as a read boundary and flushes**, whereas Rust silently merges across it.

Reproduced against live Perl (genome = the committed `phase_b/genome`):

- **Blank line interspersed** in a same-ID run:
  ```
  r1  +  chr1  4  Z  4  32  +
  <blank line>
  r1  -  chr1  9  z  4  32  +
  ```
  - **Perl:** two lines — `r1 chr1 4 32 1 0 0 0` and `r1 chr1 4 32 0 1 0 0` (the blank's `undef` ID forces a flush of the first r1; the trailing r1 flushes at EOF).
  - **Rust:** one line — `r1 chr1 4 32 1 1 0 0` (blank skipped, both r1 lines merged).
- **`<8`-field line with a *different* first token** (`OTHER\tjunk`) interspersed: same divergence — Perl 2 lines, Rust 1 line.
- **Benign cases (no divergence):** an interspersed `^Bismark` line (both `next` *before* the grouping logic — Perl `:91`), and an interspersed 8-field line with a *non-numeric coord but the SAME id* (Perl keeps the same read because col-1 still matches; both give `1 1 0 0`).

So the precise trigger is: *a line Rust's A4 path drops, whose Perl-implied ID differs from the current read's ID.* This sits inside the SPEC's "accepted divergence … cannot occur on real `--yacht` output" envelope, **but** the IMPL A4 note only reasoned about malformed lines in isolation and explicitly says they're benign — it never noticed the grouping side-effect. Recommend: (a) document the mechanism in `nome.rs` next to the A4 skips and in the SPEC, and (b) confirm with Felix whether the accepted-skip behavior is intended here, or whether a Perl-faithful "treat a malformed line as a read boundary" path is wanted. Default-skip is defensible (real data is clean), but it should be a *recorded* decision, not an unflagged consequence.

#### Medium — Non-UTF-8 input byte hard-errors (Perl preserves it)
`per_read_filtering` reads via `reader.lines()` (`nome.rs:262`), which yields `Result<String>` and returns `Err("stream did not contain valid UTF-8")` on any non-UTF-8 byte. Perl reads bytes with no UTF-8 validation. Reproduced (genome = committed fixture, input = a read ID containing `0xFF`):

- **Perl:** exit 0, emits the data line with the raw `0xFF` preserved (`hexdump`: `72 ff 31 09 …` = `r\xff1\t…`).
- **Rust:** exit **1**, output is **header-only** (the data line is lost), the error surfaces as `BismarkNomeError::Io`.

Two consequences worth noting:
1. The implementer's careful byte-faithful output writes (`w.write_all(id)`, `w.write_all(chr)` at `nome.rs:185-187`) are effectively **moot** — the `String`-based input path can never deliver a non-UTF-8 `id`/`chr` to them (it errors first). The byte-writes are still correct and harmless, just not exercising any path the `&str`→`as_bytes()` couldn't.
2. Real yacht IDs are ASCII, so real-data byte-identity holds. But this is a latent divergence (Perl is byte-transparent; Rust requires UTF-8). If full byte-transparency is desired later, the input loop would need to be `read_until(b'\n')` over `Vec<u8>` rather than `lines()`. Recommend: document this as an accepted divergence (alongside A4), or, if cheap, switch to a byte-oriented line reader to close it. Not blocking for Phase B.

#### Low — Multi-byte col-2 / col-5 truncation divergence
`state_b`/`call_b` take only the **first byte** of col-2/col-5 (`nome.rs:276-277`, `.first().copied().unwrap_or(b'?')`); the tally then compares single bytes (`call == b'z'`, `state == b'+'`). Perl compares the **whole** field (`eq '+'`, `eq 'z'`). On a malformed multi-char field (e.g. col-2 = `"++"`), Perl's `"++" eq "+"` is false (no count) while Rust tallies on `b'+'`. Cannot occur on real `--yacht` (col-2 is exactly one char, col-5 exactly one letter). Low; document or ignore.

### Errors / panics

#### Low — `g = pos as u32 + offset - 1` overflow on pathological coords
`nome.rs:147`. `pos` (usize, ≤ chr_len ≤ `u32::MAX`) cast to `u32` plus `offset` (`u32`) could overflow `u32` in debug (panic) for near-`u32::MAX` coordinates. Underflow is **not** reachable (`pos≥1`, `offset≥0` ⇒ sum ≥1). Overflow is unreachable on real genomes (<4 Gbp). The `u32` chr-length guard in `bismark-io` caps chr_len at `u32::MAX`, but `pos + offset` can still exceed it arithmetically. Purely theoretical; flag only.

### Efficiency / structure — no issues

- `revcomp` allocates a 3-byte `Vec` per reverse-G hit; the fwd path `.to_vec()`s two 3-byte slices per C. Negligible (per-read windows are short) and matches the SPEC's "allocate the 3 bytes separately" note. Not worth optimizing for a faithful port.
- `#[allow(clippy::too_many_arguments)]` on `cytosine_lookup` is justified and documented (mirrors Perl's 7-arg sub + writer).
- `#![forbid(unsafe_code)]` and `#![warn(missing_docs)]` present; clippy clean at `-D warnings`. No `.unwrap()` on parses (uses `match … => continue`). Slice indexing in `classify` is guarded by the `len != 3` early return; `&upstream[0..2]` is guarded by `upstream.len() >= 2`.

---

## Verifications performed (positive confirmations)

1. **Match-guard refactor preserves Perl behavior exactly.** Hand-trace of `main.yacht.txt` against the genome (`chr1 = TTACGTTTCGTTGCGTTTGCAGTTTGCATTACGTTTTTTTTT`) reproduces the golden `1 2 1 1`, exercising CG-ACG-accept, CG-TCG-accept, CG-GCG-reject (the `next` path), CHG-GpC, CHH-GpC, and a trailing CG. The Rust `match ctx { Cg if … => …, Chg if … => …, Chh if … => …, _ => {} }` cannot route a CG context into a nonCG counter or vice-versa, and a guard failure falls to `_ => {}` (no count) — identical to Perl's "disregard"/`next`.
2. **`else→warn-skip` is genuinely unreachable from the live pipeline (SPEC claim CONFIRMED).** For a matched forward C at `seq[i]` (`pos=i+1`), the 2 bp left pad is guaranteed present by the suitability guard (`start≥4`), so `ext[i+2]==seq[i]` and `tri=ext[i+2..i+5]` ⇒ `tri[0]=='C'`. For a matched reverse G, `tri=revcomp(ext[i..i+3])` ⇒ `tri[0]=complement(ext[i+2])=complement('G')=='C'`. Hence whenever `len(tri)==3`, `tri[0]=='C'` and `classify` never returns `None`. `classify(b"NCG")==None` is reachable only via the isolated unit test, never from `process_read`-fed windows. (The `ncontext` golden confirms: it reaches `CNG`→CHG and `CNN`→CHH but never the warn-skip branch.)
3. **`i64` guard == Perl for start∈{0,1,2,3,4}**: Perl `($s-2>1)` and Rust `(s as i64 - 2 > 1)` agree on every value (0,1,2,3→false; 4→true). No underflow panic for `start<2`.
4. **`perl_substr` negative offset == Perl `substr`**: `substr(chr30, end-3=-2, 14)` → `"AC"` (len 2) in both; the degenerate `ext` ⇒ every `tri.len()<3` ⇒ the `edge.golden` all-zero reverse line. `start==L` → empty slice, no panic.
5. **Reverse-G strand path is Perl-faithful** (not covered by any golden — see test gap below). Built a 40 bp genome with a reverse CpG at genomic 7 (`tri=CGA`, upstream `TCG`); both Perl and Rust emit `rg chr1 4 20 0 1 0 0`. The reverse path is real, not a self-fulfilling unit assertion.
6. **Goldens are genuinely from Perl.** `generate_goldens.sh` runs the repo's `./NOMe_filtering` with `--dir .`, gunzips the `.manOwar.txt.gz`, commits the decompressed bytes. Re-running it produced byte-identical `main/edge/empty/ncontext` goldens. Comparison in tests is decompress-then-raw-`assert_eq!` (correct; gzip container not compared). `empty.golden` is exactly 57 bytes = the header line (note: SPEC §3/D4 says "61-byte" — that figure is the *compressed* container size and is irrelevant since the test compares decompressed; not a defect).

---

## Test coverage gaps (under-tested, all non-blocking)

1. **Reverse-G strand path has NO Perl-validated golden.** Both committed goldens that flow through `cytosine_lookup` (`main`, `ncontext`) only land covered calls on **forward-C** positions; the reverse-G `revcomp` `tri`/`upstream` branch is exercised only by the hand-asserted unit test `reverse_g_strand_cpg_tcg_counts_unmeth_cg`. I independently confirmed the path against live Perl (verification #5), so it *is* faithful — but a committed reverse-G golden (e.g. the 40 bp fixture from verification #5, or adding a reverse-strand covered position to `main`) would make the byte-identity guarantee durable against future refactors. **Recommend adding one.**
2. **GpC reverse-strand (CHG/CHH) not golden-covered.** The CHG/CHH GpC tallies in the goldens are all forward-C. Same recommendation — a reverse-strand GpC golden would close it.
3. **The `interspersed-malformed-line` divergence (High finding) is not pinned by any test.** Whatever the decision (accept-skip or Perl-faithful flush), add a test that documents the chosen behavior so it can't regress silently.
4. **No golden for `--dir`-relative input where the input lives inside `--dir`** as the SPEC §12 "VS-dir" case names it. The harness `run_case` *does* copy the fixture into a tempdir and pass `--dir tmp` (so the path contract is exercised), and `cli.rs` unit tests pin the join logic — adequate, but the SPEC called VS-dir out as a *named* golden; consider a comment cross-referencing it.
5. **VS-pad** (CpG as the literal last base of a forward read, upper-pad boundary) and **VS-guard** (exact `chr_len == start-2+length+4`): VS-guard is covered by the `guard_ge_boundary_suitable_and_one_less_not` unit test; VS-pad is not obviously present as a named fixture. Low.

---

## Recommendations (prioritized)

| Priority | Item | Action |
|---|---|---|
| **High** | Interspersed malformed/blank line breaks Perl grouping (skip-vs-flush) | Document the mechanism in `nome.rs` (next to the A4 skips at `:268`/`:274`) and the SPEC; get Felix's explicit call on accept-skip vs Perl-faithful read-boundary. Add a regression test pinning the chosen behavior. Not a real-data blocker. |
| **Medium** | Non-UTF-8 input byte hard-errors (Perl preserves it) | Document as an accepted divergence, OR switch the input loop to a byte-oriented `read_until(b'\n')` over `Vec<u8>` if full byte-transparency is wanted. Note the output byte-writes are then load-bearing. Not a real-data blocker. |
| **Medium** | Reverse-G strand path has no Perl-validated golden | Add a committed reverse-strand golden (CpG and/or GpC) so the `revcomp` branch is locked to Perl, not just to a hand-written unit expectation. |
| **Low** | Multi-byte col-2/col-5 truncation | Document or ignore (unreachable on real data). |
| **Low** | `g = pos as u32 + offset - 1` debug-overflow on near-`u32::MAX` coords | Note only; unreachable on real genomes. |
| **Low** | SPEC §3/D4 "61-byte" header note | Cosmetic: the decompressed header is 57 bytes; 61 was the gz-container size. Comparison is decompressed, so the test is right; just tidy the SPEC wording. |

No Critical findings. The implementation is byte-identical to Perl v0.25.1 on all realistic input; every divergence found is confined to malformed/non-ASCII synthetic input that the real `--yacht` emitter never produces.
