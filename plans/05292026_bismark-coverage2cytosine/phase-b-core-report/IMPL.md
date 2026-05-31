# IMPL — Phase B (TDD task list)

**Source plan:** `phase-b-core-report/PLAN.md` (rev 1). **Goal:** the core genome-wide cytosine report — cov parse + genome walk (exact coordinate arithmetic) + context classify + CpG/`--CX` emit + context summary, PLAIN output, **byte-identical to Perl v0.25.1**.

**Mode:** TDD (RED→GREEN→REFACTOR).
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c`. Crate: `rust/bismark-coverage2cytosine/`.
**Command base:** cargo from `/Users/fkrueger/Github/Bismark-c2c/rust`; per-crate `cargo test -p bismark-coverage2cytosine`. **All cargo/perl commands need `dangerouslyDisableSandbox: true`** (worktree outside the sandbox-writable root). Do NOT touch `rust/bismark-extractor` or `rust/bismark-bedgraph`.

## Test infrastructure
- **Unit tests:** inline `#[cfg(test)] mod tests` in `src/report.rs`, `src/summary.rs`, `src/cov.rs`.
- **Integration golden test:** `tests/golden_phase_b.rs` (via `assert_cmd`), comparing the built binary's output to **committed Perl-v0.25.1 goldens**.
- **Goldens are generated locally** by `tests/data/phase_b/generate_goldens.sh` — it runs the repo's `perl ../../../../coverage2cytosine` (v0.25.1, self-contained Perl; **verified runs locally** 2026-05-29) on the committed synthetic fixtures and writes the goldens. Fixtures + goldens are committed; the script is the provenance + regeneration path. (No external test data; no colossal needed for these small fixtures.)
- **Concrete arithmetic anchor** (verified via Perl this session): genome `ACGTACGCGT` + cov `chr1\t3\t3\t100\t5\t0` → CpG report:
  ```
  chr1	2	+	0	0	CG	CGT
  chr1	3	-	5	0	CG	CGT
  chr1	6	+	0	0	CG	CGC
  chr1	7	-	0	0	CG	CGT
  chr1	8	+	0	0	CG	CGT
  chr1	9	-	0	0	CG	CGC
  ```
  Use this as a kernel/integration assertion.

## Plan coverage checklist

| # | Plan item | Source | Task(s) |
|---|-----------|--------|---------|
| 1 | `perl_substr` negative-wrap helper | §3.3, §4 sig | T1 |
| 2 | `revcomp` (N untouched) | §3.3 | T1 |
| 3 | `classify_context` CG/CHG/CHH/None | §3.3.6 | T1 |
| 4 | `ContextSummary` 64-cell + accumulate (pure-ACTG gate) + write (%.2f/N/A, sorted, header) | §3.6 | T2 |
| 5 | gz-aware cov open | §3.1.1 | T3 |
| 6 | cov line parse: CRLF strip, blank skip, fields 0/1/4/5, strict u32 → `MalformedCovLine` | §3.1.2 | T3 |
| 7 | `EmptyCoverageInput` + `MalformedCovLine` errors | §5, §3.1.5 | T3, T5 |
| 8 | forward-C extraction (tri_nt, upstream incl. i=0 wrap) | §3.3.1 | T4 |
| 9 | reverse-G extraction (tri_nt revcomp, i<2 edge, upstream revcomp) | §3.3.1 | T4 |
| 10 | guards: len<3, last-base, threshold | §3.3.2-5 | T4 |
| 11 | context-summary accumulate (before CpG filter, covered only) | §3.3.7 | T4 |
| 12 | emit: CpG-only (CG) vs --CX (all); report-line bytes; --zero_based | §3.3.8, §3.4 | T4 |
| 13 | streaming per-chr flush; covered = appearance order | §3.1.3/3.1.6 | T5 |
| 14 | fresh-buffer seeding (triggering line) | §3.1.3 (rev1 A) | T5 |
| 15 | non-contiguous re-flush; `seen` ≠ flush-dedup | §3.1.4 (rev1 C1) | T5 |
| 16 | duplicate-position last-write-wins | §3.1.3 (rev1 B-I2) | T5 |
| 17 | blank/trailing-line no phantom flush | §3.1.2 (rev1 B-I3) | T5 |
| 18 | empty-cov → `EmptyCoverageInput` before uncovered pass | §3.1.5 | T5 |
| 19 | uncovered pass: `names_sorted()\seen`, threshold==0 only, no summary | §3.5 | T6 |
| 20 | cov chr absent from genome → emits nothing | §3.2 | T6 |
| 21 | `lib.rs::run` (load genome + run_report); wire `main.rs` | §2, §4 | T7 |
| 22 | filename derivation (.CpG_report.txt / .CX_report.txt / .cytosine_context_summary.txt) | §3.4/§3.6 | T7 |
| 23 | `open_report_writer` seam for Phase C | §5 step 6 | T7 |
| 24 | V1–V24 validations | §9 | T1–T8 |
| 25 | byte-identity golden integration | §9 V15 | T8 |
| 26 | clippy/fmt/workspace build | §9 V (process) | T9 |

All items map to ≥1 task. ✔ (No parallel streams — `report.rs` is shared across T1/T4/T5/T6; single sequential stream.)

---

## Task 1 — `report.rs` primitives: `perl_substr`, `revcomp`, `classify_context`

**Files:** new `src/report.rs` (+ `pub mod report;` in `lib.rs`).

**Step 1 — RED** (inline tests):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn perl_substr_interior_and_truncation() {
        assert_eq!(perl_substr(b"ACGT", 1, 2), b"CG");
        assert_eq!(perl_substr(b"ACGT", 2, 9), b"GT");      // truncated at end
        assert_eq!(perl_substr(b"ACGT", 9, 3), b"");        // start past end
    }
    #[test] fn perl_substr_negative_wraps_from_end() {
        assert_eq!(perl_substr(b"ACGT", -1, 3), b"T");      // Perl substr(s,-1,3)
        assert_eq!(perl_substr(b"ACGT", -2, 3), b"GT");
        assert_eq!(perl_substr(b"ACGT", -9, 3), b"ACG");    // |off|>len → clamp to 0
    }
    #[test] fn revcomp_complements_acgt_leaves_n() {
        assert_eq!(revcomp(b"ACG"), b"CGT");
        assert_eq!(revcomp(b"GCG"), b"CGC");
        assert_eq!(revcomp(b"ANG"), b"CNT");                // N passes through
    }
    #[test] fn classify_context_matches_perl_regex() {
        assert_eq!(classify_context(b"CGT"), Some(Context::Cg));
        assert_eq!(classify_context(b"CAG"), Some(Context::Chg));
        assert_eq!(classify_context(b"CAA"), Some(Context::Chh));
        assert_eq!(classify_context(b"CNG"), Some(Context::Chg)); // . matches N
        assert_eq!(classify_context(b"CNN"), Some(Context::Chh));
        assert_eq!(classify_context(b"CCG"), Some(Context::Chg));
        assert_eq!(classify_context(b"GTA"), None);          // not C-led
        assert_eq!(classify_context(b"CG"), Some(Context::Cg));// CG prefix, len 2 ok for ^CG
        assert_eq!(classify_context(b"CA"), None);            // len<3, no CHG/CHH match
    }
}
```
_Note: Perl `^CG` matches any `tri_nt` starting `CG` regardless of length; `^C.{1}G$` and `^C.{2}$` require exactly len 3. Mirror precisely._

**Step 2 — fail:** module/functions absent.

**Step 3 — GREEN:** implement per PLAN §3.3:
- `pub(crate) fn perl_substr(seq: &[u8], offset: isize, want: usize) -> &[u8]` (negative-from-end, clamp `|off|>len`→0, end-truncate, empty if start≥len).
- `pub(crate) fn revcomp(seq: &[u8]) -> Vec<u8>` (reverse then `complement` byte map A↔T,C↔G, else identity).
- `#[derive(Clone,Copy,PartialEq,Debug)] pub(crate) enum Context { Cg, Chg, Chh }` + `as_bytes()` → `b"CG"`/`b"CHG"`/`b"CHH"`.
- `pub(crate) fn classify_context(tri: &[u8]) -> Option<Context>` mirroring the regex (starts-with `CG` → Cg; len==3 && tri[0]==C && tri[2]==G → Chg; len==3 && tri[0]==C → Chh; else None).

**Step 4 — pass.** **Step 6 — regression:** `cargo test -p bismark-coverage2cytosine report::`.

---

## Task 2 — `summary.rs`: `ContextSummary`

**Files:** new `src/summary.rs` (+ `mod summary;`).

**Step 1 — RED:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn summary_writes_64_rows_sorted_with_header() {
        let s = ContextSummary::new();
        let mut out = Vec::new(); s.write_to(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "upstream\tC-context\tfull context\tcount methylated\tcount unmethylated\tpercent methylation");
        assert_eq!(lines.len(), 1 + 64);
        assert_eq!(lines[1], "A\tCAA\tACAA\t0\t0\tN/A");          // first sorted (tri=CAA, ubase=A)
        assert!(lines.iter().all(|l| l.contains("N/A") || l == &lines[0]));
    }
    #[test] fn summary_accumulates_pure_actg_only_and_formats_percent() {
        let mut s = ContextSummary::new();
        s.accumulate(b"CGT", b'A', 3, 1);          // ACGT-pure → counted
        s.accumulate(b"CNG", b'A', 9, 9);          // tri has N → ignored
        s.accumulate(b"CGT", b'N', 9, 9);          // ubase N → ignored
        let mut out = Vec::new(); s.write_to(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("A\tCGT\tACGT\t3\t1\t75.00"));   // 3/4*100
    }
}
```

**Step 2 — fail.**

**Step 3 — GREEN:** `ContextSummary` over a fixed `[[ (u32,u32); 4 ]; 16]` or `BTreeMap<(Vec<u8>,u8),(u32,u32)>` seeded with all 16×4; `new()` zeroes all 64; `accumulate(tri,ubase,m,u)` adds only if `tri` and `ubase` are pure `ACTG`; `write_to` prints header then sorted `(tri,ubase)` rows: `{ubase}\t{tri}\t{ubase}{tri}\t{m}\t{u}\t{perc}` where `perc = if m+u>0 { format!("{:.2}", m as f64/(m+u) as f64*100.0) } else { "N/A".into() }`.

**Step 4 — pass.** **Step 6 — regression.**

---

## Task 3 — `cov.rs`: gz-aware open + line parser

**Files:** new `src/cov.rs` (+ `mod cov;`); add `EmptyCoverageInput` + `MalformedCovLine { line_no: usize }` to `error.rs`.

**Step 1 — RED:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parse_strips_crlf_and_reads_fields() {
        let (chr, start, m, u) = parse_cov_line(b"chr1\t3\t3\t100\t5\t0\r", 1).unwrap().unwrap();
        assert_eq!((chr.as_slice(), start, m, u), (b"chr1".as_slice(), 3, 5, 0));
    }
    #[test] fn parse_blank_line_is_skipped() {
        assert!(parse_cov_line(b"", 1).unwrap().is_none());
        assert!(parse_cov_line(b"\r", 1).unwrap().is_none());
    }
    #[test] fn parse_malformed_errors() {
        assert!(matches!(parse_cov_line(b"chr1\tNOTNUM\t3\t100\t5\t0", 7),
            Err(BismarkC2cError::MalformedCovLine { line_no: 7 })));
        assert!(matches!(parse_cov_line(b"chr1\t3", 2),           // too few fields
            Err(BismarkC2cError::MalformedCovLine { .. })));
    }
}
```
(`parse_cov_line(line, line_no) -> Result<Option<(Vec<u8>, u32, u32, u32)>, _>`: `Ok(None)` = blank/skip.)

**Step 2 — fail.**

**Step 3 — GREEN:** `open_cov(path) -> Result<Box<dyn BufRead>, _>` (gz-detect by `.gz` suffix → `MultiGzDecoder`, mirror `genome.rs`); `parse_cov_line` (strip trailing `\r`; empty→`Ok(None)`; split on `b'\t'`; need ≥6 fields; fields 1/4/5 strict `u32` via `std::str::from_utf8` + `parse`, any failure → `MalformedCovLine { line_no }`; field 0 = chr `Vec<u8>`). Add the two error variants with Perl-flavored messages.

**Step 4 — pass.** **Step 6 — regression.**

---

## Task 4 — the per-position kernel (the byte-identity crux)

**Files:** `src/report.rs`.

**Step 1 — RED** (drive the exact bytes; uses the verified anchor):
```rust
#[cfg(test)]
mod kernel_tests {
    use super::*;
    use std::collections::HashMap;
    // emit_position appends the report line (if any) to `out` and accumulates summary.
    fn run(seq: &[u8], cov: &[(u32,u32,u32)], cpg_only: bool, zero: bool) -> String {
        let mut buf = HashMap::new();
        for &(p,m,u) in cov { buf.insert(p,(m,u)); }
        let mut out = Vec::new();
        let mut summ = crate::summary::ContextSummary::new();
        let name = b"chr1";
        for i in 0..seq.len() {
            if seq[i]==b'C' || seq[i]==b'G' {
                emit_position(name, seq, i, &buf, cpg_only, zero, 0, true, &mut summ, &mut out);
            }
        }
        String::from_utf8(out).unwrap()
    }
    #[test] fn cpg_report_matches_perl_anchor() {
        let got = run(b"ACGTACGCGT", &[(3,5,0)], true, false);
        assert_eq!(got, "chr1\t2\t+\t0\t0\tCG\tCGT\n\
                         chr1\t3\t-\t5\t0\tCG\tCGT\n\
                         chr1\t6\t+\t0\t0\tCG\tCGC\n\
                         chr1\t7\t-\t0\t0\tCG\tCGT\n\
                         chr1\t8\t+\t0\t0\tCG\tCGT\n\
                         chr1\t9\t-\t0\t0\tCG\tCGC\n");
    }
    #[test] fn zero_based_subtracts_one() {
        let got = run(b"ACGTACGCGT", &[(3,5,0)], true, true);
        assert!(got.contains("chr1\t2\t-\t5\t0\tCG\tCGT\n")); // pos3-1=2 for the covered C
    }
    #[test] fn cx_emits_chg_chh_too() {
        // a CHH/CHG-bearing sequence; assert non-CG contexts appear only under --CX
        let cpg = run(b"ACCAAC", &[], true, false);
        let cx  = run(b"ACCAAC", &[], false, false);
        assert!(!cpg.contains("CHH") && !cpg.contains("CHG"));
        assert!(cx.contains("CHH") || cx.contains("CHG"));
    }
    #[test] fn last_base_excluded_and_short_tri_skipped() {
        // trailing C at the very last base must not emit (len-pos==0 guard / len<3)
        let got = run(b"AAC", &[(3,9,0)], true, false);
        assert_eq!(got, "");
    }
    #[test] fn threshold_filters() {
        let got = run_t(b"ACGTACGCGT", &[(3,2,0)], true, false, 5); // coverage 2 < 5
        assert_eq!(got, /* the pos3 line is dropped; uncovered 0,0 also dropped */ "");
    }
}
```
(Add a `run_t` variant threading a non-zero threshold; with threshold>0, uncovered `0 0` positions are also dropped → empty output here.)

**Step 2 — fail.**

**Step 3 — GREEN:** implement `emit_position(name, seq, i, buffer, cpg_only, zero_based, threshold, accumulate_summary, &mut summary, &mut out)` per PLAN §3.3 exactly:
extract tri_nt+upstream (forward-C / reverse-G via `perl_substr`+`revcomp`); guard `tri.len()<3`; guard `(seq.len() as u32 - pos)==0`; lookup `(m,u)`; guard `m+u<threshold`; `classify_context` (None → return, the stderr-warn is informational); if `accumulate_summary` call `summary.accumulate(tri, upstream[0], m, u)`; emit when `cpg_only ⇒ ctx==Cg` else always — write `name`, `\t`, `pos`(or `pos-1`), `\t`, strand, `\t`, m, `\t`, u, `\t`, ctx bytes, `\t`, tri bytes, `\n` to `out`.

**Step 4 — pass** (the anchor test is the proof). **Step 5 — REFACTOR:** factor `extract(seq,i) -> (tri, upstream, strand)` if `emit_position` gets long. **Step 6 — regression.**

---

## Task 5 — `run_report` streaming flush (covered chromosomes)

**Files:** `src/report.rs`.

**Step 1 — RED** (unit-test the streaming over an in-memory genome + cov bytes; helper builds a `Genome` via a tiny FASTA tempdir + cov string):
```rust
// Tests (sketch — full bodies in implementation):
// - covered_order_is_cov_appearance: cov "chrB...chrA" -> report emits chrB block then chrA block.
// - fresh_buffer_seeded_on_transition: first covered pos of chrB present in output.
// - non_contiguous_chr_reflushes: cov "chrA(p2=5),chrB(...),chrA(p6=7)" -> chrA's full report appears TWICE.
// - duplicate_position_last_write_wins: two lines chrA p3 -> 2nd counts used.
// - trailing_newline_no_phantom + blank_line_skipped.
// - empty_cov_errors: empty cov -> Err(EmptyCoverageInput), no output files.
```

**Step 2 — fail.**

**Step 3 — GREEN:** `run_report(config, genome)`: `open_cov`; open report writer (T7 seam); init `ContextSummary`; stream lines via `parse_cov_line`; maintain `cur_chr: Option<Vec<u8>>` + `buffer: HashMap<u32,(u32,u32)>` + `seen: HashSet<Vec<u8>>`; on `chr != cur_chr` → `flush_chromosome(prev)` then `buffer.clear()` + seed with triggering line + `seen.insert`; else `buffer.insert` (last-write-wins). After EOF: if `cur_chr.is_none()` → `EmptyCoverageInput`; else flush last. `flush_chromosome(name)`: `genome.get(name)` → if None skip; else walk C/G calling `emit_position(..., accumulate_summary=true)`.

**Step 4 — pass.** **Step 6 — regression.**

---

## Task 6 — uncovered-chromosome pass

**Files:** `src/report.rs`.

**Step 1 — RED:**
```rust
// - uncovered_emitted_sorted_when_threshold_zero: genome {chrA,chrB,chrZ}, cov covers chrB only
//     -> after chrB, emit chrA then chrZ (names_sorted order), all 0,0; no summary contribution.
// - uncovered_suppressed_when_threshold_positive: --coverage_threshold 5 -> no uncovered lines.
// - cov_chr_absent_from_genome_emits_nothing: cov has chrQ not in genome -> no panic, no chrQ lines.
```

**Step 2 — fail.**

**Step 3 — GREEN:** after the covered flush in `run_report`: `if config.threshold == 0 { for name in genome.names_sorted() { if !seen.contains(name) { flush walk with empty buffer + accumulate_summary=false } } }`.

**Step 4 — pass.** **Step 6 — regression.**

---

## Task 7 — `lib.rs::run` + filename derivation + writer seam + wire `main.rs`

**Files:** `src/lib.rs`, `src/main.rs`, `src/report.rs`.

**Step 1 — RED:** unit-test filename derivation:
```rust
// report_filename(stem, cx) -> "<stem>.CpG_report.txt" / "<stem>.CX_report.txt"
// summary_filename(stem)    -> "<stem>.cytosine_context_summary.txt"
// full path = output_dir + filename (output_dir is "" or ends in '/')
```

**Step 2 — fail.**

**Step 3 — GREEN:**
- `report.rs`: `fn report_filename(stem,cx)`, `fn summary_filename(stem)`, `fn open_report_writer(config) -> io::Result<Box<dyn Write>>` (Phase B: `BufWriter<File>` at `output_dir+report_filename`; **seam** for Phase C gzip/per-chr), `fn open_summary_writer(config)`.
- `lib.rs`: `pub fn run(config: &ResolvedConfig) -> Result<(), BismarkC2cError> { let genome = Genome::load(&config.genome_folder)?; report::run_report(config, &genome) }`.
- `main.rs`: replace the Phase-A stub body of `run()` with `bismark_coverage2cytosine::run(&config)`.

**Step 4 — pass.** **Step 6 — regression.**

---

## Task 8 — byte-identity golden integration test (V15)

**Files:** `tests/data/phase_b/{genome/*.fa, in.cov, generate_goldens.sh, *.golden}`; `tests/golden_phase_b.rs`.

**Step 1 — build fixtures + goldens:**
- Synthetic multi-FASTA `genome/`: `chr1` (CpG at start + interior + last-base edge), `chr2` (an `N` run + CHG/CHH contexts for `--CX`), `scaf_short` (1–2 bp degenerate, for the `i=0`-wrap × `len<3` boundary), `chr3` (uncovered). Hand-built `in.cov` covering chr1/chr2 with a non-round percentage case (e.g. `403/803`).
- `generate_goldens.sh`: for each mode in {default, `--CX`, `--zero_based`, `--coverage_threshold 5`} run `perl ../../../../coverage2cytosine -o <mode> -g genome --dir . in.cov` and save `<mode>.CpG_report.txt`/`.CX_report.txt` + `.cytosine_context_summary.txt` as `*.golden`. (Run once; commit outputs. Re-runnable for provenance.)

**Step 2 — RED** (`tests/golden_phase_b.rs`): for each mode, run `Command::cargo_bin("coverage2cytosine_rs")` with the same flags into a tempdir, then assert the Rust output file is **raw-byte-equal** to the committed golden.

**Step 3 — GREEN:** fixes flow back into report/summary/cov as needed until every mode matches byte-for-byte.

**Step 4:** `cargo test -p bismark-coverage2cytosine --test golden_phase_b` → all modes green.

---

## Task 9 — Final verification
```
cd /Users/fkrueger/Github/Bismark-c2c/rust
cargo fmt -p bismark-coverage2cytosine
cargo clippy -p bismark-coverage2cytosine --all-targets -- -D warnings   # clean
cargo test -p bismark-coverage2cytosine                                  # all green (unit + golden)
cargo build                                                              # workspace builds; siblings untouched
git -C /Users/fkrueger/Github/Bismark-c2c status --short                 # only c2c crate + plans
```
Update PLAN implementation-notes + iteration log; flip PROGRESS Phase B → ✅ contingent on plan-manager.

## Commit plan
On `rust/coverage2cytosine` (stacks onto PR #892 per the EPIC integration model, unless per-phase PRs chosen):
```
feat(c2c): Phase B — core genome-wide cytosine report

cov.rs (gz-aware parse), report.rs (perl_substr/revcomp/classify + per-position
kernel + streaming run_report incl. non-contiguous re-flush + uncovered pass),
summary.rs (cytosine_context_summary). Byte-identical to Perl v0.25.1 on the
synthetic golden matrix {CpG, --CX, --zero_based, --threshold}.
```
Stage: `rust/bismark-coverage2cytosine/**`, `plans/05292026_bismark-coverage2cytosine/**`.
