//! Bismark strand classification.
//!
//! Bismark encodes the strand across two SAM optional tags rather than a
//! single field:
//!
//! | Tag      | Meaning                                       | Values       |
//! |----------|-----------------------------------------------|--------------|
//! | `XR:Z:`  | Read conversion (how this record was sequenced) | `CT` or `GA` |
//! | `XG:Z:`  | Genome conversion (which converted reference)  | `CT` or `GA` |
//!
//! The four-way strand is derived from the 2×2 combination:
//!
//! | XR | XG | → Strand                  |
//! |----|----|---------------------------|
//! | CT | CT | OT (Original Top)         |
//! | GA | CT | CTOT (Complementary to OT)|
//! | CT | GA | OB (Original Bottom)      |
//! | GA | GA | CTOB (Complementary to OB)|
//!
//! See `DESIGN.md` Q1 for the API rationale, in particular the distinction
//! between **per-record** and **per-pair** strand classification: this enum
//! represents the per-record value derived from XR/XG. The pair-level
//! classification is in [`crate::io::pair::BismarkPair`].

use crate::io::error::BismarkIoError;

/// Bismark four-way strand classification.
///
/// `#[repr(u8)]` pins the layout to a single byte so downstream consumers
/// (e.g. `bismark-dedup`'s `DedupKey`) can make explicit, stable
/// size-of-struct contracts. Without this annotation, Rust's default-enum
/// layout is unspecified, and a future compiler change could grow the
/// discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BismarkStrand {
    /// Original Top (XR=CT, XG=CT).
    OT,
    /// Complementary to Original Top (XR=GA, XG=CT).
    CTOT,
    /// Original Bottom (XR=CT, XG=GA).
    OB,
    /// Complementary to Original Bottom (XR=GA, XG=GA).
    CTOB,
}

impl BismarkStrand {
    /// Derive the strand from raw `XR:Z:` and `XG:Z:` tag byte slices.
    ///
    /// Returns [`BismarkIoError::InvalidStrandTags`] if either slice is not
    /// the literal `b"CT"` or `b"GA"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use bismark::io::BismarkStrand;
    ///
    /// assert_eq!(BismarkStrand::from_xr_xg(b"CT", b"CT").unwrap(), BismarkStrand::OT);
    /// assert_eq!(BismarkStrand::from_xr_xg(b"GA", b"CT").unwrap(), BismarkStrand::CTOT);
    /// assert_eq!(BismarkStrand::from_xr_xg(b"CT", b"GA").unwrap(), BismarkStrand::OB);
    /// assert_eq!(BismarkStrand::from_xr_xg(b"GA", b"GA").unwrap(), BismarkStrand::CTOB);
    /// ```
    pub fn from_xr_xg(xr: &[u8], xg: &[u8]) -> Result<Self, BismarkIoError> {
        match (xr, xg) {
            (b"CT", b"CT") => Ok(Self::OT),
            (b"GA", b"CT") => Ok(Self::CTOT),
            (b"CT", b"GA") => Ok(Self::OB),
            (b"GA", b"GA") => Ok(Self::CTOB),
            _ => Err(BismarkIoError::InvalidStrandTags {
                xr: xr.to_vec(),
                xg: xg.to_vec(),
            }),
        }
    }

    /// Canonical Bismark string representation, used in output filenames
    /// (`CpG_OT_*.txt`, `CHG_CTOT_*.txt`, etc.) and reports.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OT => "OT",
            Self::CTOT => "CTOT",
            Self::OB => "OB",
            Self::CTOB => "CTOB",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_xr_xg_all_four_valid_combinations() {
        assert_eq!(
            BismarkStrand::from_xr_xg(b"CT", b"CT").unwrap(),
            BismarkStrand::OT
        );
        assert_eq!(
            BismarkStrand::from_xr_xg(b"GA", b"CT").unwrap(),
            BismarkStrand::CTOT
        );
        assert_eq!(
            BismarkStrand::from_xr_xg(b"CT", b"GA").unwrap(),
            BismarkStrand::OB
        );
        assert_eq!(
            BismarkStrand::from_xr_xg(b"GA", b"GA").unwrap(),
            BismarkStrand::CTOB
        );
    }

    #[test]
    fn from_xr_xg_rejects_lowercase() {
        // Bismark spec requires uppercase; we do not normalise.
        assert!(matches!(
            BismarkStrand::from_xr_xg(b"ct", b"CT"),
            Err(BismarkIoError::InvalidStrandTags { .. })
        ));
        assert!(matches!(
            BismarkStrand::from_xr_xg(b"CT", b"ga"),
            Err(BismarkIoError::InvalidStrandTags { .. })
        ));
    }

    #[test]
    fn from_xr_xg_rejects_empty() {
        assert!(matches!(
            BismarkStrand::from_xr_xg(b"", b""),
            Err(BismarkIoError::InvalidStrandTags { .. })
        ));
        assert!(matches!(
            BismarkStrand::from_xr_xg(b"", b"CT"),
            Err(BismarkIoError::InvalidStrandTags { .. })
        ));
    }

    #[test]
    fn from_xr_xg_rejects_wrong_length() {
        assert!(matches!(
            BismarkStrand::from_xr_xg(b"C", b"CT"),
            Err(BismarkIoError::InvalidStrandTags { .. })
        ));
        assert!(matches!(
            BismarkStrand::from_xr_xg(b"CTT", b"CT"),
            Err(BismarkIoError::InvalidStrandTags { .. })
        ));
    }

    #[test]
    fn from_xr_xg_rejects_unrelated_bytes() {
        assert!(matches!(
            BismarkStrand::from_xr_xg(b"AA", b"TT"),
            Err(BismarkIoError::InvalidStrandTags { .. })
        ));
        assert!(matches!(
            BismarkStrand::from_xr_xg(b"\x00\x01", b"CT"),
            Err(BismarkIoError::InvalidStrandTags { .. })
        ));
    }

    #[test]
    fn from_xr_xg_error_preserves_input_bytes() {
        let err = BismarkStrand::from_xr_xg(b"XX", b"YY").unwrap_err();
        match err {
            BismarkIoError::InvalidStrandTags { xr, xg } => {
                assert_eq!(xr, b"XX");
                assert_eq!(xg, b"YY");
            }
            other => panic!("expected InvalidStrandTags, got {other:?}"),
        }
    }

    #[test]
    fn as_str_matches_bismark_canonical_names() {
        assert_eq!(BismarkStrand::OT.as_str(), "OT");
        assert_eq!(BismarkStrand::CTOT.as_str(), "CTOT");
        assert_eq!(BismarkStrand::OB.as_str(), "OB");
        assert_eq!(BismarkStrand::CTOB.as_str(), "CTOB");
    }

    #[test]
    fn enum_is_copy_and_hashable() {
        use std::collections::HashSet;
        let s = BismarkStrand::OT;
        let _t = s; // Copy works (s still usable below)
        let mut set: HashSet<BismarkStrand> = HashSet::new();
        set.insert(s);
        set.insert(BismarkStrand::CTOT);
        set.insert(BismarkStrand::OT); // duplicate of the first insert
        assert_eq!(set.len(), 2, "duplicate OT should not be re-counted");
    }
}
