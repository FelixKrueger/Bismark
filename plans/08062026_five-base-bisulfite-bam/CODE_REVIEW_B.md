# CODE_REVIEW_B — `--five_base_bisulfite_bam` (commit `6a3ea0d`, issue #1095)

**Reviewer B** · fresh context · 2026-08-07
**Scope as assigned:** the driver (`run_five_base_bisulfite_standalone`, `cigar_to_string`), the I/O
round trip, the CLI surface, the integration tests, and the user-facing contract (docs / CHANGELOG /
help text). The per-record algorithm in `five_base_bisulfite.rs` was read for its contract, not
audited line-by-line (Reviewer A's remit).

Settled points from `PLAN_REVIEW_A/B/36` and §10b (masking mechanism, `QUAL` units, flip-counter
placement, fixture selection, the `NM` identity, D1–D3) are **not** re-raised.

Line numbers are **current working tree**, which is `6a3ea0d` + the three error-context fixes I
applied below (they shift `run_five_base_bisulfite_standalone` by ~+15 lines from the commit).

---

## Summary

The algorithm's I/O plumbing is in better shape than I expected, and I could not break the record
round trip. What I *can* break is the promise the feature makes when it refuses. **Every failure
path leaves the output BAM on disk, fully finalised and valid** — including the two cases the
error message, the CHANGELOG, the docs and `rust/README.md` all state are refused rather than
written. And **four of the seven integration tests, including the load-bearing idempotence gate,
silently no-op in CI** because the `cargo test` job has no `samtools` and the uBAM fixture the D3
guard test needs is untracked.

Two High findings, four Medium, a handful of Low. No Critical: nothing produces a *wrong* record on
a successful run, and I verified that positively at scale.

### What checks out (verified, not assumed)

| Claim | How I checked | Result |
|---|---|---|
| `crate::io::BamReader` drops unmapped reads, so raw noodles is necessary | `io/read.rs:7-9, 268-273, 628-645` — `filter_unmapped_then_classify` drops `FLAG & 0x4`; and `BismarkRecord` classification requires `XM`/`XR`/`XG`, so the second half of the doc comment (mod.rs:628-631) is also true | ✅ justified |
| Header fidelity: `@HD` / `@SQ` / `@RG` / `@CO` / all input `@PG` | crafted a BAM with all five, dumped the **raw** BAM header text from the output with Python (not via `samtools`, which adds its own `@PG`) | ✅ all preserved verbatim; our `@PG` appended; `@CO` stays last (valid SAM — only `@HD` is order-constrained) |
| `QUAL` absent (`*`) survives the `RecordBuf` round trip | mapped record with `QUAL:*` re-encoded | ✅ out as `*` |
| Other aux tags survive, including `B` arrays and floats | `RG:Z`, `ZB:B:i,1,2,3`, `ZF:f:1.5`, `XX:i:7` | ✅ preserved, same order, same types |
| Mate fields (`RNEXT`/`PNEXT`/`TLEN`) survive | ran the converter over the real PE fixture `tests/data/dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam` (12974 records, 42 with `I`/`D`) and `cmp`'d the SAM bodies | ✅ **byte-identical**, 12974/12974 re-encoded, flip rate `0.000000`, `masked 0` |
| All four strand indices | same over `tests/data/dedup/nondir_pe_1030.bam` (20 records, all four `XG`/FLAG combinations) | ✅ **byte-identical** |
| Mutual exclusion runs before both dispatches | `mod.rs:166-186` — the check is at the top of `run()`, above both `return run_*_standalone(...)` calls; `rejects_both_standalone_bam_modes_together` covers it | ✅ correct, and the ordering hazard R14/§5.2 flagged is genuinely avoided |
| Record-class triage is exhaustive | `mod.rs:732` `0x4 \| 0x100 \| 0x800` verbatim, everything else re-encoded with all five tags required | ✅ exhaustive; verified a `FLAG 4` record *with* tags passes through with tags intact |
| Mapped record with `SEQ:*` | crafted | ✅ fails loud: `XM length 80 does not match SEQ length 0` |
| `SEQ` present, `QUAL` absent | crafted | ✅ re-encoded normally (no `QUAL` dependency anywhere — that is T1's whole point) |
| `cigar_to_string` | `mod.rs:604-623` — all nine `Kind` variants mapped explicitly, no fall-through; `H`/`=`/`X`/`P` then die in `reconstruct_ref`'s `other =>` arm | ✅ deliberately unlike `cigar_to_ops`' silent map-to-`Match` |
| Empty BAM (header only) | crafted | ✅ exit 0, header-only output, `verdict no methylation calls` |
| Non-existent input / read-only output dir | crafted | ✅ both loud and named |
| `finish()` on the success path | `mod.rs:804-806`, before both post-hoc guards | ✅ reached |

---

## Issues

### High

#### H1 — Every failure path leaves a finalised, valid, *misleading* output BAM on disk

`writer.finish()` is at `mod.rs:804-806`; the D3 "nothing was re-encoded" guard is at `812-820` and
the flip-rate guard at `829-839` — **both after the file is complete**. Record-level failures return
from inside the loop, where `BamWriter` is dropped, and `Drop` on the underlying BGZF writer writes
the EOF marker anyway (`io/write.rs:36-40` documents exactly this: "the BGZF EOF marker is written
only via `Drop`, which silently swallows I/O errors").

Three measured outcomes:

| Case | Exit | File left behind |
|---|---|---|
| uBAM (all records unmapped → D3 guard) | 1 | `…bisulfite.bam` with **all 9996 records**, i.e. a complete copy of the input, `samtools quickcheck` clean |
| Mixed-convention input (flip rate `0.380952` → flip-rate guard) | 1 | `…bisulfite.bam` with **both records**, one converted and one not — precisely the half-and-half file the message says it is refusing to write |
| Missing `XM` on record 2 of 3 | 1 | `…bisulfite.bam` with **1 of 3 records**, valid BGZF, `samtools quickcheck` clean — a **silently truncated** BAM |

The third is the worst: there is nothing in the file that says it is a prefix. The other two are the
exact failure mode D3 was added to prevent, moved from "wrong exit code" to "wrong file on disk"; a
user who reruns a pipeline step, or whose wrapper ignores exit codes (or who reads the stderr note
and then globs `*.bisulfite.bam`), gets the file the guard exists to withhold.

Also note the error message at `mod.rs:829-839` — "**refusing rather than writing** a file that is
half one convention and half the other" — is factually false as shipped, and the D3 message's "The
output *would be* a copy of the input" reads as counterfactual when the copy is already there.

This is not a one-line fix (multiple `return` sites, and there is a real choice between deleting and
renaming), so I have not applied it. Recommended shape — extract the per-input body and clean up on
any `Err`:

```rust
for bam in &cli.five_base_bisulfite_bam {
    let stem = /* … as now … */;
    let out_path = out_dir.join(format!("{stem}.bisulfite.bam"));
    let report_path = out_dir.join(format!("{stem}.bisulfite_report.txt"));
    if let Err(e) = convert_one_bam(bam, &out_path, &report_path, command_line) {
        // A partial or unconverted output is worse than none: it is a valid BAM that lies.
        let _ = std::fs::remove_file(&out_path);
        let _ = std::fs::remove_file(&report_path);
        return Err(e);
    }
}
```

`BamWriter::from_path` already sets this precedent (`io/write.rs:49-55` removes the file when the
header write fails). If deletion feels too aggressive, write to `<stem>.bisulfite.bam.partial` and
rename on success — but then say so in the message. Either way the messages, the CHANGELOG, the docs
and `rust/README.md` need to match (see M1).

#### H2 — 4 of the 7 integration tests silently no-op in CI, including the load-bearing gate

`rust_ci.yml`'s `test` job (lines 17-44) installs **minimap2** and nothing else; `samtools` is
installed only in the separate `perl-oracle` job (line 178), which runs 13 named tests via
`cargo test -- --exact` and none of these. So under CI:

| Test | Runs in CI? | Why not |
|---|---|---|
| `bisulfite_input_round_trips_to_identical_sam_text` (tests:76) | ❌ | `samtools_available()` false → early `return` (tests:77-80) |
| `xm_is_left_untouched` (tests:138) | ❌ | same |
| `appends_a_pg_with_a_distinct_id` (tests:164) | ❌ | same |
| `rejects_input_without_bismark_tags` (tests:249) | ❌ | `test_files/BS-seq_10K_se_trimmed_from_TrimGalore.bam` is **untracked** (`git ls-files` — only the four `.fa.gz`/`.fastq.gz` files are committed), so `if !ubam.exists() { return }` |
| `bisulfite_input_reports_a_zero_flip_rate` | ✅ | report-text only |
| `requires_illumina_5base`, `rejects_both_standalone_bam_modes_together` | ✅ | fail before any I/O |

So the properties the plan, the module docs and `docs/…/illumina-5-base.md` all call load-bearing —
"identical SAM text", "`XM` untouched", "distinct `@PG` ID" — and the D3 guard added *because* of a
silent-no-op are, in CI, asserted by nothing. What does run is the report-text gate, which is a good
non-vacuous check of the counters (`methylation calls\t150`, tests:129-132) but says nothing about
the emitted bytes.

The repo already has the fix pattern for exactly this hazard, twice — `aligner_minimap2_as_bound.rs:34-47`
and `aligner_five_base_groundtruth.rs:63`:

```rust
fn samtools_available() -> bool {
    let present = /* … as now … */;
    if !present && std::env::var_os("CI").is_some() {
        panic!(
            "samtools not found but $CI is set: the #1095 idempotence gate requires it \
             (install it in the workflow) — refusing to no-op."
        );
    }
    present
}
```

plus `sudo apt-get install -y --no-install-recommends samtools` in the `test` job. I did not apply
this: part one alone turns CI red, and editing a workflow is the author's call.

**Better still, drop the dependency.** The integration test links the `bismark` lib, so the
idempotence gate can read both BAMs with noodles and compare `RecordBuf` streams — no `samtools`, no
skip, and it can then run over the two much larger committed fixtures I used above
(`dedup/synth_barcode_10k_R1_val_1_bismark_bt2_pe.bam`, 12974 records incl. indels, and
`dedup/nondir_pe_1030.bam`, all four `XG`/FLAG combinations). Both are already byte-identical
today — that is free coverage of PE mate fields and all four strand indices, which the committed
8-record SE fixture cannot give. Likewise, `rejects_input_without_bismark_tags` should synthesise a
2-record unmapped-only BAM in the test rather than depend on an untracked file.

### Medium

#### M1 — The user-facing contract overclaims in four places (all downstream of H1/H2)

| Where | Claim | Reality |
|---|---|---|
| `docs/…/illumina-5-base.md` (Interop, last para) | "the run fails **rather than writing** a file that is half one convention and half the other" | the file is written and finalised (H1) |
| `CHANGELOG.md` (#1095 entry) | "anything in between fails the run **rather than writing a half-converted file**" | same |
| `rust/README.md` (2026-08-07 line) | "an input where every record is passed through fails too, **rather than handing back a copy** the user believes is converted" | the copy is on disk (H1) |
| `docs/…/illumina-5-base.md` | "that identity **is a CI gate**" | the gate skips in CI (H2) |
| `mod.rs:829-839` error text | "refusing rather than writing …" | same as row 1 |

Fixing H1 and H2 makes all five true; otherwise the text has to be softened. I flag it separately
because these are the sentences a user will rely on when deciding whether a failed run left them
anything dangerous.

Everything else I checked in the contract is accurate: "no genome" (verified — no `--genome` in any
of my runs), "no re-alignment", "`XM` left untouched" (verified), the `SO:coordinate`/`bam2pat`
requirement, the chromosome-naming warning, and the `caution` block about variant calling. The CLI
help (`cli.rs:184-193`) is proportionate and does warn in the right register ("NEVER use this as the
primary BAM… must not feed a variant caller"). Two small gaps: it does not mention the mutual
exclusion with `--five_base_consensus_from_bam` (the CHANGELOG does), and nothing anywhere says the
run also writes `<stem>.bisulfite_report.txt`.

#### M2 — Two inputs with the same stem silently clobber each other

`out_path` is `<output_dir>/<file_stem>.bisulfite.bam` (`mod.rs:708-709`) and the flag is a `Vec`.
Measured with `a/x.bam` (8 records) and `b/x.bam` (0 records) in one invocation: **exit 0**, one
`x.bisulfite.bam` containing 0 records, one `x.bisulfite_report.txt` describing only `b/x.bam`. The
8-record conversion is gone and nothing says so. Per-lane BAMs named `aligned.bam` in per-sample
directories is exactly how people organise this, and it is the same "the user believes both were
converted" failure D3 exists to prevent.

Cheapest fix is a pre-flight before any work:

```rust
let mut seen = std::collections::HashMap::new();
for bam in &cli.five_base_bisulfite_bam {
    let out = out_dir.join(format!("{}.bisulfite.bam", stem_of(bam)));
    if let Some(prev) = seen.insert(out.clone(), bam.clone()) {
        return Err(AlignerError::Validation(format!(
            "bisulfite: {} and {} would both write {} — rename one, or convert them into \
             separate --output_dir folders.",
            prev.display(), bam.display(), out.display(),
        )));
    }
}
```

Alternative worth considering: honour `--prefix` here. `derive_output_path` (`mod.rs:2823-2841`) is
the aligner's naming helper and applies `--basename`/`--prefix`; this path ignores both. It takes a
`RunConfig` that the standalone path never builds, so reuse is not free — and the consensus sibling
hardcodes `five_base_consensus.bam`, so the inconsistency is pre-existing — but `--prefix` support
would give the user a way out that a hard error does not.

#### M3 — Supplementary alignments are passed through still inverted, and the warning does not say so

`mod.rs:732-745` copies `0x100`/`0x800` verbatim with one aggregated warning saying they "are copied
through unchanged (Bismark does not emit them)". True for Bismark's own output
(`five_base_next_primary` filters them), but the input space here is third-party BAMs, and minimap2 —
the **default** 5-Base engine — emits supplementary alignments; `--secondary=no` does not suppress
them. The plan's own statement of `bam2pat`'s filter (§9.9: `-F 1796`) excludes secondary (`0x100`)
but **not** supplementary (`0x800`), so a passed-through supplementary record is read by `patter`
with 5-Base polarity — inverted calls inside the file whose entire purpose is to not have any.

Re-encoding them is not really available (their CIGARs are hard-clipped, and `H` is correctly
rejected), so the choice is between failing loud and stating the consequence. Minimum: extend the
warning to say those records keep 5-Base polarity and that `bam2pat`'s default filter does not
exclude `0x800`. My preference is to fail loud on `0x800` unless a future opt-out flag is added —
consistent with everything else in this feature, and the class is absent from Bismark's own output
so nothing legitimate regresses.

#### M4 — The `@PG` comment (D2) describes behaviour I cannot reproduce, and the chain forks

`mod.rs:680-686` states, as observed fact, that "noodles owns the `@PG` chain and re-links it on
serialisation, so a later program's `PP` ends up pointing at this node regardless of what we ask for
— e.g. an input's `@PG ID:samtools PP:Bismark` comes out as `PP:bismark-five-base-bisulfite`".

I built precisely that input (`@PG ID:Bismark` → `ID:samtools PP:Bismark` → `ID:samtools.1 PP:samtools`)
and read the **raw** BAM header text out of the output:

```
@PG ID:Bismark                        VN:v0.25.1 …
@PG ID:samtools    PN:samtools PP:Bismark   VN:1.21 …     <- unchanged
@PG ID:samtools.1  PN:samtools PP:samtools  VN:1.21 …     <- unchanged
@PG ID:bismark-five-base-bisulfite PP:Bismark VN:v0.25.1 …
```

No re-link. What actually happens is a consequence of `mod.rs:688`
(`in_header.programs().as_ref().keys().next()`): we chain onto the **first** program, so `Bismark`
now has two children and the chain is a fork rather than a line — a consumer walking `PP` to find
the terminal program sees two leaves. The conventional choice (and samtools' own) is to chain onto
the **last/leaf** program, which for a single-`@PG` Bismark BAM is still `PP:Bismark` and so keeps
T6 satisfied.

Two things to fix, in this order: correct the comment to what the code does (the plan's own
iteration log #4 makes the point that a comment asserting something untrue is worse than none), and
consider `.keys().last()`. Metadata-only impact, hence Medium not High — but a wrong comment in a
file this heavily commented will be trusted.

### Low

- **L1 — the report is stamped `v0.25.1`.** `mod.rs:859` uses `BISMARK_VERSION`, so
  `<stem>.bisulfite_report.txt` opens with "Bismark 5-Base -> bisulfite-convention re-encode
  (v0.25.1)" for a feature that does not exist in v0.25.1. Correct and necessary in the `@PG VN`
  (byte-identity), but a brand-new report has no such constraint and this is the string a bug
  reporter will paste. Consider the crate version, or print both.
- **L2 — the printed `samtools sort` command yields `x.bisulfite.bam.sorted.bam`.** `mod.rs:876-885`
  interpolates the full output path into `{0}.sorted.bam`. Functional, copy-pasteable, but ugly —
  and the docs example uses the nicer `sample_pe.bisulfite.sorted.bam` form, so the two disagree.
- **L3 — `reencode` errors do not name the input BAM.** Tag errors go through `ctx` (`mod.rs:~760`)
  and name the file; algorithm errors are wrapped as `format!("bisulfite: {e}")` (`mod.rs:~795`) and
  name only the QNAME. With a repeatable flag, the user cannot tell which BAM failed. One-line fix,
  but it is in the same code I would restructure for H1, so I left it.
- **L4 — "still readable by Bismark's own extractor" is untested.** It is very likely true (`XM`,
  `POS`, `CIGAR` untouched), and there is a cheap hermetic gate: run the extractor over the fixture
  and over its conversion and diff. That would also pin the "`bam2pat --ds_test` keeps working"
  claim, which rests on the same invariant.
- **L5 — empty-BAM handling has no explicit note.** Plan §3.10 promised "header-only output + a
  `Note:`"; what you get is `verdict no methylation calls`, which is the same string a BAM full of
  call-free records produces. One `eprintln!` when `n_read == 0` would separate them.
- **L6 — verdict text is wrong when re-run on the converter's own output.** Idempotence holds (I
  reasoned it through: masked `N`s are no longer in `{meth, unmeth}`, letters are no longer `.`,
  and `MD`/`NM` were recomputed to match), but the verdict then reads "already bisulfite convention
  (input was **NOT 5-Base**)", which is misleading. Detecting our own `@PG ID:bismark-five-base-bisulfite`
  in the input header would let the message say "already converted by this tool".
- **L7 — micro-allocations in the per-record path.** `cigar_to_string` (`mod.rs:611-620`) allocates a
  `String` per CIGAR *op* via `op.len().to_string()`; use `write!(s, "{}{c}", op.len())`. The
  happy-path `qname` (`mod.rs:~753`) is allocated per record purely for error messages. Both are
  noise against BGZF (plan §6 is right about that) but they are in a loop that will see 10⁸ records.
- **L8 — single-threaded BGZF.** This path rewrites an entire whole-genome BAM and ignores
  `--parallel`. `ThreadedBamWriter` (`io/write.rs:126-…`) already exists and is used by dedup, so
  wiring it is cheap. Plan §6 anticipates this ("if it ever needs to be faster the win is parallel
  BGZF"); flagging it because a 55.7M-read sample will spend real minutes here.
- **L9 — `out.nm as i32`** (`mod.rs:~797`) truncates silently. Unreachable in practice (`NM` is
  bounded by read length plus deletions); a `i32::try_from(...).map_err(...)` would make it
  provably so.
- **L10 — §9.8's `XG` ⟺ FLAG assertion is still absent.** Already recorded as deliberate in §10b
  ("Not done — worth adding"), so this is only a reminder: my `nondir_pe_1030.bam` round trip above
  gives you the fixture plumbing for free, and the equivalence is invisible to every gate you have.

---

## Fixes applied

Three error-context fixes, all on failure paths only — no behaviour change on the success path.
`rust/bismark/src/aligner/mod.rs`, 18 insertions / 3 deletions:

1. **`reader.read_header()` now names the file and the likely cause.** Before, a SAM (or CRAM, or
   gzipped SAM) input — a very likely user mistake, and the sibling `--five_base_consensus_from_bam`
   accepts only BAM too — failed inside BGZF with the bare, pathless
   `error: I/O error: failed to fill whole buffer`. Now:
   ```
   error: bisulfite: /path/plain.sam is not a readable BAM: failed to fill whole buffer.
   SAM/CRAM input is not supported here — convert it with `samtools view -b` first.
   ```
2. **`create_dir_all(&out_dir)`** was a bare `?`, producing `error: I/O error: Permission denied
   (os error 13)` with no indication that `--output_dir` was the problem. Now named.
3. **`fs::write(&report_path, …)`** was a bare `?`; now named, matching the `map_err` style of every
   other I/O call in the function.

Gates re-run after the edits:

- `cargo fmt -p bismark -- --check` — clean
- `cargo test -p bismark --test aligner_five_base_bisulfite` — **7 passed, 0 failed**
- `cargo clippy -p bismark --all-targets` — no new warnings
- re-verified the improved SAM-input message against a real `.sam`

I deliberately did **not** touch:

- **the vacuous assertion at `tests:267-273`.** `assert!(!file.exists() || err.contains("none could
  be re-encoded"))` — the right-hand disjunct was already asserted true four lines above
  (`tests:263-266`), so the whole assertion is trivially true, and its message ("must not leave a
  file the user would mistake for a conversion") asserts the opposite of what happens (H1: the file
  is a complete 9996-record copy). Tightening it to `assert!(!file.exists())` would turn it into a
  correct, *failing* test — which is a decision about H1, not a review fix. Fix them together.
- **H1's cleanup, H2's CI wiring, and M2's pre-flight** — see each finding for why.

---

## Recommendations, in order

| # | Priority | Action |
|---|---|---|
| 1 | **High** | H1 — delete (or `.partial`-then-rename) the output on every failure path; then tighten `tests:267-273` to `assert!(!produced.exists())` |
| 2 | **High** | H2 — `panic!` in `samtools_available()` when `$CI` is set **and** add samtools to `rust_ci.yml`'s `test` job; better, re-implement the idempotence gate over noodles and extend it to the two large committed fixtures; synthesise the unmapped-only BAM instead of relying on an untracked file |
| 3 | Medium | M1 — reconcile the four "does not write the file" / "is a CI gate" claims with reality (free once 1 and 2 land) |
| 4 | Medium | M2 — fail loud on colliding output stems, or honour `--prefix` |
| 5 | Medium | M3 — fail loud on `0x800`, or state in the warning that those records keep 5-Base polarity and that `-F 1796` does not filter them |
| 6 | Medium | M4 — correct the `@PG` comment to observed behaviour; chain onto the last program, not the first |
| 7 | Low | L1-L10 as convenient; L4 (extractor round-trip gate) is the best value-per-line of the set |
