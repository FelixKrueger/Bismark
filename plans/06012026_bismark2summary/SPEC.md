# SPEC — `bismark-summary` (Rust port of Perl `bismark2summary`)

**Status:** DRAFT rev 1 (2026-06-01) — **dual plan-review findings + glob-sort spike folded in** (`SPEC_REVIEW_A.md` / `SPEC_REVIEW_B.md` / `SPIKE_glob_sort_order.md`). Awaiting implementation trigger. Do **not** implement until Felix gives the explicit `implement` / `/code-implementation` trigger.
**Date:** 2026-06-01
**Rev 1 changes (dual plan-review + spike folded in):** (a) **Glob sort CORRECTED** — Perl `glob` is **case-fold-primary, raw-ASCII-bytes-secondary, locale- and platform-invariant** (spike: macOS Perl 5.34.1 ≡ oxy/Linux Perl 5.38.2, incl. case-only tiebreak); Rust uses `(ascii_lowercased, original_bytes)`, **NOT** bytewise `.sort()` (Reviewer A C1; Reviewer B's bytewise assumption was wrong). (b) **§2.9 section-deletion restated** — the numbers deletion keys off `$dup_alignments =~ /^,{1,}$/` (`:1430`) while the percentage deletion keys off `if ($aligned)` (`:1577`): **different predicates, NOT a "mirror"**; for a **single RRBS sample** they disagree (numbers in DEDUP layout, percentages in RAW layout) — reproduce verbatim (Reviewer A C2). (c) **Fill-then-delete order pinned** — `{{aligned_seq}}` fill (`:1419`) precedes raw-section deletion (`:1430-1442`); `{{p_aligned_replace}}` fill is gated `if ($aligned)` (`:1591`) so it never fires in dedup mode (Reviewer B). (d) `/^,{1,}$/` **needs ≥1 comma** → N=1 arrays never match (load-bearing). (e) `-o 0` / `--title 0` truthiness → Rust must test `is_empty() || == "0"`. (f) `.txt` col 1 is the **raw `$bam`**, not stripped `$base`. (g) Expanded fixture matrix (§7): mixed-case glob, single-RRBS, two-RRBS, all-RRBS, single-WGBS, all-excluded, plot-excluded-in-middle, non-trivial-`%.15g`-tail, stale-oracle tripwire.
**Branch / worktree:** `rust/bismark2summary` @ `~/Github/Bismark-summary` (off `origin/rust/iron-chancellor` @ `ade2ede`)
**Perl source of truth:** `bismark2summary` (repo root, **1722 lines**, `$bismark_version = '0.25.1'`, "Last modified 09 11 2020")
**Assets:** `plotly/plot.ly` (≈3.0 MB, plotly.js), `plotly/bismark.logo` (28 KB), `plotly/bioinf.logo` (17 KB). **The HTML template itself is an INLINE single-quoted heredoc inside the Perl script (lines 489–1372)** — there is **no** `.tpl` file (this is the key structural difference from `bismark2report`).
**New crate:** `rust/bismark-summary` (lib + bin `bismark2summary_rs`); add to `rust/Cargo.toml` `members`.
**Acceptance gate:** **both** outputs byte-for-byte identical to the current Perl `v0.25.1`:
- `bismark_summary_report.txt` — **fully** byte-identical.
- `bismark_summary_report.html` — byte-identical **modulo the single `localtime` timestamp line** (defined precisely in §7).

**⚠️ Scope clarity (load-bearing).** `bismark2summary` is the **PROJECT-LEVEL, multi-sample aggregator**: it scans a run folder for Bismark BAMs, locates each one's report files, parses per-sample metrics, and emits **ONE** project summary (`.txt` table + `.html` with stacked-area graphs). It is **NOT** `bismark2report` (the per-sample HTML report, ported separately on `rust/bismark2report` @ `~/Github/Bismark-report`). They **share** report-parsing logic and the `plotly/` assets, but this is its own crate and the `bismark2report` worktree is **not** touched here. Decision (Felix, 2026-06-01): **duplicate** the small report parsers in this crate rather than couple to the not-yet-merged `bismark-report` crate; note promotion to a shared module as a future cleanup.

---

## 1. Purpose & one-paragraph summary

`bismark2summary` produces a single **project-wide** roll-up across many samples. It **never opens a BAM** — it uses BAM **filenames** only, to derive the names of each sample's text report files. For each discovered BAM it reads the **alignment report** (mandatory) plus the **deduplication** and methylation-extractor **splitting** reports (both optional), parses read counts and per-context methylation counts, then writes two files: a tab-delimited `bismark_summary_report.txt` (one row per sample, 15 columns) and a self-contained `bismark_summary_report.html` (two stacked-area alignment graphs — read numbers + percentages — and three methylation-context percentage graphs for CpG/CHG/CHH, rendered with the inlined plot.ly library). The Rust port must reproduce both files byte-for-byte against current Perl `v0.25.1` (the HTML modulo its one `localtime` line).

Like `bismark2report` (and `bismark-genome-preparation`), this port has **no BAM/SAM dependency** — no `bismark-io`, no noodles. It is a CLI + a directory glob + line parsers + a string-substitution templating engine. **Unlike** `bismark2report`, it **does** perform numeric reformatting: the HTML graph data carries `sprintf("%.2f")` percentages plus a `100 − rounded` complement, so byte-identity here requires faithful float formatting (§2.9, §8).

---

## 2. Perl behavior — the contract (derived from source; line numbers cited)

### 2.1 Top-level flow
1. `process_commandline()` (30–80) parses options → `($report_basename, $page_title, $verbose)`.
2. Read the three assets via `read_report_template` (127–149): `plot.ly`, `bismark.logo`, `bioinf.logo`.
3. **Discover BAMs** (152–205): explicit `@ARGV`, else auto-glob (§2.3).
4. Apply basename/title defaults (207–212).
5. **Per-BAM loop** (248–458): derive report filenames, parse the 3 reports, append a `.txt` row (raw values), then (after 0-defaulting + plot-exclusion) push to the 13 plot-data arrays.
6. Join arrays into comma strings (460–473), write `.txt` (478–481).
7. Build the HTML from the inline template: asset injection → placeholder fills → conditional section deletion → percentage computation → write `.html` (487–1721).

### 2.2 CLI options (`GetOptions`, 36–41; help 82–124)

| Perl option | Type | Behavior |
|---|---|---|
| `-o` / `--basename <s>` | string | Output **basename**; emits `<basename>.txt` **and** `<basename>.html` (suffixes **always** appended). Default `bismark_summary_report` (208). |
| `--title <s>` | string | HTML report title → `{{page_title}}`. Default `Bismark Summary Report` (211). |
| `--verbose` | flag | Extra STDOUT/STDERR diagnostics. **Not** byte-gated. |
| `--version` | flag | Print version banner (55–68), exit. |
| `--help` / `--man` | flag | Print help (82–124), exit. |
| *(positional)* `[<BAM file(s)>]` | args | Optional explicit BAM list; if absent, auto-detect (§2.3). |

There is **no `--dir`** and **no per-report flag** — far simpler than `bismark2report`.

- GetOptions failure → `die "Please respecify command line options\n"` (44–46).
- `unless ($report_basename)` / `unless ($page_title)` use **Perl truthiness** (207, 210): an empty **or literal `"0"`** value falls back to the default. Reproduce (edge: `-o 0` → `bismark_summary_report`; `--title 0` → `Bismark Summary Report`). **Implementation note (Reviewer A I3):** clap yields `Some("0")` for `-o 0`, so the Rust default-fallback must test `value.is_empty() || value == "0"`, **not** merely `is_none()`.

### 2.3 BAM discovery (152–205) — **row-order is byte-identity-critical**
- `@bam_files = @ARGV`. If the user supplied BAMs, they are used **verbatim, in argv order** (no globbing, no existence check).
- Otherwise auto-detect via four globs, **appended in this fixed order**, each glob contributing its own lexically-sorted matches:
  1. `<*bismark_bt2.bam>` — SE Bowtie2
  2. `<*bismark_bt2_pe.bam>` — PE Bowtie2
  3. `<*bismark_hisat2.bam>` — SE HISAT2
  4. `<*bismark_hisat2_pe.bam>` — PE HISAT2
- The four patterns are mutually exclusive (a `_pe.bam` file does not match the non-`_pe` glob — the literal suffix differs). Each prints a `warn` "Found/No …" line (informational, not gated).
- **0 BAMs total → `die`** (200–202). >0 → `warn "Generating … from N … file(s)"`.
- **Row order** in both outputs = this discovery order (or argv order). **Perl `glob` sort is CASE-FOLD-PRIMARY, raw-ASCII-bytes-secondary, and locale-/platform-invariant** (NOT bytewise) — empirically confirmed identical on macOS Perl 5.34.1 and oxy/Linux Perl 5.38.2 across default / `LC_ALL=C` / `en_US.UTF-8`, including the case-only tiebreak (`SPIKE_glob_sort_order.md`). Reproduce in Rust per glob with:
  ```rust
  matches.sort_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase())
                          .then_with(|| a.as_bytes().cmp(b.as_bytes())));
  ```
  then concatenate the four globs in the fixed order above. **Do NOT** use a plain bytewise `.sort()` or the `glob` crate's default sort (both bytewise → uppercase-first → diverges on any mixed-case sample set). Example: Perl/correct = `apple, Mango`; bytewise = `Mango, apple`. **Pinned by a mandatory mixed-case fixture (§7).** The explicit-`@ARGV` path is verbatim argv order — unaffected.

### 2.4 Per-BAM report-filename derivation (251–367)
- `$base = substr($bam, 0, -4)` — strip the trailing 4 chars (`.bam`). (Applied unconditionally; a non-`.bam` argv entry loses its last 4 chars — edge, document.)
- **PE/SE:** if `$base =~ /_pe$/` → strip `_pe`, `bm_report = "${base}_PE_report.txt"`, `paired=1`; else `bm_report = "${base}_SE_report.txt"`.
- **Alignment report is MANDATORY:** `-e $bm_report` → proceed; else `die "Could not find Bismark report ($bm_report) to open\n"` (280–285).
- **Dedup report (optional):** PE → `"${base}_pe.deduplication_report.txt"`; SE → `"${base}.deduplication_report.txt"` (319–324).
- **Splitting report (optional)** — name depends on **whether the dedup file exists** (`-e $dedup`, 352–367):
  - PE + dedup present → `"${base}_pe.deduplicated_splitting_report.txt"`; PE + no dedup → `"${base}_pe_splitting_report.txt"`.
  - SE + dedup present → `"${base}.deduplicated_splitting_report.txt"`; SE + no dedup → `"${base}_splitting_report.txt"`.
- Missing dedup / splitting → `warn "No … report present, skipping…"` and leave those fields empty.

### 2.5 Parsers (each fills per-sample scalars; all initialised to `''` at 252–265)

**(a) Alignment report (288–315)** — PE vs SE patterns selected by `$paired_end`:

| Field | PE pattern | SE pattern |
|---|---|---|
| `total_reads` | `^Sequence pairs analysed in total:\s+(\d+)$` | `^Sequences analysed in total:\s+(\d+)$` |
| `unaligned` | `^Sequence pairs with no alignments under any condition:\s+(\d+)$` | `^Sequences with no alignments under any condition:\s+(\d+)$` |
| `ambig_reads` | `^Sequence pairs did not map uniquely:\s+(\d+)$` | `^Sequences did not map uniquely:\s+(\d+)$` |
| `no_seq_reads` | `^Sequence pairs which were discarded because genomic sequence could not be extracted:\s+(\d+)$` | `^Sequences which were discarded because genomic sequence could not be extracted:\s+(\d+)$` |
| `aligned_reads` | `^Number of paired-end alignments with a unique best hit:\s+(\d+)$` | `^Number of alignments with a unique best hit from the different alignments:\s+(\d+)$` |

Context methylation (both PE/SE, 305–311) — note **`total_c` is `$`-anchored; the six meth/unmeth patterns are not**:
- `total_c`: `^Total number of C's analysed:\s+(\d+)$`
- `meth_cpg/chg/chh`: `^Total methylated C's in {CpG,CHG,CHH} context:\s+(\d+)`
- `unmeth_cpg/chg/chh`: `^Total unmethylated C's in {CpG,CHG,CHH} context:\s+(\d+)`

**Last-match-wins** semantics (Perl scans every line; the last matching line sets the value). Reproduce (scan all lines; overwrite).

**(b) Dedup report (326–345)** — only if `-e $dedup`:
- `^Total number of alignments analysed in .+:\s+(\d+)$` → **overwrites** `aligned_reads` (330–333).
- `^Total number duplicated alignments removed:\s+(\d+)` → `dup_reads`.
- `^Total count of deduplicated leftover sequences:\s+(\d+)` → `unique_reads`.
- *(Three **independent `if`** statements, NOT `elsif` (330/334/338); behavior unaffected since no line matches two patterns, but implement as independent matches — Reviewer B.)*

⚠️ The dedup "Total number of alignments analysed" is the **pre-dedup** aligned-read count and **overwrites** the alignment-report `aligned_reads`. So the `.txt` "Aligned Reads" column means *pre-dedup alignments* when a dedup report exists, and *unique-best-hit alignments* otherwise.

**(c) Splitting report (369–382)** — only if `-e $meth_extract`; **overwrites** the context-methylation fields, with a **different unmethylated pattern**:
- `total_c`: `^Total number of C's analysed:\s+(\d+)$`
- `meth_cpg/chg/chh`: `^Total methylated C's in {CpG,CHG,CHH} context:\s+(\d+)`
- `unmeth_cpg/chg/chh`: `^Total C to T conversions in {CpG,CHG,CHH} context:\s+(\d+)` *(NOT "Total unmethylated C's")*

So splitting-report methylation **takes precedence** over alignment-report methylation when present.

### 2.6 The `.txt` row — captured **before** any mutation (387–404)
`@csvrow = ($bam, $total_reads, $aligned_reads, $unaligned, $ambig_reads, $no_seq_reads, $dup_reads, $unique_reads, $total_c, $meth_cpg, $unmeth_cpg, $meth_chg, $unmeth_chg, $meth_chh, $unmeth_chh)`, `join("\t", …) . "\n"`, appended to `$summary_csv` (which starts as the header line, 246).

- These are the **raw captured strings** — an un-found field is the **empty string `''`** (yields an empty cell). This row is appended **before** the 0-defaulting at 412–424, so the `.txt` keeps blanks; the *plot arrays* (§2.7) use the mutated values.
- **Column 1 (`File`) is the raw `$bam` string** (line 388) — **not** the stripped `$base` (§2.4) and **not** the munged `$name` (§2.7.1). So a non-`.bam` argv entry still appears verbatim in column 1 even though `$base` lost its last 4 chars (Reviewer B).
- **Header (228–246), reproduced verbatim incl. the casing Perl-ism:**
  `File⇥Total Reads⇥Aligned Reads⇥Unaligned Reads⇥Ambiguously Aligned Reads⇥No Genomic Sequence⇥Duplicate Reads (removed)⇥Unique Reads (remaining)⇥Total Cs⇥Methylated CpGs⇥Unmethylated CpGs⇥Methylated chgs⇥Unmethylated chgs⇥Methylated CHHs⇥Unmethylated CHHs`
  (⇥ = tab). **Columns 12–13 are lowercase `chgs`** while CpG/CHH are capitalised — a source quirk to copy exactly. *(The checked-in `docs/images/bismark_summary_report.txt` says `CpHs` here — it is STALE; §7.)*
- `$summary_csv` always ends with a trailing `\n` (every row, incl. the header, appends `"\n"`).

### 2.7 Plot-data array assembly — **after** mutation, with plot-exclusion (406–456)
1. **Sample label** `$name` (406–410): start from `$bam`, then **in order**: `s/_bismark.bam$//` (unescaped `.` ⇒ any char), `s/\.fq\.gz$//`, `s/_trimmed$//`, `s/_[12]$//`. *(On modern `*_bismark_bt2.bam` names these are usually no-ops; the label is then the full BAM name — see the §7 example data.)* The category pushed is `"'$name'"` (single-quote-wrapped, 442).
2. **0-defaulting** (412–424): `unaligned`, `ambig_reads`, `no_seq_reads` → `0` if `''`. Then **`if ($dup_reads ne '') { $aligned_reads = "" }`** (416–418) — blank the raw aligned count when a dedup report was present (so the "Raw Aligned Reads" trace is empty and the dedup/unique traces drive the graph). Then `meth_cpg, unmeth_cpg, meth_chg, unmeth_chg, meth_chh, unmeth_chh` → `0` if `''`. *(`total_reads`, `dup_reads`, `unique_reads`, `total_c` are NOT defaulted.)*
3. **Plot exclusion** (427–440): `next` (skip pushing this sample to plot arrays) if `meth_cpg==0 && unmeth_cpg==0`, **or** `meth_chg==0 && unmeth_chg==0`, **or** `meth_chh==0 && unmeth_chh==0`. Each prints a `warn "Excluding sample >$name< … no calls in <ctx> context"`. **The sample's `.txt` row is already written** — exclusion affects graphs only.
4. **Push** (442–455) to 13 arrays: `categories, aligned, not_aligned, ambig_aligned, no_seq, dup_alignments, unique_alignments, meth_cpg_string, unmeth_cpg_string, meth_chg_string, unmeth_chg_string, meth_chh_string, unmeth_chh_string`.

### 2.8 Asset reading & line normalization (127–149) — the `read_report_template` contract
Each of `plot.ly`, `bismark.logo`, `bioinf.logo` is read line-by-line: per line `chomp`, then `s/\r//g` (strip **all** `\r`, not only a trailing CR), then `$doc .= $_ . "\n"`. Consequences (identical to `bismark2report` §2.6): fully **LF-normalized**, every line `\n`-terminated, **non-empty** input always ends in `\n`; an **empty** file yields `""` (the `while(<DOC>)` never iterates) — special-case empty → empty. The **inline HTML template** (the heredoc) does **NOT** go through this — it is a single-quoted Perl heredoc, so its bytes are exactly lines 490–1371 of the source (LF in the repo), ending `</html>\n`.

### 2.9 HTML assembly — ordered mutations on `$html_report` (1376–1719) — load-bearing order
`$report_timestamp = localtime;` (488) → scalar `localtime` → ctime string `"Www Mmm DD HH:MM:SS YYYY"` (day-of-month **space-padded** to width 2, e.g. `"Mon Jun  1 09:07:00 2026"`). Then, in order:

1. **plot.ly inject** (1378): `s/\{\{plotly_goes_here\}\}.*\{\{plotly_goes_here\}\}/$plotly_code/s` — greedy + dotall; replaces the two markers **and everything between** with the asset. **`die` if not found** (1381–1383).
2. **bismark.logo** (1384): single `s/\{\{bismark_logo_goes_here\}\}/…/` (no `/g`; one occurrence).
3. **bioinf.logo** (1385): single `s/\{\{bioinf_logo_goes_here\}\}/…/`.
4. **timestamp** (1386): `s/\{\{report_timestamp\}\}/$report_timestamp/g`.
5. **page_title** (1387): `/g`. **num_samples** (1391): `/g` — `$num_samples = scalar @bam_files` (247) = **total BAM count incl. plot-excluded samples**.
6. **x-values** (1393–1399): `@x_values = (1..$num_samples)`, joined `,`; fill `{{x_values_alignment}}` and `{{x_values_methylation}}` `/g`. ⚠️ When samples are plot-excluded, x has `num_samples` points but the y-arrays/`categories` have fewer — Perl emits the mismatch as-is (plot.ly zips to min length). Reproduce faithfully.
7. **filenames_replace** ← `$categories` `/g` (1403). **bismark_version** ← `0.25.1` `/g` (1405).
8. **Alignment numbers section** (1411–1455) — deletion gated on **`$dup_alignments`**:
   - If `$aligned =~ /^,{1,}$/` → set `$aligned = ''` (1412–1415). **`/^,{1,}$/` requires ≥1 comma and all-commas**, so it matches only when there are **≥2 samples and every one was blanked** (i.e. ≥2 dedup samples). A single sample's join is comma-free → never matches (load-bearing — Reviewer A I1).
   - **Fill `{{aligned_seq}}`, `{{no_seq}}`, `{{not_aligned}}`, `{{ambig_aligned}}` `/g` (1419–1426) — this happens BEFORE the section deletions below.** (Reviewer B: preserve Perl statement order; in dedup mode `{{aligned_seq}}` is filled with the empty `$aligned`, then its whole raw section is deleted → net no raw trace, no surviving literal. A reorder-then-fill impl risks a stray empty trace or a surviving `{{…}}`.)
   - **Section deletion (1430–1442) gated on `$dup_alignments =~ /^,{1,}$/`:**
     - **all-commas TRUE (≥2 samples, none deduplicated):** delete the `{{deduplicated_unique_reads_section}}…{{…}}` span (`//s`), delete the `{{duplicated_reads_section}}…` span, `s/\{\{raw_aligned_reads_section\}\}//g` (keep raw trace), set `$dup_alignments=''`.
     - **else (≥1 deduplicated sample, OR any single sample):** delete the `{{raw_aligned_reads_section}}…` span, `s/\{\{deduplicated_unique_reads_section\}\}//g`, `s/\{\{duplicated_reads_section\}\}//g` (keep dedup+dup traces).
   - Fill `{{dup_alignments}}` `/g` (1443). `{{unique_alignments}}`: if all-commas set `''`, fill `/g` (1447–1454).
9. **Alignment percentages** (1458–1599) — deletion gated on **`$aligned`** (a DIFFERENT predicate — **NOT** a mirror of step 8; see ⚠ box below):
   - For each plotted sample index: if `$aligned` truthy (raw mode) — if `$aligned_arr[$i] eq ''` → **`die`** "mix of samples, e.g. RRBS as well as WGBS" (1488–1490) — `total = aligned+no_seq+not_aligned+ambig`; else (`$aligned` falsy ⇒ dedup mode) `total = unique+dup+no_seq+not_aligned+ambig`.
   - `p_* = sprintf("%.2f", part/total*100)` for each part (1506–1515): raw mode emits `p_aligned`; dedup mode emits `p_deduplicated_unique_alignments` + `p_duplicated_alignments`; both emit `p_no_seq`, `p_unal`, `p_ambig`.
   - **Section deletion (1577–1588) gated on `if ($aligned)`:** raw (`$aligned` truthy) → delete `{{deduplicated_unique_reads_percentage_section}}…` + `{{duplicated_reads_percentage_section}}…` spans, keep `{{raw_unique_reads_percentage_section}}` markers; dedup (`$aligned` falsy) → delete the raw percentage span, keep the dedup+dup markers.
   - Fill (single subst each, 1590–1599): raw → `{{p_aligned_replace}}` (**gated `if ($aligned)` at 1591 — never fires in dedup mode**, where the raw percentage span was already deleted); dedup → `{{p_deduplicated_unique_alignments}}` + `{{p_duplicated_alignments}}`; both → `{{p_no_seq_replace}}`, `{{p_unal_replace}}`, `{{p_ambig_replace}}`.

> ⚠ **The numbers and percentage section deletions key off DIFFERENT variables and can disagree (Reviewer A C2, confirmed end-to-end against Perl).** Numbers → `$dup_alignments =~ /^,{1,}$/` (1430); percentages → `if ($aligned)` (1577). They agree for ≥2 all-RRBS (both raw), ≥2 all-WGBS (both dedup), and all-excluded/empty (both dedup). **They DIVERGE for exactly ONE raw/RRBS sample:** `$dup_alignments=""` (comma-free → numbers take the **DEDUP** layout) while `$aligned="900"` (truthy → percentages take the **RAW** layout). Perl emits this genuinely inconsistent HTML (numbers section shows the Raw trace AND empty dedup/dup traces; percentage section shows only `p_aligned`); **no `{{…}}` survive.** Reproduce verbatim — do NOT normalise. Pin with a single-RRBS fixture AND a two-RRBS fixture (§7).
10. **Methylation raw strings** (1607–1618): fill `{{meth_cpg_string}}`, `{{unmeth_cpg_string}}`, … `/g` (these placeholders live inside HTML **comments** in the template — they fill the comment text; reproduce).
11. **Methylation percentages** (1628–1711):
    - Per plotted sample: `total_CpG = meth_cpg+unmeth_cpg`, `total_CHG`, `total_CHH`.
    - **CpG:** if `total_CpG==0` → `p_CpG_meth='NA'`, `p_CpG_unmeth='NA'`; else `p_CpG_meth = sprintf("%.2f", meth_cpg/total_CpG*100)`, `p_CpG_unmeth = 100 - p_CpG_meth` (§2.9a).
    - **CHG:** if `total_CHG==0` → `'0','0'`; else sprintf + `100 -`.
    - **CHH:** if **`total_CHG==0`** (⚠️ **latent Perl bug — tests `total_CHG`, not `total_CHH`**, 1662) → `'0','0'`; else `sprintf("%.2f", meth_chh/total_CHH*100)` + `100 -`. *(Effectively dead code: plot-excluded samples (step 3) guarantee all three context totals > 0, so the `else` always runs — but reproduce the buggy branch verbatim for safety.)*
    - Fill `{{p_CpG_m_replace}}`, `{{p_CpG_u_replace}}`, `{{p_CHG_m_replace}}`, `{{p_CHG_u_replace}}`, `{{p_CHH_m_replace}}`, `{{p_CHH_u_replace}}` (single subst each, 1706–1711).
12. **Write** `<basename>.html` (1716–1719).

#### 2.9a The percentage stringification contract (the one real numeric subtlety)
- **Meth percentages and all alignment percentages** are `sprintf("%.2f", x)` → 2-decimal strings (e.g. `"12.34"`, `"100.00"`, `"0.00"`), joined **verbatim** (trailing zeros kept).
- **The six unmeth methylation arrays** are computed as `100 - $p_*_meth`, where `$p_*_meth` is the **already-rounded `%.2f` string**. Perl numifies that string, subtracts from integer `100`, and the resulting NV is stringified by Perl's **default `%.15g`** formatting → **trailing zeros dropped**: `100-"12.34"→"87.66"`, `100-"50.00"→"50"`, `100-"100.00"→"0"`, `100-"12.30"→"87.7"`. So `p_*_m` arrays keep `.00`/`.DD`, but `p_*_u` arrays are `%.15g`-style. **Asymmetric — reproduce exactly.**
- Rust reproduction: `let m = format!("{:.2}", part/total*100.0); let u = format_g15(100.0 - m.parse::<f64>().unwrap());` — i.e. round to 2dp **first**, re-parse the rounded string, then subtract and `%.15g`-format. Reuse `fmt_g::format_g15` (copied from `bismark-bedgraph`, §3 / §8).

### 2.10 Output writing
- `<basename>.txt` (478–481): `print SUMMARY_CSV $summary_csv` — header + every sample row (incl. plot-excluded). `die` on open failure.
- `<basename>.html` (1716–1719): `print SUMMARY_HTML $html_report`.

---

## 3. Reuse map — what comes from the existing workspace

`bismark-summary` is **standalone** (no `bismark-io`, no noodles, no `flate2` — all inputs are plain text; the `.html` is uncompressed). Reuse is convention-level + one copied module:

| Need | Reuse / source | Notes |
|---|---|---|
| CLI parse, `--version`, exit codes | clap derive; mirror `bismark-dedup`/`bismark-genome-preparation` `cli.rs`+`main.rs` | Keep Perl spellings: `-o/--basename`, `--title`, `--verbose`, `--version`, `--help/--man`, positional BAMs. |
| Embedded assets | `include_str!` the three `plotly/` files (`../../../plotly/{plot.ly,bismark.logo,bioinf.logo}`) | Same as `bismark2report`. Replay the §2.8 normalizer (NOT `str::lines()`, which only strips a *trailing* `\r`); empty-input guard. |
| Inline HTML template | extract the heredoc (source lines 490–1371) **verbatim** into `src/summary_template.html`, `include_str!` it | Add a `#[ignore]`/`perl`-guarded test that re-extracts the heredoc from the Perl source and asserts byte-equality (drift guard). |
| `%.15g` float formatting | **copy `bismark-bedgraph/src/fmt_g.rs`** (`format_g15`) into this crate | Validated vs C `printf("%.15g")` across 2M+ fractions; powers the `100 − rounded` unmeth math (§2.9a). Duplicate-not-couple (matches the parser decision); note promotion to `bismark-io` as future cleanup. |
| `%.2f` rounding | `format!("{:.2}", x)` | Round-half-to-even, matches Perl/C `sprintf` (validated in c2c Phase E). |
| Auto-detect globs | `glob = "=0.3.x"` or `std::fs::read_dir` + suffix filter + per-glob lexical sort | Four patterns in fixed order (§2.3). |
| Errors / diagnostics | `anyhow` + `thiserror`; STDERR logger à la `bismark-extractor` | `--verbose` gates detail; STDERR **not** byte-matched. |
| Report parsing | hand-written line parsers (prefix match + capture) | **Duplicated** here (not shared with `bismark-report`); mirror the exact patterns in §2.5. |
| Workspace wiring | add `bismark-summary` to `rust/Cargo.toml` `members` | Current: `bismark-io, bismark-dedup, bismark-extractor, bismark-methylation-consistency, bismark-bedgraph, bismark-coverage2cytosine, bismark-genome-preparation, bismark-nome-filtering`. |

**Crate name:** `bismark-summary`. **Binary name:** `bismark2summary_rs` (Perl-name + `_rs`, matching `bismark2report_rs`/`deduplicate_bismark_rs`; drop-in for `bismark2summary`).

---

## 4. Known divergences from Perl (documented & accepted — reviewers may challenge)

1. **Assets embedded via `include_str!`**, not read from `$RealBin/plotly/`. Output identical provided the §2.8 normalization is replayed. Removes the runtime asset-path dependency (matches all prior ports).
2. **Inline template embedded as a checked-in `.html` asset** (the Perl heredoc lifted verbatim), guarded by a source-extraction byte-equality test.
3. **STDOUT/STDERR diagnostics** mirror Perl `warn`/`print` in spirit, not byte-for-byte; `--verbose` gates detail. Not gated.
4. **`--help`/`--man`/`--version` → exit 0** (clap default). Perl's `print_helpfile` ends in `exit 1` (123) — we do **NOT** reproduce that nonzero-on-help quirk (matches the `bismark2report` decision). `--man` aliases `--help`. Help/version **text** not byte-gated.
5. **`Getopt::Long` behaviors not replicated**: `auto_abbrev`, `:s` optional-value subtleties.
6. **Timestamp determinism** (§7): live Perl uses scalar `localtime` (unpinnable without patching Perl). The acceptance gate **normalizes the single timestamp line**; Rust committed goldens use a hidden **`--__test_timestamp <UNIX_EPOCH>`** flag formatted in **UTC** with Perl's exact ctime layout. Default runtime = local `localtime`.
7. **Hardcoded version `0.25.1`** appears only in `{{bismark_version}}` (footer) and the `--version` banner; the Rust crate version (`env!("CARGO_PKG_VERSION")`) drives the banner, but `{{bismark_version}}` is the **hardcoded `0.25.1`** to match the Perl HTML byte-for-byte. *(Open question O1 — confirm we hardcode `0.25.1` in the template fill, not the crate version.)*
8. **Glob sort order** reproduced as **case-fold-primary, raw-bytes-secondary** per glob (§2.3, spike-confirmed Perl behavior — NOT bytewise), glob-order concatenation. ASCII-only folding (`to_ascii_lowercase`); non-ASCII filenames (which Bismark never produces) are a documented divergence boundary, not gated.

---

## 5. Output contract — exact bytes

### 5.1 `bismark_summary_report.txt`
- Header line (§2.6) + one row per BAM in discovery/argv order, 15 tab-separated columns, each row + the header `\n`-terminated. Un-found fields render as **empty cells**. Plot-excluded samples are **present**. Values are raw integer strings captured from the reports (no reformatting). **Fully byte-identical.**

### 5.2 `bismark_summary_report.html`
- The inline template after, in order: plot.ly inject → bismark.logo → bioinf.logo → timestamp → page_title/num_samples/x-values/filenames/version fills → alignment numbers + section deletion → alignment percentages + section deletion → methylation raw strings → methylation percentages (§2.9). LF throughout; ends `</html>\n`.
- Percentage values per §2.9a (`%.2f` verbatim for m/alignment arrays; `format_g15(100 − rounded)` for the six unmeth arrays). N/A and the `'0'` zero-context cases reproduced as literal strings.
- **Byte-identical modulo the one `{{report_timestamp}}` line** (§7).

---

## 6. CLI surface (clap derive) + exit codes

```
bismark2summary_rs [OPTIONS] [BAM_FILES]...

    [BAM_FILES]...               Optional explicit Bismark BAM(s). If none, auto-detect
                                 *bismark_{bt2,hisat2}[_pe].bam in the current directory.
-o, --basename <NAME>            Output basename (default: bismark_summary_report).
                                 Emits <NAME>.txt and <NAME>.html.
    --title <STRING>             HTML report title (default: "Bismark Summary Report").
    --verbose                    Extra diagnostics.
    --__test_timestamp <EPOCH>   HIDDEN (clap hide=true): fixed UNIX epoch, formatted in UTC
                                 with Perl's ctime layout, for byte-stable goldens. Default = localtime.
-V, --version                    Print version and exit (0).
-h, --help / --man               Print help and exit (0).
```

- **Error paths → nonzero exit** (value not byte-gated): no BAMs found/supplied (`die`, 200–202); a mandatory alignment report missing (`die`, 284); the RRBS+WGBS-mix `die` (1489); GetOptions-style arg errors (clap `2`).
- An optional dedup/splitting report being absent is **normal** (warn + empty fields), exit 0.
- `--version` banner uses `env!("CARGO_PKG_VERSION")` (dedup precedent); the HTML's `{{bismark_version}}` is the hardcoded `0.25.1` (O1).

---

## 7. Acceptance / definition of "byte-identical output"

**HARD gate (vs Perl Bismark `v0.25.1`):**
1. `bismark_summary_report.txt` — **byte-for-byte identical** (no exceptions; no timestamp in the `.txt`).
2. `bismark_summary_report.html` — identical **after normalizing the single timestamp line**. Anchor the match to the exact template line `<p>Report generated on {{report_timestamp}}</p>` → after fill it reads `<p>Report generated on Www Mmm DD HH:MM:SS YYYY</p>`; replace that ctime token with a fixed token in **both** files, **assert exactly one match per file**, then `cmp` the rest. Every other byte — the 3 MB plot.ly, logos, injected values, percentages, section presence, whitespace — must match.
3. Coverage matrix: **PE** and **SE**; **WGBS (dedup present)** vs **RRBS (no dedup → raw mode)**; **splitting present vs absent**; a **plot-excluded sample** (0 calls in a context) present in `.txt` but absent from graphs; **multi-sample** (≥2 rows, row-order); explicit-`@ARGV` vs auto-glob; `--title` with spaces; the **RRBS+WGBS-mix `die`** path.

**Timestamp determinism (DECIDED, mirrors `bismark2report`):** acceptance gate normalizes the one timestamp line (Perl `localtime` is unpinnable); committed Rust goldens use hidden `--__test_timestamp <UNIX_EPOCH>` formatted **in UTC** with Perl's exact scalar-`localtime` ctime layout (`Www Mmm DD HH:MM:SS YYYY`, space-padded mday). Self-consistent for Rust↔Rust; the gate's line-normalization bridges Rust↔Perl.

**⚠️ BOTH checked-in oracles in `docs/images/` are STALE — do NOT use them:**
- `bismark_summary_report.html` is **v0.15.2, Highcharts-era** (274 KB, **zero** `Plotly` tokens — predates the plot.ly rewrite). 
- `bismark_summary_report.txt` uses the **old `CpHs` column labels** (current source emits `chgs`).
The oracle is a **fresh run of the current Perl `bismark2summary v0.25.1`** on the same fixtures (auto-skip if `perl` absent). This is a worse staleness trap than `bismark2report`'s v0.19.1 HTML.

**Required fixtures (hand-built, tiny; rev 1 — expanded from dual plan-review). Goldens generated from Perl `v0.25.1`, committed (timestamp via `--__test_timestamp`):**
1. **Multi-sample WGBS** dir — ≥2 WGBS-PE (alignment + dedup + `deduplicated_splitting`) + 1 WGBS-SE → assert dedup-mode layout (raw sections deleted, dedup+dup kept) in **both** numbers and percentages.
2. **All-RRBS, ≥2 samples** (alignment + `_splitting`, no dedup anywhere) → the **only** way to byte-check the **raw-mode** HTML branch (numbers+percentages both raw; dedup/dup sections deleted; `{{p_aligned_replace}}` filled). The mixed dir below hits the `die`, so without this the raw branch is never exercised (Reviewer B 4.2).
3. **Single RRBS sample** → the **numbers/percentage divergence** (§2.9 ⚠ box): numbers in DEDUP layout, percentages in RAW layout (Reviewer A C2).
4. **Single WGBS sample** → consistent dedup layout; guards the `/^,{1,}$/`-needs-≥1-comma semantics (Reviewer A I1).
5. **Mixed RRBS+WGBS** dir → assert the `die` (line 1489).
6. **Plot-excluded sample in the MIDDLE** of the list (0 calls in one context) → present in `.txt`, absent from graphs; assert `{{num_samples}}`/`{{x_values_*}}` use the **total** count while `categories`/y-arrays use the **plotted** (smaller) count — the x(N)-vs-y(N−k) mismatch (Reviewer A I5 / B 4.3).
7. **All-excluded** (every sample missing a context) → zero plotted samples; empty joins → `^,{1,}$` false → both deletions take the dedup `else` branch; percentage loop doesn't iterate. Verify the Rust `^,{1,}$` equivalent returns **false** for `""` (Reviewer B 4.4).
8. **Mixed-case multi-sample auto-glob** dir (e.g. `apple_…`, `Mango_…`, `zebra_…`) → row order must follow Perl's **case-folded** glob sort; a bytewise `.sort()` would reorder and fail (spike / Reviewer A C1). **Mandatory** — the only fixture that catches a bytewise regression.
9. **Non-trivial `%.15g` tail** — pin a sample's CpG counts so `p_CpG_m` is e.g. `99.99` (→ unmeth `0.0100000000000051`) or `12.30` (→ `87.7`, trailing-zero drop), asserting the asymmetric unmeth formatting at the integration level, not just clean `.00`/`.50` (Reviewer B 4.1).
10. **`-o 0` / `--title 0`** → both fall back to defaults (truthiness; Reviewer A I3). **Explicit-`@ARGV` order** (verbatim, distinct from glob order). **`--title` with spaces** (verbatim injection, no escaping).

**Stale-oracle tripwire (Reviewer B 4.5):** a unit test that greps the committed `docs/images/bismark_summary_report.html` for `Plotly` and asserts **0 matches** — so the stale Highcharts oracle can never be silently re-adopted as the gate.

**Real-data validation (later, on `oxy`, `#[ignore]`):** run Perl + Rust on a real multi-sample Bismark output dir from `~/bismark_benchmarks`; diff `.txt` (raw) + `.html` (timestamp-normalized). Verify oxy env/paths first session; `rustup` not pre-installed → curl-install; build `--release`.

**NOT in the gate:** STDOUT/STDERR diagnostics; `--help`/`--version` text; the legacy Highcharts HTML.

---

## 8. Gotchas & candidate spikes

1. **STALE oracles (load-bearing).** Both `docs/images/` files are wrong (Highcharts HTML / `CpHs` txt). Generate fresh from current Perl. *(Biggest trap.)*
2. **`100 − sprintf("%.2f")` → `%.15g` stringification.** The six unmeth arrays drop trailing zeros (`"0"`, `"50"`, `"87.7"`), unlike the meth/alignment arrays. Reproduce via round-2dp→reparse→subtract→`format_g15` (§2.9a). **Candidate Spike A:** confirm `fmt_g::format_g15(100.0 - "<%.2f>".parse())` matches a fresh Perl run across the 0–100 range incl. `.00`/`.X0`/`0`/`100`.
3. **Asset line normalization (byte contract).** Replay `chomp`+`s/\r//g`+append-`\n` per line on the three assets; empty-input → empty guard (same helper shape as `bismark2report`).
4. **Inline-template byte fidelity.** The template is a single-quoted heredoc — lift lines 490–1371 verbatim (incl. all tabs/spaces, the HTML-comment placeholders, the trailing `</html>\n`). Guard with a source-extraction test. **Candidate Spike B:** extract the heredoc, run the asset injections, confirm the 3 markers (`plotly_goes_here`, logos) are unambiguous and `plot.ly` contains no live `{{…}}`.
5. **Greedy/dotall section deletion.** Six `{{…_section}}` markers deleted via `s/marker.*marker//s`. Implement as first-index … **last**-occurrence-of-the-second-marker splice (state "last", not "second", to future-proof against a template with >2 markers — they're the same today since each name occurs exactly twice). Mirror `bismark2report` §2.4.
5b. **⚠ Numbers vs percentage deletion are NOT a mirror (Reviewer A C2).** Numbers gate = `$dup_alignments =~ /^,{1,}$/` (1430); percentage gate = `if ($aligned)` (1577) — different variables, diverge for a single RRBS sample. See §2.9 ⚠ box. Also pin the **fill-then-delete order** (`{{aligned_seq}}` filled at 1419 *before* the raw-section deletion at 1430; `{{p_aligned_replace}}` gated `if($aligned)`) — preserve exact Perl statement order, do not reorder.
6. **Row-order = discovery order = Perl's CASE-FOLDED glob sort** (NOT bytewise; spike-confirmed macOS≡Linux, locale-invariant). Rust per-glob `(ascii_lowercased, original_bytes)` key, glob-order concat; argv path verbatim. Pin with the **mandatory mixed-case fixture** (§7.8) — a bytewise `.sort()` passes every same-case test and only corrupts row order on the first mixed-case cohort.
7. **`.txt` casing Perl-ism** `Methylated chgs`/`Unmethylated chgs` (lowercase). Copy verbatim.
8. **`.txt` raw-vs-mutated split.** `.txt` row captured at 404 (raw, blanks kept) **before** the 0-defaulting + aligned-blanking at 412–424 (which only feed plots). Easy to conflate.
9. **`aligned_reads` overwrite by dedup** (pre-dedup count) and **methylation overwrite by splitting** (`Total C to T conversions`). Order/precedence matters.
10. **Plot exclusion vs `.txt` inclusion** + the **`num_samples` (total) vs y-array (plotted) length mismatch** in x-values. Reproduce both.
11. **Latent CHH `total_CHG==0` bug** (1662) — dead for plotted samples but reproduce verbatim.
12. **RRBS+WGBS-mix `die`** (1489) and the all-commas `$aligned`/`$dup_alignments`/`$unique_alignments` mode detection (regex `^,{1,}$`).
13. **`unless ($x)` truthiness** for basename/title defaults (`-o 0` / `--title 0` → default). Edge; document.
14. **Replacement-string safety.** Perl `s/pat/$var/` inserts the value literally; Rust `.replace()` is literal. Confirm assets contain no live `{{…}}` (Spike B) and `--title`/values aren't re-interpreted.

**Candidate spikes (run if plan-review wants empirical confirmation; none blocks the SPEC):** Spike A (percentage `%.15g` parity), Spike B (asset embedding + heredoc extraction + marker unambiguity + Perl-oracle harness establishing the first goldens).

---

## 9. Scope for v1.0

The tool is small and the dual byte-identity gate exercises every path → **everything is v1.0**:

| Feature | Verdict |
|---|---|
| BAM discovery (4 globs + argv) + per-BAM report-name derivation | **v1.0** |
| Alignment / dedup / splitting parsers (PE+SE, overwrite precedence) | **v1.0** |
| `.txt` table (15 cols, raw values, row-order) | **v1.0** |
| `.html`: inline-template embed + asset inject + section deletion + `%.2f`/`%.15g` percentages + timestamp | **v1.0** |
| Plot-exclusion + raw-vs-dedup mode + RRBS+WGBS-mix die | **v1.0** |
| `-o/--basename`, `--title`, `--verbose`, `--version`, `--help/--man` | **v1.0** (text not byte-gated) |
| Hidden `--__test_timestamp` (UTC ctime) | **v1.0** (stable goldens) |

**Out of scope (all versions):** the legacy Highcharts HTML; byte-matching STDOUT/STDERR & help/version text; `Getopt::Long` `auto_abbrev`; re-rendering/upgrading plot.ly.

---

## 10. Phases (proposed — confirm before EPIC/PLAN)

Mirrors the dedup/c2c/genomeprep cadence (each phase merges to `rust/bismark2summary`; the branch later merges to `rust/iron-chancellor` only on an explicit "merge for me").

| Phase | Scope | Gate | Depends |
|-------|-------|------|---------|
| **A** | Workspace scaffold + crate (lib+bin) + clap `Cli`/`validate` + error enum + **BAM discovery** + report-name derivation + the **3 parsers** + the **`.txt` output**. | `.txt` **byte-identical** to Perl on the fixture matrix (the cheap early win). `--help`/`--version` boot. | — |
| **B** | The **`.html`**: embed inline template + 3 assets (+ normalizer) + all placeholder fills + raw/dedup section deletion + the `%.2f`/`format_g15` percentage engine + timestamp + hidden `--__test_timestamp`. | `.html` **byte-identical modulo the timestamp line** across the full §7 matrix. | A |
| **C** | **Real-data byte-identity gate** on oxy + RELEASE checklist + docs/CHANGELOG/README. | Perl≡Rust on a real multi-sample dir; gates the `bismark-summary-v1.0` tag. | A, B |

*(Spikes A/B optional, foldable into Phase A planning.)*

---

## 11. Open questions

| # | Question | Default / Resolution |
|---|----------|---------|
| O1 | `{{bismark_version}}` footer fill — hardcode `0.25.1` or use `env!("CARGO_PKG_VERSION")`? | **RESOLVED: hardcode `0.25.1`** — it is a literal constant in Perl (`$bismark_version`, line 25), not input-derived (Reviewer A confirmed; contrast `bismark2report`, where `{{bismark_version}}` IS input-derived — do not carry that pattern over). Crate version drives only the `--version` banner. |
| O2 | Promote the duplicated report parsers + `fmt_g` to a shared `bismark-io` module now, or after both HTML ports merge? | **After** (duplicate for v1.0 per Felix; note as future cleanup). Update the copied `fmt_g.rs` doc-comment to cite `bismark2summary §2.9a` not bedGraph internals (Reviewer B). |
| O3 | Real-data gate machine — oxy (per task) vs colossal? | **oxy** ([[reference_oxy_benchmark_env]]); verify env first session. |
| O4 | `--title`/`-o` value HTML-escaping (Perl injects verbatim, no escaping) | **Reproduce verbatim** (byte-identity; no escaping). |
| O5 | Glob sort order — bytewise (rev 0) vs case-folded (Reviewer A)? | **RESOLVED by spike: CASE-FOLDED** (fold-primary, raw-bytes-secondary), locale- & platform-invariant (macOS Perl 5.34.1 ≡ oxy/Linux Perl 5.38.2). Rust `(ascii_lowercased, original_bytes)`. See §2.3 / `SPIKE_glob_sort_order.md`. |

---

## 12. References
- **Perl source:** `bismark2summary` (v0.25.1, 1722 LOC) at the Bismark repo root.
- **Closest sibling:** `bismark2report` SPEC (`~/Github/Bismark-report/plans/06012026_bismark2report/SPEC.md`) — shared report-parsing + plot.ly assets + HTML-modulo-timestamp gate; **separate crate, not touched here**.
- **House style / byte-identity discipline:** `coverage2cytosine` SPEC (`plans/05292026_bismark-coverage2cytosine/SPEC.md`).
- **Rust patterns:** `bismark-dedup/src/{lib,cli,main,error,filename}.rs` (lib+bin scaffold), `bismark-bedgraph/src/fmt_g.rs` (`%.15g` formatter to copy), `bismark-extractor/src/logging.rs` (STDERR logger).
- **Memory:** `project_bismark2summary_port`, `project_bismark2report_port`, `project_coverage2cytosine_port`, `reference_oxy_benchmark_env`, `feedback_rust_ci_fmt_gate`, `project_rust_rewrite`.

## 13. Revision history
- **rev 0** (2026-06-01): initial draft. Grounded against Perl `bismark2summary` v0.25.1 (1722 LOC) + the `bismark2report`/`coverage2cytosine` SPECs + the dedup scaffold. Decisions locked with Felix: duplicate parsers (not couple to `bismark-report`); mirror the hidden `--__test_timestamp` flag. Awaiting manual review → dual plan-review.
- **rev 1** (2026-06-01): **dual plan-review (`SPEC_REVIEW_A.md` APPROVE-after-2-Criticals / `SPEC_REVIEW_B.md` APPROVE-WITH-CHANGES) + glob-sort spike folded in.** Both reviewers empirically confirmed the `%.15g` percentage engine bit-exact and both `docs/images/` oracles stale. Resolved Reviewer A's two Criticals: **C1 glob sort** (spike → case-folded, not bytewise — §2.3/§4.8/§8.6/O5/`SPIKE_glob_sort_order.md`) and **C2 numbers/percentage section-deletion asymmetry** (§2.9 steps 8–9 restated + ⚠ box; the two deletions key off different predicates and diverge for a single RRBS sample). Folded Reviewer B's fill-then-delete order pin, the independent-`if` dedup note, `.txt` col-1-is-raw-`$bam`, and the expanded §7 fixture matrix (mixed-case glob, single/two/all-RRBS, single-WGBS, all-excluded, plot-excluded-in-middle, non-trivial-`%.15g`-tail, stale-oracle tripwire). O1 (hardcode `0.25.1`) confirmed. Awaiting implementation trigger.
