# PLAN_REVIEW_A — Phase 4: minimap2 wrapper

**Reviewer:** A (independent, fresh context). **Date:** 2026-06-05.
**Plan reviewed:** `phase4-minimap2-wrapper/PLAN.md` (rev 0).
**Verification basis:** Perl oracle `bismark` v0.25.1 (worktree copy, 10027 lines), the Phase-3 spike, and the shipped Rust crate `rust/bismark-aligner` (`align.rs`/`options.rs`/`config.rs`/`convert.rs`/`merge.rs`/`aligner.rs`).

## Verdict

**APPROVE WITH CHANGES.** The plan's central load-bearing claim — *"the merge/MAPQ core needs NO change because minimap2 → `second_best=None` and `calc_mapq` is fed the same `(0,-0.2)` inputs"* — is **CORRECT and verified against the Perl source** (parse loop 2772-2796, scoring block 7891-7955, `calc_mapq` 3923-3955). The plan also correctly overrode the Phase-3 spike's mistaken `s2:i:` claim (the spike said "the merge MUST read `s2:i:`"; the Perl has no `s2:i:` branch, so the plan is right and the spike was wrong on that point — good catch).

However, there is **one Important factual error** in the `/1` SE convert delta that, if implemented as written, would BREAK the SE gate, plus a few clarifications needed. None are fatal to the approach; all are fixable in rev 1 before implementation.

---

## 1. Logic review — load-bearing claims, verified

### ✅ Claim 1 (CENTRAL): "merge/MAPQ need NO change" — VERIFIED CORRECT
- **Parse loop (Perl 2772-2796):** confirmed there is **no `s2:i:` branch**. `ZS:i:`→`second_best` (2780, unconditional) and `XS:i:`/`ZS:i:`→`second_best` (2787-2793, gated `if ($bowtie2)`). minimap2 emits neither `XS` nor `ZS` (spike Q4), so for a minimap2 line `$second_best` is **always undef**. The Rust parser (`align.rs` 97-108) strips only `AS:i:`/`XS:i:`/`ZS:i:`/`MD:Z:` → `second_best=None` for minimap2. **Match.** (And even in the hypothetical that minimap2 emitted `ZS`, both Perl 2780 and Rust 102 would capture it identically — no divergence either way.)
- **SE backfill:** with `$second_best` undef, the merge takes the "no second best hit" branch (Perl 2915-2927) → `alignment_score_second_best = undef`; for a unique alignment it is copied undef (3040) → `calc_mapq` receives `undef` for `AS_secBest` → the `!defined $AS_secBest` ladder (3947-3954). **Match** with the existing Rust no-2nd-best path.
- **`calc_mapq` inputs (3132-3136, 3923-3955):** `calc_mapq` uses globals `$score_min_intercept`/`$score_min_slope`. **Crucial verified subtlety:** the "BOWTIE 2/HISAT2 SCORING OPTIONS" block (7891-7955) is a *comment-only* section header — the code is **NOT gated by any aligner conditional**, so it runs for minimap2 too. Default global path (7951-7953) sets `(0,-0.2)` AND pushes `--score-min L,0,-0.2`. The minimap2 block then does `@aligner_options = ()` (8359), which **wipes the pushed `--score-min` flag but leaves the scalars `(0,-0.2)` intact**. So `calc_mapq` gets `(0,-0.2)` for minimap2 — exactly what Rust `options::score_min_params` returns for `cli.score_min == None` (options.rs 266), aligner-independently. **Match.** This is the most non-obvious link in the chain and the plan got it right; recommend the implementer add a code comment citing the wipe-after-set mechanism so a future reader doesn't "fix" it.

### ✅ Claim 2: clean-slate options (Perl 8359-8413) — VERIFIED COMPLETE + ORDERED
Assembly is exactly, in order: `-a` (8362) → `--MD` (8365) → `--secondary=no` (8368) → `-t 2` (8372) → `-x <preset>` (8387 `sr` / 8396 `map-pb` / 8403 `map-ont` default) → `-K 250K` (8413). Plan's V2 literal `-a --MD --secondary=no -t 2 -x map-ont -K 250K` is correct. Preset-conflict dies (8376/8379/8392) and non-minimap2-preset dies (8330-8341) confirmed. **No option dropped.**
- **Verified the clean-slate is truly clean:** the parallelization block (`-p`/`--reorder`, 7993-7999) and the PE block (`--no-mixed`/`--no-discordant`/`--dovetail`, 8044-8056) push to `@aligner_options` at lines 7992-8066 — all **before** the `@aligner_options=()` reset at 8359 — so minimap2 receives none of them. This is correct and the plan's `kind`-gated clean-slate matches.

### ✅ Claim 3: positional spawn (Perl 7025/7022) — VERIFIED
`$minimap_commandline = "$path_to_minimap2 $mm2_options $mmi $reads"` (7025), `$mmi = $bisulfiteIndex.".mmi"` (7022). No `--norc`/`--nofw` (7011-7016 commented), no `-x`/`-U`/`-1`/`-2`. The current Rust `AlignerStream::spawn` (align.rs 166-184) is hardwired to `<opts> <orient.flag()> -x <index> -U <input>`; the plan's per-aligner spawn shape (drop orient/-x/-U, pass `<basename>.mmi` positionally) is the correct delta. The options-tokenizer `options.split_whitespace()` (align.rs 174) correctly splits `-t 2`/`-x map-ont`/`-K 250K` into separate argv tokens, matching Perl's `join(' ')`→shell-split. **Bowtie2/HISAT2 paths untouched if gated on `kind`.**

### ✅ Claim 4 (PART): `.mmi` single-file discovery + bare version parse — VERIFIED
- `.mmi` single file: Perl 7022 confirms `<basename>.mmi`. Plan's `index_suffixes(Minimap2)=["mmi"]` correct.
- Version parse: Perl 7081-7084 does **nothing** for minimap2 → `$aligner_version` stays the raw `chomp`ed `minimap2 --version` output (the bare `2.31-r1302`). The existing Rust `parse_bowtie2_version` (aligner.rs 128-130) does `line.split("version").nth(1)` which **won't match** the bare number — so a minimap2-specific parse (trim the first line) is genuinely required, as the plan states. **Note:** the version is warn-only (non-fatal) and never reaches the gated BAM/report body, so even a parse miss can't break byte-identity — low risk.

### ✅ Claim 6: 4-instance (non-dir/pbat) both-strand risk — ASSESSED, BOUNDED, GATE-DETECTABLE
The genuine residual risk. For Bowtie2/HISAT2 the strand flag IS applied (6750-6754 / 6810-6814: `CTreadCTgenome`/`GAreadGAgenome`→`--norc`, others→`--nofw`); for minimap2 it is commented out (7011-7016), so every instance aligns both strands. The SE merge (Perl 2737-2796) special-cases only `flag==4` (unmapped) and does **not** discard flag-16 reverse hits — strand assignment is by instance `index`, not by FLAG. The Rust SE merge matches: `merge.rs` 197-199 keys only on `is_unmapped()` (flag==4, `align.rs` 127-129), no flag-16 filter.
- **Key reasoning:** this both-strand behavior is **identical on both Perl and Rust** (both run minimap2 with the same absent strand flags and the same faithfully-ported merge). So it is NOT a Rust-vs-Perl divergence source — it is the same population fed through the same logic. The byte-identity gate WILL catch any mishandling. Gating non-dir/pbat SE (V9) is **sufficient** to exercise the 4-instance both-strand path empirically. The plan's framing ("gating non-dir/pbat SE catches it") is correct.
- **One caveat to add to the plan:** the lockstep merge assumes **one primary line per read per instance**. This holds only if `--secondary=no` truly suppresses secondaries AND minimap2 emits no supplementary (`SA`/flag-2048) lines that would land between reads and break the read-ID lockstep. The spike saw 0 secondary/supplementary on the CT instance at default `map-ont` for *directional SE*. For non-dir/pbat 4-instance, supplementary lines are still preset-dependent and untested. If a supplementary line appears, the lockstep parse (one `last_line` per ID) could mis-step. **Recommend V9 explicitly assert (in the gate harness) zero flag-2048/flag-256 records in the raw minimap2 output for all four instances** — fail loud if any appear, rather than relying on byte-identity to surface it indirectly.

---

## 2. Important factual error — the SE `/1` delta is WRONG against the oracle

The plan repeatedly states (§2 "convert.rs", §5.7, §6 Behavior, **V7**) that minimap2 SE should append a single `/1`, e.g. V7: *"minimap2 `/1` (not `/1/1`)"* and §5.7 *"minimap2 SE → `/1`"*. **This is incorrect.**

- The SE FastQ transform (`biTransformFastQFiles`, Perl **5489-5651**) appends **NO read-number suffix at all** — not `/1`, not `/1/1`. Lines 5584-5586: `chomp` → `fix_IDs` → re-append `\n`, then the C→T/G→A bodies are written with the **unmodified** identifier (5626/5631). There is no `s/$/\/1/` anywhere in the SE path.
- The `/1`/`/2` (mm2) vs `/1/1`/`/2/2` (others) distinction lives ONLY in the **PE** transforms: `biTransformFastQFiles_paired_end` 5945-5958 (the mm2 `/1`/`/2` branch the plan cites) and `biTransformFastAFiles_paired_end` 5418-5421 (`/1/1`).
- The Rust already matches: `convert.rs` 167-184 — SE converters pass `id_suffix = b""` ("**No read-number ID suffix (SE)**", line 177), and `pe_id_suffix` (`/1/1`,`/2/2`, lines 197-200) is only used by the PE converters.

**Consequence if implemented as written:** adding a `/1` to SE minimap2 reads would make the Rust SE read IDs differ from Perl SE read IDs → the read ID propagates into the BAM QNAME → **the V9 SE gate would FAIL** (or, worse, the implementer would "fix" it by also changing Perl-side expectations and ship a real divergence).

**Required correction (Important):** Re-scope the convert delta to **PE-only**. For SE minimap2, the converter needs **no change** (the existing `b""` suffix is already correct). Drop the "SE → `/1`" assertion from V7; replace with "SE minimap2 read ID is unmodified (no suffix), same as Bowtie2/HISAT2 SE". The genuine PE delta (thread `Aligner::Minimap2` into the suffix choice so PE mm2 emits `/1`/`/2` not `/1/1`/`/2/2`) is correctly deferred to Phase 5 (OQ-4c) — keep it there, and state clearly that **there is no SE convert change**.

---

## 3. Minor clarifications / inaccuracies

1. **`--mm2_maximum_length` "must be ACTIVE" (§2):** slightly mischaracterized. The max-length **guard already exists and is wired** in `convert.rs` (`convert_fastq_impl` 332-333, fires whenever `opts.maximum_length_cutoff.is_some()`, and `ConvertOptions` already carries it, 39/52). What blocks it today is `config.rs` 205-207 erroring when `maximum_length_cutoff.is_some()` (because minimap2 is deferred). So the work is **remove the deferred-error gate**, not "activate" a guard. Default 10000 (Perl 8354) + the 100..100000 range die (8346-8351) must be ported into the un-deferred `resolve()`. (The Perl SE applies the cutoff at 5598-5603 — confirmed; Rust SE path goes through the same `convert_fastq_impl`, so it is already covered.)

2. **OQ-4d / `--multicore` (§3 item 9, OQ-4d):** strengthen the rationale with a verified fact: when `--parallel` is set, Perl pushes `-p $parallel` + `--reorder` (7998-7999) but the minimap2 clean-slate **wipes them** (8359). So minimap2 NEVER receives `-p`/`--reorder` from Bismark — it always runs at the hardcoded `-t 2`. `--multicore` therefore only affects Bismark's read-chunking/fork layer, never the minimap2 invocation. This makes the "expect worker-invariance" lean stronger than the plan states; recommend the plan note "minimap2 is invoked identically (`-t 2`) regardless of `--multicore`; only the read-split layer varies" so the worker-invariance gate is understood as testing the chunk/merge layer, not minimap2 threading.

3. **resolve_aligner deferred-error (§2 config.rs):** confirmed the current code path — `resolve_aligner` errors for `--minimap2` (config.rs 328-330) and there is a test `resolve_aligner_minimap2_still_deferred` (606-611) that must be flipped/replaced. The plan says "drop the deferred-error" — good, but also flag that the existing **test asserting the deferral** (and `resolve()`'s `maximum_length_cutoff` deferred-error test, if any) must be updated, or the suite will fail. Add to step 2.

4. **Spike contradiction acknowledgment (§11 Self-Review):** the plan should explicitly state that it **overrides Phase-3 spike findings Q4/§4/§6** (which claimed "the merge must read `s2:i:`"). Right now §2 quietly asserts the opposite of the spike without flagging that it's a correction. Make the override explicit so a reviewer/implementer doesn't trust the stale spike line and re-introduce an `s2:i:` parse branch. (This is a documentation hygiene item — the plan's conclusion is the correct one.)

---

## 4. Validation sufficiency

| Gap | Severity | Note |
|---|---|---|
| V7 SE `/1` assertion is wrong | **Important** | See §2 — must become "SE no suffix". As written, V7 would codify a divergence. |
| No assertion that minimap2 emits zero secondary/supplementary across all 4 instances | **Important** | See §1 Claim 6. Lockstep correctness depends on one-primary-per-read; `--secondary=no` covers secondaries but supplementary (flag 2048) is preset/population-dependent and untested for non-dir/pbat. Add a fail-loud check to the V9 harness. |
| `AS`/`MD` presence on every aligned minimap2 record | Optional | Plan §8 already flags "the merge dies otherwise — confirm in fake/gate". `--MD` is in the options (8365) and minimap2 emits `AS` for primaries; the Perl `die` 2838 already enforces it. Keep the assumption check. |
| V9 covers `map-ont` only (OQ-4b) | Acceptable | The `sr`/`map-pb` strings are unit-tested (V3) but not gated. Reasonable scope cut, but the plan should state that the **both-strand population is preset-dependent** (spike saw reverse+supplementary under `-ax sr`), so the supplementary-free assumption is only validated for `map-ont` — `sr`/`map-pb` remain unproven and should carry a "not byte-validated" caveat if ever exposed. |
| 1M determinism at scale (OQ-4e) | Covered | V9 includes 1M; spike confirmed multi-minibatch order-preservation at `-K 250K`. Good. |

**Could the gate pass while wrong?** Two scenarios:
- (a) If the SE `/1` is added per V7, the gate would FAIL (so it'd be caught — but it wastes a gate cycle). Fix V7 first.
- (b) A supplementary line appearing only in non-dir/pbat at scales >1M (not in the 10k/1M gate) could mis-step the lockstep silently. The explicit zero-supplementary assertion (above) closes this.

Otherwise the decompressed-SAM byte-identity gate (BAM body + report + aux), being the same gate that has held for Bowtie2/HISAT2, is strong; V10 (MAPQ implied by BAM identity) is sound.

---

## 5. Regression surface (V1 — two backends now frozen)

The plan's Self-Review correctly identifies that the clean-slate + per-aligner-spawn changes are the **biggest regression surface yet** (two byte-frozen backends). The `kind`-gating discipline is the right mitigation. Concur with the plan's V1 demand for a **HISAT2 PE cell + a Bowtie2 PE-dovetail cell** in addition to the unit suites. One addition: since the spawn refactor touches the shared `AlignerStream::spawn` (align.rs 166), V1 must include at least one Bowtie2 **non-directional** (4-instance, `--nofw`) and one HISAT2 cell to prove the `orient.flag()` path is unchanged — the SE-directional unit tests alone won't exercise the `--nofw` branch through the refactored spawn.

---

## 6. Efficiency / alternatives

- **Efficiency:** additive enum-dispatch + clean-slate branch + positional-spawn branch + single-`.mmi` suffix; zero hot-path cost. Concur with §6.
- **Alternative considered (spawn shape):** rather than branching `spawn()` internally on `kind`, consider a small `InvocationShape` enum (or two `spawn_*` constructors) so the Bowtie2/HISAT2 argv assembly is *physically untouched* and the minimap2 path is a separate function — reduces the risk of accidentally perturbing the frozen path inside a shared `if kind==Minimap2 {…} else {…}`. Minor; either works if `kind`-gated and unit-asserted (V5 already pins the minimap2 argv; recommend a companion V5b that pins the **Bowtie2** argv byte-for-byte through the refactored spawn to prove no drift).

---

## Action items

### Critical
- *(none — the approach is sound and the central merge/MAPQ claim is verified correct.)*

### Important
1. **Fix the SE `/1` delta (§2/§5.7/§6/V7).** Perl SE (`biTransformFastQFiles` 5489-5651) appends **no** read-number suffix; the Rust SE converter already matches (`id_suffix=b""`). Re-scope the `/1`/`/2` single-tag work to **PE-only** (Phase 5, per OQ-4c). Change V7 to assert "SE minimap2 ID unmodified (no suffix)". As written, V7 would codify a real divergence and fail the SE gate.
2. **Add a zero-secondary/zero-supplementary assertion to the V9 harness** for all 2/4 instances (raw minimap2 output: no flag 256/2048). The lockstep one-primary-per-read invariant — not just byte-identity — depends on it, and the non-dir/pbat 4-instance both-strand path is untested by the directional-only spike.
3. **Update the un-deferral tests** (`resolve_aligner_minimap2_still_deferred` config.rs 606-611, and the `--mm2_maximum_length`-deferred error 205-207) — these will fail once minimap2 is enabled; the plan's step 2 must call them out explicitly.

### Optional
4. Reword §2 "`--mm2_maximum_length` must be ACTIVE" → "remove the deferred-error gate; the convert-side cutoff guard + range/default validation already exist (convert.rs 332-333; port the 100..100000 die + default 10000 into the un-deferred `resolve()`)".
5. Strengthen OQ-4d: note that `-p`/`--reorder` are wiped for minimap2 (8359), so minimap2 always runs `-t 2` regardless of `--multicore` — the worker-invariance gate tests only the read-split/merge layer.
6. Make the **spike override explicit** in §11: state that Phase 4 supersedes the spike's "merge must read `s2:i:`" (Q4/§4/§6) — verified no `s2:i:` branch in Perl 2772-2796.
7. Add V5b: pin the **Bowtie2/HISAT2** argv byte-for-byte through the refactored `spawn` (regression guard) and include a Bowtie2 non-directional `--nofw` cell + a HISAT2 cell in V1.

---

**File:** `plans/06052026_bismark-aligner-v1x/phase4-minimap2-wrapper/PLAN_REVIEW_A.md`
