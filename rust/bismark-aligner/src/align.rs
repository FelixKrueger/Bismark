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

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};

use crate::error::{AlignerError, Result};

/// Per-instance strand-orientation flag (the strand-instance table, Perl 7124).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// `--norc` — CTreadCTgenome / GAreadGAgenome.
    Norc,
    /// `--nofw` — CTreadGAgenome / GAreadCTgenome.
    Nofw,
}

impl Orientation {
    /// The Bowtie 2 flag.
    pub fn flag(self) -> &'static str {
        match self {
            Orientation::Norc => "--norc",
            Orientation::Nofw => "--nofw",
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

impl SamRecord {
    /// Parse one SAM line (`split('\t')`). The line may carry a trailing
    /// terminator, which is stripped. Errors on `< 11` fields or unparseable
    /// FLAG/POS/MAPQ; unparseable tag values are left `None` (lenient — Phase 4
    /// enforces `AS`/`MD` presence, Perl `die` 2838).
    pub fn parse(line: &str) -> Result<SamRecord> {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        let f: Vec<&str> = trimmed.split('\t').collect();
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

/// A live single Bowtie 2 instance, presenting a peek/advance SAM stream.
pub struct AlignerStream {
    child: Child,
    reader: BufReader<ChildStdout>,
    current: Option<SamRecord>,
    line_buf: String,
    finished: bool,
}

impl AlignerStream {
    /// Spawn one Bowtie 2 instance and read up to the first alignment record.
    ///
    /// Args mirror Perl 6872–6882: `<aligner_options> <orient> -x <index> -U
    /// <input>`. stdout is piped; stderr is **inherited** (Bowtie 2's summary →
    /// terminal, as in Perl — so only stdout is piped and it is always drained).
    pub fn spawn(
        bowtie2: &Path,
        options: &str,
        orient: Orientation,
        index: &Path,
        input: &Path,
    ) -> Result<Self> {
        let mut cmd = Command::new(bowtie2);
        for opt in options.split_whitespace() {
            cmd.arg(opt);
        }
        cmd.arg(orient.flag())
            .arg("-x")
            .arg(index)
            .arg("-U")
            .arg(input)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd.spawn().map_err(|e| {
            AlignerError::Validation(format!(
                "failed to spawn Bowtie 2 ({}): {e}",
                bowtie2.display()
            ))
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AlignerError::Validation("Bowtie 2 stdout was not captured".into()))?;
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
                "Bowtie 2 exited unsuccessfully ({status})"
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

    #[cfg(unix)]
    fn spawn_fake(body: &str) -> (tempfile::TempDir, AlignerStream) {
        let dir = tempfile::TempDir::new().unwrap();
        let bt2 = fake_bowtie2(dir.path(), body);
        let s = AlignerStream::spawn(
            &bt2,
            "-q --score-min L,0,-0.2 --ignore-quals",
            Orientation::Norc,
            Path::new("idx"),
            Path::new("reads.fq"),
        )
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
    fn spawn_fails_on_bad_path() {
        let r = AlignerStream::spawn(
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
}
