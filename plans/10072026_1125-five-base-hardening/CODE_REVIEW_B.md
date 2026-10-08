# Code Review B — #1125 5-Base hardening (uncommitted, `fix/1125-five-base-hardening`)

Reviewer B, independent. Scope: `git diff dev -- rust/` (cli.rs, config.rs, mod.rs, output.rs).
Mode: recommend only. I edited no source files.

## Summary

The three hardening items are correct as written. The default (flag-off) path is unchanged: `describe_record` runs only on a write error, and `YS` is gated on `strand_id`. `--strandID` reaches every PE record builder. The lockstep QNAME normalisation is valid for bowtie2 and minimap2.

**The review also turned up what is very probably the #1125 root cause, and it changes how the lockstep check behaves in practice.** The 5-Base bowtie2/hisat2 option string runs `-p N` (default: every core) **without `--reorder`**. Bowtie 2 then writes pairs out of input order, while the 5-Base driver pairs FASTQ and SAM by position. Once this branch lands, the new lockstep check aborts nearly every multi-threaded `--illumina_5base --bowtie2/--hisat2` run on an early pair. That abort is correct: the run would otherwise write wrong data. But the branch should not ship without `--reorder`.

Checks run (all VERIFIED):
- `cargo test --offline -p bismark --lib -- strand_id describe_record five_base_lockstep five_base_emit_pe paired_end`: 10 passed, 0 failed.
- `RUSTFLAGS="-D warnings" cargo clippy --offline -p bismark --all-targets`: clean. The only warning is the build-script xcrun sandbox noise.
- `cargo fmt -p bismark -- --check`: clean (no diff).

---

## Critical

### C1. 5-Base bowtie2/hisat2 runs `-p N` without `--reorder`, so output order differs from input order (likely the #1125 root cause)

- **Where:** `rust/bismark/src/aligner/mod.rs:1561-1562` (`five_base_aligner_options`):
  `Aligner::Bowtie2 => format!("-q --score-min L,0,-0.6 -p {n}")`, and the same for Hisat2. `n` falls back to `available_parallelism()`.
- **The positional pairing assumption:** `five_base_align_and_call_pe` (mod.rs:1935-1943) takes the next two SAM primaries as FASTQ pair *k*.
- **Contrast:** the faithful path adds `--reorder` under `-p` (`options.rs:158-169`, Perl `bismark:7999`: "abolutely required for parallelization to work").
- **VERIFIED by experiment.** Setup: bowtie2 2.5.5 (Homebrew), a 2 Mb random genome, and 200,000 simulated PE pairs with variable read lengths (60-150). Named `read{i} 1:N:0:ACGT` and the 5-Base option string:
  - `-p 8`: **91,728 / 200,000 pairs out of input order; the first mismatch is at pair 496** (SAM has `read528`).
  - `-p 8 --reorder`: 0 out of order.
  - `minimap2 -a -x sr -t 8`: 0 out of order, so the default 5-Base engine is unaffected. That explains why only `--bowtie2` was reported.
- **Failure scenario:** FASTQ pair *k*'s SEQ/QUAL/ID gets attached to pair *j*'s CIGAR/POS. With variable-length (trimmed) reads, the SEQ length soon differs from the CIGAR's read length. noodles then rejects the record with exactly `read length-sequence length mismatch`. With fixed-length reads it is worse: no error, and the methylation calls, QNAMEs and UMI dedup are silently wrong.
- **Interaction with this branch:** `five_base_check_lockstep` will catch this on the first reordered pair, a few hundred pairs in. So item 2 turns a late, confusing noodles error into an early, accurate "out of step" error. It does not make the run succeed.
- **Recommendation:** append `--reorder` to both 5-Base option strings, e.g. `-q --score-min L,0,-0.6 -p {n} --reorder`. It belongs in the same PR, or in a PR that merges first. Without it, the branch makes every multi-threaded bowtie2/hisat2 5-Base run fail fast.
  - Also add a unit test asserting `--reorder` is in `five_base_aligner_options` for Bowtie2 and Hisat2.
  - The report's "was run with" line changes too. That is acceptable: 5-Base is not byte-frozen.
- **Hisat2:** INFERRED, since it is not installed locally. HISAT2 shares Bowtie 2's threading/output code and documents the same `--reorder` semantics.
- **Confidence:** high that bowtie2 5-Base output is reordered (measured). High that this produces the #1125 symptom on trimmed PE data (mechanism matches exactly). Final confirmation still needs the user's read pair or a rerun with `--reorder`.

---

## Logic

### L1. Lockstep normalisation: valid for bowtie2 and minimap2 (sound)

- **VERIFIED by experiment** with FASTQ ids `@read0/1 1:N:0:ACGT` / `@read0/2 2:N:0:ACGT`: both bowtie2 (`--reorder`) and minimap2 `-x sr` print QNAME `read0` for both mates. The Rust FASTQ side gives `fix_id(icpc=true)` → `read0/1` → `strip_mate_suffix` → `read0`, so it matches.
- Ids without a mate suffix and ids with a whitespace comment also match.
- The SAM side also runs `strip_mate_suffix`. So it does not matter whether the aligner strips `/1` before or after whitespace truncation: Bowtie 2 strips a trailing `/1` from the *full* name and then truncates, which can leave `read0/1`.
- Each mate is compared against its own FASTQ file, so SRA-style `X.1.1` / `X.1.2` R1/R2 names are fine.
- rammap cannot reach this path: `config.rs:1331-1337` rejects `--illumina_5base --rammap`.
- `--five_base_umi_qname`: the UMI sits inside the first whitespace token, so both sides carry it identically. No issue. (INFERRED from code; not run.)

### L2. `--skip` / `--upto` interaction (sound, minor gap)

- The check runs after the skip `continue` and the upto `break`, so skipped pairs are consumed but not checked. A desync inside the skipped prefix is still caught on the first checked pair.
- The check runs **before** the UMI-dedup `continue` (mod.rs:1985-1994), which is correct: deduplicated pairs are still verified. VERIFIED by reading.

### L3. Possible false positives on unusual but valid input (Low, INFERRED)

- **CRLF FASTQ with no whitespace comment on the id line.** `chomp_newline` keeps `\r`, and `fix_id` only truncates at space/tab, so the FASTQ id is `readA\r`. Bowtie 2 and minimap2 truncate names at `isspace`, which includes `\r`, so the QNAME is `readA` and the check aborts.
  - This input already failed before the change: the SEQ also keeps `\r`, which gives the same SEQ/CIGAR length mismatch on read 1. So this is not a regression, only a different message.
  - Optional fix: trim a trailing `\r` on both sides.
- **QNAME longer than 255 characters.** Bowtie 2 truncates QNAME at 255 characters (`truncQname`); the FASTQ side does not, so such reads would abort. These are vanishingly rare.

### L4. `--strandID` index→label mapping (sound)

- **VERIFIED** against Perl `bismark:8743-8763`: CT/CT → 0 → OT, GA/GA → 1 → CTOB, GA/CT → 2 → CTOT, CT/GA → 3 → OB. Rust `output.rs:542-547` is identical.
- The index semantics match Rust's own byte-frozen FLAG table a few lines above (0 → 99/147 OT, 1 → 163/83, 2 → 147/99, 3 → 83/163), which follows Perl 8825-8868.
- The `_ => "OB"` arm cannot be reached because the FLAG match returns `Err` first for any index above 3. It is fine, but `3 => "OB", _ => unreachable!()` would state the invariant more plainly (Low, style).

### L5. Tag order (sound)

- Perl's no-`--rg_tag` default prints `... XG, YS` (`bismark:9213-9214`). Rust inserts `YS` right after `XG` and before CB/UR. The new test asserts `NM MD XM XR XG YS` for all four indices on both mates. VERIFIED (test ran).
- CB/UR are Rust-only (no Perl oracle), so putting them after `YS` is a free choice and sensible.
- In 5-Base, `RX` is added later via `set_rx`, so it comes after `YS`. Fine.

### L6. Every PE record path receives the flag (sound)

- `paired_end_sam_output` is the only PE record builder, and `build_pe_mate` is its only callee that writes XG. Its only non-test callers are:
  - `route_pe_decision` (mod.rs:5335), which is the shared router for `drive_merge_pe` (serial/parallel faithful), `drive_merge_combined_pe` (combined index) and `select_and_route_pe_nondir`;
  - `five_base_emit_pe_record` (mod.rs:2129).
- Both pass `config.strand_id`, and because the parameter is positional, the compiler forces every caller to supply it.
- The ambiguous-BAM path (`output.rs:876`) re-emits the raw aligner line, as Perl does, so it correctly carries no YS.
- There is no 5-Base SE driver: `five_base_emit_record` is reached only from the consensus synthesiser.
- VERIFIED by `git grep`.

### L7. SE silently ignores `--strandID` (matches Perl; Low)

This is Perl-faithful. Given the suite's never-silent ethos, a one-line stderr notice for `--strandID` without `-1/-2` would be cheap. It is optional, and it does not change BAM bytes.

## Errors

### E1. `describe_record` (sound)

- It only runs inside `map_err`, so the success path and default output are untouched.
- The read-length sum counts M/I/S/=/X, which matches SAM §1.4.10 and noodles' check. The test covers S, M, D and I.
- An empty CIGAR prints `*`. An empty SEQ prints 0, which is correct: noodles skips the check for an empty sequence.
- VERIFIED (test ran).

### E2. Child process on lockstep error (Low, existing pattern)

`return Err(...)` from inside the loop drops `child` without `wait()`/`kill()`. The aligner then gets SIGPIPE when it next writes, and the parent exits. The existing "fewer PE records … (desync)" error behaves the same way, so this is not new. Leaving it as is is fine.

## Efficiency

### F1. Extra allocations per pair (Low)

- Each pair now does one extra `fix_id` + lossy UTF-8 + `strip_mate_suffix` String for R2, plus two `strip_mate_suffix` Strings for the SAM QNAMEs.
- This is negligible next to alignment, but the comparison could avoid allocating: compare `&str`s from a `strip_mate_suffix` variant that returns `&str`.

## Structure / style

- The R1 and R2 normalisation is written out twice, with slightly different shapes: R1 binds `id_bytes`, R2 inlines it. A small `fastq_identifier(&id) -> String` helper would remove the duplication and make the symmetry obvious (Low).
- Comments are single-line and describe current behaviour. The cli.rs help text and the `paired_end_sam_output` doc update are accurate.
- Minor: the label order in the help text (`OT|CTOT|CTOB|OB`) differs from the commit/issue wording (`OT|CTOB|CTOT|OB`). Either is fine.
- The `five_base_check_lockstep` error message names the aligner, the pair number, the mate and both ids. Good, and the new test covers it.

## Test gaps

- **T1 (Medium):** add a unit test that `five_base_aligner_options` contains `--reorder` for Bowtie2 and Hisat2 (this guards C1).
- **T2 (Low):** no test covers the plumbing from `config.strand_id` to the records. Both drivers are only exercised with `false`.
  - A `five_base_emit_pe_record(..., strand_id=true, ...)` assertion that index 0 gives `YS:Z:OT` and index 3 gives `YS:Z:OB` would guard the 5-Base wiring.
  - An aligner-fixture integration run with `--strandID` would guard `route_pe_decision`.

## Recommendations by priority

| Priority | Item | Location |
|---|---|---|
| **Critical** | Add `--reorder` to the 5-Base bowtie2 and hisat2 option strings (C1). Without it, item 2 makes every multi-threaded bowtie2/hisat2 5-Base run abort, and fixed-length data is silently wrong today. | mod.rs:1561-1562 |
| Medium | Test asserting `--reorder` in the 5-Base options (T1) | mod.rs tests |
| Low | Test the `strand_id=true` wiring through `five_base_emit_pe_record` (T2) | mod.rs tests |
| Low | Trim a trailing `\r` in the lockstep normalisation, or leave it (CRLF already failed) (L3) | mod.rs:1972-1980 |
| Low | Factor the duplicated FASTQ-id normalisation; compare without allocating (F1/Structure) | mod.rs:1972-1980, 2041-2056 |
| Low | `3 => "OB", _ => unreachable!()` instead of `_ => "OB"` (L4) | output.rs:542-547 |
| Low | Optional notice when `--strandID` is given for SE (L7) | config.rs |

## Evidence log

- Reorder experiment: `$TMPDIR` scratchpad `reorder/` (simulated `g.fa`, `r1.fq`, `r2.fq`, and the `chk.py` order checker).
- bowtie2 2.5.5 `-p 8`: `pairs 200000 out_of_order 91728 first (496, 'read528')`. `-p 8 --reorder`: `out_of_order 0`.
- minimap2 `-t 8`: `out_of_order 0` (200k). With `/1 /2` suffixes and comments, both aligners print QNAME `read0`.
