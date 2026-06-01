# PLAN_REVIEW_B — `bismark-report` (Rust port of Perl `bismark2report`)

**Reviewer:** Plan Reviewer B (fresh context, independent)
**Date:** 2026-06-01
**Target:** `plans/06012026_bismark2report/PLAN.md` (DRAFT rev 0)
**Contract:** `plans/06012026_bismark2report/SPEC.md` (rev 1)
**Ground truth consulted (to falsify, not trust):** Perl `bismark2report` (1316 lines); `plotly/{plotly_template.tpl, plot.ly, bismark.logo, bioinf.logo}`; `rust/Cargo.toml` (+ `Cargo.lock`); `rust/bismark-genome-preparation/{Cargo.toml,src/*}`; `rust/bismark-dedup/src/{cli.rs,lib.rs,main.rs}`; `rust/bismark-extractor/src/logging.rs`.

**Bottom line:** This is a strong, well-scoped plan. The SPEC it implements is unusually thorough (rev 1 already absorbed two prior reviews), and the PLAN tracks it faithfully — the eleven-step orchestration, the three M-bias facts, the `is_some` gates, the nucleotide fixed-key order, and the byte-exact Unknown-context snippets are all present as concrete steps with matching fixtures. I verified the load-bearing template facts empirically and they hold (markers ×2, 12+12 mbias placeholders, no `{{` in `plot.ly`, no `\r` in any asset). The findings below are **mostly precision/precedent corrections, not design holes.** There is **one Critical correctness item** (the `--help`/`--man`/`--version` handling contradicts the precedent the PLAN claims to mirror and risks an exit-code/aliasing bug), and a handful of Important coverage/precedent gaps. No reason to block; fold these into rev 1.

---

## 1. Logic review

### 1.1 SPEC coverage diff — load-bearing items (the most important pass)

I enumerated every load-bearing SPEC item and checked for **both** a concrete PLAN step **and** a test/fixture. Result table:

| SPEC item | PLAN step | Test/fixture | Verdict |
|---|---|---|---|
| Orchestration order, 11 steps (§2.3) | §3 (1–11), C4 | C6 end-to-end golden | ✅ Present, order matches Perl 59–156 exactly |
| Plot.ly / logo greedy splice + `die` if absent (§2.3.2–4) | C1 | C6 (synthetic), E2/E3 | ✅ |
| Timestamp fill at step 5 (§2.6) | A6, C4 | A7, E2 normalization | ✅ |
| Alignment gate = `is_some` on 5 fields, `0` passes (§2.7a, Perl 378) | B1 | B6 (`0`-through-gate), E1(4) | ✅ |
| Gate-fail → placeholders survive, exit 0 (§5.4, §6.1) | B1, D3 | E1(3)/E4(3) | ✅ |
| Unknown-context snippet exact bytes (§2.7a, Perl 433–448) | B1 | B6, E2 (Bowtie2 fixture) | ✅ Bytes correctly cited (5/32sp; 4sp+4tab; 4sp+3tab) |
| Percent N/A → table `N/A`, graph `0` (Perl 469–498) | B1 (parenthetical) | — | ⚠️ Step present, **no dedicated fixture** (see 1.3) |
| Dedup `\s.*` trim on dups/diff_pos (§2.7b, Perl 530/535) | B2 | B6 | ✅ |
| Dedup leftover fallback `total−dups`, signed (§2.7b) | B2 | B6, E1(5) | ✅ |
| Dedup gate `is_some` on 4 (Perl 551) | B2 | B6 | ✅ |
| Splitting phrasing differences (no `Total unmethylated` alt; `Unknown context:` w/o `(CN or CHN)`) (§2.7c) | B3 | B6 | ✅ Correctly distinguished from alignment |
| Splitting gate `is_some` on 6 meth/unmeth (Perl 784) | B3 | B6 | ✅ Note: gate is on the **6 meth/unmeth fields only**, NOT `total_C_count`/`perc_*` (see 1.2) |
| M-bias FACT 1: `$state`-driven `<div>` deletion (absent→both, SE→excise R2, PE→collapse R2 markers) | C3, §3(9) | C6 matrix, E4(1–2) | ✅ |
| M-bias FACT 2: `%mbias_2`-driven R2 fill (can diverge from `$state`) | B4 | B6 ("mbias state + empty-R2") | ✅ Edge captured |
| M-bias FACT 3: 24 data placeholders survive outside spans (absent→24, SE→12) | B4, C3, §3(9) | C6, E4(1–2) | ✅ Verified empirically: 12 mbias1 + 12 mbias2, all at template L843–1060, **outside** the section spans at L465–496 |
| Dead `{{bm_mbias_2}}`→`false` no-op (Perl 1016) | B4 | — | ✅ Verified: 0 occurrences in template; correctly a no-op |
| Nucleotide line-0 header validation (Perl 587–600) → error | B5 | B6, E1(6)/E4 | ⚠️ See 1.4 — PLAN over-specifies (col 3 AND col 5); Perl checks col 3 then col 5 with **two separate `die`s** |
| Nucleotide fixed 20-key order | B5 | B6 | ✅ Order matches Perl 632 exactly |
| Nucleotide missing key → `0`/empty (§2.7e, #711) | B5 | B6, E1(6)/E4(6) | ✅ — but see 1.4 on the exact undef semantics |
| Nucleotide log2 ratio computed-but-not-emitted | B5 | — | ✅ Step says "do NOT output any float"; no fixture needed (negative assertion belongs in golden) |
| Nucleotide plot separators ` , ` (x) vs `','` (y) | B5 | B6 (implied) | ⚠️ No **explicit** separator-byte assertion (see 1.3) |
| Sequential whole-doc subst order / cross-call re-subst (§8) | B (mod.rs), §11 | — | ⚠️ Claimed but **no test** pins the order; low risk here (see 1.5) |
| `normalize()` faithful + empty→`""` guard (§2.6/§8.2) | A5 | A7 (empty case) | ✅ |
| Timestamp UTC hook + gate normalization, assert exactly one match (§7) | A6, E2 | A7, E2 | ✅ Good — the single-match assertion guards against masking |
| Exit codes: help/man/version→0; errors→nonzero; missing-field→0 (§6.1) | A2, D2, D3 | A7, D4 | ⚠️ **Mechanism is wrong/imprecise — see Critical 1.6** |
| Line-1256 companion reset (multi-report) | D1 | D4, E1(7)/E4(7) | ✅ |
| `-o` verbatim + `>1` report → error (§2.5, Perl 1128–1132) | D2 | D4 | ✅ |
| `--dir` trailing `/` unless empty (Perl 1093–1099) | D2 | D4 | ✅ |

**Net:** every load-bearing item has a step. The gaps are (a) the help/version mechanism (Critical), and (b) a few behaviors that have a step but **no explicit assertion** — these can silently regress inside a big golden without a targeted test pointing at them (Important).

### 1.2 Splitting gate writes ungated fields — verify the PLAN preserves Perl's asymmetry
Perl `read_splitting_report` gates on the **6 meth/unmeth fields** (line 784) but, *inside* the passed block, also writes `{{total_C_count_splitting}}` (788) and the `perc_*` table cells (874–876) **without** themselves being in the gate. So a report with all 6 meth/unmeth present but missing `Total number of C` fills `{{total_C_count_splitting}}` with `undef` → the empty string. Same asymmetry exists in alignment (`total_C_count` is filled at Perl 409 but not in the 5-field gate at 378). PLAN B1/B3 say "gate `is_some` on N fields" but do **not** spell out that the *fill set is larger than the gate set* and that ungated-but-filled fields can legitimately be empty/undef. This is a real byte path (a `Total number of C` line absent from an otherwise-complete report). **Add a one-line note + a fixture** (alignment/splitting report with the meth/unmeth lines present but `Total number of C` absent → `{{total_C_count*}}` fills empty, gate still passes).

### 1.3 N/A-percentage and separator behaviors have steps but no targeted assertion
- **Percent N/A** (Perl 414–498 / 798–871): the table cell shows `N/A` while the graph string shows `0`. PLAN B1 mentions this parenthetically ("N/A→`0` in graph only; table shows `N/A`") but no fixture asserts it. A report with methylated/unmethylated counts present but a `C methylated in CpG context:` line **absent** is the trigger (`perc_CpG` undef → `N/A` in table, `0` in `{{cytosine_methylation_plotly}}`). Worth one fixture; otherwise this only gets exercised if a golden happens to omit a percent line.
- **Nucleotide separators** ` , ` vs `','` (Perl 675–679): SPEC §8.8 calls this "easy to get wrong." PLAN B5 lists the separators but B6 has no explicit "assert x-array uses ` , ` and y-array uses `','`" assertion. The full-nuc golden covers it transitively, but a 2-line unit assertion is cheap insurance and makes a regression point straight at the cause.

### 1.4 Nucleotide header validation — PLAN slightly over-/mis-specifies vs Perl
PLAN B5 says: "line-0 header validation (col 3 == `percent sample`, col 5 == `percent genomic`, else error)." Perl 587–600 does **two independent checks with two distinct `die` messages** (col 3 first; if it passes, col 5). Functionally equivalent for the gate, but: (a) the two error messages differ and the SPEC §6.1 only requires "nonzero exit" (not byte-matched), so this is fine — but the PLAN should say "either check fails → error" rather than implying a single combined predicate. (b) More importantly, the **missing-key undef semantics** are subtler than "`0`/empty": in Perl, for a key absent from `%nucs`, `$nuc_obs`/`$nuc_exp` are explicitly defaulted to `0` (lines 641–646), but `$counts_obs`/`$counts_exp`/`$cov` are **never defaulted** — they're `undef`, and the `s///` inserts the empty string. The PLAN B5 captures this ("percentages `0`, counts/coverage empty string") — good — but note the **plot arrays** push `$nuc_obs`/`$nuc_exp` (the `0`-defaulted percentages), so a missing key contributes a literal `0` into the ` , `-joined x-arrays and its key letter into the `','` y-array. The amplicon fixture (#711) must assert the **plot-array** bytes too, not just the table cells. PLAN E1(6)/E4(6) say "absent keys render 0 for percentages but empty string for counts/coverage" — extend to "and a `0` appears in the nucleo_*_x plot arrays for that key."

### 1.5 Sequential substitution order — claimed safe, but worth a guard
PLAN §11 and B (mod.rs) assert Perl's `s///g` cross-call re-substitution semantics are preserved by doing `doc.replace(...)` in a fixed order. I checked whether any *captured value* could contain a `{{name}}` substring that a later `replace` would re-hit: report values are numbers, human labels, strand tokens, and the input filename + bismark version (from `Bismark report for: X (version: Y)`). A pathological filename containing `{{...}}` is the only realistic re-subst hazard, and Perl would have the same behavior (so byte-identity holds regardless). **Conclusion: genuinely low-risk and Perl-faithful** — but since the PLAN explicitly leans on "order is fixed," add one negative test (a value that itself looks like a later placeholder must produce the *same* bytes as Perl) so the claim is pinned rather than asserted. Optional.

### 1.6 (CRITICAL) `--help` / `--man` / `--version` mechanism contradicts the cited precedent
PLAN A2: *"`--man` as a visible alias of `--help`. clap handles `--help`/`--version` → exit 0 (SPEC §6.1; do NOT reproduce Perl's exit-1-on-help)."* This is **wrong on two counts** when checked against the precedent the PLAN claims to mirror:

1. **`--version` is NOT clap-auto-handled in this workspace.** Both `bismark-dedup` (`src/cli.rs:419`, `disable_version_flag = true`; `src/main.rs:32` manual `if cli.version { println!(version_string()); return ExitCode::SUCCESS; }`) and `bismark-genome-preparation` (`src/cli.rs:41` `disable_version_flag = true`; `src/main.rs` manual handling) **disable clap's version flag** and handle it manually so they can print the Bismark provenance banner. The PLAN's A3 *does* say "`version_string()` via `env!`" — but A2's "clap handles `--version`" directly contradicts A3 and the precedent. **Decide and state explicitly:** mirror dedup/genomeprep — `disable_version_flag = true`, a `version: bool` field, manual `println!(version_string())` + `ExitCode::SUCCESS` in `main.rs`.

2. **`--man` cannot be a clap "alias of `--help`."** Clap's built-in `--help` is not aliasable. The genomeprep precedent (`src/cli.rs:98` `pub man: bool` + `src/main.rs` `if cli.man { Cli::command().print_long_help(); println!(); return ExitCode::SUCCESS; }`) handles `--man` as a **separate bool field** dispatched in `main`. Also note Perl combines them in **one** GetOptions key `'help|man'` (line 1055) — so in Perl `--help` and `--man` are literally the same flag. The Rust equivalent is a `man: bool` handled in main that prints the long help, exactly as genomeprep does. **Fix A2** to: "`--man` is a separate `man: bool` field; `main` prints long help and exits 0 (mirror genomeprep `main.rs`), and clap's auto-version is disabled with `version` handled manually."

This is Critical not because the *bytes* are gated (help/version text is explicitly out of the gate, §7) but because the PLAN's stated mechanism would not compile/behave as written and silently diverges from the two precedents it names. Exit codes (help/man/version → 0) are correct as a *goal*; only the *mechanism* is mis-stated.

### 1.7 `collapse`/`excise` whitespace survival is correct but under-asserted
I dumped the exact bytes of the four mbias marker lines:
- L465/L480: `\t{{mbias_r1_section}}\n`
- L483: `\t{{mbias_r2_section}}\t\n`  ← note the **trailing tab** after the first R2 marker
- L496: `\t{{mbias_r2_section}}\n`

PLAN C2 defines `collapse = doc.replace(marker, "")` (removes only the literal token) and `excise = first-index … last-index-end splice`. Both are **byte-correct** against Perl's `s/\{\{marker\}\}//g` and `s/\{\{marker\}\}.*\{\{marker\}\}//s`: collapse leaves the surrounding `\t…\n`; excise leaves the leading `\t` before the first marker and the `\n` after the last marker, deleting everything between. Good. **But** the surrounding-whitespace survival (the leftover `\t` and the L483 trailing `\t`) is exactly the kind of thing a naive "trim the line" implementation would get wrong. PLAN C6 should add an explicit byte assertion that, after collapse, the `\t`/`\n` around each marker **survive** (not just "no marker residue"). Cheap, and it nails SPEC §2.4 / §8.3.

### 1.8 Phase ordering / buildability — sound, with one caveat
A→B→C→D→E→F is a valid dependency DAG: A (scaffold + assets + timestamp), B (pure parsers, unit-testable against literal strings with **no** dependency on the template — `fill(doc, c)` takes a `doc: String`, so B6 can feed a synthetic mini-doc and assert substitutions), C (assembly needs A's normalize + B's fills), D (discovery needs C's `build_report`), E (gate needs all), F (real-data/docs). **Phase B parsers *can* be tested before Phase C** because the parse/fill split makes `fill` independent of the real 29 KB template (feed a stub doc). This is correct and well chosen. One caveat: B4's `fill` returns `(State, String)` while B1/B2/B3/B5 return `String` — the signature divergence (§5) is real and fine, but `build_report` (C4) must thread `state` into the **step-9 deletion** (C3), and the PLAN does say so. ✅ See 3.1 for the threading detail.

### 1.9 Dependency set (A1) — partially incorrect vs the lock
PLAN A1 deps: `clap` derive (workspace version), `anyhow`, `thiserror`, **no** `flate2`/`noodles`/`bismark-io`. I verified:
- `clap = "=4.5.30"`, `thiserror = "=2.0.0"`, `anyhow = "=1.0.86"` are the workspace pins (present in sibling Cargo.tomls). ✅ Dropping flate2/noodles is correct (plain-text reports). ✅
- **`chrono` / `time` / `glob` are NOT in `Cargo.lock`.** `grep '^name = "(chrono|time|glob)"' Cargo.lock` → **zero hits.** PLAN A6 and §10 say "pin to workspace lock — confirm in A6," and SPEC §10 lists this as "Open (non-blocking)." But the framing "(or can be) pinned in the workspace lock" understates it: **adding `chrono`, `time`, or `glob` is a brand-new dependency tree**, not a re-pin of something already vendored. For a byte-identity tool the timestamp formatting is just `sprintf("%04d-%02d-%02d"...)` on Y/M/D/H/M/S integers — **this can be done with `std` alone** (`SystemTime` for local is awkward, but the **only non-test path is local time**, and the **test/golden path is UTC from an explicit epoch**, which is trivial integer arithmetic — days-since-epoch → civil date via Howard Hinnant's algorithm, ~15 lines, no crate). **Recommendation:** prefer a tiny `std`-only UTC epoch→(date,time) for `--__test_timestamp` (deterministic, zero new deps) and use `chrono`/`time` *only* if local-time formatting genuinely needs it. If a crate is added, it must be a real new lock entry — call that out as a decision, not a "confirm." Likewise `glob` is unnecessary: D1 already offers `std::fs::read_dir` + suffix filter, which is sufficient and dep-free.

### 1.10 Workspace wiring (A1) — confirmed sufficient
`rust/Cargo.toml` is a `[workspace]` with `members = [...7 crates...]`, `resolver = "2"`, `edition = "2024"` (workspace.package), `rust-version = "1.89"`. Adding `"bismark-report"` to `members` is indeed all that's needed; the new crate inherits `edition.workspace`/`rust-version.workspace` via the genomeprep Cargo.toml pattern. ✅ The PLAN's "edition 2024 / rust 1.89" assumptions hold.

---

## 2. Assumptions

- **"Reports are ASCII / read as `&str`" (§8 assumption 1, PLAN §8).** Mostly safe, but **not guaranteed.** The `{{filename}}`/`{{bismark_version}}` come from `Bismark report for: X (version: Y)` where `X` is a user-supplied **input filename** that can contain non-ASCII / non-UTF8 bytes on some filesystems. Perl reads bytes and substitutes bytes — it never decodes UTF-8 — so a non-UTF8 filename round-trips fine in Perl but would **panic or lossy-convert** if Rust does `read_to_string` (`String` requires valid UTF-8). For *true* byte-identity on a pathological filename, the report bodies should be read as **`Vec<u8>`/`&[u8]`** and substitutions done on bytes, OR the PLAN should explicitly accept "non-UTF8 report content is out of scope / will error" as a documented divergence. The assets are verified UTF-8/ASCII (no `\r`, no `{{`), so those are fine as `&str`; the risk is purely the *input reports*. **Recommendation:** document the decision. Reading reports as `String` is pragmatic and matches the other ports' real-world inputs (Bismark reports are ASCII), but it is an *assumption with a failure mode*, not a fact — flag it as such rather than asserting "input reports are ASCII."
- **`include_str!` "mirrors genomeprep" — FALSE precedent.** I grepped the whole workspace: **no crate uses `include_str!` or `include_bytes!`.** genomeprep ships **no** embedded data assets (it generates converted FASTA at runtime). So the PLAN's repeated "same shape as genome-preparation" / "mirror genomeprep data shipping" framing for asset embedding has **no precedent to mirror** — this is a genuinely new pattern for the workspace. The PLAN's A5/§10 honestly flag the include path as "unsettled," which is good, but the "mirror genomeprep" justification is unsupported. Decide the path strategy on its own merits (recommend `include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/..."))` with the four files **copied into `rust/bismark-report/assets/`** so the crate is relocatable and doesn't reach `../../../plotly/` outside its own tree). The 3 MB binary inflation is correctly judged acceptable.
- **"Glob sort order low-stakes" (§8.8, PLAN §8).** Agreed and correct: multiple alignment reports each produce an independent file (order affects only STDOUT, which is not gated), and >1 companion match `die`s rather than depending on order. The PLAN reproduces a lexical sort anyway — fine, dep-free via `read_dir` + `sort`.
- **Stale `bismark_bt2_PE_report.html` never used as oracle.** Correctly flagged as the single biggest trap (SPEC §8.1, PLAN §8/§11). The oracle is a fresh Perl run. ✅
- **`{{`-free / `\r`-free assets (A7).** I verified empirically: `plot.ly` has 0 `{{` tokens, both logos have 0 braces, **all four assets have 0 `\r` bytes.** So the literal-splice / literal-subst safety holds **today**. The A7 assertion is still worth keeping as a regression guard against a future asset refresh. Note: the empty-input `normalize` guard is therefore only *theoretically* needed (none of the four assets is empty) — but the helper is generic, so the guard + test are correct to include (SPEC §8.2). ✅

---

## 3. Module / signature soundness (§2.1 / §5)

### 3.1 `build_report` threading of M-bias `state` — present but make it explicit
The trickiest wiring: B4's `fill` returns `(State, String)`; C4's `build_report` must (i) run R1 marker collapse, (ii) call mbias fill to get `(state, doc)`, then (iii) at **step 9** use `state` to drive the R2 `<div>` deletion (SE→excise, PE→collapse), independent of whether R2 *data* filled. PLAN C3 + §3(9) describe this correctly, and §5 shows `fn build_report(aln, dedup, split, mbias, nuc, test_epoch)`. **Gap:** the signature in §5 doesn't show *where the absent-M-bias branch* lives. When `mbias` is `None`, `read_mbias_report` never runs → **both** R1+R2 blocks excised AND all 24 placeholders survive. The PLAN handles this in C3/§3(9) prose, but the §5 signature sketch should note that `build_report` owns the present/absent branch for *every* optional section (dedup/split/mbias/nuc), not the parsers — i.e. the collapse-vs-excise decision is orchestration-level, the fill is parser-level. This matches Perl (the `if($mbias_report){...}else{...}` lives in the main loop, lines 117–138, not in `read_mbias_report`). Make that ownership boundary explicit so an implementer doesn't push the excise logic into `mbias::fill`.

### 3.2 `fill` ordering — captured only implicitly
SPEC §8 stresses the sequential whole-doc substitution order matters (cross-call re-subst). The PLAN says "in Perl's order" but **no module owns or documents the canonical order**. For safety, the per-parser `fill` should emit substitutions in the **same source order as the Perl `s///g` sequence** (e.g. alignment: Perl 382→498), and a comment should anchor each `replace` to its Perl line. This is a documentation/discipline gap, not a logic hole (1.5 showed the practical risk is near-zero), but for a byte-identity port the order should be *pinned in code comments + one negative test*, not left to "in Perl's order."

### 3.3 Multi-report parallel-slot building — has a home
D1/D3 build "per-report slots" and loop `build_report`. This mirrors Perl's five parallel arrays (`@alignment_reports` etc.) + `while(@alignment_reports){ shift ... }`. The line-1256 reset is correctly assigned to D1. ✅ One nit: the PLAN should state the slot type — a `Vec<ReportSet>` where `ReportSet { aln: PathBuf, dedup: Option<PathBuf>, ... }` is cleaner than five parallel `Vec`s and avoids index-desync bugs (Perl's parallel-array shape is an artifact, not a contract). Optional refactor.

---

## 4. Efficiency

Non-hotspot, correctly judged (PLAN §6). One report → one ~3 MB `String`, `str::replace` allocations are fine. The only note: each `doc.replace(marker, "")` reallocates the full 3 MB doc, and there are ~50+ substitutions per report → ~150 MB of transient allocation per report. **Completely irrelevant** for an interactive QC tool (sub-millisecond), and matching Perl's `s///g` which also rebuilds the string. No `mimalloc`, no parallelism — correct. No action.

---

## 5. Validation sufficiency

The §9 + Phase-E matrix is strong and targets the right risks (asset normalization, `0`-through-gate, M-bias survival counts, greedy excise, Unknown snippets, nucleotide missing-key + header, Perl-oracle PE/SE, exit codes, real-data). The auto-skip-if-no-perl oracle pattern (E2) is sound and matches methcons/genomeprep. The timestamp normalization with **"assert exactly one match per file"** (E2, SPEC §7) is the right guard — it prevents a stray timestamp-shaped string from masking a real divergence. **Gaps (all Important-or-below):**

1. **No fixture for percent-`N/A` table-vs-graph split** (1.3) — a real byte path, untested.
2. **No explicit nucleotide-separator assertion** (1.3) — covered only transitively by the full-nuc golden.
3. **No fixture for ungated-but-filled fields** (`total_C_count` absent, gate still passes, field fills empty) (1.2).
4. **No assertion that `collapse` leaves surrounding `\t`/`\n`** (1.7) — the L483 trailing-tab case is a precise byte the golden covers but no test points at.
5. **Could the gate mask a divergence?** The timestamp normalization is well-anchored (exact line `Data processed at HH:MM:SS on YYYY-MM-DD`, single-match assertion). The one residual risk: if the **Rust golden's** `--__test_timestamp` UTC formatting disagrees with the **Perl** `localtime` *format* (not value), the normalization regex must match *both* shapes. Since both use `%02d:%02d:%02d` / `%04d-%02d-%02d`, the *shape* is identical and the regex is shape-anchored — safe. ✅ No masking risk found.
6. **`--man` / `--help` / `--version` exit-code tests** (A7/D4) — keep these, and given the Critical 1.6 mechanism fix, add an explicit test that `--man` prints long help and exits 0 (genomeprep has exactly this test pattern to copy).

The real-data gate (F1, oxy, `#[ignore]`) is the right closer. ✅

---

## 6. Alternatives / efficiency (low-cost reconsiderations)

1. **Drop `chrono`/`time`/`glob` for `std`-only** (1.9). The deterministic golden path is pure integer arithmetic (epoch→UTC civil date, ~15 lines, well-known algorithm); local-time-only-for-non-gated-runtime can use `std::time::SystemTime` + a minimal civil conversion, or — since local time is *never gated* — even a small crate is acceptable but unnecessary. Globbing via `read_dir` is already in the PLAN (D1). **Net: zero new dependencies is achievable and keeps the lock clean** (consistent with the "no flate2/noodles" minimalism the PLAN already embraces). At minimum, reframe §10's "confirm in A6" as "decide: std-only vs new lock entry."
2. **`ReportSet` struct over five parallel Vecs** (3.3) — cheap clarity win, removes an index-desync footgun. Optional.
3. **Read report bodies as bytes** (§2 assumption) — only if non-UTF8 filename byte-identity is in scope; otherwise document the `String` assumption + its failure mode. Low cost to decide now.

---

## 7. Action items (prioritized)

### Critical (fix before implementation)
- **C1. Correct the `--help`/`--man`/`--version` mechanism (PLAN A2/A3).** `--version` is **not** clap-auto-handled in this workspace — both dedup (`cli.rs:419`/`main.rs:32`) and genomeprep (`cli.rs:41`/`main.rs`) use `disable_version_flag = true` + a manual `version: bool` handled in `main` printing `version_string()` then `ExitCode::SUCCESS`. `--man` is **not** a clap alias of `--help`; it must be a separate `man: bool` field dispatched in `main` (`Cli::command().print_long_help(); println!(); ExitCode::SUCCESS`), exactly as genomeprep does. Perl combines them as one `'help|man'` GetOptions key (line 1055). The exit-code *goals* (all → 0) are right; only the *mechanism* and the "mirror" claim are wrong. (Ref: SPEC §6.1, §6.2; Perl 1055, 1073–1090, 1314.)

### Important (fold into rev 1)
- **I1. Drop / re-decide `chrono`/`time`/`glob` (PLAN A1/A6/§10).** Verified absent from `Cargo.lock` — these are *new* dependency trees, not re-pins. The deterministic UTC golden path is `std`-only integer arithmetic; globbing is already `read_dir`-based (D1). Reframe as an explicit decision (prefer zero new deps), not a "confirm." (Ref: SPEC §3 reuse table, §10.)
- **I2. Add fixture: percent-`N/A` table-vs-graph split** (alignment report missing a `C methylated in CpG context:` line → table cell `N/A`, `{{cytosine_methylation_plotly}}` shows `0`). Currently a parenthetical with no test. (Ref: SPEC §2.7a; Perl 414–498.)
- **I3. Add explicit nucleotide separator + missing-key plot-array assertions** (B6/E4): assert x-arrays use ` , `, y-arrays use `','`, and that a #711 missing key contributes a `0` to the `nucleo_*_x` arrays (not just empty table cells). (Ref: SPEC §2.7e/§8.8/§8.9; Perl 663–690.)
- **I4. Spell out the ungated-but-filled-fields asymmetry** (B1/B3) + a fixture: alignment/splitting report with the gated fields present but `Total number of C` absent → `{{total_C_count*}}` fills empty, gate still passes. (Ref: Perl 378+409 / 784+788.)
- **I5. Decide & document the input-report encoding assumption** (PLAN §8). Either read report bodies as bytes for true byte-identity on non-UTF8 filenames, or document `String`/UTF-8 as an accepted assumption with its failure mode. Assets stay `&str` (verified clean). (Ref: SPEC §2.7a `{{filename}}`.)
- **I6. Correct the "mirror genomeprep" framing for asset embedding** (PLAN §2/A5/§8). No workspace crate uses `include_str!`; genomeprep ships no embedded assets. Decide the include path on its own merits (recommend copying the four files into `bismark-report/assets/` + `include_str!(concat!(env!("CARGO_MANIFEST_DIR"), ...))`, not `../../../plotly/`). (Ref: SPEC §4.1.)

### Optional (nice-to-have)
- **O1. Assert `collapse` leaves surrounding `\t`/`\n`** in C6 (the L483 trailing-tab byte) — points a test directly at SPEC §2.4/§8.3 rather than relying on the big golden. (Verified marker bytes: L483 = `\t{{mbias_r2_section}}\t\n`.)
- **O2. Pin the substitution order in code comments + one negative re-subst test** (3.2/1.5) — the practical risk is near-zero, but for a byte-identity port the "in Perl's order" claim should be anchored to Perl line numbers in comments.
- **O3. Use a `ReportSet` struct instead of five parallel Vecs** (3.3) — removes an index-desync footgun; Perl's parallel arrays are an artifact, not a contract.
- **O4. Clarify nucleotide header validation wording** (B5): "either col-3 or col-5 check fails → error" (Perl has two separate `die`s), and that the messages aren't byte-gated.

---

## 8. Where the plan is sound (brief)
- 11-step orchestration order — matches Perl 59–156 exactly.
- M-bias three facts (state-driven deletion / `%mbias_2`-driven fill / 24 surviving placeholders) — present and **empirically verified** (12+12 placeholders at L843–1060, sections at L465–496, markers ×2, `bm_mbias_2`×0).
- `is_some` gates (not truthiness), `0`-through-gate fixtures — correct and tested.
- Greedy/dotall excise + collapse semantics — byte-correct against Perl `s///s` / `s///g`.
- Unknown-context snippet byte counts — correctly cited.
- Nucleotide fixed 20-key order — matches Perl 632.
- Timestamp UTC hook + single-match normalization gate — sound, no masking risk.
- Phase A→F dependency order + parse/fill split enabling B-before-C testing — valid.
- `clap`/`anyhow`/`thiserror` pins, dropping flate2/noodles, workspace `members` wiring — correct.
- Stale-HTML trap, oracle-from-fresh-Perl, auto-skip-if-no-perl — all correctly handled.
- §10's two "Open" items are genuinely non-blocking *as risks*, though the dependency one (I1) deserves a firmer decision than "confirm."

---

**Verdict:** APPROVE WITH CHANGES. One Critical (the help/version mechanism contradicts the cited precedents and won't behave as written), six Important coverage/precedent corrections, four Optional. No design-level holes; the SPEC↔PLAN coverage is otherwise complete. Fold the items into PLAN rev 1 and proceed to the implementation trigger.
