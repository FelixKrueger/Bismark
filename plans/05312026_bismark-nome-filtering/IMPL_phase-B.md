# IMPL — Phase B (TDD): the core per-read NOMe filter + always-gzipped output

**Source plan:** `plans/05312026_bismark-nome-filtering/SPEC.md` (rev 1), Phase B (§5, §6, §8, §9-consumption, §11B, §12, §13). Goal: the byte-identity heart — stream the `--yacht` input, group by consecutive ReadID, run `cytosine_lookup` per suitable read with the exact Perl arithmetic + NOMe filters, and write the always-gzipped `.manOwar.txt.gz` report (header-first, per the D4 empty-input contract).

**Mode:** TDD (RED → GREEN → REFACTOR). Rust/cargo.

> ⚠️ **Sandbox:** worktree `~/Github/Bismark-nome` is OUTSIDE the command sandbox. Every `cargo`/`perl`/`git` Bash call needs `dangerouslyDisableSandbox: true`.

> **Phase A is done.** Reuse: `substr::perl_substr` (§9), `filename::derive_manowar_name`, `cli::ResolvedConfig{genome_folder,input_path,output_path,output_dir}`, `error::BismarkNomeError` (incl. the already-declared `EmptyInput`), `bismark_io::genome::Genome::{load,get,len}`. The Phase-A `run()` in `lib.rs` carries a D4-reserving comment at the exact insertion point — this phase restructures `run()` there.

> **Carry-forwards from Phase-A code review:** (a) the D4 header-before-loop ordering is **Task 6** (core); (b) absolute-`--dir` handling is already done+tested in Phase A — no new work; (c) the "no output in Phase A" idea is **superseded** by the Phase-B output goldens.

---

## Plan coverage checklist

| # | Plan item (SPEC §) | Task(s) |
|---|--------------------|---------|
| 1 | `revcomp` via `tr/ACTG/TGAC/` — A↔T, C↔G, identity on `N`/other (P3) | T1 |
| 2 | Context classification `^CG`→CG / `^C.G$`→CHG / `^C..$`→CHH / else→skip; `CNG`→CHG, `CNN`→CHH, unclassifiable (P-classify) | T2 |
| 3 | `cytosine_lookup`: byte-scan (NOT regex, B-A-alt2), `pos=i+1` (P4), fwd-C `tri=ext[pos+1..],up=ext[pos..]`, rev-G `tri=ext[pos-1..]+revcomp, up=ext[pos..]+revcomp`, `len<3` skip, genomic key `g=pos+offset-1` | T3 |
| 4 | NOMe filter + tally keyed on col-2 `state` (P5): CG⇒`{z,Z}`+`up∈{ACG,TCG}` (explicit `next` on fail); CHG⇒`{x,X}`+`up^GC`; CHH⇒`{h,H}`+`up^GC` (CHG/CHH fall through, no `next` — A-I3) | T3 |
| 5 | Yacht parse: 8 TAB fields, `^Bismark` skip, gz-aware input (`MultiGzDecoder`) | T4 |
| 6 | Consecutive-ReadID grouping; first line sets `start/end/chr`; per-read map keyed by col-4 genomic pos; flush-on-change + EOF flush; shared flush routine FLUSHES ONLY, seed in loop body (P17) | T4 |
| 7 | Same-position-within-read = **last wins** (unconditional insert, not `or_insert`) (P13) | T4 |
| 8 | Length calc; suitability guard uses `last_start` for BOTH strands (P2); unknown-chr ⇒ `chr_len=0` ⇒ skip | T5 |
| 9 | seq/ext extraction via `perl_substr` fwd (`start-1`/`start-3`) & rev (`end-1`/`end-3`); reverse `end∈{1,2}` ⇒ all-zero line; forward `start≤3` ⇒ NO line (P1); guard `>=` boundary | T5 |
| 10 | Output: `GzEncoder<BufWriter<File>>` + `Compression::default()`; header FIRST; data line `id\tchr\toffset\tend\t<4 counts>\n` with `offset/end` ASCENDING (P9); `finish()` | T6 |
| 11 | **D4/P11**: header written before the read loop; empty/all-`^Bismark` ⇒ `EmptyInput` AFTER `finish()` ⇒ header-only `.gz` on disk + non-zero exit | T6 |
| 12 | `run()` restructure (validate+dir+infile-exists+genome from Phase A, then writer+header+`per_read_filtering`+finish); `pub mod nome` in lib.rs | T6 |
| 13 | `generate_goldens.sh` (repo Perl v0.25.1) + tiny synthetic genome + `.txt`/`.txt.gz` yacht fixtures | T7 |
| 14 | Golden matrix (decompress-then-`assert_eq!`, emission order, un-sorted — A-I9): ACG/TCG accept, GCG reject, GpC in CHG/CHH, `^Bismark` skip, gz round-trip | T8 |
| 15 | Edge integration: VS-edge (fwd `start≤3` / rev `end∈{1,2}`), VS-empty (header-only gz + exit 1), VS-N, VS-guard, VS-pad, VS-crlf, unknown-chr skip, non-consecutive same-ReadID | T9 |

## Test infrastructure
- Unit tests in `src/nome.rs` (`#[cfg(test)] mod tests`) — pure helpers + `cytosine_lookup`/`process_read`/`per_read_filtering` driven with in-memory `Cursor` readers, `Vec<u8>` writers, and tiny inline `Genome`s (build via `tempfile` + `Genome::load`, or expose a `#[cfg(test)]` constructor — see Task 3 note).
- Integration goldens in `tests/golden_phase_b.rs` using `assert_cmd` + `flate2::read::MultiGzDecoder` (decompress-then-compare), mirroring `bismark-coverage2cytosine/tests/golden_phase_b.rs`.
- Fixtures under `tests/data/phase_b/`: `genome/` (tiny multi-FASTA), `*.yacht.txt` / `*.yacht.txt.gz`, `*.golden` (decompressed Perl output), `generate_goldens.sh`.

**Test-data note:** all fixtures are synthetic and hand-built (no external data); goldens come from the repo's macOS-runnable Perl `./NOMe_filtering` via `generate_goldens.sh`. No user input needed.

**⚠️ SPEC ambiguities flagged for the implementer:**
- **A1 — genome-test constructor.** `cytosine_lookup`/`process_read` need a `Genome` (or a raw `&[u8]` chr seq) in unit tests. `Genome` has no public constructor from bytes. *Decision:* unit-test `cytosine_lookup` against a **raw `&[u8]` seq/ext_seq** (it never needs the `Genome` type — it takes `seq`/`ext_seq` slices); unit-test `process_read` by building a real on-disk genome via `Genome::load(tempdir, &[".fa"])`. Do NOT add a public `Genome::from_bytes` just for tests.
- **A2 — `cytosine_lookup` signature.** It writes the output line itself (mirrors Perl `print CYT`). Signature: `fn cytosine_lookup<W: Write>(w: &mut W, id: &[u8], chr: &[u8], seq: &[u8], offset: u32, end: u32, ext_seq: &[u8], read: &HashMap<u32,(u8,u8)>) -> io::Result<()>`. The 4 counts are locals; the line is the only output.
- **A3 — integer width.** Parse `pos/start/end` as `u32` (matches the genome `u32` guard). For `perl_substr` offsets compute as `isize` (`start as isize - 3`). `length` as `usize`. Genomic key `g = pos + offset - 1` as `u32` (all ≥1 in practice).
- **A4 — malformed yacht line.** Perl `split /\t/` is lenient. *Decision:* a line with `<8` fields after `^Bismark`-skip → skip defensively (do not panic); a non-numeric `pos/start/end` → skip the line (accepted divergence, cannot occur on real `--yacht` output). Document in code; pin with a unit test. (Confirm with Felix if strict-error is preferred — default is skip.)

---

## Task 1 — `revcomp` + `complement_base` (P3)

**Files:** New `src/nome.rs` (create the module + this helper); `src/lib.rs` add `pub mod nome;`.

**Step 1: failing test** (in `nome.rs`)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complement_maps_actg_passes_other() {
        assert_eq!(complement_base(b'A'), b'T');
        assert_eq!(complement_base(b'T'), b'A');
        assert_eq!(complement_base(b'C'), b'G');
        assert_eq!(complement_base(b'G'), b'C');
        assert_eq!(complement_base(b'N'), b'N'); // identity
        assert_eq!(complement_base(b'R'), b'R'); // IUPAC passes through
    }
    #[test]
    fn revcomp_reverses_then_complements() {
        assert_eq!(revcomp(b"CGA"), b"TCG"); // reverse AGC → complement TCG
        assert_eq!(revcomp(b"ACGTN"), b"NACGT"); // N stays, order reversed+comp
    }
}
```

**Step 2: run, expect fail** — `nome` module / `revcomp` undefined.
```bash
cargo test -p bismark-nome-filtering nome::tests::revcomp   # dangerouslyDisableSandbox: true
```

**Step 3: implement** (`src/nome.rs` top)
```rust
//! The per-read NOMe-Seq filtering pipeline — the byte-identity crux
//! (Perl `per_read_filtering:48-230` + `cytosine_lookup:242-391`).
use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use bismark_io::genome::Genome;
use crate::error::BismarkNomeError;
use crate::substr::perl_substr;

/// Perl `tr/ACTG/TGAC/`: A↔T, C↔G; every other byte (incl. `N`) is identity.
#[must_use]
fn complement_base(b: u8) -> u8 {
    match b {
        b'A' => b'T',
        b'T' => b'A',
        b'C' => b'G',
        b'G' => b'C',
        other => other,
    }
}

/// Reverse a slice then complement each base (Perl `reverse` + `tr/ACTG/TGAC/`).
#[must_use]
fn revcomp(s: &[u8]) -> Vec<u8> {
    s.iter().rev().map(|&b| complement_base(b)).collect()
}
```

**Step 4–6:** test passes; no refactor; `cargo test -p bismark-nome-filtering nome`.

---

## Task 2 — Context classification (P-classify)

**Files:** `src/nome.rs`.

**Step 1: failing test**
```rust
    #[test]
    fn classify_contexts() {
        assert_eq!(classify(b"CGA"), Some(Context::Cg));  // ^CG
        assert_eq!(classify(b"CGG"), Some(Context::Cg));  // ^CG (CGG still CG)
        assert_eq!(classify(b"CAG"), Some(Context::Chg)); // ^C.G$
        assert_eq!(classify(b"CNG"), Some(Context::Chg)); // N middle → CHG
        assert_eq!(classify(b"CAT"), Some(Context::Chh)); // ^C..$
        assert_eq!(classify(b"CNN"), Some(Context::Chh)); // → CHH
        assert_eq!(classify(b"NCG"), None);               // doesn't start with C → warn-skip
        assert_eq!(classify(b"GCA"), None);               // not C-led
    }
```

**Step 2: run, expect fail.**

**Step 3: implement** — order matters (CG → CHG → CHH → else):
```rust
/// Cytosine context on a 3-byte (5'→3') trinucleotide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Context { Cg, Chg, Chh }

/// Classify per Perl `:291-303` (byte-level). Returns `None` for the Perl
/// `else { warn; next }` branch (a tri not starting with `C`, etc.).
fn classify(tri: &[u8]) -> Option<Context> {
    if tri.len() != 3 { return None; }       // caller already skips len<3, defensive
    if &tri[0..2] == b"CG" { return Some(Context::Cg); }      // ^CG
    if tri[0] == b'C' && tri[2] == b'G' { return Some(Context::Chg); } // ^C.G$
    if tri[0] == b'C' { return Some(Context::Chh); }          // ^C..$
    None
}
```

**Step 4–6:** pass; no refactor; regression.

---

## Task 3 — `cytosine_lookup`: scan + extract + classify + NOMe filter + tally + write (P4, P3, P5, A-I3)

**Files:** `src/nome.rs`.

**Step 1: failing tests** — drive with raw `seq`/`ext_seq` + a `read` map; capture the written line. Build `ext_seq` as `seq` with 2 bp pad each side (so `ext[i+2] == seq[i]`).
```rust
    use std::collections::HashMap;
    // helper: run cytosine_lookup, return the single written line (no trailing \n).
    fn lookup_line(seq: &[u8], ext: &[u8], offset: u32, end: u32, calls: &[(u32,u8,u8)]) -> String {
        let mut read: HashMap<u32,(u8,u8)> = HashMap::new();
        for &(pos,state,call) in calls { read.insert(pos,(state,call)); }
        let mut out = Vec::new();
        cytosine_lookup(&mut out, b"rid", b"chr1", seq, offset, end, ext, &read).unwrap();
        String::from_utf8(out).unwrap().trim_end().to_string()
    }

    #[test]
    fn cg_acg_methylated_counts_meth_cg() {
        // genome window seq = "ACGT", with 2bp pad each side: ext = "TT" + "ACGT" + "AA".
        // The forward C is at seq idx1 (pos=2). ext[pos+1=3..6] = tri; need tri="CGT".
        // Build ext so ext[3..6]="CGT" and ext[2..5] (upstream)="ACG".
        let seq = b"ACGT";
        let ext = b"TTACGTAA";              // idx: T T A C G T A A
        // C@seq idx1 → pos2; genomic g = pos+offset-1 = 2+offset-1.
        // offset=1 → g=2. Stored call 'Z' (methylated CpG), state '+'.
        let line = lookup_line(seq, ext, 1, 4, &[(2, b'+', b'Z')]);
        // meth_CG=1, others 0.
        assert_eq!(line, "rid\tchr1\t1\t4\t1\t0\t0\t0");
    }

    #[test]
    fn cg_gcg_upstream_rejected_no_count() {
        // Same C but upstream = "GCG" (G before C) → NOMe rejects → all zero.
        let seq = b"ACGT";
        let ext = b"TGCGTA";  // upstream ext[pos..pos+3] for pos2 = ext[2..5]="CGT"? -> craft so upstream="GCG"
        // (Implementer: craft ext so upstream != ACG/TCG; assert all-zero line.)
        let line = lookup_line(seq, ext, 1, 4, &[(2, b'+', b'Z')]);
        assert_eq!(line, "rid\tchr1\t1\t4\t0\t0\t0\t0");
    }

    #[test]
    fn tally_keys_on_state_not_call_case() {
        // call 'z' (lowercase) but state '-' → unmeth_CG (state drives the tally).
        let seq = b"ACGT";
        let ext = b"TTACGTAA";
        let line = lookup_line(seq, ext, 1, 4, &[(2, b'-', b'z')]);
        assert_eq!(line, "rid\tchr1\t1\t4\t0\t1\t0\t0");
    }

    #[test]
    fn gpc_chh_counts_meth_gc() {
        // A G-strand C in GpC context with call 'H'/state '+' → meth_nonCG (col 7).
        // Implementer crafts seq/ext so a 'G' scan position yields context CHH and
        // upstream (revcomp) starts "GC"; assert col-7 (meth_GC) == 1.
    }
```
*(The exact `ext` bytes for each case are for the implementer to craft so the documented `tri`/`upstream` result; the asserted output lines are the contract. Add ≥1 case each for: CG-ACG accept (+/−), CG-TCG accept, CG-GCG reject, CHG-GpC, CHH-GpC, CHG/CHH non-GpC (no count), a G-strand (reverse) hit, and a position NOT in the read map (no count).)*

**Step 2: run, expect fail.**

**Step 3: implement** (`src/nome.rs`)
```rust
/// Port of Perl `cytosine_lookup:242-391`. Scans `seq` for C/G, classifies each,
/// applies the NOMe filter against the read's stored calls, and writes ONE
/// output line: `id\tchr\toffset\tend\tmeth_CG\tunmeth_CG\tmeth_nonCG\tunmeth_nonCG\n`.
/// `offset`/`end` are passed already-ascending by the caller.
fn cytosine_lookup<W: Write>(
    w: &mut W, id: &[u8], chr: &[u8], seq: &[u8],
    offset: u32, end: u32, ext_seq: &[u8], read: &HashMap<u32,(u8,u8)>,
) -> io::Result<()> {
    let (mut meth_cg, mut unmeth_cg, mut meth_ncg, mut unmeth_ncg) = (0u32,0u32,0u32,0u32);
    for (i, &b) in seq.iter().enumerate() {
        let is_c = b == b'C';
        let is_g = b == b'G';
        if !is_c && !is_g { continue; }
        let pos = i + 1;                       // Perl pos() (P4)
        let (tri, upstream): (Vec<u8>, Vec<u8>) = if is_c {
            ( perl_substr(ext_seq, (pos + 1) as isize, 3).to_vec(),
              perl_substr(ext_seq, pos as isize, 3).to_vec() )
        } else {
            ( revcomp(perl_substr(ext_seq, (pos as isize) - 1, 3)),
              revcomp(perl_substr(ext_seq, pos as isize, 3)) )
        };
        if tri.len() < 3 { continue; }         // edge (Perl :287)
        let ctx = match classify(&tri) { Some(c) => c, None => {
            eprintln!("The sequence context could not be determined (found: '{}'). Skipping.",
                      String::from_utf8_lossy(&tri));
            continue;
        }};
        let g = pos as u32 + offset - 1;       // genomic 1-based (P4)
        let Some(&(state, call)) = read.get(&g) else { continue; };
        match ctx {
            Context::Cg => if call == b'z' || call == b'Z' {
                if upstream == b"ACG" || upstream == b"TCG" {
                    match state { b'+' => meth_cg += 1, b'-' => unmeth_cg += 1, _ => {} }
                }   // else: Perl `next` (skip) — no-op here, loop continues
            },
            Context::Chg => if call == b'x' || call == b'X' {
                if upstream.len() >= 2 && &upstream[0..2] == b"GC" {
                    match state { b'+' => meth_ncg += 1, b'-' => unmeth_ncg += 1, _ => {} }
                }   // CHG/CHH: no explicit `next` — fall through (A-I3)
            },
            Context::Chh => if call == b'h' || call == b'H' {
                if upstream.len() >= 2 && &upstream[0..2] == b"GC" {
                    match state { b'+' => meth_ncg += 1, b'-' => unmeth_ncg += 1, _ => {} }
                }
            },
        }
    }
    writeln!(w, "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        String::from_utf8_lossy(id), String::from_utf8_lossy(chr),
        offset, end, meth_cg, unmeth_cg, meth_ncg, unmeth_ncg)
}
```
*(Note: emit raw bytes if `id`/`chr` may be non-UTF-8 — prefer `w.write_all(id)?; w.write_all(b"\t")?; …` to avoid `from_utf8_lossy` altering bytes. Implementer: use byte-writes for `id`/`chr` to stay byte-faithful; the integer fields are ASCII.)*

**Step 4–6:** tests pass; refactor to byte-writes for id/chr; regression.

---

## Task 4 — `per_read_filtering`: parse + group + flush (P5-skip, P13, P17)

**Files:** `src/nome.rs`.

**Step 1: failing tests** — drive with a `Cursor` reader + `Vec<u8>` writer + an on-disk tiny genome.
```rust
    use std::io::Cursor;
    fn run_pipeline(genome_fa: &str, yacht: &str) -> String {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("chr1.fa"), genome_fa).unwrap();
        let genome = Genome::load(t.path(), &[".fa", ".fasta"]).unwrap();
        let mut out = Vec::new();
        // header is written by run(); per_read_filtering writes only data lines:
        per_read_filtering(Cursor::new(yacht.as_bytes()), &genome, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn skips_bismark_header_and_groups_consecutive() {
        // 2 reads; assert 2 data lines, in input order.
    }
    #[test]
    fn non_consecutive_same_id_is_two_reads() {
        // id A, id B, id A → THREE output lines (A flushed twice). (P10)
    }
    #[test]
    fn same_position_within_read_last_wins() {
        // one read, two calls at the same genomic pos: +Z then -z → tally reflects
        // the LAST (state '-') → unmeth. (P13)
    }
    #[test]
    fn empty_or_all_bismark_input_errors_empty() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("chr1.fa"), ">chr1\nACGTACGT\n").unwrap();
        let genome = Genome::load(t.path(), &[".fa"]).unwrap();
        let mut out = Vec::new();
        let err = per_read_filtering(Cursor::new(&b"Bismark header only\n"[..]), &genome, &mut out).unwrap_err();
        assert!(matches!(err, BismarkNomeError::EmptyInput));
    }
```

**Step 2: run, expect fail.**

**Step 3: implement** — the streaming loop. Group consecutive ReadID; flush via `process_read` (Task 5); seed in the loop body (NOT in the flush routine, P17); EOF flush; `EmptyInput` if no read ever seen.
```rust
/// Stream the yacht input, group consecutive same-ReadID calls, and write one
/// output line per suitable read (Perl `per_read_filtering`). Returns
/// `EmptyInput` if no data line was ever read (Perl `:173-175`).
pub fn per_read_filtering<R: BufRead, W: Write>(
    reader: R, genome: &Genome, w: &mut W,
) -> Result<(), BismarkNomeError> {
    let mut last: Option<(Vec<u8>, Vec<u8>, u32, u32)> = None; // (id, chr, start, end)
    let mut read: HashMap<u32,(u8,u8)> = HashMap::new();
    for line in reader.lines() {
        let line = line?;
        if line.starts_with("Bismark") { continue; }            // :91
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 8 { continue; }                            // A4: defensive skip
        let (id, state, chr, pos, call, start, end) =
            (f[0].as_bytes(), f[1].as_bytes(), f[2].as_bytes(), f[3], f[4].as_bytes(), f[5], f[6]);
        let (pos, start, end) = match (pos.parse::<u32>(), start.parse::<u32>(), end.parse::<u32>()) {
            (Ok(p), Ok(s), Ok(e)) => (p, s, e),
            _ => continue,                                       // A4: non-numeric → skip
        };
        let state_b = state.first().copied().unwrap_or(b'?');
        let call_b = call.first().copied().unwrap_or(b'?');
        match &last {
            Some((lid, ..)) if lid.as_slice() == id => {
                read.insert(pos, (state_b, call_b));             // same read (P13 last-wins)
            }
            _ => {
                if let Some((lid, lchr, lstart, lend)) = last.take() {
                    process_read(w, genome, &lid, &lchr, lstart, lend, &read)?; // flush prev (P17: flush only)
                }
                read.clear();
                read.insert(pos, (state_b, call_b));             // seed new read (in loop body)
                last = Some((id.to_vec(), chr.to_vec(), start, end));
            }
        }
    }
    match last {
        Some((lid, lchr, lstart, lend)) => process_read(w, genome, &lid, &lchr, lstart, lend, &read)?, // EOF flush
        None => return Err(BismarkNomeError::EmptyInput),        // :173-175
    }
    Ok(())
}
```
*(`reader.lines()` yields `String`; a CRLF line keeps a trailing `\r` on the last field — harmless, `end.parse::<u32>()` would fail on `"40\r"`. ⚠️ VS-crlf: to stay Perl-faithful (Perl `chomp` strips only `\n`, leaving `\r` on col-8 which is unused), strip a trailing `\r` from the WHOLE line is WRONG (Perl doesn't). Instead parse is on cols 4/6/7 which never carry the `\r` (col-8 is last). So no `\r` handling needed — but pin it with VS-crlf in Task 9. If a numeric field DID carry `\r`, the A4 skip would wrongly drop the line; confirm the fixture shows col-8 is the only `\r` carrier.)*

**Step 4–6:** depends on Task 5 (`process_read`) — implement T5 first or stub `process_read` to compile. Order T5 before T4's GREEN. Tests pass; regression.

---

## Task 5 — `process_read`: length + suitability guard + extraction + edge asymmetry (P1, P2)

**Files:** `src/nome.rs`.

**Step 1: failing tests** (via `run_pipeline` from Task 4, with crafted genome + yacht)
```rust
    #[test]
    fn forward_read_start_le_3_emits_no_line() {
        // genome chr1 length ≥ 20; a forward read with start=2 → guard fails → NO line.
        let out = run_pipeline(">chr1\nACGTACGTACGTACGTACGT\n",
            "r1\t+\tchr1\t2\tz\t2\t10\t+\n");
        assert_eq!(out, "");  // no data line
    }
    #[test]
    fn reverse_read_end_1_emits_all_zero_line() {
        // reverse read: start(col6)=rightmost=10, end(col7)=leftmost=1 → last_end=1.
        // Guard passes (last_start=10); ext_seq degenerate → all tri len<3 → all-zero.
        let out = run_pipeline(">chr1\nACGTACGTACGTACGTACGT\n",
            "r1\t-\tchr1\t10\tz\t10\t1\t-\n");
        assert_eq!(out, "r1\tchr1\t1\t10\t0\t0\t0\t0\n"); // offset/end ascending = 1,10
    }
    #[test]
    fn guard_ge_boundary_suitable_and_one_less_not() {
        // craft chr_len == last_start-2+length+4 (suitable, emits) and chr_len-1 (no line).
    }
    #[test]
    fn unknown_chromosome_emits_nothing() {
        let out = run_pipeline(">chr1\nACGTACGT\n", "r1\t+\tchrZ\t4\tz\t4\t8\t+\n");
        assert_eq!(out, "");
    }
```

**Step 2: run, expect fail.**

**Step 3: implement**
```rust
/// Perl per-read flush (`:116-168` / `:177-219`). FLUSH ONLY — never seeds the
/// next read (P17). Computes length, applies the suitability guard (which uses
/// `start` for BOTH strands — P2), extracts seq/ext_seq via `perl_substr`, and
/// calls `cytosine_lookup` with ascending `offset`/`end`.
fn process_read<W: Write>(
    w: &mut W, genome: &Genome, id: &[u8], chr: &[u8], start: u32, end: u32,
    read: &HashMap<u32,(u8,u8)>,
) -> io::Result<()> {
    let length: usize = if end >= start { (end - start + 1) as usize } else { (start - end + 1) as usize };
    let chr_seq: &[u8] = genome.get(chr).unwrap_or(&[]);   // unknown chr → empty → guard fails
    let chr_len = chr_seq.len();
    // Guard (Perl :132): (start-2 > 1) && (chr_len >= start-2 + length + 4). Use i64 to
    // avoid underflow when start < 2 (then start-2 wraps in Perl numeric → still > 1 false).
    let suitable = (start as i64 - 2 > 1)
        && (chr_len as i64 >= start as i64 - 2 + length as i64 + 4);
    if !suitable { return Ok(()); }
    let (seq, ext, offset, hi) = if end >= start {
        ( perl_substr(chr_seq, start as isize - 1, length),
          perl_substr(chr_seq, start as isize - 3, length + 4), start, end )
    } else {
        ( perl_substr(chr_seq, end as isize - 1, length),
          perl_substr(chr_seq, end as isize - 3, length + 4), end, start )
    };
    cytosine_lookup(w, id, chr, seq, offset, hi, ext, read)
}
```
*(⚠️ `perl_substr` returns `&[u8]` borrowing `chr_seq`; `seq`/`ext` borrows are fine since `chr_seq` outlives the call. The reverse `end as isize - 3` goes negative for `end∈{1,2}` → `perl_substr` reads from the chromosome end → degenerate `ext` → all `tri.len()<3` → all-zero line. This is the P1 contract; the test pins it.)*

**Step 4–6:** Task 4 + Task 5 tests pass together; regression `cargo test -p bismark-nome-filtering nome`.

---

## Task 6 — Output wiring + `run()` restructure (D4/P11, P9) + `pub mod nome`

**Files:** `src/lib.rs` (restructure `run()`, add `pub mod nome;`), `src/nome.rs` (header constant + a `write_report` entry that opens the writer).

**Step 1: failing test** (integration — empty input leaves a header-only gz)
```rust
// tests/golden_phase_b.rs (new) — see Task 9 for the full VS-empty test.
```
plus a `lib.rs` unit asserting the header constant bytes:
```rust
    #[test]
    fn header_line_bytes() {
        assert_eq!(crate::nome::HEADER,
            b"ReadID\tChr\tStart\tEnd\tmeth_CG\tunmeth_CG\tmeth_GC\tunmeth_GC\n");
    }
```

**Step 2: run, expect fail.**

**Step 3: implement** — in `nome.rs`:
```rust
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// The always-gzipped report header (Perl `:78`). Columns 7/8 are `meth_GC`/
/// `unmeth_GC` (the non-CG GpC tallies) — do NOT rename.
pub(crate) const HEADER: &[u8] =
    b"ReadID\tChr\tStart\tEnd\tmeth_CG\tunmeth_CG\tmeth_GC\tunmeth_GC\n";

/// Open the gzip writer at `output_path`, write the header FIRST (D4), stream
/// the per-read filtering, then `finish()` the encoder — so an empty input
/// still leaves a header-only `.gz` on disk before `EmptyInput` propagates.
pub fn write_report(input_path: &Path, output_path: &Path, genome: &Genome)
    -> Result<(), BismarkNomeError>
{
    let file = File::create(output_path)?;
    let mut enc = GzEncoder::new(BufWriter::new(file), Compression::default());
    enc.write_all(HEADER)?;                                   // header BEFORE the loop (D4)
    // gz-aware input open:
    let infile = File::open(input_path)?;
    let is_gz = input_path.file_name().and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".gz"));
    let result = if is_gz {
        per_read_filtering(BufReader::new(flate2::read::MultiGzDecoder::new(infile)), genome, &mut enc)
    } else {
        per_read_filtering(BufReader::new(infile), genome, &mut enc)
    };
    enc.finish()?;                                            // finish so header-only .gz is valid
    result                                                    // propagate EmptyInput AFTER finish
}
```
and restructure `lib.rs::run()` (replace the Phase-A `eprintln!` + reserving comment):
```rust
    let genome = bismark_io::genome::Genome::load(&cfg.genome_folder, &[".fa", ".fasta"])?;
    eprintln!("Stored sequence information of {} chromosomes/scaffolds in total", genome.len());
    crate::nome::write_report(&cfg.input_path, &cfg.output_path, &genome)
```
*(`use std::io::BufReader;` in nome.rs. The Phase-A `run()` test that asserted no output is removed/replaced by Task 9 goldens.)*

**Step 4: run** — VS-empty (Task 9) + header unit pass.
**Step 5: refactor** — none. **Step 6:** regression.

---

## Task 7 — Fixtures + `generate_goldens.sh`

**Files (new):** `tests/data/phase_b/genome/chr1.fa` (+ a 2nd small chr + a short scaffold + an `N` run), `tests/data/phase_b/*.yacht.txt` (and a `.gz` copy), `tests/data/phase_b/generate_goldens.sh`.

**Step 1:** Hand-build a tiny genome exercising: a CpG mid-sequence in `ACG`/`TCG` and in `GCG` (reject) context; a GpC for CHG/CHH; a CpG at the literal end (VS-pad); a short scaffold; an `N` run (VS-N). Hand-build yacht inputs (8-col, single-end) whose calls line up with those genomic positions, including: a forward read with `start≤3`, a reverse read with `end∈{1,2}` (col6>col7), a read on an unknown chr, a non-consecutive same-ReadID triple, a `^Bismark` header line, and a CRLF variant.

**Step 2:** `generate_goldens.sh` (mirror c2c's), running the repo Perl:
```bash
#!/usr/bin/env bash
set -eo pipefail
NOME="$(cd "$(dirname "$0")/../../../../.." && pwd)/NOMe_filtering"
# main matrix: run Perl, gunzip the .manOwar.txt.gz → commit decompressed .golden
for case in main edge ncontext pad crlf unknownchr noncontig; do
  perl "$NOME" -g genome --dir . "${case}.yacht.txt" >/dev/null 2>&1 || true  # edge/empty may exit nonzero
  if [ -f "${case}.manOwar.txt.gz" ]; then
    gunzip -c "${case}.manOwar.txt.gz" > "${case}.golden"
    rm -f "${case}.manOwar.txt.gz"
  fi
done
# empty: header-only artifact (Perl writes header then dies); snapshot decompressed header.
perl "$NOME" -g genome --dir . empty.yacht.txt >/dev/null 2>&1 || true
gunzip -c empty.manOwar.txt.gz > empty.golden && rm -f empty.manOwar.txt.gz
# gz input parity: same content from a gzipped yacht input.
gzip -kf main.yacht.txt   # → main.yacht.txt.gz
```
Run it once (`dangerouslyDisableSandbox: true`), commit the `.golden` files. **Verify the committed goldens visually** (the Perl output is the spec).

```bash
cd /Users/fkrueger/Github/Bismark-nome/rust/bismark-nome-filtering/tests/data/phase_b && bash generate_goldens.sh   # dangerouslyDisableSandbox: true
```

---

## Task 8 — Golden matrix tests (decompress-then-compare; emission order, un-sorted)

**Files:** `tests/golden_phase_b.rs`.

**Step 1: tests** (mirror c2c harness; decompress the Rust `.manOwar.txt.gz` and compare to the plain `.golden`):
```rust
use std::path::{Path, PathBuf};
use std::io::Read;
use assert_cmd::Command;
use flate2::read::MultiGzDecoder;

fn data() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/phase_b") }
fn gunzip(p: &Path) -> Vec<u8> { let mut d = MultiGzDecoder::new(std::fs::File::open(p).unwrap()); let mut v = Vec::new(); d.read_to_end(&mut v).unwrap(); v }

/// Copy a yacht fixture into a tempdir, run the binary with --dir there, return
/// the decompressed output bytes.
fn run_case(yacht: &str) -> Vec<u8> {
    let d = data();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::copy(d.join(yacht), tmp.path().join(yacht)).unwrap();
    Command::cargo_bin("NOMe_filtering_rs").unwrap()
        .arg("-g").arg(d.join("genome")).arg("--dir").arg(tmp.path()).arg(yacht)
        .assert().success();
    let stem = yacht.strip_suffix(".gz").unwrap_or(yacht).strip_suffix(".txt").unwrap();
    gunzip(&tmp.path().join(format!("{stem}.manOwar.txt.gz")))
}

#[test]
fn golden_main() {
    assert_eq!(run_case("main.yacht.txt"), std::fs::read(data().join("main.golden")).unwrap());
}
#[test]
fn golden_gz_input_matches_plain() {
    // .txt.gz input → same decompressed output as the .txt input (gz-aware read).
    assert_eq!(run_case("main.yacht.txt.gz"), std::fs::read(data().join("main.golden")).unwrap());
}
```
*(Add `golden_ncontext`, `golden_pad`, `golden_noncontig`, `golden_unknownchr` the same way. Each asserts raw-byte equality to the Perl golden — emission order, un-sorted.)*

**Step 2–6:** run; expect pass against the committed goldens; regression.

---

## Task 9 — Edge integration tests (VS-edge, VS-empty/D4, VS-N, VS-guard, VS-crlf)

**Files:** `tests/golden_phase_b.rs`.

```rust
#[test]
fn vs_edge_forward_start_le_3_no_line_reverse_end_1_all_zero() {
    // The `edge.yacht.txt` fixture has BOTH a forward start≤3 read and a reverse
    // end∈{1,2} read; the golden encodes "no line for the former, all-zero for
    // the latter." Assert byte-equality to edge.golden (named, in one fixture).
    assert_eq!(run_case("edge.yacht.txt"), std::fs::read(data().join("edge.golden")).unwrap());
}

#[test]
fn vs_empty_leaves_header_only_gz_and_exits_nonzero() {
    // D4/P11: empty (or all-^Bismark) input → exit 1, but a header-only .gz lands.
    let d = data();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("empty.yacht.txt"), "").unwrap();
    Command::cargo_bin("NOMe_filtering_rs").unwrap()
        .arg("-g").arg(d.join("genome")).arg("--dir").arg(tmp.path()).arg("empty.yacht.txt")
        .assert().failure().code(1);
    let got = gunzip(&tmp.path().join("empty.manOwar.txt.gz"));
    assert_eq!(got, std::fs::read(d.join("empty.golden")).unwrap()); // == the header line only
}

#[test]
fn vs_crlf_yacht_input() {
    // A CRLF yacht input must produce the same decompressed output as its LF twin.
    assert_eq!(run_case("crlf.yacht.txt"), std::fs::read(data().join("main.golden")).unwrap());
}
```
*(Plus: `vs_unknown_chr` (output omits the unknown chr), `vs_noncontiguous_same_readid` (the same-ID-twice read emits two lines). These can assert against committed goldens or inline substring checks like the c2c streaming tests.)*

---

## Final verification
```bash
# all dangerouslyDisableSandbox: true, from rust/
cargo build --workspace
cargo test -p bismark-io
cargo test -p bismark-nome-filtering          # unit + golden_phase_b
cargo clippy -p bismark-nome-filtering -p bismark-io --all-targets -- -D warnings
# spot-check against Perl on a fresh fixture:
perl ../NOMe_filtering -g tests/data/phase_b/genome --dir /tmp/nome_chk tests/data/phase_b/main.yacht.txt
cmp <(gunzip -c /tmp/nome_chk/main.manOwar.txt.gz) <(gunzip -c <rust-output>.manOwar.txt.gz)
```
Expected: workspace builds; all unit + golden tests pass (decompressed bytes == Perl v0.25.1); clippy clean.

## Commit plan
One commit on `rust/nome-filtering` (commit only when Felix asks):
- `feat(nome-filtering): Phase B — core per-read NOMe filter + always-gzipped .manOwar.txt.gz output` — `src/nome.rs`, `src/lib.rs` (run restructure + `pub mod nome`), `tests/golden_phase_b.rs`, `tests/data/phase_b/**`.

## Notes / decisions taken in this plan
- **`cytosine_lookup` writes its own line** (mirrors Perl `print CYT`); byte-writes for `id`/`chr` to stay byte-faithful (A2).
- **Unit-test `cytosine_lookup` against raw `&[u8]`** (no `Genome` needed); `process_read` against an on-disk `Genome::load` (A1 — no test-only `from_bytes`).
- **Malformed yacht lines** (`<8` fields / non-numeric coords) are **skipped defensively** (A4) — cannot occur on real `--yacht` output; pinned by a unit test. *(Flag for Felix: skip vs strict-error.)*
- **D4 ordering** realized via `write_report` (header → loop → `finish()` → propagate `EmptyInput`).
- **CRLF**: no whole-line `\r`-strip (Perl doesn't); only col-8 carries `\r` and is unused, so numeric parses are clean. Pinned by VS-crlf.
