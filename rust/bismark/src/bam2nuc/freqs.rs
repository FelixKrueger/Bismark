//! Mono-/di-nucleotide counter + genomic-composition cache.
//!
//! Faithful to Perl `bam2nuc`'s `process_sequence` (`:318-343`) and
//! `get_genomic_frequencies` (`:160-216`).
//!
//! [`NucCounts`] is an **allocation-free** tally: a `[u64; 256]` mono array +
//! a heap `[u64; 65536]` di array (indexed by the packed byte pair). The hot
//! path (`bump_*`) never allocates — critical for the ~3 Gbp genomic pass and
//! the ~55 M-read sample pass. Counts only ever increment from 0, so
//! **`count == 0` ⇔ "never seen"** — which is exactly Perl's `undef` semantics:
//! an unseen word is omitted from the cache (Perl writes only
//! `keys %genomic_freqs`) and printed as an empty field in the stats report
//! (Perl interpolates `undef` as `""`). See [`crate::bam2nuc::report`].
//!
//! Everything except `N` is counted — IUPAC ambiguity bytes (R/Y/…) ARE tallied
//! (Perl skips only the literal `N`); the ACGT restriction lives only in the
//! stats report, never here or in the cache.

use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use crate::bam2nuc::error::BismarkBam2nucError;
use crate::bam2nuc::genome::Genome;

/// The genome-composition cache filename (reused byte-for-byte if present).
pub const CACHE_FILE: &str = "genomic_nucleotide_frequencies.txt";

/// Allocation-free mono/di nucleotide tally.
///
/// `count == 0` means "never counted" (Perl `undef`); see the module docs.
#[derive(Debug)]
pub struct NucCounts {
    /// Mono counts indexed by byte value.
    mono: [u64; 256],
    /// Di counts indexed by `(first as usize) << 8 | second as usize`.
    /// Heap-allocated (512 KiB) to avoid a large stack frame.
    di: Box<[u64]>,
}

impl Default for NucCounts {
    fn default() -> Self {
        NucCounts {
            mono: [0u64; 256],
            di: vec![0u64; 65536].into_boxed_slice(),
        }
    }
}

impl NucCounts {
    #[inline]
    fn bump_mono(&mut self, b: u8) {
        self.mono[b as usize] += 1;
    }

    #[inline]
    fn bump_di(&mut self, a: u8, b: u8) {
        self.di[(a as usize) << 8 | b as usize] += 1;
    }

    /// Count for a mononucleotide byte (0 ⇔ never seen).
    #[must_use]
    pub fn mono(&self, b: u8) -> u64 {
        self.mono[b as usize]
    }

    /// Count for a dinucleotide byte pair (0 ⇔ never seen).
    #[must_use]
    pub fn di(&self, a: u8, b: u8) -> u64 {
        self.di[(a as usize) << 8 | b as usize]
    }

    /// Serialize to the cache-file byte layout: every NON-zero word as
    /// `<word>\t<count>\n`, **bytewise-sorted** (matches Perl `sort keys
    /// %genomic_freqs` under `LC_ALL=C`). Raw bytes (not a `String`) so a
    /// non-ASCII genome byte round-trips byte-exactly.
    #[must_use]
    pub fn cache_bytes(&self) -> Vec<u8> {
        let mut entries: Vec<(Vec<u8>, u64)> = Vec::new();
        for idx in 0usize..256 {
            let n = self.mono[idx];
            if n > 0 {
                entries.push((vec![idx as u8], n));
            }
        }
        for idx in 0usize..65536 {
            let n = self.di[idx];
            if n > 0 {
                entries.push((vec![(idx >> 8) as u8, (idx & 0xff) as u8], n));
            }
        }
        // Bytewise lexicographic — 1-byte words sort before their 2-byte
        // extensions (a prefix compares less), exactly like Perl's string sort.
        entries.sort_unstable_by(|x, y| x.0.cmp(&y.0));

        let mut out = Vec::new();
        for (word, n) in entries {
            out.extend_from_slice(&word);
            out.push(b'\t');
            out.extend_from_slice(n.to_string().as_bytes());
            out.push(b'\n');
        }
        out
    }
}

/// Tally mono- and di-nucleotides of `seq` into `counts` (Perl
/// `process_sequence`): every byte except `N` is a mono; every overlapping
/// 2-mer window not containing `N` is a di (`len-1` windows).
pub fn process_sequence(seq: &[u8], counts: &mut NucCounts) {
    for (i, &m) in seq.iter().enumerate() {
        if m != b'N' {
            counts.bump_mono(m);
        }
        // Di window [i, i+1]; guarded so i+1 is in bounds (Perl `:335`).
        if i + 2 <= seq.len() {
            let b = seq[i + 1];
            if m != b'N' && b != b'N' {
                counts.bump_di(m, b);
            }
        }
    }
}

/// Compute the whole-genome composition (Perl `:179-184`): one
/// `process_sequence` pass over every chromosome's **+ strand** sequence (no
/// reverse-complement). Order-agnostic (counting is commutative).
#[must_use]
pub fn compute_genomic(genome: &Genome) -> NucCounts {
    let mut counts = NucCounts::default();
    for (_name, seq) in genome.seqs() {
        process_sequence(seq, &mut counts);
    }
    counts
}

/// Resolve the genomic composition: reuse the cache byte-for-byte if it exists
/// in the genome folder, else compute it and try to write the cache (Perl
/// `get_genomic_frequencies`).
///
/// **Existence is checked ONLY against `genome_folder`** (Perl `:163`) — a cache
/// that previously landed in `output_dir` does NOT prevent a recompute.
pub fn get_genomic_frequencies(
    genome: &Genome,
    genome_folder: &Path,
    output_dir: &str,
) -> Result<NucCounts, BismarkBam2nucError> {
    let cache_path = genome_folder.join(CACHE_FILE);
    if cache_path.exists() {
        eprintln!(
            "Detected file '{CACHE_FILE}' in the genome folder already. Using nucleotide frequencies contained therein ..."
        );
        return read_cache(&cache_path);
    }
    eprintln!(
        "Could not find genomic nucleotide frequency table in the genome folder, calculating genomic frequencies ..."
    );
    let counts = compute_genomic(genome);
    write_cache(&counts, genome_folder, output_dir);
    Ok(counts)
}

/// Write the cache with Perl's precedence (Perl `:189-214`): try
/// `<genome_folder>/<CACHE_FILE>` first, fall back to `<output_dir><CACHE_FILE>`
/// on failure, else warn-and-continue. Never errors (a missing cache is not
/// fatal — it's just recomputed next time).
pub fn write_cache(counts: &NucCounts, genome_folder: &Path, output_dir: &str) {
    let bytes = counts.cache_bytes();

    let primary = genome_folder.join(CACHE_FILE);
    if write_all_bytes(&primary, &bytes).is_ok() {
        eprintln!(
            "Writing genomic nucleotide frequencies to the file >{}< for future re-use",
            primary.display()
        );
        return;
    }

    // Fall back to the output directory (a path *prefix*).
    let fallback = format!("{output_dir}{CACHE_FILE}");
    if write_all_bytes(Path::new(&fallback), &bytes).is_ok() {
        eprintln!(
            "Writing genomic nucleotide frequencies to the file >{fallback}< for future re-use"
        );
        return;
    }

    eprintln!(
        "Failed to write out file {CACHE_FILE}; skipping writing out genomic frequency table"
    );
}

fn write_all_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.write_all(bytes)
}

/// Read a `genomic_nucleotide_frequencies.txt` cache into a [`NucCounts`].
/// Each line is `<word>\t<count>`; a 1-byte word → mono, 2-byte → di.
fn read_cache(path: &Path) -> Result<NucCounts, BismarkBam2nucError> {
    let reader = BufReader::new(std::fs::File::open(path)?);
    let mut counts = NucCounts::default();
    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        let line_no = i + 1;
        let (word, freq) = line
            .split_once('\t')
            .ok_or(BismarkBam2nucError::MalformedCacheLine { line_no })?;
        let n: u64 = freq
            .parse()
            .map_err(|_| BismarkBam2nucError::MalformedCacheLine { line_no })?;
        let wb = word.as_bytes();
        match wb.len() {
            1 => counts.mono[wb[0] as usize] = n,
            2 => counts.di[(wb[0] as usize) << 8 | wb[1] as usize] = n,
            _ => return Err(BismarkBam2nucError::MalformedCacheLine { line_no }),
        }
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: tally a sequence into a fresh `NucCounts`.
    fn counts_of(seq: &[u8]) -> NucCounts {
        let mut c = NucCounts::default();
        process_sequence(seq, &mut c);
        c
    }

    #[test]
    fn process_sequence_basic_mono_and_di() {
        let c = counts_of(b"ACGT");
        for b in [b'A', b'C', b'G', b'T'] {
            assert_eq!(c.mono(b), 1, "mono {}", b as char);
        }
        assert_eq!(c.di(b'A', b'C'), 1);
        assert_eq!(c.di(b'C', b'G'), 1);
        assert_eq!(c.di(b'G', b'T'), 1);
        // No di starting at the last base.
        assert_eq!(c.di(b'T', b'A'), 0);
    }

    #[test]
    fn process_sequence_skips_n_mono_and_di() {
        // "ANG": mono A,G (N skipped); di AN/NG both contain N → none.
        let c = counts_of(b"ANG");
        assert_eq!(c.mono(b'A'), 1);
        assert_eq!(c.mono(b'G'), 1);
        assert_eq!(c.mono(b'N'), 0);
        assert_eq!(c.di(b'A', b'N'), 0);
        assert_eq!(c.di(b'N', b'G'), 0);
    }

    #[test]
    fn process_sequence_counts_iupac() {
        // "ARG": mono A,R,G; di AR,RG (R is not N → counted).
        let c = counts_of(b"ARG");
        assert_eq!(c.mono(b'A'), 1);
        assert_eq!(c.mono(b'R'), 1);
        assert_eq!(c.mono(b'G'), 1);
        assert_eq!(c.di(b'A', b'R'), 1);
        assert_eq!(c.di(b'R', b'G'), 1);
    }

    #[test]
    fn process_sequence_overlapping_windows() {
        let c = counts_of(b"AAAA");
        assert_eq!(c.mono(b'A'), 4);
        assert_eq!(c.di(b'A', b'A'), 3); // 3 overlapping windows
    }

    #[test]
    fn process_sequence_empty_and_single() {
        let c = counts_of(b"");
        assert_eq!(c.mono(b'A'), 0);
        let c = counts_of(b"A");
        assert_eq!(c.mono(b'A'), 1);
        assert_eq!(c.di(b'A', b'A'), 0);
    }

    #[test]
    fn cache_bytes_acgtn_exact_order() {
        // "ACGT" → mono A,C,G,T=1; di AC,CG,GT=1. Bytewise-sorted:
        // A, AC, C, CG, G, GT, T (1-byte word before its 2-byte extension).
        let c = counts_of(b"ACGT");
        let s = String::from_utf8(c.cache_bytes()).unwrap();
        assert_eq!(s, "A\t1\nAC\t1\nC\t1\nCG\t1\nG\t1\nGT\t1\nT\t1\n");
    }

    #[test]
    fn cache_bytes_iupac_sort_placement() {
        // "ACRGT" → mono A,C,R,G,T=1; di AC,CR,RG,GT=1.
        // Bytewise: A < AC < C < CR < G < GT < R < RG < T
        // (R=0x52 sorts after the G-group and after GT; before T=0x54).
        let c = counts_of(b"ACRGT");
        let s = String::from_utf8(c.cache_bytes()).unwrap();
        assert_eq!(
            s,
            "A\t1\nAC\t1\nC\t1\nCR\t1\nG\t1\nGT\t1\nR\t1\nRG\t1\nT\t1\n"
        );
    }

    #[test]
    fn cache_bytes_omits_zero_words() {
        // A never-seen word must NOT appear (count==0 ⇔ absent).
        let c = counts_of(b"AA");
        let s = String::from_utf8(c.cache_bytes()).unwrap();
        assert_eq!(s, "A\t2\nAA\t1\n"); // no C/G/T/other lines
    }

    #[test]
    fn compute_genomic_sums_all_chromosomes() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("g.fa"), ">c1\nAC\n>c2\nGT\n").unwrap();
        let g = Genome::load(t.path()).unwrap();
        let c = compute_genomic(&g);
        // c1: mono A,C; di AC.  c2: mono G,T; di GT.  No cross-chr di.
        assert_eq!(c.mono(b'A'), 1);
        assert_eq!(c.mono(b'C'), 1);
        assert_eq!(c.mono(b'G'), 1);
        assert_eq!(c.mono(b'T'), 1);
        assert_eq!(c.di(b'A', b'C'), 1);
        assert_eq!(c.di(b'G', b'T'), 1);
        assert_eq!(c.di(b'C', b'G'), 0); // di does NOT cross the chromosome boundary
    }

    #[test]
    fn get_genomic_frequencies_computes_then_writes_cache() {
        let gdir = tempfile::tempdir().unwrap();
        std::fs::write(gdir.path().join("g.fa"), ">c1\nACGT\n").unwrap();
        let g = Genome::load(gdir.path()).unwrap();
        let c = get_genomic_frequencies(&g, gdir.path(), "").unwrap();
        assert_eq!(c.mono(b'A'), 1);
        // The cache file was written to the genome folder.
        let cache = gdir.path().join(CACHE_FILE);
        assert!(cache.exists());
        let written = std::fs::read_to_string(&cache).unwrap();
        assert_eq!(written, "A\t1\nAC\t1\nC\t1\nCG\t1\nG\t1\nGT\t1\nT\t1\n");
    }

    #[test]
    fn get_genomic_frequencies_reuses_existing_cache_byte_for_byte() {
        let gdir = tempfile::tempdir().unwrap();
        std::fs::write(gdir.path().join("g.fa"), ">c1\nACGT\n").unwrap();
        // Plant a cache whose values could NOT arise from the genome (×1000) so a
        // recompute-instead-of-reuse bug would be visible.
        std::fs::write(
            gdir.path().join(CACHE_FILE),
            "A\t1000\nC\t2000\nG\t3000\nT\t4000\n",
        )
        .unwrap();
        let g = Genome::load(gdir.path()).unwrap();
        let c = get_genomic_frequencies(&g, gdir.path(), "").unwrap();
        assert_eq!(c.mono(b'A'), 1000); // planted value wins (no recompute)
        assert_eq!(c.mono(b'T'), 4000);
    }

    #[test]
    fn existence_checked_only_against_genome_folder() {
        // A cache sitting in output_dir does NOT prevent a recompute.
        let gdir = tempfile::tempdir().unwrap();
        let odir = tempfile::tempdir().unwrap();
        std::fs::write(gdir.path().join("g.fa"), ">c1\nACGT\n").unwrap();
        std::fs::write(odir.path().join(CACHE_FILE), "A\t9999\n").unwrap();
        let g = Genome::load(gdir.path()).unwrap();
        let out_prefix = format!("{}/", odir.path().display());
        let c = get_genomic_frequencies(&g, gdir.path(), &out_prefix).unwrap();
        // genome_folder has no cache → recompute (NOT the planted 9999).
        assert_eq!(c.mono(b'A'), 1);
    }

    #[test]
    fn read_cache_round_trips_via_write() {
        let c = counts_of(b"ACGTACGT");
        let bytes = c.cache_bytes();
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join(CACHE_FILE);
        std::fs::write(&p, &bytes).unwrap();
        let back = read_cache(&p).unwrap();
        assert_eq!(back.cache_bytes(), bytes);
    }

    #[cfg(unix)]
    #[test]
    fn write_cache_falls_back_to_output_dir_when_genome_dir_readonly() {
        use std::os::unix::fs::PermissionsExt;
        let gdir = tempfile::tempdir().unwrap();
        let odir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(gdir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let c = counts_of(b"ACGT");
        let out_prefix = format!("{}/", odir.path().display());
        write_cache(&c, gdir.path(), &out_prefix);
        // Restore perms so the tempdir can be cleaned up.
        std::fs::set_permissions(gdir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!gdir.path().join(CACHE_FILE).exists());
        assert!(odir.path().join(CACHE_FILE).exists());
    }
}
