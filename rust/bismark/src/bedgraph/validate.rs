//! Methylation-call validation, mirroring Perl `validate_methylation_call`
//! (`bismark2bedGraph:558-588`).
//!
//! The branch (CpG vs non-CpG) is chosen by the call letter: Perl tests
//! `$meth_state2 =~ /^z/i` (`:564`), so a call starting with `z`/`Z` takes
//! the CpG branch. Comparisons are **full-field string equality** (Perl
//! `eq`), which is what makes CRLF input degrade exactly as in Perl: a
//! trailing `\r` on the last (call) field makes `call == "Z"` fail → the
//! line is treated as inconsistent and skipped (SPEC §7 CRLF note).

/// Returns `true` if `(strand, call)` is a consistent Bismark methylation
/// call. Both arguments are the **full** tab-separated fields (not single
/// bytes), matching Perl's `eq` semantics.
///
/// - CpG branch (call starts with `z`/`Z`): valid iff `(+, "Z")` or `(-, "z")`.
/// - non-CpG branch: valid iff `(+, "Z"|"X"|"H")` or `(-, "z"|"x"|"h")`.
///   (`"Z"`/`"z"` never actually reach this branch — they take the CpG
///   branch — but the listing matches Perl `check_nonCpG_methylation_call`.)
#[must_use]
pub fn validate_call(strand: &str, call: &str) -> bool {
    let is_cpg = call.starts_with('z') || call.starts_with('Z');
    if is_cpg {
        (strand == "+" && call == "Z") || (strand == "-" && call == "z")
    } else {
        match strand {
            "+" => call == "Z" || call == "X" || call == "H",
            "-" => call == "z" || call == "x" || call == "h",
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpg_valid_pairs() {
        assert!(validate_call("+", "Z"));
        assert!(validate_call("-", "z"));
    }

    #[test]
    fn cpg_invalid_pairs() {
        assert!(!validate_call("+", "z")); // wrong case for +
        assert!(!validate_call("-", "Z")); // wrong case for -
    }

    #[test]
    fn non_cpg_valid_pairs() {
        assert!(validate_call("+", "X"));
        assert!(validate_call("+", "H"));
        assert!(validate_call("-", "x"));
        assert!(validate_call("-", "h"));
    }

    #[test]
    fn non_cpg_invalid_pairs() {
        assert!(!validate_call("+", "x"));
        assert!(!validate_call("-", "X"));
        assert!(!validate_call("+", "h"));
    }

    #[test]
    fn unknown_context_is_invalid() {
        // U/u (unknown context) must be skipped, like Perl.
        assert!(!validate_call("+", "U"));
        assert!(!validate_call("-", "u"));
    }

    #[test]
    fn crlf_corrupted_call_is_invalid() {
        // A trailing \r on the call field (CRLF input) → not equal to "Z"
        // → inconsistent → skipped (matches Perl's eq-based degrade).
        assert!(!validate_call("+", "Z\r"));
    }

    #[test]
    fn bad_strand_is_invalid() {
        assert!(!validate_call("?", "Z"));
        assert!(!validate_call("", "X"));
    }
}
