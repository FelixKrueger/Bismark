# Phase A PLAN — Plan Review A

**Reviewer:** Plan Reviewer A (independent, fresh context)
**Target:** `phase-a-scaffold-cli-genome/PLAN.md` (rev 0)
**Contract:** byte-identical to Perl `coverage2cytosine` v0.25.1
**Ground truth checked:** Perl `process_commandline` (1990–2197), `read_genome_into_memory` (1648–1739), `extract_chromosome_name` (1741–1751), `handle_filehandles` (89–165), `print_context_summary` (63–78); Rust `bismark-dedup/{cli,error,main,lib}.rs`, `bismark-io/cram_ref.rs`; `noodles-fasta 0.61.0` source; workspace `Cargo.toml` + `Cargo.lock`.

**Verdict: APPROVE WITH CHANGES.** The plan is faithful, well-grounded, and the highest-value claim (Deviation D1: `HashMap` over `IndexMap`) is **sound** for Phase A. No Critical correctness defects in the planned scope. Two Important issues should be fixed before implementation (a genome-reader edge case Perl handles that the plan does not name; and a missing `output_stem` strip-order subtlety), plus several validation-coverage gaps. The open questions the plan flagged as "verify in impl" are now mechanically resolved by this review (noodles semantics) — fold the answers in.

---

## 1. Logic review

### 1.1 CLI validation order vs Perl `process_commandline` — faithful, with caveats

I walked the plan's §3.2 rejection ladder against Perl 1990–2196 line by line:

| Plan §3.2 step | Perl line | Faithful? |
|---|---|---|
| 1. `version` short-circuit | 2042–2056 | ✅ (handled in `main`, like dedup) |
| 2. v1.x flags rejected | n/a (new) | ✅ correct design (P9) |
| 3. missing `-o` → `MissingOutput` | 2077 | ✅ |
| 4. missing `-g` → `MissingGenomeFolder` | 2134 | ✅ (mouse default dropped, SPEC §15) |
| 5. `merge && cx` | 2140 | ✅ |
| 6. `merge && split` | 2143 | ✅ |
| 7. `merge && threshold.is_some()` | 2176 | ✅ |
| 8. `disco && !merge` | 2165 | ✅ |
| 9. `disco` range `1..=100` | 2168 | ✅ |
| 10. `threshold == Some(0)` error; `None ⇒ 0` | 2178 | ✅ — see 1.2 |
| 11. resolve `cpg_only`, `threshold`, `output_stem`, dirs | 2112-2115, 2185 | ✅ logic; see 1.4 for a stem subtlety |

**The `Some(0)` vs `None` distinction is correct** against Perl. Perl 2174–2186: `if (defined $threshold){ ... unless($threshold>0){die} } else { $threshold = 0 }`. So an *explicitly supplied* `0` (or any non-positive) dies; *absence* resolves to `0` = report-all. The plan's `Option<u32>` with `Some(0)→ThresholdNotPositive` and `None→unwrap_or(0)` reproduces this exactly. **Good — this is the subtle one and the plan got it right.**

**`cpg_only = !cx_context` is correct** (Perl 2112-2115: `unless($CX_context){ $CX_context=0; $CpG_only=1 }`). When `--CX` is set, `$CpG_only` stays `undef` (falsy). `!cx` reproduces both branches.

**Caveat — Perl ordering divergence (acceptable, but document it).** The plan's ordering is *not* byte-for-byte Perl's ordering, and that is fine because STDERR isn't gated and only the *first* error fires. But two reorderings are worth a one-line note in the plan so a future reader doesn't "fix" them:
- Perl checks **missing `-o` (2077) BEFORE the genome folder (2119/2134)** and BEFORE the merge/disco/threshold block (2138+). The plan's order (output → genome → merge → disco → threshold) matches Perl's *textual* order. ✅ Actually consistent — no action, just confirming.
- Perl evaluates `disco` validity (2164-2171) **before** the threshold block (2173-2186). The plan puts disco (steps 8-9) before threshold (step 10). ✅ Matches.

So the ordering is actually faithful to Perl's textual order. No divergence. Good.

### 1.2 `discordance` range type mismatch (minor logic risk)

Perl: `'discordance_filter=i' => \$disco` then `unless ($disco > 0 and $disco <= 100)` (2168). Perl `=i` accepts **any signed integer**, including negatives and values > 255; the `>0 and <=100` test rejects them. The plan types `discordance: Option<u8>` (§3.1) and checks `1..=100` (§3.2 step 9). **A `u8` cannot represent a negative input** — clap will reject `--discordance_filter -5` at *parse* time (exit 2) rather than at `validate` (exit 1) with `DiscordanceOutOfRange`. Perl would reach the `die` (its own exit). This is a **behavioral divergence in error path only** (both reject; different exit code + message channel). Since STDERR/exit-message parity isn't gated and clap's parse error is arguably friendlier, this is acceptable — but the plan should **state it explicitly** so the V4 test for "discordance range" knows that `0` is the only in-`u8` out-of-range value testable via `validate()` (and `101..=255` — `u8` max 255 — also reach `validate`). The negative case is a clap-parse test, not a `validate` test. **Name this in the plan** (currently §3.2 step 9 implies all out-of-range hits `validate`).

### 1.3 Genome reader quirks — mostly faithful; ONE missing edge case

Checked each claimed quirk against Perl 1648-1751:

1. **Glob priority, first-non-empty-wins, no union** (plan §3.3.1) — ✅ matches Perl 1654-1669 (`<*.fa>`; `unless(@…){ <*.fa.gz> }` …). The "do NOT union" is the right reading of the nested `unless`.
2. **Mus skip** (plan §3.3.2) — ✅ Perl 1678 `next if (... eq 'Mus_musculus.NCBIM37.fa')`. **BUT see the IMPORTANT edge case below** — the skip happens *after* tier selection, which the plan's structure does not surface.
3. **First-whitespace-token name** (plan §3.3.4) — ✅ Perl 1745 `split /\s+/`. **Now mechanically confirmed**: noodles `Definition::from_str` (definition.rs) does `line.splitn(2, char::is_ascii_whitespace)` and takes component 0 as `name`. So `record.name()` IS up-to-first-ASCII-whitespace. **The plan's open question (§10 row 1) is RESOLVED: noodles `record.name()` is correct; the manual-split fallback is NOT needed.** One nuance below (1.6).
4. **Uppercase** (plan §3.3.5) — ✅ Perl 1720 `$sequence .= uc$_`. Single in-place pass is fine.
5. **CRLF `\r` strip** (plan §3.3.5) — **mechanically confirmed faithful for the common case.** noodles' sequence reader (`io/reader/sequence.rs`) strips a *trailing* `\r` per physical line and skips empty lines; its `read_line` (reader.rs) strips trailing `\r\n` from the definition. This matches Perl's `chomp; s/\r//` for normal CRLF files. **Subtle divergence:** Perl `s/\r//` removes the *first* `\r` anywhere in the line; noodles only strips a *trailing* `\r`. A `\r` embedded mid-sequence-line (malformed, e.g. old Mac CR-only line endings `\r` with no `\n`) would be handled differently — Perl's line-based `<CHR_IN>` reads up to `\n` so a lone `\r` mid-content is *kept* by Perl too (its `s/\r//` only strips one). This is an exotic, malformed-input case; **note it as a known non-goal**, don't chase it.
6. **Duplicate name error** (plan §3.3.6) — ✅ Perl 1702-1705/1724-1726. But see 1.5 on *when* it fires.
7. **u32 overflow guard** (plan §3.3.7) — ✅ sound; SPEC §15 confirms `u32` pos/counts. Fails loud. Good.
8. **Empty-sequence records stored** (plan §3.3.5) — ✅ Perl 1707-1711 warns but stores `''`. Plan stores empty `Vec<u8>`. ✅

### 1.4 IMPORTANT — `Mus`-only tier produces empty genome with NO error (Perl quirk not in plan)

Perl's `die "…does not contain any sequence files…"` (1671-1673) fires **only if all four globs were empty** — i.e. it tests `@chromosome_filenames` *before* the `foreach` loop, and the `Mus` skip (1678) is *inside* the loop. So a genome dir whose `*.fa` tier contains **only** `Mus_musculus.NCBIM37.fa`:
- Perl: tier is non-empty → `die` at 1671 does NOT fire → loop runs → the sole file is `next`-skipped → `%chromosomes` ends up **empty**, no error. Downstream the report walk would emit nothing / the uncovered pass iterates an empty genome.

The plan's §3.3 step 1 says "Empty after all four → `NoGenomeFasta`", and step 2 skips Mus. **The plan does not specify what happens when the chosen tier is non-empty but every file in it is skipped (Mus-only, or all-skipped).** Two faithful options:
- **(a) Match Perl exactly:** non-empty tier + all-skipped ⇒ return a `Genome` with zero chromosomes, NO `NoGenomeFasta` error.
- **(b) Diverge (arguably better):** treat zero loaded chromosomes as `NoGenomeFasta`.

For **byte-identity** the safe choice is **(a)** — but note that with zero chromosomes the report walk in Phase B will produce an empty report (and the cytosine_context_summary will be the 64-row all-`N/A` table). Whatever the choice, **the plan must state it and V9 (Mus skip) must include the Mus-*only* sub-case.** This is the single concrete logic gap I found in the genome reader. (Realistically harmless — nobody ships a genome dir of only the tophat mouse file — but it is exactly the kind of silent-empty-genome the contract should pin.)

### 1.5 Duplicate-name detection timing — verify the planned impl matches Perl's "previous-chromosome" check

Perl detects dups in **two places**: (1) inside the loop when a new `>` header is seen, it checks whether the *just-finished* `$chromosome_name` already exists (1701-1705), and (2) after EOF for the *last* record (1724-1726). Critically, Perl's check is keyed on the name of the chromosome **whose sequence just completed**, and Perl reads files **in glob order** (which is bytewise-sorted by the shell glob). noodles' `records()` iterator yields complete `Record`s, so the natural Rust impl is "insert into map; if key present, error." **That is equivalent** to Perl here because both detect the collision at the moment the second occurrence's *record* is finalized. ✅ No defect — but the plan's V11 should assert dup **across two files** (cram_ref.rs's test does within+across; the plan's V11 says "a dup-name file" — make it explicitly cover the cross-file case too, since Perl iterates multiple files and the collision can span files).

One nuance: Perl iterates files in **glob (bytewise-sorted) order**, and `noodles` + the plan's tier-collection order. For dup *detection* the order is irrelevant (collision is symmetric). For the *error message* (`DuplicateChromosomeName { name }`) it's also irrelevant. ✅ But it matters for D1 — see §2.

### 1.6 `extract_chromosome_name` — leading-whitespace divergence (theoretical)

Perl `extract_chromosome_name` (1744-1745): `$fasta_header =~ s/^>//; ($name) = split(/\s+/, $fasta_header)`. Perl `split /\s+/` on a string with a **leading** space yields a **leading empty field**, so `(split /\s+/, " foo")[0]` is `""` — Perl would extract an **empty** chromosome name from a header like `> chr1` (space immediately after `>`). noodles `splitn(2, is_ascii_whitespace)` on `" chr1"` (after stripping `>`) yields component 0 = `""` as well (the `.and_then(|s| if s.is_empty(){None}…)` then returns `ParseError::MissingName` → noodles **errors** instead of producing an empty name). So:
- Perl: `> chr1` ⇒ name = `""` (empty string key), stored.
- noodles/plan: `> chr1` ⇒ `read` errors with `MissingName` → propagates as `Io`/parse error.

This is a **divergence on malformed headers** (`>` followed immediately by whitespace). Real Bismark genomes never have this (Bowtie-style `>chr1`). **Acceptable as a non-goal**, but worth a one-line note since the plan explicitly claims to reproduce `split /\s+/` semantics — it reproduces them for the normal case, not the leading-whitespace pathological case. Don't chase it; just don't claim exactness you don't have.

### 1.7 `version` flag declared as `bool` with `-V` — verify no clap collision

Plan §3.1 declares `version: -V/--version: bool` with `disable_version_flag = true`. dedup does the same (`#[arg(short='V', long="version")]` + `disable_version_flag`). ✅ Faithful. Note Perl's flag is only `--version` (2020, no `-V`); the Rust port *adds* `-V`. Harmless additive (dedup precedent). Fine.

---

## 2. Assumptions — Deviation D1 (`HashMap` vs `IndexMap`) is the crux

**Verdict on D1: SOUND for Phase A.** I tried hard to find a path by which genome *insertion order* leaks into gated output. It does not, for these reasons grounded in the Perl:

1. **Covered chromosomes** emit in **cov-file appearance order** (SPEC §7.5 step 1; Perl buffers per-chr and flushes on `chr` change). This order is driven by the *cov file*, captured by a Phase-B insertion-ordered structure — **not** by the genome map. The genome map is only a `get(name)→seq` lookup here. Order-independent. ✅
2. **Uncovered chromosomes** emit in **`sort keys %processed`** order (Perl 722) = bytewise sort. The plan's `names_sorted()` re-sorts, so genome insertion order is overwritten. ✅
3. **Context summary** (Perl `print_context_summary`, 63-78) iterates **`sort keys %context_summary`** then **`sort keys %{…}`** — fully sorted by `(tri_nt, ubase)`, and the grid is a fixed 16×4 init (SPEC §8, plan defers to Phase B). **No genome-map iteration feeds it.** I specifically checked this because the task flagged it: the summary accumulates per-position during the walk (which is genome-*sequence*-ordered within a chromosome, and chromosome order doesn't matter because addition is commutative) and prints in sorted key order. **Insertion order cannot leak.** ✅
4. **No tie-break** anywhere consumes genome iteration order — the covered list is its own structure (Phase B), uncovered is sorted.

So `HashMap` is correct **and** D1 correctly identifies SPEC §11's `IndexMap` as over-specified *for the genome map*. **One caveat the plan should make explicit:** the SPEC's `IndexMap` was doing double duty — D1 must be crystal that the *covered-appearance-order list* (the thing that genuinely needs insertion order, SPEC §7.5/§10.4, pitfall P1) still lives in Phase B and is **NOT** the genome map. The plan §11 Deviation says this ("`IndexMap` is deferred to Phase B for the covered-chromosome appearance list") — good. Make sure the SPEC-rev note (promised "at next rev") lands so a future reader doesn't think the whole `IndexMap` idea was wrong; only its application to the *genome* map was.

**Residual D1 risk (low, name it):** `HashMap` iteration order is nondeterministic across runs (random seed). If any *future* code (Phase B/C/D) ever iterates `Genome.chromosomes` directly without sorting, it would silently break byte-identity and **tests might pass by luck** (small maps often iterate in a stable-looking order). Mitigation: the `Genome` API in §3.3 exposes only `get`/`contains`/`names_sorted`/`len`/`is_empty` — **no raw iterator**. **Keep it that way** (don't add `iter()`/`keys()` to the public API), and the D1 reasoning stays airtight by construction. Recommend the plan state this as an explicit API invariant: "`Genome` exposes no insertion-order-dependent iterator; uncovered-pass order is `names_sorted()` only."

**Other assumptions:**
- §8.6 noodles `record.name()` — **now resolved** (1.3 above): it IS up-to-first-whitespace. Remove the "verify in impl / fallback to manual split" hedge; the fallback is dead code. Keep the resolved fact.
- §8.9 `.fa.gz` is plain gzip, `MultiGzDecoder` handles plain+multi-member+BGZF — ✅ true (BGZF is gzip-framed; `MultiGzDecoder` reads concatenated members). One **efficiency caveat in §3 below**.
- §8.2 whole genome in RAM (~3 GB hg38) — ✅ matches Perl.

---

## 3. Efficiency

Phase A is I/O-bound genome loading; the plan's posture (no premature parallelism, single uppercase pass, O(1) lookups) is right.

- **`MultiGzDecoder` + noodles `Reader` over `BufReader`** — fine, but **double-buffer** it correctly: wrap as `BufReader::new(MultiGzDecoder::new(File))` (decoder is not itself buffered). The plan §3.3.3 says "wrap in `BufReader`, feed a noodles Reader" — ✅ correct intent; just ensure the `BufReader` is *outside* the decoder (the plan's wording is ambiguous on order). For plain files, `BufReader::new(File)`. State the exact nesting to avoid a per-byte-read perf cliff.
- **`record.sequence().as_ref().to_vec()` then uppercase in place** is one alloc + one pass. Fine. Don't pre-optimize.
- **`names_sorted()` O(K log K)** — trivial (K ≈ tens-to-hundreds). ✅
- **No concern**: the plan correctly does NOT add `rustc-hash`/`indexmap`/`mimalloc` in Phase A.

No efficiency defects. The only note: genome load reads the *whole* tier; if a dir has many `.fa` files this is N opens — expected and matches Perl.

---

## 4. Validation sufficiency

V1–V14 cover the main rules. **Gaps that could let Phase A ship a wrong genome/config silently:**

**Important (add these):**
- **V-gap 1 — Mus-only / all-skipped tier** (see 1.4). Add: dir with *only* `Mus_musculus.NCBIM37.fa` ⇒ assert the chosen behavior (empty `Genome`, no error — option (a)) or `NoGenomeFasta` (option (b)). Currently V9 mixes Mus+chr1 so the all-skipped path is untested.
- **V-gap 2 — glob tier-priority when a higher tier exists but is "empty after Mus skip."** Edge: `*.fa` tier = `{Mus_musculus.NCBIM37.fa}` only, `*.fa.gz` tier = `{chr1.fa.gz}`. Perl picks the `.fa` tier (non-empty), skips Mus, and **does NOT fall through to `.fa.gz`** → empty genome. Pin this — it's the nastiest interaction of P8 + the Mus skip and the plan is silent on it.
- **V-gap 3 — cross-file duplicate name** (see 1.5). V11 should assert a dup spanning *two files*, not just one multi-FASTA file.
- **V-gap 4 — `output_stem` strip is suffix-anchored.** Perl 108/111 uses `s/\.CX_report.txt$//` / `s/\.CpG_report.txt$//` — **anchored at end (`$`), and only the matching one runs based on `$CX_context`.** The plan §3.2 step 11 says "strip trailing `.CpG_report.txt`/`.CX_report.txt`". **Subtle:** Perl strips **`.CX_report.txt` iff `--CX`, else `.CpG_report.txt`** — it does NOT strip both unconditionally. So `-o foo.CX_report.txt` **without** `--CX` is **NOT** stripped by Perl (only the CpG suffix is checked in that branch) → stem stays `foo.CX_report.txt`. V7 tests `-o foo.CpG_report.txt` and `-o foo.CX_report.txt` but **does not say which `--CX` state each runs under.** The plan must specify: the strip suffix is **selected by `cx_context`**, mirroring Perl's `if($CX_context){strip CX}else{strip CpG}`. Add a V7 sub-case: `-o foo.CX_report.txt` **without** `--CX` ⇒ stem `foo.CX_report.txt` (NOT `foo`). **This is a real byte-identity trap.** (Also note: the regex `.` is literal-dot-insensitive in Perl — `\.` is escaped in 108/111? No — Perl 108 is `s/\.CX_report.txt$//` with `\.` escaping the first dot but `report.txt` has an *unescaped* `.` matching any char. Pedantically `foo.CX_reportXtxt` would also strip. This is a Perl bug-quirk; reproducing it is almost certainly unnecessary, but **note that the Rust impl using a literal `ends_with(".CX_report.txt")` is a (benign) divergence from Perl's regex `.`** — flag as known non-goal.)

**Optional (nice to have):**
- **V-gap 5 — `--CX` alias forms.** Plan §3.1 lists `-CX`/`--CX_context` (alias `--CX`). Perl `GetOptions "CX|CX_context"` accepts `--CX` and `--CX_context` (and `--CX-context`? no). clap derive: `#[arg(short, long)]` on a field named `cx_context` gives `--cx-context` (kebab) + a short — **NOT `--CX_context` with underscore** unless `long = "CX_context"` is set explicitly, and `-CX` is **not a valid short** (shorts are single-char). The plan's table conflates `-CX` (Perl's `CX` is a *long* `--CX`, not a short `-C X`). **Verify the clap attrs:** you need `#[arg(long = "CX_context", visible_alias = "CX")]` and there is **no** `-CX` short (Perl never had one either — `GetOptions "CX|CX_context"` makes `--CX` and `--CX_context` both *long*). The plan's "`-CX`" notation is misleading. Add a V-case asserting both `--CX` and `--CX_context` parse to `cx_context=true`. (dedup uses `visible_alias` — same mechanism.)
- **V-gap 6 — empty-sequence record stored.** Plan §3.3.5 says store empty; no V-row asserts it. Add: `>chrEmpty\n>chr1\nACGT` ⇒ `chrEmpty` present with len 0, `chr1` len 4.
- **V-gap 7 — `--coverage_threshold` alias `--threshold`** and `--discordance_filter` parse. Plan §3.1 lists the aliases; add a parse assertion (cheap).
- **V-gap 8 — `version_string()` format** matches `coverage2cytosine_rs <semver> (<os>/<arch>)` (dedup parity). V2 checks the `--version` regex loosely; tighten to the dedup format.

**Adequately covered:** glob priority `.fa`>`.fa.gz` (V8), uppercase (V10), `.gz` plain-gzip load (V12), `names_sorted` bytewise `chr10<chr2` (V13 — good, this is the LC_ALL=C byte-sort), empty dir (V14), every mutex/range validate rule (V4), v1.x rejection (V5), `cpg_only` coupling (V6).

---

## 5. Alternatives

- **noodles gz path vs `flate2`:** The plan uses `flate2::MultiGzDecoder` + a `noodles::Reader` because noodles' `build_from_path` "only handles BGZF." **Verify that claim** — `cram_ref.rs` (line 103) uses `noodles_fasta::io::reader::Builder.build_from_path` and its test `reconstitute_accepts_gzipped_fasta` writes **BGZF** specifically (comment: "noodles-fasta uses BGZF for .gz"). Real Bismark `.fa.gz` are **plain gzip** (`gunzip -c`, Perl 1681), not BGZF. So the plan is **right to bypass `build_from_path`** and decode with `flate2` first. **This is a correct and important divergence from the cram_ref pattern** — call it out as such (the plan does, §2 "Divergences to apply"). ✅ The alternative (rely on noodles auto-detect) would **fail or misread plain-gzip** — do NOT take it. The plan's choice is the right one.
- **`HashMap` vs `IndexMap`:** covered in §2 — `HashMap` is correct; keep the no-public-iterator invariant.
- **Manual definition-line split vs `record.name()`:** resolved — use `record.name()` (1.3). Drop the manual-split fallback to reduce surface.
- **`Vec<(name, seq)>` + sidecar vs `HashMap`:** for the genome map specifically, `HashMap` is simpler and order-irrelevant (§2). No reason to prefer a Vec here. ✅

---

## 6. Action items

### Critical
*(none — no correctness defect in the planned Phase-A scope; D1 is sound).*

### Important
1. **Specify the Mus-only / all-skipped-tier behavior** (§1.4). Decide option (a) match-Perl (non-empty tier whose files are all skipped ⇒ empty `Genome`, no `NoGenomeFasta`) vs (b) diverge (treat zero loaded chromosomes as `NoGenomeFasta`). State it in §3.3 and add **V-gap 1** + **V-gap 2** (Mus-only tier does NOT fall through to the next tier). For byte-identity, (a) is the safe default.
2. **Fix the `output_stem` strip semantics** (§4 V-gap 4). The stripped suffix is **selected by `cx_context`** (`--CX` ⇒ strip `.CX_report.txt`; else strip `.CpG_report.txt`), NOT "strip both." Update §3.2 step 11 and add the V7 sub-case `-o foo.CX_report.txt` **without** `--CX` ⇒ stem unchanged (`foo.CX_report.txt`). This is a genuine byte-identity trap.
3. **Make the D1 no-iterator invariant explicit** (§2): the `Genome` public API must expose **no** insertion-order-dependent iterator (only `get`/`contains`/`names_sorted`/`len`/`is_empty`), so the `HashMap`-is-order-irrelevant argument holds by construction across Phases B–D. State it in §3.3 step 8.
4. **Resolve the noodles open questions in-plan** (§10): `record.name()` IS up-to-first-ASCII-whitespace (confirmed in `definition.rs`) — drop the "verify/fallback" hedge. `MultiGzDecoder` plain-gzip choice is confirmed correct and is a *deliberate* divergence from `cram_ref.rs`'s `build_from_path` (which expects BGZF). Promote both from "Open" to "Resolved."
5. **Add cross-file duplicate-name test** (§1.5, V11): the dup must be asserted spanning two FASTA files, matching Perl's multi-file glob iteration (cram_ref.rs's test does this).

### Optional
6. **Clarify `--CX` clap attrs** (§4 V-gap 5): there is no `-CX` short; use `long = "CX_context", visible_alias = "CX"`. Fix the misleading "`-CX`" in the §3.1 table; add a parse test for both `--CX` and `--CX_context`.
7. **Note the `discordance` `u8` parse-vs-validate split** (§1.2): negative/`>255` inputs are rejected at clap parse (exit 2), only `0` and `101..=255` reach `DiscordanceOutOfRange` via `validate` (exit 1). Adjust V4's expectation accordingly.
8. **Add V-rows** for: empty-sequence record stored (V-gap 6), `--threshold`/`--coverage_threshold` + `--discordance_filter` parse (V-gap 7), tightened `version_string()` format (V-gap 8).
9. **State the `BufReader` nesting** explicitly: `BufReader::new(MultiGzDecoder::new(File))` (buffer outside the decoder) to avoid a per-byte read cliff (§3).
10. **Document known non-goals** (malformed input): leading-whitespace header `> chr1` (Perl→empty name; noodles→parse error), mid-line `\r` strip difference, and the literal-`.` vs regex-`.` in the suffix strip. These are all malformed-input divergences not worth chasing — just name them so they aren't mistaken for bugs later.

---

## 7. Confirmation of plan claims I verified true

- Crate pins **all match** the workspace's transitive choices in `Cargo.lock`: `clap 4.5.30`, `thiserror 2.0.0`, `noodles-fasta 0.61.0` (which depends on `noodles-core 0.20.0` — confirmed in the lock; the stray `noodles-core 0.18.0` is only `noodles-csi 0.50.0`'s and is irrelevant), `flate2 1.1.9`, `assert_cmd 2.0.16`, `predicates 3.1.2`, `tempfile 3.10.1`, `bstr 1.10.0`. **`noodles-core =0.20.0` is correct.** ✅
- The workspace `Cargo.toml` has **no `[workspace.dependencies]` table** — deps are pinned per-crate with `=` (dedup precedent). The plan's per-crate `=`-pin approach matches house style. The plan must also **add `bismark-coverage2cytosine` to `members`** in the workspace `Cargo.toml` (§2 says so — ✅, just don't forget it in impl).
- `version_string()` format, `disable_version_flag`, exit codes (0/1/2), `#![forbid(unsafe_code)]`/`#![warn(missing_docs)]`, `thiserror` enum with `#[from] std::io::Error` — all faithful to dedup `lib.rs`/`main.rs`/`error.rs`. ✅
- `cram_ref.rs` accepts `.fna`/`.ffn` and **sorts** chromosomes + uses `build_from_path` (BGZF) and does **not** uppercase — the plan correctly identifies all three as **divergences to apply** (four-suffix only, first-non-empty tier, uppercase, flate2 plain-gzip). ✅

**Bottom line:** solid Phase-A plan. The `HashMap`/D1 reasoning is airtight given a no-public-iterator `Genome` API. Fix the five Important items (Mus-only tier, `output_stem` cx-selected strip, no-iterator invariant, resolve noodles open Qs, cross-file dup test) and the validation gaps, and it's ready.
