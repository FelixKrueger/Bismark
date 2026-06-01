//! The per-read NOMe-Seq filtering pipeline — the byte-identity crux.
//!
//! Ported from Perl `NOMe_filtering` `per_read_filtering:48-230` (the streaming
//! loop + the in-loop/EOF flush) and `cytosine_lookup:242-391` (the C/G walk +
//! NOMe filter + tally). The output is the always-gzipped `.manOwar.txt.gz`
//! per-read report.
//!
//! Structure (SPEC §8):
//! - [`per_read_filtering`] streams the yacht input, groups consecutive
//!   same-ReadID calls, and flushes each read via [`process_read`] (the SHARED
//!   flush routine — it flushes ONLY; seeding the next read happens in the loop
//!   body, pitfall P17).
//! - [`process_read`] computes the read length, applies the suitability guard
//!   (which uses the larger-or-`start` coordinate for BOTH strands, P2),
//!   extracts the genome window via [`crate::substr::perl_substr`], and calls
//!   [`cytosine_lookup`].
//! - [`cytosine_lookup`] scans the window for C/G, classifies each context,
//!   applies the NOMe ACG/TCG (CpG) and GpC (non-CpG) filters, tallies by the
//!   stored `+`/`-` state, and writes the read's output line.
//! - [`write_report`] opens the gzip writer, writes the header FIRST, runs the
//!   pipeline, and `finish()`es — so an empty input still leaves a header-only
//!   `.gz` on disk before [`BismarkNomeError::EmptyInput`] propagates (D4/P11).

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use bismark_io::genome::Genome;
use flate2::Compression;
use flate2::write::GzEncoder;

use crate::error::BismarkNomeError;
use crate::substr::perl_substr;

/// The always-gzipped report header (Perl `:78`). Columns 7/8 are labelled
/// `meth_GC`/`unmeth_GC` (the non-CG GpC tallies) — do NOT rename.
pub(crate) const HEADER: &[u8] =
    b"ReadID\tChr\tStart\tEnd\tmeth_CG\tunmeth_CG\tmeth_GC\tunmeth_GC\n";

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

/// Cytosine context on a 3-byte (5'→3') trinucleotide (Perl `:291-303`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Context {
    /// `^CG` — CpG.
    Cg,
    /// `^C.G$` — CHG.
    Chg,
    /// `^C..$` — CHH.
    Chh,
}

/// Classify a trinucleotide. `None` is the Perl `else { warn; next }` branch
/// (a tri not starting with `C`, e.g. an `N`-adjacent reverse-complement).
fn classify(tri: &[u8]) -> Option<Context> {
    if tri.len() != 3 {
        return None;
    }
    if &tri[0..2] == b"CG" {
        return Some(Context::Cg); // ^CG
    }
    if tri[0] == b'C' && tri[2] == b'G' {
        return Some(Context::Chg); // ^C.G$
    }
    if tri[0] == b'C' {
        return Some(Context::Chh); // ^C..$
    }
    None
}

/// Port of Perl `cytosine_lookup:242-391`. Scans `seq` for C/G, classifies each
/// position, applies the NOMe filter against the read's stored calls (keyed by
/// the absolute genomic position), tallies by the stored `+`/`-` state, and
/// writes ONE output line:
/// `id\tchr\toffset\tend\tmeth_CG\tunmeth_CG\tmeth_nonCG\tunmeth_nonCG\n`.
/// `offset`/`end` are passed already-ascending by [`process_read`].
///
/// (The 8 parameters mirror Perl's
/// `cytosine_lookup($id,$chr,$seq,$offset,$end,$ext_seq,$read)` plus the output
/// writer — a config struct would only add indirection for a faithful port.)
#[allow(clippy::too_many_arguments)]
fn cytosine_lookup<W: Write>(
    w: &mut W,
    id: &[u8],
    chr: &[u8],
    seq: &[u8],
    offset: u32,
    end: u32,
    ext_seq: &[u8],
    read: &HashMap<u32, (u8, u8)>,
) -> io::Result<()> {
    let (mut meth_cg, mut unmeth_cg, mut meth_ncg, mut unmeth_ncg) = (0u32, 0u32, 0u32, 0u32);

    for (i, &b) in seq.iter().enumerate() {
        let is_c = b == b'C';
        let is_g = b == b'G';
        if !is_c && !is_g {
            continue;
        }
        let pos = i + 1; // Perl pos() — offset just past the matched char (P4).

        // tri_nt + upstream context, exactly as Perl extracts them.
        let (tri, upstream): (Vec<u8>, Vec<u8>) = if is_c {
            (
                perl_substr(ext_seq, (pos + 1) as isize, 3).to_vec(),
                perl_substr(ext_seq, pos as isize, 3).to_vec(),
            )
        } else {
            (
                revcomp(perl_substr(ext_seq, pos as isize - 1, 3)),
                revcomp(perl_substr(ext_seq, pos as isize, 3)),
            )
        };

        if tri.len() < 3 {
            continue; // trinucleotide could not be extracted (edge) — Perl :287
        }
        let ctx = match classify(&tri) {
            Some(c) => c,
            None => {
                eprintln!(
                    "The sequence context could not be determined (found: '{}'). Skipping.",
                    String::from_utf8_lossy(&tri)
                );
                continue;
            }
        };

        // Genomic 1-based position of this C/G; only count if the read covered it.
        let g = pos as u32 + offset - 1;
        let Some(&(state, call)) = read.get(&g) else {
            continue;
        };

        // NOMe filter + tally, keyed on the stored `+`/`-` state. Each context
        // requires (1) the call letter to match the genome context AND (2) the
        // NOMe positional filter to pass: CpG only in A-CG/T-CG context; GpC for
        // the non-CG contexts. (Perl's CG branch does an explicit `next` on a
        // failing upstream while CHG/CHH fall through — behaviourally identical
        // here, since the loop body ends after the tally either way; A-I3.)
        let is_gpc = upstream.len() >= 2 && &upstream[0..2] == b"GC";
        match ctx {
            Context::Cg
                if (call == b'z' || call == b'Z') && (upstream == b"ACG" || upstream == b"TCG") =>
            {
                match state {
                    b'+' => meth_cg += 1,
                    b'-' => unmeth_cg += 1,
                    _ => {}
                }
            }
            Context::Chg if (call == b'x' || call == b'X') && is_gpc => match state {
                b'+' => meth_ncg += 1,
                b'-' => unmeth_ncg += 1,
                _ => {}
            },
            Context::Chh if (call == b'h' || call == b'H') && is_gpc => match state {
                b'+' => meth_ncg += 1,
                b'-' => unmeth_ncg += 1,
                _ => {}
            },
            _ => {}
        }
    }

    // Byte-faithful id/chr (may be non-UTF-8); integer fields are ASCII.
    w.write_all(id)?;
    w.write_all(b"\t")?;
    w.write_all(chr)?;
    writeln!(
        w,
        "\t{offset}\t{end}\t{meth_cg}\t{unmeth_cg}\t{meth_ncg}\t{unmeth_ncg}"
    )
}

/// Perl per-read flush (`:116-168` / `:177-219`). FLUSH ONLY — never seeds the
/// next read (P17). Computes length, applies the suitability guard (which uses
/// `start` — the read's first-line col-6 — for BOTH strands, P2), extracts
/// `seq`/`ext_seq` via [`perl_substr`], and calls [`cytosine_lookup`] with the
/// ascending `offset`/`end`.
fn process_read<W: Write>(
    w: &mut W,
    genome: &Genome,
    id: &[u8],
    chr: &[u8],
    start: u32,
    end: u32,
    read: &HashMap<u32, (u8, u8)>,
) -> io::Result<()> {
    let length: usize = if end >= start {
        (end - start + 1) as usize
    } else {
        (start - end + 1) as usize
    };

    // Unknown chromosome → empty slice → chr_len 0 → guard fails → skip (Perl
    // `length(undef)` → 0 in the numeric guard).
    let chr_seq: &[u8] = genome.get(chr).unwrap_or(&[]);
    let chr_len = chr_seq.len();

    // Perl :132 — uses `start` for BOTH strands. i64 avoids underflow when
    // start < 2 (Perl numeric: start-2 negative → `> 1` false → not suitable).
    let suitable =
        (start as i64 - 2 > 1) && (chr_len as i64 >= start as i64 - 2 + length as i64 + 4);
    if !suitable {
        return Ok(());
    }

    let (seq, ext, offset, hi) = if end >= start {
        (
            perl_substr(chr_seq, start as isize - 1, length),
            perl_substr(chr_seq, start as isize - 3, length + 4),
            start,
            end,
        )
    } else {
        // Reverse read: extract from `end` (the smaller coord). For end ∈ {1,2}
        // `end-3` is negative → perl_substr reads from the chromosome END →
        // degenerate ext → every tri len<3 → all-zero line (P1).
        (
            perl_substr(chr_seq, end as isize - 1, length),
            perl_substr(chr_seq, end as isize - 3, length + 4),
            end,
            start,
        )
    };

    cytosine_lookup(w, id, chr, seq, offset, hi, ext, read)
}

/// Stream the yacht input, group consecutive same-ReadID calls, and write one
/// output line per suitable read (Perl `per_read_filtering`). Returns
/// [`BismarkNomeError::EmptyInput`] if no data line was ever read (Perl
/// `:173-175`) — the caller has already written the header by then (D4).
pub fn per_read_filtering<R: BufRead, W: Write>(
    reader: R,
    genome: &Genome,
    w: &mut W,
) -> Result<(), BismarkNomeError> {
    // (id, chr, start, end) of the read currently being accumulated.
    let mut last: Option<(Vec<u8>, Vec<u8>, u32, u32)> = None;
    let mut read: HashMap<u32, (u8, u8)> = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        if line.starts_with("Bismark") {
            continue; // defensive header skip (Perl :91)
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 8 {
            continue; // malformed line — skip defensively (cannot occur on real --yacht)
        }
        let (pos, start, end) = match (
            f[3].parse::<u32>(),
            f[5].parse::<u32>(),
            f[6].parse::<u32>(),
        ) {
            (Ok(p), Ok(s), Ok(e)) => (p, s, e),
            _ => continue, // non-numeric coords — skip defensively
        };
        let state_b = f[1].as_bytes().first().copied().unwrap_or(b'?');
        let call_b = f[4].as_bytes().first().copied().unwrap_or(b'?');
        let id = f[0].as_bytes();

        match &last {
            Some((lid, ..)) if lid.as_slice() == id => {
                // Same read — accumulate (same-position duplicate: last wins, P13).
                read.insert(pos, (state_b, call_b));
            }
            _ => {
                // New read — flush the previous one (flush ONLY, P17), then
                // reset + seed the new read here in the loop body.
                if let Some((lid, lchr, lstart, lend)) = last.take() {
                    process_read(w, genome, &lid, &lchr, lstart, lend, &read)?;
                }
                read.clear();
                read.insert(pos, (state_b, call_b));
                last = Some((id.to_vec(), f[2].as_bytes().to_vec(), start, end));
            }
        }
    }

    match last {
        Some((lid, lchr, lstart, lend)) => {
            process_read(w, genome, &lid, &lchr, lstart, lend, &read)?; // EOF flush
        }
        None => return Err(BismarkNomeError::EmptyInput), // Perl :173-175
    }
    Ok(())
}

/// Open the gzip writer at `output_path`, write the header FIRST (D4/P11),
/// stream [`per_read_filtering`], then `finish()` the encoder — so an empty /
/// all-`^Bismark` input still leaves a header-only `.gz` on disk before
/// [`BismarkNomeError::EmptyInput`] propagates.
pub fn write_report(
    input_path: &Path,
    output_path: &Path,
    genome: &Genome,
) -> Result<(), BismarkNomeError> {
    let out = File::create(output_path)?;
    let mut enc = GzEncoder::new(BufWriter::new(out), Compression::default());
    enc.write_all(HEADER)?; // header BEFORE the read loop (D4)

    let infile = File::open(input_path)?;
    let is_gz = input_path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".gz"));

    let result = if is_gz {
        per_read_filtering(
            BufReader::new(flate2::read::MultiGzDecoder::new(infile)),
            genome,
            &mut enc,
        )
    } else {
        per_read_filtering(BufReader::new(infile), genome, &mut enc)
    };

    enc.finish()?; // finish so the (possibly header-only) .gz is valid on disk
    result // propagate EmptyInput AFTER finish
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // ── Task 1: revcomp ──────────────────────────────────────────────

    #[test]
    fn complement_maps_actg_passes_other() {
        assert_eq!(complement_base(b'A'), b'T');
        assert_eq!(complement_base(b'T'), b'A');
        assert_eq!(complement_base(b'C'), b'G');
        assert_eq!(complement_base(b'G'), b'C');
        assert_eq!(complement_base(b'N'), b'N');
        assert_eq!(complement_base(b'R'), b'R');
    }

    #[test]
    fn revcomp_reverses_then_complements() {
        assert_eq!(revcomp(b"CGA"), b"TCG");
        assert_eq!(revcomp(b"ACGTN"), b"NACGT");
    }

    // ── Task 2: classify ─────────────────────────────────────────────

    #[test]
    fn classify_contexts() {
        assert_eq!(classify(b"CGA"), Some(Context::Cg));
        assert_eq!(classify(b"CGG"), Some(Context::Cg));
        assert_eq!(classify(b"CAG"), Some(Context::Chg));
        assert_eq!(classify(b"CNG"), Some(Context::Chg));
        assert_eq!(classify(b"CAT"), Some(Context::Chh));
        assert_eq!(classify(b"CNN"), Some(Context::Chh));
        assert_eq!(classify(b"NCG"), None);
        assert_eq!(classify(b"GCA"), None);
        assert_eq!(classify(b"CG"), None); // len != 3
    }

    // ── Task 3: cytosine_lookup ──────────────────────────────────────

    fn lookup_line(
        seq: &[u8],
        ext: &[u8],
        offset: u32,
        end: u32,
        calls: &[(u32, u8, u8)],
    ) -> String {
        let mut read: HashMap<u32, (u8, u8)> = HashMap::new();
        for &(p, s, c) in calls {
            read.insert(p, (s, c));
        }
        let mut out = Vec::new();
        cytosine_lookup(&mut out, b"rid", b"chr1", seq, offset, end, ext, &read).unwrap();
        String::from_utf8(out).unwrap().trim_end().to_string()
    }

    #[test]
    fn cg_acg_methylated_counts_meth_cg() {
        // seq "ACGT"; ext = "TT"+"ACGT"+"AA". Forward C @seq idx1 (pos2):
        // tri = ext[3..6] = "CGT" → CG; upstream = ext[2..5] = "ACG" → accepted.
        // g = pos+offset-1 = 2 (offset=1). call Z, state + → meth_CG.
        let line = lookup_line(b"ACGT", b"TTACGTAA", 1, 4, &[(2, b'+', b'Z')]);
        assert_eq!(line, "rid\tchr1\t1\t4\t1\t0\t0\t0");
    }

    #[test]
    fn cg_acg_unmethylated_lowercase_call_counts_unmeth_cg() {
        // Same C, state '-' with lowercase call 'z' → unmeth_CG (tally keys on state).
        let line = lookup_line(b"ACGT", b"TTACGTAA", 1, 4, &[(2, b'-', b'z')]);
        assert_eq!(line, "rid\tchr1\t1\t4\t0\t1\t0\t0");
    }

    #[test]
    fn cg_gcg_upstream_rejected_all_zero() {
        // seq "GCGT"; ext = "AA"+"GCGT"+"AA". C @idx1 (pos2): tri = ext[3..6] =
        // "CGT" → CG; upstream = ext[2..5] = "GCG" → NOT ACG/TCG → rejected.
        let line = lookup_line(b"GCGT", b"AAGCGTAA", 1, 4, &[(2, b'+', b'Z')]);
        assert_eq!(line, "rid\tchr1\t1\t4\t0\t0\t0\t0");
    }

    #[test]
    fn reverse_g_strand_cpg_tcg_counts_unmeth_cg() {
        // seq "TCGA"; ext = "AA"+"TCGA"+"AA" = "AATCGAAA". G @idx2 (pos3):
        // tri = revcomp(ext[2..5]="TCG") = "CGA" → CG; upstream =
        // revcomp(ext[3..6]="CGA") = "TCG" → accepted. g = pos+offset-1 = 3.
        // Only the G@pos3 is covered (read key 3); state '-' → unmeth_CG.
        let line = lookup_line(b"TCGA", b"AATCGAAA", 1, 4, &[(3, b'-', b'z')]);
        assert_eq!(line, "rid\tchr1\t1\t4\t0\t1\t0\t0");
    }

    #[test]
    fn chh_gpc_counts_meth_gc() {
        // seq "GCAA"; ext = "TT"+"GCAA"+"TT". C @idx1 (pos2): tri = ext[3..6] =
        // "CAA" → CHH; upstream = ext[2..5] = "GCA" → starts "GC". call H,
        // state + → meth_nonCG (col 7).
        let line = lookup_line(b"GCAA", b"TTGCAATT", 1, 4, &[(2, b'+', b'H')]);
        assert_eq!(line, "rid\tchr1\t1\t4\t0\t0\t1\t0");
    }

    #[test]
    fn chg_gpc_counts_unmeth_gc() {
        // seq "GCAG"; ext = "TT"+"GCAG"+"TT". C @idx1 (pos2): tri = ext[3..6] =
        // "CAG" → CHG; upstream = ext[2..5] = "GCA" → "GC". call x, state - →
        // unmeth_nonCG (col 8).
        let line = lookup_line(b"GCAG", b"TTGCAGTT", 1, 4, &[(2, b'-', b'x')]);
        assert_eq!(line, "rid\tchr1\t1\t4\t0\t0\t0\t1");
    }

    #[test]
    fn position_not_covered_yields_all_zero() {
        // No calls in the read map → nothing tallies.
        let line = lookup_line(b"ACGT", b"TTACGTAA", 1, 4, &[]);
        assert_eq!(line, "rid\tchr1\t1\t4\t0\t0\t0\t0");
    }

    #[test]
    fn cpg_context_but_call_is_chh_letter_disregarded() {
        // C in CG context but the stored call is 'h' (CHH letter) → mismatch →
        // disregarded (no count).
        let line = lookup_line(b"ACGT", b"TTACGTAA", 1, 4, &[(2, b'+', b'h')]);
        assert_eq!(line, "rid\tchr1\t1\t4\t0\t0\t0\t0");
    }

    // ── Tasks 4 & 5: per_read_filtering + process_read ───────────────

    fn run_pipeline(genome_fa: &str, yacht: &str) -> String {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("chr1.fa"), genome_fa).unwrap();
        let genome = Genome::load(t.path(), &[".fa", ".fasta"]).unwrap();
        let mut out = Vec::new();
        per_read_filtering(Cursor::new(yacht.as_bytes()), &genome, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    // genome with pos5=T, pos6=C, pos7=G → pos6 is a CpG in T-CG context.
    const G20: &str = ">chr1\nAAAATCGAAAAAAAAAAAAA\n";

    #[test]
    fn skips_bismark_header_and_groups_consecutive_in_order() {
        let yacht = "Bismark methylation extractor v0.25.1\n\
                     r1\t+\tchr1\t6\tZ\t4\t12\t+\n\
                     r2\t-\tchr1\t6\tz\t4\t12\t+\n";
        let out = run_pipeline(G20, yacht);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2, "got: {out:?}");
        assert_eq!(lines[0], "r1\tchr1\t4\t12\t1\t0\t0\t0"); // meth_CG
        assert_eq!(lines[1], "r2\tchr1\t4\t12\t0\t1\t0\t0"); // unmeth_CG
    }

    #[test]
    fn non_consecutive_same_id_is_two_reads() {
        let yacht = "r1\t+\tchr1\t6\tZ\t4\t12\t+\n\
                     r2\t-\tchr1\t6\tz\t4\t12\t+\n\
                     r1\t-\tchr1\t6\tz\t4\t12\t+\n";
        let out = run_pipeline(G20, yacht);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines.len(),
            3,
            "non-consecutive r1 must flush twice: {out:?}"
        );
        assert_eq!(lines[0], "r1\tchr1\t4\t12\t1\t0\t0\t0");
        assert_eq!(lines[2], "r1\tchr1\t4\t12\t0\t1\t0\t0");
    }

    #[test]
    fn same_position_within_read_last_wins() {
        // Two calls at pos6 in one read: +Z then -z → LAST (state '-') wins → unmeth.
        let yacht = "r1\t+\tchr1\t6\tZ\t4\t12\t+\n\
                     r1\t-\tchr1\t6\tz\t4\t12\t+\n";
        let out = run_pipeline(G20, yacht);
        assert_eq!(out, "r1\tchr1\t4\t12\t0\t1\t0\t0\n");
    }

    #[test]
    fn empty_or_all_bismark_input_errors_empty() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("chr1.fa"), ">chr1\nACGTACGT\n").unwrap();
        let genome = Genome::load(t.path(), &[".fa"]).unwrap();
        let mut out = Vec::new();
        let err = per_read_filtering(
            Cursor::new(&b"Bismark header only\n"[..]),
            &genome,
            &mut out,
        )
        .unwrap_err();
        assert!(matches!(err, BismarkNomeError::EmptyInput));
        assert!(out.is_empty());
    }

    #[test]
    fn malformed_short_line_is_skipped() {
        // A 3-field line then a valid read: the short line is skipped, the read processed.
        let yacht = "junk\tline\there\n\
                     r1\t+\tchr1\t6\tZ\t4\t12\t+\n";
        let out = run_pipeline(G20, yacht);
        assert_eq!(out, "r1\tchr1\t4\t12\t1\t0\t0\t0\n");
    }

    #[test]
    fn forward_read_start_le_3_emits_no_line() {
        // start=2 → guard part1 (2-2>1) false → not suitable → no line.
        let out = run_pipeline(
            ">chr1\nACGTACGTACGTACGTACGT\n",
            "r1\t+\tchr1\t2\tz\t2\t10\t+\n",
        );
        assert_eq!(out, "");
    }

    #[test]
    fn reverse_read_end_1_emits_all_zero_line() {
        // Reverse read: col6=10 (start, rightmost), col7=1 (end, leftmost).
        // last_start=10, length=10; guard needs chr_len >= 10-2+10+4 = 22.
        // 30bp genome passes; ext from end-3=-2 is degenerate → all-zero line.
        let g30 = ">chr1\nACGTACGTACGTACGTACGTACGTACGTAC\n";
        let out = run_pipeline(g30, "r1\t-\tchr1\t10\tz\t10\t1\t-\n");
        assert_eq!(out, "r1\tchr1\t1\t10\t0\t0\t0\t0\n"); // offset/end ascending = 1,10
    }

    #[test]
    fn unknown_chromosome_emits_nothing() {
        let out = run_pipeline(">chr1\nACGTACGT\n", "r1\t+\tchrZ\t4\tz\t4\t8\t+\n");
        assert_eq!(out, "");
    }

    #[test]
    fn guard_ge_boundary_suitable_and_one_less_not() {
        // Forward read start=4, end=8 → length=5; guard part2: chr_len >= 4-2+5+4 = 11.
        let yacht = "r1\t+\tchr1\t6\tz\t4\t8\t+\n";
        let g11 = ">chr1\nAAAATCGAAAA\n"; // 11 bp → suitable (emits a line)
        let g10 = ">chr1\nAAAATCGAAA\n"; //  10 bp → not suitable (no line)
        assert!(
            !run_pipeline(g11, yacht).is_empty(),
            "chr_len == boundary must be suitable"
        );
        assert_eq!(
            run_pipeline(g10, yacht),
            "",
            "chr_len == boundary-1 must skip"
        );
    }

    // ── Task 6: write_report header constant ─────────────────────────

    #[test]
    fn header_bytes_match_perl() {
        assert_eq!(
            HEADER,
            b"ReadID\tChr\tStart\tEnd\tmeth_CG\tunmeth_CG\tmeth_GC\tunmeth_GC\n"
        );
    }

    #[test]
    fn write_report_empty_input_writes_header_then_errors() {
        // D4/P11: header-only .gz on disk + EmptyInput.
        let t = tempfile::tempdir().unwrap();
        let gdir = t.path().join("g");
        std::fs::create_dir(&gdir).unwrap();
        std::fs::write(gdir.join("chr1.fa"), ">chr1\nACGTACGT\n").unwrap();
        let genome = Genome::load(&gdir, &[".fa"]).unwrap();
        let inp = t.path().join("empty.txt");
        std::fs::write(&inp, "").unwrap();
        let outp = t.path().join("empty.manOwar.txt.gz");

        let err = write_report(&inp, &outp, &genome).unwrap_err();
        assert!(matches!(err, BismarkNomeError::EmptyInput));
        // The header-only .gz must exist and decompress to exactly the header.
        let mut d = flate2::read::MultiGzDecoder::new(std::fs::File::open(&outp).unwrap());
        let mut got = Vec::new();
        std::io::Read::read_to_end(&mut d, &mut got).unwrap();
        assert_eq!(got, HEADER);
    }
}
