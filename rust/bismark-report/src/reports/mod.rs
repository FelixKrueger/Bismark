//! Report parsers. Each submodule exposes a pure `parse(&[u8]) -> Captured` plus
//! a `fill(doc, &Captured) -> doc`. The split keeps parsing unit-testable
//! without any template, and concentrates Perl's **sequential** whole-document
//! substitution ORDER in `fill` (SPEC §8: a later `{{name}}` replace can legally
//! hit text an earlier replace introduced, so order is preserved per parser).

pub mod alignment;
pub mod dedup;
pub mod mbias;
pub mod nucleotide;
pub mod splitting;

/// Iterate report lines like Perl `while (<FH>)`: split on `\n`; a trailing
/// `\n` does NOT yield an extra empty record; empty input yields no lines.
/// CRLF: the `\r` stays on the line — Perl `chomp` removes only `\n` (the
/// nucleotide parser strips `\r` itself; the others do not).
pub fn report_lines(data: &[u8]) -> Vec<&[u8]> {
    if data.is_empty() {
        return Vec::new();
    }
    let mut parts: Vec<&[u8]> = data.split(|&b| b == b'\n').collect();
    if data.last() == Some(&b'\n') {
        parts.pop();
    }
    parts
}

/// Perl `split /\t/` semantics: split on tabs AND drop trailing empty fields.
pub fn split_tab(line: &[u8]) -> Vec<&[u8]> {
    let mut v: Vec<&[u8]> = line.split(|&b| b == b'\t').collect();
    while matches!(v.last(), Some(x) if x.is_empty()) {
        v.pop();
    }
    v
}

/// Perl `(undef, $x) = split /\t/` — field index 1 after trailing-empty removal
/// (so `"label\t"` → `None`, matching Perl's dropped trailing empty field).
pub fn field1(line: &[u8]) -> Option<&[u8]> {
    split_tab(line).get(1).copied()
}

/// Owned copy of `field1` (captured values must outlive the read buffer).
pub fn field1_owned(line: &[u8]) -> Option<Vec<u8>> {
    field1(line).map(|s| s.to_vec())
}

/// Remove the FIRST `%` (Perl `s/%//`, no `/g`).
pub fn strip_first_percent(v: &[u8]) -> Vec<u8> {
    match v.iter().position(|&b| b == b'%') {
        Some(p) => {
            let mut out = v.to_vec();
            out.remove(p);
            out
        }
        None => v.to_vec(),
    }
}

/// Truncate at the first ASCII whitespace (Perl `s/\s.*//`).
pub fn before_first_ws(v: &[u8]) -> &[u8] {
    match v.iter().position(|b| b.is_ascii_whitespace()) {
        Some(p) => &v[..p],
        None => v,
    }
}

/// Graph value for a methylation percentage: `"N/A"` → `"0"` (so Plotly renders
/// nothing rather than erroring), otherwise the value verbatim. A free function
/// (not a closure) so the `'static` `b"0"` and the borrowed input unify.
pub fn graph_value(disp: &[u8]) -> &[u8] {
    if disp == b"N/A" { b"0" } else { disp }
}

/// Join byte slices with a separator (for the Plotly data strings).
pub fn join_with(parts: &[&[u8]], sep: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(sep);
        }
        out.extend_from_slice(p);
    }
    out
}

fn pad(out: &mut Vec<u8>, byte: u8, n: usize) {
    out.resize(out.len() + n, byte);
}

/// The Unknown-context `<tr>` block injected into `{{(un)meth_unknown*}}` /
/// `{{perc_unknown*}}` (Perl 433-444 / 817-828). Byte-exact: `<tr>` line = 5
/// spaces; `<th>` line = 32 spaces; `<td>` line = 4 spaces + 4 tabs; `</tr>`
/// line = 4 spaces + 3 tabs; lines joined by `\n`, no trailing newline.
/// `suffix` is `b""` for the count rows and `b"%"` for the percentage row.
pub fn unknown_tr(th_label: &[u8], value: &[u8], suffix: &[u8]) -> Vec<u8> {
    let mut s = Vec::new();
    pad(&mut s, b' ', 5);
    s.extend_from_slice(b"<tr>\n");
    pad(&mut s, b' ', 32);
    s.extend_from_slice(b"<th>");
    s.extend_from_slice(th_label);
    s.extend_from_slice(b"</th>\n");
    pad(&mut s, b' ', 4);
    pad(&mut s, b'\t', 4);
    s.extend_from_slice(b"<td>");
    s.extend_from_slice(value);
    s.extend_from_slice(suffix);
    s.extend_from_slice(b"</td>\n");
    pad(&mut s, b' ', 4);
    pad(&mut s, b'\t', 3);
    s.extend_from_slice(b"</tr>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field1_drops_trailing_empty() {
        assert_eq!(field1(b"label\tvalue"), Some(&b"value"[..]));
        assert_eq!(field1(b"label\t"), None); // Perl drops trailing empty field
        assert_eq!(field1(b"label"), None);
        assert_eq!(field1(b"a\tb\tc"), Some(&b"b"[..])); // only field index 1
    }

    #[test]
    fn report_lines_drops_final_empty() {
        assert_eq!(report_lines(b"a\nb\n"), vec![&b"a"[..], &b"b"[..]]);
        assert_eq!(report_lines(b"a\nb"), vec![&b"a"[..], &b"b"[..]]);
        assert!(report_lines(b"").is_empty());
        assert_eq!(report_lines(b"x\r"), vec![&b"x\r"[..]]); // CR stays
    }

    #[test]
    fn percent_and_ws_trims() {
        assert_eq!(strip_first_percent(b"12.34%"), b"12.34".to_vec());
        assert_eq!(before_first_ws(b"678 (5.5%)"), b"678");
    }

    #[test]
    fn unknown_tr_byte_layout() {
        let got = unknown_tr(b"Methylated C's in Unknown context", b"42", b"");
        let want = "     <tr>\n                                <th>Methylated C's in Unknown context</th>\n    \t\t\t\t<td>42</td>\n    \t\t\t</tr>";
        assert_eq!(got, want.as_bytes());
    }
}
