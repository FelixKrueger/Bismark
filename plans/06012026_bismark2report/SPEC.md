# SPEC — `bismark-report` (Rust port of Perl `bismark2report`)

**Status:** DRAFT rev 1 (2026-06-01) — **dual plan-review findings folded in** (`SPEC_REVIEW_A.md` / `SPEC_REVIEW_B.md`). Awaiting implementation trigger. Do **not** implement.
**Date:** 2026-06-01
**Rev 1 changes (dual plan-review folded in):** (a) **M-bias placeholder survival** made first-class — the `{{mbias1_*}}`/`{{mbias2_*}}` *data* placeholders sit in the trailing `<script>` blocks **outside** the deletable `{{mbias_r*_section}}` spans, so they survive as literal `{{…}}` whenever unfilled: **all 24 when M-bias is absent, the 12 `{{mbias2_*}}` for every SE sample** (§2.7d, §5.4, §8.4). (b) Fill gates restated as **`defined` / `Option::is_some()`** (NOT truthiness) — `0` is defined-but-falsy and common (`no_genomic`, `dups`) (§2.7). (c) Crate-name footnote **corrected** — all seven existing crates are hyphenated `bismark-<tool>`; `bismark-report` is already convention-correct, no rename (§3). (d) **Exit codes pinned**: `--help`/`--man`/`--version` → **0** (clap default; Perl's `exit 1`-on-help quirk intentionally **not** reproduced); error paths → nonzero (§6.1). (e) **Timestamp hook decided**: hidden `--__test_timestamp <UNIX_EPOCH>` formatted in **UTC** for byte-stable goldens; default = local time as Perl; gate **normalizes the one timestamp line** anchored exactly + asserts a single match (§7). (f) Normalizer **empty-input guard** added (§8.2). (g) New required fixtures: gate-failure, `0`-through-gate, dedup leftover-fallback, amplicon missing-nuc-key (#711), multi-report companion reset, M-bias-absent + M-bias-SE goldens (§7).
**Branch / worktree:** `rust/bismark2report` @ `~/Github/Bismark-report` (off `rust/iron-chancellor` @ `7dbcee3`)
**Perl source of truth:** `bismark2report` (repo root, 1316 lines, `$bismark2report_version = 'v0.25.1'`, help banner "last modified: 13 Sep 2021")
**Assets:** `plotly/plotly_template.tpl` (29 KB), `plotly/plot.ly` (≈3.0 MB, plotly.js v1.48.3), `plotly/bismark.logo` (28 KB), `plotly/bioinf.logo` (17 KB)
**Acceptance gate:** the **generated HTML report, byte-for-byte identical to the Perl original** (current `v0.25.1`), modulo the single `localtime` timestamp line — defined precisely in §7.
**Scope note:** `bismark2summary` is a **different** tool (multi-sample roll-up). It is **out of scope** for this port.

---

## 1. Purpose & one-paragraph summary

`bismark2report` reads a Bismark **alignment report** (mandatory) plus up to four optional companion reports — **deduplication**, methylation-extractor **splitting**, **M-bias**, and **nucleotide-coverage** — and fills a single self-contained HTML template (`plotly_template.tpl`) to produce one graphical per-sample report. It auto-detects the companion reports by file basename, or takes them explicitly via flags. The Plotly JS library and two logos are inlined into the HTML so the report is a single standalone file. The Rust port must reproduce that HTML **byte-for-byte** against the current Perl `v0.25.1`.

This is the **same shape as the genome-preparation port**: no BAM/SAM/CRAM, so it does **not** depend on `bismark-io` or noodles. Mechanically it is the *simplest* port so far — a parser + a string-substitution templating engine — but the byte-identity bar is high because the output is a 3 MB HTML file assembled from embedded assets and verbatim-injected values. **There is essentially no numeric reformatting** (see §2.7): values pass through exactly as captured from the input reports. The only non-determinism is the `localtime` timestamp (§2.6, §7).

---

## 2. Perl behavior — the contract (derived from source)

### 2.1 Top-level flow (lines 26–158)
1. `process_commandline()` parses options and builds **five parallel arrays** — `@alignment_reports`, `@dedup_reports`, `@splitting_reports`, `@mbias_reports`, `@nuc_reports` — one slot per alignment report (companion slots hold a filename or the empty string `''` = "absent").
2. `while (@alignment_reports)`: `shift` one slot from each of the five arrays and build **one HTML report** per alignment report. (So N alignment reports in a directory → N HTML files; each is independent.)
3. For each iteration: derive the output filename (§2.5), read the template + assets into `$doc`, inject assets, stamp the timestamp, then run the 1 mandatory + 4 optional parsers (each editing `$doc`), then `write_out_report`.

### 2.2 CLI options (`GetOptions`, lines 1055–1065; help text lines 1263–1315)

| Perl option | Type | Behavior |
|---|---|---|
| `--alignment_report FILE` | string | The mandatory data source. If **omitted**, auto-detect via glob `<*E_report.txt>` in the **current directory** (matches `_PE_report.txt`/`_SE_report.txt`); 0 matches → print help + exit. |
| `--dedup_report FILE` | string | Optional. `none` (case-insensitive) → skip. If omitted → auto-detect `<basename*deduplication_report.txt>`. |
| `--splitting_report FILE` | string | Optional. `none` → skip. Auto-detect `<basename*splitting_report.txt>`. |
| `--mbias_report FILE` | string | Optional. `none` → skip. Auto-detect `<basename*M-bias.txt>`. |
| `--nucleotide_report FILE` | string | Optional. `none` → skip. Auto-detect `<basename*nucleotide_stats.txt>`. |
| `--dir DIR` | string | Output directory; trailing `/` appended unless empty. Default `''` (current dir). |
| `-o` / `--output FILE` | string | Manual output filename (used **verbatim**, no `.html` auto-append). **Only legal with a single alignment report** (else `die`). |
| `--verbose` | flag | Extra STDOUT/STDERR diagnostics + `sleep` pauses. |
| `--version` | flag | Print version banner, exit. |
| `--help` / `--man` | flag | Print help, exit. |

**Auto-detection rules (per alignment report, lines 1134–1257):**
- `basename` = `$aln =~ /^(.+)_(P|S)E_report.txt$/` → `$1`. (A glob match that fails this regex leaves `$basename` undefined — §8.)
- For each companion, when **not** explicitly specified: glob `<basename*…>`. **> 1 match → `die`** ("Found N potential … reports with the same basename"). **0 matches → `''`** (treated as absent). Exactly 1 → use it.
- Explicit companion flags are reused across **all** alignment reports unless `none`/auto resets them; note the explicit value is consumed once then `undef`-reset at line 1256 (`$dedup_report = $splitting_report = $mbias_report = $nucleotide_coverage_report = undef;`) so a second alignment report falls back to auto-detection. **(Mirror this exactly — see §8.)**

### 2.3 The template assembly order (lines 59–156) — load-bearing
The order of mutations on `$doc` is fixed and **must be replicated**:
1. Read `plotly_template.tpl` → `$doc` (with the §2.6 line normalization).
2. **Inject `plot.ly`**: `s/\{\{plotly_goes_here\}\}.*\{\{plotly_goes_here\}\}/$plotly_code/s` — greedy, dotall; replaces the two markers **and everything between them** (template lines 127–129) with the asset. `die` if the markers aren't found.
3. **Inject `bismark.logo`**: same pattern on `{{bismark_logo_goes_here}}` (lines 138–140).
4. **Inject `bioinf.logo`**: same on `{{bioinf_logo_goes_here}}` (lines 1145–1147).
5. **Timestamp**: `getLoggingTime` fills `{{date}}` and `{{time}}` (§2.6).
6. **Alignment report** (mandatory): `read_alignment_report` (§2.7a).
7. **Deduplication** (optional): present → `s/\{\{deduplication_section\}\}//g` then fill; absent → `s/\{\{deduplication_section\}\}.*\{\{deduplication_section\}\}//s` (delete section incl. content).
8. **Splitting** (optional): same present/absent pattern on `{{cytosine_methylation_post_deduplication_section}}`.
9. **M-bias** (optional): present → delete `{{mbias_r1_section}}` markers + fill; then if SE state → **delete the entire `{{mbias_r2_section}}` block (incl. content)**, if PE → delete only the `{{mbias_r2_section}}` markers. Absent → delete both `mbias_r1` and `mbias_r2` blocks (§2.7d).
10. **Nucleotide coverage** (optional): present → delete `{{nucleotide_coverage_section}}` markers + fill; absent → delete the block.
11. `write_out_report`: `print OUT $doc` — no trailing manipulation.

### 2.4 Section present/absent mechanics (the `{{…_section}}` markers)
Each optional section is wrapped by **exactly two identical markers** in the template:
- **Present** → `s/\{\{marker\}\}//g` deletes *both* markers (leaves content). Rust: `replace(marker, "")` (all occurrences).
- **Absent** → `s/\{\{marker\}\}.*\{\{marker\}\}//s` (greedy + dotall) deletes from the **first** marker to the **last** marker inclusive. Rust: find first index of marker and last index of (marker + its length), splice out the span. Because each marker name occurs exactly twice, "first…last" is unambiguous. **(See §8 — replicate greedy/dotall semantics, not a non-greedy match.)**

### 2.5 Output filename derivation (lines 42–56)
```
$report_output = $alignment_report;
$report_output =~ s/^.*\///;     # strip directory
$report_output =~ s/\.txt$//;    # strip .txt
$report_output =~ s/$/.html/;    # append .html
# if -o given (single report only): use $manual_output_file verbatim instead
$report_output = $output_dir . $report_output;   # prefix output dir
```
e.g. `path/to/foo_PE_report.txt` → `foo_PE_report.html`; with `--dir out/` → `out/foo_PE_report.html`.

### 2.6 Timestamp & line normalization — the ONLY non-determinism
- `getLoggingTime` (lines 166–178): `($sec,$min,$hour,$mday,$mon,$year,...) = localtime(time)`; `{{time}}` ← `sprintf("%02d:%02d:%02d", $hour,$min,$sec)`; `{{date}}` ← `sprintf("%04d-%02d-%02d", $year+1900,$mon+1,$mday)`. These appear **only** in template line 146: `<p>Data processed at {{time}} on {{date}}</p>`.
- **Asset/template line normalization** (`read_report_template`, lines 1025–1038): each asset is opened from `$RealBin/plotly/$template`, read line-by-line; per line `chomp` then `s/\r//g` (strip **all** `\r`), then `$doc .= $_ . "\n"`. Consequences: the in-memory document is fully **LF-normalized**, every line is `\n`-terminated, and `$doc` **always ends in `\n`** regardless of whether the source file did. Mid-line `\r` (not just CRLF terminators) are removed. **This must be replayed exactly on the embedded assets** (§3, §8).

### 2.7 The five parsers (each mutates `$doc`)

#### (a) `read_alignment_report` (mandatory; lines 181–507)
Captures (tab-split, `(undef,$val) = split /\t/`):
- **General stats** (PE vs SE detected by line text, which also sets the human label `*_text`):
  - total: `^Sequence pairs analysed in total:` (PE) / `^Sequences analysed in total:` (SE) → `{{total_sequences_alignments}}` + label `{{sequences_analysed_in_total}}`.
  - unique: `^Number of paired-end alignments with a unique best hit:` (PE) / `^Number of alignments with a unique best hit from` (SE) → `{{unique_seqs}}` + `{{unique_seqs_text}}`.
  - no-aln: `^Sequence pairs with no alignments under any condition:` (PE) / `^Sequences with no alignments under any condition:` (SE) → `{{no_alignments}}` + `{{no_alignments_text}}`.
  - multiple: `^Sequence pairs did not map uniquely:` (PE) / `^Sequences did not map uniquely:` (SE) → `{{multiple_alignments}}` + `{{multiple_alignments_text}}`.
  - no-genomic: `^Sequence pairs which were discarded because genomic sequence could not be extracted:` (PE) / `^Sequences which were discarded…` (SE) → `{{no_genomic}}`.
  - `^Bismark report for: (.*) \(version: (.*)\)` → `{{filename}}` (title ×2) + `{{bismark_version}}` (footer). **Version is input-derived → deterministic.**
- **Context methylation:** `^Total number of C` → `{{total_C_count}}`; `Total methylated C's in {CpG,CHG,CHH,Unknown} context:` → `{{meth_*}}`; unmethylated (also matches the alternate phrasing `Total C to T conversions in … context:`) → `{{unmeth_*}}`; `C methylated in {CpG,CHG,CHH} context:` and `C methylated in Unknown context (CN or CHN):` → `{{perc_*}}` (with the `%` stripped).
- **Strand origin** (PE vs SE patterns): OT=`^CT/GA/CT:`(PE)/`^CT/CT:`(SE); CTOT=`^GA/CT/CT:`/`^GA/CT:`; CTOB=`^GA/CT/GA:`/`^GA/GA:`; OB=`^CT/GA/GA:`/`^CT/GA:` → `{{number_OT/CTOT/CTOB/OB}}`.
- **Plotly data strings** (verbatim join): `{{alignment_stats_plotly}}` = `$unique,$no_aln,$multiple,$no_genomic`; `{{strand_alignment_plotly}}` = `$number_OT,$number_CTOT,$number_CTOB,$number_OB`; `{{cytosine_methylation_plotly}}` = `$perc_CpG_graph,$perc_CHG_graph,$perc_CHH_graph` (N/A → `0`).
- **Unknown-context rows:** if `$meth_unknown` is defined, inject 3 multi-line `<tr>` snippets into `{{meth_unknown}}`/`{{unmeth_unknown}}`/`{{perc_unknown}}`; else inject `''`. **These snippets contain literal tab+space indentation that must be reproduced byte-for-byte** (§8; exact bytes: header `<tr>` line = 5 spaces; `<th>` line = 32 spaces; `<td>` line = 4 spaces + 4 tabs; `</tr>` line = 4 spaces + 3 tabs).
- **Percent N/A handling:** undefined `perc_*` → table shows `N/A`; the **graph** value uses `0` (so Plotly renders nothing rather than erroring).
- **ALL-OR-NOTHING FILL GATE (`defined`, NOT truthiness):** the entire fill block runs **only if** `defined $unique and defined $no_aln and defined $multiple and defined $no_genomic and defined $total_seqs` (Perl **line 378**). In Rust this is **`Option::is_some()` on each**, *not* a truthiness / `!= 0` test — a value of `0` (e.g. `no_genomic: 0`, very common) is *defined-but-falsy* and **must pass** the gate. If a field is genuinely absent → `warn "Am I missing something?"` and **the placeholders are left unfilled in the output** (literal `{{…}}` text survives). Reproduce both: the `is_some` gate AND the surviving-placeholder output on failure (do **not** partially fill or default).

#### (b) `read_deduplication_report` (optional; lines 510–568)
- `^Total number of alignments` → `$total_seqs`; `^Total number duplicated` → `$dups` (then `s/\s.*//` — keep only the leading number, drop the trailing ` (NN.NN%)`); `^Duplicated alignments were found at` → `$diff_pos` (`s/\s.*//`); `^Total count of deduplicated leftover sequences: (\d+)` → `$leftover`.
- **Leftover fallback:** if not captured but `$dups` and `$total_seqs` are → `$leftover = $total_seqs - $dups` (integer subtraction; emit the integer string).
- Fill only if **`defined` on each** of `$dups, $total_seqs, $diff_pos, $leftover` (Perl **line 551**; `is_some()` in Rust, not truthiness — `dups: 0` / `diff_pos: 0` are valid and must pass): `{{seqs_total_duplicates}}`, `{{unique_alignments_duplicates}}` (=leftover), `{{duplicate_alignments_duplicates}}` (=dups), `{{different_positions_duplicates}}`, and plot `{{duplication_stats_plotly}}` = `$leftover,$dups`. Otherwise warn and return `$doc` unchanged (placeholders survive — but note the markers were already deleted at step 7).

#### (c) `read_splitting_report` (optional; lines 705–885)
- Same context-methylation capture as alignment but writing the `*_splitting` placeholders: `{{total_C_count_splitting}}`, `{{meth_*_splitting}}`, `{{unmeth_*_splitting}}`, `{{perc_*_splitting}}`, `{{cytosine_methylation_post_duplication_plotly}}`, and the 3 `{{*_unknown_splitting}}` inject snippets (same byte layout as (a)).
- Note: splitting uses **only** `Total C to T conversions in … context:` for unmethylated (no `Total unmethylated C's` alternate), and `C methylated in Unknown context:` (no `(CN or CHN)` suffix).
- **FILL GATE (`defined`, Perl line 784):** runs only if **`defined` on each** of `$meth_CpG, $meth_CHG, $meth_CHH, $unmeth_CpG, $unmeth_CHG, $unmeth_CHH` — `Option::is_some()` in Rust, not truthiness (`0` counts are valid).

#### (d) `read_mbias_report` (optional; lines 888–1022) — returns `($state, $doc)` — **the trickiest section (rev 1)**
- Section headers `^(C.{2}) context` set `$context` (CpG/CHG/CHH); a header containing `R2` sets `$read_identity=2` and **`$state='paired'`** (default `'single'`). Data lines `^\d` (tab-split `$pos,$meth,$unmeth,$perc,$coverage`) accumulate per (read, context): `perc_x=pos`, `perc_y=perc`, `coverage_x=pos`, `coverage_y=coverage` (lines 920–931).
- Fills R1 placeholders `{{mbias1_<ctx>_{meth,coverage}_{x,y}}}` = comma-joined arrays (always, line 940+); R2 placeholders `{{mbias2_…}}` are filled **only `if (%mbias_2)`** (line 977 — i.e. R2 **data rows** were seen). **No fill gate** — when an array *is* processed but empty, it joins to the empty string.
- **THREE distinct facts that must all be reproduced (both reviewers):**
  1. **Section (`<div>`) deletion is driven by `$state`** (header-derived, step 9): SE (`single`) → delete the **whole R2 block** (`{{mbias_r2_section}}.*{{mbias_r2_section}}`); PE (`paired`) → delete only the R2 markers. Absent M-bias → delete **both** R1 and R2 blocks.
  2. **R2 fill is driven by `%mbias_2`** (data rows), which can **diverge** from `$state`: an R2 *header* with no data rows → `$state='paired'` (block kept) but `%mbias_2` empty (R2 placeholders unfilled). Edge case, but real.
  3. **The `{{mbias1_*}}` / `{{mbias2_*}}` DATA placeholders live in the trailing `<script>` blocks (template lines 836–1141), OUTSIDE the deletable `{{mbias_r*_section}}` spans (465–496).** So they are only ever filled when `read_mbias_report` runs and the read's data exists; otherwise they **survive as literal `{{…}}` in the output JS** — **all 24 when no M-bias report is given** (read_mbias_report never runs), and the **12 `{{mbias2_*}}` for every SE sample**. This is the common case, not an error path (§5.4, §7 fixtures).
- **Dead substitution:** `s/\{\{bm_mbias_2\}\}/false/g` (line 1016, in the no-R2 branch) targets a placeholder that **does not exist** in the current template → no-op. Reproduce as a no-op (or omit; document either way).

#### (e) `read_nucleotide_coverage_report` (optional; lines 571–702)
- Per line, `s/\r//` then tab-split `$element,$count_obs,$observed,$count_exp,$expected,$coverage`.
- **Header validation (line 0):** col 3 must equal `percent sample` and col 5 `percent genomic`, else `die` ("This doesn't look like a Bismark nucleotide coverage report").
- Iterates a **fixed, hardcoded key order** (NOT sorted): `A,T,C,G, AC,CA,TC,CT,CC,CG,GC,GG,AG,GA,TG,GT,TT,TA,AT,AA`. For each: fill `{{nuc_<K>_p_obs}}` (observed %), `{{nuc_<K>_p_exp}}` (expected %), `{{nuc_<K>_counts_obs}}`, `{{nuc_<K>_counts_exp}}`, `{{nuc_<K>_coverage}}` — all **verbatim** from the report.
- The observed/expected **log2 ratio is computed but its injection is commented out** → **no float output** (the only place a sprintf-formatted float could have appeared; confirm it stays unused).
- Plot arrays: `{{nucleo_sample_x}}` = `join(" , ", obs%)`, `{{nucleo_genomic_x}}` = `join(" , ", exp%)`, `{{nucleo_sample_y}}` = `{{nucleo_genomic_y}}` = `join("','", keys)` wrapped in single quotes (`'A','T',…`). **Note the distinct separators** (` , ` for x; `','` for y) — §8.
- **FILL GATE:** `looksOK` (every captured key has both obs & exp). On failure → warn + return unchanged.
- Missing keys (in the fixed list but absent from the report): percentages default to `0`; counts/coverage substitute the empty string (Perl undef-in-replacement) — §8 edge.

---

## 3. Reuse map — what comes from the existing workspace

`bismark-report` is **standalone** — no `bismark-io`, no noodles, no `flate2` (reports are plain text). Reuse is convention-level:

| Need | Reuse / source | Notes |
|---|---|---|
| CLI parsing, `--version`, exit codes | clap derive (pin to workspace version, e.g. `=4.5.x`), mirror `bismark-genome-preparation`/`bismark-dedup` `cli.rs` + `main.rs` | Keep Perl flag spellings (`--alignment_report`, `--dedup_report`, `--splitting_report`, `--mbias_report`, `--nucleotide_report`, `--dir`, `-o/--output`, `--verbose`, `--version`, `--help`/`--man`). |
| Embedded assets | `include_str!` the four `plotly/` files into the binary | Self-contained single binary (matches the rewrite ethos). Replay the §2.6 normalization on each — do **not** rely on Rust `.lines()` (only strips trailing `\r`). The 3 MB `plot.ly` inflates the binary by ~3 MB — acceptable; the alternative (read-from-`$RealBin`) reintroduces a runtime path dependency that the other ports removed. |
| Auto-detect globs | `glob = "=0.3.x"` crate, or `std::fs::read_dir` + suffix filter + sort | Patterns: `*E_report.txt`, `<basename>*deduplication_report.txt`, `*splitting_report.txt`, `*M-bias.txt`, `*nucleotide_stats.txt`. Reproduce Perl's lexical sort (low-stakes here — §8). |
| Errors / diagnostics | `anyhow` + `thiserror`; STDERR logger mirroring `bismark-extractor/src/logging.rs` | `--verbose` toggles detail; STDERR text is **not** byte-matched. `sleep` UX pauses dropped. |
| Report parsing | hand-written line parsers (prefix match + tab split) | No external parser; mirror the exact regexes/branches in §2.7. |
| Workspace wiring | add `bismark-report` to `rust/Cargo.toml` `members` | Current members: `bismark-io`, `bismark-dedup`, `bismark-extractor`, `bismark-methylation-consistency`, `bismark-bedgraph`, `bismark-coverage2cytosine`, `bismark-genome-preparation`. |

**Crate name:** `bismark-report` (per task). **Binary name:** `bismark2report_rs` (Perl-name + `_rs`, matching `deduplicate_bismark_rs` / `bismark_genome_preparation_rs`; drop-in for `bismark2report`). *(Convention check (rev 1, verified): **all seven existing crates are hyphenated `bismark-<tool>`** — `bismark-io`, `bismark-dedup`, `bismark-extractor`, `bismark-methylation-consistency`, `bismark-bedgraph`, `bismark-coverage2cytosine`, `bismark-genome-preparation` — none uses the full Perl name. So `bismark-report` is **already** convention-correct; **no rename needed**. The rev-0 footnote claiming otherwise was wrong.)*

---

## 4. Known divergences from Perl (documented & accepted — for reviewers to accept or challenge)

1. **Assets embedded via `include_str!`, not read from `$RealBin/plotly/`.** Output bytes are identical *provided* the §2.6 normalization is replayed. Removes the runtime asset-path dependency.
2. **STDOUT/STDERR diagnostics** mirror Perl's `warn`/`print` in spirit, not byte-for-byte; `--verbose` gates detail; `sleep(1)`/`sleep(3)` pauses dropped. Not gated.
3. **`--help` / `--man` / `--version` text** is clap/Rust-generated, not byte-identical to Perl's help block / banner (dedup/methcons/genomeprep precedent). Not gated. `--man` aliases `--help`.
4. **`Getopt::Long` behaviors not replicated:** `auto_abbrev` (unambiguous prefixes) and `:s` optional-value subtleties. Only the documented flags are accepted; clap enforces types.
5. **Timestamp determinism (the gate mechanism — §7):** the live Perl uses `localtime(time)` which cannot be overridden without patching Perl. The **acceptance gate normalizes the single timestamp line** in both outputs before byte-comparison. For Rust's own committed golden fixtures, a deterministic timestamp source is honored (proposal: `SOURCE_DATE_EPOCH` env var, or a hidden `--_test_timestamp`), so committed goldens are fully byte-stable. Default runtime behavior = local time formatted exactly as Perl (`%02d:%02d:%02d` / `%04d-%02d-%02d`).
6. **Hardcoded version string** `v0.25.1` appears only in the `--version` banner text (not gated). The HTML's `{{bismark_version}}` comes from the **input report**, not this constant.
7. **`{{bm_mbias_2}}` substitution** (Perl line 1016) is a no-op against the current template; reproduced as a no-op.
8. **Glob sort order** reproduced (lexical) but is **far lower-stakes** than in genomeprep: multiple alignment reports each produce an *independent* file (order affects only STDOUT), and multiple companion matches `die` rather than depending on order. (Still reproduce a deterministic sort for the alignment-report processing loop.)

---

## 5. Output contract — exact bytes

### 5.1 The HTML document `$doc`
A single `<filename>.html` (or `-o` name), prefixed by `--dir`. Its bytes are the template after, in order: plot.ly inject → bismark.logo inject → bioinf.logo inject → timestamp fill → alignment fill → optional dedup/splitting/mbias/nuc fills/deletions (§2.3). LF-normalized throughout; ends in `\n`.

### 5.2 Value injection rules (verbatim — NO reformatting)
- All captured values are injected exactly as read (after tab-split), **except**: percentages have a trailing `%` stripped; dedup `dups`/`diff_pos` drop everything from the first whitespace onward; dedup `leftover` fallback = integer `total - dups`.
- Plotly data strings are simple joins (separators per §2.7: `,` for most; ` , ` and `','` for the nucleotide arrays); N/A percentages become `0` **only in the graph string**, while the table cell shows `N/A`.
- The Unknown-context `<tr>` injection snippets are reproduced byte-for-byte (literal tabs/spaces — §2.7a, §8).

### 5.3 Section presence
- Each `{{…_section}}` is either collapsed (markers removed, content kept) or excised (markers + content removed) per §2.4. M-bias R2 has the extra SE-vs-PE rule (§2.7d).

### 5.4 Unfilled placeholders are a real output state (TWO sources — both contractual)
1. **Fill-gate failure** (§2.7a/c/e): if a gate's `defined` predicate fails, that section's `{{…}}` placeholders remain in the HTML.
2. **M-bias data placeholders outside the deletable spans (the COMMON case):** the `{{mbias1_*}}` / `{{mbias2_*}}` placeholders live in the trailing `<script>` blocks (template 836–1141), **not** inside `{{mbias_r*_section}}` (465–496). They are filled **only** when `read_mbias_report` runs and the read's data exists — otherwise they survive literally: **all 24** when no M-bias report is given, and the **12 `{{mbias2_*}}`** for **every SE sample** (R1-only). See §2.7d.

Reproduce both exactly; **do not** invent defaults or strip surviving placeholders. Both states need committed goldens (§7).

---

## 6. CLI surface (clap derive)

```
bismark2report_rs [OPTIONS]

    --alignment_report <FILE>    Bismark alignment report (mandatory data). If omitted, auto-detect
                                 *E_report.txt in the current directory (one HTML per match).
    --dedup_report <FILE>        Deduplication report; "none" to skip; auto-detect by basename if omitted.
    --splitting_report <FILE>    Methylation-extractor splitting report; "none" to skip; auto-detect.
    --mbias_report <FILE>        M-bias report; "none" to skip; auto-detect.
    --nucleotide_report <FILE>   Nucleotide-coverage report; "none" to skip; auto-detect.
    --dir <DIR>                  Output directory (default: current dir).
-o, --output <FILE>              Output filename (single alignment report only).
    --verbose                    Extra diagnostics.
    --__test_timestamp <EPOCH>   HIDDEN (test-only, clap hide=true): inject a fixed UNIX epoch, formatted
                                 in UTC, into {{date}}/{{time}} for byte-stable goldens. Default = local time.
-V, --version                    Print version and exit.
-h, --help / --man               Print help and exit.
```

### 6.1 Validation & exit codes (rev 1 — pinned)
- **`--help` / `--man` / `--version` → exit 0** (clap default). We **do NOT** reproduce Perl's quirk of exiting **1** on help (`print_helpfile`'s `exit 1`, line 1314). `--man` aliases `--help`.
- **Error paths → nonzero exit** (the value is not byte-gated — `1` via `anyhow`, or clap's `2` for arg errors): `-o`/`--output` with **> 1** alignment report ("cannot run on more than 1 file while specifying a single output file"); **no** `--alignment_report` **and** 0 `*E_report.txt` matches in cwd (emit the Perl hint message, but exit **nonzero** as an error — NOT 0); companion auto-detect with **> 1** basename match; nucleotide report failing the line-1 header check.
- **Mandatory-field-missing in the alignment report → NOT an error** (Perl warns and emits a partially-unfilled HTML — §2.7a/§5.4): exit 0, placeholders survive.

### 6.2 `--version`: `version_string()` from `lib.rs` via `env!("CARGO_PKG_VERSION")` (dedup precedent); the Bismark `v0.25.1` constant lives only in the banner text.

---

## 7. Acceptance / definition of "byte-identical output"

**HARD gate (byte-for-byte identical to Perl Bismark `v0.25.1`, modulo the timestamp line):**
1. The generated `<report>.html` equals the Perl-generated HTML **after normalizing the one timestamp line** `Data processed at HH:MM:SS on YYYY-MM-DD` (replace the `HH:MM:SS` and `YYYY-MM-DD` with fixed tokens in **both** files, then `cmp`). Every other byte — assets, injected values, section presence, whitespace — must match exactly.
2. Coverage across input shapes: **PE** and **SE** alignment reports; with/without each of the 4 optional reports; **Unknown-context present** (Bowtie2) vs absent; **M-bias SE (R1 only)** vs **PE (R1+R2)**; `--dir` and `-o`; multi-report auto-detection in a directory; `none` skips.

**Timestamp-determinism decision (rev 1 — DECIDED):**
- **Acceptance gate (Perl-vs-Rust):** *normalize the one timestamp line* in both outputs, then byte-compare the rest. **Anchor** the match to the exact template line `<p>Data processed at HH:MM:SS on YYYY-MM-DD</p>`, replace the `HH:MM:SS`/`YYYY-MM-DD` with fixed tokens, and **assert exactly one match in each file** (so a stray timestamp-shaped string elsewhere can't silently mask a real divergence). Perl's `localtime` can't be pinned without patching it, so normalization — not a shared fixed clock — is the bridge.
- **Rust committed golden fixtures:** the hidden **`--__test_timestamp <UNIX_EPOCH>`** flag (clap `hide=true`) injects a fixed epoch **formatted in UTC** (machine-TZ-independent → CI-stable) into `{{date}}`/`{{time}}` using Perl's exact `%04d-%02d-%02d` / `%02d:%02d:%02d`. Chosen over `SOURCE_DATE_EPOCH` because an explicit flag is testable without ambient env, and a single UTC interpretation removes the local-TZ ambiguity Reviewer A flagged. The committed golden is **self-consistent** (Rust generates it, Rust re-checks it); the gate's timestamp-line normalization is what bridges Rust↔Perl, so the hook's TZ never needs to match Perl's. Default (flag unset) = local time, formatted exactly as Perl.

**⚠️ The checked-in `plotly/bismark_bt2_PE_report.html` is STALE and must NOT be used as the oracle:** its footer says **v0.19.1** and its timestamp is **HH:MM** (no seconds), whereas current `getLoggingTime` emits **HH:MM:SS**. The oracle is a **fresh run of the current Perl `bismark2report v0.25.1`** on the same inputs.

**Test oracle (mirror genomeprep/methcons):** the **Perl `bismark2report` script is the primary oracle from Phase A** — run it on the same fixtures and diff (auto-skip if `perl` absent). Keep a few committed hand-checked goldens (deterministic timestamp via `--__test_timestamp`) for the subtle edges. **Fixtures must be created/captured** — the repo currently has **no** Bismark report fixtures (only `docs/images/bismark_summary_report.txt`, which is for the *different* `bismark2summary` tool).

**Required fixtures (rev 1 — from dual plan-review):** beyond the PE/SE × optional-report matrix, the suite MUST include:
1. **M-bias absent** → assert all **24** `{{mbias*}}` placeholders survive literally in the script blocks (and both `<div>` sections are deleted).
2. **M-bias SE** (R1 only) → assert the **12 `{{mbias2_*}}`** survive while the R2 `<div>` section is deleted (the `$state` vs `%mbias_2` interaction).
3. **Fill-gate FAILURE** (e.g. an alignment report missing the `no_genomic` line) → assert that section's placeholders survive **and exit code is 0**.
4. **`0`-through-a-gate** (`no_genomic: 0` and/or `dups: 0`) → assert the gate **PASSES** (guards against a truthiness regression — the §2.7 `is_some` contract).
5. **Dedup leftover-fallback** (no `Total count of deduplicated leftover sequences:` line) → assert `leftover = total − dups` (integer).
6. **Amplicon missing-nucleotide-key** (issue #711) → assert absent keys render `0` for percentages but the **empty string** for counts/coverage (Perl undef-in-replacement).
7. **Two alignment reports in one dir + explicit `--dedup_report`** → assert the line-1256 reset: the explicit dedup applies to report #1, report #2 falls back to auto-detection.

**Real-data validation (later, on `oxy`, `#[ignore]`):** run Perl + Rust on real Bismark report sets from the benchmark datasets; diff with the timestamp line normalized. Verify oxy's env on arrival (report paths, `perl` availability, `~/.cargo/bin`).

**NOT in the gate:** STDOUT/STDERR diagnostics; `--help`/`--version` text; subprocess/UX timing.

---

## 8. Gotchas & candidate spikes (call-outs)

1. **STALE reference HTML (load-bearing).** `plotly/bismark_bt2_PE_report.html` is v0.19.1 / HH:MM / 2018 → wrong on the timestamp format **and** the version footer. Generate the oracle fresh from current Perl. *(This is the single biggest trap.)*
2. **Asset line normalization (byte contract).** Replay `chomp` + `s/\r//g` (strip **all** `\r`, not just trailing) + append `\n` per line on each embedded asset. Rust `str::lines()` only strips a *trailing* `\r` → would diverge on any mid-line `\r`. Implement a faithful helper (split on `\n`, drop the trailing empty element if the content ended in `\n`, `replace('\r',"")` per piece, rejoin with `\n`, append final `\n`). **Empty-input guard (rev 1, Reviewer B):** an **empty** asset yields `""` in Perl (`while(<DOC>)` never iterates), **not** `"\n"` — so "`$doc` always ends in `\n`" holds only for non-empty input; the generic helper must special-case empty → empty (harmless for the four non-empty assets, but the helper is described generically, so add the guard + a test). **Candidate Spike A:** `include_str!` the assets + helper, assert the reconstructed `$doc`-equivalent matches a fresh Perl run's intermediate.
3. **Greedy/dotall section deletion.** `{{marker}}.*{{marker}}` with `/s` deletes first→last marker inclusive. Implement as first-index … last-index-of-second-marker splice; do **not** use a lazy match. Each marker name occurs exactly twice in the template.
4. **M-bias is the trickiest section (rev 1 — both reviewers).** THREE distinct facts: **(a) section deletion** of the `<div>` containers is driven by the header-derived `$state` (line 907) — SE (`single`) deletes the whole R2 block, PE deletes only the R2 markers, absent deletes **both** R1+R2 blocks; **(b) fill** of `{{mbias2_*}}` is driven by `%mbias_2` (R2 **data rows** present, line 977), which can diverge from `$state` (R2 header, no rows → block kept but placeholders unfilled); **(c) the `{{mbias1_*}}`/`{{mbias2_*}}` data placeholders are in the trailing `<script>` blocks OUTSIDE the deletable spans** (template 836–1141 vs 465–496), so they survive literally whenever unfilled — **all 24** with no M-bias report, the **12 `{{mbias2_*}}`** for **every SE sample**. Cover all three with goldens (§5.4, §7 fixtures 1–2).
5. **Unknown-context inject snippets — exact whitespace.** The 3 `<tr>` blocks use literal mixed tabs/spaces (header `<tr>`: 5 spaces; `<th>`: 32 spaces; `<td>`: 4 spaces + 4 tabs; `</tr>`: 4 spaces + 3 tabs) and embedded `\n`. Lift the exact bytes from the Perl source; cover with a Bowtie2 (Unknown-context-present) fixture.
6. **All-or-nothing fill gates** (alignment 5 fields; splitting 6 fields; nuc `looksOK`; dedup 4 fields) → on failure, **placeholders survive** in the output. Reproduce; do not partially fill or default.
7. **No numeric reformatting** anywhere except `%`-strip, dedup `\s.*`-trim, and the integer leftover fallback. The nucleotide log2 ratio is computed but **commented out** — confirm no float ever reaches the output. This is why "sprintf parity" collapses to the timestamp line only.
8. **Plotly array separators differ.** Nucleotide x-arrays join with `" , "`, y-arrays with `"','"` (quoted); all other plot strings join with `","`. Easy to get wrong.
9. **Nucleotide fixed key order** (`A,T,C,G,AC,CA,…,AA`) — NOT sorted; missing keys → `0` for percentages, empty string for counts/coverage (Perl undef-in-replacement). Cover the all-present case; note the amphibious missing-key edge (issue #711 context: amplicon genomes).
10. **PE/SE detection drives both labels and strand regexes.** A report is classified by exact line text; strand-origin patterns differ (`^CT/GA/CT:` PE vs `^CT/CT:` SE). The SE patterns do **not** match PE lines — `CT/GA:` is *not* a prefix of `CT/GA/CT:` (the 6th char is `:` vs `/`), so the trailing colon makes them mutually exclusive (Reviewer B's "prefix" claim was imprecise; the rev-0 "distinct anchoring" reasoning holds). **Still, preserve Perl's `elsif` first-match-wins branch order** rather than relying on independent prefix tests — free insurance against a future pattern that *does* overlap. Lock with a PE and an SE strand fixture.
11. **Companion-flag reset across multiple alignment reports** (Perl line 1256 `undef`-resets the explicit vars) — an explicit `--dedup_report X` applies to the **first** report, then subsequent reports fall back to auto-detection. Subtle; reproduce or document a deliberate divergence.
12. **Output naming with `-o`** uses the value **verbatim** (no `.html` append); only legal with a single report.
13. **Replacement-string safety.** Perl `s/pat/$var/` inserts the variable's value literally (no re-interpretation of `$`/`\` inside the value). The 3 MB `plot.ly` and base64 logos contain no `{{name}}` placeholders or replacement metacharacters that matter; a literal splice in Rust is correct. Confirm no asset contains a live `{{…}}` token (base64 has no braces; plotly.js — verify in Spike A).
14. **Glob basename regex** `^(.+)_(P|S)E_report.txt$`: a `*E_report.txt` match that lacks `_PE`/`_SE` (e.g. a file literally `E_report.txt`) leaves `$basename` undefined → companion globs degrade to `*deduplication_report.txt` etc. Edge; document, low priority.

**Candidate spikes (run during planning if review wants empirical confirmation; none blocks the SPEC):**
- **Spike A — asset embedding + normalization:** confirm `include_str!` + the faithful normalizer reconstructs the Perl in-memory `$doc` for `plotly_template.tpl` and that the 3 inject regexes have unambiguous markers; confirm `plot.ly` contains no live `{{…}}`.
- **Spike B — Perl oracle harness:** craft minimal valid PE + SE report sets (alignment + all 4 companions), run current Perl `bismark2report`, capture HTML, and confirm (i) timestamp format `HH:MM:SS`, (ii) which placeholders fill/survive, (iii) Unknown-context snippet bytes. Establishes the Phase-A oracle and the first goldens.

---

## 9. Scope for v1.0

The tool is small and the byte-identity gate (the HTML) exercises every code path, so **everything is v1.0** — there is no natural "defer" candidate:

| Feature | Verdict |
|---|---|
| Alignment parser (PE + SE, context methylation, strand origin, Unknown context) | **v1.0 (mandatory — the data spine)** |
| Dedup / splitting / M-bias / nucleotide parsers (all optional) | **v1.0** |
| Template fill + asset inject + section present/absent deletion + M-bias R2 SE/PE rule | **v1.0** |
| Auto-detection globs **and** explicit `--*_report` flags + `none` skip | **v1.0** |
| `--dir`, `-o/--output`, multi-report loop, output naming | **v1.0** |
| `--version`, `--help`/`--man`, `--verbose` | **v1.0** (text not byte-gated) |
| Deterministic-timestamp test hook (hidden `--__test_timestamp` epoch, UTC) | **v1.0** (enables stable committed goldens; §7) |

**Out of scope (regardless):**
- `bismark2summary` (separate multi-sample tool).
- Byte-matching STDOUT/STDERR, `--help`/`--version` text.
- `Getopt::Long` `auto_abbrev`.
- Re-rendering / upgrading the Plotly library or template design (the asset is shipped as-is for byte-identity).

---

## 10. Next steps (workflow)
1. ✅ **Manual review + dual plan-review COMPLETE** (rev 0 → rev 1; both reviewers' findings folded into this revision). Scope (§9), crate name (§3), exit codes (§6.1), and the timestamp hook (§7) are decided.
2. **Next: phased implementation PLAN** (mirror genomeprep/methcons `PLAN.md`) → its own **dual plan-review**.
3. Optionally run **Spike A/B** if the PLAN review wants empirical confirmation first.
4. Implement **only on an explicit trigger** (`implement` / `/code-implementation`).
5. **Dual code-review + plan-manager coverage audit**, then real-data byte-identity on `oxy`, docs/CHANGELOG/README, PR. Merge into `rust/iron-chancellor` only on an explicit "merge for me".
