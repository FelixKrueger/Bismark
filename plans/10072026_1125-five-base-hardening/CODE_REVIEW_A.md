# Code Review A — #1125 5-Base hardening (uncommitted, `fix/1125-five-base-hardening`)

Reviewer A, independent. Scope: `git diff dev -- rust/` (cli.rs, config.rs, mod.rs, output.rs). Recommend-only; no source edited.

## Summary

The three items are correctly written in isolation: `describe_record` is right, the `YS:Z:` index→label mapping matches Perl and the Rust index semantics, every production PE emission path gets the flag, and the default (flag-off) output is unchanged. Targeted unit tests, the real-minimap2 5-Base integration suites, `clippy --all-targets -D warnings` and `fmt --check` all pass.

**One Critical finding goes beyond the diff, and the lockstep check is what exposes it.** The bowtie2/HISAT2 5-Base driver runs the aligner with `-p N` and **without `--reorder`**, and `N` is always ≥ 2 (`-p 1` is rejected). So the output order does not match the FASTQ input. On `dev` this **silently** drops or mispairs reads. With this diff, the new lockstep check makes **every non-trivial bowtie2/HISAT2 5-Base PE run abort**. The check is correct here (true positive). But shipping item 2 without adding `--reorder` turns runs that currently "succeed" into hard failures. Adding `--reorder` is also the most likely real fix for #1125, though I could not reproduce the exact noodles error with it (details below).

## Critical

### C1. 5-Base bowtie2/HISAT2 output is unordered: `-p N` without `--reorder`
`rust/bismark/src/aligner/mod.rs:1550-1551` (`five_base_aligner_options`):
```rust
Aligner::Bowtie2 => format!("-q --score-min L,0,-0.6 -p {n}"),
Aligner::Hisat2 => format!("-q --no-spliced-alignment --score-min L,0,-0.6 -p {n}"),
```
`n` falls back to `available_parallelism()` when `-p` isn't given, and the CLI rejects `-p 1` ("Please select a value for -p of 2 or more!"). So this path is **always multithreaded**. Without `--reorder`, Bowtie 2 and HISAT2 emit records in completion order. Perl adds `--reorder` whenever it threads (`legacy_perl/bismark:7999`: "This is absolutely required for parallelization to work"), and so does the faithful Rust path (`options.rs:169`). The 5-Base driver pairs FASTQ↔SAM purely by position, so this breaks the pairing.

**VERIFIED** on a synthetic set: 200 kb random genome, 40,000 PE pairs, read lengths 50–150, Illumina-style spaced headers. Scratch files are at `/private/tmp/claude-501/-Users-fkrueger-Github-Bismark/cff9c386-4d4f-4a99-9cec-fcd5df9ff4a2/scratchpad/bt2exp/`; the `dev` build is a `git archive` export in `../devsrc`, binary in `../devtarget`.

| Binary | Aligner invocation | Result |
|---|---|---|
| this diff | bowtie2 `-p 8` | `error: Bowtie 2 output is out of step with the FASTQ input at read pair 497: R1 is 'p496' in the FASTQ but 'p512' in the alignment` |
| this diff | bowtie2 `-p 2` | aborts at pair 193 |
| this diff | bowtie2 + `--reorder` (via a `--path_to_bowtie2` wrapper), `-p 8` | passes; 80,000 records; `samtools quickcheck` OK |
| `dev` | bowtie2 `-p 8` | exit OK, but **`could-not-extract genomic: 24412` of 40000**: 61% of pairs silently dropped, and the survivors can be mispaired whenever lengths happen to agree |
| `dev` | bowtie2 + `--reorder`, `-p 8` | `could-not-extract genomic: 0` |

**Relation to #1125 (INFERRED):** this is the strongest root-cause candidate. Out-of-order output gives a CIGAR from one read and a SEQ from another, which is exactly the shape of noodles' "read length-sequence length mismatch". But on my synthetic data (including 20% 2-bp insertions and 20% 3-bp deletions on R1) `dev` did **not** reach the noodles error. The `ext...len() != seq.len() + 2` guard in `five_base_emit_pe_record` (`mod.rs:~2108`) dropped the mismatched pairs first. So some property of the user's data (CIGAR shape, soft clips, a guard gap) must let a mismatched pair past that guard. The triggering read pair is still needed to confirm. Either way, missing `--reorder` is a confirmed bug: it silently loses data and corrupts calls whenever mispaired reads share a length (e.g. untrimmed 2×150).

**Recommendation (Critical):** add `--reorder` to both the Bowtie2 and HISAT2 strings in `five_base_aligner_options`, plus a unit test that pins the option string. It must land **with or before** item 2. Otherwise item 2 turns silent loss into a 100% abort for bowtie2/HISAT2 5-Base users. Once `--reorder` is in, the lockstep check is a sound guard.

## Logic — item 2: lockstep check (`mod.rs:1972-1980`, `2040-2056`)

Normalisation per aligner:
- **minimap2** (the default 5-Base engine). kseq truncates the name at whitespace, and `fix_id(…, true)` truncates at space/tab. `strip_mate_suffix` on both sides absorbs any `/1` `/2`. **VERIFIED** with spaced Illumina headers: the real-minimap2 suites (`aligner_five_base_groundtruth`, 8/8, including `five_base_groundtruth_illumina_spaced_header_no_desync`) pass, and my 40k-pair `--strandID` minimap2 run passes.
- **bowtie2/HISAT2.** These strip a trailing `/1|/2|/3` from the full name, then truncate at `isspace`. For `@r/1 1:N:0`, bowtie2 gives `r/1` and both sides strip it to `r`. For `@r 1:N:0/1`, bowtie2 gives `r` and the FASTQ side `fix_id` gives `r`. Both match. **VERIFIED** end-to-end with `--reorder`.
- **rammap** is rejected for `--illumina_5base` (`config.rs:1334`). Not reachable.
- `--five_base_umi_qname`: the UMI sits before the first space, so both sides keep it. Sound. `--skip`/`--upto`: the check runs after the skip `continue`, so skipped pairs aren't validated. That's harmless because the SAM records are still consumed in step.

Residual false-positive risks (all Low):
- **L1 (INFERRED from bowtie2 source):** Bowtie 2 truncates QNAME at 255 chars (`truncQname`). A FASTQ id longer than 255 would mismatch. Very rare.
- **L2 (INFERRED):** CRLF FASTQ. `chomp_newline` keeps `\r`, so the FASTQ side is `p0\r`, while the aligner drops it as whitespace. The run now aborts at pair 1 with a message that shows an invisible `\r`. `dev` was already broken here (SEQ carries `\r`, so the extract guard drops everything), so this is not a regression. Optionally trim `\r` before comparing, or show the ids with `{:?}`.
- **L3 (INFERRED):** Bowtie 2 also strips `/3`. Negligible.

Message quality (Low): with C1 unfixed, "out of step with the FASTQ input" points users at their FASTQ, but the cause is the missing `--reorder`. After C1 it's accurate. The message could note "(input not name-sorted / truncated?)" or stay as is.

Efficiency (Low): about 4 small allocations per pair (`fix_id` Vec, the lossy `String`, two `strip_mate_suffix(...).to_string()`). Negligible next to alignment. A borrowed `&str` compare would avoid them if anyone cares.

## Logic — item 3: `--strandID` (`output.rs:541-547`, `739-742`)

- **Mapping VERIFIED** against Perl `bismark:8746-8761`: index 0 is CT/CT → OT, 1 is GA/GA → CTOB, 2 is GA/CT → CTOT, 3 is CT/GA → OB. It also matches the Rust index table in `methylation.rs:451-455`. The `_ => "OB"` arm is unreachable because the FLAG `match` above returns `Err` for any index outside 0..=3. Fine.
- **Tag order VERIFIED:** NM, MD, XM, XR, XG, YS on both mates, which is Perl's no-`--rg_tag` order (`bismark:9213-9214`). CB/UR (Rust-only, no Perl oracle) follow YS. That's reasonable and consistent.
- **Path coverage VERIFIED by grep:** every Bismark PE record is built by `paired_end_sam_output` → `build_pe_mate`, which has only two production callers. One is `route_pe_decision` (`mod.rs:5335`), which every faithful/parallel/combined/non-directional driver goes through (`drive_merge_pe`, `drive_merge_combined_pe`, `select_and_route_pe_nondir`). The other is `five_base_emit_pe_record` (`mod.rs:2129`). Both read `config.strand_id`. The only two `RunConfig` constructors (`resolve`, `run_config_stub`) set it. No path drops it.
- **Default output unchanged:** with the flag off, `ys` is `None` and no tag is inserted. The unit test asserts this for all four indices.
- **End-to-end VERIFIED:** a 5-Base minimap2 PE run and a bowtie2 run with `--reorder` both put `YS:Z:OT` on every record (80,000/80,000). The 5-Base driver only produces index 0/3, so it can only ever emit OT/OB.
- **M1 (Medium, never-silent):** SE runs with `--strandID` still ignore the flag silently. That's Perl-faithful, but the Rust suite's convention (`deferred_flags`, the never-silent notices) is to tell the user. Suggest a one-line stderr notice for SE + `--strandID`. Optional: the CLI help already says "Paired-end only".
- Bowtie 2's own `YS:i` (mate score) never reaches the Bismark BAM. The `--ambig_bam` raw lines carry `YS:i` but are a separate file and get no `YS:Z`, which matches Perl. No collision.

## Item 1: `describe_record` (`output.rs:786-829`)

Correct. M/I/S/=/X are counted as read-consuming, which matches noodles' check. The unit test covers a mixed CIGAR, and the function only runs on the error path. `write_record` is the single Bismark-record BAM sink in the aligner (VERIFIED by grep; `write_raw_record` is only the `--ambig_bam` raw path). The encode is synchronous in `noodles_bam::io::Writer`, so the record named in the error really is the one that failed. Sound.

Nit (Low): `cigar.push_str(&format!(...))` allocates per op; `write!(cigar, ...)` avoids that. Error path only, so it doesn't matter.

## Structure / style

- Comments are one line, state the current fact and give a Perl line reference. Compliant. The `paired_end_sam_output` doc correctly drops `!strandID` from its "default path" list.
- `clippy -p bismark --all-targets` with `RUSTFLAGS=-D warnings`: clean. `cargo fmt -p bismark -- --check`: clean (VERIFIED).
- Tests: the lockstep unit test covers accept, `/2`-strip and reject. Missing: an integration test that would have caught C1, i.e. a bowtie2 5-Base PE run on more than a few hundred variable-length pairs, gated on bowtie2 like the minimap2 gates. Recommend adding it with the C1 fix (High).

## Recommendations by priority

| Priority | Item | Where |
|---|---|---|
| **Critical** | Add `--reorder` to the Bowtie2 and HISAT2 5-Base option strings; land it with or before the lockstep check | `mod.rs:1550-1551` |
| High | bowtie2-gated 5-Base PE integration test (variable-length reads, ≥ a few thousand pairs, `-p ≥2`) asserting zero lockstep errors and `could-not-extract genomic: 0` | `rust/bismark/tests/` |
| High | Ask the #1125 reporter to retry with a wrapper/`--reorder` build. If the error disappears, C1 is the root cause | issue thread |
| Medium | Never-silent notice for `--strandID` on SE | `config.rs` / run setup |
| Low | Trim `\r` before the lockstep compare, or show ids with `{:?}` | `mod.rs:1972-1978` |
| Low | Optional allocation trims in lockstep / `describe_record` | — |

## Verification log
- `cargo test --offline -p bismark --lib -- strand_id five_base_lockstep describe_record five_base_emit paired_end`: 14 passed.
- `cargo test --offline -p bismark --test aligner_five_base_groundtruth --test aligner_five_base_bisulfite` (real minimap2): 8 + 11 passed.
- `RUSTFLAGS=-D warnings cargo clippy --offline -p bismark --all-targets`: clean. `cargo fmt -p bismark -- --check`: clean.
- Synthetic bowtie2 / `dev`-vs-diff experiments as tabulated under C1.
