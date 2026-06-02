# Code Review A — Phase 5: Genomic-seq + XM/XR/XG + SAM/BAM output (SE directional)

**Reviewer:** A (independent, fresh context)
**Scope:** `genome.rs`, `methylation.rs`, `output.rs`, `lib.rs`, `config.rs`, `merge.rs`, `tests/cli.rs`
**Oracle:** Perl `bismark` v0.25.1 (`/Users/fkrueger/Github/Bismark-aligner/bismark`)
**Verdict:** **APPROVE.** This is an exceptionally faithful, line-traceable port. I traced the highest-risk machinery — `make_mismatch_string` / `rebuild_md_with_deletions` (the multi-deletion re-indexing, including the trailing "last element was a digit" arm), both complement helpers and their double-application, the per-strand counters behind the edge guards, the FLAG / +2-trim / NM / tag-order logic, and the genome loader's `@SQ`-order semantics — and found **no Critical or High defects** affecting the byte-identity gate. Findings below are Medium/Low (latent divergences unreachable given the 3127 length guard, plus diagnostic-stream and robustness nits).

---

## Summary by area

### Logic — the deletion machinery (`make_mismatch_string` + `rebuild_md_with_deletions`)
This is the load-bearing risk and it is ported variable-for-variable against Perl 9252–9595. I hand-traced three cases against the algorithm:

- **Single deletion** `2M1D2M`, md_seq `ACTGT` → `MD:Z:2^T2`. ✓
- **Two deletions** `2M1D2M1D2M`, md_seq `ACTGTAAC` → `MD:Z:2^T2^A2` (exercises the cross-deletion re-indexing via `md_index_already_processed`, the `current_md_index` skip-ahead at Perl 9559/9564, and the second deletion processed in the trailing arm). ✓
- **Deletion adjacent to a mismatch** `2M1D2M`, actual `ACTT` / ref `ACGT` / md_seq `ACNGT` → `MD:Z:2^N0G1` (in-loop arm at Perl 9448–9479, then the deletions-done passthrough). ✓

Key correctness points verified against Perl:
- The five guard arms inside the `@md` loop are in the exact Perl order (Rust `output.rs:204–228` vs Perl 9395–9425): index-increment → `md_index_already_processed` skip → `op` init → `deletions_processed == total` → `this_deletion_processed`. ✓
- In-loop non-final-deletion re-indexing uses `md_index_already_processed = current_md_index − 1` (Perl 9489); the **trailing** arm uses `= current_md_index` with **no** `−1` (Perl 9563–9564, "not in the loop, hasn't been incremented"). Both reproduced exactly (`output.rs:266` vs `:310`), and the comment at `:309` documents the difference. ✓
- `del_pos`/`md_pos_so_far` accrual: M adds to `del_pos`; I/S add to **both** `del_pos` and `md_pos_so_far`; N is ignored; the deletion length is added to `del_pos` (always) and to `md_pos_so_far` (only for **non-final** deletions). Matches Perl 9359–9376 + 9477/9494–9495 + 9568/9572. ✓
- `deleted_bases = md_substr(md_sequence, del_pos, len)` with Perl-lenient out-of-range = empty (`md_substr` clamps start/end to `md_sequence.len()`, `output.rs:155–162`). ✓

### Logic — extraction (`extract_corresponding_genomic_sequence_single_end`)
- `pos = position − 1`; CIGAR walk M/I/D/S/N with the illegal-op error; **`indels` accrues for `D` only** (`methylation.rs:187`), I/S/N do not (Perl 4346/4360/4376). ✓
- +2 prepend (eff∈{1,3}) goes to `unmodified_genomic_sequence` **only**, never `genomic_seq_for_md_tag` (`:159–160`, Perl 4322). ✓ The +2 append (eff∈{0,2}) likewise (`:206`). ✓
- Edge guards: prepend guard `pos < 2` (Perl `($pos-2) >= 0`), append guard `chr.len() < pos+2` (Perl `length >= pos+2`); both return the partial sequence with `extracted:false` and **no** per-strand counter bump (`:155–161`, `:202–207`). ✓
- Per-strand counters bumped **after** both guards (`:210–216`, behind Perl 4317/4390). ✓
- revcomp for eff∈{1,2} only (`:219`, Perl 4417/4432); eff 3 prepends but does **not** revcomp. ✓
- Strand/conversion table (`:131–141`) matches Perl 4400–4445 exactly.

### Logic — `methylation_call`
- CT branch (Perl 4832–4912) and GA branch (4913–4998) both ported; GA compares `seq[i]` to `genomic[i+2]`, with upstream context `genomic[i+1]` and second-upstream `genomic[i]` (`:276–289`), matching Perl 4917/4923/4935. ✓
- `U`/`u` triggered by context base `N` **or** `X` (`:312`, `:318`, Perl 4844/4856). ✓
- Out-of-range context reads return a `0` sentinel (`:254`) — neither `G`/`C` nor `N`/`X`, so they fall through to CHH/`.`, replicating Perl's `undef`-from-out-of-range `@genomic` access (Perl emits an "uninitialized" warning but the comparison is false; behaviour is identical). ✓ The 4822 length `warn` is correctly **not** ported as a panic.
- 8 context counters accumulate via `bump()` keyed on the call char (`:360–372`, Perl 5006–5013). ✓

### Logic — SAM record (`single_end_sam_output`) + header (`generate_sam_header`)
- FLAG table (`:349–362`) matches Perl 8521–8546. ✓
- +2 trim: CT drops the **last** 2 (`g[..len−2]`), GA drops the **first** 2 (`g[2..]`) — Perl 8570–8575. ✓
- Minus-strand reorientation (`:379–386`): `revcomp(actual)`, `revcomp(ref)`, conditional `revcomp(md_seq)` for `D`-CIGARs, `scores.reverse()` (Perl 8577–8584). ✓
- **Double revcomp of `genomic_seq_for_md_tag`** for index-1 (`-`-strand) + deletion: `methylation::reverse_complement` in extraction (`methylation.rs:222`) **and** `output::revcomp` here (`output.rs:383`). I verified the net is identity on upper-case `ACGTN` and on `N`/`X` (both helpers leave those bytes unchanged) — and both are applied verbatim rather than collapsed. ✓
- `NM = hemming_dist(actual, ref) + indels` (`:389`, Perl 8588–8590). ✓
- Tag insertion order `NM, MD, XM, XR, XG` (`:420–433`, Perl 8706); the round-trip test asserts BAM-decoded key order. ✓ `XM` reversed for `-` strand (`:399–403`, Perl 8602–8607).
- The two complement helpers are correctly **distinct**: `reverse_complement` (5161, `tr/CATG/GTAC/`, upper-case only) vs `revcomp` (9228, `tr/ACTGactg/TGACTGAC/`, both cases) — `methylation.rs:60` vs `output.rs:106`. ✓
- `@HD VN:1.0 SO:unsorted`, `@SQ … LN` in `sq_order`, `@PG ID:Bismark VN:v0.25.1 CL:"bismark <argv>"` with **literal embedded quotes** — pinned by `header_hd_sq_pg_exact_bytes` (`tests` at `output.rs:739–749`). VN-before-CL via `other_fields` insertion order (the §13 deviation note correctly supersedes the plan's "typed version field" for noodles 0.85). ✓
- `BISMARK_VERSION = "v0.25.1"` matches Perl `$bismark_version` (line 28); `command_line = argv[1..].join(" ")` matches Perl `join(" ", @ARGV)` (line 32). ✓

### Logic — genome loader & driver wiring
- `read_genome_into_memory` consumes Phase-1's ordered `config.genome.fastas` (no re-glob); `sq_order` = encounter order; dup-name die, empty-seq warn, empty-name die in the loader, `chomp` + single-`\r` strip, `uc`. Matches Perl 5022–5147. ✓
- Driver order (`lib.rs:288–318`): `UniqueBest` → extract → **3127 length guard on the sequence length** (not the `extracted` bool) → `methylation_call` (before trim/revcomp) → `single_end_sam_output` → write. QNAME = `@`-stripped `fix_id`; `best.mapq` reused. Matches Perl 3120–3147. ✓
- `unique_best_alignment_count` at merge (Perl 3121); `could_not_extract` after the guard (Perl 3129); per-strand in extraction behind the guards — all three counters land where Perl puts them. ✓
- `MappingQuality::new(n)` returns `None` only for 255; the `calc_mapq` range 0–42 (incl. 0) renders correctly. ✓ (verified against `noodles-sam-0.85.0` source.)
- `reject_unsupported_output_flags` hard-rejects `--slam`/`--non_bs_mm`/`--rg_tag`/`--sam-no-hd`; `deferred_flags` drops them + `--basename`. ✓

### Efficiency
Linear in reads × read-length; genome held once as `Vec<u8>` per chromosome; `refid` is a small `HashMap`. `make_mismatch_string` re-parses the CIGAR (`rebuild_md_with_deletions`) and `cigar_to_ops` re-parses it again, plus `best.cigar.contains('D')` scans repeatedly — all O(CIGAR length), negligible. No concerns; this phase's goal is byte-identity, not speed (matches §6).

---

## Recommendations (prioritized)

### Critical
None.

### High
None.

### Medium

**M1 — `make_mismatch_string` Part 1 appends a NUL byte (not "") when `actual` outruns `ref_seq`.**
`output.rs:134` uses `ref_seq.get(pos).copied().unwrap_or(0)`. If `actual_seq` is longer than `ref_seq`, the mismatch arm pushes `ref_base as char` = `'\0'` into the MD tag (`:141`), whereas Perl's `substr($ref_seq,$pos,1)` returns `""` and appends nothing (Perl 9286/9306/9311). This is **unreachable in the gated path** — after the +2 trim both sequences are read-length and the minus-strand revcomp preserves length, and the 3127 guard rejects short windows — but it is a latent divergence from Perl's empty-substr semantics. Recommend skipping the position when `pos >= ref_seq.len()` (or asserting `actual.len() == ref_seq.len()`), so the code can never silently emit a `\0` into a tag if an upstream invariant is ever broken. (`hemming_dist` already handles the over-run consistently with Perl via `zip` + `actual.len() − matches`, so only Part 1 differs.)

### Low

**L1 — diagnostic output goes to stderr where Perl uses stdout.**
`genome.rs:152` (`chr {name} ({} bp)`) and the empty-seq notice at `:147` use `eprintln!` (stderr); Perl prints the "chr … (N bp)" line via `print` to **stdout** (5081/5088/5110/5120) and only the empty-seq notice via `warn` (stderr). This does not affect the BAM byte-identity gate (`samtools view -h`), but if any downstream check ever diffs stdout, the streams diverge. Cosmetic; flag for awareness only.

**L2 — `make_mismatch_string`/`rebuild` trailing-arm digit test is `all(is_ascii_digit)` vs Perl's unanchored `/\d+/`.**
`output.rs:287–288` gates the trailing arm on `!op_str.is_empty() && all(is_ascii_digit)`; Perl 9526 uses `$op =~ /\d+/` ("contains a digit"). These agree for every reachable `$op` value (always all-digits, a single mismatch base, or empty, by construction), so there is **no behavioural difference**. Noting only because the predicates are not literally identical — if a future change ever lets `op` hold a mixed string, they would diverge. No action required.

**L3 — `else`-arm "should never happen" path carries on instead of dying.**
`output.rs:272–282` (Perl 9506–9522): when the prior `op` is a word-char and the current element is **not** a digit, Perl `die`s (9517). The Rust skips the `if el.is_ascii_digit()` block but still runs `op_acc = Some(el.to_string())` and continues. Unreachable for valid MD strings (mismatches are always followed by a digit); the comment at `:279–280` acknowledges it. Acceptable; optionally surface it as a hard error to match Perl's fail-loud posture rather than continuing silently.

**L4 — (Inherited, out of Phase-5 scope; verify on the #18 gate) QNAME whitespace handling.**
The driver writes the `@`-stripped `fix_id` (whitespace-runs→`_`) as QNAME (`lib.rs:269–271`). This Perl `bismark` v0.25.1 does **not** normalize read-name whitespace when writing the converted FastQ (it writes `$header` verbatim, 5281, and only *counts* tab headers, 5266) — Bowtie 2 then truncates QNAMEs at the first whitespace internally. The `fix_id` mechanism is a Phase-2/4 lockstep decision already gated in those phases; for reads whose names contain spaces, confirm the Rust QNAME matches Perl's emitted QNAME on the Linux real-data gate (#18). Not a Phase-5 defect — the extraction/output code under review is correct given whatever ID it is handed.

---

## Confidence notes
- The deletion re-indexing — the single highest-risk surface — was traced by hand for the 1-del, 2-del, and del-adjacent-mismatch cases and matches both the Perl algorithm and the in-tree unit tests.
- The double-revcomp composition (index-1 + deletion) and the two-distinct-complement-helpers concern were both verified as net-identity on the upper-cased genome and correctly applied verbatim.
- The byte-exactness gate (#18, real Bowtie 2 + Perl on Linux/oxy) remains the final arbiter for argv-string and QNAME edge cases; the hermetic suite (108 unit + 16 integration, incl. the header byte-diff and the BAM round-trip) pins the noodles half well.
