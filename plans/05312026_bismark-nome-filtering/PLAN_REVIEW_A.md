# Plan Review A — `bismark-nome-filtering` SPEC (rev 0)

**Reviewer:** A (independent, fresh context)
**Target SPEC:** `plans/05312026_bismark-nome-filtering/SPEC.md`
**Byte-identity reference:** `NOMe_filtering` (Perl v0.25.1, 660 LOC) — read in full.
**Method:** I re-derived the Perl arithmetic independently with live `perl -e` experiments (`pos()`, `substr` rvalue edges, `tr`, the reverse-edge trace, the glob) rather than trusting the SPEC's transcription. Findings below cite Perl line numbers and SPEC section numbers.

**Bottom line:** The SPEC is unusually accurate. Every load-bearing arithmetic claim I checked (pos→index, fwd/rev substr offsets, genomic key, the guard asymmetry, `tr` on N, glob suffixes, filename derivation, header bytes) is **correct**. The risks that remain are (a) a handful of *under-specified* but reachable behaviors the test surface does not pin, and (b) two small documentation imprecisions that could mislead the implementer. No Critical correctness defect found in the transcription itself; the Criticals below are about validation gaps where the port could silently produce wrong bytes with no test to catch it.

---

## 1. Logic review (independent re-derivation of the Perl semantics)

### 1.1 `pos = i + 1` mapping (SPEC §8) — CORRECT
`perl -e` on `"ACGTC"` (`/([CG])/g`): C@idx1→pos=2, G@idx2→pos=3, C@idx4→pos=5. So after a match at 0-based index `i`, `pos($seq)` = `i+1`. SPEC §8 ("set `pos = i + 1`") and pitfall P4 are exactly right.

### 1.2 Forward-C `tri_nt`/`upstream` offsets (Perl :265,267) — CORRECT
Live trace with `chr="TTACGTACGTAA"`, `last_start=4`, `len=5`:
- `seq = substr(chr,3,5)="CGTAC"`, `ext_seq = substr(chr,1,9)="TACGTACGT"`.
- ext_seq index 0 = chr position `last_start-2`. A C at seq index `i` sits at ext_seq index `i+2 = pos+1`, so `tri_nt = substr(ext_seq,pos+1,3)` reads [C, +1, +2] — confirmed `CGT` for the C@pos1. `upstream = substr(ext_seq,pos,3)` reads [base-before-C, C, base-after] = `ACG`. **SPEC §8 forward offsets are exact.**

### 1.3 Reverse-G `tri_nt`/`upstream` (Perl :274-282) — CORRECT
`tri_nt = substr(ext_seq,pos-1,3)` then `reverse` then `tr/ACTG/TGAC/`; `upstream = substr(ext_seq,pos,3)` then reverse+complement. Verified `revcomp("CGA")="TCG"` (reverse→`AGC`, tr→`TCG`). SPEC §8 G-branch matches.

### 1.4 Genomic coverage key `g = pos + offset - 1` (Perl :305) — CORRECT
seq index 0 = chr 1-based position `offset` (= `last_start` fwd, `last_end` rev). A C/G at seq index `i` is genomic `offset + i = offset + (pos-1) = pos + offset - 1`. Live trace confirmed genomic keys 4,5,8 for `offset=4`. **The map is keyed by the yacht column-4 genomic position, and the lookup key derives the same coordinate — consistent.** SPEC §8 step "Coverage" is correct. *(Minor: the SPEC could state explicitly that the per-read map keys ARE the yacht col-4 genomic positions, so the implementer doesn't accidentally key by read-relative index. See §2.1.)*

### 1.5 Suitability guard asymmetry (Perl :132 / :194) — CORRECT and well-flagged
Guard: `($last_start - 2 > 1) && (chr_len >= $last_start - 2 + $length + 4)`. It uses `$last_start` for **both** strands (P2). I confirmed both halves of the SPEC's central claim:

- **Forward `start ≤ 3` → NO line.** `(last_start-2 > 1)` is true only for `last_start ≥ 4`; for `last_start ∈ {1,2,3}` clause 1 is FALSE → `suitable=0` → no `cytosine_lookup` call → no line. Verified.
- **Reverse `last_end ∈ {1,2}` → all-zero line.** Live trace: `chr` len 12, `last_start=5`, `last_end=1`, `length=5`. Guard: `5-2=3>1` TRUE, `12 >= 3+5+4=12` TRUE → **suitable=1**. Then `ext_seq = substr(chr, last_end-3, length+4) = substr(chr,-2,9) = "GT"` (2 bytes). In `cytosine_lookup`, every `tri_nt` extraction hits `length($tri_nt) < 3` → `next` → all four counts stay 0 → `print CYT ... 0 0 0 0`. Confirmed the emitted line is `id chr offset end 0 0 0 0` with `offset=last_end=1`, `end=last_start=5`. **SPEC §8 "Reverse with `end ∈ {1,2}`" and pitfall P1 are exactly right.**

This asymmetry is the single most counter-intuitive behavior in the tool and the SPEC nails it. It MUST be tested (see Critical C1).

### 1.6 `perl_substr` rvalue semantics (SPEC §9) — CORRECT, with one boundary nuance
Live `perl -e` on `"ABCDEFGH"` (L=8):
| call | Perl result | SPEC §9 rule | match? |
|------|-------------|--------------|--------|
| `substr(s,-3,3)` | `"FGH"` | neg in-range → tail bytes | ✓ |
| `substr(s,-20,3)` | **undef** | `\|offset\|>L` → empty | ✓ (L+offset<0) |
| `substr(s,6,5)` | `"GH"` | over-length → truncate `min(len,L-start)` | ✓ |
| `substr(s,20,3)` | **undef** | `start>L` → empty | ✓ |
| `substr(s,8,3)` | **`""`** (empty, NOT undef) | — | see note |

**Nuance:** at exactly `start == L` Perl returns the **empty string** (defined), whereas `start > L` returns **undef**. The SPEC §9 rule "`start < 0` OR `start > L` → empty" is correct for the over-by-one+ case, but does not separately mention `start == L` → `""`. Both `""` and the undef→`""` mapping have length 0, so downstream (`len < 3` skip) behaves identically. **No byte-identity impact**, but the implementer should write `perl_substr` so that `start == L` yields a zero-length slice (not a panic / not out-of-bounds). Recommend an explicit unit case `perl_substr(s, L, k) == b""` (Important I1) to prevent an off-by-one slice-index panic in Rust.

A second nuance the SPEC handles implicitly but should make explicit: `start` is computed as `L + offset` for negative `offset`; the empty/undef decision is on **`start`**, not on the original `offset`. The SPEC §9 text does compute `start` first then test `start < 0 || start > L`, so this is fine — just confirming it is the right order.

### 1.7 `tr/ACTG/TGAC/` on non-ACTG bytes (Perl :276,281) — CORRECT
Live: `"ACTGN" → "TGACN"` (N unchanged); lowercase `"acgtn"` unchanged. Genome is uppercased on load so only uppercase appears, but `N` is the realistic non-ACGT byte and it passes through. SPEC §8/P3 ("identity on all other bytes incl. `N`") confirmed. **Caveat:** note the translation set is `ACTG`→`TGAC` (i.e. A↔T, C↔G), NOT the alphabetical `ACGT`. The implementer must build the 4-byte map as A→T, C→G, T→A, G→C. A naive `ACGT`→`TGCA` map would also be correct complementation, but to be byte-faithful to *unusual* inputs (it isn't, for ACGTN — same result) just implement the literal A↔T/C↔G. The SPEC writes `tr/ACTG/TGAC/` verbatim, good.

### 1.8 Context classification regexes (Perl :291-303) — CORRECT
- `^CG` → CG (matches `CGx`, including `CGG`, and even `CG` of len 2 — but `len<3` already skipped those, so always 3 bytes here).
- `^C.{1}G$` → CHG (exactly 3 bytes, middle anything, ends G). At byte level `^C.G$` on a 3-byte string. SPEC writes `^C.G$` — equivalent. Note `.` matches any byte except newline; ext_seq has no newlines (chomp'd genome), safe.
- `^C.{2}$` → CHH (3 bytes starting C). SPEC `^C..$` — equivalent.
- else → STDERR warn + `next` (skip). A `tri_nt` not starting with `C` after revcomp (e.g. it begins with `G`/`A`/`T`/`N`) falls here. **Important:** because the regexes are anchored on `^C`, a reverse-complemented `tri_nt` that does **not** start with C (e.g. genomic context where the revcomp first base is `N` → `tri_nt` like `NCG`) is silently skipped via the warn-branch, NOT classified. SPEC §8 says "else STDERR warn + skip" — correct. This is a reachable path with `N`-containing genome and should be tested (Important I2).

### 1.9 NOMe filter conditions (Perl :312-376) — CORRECT
- **CG**: stored call (col-5 context letter) ∈ {`z`,`Z`} AND `upstream ∈ {ACG, TCG}` → tally. Else (other letter) silently disregarded; if call matches but upstream is e.g. `GCG`/`CCG`, `next` (skip). Confirmed `:313,315`.
- **CHG**: stored call ∈ {`x`,`X`} AND `upstream =~ /^GC/` → tally. Confirmed `:338,340`.
- **CHH**: stored call ∈ {`h`,`H`} AND `upstream =~ /^GC/` → tally. Confirmed `:358,360`.
- Tally direction keyed on **column-2 `state` (`+`/`-`)**, NOT the call-letter case (P5). Confirmed `:317,320,342,345,362,365`. The `else { die "This should never happen" }` fires if `state` is neither `+` nor `-`.

**Subtle asymmetry the SPEC under-documents (see Important I3):** For **CG**, a `upstream` that fails the `{ACG,TCG}` test executes `next` (`:329`) — skips the base. For **CHG/CHH**, a `upstream` that fails `/^GC/` has **no `next`** — control simply falls out of the inner `if` and continues the loop naturally (`:340-351`, `:360-371`). The end behavior is the same (base not tallied, loop continues), so **no byte difference**, but the SPEC's "→ tally" phrasing collapses two structurally different Perl branches. If the implementer mirrors the structure they'll be fine; flagging so nobody "simplifies" by adding a spurious early-out that changes a future edge.

### 1.10 Output line + header (Perl :78,389) — CORRECT
- Header bytes verified via `od -c`: `ReadID\tChr\tStart\tEnd\tmeth_CG\tunmeth_CG\tmeth_GC\tunmeth_GC\n` — exactly as SPEC §6. Note columns 7/8 are labelled **`meth_GC`/`unmeth_GC`** in the header but the underlying counters are `$meth_nonCG`/`$unmeth_nonCG`. SPEC §6/§8 correctly notes "Columns 7/8 (meth_GC/unmeth_GC) are the non-CG GpC tallies." Good — don't let the implementer rename the header to `meth_nonCG`.
- Data line: `id\tchr\toffset\tend\t<4 counts>\n` with `offset`/`end` = min/max(start,end) because reverse reads pass `(last_end,last_start)` (`:155,217`). P9 confirmed.
- Counts printed as bare integers (`join "\t"` of Perl scalars) — Rust `u32` Display matches. No `%`-formatting, no float — byte-identical trivially.

### 1.11 Output filename derivation (Perl :464-468, :74-76) — CORRECT
`out = infile`; `s/\.gz$//` (one); `s/\.txt$//` (one); `s/$/.manOwar.txt/`; then at write `unless /\.gz$/ { .= '.gz' }`. So `.manOwar.txt` never ends in `.gz`, so `.gz` is always appended → final `.manOwar.txt.gz`. SPEC §4 examples (`x.txt.gz`→`x.manOwar.txt.gz`; `x.gz`→`x.manOwar.txt.gz`; `x.txt`→`x.manOwar.txt.gz`) all correct. **One edge the SPEC omits:** `x.txt.txt` → strip `.gz`(none) → strip `.txt`(one) → `x.txt.manOwar.txt` → `.gz` → `x.txt.manOwar.txt.gz`. And `x` (no ext) → `x.manOwar.txt.gz`. And `x.gz.gz` → strip one `.gz` → `x.gz` → strip `.txt`(none) → `x.gz.manOwar.txt.gz`. The dedup `filename.rs` precedent uses sequential `strip_suffix` in a loop, which would strip BOTH `.gz`s — **do NOT reuse the dedup multi-ext loop here.** NOMe strips at most ONE `.gz` then at most ONE `.txt`, each independent. Recommend an explicit `x.gz.gz` / `x.txt.txt` unit test (Important I4) to pin the single-strip-per-extension behavior, because the obvious "borrow dedup's loop" instinct gets it wrong.

### 1.12 Per-read grouping & flush (Perl :89-219) — CORRECT, with two implementer traps
- First line sets `last_read/last_start/last_end/last_chr` (`:97-103`); the SAME line also falls through to the `$id eq $last_read` branch and is stored (`:105-109`) because `last_read` was just set to its own id. So the first line's call IS recorded. ✓
- Consecutive same-id lines accumulate `pos→{state,context}`. On id change: flush previous read, then re-init `last_*` to the new line AND store the new line's pos (`:160-167`). The `$read = ()` reset followed by storing the new pos was verified to clear correctly (keys = {new pos} only). ✓
- EOF flush (`:177-219`) is a verbatim copy of the in-loop flush (`:116-168` minus the re-init). SPEC §8's "one shared routine for both flush sites" is **safe and recommended** (avoids the dual-driver divergence risk per the memory). ✓ No off-by-one in the last-read flush: the last read is flushed exactly once after the loop and never inside (because no id-change follows it). ✓
- Empty / all-`^Bismark` input: `last_read` never defined → `die` (`:173-175`) → SPEC's typed `EmptyInput`. ✓ **Note:** the `unless(@ARGV)` help-exit (`:444`) and the empty-FILE die (`:173`) are different paths — the former is "no positional arg", the latter is "file present but yielded zero data lines". The SPEC §4/§5 distinguishes them; good.

**Trap A (consecutive grouping, P10):** Two NON-consecutive blocks of the same ReadID are treated as TWO separate reads (each flushed independently), because grouping is by *consecutive* id. SPEC §5/P10 says exactly this. ✓
**Trap B (same-position-twice within a read):** If a read has two yacht lines at the SAME col-4 position, the later line **overwrites** both `state` and `context` in the map (verified: last-write-wins). The `HashMap<u32,(state,call)>` in SPEC §10 reproduces this naturally (insert overwrites). This is realistic for paired overlap collapsed into a single-end yacht line? Probably not, but it's reachable and free to get right — worth one unit test (Optional O1).

### 1.13 The `chr` vs `last_chr` variable in the guard — CORRECT (no bug)
The in-loop flush at `:132` uses `length($chromosomes{$last_chr})` — the PREVIOUS read's chr — which is correct (it's processing the previous read). The new line's `$chr` (col 3) is only consumed when re-initializing `last_chr` at `:163`. No variable confusion. The SPEC §8 step 2 correctly says `chr_len` is the **last** read's chromosome length. ✓ *(I checked this specifically because mixing `$chr` and `$last_chr` is a classic flush-site bug; Perl gets it right and the SPEC mirrors it.)*

---

## 2. Assumptions (surfaced / validated / flagged)

### 2.1 [Validated, but make explicit] Per-read map key space
The map is keyed by the **yacht column-4 genomic position** (an absolute 1-based chr coordinate), and `cytosine_lookup` looks it up via the derived `pos + offset - 1`. SPEC §5/§8 imply this but never state "the map key IS the genomic coordinate." An implementer could mistakenly key by read-relative `pos`. **Add one sentence to §5/§10 making the key space explicit.**

### 2.2 [Validated] `length(undef)` wording in §7 is slightly off but behavior is right
SPEC §7 says unknown-chr reads are skipped because "`length(undef)==0`". Live test: `length(undef)` actually returns **`undef`** (with `no warnings`), not `0`. But in the numeric comparison `length($chromosomes{$last_chr}) >= (positive)`, `undef` coerces to `0`, so `0 >= positive` is FALSE → guard fails → skip. **Outcome is exactly as the SPEC claims (silent skip, no line); the wording is imprecise.** Recommend rewording to "`length($chromosomes{$last_chr})` on an absent key yields undef→0 in the numeric guard, so the second clause fails and the read is skipped." (Optional O2.) No code impact — in Rust, `genome.get(name)` returns `None` → treat length as 0 → guard fails.

### 2.3 [Flag] Promoted `bismark_io::genome` needs NEW `BismarkIoError` variants — not just a new module
SPEC §3 D1 / §7 frame the promotion as "a new module, additive, no version bump." Verified the c2c `genome.rs` lives in the **c2c crate** (not bismark-io) and raises `BismarkC2cError::{NoGenomeFasta, MalformedFastaHeader, DuplicateChromosomeName, ChromosomeTooLong}`. A `bismark_io::genome` module must surface equivalents as **new `BismarkIoError` variants** (or a module-local error). SPEC §10 says "plus genome errors surfaced from `bismark_io::genome`" — so it assumes these exist, but §7/D1 never states that **bismark-io's public error enum gains variants**. Adding enum variants to a `#[non_exhaustive]`-free public enum is *technically* a minor-version-worthy change in semver terms, though it does not break the `=beta.8` pins (additive). **Confirm bismark-io's `BismarkIoError` is `#[non_exhaustive]` or that adding variants is acceptable without a bump.** I could not see the enum's attributes in the head I read; the implementer must check `error.rs` line 14-17 region. (Important I5.)

### 2.4 [Validated] Sibling `=1.0.0-beta.8` exact pins → no-bump reasoning is SOUND
Confirmed dedup/extractor/methylation-consistency all pin `bismark-io = { version = "=1.0.0-beta.8" }`. The `=` makes ANY bump (even patch) break them. bedgraph has no bismark-io dep; c2c reads FASTA via its own module (mirrors bismark-io pins for noodles but doesn't depend on bismark-io for genome). So the "additive module, no version bump" decision (D1/P7) is correct, and a Phase-A `cargo build --workspace` check is the right guard. ✓

### 2.5 [Validated] Two-plain-suffix glob `[".fa",".fasta"]` is CORRECT for NOMe
Perl `:522` `<*.fa>` then `:526` fallback `<*.fasta>` — no `.gz` tiers. Live glob test confirmed `<*.fa>` matches `genome.fa` only (NOT `weird.fa.gz`, NOT `other.fasta`). SPEC §7/P6 is right that NOMe diverges from c2c's 4-tier reader and must NOT accept gzipped FASTA. The c2c `ends_with(".fa")` predicate (with dotfile exclusion) reproduces this. **One sub-point:** the c2c reader picks "first tier with ≥1 file." For NOMe, tier order `[".fa", ".fasta"]` reproduces Perl's "`.fa` first, fall back to `.fasta`". If a dir has BOTH `x.fa` and `y.fasta`, Perl reads ONLY `x.fa` (the `.fa` tier wins, no union). The promoted `load(folder, &[".fa",".fasta"])` with first-non-empty-tier semantics matches. ✓ (Worth a unit test: `.fa` present beats `.fasta` — Optional O3.)

### 2.6 [Flag] Mus skip + uppercase + `\r` strip + dup-name + first-token — all inherited correctly
The c2c `genome.rs` already implements: Mus skip inside the loop (`mus_only_tier_yields_empty_genome_no_error` test), uppercase-on-load, `\r`-strip (`crlf_sequence_has_no_carriage_return` test), first-whitespace-token name via noodles `record.name()` (`loads_multifasta_first_token_name` test), duplicate-name error (`duplicate_name_cross_file_errors` test). Promoting a tier-parameterized variant preserves all of these. **BUT one divergence the SPEC should call out:** the c2c reader **errors** on a bare/nameless `>` header (`bare_or_nameless_header_errors` test), whereas Perl `extract_chromosome_name` would store an empty-name chromosome. The c2c module doc admits this "cannot occur on a Bowtie2-built genome." For NOMe byte-identity this is the same accepted divergence — fine, but the SPEC §7 should explicitly inherit that documented divergence so the real-data gate doesn't surprise anyone. (Optional O4.)

### 2.7 [Validated] CLI inventory (§4) vs Perl `process_commandline`
- `$nome = 1` default (`:403`) and the GetOptions entry `'nome-seq' => \$nome` (`:415`) means passing `--nome-seq` sets it to 1 (already 1) — it is effectively **non-negatable**, so NOMe filtering is **unconditional**. SPEC §4 correct. *(Pedantic: `--nome-seq` is a boolean GetOptions flag; passing it just re-sets 1. There is no `--no-nome-seq`. So "inert" is right — it can never turn filtering off.)*
- Mandatory genome: `die` if `!$genome_folder` (`:493-494`). ✓
- Infile: `unless(@ARGV)` → help + `exit` (0) (`:444-450`); `unless(-e $infile)` → die (`:453-454`). SPEC §4 distinguishes "no file → help+exit" vs "non-existent → die." ✓
- The only reachable die in the option block is `--merge_CpGs` + `--CX` (`:497-500`). The `--split_by_chromosome` die (`:501-503`) is **unreachable**: `$split_by_chromosome` has NO GetOptions entry, so it is always undef/false. SPEC §4/§55 correct — document, don't implement. ✓ *(Confirmed: grep of GetOptions block `:405-416` has no `split_by_chromosome` key; the variable is declared at `:28` in the big `my (...)` list but never assigned.)*
- **Hidden behavioral effects of "inert" flags — checked, found NONE that reach output:**
  - `--CX` sets `$CX_context=1` and skips the `$CpG_only=1` default (`:482-485`). `$CpG_only`/`$CX_context` are returned but **never consumed** in `per_read_filtering` or `cytosine_lookup`. So `--CX` alone has no output effect (only matters via the `--merge_CpGs`+`--CX` die). ✓ SPEC's "accept-and-ignore" holds.
  - `--GC`/`$gc_context`: `:506-511` auto-sets `$gc_context=1` under nome and `warn`s, but `$gc_context` is never consumed downstream. Inert. ✓
  - `--zero_based`/`$zero`, `--gzip`/`$gzip`, `--merge_CpGs` (alone): never consumed in the processing path. ✓
  - `--dir`: **LIVE** (chdir at `:58-61`) — correctly listed live in §4.
  - `--parent_dir`: **LIVE-ish** — used only to `chdir` back after reading the genome (`:457-462, :589`). Since the Rust port reads the genome without changing the process CWD (it can read absolute paths), `--parent_dir` has **no observable output effect** in Rust. SPEC §4 lists it as "live" with "base dir restored after reading the genome." **Recommend reclassifying `--parent_dir` as effectively inert for the Rust port** (accept, ignore) — there is no genome-relative output path that depends on CWD. The only CWD-sensitive output is `--dir` (where the output file is written). If the Rust port writes the output to `output_dir.join(name)` (absolute or CWD-relative) without a real `chdir`, `--parent_dir` is a no-op. (Important I6 — make the §4 "live" classification of `--parent_dir` precise, or the implementer may waste effort emulating the Perl `chdir` dance.)

  **Subtle CWD interaction the SPEC must address (Important I7):** Perl's flow is: (1) `read_genome_into_memory` does `chdir $genome_folder` then `chdir $parent_dir` back (`:519,589`); (2) `per_read_filtering` does `chdir $output_dir` (`:58-61`); (3) opens input by **bare filename** (`:66-70`) and output by **bare filename** (`:77`) — both relative to `$output_dir`. So **the input file is opened relative to `--dir`, not relative to the original CWD.** This is because the methylation extractor hands NOMe just a basename + `--dir`. **If the Rust port resolves the input path relative to the original CWD while Perl resolves it relative to `--dir`, they will open different files (or the Rust port will fail to find the file Perl finds, or vice-versa).** The SPEC §4 says `--dir` "chdir into it to write" but does NOT note that the INPUT is also opened relative to `--dir`. For byte-identity of *output location* and for *which input is read*, the Rust port must replicate: input opened at `output_dir/infile`, output written at `output_dir/derived_name`. This is the highest-value CLI subtlety and is currently under-specified. **Critical-adjacent — raised as Critical C2.**

---

## 3. Efficiency

- Whole-genome-in-RAM is accepted (matches Perl) — fine. NOMe processes one read at a time; the per-read map is tiny (≤ read length entries). Memory is dominated by the genome HashMap, same as c2c.
- `flate2::read::MultiGzDecoder` for input and `GzEncoder` + `Compression::default()` for output (SPEC §10) match c2c and produce decompressible output; byte-identity is asserted post-decompression (P8). Correct call — raw gzip-container bytes are NOT guaranteed identical to Perl's `gzip -c` and must not be compared raw.
- `HashMap<u32,(state,call)>` per read: fine. Could use a small `Vec<(u32,...)>` but the map handles the same-position-overwrite quirk (§1.12 Trap B) for free, so keep the map.
- `seq =~ /([CG])/g` is a linear scan; in Rust, iterate bytes and branch on `b'C'`/`b'G'`. O(read length). No concern.
- No performance gate for v1.0 (SPEC §2) — appropriate; the tool is tiny relative to extraction.

No efficiency concerns.

---

## 4. Validation sufficiency

The proposed test surface (§12) is good and covers most unit-level arithmetic. Gaps where the port could **silently emit wrong bytes with no failing test**:

1. **[Critical] The two edge asymmetry lines must BOTH be golden-tested end-to-end**, not just unit-tested:
   - reverse read with `last_end ∈ {1,2}` → the **all-zero line** `id chr end start 0 0 0 0` IS emitted.
   - forward read with `start ≤ 3` → **NO line**.
   SPEC §12 mentions both in the integration-golden fixture list — good — but make them **separate, named golden cases** so a regression in either direction is unambiguous. (C1)

2. **[Critical] CWD/`--dir` input+output resolution** (see §2.7 I7 / C2): the golden harness must run the Rust binary with `--dir <outdir>` and a **bare-filename input that lives in `<outdir>`**, exactly as the methylation extractor invokes NOMe, and confirm the Rust port reads it from there and writes there. If the test always passes absolute paths, the CWD divergence ships untested.

3. **[Important] `N`-containing genome contexts** (§1.8): a `tri_nt` that after revcomp does NOT start with `C` (e.g. genomic `N` adjacent to a G call) hits the warn-and-skip branch (`:300-302`). And `CNG`→CHG, `CNN`→CHH classification (SPEC §12 lists these). Add a fixture read overlapping an `N` run so both the classify-as-CHH/CHG path AND the unclassifiable-skip path are exercised. (I2)

4. **[Important] Single-strip-per-extension filename** (§1.11): unit-test `x.gz.gz` → `x.gz.manOwar.txt.gz` and `x.txt.txt` → `x.txt.manOwar.txt.gz` to guard against accidentally reusing dedup's multi-strip loop. (I4)

5. **[Important] CRLF yacht input.** The genome reader strips `\r` (tested in c2c), but does the **yacht input parser** strip `\r`? Perl `per_read_filtering` does `chomp` (`:90`) which removes `\n` but NOT a preceding `\r` on a CRLF file — so on CRLF yacht input, the 8th field `$strand` would retain a trailing `\r`. **But the Rust port may use `lines()` which strips `\r\n` differently than Perl `chomp`.** Since `$strand`/col-8 is parsed but **never used** in the processing path (only cols 1-7 matter: id,state,chr,pos,context,start,end), a trailing `\r` on col-8 is harmless to output. **However**, if a yacht file is CRLF AND the `^Bismark` skip line has a `\r`, Perl's `/^Bismark/` still matches (anchored at start). No output impact. Verdict: low risk, but add ONE CRLF-yacht-input golden to confirm the Rust line reader doesn't mangle col-2..7. (I8)

6. **[Optional] Same-position-twice within a read** (§1.12 Trap B): last-write-wins. (O1)

7. **[Optional] `.fa` tier beats `.fasta` tier** when both present (§2.5): no-union semantics. (O3)

8. **[Optional] A CpG straddling the read's 2bp pad boundary.** The reviewer-brief asks about a CpG at the very edge where the trinucleotide needs the +2 pad. With a full ext_seq (forward reads, guard passed → ext_seq always has length+4 bytes), the last seq base's `tri_nt = substr(ext_seq, pos+1, 3)` reads up to `ext_seq[pos+1..pos+4]`; for the LAST seq position `pos = length`, that's `ext_seq[length+1..length+4]` = the 2 trailing pad bytes + ... wait, ext_seq has exactly `length+4` bytes (indices 0..length+3), so `substr(ext_seq, length+1, 3)` reads indices `length+1, length+2, length+3` = exactly the last 3 bytes. So the final C in a forward read CAN be classified iff the genome provides those pad bytes — which the guard guarantees for forward reads. For the FIRST seq base `pos=1` (`C` at seq idx 0), `tri_nt = substr(ext_seq, 2, 3)`. Fine. **No off-by-one at the pad boundary for forward reads.** For reverse reads at the chr-START the pad is missing (the all-zero case, already covered). A fixture with a **CpG as the literal last base of a forward read** would pin the upper-pad boundary. (O5)

9. **[Optional] Non-ACGTN genome bytes** (e.g. lowercase already uppercased away; IUPAC `R`/`Y`): `tr/ACTG/TGAC/` leaves them unchanged → context regex `^C..$` etc. may or may not match → warn-skip. Realistically Bismark genomes are ACGTN only; low priority. (O6)

**Real-data gate (Phase C):** mirroring c2c's RELEASE_CHECKLIST on colossal, `cmp` of decompressed outputs, `LC_ALL=C` — sound. One caveat: NOMe output order = **input read order** (not sorted), so NO sort step is needed and none should be introduced (a sort would diverge). The SPEC §12 says "`LC_ALL=C` for any sort step" — there should be **no sort step** for the NOMe output itself; only apply it if the *upstream yacht generation* needs deterministic ordering. **Clarify that the NOMe output is compared in emission order, un-sorted.** (Important I9.)

---

## 5. Alternatives

1. **`perl_substr` return type.** SPEC §9 offers `&[u8]` / `Cow` / `Vec<u8>`. Prefer **`&[u8]`** (a sub-slice of `ext_seq`) for the forward/positive-offset path; the revcomp path must allocate anyway (reverse+complement). A borrowed slice avoids per-call allocation in the hot loop. Minor.

2. **Iterate bytes vs regex.** Implement the `/([CG])/g` walk as a plain byte loop (`for (i, b) in seq.iter().enumerate()` matching `b'C' | b'G'`). No regex crate needed; faster and clearer. The `pos = i+1` mapping (§1.1) makes this a direct translation.

3. **One shared flush routine** (SPEC §8) — strongly endorse. The Perl duplicate (`:116-168` vs `:177-219`) is exactly the dual-driver hazard from the memory. A single `fn flush_read(...)` called from both the id-change branch and post-loop is the right move and reduces the test burden.

4. **Module-local error vs new `BismarkIoError` variants** (§2.3): an alternative to widening `bismark-io`'s public error enum is a `genome`-module-local `GenomeError` that the NOMe crate maps into `BismarkNomeError`. This keeps `BismarkIoError` untouched (cleanest for the "no API churn" goal) and is worth considering vs. adding variants. Either works; the module-local error is the more conservative additive choice.

---

## 6. Action items (prioritized)

### Critical (correctness / silent-wrong-bytes risk — resolve before implement)
- **C1 — Golden-test BOTH edge-asymmetry outcomes as named cases.** reverse `last_end∈{1,2}` → all-zero line emitted (`id chr offset end 0 0 0 0`); forward `start≤3` → no line. Both fully verified against Perl in this review (§1.5). They are the most counter-intuitive behaviors and a regression in either direction is silent. SPEC §12 lists them in the fixture; elevate to explicit independent golden assertions.
- **C2 — Pin the `--dir` input/output CWD semantics (§2.7 I7).** Perl opens the INPUT by bare filename **relative to `--dir`** (after `chdir $output_dir`), and writes the output relative to `--dir` too. The SPEC §4 only documents the output side. Specify exactly how the Rust port resolves the input path (recommend: `output_dir.join(infile)` for the read, `output_dir.join(derived_name)` for the write, WITHOUT a real process `chdir`), and add a golden that invokes the binary the way the extractor does (bare-filename input inside `--dir`). Otherwise the port may read/write the wrong location with no failing test.

### Important (clarity / reachable behavior / test coverage)
- **I1 — `perl_substr` `start == L` → empty slice (no panic).** Add the explicit boundary unit case; ensure Rust slicing at `start == len` yields `&[]` not an out-of-range panic (§1.6).
- **I2 — `N`-genome context test.** Fixture exercising `CNG`→CHG, `CNN`→CHH, AND a revcomp `tri_nt` not starting with `C` → warn-skip branch (§1.8).
- **I3 — Document the CG-vs-CHG/CHH `next` structural difference** (§1.9): CG-upstream-fail does `next`; CHG/CHH-upstream-fail just falls through. Same output, different structure — note it so a "simplification" doesn't introduce a divergence.
- **I4 — Filename single-strip unit tests** (`x.gz.gz`, `x.txt.txt`) and an explicit warning in §10 NOT to reuse dedup's multi-ext `strip_suffix` loop (§1.11).
- **I5 — Confirm bismark-io error-enum extension policy** (§2.3): decide module-local `GenomeError` vs new `BismarkIoError` variants; check whether the enum is `#[non_exhaustive]`. State the choice in §7/§10.
- **I6 — Reclassify `--parent_dir`** in §4 as effectively inert for the Rust port (no observable output effect once the genome is read by absolute/explicit path; no real `chdir` needed) (§2.7).
- **I7 — (folded into C2)** input-relative-to-`--dir` resolution.
- **I8 — CRLF yacht-input golden** (low risk but cheap; confirms the Rust line reader doesn't corrupt cols 2-7) (§4 / §1.12).
- **I9 — State the NOMe output is compared in emission (input read) order, un-sorted** (§4 validation); ensure no sort step is applied to the NOMe output in the gate.

### Optional (nice-to-have)
- **O1 — Same-position-twice-within-a-read** unit test (last-write-wins) (§1.12 Trap B).
- **O2 — Reword §7 `length(undef)==0`** to "undef→0 in the numeric guard" (§2.2).
- **O3 — `.fa` tier beats `.fasta` tier** unit test (no-union) (§2.5).
- **O4 — Inherit the documented bare-`>`-header divergence** from c2c's genome reader explicitly in §7 (§2.6).
- **O5 — CpG as the literal last base of a forward read** golden (upper-pad boundary) (§4 #8).
- **O6 — Non-ACGTN/IUPAC genome byte** behavior note (warn-skip), low priority (§4 #9).

---

### Verdict
The SPEC is **ready to proceed to implementation after resolving C1 and C2** (both are specification/test gaps, not transcription errors) and folding the Important items. The byte-identity arithmetic transcription is correct in every case I independently re-derived. The two Criticals are about *making behaviors testable / fully specifying the CWD contract*, not about wrong math.
