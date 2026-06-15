//! Genome folder + bisulfite-index + raw-FASTA discovery.
//!
//! Mirrors Perl `bismark` 7604–7800 / `read_genome_into_memory` 5031–50:
//! - the genome folder is made **absolute** (Perl `chdir`+`getcwd`);
//! - the Bowtie 2 small index (`BS_CT.{1,2,3,4,rev.1,rev.2}.bt2`) is required,
//!   with a **large** (`.bt2l`) fallback for >4 Gbp references;
//! - the raw FASTA is found by **extension priority** (`.fa` → `.fa.gz` →
//!   `.fasta` → `.fasta.gz`). The extension **match** is **case-SENSITIVE** on
//!   raw bytes (Perl `<*.fa>` glob on Linux), while the **sort** within the
//!   chosen group is **case-INSENSITIVE** (Perl's bundled `File::Glob` folds on
//!   all platforms). The order is **byte-significant** — it sets the BAM `@SQ`
//!   order in Phase 5. This logic is a deliberate mirror of
//!   `bismark-genome-preparation::discovery` (`in_group` + `fasta_name_cmp`):
//!   the two ports **jointly** define the `@SQ`/index ordering contract and MUST
//!   stay in lockstep — adjudicate on Linux/oxy, never macOS (the genome-prep
//!   glob-fold lesson). *Follow-up:* promote this to a shared crate to remove
//!   the duplication.

use std::path::{Path, PathBuf};

use crate::config::Aligner;
use crate::error::{AlignerError, Result};

/// Which FASTA extension category was found (extension-priority order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastaKind {
    /// `*.fa`
    Fa,
    /// `*.fa.gz`
    FaGz,
    /// `*.fasta`
    Fasta,
    /// `*.fasta.gz`
    FastaGz,
}

impl FastaKind {
    /// The extension-priority probe order Perl uses (`.fa` → `.fa.gz` →
    /// `.fasta` → `.fasta.gz`).
    const PROBE_ORDER: [FastaKind; 4] = [
        FastaKind::Fa,
        FastaKind::FaGz,
        FastaKind::Fasta,
        FastaKind::FastaGz,
    ];

    /// Case-SENSITIVE raw-byte membership test (mirrors Perl `<*.fa>` + the
    /// sibling `bismark-genome-preparation::discovery::in_group`): `.fa`/`.fasta`
    /// exclude their `.gz` siblings so the groups are disjoint. Matching on bytes
    /// (not `&str`) means a non-UTF-8 file name is **not** silently dropped.
    fn matches(self, name: &[u8]) -> bool {
        match self {
            FastaKind::Fa => name.ends_with(b".fa") && !name.ends_with(b".fa.gz"),
            FastaKind::FaGz => name.ends_with(b".fa.gz"),
            FastaKind::Fasta => name.ends_with(b".fasta") && !name.ends_with(b".fasta.gz"),
            FastaKind::FastaGz => name.ends_with(b".fasta.gz"),
        }
    }
}

/// Compare FASTA file names the way Perl's `<*.fa>` sorts them: **case-insensitively**
/// (ASCII fold) with the raw bytes as a tiebreak. Must match
/// `bismark-genome-preparation::discovery::fasta_name_cmp` exactly.
fn fasta_name_cmp(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    a.to_ascii_lowercase()
        .cmp(&b.to_ascii_lowercase())
        .then_with(|| a.cmp(b))
}

/// Resolved genome indexes + raw FASTA inventory (the input contract for the
/// rest of the pipeline).
#[derive(Debug, Clone)]
pub struct GenomeIndexes {
    /// Absolute path to the genome folder.
    pub genome_dir: PathBuf,
    /// `<genome>/Bisulfite_Genome/CT_conversion/BS_CT` (index basename).
    pub ct_index_basename: PathBuf,
    /// `<genome>/Bisulfite_Genome/GA_conversion/BS_GA` (index basename).
    pub ga_index_basename: PathBuf,
    /// `true` if the large (`.bt2l`) index was found instead of the small one.
    pub large_index: bool,
    /// `<genome>/Bisulfite_Genome/Combined/BS_combined` (index basename) — the v2
    /// combined CT+GA index. `Some` iff a complete set (small `.bt2` OR large
    /// `.bt2l`) is present; probed best-effort for **every** run (the cost is a
    /// directory stat). `--combined_index` runs require it (the `resolve` guard
    /// errors when this is `None`); faithful runs ignore it. Deviates from the
    /// PLAN §7 `PathBuf` (it must be optional — most genomes have no combined index).
    pub combined_index_basename: Option<PathBuf>,
    /// Raw FASTA file(s), in byte-significant order (sets `@SQ` order, Phase 5).
    pub fastas: Vec<PathBuf>,
    /// Which extension category the FASTA(s) came from.
    pub fasta_kind: FastaKind,
}

/// The expected index file names for a given basename stem. This is a per-aligner
/// **suffix-arity** difference, not just an extension swap:
/// - **Bowtie 2** — 6 files `{1,2,3,4,rev.1,rev.2}.bt2` (`.bt2l` large).
/// - **HISAT2** — 8 files `{1,2,3,4,5,6,7,8}.ht2` (`.ht2l` large; no `rev.*`).
/// - **minimap2** — a SINGLE `<stem>.mmi` (Perl 7022 `$bisulfiteIndex.".mmi"`).
///   There is no large-index variant, so `large` is ignored: the small/large
///   fallback in [`discover_genome`] is a harmless no-op (both probe the same
///   `.mmi`), and `large_index` stays `false`.
fn index_suffixes(aligner: Aligner, stem: &str, large: bool) -> Vec<String> {
    match aligner {
        Aligner::Bowtie2 => {
            let ext = if large { "bt2l" } else { "bt2" };
            ["1", "2", "3", "4", "rev.1", "rev.2"]
                .iter()
                .map(|s| format!("{stem}.{s}.{ext}"))
                .collect()
        }
        Aligner::Hisat2 => {
            let ext = if large { "ht2l" } else { "ht2" };
            (1..=8).map(|n| format!("{stem}.{n}.{ext}")).collect()
        }
        // rammap is minimap-like — the same single `<stem>.mmi` (no large variant).
        Aligner::Minimap2 | Aligner::Rammap => vec![format!("{stem}.mmi")],
    }
}

/// Check that all expected index files for `stem` exist in `dir`. Returns the
/// first missing file name, or `None` if all present.
fn first_missing(aligner: Aligner, dir: &Path, stem: &str, large: bool) -> Option<String> {
    index_suffixes(aligner, stem, large)
        .into_iter()
        .find(|f| !dir.join(f).is_file())
}

/// Discover the genome folder, validate the bisulfite indexes for `aligner`
/// (Bowtie 2 `.bt2` or HISAT2 `.ht2`), and inventory the raw FASTA file(s).
pub fn discover_genome(aligner: Aligner, genome_arg: &Path) -> Result<GenomeIndexes> {
    // Absolute path (Perl chdir + getcwd). canonicalize also verifies existence.
    let genome_dir = std::fs::canonicalize(genome_arg)
        .map_err(|_| AlignerError::GenomeFolder(genome_arg.to_path_buf()))?;
    if !genome_dir.is_dir() {
        return Err(AlignerError::GenomeFolder(genome_arg.to_path_buf()));
    }

    let ct_dir = genome_dir.join("Bisulfite_Genome").join("CT_conversion");
    let ga_dir = genome_dir.join("Bisulfite_Genome").join("GA_conversion");

    // Small index first, then large fallback (Perl 7646–7800).
    let large_index = match (
        first_missing(aligner, &ct_dir, "BS_CT", false),
        first_missing(aligner, &ga_dir, "BS_GA", false),
    ) {
        (None, None) => false,
        _ => {
            // Small incomplete — require the large index instead.
            if let Some(missing) = first_missing(aligner, &ct_dir, "BS_CT", true) {
                return Err(AlignerError::FaultyIndex {
                    aligner: aligner.name().to_string(),
                    converted: "C->T".to_string(),
                    missing,
                });
            }
            if let Some(missing) = first_missing(aligner, &ga_dir, "BS_GA", true) {
                return Err(AlignerError::FaultyIndex {
                    aligner: aligner.name().to_string(),
                    converted: "G->A".to_string(),
                    missing,
                });
            }
            true
        }
    };

    // v2 combined CT+GA index (`Bisulfite_Genome/Combined/BS_combined`), probed
    // best-effort: `Some` iff a complete small OR large set exists. Its large-ness
    // is independent of the CT/GA index (Bowtie 2 `-x <basename>` auto-detects
    // .bt2 vs .bt2l), so only the basename is stored. No error here — the
    // `--combined_index` presence requirement is enforced in `config::resolve`.
    let combined_dir = genome_dir.join("Bisulfite_Genome").join("Combined");
    let combined_index_basename = if combined_dir.is_dir()
        && (first_missing(aligner, &combined_dir, "BS_combined", false).is_none()
            || first_missing(aligner, &combined_dir, "BS_combined", true).is_none())
    {
        Some(combined_dir.join("BS_combined"))
    } else {
        None
    };

    let (fastas, fasta_kind) = discover_fastas(&genome_dir)?;

    Ok(GenomeIndexes {
        ct_index_basename: ct_dir.join("BS_CT"),
        ga_index_basename: ga_dir.join("BS_GA"),
        genome_dir,
        large_index,
        combined_index_basename,
        fastas,
        fasta_kind,
    })
}

/// Find the raw FASTA file(s) by extension priority (first non-empty group
/// wins), case-insensitively sorted. Filters on `path.is_file()` (which
/// **follows symlinks**) and matches on raw file-name bytes (non-UTF-8 safe) —
/// both mirroring `bismark-genome-preparation::discovery::find_fasta_files`.
fn discover_fastas(genome_dir: &Path) -> Result<(Vec<PathBuf>, FastaKind)> {
    for kind in FastaKind::PROBE_ORDER {
        let mut group: Vec<PathBuf> = std::fs::read_dir(genome_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.file_name()
                        .map(|n| kind.matches(n.as_encoded_bytes()))
                        .unwrap_or(false)
            })
            .collect();
        if group.is_empty() {
            continue;
        }
        group.sort_by(|a, b| {
            let ka = a.file_name().map(|n| n.as_encoded_bytes()).unwrap_or(b"");
            let kb = b.file_name().map(|n| n.as_encoded_bytes()).unwrap_or(b"");
            fasta_name_cmp(ka, kb)
        });
        return Ok((group, kind));
    }

    Err(AlignerError::NoFasta(genome_dir.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_small_index(dir: &Path) {
        let ct = dir.join("Bisulfite_Genome").join("CT_conversion");
        let ga = dir.join("Bisulfite_Genome").join("GA_conversion");
        fs::create_dir_all(&ct).unwrap();
        fs::create_dir_all(&ga).unwrap();
        for s in ["1", "2", "3", "4", "rev.1", "rev.2"] {
            fs::write(ct.join(format!("BS_CT.{s}.bt2")), b"x").unwrap();
            fs::write(ga.join(format!("BS_GA.{s}.bt2")), b"x").unwrap();
        }
    }

    /// A complete small HISAT2 index: 8 `.ht2` files per converted genome (no
    /// `rev.*`) + one FASTA. `large` writes `.ht2l` instead.
    fn make_ht2_index(dir: &Path, large: bool) {
        let ct = dir.join("Bisulfite_Genome").join("CT_conversion");
        let ga = dir.join("Bisulfite_Genome").join("GA_conversion");
        fs::create_dir_all(&ct).unwrap();
        fs::create_dir_all(&ga).unwrap();
        let ext = if large { "ht2l" } else { "ht2" };
        for n in 1..=8 {
            fs::write(ct.join(format!("BS_CT.{n}.{ext}")), b"x").unwrap();
            fs::write(ga.join(format!("BS_GA.{n}.{ext}")), b"x").unwrap();
        }
        fs::write(dir.join("genome.fa"), b">chr1\nACGT\n").unwrap();
    }

    #[test]
    fn fasta_priority_prefers_fa_and_sorts_case_insensitively() {
        let tmp = TempDir::new().unwrap();
        make_small_index(tmp.path());
        fs::write(tmp.path().join("B.fa"), b">b\nA\n").unwrap();
        fs::write(tmp.path().join("a.fa"), b">a\nA\n").unwrap();
        // a `.fa.gz` must be ignored entirely because `.fa` files exist.
        fs::write(tmp.path().join("z.fa.gz"), b"x").unwrap();

        let g = discover_genome(Aligner::Bowtie2, tmp.path()).unwrap();
        assert_eq!(g.fasta_kind, FastaKind::Fa);
        assert!(!g.large_index);
        let names: Vec<String> = g
            .fastas
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.fa".to_string(), "B.fa".to_string()]);
        assert!(g.ct_index_basename.ends_with("CT_conversion/BS_CT"));
        assert!(g.ga_index_basename.ends_with("GA_conversion/BS_GA"));
    }

    /// v2: a complete `Combined/BS_combined.*.bt2` set is discovered as
    /// `combined_index_basename = Some(...)`; a genome without it stays `None`.
    #[test]
    fn discovers_combined_index_when_present() {
        let tmp = TempDir::new().unwrap();
        make_small_index(tmp.path());
        fs::write(tmp.path().join("g.fa"), b">c\nA\n").unwrap();

        // No Combined dir yet → None.
        let g = discover_genome(Aligner::Bowtie2, tmp.path()).unwrap();
        assert!(g.combined_index_basename.is_none());

        // Build the combined small index → Some.
        let comb = tmp.path().join("Bisulfite_Genome").join("Combined");
        fs::create_dir_all(&comb).unwrap();
        for s in ["1", "2", "3", "4", "rev.1", "rev.2"] {
            fs::write(comb.join(format!("BS_combined.{s}.bt2")), b"x").unwrap();
        }
        let g = discover_genome(Aligner::Bowtie2, tmp.path()).unwrap();
        // Compare against the CANONICALISED genome_dir (macOS resolves
        // /var → /private/var), not the raw tmp path.
        let expected = g
            .genome_dir
            .join("Bisulfite_Genome")
            .join("Combined")
            .join("BS_combined");
        assert_eq!(
            g.combined_index_basename.as_deref(),
            Some(expected.as_path())
        );
    }

    /// v2: an INCOMPLETE combined set (missing one file) is NOT accepted → None
    /// (so the `--combined_index` resolve guard errors clearly rather than the run
    /// failing later on a missing Bowtie 2 index file).
    #[test]
    fn incomplete_combined_index_is_none() {
        let tmp = TempDir::new().unwrap();
        make_small_index(tmp.path());
        fs::write(tmp.path().join("g.fa"), b">c\nA\n").unwrap();
        let comb = tmp.path().join("Bisulfite_Genome").join("Combined");
        fs::create_dir_all(&comb).unwrap();
        // Only 5 of the 6 required files.
        for s in ["1", "2", "3", "4", "rev.1"] {
            fs::write(comb.join(format!("BS_combined.{s}.bt2")), b"x").unwrap();
        }
        let g = discover_genome(Aligner::Bowtie2, tmp.path()).unwrap();
        assert!(g.combined_index_basename.is_none());
    }

    #[test]
    fn falls_back_to_fasta_when_no_fa() {
        let tmp = TempDir::new().unwrap();
        make_small_index(tmp.path());
        fs::write(tmp.path().join("genome.fasta"), b">c\nA\n").unwrap();
        let g = discover_genome(Aligner::Bowtie2, tmp.path()).unwrap();
        assert_eq!(g.fasta_kind, FastaKind::Fasta);
    }

    #[test]
    fn incomplete_index_errors() {
        let tmp = TempDir::new().unwrap();
        make_small_index(tmp.path());
        fs::write(tmp.path().join("g.fa"), b">c\nA\n").unwrap();
        fs::remove_file(
            tmp.path()
                .join("Bisulfite_Genome")
                .join("CT_conversion")
                .join("BS_CT.3.bt2"),
        )
        .unwrap();
        let err = discover_genome(Aligner::Bowtie2, tmp.path()).unwrap_err();
        assert!(matches!(err, AlignerError::FaultyIndex { .. }));
    }

    #[test]
    fn no_fasta_errors() {
        let tmp = TempDir::new().unwrap();
        make_small_index(tmp.path());
        let err = discover_genome(Aligner::Bowtie2, tmp.path()).unwrap_err();
        assert!(matches!(err, AlignerError::NoFasta(_)));
    }

    // ---- HISAT2 index discovery (Phase 2a; V4) -----------------------------

    /// HISAT2 = 8 `.ht2` suffixes, no `rev.*` (vs Bowtie 2's 6).
    #[test]
    fn hisat2_suffix_arity_is_eight_ht2() {
        let s = index_suffixes(Aligner::Hisat2, "BS_CT", false);
        assert_eq!(
            s,
            vec![
                "BS_CT.1.ht2",
                "BS_CT.2.ht2",
                "BS_CT.3.ht2",
                "BS_CT.4.ht2",
                "BS_CT.5.ht2",
                "BS_CT.6.ht2",
                "BS_CT.7.ht2",
                "BS_CT.8.ht2",
            ]
        );
        assert!(s.iter().all(|f| !f.contains("rev.")));
        // large fallback flips the extension to .ht2l.
        let l = index_suffixes(Aligner::Hisat2, "BS_GA", true);
        assert_eq!(l[0], "BS_GA.1.ht2l");
        assert_eq!(l.len(), 8);
    }

    #[test]
    fn discovers_complete_ht2_index() {
        let tmp = TempDir::new().unwrap();
        make_ht2_index(tmp.path(), false);
        let g = discover_genome(Aligner::Hisat2, tmp.path()).unwrap();
        assert!(!g.large_index);
        assert!(g.ct_index_basename.ends_with("CT_conversion/BS_CT"));
        assert!(g.ga_index_basename.ends_with("GA_conversion/BS_GA"));
    }

    #[test]
    fn falls_back_to_large_ht2l_index() {
        let tmp = TempDir::new().unwrap();
        make_ht2_index(tmp.path(), true);
        let g = discover_genome(Aligner::Hisat2, tmp.path()).unwrap();
        assert!(g.large_index);
    }

    #[test]
    fn incomplete_ht2_index_errors_with_hisat2_wording() {
        let tmp = TempDir::new().unwrap();
        make_ht2_index(tmp.path(), false);
        // Remove the 7th small file — a suffix that does NOT exist for Bowtie 2,
        // so a 6-suffix check would never look for it. With the small index
        // incomplete, discovery falls back to requiring the LARGE (.ht2l) index
        // (Perl 7646-7800); that index is absent → it reports the first missing
        // large file, named for HISAT2 (never a Bowtie 2 `.bt2`).
        fs::remove_file(
            tmp.path()
                .join("Bisulfite_Genome")
                .join("CT_conversion")
                .join("BS_CT.7.ht2"),
        )
        .unwrap();
        let err = discover_genome(Aligner::Hisat2, tmp.path()).unwrap_err();
        match err {
            AlignerError::FaultyIndex {
                aligner, missing, ..
            } => {
                assert_eq!(aligner, "HISAT2");
                assert!(
                    missing.ends_with(".ht2l"),
                    "expected an HISAT2 large-index file, got {missing}"
                );
            }
            other => panic!("expected FaultyIndex, got {other:?}"),
        }
    }

    /// A small `.ht2` index with fewer than 8 files (Bowtie 2 has only 6) is
    /// rejected — proves the 8-suffix arity is enforced, not the Bowtie 2 6.
    #[test]
    fn six_ht2_files_is_not_a_complete_hisat2_index() {
        let tmp = TempDir::new().unwrap();
        let ct = tmp.path().join("Bisulfite_Genome").join("CT_conversion");
        let ga = tmp.path().join("Bisulfite_Genome").join("GA_conversion");
        fs::create_dir_all(&ct).unwrap();
        fs::create_dir_all(&ga).unwrap();
        // Only 6 of the 8 required small files (mirrors a Bowtie 2-arity mistake).
        for n in 1..=6 {
            fs::write(ct.join(format!("BS_CT.{n}.ht2")), b"x").unwrap();
            fs::write(ga.join(format!("BS_GA.{n}.ht2")), b"x").unwrap();
        }
        fs::write(tmp.path().join("g.fa"), b">c\nA\n").unwrap();
        let err = discover_genome(Aligner::Hisat2, tmp.path()).unwrap_err();
        assert!(matches!(err, AlignerError::FaultyIndex { .. }));
    }

    /// A Bowtie 2 `.bt2` index is NOT accepted in HISAT2 mode (the arity/extension
    /// differ) — guards against a silent Bowtie 2-vs-HISAT2 index mix-up.
    #[test]
    fn bt2_index_rejected_in_hisat2_mode() {
        let tmp = TempDir::new().unwrap();
        make_small_index(tmp.path());
        fs::write(tmp.path().join("g.fa"), b">c\nA\n").unwrap();
        let err = discover_genome(Aligner::Hisat2, tmp.path()).unwrap_err();
        assert!(matches!(err, AlignerError::FaultyIndex { .. }));
    }

    // ---- minimap2 index discovery (Phase 4; V4) ---------------------------

    /// A complete minimap2 index: a single `.mmi` per converted genome + one FASTA.
    fn make_mmi_index(dir: &Path) {
        let ct = dir.join("Bisulfite_Genome").join("CT_conversion");
        let ga = dir.join("Bisulfite_Genome").join("GA_conversion");
        fs::create_dir_all(&ct).unwrap();
        fs::create_dir_all(&ga).unwrap();
        fs::write(ct.join("BS_CT.mmi"), b"x").unwrap();
        fs::write(ga.join("BS_GA.mmi"), b"x").unwrap();
        fs::write(dir.join("genome.fa"), b">chr1\nACGT\n").unwrap();
    }

    /// V4: minimap2 = a SINGLE `.mmi` suffix; `large` is ignored (no `.mmil`).
    #[test]
    fn minimap2_suffix_is_single_mmi() {
        let s = index_suffixes(Aligner::Minimap2, "BS_CT", false);
        assert_eq!(s, vec!["BS_CT.mmi".to_string()]);
        // large flag has no effect for minimap2 (no large-index variant).
        assert_eq!(index_suffixes(Aligner::Minimap2, "BS_CT", true), s);
    }

    /// Phase 3 (T1): rammap is minimap-like — the SAME single `.mmi` suffix;
    /// `large` is ignored (no large-index variant).
    #[test]
    fn rammap_suffix_is_single_mmi() {
        assert_eq!(
            index_suffixes(Aligner::Rammap, "BS_CT", false),
            vec!["BS_CT.mmi".to_string()]
        );
        assert_eq!(
            index_suffixes(Aligner::Rammap, "BS_CT", true),
            vec!["BS_CT.mmi".to_string()]
        );
    }

    /// Phase 3 (T4): a Bowtie 2 `.bt2` index is NOT accepted in rammap mode (no
    /// `.mmi`) — mirror `bt2_index_rejected_in_minimap2_mode` with `Aligner::Rammap`.
    #[test]
    fn bt2_index_rejected_in_rammap_mode() {
        let tmp = TempDir::new().unwrap();
        make_small_index(tmp.path());
        fs::write(tmp.path().join("g.fa"), b">c\nA\n").unwrap();
        let err = discover_genome(Aligner::Rammap, tmp.path()).unwrap_err();
        assert!(matches!(err, AlignerError::FaultyIndex { .. }));
    }

    /// V4: a complete `.mmi` index resolves; `large_index` stays false.
    #[test]
    fn discovers_complete_mmi_index() {
        let tmp = TempDir::new().unwrap();
        make_mmi_index(tmp.path());
        let g = discover_genome(Aligner::Minimap2, tmp.path()).unwrap();
        assert!(!g.large_index);
        assert!(g.ct_index_basename.ends_with("CT_conversion/BS_CT"));
        assert!(g.ga_index_basename.ends_with("GA_conversion/BS_GA"));
    }

    /// V4: a missing `.mmi` errors (reported as a minimap2 faulty index, never a
    /// Bowtie 2 `.bt2`).
    #[test]
    fn missing_mmi_errors_with_minimap2_wording() {
        let tmp = TempDir::new().unwrap();
        make_mmi_index(tmp.path());
        fs::remove_file(
            tmp.path()
                .join("Bisulfite_Genome")
                .join("CT_conversion")
                .join("BS_CT.mmi"),
        )
        .unwrap();
        let err = discover_genome(Aligner::Minimap2, tmp.path()).unwrap_err();
        match err {
            AlignerError::FaultyIndex {
                aligner, missing, ..
            } => {
                assert_eq!(aligner, "minimap2");
                assert!(
                    missing.ends_with(".mmi"),
                    "expected the .mmi, got {missing}"
                );
            }
            other => panic!("expected FaultyIndex, got {other:?}"),
        }
    }

    /// A Bowtie 2 `.bt2` index is NOT accepted in minimap2 mode (no `.mmi`).
    #[test]
    fn bt2_index_rejected_in_minimap2_mode() {
        let tmp = TempDir::new().unwrap();
        make_small_index(tmp.path());
        fs::write(tmp.path().join("g.fa"), b">c\nA\n").unwrap();
        let err = discover_genome(Aligner::Minimap2, tmp.path()).unwrap_err();
        assert!(matches!(err, AlignerError::FaultyIndex { .. }));
    }

    #[test]
    fn extension_match_is_case_sensitive() {
        // Perl `<*.fa>` is case-sensitive on Linux; an uppercase `.FA` must NOT
        // match (mirrors the sibling genome-prep contract for the @SQ order).
        let tmp = TempDir::new().unwrap();
        make_small_index(tmp.path());
        fs::write(tmp.path().join("GENOME.FA"), b">c\nA\n").unwrap();
        let err = discover_genome(Aligner::Bowtie2, tmp.path()).unwrap_err();
        assert!(matches!(err, AlignerError::NoFasta(_)));
    }

    #[cfg(unix)]
    #[test]
    fn follows_symlinked_fasta() {
        let tmp = TempDir::new().unwrap();
        make_small_index(tmp.path());
        let real = tmp.path().join("real.fa");
        fs::write(&real, b">c\nA\n").unwrap();
        std::os::unix::fs::symlink(&real, tmp.path().join("link.fa")).unwrap();
        let g = discover_genome(Aligner::Bowtie2, tmp.path()).unwrap();
        // both the real file and the symlink are followed (is_file()).
        assert_eq!(g.fastas.len(), 2);
    }

    #[test]
    fn matcher_handles_non_utf8_bytes() {
        // A non-UTF-8 name ending in `.fa` (0xFF is invalid UTF-8) must match —
        // the byte-level guarantee that fixes the `to_str()`-drops-non-UTF-8 bug.
        assert!(FastaKind::Fa.matches(&[0xff, b'.', b'f', b'a']));
        assert!(!FastaKind::Fa.matches(&[0xff, b'.', b'f', b'a', b'.', b'g', b'z']));
        assert!(FastaKind::FaGz.matches(&[0xff, b'.', b'f', b'a', b'.', b'g', b'z']));
    }

    // Real non-UTF-8 file on disk: Linux only (macOS/APFS rejects such names).
    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_filename_not_dropped() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let tmp = TempDir::new().unwrap();
        make_small_index(tmp.path());
        let name = OsString::from_vec(vec![0xff, b'.', b'f', b'a']);
        fs::write(tmp.path().join(&name), b">c\nA\n").unwrap();
        let g = discover_genome(Aligner::Bowtie2, tmp.path()).unwrap();
        assert_eq!(g.fasta_kind, FastaKind::Fa);
        assert_eq!(g.fastas.len(), 1);
    }
}
