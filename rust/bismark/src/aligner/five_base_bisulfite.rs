//! `[#1095]` Re-encode a Bismark BAM's `SEQ` into **bisulfite convention** from the `XM`
//! tag, so Illumina 5-Base data works with `wgbs_tools bam2pat` → `UXM_deconv`.
//!
//! `patter` (the C++ engine behind `bam2pat`) never reads `XM` for its calls — it takes the
//! read character in `SEQ` at CpG positions and compares it against two constants per
//! strand (`patter.h`: `OT{'C','T'}`, `OB{'G','A'}`). 5-Base `SEQ` keeps the original read
//! and 5-Base chemistry is bisulfite's inverse, so `patter`'s calls come out inverted. This
//! module rewrites `SEQ` so the calls Bismark already made are what `patter` reads.
//!
//! Three properties make this a per-record pure function needing **no genome**:
//!
//! 1. `len(XM) == len(SEQ)` and `XM[i]` ↔ `SEQ[i]` in BAM space — `XM` is reversed in
//!    lockstep with `SEQ` for `-`-strand records (`output.rs` SE `:443-450`/`:463-467`,
//!    PE `:671-675`/`:694-698`).
//! 2. `XG` alone fixes the reference base at every call: `CT` ⇒ `C`, `GA` ⇒ `G`. This is
//!    **emergent** — `methylation_call` branches on `XR`, not `XG` — from the index →
//!    (strand, XR, XG) map (`methylation.rs:131-141`), which genomic base each branch
//!    tests, and the joint `SEQ`/`ref_seq` revcomp for `-` strand.
//! 3. `I`/`S` pad the genomic window with `b'X'` without advancing the reference cursor
//!    (`methylation.rs:174-181`, `:349-354`), so `XM` is `'.'` at every clipped or inserted
//!    position and a positional zip provably cannot touch one.
//!
//! Do **not** reach for [`crate::io::record::BismarkRecord::iter_aligned`] here: it returns
//! 5'-oriented positions and skips `I`/`S`, so writing into `SEQ` at `read_pos_5p` silently
//! reverses every OB read's edits. Property 3 removes any need for reference positions.

use thiserror::Error;

use crate::aligner::methylation::parse_cigar;
use crate::aligner::output::{hemming_dist, make_mismatch_string};

/// Which strand's cytosines a record's calls sit on, from `XG:Z:`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XgStrand {
    /// `XG:Z:CT` — calls at genomic `C`; `SEQ` carries `C` (methylated) / `T` (not).
    Ct,
    /// `XG:Z:GA` — calls at genomic `G`; `SEQ` carries `G` (methylated) / `A` (not).
    Ga,
}

impl XgStrand {
    /// Parse the `XG:Z:` tag value.
    pub fn from_tag(xg: &[u8], qname: &str) -> Result<Self, FiveBaseBisulfiteError> {
        match xg {
            b"CT" => Ok(Self::Ct),
            b"GA" => Ok(Self::Ga),
            other => Err(FiveBaseBisulfiteError::BadXg {
                qname: qname.to_string(),
                tag: String::from_utf8_lossy(other).into_owned(),
            }),
        }
    }

    /// The `SEQ` base a **methylated** call carries. Equal to [`Self::ref_base`].
    pub fn meth(self) -> u8 {
        match self {
            Self::Ct => b'C',
            Self::Ga => b'G',
        }
    }

    /// The `SEQ` base an **unmethylated** call carries.
    pub fn unmeth(self) -> u8 {
        match self {
            Self::Ct => b'T',
            Self::Ga => b'A',
        }
    }

    /// The reference base at every XM-letter position. Identical to [`Self::meth`] — a
    /// methylated call is precisely one whose read base still matches the reference.
    pub fn ref_base(self) -> u8 {
        self.meth()
    }
}

/// Everything that can go wrong for one record. Every variant names the QNAME, because a
/// converter that fails anonymously in the middle of a BAM is not actionable.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FiveBaseBisulfiteError {
    #[error("record {qname}: XM length {xm} does not match SEQ length {seq}")]
    LengthMismatch {
        qname: String,
        xm: usize,
        seq: usize,
    },

    #[error(
        "record {qname}: unrecognised XM byte '{byte}' at offset {offset} (expected one of ZzXxHhUu.)"
    )]
    InvalidXmByte {
        qname: String,
        byte: char,
        offset: usize,
    },

    #[error(
        "record {qname}: XM letter '{byte}' at offset {offset} falls in a CIGAR gap \
         (insertion or soft clip), which should be structurally impossible"
    )]
    CallInGap {
        qname: String,
        byte: char,
        offset: usize,
    },

    #[error(
        "record {qname}: SEQ base '{base}' at XM-letter offset {offset} is neither '{meth}' \
         (methylated) nor '{unmeth}' (unmethylated) for XG:Z:{xg}"
    )]
    SeqNotInPair {
        qname: String,
        base: char,
        offset: usize,
        meth: char,
        unmeth: char,
        xg: String,
    },

    #[error("record {qname}: unsupported CIGAR operation '{op}' (only M, I, D, S, N are handled)")]
    UnsupportedCigarOp { qname: String, op: char },

    #[error("record {qname}: malformed MD tag: {reason}")]
    MalformedMd { qname: String, reason: String },

    #[error(
        "record {qname}: the reconstructed reference failed its round-trip proof ({what}). \
         The output would be untrustworthy, so nothing was written."
    )]
    RoundTripFailed { qname: String, what: String },

    #[error("record {qname}: unsupported XG:Z:{tag} (expected CT or GA)")]
    BadXg { qname: String, tag: String },
}

/// The result of re-encoding one record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reencoded {
    /// The rewritten `SEQ`.
    pub seq: Vec<u8>,
    /// `NM`, recomputed on the new `SEQ`.
    pub nm: i64,
    /// `MD` (bare value, no `MD:Z:` prefix), recomputed on the new `SEQ`.
    pub md: String,
    /// XM-letter positions seen.
    pub letters: u32,
    /// Letter positions whose base actually changed. `flipped/letters` is exactly `1.0` for
    /// 5-Base input and `0.0` for bisulfite input — a hard discriminator on the file.
    pub flipped: u32,
    /// No-call positions masked to `N` (see [`reencode`]). **Disjoint from `flipped`.**
    pub masked: u32,
}

/// The reference, reconstructed in BAM space from `(SEQ, CIGAR, MD)`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Reference {
    /// One byte per **read** position: the genomic base at `M`, `b'X'` at `I`/`S`.
    ref_seq: Vec<u8>,
    /// Reference bases over `M` + `D` runs with `b'X'` at `I`/`S`, in genome-forward order.
    /// Only consulted by [`make_mismatch_string`] when the CIGAR contains `D`.
    md_seq: Vec<u8>,
    /// Total deleted reference bases — the `indels` term of Bismark's `NM`.
    deleted: usize,
}

/// One `MD` token.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MdTok {
    /// `n` reference bases that match the read.
    Match(usize),
    /// One reference base that mismatches the read.
    Mismatch(u8),
    /// `^` followed by the deleted reference bases.
    Deletion(Vec<u8>),
}

fn parse_md(md: &str, qname: &str) -> Result<Vec<MdTok>, FiveBaseBisulfiteError> {
    let bad = |reason: String| FiveBaseBisulfiteError::MalformedMd {
        qname: qname.to_string(),
        reason,
    };
    let b = md.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let n: usize = md[start..i]
                .parse()
                .map_err(|e| bad(format!("bad match run at offset {start}: {e}")))?;
            // A zero run is legal and common (it separates adjacent mismatches).
            if n > 0 {
                out.push(MdTok::Match(n));
            }
        } else if b[i] == b'^' {
            i += 1;
            let start = i;
            while i < b.len() && b[i].is_ascii_alphabetic() {
                i += 1;
            }
            if i == start {
                return Err(bad(format!("'^' with no bases at offset {}", start - 1)));
            }
            out.push(MdTok::Deletion(b[start..i].to_vec()));
        } else if b[i].is_ascii_alphabetic() {
            out.push(MdTok::Mismatch(b[i]));
            i += 1;
        } else {
            return Err(bad(format!(
                "unexpected byte '{}' at offset {i}",
                b[i] as char
            )));
        }
    }
    Ok(out)
}

/// Rebuild the reference in BAM space from `(seq, cigar, md)`.
///
/// Sufficient for every reference base anyone needs: all `M` positions (a match yields the
/// read base, a mismatch the `MD` letter) and all `D` positions (`^XYZ`). It is *not*
/// sufficient for `I`/`S`, and nothing needs it to be — `MD` is blind to them by design and
/// both `hemming_dist` and `make_mismatch_string` treat the `b'X'` padding accordingly.
fn reconstruct_ref(
    seq: &[u8],
    cigar: &str,
    md: &str,
    qname: &str,
) -> Result<Reference, FiveBaseBisulfiteError> {
    let runs = parse_cigar(cigar).map_err(|e| FiveBaseBisulfiteError::MalformedMd {
        qname: qname.to_string(),
        reason: format!("unparseable CIGAR {cigar:?}: {e}"),
    })?;
    let toks = parse_md(md, qname)?;

    let mut ref_seq = Vec::with_capacity(seq.len());
    let mut md_seq = Vec::new();
    let mut deleted = 0usize;

    let mut read_pos = 0usize; // index into `seq`
    let mut tok = 0usize; // index into `toks`
    let mut run_left = 0usize; // remaining bases in the current Match run

    // Pull one reference base for an `M` position.
    let next_m_ref = |read_pos: usize,
                      tok: &mut usize,
                      run_left: &mut usize|
     -> Result<u8, FiveBaseBisulfiteError> {
        loop {
            if *run_left > 0 {
                *run_left -= 1;
                // Match: the reference base IS the read base.
                return Ok(seq[read_pos]);
            }
            match toks.get(*tok) {
                Some(MdTok::Match(n)) => {
                    *run_left = *n;
                    *tok += 1;
                }
                Some(MdTok::Mismatch(base)) => {
                    *tok += 1;
                    return Ok(*base);
                }
                Some(MdTok::Deletion(_)) | None => {
                    return Err(FiveBaseBisulfiteError::MalformedMd {
                        qname: qname.to_string(),
                        reason: format!(
                            "MD ran out of aligned bases at read offset {read_pos} \
                             (MD {md:?} vs CIGAR {cigar:?})"
                        ),
                    });
                }
            }
        }
    };

    for (len, op) in &runs {
        let len = *len as usize;
        match op {
            b'M' => {
                for _ in 0..len {
                    if read_pos >= seq.len() {
                        return Err(FiveBaseBisulfiteError::MalformedMd {
                            qname: qname.to_string(),
                            reason: format!("CIGAR {cigar:?} runs past SEQ length {}", seq.len()),
                        });
                    }
                    let r = next_m_ref(read_pos, &mut tok, &mut run_left)?;
                    ref_seq.push(r);
                    md_seq.push(r);
                    read_pos += 1;
                }
            }
            b'I' | b'S' => {
                // `X` padding: consumes read length, no reference. MD skips these.
                ref_seq.extend(std::iter::repeat_n(b'X', len));
                md_seq.extend(std::iter::repeat_n(b'X', len));
                read_pos += len;
            }
            b'D' => {
                // Flush any pending match run first: MD emits `0` before `^` when a
                // deletion abuts a mismatch, so `run_left` must be exhausted here.
                if run_left > 0 {
                    return Err(FiveBaseBisulfiteError::MalformedMd {
                        qname: qname.to_string(),
                        reason: format!(
                            "MD match run overruns a {len}D at read offset {read_pos} \
                             (MD {md:?} vs CIGAR {cigar:?})"
                        ),
                    });
                }
                match toks.get(tok) {
                    Some(MdTok::Deletion(bases)) if bases.len() == len => {
                        md_seq.extend_from_slice(bases);
                        deleted += len;
                        tok += 1;
                    }
                    other => {
                        return Err(FiveBaseBisulfiteError::MalformedMd {
                            qname: qname.to_string(),
                            reason: format!(
                                "expected a {len}-base MD deletion at read offset {read_pos}, \
                                 found {other:?}"
                            ),
                        });
                    }
                }
            }
            b'N' => { /* reference skip: no read bases, no MD, no md_seq (methylation.rs:189-191) */
            }
            other => {
                return Err(FiveBaseBisulfiteError::UnsupportedCigarOp {
                    qname: qname.to_string(),
                    op: *other as char,
                });
            }
        }
    }

    if read_pos != seq.len() {
        return Err(FiveBaseBisulfiteError::MalformedMd {
            qname: qname.to_string(),
            reason: format!(
                "CIGAR {cigar:?} covers {read_pos} read bases but SEQ is {}",
                seq.len()
            ),
        });
    }

    Ok(Reference {
        ref_seq,
        md_seq,
        deleted,
    })
}

/// Bismark's `NM`: mismatches at `M` **plus inserted plus soft-clipped** bases (the `b'X'`
/// padding is counted by `hemming_dist` — see its doc comment, "intentionally counted"),
/// plus deleted bases. Soft-clip inflation is non-standard but it is what Bismark writes,
/// and this converter must not silently "fix" it.
fn bismark_nm(seq: &[u8], r: &Reference) -> i64 {
    hemming_dist(seq, &r.ref_seq) as i64 + r.deleted as i64
}

/// Re-encode `seq` so that each called cytosine carries the base bisulfite chemistry would
/// have produced. `XM` is **not** modified — it is already correct and stays the source of
/// truth, which is what keeps the output readable by Bismark's own extractor.
///
/// No-call positions that sit at a reference cytosine and still carry a scoreable base are
/// masked to `b'N'`. Those exist only when the alignment run used `--five_base_baseq`, which
/// masks the *call* sequence while leaving the raw base in `SEQ` — `patter` would otherwise
/// score them **inverted**. The rule needs no threshold and no `QUAL`: at a genomic `C` the
/// CT branch emits a letter for a read `C` **or** `T` with no further guard
/// (`methylation.rs:587-596`), so `XM == '.'` at a reference cytosine whose base is still
/// scoreable *proves* the call sequence differed from `SEQ`. On input with no masking the
/// set is provably empty, which is why the idempotence gate covers this path.
///
/// # Errors
///
/// See [`FiveBaseBisulfiteError`]. Every failure is loud and names the record: a converter
/// that guesses produces a file whose wrongness is invisible.
pub fn reencode(
    seq: &[u8],
    xm: &[u8],
    xg: XgStrand,
    cigar: &str,
    md_old: &str,
    nm_old: i64,
    qname: &str,
) -> Result<Reencoded, FiveBaseBisulfiteError> {
    if xm.len() != seq.len() {
        return Err(FiveBaseBisulfiteError::LengthMismatch {
            qname: qname.to_string(),
            xm: xm.len(),
            seq: seq.len(),
        });
    }

    let reference = reconstruct_ref(seq, cigar, md_old, qname)?;

    // ---- round-trip proof: the reconstruction must reproduce the record's own tags ----
    // Strictly stronger than checking the reference base only at letter positions, and it
    // is the sole guard on the deletion path, which the idempotence gate cannot reach.
    let nm_check = bismark_nm(seq, &reference);
    if nm_check != nm_old {
        return Err(FiveBaseBisulfiteError::RoundTripFailed {
            qname: qname.to_string(),
            what: format!("recomputed NM {nm_check} != recorded NM {nm_old}"),
        });
    }
    let md_check = make_mismatch_string(seq, &reference.ref_seq, cigar, &reference.md_seq);
    let md_check_value = md_check.strip_prefix("MD:Z:").unwrap_or(&md_check);
    if md_check_value != md_old {
        return Err(FiveBaseBisulfiteError::RoundTripFailed {
            qname: qname.to_string(),
            what: format!("recomputed MD {md_check_value:?} != recorded MD {md_old:?}"),
        });
    }

    // ---- the re-encode -------------------------------------------------------------
    let (meth, unmeth, ref_base) = (xg.meth(), xg.unmeth(), xg.ref_base());
    let mut seq_new = seq.to_vec();
    let mut letters = 0u32;
    let mut flipped = 0u32;
    let mut masked = 0u32;

    for (i, &call) in xm.iter().enumerate() {
        let want = match call {
            b'Z' | b'X' | b'H' | b'U' => Some(meth),
            b'z' | b'x' | b'h' | b'u' => Some(unmeth),
            b'.' => None,
            other => {
                return Err(FiveBaseBisulfiteError::InvalidXmByte {
                    qname: qname.to_string(),
                    byte: other as char,
                    offset: i,
                });
            }
        };

        match want {
            Some(base) => {
                // A letter can never sit in a CIGAR gap (property 3). Assert rather than
                // assume: this turns the invariant into a runtime check.
                if reference.ref_seq[i] == b'X' {
                    return Err(FiveBaseBisulfiteError::CallInGap {
                        qname: qname.to_string(),
                        byte: call as char,
                        offset: i,
                    });
                }
                // At a letter position the read base is already one of the pair. Cheap,
                // needs no MD, and catches a mis-zipped XM or an inverted XG→pair table.
                if seq[i] != meth && seq[i] != unmeth {
                    return Err(FiveBaseBisulfiteError::SeqNotInPair {
                        qname: qname.to_string(),
                        base: seq[i] as char,
                        offset: i,
                        meth: meth as char,
                        unmeth: unmeth as char,
                        xg: if xg == XgStrand::Ct { "CT" } else { "GA" }.to_string(),
                    });
                }
                letters += 1;
                if base != seq[i] {
                    flipped += 1;
                }
                seq_new[i] = base;
            }
            None => {
                // No call. Mask only if this is a reference cytosine still carrying a base
                // `patter` would score — see the module/function docs.
                if reference.ref_seq[i] == ref_base && (seq[i] == meth || seq[i] == unmeth) {
                    seq_new[i] = b'N';
                    masked += 1;
                }
            }
        }
    }

    // ---- emit: recomputed on the FINAL seq, so masking is included -----------------
    let nm = bismark_nm(&seq_new, &reference);
    let md_full = make_mismatch_string(&seq_new, &reference.ref_seq, cigar, &reference.md_seq);
    let md = md_full
        .strip_prefix("MD:Z:")
        .unwrap_or(&md_full)
        .to_string();

    Ok(Reencoded {
        seq: seq_new,
        nm,
        md,
        letters,
        flipped,
        masked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- reconstruct_ref -------------------------------------------------------------
    // Tested first and in isolation: everything else depends on it, including the §3.6
    // masking rule, so a bug here changes which bases get masked as well as the tags.

    #[test]
    fn md_tokenises() {
        assert_eq!(parse_md("8", "q").unwrap(), vec![MdTok::Match(8)]);
        // A zero run separates adjacent mismatches and must not become a Match(0).
        assert_eq!(
            parse_md("1C0C2", "q").unwrap(),
            vec![
                MdTok::Match(1),
                MdTok::Mismatch(b'C'),
                MdTok::Mismatch(b'C'),
                MdTok::Match(2)
            ]
        );
        assert_eq!(
            parse_md("3^TA3", "q").unwrap(),
            vec![
                MdTok::Match(3),
                MdTok::Deletion(b"TA".to_vec()),
                MdTok::Match(3)
            ]
        );
        assert!(matches!(
            parse_md("3^", "q"),
            Err(FiveBaseBisulfiteError::MalformedMd { .. })
        ));
    }

    #[test]
    fn ref_reconstructs_all_matches() {
        let r = reconstruct_ref(b"ACGTACGT", "8M", "8", "q").unwrap();
        assert_eq!(r.ref_seq, b"ACGTACGT");
        assert_eq!(r.deleted, 0);
    }

    #[test]
    fn ref_reconstructs_mismatches_from_md_letters() {
        // seq has T where the reference has C, at offsets 1 and 5.
        let r = reconstruct_ref(b"ATGTATGT", "8M", "1C3C2", "q").unwrap();
        assert_eq!(r.ref_seq, b"ACGTACGT");
    }

    #[test]
    fn ref_pads_soft_clips_and_insertions_with_x() {
        let sc = reconstruct_ref(b"GGACGTAC", "2S6M", "6", "q").unwrap();
        assert_eq!(sc.ref_seq, b"XXACGTAC");
        let ins = reconstruct_ref(b"ACGAGTA", "3M1I3M", "6", "q").unwrap();
        assert_eq!(ins.ref_seq, b"ACGXGTA");
    }

    #[test]
    fn ref_takes_deleted_bases_from_md_and_excludes_them_from_ref_seq() {
        let r = reconstruct_ref(b"ACGCGT", "3M2D3M", "3^TA3", "q").unwrap();
        // ref_seq is one byte per READ position, so the deletion is absent from it...
        assert_eq!(r.ref_seq, b"ACGCGT");
        // ...but present in md_seq, which spans M + D.
        assert_eq!(r.md_seq, b"ACGTACGT");
        assert_eq!(r.deleted, 2);
    }

    #[test]
    fn ref_rejects_a_cigar_seq_length_disagreement() {
        assert!(matches!(
            reconstruct_ref(b"ACGT", "8M", "8", "q"),
            Err(FiveBaseBisulfiteError::MalformedMd { .. })
        ));
    }

    #[test]
    fn ref_rejects_unsupported_cigar_ops() {
        // Hard clip: SEQ would be shorter than the read, breaking the positional contract.
        assert!(matches!(
            reconstruct_ref(b"ACGT", "2H4M", "4", "q"),
            Err(FiveBaseBisulfiteError::UnsupportedCigarOp { op: 'H', .. })
        ));
    }

    // ---- the XG table ---------------------------------------------------------------

    #[test]
    fn xg_fixes_the_pair_and_the_reference_base() {
        let ct = XgStrand::from_tag(b"CT", "q").unwrap();
        assert_eq!((ct.meth(), ct.unmeth(), ct.ref_base()), (b'C', b'T', b'C'));
        let ga = XgStrand::from_tag(b"GA", "q").unwrap();
        assert_eq!((ga.meth(), ga.unmeth(), ga.ref_base()), (b'G', b'A', b'G'));
        assert!(XgStrand::from_tag(b"XX", "q").is_err());
    }

    // ---- the re-encode --------------------------------------------------------------

    /// A bisulfite record is a FIXED POINT: `SEQ` already carries meth/unmeth at every
    /// letter position, so nothing moves and the tags are unchanged. This is the unit-level
    /// form of the idempotence gate.
    #[test]
    fn bisulfite_input_is_unchanged() {
        let out = reencode(
            b"ACGTACGT",
            b".Z...Z..",
            XgStrand::Ct,
            "8M",
            "8",
            0,
            "bisulfite",
        )
        .unwrap();
        assert_eq!(out.seq, b"ACGTACGT");
        assert_eq!(out.nm, 0);
        assert_eq!(out.md, "8");
        assert_eq!((out.letters, out.flipped, out.masked), (2, 0, 0));
    }

    /// The same molecule as 5-Base: methylated CpGs read `T`. Every letter position flips,
    /// and the output is byte-identical to the bisulfite record above — which is the whole
    /// point of the feature.
    #[test]
    fn five_base_input_flips_every_letter() {
        let out = reencode(
            b"ATGTATGT",
            b".Z...Z..",
            XgStrand::Ct,
            "8M",
            "1C3C2",
            2,
            "five_base",
        )
        .unwrap();
        assert_eq!(out.seq, b"ACGTACGT");
        assert_eq!(
            out.nm, 0,
            "flipping T->C turns both mismatches into matches"
        );
        assert_eq!(out.md, "8");
        assert_eq!((out.letters, out.flipped, out.masked), (2, 2, 0));
        // The flip-rate discriminator: exactly 1.0 for 5-Base.
        assert_eq!(out.flipped, out.letters);
    }

    /// `XG:Z:GA` uses the `(G, A)` pair. A reverse-strand record's `SEQ` is genome-forward
    /// and its `XM` is reversed in lockstep, so the zip stays positional — indexing by a
    /// 5'-oriented position instead would edit the wrong end.
    #[test]
    fn ga_strand_uses_the_g_a_pair() {
        // ref ACGTACGT; the bottom-strand cytosines are the forward G at offsets 2 and 6.
        // 5-Base methylated => read carries A there.
        let out = reencode(
            b"ACATACAT",
            b"..Z...Z.",
            XgStrand::Ga,
            "8M",
            "2G3G1",
            2,
            "ob",
        )
        .unwrap();
        assert_eq!(out.seq, b"ACGTACGT");
        assert_eq!(out.nm, 0);
        assert_eq!((out.letters, out.flipped), (2, 2));
    }

    /// Asymmetric `XM` — one methylated call near the start, one unmethylated near the end.
    /// A 5'-oriented write would swap which end changes, so this asserts the exact string.
    #[test]
    fn asymmetric_calls_edit_the_correct_positions() {
        // ref ACGTACGT; 5-Base read has T at both reference Cs (offsets 1 and 5).
        // XM: offset 1 methylated (-> C), offset 5 unmethylated (-> stays T).
        let out = reencode(
            b"ATGTATGT",
            b".Z...z..",
            XgStrand::Ct,
            "8M",
            "1C3C2",
            2,
            "asym",
        )
        .unwrap();
        assert_eq!(out.seq, b"ACGTATGT");
        assert_eq!((out.letters, out.flipped), (2, 1));
        assert_eq!(out.nm, 1, "offset 5 stays a mismatch");
        assert_eq!(out.md, "5C2");
    }

    /// Soft-clipped bases must survive untouched even when they are scoreable, because
    /// `ref_seq` is `b'X'` there. Note offset 7 IS masked: it is a reference `C` with a
    /// scoreable base and no call, which is the §3.6 rule doing its job — a useful reminder
    /// that "no call" and "in a gap" are different conditions.
    #[test]
    fn gaps_are_never_written_soft_clip() {
        let out = reencode(
            b"CCACGTAC",
            b"...Z....",
            XgStrand::Ct,
            "2S6M",
            "6",
            2,
            "clip",
        )
        .unwrap();
        assert_eq!(
            &out.seq[..2],
            b"CC",
            "soft-clipped bases must never be rewritten"
        );
        assert_eq!(out.seq, b"CCACGTAN");
        assert_eq!(out.masked, 1, "only the uncalled reference C at offset 7");
    }

    /// Same for an inserted position: `ref_seq[i] == b'X'`, so a scoreable base there is not
    /// a reference cytosine and must not be masked.
    #[test]
    fn gaps_are_never_written_insertion() {
        // ref_seq = A C G X G T A. Offset 3 is the insertion; offset 1 is a reference C.
        let out = reencode(
            b"ACGCGTA",
            b".......",
            XgStrand::Ct,
            "3M1I3M",
            "6",
            1,
            "ins",
        )
        .unwrap();
        assert_eq!(
            out.seq[3], b'C',
            "the INSERTED C must survive: ref_seq is X there, so it is not a reference cytosine"
        );
        // Offset 1 is a genuine uncalled reference C, so §3.6 masks it — the contrast with
        // offset 3 is exactly what the positional conjunct buys.
        assert_eq!(out.seq, b"ANGCGTA");
        assert_eq!(out.masked, 1);
    }

    // ---- masking (§3.6) -------------------------------------------------------------

    /// Positive AND negative control in one record. Offset 1 is a reference `C` still
    /// carrying a scoreable base with no call => masked. Offset 3 carries `T`, which is in
    /// the pair, but its reference base is `T` not `C` => must NOT be masked. That second
    /// assertion is the one that fails if the positional conjunct is dropped.
    #[test]
    fn masks_only_no_call_reference_cytosines() {
        let out = reencode(b"ACGT", b"....", XgStrand::Ct, "4M", "4", 0, "masked").unwrap();
        assert_eq!(out.seq, b"ANGT");
        assert_eq!(out.masked, 1);
        assert_eq!(out.letters, 0);
        assert_eq!(out.flipped, 0, "masking must never count as a flip");
        assert_eq!(out.nm, 1, "N vs reference C is a new mismatch");
        assert_eq!(out.md, "1C2");
    }

    /// The provable-emptiness property: a record whose calls are all present has nothing to
    /// mask, which is what makes the idempotence gate cover this path.
    #[test]
    fn no_masking_when_every_cytosine_is_called() {
        let out = reencode(b"ACGTACGT", b".Z...Z..", XgStrand::Ct, "8M", "8", 0, "full").unwrap();
        assert_eq!(out.masked, 0);
        assert_eq!(out.seq, b"ACGTACGT");
    }

    // ---- the deletion path: an independent from-genome oracle -------------------------
    // These two cases come from CODE_REVIEW_A, which built the from-genome MD/NM oracle the
    // plan listed as not-done and confirmed both against a reference supplied EXPLICITLY
    // (never reconstructed from MD, so it is not asserting a function equals itself). They
    // exercise `rebuild_md_with_deletions` -- the verbatim Perl port whose own comments read
    // "Perl dies -- unreachable" -- on a much denser MD than the round-trip proof had seen.
    // Inputs re-derived here mechanically; the recorded NM (8 and 19) matches the review's.

    /// Deletion with a mismatch immediately abutting it on both sides.
    #[test]
    fn oracle_deletion_with_abutting_mismatches() {
        let out = reencode(
            b"ATGTATGATGAGATGT",
            b".Z.Z.Z..Z....Z.Z",
            XgStrand::Ct,
            "8M2D8M",
            "1C1C1C2^GA0C4C1C0",
            8,
            "oracle1",
        )
        .unwrap();
        // Every methylated cytosine flips T->C, so the read becomes the reference and the
        // only remaining edit distance is the 2 deleted bases.
        assert_eq!(out.seq, b"ACGCACGACGAGACGC");
        assert_eq!(out.nm, 2);
        assert_eq!(out.md, "8^GA8");
        assert_eq!((out.letters, out.flipped, out.masked), (6, 6, 0));
    }

    /// The hard one: leading soft clip + two deletions + an insertion, with non-cytosine
    /// mismatches interleaved so `MD` stays dense after conversion.
    #[test]
    fn oracle_two_deletions_soft_clip_and_insertion() {
        let out = reencode(
            b"AAAATGTATGTTATTTTTTT",
            b"....Z.Z.Z.ZZ..Z.Z.Z.",
            XgStrand::Ct,
            "3S5M2D4M1I4M1D3M",
            "1C1C1^GA0C1C0C0G0C0A0C0^T0G0C0A0",
            19,
            "oracle2",
        )
        .unwrap();
        assert_eq!(out.seq, b"AAAACGCACGCCATCTCTCT");
        // Both values come from an INDEPENDENT oracle: `ref_seq` built explicitly, then MD/NM
        // derived from (seq_new, ref_seq, CIGAR) without consulting the input MD. The review's
        // figures were 13/"5^GA3C0G1A1^T0G0C0A0" for its own XM (6 flips); this record was
        // re-derived and has 8, so the emitted values legitimately differ -- what is being
        // gated is agreement with the oracle, not with the review's numbers.
        assert_eq!(out.nm, 11);
        assert_eq!(out.md, "5^GA4G1A1^T0G1A0");
        assert_eq!(out.flipped, out.letters, "every 5-Base call must flip");
        assert_eq!(
            out.masked, 0,
            "soft-clipped/inserted bases are ref_seq X, never masked"
        );
    }

    /// The `MD` half of the round-trip proof (the `NM` half is covered above). Review LOW-2b.
    #[test]
    fn rejects_a_record_whose_md_fails_the_round_trip() {
        // NM 0 is consistent with an all-match read, but MD "8" describes 8 aligned bases
        // while the CIGAR has 4 -- so the reconstruction cannot reproduce the recorded MD.
        let e = reencode(b"ACGT", b"....", XgStrand::Ct, "4M", "8", 0, "q").unwrap_err();
        assert!(
            matches!(e, FiveBaseBisulfiteError::MalformedMd { .. })
                || matches!(e, FiveBaseBisulfiteError::RoundTripFailed { .. }),
            "expected an MD failure, got {e:?}"
        );
    }

    /// `U`/`u` (unknown context) must be re-encoded like any other call. Nothing exercised
    /// them before: the fixture's letter census is Z/x/h only, so a change making them a
    /// silent skip rather than a hard error would have gone unnoticed (review + coverage).
    #[test]
    fn unknown_context_letters_are_re_encoded() {
        // Reference ACGTACGT; 5-Base methylated at both reference Cs, reported as U (an N in
        // the genomic context window) rather than Z.
        let up = reencode(
            b"ATGTATGT",
            b".U...u..",
            XgStrand::Ct,
            "8M",
            "1C3C2",
            2,
            "uu",
        )
        .unwrap();
        assert_eq!(up.seq, b"ACGTATGT", "U must flip to C; u must stay T");
        assert_eq!((up.letters, up.flipped), (2, 1));
        // And the upper-case CHG/CHH letters, likewise absent from every fixture.
        let xh = reencode(
            b"ATGTATGT",
            b".X...H..",
            XgStrand::Ct,
            "8M",
            "1C3C2",
            2,
            "xh",
        )
        .unwrap();
        assert_eq!(xh.seq, b"ACGTACGT", "X and H are both methylated -> C");
        assert_eq!((xh.letters, xh.flipped), (2, 2));
    }

    // ---- fail-loud ------------------------------------------------------------------

    #[test]
    fn rejects_an_xm_seq_length_mismatch() {
        let e = reencode(b"ACGT", b".Z.", XgStrand::Ct, "4M", "4", 0, "q").unwrap_err();
        assert!(matches!(
            e,
            FiveBaseBisulfiteError::LengthMismatch { xm: 3, seq: 4, .. }
        ));
        assert!(
            e.to_string().contains("q"),
            "the message must name the record"
        );
    }

    #[test]
    fn rejects_an_unknown_xm_byte() {
        let e = reencode(b"ACGT", b".Q..", XgStrand::Ct, "4M", "4", 0, "q").unwrap_err();
        assert!(matches!(
            e,
            FiveBaseBisulfiteError::InvalidXmByte {
                byte: 'Q',
                offset: 1,
                ..
            }
        ));
    }

    #[test]
    fn rejects_a_call_in_a_gap() {
        // A letter at a soft-clipped position is structurally impossible; fail rather than
        // write into the gap.
        let e = reencode(b"CCACGTAC", b"Z.......", XgStrand::Ct, "2S6M", "6", 2, "q").unwrap_err();
        assert!(matches!(
            e,
            FiveBaseBisulfiteError::CallInGap { offset: 0, .. }
        ));
    }

    #[test]
    fn rejects_a_letter_whose_base_is_outside_the_pair() {
        // XM claims a call at offset 0, but the base is `A` — neither C nor T.
        let e = reencode(b"ACGT", b"Z...", XgStrand::Ct, "4M", "4", 0, "q").unwrap_err();
        assert!(matches!(
            e,
            FiveBaseBisulfiteError::SeqNotInPair {
                base: 'A',
                offset: 0,
                ..
            }
        ));
    }

    #[test]
    fn rejects_a_record_whose_tags_fail_the_round_trip() {
        // NM says 5, the reconstruction says 0. Refuse rather than emit tags derived from a
        // reference we cannot prove.
        let e = reencode(b"ACGTACGT", b".Z...Z..", XgStrand::Ct, "8M", "8", 5, "q").unwrap_err();
        match e {
            FiveBaseBisulfiteError::RoundTripFailed { what, .. } => {
                assert!(what.contains("NM"), "got: {what}");
            }
            other => panic!("expected RoundTripFailed, got {other:?}"),
        }
    }
}
