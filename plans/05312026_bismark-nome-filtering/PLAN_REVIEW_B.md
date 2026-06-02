# PLAN_REVIEW_B — `bismark-nome-filtering` SPEC (rev 0)

**Reviewer B** · independent fresh-context review · 2026-05-31
**Target:** `plans/05312026_bismark-nome-filtering/SPEC.md` vs Perl `NOMe_filtering` v0.25.1 (660 LOC)
**Method:** Re-derived every arithmetic claim against the Perl source, then **executed the actual `./NOMe_filtering` script** against synthetic genome+yacht fixtures to capture ground-truth bytes. All quoted outputs below are real Perl v0.25.1 runs.

**Bottom line:** The SPEC's core arithmetic (§8/§9) is **correct** — I verified the `pos=i+1` mapping, the fwd-C and rev-G `substr` offsets, the genomic key `pos+offset-1`, the `tr/ACTG/TGAC/` complement, the context regexes, the NOMe filters, the column-2 tally, the ascending offset/end output, and the reverse-edge all-zero asymmetry, all against live Perl output and they match. The SPEC is implementation-grade. However I found **two genuine behavioral gaps** (empty-input file-state divergence; same-position last-wins semantics unstated) and several documentation/test-surface hardening items. None are arithmetic errors.

---

## Logic review

### What I verified as CORRECT (ground-truth Perl confirmed)

| SPEC claim | §ref | Perl ground truth | Verdict |
|---|---|---|---|
| `pos = i + 1` (pos() returns offset after match) | §8 L97 | `/([CG])/g` on `"ACGTC"` → C@idx0 gives `pos()=1`, etc. | ✅ correct |
| fwd-C: `tri=substr(ext,pos+1,3)`, `up=substr(ext,pos,3)` | §8 L98 | READ1 C@pos14 → tri=`CGG` up=`ACG` (genomic 18) — matched live output meth_CG=1 | ✅ correct |
| rev-G: `tri=substr(ext,pos-1,3)` revcomp; `up=substr(ext,pos,3)` revcomp | §8 L99 | RS read G-path @pos5 → tri=`CGC` up=`ACG` → unmeth_CG=1, live output `RS chr1 5 12 0 1 0 0` | ✅ correct |
| genomic key `g = pos + offset - 1` | §8 L102 | reverse read offset=`last_end`=5: pos5→g9, matched stored call at 9 | ✅ correct |
| `tr/ACTG/TGAC/` leaves N + other bytes unchanged | §8 L99, P3 | `"ACGTNXacgt"` → `"TGCANXacgt"` (only uppercase ACTG mapped) | ✅ correct |
| context regexes `^CG`/`^C.G$`/`^C..$` | §8 L101 | all classifications matched manual trace | ✅ correct |
| CG filter: call∈{z,Z} AND up∈{ACG,TCG}; GCG/CCG reject | §8 L104 | G1 read: up=ACG→counted, up=CCG→rejected (no count) | ✅ correct |
| CHG/CHH filter: call∈{x,X}/{h,H} AND up=~`^GC` | §8 L105-6 | G1 read genomic13 CHH up=GCA `h`/`+` → meth_GC=1 | ✅ correct |
| tally keys on **column-2** state (+/-), not call-letter case | §8, P5 | C-read same-pos +Z then -Z → unmeth (last state -) | ✅ correct |
| stored-call/context mismatch silently disregarded | §8 L107 | READ1 genomic7 ctx=CHH but stored `z` → no count | ✅ correct |
| offset/end ascending (min/max) for reverse | §6, P9 | RS output `5 12`, READ2 output `2 10` | ✅ correct |
| output columns `id chr offset end meth_CG unmeth_CG meth_nonCG unmeth_nonCG` + header bytes | §6 | byte-exact match incl. header `meth_GC`/`unmeth_GC` | ✅ correct |
| suitability guard uses `last_start` for BOTH strands | §8 L91, P2 | reverse RR (last_start=20,len16) NOT suitable: `20-2+16+4=38 > 30` → "Processed 0 reads" | ✅ correct (do NOT "fix") |
| reverse `end∈{1,2}` → all-zero line; forward `start≤3` → no line | §8 L95, P1/D3 | READ2 (end=2)→`2 10 0 0 0 0`; READ3 (start=3)→no line; both in one run | ✅ correct |
| unknown chromosome → silently skipped (length(undef)==0) | §7 L83 | READ4 chrZ → no line, stderr "uninitialized value in numeric ge" | ✅ correct |
| `.manOwar.txt.gz` filename derivation (strip 1 `.gz`, 1 `.txt`, append, force `.gz`) | §4 L57 | `x.txt.gz`/`x.gz`/`x.txt`→`x.manOwar.txt.gz`; `x.foo`→`x.foo.manOwar.txt.gz` | ✅ correct |
| `--merge_CpGs`+`--CX` is the ONLY reachable die; `--merge_CpGs` alone inert | §4 L55 | die fires for the combo; `--merge_CpGs` alone → exit 0, normal output | ✅ correct |
| consecutive-ReadID grouping; A,B,A → 3 lines (A flushed twice) | §5, P10 | live: 3 lines, A appears twice with different counts | ✅ correct |
| genome glob: `<*.fa>` strict-falls-back to `<*.fasta>` (no union), skips dotfiles | §7 L80, P6 | `<*.fa>`→`[chr1.fa]` only (not `.fasta`, not `.hidden.fa`); `.fasta` fallback works; missing both → die | ✅ correct |

This is an unusually clean transcription. The arithmetic crux the orchestrator flagged as highest-risk is **right**.

### Gaps / divergences found

**L1 (Important) — Empty-input is NOT a clean pre-output error; Perl writes the header THEN dies.**
The SPEC §5 (L68) and §10 (L123) model `EmptyInput` as a typed error and imply a dedup-style "die before any output." That is **not** what Perl does. Live run on empty input AND on all-`^Bismark` input:
```
>>> Writing genome-wide cytosine report to: empty.manOwar.txt.gz <<<
No last read was defined, ... (die, non-zero exit)
$ ls -la empty.manOwar.txt.gz   →  61 bytes on disk
$ gunzip -c empty.manOwar.txt.gz → ReadID\tChr\tStart\tEnd\tmeth_CG\tunmeth_CG\tmeth_GC\tunmeth_GC\n
```
Perl opens `CYT`, prints the header (`:78`), enters the `while` loop, reads nothing, falls through, and only THEN hits the `unless (defined $last_read)` die (`:173-175`). So **Perl leaves a header-only gzipped file on disk and exits non-zero.** If the Rust raises `EmptyInput` before opening the writer (the natural design), the on-disk state diverges: Rust leaves no file (or no header) where Perl leaves a 61-byte `.gz`. The byte-identity golden gate only compares **successful** runs, so this would NOT be caught by tests as currently scoped. **Decide and document:** either (a) replicate Perl exactly (open writer, write header, then error — leaving the file), or (b) deviate intentionally (error pre-output, no file) and record it as a deviation alongside the dropped `sleep(2)` calls. Note the SPEC already classifies STDERR as non-gated, but **on-disk output-file state is not STDERR** — it's the one gated artifact. My recommendation: replicate (a) for true fidelity; it's cheap (open writer + write header before the read loop, exactly mirroring Perl's order).

**L2 (Important) — Same-position last-wins semantics are tested but never specified.**
§12 lists "a read with multiple calls at the same position" as a test fixture but neither §5 nor §8 states the resolution rule. Live Perl: two calls at the same `pos` within one read → `$read->{$pos}->{state}` is **overwritten**, last line wins:
```
C +Z @pos18 ; C -Z @pos18  →  output  C chr1 5 20 0 1 0 0   (unmeth — last state '-')
```
A Rust `HashMap::insert` is also last-wins, so the natural port matches — **but only if the grouping loop inserts unconditionally in input order.** If an implementer chooses `entry().or_insert()` (insert-if-absent) "to be safe," it becomes first-wins and silently diverges. Pin the rule in §5/§8: *"same-position duplicate within a read → last line wins (overwrite); use unconditional insert in input order."* Add this as an explicit assertion in the §12 test (not just 'a read with multiple calls at the same position').

**L3 (Optional) — `read = ()` then immediate re-init is a hash reset, faithfully a fresh map.**
Perl `:165` `$read = ();` clears, then `:166-167` seeds the new read's first line. The SPEC's "re-init for the new one" (§5) covers this, but be explicit that the new read's **first line is seeded during the flush of the previous read** (the `else` branch handles both flush-previous AND seed-current). The "one shared flush routine" (§8 L87) must NOT also do the seeding — seeding belongs to the loop body after the flush returns, or the EOF flush (which does NOT seed) will diverge. This is a real structuring trap worth a sentence.

**L4 (Optional) — `--dir` chdir happens inside `per_read_filtering` and changes CWD before output.**
Perl `:58-61` does `chdir $output_dir` at the top of processing, and the genome reader earlier did `chdir $genome_folder` then `chdir $parent_dir` (`:519,589`). The SPEC §4 mentions `--dir` "chdir into it to write" and `--parent_dir` "restored after reading the genome." The Rust port should resolve the output path explicitly (join dir + filename) rather than literally `chdir`, but must reproduce that the output filename is the **basename-derived** name written **into** `--dir`. The SPEC is silent on whether the infile path passed on the CLI is treated as basename-only when deriving the output (Perl derives `$cytosine_out` from the raw `$coverage_infile` which may include a path, but then `chdir`s into `--dir` and opens the *relative* `$cytosine_out` — so a path-qualified infile + `--dir` produces a path-qualified output relative to dir). Edge: `--dir /out/ ./sub/x.txt` → Perl writes `/out/./sub/x.manOwar.txt.gz`? Worth one fixture or an explicit "infiles arrive as bare filenames from the extractor; path-qualified infile + --dir is unsupported/untested" note (the Perl comment at `:65` says infiles "will be just the filename on their own"). Low risk because real callers pass bare names, but undocumented.

---

## Assumptions

- **A1 — `perl_substr` boundary `start == L` → empty (not undef).** Verified: `substr("ABCDEFGH",8,3)` returns `""` (len 0), NOT undef. The SPEC §9 formula `min(len, L-start)` yields 0 bytes here, which is correct, but the prose (L114) only enumerates `start<0` / `start>L` → empty. Add `start==L` explicitly to the unit-test matrix; it's the exact boundary the reverse-edge degenerate path lands on (`ext_seq="T"`, then `substr(ext,pos+1,3)` with pos near len). Confirmed live: READ2 path produced UNDEF (offset>L) and `""` (offset==L) interchangeably, both → skip.
- **A2 — rev-G `substr(ext,pos-1,3)` never needs negative-offset handling.** `pos≥1` always (pos() after a match is ≥1), so `pos-1≥0`. The only negative offset in the whole tool is `ext_seq=substr(chr,last_end-3,…)` for reverse reads with `last_end∈{1,2}`. The SPEC's `perl_substr(s,offset:isize,…)` signature correctly anticipates this; just confirm the genomic-window extraction is the sole caller passing a possibly-negative offset.
- **A3 — additive `bismark_io::genome` with no version bump is safe.** Verified: `bismark-io` is `1.0.0-beta.8`; siblings (dedup, extractor, methylation-consistency) pin `=1.0.0-beta.8`; **c2c does NOT depend on bismark-io at all** (it has its own local `genome.rs` + `noodles-fasta`/`flate2` direct deps). Adding `pub mod genome;` is purely additive and cannot break a pinned consumer. **Caveat (see V/efficiency): error-type ownership** — if genome errors are added as **new variants of the public `BismarkIoError` enum**, any sibling that matches it **non-exhaustively-without-`_`** could fail to compile. Confirm `BismarkIoError` is `#[non_exhaustive]` OR that genome errors live in a genome-local error type. The SPEC §10 says NOMe's error enum surfaces "genome errors from `bismark_io::genome`" — pin whether those are new `BismarkIoError` variants or a separate `GenomeError`. This is the one place "additive, no bump" could bite.
- **A4 — two-plain-tier glob `[".fa",".fasta"]` (no `.gz`) is correct for NOMe.** Verified against Perl `:522-527`: `<*.fa>` then fallback `<*.fasta>` — **no gzip tier exists in `NOMe_filtering`** (unlike c2c). The SPEC's deliberate divergence from c2c's 4-tier is right. ⚠️ Consequence: a genome stored as `.fa.gz` (common!) that works fine in c2c will be **invisible** to NOMe and trigger the "does not contain any sequence files" die. This is Perl-faithful, but the SPEC should call it out as a user-facing footgun (the c2c session may hand a `.fa.gz` genome and be surprised). Worth a sentence in §7 and a pitfall row.
- **A5 — uppercase-on-load, Mus skip, `\r` strip, first-token name, dup-name error.** All confirmed in the c2c `genome.rs` twin (which the promoted module mirrors). One divergence the c2c module already documents: a **bare `>` header** (no name) → c2c **errors** (noodles InvalidData) where Perl stores an empty-name chromosome. For NOMe this can never matter (Bismark genomes have clean headers, and an empty-name chr is never referenced by yacht), but if the promoted module keeps c2c's noodles-based reader, it inherits this documented divergence — acceptable, just note it carries over.

---

## Efficiency

- Whole-genome-in-RAM is accepted and matches Perl. Fine.
- Per-read map `HashMap<u32,(state,call)>` is appropriate; reads are short so `FxHashMap` is marginal. The `state`/`call` can be single `u8` bytes — no allocation per call.
- **The genome key existence check** `if exists $read->{$pos+$offset-1}` (`:305`) is the hot inner check; a `u32` key lookup is O(1). Good.
- The regex walk `while ($seq =~ /([CG])/g)` is over the genome **reference** slice (not the read) — the SPEC correctly ports this as a byte scan for `b'C'|b'G'`. A simple `for (i,b) in seq.iter().enumerate()` with `pos=i+1` is exact and faster than a regex. Confirm the implementation uses a byte scan, not the `regex` crate (no need to pull it in).
- No scalability concern: output is per-read and small (SPEC §12 notes this is why the c2c oxy disk-retarget doesn't apply). Agreed.

---

## Validation sufficiency

The §12 surface is strong and the golden matrix covers the headline edges (forward `start≤3` no-line, reverse `end∈{1,2}` all-zero, unknown chr skip, ACG/TCG pass + GCG reject, GpC in CHG/CHH, `^Bismark` skip, gz round-trip). Gaps to close:

- **VS1 (Important) — Empty-input on-disk state untested (ties to L1).** No fixture asserts what file (if any) exists after empty/all-`^Bismark` input. Add a golden that runs Perl on empty input, captures the header-only `.gz`, and asserts the Rust produces the same on-disk artifact (or documents the intentional deviation). This is currently a silent-divergence hole.
- **VS2 (Important) — Same-position last-wins untested as an assertion (ties to L2).** §12 lists the fixture but not the expected resolution. Add `+Z then -Z @same pos → unmeth` as a hard `assert_eq!`.
- **VS3 (Optional) — `perl_substr` `start==L` boundary** (ties to A1) — add to the §9 adversarial unit tests.
- **VS4 (Optional) — CRLF yacht input.** §12 tests CRLF genome (inherited from c2c) but not CRLF in the **yacht input**. Perl `chomp` strips `\n` but leaves `\r`; the last field `$strand` would carry a trailing `\r`, and a `\r` on `$pos`/`$start`/`$end` numerics is silently tolerated by Perl numeric context. The Rust split-on-`\t` + parse would need to handle/trim `\r`. Low risk (yacht is machine-generated LF), but a one-line fixture removes doubt. Note: Perl does **not** strip `\r` from yacht lines (only `chomp`), so a CRLF yacht file feeds `\r`-suffixed `$strand` — which never matters because the tally keys on column-2 `$state` and column-2 has no `\r`. Still, pin the behavior.
- **VS5 (Optional) — Read whose extraction window straddles a chromosome with a CpG at the very last 2bp pad.** §12 has "CpG at end" in the genome but should ensure a read whose `last_start-2+length+4 == chr_len` exactly (guard boundary `>=`) is tested — the `>=` (not `>`) boundary is easy to get wrong (off-by-one in the guard). Add a read sized to hit `chr_len == last_start-2+length+4` exactly (suitable) and `chr_len == that-1` (not suitable).
- **VS6 (Optional) — non-ACGTN genome bytes.** A genome byte like lowercase (pre-uppercase) or IUPAC `R`/`Y`. Uppercasing handles soft-mask; an IUPAC base in `tri_nt` → context regex `^C..$` could still classify as CHH, and `tr/ACTG/TGAC/` leaves it unchanged. Perl would warn "context could not be determined" only if the trinucleotide doesn't match any of the three patterns. Add a fixture with an `N`-run (already in §12) AND one with a stray IUPAC byte to confirm the unclassifiable→skip path (the `else{warn;next}` at `:300-303`, which the SPEC §8 L101 notes but no fixture exercises a genuinely-unclassifiable real `tri_nt` like `CN` truncated or `GG`-leading). Actually note: the scan only fires on `C`/`G` seq bytes, and a `G`-strand revcomp always begins with `C` after complement, so reaching the `else` die-skip requires a non-ACGT byte making the revcomp not start with `C` — e.g. seq `G` with ext `...NG.` → revcomp could be `CN?`/non-`C`-leading. Add one fixture to actually hit the `warn+next` branch; currently §12 only claims `CNG`→CHG / `CNN`→CHH which still classify.
- **VS7 (Optional) — Phase C single-end only.** §12 real-data gate uses `--yacht` single-end. Correct — `NOMe_filtering` help (`:612`) explicitly says "single-end reads"; the yacht emitter for PE is out of scope. Just confirm the extractor's `--yacht` PE output is genuinely never fed here (the tool assumes one read per ReadID group spans contiguous coords).

---

## Alternatives

- **A-alt1 — Replicate empty-input header-then-die vs deviate.** (See L1.) Replicating is cheaper for fidelity and avoids a documented-deviation footnote that someone will later trip over. Recommend replicate.
- **A-alt2 — Byte scan vs regex for the C/G walk.** Use a plain `for (i,&b)` byte loop (`pos=i+1`); do not pull in the `regex` crate. The SPEC implies this but doesn't forbid `regex` — say so.
- **A-alt3 — `perl_substr` return type.** `&[u8]` (borrow from `ext_seq`) is allocation-free and sufficient since the revcomp produces a fresh `Vec<u8>` anyway only on the `G` branch. Prefer `&[u8]` return; revcomp allocates a 3-byte `Vec`/array on the rev path. A `[u8;3]`+len-tracking avoids even that, but it's premature — fine either way.
- **A-alt4 — Genome error ownership.** (See A3.) A genome-local `GenomeError` in `bismark_io::genome` (rather than new `BismarkIoError` variants) keeps the "additive, no bump" claim airtight against non-exhaustive sibling matches. Recommend a dedicated error or confirm `BismarkIoError` is `#[non_exhaustive]`.

---

## Action items (prioritized)

### Critical
*(none — no arithmetic or correctness defects found; the byte-identity crux is correct)*

### Important
1. **L1 / VS1 — Empty-input file state.** Perl opens the writer, prints the header, THEN dies on empty/all-`^Bismark` input, leaving a 61-byte header-only `.gz` on disk and exiting non-zero. Decide: replicate exactly (recommended — write header before the read loop) or deviate intentionally and document it next to the dropped `sleep(2)`. Add a fixture/assertion for the on-disk artifact. *(Perl `:74-78,89-90,173-175`; SPEC §5 L68, §10 L123.)*
2. **L2 / VS2 — Same-position last-wins.** Pin in §5/§8 that a duplicate `pos` within one read overwrites (last line wins; unconditional insert in input order — NOT `or_insert`). Add a hard `assert_eq!` fixture. *(Perl `:107-108,166-167`; SPEC §5, §12 L137.)*
3. **A3 / A-alt4 — Genome error-type ownership.** Confirm genome errors are either a genome-local type or that `BismarkIoError` is `#[non_exhaustive]`, so "additive, no version bump" cannot break a sibling's non-exhaustive match. State it in §7/§10. *(SPEC §10 L123, §3 D1.)*

### Optional
4. **A4 — `.fa.gz` genome invisibility footgun.** NOMe's two-plain-tier glob (Perl-faithful) means a `.fa.gz`-only genome dies with "no FASTA files." Add a §7 sentence + pitfall row so the c2c session isn't surprised. *(Perl `:522-529`.)*
5. **VS5 — Guard `>=` boundary fixture.** Add reads at `chr_len == last_start-2+length+4` (suitable) and one less (not). *(Perl `:132,194`.)*
6. **VS6 — Unclassifiable-`tri_nt` skip branch.** Add a fixture that actually reaches the `:300-303` warn+next (e.g. non-ACGT byte breaking the revcomp `^C`), not just `CNG`/`CNN` which still classify.
7. **A1 / VS3 — `perl_substr` `start==L`→empty** in the §9 unit matrix (verified: returns `""`, not undef).
8. **VS4 — CRLF yacht input fixture** (Perl does NOT `\r`-strip yacht lines; trailing `\r` lands on `$strand` and is harmless because tally keys on col-2). Pin the Rust trim/no-trim choice.
9. **L3 / L4 — Document flush-vs-seed separation and `--dir`/basename output-path resolution** (one sentence each).
10. **A-alt2 — State "byte scan, not the `regex` crate"** for the C/G walk in §10.

---

### Top 5 findings (for orchestrator)
1. **Arithmetic crux is CORRECT** — `pos=i+1`, fwd/rev `substr` offsets, `pos+offset-1`, `tr/ACTG/TGAC/`, context regexes, NOMe filters, col-2 tally, ascending offset/end, and the reverse-edge all-zero vs forward-no-line asymmetry all verified against **live Perl v0.25.1 output**. No correctness defects.
2. **Important behavioral gap:** Perl writes the output **header then dies** on empty/all-`^Bismark` input (leaves a 61-byte header-only `.gz` on disk, exits non-zero). SPEC models `EmptyInput` as a clean pre-output error — divergence the golden gate won't catch. Decide replicate-vs-deviate + add a fixture.
3. **Important under-specification:** same-position duplicate-within-a-read is **last-wins** (overwrite) in Perl; SPEC tests it but never states the rule — an `or_insert` implementation would silently diverge. Pin "unconditional insert, last wins."
4. **"Additive, no version bump" verified sound** (bismark-io beta.8, siblings pin it, c2c doesn't even use it, no `genome` module yet) — **but** confirm genome errors don't add non-exhaustive variants to public `BismarkIoError`.
5. **Footgun:** two-plain-tier glob (`.fa`/`.fasta`, no `.gz`) is Perl-faithful but means a `.fa.gz` genome is invisible and dies — worth documenting for the parallel c2c session.

**Report path:** `/Users/fkrueger/Github/Bismark-nome/plans/05312026_bismark-nome-filtering/PLAN_REVIEW_B.md`
