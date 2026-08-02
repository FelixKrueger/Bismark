# PLAN REVIEW B — `08022026_rammap-inprocess-preset` (PLAN.md rev 1)

**Reviewer:** B (independent; no coordination with A)
**Target:** `plans/08022026_rammap-inprocess-preset/PLAN.md` rev 1 + `SPIKE.md`
**Base checked against:** `dev` `11efbab`, clean tree
**rammap source read:** `~/.cargo/git/checkouts/rammap-aa480586e5eab817/5ea62cd/`

## Verdict

**Not ready to implement as written.** The design core is right — typed `Mm2Preset`, one derivation feeding both the emitted `-x` and the backend, exhaustive feature-gated bridge, default path unmoved. Four findings change either what the plan says to build or what it claims its gates prove:

- **C1** V8 injection (i) does **not** fail V7 as specified. The gate still proves the bridge, not the wiring — the exact fault §9a exists to catch survives it.
- **C2** The spike's blocking premise is false. rammap **can write** an `.mmi` (`save_index`), so `from_index` — the production constructor — *is* hermetically testable. The gate can be strictly better than the plan believes, at lower cost.
- **C3** `Preset::MapPb` is **provably inert** on the `from_index` path. `--rammap --mm2_pacbio` alignments do **not** change. Three statements in the plan (and the drafted CHANGELOG line) are wrong, and the never-silent problem persists for that flag.
- **C4** A fault that ships green through every gate in §9: **collapse or swap the three Perl-faithful conflict messages.** `minimap2_preset_conflicts_die` asserts only `.is_err()`. A6/V2/§3.1's "asserted by" claims are false, and V8 injection (iii) cannot fail.

C4 is the #1081 precedent failure mode repeating verbatim: a claim that a test enforces something it does not look at.

### What I checked

Every `file:line` the plan cites, in the source, at `11efbab`; the resolve-order question in `config.rs`; the two CI jobs in `rust_ci.yml`; all four `Preset::` literals in the tree (the plan accounts for three); `rammap`'s `from_index` / `from_loaded_index` / `build_options` / `apply_preset_str` / `Index::save` / `Index::load_part`, and the rammap CLI's prebuilt-index path; `consumed_read_len`'s op set; the #1081 AS-bound gate's scope; whether a `RunConfig` is obtainable in a test.

---

## Critical

### C1 — V7 gates the bridge, not the wiring; V8 injection (i) is green

`PLAN.md:261` mandates extracting `inprocess_preset(config: &RunConfig) -> ::rammap::Preset` and having V7 "drive **that** function from a `RunConfig`". V7 (`PLAN.md:238`) then builds a `from_seqs` aligner in the test and asserts `Mapping.score`.

Now apply V8 injection (i) — "revert `:968` to the `Preset::MapOnt` literal":

- `inprocess_preset` still exists, still returns `Sr` for an `Sr` config.
- V7 never touches `mod.rs:968` or the `load` closure at `mod.rs:958-972`.
- V7 scores 270 and passes.

So the fault the whole §9a exercise was built to catch survives the gate. `PLAN.md:239` ("Injection (i) … after the spike it is genuinely caught"), `PLAN.md:251` ("**V7 is a real behavioural gate**, and injection (i) is genuinely caught") and `SPIKE.md:106` all overstate what the recommended shape delivers. The extraction *narrows* the fault — from "`:968` names a literal" to "`:968` calls the seam and ignores the result", or "`inprocess_preset` is defined and never called" — but it does not close it, and the plan's own §9a diagnosis of this shape ("If V7 builds its own `from_seqs` aligner it proves the *bridge*, not the *wiring*") applies just as much to `inprocess_preset` + a test-built aligner as it did to a test-built aligner alone. Two things are needed for a wiring gate: the resolved preset must reach `from_index`, **and** the observation must come out the other end of the production construction.

That is achievable — see C2.

### C2 — `save_index` exists: the production constructor `from_index` is hermetically testable

`SPIKE.md:11` and `PLAN.md:243` rest the whole difficulty on "`from_index` needs a real `.mmi`" while `make_genome_mmi` writes a 1-byte placeholder (confirmed: `tests/aligner_cli.rs:2648-2657` writes `b"x"`). The spike never asked whether rammap can *write* one. It can:

- `rammap-core/src/api.rs:425` — `pub fn save_index(&self, path: &str) -> io::Result<()>`
- → `rammap-core/src/align/index.rs:319-329` — `Index::save` / `save_part`: RMMI magic + `bincode::serialize_into(writer, self)`, i.e. the **whole** index including `seqs`, so CIGAR/MD output works off it
- `from_index` (`api.rs:249-251`) → `Index::load` (`align/index.rs:332`) → `load_part` (`:347`) sniffs RMMI / `MMI\x02` / legacy, so the round-trip is supported by design

So a feature-gated test can:

1. `rammap::Aligner::from_seqs(vec![("chr1_CT_converted", reference)], Preset::MapOnt).save_index(tmp.join("BS_CT.mmi"))` — a real index, no binary, no network;
2. build a `RunConfig` pointing `genome.ct_index_basename` at it (all `RunConfig` fields are `pub` and `mod.rs` is in-crate, so a `#[cfg(test)]` helper can construct one directly — no `resolve`, no `detect_aligner`);
3. write a converted-FastQ temp holding the `mm3_150` read and call **`build_se_inprocess_streams(&config, &converted)`** — the real production function, the real `load` closure, the real `from_index`;
4. drain the stream and assert the emitted `SamRecord.alignment_score`.

That gate covers `mod.rs:968` + `from_index` + the new `RunConfig` field in one assertion, fails injection (i) exactly as V8 claims, and also fails I1's "field never populated" fault. It additionally **dissolves `SPIKE.md:122`'s fourth limitation** ("`from_seqs`, not `from_index`… this was not separately verified") by not needing the assumption.

Two caveats to write into the plan:

- The gate's index is built by `from_seqs` at `k=15/w=10` (`api.rs:297+`); production's `.mmi` comes from `minimap2 -k 20 -d` (`src/genome_prep/indexer.rs:118-124`). Irrelevant to mismatch-penalty arithmetic, but it means the expected scores must be **run and pinned**, not copied from the spike's `from_seqs` numbers.
- Keep the derivation in the comment as `§11` risk 3 already says: `2·150 − 3·(2+8) = 270` vs `2·150 − 3·(2+4) = 282`.

If this is judged too much for one fix, the honest fallback is to **relabel** V7 as a bridge+resolver gate, delete the "injection (i) is genuinely caught" claims, and state that `:968` is covered structurally only. What must not ship is the current combination: a structural guarantee described as a behavioural gate.

### C3 — `MapPb` is inert on `from_index`; `--rammap --mm2_pacbio` does not change

`from_index` → `from_loaded_index` (`api.rs:234-245`) → `build_options(preset, index.kmer_size, index.window_size)` (`api.rs:455-474`). `build_options` seeds `k`/`w`/`is_hpc` as **locals**, hands them to `apply_preset_str`, and then **returns only `(MapOptions, OutputConfig)`** — the k/w/hpc the preset wrote are dropped on the floor, and the already-built index keeps its own.

`map-pb`'s entire preset body is index-only (`api.rs:617-619`):

```rust
"map10k" | "map-pb" => { *is_hpc = true; *k = 19; },
```

Nothing touches `opt`. Therefore **`from_index(mmi, Preset::MapPb)` and `from_index(mmi, Preset::MapOnt)` produce identical aligners** — identical `MapOptions`, same index, same everything.

Consequences for the plan:

| plan statement | status |
|---|---|
| `PLAN.md:142` — `--rammap --mm2_pacbio` … after: **`map-pb`** | wrong; behaviour is unchanged |
| `PLAN.md:209` — "for `--rammap --mm2_short_reads` / `--mm2_pacbio` … **alignments change** … propagates to `AS`, hence MAPQ, methylation calls" | wrong for the pacbio arm |
| `PLAN.md:193` (CHANGELOG draft) — "alignments change for those two invocations" | wrong for the pacbio arm |
| `SPIKE.md:65-69` (F2) — `MapPb` "cannot be pinned this way" | true but under-read: it is not merely un-gateable, it is a **no-op** |

The reframe matters. `PLAN.md:292` demotes `MapPb` to "an arm gated by unit tests only". The reality is that after this fix a user typing `--rammap --mm2_pacbio` still gets `map-ont` behaviour, and still gets no word about it — the same silent-ignored-flag shape §1 opens with, one level down. For completeness: the **subprocess** rammap CLI behaves identically on a prebuilt index (`rammap/src/main.rs:826-833` takes `saved_k`/`saved_w`/`saved_is_hpc` from the loaded index), so the fix *does* still make the two backends agree — but they agree on "inert", which is not what a CHANGELOG should describe as a behaviour change.

Minimum action: correct §3.3, §7 and the CHANGELOG wording, and state once — in the plan and in the run's notice (see I6) — that `map-pb` differs from `map-ont` only in index-build parameters and so cannot take effect against a pre-built `.mmi`. Rejecting `--rammap --mm2_pacbio` outright is the stricter alternative; I would not, since the flag is equally inert on the subprocess and minimap2 paths for the same reason, and a die would be a new incompatibility.

### C4 — Fault that ships green through §9: rewrite the three conflict messages

`src/aligner/options.rs:916-929`:

```rust
let cli = cli_from(&conflict);
assert!(
    build_aligner_options(&cli, Aligner::Minimap2, ReadFormat::FastQ, false, None).is_err(),
    "{conflict:?} should die"
);
```

`.is_err()` only. No message is inspected, and the triple conflict is not in the loop. So these three statements are false:

- `PLAN.md:110` — "Their message strings are Perl-faithful … and asserted by `minimap2_preset_conflicts_die`"
- `PLAN.md:223` (A6) — "✅ Asserted by `minimap2_preset_conflicts_die`; Perl-faithful strings"
- `PLAN.md:233` (V2) — "all three pairs, **same messages**"

And the injection is real. During the extraction, replace all three strings with one generic `"conflicting --mm2_* presets"`, or swap short⊕nanopore's text with short⊕pacbio's, or reorder the checks:

- V1 green — the emitted option string is untouched
- V2 green — all three pairs still `is_err()`
- V3 green — selector→variant mapping unchanged
- V4/V5/V6/V7 green — nothing to do with messages
- V8(iii) green — **reordering cannot change `is_err()`** for any of the three tested pairs, so the stated injection ("reorder the conflict dies → V2 must fail") does not hold

Perl-faithful stderr silently changes, and the plan's own §3.1 concern ("the order determines which message a triple-conflict produces") has **zero** coverage today.

Fix: V2 must assert the exact message per pair, and add `["--mm2_short_reads", "--mm2_pacbio", "--mm2_nanopore"]` asserting the short⊕nanopore text — that single case is what pins the order the plan says is behaviour. Then V8(iii) becomes true.

---

## Important

### I1 — the new `RunConfig` field assignment is ungated, and `resolve` can't be driven in the job that compiles the fix

V3 (`PLAN.md:234`) drives `resolve_mm2_preset(cli)`. The line the fix actually depends on is the struct-literal field at `config.rs:858`+. Leave a stub there — `mm2_preset: Mm2Preset::MapOnt`, with `resolve_mm2_preset` called for its validation side effect and its value dropped — and V1–V8 are all green.

It cannot be closed the obvious way:

- every `resolve` unit test in `config.rs` (1682, 1703, 1793, 1801, 1816, 1829, 1849, 1927, 1942, 1971, 1993, 2022, 2099) asserts an **error**; none reaches the `RunConfig` construction;
- the success path runs the aligner binary — `config.rs:830` → `aligner.rs:94-110` executes `<aligner> --version` unconditionally, including for `Aligner::Rammap` (the in-process backend doesn't need the binary at run time, but `resolve` still detects it);
- the `rammap-inprocess` CI job installs **minimap2** and **not rammap** (`.github/workflows/rust_ci.yml:89-102`).

So `PLAN.md:261`'s "have V7 drive that function **from a `RunConfig`**" is not achievable via `resolve` in the only job that compiles it. The plan must say which construction the test uses. The likely unguided accommodation is to change the seam to `inprocess_preset(p: Mm2Preset) -> ::rammap::Preset` because a `RunConfig` was awkward — at which point the seam **is** the bridge and V7 silently reverts to C1's failure while still being labelled the wiring gate. Naming the construction (a hand-built `RunConfig`, or a `#[cfg(test)]` helper, per C2 step 2) closes both I1 and C1.

### I2 — nothing gates the single-sourcing that is the plan's stated purpose

§2 (`PLAN.md:73-75`) sells this as a class fix: one derivation feeding both the emitted `-x` and the in-process backend, per #1079's `ScoreMinForm`. But V1 asserts hardcoded option strings and V3 asserts the resolver *independently*. An implementation that adds `resolve_mm2_preset` and leaves `minimap2_options`' own `if` chain (`options.rs:259-285`) in place passes every gate while re-creating the two-derivations drift the plan exists to remove.

Add one cross-check over the five clis (default, `--mm2_nanopore`, `--mm2_pacbio`, `--mm2_short_reads`, `--illumina_5base`):

```rust
let s = minimap2_options(&cli)?;
assert!(s.contains(&format!("-x {}", resolve_mm2_preset(&cli)?.as_option_str())));
```

It is the only assertion that expresses "these are one fact" rather than "these two hardcoded lists agree today".

### I3 — the die-order hazard is real, but §5 step 3 checks a case that cannot discriminate

`PLAN.md:190` says to confirm `--bowtie2 --mm2_short_reads` still dies with today's message. That case can't discriminate: `resolve_mm2_preset` never errors on a single selector, so both orderings give the same message.

The discriminating case is a **conflicting pair with a non-minimap aligner**: `--bowtie2 --mm2_short_reads --mm2_nanopore`. Today `resolve_mm2_max_length` is called at `config.rs:683` and dies with `"You cannot specify minimap2 options (--mm2_short_reads) unless you also use --minimap2. Please respecify!"` (`config.rs:1282-1288`), because `minimap2_options` is never reached for Bowtie 2 (`options.rs:235`). Placing the new resolver at ~840 as §5 step 3 says preserves that (683 < 840). Placing it anywhere earlier — e.g. beside `resolve_aligner` at `config.rs:590`, a natural home for a resolver — flips the user-visible message to `"Please select minimap2 in Short Read or Nanopore mode, but not both..."`.

No test covers it: **no file under `tests/` mentions `--mm2_short_reads`, `--mm2_pacbio` or `--mm2_nanopore` at all.** Add the case so the placement is pinned rather than remembered.

### I4 — §2's Files table sends V1/V2 to the wrong file

`PLAN.md:84` lists `rust/bismark/tests/aligner_cli.rs` as holding `minimap2_preset_selection` and `minimap2_preset_conflicts_die`. They are unit tests in `src/aligner/options.rs`'s `#[cfg(test)] mod tests` — `options.rs:897` and `options.rs:916` (the 894/910 in the row are the doc-comment lines, so the row contradicts itself). `tests/aligner_cli.rs` contains neither.

The one integration-level `-x sr` assertion is the 5-Base end-to-end test at `tests/aligner_cli.rs:6099+`, which spawns the real binary and real minimap2 — worth naming in V1 explicitly, since it is the only place the `--illumina_5base` arm of the extracted resolver is exercised end-to-end (A5's verification target).

### I5 — step 5's rationale is unachievable and misses a fourth literal

`PLAN.md:192` — "pass the preset explicitly at `inprocess.rs:702`/`:754` so no `Preset::MapOnt` literal survives anywhere; keeps a future grep honest."

Two problems. Those two sites **already** pass the preset explicitly to `from_seqs`; there is no config in a synthetic-reference test to route through, so there is nothing to change. And a fourth literal exists that the plan never mentions: `tests/aligner_rammap_inprocess_crosscheck.rs:147` hard-codes `rammap::Preset::MapOnt` — the env-gated in-process-vs-subprocess crosscheck, i.e. precisely the test §10 nominates as the vehicle for follow-up (c). So "no literal survives anywhere" cannot be delivered, and the grep-honesty goal is better served by grepping `from_index(` in `src/` (one hit).

The useful version of step 5: parameterize the crosscheck's preset (e.g. a `RAMMAP_PRESET` env var defaulting to `map-ont`) so it can never compare in-process `map-ont` against a subprocess run someone invoked with `-x sr`, and so option (c) is a small delta later.

### I6 — a CHANGELOG line is not never-silent; the run's notice is the right place

D-A4 (`PLAN.md:14`, `:272`) requires the release note to say in-process `sr` is newly reachable and its concordance ungated. A release note reaches readers of release notes. Meanwhile the run already prints a never-silent backend notice at `mod.rs:206-247` that names the backend and thread count and **never names the preset** — while the SE report's "was run with" line has been printing `-x sr` all along, i.e. the report was already asserting a preset the run did not use.

Adding the resolved preset to that notice (plus, per C3, one clause for `map-pb`) costs a line, matches the three existing never-silent notices in the same block, and reaches the user who actually ran `--mm2_short_reads`. Worth adding to §5 as a step.

Also worth stating positively in §7: because V1 freezes the option string, **no report bytes change** — the report simply stops lying. That is an output surface the plan should name as *unchanged*, since a reader of §7 would reasonably wonder.

### I7 — §7's output-surface list is thinner than the change

`sr` is not only a scoring change (`api.rs:684-700`): `min_chain_score = 25`, `min_dp_max = 40`, `best_n = 20`, `pri_ratio = 0.5`, `zdrop = 100`, `max_gap = 100`, `bandwidth = 100`, `end_bonus = 10`, plus `SHORT_READ | FRAG_MODE | NO_PRINT_2ND | HEAP_SORT`. The spike's own table shows **mapped → UNMAPPED flips at 60 bp and 100 bp** (`SPIKE.md:42,45`) — i.e. at typical trimmed WGBS read lengths.

So mapping *rate* moves, not just the `AS` column: the SE report percentages, `--unmapped` FastQ contents, and the unique/ambiguous split all shift. Given A4 says this path is unmeasured, "some reads that map today will not map under `sr`" is the most user-visible consequence of the fix and belongs in §7 and the CHANGELOG.

Two cheap assertions worth adding to the new gate while it is being written:

1. **`consumed_read_len(cigar) == read_len`** under `Sr`. `inprocess.rs:132-157` counts `M I S = X` and not `D N H P`; a CIGAR failing this equality is **silently skipped** by the methylation length guard (the crosscheck's own gate condition, `tests/aligner_rammap_inprocess_crosscheck.rs:8-11`). `sr`'s local-extension behaviour is a newly reachable source of clipped CIGARs, so this is the one silent-wrong-output mode the AS assertion does not cover.
2. **A perfect 150 bp read scores exactly 300 under `Sr`.** #1081's MAPQ denominator assumes `AS ≤ 2·read_length`, and its live gate `tests/aligner_minimap2_as_bound.rs` runs **minimap2 only** (`:1-21`, `:29`) — nothing covers rammap, whose `sr` sets `end_bonus = 10` (`api.rs:692`). The spike's numbers are already evidence the bound holds at `5ea62cd` (`mm1_150` Sr = 290 = 300 − 10; `mm3_150` Sr = 270 = 300 − 30, both consistent with a 300 baseline and no additive bonus), but that is evidence sitting in a deleted throwaway. One assertion turns it into a gate for the path this fix newly makes reachable.

### I8 — feature-split coherence: the split is fine, placement is the risk

`Mm2Preset` unconditional + bridge feature-gated is coherent. `RunConfig.mm2_preset` is a `pub` field of a `pub` struct, so feature-off leaves it written-but-never-read with no `dead_code` warning, and `as_option_str()` is used by `minimap2_options` on both builds.

The reverse-direction risk you asked about is **placement of V4/V7**. Any test naming `::rammap::Preset` must carry either a per-test `#[cfg(feature = "rammap-inprocess")]` (the pattern at `inprocess.rs:665`, `:689`) or a per-file `#![cfg(feature = "rammap-inprocess")]` (`tests/aligner_rammap_inprocess_crosscheck.rs:36`). Omit it and the **default** jobs fail to compile — `cargo test -p bismark` and, first, `cargo clippy --workspace --all-targets -- -D warnings` (`rust_ci.yml:63-64`, note `--workspace`, not the `-p bismark` in §5 step 8). Since V4 and V7 are new feature-gated tests in a file (`mod.rs`) whose `#[cfg(test)]` module is *not* feature-gated as a whole, this is a live trip hazard worth one line in step 8.

---

## Optional

- **O1 — the spike's limitations, judged.** Of `SPIKE.md:119-122`: "one reference, one seed" does **not** undermine the recommended cell — `mm3_150`'s delta is parameter arithmetic (`300 − 3·10` vs `300 − 3·6`), and the spike's own `perfect_100`/`perfect_150` rows independently confirm the `2·len` baseline under all three presets, which is what would break if the cell were reference-sensitive. The reference-sensitivity the spike observed is confined to the mapped/unmapped cells, which the recommendation already avoids. "`from_seqs`, not `from_index`" is the limitation that matters, and C2 dissolves it rather than accepting it. "rammap `5ea62cd` only" is handled correctly by §11 risk 3 (derive, don't baseline).
- **O2 — `PLAN.md:292-296` numbers two items `3`.**
- **O3 — §6 "Efficiency: Nil" is very slightly wrong.** `apply_preset_str` calls `crate::align::dp::set_simd_cap(...)` on every preset application (`api.rs:613`, and `Avx2` for `sr` at `api.rs:685`) — a **process-global** SIMD cap set as a side effect of preset selection. Both index loads in a run pass the same preset, so there is no conflict today, but the new code path mutates global state and §6's "one enum, resolved once" undersells it. One line.
- **O4** — A2 (`PLAN.md:219`) says "the other two `Preset::` uses are inside `#[cfg(test)]`". There are **three** other uses; the third is `tests/aligner_rammap_inprocess_crosscheck.rs:147` (see I5). The claim "`mod.rs:968` is the only **production** site" is still correct.

---

## Assumption audit

| # | Plan's status | My finding |
|---|---|---|
| A1 | ✅ verified | Confirmed — `MapOnt`/`MapPb`/`Sr` at `api.rs:39-56` |
| A2 | ✅ verified | Production claim holds. Count is wrong: four literals, not three (O4/I5) |
| A3 | ✅ | Confirmed — the preset exists only inside `aligner_options` |
| A4 | 🟠 the honest risk | **Understated.** Not just "unmeasured": in-process `sr` is a *hybrid* — `sr` mapping options on an index built at `k=20` (`indexer.rs:118-124`), because `from_index` discards the preset's k/w (C3's mechanism). §7's "it should *improve* agreement with `--minimap2 --mm2_short_reads`" is therefore only half true: scoring converges, seeding does not, since minimap2 given a FASTA builds at the preset's k. Worth saying, because it is the figure option (c) will produce |
| A5 | ⚠️ verify at implementation | Right to flag. The end-to-end coverage of that arm is `tests/aligner_cli.rs:6099+`, not a unit test — name it in V1 |
| A6 | ✅ asserted | **False** — see C4. `minimap2_preset_conflicts_die` asserts `.is_err()` only |
| A7 | ✅ verified | Confirmed for `--mm2_maximum_length` (`convert.rs` drops pre-alignment, backend-independent) and `-p` (`config.rs:385`, `mod.rs:1136`) |

---

## Action items

**Critical (block implementation)**

1. **C1/C2** — decide the gate honestly. Either build the `save_index`-backed test that drives `build_se_inprocess_streams` / the real `from_index` (recommended; it also closes I1 and dissolves `SPIKE.md:122`), or relabel V7 as a resolver+bridge gate and delete "injection (i) is genuinely caught" from `PLAN.md:239`, `:251` and `SPIKE.md:106`.
2. **C3** — correct `PLAN.md:142`, `:209`, `:193`: `--rammap --mm2_pacbio` alignments do **not** change, because `from_index` discards the preset's k/w/HPC and `map-pb` sets nothing else. Record that `map-pb` cannot take effect against a pre-built `.mmi` on any backend, and say so in the run's notice.
3. **C4** — V2 must assert the exact message per pair and add the triple-conflict case; fix A6, `PLAN.md:110`'s "asserted by", and V8 injection (iii).

**Important**

4. **I1** — state how the test obtains a `RunConfig`; note that `resolve` needs the rammap binary, which the feature CI job does not install.
5. **I2** — add the `minimap2_options` ↔ `resolve_mm2_preset` cross-check; without it the class fix is unenforced.
6. **I3** — replace step 3's non-discriminating check with `--bowtie2 --mm2_short_reads --mm2_nanopore`, and add it as a test.
7. **I4** — fix the §2 Files row (`src/aligner/options.rs:897`/`:916`), and name `tests/aligner_cli.rs:6099+` for the 5-Base arm.
8. **I5** — drop step 5's "no literal survives anywhere"; parameterize the crosscheck's preset instead.
9. **I6** — add the resolved preset to the `mod.rs:206-247` notice; note in §7 that report bytes do not change.
10. **I7** — add "mapping rate changes" to §7/CHANGELOG; add the `consumed_read_len` and perfect-`Sr`-score assertions to the gate.
11. **I8** — one line in step 8 on per-test feature gating and `clippy --workspace --all-targets`.

**Optional** — O1–O4.

---

## Alternatives considered

- **Reject `--rammap --mm2_pacbio` instead of documenting its inertness (C3).** Cleaner against the never-silent rule, but the flag is equally inert on subprocess rammap and on `--minimap2` against a pre-built `.mmi`, so a die would be a new incompatibility introduced by a correctness fix. Document, don't die.
- **End-to-end gate via a stub `rammap` binary + a real `.mmi`.** `--path_to_rammap` pointing at a shell stub that answers `--version` is enough, since the in-process path never spawns the binary — combined with C2's `save_index`, that yields a full `bismark --rammap --mm2_short_reads` → BAM `AS:i:` gate in the `aligner_cli.rs` idiom. Strongest possible, but it needs a converted-genome/FASTA pair set up correctly; I would only reach for it if C2's in-crate test proves awkward.
- **Re-parsing `-x (\S+)` from `aligner_options`.** The plan rejects this at `PLAN.md:73` and is right to; nothing I found changes that.
