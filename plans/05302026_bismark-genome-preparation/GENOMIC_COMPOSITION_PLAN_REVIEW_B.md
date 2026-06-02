# GENOMIC_COMPOSITION_PLAN — Plan Review B

**Reviewer:** B (independent, fresh context)
**Plan:** `GENOMIC_COMPOSITION_PLAN.md` (rev 0, 2026-05-31)
**Perl source of truth:** `bismark_genome_preparation` — `get_genomic_frequencies` (518–543), `process_sequence` (546–570), `extract_chromosome_name` (572–582), `read_genome_into_memory` (665–751), flow (184–194).
**Verdict:** Sound core algorithm; the mono/di/N/sort logic is faithful and I verified it empirically. But there are **2 real divergences in the header / first-line handling** and an **error-path side-effect divergence** that the plan currently under-specifies or under-states. Acceptance gate (byte-identical `genomic_nucleotide_frequencies.txt`) is achievable with the fixes below.

Counts: **Critical 1 · Important 3 · Optional 4**

---

## What I verified empirically (Perl oracle)

All run against system `perl` on the actual snippets.

1. **`process_sequence` output for `chr1="ACGTNRACG"`, `chr2="GGCC"` (counted separately):**
   ```
   A 2 / AC 2 / C 4 / CC 1 / CG 2 / G 4 / GC 1 / GG 1 / GT 1 / R 1 / RA 1 / T 1
   ```
   Confirms: mono skips only `N`; ambiguity `R` is counted as mono **and** in di (`RA`); di skips any 2-mer containing `N`; di does **not** span the chr boundary (no `G`+`G` di joining `…ACG` to `GGCC`). The plan's `prev`-carry with reset-at-header reproduces this. ✔
2. **Sort order:** Perl default `sort` on keys `{ , \t, 9, 9A, A, AA, AB, AC, AT, C, R, RA, Z, a}` is **identical** to `LC_ALL=C sort` (byte/`cmp`). So `BTreeMap<Vec<u8>>` iteration matches Perl. Digits < uppercase < lowercase; mono `A`(0x41) sorts before its di `AA` (prefix-first). ✔ (Plan §2/§3 correct.)
3. **di-N skip equivalence:** `index($di,'N') < 0` ⇔ plan's `p != N && u != N` for the 2-byte key. ✔
4. **Empty `%genomic_freqs` → a 0-byte file IS created** (the `open` succeeds, the `foreach` writes nothing). Plan Decision #3's recommendation (write an empty file, not "no file") is **correct**. ✔
5. **`chomp` + `s/\r//`** removes the trailing `\n` then the **first** `\r` anywhere; an interior-only `\r` (`AC\rGT\n`) becomes `ACGT`, and a doubled `\r` (`AC\r\rGT\n`) leaves a literal `\r` in the sequence (→ a `\r` mono key + `C\r`/`\rG` di). ✔ (Plan acknowledges this as pathological.)
6. **`uc` on Latin-1 high bytes** (`\xe9\xff\xc0\x80a`) in this Perl build leaves high bytes untouched and folds only ASCII `a→A` — i.e. **byte-equivalent to Rust `to_ascii_uppercase`**, matching the already-gated `convert.rs`. No divergence. ✔
7. **Dup-name ordering** (see Critical C1 / Important I1).

---

## Logic review

### C1 (Critical) — The first line of each file is UNCONDITIONALLY a header in Perl; the plan's "starts with `>`" rule diverges on a non-header first line.

`read_genome_into_memory` reads the first line with a bare `my $first_line = <CHR_IN>;` (line 703), `chomp`s + `s/\r//`s it, and passes it **straight to `extract_chromosome_name`** (line 708) — with **no `/^>/` test**. `extract_chromosome_name` (575) `die`s if the line does not start with `>`. Consequences the plan does not capture:

- **The first line's bytes are NEVER counted**, even if it doesn't start with `>`. The plan's §2 rule ("if the line starts with `>`: header (also handles the first line); else: sequence line") means that **if a file's first line is NOT a header, the Rust code would count those bytes as sequence**, whereas Perl `die`s ("not in FASTA format"). Verified: `extract_chromosome_name("ACGT")` → `die`.
- This is the **same root cause** as a file that starts with a blank line: Perl consumes the blank first line as a (failed) header and `die`s; the plan would treat it as an (empty) sequence line and proceed.

**Why it matters:** silent wrong output (extra counts) where Perl aborts. The existing `convert.rs` already handles this correctly — `convert_split` reads the first line, then `handle_header` calls `extract_chromosome_name` which returns `Err(NotFasta)` on a non-`>` first line (and `empty_file_errors`/`NotFasta` are tested). The composition path **must mirror that structure**: read the first line, require it to be a header (else `NotFasta`), and only `/^>/`-test **subsequent** lines.

**Fix:** Rewrite §2's loop to match Perl/`convert.rs`: (a) read the first line; if `read_until` returns 0 → `NotFasta` (empty file, as Perl's undef first line); call `extract_chromosome_name` on it (propagating `NotFasta` if it doesn't start with `>`) and set `prev = None`; (b) then loop the remaining lines with the `/^>/` test. Do **not** rely on a generic "first line counts as header because it starts with `>`" — a malformed first line must error, not be counted.

---

### I1 (Important) — Dup-name error path: Perl writes NO table; the plan writes the table then dies in conversion. (Open Decision #2 understated.)

Flow (184–194): `get_genomic_frequencies()` runs **before** `process_sequence_files()`. Inside it, `read_genome_into_memory` (522) `die`s on a duplicate chromosome name (716–719 / 737–740) **before** the table-write block (532). I verified: on a dup-name genome Perl leaves **no** `genomic_nucleotide_frequencies.txt`.

The plan's stream-counter does **not** key by chromosome name, so it would **not** detect the dup → it **writes the (complete) table**, and only then does the later conversion step (`convert_split`, which *does* dup-check via `seen`) error out. Net divergence:

| genome with dup chr name | Perl v0.25.1 | Rust plan as written |
|---|---|---|
| `genomic_nucleotide_frequencies.txt` | **not created** | **created (full table)** |
| exit | dies in freq step | dies in conversion step |

Decision #2's text ("both error out before/around the table") is wrong on the side-effect: Perl errors **before** the table exists; Rust leaves an orphan table on disk on a failed run. The byte-identity gate only compares on success, so this is not a gate failure — but it's a real, observable divergence (a stale artifact from a failed invocation) and the kind of thing a reviewer should not wave away.

**Fix (pick one, document it):**
- **(preferred, faithful + cheap):** have the composition pass do its **own** dup-name check keyed by `extract_chromosome_name` (reuse a `HashSet<Vec<u8>>` exactly like `convert.rs`'s `seen`) and return `DuplicateChromosome` **before** opening/writing the table. This reproduces Perl's "die before any table is written" precisely and is trivial since the code already extracts the name at each header. This **supersedes** Decision #2's "rely on the conversion's dup-check."
- **(weaker):** keep relying on the conversion, but then explicitly document that the Rust freq path leaves an orphan table on a dup-name genome where Perl leaves none — and confirm that is acceptable. Note this still doesn't reproduce Perl's behavior of dying *in the freq step*.

---

### I2 (Important) — `prev` must reset at file boundaries; plan must declare `prev` scope.

Perl calls `process_sequence` **once per chromosome** (523–526), so di-mers never span chromosomes **or files**. The plan's reset is "`prev = None` at every `>` header." That is correct **only because** every file's first line is a header (per C1). The plan never states whether `prev` is declared inside or outside the per-file loop. If declared **outside** and combined with the (incorrect) "first line counts as header iff it starts with `>`" rule, then a second file whose first byte is sequence would carry `prev` across the file boundary — a cross-file di-mer Perl never produces.

**Fix:** Once C1 is fixed (first line of each file is forced to be a header → resets `prev`), this is automatically correct. To be safe and unambiguous, **declare `prev` inside the per-file loop** (initialized to `None` per file) and additionally set it `None` at every header. State this explicitly in §2.

---

### I3 (Important) — Tests do not cover the C1/I1 error paths or the multi-file/file-boundary di reset.

§4's unit tests cover ACGT-only, N-skip, ambiguity, line-boundary di, two-records-in-one-file (no cross-record di), and sort order — good. **Missing:**
- A file whose **first line is not a `>` header** (and an **empty file**) → must error (`NotFasta`), not count bytes (C1).
- A **dup chromosome name** genome → must error **and not leave a `genomic_nucleotide_frequencies.txt`** behind (I1).
- A **multi-file** genome (two `.fa` files) → di-mers must **not span the file boundary** (I2). The current "two records" test only covers two records in one file.
- The **empty/N-only genome → 0-byte file** assertion (Decision #3): assert the file exists and is exactly 0 bytes.

Add these to §4. The integration/Perl-oracle case should include a multi-file genome and at least one ambiguity code + one N to exercise the interesting paths byte-for-byte.

---

## Assumptions

- **Reuse of `convert_split`'s `files` slice** (same glob precedence/order as Perl's independent re-glob in `read_genome_into_memory` at 671–685): valid — both use identical precedence and `discovery::find_fasta_files` is the single source. ✔ But note the **`Mus_musculus.NCBIM37.fa` skip is composition-only** (Perl line 694); the conversion does *not* skip it. The plan correctly skips it in the composition loop only. Confirm the skip is an **exact, case-sensitive byte match on `file_name()`** (`name == b"Mus_musculus.NCBIM37.fa"`), matching Perl's `eq`, and works for non-UTF-8 names (use `as_encoded_bytes()`/`OsStr` bytes, consistent with `discovery.rs`'s byte-matching M1 fix). (Optional O1.)
- **Output path** `genome_folder.join("genomic_nucleotide_frequencies.txt")`: equals Perl's `"${genome_folder}genomic_nucleotide_frequencies.txt"` because Perl forces a trailing slash on `$genome_folder` (lines 93–94) and Rust's config canonicalizes the folder (cli.rs:191) then `join`s. ✔
- **Non-fatal write (Decision #1):** Perl `warn`s and skips on `open` failure (540–542). The plan's "log a warning and return `Ok(())`" matches. ✔ But see O2 — the plan only mentions the **open** failure; Perl also `warn`s (not dies) on **close** failure (538) and does **not** check per-`print` write errors. Decide whether a mid-write I/O error is non-fatal too (it should be, to match Perl's `print`-without-error-check). The current `error.rs` makes all I/O `?`-propagating; the composition writer must deliberately swallow write errors to be faithful — call this out so the implementer doesn't `?`-propagate a write error into a hard exit.

---

## Efficiency

- `BTreeMap<Vec<u8>, u64>` with 1–2 byte keys: fine. Each key allocates a tiny `Vec` — for whole-human-genome di-counting the number of **distinct** keys is ~bounded (≤ ~26 mono + ~26² di + stray bytes), so the map is tiny; the per-2-mer `entry(vec![p,u])` allocates a `Vec` on **insert only** (BTreeMap reuses the key thereafter), so it's not per-base allocation for existing keys — **but** `entry(vec![…])` constructs the `Vec` on **every** call to compute the lookup key. For a 3 Gbp genome that's ~3e9 transient 2-byte `Vec` allocations. **Optimization (O3):** key on `[u8; 2]` / a small inline type, or use a fixed `u64`/`u16`-indexed array for the 256×256 di + 256 mono space and emit sorted at the end — avoids per-base heap traffic. Perl is the slow baseline ("may take several minutes"), so even the naive version beats it, but the array approach is both faster and trivially byte-sortable. Not required for correctness.
- Streaming (not slurping) is **better** than Perl, which slurps the whole genome into `%chromosomes` (SPEC §8 #12 flags this as Perl's memory cost). Plan's streaming avoids it. ✔ Worth stating as an accepted, beneficial divergence.

---

## Alternatives

- **Array-indexed counters** (O3 above) instead of `BTreeMap<Vec<u8>>` — faster, still byte-sortable on emit. Trade-off: must iterate 256/256² slots and skip zero-count keys; mono/di interleave on output is reconstructed by merging two sorted streams, which is more code than `BTreeMap`'s free ordering. Given the map is tiny, `BTreeMap` keyed by `[u8;2]`/`SmallVec` is the sweet spot.
- **Reuse `convert.rs::open_fasta`** rather than a new opener — the plan says "reuse the gz-aware opener," but `open_fasta` is currently **private** to `convert.rs`. Either make it `pub(crate)` or factor it into a shared module; don't duplicate the gz-detection logic (it must stay identical to the conversion path). (Optional O4.)

---

## Action items

### Critical
- **C1** — Fix §2's loop so the **first line of each file is unconditionally treated as a header** (require `>` via `extract_chromosome_name`, else `NotFasta`; empty file → `NotFasta`), mirroring `convert_split`/`read_genome_into_memory` (Perl 703–708). Only `/^>/`-test **subsequent** lines. As written, a non-header first line is silently counted instead of erroring.

### Important
- **I1** — Resolve Open Decision #2 in favor of an **own dup-name check in the freq pass that errors before writing the table** (Perl dies before the table exists; verified). Correct the decision text: Perl writes **no** table on a dup-name genome; the plan as written leaves an orphan table.
- **I2** — Declare `prev` **per-file** (reset `None` at each file start and each header); state it explicitly in §2.
- **I3** — Add tests: non-header/empty first line → error; dup-name → error + no table file left; **multi-file** genome di-reset; empty/N-only → exactly 0-byte file.

### Optional
- **O1** — Specify the `Mus_musculus.NCBIM37.fa` skip as an exact case-sensitive **byte** match on `file_name()` (non-UTF-8-safe), consistent with `discovery.rs`.
- **O2** — Make the non-fatal contract explicit for **write/close** errors (not just `open`): swallow per-write I/O errors + warn (Perl never checks `print` errors and only `warn`s on `close`). Ensure the writer doesn't `?`-propagate.
- **O3** — Consider array-indexed counters (or `BTreeMap<[u8;2], …>`) to avoid ~per-base `vec![p,u]` allocations on a 3 Gbp genome.
- **O4** — Reuse `convert.rs::open_fasta` (make it `pub(crate)`) instead of duplicating gz-detection; add a test asserting `.fa.gz` input counts identically to plain `.fa`.

---

## Summary
The mono/di/N/sort algorithm is correct and empirically matches Perl, and the non-fatal-write + empty-table decisions are right. The gaps are all in **header/first-line/error-path fidelity**: (C1) the first line must be forced to be a header and error otherwise — as `convert.rs` already does — not opportunistically counted; (I1) a dup-name genome must not leave a table file behind; (I2/I3) `prev` reset scope and the missing error-path/multi-file tests. Fix C1 and I1 and the byte-identity gate is reachable.
