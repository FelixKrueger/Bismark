# PLAN_REVIEW_B — Phase 2: Read conversion (C→T, FastQ SE directional)

**Reviewer:** B (independent, fresh context)
**Target:** `phase2-read-conversion/PLAN.md`
**Oracle verified against:** `bismark` `biTransformFastQFiles` (5489–5651), `fix_IDs` (6235–6246),
`$icpc`/`$maximum_length_cutoff` definitions (process_command_line 7305–8447), `$temp_dir` normalization
(8211–8231), and the Phase-1 implementation (`src/cli.rs`, `src/config.rs`, `src/lib.rs`).
**Verdict:** Sound core; the per-record transform faithfully mirrors Perl. **No Critical correctness bug**
in the converted-content path, but **four Important issues** must be closed before implementation: a
Phase-1 CLI mislabel that Phase 2 silently depends on, an un-replicated `$temp_dir` normalization, a
missing/non-existent validation golden, and a latent `--skip`/sanity-ordering trap. Details below.

---

## 1. Logic review (vs Perl line by line)

The plan's per-record transform is **correct** and matches Perl where it counts:

- **`chomp` → `fix_IDs` → re-append `\n`** (Perl 5584–5586): plan §3.3.2 matches. `chomp` strips only a
  trailing `\n`, so `\r` survives → CRLF preserved. ✓
- **`fix_IDs`** (6235–6246): default `s/[ \t]+/_/g`, `--icpc` `s/[ \t].*$//g`. Plan §3.2 + helper §5.2
  match exactly, incl. operating on the chomped id with the leading `@` kept. ✓
- **`uc$sequence` then `tr/C/T/`** (5597, 5625): plan §3.3.4 matches ("uc then C→T"; lowercase `c`→`C`→`T`).
  The `uc` runs on the line *with* its trailing `\n` (uc of `\n` is `\n`) — plan notes newline is
  preserved. ✓
- **`id2`/`qual` written verbatim** (join at 5626): plan §3.3.7 matches — only `id` is re-terminated and
  only `seq` transformed. ✓
- **`last unless (all four lines)`** (5582): plan §3.3 / edge case match — a truncated tail (<4 lines) is
  dropped. ✓
- **FastQ sanity only at `count == 1`** (5612–5616): the *condition* matches.
- **`$maximum_length_cutoff` guard** (5598–5604): I independently confirmed the plan's claim. The scalar is
  **only ever `defined` under `--minimap2`** (set to the 10000 default *inside* `if ($mm2)` at 8354, and
  `die`s at 8333–8334 if `--mm2_maximum_length` is passed without `--mm2`). On the Bowtie2 v1 spine it is
  always `undef`, so the `if (defined …)` at 5598 never fires. Plan §8 is **correct**; the guard is inert.
  ✓ (But see Important-4 — the guard is currently *omitted* from the §3.3 loop steps; it should still be
  present-but-dead for faithfulness, or its absence explicitly justified.)

**Two output-neutral ordering divergences** (the plan's loop order differs from Perl's):

1. Plan §3.3 does **count++ (step 1) then ID-fix (step 2)**; Perl does **ID-fix (5584–86) then `++$count`
   (5588)**. Output-neutral (ID fixing doesn't depend on count; count is only read by skip/upto, which
   follow both). But §11's self-review claims "ordering matches Perl … (count++ before skip/upto)" — that
   phrasing is *imprecise* about where the ID fix sits relative to count++. Cosmetic; fix the wording so an
   implementer doesn't "lock in" the wrong order as gospel.
2. Plan §3.3 lists **sanity (step 5) before tab-detect (step 6)**; Perl does **tab-detect (5607) before
   sanity (5612)**. Output-neutral (tab only sets a non-byte-affecting flag; sanity may `die` — and a die
   aborts regardless of the flag). Optional to reorder, but note it so the swap is a conscious decision.

---

## 2. Assumptions

- **"Original read intentionally NOT stored, re-read in lockstep later"** (§1, §7): **correct per Perl.**
  `biTransformFastQFiles` returns only the converted filename(s); the unconverted read is re-opened in the
  methylation-call loop. Clean hand-off — Phase 2's deliverable is just the temp path. ✓
- **`ReadProcessing` is additive to `RunConfig`** (§8, §10): plausible and non-disruptive — Phase-1 tests
  key off existing fields, and adding a sub-struct won't break them. **But watch for a two-sources-of-truth
  hazard:** `prefix` and `gzip` **already live in `OutputTarget`** (`config.rs` 99/104). If `ReadProcessing`
  *also* carries `prefix`/`gzip`, the converter and the output-naming path can diverge. Recommend
  `ReadProcessing` carry **only the net-new fields** (`skip`, `upto`, `icpc`, `maximum_length_cutoff`) and
  read `prefix`/`gzip` from the existing `OutputTarget`, or vice-versa — but pick one owner. The plan does
  not address this; specify it.
- **gzip input via flate2 `MultiGzDecoder`** (§2/§3.2): sound and multi-member-safe; matches `gunzip -c`'s
  decompressed bytes, mirroring the sibling `bismark-genome-preparation`. The multi-member trap is only on
  the *input* side and MultiGzDecoder handles it. ✓
- **`--gzip` temp output gated on decompressed content only** (§3 edge case, §8): correct given the epic
  gate definition (only decompressed content is gated; flate2 `GzEncoder` bytes need not equal `gzip -c`).
  ✓
- **`upto`/`skip` falsy-zero semantics** (§3.3.3, §8): correct. Perl `if ($skip)` / `if ($upto)` treat `0`
  as false → both `0` and unset disable. The plan replicates "only apply when > 0." ✓ Note: Phase-1 stores
  these as `Option<u64>` (`cli.rs` 91/94), so `Some(0)` is reachable from the CLI and must be treated as
  "disabled" — the plan should state that `Some(0)` ≡ `None` for skip/upto (the falsy collapse), not just
  "unset disables."

---

## 3. Efficiency

Linear, buffered, small per-record allocations — appropriate. Not a hot path vs alignment. No concern. The
note in §6 about reusing buffers is good; the only caution is the per-record `Vec` allocations in the
proposed helper signatures (`fix_id -> Vec<u8>`, `convert_seq_line -> Vec<u8>`). For a temp-file pass that's
fine; if it ever becomes hot, switch to in-place mutation of a reused buffer. Optional.

---

## 4. Validation sufficiency (§9)

The table covers the right axes (fix_id both modes, seq transform, CRLF, gzip-input equivalence, skip/upto,
malformed, empty). **But there are real gaps:**

- **Validation #5 references a golden that does not exist.** I searched the repo: there is **no**
  `subset.fq.gz_C_to_T.fastq` (nor any `*_C_to_T*` or committed `subset.fq*`). The Phase-0 spike ran on
  oxy/`/var/tmp` and committed only the *scripts* (`spike_determinism.sh`), whose C→T step is a naive
  `awk 'NR%4==2{gsub(/C/,"T")}1'` — it does **not** `uc` first and does **not** apply `fix_IDs`, so it is
  **not** a valid byte-identity oracle. §9 #5 ("reuse the Phase-0 spike's `subset.fq.gz_C_to_T.fastq`")
  therefore points at a non-existent, non-hermetic, and (if interpreted as the spike's awk output)
  *incorrect* artifact. §10 (RESOLVED) says the opposite — "a tiny **synthetic golden generated by Perl
  v0.25.1** (committed)". These two are **contradictory**, and the committed golden doesn't exist yet. This
  must be reconciled: drop the spike reference and make the Perl-generated synthetic golden the §9 #5
  oracle, and add it to the implementation outline as an explicit artifact-creation step.
- **No test exercises the `--skip` + FastQ-sanity interaction** (see Important-3). A golden/integration case
  with `--skip 1` on a file whose record 1 is malformed would catch the silent-divergence trap.
- **No test for CRLF + lowercase combined** (the focus list calls it out explicitly). #3 covers lowercase,
  #4 covers CRLF, but not both at once on a real record through the full writer (where a wrong
  uppercase-of-`\r` or chomp-of-`\r\n` bug could surface). Add one combined record to the golden.
- **No `--prefix` test.** §3.1 names the `<prefix.><basename>` rule but no validation row checks it (e.g.
  `--prefix foo` → `foo.subset.fq.gz_C_to_T.fastq`). Note Perl strips trailing dots from `$prefix`
  (8237–8240) *before* this — verify the prefix used here is the already-dot-stripped one from Phase 1.
- **No `--temp_dir` path test** (see Important-2): a case asserting the returned `path`/`name` for a
  non-empty temp_dir would catch the normalization gap.

These are enough additions to catch silent-wrong-output; without them, #5 in particular cannot run at all.

---

## 5. Important / Critical findings

### Important-1 — Phase-1 `--icpc` is **mislabeled and wrongly marked "deferred"**; Phase 2 depends on it
`cli.rs` 216–218 declares:
```rust
/// HISAT2 `--ignore-quals` variant (deferred).
#[arg(long)]
pub icpc: bool,
```
under the `// ---- HISAT2 / minimap2 specific (parsed; deferred to v1.x) ----` section. This is **factually
wrong** on two counts, verified against Perl:
- `--icpc` is a **plain boolean** (`'icpc' => \$icpc`, 7375) whose *only* effect is in **`fix_IDs`**
  (6238–6239: truncate the read ID at the first space/tab — GitHub issue #236) plus a warning (8430–8431).
  It is **not** a HISAT2 option and **not** `--ignore-quals` (that's the always-on `--ignore-quals` Bowtie2
  flag, a different thing — `options.rs`/§3.8-11 of Phase 1).
- It is **not deferred** — it is *active* in exactly the read-conversion path Phase 2 implements.

The plan §3.2 correctly uses `icpc` to drive `fix_IDs`, so the *semantics* are right, but it relies on a
CLI field whose doc comment, category, and "(deferred)" framing are all wrong — and Phase 1's
`deferred_flags()` does **not** list `--icpc`, so at least it isn't double-mislabeled there. **Action:** fix
the `cli.rs` doc comment + move it out of the HISAT2 section (or note it as cross-cutting), and have the
plan explicitly flag this Phase-1 mislabel (mirrors how §8 flags the `skip`/`upto` threading deviation).
Leaving it as-is invites a future maintainer to "clean up" a deferred HISAT2 flag and break read-ID fixing.

### Important-2 — Phase-1 `temp_dir` is **not** normalized the way Perl normalizes `$temp_dir`
Perl (8211–8231): when `--temp_dir` is given, `$temp_dir` is `chdir`'d into, made **absolute via `getcwd`**,
and forced to **end in `/`** (`$temp_dir =~ s/$/\//`); when absent it is the empty string `''`. The path is
then built by **raw concatenation** `"$temp_dir$C_to_T_infile"` (5554). Phase-1 `config.rs` (376–384)
stores `temp_dir = cli.temp_dir.clone().unwrap_or_default()` — the **raw, un-canonicalized,
no-trailing-slash** user value. The plan §3.1 *assumes* "temp_dir already carries its separator / is empty
for CWD" — but that normalization **was never implemented** (Phase 1 explicitly created/`chdir`'d no temp
dir). Consequences:
- For the **default empty temp_dir** (the primary §9 #5 gate), this is harmless: both Perl and the plan
  produce a bare relative `<name>`. The converted-content gate is unaffected.
- For **`--temp_dir <dir>`**, the plan would either (a) join with the user's raw value (missing trailing
  slash / not absolute) or (b) use `Path::join` (which §3.1 rightly forbids) — diverging from Perl's
  absolute-with-trailing-slash string in both the returned `path`/`name` and any downstream `-U`/unlink/warn
  text.
**Action:** either retro-fix Phase 1 to Perl-normalize `temp_dir` (mkdir + canonicalize + trailing `/`), or
have Phase 2 own that normalization before concatenation, and add a `--temp_dir` validation row. Also pin
down the **byte-level concatenation** technique (`OsString`/`Vec<u8>` push, not `PathBuf::push`) since
§3.1's "raw concat vs Path::join" warning is correct but the mechanism isn't specified in the outline.

### Important-3 — `--skip` silently disables the FastQ-format sanity check (faithfulness trap)
In Perl the sanity check is `if ($count == 1)` at **5612**, which sits **after** the skip `next` at
**5591**. So when `--skip ≥ 1`, records 1..skip are `next`-ed and the `count == 1` branch is never reached
→ **the FastQ-format sanity check never runs under `--skip`.** The plan §3.3 *orders* skip (step 3) before
sanity (step 5), which is consistent — **but** the phrase "FastQ sanity (record 1 only)" is ambiguous: a
naive implementer could read "record 1" as "the first record we actually keep" and run the check on record
`skip+1`, which would **diverge** (Perl runs it on neither). This is a silent behavioral difference, not a
crash. **Action:** state explicitly that the sanity check fires **iff `count == 1` AND that record was not
skipped** (i.e. it is gated by the same `count` value *after* the skip `continue`, so with any `--skip ≥ 1`
it never fires), and add a validation case (`--skip 1`, malformed record 1 → **no** die; matches Perl).

### Important-4 — `flate2` dependency and gzip writer are not in the plan's build steps
The crate `Cargo.toml` (verified) currently has **no `flate2`** (deps: clap, which, thiserror, anyhow).
The plan needs `flate2::read::MultiGzDecoder` (input) and a gzip writer for the `--gzip` temp file (Perl
`| gzip -c -`), yet implementation outline §5 never adds the dependency or names the encoder. **Action:**
add `flate2 = "=1.1.9"` (the sibling pin) to §5 step 1, and specify the writer (`flate2::write::GzEncoder`,
default compression) with an explicit note that its compressed bytes need not match `gzip -c` because only
decompressed temp content is gated.

---

## 6. Alternatives & smaller notes

- **`ConvertedReads.count` is extra vs Perl.** Perl returns only the filename(s); `$count` is used solely
  for the closing `warn`. Harmless as an informational field, but note that Perl's reported count **includes
  the upto+1 record** (`++$count` fires *before* `last if count > upto`, so on hitting the cap count =
  upto+1). If `count` is ever surfaced to mimic Perl's "N sequences in total" message, replicate the
  off-by-one; otherwise document that the field is informational and not Perl-equivalent. Optional.
- **Stale "deferred" notice.** Once Phase 2 wires conversion, `--skip`/`--upto`/`--gzip`/`--prefix` become
  active and should be **removed** from `config::deferred_flags()`'s "recognised but not yet active" notice
  (`config.rs` 247–258) so the binary stops claiming they're inert. The plan's §5.5 doesn't mention this.
  Optional but improves coherence.
- **`uc` vs ASCII-uppercase on non-ASCII bytes.** Bismark has no `use locale`/`use utf8`, so Perl `uc` only
  uppercases `a`–`z` on a byte string; Rust `make_ascii_uppercase` matches. Equivalent for any realistic
  FastQ. Optional one-line note for completeness.
- **`fix_id` regex semantics.** The default `s/[ \t]+/_/g` collapses a *run* of spaces/tabs to a **single**
  `_`; `--icpc` `s/[ \t].*$//g` truncates at the **first** space/tab (the `/g` is irrelevant once `.*$`
  matches to EOL). The plan's helper §5.2 says "byte-level `[ \t]+`→`_` (or truncate)" — correct, but the
  unit test (#1) should include a **multi-space run** (`@R  1` → `@R_1`, not `@R__1`) and a leading/trailing
  space case to lock the "collapse-run" semantics. Add to validation #1.

---

## 7. Action items (prioritized)

**Critical:** none. (The converted-content transform is faithful; the default-temp_dir gate is unaffected.)

**Important:**
1. **Fix the Phase-1 `--icpc` mislabel** (`cli.rs` 216–218: not a HISAT2 `--ignore-quals` flag, not
   deferred — it drives `fix_IDs`, Perl 6238) and have the plan flag this deviation explicitly, the way §8
   flags the skip/upto threading. (Important-1)
2. **Resolve `temp_dir` normalization** — Perl makes it absolute + trailing-`/` (8211–8231); Phase 1 stores
   the raw value. Decide owner (retro-fix Phase 1 vs Phase 2), specify byte-level concatenation (not
   `PathBuf::push`/`Path::join`), and add a `--temp_dir` validation row. (Important-2)
3. **Pin the `--skip`/sanity interaction**: sanity fires iff `count == 1` *after* the skip continue → never
   under `--skip ≥ 1`. State it unambiguously and add a `--skip 1` + malformed-record-1 test. (Important-3)
4. **Add `flate2` to the build + name the gzip writer** in §5; note the decompressed-only gate for
   `--gzip`. (Important-4)
5. **Reconcile validation #5**: drop the non-existent Phase-0-spike golden reference; make the
   Perl-v0.25.1-generated synthetic golden the committed oracle and add its creation to §5. The spike's awk
   C→T is not a valid oracle (no `uc`, no `fix_IDs`). (§4)
6. **Decide `ReadProcessing` ownership of `prefix`/`gzip`** to avoid two sources of truth with the existing
   `OutputTarget` fields. (§2)

**Optional:**
7. Treat `Some(0)` for skip/upto as disabled (falsy collapse) and say so in §8.
8. Reorder §3.3 to mirror Perl exactly (ID-fix before count++; tab-detect before sanity) **or** soften §11's
   "ordering matches Perl" wording — both divergences are output-neutral.
9. Extend validation #1 with a multi-space run (`@R  1`→`@R_1`) and #3/#4 with a combined CRLF+lowercase
   record; add a `--prefix` row (with trailing-dot-stripped prefix).
10. Document `ConvertedReads.count` as informational (Perl reports `upto+1` at the cap) and remove the
    now-active flags from `deferred_flags()` once wired.
