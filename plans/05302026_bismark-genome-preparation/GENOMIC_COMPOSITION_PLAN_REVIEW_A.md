# PLAN REVIEW A — `--genomic_composition` (genomeprep #919)

**Reviewer:** A (independent, fresh context)
**Target:** `GENOMIC_COMPOSITION_PLAN.md` (rev 0, 2026-05-31)
**Perl source of truth:** `bismark_genome_preparation` — `get_genomic_frequencies` (518–543), `process_sequence` (546–570), `read_genome_into_memory` (665–751), top-level flow (184–194).
**Companion:** genomeprep `SPEC.md` §2.7 / §5.3.

**Verdict:** Sound plan. The core algorithm (streaming `prev`-carry, `BTreeMap` sort, `uc`-only counting, N-skipping) is byte-faithful. I found **0 Critical**, **4 Important**, **5 Optional** items — all are precision/test-coverage gaps rather than logic errors that would silently break byte-identity on real genomes, but two of the Important items (empty-line carry; `chomp` final-`\r` semantics) are genuine fidelity hazards on pathological inputs and must at least be pinned by a test or an explicit decision.

---

## 1. Logic review

### 1.1 Di-mer index alignment — CORRECT (verified against Perl)
Perl `process_sequence` (552–568): outer loop `$index` in `0..len-1`; di fires `unless (($index+2) > len)`, i.e. for `$index` in `0..len-2`, emitting `substr($seq,$index,2) = (seq[index], seq[index+1])`.

The plan's streaming model (§2 bullet "di"): at the byte logically at position `i` (current `u`, `prev = p = seq[i-1]`), it emits `(p,u) = (seq[i-1], seq[i])` — equivalent to Perl's di at `$index = i-1`. As `i` runs `1..len-1`, `$index` runs `0..len-2`. **Exact match, no off-by-one.** The plan's §3 "Last base of a chromosome / genome: no trailing di" is correct: the final byte has no successor, so no di is emitted — matching Perl, where at `$index = len-1` the di condition `(len-1+2) > len` is true → skipped.

### 1.2 "NOT ambiguity→N" distinction — CORRECT and load-bearing
`read_genome_into_memory` line 733 does `$sequence .= uc$_;` — **`uc` only**, no `tr`/`s///` mapping. `process_sequence` then counts the raw uppercased bytes, skipping only literal `'N'` (mono, 559) and any 2-mer containing `'N'` (di, 565). The plan (§1 "The frequency path is NOT the conversion path", §2 di/mono bullets, §3 N-skipping) reproduces this precisely and explicitly does **not** reuse `convert::map_into`'s `[^ATCGN]→N` step. This is the single most important correctness point and the plan gets it right. IUPAC codes (`R/Y/S/W/K/M/B/D/H/V`) and stray bytes (space/tab) become their own keys. Confirmed.

One sharpening (Optional, see §5 below): the plan should note that `uc` in Perl uppercases only ASCII `a–z` under the C locale (and the bytes Bismark cares about); a high byte ≥ 0x80 is left unchanged by both Perl `uc` and Rust `b.to_ascii_uppercase()`, so they agree. Worth one sentence so the implementer doesn't reach for a Unicode uppercase.

### 1.3 Sort order — CORRECT, no locale concern
Perl line 534 `sort keys %genomic_freqs` with no comparator and no `use locale` → default string `cmp`, which is **byte-wise** on the underlying bytes. `BTreeMap<Vec<u8>, u64>` iterates in `Ord for Vec<u8>` order = lexicographic byte compare. **These match.** Crucially, this is the *opposite* situation to `discovery::fasta_name_cmp` (which had to case-fold to mimic Perl's `glob`/`bsd_glob` ordering). Here there is **no glob, no locale, no fold** — plain `cmp` on hash keys. The plan (§1 output bullet, §3 "Sort", §4 "sort order" test) is right, including the mono-before-its-di interleave (`A`(0x41) < `AA`(0x41 0x41): a prefix sorts before any extension of it, both in Perl `cmp` and in `Vec<u8>` Ord). **The plan should explicitly state that this case must NOT use `fasta_name_cmp`** — an implementer reusing the discovery module's "Perl sort = our `fasta_name_cmp`" mental model would introduce a case-fold bug here. See Important #3.

### 1.4 Chromosome-boundary carry reset — CORRECT
Perl counts per chromosome: `get_genomic_frequencies` (523–526) calls `process_sequence($chromosomes{$chr})` once per chromosome, on the isolated per-chromosome string. So a di-mer never spans two chromosomes. The plan's "set `prev = None` at each `>` header" (§2, §3) reproduces this. Counts accumulate into one shared `%freqs` across chromosomes (560/566) — order-independent because addition commutes, which the plan notes (§1).

### 1.5 Output path — CORRECT
Perl writes to `"${genome_folder}genomic_nucleotide_frequencies.txt"` (532). `$genome_folder` always carries a trailing `/` (forced at lines 93–94 / 101–102), so the Perl string is `<dir>/genomic_nucleotide_frequencies.txt`. The Rust `config.genome_folder` is a `canonicalize`'d `PathBuf` (cli.rs:191) with **no** trailing slash, so `config.genome_folder.join("genomic_nucleotide_frequencies.txt")` yields the identical path. The plan's choice (§2) is correct. (Note: `canonicalize` resolves symlinks, which Perl `chdir`+`getcwd` also does; no divergence for the filename.)

### 1.6 Mus_musculus.NCBIM37.fa skip — CORRECT, but match-on-bytes
Perl line 694 `next if ($chromosome_filename eq 'Mus_musculus.NCBIM37.fa');` skips by exact basename in the freq path **only** (the conversion path, `create_bisulfite_genome_folders`/`process_sequence_files`, does NOT skip it). The plan (§1, §2 "skip ... by file_name", §3) is correct that this skip lives only in the composition path. **Consistency hazard (Important #1):** the comparison must be on the raw `file_name()` bytes (`b"Mus_musculus.NCBIM37.fa"`), not via `to_str()`, to match `discovery.rs`'s deliberate non-UTF-8 handling (the "M1: match on bytes" convention). The plan says "by file_name" but does not pin byte-vs-str; an implementer using `to_str() == "..."` would be inconsistent with the rest of the crate (harmless for this literal ASCII name in practice, but a real inconsistency a reviewer should flag).

### 1.7 Wiring order — CORRECT
Perl flow (184–192): `create_bisulfite_genome_folders()` → `if ($genomic_composition){ get_genomic_frequencies(); %chromosomes=(); }` → `process_sequence_files()`. So composition runs **after** folder creation, **before** conversion. The plan wires it after `create_tree` (Step I) and before `convert_split` (Step II) — exact match. The `%chromosomes = ()` reset (189) is a Perl memory hygiene step that has **no Rust analogue** (the streaming counter holds no genome in memory), correctly implied by the plan's "no shared state."

---

## 2. Assumptions — validated / flagged

| # | Assumption | Status |
|---|---|---|
| A | `sort keys` is plain byte `cmp`, no locale → `BTreeMap<Vec<u8>>` matches | **VALID** (no `use locale` in script; default sort) |
| B | `read_genome_into_memory` does `uc` only, no `[^ATCGN]→N` | **VALID** (line 733) |
| C | Di never crosses chromosomes; resets at `>` | **VALID** (per-chromosome `process_sequence`) |
| D | `genome_folder.join(...)` == Perl `${genome_folder}file` | **VALID** (trailing-slash forcing 93–102 vs canonicalized PathBuf) |
| E | Empty/N-only genome → empty (0-byte) file | **PLAUSIBLE but UNVERIFIED** — see Important #4 / Open Decision 3. Perl opens the filehandle and writes nothing inside the `foreach sort keys` loop, so it produces a 0-byte file (and still `close`s it). The plan flags this as "confirm" — it is verifiable from the source *now* (no real run needed): empty `%genomic_freqs` ⇒ `open` succeeds ⇒ loop body never runs ⇒ 0-byte file ⇒ `close`. The plan should assert this rather than leave it open. |
| F | gzip input handled by `open_fasta` (`MultiGzDecoder`) | **VALID** — but see Important #2: `open_fasta` is currently a **private** fn in `convert.rs`; reuse requires making it `pub(crate)` or duplicating. Plan says "reuse the gz-aware opener" without noting the visibility change. |

---

## 3. The `chomp` + `s/\r//` semantics (extra-scrutiny area)

Perl per sequence line (712–713): `chomp;` then `$_ =~ s/\r//;`.
- `chomp` removes the trailing `$/` (a single `\n`), if present — exactly one, only if it is the last char.
- `s/\r//` (no `/g`) removes the **first** `\r` anywhere in the (post-chomp) string, exactly once.

For a normal `...seq\n` line: chomp → `...seq`; `s/\r//` → no `\r`, unchanged. For CRLF `...seq\r\n`: chomp → `...seq\r`; `s/\r//` → `...seq`. **Match** with "strip trailing `\n` then first `\r`."

**Pathological cases the plan currently under-specifies (Important #2 below merges these):**
1. **Interior single `\r`** (e.g. `AC\rGT\n`): Perl chomp→`AC\rGT`, `s/\r//`→`ACGT` (the `\r` is *deleted from the middle*, joining `AC`+`GT`). A naive Rust implementation that only strips a *trailing* `\r` (after stripping `\n`) would keep the interior `\r` as a counted byte (`\r` mono key, plus `C\r` and `\rG` di keys) — **divergent**. The plan's §2 says "strip the trailing `\n` then first `\r`" which, read literally as "trailing", does NOT match Perl's "first `\r` anywhere." This is a real fidelity gap on `\r`-containing lines.
2. **Multiple `\r`** (e.g. `A\r\rC\n`): Perl chomp→`A\r\rC`, `s/\r//`→`A\rC` (only the FIRST `\r` removed; a second survives and is counted). A "strip all `\r`" implementation would diverge here, and a "strip trailing `\r`" implementation would also diverge.
3. **Bare `\r\n` with content then `\r` at end** etc.

The plan acknowledges this is "pathological" and says "document, don't over-engineer" (§1) — but the *correct* faithful behavior is specifically "delete the FIRST `\r` (anywhere)", which is neither "strip trailing `\r`" nor "strip all `\r`". The plan's own §2 wording ("strip the trailing `\n` then first `\r`") is slightly self-contradictory (it says "first `\r`" in §1 and §2-prose but the §2 algorithm step phrasing leans "trailing"). **This must be made unambiguous in the plan, and pinned by a test**, even if the team decides real genomes never contain interior `\r` — because if they choose the simpler "strip trailing `\r`" they are knowingly accepting a (tiny) byte-divergence, which should be a recorded decision, not an accident. Note: `convert.rs` solves the analogous problem differently (it keeps `\r` in the keep-set and never strips it, because it re-emits raw lines), so there is **no existing helper to reuse** — the composition path needs its own line-trim that mirrors `chomp`+`s/\r//`.

---

## 4. Validation sufficiency

The proposed tests (§4) cover the main risks well: ACGT mono+di, N split, ambiguity-as-key, di across line boundary, di NOT across chromosome, sort order incl. mono-before-di, plus a Perl-oracle integration test and a real-data E.coli gate. That is a strong matrix. Gaps:

1. **No test for the `\r` semantics** (interior/multiple `\r`). Given §3, at minimum a unit test pinning the chosen behavior on `AC\rGT` and `A\r\rC` is required, and ideally an oracle comparison so the choice is validated against Perl. (Important.)
2. **No test for an empty sequence line mid-chromosome** carrying `prev` across it. Perl: a blank line chomps to `""`, adds nothing, so the di spans it (e.g. `>c\nAC\n\nGT\n` → di `CG` IS counted across the blank line). The streaming model preserves `prev` when a line has zero bytes — correct — but this is exactly the kind of carry interaction that an implementer could break (e.g. by resetting `prev` on any non-`>` line, or by treating EOF/blank specially). **Add a unit test.** (Important, folded into #4.)
3. **No test for the trailing-record-with-no-final-newline** case in the di carry (e.g. a chromosome whose last line lacks `\n`): the last byte must still produce its di with the previous byte but no di after it. (Optional — the index logic in §1.1 already guarantees this, but a cheap regression pin.)
4. **No test that the output file is created in the genome folder, not `Bisulfite_Genome/`** (the §1.5 path). A one-line integration assertion. (Optional.)
5. **Oracle test environment:** the plan auto-skips if `perl` is absent — good, matches the existing `integration.rs` pattern — but the real byte-identity gate (`byte_identity_real_data.rs`, `#[ignore]`) is where the empty-file and `\r` decisions get their final confirmation. The plan should state the empty-genome case is also exercised there (or by a dedicated tiny oracle synthetic), since §3 Open Decision can be closed purely by source-reading + a synthetic oracle without needing the E.coli run.

No scenario where the code could **silently** produce a wrong table goes entirely unguarded *except* the `\r` semantics and the empty-line carry — hence those two are Important.

---

## 5. Open decisions (§5) — recommendations assessed

1. **Write-failure non-fatal** — *Sound.* Perl (532–542) `warn`s and skips, does NOT `die`. The plan's recommendation (non-fatal: log warning, return `Ok(())`) matches exactly. **One nuance to pin:** Perl distinguishes *open* failure (warn, skip table) from a failed `close` (538: `close FREQS or warn`). In Rust the analogue is a write/flush error *after* a successful create. The plan says "Non-fatal on open/write error" — that is *more* lenient than Perl on the write path? Actually no: Perl's `print FREQS` failures are silently ignored (no checking), and `close` failure only warns. So Rust treating any open OR write OR flush error as warn-and-continue is faithful (Perl never dies in this sub). **Recommendation: accept non-fatal for the whole open+write+flush, log one warning. Confirm the warning text need not be byte-matched** (it goes to stderr, not the gated file — correct, stderr is not byte-gated per SPEC).

2. **Duplicate-chr in freq path** — *Sound, with a caveat.* The plan recommends relying on the conversion's dup-check and NOT re-checking in the freq path. **But note the ORDER:** composition runs *before* conversion (§1.7). So on a genuine duplicate-name genome, Perl's `read_genome_into_memory` (716–718/737–739) would `die` **inside the composition step, before the conversion runs** — i.e. Perl errors *earlier* than the Rust plan, which would silently count the dup (its streaming counter doesn't key by name) and only fail later in `convert_split`. The end result (a hard error, no output) is the same, and `genomic_nucleotide_frequencies.txt` is written by Perl only *after* the dup-die would have already fired (so Perl writes no freq file on a dup genome either)... **wait — verify:** Perl `get_genomic_frequencies` calls `read_genome_into_memory` (522) which dies on dup *before* `process_sequence`/the write (532). So Perl produces **no** freq file on a dup genome. The Rust plan would: (a) stream-count successfully, (b) **write the freq file**, (c) then fail in `convert_split`. That is a **behavioral divergence on the dup-genome edge: Rust would leave a `genomic_nucleotide_frequencies.txt` on disk that Perl never creates.** This is niche (real genomes have unique names) and the final exit is non-zero either way, but if strict byte/artifact-identity matters on error paths, the freq path should detect dup names (cheap: track a `HashSet<Vec<u8>>` of header names and error before writing) so it dies before writing, exactly like Perl. **Reviewer recommendation: Important — either replicate the dup-die in the freq path (preferred, trivially cheap, removes a real divergence) OR explicitly document that on a dup-name genome the Rust port leaves a stray freq file whereas Perl does not, and accept it.** The plan currently leans "rely on conversion's check" without noticing the ordering means Perl dies *before* writing while Rust writes *then* dies.

3. **Empty-table output** — *Sound but should be closed now.* As noted (§2 assumption E), this is determinable from the source without a run: empty `%freqs` ⇒ 0-byte file. The plan should change "confirm Perl writes a 0-line file" to an asserted fact + a unit/oracle test, not leave it open.

---

## 6. Efficiency

Negligible concern. The Rust plan **streams** (`read_until`) and counts into a `BTreeMap` — strictly better than Perl, which **slurps the whole genome into `%chromosomes`** then iterates (SPEC §8.12 explicitly calls the Perl path a memory consideration). `BTreeMap<Vec<u8>, u64>` has at most a few hundred keys (256 mono + up to 65536 di, realistically <300), so per-byte `entry(vec![...])` allocations (1–2 byte `Vec`) are the only inefficiency. Optional micro-opt: key on a fixed `[u8; 2]` + length, or `u16`/`SmallVec`, to avoid a heap alloc per byte — but at ~3 GB genome × 1 alloc/byte this *is* measurable. **Optional**: use a stack key (e.g. `[u8;2]` with a discriminant, or two `[u64; 256]` / `[u64; 65536]` count arrays then emit sorted) to avoid per-byte heap churn; emit by iterating the arrays in byte order (mono index, then di) — naturally sorted, no `BTreeMap` needed. This also sidesteps the `Vec<u8>` allocation entirely. Not required for correctness; flag for the implementer since this is the hot loop over the whole genome.

---

## 7. Alternatives

- **Count arrays instead of `BTreeMap`** (see §6): `[u64;256]` mono + `[u64;65536]` di. Emitting in order = iterate 0..256 (mono `b`, skip `N`=0x4E and zero-count), then 0..65536 (di `hi,lo`, skip any with `N`, skip zero-count). Byte-sorted-by-key requires interleaving mono and di in a single ordered stream — which `BTreeMap` does for free. So the array approach trades the alloc-free hot loop for slightly more fiddly merge-emit. Given correctness-first and the plan's byte-identity stakes, **`BTreeMap` is the safer choice; keep it** and note the array option only if profiling on human genomes shows the alloc to matter.
- **Reusing `convert::open_fasta`** requires a visibility bump (Important #2). Alternative: a tiny private opener in `composition.rs` duplicating the 12-line `open_fasta`. Duplication is mildly worse than bumping `open_fasta` to `pub(crate)`; recommend the visibility bump and a doc note that both paths share the gz semantics.

---

## 8. Action items

### Critical
*(none)*

### Important
1. **`\r` semantics must be unambiguous + tested.** The plan's §1 says "first `\r`" but §2's algorithm phrasing leans "trailing `\r`". Perl's `s/\r//` deletes the **first** `\r` *anywhere* in the post-`chomp` line (interior `\r` joins the flanks; a second `\r` survives and is counted). Pin the exact rule in the plan and add unit tests on `AC\rGT` (→ `ACGT`) and `A\r\rC` (→ `A\rC`, the surviving `\r` counted). If the team chooses the simpler "strip trailing `\r`" for speed, record it as an accepted divergence — do not leave it implicit. No reusable helper exists (`convert.rs` keeps `\r`), so this is new code.
2. **Empty-sequence-line carry test + `open_fasta` visibility.** (a) Add a unit test that a blank line mid-chromosome does NOT reset `prev` (`>c\nAC\n\nGT\n` ⇒ di `CG` counted). (b) The plan says "reuse the gz-aware opener" but `convert::open_fasta` is private — note the required `pub(crate)` bump (or a small local duplicate).
3. **Explicitly forbid `fasta_name_cmp` for the key sort.** State in the plan that the freq-table sort is plain byte `cmp` (`BTreeMap<Vec<u8>>`), NOT the case-folding `discovery::fasta_name_cmp` used for the glob — these are different Perl constructs (`sort keys` has no locale/fold; `glob` does). Prevents an easy copy-paste fold bug. Add the existing §4 sort-order test asserting `A` before `AA` and case-distinct keys in byte order.
4. **Resolve the dup-name ordering divergence (Open Decision 2).** Composition runs *before* conversion, so on a duplicate-chromosome genome Perl `die`s inside `read_genome_into_memory` **before writing any freq file**, whereas the Rust plan would stream-count, **write `genomic_nucleotide_frequencies.txt`, then** fail in `convert_split` — leaving a stray file Perl never creates. Either replicate the dup-die in the freq path (cheap `HashSet<Vec<u8>>` of header names, error before write) — preferred — or document and accept the stray-file divergence on this error path.

### Optional
5. **Close Open Decision 3 from source now:** empty/N-only genome ⇒ empty `%freqs` ⇒ 0-byte file (open succeeds, write-loop never runs). Change "confirm" to an asserted fact and add a unit + tiny synthetic-oracle test.
6. **`Mus_musculus.NCBIM37.fa` skip on bytes,** not `to_str()`, to match the crate's non-UTF-8 `file_name` convention (`discovery.rs` "match on bytes" M1). Harmless for this ASCII literal but keeps the convention consistent.
7. **One sentence on `uc` ASCII-only equivalence** (Perl C-locale `uc` vs Rust `to_ascii_uppercase`): both leave bytes ≥ 0x80 unchanged, so they agree; don't reach for Unicode uppercase.
8. **Trailing-no-newline + last-base regression test** (chromosome whose final line lacks `\n`): last byte produces its di with `prev` and none after.
9. **Hot-loop alloc micro-opt (defer unless profiled):** per-byte `entry(vec![..])` allocates over the whole genome; `[u64;256]`+`[u64;65536]` count arrays avoid it but complicate sorted emit. Keep `BTreeMap` for correctness-first; note the option for the implementer.

---

## Summary
**0 Critical, 4 Important, 5 Optional.** The plan's central byte-identity claims — `uc`-only counting (not the conversion transform), per-chromosome di carry with header reset, N-skipping (mono: only `N`; di: any 2-mer with `N`), and byte-wise `BTreeMap` sort — are all **verified correct against the Perl source** (lines 518–570, 665–751), with no off-by-one in the di index mapping. The Important items are fidelity/precision gaps, not algorithm errors: (1) the `s/\r//` "first `\r` anywhere" rule is under-specified vs the plan's "trailing" phrasing and untested; (2) the empty-line carry and the private `open_fasta` reuse need pinning; (3) the sort must be explicitly distinguished from `fasta_name_cmp`; (4) the dup-name path has a real before-vs-after-write ordering divergence with Perl that produces a stray file on the error path. Closing these (plus the optional tests) makes the plan ready to implement.

**File:** `/Users/fkrueger/Github/Bismark-genomeprep/plans/05302026_bismark-genome-preparation/GENOMIC_COMPOSITION_PLAN_REVIEW_A.md`
