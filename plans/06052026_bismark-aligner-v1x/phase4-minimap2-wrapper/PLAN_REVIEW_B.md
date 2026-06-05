# PLAN_REVIEW_B — Phase 4: minimap2 wrapper (byte-identity)

**Reviewer B** · 2026-06-05 · independent / fresh context · adversarial.
**Target:** `plans/06052026_bismark-aligner-v1x/phase4-minimap2-wrapper/PLAN.md` (rev 0).
**Oracle audited:** repo-root Perl `bismark` v0.25.1 (line cites below). **Rust:** `rust/bismark-aligner` @ `49a1518`.

> **Re-run note:** this report was produced by a prior run that stalled on infra; this fresh re-run **independently re-verified every Critical and Important finding directly against the Perl source and the current Rust crate** before signing off. All cites below were confirmed by `sed`/`grep` on the live files (C-1 @ 6697-6708, C-2 @ 1845-1850 vs SE 1722-1728, I-1 @ 5945-5959 vs SE 5489+, I-2 @ 8344-8356, I-3 @ 5598-5604, I-4 spike Q4/§4, I-5 @ 8404-8408). Two corrections vs the prior draft are folded in (I-2 convert-drop already exists; I-4 the spike *actively contradicts* the plan).

**Verdict: REQUEST CHANGES.** The SE wrapper plan is sound and the central "merge is a no-op" thesis is **VERIFIED in the Perl source** for SE (and for PE too, but for a different reason than the spike implies). However the review surfaced **two Critical hazards the plan does not flag**, both about the **PE path** the plan promises to *implement here* (convert delta) and *gate at Phase 5*:

1. **The Perl PE minimap2 oracle is unfinished WIP** (`# TODO: Need to check this.` + an uncommented `warn`+`sleep(1)` per read pair, lines 6697–6708) — there may be no trustworthy PE byte-identity oracle.
2. **The Perl PE report writer has no minimap2 branch** — it labels minimap2 PE runs as **"HISAT2"** (line 1845–1850). A byte-identical PE gate would have to replicate that bug.

Plus several Important faithful-port gaps. The most dangerous of these (I-4) is that **the Phase-3 spike the plan cites as its foundation explicitly instructs the implementer to read minimap2's `s2:i:` tag into the merge's second-best — which Perl never does**; following the spike would silently break MAPQ byte-identity. The plan reaches the right answer but does not flag that its own spike contradicts it. The remaining Importants are routine faithful-port gaps (max-len range validation + default, the SE-vs-PE `/1` framing trap, the `--mm2_nanopore` preset). The SE scope (V9) is gateable and the plan can proceed for SE; **PE must be re-scoped with eyes open about the broken/mislabelling Perl oracle.**

---

## What I verified HOLDS (the plan's load-bearing claims, source-checked)

- **SE merge is genuinely a no-op (VERIFIED).** SE parse loop 2772–2796: `AS:i:`→score; `ZS:i:`→second_best (unconditional, line 2780); `MD:Z:`→md; the `XS:i:`/`ZS:i:` branch is gated `if($bowtie2)` (2787). For `--minimap2`, `$bowtie2=0` and `$hisat2=0` (7429–7431), so that branch is dead, and minimap2's tag is `s2:i:` (lowercase, no Z) which the case-sensitive `/ZS:i:/` at 2780 never matches → `second_best` stays undef. The Rust parser (align.rs 98–104) `strip_prefix("XS:i:").or("ZS:i:")` likewise never matches `s2:i:` → `second_best=None`. **Identical.**
- **`calc_mapq` verbatim, AS-sign-safe (VERIFIED).** mapq.rs takes `as_best:i64`/`as_second:Option<i64>` with no clamping or sign assumption; the `(max AS = 0)` text at line 26 is a *comment*, not logic. minimap2's positive AS + `second_best=None` + `(0,-0.2)` drives `best_over = AS - sc_min` large-positive → MAPQ 42 for every unique read — and Perl runs the *same* formula on the *same* inputs, so it's byte-identical (the spike's BAM, incl. MAPQ col, was byte-deterministic). The Rust AS field parses `i64` (handles positive). **No Bowtie2-specific assumption breaks on minimap2 AS.**
- **Option string (VERIFIED, V2 literal correct).** Perl 8359–8413: `@aligner_options=()` then push `-a`, `--MD`, `--secondary=no`, `-t 2`, `-x {sr|map-pb|map-ont}`, `-K 250K` → joined `-a --MD --secondary=no -t 2 -x map-ont -K 250K`. Matches V2 exactly. `-t 2` is hardcoded (8372), independent of any thread/`-p` choice (OQ-4e correct).
- **Positional `.mmi` invocation (VERIFIED).** `single_end_..._minimap2` 7022/7025: `$mmi=$fh->{bisulfiteIndex}.".mmi"; "$path_to_minimap2 $mm2_options $mmi $temp_dir$fh->{inputfile}"` — no `-x basename`, no `-U`, no `--norc`/`--nofw` (7011–7016 commented). Matches §2 align.rs delta.
- **Version parse (OQ-4a VERIFIED).** 7081–7084: minimap2 branch does NO regex; `$aligner_version` = the whole `--version` output, chomped (7076). For single-line `2.31-r1302\n` → `2.31-r1302`. And the version is **stderr-only** (warn 7089) — it never lands in the BAM or the report.txt, so a parse imperfection cannot break byte-identity. Plan's "first whole line, trims" is right.
- **`s2:i:` is NOT captured (VERIFIED, but see Important #4).** The spike (Q4) calls `s2:i:` "the 2nd-best the merge needs" — that is **wrong about Perl**; Perl ignores it. The plan reaches the correct conclusion (`second_best=None`), but the spike's framing is a trap for a future maintainer.

---

## Critical (must resolve before the PE work / Phase 5)

### C-1. The Perl PE minimap2 oracle is unfinished WIP — a PE byte-identity gate has no valid oracle
`paired_end_align_fragments_to_bisulfite_genome_fastQ_minimap2` (6623+) contains, **uncommented**:
- line 6697: `# TODO: Need to check this.`
- line 6698: `warn "$id_1\n$id_2\n";sleep(1);`
- line 6708: `warn "$id_1\n$id_2\n";sleep(1);`

These are the **only** uncommented `warn`+`sleep(1)` debug statements in the whole script (every other `sleep(1)` is commented). The init loop sleeps **1 s per read pair** and floods stderr — i.e. the upstream author never finished/validated PE minimap2. The plan (§3.7, OQ-4c, §5 step 7) says "implement the convert delta SE+PE here … gate PE at Phase 5" and the EPIC Phase-5 row promises a PE gate, but **you cannot byte-identity-gate against an oracle that doesn't run sanely.** Required:
- Before promising any PE minimap2 gate, **run Perl `bismark --minimap2 -1 -2` on oxy** (even 1k) and confirm it (a) completes, (b) produces a sane PE report, (c) the `/1` strip at 6699–6707 doesn't hit the `die` at 6706. Capture the result in the plan / a mini-spike.
- If the Perl PE path is genuinely broken, **either** restrict v1.x minimap2 to **SE only** (document like the HISAT2-multicore reject) **or** treat PE as a Felix-decision concordance/oracle-fix path. The plan must state this fork instead of silently assuming a PE gate is reachable.

### C-2. Perl PE report mislabels minimap2 as "HISAT2" — a byte-identical PE report is impossible without replicating the bug
PE report writer (1842–1850) is a binary `if($bowtie2){"Bowtie 2"} else {"HISAT2"}` — **no minimap2 branch.** For minimap2 PE (`$bowtie2=0`), Perl writes *"Bismark was run with HISAT2 …"*. (Contrast the SE writer 1722–1728, which *does* have a correct minimap2 branch.) The plan's §2 `report.rs` line — "the `aligner.name()` 'was run with …' branch (2a) covers minimap2 once the enum variant + name exist" — is true for the Rust, but it makes the Rust write **"minimap2"** where Perl PE writes **"HISAT2"** → the PE report.txt is **not byte-identical**. Required: the plan/Phase-5 must decide explicitly — replicate the Perl bug (Rust writes "HISAT2" for minimap2 PE, ugh), fix the oracle, or scope minimap2 PE out. Tie this to C-1 (same PE-oracle-quality root cause). **SE is unaffected** (SE writer is correct), so SE gating in V9 stands.

---

## Important

### I-1. The `/1` retention delta is **PE-only** — the plan's SE framing is a byte-identity trap
The plan §2 (`convert.rs` bullet), §3.6, §4 sketch (`single /1 (Minimap2)`) and the spike all imply minimap2 SE involves `/1`. **It does not.** The SE converter `biTransformFastQFiles` (5489–5652) appends **no** read-number tag for any aligner — the `/1`/`/2` (mm2) vs `/1/1`/`/2/2` (others) logic lives **only** in the PE converter `biTransformFastQFiles_paired_end` (5945–5959). For SE minimap2 the temp ID is bare `<id>`, minimap2 returns bare `<id>`, and the SE lockstep `last_seq_id eq $identifier` (2735) compares `<id> eq <id>` — trivially. **If an implementer "adds /1 to SE minimap2" per the plan's wording, the SE gate (V9) will break** (the SE path stores `last_seq_id=$id` *unstripped* at 7046 — a `/1` would never match). Fix: state plainly that **SE adds nothing (unchanged from today)**; the single-`/1`/`/2` delta is PE-converter-only. Add an SE-minimap2 convert test asserting **no** tag is appended (the inverse of V7).

### I-2. `--mm2_maximum_length` range validation + default-10000 assignment are missing from the plan and the Rust
Perl 8344–8356: if `--mm2_maximum_length` defined, **die** when `<100` or `>100000` (8346–8351); else default **10000** (8354). The Rust **convert-side drop already exists** (convert.rs 332–336: `if seq.len() > cutoff { continue }` — verified, so the prior draft's "no convert-side drop" was wrong). What is genuinely missing: config.rs only has the *mode* guard (205–207, rejects the flag outside minimap2 mode) — there is **no range check** and **no default-10000 assignment**, so today an in-minimap2-mode run with the flag absent leaves `maximum_length_cutoff=None` (Perl would set 10000) and an out-of-range value would be accepted (Perl dies). The plan §2 says "default 10000" and "activate the cutoff" but the V-table (V7) tests only "a >cutoff read dropped"; it omits the **range `die`** and the **default assignment**. Add: range-validation tests (`<100` dies, `>100000` dies, `100`/`100000` OK; absent→default 10000) and wire the default at minimap2-resolve time. Faithful-port correctness. (Note: the default 10000 is inert on real bisulfite reads, so a missing default won't surface at V9 — it's only visible via a unit test or a pathological input.)

### I-3. The max-length drop interacts with the analysis counter — under-specified, and the gate won't catch it
The cutoff `next`s the read **out of the temp file** (5598–5604) so minimap2 never sees it, **but** the original-input analysis loop (2413–2444) still reads + `++$counting{sequences_count}` for that read and calls `check_results_single_end`, where it finds no matching `last_seq_id` → counted as **analysed + no-alignment** (not dropped from "sequences analysed"). An implementer who instead skips the read entirely (so it's not counted/analysed) would produce a divergent report. Because the default 10000 never fires on real bisulfite reads, **V9 cannot catch this** — it needs an explicit unit/integration test with a forced-short cutoff that asserts the dropped read still increments "sequences analysed" and lands in "no alignment". The plan's V7 must assert the *count interaction*, not just "dropped".

### I-4. `s2:i:` footgun — the SPIKE the plan is built on ACTIVELY INSTRUCTS the wrong thing
This is the sharpest live trap and it is **stronger than "the spike's framing is loose"**: the Phase-3 spike does not merely mention `s2:i:` — it **repeatedly and explicitly directs the Phase-4 implementer to read `s2:i:` into the merge's second-best** for minimap2:
- Q4 (line 23): "The Phase-4 merge's 2nd-best source for minimap2 is `s2:i:`, not the `XS`/`ZS` …"
- §findings (28): "the new work is the **2nd-best tag (`s2:i:`)** …"
- §2nd-best (34): "**The merge must read `s2:i:` for minimap2** (cf. `XS:i:` Bowtie2 / `ZS:i:` HISAT2)."
- §scope (40): "the **`s2:i:` 2nd-best in `align.rs`/the merge**."

The Perl source proves the opposite: there is **no `s2` branch anywhere** (SE 2780/2787; PE read-1 3376, read-2 3397), and `s2:i:` is lowercase so it never matches the case-sensitive `/ZS:i:/`. Perl feeds `calc_mapq` a `second_best` that is **always undef→backfilled to AS** for minimap2. The plan (rev 0) correctly lands on `second_best=None` — **but it is built on a spike whose explicit Phase-4 instruction would break byte-identity.** An implementer who follows the spike (its stated reason to exist) will add an `s2:i:` branch to align.rs 101, introduce a `second_best` Perl never had, and silently corrupt MAPQ on every multi-mapper. The current Rust align.rs (98–104) is correct *today* (only `XS`/`ZS`), but nothing guards it. Required mitigations: (a) the plan must **explicitly flag the spike Q4 as superseded/wrong** (one line: "spike Q4 says read `s2:i:` — that is WRONG; Perl ignores it; do NOT add an `s2` branch"); (b) V6's parse test **must** feed a real minimap2 tag set (`AS:i:0 ms:i:.. s1:i:.. s2:i:.. NM:i:.. MD:Z:..`) and assert `second_best==None` (the existing tests at align.rs 474–556 cover XS/ZS/none but NOT a present-but-ignored `s2:i:`); (c) add a comment at align.rs ~60/101 that minimap2's `s2:i:` is intentionally ignored to match Perl (no `s2` branch at 2780/2787/3376/3397). Treat this as load-bearing, not incidental.

### I-5. `--mm2_nanopore` preset path is not enumerated
Plan §2/§3.3/V3 enumerate presets as default `map-ont` / `--mm2_short_reads`→`sr` / `--mm2_pacbio`→`map-pb`, but Perl 8399–8403 routes **both** the default **and** explicit `--mm2_nanopore` through the `else` → `-x map-ont` (it even sets `$mm2_nanopore=1` in the default case). The Rust preset selection must treat `--mm2_nanopore` (alone) identically to the default. Add a V3 case: `--mm2_nanopore` → `map-ont`. The conflict-dies (sr⊕nanopore, sr⊕pacbio, pacbio⊕nanopore — 8375/8378/8391) are correctly cited; just complete the *positive* enumeration.

### I-6. PE read-2 parse takes the `ZS` (not `s2`) branch for minimap2 — confirm the Rust PE parser produces `None`
For PE, read-1 parses `XS:i:` unconditionally (3376) and read-2 takes `else{ ZS:i: }` for non-Bowtie2 (3397–3401). minimap2 emits neither `XS` nor `ZS` (only `s2`), so both `second_best_1`/`second_best_2` are undef → the backfill at 3466 is skipped → no second-best sum (verified 3460–3490). The Rust uniform parser must yield `second_best=None` for *both* PE mates of a minimap2 record. This is the same shape as the HISAT2 PE read-1 fix (#2b), but reached via a *different* tag mismatch — when PE is implemented, add a **PE-minimap2 multi-mapper** unit test (a record carrying `s2:i:` on both mates → both `second_best=None`), analogous to the 2b test. (Folds into the PE work behind C-1.)

---

## Optional / lower-risk

- **O-1 (angle #2, both-strand at non-dir/1M):** the spike saw 0 reverse on the CT instance at SE-dir 10k under `map-ont --secondary=no`, but **non-dir (4 instances) and 1M were not spiked.** V9 *does* gate non-dir SE at 1M, so a divergence would be caught — but the plan has **no documented fallback** if the both-strand effect bites at non-dir/scale (unlike the HISAT2 reject). Add one line: "if non-dir/1M diverges due to both-strand selection, restrict/document like HISAT2." Low risk, but the only residual exposure of the retired headline fear.
- **O-2 (angle #6, multicore thread compounding):** `--multicore N` × `-t 2` = 2N minimap2 threads. Not a byte-identity issue (correctly out of scope) but worth a note so the OQ-4d gate cell's resource use isn't a surprise on oxy.
- **O-3 (angle #5, determinism at scale):** spike confirmed 10k (~3 minibatches) input-order + deterministic with `-t 2`. The plan defers 1M-multi-minibatch confirmation to the gate (OQ-4e) — reasonable, but make V9 assert run-to-run determinism at 1M explicitly (re-run once), not only Perl==Rust, so a latent reorder can't hide behind a coincidentally-matching single run.
- **O-4:** discovery `.mmi` has **no large-index fallback** (minimap2 has no `.mmil`) — the plan's `index_suffixes(Minimap2)=["mmi"]` is correct; just ensure the `large_index` plumbing (Bowtie2/HISAT2) is short-circuited for Minimap2 (no `large` probe), else a spurious "large index" branch could fire.

---

## Answers to the caller's adversarial angles

1. **Merge no-op real?** YES for SE (verified: dead `$bowtie2` branch + `s2`≠`ZS`/`XS`). YES for PE too but via read-2's `ZS`-not-`s2` mismatch (I-6). minimap2 emits NO tag the Rust captures as second_best. calc_mapq is AS-sign-safe; positive minimap2 AS → MAPQ 42, byte-identical to Perl's same-formula. **No latent MAPQ divergence.**
2. **4-instance/scale both-strand:** real exposure is **non-dir/1M and PE**, not SE-dir; V9 covers non-dir SE 1M (catches it) but has no fallback (O-1); **PE is the real exposure and it's blocked on a broken oracle (C-1).**
3. **Options completeness:** clean-slate string verified complete (no `-Y`/`--cs`/`-p`); `-t 2` truly hardcoded (OQ-4e ✓); BUT max-len **range validation + default missing** (I-2) and its **count interaction under-specified** (I-3).
4. **`/1` retention + lockstep:** the delta is **PE-only**; SE adds nothing — the plan's SE framing is a trap (I-1). PE `/1` strip (6699–6707) is itself in the WIP block (C-1).
5. **Determinism at scale:** plan defers to the gate; tighten V9 to assert run-to-run determinism at 1M (O-3).
6. **Scope:** SE + `--multicore` SE is fairly gate-able expecting invariance (OQ-4d lean reasonable, minimap2 is per-read-independent). **PE is mis-scoped** — it cannot be cleanly gated against the current WIP/mislabelled Perl PE oracle (C-1/C-2); re-scope before Phase 5.

---

## Action items (prioritized)

**Critical**
- [ ] C-1: Validate the Perl PE minimap2 oracle on oxy (does it run / report sanely / not hit the 6706 `die`?) before promising a PE gate; document the SE-only-vs-PE fork.
- [ ] C-2: Decide the PE report-label policy (replicate Perl's "HISAT2"-for-minimap2 bug, fix the oracle, or scope PE out). SE writer is fine.

**Important**
- [ ] I-1: State SE minimap2 appends **no** tag (unchanged); the `/1`/`/2` single-tag is PE-converter-only. Add the inverse SE convert test.
- [ ] I-2: Add `--mm2_maximum_length` range `die` (<100 / >100000) + default-10000 assignment + tests (convert-side drop already exists).
- [ ] I-3: Specify + test the max-len drop's count interaction (dropped read still "analysed" + "no alignment").
- [ ] I-4 (high among Importants): **Add a plan line flagging spike Q4 as WRONG** — do NOT add an `s2:i:` branch; Perl ignores it. V6 parse test feeds a real minimap2 tag set incl. `s2:i:` → assert `second_best==None`; add an "intentionally ignored" comment at align.rs.
- [ ] I-5: Enumerate `--mm2_nanopore`→`map-ont` in the preset logic + V3.
- [ ] I-6: (with PE) add a PE-minimap2 multi-mapper test asserting both mates `second_best=None`.

**Optional**
- [ ] O-1: Document a non-dir/1M both-strand fallback. O-2: note thread compounding. O-3: assert 1M run-to-run determinism in V9. O-4: short-circuit `large_index` for Minimap2.
