# PLAN REVIEW A — Phase 1 (CLI + options + genome/index discovery + aligner detection)

- **Reviewer:** A (independent, fresh context)
- **Plan reviewed:** `phase1-cli-options-discovery/PLAN.md`
- **Grounded against:** Perl `bismark` v0.25.1 (`sub process_command_line` 7247–8448, `sub ensure_the_aligner_is_working` 7060–7092, file-scope lines 26–47, `read_genome_into_memory` 5022–5126, `@PG` emission 8480/8482), SPEC §8, EPIC §5, Phase-0 SPIKE.
- **Verdict:** Plan is well-scoped and largely faithful, but **the `aligner_options` ordering in §3.8 is wrong/incomplete** (a wrong-output bug if implemented as written), and **two byte-identity-critical details (the `@PG CL:` source string and the FASTA glob priority/`@SQ` ordering) are under-specified.** Several CRITICAL items below must be fixed before implementation.

---

## 1. Logic review

### 1.1 CRITICAL — `aligner_options` assembly order in §3.8 is incomplete and the "ignore-quals last" claim is FALSE

The plan (§3.8 step 6, and §3 line 78) states `--ignore-quals` is *"always appended last (8012)"* and asserts the full default string is exactly `-q --score-min L,0,-0.2 --ignore-quals`. That assertion is correct **only for the default SE-directional FastQ case** — but the plan presents the 6-step order as *the* assembly order, and it is neither complete nor correctly ordered against the Perl source. The actual `push @aligner_options` sequence in `process_command_line` is:

1. `-f` / `-q` (7811/7816/7822)
2. `--phred33` (7845) / `--phred64` (7853) — **the plan omits these from the §3.8 ordered list entirely** (mentioned nowhere in §3.8; only `--score_min`, seed/effort, rdg/rfg, parallel, ignore-quals appear). They are pushed *before* `-N`/`-L`.
3. `-N` (7864), `-L` (7873)
4. `-D` (7882), `-R` (7888)
5. `--local` + `--score-min …` (7904/7905/7913/7922/7944/7948/7953)
6. `--rdg` (7968), `--rfg` (7984)
7. `-p` + `--reorder` (7998/7999)
8. **`--ignore-quals` (8012)** ← the plan calls this "last", but it is NOT last:
9. `--no-mixed`, `--no-discordant`, (`--dovetail`) (8044/8045/8056) — PE only
10. `--minins` (8125), `--maxins`/`--maxins 500` default (8131/8135) — PE only; **note the `--maxins 500` default is pushed for *every* PE run**
11. `--quiet` (8141)
12. HISAT2/minimap2 blocks (8295–8413) — out of v1 scope

So `--ignore-quals` is appended at 8012, **before** the PE flags, `--minins/--maxins`, and `--quiet`. For the v1 SE-directional FastQ default this is invisible (none of 9–11 fire), so the §3.8 assertion holds — but the plan's stated *rule* ("ignore-quals always last") is wrong and will mis-order options the moment `--quiet` is supplied even in SE mode (`--quiet` is not PE-gated; lines 8139–8142 fire for SE too), producing `… --ignore-quals --quiet` in Perl vs `… --quiet --ignore-quals` if a Rust dev implements "ignore-quals last" literally. **`--quiet` is a v1-reachable SE option and would break byte-identity of the Bowtie2 invocation** (and, if the invocation is ever surfaced in a report/`@PG`-adjacent string, the BAM). The plan must replace "append last" with the exact Perl push order and explicitly enumerate `--phred33/--phred64` and `--quiet` as v1-wired SE options.

Cross-ref: §9 validation table has no case for `--quiet`, `--phred33/64`, or option-order beyond the bare default — see §4.

### 1.2 CRITICAL — version-parse regex in §3.7 does not match Perl

Plan §3 line 63: *"Parse the version triple from `bowtie.*version (\d+\.\d+\.\d+)`."* The Perl regex (7078) is:

```perl
$aligner_version =~ /bowtie.*\s+version\s+(\d+\.\d+\.\d+)/;
```

The plan's pattern drops the `\s+` anchors around `version` and uses a single literal space. Bowtie2's `--version` first line is `…/bowtie2-align-s version 2.5.5` — a single space, so the plan's pattern *happens* to match here, but: (a) it is not the source regex, and (b) the Perl `.*` is greedy across the whole (multi-line, but `chomp`ed to first read — actually backtick captures full multi-line output, and `=~` without `/m` matches across the first line only up to first newline for `.`; `.*` won't cross `\n`). The risk is low for the pinned 2.5.5 string, but since this value only feeds a *warning* (not the gate) the bigger issue is **the plan should cite the exact Perl regex verbatim** so the implementer doesn't invent a looser/stricter one. Minor wrong-output risk, but flag it Critical-adjacent because it is a documented divergence from the cited line.

### 1.3 CRITICAL — `@PG CL:` source string under-specified (byte-identity critical for the gate)

§3.1 says "preserve the raw, ordered argument vector … store it verbatim in `RunConfig.argv`" and §8 says "`@PG` reconstructed from argv". But the Perl truth (lines 28, 32, 8480) is more specific and the plan does not pin it:

- `$command_line = join (" ", @ARGV)` is captured at **file scope, line 32**, *before* `GetOptions` consumes options. So `CL:` = **all original args joined by single spaces, EXCLUDING the program name** — the literal word `bismark` is prepended in the print: `CL:"bismark $command_line"` (8480).
- Therefore `RunConfig.argv` must be the args **without** `argv[0]`, and the `@PG` builder must emit `bismark ` + `args.join(" ")` wrapped in literal double quotes. The plan's `argv: Vec<String> // verbatim` is ambiguous about whether `argv[0]` is included; if the Rust binary stores `std::env::args()` (which includes `bismark_rs` as `argv[0]`), the `CL:` line will be wrong. **State explicitly: store args[1..], reconstruct `CL:"bismark <args[1..] joined by single space>"`.**
- **Edge case the plan misses:** lines 36–40 rewrite `--solexa1.3-quals` → `--phred64-quals` in `@ARGV` *after* line 32 captured `$command_line`. So the stored `CL:` preserves the **original** `--solexa1.3-quals` spelling while the parser sees `--phred64-quals`. The plan does not list `--solexa1.3-quals` at all (it's not in the GetOptions block because of the dot-rewrite). For byte-identity of `@PG` when a user passes `--solexa1.3-quals`, the Rust port must (a) capture argv before any rewrite, and (b) perform the same pre-rewrite for parsing. Low-frequency, but it is a real byte-identity-of-header path. At minimum, document it as a known deferred edge (it's a `@PG` concern, surfacing in Phase 5, but the *argv capture contract* is set HERE in Phase 1).

### 1.4 CRITICAL — FASTA discovery semantics in §3.6 are wrong (priority-fallback, not union) and miss `@SQ` ordering

§3.6 last bullet: *"Locate the raw genome FASTA(s) in `<genome>/` (`.fa`/`.fasta`, ±`.gz`) … just record paths now."* The Perl `read_genome_into_memory` (5031–5050) does **priority-ordered fallback**, not a union:

```
@f = <*.fa>;  unless(@f){ @f = <*.fa.gz>; } unless(@f){ @f = <*.fasta>; } unless(@f){ @f = <*.fasta.gz>; }
```

i.e. if any `*.fa` exists, `*.fasta`/`*.gz` are **never** considered. The plan's "`.fa`/`.fasta`, ±`.gz`" reads like a combined glob, which would (a) pick up `.fasta` files alongside `.fa` (Perl would not), and (b) silently change which/how-many files are loaded → different `@SQ` lines → BAM header diff. **Two further byte-identity hazards the plan does not name:**

- **Glob ORDER = `@SQ` order.** `%SQ_order` (5092/5122) is populated in glob iteration order, and that order becomes the `@SQ` header order in the BAM (SPEC §5: `@HD`/`@SQ` must match byte-for-byte). Phase 1 is where FASTA paths are discovered, so the *ordering contract* must be fixed here. The epic's own shared assumptions warn that glob case-fold/order was "a platform-specific contract [that] flip-flopped 3× on macOS before Linux CI settled it" — this is exactly that landmine, and the plan does not flag it for the FASTA glob. Even though loading is Phase 5, the discovery+ordering decision is a Phase-1 deliverable per §3.6/§4 (`genome_fastas: Vec<PathBuf>`).
- **Empty-result error message** (5048–5050) is a distinct die the plan's edge-case list does not include ("does not contain any sequence files in FastA format…").

**Action:** §3.6 must specify the four-tier priority fallback, that the resulting order is byte-significant (`@SQ`), defer the *order-correctness adjudication to Linux CI*, and add the no-FASTA die.

### 1.5 IMPORTANT — `--temp_dir` / `--output_dir` default is `''` (empty string → CWD-relative), not "the output dir"

§3.9 and §8 say *"`--temp_dir` defaults to the output dir (confirm against Perl)."* Perl (8200–8202, 8230–8232): when unset, **both** `$output_dir` and `$temp_dir` default to the **empty string `''`** (not the output dir, not CWD-absolute). When set, they are made absolute via `chdir`+`getcwd` and force a trailing slash, **creating the dir if absent** (8190/8220 `mkdir`). Downstream paths are built as `${output_dir}${file}` / `${temp_dir}${file}`, so empty-string means "current working directory, relative." The plan's assumption (temp_dir = output_dir) is **wrong** and §10 Q3 correctly flags it as open — but the answer is in the source: default = `''`. This affects temp-file paths (Phase 2) and is set in Phase 1. **Resolve §10-Q3 now: default empty/relative, mirror the `chdir`+`getcwd`+trailing-slash+`mkdir` behavior.** Note also `chdir $parent_dir` is performed before each of these resolutions (8177/8206) — the Rust port has no `chdir`-based CWD mutation, so it must replicate the *path arithmetic* (canonicalize relative to the original CWD) without actually changing process CWD.

### 1.6 IMPORTANT — index-presence check is a "warn-then-continue, die-at-end" pattern, not fail-fast; large-index branch has a latent Perl bug

§3.6 says "If any missing, check the large index before failing." The Perl small-index loop (7653–7669) **`warn`s per missing file and sets a flag but keeps checking** (it does not die mid-loop); only after both CT and GA loops does it branch to the large-index search (7672–7708). The large-index loop, however, **`die`s on the FIRST missing `.bt2l` file** (7686/7697) — meaning if the small index is incomplete AND the large index is also incomplete, Perl dies on the first missing large file with the per-file message, never reaching the final "Failed to detect either…" die (7706). This is arguably a Perl bug (the `$bt2_large_index_present = 0` after `die` is dead code), but **byte-identical *behavior* (which message, which exit) requires replicating it.** The plan's "emit the Perl message" is too vague about *which* of the three possible messages fires in which order. For a faithful port, decide: replicate the exact warn-stream + die-point, or (cleaner) emit one well-formed error — but that is a documented deviation. Flag for an explicit decision.

### 1.7 IMPORTANT — `--maxins 500` default is part of `aligner_options` for all PE runs

Lines 8133–8137: when not SE and `--maxins` unset, `--maxins 500` is pushed. The plan parses PE but defers wiring; however since §3.8 claims to assemble the full ordered string and §4 stores `aligner_options` in `RunConfig`, the plan should note that PE assembly (later phase) adds `--no-mixed --no-discordant [--dovetail] [--minins N] --maxins {N|500} [--quiet]` after `--ignore-quals`. Not a Phase-1 wiring item, but the §3.8 "ordered string" description is presented as complete and is not. (Same root cause as 1.1.)

### 1.8 MINOR — mutual-exclusivity replication is "functional," but Perl resolves aligner BEFORE library/format; precedence matters for *which* die fires

The plan §3.3–§3.5 lists aligner → library → format. Perl order is: aligner mutual-exclusion (7414–7437) → path resolution → **aligner version check (7506)** → SAM/CRAM/BAM (7517) → samtools (7549) → genome folder (7603) → **index discovery (7646)** → format (7803) → … → library/directional (8146). So the *aligner version check and index discovery happen before format/library resolution*. If two error conditions are simultaneously true (e.g. `--hisat2` AND a bad `--score_min`), Perl dies on the aligner one first. The plan's ordering (§3 numbered 3.3→3.10) is *close* but puts options assembly (3.8) before output (3.9) and library (3.4) before discovery (3.6) — the **library/directional resolution actually happens last in Perl (8146), after the entire options string is built and after PE/SE filename resolution.** For pure "first error wins" fidelity this matters. Recommend the plan add a note: "resolution/validation order follows Perl's lexical order in `process_command_line` so the first-failing check matches." Low risk because most invocations have ≤1 error, but it is a faithfulness gap.

---

## 2. Assumptions

| Plan assumption (§8/§10) | Verdict |
|---|---|
| Default options == `-q --score-min L,0,-0.2 --ignore-quals` | ✅ Correct for SE-dir FastQ default (Phase-0 + source 7822/7953/8012). But see 1.1 — it is not the *general* assembly rule. |
| Full surface parsed, only v1 spine wired; HISAT2/minimap2 → hard error | ✅ Sound and matches SPEC §8.4. But "v1 spine" must explicitly *include* `--quiet`, `--phred33/64`, `-N/-L/-D/-R`, `--score_min`, `--rdg/--rfg`, `-p/--reorder` as **wired SE options** (they all push to `aligner_options` in SE mode) — the plan lumps them under "seed/effort if supplied" without confirming they are wired (not just parsed). |
| Help/`--version` not byte-gated | ✅ Reasonable (Phase-0/EPIC agree only BAM is gated). |
| GetOptions auto-abbreviation not replicated, alias documented forms only (§10-Q1) | ⚠️ **Risk.** Perl `Getopt::Long` default *does* allow unambiguous abbreviation (`--geno` → `--genome_folder`). A real user/script relying on `--geno` would parse-fail in Rust → different exit/behavior. Not a *byte-identity-of-BAM* risk (the BAM is identical once it runs), so "alias documented forms only" is defensible — but the plan should note `--genome` is itself **not** a GetOptions key (only `genome_folder=s` exists at 7323; `--genome` works in Perl *solely* via abbreviation of `--genome_folder`). So the plan's "incl. `--genome`" alias is actually adding an abbreviation Perl supports only by its abbreviation engine. Document this explicitly. Keep as taken-assumption (Optional escalation). |
| `--temp_dir` default = output dir (§8, §10-Q3) | ❌ **Wrong** — default is empty string / CWD-relative (1.5). Resolve now from source, do not defer. |
| BAM-only for v1; SAM/CRAM parse-but-error (§10-Q2) | ⚠️ Mostly fine, but note Perl's *default* path sets `$bam=1` and **also probes for samtools** (7549–7592), falling back to `$bam=2` (gzip `.sam.gz`) if samtools absent. The Rust port writes BAM via noodles (no samtools needed for output), so the `$bam=2` gzip-fallback branch is **not** reproduced — that's an intentional architectural divergence (noodles vs samtools-pipe, per Phase-0 refinement A) and should be stated as a deliberate deviation, not silently dropped. Also: the samtools-pipe `@PG` line (Phase-0 refinement B) policy is still pending Felix — the plan correctly says it "does not affect Phase 1," which is true, but the **samtools presence/path resolution** (7549–7592) is Phase-1 territory and the plan does not mention it at all. If the gate ever reproduces the samtools `@PG` line, the samtools path resolved here feeds it. Flag as a gap. |

**Unstated assumptions the plan should surface:**
- The Rust port does **not** `chdir` (Perl mutates CWD repeatedly via `chdir $genome_folder` / `chdir $parent_dir` / `chdir $CT_dir`). All Perl path canonicalization is `chdir`+`getcwd`-based. The Rust equivalent must be `std::fs::canonicalize` *relative to the original invocation CWD*, and must replicate the **forced trailing slash** (`s/$/\//`) that every Perl path gets — because downstream string-concatenation (`${dir}file`) depends on it. The plan mentions "trailing-sep semantics match Perl's chdir+getcwd" for the genome folder only; it must apply to `output_dir`, `temp_dir`, `CT_dir`, `GA_dir`, and the index basenames too.
- `canonicalize` resolves symlinks; Perl's `chdir`+`getcwd` also resolves symlinks → behavior matches, but `canonicalize` errors if the path doesn't exist whereas Perl `chdir` fails→falls into `mkdir` for output/temp dirs. So for `output_dir`/`temp_dir` the Rust port must **not** use `canonicalize` (which requires existence) but replicate create-then-resolve.

---

## 3. Efficiency

§6 is accurate — argument parse + a handful of `stat`s + one `bowtie2 --version` subprocess; no genome load. No concerns. One micro-note: the index-presence check does 6 (or 12) `stat`s per converted dir; trivial. The plan correctly defers genome loading to Phase 5.

---

## 4. Validation sufficiency

The §9 table covers the headline cases but has **gaps against the highest-risk silent-wrong-output modes identified above**:

- **Missing (CRITICAL):** an `aligner_options`-ordering test with `--quiet` (SE) → expect `… --ignore-quals --quiet` (catches the 1.1 "ignore-quals last" bug). Add `--phred64` SE → expect `--phred64` positioned *before* `-N/-L` and before `--score-min`.
- **Missing (CRITICAL):** `--score_min` with a *malformed* functional form (e.g. `--score_min L,0` or `--score_min X,0,-0.2`) → expect the exact Perl die message (`… needs to be in the format <L,value,value> …`). §9 row 2 only tests the *happy* override `L,0,-0.4`. The regex is `/^L,(.+),(.+)$/` (7917) — note `(.+)` is permissive (accepts non-numeric), so `--score_min L,foo,bar` is *accepted* by Perl and passed to Bowtie2 verbatim; a Rust port that "validates numerics" would be *stricter* than Perl and reject input Perl accepts → divergence. Test this.
- **Missing (IMPORTANT):** `--rdg`/`--rfg` malformed (`--rdg 5` or `--rdg 5,x`) → Perl regex is `/^(\d+),(\d+)$/` (strict integers) → die. And valid `--rdg 4,2` → `--rdg 4,2` appears in options string (7968). §9 has no rdg/rfg row.
- **Missing (IMPORTANT):** FASTA priority-fallback test — a fixture dir containing **both** `genome.fa` and `extra.fasta` → expect ONLY the `.fa` file(s) discovered, in glob order (catches 1.4). Plus a no-FASTA dir → expect the "does not contain any sequence files in FastA format" die.
- **Missing (IMPORTANT):** `argv` / `@PG CL:` reconstruction test — given args, expect the `CL:` string `bismark <args joined by single space>` with `argv[0]` excluded (catches 1.3). Even if `@PG` emission is Phase 5, the argv-capture contract is testable now.
- **Missing (MINOR):** `.bt2l` large-index happy path — §9 row 4 tests missing small index but not the *successful fall-through to large index* (the `large_index: true` path). Add a fixture with only `.bt2l` files → expect success + `large_index == true`.
- **Missing (MINOR):** mutual-exclusivity dies: `--non_directional --pbat` (8149), `-1/-2 --se` (8023), `--phred33 --phred64` (8838), `-f -q` together (7804), `--bowtie2 --hisat2` (7416). §3 edge-cases mention some; §9 should have at least one combined-mode row to lock the messages.

Row 8 (version parse + pin-warn against real 2.5.5) is good but **environment-dependent** (needs bowtie2 on the runner). Recommend also a *unit* test that feeds a captured `--version` string (e.g. `bowtie2-align-s version 2.5.5\n…`) to the parser so the regex is testable without the binary present, and a `2.4.5` string → expect warn.

---

## 5. Alternatives

- **Generate the clap surface from a single source-of-truth table.** Since §3.2 wants all 60 options for help/argv fidelity but only ~12 wired, consider a table-driven enum (option → {parse-only | wired-SE | deferred-mode}) so the "v1-wired vs parsed-but-deferred boundary" (the plan's own stated remaining risk, §11) is enforced *structurally* — every deferred option routes through one "deferred to <phase>" error path, eliminating the silent-half-support risk. The plan's prose mitigation ("explicit deferred errors") is weaker than a typed boundary.
- **Do NOT validate `--score_min`/`--rdg` more strictly than Perl.** Perl's `(.+)` for score_min accepts garbage; resist the temptation to parse floats. Match the exact regexes (`/^L,(.+),(.+)$/`, `/^(\d+),(\d+)$/`) byte-for-byte, including the die strings. A stricter Rust validator is a *behavioral* divergence even if "more correct."
- **Path handling:** rather than `chdir`-emulation, build all paths as `original_cwd.join(user_path)` then a custom "ensure trailing slash" + lexical-or-existence canonicalize that mirrors Perl's getcwd (resolves `..`/symlinks for existing dirs, falls back to mkdir for output/temp). Encapsulate the trailing-slash rule once.
- **Consider capturing argv twice:** raw (pre any rewrite, for `@PG`) and normalized (post `--solexa1.3-quals`→`--phred64-quals` rewrite, for parsing) — mirrors Perl lines 32 vs 36–40 exactly.

---

## 6. Action items (prioritized)

### Critical (fix before implementation — wrong-output / wrong-header risk)
1. **Rewrite §3.8 to the exact Perl push order** (1.1): `-q/-f` → `--phred33/--phred64` → `-N` → `-L` → `-D` → `-R` → `--local`+`--score-min` → `--rdg` → `--rfg` → `-p`+`--reorder` → **`--ignore-quals`** → (PE: `--no-mixed`/`--no-discordant`/`--dovetail`) → (PE: `--minins`/`--maxins`|`--maxins 500`) → `--quiet`. Remove the false "ignore-quals is last" rule. Explicitly mark `--quiet`, `--phred33/64`, `-N/-L/-D/-R`, `--score_min`, `--rdg/--rfg`, `-p` as **v1-wired SE options** (they push in SE mode).
2. **Pin the `@PG CL:` contract in §3.1/§3.9** (1.3): store `args[1..]` (exclude program name), reconstruct `CL:"bismark " + args.join(" ")`; capture argv **before** the `--solexa1.3-quals`→`--phred64-quals` rewrite; document the rewrite (Perl 36–40) and the dual-capture.
3. **Fix §3.6 FASTA discovery to four-tier priority fallback** (1.4) (`*.fa` → else `*.fa.gz` → else `*.fasta` → else `*.fasta.gz`), state that glob ORDER is the byte-significant `@SQ` order (adjudicate on Linux CI, per the epic's glob-fold lesson), and add the no-FASTA die message.
4. **Cite the exact version regex** `/bowtie.*\s+version\s+(\d+\.\d+\.\d+)/` in §3.7** (1.2), and add a unit test feeding a captured version string.
5. **Add the missing validation rows** (§4): `--quiet` option order, `--score_min` malformed (with permissive `(.+)` semantics — don't over-validate), `--rdg/--rfg` malformed + valid, FASTA priority-fallback + no-FASTA, argv/`CL:` reconstruction.

### Important
6. **Resolve §10-Q3 from source now** (1.5): `--temp_dir` and `--output_dir` default = empty string (CWD-relative); when set → trailing-slash + create-if-absent + resolve-absolute; the Rust port replicates path arithmetic without `chdir`. Apply the forced-trailing-slash rule to `output_dir`/`temp_dir`/`CT_dir`/`GA_dir`/index basenames, not just the genome folder.
7. **Decide and document the index-presence error semantics** (1.6): replicate Perl's warn-per-missing + which-die-fires-first (incl. the large-index die-on-first-missing quirk), or emit one clean error as a documented deviation.
8. **Document the samtools-path resolution gap** (Assumptions table): Perl resolves a samtools path/`$bam`/`$bam=2` fallback here (7549–7592); the noodles port deliberately diverges (no samtools-pipe), but the samtools path may feed the pending samtools `@PG` policy — state it as a deliberate deviation, not an omission.
9. **Add `--maxins 500` PE-default note** to §3.8 (1.7) so the later PE phase doesn't drop it.

### Optional
10. Add a note in §3 that resolution/validation order mirrors Perl's lexical order in `process_command_line` so "first error wins" matches (1.8). In particular: library/directional resolution is **last** in Perl (8146), after the full options string + filename resolution.
11. Clarify §8/§10-Q1: `--genome` is supported by Perl only via GetOptions abbreviation of `--genome_folder` (no explicit `genome` key at 7323); adding it as an explicit alias is a deliberate, documented choice.
12. Consider the table-driven option-classification (§5 alternatives) to structurally enforce the v1-wired/deferred boundary (the plan's own §11 "remaining risk").
13. Add a `.bt2l`-only happy-path test and at least one mutual-exclusivity die test to §9.

---

## 7. Bottom line

The phase scope (parse full surface, wire v1 SE-directional spine, hard-error HISAT2/minimap2, parse-store the rest) is **sound** and matches SPEC §8. The `RunConfig` seam (§4) is a clean, complete-looking interface for Phases 2–10. But the plan has **three byte-identity-critical inaccuracies against the cited Perl source** — the `aligner_options` order (the "ignore-quals last" rule is false and `--quiet`/`--phred*` are omitted), the `@PG CL:` argv contract (program-name exclusion + pre-rewrite capture), and the FASTA glob (priority-fallback + `@SQ` ordering) — each of which would silently produce a wrong Bowtie2 invocation or wrong BAM header. None are hard to fix, but all must be corrected in the plan text before implementation so the implementer codes to the source, not to the plan's paraphrase. Recommend **revise-then-implement**, not implement-as-is.
