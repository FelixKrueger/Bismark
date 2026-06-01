# PLAN_REVIEW_B — Phase 1 (CLI + options + genome/index discovery + aligner detection)

- **Reviewer:** B (independent, fresh context)
- **Plan reviewed:** `phase1-cli-options-discovery/PLAN.md`
- **Grounding:** EPIC.md, SPEC.md (§8 decisions), SPIKE_determinism.md, **and the Perl source `bismark`** — `process_command_line` 7247–8448, `ensure_the_aligner_is_working` 7060–7092, `read_genome_into_memory` 5022–5061, `merge_individual_BAM_files` 1390–1432.
- **Verdict:** The plan is well-scoped and the seam to later phases (`RunConfig`) is sound, but it contains **one byte-identity-critical error** (the `aligner_options` assembly order in §3.8 does not match the Perl append order) and **several material omissions** in the options string that *will* surface as wrong-output bugs the moment alignment is wired in Phase 3/5. These must be fixed in the plan before implementation, even though Phase 1 emits no BAM — because §3.8 explicitly asserts a fixed ordered string and that string is what Phase 3 hands to Bowtie2.

---

## 1. Logic review

### 1.1 CRITICAL — the `aligner_options` append order in §3.8 is wrong

The plan (§3.8) lists the order as:
`1. -q/-f → 2. --score-min → 3. -N/-L/-D/-R → 4. --rdg/--rfg → 5. -p/--reorder → 6. --ignore-quals`.

The **actual** Perl append order (verified line by line) is:

| # | Option | Perl line | Condition |
|---|--------|-----------|-----------|
| 1 | `-q` / `-f` | 7811 / 7816 / 7822 | always (format) |
| 2 | `--phred33` / `--phred64` | 7845 / 7853 | **only if supplied** |
| 3 | `-N <n>` (mismatches) | 7864 | only if supplied |
| 4 | `-L <n>` (seed_length) | 7873 | only if supplied |
| 5 | `-D <n>` | 7882 | only if supplied |
| 6 | `-R <n>` | 7888 | only if supplied |
| 7 | `--score-min L,i,s` | 7905/7913/7922/7944/7948/7953 | always |
| 8 | `--rdg r` | 7968 | only if supplied |
| 9 | `--rfg r` | 7984 | only if supplied |
| 10 | `-p <n>` then `--reorder` | 7998 / 7999 | only if `$parallel` (`-p`) |
| 11 | `--ignore-quals` | 8012 | always |
| 12 | `--no-mixed`, `--no-discordant`, `--dovetail` | 8044 / 8045 / 8056 | **PE only** |
| 13 | `--minins n` / `--maxins n` (PE default `--maxins 500`) | 8125 / 8131 / 8135 | PE only |
| 14 | `--quiet` | 8141 | only if supplied |

Two concrete divergences from the plan:
- **`--score-min` comes AFTER `-N/-L/-D/-R`, not before them** (Perl 7864–7888 precede 7905–7953). The plan has them reversed (its step 2 vs step 3). For the *default* SE invocation this is invisible (no `-N/-L/-D/-R` supplied), so the §3.8 default assertion `-q --score-min L,0,-0.2 --ignore-quals` still passes — which is exactly why this bug would slip through Phase 1's only assertion and then produce a non-byte-identical Bowtie2 invocation the first time a user passes `-N 1` or `-L 20`. **This is the highest-risk silent-wrong-output failure mode in the phase.**
- **`--phred33`/`--phred64` are entirely missing from §3.8's ordered list** (Perl 7845/7853), yet `RunConfig` (§4) carries a `phred` field. They belong at position 2, between format and `-N`.

**Action:** rewrite §3.8 to the 14-row order above, marking which entries are SE-relevant (1–11) vs PE-only (12–13) vs always-tail (14). Even though PE (12,13) and `--quiet` (14) are parsed-not-wired, the *string-assembly function* must emit them in this order or PE Phase 7 inherits a latent bug. Add unit assertions for at least: `-N 1` present invocation, `-L`+`-N` together (ordering), and a PE default (must end `… --ignore-quals --no-mixed --no-discordant --dovetail --maxins 500`).

### 1.2 IMPORTANT — `--maxins 500` is a default that is appended (PE), and `--dovetail` defaults ON for PE

The plan treats `--maxins` purely as a pass-through (§3.8 doesn't mention it). Perl 8133–8137: for **paired-end**, if `--maxins` is not given it pushes `--maxins 500` by default; for SE it pushes nothing. Likewise 8047–8056: for PE, unless `--no_dovetail`, `$dovetail` is auto-set and `--dovetail` is appended. The plan's §4 `RunConfig` does not carry a dovetail/maxins-default notion. These are PE-phase concerns, but since the plan claims to *assemble the full options string* and *parse-store PE*, the defaults should be captured now (or §3.8 should explicitly say "PE option tail deferred to Phase 7" rather than silently dropping it). Right now it's silently dropped — a half-support risk the plan's own §11 warns against.

### 1.3 IMPORTANT — output BAM-name derivation in §3.9 is oversimplified

§3.9 says "Compute the output BAM name the Perl way (`<basename-or-derived>_bismark_bt2.bam`)." The real derivation (`merge_individual_BAM_files` 1393–1422, mirrored in the `determine_…` block ~654–740) is:
1. strip path: `s/.*\///` (1396),
2. strip read-suffix: `s/(\.fastq\.gz|\.fq\.gz|\.fastq|\.fq)$//` (1398),
3. if `--prefix`: `$merged_name = "$prefix.$merged_name"` (1405–1407) — note the **literal dot join**, and `--prefix` itself first has trailing dots stripped (`s/\.+$//`, 8238),
4. append `_bismark_bt2.bam` (SE) / `_bismark_bt2_pe.bam` (PE) (1411/1426),
5. **`--basename` fully OVERRIDES**: `$merged_name = "${basename}.bam"` (1421) — it does *not* append `_bismark_bt2`. The plan's "`<basename-or-derived>_bismark_bt2.bam`" wording is wrong for the `--basename` case.

Also note the multi-input `.temp.N` naming used under multicore (654–740) is a different code path; Phase 1 only needs the single-file form, but the plan should cite 1393–1422 (not invent a formula) and flag the suffix-strip + prefix-dot + basename-override rules so Phase 1's `OutputTarget` records the *inputs* to this formula correctly. **Recommend Phase 1 store the components (pathless stem, prefix, basename, PE/SE) and defer the final string to the output phase**, rather than computing a name now that may drift.

### 1.4 IMPORTANT — genome-folder canonicalization uses `chdir`+`getcwd`, and the plan must replicate symlink resolution, not Rust `canonicalize()`

Perl 7623–7630: `chdir $genome_folder` then `$absolute = getcwd()`. `getcwd()` returns the **logical-or-physical** cwd; critically it does **not** necessarily resolve symlinks the way Rust's `std::fs::canonicalize` (which calls `realpath`) does. The plan §3.6 says "canonicalize" — if implemented as `Path::canonicalize`, a symlinked genome folder would yield a *different* absolute path than Perl's `getcwd()`, and that path is embedded in the `@PG CL:` line region indirectly and in warnings. The byte gate is on the BAM, and the genome path is **not** in the BAM records, so this is lower-risk than it first appears — but the plan should explicitly note "match `chdir`+`getcwd` semantics (no forced symlink resolution)" rather than reaching for `canonicalize()`, since the same trailing-slash/abs-path string is reused to build `CT_dir`/`GA_dir`/index basenames and the FASTA dir. The plan's parenthetical "ensure trailing separator semantics match Perl's `chdir`+`getcwd`" is on the right track but is in tension with the word "canonicalize" in the same sentence. **Pick one and be explicit; prefer the `getcwd`-equivalent (current dir after chdir) over `realpath`.**

The trailing-separator handling (Perl 7619–7621 then 7625–7627) is correctly identified.

### 1.5 The index-presence check logic is faithful, with one nuance to preserve

§3.6 correctly captures: check small `.bt2` first; if any missing, fall back to `.bt2l`; record `bt2_large_index`. Two nuances from 7646–7708 the plan should preserve exactly:
- The **small**-index loop only `warn`s per missing file and sets `$bt2_small_index_present = 0` (7655–7656) — it does **not** die; it continues checking all 12 files (CT then GA), accumulating warnings. The plan's wording "If any missing … emit the Perl message" is close but should note **all** missing files are warned, then the fallback fires once.
- The **large**-index loop actually `die`s inside the loop on the first missing file (7686), with `$bt2_large_index_present = 0` written as dead code after the `die`. So if the small index is incomplete AND the large index is also incomplete, the *first missing `.bt2l` file* dies immediately (7686) — you never reach the "Failed to detect either…" message at 7706 unless all six CT `.bt2l` exist but a GA one is missing... actually no: 7686 dies on the first missing CT `.bt2l`. This is a Perl quirk (inconsistent die-vs-warn between the two loops). The plan says "emit the Perl message ('…faulty or non-existant…')" which is the *warn* message; it does not capture that the large-index path dies on the *per-file* message (7686/7697), not the summary message (7706). For Phase 1's validation #4 (missing `BS_CT.3.bt2`), the small path warns then the large path dies — **the user-visible terminal error is the large-index per-file message, not the small-index one.** The plan's expected-output in §9 row 4 ("faulty or non-existant error") is ambiguous about which of the (nearly identical) messages fires. Worth pinning, since error-message text is part of "Perl-aligned."

### 1.6 Aligner detection (§3.7) — faithful, two small gaps

`ensure_the_aligner_is_working` 7060–7092 verified. Plan is faithful on: run `--version`, non-zero → die, parse `bowtie.*version (\d+\.\d+\.\d+)`. Note:
- The Perl regex is `/bowtie.*\s+version\s+(\d+\.\d+\.\d+)/` (7078) — requires whitespace around `version`. The plan's quoted regex `bowtie.*version (\d+\.\d+\.\d+)` drops the `\s+`. Minor (likely matches the same lines) but should be transcribed exactly.
- Perl's return-code check is `if ($return ne 0)` (7071) — a **string** comparison of the raw `system` return (which is `$? `, i.e. `exit_code << 8`). For Rust, the correct equivalent is "non-zero exit status," which the plan states. Fine.
- The **version pin → warning, not error** is a *deliberate deviation from Perl* (Perl 7060–7092 has no version pin at all). The plan flags this in §11 ("Adjusted during review"). This is a reasonable, well-documented deviation and is output-neutral (the warning isn't in the BAM). **Approve, but confirm the warning text is not asserted by any byte gate** (it isn't — stderr).

### 1.7 Read-layout / `@filenames` resolution (§3.5) — a missing existence check

Perl 8093–8120 checks each input file exists (`-e`) and dies "Supplied filename '…' does not exist" if not. The plan §3.5 resolves layout but §9 has **no validation row for a missing input read file**, and §3 edge cases don't list it. This is a real, common user error with a specific Perl message. Also Perl 8080/8087 normalizes `--se` separators: colons→commas (8080) and, for positional singles, spaces→commas (8087), then splits on comma (8089). The plan §3.5 ("positional singles → single-end") doesn't mention the `:`/space normalization or the comma-split — needed for multi-file SE byte parity (the per-file output names depend on it). **Add: input-file-existence validation + the `:`/space→comma normalization rule.**

### 1.8 PE same-file guard and `-1/-2` count match — not mentioned

Perl 8029–8038: PE requires equal counts of `-1` and `-2` files (8029) and dies if any `$mate1 eq $mate2` (8037). The plan §3.5 says "`-1` & `-2` → paired-end" and §3 edge cases cover "both -1/-2 and --se → error" (matches 8022–8023) but omit the equal-count and identical-file guards. Since PE is "parsed and stored" in Phase 1, these guards can be deferred — **but the plan should say so explicitly** (it currently silently omits them, which is the exact half-support risk §11 flags).

---

## 2. Assumptions

- **"Default options == `-q --score-min L,0,-0.2 --ignore-quals`" (§8, §3.8):** VERIFIED against Perl for the SE-directional-FastQ-no-flags case (format `-q` 7816; score-min `L,0,-0.2` 7952–7953; ignore-quals 8012; no PE tail because singles, 8134). Safe as a fixed assertion target. Good.
- **"`--temp_dir` defaults to the output dir" (§8, §10):** **UNSAFE / WRONG as stated.** Perl 8206–8232: if `--temp_dir` is not given, `$temp_dir = ''` (8231) — i.e. the **empty string**, which downstream resolves relative to the current/parent dir, **not** the output dir. Likewise `$output_dir` defaults to `''` (8201), not CWD. The plan's §8 assumption "`--temp_dir` defaults to the output dir (confirm against Perl)" and §10 open Q ("output dir vs CWD") are **both wrong** — the answer is "empty string ⇒ parent/CWD-relative, independent of output dir." This matters for where temp converted-FastQ files land (Phase 2) and is worth correcting now since it's an explicit open question. **Recommend: temp_dir default = '' (parent dir), output_dir default = '' ; only canonicalize when explicitly set (8178–8197 / 8207–8227).**
- **"Output default = BAM" (§8):** Partially right but missing the **gzip fallback**: Perl 8579–8591 — if no `--samtools_path` and `which samtools` fails, `$bam = 2` and output is `.sam.gz` (8589–8590). The plan's `OutputTarget` (§4) lists `bam/sam/cram` + `gzip` but doesn't encode the tri-state `$bam` (1 = real BAM via samtools, 2 = gzip fallback). For byte-identity on the oxy gate samtools is always present, so `$bam=2` won't fire there — low real-world risk — but the plan should record the samtools-presence resolution (and `$samtools_path`) since the `@PG` samtools-pipe line (Phase 5, pending policy) depends on the resolved samtools path. **Add `samtools_path` + the `$bam` tri-state to `RunConfig`/`OutputTarget`.**
- **"Alias only documented forms, incl. `--genome`" (§8, §10 open Q):** Note the GetOptions key is `'genome_folder=s'` (7323) — there is **no** `--genome` alias declared; `--genome` works in Perl *only* via GetOptions prefix-abbreviation of `--genome_folder` (and would be ambiguous if another `--genome…` option existed; none does, so `--genome`, `--genom`, `--gen` etc. all resolve). The plan proposes to add `--genome` as an explicit alias — that reproduces the common case but **diverges** from Perl for `--genom`/`--geno`/`--gen`. Since the gate is the BAM (not CLI ergonomics) and real pipelines use `--genome`, this is **acceptable**, but the plan should state that the *only* abbreviation it guarantees is `--genome` and that other abbreviations are intentionally unsupported. The risk is a user script using `--geno` that silently fails-to-parse in Rust where Perl accepted it — a usability regression, not a wrong-output bug. Acceptable to defer; keep the §10 open item.
- **clap vs GetOptions semantics:** A deeper assumption the plan doesn't surface: Perl GetOptions is **case-insensitive by default** for long options and supports `--opt=val` and `--opt val` and clustering differently than clap. clap derive is case-sensitive and stricter. For byte-identity this doesn't matter (BAM is the gate); for "accept the same command lines" it does. **Recommend the plan explicitly scope CLI-compat to "the documented invocations + `--genome`", not full GetOptions parity** — otherwise a reviewer later mistakes the divergence for a bug.

---

## 3. Efficiency

§6 is correct: argument parsing + a handful of `stat`s + one `bowtie2 --version` subprocess; no genome load (deferred to Phase 5). Nothing hot. One note: the plan correctly **defers** loading the FASTA (§3.6 "just record paths now"), which is right — `read_genome_into_memory` (5022) is the heavy step and belongs to Phase 5. No efficiency concerns.

One forward-looking nit: §3.6 says "Locate the raw genome FASTA(s)" and record paths. The Perl glob order (5031–5046) is a **precedence cascade**: `<*.fa>` first; only if empty, `<*.fa.gz>`; then `<*.fasta>`; then `<*.fasta.gz>` — it does NOT mix extensions. If Phase 1 "records paths" using a different discovery (e.g. globbing all four patterns and concatenating), the recorded set could differ from what Phase 5 will actually load, and the **glob order is byte-relevant** (it sets `@SQ` order in the header — `generate_SAM_header` iterates chromosomes in load order). The genome-prep port already litigated glob-case-fold ordering on macOS vs Linux (per EPIC §5 / MEMORY). **Recommend: either (a) don't record FASTA paths in Phase 1 at all — just verify ≥1 matching file exists and let Phase 5 own the canonical cascade — or (b) record them via the exact same cascade + sort that Phase 5/genome-prep uses.** Recording a divergent list now is a latent `@SQ`-ordering bug.

---

## 4. Validation sufficiency

The §9 table covers the default string, a `--score_min` override, basename derivation, missing index, missing bowtie2, hisat2/minimap2 rejection, argv capture, and version parse/pin. Good coverage of the happy path and the headline errors. **Gaps that map directly to silent-wrong-output or Perl-divergence risk:**

1. **No test for option-string ORDER with `-N/-L/-D/-R` present** — this is exactly the §1.1 bug's blind spot. The single default-string assertion (row 1) cannot catch the reversed score-min/-N ordering. **Add: assert `-N 0` + `-L 20` together produce `-q -N 0 -L 20 --score-min L,0,-0.2 --ignore-quals`.** (Critical — pairs with 1.1.)
2. **No test for the PE default tail** (`--no-mixed --no-discordant --dovetail --maxins 500`). Even if PE is "parse-store," the string builder runs; assert it. (Important.)
3. **No missing-input-read-file test** (Perl 8102/8117 die). (Important — §1.7.)
4. **No `--phred64`/`--phred33` placement test** and no "phred without -q dies" test (Perl 7842/7850). (Important.)
5. **No `--score_min` *malformed* test** beyond the implicit row-2 success case. Add a `--score_min 0,-0.4` (missing `L,`) → die-with-Perl-message test. Note the Perl validation is **shape-only** (`/^L,(.+),(.+)$/`, 7917) — it does NOT check that intercept/slope are numeric, so `--score_min L,foo,bar` is *accepted* by Perl. The plan §3.8 says "validate the `L,<i>,<s>` form" which is correct **only if** "form" means the shape, not numericness. **Pin this: replicate Perl's lax regex exactly; do NOT add stricter numeric validation** or you'll reject inputs Perl accepts (a divergence). (Important.)
6. **No `--bowtie2`+`--hisat2` mutual-exclusion test** (Perl 7415/7426). §3.3 says "replicate functionally" but §9 has no row. Add one (exit ≠ 0). (Optional — low risk, but cheap.)
7. **No `--local` score-min `G,…` form test.** §3.8 item 2 says parse/validate `--local`. Since v1 is end-to-end, the safest behavior is to **reject `--local`** in Phase 1 (it's not on the v1 spine and changes the whole scoring/MAPQ path). The plan currently says "parse/validate `--local` per 7895–7954" but doesn't say whether `--local` is *wired* or *rejected*. **This is ambiguous and risky:** if `--local` is parsed-and-honored it flips `--score-min` to `G,20,8` and adds `--local` to the options (7943–7944) — a completely different alignment mode that is NOT in the v1 byte-identity scope. **Recommend: reject `--local` with a "deferred" error in v1, like hisat2/minimap2.** (Important — escalate; see §6 open-Q discussion.)

The validations are sufficient for the *discovery/detection* surface but **insufficient for the options-string surface**, which is the byte-identity-load-bearing part of this phase.

---

## 5. Alternatives

- **Don't compute the output BAM name in Phase 1 (§3.9).** Store the *ingredients* (pathless input stem, prefix-with-trailing-dots-stripped, basename, PE/SE, gzip/bam tri-state) and let the output phase apply `merge_individual_BAM_files`'s formula. This avoids encoding a formula now that the plan already states slightly wrong (§1.3). The `RunConfig` seam stays clean and the byte-relevant string is computed in one place, next to where the file is written.
- **Defer FASTA path recording to Phase 5 (§3.6).** As in §3 above — verify presence only, since the canonical cascade + ordering is a Phase-5/genome-prep concern with a known cross-platform footgun.
- **Reject (don't store) the non-spine modes that change `aligner_options`.** hisat2/minimap2 already hard-error. `--local`, `--non_bs_mm` (end-to-end only, 8433), `--slam`, `--icpc` either change scoring or downstream behavior. For Phase 1, "parse-store-but-error-at-use" is fine for *layout/library* (SE/PE/non-dir/pbat) because those don't perturb the v1-spine options string, but for anything that **mutates `aligner_options` or the scoring function** (`--local`, minimap2's full reset at 8359) the cleaner contract is **reject now**. The plan's blanket "parse the full surface, wire the spine" is right for most options but should carve out the scoring-mutating ones as reject-now to avoid a half-wired `--local`.
- **Consider a Perl-oracle test that diffs the assembled `aligner_options` string against the real Perl `bismark` for a matrix of flag combinations** (the plan mentions this "where feasible" in §5.9). Given §1.1, this is the single highest-value test in the phase — it would have caught the ordering bug mechanically. **Recommend promoting it from "where feasible" to a required validation** (it's cheap: Perl `bismark` prints "Summary of all aligner options:\t…" to stderr at 8421 — capture and diff).

---

## 6. The three §10 open questions

1. **GetOptions abbreviation** — Assumption (alias documented forms + `--genome`) is **safe for the byte gate** but is a usability divergence for rare abbreviations. Keep as Optional/open; document that only `--genome` is guaranteed. Not a wrong-output risk. **Leave as taken.**
2. **`--sam`/`--cram` in v1** — Assumption (BAM-only; SAM/CRAM error "not yet supported") is **safe**. Perl 7517–7546 shows SAM/CRAM are distinct output paths; the gate (SPEC §8 fork 2) is defined on decompressed BAM content. Erroring is the conservative choice. **Leave as taken — low risk, correctly assessed.**
3. **`--temp_dir` default** — Assumption ("defaults to the output dir") is **WRONG** (see §2). The real default is `''` (parent/CWD-relative), independent of `--output_dir`. **This open question should be CLOSED with the correct answer now, not deferred to implementation** — it's a one-line read of Perl 8230–8232. Not critical to Phase 1 output (no temp files written yet) but it's the kind of "confirm during implementation" item that, if confirmed wrong, mis-shapes the Phase 2 contract. **Escalate to: resolve before implementation; correct §8 + §10.**

None of the three needs to be **Critical**, but #3's stated assumption is factually wrong and should be fixed in the plan text.

---

## 7. Action items

### Critical (fix before implementation — byte-identity-load-bearing)
- **C1.** Rewrite §3.8 `aligner_options` assembly to the verified 14-row Perl order (table in §1.1). The two specific fixes: **`--score-min` must come AFTER `-N/-L/-D/-R`**, and **`--phred33/--phred64` must be inserted at position 2** (between format and `-N`). The current order silently passes the default-only assertion but breaks the first time any seed/effort flag is supplied. (Perl 7811–8012.)
- **C2.** Add a validation that exercises the order with seed/effort flags present (e.g. `-N 0 -L 20` → `-q -N 0 -L 20 --score-min L,0,-0.2 --ignore-quals`). Promote the **Perl-oracle options-string diff** (capture Perl's "Summary of all aligner options:" stderr, 8421) from "where feasible" to required. Without this, C1's class of bug is untestable by §9.

### Important
- **I1.** Correct the `--temp_dir`/`--output_dir` default in §8 and §10: both default to **`''`** (parent/CWD-relative), NOT the output dir (Perl 8201, 8231). Close §10 open-Q #3.
- **I2.** Fix the output-name rule in §3.9 to match `merge_individual_BAM_files` 1393–1422: path-strip, read-suffix-strip (`\.fastq\.gz|\.fq\.gz|\.fastq|\.fq`), `--prefix` dot-join (after stripping the prefix's trailing dots, 8238), `_bismark_bt2.bam` append, and **`--basename` full override → `${basename}.bam`** (not append). Prefer storing ingredients and deferring the final string (§5).
- **I3.** Add the PE options tail to the string builder (or explicitly mark it Phase-7-deferred in §3.8): `--no-mixed --no-discordant --dovetail` (dovetail defaults ON unless `--no_dovetail`) + `--maxins 500` default. Add a PE-default string assertion.
- **I4.** Add missing-input-read-file validation (Perl 8102/8117) + the `--se` separator normalization (`:`→`,`, space→`,`, comma-split; Perl 8080/8087/8089) to §3.5 / §9.
- **I5.** Resolve the `--local` ambiguity: **reject `--local` in v1** (it flips score-min to `G,20,8` + adds `--local` — off the v1 byte-identity spine). Likewise gate `--non_bs_mm`/`--slam`/`--icpc` behavior. Update §3.8 item 2 and §3 edge cases.
- **I6.** Record `samtools_path` + the `$bam` tri-state (1 vs 2 gzip-fallback, Perl 8579–8591) and the resolved samtools path in `RunConfig`/`OutputTarget` — needed for the Phase-5 samtools-pipe `@PG` policy.
- **I7.** Clarify the large-index error path in §3.6/§9-row-4: the **large**-index loop *dies per-file* (7686/7697) with a near-identical message; the **small**-index loop only *warns* and accumulates. Pin which message the missing-`BS_CT.3.bt2` fixture should produce.
- **I8.** Add `--phred64`-without-`-q` die test (7842/7850) and a malformed-`--score_min` test, with an explicit note that Perl's `score_min` validation is **shape-only** (`/^L,(.+),(.+)$/`) — do NOT add numeric validation (would reject inputs Perl accepts).

### Optional
- **O1.** §3.6 FASTA discovery: defer to Phase 5 (presence-check only) or replicate the exact 4-pattern precedence cascade + ordering (Perl 5031–5046) to avoid a latent `@SQ`-ordering divergence.
- **O2.** Transcribe the bowtie2 version regex exactly: `/bowtie.*\s+version\s+(\d+\.\d+\.\d+)/` (the plan drops the `\s+`).
- **O3.** §8: state explicitly that CLI-compat is scoped to documented invocations + `--genome` (not full case-insensitive GetOptions/abbreviation parity), so later reviewers don't mistake the deliberate divergence for a bug.
- **O4.** Add a `--bowtie2`+`--hisat2` mutual-exclusion test row (cheap; Perl 7415/7426).

---

## 8. Summary

The plan's architecture (parse-full / wire-spine, `RunConfig` seam, defer genome load) is sound and well-aligned with the EPIC/SPEC decisions. The discovery and aligner-detection logic faithfully mirror Perl. **But the byte-identity-critical part of the phase — the `aligner_options` string — has a wrong append order (score-min before -N/-L/-D/-R; phred missing) that the phase's single default-only assertion cannot catch (C1/C2).** Secondary issues: the `--temp_dir`/`--output_dir` defaults are stated wrong (they're `''`, not the output dir — I1), the output-name and `--basename`-override rule is oversimplified (I2), the PE options tail and several input/score-min validations are missing (I3–I5, I8), and `--local` should be rejected rather than half-wired (I5). None block the *discovery* deliverable, but C1/C2 must be fixed before the string flows into Phase 3, and a Perl-oracle options-string diff should be the phase's anchor test.
