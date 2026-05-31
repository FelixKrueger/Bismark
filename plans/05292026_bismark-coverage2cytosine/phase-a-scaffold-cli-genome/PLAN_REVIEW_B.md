# Phase A PLAN — Review B

**Reviewer:** Plan Reviewer B (independent, fresh context).
**Target:** `phase-a-scaffold-cli-genome/PLAN.md` (rev 0).
**Method:** Plan claims verified line-by-line against Perl `coverage2cytosine` v0.25.1, the dedup/cram_ref Rust patterns, `Cargo.lock` pins, and the actual noodles-fasta 0.61.0 source in the cargo cache.

**Verdict: APPROVE WITH CHANGES.** The plan is well-grounded, the dependency pins are correct, and the central design bet (Deviation D1: `HashMap` over `IndexMap`) is **provably correct** — I traced every `%chromosomes`/`%processed` iteration in the Perl and none of them leak genome-map insertion order to any output stream. Two issues must be fixed before implementation (both in `ResolvedConfig` resolution, §3.2 step 11): the **output-stem strip is context-conditional in Perl** (the plan strips both suffixes unconditionally), and the **`output_dir` default is `''` not CWD**. A handful of Important/Optional clarifications follow.

---

## 1. Logic review

### 1.1 CLI validation order vs Perl `process_commandline` — faithful, with caveats

I walked the plan's §3.2 ordered rejection list against Perl `:2031-2194`. The mapping is accurate:

| Plan step | Perl line | Verdict |
|-----------|-----------|---------|
| 2. v1.x reject (`--gc`/`--nome`/`--drach`/`--ffs`) | n/a (new) | OK — correct port-policy addition (P9, SPEC §3) |
| 3. missing `-o` | :2077-2079 | OK |
| 4. missing `-g` | :2133-2134 | OK (mouse default rejected, SPEC §15) |
| 5. merge+CX | :2139-2141 | OK |
| 6. merge+split | :2142-2144 | OK |
| 7. merge+threshold | :2174-2177 | OK |
| 8. discordance without merge | :2164-2165 | OK |
| 9. discordance range 1..=100 | :2168 | OK |
| 10. `threshold == Some(0)` error; `None`⇒0 | :2178-2186 | OK — see §1.2 |
| 11. resolve `cpg_only=!cx`, stem, dirs | :2112-2115, :104-112, :2070-2110 | **Two bugs — see §1.3** |

**One ordering nuance worth recording (not a bug).** In Perl the **`-o` check (`:2077`) runs BEFORE the `-g` check (`:2133`)** and before the merge/discordance/threshold block (`:2138-2194`). The plan preserves this (`-o`=step 3, `-g`=step 4, merge block=steps 5-10), so the *first* error a user sees matches Perl. The v1.x reject (plan step 2) is sequenced *ahead* of `-o`/`-g`; Perl has no equivalent (it would silently accept). Since STDERR isn't byte-gated and these are exit-1 error paths, the exact ordering between the new v1.x reject and the Perl-native rejects is not byte-identity-relevant — but front-loading it is the right UX call. Fine as-is.

**Another nuance (not a bug):** Perl's `--nome-seq` at `:2147-2161` has its own mutex logic (dies on `--CX`, dies on `--merge_CpGs`, auto-sets `--gc` + threshold=1). The Rust port rejects `--nome-seq` outright (plan step 2) before any of that runs, which is correct for v1.0 — there's no behavior to preserve since the whole mode is deferred.

### 1.2 `threshold Some(0)` vs `None` — correct

Perl `:2174-2186`: if `defined $threshold` and `$threshold > 0` fails → die (`:2178`); else (`undef`) → `$threshold = 0` (`:2185`). So **explicit `0` dies, absence ⇒ 0 = report-all**. The plan's step 10 reproduces this exactly (`Some(0)`⇒`ThresholdNotPositive`; `None`⇒`unwrap_or(0)`). The plan's inline note is precise. ✔

One subtlety the plan glosses (Optional): Perl's `=i` parser accepts a **negative** threshold (e.g. `--coverage_threshold -5`), which then dies at `unless ($threshold > 0)`. The Rust `Option<u32>` makes clap reject negatives at *parse* time (exit 2, clap usage error) rather than at `validate` (exit 1, `ThresholdNotPositive`). Different exit code + different STDERR for a malformed input. STDERR isn't gated and this is a garbage-in case, so it's acceptable — but record it so a future reader doesn't treat the divergence as a regression.

### 1.3 `cpg_only = !cx_context` — correct

Perl `:2112-2115`: `unless ($CX_context){ $CX_context=0; $CpG_only=1; }`. With `--CX`, `$CpG_only` stays `undef` (falsy). So `cpg_only == !cx_context` is the faithful coupling. ✔ (V6 covers it.)

### 1.4 Genome reader quirks — faithful, and the noodles open question is RESOLVED

I verified every step of §3.3 against Perl `read_genome_into_memory:1648-1739` + `extract_chromosome_name:1741-1751`:

- **Glob priority** (step 1): Perl `:1654-1669` is exactly the four-tier `unless (@…)` fallback `*.fa → *.fa.gz → *.fasta → *.fasta.gz`, **first non-empty wins** (each tier only runs if the prior is empty). The plan reproduces this and correctly flags that `cram_ref.rs`'s `.fna`/`.ffn` are NOT in c2c's set. ✔ (P8, V8.)
- **Mus skip** (step 2): Perl `:1678` `next if … eq 'Mus_musculus.NCBIM37.fa'` — exact filename literal. ✔ (V9.)
- **First-token name** (step 4): Perl `:1745` `split /\s+/`, token 0. **I confirmed noodles 0.61.0 matches this** — see §5.1. The plan's "verify, fallback to manual split" is now answerable: noodles is correct, no fallback needed.
- **Uppercase** (step 5): Perl `:1720` `$sequence .= uc$_`. ✔ (P2, V10.) Critical divergence from cram_ref.rs (which does NOT uppercase) — correctly called out.
- **Duplicate name** (step 6): Perl `:1702-1705` / `:1724-1726`. ✔ (V11.)
- **`u32` overflow guard** (step 7): SPEC §15 resolved. ✔
- **HashMap** (step 8): **validated — see §2.**

**Gap (Important): the empty-FASTA / no-`>`-header edge diverges silently.** Perl reads the **first line unconditionally as a header** (`:1688` `my $first_line = <CHR_IN>; … extract_chromosome_name`), and `extract_chromosome_name` *dies* (`:1749`) if that line doesn't start with `>`. A **completely empty file** in the chosen tier: Perl's `<CHR_IN>` returns `undef`, `chomp undef`, and `extract_chromosome_name(undef)` — the `s/^>//` fails ⇒ Perl die "doesn't seem to be in FASTA format". noodles' `records()` on an empty file yields **zero records and no error** (the `Records` iterator returns `None` on `read_definition Ok(0)` — I read the source). So a stray empty `.fa` (e.g. a `touch chr0.fa` that wins the `.fa` tier) makes noodles silently contribute nothing, whereas Perl dies. This is exactly the "silently load a wrong genome undetected" failure the prompt asks about. **It is not in V1–V14.** Decide the contract (match Perl's die, or document the benign divergence) and add a test. Note that an empty *file* also interacts with glob priority: Perl's `<*.fa>` returns the empty file as a member, so the `.fa` tier is "non-empty" (one filename) and the `.fa.gz` tier is never consulted — see §4.1.

**Gap (Important): malformed first line that is non-empty but lacks `>`.** Perl `extract_chromosome_name` dies (`:1749`). noodles `Definition::from_str` returns `ParseError::MissingPrefix`, which the `Records` iterator surfaces as `io::Error(InvalidData)` — so noodles *does* error here, but through the `Io` variant with a noodles message, not a c2c-typed "not in FASTA format" error. Behaviorally close enough (both fail loud), but neither V11 nor any test exercises it. Add a test asserting it errors (variant can be `Io`).

### 1.5 No-positional-cov-infile path — scope-correct but note the divergence

Perl `:2059-2065`: no `@ARGV` ⇒ warn + `sleep(2)` + print help + `exit` (exit 0!). The plan makes `cov_infile` a **required positional** (§3.1), so clap errors with exit 2 (usage error) when it's missing. This diverges from Perl (which exits 0 after printing help) but is the idiomatic Rust/clap behavior and matches the dedup precedent. STDERR/exit-on-missing-arg isn't gated. Fine — but the plan doesn't mention it; a one-line note would help (Optional).

---

## 2. Assumptions — Deviation D1 (`HashMap` vs `IndexMap`): VALIDATED

This is the load-bearing assumption, so I traced it exhaustively. **D1 is correct.** I grepped every iteration over the genome/processed hashes in the Perl and classified each:

```
:33   scalar keys %chromosomes   → STDERR count only (order-irrelevant)
:66   sort keys %context_summary → context summary, SORTED, fixed 64-cell grid
:67   sort keys %{…{$context}}   → context summary inner, SORTED
:722  sort keys %processed       → uncovered-chromosome pass, BYTEWISE SORTED
```

There is **no** bare `keys %chromosomes` / `foreach %chromosomes` / `values %chromosomes` / `each` that drives output. Confirmed paths:

- **Covered chromosomes** (`:206-468`, last-chr flush `:476-704`): order = cov-file first-appearance order, NOT genome-map order. A Phase-B concern (insertion-ordered structure for the covered set), entirely independent of how `Genome` stores its map.
- **Uncovered chromosomes** (`:717-728`): `foreach my $chr (sort keys %processed)` → bytewise-sorted names, each looked up by name in `%chromosomes` (`process_unprocessed_chromosomes:1405` `$chromosomes{$chr}`). The plan's `names_sorted()` (bytewise `Vec<u8>` Ord) reproduces this exactly. Lookup is by-key, not by-iteration.
- **Context summary** (`:63-78`): a fixed `C{ACGT}{ACGT} × {ACGT}` 64-cell grid, emitted `sort keys` — never touches genome-map order. (This directly answers the prompt's "context summary ordering" leak question: NO leak.)

**Multi-FASTA tie-breaks:** the only place genome-map *content* (not order) matters is duplicate-name detection, which errors regardless of structure. There is no "first wins on tie" path because duplicates are fatal. So no hidden order dependency there either.

**Conclusion:** the `HashMap<Vec<u8>, Vec<u8>>` choice is sound, the `IndexMap` in SPEC §11 was indeed an over-statement for the *genome* map, and the deviation correctly defers `IndexMap` to Phase B for the *covered-appearance* list. The plan's commitment to update the SPEC at next rev is the right bookkeeping. **No action needed on D1 itself** — only the SPEC-sync note (Optional).

**One caution for Phase B handoff (not a Phase-A bug):** the `Genome` API exposes `names_sorted()` but NOT an insertion-ordered iterator. That's fine — Phase B's covered order comes from the cov file, not the genome — but the plan should make explicit (it implies it) that **`Genome` deliberately offers no insertion-order iteration**, so Phase B doesn't accidentally reach for one expecting genome order.

---

## 3. Efficiency

- **Whole-genome-in-RAM**: matches Perl; hg38 ≈ 3 GB. Acceptable per SPEC §6/§10.7. No concern.
- **Uppercasing**: the plan says "single in-place pass per record." Note that with noodles you receive `record.sequence().as_ref()` as a freshly-allocated `Vec<u8>` per record; uppercasing is `iter_mut().for_each(to_ascii_uppercase)` on that owned vec — one pass, no extra allocation beyond what noodles already does. Fine. (Aside: noodles already allocates the sequence buffer; there's no zero-copy win available here, and none is needed at Phase-A scope.)
- **Lookup**: `HashMap` O(1) per covered chromosome — correct. `names_sorted()` O(K log K), K tiny. Fine.
- **No premature parallelism**: correct posture (byte-identity gate first, SPEC §10.7).
- **No over-engineering**: the plan correctly defers `indexmap`/`rustc-hash`/`mimalloc`. Good discipline.

`flate2` in Phase A: justified — the genome reader itself can be `.fa.gz`, and noodles `build_from_path` only auto-handles **BGZF**, not plain gzip (confirmed in §5.2). So `flate2` genuinely belongs in Phase A. ✔ (This corrects the SPEC's "Phase C gzip" framing; the plan's Deviation note is accurate.)

---

## 4. Validation sufficiency

V1–V14 cover the headline cases well (glob priority, Mus skip, uppercase, dup, gz, sorted names, empty dir, every CLI rule). But several **high-risk, silent-wrong-result** cases are untested:

### 4.1 Critical/Important gaps

- **(Important) Empty file in the winning tier** — §1.4. A `touch chr0.fa` makes Perl die but noodles silently skip. *Worse*: because Perl's glob counts the empty file as a tier member, the presence of an empty `.fa` **suppresses the `.fa.gz` tier** (the real genome). So an empty `chr0.fa` next to `chr1.fa.gz` ⇒ Perl reads only `chr0.fa` (empty) then dies; the Rust glob-priority logic must reproduce "tier is non-empty if it has ≥1 filename, even empty ones" — does the plan's "first non-empty tier wins" mean *non-empty list of filenames* or *non-empty file contents*? **Perl means non-empty list of filenames** (`@chromosome_filenames` truthiness). The plan's wording "first non-empty tier wins" is ambiguous and the §3.3 step-1 phrasing "Empty after all four → NoGenomeFasta" suggests filename-list semantics, which is correct — but add a test with an empty `.fa` + a real `.fa.gz` to pin that the `.fa` tier still wins (matching Perl) and the `.fa.gz` is NOT read.

- **(Important) FASTA header with trailing description** (`>chr1 some description here`) — V11 mentions `>chr1 desc` but only asserts the *name*; add an explicit assertion that the description bytes after the first whitespace are **dropped** from the stored name (this is the exact `split /\s+/` semantic and the noodles `name()` boundary). Low effort, high value.

- **(Important) Glob priority when a higher tier exists but its only member is empty** — overlaps with the first bullet; the prompt explicitly calls this out. One combined test ("empty `.fa` + populated `.fa.gz`") closes both.

### 4.2 Optional gaps

- **CRLF / `\r` stripping (V-gap):** noodles strips trailing `\r` automatically in *both* definition (`read_line` pops `\n` then `\r`) and sequence (`fill_buf` trims trailing CR) — I verified the 0.61.0 source. So the plan's §3.3-step-5 "ensure CRLF `\r` is dropped" is **already handled by noodles and the manual strip is redundant** (and would be wrong if applied mid-sequence). Recommend: drop the "strip `\r` ourselves" language, add one CRLF-file test (`>chr1\r\nAC\r\nGT\r\n` ⇒ name `chr1`, seq `ACGT`) to lock the noodles behavior so a future noodles bump can't regress it silently. Note: noodles only strips a **trailing** `\r` (CRLF); a bare embedded `\r` (old-Mac CR-only line endings, no `\n`) would NOT be split into lines by either Perl `<>` (which splits on `\n`) or noodles — both treat the whole CR-delimited blob as one line. Behavior matches Perl. Fine.

- **Empty-sequence record** (`>chr1\n>chr2\nACGT`): plan §3.3-step-5 says "keep (Perl warns but stores) — store empty `Vec<u8>`." I confirmed Perl stores it (`:1707-1711`, warns then `$chromosomes{$name}=''`). noodles `records()` on `>chr1\n>chr2\nACGT\n` yields chr1 with an empty `Sequence` (the sequence reader stops at the next `>` and `consume_empty_lines` handles the blank). So Rust matches Perl. But there's **no test** for it — add one (it's in SPEC §12.1 but absent from this plan's V-table). A zero-length chromosome that survives to Phase B's C/G walk produces zero matches (no output lines) — benign, but pin it.

- **gz that is BGZF vs plain gzip:** plan §3.3-step-3 uses `flate2::read::MultiGzDecoder` for *all* `.gz`. `MultiGzDecoder` reads plain gzip AND multi-member gzip; BGZF is gzip-framed (concatenated gzip members) so `MultiGzDecoder` reads it too. ✔ The plan's assumption #9 is correct. V12 tests plain gzip; add a one-liner BGZF-`.fa.gz` test (cram_ref.rs already has a `noodles_bgzf::io::Writer` fixture pattern to copy) to prove `MultiGzDecoder` doesn't choke on a real Bismark `.fa.gz` if one happens to be BGZF. Low effort.

- **`output_stem` strip with `-o` already ending in suffix** — see §1.3 below; this needs a *correctness* fix AND a test, not just a test.

- **`--CX` alias surface** (`--CX` / `-CX` / `--CX_context`): Perl `:2016` `"CX|CX_context"` defines long names `CX` and `CX_context` (and Getopt auto-allows `-CX`? No — Getopt `CX|CX_context` makes `--CX` and `--CX_context`; single-dash `-CX` is **not** a Getopt long-with-single-dash by default). The plan §3.1 lists `-CX`/`--CX_context (alias --CX)`. **clap will NOT accept `-CX` as a single short flag** (`-CX` parses as `-C -X` bundled shorts, or errors). Perl's `--CX` is a *long* option. Recommend: model it as `#[arg(long = "CX_context", visible_alias = "CX")]` (two long forms `--CX_context`/`--CX`), and DROP the `-CX` short unless you confirm Perl actually accepts `-CX` (it does via Getopt's single-char-cluster leniency, but reproducing that in clap is not worth it and not byte-gated). V2 should assert `--help` shows `--CX_context`/`--CX`. This is a real CLI-surface bug-in-waiting; flagging Important.

### 4.3 Where Phase A could silently load a wrong genome / resolve wrong config

Synthesizing the prompt's core worry:
1. **Empty file wins a tier** (§4.1) — silent wrong genome (Rust loads nothing where Perl dies). **Highest silent-failure risk.**
2. **Context-conditional stem strip** (§1.3 / Critical below) — silent wrong *output filename* (double-suffix or wrong stem), undetected until Phase B/E byte-diff. **Highest config-resolution risk.**
3. **`output_dir` default `''` vs CWD** (§1.3) — affects where files land in Phase B/C; mostly equivalent but pin the semantics now.

---

## 5. Alternatives

### 5.1 noodles `record.name()` semantics — RESOLVED (the plan's open Q is answerable now)

I read noodles-fasta 0.61.0 source directly:
- `Record::name()` → `Definition::name()` → returns `&BStr` (the `name` field).
- `Records::next()` builds the `Definition` via `self.line_buf.parse()` ⇒ `Definition::from_str` (`record/definition.rs:108`), which does `line.splitn(2, |c: char| c.is_ascii_whitespace())` and takes component 0 as the name.

So **noodles `name()` is exactly "up to the first ASCII-whitespace char"**, matching Perl's `split /\s+/` token 0. **The plan's fallback manual split is unnecessary.** Recommend resolving the open Q in the plan to "noodles confirmed equivalent; no manual split" and dropping the fallback from §3.3-step-4 (or keeping it as defensive but noting it's confirmed dead code). One caveat worth a test: Perl `split /\s+/` on a header with a **leading space after `>`** (`>  chr1`) yields a leading empty field then `chr1` (Perl returns `''` as token 0 → empty chromosome name!). noodles `splitn` on `"  chr1"` → first component is `""` → `from_str` maps empty name to `ParseError` (`and_then(|s| if s.is_empty() { None …})` then `MissingName`). **So Perl would store an empty-string chromosome name; noodles errors.** This is an extreme edge (malformed header) and arguably noodles' behavior is *better*, but it's a divergence. Document it; don't bother matching (not byte-gated for real genomes). 

Also note: `read_definition` reads into a **`String`** (`read_line(reader, buf: &mut String)`), so a non-UTF-8 header byte makes noodles error before you ever get a `Vec<u8>` name. The plan's `Vec<u8>` name rationale (non-UTF-8 fidelity, inherited from cram_ref.rs) is therefore **moot at the reader boundary** — names are always valid UTF-8 by the time noodles hands them to you. Keeping `Vec<u8>` is still fine (cheap, matches cram_ref house style, avoids a re-encode), just don't claim it buys non-UTF-8 support in Phase A — it doesn't, given this reader. Optional doc tweak.

### 5.2 flate2 `MultiGzDecoder` vs noodles native gz

cram_ref.rs uses noodles `build_from_path`, whose test (`reconstitute_accepts_gzipped_fasta`) explicitly writes **BGZF** via `noodles_bgzf::io::Writer` — confirming noodles' `.gz` path is BGZF-oriented. Real Bismark genome `.fa.gz` files are produced by plain `gzip` (Perl `gunzip -c`, `:1681`), which is plain-gzip, NOT BGZF. So **`build_from_path` on a plain-gzip `.fa.gz` would fail or misread** — the plan is right to use `flate2::read::MultiGzDecoder` feeding a `fasta::io::Reader` instead. This is the correct alternative and the plan chose it. ✔ No change.

### 5.3 HashMap vs IndexMap

Covered in §2 — `HashMap` is correct. The alternative (`IndexMap`) would carry an unused dep and falsely imply genome order matters. Plan's choice is the better one.

### 5.4 `discordance: Option<u8>` type choice

Perl `discordance_filter=i` is a signed int validated to `1..=100`. `u8` is fine for the value range, but: (a) negatives error at clap-parse (exit 2) not validate (exit 1) — divergent exit code vs Perl die; (b) `101..=255` reach `validate` and hit `DiscordanceOutOfRange` (good); (c) `>255` errors at parse. All garbage-in cases, STDERR not gated. Acceptable. The only thing to confirm: the plan stores `discordance: Option<u8>` but the **discordant-filter comparison in Phase D uses `%.6f` percentages and compares `abs(top-bottom) > N`** (SPEC §9) — `N` as `u8` widened to f64 is fine. No Phase-A action. (Optional: a one-line comment that `u8` is deliberate and the negative/overflow divergence is accepted.)

---

## 6. Action items

### Critical (fix before implementation)

- **C1 — Context-conditional `output_stem` strip.** Plan §3.2 step 11 strips *both* `.CpG_report.txt` AND `.CX_report.txt` unconditionally. Perl `handle_filehandles:107-112` strips **only one**, gated on `$CX_context`: `if ($CX_context) s/\.CX_report.txt$//` **else** `s/\.CpG_report.txt$//`. Consequence of the plan-as-written: in default (CpG) mode with `-o foo.CX_report.txt`, Perl leaves the stem as `foo.CX_report.txt` and produces `foo.CX_report.txt.CpG_report.txt`, whereas the plan would strip to `foo` → `foo.CpG_report.txt`. **Byte-divergent output filename.** Fix: make the strip conditional on `cx_context`, matching Perl exactly. Update V7 to assert the *cross* cases: `(default mode, -o foo.CX_report.txt)` ⇒ stem stays `foo.CX_report.txt`; `(--CX, -o foo.CpG_report.txt)` ⇒ stem stays `foo.CpG_report.txt`. (Minor sub-note: Perl's regex `\.CpG_report.txt$` has an unescaped `.` before `txt` — matches any char, so `.CpG_report_txt` would also strip. Negligible; a literal `.strip_suffix(".CpG_report.txt")` is acceptable and arguably more correct, but document the deliberate divergence.)

- **C2 — `output_dir` default is `''`, not CWD.** Plan §3.2 step 11 says "dir/parent_dir → CWD when None." Perl `:2108-2110` sets `$output_dir = ''` (empty string) when `--dir` is absent — used as a literal path prefix `"${output_dir}$file"`. `parent_dir`, by contrast, *does* default to `getcwd()` (`:2070-2071`). These are different defaults. Resolving `output_dir` to CWD changes the prefix from `""` to an absolute path; after Perl's `chdir` dance the *effective* write location is the same, but the `ResolvedConfig` field semantics (and any logging/derivation that interpolates it) differ. Fix the plan: `output_dir` default = empty/`None`-meaning-no-prefix (or replicate Perl's exact absolute-path-after-chdir behavior in Phase B and keep Phase A's field as the raw `Option`); `parent_dir` default = CWD is correct. At minimum, split the two so they don't share "⇒ CWD."

### Important (fix or explicitly accept before implementation)

- **I1 — `--CX` flag surface.** `-CX` is not a valid clap short flag (parses as bundled `-C -X`). Model as `#[arg(long = "CX_context", visible_alias = "CX")]` (drop the single-dash short, or confirm-and-document Perl's Getopt leniency). Add V2 assertion that `--help` lists `--CX_context` and `--CX`. (§4.2)

- **I2 — Empty-file-in-winning-tier test + contract.** A `touch chr0.fa` makes Perl die ("not in FASTA format") AND suppresses the `.fa.gz` tier. Confirm the plan's glob means "tier with ≥1 filename wins" (filename-list truthiness, matching Perl), and decide whether to match Perl's die on the empty file's empty header. Add a test: `{empty chr0.fa, populated chr1.fa.gz}` ⇒ `.fa` tier wins, `.fa.gz` NOT read; empty `chr0.fa` ⇒ error (not silent skip). (§1.4, §4.1, §4.3-#1)

- **I3 — Trailing-description drop test.** Add an explicit assertion (extend V11) that `>chr1 trailing description` stores name `chr1` and the description bytes are gone. Resolves the noodles open Q with a regression lock. (§4.1, §5.1)

- **I4 — Malformed-header error test.** `>` with no name, or a first line not starting with `>`, must error (variant may be `Io` from noodles, not a c2c-typed error). Add a test so the divergence from Perl's typed die is a *known* behavior. (§1.4)

### Optional (nice-to-have; record the decision)

- **O1 — Resolve the noodles `record.name()` open Q in the plan** to "confirmed up-to-whitespace; no manual fallback needed" and drop the fallback language (or mark it confirmed-dead). (§5.1)
- **O2 — Drop the redundant manual `\r` strip** from §3.3-step-5 (noodles already strips trailing CR in def + seq); add a CRLF-file test to lock noodles' behavior. (§4.2)
- **O3 — Empty-sequence-record test** (`>chr1\n>chr2\nACGT`) — Perl stores empty, noodles yields empty Sequence; pin parity. (§4.2)
- **O4 — BGZF `.fa.gz` test** proving `MultiGzDecoder` reads BGZF too (copy cram_ref.rs's `noodles_bgzf::io::Writer` fixture). (§4.2)
- **O5 — Note the negative-`--coverage_threshold` / negative-`--discordance` exit-code divergence** (clap exit 2 vs Perl die exit 1; STDERR not gated, so acceptable). (§1.2, §5.4)
- **O6 — Note the no-positional-infile divergence** (clap exit 2 vs Perl print-help+exit-0). (§1.5)
- **O7 — Soften the `Vec<u8>`-name rationale**: noodles reads the header through a `String`, so names are always valid UTF-8 at the boundary; `Vec<u8>` is fine for house-style consistency but doesn't buy non-UTF-8 support in this reader. (§5.1)
- **O8 — Note that `Genome` deliberately exposes no insertion-order iterator** (only `names_sorted()`), so Phase B doesn't reach for genome order. (§2)
- **O9 — SPEC §11 sync** (the `IndexMap`→`HashMap` correction) as the plan already promises. (§2)

---

## 7. Cross-checks performed

- **Dep pins (Cargo.lock):** clap 4.5.30 ✔, thiserror 2.0.0 ✔, noodles-fasta 0.61.0 ✔, noodles-core 0.20.0 ✔ (note: 0.18.0 ALSO present transitively — pinning `=0.20.0` is correct and the one this crate should use), flate2 1.1.9 ✔, assert_cmd 2.0.16 ✔, predicates 3.1.2 ✔, tempfile 3.10.1 ✔, bstr 1.10.0 ✔. All match. Adding `bismark-coverage2cytosine` to `members` (edition 2024, rust-version 1.89) is the correct workspace setup; the `[workspace.package]` inheritance applies.
- **Pattern fidelity:** the `Cli`→`validate()`→`ResolvedConfig`, `disable_version_flag` + `version_string()`, exit-code (0/1/2), and `thiserror` enum patterns faithfully mirror `bismark-dedup::{cli,error,main}`. The `BismarkC2cError` variant set (§5) is idiomatic and matches dedup's style. ✔
- **noodles 0.61.0 source read:** `record/definition.rs` (name = splitn-2 on first whitespace), `io/reader.rs` (`read_line` pops `\n`+`\r`), `io/reader/sequence.rs` (`fill_buf` trims trailing CR, `consume_empty_lines`), `io/reader/records.rs` (`Records::next` → `parse()` → `Definition::from_str`). All claims about noodles behavior in this review are source-verified.

**Report path:** `/Users/fkrueger/Github/Bismark-c2c/plans/05292026_bismark-coverage2cytosine/phase-a-scaffold-cli-genome/PLAN_REVIEW_B.md`
