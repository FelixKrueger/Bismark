# PLAN_REVIEW_A — Phase 2: HISAT2 wrapper + byte-identity gate

**Reviewer:** A (independent; fresh context). **Plan:** `phase2-hisat2-wrapper/PLAN.md` (rev 0).
**Verdict:** Sound architecture, the "thin wrapper" thesis holds against the source — but the plan **under-specifies two real seams** (index-suffix arity, `parallel.rs`) and carries **one factually-wrong assumption** (OQ-2a PE option placement) that is pinnable *now* from Perl, not deferrable. Plus three smaller gaps (HISAT2 splice flags, `error.rs` wording, `ReportHeader` struct field). None are blockers; all are cheap to fix before implementation.

Crate read in full: `config.rs`, `aligner.rs`, `options.rs`, `discovery.rs`, `align.rs`, `lib.rs` (naming sites), `report.rs`, `methylation.rs` (N-op), `merge.rs`/`parallel.rs` (aligner coupling), `cli.rs`, `error.rs`, plus Perl `bismark` (`process_command_line`, index discovery, align fns, report lines) and the spike/SPEC/EPIC.

---

## 1. Logic

### 1.1 The wrapper thesis checks out where the plan says it does
- **`align.rs` spawn shape is genuinely aligner-agnostic.** `AlignerStream::spawn` / `PairedAlignerStream::spawn` (align.rs L166/L354) take the binary as a `&Path` param and emit `<opts> <orient> -x <index> -U <reads>` (SE) / `-1 -2` (PE). Perl drives HISAT2 with the *identical* shape (`$path_to_hisat2 $hisat2_options -x … -U …`, bismark L6818; `-1 … -2 …`, L6380). ✅ The plan's "use the resolved aligner binary; verify `ZS` parse" is correct — no new spawn code, just a binary path and (for clarity) renaming the `bowtie2:` param.
- **`ZS`-or-`XS` parse already present and tested.** align.rs L100-104 strips `XS:i:` *or* `ZS:i:`; tests `parse_negative_as_and_hisat2_zs` (L499) and `both_xs_and_zs_last_wins` (L507) already exercise it. ✅ The plan correctly treats this as *verify*, not *implement*.
- **N-CIGAR spliced extraction present.** methylation.rs handles `b'N'` in both CIGAR walkers (L189, L362) as a skipped region. ✅ "Already present; verify byte-equal" is accurate.
- **`merge.rs` has no aligner-specific branch** — confirms the merge/scoring core is reused unchanged for HISAT2 (the SPEC's claim).
- **`RunConfig.aligner` field already exists** (config.rs L128) — only the `Aligner` enum needs the `Hisat2` variant (L20-23 has only `Bowtie2`). So the plan's "thread `Aligner` into `RunConfig` (if not already)" over-states the work: the field is there; add the variant + populate it. The *consumers* (output naming, report, detection dispatch) are what must read it.

### 1.2 🔴 Seam-inventory gaps (the highest-value findings)

**(a) Index discovery is NOT a one-line `.ht2` extension swap — it is a different *arity*.** The plan §4 prose says "small: `.1.ht2`…`.8.ht2`" (correct: Perl L7739 lists **8** files `BS_CT.{1..8}.ht2`, and 8 `.ht2l`), but the proposed seam in §4 is only `index_exts(kind) -> &'static [&'static str]` returning the *extension string*. The actual code that must change is `discovery.rs::bt2_suffixes` (L88-98), which is **hardcoded to the 6 Bowtie2 suffixes** `{1,2,3,4,rev.1,rev.2}` and consumed by `first_missing` (L102) and `discover_genome` (L122-143). HISAT2 needs `{1,2,3,4,5,6,7,8}` (no `rev.*`), 8 files, with a `.ht2l` large fallback. `index_exts` returning `["ht2","ht2l"]` does **not** capture the suffix-set difference. **This is the single most under-specified seam.** Fix: make the suffix *list* per-aligner (e.g. `index_suffixes(kind, large) -> Vec<String>`), not just the extension. (Functional impact is low for the gate — the index already exists on oxy so discovery succeeds — but the *missing-index error path* and any unit test asserting "8 files" depends on getting the arity right, and the plan's V4 ("`.ht2`/`.ht2l` discovery; large fallback") will silently pass against a 6-suffix check if the author copies the Bowtie2 shape.)

**(b) `parallel.rs` is a 7th seam the plan omits entirely.** The `--multicore`/`--parallel` worker path (Phase 9b) hardcodes `_bismark_bt2` in **ten** places: temp names L406/L409/L458/L461 and final `derive_output_path` calls L685/L695/L728/L828/L840/L888. The plan's seam list (six bullets + align + methylation) never names `parallel.rs`, behavior §6 never mentions `--multicore`, and the gate (V8/V9) is implicitly `--parallel 1` (default). Consequence: a HISAT2 run with `--multicore N>1` would emit/consume `_bismark_bt2`-named temp + final files (wrong names; likely a "file not found" merge failure), and **nothing in the plan catches it**. The same `aligner_token(kind)` threading the plan applies to lib.rs L330/341/477/912/922/1014 must also be applied to those parallel.rs call sites. Either (i) thread the token through parallel.rs too, or (ii) explicitly scope `--multicore`+`--hisat2` OUT of Phase 2 with a fail-loud guard + a gate note. Silent wrong-naming is the worst outcome.

**(c) `error.rs` wording is Bowtie2-specific (not byte-gated, so flag as fidelity-only).** `FaultyIndex` (error.rs L42) says "the **Bowtie 2** index of the {converted}->converted genome seems to be faulty"; Perl's HISAT2 path says "The **HISAT2** index … faulty … Please run bismark_genome_preparation **--hisat2**" (L7743/7753/7791). The detector-not-working message (L55) likewise hardcodes "Bowtie 2 … --path_to_bowtie2". These are stderr, **outside** the SAM+report+aux gate, so byte-identity will *never* catch the mismatch — call it out as a known fidelity gap, not a gate item. Optional to fix; worth a one-line decision in the plan.

### 1.3 🔴 OQ-2a is factually wrong AND pinnable now (do not defer)
The plan's §3.3 says the HISAT2 delta is appended "after `--ignore-quals`, before the PE tail," and OQ-2a *assumes* "same relative position as SE." **Both are wrong, and the truth is a literal read of Perl, available today:**

- `--no-softclip --omit-sec-seq` is **not** appended in the SE/PE align functions. It is pushed onto `@aligner_options` in `process_command_line` at **L8314**, inside the `if ($hisat2)` block at **L8287-8317** — which runs *after* every other option push.
- The push order is total and unambiguous (`grep "push @aligner_options"`): format → phred → `-N`/`-L`/`-D`/`-R` → `--score-min` → `--rdg`/`--rfg` → `-p`/`--reorder` → **`--ignore-quals` (L8012)** → PE: `--no-mixed`/`--no-discordant`/`--dovetail` (L8044-8056) → `--minins`/`--maxins`/`--maxins 500` (L8125-8135) → **`--quiet` (L8141)** → **HISAT2: `--no-softclip --omit-sec-seq` (L8314)**.

So the **PE** HISAT2 string is `…--ignore-quals --no-mixed --no-discordant --dovetail --maxins 500 [--quiet] --no-softclip --omit-sec-seq` — the delta lands **last, after the PE tail and after `--quiet`**, not "before the PE tail." The SE case only *coincidentally* matches "after `--ignore-quals`" because SE has nothing in between. **Action:** rewrite §3.3 + OQ-2a as RESOLVED, and pin V2/V3 expected strings to "Bowtie2 string with `--no-softclip --omit-sec-seq` appended at the very end." This is a Critical correctness item because the plan's *current* V3 oracle would be derived from a wrong mental model.

Implementation note: the cleanest Rust shape is **not** an extra `kind` param wired into the middle of `build_aligner_options` (risk to the byte-frozen Bowtie2 order, see §3) — it is to append `" --no-softclip --omit-sec-seq"` to the *finished* options string when `kind == Hisat2`, exactly as Perl pushes it last. That also trivially preserves V1.

### 1.4 HISAT2 splice flags are parsed-but-unhandled (silent wrong-options risk)
`cli.rs` already parses `--no-spliced-alignment` (`nosplice`, L214) and `--known-splicesite-infile` (`known_splices`, L211). Perl (L8289-8307) pushes `--no-spliced-alignment` and/or `--known-splicesite-infile <path>` **before** `--no-softclip --omit-sec-seq` when set. The plan's option assembly handles only the default. If a user passes `--hisat2 --no-spliced-alignment`, the Rust string would diverge from Perl and the gate (default flags) wouldn't catch it. **Action:** either handle these two flags in the HISAT2 option assembly (faithful) or **fail-loud reject** them in HISAT2 mode in Phase 2 with a gate note (Perl already dies on `--no-spliced-alignment` in *Bowtie2* mode, L8319 — that reject is also currently missing in Rust `config.rs`, but that's pre-existing/out of scope). At minimum, enumerate the decision.

### 1.5 Smaller logic notes
- **`ReportHeader` needs a new field.** report.rs L64 hardcodes "Bismark was run with **Bowtie 2**…"; the struct (L34) carries `aligner_options` but **not** the aligner kind. The plan says "report.rs — … the line" but doesn't name the struct change. Perl has *three* aligner-specific report lines (Bowtie2 L1722/1846, HISAT2 L1728/1849) — add an `aligner: Aligner` (or a pre-rendered `aligner_name`) field and branch. Trivial, but list it.
- **`--basename` correctly drops the token already.** `derive_output_path` (lib.rs L529) uses `basename_suffix` (`.bam`, `_SE_report.txt`, …) which carries **no** `bt2`/`hisat2` token when `--basename` is set — matching Perl (the `_bismark_bt2`/`_bismark_hisat2` token only appears in the derived-name path). So the `aligner_token` only needs threading into the `default_suffix` argument, not `basename_suffix`. Good — but the plan should state this so the implementer doesn't accidentally inject the token into the basename branch.
- **`@PG` is aligner-independent** — Perl's Bismark `@PG` is `ID:Bismark VN:<v> CL:"bismark <argv>"` (output.rs L51-67; Perl L8480) with no aligner token. So the report line and `@PG` are correctly *separate* concerns, and the gate's `@PG` filter is unaffected by the HISAT2 token. ✅ (No action — confirms the plan's implicit assumption.)

---

## 2. Assumptions

- **Stated, validated:** determinism (spike ✅); SE option string (`-q --score-min L,0,-0.2 --ignore-quals --no-softclip --omit-sec-seq`, spike Q2 ✅, matches Perl); 2/4-instance strand model reused (merge.rs has no aligner branch ✅); invocation shape identical (Perl L6818/6380 ✅); indexes present on oxy (SPEC §6 ✅).
- **Stated, WRONG:** OQ-2a PE placement (§1.3 — pinnable now, lands last).
- **Stated, under-specified:** "add `.ht2`(+`.ht2l`)" hides the 6→8 suffix-arity change (§1.2a); "thread `Aligner` into `RunConfig` (if not already)" — field exists, only the enum variant + consumers are work (§1.1).
- **Implicit, unsurfaced:** (i) `--multicore`/`--parallel` + `--hisat2` interaction (§1.2b); (ii) HISAT2 splice flags (§1.4); (iii) non-gated error wording (§1.2c). All three should be made explicit (handle or fail-loud-and-note).
- **Reasonable:** OQ-2b (a small `aligner_token(kind)` helper threaded into existing call sites is the minimal-churn choice — agree; just remember parallel.rs is among the call sites). OQ-2c (FastA folded in, gated last — fine; FastA only changes `-q`→`-f`, which `build_aligner_options` already does aligner-agnostically).

---

## 3. Bowtie 2 byte-frozen — is V1 sufficient?
**Mostly yes, with one structural caveat.** V1 (full suite + Bowtie2 oxy gate, before *and* after) is the right guard, and the unit suite already pins the exact Bowtie2 option strings (options.rs tests L257-328) and output names (cli.rs integration asserts `reads_bismark_bt2*`). The risk is **how** the `kind` param is introduced:
- The plan's §4 signature adds `kind` *into* `build_aligner_options(...)`. If the implementer threads `kind` and adds a conditional *inside* the order-critical push sequence (options.rs L22-180), there's a real chance of perturbing the Bowtie2 order. **Mitigation (recommend the plan mandate it):** keep `build_aligner_options` producing the Bowtie2 string unchanged, then append `--no-softclip --omit-sec-seq` to the *finished* string iff `Hisat2` — mirroring Perl's last-push and making the Bowtie2 branch provably byte-identical (the `kind` only gates a trailing concat). This converts V1 from "hope the refactor didn't move anything" to "structurally cannot move anything."
- Same principle for `aligner_token`: branch only at the `default_suffix` argument site; never touch the `basename_suffix` or the shared formatting. With those two disciplines, V1 is sufficient.

---

## 4. Validation sufficiency — could the gate pass while wrong?
The biggest review question. Findings:

- **V10 (discard arithmetic) is an *observation*, not an *assertion that guards a wrong path*.** "unique-best − discards == BAM records" is an identity that holds *by construction* of how the report counters and the BAM writer are driven (the same code path computes both); it would hold even if the genomic-seq extraction were subtly wrong, because the *discard* is counted by the same guard that suppresses the record. It is a useful smoke check (catches a gross counter/writer desync) but it is **not** evidence that the spliced-`N` XM call is correct. Treat it as a sanity cross-check, not a correctness guard, and do not let it substitute for V6.
- **The spliced-`N` path is the real "pass-while-wrong" exposure.** The gate's directional 10k/1M cells exercise only ~12/8360 spliced records (spike Q4), all from one real dataset. A subtly-wrong N-op de-conversion (e.g. off-by-one in the skipped-region genomic-seq window) could (a) be byte-masked if those 12 records happen to be CpG-free in the skipped flank, or (b) simply not be hit by an untested CIGAR shape (e.g. `M…N…M…N…M`, multiple introns; spliced reads on the GA/OB strand; spliced *and* indel in the same read). **Recommend:** the V6 fake should not be a single canned spliced record — it should cover (i) a multi-`N` CIGAR, (ii) an `N` adjacent to an `I`/`D`, and (iii) a spliced record on a GA-converted (OB) RNAME — each XM-asserted against a hand-computed expectation, *plus* the oxy 12-record path. The plan currently lists "fake spliced record + oxy 12-record path" (singular) — broaden it.
- **`ZS` multi-mapper (V5) is adequately targeted** by a fake emitting a `ZS:i:` 2nd-best, but assert the *consequence*: that the merge's best-vs-second decision and the resulting **MAPQ** (`calc_mapq`) are byte-identical, not just that `second_best` parses (the parse is already unit-tested). A multi-mapper whose `ZS` equals `AS` (true tie → ambiguous) vs `ZS < AS` (unique-but-repetitive → MAPQ shift) are different code paths; cover both.
- **Fake-binary naming:** the existing fakes are written as `bowtie2` (align.rs L588 `dir.join("bowtie2")`; cli.rs L53 `dir.join("bowtie2")`) and reached via `--path_to_bowtie2`. A HISAT2 fake must be named **`hisat2`** and reached via `--path_to_hisat2`, because detection resolves the binary by name (`resolve_bowtie2_path` joins literal `"bowtie2"`; the generalized resolver must join `"hisat2"`). The plan's step 8 says "a fake `hisat2`" — correct — but it should explicitly note the `--path_to_hisat2` wiring and the version banner must read `hisat2-align-s version 2.2.2` (so `detect_aligner` parses `2.2.2` and the pinned-version branch is the no-warning path). The existing `version` parser (`parse_bowtie2_version`, aligner.rs L91) already handles that line shape (`split("version")` → triple), so the generalized detector can reuse it verbatim — confirm in a unit test (plan step 2 already lists this ✅).
- **Coverage breadth is right but sequence it:** directional SE+PE 10k/1M first (V8) → non-dir/pbat (V9) → FastA last (OQ-2c). `--phred64` (options.rs L41, aligner-agnostic, FastQ-gated) and `.ht2l` (V4) are cheap unit/gate adds. Add **one** non-dir/pbat *gate* cell (not just an integration test) — the 4-instance HISAT2 path is where a strand-table or instance-plan assumption would surface, and only the oxy gate against real HISAT2 proves it.
- **Net:** the gate *can* catch the headline failure modes **if** V6 is broadened and V10 is demoted to a smoke check. As written (single spliced fake + V10 framed as a guard), a subtly-wrong spliced extraction could plausibly slip the directional gate and surface only on untested data.

---

## 5. Efficiency
No concerns. The change is additive enum-dispatch + a trailing string concat + per-aligner suffix list; zero hot-path impact; reuses the proven convert→instances→merge→output pipeline. The plan's §6 is accurate.

---

## 6. Alternatives
- **Append-don't-thread for options (recommended, §3):** post-concat `--no-softclip --omit-sec-seq` to the finished Bowtie2 string rather than threading `kind` into the push loop — strictly safer for V1 and a faithful mirror of Perl's last-push.
- **Per-aligner suffix list over `index_exts` (recommended, §1.2a):** return the full `Vec<String>` of expected index files per kind, so the 6-vs-8 arity is data, not a copied loop.
- **Scope `--multicore`+`--hisat2` out with a fail-loud guard for Phase 2 (acceptable alternative to threading parallel.rs, §1.2b):** if threading the token through 10 parallel.rs sites is judged out-of-budget, a one-line reject ("`--multicore` with `--hisat2` is not yet supported") is *far* better than silent `_bismark_bt2` naming — and it's honest about the gate's single-core coverage. Document whichever is chosen.
- **A `trait Backend` (SPEC §3 floats this) is over-engineering for HISAT2 alone** — agree with the plan's enum-dispatch + helper approach; revisit a trait only when minimap2 (Phase 4) adds the merge-adaptation divergence.

---

## 7. Action items (prioritized)

### Critical (fix before implementation)
1. **Correct OQ-2a / §3.3: `--no-softclip --omit-sec-seq` is appended LAST** (Perl `process_command_line` L8314, after the PE tail *and* `--quiet`), not "before the PE tail." Mark OQ-2a RESOLVED and pin V2 (SE) + V3 (PE) expected strings accordingly: PE = `-q --score-min L,0,-0.2 --ignore-quals --no-mixed --no-discordant --dovetail --maxins 500 --no-softclip --omit-sec-seq`. (§1.3)
2. **Add `parallel.rs` as a seam** (10 `_bismark_bt2` literals: L406/409/458/461/685/695/728/828/840/888). Either thread `aligner_token` through them, or fail-loud reject `--multicore`+`--hisat2` in Phase 2 with a gate note. Silent wrong-naming is unacceptable. (§1.2b)
3. **Re-spec index discovery as a suffix-*arity* change, not an extension swap:** HISAT2 = 8 files `BS_{CT,GA}.{1..8}.ht2` (+8 `.ht2l`), `discovery.rs::bt2_suffixes`/`first_missing`/`discover_genome` must take a per-aligner suffix *list*; `index_exts` alone is insufficient. (§1.2a)

### Important (resolve in the plan, then implement)
4. **Decide HISAT2 splice flags** (`--no-spliced-alignment`, `--known-splicesite-infile`): handle in option assembly (faithful, pushed before the softclip delta per Perl L8289-8307) or fail-loud reject in HISAT2 mode + gate note. Don't leave parsed-and-silently-ignored. (§1.4)
5. **Broaden V6:** the spliced-`N` fake must cover multi-`N`, `N`-adjacent-to-`I`/`D`, and a GA/OB-strand spliced record, each XM-asserted, *in addition to* the oxy 12-record path. Demote V10 to a smoke check (it's an identity, not a correctness guard). (§4)
6. **Mandate the byte-frozen-safe shape:** append the HISAT2 delta to the *finished* options string (not a conditional inside the push loop); branch `aligner_token` only at the `default_suffix` site (not `basename_suffix`). Strengthens V1 from "hope" to "structural." (§3, §1.5)
7. **Name the `ReportHeader` struct change** (add `aligner`/`aligner_name`; branch the "was run with …" line — Perl L1728/1849) and the **fake-binary naming** (`hisat2` via `--path_to_hisat2`, banner `hisat2-align-s version 2.2.2`). (§1.5, §4)
8. **Strengthen V5:** assert merge-decision + MAPQ byte-equality for both `ZS==AS` (tie→ambiguous) and `ZS<AS` (unique-repetitive→MAPQ) multi-mappers, not just that `ZS` parses. (§4)
9. **Add one non-dir/pbat HISAT2 oxy *gate* cell** (V9 as a real gate, not integration-only) — the 4-instance path is where a strand/instance assumption surfaces. (§4)

### Optional (fidelity / polish)
10. **`error.rs` wording** (FaultyIndex L42, detector-not-working L55) is Bowtie2-specific and **outside the gate** (stderr) — note as a known fidelity gap; fix to HISAT2 wording (Perl L7743/7791) if cheap. (§1.2c)
11. **Header/doc prose** in lib.rs (L4/L243/L810), config.rs `summary()` (L497 hardcodes "Bowtie 2"), align.rs error strings ("failed to spawn Bowtie 2") — non-gated; tidy opportunistically.
