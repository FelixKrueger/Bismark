//! Single-instance Bowtie 2 stream + SAM-line parse — the lockstep primitive.
//!
//! Spawns ONE Bowtie 2 subprocess against the converted temp FastQ, skips the
//! SAM header, and exposes a *peek + advance* interface (`current()` /
//! `advance()`) over parsed [`SamRecord`]s. Phase 4 drives 2–4 of these in
//! read-ID lockstep for the best-alignment merge; this phase has **no** scoring,
//! strand assignment, `XM` call, or BAM output, and is **not wired into `run()`**.
//!
//! Mirrors Perl `single_end_align_fragments_to_bisulfite_genome_fastQ_bowtie2`
//! (6849–6912: spawn, `^@` header-skip, store-first) and the field/tag
//! extraction in `check_results_single_end` (2737/2773–2795).

use std::ffi::OsString;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};

use crate::aligner::config::Aligner;
use crate::aligner::error::{AlignerError, Result};

/// Per-instance strand-orientation flag (the strand-instance table, Perl 7124).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// `--norc` — CTreadCTgenome / GAreadGAgenome.
    Norc,
    /// `--nofw` — CTreadGAgenome / GAreadCTgenome.
    Nofw,
    /// No strand restriction (`--combined_index`, v2): one both-strands search
    /// over the combined CT+GA index — emits NEITHER `--norc` nor `--nofw`.
    Both,
}

impl Orientation {
    /// The Bowtie 2 strand flag, or `None` for [`Orientation::Both`] (the combined
    /// search restricts no strand → emits no token; callers must NOT push an empty
    /// argument for `None`).
    pub fn flag(self) -> Option<&'static str> {
        match self {
            Orientation::Norc => Some("--norc"),
            Orientation::Nofw => Some("--nofw"),
            Orientation::Both => None,
        }
    }
}

/// A parsed SAM alignment line (the fields Phase 4's scoring needs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamRecord {
    /// QNAME (field 0).
    pub qname: String,
    /// FLAG (field 1).
    pub flag: u16,
    /// RNAME (field 2) — kept **raw**, incl. the `_CT_converted`/`_GA_converted`
    /// suffix; de-conversion happens in Phase 4/5.
    pub rname: String,
    /// POS (field 3).
    pub pos: u32,
    /// MAPQ (field 4).
    pub mapq: u8,
    /// CIGAR (field 5).
    pub cigar: String,
    /// SEQ (field 9).
    pub seq: String,
    /// QUAL (field 10).
    pub qual: String,
    /// `AS:i:` alignment score (≤ 0 for Bowtie 2 end-to-end).
    pub alignment_score: Option<i64>,
    /// `XS:i:` (Bowtie 2) / `ZS:i:` (HISAT2) second-best score.
    pub second_best: Option<i64>,
    /// `MD:Z:` mismatch string.
    pub md_tag: Option<String>,
    /// The **chomped** line (no trailing `\n`/`\r`) — Perl stores `last_line`
    /// post-`chomp` (6898) and `--ambig_bam` re-emits it (2807–08).
    pub raw_line: String,
}

/// Strip the trailing CR/LF terminator then split on TAB, operating on the
/// already-validated `&str`'s bytes. This avoids
/// `line.trim_end_matches(['\n', '\r']).split('\t')`, whose `CharSearcher`
/// (per-char UTF-8 decode) was the aligner's hottest parse cost (06222026 perf
/// epic; ~330 self-samples). The line is valid UTF-8 (read_line validated it)
/// and `\t`/`\n`/`\r` are all < 0x80, so a byte slice at those boundaries never
/// lands mid-codepoint and yields valid `&str` fields with no re-validation.
///
/// **Byte-identical to the char-based reference** by construction (only trailing
/// `\n`/`\r` are trimmed; field bytes are untouched; an empty input yields a
/// single empty field, like `"".split('\t')`). Returned as a seam so the
/// equivalence is pinned by a CI unit test (`trim_and_split_*`), not just the
/// `#[ignore]`d real-data oracle. The returned `trimmed` slice backs both the
/// fields and `raw_line`.
fn trim_and_split(line: &str) -> (&str, Vec<&str>) {
    let lb = line.as_bytes();
    let mut end = lb.len();
    while end > 0 && (lb[end - 1] == b'\n' || lb[end - 1] == b'\r') {
        end -= 1;
    }
    let trimmed = &line[..end];
    let mut f: Vec<&str> = Vec::with_capacity(16);
    let mut start = 0usize;
    for (i, &b) in trimmed.as_bytes().iter().enumerate() {
        if b == b'\t' {
            f.push(&trimmed[start..i]);
            start = i + 1;
        }
    }
    f.push(&trimmed[start..]);
    (trimmed, f)
}

impl SamRecord {
    /// Parse one SAM line (`split('\t')`). The line may carry a trailing
    /// terminator, which is stripped. Errors on `< 11` fields or unparseable
    /// FLAG/POS/MAPQ; unparseable tag values are left `None` (lenient — Phase 4
    /// enforces `AS`/`MD` presence, Perl `die` 2838).
    pub fn parse(line: &str) -> Result<SamRecord> {
        let (trimmed, f) = trim_and_split(line);
        if f.len() < 11 {
            return Err(AlignerError::Validation(format!(
                "malformed SAM line ({} fields, expected >= 11): {trimmed}",
                f.len()
            )));
        }
        let flag = f[1]
            .parse::<u16>()
            .map_err(|_| AlignerError::Validation(format!("bad SAM FLAG '{}'", f[1])))?;
        let pos = f[3]
            .parse::<u32>()
            .map_err(|_| AlignerError::Validation(format!("bad SAM POS '{}'", f[3])))?;
        let mapq = f[4]
            .parse::<u8>()
            .map_err(|_| AlignerError::Validation(format!("bad SAM MAPQ '{}'", f[4])))?;

        // Optional tags, scanned in field order. Prefixes are disjoint, so
        // `second_best` is simply overwritten on each XS/ZS match (last wins) —
        // matches Perl setting it at 2780 (ZS) and 2788 (XS) as fields advance.
        //
        // 🔴 minimap2's within-instance second-best tag is `s2:i:` (lowercase) —
        // it is INTENTIONALLY NOT captured here. Bismark's parse loop (Perl
        // 2772-2796) has no `s2` branch, so `second_best` stays `None` for minimap2
        // → `calc_mapq` takes the no-2nd-best path, byte-identical to Perl. Adding
        // an `s2:i:` branch (as the Phase-3 spike WRONGLY suggested) would silently
        // break MAPQ byte-identity. `minimap2_s2_tag_is_ignored` (below) guards this.
        let (mut alignment_score, mut second_best, mut md_tag) = (None, None, None);
        for fld in &f[11..] {
            if let Some(v) = fld.strip_prefix("AS:i:") {
                alignment_score = v.parse::<i64>().ok();
            } else if let Some(v) = fld
                .strip_prefix("XS:i:")
                .or_else(|| fld.strip_prefix("ZS:i:"))
            {
                second_best = v.parse::<i64>().ok();
            } else if let Some(v) = fld.strip_prefix("MD:Z:") {
                md_tag = Some(v.to_string());
            }
        }

        Ok(SamRecord {
            qname: f[0].to_string(),
            flag,
            rname: f[2].to_string(),
            pos,
            mapq,
            cigar: f[5].to_string(),
            seq: f[9].to_string(),
            qual: f[10].to_string(),
            alignment_score,
            second_best,
            md_tag,
            raw_line: trimmed.to_string(),
        })
    }

    /// SE-unmapped test (Perl 2739: `flag == 4`). PE differs (Phase 7).
    pub fn is_unmapped(&self) -> bool {
        self.flag == 4
    }
}

/// The peek/advance interface the Phase-4 merge drives. Implemented by
/// [`AlignerStream`] (a real Bowtie 2 subprocess) and by test doubles, so the
/// N-way merge can be unit-tested with canned records.
pub trait SamStream {
    /// Peek the current record (`None` at EOF).
    fn current(&self) -> Option<&SamRecord>;
    /// Advance to the next record.
    fn advance(&mut self) -> Result<()>;
}

impl SamStream for AlignerStream {
    fn current(&self) -> Option<&SamRecord> {
        AlignerStream::current(self)
    }
    fn advance(&mut self) -> Result<()> {
        AlignerStream::advance(self)
    }
}

/// A live single aligner instance (Bowtie 2 / HISAT2 / minimap2), presenting a
/// peek/advance SAM stream.
pub struct AlignerStream {
    child: Child,
    reader: BufReader<ChildStdout>,
    current: Option<SamRecord>,
    line_buf: String,
    finished: bool,
    /// The backend, for aligner-aware error messages (never-silent: a HISAT2 failure
    /// must not report "Bowtie 2"). Mirrors [`PairedAlignerStream`]'s field — the SE
    /// sibling lagged until the v2.x combined-index sequential path drove HISAT2 SE
    /// through this stream (PLAN 06102026 phase 7).
    aligner: Aligner,
}

/// Build the per-instance argv (excluding the binary path) for one single-end
/// alignment. Extracted as a pure function so the per-aligner invocation shape is
/// unit-testable without spawning a process (and so the frozen Bowtie 2 / HISAT2
/// shape is physically separated from minimap2's).
///
/// - **Bowtie 2 / HISAT2** (Perl 6872-6882): `<opts…> <orient> -x <index> -U <input>`
///   — the strand flag (`--norc`/`--nofw`) plus the `-x basename` / `-U reads` shape.
/// - **minimap2** (Perl 7022/7025): `<opts…> <index>.mmi <input>` — the index is
///   passed **positionally** as `<basename>.mmi` with NO strand flag and NO
///   `-x`/`-U` (Bismark comments `--norc`/`--nofw` out, 7011-7016). The `.mmi` is a
///   literal byte append to the basename (`$bisulfiteIndex.".mmi"`). NB: the
///   minimap2 *options* legitimately contain `-x <preset>` (e.g. `-x map-ont`) —
///   that is the preset, distinct from the Bowtie 2 `-x <index>` shape.
fn build_se_argv(
    aligner: Aligner,
    options: &str,
    orient: Orientation,
    index: &Path,
    input: &Path,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = options.split_whitespace().map(OsString::from).collect();
    match aligner {
        Aligner::Bowtie2 | Aligner::Hisat2 => {
            // `Orientation::Both` (combined index) emits no strand flag — push the
            // token only when there is one (never an empty arg).
            if let Some(f) = orient.flag() {
                args.push(f.into());
            }
            args.push("-x".into());
            args.push(index.as_os_str().to_owned());
            args.push("-U".into());
            args.push(input.as_os_str().to_owned());
        }
        // rammap is minimap-like — the identical positional `<index>.mmi <input>`
        // invocation shape (NO strand flag, NO `-x <index>`/`-U`).
        Aligner::Minimap2 | Aligner::Rammap => {
            let mut mmi = index.as_os_str().to_owned();
            mmi.push(".mmi");
            args.push(mmi);
            args.push(input.as_os_str().to_owned());
        }
    }
    args
}

/// Build the per-instance argv (excluding the binary path) for one paired-end
/// alignment — the PE analog of [`build_se_argv`]. Bowtie 2 + HISAT2 share the shape
/// `<opts…> <orient> -x <index> -1 <input1> -2 <input2>` (Perl 6474); the argv is
/// **identical** for the two backends (only the options string differs), so this is
/// byte-neutral vs the previous inline Bowtie-2-named spawn. minimap2 paired-end is
/// rejected at resolve (no trustworthy oracle, `config.rs`), so it is unreachable here.
fn build_pe_argv(
    aligner: Aligner,
    options: &str,
    orient: Orientation,
    index: &Path,
    input1: &Path,
    input2: &Path,
) -> Vec<OsString> {
    match aligner {
        Aligner::Bowtie2 | Aligner::Hisat2 => {
            let mut args: Vec<OsString> = options.split_whitespace().map(OsString::from).collect();
            // `Orientation::Both` (combined index) emits no strand flag.
            if let Some(f) = orient.flag() {
                args.push(f.into());
            }
            args.push("-x".into());
            args.push(index.as_os_str().to_owned());
            args.push("-1".into());
            args.push(input1.as_os_str().to_owned());
            args.push("-2".into());
            args.push(input2.as_os_str().to_owned());
            args
        }
        // minimap2 + rammap (minimap-like) paired-end are both rejected at resolve
        // (SE-only; no trustworthy oracle), so neither reaches build_pe_argv.
        Aligner::Minimap2 | Aligner::Rammap => unreachable!(
            "minimap2 paired-end (and the minimap-like rammap) is rejected at resolve \
             (no trustworthy oracle); build_pe_argv must not be reached for minimap2/rammap"
        ),
    }
}

impl AlignerStream {
    /// Spawn one aligner instance (Bowtie 2 / HISAT2 / minimap2) and read up to the
    /// first alignment record. The argv shape is per-aligner ([`build_se_argv`]).
    /// stdout is piped; stderr is **inherited** (the aligner's summary → terminal,
    /// as in Perl — so only stdout is piped and it is always drained).
    pub fn spawn(
        aligner: Aligner,
        bin: &Path,
        options: &str,
        orient: Orientation,
        index: &Path,
        input: &Path,
    ) -> Result<Self> {
        let mut cmd = Command::new(bin);
        cmd.args(build_se_argv(aligner, options, orient, index, input))
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd.spawn().map_err(|e| {
            AlignerError::Validation(format!(
                "failed to spawn {} ({}): {e}",
                aligner.name(),
                bin.display()
            ))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AlignerError::Validation(format!("{} stdout was not captured", aligner.name()))
        })?;
        let mut reader = BufReader::new(stdout);

        // Skip `@` header lines; the first non-`@` line is the first record.
        let mut line = String::new();
        let current = loop {
            line.clear();
            let n = reader.read_line(&mut line)?;
            if n == 0 {
                break None; // header-only / empty stream
            }
            if line.starts_with('@') {
                continue;
            }
            break Some(SamRecord::parse(&line)?);
        };

        Ok(AlignerStream {
            child,
            reader,
            current,
            line_buf: String::new(),
            finished: false,
            aligner,
        })
    }

    /// Peek the current record without consuming it (`None` at EOF).
    pub fn current(&self) -> Option<&SamRecord> {
        self.current.as_ref()
    }

    /// Advance to the next record (sets `current` to `None` at EOF).
    pub fn advance(&mut self) -> Result<()> {
        self.line_buf.clear();
        let n = self.reader.read_line(&mut self.line_buf)?;
        self.current = if n == 0 {
            None
        } else {
            Some(SamRecord::parse(&self.line_buf)?)
        };
        Ok(())
    }

    /// Drain any remaining stdout, reap the child, and check its exit status.
    /// Draining first avoids deadlocking the child on a full stdout pipe when
    /// the caller stopped early (Phase-4 early-stop). Non-zero exit → error
    /// (an intentional fail-closed deviation from Perl's fail-open pipe close).
    pub fn finish(mut self) -> Result<()> {
        std::io::copy(&mut self.reader, &mut std::io::sink())?;
        let status = self.child.wait()?;
        self.finished = true;
        if status.success() {
            Ok(())
        } else {
            Err(AlignerError::Validation(format!(
                "{} exited unsuccessfully ({status})",
                self.aligner.name()
            )))
        }
    }
}

impl Drop for AlignerStream {
    fn drop(&mut self) {
        // If not finished cleanly, kill THEN wait — kill alone leaves a zombie.
        if !self.finished {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// A file-backed [`SamStream`]: replays SAM records (one per line) from a spilled
/// alignment file, presenting the same peek/advance interface as [`AlignerStream`].
///
/// Used by the **sequential** non-directional combined-index path (v2 phase 9): pass
/// 1's Bowtie 2 output is spilled to a temp file, its process exits (freeing the
/// combined index), then pass 2 runs while this stream replays pass 1's records —
/// letting the (signature-widened) two-stream `drive_merge_combined_nondir` merge a
/// disk-backed C→T stream against a live G→A stream, one index resident at a time.
///
/// Reads **line-by-line over a `BufReader` with a reused buffer** (NOT a slurp) so
/// peak memory stays O(1) in the file size — the whole point of spilling to disk is
/// to NOT hold pass 1's records in RAM. `@` header lines are skipped for symmetry
/// with [`AlignerStream`] (the spill writes only records, so in practice there are
/// none, but a header would be tolerated).
pub struct FileSamStream {
    reader: BufReader<noodles_bgzf::io::Reader<File>>,
    current: Option<SamRecord>,
    line_buf: String,
}

impl FileSamStream {
    /// Open a spilled SAM-record file and read up to the first record (`@` lines
    /// skipped; `None` at EOF / empty file). Mirrors [`AlignerStream::spawn`]'s
    /// header-skip + store-first.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| {
            AlignerError::Validation(format!(
                "failed to open spilled alignment file ({}): {e}",
                path.display()
            ))
        })?;
        let mut reader = BufReader::new(noodles_bgzf::io::Reader::new(file)); // BGZF spill (issue #1019)
        let mut line = String::new();
        let current = loop {
            line.clear();
            let n = reader.read_line(&mut line)?;
            if n == 0 {
                break None; // empty / header-only
            }
            if line.starts_with('@') {
                continue;
            }
            break Some(SamRecord::parse(&line)?);
        };
        Ok(FileSamStream {
            reader,
            current,
            line_buf: String::new(),
        })
    }

    /// Peek the current record without consuming it (`None` at EOF).
    pub fn current(&self) -> Option<&SamRecord> {
        self.current.as_ref()
    }

    /// Advance to the next record (sets `current` to `None` at EOF). The spill
    /// writes only records, so unlike [`open`](Self::open) there is no header to
    /// skip mid-stream.
    pub fn advance(&mut self) -> Result<()> {
        self.line_buf.clear();
        let n = self.reader.read_line(&mut self.line_buf)?;
        self.current = if n == 0 {
            None
        } else {
            Some(SamRecord::parse(&self.line_buf)?)
        };
        Ok(())
    }
}

impl SamStream for FileSamStream {
    fn current(&self) -> Option<&SamRecord> {
        FileSamStream::current(self)
    }
    fn advance(&mut self) -> Result<()> {
        FileSamStream::advance(self)
    }
}

// ===========================================================================
// Paired-end stream (Phase 7) — peek-TWO / advance-TWO.
// ===========================================================================

/// One paired-end alignment: read 1 + read 2 records, **canonicalised** so
/// `read1` is always the `/1` mate regardless of which line Bowtie 2 emitted
/// first (it reports the leftmost-position mate first — Perl 6494–6508), plus
/// the `/1`-stripped `seq_id` used for the read-ID lockstep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamPair {
    /// Read 1 (the `/1` mate).
    pub read1: SamRecord,
    /// Read 2 (the `/2` mate).
    pub read2: SamRecord,
    /// QNAME with the trailing `/1` removed (Perl 6500–6508).
    pub seq_id: String,
}

impl SamPair {
    /// Build a pair from two raw SAM lines, identifying read 1 by the trailing
    /// `/1` on its QNAME (Perl 6500–6508). Bowtie 2 strips the outer `/1`,`/2` we
    /// added as `/1/1`,`/2/2`, leaving `/1`,`/2`. `die` if neither line is read 1.
    pub(crate) fn from_lines(line1: &str, line2: &str) -> Result<SamPair> {
        let r1 = SamRecord::parse(line1)?;
        let r2 = SamRecord::parse(line2)?;
        if let Some(id) = r1.qname.strip_suffix("/1") {
            let seq_id = id.to_string();
            Ok(SamPair {
                read1: r1,
                read2: r2,
                seq_id,
            })
        } else if let Some(id) = r2.qname.strip_suffix("/1") {
            let seq_id = id.to_string();
            // read 2's line was emitted first → swap so `read1` is the /1 mate.
            Ok(SamPair {
                read1: r2,
                read2: r1,
                seq_id,
            })
        } else {
            Err(AlignerError::Validation(format!(
                "Either the first or the second id need to be read 1! ID1 was: {}; ID2 was: {}",
                r1.qname, r2.qname
            )))
        }
    }

    /// The PE no-alignment marker: read 1 FLAG 77 (1+4+8+64) and read 2 FLAG 141
    /// (1+4+8+128) (Perl 3317). Distinct from SE's single `flag == 4`.
    pub fn is_unmapped_pair(&self) -> bool {
        self.read1.flag == 77 && self.read2.flag == 141
    }
}

/// The paired peek/advance interface the Phase-7 merge drives. Implemented by
/// [`PairedAlignerStream`] (a real Bowtie 2 `-1/-2` subprocess) and by test
/// doubles, so `check_results_paired_end` can be unit-tested with canned pairs.
pub trait PairedSamStream {
    /// Peek the current pair (`None` at EOF).
    fn current_pair(&self) -> Option<&SamPair>;
    /// Advance to the next pair (two SAM lines).
    fn advance_pair(&mut self) -> Result<()>;
}

impl PairedSamStream for PairedAlignerStream {
    fn current_pair(&self) -> Option<&SamPair> {
        PairedAlignerStream::current_pair(self)
    }
    fn advance_pair(&mut self) -> Result<()> {
        PairedAlignerStream::advance_pair(self)
    }
}

/// A live paired-end aligner instance (Bowtie 2 / HISAT2), presenting a peek/advance
/// pair stream. Each pair is two consecutive SAM lines (R1 + R2, in the aligner's
/// leftmost-first order); [`SamPair::from_lines`] canonicalises them to (read1, read2).
/// (minimap2 PE is rejected at resolve, so it never reaches here.)
pub struct PairedAlignerStream {
    child: Child,
    reader: BufReader<ChildStdout>,
    current: Option<SamPair>,
    finished: bool,
    /// The backend, for aligner-aware error messages (never-silent: a HISAT2 failure
    /// must not report "Bowtie 2").
    aligner: Aligner,
}

impl PairedAlignerStream {
    /// Spawn one paired aligner instance (Bowtie 2 / HISAT2) and read the first pair.
    ///
    /// Args mirror Perl 6474: `<aligner_options> <orient> -x <index> -1 <input1>
    /// -2 <input2>` ([`build_pe_argv`]) — identical for Bowtie 2 and HISAT2. stdout
    /// piped; stderr inherited (only stdout is drained).
    pub fn spawn(
        aligner: Aligner,
        bin: &Path,
        options: &str,
        orient: Orientation,
        index: &Path,
        input1: &Path,
        input2: &Path,
    ) -> Result<Self> {
        // Orientation flag: the faithful directional/pbat/non-dir PE slots pass
        // `Norc`/`Nofw` (`flag()` → `Some`); the v2.x combined-index PE pass uses
        // `Orientation::Both` (`flag()` → `None`) so ONE pass searches both sub-genomes.
        let mut cmd = Command::new(bin);
        cmd.args(build_pe_argv(
            aligner, options, orient, index, input1, input2,
        ))
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

        let mut child = cmd.spawn().map_err(|e| {
            AlignerError::Validation(format!(
                "failed to spawn {} ({}): {e}",
                aligner.name(),
                bin.display()
            ))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AlignerError::Validation(format!("{} stdout was not captured", aligner.name()))
        })?;
        let mut reader = BufReader::new(stdout);

        // Skip `@` header lines; the first non-`@` line is the first record of
        // the first pair (Perl 6477–6488).
        let mut line1 = String::new();
        loop {
            line1.clear();
            let n = reader.read_line(&mut line1)?;
            if n == 0 {
                // header-only / empty stream → no pairs
                return Ok(PairedAlignerStream {
                    child,
                    reader,
                    current: None,
                    finished: false,
                    aligner,
                });
            }
            if !line1.starts_with('@') {
                break;
            }
        }
        let mut line2 = String::new();
        let n2 = reader.read_line(&mut line2)?;
        let current = if n2 == 0 {
            None // a lone trailing line is not a complete pair (Perl 6491)
        } else {
            Some(SamPair::from_lines(&line1, &line2)?)
        };

        Ok(PairedAlignerStream {
            child,
            reader,
            current,
            finished: false,
            aligner,
        })
    }

    /// Peek the current pair without consuming it (`None` at EOF).
    pub fn current_pair(&self) -> Option<&SamPair> {
        self.current.as_ref()
    }

    /// Advance to the next pair (reads two lines; `None` at EOF / a lone line).
    pub fn advance_pair(&mut self) -> Result<()> {
        let mut line1 = String::new();
        let n1 = self.reader.read_line(&mut line1)?;
        if n1 == 0 {
            self.current = None;
            return Ok(());
        }
        let mut line2 = String::new();
        let n2 = self.reader.read_line(&mut line2)?;
        self.current = if n2 == 0 {
            None
        } else {
            Some(SamPair::from_lines(&line1, &line2)?)
        };
        Ok(())
    }

    /// Drain remaining stdout, reap the child, check exit status (as [`AlignerStream::finish`]).
    pub fn finish(mut self) -> Result<()> {
        std::io::copy(&mut self.reader, &mut std::io::sink())?;
        let status = self.child.wait()?;
        self.finished = true;
        if status.success() {
            Ok(())
        } else {
            Err(AlignerError::Validation(format!(
                "{} exited unsuccessfully ({status})",
                self.aligner.name()
            )))
        }
    }
}

impl Drop for PairedAlignerStream {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// A spilled paired-end SAM-record file presented as a [`PairedSamStream`] — the PE
/// analog of [`FileSamStream`] (Phase 6, the sequential low-RSS PE variant). Reads
/// **two** lines per pair → [`SamPair::from_lines`] (which re-canonicalises read1/read2
/// regardless of the spill order). The whole point of spilling pass 1 to disk is to
/// replay it against the live pass-2 stream WITHOUT holding the index resident — so the
/// file is streamed one pair at a time (peak memory O(1) in the file size). `@` header
/// lines are skipped for symmetry with [`PairedAlignerStream::spawn`] (the spill writes
/// only records, so in practice there are none, but a header would be tolerated); a
/// **lone trailing line** (odd line count → no second mate) yields `None`, mirroring
/// [`PairedAlignerStream::advance_pair`] (Perl 6491), NOT an error.
pub struct PairedFileSamStream {
    reader: BufReader<noodles_bgzf::io::Reader<File>>,
    current: Option<SamPair>,
}

impl PairedFileSamStream {
    /// Open a spilled PE SAM-record file and read up to the first pair (`@` lines
    /// skipped; `None` at EOF / empty / header-only / a lone trailing line). Mirrors
    /// [`PairedAlignerStream::spawn`]'s header-skip + first-pair read, file-backed.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| {
            AlignerError::Validation(format!(
                "failed to open spilled paired-end alignment file ({}): {e}",
                path.display()
            ))
        })?;
        let mut reader = BufReader::new(noodles_bgzf::io::Reader::new(file)); // BGZF spill (issue #1019)
        // Skip `@` header lines; the first non-`@` line is the first record of the
        // first pair (cf. `PairedAlignerStream::spawn`).
        let mut line1 = String::new();
        let current = loop {
            line1.clear();
            let n1 = reader.read_line(&mut line1)?;
            if n1 == 0 {
                break None; // empty / header-only
            }
            if line1.starts_with('@') {
                continue;
            }
            let mut line2 = String::new();
            let n2 = reader.read_line(&mut line2)?;
            // A lone trailing line is not a complete pair (Perl 6491) → no pairs.
            break if n2 == 0 {
                None
            } else {
                Some(SamPair::from_lines(&line1, &line2)?)
            };
        };
        Ok(PairedFileSamStream { reader, current })
    }

    /// Peek the current pair without consuming it (`None` at EOF).
    pub fn current_pair(&self) -> Option<&SamPair> {
        self.current.as_ref()
    }

    /// Advance to the next pair (reads two lines; `None` at EOF / a lone line). The
    /// spill writes only records, so — unlike [`open`](Self::open) — there is no header
    /// to skip mid-stream.
    pub fn advance_pair(&mut self) -> Result<()> {
        let mut line1 = String::new();
        let n1 = self.reader.read_line(&mut line1)?;
        if n1 == 0 {
            self.current = None;
            return Ok(());
        }
        let mut line2 = String::new();
        let n2 = self.reader.read_line(&mut line2)?;
        self.current = if n2 == 0 {
            None
        } else {
            Some(SamPair::from_lines(&line1, &line2)?)
        };
        Ok(())
    }
}

impl PairedSamStream for PairedFileSamStream {
    fn current_pair(&self) -> Option<&SamPair> {
        PairedFileSamStream::current_pair(self)
    }
    fn advance_pair(&mut self) -> Result<()> {
        PairedFileSamStream::advance_pair(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAPPED: &str = "r1\t0\tchr1_CT_converted\t100\t40\t10M\t*\t0\t0\tACGTACGTAC\tIIIIIIIIII\tAS:i:0\tXS:i:-12\tMD:Z:10";

    #[test]
    fn parse_core_fields() {
        let r = SamRecord::parse(MAPPED).unwrap();
        assert_eq!(r.qname, "r1");
        assert_eq!(r.flag, 0);
        assert_eq!(r.rname, "chr1_CT_converted"); // suffix kept raw
        assert_eq!(r.pos, 100);
        assert_eq!(r.mapq, 40);
        assert_eq!(r.cigar, "10M");
        assert_eq!(r.seq, "ACGTACGTAC"); // index 9, not an earlier field
        assert_eq!(r.qual, "IIIIIIIIII"); // index 10
        assert!(!r.is_unmapped());
    }

    #[test]
    fn parse_tags() {
        let r = SamRecord::parse(MAPPED).unwrap();
        assert_eq!(r.alignment_score, Some(0));
        assert_eq!(r.second_best, Some(-12)); // negative AS/XS accepted
        assert_eq!(r.md_tag.as_deref(), Some("10"));
    }

    #[test]
    fn parse_negative_as_and_hisat2_zs() {
        let line = "r\t0\tc_CT_converted\t1\t40\t4M\t*\t0\t0\tACGT\tIIII\tAS:i:-6\tZS:i:-9\tMD:Z:4";
        let r = SamRecord::parse(line).unwrap();
        assert_eq!(r.alignment_score, Some(-6));
        assert_eq!(r.second_best, Some(-9)); // ZS:i: (HISAT2) feeds second_best too
    }

    #[test]
    fn both_xs_and_zs_last_wins() {
        let line = "r\t0\tc_CT_converted\t1\t40\t4M\t*\t0\t0\tACGT\tIIII\tAS:i:0\tXS:i:-5\tZS:i:-9";
        let r = SamRecord::parse(line).unwrap();
        assert_eq!(r.second_best, Some(-9)); // last XS/ZS field in order wins
    }

    #[test]
    fn unique_alignment_has_no_second_best() {
        let line = "r\t0\tc_CT_converted\t1\t40\t4M\t*\t0\t0\tACGT\tIIII\tAS:i:0\tMD:Z:4";
        let r = SamRecord::parse(line).unwrap();
        assert_eq!(r.second_best, None);
    }

    #[test]
    fn unmapped_record() {
        let line = "r\t4\t*\t0\t0\t*\t*\t0\t0\tACGT\tIIII";
        let r = SamRecord::parse(line).unwrap();
        assert!(r.is_unmapped());
        assert_eq!(r.alignment_score, None); // no AS/MD required when unmapped
        assert_eq!(r.md_tag, None);
    }

    #[test]
    fn short_line_errors() {
        assert!(SamRecord::parse("r\t0\tchr\t1\t40").is_err());
    }

    #[test]
    fn mapped_record_missing_as_md_parses_to_none() {
        // A MAPPED read (flag 0) with NO AS:i:/MD:Z: parses leniently to None —
        // it must NOT die here (Phase 4 enforces presence). Distinct from the
        // unmapped case, where missing tags are legitimate.
        let line = "r\t0\tc_CT_converted\t1\t40\t4M\t*\t0\t0\tACGT\tIIII";
        let r = SamRecord::parse(line).unwrap();
        assert!(!r.is_unmapped());
        assert_eq!(r.alignment_score, None);
        assert_eq!(r.md_tag, None);
    }

    #[test]
    fn realistic_line_with_mate_fields_and_trailing_md() {
        // RNEXT/PNEXT/TLEN populated (fields 6/7/8) + ignored tags (YT:Z:, NM:i:)
        // before MD:Z: last — guards the SEQ/QUAL index split (9/10) and tag scan.
        let line = "r1\t0\tchr2_CT_converted\t500\t42\t8M\t=\t650\t150\tACGTACGT\tFFFFFFFF\tAS:i:-3\tYT:Z:UU\tNM:i:1\tMD:Z:3A4";
        let r = SamRecord::parse(line).unwrap();
        assert_eq!(r.seq, "ACGTACGT"); // field 9, not RNEXT/PNEXT/TLEN
        assert_eq!(r.qual, "FFFFFFFF"); // field 10
        assert_eq!(r.alignment_score, Some(-3));
        assert_eq!(r.md_tag.as_deref(), Some("3A4")); // found despite YT/NM between
        assert_eq!(r.second_best, None);
    }

    #[test]
    fn md_tag_with_mismatch_letters() {
        let line =
            "r\t0\tc_CT_converted\t1\t40\t10M\t*\t0\t0\tACGTACGTAC\tIIIIIIIIII\tAS:i:-6\tMD:Z:5A4";
        let r = SamRecord::parse(line).unwrap();
        assert_eq!(r.md_tag.as_deref(), Some("5A4"));
    }

    #[test]
    fn malformed_numeric_fields_error() {
        // 11 fields present, but FLAG / POS / MAPQ are non-numeric → parse error.
        assert!(SamRecord::parse("r\tXX\tc\t1\t40\t4M\t*\t0\t0\tA\tI").is_err());
        assert!(SamRecord::parse("r\t0\tc\tXX\t40\t4M\t*\t0\t0\tA\tI").is_err());
        assert!(SamRecord::parse("r\t0\tc\t1\tXX\t4M\t*\t0\t0\tA\tI").is_err());
    }

    #[test]
    fn crlf_trimmed_and_raw_line_clean() {
        let r = SamRecord::parse(&format!("{MAPPED}\r\n")).unwrap();
        assert_eq!(r.qual, "IIIIIIIIII"); // no trailing \r on QUAL
        assert!(!r.raw_line.ends_with('\r') && !r.raw_line.ends_with('\n'));
        assert_eq!(r.raw_line, MAPPED);
    }

    #[test]
    fn trim_and_split_matches_char_based_reference() {
        // The byte-scan field split (06222026 perf epic) MUST be byte-identical to
        // the original `line.trim_end_matches(['\n', '\r']).split('\t')`. This pins
        // the edge cases reviewed by hand on PR #1013 so CI guards them (the
        // real-data oracle that also covers them is `#[ignore]`d): empty / leading /
        // trailing-tab fields, lone `\r`, `\r\n`, mid-line `\r` (NOT trailing, so it
        // stays inside the field), no trailing newline, and empty input (→ `[""]`).
        for line in [
            "",            // empty → one empty field
            "\n",          // bare LF → one empty field
            "\r",          // bare CR → one empty field
            "\r\n",        // CRLF → one empty field
            "\n\r",        // both terminators trimmed
            "a",           // single field, no terminator
            "a\n",         // single field + LF
            "a\r\n",       // single field + CRLF
            "\ta",         // leading tab → empty first field
            "a\t",         // trailing tab → empty last field
            "\t",          // lone tab → two empty fields
            "a\t\tb",      // empty middle field
            "a\tb\tc",     // ordinary three fields
            "a\tb\tc\r\n", // three fields + CRLF
            "a\rb",        // mid-line CR is content, not trimmed
            "a\tb\rc\n",   // mid-line CR inside field 2, trailing LF trimmed
        ] {
            let reference: Vec<&str> = line.trim_end_matches(['\n', '\r']).split('\t').collect();
            let (trimmed, got) = trim_and_split(line);
            assert_eq!(got, reference, "field split mismatch for {line:?}");
            assert_eq!(
                trimmed,
                line.trim_end_matches(['\n', '\r']),
                "trim mismatch"
            );
            assert!(
                !got.is_empty(),
                "split always yields >= 1 field (even empty input)"
            );
        }
    }

    // ---- FileSamStream (v2 phase 9 — sequential combined-index spill replay) ----

    /// Replays a spilled file (a mapped record + a FLAG-4 miss) through the same
    /// peek/advance interface as a live `AlignerStream`, then EOF.
    #[test]
    fn file_sam_stream_replays_records_incl_unmapped() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("ct_pass.sam");
        let miss = "r2\t4\t*\t0\t0\t*\t*\t0\t0\tACGTAC\tFFFFFF";
        write_bgzf(&p, format!("{MAPPED}\n{miss}\n").as_bytes());

        let mut s = FileSamStream::open(&p).unwrap();
        assert_eq!(s.current().unwrap().qname, "r1");
        assert_eq!(s.current().unwrap().raw_line, MAPPED); // lossless
        s.advance().unwrap();
        assert_eq!(s.current().unwrap().qname, "r2");
        assert!(s.current().unwrap().is_unmapped()); // FLAG-4 survives the round-trip
        s.advance().unwrap();
        assert!(s.current().is_none()); // EOF
    }

    /// An empty file → immediately `None`; a header-only file → `None` (the `@`
    /// skip); a header-then-record file → the record is found.
    #[test]
    fn file_sam_stream_empty_and_header_skip() {
        let dir = tempfile::TempDir::new().unwrap();

        let empty = dir.path().join("empty.sam");
        write_bgzf(&empty, b"");
        assert!(FileSamStream::open(&empty).unwrap().current().is_none());

        let hdr = dir.path().join("hdronly.sam");
        write_bgzf(&hdr, b"@HD\tVN:1.0\n");
        assert!(FileSamStream::open(&hdr).unwrap().current().is_none());

        let mix = dir.path().join("mix.sam");
        write_bgzf(&mix, format!("@HD\tVN:1.0\n{MAPPED}\n").as_bytes());
        assert_eq!(
            FileSamStream::open(&mix).unwrap().current().unwrap().qname,
            "r1"
        );
    }

    /// V6 (Phase 4 — guards the spike's WRONG "read s2" instruction): a real
    /// minimap2 tag set incl. a positive `AS:i:` and an `s2:i:` second-best chaining
    /// score must yield `second_best == None` (Bismark IGNORES `s2` — no `s2` branch
    /// in Perl 2772-2796). Adding an `s2` branch would silently break MAPQ identity.
    #[test]
    fn minimap2_s2_tag_is_ignored() {
        // minimap2 primary-record tags (incl. the lowercase `s2:i:`).
        let line = "r\t0\tc_CT_converted\t100\t60\t10M\t*\t0\t0\tACGTACGTAC\tIIIIIIIIII\t\
            NM:i:0\tms:i:20\tAS:i:20\tnn:i:0\ttp:A:P\tcm:i:3\ts1:i:18\ts2:i:14\tde:f:0\trl:i:0\tMD:Z:10";
        let r = SamRecord::parse(line).unwrap();
        assert_eq!(r.alignment_score, Some(20)); // positive AS captured (no sign assumption)
        assert_eq!(r.md_tag.as_deref(), Some("10")); // MD captured despite the noise tags
        assert_eq!(r.second_best, None); // s2:i:14 IGNORED — it is neither XS nor ZS
    }

    /// Phase 3 (T4, design#2): rammap's SAM tag set is identical to minimap2 — it
    /// emits the lowercase `s2:i:` chaining second-best (NOT `XS`/`ZS`), so the
    /// parser ignores it BY CONSTRUCTION → `second_best == None` (no parser change).
    #[test]
    fn rammap_s2_tag_is_ignored() {
        let rec = SamRecord::parse(
            "r\t0\tchr1\t100\t60\t50M\t*\t0\t0\tACGTACGT\tIIIIIIII\tAS:i:90\ts2:i:40\tMD:Z:50",
        )
        .unwrap();
        assert_eq!(rec.alignment_score, Some(90));
        assert_eq!(rec.second_best, None);
    }

    // ---- per-aligner SE invocation shape (Phase 4; V5 / V5b) ---------------

    fn argv_strings(argv: &[OsString]) -> Vec<String> {
        argv.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    /// V5b regression: the Bowtie 2 argv is byte-frozen through the refactored
    /// builder — `<opts> <orient> -x <index> -U <input>`.
    #[test]
    fn se_argv_bowtie2_shape_frozen() {
        let argv = build_se_argv(
            Aligner::Bowtie2,
            "-q --score-min L,0,-0.2 --ignore-quals",
            Orientation::Norc,
            Path::new("/idx/BS_CT"),
            Path::new("/tmp/r_C_to_T.fastq"),
        );
        assert_eq!(
            argv_strings(&argv),
            vec![
                "-q",
                "--score-min",
                "L,0,-0.2",
                "--ignore-quals",
                "--norc",
                "-x",
                "/idx/BS_CT",
                "-U",
                "/tmp/r_C_to_T.fastq",
            ]
        );
    }

    /// V5b: HISAT2 uses the SAME shape as Bowtie 2 (only the options differ); the
    /// `--nofw` strand flag is honoured.
    #[test]
    fn se_argv_hisat2_same_shape_as_bowtie2() {
        let argv = build_se_argv(
            Aligner::Hisat2,
            "-q --no-softclip --omit-sec-seq",
            Orientation::Nofw,
            Path::new("/idx/BS_GA"),
            Path::new("/tmp/r.fastq"),
        );
        assert_eq!(
            argv_strings(&argv),
            vec![
                "-q",
                "--no-softclip",
                "--omit-sec-seq",
                "--nofw",
                "-x",
                "/idx/BS_GA",
                "-U",
                "/tmp/r.fastq",
            ]
        );
    }

    /// V5: minimap2 = positional `<index>.mmi <input>` — NO strand flag, NO
    /// `-x <index>`/`-U` (the `-x map-ont` is the preset, not the index). The
    /// orientation is ignored (Perl comments `--norc`/`--nofw` out).
    #[test]
    fn se_argv_minimap2_positional_mmi() {
        let argv = build_se_argv(
            Aligner::Minimap2,
            "-a --MD --secondary=no -t 2 -x map-ont -K 250K",
            Orientation::Norc, // ignored for minimap2
            Path::new("/idx/BS_CT"),
            Path::new("/tmp/r_C_to_T.fastq"),
        );
        let got = argv_strings(&argv);
        assert_eq!(
            got,
            vec![
                "-a",
                "--MD",
                "--secondary=no",
                "-t",
                "2",
                "-x",
                "map-ont",
                "-K",
                "250K",
                "/idx/BS_CT.mmi", // positional index, literal `.mmi` append
                "/tmp/r_C_to_T.fastq",
            ]
        );
        // No strand flag, no `-U`, and the index is NOT passed as `-x <basename>`.
        assert!(!got.contains(&"--norc".to_string()) && !got.contains(&"--nofw".to_string()));
        assert!(!got.contains(&"-U".to_string()));
        assert!(!got.contains(&"/idx/BS_CT".to_string())); // only the `.mmi` form
    }

    /// Phase 3 (T1): rammap is minimap-like — the SAME positional `<index>.mmi
    /// <input>` shape as minimap2 (NO `-U`, NO strand flag, NO `-x <basename>`).
    #[test]
    fn se_argv_rammap_positional_mmi() {
        let argv = build_se_argv(
            Aligner::Rammap,
            "-a --MD --secondary=no -t 2 -x map-ont -K 250K",
            Orientation::Norc, // ignored for rammap (minimap-like)
            Path::new("/g/BS_CT"),
            Path::new("/r/reads.fq"),
        );
        let s = argv_strings(&argv);
        assert!(s.contains(&"/g/BS_CT.mmi".to_string()));
        assert!(s.contains(&"/r/reads.fq".to_string()));
        assert!(!s.contains(&"-U".to_string()));
    }

    // ---- build_pe_argv (v2.x Phase 5: HISAT2 PE shares the Bowtie 2 PE shape) ------
    #[test]
    fn pe_argv_bowtie2_shape_frozen() {
        let argv = build_pe_argv(
            Aligner::Bowtie2,
            "-q --score-min L,0,-0.2 --no-mixed",
            Orientation::Norc,
            Path::new("/idx/BS_CT"),
            Path::new("/tmp/r1_C_to_T.fastq"),
            Path::new("/tmp/r2_G_to_A.fastq"),
        );
        assert_eq!(
            argv_strings(&argv),
            vec![
                "-q",
                "--score-min",
                "L,0,-0.2",
                "--no-mixed",
                "--norc",
                "-x",
                "/idx/BS_CT",
                "-1",
                "/tmp/r1_C_to_T.fastq",
                "-2",
                "/tmp/r2_G_to_A.fastq",
            ]
        );
    }

    /// Phase 5: HISAT2 PE uses the SAME argv shape as Bowtie 2 PE (only the options
    /// string differs) — the basis for reusing the PE machinery with `--hisat2`.
    #[test]
    fn pe_argv_hisat2_same_shape_as_bowtie2() {
        let argv = build_pe_argv(
            Aligner::Hisat2,
            "-q --no-softclip --omit-sec-seq -k 2",
            Orientation::Nofw,
            Path::new("/idx/BS_combined"),
            Path::new("/tmp/r1_G_to_A.fastq"),
            Path::new("/tmp/r2_C_to_T.fastq"),
        );
        assert_eq!(
            argv_strings(&argv),
            vec![
                "-q",
                "--no-softclip",
                "--omit-sec-seq",
                "-k",
                "2",
                "--nofw",
                "-x",
                "/idx/BS_combined",
                "-1",
                "/tmp/r1_G_to_A.fastq",
                "-2",
                "/tmp/r2_C_to_T.fastq",
            ]
        );
    }

    /// `--combined_index` PE: `Orientation::Both` emits NO strand flag (no empty arg
    /// before `-x`); `-1`/`-2` follow `-x <index>`.
    #[test]
    fn pe_argv_combined_both_no_strand_flag() {
        let argv = build_pe_argv(
            Aligner::Hisat2,
            "-q -k 2",
            Orientation::Both,
            Path::new("/idx/BS_combined"),
            Path::new("/tmp/r1_G_to_A.fastq"),
            Path::new("/tmp/r2_C_to_T.fastq"),
        );
        let got = argv_strings(&argv);
        assert_eq!(
            got,
            vec![
                "-q",
                "-k",
                "2",
                "-x",
                "/idx/BS_combined",
                "-1",
                "/tmp/r1_G_to_A.fastq",
                "-2",
                "/tmp/r2_C_to_T.fastq",
            ]
        );
        assert!(!got.contains(&"--norc".to_string()) && !got.contains(&"--nofw".to_string()));
        assert!(!got.iter().any(|a| a.is_empty())); // never an empty arg before -x
    }

    #[test]
    #[should_panic(expected = "minimap2 paired-end")]
    fn pe_argv_minimap2_is_unreachable() {
        // minimap2 PE is rejected at resolve → build_pe_argv must never see it.
        let _ = build_pe_argv(
            Aligner::Minimap2,
            "-a --MD",
            Orientation::Both,
            Path::new("/idx/BS_combined"),
            Path::new("/tmp/r1.fastq"),
            Path::new("/tmp/r2.fastq"),
        );
    }

    /// Phase 3 (T4): rammap PE is rejected at resolve too (minimap-like, SE-only) →
    /// build_pe_argv must never see it (the shared minimap2/rammap unreachable arm).
    #[test]
    #[should_panic(expected = "rammap")]
    fn pe_argv_rammap_is_unreachable() {
        let _ = build_pe_argv(
            Aligner::Rammap,
            "-a --MD",
            Orientation::Both,
            Path::new("/idx/BS_combined"),
            Path::new("/tmp/r1.fastq"),
            Path::new("/tmp/r2.fastq"),
        );
    }

    /// `--combined_index` (v2): `Orientation::Both` emits NO strand flag (neither
    /// `--norc` nor `--nofw`) and — critically — does NOT push an empty argument
    /// before `-x`. The combined argv carries `-k 2` (added by the combined drive)
    /// so the runner-up is visible to the classifier.
    #[test]
    fn se_argv_bowtie2_orientation_both_emits_no_strand_flag() {
        let argv = build_se_argv(
            Aligner::Bowtie2,
            "-q --score-min L,0,-0.2 --ignore-quals -k 2",
            Orientation::Both,
            Path::new("/idx/Combined/BS_combined"),
            Path::new("/tmp/r_C_to_T.fastq"),
        );
        let got = argv_strings(&argv);
        assert_eq!(
            got,
            vec![
                "-q",
                "--score-min",
                "L,0,-0.2",
                "--ignore-quals",
                "-k",
                "2",
                "-x",
                "/idx/Combined/BS_combined",
                "-U",
                "/tmp/r_C_to_T.fastq",
            ]
        );
        assert!(!got.contains(&"--norc".to_string()) && !got.contains(&"--nofw".to_string()));
        assert!(!got.iter().any(|a| a.is_empty())); // no empty arg pushed for `Both`
        assert_eq!(Orientation::Both.flag(), None);
    }

    /// minimap2  orientation is irrelevant — `--norc` and `--nofw` produce the same
    /// argv (the strand flag is never emitted).
    #[test]
    fn se_argv_minimap2_orientation_independent() {
        let mk = |o| {
            argv_strings(&build_se_argv(
                Aligner::Minimap2,
                "-a -x map-ont",
                o,
                Path::new("/idx/BS_GA"),
                Path::new("/tmp/r.fastq"),
            ))
        };
        assert_eq!(mk(Orientation::Norc), mk(Orientation::Nofw));
    }

    // ---- stream over a fake bowtie2 (hermetic; no real Bowtie 2 needed) -----

    #[cfg(unix)]
    fn fake_bowtie2(dir: &Path, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("bowtie2");
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        let mut perm = std::fs::metadata(&p).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&p, perm).unwrap();
        p
    }

    /// Retry a spawn on the transient `Text file busy` (ETXTBSY) write-then-exec race.
    /// In the multi-threaded test runner a concurrent `fork`+`exec` on another thread
    /// momentarily inherits this thread's just-written fake-`bowtie2` fd (the CLOEXEC
    /// flag only closes it at the *other* child's exec), so exec'ing the freshly-written
    /// file here can fail transiently — a short pause + retry clears it. Test-only:
    /// production `bowtie2` is a real installed binary, never freshly written, so the
    /// production `spawn` (which has no retry) never hits this. (Latent since Phase 3;
    /// observed flaking the Phase-8 and Phase-9b CI runs.)
    #[cfg(unix)]
    fn spawn_retry_etxtbsy<T, E: std::fmt::Display>(
        mut attempt: impl FnMut() -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let mut last = attempt();
        let mut tries = 0u64;
        while tries < 9 {
            match &last {
                Err(e) if e.to_string().contains("Text file busy") => {
                    tries += 1;
                    std::thread::sleep(std::time::Duration::from_millis(20 * tries));
                    last = attempt();
                }
                _ => break,
            }
        }
        last
    }

    #[cfg(unix)]
    fn spawn_fake(body: &str) -> (tempfile::TempDir, AlignerStream) {
        let dir = tempfile::TempDir::new().unwrap();
        let bt2 = fake_bowtie2(dir.path(), body);
        let s = spawn_retry_etxtbsy(|| {
            AlignerStream::spawn(
                Aligner::Bowtie2,
                &bt2,
                "-q --score-min L,0,-0.2 --ignore-quals",
                Orientation::Norc,
                Path::new("idx"),
                Path::new("reads.fq"),
            )
        })
        .unwrap();
        (dir, s)
    }

    #[cfg(unix)]
    #[test]
    fn stream_skips_header_then_walks_records_to_eof() {
        // 2 header lines + 3 records.
        let body = "printf '@HD\\tVN:1.0\\n@SQ\\tSN:c_CT_converted\\tLN:9\\n\
            a\\t0\\tc_CT_converted\\t1\\t40\\t4M\\t*\\t0\\t0\\tACGT\\tIIII\\tAS:i:0\\tMD:Z:4\\n\
            b\\t0\\tc_CT_converted\\t2\\t40\\t4M\\t*\\t0\\t0\\tACGT\\tIIII\\tAS:i:0\\tMD:Z:4\\n\
            c\\t4\\t*\\t0\\t0\\t*\\t*\\t0\\t0\\tACGT\\tIIII\\n'";
        let (_d, mut s) = spawn_fake(body);
        assert_eq!(s.current().unwrap().qname, "a"); // header skipped
        s.advance().unwrap();
        assert_eq!(s.current().unwrap().qname, "b");
        s.advance().unwrap();
        assert_eq!(s.current().unwrap().qname, "c");
        assert!(s.current().unwrap().is_unmapped());
        s.advance().unwrap();
        assert!(s.current().is_none()); // EOF
        s.finish().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn all_header_stream_has_no_records() {
        let (_d, s) = spawn_fake("printf '@HD\\tVN:1.0\\n'");
        assert!(s.current().is_none());
        s.finish().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn finish_errors_on_nonzero_exit() {
        let body = "printf 'a\\t0\\tc_CT_converted\\t1\\t40\\t4M\\t*\\t0\\t0\\tA\\tI\\tAS:i:0\\tMD:Z:1\\n'; exit 1";
        let (_d, s) = spawn_fake(body);
        assert!(s.finish().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn finish_error_names_the_resolved_aligner_not_bowtie2() {
        // Never-silent (PLAN 06102026 phase 7 / review A-I1): the SE stream's finish()
        // error must name the RESOLVED aligner — a HISAT2 instance that exits non-zero
        // reports "HISAT2", not the old hardcoded "Bowtie 2". (The SE sibling lagged the
        // PE PairedAlignerStream until HISAT2 SE was driven through this path.)
        let dir = tempfile::TempDir::new().unwrap();
        let h2 = fake_bowtie2(
            dir.path(),
            "printf 'a\\t0\\tc_CT_converted\\t1\\t40\\t4M\\t*\\t0\\t0\\tA\\tI\\tAS:i:0\\tMD:Z:1\\n'; exit 1",
        );
        let s = spawn_retry_etxtbsy(|| {
            AlignerStream::spawn(
                Aligner::Hisat2,
                &h2,
                "-q",
                Orientation::Norc,
                Path::new("idx"),
                Path::new("reads.fq"),
            )
        })
        .unwrap();
        let msg = format!("{}", s.finish().unwrap_err());
        assert!(msg.contains("HISAT2"), "got: {msg}");
        assert!(!msg.contains("Bowtie 2"), "got: {msg}");
    }

    #[cfg(unix)]
    #[test]
    fn spawn_fails_on_bad_path() {
        let r = AlignerStream::spawn(
            Aligner::Bowtie2,
            Path::new("/no/such/bowtie2"),
            "-q",
            Orientation::Norc,
            Path::new("idx"),
            Path::new("reads.fq"),
        );
        assert!(r.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn early_stop_does_not_deadlock_or_zombie() {
        // Emit ~5000 records (> the 64K stdout pipe buffer) so a non-draining
        // finish() would block the child on write(); our drain-then-wait must
        // complete. Read only the first record, then finish().
        let body = "i=0; while [ $i -lt 5000 ]; do \
            printf 'r%d\\t0\\tc_CT_converted\\t1\\t40\\t4M\\t*\\t0\\t0\\tACGT\\tIIII\\tAS:i:0\\tMD:Z:4\\n' $i; \
            i=$((i+1)); done";
        let (_d, s) = spawn_fake(body);
        assert_eq!(s.current().unwrap().qname, "r0");
        s.finish().unwrap(); // drains remaining stdout, then reaps — no hang
    }

    // ---- paired-end pair construction + stream (Phase 7) --------------------

    fn pe_line(qname: &str, flag: u16, pos: u32) -> String {
        format!(
            "{qname}\t{flag}\tchr1_CT_converted\t{pos}\t40\t10M\t=\t{pos}\t0\tACGTACGTAC\tIIIIIIIIII\tAS:i:0\tMD:Z:10"
        )
    }

    #[test]
    fn sampair_identifies_read1_by_slash1() {
        let p = SamPair::from_lines(&pe_line("readX/1", 99, 100), &pe_line("readX/2", 147, 140))
            .unwrap();
        assert_eq!(p.seq_id, "readX");
        assert_eq!(p.read1.qname, "readX/1");
        assert_eq!(p.read2.qname, "readX/2");
    }

    #[test]
    fn sampair_swaps_when_read1_emitted_second() {
        // Bowtie 2 emits the leftmost mate first; here read 2 is leftmost.
        let p = SamPair::from_lines(&pe_line("readX/2", 147, 100), &pe_line("readX/1", 99, 140))
            .unwrap();
        assert_eq!(p.seq_id, "readX");
        assert_eq!(p.read1.qname, "readX/1"); // canonicalised: read1 is the /1 mate
        assert_eq!(p.read2.qname, "readX/2");
    }

    #[test]
    fn sampair_dies_when_neither_is_read1() {
        assert!(
            SamPair::from_lines(&pe_line("readX/2", 147, 100), &pe_line("readY/2", 147, 140))
                .is_err()
        );
    }

    #[test]
    fn sampair_unmapped_marker_77_141() {
        let p = SamPair::from_lines(&pe_line("r/1", 77, 0), &pe_line("r/2", 141, 0)).unwrap();
        assert!(p.is_unmapped_pair());
        let mapped =
            SamPair::from_lines(&pe_line("r/1", 99, 100), &pe_line("r/2", 147, 140)).unwrap();
        assert!(!mapped.is_unmapped_pair());
    }

    #[cfg(unix)]
    fn spawn_fake_pe(body: &str) -> (tempfile::TempDir, PairedAlignerStream) {
        let dir = tempfile::TempDir::new().unwrap();
        let bt2 = fake_bowtie2(dir.path(), body);
        let s = spawn_retry_etxtbsy(|| {
            PairedAlignerStream::spawn(
                Aligner::Bowtie2,
                &bt2,
                "-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --dovetail --maxins 500",
                Orientation::Norc,
                Path::new("idx"),
                Path::new("r1.fq"),
                Path::new("r2.fq"),
            )
        })
        .unwrap();
        (dir, s)
    }

    #[cfg(unix)]
    #[test]
    fn pe_stream_skips_header_then_walks_pairs_to_eof() {
        // 1 header line + 2 pairs (4 records). Pair b is emitted read2-first.
        let body = "printf '@HD\\tVN:1.0\\n\
            a/1\\t99\\tchr1_CT_converted\\t10\\t40\\t4M\\t=\\t20\\t14\\tACGT\\tIIII\\tAS:i:0\\tMD:Z:4\\n\
            a/2\\t147\\tchr1_CT_converted\\t20\\t40\\t4M\\t=\\t10\\t-14\\tACGT\\tIIII\\tAS:i:0\\tMD:Z:4\\n\
            b/2\\t147\\tchr1_CT_converted\\t30\\t40\\t4M\\t=\\t40\\t14\\tACGT\\tIIII\\tAS:i:0\\tMD:Z:4\\n\
            b/1\\t99\\tchr1_CT_converted\\t40\\t40\\t4M\\t=\\t30\\t-14\\tACGT\\tIIII\\tAS:i:0\\tMD:Z:4\\n'";
        let (_d, mut s) = spawn_fake_pe(body);
        assert_eq!(s.current_pair().unwrap().seq_id, "a");
        assert_eq!(s.current_pair().unwrap().read1.pos, 10);
        s.advance_pair().unwrap();
        let b = s.current_pair().unwrap();
        assert_eq!(b.seq_id, "b");
        assert_eq!(b.read1.qname, "b/1"); // canonicalised despite read2-first emission
        assert_eq!(b.read1.pos, 40);
        s.advance_pair().unwrap();
        assert!(s.current_pair().is_none()); // EOF
        s.finish().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn pe_stream_all_header_has_no_pairs() {
        let (_d, s) = spawn_fake_pe("printf '@HD\\tVN:1.0\\n'");
        assert!(s.current_pair().is_none());
        s.finish().unwrap();
    }

    // ---- PairedFileSamStream (Phase 6 — the spilled PE pass replay) ---------

    /// Write `content` to `path` as a BGZF stream — the spill is BGZF since issue #1019,
    /// so reader fixtures must be BGZF too. Mirrors the production writer
    /// (`MultithreadedWriter` + `finish()`; 1 worker is plenty for tests).
    fn write_bgzf(path: &std::path::Path, content: &[u8]) {
        use std::io::Write as _;
        let workers = std::num::NonZeroUsize::new(1).unwrap();
        let mut w = noodles_bgzf::io::MultithreadedWriter::with_worker_count(
            workers,
            std::fs::File::create(path).unwrap(),
        );
        w.write_all(content).unwrap();
        w.finish().unwrap();
    }

    /// Write SAM `lines` (each newline-terminated) to a fresh temp file as a BGZF spill
    /// and return it (the TempDir keeps the file alive for the test's lifetime).
    fn write_spill(lines: &[&str]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("ct_pass.sam.gz");
        let mut body = String::new();
        for l in lines {
            body.push_str(l);
            body.push('\n');
        }
        write_bgzf(&path, body.as_bytes());
        (dir, path)
    }

    #[test]
    fn paired_file_sam_stream_round_trips_incl_miss() {
        // Two pairs, two lines each: a normal pair (read2-first, to exercise the
        // canonicalising re-pair) + a (77,141) miss pair. Replays identically.
        let (_d, path) = write_spill(&[
            &pe_line("a/2", 147, 20),
            &pe_line("a/1", 99, 10),
            &pe_line("b/1", 77, 0),
            &pe_line("b/2", 141, 0),
        ]);
        let mut s = PairedFileSamStream::open(&path).unwrap();
        let a = s.current_pair().unwrap();
        assert_eq!(a.seq_id, "a");
        assert_eq!(a.read1.qname, "a/1"); // canonicalised despite read2-first spill order
        assert_eq!(a.read1.pos, 10);
        assert!(!a.is_unmapped_pair());
        s.advance_pair().unwrap();
        let b = s.current_pair().unwrap();
        assert_eq!(b.seq_id, "b");
        assert!(b.is_unmapped_pair()); // the (77,141) miss round-trips
        s.advance_pair().unwrap();
        assert!(s.current_pair().is_none()); // EOF
    }

    #[test]
    fn paired_file_sam_stream_skips_header() {
        let (_d, path) = write_spill(&[
            "@HD\tVN:1.0",
            &pe_line("a/1", 99, 10),
            &pe_line("a/2", 147, 20),
        ]);
        let s = PairedFileSamStream::open(&path).unwrap();
        assert_eq!(s.current_pair().unwrap().seq_id, "a");
    }

    #[test]
    fn paired_file_sam_stream_lone_trailing_line_is_none() {
        // An odd line count: a complete pair followed by a single dangling record →
        // the dangling line has no second mate, so the second `advance` yields None
        // (mirrors PairedAlignerStream::advance_pair — NOT an error).
        let (_d, path) = write_spill(&[
            &pe_line("a/1", 99, 10),
            &pe_line("a/2", 147, 20),
            &pe_line("b/1", 99, 30),
        ]);
        let mut s = PairedFileSamStream::open(&path).unwrap();
        assert_eq!(s.current_pair().unwrap().seq_id, "a");
        s.advance_pair().unwrap();
        assert!(s.current_pair().is_none()); // lone trailing b/1 → no pair
    }

    #[test]
    fn paired_file_sam_stream_empty_and_header_only_none() {
        let (_d, empty) = write_spill(&[]);
        assert!(
            PairedFileSamStream::open(&empty)
                .unwrap()
                .current_pair()
                .is_none()
        );
        let (_d2, header) = write_spill(&["@HD\tVN:1.0", "@SQ\tSN:chr1_CT_converted\tLN:16"]);
        assert!(
            PairedFileSamStream::open(&header)
                .unwrap()
                .current_pair()
                .is_none()
        );
    }
}
