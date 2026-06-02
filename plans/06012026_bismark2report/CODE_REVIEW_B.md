# Code Review B — `bismark-report` (Rust port of Perl `bismark2report`)

**Reviewer:** Code Reviewer B (fresh context, re-run after prior crash)
**Date:** 2026-06-01
**Crate:** `rust/bismark-report` @ branch `rust/bismark2report` (`~/Github/Bismark-report`)
**Acceptance gate:** generated HTML byte-for-byte identical to live Perl `bismark2report v0.25.1`, modulo the single `localtime` timestamp line.
**Verdict:** **One byte-identity-affecting bug found** (High) — `parse_report_for` drops `{{filename}}`/`{{bismark_version}}` on a **CRLF-terminated alignment report**. Everything else reviewed is faithful; 36 unit + 8 CLI + 4 Perl-vs-Rust golden tests pass, clippy `-D warnings` clean, and 11 additional adversarial Perl-vs-Rust cases I crafted are byte-identical except the one below.

---

## Summary

The port is a careful, well-structured reproduction of the Perl contract. Document assembly is byte-oriented (`Vec<u8>`), the substitution ORDER is preserved per parser, fill gates use `is_some()` (not truthiness), the M-bias `state`-vs-`%mbias_2` split is correctly modeled, the nucleotide fixed-20-key order + `0`/empty-string missing-key handling is right, the plotly separators (`,` / ` , ` / `','`) are correct, the asset normalizer faithfully replays `chomp`+`s/\r//g`+`\n` with the empty-input guard, and the multi-report companion-reset (Perl line 1256) is reproduced exactly. I verified all of this empirically by running BOTH binaries and `cmp`-ing timestamp-normalized HTML.

The single defect is in the alignment report's `Bismark report for:` parser: it is **stricter** than Perl's regex about what may follow the closing `)`, so a Windows/CRLF-terminated report diverges.

---

## Issues by area

### Logic / Errors

#### HIGH — `parse_report_for` drops filename + version on a CRLF-terminated (or trailing-content) alignment report → byte divergence

**File:** `src/reports/alignment.rs:133-144`
**Perl:** `bismark2report:226` — `elsif($_ =~ /^Bismark report for: (.*) \(version: (.*)\)/)`

The Rust code:

```rust
let after = &rest[vpos + sep.len()..];
if after.last() == Some(&b')') {           // <-- requires ')' to be the LAST byte
    a.input_filename = Some(fname.to_vec());
    a.bismark_version = Some(after[..after.len() - 1].to_vec());
}
```

requires the `)` to be the **final byte of the line**. Perl's regex has **no `$` end-anchor**, so it tolerates *any* trailing bytes after the last `)`. The realistic trigger is a **CRLF-terminated report**: Perl `chomp` removes only the `\n`, leaving a trailing `\r` on the line; the regex still matches (`(.*)` backtracks so `\)` lands on the real `)`, `\r` ignored). Rust sees `after = "v0.25.1)\r"`, whose last byte is `\r` ≠ `)`, so it sets **neither** field.

Because the 5-field gate still passes, Rust then substitutes `{{filename}}` (twice — `<title>` + header) and `{{bismark_version}}` (footer) with the **empty string**, while Perl injects the real values. **The HTML diverges.**

**Reproduction (verified against live Perl v0.25.1):**

Alignment report whose lines are terminated with `\r\n` (Windows), e.g. first line:
```
Bismark report for: crlf2_R1.fq.gz and crlf2_R2.fq.gz (version: v0.25.1)\r\n
```
+ the 5 mandatory PE stat lines (also `\r\n`). First normalized-HTML divergence at byte 150:
```
perl: ...<title>Bismark Processing Report - crlf2_R1.fq.gz and crlf2_R2.fq.gz</title>...
rust: ...<title>Bismark Processing Report - </title>...
```
`perl v0.25.1 count: 1, rust: 0` in the footer; filename count `2` vs `0`. (Diff isolated to filename×2 + version×1 — all other CRLF-retained `\r` values, e.g. `1000\r`, `77.2\r`, are identical between the two tools, so the bug is precisely localized to this one parser.)

I also characterized Perl's regex directly:
| input (after `report for: `)                       | Perl filename            | Perl version | Rust today |
|----------------------------------------------------|--------------------------|--------------|------------|
| `A and B (version: v0.25.1)\r`                      | `A and B`                | `v0.25.1`    | **none**   |
| `A and B (version: v0.25.1) ` (trailing space)      | `A and B`                | `v0.25.1`    | **none**   |
| `A and B (version: v0.25.1)` (clean)                | `A and B`                | `v0.25.1`    | OK         |
| `A (version: v1) extra (version: v2)`               | `A (version: v1) extra`  | `v2`         | OK (rfind) |

The `rfind` choice for the separator already correctly reproduces Perl's greedy-filename / last-`(version:)`-wins behavior — only the trailing-byte assumption is wrong.

**Recommended fix (recommend, not applied):** locate the **last** `)` *inside `after`* (not require it at the end), take version = bytes up to that `)`, and ignore any trailing bytes:

```rust
if let Some(rparen) = rfind(after, b")") {
    a.input_filename = Some(fname.to_vec());
    a.bismark_version = Some(after[..rparen].to_vec());
}
```

This handles CRLF, trailing whitespace, and the clean case identically to Perl. Add a CRLF-terminated alignment-report fixture to `tests/perl_vs_rust.rs` (none of the 4 committed fixtures is CRLF) and a `parse_report_for` unit test with a trailing `\r`.

**Severity rationale:** High, not Critical — it only fires on CRLF/trailing-content input, which standard Bismark output (LF) does not produce, so committed/oxy LF fixtures will not catch it. But CRLF reports are a realistic field input (Windows transfer, editor round-trips), and when present the output silently loses the sample name + version with no error — exactly the kind of silent wrong-output the gate exists to prevent.

---

### Logic — items verified CORRECT (no action)

- **Fill gates use `is_some()` not truthiness** (alignment 5 fields `alignment.rs:151`, dedup 4 `dedup.rs:60`, splitting 6 `splitting.rs:69`): verified `no_genomic:0` (c2/c6) and `dups:0`/`diff_pos:0` (c6) pass the gate and produce byte-identical HTML.
- **Gate-failure → surviving placeholders, exit 0** (`alignment.rs:157`): c4 (missing `no_genomic` line) byte-identical to Perl; placeholders survive.
- **Dedup leftover fallback `total - dups`** (`dedup.rs:41-45`): c1 (no leftover line) byte-identical; `before_first_ws` drops the trailing ` (NN.NN%)` correctly.
- **Splitting phrasing** (`splitting.rs:45-60`): unmethylated is **only** `Total C to T conversions …`, Unknown percentage is `C methylated in Unknown context:` (no `(CN or CHN)`) — matches Perl 745-781. Distinct from alignment's dual patterns.
- **Nucleotide fixed-20-key order + header validation + missing key** (`nucleotide.rs`): c2 (#711 amplicon, only A+AA present) byte-identical — absent keys render `0` for %, empty string for counts/coverage. Bad-header → `Err`. log2 ratio never emitted (Perl 657-660 commented; Rust never computes it). `s/\r//` first-only reproduced (`strip_first_cr`), confirmed via c5 CRLF nucleotide report = byte-identical.
- **Plotly separators** (`nucleotide.rs:114-130`): y-array `'A','T',…` (wrap+`','`), x-arrays joined ` , `; other plots `,`. c2/c5 confirm.
- **M-bias `state` vs `%mbias_2`** (`mbias.rs` + `template.rs:137-151`): SE → R2 `<div>` excised + 12 `{{mbias2_*}}` survive (golden `wgbs_se` + unit test); PE → both filled; **R2 header with no data rows** (c3) → `state=Paired` (R2 section kept) but R2 placeholders survive — byte-identical to Perl. The dead `{{bm_mbias_2}}` no-op is reproduced (`mbias.rs:98`).
- **M-bias context header match** (`mbias.rs:53`): `line[0]==b'C' && line[3..].starts_with(b" context")` is equivalent to Perl `^(C.{2}) context`; `R2` scanned over the whole line via `windows(2)` = Perl `/R2/`.
- **Unknown-context `<tr>` inject bytes** (`reports/mod.rs:96-114`): exact 5/32/(4sp+4tab)/(4sp+3tab) layout; `N/A%` row when `meth_unknown` present but `perc_unknown` absent (c10) byte-identical; nondir fixture (Unknown present) passes.
- **Section collapse/excise greedy-dotall** (`template.rs:48-66`): first→last inclusive splice; single-marker is a no-op (matches Perl regex non-match).
- **Asset normalizer** (`assets.rs:38-55`): `chomp`+`s/\r//g`+`\n` per line, empty→empty guard, drift test asserts embedded == on-disk. Brace/CR-free asset test guards the literal-splice safety assumption.
- **Multi-report companion reset (Perl 1256)** (`discovery.rs:48,119`): c7 (2 reports + explicit `--dedup_report`) — report #1 uses explicit dedup, report #2 auto-detects its own; both byte-identical to Perl.
- **Output naming**: `-o` verbatim (no `.html` append) + `--dir` w/o trailing slash (c8) byte-identical; derive-from-alignment + multi-report-one-html-each (cli.rs tests) pass.
- **Timestamp** (`timestamp.rs`): UTC civil math correct (epoch 0, leap day, known epoch); `localtime_r` unsafe block is sound (owned zeroed `tm`, `t` outlives the call) and is not byte-gated.

### Efficiency

- **Low — repeated full-document rescans.** Each `subst_all`/`collapse`/`excise` is an O(n) pass over the ~3 MB doc, and there are ~100+ substitutions, so the build is effectively O(n · substitutions) with a fresh `Vec` allocated per call (`template.rs:13-29`). For a 3 MB doc this is still well under a second and the byte-identity bar forbids cleverer single-pass schemes that might reorder substitutions (a later `{{name}}` can legally hit text an earlier one introduced — `reports/mod.rs` docstring notes this). **Acceptable; no change recommended** — matches the Perl model and is not a bottleneck for a per-sample report tool. Flagging only for completeness.

### Structure

- **Low — `assets.rs:7` doc comment references `tests/assets.rs`** for the drift test, but the drift + brace/CR tests actually live inline in `assets.rs` (`#[cfg(test)]`). Harmless stale comment; no `tests/assets.rs` exists. Recommend updating the comment to say "see the `#[cfg(test)] mod tests` below".
- **Low — `parse_report_for` elsif-consumption vs Perl.** Rust's `else if line.starts_with(b"Bismark report for: ")` consumes the branch on prefix alone, whereas Perl's `elsif` only consumes if the **full** regex matches. Verified harmless (c9: a `Bismark report for: …` line with **no** `(version: …)` is byte-identical — no later branch can match such a line). Document-only; the HIGH fix above is the substantive concern on this line.

---

## Recommendations (prioritized)

1. **HIGH — Fix `parse_report_for` (`alignment.rs:133-144`)** to match Perl's non-`$`-anchored regex: find the **last `)` within `after`** instead of requiring `)` to be the final byte; ignore trailing bytes. Add a CRLF-terminated alignment-report golden to `tests/perl_vs_rust.rs` + a `parse_report_for` unit test with trailing `\r`. (Byte-identity-affecting; trigger = CRLF / Windows-line-ending alignment report.)
2. **Low** — update the stale `tests/assets.rs` reference in `assets.rs:7`.
3. **Low** — optionally add a one-line comment on the `Bismark report for:` branch noting the deliberate prefix-vs-full-regex equivalence (verified harmless).

---

## Test evidence

- `cargo test -p bismark-report` → 36 lib + 8 cli + 4 perl_vs_rust = **48 passed, 0 failed** (perl available, v5.34.1).
- `cargo clippy -p bismark-report --all-targets -- -D warnings` → **clean**.
- 11 hand-crafted adversarial Perl-vs-Rust cases (timestamp-normalized `cmp`): c1 (SE+leftover-fallback), c2 (#711 amplicon + zeros-through-gate), c3 (R2-header-no-data), c4 (gate failure), c5 (CRLF nucleotide), c6 (dups=0), c7 (multi-report reset), c8 (`-o`+`--dir`), c9 (no-version line), c10 (`N/A%` inject) = **all byte-identical**; **c11 (CRLF alignment report) = DIVERGES** (the HIGH bug).
