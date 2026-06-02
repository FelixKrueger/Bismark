# PLAN_REVIEW_B — `bismark-genome-preparation` phased implementation plan

**Reviewer:** B (independent, fresh context)
**Date:** 2026-05-30
**Target:** `plans/05302026_bismark-genome-preparation/PLAN.md` (rev 0)
**Companion:** `SPEC.md` (rev 2)
**Perl source of truth:** `bismark_genome_preparation` (848 lines, v0.25.1)
**Prior art audited:** `alanhoyle-bismark-rustport/rust/bismark-genome-prep/src/main.rs`

**Verdict:** The plan is well-structured, phase boundaries are sound, the reuse map is right (no `bismark-io`), and the headline byte-identity traps (CRLF preservation, final-no-newline, slam `_CT_`/`_GA_` suffix, raw-line transform) are all correctly identified and pinned. **However, I found two byte-identity defects in the spec/plan's description of `extract_chromosome_name` that, if implemented as written, will diverge from Perl** — and both are independently reproduced (with experimental confirmation below). These are Critical because they sit directly on the acceptance gate. Everything else is Important/Optional.

---

## 1. Logic review

### 1.1 CRITICAL — bare `>` header must NOT error (PLAN A3 / SPEC §8.9 are wrong)

PLAN A3 unit test list: *"name extraction (`>chr1 desc` → `chr1`; CRLF header `>chr1\r` → `chr1`; **bare `>` / no-`>` → error**)."* SPEC §8.9 likewise: *"a file whose first line isn't `>` → die."* The coverage table row 22 maps "First line not `>` → error" to A3/C2.

The "no-`>` → error" half is correct. The "**bare `>` → error**" half is **wrong**. Perl `extract_chromosome_name` (lines 575–581):

```perl
if ($fasta_header =~ s/^>//){          # succeeds for ">" too (strips the > leaving "")
    my ($chromosome_name) = split (/\s+/,$fasta_header);   # split("","") -> empty list
    return $chromosome_name;            # undef
}
else{ die ... }                         # only reached if header does NOT start with >
```

Verified experimentally:
```
bare ">"      -> name = undef  (does NOT die; "use warnings" emits a warning, execution continues)
"chr1" (no >) -> DIE
```
Downstream, `undef` is used in string context: line 427 `print CT_CONVERT ">",$chromosome_name,"_CT_converted\n"` emits **`>_CT_converted\n`** (empty name), and in `--single_fasta` mode opens **`.CT_conversion.fa`** / **`.GA_conversion.fa`**. So a genome whose record header is a bare `>` is *accepted* and produces a specific byte output. A Rust port that errors here fails byte-identity on that (admittedly pathological) input, and — more importantly — a `code-reviewer`/`plan-manager` audit will green-light the wrong behavior because the plan told it to.

**Action:** Change the A3 test + SPEC §8.9 to: *only* a first line **not** starting with `>` errors; a bare `>` (or a `>`-then-whitespace) produces an **empty chromosome name** used verbatim. Add a fixture pinning `>` → header `>_CT_converted`.

### 1.2 CRITICAL — leading whitespace after `>` yields an EMPTY name in Perl, not the next token

PLAN A3/A5 and SPEC §2.5/§5.2 describe the name as *"first whitespace-delimited token"*. The alanhoyle reference (the plan's structural model) implements this as `split_whitespace().next()`. These two are **not equivalent** when the header has leading whitespace after `>`:

Perl `split /\s+/` on a string with leading whitespace returns a **leading empty field** (Perl only strips leading empty fields for the special `split ' '` form, *not* for `split /\s+/`). Verified:
```
">  chr1 desc"  Perl split/\s+/  -> ""        (EMPTY name)
">  chr1 desc"  Rust split_whitespace -> "chr1"   <-- DIVERGES
">chr1\tdesc"   both -> "chr1"   (agree)
">chr1\r"       both -> "chr1"   (agree; \r is whitespace, trailing)
```

So the faithful Rust form is `header[1..].split(|c: char| c.is_whitespace()).next()` (which keeps a leading empty field), **not** `split_whitespace()`. The plan must (a) specify the exact split semantics, and (b) add a `>  chr1` → empty-name test. As written, "first whitespace-delimited token" is the ambiguous phrasing that leads straight to the alanhoyle behavior — i.e. the plan is one careless implementation away from a silent divergence on any header with a leading space/tab.

*Note on realism:* leading-space-after-`>` headers are rare but not unheard-of (some assemblers/exports emit `> chr1 ...`). Even if you judge it out of scope, the plan should say so explicitly rather than mis-state the contract.

### 1.3 IMPORTANT — `extract_chromosome_name` operates on the CHOMPED first line, but `read_until` keeps the terminator

PLAN A5 says headers are detected and the name extracted, while sequence lines are read via `read_until(b'\n')` (terminator retained). For **header** lines the name must be extracted from a buffer that still contains `\n`/`\r`. Perl handles this by `chomp`-ing the first line (line 403) before extraction, and for in-file headers (line 434 `if ($_ =~ /^>/`) it passes the **un-chomped** `$_` (still carrying `\n`/`\r`) into `extract_chromosome_name`, which relies on `split /\s+/` to drop the trailing whitespace. The plan should state explicitly that, for the Rust port, header-name extraction must strip the trailing terminator (or rely on a whitespace split that discards it) — and that the converted header is **always re-emitted with a single `\n`** regardless of the input terminator (CRLF header → LF output header). PLAN row 7 / SPEC §2.5 assert LF headers, which is right; just make sure A5's `read_until`-based reader doesn't accidentally let a stray `\r` leak into the rewritten header (it won't if the name is taken as the first whitespace-split field, but the plan never says which field-splitter is used — see 1.2).

### 1.4 IMPORTANT — first-line-of-file may itself be a sequence line edge

Perl unconditionally treats the **first line** of each file as a header (line 402, no `^>` check before `extract_chromosome_name`), and `extract_chromosome_name` dies only if it doesn't start with `>`. The plan's A5 says "first line → header (extract name…)". Good. But there is a subtle ordering question the plan should pin: Perl reads the first line with `<IN>` then enters `while (<IN>)` for the rest. An **empty file** (zero lines) → `$first_line = undef` → `chomp undef` → `extract_chromosome_name(undef)` → `s/^>//` fails → **die** ("doesn't seem to be in FASTA format"). The plan/SPEC don't mention the zero-byte-FASTA-file case (distinct from empty *dir*). Add a test: a present-but-empty `.fa` file → error. (Discovery in A3 will *find* the empty file because the glob matches by name, then convert.rs hits the empty-first-line die.)

### 1.5 IMPORTANT — indexer re-glob is `*.fa` only; confirm single_fasta filenames all end `.fa`

PLAN A7: *"re-glob `*.fa` in the dir"* — correct, Perl globs `<*.fa>` (lines 266/291) **not** `.fasta`. The alanhoyle reference globs `.fa` **or** `.fasta`, which is a (harmless, secondary) divergence — the plan correctly says `*.fa`. Confirm in implementation that single_fasta outputs are named `<chr>.CT_conversion.fa` (they are, per Perl lines 419/423), so `*.fa` catches them. This is a secondary (non-byte-gated) concern but worth a one-line assertion in the A7 test so nobody "helpfully" adds `.fasta` matching.

### 1.6 OK — slam header suffix, CRLF/final-newline, raw-byte transform

- **Slam suffix (PLAN C1 / SPEC §8.13):** Verified against Perl lines 427–429 and 454–455: the header print is literally `">",$chromosome_name,"_CT_converted\n"` / `_GA_converted` and is **never** branched on `$slam` (the `### TODO: Change this for GrandSlam` comment was never acted on). The alanhoyle port at lines 251–252 *does* emit `_TC_converted`/`_AG_converted` — confirmed divergence. The plan's instruction to pin this with a slam-mode header test is correct and load-bearing.
- **Raw-byte transform (PLAN A5 / SPEC §5.2):** The per-byte `to_ascii_uppercase → keep {A,T,C,G,N,\r,\n} else N → C→T/G→A` on bytes-including-terminator faithfully reproduces Perl's `uc → s/[^ATCGN\n\r]/N/g → tr`. I verified high-byte behavior (0xE9): Perl `uc` leaves it 0xE9, then `s/…/N/` → `N`; Rust `to_ascii_uppercase` leaves 0xE9, then keep-set → `N`. **They agree** (both map any non-ASCII/ambiguous byte to N regardless of case-fold), so there is no non-ASCII divergence. Good — this is the right approach and the alanhoyle `trim_end_matches` + `writeln!` pattern is correctly called out as divergence #1.
- **Final-no-newline / CRLF in seq lines:** Correctly preserved by never re-terminating. The A5 unit-test list covers both.

---

## 2. Assumptions

### 2.1 IMPORTANT — "Perl glob lexical sort == `Vec<PathBuf>::sort()`" is *probably* true but under-verified for the gate

PLAN A3 / SPEC §8.1 claim Perl `<*.fa>` lexical (ASCII) order equals Rust `Vec<PathBuf>::sort()`. Two caveats the plan under-states:

1. **`PathBuf` sort vs filename sort.** `Vec<PathBuf>::sort()` orders by the **whole path** (component-wise `OsStr` comparison). If `find_fasta_files` returns absolute paths (and A2 absolutizes the genome folder), the common prefix is identical so the tail (filename) decides — fine. But if some entries are absolute and some relative, or there's a mix, ordering could differ. The plan should mandate sorting by **`file_name()` bytes** (matching Perl's glob which returns bare filenames in the cwd), not by full `PathBuf`. This is the safer, provably-Perl-equivalent choice and removes the "are these all same-prefix paths?" worry.
2. **Locale.** Perl `File::Glob` default sort is **ASCII/byte** (not locale-collated) unless `:locale` is imported (it isn't here). Rust `OsStr`/`str` `Ord` is **byte-lexical**. So for ASCII filenames they match. The plan says "expected to match for ASCII filenames" — correct, but should explicitly note the **non-ASCII filename** case is undefined/untested and out of scope (genome chromosome files are ASCII-named in practice).

The load-bearing test (`chr1, chr10, chr11, chr2` lexical-not-numeric) is good and necessary. **Add** a `.gz`-sibling case to the *same* group (e.g. a dir of only `.fa.gz` with `chr1.fa.gz, chr10.fa.gz, chr2.fa.gz`) to confirm the precedence-group's internal sort, and pin sorting-by-filename in the unit test.

### 2.2 IMPORTANT — extension precedence uses byte-suffix matching, but Perl glob `*.fa` excludes `*.fa.gz` *by glob semantics*; the alanhoyle approach (`ends_with(".fa") && !ends_with(".fa.gz")`) is correct — confirm the plan adopts it

PLAN A3: *"try `.fa` (excluding `.fa.gz`)…"*. This matches Perl: `<*.fa>` does NOT match `foo.fa.gz` (the glob `*.fa` requires the name to *end* in `.fa`; `foo.fa.gz` ends in `.gz`). The explicit-exclusion phrasing is right. One under-specified corner: a file literally named `.fa` (dotfile, empty stem) — Perl `<*.fa>` does **not** match leading-dot files by default (`GLOB_NOSORT`/dotglob off), whereas `read_dir + ends_with(".fa")` **would** match `.fa`. Vanishingly rare, but if you want strict parity, exclude names that are exactly the extension or start with `.`. Optional.

### 2.3 IMPORTANT — `--path_to_aligner` validation semantics differ; plan should pin failure-equivalence, not behavior-equivalence

SPEC §4.7 + PLAN A7: Perl `chdir`s into `--path_to_aligner` to validate (lines 589–604) and prefixes the binary. The plan validates "the directory exists and resolves the binary within it." Two notes:
- Perl validates the **directory** is `chdir`-able **before** globbing FASTA (Step I, line 589), and dies there if not. The Rust port should perform this check at the same logical point (config/Step I), not lazily at indexer launch (Step III), so an invalid `--path_to_aligner` fails *before* the (potentially long) conversion runs — matching Perl's early-exit and avoiding wasted work. PLAN A2 validation does *not* list `--path_to_aligner` dir existence; A7 (Step III) does the discovery. **Move the dir-exists check earlier** (or at least call out that the FASTA conversion happens regardless and the failure surfaces only at Step III — a behavioral divergence from Perl's ordering, harmless to the gate but worth documenting under §4).
- Perl prefixes the binary name to the path and runs it **without** a PATH/`which` lookup (it just execs `<path>/bowtie2-build`). The plan's "validate via `which::which` … or direct path existence" is fine, but when `--path_to_aligner` is given, do **not** fall back to `which` (Perl wouldn't) — use the explicit path and error if absent. Pin this in the A7 test.

### 2.4 OK — concurrency, gzip MultiGzDecoder, version constant

Concurrent CT/GA (thread + main) mirrors Perl `fork`; affects wall-time only, not the gate. `MultiGzDecoder` for multi-member `.gz` is correct (a plain `GzDecoder` truncates at the first member — real risk for bgzip'd/concatenated genome `.gz`). Hardcoded `v0.25.1`/`19 May 2022` banner in diagnostics only (never in FASTA bytes) is correct; SPEC §6.2 routes the binary `--version` through `CARGO_PKG_VERSION` while the **Step I banner** keeps the literal `v0.25.1` constant — the plan's A6 says exactly that. Good, but note the two version strings now diverge (banner = Bismark v0.25.1 literal; `--version` = crate semver) — intentional and documented, just make sure A6/C3 don't accidentally unify them.

---

## 3. Efficiency analysis

### 3.1 OK — streaming, never slurp (conversion path)

The `read_until(b'\n')` line-streaming with two `BufWriter`s is O(genome size) time, O(line) memory — correct for ~3 GB human / axolotl-scale references. The plan's residual-risk note ("never slurp the conversion path") is the right guardrail. The only slurp path in Perl is `--genomic_composition` (`read_genome_into_memory`, whole genome in a hash), which the plan **defers** — so the memory hazard is avoided in v1.0. Good.

### 3.2 OPTIONAL — combined-genome FASTA: prefer stream-concatenation over re-running conversion

PLAN D1 offers two impls: *"stream-concatenate the produced CT then GA content (or re-run `convert_all` into the combined writer)"*. Re-running `convert_all` re-reads + re-transforms the entire genome a second time (doubling conversion CPU + re-globbing input). Stream-concatenating the **already-written** `genome_mfa.CT_conversion.fa` then `genome_mfa.GA_conversion.fa` is O(output) I/O only and is **exactly** the §10.4 structural-equality target by construction (combined == CT bytes ++ GA bytes). Recommend the plan *pick* stream-concatenation as the primary impl (the re-run option is strictly worse and risks a divergence if the re-run path differs subtly from the writers). Note: stream-concat only works cleanly in **MFA mode** where the two MFA files exist; since §10.1/§6.4 mandate the combined output is **always a single MFA independent of `--single_fasta`**, in single_fasta mode there is no `genome_mfa.*` to concat from — so the combined builder must either (a) also emit the MFA pair internally, or (b) concatenate the per-chr files in glob order. **PLAN D1 does not resolve this** — see 4.2.

### 3.3 OPTIONAL — `--parallel` total-core accounting

Cosmetic: Perl warns "uses `parallel*2` cores in total" and passes `--threads N` to *each* of the two concurrent builds. With `--combined_genome` there's a **third** build; if it runs concurrently with the other two (PLAN D2 "additional job after/with the standard two") the peak is `parallel*3` cores, not `*2`. Not a correctness issue, but the diagnostic text (if reproduced) would be misleading. Prefer running the combined build **after** the split pair (sequential third job) to keep peak resource use bounded and the messaging honest. Optional.

---

## 4. Validation sufficiency

### 4.1 IMPORTANT — the byte-identity gate tests need fixtures for the exact traps, and they should diff against ACTUAL Perl, not only a hand-authored fixture

PLAN A9 diffs against an "expected fixture (and, where `perl` is available, vs the actual Perl script — auto-skip if absent)." The hand-authored fixture is the weak link: if the author mis-derives the expected bytes for the very edge cases under test (CRLF, final-no-newline, bare `>`, leading-whitespace name), the test passes against a wrong expectation. **Mitigation:** make the **Perl oracle the primary assertion** in CI where `perl` is present (it is a near-universal dependency and the repo is a Perl codebase), and treat the static fixture as a fallback only. Better: generate the fixtures *from* a one-time Perl run and commit them with a comment recording the Perl version. Without this, items 1.1/1.2 above would not have been caught by the plan's own tests.

### 4.2 IMPORTANT — combined-genome in `--single_fasta` mode is untested / under-specified

The §10.4 structural check (`combined == CT_MFA ++ GA_MFA`) presumes MFA files exist. PLAN D1's test says "incl. a `--single_fasta` run (combined still one file)". But in single_fasta mode **there is no `genome_mfa.CT_conversion.fa`** to compare against — so what exactly is the combined output's expected bytes, and what is it concatenated from? The plan must define: in single_fasta mode the combined FASTA = per-chr CT files (glob/header order) ++ per-chr GA files. Add an explicit test that pins those bytes (e.g. == the bytes you *would* have produced in MFA mode for CT, then GA). As written, the D1 test for the single_fasta combined case has no well-defined oracle. **This is the biggest gap in Phase D.**

### 4.3 IMPORTANT — no test for "duplicate name spanning *gzipped* + plain across the precedence boundary"

Duplicate detection spans all files in the *winning* extension group (Perl only globs one group). The plan's dup test (A3/C2) should clarify that duplicates are only ever detected **within a single extension group** (you can't have a `.fa` and a `.fa.gz` both selected — precedence picks one group). A test asserting that a `chr1.fa` + `chr1.fasta` in the same dir does **not** even reach dup-detection (because `.fa` wins and `.fasta` is never globbed) would pin the precedence×uniqueness interaction. Minor but it closes a reasoning gap.

### 4.4 IMPORTANT — empty / zero-sequence chromosome and empty-sequence-line passthrough

- **Zero-byte `.fa` file** (1.4): add a test (present-but-empty file → error, matching Perl's first-line `undef` die path). Currently uncovered.
- **Empty sequence line** (`\n` only): Perl `uc("\n")` → `"\n"`, `s/[^…\n\r]/N/` → `"\n"`, `tr/C/T/` → `"\n"`; emitted verbatim. PLAN A5 lists "empty line `\n` passthrough" — good.
- **Chromosome with header but zero sequence lines** (record is just `>chr1\n` then EOF or next header): Perl writes only the converted header, no sequence. The plan should have a fixture for a zero-sequence record (two headers back-to-back) to confirm no spurious bytes. Not explicitly listed.

### 4.5 OK — minimap2 exclusions, aligner mutual-exclusion, parallel<2

A2/B3 cover the validation matrix (3 aligner conflicts, mm2×{single_fasta,slam,large-index}, parallel<2, missing folder, no-FASTA). These mirror Perl lines 124–179 and 110. Adequate. One nit: Perl's aligner precedence is order-sensitive (`--hisat2` checked first, then `--minimap2`, else bowtie2 default) and emits *specific* die messages per pair; the plan's "count > 1 → error" collapses these into one generic error. Since error *text* isn't gated (§7), this is acceptable — but the A2 test should assert each pair errors, not just "some conflict errors" (the plan already says "each conflict → error" — good).

### 4.6 IMPORTANT — real-data gate (E1/E2) doesn't pin that BOTH tools see an identical input copy and that the diff covers the file *set* in single_fasta

E1 diffs CT/GA MFA byte-for-byte on copies — good. For the single_fasta arm, the gate per SPEC §7.3 is "every `<chr>` file byte-identical **AND the set of files matches**." E1 says "Cover MFA + `--single_fasta`" but doesn't explicitly assert the **file-set equality** (a missing/extra per-chr file would slip a CT/GA-MFA-only diff). Add: enumerate both `CT_conversion/` dirs and assert identical filename sets before diffing contents.

---

## 5. Alternatives

1. **Single binary `main.rs` vs the 10-module layout.** The alanhoyle port is one ~490-line `main.rs` and is *readable*. The plan's 10-module split (cli/error/logging/discovery/convert/folders/indexer/combined/pipeline/lib) mirrors dedup/methcons and is the right call for testability (the pure `transform_seq_line` and `find_fasta_files` are the units that carry the gate) — but it is heavier than this trivial algorithm strictly needs. Acceptable: consistency with the workspace wins, and the pure-function isolation is what makes the byte traps unit-testable. No change required; just flagging that `logging.rs` + `folders.rs` could fold into `pipeline.rs` if the module count feels like ceremony.

2. **Header-name extraction: byte-slice vs `String`.** Doing name extraction on `&[u8]` (find first `>` byte, split on ASCII-whitespace bytes) avoids a UTF-8 validation step on header lines that may contain arbitrary bytes. Headers are normally ASCII, but a non-UTF-8 header byte would make `String`-based extraction (`read_line`/`from_utf8`) **error**, whereas Perl is byte-oriented and would not. Minor robustness edge; recommend the plan specify byte-level header handling for parity (and to avoid a panic/err on exotic input). Optional but cheap.

3. **Combined genome via concat at the OS level.** §3.2 above — prefer streaming concat of the produced MFAs over re-conversion; resolve the single_fasta source (4.2).

---

## 6. Action items (prioritized)

### Critical (byte-identity gate; fix before/at implementation)
- **C1 — Bare `>` must NOT error.** Correct PLAN A3 test + SPEC §8.9: a bare `>` (or `>`+whitespace) yields an **empty chromosome name** used verbatim (`>_CT_converted\n`; single_fasta `.CT_conversion.fa`). Only a first line **not** starting with `>` dies. Add a pinning fixture. (Perl 575–581; verified.)
- **C2 — Leading-whitespace-after-`>` yields an EMPTY name, not the next token.** Replace the ambiguous "first whitespace-delimited token" with the exact Perl `split /\s+/` semantics: `header[1..].split(char::is_whitespace).next()` (keeps the leading empty field), **not** `split_whitespace()`. Add a `>  chr1 desc` → `""` test. This is precisely where the alanhoyle reference (`split_whitespace`) would mislead the implementer. (Perl 576; verified.)

### Important
- **I1 — Sort by `file_name()` bytes, not `PathBuf`.** Pin the glob sort to filename-byte order (Perl-equivalent), note ASCII-only/locale assumptions, and add a `.gz`-sibling sort test alongside the `chr1/chr10/chr2` case. (§8.1, A3.)
- **I2 — `--path_to_aligner` validate the dir early (Step I), and don't `which`-fallback when it's given.** Document the ordering relative to Perl; pin in A7 test. (Perl 589–604.)
- **I3 — Combined-genome in `--single_fasta` mode is under-specified.** Define and test the combined FASTA's bytes/source when no MFA files exist; prefer stream-concat of the MFA pair (emit it internally if needed). (PLAN D1, §10; biggest Phase-D gap.)
- **I4 — Zero-byte FASTA file → error.** Add a fixture (distinct from empty-dir). (Perl 402–406.)
- **I5 — Make the Perl oracle the primary A9/E1 assertion** where `perl` is present; derive committed fixtures from a recorded Perl run. Without this, C1/C2-class mistakes pass the plan's own tests. (A9.)
- **I6 — single_fasta gate must assert file-SET equality**, not just per-file content. (E1, §7.3.)
- **I7 — Header-name extraction must strip the terminator / use whitespace split** so a CRLF/`\r` never leaks into the re-emitted LF header; state the splitter explicitly. (A5; ties to C2.)
- **I8 — Zero-sequence record** (back-to-back headers) fixture: only headers emitted, no stray bytes. (A5.)

### Optional
- **O1 — Combined build sequential (third job after the split pair)** to bound peak cores and keep the `parallel*2` messaging honest. (D2.)
- **O2 — Indexer re-glob: assert `*.fa`-only** in the A7 test (don't add `.fasta` like the reference does). (Perl 266/291.)
- **O3 — Dotfile `.fa` / leading-dot exclusion** for strict glob parity. (A3.)
- **O4 — Byte-level header handling** to avoid UTF-8 errors on exotic header bytes. (A5.)
- **O5 — Precedence×uniqueness test**: `chr1.fa` + `chr1.fasta` in one dir → no dup error (`.fa` group wins). (A3.)

---

## 7. Summary

The plan's phasing (A = full byte-identity gate for the common case; B = modes/indexers; C = slam/edge/accept-ignore; D = additive combined; E = real-data + docs), its "sequential not parallel streams" call (shared `convert.rs`/`indexer.rs`), the no-`bismark-io` reuse map, and the headline byte traps (CRLF, final-no-newline, slam suffix, raw-byte transform) are all **correct and well-reasoned**. I confirmed the three alanhoyle divergences exist and that the plan flags them.

The two **Critical** findings (C1 bare-`>`, C2 leading-whitespace name) are subtle Perl `split /\s+/` / `s/^>//` semantics that the spec/plan currently *mis-state*, both sitting on the acceptance gate and both reproduced experimentally here. They are easy to fix in the plan text + tests, but if implemented as written they would silently diverge on those (rare) inputs — and the plan's own tests would not catch them, which is why **I5 (Perl-as-oracle)** is the most leverage-y Important item.

**Counts: Critical = 2, Important = 8, Optional = 5.**

Report written to `/Users/fkrueger/Github/Bismark-genomeprep/plans/05302026_bismark-genome-preparation/PLAN_REVIEW_B.md`.
