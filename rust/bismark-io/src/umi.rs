//! UMI extractors for Bismark BAM qnames.
//!
//! Bismark's downstream `deduplicate_bismark` Perl script (v0.25.1)
//! supports two UMI encoding formats in the read ID:
//!
//! - **`--barcode` / `--umi`** — UMI is the tail-of-qname token after
//!   the last `:`, e.g. `MISEQ:...:CTCCTTAG`. Perl regex
//!   (`deduplicate_bismark:659`): `:([\w\+]+)$`.
//!
//! - **`--bclconvert`** — UMI is at an internal position in the
//!   bcl-convert read ID format, e.g.
//!   `A00001:...:CAAGAG_1:N:0:AATGACGC`. Perl regex
//!   (`deduplicate_bismark:650`):
//!   `:([CAGTN\+]+)_\d:N:\d:([CAGTN\+]+)$`.
//!
//! Both extractors below mirror Perl exactly — character classes,
//! anchoring, and empty-token rejection all match. Each returns
//! `Option<&[u8]>` borrowed from the input qname (zero-copy). The
//! caller chooses which extractor to call based on the user's CLI
//! flag (mode selection is upstream — these extractors are pure).
//!
//! Non-ASCII bytes (>= 0x80) fail the character-class checks and the
//! functions return `None`. Bismark BAM qnames are pure ASCII in
//! practice; non-ASCII input is handled gracefully.

/// Extract the `--barcode`/`--umi`-format UMI from a Bismark qname.
///
/// Matches Perl `deduplicate_bismark`'s regex at line 659:
/// `:([\w\+]+)$`. The UMI is the **tail token** of the qname,
/// separated by `:` from the rest, consisting only of word characters
/// (ASCII alphanumeric or underscore) and/or `+`.
///
/// Returns `None` if the qname has no `:`, the tail is empty, or
/// contains any character outside `[\w\+]`.
///
/// Non-ASCII bytes (any byte >= 0x80) fail the `[\w\+]` check and
/// the function returns `None` — Bismark BAM qnames are pure ASCII in
/// practice, but non-ASCII input is handled gracefully.
///
/// Zero-copy: the returned slice is borrowed from `qname`.
///
/// # Examples
///
/// ```
/// use bismark_io::umi;
/// // Post-fix_IDs format (what bismark-dedup actually sees in v1.2):
/// assert_eq!(
///     umi::extract_barcode(b"SRR24766921.1_A00686:91:HTHYKDMXX:1:1101:19696:1000:CAGCACTT"),
///     Some(b"CAGCACTT".as_slice())
/// );
/// // Perl docs canonical example:
/// assert_eq!(
///     umi::extract_barcode(b"MISEQ:14:000000000-A55D0:1:1101:18024:2858_1:N:0:CTCCTTAG"),
///     Some(b"CTCCTTAG".as_slice())
/// );
/// assert_eq!(
///     umi::extract_barcode(b"read_with_no_colon"),
///     None
/// );
/// assert_eq!(
///     umi::extract_barcode(b"read:with:slash/tail"),
///     None  // /tail is not [\w\+]
/// );
/// ```
pub fn extract_barcode(qname: &[u8]) -> Option<&[u8]> {
    // Step 1: find the last ':' in qname.
    let last_colon = qname.iter().rposition(|&b| b == b':')?;
    // Step 3: the candidate UMI is everything after the last colon.
    let candidate = &qname[last_colon + 1..];
    // Step 4: reject empty tails (Perl's `+` quantifier requires >= 1).
    if candidate.is_empty() {
        return None;
    }
    // Step 5: validate against [\w\+] = ASCII alnum or '_' or '+'.
    // Note: Rust's `u8::is_ascii_alphanumeric` does NOT include '_';
    // the explicit `|| b == b'_'` check is required to match Perl's
    // `\w` which is `[A-Za-z0-9_]`.
    if !candidate
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'+')
    {
        return None;
    }
    Some(candidate)
}

/// Extract the `--bclconvert`-format UMI from a Bismark qname.
///
/// Matches Perl `deduplicate_bismark`'s regex at line 650:
/// `:([CAGTN\+]+)_\d:N:\d:([CAGTN\+]+)$`. The UMI is the **first
/// capture group** (the `[CAGTN+]+` immediately after a `:` and
/// followed by `_<digit>:N:<digit>:i7`).
///
/// Returns `None` if the qname doesn't match the expected structure
/// (no trailing `_<d>:N:<d>:i7` segment, UMI not strictly `[CAGTN+]+`,
/// or i7 not strictly `[CAGTN+]+`).
///
/// Non-ASCII bytes (any byte >= 0x80) fail the `[CAGTN+]` check and
/// the function returns `None`. Bismark BAM qnames are pure ASCII in
/// practice; non-ASCII input is handled gracefully.
///
/// Zero-copy: the returned slice is borrowed from `qname`.
///
/// # Examples
///
/// ```
/// use bismark_io::umi;
/// // Post-fix_IDs format (what bismark-dedup actually sees in v1.2):
/// assert_eq!(
///     umi::extract_bclconvert(b"SRR24766921.1_A00686:91:HTHYKDMXX:1:1101:19696:1000:CAGCACTT_1:N:0:NNNNNNNN"),
///     Some(b"CAGCACTT".as_slice())
/// );
/// // Perl docs canonical example:
/// assert_eq!(
///     umi::extract_bclconvert(b"A00001:001:HN2F7DRX1:1:1101:1452:1000:CAAGAG_1:N:0:AATGACGC"),
///     Some(b"CAAGAG".as_slice())
/// );
/// assert_eq!(
///     // i7 with N placeholders (matches the regex's `[CAGTN+]` char class)
///     umi::extract_bclconvert(b"a:b:CAAGAG_1:N:0:NNNNNNNN"),
///     Some(b"CAAGAG".as_slice())
/// );
/// assert_eq!(
///     umi::extract_bclconvert(b"no_internal_structure"),
///     None
/// );
/// ```
pub fn extract_bclconvert(qname: &[u8]) -> Option<&[u8]> {
    // Step 1: find the last ':' (i7 separator).
    let i7_colon = qname.iter().rposition(|&b| b == b':')?;
    // Step 2: i7 candidate must be non-empty and strictly [CAGTN+].
    let i7 = &qname[i7_colon + 1..];
    if i7.is_empty() || !i7.iter().all(is_cagtn_plus) {
        return None;
    }
    // Step 3: underflow guard — need 6 bytes before i7_colon for the
    // `_<digit>:N:<digit>` infix.
    if i7_colon < 6 {
        return None;
    }
    // Step 4: validate the 6-byte infix pattern `_<d>:N:<d>`.
    let infix = &qname[i7_colon - 6..i7_colon];
    if infix[0] != b'_'
        || !infix[1].is_ascii_digit()
        || infix[2] != b':'
        || infix[3] != b'N'
        || infix[4] != b':'
        || !infix[5].is_ascii_digit()
    {
        return None;
    }
    // Step 5: UMI ends just before the '_' at infix[0].
    let umi_end = i7_colon - 6;
    // Step 6: find the ':' immediately before the UMI.
    let umi_colon = qname[..umi_end].iter().rposition(|&b| b == b':')?;
    // Step 7: UMI candidate must be non-empty and strictly [CAGTN+].
    let umi = &qname[umi_colon + 1..umi_end];
    if umi.is_empty() || !umi.iter().all(is_cagtn_plus) {
        return None;
    }
    Some(umi)
}

/// Predicate matching Perl's `[CAGTN\+]` character class: literal
/// DNA-base letters `C`, `A`, `G`, `T`, the ambiguity character `N`,
/// or the `+` dual-UMI separator.
fn is_cagtn_plus(b: &u8) -> bool {
    matches!(b, b'C' | b'A' | b'G' | b'T' | b'N' | b'+')
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────── extract_barcode ───────────────────

    #[test]
    fn extract_barcode_perl_example() {
        // The exact example from Perl's --barcode help text.
        assert_eq!(
            extract_barcode(b"MISEQ:14:000000000-A55D0:1:1101:18024:2858_1:N:0:CTCCTTAG"),
            Some(b"CTCCTTAG".as_slice())
        );
    }

    #[test]
    fn extract_barcode_simple() {
        assert_eq!(extract_barcode(b"read:UMI"), Some(b"UMI".as_slice()));
    }

    #[test]
    fn extract_barcode_with_underscore_and_plus() {
        // Underscore and '+' are in [\w\+] per Perl regex.
        assert_eq!(extract_barcode(b"x:Y_+Z"), Some(b"Y_+Z".as_slice()));
    }

    #[test]
    fn extract_barcode_no_colon() {
        assert_eq!(extract_barcode(b"noseparator"), None);
    }

    #[test]
    fn extract_barcode_empty_tail() {
        assert_eq!(extract_barcode(b"trailing:"), None);
    }

    #[test]
    fn extract_barcode_invalid_char() {
        // '/' is not in [\w\+].
        assert_eq!(extract_barcode(b"x:has/slash"), None);
    }

    #[test]
    fn extract_barcode_empty_input() {
        assert_eq!(extract_barcode(b""), None);
    }

    #[test]
    fn extract_barcode_only_colon() {
        assert_eq!(extract_barcode(b":"), None);
    }

    #[test]
    fn extract_barcode_after_fix_ids() {
        // The post-Bismark-fix_IDs format that bismark-dedup actually
        // sees: SRA prefix joined to Illumina qname via underscore.
        assert_eq!(
            extract_barcode(b"SRR24766921.1_A00686:91:HTHYKDMXX:1:1101:19696:1000:CAGCACTT"),
            Some(b"CAGCACTT".as_slice())
        );
    }

    #[test]
    fn extract_barcode_with_slash_tail() {
        // Catches the Phase 0 bug class: if synth_umi.py preserves
        // the /1 mate suffix, the BAM qname ends with `:UMI/1` and
        // Perl's regex fails (because `/` is not in [\w\+]). Our
        // extractor must report None for this case so Phase B can
        // surface the misconfiguration loudly.
        assert_eq!(extract_barcode(b"x:UMI/1"), None);
    }

    #[test]
    fn extract_barcode_only_digits() {
        // Locks in plan §11's "no first-char restriction" — Perl's
        // [\w\+] allows digits at any position including the start.
        assert_eq!(extract_barcode(b"x:12345678"), Some(b"12345678".as_slice()));
    }

    #[test]
    fn extract_barcode_with_plus_dual_umi() {
        // Perl's [\w\+] allows '+' as a dual-UMI separator. The
        // Illumina dual-UMI convention writes paired barcodes as
        // `UMI1+UMI2`.
        assert_eq!(
            extract_barcode(b"x:CAGCACTT+TTAGTTGT"),
            Some(b"CAGCACTT+TTAGTTGT".as_slice())
        );
    }

    // ─────────────────── extract_bclconvert ───────────────────

    #[test]
    fn extract_bclconvert_perl_example() {
        // The exact example from Perl's --bclconvert help text.
        assert_eq!(
            extract_bclconvert(b"A00001:001:HN2F7DRX1:1:1101:1452:1000:CAAGAG_1:N:0:AATGACGC"),
            Some(b"CAAGAG".as_slice())
        );
    }

    #[test]
    fn extract_bclconvert_n_placeholder_i7() {
        // Phase 0's synthesized format: i7 is all-N (the regex's
        // [CAGTN+] class accepts N).
        assert_eq!(
            extract_bclconvert(b"a:b:CAAGAG_1:N:0:NNNNNNNN"),
            Some(b"CAAGAG".as_slice())
        );
    }

    #[test]
    fn extract_bclconvert_after_fix_ids() {
        // Post-Bismark-fix_IDs format.
        assert_eq!(
            extract_bclconvert(
                b"SRR24766921.1_A00686:91:HTHYKDMXX:1:1101:19696:1000:CAGCACTT_1:N:0:NNNNNNNN"
            ),
            Some(b"CAGCACTT".as_slice())
        );
    }

    #[test]
    fn extract_bclconvert_r2_mate() {
        // R2 differs only in the mate-id digit (`_2:` vs `_1:`); the
        // UMI must extract identically.
        assert_eq!(
            extract_bclconvert(b"x:y:CAGCACTT_2:N:0:NNNNNNNN"),
            Some(b"CAGCACTT".as_slice())
        );
    }

    #[test]
    fn extract_bclconvert_no_internal_structure() {
        assert_eq!(extract_bclconvert(b"flatname"), None);
    }

    #[test]
    fn extract_bclconvert_no_colon_before_umi() {
        // Pattern matches up to `_1:N:0:i7` but there's no `:` before
        // the UMI for the extractor to anchor on.
        assert_eq!(extract_bclconvert(b"UMIonlyatstart_1:N:0:CAGT"), None);
    }

    #[test]
    fn extract_bclconvert_umi_with_slash() {
        // '/' is not in [CAGTN+].
        assert_eq!(extract_bclconvert(b"x:UMI/1_1:N:0:CAGT"), None);
    }

    #[test]
    fn extract_bclconvert_empty_umi() {
        assert_eq!(extract_bclconvert(b"x:_1:N:0:CAGT"), None);
    }

    #[test]
    fn extract_bclconvert_empty_i7() {
        assert_eq!(extract_bclconvert(b"x:UMI_1:N:0:"), None);
    }

    #[test]
    fn extract_bclconvert_i7_with_invalid_char() {
        // i7 must be strictly [CAGTN+].
        assert_eq!(extract_bclconvert(b"x:CAGCACTT_1:N:0:AB12CD"), None);
    }

    #[test]
    fn extract_bclconvert_wrong_n_byte() {
        // The `N` byte in `_<d>:N:<d>:` must literally be 'N'.
        assert_eq!(extract_bclconvert(b"x:UMI_1:Y:0:CAGT"), None);
    }

    #[test]
    fn extract_bclconvert_short_pattern() {
        // No UMI before the '_'.
        assert_eq!(extract_bclconvert(b":_1:N:0:CAGT"), None);
    }

    #[test]
    fn extract_bclconvert_with_plus_dual_umi() {
        // Perl's [CAGTN\+] allows '+' for dual-UMI separator. Per Perl
        // docs line 645.
        assert_eq!(
            extract_bclconvert(b"x:y:CAGCACTT+TTAGTTGT_1:N:0:NNNNNNNN"),
            Some(b"CAGCACTT+TTAGTTGT".as_slice())
        );
    }

    #[test]
    fn extract_bclconvert_empty_input() {
        // Underflow guard: empty input has no `:` so the rposition
        // returns None at step 1; never reaches the underflow check.
        assert_eq!(extract_bclconvert(b""), None);
    }

    #[test]
    fn extract_bclconvert_sub_6_bytes_before_i7_colon() {
        // i7_colon = position of last ':' = 2 (in "ab:CD"), but
        // i7_colon < 6 so the underflow guard returns None.
        assert_eq!(extract_bclconvert(b"ab:CD"), None);
    }

    #[test]
    fn extract_bclconvert_single_char_umi() {
        // Minimum-length UMI (1 byte). Locks the `> 0` (not `>= 2`)
        // boundary mutation slot.
        assert_eq!(extract_bclconvert(b"x:C_1:N:0:NNNN"), Some(b"C".as_slice()));
    }

    #[test]
    fn extract_bclconvert_single_char_i7() {
        // Minimum-length i7 (1 byte). Mirror of the single-char-UMI
        // boundary test.
        assert_eq!(
            extract_bclconvert(b"x:CAGCACTT_1:N:0:N"),
            Some(b"CAGCACTT".as_slice())
        );
    }
}
